//! Explicit OTB/bdat metadata flags: CLI, batch and library produce the same
//! bytes; invalid or misplaced flags fail before any output is written.
#![cfg(all(feature = "bdf", any(feature = "otb", feature = "bdat")))]
use fontpixel::formats::embedded::{
    EmbeddedAverageWidth, EmbeddedCmapUnicodeFull, EmbeddedIndexLayout, EmbeddedMetadataOptions,
    EmbeddedPostNames, EmbeddedUnicodeRanges, EmbeddedWidthMax, EmbeddedWriteOptions,
};
use fontpixel::sfnt::name::{NameRecord, NameTable};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "embedded-metadata-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
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
fn glyph(code: i64, advance: i64) -> String {
    format!(
        "STARTCHAR g{code}\nENCODING {code}\nSWIDTH 500 0\nDWIDTH {advance} 0\nBBX 2 2 0 0\nBITMAP\n80\nC0\nENDCHAR\n"
    )
}
fn bdf() -> String {
    let glyphs: String = (65..75)
        .map(|c| glyph(c, 3))
        .chain([glyph(0x1f600, 4)])
        .collect();
    format!(
        "STARTFONT 2.1\nFONT -test-phase6-medium-r-normal--2-20-75-75-p-20-iso10646-1\nSIZE 2 75 75\nFONTBOUNDINGBOX 2 2 0 0\nSTARTPROPERTIES 6\nFONT_ASCENT 2\nFONT_DESCENT 0\nFAMILY_NAME \"Meta\"\nCHARSET_REGISTRY \"ISO10646\"\nCHARSET_ENCODING \"1\"\nCOPYRIGHT \"Source notice\"\nENDPROPERTIES\nCHARS 11\n{glyphs}ENDFONT\n"
    )
}
const FLAGS: &[&str] = &[
    "--otb-name-record",
    "3/1/0x409/1=Meta Family",
    "--otb-name-record",
    "3/1/0x409/16=Meta Family",
    "--otb-name-record",
    "1/0/0/0=© Meta",
    "--otb-name-record-hex",
    "3/2/0x411/2=0041",
    "--otb-name-record",
    "0/4/0x8000/17=Regular",
    "--otb-name-language-tag",
    "en-GB",
    "--otb-head-flags",
    "0x3",
    "--otb-lowest-rec-ppem",
    "14",
    "--otb-created",
    "3873996467",
    "--otb-modified",
    "-5",
    "--otb-os2-version",
    "3",
    "--otb-vendor",
    "TONG",
    "--otb-panose",
    "2,11,5,3,0,0,0,0,0,0",
    "--otb-unicode-ranges",
    "non-bmp",
    "--otb-break-char",
    "32",
    "--otb-fs-type",
    "4",
    "--otb-fs-selection",
    "0",
    "--otb-average-width",
    "floor",
    "--otb-post-names",
    "omit",
    "--otb-fixed-pitch",
    "true",
    "--otb-cmap-unicode-full",
    "non-bmp",
    "--otb-width-max",
    "advance",
    "--otb-index-layout",
    "shared-runs",
    "--otb-index-min-run",
    "4",
];
fn expected_options() -> EmbeddedMetadataOptions {
    let record = |platform_id, encoding_id, language_id, name_id, value: Vec<u8>| NameRecord {
        platform_id,
        encoding_id,
        language_id,
        name_id,
        value,
    };
    EmbeddedMetadataOptions {
        names: Some(Arc::new(NameTable {
            records: vec![
                NameRecord::unicode(1, 0x409, "Meta Family"),
                NameRecord::unicode(16, 0x409, "Meta Family"),
                // Mac Roman: U+00A9 is 0xA9.
                record(1, 0, 0, 0, b"\xa9 Meta".to_vec()),
                record(3, 2, 0x411, 2, vec![0, 0x41]),
                record(
                    0,
                    4,
                    0x8000,
                    17,
                    "Regular"
                        .encode_utf16()
                        .flat_map(u16::to_be_bytes)
                        .collect(),
                ),
            ],
            language_tags: vec!["en-GB".into()],
        })),
        head_flags: Some(3),
        lowest_rec_ppem: Some(14),
        timestamps: Some([3_873_996_467, -5]),
        os2_version: Some(3),
        vendor_id: Some(*b"TONG"),
        panose: Some([2, 11, 5, 3, 0, 0, 0, 0, 0, 0]),
        unicode_ranges: EmbeddedUnicodeRanges::NonBmpOnly,
        break_char: Some(32),
        embedding_flags: Some(4),
        selection_flags: Some(0),
        average_width: EmbeddedAverageWidth::Floor,
        post_names: EmbeddedPostNames::Omit,
        fixed_pitch: Some(true),
        cmap_unicode_full: EmbeddedCmapUnicodeFull::NonBmpOnly,
        width_max: EmbeddedWidthMax::Advance,
        index_layout: EmbeddedIndexLayout::SharedMetricRuns { minimum_run: 4 },
        ..Default::default()
    }
}
fn library(format: &str, metadata: EmbeddedMetadataOptions) -> fontpixel::resources::Written {
    let mut registry = fontpixel::registry();
    let loaded = registry
        .read(
            &fontpixel::Input::new(bdf().as_bytes()).with_name("meta.bdf"),
            &Default::default(),
        )
        .unwrap();
    registry.set_embedded_write_options(EmbeddedWriteOptions {
        metadata,
        ..Default::default()
    });
    registry
        .write(
            &loaded.fonts,
            format,
            &fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("out").unwrap()),
        )
        .unwrap()
}

#[test]
fn cli_batch_and_library_metadata_bytes_agree() {
    let w = Work::new();
    fs::write(w.0.join("meta.bdf"), bdf()).unwrap();
    let formats: &[&str] = &[
        #[cfg(feature = "otb")]
        "otb",
        #[cfg(feature = "bdat")]
        "bdat",
    ];
    for format in formats {
        let expected = library(format, expected_options());
        assert!(
            expected
                .report
                .entries
                .iter()
                .any(|e| e.code == "embedded.metadata_name")
        );
        let output = format!("cli.{format}");
        let mut args = vec!["convert", "meta.bdf", "--to", format, "-o", &output];
        args.extend_from_slice(FLAGS);
        success(&w.run(&args));
        assert_eq!(
            fs::read(w.0.join(&output)).unwrap(),
            expected.output.into_single().unwrap()
        );
        // Batch spells the same flags with underscores; arrays repeat.
        let mut options = String::new();
        let mut records = Vec::new();
        for pair in FLAGS.chunks(2) {
            let key = pair[0].trim_start_matches("--").replace('-', "_");
            if key == "otb_name_record" {
                records.push(format!("{:?}", pair[1]));
            } else {
                options.push_str(&format!("{key} = {:?}\n", pair[1]));
            }
        }
        let batch_output = format!("batch.{format}");
        fs::write(
            w.0.join("jobs.toml"),
            format!(
                "version = 1\n[[tasks]]\nid = 'meta'\nkind = 'convert'\ninputs = ['meta.bdf']\nfrom = 'bdf'\nto = '{format}'\noutput = '{batch_output}'\n[tasks.options]\notb_name_record = [{}]\n{options}",
                records.join(", ")
            ),
        )
        .unwrap();
        success(&w.run(&["batch", "jobs.toml"]));
        assert_eq!(
            fs::read(w.0.join(&batch_output)).unwrap(),
            fs::read(w.0.join(&output)).unwrap()
        );
        fs::remove_file(w.0.join("jobs.toml")).unwrap();
        // No flag: unchanged native default.
        let plain = format!("plain.{format}");
        success(&w.run(&["convert", "meta.bdf", "--to", format, "-o", &plain]));
        assert_eq!(
            fs::read(w.0.join(&plain)).unwrap(),
            library(format, Default::default())
                .output
                .into_single()
                .unwrap()
        );
    }
}

#[test]
fn invalid_or_misplaced_metadata_flags_fail_without_output() {
    let w = Work::new();
    fs::write(w.0.join("meta.bdf"), bdf()).unwrap();
    for extra in [
        &["--otb-vendor", "TOO LONG"][..],
        &["--otb-vendor", "TON"],
        &["--otb-panose", "1,2,3"],
        &["--otb-head-flags", "0x8000"],
        &["--otb-lowest-rec-ppem", "0"],
        &["--otb-created", "1"],
        &["--otb-index-min-run", "4"],
        &["--otb-unicode-ranges", "1,2,3"],
        &["--otb-unicode-ranges", "0,0,0,0x08000000"],
        &["--otb-fs-type", "1"],
        &["--otb-fs-selection", "0x80", "--otb-os2-version", "3"],
        &["--otb-name-record", "3/1/0x409=Missing ID"],
        &["--otb-name-record", "3/1/0x409/1"],
        &["--otb-name-record", "1/0/0/1=永"],
        &["--otb-name-record", "3/3/0x804/1=PRC"],
        &["--otb-name-record", "9/0/0/1=Unknown"],
        &["--otb-name-record", "3/1/0x409/6=Bad Name"],
        &[
            "--otb-name-record",
            "3/1/0x409/1=A",
            "--otb-name-record",
            "3/1/0x409/1=B",
        ],
        &["--otb-name-record-hex", "3/1/0x409/1=0"],
        &["--otb-name-record-hex", "3/1/0x409/1=d800"],
        &["--otb-name-language-tag", "en_GB"],
    ] {
        let mut args = vec!["convert", "meta.bdf", "--to", "otb", "-o", "bad.otb"];
        args.extend_from_slice(extra);
        let output = w.run(&args);
        assert!(!output.status.success(), "{extra:?}");
        assert!(!w.0.join("bad.otb").exists(), "{extra:?}");
    }
    let output = w.run(&[
        "convert",
        "meta.bdf",
        "--to",
        "pcf",
        "-o",
        "bad.pcf",
        "--otb-vendor",
        "TONG",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("require otb or bdat output"));
    assert!(!w.0.join("bad.pcf").exists());
    // The dropped source copyright is a reported loss, absent from the
    // native default report.
    let native = w.run(&[
        "convert",
        "meta.bdf",
        "--to",
        "otb",
        "-o",
        "native.otb",
        "--report",
        "json",
    ]);
    success(&native);
    assert!(!String::from_utf8_lossy(&native.stdout).contains("embedded.metadata_name"));
    let explicit = w.run(&[
        "convert",
        "meta.bdf",
        "--to",
        "otb",
        "-o",
        "explicit.otb",
        "--report",
        "json",
        "--otb-name-record",
        "3/1/0x409/1=Meta",
    ]);
    success(&explicit);
    assert!(String::from_utf8_lossy(&explicit.stdout).contains("embedded.metadata_name"));
}
