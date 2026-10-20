use opentab_playback::{Channel, ChannelState, MidiMessage, Performance, ScoreTime};

const CHANNELS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    owner: usize,
    state: ChannelState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChaseIndex {
    lookup: Vec<[Option<usize>; CHANNELS]>,
    slots: Vec<Slot>,
    dynamic: Vec<usize>,
    rows: Vec<ChannelState>,
    bar_rows: Vec<usize>,
    current: Vec<ChannelState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChasedMessage {
    pub(crate) track: usize,
    pub(crate) channel: Channel,
    pub(crate) message: MidiMessage,
}

fn slot_of(lookup: &[[Option<usize>; CHANNELS]], track: usize, channel: Channel) -> Option<usize> {
    lookup
        .get(track)
        .and_then(|channels| channels.get(usize::from(channel.get())))
        .copied()
        .flatten()
}

fn apply(
    lookup: &[[Option<usize>; CHANNELS]],
    states: &mut [ChannelState],
    track: usize,
    channel: Channel,
    message: MidiMessage,
) {
    if let Some(state) = slot_of(lookup, track, channel).and_then(|slot| states.get_mut(slot)) {
        state.apply(message);
    }
}

impl ChaseIndex {
    pub(crate) fn new(performance: &Performance) -> ChaseIndex {
        let mut lookup: Vec<[Option<usize>; CHANNELS]> = Vec::new();
        let mut slots: Vec<Slot> = Vec::new();
        for setup in &performance.tracks {
            let mut channels = [None; CHANNELS];
            for (owner, entry) in setup.channels.iter().enumerate() {
                if let Some(found) = channels.get_mut(usize::from(entry.channel.get()))
                    && found.is_none()
                {
                    *found = Some(slots.len());
                    slots.push(Slot {
                        owner,
                        state: entry.state,
                    });
                }
            }
            lookup.push(channels);
        }
        let base: Vec<ChannelState> = slots.iter().map(|slot| slot.state).collect();
        let mut running = base.clone();
        let mut changes = vec![false; slots.len()];
        for event in &performance.events {
            let Some(slot) = slot_of(&lookup, event.track, event.channel) else {
                continue;
            };
            if let (Some(state), Some(changed)) = (running.get_mut(slot), changes.get_mut(slot)) {
                let before = *state;
                state.apply(event.message);
                *changed |= *state != before;
            }
        }
        let dynamic: Vec<usize> = (0..slots.len())
            .filter(|slot| changes.get(*slot).copied().unwrap_or(false))
            .collect();
        let width = dynamic.len();
        let mut running = base.clone();
        let mut rows: Vec<ChannelState> = Vec::new();
        let mut row_count: usize = 0;
        let mut bar_rows = Vec::with_capacity(performance.order.bars.len());
        let mut row: Vec<ChannelState> = Vec::with_capacity(width);
        let mut next = 0;
        for bar in &performance.order.bars {
            while let Some(event) = performance.events.get(next)
                && event.at <= bar.start
            {
                apply(
                    &lookup,
                    &mut running,
                    event.track,
                    event.channel,
                    event.message,
                );
                next += 1;
            }
            row.clear();
            row.extend(
                dynamic
                    .iter()
                    .filter_map(|slot| running.get(*slot).copied()),
            );
            let previous = row_count
                .checked_sub(1)
                .and_then(|last| rows.get(last * width..(last + 1) * width));
            if row_count == 0 || previous != Some(row.as_slice()) {
                rows.extend_from_slice(&row);
                row_count += 1;
            }
            bar_rows.push(row_count - 1);
        }
        ChaseIndex {
            lookup,
            slots,
            dynamic,
            rows,
            bar_rows,
            current: base,
        }
    }

    pub(crate) fn restore<F>(
        &mut self,
        performance: &Performance,
        at: ScoreTime,
        mut sink: F,
    ) -> u64
    where
        F: FnMut(ChasedMessage),
    {
        for (state, slot) in self.current.iter_mut().zip(&self.slots) {
            *state = slot.state;
        }
        let bars = &performance.order.bars;
        let events = &performance.events;
        let reached = bars.partition_point(|bar| bar.start <= at);
        let from = match reached.checked_sub(1) {
            Some(bar) => {
                let width = self.dynamic.len();
                let row = self.bar_rows.get(bar).copied().unwrap_or(0);
                if let Some(states) = self.rows.get(row * width..(row + 1) * width) {
                    for (slot, state) in self.dynamic.iter().zip(states) {
                        if let Some(target) = self.current.get_mut(*slot) {
                            *target = *state;
                        }
                    }
                }
                bars.get(bar).map_or(0, |played| {
                    events.partition_point(|event| event.at <= played.start)
                })
            }
            None => 0,
        };
        let to = events.partition_point(|event| event.at <= at).max(from);
        let mut steps = 0;
        for event in events.get(from..to).unwrap_or(&[]) {
            apply(
                &self.lookup,
                &mut self.current,
                event.track,
                event.channel,
                event.message,
            );
            steps += 1;
        }
        for (track, setup) in performance.tracks.iter().enumerate() {
            for (index, entry) in setup.channels.iter().enumerate() {
                let state = slot_of(&self.lookup, track, entry.channel)
                    .filter(|slot| {
                        self.slots
                            .get(*slot)
                            .is_some_and(|found| found.owner == index)
                    })
                    .and_then(|slot| self.current.get(slot).copied())
                    .unwrap_or(entry.state);
                for message in state.messages() {
                    sink(ChasedMessage {
                        track,
                        channel: entry.channel,
                        message,
                    });
                }
            }
        }
        steps
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use opentab_gp::read_gp;
    use opentab_model::Fraction;
    use opentab_playback::{Performance, PlaybackOptions, Profile, ScoreTime, perform};
    use opentab_testkit::arb_score;
    use proptest::prelude::*;
    use proptest::test_runner::{FileFailurePersistence, RngSeed};

    use super::{ChaseIndex, ChasedMessage};

    const PROFILES: [Profile; 2] = [Profile::Gp, Profile::Musical];
    const PROPERTY_SEED: u64 = 0x0074_3152;
    const POSITIONS: i64 = 97;
    const EVENT_POSITIONS: usize = 128;

    fn full_scan(performance: &Performance, at: ScoreTime) -> Vec<ChasedMessage> {
        (0..performance.tracks.len())
            .flat_map(|track| performance.chase(track, at))
            .map(|event| ChasedMessage {
                track: event.track,
                channel: event.channel,
                message: event.message,
            })
            .collect()
    }

    fn checkpointed(
        index: &mut ChaseIndex,
        performance: &Performance,
        at: ScoreTime,
    ) -> Vec<ChasedMessage> {
        let mut out = Vec::new();
        let steps = index.restore(performance, at, |message| out.push(message));
        let bars = &performance.order.bars;
        let bound = match bars.partition_point(|bar| bar.start <= at).checked_sub(1) {
            Some(bar) => {
                let start = bars[bar].start;
                let end = bars.get(bar + 1).map(|next| next.start);
                performance
                    .events
                    .iter()
                    .filter(|event| event.at > start && end.is_none_or(|end| event.at < end))
                    .count()
            }
            None => performance
                .events
                .iter()
                .filter(|event| event.at <= at)
                .count(),
        };
        assert!(
            steps as usize <= bound,
            "{steps} steps, bar holds {bound} events"
        );
        out
    }

    fn positions(performance: &Performance) -> Vec<ScoreTime> {
        let end = performance.order.end().whole_notes();
        let mut times: Vec<ScoreTime> =
            performance.order.bars.iter().map(|bar| bar.start).collect();
        let stride = (performance.events.len() / EVENT_POSITIONS).max(1);
        times.extend(
            performance
                .events
                .iter()
                .step_by(stride)
                .map(|event| event.at),
        );
        times.extend(
            (0..=POSITIONS + 1)
                .filter_map(|step| ScoreTime::new(end * Fraction::new(step, POSITIONS)).ok()),
        );
        times
    }

    fn check(performance: &Performance) {
        let mut index = ChaseIndex::new(performance);
        for at in positions(performance) {
            assert_eq!(
                checkpointed(&mut index, performance, at),
                full_scan(performance, at),
                "at {:?}",
                at
            );
        }
    }

    #[test]
    fn a_checkpoint_chase_equals_the_full_scan_on_every_fixture() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gp");
        let mut names: Vec<_> = std::fs::read_dir(&folder)
            .expect("fixtures")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "gp"))
            .collect();
        names.sort();
        assert!(names.len() > 10);
        for path in names {
            let score = read_gp(&std::fs::read(&path).expect("fixture")).expect("reads");
            for profile in PROFILES {
                check(&perform(&score, &PlaybackOptions { profile }).expect("perform"));
            }
        }
    }

    fn config() -> ProptestConfig {
        ProptestConfig {
            cases: 64,
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            rng_seed: RngSeed::Fixed(PROPERTY_SEED),
            ..ProptestConfig::default()
        }
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn a_checkpoint_chase_equals_the_full_scan_on_generated_scores(score in arb_score()) {
            for profile in PROFILES {
                if let Ok(performance) = perform(&score, &PlaybackOptions { profile }) {
                    check(&performance);
                }
            }
        }
    }
}
