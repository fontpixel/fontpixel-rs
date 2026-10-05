//! Explicit scalable-advance projection through `convert` and `batch`.
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../local-work/w6.1-tests");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "projection-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, data: impl AsRef<[u8]>) {
        fs::write(self.0.join(path), data).unwrap();
    }
    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap()
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
fn success(o: &Output) {
    assert!(
        o.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}

/// 13px font: one encoded glyph with SWIDTH 500, one unencoded without SWIDTH,
/// plus a copyright COMMENT that must survive the projection.
const SOURCE: &str = "STARTFONT 2.1\nCOMMENT Copyright example, CC0\nFONT -test-proj-medium-r-normal--13-120-75-75-c-60-iso10646-1\nSIZE 13 75 75\nFONTBOUNDINGBOX 6 13 0 -3\nSTARTPROPERTIES 2\nFONT_ASCENT 10\nFONT_DESCENT 3\nENDPROPERTIES\nCHARS 2\nSTARTCHAR A\nENCODING 65\nSWIDTH 500 0\nDWIDTH 6 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nSTARTCHAR extra\nENCODING -1\nDWIDTH 12 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n";

fn swidths(bdf: &str) -> Vec<&str> {
    bdf.lines().filter(|l| l.starts_with("SWIDTH ")).collect()
}

#[test]
fn convert_projects_every_glyph_and_reports_loss() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    let o = w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "out.bdf",
        "--swidth-ppem",
        "13",
        "--report",
        "json",
    ]);
    success(&o);
    let out = w.read("out.bdf");
    assert_eq!(swidths(&out), ["SWIDTH 462 0", "SWIDTH 923 0"]);
    assert!(
        out.lines()
            .any(|l| l.starts_with("COMMENT") && l.contains("Copyright example, CC0"))
    );
    let report = String::from_utf8_lossy(&o.stdout);
    assert!(report.contains("projection.scalable_advance.replaced"));
    assert!(report.contains("projection.scalable_advance.derived"));
}

#[test]
fn convert_without_projection_keeps_source_values() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    success(&w.run(&["convert", "in.bdf", "-o", "out.bdf"]));
    assert_eq!(swidths(&w.read("out.bdf"))[0], "SWIDTH 500 0");
}

#[test]
fn strict_rejects_replaced_values_and_keeps_old_output() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    w.write("out.bdf", "old");
    let o = w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "out.bdf",
        "--swidth-ppem",
        "13",
        "--strict",
    ]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(w.read("out.bdf"), "old");
}

#[test]
fn units_and_ppem_are_validated() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    for args in [
        &["--swidth-ppem", "0"][..],
        &["--swidth-units", "1000"][..],
        &["--swidth-ppem", "13", "--swidth-units", "0"][..],
    ] {
        let mut all = vec!["convert", "in.bdf", "-o", "out.bdf"];
        all.extend_from_slice(args);
        assert!(!w.run(&all).status.success(), "{args:?}");
    }
    assert!(!w.0.join("out.bdf").exists());
    let o = w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "out.bdf",
        "--swidth-ppem",
        "13",
        "--swidth-units",
        "2048",
    ]);
    success(&o);
    // 6 * 2048 / 13 = 945.23 -> 945; 12 * 2048 / 13 = 1890.46 -> 1890.
    assert_eq!(
        swidths(&w.read("out.bdf")),
        ["SWIDTH 945 0", "SWIDTH 1890 0"]
    );
}

#[test]
fn batch_options_use_the_same_projection() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    w.write(
        "jobs.toml",
        "version=1\n[[tasks]]\nid='p'\nkind='convert'\ninputs=['in.bdf']\noutput='out.bdf'\n[tasks.options]\nswidth_ppem=13\n",
    );
    success(&w.run(&["batch", "jobs.toml"]));
    assert_eq!(
        swidths(&w.read("out.bdf")),
        ["SWIDTH 462 0", "SWIDTH 923 0"]
    );
}
