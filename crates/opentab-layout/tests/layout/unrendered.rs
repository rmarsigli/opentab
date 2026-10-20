use opentab_layout::{BarRange, LayoutOptions, StaffMode, Unrendered};
use opentab_model::{
    DirectionJump, DirectionTarget, Directions, ElementId, InstrumentKind, NoteKind, OpaquePayload,
    Score, TripletFeel,
};

use crate::music::{build, measure, options, render};
use crate::support::add_track;
use crate::tab_effects::quarters;

fn score(bars: usize) -> Score {
    let measures: Vec<_> = (0..bars)
        .map(|_| measure((4, 4), 0, vec![(0, quarters(&[(2, 5); 4]))]))
        .collect();
    build(&measures)
}

fn keep(score: &mut Score, id: ElementId, data: &str) {
    score
        .extensions
        .0
        .entry(id)
        .or_default()
        .push(OpaquePayload {
            format: "gpif".to_string(),
            data: data.to_string(),
        });
}

fn found(score: &Score, options: &LayoutOptions) -> Vec<Unrendered> {
    render(score, options).unrendered
}

fn beat_id(score: &Score, bar: usize, beat: usize) -> ElementId {
    score.tracks[0].staves[0].bars[bar].voices[0].beats[beat]
        .id
        .0
}

fn fermata(score: &mut Score) {
    let id = score.master_bars[1].id.0;
    keep(
        score,
        id,
        "<Keep parent=\"\" index=\"2\"><Fermatas><Fermata><Type>Long</Type></Fermata></Fermatas></Keep>",
    );
}

fn hairpin(score: &mut Score) {
    let id = beat_id(score, 1, 2);
    keep(
        score,
        id,
        "<Keep parent=\"\" index=\"4\"><Hairpin>Crescendo</Hairpin></Keep>",
    );
}

fn legato(score: &mut Score) {
    let id = beat_id(score, 1, 0);
    keep(
        score,
        id,
        "<Keep parent=\"\" index=\"4\"><Legato origin=\"true\" destination=\"false\"/></Keep>",
    );
}

fn track_lyrics(score: &mut Score) {
    let id = score.tracks[0].id.0;
    keep(
        score,
        id,
        "<Keep parent=\"\" index=\"18\"><Lyrics dispatched=\"true\"><Line><Text><![CDATA[la la]]></Text><Offset>0</Offset></Line></Lyrics></Keep>",
    );
}

fn beat_lyrics(score: &mut Score) {
    let id = beat_id(score, 1, 1);
    keep(
        score,
        id,
        "<Keep parent=\"\" index=\"6\"><Lyrics><Line><![CDATA[Grace]]></Line></Lyrics></Keep>",
    );
}

fn diagram(score: &mut Score) {
    let staff = score.tracks[0].staves[0].id.0;
    keep(
        score,
        staff,
        "<Keep parent=\"Properties\" index=\"7\"><Property name=\"DiagramCollection\"><Items><Item id=\"0\" name=\"E\"><Diagram stringCount=\"6\"/></Item><Item id=\"1\" name=\"N.C.\"/></Items></Property></Keep>",
    );
    score.tracks[0].staves[0].bars[1].voices[0].beats[0]
        .effects
        .chord_name = Some("E".to_string());
    let id = beat_id(score, 1, 0);
    keep(
        score,
        id,
        "<Keep parent=\"\" index=\"4\" guard=\"chord:=E\"><Chord><![CDATA[0]]></Chord></Keep>",
    );
}

fn rests(score: &mut Score) {
    for bar in [2, 3] {
        for beat in &mut score.tracks[0].staves[0].bars[bar].voices[0].beats {
            beat.notes.clear();
        }
    }
}

fn drums(score: &mut Score) {
    let track = add_track(score, "Drums", 0, InstrumentKind::Percussion);
    let mut notes = score.tracks[0].staves[0].bars[1].clone();
    for voice in &mut notes.voices {
        voice.id = score.alloc().unwrap();
        for beat in &mut voice.beats {
            beat.id = score.alloc().unwrap();
            for note in &mut beat.notes {
                note.id = score.alloc().unwrap();
                note.kind = NoteKind::Percussion { articulation: 81 };
            }
        }
    }
    notes.id = score.tracks[track].staves[0].bars[1].id;
    score.tracks[track].staves[0].bars[1] = notes;
}

type Feature = (fn(&mut Score), Unrendered);

const FEATURES: [Feature; 8] = [
    (fermata, Unrendered::Fermatas),
    (hairpin, Unrendered::Hairpins),
    (legato, Unrendered::LegatoSlurs),
    (track_lyrics, Unrendered::Lyrics),
    (beat_lyrics, Unrendered::Lyrics),
    (diagram, Unrendered::ChordDiagrams),
    (rests, Unrendered::MultiBarRests),
    (drums, Unrendered::PercussionArticulations),
];

#[test]
fn each_unrendered_feature_is_detected_on_its_own() {
    let both = options(StaffMode::Both);
    assert!(found(&score(4), &both).is_empty());
    for (add, feature) in FEATURES {
        let mut one = score(4);
        add(&mut one);
        assert_eq!(found(&one, &both), vec![feature], "{feature:?}");
    }
}

#[test]
fn every_feature_together_is_listed_once_and_sorted_by_name() {
    let mut all = score(4);
    for (add, _) in FEATURES {
        add(&mut all);
    }
    let list = found(&all, &options(StaffMode::Both));
    let names: Vec<&str> = list.iter().map(|feature| feature.name()).collect();
    assert_eq!(
        names,
        vec![
            "chord_diagrams",
            "fermatas",
            "hairpins",
            "legato_slurs",
            "lyrics",
            "multi_bar_rests",
            "percussion_articulations"
        ]
    );
    let first_bar = LayoutOptions {
        bars: Some(BarRange { first: 0, last: 0 }),
        ..LayoutOptions::default()
    };
    assert_eq!(found(&all, &first_bar), vec![Unrendered::Lyrics]);
    let guitar_only = LayoutOptions {
        tracks: Some(vec![0]),
        bars: Some(BarRange { first: 2, last: 3 }),
        ..LayoutOptions::default()
    };
    assert_eq!(found(&all, &guitar_only), vec![Unrendered::MultiBarRests]);
    let drums_only = LayoutOptions {
        tracks: Some(vec![1]),
        bars: Some(BarRange { first: 1, last: 1 }),
        ..LayoutOptions::default()
    };
    assert_eq!(
        found(&all, &drums_only),
        vec![Unrendered::Fermatas, Unrendered::PercussionArticulations]
    );
}

#[test]
fn look_alikes_of_each_feature_are_not_reported() {
    let both = options(StaffMode::Both);
    let mut quiet = score(4);
    let track = quiet.tracks[0].id.0;
    keep(
        &mut quiet,
        track,
        "<Keep parent=\"\" index=\"18\"><Lyrics dispatched=\"true\"><Line><Text><![CDATA[ ]]></Text><Offset>12</Offset></Line></Lyrics></Keep>",
    );
    let legato = beat_id(&quiet, 1, 0);
    keep(
        &mut quiet,
        legato,
        "<Keep parent=\"\" index=\"4\"><Legato origin=\"false\" destination=\"false\"/></Keep>",
    );
    let staff = quiet.tracks[0].staves[0].id.0;
    keep(
        &mut quiet,
        staff,
        "<Keep parent=\"Properties\" index=\"7\"><Property name=\"DiagramCollection\"><Items><Item id=\"0\" name=\"E\"/></Items></Property></Keep>",
    );
    quiet.tracks[0].staves[0].bars[1].voices[0].beats[0]
        .effects
        .chord_name = Some("E".to_string());
    let beat = beat_id(&quiet, 1, 0);
    keep(
        &mut quiet,
        beat,
        "<Keep parent=\"\" index=\"4\"><Chord><![CDATA[0]]></Chord></Keep>",
    );
    rests(&mut quiet);
    quiet.master_bars[3].repeat_start = true;
    let mut empty = score(4);
    for bar in [1, 2] {
        empty.tracks[0].staves[0].bars[bar].voices.clear();
    }
    assert!(found(&empty, &both).is_empty());
    let mut known = score(4);
    drums(&mut known);
    let drums = known.tracks.len() - 1;
    for beat in &mut known.tracks[drums].staves[0].bars[1].voices[0].beats {
        for note in &mut beat.notes {
            note.kind = NoteKind::Percussion { articulation: 38 };
        }
    }
    known.tracks[drums].drum_kit = Some(crate::percussion::general_kit());
    assert!(found(&quiet, &both).is_empty());
    assert!(found(&known, &both).is_empty());
}

fn names(score: &Score, options: &LayoutOptions) -> Vec<&'static str> {
    found(score, options)
        .into_iter()
        .map(Unrendered::name)
        .collect()
}

#[test]
fn effects_hidden_by_the_staff_mode_are_named() {
    let score = crate::snapshots::fixture("note-effects.gp");
    assert_eq!(
        names(&score, &options(StaffMode::Both)),
        vec!["right_hand_fingering"]
    );
    assert_eq!(
        names(&score, &options(StaffMode::Standard)),
        vec![
            "bends",
            "hammer_on_pull_off",
            "harmonic_types",
            "let_ring",
            "palm_mute",
            "right_hand_fingering",
            "slides",
            "tapping",
            "vibrato"
        ]
    );
    assert_eq!(
        names(&score, &options(StaffMode::Tab)),
        vec!["accents", "right_hand_fingering", "staccato"]
    );
    let strums = crate::snapshots::fixture("beat-effects.gp");
    assert!(names(&strums, &options(StaffMode::Standard)).contains(&"strokes"));
    assert!(!names(&strums, &options(StaffMode::Both)).contains(&"strokes"));
}

#[test]
fn model_fields_that_are_never_drawn_are_always_named() {
    let both = options(StaffMode::Both);
    let mut score = score(2);
    {
        let beats = &mut score.tracks[0].staves[0].bars[0].voices[0].beats;
        beats[0].effects.fade_in = true;
        beats[1].effects.slap = true;
        beats[2].effects.pop = true;
        beats[3].notes[0].effects.right_hand_finger = Some(opentab_model::Finger::Index);
    }
    score.meta.copyright = "(c) someone".to_string();
    score.meta.instructions = "play softly".to_string();
    score.meta.notices = "note".to_string();
    score.meta.title = "Song \u{4E2D}".to_string();
    assert_eq!(
        names(&score, &both),
        vec![
            "copyright",
            "fade_in",
            "instructions",
            "missing_characters",
            "notices",
            "pop",
            "right_hand_fingering",
            "slap"
        ]
    );
    score.meta.copyright = " \t".to_string();
    score.meta.title = "Song".to_string();
    assert!(!names(&score, &both).contains(&"copyright"));
    assert!(!names(&score, &both).contains(&"missing_characters"));
}

#[test]
fn a_second_voice_has_no_rhythm_under_a_lone_tab() {
    let two = build(&[measure(
        (4, 4),
        0,
        vec![(0, quarters(&[(1, 5); 4])), (1, quarters(&[(4, 2); 4]))],
    )]);
    assert_eq!(
        names(&two, &options(StaffMode::Tab)),
        vec!["secondary_voice_rhythm"]
    );
    assert!(names(&two, &options(StaffMode::Both)).is_empty());
    assert!(names(&score(1), &options(StaffMode::Tab)).is_empty());
}

struct OpaqueCase {
    element: &'static str,
    name: &'static str,
}

impl OpaqueCase {
    const fn new(element: &'static str, name: &'static str) -> OpaqueCase {
        OpaqueCase { element, name }
    }
}

#[test]
fn opaque_guitar_pro_notation_is_named_by_its_element() {
    let both = options(StaffMode::Both);
    let cases = [
        OpaqueCase::new("<TripletFeel>Triplet8th</TripletFeel>", "triplet_feel"),
        OpaqueCase::new("<Arpeggio>Down</Arpeggio>", "arpeggios"),
        OpaqueCase::new("<Wah>Open</Wah>", "wah"),
        OpaqueCase::new("<Golpe>Thumb</Golpe>", "golpe"),
        OpaqueCase::new("<Ornament>UpperMordent</Ornament>", "ornaments"),
        OpaqueCase::new("<SimileMark>Simple</SimileMark>", "simile_marks"),
        OpaqueCase::new("<Slashed/>", "slash_notation"),
        OpaqueCase::new("<LetRingThroughout/>", "let_ring_throughout"),
        OpaqueCase::new("<Fermatas><Fermata/></Fermatas>", "fermatas"),
    ];
    for OpaqueCase { element, name } in cases {
        let mut one = score(2);
        let ids = [
            one.master_bars[1].id.0,
            one.tracks[0].id.0,
            one.tracks[0].staves[0].bars[1].id.0,
            beat_id(&one, 1, 0),
            one.tracks[0].staves[0].bars[1].voices[0].beats[0].notes[0]
                .id
                .0,
        ];
        for (place, id) in ids.iter().enumerate() {
            let mut placed = one.clone();
            keep(
                &mut placed,
                *id,
                &format!("<Keep parent=\"\" index=\"{place}\">{element}</Keep>"),
            );
            assert_eq!(names(&placed, &both), vec![name], "{element} at {place}");
        }
        keep(
            &mut one,
            ids[0],
            "<Keep parent=\"\" index=\"2\"><Wah>None</Wah></Keep>",
        );
        assert!(names(&one, &both).is_empty());
    }
}

#[test]
fn typed_directions_are_reported_only_in_the_bars_shown() {
    let both = options(StaffMode::Both);
    let mut marked = score(2);
    assert!(names(&marked, &both).is_empty());
    marked.master_bars[1].directions = Directions {
        targets: vec![DirectionTarget::Coda],
        jumps: vec![DirectionJump::DaCoda],
    };
    assert_eq!(names(&marked, &both), vec!["directions"]);
    let first = LayoutOptions {
        bars: Some(BarRange { first: 0, last: 0 }),
        ..LayoutOptions::default()
    };
    assert!(names(&marked, &first).is_empty());
    let mut opaque = score(2);
    let id = opaque.master_bars[1].id.0;
    keep(
        &mut opaque,
        id,
        "<Keep parent=\"\" index=\"2\" guard=\"directions:;\"><Directions><Jump>ToTheEnd</Jump></Directions></Keep>",
    );
    assert!(names(&opaque, &both).is_empty());
}

#[test]
fn a_typed_triplet_feel_is_reported_only_in_the_bars_shown() {
    let both = options(StaffMode::Both);
    let mut felt = score(2);
    felt.master_bars[1].triplet_feel = TripletFeel::DottedEighth;
    assert_eq!(names(&felt, &both), vec!["triplet_feel"]);
    let first = LayoutOptions {
        bars: Some(BarRange { first: 0, last: 0 }),
        ..LayoutOptions::default()
    };
    assert!(names(&felt, &first).is_empty());
}

#[test]
fn track_lyrics_count_only_in_the_bars_their_syllables_reach() {
    let mut sung = score(4);
    let track = sung.tracks[0].id.0;
    keep(
        &mut sung,
        track,
        "<Keep parent=\"\" index=\"18\"><Lyrics dispatched=\"true\"><Line><Text><![CDATA[a-ma-zing grace how sweet]]></Text><Offset>1</Offset></Line></Lyrics></Keep>",
    );
    let range = |first: usize, last: usize| LayoutOptions {
        bars: Some(BarRange { first, last }),
        ..LayoutOptions::default()
    };
    assert!(names(&sung, &range(0, 0)).is_empty());
    assert_eq!(names(&sung, &range(1, 1)), vec!["lyrics"]);
    assert_eq!(names(&sung, &range(2, 2)), vec!["lyrics"]);
    assert!(names(&sung, &range(3, 3)).is_empty());
}

#[test]
fn direction_fixtures_report_directions() {
    let both = options(StaffMode::Both);
    for name in ["directions-dc-fine.gp", "directions-ds-coda.gp"] {
        let score = crate::snapshots::fixture(name);
        assert!(names(&score, &both).contains(&"directions"), "{name}");
    }
    let plain = crate::snapshots::fixture("repeats.gp");
    assert!(!names(&plain, &both).contains(&"directions"));
}

#[test]
fn whammy_and_trill_are_read_from_the_typed_effects_only() {
    let both = options(StaffMode::Both);
    let point = |offset, value| opentab_model::BendPoint::new(offset, value).unwrap();
    let dive = opentab_model::TremoloBar::new(
        point(0, 0),
        point(35, -100),
        point(35, -100),
        point(99, -200),
    )
    .unwrap();
    let flat =
        opentab_model::TremoloBar::new(point(0, 0), point(50, 0), point(50, 0), point(100, 0))
            .unwrap();
    let mut moving = score(2);
    moving.tracks[0].staves[0].bars[1].voices[0].beats[0]
        .effects
        .tremolo_bar = Some(dive);
    assert_eq!(names(&moving, &both), vec!["whammy_bar"]);
    let mut still = score(2);
    still.tracks[0].staves[0].bars[1].voices[0].beats[0]
        .effects
        .tremolo_bar = Some(flat);
    assert!(names(&still, &both).is_empty());
    let mut opaque = score(2);
    let beat = beat_id(&opaque, 1, 0);
    keep(
        &mut opaque,
        beat,
        "<Keep parent=\"\" index=\"3\"><Whammy originValue=\"0.3\" middleValue=\"-50\"/></Keep>",
    );
    assert!(names(&opaque, &both).is_empty());
    let mut unmapped = score(2);
    let beat = beat_id(&unmapped, 1, 0);
    keep(
        &mut unmapped,
        beat,
        "<Keep parent=\"\" index=\"3\" guard=\"tremolo-bar:none\"><Whammy originValue=\"0.3\" middleValue=\"-50\"/></Keep>",
    );
    assert_eq!(names(&unmapped, &both), vec!["whammy_bar"]);
    let mut trilled = score(2);
    trilled.tracks[0].staves[0].bars[1].voices[0].beats[0].notes[0]
        .effects
        .trill = Some(opentab_model::Trill {
        fret: 2,
        speed: opentab_model::TrillSpeed::default(),
    });
    for mode in [StaffMode::Both, StaffMode::Tab, StaffMode::Standard] {
        assert_eq!(names(&trilled, &options(mode)), vec!["trill"], "{mode:?}");
    }
}
