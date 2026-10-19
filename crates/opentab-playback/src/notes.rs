use std::collections::BTreeMap;

use opentab_model::{
    Beat, DrumKit, Fraction, Grace, InstrumentKind, Note, NoteKind, Score, Track, Voice,
    gm_drum_kit,
};

use crate::bends::gp_bend;
use crate::exact::Exact;
use crate::harmonics;
use crate::midi::{Channel, MidiMessage, NOTE_OFF_VELOCITY, velocity_of};
use crate::order::{PlayOrder, PlayedBar};
use crate::{PlaybackError, PlaybackWarning, ScoreTime};

const MAX_KEY: u16 = 127;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Pending {
    pub(crate) at: Exact,
    pub(crate) rank: u8,
    pub(crate) order: usize,
    pub(crate) track: usize,
    pub(crate) channel: Channel,
    pub(crate) message: MidiMessage,
}

#[derive(Debug, Default)]
pub(crate) struct Stream {
    pub(crate) pending: Vec<Pending>,
}

impl Stream {
    pub(crate) fn push(&mut self, at: Exact, track: usize, channel: Channel, message: MidiMessage) {
        let order = self.pending.len();
        self.pending.push(Pending {
            at,
            rank: message.rank(),
            order,
            track,
            channel,
            message,
        });
    }
}

#[derive(Debug, Clone, Copy)]
struct Sounding {
    start: Exact,
    end: Exact,
    key: u8,
    velocity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TieKey {
    staff: usize,
    slot: u8,
    key: u8,
}

#[derive(Debug, Clone, Copy)]
struct OpenTie {
    sounding: usize,
    until: Exact,
}

#[derive(Debug, Clone, Copy)]
struct Place {
    staff: usize,
    slot: u8,
    ties: bool,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    start: Exact,
    length: Exact,
}

#[derive(Debug, Default)]
struct GracePlan<'a> {
    before: Vec<&'a Beat>,
    on: Vec<&'a Beat>,
}

pub(crate) struct TrackWriter<'a> {
    score: &'a Score,
    track: &'a Track,
    index: usize,
    channel: Channel,
    kit: Option<DrumKit>,
    soundings: Vec<Sounding>,
    ties: BTreeMap<TieKey, OpenTie>,
    pub(crate) warnings: Vec<PlaybackWarning>,
}

fn length_of(beat: &Beat) -> Exact {
    Exact::of(beat.duration.length())
}

fn total(beats: &[&Beat]) -> Result<Exact, PlaybackError> {
    beats
        .iter()
        .try_fold(Exact::ZERO, |sum, beat| sum.add(length_of(beat)))
}

fn time_of(at: Exact) -> Result<ScoreTime, PlaybackError> {
    at.fraction().and_then(ScoreTime::new)
}

fn pieces(span: Span, value: Fraction) -> Result<Vec<Span>, PlaybackError> {
    let step = Exact::of(value);
    let end = span.start.add(span.length)?;
    let mut out = Vec::new();
    let mut at = span.start;
    while at < end {
        let next = at.add(step)?.min(end);
        out.push(Span {
            start: at,
            length: next.sub(at)?,
        });
        at = next;
    }
    Ok(out)
}

impl<'a> TrackWriter<'a> {
    pub(crate) fn new(
        score: &'a Score,
        index: usize,
        track: &'a Track,
        channel: Channel,
    ) -> TrackWriter<'a> {
        TrackWriter {
            score,
            track,
            index,
            channel,
            kit: track.kit().cloned(),
            soundings: Vec::new(),
            ties: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    pub(crate) fn write(
        mut self,
        order: &PlayOrder,
        stream: &mut Stream,
    ) -> Result<Vec<PlaybackWarning>, PlaybackError> {
        for (staff_index, staff) in self.track.staves.iter().enumerate() {
            for played in &order.bars {
                let Some(bar) = staff.bars.get(played.source) else {
                    continue;
                };
                for voice in &bar.voices {
                    self.voice(staff_index, played, voice, stream)?;
                }
            }
        }
        for sounding in &self.soundings {
            stream.push(
                sounding.start,
                self.index,
                self.channel,
                MidiMessage::NoteOn {
                    key: sounding.key,
                    velocity: sounding.velocity,
                },
            );
            stream.push(
                sounding.end,
                self.index,
                self.channel,
                MidiMessage::NoteOff {
                    key: sounding.key,
                    velocity: NOTE_OFF_VELOCITY,
                },
            );
        }
        Ok(self.warnings)
    }

    fn pickup_shift(&self, played: &PlayedBar, voice: &Voice) -> Result<Exact, PlaybackError> {
        if !self.score.anacrusis || played.source != 0 {
            return Ok(Exact::ZERO);
        }
        let content = voice
            .beats
            .iter()
            .filter(|beat| beat.effects.grace.is_none())
            .try_fold(Exact::ZERO, |sum, beat| sum.add(length_of(beat)))?;
        let gap = Exact::of(played.length).sub(content)?;
        Ok(gap.max(Exact::ZERO))
    }

    fn voice(
        &mut self,
        staff: usize,
        played: &PlayedBar,
        voice: &Voice,
        stream: &mut Stream,
    ) -> Result<(), PlaybackError> {
        let start = Exact::of(played.start.whole_notes()).add(self.pickup_shift(played, voice)?)?;
        let place = Place {
            staff,
            slot: voice.slot.get(),
            ties: true,
        };
        let mut offset = Exact::ZERO;
        let mut graces: Vec<&Beat> = Vec::new();
        for beat in &voice.beats {
            if beat.effects.grace.is_some() {
                graces.push(beat);
                continue;
            }
            let onset = start.add(offset)?;
            let length = length_of(beat);
            let shift = self.graces(&graces, onset, length, place, stream)?;
            graces.clear();
            let span = Span {
                start: onset.add(shift)?,
                length: length.sub(shift)?,
            };
            self.beat(beat, span, place, stream)?;
            offset = offset.add(length)?;
        }
        let end = start.add(offset)?;
        self.graces(&graces, end, Exact::ZERO, place, stream)?;
        Ok(())
    }

    fn graces<'b>(
        &mut self,
        graces: &[&'b Beat],
        onset: Exact,
        length: Exact,
        place: Place,
        stream: &mut Stream,
    ) -> Result<Exact, PlaybackError> {
        if graces.is_empty() {
            return Ok(Exact::ZERO);
        }
        let mut plan: GracePlan<'b> = GracePlan::default();
        for beat in graces {
            match beat.effects.grace {
                Some(Grace::BeforeBeat) => plan.before.push(beat),
                Some(Grace::OnBeat) | None => plan.on.push(beat),
            }
        }
        if total(&plan.before)? > onset {
            let mut moved = std::mem::take(&mut plan.before);
            moved.append(&mut plan.on);
            plan.on = moved;
        }
        let on_total = total(&plan.on)?;
        if on_total >= length && total(&plan.before)?.add(on_total)? <= onset {
            let mut moved = std::mem::take(&mut plan.on);
            plan.before.append(&mut moved);
        }
        let grace_place = Place {
            ties: false,
            ..place
        };
        let mut at = onset.sub(total(&plan.before)?)?;
        for beat in &plan.before {
            let length = length_of(beat);
            self.beat(beat, Span { start: at, length }, grace_place, stream)?;
            at = at.add(length)?;
        }
        let mut at = onset;
        for beat in &plan.on {
            let length = length_of(beat);
            self.beat(beat, Span { start: at, length }, grace_place, stream)?;
            at = at.add(length)?;
        }
        let on_total = total(&plan.on)?;
        Ok(if on_total < length {
            on_total
        } else {
            Exact::ZERO
        })
    }

    fn beat(
        &mut self,
        beat: &Beat,
        span: Span,
        place: Place,
        stream: &mut Stream,
    ) -> Result<(), PlaybackError> {
        let velocity = velocity_of(beat.effects.dynamic);
        for note in &beat.notes {
            let Some(key) = self.key_of(note, span.start)? else {
                continue;
            };
            let sounding = if note.effects.staccato && note.effects.accent.is_none() {
                Span {
                    start: span.start,
                    length: span.length.mul(Exact::new(1, 2)?)?,
                }
            } else {
                span
            };
            if self.track.instrument.kind == InstrumentKind::Fretted {
                for event in gp_bend(note, sounding.start, sounding.length)? {
                    stream.push(
                        event.at,
                        self.index,
                        self.channel,
                        MidiMessage::PitchBend { value: event.value },
                    );
                }
            }
            if let Some(auxiliary) = self.trill_key(note, span.start)? {
                let speed = note
                    .effects
                    .trill
                    .map(|trill| trill.speed.value().denominator())
                    .unwrap_or(1);
                for (index, piece) in pieces(sounding, Fraction::new(1, speed))?
                    .into_iter()
                    .enumerate()
                {
                    let key = if index % 2 == 0 { key } else { auxiliary };
                    self.sound(piece, key, velocity, None);
                }
            } else if let Some(tremolo) = beat.effects.tremolo_picking {
                let value = Fraction::new(1, tremolo.value().denominator());
                for piece in pieces(sounding, value)? {
                    self.sound(piece, key, velocity, None);
                }
            } else {
                let tie = place.ties.then_some((place, note, span));
                self.sound(sounding, key, velocity, tie);
            }
        }
        Ok(())
    }

    fn sound(&mut self, span: Span, key: u8, velocity: u8, tie: Option<(Place, &Note, Span)>) {
        let Ok(end) = span.start.add(span.length) else {
            return;
        };
        if end <= span.start {
            return;
        }
        let Some((place, note, written)) = tie else {
            self.soundings.push(Sounding {
                start: span.start,
                end,
                key,
                velocity,
            });
            return;
        };
        let tie_key = TieKey {
            staff: place.staff,
            slot: place.slot,
            key,
        };
        let until = written.start.add(written.length).unwrap_or(end);
        if note.effects.tie_destination
            && let Some(open) = self.ties.get(&tie_key).copied()
            && open.until == span.start
            && let Some(merged) = self.soundings.get_mut(open.sounding)
        {
            merged.end = end.max(merged.end);
            if note.effects.tie_origin {
                self.ties.insert(
                    tie_key,
                    OpenTie {
                        sounding: open.sounding,
                        until,
                    },
                );
            } else {
                self.ties.remove(&tie_key);
            }
            return;
        }
        self.soundings.push(Sounding {
            start: span.start,
            end,
            key,
            velocity,
        });
        if note.effects.tie_origin {
            self.ties.insert(
                tie_key,
                OpenTie {
                    sounding: self.soundings.len() - 1,
                    until,
                },
            );
        } else {
            self.ties.remove(&tie_key);
        }
    }

    fn out_of_range(&mut self, at: Exact, pitch: u16) -> Result<Option<u8>, PlaybackError> {
        if pitch > MAX_KEY {
            self.warnings.push(PlaybackWarning::PitchOutOfRange {
                track: self.index,
                at: time_of(at)?,
                pitch,
            });
            return Ok(None);
        }
        Ok(u8::try_from(pitch).ok())
    }

    fn open_string(&mut self, string: u8, at: Exact) -> Result<Option<u16>, PlaybackError> {
        match self.track.tuning.strings.get(usize::from(string)) {
            Some(open) => Ok(Some(u16::from(open.0) + u16::from(self.track.capo))),
            None => {
                self.warnings.push(PlaybackWarning::MissingString {
                    track: self.index,
                    at: time_of(at)?,
                    string,
                });
                Ok(None)
            }
        }
    }

    fn key_of(&mut self, note: &Note, at: Exact) -> Result<Option<u8>, PlaybackError> {
        match note.kind {
            NoteKind::Fretted { string, fret } => {
                let Some(open) = self.open_string(string, at)? else {
                    return Ok(None);
                };
                let fretted = open + u16::from(fret);
                let pitch = note.effects.harmonic.map_or(fretted, |harmonic| {
                    harmonics::sounding(open, fretted, harmonic)
                });
                self.out_of_range(at, pitch)
            }
            NoteKind::Percussion { articulation } => match self
                .kit
                .get_or_insert_with(gm_drum_kit)
                .key_of(articulation)
            {
                Some(key) => self.out_of_range(at, u16::from(key)),
                None => {
                    self.warnings.push(PlaybackWarning::UnknownArticulation {
                        track: self.index,
                        at: time_of(at)?,
                        articulation,
                    });
                    Ok(None)
                }
            },
        }
    }

    fn trill_key(&mut self, note: &Note, at: Exact) -> Result<Option<u8>, PlaybackError> {
        let (Some(trill), NoteKind::Fretted { string, .. }) = (note.effects.trill, note.kind)
        else {
            return Ok(None);
        };
        let Some(open) = self.track.tuning.strings.get(usize::from(string)) else {
            return Ok(None);
        };
        let pitch = u16::from(open.0) + u16::from(self.track.capo) + u16::from(trill.fret);
        self.out_of_range(at, pitch)
    }
}
