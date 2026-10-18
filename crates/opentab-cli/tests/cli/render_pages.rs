use serde_json::json;

use crate::render::{fixture, long_score, png_size};
use crate::support::{file_names, new_score, path_str, read_json, run, temp};

#[test]
fn page_renders_one_page_of_the_layout_into_out() {
    let scores = temp();
    let input = long_score(scores.path(), 200);
    let full = temp();
    let all = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--out",
        path_str(&full.path().join("all.svg")),
        "--json",
    ]);
    assert_eq!(all.code, 0, "{}", all.stdout);
    let pages = all.json()["pages"].as_u64().unwrap();
    assert!(pages >= 3, "{}", all.stdout);
    let dir = temp();
    let out = dir.path().join("second.svg");
    let second = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--page",
        "2",
        "--out",
        path_str(&out),
        "--json",
    ]);
    assert_eq!(second.code, 0, "{}", second.stdout);
    let wire = second.json();
    assert_eq!(wire["pages"], json!(pages));
    assert_eq!(wire["systems"], all.json()["systems"]);
    assert_eq!(wire["files"], json!([path_str(&out)]));
    assert_eq!(file_names(dir.path()), vec!["second.svg"]);
    assert_eq!(
        std::fs::read(&out).unwrap(),
        std::fs::read(full.path().join("all-p2.svg")).unwrap()
    );
    let last = pages.to_string();
    let into = temp();
    let human = run(&[
        "render",
        path_str(&input),
        "--format",
        "png",
        "--page",
        &last,
        "--out",
        path_str(into.path()),
    ]);
    assert_eq!(human.code, 0, "{}", human.stderr);
    assert!(
        human
            .stdout
            .starts_with(&format!("rendered page {pages} of {pages}\n")),
        "{}",
        human.stdout
    );
    let name = format!("long-score-p{pages}.png");
    assert_eq!(file_names(into.path()), vec![name.clone()]);
    assert_eq!(png_size(&into.path().join(name)), (794, 1123));
}

#[test]
fn pages_outside_the_layout_are_refused_without_writing() {
    let input = fixture("multi-track.gp");
    for (page, code, kind) in [
        ("2", 2, "invalid_input"),
        ("4294967295", 2, "invalid_input"),
        ("0", 1, "usage"),
        ("-1", 1, "usage"),
        ("x", 1, "usage"),
    ] {
        let dir = temp();
        let result = run(&[
            "render",
            path_str(&input),
            "--format",
            "svg",
            "--page",
            page,
            "--out",
            path_str(&dir.path().join("p.svg")),
            "--json",
        ]);
        result.assert_no_panic();
        assert_eq!(result.code, code, "{page}: {}", result.stdout);
        assert_eq!(result.json()["error"]["kind"], kind, "{page}");
        assert!(file_names(dir.path()).is_empty(), "{page}");
        if code == 2 {
            let message = result.json()["error"]["message"].to_string();
            assert!(message.contains("1 page(s)"), "{message}");
        }
    }
}

#[test]
fn a_missing_out_directory_is_named_in_an_io_error() {
    let dir = temp();
    let input = fixture("multi-track.gp");
    let missing = dir.path().join("missing");
    let slashed = format!("{}/", path_str(&missing));
    let nested = missing.join("page.svg");
    for out in [path_str(&missing), slashed.as_str(), path_str(&nested)] {
        let result = run(&[
            "render",
            path_str(&input),
            "--format",
            "svg",
            "--out",
            out,
            "--json",
        ]);
        result.assert_no_panic();
        assert_eq!(result.code, 2, "{out}: {}", result.stdout);
        let error = &result.json()["error"];
        assert_eq!(error["kind"], "io", "{out}");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains("no such directory"), "{message}");
        assert!(message.contains(path_str(&missing)), "{message}");
        assert!(file_names(dir.path()).is_empty());
    }
    let wrong = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--out",
        path_str(&dir.path().join("page.png")),
        "--json",
    ]);
    assert_eq!(wrong.json()["error"]["kind"], "unsupported_format");
}

#[test]
fn a_png_over_the_pixel_cap_is_refused_with_the_limit() {
    let dir = temp();
    let input = fixture("multi-track.gp");
    let out = dir.path().join("big.png");
    let result = run(&[
        "render",
        path_str(&input),
        "--format",
        "png",
        "--scale",
        "9",
        "--out",
        path_str(&out),
        "--json",
    ]);
    result.assert_no_panic();
    assert_eq!(result.code, 2, "{}", result.stdout);
    let error = &result.json()["error"];
    assert_eq!(error["kind"], "invalid_input");
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("60000000") && message.contains("lower the scale"),
        "{message}"
    );
    assert!(file_names(dir.path()).is_empty());
    let fits = run(&[
        "render",
        path_str(&input),
        "--format",
        "png",
        "--scale",
        "8",
        "--out",
        path_str(&out),
    ]);
    assert_eq!(fits.code, 0, "{}", fits.stderr);
    assert_eq!(png_size(&out), (6350, 8980));
}

#[test]
fn unrendered_features_of_the_drawn_bars_are_listed_in_json_and_text() {
    let dir = temp();
    let path = new_score(dir.path(), "held.opentab.json");
    let mut score = read_json(&path);
    let master = score["master_bars"][0]["id"].as_u64().unwrap().to_string();
    let track = score["tracks"][0]["id"].as_u64().unwrap().to_string();
    score["extensions"][master] = json!([{
        "format": "gpif",
        "data": "<Keep parent=\"\" index=\"2\"><Fermatas><Fermata><Type>Long</Type></Fermata></Fermatas></Keep>"
    }]);
    score["extensions"][track] = json!([{
        "format": "gpif",
        "data": "<Keep parent=\"\" index=\"18\"><Lyrics><Line><Text><![CDATA[la]]></Text><Offset>0</Offset></Line></Lyrics></Keep>"
    }]);
    let input = dir.path().join("held-bar.opentab.json");
    std::fs::write(&input, serde_json::to_string(&score).unwrap()).unwrap();
    let out = temp();
    let wire = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--out",
        path_str(&out.path().join("a.svg")),
        "--json",
    ]);
    assert_eq!(wire.code, 0, "{}", wire.stdout);
    assert_eq!(wire.json()["unrendered"], json!(["fermatas", "lyrics"]));
    let human = run(&[
        "render",
        path_str(&input),
        "--format",
        "svg",
        "--out",
        path_str(&out.path().join("b.svg")),
    ]);
    assert!(
        human
            .stdout
            .contains("not drawn yet (outside the renderer's scope): fermatas, lyrics"),
        "{}",
        human.stdout
    );
}
