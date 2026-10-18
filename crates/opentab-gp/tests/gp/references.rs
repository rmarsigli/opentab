use opentab_gp::{GpError, GpifProblem, PoolKind};
use opentab_model::ModelError;

use crate::support::{Doc, document_payloads, read_gpif, read_gpif_error};

fn failure(gpif: &str) -> (String, GpifProblem) {
    match read_gpif_error(gpif) {
        GpError::Gpif {
            entry,
            path,
            problem,
        } => {
            assert_eq!(entry, "Content/score.gpif");
            (path, problem)
        }
        other => panic!("unexpected error {other}"),
    }
}

fn base() -> String {
    Doc::default().render()
}

#[test]
fn ids_out_of_document_order_are_rejected() {
    let gpif = base().replace(
        "<Bars><Bar id=\"0\"><Clef>G2</Clef><Voices>0 -1 -1 -1</Voices></Bar></Bars>",
        "<Bars><Bar id=\"1\"><Clef>G2</Clef><Voices>0 -1 -1 -1</Voices></Bar><Bar id=\"0\"><Clef>G2</Clef><Voices>-1 -1 -1 -1</Voices></Bar></Bars>",
    );
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/Bars/Bar[1]".to_string(),
            GpifProblem::IdOutOfOrder {
                kind: PoolKind::Bar,
                expected: 0,
                found: "1".to_string(),
            }
        )
    );
}

#[test]
fn gaps_in_ids_are_rejected() {
    let gpif = base().replace(
        "</Note></Notes>",
        "</Note><Note id=\"2\"><Properties/></Note></Notes>",
    );
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/Notes/Note[2]".to_string(),
            GpifProblem::IdOutOfOrder {
                kind: PoolKind::Note,
                expected: 1,
                found: "2".to_string(),
            }
        )
    );
}

#[test]
fn a_missing_id_attribute_is_rejected() {
    let gpif = base().replace("<Voice id=\"0\">", "<Voice>");
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/Voices/Voice".to_string(),
            GpifProblem::MissingAttribute {
                name: "id".to_string()
            }
        )
    );
}

#[test]
fn references_to_missing_ids_name_the_referencing_element() {
    let cases = [
        (
            base().replace("<Beats>0</Beats>", "<Beats>0 7</Beats>"),
            "/GPIF/Voices/Voice/Beats",
            PoolKind::Beat,
            "7",
        ),
        (
            base().replace("<Rhythm ref=\"0\"/>", "<Rhythm ref=\"3\"/>"),
            "/GPIF/Beats/Beat/Rhythm",
            PoolKind::Rhythm,
            "3",
        ),
        (
            base().replace("<Voices>0 -1 -1 -1</Voices>", "<Voices>5 -1 -1 -1</Voices>"),
            "/GPIF/Bars/Bar/Voices",
            PoolKind::Voice,
            "5",
        ),
        (
            base().replace("<Bars>0</Bars>", "<Bars>4</Bars>"),
            "/GPIF/MasterBars/MasterBar/Bars",
            PoolKind::Bar,
            "4",
        ),
        (
            base().replace("<Tracks>0</Tracks>", "<Tracks>3</Tracks>"),
            "/GPIF/MasterTrack/Tracks",
            PoolKind::Track,
            "3",
        ),
        (
            base().replace("<Notes>0</Notes>", "<Notes>0 -1</Notes>"),
            "/GPIF/Beats/Beat/Notes",
            PoolKind::Note,
            "-1",
        ),
    ];
    for (gpif, path, kind, id) in cases {
        assert_eq!(
            failure(&gpif),
            (
                path.to_string(),
                GpifProblem::MissingReference {
                    kind,
                    id: id.to_string()
                }
            )
        );
    }
}

#[test]
fn malformed_reference_lists_are_rejected() {
    let gpif = base().replace("<Beats>0</Beats>", "<Beats>0 x</Beats>");
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/Voices/Voice/Beats".to_string(),
            GpifProblem::InvalidNumber {
                text: "x".to_string()
            }
        )
    );
}

#[test]
fn unreferenced_elements_are_kept_as_document_payloads() {
    let gpif = base()
        .replace(
            "</Beat></Beats>",
            "</Beat><Beat id=\"1\"><Rhythm ref=\"1\"/><Notes>1</Notes></Beat></Beats>",
        )
        .replace(
            "</Note></Notes>",
            "</Note><Note id=\"1\"><Properties/></Note></Notes>",
        )
        .replace(
            "</Rhythm></Rhythms>",
            "</Rhythm><Rhythm id=\"1\"><NoteValue>Half</NoteValue></Rhythm></Rhythms>",
        );
    let score = read_gpif(&gpif);
    assert_eq!(score.tracks[0].staves[0].bars[0].voices[0].beats.len(), 1);
    let payloads = document_payloads(&score);
    let tail: Vec<&str> = payloads
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(String::as_str)
        .collect();
    assert_eq!(
        tail,
        vec![
            "<Keep parent=\"Beats\" index=\"1\"><Beat id=\"1\"><Rhythm ref=\"1\"/><Notes>1</Notes></Beat></Keep>",
            "<Keep parent=\"Notes\" index=\"1\"><Note id=\"1\"><Properties/></Note></Keep>",
            "<Keep parent=\"Rhythms\" index=\"1\"><Rhythm id=\"1\"><NoteValue>Half</NoteValue></Rhythm></Keep>",
        ]
    );
}

#[test]
fn track_lists_must_be_unique_and_unlisted_tracks_follow() {
    let gpif = base().replace("<Tracks>0</Tracks>", "<Tracks>0 0</Tracks>");
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/MasterTrack/Tracks".to_string(),
            GpifProblem::DuplicateTrackReference { id: 0 }
        )
    );
    let gpif = base().replace("<Tracks>0</Tracks>", "<Tracks></Tracks>");
    let score = read_gpif(&gpif);
    assert_eq!(score.tracks.len(), 1);
    assert_eq!(score.tracks[0].name, "Guitar");
}

#[test]
fn master_bars_must_list_one_bar_per_staff() {
    let gpif = base().replace("<Bars>0</Bars>", "<Bars>0 0</Bars>");
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/MasterBars/MasterBar/Bars".to_string(),
            GpifProblem::StaffCountMismatch {
                expected: 1,
                found: 2
            }
        )
    );
}

#[test]
fn a_fifth_voice_slot_is_rejected() {
    let gpif = base().replace(
        "<Voices>0 -1 -1 -1</Voices>",
        "<Voices>-1 -1 -1 -1 0</Voices>",
    );
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/Bars/Bar/Voices".to_string(),
            GpifProblem::Model(ModelError::InvalidVoiceSlot { slot: 4 })
        )
    );
}

#[test]
fn a_mapped_element_given_twice_is_rejected() {
    let gpif = base().replace("<Clef>G2</Clef>", "<Clef>G2</Clef><Clef>F4</Clef>");
    assert_eq!(
        failure(&gpif),
        (
            "/GPIF/Bars/Bar/Clef[2]".to_string(),
            GpifProblem::DuplicateElement {
                name: "Clef".to_string()
            }
        )
    );
}
