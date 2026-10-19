use opentab_model::{
    Accent, Bar, Beat, Bend, BendPoint, Clef, Directions, DrumKit, Dynamic, Finger, Fraction,
    Grace, Harmonic, InstrumentKind, InstrumentSet, NoteKind, Pitch, Slide, Stroke, Tap,
    TimeSignature, TrackMidi, Transpose, TremoloBar, TremoloPicking, Trill, TripletFeel, Vibrato,
    VoiceSlot,
};

use crate::canon;
use crate::codes::{
    CLEFS, DIRECTION_JUMPS, DIRECTION_TARGETS, DYNAMICS, FINGERS, GRACES, HARMONICS,
    INSTRUMENT_SETS, NOTE_VALUES, STROKE_DIRECTIONS, TREMOLOS, TRIPLET_FEELS, VIBRATOS, name_of,
    slide_bits,
};

use crate::dom::write_element;

const NONE: &str = "none";

pub(crate) struct PitchInputs<'a> {
    pub kind: NoteKind,
    pub tuning: &'a [Pitch],
    pub capo: u8,
    pub transpose: Transpose,
}

pub(crate) fn pitch(inputs: &PitchInputs<'_>) -> String {
    match inputs.kind {
        NoteKind::Fretted { string, fret } => {
            let tuning: Vec<String> = inputs
                .tuning
                .iter()
                .map(|pitch| pitch.0.to_string())
                .collect();
            format!(
                "pitch:fretted {string} {fret};tuning {};capo {};transpose {} {}",
                tuning.join(" "),
                inputs.capo,
                inputs.transpose.chromatic(),
                inputs.transpose.octave()
            )
        }
        NoteKind::Percussion { articulation } => format!("pitch:percussion {articulation}"),
    }
}

pub(crate) fn carried(bar: Option<&Bar>, slot: VoiceSlot) -> Option<&Beat> {
    bar?.voices
        .iter()
        .find(|voice| voice.slot == slot)
        .and_then(|voice| voice.beats.last())
}

pub(crate) fn hopo(previous: Option<&Beat>, kind: NoteKind) -> String {
    let origin = match kind {
        NoteKind::Fretted { string, .. } => previous.and_then(|beat| {
            beat.notes
                .iter()
                .find(|note| {
                    matches!(note.kind, NoteKind::Fretted { string: other, .. } if other == string)
                })
                .map(|note| note.effects.hammer_on_pull_off)
        }),
        NoteKind::Percussion { .. } => None,
    };
    match origin {
        Some(origin) => format!("hopo:{origin}"),
        None => format!("hopo:{NONE}"),
    }
}

pub(crate) fn note_kind(kind: NoteKind) -> String {
    match kind {
        NoteKind::Fretted { .. } => "note-kind:fretted".to_string(),
        NoteKind::Percussion { .. } => "note-kind:percussion".to_string(),
    }
}

const TEMPO_POSITION: &str = "tempo-position:";
const TEMPO_LINEAR: &str = "tempo-linear:";

fn tempo_anchor_of(position: Fraction, time: TimeSignature) -> String {
    format!(
        "{}/{};time {}/{}",
        position.numer(),
        position.denom(),
        time.numerator(),
        time.denominator()
    )
}

pub(crate) fn tempo_position(position: Fraction, time: TimeSignature) -> String {
    format!("{TEMPO_POSITION}{}", tempo_anchor_of(position, time))
}

pub(crate) fn tempo_linear(position: Fraction, time: TimeSignature, linear: bool) -> String {
    format!("{TEMPO_LINEAR}{linear};{}", tempo_anchor_of(position, time))
}

pub(crate) fn tempo_anchor(guard: &str) -> Option<&str> {
    if let Some(anchor) = guard.strip_prefix(TEMPO_POSITION) {
        return Some(anchor);
    }
    let rest = guard.strip_prefix(TEMPO_LINEAR)?;
    rest.split_once(';').map(|(_, anchor)| anchor)
}

pub(crate) fn stroke(stroke: Option<Stroke>) -> String {
    match stroke {
        Some(stroke) => format!(
            "stroke:{} {}",
            name_of(STROKE_DIRECTIONS, stroke.direction),
            stroke.duration.ticks()
        ),
        None => format!("stroke:{NONE}"),
    }
}

pub(crate) fn repeat(start: bool, end: Option<u8>) -> String {
    match end {
        Some(count) => format!("repeat:{start} {count}"),
        None => format!("repeat:{start} {NONE}"),
    }
}

pub(crate) fn accent(accent: Option<Accent>, staccato: bool) -> String {
    let accent = match accent {
        Some(Accent::Normal) => "normal",
        Some(Accent::Heavy) => "heavy",
        None => NONE,
    };
    format!("accent:{accent} {staccato}")
}

pub(crate) fn slide(slide: Option<Slide>) -> String {
    match slide {
        Some(slide) => format!("slide:{}", slide_bits(slide)),
        None => format!("slide:{NONE}"),
    }
}

fn points(points: [BendPoint; 4]) -> String {
    let points: Vec<String> = points
        .iter()
        .map(|point| format!("{} {}", point.offset(), point.value()))
        .collect();
    points.join(";")
}

pub(crate) fn bend(bend: Option<Bend>) -> String {
    match bend {
        Some(bend) => format!(
            "bend:{}",
            points([
                bend.origin(),
                bend.middle_start(),
                bend.middle_end(),
                bend.destination(),
            ])
        ),
        None => format!("bend:{NONE}"),
    }
}

pub(crate) fn tremolo_bar(bar: Option<TremoloBar>) -> String {
    match bar {
        Some(bar) => format!(
            "tremolo-bar:{}",
            points([
                bar.origin(),
                bar.middle_start(),
                bar.middle_end(),
                bar.destination(),
            ])
        ),
        None => format!("tremolo-bar:{NONE}"),
    }
}

pub(crate) fn trill(trill: Option<Trill>) -> String {
    match trill {
        Some(trill) => format!(
            "trill:{} {}",
            trill.fret,
            name_of(NOTE_VALUES, trill.speed.value())
        ),
        None => format!("trill:{NONE}"),
    }
}

pub(crate) fn harmonic(harmonic: Option<Harmonic>) -> String {
    match harmonic {
        Some(harmonic) => format!(
            "harmonic:{} {}/{}",
            name_of(HARMONICS, harmonic.kind),
            harmonic.fret.numer(),
            harmonic.fret.denom()
        ),
        None => format!("harmonic:{NONE}"),
    }
}

pub(crate) fn tap(tap: Option<Tap>) -> String {
    let tap = match tap {
        Some(Tap::Right) => "right",
        Some(Tap::Left) => "left",
        None => NONE,
    };
    format!("tap:{tap}")
}

pub(crate) fn chord(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("chord:={name}"),
        None => format!("chord:{NONE}"),
    }
}

pub(crate) fn instrument(kind: InstrumentKind, set: Option<InstrumentSet>) -> String {
    let kind = match kind {
        InstrumentKind::Fretted => "fretted",
        InstrumentKind::Percussion => "percussion",
    };
    let set = set.map_or(NONE, |set| name_of(INSTRUMENT_SETS, set));
    format!("instrument:{kind} {set}")
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn kit_digest(kit: &DrumKit) -> u64 {
    let mut text = String::new();
    write_element(&canon::drum_elements(kit), &mut text);
    text.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

pub(crate) fn drum_kit(kind: InstrumentKind, kit: Option<&DrumKit>) -> String {
    let kind = match kind {
        InstrumentKind::Fretted => "fretted",
        InstrumentKind::Percussion => "percussion",
    };
    match kit {
        Some(kit) => format!("drum-kit:{kind} kit {:016x}", kit_digest(kit)),
        None => format!("drum-kit:{kind} {NONE}"),
    }
}

pub(crate) fn midi_connection(midi: &TrackMidi) -> String {
    format!(
        "midi-connection:{} {} {} {}",
        midi.port,
        midi.primary.get(),
        midi.secondary.get(),
        midi.per_string
    )
}

const SOUND_LABEL: &str = "sound-label:";

pub(crate) fn sound_label(index: usize, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{SOUND_LABEL}{index} {name}"),
        None => format!("{SOUND_LABEL}{index}"),
    }
}

pub(crate) fn sound_label_index(guard: &str) -> Option<usize> {
    let rest = guard.strip_prefix(SOUND_LABEL)?;
    let index = rest.split(' ').next()?;
    if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    index.parse().ok()
}

pub(crate) fn clef(clef: Clef) -> String {
    format!("clef:{}", name_of(CLEFS, clef))
}

pub(crate) fn dynamic(dynamic: Dynamic) -> String {
    format!("dynamic:{}", name_of(DYNAMICS, dynamic))
}

pub(crate) fn grace(grace: Option<Grace>) -> String {
    format!(
        "grace:{}",
        grace.map_or(NONE, |grace| name_of(GRACES, grace))
    )
}

pub(crate) fn tremolo(tremolo: Option<TremoloPicking>) -> String {
    let name = tremolo.map_or(NONE, |tremolo| name_of(TREMOLOS, tremolo.value()));
    format!("tremolo:{name}")
}

pub(crate) fn fade_in(fade_in: bool) -> String {
    format!("fade-in:{fade_in}")
}

pub(crate) fn vibrato(vibrato: Option<Vibrato>) -> String {
    let name = vibrato.map_or(NONE, |vibrato| name_of(VIBRATOS, vibrato));
    format!("vibrato:{name}")
}

pub(crate) fn ghost(ghost: bool) -> String {
    format!("ghost:{ghost}")
}

pub(crate) fn left_finger(finger: Option<Finger>) -> String {
    format!("left-finger:{}", finger_name(finger))
}

pub(crate) fn right_finger(finger: Option<Finger>) -> String {
    format!("right-finger:{}", finger_name(finger))
}

fn finger_name(finger: Option<Finger>) -> &'static str {
    finger.map_or(NONE, |finger| name_of(FINGERS, finger))
}

pub(crate) fn minor(minor: bool) -> String {
    format!("minor:{minor}")
}

pub(crate) fn flag(name: &str, value: bool) -> String {
    format!("flag:{name} {value}")
}

pub(crate) fn directions(directions: &Directions) -> String {
    let targets: Vec<&str> = directions
        .targets
        .iter()
        .map(|target| name_of(DIRECTION_TARGETS, *target))
        .collect();
    let jumps: Vec<&str> = directions
        .jumps
        .iter()
        .map(|jump| name_of(DIRECTION_JUMPS, *jump))
        .collect();
    format!("directions:{};{}", targets.join(" "), jumps.join(" "))
}

pub(crate) fn triplet_feel(feel: TripletFeel) -> String {
    format!("triplet-feel:{}", name_of(TRIPLET_FEELS, feel))
}
