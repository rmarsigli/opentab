use std::sync::OnceLock;

use crate::display::Font;
use crate::glyphs::Metrics;
pub use crate::outline::OutlineFont;

pub static SOURCE_SANS_OTF: &[u8] =
    include_bytes!("../../../assets/fonts/source-sans-3/SourceSans3-Regular.otf");

static SOURCE_SANS: OnceLock<OutlineFont> = OnceLock::new();

pub fn text_font() -> &'static OutlineFont {
    SOURCE_SANS.get_or_init(|| match OutlineFont::parse(SOURCE_SANS_OTF) {
        Ok(font) => font,
        Err(error) => panic!("the bundled Source Sans 3 font does not load: {error}"),
    })
}

pub fn font(font: Font) -> &'static OutlineFont {
    match font {
        Font::Text => text_font(),
        Font::Music => Metrics::bravura().font(),
    }
}
