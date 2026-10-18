use std::path::{Path, PathBuf};

use serde_json::json;

use crate::support::{file_names, new_score, path_str, read_json, run, temp};

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

fn long_score(dir: &Path, bars: usize) -> PathBuf {
    let path = new_score(dir, "long.opentab.json");
    let mut score = read_json(&path);
    let master = score["master_bars"][0].clone();
    let bar = score["tracks"][0]["staves"][0]["bars"][0].clone();
    let mut next = score["ids"]["next"].as_u64().unwrap();
    let mut take = || {
        next += 1;
        next - 1
    };
    let mut masters = Vec::new();
    let mut staff_bars = Vec::new();
    for _ in 0..bars {
        let mut master = master.clone();
        master["id"] = json!(take());
        master["tempo_changes"] = json!([]);
        masters.push(master);
        let mut bar = bar.clone();
        bar["id"] = json!(take());
        bar["voices"][0]["id"] = json!(take());
        bar["voices"][0]["beats"][0]["id"] = json!(take());
        staff_bars.push(bar);
    }
    score["master_bars"] = json!(masters);
    score["tracks"][0]["staves"][0]["bars"] = json!(staff_bars);
    score["ids"]["next"] = json!(take());
    let path = dir.join("long-score.opentab.json");
    std::fs::write(&path, serde_json::to_string(&score).unwrap()).unwrap();
    path
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
        json!({"pages": 1, "systems": 1, "bars": 2, "staff_space": 1750, "crowded": [], "files": [path_str(&out)]})
    );
    assert_eq!(file_names(dir.path()), vec!["song.svg"]);
}

#[test]
fn several_pages_are_numbered_after_the_out_stem() {
    let scores = temp();
    let input = long_score(scores.path(), 200);
    let dir = temp();
    let out = dir.path().join("long.SVG");
    let result = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let wire = result.json();
    let pages = wire["pages"].as_u64().unwrap();
    assert!(pages >= 2, "{wire}");
    assert_eq!(wire["bars"], 200);
    assert!(wire["systems"].as_u64().unwrap() > pages);
    let expected: Vec<String> = (1..=pages)
        .map(|page| format!("long-p{page}.SVG"))
        .collect();
    assert_eq!(file_names(dir.path()), expected);
    let files: Vec<String> = expected
        .iter()
        .map(|name| path_str(&dir.path().join(name)).to_string())
        .collect();
    assert_eq!(wire["files"], json!(files));
    let second_page = std::fs::read_to_string(dir.path().join("long-p2.SVG")).unwrap();
    assert!(second_page.contains("<g data-id="), "{second_page}");
}

#[test]
fn a_directory_out_names_pages_after_the_input() {
    let dir = temp();
    let scores = temp();
    let input = long_score(scores.path(), 120);
    let result = run(&[
        "render",
        path_str(&input),
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
        vec!["long-score-p1.png", "long-score-p2.png"]
    );
    assert_eq!(png_size(&dir.path().join("long-score-p1.png")), (794, 1123));

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
        json!({"pages": 1, "systems": 1, "bars": 1, "staff_space": 1750, "crowded": [], "files": [path_str(&expected)]})
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
    let scores = temp();
    let input = long_score(scores.path(), 120);
    let render = |name: &str, format: &str| {
        let out = dir.path().join(name);
        let result = run(&[
            "render",
            path_str(&input),
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
        "--track",
        "--bars",
        "--staff",
        "standard",
        "-p<N>",
        "96 pixels per inch",
        "\"files\"",
        "\"systems\"",
        "\"staff_space\"",
        "\"crowded\"",
        "0.9 mm",
        "layout_limit",
        "Examples:",
    ] {
        assert!(help.stdout.contains(needle), "{needle}\n{}", help.stdout);
    }
    assert!(run(&["--help"]).stdout.contains("render"));
}

fn render_json(input: &Path, extra: &[&str]) -> (crate::support::Run, tempfile::TempDir) {
    let dir = temp();
    let out = dir.path().join("page.svg");
    let mut args = vec![
        "render",
        path_str(input),
        "--format",
        "svg",
        "--out",
        path_str(&out),
    ];
    args.extend(extra.iter().copied());
    args.push("--json");
    (run(&args), dir)
}

#[test]
fn track_bar_and_staff_options_select_what_is_drawn() {
    let input = fixture("multi-track.gp");
    let (all, _keep) = render_json(&input, &[]);
    assert_eq!(all.code, 0, "{}", all.stdout);
    let (one, dir) = render_json(&input, &["--track", "3", "--bars", "2", "--staff", "tab"]);
    assert_eq!(one.code, 0, "{}", one.stdout);
    assert_eq!(one.json()["bars"], 1);
    assert_eq!(one.json()["systems"], 1);
    let svg = std::fs::read_to_string(dir.path().join("page.svg")).unwrap();
    let full = std::fs::read_to_string(_keep.path().join("page.svg")).unwrap();
    assert!(svg.len() < full.len() / 4, "{} {}", svg.len(), full.len());
    let lines = svg.matches("<line ").count();
    assert!(lines >= 4, "{lines}");
    let (two, _dir) = render_json(&input, &["--track", "1", "--track", "2", "--bars", "1-2"]);
    assert_eq!(two.code, 0, "{}", two.stdout);
    assert_eq!(two.json()["bars"], 2);
}

#[test]
fn out_of_range_tracks_and_bars_are_refused() {
    let input = fixture("multi-track.gp");
    for (extra, code, kind) in [
        (vec!["--track", "5"], 2, "invalid_input"),
        (vec!["--track", "0"], 1, "usage"),
        (vec!["--bars", "3"], 2, "invalid_input"),
        (vec!["--bars", "2-9"], 2, "invalid_input"),
        (vec!["--bars", "2-1"], 1, "usage"),
        (vec!["--bars", "x"], 1, "usage"),
        (vec!["--staff", "piano"], 1, "usage"),
    ] {
        let (result, dir) = render_json(&input, &extra);
        result.assert_no_panic();
        assert_eq!(result.code, code, "{extra:?}: {}", result.stdout);
        assert_eq!(result.json()["error"]["kind"], kind, "{extra:?}");
        assert!(file_names(dir.path()).is_empty(), "{extra:?}");
    }
    let (track, _dir) = render_json(&input, &["--track", "5"]);
    let message = track.json()["error"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(message.contains("4 track(s)"), "{message}");
}

fn tall_score(dir: &Path, tracks: usize) -> PathBuf {
    let path = new_score(dir, &format!("blank-{tracks}.opentab.json"));
    let mut score = read_json(&path);
    let track = score["tracks"][0].clone();
    let mut next = score["ids"]["next"].as_u64().unwrap();
    let mut all = Vec::new();
    for _ in 0..tracks {
        let mut copy = track.clone();
        for pointer in [
            "/id",
            "/staves/0/id",
            "/staves/0/bars/0/id",
            "/staves/0/bars/0/voices/0/id",
            "/staves/0/bars/0/voices/0/beats/0/id",
        ] {
            *copy.pointer_mut(pointer).unwrap() = json!(next);
            next += 1;
        }
        all.push(copy);
    }
    score["tracks"] = json!(all);
    score["ids"]["next"] = json!(next);
    let path = dir.join(format!("tall-{tracks}.opentab.json"));
    std::fs::write(&path, serde_json::to_string(&score).unwrap()).unwrap();
    path
}

#[test]
fn a_shrunk_staff_space_is_reported_and_a_hopeless_one_is_a_layout_limit() {
    let input = fixture("tunings.gp");
    let (shrunk, _dir) = render_json(&input, &[]);
    assert_eq!(shrunk.code, 0, "{}", shrunk.stdout);
    let used = shrunk.json()["staff_space"].as_i64().unwrap();
    assert!((900..1750).contains(&used), "{used}");
    let dir = temp();
    let human = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--out",
        path_str(&dir.path().join("t.svg")),
    ]);
    assert_eq!(human.code, 0);
    assert!(
        human.stdout.contains(&format!(
            "staff space reduced from 1750 to {used} micrometres"
        )),
        "{}",
        human.stdout
    );
    let scores = temp();
    let tall = tall_score(scores.path(), 14);
    let (refused, out) = render_json(&tall, &[]);
    refused.assert_no_panic();
    assert_eq!(refused.code, 2, "{}", refused.stdout);
    assert_eq!(refused.json()["error"]["kind"], "layout_limit");
    assert!(file_names(out.path()).is_empty());
    let (fits, _out) = render_json(&tall_score(scores.path(), 8), &[]);
    assert_eq!(fits.code, 0, "{}", fits.stdout);
    assert_eq!(fits.json()["crowded"], json!([]));
}

#[test]
fn a_bar_too_dense_for_any_line_is_reported_as_crowded() {
    let dir = temp();
    let path = new_score(dir.path(), "dense.opentab.json");
    let mut score = read_json(&path);
    let beat = score["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"][0].clone();
    let mut next = score["ids"]["next"].as_u64().unwrap();
    let beats: Vec<serde_json::Value> = (0..512)
        .map(|_| {
            let mut copy = beat.clone();
            copy["id"] = json!(next);
            copy["duration"]["value"] = json!("one_twenty_eighth");
            next += 1;
            copy
        })
        .collect();
    score["master_bars"][0]["time_signature"] = json!({"numerator": 16, "denominator": 4});
    score["tracks"][0]["staves"][0]["bars"][0]["voices"][0]["beats"] = json!(beats);
    score["ids"]["next"] = json!(next);
    let input = dir.path().join("dense-bar.opentab.json");
    std::fs::write(&input, serde_json::to_string(&score).unwrap()).unwrap();
    let (result, _out) = render_json(&input, &[]);
    assert_eq!(result.code, 0, "{}", result.stdout);
    assert_eq!(result.json()["crowded"], json!([1]));
    assert_eq!(result.json()["staff_space"], 900);
    let out = temp();
    let human = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--out",
        path_str(&out.path().join("d.svg")),
    ]);
    assert!(
        human.stdout.contains("crowded bars (notes may touch): 1"),
        "{}",
        human.stdout
    );
}
