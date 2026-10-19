const FLOOR_REASON: &str = "GP floors microseconds per quarter where we round to the nearest microsecond (T8 quantisation): GP's value is ours minus one";
const DOTTED_REASON: &str = "GP writes a dotted-quarter tempo with factor 1.25 instead of 1.5 (M3-D5): GP's value is ours times 6/5, floored; we export the correct value";
const EARLY_FLOOR_REASON: &str = "GP places this 3/4-bar quarter position one tick early and floors its microseconds: GP's tick and value are ours minus one";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoDefect {
    MicrosFloored,
    DottedQuarterAsFiveQuarters,
    TickBeforeThePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TempoAt {
    pub tick: u64,
    pub micros: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum Rule {
    TempoEvent {
        file: &'static str,
        gp: TempoAt,
        ours: TempoAt,
        defects: &'static [TempoDefect],
    },
    OffGridLength {
        ticks: u64,
    },
    BendTickFloored {
        ticks: u64,
    },
    OrderWithinTick,
    TrillAuxiliaryReleasedOnThePrincipal,
    CodaNeverReached,
}

#[derive(Debug, Clone, Copy)]
pub struct Allowance {
    pub name: &'static str,
    pub reason: &'static str,
    pub rule: Rule,
}

const fn tempo_at(tick: u64, micros: u32) -> TempoAt {
    TempoAt { tick, micros }
}

const fn tempo_event(
    name: &'static str,
    reason: &'static str,
    file: &'static str,
    gp: TempoAt,
    ours: TempoAt,
    defects: &'static [TempoDefect],
) -> Allowance {
    Allowance {
        name,
        reason,
        rule: Rule::TempoEvent {
            file,
            gp,
            ours,
            defects,
        },
    }
}

pub const ALLOWED_DIFFERENCES: [Allowance; 12] = [
    tempo_event(
        "tempo-1920-floor",
        FLOOR_REASON,
        "tempo.gp",
        tempo_at(1920, 1_325_966),
        tempo_at(1920, 1_325_967),
        &[TempoDefect::MicrosFloored],
    ),
    tempo_event(
        "tempo-2880-dotted",
        DOTTED_REASON,
        "tempo.gp",
        tempo_at(2880, 480_000),
        tempo_at(2880, 400_000),
        &[TempoDefect::DottedQuarterAsFiveQuarters],
    ),
    tempo_event(
        "tempo-4320-early-floor",
        EARLY_FLOOR_REASON,
        "tempo.gp",
        tempo_at(4319, 277_777),
        tempo_at(4320, 277_778),
        &[
            TempoDefect::TickBeforeThePosition,
            TempoDefect::MicrosFloored,
        ],
    ),
    tempo_event(
        "tempo-5280-dotted",
        DOTTED_REASON,
        "tempo.gp",
        tempo_at(5280, 360_225),
        tempo_at(5280, 300_188),
        &[TempoDefect::DottedQuarterAsFiveQuarters],
    ),
    tempo_event(
        "tempo-8160-floor",
        FLOOR_REASON,
        "tempo.gp",
        tempo_at(8160, 2_666_666),
        tempo_at(8160, 2_666_667),
        &[TempoDefect::MicrosFloored],
    ),
    tempo_event(
        "tempo-ramp-6720-floor",
        FLOOR_REASON,
        "tempo-ramp.gp",
        tempo_at(6720, 666_666),
        tempo_at(6720, 666_667),
        &[TempoDefect::MicrosFloored],
    ),
    tempo_event(
        "tempo-ramp-7680-dotted",
        DOTTED_REASON,
        "tempo-ramp.gp",
        tempo_at(7680, 600_000),
        tempo_at(7680, 500_000),
        &[TempoDefect::DottedQuarterAsFiveQuarters],
    ),
    Allowance {
        name: "off-grid-length",
        reason: "a tuplet note whose exact start or end falls between ticks: GP rounds each length on its own (69 for 68.57), we round both ends half up from the absolute position (M3-D4)",
        rule: Rule::OffGridLength { ticks: 1 },
    },
    Allowance {
        name: "bend-tick-floored",
        reason: "GP floors bend knot and step ticks per note and its float can land one tick under an exact tick (1919 for 1920); we round half up, so ours is GP's tick or one later",
        rule: Rule::BendTickFloored { ticks: 1 },
    },
    Allowance {
        name: "order-within-one-tick",
        reason: "GP writes a note-on before the track's program and RPN at tick 0; notes, programs and ranges are compared as multisets, bends keep their order per channel",
        rule: Rule::OrderWithinTick,
    },
    Allowance {
        name: "trill-auxiliary-release",
        reason: "GP releases a trill's auxiliary note with a note-off on the principal key, leaving the auxiliary sounding; we pair it on its own key",
        rule: Rule::TrillAuxiliaryReleasedOnThePrincipal,
    },
    Allowance {
        name: "coda-never-reached",
        reason: "when a To Coda lies on no played path, GP's export holds only the bars from the Coda on (gpif.md 5.1 rule 12, a GP defect); we play the whole order; no golden triggers it",
        rule: Rule::CodaNeverReached,
    },
];

fn tolerance(pick: fn(Rule) -> Option<u64>) -> u64 {
    ALLOWED_DIFFERENCES
        .iter()
        .find_map(|allowance| pick(allowance.rule))
        .unwrap_or(0)
}

pub fn off_grid_ticks() -> u64 {
    tolerance(|rule| match rule {
        Rule::OffGridLength { ticks } => Some(ticks),
        _ => None,
    })
}

pub fn bend_ticks() -> u64 {
    tolerance(|rule| match rule {
        Rule::BendTickFloored { ticks } => Some(ticks),
        _ => None,
    })
}

pub fn named(rule: fn(&Rule) -> bool) -> &'static str {
    ALLOWED_DIFFERENCES
        .iter()
        .find(|allowance| rule(&allowance.rule))
        .map_or("missing", |allowance| allowance.name)
}

pub fn explains(defects: &[TempoDefect], gp: TempoAt, ours: TempoAt) -> bool {
    let early = defects.contains(&TempoDefect::TickBeforeThePosition);
    let floored = defects.contains(&TempoDefect::MicrosFloored);
    let dotted = defects.contains(&TempoDefect::DottedQuarterAsFiveQuarters);
    let tick = if early {
        ours.tick.checked_sub(1)
    } else {
        Some(ours.tick)
    };
    let micros = match (floored, dotted) {
        (true, false) => ours.micros.checked_sub(1),
        (false, true) => u32::try_from(u64::from(ours.micros) * 6 / 5).ok(),
        (false, false) => Some(ours.micros),
        (true, true) => None,
    };
    !defects.is_empty() && tick == Some(gp.tick) && micros == Some(gp.micros)
}
