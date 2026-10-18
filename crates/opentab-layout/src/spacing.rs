use std::num::NonZeroI128;

use crate::units::{Units, narrow, round_div};

pub const DURATION_WIDTHS: [Units; 8] = [
    Units(1536),
    Units(1792),
    Units(2048),
    Units(2560),
    Units(3328),
    Units(4096),
    Units(5120),
    Units(6144),
];

pub const WIDTH_PER_DOUBLING_BEYOND_WHOLE: Units = Units(1024);

const SHORTEST_EXPONENT: i32 = -7;
const LONGEST_EXPONENT: i32 = 120;

fn table_width(exponent: i32) -> Units {
    if exponent <= 0 {
        let index = usize::try_from(exponent - SHORTEST_EXPONENT).unwrap_or(0);
        DURATION_WIDTHS
            .get(index)
            .copied()
            .unwrap_or(DURATION_WIDTHS[0])
    } else {
        let whole = DURATION_WIDTHS[DURATION_WIDTHS.len() - 1];
        whole
            + Units(
                WIDTH_PER_DOUBLING_BEYOND_WHOLE
                    .0
                    .saturating_mul(i64::from(exponent)),
            )
    }
}

fn shifted(value: i128, power: i32) -> Option<i128> {
    value.checked_mul(1_i128.checked_shl(power.unsigned_abs())?)
}

fn at_least_power(numerator: i128, denominator: i128, power: i32) -> bool {
    if power >= 0 {
        shifted(denominator, power).is_some_and(|scaled| scaled <= numerator)
    } else {
        shifted(numerator, power).is_none_or(|scaled| denominator <= scaled)
    }
}

pub fn duration_width(numerator: i128, denominator: NonZeroI128) -> Units {
    let denominator = denominator.get();
    if denominator < 0 || !at_least_power(numerator, denominator, SHORTEST_EXPONENT) {
        return DURATION_WIDTHS[0];
    }
    let mut exponent = SHORTEST_EXPONENT;
    while exponent < LONGEST_EXPONENT && at_least_power(numerator, denominator, exponent + 1) {
        exponent += 1;
    }
    let low = table_width(exponent);
    let high = table_width(exponent + 1);
    let above = shifted(numerator, -exponent.min(0));
    let below = shifted(denominator, exponent.max(0)).and_then(NonZeroI128::new);
    let (Some(above), Some(below)) = (above, below) else {
        return low;
    };
    let rise = i128::from((high - low).0);
    match rise.checked_mul(above - below.get()) {
        Some(scaled) => low + Units(narrow(round_div(scaled, below))),
        None => low,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    Fixed(Units),
    Space { natural: Units, weight: Units },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Totals {
    fixed: i128,
    natural: i128,
    weight: i128,
}

impl Totals {
    fn add(self, piece: &Piece) -> Totals {
        match *piece {
            Piece::Fixed(width) => Totals {
                fixed: self.fixed + i128::from(width.0),
                ..self
            },
            Piece::Space { natural, weight } => Totals {
                natural: self.natural + i128::from(natural.0),
                weight: self.weight + i128::from(weight.0),
                ..self
            },
        }
    }
}

pub fn natural_width(pieces: &[Piece]) -> Units {
    let totals = pieces.iter().fold(Totals::default(), Totals::add);
    Units(narrow(totals.fixed + totals.natural))
}

pub fn place(pieces: &[Piece], target: Units) -> Option<Vec<Units>> {
    let totals = pieces.iter().fold(Totals::default(), Totals::add);
    let target = i128::from(target.0);
    let room = target.checked_sub(totals.fixed).filter(|room| *room >= 0)?;
    let extra = room - totals.natural;
    let weight = NonZeroI128::new(totals.weight)?;
    let natural = NonZeroI128::new(totals.natural)?;
    let mut running = Totals::default();
    let mut positions = Vec::with_capacity(pieces.len() + 1);
    let position = |running: Totals| {
        let spaces = if extra >= 0 {
            running.natural + extra * running.weight / weight.get()
        } else {
            running.natural * room / natural.get()
        };
        Units(narrow(running.fixed + spaces))
    };
    positions.push(position(running));
    for piece in pieces {
        running = running.add(piece);
        positions.push(position(running));
    }
    Some(positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whole(numerator: i128, denominator: i128) -> Units {
        duration_width(numerator, NonZeroI128::new(denominator).unwrap())
    }

    #[test]
    fn plain_values_take_their_table_entry() {
        let values = [128, 64, 32, 16, 8, 4, 2, 1];
        for (value, width) in values.iter().zip(DURATION_WIDTHS) {
            assert_eq!(whole(1, *value), width, "1/{value}");
        }
        assert_eq!(whole(2, 1), Units(6144 + 1024));
        assert_eq!(whole(4, 1), Units(6144 + 2048));
        assert_eq!(whole(1, 256), DURATION_WIDTHS[0]);
        assert_eq!(whole(0, 1), DURATION_WIDTHS[0]);
    }

    #[test]
    fn dotted_and_tuplet_values_interpolate_exactly() {
        assert_eq!(whole(3, 8), Units(4096 + 512));
        assert_eq!(whole(7, 16), Units(4096 + 768));
        assert_eq!(whole(1, 6), Units(3328 + 256));
        assert_eq!(whole(1, 12), Units(2560 + 256));
        assert_eq!(whole(3, 2), Units(6144 + 512));
        assert_eq!(whole(1, 96), Units(1536 + 85));
    }

    #[test]
    fn widths_never_decrease_with_duration() {
        let mut previous = Units::ZERO;
        for numerator in 0..=4096 {
            let width = whole(numerator, 512);
            assert!(width >= previous, "{numerator}/512");
            previous = width;
        }
    }

    #[test]
    fn widths_match_exact_linear_interpolation_between_powers() {
        for numerator in 1_i128..=2048 {
            let denominator = 256_i128;
            let width = i128::from(whole(numerator, denominator).0);
            let mut exponent = -7;
            while exponent < 3 {
                let next = exponent + 1;
                let fits = if next >= 0 {
                    denominator << next <= numerator
                } else {
                    denominator <= numerator << -next
                };
                if !fits {
                    break;
                }
                exponent = next;
            }
            if numerator * 128 < denominator {
                continue;
            }
            let low = i128::from(table_width(exponent).0);
            let high = i128::from(table_width(exponent + 1).0);
            let base_numerator = if exponent >= 0 { 1 << exponent } else { 1 };
            let base_denominator = if exponent >= 0 { 1 } else { 1 << -exponent };
            let offset_numerator = numerator * base_denominator - denominator * base_numerator;
            let offset_denominator = denominator * base_numerator;
            let twice = 2 * (high - low) * offset_numerator;
            let exact_low = (2 * (width - low) - 1) * offset_denominator;
            let exact_high = (2 * (width - low) + 1) * offset_denominator;
            assert!(
                exact_low <= twice && twice <= exact_high,
                "{numerator}/{denominator}: {width}"
            );
        }
    }

    fn space(natural: i64, weight: i64) -> Piece {
        Piece::Space {
            natural: Units(natural),
            weight: Units(weight),
        }
    }

    #[test]
    fn stretching_adds_space_in_proportion_to_the_duration_weight() {
        let pieces = [
            Piece::Fixed(Units(100)),
            space(300, 100),
            space(300, 300),
            Piece::Fixed(Units(50)),
        ];
        assert_eq!(natural_width(&pieces), Units(750));
        let placed = place(&pieces, Units(1150)).unwrap();
        assert_eq!(
            placed,
            vec![Units(0), Units(100), Units(500), Units(1100), Units(1150)]
        );
        let exact = place(&pieces, Units(750)).unwrap();
        assert_eq!(
            exact,
            vec![Units(0), Units(100), Units(400), Units(700), Units(750)]
        );
    }

    #[test]
    fn compressing_scales_only_the_spaces() {
        let pieces = [
            Piece::Fixed(Units(100)),
            space(300, 100),
            space(300, 300),
            Piece::Fixed(Units(50)),
        ];
        let placed = place(&pieces, Units(450)).unwrap();
        assert_eq!(
            placed,
            vec![Units(0), Units(100), Units(250), Units(400), Units(450)]
        );
        assert_eq!(place(&pieces, Units(149)), None);
        assert_eq!(
            place(&pieces, Units(150)).unwrap(),
            vec![Units(0), Units(100), Units(100), Units(100), Units(150)]
        );
    }

    #[test]
    fn placement_always_ends_exactly_on_the_target_and_never_goes_back() {
        let pieces = [
            Piece::Fixed(Units(7)),
            space(1536, 1536),
            space(2000, 1792),
            Piece::Fixed(Units(13)),
            space(4096, 4096),
            space(1700, 1536),
        ];
        for target in 20..20_000 {
            let placed = place(&pieces, Units(target)).unwrap();
            assert_eq!(placed.first(), Some(&Units(0)));
            assert_eq!(placed.last(), Some(&Units(target)), "{target}");
            assert!(placed.windows(2).all(|pair| pair[0] <= pair[1]), "{target}");
        }
    }
}
