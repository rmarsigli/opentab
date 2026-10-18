use opentab_model::{
    Bar, Beat, BeatId, Duration, ElementId, MasterBar, Note, NoteId, OpaquePayload, StaffId,
    TempoChange, TimeSignature, TrackId, VoiceId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaField {
    Title,
    Subtitle,
    Artist,
    Album,
    Words,
    Music,
    Copyright,
    TabAuthor,
    Instructions,
    Notices,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionEntry {
    pub id: ElementId,
    pub payloads: Vec<OpaquePayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SetMeta {
        field: MetaField,
        value: String,
    },
    SetAnacrusis {
        value: bool,
    },
    InsertMasterBar {
        index: usize,
        time_signature: TimeSignature,
    },
    RemoveMasterBar {
        index: usize,
    },
    InsertBeat {
        voice: VoiceId,
        index: usize,
        duration: Duration,
    },
    RemoveBeat {
        beat: BeatId,
    },
    SetBeatDuration {
        beat: BeatId,
        duration: Duration,
    },
    AddNote {
        beat: BeatId,
        string: u8,
        fret: u8,
    },
    RemoveNote {
        note: NoteId,
    },
    SetFret {
        note: NoteId,
        fret: u8,
    },
    SetTempo {
        master_bar: usize,
        bpm: f64,
    },
    PadStaffBars {
        track: TrackId,
        staff: StaffId,
    },
    TrimStaffBars {
        track: TrackId,
        staff: StaffId,
        len: usize,
    },
    RestoreMasterBar {
        index: usize,
        master_bar: MasterBar,
        bars: Vec<Bar>,
        extensions: Vec<ExtensionEntry>,
    },
    RestoreBeat {
        voice: VoiceId,
        index: usize,
        beat: Beat,
        extensions: Vec<ExtensionEntry>,
    },
    RestoreNote {
        beat: BeatId,
        index: usize,
        note: Note,
        extensions: Vec<ExtensionEntry>,
    },
    RestoreTempoChanges {
        master_bar: usize,
        tempo_changes: Vec<TempoChange>,
    },
    RestoreStaffBars {
        track: TrackId,
        staff: StaffId,
        bars: Vec<Bar>,
        extensions: Vec<ExtensionEntry>,
    },
}
