//! Unit F: `convert --notdef`, `variants` metadata flags (sharing the
//! `--otb-name-record` syntax) and the new `--otb-*` bitmap TTF flags, on the
//! command line and in batch files.
#![cfg(all(feature = "bdf", feature = "opentype", feature = "otb"))]
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
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("strict-f-tests");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "f-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, data: impl AsRef<[u8]>) {
        fs::write(self.0.join(path), data).unwrap();
    }
    fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.0.join(path)).unwrap()
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
fn failure(o: &Output) -> String {
    assert!(
        !o.status.success(),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}
fn table<'a>(font: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let count = usize::from(u16::from_be_bytes([font[4], font[5]]));
    font[12..12 + 16 * count]
        .as_chunks::<16>()
        .0
        .iter()
        .find(|record| &record[..4] == tag)
        .map(|record| {
            let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
            let length = u32::from_be_bytes(record[12..16].try_into().unwrap()) as usize;
            &font[offset..offset + length]
        })
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

/// 8 px font with a source `.notdef`, two encoded glyphs (the second has a
/// supplementary code point) and a copyright.
const SOURCE: &str = "STARTFONT 2.1\nFONT -test-f-medium-r-normal--8-80-75-75-c-40-iso10646-1\nSIZE 8 75 75\nFONTBOUNDINGBOX 4 8 0 -2\nSTARTPROPERTIES 4\nFONT_ASCENT 6\nFONT_DESCENT 2\nCOPYRIGHT \"Source notice\"\nSPACING \"C\"\nENDPROPERTIES\nCHARS 3\nSTARTCHAR .notdef\nENCODING -1\nDWIDTH 4 0\nBBX 2 2 0 0\nBITMAP\nC0\nC0\nENDCHAR\nSTARTCHAR B\nENCODING 66\nDWIDTH 4 0\nBBX 1 1 1 0\nBITMAP\n80\nENDCHAR\nSTARTCHAR A\nENCODING 65\nDWIDTH 4 0\nBBX 2 1 1 0\nBITMAP\nC0\nENDCHAR\nENDFONT\n";

#[test]
fn convert_notdef_policy_reaches_ttf_and_reports_replacement() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    success(&w.run(&["convert", "in.bdf", "-o", "native.ttf"]));
    success(&w.run(&[
        "convert", "in.bdf", "-o", "kept.ttf", "--notdef", "preserve",
    ]));
    assert_eq!(w.bytes("native.ttf"), w.bytes("kept.ttf"));
    let o = w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "empty.ttf",
        "--notdef",
        "empty",
        "--notdef-advance",
        "400",
        "--notdef-lsb",
        "-5",
        "--report",
        "json",
    ]);
    success(&o);
    assert!(String::from_utf8_lossy(&o.stdout).contains("variants.notdef.replaced"));
    let hmtx = |font: &[u8]| {
        let t = table(font, b"hmtx").unwrap();
        (u16_at(t, 0), i16::from_be_bytes([t[2], t[3]]))
    };
    assert_eq!(hmtx(&w.bytes("native.ttf")), (400, 0));
    assert_eq!(hmtx(&w.bytes("empty.ttf")), (400, -5));
    // The source .notdef is replaced: strict rejects the loss.
    failure(&w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "strict.ttf",
        "--notdef",
        "empty",
        "--notdef-advance",
        "400",
        "--strict",
    ]));
    assert!(!w.0.join("strict.ttf").exists());
    for bad in [
        &["-o", "bad.ttf", "--notdef", "empty"][..],
        &["-o", "bad.ttf", "--notdef-advance", "3"][..],
        &["-o", "bad.bdf", "--notdef", "preserve"][..],
        &["-o", "bad.svg", "--notdef", "preserve"][..],
    ] {
        failure(&w.run(&[&["convert", "in.bdf"][..], bad].concat()));
    }
    w.write(
        "jobs.toml",
        "version=1\n[[tasks]]\nid='t'\nkind='convert'\ninputs=['in.bdf']\noutput='batch.ttf'\n[tasks.options]\nnotdef='empty'\nnotdef_advance=400\nnotdef_lsb=-5\n",
    );
    success(&w.run(&["batch", "jobs.toml"]));
    assert_eq!(w.bytes("batch.ttf"), w.bytes("empty.ttf"));
}

const VARIANT_FLAGS: &[&str] = &[
    "--name-record",
    "3/1/0x409/0=Copy \u{2014} right",
    "--name-record",
    "1/0/0/0=Copy \u{2014} right",
    "--name-record",
    "3/1/0x409/1=Face",
    "--name-record-hex",
    "1/0/0/1=46616365",
    "--fs-type",
    "4",
    "--fs-selection",
    "0",
    "--break-char",
    "32",
    "--code-page-ranges",
    "0,0",
    "--created",
    "3660681600",
    "--modified",
    "3660681600",
    "--fixed-pitch",
    "false",
    "--max-zones",
    "2",
    "--gasp",
    "omit",
    "--cmap-unicode-full",
    "when-needed",
    "--glyph-order",
    "codepoint",
    "--dot-glyph",
    "after-notdef",
];

#[test]
fn variants_metadata_flags_reach_every_table_and_match_batch() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    let base = [
        "variants",
        "--base",
        "in.bdf",
        "--shape",
        "dot",
        "--dot-reuse",
        "literal",
    ];
    success(&w.run(&[&base[..], &["-o", "native.ttf"]].concat()));
    success(
        &w.run(
            &[
                &base[..],
                &[
                    "-o",
                    "same.ttf",
                    "--gasp",
                    "keep",
                    "--cmap-unicode-full",
                    "always",
                    "--glyph-order",
                    "planned",
                    "--dot-glyph",
                    "last",
                ],
            ]
            .concat(),
        ),
    );
    assert_eq!(w.bytes("native.ttf"), w.bytes("same.ttf"));
    let o = w.run(
        &[
            &base[..],
            &["-o", "app.ttf", "--report", "json"],
            VARIANT_FLAGS,
        ]
        .concat(),
    );
    success(&o);
    // The base's COPYRIGHT is not in the exact table: reported loss.
    let report = String::from_utf8_lossy(&o.stdout);
    assert!(report.contains("variants.metadata_name"), "{report}");
    assert!(
        report.contains("variants.metadata_configuration"),
        "{report}"
    );
    let font = w.bytes("app.ttf");
    let head = table(&font, b"head").unwrap();
    assert_eq!(
        i64::from_be_bytes(head[20..28].try_into().unwrap()),
        3_660_681_600
    );
    let os2 = table(&font, b"OS/2").unwrap();
    assert_eq!((u16_at(os2, 8), u16_at(os2, 62)), (4, 0));
    assert_eq!(&os2[78..86], &[0; 8], "code pages");
    assert_eq!(u16_at(os2, 92), 32, "usBreakChar");
    assert_eq!(&table(&font, b"post").unwrap()[12..16], &[0; 4]);
    assert_eq!(u16_at(table(&font, b"maxp").unwrap(), 14), 2);
    assert!(table(&font, b"gasp").is_none());
    assert_eq!(u16_at(table(&font, b"cmap").unwrap(), 2), 2);
    let name = table(&font, b"name").unwrap();
    assert_eq!(u16_at(name, 2), 4, "exactly the given records");
    // Strict rejects the dropped source notice before writing.
    failure(&w.run(&[&base[..], &["-o", "strict.ttf", "--strict"], VARIANT_FLAGS].concat()));
    let mut options = String::new();
    for pair in VARIANT_FLAGS.chunks(2) {
        let key = pair[0].trim_start_matches("--").replace('-', "_");
        if key.starts_with("name_record") {
            continue;
        }
        options.push_str(&format!("{key}={:?}\n", pair[1]));
    }
    w.write(
        "jobs.toml",
        format!(
            "version=1\n[[tasks]]\nid='v'\nkind='variants'\nbase='in.bdf'\noutput='batch.ttf'\n[tasks.options]\nshape='dot'\ndot_reuse='literal'\nname_record=['3/1/0x409/0=Copy \u{2014} right','1/0/0/0=Copy \u{2014} right','3/1/0x409/1=Face']\nname_record_hex=['1/0/0/1=46616365']\n{options}"
        ),
    );
    success(&w.run(&["batch", "jobs.toml"]));
    assert_eq!(w.bytes("batch.ttf"), font);
}

#[test]
fn variants_metadata_flags_are_validated() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    let base = ["variants", "--base", "in.bdf", "-o", "bad.ttf"];
    for (extra, message) in [
        (
            &["--dot-glyph", "after-notdef"][..],
            "TrueType dot outlines",
        ),
        (&["--fs-type", "1"][..], "reserved bit"),
        (&["--fs-selection", "0x80"][..], "require version 4"),
        (&["--code-page-ranges", "0x200,0"][..], "reserved bit"),
        (&["--name-record", "3/2/0/1=X"][..], "--name-record-hex"),
        (&["--name-record", "3/1/0x409=X"][..], "NAME_ID"),
        (&["--created", "1"][..], "modified"),
        (
            &["--max-zones", "2", "--to", "otf"][..],
            "TrueType outlines",
        ),
    ] {
        let message_seen = failure(&w.run(&[&base[..], extra].concat()));
        assert!(message_seen.contains(message), "{extra:?}: {message_seen}");
    }
}

#[test]
fn otb_bitmap_ttf_flags_reach_their_tables_and_are_rejected_elsewhere() {
    let w = Work::new();
    w.write("in.bdf", SOURCE);
    success(&w.run(&["convert", "in.bdf", "-o", "native.otb"]));
    success(&w.run(&[
        "convert",
        "in.bdf",
        "-o",
        "same.otb",
        "--otb-outline-tables",
        "none",
        "--otb-notdef",
        "preserve",
        "--otb-line-extremes",
        "computed",
        "--otb-vertical-line-metrics",
        "zero",
        "--otb-hmtx",
        "full",
        "--otb-glyph-order",
        "first-seen",
        "--otb-index-layout",
        "canonical",
        "--otb-cmap-unicode-full",
        "always",
    ]));
    assert_eq!(w.bytes("native.otb"), w.bytes("same.otb"));
    let flags = [
        "--otb-outline-tables",
        "empty-glyf",
        "--otb-max-zones",
        "2",
        "--otb-notdef",
        "empty",
        "--otb-notdef-advance",
        "400",
        "--otb-caret-slope",
        "0,1",
        "--otb-line-extremes",
        "zero",
        "--otb-vertical-line-metrics",
        "mirror-horizontal",
        "--otb-underline",
        "0,0",
        "--otb-hmtx",
        "compact-trailing",
        "--otb-glyph-order",
        "codepoint",
        "--otb-index-layout",
        "spanning",
        "--otb-cmap-unicode-full",
        "when-needed",
    ];
    let o = w.run(
        &[
            &[
                "convert", "in.bdf", "-o", "app.ttf", "--to", "otb", "--report", "json",
            ][..],
            &flags,
        ]
        .concat(),
    );
    success(&o);
    let report = String::from_utf8_lossy(&o.stdout);
    assert!(report.contains("embedded.notdef_replaced"), "{report}");
    let font = w.bytes("app.ttf");
    assert_eq!(table(&font, b"glyf").unwrap(), [0]);
    let maxp = table(&font, b"maxp").unwrap();
    assert_eq!((maxp.len(), u16_at(maxp, 14)), (32, 2));
    let n = usize::from(u16_at(maxp, 4));
    assert_eq!(table(&font, b"loca").unwrap(), vec![0; 2 * (n + 1)]);
    assert_eq!(
        u16_at(table(&font, b"hhea").unwrap(), 34),
        1,
        "compact hmtx"
    );
    let eblc = table(&font, b"EBLC").unwrap();
    // Strike 0: caret 0/1 and a vertical copy of the horizontal record.
    assert_eq!((eblc[8 + 16 + 3], eblc[8 + 16 + 4]), (0, 1));
    assert_eq!(eblc[8 + 16..8 + 28], eblc[8 + 28..8 + 40]);
    assert_eq!(u16_at(eblc, 8 + 40), 1, "glyph 0 has no bitmap");
    failure(
        &w.run(
            &[
                &["convert", "in.bdf", "-o", "strict.otb", "--strict"][..],
                &flags,
            ]
            .concat(),
        ),
    );
    for bad in [
        &["-o", "bad.bdf", "--otb-hmtx", "compact-trailing"][..],
        &["-o", "bad.otb", "--otb-max-zones", "2"][..],
        &["-o", "bad.otb", "--otb-notdef", "empty"][..],
        &["-o", "bad.otb", "--otb-notdef-advance", "5"][..],
        &["-o", "bad.otb", "--otb-caret-slope", "0,0"][..],
        &["-o", "bad.otb", "--otb-underline", "1"][..],
    ] {
        failure(&w.run(&[&["convert", "in.bdf"][..], bad].concat()));
    }
    let mut options = String::new();
    for pair in flags.chunks(2) {
        let key = pair[0].trim_start_matches("--").replace('-', "_");
        options.push_str(&format!("{key}={:?}\n", pair[1]));
    }
    w.write(
        "jobs.toml",
        format!(
            "version=1\n[[tasks]]\nid='o'\nkind='convert'\ninputs=['in.bdf']\nto='otb'\noutput='batch.ttf'\n[tasks.options]\n{options}"
        ),
    );
    success(&w.run(&["batch", "jobs.toml"]));
    assert_eq!(w.bytes("batch.ttf"), font);
}
