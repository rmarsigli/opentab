use opentab_model::{
    BeatId, BeatPath, ElementId, MasterBarId, NoteId, NotePath, TrackId, VoiceId, VoicePath,
};

use crate::support::{four_four, guitar, quarters};

#[test]
fn finds_every_kind_of_element() {
    let mut score = four_four(&quarters(4));
    let second = guitar(&mut score, vec![quarters(2), quarters(4)]);
    score.tracks.push(second.clone());
    let voice = &second.staves[0].bars[1].voices[0];
    let beat = &voice.beats[3];
    let note = &beat.notes[0];
    assert_eq!(score.find_track(second.id), Some(&second));
    assert_eq!(
        score.find_master_bar_index(score.master_bars[0].id),
        Some(0)
    );
    let voice_path = VoicePath {
        track: 1,
        staff: 0,
        bar: 1,
        voice: 0,
    };
    assert_eq!(score.find_voice(voice.id), Some(voice_path));
    let beat_path = BeatPath {
        track: 1,
        staff: 0,
        bar: 1,
        voice: 0,
        beat: 3,
    };
    assert_eq!(score.find_beat(beat.id), Some(beat_path));
    let note_path = NotePath {
        beat: beat_path,
        note: 0,
    };
    assert_eq!(score.find_note(note.id), Some(note_path));
}

#[test]
fn unknown_and_wrong_kind_ids_are_none() {
    let score = four_four(&quarters(4));
    let missing = ElementId(9999);
    assert_eq!(score.find_track(TrackId(missing)), None);
    assert_eq!(score.find_master_bar_index(MasterBarId(missing)), None);
    assert_eq!(score.find_voice(VoiceId(missing)), None);
    assert_eq!(score.find_beat(BeatId(missing)), None);
    assert_eq!(score.find_note(NoteId(missing)), None);
    let note = score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].id;
    assert_eq!(score.find_beat(BeatId(note.0)), None);
    assert_eq!(score.find_track(TrackId(note.0)), None);
}

#[test]
fn empty_score_lookups_are_none() {
    let score = opentab_model::Score::new();
    assert_eq!(score.find_track(TrackId(ElementId(1))), None);
    assert_eq!(score.find_beat(BeatId(ElementId(1))), None);
}
