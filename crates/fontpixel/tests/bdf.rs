//! BDF core editing, transports, safety and structured-loss contracts.
#![cfg(feature = "bdf")]
use fontpixel::bitmap::{BitmapLimits, Bounds, Gray8};
use fontpixel::formats::{
    Confidence,
    bdf::{self, BdfWriteOptions},
};
use fontpixel::model::{Codepoint, GlyphBitmap, Property, PropertyValue, Rational, Vector};
use fontpixel::{
    Bitmap, Error, Font, FontSet, Glyph, Input, ReadOptions, ResourceLimits, ResourcePath,
    WriteOptions,
};
const MINI: &str = "STARTFONT 2.1\nFONT tiny\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 8 0 -1\nSTARTPROPERTIES 3\nFAMILY_NAME \"tiny\"\nFONT_ASCENT 7\nFONT_DESCENT 1\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 1 1 0 0\nBITMAP\n8f\nENDCHAR\nENDFONT\n";
fn parse(s: &str) -> Font {
    bdf::read(s.as_bytes(), &ResourceLimits::default())
        .unwrap()
        .0
}
fn output(font: &Font) -> String {
    String::from_utf8(bdf::write(font, &BdfWriteOptions::default()).unwrap().0).unwrap()
}
fn strict(font: Font) -> Result<fontpixel::resources::Written, Error> {
    let mut o = WriteOptions::new(ResourcePath::new("x.bdf").unwrap());
    o.strict = true;
    fontpixel::write(&font.into(), "bdf", &o)
}
#[test]
fn detect_registry_and_feature_capabilities() {
    let registry = fontpixel::registry();
    let bdf = registry.status("bdf").unwrap();
    assert!(bdf.read && bdf.write && bdf.detect);
    assert!(
        registry
            .formats()
            .filter(|s| ![
                "bdf",
                "pcf",
                "yaff",
                "otb",
                "bdat",
                "color",
                "ttf",
                "otf",
                "woff",
                "woff2",
                "ttc",
                "vector-raster",
                "svg",
                "hex",
                "psf",
                "fnt",
                "fon",
                "image",
                "c",
                "kbitx",
                "pbf",
                "hbf"
            ]
            .contains(&s.format.id))
            .all(|s| !s.read && !s.write && !s.detect)
    );
    for prefix in ["", "\u{feff}", " \r\n\t\u{b}\u{c}"] {
        let input = format!("{prefix}{MINI}");
        assert_eq!(bdf::detect(input.as_bytes()), Confidence::Certain);
        assert_eq!(
            registry
                .detect(
                    &Input::new(input.as_bytes()).with_name("misleading.pcf"),
                    &ResourceLimits::default()
                )
                .unwrap()
                .format
                .id,
            "bdf"
        );
    }
    for source in [b"STARTFONTx 2.1".as_slice(), b"anything", b"\x01fcp"] {
        assert_eq!(bdf::detect(source), Confidence::NoMatch);
    }
    assert_eq!(bdf::detect(b"STARTFONT wrong"), Confidence::Certain);
}
#[test]
fn edits_override_only_changed_source_fields() {
    let mut f = parse(MINI);
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    assert!(output(&f).contains("\n8f\n"));
    f.edit_glyph(id, |g| {
        g.name = Some("edited name".into());
        g.shift(-2, 3)?;
        g.metrics.advance = Some(Vector::new(9, 1));
        Ok(())
    })
    .unwrap();
    let s = output(&f);
    assert!(s.contains("STARTCHAR edited name\n"));
    assert!(s.contains("DWIDTH 9 1\nBBX 1 1 -2 3\nBITMAP\n8f\n"));
    f.edit_glyph(id, |g| {
        let GlyphBitmap::Mono(b) = &mut g.bitmap else {
            unreachable!()
        };
        b.set_pixel(0, 0, 0)
    })
    .unwrap();
    assert!(output(&f).contains("BITMAP\n00\n"));
    f.metrics.bounds = Some(Bounds::new(10, 9, -3, -2));
    f.metrics.point_size = Some(Rational::integer(9));
    f.names.family = Some("new family".into());
    f.names.xlfd = Some("new name".into());
    let s = output(&f);
    assert!(s.contains("FONT new name\nSIZE 9 75 75\nFONTBOUNDINGBOX 10 9 -3 -2\n"));
    assert!(s.contains("FAMILY_NAME \"new family\""));
    let f2 = parse(&s);
    assert_eq!(f2.metrics.bounds, f.metrics.bounds);
    assert_eq!(f2.names.family, f.names.family);
}
#[test]
fn direct_property_edits_and_deletions_are_not_overridden_by_stale_views() {
    let mut f = parse(MINI);
    f.properties.set("FAMILY_NAME", "property edit");
    f.properties.remove("FONT_ASCENT");
    let s = output(&f);
    assert!(s.contains("FAMILY_NAME \"property edit\""));
    assert!(!s.contains("FONT_ASCENT"));
    f.names.family = Some("semantic edit".into());
    assert!(output(&f).contains("FAMILY_NAME \"semantic edit\""));
    f.headers.set(
        "SIZE",
        PropertyValue::List(vec![9_i64.into(), 72_i64.into(), 72_i64.into()]),
    );
    assert!(output(&f).contains("SIZE 9 72 72"));
    f.headers.remove("SIZE");
    assert!(bdf::write(&f, &BdfWriteOptions::default()).is_err());
}
#[test]
fn duplicate_unencoded_and_out_of_unicode_codes_remain_inspectable() {
    let data=MINI.replace("CHARS 1","CHARS 4").replace("ENDFONT\n", "STARTCHAR replacement\nENCODING 65\nBBX 0 0 0 0\nENDCHAR\nSTARTCHAR neg\nENCODING -9 123\nBBX 0 0 0 0\nENDCHAR\nSTARTCHAR out\nENCODING 2147483647\nBBX 0 0 0 0\nENDCHAR\nENDFONT\n");
    let (f, report) = bdf::read(data.as_bytes(), &ResourceLimits::default()).unwrap();
    assert_eq!(f.len(), 4);
    assert_eq!(f.glyph_ids_for_codepoint(Codepoint::Unicode(65)).len(), 2);
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .name
            .as_deref(),
        Some("replacement")
    );
    assert!(
        f.glyph_for_codepoint(Codepoint::Unicode(2147483647))
            .is_some()
    );
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "bdf.parse.duplicate_encoding")
    );
    let (bytes, write_report) = bdf::write(&f, &BdfWriteOptions::default()).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    assert!(s.contains("CHARS 3\n"));
    assert!(!s.contains("STARTCHAR A\n"));
    assert!(s.contains("ENCODING -1 123\n"));
    assert!(write_report.has_loss());
    assert!(matches!(strict(f), Err(Error::StrictLoss(_))));
}
#[test]
fn malformed_rasters_preserve_source_but_guard_drawing() {
    let (f, report) = bdf::read(
        MINI.replace("8f\n", "XX\nBAD\n").as_bytes(),
        &ResourceLimits::default(),
    )
    .unwrap();
    let (_, g) = f.glyphs().next().unwrap();
    assert!(
        bdf::validate_bitmap(g).unwrap_err().to_string().contains(
            "Glyph \"A\" (codepoint 65): bitmap row 0 \"XX\" contains a non-hex character"
        )
    );
    assert!(output(&f).contains("XX\nBAD\n"));
    assert_eq!(g.bitmap.as_bytes(), &[0]);
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "bdf.bitmap.invalid_hex")
    );
    let empty = parse(&MINI.replace("8f\n", ""));
    let (_, g) = empty.glyphs().next().unwrap();
    assert!(bdf::validate_bitmap(g).is_ok());
    assert_eq!(g.bitmap.as_bytes(), &[0]);
    let short = parse(&MINI.replace("BBX 1 1", "BBX 9 1").replace("8f", "f"));
    assert_eq!(
        short.glyphs().next().unwrap().1.bitmap.as_bytes(),
        &[0xf0, 0]
    );
}
#[test]
fn fresh_models_round_trip_without_source_cache() {
    let mut f = Font::default();
    f.metrics.point_size = Some(Rational::integer(8));
    f.metrics.resolution = Some(Vector::new(75, 75));
    f.metrics.bounds = Some(Bounds::new(8, 8, 0, -1));
    f.names.xlfd = Some("created".into());
    f.names.family = Some("authored".into());
    let mut g = Glyph::new("#.#\n.#.".parse::<Bitmap>().unwrap());
    g.codepoints.push(Codepoint::Unicode(66));
    g.metrics.advance = Some(Vector::new(5, 0));
    f.add_glyph(g).unwrap();
    let s = output(&f);
    assert!(
        s.contains("STARTCHAR uni0042\nENCODING 66\nDWIDTH 5 0\nBBX 3 2 0 0\nBITMAP\nA0\n40\n")
    );
    let again = parse(&s);
    assert_eq!(
        again
            .glyph_for_codepoint(Codepoint::Unicode(66))
            .unwrap()
            .bitmap,
        f.glyphs().next().unwrap().1.bitmap
    );
    assert_eq!(output(&again), s);
}
#[test]
fn reports_all_unrepresentable_fields_and_gray_requires_threshold() {
    let mut f = parse(MINI);
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.unicode_sequences.push(vec![65, 0x301]);
        g.properties.push(Property::new("UNKNOWN", "value"));
        g.metrics.image_offset = Some(Vector::new(1, 2));
        g.codepoints.push(Codepoint::Unicode(66));
        Ok(())
    })
    .unwrap();
    let (_, report) = bdf::write(&f, &BdfWriteOptions::default()).unwrap();
    for code in [
        "bdf.write.sequences",
        "bdf.write.glyph_property",
        "bdf.write.glyph_metric",
        "bdf.write.alias_identity",
    ] {
        assert!(report.entries.iter().any(|e| e.code == code), "{code}");
    }
    assert!(matches!(strict(f.clone()), Err(Error::StrictLoss(_))));
    f.edit_glyph(id, |g| {
        g.bitmap = Bitmap::<Gray8>::from_raw(1, 1, &[127], BitmapLimits::default())?.into();
        Ok(())
    })
    .unwrap();
    assert!(bdf::write(&f, &BdfWriteOptions::default()).is_err());
    let options = BdfWriteOptions {
        gray_threshold: Some(128),
        ..BdfWriteOptions::default()
    };
    let (bytes, r) = bdf::write(&f, &options).unwrap();
    assert!(String::from_utf8(bytes).unwrap().contains("BITMAP\n00\n"));
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "bdf.write.gray_quantization")
    );
    assert!(
        fontpixel::write(
            &FontSet {
                fonts: vec![f.clone(), f],
                ..FontSet::default()
            },
            "bdf",
            &WriteOptions::new(ResourcePath::new("x.bdf").unwrap())
        )
        .is_err()
    );
}
#[test]
fn source_metadata_charset_and_vertical_metrics() {
    let source=MINI.replace("STARTPROPERTIES 3","METRICSSET 2\nDWIDTH1 0 -8\nVVECTOR 4,7\nSTARTPROPERTIES 5\nCHARSET_REGISTRY \"GB2312.1980\"\nCHARSET_ENCODING \"0\"").replace("ENCODING 65","ENCODING 18507");
    let f = parse(&source);
    assert_eq!(f.charset.canonical_name(), "gb2312");
    assert_eq!(f.charset.gl, Some(true));
    assert!(f.glyph_for_codepoint(Codepoint::Legacy(18507)).is_some());
    assert_eq!(
        f.metrics.glyph_defaults.vertical_origin,
        Some(Vector::new(4, 7))
    );
    assert_eq!(f.metrics.metrics_set, Some(2));
    #[cfg(feature = "cjk-charsets")]
    assert_eq!(f.charset.codec().unwrap().cp2char(18507), Some('人'));
    #[cfg(not(feature = "cjk-charsets"))]
    assert!(f.charset.codec().is_err());
    assert!(output(&f).contains("ENCODING 18507\n"));
}
#[test]
fn rejects_malformed_numbers_and_enforces_all_resource_limits() {
    let base = ResourceLimits::default();
    for limits in [
        ResourceLimits {
            max_input_bytes: 10,
            ..base.clone()
        },
        ResourceLimits {
            max_total_bytes: 10,
            ..base.clone()
        },
        ResourceLimits {
            max_glyphs: 0,
            ..base.clone()
        },
        ResourceLimits {
            max_pixels_per_glyph: 0,
            ..base.clone()
        },
        ResourceLimits {
            max_table_bytes: 5,
            ..base.clone()
        },
    ] {
        assert!(matches!(
            bdf::read(MINI.as_bytes(), &limits),
            Err(Error::ResourceLimit { .. })
        ));
    }
    for (old, new) in [
        ("BBX 1 1 0 0", "BBX 2147483647 2147483647 0 0"),
        ("BBX 1 1 0 0", "BBX -1 1 0 0"),
        ("ENCODING 65", "ENCODING 2147483648"),
        ("SIZE 8 75 75", "SIZE inf 75 75"),
    ] {
        assert!(bdf::read(MINI.replace(old, new).as_bytes(), &base).is_err());
    }
    let f = parse(MINI);
    assert!(matches!(
        bdf::write(
            &f,
            &BdfWriteOptions {
                max_output_bytes: 10,
                ..BdfWriteOptions::default()
            }
        ),
        Err(Error::ResourceLimit { .. })
    ));
    let mut f = parse(MINI);
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.set_bitmap(
            Bitmap::<fontpixel::bitmap::Mono>::with_limits(
                0,
                i32::MAX as u32,
                BitmapLimits {
                    max_dimension: i32::MAX as u32,
                    ..BitmapLimits::default()
                },
            )?,
            0,
            0,
            false,
        )
    })
    .unwrap();
    let bytes = bdf::write(
        &f,
        &BdfWriteOptions {
            max_output_bytes: 10000,
            ..BdfWriteOptions::default()
        },
    )
    .unwrap()
    .0;
    assert!(bytes.len() < 10000);
    assert_eq!(
        parse(std::str::from_utf8(&bytes).unwrap())
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .height(),
        i32::MAX as u32
    );
}
#[cfg(feature = "gzip")]
#[test]
fn bounded_gzip_transport_is_deterministic_and_codec_independent() {
    use std::io::Write;
    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(bytes).unwrap();
        e.finish().unwrap()
    }
    let compressed = gzip(MINI.as_bytes());
    let loaded = fontpixel::read(&compressed, &ReadOptions::default()).unwrap();
    assert_eq!(loaded.format, "bdf");
    let o = WriteOptions::new(ResourcePath::new("font.bdf.gz").unwrap());
    let a = fontpixel::write(&loaded.fonts, "bdf", &o)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let b = fontpixel::write(&loaded.fonts, "bdf", &o)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(&a[..3], &[0x1f, 0x8b, 8]);
    assert_eq!(&a[4..8], &[0, 0, 0, 0]);
    assert_eq!(
        fontpixel::read(&a, &ReadOptions::default())
            .unwrap()
            .fonts
            .single()
            .unwrap()
            .len(),
        1
    );
    let mut two = gzip(&MINI.as_bytes()[..40]);
    two.extend(gzip(&MINI.as_bytes()[40..]));
    assert!(fontpixel::read(&two, &ReadOptions::default()).is_ok());
    let o = ReadOptions {
        limits: ResourceLimits {
            max_decompressed_bytes: 10,
            ..ResourceLimits::default()
        },
        ..ReadOptions::default()
    };
    assert!(matches!(
        fontpixel::read(&compressed, &o),
        Err(Error::ResourceLimit { .. })
    ));
    let o = ReadOptions {
        limits: ResourceLimits {
            max_input_bytes: compressed.len(),
            ..ResourceLimits::default()
        },
        ..ReadOptions::default()
    };
    assert!(fontpixel::read(&compressed, &o).is_ok());
    for bad in [
        &compressed[..8],
        &compressed[..compressed.len() - 2],
        b"\x1f\x8bgarbage",
    ] {
        assert!(
            fontpixel::read(bad, &ReadOptions::default())
                .unwrap_err()
                .to_string()
                .contains("gzip")
        );
    }
    let mut corrupt = compressed.clone();
    let i = corrupt.len() - 8;
    corrupt[i] ^= 1;
    assert!(fontpixel::read(&corrupt, &ReadOptions::default()).is_err());
    assert!(
        fontpixel::read(&gzip(&compressed), &ReadOptions::default())
            .unwrap_err()
            .to_string()
            .contains("nested gzip")
    );
}

#[test]
fn reserved_source_names_cannot_mask_unknown_field_loss() {
    let input = MINI
        .replace("FONT tiny", "bdf.source.headers value\nFONT tiny")
        .replace("DWIDTH 8 0", "bdf.bitmap.rows value\nDWIDTH 8 0");
    let f = parse(&input);
    assert!(
        f.headers
            .iter()
            .any(|p| p.name == "bdf.source.headers" && p.raw.is_some())
    );
    assert!(
        f.glyphs()
            .next()
            .unwrap()
            .1
            .properties
            .iter()
            .any(|p| p.name == "bdf.bitmap.rows" && p.raw.is_some())
    );
    let (bytes, report) = bdf::write(&f, &BdfWriteOptions::default()).unwrap();
    assert!(String::from_utf8(bytes).unwrap().contains("8f\n"));
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "bdf.write.unknown_header")
    );
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "bdf.write.glyph_property")
    );
    assert!(matches!(strict(f), Err(Error::StrictLoss(_))));
}
#[cfg(feature = "cjk-charsets")]
#[test]
fn changed_charset_properties_recompute_inferred_gl() {
    let mut f = parse(MINI);
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.codepoints = vec![Codepoint::Unicode('人' as u32)];
        Ok(())
    })
    .unwrap();
    f.properties.set("CHARSET_REGISTRY", "GB2312.1980");
    f.properties.set("CHARSET_ENCODING", "0");
    assert!(output(&f).contains("ENCODING 18507\n"));
    f.charset.gl = Some(true);
    assert!(output(&f).contains("ENCODING 18507\n"));
}
#[test]
fn trailing_numeric_tokens_do_not_allocate_a_token_vector() {
    let input = MINI.replace(
        "SIZE 8 75 75",
        &format!("SIZE 8 75 75 {}", "x ".repeat(50_000)),
    );
    let (f, _) = bdf::read(
        input.as_bytes(),
        &ResourceLimits {
            max_total_bytes: 600_000,
            ..ResourceLimits::default()
        },
    )
    .unwrap();
    assert!(output(&f).contains("SIZE 8 75 75\n"));
}
