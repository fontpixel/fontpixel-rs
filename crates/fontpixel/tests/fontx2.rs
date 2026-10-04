//! FONTX2 fixtures are built without invoking the production writer.
#![cfg(feature = "fontx2")]
use fontpixel::formats::fontx2::{self, CodeMode, Fontx2ReadOptions, Fontx2WriteOptions};
use fontpixel::model::Codepoint;
use fontpixel::{Font, ResourceLimits};
fn fixture(double: bool, w: u8, h: u8) -> Vec<u8> {
    let mut b = b"FONTX2WireTest".to_vec();
    b.extend([w, h, u8::from(double)]);
    let cps = if double {
        b.push(2);
        b.extend([0x40, 0x81, 0x41, 0x81, 0xa0, 0x82, 0xa0, 0x82]);
        3
    } else {
        256
    };
    for i in 0..cps {
        for y in 0..h {
            for x in 0..usize::from(w).div_ceil(8) {
                let mut v = ((i + usize::from(y) + x) * 47) as u8;
                if x + 1 == usize::from(w).div_ceil(8) && !w.is_multiple_of(8) {
                    v &= 255 << (8 - w % 8);
                }
                b.push(v);
            }
        }
    }
    b
}
fn load(b: &[u8]) -> Font {
    fontx2::read(b, &Default::default(), &Default::default())
        .unwrap()
        .0
}
#[test]
fn independent_layouts_and_native_exact_roundtrip() {
    for double in [false, true] {
        for w in [1, 7, 8, 9, 16, 17] {
            let f = load(&fixture(double, w, 3));
            assert_eq!(f.len(), if double { 3 } else { 256 });
            assert_eq!(
                f.glyphs().next().unwrap().1.codepoints,
                [Codepoint::Legacy(if double { 0x8140 } else { 0 })]
            );
            let (b, r) = fontx2::write(&f, &Default::default()).unwrap();
            assert!(!r.has_loss(), "{r}");
            assert_eq!(load(&b), f);
        }
    }
}
#[test]
fn all_truncations_and_overlapping_blocks_fail() {
    let good = fixture(true, 9, 3);
    for n in 0..good.len() {
        assert!(fontx2::read(&good[..n], &Default::default(), &Default::default()).is_err());
    }
    let mut bad = good;
    bad[22..26].copy_from_slice(&[0x41, 0x81, 0x41, 0x81]);
    assert!(fontx2::read(&bad, &Default::default(), &Default::default()).is_err());
}
#[test]
fn exact_ank_charset_is_explicit() {
    let f = load(&fixture(false, 8, 2));
    assert_eq!(f.charset.canonical_name(), "fontx2ank");
    let f = fontx2::read(
        &fixture(false, 8, 2),
        &Fontx2ReadOptions {
            charset: Some("cp437".into()),
        },
        &Default::default(),
    )
    .unwrap()
    .0;
    let (b, r) = fontx2::write(&f, &Default::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(
        fontx2::read(
            &b,
            &Fontx2ReadOptions {
                charset: Some("cp437".into())
            },
            &Default::default()
        )
        .unwrap()
        .0,
        f
    );
}
#[test]
fn limits_writer_readback() {
    let f = load(&fixture(true, 9, 3));
    for max in [0, 100, 6000, 8192, 65536] {
        let mut o = Fontx2WriteOptions::default();
        o.limits.max_total_bytes = max;
        if let Ok((b, _)) = fontx2::write(&f, &o) {
            assert!(fontx2::read(&b, &Default::default(), &o.limits).is_ok());
        }
    }
    let l = ResourceLimits {
        max_table_bytes: 7,
        ..Default::default()
    };
    assert!(fontx2::read(&fixture(true, 9, 3), &Default::default(), &l).is_err());
}
#[test]
fn sparse_single_byte_inserts_blanks_with_report() {
    let mut f = load(&fixture(false, 8, 2));
    let ids = f.glyphs().map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids.into_iter().skip(1) {
        f.remove_glyph(id);
    }
    let (b, r) = fontx2::write(
        &f,
        &Fontx2WriteOptions {
            mode: CodeMode::SingleByte,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(load(&b).len(), 256);
    assert!(r.entries.iter().any(|e| e.code.ends_with("blank_slots")));
}
#[test]
fn mutation_smoke_no_panic() {
    let good = fixture(true, 9, 3);
    for i in 0..good.len() {
        for value in [0, 1, 127, 255] {
            let mut b = good.clone();
            b[i] = value;
            let _ = fontx2::read(
                &b,
                &Default::default(),
                &ResourceLimits {
                    max_glyphs: 1024,
                    max_total_bytes: 4 * 1024 * 1024,
                    ..Default::default()
                },
            );
        }
    }
}

#[test]
fn fragmented_codes_coarsen_with_located_loss_and_valid_output() {
    let mut f = load(&fixture(true, 8, 2));
    let sample = f.glyphs().next().unwrap().1.clone();
    let ids = f.glyphs().map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        f.remove_glyph(id);
    }
    for i in 0..256 {
        let mut glyph = sample.clone();
        glyph.codepoints = vec![Codepoint::Legacy(0x8100 + i * 2)];
        f.add_glyph(glyph).unwrap();
    }
    let (out, r) = fontx2::write(&f, &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code.ends_with("blank_slots")));
    assert_eq!(load(&out).len(), 510);
}
#[test]
fn ordered_duplicate_metadata_is_never_silent() {
    let mut f = load(&fixture(false, 8, 2));
    let prop = f.headers.iter().next().unwrap().clone();
    f.headers.push(prop);
    let (_, r) = fontx2::write(&f, &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code.ends_with("property_order")));
}
#[test]
fn source_metadata_and_diagnostics_share_budget() {
    use fontpixel::model::{Property, PropertyValue};
    let mut f = load(&fixture(true, 8, 2));
    f.properties.push(Property::new(
        "large",
        PropertyValue::Bytes(vec![0; 32_000]),
    ));
    let mut o = Fontx2WriteOptions::default();
    o.limits.max_total_bytes = 16_000;
    assert!(matches!(
        fontx2::write(&f, &o),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn every_unrepresentable_raster_survives_fatal_report() {
    let mut f = load(&fixture(true, 9, 3));
    let ids = f.glyphs().map(|(id, _)| id).collect::<Vec<_>>();
    for &id in &ids {
        f.edit_glyph(id, |g| {
            g.metrics.bounds.y = i64::MIN;
            Ok(())
        })
        .unwrap();
    }
    let error = fontx2::write(&f, &Default::default()).unwrap_err();
    for id in ids {
        assert!(error.report().unwrap().entries.iter().any(|entry| {
            entry.code.ends_with("write.omitted")
                && entry.location.glyph_index == Some(id.index())
                && entry.location.codepoint.is_some()
        }));
    }
}
