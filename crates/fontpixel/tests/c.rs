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
fn automatic_range_failure_retains_every_unmapped_glyph_and_a_fatal_diagnostic() {
    for legacy in [false, true] {
        let (mut source, _) = read(ARRAY, generic()).unwrap();
        if legacy {
            source.fonts[0].charset.name = Some("cp437".into());
        }
        let ids: Vec<_> = source.fonts[0].glyphs().map(|(id, _)| id).collect();
        for id in &ids {
            source.fonts[0]
                .edit_glyph(*id, |g| {
                    g.codepoints = if legacy {
                        vec![Codepoint::Legacy(65 + id.index() as u32)]
                    } else {
                        Vec::new()
                    };
                    Ok(())
                })
                .unwrap();
        }
        // Explicit generic geometry supplies first_code, but still needs an
        // encodable glyph to infer the last code. Exercise both failure paths.
        let options = if legacy {
            vec![CWriteOptions {
                family: Family::Gfx,
                ..Default::default()
            }]
        } else {
            vec![
                CWriteOptions::default(),
                CWriteOptions {
                    generic: Some(geometry()),
                    ..Default::default()
                },
            ]
        };
        for write_options in options {
            let error = CCodec {
                write_options,
                ..Default::default()
            }
            .write(
                &source,
                &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
            )
            .unwrap_err();
            let fontpixel::Error::ReportedErrors(report) = error else {
                panic!("omission report was discarded: {error}");
            };
            assert_eq!(report.entries.len(), ids.len() + 1);
            for (entry, id) in report.entries.iter().zip(&ids) {
                assert_eq!(entry.code, "c.write.unmapped");
                assert_eq!(entry.level, fontpixel::report::Level::Warning);
                assert_eq!(entry.impact, fontpixel::report::Impact::Loss);
                assert_eq!(entry.location.font_index, Some(0));
                assert_eq!(entry.location.glyph_index, Some(id.index()));
            }
            let fatal = report.entries.last().unwrap();
            assert_eq!(fatal.code, "c.write.no_representable_glyphs");
            assert_eq!(fatal.level, fontpixel::report::Level::Error);
            assert_eq!(fatal.impact, fontpixel::report::Impact::None);
            assert_eq!(fatal.location.font_index, Some(0));
            assert_eq!(fatal.location.glyph_index, None);
        }
    }
}

#[test]
fn explicit_range_still_generates_empty_glyphs_when_source_is_unmapped() {
    let (mut source, _) = read(ARRAY, generic()).unwrap();
    let ids: Vec<_> = source.fonts[0].glyphs().map(|(id, _)| id).collect();
    for id in ids {
        source.fonts[0]
            .edit_glyph(id, |g| {
                g.codepoints.clear();
                Ok(())
            })
            .unwrap();
    }
    for family in [Family::Generic, Family::Gfx] {
        let written = CCodec {
            write_options: CWriteOptions {
                family,
                range: Some((32, 33)),
                ..Default::default()
            },
            ..Default::default()
        }
        .write(
            &source,
            &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
        )
        .unwrap();
        assert!(!written.report.has_errors());
        for code in ["c.write.unmapped", "c.write.added_empty"] {
            assert_eq!(
                written
                    .report
                    .entries
                    .iter()
                    .filter(|e| e.code == code)
                    .count(),
                2
            );
        }
        let bytes = written.output.into_single().unwrap();
        let (back, _) = read(
            std::str::from_utf8(&bytes).unwrap(),
            CReadOptions::default(),
        )
        .unwrap();
        assert_eq!(back.fonts[0].len(), 2);
        for cp in 32..=33 {
            let glyph = back.fonts[0]
                .glyph_for_codepoint(Codepoint::Unicode(cp))
                .unwrap();
            assert!(glyph.bitmap.as_bytes().iter().all(|&b| b == 0));
        }
    }
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

#[test]
fn native_generic_comment_wins_over_font8x8_symbol_name() {
    let (source, _) = read(ARRAY, generic()).unwrap();
    let writer = CCodec {
        write_options: CWriteOptions {
            array_name: "font8x8_custom".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = writer
        .write(
            &source,
            &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
        )
        .unwrap();
    assert!(!output.report.has_loss());
    let bytes = output.output.into_single().unwrap();
    assert_eq!(
        read(
            std::str::from_utf8(&bytes).unwrap(),
            CReadOptions::default()
        )
        .unwrap()
        .0,
        source
    );
    let fake =
        "const char *label = \"/* fontpixel-c: invalid */ font8x8_\"; uint8_t bytes[]={128};";
    assert!(read(fake, CReadOptions::default()).is_err());
}

#[test]
fn font8x8_ranges_belong_to_each_declaration() {
    let source = "// unicode points U+0041 - U+0041\nchar font8x8_first[1][8]={{1,0,0,0,0,0,0,0}};\n// unicode points U+0042 - U+0042\nchar font8x8_second[1][8]={{2,0,0,0,0,0,0,0}};";
    let (selected, _) = read(
        source,
        CReadOptions {
            array_name: Some("font8x8_second".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(selected.fonts[0].len(), 1);
    assert!(
        selected.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(66))
            .is_some()
    );
    let (all, _) = read(source, CReadOptions::default()).unwrap();
    assert_eq!(all.fonts[0].len(), 2);
}

#[test]
fn native_u8g2_and_lvgl_roundtrip_unicode_and_pixels() {
    for family in [Family::U8g2, Family::Lvgl] {
        let (mut source, _) = read(ARRAY, generic()).unwrap();
        let ids: Vec<_> = source.fonts[0].glyphs().map(|(id, _)| id).collect();
        source.fonts[0]
            .edit_glyph(ids[1], |g| {
                g.codepoints = vec![Codepoint::Unicode(0x4e00)];
                Ok(())
            })
            .unwrap();
        for compression in [
            fontpixel::formats::c::LvglCompression::Plain,
            fontpixel::formats::c::LvglCompression::Rle,
            fontpixel::formats::c::LvglCompression::RleXor,
        ] {
            let codec = CCodec {
                write_options: CWriteOptions {
                    family,
                    lvgl_compression: compression,
                    ..Default::default()
                },
                ..Default::default()
            };
            let written = codec
                .write(
                    &source,
                    &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
                )
                .unwrap();
            let bytes = written.output.into_single().unwrap();
            let (back, _) = read(
                std::str::from_utf8(&bytes).unwrap(),
                CReadOptions::default(),
            )
            .unwrap();
            for cp in [65, 0x4e00] {
                let a = source.fonts[0]
                    .glyph_for_codepoint(Codepoint::Unicode(cp))
                    .unwrap();
                let z = back.fonts[0]
                    .glyph_for_codepoint(Codepoint::Unicode(cp))
                    .unwrap();
                assert_eq!(a.bitmap, z.bitmap, "{family:?} {compression:?}");
                assert_eq!(a.metrics, z.metrics, "{family:?} {compression:?}");
            }
        }
    }
}

#[test]
fn survey_legacy_profiles_and_lvgl_binary_roundtrip() {
    use fontpixel::formats::c::{CContainer, U8glibFormat};
    let source = "uint8_t cells[2][8]={{128,64,32,16,8,4,2,1},{255,129,129,129,129,129,129,255}};";
    let geometry = ArrayOptions {
        width: 8,
        height: 8,
        first_code: 65,
        charset: "unicode".into(),
        baseline: 8,
        lsb_first: false,
    };
    let (set, _) = read(
        source,
        CReadOptions {
            generic: Some(geometry),
            ..Default::default()
        },
    )
    .unwrap();
    for family in [Family::Netbsd, Family::U8glib, Family::U8x8, Family::Lvgl] {
        for variant in 0..3 {
            let write_options = CWriteOptions {
                family,
                u8glib_format: if variant == 1 {
                    U8glibFormat::Compact
                } else if variant == 2 {
                    U8glibFormat::Gray2
                } else {
                    U8glibFormat::Full
                },
                u8x8_legacy: variant == 1,
                container: if family == Family::Lvgl {
                    CContainer::LvglBinary
                } else {
                    CContainer::Source
                },
                ..Default::default()
            };
            let codec = CCodec {
                write_options,
                ..Default::default()
            };
            let w = codec
                .write(
                    &set,
                    &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
                )
                .unwrap();
            let bytes = w.output.into_single().unwrap();
            let input = Input::new(&bytes);
            let ro = ReadOptions::default();
            let (back, _) = CCodec {
                read_options: CReadOptions {
                    family,
                    u8x8_legacy: variant == 1,
                    ..Default::default()
                },
                ..Default::default()
            }
            .read(&mut ReadContext::new(&input, &ro.limits).unwrap(), &ro)
            .unwrap();
            for cp in [65, 66] {
                let a = set.fonts[0]
                    .glyph_for_codepoint(Codepoint::Unicode(cp))
                    .unwrap();
                let z = back.fonts[0]
                    .glyph_for_codepoint(Codepoint::Unicode(cp))
                    .unwrap();
                assert_eq!(a.bitmap.width(), z.bitmap.width());
                assert_eq!(a.bitmap.height(), z.bitmap.height());
                assert_eq!(a.metrics, z.metrics, "{family:?} variant {variant}");
                if family == Family::U8glib && variant == 2 {
                    if let fontpixel::model::GlyphBitmap::Gray8(v) = &z.bitmap {
                        for y in 0..8 {
                            for x in 0..8 {
                                if let fontpixel::model::GlyphBitmap::Mono(a) = &a.bitmap {
                                    assert_eq!(
                                        v.get_pixel(x, y).unwrap(),
                                        if a.get_pixel(x, y).unwrap() != 0 {
                                            255
                                        } else {
                                            0
                                        }
                                    );
                                }
                            }
                        }
                    } else {
                        panic!("expected Gray8");
                    }
                } else {
                    assert_eq!(a.bitmap, z.bitmap, "{family:?} variant {variant}");
                }
            }
        }
    }
}

#[test]
fn netbsd_native_metadata_and_bit_byte_order() {
    let source = "static u_char data[]={0,1,0,2}; struct wsdisplay_font Test={.name=\"example\",.firstchar=' ',.numchars=34-' ',.encoding=WSDISPLAY_FONTENC_ISO,.fontwidth=9,.fontheight=1,.stride=2,.bitorder=WSDISPLAY_FONTORDER_R2L,.byteorder=WSDISPLAY_FONTORDER_R2L,.data=data};";
    let (set, _) = read(source, CReadOptions::default()).unwrap();
    assert_eq!(set.fonts[0].names.full.as_deref(), Some("example"));
    assert_eq!(
        set.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(32))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[128, 0]
    );
    for bad in [
        source.replace("34-' '", "function()"),
        source.replace(".stride=2", ".stride=1"),
        source.replace(
            ".bitorder=WSDISPLAY_FONTORDER_R2L",
            ".bitorder=WSDISPLAY_FONTORDER_KNOWN",
        ),
    ] {
        assert!(read(&bad, CReadOptions::default()).is_err());
    }
}

fn lvgl_fixture(bitmap: &str, bpp: u8, format: u8, cmap: &str, extra: &str) -> String {
    format!(
        "#include <lvgl.h>\nstatic const uint8_t pixels[]={{ {bitmap} }};\nstatic const lv_font_fmt_txt_glyph_dsc_t glyphs[]={{ {{.bitmap_index=0,.adv_w=0,.box_w=0,.box_h=0,.ofs_x=0,.ofs_y=0}},{{.bitmap_index=0,.adv_w=56,.box_w=3,.box_h=1,.ofs_x=-1,.ofs_y=-2}} }};\n{extra}\nstatic const lv_font_fmt_txt_cmap_t cmaps[]={{{cmap}}};\nstatic const lv_font_fmt_txt_dsc_t desc={{.glyph_bitmap=pixels,.glyph_dsc=glyphs,.cmaps=cmaps,.cmap_num=1,.bpp={bpp},.bitmap_format={format},.kern_dsc=NULL}};\nconst lv_font_t font={{.get_glyph_dsc=lv_font_get_glyph_dsc_fmt_txt,.get_glyph_bitmap=lv_font_get_bitmap_fmt_txt,.line_height=8,.base_line=2,.dsc=&desc}};"
    )
}
#[test]
fn lvgl_all_cmap_types_and_fractional_advance_loss() {
    let cases = [
        (
            "LV_FONT_FMT_TXT_CMAP_FORMAT0_FULL",
            "NULL",
            "offs",
            "static const uint8_t offs[]={0};",
        ),
        ("LV_FONT_FMT_TXT_CMAP_FORMAT0_TINY", "NULL", "NULL", ""),
        (
            "LV_FONT_FMT_TXT_CMAP_SPARSE_FULL",
            "unicodes",
            "offs",
            "static const uint16_t unicodes[]={0};static const uint16_t offs[]={0};",
        ),
        (
            "LV_FONT_FMT_TXT_CMAP_SPARSE_TINY",
            "unicodes",
            "NULL",
            "static const uint16_t unicodes[]={0};",
        ),
    ];
    for (ty, ul, offs, extra) in cases {
        let cmap = format!(
            "{{.range_start=65,.range_length=1,.glyph_id_start=1,.unicode_list={ul},.glyph_id_ofs_list={offs},.list_length=1,.type={ty}}}"
        );
        let source = lvgl_fixture("0xa0", 1, 0, &cmap, extra);
        let (s, r) = read(&source, CReadOptions::default()).unwrap();
        let g = s.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap();
        assert_eq!(g.bitmap.as_bytes(), &[160]);
        assert_eq!(g.metrics.advance.unwrap().x, 4);
        assert_eq!(g.metrics.bounds.x, -1);
        assert!(
            r.entries
                .iter()
                .any(|e| e.code == "c.read.lvgl.fractional_advance")
        );
    }
}
#[test]
fn lvgl_depths_rle_counter_xor_and_kerning_projection() {
    let cmap = "{.range_start=65,.range_length=1,.glyph_id_start=1,.unicode_list=NULL,.glyph_id_ofs_list=NULL,.list_length=0,.type=2}";
    for (bpp, bytes, expected) in [
        (2, "0x1c", vec![0, 85, 255]),
        (3, "0x15,0x80", vec![0, 182, 109]),
        (4, "0x08,0xf0", vec![0, 136, 255]),
        (8, "0,128,255", vec![0, 128, 255]),
    ] {
        let (s, _) = read(
            &lvgl_fixture(bytes, bpp, 0, cmap, ""),
            CReadOptions::default(),
        )
        .unwrap();
        assert_eq!(
            s.fonts[0]
                .glyph_for_codepoint(Codepoint::Unicode(65))
                .unwrap()
                .bitmap
                .as_bytes(),
            expected
        );
    }
    // 2-bpp values [3,3,3]: two literal pixels, followed by repeat bit 1.
    let (s, _) = read(
        &lvgl_fixture("0xf8", 2, 2, cmap, ""),
        CReadOptions::default(),
    )
    .unwrap();
    assert_eq!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[255, 255, 255]
    );
    let text = lvgl_fixture("0xff,0xff,0xe0", 1, 2, cmap, "").replace(".box_w=3", ".box_w=76");
    let (s, _) = read(&text, CReadOptions::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Unicode(65))
        .unwrap();
    assert_eq!(
        g.bitmap.as_bytes(),
        &[255, 255, 255, 255, 255, 255, 255, 255, 255, 224]
    );
    let text = lvgl_fixture("0xa4", 1, 1, cmap, "").replace(".box_h=1", ".box_h=2");
    let (s, _) = read(&text, CReadOptions::default()).unwrap();
    assert_eq!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[160, 160]
    );
    let extra = "static const uint8_t ids[]={1,1};static const int8_t values[]={-4};static const lv_font_fmt_txt_kern_pair_t kern={.glyph_ids=ids,.values=values,.pair_cnt=1,.glyph_ids_size=0};";
    let text = lvgl_fixture("0xa0", 1, 0, cmap, extra).replace(
        ".kern_dsc=NULL",
        ".kern_dsc=&kern,.kern_classes=0,.kern_scale=16",
    );
    let (_, r) = read(&text, CReadOptions::default()).unwrap();
    assert!(r.entries.iter().any(|v| v.code == "c.read.lvgl.kerning"));
    assert!(
        read(
            &text.replace("ids[]={1,1}", "ids[]={1,2}"),
            CReadOptions::default()
        )
        .is_err()
    );
}
#[test]
fn native_limit_failures_and_source_metadata_are_bounded() {
    let (set, _) = read(ARRAY, generic()).unwrap();
    for family in [Family::U8g2, Family::Lvgl, Family::Netbsd, Family::U8glib] {
        for max in [1024, 8192, 65536, 1048576] {
            let limits = ResourceLimits {
                max_total_bytes: max,
                ..Default::default()
            };
            let codec = CCodec {
                write_options: CWriteOptions {
                    family,
                    limits: limits.clone(),
                    ..Default::default()
                },
                ..Default::default()
            };
            if let Ok(w) = codec.write(
                &set,
                &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
            ) {
                let data = w.output.into_single().unwrap();
                let input = Input::new(&data);
                let ro = ReadOptions {
                    limits,
                    ..Default::default()
                };
                assert!(
                    CCodec {
                        read_options: CReadOptions {
                            family,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .read(&mut ReadContext::new(&input, &ro.limits).unwrap(), &ro)
                    .is_ok()
                );
            }
        }
        let mut huge = set.clone();
        huge.fonts[0].names.family = Some("x".repeat(65537));
        let codec = CCodec {
            write_options: CWriteOptions {
                family,
                limits: ResourceLimits {
                    max_total_bytes: 65536,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            codec
                .write(
                    &huge,
                    &WriteOptions::new(ResourcePath::new("font.h").unwrap())
                )
                .is_err()
        );
    }
}

#[test]
fn u8g2_repeat_pairs_compress_large_checkerboard_into_native_record() {
    let mut f = fontpixel::model::Font::default();
    let mut bytes = Vec::new();
    for y in 0..32 {
        bytes.extend_from_slice(&[if y % 2 == 0 { 0x55 } else { 0xaa }; 4]);
    }
    let mut g = fontpixel::model::Glyph::new(
        fontpixel::bitmap::Bitmap::<fontpixel::bitmap::Mono>::from_raw(
            32,
            32,
            &bytes,
            Default::default(),
        )
        .unwrap(),
    );
    g.codepoints.push(Codepoint::Unicode(65));
    g.metrics.advance = Some(fontpixel::model::Vector::new(32, 0));
    f.add_glyph(g).unwrap();
    f.update_bounds().unwrap();
    let set = fontpixel::model::FontSet::from(f);
    let w = CCodec {
        write_options: CWriteOptions {
            family: Family::U8g2,
            ..Default::default()
        },
        ..Default::default()
    }
    .write(
        &set,
        &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
    )
    .unwrap();
    let data = w.output.into_single().unwrap();
    let (back, _) = read(std::str::from_utf8(&data).unwrap(), CReadOptions::default()).unwrap();
    assert_eq!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        bytes
    );
}

#[test]
fn lvgl_generated_version_guards_select_literal_data_without_execution() {
    let cmap = "{.range_start=65,.range_length=1,.glyph_id_start=1,.type=LV_FONT_FMT_TXT_CMAP_FORMAT0_TINY}";
    let source=lvgl_fixture("0xa0",1,0,cmap,"").replace("static const lv_font_fmt_txt_dsc_t desc={", "#ifndef LV_FONT_TEST\n#define LV_FONT_TEST 1\n#endif\n#if LV_FONT_TEST && (LVGL_VERSION_MAJOR >= 8)\n#if LV_VERSION_CHECK(8,0,0)\nstatic const lv_font_fmt_txt_dsc_t desc={\n#else\nstatic lv_font_fmt_txt_dsc_t desc={\n#endif\n");
    let source = format!("{source}\n#endif\n");
    let (set, report) = read(&source, CReadOptions::default()).unwrap();
    assert_eq!(set.fonts[0].len(), 1);
    assert!(
        report
            .entries
            .iter()
            .any(|v| v.code == "c.read.lvgl.fractional_advance")
    );
    assert!(
        read(
            &source.replace("LVGL_VERSION_MAJOR >= 8", "system(123)"),
            CReadOptions::default()
        )
        .is_err()
    );
    assert!(
        read(
            &source.replace("#endif\n", "#else\n#else\n#endif\n"),
            CReadOptions::default()
        )
        .is_err()
    );
}

fn detect_source(bytes: &[u8], limits: &ResourceLimits) -> fontpixel::formats::Confidence {
    use fontpixel::formats::FormatDetector;
    let input = Input::new(bytes);
    CCodec::default()
        .detect(&mut ReadContext::new(&input, limits).unwrap())
        .unwrap()
}
#[test]
fn detector_ignores_comment_string_and_directive_native_tokens() {
    use fontpixel::formats::Confidence;
    for token in [
        "U8G_FONT_SECTION",
        "U8G2_FONT_SECTION",
        "u8x8_font_fake",
        "lv_font_fmt_txt_dsc_t",
        "wsdisplay_font",
        "GFXfont GFXglyph",
        "font8x8_basic",
    ] {
        for text in [
            format!("/* {token} */"),
            format!("// {token} = {{0}};"),
            format!("# Source declaration: {token} = {{0}};"),
            format!("const char *note = \"{token} = {{0}};\";"),
        ] {
            assert_eq!(
                detect_source(text.as_bytes(), &ResourceLimits::default()),
                Confidence::NoMatch,
                "{text}"
            );
        }
    }
    let fake = "const char *note = \"/* fontpixel-c: {} */\"; const uint8_t cells[1] = {0};";
    assert_eq!(
        detect_source(fake.as_bytes(), &ResourceLimits::default()),
        Confidence::NoMatch
    );
    assert_eq!(
        detect_source(b"/* fontpixel-c: {} */", &ResourceLimits::default()),
        Confidence::NoMatch
    );
}
#[test]
fn detector_preserves_actual_font8x8_include_without_resolving_it() {
    use fontpixel::formats::Confidence;
    for text in [
        "#include \"font8x8_basic.h\"",
        "# include \"sub/font8x8_basic.h\"",
    ] {
        // No resolver is installed. Successful detection must not read a file.
        assert_eq!(
            detect_source(text.as_bytes(), &ResourceLimits::default()),
            Confidence::Possible
        );
    }
    for text in [
        "// #include \"font8x8_basic.h\"",
        "/* #include \"font8x8_basic.h\" */",
        "#define note \"font8x8_basic.h\"",
    ] {
        assert_eq!(
            detect_source(text.as_bytes(), &ResourceLimits::default()),
            Confidence::NoMatch
        );
    }
}
#[test]
fn detector_recognizes_every_native_source_writer_and_bounds_tokens() {
    use fontpixel::formats::Confidence;
    let source = "uint8_t cells[2][8]={{128,64,32,16,8,4,2,1},{255,129,129,129,129,129,129,255}};";
    let (set, _) = read(
        source,
        CReadOptions {
            generic: Some(ArrayOptions {
                width: 8,
                height: 8,
                first_code: 65,
                charset: "unicode".into(),
                baseline: 8,
                lsb_first: false,
            }),
            ..Default::default()
        },
    )
    .unwrap();
    for family in [
        Family::Generic,
        Family::Gfx,
        Family::U8g2,
        Family::Lvgl,
        Family::Netbsd,
        Family::U8glib,
        Family::U8x8,
    ] {
        let writer = CCodec {
            write_options: CWriteOptions {
                family,
                ..Default::default()
            },
            ..Default::default()
        };
        let bytes = writer
            .write(
                &set,
                &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
            )
            .unwrap()
            .output
            .into_single()
            .unwrap();
        assert_eq!(
            detect_source(&bytes, &ResourceLimits::default()),
            Confidence::Certain,
            "{family:?}"
        );
        assert!(read(std::str::from_utf8(&bytes).unwrap(), Default::default()).is_ok());
        for limited in [
            ResourceLimits {
                max_total_bytes: bytes.len(),
                ..Default::default()
            },
            ResourceLimits {
                max_table_bytes: bytes.len() - 1,
                ..Default::default()
            },
        ] {
            assert_eq!(
                detect_source(&bytes, &limited),
                Confidence::NoMatch,
                "{family:?}"
            );
        }
    }
}
#[cfg(feature = "hex")]
#[test]
fn native_names_inside_real_hex_comments_never_override_hex() {
    use fontpixel::formats::Confidence;
    for token in [
        "U8G_FONT_SECTION",
        "U8G2_FONT_SECTION",
        "u8x8_font_chroma48medium8_r",
        "lv_font_fmt_txt_dsc_t",
        "wsdisplay_font",
        "GFXfont GFXglyph",
        "/* fontpixel-c: {} */",
        "const uint8_t u8x8_font_fake[] = {0};",
    ] {
        let text = format!(
            "# Source declaration: {token}\n0041:80808080808080808080808080808080\n0042:40404040404040404040404040404040\n"
        );
        assert_eq!(
            detect_source(text.as_bytes(), &ResourceLimits::default()),
            Confidence::NoMatch
        );
        let loaded = fontpixel::registry()
            .read(
                &Input::new(text.as_bytes()).with_name("misleading.h"),
                &ReadOptions::default(),
            )
            .unwrap();
        assert_eq!(loaded.format, "hex");
        assert_eq!(loaded.fonts.fonts[0].len(), 2);
    }
}
