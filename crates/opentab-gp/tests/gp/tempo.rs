use opentab_gp::{GpifProblem, read_gp, write_gp};
use opentab_model::{Fraction, Score, TempoUnit};

use crate::anchors::gpif_text;

use crate::support::{Doc, document_payloads, gpif_path, master_bar, payloads};

fn automation(bar: &str, position: &str, value: &str) -> String {
    format!(
        "<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>{bar}</Bar><Position>{position}</Position><Visible>true</Visible><Value>{value}</Value></Automation>"
    )
}

fn with_automations(automations: &str, time: &str) -> Doc {
    Doc {
        master_track: format!("<Automations>{automations}</Automations>"),
        time: time.to_string(),
        ..Doc::default()
    }
}

#[test]
fn tempo_value_and_unit_are_mapped() {
    let codes = [
        ("1", TempoUnit::Eighth),
        ("2", TempoUnit::Quarter),
        ("3", TempoUnit::DottedQuarter),
        ("4", TempoUnit::Half),
        ("5", TempoUnit::DottedHalf),
        ("0", TempoUnit::Quarter),
    ];
    for (code, unit) in codes {
        let doc = with_automations(&automation("0", "0", &format!("137.5 {code}")), "4/4");
        let score = doc.read();
        let change = &master_bar(&score).tempo_changes[0];
        assert_eq!(change.bpm.bpm(), 137.5);
        assert_eq!(change.unit, unit, "code {code}");
        assert_eq!(change.position, Fraction::from_integer(0));
    }
}

#[test]
fn position_is_rounded_and_every_extra_guarded_by_it() {
    let doc = with_automations(&automation("0", "0.33333", "120 2"), "3/4");
    let score = doc.read();
    let bar = master_bar(&score);
    assert_eq!(bar.tempo_changes[0].position, Fraction::new(1, 4));
    assert_eq!(
        payloads(&score, bar.id.0),
        vec![
            "<Keep parent=\"*Tempo#0\" index=\"1\" guard=\"tempo-linear:false;1/4;time 3/4\"><Linear>false</Linear></Keep>".to_string(),
            "<Keep parent=\"*Tempo#0\" index=\"3\" guard=\"tempo-position:1/4;time 3/4\"><Position>0.33333</Position></Keep>".to_string(),
            "<Keep parent=\"*Tempo#0\" index=\"4\" guard=\"tempo-position:1/4;time 3/4\"><Visible>true</Visible></Keep>".to_string(),
        ]
    );
}

#[test]
fn a_position_in_the_writer_form_is_mapped() {
    let doc = with_automations(&automation("0", "0.333333", "120 2"), "3/4");
    let score = doc.read();
    let bar = master_bar(&score);
    assert_eq!(bar.tempo_changes[0].position, Fraction::new(1, 4));
    assert!(
        payloads(&score, bar.id.0)
            .iter()
            .all(|payload| !payload.contains("<Position>"))
    );
}

#[test]
fn several_tempos_in_one_bar_and_other_automations() {
    let sync = "<Automation><Type>SyncPoint</Type><Bar>0</Bar><Value><BarIndex>0</BarIndex></Value></Automation>";
    let automations = format!(
        "{}{}{sync}",
        automation("0", "0", "100 2"),
        automation("0", "0.5", "80 1")
    );
    let score = with_automations(&automations, "4/4").read();
    let changes = &master_bar(&score).tempo_changes;
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[1].position, Fraction::new(1, 2));
    assert_eq!(changes[1].unit, TempoUnit::Eighth);
    assert!(
        payloads(&score, master_bar(&score).id.0)
            .iter()
            .any(|payload| payload.starts_with("<Keep parent=\"*Tempo#1\" index=\"4\""))
    );
    assert!(document_payloads(&score).contains(&format!(
        "<Keep parent=\"MasterTrack/Automations\" index=\"2\" bars=\"-\">{sync}</Keep>"
    )));
}

#[test]
fn a_tempo_for_a_missing_bar_is_rejected() {
    let error = with_automations(&automation("4", "0", "120 2"), "4/4").error();
    assert_eq!(
        gpif_path(&error),
        "/GPIF/MasterTrack/Automations/Automation/Bar"
    );
    assert!(matches!(
        error,
        opentab_gp::GpError::Gpif {
            problem: GpifProblem::TempoBarOutOfRange {
                bar: 4,
                master_bars: 1
            },
            ..
        }
    ));
}

#[test]
fn invalid_tempo_values_are_rejected() {
    for value in ["fast 2", "120 9", "0 2", "NaN 2", "120 2 7"] {
        let error = with_automations(&automation("0", "0", value), "4/4").error();
        assert_eq!(
            gpif_path(&error),
            "/GPIF/MasterTrack/Automations/Automation/Value",
            "{value}"
        );
    }
    let error = with_automations(&automation("0", "-0.5", "120 2"), "4/4").error();
    assert_eq!(
        gpif_path(&error),
        "/GPIF/MasterTrack/Automations/Automation/Position"
    );
}

#[test]
fn set_tempo_keeps_the_extras_of_a_later_tempo_change_in_the_bar() {
    let later = concat!(
        "<Automation><Type>Tempo</Type><Linear>true</Linear><Bar>0</Bar>",
        "<Position>0.5</Position><Visible>true</Visible>",
        "<Text><![CDATA[ramp]]></Text><Value>90 2</Value></Automation>"
    );
    let mut score = with_automations(later, "4/4").read();
    let set = opentab_command::Command::SetTempo {
        master_bar: 0,
        bpm: 140.0,
    };
    opentab_command::apply(&mut score, &set).expect("set_tempo applies");
    let changes = &master_bar(&score).tempo_changes;
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].position, Fraction::from_integer(0));
    assert_eq!(changes[1].position, Fraction::new(1, 2));
    let bytes = opentab_gp::write_gp(&score).expect("write");
    let again = opentab_gp::read_gp(&bytes).expect("read");
    let carried: Vec<String> = payloads(&again, master_bar(&again).id.0);
    assert!(changes[1].linear);
    assert!(!changes[0].linear);
    assert!(
        carried
            .iter()
            .any(|data| data.contains("*Tempo#1") && data.contains("ramp")),
        "{carried:?}"
    );
    assert!(
        !carried.iter().any(|data| data.contains("*Tempo#0")),
        "{carried:?}"
    );
    assert_eq!(again.master_bars[0].tempo_changes, *changes);
}

fn ramp(linear: &str) -> String {
    format!(
        "<Automation><Type>Tempo</Type><Linear>{linear}</Linear><Bar>0</Bar><Position>0</Position><Visible>true</Visible><Value>60 2</Value></Automation><Automation><Type>Tempo</Type><Linear>false</Linear><Bar>0</Bar><Position>0.5</Position><Visible>true</Visible><Value>120 2</Value></Automation>"
    )
}

fn tempo_blocks(score: &Score) -> Vec<String> {
    let text = gpif_text(&write_gp(score).expect("write"));
    text.split("<Automation>")
        .skip(1)
        .map(|block| {
            block
                .split("</Automation>")
                .next()
                .unwrap_or_default()
                .replace('\n', "")
        })
        .collect()
}

#[test]
fn a_linear_tempo_is_typed_and_written_back_in_place() {
    let score = with_automations(&ramp("true"), "4/4").read();
    let changes = &master_bar(&score).tempo_changes;
    assert_eq!(
        changes
            .iter()
            .map(|change| change.linear)
            .collect::<Vec<_>>(),
        vec![true, false]
    );
    assert!(
        payloads(&score, master_bar(&score).id.0)
            .iter()
            .all(|payload| !payload.contains("<Linear>true"))
    );
    let blocks = tempo_blocks(&score);
    assert_eq!(
        blocks[0],
        "<Type>Tempo</Type><Linear>true</Linear><Bar>0</Bar><Position>0</Position><Visible>true</Visible><Value>60 2</Value>"
    );
    assert_eq!(
        blocks[1],
        "<Type>Tempo</Type><Linear>false</Linear><Bar>0</Bar><Position>0.5</Position><Visible>true</Visible><Value>120 2</Value>"
    );
    let again = read_gp(&write_gp(&score).unwrap()).unwrap();
    assert_eq!(again, score);
}

#[test]
fn flipping_linear_replaces_the_kept_flag() {
    let mut score = with_automations(&ramp("true"), "4/4").read();
    score.master_bars[0].tempo_changes[0].linear = false;
    score.master_bars[0].tempo_changes[1].linear = true;
    let blocks = tempo_blocks(&score);
    assert!(!blocks[0].contains("<Linear>"), "{}", blocks[0]);
    assert!(blocks[1].contains("<Linear>true</Linear>"), "{}", blocks[1]);
    assert!(
        !blocks[1].contains("<Linear>false</Linear>"),
        "{}",
        blocks[1]
    );
    let again = read_gp(&write_gp(&score).unwrap()).unwrap();
    let linear: Vec<bool> = again.master_bars[0]
        .tempo_changes
        .iter()
        .map(|change| change.linear)
        .collect();
    assert_eq!(linear, vec![false, true]);
}

#[test]
fn a_linear_flag_in_another_spelling_is_kept_with_its_meaning() {
    let score = with_automations(&ramp(" true "), "4/4").read();
    assert!(master_bar(&score).tempo_changes[0].linear);
    let blocks = tempo_blocks(&score);
    assert!(
        blocks[0].contains("<Linear> true </Linear>"),
        "{}",
        blocks[0]
    );
    assert_eq!(blocks[0].matches("<Linear>").count(), 1, "{}", blocks[0]);
    let mut flipped = score.clone();
    flipped.master_bars[0].tempo_changes[0].linear = false;
    assert!(!tempo_blocks(&flipped)[0].contains("<Linear>"));
    let unreadable = with_automations(&ramp("yes"), "4/4").read();
    assert!(!master_bar(&unreadable).tempo_changes[0].linear);
    assert!(tempo_blocks(&unreadable)[0].contains("<Linear>yes</Linear>"));
}
