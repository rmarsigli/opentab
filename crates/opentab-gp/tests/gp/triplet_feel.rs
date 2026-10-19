use opentab_command::{Command, apply};
use opentab_gp::{read_gp, write_gp};
use opentab_model::{Score, TRIPLET_FEELS, TripletFeel};

use crate::anchors::gpif_text;
use crate::inventory::master_bar_children;
use crate::support::{Doc, master_bar, payloads};

const FEEL_NAMES: [(&str, TripletFeel); 6] = [
    ("Triplet8th", TripletFeel::TripletEighth),
    ("Triplet16th", TripletFeel::TripletSixteenth),
    ("Dotted8th", TripletFeel::DottedEighth),
    ("Dotted16th", TripletFeel::DottedSixteenth),
    ("Scottish8th", TripletFeel::ScottishEighth),
    ("Scottish16th", TripletFeel::ScottishSixteenth),
];

fn with_master_bar(extra: &str) -> Doc {
    Doc {
        master_bar: extra.to_string(),
        ..Doc::default()
    }
}

fn written(score: &Score) -> String {
    gpif_text(&write_gp(score).expect("write")).replace('\n', "")
}

fn set_feel(score: &mut Score, triplet_feel: TripletFeel) -> Command {
    apply(
        score,
        &Command::SetTripletFeel {
            master_bar: 0,
            triplet_feel,
        },
    )
    .expect("set_triplet_feel")
}

#[test]
fn every_guitar_pro_feel_name_is_typed_and_written_back_unchanged() {
    for (name, feel) in FEEL_NAMES {
        let element = format!("<TripletFeel>{name}</TripletFeel>");
        let score = with_master_bar(&element).read();
        assert_eq!(master_bar(&score).triplet_feel, feel, "{name}");
        assert!(payloads(&score, master_bar(&score).id.0).is_empty());
        let text = written(&score);
        assert!(text.contains(&element), "{name}");
        assert_eq!(
            read_gp(&write_gp(&score).expect("write")).expect("read"),
            score
        );
    }
    let plain = Doc::default().read();
    assert_eq!(master_bar(&plain).triplet_feel, TripletFeel::None);
    assert!(!written(&plain).contains("TripletFeel"));
    assert_eq!(TRIPLET_FEELS.len(), FEEL_NAMES.len() + 1);
}

#[test]
fn a_feel_outside_the_writer_form_is_replayed_while_the_feel_is_unchanged() {
    let cases: [(&str, TripletFeel, &str); 3] = [
        ("<TripletFeel>None</TripletFeel>", TripletFeel::None, "None"),
        (
            "<TripletFeel>Swing</TripletFeel>",
            TripletFeel::None,
            "None",
        ),
        (
            "<TripletFeel> Dotted8th </TripletFeel>",
            TripletFeel::DottedEighth,
            "Dotted8th",
        ),
    ];
    for (element, feel, guard) in cases {
        let score = with_master_bar(element).read();
        assert_eq!(master_bar(&score).triplet_feel, feel, "{element}");
        assert_eq!(
            payloads(&score, master_bar(&score).id.0),
            vec![format!(
                "<Keep parent=\"\" index=\"2\" guard=\"triplet-feel:{guard}\">{element}</Keep>"
            )]
        );
        let text = written(&score);
        assert!(text.contains(element), "{element}");
        assert_eq!(text.matches("<TripletFeel").count(), 1, "{element}");
        let mut changed = score.clone();
        set_feel(&mut changed, TripletFeel::ScottishSixteenth);
        let text = written(&changed);
        assert!(!text.contains(element), "{element}");
        assert!(text.contains("<TripletFeel>Scottish16th</TripletFeel>"));
        assert_eq!(text.matches("<TripletFeel").count(), 1, "{element}");
    }
}

#[test]
fn setting_and_clearing_a_feel_is_undone_exactly() {
    let mut score = Doc::default().read();
    let before = score.clone();
    let inverse = set_feel(&mut score, TripletFeel::TripletEighth);
    assert_eq!(
        inverse,
        Command::SetTripletFeel {
            master_bar: 0,
            triplet_feel: TripletFeel::None,
        }
    );
    assert!(written(&score).contains("<TripletFeel>Triplet8th</TripletFeel>"));
    apply(&mut score, &inverse).expect("undo");
    assert_eq!(score, before);
    assert!(!written(&score).contains("TripletFeel"));
}

#[test]
fn a_typed_feel_lands_in_guitar_pro_child_order() {
    let score = with_master_bar(concat!(
        "<Repeat start=\"true\" end=\"true\" count=\"2\"/>",
        "<AlternateEndings>1</AlternateEndings><DoubleBar/>",
        "<Directions><Target>Segno</Target></Directions>"
    ))
    .read();
    let mut felt = score.clone();
    set_feel(&mut felt, TripletFeel::DottedSixteenth);
    let text = gpif_text(&write_gp(&felt).expect("write"));
    assert_eq!(
        master_bar_children(&text).remove(0),
        [
            "Key",
            "Time",
            "Repeat",
            "AlternateEndings",
            "TripletFeel",
            "DoubleBar",
            "Directions",
            "Bars"
        ]
    );
}

#[test]
fn inserted_bars_have_no_feel_and_a_removed_bar_comes_back_with_its_feel() {
    let mut score = with_master_bar("<TripletFeel>Scottish8th</TripletFeel>").read();
    let time_signature = score.master_bars[0].time_signature;
    apply(
        &mut score,
        &Command::InsertMasterBar {
            index: 1,
            time_signature,
        },
    )
    .expect("insert");
    assert_eq!(score.master_bars[1].triplet_feel, TripletFeel::None);
    let before = score.clone();
    let inverse = apply(&mut score, &Command::RemoveMasterBar { index: 0 }).expect("remove");
    let Command::RestoreMasterBar { master_bar, .. } = &inverse else {
        panic!("restore_master_bar expected");
    };
    assert_eq!(master_bar.triplet_feel, TripletFeel::ScottishEighth);
    apply(&mut score, &inverse).expect("restore");
    assert_eq!(score, before);
}
