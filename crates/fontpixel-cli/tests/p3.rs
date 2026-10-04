//! Exercise phase 5 format selection, pixels, limits and output transactions.
#![cfg(all(
    feature = "bdf",
    feature = "raw",
    feature = "vfont",
    feature = "fzx",
    feature = "figlet",
    feature = "hanfont",
    feature = "iigs",
    feature = "color"
))]
use fontpixel::formats::{FormatReader, embedded, figlet, fzx, iigs, vfont};
use fontpixel::model::{Codepoint, GlyphBitmap};
use fontpixel::resources::ReadContext;
use fontpixel::{FontSet, Input, ReadOptions};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

const ROWS: [[u8; 8]; 2] = [
    [0x18, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0],
    [0x7c, 0x42, 0x42, 0x7c, 0x42, 0x42, 0x7c, 0],
];
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "fontpixel-p3-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self) -> PathBuf {
        let mut bdf = String::from(
            "STARTFONT 2.1\nFONT cli-p3\nSIZE 8 72 72\nFONTBOUNDINGBOX 8 8 0 0\nSTARTPROPERTIES 4\nFONT_ASCENT 8\nFONT_DESCENT 0\nCHARSET_REGISTRY \"ISO10646\"\nCHARSET_ENCODING \"1\"\nENDPROPERTIES\nCHARS 2\n",
        );
        for (i, rows) in ROWS.iter().enumerate() {
            bdf.push_str(&format!(
                "STARTCHAR glyph{i}\nENCODING {}\nDWIDTH 8 0\nBBX 8 8 0 0\nBITMAP\n",
                65 + i
            ));
            for row in rows {
                bdf.push_str(&format!("{row:02X}\n"));
            }
            bdf.push_str("ENDCHAR\n");
        }
        bdf.push_str("ENDFONT\n");
        let path = self.0.join("source.bdf");
        fs::write(&path, bdf).unwrap();
        path
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
fn convert(source: &Path, target: &Path, id: &str, options: &[&str]) {
    let mut args = vec!["convert", text(source), "-o", text(target), "--to", id];
    args.extend_from_slice(options);
    ok(&args);
}
fn read(path: &Path, codec: &(impl FormatReader<FontSet> + ?Sized)) -> FontSet {
    struct Files<'a>(&'a Path);
    impl fontpixel::ResourceResolver for Files<'_> {
        fn resolve(
            &self,
            path: &fontpixel::ResourcePath,
            max_bytes: usize,
        ) -> fontpixel::Result<Vec<u8>> {
            use std::io::Read;
            let file = match fs::File::open(self.0.join(path.as_str())) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(fontpixel::Error::MissingResource(path.to_string()));
                }
                Err(error) => return Err(error.into()),
            };
            let mut bytes = Vec::new();
            file.take(max_bytes.saturating_add(1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > max_bytes {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "test companion bytes",
                    limit: max_bytes,
                });
            }
            Ok(bytes)
        }
    }
    let bytes = fs::read(path).unwrap();
    let resolver = Files(path.parent().unwrap());
    let input = Input::new(&bytes)
        .with_name(path.file_name().unwrap().to_str().unwrap())
        .with_resources(&resolver);
    let options = ReadOptions::default();
    codec
        .read(
            &mut ReadContext::new(&input, &options.limits).unwrap(),
            &options,
        )
        .unwrap()
        .0
}

#[cfg(all(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "pxl",
    feature = "signum"
))]
#[test]
fn historical_variants_preserve_ink_and_write_companions() {
    use fontpixel::formats::{daisy, polyprint, printshop, pxl, signum, tasprint};

    let dir = Dir::new();
    let source = dir.source();
    let daisy_source = dir.0.join("daisy-source.bdf");
    fs::write(
        &daisy_source,
        fs::read_to_string(&source)
            .unwrap()
            .replace("DWIDTH 8 0", "DWIDTH 9 0"),
    )
    .unwrap();
    for version in ["2", "3", "magnified"] {
        let target = dir.0.join(format!("daisy-{version}.nlq"));
        convert(
            &daisy_source,
            &target,
            "daisy",
            &[
                "--native-unicode-as-legacy",
                "--daisy-version",
                version,
                "--daisy-allow-control-bytes",
                "--daisy-height",
                if version == "magnified" { "64" } else { "16" },
                "--daisy-underline",
                "9",
            ],
        );
        ink(&read(&target, &daisy::DaisyCodec::default()));
    }
    let target = dir.0.join("polyprint.fnt");
    convert(
        &source,
        &target,
        "polyprint",
        &["--native-unicode-as-legacy"],
    );
    ink(&read(&target, &polyprint::PolyprintCodec::default()));
    let target = dir.0.join("printshop.pnf");
    convert(
        &source,
        &target,
        "printshop",
        &["--native-unicode-as-legacy"],
    );
    ink(&read(&target, &printshop::PrintshopCodec::default()));
    for version in ["48k", "plus3", "cpc", "pcw"] {
        let target = dir.0.join(format!("tasprint-{version}.bin"));
        convert(
            &source,
            &target,
            "tasprint",
            &["--native-unicode-as-legacy", "--tasprint-version", version],
        );
        ink(&read(&target, &tasprint::TasprintCodec::default()));
    }
    for version in ["1001", "1002"] {
        let target = dir.0.join(format!("pxl-{version}.pxl"));
        let companion = format!("metrics-{version}.tfm");
        convert(
            &source,
            &target,
            "pxl",
            &[
                "--native-unicode-as-legacy",
                "--pxl-version",
                version,
                "--pxl-tfm",
                &companion,
                "--pxl-magnification",
                "1000",
            ],
        );
        assert!(dir.0.join(&companion).is_file());
        let codec = pxl::PxlCodec {
            read_options: pxl::PxlReadOptions {
                tfm: Some(fontpixel::ResourcePath::new(&companion).unwrap()),
                ..Default::default()
            },
            ..Default::default()
        };
        ink(&read(&target, &codec));
        ok(&[
            "info",
            text(&target),
            "--from",
            "pxl",
            "--pxl-read-version",
            version,
            "--pxl-read-tfm",
            &companion,
        ]);
    }
    for version in ["e24", "p9", "p24", "l30"] {
        let target = dir.0.join(format!("signum.{version}"));
        convert(
            &source,
            &target,
            "signum",
            &["--native-unicode-as-legacy", "--signum-version", version],
        );
        ink(&read(&target, &signum::SignumCodec::default()));
    }
}
fn ink(fonts: &FontSet) {
    ink_at(fonts, 65);
}
fn ink_at(fonts: &FontSet, first: u32) {
    use std::collections::BTreeSet;
    let font = fonts.single().unwrap();
    for (i, rows) in ROWS.iter().enumerate() {
        let code = first + i as u32;
        let glyph = font
            .glyph_for_codepoint(Codepoint::Legacy(code))
            .or_else(|| font.glyph_for_codepoint(Codepoint::Unicode(code)))
            .unwrap();
        let mut actual = BTreeSet::new();
        for y in 0..glyph.bitmap.height() {
            for x in 0..glyph.bitmap.width() {
                let value = match &glyph.bitmap {
                    GlyphBitmap::Mono(b) => b.get_pixel(x, y).unwrap(),
                    GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap(),
                };
                if value != 0 {
                    actual.insert((
                        glyph.metrics.bounds.x + i64::from(x),
                        glyph.metrics.bounds.y + i64::from(glyph.bitmap.height() - 1 - y),
                    ));
                }
            }
        }
        let mut expected = BTreeSet::new();
        for (y, row) in rows.iter().enumerate() {
            for x in 0..8 {
                if row & (0x80 >> x) != 0 {
                    expected.insert((x, 7 - y as i64));
                }
            }
        }
        assert_eq!(
            actual,
            expected,
            "{} code {code}",
            fonts.source_format.as_deref().unwrap_or("unknown")
        );
    }
}
fn pixels(fonts: &FontSet) {
    ink(fonts);
    let font = fonts.single().unwrap();
    for (i, rows) in ROWS.iter().enumerate() {
        let code = 65 + i as u32;
        let glyph = font
            .glyph_for_codepoint(Codepoint::Unicode(code))
            .or_else(|| font.glyph_for_codepoint(Codepoint::Legacy(code)))
            .unwrap();
        assert_eq!((glyph.bitmap.width(), glyph.bitmap.height()), (8, 8));
        for (y, row) in rows.iter().enumerate() {
            for x in 0..8 {
                let value = match &glyph.bitmap {
                    GlyphBitmap::Mono(bitmap) => bitmap.get_pixel(x, y as u32).unwrap() * 255,
                    GlyphBitmap::Gray8(bitmap) => bitmap.get_pixel(x, y as u32).unwrap(),
                };
                assert_eq!(
                    value,
                    if row & (0x80 >> x) != 0 { 255 } else { 0 },
                    "code {code}, ({x},{y})"
                );
            }
        }
        assert_eq!(glyph.metrics.advance.unwrap().x, 8);
    }
}

#[cfg(all(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "xbin",
    feature = "zapfont",
    feature = "comfont",
    feature = "consoleet",
    feature = "yaff"
))]
#[test]
fn native_console_loaders_and_real_directories() {
    use fontpixel::formats::{bbc, comfont, grasp, wyse, xbin, zapfont};
    let dir = Dir::new();
    let source = dir.source();
    for (id, codec) in [
        (
            "bbc",
            Box::new(bbc::BbcCodec::default()) as Box<dyn FormatReader<FontSet>>,
        ),
        ("wyse", Box::new(wyse::WyseCodec::default())),
        ("grasp", Box::new(grasp::GraspCodec::default())),
    ] {
        let path = dir.0.join(format!("font.{id}"));
        convert(&source, &path, id, &["--native-unicode-as-legacy"]);
        ink(&read(&path, &*codec));
    }
    for count in ["256", "512"] {
        let path = dir.0.join(format!("font-{count}.xbin"));
        convert(
            &source,
            &path,
            "xbin",
            &[
                "--native-unicode-as-legacy",
                "--xbin-count",
                count,
                "--xbin-screen",
                "font-only",
            ],
        );
        pixels(&read(&path, &xbin::XbinCodec::default()));
    }
    for variant in ["old", "redraw"] {
        let path = dir.0.join(format!("font-{variant}.zap"));
        convert(
            &source,
            &path,
            "zapfont",
            &["--native-unicode-as-legacy", "--zapfont-variant", variant],
        );
        pixels(&read(&path, &zapfont::ZapfontCodec::default()));
    }
    for variant in [
        "fontedit",
        "frapt",
        "frapt-tsr",
        "letafont",
        "mania",
        "psfcom",
        "udg",
    ] {
        let path = dir.0.join(format!("font-{variant}.com"));
        convert(
            &source,
            &path,
            "comfont",
            &[
                "--native-unicode-as-legacy",
                "--comfont-variant",
                variant,
                "--comfont-cpm-target",
                "pcw",
            ],
        );
        ink(&read(&path, &comfont::ComfontCodec::default()));
        ok(&[
            "info",
            text(&path),
            "--from",
            "comfont",
            "--comfont-read-variant",
            variant,
        ]);
    }
    let folder = dir.0.join("glyphs");
    convert(
        &source,
        &folder,
        "consoleet",
        &["--consoleet-codes", "unicode"],
    );
    assert!(folder.is_dir());
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
    let restored = dir.0.join("restored.yaff");
    convert(
        &folder,
        &restored,
        "yaff",
        &["--from", "consoleet", "--consoleet-read-codes", "unicode"],
    );
    pixels(&read(
        &restored,
        &fontpixel::formats::yaff::YaffCodec::default(),
    ));
    assert!(!run(&["info", text(&folder)]).status.success());
    let failed = dir.0.join("bounded.bdf");
    assert!(
        !run(&[
            "convert",
            text(&folder),
            "--from",
            "consoleet",
            "--to",
            "bdf",
            "-o",
            text(&failed),
            "--max-resources",
            "1"
        ])
        .status
        .success()
    );
    assert!(!failed.exists());
}

#[cfg(all(
    feature = "fbit",
    feature = "lisa",
    feature = "mgtk",
    feature = "qpf",
    feature = "qpf2",
    feature = "beos",
    feature = "oberon",
    feature = "symbos",
    feature = "wsfont",
    feature = "vtfont"
))]
#[test]
fn system_fonts_options_preserve_native_pixels() {
    use fontpixel::formats::{beos, fbit, lisa, mgtk, oberon, qpf, qpf2, symbos, vtfont, wsfont};
    let dir = Dir::new();
    let source = dir.source();
    let legacy = dir.0.join("legacy.raw");
    fs::write(&legacy, ROWS.concat()).unwrap();
    let raw = [
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
    ];
    for container in ["bare", "macbinary", "binhex", "apple-single", "dfont"] {
        let path = dir.0.join(format!("fbit-{container}.bin"));
        let mut args = raw.to_vec();
        args[9] = "33088";
        args[11] = "shift-jis";
        args.extend(["--fbit-container", container]);
        convert(&legacy, &path, "fbit", &args);
        ink_at(&read(&path, &fbit::FbitCodec::default()), 0x8140);
    }
    let path = dir.0.join("split.fbit");
    let mut args = raw.to_vec();
    args[9] = "33088";
    args[11] = "shift-jis";
    args.extend([
        "--fbit-definition",
        "data-fork7",
        "--fbit-data-fork",
        "pixels.data",
    ]);
    convert(&legacy, &path, "fbit", &args);
    assert!(dir.0.join("pixels.data").is_file());
    let codec = fbit::FbitCodec {
        read_options: fbit::FbitReadOptions {
            data_fork: Some(fontpixel::ResourcePath::new("pixels.data").unwrap()),
            ..Default::default()
        },
        ..Default::default()
    };
    ink_at(&read(&path, &codec), 0x8140);
    ok(&[
        "info",
        text(&path),
        "--from",
        "fbit",
        "--fbit-read-data-fork",
        "pixels.data",
    ]);
    for compression in ["preserve", "uncompressed"] {
        let path = dir.0.join(format!("lisa-{compression}.lib"));
        let mut args = raw.to_vec();
        args.extend(["--lisa-compression", compression]);
        convert(&legacy, &path, "lisa", &args);
        ink(&read(&path, &lisa::LisaCodec::default()));
    }
    let path = dir.0.join("mgtk.font");
    let mut args = raw.to_vec();
    args.extend(["--mgtk-columns", "double"]);
    convert(&legacy, &path, "mgtk", &args);
    ink(&read(&path, &mgtk::MgtkCodec::default()));
    let path = dir.0.join("qt.qpf");
    convert(&source, &path, "qpf", &[]);
    pixels(&read(&path, &qpf::QpfCodec::default()));
    for format in ["gray8", "mono"] {
        let path = dir.0.join(format!("qt-{format}.qpf2"));
        convert(&source, &path, "qpf2", &["--qpf2-glyph-format", format]);
        pixels(&read(&path, &qpf2::Qpf2Codec::default()));
    }
    for coverage in ["native8", "full-nibble"] {
        let path = dir.0.join(format!("beos-{coverage}.bmf"));
        convert(&source, &path, "beos", &["--beos-coverage", coverage]);
        let codec = beos::BeosCodec {
            read_options: beos::BeosReadOptions {
                coverage: if coverage == "native8" {
                    beos::BeosCoverage::Native8
                } else {
                    beos::BeosCoverage::FullNibble
                },
                ..Default::default()
            },
            ..Default::default()
        };
        pixels(&read(&path, &codec));
        ok(&[
            "info",
            text(&path),
            "--from",
            "beos",
            "--beos-read-coverage",
            coverage,
        ]);
    }
    let path = dir.0.join("oberon.fnt");
    convert(
        &source,
        &path,
        "oberon",
        &[
            "--native-unicode-as-legacy",
            "--oberon-family",
            "6",
            "--oberon-variant",
            "32",
        ],
    );
    ink(&read(&path, &oberon::OberonCodec::default()));
    for version in ["small", "medium", "big"] {
        let path = dir.0.join(format!("symbos-{version}.fnt"));
        convert(
            &source,
            &path,
            "symbos",
            &["--native-unicode-as-legacy", "--symbos-version", version],
        );
        ink(&read(&path, &symbos::SymbosCodec::default()));
    }
    for order in ["left", "right", "known"] {
        let path = dir.0.join(format!("wsf-{order}.wsf"));
        convert(
            &source,
            &path,
            "wsfont",
            &[
                "--native-unicode-as-legacy",
                "--wsfont-bit-order",
                order,
                "--wsfont-byte-order",
                order,
                "--wsfont-depth",
                "mono",
                "--wsfont-stride",
                "2",
            ],
        );
        pixels(&read(&path, &wsfont::WsfontCodec::default()));
    }
    let path = dir.0.join("vtfont.fnt");
    convert(
        &source,
        &path,
        "vtfont",
        &[
            "--vtfont-cell-width",
            "8",
            "--vtfont-cell-height",
            "8",
            "--vtfont-rebuild",
        ],
    );
    // The VFNT model contains regular and bold strikes with independent fallback.
    let set = read(&path, &vtfont::VtfontCodec::default());
    for font in set.fonts {
        ink(&FontSet::from(font));
    }
}

#[cfg(all(
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf"
))]
#[test]
fn device_fonts_and_playdate_atlas_options() {
    use fontpixel::formats::{cybiko, hmzk, playdate, rockbox, sbf, u8m};
    let dir = Dir::new();
    let source = dir.source();
    for flavor in ["rb11", "rb12"] {
        for depth in ["mono", "gray4"] {
            let path = dir.0.join(format!("{flavor}-{depth}.fnt"));
            if flavor == "rb11" && depth == "gray4" {
                let out = run(&[
                    "convert",
                    text(&source),
                    "-o",
                    text(&path),
                    "--to",
                    "rockbox",
                    "--rockbox-flavor",
                    flavor,
                    "--rockbox-depth",
                    depth,
                ]);
                assert!(!out.status.success());
                assert!(
                    String::from_utf8_lossy(&out.stderr).contains("RB11 cannot store grayscale")
                );
                assert!(!path.exists());
                continue;
            }
            convert(
                &source,
                &path,
                "rockbox",
                &["--rockbox-flavor", flavor, "--rockbox-depth", depth],
            );
            pixels(&read(&path, &rockbox::RockboxCodec::default()));
        }
    }
    let path = dir.0.join("font.u8m");
    convert(&source, &path, "u8m", &["--u8m-load-address", "32768"]);
    assert_eq!(&fs::read(&path).unwrap()[..2], &[0, 128]);
    pixels(&read(&path, &u8m::U8mCodec::default()));
    let path = dir.0.join("font.cyf");
    convert(&source, &path, "cybiko", &["--native-unicode-as-legacy"]);
    pixels(&read(&path, &cybiko::CybikoCodec::default()));
    let path = dir.0.join("font.sbf");
    convert(&source, &path, "sbf", &["--native-unicode-as-legacy"]);
    pixels(&read(&path, &sbf::SbfCodec::default()));
    for flavor in ["fnt-embedded", "fnt-png", "pft"] {
        let path = dir.0.join(format!("playdate-{flavor}.fnt"));
        convert(
            &source,
            &path,
            "playdate",
            &[
                "--playdate-flavor",
                flavor,
                "--playdate-cell-width",
                "8",
                "--playdate-cell-height",
                "8",
                "--playdate-tracking",
                "0",
            ],
        );
        pixels(&read(&path, &playdate::PlaydateCodec::default()));
        ok(&["info", text(&path), "--from", "playdate"]);
    }
    let path = dir.0.join("font.hmzk");
    convert(&source, &path, "hmzk", &[]);
    ink(&read(&path, &hmzk::HmzkCodec::default()));
}

#[cfg(feature = "yaff")]
#[test]
fn raw_cli_packing_and_horizontal_strips_match_independent_bytes() {
    let dir = Dir::new();
    for (packing, order, columns) in [
        ("bit-aligned", "row-major", 1usize),
        ("byte-rows", "column-major", 2),
        ("byte-rows", "row-major", 2),
    ] {
        let lit = |glyph: usize, x: usize, y: usize| (x + 3 * y + 5 * glyph) % 7 <= 1;
        let mut bytes = vec![0u8; if packing == "bit-aligned" { 7 } else { 9 }];
        for glyph in 0..2 {
            for y in 0..3 {
                for x in 0..9 {
                    if !lit(glyph, x, y) {
                        continue;
                    }
                    let bit = if packing == "bit-aligned" {
                        glyph * 27 + y * 9 + x
                    } else {
                        y * 24 + glyph * 9 + x
                    };
                    let byte = if order == "column-major" {
                        (bit / 8 % 3) * 3 + bit / 24
                    } else {
                        bit / 8
                    };
                    bytes[byte] |= 0x80 >> (bit % 8);
                }
            }
        }
        let source = dir.0.join(format!("{packing}-{order}.raw"));
        fs::write(&source, &bytes).unwrap();
        let target = source.with_extension("yaff");
        let columns = columns.to_string();
        let layout = [
            "--raw-width",
            "9",
            "--raw-height",
            "3",
            "--raw-count",
            "2",
            "--raw-first",
            "65",
            "--raw-packing",
            packing,
            "--raw-byte-order",
            order,
            "--raw-strike-count",
            &columns,
        ];
        let mut args = layout.to_vec();
        args.extend(["--from", "raw"]);
        convert(&source, &target, "yaff", &args);
        let set = read(&target, &fontpixel::formats::yaff::YaffCodec::default());
        for glyph in 0..2 {
            let g = set
                .single()
                .unwrap()
                .glyph_for_codepoint(Codepoint::Legacy(65 + glyph as u32))
                .unwrap();
            let GlyphBitmap::Mono(bitmap) = &g.bitmap else {
                panic!("expected mono");
            };
            assert_eq!((bitmap.width(), bitmap.height()), (9, 3));
            for y in 0..3 {
                for x in 0..9 {
                    assert_eq!(
                        bitmap.get_pixel(x as u32, y as u32).unwrap() != 0,
                        lit(glyph, x, y)
                    );
                }
            }
        }
        let rewritten = source.with_extension("out");
        convert(&target, &rewritten, "raw", &layout);
        assert_eq!(fs::read(&rewritten).unwrap(), bytes);
    }
}

#[cfg(all(feature = "pilfont", feature = "vlw", feature = "geos"))]
#[test]
fn native_image_masks_vlw_versions_and_geos_containers() {
    use fontpixel::formats::{geos, pilfont, vlw};
    let dir = Dir::new();
    let source = dir.source();
    let path = dir.0.join("pillow.pil");
    convert(
        &source,
        &path,
        "pilfont",
        &["--native-unicode-as-legacy", "--pilfont-max-width", "8"],
    );
    let atlas = dir.0.join("pillow.pbm");
    assert!(fs::read(&atlas).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
    pixels(&read(&path, &pilfont::PilfontCodec::default()));
    ok(&[
        "info",
        text(&path),
        "--from",
        "pilfont",
        "--pilfont-read-atlas",
        "pillow.pbm",
        "--max-resources",
        "1",
    ]);
    let denied = run(&[
        "info",
        text(&path),
        "--from",
        "pilfont",
        "--pilfont-read-atlas",
        "pillow.pbm",
        "--max-resource-bytes",
        "1",
    ]);
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("exceeds configured limit"));
    for version in ["8", "9", "10", "11"] {
        let path = dir.0.join(format!("processing-{version}.vlw"));
        convert(
            &source,
            &path,
            "vlw",
            &["--vlw-version", version, "--vlw-smooth", "false"],
        );
        assert_eq!(
            u32::from_be_bytes(fs::read(&path).unwrap()[4..8].try_into().unwrap()).to_string(),
            version
        );
        pixels(&read(&path, &vlw::VlwCodec::default()));
    }
    for (container, variant) in [
        ("bare", "standard"),
        ("cvt", "standard"),
        ("cvt", "extended"),
        ("cvt", "utf8"),
        ("cvt", "mega"),
        ("cvt", "mega-utf8"),
    ] {
        let path = dir.0.join(format!("geos-{container}-{variant}.cvt"));
        convert(
            &source,
            &path,
            "geos",
            &[
                "--native-unicode-as-legacy",
                "--geos-container",
                container,
                "--geos-variant",
                variant,
                "--geos-font-id",
                "32",
                "--geos-family",
                "Original",
                "--geos-rebuild",
            ],
        );
        ink(&read(&path, &geos::GeosCodec::default()));
    }
}

#[cfg(all(feature = "image", feature = "yaff"))]
#[test]
fn imageset_directory_preserves_pixels_and_enforces_scope_and_limits() {
    use fontpixel::formats::yaff::YaffCodec;
    let dir = Dir::new();
    let source = dir.source();
    for (encoding, background) in [
        ("png", "black"),
        ("png", "white"),
        ("png", "alpha"),
        ("bmp", "black"),
        ("gif", "black"),
        ("tga", "alpha"),
        ("pbm", "white"),
        ("pgm", "black"),
    ] {
        let target = dir.0.join(format!("images-{encoding}-{background}"));
        convert(
            &source,
            &target,
            "image",
            &[
                "--image-layout",
                "imageset",
                "--image-domain",
                "unicode",
                "--image-prefix",
                "u",
                "--image-radix",
                "16",
                "--image-background",
                background,
                "--image-encoding",
                encoding,
            ],
        );
        assert!(target.is_dir());
        let files = fs::read_dir(&target)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(
            files
                .iter()
                .all(|v| v.starts_with('u') && v.ends_with(encoding)),
            "{files:?}"
        );
        let restored = dir.0.join(format!("images-{encoding}-{background}.yaff"));
        ok(&[
            "convert",
            text(&target),
            "--from",
            "image",
            "-o",
            text(&restored),
            "--to",
            "yaff",
            "--image-read-domain",
            "unicode",
            "--image-read-prefix",
            "u",
            "--image-read-radix",
            "16",
            "--image-read-background",
            background,
        ]);
        pixels(&read(&restored, &YaffCodec::default()));
        let denied = dir.0.join(format!("denied-{encoding}-{background}.yaff"));
        let result = run(&[
            "convert",
            text(&target),
            "--from",
            "image",
            "-o",
            text(&denied),
            "--to",
            "yaff",
            "--image-read-domain",
            "unicode",
            "--image-read-prefix",
            "u",
            "--max-resources",
            "1",
        ]);
        assert!(!result.status.success());
        assert!(!denied.exists());
        assert!(String::from_utf8_lossy(&result.stderr).contains("exceeds configured limit"));
    }
    let target = dir.0.join("escape");
    let denied = run(&[
        "convert",
        text(&source),
        "--from",
        "bdf",
        "-o",
        text(&target),
        "--to",
        "image",
        "--image-layout",
        "imageset",
        "--image-domain",
        "unicode",
        "--image-prefix",
        "../",
    ]);
    assert!(!denied.status.success());
    assert!(!target.exists());
    let strict = dir.0.join("strict-images");
    let denied = run(&[
        "convert",
        text(&source),
        "--from",
        "bdf",
        "-o",
        text(&strict),
        "--to",
        "image",
        "--image-layout",
        "imageset",
        "--image-domain",
        "unicode",
        "--strict",
    ]);
    assert!(
        !denied.status.success(),
        "native filenames cannot retain glyph names and font metadata"
    );
    assert!(!strict.exists());
}

#[cfg(all(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "hppcl",
    feature = "dashen",
    feature = "prebuilt",
    feature = "yaff"
))]
#[test]
fn workstation_options_preserve_native_pixels_and_names() {
    use fontpixel::formats::{catalyst, chiwriter, dashen, hppcl, prebuilt};
    let dir = Dir::new();
    let source = dir.source();
    for base in ["resource-body", "glyph-data"] {
        let path = dir.0.join(format!("catalyst-{base}.bin"));
        convert(
            &source,
            &path,
            "catalyst",
            &[
                "--native-unicode-as-legacy",
                "--catalyst-write-offset-base",
                base,
                "--catalyst-height",
                "8",
                "--catalyst-leading",
                "0",
                "--catalyst-sidebearing",
                "0",
            ],
        );
        let codec = catalyst::CatalystCodec {
            read_options: catalyst::CatalystReadOptions {
                offset_base: if base == "glyph-data" {
                    catalyst::CatalystOffsetBase::GlyphData
                } else {
                    catalyst::CatalystOffsetBase::ResourceBody
                },
            },
            ..Default::default()
        };
        ink(&read(&path, &codec));
        ok(&[
            "info",
            text(&path),
            "--from",
            "catalyst",
            "--catalyst-offset-base",
            base,
        ]);
    }
    for version in ["3", "4"] {
        for &layout in if version == "3" {
            &["contiguous", "absolute", "relative"][..]
        } else {
            &["contiguous"][..]
        } {
            let path = dir.0.join(format!("chi-{version}-{layout}.set"));
            convert(
                &source,
                &path,
                "chiwriter",
                &[
                    "--native-unicode-as-legacy",
                    "--chiwriter-version",
                    version,
                    "--chiwriter-layout",
                    layout,
                    "--chiwriter-stride",
                    "2",
                    "--chiwriter-baseline",
                    "8",
                ],
            );
            let codec = chiwriter::ChiwriterCodec {
                read_options: chiwriter::ChiwriterReadOptions {
                    version: Some(if version == "3" {
                        chiwriter::ChiwriterVersion::V3
                    } else {
                        chiwriter::ChiwriterVersion::V4
                    }),
                    layout: match layout {
                        "absolute" => chiwriter::ChiwriterLayout::AbsoluteOffsets,
                        "relative" => chiwriter::ChiwriterLayout::RelativeOffsets,
                        _ => chiwriter::ChiwriterLayout::Contiguous,
                    },
                    stride: Some(2),
                },
                ..Default::default()
            };
            ink(&read(&path, &codec));
            ok(&[
                "info",
                text(&path),
                "--from",
                "chiwriter",
                "--chiwriter-read-version",
                version,
                "--chiwriter-read-layout",
                layout,
                "--chiwriter-read-stride",
                "2",
            ]);
        }
    }
    let overlap = dir.0.join("chi-v4-overlap.set");
    let denied = run(&[
        "convert",
        text(&source),
        "-o",
        text(&overlap),
        "--to",
        "chiwriter",
        "--native-unicode-as-legacy",
        "--chiwriter-version",
        "4",
        "--chiwriter-layout",
        "absolute",
    ]);
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("offsets overlap"));
    assert!(!overlap.exists());
    for orientation in [
        "portrait",
        "landscape",
        "reverse-portrait",
        "reverse-landscape",
    ] {
        for compression in ["raw", "rle"] {
            let path = dir
                .0
                .join(format!("printer-{orientation}-{compression}.sfp"));
            convert(
                &source,
                &path,
                "hppcl",
                &[
                    "--native-unicode-as-legacy",
                    "--hppcl-orientation",
                    orientation,
                    "--hppcl-compression",
                    compression,
                    "--hppcl-continuation-bytes",
                    "17",
                    "--hppcl-x-resolution",
                    "300",
                    "--hppcl-y-resolution",
                    "300",
                ],
            );
            ink(&read(&path, &hppcl::HppclCodec::default()));
        }
    }
    let mut two_strikes = read(&source, &fontpixel::formats::bdf::BdfCodec::default());
    two_strikes.fonts.push(two_strikes.fonts[0].clone());
    let dual = dir.0.join("screen-and-printer.yaff");
    fs::write(
        &dual,
        fontpixel::formats::yaff::write(&two_strikes, &Default::default())
            .unwrap()
            .0,
    )
    .unwrap();
    for version in ["3", "4"] {
        let path = dir.0.join(format!("dashen-{version}.pft"));
        convert(
            if version == "4" { &dual } else { &source },
            &path,
            "dashen",
            &[
                "--native-unicode-as-legacy",
                "--dashen-version",
                version,
                "--dashen-top",
                "8",
                "--dashen-codepoint-map",
            ],
        );
        let result = read(&path, &dashen::DashenCodec::default());
        assert_eq!(result.fonts.len(), if version == "4" { 2 } else { 1 });
        for font in result.fonts {
            let mut single = FontSet::default();
            single.fonts.push(font);
            ink(&single);
        }
        ok(&[
            "info",
            text(&path),
            "--from",
            "dashen",
            "--dashen-read-version",
            version,
        ]);
    }
    for order in ["big", "little"] {
        for bit_order in ["msb", "lsb"] {
            for depth in ["mono", "gray2", "gray4", "gray8"] {
                let path = dir
                    .0
                    .join(format!("prebuilt-{order}-{bit_order}-{depth}.bepf"));
                convert(
                    &source,
                    &path,
                    "prebuilt",
                    &[
                        "--prebuilt-byte-order",
                        order,
                        "--prebuilt-bitmap-bit-order",
                        bit_order,
                        "--prebuilt-depth",
                        depth,
                        "--prebuilt-charset-name",
                        "Original",
                        "--prebuilt-identifier",
                        "42",
                    ],
                );
                let codec = prebuilt::PrebuiltCodec {
                    read_options: prebuilt::PrebuiltReadOptions {
                        bitmap_bit_order: if bit_order == "lsb" {
                            prebuilt::PrebuiltBitOrder::LsbFirst
                        } else {
                            prebuilt::PrebuiltBitOrder::MsbFirst
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let result = read(&path, &codec);
                let source = read(&source, &fontpixel::formats::bdf::BdfCodec::default());
                assert_eq!(source.fonts[0].len(), result.fonts[0].len());
                for (a, b) in source.fonts[0].glyphs().zip(result.fonts[0].glyphs()) {
                    assert_eq!(a.1.name, b.1.name);
                    assert_eq!(a.1.metrics.bounds, b.1.metrics.bounds);
                    let sample = |bitmap: &GlyphBitmap, x, y| match bitmap {
                        GlyphBitmap::Mono(b) => b.get_pixel(x, y).unwrap() * 255,
                        GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap(),
                    };
                    for y in 0..8 {
                        for x in 0..8 {
                            assert_eq!(sample(&a.1.bitmap, x, y), sample(&b.1.bitmap, x, y));
                        }
                    }
                }
                ok(&[
                    "info",
                    text(&path),
                    "--from",
                    "prebuilt",
                    "--prebuilt-read-byte-order",
                    order,
                    "--prebuilt-read-bitmap-bit-order",
                    bit_order,
                ]);
            }
        }
    }
}

#[cfg(all(
    feature = "gpi",
    feature = "pcgeos",
    feature = "riscos",
    feature = "pfr",
    feature = "sfont"
))]
#[test]
fn portable_native_profiles_and_metrics_companions() {
    use fontpixel::formats::{gpi, pcgeos, pfr, riscos, sfont};
    let dir = Dir::new();
    let source = dir.source();
    for container in ["bare", "ne", "lx"] {
        for kind in ["fixed", "proportional", "abc"] {
            let path = dir.0.join(format!("gpi-{container}-{kind}.fnt"));
            convert(
                &source,
                &path,
                "gpi",
                &[
                    "--native-unicode-as-legacy",
                    "--gpi-container",
                    container,
                    "--gpi-kind",
                    kind,
                    "--gpi-codepage",
                    "850",
                    "--gpi-registry-id",
                    "37",
                    "--gpi-rebuild",
                ],
            );
            ink(&read(&path, &gpi::GpiCodec::default()));
            ok(&[
                "info",
                text(&path),
                "--from",
                "gpi",
                "--gpi-include-unmapped-null",
                "false",
            ]);
        }
    }
    for variant in ["sbcs", "dbcs"] {
        let path = dir.0.join(format!("pcgeos-{variant}.fnt"));
        convert(
            &source,
            &path,
            "pcgeos",
            &[
                "--native-unicode-as-legacy",
                "--pcgeos-variant",
                variant,
                "--pcgeos-font-id",
                "42",
                "--pcgeos-family",
                "Original",
                "--pcgeos-rebuild",
            ],
        );
        ink(&read(&path, &pcgeos::PcgeosCodec::default()));
        ok(&[
            "info",
            text(&path),
            "--from",
            "pcgeos",
            "--pcgeos-read-variant",
            variant,
        ]);
    }
    for version in ["xy", "4", "5", "6", "7", "8"] {
        for compression in if version == "xy" {
            &["raw"][..]
        } else {
            &["raw", "packed"][..]
        } {
            let path = dir.0.join(format!("riscos-{version}-{compression}.bin"));
            let sidecar = format!("riscos-{version}-{compression}.metrics");
            convert(
                &source,
                &path,
                "riscos",
                &[
                    "--native-unicode-as-legacy",
                    "--riscos-version",
                    version,
                    "--riscos-compression",
                    compression,
                    "--riscos-depth",
                    if version == "xy" { "gray4" } else { "mono" },
                    "--riscos-metrics",
                    &sidecar,
                    "--riscos-metrics-version",
                    "2",
                ],
            );
            assert!(dir.0.join(&sidecar).is_file());
            let codec = riscos::RiscosCodec {
                read_options: riscos::RiscosReadOptions {
                    metrics_path: Some(fontpixel::ResourcePath::new(&sidecar).unwrap()),
                    ..Default::default()
                },
                ..Default::default()
            };
            ink(&read(&path, &codec));
            ok(&[
                "info",
                text(&path),
                "--from",
                "riscos",
                "--riscos-read-version",
                version,
                "--riscos-read-metrics",
                &sidecar,
            ]);
        }
    }
    for dialect in ["davic", "freetype"] {
        for compression in ["raw", "rle4", "rle8"] {
            for black in ["0", "1"] {
                let path = dir
                    .0
                    .join(format!("pfr-{dialect}-{compression}-{black}.pfr"));
                convert(
                    &source,
                    &path,
                    "pfr",
                    &[
                        "--native-unicode-as-legacy",
                        "--pfr-dialect",
                        dialect,
                        "--pfr-compression",
                        compression,
                        "--pfr-black-pixel",
                        black,
                        "--pfr-top-down",
                    ],
                );
                ink(&read(&path, &pfr::PfrCodec::default()));
                ok(&[
                    "info",
                    text(&path),
                    "--from",
                    "pfr",
                    "--pfr-read-dialect",
                    dialect,
                ]);
            }
        }
    }
    for flavor in ["sfont", "rfont"] {
        let path = dir.0.join(format!("markers-{flavor}.png"));
        let mut opts = vec!["--sfont-flavor", flavor];
        if flavor == "sfont" {
            opts.extend(["--sfont-codepoints", "U+0041,U+0042"]);
        }
        convert(&source, &path, "sfont", &opts);
        let codec = sfont::SfontCodec {
            read_options: sfont::SfontReadOptions {
                codepoints: if flavor == "sfont" {
                    Some(vec![Codepoint::Unicode(65), Codepoint::Unicode(66)])
                } else {
                    None
                },
                ..Default::default()
            },
            ..Default::default()
        };
        ink(&read(&path, &codec));
        let mut args = vec![
            "info",
            text(&path),
            "--from",
            "sfont",
            "--sfont-read-flavor",
            flavor,
        ];
        if flavor == "sfont" {
            args.extend([
                "--sfont-read-codepoints",
                "U+0041,U+0042",
                "--sfont-read-baseline",
                "8",
            ]);
        }
        ok(&args);
    }
}

#[cfg(all(feature = "xerox", feature = "ssfn"))]
#[test]
fn xerox_families_and_ssfn_native_encodings() {
    use fontpixel::formats::{ssfn, xerox};
    let dir = Dir::new();
    let source = dir.source();
    for (variant, layout) in [
        ("alto", None),
        ("bitblt", Some("plain")),
        ("bitblt", Some("kerned")),
        ("bitblt", Some("plain-index")),
        ("bitblt", Some("kerned-index")),
        ("prepress", None),
    ] {
        let path = dir.0.join(format!(
            "xerox-{variant}-{}.bin",
            layout.unwrap_or("default")
        ));
        let mut options = vec!["--native-unicode-as-legacy", "--xerox-variant", variant];
        if let Some(layout) = layout {
            options.extend(["--xerox-bitblt-layout", layout]);
        }
        if variant == "alto" {
            options.extend(["--xerox-alto-baseline", "8"]);
        }
        if variant == "prepress" {
            options.extend(["--xerox-rotation-minutes", "0"]);
        }
        convert(&source, &path, "xerox", &options);
        ink(&read(&path, &xerox::XeroxCodec::default()));
        ok(&[
            "info",
            text(&path),
            "--from",
            "xerox",
            "--xerox-read-variant",
            variant,
        ]);
    }
    for encoding in ["binary", "ascii"] {
        let path = dir.0.join(format!("native-{encoding}.sfn"));
        convert(&source, &path, "ssfn", &["--ssfn-encoding", encoding]);
        let data = fs::read(&path).unwrap();
        assert!(data.starts_with(if encoding == "binary" {
            b"SFN2"
        } else {
            b"# Scalable Screen Font #"
        }));
        ink(&read(&path, &ssfn::SsfnCodec::default()));
        ok(&["info", text(&path), "--from", "ssfn"]);
    }
}

#[test]
fn text_and_legacy_options_reach_their_native_codecs() {
    let dir = Dir::new();
    let source = dir.source();
    for order in ["big", "little"] {
        let target = dir.0.join(format!("{order}.vfont"));
        convert(
            &source,
            &target,
            "vfont",
            &["--vfont-byte-order", order, "--vfont-unicode-as-legacy"],
        );
        let bytes = fs::read(&target).unwrap();
        assert_eq!(
            &bytes[..2],
            if order == "big" {
                &[1, 0x1e]
            } else {
                &[0x1e, 1]
            }
        );
        pixels(&read(&target, &vfont::VfontCodec::default()));
        ok(&[
            "info",
            text(&target),
            "--from",
            "vfont",
            "--vfont-read-byte-order",
            order,
        ]);
    }
    let fzx = dir.0.join("native.fzx");
    convert(
        &source,
        &fzx,
        "fzx",
        &[
            "--fzx-unicode-as-legacy",
            "--fzx-height",
            "8",
            "--fzx-tracking",
            "0",
        ],
    );
    assert_eq!(fs::read(&fzx).unwrap()[..2], [8, 0]);
    pixels(&read(&fzx, &fzx::FzxCodec::default()));
    let figlet = dir.0.join("native.flf");
    convert(
        &source,
        &figlet,
        "figlet",
        &["--figlet-height", "8", "--figlet-baseline", "8"],
    );
    pixels(&read(&figlet, &figlet::FigletCodec::default()));
}

#[test]
fn color_flavor_keeps_coverage_and_strict_keeps_the_previous_file() {
    let dir = Dir::new();
    let source = dir.source();
    for flavor in ["cbdt", "sbix"] {
        let target = dir.0.join(format!("{flavor}.ttf"));
        convert(&source, &target, "color", &["--color-flavor", flavor]);
        let bytes = fs::read(&target).unwrap();
        assert!(
            bytes
                .windows(4)
                .any(|tag| tag == if flavor == "cbdt" { b"CBDT" } else { b"sbix" })
        );
        let (fonts, _) = embedded::read(
            &bytes,
            fontpixel::sfnt::bitmap::Kind::Color,
            &Default::default(),
        )
        .unwrap();
        pixels(&fonts);
        let result = run(&[
            "convert",
            text(&source),
            "-o",
            text(&target),
            "--to",
            "color",
            "--color-flavor",
            flavor,
            "--strict",
        ]);
        assert!(!result.status.success());
        assert_eq!(fs::read(&target).unwrap(), bytes);
    }
}

#[test]
fn iigs_versions_and_binscii_preserve_pixels() {
    let dir = Dir::new();
    let source = dir.0.join("source.raw");
    fs::write(&source, ROWS.concat()).unwrap();
    for version in ["1.1", "1.5"] {
        for container in ["bare", "binscii"] {
            let target = dir.0.join(format!("{version}-{container}.iigs"));
            convert(
                &source,
                &target,
                "iigs",
                &[
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
                    "--iigs-version",
                    version,
                    "--iigs-container",
                    container,
                ],
            );
            pixels(&read(&target, &iigs::IigsCodec::default()));
            ok(&[
                "info",
                text(&target),
                "--from",
                "iigs",
                "--iigs-read-charset",
                "mac-roman",
            ]);
        }
    }
}

#[test]
fn hanfont_headerless_components_require_explicit_selection() {
    let dir = Dir::new();
    let source = dir.0.join("components.unknown");
    let bytes: Vec<_> = (0..11520).map(|i| (i * 37 + i / 32) as u8).collect();
    fs::write(&source, &bytes).unwrap();
    assert!(!run(&["info", text(&source)]).status.success());
    let target = dir.0.join("rewritten.han");
    convert(
        &source,
        &target,
        "hanfont",
        &["--from", "hanfont", "--hanfont-read-mode", "components"],
    );
    assert_eq!(fs::read(&target).unwrap(), bytes);
    for mode in ["syllables", "components"] {
        ok(&[
            "info",
            text(&source),
            "--from",
            "hanfont",
            "--hanfont-read-mode",
            mode,
        ]);
    }
}

#[test]
fn incompatible_options_fail_before_writing() {
    let dir = Dir::new();
    let source = dir.source();
    let target = dir.0.join("keep.bdf");
    fs::write(&target, b"original").unwrap();
    for options in [
        vec!["--vfont-byte-order", "big"],
        vec!["--fzx-height", "8"],
        vec!["--color-flavor", "sbix"],
        vec!["--iigs-container", "binscii"],
    ] {
        let mut args = vec!["convert", text(&source), "-o", text(&target), "--to", "bdf"];
        args.extend(options);
        assert!(!run(&args).status.success());
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }
}

#[cfg(all(feature = "palm", feature = "gdr"))]
#[test]
fn palm_wrappers_and_gdr_metric_versions_reach_native_writers() {
    use fontpixel::formats::{gdr, palm};
    let dir = Dir::new();
    let source = dir.source();
    for version in ["1", "2", "afnx", "fontsubst", "gray1", "gray2"] {
        for container in ["bare", "pdb", "prc"] {
            let target = dir.0.join(format!("{version}-{container}.palm"));
            if version.starts_with("gray") && container != "prc" {
                let result = run(&[
                    "convert",
                    text(&source),
                    "-o",
                    text(&target),
                    "--to",
                    "palm",
                    "--palm-version",
                    version,
                    "--palm-container",
                    container,
                    "--palm-unicode-as-legacy",
                ]);
                assert!(!result.status.success());
                assert!(!target.exists());
                continue;
            }
            convert(
                &source,
                &target,
                "palm",
                &[
                    "--palm-version",
                    version,
                    "--palm-container",
                    container,
                    "--palm-unicode-as-legacy",
                ],
            );
            pixels(&read(&target, &palm::PalmCodec::default()));
            ok(&[
                "info",
                text(&target),
                "--from",
                "palm",
                "--palm-read-container",
                container,
            ]);
        }
    }
    for version in ["39", "40", "41", "42"] {
        let target = dir.0.join(format!("{version}.gdr"));
        convert(
            &source,
            &target,
            "gdr",
            &[
                "--gdr-version",
                version,
                "--gdr-collection-uid",
                "0x10203040",
            ],
        );
        pixels(&read(&target, &gdr::GdrCodec::default()));
    }
}

#[cfg(feature = "c")]
#[test]
fn embedded_c_families_and_lvgl_binary_preserve_coverage() {
    use fontpixel::formats::c::{self, CCodec, CReadOptions, Family};
    let dir = Dir::new();
    let source = dir.source();
    for (name, family) in [
        ("u8g2", Family::U8g2),
        ("netbsd", Family::Netbsd),
        ("u8glib", Family::U8glib),
        ("u8x8", Family::U8x8),
    ] {
        let target = dir.0.join(format!("{name}.c"));
        convert(&source, &target, "c", &["--c-family", name]);
        pixels(&read(
            &target,
            &CCodec {
                read_options: CReadOptions {
                    family,
                    ..Default::default()
                },
                ..Default::default()
            },
        ));
        ok(&[
            "info",
            text(&target),
            "--from",
            "c",
            "--c-read-family",
            name,
        ]);
    }
    for container in ["source", "lvgl-binary"] {
        for bpp in ["1", "2", "4", "8"] {
            for compression in ["plain", "rle", "rle-xor"] {
                let target = dir
                    .0
                    .join(format!("lvgl-{container}-{bpp}-{compression}.font"));
                convert(
                    &source,
                    &target,
                    "c",
                    &[
                        "--c-family",
                        "lvgl",
                        "--c-container",
                        container,
                        "--lvgl-bpp",
                        bpp,
                        "--lvgl-compression",
                        compression,
                    ],
                );
                pixels(&read(
                    &target,
                    &CCodec {
                        read_options: CReadOptions {
                            family: Family::Lvgl,
                            container: Some(if container == "source" {
                                c::CContainer::Source
                            } else {
                                c::CContainer::LvglBinary
                            }),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                ));
                ok(&[
                    "info",
                    text(&target),
                    "--from",
                    "c",
                    "--c-read-family",
                    "lvgl",
                    "--c-read-container",
                    container,
                ]);
            }
        }
    }
    for format in ["full", "compact", "gray2"] {
        let target = dir.0.join(format!("u8glib-{format}.c"));
        convert(
            &source,
            &target,
            "c",
            &["--c-family", "u8glib", "--u8glib-format", format],
        );
        pixels(&read(
            &target,
            &CCodec {
                read_options: CReadOptions {
                    family: Family::U8glib,
                    ..Default::default()
                },
                ..Default::default()
            },
        ));
    }
}

#[cfg(feature = "drcs")]
#[test]
fn drcs_terminal_options_preserve_explicit_cells() {
    use fontpixel::formats::drcs::{DrcsCodec, DrcsReadOptions, Terminal};
    let dir = Dir::new();
    let source = dir.0.join("source.raw");
    fs::write(&source, ROWS.concat()).unwrap();
    for terminal in ["vt320", "vt510"] {
        for charset in ["94", "96"] {
            let target = dir.0.join(format!("{terminal}-{charset}.drcs"));
            convert(
                &source,
                &target,
                "drcs",
                &[
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
                    "ascii",
                    "--drcs-terminal",
                    terminal,
                    "--drcs-charset",
                    charset,
                    "--drcs-cell-width",
                    "8",
                    "--drcs-cell-height",
                    "8",
                    "--drcs-eight-bit-controls",
                ],
            );
            pixels(&read(
                &target,
                &DrcsCodec {
                    read_options: DrcsReadOptions {
                        terminal: if terminal == "vt320" {
                            Terminal::Vt320
                        } else {
                            Terminal::Vt510
                        },
                        baseline: Some(8),
                    },
                    ..Default::default()
                },
            ));
        }
    }
}

#[cfg(all(
    feature = "dosstart",
    feature = "edwin",
    feature = "mkwinfont",
    feature = "psf2txt",
    feature = "pcr",
    feature = "plus3dos",
    feature = "drhalo",
    feature = "writeon"
))]
#[test]
fn small_native_formats_share_explicit_cell_and_code_projection_options() {
    let dir = Dir::new();
    let source = dir.source();
    for id in [
        "dosstart",
        "edwin",
        "mkwinfont",
        "psf2txt",
        "pcr",
        "plus3dos",
        "drhalo",
        "writeon",
    ] {
        let target = dir.0.join(format!("native.{id}"));
        let height = if id == "writeon" { "14" } else { "8" };
        convert(
            &source,
            &target,
            id,
            &[
                "--native-unicode-as-legacy",
                "--native-cell-width",
                "8",
                "--native-cell-height",
                height,
            ],
        );
        let bytes = fs::read(&target).unwrap();
        let loaded = fontpixel::read(
            &bytes,
            &ReadOptions {
                format: Some(id.into()),
                ..Default::default()
            },
        )
        .unwrap();
        if id == "writeon" {
            for (i, rows) in ROWS.iter().enumerate() {
                let glyph = loaded
                    .fonts
                    .single()
                    .unwrap()
                    .glyph_for_codepoint(Codepoint::Legacy(65 + i as u32))
                    .unwrap();
                assert_eq!((glyph.bitmap.width(), glyph.bitmap.height()), (8, 14));
                assert_eq!(
                    glyph.bitmap.as_bytes(),
                    [vec![0; 6], rows.to_vec()].concat()
                );
                assert_eq!(glyph.metrics.advance.unwrap().x, 8);
            }
        } else {
            pixels(&loaded.fonts);
        }
        ok(&["info", text(&target), "--from", id]);
    }
    let target = dir.0.join("invalid.txt");
    assert!(
        !run(&[
            "convert",
            text(&source),
            "-o",
            text(&target),
            "--to",
            "dosstart",
            "--native-cell-width",
            "8"
        ])
        .status
        .success()
    );
    assert!(!target.exists());
}
