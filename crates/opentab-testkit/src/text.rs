use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

const LONG_TEXT_CHARS: usize = 10_000;
const LONG_TEXT_WEIGHT: u32 = 1;
const SHORT_TEXT_WEIGHT: u32 = 40;
const ACCENTED: [char; 8] = ['é', 'ñ', 'ü', 'ç', 'ø', 'ß', 'Ω', 'ж'];
const WIDE: [char; 6] = ['日', '本', '語', '한', '글', 'あ'];
const EMOJI: [char; 6] = ['🎸', '🥁', '🎼', '😀', '🎶', '🤘'];
const MARKUP: [char; 6] = ['<', '>', '&', ']', '\'', '\t'];
const UNWRITABLE: [char; 6] = ['\u{0}', '\u{7}', '\r', '\u{1B}', '\u{FFFE}', '\u{FFFF}'];
const REPLACEMENT: char = '?';

fn writable(character: char) -> char {
    match character {
        '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'.. => character,
        _ => REPLACEMENT,
    }
}

fn arb_char() -> impl Strategy<Value = char> {
    prop_oneof![
        6 => proptest::char::range('a', 'z'),
        1 => Just(' '),
        1 => Just('"'),
        1 => Just('\\'),
        1 => Just('\n'),
        2 => select(ACCENTED.to_vec()),
        2 => select(WIDE.to_vec()),
        2 => select(EMOJI.to_vec()),
        1 => select(UNWRITABLE.to_vec()),
        1 => any::<char>(),
    ]
}

fn arb_writable_char() -> impl Strategy<Value = char> {
    prop_oneof![
        12 => arb_char().prop_map(writable),
        2 => select(MARKUP.to_vec()),
    ]
}

fn arb_char_for(gp_writable: bool) -> BoxedStrategy<char> {
    if gp_writable {
        arb_writable_char().boxed()
    } else {
        arb_char().prop_map(writable).boxed()
    }
}

pub fn arb_any_text(max_chars: usize) -> impl Strategy<Value = String> {
    vec(arb_char(), 0..=max_chars).prop_map(|chars| chars.into_iter().collect())
}

pub fn arb_text_for(max_chars: usize, gp_writable: bool) -> impl Strategy<Value = String> {
    vec(arb_char_for(gp_writable), 0..=max_chars).prop_map(|chars| chars.into_iter().collect())
}

fn arb_long_text(gp_writable: bool) -> impl Strategy<Value = String> {
    vec(arb_char_for(gp_writable), 1..=8)
        .prop_map(|chars| chars.into_iter().cycle().take(LONG_TEXT_CHARS).collect())
}

pub fn arb_meta_text(gp_writable: bool) -> impl Strategy<Value = String> {
    prop_oneof![
        SHORT_TEXT_WEIGHT => arb_text_for(24, gp_writable),
        LONG_TEXT_WEIGHT => arb_long_text(gp_writable),
    ]
}
