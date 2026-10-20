use std::ffi::OsString;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use opentab_layout::{LayoutError, LayoutOptions, StaffMode, layout};
use opentab_model::Score;
use opentab_render::{
    BarSpan, Ratio, RenderError, ScoreSelection, SelectionError, Summary, layout_options,
    page_number, to_png, to_svg,
};
use serde::Serialize;

use crate::error::CliError;
use crate::output::Rendered;
use crate::text::escape_controls;
use opentab_files::{self as files, Format};

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

#[derive(Serialize)]
pub struct RenderedPages {
    #[serde(flatten)]
    summary: Summary,
    files: Vec<String>,
}

pub struct RenderRequest<'a> {
    pub path: &'a Path,
    pub format: ImageFormat,
    pub out: &'a Path,
    pub scale: Option<Ratio<u32>>,
    pub tracks: Vec<u32>,
    pub bars: Option<BarSpan>,
    pub staff: Option<StaffChoice>,
    pub page: Option<u32>,
}

fn options(request: &RenderRequest<'_>, score: &Score) -> Result<LayoutOptions, CliError> {
    let selection = ScoreSelection {
        tracks: request.tracks.clone(),
        bars: request.bars,
        staff: request.staff.map(StaffChoice::mode),
    };
    layout_options(score, &selection).map_err(selection_failure)
}

pub fn selection_failure(error: SelectionError) -> CliError {
    CliError::InvalidInput {
        message: format!("--{error}"),
    }
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
        check_out_file(request.out, request.format)?;
    }
    let score = files::read_score(request.path)?;
    let options = options(request, &score)?;
    let list = layout(&score, &options).map_err(layout_failure)?;
    let summary = Summary::of(&list);
    let count = summary.pages;
    let staff_space = summary.staff_space;
    let chosen: Vec<usize> = match request.page {
        Some(number) => vec![page_number(number, count).map_err(selection_failure)?],
        None => (1..=count).collect(),
    };
    let targets: Vec<PathBuf> = chosen
        .iter()
        .map(|number| {
            if into_directory {
                in_directory(
                    request.path,
                    input_format,
                    request.out,
                    request.format,
                    *number,
                    count,
                )
            } else if request.page.is_some() {
                request.out.to_path_buf()
            } else {
                numbered(request.out, *number, count)
            }
        })
        .collect();
    let scale = request.scale.unwrap_or_else(|| Ratio::from_integer(1));
    let mut encoded = Vec::with_capacity(chosen.len());
    for number in &chosen {
        let page = &list.pages[number - 1];
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
    let mut lines = vec![match request.page {
        Some(number) => format!("rendered page {number} of {count}"),
        None => format!(
            "rendered {count} {}",
            if count == 1 { "page" } else { "pages" }
        ),
    }];
    if staff_space != options.staff_space.0 {
        lines.push(format!(
            "staff space reduced from {} to {staff_space} micrometres to fit the page",
            options.staff_space.0
        ));
    }
    if !summary.crowded.is_empty() {
        let numbers: Vec<String> = summary.crowded.iter().map(ToString::to_string).collect();
        lines.push(format!(
            "crowded bars (notes may touch): {}",
            numbers.join(", ")
        ));
    }
    if !summary.dropped.is_empty() {
        let marks: Vec<String> = summary
            .dropped
            .iter()
            .map(|mark| match mark.bar {
                Some(bar) => format!("{} in bar {bar}", mark.kind),
                None => format!("{} block", mark.kind),
            })
            .collect();
        lines.push(format!(
            "marks left out for lack of room: {}",
            marks.join(", ")
        ));
    }
    if summary.title_page {
        lines.push("the title block fills page 1; the music starts on page 2".to_string());
    }
    if !summary.unrendered.is_empty() {
        lines.push(format!(
            "not drawn yet (outside the renderer's scope): {}",
            summary.unrendered.join(", ")
        ));
    }
    lines.extend(
        written
            .iter()
            .map(|file| format!("  {}", escape_controls(file))),
    );
    Ok(Rendered {
        wire: RenderedPages {
            summary,
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

fn names_a_directory(out: &Path) -> bool {
    let text = out.as_os_str().to_string_lossy();
    out.extension().is_none() || text.ends_with('/') || text.ends_with(MAIN_SEPARATOR)
}

fn stemless(out: &Path, format: ImageFormat) -> bool {
    out.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(format.suffix()))
}

fn check_out_file(out: &Path, format: ImageFormat) -> Result<(), CliError> {
    if stemless(out, format) {
        return Err(CliError::InvalidInput {
            message: format!(
                "{}: the file name needs a name before {}",
                out.display(),
                format.suffix()
            ),
        });
    }
    if out.exists() && !out.is_dir() && names_a_directory(out) {
        return Err(CliError::Io {
            path: out.to_path_buf(),
            message: format!(
                "exists and is not a directory; give a directory, or a file name ending in {}",
                format.suffix()
            ),
        });
    }
    if names_a_directory(out) {
        return Err(CliError::Io {
            path: out.to_path_buf(),
            message: format!(
                "no such directory; create it first, or give a file name ending in {}",
                format.suffix()
            ),
        });
    }
    require_extension(out, format)?;
    match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.is_dir() => Err(CliError::Io {
            path: parent.to_path_buf(),
            message: "no such directory; create it first".to_string(),
        }),
        _ => Ok(()),
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

#[cfg(test)]
mod tests {
    use super::*;

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
