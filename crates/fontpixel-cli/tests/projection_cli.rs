//! Explicit model projections and writer/variants application options
//! through `convert`, `variants` and `batch`.
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

/// Cozette-like source: SIZE 12 with POINT_SIZE 120 and 72 dpi properties.
const METADATA: &str = "STARTFONT 2.1\nFONT -test-meta-medium-r-normal--13-120-72-72-c-60-iso10646-1\nSIZE 12 72 72\nFONTBOUNDINGBOX 6 13 0 -3\nCOMMENT \"(c) 2026 Example notice\"\nSTARTPROPERTIES 6\nFONT \"-test-meta-medium-r-normal--13-120-72-72-c-60-iso10646-1\"\nPOINT_SIZE 120\nRESOLUTION_X 72\nRESOLUTION_Y 72\nFONT_ASCENT 10\nFONT_DESCENT 3\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nSWIDTH 500 0\nDWIDTH 6 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n";
const APP_NAME: &str = "-opf-meta-medium-r-normal--13-130-75-75-p-130-iso10646-1";

fn args<'a>(base: &[&'a str], extra: &[&'a str]) -> Vec<&'a str> {
    base.iter().chain(extra).copied().collect()
}

#[test]
fn convert_projects_name_size_and_properties_with_full_report() {
    let w = Work::new();
    w.write("in.bdf", METADATA);
    let name = format!("--font-name={APP_NAME}");
    let o = w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "out.bdf",
        &name,
        "--point-size",
        "13",
        "--resolution",
        "75,75",
        "--set-integer-property",
        "PIXEL_SIZE=13",
        "--set-text-property",
        "WEIGHT_NAME=Medium",
        "--remove-property",
        "FONT",
        "--report",
        "json",
    ]);
    success(&o);
    let out = w.read("out.bdf");
    assert!(out.contains(&format!("\nFONT {APP_NAME}\n")));
    assert!(out.contains("\nSIZE 13 75 75\n"));
    assert!(out.contains("COMMENT \"(c) 2026 Example notice\""));
    assert!(out.contains("\nPOINT_SIZE 120\n"));
    assert!(out.contains("\nRESOLUTION_X 72\n"));
    assert!(out.contains("\nPIXEL_SIZE 13\n"));
    assert!(out.contains("\nWEIGHT_NAME \"Medium\"\n"));
    assert!(!out.contains("\nFONT \""));
    let report = String::from_utf8_lossy(&o.stdout);
    for code in [
        "projection.font_name.replaced",
        "projection.font_name.property_synced",
        "projection.point_size.replaced",
        "projection.resolution.replaced",
        "projection.property.added",
        "projection.property.removed",
    ] {
        assert!(report.contains(code), "{code}: {report}");
    }
    // The same projection under --strict is rejected and writes nothing.
    let o = w.run(&["convert", "in.bdf", "-o", "strict.bdf", &name, "--strict"]);
    assert_eq!(o.status.code(), Some(3));
    assert!(!w.0.join("strict.bdf").exists());
}

#[test]
fn projections_equal_to_the_source_leave_output_byte_identical() {
    let w = Work::new();
    w.write("in.bdf", METADATA);
    for (output, to) in [("plain.bdf", "bdf"), ("plain.pcf", "pcf")] {
        success(&w.run(&["convert", "in.bdf", "-o", output, "--to", to]));
    }
    let name = "--font-name=-test-meta-medium-r-normal--13-120-72-72-c-60-iso10646-1";
    let same = [
        name,
        "--point-size",
        "12",
        "--resolution",
        "72,72",
        "--set-integer-property",
        "FONT_ASCENT=10",
    ];
    success(&w.run(&args(
        &["convert", "in.bdf", "-o", "same.bdf", "--to", "bdf"],
        &same,
    )));
    success(
        &w.run(&args(
            &["convert", "in.bdf", "-o", "same.pcf", "--to", "pcf"],
            &[
                &same[..],
                &["--pcf-metadata-precedence", "explicit-properties"],
            ]
            .concat(),
        )),
    );
    success(&w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "default.pcf",
        "--pcf-metadata-precedence",
        "normalized-edits",
    ]));
    let bytes = |path: &str| fs::read(w.0.join(path)).unwrap();
    assert_eq!(bytes("plain.bdf"), bytes("same.bdf"));
    assert_eq!(bytes("plain.pcf"), bytes("same.pcf"));
    assert_eq!(bytes("plain.pcf"), bytes("default.pcf"));
}

#[test]
fn pcf_precedence_is_explicit_and_reports_kept_properties() {
    let w = Work::new();
    w.write("in.bdf", METADATA);
    let size = ["--point-size", "13", "--resolution", "75,75"];
    let o = w.run(&args(
        &[
            "convert",
            "in.bdf",
            "-o",
            "explicit.pcf",
            "--report",
            "json",
        ],
        &[
            &size[..],
            &["--pcf-metadata-precedence", "explicit-properties"],
        ]
        .concat(),
    ));
    success(&o);
    let report = String::from_utf8_lossy(&o.stdout);
    assert_eq!(report.matches("pcf.write.property_precedence").count(), 3);
    let o = w.run(&args(
        &[
            "convert",
            "in.bdf",
            "-o",
            "normalized.pcf",
            "--report",
            "json",
        ],
        &size,
    ));
    success(&o);
    assert!(!String::from_utf8_lossy(&o.stdout).contains("property_precedence"));
    assert_ne!(
        fs::read(w.0.join("explicit.pcf")).unwrap(),
        fs::read(w.0.join("normalized.pcf")).unwrap()
    );
    // The precedence option is PCF-only.
    let o = w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "x.bdf",
        "--pcf-metadata-precedence",
        "explicit-properties",
    ]);
    assert!(!o.status.success());
}

#[test]
fn unsafe_or_incomplete_metadata_edits_are_rejected() {
    let w = Work::new();
    w.write("in.bdf", METADATA);
    for extra in [
        &["--remove-property", "COMMENT"][..],
        &["--remove-property", "COPYRIGHT"][..],
        &["--set-text-property", "NOTICE=x"][..],
        &["--set-text-property", "FAMILY_NAME=x"][..],
        &["--set-text-property", "FONT=x"][..],
        &["--set-integer-property", "PIXEL_SIZE=x"][..],
        &["--set-integer-property", "=1"][..],
        &["--point-size", "13"][..],
        &["--resolution", "75,75"][..],
        &["--point-size", "13", "--resolution", "75"][..],
        &["--font-name", " "][..],
    ] {
        let o = w.run(&args(&["convert", "in.bdf", "-o", "out.bdf"], extra));
        assert!(!o.status.success(), "{extra:?}");
    }
    assert!(!w.0.join("out.bdf").exists());
}

#[test]
fn batch_options_cover_every_metadata_projection() {
    let w = Work::new();
    w.write("in.bdf", METADATA);
    w.write(
        "jobs.toml",
        format!(
            "version=1\n[[tasks]]\nid='b'\nkind='convert'\ninputs=['in.bdf']\noutput='out.bdf'\n[tasks.options]\nswidth_ppem=13\nfont_name='{APP_NAME}'\npoint_size=13\nresolution='75,75'\nset_integer_property=['PIXEL_SIZE=13','FONT_ASCENT=10']\nremove_property=['FONT']\n\
[[tasks]]\nid='p'\nkind='convert'\ninputs=['in.bdf']\noutput='out.pcf'\n[tasks.options]\npoint_size=13\nresolution='75,75'\npcf_metadata_precedence='explicit-properties'\n"
        ),
    );
    success(&w.run(&["batch", "jobs.toml"]));
    let out = w.read("out.bdf");
    assert!(out.contains(&format!("\nFONT {APP_NAME}\n")));
    assert!(out.contains("\nSIZE 13 75 75\n"));
    assert!(out.contains("\nPIXEL_SIZE 13\n"));
    assert_eq!(swidths(&out), ["SWIDTH 462 0"]);
    assert!(w.0.join("out.pcf").exists());
}

#[cfg(feature = "opentype")]
fn table<'a>(font: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
    let count = usize::from(u16::from_be_bytes([font[4], font[5]]));
    for record in font[12..12 + 16 * count].as_chunks::<16>().0 {
        if &record[..4] == tag {
            let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
            let length = u32::from_be_bytes(record[12..16].try_into().unwrap()) as usize;
            return &font[offset..offset + length];
        }
    }
    panic!("missing table {tag:?}");
}

#[cfg(feature = "opentype")]
#[test]
fn variants_empty_notdef_has_exact_design_metrics_and_empty_outline() {
    let w = Work::new();
    w.write("in.bdf", METADATA);
    let base = [
        "variants", "--base", "in.bdf", "--ppu", "76", "--upm", "988",
    ];
    success(&w.run(&args(&base, &["-o", "box.ttf"])));
    success(&w.run(&args(
        &base,
        &[
            "-o",
            "empty.ttf",
            "--notdef",
            "empty",
            "--notdef-advance",
            "494",
            "--notdef-lsb",
            "0",
        ],
    )));
    let boxed = fs::read(w.0.join("box.ttf")).unwrap();
    let empty = fs::read(w.0.join("empty.ttf")).unwrap();
    let hmtx = |font: &[u8]| {
        let t = table(font, b"hmtx");
        (
            u16::from_be_bytes([t[0], t[1]]),
            i16::from_be_bytes([t[2], t[3]]),
        )
    };
    // Default: the generated 7-pixel box with a one-pixel bearing.
    assert_eq!(hmtx(&boxed), (7 * 76, 76));
    assert_eq!(hmtx(&empty), (494, 0));
    let long = i16::from_be_bytes(table(&empty, b"head")[50..52].try_into().unwrap()) != 0;
    let loca = table(&empty, b"loca");
    let (first, second) = if long {
        (
            u32::from_be_bytes(loca[0..4].try_into().unwrap()),
            u32::from_be_bytes(loca[4..8].try_into().unwrap()),
        )
    } else {
        (
            u32::from(u16::from_be_bytes([loca[0], loca[1]])),
            u32::from(u16::from_be_bytes([loca[2], loca[3]])),
        )
    };
    assert_eq!(first, second, "glyph 0 has no outline");
    for extra in [&["--notdef", "empty"][..], &["--notdef-advance", "494"][..]] {
        let o = w.run(&args(&base, &[&["-o", "bad.ttf"][..], extra].concat()));
        assert!(!o.status.success(), "{extra:?}");
    }
}
