//! C literals are checked independently of a C compiler; wire output is also compiled in crosschecks.
#![cfg(feature = "c")]
use fontpixel::formats::c::{ArrayOptions, CCodec, CReadOptions, CWriteOptions, Family};
use fontpixel::formats::{FormatReader, FormatWriter, ReadOptions, WriteOptions};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::resources::ReadContext;
use fontpixel::{FontSet, Input, ResourceLimits, ResourcePath, ResourceResolver};
fn read(s: &str, options: CReadOptions) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    let input = Input::new(s.as_bytes()).with_name("font.h");
    let o = ReadOptions::default();
    CCodec {
        read_options: options,
        ..Default::default()
    }
    .read(&mut ReadContext::new(&input, &o.limits)?, &o)
}
fn geometry() -> ArrayOptions {
    ArrayOptions {
        width: 9,
        height: 2,
        first_code: 65,
        charset: "unicode".into(),
        baseline: 1,
        lsb_first: false,
    }
}
fn generic() -> CReadOptions {
    CReadOptions {
        family: Family::Generic,
        generic: Some(geometry()),
        ..Default::default()
    }
}
const ARRAY: &str = "static const uint8_t Cells[2][4] = {{0x80,0x80,0x7f,0}, {0xff,0x80,0,0}};";
#[test]
fn generic_wide_rows_literals_and_legacy_tags() {
    let (s, r) = read(ARRAY, generic()).unwrap();
    assert!(!r.has_loss());
    let f = &s.fonts[0];
    let g = f.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[0x80, 0x80, 0x7f, 0]);
    assert_eq!(g.metrics.bounds.y, -1);
    assert_eq!(g.metrics.advance, Some(Vector::new(9, 0)));
    let mut o = geometry();
    o.charset = "cp437".into();
    o.lsb_first = true;
    let (s, _) = read(
        "unsigned char arr[] = {01U, 01, 0376, 0};",
        CReadOptions {
            generic: Some(o),
            ..Default::default()
        },
    )
    .unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[0x80, 0x80, 0x7f, 0]);
}
#[test]
fn generic_writer_option_comment_is_deterministic_and_reversible() {
    let (s, _) = read(ARRAY, generic()).unwrap();
    let o = WriteOptions::new(ResourcePath::new("font.h").unwrap());
    let c = CCodec::default();
    let a = c.write(&s, &o).unwrap();
    let b = c.write(&s, &o).unwrap();
    assert_eq!(a.output, b.output);
    assert!(!a.report.has_loss(), "{}", a.report);
    let bytes = a.output.into_single().unwrap();
    let (back, _) = read(
        std::str::from_utf8(&bytes).unwrap(),
        CReadOptions::default(),
    )
    .unwrap();
    assert_eq!(s, back);
}
#[test]
fn dimensions_ambiguity_expressions_and_truncation_fail() {
    for s in [
        "uint8_t a[1][4]={{1,0,2,0},{1,0,2,0}};",
        "uint8_t a[1][4]={{1<<2,0,0,0}};",
        "uint8_t a[1][4]={{MACRO,0,0,0}};",
        "uint8_t a[1][4]={{256,0,0,0}};",
        "uint8_t a[1][4]={{-1,0,0,0}};",
        "uint8_t a[1][4]={{1,0,0,0}}",
        "uint8_t a[1][4]={{1,0,0}};",
        "uint8_t a[1][4]={{1,0,0,0}}; /*",
        "uint8_t a[1][4]={{1,0,0,0}}; \"",
        "uint8_t a[1][4]={{0x1.0,0,0,0}};",
    ] {
        assert!(read(s, generic()).is_err(), "{s}");
    }
    let s = format!("{ARRAY}\nuint8_t Other[1][4]={{ {{0,0,0,0}} }};");
    assert!(read(&s, generic()).is_err());
    let mut o = generic();
    o.array_name = Some("Cells".into());
    assert!(read(&s, o).is_ok());
    assert!(read(ARRAY, CReadOptions::default()).is_err());
}
const GFX: &str = r#"
const uint8_t TestBitmaps[] PROGMEM = {0xff, 0xab, 0x80};
const GFXglyph TestGlyphs[2] PROGMEM = {
 {1,3,3,4,-1,-2},
 {3,0,0,3,0,0}
};
const GFXfont Test PROGMEM = {(uint8_t *)TestBitmaps,(GFXglyph *)TestGlyphs,0x41,0x42,5};
"#;
#[test]
fn gfx_continuous_bits_bearings_empty_advance_and_line_height() {
    let (s, r) = read(GFX, CReadOptions::default()).unwrap();
    assert!(!r.has_loss());
    let f = &s.fonts[0];
    let g = f.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[0xa0, 0x40, 0xe0]);
    assert_eq!(g.metrics.bounds.x, -1);
    assert_eq!(g.metrics.bounds.y, -1);
    assert_eq!(f.metrics.line_height, Some(5));
    assert_eq!(f.metrics.ascent, Some(2));
    assert_eq!(f.metrics.descent, Some(1));
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Unicode(66))
            .unwrap()
            .metrics
            .advance,
        Some(Vector::new(3, 0))
    );
    let c = CCodec {
        write_options: CWriteOptions {
            family: Family::Gfx,
            array_name: "Roundtrip".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let w = c
        .write(&s, &WriteOptions::new(ResourcePath::new("font.h").unwrap()))
        .unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let bytes = w.output.into_single().unwrap();
    assert_eq!(
        s,
        read(
            std::str::from_utf8(&bytes).unwrap(),
            CReadOptions::default()
        )
        .unwrap()
        .0
    );
}
#[test]
fn gfx_field_widths_offsets_and_pointers_are_checked() {
    for bad in [
        GFX.replace("{1,3,3,4,-1,-2}", "{65535,3,3,4,-1,-2}"),
        GFX.replace("{1,3,3,4,-1,-2}", "{1,256,3,4,-1,-2}"),
        GFX.replace("{1,3,3,4,-1,-2}", "{1,3,3,4,-129,-2}"),
        GFX.replace("0x41,0x42,5", "0x42,0x41,5"),
        GFX.replace("0x41,0x42,5", "0x41,0x43,5"),
        GFX.replace("(uint8_t *)TestBitmaps", "(uint8_t *)(TestBitmaps+1)"),
    ] {
        assert!(read(&bad, CReadOptions::default()).is_err(), "{bad}");
    }
}
#[test]
fn font8x8_declared_range_beats_incorrect_comments() {
    let s = "// unicode points U+03AA - U+03AB\nchar font8x8_greek[2][8] = {\n{1,3,7,15,31,63,127,255}, // U+0399\n{2,4,8,16,32,64,128,0}, // U+03A5\n};";
    let (s, r) = read(s, CReadOptions::default()).unwrap();
    assert!(!r.has_loss());
    let f = &s.fonts[0];
    assert!(f.glyph_for_codepoint(Codepoint::Unicode(0x399)).is_none());
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Unicode(0x3aa))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[128, 192, 224, 240, 248, 252, 254, 255]
    );
    assert_eq!(f.metrics.ascent, Some(7));
    assert_eq!(f.metrics.descent, Some(1));
}
#[test]
fn tmat_comments_can_contain_braces_without_changing_structure() {
    let mut s = String::from("static const unsigned char chars[][8] = {\n");
    for cp in 32..128 {
        s.push_str(&format!("{{ // {cp} (0x{cp:02x}) : '{}'\n0b00000010,0b00000100,0b00001000,0b00010000,0b00100000,0b01000000,0b10000000,0b00000000\n}},\n",char::from_u32(cp).unwrap()));
    }
    s.push_str("};");
    let (font, _) = read(&s, CReadOptions::default()).unwrap();
    assert_eq!(font.fonts[0].len(), 96);
    assert_eq!(
        font.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(47))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[2, 4, 8, 16, 32, 64, 128, 0]
    );
    assert!(
        read(
            &s.replace("47 (0x2f)", "47 (0x30)"),
            CReadOptions::default()
        )
        .is_err()
    );
}
#[test]
fn font8x8_include_cycles_deduplicate_and_missing_mappings_report_loss() {
    struct Resolver;
    impl ResourceResolver for Resolver {
        fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
            let text = match p.as_str() {
                "font8x8.h" => "#include \"part.h\"",
                "part.h" => {
                    "#include \"font8x8.h\"\nchar font8x8_part[2][8] = {\n{1,2,3,4,0,0,0,0}, // U+20A7\n{0,0,0,0,0,0,0,0}, // no mapping\n};"
                }
                _ => return Err(fontpixel::Error::MissingResource(p.to_string())),
            };
            assert!(text.len() <= max);
            Ok(text.as_bytes().to_vec())
        }
    }
    let source = b"#include \"part.h\"";
    let input = Input::new(source)
        .with_name("font8x8.h")
        .with_resources(&Resolver);
    let options = ReadOptions::default();
    let c = CCodec {
        read_options: CReadOptions {
            family: Family::Font8x8,
            ..Default::default()
        },
        ..Default::default()
    };
    let (s, r) = c
        .read(
            &mut ReadContext::new(&input, &options.limits).unwrap(),
            &options,
        )
        .unwrap();
    assert_eq!(s.fonts[0].len(), 1);
    assert!(r.has_loss());
}
#[test]
fn reader_budgets_reject_excess_before_allocating_model() {
    for l in [
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 17,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 128,
            ..Default::default()
        },
    ] {
        let input = Input::new(ARRAY.as_bytes());
        let o = ReadOptions {
            limits: l,
            ..Default::default()
        };
        let result = ReadContext::new(&input, &o.limits).and_then(|mut c| {
            CCodec {
                read_options: generic(),
                ..Default::default()
            }
            .read(&mut c, &o)
        });
        assert!(result.is_err());
    }
}
#[test]
fn sparse_c_output_reports_added_slots_and_glyph_metadata_loss() {
    let (mut s, _) = read(ARRAY, generic()).unwrap();
    let id = s.fonts[0].glyphs().nth(1).unwrap().0;
    let mut g = s.fonts[0].glyph(id).unwrap().clone();
    s.fonts[0].remove_glyph(id);
    g.codepoints = vec![Codepoint::Unicode(68), Codepoint::Unicode(100)];
    g.name = Some("named".into());
    s.fonts[0].add_glyph(g).unwrap();
    let w = CCodec::default()
        .write(&s, &WriteOptions::new(ResourcePath::new("font.h").unwrap()))
        .unwrap();
    assert!(
        w.report
            .entries
            .iter()
            .any(|e| e.code == "c.write.added_empty")
    );
    assert!(
        w.report
            .entries
            .iter()
            .any(|e| e.code == "c.write.mappings")
    );
    assert!(
        w.report
            .entries
            .iter()
            .any(|e| e.code == "c.write.glyph_metadata")
    );
}

#[test]
fn c_integer_suffixes_follow_literal_grammar() {
    for suffix in ["", "u", "U", "l", "L", "ll", "LL", "uLL", "llU"] {
        let source = format!("const uint8_t bits[1][1] = {{{{128{suffix}}}}};");
        let options = CReadOptions {
            generic: Some(ArrayOptions {
                width: 8,
                height: 1,
                first_code: 65,
                charset: "unicode".into(),
                baseline: 1,
                lsb_first: false,
            }),
            ..Default::default()
        };
        assert!(read(&source, options).is_ok(), "{suffix}");
    }
    for suffix in ["UU", "ULUL", "LLLL", "lL", "Ll", "lul"] {
        let source = format!("const uint8_t bits[1][1] = {{{{128{suffix}}}}};");
        let options = CReadOptions {
            generic: Some(ArrayOptions {
                width: 8,
                height: 1,
                first_code: 65,
                charset: "unicode".into(),
                baseline: 1,
                lsb_first: false,
            }),
            ..Default::default()
        };
        assert!(read(&source, options).is_err(), "{suffix}");
    }
}

#[test]
fn reordering_equal_slot_counts_is_a_strict_loss() {
    let (mut set, _) = read(ARRAY, generic()).unwrap();
    let glyphs: Vec<_> = set.fonts[0]
        .glyphs()
        .map(|(id, g)| (id, g.clone()))
        .collect();
    set.fonts[0]
        .replace_glyph(glyphs[0].0, glyphs[1].1.clone())
        .unwrap();
    set.fonts[0]
        .replace_glyph(glyphs[1].0, glyphs[0].1.clone())
        .unwrap();
    let mut options = WriteOptions::new(ResourcePath::new("font.h").unwrap());
    let written = fontpixel::registry().write(&set, "c", &options).unwrap();
    assert!(
        written
            .report
            .entries
            .iter()
            .any(|e| e.code == "c.write.glyph_slots")
    );
    options.strict = true;
    assert!(fontpixel::registry().write(&set, "c", &options).is_err());
}

#[test]
fn font8x8_array_selection_filters_and_requires_one_match() {
    let source = "char font8x8_a[1][8]={{1,0,0,0,0,0,0,0}, // U+0041\n};\nchar font8x8_b[1][8]={{2,0,0,0,0,0,0,0}, // U+0042\n};";
    let (all, _) = read(source, CReadOptions::default()).unwrap();
    assert_eq!(all.fonts[0].len(), 2);
    let options = CReadOptions {
        array_name: Some("font8x8_b".into()),
        ..Default::default()
    };
    let (selected, _) = read(source, options).unwrap();
    assert_eq!(selected.fonts[0].len(), 1);
    assert!(
        selected.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(66))
            .is_some()
    );
    let options = CReadOptions {
        array_name: Some("missing".into()),
        ..Default::default()
    };
    assert!(read(source, options).is_err());
}
