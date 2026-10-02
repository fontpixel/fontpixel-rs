//! Lossless model, index maintenance, resource validation and real-model registry tests.
use fontpixel::bitmap::{Bounds, Gray8};
use fontpixel::model::*;
use fontpixel::{Bitmap, Font, FontSet, Glyph, ResourceLimits, Result};

fn glyph(cp: u32, text: &str) -> Glyph {
    let mut glyph = Glyph::new(text.parse::<Bitmap>().unwrap());
    glyph.codepoints.push(Codepoint::Unicode(cp));
    glyph
}
#[test]
fn original_order_duplicates_sequences_and_transactional_edit() -> Result<()> {
    let mut font = Font::default();
    let mut a = glyph(66, "#");
    a.codepoints.extend([
        Codepoint::Unicode(65),
        Codepoint::Legacy(0x80),
        Codepoint::Unicode(65),
    ]);
    a.unicode_sequences = vec![vec![65, 0x301], vec![65, 0x301]];
    let first = font.add_glyph(a.clone())?;
    let unencoded = font.add_glyph(Glyph::default())?;
    let last = font.add_glyph(glyph(65, "."))?;
    assert_eq!(font.len(), 3);
    assert_eq!(
        font.glyphs().map(|(id, _)| id).collect::<Vec<_>>(),
        vec![first, unencoded, last]
    );
    assert_eq!(font.glyph_id(Codepoint::Unicode(65)), Some(last));
    assert_eq!(
        font.glyph_ids_for_codepoint(Codepoint::Unicode(65)),
        &[first, last]
    );
    assert_eq!(font.glyph_for_sequence(&[65, 0x301]), Some(&a));
    assert_eq!(
        font.codepoints().map(|(cp, _)| cp).collect::<Vec<_>>(),
        vec![
            Codepoint::Unicode(65),
            Codepoint::Unicode(66),
            Codepoint::Legacy(0x80)
        ]
    );
    assert_eq!(
        font.codepoint_range(Codepoint::Unicode(100), Codepoint::Unicode(0))
            .count(),
        0
    );
    assert_eq!(
        font.codepoint_range(Codepoint::Unicode(65), Codepoint::Unicode(66))
            .count(),
        2
    );
    let snapshot = font.clone();
    assert!(
        font.edit_glyph(first, |g| {
            g.metrics.bounds.width = 10;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(font, snapshot);
    assert!(
        font.edit_glyph(first, |_| Err(fontpixel::Error::InvalidInput(
            "no edit".into()
        )))
        .is_err()
    );
    assert_eq!(font, snapshot);
    font.edit_glyph(first, |g| {
        g.codepoints = vec![Codepoint::Unicode(67)];
        g.unicode_sequences.clear();
        Ok(())
    })?;
    assert!(font.glyph_for_sequence(&[65, 0x301]).is_none());
    assert!(font.glyph_id(Codepoint::Legacy(0x80)).is_none());
    assert_eq!(font.glyph_id(Codepoint::Unicode(67)), Some(first));
    font.metrics.default_glyph = Some(last);
    assert!(font.remove_glyph(last).is_some());
    assert_eq!(font.metrics.default_glyph, None);
    assert!(font.glyph_id(Codepoint::Unicode(65)).is_none());
    assert!(font.remove_glyph(last).is_none());
    let next = font.add_glyph(glyph(65, "#"))?;
    assert!(next.index() > last.index());
    assert!(font.replace_glyph(last, a).is_err());
    font.validate(&ResourceLimits::default())?;
    Ok(())
}
#[test]
fn model_preserves_every_source_field() -> Result<()> {
    let mut g = glyph(0x10000, "##\n.#");
    g.name = Some("uniOriginal".into());
    g.unicode_sequences.push(vec![0x61, 0x301]);
    g.metrics = GlyphMetrics {
        bounds: Bounds::new(2, 2, -2, -1),
        advance: Some(Vector::new(9, 1)),
        scalable_advance: Some(Vector::new(650, 2)),
        vertical_advance: Some(Vector::new(3, -16)),
        vertical_scalable_advance: Some(Vector::new(4, -1000)),
        vertical_origin: Some(Vector::new(5, 13)),
        abc: Some(AbcWidths { a: -2, b: 2, c: 9 }),
        left_bearing: Some(-2),
        right_bearing: Some(9),
        image_offset: Some(Vector::new(1, 4)),
    };
    g.properties.push(Property {
        name: "ENCODING".into(),
        value: PropertyValue::Integer(-1),
        raw: Some(b"ENCODING -1 66\r\n".to_vec()),
    });
    let mut font = Font::from_glyphs([g.clone(), Glyph::default()])?;
    font.source_format = Some("bdf".into());
    font.charset = FontCharset {
        registry: Some("Microsoft".into()),
        encoding: Some("1252".into()),
        name: None,
        gl: None,
        windows_charset: Some(0),
        code_page: Some(1252),
        code_ranges: vec![(0, 255)],
    };
    assert_eq!(font.charset.codec().unwrap().cp2char(0x80), Some('€'));
    let named = FontCharset {
        name: Some("Windows-1252".into()),
        ..FontCharset::default()
    };
    assert_eq!(named.canonical_name(), "cp1252");
    assert_eq!(named.codec().unwrap().cp2char(0x80), Some('€'));

    font.names = FontNames {
        family: Some("Family".into()),
        style: Some("Regular".into()),
        full: Some("Family Regular".into()),
        postscript: Some("Family-Regular".into()),
        version: Some("1.0".into()),
        copyright: Some("Copyright".into()),
        license: Some("MIT".into()),
        license_url: Some("https://example.test/license".into()),
        foundry: Some("Foundry".into()),
        xlfd: Some("-foundry-family-...".into()),
        xlfd_fields: vec!["".into(), "foundry".into()],
        localized: vec![Property::new("ja", "Family JP")].into(),
    };
    font.metrics.point_size = Some(Rational::new(125, 10)?);
    font.metrics.pixel_size = Some(16);
    font.metrics.ppem_x = Some(15);
    font.metrics.ppem_y = Some(16);
    font.metrics.resolution = Some(Vector::new(75, 75));
    font.metrics.ascent = Some(13);
    font.metrics.descent = Some(3);
    font.metrics.line_gap = Some(1);
    font.metrics.line_height = Some(17);
    font.metrics.underline_position = Some(-2);
    font.metrics.underline_thickness = Some(1);
    font.metrics.space_width = Some(8);
    font.metrics.spacing = Some(Spacing::CharacterCell);
    font.metrics.metrics_set = Some(2);
    font.metrics.glyph_defaults = g.metrics.clone();
    font.headers.push(Property::new(
        "bdfversion",
        PropertyValue::Decimal("2.2".into()),
    ));
    font.headers.push(Property::new("COMMENT", "first"));
    font.headers.push(Property::new("COMMENT", "second"));
    font.properties.push(Property {
        name: "MinSpace".into(),
        value: 3i64.into(),
        raw: Some(b"MinSpace  3\n".to_vec()),
    });
    font.properties
        .push(Property::new("UNKNOWN", PropertyValue::Bytes(vec![0, 255])));
    font.properties
        .push(Property::new("NoValue", PropertyValue::Flag));
    font.update_bounds()?;
    assert_eq!(font.metrics.bounds, Some(Bounds::new(2, 2, -2, -1)));
    let set = FontSet {
        fonts: vec![font.clone(), font.clone()],
        properties: vec![Property::new("container", "original")].into(),
        source_format: Some("fon".into()),
    };
    assert_eq!(set, set.clone());
    assert_eq!(set.fonts[0].glyphs().next().unwrap().1, &g);
    assert_eq!(
        set.fonts[0]
            .properties
            .iter()
            .next()
            .unwrap()
            .raw
            .as_deref(),
        Some(&b"MinSpace  3\n"[..])
    );
    assert!(set.single().is_err());
    assert!(set.into_single().is_err());
    assert!(FontSet::default().into_single().is_err());
    assert_eq!(FontSet::from(font.clone()).into_single()?, font);
    assert!(Rational::new(1, 0).is_err());
    Ok(())
}
#[test]
fn property_edits_keep_spelling_and_order_clear_raw() {
    let mut properties: Properties = vec![
        Property::new("MinSpace", 3i64),
        Property {
            name: "minspace".into(),
            value: 4i64.into(),
            raw: Some(b"old".to_vec()),
        },
    ]
    .into();
    assert_eq!(properties.get("MinSpace"), Some(&3i64.into()));
    assert_eq!(
        properties.get_case_insensitive("MINSPACE"),
        Some(&4i64.into())
    );
    properties.set("MINSPACE", 5i64);
    assert_eq!(
        properties
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["MinSpace", "minspace"]
    );
    assert_eq!(properties.iter().last().unwrap().raw, None);
    properties.set(
        "CUSTOM",
        PropertyValue::List(vec!["a".into(), PropertyValue::Unsigned(u64::MAX)]),
    );
    assert_eq!(properties.len(), 3);
    assert_eq!(properties.remove("mInSpAcE"), 2);
    assert_eq!(properties.into_vec()[0].name, "CUSTOM");
}
#[test]
fn glyph_cells_trimming_bounds_and_budget_errors() -> Result<()> {
    let mut g = glyph(65, ".#.\n###");
    g.metrics.bounds.x = -1;
    g.metrics.bounds.y = -1;
    let cell = g.bitmap_in_cell(Bounds::new(5, 4, -2, -2))?;
    if let GlyphBitmap::Mono(bitmap) = cell {
        assert_eq!(bitmap.to_string(), ".....\n..#..\n.###.\n.....");
    } else {
        panic!("depth changed");
    }
    g.metrics.advance = Some(Vector::new(5, 0));
    g.set_bitmap("...\n.#.\n...".parse::<Bitmap>()?, -2, -2, true)?;
    assert_eq!(g.metrics.bounds, Bounds::new(1, 1, -1, -1));
    assert_eq!(g.metrics.advance, Some(Vector::new(5, 0)));
    g.set_bitmap(Bitmap::<Gray8>::new(3, 3)?, 99, 99, true)?;
    assert_eq!(g.metrics.bounds, Bounds::default());
    assert!(matches!(g.bitmap, GlyphBitmap::Gray8(_)));
    g.shift(i64::MAX, 0)?;
    let clone = g.clone();
    assert!(g.shift(1, 0).is_err());
    assert_eq!(g, clone);
    g = glyph(65, "#");
    assert!(g.shift(i64::MAX, 0).is_err());
    let mut font = Font::from_glyphs([glyph(65, "##")])?;
    font.metrics.bounds = Some(Bounds::new(4, 4, 0, 0));
    let mut limits = ResourceLimits {
        max_pixels_per_glyph: 1,
        ..ResourceLimits::default()
    };
    assert!(font.validate(&limits).is_err());
    limits.max_pixels_per_glyph = 2;
    font.validate(&limits)?;
    limits.max_glyphs = 1;
    assert!(
        FontSet {
            fonts: vec![font.clone(), font.clone()],
            ..FontSet::default()
        }
        .validate(&limits)
        .is_err()
    );
    let id = font.glyphs().next().unwrap().0;
    font.remove_glyph(id);
    font.update_bounds()?;
    assert_eq!(font.metrics.bounds, Some(Bounds::new(4, 4, 0, 0)));
    let mut bad = glyph(0x110000, "#");
    bad.validate()?;
    assert_eq!(bad.codepoints[0].scalar(), None);
    let font = Font::from_glyphs([bad.clone()])?;
    assert_eq!(
        font.glyph_for_codepoint(Codepoint::Unicode(0x110000)),
        Some(&bad)
    );
    bad.codepoints = vec![Codepoint::Unicode(0xd800)];
    bad.validate()?;
    assert_eq!(bad.codepoints[0].scalar(), None);
    assert_eq!(Codepoint::Unicode(65).scalar(), Some('A'));
    assert_eq!(Codepoint::Legacy(65).scalar(), None);
    bad.unicode_sequences = vec![vec![0x110000]];
    assert!(bad.validate().is_err());
    bad.unicode_sequences = vec![vec![]];
    assert!(bad.validate().is_err());
    Ok(())
}

#[test]
fn real_model_registry_preserves_reports_sidecars_and_strictness() -> Result<()> {
    use fontpixel::formats::*;
    use fontpixel::report::*;
    use fontpixel::resources::*;
    struct Codec;
    impl FormatReader<FontSet> for Codec {
        fn read(&self, input: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
            let _ = input.resource(&ResourcePath::new("sidecar.bin")?)?;
            let mut report = Report::default();
            report.push(ReportEntry {
                level: Level::Info,
                impact: Impact::Repair,
                code: "model.repair".into(),
                location: Location::default(),
                message: "synthetic read repair".into(),
            });
            Ok((Font::from_glyphs([glyph(65, "#")])?.into(), report))
        }
    }
    impl FormatWriter<FontSet> for Codec {
        fn write(&self, _: &FontSet, options: &WriteOptions) -> Result<Written> {
            let mut report = Report::default();
            report.push(ReportEntry {
                level: Level::Warning,
                impact: Impact::Loss,
                code: "model.loss".into(),
                location: Location::default(),
                message: "synthetic deliberate loss".into(),
            });
            Ok(Written {
                output: OutputBundle::new(
                    options.primary_name.clone(),
                    vec![
                        OutputAsset {
                            path: options.primary_name.clone(),
                            bytes: vec![1],
                        },
                        OutputAsset {
                            path: ResourcePath::new("side.bin")?,
                            bytes: vec![2],
                        },
                    ],
                )?,
                report,
            })
        }
    }
    let mut registry = fontpixel::registry();
    assert!(
        registry
            .formats()
            .filter(|f| f.format.id != "bdf" && f.format.id != "pcf")
            .all(|f| !f.read && !f.write)
    );
    registry.register(Registration {
        format: FormatInfo {
            id: "test-model",
            name: "Synthetic model test",
            extensions: &["testmodel"],
            feature_enabled: true,
        },
        detector: None,
        reader: Some(Box::new(Codec)),
        writer: Some(Box::new(Codec)),
    })?;
    let options = ReadOptions {
        format: Some("test-model".into()),
        ..ReadOptions::default()
    };
    assert!(registry.read(&Input::new(&[]), &options).is_err());
    struct Sidecar;
    impl ResourceResolver for Sidecar {
        fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
            assert_eq!(path.as_str(), "sidecar.bin");
            if max_bytes < 1 {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "sidecar",
                    limit: max_bytes,
                });
            }
            Ok(vec![42])
        }
    }
    let loaded = registry.read(&Input::new(&[]).with_resources(&Sidecar), &options)?;
    assert_eq!(loaded.fonts.single()?.len(), 1);
    let single = loaded.into_single()?;
    assert_eq!(single.fonts.len(), 1);
    assert_eq!(single.format, "test-model");
    assert_eq!(single.report.entries[0].impact, Impact::Repair);
    let mut limited = options.clone();
    limited.limits.max_resource_bytes = 0;
    assert!(
        registry
            .read(&Input::new(&[]).with_resources(&Sidecar), &limited)
            .is_err()
    );

    let mut write = WriteOptions::new(ResourcePath::new("main.bin")?);
    let set: FontSet = Font::from_glyphs([glyph(65, "#")])?.into();
    let written = registry.write(&set, "test-model", &write)?;
    assert!(written.output.into_single().is_err());
    let combined = registry.write_with_report(&set, "test-model", &write, single.report)?;
    assert_eq!(combined.report.entries.len(), 2);
    assert_eq!(combined.report.entries[0].code, "model.repair");
    assert_eq!(combined.report.entries[1].code, "model.loss");
    write.strict = true;
    assert!(matches!(
        registry.write(&set, "test-model", &write),
        Err(fontpixel::Error::StrictLoss(_))
    ));
    Ok(())
}
