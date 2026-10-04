//! Independent Pebble layout, roundtrip, corruption and resource regressions.
#![cfg(feature = "pbf")]
use fontpixel::formats::pbf::{
    self, Compression, OffsetWidth, PbfReadOptions, PbfWriteOptions, Version,
};
use fontpixel::model::{Codepoint, GlyphBitmap, Property, PropertyValue, Vector};
use fontpixel::{Bitmap, Font, Glyph, ResourceLimits};
fn limits() -> ResourceLimits {
    ResourceLimits::default()
}
fn fixture(version: u8, rle: bool, cp4: bool, off4: bool, aliases: bool) -> Vec<u8> {
    let cps = if aliases { vec![65u32, 66] } else { vec![65] };
    let mut b = vec![version, 4];
    b.extend((cps.len() as u16).to_le_bytes());
    b.extend(65u16.to_le_bytes());
    let cp = if cp4 { 4 } else { 2 };
    let off = if off4 { 4 } else { 2 };
    if version > 1 {
        b.extend([1, cp]);
    }
    if version == 3 {
        b.extend([10, u8::from(!off4) | (u8::from(rle) << 1)]);
    }
    if version > 1 {
        b.extend([0, cps.len() as u8, 0, 0]);
    }
    for code in cps {
        b.extend(&code.to_le_bytes()[..cp as usize]);
        let offset = if version == 1 { 1u32 } else { 4 };
        b.extend(&offset.to_le_bytes()[..off as usize]);
    }
    b.extend([0; 4]);
    if version == 1 {
        b.extend([2, 2, 255, 3, 0, 0, 0, 3]);
    } else {
        b.extend([2, if rle { 1 } else { 2 }, 255, 3, 3]);
    }
    b.push(if rle { 11 } else { 15 });
    b
}
fn load(b: &[u8]) -> Font {
    pbf::read(b, &PbfReadOptions::default(), &limits())
        .unwrap()
        .0
}
#[test]
fn independent_version_layouts_and_aliases() {
    for (version, rle, cp4, off4) in [
        (1, false, false, false),
        (2, false, false, true),
        (2, false, true, true),
        (3, false, false, false),
        (3, true, false, true),
        (3, true, true, false),
    ] {
        let b = fixture(version, rle, cp4, off4, true);
        let f = load(&b);
        assert_eq!(f.len(), 1);
        let (id, g) = f.glyphs().next().unwrap();
        assert_eq!(
            g.codepoints,
            [Codepoint::Unicode(65), Codepoint::Unicode(66)]
        );
        assert_eq!(
            g.metrics.bounds,
            fontpixel::bitmap::Bounds::new(2, 2, -1, -1)
        );
        assert_eq!(g.metrics.advance, Some(Vector::new(3, 0)));
        assert_eq!(
            g.bitmap,
            GlyphBitmap::Mono("##\n##".parse::<Bitmap>().unwrap())
        );
        assert_eq!(f.metrics.ascent, Some(4));
        assert_eq!(f.metrics.descent, Some(1));
        assert_eq!(f.metrics.default_glyph, Some(id));
    }
}
#[test]
fn native_roundtrips_all_versions_and_offset_modes() {
    let f = load(&fixture(3, false, false, false, true));
    for version in [Version::One, Version::Two, Version::Three] {
        for compression in [Compression::Raw, Compression::Rle4] {
            if version != Version::Three && compression == Compression::Rle4 {
                continue;
            }
            for offset_width in [OffsetWidth::Auto, OffsetWidth::U16, OffsetWidth::U32] {
                if (version == Version::One && offset_width == OffsetWidth::U32)
                    || (version == Version::Two && offset_width == OffsetWidth::U16)
                {
                    continue;
                }
                let o = PbfWriteOptions {
                    version,
                    compression,
                    offset_width,
                    ..Default::default()
                };
                let (bytes, r) = pbf::write(&f, &o).unwrap();
                assert!(!r.has_loss(), "{r}");
                assert_eq!(load(&bytes), f);
                assert_eq!(pbf::write(&f, &o).unwrap().0, bytes);
            }
        }
    }
}
#[test]
fn independent_glyphs_are_not_deduplicated_and_empty_advance_survives() {
    let mut f = load(&fixture(3, false, false, false, false));
    let mut g = f.glyphs().next().unwrap().1.clone();
    g.codepoints = vec![Codepoint::Unicode(66)];
    f.add_glyph(g).unwrap();
    let mut space = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::new(0, 0).unwrap());
    space.codepoints = vec![Codepoint::Unicode(32)];
    space.metrics.advance = Some(Vector::new(2, 0));
    space.metrics.bounds.y = 4;
    f.add_glyph(space).unwrap();
    let (b, r) = pbf::write(&f, &PbfWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    let back = load(&b);
    assert_eq!(back, f);
    assert_eq!(back.len(), 3);
}
#[test]
fn rle_width_and_runs_are_checked() {
    let mut b = fixture(3, true, false, false, false);
    let last = b.len() - 1;
    b[last] = 10;
    let (partial, report) = pbf::read(&b, &PbfReadOptions::default(), &limits()).unwrap();
    assert_eq!(
        partial.glyphs().next().unwrap().1.bitmap,
        GlyphBitmap::Mono("##".parse::<Bitmap>().unwrap())
    );
    assert!(
        report
            .entries
            .iter()
            .any(|entry| entry.code == "pbf.read.rle_tail"
                && entry.impact == fontpixel::report::Impact::Loss)
    ); // three pixels / width two
    b[last] = 11;
    b[last - 5] = 0;
    assert!(pbf::read(&b, &PbfReadOptions::default(), &limits()).is_err());
    let mut f = load(&fixture(3, false, false, false, false));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.bitmap = GlyphBitmap::Mono(Bitmap::new(0, 2)?);
        g.metrics.bounds.width = 0;
        g.metrics.bounds.height = 2;
        Ok(())
    })
    .unwrap();
    assert!(
        pbf::write(
            &f,
            &PbfWriteOptions {
                compression: Compression::Rle4,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn table_corruption_and_truncation_are_rejected() {
    let good = fixture(3, false, false, false, true);
    for end in 0..good.len() {
        assert!(
            pbf::read(&good[..end], &PbfReadOptions::default(), &limits()).is_err(),
            "end={end}"
        );
    }
    for (at, value) in [
        (6, 0),
        (7, 3),
        (8, 9),
        (9, 128),
        (10, 1),
        (11, 1),
        (12, 1),
        (15, 0xd8),
        (18, 65),
        (20, 5),
    ] {
        let mut b = good.clone();
        b[at] = value;
        assert!(
            pbf::read(&b, &PbfReadOptions::default(), &limits()).is_err(),
            "at={at}"
        );
    }
}
#[test]
fn metadata_unknown_header_and_v1_padding_preserve_or_report_loss() {
    let mut b = fixture(3, false, false, false, false);
    b[8] = 12;
    b.splice(10..10, [123, 45]);
    let f = load(&b);
    let (back, r) = pbf::write(&f, &PbfWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(load(&back), f);
    assert!(
        pbf::write(
            &f,
            &PbfWriteOptions {
                version: Version::Two,
                ..Default::default()
            }
        )
        .unwrap()
        .1
        .has_loss()
    );
    let mut b = fixture(1, false, false, false, false);
    b[18..21].copy_from_slice(&[1, 2, 3]);
    let f = load(&b);
    let (back, r) = pbf::write(
        &f,
        &PbfWriteOptions {
            version: Version::One,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(load(&back), f);
    assert!(
        pbf::write(&f, &PbfWriteOptions::default())
            .unwrap()
            .1
            .has_loss()
    );
}
#[test]
fn trimming_is_explicit_and_blank_bounds_match_catalogue() {
    let mut b = fixture(3, false, false, false, false);
    *b.last_mut().unwrap() = 1;
    let native = load(&b);
    assert_eq!(native.glyphs().next().unwrap().1.metrics.bounds.width, 2);
    let (f, r) = pbf::read(&b, &PbfReadOptions { trim: true }, &limits()).unwrap();
    assert!(r.has_loss());
    let g = f.glyphs().next().unwrap().1;
    assert_eq!(
        g.metrics.bounds,
        fontpixel::bitmap::Bounds::new(1, 1, -1, 0)
    );
    assert_eq!(f.metrics.descent, Some(0));
    *b.last_mut().unwrap() = 0;
    let (f, _) = pbf::read(&b, &PbfReadOptions { trim: true }, &limits()).unwrap();
    assert_eq!(
        f.glyphs().next().unwrap().1.metrics.bounds,
        fontpixel::bitmap::Bounds::default()
    );
}
#[test]
fn malformed_wire_and_model_budgets_fail_before_growth() {
    let b = fixture(3, false, false, false, true);
    for l in [
        ResourceLimits {
            max_total_bytes: 100,
            ..limits()
        },
        ResourceLimits {
            max_table_bytes: 10,
            ..limits()
        },
        ResourceLimits {
            max_glyphs: 1,
            ..limits()
        },
        ResourceLimits {
            max_pixels_per_glyph: 1,
            ..limits()
        },
    ] {
        assert!(pbf::read(&b, &PbfReadOptions::default(), &l).is_err());
    }
    let f = load(&b);
    assert!(
        pbf::write(
            &f,
            &PbfWriteOptions {
                limits: ResourceLimits {
                    max_total_bytes: 100,
                    ..limits()
                },
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        pbf::write(
            &f,
            &PbfWriteOptions {
                max_output_bytes: 20,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn bucket_count_is_unsigned_and_bmp_extended_mappings_do_not_alias() {
    let mut f = load(&fixture(3, false, false, false, false));
    let template = f.glyphs().next().unwrap().1.clone();
    for cp in 1000..1200 {
        let mut g = template.clone();
        g.codepoints = vec![Codepoint::Unicode(cp)];
        f.add_glyph(g).unwrap();
    }
    let (bytes, _) = pbf::write(
        &f,
        &PbfWriteOptions {
            hash_buckets: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bytes[11], 201);
    assert_eq!(load(&bytes).len(), 201);
    let mut g = template;
    g.codepoints = vec![Codepoint::Unicode(0x1f600)];
    f.add_glyph(g).unwrap();
    let (bytes, _) = pbf::write(&f, &PbfWriteOptions::default()).unwrap();
    assert_eq!(bytes[7], 4);
    assert!(
        load(&bytes)
            .glyph_for_codepoint(Codepoint::Unicode(0x1f600))
            .is_some()
    );
    assert!(
        pbf::write(
            &f,
            &PbfWriteOptions {
                version: Version::One,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn gray_and_source_metadata_loss_is_explicit() {
    let mut f = load(&fixture(3, false, false, false, false));
    f.names.family = Some("test".into());
    f.properties.push(Property::new(
        "X",
        PropertyValue::Text("preserve elsewhere".into()),
    ));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.name = Some("A".into());
        g.unicode_sequences.push(vec![65, 66]);
        g.metrics.vertical_advance = Some(Vector::new(0, -4));
        let GlyphBitmap::Mono(b) = &g.bitmap else {
            unreachable!()
        };
        g.bitmap = GlyphBitmap::Gray8(b.to_gray8()?);
        Ok(())
    })
    .unwrap();
    assert!(pbf::write(&f, &PbfWriteOptions::default()).is_err());
    let (_, r) = pbf::write(
        &f,
        &PbfWriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    for code in [
        "pbf.write.names",
        "pbf.write.property",
        "pbf.write.glyph_name",
        "pbf.write.sequence",
        "pbf.write.glyph_metric",
        "pbf.write.gray_threshold",
    ] {
        assert!(r.entries.iter().any(|e| e.code == code), "{code}: {r}");
    }
}

#[test]
fn continuous_non_byte_aligned_rows_and_rle_extremes() {
    for width in [1u32, 7, 8, 9, 17] {
        for height in [0u32, 1, 3, 8] {
            for seed in [0u32, 1, 37, u32::MAX] {
                let mut f = Font::default();
                f.metrics.ascent = Some(8);
                let mut bitmap: Bitmap = Bitmap::new(width, height).unwrap();
                let mut state = seed;
                for y in 0..height {
                    for x in 0..width {
                        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                        bitmap
                            .set_pixel(
                                x,
                                y,
                                if seed == 0 {
                                    0
                                } else if seed == u32::MAX {
                                    1
                                } else {
                                    (state >> 31) as u8
                                },
                            )
                            .unwrap();
                    }
                }
                let mut g = Glyph::new(bitmap);
                g.codepoints = vec![Codepoint::Unicode(65)];
                g.metrics.advance = Some(Vector::new(-3, 0));
                g.metrics.bounds.y = 8 - i64::from(height);
                f.add_glyph(g).unwrap();
                let (raw, _) = pbf::write(&f, &PbfWriteOptions::default()).unwrap();
                let canonical = load(&raw);
                let (rle, r) = pbf::write(
                    &canonical,
                    &PbfWriteOptions {
                        compression: Compression::Rle4,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(!r.has_loss(), "{r}");
                assert_eq!(load(&rle), canonical, "{width}x{height} seed{seed}");
            }
        }
    }
}

#[test]
fn colliding_buckets_and_duplicate_aliases_never_drop_silently() {
    let mut f = load(&fixture(3, false, false, false, true));
    let (mut bytes, _) = pbf::write(
        &f,
        &PbfWriteOptions {
            hash_buckets: 2,
            ..Default::default()
        },
    )
    .unwrap();
    bytes[16..18].copy_from_slice(&0u16.to_le_bytes());
    assert!(pbf::read(&bytes, &PbfReadOptions::default(), &limits()).is_err());
    let mut g = f.glyphs().next().unwrap().1.clone();
    g.codepoints = vec![Codepoint::Unicode(65)];
    f.add_glyph(g).unwrap();
    assert!(pbf::write(&f, &PbfWriteOptions::default()).is_err());
}

#[test]
fn empty_special_properties_and_raw_provenance_are_reported() {
    for (version, name, bytes, is_font) in [
        (Version::Three, "pbf.header_extra", vec![], true),
        (Version::One, "pbf.v1_padding", vec![0, 0, 0], false),
        (Version::Three, "pbf.header_extra", vec![9], true),
        (Version::One, "pbf.v1_padding", vec![1, 0, 0], false),
    ] {
        let mut f = load(&fixture(3, false, false, false, false));
        let mut p = Property::new(name, PropertyValue::Bytes(bytes.clone()));
        if bytes.iter().any(|&v| v != 0) {
            p.raw = Some(vec![77]);
        }
        if is_font {
            f.headers.push(p);
        } else {
            let id = f.glyphs().next().unwrap().0;
            f.edit_glyph(id, |g| {
                g.properties.push(p);
                Ok(())
            })
            .unwrap();
        }
        let (_, r) = pbf::write(
            &f,
            &PbfWriteOptions {
                version,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.has_loss(), "{name}");
    }
}
#[test]
fn already_empty_rectangle_trim_resets_offsets() {
    let mut b = fixture(3, false, false, false, false);
    let p = b.len() - 6;
    b[p] = 0;
    b[p + 1] = 0;
    b.truncate(b.len() - 1);
    let native = load(&b);
    assert_ne!(
        native.glyphs().next().unwrap().1.metrics.bounds,
        fontpixel::bitmap::Bounds::default()
    );
    let (f, r) = pbf::read(&b, &PbfReadOptions { trim: true }, &limits()).unwrap();
    assert!(r.has_loss());
    assert_eq!(
        f.glyphs().next().unwrap().1.metrics.bounds,
        fontpixel::bitmap::Bounds::default()
    );
}

#[test]
fn inherited_horizontal_advance_is_materialized() {
    let mut f = load(&fixture(3, false, false, false, false));
    f.metrics.glyph_defaults.advance = Some(Vector::new(9, 0));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.metrics.advance = None;
        Ok(())
    })
    .unwrap();
    let (bytes, report) = pbf::write(&f, &PbfWriteOptions::default()).unwrap();
    assert!(report.has_loss());
    assert_eq!(
        load(&bytes).glyphs().next().unwrap().1.metrics.advance,
        Some(Vector::new(9, 0))
    );
}

#[test]
fn rle_run_count_limit_is_exact() {
    for (w, h, succeeds) in [(17, 15, true), (16, 16, false)] {
        let mut f = Font::default();
        f.metrics.ascent = Some(16);
        let mut bitmap: Bitmap = Bitmap::new(w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                bitmap.set_pixel(x, y, ((y * w + x) % 2) as u8).unwrap();
            }
        }
        let mut g = Glyph::new(bitmap);
        g.codepoints = vec![Codepoint::Unicode(65)];
        f.add_glyph(g).unwrap();
        let result = pbf::write(
            &f,
            &PbfWriteOptions {
                compression: Compression::Rle4,
                ..Default::default()
            },
        );
        assert_eq!(result.is_ok(), succeeds);
        if let Ok((bytes, _)) = result {
            assert_eq!(
                load(&bytes).glyphs().next().unwrap().1.bitmap,
                f.glyphs().next().unwrap().1.bitmap
            );
        }
    }
}

#[test]
fn expanding_rle_output_respects_the_same_input_limit() {
    let mut f = Font::default();
    f.metrics.ascent = Some(16);
    let mut g = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::new(16, 16).unwrap());
    g.codepoints = vec![Codepoint::Unicode(65)];
    f.add_glyph(g).unwrap();
    let (compressed, _) = pbf::write(
        &f,
        &PbfWriteOptions {
            compression: Compression::Rle4,
            hash_buckets: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let limits = ResourceLimits {
        max_input_bytes: compressed.len(),
        ..limits()
    };
    let canonical = pbf::read(&compressed, &PbfReadOptions::default(), &limits)
        .unwrap()
        .0;
    assert!(
        pbf::write(
            &canonical,
            &PbfWriteOptions {
                hash_buckets: 1,
                limits: limits.clone(),
                ..Default::default()
            }
        )
        .is_err()
    );
    let (bytes, _) = pbf::write(
        &canonical,
        &PbfWriteOptions {
            hash_buckets: 1,
            compression: Compression::Rle4,
            limits: limits.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        pbf::read(&bytes, &PbfReadOptions::default(), &limits)
            .unwrap()
            .0,
        canonical
    );
}
#[test]
fn alias_dense_output_is_preflighted_against_reader_model_budget() {
    let mut f = load(&fixture(3, false, false, false, false));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints = (1000..1200).map(Codepoint::Unicode).collect();
        Ok(())
    })
    .unwrap();
    let small = ResourceLimits {
        max_total_bytes: 200_000,
        ..limits()
    };
    assert!(
        pbf::write(
            &f,
            &PbfWriteOptions {
                limits: small,
                ..Default::default()
            }
        )
        .is_err()
    );
    let enough = ResourceLimits {
        max_total_bytes: 2_000_000,
        ..limits()
    };
    let (bytes, _) = pbf::write(
        &f,
        &PbfWriteOptions {
            limits: enough.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(pbf::read(&bytes, &PbfReadOptions::default(), &enough).is_ok());
}
