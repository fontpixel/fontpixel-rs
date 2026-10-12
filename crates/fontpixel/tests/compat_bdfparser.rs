//! Direct Rust goldens and attachment/resource invariants for SPEC-v3.
#![cfg(feature = "compat-bdfparser")]
use fontpixel::compat::bdfparser::{
    Bitmap, CreateOptions, Font, outline,
    ttf::{self, TtfOptions},
};
use serde_json::{Value, json};
const MINI: &str = include_str!("../../../tests/data/compat/mini.bdf");
fn mini() -> Font {
    let mut f = Font::default();
    f.load_string(MINI).unwrap();
    f
}
#[test]
fn exact_reference_bdf_svg_and_ttf_goldens() {
    let f = mini();
    assert_eq!(f.tobdf().unwrap(), MINI);
    assert_eq!(
        f.tosvgfont(0.0, 0.0, false, "hexncr").unwrap(),
        include_str!("../../../tests/data/compat/mini.svg")
    );
    let manifest: Value =
        serde_json::from_str(include_str!("../../../tests/data/compat/ttf.json")).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/compat");
    for rec in manifest.as_array().unwrap() {
        let o = &rec["options"];
        let options = TtfOptions {
            shape: o["shape"].as_str().unwrap().into(),
            dotsize: o["dotsize"].as_f64().unwrap(),
            ppu: 64,
            metadata: o["metadata"].as_bool().unwrap(),
            timestamp: 0,
        };
        let name = rec["name"].as_str().unwrap();
        let bytes = f.tottf(&options).unwrap();
        assert_eq!(
            bytes,
            std::fs::read(dir.join(format!("{name}.ttf"))).unwrap(),
            "{name}"
        );
        assert_eq!(
            ttf::read_ttf(&bytes, Some(8)).unwrap().tobdf().unwrap(),
            std::fs::read_to_string(dir.join(format!("{name}.bdf"))).unwrap(),
            "{name}"
        );
    }
}
#[test]
fn stale_handles_merge_changed_fields_and_do_not_delete_replacements() {
    let mut f = mini();
    let mut a = f.glyphbycp(65).unwrap();
    let mut b = f.glyphbycp(65).unwrap();
    b.shift(2, 3).unwrap();
    a.setmeta(json!({"dwx0":9}).as_object().unwrap()).unwrap();
    assert_eq!(a.meta()[4], json!(2));
    assert_eq!(a.meta()[8], json!(9));
    f.newglyph(65, None, 0, None, None).unwrap();
    a.shift(1, 1).unwrap();
    f.remove_handle(&mut a);
    assert!(f.glyphbycp(65).is_some());
    assert!(!a.is_attached());
}
#[test]
fn detached_clone_removal_follows_python_reference() {
    let mut f = mini();
    let mut detached = f.glyphbycp(65).unwrap().clone();
    f.remove_handle(&mut detached);
    assert!(f.glyphbycp(65).is_none());
}
#[test]
fn clones_and_subsets_own_independent_native_models() {
    let f = mini();
    let mut c = f.clone();
    c.removeglyph(65);
    assert_eq!(f.length(), 3);
    assert_eq!(c.length(), 2);
    let subset = f.subset(&json!([65, 66])).unwrap();
    assert_eq!(subset.length(), 2);
    assert_eq!(subset.core().len(), 2);
}
#[test]
fn integer_rows_are_exact_beyond_64_bits() {
    let b = Bitmap::new(&["1".repeat(128)]).unwrap();
    assert_eq!(
        b.todata(5).unwrap().to_string(),
        "[340282366920938463463374607431768211455]"
    );
    assert_eq!(
        b.todata(4).unwrap(),
        json!(["ffffffffffffffffffffffffffffffff"])
    );
}
#[test]
fn invalid_ragged_input_is_rejected_without_changing_raster() {
    let mut b = Bitmap::new(&["110".into(), "010".into()]).unwrap();
    let before = b.clone();
    assert!(b.replace("11", "").is_err());
    assert_eq!(b, before);
    assert!(Bitmap::new(&["110".into(), "1".into()]).is_err());
    assert!(b.bold(-1).is_err());
}
#[test]
fn empty_shapes_and_fromstring_whitespace() {
    assert_eq!(Bitmap::blank(7, 0).unwrap().width(), 0);
    assert_eq!(
        Bitmap::fromstring("\n \r\n#.\n.#\n \n").unwrap().bindata(),
        vec!["10", "01"]
    );
    assert!(Bitmap::fromstring(" #\n##").is_err());
    let mut b = Bitmap::blank(0, 2).unwrap();
    b.rotate(90).unwrap();
    assert_eq!(b.bindata(), Vec::<String>::new());
}
#[test]
fn outline_holes_pinches_and_pixel_centres_match_reference() {
    let rows = ["111".into(), "101".into(), "111".into()];
    let cs = outline::bitmap_contours(&rows, "square", 1.0, 1, 0, 0).unwrap();
    assert_eq!(
        cs,
        vec![
            vec![
                (0., 0., true),
                (0., 3., true),
                (3., 3., true),
                (3., 0., true)
            ],
            vec![
                (1., 1., true),
                (2., 1., true),
                (2., 2., true),
                (1., 2., true)
            ]
        ]
    );
    assert_eq!(
        outline::rasterize(&cs, 1., 0, 0, 3, 3).unwrap().bindata(),
        rows
    );
    for shape in ["square", "circle"] {
        let cs = outline::bitmap_contours(&rows, shape, 1.0, 64, -2, -1).unwrap();
        assert_eq!(
            outline::rasterize(&cs, 64., -2, -1, 3, 3)
                .unwrap()
                .bindata(),
            rows
        );
    }
}
#[test]
fn unknown_charset_warns_once_and_surrogates_have_numeric_access() {
    let mut f = Font::create(&CreateOptions::default()).unwrap();
    f.encoding("unknown-legacy");
    assert_eq!(f.cp2unicode(0xd800).unwrap(), Some(0xd800));
    assert_eq!(f.cp2char(0xd800).unwrap(), None);
    f.char2cp("a").unwrap();
    assert_eq!(
        f.report()
            .entries
            .iter()
            .filter(|e| e.code == "bdfparser.warning")
            .count(),
        1
    );
}
#[test]
fn bounded_corrupt_true_type_and_option_errors() {
    for len in 0..100 {
        let data = vec![0; len];
        assert!(ttf::read_ttf(&data, Some(8)).is_err());
    }
    assert!(
        mini()
            .tottf(&TtfOptions {
                ppu: 3,
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        mini()
            .tottf(&TtfOptions {
                timestamp: i64::MAX,
                ..Default::default()
            })
            .is_err()
    );
    assert!(Bitmap::blank(i64::MAX, 2).is_err());
}
#[test]
fn encoding_override_is_not_a_pcf_property_edit() {
    let mut f = mini();
    let before = f.topcf(&Default::default()).unwrap();
    f.encoding("cp936");
    assert_eq!(before, f.topcf(&Default::default()).unwrap());
}
#[test]
fn io_byte_palettes_and_custom_encoding() {
    use fontpixel::compat::bdfparser::{Converter, SaveOptions};
    let mut f = Font::default();
    f.load_lines(MINI.lines().map(str::to_owned)).unwrap();
    assert_eq!(f.tobdf().unwrap(), MINI);
    let mut out = Vec::new();
    f.save_to(&mut out, &SaveOptions::Bdf).unwrap();
    assert_eq!(out, MINI.as_bytes());
    let b = Bitmap::new(&["012".into()]).unwrap();
    assert_eq!(b.tobytes("L", None).unwrap(), vec![255, 0, 127]);
    assert_eq!(b.tobytes("1", None).unwrap(), vec![0x9f]);
    f.set_converters(
        Some(Converter::new(|u| (u == 'a' as u32).then_some(65))),
        Some(Converter::new(|cp| (cp == 65).then_some('a' as u32))),
    );
    assert_eq!(f.glyph("a").unwrap().unwrap().cp(), 65);
    assert_eq!(f.cp2char(65).unwrap(), Some('a'));
    let c = f.clone();
    assert_eq!(c.char2cp("a").unwrap(), Some(65));
    f.charsetconvert("iso10646").unwrap();
    assert!(f.glyphbycp(97).is_some());
    assert_eq!(f.char2cp("a").unwrap(), Some(97));
}

#[test]
fn ttf_metadata_cumulative_zero_width_rows_are_bounded() {
    use fontpixel::sfnt::{Limits, Sfnt, SfntBuilder};
    let source = include_bytes!("../../../tests/data/compat/mini-square-1.0-1.ttf");
    let limits = Limits::default();
    let font = Sfnt::parse(source, &limits).unwrap();
    let mut metadata: serde_json::Value =
        serde_json::from_slice(font.table(*b"BDFP").unwrap()).unwrap();
    let mut entry = metadata["glyphs"][0].clone();
    assert_eq!(entry.as_array().unwrap().len(), 18);
    entry[3] = serde_json::json!(0);
    entry[4] = serde_json::json!(1_000_000);
    let entries: Vec<_> = (0..8)
        .map(|i| {
            let mut e = entry.clone();
            e[1] = serde_json::json!(1000 + i);
            e
        })
        .collect();
    metadata["glyphs"] = serde_json::json!(entries);
    let mut builder = SfntBuilder::new(font.flavor());
    for table in font.tables() {
        builder
            .insert(
                table.tag,
                if table.tag == *b"BDFP" {
                    serde_json::to_vec(&metadata).unwrap()
                } else {
                    table.data.to_vec()
                },
            )
            .unwrap();
    }
    let data = builder.build(&limits).unwrap();
    let error = fontpixel::compat::bdfparser::ttf::read_ttf(&data, None).unwrap_err();
    assert!(
        error.to_string().contains("cumulative model budget"),
        "{error}"
    );
}

fn warnings(f: &Font) -> Vec<String> {
    f.report()
        .entries
        .iter()
        .filter(|e| e.code == "bdfparser.warning")
        .map(|e| e.message.clone())
        .collect()
}
fn obj(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn slots_attach_like_lookup_before_and_after_edits() {
    let mut f = mini();
    let (id, generation, revision, snapshot) = f.slot(65).unwrap();
    assert_eq!(&snapshot, f.lookup(65).unwrap().meta());
    assert!(f.slot(999).is_none());
    // an unchanged model gives lookup's handle
    let a = f.attach(id, generation, revision, None).unwrap();
    assert!(a.is_attached() && a.meta() == &snapshot);
    // after an edit (through another handle) the snapshot is needed
    f.glyphbycp(65)
        .unwrap()
        .setmeta(&obj(json!({"dwx0": 9})))
        .unwrap();
    assert!(f.attach(id, generation, revision, None).is_none());
    let mut b = f
        .attach(id, generation, revision, Some(snapshot.clone()))
        .unwrap();
    assert!(b.is_attached() && b.meta() == &snapshot);
    // it merges like a handle taken before the edit: its own change wins,
    // the other handle's change stays
    b.setmeta(&obj(json!({"swx0": 500}))).unwrap();
    let now = f.lookup(65).unwrap().meta().clone();
    assert_eq!((now[6].clone(), now[8].clone()), (json!(500), json!(9)));
    // the report length is the count warnings_since returns
    assert_eq!(f.report_len(), f.warnings_since(0).1);
    f.set_header("fbbx", json!(8));
    assert!(f.slot(65).unwrap().2 != revision);
}

#[test]
fn header_and_unencoded_editors_edit_in_place() {
    let mut f = mini();
    let mut g = f.glyphbycp(65).unwrap();
    f.set_header("fbbx", json!(9));
    assert!(f.tobdf().unwrap().contains("FONTBOUNDINGBOX 9 8 0 -1\n"));
    // a lone pair component is stored with its partner as None
    f.set_header("dwx0", json!(3));
    assert_eq!(f.headers()["dwx0"], json!(3));
    assert_eq!(f.headers()["dwy0"], Value::Null);
    assert!(f.remove_header("dwx0") && !f.remove_header("nope"));
    let mut h = f.headers();
    h.shift_remove("comment");
    f.set_headers(&h);
    assert!(!f.tobdf().unwrap().contains("COMMENT \"mini"));
    // handles stay attached through header edits
    g.shift(1, 0).unwrap();
    assert!(g.is_attached());
    assert_eq!(f.lookup(65).unwrap().meta()[4], json!(1));

    let m = f
        .prepare_meta(&obj(
            json!({"glyphname": "u", "codepoint": -1, "bbw": 1, "bbh": 1, "hexdata": ["80"]}),
        ))
        .unwrap();
    assert_eq!(m[2..6], [json!(1), json!(1), json!(0), json!(0)]);
    f.put_with_alt(m, json!(42)).unwrap();
    assert_eq!(f.unencoded_altcode(0), Some(json!(42)));
    let mut u = f.unencoded_handle(0).unwrap();
    assert!(
        f.set_unencoded_meta(0, &obj(json!({"glyphname": "named"})))
            .unwrap()
    );
    assert!(
        !f.set_unencoded_meta(1, &obj(json!({"glyphname": "x"})))
            .unwrap()
    );
    assert!(f.set_unencoded_altcode(0, &json!(7)).unwrap());
    assert!(
        f.tobdf()
            .unwrap()
            .contains("STARTCHAR named\nENCODING -1 7\n")
    );
    u.shift(2, 0).unwrap(); // the handle still edits the slot
    assert_eq!(f.unencoded_handles()[0].meta()[4], json!(2));
    assert_eq!(f.unencoded_handles()[0].meta()[0], json!("named"));
    assert!(f.set_unencoded_altcode(0, &Value::Null).unwrap());
    assert!(f.tobdf().unwrap().contains("ENCODING -1\n"));
    assert!(!f.remove_unencoded(Some(3)));
    assert!(f.remove_unencoded(None) && f.unencoded_handles().is_empty());
    assert!(!u.is_attached());
}

#[test]
fn second_handle_follows_a_moved_glyph() {
    let f = mini();
    let mut g1 = f.glyphbycp(65).unwrap();
    let mut g2 = f.glyphbycp(65).unwrap();
    g1.setmeta(&obj(json!({"codepoint": 90}))).unwrap();
    g2.shift(2, 0).unwrap();
    assert!(g2.is_attached());
    assert_eq!(g2.meta()[1], json!(90));
    assert_eq!(f.lookup(90).unwrap().meta()[4], json!(2));
    assert!(f.lookup(65).is_none());
    g1.setmeta(&obj(json!({"glyphname": "Z"}))).unwrap();
    let m = f.lookup(90).unwrap().meta().clone();
    assert_eq!((m[0].clone(), m[4].clone()), (json!("Z"), json!(2)));
    // moving onto an existing code replaces that glyph; its handles detach
    let b = f.glyphbycp(66).unwrap();
    g2.setmeta(&obj(json!({"codepoint": 66}))).unwrap();
    assert!(!b.is_attached() && g1.is_attached());
    assert_eq!(f.lookup(66).unwrap().meta()[0], json!("Z"));
}

#[test]
fn lookups_and_warnings_use_bdfparser_texts() {
    let f = mini();
    assert!(f.lookup(0x20ac).is_none() && warnings(&f).is_empty());
    assert!(f.glyphbycp(0x20ac).is_none());
    assert_eq!(
        warnings(&f),
        ["Glyph \"€\" (codepoint 8364) does not exist in the font. Will return `None`"]
    );
    let mut f = mini();
    f.setprop("CHARSET_REGISTRY", json!("ISO8859"))
        .setprop("CHARSET_ENCODING", json!("1"));
    assert!(f.glyph("人").unwrap().is_none());
    assert_eq!(
        warnings(&f).last().unwrap(),
        "Character \"人\" cannot be encoded in the charset 'iso88591'. Will return `None`"
    );
    f.charsetconvert("no-such-charset").unwrap();
    let key = fontpixel::charset::canonical_charset("no-such-charset");
    assert!(warnings(&f).contains(&format!(
        "Unknown charset '{key}', characters are mapped to codepoints as Unicode"
    )));
}

#[test]
fn writer_and_drawing_behaviour_matches_bdfparser() {
    use fontpixel::compat::bdfparser::{Glyph, fmtver, svg::fmtnum};
    assert_eq!(
        (fmtver(2.0), fmtver(2.1), fmtver(1e-7)),
        ("2.0".into(), "2.1".into(), "1e-07".into())
    );
    assert_eq!(
        (fmtnum(-0.0), fmtnum(3.0), fmtnum(0.5)),
        ("0".into(), "3".into(), "0.5".into())
    );
    let mut f = mini();
    f.setprop("FLOAT", json!(1.5)).setprop("BIG", json!(1e20));
    let bdf = f.tobdf().unwrap();
    assert!(bdf.contains("\nFLOAT \"1.5\"\n") && bdf.contains("\nBIG \"1e+20\"\n"));
    f.remove_header("fbbyoff");
    f.remove_header("xres");
    assert_eq!(
        f.tobdf().unwrap_err().to_string(),
        "Cannot write BDF: missing header(s) xres, fbbyoff (font not loaded?)"
    );

    // a lone global DWIDTH component drives mode-1 spacing
    let mut f = mini();
    for cp in [65, 66] {
        f.glyphbycp(cp)
            .unwrap()
            .setmeta(&obj(json!({"dwx0": null, "dwy0": null})))
            .unwrap();
    }
    f.set_header("dwx0", json!(3));
    let kw = serde_json::Map::new();
    assert_eq!(f.drawcps(&[65, 65], &kw).unwrap().width(), 6 + 3);
    f.remove_header("dwx0");
    f.set_header("dwy0", json!(4));
    assert_eq!(f.drawcps(&[65, 65], &kw).unwrap().width(), 6 + 4);
    let err = f
        .drawcps(&[65], &obj(json!({"linelimit": 3})))
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "`linelimit` (3) is too small the line can't even contain one glyph: \"A\" (codepoint 65, width: 6)"
    );

    // PCF: global x components and negative boxes
    let mut f = mini();
    f.glyphbycp(65)
        .unwrap()
        .setmeta(&obj(
            json!({"dwx0": null, "dwy0": null, "swx0": null, "swy0": null}),
        ))
        .unwrap();
    f.set_header("dwx0", json!(7));
    f.set_header("swx0", json!(333));
    let mut back = Font::default();
    back.load_bytes(&f.topcf(&Default::default()).unwrap())
        .unwrap();
    let m = back.lookup(65).unwrap().meta().clone();
    assert_eq!((m[6].clone(), m[8].clone()), (json!(333), json!(7)));
    f.glyphbycp(65)
        .unwrap()
        .setmeta(&obj(json!({"bbw": -1, "glyphname": " A name"})))
        .unwrap();
    assert_eq!(
        f.topcf(&Default::default()).unwrap_err().to_string(),
        "Glyph \"A\" has a negative sized bitmap, -1x7"
    );

    // glyph hex-row messages print Python values
    let g = Glyph::new(
        &obj(
            json!({"glyphname": "my \"A\"", "codepoint": null, "bbw": 4, "bbh": 1, "hexdata": ["\"G"]}),
        ),
        &Font::default(),
    )
    .unwrap();
    assert_eq!(g.cp(), -1);
    assert_eq!(
        g.draw(1, None).unwrap_err().to_string(),
        "Glyph \"my \"A\"\" (codepoint None): bitmap row 0 \"\\\"G\" contains a non-hex character"
    );

    // gzip transport errors use bdfparser's prefix
    let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    gz.extend([1, 2, 3]);
    let err = Font::default().load_bytes(&gz).unwrap_err().to_string();
    assert!(err.starts_with("Malformed gzip data: "), "{err}");
    // as in bdfparser, the two magic bytes alone already mean gzip
    let err = Font::default()
        .load_bytes(b"\x1f\x8bgarbage")
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("Malformed gzip data: "), "{err}");
}

#[test]
fn true_type_messages_match_bdfparser() {
    let data = mini().tottf(&TtfOptions::default()).unwrap();
    let num = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num {
        let rec = &data[12 + 16 * i..28 + 16 * i];
        let tag = String::from_utf8_lossy(&rec[..4]).into_owned();
        let off = u32::from_be_bytes(rec[8..12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(rec[12..16].try_into().unwrap()) as usize;
        assert_eq!(
            ttf::read_ttf(&data[..off + len - 1], None)
                .unwrap_err()
                .to_string(),
            format!("Malformed TrueType font: table '{tag}' exceeds the file")
        );
    }
    let mut renamed = data.clone();
    let i = renamed.windows(4).position(|w| w == b"glyf").unwrap();
    renamed[i..i + 4].copy_from_slice(b"glyx");
    assert_eq!(
        ttf::read_ttf(&renamed, None).unwrap_err().to_string(),
        "Malformed TrueType font: missing required table 'glyf'"
    );
    assert_eq!(
        ttf::read_ttf(b"wOFF\0\0\0\0\0\0\0\0", None)
            .unwrap_err()
            .to_string(),
        "Not a TrueType font (sfnt version b'wOFF')"
    );
    let plain = mini()
        .tottf(&TtfOptions {
            metadata: false,
            ..Default::default()
        })
        .unwrap();
    let err = ttf::read_ttf(&plain, Some(8 * 700))
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("Malformed TrueType font: glyph ")
            && err.contains(" bounding box too large (")
            && err.ends_with(" pixels)"),
        "{err}"
    );
    let f = mini();
    f.glyphbycp(65)
        .unwrap()
        .setmeta(&obj(json!({"dwx0": -1})))
        .unwrap();
    assert_eq!(
        f.tottf(&TtfOptions {
            ppu: 2,
            ..Default::default()
        })
        .unwrap_err()
        .to_string(),
        "value out of range for a TrueType field: (-2, 0) (argument out of range)"
    );
}

#[test]
fn round_dot_true_type_point_admission_follows_the_memory_budget() {
    // 300 glyphs of 64x64 ink: 300 * 4096 dots * 8 points = 9.8M points,
    // above the former fixed 4M point cap; admitted because each glyph's
    // points are charged to the memory ledger and released once encoded.
    let mut f = Font::create(&CreateOptions {
        pixelsize: 64,
        ..Default::default()
    })
    .unwrap();
    let rows = vec!["1".repeat(64); 64];
    let b = Bitmap::new(&rows).unwrap();
    for cp in 0..300 {
        f.newglyph(0x4e00 + cp, Some(&b), 1, None, None).unwrap();
    }
    let data = f
        .tottf(&TtfOptions {
            shape: "circle".into(),
            ..Default::default()
        })
        .unwrap();
    let back = ttf::read_ttf(&data, None).unwrap();
    assert_eq!(back.length(), 300);
    assert_eq!(back.lookup(0x4e00).unwrap().meta()[2], json!(64));
}

#[test]
fn line_limit_check_accumulates_like_bdfparser() {
    let f = mini();
    f.glyphbycp(65)
        .unwrap()
        .setmeta(&obj(json!({"dwx0": -3})))
        .unwrap();
    f.glyphbycp(66)
        .unwrap()
        .setmeta(&obj(json!({"dwx0": 12})))
        .unwrap();
    let kw = obj(json!({"linelimit": 10}));
    // A costs 6 + (-3 - 6) = -3, so B (6 + 12 - 6 = 12) still fits its line.
    assert!(f.drawcps(&[65, 66], &kw).is_ok());
    assert_eq!(
        f.drawcps(&[66], &kw).unwrap_err().to_string(),
        "`linelimit` (10) is too small the line can't even contain one glyph: \"B\" (codepoint 66, width: 6)"
    );
}

#[test]
fn header_writes_keep_metrics_of_other_sources() {
    let mut core = fontpixel::Font::default();
    core.metrics.bounds = Some(fontpixel::bitmap::Bounds::new(4, 2, 0, 0));
    let mut g = fontpixel::Glyph::new("#...\n.#..".parse::<fontpixel::Bitmap>().unwrap());
    g.codepoints = vec![fontpixel::model::Codepoint::Unicode(65)];
    core.add_glyph(g).unwrap();
    let mut f = Font::from_core(core).unwrap();
    let kw = serde_json::Map::new();
    let before = f.drawcps(&[65], &kw).unwrap();
    f.set_header("fontname", json!("x"));
    assert_eq!(f.drawcps(&[65], &kw).unwrap(), before);
    assert_eq!(f.core().metrics.bounds.unwrap().width, 4);
    // a header-sourced box follows the headers
    f.set_header("fbbx", json!(6));
    f.set_header("fbby", json!(2));
    f.set_header("fbbxoff", json!(0));
    f.set_header("fbbyoff", json!(0));
    assert_eq!(f.core().metrics.bounds.unwrap().width, 6);
    f.remove_header("fbbx");
    f.remove_header("fbby");
    f.remove_header("fbbxoff");
    f.remove_header("fbbyoff");
    assert!(f.core().metrics.bounds.is_none());
}

#[test]
fn prepared_drawing_model_follows_every_edit() {
    let mut f = mini();
    let kw = serde_json::Map::new();
    let first = f.drawcps(&[65, 66], &kw).unwrap();
    assert_eq!(f.drawcps(&[65, 66], &kw).unwrap(), first); // cached
    // a glyph edit through a handle
    f.glyphbycp(65)
        .unwrap()
        .setmeta(&obj(json!({"dwx0": 9})))
        .unwrap();
    assert_eq!(f.drawcps(&[65, 66], &kw).unwrap().width(), 9 + 6);
    // a header edit
    f.set_header("fbbx", json!(8));
    assert_eq!(f.drawcps(&[66], &kw).unwrap().width(), 8);
    // removal and charset changes
    f.removeglyph(66);
    let missing = f.drawcps(&[66], &obj(json!({"mode": 0}))).unwrap();
    assert!(missing.bindata().iter().all(|r| !r.contains('1')));
    f.encoding("iso8859-1");
    assert!(
        f.draw("A", &kw)
            .unwrap()
            .bindata()
            .iter()
            .any(|r| r.contains('1'))
    );
}

#[test]
fn drawing_admits_images_beyond_the_native_pixel_cap_by_memory() {
    // 4,200 cells of 64x64: 17.2 Mpx (above the native 16,777,216-pixel cap),
    // about 19 MB with the packed render, within max_total_bytes.
    let mut f = Font::create(&CreateOptions {
        pixelsize: 64,
        ..Default::default()
    })
    .unwrap();
    let b = Bitmap::new(&["1".into()]).unwrap();
    f.newglyph(0x41, Some(&b), 1, None, None).unwrap();
    let cps = vec![0x41; 4200];
    let out = f
        .drawcps(&cps, &obj(json!({"mode": 0, "linelimit": 64})))
        .unwrap();
    assert_eq!((out.width(), out.height()), (64, 64 * 4200));
    // still bounded: far beyond the memory allowance is refused before allocation
    let huge = vec![0x41; 200_000];
    assert!(
        f.drawcps(&huge, &obj(json!({"mode": 0, "linelimit": 64})))
            .is_err()
    );
}
