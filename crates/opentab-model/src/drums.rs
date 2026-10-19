use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::ModelError;

const MAX_KEY: u8 = 127;
pub const MAX_DRUM_BEAT_NOTES: usize = 6;
const PLAIN_HEADS: [&str; 3] = ["noteheadBlack", "noteheadHalf", "noteheadWhole"];
const CROSS_HEADS: [&str; 3] = ["noteheadXBlack", "noteheadXHalf", "noteheadXWhole"];
const KICK: &str = "kickDrum";
const SNARE: &str = "snare";
const HI_HAT: &str = "hiHat";
const TOM: &str = "tom";
const RIDE: &str = "ride";
const CRASH: &str = "crash";
const UNKNOWN: &str = "unknown";
const SECOND_LEDGER_ABOVE: i8 = -4;
const SPACE_ABOVE_FIRST_LEDGER: i8 = -3;
const FIRST_LEDGER_ABOVE: i8 = -2;
const SPACE_ABOVE_STAFF: i8 = -1;
const TOP_LINE: i8 = 0;
const TOP_SPACE: i8 = 1;
const SECOND_LINE: i8 = 2;
const SECOND_SPACE: i8 = 3;
const MIDDLE_LINE: i8 = 4;
const THIRD_SPACE: i8 = 5;
const BOTTOM_SPACE: i8 = 7;
const SPACE_BELOW_STAFF: i8 = 9;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechniquePlacement {
    #[default]
    Outside,
    Inside,
    Above,
    Below,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrumArticulation {
    pub name: String,
    pub staff_line: i8,
    pub noteheads: [String; 3],
    pub technique_placement: TechniquePlacement,
    pub technique_symbol: String,
    pub input_keys: Vec<u8>,
    pub rse_sound: String,
    pub output_key: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrumElement {
    pub name: String,
    pub kind: String,
    pub soundbank: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "DrumKitRepr")]
pub struct DrumKit {
    elements: Vec<DrumElement>,
    articulations: Vec<DrumArticulation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrumKitRepr {
    elements: Vec<DrumElement>,
    articulations: Vec<DrumArticulation>,
}

impl TryFrom<DrumKitRepr> for DrumKit {
    type Error = ModelError;

    fn try_from(repr: DrumKitRepr) -> Result<Self, Self::Error> {
        DrumKit::new(repr.elements, repr.articulations)
    }
}

fn check_key(key: u8) -> Result<(), ModelError> {
    if key > MAX_KEY {
        return Err(ModelError::InvalidDrumKey { key });
    }
    Ok(())
}

fn check_notehead(name: &str) -> Result<(), ModelError> {
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return Err(ModelError::InvalidNotehead {
            name: name.to_string(),
        });
    }
    Ok(())
}

impl DrumKit {
    pub fn new(
        elements: Vec<DrumElement>,
        articulations: Vec<DrumArticulation>,
    ) -> Result<DrumKit, ModelError> {
        let counted = elements
            .iter()
            .try_fold(0usize, |total, element| total.checked_add(element.count));
        if counted != Some(articulations.len()) {
            return Err(ModelError::DrumKitCount {
                articulations: articulations.len(),
            });
        }
        for articulation in &articulations {
            check_key(articulation.output_key)?;
            for key in &articulation.input_keys {
                check_key(*key)?;
            }
            for name in &articulation.noteheads {
                check_notehead(name)?;
            }
        }
        Ok(DrumKit {
            elements,
            articulations,
        })
    }

    pub fn elements(&self) -> &[DrumElement] {
        &self.elements
    }

    pub fn articulations(&self) -> &[DrumArticulation] {
        &self.articulations
    }

    pub fn key_of(&self, articulation: u16) -> Option<u8> {
        self.articulations
            .get(usize::from(articulation))
            .map(|found| found.output_key)
    }
}

struct GeneralDrum {
    key: u8,
    name: &'static str,
    kind: &'static str,
    staff_line: i8,
    heads: [&'static str; 3],
}

const fn drum(
    key: u8,
    name: &'static str,
    kind: &'static str,
    staff_line: i8,
    heads: [&'static str; 3],
) -> GeneralDrum {
    GeneralDrum {
        key,
        name,
        kind,
        staff_line,
        heads,
    }
}

const GENERAL_MIDI_DRUMS: [GeneralDrum; 47] = [
    drum(35, "Acoustic Bass Drum", KICK, BOTTOM_SPACE, PLAIN_HEADS),
    drum(36, "Bass Drum 1", KICK, BOTTOM_SPACE, PLAIN_HEADS),
    drum(37, "Side Stick", SNARE, SECOND_SPACE, CROSS_HEADS),
    drum(38, "Acoustic Snare", SNARE, SECOND_SPACE, PLAIN_HEADS),
    drum(39, "Hand Clap", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(40, "Electric Snare", SNARE, SECOND_SPACE, PLAIN_HEADS),
    drum(41, "Low Floor Tom", TOM, THIRD_SPACE, PLAIN_HEADS),
    drum(42, "Closed Hi-Hat", HI_HAT, SPACE_ABOVE_STAFF, CROSS_HEADS),
    drum(43, "High Floor Tom", TOM, THIRD_SPACE, PLAIN_HEADS),
    drum(44, "Pedal Hi-Hat", HI_HAT, SPACE_BELOW_STAFF, CROSS_HEADS),
    drum(45, "Low Tom", TOM, SECOND_LINE, PLAIN_HEADS),
    drum(46, "Open Hi-Hat", HI_HAT, SPACE_ABOVE_STAFF, CROSS_HEADS),
    drum(47, "Low-Mid Tom", TOM, SECOND_LINE, PLAIN_HEADS),
    drum(48, "Hi-Mid Tom", TOM, TOP_SPACE, PLAIN_HEADS),
    drum(49, "Crash Cymbal 1", CRASH, FIRST_LEDGER_ABOVE, CROSS_HEADS),
    drum(50, "High Tom", TOM, TOP_SPACE, PLAIN_HEADS),
    drum(51, "Ride Cymbal 1", RIDE, TOP_LINE, CROSS_HEADS),
    drum(
        52,
        "Chinese Cymbal",
        UNKNOWN,
        SECOND_LEDGER_ABOVE,
        CROSS_HEADS,
    ),
    drum(53, "Ride Bell", RIDE, TOP_LINE, CROSS_HEADS),
    drum(54, "Tambourine", UNKNOWN, MIDDLE_LINE, CROSS_HEADS),
    drum(
        55,
        "Splash Cymbal",
        UNKNOWN,
        SPACE_ABOVE_FIRST_LEDGER,
        CROSS_HEADS,
    ),
    drum(56, "Cowbell", UNKNOWN, TOP_SPACE, CROSS_HEADS),
    drum(57, "Crash Cymbal 2", CRASH, FIRST_LEDGER_ABOVE, CROSS_HEADS),
    drum(58, "Vibraslap", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(59, "Ride Cymbal 2", RIDE, TOP_LINE, CROSS_HEADS),
    drum(60, "Hi Bongo", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(61, "Low Bongo", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(62, "Mute Hi Conga", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(63, "Open Hi Conga", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(64, "Low Conga", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(65, "High Timbale", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(66, "Low Timbale", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(67, "High Agogo", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(68, "Low Agogo", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(69, "Cabasa", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(70, "Maracas", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(71, "Short Whistle", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(72, "Long Whistle", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(73, "Short Guiro", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(74, "Long Guiro", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(75, "Claves", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(76, "Hi Wood Block", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(77, "Low Wood Block", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(78, "Mute Cuica", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(79, "Open Cuica", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(80, "Mute Triangle", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
    drum(81, "Open Triangle", UNKNOWN, MIDDLE_LINE, PLAIN_HEADS),
];

static GENERAL_MIDI_KIT: OnceLock<DrumKit> = OnceLock::new();

pub fn gm_drum_kit() -> DrumKit {
    general_midi_kit().clone()
}

pub(crate) fn general_midi_kit() -> &'static DrumKit {
    GENERAL_MIDI_KIT.get_or_init(build_general_midi_kit)
}

fn build_general_midi_kit() -> DrumKit {
    let elements = GENERAL_MIDI_DRUMS
        .iter()
        .map(|drum| DrumElement {
            name: drum.name.to_string(),
            kind: drum.kind.to_string(),
            soundbank: String::new(),
            count: 1,
        })
        .collect();
    let articulations = GENERAL_MIDI_DRUMS
        .iter()
        .map(|drum| DrumArticulation {
            name: drum.name.to_string(),
            staff_line: drum.staff_line,
            noteheads: drum.heads.map(str::to_string),
            technique_placement: TechniquePlacement::Outside,
            technique_symbol: String::new(),
            input_keys: vec![drum.key],
            rse_sound: String::new(),
            output_key: drum.key,
        })
        .collect();
    DrumKit {
        elements,
        articulations,
    }
}
