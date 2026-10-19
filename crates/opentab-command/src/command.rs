use opentab_model::{
    Automation, Bar, BarId, Beat, BeatId, Directions, DrumKit, Duration, ElementId, MasterBar,
    Note, NoteId, OpaquePayload, SoundOverride, StaffId, TempoChange, TimeSignature, TrackId,
    TrackMidi, TremoloBar, Trill, Voice, VoiceId,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationEntry {
    pub track: TrackId,
    pub index: usize,
    pub automation: Automation,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voice: Option<VoiceId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bar: Option<BarId>,
        index: usize,
        duration: Duration,
    },
    RemoveBeat {
        beat: BeatId,
    },
    RemoveVoice {
        voice: VoiceId,
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
    SetTremoloBar {
        beat: BeatId,
        tremolo_bar: Option<TremoloBar>,
    },
    SetTrill {
        note: NoteId,
        trill: Option<Trill>,
    },
    SetTempo {
        master_bar: usize,
        bpm: f64,
    },
    SetTempoLinear {
        master_bar: usize,
        index: usize,
        linear: bool,
    },
    SetDirections {
        master_bar: usize,
        directions: Directions,
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
    SetTrackMidi {
        track: TrackId,
        midi: TrackMidi,
    },
    SetDrumKit {
        track: TrackId,
        drum_kit: Option<DrumKit>,
    },
    InsertAutomation {
        track: TrackId,
        index: usize,
        automation: Automation,
    },
    RemoveAutomation {
        track: TrackId,
        index: usize,
    },
    SetSoundOverride {
        track: TrackId,
        bank: Option<String>,
    },
    RestoreSoundOverrides {
        sound_overrides: Vec<SoundOverride>,
    },
    RestoreMasterBar {
        index: usize,
        master_bar: MasterBar,
        bars: Vec<Bar>,
        extensions: Vec<ExtensionEntry>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        automations: Vec<AutomationEntry>,
    },
    RestoreBeat {
        voice: VoiceId,
        index: usize,
        beat: Beat,
        extensions: Vec<ExtensionEntry>,
    },
    RestoreVoice {
        bar: BarId,
        index: usize,
        voice: Voice,
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
