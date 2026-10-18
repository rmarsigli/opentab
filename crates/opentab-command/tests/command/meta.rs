use opentab_command::{Command, MetaField, apply};

use crate::checks::round_trip;
use crate::support::fixture;

const FIELDS: [MetaField; 10] = [
    MetaField::Title,
    MetaField::Subtitle,
    MetaField::Artist,
    MetaField::Album,
    MetaField::Words,
    MetaField::Music,
    MetaField::Copyright,
    MetaField::TabAuthor,
    MetaField::Instructions,
    MetaField::Notices,
];

#[test]
fn set_meta_writes_every_field_and_inverts() {
    let mut fx = fixture();
    for (index, field) in FIELDS.into_iter().enumerate() {
        let value = format!("value {index} é 🎸");
        let command = Command::SetMeta {
            field,
            value: value.clone(),
        };
        let inverse = round_trip(&mut fx.score, &command);
        apply(&mut fx.score, &command).unwrap();
        let meta = &fx.score.meta;
        let stored = match field {
            MetaField::Title => &meta.title,
            MetaField::Subtitle => &meta.subtitle,
            MetaField::Artist => &meta.artist,
            MetaField::Album => &meta.album,
            MetaField::Words => &meta.words,
            MetaField::Music => &meta.music,
            MetaField::Copyright => &meta.copyright,
            MetaField::TabAuthor => &meta.tab_author,
            MetaField::Instructions => &meta.instructions,
            MetaField::Notices => &meta.notices,
        };
        assert_eq!(stored, &value);
        let Command::SetMeta { field: back, .. } = inverse else {
            panic!("inverse of set_meta is set_meta");
        };
        assert_eq!(back, field);
    }
}

#[test]
fn set_meta_inverse_carries_the_old_value() {
    let mut fx = fixture();
    let command = Command::SetMeta {
        field: MetaField::Title,
        value: "New".to_owned(),
    };
    let inverse = apply(&mut fx.score, &command).unwrap();
    assert_eq!(
        inverse,
        Command::SetMeta {
            field: MetaField::Title,
            value: "Original".to_owned(),
        }
    );
}
