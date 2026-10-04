//! Legacy subfamily selection must not alter typographic style or exact tables.
#![cfg(feature = "sfnt")]
use fontpixel::{
    Bitmap, Font, Glyph, Variants,
    model::{Codepoint, Vector},
    sfnt::{
        Sfnt,
        name::{NameRecord, NameTable},
    },
    variants::{self, Options},
};
fn sample() -> Variants {
    let mut font = Font::default();
    font.metrics.pixel_size = Some(8);
    font.metrics.ascent = Some(7);
    font.metrics.descent = Some(1);
    let mut glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
    glyph.codepoints = vec![Codepoint::Unicode(65)];
    glyph.metrics.advance = Some(Vector::new(2, 0));
    font.add_glyph(glyph).unwrap();
    Variants::new(font)
}
fn names(bytes: &[u8]) -> NameTable {
    let font = Sfnt::parse(bytes, &Default::default()).unwrap();
    NameTable::decode(font.table(*b"name").unwrap(), &Default::default()).unwrap()
}
fn name(table: &NameTable, id: u16) -> String {
    table
        .records
        .iter()
        .find(|r| r.name_id == id && r.platform_id == 3)
        .unwrap()
        .text()
        .unwrap()
        .unwrap()
}
#[test]
fn default_legacy_style_is_byte_identical_to_explicit_regular() {
    let sample = sample();
    let defaults = Options::default();
    assert!(defaults.legacy_style.is_none());
    let a = variants::encode(&sample, &defaults).unwrap();
    let b = variants::encode(
        &sample,
        &Options {
            legacy_style: Some("Regular".into()),
            ..defaults
        },
    )
    .unwrap();
    assert_eq!(a.bytes, b.bytes);
    assert_eq!(a.report, b.report);
    assert_eq!(name(&names(&a.bytes), 2), "Regular");
    assert_eq!(name(&names(&a.bytes), 17), "Square");
}
#[test]
fn explicit_legacy_style_changes_only_name_id_two() {
    let sample = sample();
    let options = Options {
        style: Some("SquareMedium".into()),
        ..Default::default()
    };
    let original = variants::encode(&sample, &options).unwrap();
    let replaced = variants::encode(
        &sample,
        &Options {
            legacy_style: Some("Medium".into()),
            ..options
        },
    )
    .unwrap();
    let mut a = names(&original.bytes);
    let mut b = names(&replaced.bytes);
    assert_eq!(name(&b, 2), "Medium");
    assert_eq!(name(&b, 17), "SquareMedium");
    a.records.retain(|r| r.name_id != 2);
    b.records.retain(|r| r.name_id != 2);
    assert_eq!(a, b);
    for tag in [
        *b"glyf", *b"loca", *b"hmtx", *b"hhea", *b"cmap", *b"OS/2", *b"post", *b"maxp",
    ] {
        let a = Sfnt::parse(&original.bytes, &Default::default()).unwrap();
        let b = Sfnt::parse(&replaced.bytes, &Default::default()).unwrap();
        assert_eq!(a.table(tag), b.table(tag));
    }
    assert_eq!(original.report, replaced.report);
}
#[test]
fn exact_name_table_keeps_precedence_over_legacy_style() {
    let table = NameTable {
        records: vec![
            NameRecord::unicode(2, 0x409, "Exact"),
            NameRecord::unicode(17, 0x409, "Exact Typographic"),
        ],
        language_tags: Vec::new(),
    };
    let sample = sample();
    let a = variants::encode(
        &sample,
        &Options {
            names: Some(table.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    for legacy in ["Ignored", "", " \t"] {
        let b = variants::encode(
            &sample,
            &Options {
                names: Some(table.clone()),
                legacy_style: Some(legacy.into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(name(&names(&b.bytes), 2), "Exact");
        assert_eq!(name(&names(&b.bytes), 17), "Exact Typographic");
    }
}
#[test]
fn invalid_or_oversized_legacy_style_fails_before_generation() {
    let sample = sample();
    for value in ["", " \n\t", "\u{2003}"] {
        let error = variants::encode(
            &sample,
            &Options {
                legacy_style: Some(value.into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(error, fontpixel::Error::InvalidInput(_)));
    }
    let mut options = Options {
        legacy_style: Some("Longer".into()),
        ..Default::default()
    };
    options.limits.sfnt.resources.max_table_bytes = 5;
    let error = variants::encode(&sample, &options).unwrap_err();
    assert!(matches!(
        error,
        fontpixel::Error::ResourceLimit {
            resource: "legacy style bytes",
            limit: 5
        }
    ));
}

#[test]
fn explicit_full_and_postscript_names_preserve_exact_text() {
    let sample = sample();
    let original = variants::encode(
        &sample,
        &Options {
            style: Some("Heavy Extended".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let changed = variants::encode(
        &sample,
        &Options {
            style: Some("Heavy Extended".into()),
            full_name: Some(" Catalogue Full Name ".into()),
            postscript_name: Some("Catalogue-HeavyExtended".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = names(&original.bytes);
    let mut b = names(&changed.bytes);
    assert_eq!(name(&b, 4), " Catalogue Full Name ");
    assert_eq!(name(&b, 6), "Catalogue-HeavyExtended");
    assert_eq!(name(&b, 17), "Heavy Extended");
    a.records.retain(|r| ![4, 6].contains(&r.name_id));
    b.records.retain(|r| ![4, 6].contains(&r.name_id));
    assert_eq!(a, b);
    let table = NameTable {
        records: vec![
            NameRecord::unicode(4, 0x409, "Raw Full"),
            NameRecord::unicode(6, 0x409, "RawPS"),
        ],
        language_tags: Vec::new(),
    };
    let exact = variants::encode(
        &sample,
        &Options {
            full_name: Some(String::new()),
            postscript_name: Some(String::new()),
            names: Some(table),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(name(&names(&exact.bytes), 4), "Raw Full");
    assert_eq!(name(&names(&exact.bytes), 6), "RawPS");
}

#[test]
fn empty_and_oversized_full_or_postscript_names_are_rejected() {
    let sample = sample();
    let setters: [fn(&mut Options, String); 2] = [
        |o, value| o.full_name = Some(value),
        |o, value| o.postscript_name = Some(value),
    ];
    for set in setters {
        for value in ["", " \n\t"] {
            let mut options = Options::default();
            set(&mut options, value.into());
            assert!(matches!(
                variants::encode(&sample, &options),
                Err(fontpixel::Error::InvalidInput(_))
            ));
        }
        let mut options = Options::default();
        set(&mut options, "Longer".into());
        options.limits.sfnt.resources.max_table_bytes = 5;
        assert!(matches!(
            variants::encode(&sample, &options),
            Err(fontpixel::Error::ResourceLimit { limit: 5, .. })
        ));
    }
}

#[test]
fn prepared_and_single_paths_honor_all_direct_name_overrides() {
    let sample = sample();
    let options = Options {
        legacy_style: Some("Medium".into()),
        style: Some("Heavy Extended".into()),
        full_name: Some("Catalogue Heavy Extended".into()),
        postscript_name: Some("Catalogue-HeavyExtended".into()),
        ..Default::default()
    };
    let direct = variants::encode(&sample, &options).unwrap();
    let prepared = sample.prepare(&options.limits).unwrap();
    let cached = prepared.encode(&options).unwrap();
    assert_eq!(direct.bytes, cached.bytes);
    assert_eq!(direct.report, cached.report);
    let single = variants::encode_single(&sample.base, &options).unwrap();
    for bytes in [&direct.bytes, &cached.bytes, &single.bytes] {
        let table = names(bytes);
        assert_eq!(name(&table, 2), "Medium");
        assert_eq!(name(&table, 4), "Catalogue Heavy Extended");
        assert_eq!(name(&table, 6), "Catalogue-HeavyExtended");
        assert_eq!(name(&table, 17), "Heavy Extended");
    }
}

#[test]
fn every_export_entry_rejects_empty_direct_identity_options() {
    let sample = sample();
    let setters: [fn(&mut Options); 3] = [
        |o| o.legacy_style = Some(" \t".into()),
        |o| o.full_name = Some(" \t".into()),
        |o| o.postscript_name = Some(" \t".into()),
    ];
    let prepared = sample.prepare(&Default::default()).unwrap();
    for set in setters {
        let mut options = Options::default();
        set(&mut options);
        assert!(matches!(
            variants::encode(&sample, &options),
            Err(fontpixel::Error::InvalidInput(_))
        ));
        assert!(matches!(
            variants::encode_single(&sample.base, &options),
            Err(fontpixel::Error::InvalidInput(_))
        ));
        assert!(matches!(
            prepared.encode(&options),
            Err(fontpixel::Error::InvalidInput(_))
        ));
    }
}

#[cfg(feature = "cff")]
#[test]
fn cff_direct_names_agree_with_the_name_table_without_silent_sanitization() {
    let sample = sample();
    let options = Options {
        flavor: variants::OutlineFlavor::Cff,
        style: Some("Heavy Extended".into()),
        full_name: Some("Catalogue Heavy Extended".into()),
        postscript_name: Some("Catalogue-HeavyExtended".into()),
        ..Default::default()
    };
    let encoded = variants::encode(&sample, &options).unwrap();
    let font = Sfnt::parse(&encoded.bytes, &Default::default()).unwrap();
    font.validate_checksums().unwrap();
    let cff = fontpixel::sfnt::cff::Cff::from_sfnt(&font, &Default::default()).unwrap();
    // The native CFF writer assigns SID 391 to FullName. Inspect that actual
    // CFF string in addition to the independent OpenType name table.
    assert_eq!(
        cff.fontations()
            .string(read_fonts::ps::string::Sid::new(391)),
        Some(b"Catalogue Heavy Extended".as_slice()),
    );
    // CFF1's first INDEX after its header contains exactly one PostScript name.
    // The table was structurally validated above before inspecting offsets.
    let data = cff.data();
    let start = usize::from(data[2]);
    assert_eq!(u16::from_be_bytes([data[start], data[start + 1]]), 1);
    let width = usize::from(data[start + 2]);
    let offset = |bytes: &[u8]| {
        bytes
            .iter()
            .fold(0usize, |value, byte| value * 256 + usize::from(*byte))
    };
    let first = offset(&data[start + 3..start + 3 + width]);
    let last = offset(&data[start + 3 + width..start + 3 + 2 * width]);
    let storage = start + 3 + 2 * width;
    assert_eq!(
        &data[storage + first - 1..storage + last - 1],
        b"Catalogue-HeavyExtended"
    );
    let table = names(&encoded.bytes);
    assert_eq!(name(&table, 4), "Catalogue Heavy Extended");
    assert_eq!(name(&table, 6), "Catalogue-HeavyExtended");
    let mut invalid = options.clone();
    invalid.postscript_name = Some("Contains Space".into());
    assert!(matches!(
        variants::encode(&sample, &invalid),
        Err(fontpixel::Error::InvalidInput(_))
    ));
    invalid = options;
    invalid.full_name = Some("Explicit 名称".into());
    assert!(matches!(
        variants::encode(&sample, &invalid),
        Err(fontpixel::Error::InvalidInput(_))
    ));
}

#[cfg(feature = "cff")]
#[test]
fn cff_exact_name_table_ignores_all_new_direct_name_options() {
    let sample = sample();
    let table = NameTable {
        records: vec![
            NameRecord::unicode(2, 0x409, "Raw Legacy"),
            NameRecord::unicode(4, 0x409, "Raw Full"),
            NameRecord::unicode(6, 0x409, "RawPS"),
        ],
        language_tags: Vec::new(),
    };
    let baseline = Options {
        flavor: variants::OutlineFlavor::Cff,
        names: Some(table),
        ..Default::default()
    };
    let expected = variants::encode(&sample, &baseline).unwrap();
    let actual = variants::encode(
        &sample,
        &Options {
            legacy_style: Some(String::new()),
            full_name: Some("Ignored 名称".into()),
            postscript_name: Some("Ignored Invalid Name".into()),
            ..baseline
        },
    )
    .unwrap();
    assert_eq!(expected.bytes, actual.bytes);
    assert_eq!(expected.report, actual.report);
}
