use std::path::{Path, PathBuf};

use serde_json::json;

use crate::support::{file_names, new_score, path_str, run, temp};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gp")
        .join(name)
}

fn png_size(path: &Path) -> (u32, u32) {
    let bytes = std::fs::read(path).expect("read png");
    assert!(bytes.starts_with(PNG_SIGNATURE), "{}", path.display());
    let word = |offset: usize| {
        u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("four bytes"))
    };
    (word(16), word(20))
}

#[test]
fn an_svg_of_a_gp_fixture_matches_the_snapshot() {
    let dir = temp();
    let out = dir.path().join("multi-track.svg");
    let result = run(&[
        "render",
        path_str(&fixture("multi-track.gp")),
        "--format",
        "svg",
        "--out",
        path_str(&out),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let svg = std::fs::read_to_string(&out).expect("read svg");
    insta::assert_snapshot!(svg);
}

#[test]
fn json_reports_the_pages_and_the_written_files() {
    let dir = temp();
    let out = dir.path().join("song.svg");
    let result = run(&[
        "render",
        path_str(&fixture("multi-track.gp")),
        "--format",
        "svg",
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(
        result.json(),
        json!({"pages": 1, "files": [path_str(&out)]})
    );
    assert_eq!(file_names(dir.path()), vec!["song.svg"]);
}

#[test]
fn several_pages_are_numbered_after_the_out_stem() {
    let dir = temp();
    let out = dir.path().join("tunings.SVG");
    let result = run(&[
        "render",
        path_str(&fixture("tunings.gp")),
        "--format",
        "svg",
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let first = dir.path().join("tunings-p1.SVG");
    let second = dir.path().join("tunings-p2.SVG");
    assert_eq!(
        result.json(),
        json!({"pages": 2, "files": [path_str(&first), path_str(&second)]})
    );
    assert_eq!(
        file_names(dir.path()),
        vec!["tunings-p1.SVG", "tunings-p2.SVG"]
    );
    let second_page = std::fs::read_to_string(&second).unwrap();
    assert!(second_page.contains("<g data-id="), "{second_page}");
}

#[test]
fn a_directory_out_names_pages_after_the_input() {
    let dir = temp();
    let result = run(&[
        "render",
        path_str(&fixture("tunings.gp")),
        "--format",
        "png",
        "--out",
        path_str(dir.path()),
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(
        result.stdout.starts_with("rendered 2 pages\n"),
        "{}",
        result.stdout
    );
    assert_eq!(
        file_names(dir.path()),
        vec!["tunings-p1.png", "tunings-p2.png"]
    );
    assert_eq!(png_size(&dir.path().join("tunings-p1.png")), (794, 1123));

    let scores = temp();
    let score = new_score(scores.path(), "My.Song.opentab.json");
    let single = run(&[
        "render",
        path_str(&score),
        "--format",
        "svg",
        "--out",
        path_str(dir.path()),
        "--json",
    ]);
    assert_eq!(single.code, 0, "{}", single.stderr);
    let expected = dir.path().join("My.Song.svg");
    assert_eq!(
        single.json(),
        json!({"pages": 1, "files": [path_str(&expected)]})
    );
}

#[test]
fn the_png_scale_sets_the_pixel_size() {
    let dir = temp();
    for (scale, size) in [
        ("1", (794, 1123)),
        ("3/2", (1191, 1684)),
        ("2", (1587, 2245)),
        ("1.5", (1191, 1684)),
    ] {
        let out = dir.path().join("page.png");
        let result = run(&[
            "render",
            path_str(&fixture("multi-track.gp")),
            "--format",
            "png",
            "--scale",
            scale,
            "--out",
            path_str(&out),
        ]);
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert_eq!(png_size(&out), size, "{scale}");
    }
}

#[test]
fn rendering_twice_writes_identical_bytes() {
    let dir = temp();
    let render = |name: &str, format: &str| {
        let out = dir.path().join(name);
        let result = run(&[
            "render",
            path_str(&fixture("tunings.gp")),
            "--format",
            format,
            "--out",
            path_str(&out),
        ]);
        assert_eq!(result.code, 0, "{}", result.stderr);
    };
    render("a.svg", "svg");
    render("b.svg", "svg");
    render("a.png", "png");
    render("b.png", "png");
    for (left, right) in [
        ("a-p1.svg", "b-p1.svg"),
        ("a-p2.svg", "b-p2.svg"),
        ("a-p1.png", "b-p1.png"),
        ("a-p2.png", "b-p2.png"),
    ] {
        assert_eq!(
            std::fs::read(dir.path().join(left)).unwrap(),
            std::fs::read(dir.path().join(right)).unwrap(),
            "{left}"
        );
    }
}

#[test]
fn bad_requests_fail_without_writing() {
    let dir = temp();
    let input = fixture("multi-track.gp");
    let input = path_str(&input);
    let wrong_suffix = dir.path().join("page.png");
    let cases: [(Vec<&str>, i32, &str); 6] = [
        (
            vec!["--format", "svg", "--out", path_str(&wrong_suffix)],
            2,
            "unsupported_format",
        ),
        (
            vec![
                "--format",
                "svg",
                "--scale",
                "2",
                "--out",
                path_str(&wrong_suffix),
            ],
            2,
            "invalid_input",
        ),
        (
            vec![
                "--format",
                "png",
                "--scale",
                "0",
                "--out",
                path_str(&wrong_suffix),
            ],
            1,
            "usage",
        ),
        (
            vec![
                "--format",
                "png",
                "--scale",
                "-1",
                "--out",
                path_str(&wrong_suffix),
            ],
            1,
            "usage",
        ),
        (
            vec![
                "--format",
                "png",
                "--scale",
                "100",
                "--out",
                path_str(&wrong_suffix),
            ],
            2,
            "invalid_input",
        ),
        (
            vec!["--format", "pdf", "--out", path_str(&wrong_suffix)],
            1,
            "usage",
        ),
    ];
    for (extra, code, kind) in cases {
        let mut args = vec!["render", input];
        args.extend(extra.iter().copied());
        args.push("--json");
        let result = run(&args);
        result.assert_no_panic();
        assert_eq!(result.code, code, "{extra:?}: {}", result.stdout);
        assert_eq!(result.json()["error"]["kind"], kind, "{extra:?}");
        assert!(file_names(dir.path()).is_empty(), "{extra:?}");
    }
    let missing_format = run(&["render", input, "--out", path_str(&wrong_suffix)]);
    assert_eq!(missing_format.code, 1);
    let unsupported_input = run(&[
        "render",
        "song.txt",
        "--format",
        "svg",
        "--out",
        path_str(&dir.path().join("x.svg")),
        "--json",
    ]);
    assert_eq!(unsupported_input.code, 2);
    assert_eq!(
        unsupported_input.json()["error"]["kind"],
        "unsupported_format"
    );
    assert!(file_names(dir.path()).is_empty());
}

#[test]
fn render_help_documents_the_options_and_the_json_shape() {
    let help = run(&["render", "--help"]);
    assert_eq!(help.code, 0);
    for needle in [
        "--format",
        "--out",
        "--scale",
        "--json",
        "-p<N>",
        "96 pixels per inch",
        "\"files\"",
        "Examples:",
    ] {
        assert!(help.stdout.contains(needle), "{needle}\n{}", help.stdout);
    }
    assert!(run(&["--help"]).stdout.contains("render"));
}
