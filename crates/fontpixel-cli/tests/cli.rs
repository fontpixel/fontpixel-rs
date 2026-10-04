//! CLI dispatch, actual BDF behavior and safe output persistence.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fontpixel"))
        .args(args)
        .output()
        .unwrap()
}
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "fontpixel-cli-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn formats_lists_all_46_and_marks_actual_codecs() {
    let output = run(&["formats"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 47);
    assert!(lines[0].starts_with("ID\tREAD\tWRITE"));
    assert!(
        lines[1..]
            .iter()
            .filter(|line| !line.starts_with("bdf\t")
                && !line.starts_with("pcf\t")
                && !line.starts_with("yaff\t")
                && !line.starts_with("hex\t")
                && !line.starts_with("psf\t")
                && !line.starts_with("fnt\t")
                && !line.starts_with("fon\t")
                && !line.starts_with("otb\t")
                && !line.starts_with("bdat\t")
                && !line.starts_with("color\t")
                && !line.starts_with("svg\t")
                && !["svg", "ttf", "otf", "woff", "woff2", "ttc", "vector-raster"]
                    .iter()
                    .any(|id| line.starts_with(&format!("{id}\t"))))
            .all(|line| line.contains("\tnot implemented\tnot implemented\t"))
    );
    for id in ["bdf", "pcf", "hbf", "pk", "gf", "vector-raster", "drcs"] {
        assert!(
            lines[1..]
                .iter()
                .any(|line| line.starts_with(&format!("{id}\t")))
        );
    }
}

#[test]
fn formats_json_is_parseable_and_honest() {
    let output = run(&["formats", "--json"]);
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let formats = json["formats"].as_array().unwrap();
    assert_eq!(formats.len(), 46);
    assert!(
        formats
            .iter()
            .filter(|entry| entry["format"]["id"] != "bdf"
                && entry["format"]["id"] != "pcf"
                && entry["format"]["id"] != "yaff"
                && entry["format"]["id"] != "hex"
                && entry["format"]["id"] != "psf"
                && entry["format"]["id"] != "fnt"
                && entry["format"]["id"] != "fon"
                && entry["format"]["id"] != "otb"
                && entry["format"]["id"] != "bdat"
                && entry["format"]["id"] != "color"
                && !["svg", "ttf", "otf", "woff", "woff2", "ttc", "vector-raster"]
                    .iter()
                    .any(|id| entry["format"]["id"] == *id))
            .all(|entry| entry["read"] == false
                && entry["write"] == false
                && entry["detect"] == false)
    );
    for id in ["svg", "ttf", "otf", "woff", "woff2", "ttc", "vector-raster"] {
        let entry = formats
            .iter()
            .find(|entry| entry["format"]["id"] == id)
            .unwrap();
        let enabled = entry["format"]["feature_enabled"].as_bool().unwrap();
        assert_eq!(entry["read"], enabled);
        assert_eq!(entry["write"], enabled);
        assert_eq!(entry["detect"], enabled && id != "vector-raster");
    }
    for id in ["otb", "bdat", "color"] {
        let entry = formats
            .iter()
            .find(|entry| entry["format"]["id"] == id)
            .unwrap();
        let enabled = entry["format"]["feature_enabled"].as_bool().unwrap();
        assert_eq!(entry["read"], enabled);
        assert_eq!(entry["detect"], enabled);
        assert_eq!(entry["write"], enabled && id != "color");
    }
}

#[test]
fn help_and_version_succeed_but_invalid_usage_returns_two() {
    for args in [
        vec!["--help"],
        vec!["--version"],
        vec!["info", "--help"],
        vec!["convert", "--help"],
    ] {
        assert!(run(&args).status.success());
    }
    for args in [
        vec![],
        vec!["missing-command"],
        vec!["info"],
        vec!["convert", "in.bdf"],
        vec!["convert", "in.bdf", "-o", "out.pcf", "--to", "unknown"],
    ] {
        assert_eq!(run(&args).status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn info_does_not_claim_success_for_truncated_bdf() {
    let dir = TempDir::new();
    let input = dir.0.join("input.bdf");
    fs::write(&input, b"STARTFONT 2.1\n").unwrap();
    let output = run(&["info", input.to_str().unwrap(), "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Unexpected end of file before the end of the headers")
    );
}

#[test]
fn info_reports_missing_inputs_and_ambiguous_extensions() {
    let dir = TempDir::new();
    let missing = dir.0.join("missing.bdf");
    assert_eq!(
        run(&["info", missing.to_str().unwrap()]).status.code(),
        Some(1)
    );
    let input = dir.0.join("ambiguous.fnt");
    fs::write(&input, b"synthetic unknown bytes").unwrap();
    let output = run(&["info", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("ambiguous format"));
    let explicit = run(&["info", input.to_str().unwrap(), "--from", "snf"]);
    assert!(
        String::from_utf8_lossy(&explicit.stderr)
            .contains("read for format 'snf' is not implemented")
    );
}

#[test]
fn failed_conversion_never_creates_or_overwrites_output() {
    let dir = TempDir::new();
    let input = dir.0.join("input.bdf");
    let target = dir.0.join("out.yaff");
    fs::write(&input, b"STARTFONT 2.1\n").unwrap();
    for existing in [false, true] {
        if existing {
            fs::write(&target, b"KEEP EXISTING BYTES").unwrap();
        }
        let output = run(&[
            "convert",
            input.to_str().unwrap(),
            "-o",
            target.to_str().unwrap(),
            "--strict",
            "--report",
            "json",
        ]);
        assert_eq!(output.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Unexpected end of file before the end of the headers")
        );
        if existing {
            assert_eq!(fs::read(&target).unwrap(), b"KEEP EXISTING BYTES");
        } else {
            assert!(!target.exists());
        }
    }
}

#[test]
fn convert_dispatch_accepts_multiple_inputs_and_explicit_target() {
    let dir = TempDir::new();
    let target = dir.0.join("output.unusual");
    let output = run(&[
        "convert",
        "first.bdf",
        "second.bdf",
        "-o",
        target.to_str().unwrap(),
        "--from",
        "bdf",
        "--to",
        "snf",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("write for format 'snf' is not implemented")
    );
    assert!(!target.exists());
}

#[test]
fn ambiguous_target_is_not_selected_by_catalogue_order() {
    let output = run(&["convert", "input.bdf", "-o", "out.fnt"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("ambiguous format"));
}

const BDF: &str = "STARTFONT 2.1\nFONT test\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 8 0 0\nCHARS 1\nSTARTCHAR A\nENCODING 65\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n";
#[test]
fn bdf_info_and_convert_are_real_and_output_is_stable() {
    let dir = TempDir::new();
    let input = dir.0.join("input.unusual");
    let target = dir.0.join("out.bdf");
    fs::write(&input, BDF).unwrap();
    let info = run(&["info", input.to_str().unwrap(), "--json"]);
    assert!(
        info.status.success(),
        "{}",
        String::from_utf8_lossy(&info.stderr)
    );
    let j: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(j["format"], "bdf");
    assert_eq!(j["fonts"][0]["glyph_count"], 1);
    fs::write(&target, b"old output").unwrap();
    let converted = run(&[
        "convert",
        input.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
        "--strict",
        "--report",
        "json",
    ]);
    assert!(
        converted.status.success(),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    assert_eq!(fs::read(&target).unwrap(), BDF.as_bytes());
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
    let compressed = dir.0.join("out.bdf.gz");
    assert!(
        run(&[
            "convert",
            input.to_str().unwrap(),
            "-o",
            compressed.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert!(fs::read(&compressed).unwrap().starts_with(&[0x1f, 0x8b]));
    assert!(
        run(&["info", compressed.to_str().unwrap()])
            .status
            .success()
    );
}
#[test]
fn malformed_loss_multi_font_and_filesystem_errors_leave_outputs_untouched() {
    let dir = TempDir::new();
    let input = dir.0.join("input.bdf");
    let target = dir.0.join("out.bdf");
    for (source, code) in [
        (BDF.replace("ENCODING 65", "ENCODING malformed"), 1),
        (BDF.replace("FONT test", "UNKNOWN field\nFONT test"), 3),
    ] {
        fs::write(&input, source).unwrap();
        fs::write(&target, b"KEEP").unwrap();
        let result = run(&[
            "convert",
            input.to_str().unwrap(),
            "-o",
            target.to_str().unwrap(),
            "--strict",
            "--report",
            "json",
        ]);
        assert_eq!(result.status.code(), Some(code));
        assert_eq!(fs::read(&target).unwrap(), b"KEEP");
    }
    fs::write(&input, BDF).unwrap();
    let result = run(&[
        "convert",
        input.to_str().unwrap(),
        input.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(fs::read(&target).unwrap(), b"KEEP");
    let bad = dir.0.join("missing/out.bdf");
    assert_eq!(
        run(&[
            "convert",
            input.to_str().unwrap(),
            "-o",
            bad.to_str().unwrap()
        ])
        .status
        .code(),
        Some(1)
    );
    assert!(!bad.exists());
    // Input and output may be the same path: all reading/validation happens first.
    assert!(
        run(&[
            "convert",
            input.to_str().unwrap(),
            "-o",
            input.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&input).unwrap(), BDF.as_bytes());
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
}

#[test]
fn pcf_conversion_layout_gzip_and_strict_reporting() {
    let status: serde_json::Value =
        serde_json::from_slice(&run(&["formats", "--json"]).stdout).unwrap();
    if !status["formats"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["format"]["id"] == "pcf" && v["write"] == true)
    {
        return;
    }
    let dir = TempDir::new();
    let source = dir.0.join("in.bdf");
    let target = dir.0.join("out.pcf.gz");
    fs::write(&source,b"STARTFONT 2.1\nFONT tiny\nSIZE 8 75 75\nFONTBOUNDINGBOX 1 1 0 0\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 2 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n").unwrap();
    let p = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
        "--byteorder",
        "lsb",
        "--bitorder",
        "msb",
        "--glyphpad",
        "8",
        "--scanunit",
        "4",
        "--report",
        "json",
    ]);
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    assert!(fs::read(&target).unwrap().starts_with(&[31, 139]));
    let p = run(&["info", target.to_str().unwrap(), "--json"]);
    assert!(p.status.success());
    let j: serde_json::Value = serde_json::from_slice(&p.stdout).unwrap();
    assert_eq!(j["format"], "pcf");
    let existing = fs::read(&target).unwrap();
    let p = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
        "--byteorder",
        "lsb",
        "--glyphpad",
        "1",
        "--scanunit",
        "4",
    ]);
    assert_eq!(p.status.code(), Some(1));
    assert_eq!(fs::read(&target).unwrap(), existing);
    let text = fs::read_to_string(&source)
        .unwrap()
        .replace("ENCODING 65", "ENCODING 70000");
    fs::write(&source, text).unwrap();
    let p = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
        "--strict",
        "--report",
        "json",
    ]);
    assert_eq!(p.status.code(), Some(3));
    assert_eq!(fs::read(&target).unwrap(), existing);
}

#[test]
fn preview_keeps_art_on_stdout_and_reports_on_stderr() {
    let dir = TempDir::new();
    let input = dir.0.join("font.bdf");
    fs::write(&input, BDF).unwrap();
    let result = run(&["preview", input.to_str().unwrap(), "A", "--chars", "10"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        "00000000\n00000000\n00000000\n00000000\n00000000\n00000000\n00000000\n10000000\n"
    );
    let result = run(&["preview", input.to_str().unwrap(), "A?", "--report", "json"]);
    assert!(result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert!(
        report["report"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["code"] == "render.missing_glyph")
    );
    for (limit, success) in [("73", false), ("74", true)] {
        let result = run(&[
            "preview",
            input.to_str().unwrap(),
            "A",
            "--chars",
            "█.",
            "--max-output-bytes",
            limit,
        ]);
        assert_eq!(result.status.success(), success);
        if success {
            assert_eq!(result.stdout.len(), 74);
        } else {
            assert!(result.stdout.is_empty());
        }
    }
    let result = run(&["preview", input.to_str().unwrap(), "A?", "--strict"]);
    assert_eq!(result.status.code(), Some(3));
    assert!(result.stdout.is_empty());
    assert_eq!(
        run(&[
            "preview",
            input.to_str().unwrap(),
            "A",
            "--direction",
            "invalid"
        ])
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        run(&["preview", input.to_str().unwrap(), "A", "--chars", "three"])
            .status
            .code(),
        Some(1)
    );
}
#[test]
fn render_svg_drawall_controls_and_atomic_failure() {
    let dir = TempDir::new();
    let input = dir.0.join("font.bdf");
    fs::write(&input, BDF).unwrap();
    let target = dir.0.join("image.svg");
    let result = run(&[
        "render",
        input.to_str().unwrap(),
        "--all",
        "--range",
        "U+0041-0041",
        "--order",
        "reverse-file",
        "-o",
        target.to_str().unwrap(),
        "--scale",
        "3",
        "--color",
        "#a23",
        "--background",
        "transparent",
        "--report",
        "json",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["width"], 8);
    let original = fs::read(&target).unwrap();
    let svg = String::from_utf8(original.clone()).unwrap();
    assert!(svg.contains("width=\"24\" height=\"24\""));
    assert!(svg.contains("#aa2233"));
    for flags in [
        vec!["--strict"],
        vec!["--fail-missing"],
        vec!["--max-pixels", "1"],
    ] {
        let mut args = vec![
            "render",
            input.to_str().unwrap(),
            "?",
            "-o",
            target.to_str().unwrap(),
        ];
        args.extend(flags);
        assert!(!run(&args).status.success());
        assert_eq!(fs::read(&target).unwrap(), original);
    }
    let result = run(&[
        "render",
        input.to_str().unwrap(),
        "A",
        "-o",
        target.to_str().unwrap(),
        "--scale",
        "0",
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(fs::read(&target).unwrap(), original);
    let result = run(&[
        "preview",
        input.to_str().unwrap(),
        "--all",
        "--range",
        "U+0042-0043",
    ]);
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
}
#[cfg(feature = "png")]
#[test]
fn render_png_and_empty_input_behavior() {
    let dir = TempDir::new();
    let input = dir.0.join("font.bdf");
    fs::write(&input, BDF).unwrap();
    let target = dir.0.join("image.png");
    assert!(
        run(&[
            "render",
            input.to_str().unwrap(),
            "A",
            "-o",
            target.to_str().unwrap()
        ])
        .status
        .success()
    );
    let original = fs::read(&target).unwrap();
    assert!(original.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(
        !run(&[
            "render",
            input.to_str().unwrap(),
            "",
            "-o",
            target.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&target).unwrap(), original);
    assert!(
        run(&["preview", input.to_str().unwrap(), ""])
            .stdout
            .is_empty()
    );
}

#[test]
fn yaff_collection_and_gzip_roundtrip() {
    let dir = TempDir::new();
    let source = dir.0.join("source.yaff");
    let output = dir.0.join("out.yaffs.gz");
    fs::write(
        &source,
        include_bytes!("../../../tests/data/yaff/portable.yaff"),
    )
    .unwrap();
    let converted = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        output.to_str().unwrap(),
        "--strict",
        "--report",
        "json",
    ]);
    assert!(
        converted.status.success(),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    let info = run(&["info", output.to_str().unwrap(), "--json"]);
    assert!(info.status.success());
    let json: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(json["format"], "yaff");
    assert_eq!(json["fonts"].as_array().unwrap().len(), 2);
}

#[cfg(feature = "opentype")]
#[test]
fn variants_subset_cli_all_containers_and_selectors() {
    let dir = TempDir::new();
    let base = dir.0.join("base.bdf");
    fs::write(&base, include_bytes!("../../../tests/data/compat/mini.bdf")).unwrap();
    let base = base.to_str().unwrap();
    for ext in ["ttf", "otf", "woff", "woff2"] {
        let out = dir.0.join(format!("variants.{ext}"));
        let output = run(&[
            "variants",
            "--base",
            base,
            "--lang",
            &format!("ja={base}"),
            "--script",
            &format!("latn,grek,cyrl={base}"),
            "-o",
            out.to_str().unwrap(),
            "--woff2-quality",
            "1",
            "--report",
            "json",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for (selector, value) in [("--text", "A"), ("--unicodes", "U+0041")] {
            let subset = dir.0.join(format!("subset-{selector}.{ext}"));
            let output = run(&[
                "subset",
                out.to_str().unwrap(),
                selector,
                value,
                "-o",
                subset.to_str().unwrap(),
                "--woff2-quality",
                "1",
            ]);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let input = dir.0.join("variants.ttf");
    let output = dir.0.join("subset.ttf");
    let text = dir.0.join("text.txt");
    fs::write(&text, "A").unwrap();
    assert!(
        run(&[
            "subset",
            input.to_str().unwrap(),
            "--text-file",
            text.to_str().unwrap(),
            "-o",
            output.to_str().unwrap()
        ])
        .status
        .success()
    );
    fs::write(&output, "sentinel").unwrap();
    for selection in ["U+D800", "U+110000", "U+45-41", "oops"] {
        assert!(
            !run(&[
                "subset",
                input.to_str().unwrap(),
                "--unicodes",
                selection,
                "-o",
                output.to_str().unwrap()
            ])
            .status
            .success()
        );
        assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    }
    assert_eq!(
        run(&[
            "subset",
            input.to_str().unwrap(),
            "--text",
            "A",
            "--unicodes",
            "41",
            "-o",
            output.to_str().unwrap()
        ])
        .status
        .code(),
        Some(2)
    );
}

#[cfg(feature = "opentype")]
#[test]
fn raster_collection_cli_and_atomic_failures() {
    let dir = TempDir::new();
    let source = dir.0.join("source.bdf");
    fs::write(&source,"STARTFONT 2.1\nFONT cli-grid\nSIZE 8 75 75\nFONTBOUNDINGBOX 3 3 0 -1\nSTARTPROPERTIES 3\nPIXEL_SIZE 8\nFONT_ASCENT 7\nFONT_DESCENT 1\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nSWIDTH 500 0\nDWIDTH 4 0\nBBX 3 3 0 -1\nBITMAP\nA0\nE0\nA0\nENDCHAR\nENDFONT\n").unwrap();
    for extension in ["ttf", "otf", "woff", "woff2", "ttc", "otc"] {
        let output = dir.0.join(format!("font.{extension}"));
        let result = run(&[
            "convert",
            source.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let info = run(&["info", output.to_str().unwrap(), "--json"]);
        assert!(
            info.status.success(),
            "{}",
            String::from_utf8_lossy(&info.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
        assert_eq!(json["font_count"], 1);
    }
    let dot = dir.0.join("dot.ttf");
    let result = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        dot.to_str().unwrap(),
        "--shape",
        "dot",
    ]);
    assert!(result.status.success());
    assert!(!run(&["info", dot.to_str().unwrap()]).status.success());
    assert!(
        run(&["info", dot.to_str().unwrap(), "--ppem", "8"])
            .status
            .success()
    );
    assert!(
        !run(&[
            "info",
            dot.to_str().unwrap(),
            "--face-index",
            "1",
            "--ppem",
            "8"
        ])
        .status
        .success()
    );
    let sentinel = dir.0.join("sentinel.bdf");
    fs::write(&sentinel, b"keep me").unwrap();
    let fail = run(&[
        "convert",
        dot.to_str().unwrap(),
        "-o",
        sentinel.to_str().unwrap(),
        "--ppem",
        "8",
        "--strict",
    ]);
    assert_eq!(fail.status.code(), Some(3));
    assert_eq!(fs::read(&sentinel).unwrap(), b"keep me");
    let many = dir.0.join("many.ttc");
    assert!(
        run(&[
            "convert",
            source.to_str().unwrap(),
            source.to_str().unwrap(),
            "-o",
            many.to_str().unwrap(),
            "--collection-v2"
        ])
        .status
        .success()
    );
    let one = run(&[
        "info",
        many.to_str().unwrap(),
        "--face-index",
        "1",
        "--json",
    ]);
    assert!(one.status.success());
    let json: serde_json::Value = serde_json::from_slice(&one.stdout).unwrap();
    assert_eq!(json["font_count"], 1);
    assert!(
        !run(&["info", many.to_str().unwrap(), "--face-index", "2"])
            .status
            .success()
    );
}

#[cfg(feature = "svg")]
#[test]
fn svg_convert_render_and_atomic_errors() {
    let dir = TempDir::new();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/bdf/tiny.bdf");
    // Use a generated independent minimal input, including a blank space glyph.
    let source = if source.is_file() {
        source
    } else {
        let p = dir.0.join("source.bdf");
        fs::write(&p,"STARTFONT 2.1\nFONT svg-test\nSIZE 8 75 75\nFONTBOUNDINGBOX 3 3 0 0\nSTARTPROPERTIES 3\nFONT_ASCENT 3\nFONT_DESCENT 0\nPIXEL_SIZE 8\nENDPROPERTIES\nCHARS 2\nSTARTCHAR A\nENCODING 65\nSWIDTH 500 0\nDWIDTH 4 0\nBBX 3 3 0 0\nBITMAP\nA0\nE0\nA0\nENDCHAR\nSTARTCHAR space\nENCODING 32\nSWIDTH 500 0\nDWIDTH 2 0\nBBX 0 0 0 0\nBITMAP\nENDCHAR\nENDFONT\n").unwrap();
        p
    };
    let svg = dir.0.join("font.svg");
    let output = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        svg.to_str().unwrap(),
        "--shape",
        "dot",
        "--dotsize",
        "0.8",
        "--ppu",
        "20",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let saved = fs::read(&svg).unwrap();
    let info = run(&["info", svg.to_str().unwrap(), "--json"]);
    assert!(info.status.success());
    let json: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(json["format"], "svg");
    let image = dir.0.join("image.svg");
    assert!(
        run(&[
            "render",
            svg.to_str().unwrap(),
            "A",
            "-o",
            image.to_str().unwrap(),
            "--shape",
            "dot"
        ])
        .status
        .success()
    );
    let bad = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        svg.to_str().unwrap(),
        "--embed-bitmaps",
    ]);
    assert!(!bad.status.success());
    assert_eq!(fs::read(&svg).unwrap(), saved);
    let bad = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        svg.to_str().unwrap(),
        "--strict",
    ]);
    assert_eq!(bad.status.code(), Some(3));
    assert_eq!(fs::read(&svg).unwrap(), saved);
}

#[cfg(feature = "svg")]
#[test]
fn svg_explicit_input_budget_applies_to_inspection_conversion_and_rendering() {
    let dir = TempDir::new();
    let source = dir.0.join("source.svg");
    fs::write(&source, "<svg><font><font-face units-per-em='800'/><glyph unicode='A' d='M0 0h100v100H0Z'/></font></svg>").unwrap();
    let destination = dir.0.join("output.svg");
    for mut args in [
        vec!["info", source.to_str().unwrap()],
        vec![
            "convert",
            source.to_str().unwrap(),
            "-o",
            destination.to_str().unwrap(),
        ],
        vec![
            "render",
            source.to_str().unwrap(),
            "A",
            "-o",
            destination.to_str().unwrap(),
        ],
    ] {
        fs::write(&destination, b"keep me").unwrap();
        args.extend(["--max-input-bytes", "16"]);
        assert!(!run(&args).status.success());
        assert_eq!(fs::read(&destination).unwrap(), b"keep me");
        *args.last_mut().unwrap() = "4096";
        let result = run(&args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[cfg(feature = "opentype")]
#[test]
fn native_dot_reuse_cli_policies_and_validation() {
    let temp = TempDir::new();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/compat/mini.bdf");
    for policy in ["literal", "rows", "patterns"] {
        let output = temp.0.join(format!("{policy}.ttf"));
        let result = run(&[
            "variants",
            "--base",
            source.to_str().unwrap(),
            "--shape",
            "dot",
            "--dot-reuse",
            policy,
            "-o",
            output.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let converted = temp.0.join(format!("converted-{policy}.ttf"));
        let result = run(&[
            "convert",
            source.to_str().unwrap(),
            "--shape",
            "dot",
            "--dot-reuse",
            policy,
            "-o",
            converted.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let output = temp.0.join("invalid.svg");
    assert!(
        !run(&[
            "convert",
            source.to_str().unwrap(),
            "--dot-reuse",
            "rows",
            "-o",
            output.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert!(!output.exists());
}

#[cfg(all(feature = "hex", feature = "psf"))]
#[test]
fn hex_psf_cli_variants_and_strict_atomic_failure() {
    let temp = TempDir::new();
    let source = temp.0.join("source.hex");
    fs::write(
        &source,
        format!("0041:{}\n0042:{}\n", "A5".repeat(8), "5A".repeat(8)),
    )
    .unwrap();
    let psf = temp.0.join("out.psf.gz");
    let back = temp.0.join("back.hex");
    let o = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        psf.to_str().unwrap(),
        "--hex-height",
        "8",
        "--strict",
        "--report",
        "json",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o = run(&["info", psf.to_str().unwrap(), "--json"]);
    assert!(o.status.success());
    let info: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(info["format"], "psf");
    let o = run(&[
        "convert",
        psf.to_str().unwrap(),
        "-o",
        back.to_str().unwrap(),
        "--hex-height",
        "8",
        "--strict",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(fs::read(&back).unwrap(), fs::read(&source).unwrap());
    let sentinel = temp.0.join("sentinel.psf");
    fs::write(&sentinel, b"unchanged").unwrap();
    let o = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        sentinel.to_str().unwrap(),
        "--hex-height",
        "8",
        "--psf-version",
        "1",
        "--strict",
    ]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged");
    let o = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        sentinel.to_str().unwrap(),
        "--hex-height",
        "8",
        "--psf-version",
        "1",
        "--psf1-count",
        "512",
        "--report",
        "json",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(&fs::read(&sentinel).unwrap()[..4], &[0x36, 4, 3, 8]);
    for args in [
        vec!["--psf1-count", "17"],
        vec!["--psf1-count", "256"],
        vec!["--psf-version", "1", "--psf1-count", "17"],
    ] {
        fs::write(&sentinel, b"unchanged").unwrap();
        let mut a = vec![
            "convert",
            source.to_str().unwrap(),
            "-o",
            sentinel.to_str().unwrap(),
            "--hex-height",
            "8",
        ];
        a.extend(args);
        assert!(!run(&a).status.success());
        assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged");
    }
    let o = run(&[
        "info",
        source.to_str().unwrap(),
        "--hex-height",
        "8",
        "--hex-descent",
        "9",
    ]);
    assert!(!o.status.success());
    let o = run(&[
        "convert",
        source.to_str().unwrap(),
        "-o",
        back.to_str().unwrap(),
        "--hex-height",
        "8",
        "--psf-version",
        "1",
    ]);
    assert!(!o.status.success());
}

#[cfg(all(feature = "fnt", feature = "fon"))]
#[test]
fn windows_options_multifont_and_strict_preserve_destination() {
    let dir = TempDir::new();
    let input = dir.0.join("input.bdf");
    fs::write(&input, BDF).unwrap();
    let target = dir.0.join("font.fnt");
    let result = run(&[
        "convert",
        input.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
        "--to",
        "fnt",
        "--fnt-version",
        "3",
        "--fnt-charset",
        "0",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(&fs::read(&target).unwrap()[..2], b"\0\x03");
    let ne = dir.0.join("pair.fon");
    let result = run(&[
        "convert",
        target.to_str().unwrap(),
        target.to_str().unwrap(),
        "-o",
        ne.to_str().unwrap(),
        "--to",
        "fon",
        "--strict",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = run(&["info", ne.to_str().unwrap(), "--json"]);
    assert!(result.status.success());
    let j: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(j["font_count"], 2);
    fs::write(&target, b"PROTECTED").unwrap();
    let result = run(&[
        "convert",
        input.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
        "--to",
        "fnt",
        "--strict",
    ]);
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(fs::read(&target).unwrap(), b"PROTECTED");
    let result = run(&[
        "convert",
        input.to_str().unwrap(),
        "-o",
        dir.0.join("x.bdf").to_str().unwrap(),
        "--fnt-version",
        "3",
    ]);
    assert!(!result.status.success());
}
