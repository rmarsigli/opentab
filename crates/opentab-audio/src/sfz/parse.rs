use crate::sfz::limits::SfzLimits;
use crate::sfz::listing::{IgnoreReason, Listing, SfzScope};
use crate::sfz::region::{CcRange, Envelope, LoopMode, OffMode, SfzRegion, Trigger, VelocityPoint};
use crate::{BankError, UnsafeBank};

const MAX_NOTE: i64 = 127;
const GENERATOR_PREFIX: char = '*';
const VARIABLE: char = '$';
const DEFAULT_RELEASE: f32 = 0.001;
const DEFAULT_OFF_TIME: f32 = 0.006;
const DEFAULT_KEYCENTER: u8 = 60;
const DEFAULT_KEYTRACK: i32 = 100;
const SLOTS: usize = 128;
const SLOT_TABLES: u64 = 3;
const SETTINGS_STEPS: u64 = SLOT_TABLES * SLOTS as u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Header(&'a str),
    Opcode(&'a str, &'a str),
    Junk,
}

fn is_name_byte(byte: u8) -> bool {
    !byte.is_ascii_whitespace() && byte != b'=' && byte != b'<' && byte != b'>'
}

fn starts_opcode(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    let name = bytes.iter().take_while(|byte| is_name_byte(**byte)).count();
    name > 0 && bytes.get(name) == Some(&b'=')
}

fn value_end(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            let next = bytes[index..]
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .map_or(bytes.len(), |offset| index + offset);
            if next >= bytes.len() || bytes[next] == b'<' || starts_opcode(&line[next..]) {
                return index;
            }
            index = next;
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn tokens(line: &str) -> Vec<Token<'_>> {
    let mut found = Vec::new();
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            match after.find('>') {
                Some(end) => {
                    found.push(Token::Header(&after[..end]));
                    rest = after[end + 1..].trim_start();
                }
                None => {
                    found.push(Token::Junk);
                    rest = "";
                }
            }
            continue;
        }
        let name_length = rest.bytes().take_while(|byte| is_name_byte(*byte)).count();
        if name_length > 0 && rest.as_bytes().get(name_length) == Some(&b'=') {
            let name = &rest[..name_length];
            let after = &rest[name_length + 1..];
            let end = value_end(after);
            found.push(Token::Opcode(name, after[..end].trim()));
            rest = after[end..].trim_start();
        } else {
            found.push(Token::Junk);
            let skip = rest.find(char::is_whitespace).unwrap_or(rest.len()).max(1);
            let skip = (skip..=rest.len())
                .find(|index| rest.is_char_boundary(*index))
                .unwrap_or(rest.len());
            rest = rest[skip..].trim_start();
        }
    }
    found
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Settings {
    sample: Option<String>,
    lokey: Option<u8>,
    hikey: Option<u8>,
    lovel: Option<u8>,
    hivel: Option<u8>,
    pitch_keycenter: Option<u8>,
    tune: Option<i32>,
    transpose: Option<i32>,
    pitch_keytrack: Option<i32>,
    volume: Option<f32>,
    amplitude: Option<f32>,
    pan: Option<f32>,
    amp_veltrack: Option<f32>,
    amp_velcurve: Slots<f32>,
    attack: Option<f32>,
    hold: Option<f32>,
    decay: Option<f32>,
    sustain: Option<f32>,
    release: Option<f32>,
    loop_mode: Option<LoopMode>,
    loop_start: Option<u64>,
    loop_end: Option<u64>,
    offset: Option<u64>,
    end: Option<i64>,
    group: Option<u32>,
    off_by: Option<u32>,
    off_mode: Option<OffMode>,
    off_time: Option<f32>,
    lorand: Option<f32>,
    hirand: Option<f32>,
    seq_length: Option<u8>,
    seq_position: Option<u8>,
    trigger: Option<TriggerSetting>,
    sw_last: Option<u8>,
    cc_low: Slots<u8>,
    cc_high: Slots<u8>,
}

#[derive(Debug, Clone, PartialEq)]
struct Slots<T: Copy>([Option<T>; SLOTS]);

impl<T: Copy> Default for Slots<T> {
    fn default() -> Slots<T> {
        Slots([None; SLOTS])
    }
}

impl<T: Copy> Slots<T> {
    fn set(&mut self, index: u8, value: T) {
        if let Some(slot) = self.0.get_mut(usize::from(index)) {
            *slot = Some(value);
        }
    }

    fn get(&self, index: usize) -> Option<T> {
        self.0.get(index).copied().flatten()
    }

    fn inherit(&mut self, parent: &Slots<T>) -> u64 {
        for (mine, theirs) in self.0.iter_mut().zip(parent.0.iter()) {
            if mine.is_none() {
                *mine = *theirs;
            }
        }
        SLOTS as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSetting {
    Play(Trigger),
    Release,
}

fn fill<T: Clone>(slot: &mut Option<T>, parent: &Option<T>) {
    if slot.is_none() {
        slot.clone_from(parent);
    }
}

impl Settings {
    fn inherit(&mut self, parent: &Settings) -> u64 {
        let mut steps = 0;
        fill(&mut self.sample, &parent.sample);
        fill(&mut self.lokey, &parent.lokey);
        fill(&mut self.hikey, &parent.hikey);
        fill(&mut self.lovel, &parent.lovel);
        fill(&mut self.hivel, &parent.hivel);
        fill(&mut self.pitch_keycenter, &parent.pitch_keycenter);
        fill(&mut self.tune, &parent.tune);
        fill(&mut self.transpose, &parent.transpose);
        fill(&mut self.pitch_keytrack, &parent.pitch_keytrack);
        fill(&mut self.volume, &parent.volume);
        fill(&mut self.amplitude, &parent.amplitude);
        fill(&mut self.pan, &parent.pan);
        fill(&mut self.amp_veltrack, &parent.amp_veltrack);
        steps += self.amp_velcurve.inherit(&parent.amp_velcurve);
        fill(&mut self.attack, &parent.attack);
        fill(&mut self.hold, &parent.hold);
        fill(&mut self.decay, &parent.decay);
        fill(&mut self.sustain, &parent.sustain);
        fill(&mut self.release, &parent.release);
        fill(&mut self.loop_mode, &parent.loop_mode);
        fill(&mut self.loop_start, &parent.loop_start);
        fill(&mut self.loop_end, &parent.loop_end);
        fill(&mut self.offset, &parent.offset);
        fill(&mut self.end, &parent.end);
        fill(&mut self.group, &parent.group);
        fill(&mut self.off_by, &parent.off_by);
        fill(&mut self.off_mode, &parent.off_mode);
        fill(&mut self.off_time, &parent.off_time);
        fill(&mut self.lorand, &parent.lorand);
        fill(&mut self.hirand, &parent.hirand);
        fill(&mut self.seq_length, &parent.seq_length);
        fill(&mut self.seq_position, &parent.seq_position);
        fill(&mut self.trigger, &parent.trigger);
        fill(&mut self.sw_last, &parent.sw_last);
        steps += self.cc_low.inherit(&parent.cc_low);
        steps += self.cc_high.inherit(&parent.cc_high);
        steps
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Header {
    Control,
    Global,
    Master,
    Group,
    Region,
    Ignored,
    Before,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Control {
    pub(crate) default_path: String,
    pub(crate) set_cc: Vec<(u8, u8)>,
    pub(crate) sw_default: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Parsed {
    pub(crate) regions: Vec<SfzRegion>,
    pub(crate) control: Control,
    pub(crate) listing: Listing,
    pub(crate) bytes: u64,
    closed: u64,
    steps: u64,
}

pub(crate) struct ParseRun {
    pub(crate) result: Result<Parsed, BankError>,
    pub(crate) steps: u64,
}

pub(crate) fn region_cost(region: &SfzRegion) -> u64 {
    (std::mem::size_of::<SfzRegion>()
        + std::mem::size_of::<usize>()
        + region.sample.len()
        + region.amp_velcurve.len() * std::mem::size_of::<VelocityPoint>()
        + region.cc_ranges.len() * std::mem::size_of::<CcRange>()) as u64
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Number {
    Value(f64),
    Invalid,
}

fn number(value: &str) -> Number {
    match value.parse::<f64>() {
        Ok(parsed) if parsed.is_finite() => Number::Value(parsed),
        _ => Number::Invalid,
    }
}

pub(crate) fn note_value(value: &str) -> Option<i64> {
    if let Ok(number) = value.parse::<i64>() {
        return Some(number);
    }
    let mut chars = value.chars();
    let letter = chars.next()?.to_ascii_lowercase();
    let base = match letter {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, octave) = if let Some(after) = rest.strip_prefix('#') {
        (1, after)
    } else if let Some(after) = rest.strip_prefix('b') {
        (-1, after)
    } else {
        (0, rest)
    };
    if octave.starts_with('+') {
        return None;
    }
    let octave: i64 = octave.parse().ok()?;
    let note = 12 * (octave + 1) + base + shift;
    (0..=MAX_NOTE).contains(&note).then_some(note)
}

struct Assign<'a> {
    name: &'a str,
    scope: SfzScope,
    listing: &'a mut Listing,
}

impl Assign<'_> {
    fn invalid(&mut self) {
        self.listing
            .note(self.name, self.scope, IgnoreReason::InvalidValue);
    }

    fn clamped(&mut self) {
        self.listing
            .note(self.name, self.scope, IgnoreReason::Clamped);
    }

    fn real(&mut self, value: &str, low: f64, high: f64) -> Option<f64> {
        match number(value) {
            Number::Value(parsed) => {
                if parsed < low || parsed > high {
                    self.clamped();
                }
                Some(parsed.clamp(low, high))
            }
            Number::Invalid => {
                self.invalid();
                None
            }
        }
    }

    fn float(&mut self, value: &str, low: f64, high: f64) -> Option<f32> {
        self.real(value, low, high).map(|parsed| parsed as f32)
    }

    fn integer(&mut self, value: &str, low: i64, high: i64) -> Option<i64> {
        self.real(value, low as f64, high as f64)
            .map(|parsed| parsed.round() as i64)
    }

    fn byte(&mut self, value: &str) -> Option<u8> {
        self.integer(value, 0, MAX_NOTE)
            .and_then(|parsed| u8::try_from(parsed).ok())
    }

    fn note(&mut self, value: &str) -> Option<u8> {
        match note_value(value) {
            Some(parsed) => {
                if !(0..=MAX_NOTE).contains(&parsed) {
                    self.clamped();
                }
                u8::try_from(parsed.clamp(0, MAX_NOTE)).ok()
            }
            None => {
                self.invalid();
                None
            }
        }
    }

    fn unsupported(&mut self) {
        self.listing
            .note(self.name, self.scope, IgnoreReason::Unsupported);
    }
}

fn suffix_number(name: &str, prefix: &str) -> Option<u8> {
    let digits = name.strip_prefix(prefix)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u8>().ok().filter(|number| *number <= 127)
}

fn is_silent(name: &str) -> bool {
    name.ends_with("_label") || name.starts_with("label_cc") || name == "curve_index"
}

fn apply(settings: &mut Settings, control: &mut Control, assign: &mut Assign<'_>, value: &str) {
    let name = assign.name;
    match name {
        "sample" => settings.sample = Some(value.to_owned()),
        "key" => {
            if let Some(note) = assign.note(value) {
                settings.lokey = Some(note);
                settings.hikey = Some(note);
                settings.pitch_keycenter = Some(note);
            }
        }
        "lokey" => settings.lokey = assign.note(value).or(settings.lokey),
        "hikey" => settings.hikey = assign.note(value).or(settings.hikey),
        "pitch_keycenter" => {
            settings.pitch_keycenter = assign.note(value).or(settings.pitch_keycenter);
        }
        "lovel" => settings.lovel = assign.byte(value).or(settings.lovel),
        "hivel" => settings.hivel = assign.byte(value).or(settings.hivel),
        "tune" => {
            settings.tune = assign
                .integer(value, -9600, 9600)
                .map(|cents| cents as i32)
                .or(settings.tune);
        }
        "transpose" => {
            settings.transpose = assign
                .integer(value, -127, 127)
                .map(|semitones| semitones as i32)
                .or(settings.transpose);
        }
        "pitch_keytrack" => {
            settings.pitch_keytrack = assign
                .integer(value, -1200, 1200)
                .map(|cents| cents as i32)
                .or(settings.pitch_keytrack);
        }
        "volume" => settings.volume = assign.float(value, -144.0, 6.0).or(settings.volume),
        "amplitude" => {
            settings.amplitude = assign.float(value, 0.0, 100.0).or(settings.amplitude);
        }
        "pan" => settings.pan = assign.float(value, -100.0, 100.0).or(settings.pan),
        "amp_veltrack" => {
            settings.amp_veltrack = assign.float(value, -100.0, 100.0).or(settings.amp_veltrack);
        }
        "ampeg_attack" => settings.attack = assign.float(value, 0.0, 100.0).or(settings.attack),
        "ampeg_hold" => settings.hold = assign.float(value, 0.0, 100.0).or(settings.hold),
        "ampeg_decay" => settings.decay = assign.float(value, 0.0, 100.0).or(settings.decay),
        "ampeg_sustain" => {
            settings.sustain = assign.float(value, 0.0, 100.0).or(settings.sustain);
        }
        "ampeg_release" => {
            settings.release = assign.float(value, 0.0, 100.0).or(settings.release);
        }
        "loop_mode" | "loopmode" => match value.to_ascii_lowercase().as_str() {
            "no_loop" => settings.loop_mode = Some(LoopMode::NoLoop),
            "one_shot" => settings.loop_mode = Some(LoopMode::OneShot),
            "loop_continuous" => settings.loop_mode = Some(LoopMode::Continuous),
            "loop_sustain" => settings.loop_mode = Some(LoopMode::Sustain),
            _ => assign.invalid(),
        },
        "loop_start" | "loopstart" => {
            settings.loop_start = frames(assign, value).or(settings.loop_start);
        }
        "loop_end" | "loopend" => {
            settings.loop_end = frames(assign, value).or(settings.loop_end);
        }
        "offset" => settings.offset = frames(assign, value).or(settings.offset),
        "end" => {
            settings.end = assign
                .integer(value, -1, i64::from(u32::MAX))
                .or(settings.end);
        }
        "group" => settings.group = group_number(assign, value).or(settings.group),
        "off_by" => settings.off_by = group_number(assign, value).or(settings.off_by),
        "off_mode" => match value.to_ascii_lowercase().as_str() {
            "fast" => settings.off_mode = Some(OffMode::Fast),
            "normal" => settings.off_mode = Some(OffMode::Normal),
            "time" => settings.off_mode = Some(OffMode::Time),
            _ => assign.invalid(),
        },
        "off_time" => settings.off_time = assign.float(value, 0.0, 100.0).or(settings.off_time),
        "lorand" => settings.lorand = assign.float(value, 0.0, 1.0).or(settings.lorand),
        "hirand" => settings.hirand = assign.float(value, 0.0, 1.0).or(settings.hirand),
        "seq_length" => {
            settings.seq_length = assign
                .integer(value, 1, 100)
                .and_then(|length| u8::try_from(length).ok())
                .or(settings.seq_length);
        }
        "seq_position" => {
            settings.seq_position = assign
                .integer(value, 1, 100)
                .and_then(|position| u8::try_from(position).ok())
                .or(settings.seq_position);
        }
        "trigger" => match value.to_ascii_lowercase().as_str() {
            "attack" => settings.trigger = Some(TriggerSetting::Play(Trigger::Attack)),
            "first" => settings.trigger = Some(TriggerSetting::Play(Trigger::First)),
            "legato" => settings.trigger = Some(TriggerSetting::Play(Trigger::Legato)),
            "release" | "release_key" => settings.trigger = Some(TriggerSetting::Release),
            _ => assign.invalid(),
        },
        "sw_last" => settings.sw_last = assign.note(value).or(settings.sw_last),
        "sw_default" => control.sw_default = assign.note(value).or(control.sw_default),
        "sw_lokey" | "sw_hikey" => {
            assign.note(value);
        }
        "default_path" if assign.scope == SfzScope::Control => {
            control.default_path = value.to_owned();
        }
        _ => apply_numbered(settings, control, assign, value),
    }
}

fn apply_numbered(
    settings: &mut Settings,
    control: &mut Control,
    assign: &mut Assign<'_>,
    value: &str,
) {
    let name = assign.name;
    if let Some(velocity) = suffix_number(name, "amp_velcurve_") {
        if let Some(gain) = assign.float(value, 0.0, 1.0) {
            settings.amp_velcurve.set(velocity, gain);
        }
    } else if let Some(controller) = suffix_number(name, "locc") {
        if let Some(low) = assign.byte(value) {
            settings.cc_low.set(controller, low);
        }
    } else if let Some(controller) = suffix_number(name, "hicc") {
        if let Some(high) = assign.byte(value) {
            settings.cc_high.set(controller, high);
        }
    } else if let Some(controller) = suffix_number(name, "set_cc") {
        if assign.scope != SfzScope::Control {
            assign.unsupported();
        } else if let Some(initial) = assign.byte(value) {
            control.set_cc.retain(|pair| pair.0 != controller);
            control.set_cc.push((controller, initial));
        }
    } else if !is_silent(name) {
        assign.unsupported();
    }
}

fn frames(assign: &mut Assign<'_>, value: &str) -> Option<u64> {
    assign
        .integer(value, 0, i64::from(u32::MAX))
        .and_then(|frame| u64::try_from(frame).ok())
}

fn group_number(assign: &mut Assign<'_>, value: &str) -> Option<u32> {
    assign
        .integer(value, 0, i64::from(u32::MAX))
        .and_then(|number| u32::try_from(number).ok())
}

fn finish(
    settings: Settings,
    default_path: &str,
    path_cap: usize,
    listing: &mut Listing,
) -> Result<Option<SfzRegion>, BankError> {
    let Some(sample) = settings.sample.as_deref() else {
        listing.note("sample", SfzScope::Region, IgnoreReason::MissingSample);
        return Ok(None);
    };
    if sample.is_empty() {
        listing.note("sample", SfzScope::Region, IgnoreReason::MissingSample);
        return Ok(None);
    }
    if sample.starts_with(GENERATOR_PREFIX) {
        listing.note("sample", SfzScope::Region, IgnoreReason::Generator);
        return Ok(None);
    }
    if sample.contains(VARIABLE) {
        listing.note("sample", SfzScope::Region, IgnoreReason::UnresolvedVariable);
        return Ok(None);
    }
    if default_path.len().saturating_add(sample.len()) > path_cap {
        return Err(BankError::Unsafe {
            reason: UnsafeBank::PathTooLong {
                limit: path_cap as u64,
            },
        });
    }
    let sample = format!("{default_path}{sample}");
    let trigger = match settings.trigger {
        Some(TriggerSetting::Release) => {
            listing.note("trigger", SfzScope::Region, IgnoreReason::ReleaseTrigger);
            return Ok(None);
        }
        Some(TriggerSetting::Play(trigger)) => trigger,
        None => Trigger::Attack,
    };
    let end = match settings.end {
        Some(-1) => {
            listing.note("end", SfzScope::Region, IgnoreReason::DisabledRegion);
            return Ok(None);
        }
        Some(frame) => u64::try_from(frame).ok(),
        None => None,
    };
    let mut amp_velcurve: Vec<VelocityPoint> = Vec::new();
    let mut cc_ranges: Vec<CcRange> = Vec::new();
    for (index, number) in (0..SLOTS).zip(0u8..) {
        if let Some(gain) = settings.amp_velcurve.get(index) {
            amp_velcurve.push(VelocityPoint {
                velocity: number,
                gain,
            });
        }
        let low = settings.cc_low.get(index);
        let high = settings.cc_high.get(index);
        if low.is_some() || high.is_some() {
            cc_ranges.push(CcRange {
                controller: number,
                low: low.unwrap_or(0),
                high: high.unwrap_or(MAX_NOTE as u8),
            });
        }
    }
    Ok(Some(SfzRegion {
        sample,
        lokey: settings.lokey.unwrap_or(0),
        hikey: settings.hikey.unwrap_or(MAX_NOTE as u8),
        lovel: settings.lovel.unwrap_or(0),
        hivel: settings.hivel.unwrap_or(MAX_NOTE as u8),
        pitch_keycenter: settings.pitch_keycenter.unwrap_or(DEFAULT_KEYCENTER),
        tune: settings.tune.unwrap_or(0),
        transpose: settings.transpose.unwrap_or(0),
        pitch_keytrack: settings.pitch_keytrack.unwrap_or(DEFAULT_KEYTRACK),
        volume: settings.volume.unwrap_or(0.0),
        amplitude: settings.amplitude.unwrap_or(100.0),
        pan: settings.pan.unwrap_or(0.0),
        amp_veltrack: settings.amp_veltrack.unwrap_or(100.0),
        amp_velcurve,
        ampeg: Envelope {
            attack: settings.attack.unwrap_or(0.0),
            hold: settings.hold.unwrap_or(0.0),
            decay: settings.decay.unwrap_or(0.0),
            sustain: settings.sustain.unwrap_or(100.0),
            release: settings.release.unwrap_or(DEFAULT_RELEASE),
        },
        loop_mode: settings.loop_mode.unwrap_or(LoopMode::NoLoop),
        loop_start: settings.loop_start,
        loop_end: settings.loop_end,
        offset: settings.offset.unwrap_or(0),
        end,
        group: settings.group.unwrap_or(0),
        off_by: settings.off_by.unwrap_or(0),
        off_mode: settings.off_mode.unwrap_or(OffMode::Fast),
        off_time: settings.off_time.unwrap_or(DEFAULT_OFF_TIME),
        lorand: settings.lorand.unwrap_or(0.0),
        hirand: settings.hirand.unwrap_or(1.0),
        seq_length: settings.seq_length.unwrap_or(1),
        seq_position: settings.seq_position.unwrap_or(1),
        trigger,
        sw_last: settings.sw_last,
        cc_ranges,
    }))
}

struct Scopes {
    global: Settings,
    master: Settings,
    group: Settings,
    region: Settings,
    scratch: Settings,
    header: Header,
}

impl Scopes {
    fn current(&mut self) -> Option<(&mut Settings, SfzScope)> {
        match self.header {
            Header::Global => Some((&mut self.global, SfzScope::Global)),
            Header::Master => Some((&mut self.master, SfzScope::Master)),
            Header::Group => Some((&mut self.group, SfzScope::Group)),
            Header::Region => Some((&mut self.region, SfzScope::Region)),
            Header::Control | Header::Ignored | Header::Before => None,
        }
    }

    fn close_region(
        &mut self,
        parsed: &mut Parsed,
        limits: &SfzLimits,
        budget: u64,
    ) -> Result<(), BankError> {
        if self.header != Header::Region {
            return Ok(());
        }
        if parsed.closed >= limits.regions {
            return Err(BankError::Unsafe {
                reason: UnsafeBank::TooManyRegions {
                    limit: limits.regions,
                },
            });
        }
        parsed.closed += 1;
        let mut settings = std::mem::take(&mut self.region);
        let mut steps = SETTINGS_STEPS;
        steps += settings.inherit(&self.group);
        steps += settings.inherit(&self.master);
        steps += settings.inherit(&self.global);
        steps += SETTINGS_STEPS;
        parsed.steps = parsed.steps.saturating_add(steps);
        if let Some(region) = finish(
            settings,
            &parsed.control.default_path,
            limits.path_bytes,
            &mut parsed.listing,
        )? {
            let cost = region_cost(&region);
            if parsed
                .bytes
                .saturating_add(cost)
                .saturating_add(parsed.listing.bytes())
                > budget
            {
                return Err(BankError::Unsafe {
                    reason: UnsafeBank::LoaderBudget,
                });
            }
            parsed.bytes += cost;
            parsed.listing.allow(budget - parsed.bytes);
            parsed.regions.push(region);
        }
        Ok(())
    }
}

fn open_header(scopes: &mut Scopes, name: &str, listing: &mut Listing) -> u64 {
    let lower = name.trim().to_ascii_lowercase();
    let (header, cleared) = match lower.as_str() {
        "control" => (Header::Control, 0),
        "global" => {
            scopes.global = Settings::default();
            scopes.master = Settings::default();
            scopes.group = Settings::default();
            (Header::Global, 3)
        }
        "master" => {
            scopes.master = Settings::default();
            scopes.group = Settings::default();
            (Header::Master, 2)
        }
        "group" => {
            scopes.group = Settings::default();
            (Header::Group, 1)
        }
        "region" => {
            scopes.region = Settings::default();
            (Header::Region, 1)
        }
        other => {
            listing.note(
                &format!("<{other}>"),
                SfzScope::File,
                IgnoreReason::Unsupported,
            );
            (Header::Ignored, 0)
        }
    };
    scopes.header = header;
    cleared * SETTINGS_STEPS
}

pub(crate) fn parse(text: &str, limits: &SfzLimits, budget: u64) -> Result<Parsed, BankError> {
    parse_measured(text, limits, budget).result
}

pub(crate) fn parse_measured(text: &str, limits: &SfzLimits, budget: u64) -> ParseRun {
    let mut parsed = Parsed {
        regions: Vec::new(),
        control: Control::default(),
        listing: Listing::capped(limits.ignored_names, budget),
        bytes: 0,
        closed: 0,
        steps: 0,
    };
    let mut scopes = Scopes {
        global: Settings::default(),
        master: Settings::default(),
        group: Settings::default(),
        region: Settings::default(),
        scratch: Settings::default(),
        header: Header::Before,
    };
    parsed.steps = 5 * SETTINGS_STEPS;
    let outcome = parse_lines(text, limits, budget, &mut scopes, &mut parsed);
    let steps = parsed.steps;
    ParseRun {
        result: outcome.map(|()| parsed),
        steps,
    }
}

fn parse_lines(
    text: &str,
    limits: &SfzLimits,
    budget: u64,
    scopes: &mut Scopes,
    parsed: &mut Parsed,
) -> Result<(), BankError> {
    for line in text.lines() {
        parsed.steps = parsed.steps.saturating_add(line.len() as u64 + 1);
        for token in tokens(line) {
            match token {
                Token::Header(name) => {
                    scopes.close_region(parsed, limits, budget)?;
                    let cleared = open_header(scopes, name, &mut parsed.listing);
                    parsed.steps = parsed.steps.saturating_add(cleared);
                }
                Token::Opcode(raw_name, value) => {
                    apply_token(scopes, parsed, raw_name, value);
                }
                Token::Junk => {
                    if scopes.header != Header::Ignored {
                        parsed
                            .listing
                            .note("text", SfzScope::File, IgnoreReason::InvalidValue);
                    }
                }
            }
            parsed.listing.check()?;
        }
    }
    scopes.close_region(parsed, limits, budget)
}

fn apply_token(scopes: &mut Scopes, parsed: &mut Parsed, raw_name: &str, value: &str) {
    let name = raw_name.to_ascii_lowercase();
    let header = scopes.header;
    match header {
        Header::Ignored => {}
        Header::Before => {
            parsed
                .listing
                .note(&name, SfzScope::File, IgnoreReason::OutsideHeader);
        }
        Header::Control => {
            let scope = SfzScope::Control;
            if name.contains(VARIABLE) || value.contains(VARIABLE) {
                parsed
                    .listing
                    .note(&name, scope, IgnoreReason::UnresolvedVariable);
                return;
            }
            let mut assign = Assign {
                name: &name,
                scope,
                listing: &mut parsed.listing,
            };
            let ignored = &mut scopes.scratch;
            match name.as_str() {
                "default_path" | "sw_default" => {
                    apply(ignored, &mut parsed.control, &mut assign, value);
                }
                _ if suffix_number(&name, "set_cc").is_some() || is_silent(&name) => {
                    apply_numbered(ignored, &mut parsed.control, &mut assign, value);
                }
                _ => assign.unsupported(),
            }
        }
        _ => {
            let Some((settings, scope)) = scopes.current() else {
                return;
            };
            if (name.contains(VARIABLE) || value.contains(VARIABLE)) && name != "sample" {
                parsed
                    .listing
                    .note(&name, scope, IgnoreReason::UnresolvedVariable);
                return;
            }
            let mut assign = Assign {
                name: &name,
                scope,
                listing: &mut parsed.listing,
            };
            apply(settings, &mut parsed.control, &mut assign, value);
        }
    }
}
