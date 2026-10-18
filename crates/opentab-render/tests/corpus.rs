use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_layout::{DisplayList, LayoutError, LayoutOptions, layout};
use opentab_render::{Ratio, to_png, to_svg};
use resvg::tiny_skia::Pixmap;

const CORPUS_VARIABLE: &str = "OPENTAB_CORPUS";
const EXPECTED_FILES: usize = 27;
const PNG_PAGE_ONE: (u32, u32) = (794, 1123);

struct Row {
    name: String,
    outcome: Outcome,
}

enum Outcome {
    Rendered(Summary),
    Limit(String),
}

struct Summary {
    pages: usize,
    systems: usize,
    staff_space: i64,
    crowded: usize,
    dropped: usize,
    unrendered: Vec<&'static str>,
    svg_bytes: usize,
}

fn corpus() -> Option<Vec<PathBuf>> {
    let directory = std::env::var(CORPUS_VARIABLE).ok()?;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("corpus directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "gp"))
        .collect();
    paths.sort();
    Some(paths)
}

fn summarise(list: &DisplayList) -> Result<Summary, String> {
    let mut svg_bytes = 0;
    for page in &list.pages {
        let svg = to_svg(page);
        if !svg.starts_with("<svg ") {
            return Err("an svg page does not start with <svg".to_string());
        }
        svg_bytes += svg.len();
    }
    let first = list.pages.first().ok_or("no page")?;
    let png = to_png(first, Ratio::from_integer(1)).map_err(|error| error.to_string())?;
    let pixmap = Pixmap::decode_png(&png).map_err(|error| error.to_string())?;
    if (pixmap.width(), pixmap.height()) != PNG_PAGE_ONE {
        return Err(format!(
            "page 1 png is {}x{}",
            pixmap.width(),
            pixmap.height()
        ));
    }
    Ok(Summary {
        pages: list.pages.len(),
        systems: list.pages.iter().map(|page| page.systems.len()).sum(),
        staff_space: list.staff_space.micrometres().0,
        crowded: list.crowded.len(),
        dropped: list.dropped.len(),
        unrendered: list
            .unrendered
            .iter()
            .map(|feature| feature.name())
            .collect(),
        svg_bytes,
    })
}

fn render(path: &Path) -> Result<Outcome, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let score = read_gp(&bytes).map_err(|error| format!("read: {error}"))?;
    match layout(&score, &LayoutOptions::default()) {
        Ok(list) => summarise(&list).map(Outcome::Rendered),
        Err(error @ (LayoutError::PageTooSmall | LayoutError::Rhythm { .. })) => {
            Ok(Outcome::Limit(error.to_string()))
        }
        Err(error) => Err(format!("layout: {error}")),
    }
}

fn line(row: &Row) -> String {
    match &row.outcome {
        Outcome::Rendered(summary) => format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            row.name,
            summary.pages,
            summary.systems,
            summary.staff_space,
            summary.crowded,
            summary.dropped,
            if summary.unrendered.is_empty() {
                "-".to_string()
            } else {
                summary.unrendered.join(", ")
            },
            summary.svg_bytes
        ),
        Outcome::Limit(reason) => format!("| {} | layout limit: {reason} |", row.name),
    }
}

#[test]
fn every_corpus_example_lays_out_and_renders_every_page() {
    let Some(paths) = corpus() else {
        return;
    };
    assert_eq!(paths.len(), EXPECTED_FILES);
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    for path in &paths {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match catch_unwind(AssertUnwindSafe(|| render(path))) {
            Ok(Ok(outcome)) => rows.push(Row { name, outcome }),
            Ok(Err(error)) => failures.push(format!("{name}: {error}")),
            Err(_) => failures.push(format!("{name}: panicked")),
        }
    }
    println!(
        "| file | pages | systems | staff space (um) | crowded bars | dropped marks | unrendered | svg bytes |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for row in &rows {
        println!("{}", line(row));
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(rows.len(), EXPECTED_FILES);
}
