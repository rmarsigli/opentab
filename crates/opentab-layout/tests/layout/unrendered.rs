use opentab_layout::{BarRange, LayoutOptions, StaffMode, Unrendered};
use opentab_model::{ElementId, InstrumentKind, NoteKind, OpaquePayload, Score};

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

fn note_id(score: &Score, bar: usize, beat: usize) -> ElementId {
    score.tracks[0].staves[0].bars[bar].voices[0].beats[beat].notes[0]
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
    let id = note_id(score, 1, 0);
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
    assert_eq!(
        found(&all, &guitar_only),
        vec![Unrendered::Lyrics, Unrendered::MultiBarRests]
    );
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
    let note = note_id(&quiet, 1, 0);
    keep(
        &mut quiet,
        note,
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
    let mut known = score(4);
    drums(&mut known);
    let drums = known.tracks.len() - 1;
    for beat in &mut known.tracks[drums].staves[0].bars[1].voices[0].beats {
        for note in &mut beat.notes {
            note.kind = NoteKind::Percussion { articulation: 38 };
        }
    }
    assert!(found(&quiet, &both).is_empty());
    assert!(found(&known, &both).is_empty());
}
