use opentab_command::{Command, CommandError, apply};
use opentab_model::{
    Clef, Dynamic, NoteValue, Score, TempoUnit, TimeSignature, VoiceSlot, diagnose,
};

use crate::checks::{rejected, round_trip};
use crate::support::fixture;

fn quarter_time() -> TimeSignature {
    TimeSignature::new(4, 4).unwrap()
}

fn clefs(score: &Score, track: usize) -> Vec<Clef> {
    score.tracks[track].staves[0]
        .bars
        .iter()
        .map(|bar| bar.clef)
        .collect()
}

#[test]
fn inserted_master_bars_inherit_clefs_and_start_with_slot_zero() {
    let mut fx = fixture();
    fx.score.tracks[0].staves[0].bars[0].clef = Clef::Bass;
    fx.score.tracks[0].staves[0].bars[1].clef = Clef::Treble;
    fx.score.tracks[1].staves[0].bars[0].clef = Clef::Neutral;
    fx.score.tracks[1].staves[0].bars[1].clef = Clef::Neutral;
    let insert = |index| Command::InsertMasterBar {
        index,
        time_signature: quarter_time(),
    };
    round_trip(&mut fx.score, &insert(1));
    apply(&mut fx.score, &insert(1)).unwrap();
    assert_eq!(clefs(&fx.score, 0), [Clef::Bass, Clef::Bass, Clef::Treble]);
    assert_eq!(clefs(&fx.score, 1), [Clef::Neutral; 3]);
    apply(&mut fx.score, &insert(0)).unwrap();
    assert_eq!(clefs(&fx.score, 0)[0], Clef::Bass);
    let created = &fx.score.master_bars[0];
    assert!(!created.double_bar);
    let bar = &fx.score.tracks[0].staves[0].bars[0];
    assert_eq!(bar.voices.len(), 1);
    assert_eq!(bar.voices[0].slot, VoiceSlot::default());
}

#[test]
fn inserting_into_a_score_without_bars_picks_a_clef_by_instrument() {
    let mut fx = fixture();
    fx.score.master_bars.clear();
    for track in &mut fx.score.tracks {
        track.staves[0].bars.clear();
    }
    apply(
        &mut fx.score,
        &Command::InsertMasterBar {
            index: 0,
            time_signature: quarter_time(),
        },
    )
    .unwrap();
    assert_eq!(clefs(&fx.score, 0), [Clef::Treble]);
    assert_eq!(clefs(&fx.score, 1), [Clef::Neutral]);
}

#[test]
fn padded_bars_continue_the_clef_of_the_staff() {
    let mut fx = fixture();
    fx.score.tracks[0].staves[0].bars[1].clef = Clef::Bass;
    let staff = fx.score.tracks[0].staves[0].id;
    let track = fx.guitar;
    let mut score = fx.score.clone();
    score.tracks[0].staves[0].bars.clear();
    apply(&mut score, &Command::PadStaffBars { track, staff }).unwrap();
    assert_eq!(clefs(&score, 0), [Clef::Treble, Clef::Treble]);
    let mut kept = fx.score.clone();
    kept.tracks[0].staves[0].bars.truncate(1);
    kept.tracks[0].staves[0].bars[0].clef = Clef::Bass;
    apply(&mut kept, &Command::PadStaffBars { track, staff }).unwrap();
    assert_eq!(clefs(&kept, 0), [Clef::Bass, Clef::Bass]);
}

#[test]
fn new_beats_have_the_default_dynamic_and_no_grace() {
    let mut fx = fixture();
    let voice = fx.voice;
    apply(
        &mut fx.score,
        &Command::InsertBeat {
            voice,
            index: 0,
            duration: opentab_model::Duration::new(NoteValue::Eighth, 0, None).unwrap(),
        },
    )
    .unwrap();
    let beat = &fx.score.tracks[0].staves[0].bars[0].voices[0].beats[0];
    assert_eq!(beat.effects.dynamic, Dynamic::Mf);
    assert_eq!(beat.effects.grace, None);
}

#[test]
fn set_tempo_creates_quarter_changes_and_keeps_the_unit_of_existing_ones() {
    let mut fx = fixture();
    apply(
        &mut fx.score,
        &Command::SetTempo {
            master_bar: 0,
            bpm: 80.0,
        },
    )
    .unwrap();
    assert_eq!(
        fx.score.master_bars[0].tempo_changes[0].unit,
        TempoUnit::Quarter
    );
    fx.score.master_bars[0].tempo_changes[0].unit = TempoUnit::DottedHalf;
    round_trip(
        &mut fx.score,
        &Command::SetTempo {
            master_bar: 0,
            bpm: 90.0,
        },
    );
    apply(
        &mut fx.score,
        &Command::SetTempo {
            master_bar: 0,
            bpm: 90.0,
        },
    )
    .unwrap();
    assert_eq!(
        fx.score.master_bars[0].tempo_changes[0].unit,
        TempoUnit::DottedHalf
    );
}

fn swap_slots(score: &mut Score, track: usize, bar: usize, first: u8, second: u8) {
    let target = &mut score.tracks[track].staves[0].bars[bar];
    let mut extra = target.voices[0].clone();
    extra.id = score.alloc().unwrap();
    extra.beats.clear();
    extra.slot = VoiceSlot::new(second).unwrap();
    let target = &mut score.tracks[track].staves[0].bars[bar];
    target.voices[0].slot = VoiceSlot::new(first).unwrap();
    target.voices.push(extra);
}

#[test]
fn restore_master_bar_rejects_voice_slots_out_of_order() {
    for (first, second) in [(1, 1), (2, 0), (3, 2)] {
        let mut fx = fixture();
        let Command::RestoreMasterBar {
            index,
            master_bar,
            mut bars,
            extensions,
        } = apply(&mut fx.score, &Command::RemoveMasterBar { index: 1 }).unwrap()
        else {
            panic!("restore_master_bar expected");
        };
        let mut staging = fx.score.clone();
        staging.tracks[0].staves[0].bars.push(bars.remove(0));
        swap_slots(&mut staging, 0, 1, first, second);
        bars.insert(0, staging.tracks[0].staves[0].bars.remove(1));
        fx.score.ids = staging.ids;
        let restore = Command::RestoreMasterBar {
            index,
            master_bar,
            bars,
            extensions,
        };
        let CommandError::InvalidValue { message } = rejected(&mut fx.score, &restore) else {
            panic!("invalid value expected");
        };
        assert!(message.contains("strictly increasing"), "{message}");
    }
}

#[test]
fn restore_master_bar_accepts_gapped_increasing_slots() {
    let mut fx = fixture();
    swap_slots(&mut fx.score, 0, 1, 1, 3);
    let removal = Command::RemoveMasterBar { index: 1 };
    let inverse = apply(&mut fx.score, &removal).unwrap();
    apply(&mut fx.score, &inverse).unwrap();
    let slots: Vec<u8> = fx.score.tracks[0].staves[0].bars[1]
        .voices
        .iter()
        .map(|voice| voice.slot.get())
        .collect();
    assert_eq!(slots, [1, 3]);
}

#[test]
fn removing_a_bar_whose_slots_are_already_out_of_order_is_refused() {
    let mut fx = fixture();
    swap_slots(&mut fx.score, 0, 1, 2, 2);
    assert!(!diagnose(&fx.score).is_empty());
    let CommandError::InvalidValue { message } =
        rejected(&mut fx.score, &Command::RemoveMasterBar { index: 1 })
    else {
        panic!("invalid value expected");
    };
    assert!(message.contains("strictly increasing"), "{message}");
}
