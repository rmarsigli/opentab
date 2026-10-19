use std::collections::BTreeMap;

use opentab_model::{
    Accent, Beat, DrumKit, Fraction, Grace, InstrumentKind, Note, NoteKind, Score, Slide, Track,
    Voice, gm_drum_kit,
};

use crate::allocate::ChannelNeed;
use crate::bends::{BendEvent, bend_events};
use crate::controls::is_guitar;
use crate::curves::{CENTS_PER_SEMITONE, Curve, MUSICAL_BEND_RANGE, Shape, Window};
use crate::exact::Exact;
use crate::harmonics;
use crate::midi::{Channel, MidiMessage, NOTE_OFF_VELOCITY, velocity_of};
use crate::order::{PlayOrder, PlayedBar};
use crate::perform::Profile;
use crate::swing::Swing;
use crate::{PlaybackError, PlaybackWarning, ScoreTime};

const MAX_KEY: u16 = 127;
const MIN_VELOCITY: i16 = 1;
const MAX_VELOCITY: i16 = 127;
pub(crate) const ACCENT_VELOCITY: i16 = 16;
pub(crate) const HEAVY_ACCENT_VELOCITY: i16 = 28;
pub(crate) const GHOST_VELOCITY: i16 = -28;
pub(crate) const PALM_MUTE_VELOCITY: i16 = -12;
pub(crate) const DEAD_VELOCITY: i16 = -8;
const PALM_MUTE_LENGTH: (i128, i128) = (3, 4);
const STACCATO_LENGTH: (i128, i128) = (1, 2);
const DEAD_LENGTH: Fraction = Fraction::new_raw(1, 32);
const LET_RING_LIMIT: Fraction = Fraction::new_raw(1, 1);

#[derive(Debug, Clone, Copy)]
pub(crate) struct Pending {
    pub(crate) at: Exact,
    pub(crate) rank: u8,
    pub(crate) order: usize,
    pub(crate) track: usize,
    pub(crate) staff: usize,
    pub(crate) channel: Channel,
    pub(crate) message: MidiMessage,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Lane {
    pub(crate) track: usize,
    pub(crate) staff: usize,
    pub(crate) channel: Channel,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Struck {
    pub(crate) track: usize,
    pub(crate) channel: Channel,
    pub(crate) key: u8,
    pub(crate) start: Exact,
    pub(crate) end: Exact,
    pub(crate) velocity: u8,
    pub(crate) expressive: bool,
    pub(crate) dropped: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Restrikes {
    pub cut: u32,
    pub merged: u32,
}

impl Restrikes {
    pub(crate) fn add(&mut self, other: Restrikes) {
        self.cut = self.cut.saturating_add(other.cut);
        self.merged = self.merged.saturating_add(other.merged);
    }
}

fn merge(notes: &mut [Struck], keep: usize, drop: usize) {
    let (kept, gone) = (notes[keep], notes[drop]);
    notes[keep].end = kept.end.max(gone.end);
    notes[keep].velocity = kept.velocity.max(gone.velocity);
    notes[keep].expressive = kept.expressive || gone.expressive;
    notes[drop].dropped = true;
}

pub(crate) fn restrike(
    notes: &mut [Struck],
    warnings: &mut Vec<PlaybackWarning>,
) -> Result<Restrikes, PlaybackError> {
    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by(|left, right| {
        let (a, b) = (&notes[*left], &notes[*right]);
        (a.channel, a.key, a.start, *left).cmp(&(b.channel, b.key, b.start, *right))
    });
    let mut counted = Restrikes::default();
    let mut previous: Option<usize> = None;
    for index in order {
        let current = notes[index];
        let Some(before) = previous.filter(|before| {
            let earlier = &notes[*before];
            earlier.channel == current.channel && earlier.key == current.key
        }) else {
            previous = Some(index);
            continue;
        };
        let earlier = notes[before];
        if earlier.end <= current.start {
            previous = Some(index);
        } else if earlier.start == current.start {
            let (keep, drop) = if current.expressive && !earlier.expressive {
                (index, before)
            } else {
                (before, index)
            };
            merge(notes, keep, drop);
            warnings.push(PlaybackWarning::MergedUnison {
                track: notes[drop].track,
                at: time_of(notes[drop].start)?,
            });
            counted.merged = counted.merged.saturating_add(1);
            previous = Some(keep);
        } else {
            notes[before].end = current.start;
            counted.cut = counted.cut.saturating_add(1);
            previous = Some(index);
        }
    }
    Ok(counted)
}

#[derive(Debug, Clone, Copy)]
struct PendingNote {
    struck: Struck,
    lane: Lane,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Sounded {
    pub(crate) start: Exact,
    pub(crate) end: Exact,
    pub(crate) key: u8,
    pub(crate) velocity: u8,
    pub(crate) expressive: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Stream {
    pending: Vec<Pending>,
    notes: Vec<PendingNote>,
}

impl Stream {
    pub(crate) fn note(&mut self, lane: Lane, sounded: Sounded) {
        self.notes.push(PendingNote {
            struck: Struck {
                track: lane.track,
                channel: lane.channel,
                key: sounded.key,
                start: sounded.start,
                end: sounded.end,
                velocity: sounded.velocity,
                expressive: sounded.expressive,
                dropped: false,
            },
            lane,
        });
    }

    pub(crate) fn finish(
        mut self,
        warnings: &mut Vec<PlaybackWarning>,
    ) -> Result<(Vec<Pending>, Restrikes), PlaybackError> {
        let mut struck: Vec<Struck> = self.notes.iter().map(|note| note.struck).collect();
        let counted = restrike(&mut struck, warnings)?;
        let notes = std::mem::take(&mut self.notes);
        for (note, struck) in notes.into_iter().zip(struck) {
            if struck.dropped {
                continue;
            }
            self.push(
                struck.start,
                note.lane,
                MidiMessage::NoteOn {
                    key: struck.key,
                    velocity: struck.velocity,
                },
            );
            self.push(
                struck.end,
                note.lane,
                MidiMessage::NoteOff {
                    key: struck.key,
                    velocity: NOTE_OFF_VELOCITY,
                },
            );
        }
        Ok((self.pending, counted))
    }

    pub(crate) fn push(&mut self, at: Exact, lane: Lane, message: MidiMessage) {
        let order = self.pending.len();
        self.pending.push(Pending {
            at,
            rank: message.rank(),
            order,
            track: lane.track,
            staff: lane.staff,
            channel: lane.channel,
            message,
        });
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StrandNote {
    pub(crate) start: Exact,
    pub(crate) end: Exact,
    pub(crate) key: u8,
    pub(crate) velocity: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct Strand {
    pub(crate) track: usize,
    pub(crate) staff: usize,
    pub(crate) muted: bool,
    pub(crate) bends: Vec<BendEvent>,
    pub(crate) notes: Vec<StrandNote>,
}

impl Strand {
    pub(crate) fn need(&self) -> ChannelNeed {
        if !self.bends.is_empty() {
            ChannelNeed::Exclusive
        } else if self.muted {
            ChannelNeed::Muted
        } else {
            ChannelNeed::Shared
        }
    }

    pub(crate) fn start(&self) -> Exact {
        self.notes
            .iter()
            .map(|note| note.start)
            .min()
            .unwrap_or(Exact::ZERO)
    }

    pub(crate) fn end(&self) -> Exact {
        self.notes
            .iter()
            .map(|note| note.end)
            .max()
            .unwrap_or(Exact::ZERO)
    }
}

#[derive(Debug, Clone, Copy)]
struct Sounding {
    staff: usize,
    start: Exact,
    end: Exact,
    key: u8,
    velocity: u8,
    strand: usize,
    string: Option<u8>,
    let_ring: bool,
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
    written_until: Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct StringKey {
    staff: usize,
    slot: u8,
    string: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Hammer,
    LegatoSlide,
    ShiftSlide,
}

#[derive(Debug, Clone, Copy)]
struct OpenLink {
    link: Link,
    sounding: usize,
    strand: usize,
    until: Exact,
    written_until: Exact,
    origin: Window,
    base_key: u8,
    cents: i64,
}

#[derive(Debug, Clone, Copy)]
struct Place {
    staff: usize,
    slot: u8,
    ties: bool,
    written_start: Exact,
    written_end: Exact,
}

impl Place {
    fn at(self, span: Span) -> Result<Place, PlaybackError> {
        Ok(Place {
            written_start: span.start,
            written_end: span.start.add(span.length)?,
            ..self
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct Span {
    start: Exact,
    length: Exact,
}

#[derive(Debug, Clone, Copy)]
struct Voicing {
    key: u8,
    velocity: u8,
    staff: usize,
    strand: usize,
    string: Option<u8>,
    let_ring: bool,
}

#[derive(Debug, Default)]
struct GracePlan<'a> {
    before: Vec<&'a Beat>,
    on: Vec<&'a Beat>,
}

pub(crate) struct TrackOutput {
    pub(crate) warnings: Vec<PlaybackWarning>,
    pub(crate) strands: Vec<Strand>,
}

pub(crate) struct TrackWriter<'a> {
    score: &'a Score,
    track: &'a Track,
    index: usize,
    channel: Channel,
    profile: Profile,
    kit: Option<DrumKit>,
    soundings: Vec<Sounding>,
    strands: Vec<Strand>,
    ties: BTreeMap<TieKey, OpenTie>,
    links: BTreeMap<StringKey, OpenLink>,
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

fn scaled(span: Span, ratio: (i128, i128)) -> Result<Span, PlaybackError> {
    Ok(Span {
        start: span.start,
        length: span.length.mul(Exact::new(ratio.0, ratio.1)?)?,
    })
}

pub(crate) fn musical_velocity(dynamic: u8, note: &Note) -> u8 {
    let effects = &note.effects;
    let mut offset: i16 = 0;
    offset += match effects.accent {
        Some(Accent::Normal) => ACCENT_VELOCITY,
        Some(Accent::Heavy) => HEAVY_ACCENT_VELOCITY,
        None => 0,
    };
    if effects.ghost {
        offset += GHOST_VELOCITY;
    }
    if effects.palm_mute {
        offset += PALM_MUTE_VELOCITY;
    }
    if effects.dead {
        offset += DEAD_VELOCITY;
    }
    let value = (i16::from(dynamic) + offset).clamp(MIN_VELOCITY, MAX_VELOCITY);
    u8::try_from(value).unwrap_or(1)
}

fn musical_span(span: Span, note: &Note) -> Result<Span, PlaybackError> {
    let effects = &note.effects;
    let mut sounding = span;
    if effects.staccato {
        sounding = scaled(sounding, STACCATO_LENGTH)?;
    }
    if effects.palm_mute {
        sounding = scaled(sounding, PALM_MUTE_LENGTH)?;
    }
    if effects.dead {
        sounding.length = sounding.length.min(Exact::of(DEAD_LENGTH));
    }
    Ok(sounding)
}

fn shape_of(note: &Note, beat: &Beat) -> Option<Shape> {
    if let Some(bend) = note.effects.bend {
        return Some(Shape {
            origin: bend.origin(),
            middle_start: bend.middle_start(),
            middle_end: bend.middle_end(),
            destination: bend.destination(),
        });
    }
    beat.effects
        .tremolo_bar
        .filter(|bar| bar.moves())
        .map(|bar| Shape {
            origin: bar.origin(),
            middle_start: bar.middle_start(),
            middle_end: bar.middle_end(),
            destination: bar.destination(),
        })
}

fn link_of(note: &Note) -> Option<Link> {
    match note.effects.slide {
        Some(Slide::Legato) => Some(Link::LegatoSlide),
        Some(Slide::Shift) => Some(Link::ShiftSlide),
        _ => note.effects.hammer_on_pull_off.then_some(Link::Hammer),
    }
}

impl<'a> TrackWriter<'a> {
    pub(crate) fn new(
        score: &'a Score,
        index: usize,
        track: &'a Track,
        channel: Channel,
        profile: Profile,
    ) -> TrackWriter<'a> {
        TrackWriter {
            score,
            track,
            index,
            channel,
            profile,
            kit: track.kit().cloned(),
            soundings: Vec::new(),
            strands: Vec::new(),
            ties: BTreeMap::new(),
            links: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    pub(crate) fn write(
        mut self,
        order: &PlayOrder,
        stream: &mut Stream,
    ) -> Result<TrackOutput, PlaybackError> {
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
        match self.profile {
            Profile::Gp => {
                for sounding in &self.soundings {
                    let lane = self.lane(sounding.staff);
                    stream.note(
                        lane,
                        Sounded {
                            start: sounding.start,
                            end: sounding.end,
                            key: sounding.key,
                            velocity: sounding.velocity,
                            expressive: false,
                        },
                    );
                }
                Ok(TrackOutput {
                    warnings: self.warnings,
                    strands: Vec::new(),
                })
            }
            Profile::Musical => {
                self.let_ring()?;
                let mut strands = std::mem::take(&mut self.strands);
                for sounding in &self.soundings {
                    if let Some(strand) = strands.get_mut(sounding.strand) {
                        strand.notes.push(StrandNote {
                            start: sounding.start,
                            end: sounding.end,
                            key: sounding.key,
                            velocity: sounding.velocity,
                        });
                    }
                }
                for strand in &mut strands {
                    let end = strand.end();
                    strand.bends.retain(|event| event.at < end);
                }
                strands.retain(|strand| !strand.notes.is_empty());
                Ok(TrackOutput {
                    warnings: self.warnings,
                    strands,
                })
            }
        }
    }

    fn let_ring(&mut self) -> Result<(), PlaybackError> {
        let limit = Exact::of(LET_RING_LIMIT);
        let mut onsets: BTreeMap<(usize, u8), Vec<Exact>> = BTreeMap::new();
        for sounding in &self.soundings {
            if let Some(string) = sounding.string {
                onsets
                    .entry((sounding.staff, string))
                    .or_default()
                    .push(sounding.start);
            }
        }
        for list in onsets.values_mut() {
            list.sort();
        }
        for sounding in &mut self.soundings {
            let (true, Some(string)) = (sounding.let_ring, sounding.string) else {
                continue;
            };
            let cap = sounding.end.add(limit)?;
            let next = onsets
                .get(&(sounding.staff, string))
                .and_then(|list| list.iter().find(|at| **at > sounding.start).copied())
                .map_or(cap, |next| next.min(cap));
            sounding.end = sounding.end.max(next);
        }
        Ok(())
    }

    fn lane(&self, staff: usize) -> Lane {
        Lane {
            track: self.index,
            staff,
            channel: self.channel,
        }
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
        let bar_start = Exact::of(played.start.whole_notes());
        let shift = self.pickup_shift(played, voice)?;
        let swing = Swing::of(
            self.score
                .master_bars
                .get(played.source)
                .map(|bar| bar.triplet_feel)
                .unwrap_or_default(),
        )?;
        let place = Place {
            staff,
            slot: voice.slot.get(),
            ties: true,
            written_start: Exact::ZERO,
            written_end: Exact::ZERO,
        };
        let mut offset = Exact::ZERO;
        let mut graces: Vec<&Beat> = Vec::new();
        for beat in &voice.beats {
            if beat.effects.grace.is_some() {
                graces.push(beat);
                continue;
            }
            let written = length_of(beat);
            let swung = Swing::apply(swing, shift.add(offset)?, written)?;
            let onset = bar_start.add(swung.start)?;
            let length = swung.length;
            let grace_shift = self.graces(&graces, onset, length, place, stream)?;
            graces.clear();
            let span = Span {
                start: onset.add(grace_shift)?,
                length: length.sub(grace_shift)?,
            };
            let written_start = bar_start.add(shift)?.add(offset)?;
            let beat_place = Place {
                written_start,
                written_end: written_start.add(written)?,
                ..place
            };
            self.beat(beat, span, beat_place, stream)?;
            offset = offset.add(written)?;
        }
        let end = bar_start.add(shift)?.add(offset)?;
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
            let span = Span { start: at, length };
            self.beat(beat, span, grace_place.at(span)?, stream)?;
            at = at.add(length)?;
        }
        let mut at = onset;
        for beat in &plan.on {
            let length = length_of(beat);
            let span = Span { start: at, length };
            self.beat(beat, span, grace_place.at(span)?, stream)?;
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
        match self.profile {
            Profile::Gp => self.gp_beat(beat, span, place, stream),
            Profile::Musical => self.musical_beat(beat, span, place),
        }
    }

    fn gp_beat(
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
                scaled(span, STACCATO_LENGTH)?
            } else {
                span
            };
            if self.track.instrument.kind == InstrumentKind::Fretted {
                let window = self.gp_vibrato_window(place, note, key, sounding)?;
                for event in bend_events(note, window.start, window.length)? {
                    stream.push(
                        event.at,
                        self.lane(place.staff),
                        MidiMessage::PitchBend { value: event.value },
                    );
                }
            }
            let voicing = Voicing {
                key,
                velocity,
                staff: place.staff,
                strand: 0,
                string: None,
                let_ring: false,
            };
            let auxiliary = self.trill_key(note, span.start)?;
            if !self.pieces(note, beat, auxiliary, sounding, voicing)? {
                let tie = place.ties.then_some((place, note, span));
                self.sound(sounding, voicing, tie);
            }
        }
        Ok(())
    }

    fn gp_vibrato_window(
        &self,
        place: Place,
        note: &Note,
        key: u8,
        sounding: Span,
    ) -> Result<Span, PlaybackError> {
        if !place.ties || note.effects.vibrato.is_none() || note.effects.bend.is_some() {
            return Ok(sounding);
        }
        let Some(merged) = self
            .tie_of(place, note, key, sounding.start)
            .and_then(|index| self.soundings.get(index))
        else {
            return Ok(sounding);
        };
        let end = sounding.start.add(sounding.length)?;
        Ok(Span {
            start: merged.start,
            length: end.sub(merged.start)?,
        })
    }

    fn pieces(
        &mut self,
        note: &Note,
        beat: &Beat,
        auxiliary: Option<u8>,
        sounding: Span,
        voicing: Voicing,
    ) -> Result<bool, PlaybackError> {
        if let Some(auxiliary) = auxiliary {
            let speed = note
                .effects
                .trill
                .map(|trill| trill.speed.value().denominator())
                .unwrap_or(1);
            for (index, piece) in pieces(sounding, Fraction::new(1, speed))?
                .into_iter()
                .enumerate()
            {
                let key = if index % 2 == 0 {
                    voicing.key
                } else {
                    auxiliary
                };
                self.sound(piece, Voicing { key, ..voicing }, None);
            }
            return Ok(true);
        }
        if let Some(tremolo) = beat.effects.tremolo_picking {
            let value = Fraction::new(1, tremolo.value().denominator());
            for piece in pieces(sounding, value)? {
                self.sound(piece, voicing, None);
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn musical_beat(&mut self, beat: &Beat, span: Span, place: Place) -> Result<(), PlaybackError> {
        let dynamic = velocity_of(beat.effects.dynamic);
        let fretted = self.track.instrument.kind == InstrumentKind::Fretted;
        let guitar = fretted && is_guitar(self.track.instrument.midi_program);
        for note in &beat.notes {
            let Some(key) = self.key_of(note, span.start)? else {
                continue;
            };
            let sounding = musical_span(span, note)?;
            let string = match note.kind {
                NoteKind::Fretted { string, .. } => Some(string),
                NoteKind::Percussion { .. } => None,
            };
            let string_key = string.map(|string| StringKey {
                staff: place.staff,
                slot: place.slot,
                string,
            });
            let open = string_key
                .and_then(|found| self.links.remove(&found))
                .filter(|open| {
                    open.until == span.start || open.written_until == place.written_start
                });
            let auxiliary = self.trill_key(note, span.start)?;
            let simple = auxiliary.is_none() && beat.effects.tremolo_picking.is_none();
            let tie = (place.ties && simple)
                .then(|| self.tie_of(place, note, key, span.start))
                .flatten();
            let mut voicing = Voicing {
                key,
                velocity: musical_velocity(dynamic, note),
                staff: place.staff,
                strand: 0,
                string,
                let_ring: note.effects.let_ring,
            };
            let window = Window {
                start: sounding.start,
                end: sounding.start.add(sounding.length)?,
            };
            if let Some(open) = open
                && open.link == Link::ShiftSlide
                && fretted
            {
                let reach = i64::from(MUSICAL_BEND_RANGE) * CENTS_PER_SEMITONE;
                let target = ((i64::from(key) - i64::from(open.base_key)) * CENTS_PER_SEMITONE)
                    .clamp(-reach, reach);
                self.glide_into(open, target)?;
            }
            let joined = open.filter(|open| {
                open.link != Link::ShiftSlide
                    && fretted
                    && simple
                    && tie.is_none()
                    && (i64::from(key) - i64::from(open.base_key)).abs()
                        <= i64::from(MUSICAL_BEND_RANGE)
            });
            if let Some(open) = joined {
                let cents = (i64::from(key) - i64::from(open.base_key)) * CENTS_PER_SEMITONE;
                match open.link {
                    Link::Hammer => {
                        let mut curve = Curve::default();
                        curve.point(span.start, cents);
                        self.extend_strand(open.strand, curve);
                    }
                    Link::LegatoSlide | Link::ShiftSlide => self.glide_into(open, cents)?,
                }
                self.curves(open.strand, note, beat, window, cents)?;
                if let Some(merged) = self.soundings.get_mut(open.sounding) {
                    merged.end = merged.end.max(window.end);
                    merged.let_ring |= note.effects.let_ring;
                }
                if let (Some(link), Some(found)) = (link_of(note), string_key) {
                    self.links.insert(
                        found,
                        OpenLink {
                            link,
                            sounding: open.sounding,
                            strand: open.strand,
                            until: span.start.add(span.length)?,
                            written_until: place.written_end,
                            origin: window,
                            base_key: open.base_key,
                            cents,
                        },
                    );
                }
                continue;
            }
            let strand = match tie.and_then(|index| self.soundings.get(index)) {
                Some(merged) => merged.strand,
                None => {
                    self.strands.push(Strand {
                        track: self.index,
                        staff: place.staff,
                        muted: guitar && (note.effects.palm_mute || note.effects.dead),
                        bends: Vec::new(),
                        notes: Vec::new(),
                    });
                    self.strands.len() - 1
                }
            };
            voicing.strand = strand;
            if fretted {
                self.curves(strand, note, beat, window, 0)?;
            }
            if !self.pieces(note, beat, auxiliary, sounding, voicing)? {
                self.sound(sounding, voicing, place.ties.then_some((place, note, span)));
            }
            if let (Some(link), Some(found)) = (link_of(note), string_key)
                && simple
            {
                let sounding_index = self.soundings.len().saturating_sub(1);
                let sounding_index = tie.unwrap_or(sounding_index);
                self.links.insert(
                    found,
                    OpenLink {
                        link,
                        sounding: sounding_index,
                        strand,
                        until: span.start.add(span.length)?,
                        written_until: place.written_end,
                        origin: window,
                        base_key: key,
                        cents: 0,
                    },
                );
            }
        }
        Ok(())
    }

    fn glide_into(&mut self, open: OpenLink, target: i64) -> Result<(), PlaybackError> {
        let origin = Window {
            start: open.origin.start,
            end: open.until.max(open.origin.start),
        };
        let mut curve = Curve::default();
        curve.towards(origin, open.cents, target)?;
        self.extend_strand(open.strand, curve);
        Ok(())
    }

    fn extend_strand(&mut self, strand: usize, curve: Curve) {
        if let Some(found) = self.strands.get_mut(strand) {
            found.bends.extend(curve.events);
        }
    }

    fn curves(
        &mut self,
        strand: usize,
        note: &Note,
        beat: &Beat,
        window: Window,
        base: i64,
    ) -> Result<(), PlaybackError> {
        let mut curve = Curve::default();
        let mut settled = window.start;
        if let Some(shape) = shape_of(note, beat) {
            settled = curve.shape(window, shape, base)?;
        } else if let Some(slide) = note.effects.slide {
            curve.slide(window, slide, base)?;
        }
        if let Some(vibrato) = note.effects.vibrato {
            let tail = Window {
                start: settled,
                end: window.end,
            };
            let base = if settled == window.start {
                if curve.events.is_empty() {
                    curve.point(window.start, base);
                }
                base
            } else {
                shape_of(note, beat)
                    .map_or(base, |shape| base + i64::from(shape.destination.value()))
            };
            curve.vibrato(tail, vibrato, base)?;
        }
        self.extend_strand(strand, curve);
        Ok(())
    }

    fn tie_of(&self, place: Place, note: &Note, key: u8, start: Exact) -> Option<usize> {
        if !note.effects.tie_destination {
            return None;
        }
        let tie_key = TieKey {
            staff: place.staff,
            slot: place.slot,
            key,
        };
        self.ties
            .get(&tie_key)
            .filter(|open| open.until == start || open.written_until == place.written_start)
            .map(|open| open.sounding)
    }

    fn sound(&mut self, span: Span, voicing: Voicing, tie: Option<(Place, &Note, Span)>) {
        let Ok(end) = span.start.add(span.length) else {
            return;
        };
        if end <= span.start {
            return;
        }
        let fresh = Sounding {
            staff: voicing.staff,
            start: span.start,
            end,
            key: voicing.key,
            velocity: voicing.velocity,
            strand: voicing.strand,
            string: voicing.string,
            let_ring: voicing.let_ring,
        };
        let Some((place, note, written)) = tie else {
            self.soundings.push(fresh);
            return;
        };
        let tie_key = TieKey {
            staff: place.staff,
            slot: place.slot,
            key: voicing.key,
        };
        let until = written.start.add(written.length).unwrap_or(end);
        if let Some(index) = self.tie_of(place, note, voicing.key, span.start)
            && let Some(merged) = self.soundings.get_mut(index)
        {
            merged.end = end.max(merged.end);
            merged.let_ring |= voicing.let_ring;
            if note.effects.tie_origin {
                self.ties.insert(
                    tie_key,
                    OpenTie {
                        sounding: index,
                        until,
                        written_until: place.written_end,
                    },
                );
            } else {
                self.ties.remove(&tie_key);
            }
            return;
        }
        self.soundings.push(fresh);
        if note.effects.tie_origin {
            self.ties.insert(
                tie_key,
                OpenTie {
                    sounding: self.soundings.len() - 1,
                    until,
                    written_until: place.written_end,
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
                    harmonics::sounding(open, fretted, harmonic, self.profile)
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
