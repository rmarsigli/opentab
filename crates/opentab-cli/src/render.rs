use std::ffi::OsString;
use std::path::{Path, PathBuf};

use opentab_layout::placeholder_layout;
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

#[derive(Serialize)]
pub struct RenderedPages {
    pages: usize,
    files: Vec<String>,
}

pub struct RenderRequest<'a> {
    pub path: &'a Path,
    pub format: ImageFormat,
    pub out: &'a Path,
    pub scale: Option<Ratio<u32>>,
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
    let list = placeholder_layout(&score);
    let count = list.pages.len();
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
    lines.extend(
        written
            .iter()
            .map(|file| format!("  {}", escape_controls(file))),
    );
    Ok(Rendered {
        wire: RenderedPages {
            pages: count,
            files: written,
        },
        human: lines.join("\n"),
    })
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
