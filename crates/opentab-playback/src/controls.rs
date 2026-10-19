use opentab_model::{AutomationValue, Balance, Level, Score, Track};

use crate::PlaybackError;
use crate::curves::MUSICAL_BEND_RANGE;
use crate::exact::Exact;
use crate::midi::{ChannelState, MidiMessage, PAN_CONTROLLER, VOLUME_CONTROLLER};
use crate::order::PlayOrder;

const MILLIONTHS: u32 = 1_000_000;
const VOLUME_SCALE: u32 = 200;
const PAN_SCALE: u32 = 128;
const MAX_CONTROL: u32 = 127;
const RAMP_STEP: (i128, i128) = (1, 16);
pub(crate) const MUTED_GUITAR_PROGRAM: u8 = 28;
const GUITAR_PROGRAMS: std::ops::RangeInclusive<u8> = 24..=31;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Change {
    pub(crate) at: Exact,
    pub(crate) message: MidiMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Timeline {
    pub(crate) initial: ChannelState,
    pub(crate) changes: Vec<Change>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Volume,
    Pan,
}

#[derive(Debug, Clone, Copy)]
struct Point {
    at: Exact,
    value: u8,
    linear: bool,
}

pub(crate) fn volume_control(level: Level) -> u8 {
    u8::try_from((level.millionths() * VOLUME_SCALE / MILLIONTHS).min(MAX_CONTROL)).unwrap_or(127)
}

pub(crate) fn pan_control(balance: Balance) -> u8 {
    u8::try_from((balance.millionths() * PAN_SCALE / MILLIONTHS).min(MAX_CONTROL)).unwrap_or(127)
}

pub(crate) fn is_guitar(program: u8) -> bool {
    GUITAR_PROGRAMS.contains(&program)
}

pub(crate) fn initial_state(track: &Track) -> ChannelState {
    ChannelState {
        program: track.instrument.midi_program,
        bank: track.midi.bank,
        volume: volume_control(track.midi.volume),
        pan: pan_control(track.midi.pan),
        bend_range: MUSICAL_BEND_RANGE,
        ..ChannelState::default()
    }
}

fn controller(kind: Kind, value: u8) -> MidiMessage {
    let controller = match kind {
        Kind::Volume => VOLUME_CONTROLLER,
        Kind::Pan => PAN_CONTROLLER,
    };
    MidiMessage::Control { controller, value }
}

fn ramp(out: &mut Vec<Change>, kind: Kind, from: Point, to: Point) -> Result<(), PlaybackError> {
    let step = Exact::new(RAMP_STEP.0, RAMP_STEP.1)?;
    let span = to.at.sub(from.at)?;
    let rise = i128::from(to.value) - i128::from(from.value);
    let mut at = from.at.add(step)?;
    while at < to.at {
        let share = at.sub(from.at)?.div(span)?;
        let value = Exact::integer(rise).mul(share)?.floor() + i128::from(from.value);
        out.push(Change {
            at,
            message: controller(kind, u8::try_from(value).unwrap_or(from.value)),
        });
        at = at.add(step)?;
    }
    Ok(())
}

pub(crate) fn timeline(
    score: &Score,
    track: &Track,
    order: &PlayOrder,
) -> Result<Timeline, PlaybackError> {
    let initial = initial_state(track);
    let mut raw: Vec<Change> = Vec::new();
    let mut points: Vec<(Kind, Point)> = Vec::new();
    for played in &order.bars {
        let start = Exact::of(played.start.whole_notes());
        let length = Exact::of(played.length);
        for automation in &track.automations {
            if score.find_master_bar_index(automation.master_bar) != Some(played.source) {
                continue;
            }
            let at = start.add(Exact::of(automation.position).min(length))?;
            match automation.value {
                AutomationValue::Volume(level) => points.push((
                    Kind::Volume,
                    Point {
                        at,
                        value: volume_control(level),
                        linear: automation.linear,
                    },
                )),
                AutomationValue::Pan(balance) => points.push((
                    Kind::Pan,
                    Point {
                        at,
                        value: pan_control(balance),
                        linear: automation.linear,
                    },
                )),
                AutomationValue::Sound(sound) => {
                    if let Some(sound) = track.sounds.get(sound.0) {
                        raw.push(Change {
                            at,
                            message: MidiMessage::Bank { bank: sound.bank },
                        });
                        raw.push(Change {
                            at,
                            message: MidiMessage::Program {
                                program: sound.program,
                            },
                        });
                    }
                }
            }
        }
    }
    for kind in [Kind::Volume, Kind::Pan] {
        let mut series: Vec<Point> = points
            .iter()
            .filter(|(candidate, _)| *candidate == kind)
            .map(|(_, point)| *point)
            .collect();
        series.sort_by(|left, right| left.at.cmp(&right.at));
        for (index, point) in series.iter().enumerate() {
            raw.push(Change {
                at: point.at,
                message: controller(kind, point.value),
            });
            if let Some(next) = series.get(index + 1)
                && point.linear
                && next.at > point.at
            {
                ramp(&mut raw, kind, *point, *next)?;
            }
        }
    }
    raw.sort_by(|left, right| left.at.cmp(&right.at));
    let mut state = initial;
    let mut changes = Vec::with_capacity(raw.len());
    for change in raw {
        let before = state;
        state.apply(change.message);
        if state != before {
            changes.push(change);
        }
    }
    Ok(Timeline { initial, changes })
}

impl Timeline {
    pub(crate) fn state_at(&self, at: Exact) -> ChannelState {
        let mut state = self.initial;
        for change in self.changes.iter().take_while(|change| change.at <= at) {
            state.apply(change.message);
        }
        state
    }
}
