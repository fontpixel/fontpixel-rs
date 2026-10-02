//! CLI behavior and side-effect checks; no real font support is asserted.
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
fn formats_lists_all_46_and_marks_every_codec_unimplemented() {
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
    assert!(formats.iter().all(|entry| entry["read"] == false
        && entry["write"] == false
        && entry["detect"] == false));
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
fn info_does_not_claim_success_for_unimplemented_reader() {
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
            .contains("read for format 'bdf' is not implemented")
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
    let target = dir.0.join("out.pcf");
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
                .contains("write for format 'pcf' is not implemented")
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
