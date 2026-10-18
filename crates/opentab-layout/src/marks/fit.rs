use std::num::NonZeroI64;

use crate::display::{Font, Primitive, TextAnchor, TextRun};
use crate::text::text_font;
use crate::units::{Point, REFERENCE_EM, Rect, Units};

pub const ELLIPSIS: char = '\u{2026}';
const EM_DIVISOR: NonZeroI64 = NonZeroI64::new(REFERENCE_EM.0).unwrap();
const INK_GUARD: i64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted {
    pub text: String,
    pub ink: Rect,
    pub em: Units,
}

impl Fitted {
    pub fn width(&self) -> Units {
        self.ink.width()
    }

    pub fn origin_for(&self, left: Units) -> Units {
        left - self.ink.min.x
    }

    pub fn run(&self, left: Units, baseline: Units) -> Primitive {
        Primitive::Text(TextRun {
            origin: Point {
                x: self.origin_for(left),
                y: baseline,
            },
            text: self.text.clone(),
            font: Font::Text,
            em: self.em,
            anchor: TextAnchor::Start,
        })
    }
}

pub fn clean(text: &str) -> String {
    let spaced: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    spaced.trim().to_string()
}

pub fn ascent(em: Units) -> Units {
    text_font().ascent(em)
}

pub fn descent(em: Units) -> Units {
    text_font().descent(em)
}

pub fn line_height(em: Units) -> Units {
    ascent(em) + descent(em)
}

pub fn ink(text: &str, em: Units) -> Option<Rect> {
    let font = text_font();
    let positions = font.pen_positions(text, em);
    let mut bounds: Option<Rect> = None;
    for (character, pen) in text.chars().zip(positions) {
        let Some(glyph) = font.outline(character).bounds() else {
            continue;
        };
        let min = Point {
            x: pen + glyph.min.x.scaled(em.0, EM_DIVISOR),
            y: glyph.min.y.scaled(em.0, EM_DIVISOR),
        };
        let max = Point {
            x: pen + glyph.max.x.scaled(em.0, EM_DIVISOR),
            y: glyph.max.y.scaled(em.0, EM_DIVISOR),
        };
        bounds = Some(match bounds {
            Some(known) => known.including(min).including(max),
            None => Rect::new(min, max),
        });
    }
    bounds
}

fn fitted(text: String, em: Units, max: Units) -> Option<Fitted> {
    let rect = ink(&text, em)?;
    (rect.width() <= max).then_some(Fitted {
        text,
        ink: rect,
        em,
    })
}

pub fn fit_line(text: &str, em: Units, max: Units) -> Option<Fitted> {
    let cleaned = clean(text);
    if cleaned.is_empty() || max <= Units::ZERO {
        return None;
    }
    let font = text_font();
    let positions = font.pen_positions(&cleaned, em);
    let total = positions.last().copied().unwrap_or_default();
    if total <= Units(max.0.saturating_mul(INK_GUARD)) {
        let whole = ink(&cleaned, em)?;
        if whole.width() <= max {
            return Some(Fitted {
                text: cleaned,
                ink: whole,
                em,
            });
        }
    }
    let ellipsis = font.advance(ELLIPSIS, em);
    let characters: Vec<char> = cleaned.chars().collect();
    let mut keep = positions
        .iter()
        .rposition(|pen| *pen + ellipsis <= max)
        .unwrap_or(0)
        .min(characters.len());
    loop {
        let prefix: String = characters[..keep].iter().collect();
        let mut candidate = prefix.trim_end().to_string();
        candidate.push(ELLIPSIS);
        if let Some(found) = fitted(candidate, em, max) {
            return Some(found);
        }
        keep = keep.checked_sub(1)?;
    }
}

fn split_word(word: &str, em: Units, max: Units) -> usize {
    let positions = text_font().pen_positions(word, em);
    let count = positions
        .iter()
        .rposition(|pen| *pen <= max)
        .unwrap_or(0)
        .max(1);
    word.char_indices()
        .nth(count)
        .map_or(word.len(), |(at, _)| at)
}

pub fn wrap(text: &str, em: Units, max: Units, limit: usize) -> Vec<Fitted> {
    let cleaned = clean(text);
    let font = text_font();
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < words.len() {
        if lines.len() + 1 >= limit {
            let mut rest: Vec<&str> = Vec::with_capacity(words.len() - index + 1);
            if !current.is_empty() {
                rest.push(&current);
            }
            rest.extend(&words[index..]);
            current = rest.join(" ");
            break;
        }
        let word = words[index];
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if font.measure(&candidate, em) <= max {
            current = candidate;
            index += 1;
        } else if current.is_empty() {
            let split = split_word(word, em, max);
            lines.push(word[..split].to_string());
            words[index] = &word[split..];
            if words[index].is_empty() {
                index += 1;
            }
        } else {
            lines.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
        .iter()
        .take(limit)
        .filter_map(|line| fit_line(line, em, max))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::STAFF_SPACE;

    const EM: Units = Units(STAFF_SPACE.0 * 3 / 2);

    #[test]
    fn control_characters_become_spaces_and_the_ends_are_trimmed() {
        assert_eq!(clean("\tA\nB\u{7}C  "), "A B C");
        assert_eq!(clean(" \n\t "), "");
    }

    #[test]
    fn a_short_line_is_kept_whole_and_a_long_one_ends_in_an_ellipsis() {
        let short = fit_line("Am7", EM, Units(100_000)).unwrap();
        assert_eq!(short.text, "Am7");
        assert_eq!(Some(short.ink), ink("Am7", EM));
        let max = Units(STAFF_SPACE.0 * 6);
        let long = fit_line("Words and music by somebody", EM, max).unwrap();
        assert!(long.text.ends_with(ELLIPSIS), "{}", long.text);
        assert!(long.width() <= max);
        let source: Vec<char> = "Words and music by somebody".chars().collect();
        let best = (0..=source.len())
            .map(|keep| {
                let prefix: String = source[..keep].iter().collect();
                format!("{}{ELLIPSIS}", prefix.trim_end())
            })
            .rfind(|candidate| ink(candidate, EM).unwrap().width() <= max)
            .unwrap();
        assert_eq!(long.text, best);
        assert_eq!(fit_line("   ", EM, max), None);
        assert_eq!(fit_line("abc", EM, Units(1)), None);
    }

    #[test]
    fn wrapping_breaks_at_words_and_splits_a_word_wider_than_the_line() {
        let max = Units(STAFF_SPACE.0 * 12);
        let lines = wrap("one two three four five six", EM, max, 10);
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        assert!(texts.len() > 1);
        assert_eq!(texts.join(" "), "one two three four five six");
        for pair in texts.windows(2) {
            let first = pair[1].split(' ').next().unwrap();
            let joined = format!("{} {first}", pair[0]);
            assert!(text_font().measure(&joined, EM) > max, "{joined}");
        }
        let word = "x".repeat(400);
        let broken = wrap(&word, EM, max, 100);
        assert!(broken.len() > 3);
        assert_eq!(
            broken
                .iter()
                .map(|line| line.text.as_str())
                .collect::<String>(),
            word
        );
        for line in &broken {
            assert!(line.width() <= max);
        }
    }

    #[test]
    fn wrapping_stops_at_the_line_limit_with_an_ellipsis() {
        let max = Units(STAFF_SPACE.0 * 12);
        let lines = wrap(&"word ".repeat(200), EM, max, 3);
        assert_eq!(lines.len(), 3);
        assert!(lines[2].text.ends_with(ELLIPSIS));
        assert!(!lines[1].text.ends_with(ELLIPSIS));
        assert!(wrap("", EM, max, 3).is_empty());
    }

    #[test]
    fn every_glyph_of_the_text_font_lies_inside_its_line_box() {
        let font = text_font();
        let top = -ascent(REFERENCE_EM);
        let bottom = descent(REFERENCE_EM);
        let mut checked = 0;
        for character in font.codepoints().into_iter().chain(['\u{E000}']) {
            if let Some(bounds) = font.outline(character).bounds() {
                assert!(bounds.min.y >= top, "{character:?} {bounds:?}");
                assert!(bounds.max.y <= bottom, "{character:?} {bounds:?}");
                checked += 1;
            }
        }
        assert!(checked > 1000, "{checked}");
    }
}
