//! Explicit HZK geometry, presets, transpose, charset and bounded roundtrips.
#![cfg(feature = "hzk")]
use fontpixel::formats::hzk::{
    self, CodeSpace, Geometry, HzkReadOptions, HzkWriteOptions, Preset, Storage,
};
use fontpixel::model::Codepoint;
use fontpixel::{Font, ResourceLimits};
fn geo(storage: Storage) -> Geometry {
    Geometry {
        width: 3,
        height: 5,
        first_code: 0xb0a1,
        last_code: 0xb0fe,
        code_space: CodeSpace::Gb2312,
        storage,
    }
}
fn read(b: &[u8], g: Geometry) -> Font {
    hzk::read(
        b,
        &HzkReadOptions {
            geometry: Some(g),
            ..Default::default()
        },
        &Default::default(),
    )
    .unwrap()
    .0
}
#[test]
fn independent_nonsquare_transpose() {
    let rows = [0x80, 0x40, 0x20, 0xa0, 0x60];
    let cols = [0x90, 0x48, 0x38];
    let a = read(&rows, geo(Storage::Rows));
    let b = read(&cols, geo(Storage::Columns));
    assert_eq!(a, b);
    for s in [Storage::Rows, Storage::Columns] {
        let (out, r) = hzk::write(
            &a,
            &HzkWriteOptions {
                geometry: Some(geo(s)),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.has_loss(), "{r}");
        assert_eq!(
            out,
            if s == Storage::Rows {
                rows.to_vec()
            } else {
                cols.to_vec()
            }
        );
    }
}
#[test]
fn all_presets_have_native_roundtrip() {
    for &p in Preset::ALL {
        assert_eq!(p.name().parse::<Preset>().unwrap(), p);
        let g = p.geometry();
        let (w, h) = if g.storage == Storage::Rows {
            (g.width, g.height)
        } else {
            (g.height, g.width)
        };
        let stride = usize::from(w).div_ceil(8);
        let mut raw = vec![0; stride * usize::from(h) * 3];
        for (i, b) in raw.iter_mut().enumerate() {
            *b = (i * 47) as u8;
            if i % stride == stride - 1 && !w.is_multiple_of(8) {
                *b &= 255 << (8 - w % 8);
            }
        }
        let f = hzk::read(
            &raw,
            &HzkReadOptions {
                preset: Some(p),
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap()
        .0;
        let (out, r) = hzk::write(
            &f,
            &HzkWriteOptions {
                preset: Some(p),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.has_loss(), "{p:?}: {r}");
        assert_eq!(out, raw);
    }
}
#[test]
fn no_header_means_no_structural_detection() {
    assert_eq!(
        hzk::detect(&[0; 32]),
        fontpixel::formats::Confidence::NoMatch
    );
    assert!(hzk::read(&[0; 32], &Default::default(), &Default::default()).is_err());
    assert!(!fontpixel::registry().status("hzk").unwrap().detect);
}
#[test]
fn malformed_and_limits() {
    let g = geo(Storage::Rows);
    for n in [0, 1, 4, 6, 9] {
        assert!(
            hzk::read(
                &vec![0; n],
                &HzkReadOptions {
                    geometry: Some(g),
                    ..Default::default()
                },
                &Default::default()
            )
            .is_err()
        );
    }
    let f = read(&[0x80; 5], g);
    for max in [0, 1, 2048, 4096, 65536] {
        let mut o = HzkWriteOptions {
            geometry: Some(g),
            ..Default::default()
        };
        o.limits.max_total_bytes = max;
        if let Ok((out, _)) = hzk::write(&f, &o) {
            assert!(
                hzk::read(
                    &out,
                    &HzkReadOptions {
                        geometry: Some(g),
                        ..Default::default()
                    },
                    &o.limits
                )
                .is_ok()
            );
        }
    }
}
#[test]
fn gb2312_raw_does_not_require_tables_and_omissions_survive() {
    let mut f = read(&[0x80; 5], geo(Storage::Rows));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(0x1f600));
        Ok(())
    })
    .unwrap();
    let (_, r) = hzk::write(
        &f,
        &HzkWriteOptions {
            geometry: Some(geo(Storage::Rows)),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        r.entries
            .iter()
            .any(|e| e.location.codepoint == Some(0x1f600) && e.code.ends_with("write.omitted"))
    );
    f.edit_glyph(id, |g| {
        g.codepoints = vec![Codepoint::Unicode(0x1f600)];
        Ok(())
    })
    .unwrap();
    let e = hzk::write(
        &f,
        &HzkWriteOptions {
            geometry: Some(geo(Storage::Rows)),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(e.report().unwrap().entries.iter().any(
        |e| e.location.glyph_index == Some(id.index()) && e.location.codepoint == Some(0x1f600)
    ));
}
#[cfg(feature = "cjk-charsets")]
#[test]
fn unicode_to_gb2312_is_exact() {
    let mut f = read(&[0x80; 5], geo(Storage::Rows));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints = vec![Codepoint::Unicode('啊' as u32)];
        Ok(())
    })
    .unwrap();
    let (out, r) = hzk::write(
        &f,
        &HzkWriteOptions {
            geometry: Some(geo(Storage::Rows)),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.has_loss());
    assert_eq!(
        read(&out, geo(Storage::Rows))
            .glyphs()
            .next()
            .unwrap()
            .1
            .codepoints,
        [Codepoint::Legacy(0xb0a1)]
    );
}
#[test]
fn random_small_geometry_roundtrips() {
    let mut state = 42u64;
    for w in 1..20 {
        for h in 1..10 {
            for storage in [Storage::Rows, Storage::Columns] {
                let g = Geometry {
                    width: w,
                    height: h,
                    ..geo(storage)
                };
                let (ww, hh) = if storage == Storage::Rows {
                    (w, h)
                } else {
                    (h, w)
                };
                let stride = usize::from(ww).div_ceil(8);
                let mut raw = vec![0; stride * usize::from(hh) * 2];
                for (i, b) in raw.iter_mut().enumerate() {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    *b = (state >> 40) as u8;
                    if i % stride == stride - 1 && !ww.is_multiple_of(8) {
                        *b &= 255 << (8 - ww % 8);
                    }
                }
                let f = read(&raw, g);
                let (out, r) = hzk::write(
                    &f,
                    &HzkWriteOptions {
                        geometry: Some(g),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(!r.has_loss(), "{r}");
                assert_eq!(out, raw);
            }
        }
    }
}
#[test]
fn tiny_table_limit_is_irrelevant_to_headerless_data() {
    let l = ResourceLimits {
        max_table_bytes: 0,
        ..Default::default()
    };
    assert!(
        hzk::read(
            &[0; 5],
            &HzkReadOptions {
                geometry: Some(geo(Storage::Rows)),
                ..Default::default()
            },
            &l
        )
        .is_ok()
    );
}

#[test]
fn source_metadata_and_output_share_budget() {
    use fontpixel::model::{Property, PropertyValue};
    let mut f = read(&[0x80; 5], geo(Storage::Rows));
    f.properties.push(Property::new(
        "large",
        PropertyValue::Bytes(vec![0; 32_000]),
    ));
    let mut o = HzkWriteOptions {
        geometry: Some(geo(Storage::Rows)),
        ..Default::default()
    };
    o.limits.max_total_bytes = 16_000;
    assert!(matches!(
        hzk::write(&f, &o),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn every_unrepresentable_raster_survives_fatal_report() {
    let mut f = read(&[0; 15], geo(Storage::Rows));
    let ids = f.glyphs().map(|(id, _)| id).collect::<Vec<_>>();
    for &id in &ids {
        f.edit_glyph(id, |g| {
            g.metrics.bounds.x = i64::MIN;
            Ok(())
        })
        .unwrap();
    }
    let error = hzk::write(
        &f,
        &HzkWriteOptions {
            geometry: Some(geo(Storage::Rows)),
            ..Default::default()
        },
    )
    .unwrap_err();
    for id in ids {
        assert!(error.report().unwrap().entries.iter().any(|entry| {
            entry.code.ends_with("write.omitted")
                && entry.location.glyph_index == Some(id.index())
                && entry.location.codepoint.is_some()
        }));
    }
}
