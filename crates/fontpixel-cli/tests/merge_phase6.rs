//! Reconstructed W6.6 integration regressions; validation pending.
//! Historical acceptance logs do not apply to this newly reconstructed source.
#![cfg(feature = "bdf")]
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "fontpixel-merge-reconstructed-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fontpixel"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn bdf(&self, file: &str, family: &str, glyphs: &[(u32, i64, &str)]) {
        let mut data = format!(
            "STARTFONT 2.1\nFONT -Test-{family}-Medium-R-Normal--8-80-75-75-P-40-ISO10646-1\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 1 0 0\nSTARTPROPERTIES 5\nFAMILY_NAME \"{family}\"\nFONT_ASCENT 7\nFONT_DESCENT 1\nSPACING \"P\"\nCOPYRIGHT \"MIT License\"\nENDPROPERTIES\nCHARS {}\n",
            glyphs.len()
        );
        for (code, advance, bits) in glyphs {
            data.push_str(&format!("STARTCHAR uni{code:04X}\nENCODING {code}\nSWIDTH {} 0\nDWIDTH {advance} 0\nBBX 8 1 0 0\nBITMAP\n{bits}\nENDCHAR\n", advance * 125));
        }
        data.push_str("ENDFONT\n");
        fs::write(self.0.join(file), data).unwrap();
    }
    fn prepare(&self) {
        self.bdf("cjk.bdf", "Regional", &[(0x4E00, 8, "FF"), (0x41, 4, "80")]);
        self.bdf("latin.bdf", "Latin", &[(0x41, 4, "40"), (0x42, 4, "20")]);
    }
    fn success(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn ordered_part_scoping_and_signed_shift() {
    let f = Fixture::new();
    f.prepare();
    let report = f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "cjk",
        "--from",
        "bdf",
        "--part",
        "latin.bdf",
        "--take",
        "western",
        "--dy=-1",
        "--spacing",
        "prop",
        "-o",
        "mix.bdf",
        "--report",
        "json",
    ]);
    assert_eq!(report["glyphs"], 3);
    assert_eq!(report["merge"]["parts"][0]["dy"], 0);
    assert_eq!(report["merge"]["parts"][1]["dy"], -1);
}
#[test]
fn parser_rejects_unbound_or_repeated_part_flags() {
    let f = Fixture::new();
    for prefix in [
        vec!["--take", "all"],
        vec!["--dy=-1"],
        vec!["--from", "bdf"],
        vec!["--font-index", "0"],
    ] {
        let mut args = vec!["merge", "--family", "Mix"];
        args.extend(prefix);
        args.extend(["--part", "missing.bdf", "--take", "all", "-o", "out.bdf"]);
        assert_eq!(f.run(&args).status.code(), Some(2), "{args:?}");
    }
    for tail in [
        vec!["--take", "all"],
        vec!["--dy", "1", "--dy", "2"],
        vec!["--from", "bdf", "--from", "pcf"],
        vec!["--font-index", "0", "--font-index", "1"],
    ] {
        let mut args = vec![
            "merge",
            "--family",
            "Mix",
            "--part",
            "missing.bdf",
            "--take",
            "all",
        ];
        args.extend(tail);
        args.extend(["-o", "out.bdf"]);
        assert_eq!(f.run(&args).status.code(), Some(2), "{args:?}");
    }
}
#[test]
fn parser_requires_selection_and_nonempty_family() {
    let f = Fixture::new();
    for args in [
        vec![
            "merge",
            "--family",
            "Mix",
            "--part",
            "missing.bdf",
            "-o",
            "out.bdf",
        ],
        vec![
            "merge",
            "--family",
            "   ",
            "--part",
            "missing.bdf",
            "--take",
            "all",
            "-o",
            "out.bdf",
        ],
        vec![
            "merge",
            "--family",
            "Mix",
            "--part",
            "missing.bdf",
            "--take",
            "U+ZZZZ",
            "-o",
            "out.bdf",
        ],
        vec![
            "merge",
            "--family",
            "Mix",
            "--to",
            "bdf",
            "--part",
            "missing.bdf",
            "--take",
            "all",
            "-o",
            "out.bdf",
        ],
    ] {
        assert_eq!(f.run(&args).status.code(), Some(2), "{args:?}");
    }
}
#[test]
fn first_present_source_wins() {
    let f = Fixture::new();
    f.prepare();
    f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--part",
        "latin.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "-o",
        "mix.bdf",
        "--report",
        "json",
    ]);
    let text = fs::read_to_string(f.0.join("mix.bdf")).unwrap();
    let a = text
        .split("ENCODING 65\n")
        .nth(1)
        .unwrap()
        .split("ENDCHAR")
        .next()
        .unwrap();
    assert!(a.contains("BITMAP\n80\n"));
}
#[test]
fn fallback_is_opt_in() {
    let f = Fixture::new();
    f.prepare();
    let without = f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "U+0041-0042",
        "--part",
        "latin.bdf",
        "--take",
        "U+0043",
        "--spacing",
        "prop",
        "-o",
        "without.bdf",
        "--report",
        "json",
    ]);
    let with = f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "U+0041-0042",
        "--part",
        "latin.bdf",
        "--take",
        "U+0043",
        "--spacing",
        "prop",
        "--fallback",
        "-o",
        "with.bdf",
        "--report",
        "json",
    ]);
    assert_eq!(without["glyphs"], 1);
    assert_eq!(with["glyphs"], 2);
    assert_eq!(with["merge"]["parts"][1]["fallback_codepoints"], 1);
}
#[test]
fn later_output_error_preserves_earlier_file() {
    let f = Fixture::new();
    f.prepare();
    fs::write(f.0.join("out.bdf"), b"sentinel").unwrap();
    let result = f.run(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "-o",
        "out.bdf",
        "-o",
        "unknown.ambiguous",
    ]);
    assert!(!result.status.success());
    assert_eq!(fs::read(f.0.join("out.bdf")).unwrap(), b"sentinel");
}
#[test]
fn input_is_protected_even_when_output_is_identical_path() {
    let f = Fixture::new();
    f.prepare();
    let before = fs::read(f.0.join("cjk.bdf")).unwrap();
    assert!(
        !f.run(&[
            "merge",
            "--family",
            "Mix",
            "--part",
            "cjk.bdf",
            "--take",
            "all",
            "--spacing",
            "prop",
            "-o",
            "cjk.bdf"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(f.0.join("cjk.bdf")).unwrap(), before);
}
#[test]
fn duplicate_outputs_are_rejected_transactionally() {
    let f = Fixture::new();
    f.prepare();
    fs::write(f.0.join("out.bdf"), b"sentinel").unwrap();
    assert!(
        !f.run(&[
            "merge",
            "--family",
            "Mix",
            "--part",
            "cjk.bdf",
            "--take",
            "all",
            "--spacing",
            "prop",
            "-o",
            "out.bdf",
            "-o",
            "./out.bdf"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(f.0.join("out.bdf")).unwrap(), b"sentinel");
}
#[cfg(unix)]
#[test]
fn input_hardlink_alias_is_protected() {
    let f = Fixture::new();
    f.prepare();
    let before = fs::read(f.0.join("cjk.bdf")).unwrap();
    fs::hard_link(f.0.join("cjk.bdf"), f.0.join("alias.bdf")).unwrap();
    assert!(
        !f.run(&[
            "merge",
            "--family",
            "Mix",
            "--part",
            "cjk.bdf",
            "--take",
            "all",
            "--spacing",
            "prop",
            "-o",
            "alias.bdf"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(f.0.join("cjk.bdf")).unwrap(), before);
}
#[test]
fn resource_failure_protects_existing_output() {
    let f = Fixture::new();
    f.prepare();
    fs::write(f.0.join("out.bdf"), b"sentinel").unwrap();
    assert!(
        !f.run(&[
            "merge",
            "--family",
            "Mix",
            "--part",
            "cjk.bdf",
            "--take",
            "all",
            "--spacing",
            "prop",
            "--max-total-bytes",
            "64",
            "-o",
            "out.bdf"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(f.0.join("out.bdf")).unwrap(), b"sentinel");
}
#[cfg(feature = "pcf")]
#[test]
fn scoped_formats_write_real_bdf_and_pcf() {
    let f = Fixture::new();
    f.prepare();
    f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "-o",
        "first.data",
        "--to",
        "bdf",
        "-o",
        "second.data",
        "--to",
        "pcf",
        "--report",
        "json",
    ]);
    assert!(
        fs::read(f.0.join("first.data"))
            .unwrap()
            .starts_with(b"STARTFONT")
    );
    assert!(
        fs::read(f.0.join("second.data"))
            .unwrap()
            .starts_with(b"\x01fcp")
    );
}
#[cfg(feature = "pcf")]
#[test]
fn true_batch_merge_supports_output_formats_and_relative_inputs() {
    let f = Fixture::new();
    f.prepare();
    fs::write(
        f.0.join("jobs.toml"),
        r#"version = 1
[[tasks]]
id = "mix"
kind = "merge"
family = "Mix"
outputs = ["first.data", "second.data"]
output_formats = ["bdf", "pcf"]
[[tasks.parts]]
input = "cjk.bdf"
take = "cjk"
[[tasks.parts]]
input = "latin.bdf"
take = "western"
[tasks.options]
spacing = "prop"
"#,
    )
    .unwrap();
    let result = f.success(&["batch", "jobs.toml"]);
    assert_eq!(result["tasks"][0]["committed"], true);
    assert_eq!(result["summary"]["committed"], 1);
    assert_eq!(
        result["tasks"][0]["result"]["merge"]["parts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        fs::read(f.0.join("first.data"))
            .unwrap()
            .starts_with(b"STARTFONT")
    );
    assert!(
        fs::read(f.0.join("second.data"))
            .unwrap()
            .starts_with(b"\x01fcp")
    );
    f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "cjk",
        "--part",
        "latin.bdf",
        "--take",
        "western",
        "--spacing",
        "prop",
        "-o",
        "standalone.bdf",
        "-o",
        "standalone.pcf",
        "--report",
        "json",
    ]);
    assert_eq!(
        fs::read(f.0.join("first.data")).unwrap(),
        fs::read(f.0.join("standalone.bdf")).unwrap()
    );
    assert_eq!(
        fs::read(f.0.join("second.data")).unwrap(),
        fs::read(f.0.join("standalone.pcf")).unwrap()
    );
    let bad = fs::read_to_string(f.0.join("jobs.toml"))
        .unwrap()
        .replace("[\"bdf\", \"pcf\"]", "[\"bdf\"]");
    let first = fs::read(f.0.join("first.data")).unwrap();
    let second = fs::read(f.0.join("second.data")).unwrap();
    fs::write(f.0.join("bad.toml"), bad).unwrap();
    assert!(!f.run(&["batch", "bad.toml"]).status.success());
    assert_eq!(fs::read(f.0.join("first.data")).unwrap(), first);
    assert_eq!(fs::read(f.0.join("second.data")).unwrap(), second);
}
#[test]
fn relocating_input_does_not_change_font_bytes() {
    let f = Fixture::new();
    f.prepare();
    fs::copy(f.0.join("cjk.bdf"), f.0.join("relocated.bdf")).unwrap();
    for (input, output) in [("cjk.bdf", "one.bdf"), ("relocated.bdf", "two.bdf")] {
        f.success(&[
            "merge",
            "--family",
            "Mix",
            "--part",
            input,
            "--take",
            "all",
            "--spacing",
            "prop",
            "-o",
            output,
            "--report",
            "json",
        ]);
    }
    assert_eq!(
        fs::read(f.0.join("one.bdf")).unwrap(),
        fs::read(f.0.join("two.bdf")).unwrap()
    );
}
#[test]
fn help_explains_part_binding_and_examples() {
    let f = Fixture::new();
    let result = f.run(&["merge", "--help"]);
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    for word in [
        "--part",
        "--take",
        "--dy",
        "--family",
        "--geometric-width",
        "Examples",
    ] {
        assert!(text.contains(word), "missing {word}: {text}");
    }
}

#[test]
fn unknown_license_strict_failure_preserves_output() {
    let f = Fixture::new();
    f.prepare();
    let source = fs::read_to_string(f.0.join("cjk.bdf"))
        .unwrap()
        .replace("MIT License", "Custom license of unknown permissions");
    fs::write(f.0.join("cjk.bdf"), source).unwrap();
    fs::write(f.0.join("out.bdf"), b"sentinel").unwrap();
    let result = f.run(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "--strict",
        "-o",
        "out.bdf",
        "--report",
        "json",
    ]);
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(fs::read(f.0.join("out.bdf")).unwrap(), b"sentinel");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("merge.license.unknown")
            || String::from_utf8_lossy(&result.stdout).contains("merge.license.unknown")
    );
}
#[test]
fn explicit_metrics_and_narrow_mono_padding_are_reported() {
    let f = Fixture::new();
    f.prepare();
    f.bdf("latin.bdf", "Latin", &[(0x41, 2, "80")]);
    let report = f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "cjk",
        "--part",
        "latin.bdf",
        "--take",
        "western",
        "--dy=-2",
        "--spacing",
        "mono",
        "--ascent",
        "9",
        "--descent",
        "4",
        "-o",
        "out.bdf",
        "--report",
        "json",
    ]);
    assert_eq!(report["merge"]["full_width"], 8);
    assert_eq!(report["merge"]["metrics"]["ascent"], 9);
    assert_eq!(report["merge"]["metrics"]["descent"], 4);
    let adjusted = report["merge"]["adjustments"].as_array().unwrap();
    assert!(
        adjusted.iter().any(|a| a["original_advance"] == 2
            && a["target_advance"] == 4
            && a["kind"] == "padded")
    );
    let data = fs::read(f.0.join("out.bdf")).unwrap();
    let loaded = fontpixel::read(&data, &fontpixel::ReadOptions::default()).unwrap();
    let glyph = loaded.fonts.fonts[0]
        .glyph_for_codepoint(fontpixel::model::Codepoint::Unicode(0x41))
        .unwrap();
    assert_eq!(glyph.metrics.advance.unwrap().x, 4);
    assert_eq!(glyph.bitmap.as_bytes(), &[0x80]);
    assert_eq!(glyph.metrics.bounds.y, -2);
}
#[test]
fn mono_too_wide_requires_explicit_allowance() {
    let f = Fixture::new();
    f.prepare();
    f.bdf("latin.bdf", "Latin", &[(0x41, 5, "80")]);
    let args = [
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "cjk",
        "--part",
        "latin.bdf",
        "--take",
        "western",
        "--spacing",
        "mono",
        "-o",
        "out.bdf",
        "--report",
        "json",
    ];
    assert!(!f.run(&args).status.success());
    assert!(!f.0.join("out.bdf").exists());
    let mut allowed = args.to_vec();
    allowed.push("--allow-misaligned");
    let report = f.success(&allowed);
    assert!(
        report["merge"]["adjustments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "misaligned" && a["original_advance"] == 5)
    );
}
#[test]
fn auto_requires_every_part_to_declare_mono() {
    let f = Fixture::new();
    f.prepare();
    let args = [
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "cjk",
        "--part",
        "latin.bdf",
        "--take",
        "western",
        "-o",
        "out.bdf",
        "--report",
        "json",
    ];
    assert_eq!(f.success(&args)["merge"]["effective_spacing"], "prop");
    for file in ["cjk.bdf", "latin.bdf"] {
        let source = fs::read_to_string(f.0.join(file))
            .unwrap()
            .replace("SPACING \"P\"", "SPACING \"M\"");
        fs::write(f.0.join(file), source).unwrap();
    }
    assert_eq!(f.success(&args)["merge"]["effective_spacing"], "mono");
}
#[test]
fn geometric_width_is_explicit_and_preserves_full_policy_source_metrics() {
    let f = Fixture::new();
    f.prepare();
    f.bdf("latin.bdf", "Latin", &[(0x2500, 2, "FF")]);
    for (policy, expected) in [("half", 4), ("full", 2)] {
        f.success(&[
            "merge",
            "--family",
            "Mix",
            "--part",
            "cjk.bdf",
            "--take",
            "cjk",
            "--part",
            "latin.bdf",
            "--take",
            "geometric",
            "--spacing",
            "mono",
            "--geometric-width",
            policy,
            "-o",
            "out.bdf",
            "--report",
            "json",
        ]);
        let data = fs::read(f.0.join("out.bdf")).unwrap();
        let loaded = fontpixel::read(&data, &fontpixel::ReadOptions::default()).unwrap();
        let glyph = loaded.fonts.fonts[0]
            .glyph_for_codepoint(fontpixel::model::Codepoint::Unicode(0x2500))
            .unwrap();
        assert_eq!(glyph.metrics.advance.unwrap().x, expected);
    }
}
#[cfg(feature = "yaff")]
#[test]
fn collections_require_an_explicit_valid_font_index() {
    let f = Fixture::new();
    f.prepare();
    let converted = f.run(&["convert", "cjk.bdf", "latin.bdf", "-o", "collection.yaff"]);
    assert!(
        converted.status.success(),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    let base = [
        "merge",
        "--family",
        "Mix",
        "--part",
        "collection.yaff",
        "--take",
        "all",
        "--spacing",
        "prop",
        "-o",
        "out.bdf",
        "--report",
        "json",
    ];
    assert!(!f.run(&base).status.success());
    let mut chosen = base.to_vec();
    chosen.extend(["--font-index", "1"]);
    let report = f.success(&chosen);
    assert_eq!(report["merge"]["parts"][0]["label"], "Latin");
    chosen.pop();
    chosen.push("99");
    assert!(!f.run(&chosen).status.success());
}
#[cfg(all(feature = "pcf", feature = "opentype", feature = "otb"))]
#[test]
fn seven_outputs_are_real_font_containers() {
    let f = Fixture::new();
    f.prepare();
    let names = [
        "out.bdf",
        "out.pcf",
        "out.ttf",
        "out.otf",
        "out.woff",
        "out.woff2",
        "out.otb",
    ];
    let mut args = vec![
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "--report",
        "json",
    ];
    for name in &names {
        args.extend(["-o", *name]);
    }
    let report = f.success(&args);
    assert_eq!(report["outputs"].as_array().unwrap().len(), 7);
    for (name, signature) in names.iter().zip([
        &b"STARTFONT"[..],
        &b"\x01fcp"[..],
        &b"\x00\x01\x00\x00"[..],
        &b"OTTO"[..],
        &b"wOFF"[..],
        &b"wOF2"[..],
        &b"\x00\x01\x00\x00"[..],
    ]) {
        assert!(
            fs::read(f.0.join(name)).unwrap().starts_with(signature),
            "{name}"
        );
    }
}
#[cfg(feature = "opentype")]
#[test]
fn mixed_bitmap_vector_group_accepts_dot_outline_options() {
    let f = Fixture::new();
    f.prepare();
    f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "--shape",
        "dot",
        "--ppu",
        "100",
        "-o",
        "out.bdf",
        "-o",
        "out.ttf",
        "--report",
        "json",
    ]);
    assert!(
        fs::read(f.0.join("out.bdf"))
            .unwrap()
            .starts_with(b"STARTFONT")
    );
    assert!(
        fs::read(f.0.join("out.ttf"))
            .unwrap()
            .starts_with(b"\x00\x01\x00\x00")
    );
}
#[cfg(feature = "yaff")]
#[test]
fn strict_later_writer_loss_protects_the_whole_group() {
    let f = Fixture::new();
    f.prepare();
    // Recognized MIT declarations permit merging. BDF's multiline notice
    // projection is separately and explicitly lossy; YAFF retains the metadata.
    f.success(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "--strict",
        "-o",
        "ok.yaff",
        "--report",
        "json",
    ]);
    fs::write(f.0.join("first.yaff"), b"first sentinel").unwrap();
    fs::write(f.0.join("second.bdf"), b"second sentinel").unwrap();
    let result = f.run(&[
        "merge",
        "--family",
        "Mix",
        "--part",
        "cjk.bdf",
        "--take",
        "all",
        "--spacing",
        "prop",
        "--strict",
        "-o",
        "first.yaff",
        "-o",
        "second.bdf",
        "--report",
        "json",
    ]);
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(fs::read(f.0.join("first.yaff")).unwrap(), b"first sentinel");
    assert_eq!(
        fs::read(f.0.join("second.bdf")).unwrap(),
        b"second sentinel"
    );
}
#[cfg(feature = "image")]
#[test]
fn batch_native_directory_outputs_reject_stale_contents() {
    let f = Fixture::new();
    f.prepare();
    let job = r#"version = 1
[[tasks]]
id = "images"
kind = "merge"
family = "Mix"
outputs = ["sprites"]
output_formats = ["image"]
[[tasks.parts]]
input = "latin.bdf"
take = "western"
[tasks.options]
spacing = "prop"
image_layout = "imageset"
image_domain = "unicode"
image_encoding = "png"
"#;
    fs::write(f.0.join("images.toml"), job).unwrap();
    assert_eq!(
        f.success(&["batch", "images.toml"])["tasks"][0]["committed"],
        true
    );
    let entries: Vec<_> = fs::read_dir(f.0.join("sprites"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 2);
    for file in &entries {
        assert!(fs::read(file).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
    }
    fs::write(f.0.join("sprites/stale.txt"), b"keep").unwrap();
    let originals: Vec<_> = entries.iter().map(|file| fs::read(file).unwrap()).collect();
    assert!(!f.run(&["batch", "images.toml"]).status.success());
    assert_eq!(fs::read(f.0.join("sprites/stale.txt")).unwrap(), b"keep");
    for (file, bytes) in entries.iter().zip(originals) {
        assert_eq!(fs::read(file).unwrap(), bytes);
    }
}
#[test]
fn output_bundle_owned_capacity_accounting_is_exact_at_the_boundary() {
    use fontpixel::{OutputAsset, OutputBundle, ResourcePath};
    let mut primary = String::with_capacity(37);
    primary.push_str("out.bdf");
    let primary_capacity = primary.capacity();
    let mut path = String::with_capacity(71);
    path.push_str("out.bdf");
    let path_capacity = path.capacity();
    let mut bytes = Vec::with_capacity(129);
    bytes.push(7);
    let byte_capacity = bytes.capacity();
    let mut assets = Vec::with_capacity(5);
    let asset_capacity = assets.capacity();
    assets.push(OutputAsset {
        path: ResourcePath::new(path).unwrap(),
        bytes,
    });
    let bundle = OutputBundle::new(ResourcePath::new(primary).unwrap(), assets).unwrap();
    let expected = std::mem::size_of::<OutputBundle>()
        + primary_capacity
        + asset_capacity * std::mem::size_of::<OutputAsset>()
        + path_capacity
        + byte_capacity;
    assert_eq!(bundle.retained_bytes(expected).unwrap(), expected);
    assert!(matches!(
        bundle.retained_bytes(expected - 1),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    assert!(matches!(
        bundle.retained_bytes(0),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}
#[test]
fn output_bundle_capacity_accounting_covers_directory_assets_and_empty_payloads() {
    use fontpixel::{OutputAsset, OutputBundle, ResourcePath};
    let assets = vec![OutputAsset {
        path: ResourcePath::new("sprites/0041.png").unwrap(),
        bytes: Vec::new(),
    }];
    let bundle = OutputBundle::directory(ResourcePath::new("sprites").unwrap(), assets).unwrap();
    let count = bundle.retained_bytes(usize::MAX).unwrap();
    assert!(
        count
            >= std::mem::size_of::<OutputBundle>()
                + std::mem::size_of::<OutputAsset>()
                + "sprites".len()
                + "sprites/0041.png".len()
    );
    assert_eq!(bundle.retained_bytes(count).unwrap(), count);
    assert!(bundle.retained_bytes(count - 1).is_err());
}
