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
                && !line.starts_with("yaff\t"))
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
                && entry["format"]["id"] != "yaff")
            .all(|entry| entry["read"] == false
                && entry["write"] == false
                && entry["detect"] == false)
    );
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
    let explicit = run(&["info", input.to_str().unwrap(), "--from", "fnt"]);
    assert!(
        String::from_utf8_lossy(&explicit.stderr)
            .contains("read for format 'fnt' is not implemented")
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
        "fnt",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("write for format 'fnt' is not implemented")
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
