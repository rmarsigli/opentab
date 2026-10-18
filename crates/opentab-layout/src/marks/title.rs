use opentab_model::ScoreMeta;

use crate::display::Primitive;
use crate::marks::fit::{Fitted, ascent, clean, fit_line, line_height, wrap};
use crate::units::{STAFF_SPACE, Units};

pub const TITLE_EM: Units = Units(STAFF_SPACE.0 * 9 / 2);
pub const SUBTITLE_EM: Units = Units(STAFF_SPACE.0 * 5 / 2);
pub const ARTIST_EM: Units = Units(STAFF_SPACE.0 * 2);
pub const ALBUM_EM: Units = Units(STAFF_SPACE.0 * 7 / 4);
pub const CREDIT_EM: Units = Units(STAFF_SPACE.0 * 3 / 2);
pub const TITLE_LINES: usize = 3;
pub const SUBTITLE_LINES: usize = 2;
pub const CREDIT_GAP: Units = STAFF_SPACE;
pub const BLOCK_GAP: Units = STAFF_SPACE;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TitleBlock {
    pub items: Vec<Primitive>,
    pub height: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub left: Units,
    pub right: Units,
    pub top: Units,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Centre,
    Right,
}

struct Writer {
    frame: Frame,
    cursor: Units,
    items: Vec<Primitive>,
}

impl Writer {
    fn left_of(&self, line: &Fitted, align: Align) -> Units {
        match align {
            Align::Left => self.frame.left,
            Align::Right => self.frame.right - line.width(),
            Align::Centre => {
                self.frame.left + (self.frame.right - self.frame.left - line.width()).half()
            }
        }
    }

    fn row(&mut self, em: Units, cells: &[(Option<Fitted>, Align)]) {
        if cells.iter().all(|(line, _)| line.is_none()) {
            return;
        }
        let baseline = self.cursor + ascent(em);
        for (line, align) in cells {
            if let Some(line) = line {
                let left = self.left_of(line, *align);
                self.items.push(line.run(left, baseline));
            }
        }
        self.cursor += line_height(em);
    }

    fn lines(&mut self, lines: Vec<Fitted>, em: Units) {
        for line in lines {
            self.row(em, &[(Some(line), Align::Centre)]);
        }
    }
}

pub fn credits(meta: &ScoreMeta) -> Vec<String> {
    let words = clean(&meta.words);
    let music = clean(&meta.music);
    if !words.is_empty() && words == music {
        return vec![format!("Words & Music by {words}")];
    }
    let mut lines = Vec::new();
    if !words.is_empty() {
        lines.push(format!("Words by {words}"));
    }
    if !music.is_empty() {
        lines.push(format!("Music by {music}"));
    }
    lines
}

pub fn tab_credit(meta: &ScoreMeta) -> Option<String> {
    let author = clean(&meta.tab_author);
    (!author.is_empty()).then(|| format!("Tab by {author}"))
}

pub fn title_block(meta: &ScoreMeta, frame: Frame) -> TitleBlock {
    let width = frame.right - frame.left;
    let mut writer = Writer {
        frame,
        cursor: frame.top,
        items: Vec::new(),
    };
    writer.lines(wrap(&meta.title, TITLE_EM, width, TITLE_LINES), TITLE_EM);
    writer.lines(
        wrap(&meta.subtitle, SUBTITLE_EM, width, SUBTITLE_LINES),
        SUBTITLE_EM,
    );
    writer.row(
        ARTIST_EM,
        &[(fit_line(&meta.artist, ARTIST_EM, width), Align::Centre)],
    );
    writer.row(
        ALBUM_EM,
        &[(fit_line(&meta.album, ALBUM_EM, width), Align::Centre)],
    );
    let credits = credits(meta);
    let tab = tab_credit(meta);
    let shared = Units((width - CREDIT_GAP).0.max(0) / 2);
    let rows = credits.len().max(usize::from(tab.is_some()));
    for row in 0..rows {
        let left = if row == 0 { tab.as_deref() } else { None };
        let right = credits.get(row).map(String::as_str);
        let room = if left.is_some() && right.is_some() {
            shared
        } else {
            width
        };
        writer.row(
            CREDIT_EM,
            &[
                (
                    left.and_then(|text| fit_line(text, CREDIT_EM, room)),
                    Align::Left,
                ),
                (
                    right.and_then(|text| fit_line(text, CREDIT_EM, room)),
                    Align::Right,
                ),
            ],
        );
    }
    let used = writer.cursor - frame.top;
    TitleBlock {
        height: if writer.items.is_empty() {
            Units::ZERO
        } else {
            used + BLOCK_GAP
        },
        items: writer.items,
    }
}
