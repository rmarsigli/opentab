use std::sync::Arc;

use opentab_model::Fraction;
use opentab_playback::{
    Channel, ChannelState, MidiMessage, NOTE_OFF_VELOCITY, Performance, PlayOrder, PlayedPosition,
    ScoreTime, Seconds, Speed,
};

use crate::chase::ChaseIndex;
use crate::command::{
    BarRange, CommandQueue, CommandReceiver, CommandSender, QueueFull, TransportCommand,
    command_queue,
};
use crate::render::{BLOCK_FRAMES, from_bus, to_bus};
use crate::session::Session;
use crate::status::{StatusCell, TransportState, TransportStatus};
use crate::timeline::{Timeline, frames_until, is_due, step_of};
use crate::{AudioError, Limiter, LimiterSettings, SampleRate, SoundUse, Synth};

const CHANNELS: usize = 16;
const KEYS: usize = 128;
const PERCENT: i128 = 100;
pub const CURSOR_STEP: Fraction = Fraction::new_raw(1, 3840);
pub const CLICK_DOWNBEAT_KEY: u8 = 76;
pub const CLICK_BEAT_KEY: u8 = 77;
pub const CLICK_DOWNBEAT_VELOCITY: u8 = 120;
pub const CLICK_BEAT_VELOCITY: u8 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionConfig {
    pub rate: SampleRate,
    pub speed: Speed,
    pub metronome: bool,
    pub count_in: bool,
}

impl Default for SessionConfig {
    fn default() -> SessionConfig {
        SessionConfig {
            rate: SampleRate::default(),
            speed: Speed::NORMAL,
            metronome: false,
            count_in: false,
        }
    }
}

pub fn played_range(order: &PlayOrder, sources: BarRange) -> Option<BarRange> {
    if sources.first > sources.last {
        return None;
    }
    let inside = |index: &usize| {
        order
            .bars
            .get(*index)
            .is_some_and(|bar| (sources.first..=sources.last).contains(&bar.source))
    };
    let first = (0..order.bars.len()).find(inside)?;
    let last = (first..order.bars.len())
        .take_while(inside)
        .last()
        .unwrap_or(first);
    Some(BarRange { first, last })
}

pub fn range_frames(
    performance: &Performance,
    range: BarRange,
    speed: Speed,
    rate: SampleRate,
) -> Result<u64, AudioError> {
    let timeline = Timeline::new(performance, rate)?;
    let (Some(start), Some(end)) = (
        timeline.bars.get(range.first),
        timeline.bars.get(range.last.saturating_add(1)),
    ) else {
        return Err(AudioError::BarRange {
            first: range.first,
            last: range.last,
            bars: timeline.bar_count(),
        });
    };
    if range.first > range.last {
        return Err(AudioError::BarRange {
            first: range.first,
            last: range.last,
            bars: timeline.bar_count(),
        });
    }
    Ok(frames_until(*end, *start, step_of(speed)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sounding {
    keys: [[u8; KEYS]; CHANNELS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoopSpan {
    range: BarRange,
    end: ScoreTime,
    end_unit: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CountInRun {
    elapsed: u64,
    length: u64,
    origin: u64,
    lead: u64,
    first_click: usize,
    split_click: usize,
    end_click: usize,
    struck: usize,
    settled: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BarLead {
    bar: usize,
    track: usize,
    lead: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CountInClick {
    index: usize,
    due: u64,
}

impl CountInRun {
    fn next(&self, units: &[u64]) -> Option<CountInClick> {
        let late = self.end_click.saturating_sub(self.split_click);
        let (index, wrap) = if self.struck < late {
            (self.split_click + self.struck, 0)
        } else {
            (self.first_click + (self.struck - late), self.length)
        };
        let limit = if wrap == 0 {
            self.end_click
        } else {
            self.split_click
        };
        if index >= limit {
            return None;
        }
        let unit = *units.get(index)?;
        let due = unit
            .saturating_sub(self.origin)
            .saturating_add(wrap)
            .saturating_sub(self.lead);
        Some(CountInClick { index, due })
    }
}

pub struct Transport {
    performance: Performance,
    timeline: Timeline,
    voices: Vec<Box<dyn Synth>>,
    click: Box<dyn Synth>,
    rate: SampleRate,
    commands: CommandReceiver,
    status: Arc<StatusCell>,
    state: TransportState,
    step: u64,
    position: u64,
    next_event: usize,
    next_click: usize,
    bar: usize,
    span: Option<LoopSpan>,
    wrapped: bool,
    count_in: Option<CountInRun>,
    leads: Vec<BarLead>,
    metronome: bool,
    counts_in: bool,
    chase_at: Option<ScoreTime>,
    muted: Vec<bool>,
    soloed: Vec<bool>,
    sounding: Vec<Sounding>,
    chases: ChaseIndex,
    chase_steps: u64,
    visited: Vec<bool>,
    clock: u64,
    scratch_left: Vec<f32>,
    scratch_right: Vec<f32>,
    bus_left: Vec<i64>,
    bus_right: Vec<i64>,
    limiter: Limiter,
}

pub struct TransportHandle {
    commands: CommandSender,
    status: Arc<StatusCell>,
}

impl TransportHandle {
    pub fn send(&mut self, command: TransportCommand) -> Result<(), QueueFull> {
        self.commands.send(command)
    }

    pub fn status(&self) -> TransportStatus {
        self.status.read()
    }
}

fn sound_uses(performance: &Performance, track: usize) -> Vec<SoundUse> {
    let mut states = [ChannelState::default(); CHANNELS];
    if let Some(setup) = performance.tracks.get(track) {
        for entry in setup.channels.iter().rev() {
            if let Some(state) = states.get_mut(usize::from(entry.channel.get())) {
                *state = entry.state;
            }
        }
    }
    let mut uses: Vec<SoundUse> = Vec::new();
    for event in performance
        .events
        .iter()
        .filter(|event| event.track == track)
    {
        let Some(state) = states.get_mut(usize::from(event.channel.get())) else {
            continue;
        };
        state.apply(event.message);
        if let MidiMessage::NoteOn { velocity, .. } = event.message
            && velocity > 0
        {
            let found = SoundUse {
                channel: event.channel,
                bank: state.bank,
                program: state.program,
            };
            if !uses.contains(&found) {
                uses.push(found);
            }
        }
    }
    uses
}

fn lead_ins(performance: &Performance, timeline: &Timeline) -> Vec<BarLead> {
    let bars = timeline.bar_count();
    let mut leads: Vec<BarLead> = Vec::new();
    let mut marked: Vec<Option<usize>> = vec![None; performance.tracks.len()];
    let mut bar = 0;
    for (event, unit) in performance.events.iter().zip(&timeline.events) {
        let MidiMessage::NoteOn { velocity, .. } = event.message else {
            continue;
        };
        if velocity == 0 {
            continue;
        }
        while bar + 1 < bars
            && timeline
                .bars
                .get(bar + 1)
                .is_some_and(|start| *start <= *unit)
        {
            bar += 1;
        }
        let (Some(start), Some(end)) = (timeline.bars.get(bar), timeline.bars.get(bar + 1)) else {
            continue;
        };
        if let Some(mark) = marked.get_mut(event.track)
            && *mark != Some(bar)
            && *start <= *unit
            && *unit < *end
        {
            *mark = Some(bar);
            leads.push(BarLead {
                bar,
                track: event.track,
                lead: unit - start,
            });
        }
    }
    leads
}

fn audible(muted: bool, soloed: bool, soloing: bool) -> bool {
    !muted && (!soloing || soloed)
}

fn strike(click: &mut dyn Synth, downbeat: bool) {
    let (key, velocity) = if downbeat {
        (CLICK_DOWNBEAT_KEY, CLICK_DOWNBEAT_VELOCITY)
    } else {
        (CLICK_BEAT_KEY, CLICK_BEAT_VELOCITY)
    };
    click.handle(Channel::DRUMS, &MidiMessage::NoteOn { key, velocity });
    click.handle(
        Channel::DRUMS,
        &MidiMessage::NoteOff {
            key,
            velocity: NOTE_OFF_VELOCITY,
        },
    );
}

fn add_to_bus(bus: &mut [i64], samples: &[f32]) {
    for (target, sample) in bus.iter_mut().zip(samples) {
        *target = target.saturating_add(to_bus(*sample));
    }
}

impl Transport {
    pub(crate) fn create(
        performance: Performance,
        mut voices: Vec<Box<dyn Synth>>,
        mut click: Box<dyn Synth>,
        config: SessionConfig,
    ) -> Result<Session, AudioError> {
        if voices.len() != performance.tracks.len() {
            return Err(AudioError::VoiceCount {
                voices: voices.len(),
                tracks: performance.tracks.len(),
            });
        }
        let timeline = Timeline::new(&performance, config.rate)?;
        for (track, voice) in voices.iter_mut().enumerate() {
            voice.reset();
            voice.prepare(&sound_uses(&performance, track));
        }
        click.reset();
        let tracks = performance.tracks.len();
        let bars = timeline.bar_count();
        let leads = lead_ins(&performance, &timeline);
        let chases = ChaseIndex::new(&performance);
        let CommandQueue { sender, receiver } = command_queue();
        let status = Arc::new(StatusCell::new());
        let mut transport = Transport {
            performance,
            timeline,
            voices,
            click,
            rate: config.rate,
            commands: receiver,
            status: Arc::clone(&status),
            state: TransportState::Stopped,
            step: step_of(config.speed),
            position: 0,
            next_event: 0,
            next_click: 0,
            bar: 0,
            span: None,
            wrapped: false,
            count_in: None,
            leads,
            metronome: config.metronome,
            counts_in: config.count_in,
            chase_at: None,
            muted: vec![false; tracks],
            soloed: vec![false; tracks],
            sounding: vec![
                Sounding {
                    keys: [[0; KEYS]; CHANNELS]
                };
                tracks
            ],
            chases,
            chase_steps: 0,
            visited: vec![false; bars],
            clock: 0,
            scratch_left: vec![0.0; BLOCK_FRAMES],
            scratch_right: vec![0.0; BLOCK_FRAMES],
            bus_left: vec![0; BLOCK_FRAMES],
            bus_right: vec![0; BLOCK_FRAMES],
            limiter: Limiter::new(LimiterSettings::DEFAULT, config.rate),
        };
        transport.publish();
        Ok(Session {
            transport,
            handle: TransportHandle {
                commands: sender,
                status,
            },
        })
    }

    pub fn rate(&self) -> SampleRate {
        self.rate
    }

    pub fn performance(&self) -> &Performance {
        &self.performance
    }

    pub fn clock(&self) -> u64 {
        self.clock
    }

    pub fn chase_steps(&self) -> u64 {
        self.chase_steps
    }

    pub fn played_bars(&self) -> Vec<usize> {
        self.visited
            .iter()
            .enumerate()
            .filter(|(_, visited)| **visited)
            .map(|(index, _)| index)
            .collect()
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        let (left, left_rest) = left.split_at_mut(frames);
        let (right, right_rest) = right.split_at_mut(frames);
        left_rest.fill(0.0);
        right_rest.fill(0.0);
        self.apply_commands();
        let mut done = 0;
        while done < frames {
            self.settle();
            let remaining = frames - done;
            let run = usize::try_from(self.next_run())
                .unwrap_or(usize::MAX)
                .clamp(1, remaining.min(BLOCK_FRAMES));
            self.mix(&mut left[done..done + run], &mut right[done..done + run]);
            self.advance(run as u64);
            done += run;
        }
        self.clock = self.clock.saturating_add(frames as u64);
        self.publish();
    }

    fn apply_commands(&mut self) {
        for _ in 0..self.commands.pending() {
            let Some(word) = self.commands.receive() else {
                break;
            };
            if let Some(command) = word {
                self.apply(command);
            }
        }
        if self.running() {
            self.chase_now();
        }
    }

    fn running(&self) -> bool {
        matches!(
            self.state,
            TransportState::Playing | TransportState::CountIn
        )
    }

    fn apply(&mut self, command: TransportCommand) {
        match command {
            TransportCommand::Play => {
                if matches!(self.state, TransportState::Stopped | TransportState::Paused) {
                    self.start();
                }
            }
            TransportCommand::Pause => {
                if self.running() {
                    self.silence();
                    self.count_in = None;
                    self.state = TransportState::Paused;
                }
            }
            TransportCommand::Stop => {
                self.silence();
                self.count_in = None;
                self.state = TransportState::Stopped;
                let home = self.span.map_or(0, |span| span.range.first);
                if home < self.timeline.bar_count() {
                    self.move_to(home);
                }
            }
            TransportCommand::Seek(bar) => {
                if bar < self.timeline.bar_count() {
                    if self
                        .span
                        .is_some_and(|span| !(span.range.first..=span.range.last).contains(&bar))
                    {
                        self.span = None;
                    }
                    self.move_to(bar);
                    self.recount();
                }
            }
            TransportCommand::Loop(range) => self.set_loop(range),
            TransportCommand::ClearLoop => self.span = None,
            TransportCommand::Speed(speed) => self.step = step_of(speed),
            TransportCommand::Mute(track, muted) => {
                if let Some(flag) = self.muted.get_mut(track) {
                    *flag = muted;
                }
                self.reaim();
            }
            TransportCommand::Solo(track, soloed) => {
                if let Some(flag) = self.soloed.get_mut(track) {
                    *flag = soloed;
                }
                self.reaim();
            }
            TransportCommand::Metronome(on) => self.metronome = on,
            TransportCommand::CountIn(on) => self.counts_in = on,
        }
    }

    fn set_loop(&mut self, range: BarRange) {
        if range.first > range.last || range.last >= self.timeline.bar_count() {
            return;
        }
        let (Some(start_unit), Some(end_unit), Some(end)) = (
            self.timeline.bars.get(range.first).copied(),
            self.timeline.bars.get(range.last + 1).copied(),
            self.end_of_bar(range.last),
        ) else {
            return;
        };
        if end_unit <= start_unit {
            return;
        }
        self.span = Some(LoopSpan {
            range,
            end,
            end_unit,
        });
        if !(range.first..=range.last).contains(&self.bar) {
            self.move_to(range.first);
            self.recount();
        }
    }

    fn end_of_bar(&self, bar: usize) -> Option<ScoreTime> {
        let bars = &self.performance.order.bars;
        match bars.get(bar + 1) {
            Some(next) => Some(next.start),
            None => bars.get(bar).map(|_| self.performance.order.end()),
        }
    }

    fn start(&mut self) {
        self.wrapped = false;
        self.begin_count_in();
    }

    fn recount(&mut self) {
        if self.state == TransportState::CountIn {
            self.begin_count_in();
        }
    }

    fn begin_count_in(&mut self) {
        self.count_in = if self.counts_in {
            self.count_in_run()
        } else {
            None
        };
        self.state = if self.count_in.is_some() {
            TransportState::CountIn
        } else {
            TransportState::Playing
        };
    }

    fn count_in_run(&self) -> Option<CountInRun> {
        let bar = self.bar;
        let origin = *self.timeline.bars.get(bar)?;
        let end = *self.timeline.bars.get(bar + 1)?;
        let length = end.checked_sub(origin).filter(|length| *length > 0)?;
        let first_click = *self.timeline.bar_clicks.get(bar)?;
        let end_click = *self.timeline.bar_clicks.get(bar + 1)?;
        let lead = if self.position == origin {
            self.audible_lead(bar)
        } else {
            0
        };
        let entry = origin.saturating_add(lead);
        let split_click = first_click
            + self
                .timeline
                .clicks
                .get(first_click..end_click)
                .map_or(0, |units| units.partition_point(|unit| *unit < entry));
        Some(CountInRun {
            elapsed: 0,
            length,
            origin,
            lead,
            first_click,
            split_click,
            end_click,
            struck: 0,
            settled: None,
        })
    }

    fn reaim(&mut self) {
        if self.state != TransportState::CountIn {
            return;
        }
        let Some(fresh) = self.count_in_run() else {
            return;
        };
        let step = self.step;
        let Some(run) = self.count_in.as_mut() else {
            return;
        };
        run.lead = fresh.lead;
        run.split_click = fresh.split_click;
        run.struck = 0;
        let Some(settled) = run.settled else {
            return;
        };
        while let Some(next) = run.next(&self.timeline.clicks) {
            if !is_due(next.due, settled, step) {
                break;
            }
            run.struck += 1;
        }
    }

    fn track_audible(&self, track: usize, soloing: bool) -> bool {
        let muted = self.muted.get(track).copied().unwrap_or(false);
        let soloed = self.soloed.get(track).copied().unwrap_or(false);
        audible(muted, soloed, soloing)
    }

    fn audible_lead(&self, bar: usize) -> u64 {
        let soloing = self.soloed.iter().any(|soloed| *soloed);
        let first = self.leads.partition_point(|entry| entry.bar < bar);
        let last = self.leads.partition_point(|entry| entry.bar <= bar);
        self.leads
            .get(first..last)
            .unwrap_or(&[])
            .iter()
            .filter(|entry| self.track_audible(entry.track, soloing))
            .map(|entry| entry.lead)
            .min()
            .unwrap_or(0)
    }

    fn move_to(&mut self, bar: usize) {
        let Some((start, unit)) = self
            .performance
            .order
            .bars
            .get(bar)
            .map(|played| played.start)
            .zip(self.timeline.bars.get(bar).copied())
        else {
            return;
        };
        self.silence();
        self.position = unit;
        self.bar = bar;
        self.next_event = self
            .performance
            .events
            .partition_point(|event| event.at < start);
        self.next_click = self
            .performance
            .clicks
            .partition_point(|click| click.at < start);
        self.chase_at = Some(start);
    }

    fn chase_now(&mut self) {
        if let Some(at) = self.chase_at.take() {
            self.chase(at);
        }
    }

    fn chase(&mut self, at: ScoreTime) {
        let voices = &mut self.voices;
        let steps = self.chases.restore(&self.performance, at, |chased| {
            if let Some(voice) = voices.get_mut(chased.track) {
                voice.handle(chased.channel, &chased.message);
            }
        });
        self.chase_steps = self.chase_steps.saturating_add(steps);
    }

    fn silence(&mut self) {
        for (sounding, voice) in self.sounding.iter_mut().zip(self.voices.iter_mut()) {
            for (channel, keys) in (0u8..).zip(sounding.keys.iter_mut()) {
                let Some(channel) = Channel::new(channel) else {
                    continue;
                };
                for (key, count) in (0u8..).zip(keys.iter_mut()) {
                    while *count > 0 {
                        voice.handle(
                            channel,
                            &MidiMessage::NoteOff {
                                key,
                                velocity: NOTE_OFF_VELOCITY,
                            },
                        );
                        *count -= 1;
                    }
                }
            }
        }
    }

    fn dispatch(&mut self, index: usize) {
        let Some(event) = self.performance.events.get(index).copied() else {
            return;
        };
        let (Some(voice), Some(sounding)) = (
            self.voices.get_mut(event.track),
            self.sounding.get_mut(event.track),
        ) else {
            return;
        };
        let channel = usize::from(event.channel.get());
        match event.message {
            MidiMessage::NoteOn { key, velocity } if velocity > 0 => {
                if let Some(count) = sounding
                    .keys
                    .get_mut(channel)
                    .and_then(|keys| keys.get_mut(usize::from(key)))
                {
                    *count = count.saturating_add(1);
                }
                voice.handle(event.channel, &event.message);
            }
            MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                if let Some(count) = sounding
                    .keys
                    .get_mut(channel)
                    .and_then(|keys| keys.get_mut(usize::from(key)))
                    && *count > 0
                {
                    *count -= 1;
                    voice.handle(event.channel, &event.message);
                }
            }
            _ => voice.handle(event.channel, &event.message),
        }
    }

    fn settle(&mut self) {
        loop {
            let again = match self.state {
                TransportState::CountIn => self.settle_count_in(),
                TransportState::Playing => self.settle_playing(),
                TransportState::Stopped | TransportState::Paused => false,
            };
            if !again {
                return;
            }
        }
    }

    fn enter(&mut self, run: CountInRun) {
        if run.lead > 0 {
            let entry = run.origin.saturating_add(run.lead);
            self.position = entry;
            self.next_click = self.timeline.clicks.partition_point(|unit| *unit < entry);
        }
    }

    fn settle_count_in(&mut self) -> bool {
        let step = self.step;
        let Some(run) = self.count_in.as_mut() else {
            self.state = TransportState::Playing;
            return true;
        };
        while let Some(next) = run.next(&self.timeline.clicks) {
            let Some(click) = self.performance.clicks.get(next.index) else {
                break;
            };
            if !is_due(next.due, run.elapsed, step) {
                break;
            }
            strike(self.click.as_mut(), click.downbeat);
            run.struck += 1;
        }
        run.settled = Some(run.elapsed);
        if is_due(run.length, run.elapsed, step) {
            let ended = *run;
            self.count_in = None;
            self.enter(ended);
            self.state = TransportState::Playing;
            return true;
        }
        false
    }

    fn visit(&mut self) {
        let ceiling = match self.span {
            Some(span) if self.bar <= span.range.last => span.range.last + 1,
            _ => self.timeline.bar_count(),
        };
        while self.bar + 1 < ceiling
            && self
                .timeline
                .bars
                .get(self.bar + 1)
                .is_some_and(|unit| is_due(*unit, self.position, self.step))
        {
            self.bar += 1;
        }
        if let Some(visited) = self.visited.get_mut(self.bar) {
            *visited = true;
        }
    }

    fn settle_playing(&mut self) -> bool {
        self.visit();
        let (position, step) = (self.position, self.step);
        let limit = self.span.map(|span| span.end);
        while let (Some(unit), Some(event)) = (
            self.timeline.events.get(self.next_event),
            self.performance.events.get(self.next_event),
        ) {
            if !is_due(*unit, position, step) || limit.is_some_and(|end| event.at >= end) {
                break;
            }
            self.dispatch(self.next_event);
            self.next_event += 1;
        }
        while let (Some(unit), Some(click)) = (
            self.timeline.clicks.get(self.next_click),
            self.performance.clicks.get(self.next_click),
        ) {
            if !is_due(*unit, position, step) || limit.is_some_and(|end| click.at >= end) {
                break;
            }
            if self.metronome {
                strike(self.click.as_mut(), click.downbeat);
            }
            self.next_click += 1;
        }
        let ended = self.next_event >= self.timeline.events.len()
            && is_due(self.timeline.end, position, step);
        if let Some(span) = self.span {
            if !self.wrapped && (ended || is_due(span.end_unit, position, step)) {
                self.move_to(span.range.first);
                self.chase_now();
                self.wrapped = true;
                return true;
            }
            return false;
        }
        if ended {
            self.silence();
            self.state = TransportState::Stopped;
            if self.timeline.bar_count() > 0 {
                self.move_to(0);
            }
        }
        false
    }

    fn next_run(&self) -> u64 {
        let step = self.step;
        match self.state {
            TransportState::Playing => {
                let position = self.position;
                let mut next = u64::MAX;
                if let Some(unit) = self.timeline.events.get(self.next_event) {
                    next = next.min(frames_until(*unit, position, step));
                } else {
                    next = next.min(frames_until(self.timeline.end, position, step));
                }
                if let Some(unit) = self.timeline.clicks.get(self.next_click) {
                    next = next.min(frames_until(*unit, position, step));
                }
                if let Some(span) = self.span {
                    next = next.min(frames_until(span.end_unit, position, step));
                }
                next
            }
            TransportState::CountIn => self.count_in.map_or(u64::MAX, |run| {
                let mut next = frames_until(run.length, run.elapsed, step);
                if let Some(click) = run.next(&self.timeline.clicks) {
                    next = next.min(frames_until(click.due, run.elapsed, step));
                }
                next
            }),
            TransportState::Stopped | TransportState::Paused => u64::MAX,
        }
    }

    fn advance(&mut self, frames: u64) {
        let units = frames.saturating_mul(self.step);
        match self.state {
            TransportState::Playing => {
                self.position = self.position.saturating_add(units);
                self.wrapped = false;
            }
            TransportState::CountIn => {
                if let Some(run) = self.count_in.as_mut() {
                    run.elapsed = run.elapsed.saturating_add(units);
                }
            }
            TransportState::Stopped | TransportState::Paused => {}
        }
    }

    fn mix(&mut self, left: &mut [f32], right: &mut [f32]) {
        let count = left.len().min(right.len()).min(BLOCK_FRAMES);
        let soloing = self.soloed.iter().any(|soloed| *soloed);
        let (Some(bus_left), Some(bus_right), Some(scratch_left), Some(scratch_right)) = (
            self.bus_left.get_mut(..count),
            self.bus_right.get_mut(..count),
            self.scratch_left.get_mut(..count),
            self.scratch_right.get_mut(..count),
        ) else {
            return;
        };
        bus_left.fill(0);
        bus_right.fill(0);
        for (track, voice) in self.voices.iter_mut().enumerate() {
            voice.render(scratch_left, scratch_right);
            let muted = self.muted.get(track).copied().unwrap_or(false);
            let soloed = self.soloed.get(track).copied().unwrap_or(false);
            if audible(muted, soloed, soloing) {
                add_to_bus(bus_left, scratch_left);
                add_to_bus(bus_right, scratch_right);
            }
        }
        self.click.render(scratch_left, scratch_right);
        add_to_bus(bus_left, scratch_left);
        add_to_bus(bus_right, scratch_right);
        self.limiter.process(bus_left, bus_right);
        for (target, sample) in left.iter_mut().zip(bus_left.iter()) {
            *target = from_bus(*sample);
        }
        for (target, sample) in right.iter_mut().zip(bus_right.iter()) {
            *target = from_bus(*sample);
        }
    }

    fn cursor(&self) -> PlayedPosition {
        let order = &self.performance.order;
        let Some(last) = order.bars.last() else {
            return PlayedPosition {
                played_bar: 0,
                source_bar: 0,
                offset: Fraction::new_raw(0, 1),
            };
        };
        let reached = self.position.saturating_add(self.step).saturating_sub(1);
        let found = Seconds::new(i128::from(reached), i128::from(self.rate.hz()) * PERCENT)
            .ok()
            .and_then(|seconds| {
                self.performance
                    .tempo
                    .time_at_floor(seconds, Speed::NORMAL, CURSOR_STEP)
                    .ok()
            })
            .and_then(|time| order.position_at(time));
        found.unwrap_or(PlayedPosition {
            played_bar: order.bars.len() - 1,
            source_bar: last.source,
            offset: last.length,
        })
    }

    fn publish(&mut self) {
        let position = self.cursor();
        self.status.publish(&TransportStatus {
            state: self.state,
            played_bar: position.played_bar,
            source_bar: position.source_bar,
            offset: position.offset,
            frame: self.clock,
        });
    }
}
