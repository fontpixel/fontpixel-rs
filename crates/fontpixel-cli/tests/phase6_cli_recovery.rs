//! Recovery reconstruction of later end-to-end regressions, plus local phase-6 fixes.
//! Native acceptance is established only by the coordinator's recorded test runs.
#![cfg(feature = "bdf")]
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("w6.1-tests");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, data: impl AsRef<[u8]>) {
        fs::write(self.0.join(path), data).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fontpixel"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn glyph(code: i64, row: &str) -> String {
    format!(
        "STARTCHAR g{code}\nENCODING {code}\nSWIDTH 500 0\nDWIDTH 2 0\nBBX 2 2 0 0\nBITMAP\n{row}\nC0\nENDCHAR\n"
    )
}
fn bdf(codes: &[i64]) -> String {
    format!(
        "STARTFONT 2.1\nFONT -test-phase6-medium-r-normal--2-20-75-75-p-20-iso10646-1\nSIZE 2 75 75\nFONTBOUNDINGBOX 2 2 0 0\nSTARTPROPERTIES 5\nFONT_ASCENT 2\nFONT_DESCENT 0\nCHARSET_REGISTRY \"ISO10646\"\nCHARSET_ENCODING \"1\"\nCOPYRIGHT \"CC0\"\nENDPROPERTIES\nCHARS {}\n{}ENDFONT\n",
        codes.len(),
        codes.iter().map(|c| glyph(*c, "80")).collect::<String>()
    )
}
fn json(o: &Output) -> serde_json::Value {
    serde_json::from_slice(&o.stdout).unwrap()
}
fn success(o: &Output) {
    assert!(
        o.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
fn batch_task(id: &str, input: &str, output: &str) -> String {
    format!(
        "[[tasks]]\nid=\"{id}\"\nkind=\"convert\"\ninputs=[\"{input}\"]\noutput=\"{output}\"\nfrom=\"bdf\"\nto=\"bdf\"\n"
    )
}
#[test]
fn batch_reports_use_actual_aggregate_bytes_not_equal_shares() {
    let w = Work::new();
    w.write("small.bdf", bdf(&[65]));
    w.write("verbose.bdf", bdf(&vec![65; 30]));
    let mut tasks = batch_task("verbose", "verbose.bdf", "verbose-out.bdf");
    for i in 0..11 {
        tasks.push_str(&batch_task(
            &format!("small{i}"),
            "small.bdf",
            &format!("small{i}.bdf"),
        ));
    }
    w.write(
        "jobs.toml",
        format!("version=1\nmax_report_bytes=131072\nmax_task_report_bytes=60000\n{tasks}"),
    );
    let o = w.run(&["batch", "jobs.toml", "--jobs", "4"]);
    success(&o);
    let j = json(&o);
    assert_eq!(j["summary"]["committed"], 12);
    assert!(
        j["tasks"][0]["result"]["report"]["entries"]
            .as_array()
            .unwrap()
            .len()
            >= 20
    );
}
#[test]
fn batch_aggregate_report_exhaustion_aborts_every_output() {
    let w = Work::new();
    w.write("verbose.bdf", bdf(&vec![65; 30]));
    let tasks = (0..12)
        .map(|i| batch_task(&format!("t{i}"), "verbose.bdf", &format!("out{i}.bdf")))
        .collect::<String>();
    w.write(
        "jobs.toml",
        format!("version=1\nmax_report_bytes=65536\nmax_task_report_bytes=60000\n{tasks}"),
    );
    let o = w.run(&["batch", "jobs.toml", "--jobs", "4"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(json(&o)["summary"]["committed"], 0);
    for i in 0..12 {
        assert!(!w.0.join(format!("out{i}.bdf")).exists());
    }
}
#[test]
fn preview_json_preserves_complete_ordered_strict_diagnostics() {
    let w = Work::new();
    w.write("font.bdf", bdf(&[65, 65]));
    let ordinary = w.run(&["preview", "font.bdf", "AZΩ", "--report", "json"]);
    success(&ordinary);
    let ordinary: serde_json::Value = serde_json::from_slice(&ordinary.stderr).unwrap();
    let strict = w.run(&["preview", "font.bdf", "AZΩ", "--report", "json", "--strict"]);
    assert_eq!(strict.status.code(), Some(3));
    assert!(strict.stdout.is_empty());
    let strict: serde_json::Value = serde_json::from_slice(&strict.stderr).unwrap();
    assert!(ordinary["report"]["entries"].as_array().unwrap().len() >= 2);
    assert_eq!(ordinary["report"], strict["report"]);
    let rendered = w.run(&[
        "render",
        "font.bdf",
        "AZΩ",
        "-o",
        "preview.svg",
        "--report",
        "json",
    ]);
    success(&rendered);
    let rendered = json(&rendered);
    assert_eq!(rendered["report"], ordinary["report"]);
    assert_eq!(rendered["format"], "svg");
    assert_eq!(rendered["output"], "preview.svg");
    assert_eq!(
        rendered["bytes"].as_u64().unwrap(),
        fs::metadata(w.0.join("preview.svg")).unwrap().len()
    );
    assert!(rendered["width"].as_u64().unwrap() > 0);
    let info = w.run(&["info", "font.bdf", "--json"]);
    success(&info);
    let info = json(&info);
    assert_eq!(info["fonts"][0]["distinct_codes"], 1);
    assert!(!info["report"]["entries"].as_array().unwrap().is_empty());
}
#[cfg(feature = "opentype")]
#[test]
fn variants_json_and_batch_preserve_native_report_and_output_bytes() {
    let w = Work::new();
    let source = bdf(&vec![65; 80]);
    w.write("font.bdf", &source);
    let loaded = fontpixel::registry()
        .read(
            &fontpixel::Input::new(source.as_bytes()).with_name("font.bdf"),
            &Default::default(),
        )
        .unwrap();
    let mut expected_report = loaded.report;
    let variants = fontpixel::Variants::new(loaded.fonts.into_single().unwrap());
    let encoded = fontpixel::variants::encode(&variants, &Default::default()).unwrap();
    expected_report.extend(encoded.report);
    assert!(expected_report.entries.len() >= 79);
    let expected_report = serde_json::to_value(&expected_report).unwrap();
    let standalone = w.run(&[
        "variants",
        "--base",
        "font.bdf",
        "-o",
        "direct.ttf",
        "--report",
        "json",
    ]);
    success(&standalone);
    assert_eq!(json(&standalone)["report"], expected_report);
    assert_eq!(fs::read(w.0.join("direct.ttf")).unwrap(), encoded.bytes);
    w.write("jobs.toml","version=1\n[[tasks]]\nid='variants'\nkind='variants'\nbase='font.bdf'\noutput='batch.ttf'\n");
    let batch = w.run(&["batch", "jobs.toml"]);
    success(&batch);
    let batch = json(&batch);
    assert_eq!(batch["summary"]["committed"], 1);
    assert_eq!(batch["tasks"][0]["result"]["report"], expected_report);
    assert_eq!(fs::read(w.0.join("batch.ttf")).unwrap(), encoded.bytes);
}
#[test]
fn batch_worker_count_obeys_admission_without_reordering_results() {
    let w = Work::new();
    w.write("font.bdf", bdf(&[65]));
    let tasks = (0..6)
        .map(|i| batch_task(&format!("task{i}"), "font.bdf", &format!("out{i}.bdf")))
        .collect::<String>();
    w.write(
        "jobs.toml",
        format!("version=1\njobs=6\nmax_working_bytes=536870912\n{tasks}"),
    );
    let output = w.run(&["batch", "jobs.toml"]);
    success(&output);
    let report = json(&output);
    assert_eq!(report["jobs"], 6);
    assert_eq!(report["workers"], 2);
    assert_eq!(report["summary"]["committed"], 6);
    for i in 0..6 {
        assert_eq!(report["tasks"][i]["id"], format!("task{i}"));
        assert!(w.0.join(format!("out{i}.bdf")).exists());
    }
}
#[test]
fn batch_all_invalid_tasks_need_no_workers_and_preserve_order() {
    let w = Work::new();
    w.write("font.bdf", bdf(&[65]));
    let tasks = (0..3)
        .map(|i| {
            format!(
                "{}[tasks.options]\ninvented=true\n",
                batch_task(&format!("bad{i}"), "font.bdf", &format!("out{i}.bdf"))
            )
        })
        .collect::<String>();
    w.write("jobs.toml", format!("version=1\njobs=9\n{tasks}"));
    let output = w.run(&["batch", "jobs.toml"]);
    assert_eq!(output.status.code(), Some(2));
    let report = json(&output);
    assert_eq!(report["workers"], 0);
    assert_eq!(report["summary"]["committed"], 0);
    for i in 0..3 {
        assert_eq!(report["tasks"][i]["id"], format!("bad{i}"));
        assert_eq!(report["tasks"][i]["exit_code"], 2);
        assert!(!w.0.join(format!("out{i}.bdf")).exists());
    }
}
#[test]
fn batch_large_reports_spool_without_changing_payload_order_or_outputs() {
    let w = Work::new();
    w.write("verbose.bdf", bdf(&vec![65; 6000]));
    w.write("small.bdf", bdf(&[65]));
    fs::create_dir(w.0.join("spools")).unwrap();
    let direct = w.run(&[
        "convert",
        "verbose.bdf",
        "-o",
        "direct.bdf",
        "--report",
        "json",
    ]);
    success(&direct);
    let expected = json(&direct)["report"].clone();
    assert!(serde_json::to_vec(&expected).unwrap().len() > 1024 * 1024);
    let tasks = format!(
        "{}{}{}",
        batch_task("first", "verbose.bdf", "first.bdf"),
        batch_task("tiny", "small.bdf", "tiny.bdf"),
        batch_task("last", "verbose.bdf", "last.bdf")
    );
    w.write("jobs.toml", format!("version=1\njobs=3\n{tasks}"));
    let output = Command::new(env!("CARGO_BIN_EXE_fontpixel"))
        .current_dir(&w.0)
        .env("TMPDIR", w.0.join("spools"))
        .args(["batch", "jobs.toml"])
        .output()
        .unwrap();
    success(&output);
    let report = json(&output);
    assert_eq!(report["summary"]["committed"], 3);
    for (index, id) in ["first", "tiny", "last"].iter().enumerate() {
        assert_eq!(report["tasks"][index]["id"], *id);
    }
    assert_eq!(report["tasks"][0]["result"]["report"], expected);
    assert_eq!(report["tasks"][2]["result"]["report"], expected);
    assert_eq!(
        fs::read(w.0.join("first.bdf")).unwrap(),
        fs::read(w.0.join("direct.bdf")).unwrap()
    );
    assert_eq!(
        fs::read(w.0.join("last.bdf")).unwrap(),
        fs::read(w.0.join("direct.bdf")).unwrap()
    );
    assert_eq!(fs::read_dir(w.0.join("spools")).unwrap().count(), 0);
}
#[test]
fn batch_spool_io_failure_aborts_all_outputs_and_preserves_inputs() {
    let w = Work::new();
    let original = bdf(&vec![65; 6000]);
    w.write("verbose.bdf", &original);
    w.write("small.bdf", bdf(&[65]));
    w.write("not-a-directory", "temporary-root sentinel");
    w.write("first.bdf", "output sentinel");
    let tasks = format!(
        "{}{}",
        batch_task("verbose", "verbose.bdf", "first.bdf"),
        batch_task("small", "small.bdf", "second.bdf")
    );
    w.write("jobs.toml", format!("version=1\n{tasks}"));
    let output = Command::new(env!("CARGO_BIN_EXE_fontpixel"))
        .current_dir(&w.0)
        .env("TMPDIR", w.0.join("not-a-directory"))
        .args(["batch", "jobs.toml"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output);
    assert_eq!(report["summary"]["committed"], 0);
    assert!(
        report["tasks"][0]["error"]
            .as_str()
            .unwrap()
            .contains("report storage failed")
    );
    assert_eq!(
        fs::read_to_string(w.0.join("first.bdf")).unwrap(),
        "output sentinel"
    );
    assert!(!w.0.join("second.bdf").exists());
    assert_eq!(
        fs::read_to_string(w.0.join("verbose.bdf")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(w.0.join("not-a-directory")).unwrap(),
        "temporary-root sentinel"
    );
}
#[test]
fn batch_report_exhaustion_after_spilling_cleans_up_before_commit() {
    let w = Work::new();
    w.write("verbose.bdf", bdf(&vec![65; 6000]));
    fs::create_dir(w.0.join("spools")).unwrap();
    let tasks = (0..4)
        .map(|i| batch_task(&format!("large{i}"), "verbose.bdf", &format!("out{i}.bdf")))
        .collect::<String>();
    w.write("out0.bdf", "previous output");
    w.write(
        "jobs.toml",
        format!(
            "version=1\njobs=4\nmax_report_bytes=3145728\nmax_task_report_bytes=4194304\n{tasks}"
        ),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_fontpixel"))
        .current_dir(&w.0)
        .env("TMPDIR", w.0.join("spools"))
        .args(["batch", "jobs.toml"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output);
    assert_eq!(report["summary"]["committed"], 0);
    assert!(
        report["tasks"][0]["error"]
            .as_str()
            .unwrap()
            .contains("aggregate report byte limit")
    );
    assert_eq!(
        fs::read_to_string(w.0.join("out0.bdf")).unwrap(),
        "previous output"
    );
    for i in 1..4 {
        assert!(!w.0.join(format!("out{i}.bdf")).exists());
    }
    assert_eq!(fs::read_dir(w.0.join("spools")).unwrap().count(), 0);
}

#[test]
fn reconstructed_commands_have_help_examples() {
    let w = Work::new();
    for command in [
        "batch", "diff", "validate", "charset", "convert", "formats", "info", "preview", "render",
    ] {
        let o = w.run(&[command, "--help"]);
        success(&o);
        let text = String::from_utf8(o.stdout).unwrap();
        assert!(text.contains("Example"));
    }
}
#[test]
fn native_validate_reports_scope_and_honors_input_budget() {
    let w = Work::new();
    w.write("a.bdf", bdf(&[65]));
    let o = w.run(&["validate", "a.bdf", "--json"]);
    success(&o);
    let report = json(&o);
    assert_eq!(report["valid"], true);
    assert_eq!(report["font_count"], 1);
    assert!(
        report["faces"][0]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "model")
    );
    let o = w.run(&["validate", "a.bdf", "--json", "--max-total-bytes", "32"]);
    assert_eq!(o.status.code(), Some(1));
    w.write("bad.bdf", "this is not a font");
    let o = w.run(&["validate", "bad.bdf", "--from", "bdf"]);
    assert_eq!(o.status.code(), Some(1));
}
#[test]
fn diff_ignores_encoded_storage_order_but_detects_bitmap_and_metrics() {
    let w = Work::new();
    w.write("a.bdf", bdf(&[65, 66]));
    w.write("b.bdf", bdf(&[66, 65]));
    let o = w.run(&["diff", "a.bdf", "b.bdf", "--json"]);
    success(&o);
    assert_eq!(json(&o)["equal"], true);
    w.write("b.bdf", bdf(&[66, 65]).replacen("80\nC0", "40\nC0", 1));
    let o = w.run(&["diff", "a.bdf", "b.bdf", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        json(&o)["differences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "glyph_bitmap")
    );
    w.write("b.bdf", bdf(&[65, 66]).replace("DWIDTH 2 0", "DWIDTH 3 0"));
    let o = w.run(&["diff", "a.bdf", "b.bdf", "--json", "--max-differences", "0"]);
    assert_eq!(o.status.code(), Some(1));
    let j = json(&o);
    assert!(j["difference_count"].as_u64().unwrap() > 0);
    assert!(j["differences"].as_array().unwrap().is_empty());
    assert_eq!(j["truncated"], true);
}
#[test]
fn diff_keeps_duplicate_mappings_and_unencoded_order() {
    let w = Work::new();
    w.write("a.bdf", bdf(&[65, 65, -1]));
    w.write("b.bdf", bdf(&[65, -1]));
    let o = w.run(&["diff", "a.bdf", "b.bdf", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        json(&o)["differences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "mapping_count")
    );
    w.write(
        "b.bdf",
        bdf(&[65, 65, -1]).replacen(
            "ENCODING -1\nSWIDTH 500 0\nDWIDTH 2 0",
            "ENCODING -1\nSWIDTH 500 0\nDWIDTH 4 0",
            1,
        ),
    );
    let o = w.run(&["diff", "a.bdf", "b.bdf", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        json(&o)["differences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "glyph_metrics" && d["codepoint"].is_null())
    );
}
#[test]
fn charset_exact_roundtrip_and_strict_preserves_outputs() {
    let w = Work::new();
    w.write("unicode.bdf", bdf(&[65, 0x20ac]));
    let o = w.run(&[
        "charset",
        "unicode.bdf",
        "-o",
        "latin.bdf",
        "--to",
        "cp1252",
        "--report",
        "json",
    ]);
    success(&o);
    assert!(
        fs::read_to_string(w.0.join("latin.bdf"))
            .unwrap()
            .contains("ENCODING 128")
    );
    let o = w.run(&[
        "charset",
        "latin.bdf",
        "-o",
        "again.bdf",
        "--to",
        "unicode",
        "--report",
        "json",
    ]);
    success(&o);
    success(&w.run(&["diff", "unicode.bdf", "again.bdf", "--json"]));
    w.write("missing.bdf", bdf(&[65, 0x1f600]));
    w.write("sentinel.bdf", "old output");
    let o = w.run(&[
        "charset",
        "missing.bdf",
        "-o",
        "sentinel.bdf",
        "--to",
        "cp1252",
        "--strict",
        "--report",
        "json",
    ]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(w.0.join("sentinel.bdf")).unwrap(),
        "old output"
    );
    assert_eq!(
        fs::read_to_string(w.0.join("missing.bdf")).unwrap(),
        bdf(&[65, 0x1f600])
    );
}
#[test]
fn charset_unknown_and_input_aliases_fail_without_replacement() {
    let w = Work::new();
    let original = bdf(&[65]);
    w.write("input.bdf", &original);
    assert_eq!(
        w.run(&[
            "charset",
            "input.bdf",
            "-o",
            "out.bdf",
            "--to",
            "imaginary-charset"
        ])
        .status
        .code(),
        Some(1)
    );
    assert!(!w.0.join("out.bdf").exists());
    fs::hard_link(w.0.join("input.bdf"), w.0.join("alias.bdf")).unwrap();
    assert_eq!(
        w.run(&["charset", "input.bdf", "-o", "alias.bdf", "--to", "cp1252"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(fs::read_to_string(w.0.join("input.bdf")).unwrap(), original);
}
#[test]
fn charset_reports_unmappable_default_even_when_all_glyphs_map() {
    let w = Work::new();
    let source = bdf(&[65])
        .replace("STARTPROPERTIES 5", "STARTPROPERTIES 6")
        .replace("ENDPROPERTIES", "DEFAULT_CHAR 128512\nENDPROPERTIES");
    w.write("input.bdf", source);
    let o = w.run(&[
        "charset",
        "input.bdf",
        "-o",
        "out.bdf",
        "--to",
        "cp1252",
        "--strict",
        "--report",
        "json",
    ]);
    assert_eq!(o.status.code(), Some(3));
    let report: serde_json::Value = serde_json::from_slice(&o.stderr).unwrap();
    assert!(
        report["report"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["code"] == "charset.unmappable_default_character")
    );
    assert!(!w.0.join("out.bdf").exists());
}
#[test]
fn batch_independent_usage_failure_can_commit_other_valid_task() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    let tasks = format!(
        "{}[tasks.options]\ninvented=true\n{}",
        batch_task("invalid", "input.bdf", "bad.bdf"),
        batch_task("good", "input.bdf", "good.bdf")
    );
    w.write("jobs.toml", format!("version=1\n{tasks}"));
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(2));
    let j = json(&o);
    assert_eq!(j["summary"]["committed"], 1);
    assert_eq!(j["tasks"][0]["committed"], false);
    assert_eq!(j["tasks"][1]["committed"], true);
    assert!(!w.0.join("bad.bdf").exists());
    assert!(w.0.join("good.bdf").exists());
}
#[test]
fn batch_failed_task_declared_input_remains_protected() {
    let w = Work::new();
    w.write("a.bdf", bdf(&[65]));
    let original = bdf(&[66]);
    w.write("b.bdf", &original);
    let tasks = format!(
        "{}{}[tasks.options]\ninvented=true\n",
        batch_task("writer", "a.bdf", "b.bdf"),
        batch_task("invalid", "b.bdf", "out.bdf")
    );
    w.write("jobs.toml", format!("version=1\n{tasks}"));
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(2));
    assert_eq!(json(&o)["summary"]["committed"], 0);
    assert_eq!(fs::read_to_string(w.0.join("b.bdf")).unwrap(), original);
}
#[test]
fn batch_missing_declared_input_cannot_be_fabricated_by_another_task() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    w.write(
        "jobs.toml",
        format!(
            "version=1\n{}{}",
            batch_task("writer", "input.bdf", "missing.bdf"),
            batch_task("reader", "missing.bdf", "out.bdf")
        ),
    );
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(json(&o)["summary"]["committed"], 0);
    assert!(!w.0.join("missing.bdf").exists());
}
#[test]
fn batch_duplicate_destinations_and_job_file_are_protected() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    w.write("out.bdf", "old output");
    w.write(
        "jobs.toml",
        format!(
            "version=1\n{}{}",
            batch_task("a", "input.bdf", "out.bdf"),
            batch_task("b", "input.bdf", "out.bdf")
        ),
    );
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(json(&o)["summary"]["committed"], 0);
    assert_eq!(
        fs::read_to_string(w.0.join("out.bdf")).unwrap(),
        "old output"
    );
    let source = format!(
        "version=1\n{}",
        batch_task("overwrite", "input.bdf", "jobs.toml")
    );
    w.write("jobs.toml", &source);
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(fs::read_to_string(w.0.join("jobs.toml")).unwrap(), source);
}
#[test]
fn batch_strict_and_pending_limits_precede_commit() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    w.write(
        "jobs.toml",
        format!(
            "version=1\nmax_pending_bytes=1\n{}",
            batch_task("too-big", "input.bdf", "out.bdf")
        ),
    );
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(1));
    let failure = json(&o);
    assert!(
        failure["summary"]["committed"] == 0
            || failure["tasks"]
                .as_array()
                .is_some_and(|tasks| tasks.is_empty())
    );
    assert!(!w.0.join("out.bdf").exists());
    w.write("input.bdf", bdf(&[65, 0x1f600]));
    w.write("jobs.toml","version=1\n[[tasks]]\nid='strict'\nkind='charset'\ninputs=['input.bdf']\noutput='out.bdf'\nto='cp1252'\nstrict=true\n");
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(json(&o)["summary"]["committed"], 0);
    assert!(!w.0.join("out.bdf").exists());
}
#[cfg(feature = "opentype")]
fn sfnt_fixture(w: &Work) -> Vec<u8> {
    w.write("font.bdf", bdf(&[65, 66]));
    success(&w.run(&["variants", "--base", "font.bdf", "-o", "font.ttf"]));
    fs::read(w.0.join("font.ttf")).unwrap()
}
#[cfg(feature = "opentype")]
fn replace_table(original: &[u8], tag: [u8; 4], data: Vec<u8>) -> Vec<u8> {
    let limits = fontpixel::sfnt::Limits::default();
    let font = fontpixel::sfnt::Sfnt::parse(original, &limits).unwrap();
    let mut builder = fontpixel::sfnt::SfntBuilder::new(font.flavor());
    for record in font.tables() {
        if record.tag != tag {
            builder.insert(record.tag, record.data.to_vec()).unwrap();
        }
    }
    builder.insert(tag, data).unwrap();
    builder.build(&limits).unwrap()
}
#[cfg(feature = "opentype")]
#[test]
fn validate_sfnt_rejects_checksum_repaired_bad_glyph_structure() {
    let w = Work::new();
    let original = sfnt_fixture(&w);
    success(&w.run(&["validate", "font.ttf", "--json"]));
    let bad = replace_table(&original, *b"loca", vec![0; 2]);
    w.write("bad.ttf", bad);
    let o = w.run(&["validate", "bad.ttf", "--json"]);
    assert_eq!(o.status.code(), Some(1));
}
#[cfg(feature = "opentype")]
fn constant_cmap_group(start: u32, end: u32) -> Vec<u8> {
    let mut cmap = Vec::new();
    cmap.extend(0u16.to_be_bytes());
    cmap.extend(1u16.to_be_bytes());
    cmap.extend(3u16.to_be_bytes());
    cmap.extend(10u16.to_be_bytes());
    cmap.extend(12u32.to_be_bytes());
    cmap.extend(13u16.to_be_bytes());
    cmap.extend(0u16.to_be_bytes());
    cmap.extend(28u32.to_be_bytes());
    cmap.extend(0u32.to_be_bytes());
    cmap.extend(1u32.to_be_bytes());
    cmap.extend(start.to_be_bytes());
    cmap.extend(end.to_be_bytes());
    cmap.extend(1u32.to_be_bytes());
    cmap
}

#[cfg(feature = "opentype")]
#[test]
fn validate_cmap_expansion_is_bounded_independently_of_wire_size() {
    let w = Work::new();
    let original = sfnt_fixture(&w);
    // A valid supplementary-only span keeps the wire tiny while requiring
    // over a million mappings. Surrogate rejection is tested independently.
    let cmap = constant_cmap_group(0x10000, 0x10ffff);
    w.write("wide.ttf", replace_table(&original, *b"cmap", cmap));
    let o = w.run(&[
        "validate",
        "wide.ttf",
        "--json",
        "--max-total-bytes",
        "1048576",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let error = String::from_utf8_lossy(&o.stderr);
    assert!(
        error.contains("sfnt cmap mappings") && error.contains("limit"),
        "{error}"
    );
}

#[cfg(feature = "opentype")]
#[test]
fn validate_rejects_cmap_ranges_crossing_surrogates() {
    let w = Work::new();
    let original = sfnt_fixture(&w);
    w.write(
        "surrogates.ttf",
        replace_table(&original, *b"cmap", constant_cmap_group(0, 0x10ffff)),
    );
    let output = w.run(&["validate", "surrogates.ttf", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("invalid cmap group range"), "{error}");
}

#[cfg(feature = "yaff")]
#[test]
fn diff_and_validate_visit_every_strike() {
    let w = Work::new();
    w.write("a.bdf", bdf(&[65]));
    w.write("b.bdf", bdf(&[66]));
    success(&w.run(&[
        "convert", "a.bdf", "b.bdf", "-o", "two.yaff", "--to", "yaff",
    ]));
    let o = w.run(&["validate", "two.yaff", "--json"]);
    success(&o);
    assert_eq!(json(&o)["font_count"], 2);
    success(&w.run(&["diff", "two.yaff", "two.yaff", "--json"]));
    let o = w.run(&["diff", "a.bdf", "two.yaff", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        json(&o)["differences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "font_count")
    );
}
#[cfg(feature = "yaff")]
#[test]
fn charset_handles_thousands_of_aliases_under_bounded_scratch() {
    let w = Work::new();
    let mut font = fontpixel::Font::default();
    let mut glyph = fontpixel::Glyph::new("#".parse::<fontpixel::Bitmap>().unwrap());
    glyph.codepoints = (32..5032)
        .map(fontpixel::model::Codepoint::Unicode)
        .collect();
    font.add_glyph(glyph).unwrap();
    font.charset.name = Some("unicode".into());
    let set = fontpixel::FontSet::from(font);
    let options =
        fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("aliases.yaff").unwrap());
    let written = fontpixel::registry().write(&set, "yaff", &options).unwrap();
    w.write("aliases.yaff", written.output.into_single().unwrap());
    let o = w.run(&[
        "charset",
        "aliases.yaff",
        "-o",
        "mapped.yaff",
        "--to",
        "unicode",
        "--report",
        "json",
    ]);
    success(&o);
    let bytes = fs::read(w.0.join("mapped.yaff")).unwrap();
    let loaded = fontpixel::registry()
        .read(
            &fontpixel::Input::new(&bytes).with_name("mapped.yaff"),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(loaded.fonts.fonts[0].codepoints().count(), 5000);
    let o = w.run(&[
        "charset",
        "aliases.yaff",
        "-o",
        "too-small.yaff",
        "--to",
        "unicode",
        "--max-total-bytes",
        "65536",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!w.0.join("too-small.yaff").exists());
}
#[cfg(feature = "bmfont")]
#[test]
fn batch_protects_sidecars_discovered_during_other_task_reads() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    let o = w.run(&[
        "convert",
        "input.bdf",
        "-o",
        "font.fnt",
        "--to",
        "bmfont",
        "--report",
        "json",
    ]);
    success(&o);
    let j = json(&o);
    let atlas = j["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["path"].as_str().unwrap())
        .find(|p| p.ends_with(".png"))
        .unwrap();
    let original = fs::read(w.0.join(atlas)).unwrap();
    let tasks = format!(
        "[[tasks]]\nid='read-pages'\nkind='convert'\ninputs=['font.fnt']\nfrom='bmfont'\noutput='read.bdf'\nto='bdf'\n{}",
        batch_task("overwrite-page", "input.bdf", atlas)
    );
    w.write("jobs.toml", format!("version=1\n{tasks}"));
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(json(&o)["summary"]["committed"], 0);
    assert_eq!(fs::read(w.0.join(atlas)).unwrap(), original);
    assert!(!w.0.join("read.bdf").exists());
}
#[cfg(all(feature = "color", feature = "opentype"))]
#[test]
fn validate_checks_color_tables_after_directory_checksums_pass() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    success(&w.run(&["convert", "input.bdf", "-o", "color.ttf", "--to", "color"]));
    success(&w.run(&["validate", "color.ttf", "--json"]));
    let original = fs::read(w.0.join("color.ttf")).unwrap();
    let sfnt = fontpixel::sfnt::Sfnt::parse(&original, &Default::default()).unwrap();
    let tag = if sfnt.table(*b"CBLC").is_some() {
        *b"CBLC"
    } else {
        *b"sbix"
    };
    w.write("broken.ttf", replace_table(&original, tag, vec![0; 4]));
    let o = w.run(&["validate", "broken.ttf", "--json"]);
    assert_eq!(o.status.code(), Some(1));
}
#[test]
fn convert_explicit_names_apply_and_empty_identity_is_usage_error() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    let o = w.run(&[
        "convert",
        "input.bdf",
        "-o",
        "named.bdf",
        "--family",
        "Explicit Family",
        "--style",
        "Monospaced",
        "--copyright",
        "Original author notice",
        "--report",
        "json",
    ]);
    success(&o);
    let bytes = fs::read(w.0.join("named.bdf")).unwrap();
    let loaded = fontpixel::registry()
        .read(
            &fontpixel::Input::new(&bytes).with_name("named.bdf"),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(
        loaded.fonts.fonts[0].names.family.as_deref(),
        Some("Explicit Family")
    );
    assert_eq!(
        loaded.fonts.fonts[0].names.style.as_deref(),
        Some("Monospaced")
    );
    assert_eq!(
        loaded.fonts.fonts[0].names.copyright.as_deref(),
        Some("Original author notice")
    );
    assert_eq!(
        w.run(&["convert", "input.bdf", "-o", "bad.bdf", "--family", "   "])
            .status
            .code(),
        Some(2)
    );
    assert!(!w.0.join("bad.bdf").exists());
    w.write("jobs.toml","version=1\n[[tasks]]\nid='names'\nkind='convert'\ninputs=['input.bdf']\noutput='batch.bdf'\nfamily='Batch Family'\n[tasks.options]\nstyle='Monospaced'\ncopyright='Original author notice'\n");
    success(&w.run(&["batch", "jobs.toml"]));
    let bytes = fs::read(w.0.join("batch.bdf")).unwrap();
    let loaded = fontpixel::registry()
        .read(
            &fontpixel::Input::new(&bytes).with_name("batch.bdf"),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(
        loaded.fonts.fonts[0].names.family.as_deref(),
        Some("Batch Family")
    );
}

#[test]
fn convert_each_requires_format_and_groups_every_output_transactionally() {
    let w = Work::new();
    w.write("a.bdf", bdf(&[65]));
    w.write("b.bdf", bdf(&[66]));
    fs::create_dir(w.0.join("out")).unwrap();
    assert_eq!(
        w.run(&["convert", "a.bdf", "b.bdf", "--each", "-o", "out"])
            .status
            .code(),
        Some(2)
    );
    let o = w.run(&[
        "convert", "a.bdf", "b.bdf", "--each", "--to", "bdf", "-o", "out", "--report", "json",
    ]);
    success(&o);
    assert_eq!(json(&o)["outputs"].as_array().unwrap().len(), 2);
    assert!(w.0.join("out/a.bdf").exists());
    assert!(w.0.join("out/b.bdf").exists());
    w.write("out/a.bdf", "previous A");
    w.write("out/b.bdf", "previous B");
    w.write("b.bdf", "invalid font");
    let o = w.run(&[
        "convert", "a.bdf", "b.bdf", "--each", "--to", "bdf", "-o", "out", "--from", "bdf",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(w.0.join("out/a.bdf")).unwrap(),
        "previous A"
    );
    assert_eq!(
        fs::read_to_string(w.0.join("out/b.bdf")).unwrap(),
        "previous B"
    );
}
#[test]
fn convert_each_collisions_and_font_index_fail_before_writes() {
    let w = Work::new();
    w.write("a.bdf", bdf(&[65]));
    fs::create_dir(w.0.join("out")).unwrap();
    assert_eq!(
        w.run(&[
            "convert", "a.bdf", "a.bdf", "--each", "--to", "bdf", "-o", "out"
        ])
        .status
        .code(),
        Some(1)
    );
    assert_eq!(fs::read_dir(w.0.join("out")).unwrap().count(), 0);
    assert_eq!(
        w.run(&["convert", "a.bdf", "--font-index", "1", "-o", "missing.bdf"])
            .status
            .code(),
        Some(1)
    );
    assert!(!w.0.join("missing.bdf").exists());
    let o = w.run(&["convert", "a.bdf", "--font-index", "0", "-o", "chosen.bdf"]);
    success(&o);
}
#[test]
fn info_borrows_properties_and_reports_unicode_block_counts() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65, 66, 0x4e00]));
    let o = w.run(&["info", "input.bdf", "--json"]);
    success(&o);
    let j = json(&o);
    assert_eq!(j["fonts"][0]["distinct_codes"], 3);
    let blocks = j["fonts"][0]["unicode_blocks"].as_array().unwrap();
    assert!(
        blocks
            .iter()
            .any(|b| b["name"] == "Basic Latin" && b["count"] == 2)
    );
    assert!(
        blocks
            .iter()
            .any(|b| b["name"] == "CJK Unified Ideographs" && b["count"] == 1)
    );
    let o = w.run(&["--help"]);
    success(&o);
    assert!(String::from_utf8_lossy(&o.stdout).contains("vectorize"));
}

#[cfg(feature = "bmfont")]
#[test]
fn identity_budget_failure_blocks_an_already_prepared_sidecar_overwrite() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    let o = w.run(&[
        "convert",
        "input.bdf",
        "-o",
        "font.fnt",
        "--to",
        "bmfont",
        "--bmfont-page-width",
        "4",
        "--bmfont-page-height",
        "4",
        "--report",
        "json",
    ]);
    success(&o);
    let j = json(&o);
    let atlas = j["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["path"].as_str().unwrap())
        .find(|p| p.ends_with(".png"))
        .unwrap();
    let original = fs::read(w.0.join(atlas)).unwrap();
    let text = fs::read_to_string(w.0.join("font.fnt")).unwrap();
    let mut descriptor = String::new();
    for line in text.lines() {
        if line.starts_with("page ") {
            continue;
        }
        descriptor.push_str(&line.replace("pages=1", "pages=80"));
        descriptor.push('\n');
    }
    for index in 0..80 {
        w.write(&format!("page-{index}.png"), &original);
        descriptor.push_str(&format!("page id={index} file=\"page-{index}.png\"\n"));
    }
    w.write("many.fnt", descriptor);
    // First prepare an output targeting a page the second task has not reached.
    // The failed remember must latch incomplete protection across the transaction.
    let tasks = format!(
        "{}[[tasks]]\nid='read-many-pages'\nkind='convert'\ninputs=['many.fnt']\nfrom='bmfont'\noutput='read.bdf'\nto='bdf'\n",
        batch_task("prepared", "input.bdf", "page-79.png")
    );
    w.write(
        "jobs.toml",
        format!("version=1\njobs=1\nmax_pending_bytes=4096\n{tasks}"),
    );
    let o = w.run(&["batch", "jobs.toml"]);
    assert_eq!(o.status.code(), Some(1));
    let j = json(&o);
    assert_eq!(j["summary"]["committed"], 0);
    assert_eq!(fs::read(w.0.join("page-79.png")).unwrap(), original);
    assert!(!w.0.join("read.bdf").exists());
    assert!(j["tasks"].as_array().unwrap().iter().any(|task| {
        task["error"]
            .as_str()
            .is_some_and(|e| e.contains("identity") || e.contains("protection"))
    }));
}

#[cfg(feature = "opentype")]
#[test]
fn structural_validation_enforces_explicit_container() {
    let w = Work::new();
    let _ = sfnt_fixture(&w);
    for format in ["otf", "ttc", "woff", "woff2"] {
        let o = w.run(&["validate", "font.ttf", "--from", format, "--json"]);
        assert_eq!(o.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&o.stderr).contains("explicit --from"));
    }
    success(&w.run(&["validate", "font.ttf", "--from", "ttf", "--json"]));
}
#[cfg(not(feature = "opentype"))]
#[test]
fn structural_validation_does_not_enable_disabled_readers() {
    let w = Work::new();
    w.write("font.bdf", bdf(&[65]));
    let o = w.run(&["validate", "font.bdf", "--from", "ttf", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("disabled"));
}

#[cfg(feature = "opentype")]
#[test]
fn opentype_commands_have_help_examples() {
    let w = Work::new();
    for command in ["variants", "subset"] {
        let o = w.run(&[command, "--help"]);
        success(&o);
        assert!(String::from_utf8_lossy(&o.stdout).contains("Example"));
    }
}

#[cfg(any(feature = "otb", feature = "bdat"))]
#[test]
fn embedded_controls_match_public_writer_and_batch_and_preserve_defaults() {
    use fontpixel::formats::embedded::{
        EmbeddedReferenceStrike, EmbeddedRounding, EmbeddedWriteOptions,
    };
    let w = Work::new();
    let small = bdf(&[65]);
    let large = small
        .replace("-normal--2-20-75-75-", "-normal--4-40-75-75-")
        .replace("SIZE 2 75 75", "SIZE 4 75 75")
        .replace("FONTBOUNDINGBOX 2 2 0 0", "FONTBOUNDINGBOX 2 4 0 0")
        .replace("FONT_ASCENT 2", "FONT_ASCENT 4")
        .replace("BBX 2 2 0 0", "BBX 2 4 0 0")
        .replace("BITMAP\n80\nC0", "BITMAP\n80\nC0\n80\nC0");
    w.write("small.bdf", &small);
    w.write("large.bdf", &large);
    let mut fonts = fontpixel::FontSet::default();
    let mut registry = fontpixel::registry();
    for (name, data) in [("small.bdf", &small), ("large.bdf", &large)] {
        let loaded = registry
            .read(
                &fontpixel::Input::new(data.as_bytes()).with_name(name),
                &Default::default(),
            )
            .unwrap();
        fonts.fonts.extend(loaded.fonts.fonts);
    }
    assert_eq!(fonts.fonts.len(), 2);
    assert_eq!(fonts.fonts[0].metrics.pixel_size, Some(2));
    assert_eq!(fonts.fonts[1].metrics.pixel_size, Some(4));
    registry.set_embedded_write_options(EmbeddedWriteOptions {
        units_per_em: Some(1000),
        reference_strike: EmbeddedReferenceStrike::LargestPpem,
        rounding: EmbeddedRounding::NearestTiesEven,
        ..Default::default()
    });
    let formats: &[&str] = &[
        #[cfg(feature = "otb")]
        "otb",
        #[cfg(feature = "bdat")]
        "bdat",
    ];
    for id in formats {
        let output = format!("explicit.{id}");
        let expected = registry
            .write(
                &fonts,
                id,
                &fontpixel::WriteOptions::new(fontpixel::ResourcePath::new(&output).unwrap()),
            )
            .unwrap();
        success(&w.run(&[
            "convert",
            "small.bdf",
            "large.bdf",
            "--to",
            id,
            "-o",
            &output,
            "--otb-upm",
            "1000",
            "--otb-reference",
            "largest",
            "--otb-rounding",
            "ties-even",
        ]));
        let actual = fs::read(w.0.join(&output)).unwrap();
        assert_eq!(actual, expected.output.into_single().unwrap());
        let font = fontpixel::sfnt::Sfnt::parse(&actual, &Default::default()).unwrap();
        assert_eq!(
            &font.table(*b"head").unwrap()[18..20],
            &1000u16.to_be_bytes()
        );
        let batch_output = format!("batch.{id}");
        w.write("jobs.toml", format!("version=1\n[[tasks]]\nid='embedded'\nkind='convert'\ninputs=['small.bdf','large.bdf']\nfrom='bdf'\nto='{id}'\noutput='{batch_output}'\n[tasks.options]\notb_upm=1000\notb_reference='largest'\notb_rounding='ties-even'\n"));
        success(&w.run(&["batch", "jobs.toml"]));
        assert_eq!(fs::read(w.0.join(batch_output)).unwrap(), actual);
        let implicit = format!("implicit.{id}");
        let explicit = format!("defaults.{id}");
        success(&w.run(&[
            "convert",
            "small.bdf",
            "large.bdf",
            "--to",
            id,
            "-o",
            &implicit,
        ]));
        success(&w.run(&[
            "convert",
            "small.bdf",
            "large.bdf",
            "--to",
            id,
            "-o",
            &explicit,
            "--otb-reference",
            "first",
            "--otb-rounding",
            "half-away",
        ]));
        assert_eq!(
            fs::read(w.0.join(implicit)).unwrap(),
            fs::read(w.0.join(explicit)).unwrap()
        );
        // Index refers to the input FontSet, independent of physical strike sorting.
        let indexed = format!("indexed.{id}");
        success(&w.run(&[
            "convert",
            "small.bdf",
            "large.bdf",
            "--to",
            id,
            "-o",
            &indexed,
            "--otb-upm",
            "1000",
            "--otb-reference",
            "1",
            "--otb-rounding",
            "ties-even",
        ]));
        assert_eq!(fs::read(w.0.join(indexed)).unwrap(), actual);
        w.write(&output, "existing output");
        assert_eq!(
            w.run(&[
                "convert",
                "small.bdf",
                "--to",
                id,
                "-o",
                &output,
                "--otb-reference",
                "1"
            ])
            .status
            .code(),
            Some(1)
        );
        assert_eq!(
            fs::read_to_string(w.0.join(output)).unwrap(),
            "existing output"
        );
    }
}

#[test]
fn embedded_controls_reject_invalid_values_and_wrong_output_without_writes() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    w.write("out.bdf", "existing output");
    for (flag, value) in [
        ("--otb-upm", "15"),
        ("--otb-upm", "16385"),
        ("--otb-reference", "-1"),
        ("--otb-reference", "+1"),
        ("--otb-reference", "largest-ish"),
        ("--otb-rounding", "floor"),
    ] {
        assert_eq!(
            w.run(&[
                "convert",
                "input.bdf",
                "--to",
                "bdf",
                "-o",
                "out.bdf",
                flag,
                value
            ])
            .status
            .code(),
            Some(2)
        );
    }
    for (flag, value) in [
        ("--otb-upm", "1000"),
        ("--otb-reference", "largest"),
        ("--otb-rounding", "ties-even"),
    ] {
        let output = w.run(&[
            "convert",
            "input.bdf",
            "--to",
            "bdf",
            "-o",
            "out.bdf",
            flag,
            value,
        ]);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("require otb or bdat"));
    }
    assert_eq!(
        fs::read_to_string(w.0.join("out.bdf")).unwrap(),
        "existing output"
    );
}

#[cfg(feature = "color")]
#[test]
fn embedded_controls_do_not_silently_change_color_policy() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    let output = w.run(&[
        "convert",
        "input.bdf",
        "--to",
        "color",
        "-o",
        "out.ttf",
        "--otb-upm",
        "1000",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("require otb or bdat"));
    assert!(!w.0.join("out.ttf").exists());
}

#[cfg(not(feature = "otb"))]
#[test]
fn embedded_controls_remain_recognized_when_otb_feature_is_disabled() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65]));
    let output = w.run(&[
        "convert",
        "input.bdf",
        "--to",
        "otb",
        "-o",
        "out.otb",
        "--otb-upm",
        "1000",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("feature"));
    assert!(!w.0.join("out.otb").exists());
}

#[cfg(feature = "otb")]
#[test]
fn charset_strict_loss_precedes_native_writer_limit_and_preserves_existing_output() {
    let w = Work::new();
    w.write("input.bdf", bdf(&[65, 0x1f600]));
    w.write("out.otb", "existing output");
    let args = [
        "charset",
        "input.bdf",
        "--to",
        "cp1252",
        "--format",
        "otb",
        "-o",
        "out.otb",
        "--max-table-bytes",
        "1",
        "--report",
        "json",
    ];
    // A permissive conversion establishes that mapping reaches its loss report
    // and the default native writer can complete. Supplying the existing table
    // allowance explicitly must preserve both output bytes and the full report.
    let cap_index = args
        .iter()
        .position(|arg| *arg == "--max-table-bytes")
        .unwrap();
    let mut defaults = args.to_vec();
    defaults.drain(cap_index..cap_index + 2);
    let baseline = w.run(&defaults);
    success(&baseline);
    let baseline_bytes = fs::read(w.0.join("out.otb")).unwrap();
    let baseline_report = json(&baseline);
    assert_eq!(baseline_report["charset"], "cp1252");
    assert!(
        baseline_report["report"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["code"] == "charset.unmappable_code")
    );
    let default_table_cap = fontpixel::ResourceLimits::default()
        .max_table_bytes
        .to_string();
    let mut explicit_default = args.to_vec();
    explicit_default[cap_index + 1] = &default_table_cap;
    let explicit_default = w.run(&explicit_default);
    success(&explicit_default);
    assert_eq!(fs::read(w.0.join("out.otb")).unwrap(), baseline_bytes);
    assert_eq!(json(&explicit_default), baseline_report);
    w.write("out.otb", "existing output");
    let mut strict_args = args.to_vec();
    strict_args.push("--strict");
    let strict = w.run(&strict_args);
    assert_eq!(
        strict.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&strict.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&strict.stderr).unwrap();
    assert!(
        report["report"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["code"] == "charset.unmappable_code")
    );
    let permissive = w.run(&args);
    assert_eq!(
        permissive.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&permissive.stderr)
    );
    assert!(String::from_utf8_lossy(&permissive.stderr).contains("limit"));
    assert_eq!(
        fs::read_to_string(w.0.join("out.otb")).unwrap(),
        "existing output"
    );
}

#[test]
fn convert_each_keeps_prepared_outputs_and_reports_in_the_next_writer_budget() {
    let w = Work::new();
    let source = bdf(&[65]).replace(
        "COPYRIGHT \"CC0\"",
        &format!("COPYRIGHT \"{}\"", "author notice ".repeat(4096)),
    );
    w.write("a.bdf", &source);
    w.write("b.bdf", &source);
    let succeeds = |limit: usize| {
        w.run(&[
            "convert",
            "a.bdf",
            "-o",
            "single.bdf",
            "--to",
            "bdf",
            "--max-total-bytes",
            &limit.to_string(),
        ])
        .status
        .success()
    };
    let mut low = 0;
    let mut high = 16 * 1024 * 1024;
    assert!(succeeds(high));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if succeeds(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    assert!(succeeds(high));
    assert!(!succeeds(high - 1));
    fs::create_dir(w.0.join("out")).unwrap();
    w.write("out/a.bdf", "existing first output");
    w.write("out/b.bdf", "existing second output");
    let failed = w.run(&[
        "convert",
        "a.bdf",
        "b.bdf",
        "--each",
        "--to",
        "bdf",
        "-o",
        "out",
        "--max-total-bytes",
        &high.to_string(),
    ]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("limit"));
    assert_eq!(
        fs::read_to_string(w.0.join("out/a.bdf")).unwrap(),
        "existing first output"
    );
    assert_eq!(
        fs::read_to_string(w.0.join("out/b.bdf")).unwrap(),
        "existing second output"
    );
    assert_eq!(fs::read_to_string(w.0.join("a.bdf")).unwrap(), source);
}

/// `convert` moves the model into the outline writer, which charges it on the
/// same allowance and drops it before encoding. The smallest accepted total
/// still produces the default bytes, one byte less is a clean limit failure,
/// and in `--each` the earlier output keeps reducing the next group's budget.
#[cfg(feature = "opentype")]
#[test]
fn convert_moves_the_model_into_owned_outline_writers_with_an_exact_budget() {
    let w = Work::new();
    let source = bdf(&[65, 66, 67, 0x4e00]);
    w.write("a.bdf", &source);
    w.write("b.bdf", &source);
    for (ext, extra) in [
        ("woff2", &[][..]),
        ("woff", &[]),
        ("ttf", &["--shape", "dot"]),
    ] {
        let output = format!("single.{ext}");
        let run = |limit: Option<usize>| {
            let mut args = vec!["convert", "a.bdf", "-o", &output];
            args.extend_from_slice(extra);
            let limit = limit.map(|l| l.to_string());
            if let Some(limit) = &limit {
                args.extend(["--max-total-bytes", limit]);
            }
            let _ = fs::remove_file(w.0.join(&output));
            w.run(&args)
        };
        let expected = run(None);
        success(&expected);
        let expected = fs::read(w.0.join(&output)).unwrap();
        let mut low = 0;
        let mut high = 256 * 1024 * 1024;
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if run(Some(middle)).status.success() {
                high = middle;
            } else {
                low = middle;
            }
        }
        success(&run(Some(high)));
        assert_eq!(fs::read(w.0.join(&output)).unwrap(), expected, "{ext}");
        let short = run(Some(high - 1));
        assert_eq!(short.status.code(), Some(1), "{ext}");
        assert!(String::from_utf8_lossy(&short.stderr).contains("limit"));
        assert!(!w.0.join(&output).exists());
        let dir = format!("out-{ext}");
        fs::create_dir(w.0.join(&dir)).unwrap();
        w.write(&format!("{dir}/a.{ext}"), "existing first output");
        let limit = high.to_string();
        let mut args = vec![
            "convert", "a.bdf", "b.bdf", "--each", "--to", ext, "-o", &dir,
        ];
        args.extend_from_slice(extra);
        args.extend(["--max-total-bytes", &limit]);
        let failed = w.run(&args);
        assert_eq!(failed.status.code(), Some(1), "{ext}");
        assert!(String::from_utf8_lossy(&failed.stderr).contains("limit"));
        assert_eq!(
            fs::read_to_string(w.0.join(format!("{dir}/a.{ext}"))).unwrap(),
            "existing first output"
        );
        assert!(!w.0.join(format!("{dir}/b.{ext}")).exists());
    }
}
