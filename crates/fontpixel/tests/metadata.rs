//! Public, atomic normalized-name edits and their owned-storage budgets.
use fontpixel::{
    Bitmap, Error, Font, FontSet, Glyph, ResourceLimits,
    metadata::{NameOverrides, apply_names},
    model::{Codepoint, FontNames, Property, Vector},
    report::{Impact, Level},
};

fn sample() -> FontSet {
    let mut font = Font::default();
    font.names = FontNames {
        family: Some("Original".into()),
        style: Some("Regular".into()),
        full: Some("Original Regular".into()),
        postscript: Some("Original-Regular".into()),
        version: Some("Version 1".into()),
        copyright: Some("Original copyright".into()),
        license: Some("Original license".into()),
        license_url: Some("https://example.org/license".into()),
        foundry: Some("Original foundry".into()),
        xlfd: Some("-source-original-medium-r-normal--8-80-75-75-c-80-iso10646-1".into()),
        xlfd_fields: vec!["source".into(), "original".into()],
        localized: vec![Property::new("localized_name", "Source display")].into(),
    };
    font.source_format = Some("source-strike".into());
    font.headers.push(Property::new("COMMENT", "Original header"));
    font.properties.push(Property {
        name: "COPYRIGHT".into(),
        value: "Original copyright".into(),
        raw: Some(b"COPYRIGHT \"Original copyright\"\r\n".to_vec()),
    });
    let mut glyph = Glyph::new("#.\n##".parse::<Bitmap>().unwrap());
    glyph.codepoints = vec![Codepoint::Unicode(65), Codepoint::Unicode(66)];
    glyph.unicode_sequences = vec![vec![65, 0x301]];
    glyph.metrics.advance = Some(Vector::new(4, 0));
    glyph.name = Some("A".into());
    glyph.properties.push(Property::new("SOURCE", "glyph record"));
    let id = font.add_glyph(glyph).unwrap();
    font.metrics.default_glyph = Some(id);
    FontSet {
        fonts: vec![font.clone(), font],
        properties: vec![Property::new("CONTAINER", "source container")].into(),
        source_format: Some("source-container".into()),
    }
}
fn complete() -> NameOverrides {
    NameOverrides {
        family: Some(" New 家族 ".into()),
        style: Some("Bold".into()),
        full_name: Some("New Family Bold".into()),
        postscript_name: Some("NewFamily-Bold".into()),
        version: Some("Version 2.34".into()),
        copyright: Some("Replacement copyright".into()),
        license: Some("Replacement license".into()),
        license_url: Some("https://example.org/replacement".into()),
        foundry: Some("Replacement foundry".into()),
    }
}

#[test]
fn all_fields_apply_to_every_strike_without_changing_source_or_glyph_storage() {
    let mut fonts = sample();
    let before = fonts.clone();
    let pointers: Vec<_> = fonts.fonts.iter().map(|font| {
        let (_, glyph) = font.glyphs().next().unwrap();
        (glyph.bitmap.as_bytes().as_ptr(), glyph.codepoints.as_ptr(), font.names.xlfd.as_ref().unwrap().as_ptr())
    }).collect();
    let input = complete();
    assert!(!input.is_empty());
    let report = apply_names(&mut fonts, &input, &ResourceLimits::default()).unwrap();
    assert_eq!(report.entries.len(), 18);
    let codes = ["family", "style", "full_name", "postscript_name", "version", "copyright", "license", "license_url", "foundry"];
    for (index, font) in fonts.fonts.iter().enumerate() {
        assert_eq!(font.names.family, input.family);
        assert_eq!(font.names.style, input.style);
        assert_eq!(font.names.full, input.full_name);
        assert_eq!(font.names.postscript, input.postscript_name);
        assert_eq!(font.names.version, input.version);
        assert_eq!(font.names.copyright, input.copyright);
        assert_eq!(font.names.license, input.license);
        assert_eq!(font.names.license_url, input.license_url);
        assert_eq!(font.names.foundry, input.foundry);
        let (_, glyph) = font.glyphs().next().unwrap();
        assert_eq!(pointers[index], (glyph.bitmap.as_bytes().as_ptr(), glyph.codepoints.as_ptr(), font.names.xlfd.as_ref().unwrap().as_ptr()));
        let mut expected = before.fonts[index].clone();
        expected.names.family = input.family.clone();
        expected.names.style = input.style.clone();
        expected.names.full = input.full_name.clone();
        expected.names.postscript = input.postscript_name.clone();
        expected.names.version = input.version.clone();
        expected.names.copyright = input.copyright.clone();
        expected.names.license = input.license.clone();
        expected.names.license_url = input.license_url.clone();
        expected.names.foundry = input.foundry.clone();
        assert_eq!(font, &expected);
        for (entry, field) in report.entries[index * 9..index * 9 + 9].iter().zip(codes) {
            assert_eq!(entry.code, format!("metadata.override.{field}"));
            assert_eq!(entry.location.font_index, Some(index));
            assert_eq!(entry.level, Level::Info);
            assert_eq!(entry.impact, Impact::None);
        }
    }
    assert_eq!(fonts.properties, before.properties);
    assert_eq!(fonts.source_format, before.source_format);
    assert!(!report.has_loss());
    let mut again = before;
    assert_eq!(apply_names(&mut again, &input, &ResourceLimits::default()).unwrap(), report);
    assert_eq!(again, fonts);
    assert!(apply_names(&mut fonts, &input, &ResourceLimits::default()).unwrap().entries.is_empty());
}

#[test]
fn only_actual_identity_changes_clear_unsupplied_derived_names() {
    for change_style in [false, true] {
        let mut fonts = sample();
        let input = if change_style {
            NameOverrides { style: Some("Bold".into()), ..Default::default() }
        } else {
            NameOverrides { family: Some("New".into()), ..Default::default() }
        };
        let report = apply_names(&mut fonts, &input, &ResourceLimits::default()).unwrap();
        assert_eq!(report.entries.len(), 6);
        for font in &fonts.fonts {
            assert_eq!(font.names.full, None);
            assert_eq!(font.names.postscript, None);
            assert_eq!(font.names.copyright.as_deref(), Some("Original copyright"));
        }
        assert_eq!(report.entries[1].code, "metadata.clear_derived.full_name");
        assert_eq!(report.entries[2].code, "metadata.clear_derived.postscript_name");
    }
    let mut fonts = sample();
    fonts.fonts[1].names.family = Some("Other".into());
    let report = apply_names(&mut fonts, &NameOverrides {
        family: Some("Original".into()), ..Default::default()
    }, &ResourceLimits::default()).unwrap();
    assert_eq!(report.entries.len(), 3);
    assert!(report.entries.iter().all(|e| e.location.font_index == Some(1)));
    assert_eq!(fonts.fonts[0].names.full.as_deref(), Some("Original Regular"));
    assert_eq!(fonts.fonts[1].names.full, None);
}

#[test]
fn explicit_derived_names_survive_and_unchanged_or_absent_edits_are_noops() {
    let mut fonts = sample();
    let before = fonts.clone();
    assert!(NameOverrides::default().is_empty());
    let limits = ResourceLimits { max_total_bytes: fonts.retained_usage(usize::MAX).unwrap().bytes, ..Default::default() };
    assert!(apply_names(&mut fonts, &NameOverrides::default(), &limits).unwrap().entries.is_empty());
    assert!(apply_names(&mut fonts, &NameOverrides {
        family: Some("Original".into()), style: Some("Regular".into()), ..Default::default()
    }, &limits).unwrap().entries.is_empty());
    assert_eq!(fonts, before);
    let report = apply_names(&mut fonts, &NameOverrides {
        family: Some("New".into()),
        full_name: Some("Original Regular".into()),
        postscript_name: Some("NewExplicit".into()),
        ..Default::default()
    }, &ResourceLimits::default()).unwrap();
    assert_eq!(report.entries.len(), 4);
    assert_eq!(fonts.fonts[0].names.full.as_deref(), Some("Original Regular"));
    assert_eq!(fonts.fonts[0].names.postscript.as_deref(), Some("NewExplicit"));
}

#[test]
fn empty_notices_clear_normalized_text_with_audit_but_preserve_source_records() {
    let mut fonts = sample();
    let before = fonts.clone();
    let report = apply_names(&mut fonts, &NameOverrides {
        copyright: Some(String::new()), license: Some(String::new()),
        license_url: Some(String::new()), foundry: Some(String::new()),
        ..Default::default()
    }, &ResourceLimits::default()).unwrap();
    for (font, source) in fonts.fonts.iter().zip(&before.fonts) {
        assert_eq!(font.names.copyright.as_deref(), Some(""));
        assert_eq!(font.names.license.as_deref(), Some(""));
        assert_eq!(font.names.license_url.as_deref(), Some(""));
        assert_eq!(font.names.foundry.as_deref(), Some(""));
        assert_eq!(font.properties, source.properties);
        assert_eq!(font.headers, source.headers);
        assert_eq!(font.names.full, source.names.full);
    }
    for entry in report.entries.iter().filter(|e| !e.code.ends_with("foundry")) {
        assert!(entry.message.contains("Explicitly cleared normalized notice text"));
        assert!(entry.message.contains("does not establish permission"));
    }
}

#[test]
fn empty_identity_and_oversized_records_fail_atomically() {
    let setters: [fn(&mut NameOverrides, String); 5] = [
        |o, s| o.family = Some(s), |o, s| o.style = Some(s),
        |o, s| o.full_name = Some(s), |o, s| o.postscript_name = Some(s),
        |o, s| o.version = Some(s),
    ];
    for set in setters {
        for value in ["", " \t\n", "\u{2003}"] {
            let mut fonts = sample();
            let before = fonts.clone();
            let mut input = complete();
            set(&mut input, value.into());
            assert!(matches!(apply_names(&mut fonts, &input, &ResourceLimits::default()), Err(Error::InvalidInput(_))));
            assert_eq!(fonts, before);
        }
    }
    let mut fonts = sample();
    let before = fonts.clone();
    let input = NameOverrides { family: Some("字母".into()), ..Default::default() };
    let mut limits = ResourceLimits { max_table_bytes: 5, ..Default::default() };
    assert!(matches!(apply_names(&mut fonts, &input, &limits), Err(Error::ResourceLimit { resource: "metadata name bytes", limit: 5 })));
    assert_eq!(fonts, before);
    limits.max_table_bytes = 6;
    apply_names(&mut fonts, &input, &limits).unwrap();
    assert_eq!(fonts.fonts[0].names.family.as_deref(), Some("字母"));
}

#[test]
fn aggregate_budget_includes_all_staged_edits_and_complete_report_before_mutation() {
    let source = sample();
    let input = complete();
    let retained = source.clone().retained_usage(usize::MAX).unwrap().bytes;
    let mut low = retained;
    let mut high = retained + 32_768;
    // Discover the complete public-operation boundary without assuming a private
    // StagedEdit ABI or allocator capacity. Every failed probe must be atomic.
    while low < high {
        let candidate = low + (high - low) / 2;
        let mut fonts = source.clone();
        let limits = ResourceLimits { max_total_bytes: candidate, ..Default::default() };
        if apply_names(&mut fonts, &input, &limits).is_ok() {
            high = candidate;
        } else {
            assert_eq!(fonts, source);
            low = candidate + 1;
        }
    }
    let mut fonts = source.clone();
    let exact = ResourceLimits { max_total_bytes: low, ..Default::default() };
    let report = apply_names(&mut fonts, &input, &exact).unwrap();
    let report_bytes = report.entries.capacity() * std::mem::size_of::<fontpixel::ReportEntry>()
        + report.entries.iter().map(|e| e.code.capacity() + e.message.capacity()).sum::<usize>();
    assert!(low >= retained + report_bytes);
    let mut fonts = source.clone();
    let short = ResourceLimits { max_total_bytes: low - 1, ..Default::default() };
    assert!(matches!(apply_names(&mut fonts, &input, &short), Err(Error::ResourceLimit { .. })));
    assert_eq!(fonts, source);
    let mut one = source.clone();
    one.fonts.truncate(1);
    apply_names(&mut one, &input, &short).unwrap();
}

#[test]
fn source_spare_capacity_and_model_limits_are_not_discounted() {
    let mut fonts = sample();
    fonts.fonts[1].names.xlfd.as_mut().unwrap().reserve(16_384);
    let before = fonts.clone();
    let retained = fonts.retained_usage(usize::MAX).unwrap().bytes;
    let limits = ResourceLimits { max_total_bytes: retained - 1, ..Default::default() };
    assert!(matches!(apply_names(&mut fonts, &NameOverrides::default(), &limits), Err(Error::ResourceLimit { resource: "retained model bytes", .. })));
    assert_eq!(fonts, before);
    for limits in [
        ResourceLimits { max_glyphs: 1, ..Default::default() },
        ResourceLimits { max_pixels_per_glyph: 3, ..Default::default() },
    ] {
        assert!(matches!(apply_names(&mut fonts, &complete(), &limits), Err(Error::ResourceLimit { .. })));
        assert_eq!(fonts, before);
    }
    let id = fonts.fonts[1].glyphs().next().unwrap().0;
    fonts.fonts[1].remove_glyph(id).unwrap();
    fonts.fonts[1].metrics.default_glyph = Some(id);
    let invalid = fonts.clone();
    assert!(matches!(apply_names(&mut fonts, &complete(), &ResourceLimits::default()), Err(Error::InvalidInput(_))));
    assert_eq!(fonts, invalid);
}

#[test]
fn bitmap_larger_than_remaining_budget_is_not_cloned() {
    let bitmap: Bitmap = Bitmap::new(2048, 1024).unwrap();
    let mut font = Font::default();
    let id = font.add_glyph(Glyph::new(bitmap)).unwrap();
    let mut fonts = FontSet::from(font);
    let pointer = fonts.fonts[0].glyph(id).unwrap().bitmap.as_bytes().as_ptr();
    let retained = fonts.retained_usage(usize::MAX).unwrap().bytes;
    let limits = ResourceLimits { max_total_bytes: retained + 4096, ..Default::default() };
    apply_names(&mut fonts, &NameOverrides { family: Some("Large".into()), ..Default::default() }, &limits).unwrap();
    assert_eq!(fonts.fonts[0].glyph(id).unwrap().bitmap.as_bytes().as_ptr(), pointer);
}

#[test]
fn empty_sets_validate_options_and_do_not_invent_audit_entries() {
    let mut fonts = FontSet::default();
    assert!(apply_names(&mut fonts, &complete(), &ResourceLimits::default()).unwrap().entries.is_empty());
    assert!(apply_names(&mut fonts, &NameOverrides { version: Some(String::new()), ..Default::default() }, &ResourceLimits::default()).is_err());
    assert_eq!(fonts, FontSet::default());
}

#[cfg(feature = "bdf")]
#[test]
fn bdf_preserves_raw_property_precedence_and_authoritative_xlfd() {
    use fontpixel::formats::bdf::{self, BdfWriteOptions};
    const SOURCE: &str = "STARTFONT 2.1\nFONT source-font\nSIZE 8 75 75\nFONTBOUNDINGBOX 1 1 0 0\nSTARTPROPERTIES 3\nFAMILY_NAME \"Original\"\nCOPYRIGHT \"Original copyright\"\nCOMMENT \"Original notice\"\nENDPROPERTIES\nCHARS 0\nENDFONT\n";
    let (mut font, _) = bdf::read(SOURCE.as_bytes(), &ResourceLimits::default()).unwrap();
    font.properties.set("COPYRIGHT", "Independent property edit");
    let mut fonts = FontSet::from(font);
    let report = apply_names(&mut fonts, &NameOverrides { copyright: Some("Original copyright".into()), ..Default::default() }, &ResourceLimits::default()).unwrap();
    assert!(report.entries.is_empty());
    let (bytes, _) = bdf::write(&fonts.fonts[0], &BdfWriteOptions::default()).unwrap();
    assert!(String::from_utf8(bytes).unwrap().contains("COPYRIGHT \"Independent property edit\""));
    apply_names(&mut fonts, &NameOverrides { family: Some("New family".into()), copyright: Some(String::new()), ..Default::default() }, &ResourceLimits::default()).unwrap();
    let (bytes, _) = bdf::write(&fonts.fonts[0], &BdfWriteOptions::default()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("FONT source-font\n"));
    assert!(text.contains("FAMILY_NAME \"New family\""));
    assert!(text.contains("COPYRIGHT \"\""));
    assert!(text.contains("Original notice"));
    assert_eq!(fonts.fonts[0].properties.get("COPYRIGHT"), Some(&"Independent property edit".into()));
}
