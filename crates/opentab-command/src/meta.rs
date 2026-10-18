use opentab_model::{Score, ScoreMeta};

use crate::{Command, MetaField};

fn slot(meta: &mut ScoreMeta, field: MetaField) -> &mut String {
    match field {
        MetaField::Title => &mut meta.title,
        MetaField::Subtitle => &mut meta.subtitle,
        MetaField::Artist => &mut meta.artist,
        MetaField::Album => &mut meta.album,
        MetaField::Words => &mut meta.words,
        MetaField::Music => &mut meta.music,
        MetaField::Copyright => &mut meta.copyright,
        MetaField::TabAuthor => &mut meta.tab_author,
        MetaField::Instructions => &mut meta.instructions,
        MetaField::Notices => &mut meta.notices,
    }
}

pub(crate) fn set_anacrusis(score: &mut Score, value: bool) -> Command {
    let old = std::mem::replace(&mut score.anacrusis, value);
    Command::SetAnacrusis { value: old }
}

pub(crate) fn set(score: &mut Score, field: MetaField, value: &str) -> Command {
    let old = std::mem::replace(slot(&mut score.meta, field), value.to_owned());
    Command::SetMeta { field, value: old }
}
