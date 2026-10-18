use std::collections::BTreeMap;

use opentab_model::{Bar, Fraction};

use crate::structure::{BeatSlot, GraceSlot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridColumn {
    pub ticks: i128,
    pub graces: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub per_whole: i128,
    pub span: i128,
    pub columns: Vec<GridColumn>,
    pub slots: Vec<Vec<BeatSlot>>,
}

#[derive(Debug, Clone, Copy)]
struct Placed {
    voice: usize,
    beat: usize,
    ticks: i128,
    grace: Option<GraceSlot>,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a.abs()
}

fn lcm(a: i128, b: i128) -> Option<i128> {
    let divisor = gcd(a, b);
    if divisor == 0 {
        return None;
    }
    (a / divisor).checked_mul(b)
}

fn ticks(length: Fraction, per_whole: i128) -> Option<i128> {
    let denominator = i128::from(*length.denom());
    if denominator <= 0 {
        return None;
    }
    i128::from(*length.numer()).checked_mul(per_whole / denominator)
}

fn timed_lengths<'a>(bars: &'a [Option<&'a Bar>]) -> impl Iterator<Item = Fraction> + 'a {
    bars.iter()
        .flatten()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .filter(|beat| beat.effects.grace.is_none())
        .map(|beat| beat.duration.length())
}

struct Walk {
    placed: Vec<Placed>,
    end: Option<i128>,
}

fn walk(bar: &Bar, per_whole: i128) -> Option<Walk> {
    let mut placed = Vec::new();
    let mut latest: Option<i128> = None;
    for (voice_index, voice) in bar.voices.iter().enumerate() {
        let mut time = 0_i128;
        let mut timed = false;
        let mut pending: Vec<usize> = Vec::new();
        for (beat_index, beat) in voice.beats.iter().enumerate() {
            if beat.effects.grace.is_some() {
                pending.push(beat_index);
                continue;
            }
            flush(&mut placed, voice_index, &mut pending, time);
            placed.push(Placed {
                voice: voice_index,
                beat: beat_index,
                ticks: time,
                grace: None,
            });
            time = time.checked_add(ticks(beat.duration.length(), per_whole)?)?;
            timed = true;
        }
        flush(&mut placed, voice_index, &mut pending, time);
        if timed {
            latest = Some(latest.map_or(time, |known| known.max(time)));
        }
    }
    Some(Walk {
        placed,
        end: latest,
    })
}

fn flush(placed: &mut Vec<Placed>, voice: usize, pending: &mut Vec<usize>, time: i128) {
    let count = pending.len();
    for (order, beat) in pending.drain(..).enumerate() {
        placed.push(Placed {
            voice,
            beat,
            ticks: time,
            grace: Some(GraceSlot { order, count }),
        });
    }
}

pub fn grid(bars: &[Option<&Bar>], length: Fraction, pickup: bool) -> Option<Grid> {
    let mut per_whole = i128::from(*length.denom());
    for item in timed_lengths(bars) {
        per_whole = lcm(per_whole, i128::from(*item.denom()))?;
    }
    if per_whole <= 0 {
        return None;
    }
    let bar_length = ticks(length, per_whole)?;
    let mut walks = Vec::with_capacity(bars.len());
    for bar in bars {
        walks.push(match bar {
            Some(bar) => walk(bar, per_whole)?,
            None => Walk {
                placed: Vec::new(),
                end: None,
            },
        });
    }
    let mut graces: BTreeMap<i128, usize> = BTreeMap::new();
    for item in walks.iter().flat_map(|walk| &walk.placed) {
        let count = item.grace.map_or(0, |grace| grace.count);
        let entry = graces.entry(item.ticks).or_insert(0);
        *entry = (*entry).max(count);
    }
    let end = walks.iter().filter_map(|walk| walk.end).max();
    let span = match (end, pickup) {
        (Some(end), true) => end,
        (Some(end), _) => end.max(bar_length),
        (None, _) => bar_length,
    };
    let columns: Vec<GridColumn> = graces
        .iter()
        .map(|(ticks, graces)| GridColumn {
            ticks: *ticks,
            graces: *graces,
        })
        .collect();
    let slots = walks
        .iter()
        .map(|walk| {
            walk.placed
                .iter()
                .filter_map(|item| {
                    let column = columns
                        .binary_search_by_key(&item.ticks, |column| column.ticks)
                        .ok()?;
                    Some(BeatSlot {
                        voice: item.voice,
                        beat: item.beat,
                        column,
                        grace: item.grace,
                    })
                })
                .collect()
        })
        .collect();
    Some(Grid {
        per_whole,
        span,
        columns,
        slots,
    })
}

#[cfg(test)]
mod tests {
    use opentab_model::{
        BarId, Beat, BeatEffects, BeatId, Clef, Duration, ElementId, Grace, NoteValue, Tuplet,
        Voice, VoiceId, VoiceSlot,
    };

    use super::*;

    fn beat(value: NoteValue, dots: u8, tuplet: Option<(u8, u8)>, grace: Option<Grace>) -> Beat {
        let tuplet = tuplet.map(|(num, den)| Tuplet::new(num, den).unwrap());
        Beat {
            id: BeatId(ElementId(1)),
            duration: Duration::new(value, dots, tuplet).unwrap(),
            notes: Vec::new(),
            effects: BeatEffects {
                grace,
                ..BeatEffects::default()
            },
        }
    }

    fn plain(value: NoteValue) -> Beat {
        beat(value, 0, None, None)
    }

    fn bar(voices: Vec<Vec<Beat>>) -> Bar {
        Bar {
            id: BarId(ElementId(2)),
            clef: Clef::Treble,
            voices: voices
                .into_iter()
                .enumerate()
                .map(|(slot, beats)| Voice {
                    id: VoiceId(ElementId(3)),
                    slot: VoiceSlot::new(u8::try_from(slot).unwrap()).unwrap(),
                    beats,
                })
                .collect(),
        }
    }

    fn ticks_of(grid: &Grid) -> Vec<i128> {
        grid.columns.iter().map(|column| column.ticks).collect()
    }

    #[test]
    fn onsets_of_all_voices_and_staves_merge_into_one_grid() {
        let first = bar(vec![
            vec![plain(NoteValue::Half), plain(NoteValue::Half)],
            vec![
                plain(NoteValue::Quarter),
                beat(NoteValue::Quarter, 1, None, None),
                plain(NoteValue::Eighth),
                plain(NoteValue::Half),
            ],
        ]);
        let second = bar(vec![vec![
            beat(NoteValue::Eighth, 0, Some((3, 2)), None),
            beat(NoteValue::Eighth, 0, Some((3, 2)), None),
            beat(NoteValue::Eighth, 0, Some((3, 2)), None),
            beat(NoteValue::Quarter, 2, None, None),
            plain(NoteValue::Sixteenth),
            plain(NoteValue::Half),
        ]]);
        let grid = grid(&[Some(&first), Some(&second)], Fraction::new(4, 4), false).unwrap();
        assert_eq!(grid.per_whole, 48);
        assert_eq!(ticks_of(&grid), vec![0, 4, 8, 12, 24, 30, 33, 36]);
        assert_eq!(grid.span, 60);
        assert_eq!(grid.slots.len(), 2);
        let columns: Vec<(usize, usize, usize)> = grid.slots[0]
            .iter()
            .map(|slot| (slot.voice, slot.beat, slot.column))
            .collect();
        assert_eq!(
            columns,
            vec![
                (0, 0, 0),
                (0, 1, 4),
                (1, 0, 0),
                (1, 1, 3),
                (1, 2, 5),
                (1, 3, 7)
            ]
        );
        let second_columns: Vec<usize> = grid.slots[1].iter().map(|slot| slot.column).collect();
        assert_eq!(second_columns, vec![0, 1, 2, 3, 6, 7]);
    }

    #[test]
    fn grace_beats_attach_to_the_next_timed_beat_or_the_voice_end() {
        let first = bar(vec![vec![
            beat(NoteValue::Eighth, 0, None, Some(Grace::BeforeBeat)),
            beat(NoteValue::Sixteenth, 0, None, Some(Grace::OnBeat)),
            plain(NoteValue::Half),
            plain(NoteValue::Half),
            beat(NoteValue::Eighth, 0, None, Some(Grace::BeforeBeat)),
        ]]);
        let grid = grid(&[Some(&first)], Fraction::new(4, 4), false).unwrap();
        assert_eq!(ticks_of(&grid), vec![0, 1, 2]);
        let graces: Vec<usize> = grid.columns.iter().map(|column| column.graces).collect();
        assert_eq!(graces, vec![2, 0, 1]);
        assert_eq!(grid.span, 2);
        let slots = &grid.slots[0];
        assert_eq!(slots[0].grace, Some(GraceSlot { order: 0, count: 2 }));
        assert_eq!(slots[1].grace, Some(GraceSlot { order: 1, count: 2 }));
        assert_eq!((slots[2].column, slots[2].grace), (0, None));
        assert_eq!((slots[4].beat, slots[4].column), (4, 2));
        assert_eq!(slots[4].grace, Some(GraceSlot { order: 0, count: 1 }));
    }

    #[test]
    fn grace_only_and_empty_voices_span_the_whole_bar() {
        let graces = bar(vec![
            vec![beat(NoteValue::Eighth, 0, None, Some(Grace::BeforeBeat))],
            Vec::new(),
        ]);
        let grid = grid(&[Some(&graces), None], Fraction::new(3, 4), false).unwrap();
        assert_eq!(ticks_of(&grid), vec![0]);
        assert_eq!(grid.span, 3);
        assert!(grid.slots[1].is_empty());
        let empty = grid_of_nothing();
        assert!(empty.columns.is_empty());
        assert_eq!(empty.span, 7);
    }

    fn grid_of_nothing() -> Grid {
        let bars = [
            None,
            Some(&Bar {
                id: BarId(ElementId(9)),
                clef: Clef::Bass,
                voices: Vec::new(),
            }),
        ];
        grid(&[bars[0], bars[1]], Fraction::new(7, 8), true).unwrap()
    }

    #[test]
    fn a_pickup_spans_its_content_and_an_overfull_bar_its_longest_voice() {
        let short = bar(vec![vec![plain(NoteValue::Quarter)]]);
        let pickup = grid(&[Some(&short)], Fraction::new(4, 4), true).unwrap();
        assert_eq!((pickup.per_whole, pickup.span), (4, 1));
        let normal = grid(&[Some(&short)], Fraction::new(4, 4), false).unwrap();
        assert_eq!(normal.span, 4);
        let long = bar(vec![vec![plain(NoteValue::Whole), plain(NoteValue::Half)]]);
        let overfull = grid(&[Some(&long)], Fraction::new(4, 4), false).unwrap();
        assert_eq!((overfull.per_whole, overfull.span), (2, 3));
        let overfull_pickup = grid(&[Some(&long)], Fraction::new(4, 4), true).unwrap();
        assert_eq!(overfull_pickup.span, 3);
    }

    #[test]
    fn coprime_tuplets_share_one_exact_tick_grid() {
        let tuplets: Vec<Vec<Beat>> = [3_u8, 5, 7, 11, 13]
            .iter()
            .map(|num| {
                (0..*num)
                    .map(|_| beat(NoteValue::SixtyFourth, 3, Some((*num, 64)), None))
                    .collect()
            })
            .collect();
        let bars: Vec<Bar> = tuplets.into_iter().map(|beats| bar(vec![beats])).collect();
        let refs: Vec<Option<&Bar>> = bars.iter().map(Some).collect();
        let grid = grid(&refs, Fraction::new(1, 1), false).unwrap();
        assert_eq!(grid.per_whole, 8 * 7 * 11 * 13);
        assert_eq!(grid.columns.len(), 1 + 2 + 4 + 6 + 10 + 12);
        assert_eq!(grid.span, grid.per_whole * 15 / 8);
    }
}
