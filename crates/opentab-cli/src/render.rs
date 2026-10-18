use std::ffi::OsString;
use std::path::{Path, PathBuf};

use opentab_layout::marks::{DroppedMark, MarkKind};
use opentab_layout::{BarRange, DisplayList, LayoutError, LayoutOptions, StaffMode, layout};
use opentab_model::Score;
use opentab_render::{Ratio, RenderError, to_png, to_svg};
use serde::Serialize;

use crate::error::CliError;
use crate::files::{self, Format};
use crate::output::Rendered;
use crate::text::escape_controls;

const MAX_SCALE_DIGITS: usize = 9;

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum ImageFormat {
    Svg,
    Png,
}

impl ImageFormat {
    fn extension(self) -> &'static str {
        match self {
            ImageFormat::Svg => "svg",
            ImageFormat::Png => "png",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            ImageFormat::Svg => ".svg",
            ImageFormat::Png => ".png",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum StaffChoice {
    Standard,
    Tab,
    Both,
}

impl StaffChoice {
    fn mode(self) -> StaffMode {
        match self {
            StaffChoice::Standard => StaffMode::Standard,
            StaffChoice::Tab => StaffMode::Tab,
            StaffChoice::Both => StaffMode::Both,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BarSpan {
    pub first: u32,
    pub last: u32,
}

#[derive(Serialize)]
pub struct RenderedPages {
    pages: usize,
    systems: usize,
    bars: usize,
    staff_space: i64,
    crowded: Vec<usize>,
    dropped: Vec<DroppedEntry>,
    title_page: bool,
    files: Vec<String>,
}

#[derive(Serialize)]
pub struct DroppedEntry {
    bar: usize,
    kind: &'static str,
}

fn kind_name(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Tempo => "tempo",
        MarkKind::Section => "section",
        MarkKind::Chord => "chord",
        MarkKind::Text => "text",
        MarkKind::Dynamic => "dynamic",
    }
}

fn dropped_entries(dropped: &[DroppedMark]) -> Vec<DroppedEntry> {
    dropped
        .iter()
        .map(|mark| DroppedEntry {
            bar: mark.bar + 1,
            kind: kind_name(mark.kind),
        })
        .collect()
}

pub struct RenderRequest<'a> {
    pub path: &'a Path,
    pub format: ImageFormat,
    pub out: &'a Path,
    pub scale: Option<Ratio<u32>>,
    pub tracks: Vec<u32>,
    pub bars: Option<BarSpan>,
    pub staff: Option<StaffChoice>,
}

fn options(request: &RenderRequest<'_>, score: &Score) -> Result<LayoutOptions, CliError> {
    let track_count = score.tracks.len();
    let mut tracks = Vec::with_capacity(request.tracks.len());
    for number in &request.tracks {
        let index = usize::try_from(*number)
            .ok()
            .and_then(|number| number.checked_sub(1))
            .filter(|index| *index < track_count)
            .ok_or_else(|| CliError::InvalidInput {
                message: format!(
                    "--track {number}: the score has {track_count} track(s), numbered from 1"
                ),
            })?;
        tracks.push(index);
    }
    let bar_count = score.master_bars.len();
    let bars = match request.bars {
        Some(span) => {
            let index = |number: u32| usize::try_from(number).ok().and_then(|n| n.checked_sub(1));
            match (index(span.first), index(span.last)) {
                (Some(first), Some(last)) if first <= last && last < bar_count => {
                    Some(BarRange { first, last })
                }
                _ => {
                    return Err(CliError::InvalidInput {
                        message: format!(
                            "--bars {}-{}: the score has {bar_count} bar(s), numbered from 1",
                            span.first, span.last
                        ),
                    });
                }
            }
        }
        None => None,
    };
    Ok(LayoutOptions {
        tracks: (!tracks.is_empty()).then_some(tracks),
        bars,
        staff_mode: request.staff.map(StaffChoice::mode),
        ..LayoutOptions::default()
    })
}

fn bar_count(list: &DisplayList) -> usize {
    list.pages
        .iter()
        .flat_map(|page| &page.systems)
        .filter_map(|system| system.staves.first())
        .map(|staff| staff.bars.len())
        .sum()
}

pub fn run(request: &RenderRequest<'_>) -> Result<Rendered<RenderedPages>, CliError> {
    let input_format = files::format_of(request.path)?;
    if request.scale.is_some() && request.format == ImageFormat::Svg {
        return Err(CliError::InvalidInput {
            message: "--scale applies to png output only".to_string(),
        });
    }
    let into_directory = request.out.is_dir();
    if !into_directory {
        require_extension(request.out, request.format)?;
    }
    let score = files::read_score(request.path)?;
    let options = options(request, &score)?;
    let list = layout(&score, &options).map_err(layout_failure)?;
    let staff_space = list.staff_space.micrometres().0;
    let crowded: Vec<usize> = list.crowded.iter().map(|index| index + 1).collect();
    let dropped = dropped_entries(&list.dropped);
    let title_page = list.title_page;
    let count = list.pages.len();
    let systems = list.pages.iter().map(|page| page.systems.len()).sum();
    let bars = bar_count(&list);
    let targets: Vec<PathBuf> = (1..=count)
        .map(|number| {
            if into_directory {
                in_directory(
                    request.path,
                    input_format,
                    request.out,
                    request.format,
                    number,
                    count,
                )
            } else {
                numbered(request.out, number, count)
            }
        })
        .collect();
    let scale = request.scale.unwrap_or_else(|| Ratio::from_integer(1));
    let mut encoded = Vec::with_capacity(count);
    for page in &list.pages {
        encoded.push(match request.format {
            ImageFormat::Svg => to_svg(page).into_bytes(),
            ImageFormat::Png => to_png(page, scale).map_err(render_failure)?,
        });
    }
    for (target, bytes) in targets.iter().zip(&encoded) {
        files::write_bytes(target, bytes)?;
    }
    let written: Vec<String> = targets
        .iter()
        .map(|target| target.display().to_string())
        .collect();
    let mut lines = vec![format!(
        "rendered {count} {}",
        if count == 1 { "page" } else { "pages" }
    )];
    if staff_space != options.staff_space.0 {
        lines.push(format!(
            "staff space reduced from {} to {staff_space} micrometres to fit the page",
            options.staff_space.0
        ));
    }
    if !crowded.is_empty() {
        let numbers: Vec<String> = crowded.iter().map(ToString::to_string).collect();
        lines.push(format!(
            "crowded bars (notes may touch): {}",
            numbers.join(", ")
        ));
    }
    if !dropped.is_empty() {
        let marks: Vec<String> = dropped
            .iter()
            .map(|mark| format!("{} in bar {}", mark.kind, mark.bar))
            .collect();
        lines.push(format!(
            "marks left out for lack of room: {}",
            marks.join(", ")
        ));
    }
    if title_page {
        lines.push("the title block fills page 1; the music starts on page 2".to_string());
    }
    lines.extend(
        written
            .iter()
            .map(|file| format!("  {}", escape_controls(file))),
    );
    Ok(Rendered {
        wire: RenderedPages {
            pages: count,
            systems,
            bars,
            staff_space,
            crowded,
            dropped,
            title_page,
            files: written,
        },
        human: lines.join("\n"),
    })
}

fn layout_failure(error: LayoutError) -> CliError {
    match error {
        LayoutError::PageTooSmall | LayoutError::Rhythm { .. } => CliError::LayoutLimit {
            message: error.to_string(),
        },
        other => CliError::InvalidInput {
            message: other.to_string(),
        },
    }
}

fn render_failure(error: RenderError) -> CliError {
    CliError::InvalidInput {
        message: error.to_string(),
    }
}

fn require_extension(out: &Path, format: ImageFormat) -> Result<(), CliError> {
    let matches = out.file_stem().is_some_and(|stem| !stem.is_empty())
        && out
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case(format.extension()));
    if matches {
        Ok(())
    } else {
        Err(CliError::UnsupportedFormat {
            path: out.to_path_buf(),
            supported: format.suffix(),
        })
    }
}

fn numbered(out: &Path, number: usize, count: usize) -> PathBuf {
    if count == 1 {
        return out.to_path_buf();
    }
    let mut name = out.file_stem().map(OsString::from).unwrap_or_default();
    name.push(format!("-p{number}"));
    if let Some(extension) = out.extension() {
        name.push(".");
        name.push(extension);
    }
    out.with_file_name(name)
}

fn in_directory(
    input: &Path,
    input_format: Format,
    directory: &Path,
    format: ImageFormat,
    number: usize,
    count: usize,
) -> PathBuf {
    let mut name = input_stem(input, input_format);
    if count > 1 {
        name.push(format!("-p{number}"));
    }
    name.push(format.suffix());
    directory.join(name)
}

fn input_stem(input: &Path, format: Format) -> OsString {
    let stem = input.file_stem().map(Path::new);
    let stem = match format {
        Format::Gp => stem,
        Format::Json => stem.and_then(Path::file_stem).map(Path::new),
    };
    stem.map(|stem| stem.as_os_str().to_os_string())
        .unwrap_or_default()
}

pub fn parse_bars(text: &str) -> Result<BarSpan, String> {
    let invalid = || format!("invalid bars {text:?}: use A-B or A, counted from 1");
    let (first, last) = text.split_once('-').unwrap_or((text, text));
    let number = |part: &str| whole(part).filter(|value| *value > 0);
    let first = number(first).ok_or_else(invalid)?;
    let last = number(last).ok_or_else(invalid)?;
    if first > last {
        return Err(invalid());
    }
    Ok(BarSpan { first, last })
}

pub fn parse_scale(text: &str) -> Result<Ratio<u32>, String> {
    let invalid = || format!("invalid scale {text:?}: use a positive number such as 2, 1.5 or 3/2");
    let ratio = match text.split_once('/') {
        Some((numerator, denominator)) => {
            let numerator = whole(numerator).ok_or_else(invalid)?;
            let denominator = whole(denominator).ok_or_else(invalid)?;
            if denominator == 0 {
                return Err(invalid());
            }
            Ratio::new(numerator, denominator)
        }
        None => decimal(text).ok_or_else(invalid)?,
    };
    if *ratio.numer() == 0 {
        return Err("the scale must be greater than zero".to_string());
    }
    Ok(ratio)
}

fn whole(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > MAX_SCALE_DIGITS || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    text.parse().ok()
}

fn decimal(text: &str) -> Option<Ratio<u32>> {
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    if text.ends_with('.') || integer.len() + fraction.len() > MAX_SCALE_DIGITS {
        return None;
    }
    let numerator = whole(&format!("{integer}{fraction}"))?;
    let denominator = 10_u32.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    if integer.is_empty() {
        return None;
    }
    Some(Ratio::new(numerator, denominator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_parse_as_exact_ratios() {
        assert_eq!(parse_scale("1"), Ok(Ratio::from_integer(1)));
        assert_eq!(parse_scale("2"), Ok(Ratio::from_integer(2)));
        assert_eq!(parse_scale("1.5"), Ok(Ratio::new(3, 2)));
        assert_eq!(parse_scale("0.25"), Ok(Ratio::new(1, 4)));
        assert_eq!(parse_scale("3/2"), Ok(Ratio::new(3, 2)));
        assert_eq!(parse_scale("6/4"), Ok(Ratio::new(3, 2)));
        assert_eq!(parse_scale("2.50"), Ok(Ratio::new(5, 2)));
    }

    #[test]
    fn bar_spans_parse_from_one_or_two_numbers() {
        assert_eq!(parse_bars("3-7"), Ok(BarSpan { first: 3, last: 7 }));
        assert_eq!(parse_bars("4"), Ok(BarSpan { first: 4, last: 4 }));
        assert_eq!(parse_bars("2-2"), Ok(BarSpan { first: 2, last: 2 }));
        for text in [
            "", "0", "0-3", "5-4", "a", "1-", "-2", "1-2-3", " 1", "1.5", "+1",
        ] {
            assert!(parse_bars(text).is_err(), "{text}");
        }
    }

    #[test]
    fn bad_scales_are_refused() {
        for text in [
            "",
            "0",
            "0.0",
            "0/5",
            "-1",
            "+1",
            "1/0",
            "1.",
            ".5",
            "1.2.3",
            "a",
            "1e3",
            "1/",
            "/2",
            "1/2/3",
            " 1",
            "1234567890",
            "0.0000000001",
        ] {
            assert!(parse_scale(text).is_err(), "{text}");
        }
    }

    #[test]
    fn page_names_follow_the_out_path_or_the_input_stem() {
        let out = Path::new("dir/song.SVG");
        assert_eq!(numbered(out, 1, 1), PathBuf::from("dir/song.SVG"));
        assert_eq!(numbered(out, 2, 3), PathBuf::from("dir/song-p2.SVG"));
        let input = Path::new("in/My.Song.opentab.json");
        assert_eq!(
            in_directory(input, Format::Json, Path::new("o"), ImageFormat::Png, 1, 1),
            PathBuf::from("o/My.Song.png")
        );
        assert_eq!(
            in_directory(
                Path::new("a.b.GP"),
                Format::Gp,
                Path::new("o"),
                ImageFormat::Svg,
                3,
                4
            ),
            PathBuf::from("o/a.b-p3.svg")
        );
    }
}
