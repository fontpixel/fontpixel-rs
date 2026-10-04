//! Exercise format options through the executable and its file transaction layer.
#![cfg(any(
    all(feature = "bdf", feature = "raw", feature = "cpi", feature = "draw"),
    all(feature = "bdf", feature = "pf2", feature = "fontx2", feature = "hzk"),
    all(feature = "nfnt", feature = "raw"),
    all(
        feature = "bdf",
        feature = "bmfont",
        feature = "sfd",
        feature = "kbits"
    ),
    all(feature = "bdf", feature = "pk", feature = "gf", feature = "plan9"),
    all(feature = "bdf", feature = "gem", feature = "snf", feature = "amiga")
))]
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fontpixel"))
        .args(args)
        .output()
        .unwrap()
}
fn ok(args: &[&str]) -> Output {
    let out = run(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "fontpixel-p2-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    #[cfg(any(
        all(feature = "bdf", feature = "raw", feature = "cpi", feature = "draw"),
        all(feature = "bdf", feature = "pf2", feature = "fontx2", feature = "hzk"),
        all(
            feature = "bdf",
            feature = "bmfont",
            feature = "sfd",
            feature = "kbits"
        ),
        all(feature = "bdf", feature = "pk", feature = "gf", feature = "plan9"),
        all(feature = "bdf", feature = "gem", feature = "snf", feature = "amiga")
    ))]
    fn source(&self) -> PathBuf {
        let p = self.0.join("source.bdf");
        fs::write(&p, "STARTFONT 2.1\nFONT cli-p2\nSIZE 8 72 72\nFONTBOUNDINGBOX 8 8 0 0\nSTARTPROPERTIES 4\nFONT_ASCENT 8\nFONT_DESCENT 0\nCHARSET_REGISTRY \"ISO10646\"\nCHARSET_ENCODING \"1\"\nENDPROPERTIES\nCHARS 2\nSTARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 8 8 0 0\nBITMAP\n18\n24\n42\n7E\n42\n42\n42\n00\nENDCHAR\nSTARTCHAR B\nENCODING 66\nDWIDTH 8 0\nBBX 8 8 0 0\nBITMAP\n7C\n42\n42\n7C\n42\n42\n7C\n00\nENDCHAR\nENDFONT\n").unwrap();
        p
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn convert(input: &Path, output: &Path, id: &str, extra: &[&str]) {
    let mut args = vec!["convert", text(input), "-o", text(output), "--to", id];
    args.extend_from_slice(extra);
    ok(&args);
}
fn inspect(path: &Path, id: &str, extra: &[&str]) {
    let mut args = vec!["info", text(path), "--from", id, "--json"];
    args.extend_from_slice(extra);
    let result = ok(&args);
    let _: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
}

#[cfg(all(feature = "bdf", feature = "raw", feature = "cpi", feature = "draw"))]
#[test]
fn console_layout_options_and_all_cpi_output_variants() {
    let d = Dir::new();
    let source = d.source();
    let raw = d.0.join("grid.raw");
    let layout = [
        "--raw-width",
        "8",
        "--raw-height",
        "8",
        "--raw-count",
        "2",
        "--raw-first",
        "65",
        "--raw-charset",
        "cp437",
        "--raw-bit-order",
        "lsb",
        "--raw-row-order",
        "bottom-up",
        "--raw-row-padding",
        "1",
        "--raw-offset",
        "2",
    ];
    convert(&source, &raw, "raw", &layout);
    let bytes = fs::read(&raw).unwrap();
    assert_eq!(bytes.len(), 34);
    assert_eq!(&bytes[..6], &[0, 0, 0, 0, 0x42, 0]);
    inspect(&raw, "raw", &layout);
    assert!(!run(&["info", text(&raw), "--from", "raw"]).status.success());
    let c64 = d.0.join("partial.64c");
    convert(&source, &c64, "raw", &["--raw-preset", "64c"]);
    let c64_bytes = fs::read(&c64).unwrap();
    assert_eq!(c64_bytes.len(), 18);
    assert_eq!(&c64_bytes[..2], &[0, 0x38]);
    inspect(&c64, "raw", &["--raw-preset", "64c"]);
    let alternate = d.0.join("alternate.64c");
    convert(
        &source,
        &alternate,
        "raw",
        &[
            "--raw-preset",
            "64c",
            "--raw-default-load-address",
            "0x2000",
        ],
    );
    assert_eq!(&fs::read(&alternate).unwrap()[..2], &[0, 0x20]);
    convert(
        &c64,
        &alternate,
        "raw",
        &[
            "--from",
            "raw",
            "--raw-preset",
            "64c",
            "--raw-default-load-address",
            "0x2000",
        ],
    );
    assert_eq!(fs::read(&alternate).unwrap(), c64_bytes);
    let custom_prefix = d.0.join("custom.raw");
    convert(
        &source,
        &custom_prefix,
        "raw",
        &["--raw-preset", "64c", "--raw-offset", "3"],
    );
    assert_eq!(fs::read(&custom_prefix).unwrap().len(), 19);
    assert_eq!(&fs::read(&custom_prefix).unwrap()[..3], &[0, 0, 0]);
    for variant in ["font", "font-nt", "drfont", "cp", "cpx"] {
        let target = d.0.join(format!("{variant}.cpi"));
        let mut args = vec![
            "--from",
            "raw",
            "--cpi-version",
            variant,
            "--cpi-code-page",
            "437",
        ];
        args.extend_from_slice(&layout);
        convert(&raw, &target, "cpi", &args);
        inspect(&target, "cpi", &[]);
    }
    let standalone = d.0.join("standalone.CP");
    let mut args = vec!["--from", "raw", "--cpi-code-page", "437"];
    args.extend_from_slice(&layout);
    convert(&raw, &standalone, "cpi", &args);
    assert_ne!(&fs::read(&standalone).unwrap()[1..5], b"FONT");
    inspect(&standalone, "cpi", &[]);
    let target = d.0.join("font.draw");
    convert(&source, &target, "draw", &[]);
    inspect(&target, "draw", &[]);
    let cpx = d.0.join("font.cpx");
    convert(&raw, &cpx, "cpi", &args);
    let packed = fs::read(&cpx).unwrap();
    assert_eq!(&packed[..2], &[0x81, 0xfc]);
    assert!(
        packed[..100.min(packed.len())]
            .windows(4)
            .any(|w| w == b"UPX!")
    );
    inspect(&cpx, "cpi", &[]);
    fs::write(&cpx, b"KEEP CPX").unwrap();
    let failed = run(&[
        "convert",
        text(&source),
        "-o",
        text(&cpx),
        "--cpi-version",
        "font",
    ]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("requires --cpi-version cpx"));
    assert_eq!(fs::read(&cpx).unwrap(), b"KEEP CPX");
    let before = fs::read(&raw).unwrap();
    let failed = run(&[
        "convert",
        text(&source),
        "-o",
        text(&raw),
        "--to",
        "raw",
        "--raw-width",
        "8",
    ]);
    assert!(!failed.status.success());
    assert_eq!(fs::read(&raw).unwrap(), before);
}

#[cfg(all(feature = "bdf", feature = "pf2", feature = "fontx2", feature = "hzk"))]
#[test]
fn cjk_custom_geometry_and_fontx2_options() {
    let d = Dir::new();
    let source = d.source();
    let pf2 = d.0.join("font.pf2");
    convert(&source, &pf2, "pf2", &[]);
    inspect(&pf2, "pf2", &[]);
    let fx = d.0.join("font.fon");
    convert(
        &source,
        &fx,
        "fontx2",
        &[
            "--fontx2-mode",
            "single",
            "--fontx2-charset",
            "ascii",
            "--fontx2-width",
            "8",
            "--fontx2-height",
            "8",
        ],
    );
    assert_eq!(&fs::read(&fx).unwrap()[..6], b"FONTX2");
    inspect(&fx, "fontx2", &["--fontx2-read-charset", "ascii"]);
    let raw = d.0.join("font.hzk");
    let geometry = [
        "--hzk-width",
        "8",
        "--hzk-height",
        "8",
        "--hzk-first",
        "U+0041",
        "--hzk-last",
        "0x42",
        "--hzk-code-space",
        "ascii",
        "--hzk-storage",
        "columns",
        "--hzk-count",
        "2",
    ];
    convert(&source, &raw, "hzk", &geometry);
    assert_eq!(fs::metadata(&raw).unwrap().len(), 16);
    inspect(&raw, "hzk", &geometry);
    assert!(!run(&["info", text(&raw), "--from", "hzk"]).status.success());
    let bytes = fs::read(&raw).unwrap();
    let mut args = vec![
        "convert",
        text(&source),
        "-o",
        text(&raw),
        "--to",
        "hzk",
        "--strict",
    ];
    args.extend_from_slice(&geometry);
    assert!(!run(&args).status.success());
    assert_eq!(fs::read(&raw).unwrap(), bytes);
}

#[cfg(all(feature = "nfnt", feature = "raw"))]
#[test]
fn mac_containers_and_appledouble_companion() {
    let d = Dir::new();
    let source = d.0.join("source.raw");
    // Original 8x8 A/B wire cells with explicitly declared Macintosh byte codes.
    fs::write(
        &source,
        [
            0x18, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0, 0x7c, 0x42, 0x42, 0x7c, 0x42, 0x42, 0x7c,
            0,
        ],
    )
    .unwrap();
    for container in [
        "bare",
        "resource-fork",
        "dfont",
        "suitcase",
        "macbinary",
        "apple-single",
        "apple-double",
        "binhex",
    ] {
        let target = d.0.join(format!("{container}.nfnt"));
        convert(
            &source,
            &target,
            "nfnt",
            &[
                "--nfnt-container",
                container,
                "--nfnt-resource-type",
                "font",
                "--from",
                "raw",
                "--raw-width",
                "8",
                "--raw-height",
                "8",
                "--raw-count",
                "2",
                "--raw-first",
                "65",
                "--raw-charset",
                "mac-roman",
            ],
        );
        if container == "apple-double" {
            let companion = format!("._{container}.nfnt");
            assert!(d.0.join(&companion).is_file());
            inspect(
                &target,
                "nfnt",
                &["--nfnt-apple-double-resource", &companion],
            );
        } else {
            inspect(&target, "nfnt", &[]);
        }
    }
}

#[cfg(all(
    feature = "bdf",
    feature = "bmfont",
    feature = "sfd",
    feature = "kbits"
))]
#[test]
fn editor_encodings_page_assets_and_strict_protection() {
    let d = Dir::new();
    let source = d.source();
    for encoding in ["text", "xml", "binary", "json"] {
        let target = d.0.join(format!("{encoding}.fnt"));
        convert(
            &source,
            &target,
            "bmfont",
            &[
                "--bmfont-encoding",
                encoding,
                "--bmfont-page-width",
                "8",
                "--bmfont-page-height",
                "8",
                "--bmfont-padding",
                "0",
            ],
        );
        inspect(&target, "bmfont", &[]);
        let pages: Vec<_> = fs::read_dir(&d.0)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "png"))
            .collect();
        assert!(pages.len() >= 2);
        let before: Vec<_> = pages.iter().map(|p| fs::read(p).unwrap()).collect();
        let descriptor = fs::read(&target).unwrap();
        assert!(
            !run(&[
                "convert",
                text(&source),
                "-o",
                text(&target),
                "--to",
                "bmfont",
                "--strict",
                "--bmfont-page-width",
                "8",
                "--bmfont-page-height",
                "8",
                "--bmfont-padding",
                "0"
            ])
            .status
            .success()
        );
        assert_eq!(fs::read(&target).unwrap(), descriptor);
        for (p, bytes) in pages.iter().zip(before) {
            assert_eq!(fs::read(p).unwrap(), bytes);
        }
    }
    for id in ["sfd", "kbits"] {
        let target = d.0.join(format!("font.{id}"));
        convert(&source, &target, id, &[]);
        inspect(&target, id, &[]);
    }
}

#[cfg(all(feature = "bdf", feature = "pk", feature = "gf", feature = "plan9"))]
#[test]
fn tex_metrics_and_plan9_sidecars() {
    let d = Dir::new();
    let source = d.source();
    for id in ["pk", "gf"] {
        let target = d.0.join(format!("font.{id}"));
        convert(
            &source,
            &target,
            id,
            &[
                "--tex-design-size",
                "21/2",
                "--tex-dpi-x",
                "72",
                "--tex-dpi-y",
                "96",
            ],
        );
        inspect(&target, id, &[]);
    }
    let target = d.0.join("font.plan9");
    convert(
        &source,
        &target,
        "plan9",
        &["--plan9-container", "font", "--plan9-max-glyphs", "1"],
    );
    inspect(&target, "plan9", &[]);
    let manifest = fs::read_to_string(&target).unwrap();
    assert_eq!(manifest.lines().count(), 3);
    let before = fs::read(&target).unwrap();
    assert!(
        !run(&[
            "convert",
            text(&source),
            "-o",
            text(&target),
            "--to",
            "plan9",
            "--tex-dpi",
            "72"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&target).unwrap(), before);
    let compressed = d.0.join("literal.plan9");
    convert(
        &source,
        &compressed,
        "plan9",
        &[
            "--plan9-container",
            "font",
            "--plan9-compression",
            "literal",
        ],
    );
    inspect(&compressed, "plan9", &[]);
}

#[cfg(all(feature = "bdf", feature = "gem", feature = "snf", feature = "amiga"))]
#[test]
fn machine_orders_and_amiga_asset_packaging() {
    use fontpixel::bitmap::{BitOrder, ByteLayout, ByteOrder, RowOrder};
    use fontpixel::formats::{FormatReader, amiga, gem, snf};
    use fontpixel::model::Codepoint;
    use fontpixel::resources::{ReadContext, ResourceResolver};
    use fontpixel::{FontSet, Input, ReadOptions, ResourcePath};

    struct Assets<'a>(&'a Path);
    impl ResourceResolver for Assets<'_> {
        fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> fontpixel::Result<Vec<u8>> {
            let bytes = fs::read(self.0.join(path.as_str()))?;
            assert!(bytes.len() <= max_bytes);
            Ok(bytes)
        }
    }
    fn pixels(path: &Path, codec: &impl FormatReader<FontSet>) {
        let bytes = fs::read(path).unwrap();
        let assets = Assets(path.parent().unwrap());
        let input = Input::new(&bytes)
            .with_name(path.file_name().unwrap().to_str().unwrap())
            .with_resources(&assets);
        let options = ReadOptions::default();
        let (fonts, _) = codec
            .read(
                &mut ReadContext::new(&input, &options.limits).unwrap(),
                &options,
            )
            .unwrap();
        let font = fonts.single().unwrap();
        for (code, rows) in [
            (65, [0x18, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0]),
            (66, [0x7c, 0x42, 0x42, 0x7c, 0x42, 0x42, 0x7c, 0]),
        ] {
            let glyph = font
                .glyphs()
                .map(|(_, glyph)| glyph)
                .find(|glyph| {
                    glyph.codepoints.contains(&Codepoint::Unicode(code))
                        || glyph.codepoints.contains(&Codepoint::Legacy(code))
                })
                .unwrap();
            assert_eq!((glyph.bitmap.width(), glyph.bitmap.height()), (8, 8));
            assert_eq!(glyph.bitmap.as_bytes(), rows);
        }
    }
    let d = Dir::new();
    let source = d.source();
    for order in ["big", "little"] {
        let byte_order = if order == "big" {
            ByteOrder::Big
        } else {
            ByteOrder::Little
        };
        let target = d.0.join(format!("{order}.gem"));
        convert(
            &source,
            &target,
            "gem",
            &[
                "--gem-byte-order",
                order,
                "--gem-word-order",
                order,
                "--gem-unicode-as-legacy",
            ],
        );
        inspect(
            &target,
            "gem",
            &[
                "--gem-read-byte-order",
                order,
                "--gem-read-word-order",
                order,
            ],
        );
        pixels(
            &target,
            &gem::GemCodec {
                read_options: gem::GemReadOptions {
                    byte_order: Some(byte_order),
                    bitmap_word_order: Some(byte_order),
                },
                ..Default::default()
            },
        );
        let snf = d.0.join(format!("{order}.snf"));
        convert(
            &source,
            &snf,
            "snf",
            &[
                "--snf-byte-order",
                order,
                "--snf-layout",
                "lsb,little,4,2,bottom-up",
                "--snf-no-layout-property",
            ],
        );
        inspect(
            &snf,
            "snf",
            &[
                "--snf-read-byte-order",
                order,
                "--snf-read-layout",
                "lsb,little,4,2,bottom-up",
            ],
        );
        pixels(
            &snf,
            &snf::SnfCodec {
                read_options: snf::SnfReadOptions {
                    byte_order: Some(byte_order),
                    bitmap: Some(ByteLayout {
                        bit_order: BitOrder::LsbFirst,
                        byte_order: ByteOrder::Little,
                        row_alignment: 4,
                        scan_unit: 2,
                        row_order: RowOrder::BottomUp,
                    }),
                },
                ..Default::default()
            },
        );
    }
    // Symmetric offsets and geometry make both GEM byte orders structurally
    // valid. A reader override must also reach the registry's detector.
    let ambiguous = d.0.join("ambiguous.fnt");
    let mut bytes = vec![0; 0x0002_0200 + 0x0101 * 0x0101];
    bytes[72..76].copy_from_slice(&0x0001_0100_u32.to_be_bytes());
    bytes[76..80].copy_from_slice(&0x0002_0200_u32.to_be_bytes());
    bytes[80..82].copy_from_slice(&0x0101_u16.to_be_bytes());
    bytes[82..84].copy_from_slice(&0x0101_u16.to_be_bytes());
    fs::write(&ambiguous, bytes).unwrap();
    assert!(!run(&["info", text(&ambiguous)]).status.success());
    ok(&[
        "info",
        text(&ambiguous),
        "--gem-read-byte-order",
        "big",
        "--json",
    ]);
    for packaging in ["strike", "contents"] {
        let target = d.0.join(format!("{packaging}.font"));
        convert(&source, &target, "amiga", &["--amiga-packaging", packaging]);
        inspect(&target, "amiga", &[]);
        let bytes = fs::read(&target).unwrap();
        if packaging == "strike" {
            assert_eq!(&bytes[..4], &[0, 0, 3, 0xf3]);
        } else {
            assert_eq!(&bytes[..2], &[0x0f, 2]);
        }
        pixels(&target, &amiga::AmigaCodec::default());
    }
}
