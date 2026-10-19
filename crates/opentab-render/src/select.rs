use num_rational::Ratio;
use opentab_layout::{BarRange, LayoutOptions, StaffMode};
use opentab_model::Score;

const MAX_SCALE_DIGITS: usize = 9;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BarSpan {
    pub first: u32,
    pub last: u32,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScoreSelection {
    pub tracks: Vec<u32>,
    pub bars: Option<BarSpan>,
    pub staff: Option<StaffMode>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum SelectionError {
    #[error("track {number}: the score has {count} track(s), numbered from 1")]
    Track { number: u32, count: usize },
    #[error("bars {first}-{last}: the score has {count} bar(s), numbered from 1")]
    Bars { first: u32, last: u32, count: usize },
    #[error("page {number}: the score has {count} page(s), numbered from 1")]
    Page { number: u32, count: usize },
}

pub fn layout_options(
    score: &Score,
    selection: &ScoreSelection,
) -> Result<LayoutOptions, SelectionError> {
    let track_count = score.tracks.len();
    let mut tracks = Vec::with_capacity(selection.tracks.len());
    for number in &selection.tracks {
        let index = from_one(*number)
            .filter(|index| *index < track_count)
            .ok_or(SelectionError::Track {
                number: *number,
                count: track_count,
            })?;
        tracks.push(index);
    }
    let bar_count = score.master_bars.len();
    let bars = match selection.bars {
        Some(span) => match (from_one(span.first), from_one(span.last)) {
            (Some(first), Some(last)) if first <= last && last < bar_count => {
                Some(BarRange { first, last })
            }
            _ => {
                return Err(SelectionError::Bars {
                    first: span.first,
                    last: span.last,
                    count: bar_count,
                });
            }
        },
        None => None,
    };
    Ok(LayoutOptions {
        tracks: (!tracks.is_empty()).then_some(tracks),
        bars,
        staff_mode: selection.staff,
        ..LayoutOptions::default()
    })
}

fn from_one(number: u32) -> Option<usize> {
    usize::try_from(number).ok()?.checked_sub(1)
}

pub fn page_number(number: u32, count: usize) -> Result<usize, SelectionError> {
    usize::try_from(number)
        .ok()
        .filter(|number| (1..=count).contains(number))
        .ok_or(SelectionError::Page { number, count })
}

pub fn parse_bars(text: &str) -> Result<BarSpan, String> {
    let invalid = || format!("invalid bars {text:?}: use A-B or A, counted from 1");
    let (first, last) = text.split_once('-').unwrap_or((text, text));
    let number = |part: &str| whole(part).filter(|value| *value > 0);
    let first = number(first).ok_or_else(invalid)?;
    let last = number(last).ok_or_else(invalid)?;
    if first > last {
        return Err(invalid());
    }
    Ok(BarSpan { first, last })
}

pub fn parse_scale(text: &str) -> Result<Ratio<u32>, String> {
    let invalid = || format!("invalid scale {text:?}: use a positive number such as 2, 1.5 or 3/2");
    let ratio = match text.split_once('/') {
        Some((numerator, denominator)) => {
            let numerator = whole(numerator).ok_or_else(invalid)?;
            let denominator = whole(denominator).ok_or_else(invalid)?;
            if denominator == 0 {
                return Err(invalid());
            }
            Ratio::new(numerator, denominator)
        }
        None => decimal(text).ok_or_else(invalid)?,
    };
    if *ratio.numer() == 0 {
        return Err("the scale must be greater than zero".to_string());
    }
    Ok(ratio)
}

fn whole(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > MAX_SCALE_DIGITS || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    text.parse().ok()
}

fn decimal(text: &str) -> Option<Ratio<u32>> {
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    if text.ends_with('.') || integer.len() + fraction.len() > MAX_SCALE_DIGITS {
        return None;
    }
    let numerator = whole(&format!("{integer}{fraction}"))?;
    let denominator = 10_u32.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    if integer.is_empty() {
        return None;
    }
    Some(Ratio::new(numerator, denominator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_parse_as_exact_ratios() {
        assert_eq!(parse_scale("1"), Ok(Ratio::from_integer(1)));
        assert_eq!(parse_scale("2"), Ok(Ratio::from_integer(2)));
        assert_eq!(parse_scale("1.5"), Ok(Ratio::new(3, 2)));
        assert_eq!(parse_scale("0.25"), Ok(Ratio::new(1, 4)));
        assert_eq!(parse_scale("3/2"), Ok(Ratio::new(3, 2)));
        assert_eq!(parse_scale("6/4"), Ok(Ratio::new(3, 2)));
        assert_eq!(parse_scale("2.50"), Ok(Ratio::new(5, 2)));
    }

    #[test]
    fn numbers_take_at_most_nine_digits() {
        assert_eq!(
            parse_bars("123456789"),
            Ok(BarSpan {
                first: 123_456_789,
                last: 123_456_789
            })
        );
        assert!(parse_bars("1234567890").is_err());
        assert!(parse_bars("1-1234567890").is_err());
        assert_eq!(
            parse_scale("123456789/1"),
            Ok(Ratio::from_integer(123_456_789))
        );
        assert!(parse_scale("1234567890/1").is_err());
        assert!(parse_scale("1/1234567890").is_err());
        assert_eq!(
            parse_scale("12345.6789"),
            Ok(Ratio::new(123_456_789, 10_000))
        );
        assert_eq!(parse_scale("0.00000001"), Ok(Ratio::new(1, 100_000_000)));
        assert!(parse_scale("12345.67891").is_err());
    }

    #[test]
    fn bar_spans_parse_from_one_or_two_numbers() {
        assert_eq!(parse_bars("3-7"), Ok(BarSpan { first: 3, last: 7 }));
        assert_eq!(parse_bars("4"), Ok(BarSpan { first: 4, last: 4 }));
        assert_eq!(parse_bars("2-2"), Ok(BarSpan { first: 2, last: 2 }));
        for text in [
            "", "0", "0-3", "5-4", "a", "1-", "-2", "1-2-3", " 1", "1.5", "+1",
        ] {
            assert!(parse_bars(text).is_err(), "{text}");
        }
    }

    #[test]
    fn bad_scales_are_refused() {
        for text in [
            "",
            "0",
            "0.0",
            "0/5",
            "-1",
            "+1",
            "1/0",
            "1.",
            ".5",
            "1.2.3",
            "a",
            "1e3",
            "1/",
            "/2",
            "1/2/3",
            " 1",
            "1234567890",
            "0.0000000001",
        ] {
            assert!(parse_scale(text).is_err(), "{text}");
        }
    }

    #[test]
    fn page_numbers_count_from_one_up_to_the_page_count() {
        assert_eq!(page_number(1, 3), Ok(1));
        assert_eq!(page_number(3, 3), Ok(3));
        assert_eq!(
            page_number(0, 3),
            Err(SelectionError::Page {
                number: 0,
                count: 3
            })
        );
        assert_eq!(
            page_number(4, 3),
            Err(SelectionError::Page {
                number: 4,
                count: 3
            })
        );
        assert_eq!(
            page_number(u32::MAX, 3).map_err(|error| error.to_string()),
            Err(format!(
                "page {}: the score has 3 page(s), numbered from 1",
                u32::MAX
            ))
        );
    }
}
