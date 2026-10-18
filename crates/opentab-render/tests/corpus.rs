use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use opentab_gp::read_gp;
use opentab_layout::{DisplayList, LayoutError, LayoutOptions, layout};
use opentab_model::{InstrumentKind, Score};
use opentab_render::{Ratio, to_png, to_svg};
use resvg::tiny_skia::Pixmap;
use resvg::usvg;

const CORPUS_VARIABLE: &str = "OPENTAB_CORPUS";
const EXPECTED_FILES: usize = 27;
const EXPECTED_DRUM_TRACKS: usize = 10;
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
        usvg::Tree::from_str(&svg, &usvg::Options::default())
            .map_err(|error| format!("an svg page does not parse: {error}"))?;
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

fn read(path: &Path) -> Result<Score, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    read_gp(&bytes).map_err(|error| format!("read: {error}"))
}

fn drums(score: &Score) -> Option<usize> {
    score
        .tracks
        .iter()
        .position(|track| track.instrument.kind == InstrumentKind::Percussion)
}

fn render(score: &Score, options: &LayoutOptions) -> Result<Outcome, String> {
    match layout(score, options) {
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
        println!("corpus smoke skipped: set {CORPUS_VARIABLE} to the examples directory");
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
        let score = match read(path) {
            Ok(score) => score,
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        let mut runs = vec![(name.clone(), LayoutOptions::default())];
        if let Some(track) = drums(&score) {
            runs.push((
                format!("{name} (drums only)"),
                LayoutOptions {
                    tracks: Some(vec![track]),
                    ..LayoutOptions::default()
                },
            ));
        }
        for (label, options) in runs {
            match catch_unwind(AssertUnwindSafe(|| render(&score, &options))) {
                Ok(Ok(outcome)) => rows.push(Row {
                    name: label,
                    outcome,
                }),
                Ok(Err(error)) => failures.push(format!("{label}: {error}")),
                Err(_) => failures.push(format!("{label}: panicked")),
            }
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
    assert_eq!(rows.len(), EXPECTED_FILES + EXPECTED_DRUM_TRACKS);
}
