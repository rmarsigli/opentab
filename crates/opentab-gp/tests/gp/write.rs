use std::io::{Cursor, Read};

use opentab_gp::{GpError, Refusal, read_gp, write_gp};
use opentab_model::{
    Bar, Base64Bytes, Beat, BeatEffects, ContainerEntry, Duration, Finger, Fraction, Harmonic,
    HarmonicKind, Instrument, InstrumentKind, InstrumentSet, KeySignature, Marker, MasterBar, Note,
    NoteEffects, NoteKind, NoteValue, OpaquePayload, Pitch, Score, Staff, Tempo, TempoChange,
    TempoUnit, TimeSignature, Track, TrackMidi, Transpose, Tuning, Voice, VoiceSlot,
};
use zip::{CompressionMethod, ZipArchive};

use crate::support::{Doc, SCORE_ENTRY, gp, read_gpif};
use crate::track::two_staff_document;

const STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

struct Scratch {
    score: Score,
}

impl Scratch {
    fn new(master_bars: usize) -> Scratch {
        let mut score = Score::new();
        for _ in 0..master_bars {
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
                tempo_changes: Vec::new(),
            });
        }
        Scratch { score }
    }

    fn track(&mut self, kind: InstrumentKind, bars: Vec<Vec<(NoteValue, Vec<NoteKind>)>>) {
        let id = self.score.alloc().expect("id");
        let tuning = match kind {
            InstrumentKind::Fretted => STANDARD.iter().map(|pitch| Pitch(*pitch)).collect(),
            InstrumentKind::Percussion => Vec::new(),
        };
        let staff_id = self.score.alloc().expect("id");
        let mut staff = Staff {
            id: staff_id,
            bars: Vec::new(),
        };
        for beats in bars {
            let bar_id = self.score.alloc().expect("id");
            let voice_id = self.score.alloc().expect("id");
            let mut voice = Voice {
                id: voice_id,
                slot: VoiceSlot::new(0).expect("slot"),
                beats: Vec::new(),
            };
            for (value, kinds) in beats {
                let beat_id = self.score.alloc().expect("id");
                let mut beat = Beat {
                    id: beat_id,
                    duration: Duration::new(value, 0, None).expect("duration"),
                    notes: Vec::new(),
                    effects: BeatEffects::default(),
                };
                for kind in kinds {
                    let note_id = self.score.alloc().expect("id");
                    beat.notes.push(Note {
                        id: note_id,
                        kind,
                        effects: NoteEffects::default(),
                    });
                }
                voice.beats.push(beat);
            }
            staff.bars.push(Bar {
                id: bar_id,
                clef: opentab_model::Clef::Treble,
                voices: vec![voice],
            });
        }
        self.score.tracks.push(Track {
            id,
            name: "Guitar".to_string(),
            short_name: "gtr".to_string(),
            instrument: Instrument {
                kind,
                midi_program: 0,
                set: Some(match kind {
                    InstrumentKind::Fretted => InstrumentSet::ElectricGuitar,
                    InstrumentKind::Percussion => InstrumentSet::DrumKit,
                }),
            },
            midi: TrackMidi::default_for(kind),
            sounds: Vec::new(),
            tuning: Tuning { strings: tuning },
            capo: 0,
            transpose: Transpose::default(),
            staves: vec![staff],
            automations: Vec::new(),
        });
    }
}

fn fretted(string: u8, fret: u8) -> NoteKind {
    NoteKind::Fretted { string, fret }
}

fn one_note() -> Score {
    let mut scratch = Scratch::new(1);
    scratch.track(
        InstrumentKind::Fretted,
        vec![vec![(NoteValue::Whole, vec![fretted(5, 3)])]],
    );
    scratch.score
}

fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>, CompressionMethod, String)> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut out = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("entry");
        let mut data = Vec::new();
        file.read_to_end(&mut data).expect("data");
        let date = file
            .last_modified()
            .map(|date| format!("{}-{:02}-{:02}", date.year(), date.month(), date.day()))
            .unwrap_or_default();
        out.push((file.name().to_string(), data, file.compression(), date));
    }
    out
}

fn gpif_of(bytes: &[u8]) -> String {
    let entry = entries(bytes)
        .into_iter()
        .find(|(name, ..)| name == SCORE_ENTRY)
        .expect("score entry");
    String::from_utf8(entry.1).expect("utf-8")
}

fn written(score: &Score) -> Vec<u8> {
    match write_gp(score) {
        Ok(bytes) => bytes,
        Err(error) => panic!("write failed: {error}"),
    }
}

fn refusal(score: &Score) -> (String, Refusal) {
    match write_gp(score) {
        Err(GpError::Unsupported { path, reason }) => (path, reason),
        other => panic!(
            "expected a refusal, got {:?}",
            other.map(|bytes| bytes.len())
        ),
    }
}

const MINIMAL_GPIF: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<GPIF>\n",
    "<GPVersion>8.1.5</GPVersion>\n",
    "<GPRevision required=\"12024\" recommended=\"13000\">13008</GPRevision>\n",
    "<Score>\n<Title><![CDATA[]]></Title>\n<SubTitle><![CDATA[]]></SubTitle>\n",
    "<Artist><![CDATA[]]></Artist>\n<Album><![CDATA[]]></Album>\n<Words><![CDATA[]]></Words>\n",
    "<Music><![CDATA[]]></Music>\n<Copyright><![CDATA[]]></Copyright>\n",
    "<Tabber><![CDATA[]]></Tabber>\n<Instructions><![CDATA[]]></Instructions>\n",
    "<Notices><![CDATA[]]></Notices>\n</Score>\n",
    "<MasterTrack>\n<Tracks>0</Tracks>\n</MasterTrack>\n",
    "<Tracks>\n<Track id=\"0\">\n<Name><![CDATA[Guitar]]></Name>\n<ShortName><![CDATA[gtr]]></ShortName>\n",
    "<InstrumentSet>\n<Type>electricGuitar</Type>\n</InstrumentSet>\n",
    "<Transpose>\n<Chromatic>0</Chromatic>\n<Octave>0</Octave>\n</Transpose>\n",
    "<Sounds>\n<Sound>\n<MIDI>\n<Program>0</Program>\n</MIDI>\n</Sound>\n</Sounds>\n",
    "<Staves>\n<Staff>\n<Properties>\n",
    "<Property name=\"CapoFret\">\n<Fret>0</Fret>\n</Property>\n",
    "<Property name=\"Tuning\">\n<Pitches>40 45 50 55 59 64</Pitches>\n</Property>\n",
    "</Properties>\n</Staff>\n</Staves>\n</Track>\n</Tracks>\n",
    "<MasterBars>\n<MasterBar>\n<Key>\n<AccidentalCount>0</AccidentalCount>\n<Mode>Major</Mode>\n</Key>\n",
    "<Time>4/4</Time>\n<Bars>0</Bars>\n</MasterBar>\n</MasterBars>\n",
    "<Bars>\n<Bar id=\"0\">\n<Clef>G2</Clef>\n<Voices>0 -1 -1 -1</Voices>\n</Bar>\n</Bars>\n",
    "<Voices>\n<Voice id=\"0\">\n<Beats>0</Beats>\n</Voice>\n</Voices>\n",
    "<Beats>\n<Beat id=\"0\">\n<Dynamic>MF</Dynamic>\n<Rhythm ref=\"0\"/>\n<Notes>0</Notes>\n</Beat>\n</Beats>\n",
    "<Notes>\n<Note id=\"0\">\n<Properties>\n",
    "<Property name=\"ConcertPitch\">\n<Pitch>\n<Step>G</Step>\n<Accidental/>\n<Octave>3</Octave>\n</Pitch>\n</Property>\n",
    "<Property name=\"Fret\">\n<Fret>3</Fret>\n</Property>\n",
    "<Property name=\"Midi\">\n<Number>43</Number>\n</Property>\n",
    "<Property name=\"String\">\n<String>0</String>\n</Property>\n",
    "<Property name=\"TransposedPitch\">\n<Pitch>\n<Step>G</Step>\n<Accidental/>\n<Octave>3</Octave>\n</Pitch>\n</Property>\n",
    "</Properties>\n</Note>\n</Notes>\n",
    "<Rhythms>\n<Rhythm id=\"0\">\n<NoteValue>Whole</NoteValue>\n</Rhythm>\n</Rhythms>\n",
    "</GPIF>\n"
);

#[test]
fn a_score_from_scratch_gets_the_minimal_container_and_gpif() {
    let score = one_note();
    assert!(score.container.is_empty());
    let bytes = written(&score);
    let found: Vec<(String, Vec<u8>, CompressionMethod, String)> = entries(&bytes)
        .into_iter()
        .map(|(name, data, method, date)| {
            let data = if name == SCORE_ENTRY {
                Vec::new()
            } else {
                data
            };
            (name, data, method, date)
        })
        .collect();
    let date = "1980-01-01".to_string();
    assert_eq!(
        found,
        vec![
            (
                "Content/PartConfiguration".to_string(),
                Vec::new(),
                CompressionMethod::Deflated,
                date.clone()
            ),
            (
                SCORE_ENTRY.to_string(),
                Vec::new(),
                CompressionMethod::Deflated,
                date.clone()
            ),
            (
                "VERSION".to_string(),
                b"7.0".to_vec(),
                CompressionMethod::Deflated,
                date
            ),
        ]
    );
    assert_eq!(gpif_of(&bytes), MINIMAL_GPIF);
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn writing_twice_gives_identical_bytes() {
    let mut scratch = Scratch::new(2);
    let values = [
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Quarter,
        NoteValue::Half,
        NoteValue::Sixteenth,
        NoteValue::Eighth,
        NoteValue::Whole,
        NoteValue::ThirtySecond,
    ];
    let bars = vec![
        values
            .iter()
            .map(|value| (*value, vec![fretted(0, 1)]))
            .collect(),
        values
            .iter()
            .rev()
            .map(|value| (*value, vec![fretted(1, 2), fretted(2, 3)]))
            .collect(),
    ];
    scratch.track(InstrumentKind::Fretted, bars);
    let score = scratch.score;
    let first = written(&score);
    for _ in 0..8 {
        assert_eq!(written(&score), first);
    }
    let gpif = gpif_of(&first);
    let rhythms: Vec<&str> = gpif
        .split("<NoteValue>")
        .skip(1)
        .filter_map(|rest| rest.split('<').next())
        .collect();
    assert_eq!(
        rhythms,
        vec!["Quarter", "Eighth", "Half", "16th", "Whole", "32nd"]
    );
    assert_eq!(read_gp(&first).expect("read"), score);
}

#[test]
fn xml_special_and_non_ascii_text_round_trips() {
    let mut score = one_note();
    score.meta.title = "Canção 🎸 <a & b> \"q\" 'x'".to_string();
    score.meta.notices = "end ]]> of ]]]]> data\nnext line\ttab".to_string();
    score.tracks[0].name = "Gtr ]]>".to_string();
    score.master_bars[0].marker = Some(Marker {
        text: "Verse & <Chorus>".to_string(),
        letter: "]]>".to_string(),
    });
    score.tracks[0].staves[0].bars[0].voices[0].beats[0]
        .effects
        .text = Some("日本語 🤘 a]]>b".to_string());
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    assert!(gpif.contains("<Title><![CDATA[Canção 🎸 <a & b> \"q\" 'x']]></Title>"));
    assert!(gpif.contains("<Name><![CDATA[Gtr ]]]]><![CDATA[>]]></Name>"));
    assert!(gpif.contains("<FreeText><![CDATA[日本語 🤘 a]]]]><![CDATA[>b]]></FreeText>"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn opaque_text_with_markup_round_trips() {
    let doc = Doc {
        beat: "<Lyrics><Line><![CDATA[x <&> y]]></Line><Line>a &amp; b &lt;c&gt;</Line></Lyrics>"
            .to_string(),
        root: "<Encoding><EncodingDescription>GP8 ✓</EncodingDescription></Encoding>".to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    assert!(gpif.contains("<Line><![CDATA[x <&> y]]></Line>"));
    assert!(gpif.contains("<Line>a &amp; b &lt;c&gt;</Line>"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn a_partly_known_note_keeps_both_parts() {
    let doc = Doc {
        note: "<Ornament>UpperMordent</Ornament>".to_string(),
        note_properties: concat!(
            "<Property name=\"HopoOrigin\"><Enable/></Property>",
            "<Property name=\"HopoDestination\"><Enable/></Property>",
            "<Property name=\"Slide\"><Flags>2</Flags></Property>"
        )
        .to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    let note = crate::support::note(&score);
    assert!(note.effects.hammer_on_pull_off);
    assert_eq!(note.effects.slide, Some(opentab_model::Slide::Legato));
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    assert!(gpif.contains("<Ornament>UpperMordent</Ornament>"));
    assert!(gpif.contains("<Property name=\"HopoDestination\">\n<Enable/>\n</Property>"));
    assert!(gpif.contains("<Property name=\"HopoOrigin\">\n<Enable/>\n</Property>"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn replayed_values_are_dropped_when_the_model_changes() {
    let doc = Doc {
        note_properties: "<Property name=\"Midi\"><Number>99</Number></Property>".to_string(),
        beat: concat!(
            "<Properties><Property name=\"Brush\"><Direction>Up</Direction></Property></Properties>",
            "<XProperties><XProperty id=\"687935490\"><Float>0.5</Float></XProperty></XProperties>"
        )
        .to_string(),
        ..Doc::default()
    };
    let mut score = doc.read();
    let unchanged = gpif_of(&written(&score));
    assert!(unchanged.contains("<Number>99</Number>"));
    assert!(unchanged.contains("<XProperty id=\"687935490\">"));
    let beat = &mut score.tracks[0].staves[0].bars[0].voices[0].beats[0];
    beat.effects.stroke = None;
    beat.notes[0].kind = fretted(5, 5);
    let bytes = written(&score);
    let changed = gpif_of(&bytes);
    assert!(!changed.contains("<Number>99</Number>"));
    assert!(changed.contains("<Number>45</Number>"));
    assert!(!changed.contains("687935490"));
    assert!(!changed.contains("Brush"));
    let again = read_gp(&bytes).expect("read");
    assert_eq!(again.tracks, score.tracks);
}

#[test]
fn unreferenced_pool_elements_are_renumbered_with_their_references() {
    let gpif = Doc::default()
        .render()
        .replace(
            "<Beats><Beat id=\"0\">",
            "<Beats><Beat id=\"0\"><Rhythm ref=\"0\"/></Beat><Beat id=\"1\">",
        )
        .replace("<Beats>0</Beats>", "<Beats>1</Beats>");
    let score = read_gpif(&gpif);
    let bytes = written(&score);
    let written_gpif = gpif_of(&bytes);
    assert!(written_gpif.contains("<Beats>1</Beats>"));
    assert!(
        written_gpif.contains("<Beat id=\"0\">\n<Rhythm ref=\"0\"/>\n</Beat>\n<Beat id=\"1\">")
    );
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn a_second_staff_without_its_own_tuning_gets_the_track_tuning_and_capo() {
    let mut score = one_note();
    score.tracks[0].capo = 2;
    let mut second = score.tracks[0].staves[0].clone();
    second.id = score.alloc().expect("id");
    second.bars[0].id = score.alloc().expect("id");
    second.bars[0].voices[0].id = score.alloc().expect("id");
    second.bars[0].voices[0].beats[0].id = score.alloc().expect("id");
    second.bars[0].voices[0].beats[0].notes[0].id = score.alloc().expect("id");
    score.tracks[0].staves.push(second);
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    let staff = concat!(
        "<Staff>\n<Properties>\n<Property name=\"CapoFret\">\n<Fret>2</Fret>\n</Property>\n",
        "<Property name=\"Tuning\">\n<Pitches>40 45 50 55 59 64</Pitches>\n</Property>\n",
        "</Properties>\n</Staff>\n"
    );
    assert_eq!(gpif.matches(staff).count(), 2);
    let read = read_gp(&bytes).expect("read");
    assert_eq!(read, score);
    assert!(read.extensions.0.is_empty());
    let mut own = score.clone();
    own.extensions.0.insert(
        own.tracks[0].staves[1].id.0,
        vec![OpaquePayload {
            format: "gpif".to_string(),
            data: "<Keep parent=\"Properties\" index=\"1\"><Property name=\"Tuning\"><Pitches>38 45 50 55 59 64</Pitches></Property></Keep>".to_string(),
        }],
    );
    let gpif = gpif_of(&written(&own));
    assert_eq!(gpif.matches("<Pitches>").count(), 2);
    assert!(gpif.contains("<Pitches>38 45 50 55 59 64</Pitches>"));
}

#[test]
fn a_second_staff_writes_its_notes_with_its_own_tuning() {
    let score = read_gpif(&two_staff_document(concat!(
        "<Property name=\"Tuning\"><Pitches>28 33 38 43</Pitches></Property>",
        "<Property name=\"CapoFret\"><Fret>3</Fret></Property>"
    )));
    let bytes = written(&score);
    assert_eq!(read_gp(&bytes).expect("read"), score);
    let mut changed = score.clone();
    changed.tracks[0].staves[1].bars[0].voices[0].beats[0].notes[0].kind = fretted(0, 1);
    let bytes = written(&changed);
    let gpif = gpif_of(&bytes);
    assert!(gpif.contains("<String>3</String>"));
    assert!(gpif.contains("<Number>47</Number>"));
    assert_eq!(read_gp(&bytes).expect("read").tracks, changed.tracks);
}

fn chord_document() -> Doc {
    Doc {
        staff: concat!(
            "<Property name=\"DiagramCollection\"><Items>",
            "<Item id=\"0\" name=\"G\"><Diagram/></Item><Item id=\"1\" name=\"Am\"><Diagram/></Item>",
            "</Items></Property>"
        )
        .to_string(),
        beat: "<Chord><![CDATA[0]]></Chord>".to_string(),
        ..Doc::default()
    }
}

#[test]
fn chord_names_use_the_diagrams_of_their_staff() {
    let mut score = chord_document().read();
    let beat = &mut score.tracks[0].staves[0].bars[0].voices[0].beats[0];
    assert_eq!(beat.effects.chord_name.as_deref(), Some("G"));
    assert!(crate::support::beat_payloads(&score).is_empty());
    let bytes = written(&score);
    assert_eq!(read_gp(&bytes).expect("read"), score);
    let beat = &mut score.tracks[0].staves[0].bars[0].voices[0].beats[0];
    beat.effects.chord_name = Some("Am".to_string());
    let bytes = written(&score);
    assert!(gpif_of(&bytes).contains("<Chord><![CDATA[1]]></Chord>"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn every_documented_refusal_is_reachable() {
    let mut text = one_note();
    text.meta.title = "carriage\rreturn".to_string();
    assert_eq!(
        refusal(&text),
        ("meta.title".to_string(), Refusal::Character { code: 13 })
    );
    let mut control = one_note();
    control.tracks[0].name = "nul\u{0}".to_string();
    assert_eq!(
        refusal(&control),
        ("tracks[0].name".to_string(), Refusal::Character { code: 0 })
    );

    let file = |name: &str, data: &[u8]| ContainerEntry::File {
        name: name.to_string(),
        data: Base64Bytes(data.to_vec()),
    };
    let score_entry = |name: &str| ContainerEntry::Score {
        name: name.to_string(),
    };
    let containers = [
        (
            vec![file("VERSION", b"7.0")],
            Refusal::ScoreEntries { count: 0 },
        ),
        (
            vec![score_entry("score.xml")],
            Refusal::ScoreEntryName {
                name: "score.xml".to_string(),
            },
        ),
        (
            vec![score_entry(SCORE_ENTRY), file("a", b""), file("a", b"")],
            Refusal::DuplicateEntry {
                name: "a".to_string(),
            },
        ),
        (
            vec![score_entry(SCORE_ENTRY), file(SCORE_ENTRY, b"")],
            Refusal::EntryName {
                name: SCORE_ENTRY.to_string(),
            },
        ),
        (
            vec![score_entry(SCORE_ENTRY), file("Content/", b"x")],
            Refusal::DirectoryData {
                name: "Content/".to_string(),
            },
        ),
    ];
    for (container, reason) in containers {
        let mut score = one_note();
        score.container = container;
        assert_eq!(refusal(&score).1, reason);
    }

    let mut format = one_note();
    format.document_extensions.push(OpaquePayload {
        format: "musicxml".to_string(),
        data: String::new(),
    });
    assert_eq!(
        refusal(&format).1,
        Refusal::PayloadFormat {
            format: "musicxml".to_string()
        }
    );
    let mut malformed = one_note();
    malformed.document_extensions.push(OpaquePayload {
        format: "gpif".to_string(),
        data: "<Keep index=\"0\"><A/></Keep>".to_string(),
    });
    assert!(matches!(
        refusal(&malformed).1,
        Refusal::MalformedPayload { .. }
    ));
    let mut control = one_note();
    control.document_extensions.push(OpaquePayload {
        format: "gpif".to_string(),
        data: "<Keep parent=\"\" index=\"0\"><A>x\u{1}</A></Keep>".to_string(),
    });
    assert_eq!(
        refusal(&control),
        (
            "document_extensions".to_string(),
            Refusal::Character { code: 1 }
        )
    );
    let mut guarded = one_note();
    let note_id = guarded.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .id
        .0;
    guarded.extensions.0.insert(
        note_id,
        vec![OpaquePayload {
            format: "gpif".to_string(),
            data: "<Keep parent=\"\" index=\"0\" guard=\"clef:G2\"><A/></Keep>".to_string(),
        }],
    );
    assert!(matches!(
        refusal(&guarded),
        (path, Refusal::MalformedPayload { .. })
            if path == "tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]"
    ));
    let mut dangling = one_note();
    dangling.extensions.0.insert(
        opentab_model::ElementId(999),
        vec![OpaquePayload {
            format: "gpif".to_string(),
            data: "<Keep parent=\"\" index=\"0\"><A/></Keep>".to_string(),
        }],
    );
    assert_eq!(refusal(&dangling).1, Refusal::DanglingExtension { id: 999 });

    let mut bars = one_note();
    bars.tracks[0].staves[0].bars.clear();
    assert_eq!(
        refusal(&bars),
        (
            "tracks[0].staves[0]".to_string(),
            Refusal::BarCount {
                expected: 1,
                found: 0
            }
        )
    );
    let mut slots = one_note();
    let voice = slots.tracks[0].staves[0].bars[0].voices[0].clone();
    slots.tracks[0].staves[0].bars[0].voices.push(voice);
    assert_eq!(refusal(&slots).1, Refusal::VoiceSlots);

    let mut kind = one_note();
    kind.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].kind =
        NoteKind::Percussion { articulation: 3 };
    assert_eq!(refusal(&kind).1, Refusal::NoteKind);
    let mut string = one_note();
    string.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].kind = fretted(6, 0);
    assert_eq!(
        refusal(&string).1,
        Refusal::String {
            string: 6,
            strings: 6
        }
    );
    let mut pitch = one_note();
    pitch.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0].kind = fretted(0, 64);
    assert_eq!(refusal(&pitch).1, Refusal::Pitch);

    let mut tempo = one_note();
    tempo.master_bars[0].tempo_changes.push(TempoChange {
        position: Fraction::new(1, 7),
        bpm: Tempo::new(120.0).expect("bpm"),
        unit: TempoUnit::Quarter,
    });
    assert_eq!(
        refusal(&tempo),
        (
            "master_bars[0].tempo_changes[0].position".to_string(),
            Refusal::TempoPosition { numer: 1, denom: 7 }
        )
    );
    let mut late = one_note();
    late.master_bars[0].tempo_changes.push(TempoChange {
        position: Fraction::from_integer(1),
        bpm: Tempo::new(120.0).expect("bpm"),
        unit: TempoUnit::Quarter,
    });
    assert_eq!(
        refusal(&late).1,
        Refusal::TempoPosition { numer: 1, denom: 1 }
    );

    let mut harmonic = one_note();
    harmonic.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .effects
        .harmonic = Some(Harmonic {
        kind: HarmonicKind::Natural,
        fret: Fraction::new(1, 3),
    });
    assert_eq!(
        refusal(&harmonic).1,
        Refusal::HarmonicFret { numer: 1, denom: 3 }
    );
    let mut open = one_note();
    open.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .effects
        .right_hand_finger = Some(Finger::Open);
    assert_eq!(refusal(&open).1, Refusal::RightHandOpen);
    let mut drums = Scratch::new(1);
    drums.track(
        InstrumentKind::Percussion,
        vec![vec![(
            NoteValue::Whole,
            vec![NoteKind::Percussion { articulation: 3 }],
        )]],
    );
    assert_eq!(
        refusal(&drums.score),
        ("tracks[0]".to_string(), Refusal::DrumKit)
    );
    assert!(crate::support::add_drum_kits(&mut drums.score));
    assert_eq!(read_gp(&written(&drums.score)).expect("read"), drums.score);
    let mut bare = one_note();
    bare.tracks.clear();
    assert_eq!(refusal(&bare), ("tracks".to_string(), Refusal::NoTracks));
    let mut barless = one_note();
    barless.master_bars.clear();
    barless.tracks[0].staves[0].bars.clear();
    assert_eq!(
        refusal(&barless),
        ("master_bars".to_string(), Refusal::NoMasterBars)
    );
    barless.tracks.clear();
    assert_eq!(
        refusal(&barless),
        ("master_bars".to_string(), Refusal::NoMasterBars)
    );
    let mut chord = one_note();
    chord.tracks[0].staves[0].bars[0].voices[0].beats[0]
        .effects
        .chord_name = Some("bad\u{1}".to_string());
    assert_eq!(
        refusal(&chord),
        (
            "tracks[0].staves[0].bars[0].voices[0].beats[0].effects.chord_name".to_string(),
            Refusal::Character { code: 1 }
        )
    );
}

#[test]
fn a_replayed_tempo_position_outside_the_bar_is_kept() {
    let doc = Doc {
        master_track: "<Automations><Automation><Type>Tempo</Type><Bar>0</Bar><Position>1</Position><Value>90 2</Value></Automation></Automations>".to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    assert_eq!(
        score.master_bars[0].tempo_changes[0].position,
        Fraction::from_integer(1)
    );
    let bytes = written(&score);
    assert!(gpif_of(&bytes).contains("<Position>1</Position>"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn harmonic_frets_are_written_as_exact_decimals() {
    let mut score = one_note();
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .effects
        .harmonic = Some(Harmonic {
        kind: HarmonicKind::Artificial,
        fret: Fraction::new(1, 1024),
    });
    let bytes = written(&score);
    assert!(gpif_of(&bytes).contains("<HFret>0.0009765625</HFret>"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn a_gp_archive_keeps_its_entries_in_order() {
    let gpif = Doc::default().render();
    let source = crate::support::archive(&[
        ("Content/", b""),
        ("Content/BinaryStylesheet", &[0, 159, 146, 150, 255]),
        (SCORE_ENTRY, gpif.as_bytes()),
        ("VERSION", b"7.0"),
        ("meta.json", b"{}"),
    ]);
    let score = read_gp(&source).expect("read");
    let bytes = written(&score);
    let names: Vec<(String, CompressionMethod)> = entries(&bytes)
        .into_iter()
        .map(|(name, _, method, _)| (name, method))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Content/".to_string(), CompressionMethod::Stored),
            (
                "Content/BinaryStylesheet".to_string(),
                CompressionMethod::Deflated
            ),
            (SCORE_ENTRY.to_string(), CompressionMethod::Deflated),
            ("VERSION".to_string(), CompressionMethod::Deflated),
            ("meta.json".to_string(), CompressionMethod::Deflated),
        ]
    );
    assert_eq!(read_gp(&bytes).expect("read"), score);
    assert_eq!(
        read_gp(&gp(&gpif_of(&bytes))).expect("read").tracks,
        score.tracks
    );
}

#[test]
fn chord_names_without_diagrams_get_empty_diagram_items() {
    let mut score = one_note();
    let mut scratch = Scratch::new(1);
    scratch.track(
        InstrumentKind::Fretted,
        vec![vec![
            (NoteValue::Quarter, vec![fretted(0, 1)]),
            (NoteValue::Quarter, vec![fretted(0, 2)]),
            (NoteValue::Half, vec![fretted(0, 3)]),
        ]],
    );
    score.clone_from(&scratch.score);
    let names = ["Am", "G <&>\"x", "Am"];
    for (beat, name) in score.tracks[0].staves[0].bars[0].voices[0]
        .beats
        .iter_mut()
        .zip(names)
    {
        beat.effects.chord_name = Some(name.to_string());
    }
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    assert!(gpif.contains(concat!(
        "<Property name=\"DiagramCollection\">\n<Items>\n",
        "<Item id=\"0\" name=\"Am\">\n<Diagram stringCount=\"6\" fretCount=\"5\"/>\n</Item>\n",
        "<Item id=\"1\" name=\"G &lt;&amp;&gt;&quot;x\">\n",
        "<Diagram stringCount=\"6\" fretCount=\"5\"/>\n</Item>\n",
        "</Items>\n</Property>"
    )));
    assert!(gpif.contains("<Chord><![CDATA[1]]></Chord>"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

#[test]
fn a_new_chord_name_joins_the_kept_diagram_collection() {
    let mut score = chord_document().read();
    let staff_payloads = crate::support::payloads(&score, score.tracks[0].staves[0].id.0);
    score.tracks[0].staves[0].bars[0].voices[0].beats[0]
        .effects
        .chord_name = Some("C#m7".to_string());
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    assert!(gpif.contains(
        "<Item id=\"1\" name=\"Am\">\n<Diagram/>\n</Item>\n<Item id=\"2\" name=\"C#m7\">\n<Diagram stringCount=\"6\" fretCount=\"5\"/>\n</Item>"
    ));
    assert!(gpif.contains("<Chord><![CDATA[2]]></Chord>"));
    let again = read_gp(&bytes).expect("read");
    assert_eq!(again, score);
    assert_eq!(
        crate::support::payloads(&again, again.tracks[0].staves[0].id.0),
        staff_payloads
    );
}

fn hopo_document() -> String {
    Doc {
        note_properties: "<Property name=\"HopoOrigin\"><Enable/></Property>".to_string(),
        ..Doc::default()
    }
    .render()
    .replace("<Beats>0</Beats>", "<Beats>0 1</Beats>")
    .replace(
        "</Beat></Beats>",
        "</Beat><Beat id=\"1\"><Rhythm ref=\"0\"/><Notes>1</Notes></Beat></Beats>",
    )
    .replace(
        "</Note></Notes>",
        concat!(
            "</Note><Note id=\"1\"><InstrumentArticulation>0</InstrumentArticulation><Properties>",
            "<Property name=\"String\"><String>0</String></Property>",
            "<Property name=\"Fret\"><Fret>5</Fret></Property>",
            "<Property name=\"HopoDestination\"><Enable/></Property>",
            "</Properties></Note></Notes>"
        ),
    )
}

#[test]
fn a_hopo_destination_is_dropped_when_its_origin_is_cleared() {
    let mut score = read_gpif(&hopo_document());
    let destination = &score.tracks[0].staves[0].bars[0].voices[0].beats[1].notes[0];
    assert!(
        crate::support::payloads(&score, destination.id.0)
            .iter()
            .any(|payload| payload.contains("guard=\"hopo:true\""))
    );
    let bytes = written(&score);
    assert!(gpif_of(&bytes).contains("HopoDestination"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
    let origin = &mut score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0];
    origin.effects.hammer_on_pull_off = false;
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    assert!(!gpif.contains("HopoDestination"));
    assert!(!gpif.contains("HopoOrigin"));
    let again = read_gp(&bytes).expect("read");
    assert_eq!(again.tracks, score.tracks);
}

#[test]
fn a_pitched_articulation_is_dropped_when_the_note_becomes_percussion() {
    let mut score = read_gpif(&hopo_document());
    let track = &mut score.tracks[0];
    track.instrument.kind = InstrumentKind::Percussion;
    track.instrument.set = Some(InstrumentSet::DrumKit);
    for beat in &mut track.staves[0].bars[0].voices[0].beats {
        for note in &mut beat.notes {
            note.kind = NoteKind::Percussion { articulation: 7 };
        }
    }
    assert_eq!(refusal(&score), ("tracks[0]".to_string(), Refusal::DrumKit));
    assert!(crate::support::add_drum_kits(&mut score));
    let bytes = written(&score);
    let gpif = gpif_of(&bytes);
    assert_eq!(gpif.matches("<InstrumentArticulation>").count(), 2);
    assert!(!gpif.contains("<InstrumentArticulation>0</InstrumentArticulation>"));
    let again = read_gp(&bytes).expect("read");
    assert_eq!(again.tracks, score.tracks);
}

#[test]
fn a_hopo_origin_in_the_previous_bar_guards_the_destination() {
    let mut scratch = Scratch::new(2);
    scratch.track(
        InstrumentKind::Fretted,
        vec![
            vec![(NoteValue::Whole, vec![fretted(5, 3)])],
            vec![(NoteValue::Whole, vec![fretted(5, 5)])],
        ],
    );
    let mut score = scratch.score;
    let destination = score.tracks[0].staves[0].bars[1].voices[0].beats[0].notes[0]
        .id
        .0;
    score.tracks[0].staves[0].bars[0].voices[0].beats[0].notes[0]
        .effects
        .hammer_on_pull_off = true;
    score.extensions.0.insert(
        destination,
        vec![OpaquePayload {
            format: "gpif".to_string(),
            data: "<Keep parent=\"Properties\" index=\"5\" guard=\"hopo:true\"><Property name=\"HopoDestination\"><Enable/></Property></Keep>".to_string(),
        }],
    );
    let bytes = written(&score);
    assert!(gpif_of(&bytes).contains("HopoDestination"));
    assert_eq!(read_gp(&bytes).expect("read"), score);
}

fn central_directory(bytes: &[u8]) -> Vec<(u8, u8, u32)> {
    let mut records = Vec::new();
    let mut at = 0;
    while at + 46 <= bytes.len() {
        if bytes[at..at + 4] == [0x50, 0x4b, 0x01, 0x02] {
            let attributes = u32::from_le_bytes([
                bytes[at + 38],
                bytes[at + 39],
                bytes[at + 40],
                bytes[at + 41],
            ]);
            records.push((bytes[at + 5], bytes[at + 4], attributes));
            at += 46;
        } else {
            at += 1;
        }
    }
    records
}

#[test]
fn central_directory_system_and_attributes_are_pinned() {
    let gpif = Doc::default().render();
    let source = crate::support::archive(&[
        ("Content/", b""),
        (SCORE_ENTRY, gpif.as_bytes()),
        ("VERSION", b"7.0"),
    ]);
    let bytes = written(&read_gp(&source).expect("read"));
    let file = (0o100_644u32) << 16;
    let directory = (0o040_755u32) << 16;
    let records = central_directory(&bytes);
    let systems: Vec<(u8, u32)> = records
        .iter()
        .map(|(system, _, attributes)| (*system, *attributes))
        .collect();
    assert_eq!(systems, vec![(3, directory), (3, file), (3, file)]);
    let scratch = central_directory(&written(&one_note()));
    assert!(
        scratch
            .iter()
            .all(|(system, _, attributes)| *system == 3 && *attributes == file)
    );
}

#[test]
fn a_kept_gp_version_replaces_the_generated_one() {
    let score = read_gpif(
        &Doc {
            root: "<GPVersion>7.6.0</GPVersion>".to_string(),
            ..Doc::default()
        }
        .render(),
    );
    let gpif = gpif_of(&written(&score));
    assert_eq!(gpif.matches("<GPVersion>").count(), 1);
    assert!(gpif.contains("<GPVersion>7.6.0</GPVersion>"));
    assert!(!gpif.contains("8.1.5"));
    assert_eq!(read_gp(&written(&score)).expect("read"), score);
    let fresh = gpif_of(&written(&one_note()));
    assert_eq!(fresh.matches("<GPVersion>8.1.5</GPVersion>").count(), 1);
}

#[test]
fn an_empty_drum_kit_is_refused() {
    let mut drums = Scratch::new(1);
    drums.track(
        InstrumentKind::Percussion,
        vec![vec![(
            NoteValue::Whole,
            vec![NoteKind::Percussion { articulation: 3 }],
        )]],
    );
    let id = drums.score.tracks[0].id.0;
    drums.score.extensions.0.insert(
        id,
        vec![OpaquePayload {
            format: "gpif".to_string(),
            data: "<Keep parent=\"InstrumentSet\" index=\"1\"><Elements/></Keep>".to_string(),
        }],
    );
    assert_eq!(
        refusal(&drums.score),
        ("tracks[0]".to_string(), Refusal::DrumKit)
    );
    drums.score.extensions.0.clear();
    assert!(crate::support::add_drum_kits(&mut drums.score));
    assert_eq!(read_gp(&written(&drums.score)).expect("read"), drums.score);
}
