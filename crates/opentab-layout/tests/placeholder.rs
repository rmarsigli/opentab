use opentab_layout::{
    A4, DEFAULT_STAFF_SPACE, Group, GroupId, Line, Page, Point, Primitive, Units,
    placeholder_layout,
};
use opentab_model::{ElementId, Score, TrackId, blank_score};

fn groups(page: &Page) -> Vec<&Group> {
    page.items
        .iter()
        .map(|item| match item {
            Primitive::Group(group) => group,
            other => panic!("unexpected top-level primitive {other:?}"),
        })
        .collect()
}

fn lines(group: &Group) -> Vec<Line> {
    group
        .items
        .iter()
        .map(|item| match item {
            Primitive::Line(line) => *line,
            other => panic!("unexpected primitive {other:?}"),
        })
        .collect()
}

fn with_tracks(count: usize) -> Score {
    let mut score = blank_score().unwrap();
    let track = score.tracks[0].clone();
    score.tracks.clear();
    for index in 0..count {
        let mut copy = track.clone();
        copy.id = TrackId(ElementId(1000 + u64::try_from(index).unwrap()));
        score.tracks.push(copy);
    }
    score
}

#[test]
fn a_blank_score_gives_one_a4_page_with_a_staff_and_a_tab_staff() {
    let score = blank_score().unwrap();
    let list = placeholder_layout(&score);
    assert_eq!(list.pages.len(), 1);
    let page = &list.pages[0];
    assert_eq!(page.size, A4);
    assert_eq!(page.staff_space, DEFAULT_STAFF_SPACE);
    let groups = groups(page);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, GroupId(score.tracks[0].id.0.0));
    let lines = lines(groups[0]);
    assert_eq!(lines.len(), 11);
    let ys: Vec<i64> = lines.iter().map(|line| line.from.y.0).collect();
    assert_eq!(
        ys,
        vec![
            8777, 9801, 10825, 11849, 12873, 16969, 18505, 20041, 21577, 23113, 24649
        ]
    );
    for line in &lines {
        assert_eq!(line.from.x, Units(8777));
        assert_eq!(line.to.x, Units(122_880 - 8777));
        assert_eq!(line.from.y, line.to.y);
        assert_eq!(line.thickness, Units(133));
        assert_eq!(line.dash, None);
    }
}

#[test]
fn a_track_without_strings_has_only_the_standard_staff() {
    let mut score = blank_score().unwrap();
    score.tracks[0].tuning.strings.clear();
    let list = placeholder_layout(&score);
    assert_eq!(lines(groups(&list.pages[0])[0]).len(), 5);
}

#[test]
fn a_score_without_tracks_gives_one_blank_page() {
    let list = placeholder_layout(&with_tracks(0));
    assert_eq!(list.pages.len(), 1);
    assert!(list.pages[0].items.is_empty());
}

#[test]
fn systems_overflow_onto_new_pages_inside_the_margins() {
    let list = placeholder_layout(&with_tracks(14));
    let per_page: Vec<usize> = list.pages.iter().map(|page| groups(page).len()).collect();
    assert_eq!(per_page, vec![6, 6, 2]);
    let mut ids = Vec::new();
    for page in &list.pages {
        let bottom = page.height() - Units(8777);
        for group in groups(page) {
            ids.push(group.id.0);
            for line in lines(group) {
                assert!(line.from.y >= Units(8777) && line.from.y <= bottom);
            }
        }
        let first = groups(page)[0];
        assert_eq!(lines(first)[0].from, Point::new(8777, 8777));
    }
    assert_eq!(ids, (1000..1014).collect::<Vec<u64>>());
}

#[test]
fn a_system_taller_than_a_page_still_gets_its_own_page() {
    let mut score = with_tracks(2);
    let pitch = score.tracks[0].tuning.strings[0];
    score.tracks[0].tuning.strings = vec![pitch; 200];
    let list = placeholder_layout(&score);
    assert_eq!(list.pages.len(), 2);
    assert_eq!(groups(&list.pages[0]).len(), 1);
}

#[test]
fn the_layout_is_deterministic() {
    let score = with_tracks(9);
    assert_eq!(placeholder_layout(&score), placeholder_layout(&score));
}
