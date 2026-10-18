use std::io::{Cursor, Write};

use opentab_gp::{GpError, Refusal, read_gp};
use opentab_model::{Beat, ElementId, MasterBar, Note, OpaquePayload, Score, Track};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub const SCORE_ENTRY: &str = "Content/score.gpif";

pub struct Doc {
    pub root: String,
    pub score: String,
    pub master_track: String,
    pub instrument: String,
    pub track: String,
    pub tuning: String,
    pub staff: String,
    pub time: String,
    pub master_bar: String,
    pub bar: String,
    pub beat: String,
    pub note: String,
    pub string: String,
    pub note_properties: String,
    pub rhythm: String,
}

impl Default for Doc {
    fn default() -> Self {
        Doc {
            root: String::new(),
            score: String::new(),
            master_track: String::new(),
            instrument: "electricGuitar".to_string(),
            track: String::new(),
            tuning: "40 45 50 55 59 64".to_string(),
            staff: String::new(),
            time: "4/4".to_string(),
            master_bar: String::new(),
            bar: String::new(),
            beat: String::new(),
            note: String::new(),
            string: "0".to_string(),
            note_properties: String::new(),
            rhythm: String::new(),
        }
    }
}

impl Doc {
    pub fn render(&self) -> String {
        format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<GPIF>\n",
                "<GPRevision>13008</GPRevision>\n{root}",
                "<Score><Title><![CDATA[Song]]></Title>{score}</Score>\n",
                "<MasterTrack><Tracks>0</Tracks>{master_track}</MasterTrack>\n",
                "<Tracks><Track id=\"0\"><Name><![CDATA[Guitar]]></Name>",
                "<ShortName><![CDATA[gtr]]></ShortName>",
                "<InstrumentSet><Type>{instrument}</Type></InstrumentSet>{track}",
                "<Staves><Staff><Properties><Property name=\"Tuning\"><Pitches>{tuning}</Pitches>",
                "</Property>{staff}</Properties></Staff></Staves></Track></Tracks>\n",
                "<MasterBars><MasterBar><Key><AccidentalCount>0</AccidentalCount><Mode>Major</Mode>",
                "</Key><Time>{time}</Time>{master_bar}<Bars>0</Bars></MasterBar></MasterBars>\n",
                "<Bars><Bar id=\"0\"><Clef>G2</Clef><Voices>0 -1 -1 -1</Voices>{bar}</Bar></Bars>\n",
                "<Voices><Voice id=\"0\"><Beats>0</Beats></Voice></Voices>\n",
                "<Beats><Beat id=\"0\"><Rhythm ref=\"0\"/>{beat}<Notes>0</Notes></Beat></Beats>\n",
                "<Notes><Note id=\"0\">{note}<Properties><Property name=\"String\">",
                "<String>{string}</String></Property><Property name=\"Fret\"><Fret>3</Fret>",
                "</Property>{note_properties}</Properties></Note></Notes>\n",
                "<Rhythms><Rhythm id=\"0\"><NoteValue>Whole</NoteValue>{rhythm}</Rhythm></Rhythms>\n",
                "</GPIF>\n"
            ),
            root = self.root,
            score = self.score,
            master_track = self.master_track,
            instrument = self.instrument,
            track = self.track,
            tuning = self.tuning,
            staff = self.staff,
            time = self.time,
            master_bar = self.master_bar,
            bar = self.bar,
            beat = self.beat,
            note = self.note,
            string = self.string,
            note_properties = self.note_properties,
            rhythm = self.rhythm,
        )
    }

    pub fn read(&self) -> Score {
        read_gpif(&self.render())
    }

    pub fn error(&self) -> GpError {
        read_gpif_error(&self.render())
    }
}

pub fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, data) in entries {
        if name.ends_with('/') {
            writer.add_directory(*name, options).expect("directory");
        } else {
            writer.start_file(*name, options).expect("entry");
            writer.write_all(data).expect("data");
        }
    }
    writer.finish().expect("archive").into_inner()
}

pub fn gp(gpif: &str) -> Vec<u8> {
    archive(&[("VERSION", b"7.0"), (SCORE_ENTRY, gpif.as_bytes())])
}

pub fn read_gpif(gpif: &str) -> Score {
    match read_gp(&gp(gpif)) {
        Ok(score) => score,
        Err(error) => panic!("read failed: {error}"),
    }
}

pub fn read_gpif_error(gpif: &str) -> GpError {
    match read_gp(&gp(gpif)) {
        Ok(_) => panic!("read unexpectedly succeeded"),
        Err(error) => error,
    }
}

pub fn payloads(score: &Score, id: ElementId) -> Vec<String> {
    score
        .extensions
        .0
        .get(&id)
        .map(|payloads| payloads.iter().map(data_of).collect())
        .unwrap_or_default()
}

pub fn document_payloads(score: &Score) -> Vec<String> {
    score.document_extensions.iter().map(data_of).collect()
}

fn data_of(payload: &OpaquePayload) -> String {
    assert_eq!(payload.format, "gpif");
    payload.data.clone()
}

pub fn track(score: &Score) -> &Track {
    &score.tracks[0]
}

pub fn master_bar(score: &Score) -> &MasterBar {
    &score.master_bars[0]
}

pub fn beat(score: &Score) -> &Beat {
    &score.tracks[0].staves[0].bars[0].voices[0].beats[0]
}

pub fn note(score: &Score) -> &Note {
    &beat(score).notes[0]
}

pub fn note_payloads(score: &Score) -> Vec<String> {
    payloads(score, note(score).id.0)
}

pub fn beat_payloads(score: &Score) -> Vec<String> {
    payloads(score, beat(score).id.0)
}

pub fn gpif_path(error: &GpError) -> String {
    match error {
        GpError::Gpif { path, .. } => path.clone(),
        other => panic!("not a GPIF error: {other}"),
    }
}

pub fn documented(reason: &Refusal) -> bool {
    matches!(
        reason,
        Refusal::Character { .. }
            | Refusal::ScoreEntries { .. }
            | Refusal::ScoreEntryName { .. }
            | Refusal::DuplicateEntry { .. }
            | Refusal::EntryName { .. }
            | Refusal::DirectoryData { .. }
            | Refusal::PayloadFormat { .. }
            | Refusal::MalformedPayload { .. }
            | Refusal::DanglingExtension { .. }
            | Refusal::BarCount { .. }
            | Refusal::VoiceSlots
            | Refusal::NoteKind
            | Refusal::String { .. }
            | Refusal::Pitch
            | Refusal::TempoPosition { .. }
            | Refusal::HarmonicFret { .. }
            | Refusal::RightHandOpen
    )
}
