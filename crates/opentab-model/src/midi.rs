use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::fraction::deserialize_non_negative;
use crate::{Fraction, InstrumentKind, MasterBarId, ModelError, TrackId, WholeNotes};

const MAX_CHANNEL: u8 = 15;
const DRUM_CHANNEL: u8 = 9;
const MAX_BANK: u8 = 127;
const MILLIONTHS: u32 = 1_000_000;
const HALF: u32 = 500_000;
const PROGRAMS_PER_FAMILY: u8 = 8;
pub const MAX_PROGRAM: u8 = 127;
pub const AUTOMATION_GRID: i64 = 3840;

pub(crate) fn deserialize_program<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let program = u8::deserialize(deserializer)?;
    check_program(program).map_err(D::Error::custom)
}

pub fn check_program(program: u8) -> Result<u8, ModelError> {
    if program > MAX_PROGRAM {
        return Err(ModelError::InvalidProgram { program });
    }
    Ok(program)
}

pub fn automations_in_order(automations: &[Automation]) -> bool {
    let first_sound = automations
        .iter()
        .position(|automation| matches!(automation.value, AutomationValue::Sound(_)));
    first_sound.is_none_or(|first| {
        automations
            .iter()
            .skip(first)
            .all(|automation| matches!(automation.value, AutomationValue::Sound(_)))
    })
}

pub fn on_automation_grid(position: Fraction) -> bool {
    (position * AUTOMATION_GRID).is_integer()
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "u8", into = "u8")]
pub struct MidiChannel(u8);

impl MidiChannel {
    pub const DRUMS: MidiChannel = MidiChannel(DRUM_CHANNEL);

    pub fn new(channel: u8) -> Result<MidiChannel, ModelError> {
        if channel > MAX_CHANNEL {
            return Err(ModelError::InvalidMidiChannel { channel });
        }
        Ok(MidiChannel(channel))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for MidiChannel {
    type Error = ModelError;

    fn try_from(channel: u8) -> Result<Self, Self::Error> {
        MidiChannel::new(channel)
    }
}

impl From<MidiChannel> for u8 {
    fn from(channel: MidiChannel) -> Self {
        channel.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "MidiBankRepr")]
pub struct MidiBank {
    msb: u8,
    lsb: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MidiBankRepr {
    msb: u8,
    lsb: u8,
}

impl MidiBank {
    pub fn new(msb: u8, lsb: u8) -> Result<MidiBank, ModelError> {
        if msb > MAX_BANK || lsb > MAX_BANK {
            return Err(ModelError::InvalidMidiBank { msb, lsb });
        }
        Ok(MidiBank { msb, lsb })
    }

    pub fn msb(self) -> u8 {
        self.msb
    }

    pub fn lsb(self) -> u8 {
        self.lsb
    }
}

impl TryFrom<MidiBankRepr> for MidiBank {
    type Error = ModelError;

    fn try_from(repr: MidiBankRepr) -> Result<Self, Self::Error> {
        MidiBank::new(repr.msb, repr.lsb)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Level(u32);

impl Level {
    pub const HALF: Level = Level(HALF);
    pub const FULL: Level = Level(MILLIONTHS);

    pub fn new(millionths: u32) -> Result<Level, ModelError> {
        if millionths > MILLIONTHS {
            return Err(ModelError::InvalidLevel { millionths });
        }
        Ok(Level(millionths))
    }

    pub fn millionths(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for Level {
    type Error = ModelError;

    fn try_from(millionths: u32) -> Result<Self, Self::Error> {
        Level::new(millionths)
    }
}

impl From<Level> for u32 {
    fn from(level: Level) -> Self {
        level.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Balance(u32);

impl Balance {
    pub const LEFT: Balance = Balance(0);
    pub const CENTRE: Balance = Balance(HALF);
    pub const RIGHT: Balance = Balance(MILLIONTHS);

    pub fn new(millionths: u32) -> Result<Balance, ModelError> {
        if millionths > MILLIONTHS {
            return Err(ModelError::InvalidBalance { millionths });
        }
        Ok(Balance(millionths))
    }

    pub fn millionths(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for Balance {
    type Error = ModelError;

    fn try_from(millionths: u32) -> Result<Self, Self::Error> {
        Balance::new(millionths)
    }
}

impl From<Balance> for u32 {
    fn from(balance: Balance) -> Self {
        balance.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackMidi {
    pub port: u8,
    pub primary: MidiChannel,
    pub secondary: MidiChannel,
    pub per_string: bool,
    pub bank: MidiBank,
    pub volume: Level,
    pub pan: Balance,
}

impl TrackMidi {
    pub fn default_for(kind: InstrumentKind) -> TrackMidi {
        let channel = match kind {
            InstrumentKind::Fretted => MidiChannel::default(),
            InstrumentKind::Percussion => MidiChannel::DRUMS,
        };
        TrackMidi {
            port: 0,
            primary: channel,
            secondary: channel,
            per_string: false,
            bank: MidiBank::default(),
            volume: Level::HALF,
            pan: Balance::CENTRE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sound {
    pub name: String,
    #[serde(deserialize_with = "deserialize_program")]
    pub program: u8,
    pub bank: MidiBank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SoundRef(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AutomationValue {
    Volume(Level),
    Pan(Balance),
    Sound(SoundRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Automation {
    pub master_bar: MasterBarId,
    #[serde(deserialize_with = "deserialize_non_negative")]
    pub position: WholeNotes,
    pub linear: bool,
    pub visible: bool,
    pub value: AutomationValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundOverride {
    pub track: TrackId,
    pub bank: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentSet {
    ElectricGuitar,
    SteelGuitar,
    NylonGuitar,
    ElectricBass,
    AcousticBass,
    Violin,
    AcousticPiano,
    Saxophone,
    Trumpet,
    ElectricOrgan,
    Voice,
    DrumKit,
}

impl InstrumentSet {
    pub fn kind(self) -> InstrumentKind {
        match self {
            InstrumentSet::DrumKit => InstrumentKind::Percussion,
            _ => InstrumentKind::Fretted,
        }
    }

    pub fn family(self) -> InstrumentFamily {
        match self {
            InstrumentSet::ElectricGuitar
            | InstrumentSet::SteelGuitar
            | InstrumentSet::NylonGuitar => InstrumentFamily::Guitar,
            InstrumentSet::ElectricBass | InstrumentSet::AcousticBass => InstrumentFamily::Bass,
            InstrumentSet::Violin => InstrumentFamily::Strings,
            InstrumentSet::AcousticPiano => InstrumentFamily::Piano,
            InstrumentSet::Saxophone => InstrumentFamily::Reed,
            InstrumentSet::Trumpet => InstrumentFamily::Brass,
            InstrumentSet::ElectricOrgan => InstrumentFamily::Organ,
            InstrumentSet::Voice => InstrumentFamily::Ensemble,
            InstrumentSet::DrumKit => InstrumentFamily::Drums,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentFamily {
    Piano,
    ChromaticPercussion,
    Organ,
    Guitar,
    Bass,
    Strings,
    Ensemble,
    Brass,
    Reed,
    Pipe,
    SynthLead,
    SynthPad,
    SynthEffects,
    Ethnic,
    Percussive,
    SoundEffects,
    Drums,
}

const PROGRAM_FAMILIES: [InstrumentFamily; 16] = [
    InstrumentFamily::Piano,
    InstrumentFamily::ChromaticPercussion,
    InstrumentFamily::Organ,
    InstrumentFamily::Guitar,
    InstrumentFamily::Bass,
    InstrumentFamily::Strings,
    InstrumentFamily::Ensemble,
    InstrumentFamily::Brass,
    InstrumentFamily::Reed,
    InstrumentFamily::Pipe,
    InstrumentFamily::SynthLead,
    InstrumentFamily::SynthPad,
    InstrumentFamily::SynthEffects,
    InstrumentFamily::Ethnic,
    InstrumentFamily::Percussive,
    InstrumentFamily::SoundEffects,
];

fn family_set(family: InstrumentFamily) -> Option<InstrumentSet> {
    match family {
        InstrumentFamily::Piano => Some(InstrumentSet::AcousticPiano),
        InstrumentFamily::Organ => Some(InstrumentSet::ElectricOrgan),
        InstrumentFamily::Guitar => Some(InstrumentSet::ElectricGuitar),
        InstrumentFamily::Bass => Some(InstrumentSet::ElectricBass),
        InstrumentFamily::Strings => Some(InstrumentSet::Violin),
        InstrumentFamily::Ensemble => Some(InstrumentSet::Voice),
        InstrumentFamily::Brass => Some(InstrumentSet::Trumpet),
        InstrumentFamily::Reed => Some(InstrumentSet::Saxophone),
        InstrumentFamily::Drums => Some(InstrumentSet::DrumKit),
        InstrumentFamily::ChromaticPercussion
        | InstrumentFamily::Pipe
        | InstrumentFamily::SynthLead
        | InstrumentFamily::SynthPad
        | InstrumentFamily::SynthEffects
        | InstrumentFamily::Ethnic
        | InstrumentFamily::Percussive
        | InstrumentFamily::SoundEffects => None,
    }
}

impl InstrumentSet {
    pub fn derived(kind: InstrumentKind, program: u8) -> InstrumentSet {
        let fallback = match kind {
            InstrumentKind::Fretted => InstrumentSet::ElectricGuitar,
            InstrumentKind::Percussion => InstrumentSet::DrumKit,
        };
        let family = InstrumentFamily::of(kind, None, program);
        family_set(family)
            .filter(|set| set.kind() == kind)
            .unwrap_or(fallback)
    }
}

impl InstrumentFamily {
    pub fn of(kind: InstrumentKind, set: Option<InstrumentSet>, program: u8) -> InstrumentFamily {
        match set {
            Some(set) => set.family(),
            None if kind == InstrumentKind::Percussion => InstrumentFamily::Drums,
            None => InstrumentFamily::of_program(program),
        }
    }

    pub fn of_program(program: u8) -> InstrumentFamily {
        PROGRAM_FAMILIES
            .get(usize::from(program / PROGRAMS_PER_FAMILY))
            .copied()
            .unwrap_or(InstrumentFamily::SoundEffects)
    }
}
