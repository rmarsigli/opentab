use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{
    Bar, Beat, BeatEffects, Clef, Diagnostic, Directions, DrumArticulation, DrumElement, DrumKit,
    Duration, InstrumentKind, InstrumentSet, KeySignature, MasterBar, Note, NoteEffects, NoteKind,
    NoteValue, Pitch, Score, TechniquePlacement, TimeSignature, TrackMidi, Transpose, Voice,
    VoiceSlot, blank_score, diagnose, gm_drum_kit,
};

use crate::anchors::gpif_text;
use crate::support::{Doc, payloads, read_gpif, track};

const NOTES_PER_BEAT: usize = 4;
const BEATS_PER_BAR: usize = 4;
const BARE_SET: &str = "<InstrumentSet><Type>drumKit</Type></InstrumentSet>";
const PERCUSSION_NONE: &str = "guard=\"drum-kit:percussion none\"";

fn drum_beats(score: &mut Score, articulations: &[u16]) -> Vec<Beat> {
    let mut beats = Vec::new();
    for chord in articulations.chunks(NOTES_PER_BEAT) {
        let mut notes = Vec::new();
        for articulation in chord {
            notes.push(Note {
                id: score.alloc().expect("id"),
                kind: NoteKind::Percussion {
                    articulation: *articulation,
                },
                effects: NoteEffects::default(),
            });
        }
        beats.push(Beat {
            id: score.alloc().expect("id"),
            duration: Duration::new(NoteValue::Quarter, 0, None).expect("duration"),
            notes,
            effects: BeatEffects::default(),
        });
    }
    beats
}

fn from_scratch(articulations: &[u16]) -> Score {
    let mut score = blank_score().expect("blank");
    let mut beats = drum_beats(&mut score, articulations);
    while !beats.len().is_multiple_of(BEATS_PER_BAR) {
        beats.push(Beat {
            id: score.alloc().expect("id"),
            duration: Duration::new(NoteValue::Quarter, 0, None).expect("duration"),
            notes: Vec::new(),
            effects: BeatEffects::default(),
        });
    }
    let mut bars = Vec::new();
    for chunk in beats.chunks(BEATS_PER_BAR) {
        bars.push(Bar {
            id: score.alloc().expect("id"),
            clef: Clef::Neutral,
            voices: vec![Voice {
                id: score.alloc().expect("id"),
                slot: VoiceSlot::new(0).expect("slot"),
                beats: chunk.to_vec(),
            }],
        });
    }
    while score.master_bars.len() < bars.len() {
        let id = score.alloc().expect("id");
        score.master_bars.push(MasterBar {
            id,
            time_signature: TimeSignature::new(4, 4).expect("time"),
            key_signature: KeySignature::default(),
            repeat_start: false,
            repeat_end: None,
            alternate_endings: 0,
            double_bar: false,
            marker: None,
            directions: Directions::default(),
            tempo_changes: Vec::new(),
        });
    }
    let track = &mut score.tracks[0];
    track.name = "Drums".to_string();
    track.short_name = "Dr".to_string();
    track.instrument.kind = InstrumentKind::Percussion;
    track.instrument.set = Some(InstrumentSet::DrumKit);
    track.instrument.midi_program = 0;
    track.midi = TrackMidi::default_for(InstrumentKind::Percussion);
    track.tuning.strings = vec![Pitch(0); 6];
    track.transpose = Transpose::default();
    track.staves[0].bars = bars;
    score
}

fn every_gm_articulation() -> Vec<u16> {
    let count = u16::try_from(gm_drum_kit().articulations().len()).expect("small kit");
    (0..count).collect()
}

fn texts(gpif: &str, name: &str) -> Vec<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    gpif.split(&open)
        .skip(1)
        .filter_map(|piece| piece.split_once(&close).map(|(text, _)| text.to_string()))
        .collect()
}

#[test]
fn a_percussion_track_built_from_scratch_is_written_with_the_general_midi_kit() {
    let score = from_scratch(&every_gm_articulation());
    assert_eq!(score.tracks[0].drum_kit, None);
    assert_eq!(diagnose(&score), Vec::new());
    let bytes = write_gp(&score).expect("a kit-less percussion track is written");
    let gpif = gpif_text(&bytes);
    let kit = gm_drum_kit();
    let keys: Vec<String> = kit
        .articulations()
        .iter()
        .map(|found| found.output_key.to_string())
        .collect();
    assert_eq!(texts(&gpif, "OutputMidiNumber"), keys);
    assert_eq!(texts(&gpif, "InputMidiNumbers"), keys);
    assert_eq!(gpif.matches("<Element>").count(), kit.elements().len());
    let used: Vec<String> = every_gm_articulation().iter().map(u16::to_string).collect();
    assert_eq!(texts(&gpif, "InstrumentArticulation"), used);
    assert!(gpif.contains("<InstrumentSet>\n<Type>drumKit</Type>\n<Elements>\n<Element>\n"));
    assert!(
        gpif.contains("<Sounds>"),
        "GP crashes on a drum track without Sounds"
    );
    let mut typed = score.clone();
    typed.tracks[0].drum_kit = Some(kit.clone());
    assert_eq!(write_gp(&typed).expect("typed"), bytes);
    let again = read_gp(&bytes).expect("read");
    assert_eq!(again.tracks[0].drum_kit, Some(kit));
    assert_eq!(articulations_of(&again), every_gm_articulation());
    assert_eq!(write_gp(&again).expect("rewrite"), bytes);
    assert_eq!(read_gp(&bytes).expect("reread"), again);
}

fn articulations_of(score: &Score) -> Vec<u16> {
    score.tracks[0].staves[0]
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| match note.kind {
            NoteKind::Percussion { articulation } => Some(articulation),
            NoteKind::Fretted { .. } => None,
        })
        .collect()
}

fn custom_kit() -> DrumKit {
    let articulation = |name: &str, line: i8, keys: Vec<u8>, output: u8| DrumArticulation {
        name: name.to_string(),
        staff_line: line,
        noteheads: ["noteheadXBlack", "noteheadHalf", "x"].map(str::to_string),
        technique_placement: TechniquePlacement::Above,
        technique_symbol: "articAccentAbove".to_string(),
        input_keys: keys,
        rse_sound: "a.b&c".to_string(),
        output_key: output,
    };
    let element = |name: &str, kind: &str, count: usize| DrumElement {
        name: name.to_string(),
        kind: kind.to_string(),
        soundbank: " spaced ".to_string(),
        count,
    };
    DrumKit::new(
        vec![
            element("<Mine & \"yours\">", "tom", 2),
            element("", "", 0),
            element("  ", "unknown", 1),
        ],
        vec![
            articulation("first\r\nline", -24, vec![1, 127], 0),
            articulation("", 38, Vec::new(), 127),
            articulation("tab\tname", 0, vec![60], 60),
        ],
    )
    .expect("kit")
}

#[test]
fn a_custom_kit_round_trips_exactly() {
    let mut score = from_scratch(&[2, 0, 1, 2]);
    score.tracks[0].drum_kit = Some(custom_kit());
    let bytes = write_gp(&score).expect("write");
    let gpif = gpif_text(&bytes);
    assert!(gpif.contains("<Articulations/>"));
    assert!(gpif.contains("<InputMidiNumbers/>"));
    assert!(gpif.contains("<InputMidiNumbers>1 127</InputMidiNumbers>"));
    assert!(gpif.contains("<TechniquePlacement>above</TechniquePlacement>"));
    let again = read_gp(&bytes).expect("read");
    assert_eq!(again.tracks[0].drum_kit, Some(custom_kit()));
    assert_eq!(articulations_of(&again), vec![2, 0, 1, 2]);
    assert_eq!(write_gp(&again).expect("rewrite"), bytes);
}

fn canonical_elements() -> String {
    concat!(
        "<Elements><Element><Name>Kick</Name><Type>kickDrum</Type><SoundbankName>Bank-K</SoundbankName>",
        "<Articulations><Articulation><Name>Kick hit</Name><StaffLine>7</StaffLine>",
        "<Noteheads>noteheadBlack noteheadHalf noteheadWhole</Noteheads>",
        "<TechniquePlacement>below</TechniquePlacement><TechniqueSymbol></TechniqueSymbol>",
        "<InputMidiNumbers>36</InputMidiNumbers><OutputRSESound>kick.hit</OutputRSESound>",
        "<OutputMidiNumber>35</OutputMidiNumber></Articulation></Articulations></Element>",
        "<Element><Name>Hat</Name><Type>hiHat</Type><SoundbankName></SoundbankName>",
        "<Articulations><Articulation><Name>Hat closed</Name><StaffLine>-1</StaffLine>",
        "<Noteheads>noteheadXBlack noteheadXHalf noteheadXWhole</Noteheads>",
        "<TechniquePlacement>outside</TechniquePlacement><TechniqueSymbol>pictOpen</TechniqueSymbol>",
        "<InputMidiNumbers>42 44</InputMidiNumbers><OutputRSESound></OutputRSESound>",
        "<OutputMidiNumber>42</OutputMidiNumber></Articulation></Articulations></Element></Elements>"
    )
    .to_string()
}

fn drum_doc(elements: &str) -> String {
    let doc = Doc {
        instrument: "drumKit".to_string(),
        tuning: "0 0 0 0 0 0".to_string(),
        note: "<InstrumentArticulation>1</InstrumentArticulation>".to_string(),
        ..Doc::default()
    };
    let set = format!(
        "<InstrumentSet>\n<Name>Kit</Name>\n<Type>drumKit</Type>\n<LineCount>5</LineCount>\n{elements}\n</InstrumentSet>"
    );
    let text = doc.render();
    assert!(text.contains(BARE_SET));
    text.replace(BARE_SET, &set)
}

fn set_children(gpif: &str) -> Vec<String> {
    let start = gpif.find("<InstrumentSet>").expect("set");
    let end = gpif.find("</InstrumentSet>").expect("set end");
    gpif[start..end]
        .lines()
        .skip(1)
        .filter(|line| !line.starts_with("<Element") || line.starts_with("<Elements"))
        .filter_map(|line| {
            let name: String = line
                .strip_prefix('<')?
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric())
                .collect();
            ["Name", "Type", "LineCount", "Elements"]
                .contains(&name.as_str())
                .then_some(name)
        })
        .take(4)
        .collect()
}

#[test]
fn a_canonical_kit_is_typed_and_the_set_keeps_its_child_order() {
    let score = read_gpif(&drum_doc(&canonical_elements()));
    let kit = track(&score).drum_kit.clone().expect("typed");
    assert_eq!(
        kit.elements()
            .iter()
            .map(|element| (element.name.as_str(), element.kind.as_str(), element.count))
            .collect::<Vec<_>>(),
        vec![("Kick", "kickDrum", 1), ("Hat", "hiHat", 1)]
    );
    assert_eq!(kit.key_of(0), Some(35));
    assert_eq!(kit.key_of(1), Some(42));
    assert_eq!(kit.articulations()[1].input_keys, vec![42, 44]);
    assert_eq!(kit.articulations()[1].staff_line, -1);
    assert_eq!(
        kit.articulations()[0].technique_placement,
        TechniquePlacement::Below
    );
    let kept = payloads(&score, track(&score).id.0);
    assert!(
        kept.iter().all(|payload| !payload.contains("<Elements")),
        "{kept:?}"
    );
    let bytes = write_gp(&score).expect("write");
    let gpif = gpif_text(&bytes);
    assert_eq!(
        set_children(&gpif),
        ["Name", "Type", "LineCount", "Elements"]
    );
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn an_untypeable_kit_stays_opaque_until_a_kit_is_set() {
    let canonical = canonical_elements();
    let variants = [
        canonical.replace("<OutputRSESound>kick.hit</OutputRSESound>", ""),
        canonical.replace(
            "noteheadBlack noteheadHalf noteheadWhole",
            "noteheadBlack noteheadHalf",
        ),
        canonical.replace("<Name>Kick</Name>", "<Name><![CDATA[Kick]]></Name>"),
        canonical.replace("<StaffLine>7</StaffLine>", "<StaffLine>+7</StaffLine>"),
        canonical.replace("<Element>", "<Element kind=\"x\">"),
        canonical.replace("</Element></Elements>", "<Extra/></Element></Elements>"),
        canonical.replace(
            "<OutputMidiNumber>42</OutputMidiNumber>",
            "<OutputMidiNumber>300</OutputMidiNumber>",
        ),
        canonical.replace("below", "sideways"),
    ];
    for variant in variants {
        let score = read_gpif(&drum_doc(&variant));
        assert_eq!(track(&score).drum_kit, None, "{variant}");
        let kept = payloads(&score, track(&score).id.0);
        assert_eq!(
            kept.iter()
                .filter(|payload| payload.contains(PERCUSSION_NONE))
                .count(),
            1,
            "{kept:?}"
        );
        let bytes = write_gp(&score).expect("write");
        let gpif = gpif_text(&bytes);
        assert_eq!(gpif.matches("<Elements>").count(), 1);
        assert_eq!(
            set_children(&gpif),
            ["Name", "Type", "LineCount", "Elements"]
        );
        assert_eq!(read_gp(&bytes).expect("read"), score);
        let mut typed = score.clone();
        typed.tracks[0].drum_kit = Some(gm_drum_kit());
        let bytes = write_gp(&typed).expect("write");
        let gpif = gpif_text(&bytes);
        assert_eq!(gpif.matches("<Elements>").count(), 1);
        assert_eq!(gpif.matches("<Articulation>").count(), 47);
        assert_eq!(
            set_children(&gpif),
            ["Name", "Type", "LineCount", "Elements"]
        );
        let again = read_gp(&bytes).expect("read");
        assert_eq!(again.tracks[0].drum_kit, Some(gm_drum_kit()));
        let mut fretted = score.clone();
        fretted.tracks[0].instrument.kind = InstrumentKind::Fretted;
        fretted.tracks[0].instrument.set = Some(InstrumentSet::ElectricGuitar);
        fretted.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].kind =
            NoteKind::Fretted { string: 0, fret: 0 };
        let gpif = gpif_text(&write_gp(&fretted).expect("write"));
        assert!(!gpif.contains("<Elements"), "{gpif}");
    }
}

#[test]
fn only_the_first_elements_child_is_typed() {
    let doubled = format!("{}{}", canonical_elements(), canonical_elements());
    let score = read_gpif(&drum_doc(&doubled));
    assert!(track(&score).drum_kit.is_some());
    let bytes = write_gp(&score).expect("write");
    assert_eq!(gpif_text(&bytes).matches("<Elements>").count(), 2);
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn a_kit_on_a_fretted_track_is_refused_and_diagnosed() {
    let mut score = Doc::default().read();
    score.tracks[0].drum_kit = Some(gm_drum_kit());
    assert_eq!(
        diagnose(&score),
        vec![Diagnostic::DrumKitOnFretted {
            track: score.tracks[0].id
        }]
    );
    match write_gp(&score) {
        Err(GpError::Unsupported { path, reason }) => {
            assert_eq!(path, "tracks[0].drum_kit");
            assert_eq!(reason, Refusal::DrumKitOnFretted);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn kit_text_guitar_pro_cannot_hold_is_refused() {
    let mut score = from_scratch(&[0]);
    let kit = gm_drum_kit();
    let mut articulations = kit.articulations().to_vec();
    articulations[0].rse_sound = "bell\u{1}".to_string();
    score.tracks[0].drum_kit =
        Some(DrumKit::new(kit.elements().to_vec(), articulations).expect("kit"));
    match write_gp(&score) {
        Err(GpError::Unsupported { path, reason }) => {
            assert_eq!(path, "tracks[0].drum_kit");
            assert_eq!(reason, Refusal::Character { code: 1 });
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}
