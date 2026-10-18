use opentab_layout::DisplayList;
use opentab_layout::marks::{DroppedMark, MarkKind};
use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub pages: usize,
    pub systems: usize,
    pub bars: usize,
    pub staff_space: i64,
    pub crowded: Vec<usize>,
    pub dropped: Vec<DroppedEntry>,
    pub title_page: bool,
    pub unrendered: Vec<&'static str>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DroppedEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar: Option<usize>,
    pub kind: &'static str,
}

impl Summary {
    pub fn of(list: &DisplayList) -> Summary {
        Summary {
            pages: list.pages.len(),
            systems: list.pages.iter().map(|page| page.systems.len()).sum(),
            bars: bar_count(list),
            staff_space: list.staff_space.micrometres().0,
            crowded: list.crowded.iter().map(|index| index + 1).collect(),
            dropped: list.dropped.iter().map(dropped_entry).collect(),
            title_page: list.title_page,
            unrendered: list
                .unrendered
                .iter()
                .map(|feature| feature.name())
                .collect(),
        }
    }
}

fn bar_count(list: &DisplayList) -> usize {
    list.pages
        .iter()
        .flat_map(|page| &page.systems)
        .filter_map(|system| system.staves.first())
        .map(|staff| staff.bars.len())
        .sum()
}

fn dropped_entry(mark: &DroppedMark) -> DroppedEntry {
    DroppedEntry {
        bar: mark.bar.map(|bar| bar + 1),
        kind: kind_name(mark.kind),
    }
}

fn kind_name(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Title => "title",
        MarkKind::Tempo => "tempo",
        MarkKind::Section => "section",
        MarkKind::Chord => "chord",
        MarkKind::Text => "text",
        MarkKind::Dynamic => "dynamic",
    }
}

#[cfg(test)]
mod tests {
    use opentab_layout::{A4, DEFAULT_STAFF_SPACE, Micrometres, Page, StaffSpace, Unrendered};

    use super::*;

    fn dropped(bar: Option<usize>, kind: MarkKind) -> DroppedMark {
        DroppedMark { bar, kind }
    }

    #[test]
    fn marks_and_bars_are_reported_from_one_with_their_kind_names() {
        let list = DisplayList {
            pages: vec![
                Page::new(A4, DEFAULT_STAFF_SPACE),
                Page::new(A4, DEFAULT_STAFF_SPACE),
            ],
            staff_space: StaffSpace::new(Micrometres(1_209)).unwrap(),
            crowded: vec![0, 4],
            dropped: vec![
                dropped(None, MarkKind::Title),
                dropped(Some(0), MarkKind::Tempo),
                dropped(Some(1), MarkKind::Section),
                dropped(Some(2), MarkKind::Chord),
                dropped(Some(3), MarkKind::Text),
                dropped(Some(9), MarkKind::Dynamic),
            ],
            title_page: true,
            unrendered: vec![Unrendered::Accents, Unrendered::Bends],
        };
        let summary = Summary::of(&list);
        assert_eq!(summary.pages, 2);
        assert_eq!(summary.systems, 0);
        assert_eq!(summary.bars, 0);
        assert_eq!(summary.staff_space, 1_209);
        assert_eq!(summary.crowded, vec![1, 5]);
        assert!(summary.title_page);
        assert_eq!(summary.unrendered, vec!["accents", "bends"]);
        let kinds: Vec<(Option<usize>, &str)> = summary
            .dropped
            .iter()
            .map(|entry| (entry.bar, entry.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (None, "title"),
                (Some(1), "tempo"),
                (Some(2), "section"),
                (Some(3), "chord"),
                (Some(4), "text"),
                (Some(10), "dynamic"),
            ]
        );
    }
}
