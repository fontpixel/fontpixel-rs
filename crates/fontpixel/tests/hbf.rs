//! HBF grammar, resource, encoding and semantic roundtrip regressions.
#![cfg(feature = "hbf")]
use fontpixel::formats::hbf::{self, HbfWriteOptions};
use fontpixel::model::{Codepoint, Property, PropertyValue, Vector};
use fontpixel::resources::{Input, ReadContext, ResourceResolver};
use fontpixel::{Error, Font, ResourceLimits, ResourcePath, Result};
use std::collections::BTreeMap;

#[derive(Default)]
struct Files(BTreeMap<String, Vec<u8>>);
impl ResourceResolver for Files {
    fn resolve(&self, path: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let b = self
            .0
            .get(path.as_str())
            .ok_or_else(|| Error::MissingResource(path.to_string()))?;
        if b.len() > max {
            return Err(Error::ResourceLimit {
                resource: "fixture bytes",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
fn load(header: &str, files: &Files, limits: &ResourceLimits) -> Result<(Font, fontpixel::Report)> {
    let input = Input::new(header.as_bytes()).with_resources(files);
    hbf::read(&mut ReadContext::new(&input, limits)?)
}
fn fixture(scheme: &str, byte_ranges: &str, ranges: &str, chars: usize) -> String {
    format!(
        "COMMENT header\nHBF_START_FONT 1.1\nHBF_CODE_SCHEME {scheme}\nFONT test-font\nSIZE 12 72 72\nHBF_BITMAP_BOUNDING_BOX 9 2 -1 -1\nFONTBOUNDINGBOX 11 4 -2 -2\nSTARTPROPERTIES 4\nDEFAULT_CHAR 0x41\nFAMILY_NAME \"Test \"\"Family\"\"\"\nCUSTOM 077\nCUSTOM -0x2\nENDPROPERTIES\nCHARS {chars}\n{byte_ranges}HBF_START_CODE_RANGES {}\n{ranges}HBF_END_CODE_RANGES\nHBF_END_FONT\n",
        ranges.lines().count()
    )
}
fn one() -> (String, Files) {
    (
        fixture("Unicode 1.1", "", "HBF_CODE_RANGE 0x41-0x42 a.bin 2\n", 2),
        Files(BTreeMap::from([(
            "a.bin".into(),
            vec![99, 88, 0x80, 0x80, 0x40, 0, 0x40, 0, 0x80, 0x80],
        )])),
    )
}
fn read_bundle(out: &fontpixel::resources::Written) -> (Font, fontpixel::Report) {
    let primary = out.output.primary().as_str();
    let root = primary.rsplit_once('/').map_or("", |(p, _)| p);
    let mut files = Files::default();
    let mut header = None;
    for asset in out.output.assets() {
        if asset.path.as_str() == primary {
            header = Some(std::str::from_utf8(&asset.bytes).unwrap());
        } else {
            let name = if root.is_empty() {
                asset.path.as_str()
            } else {
                asset
                    .path
                    .as_str()
                    .strip_prefix(&format!("{root}/"))
                    .unwrap()
            };
            files.0.insert(name.into(), asset.bytes.clone());
        }
    }
    load(header.unwrap(), &files, &ResourceLimits::default()).unwrap()
}
#[test]
fn native_roundtrip_distinct_bounds_offsets_properties_and_comments() {
    let (h, f) = one();
    let (font, r) = load(&h, &f, &ResourceLimits::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(font.len(), 2);
    assert_eq!(font.names.family.as_deref(), Some("Test \"Family\""));
    assert_eq!(
        font.properties.get("CUSTOM"),
        Some(&PropertyValue::Integer(-2))
    );
    assert_eq!(
        font.glyphs().next().unwrap().1.metrics.advance,
        Some(Vector::new(11, 0))
    );
    let out = hbf::write(
        &font,
        &ResourcePath::new("sub/font.hbf").unwrap(),
        &HbfWriteOptions::default(),
    )
    .unwrap();
    assert!(!out.report.has_loss(), "{:?}", out.report);
    let (next, r) = read_bundle(&out);
    assert!(!r.has_loss());
    assert_eq!(font, next);
    let second = hbf::write(
        &next,
        &ResourcePath::new("sub/font.hbf").unwrap(),
        &HbfWriteOptions::default(),
    )
    .unwrap();
    assert_eq!(out, second);
}
#[test]
fn byte_gaps_multi_file_offsets_are_ranked_not_scanned() {
    let b = "HBF_START_BYTE_2_RANGES 2\nHBF_BYTE_2_RANGE 0x40-0x41\nHBF_BYTE_2_RANGE 0xFE-0xFF\nHBF_END_BYTE_2_RANGES\n";
    let h = fixture(
        "Vendor Encoding",
        b,
        "HBF_CODE_RANGE 0xA141-0xA240 a.bin 1\nHBF_CODE_RANGE 0xA241-0xA241 b.bin 3\n",
        5,
    );
    let f = Files(BTreeMap::from([
        ("a.bin".into(), vec![0; 17]),
        ("b.bin".into(), vec![0; 7]),
    ]));
    let (font, r) = load(&h, &f, &ResourceLimits::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(
        font.glyphs()
            .map(|(_, g)| g.codepoints[0])
            .collect::<Vec<_>>(),
        [0xa141, 0xa1fe, 0xa1ff, 0xa240, 0xa241].map(Codepoint::Legacy)
    );
    let out = hbf::write(
        &font,
        &ResourcePath::new("font.hbf").unwrap(),
        &HbfWriteOptions {
            glyphs_per_file: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(out.output.assets().len(), 4);
    let (next, _) = read_bundle(&out);
    for ((_, a), (_, b)) in font.glyphs().zip(next.glyphs()) {
        assert_eq!(a, b);
    }
    assert_eq!(next.charset.name, font.charset.name);
}
#[test]
fn three_byte_spaces_keep_explicit_planes() {
    let b = "HBF_START_BYTE_2_RANGES 1\nHBF_BYTE_2_RANGE 2-3\nHBF_END_BYTE_2_RANGES\nHBF_START_BYTE_3_RANGES 2\nHBF_BYTE_3_RANGE 4-4\nHBF_BYTE_3_RANGE 7-7\nHBF_END_BYTE_3_RANGES\n";
    let h = fixture(
        "Vendor 3 byte",
        b,
        "HBF_CODE_RANGE 0x10207-0x10307 a.bin 0\n",
        3,
    );
    let f = Files(BTreeMap::from([("a.bin".into(), vec![0; 12])]));
    let (font, _) = load(&h, &f, &ResourceLimits::default()).unwrap();
    assert_eq!(
        font.glyphs()
            .map(|(_, g)| g.codepoints[0].value())
            .collect::<Vec<_>>(),
        vec![0x10207, 0x10304, 0x10307]
    );
    let out = hbf::write(
        &font,
        &ResourcePath::new("f.hbf").unwrap(),
        &HbfWriteOptions::default(),
    )
    .unwrap();
    assert_eq!(read_bundle(&out).0, font);
}
#[test]
fn known_gl_gr_domains_never_substitute_unicode() {
    for (scheme, first, second, gl) in [
        ("GB2312-1980", 0x2121, "0x21-0x7E", true),
        ("JISX0208-1990", 0xa1a1, "0xA1-0xFE", false),
        ("KSC5601-1987", 0x2121, "0x21-0x7E", true),
        ("CNS11643-92p3", 0x2121, "0x21-0x7E", true),
    ] {
        let b = format!(
            "HBF_START_BYTE_2_RANGES 1\nHBF_BYTE_2_RANGE {second}\nHBF_END_BYTE_2_RANGES\n"
        );
        let h = fixture(
            scheme,
            &b,
            &format!("HBF_CODE_RANGE {first}-{first} a.bin 0\n"),
            1,
        );
        let f = Files(BTreeMap::from([("a.bin".into(), vec![0; 4])]));
        let (font, _) = load(&h, &f, &ResourceLimits::default()).unwrap();
        assert_eq!(
            font.glyphs().next().unwrap().1.codepoints,
            vec![Codepoint::Legacy(first)]
        );
        if !scheme.starts_with("CNS") {
            assert_eq!(font.charset.gl, Some(gl));
        } else {
            assert_eq!(font.charset.name.as_deref(), Some(scheme));
            assert_eq!(font.charset.gl, None);
        }
    }
}
#[test]
fn malformed_structure_ranges_offsets_resources_and_limits_fail() {
    let (h, f) = one();
    for bad in [
        h.replace("1.1", "1.0"),
        h.replace("CHARS 2", "CHARS 3"),
        h.replace("STARTPROPERTIES 4", "STARTPROPERTIES 3"),
        h.replace("CUSTOM 077", "CUSTOM 089"),
        h.replace("0x41-0x42", "0x42-0x41"),
        h.replace("a.bin 2", "a.bin 3"),
        h.replace("a.bin 2", "../a.bin 2"),
        h.replace("a.bin 2", "missing.bin 2"),
        h.replace("DEFAULT_CHAR 0x41", "DEFAULT_CHARACTER 0x41"),
        h.replace("FONTBOUNDINGBOX 11 4 -2 -2", "FONTBOUNDINGBOX 8 2 0 0"),
        h.replace("HBF_END_FONT", "UNKNOWN\nHBF_END_FONT"),
        h.replace("Test", "Tést"),
    ] {
        assert!(
            load(&bad, &f, &ResourceLimits::default()).is_err(),
            "accepted {bad}"
        );
    }
    for limits in [
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 17,
            ..Default::default()
        },
        ResourceLimits {
            max_resource_bytes: 9,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: h.len() + 3,
            ..Default::default()
        },
        ResourceLimits {
            max_resources: 0,
            ..Default::default()
        },
    ] {
        assert!(load(&h, &f, &limits).is_err());
    }
}
#[test]
fn padding_loss_and_writer_losses_are_explicit() {
    let (h, mut f) = one();
    f.0.get_mut("a.bin").unwrap()[3] = 0xff;
    let (mut font, r) = load(&h, &f, &ResourceLimits::default()).unwrap();
    assert!(r.has_loss());
    font.names.localized.push(Property::new("ja", "name"));
    font.metrics.line_height = Some(20);
    font.headers
        .push(Property::new("other", PropertyValue::Flag));
    font.properties.push(Property::new(
        "UNREPRESENTABLE",
        PropertyValue::Bytes(vec![1, 2]),
    ));
    let out = hbf::write(
        &font,
        &ResourcePath::new("f.hbf").unwrap(),
        &HbfWriteOptions::default(),
    )
    .unwrap();
    for code in ["hbf.names", "hbf.metadata", "hbf.header", "hbf.property"] {
        assert!(
            out.report.entries.iter().any(|e| e.code == code),
            "missing {code}"
        );
    }
    assert!(!read_bundle(&out).1.has_loss());
}
#[test]
fn source_cache_and_unsafe_output_paths_are_bounded() {
    let b = "HBF_START_BYTE_2_RANGES 1\nHBF_BYTE_2_RANGE 0-255\nHBF_END_BYTE_2_RANGES\n";
    let h = fixture(
        "Vendor",
        b,
        "HBF_CODE_RANGE 256-256 a.bin 0\nHBF_CODE_RANGE 257-257 a.bin 4\n",
        2,
    );
    let f = Files(BTreeMap::from([("a.bin".into(), vec![0; 8])]));
    let (font, _) = load(
        &h,
        &f,
        &ResourceLimits {
            max_resources: 1,
            ..Default::default()
        },
    )
    .unwrap();
    for options in [
        HbfWriteOptions {
            bitmap_name: Some(ResourcePath::new("f.hbf").unwrap()),
            ..Default::default()
        },
        HbfWriteOptions {
            bitmap_name: Some(ResourcePath::new("a b.bin").unwrap()),
            ..Default::default()
        },
        HbfWriteOptions {
            glyphs_per_file: Some(0),
            ..Default::default()
        },
        HbfWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: 100,
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        assert!(hbf::write(&font, &ResourcePath::new("f.hbf").unwrap(), &options).is_err());
    }
}

#[test]
fn grayscale_requires_threshold_and_reports_conversion() {
    use fontpixel::bitmap::{Bitmap, BitmapLimits, Bounds, Gray8};
    use fontpixel::model::{Glyph, GlyphBitmap};
    let mut font = Font::default();
    let mut g = Glyph::new(
        Bitmap::<Gray8>::from_raw(
            9,
            1,
            &[0, 127, 128, 255, 128, 0, 127, 255, 255],
            BitmapLimits::default(),
        )
        .unwrap(),
    );
    g.codepoints.push(Codepoint::Unicode(65));
    g.metrics.advance = Some(Vector::new(9, 0));
    font.add_glyph(g).unwrap();
    font.metrics.bounds = Some(Bounds::new(9, 1, 0, 0));
    let primary = ResourcePath::new("gray.hbf").unwrap();
    assert!(hbf::write(&font, &primary, &HbfWriteOptions::default()).is_err());
    let out = hbf::write(
        &font,
        &primary,
        &HbfWriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        out.report
            .entries
            .iter()
            .any(|e| e.code == "hbf.gray_threshold")
    );
    let (back, _) = read_bundle(&out);
    assert!(matches!(
        back.glyphs().next().unwrap().1.bitmap,
        GlyphBitmap::Mono(_)
    ));
    assert_eq!(
        back.glyphs().next().unwrap().1.bitmap.as_bytes(),
        &[0x39, 0x80]
    );
}

#[test]
fn aliases_mixed_domains_empty_glyphs_and_trailing_slots_are_explicit() {
    let (h, f) = one();
    let (original, _) = load(&h, &f, &ResourceLimits::default()).unwrap();
    let first = original.glyphs().next().unwrap().0;
    for mutation in 0..4 {
        let mut font = original.clone();
        font.edit_glyph(first, |g| {
            match mutation {
                0 => g.codepoints.push(Codepoint::Unicode(99)),
                1 => g.codepoints.clear(),
                2 => g.unicode_sequences.push(vec![65, 66]),
                _ => g.codepoints[0] = Codepoint::Legacy(65),
            }
            Ok(())
        })
        .unwrap();
        assert!(
            hbf::write(
                &font,
                &ResourcePath::new("f.hbf").unwrap(),
                &HbfWriteOptions::default()
            )
            .is_err()
        );
    }
    let mut font = original;
    let last = font.glyphs().last().unwrap().0;
    font.remove_glyph(last).unwrap();
    let out = hbf::write(
        &font,
        &ResourcePath::new("f.hbf").unwrap(),
        &HbfWriteOptions::default(),
    )
    .unwrap();
    assert!(
        out.report
            .entries
            .iter()
            .any(|e| e.code == "hbf.glyph_order")
    );
}

#[test]
fn text_controls_and_sparse_endpoints_are_rejected() {
    let (h, f) = one();
    for bad in [
        h.replace("Test", "Bad\tName"),
        h.replace("header", "Bad\rComment"),
        h.replace("test-font", "Bad\tName"),
        h.replace("0x41-0x42", "0xD800-0xD801"),
        h.replace("0x41-0x42", "0x4 1-0x42"),
    ] {
        assert!(load(&bad, &f, &ResourceLimits::default()).is_err());
    }
    let b = "HBF_START_BYTE_2_RANGES 2\nHBF_BYTE_2_RANGE 0x40-0x41\nHBF_BYTE_2_RANGE 0xFE-0xFF\nHBF_END_BYTE_2_RANGES\n";
    let h = fixture("Vendor", b, "HBF_CODE_RANGE 0xA142-0xA1FE a.bin 0\n", 1);
    assert!(load(&h, &f, &ResourceLimits::default()).is_err());
    let overlapping = h.replace("0xFE-0xFF", "0x41-0xFF");
    assert!(load(&overlapping, &f, &ResourceLimits::default()).is_err());
    assert_eq!(
        hbf::detect(b"COMMENT\ta\nHBF_START_FONT\t1.1\n"),
        fontpixel::formats::Confidence::Certain
    );
}

#[test]
fn complete_metadata_comparison_raw_types_and_size_validation() {
    let (h, f) = one();
    let (mut font, _) = load(&h, &f, &ResourceLimits::default()).unwrap();
    font.metrics.ppem_x = Some(12);
    font.metrics.ppem_y = Some(13);
    font.charset.windows_charset = Some(1);
    font.charset.code_page = Some(1252);
    let mut p = Property::new("CUSTOM_RAW", PropertyValue::Unsigned(1));
    p.raw = Some(b"original opaque bytes".to_vec());
    font.properties.push(p);
    let mut p = Property::new("hbf.comment", "comment");
    p.raw = Some(b"COMMENT\tcomment".to_vec());
    font.headers.push(p);
    let out = hbf::write(
        &font,
        &ResourcePath::new("f.hbf").unwrap(),
        &HbfWriteOptions::default(),
    )
    .unwrap();
    for code in [
        "hbf.metadata",
        "hbf.charset",
        "hbf.property_raw",
        "hbf.property_type",
        "hbf.header_raw",
    ] {
        assert!(
            out.report.entries.iter().any(|e| e.code == code),
            "missing {code}"
        );
    }
    let (back, _) = read_bundle(&out);
    assert_eq!(back.metrics.ppem_x, None);
    assert_eq!(back.metrics.ppem_y, None);
    assert_eq!(
        back.properties.get("CUSTOM_RAW"),
        Some(&PropertyValue::Integer(1))
    );
    font.metrics.point_size = None;
    font.metrics.pixel_size = Some(0);
    assert!(
        hbf::write(
            &font,
            &ResourcePath::new("f.hbf").unwrap(),
            &HbfWriteOptions::default()
        )
        .is_err()
    );
}

#[test]
fn duplicate_and_invalid_native_structural_metadata_are_rejected() {
    let (h, f) = one();
    let (font, _) = load(&h, &f, &ResourceLimits::default()).unwrap();
    let mut duplicate = font.clone();
    duplicate
        .headers
        .push(Property::new("hbf.code_scheme", "Unicode 1.1"));
    assert!(
        hbf::write(
            &duplicate,
            &ResourcePath::new("f.hbf").unwrap(),
            &HbfWriteOptions::default()
        )
        .is_err()
    );
    let mut invalid = font;
    invalid
        .headers
        .set("hbf.code_scheme", PropertyValue::Integer(12));
    assert!(
        hbf::write(
            &invalid,
            &ResourcePath::new("f.hbf").unwrap(),
            &HbfWriteOptions::default()
        )
        .is_err()
    );
}
