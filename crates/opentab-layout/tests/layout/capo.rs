use opentab_layout::marks::{CREDIT_EM, capo_notes};
use opentab_layout::{DisplayList, StaffMode, TextRun};
use opentab_model::{InstrumentKind, Score};

use crate::marks::{block_texts, text_ink};
use crate::music::{options, render};
use crate::support::{add_track, content, guitar, quarter_bars};

fn runs(list: &DisplayList) -> Vec<TextRun> {
    block_texts(&list.pages[0]).into_iter().cloned().collect()
}

fn capo_runs(list: &DisplayList) -> Vec<TextRun> {
    runs(list)
        .into_iter()
        .filter(|run| run.text.contains("Capo fret"))
        .collect()
}

fn titled(capo: u8) -> Score {
    let mut score = guitar(&quarter_bars(2));
    score.meta.title = "Song".to_string();
    score.tracks[0].capo = capo;
    score
}

#[test]
fn a_capo_is_printed_left_under_the_title_only_when_set() {
    let list = render(&titled(3), &options(StaffMode::Both));
    let found = capo_runs(&list);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "Capo fret 3");
    assert_eq!(found[0].em, CREDIT_EM);
    let ink = text_ink(&found[0]).unwrap();
    assert_eq!(ink.min.x, content(&list.pages[0]).min.x);
    let title = runs(&list)
        .into_iter()
        .find(|run| run.text == "Song")
        .unwrap();
    assert!(ink.min.y > text_ink(&title).unwrap().max.y);
    let system = &list.pages[0].systems[0];
    assert!(ink.max.y < system.rect.min.y);
    assert!(capo_runs(&render(&titled(0), &options(StaffMode::Both))).is_empty());
    let mut untitled = titled(2);
    untitled.meta.title.clear();
    assert_eq!(
        capo_runs(&render(&untitled, &options(StaffMode::Both)))[0].text,
        "Capo fret 2"
    );
}

#[test]
fn several_tracks_name_the_track_whose_capo_is_set() {
    let mut score = titled(0);
    let bass = add_track(&mut score, "Second\tguitar", 6, InstrumentKind::Fretted);
    score.tracks[bass].capo = 5;
    let texts: Vec<String> = capo_runs(&render(&score, &options(StaffMode::Both)))
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert_eq!(texts, vec!["Second guitar: Capo fret 5"]);
    assert_eq!(
        capo_notes(&[("A", 1), ("B", 0), ("C", 7)]),
        vec!["A: Capo fret 1".to_string(), "C: Capo fret 7".to_string()]
    );
    assert_eq!(capo_notes(&[("A", 4)]), vec!["Capo fret 4".to_string()]);
    assert!(capo_notes(&[("A", 0), ("B", 0)]).is_empty());
}
