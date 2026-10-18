use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

const LONG_TEXT_CHARS: usize = 10_000;
const LONG_TEXT_WEIGHT: u32 = 1;
const SHORT_TEXT_WEIGHT: u32 = 40;
const ACCENTED: [char; 8] = ['é', 'ñ', 'ü', 'ç', 'ø', 'ß', 'Ω', 'ж'];
const WIDE: [char; 6] = ['日', '本', '語', '한', '글', 'あ'];
const EMOJI: [char; 6] = ['🎸', '🥁', '🎼', '😀', '🎶', '🤘'];

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
        1 => any::<char>(),
    ]
}

pub fn arb_text(max_chars: usize) -> impl Strategy<Value = String> {
    vec(arb_char(), 0..=max_chars).prop_map(|chars| chars.into_iter().collect())
}

fn arb_long_text() -> impl Strategy<Value = String> {
    vec(arb_char(), 1..=8)
        .prop_map(|chars| chars.into_iter().cycle().take(LONG_TEXT_CHARS).collect())
}

pub fn arb_meta_text() -> impl Strategy<Value = String> {
    prop_oneof![
        SHORT_TEXT_WEIGHT => arb_text(24),
        LONG_TEXT_WEIGHT => arb_long_text(),
    ]
}
