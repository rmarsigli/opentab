use opentab_model::{
    Accent, BeatEffects, Bend, BendPoint, Dynamic, Finger, Fraction, Grace, Harmonic, HarmonicKind,
    NoteEffects, NoteValue, Score, Slide, Stroke, StrokeDirection, StrokeTicks, Tap,
    TremoloPicking, Vibrato,
};

use crate::fixtures::FixtureError;
use crate::fixtures::sketch::{
    BeatSketch, NoteSketch, Sketch, TrackSetup, affected, beat, duration, fretted, rest, voice,
    with_effects,
};

fn chord(fret: u8) -> Vec<NoteSketch> {
    (0..6u8).map(|string| fretted(string, fret)).collect()
}

fn effect_beat(effects: BeatEffects) -> Result<BeatSketch, FixtureError> {
    Ok(with_effects(
        duration(NoteValue::Quarter)?,
        vec![fretted(2, 2)],
        effects,
    ))
}

fn tremolo(value: NoteValue) -> Result<BeatEffects, FixtureError> {
    Ok(BeatEffects {
        tremolo_picking: Some(TremoloPicking::new(value)?),
        ..BeatEffects::default()
    })
}

fn stroke(direction: StrokeDirection, ticks: u16) -> Result<BeatSketch, FixtureError> {
    Ok(with_effects(
        duration(NoteValue::Quarter)?,
        chord(0),
        BeatEffects {
            stroke: Some(Stroke {
                direction,
                duration: StrokeTicks::new(ticks)?,
            }),
            ..BeatEffects::default()
        },
    ))
}

fn named(name: &str, fret: u8) -> Result<BeatSketch, FixtureError> {
    Ok(with_effects(
        duration(NoteValue::Quarter)?,
        chord(fret),
        BeatEffects {
            chord_name: Some(name.to_string()),
            ..BeatEffects::default()
        },
    ))
}

fn graced(grace: Grace, value: NoteValue) -> Result<BeatSketch, FixtureError> {
    Ok(with_effects(
        duration(value)?,
        vec![fretted(2, 4)],
        BeatEffects {
            grace: Some(grace),
            ..BeatEffects::default()
        },
    ))
}

fn dynamic(dynamic: Dynamic) -> Result<BeatSketch, FixtureError> {
    effect_beat(BeatEffects {
        dynamic,
        ..BeatEffects::default()
    })
}

pub fn beat_effects() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Beat effects");
    sketch.master_bars(7, 4, 4)?;
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    let quarter = duration(NoteValue::Quarter)?;
    sketch.fill(
        track,
        0,
        vec![voice(
            0,
            vec![
                effect_beat(tremolo(NoteValue::Eighth)?)?,
                effect_beat(tremolo(NoteValue::Sixteenth)?)?,
                effect_beat(tremolo(NoteValue::ThirtySecond)?)?,
                beat(quarter, vec![fretted(2, 2)]),
            ],
        )],
    )?;
    sketch.fill(
        track,
        1,
        vec![voice(
            0,
            vec![
                stroke(StrokeDirection::Down, 480)?,
                stroke(StrokeDirection::Up, 480)?,
                stroke(StrokeDirection::Down, 120)?,
                stroke(StrokeDirection::Up, 30)?,
            ],
        )],
    )?;
    sketch.fill(
        track,
        2,
        vec![voice(
            0,
            vec![
                named("Am", 0)?,
                named("C", 1)?,
                named("G7", 3)?,
                named("Am", 0)?,
            ],
        )],
    )?;
    sketch.fill(
        track,
        3,
        vec![voice(
            0,
            vec![
                effect_beat(BeatEffects {
                    text: Some("Free text <&> ünï".to_string()),
                    ..BeatEffects::default()
                })?,
                effect_beat(BeatEffects {
                    fade_in: true,
                    ..BeatEffects::default()
                })?,
                effect_beat(BeatEffects {
                    slap: true,
                    ..BeatEffects::default()
                })?,
                effect_beat(BeatEffects {
                    pop: true,
                    ..BeatEffects::default()
                })?,
            ],
        )],
    )?;
    sketch.fill(
        track,
        4,
        vec![voice(
            0,
            vec![
                graced(Grace::BeforeBeat, NoteValue::Eighth)?,
                beat(quarter, vec![fretted(2, 5)]),
                graced(Grace::OnBeat, NoteValue::Sixteenth)?,
                beat(quarter, vec![fretted(2, 5)]),
                beat(quarter, vec![fretted(2, 7)]),
                rest(quarter),
            ],
        )],
    )?;
    sketch.fill(
        track,
        5,
        vec![voice(
            0,
            vec![
                dynamic(Dynamic::Ppp)?,
                dynamic(Dynamic::Pp)?,
                dynamic(Dynamic::P)?,
                dynamic(Dynamic::Mp)?,
            ],
        )],
    )?;
    sketch.fill(
        track,
        6,
        vec![voice(
            0,
            vec![
                dynamic(Dynamic::Mf)?,
                dynamic(Dynamic::F)?,
                dynamic(Dynamic::Ff)?,
                dynamic(Dynamic::Fff)?,
            ],
        )],
    )?;
    Ok(sketch.finish())
}

fn quarter_notes(notes: Vec<NoteSketch>) -> Result<Vec<BeatSketch>, FixtureError> {
    let quarter = duration(NoteValue::Quarter)?;
    Ok(notes
        .into_iter()
        .map(|note| beat(quarter, vec![note]))
        .collect())
}

fn on(string: u8, fret: u8, effects: NoteEffects) -> NoteSketch {
    affected(string, fret, effects)
}

fn slide(slide: Slide) -> NoteEffects {
    NoteEffects {
        slide: Some(slide),
        ..NoteEffects::default()
    }
}

fn harmonic(kind: HarmonicKind, fret: Fraction) -> NoteEffects {
    NoteEffects {
        harmonic: Some(Harmonic { kind, fret }),
        ..NoteEffects::default()
    }
}

fn point(offset: u8, value: i16) -> Result<BendPoint, FixtureError> {
    Ok(BendPoint::new(offset, value)?)
}

fn bend(points: [(u8, i16); 4]) -> Result<NoteEffects, FixtureError> {
    let [origin, start, end, destination] = points;
    Ok(NoteEffects {
        bend: Some(Bend::new(
            point(origin.0, origin.1)?,
            point(start.0, start.1)?,
            point(end.0, end.1)?,
            point(destination.0, destination.1)?,
        )?),
        ..NoteEffects::default()
    })
}

fn left(finger: Finger) -> NoteEffects {
    NoteEffects {
        left_hand_finger: Some(finger),
        ..NoteEffects::default()
    }
}

fn right(finger: Finger) -> NoteEffects {
    NoteEffects {
        right_hand_finger: Some(finger),
        ..NoteEffects::default()
    }
}

const BENDS: [[(u8, i16); 4]; 8] = [
    [(0, 0), (20, 200), (40, 200), (60, 200)],
    [(0, 0), (20, 200), (60, 200), (100, 0)],
    [(0, 200), (30, 200), (60, 200), (100, 200)],
    [(0, 200), (30, 200), (60, 200), (100, 0)],
    [(0, 0), (20, 50), (40, 50), (60, 50)],
    [(0, 0), (30, 75), (50, 75), (100, 75)],
    [(0, 0), (25, -100), (75, -100), (100, 0)],
    [(0, 0), (10, 1200), (50, 1200), (100, 1200)],
];

pub fn note_effects() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Note effects");
    sketch.master_bars(13, 4, 4)?;
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    let tie_both = NoteEffects {
        tie_origin: true,
        tie_destination: true,
        ..NoteEffects::default()
    };
    let bars: Vec<Vec<NoteSketch>> = vec![
        vec![
            on(
                2,
                5,
                NoteEffects {
                    tie_origin: true,
                    ..NoteEffects::default()
                },
            ),
            on(2, 5, tie_both),
            on(
                2,
                5,
                NoteEffects {
                    tie_destination: true,
                    ..NoteEffects::default()
                },
            ),
            on(
                2,
                7,
                NoteEffects {
                    ghost: true,
                    ..NoteEffects::default()
                },
            ),
        ],
        vec![
            on(
                2,
                5,
                NoteEffects {
                    dead: true,
                    ..NoteEffects::default()
                },
            ),
            on(
                2,
                5,
                NoteEffects {
                    accent: Some(Accent::Normal),
                    ..NoteEffects::default()
                },
            ),
            on(
                2,
                5,
                NoteEffects {
                    accent: Some(Accent::Heavy),
                    ..NoteEffects::default()
                },
            ),
            on(
                2,
                5,
                NoteEffects {
                    staccato: true,
                    ..NoteEffects::default()
                },
            ),
        ],
        vec![
            on(
                5,
                0,
                NoteEffects {
                    palm_mute: true,
                    ..NoteEffects::default()
                },
            ),
            on(
                4,
                2,
                NoteEffects {
                    let_ring: true,
                    ..NoteEffects::default()
                },
            ),
            on(
                1,
                8,
                NoteEffects {
                    vibrato: Some(Vibrato::Slight),
                    ..NoteEffects::default()
                },
            ),
            on(
                1,
                8,
                NoteEffects {
                    vibrato: Some(Vibrato::Wide),
                    ..NoteEffects::default()
                },
            ),
        ],
        vec![
            on(
                3,
                5,
                NoteEffects {
                    hammer_on_pull_off: true,
                    ..NoteEffects::default()
                },
            ),
            fretted(3, 7),
            on(
                3,
                7,
                NoteEffects {
                    hammer_on_pull_off: true,
                    ..NoteEffects::default()
                },
            ),
            fretted(3, 5),
        ],
        vec![
            on(2, 5, slide(Slide::Shift)),
            fretted(2, 7),
            on(2, 7, slide(Slide::Legato)),
            fretted(2, 5),
        ],
        vec![
            on(2, 5, slide(Slide::IntoFromBelow)),
            on(2, 5, slide(Slide::IntoFromAbove)),
            on(2, 5, slide(Slide::OutDownwards)),
            on(2, 5, slide(Slide::OutUpwards)),
        ],
        vec![
            on(
                0,
                12,
                harmonic(HarmonicKind::Natural, Fraction::from_integer(12)),
            ),
            on(
                1,
                7,
                harmonic(HarmonicKind::Natural, Fraction::from_integer(7)),
            ),
            on(2, 3, harmonic(HarmonicKind::Natural, Fraction::new(16, 5))),
            on(
                3,
                5,
                harmonic(HarmonicKind::Artificial, Fraction::from_integer(17)),
            ),
        ],
        vec![
            on(
                2,
                5,
                harmonic(HarmonicKind::Pinch, Fraction::from_integer(12)),
            ),
            on(
                2,
                5,
                harmonic(HarmonicKind::Tap, Fraction::from_integer(17)),
            ),
            on(
                2,
                5,
                harmonic(HarmonicKind::Semi, Fraction::from_integer(12)),
            ),
            on(
                2,
                5,
                harmonic(HarmonicKind::Feedback, Fraction::from_integer(12)),
            ),
        ],
        vec![
            on(2, 7, bend(BENDS[0])?),
            on(2, 7, bend(BENDS[1])?),
            on(2, 7, bend(BENDS[2])?),
            on(2, 7, bend(BENDS[3])?),
        ],
        vec![
            on(2, 7, bend(BENDS[4])?),
            on(2, 7, bend(BENDS[5])?),
            on(2, 7, bend(BENDS[6])?),
            on(2, 7, bend(BENDS[7])?),
        ],
        vec![
            on(
                1,
                12,
                NoteEffects {
                    tap: Some(Tap::Right),
                    ..NoteEffects::default()
                },
            ),
            on(
                1,
                5,
                NoteEffects {
                    tap: Some(Tap::Left),
                    ..NoteEffects::default()
                },
            ),
            on(
                1,
                8,
                NoteEffects {
                    accent: Some(Accent::Normal),
                    staccato: true,
                    let_ring: true,
                    vibrato: Some(Vibrato::Slight),
                    palm_mute: true,
                    ..NoteEffects::default()
                },
            ),
            fretted(1, 8),
        ],
    ];
    for (index, notes) in bars.into_iter().enumerate() {
        sketch.fill(track, index, vec![voice(0, quarter_notes(notes)?)])?;
    }
    let quarter = duration(NoteValue::Quarter)?;
    let lefts = vec![
        on(0, 0, left(Finger::Open)),
        on(1, 1, left(Finger::Index)),
        on(2, 2, left(Finger::Middle)),
        on(3, 3, left(Finger::Ring)),
        on(4, 4, left(Finger::Little)),
        on(5, 3, left(Finger::Thumb)),
    ];
    let rights = vec![
        on(1, 0, right(Finger::Ring)),
        on(2, 0, right(Finger::Middle)),
        on(3, 0, right(Finger::Index)),
        on(4, 0, right(Finger::Little)),
        on(5, 0, right(Finger::Thumb)),
    ];
    sketch.fill(
        track,
        11,
        vec![voice(
            0,
            vec![
                beat(quarter, lefts),
                beat(quarter, rights),
                rest(quarter),
                rest(quarter),
            ],
        )],
    )?;
    let ringing = |effects: NoteEffects| NoteEffects {
        let_ring: true,
        ..effects
    };
    let pairs = vec![
        on(
            1,
            7,
            ringing(NoteEffects {
                palm_mute: true,
                ..NoteEffects::default()
            }),
        ),
        on(
            1,
            7,
            ringing(NoteEffects {
                staccato: true,
                ..NoteEffects::default()
            }),
        ),
        on(
            1,
            7,
            ringing(NoteEffects {
                accent: Some(Accent::Normal),
                ..NoteEffects::default()
            }),
        ),
        on(
            1,
            7,
            ringing(NoteEffects {
                vibrato: Some(Vibrato::Slight),
                ..NoteEffects::default()
            }),
        ),
    ];
    sketch.fill(track, 12, vec![voice(0, quarter_notes(pairs)?)])?;
    Ok(sketch.finish())
}

fn marked(ghost: bool, accent: Option<Accent>, staccato: bool) -> NoteEffects {
    NoteEffects {
        ghost,
        accent,
        staccato,
        ..NoteEffects::default()
    }
}

fn dynamic_beats(
    dynamic: Dynamic,
    notes: Vec<NoteSketch>,
) -> Result<Vec<BeatSketch>, FixtureError> {
    let quarter = duration(NoteValue::Quarter)?;
    Ok(notes
        .into_iter()
        .map(|note| {
            with_effects(
                quarter,
                vec![note],
                BeatEffects {
                    dynamic,
                    ..BeatEffects::default()
                },
            )
        })
        .collect())
}

fn accent_row() -> Vec<NoteSketch> {
    vec![
        fretted(2, 5),
        on(2, 5, marked(true, None, false)),
        on(2, 5, marked(false, Some(Accent::Normal), false)),
        on(2, 5, marked(false, Some(Accent::Heavy), false)),
    ]
}

pub fn ghost_accent() -> Result<Score, FixtureError> {
    let mut sketch = Sketch::new("Ghost notes and accents");
    sketch.master_bars(7, 4, 4)?;
    let track = sketch.track(TrackSetup::guitar("Guitar", "Gtr"))?;
    let rows = [
        (Dynamic::Mf, accent_row()),
        (
            Dynamic::Mf,
            vec![
                on(2, 5, marked(true, Some(Accent::Normal), false)),
                on(2, 5, marked(true, Some(Accent::Heavy), false)),
                on(2, 5, marked(false, Some(Accent::Normal), true)),
                on(2, 5, marked(true, None, true)),
            ],
        ),
        (Dynamic::Pp, accent_row()),
        (Dynamic::Ff, accent_row()),
        (Dynamic::Ppp, accent_row()),
        (Dynamic::Fff, accent_row()),
    ];
    for (index, (dynamic, notes)) in rows.into_iter().enumerate() {
        sketch.fill(track, index, vec![voice(0, dynamic_beats(dynamic, notes)?)])?;
    }
    let quarter = duration(NoteValue::Quarter)?;
    sketch.fill(
        track,
        6,
        vec![voice(
            0,
            vec![
                beat(
                    quarter,
                    vec![on(2, 5, marked(true, None, false)), fretted(1, 5)],
                ),
                beat(
                    quarter,
                    vec![
                        on(2, 5, marked(false, Some(Accent::Normal), false)),
                        fretted(1, 5),
                    ],
                ),
                beat(
                    quarter,
                    vec![
                        on(2, 5, marked(false, Some(Accent::Heavy), false)),
                        on(1, 5, marked(true, None, false)),
                    ],
                ),
                rest(quarter),
            ],
        )],
    )?;
    Ok(sketch.finish())
}
