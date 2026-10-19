use opentab_model::{InstrumentKind, InstrumentSet, MidiBank, NoteKind, Pitch, Sound};

use crate::support::{Doc, note, payloads, read_gpif, track};

#[test]
fn track_names_tuning_and_transpose_are_mapped() {
    let doc = Doc {
        track: "<Transpose><Chromatic>-2</Chromatic><Octave>-1</Octave></Transpose>".to_string(),
        tuning: "38 45 50 55 59 64".to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    let track = track(&score);
    assert_eq!(track.name, "Guitar");
    assert_eq!(track.short_name, "gtr");
    assert_eq!(track.instrument.kind, InstrumentKind::Fretted);
    let strings: Vec<u8> = track.tuning.strings.iter().map(|pitch| pitch.0).collect();
    assert_eq!(strings, vec![64, 59, 55, 50, 45, 38]);
    assert_eq!(track.transpose.chromatic(), -2);
    assert_eq!(track.transpose.octave(), -1);
    assert_eq!(track.capo, 0);
}

#[test]
fn the_capo_is_mapped_and_raises_the_pitch() {
    let doc = Doc {
        staff: "<Property name=\"CapoFret\"><Fret>2</Fret></Property>".to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    let track = track(&score);
    assert_eq!(track.capo, 2);
    assert_eq!(track.pitch_of(note(&score)), Some(Pitch(45)));
}

#[test]
fn the_instrument_type_is_typed_unknown_ones_replayed_and_drum_kits_are_percussion() {
    let score = Doc::default().read();
    assert_eq!(payloads(&score, track(&score).id.0), Vec::<String>::new());
    assert_eq!(
        track(&score).instrument.set,
        Some(InstrumentSet::ElectricGuitar)
    );
    let steel = Doc {
        instrument: "steelGuitar".to_string(),
        ..Doc::default()
    }
    .read();
    assert_eq!(payloads(&steel, track(&steel).id.0), Vec::<String>::new());
    assert_eq!(
        track(&steel).instrument.set,
        Some(InstrumentSet::SteelGuitar)
    );
    let banjo = Doc {
        instrument: "banjo".to_string(),
        ..Doc::default()
    }
    .read();
    assert_eq!(track(&banjo).instrument.set, None);
    assert_eq!(track(&banjo).instrument.kind, InstrumentKind::Fretted);
    assert_eq!(
        payloads(&banjo, track(&banjo).id.0),
        vec![
            "<Keep parent=\"InstrumentSet\" index=\"0\" guard=\"instrument:fretted none\"><Type>banjo</Type></Keep>"
                .to_string()
        ]
    );
    let drums = Doc {
        instrument: "drumKit".to_string(),
        tuning: "0 0 0 0 0 0".to_string(),
        note: "<InstrumentArticulation>12</InstrumentArticulation>".to_string(),
        ..Doc::default()
    }
    .read();
    assert_eq!(track(&drums).instrument.kind, InstrumentKind::Percussion);
    assert_eq!(track(&drums).instrument.set, Some(InstrumentSet::DrumKit));
    assert_eq!(note(&drums).kind, NoteKind::Percussion { articulation: 12 });
    assert!(
        !crate::support::note_payloads(&drums)
            .iter()
            .any(|payload| payload.contains("Property name=\"String\""))
    );
}

#[test]
fn every_sound_is_typed_and_the_first_gives_the_program() {
    let doc = Doc {
        track: concat!(
            "<Sounds><Sound><Name><![CDATA[Clean]]></Name><MIDI><LSB>3</LSB><MSB>1</MSB><Program>27</Program></MIDI></Sound>",
            "<Sound><Name><![CDATA[Dirty]]></Name><MIDI><Program>30</Program></MIDI></Sound></Sounds>"
        )
        .to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    let track = track(&score);
    assert_eq!(track.instrument.midi_program, 27);
    assert_eq!(track.midi.bank, MidiBank::new(1, 3).unwrap());
    assert_eq!(
        track.sounds,
        vec![
            Sound {
                name: "Clean".to_string(),
                program: 27,
                bank: MidiBank::new(1, 3).unwrap(),
            },
            Sound {
                name: "Dirty".to_string(),
                program: 30,
                bank: MidiBank::default(),
            },
        ]
    );
    assert_eq!(payloads(&score, track.id.0), Vec::<String>::new());
}

#[test]
fn staff_properties_beyond_tuning_and_capo_are_kept_on_the_staff() {
    let doc = Doc {
        staff: "<Property name=\"FretCount\"><Number>24</Number></Property><Name><![CDATA[Standard]]></Name>"
            .to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    let staff = &track(&score).staves[0];
    assert_eq!(
        payloads(&score, staff.id.0),
        vec![
            "<Keep parent=\"Properties\" index=\"1\"><Property name=\"FretCount\"><Number>24</Number></Property></Keep>".to_string(),
            "<Keep parent=\"Properties\" index=\"2\"><Name><![CDATA[Standard]]></Name></Keep>".to_string(),
        ]
    );
}

#[test]
fn tracks_follow_the_master_track_order() {
    let gpif = Doc::default().render();
    let second = "<Track id=\"1\"><Name><![CDATA[Bass]]></Name><InstrumentSet><Type>electricBass</Type></InstrumentSet></Track>";
    let gpif = gpif
        .replace("</Track></Tracks>", &format!("</Track>{second}</Tracks>"))
        .replace("<Tracks>0</Tracks>", "<Tracks>1 0</Tracks>");
    let score = read_gpif(&gpif);
    let names: Vec<&str> = score
        .tracks
        .iter()
        .map(|track| track.name.as_str())
        .collect();
    assert_eq!(names, vec!["Bass", "Guitar"]);
    assert!(score.tracks[0].staves.is_empty());
    assert_eq!(score.tracks[1].staves[0].bars.len(), 1);
}

pub fn two_staff_document(second_staff_properties: &str) -> String {
    let second_staff = format!("<Staff><Properties>{second_staff_properties}</Properties></Staff>");
    Doc::default()
        .render()
        .replace("</Staff></Staves>", &format!("</Staff>{second_staff}</Staves>"))
        .replace("<Bars>0</Bars>", "<Bars>0 1</Bars>")
        .replace(
            "</Bar></Bars>",
            "</Bar><Bar id=\"1\"><Clef>F4</Clef><Voices>1 -1 -1 -1</Voices></Bar></Bars>",
        )
        .replace(
            "</Voice></Voices>",
            "</Voice><Voice id=\"1\"><Beats>1</Beats></Voice></Voices>",
        )
        .replace(
            "</Beat></Beats>",
            "</Beat><Beat id=\"1\"><Rhythm ref=\"0\"/><Notes>1</Notes></Beat></Beats>",
        )
        .replace(
            "</Note></Notes>",
            "</Note><Note id=\"1\"><Properties><Property name=\"String\"><String>0</String></Property><Property name=\"Fret\"><Fret>2</Fret></Property><Property name=\"Midi\"><Number>30</Number></Property></Properties></Note></Notes>",
        )
}

#[test]
fn a_second_staff_maps_its_notes_with_its_own_tuning_and_keeps_it() {
    let gpif = two_staff_document(concat!(
        "<Property name=\"Tuning\"><Pitches>28 33 38 43</Pitches></Property>",
        "<Property name=\"CapoFret\"><Fret>3</Fret></Property>"
    ));
    let score = read_gpif(&gpif);
    let track = track(&score);
    assert_eq!(track.tuning.strings.len(), 6);
    assert_eq!(track.capo, 0);
    let staff = &track.staves[1];
    let note = &staff.bars[0].voices[0].beats[0].notes[0];
    assert_eq!(note.kind, NoteKind::Fretted { string: 3, fret: 2 });
    let staff_payloads = payloads(&score, staff.id.0);
    assert_eq!(
        staff_payloads,
        vec![
            "<Keep parent=\"Properties\" index=\"0\"><Property name=\"Tuning\"><Pitches>28 33 38 43</Pitches></Property></Keep>".to_string(),
            "<Keep parent=\"Properties\" index=\"1\"><Property name=\"CapoFret\"><Fret>3</Fret></Property></Keep>".to_string(),
        ]
    );
    assert_eq!(
        payloads(&score, note.id.0),
        vec![
            "<Keep parent=\"Properties\" index=\"2\" guard=\"pitch:fretted 3 2;tuning 43 38 33 28;capo 3;transpose 0 0\"><Property name=\"Midi\"><Number>30</Number></Property></Keep>".to_string()
        ]
    );
}

#[test]
fn a_second_staff_without_its_own_tuning_uses_the_first() {
    let score = read_gpif(&two_staff_document(""));
    let staff = &track(&score).staves[1];
    let note = &staff.bars[0].voices[0].beats[0].notes[0];
    assert_eq!(note.kind, NoteKind::Fretted { string: 5, fret: 2 });
    assert!(payloads(&score, note.id.0)[0].contains("tuning 64 59 55 50 45 40;capo 0;"));
}
