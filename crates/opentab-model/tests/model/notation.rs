use opentab_model::{
    Bend, BendPoint, Dynamic, ModelError, NoteValue, StrokeTicks, TempoUnit, Transpose,
    TremoloPicking, VoiceSlot,
};

#[test]
fn voice_slot_accepts_zero_to_three_only() {
    for slot in 0..=3 {
        assert_eq!(VoiceSlot::new(slot).unwrap().get(), slot);
    }
    for slot in [4, 5, 255] {
        assert_eq!(
            VoiceSlot::new(slot),
            Err(ModelError::InvalidVoiceSlot { slot })
        );
    }
    assert_eq!(VoiceSlot::default().get(), 0);
    assert_eq!(
        serde_json::to_string(&VoiceSlot::new(3).unwrap()).unwrap(),
        "3"
    );
    assert!(serde_json::from_str::<VoiceSlot>("4").is_err());
    assert!(serde_json::from_str::<VoiceSlot>("-1").is_err());
}

#[test]
fn transpose_accepts_inclusive_bounds() {
    for (chromatic, octave) in [(0, 0), (-12, -3), (12, 3), (-11, -1), (5, 0)] {
        let transpose = Transpose::new(chromatic, octave).unwrap();
        assert_eq!(
            (transpose.chromatic(), transpose.octave()),
            (chromatic, octave)
        );
    }
    assert_eq!(Transpose::default(), Transpose::new(0, 0).unwrap());
}

#[test]
fn transpose_rejects_values_just_outside() {
    for (chromatic, octave) in [(13, 0), (-13, 0), (0, 4), (0, -4), (i8::MAX, i8::MIN)] {
        assert_eq!(
            Transpose::new(chromatic, octave),
            Err(ModelError::InvalidTranspose { chromatic, octave })
        );
    }
    let error = serde_json::from_str::<Transpose>(r#"{"chromatic":13,"octave":0}"#).unwrap_err();
    assert!(error.to_string().contains("transpose"), "{error}");
}

#[test]
fn stroke_ticks_accept_the_slider_range() {
    for ticks in [30, 31, 45, 147, 479, 480] {
        assert_eq!(StrokeTicks::new(ticks).unwrap().ticks(), ticks);
    }
    for ticks in [0, 1, 29, 481, 960, u16::MAX] {
        assert_eq!(
            StrokeTicks::new(ticks),
            Err(ModelError::InvalidStrokeTicks { ticks })
        );
    }
    assert_eq!(StrokeTicks::default().ticks(), 480);
    assert!(serde_json::from_str::<StrokeTicks>("29").is_err());
    assert_eq!(
        serde_json::to_string(&StrokeTicks::new(60).unwrap()).unwrap(),
        "60"
    );
}

#[test]
fn tremolo_picking_accepts_only_the_three_gp_values() {
    for value in [
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ] {
        assert_eq!(TremoloPicking::new(value).unwrap().value(), value);
    }
    for value in [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::SixtyFourth,
        NoteValue::OneTwentyEighth,
    ] {
        assert_eq!(
            TremoloPicking::new(value),
            Err(ModelError::InvalidTremolo { value })
        );
    }
    assert!(serde_json::from_str::<TremoloPicking>("\"sixty_fourth\"").is_err());
    assert_eq!(
        serde_json::to_string(&TremoloPicking::new(NoteValue::Sixteenth).unwrap()).unwrap(),
        "\"sixteenth\""
    );
}

#[test]
fn bend_point_value_is_validated_in_cents() {
    for (offset, value) in [(0, 0), (100, 1200), (100, -1200), (50, 37)] {
        let point = BendPoint::new(offset, value).unwrap();
        assert_eq!((point.offset(), point.value()), (offset, value));
    }
    for (offset, value) in [(101, 0), (0, 1201), (0, -1201), (0, i16::MAX)] {
        assert_eq!(
            BendPoint::new(offset, value),
            Err(ModelError::InvalidBendPoint { offset, value })
        );
    }
}

#[test]
fn bend_middles_share_one_value() {
    let point = |offset, value| BendPoint::new(offset, value).unwrap();
    let bend = Bend::new(point(0, 0), point(20, 100), point(60, 100), point(100, 50)).unwrap();
    assert_eq!(bend.middle_value(), 100);
    assert_eq!(bend.origin(), point(0, 0));
    assert_eq!(bend.middle_start(), point(20, 100));
    assert_eq!(bend.middle_end(), point(60, 100));
    assert_eq!(bend.destination(), point(100, 50));
    assert_eq!(
        Bend::new(point(0, 0), point(20, 100), point(60, 99), point(100, 50)),
        Err(ModelError::InvalidBend {
            start: 100,
            end: 99
        })
    );
    let text = serde_json::to_string(&bend).unwrap();
    assert_eq!(serde_json::from_str::<Bend>(&text).unwrap(), bend);
    let skewed = text.replace(
        r#""middle_end":{"offset":60,"value":100}"#,
        r#""middle_end":{"offset":60,"value":99}"#,
    );
    assert_ne!(skewed, text);
    assert!(serde_json::from_str::<Bend>(&skewed).is_err());
}

#[test]
fn defaults_follow_the_format() {
    assert_eq!(Dynamic::default(), Dynamic::Mf);
    assert_eq!(TempoUnit::default(), TempoUnit::Quarter);
}
