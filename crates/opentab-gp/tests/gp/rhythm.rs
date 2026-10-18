use opentab_model::{Fraction, NoteValue};

use crate::support::{Doc, beat, beat_payloads, gpif_path, read_gpif};

fn with_rhythm(value: &str, extra: &str) -> String {
    Doc {
        rhythm: extra.to_string(),
        ..Doc::default()
    }
    .render()
    .replace(
        "<NoteValue>Whole</NoteValue>",
        &format!("<NoteValue>{value}</NoteValue>"),
    )
}

#[test]
fn every_note_value_name_is_mapped() {
    let names = [
        ("Whole", NoteValue::Whole),
        ("Half", NoteValue::Half),
        ("Quarter", NoteValue::Quarter),
        ("Eighth", NoteValue::Eighth),
        ("16th", NoteValue::Sixteenth),
        ("32nd", NoteValue::ThirtySecond),
        ("64th", NoteValue::SixtyFourth),
        ("128th", NoteValue::OneTwentyEighth),
    ];
    for (name, value) in names {
        let score = read_gpif(&with_rhythm(name, ""));
        assert_eq!(beat(&score).duration.value(), value, "{name}");
    }
}

#[test]
fn dots_are_mapped() {
    let score = read_gpif(&with_rhythm("Quarter", "<AugmentationDot count=\"2\"/>"));
    let duration = beat(&score).duration;
    assert_eq!(duration.dots(), 2);
    assert_eq!(duration.length(), Fraction::new(7, 16));
}

#[test]
fn tuplet_num_is_notes_played_and_den_the_time_taken() {
    let score = read_gpif(&with_rhythm(
        "Eighth",
        "<PrimaryTuplet num=\"3\" den=\"2\"/>",
    ));
    let tuplet = beat(&score).duration.tuplet().expect("tuplet");
    assert_eq!(tuplet.num(), 3);
    assert_eq!(tuplet.den(), 2);
    assert_eq!(beat(&score).duration.length(), Fraction::new(1, 12));
    let score = read_gpif(&with_rhythm(
        "Quarter",
        "<PrimaryTuplet num=\"4\" den=\"3\"/>",
    ));
    let tuplet = beat(&score).duration.tuplet().expect("tuplet");
    assert_eq!((tuplet.num(), tuplet.den()), (4, 3));
}

#[test]
fn unknown_rhythm_children_are_kept_on_the_beat() {
    let score = read_gpif(&with_rhythm(
        "Eighth",
        "<SecondaryTuplet num=\"3\" den=\"2\"/>",
    ));
    assert!(
        beat_payloads(&score).contains(
            &"<Keep parent=\"*Rhythm\" index=\"1\"><SecondaryTuplet num=\"3\" den=\"2\"/></Keep>"
                .to_string()
        )
    );
}

#[test]
fn invalid_rhythms_are_rejected_with_their_path() {
    let error = crate::support::read_gpif_error(&with_rhythm("Breve", ""));
    assert_eq!(gpif_path(&error), "/GPIF/Rhythms/Rhythm/NoteValue");
    let error = crate::support::read_gpif_error(&with_rhythm(
        "Eighth",
        "<PrimaryTuplet num=\"0\" den=\"2\"/>",
    ));
    assert_eq!(gpif_path(&error), "/GPIF/Rhythms/Rhythm/PrimaryTuplet");
    let error = crate::support::read_gpif_error(&with_rhythm("Eighth", "<AugmentationDot/>"));
    assert_eq!(gpif_path(&error), "/GPIF/Rhythms/Rhythm/AugmentationDot");
}
