//! Original Macintosh resource fixtures and boundary regressions.
#![cfg(feature = "nfnt")]
use fontpixel::formats::nfnt::{
    self, Container, NfntCodec, NfntReadOptions, NfntWriteOptions, ResourceType,
};
use fontpixel::formats::{FormatReader, ReadOptions};
use fontpixel::model::{Codepoint, Property, PropertyValue, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{Error, ResourceLimits};

// Original wire fixture: A/B/missing, a two-byte-wide shared bitmap.
fn wire() -> Vec<u8> {
    vec![
        0x90, 0x00, 0, 65, 0, 66, 0, 3, 0xff, 0xff, 0xff, 0xff, 0, 3, 0, 3, 0, 12, 0, 2, 0, 1, 0,
        1, 0, 1, 0xb8, 0, 0xc8, 0, 0xb8, 0, 0, 0, 0, 2, 0, 3, 0, 5, 0, 3, 1, 2, 0, 3, 0xff, 0xff,
    ]
}
fn read(b: &[u8]) -> fontpixel::FontSet {
    nfnt::read(b, &NfntReadOptions::default(), &ResourceLimits::default())
        .unwrap()
        .0
}
fn options(c: Container) -> NfntWriteOptions {
    NfntWriteOptions {
        container: c,
        ..Default::default()
    }
}
#[test]
fn hand_authored_raster_and_exact_bare_roundtrip() {
    let b = wire();
    let set = read(&b);
    let f = &set.fonts[0];
    assert_eq!(f.len(), 3);
    let a = f.glyphs().next().unwrap().1;
    assert_eq!(a.codepoints, [Codepoint::Legacy(65)]);
    assert_eq!(a.metrics.advance, Some(Vector::new(3, 0)));
    assert_eq!(a.metrics.bounds.x, -1);
    assert_eq!(a.metrics.bounds.y, -1);
    assert_eq!(a.bitmap.as_bytes(), [0x80, 0xc0, 0x80]);
    let (out, r) = nfnt::write(&set, &options(Container::Bare)).unwrap();
    assert_eq!(b, out);
    assert!(!r.has_loss(), "{r}");
    assert_eq!(set, read(&out));
}
#[test]
fn optional_tables_and_unknown_flag_preserved() {
    let mut b = wire();
    b[0] |= 0x40;
    b[1] = 3;
    b.extend_from_slice(&[3, 0, 2, 128, 3, 64, 0, 3, 1, 2, 0, 3]);
    let set = read(&b);
    let (out, r) = nfnt::write(&set, &options(Container::Bare)).unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn every_single_asset_container_roundtrips() {
    for c in [
        Container::ResourceFork,
        Container::Dfont,
        Container::Suitcase,
        Container::MacBinary,
        Container::AppleSingle,
        Container::BinHex,
    ] {
        let source = read(&wire());
        let (out, _) = nfnt::write(&source, &options(c)).unwrap();
        let parsed = read(&out);
        let (again, r) = nfnt::write(&parsed, &options(c)).unwrap();
        assert!(!r.has_loss(), "{c:?}: {r}");
        assert_eq!(out, again);
        assert_eq!(parsed, read(&again));
    }
}
#[test]
fn font_resources_and_multi_strike() {
    let mut source = read(&wire());
    let mut second = source.fonts[0].clone();
    second.properties.remove("nfnt.source");
    second.metrics.pixel_size = Some(24);
    source.fonts.push(second);
    let opts = NfntWriteOptions {
        resource_type: ResourceType::Font,
        ..Default::default()
    };
    let (out, _) = nfnt::write(&source, &opts).unwrap();
    let parsed = read(&out);
    assert_eq!(parsed.fonts.len(), 2);
    assert_eq!(parsed.fonts[1].metrics.pixel_size, Some(3));
    let (_, r) = nfnt::write(&parsed, &opts).unwrap();
    assert!(!r.has_loss(), "{r}");
}
struct Assets(Vec<(String, Vec<u8>)>);
impl ResourceResolver for Assets {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = &self
            .0
            .iter()
            .find(|v| v.0 == p.as_str())
            .ok_or_else(|| Error::MissingResource(p.to_string()))?
            .1;
        if b.len() > max {
            return Err(Error::ResourceLimit {
                resource: "test resource",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
#[test]
fn appledouble_uses_scoped_companion_and_bundle() {
    let mut source = read(&wire());
    source.properties.push(Property::new(
        "nfnt.data-fork",
        PropertyValue::Bytes(b"data fork".to_vec()),
    ));
    let p = ResourcePath::new("family.suit").unwrap();
    let written = nfnt::write_bundle(&source, &options(Container::AppleDouble), &p).unwrap();
    assert_eq!(written.output.assets().len(), 2);
    let assets = Assets(
        written
            .output
            .assets()
            .iter()
            .map(|a| (a.path.to_string(), a.bytes.clone()))
            .collect(),
    );
    let input = Input::new(b"data fork").with_resources(&assets);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits).unwrap();
    let codec = NfntCodec {
        read_options: NfntReadOptions {
            apple_double_resource: Some(ResourcePath::new("._family.suit").unwrap()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (set, _) = codec.read(&mut c, &ReadOptions::default()).unwrap();
    assert_eq!(
        set.properties.get("nfnt.data-fork"),
        source.properties.get("nfnt.data-fork")
    );
    let written = nfnt::write_bundle(&set, &options(Container::AppleDouble), &p).unwrap();
    assert!(!written.report.has_loss(), "{}", written.report);
    assert!(matches!(
        nfnt::write(&set, &options(Container::AppleDouble)),
        Err(Error::MultipleAssets { .. })
    ));
}
#[test]
fn resource_opaque_and_fond_optional_tables_survive() {
    let source = read(&wire());
    let (out, _) = nfnt::write(&source, &options(Container::Dfont)).unwrap();
    let mut set = read(&out);
    let raw = vec![
        b'T', b'E', b'S', b'T', 0, 7, 0x80, 1, 2, 3, 4, 0, 3, b'o', b'n', b'e', 9, 8, 7,
    ];
    set.properties.push(Property::new(
        "nfnt.resource",
        PropertyValue::Bytes(raw.clone()),
    ));
    let (out, r) = nfnt::write(&set, &options(Container::Dfont)).unwrap();
    assert!(!r.has_loss(), "{r}");
    let parsed = read(&out);
    assert!(
        parsed
            .properties
            .iter()
            .any(|p| p.value == PropertyValue::Bytes(raw.clone()))
    );
}
#[test]
fn malformed_ranges_truncations_crc_and_overlap() {
    let b = wire();
    for n in 0..b.len() {
        assert!(
            nfnt::read(&b[..n], &Default::default(), &Default::default()).is_err(),
            "truncation {n}"
        );
    }
    let mut bad = b.clone();
    bad[34..36].copy_from_slice(&99u16.to_be_bytes());
    assert!(nfnt::read(&bad, &Default::default(), &Default::default()).is_err());
    let (mut mac, _) = nfnt::write(&read(&b), &options(Container::MacBinary)).unwrap();
    mac[65] ^= 1;
    assert!(nfnt::read(&mac, &Default::default(), &Default::default()).is_err());
    let (mut apple, _) = nfnt::write(&read(&b), &options(Container::AppleSingle)).unwrap();
    apple[30..34].copy_from_slice(&0u32.to_be_bytes());
    assert!(nfnt::read(&apple, &Default::default(), &Default::default()).is_err());
}
#[test]
fn resource_limits_cover_all_strikes_and_unknown_records() {
    let set = read(&wire());
    let l = ResourceLimits {
        max_glyphs: 2,
        ..Default::default()
    };
    assert!(nfnt::read(&wire(), &Default::default(), &l).is_err());
    let opts = NfntWriteOptions {
        limits: l,
        ..Default::default()
    };
    assert!(nfnt::write(&set, &opts).is_err());
    let mut set = read(&wire());
    set.fonts.push(set.fonts[0].clone());
    let (out, _) = nfnt::write(&set, &Default::default()).unwrap();
    let l = ResourceLimits {
        max_glyphs: 5,
        ..Default::default()
    };
    assert!(nfnt::read(&out, &Default::default(), &l).is_err());
    let l = ResourceLimits {
        max_resources: 1,
        ..Default::default()
    };
    assert!(nfnt::read(&out, &Default::default(), &l).is_err());
}
#[test]
fn writer_same_limits_readable_and_inclusive_budget() {
    let source = read(&wire());
    let o = NfntWriteOptions {
        limits: ResourceLimits {
            max_glyphs: 3,
            ..Default::default()
        },
        container: Container::Bare,
        ..Default::default()
    };
    let (bytes, _) = nfnt::write(&source, &o).unwrap();
    assert!(nfnt::read(&bytes, &Default::default(), &o.limits).is_ok());
    let o = NfntWriteOptions {
        max_output_bytes: wire().len() - 1,
        ..o
    };
    assert!(nfnt::write(&source, &o).is_err());
}
#[test]
fn complete_metadata_and_unicode_loss_is_reported() {
    let mut f = read(&wire()).into_single().unwrap();
    f.names.copyright = Some("copyright".into());
    f.metrics.ppem_x = Some(17);
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.metrics.vertical_origin = Some(Vector::new(1, 2));
        g.name = Some("A".into());
        g.properties.push(Property::new("x", 9i64));
        Ok(())
    })
    .unwrap();
    let (_, r) = nfnt::write(&f.into(), &options(Container::Bare)).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "nfnt.names.copyright"));
    assert!(r.entries.iter().any(|e| e.code == "nfnt.metrics.ppem_x"));
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "nfnt.glyph_metrics.vertical_origin")
    );
}
#[test]
fn all_unrepresentable_keeps_omissions() {
    let mut f = fontpixel::Font::default();
    let mut g = fontpixel::Glyph::new("#".parse::<fontpixel::Bitmap>().unwrap());
    g.codepoints.push(Codepoint::Unicode(0x4e00));
    f.add_glyph(g).unwrap();
    let err = nfnt::write(&f.into(), &options(Container::Bare)).unwrap_err();
    let Error::ReportedErrors(r) = err else {
        panic!("unexpected {err:?}")
    };
    assert!(r.entries.iter().any(|e| e.code == "nfnt.mapping_omitted"));
    assert!(r.has_errors());
}

#[test]
fn grayscale_depths_decode_and_exact_source_writes() {
    for (depth, flag) in [(2, 4), (4, 8), (8, 12)] {
        // One encoded sample and one missing sample, height one.
        let mut b = vec![
            0x90, flag, 0, 65, 0, 65, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 9, 0, 1, 0, 0, 0, 0, 0, 1,
        ];
        let a = 1u8 << (8 - depth);
        let strike = if depth == 8 {
            vec![1, 255]
        } else {
            vec![a | (((1u16 << depth) - 1) << (8 - 2 * depth)) as u8, 0]
        };
        b.extend_from_slice(&strike);
        b.extend_from_slice(&[0, 0, 0, 1, 0, 2, 0, 1, 0, 1, 0xff, 0xff]);
        let set = read(&b);
        let g = set.fonts[0].glyphs().next().unwrap().1;
        assert!(matches!(g.bitmap, fontpixel::model::GlyphBitmap::Gray8(_)));
        assert_eq!(g.bitmap.as_bytes(), [255 / ((1u16 << depth) - 1) as u8]);
        let (bytes, report) = nfnt::write(&set, &options(Container::Bare)).unwrap();
        assert_eq!(bytes, b);
        assert!(!report.has_loss(), "{report}");
    }
}
#[test]
fn generated_models_preserve_legacy_geometry_after_normalization() {
    for n in 1..40 {
        let mut f = fontpixel::Font::default();
        for code in 0..n {
            let mut bitmap: fontpixel::Bitmap =
                fontpixel::Bitmap::new((code % 7 + 1) as u32, (n % 9 + 1) as u32).unwrap();
            bitmap.set_pixel(0, 0, 1).unwrap();
            let mut g = fontpixel::Glyph::new(bitmap);
            g.codepoints.push(Codepoint::Legacy((32 + code) as u32));
            g.metrics.bounds.x = (code % 3) - 1;
            g.metrics.bounds.y = -2;
            g.metrics.advance = Some(Vector::new(code % 7 + 2, 0));
            f.add_glyph(g).unwrap();
        }
        let (b, _) = nfnt::write(&f.into(), &options(Container::Dfont)).unwrap();
        let set = read(&b);
        let (again, r) = nfnt::write(&set, &options(Container::Dfont)).unwrap();
        assert_eq!(b, again);
        assert!(!r.has_loss(), "{r}");
    }
}
#[test]
fn minimum_aggregate_budget_is_inclusive() {
    let b = wire();
    let mut lo = 0;
    let mut hi = 100000;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let limits = ResourceLimits {
            max_total_bytes: mid,
            ..Default::default()
        };
        if nfnt::read(&b, &Default::default(), &limits).is_ok() {
            hi = mid
        } else {
            lo = mid + 1
        }
    }
    assert!(
        nfnt::read(
            &b,
            &Default::default(),
            &ResourceLimits {
                max_total_bytes: lo,
                ..Default::default()
            }
        )
        .is_ok()
    );
    assert!(
        nfnt::read(
            &b,
            &Default::default(),
            &ResourceLimits {
                max_total_bytes: lo - 1,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn apple_data_fork_resource_variant() {
    let (fork, _) = nfnt::write(&read(&wire()), &options(Container::Dfont)).unwrap();
    let mut b = vec![0, 5, 0x16, 0, 0, 2, 0, 0];
    b.extend_from_slice(&[0; 16]);
    b.extend_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0, 0, 38]);
    b.extend_from_slice(&(fork.len() as u32).to_be_bytes());
    b.extend_from_slice(&fork);
    let set = read(&b);
    let (out, r) = nfnt::write(&set, &options(Container::AppleSingle)).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(out, b);
}
#[test]
fn extreme_origin_differences_never_panic() {
    let mut f = read(&wire()).into_single().unwrap();
    let ids = f.glyphs().map(|(i, _)| i).collect::<Vec<_>>();
    f.edit_glyph(ids[0], |g| {
        g.metrics.bounds.x = -32768;
        Ok(())
    })
    .unwrap();
    f.edit_glyph(ids[1], |g| {
        g.metrics.bounds.x = i64::MAX - 1;
        Ok(())
    })
    .unwrap();
    assert!(nfnt::write(&f.into(), &options(Container::Bare)).is_err());
}

#[test]
fn authored_gray8_preserves_all_samples_and_threshold_is_explicit() {
    let bitmap = fontpixel::Bitmap::<fontpixel::bitmap::Gray8>::from_raw(
        3,
        2,
        &[0, 1, 2, 127, 128, 255],
        Default::default(),
    )
    .unwrap();
    let mut g = fontpixel::Glyph::new(bitmap);
    g.codepoints.push(Codepoint::Legacy(65));
    g.metrics.advance = Some(Vector::new(3, 0));
    let f = fontpixel::Font::from_glyphs([g]).unwrap();
    let set = fontpixel::FontSet::from(f);
    let (b, _) = nfnt::write(&set, &options(Container::Bare)).unwrap();
    let f = read(&b).into_single().unwrap();
    assert_eq!(
        f.glyphs().next().unwrap().1.bitmap.as_bytes(),
        [0, 1, 2, 127, 128, 255]
    );
    let (b, r) = nfnt::write(
        &set,
        &NfntWriteOptions {
            gray_threshold: Some(128),
            ..options(Container::Bare)
        },
    )
    .unwrap();
    assert!(r.entries.iter().any(|e| e.code == "nfnt.gray_threshold"));
    assert_eq!(
        read(&b).fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes(),
        [0, 0x60]
    );
}
#[test]
fn cumulative_envelope_and_resource_count() {
    let (b, _) = nfnt::write(&read(&wire()), &options(Container::AppleSingle)).unwrap();
    let l = ResourceLimits {
        max_resources: 2,
        ..Default::default()
    };
    assert!(nfnt::read(&b, &Default::default(), &l).is_err());
    let l = ResourceLimits {
        max_resources: 3,
        ..Default::default()
    };
    assert!(nfnt::read(&b, &Default::default(), &l).is_ok());
}

#[test]
fn appledouble_write_preflights_companion_resolution_limits() {
    let set = read(&wire());
    let options = NfntWriteOptions {
        container: Container::AppleDouble,
        limits: ResourceLimits {
            max_resources: 3,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(nfnt::write_bundle(&set, &options, &ResourcePath::new("font").unwrap()).is_err());
    let options = NfntWriteOptions {
        limits: ResourceLimits {
            max_resource_bytes: 200,
            ..Default::default()
        },
        ..options
    };
    assert!(nfnt::write_bundle(&set, &options, &ResourcePath::new("font").unwrap()).is_err());
}
#[test]
fn mutations_remain_bounded_and_readable() {
    let b = wire();
    for i in 0..b.len() {
        for bit in 0..8 {
            let mut bytes = b.clone();
            bytes[i] ^= 1 << bit;
            if let Ok((set, _)) = nfnt::read(&bytes, &Default::default(), &Default::default())
                && let Ok((output, r)) = nfnt::write(&set, &options(Container::Bare))
            {
                let reread = read(&output);
                if !r.has_loss() {
                    assert_eq!(set, reread);
                }
            }
        }
    }
}

fn color_fork(entries: &[(u16, u16, u16, u16)], flags: u16) -> Vec<u8> {
    let nfnt = vec![
        0x92, 0x84, 0, 65, 0, 65, 0, 4, 0, 0, 0, 0, 0, 4, 0, 1, 0, 9, 0, 1, 0, 0, 0, 0, 0, 1, 0x1b,
        0, 0, 0, 0, 4, 0, 4, 0, 4, 0, 0, 0xff, 0xff,
    ];
    let mut palette = vec![0, 0, 4, 0];
    palette.extend_from_slice(&flags.to_be_bytes());
    palette.extend_from_slice(&(entries.len() as u16).wrapping_sub(1).to_be_bytes());
    for &(index, r, g, b) in entries {
        for word in [index, r, g, b] {
            palette.extend_from_slice(&word.to_be_bytes());
        }
    }
    let mut data = Vec::new();
    data.extend_from_slice(&(nfnt.len() as u32).to_be_bytes());
    data.extend_from_slice(&nfnt);
    let second = data.len();
    data.extend_from_slice(&(palette.len() as u32).to_be_bytes());
    data.extend_from_slice(&palette);
    let mut header = Vec::new();
    for word in [256u32, 256 + data.len() as u32, data.len() as u32, 70] {
        header.extend_from_slice(&word.to_be_bytes());
    }
    let mut map = header.clone();
    map.extend_from_slice(&[0; 8]);
    for word in [28u16, 70, 1] {
        map.extend_from_slice(&word.to_be_bytes());
    }
    map.extend_from_slice(b"NFNT");
    for word in [0u16, 18] {
        map.extend_from_slice(&word.to_be_bytes());
    }
    map.extend_from_slice(b"fctb");
    for word in [0u16, 30] {
        map.extend_from_slice(&word.to_be_bytes());
    }
    for offset in [0u32, second as u32] {
        map.extend_from_slice(&[0, 128, 0xff, 0xff]);
        map.extend_from_slice(&offset.to_be_bytes());
        map.extend_from_slice(&[0; 4]);
    }
    let mut bytes = header;
    bytes.resize(256, 0);
    bytes.extend_from_slice(&data);
    bytes.extend_from_slice(&map);
    bytes
}
#[test]
fn indexed_fctb_projects_rgb_and_preserves_original_resources() {
    let raw = color_fork(
        &[
            (0, 65535, 65535, 65535),
            (1, 65535, 0, 0),
            (2, 0, 65535, 0),
            (3, 0, 0, 65535),
        ],
        0,
    );
    let (set, r) = nfnt::read(&raw, &Default::default(), &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "nfnt.palette_to_gray"));
    assert!(
        !r.entries
            .iter()
            .any(|e| e.code == "nfnt.palette_port_colors")
    );
    assert_eq!(
        set.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
        [0, 179, 105, 226]
    );
    let source = set.fonts[0].properties.get("nfnt.source").unwrap().clone();
    let (bytes, _) = nfnt::write(&set, &options(Container::Dfont)).unwrap();
    let parsed = read(&bytes);
    assert_eq!(parsed.fonts[0].properties.get("nfnt.source"), Some(&source));
    assert_eq!(
        parsed.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
        [0, 179, 105, 226]
    );
    let (bytes, r) = nfnt::write(&set, &options(Container::Bare)).unwrap();
    assert!(r.has_loss());
    assert_eq!(u16::from_be_bytes([bytes[0], bytes[1]]) & 0x280, 0);
    assert_eq!(
        read(&bytes).fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes(),
        [0, 179, 105, 226]
    );
}
#[test]
fn partial_palette_uses_ordered_port_ramp_and_device_indices() {
    let raw = color_fork(&[(3, 65535, 0, 0)], 0);
    let (set, r) = nfnt::read(&raw, &Default::default(), &Default::default()).unwrap();
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "nfnt.palette_port_colors")
    );
    assert_eq!(
        set.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
        [0, 127, 255, 179]
    );
    let raw = color_fork(
        &[
            (0, 65535, 65535, 65535),
            (0, 65535, 0, 0),
            (0, 0, 65535, 0),
            (0, 0, 0, 65535),
        ],
        0x8000,
    );
    assert_eq!(
        read(&raw).fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes(),
        [0, 179, 105, 226]
    );
}
#[test]
fn edited_palette_font_writes_model_samples_not_stale_indices() {
    let mut set = read(&color_fork(
        &[
            (0, 65535, 65535, 65535),
            (1, 65535, 0, 0),
            (2, 0, 65535, 0),
            (3, 0, 0, 65535),
        ],
        0,
    ));
    let id = set.fonts[0].glyphs().next().unwrap().0;
    set.fonts[0]
        .edit_glyph(id, |g| {
            if let fontpixel::model::GlyphBitmap::Gray8(b) = &mut g.bitmap {
                b.set_pixel(0, 0, 42)?;
            }
            Ok(())
        })
        .unwrap();
    let (bytes, r) = nfnt::write(&set, &Default::default()).unwrap();
    assert!(r.has_loss());
    let read = read(&bytes);
    assert_eq!(
        read.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
        [42, 179, 105, 226]
    );
    assert!(
        read.properties
            .iter()
            .any(|p| matches!(&p.value,PropertyValue::Bytes(b) if b.starts_with(b"fctb")))
    );
}
#[test]
fn malformed_palettes_fail_explicitly() {
    for (entries, flags) in [
        (vec![(1, 0, 0, 0), (1, 65535, 65535, 65535)], 0),
        (vec![(4, 0, 0, 0)], 0),
        (vec![(1, 0, 0, 0)], 0x4000),
    ] {
        assert!(
            nfnt::read(
                &color_fork(&entries, flags),
                &Default::default(),
                &Default::default()
            )
            .is_err()
        );
    }
}

// Independent old-ROM FONT: family 200 named Griffin, ten-point strike.
fn classic_font_fork() -> Vec<u8> {
    let raw = wire();
    let mut data = vec![0; 4]; // empty family-name resource
    data.extend_from_slice(&(raw.len() as u32).to_be_bytes());
    data.extend_from_slice(&raw);
    let mut header = Vec::new();
    for word in [256u32, 256 + data.len() as u32, data.len() as u32, 70] {
        header.extend_from_slice(&word.to_be_bytes());
    }
    let mut map = header.clone();
    map.extend_from_slice(&[0; 8]);
    for word in [28u16, 62, 0] {
        map.extend_from_slice(&word.to_be_bytes());
    }
    map.extend_from_slice(b"FONT");
    for word in [1u16, 10] {
        map.extend_from_slice(&word.to_be_bytes());
    }
    for (id, name, offset) in [(25600u16, 0u16, 0u32), (25610, 65535, 4)] {
        map.extend_from_slice(&id.to_be_bytes());
        map.extend_from_slice(&name.to_be_bytes());
        map.extend_from_slice(&offset.to_be_bytes());
        map.extend_from_slice(&[0; 4]);
    }
    map.extend_from_slice(b"\x07Griffin");
    header.resize(256, 0);
    header.extend_from_slice(&data);
    header.extend_from_slice(&map);
    header
}
#[test]
fn classic_font_name_resource_and_encoded_point_size() {
    let set = read(&classic_font_fork());
    assert_eq!(set.fonts.len(), 1);
    assert_eq!(set.fonts[0].names.family.as_deref(), Some("Griffin"));
    assert_eq!(set.fonts[0].metrics.pixel_size, Some(10));
    assert_eq!(
        set.properties
            .iter()
            .filter(|p| p.name == "nfnt.resource")
            .count(),
        1
    );
    let (out, r) = nfnt::write(&set, &Default::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(set, read(&out));
}
#[test]
fn authored_font_generates_valid_classic_ids_and_name_record() {
    let mut set = read(&wire());
    set.fonts[0].names.family = Some("Classic".into());
    let opts = NfntWriteOptions {
        resource_type: ResourceType::Font,
        ..Default::default()
    };
    let (out, _) = nfnt::write(&set, &opts).unwrap();
    let set = read(&out);
    let id = set.fonts[0]
        .properties
        .iter()
        .find(|p| p.name == "nfnt.resource")
        .unwrap();
    let PropertyValue::Bytes(metadata) = &id.value else {
        panic!("metadata")
    };
    assert_eq!(
        u16::from_be_bytes([metadata[4], metadata[5]]),
        128 * 128 + 3
    );
    assert_eq!(
        set.properties
            .iter()
            .filter(|p| p.name == "nfnt.resource")
            .count(),
        2
    );
    let (again, r) = nfnt::write(&set, &opts).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(out, again);
}

#[test]
fn derived_properties_and_trailing_glyph_slots_are_reported() {
    let mut set = read(&wire());
    set.fonts[0].properties.remove("nfnt.source");
    let (_, r) = nfnt::write(&set, &options(Container::Bare)).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "nfnt.property_added"));
    let mut set = read(&wire());
    let last = set.fonts[0].add_glyph(fontpixel::Glyph::default()).unwrap();
    set.fonts[0].remove_glyph(last);
    let (_, r) = nfnt::write(&set, &options(Container::Bare)).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "nfnt.glyph_slots"));
}

#[test]
fn appledouble_empty_primary_is_present_in_readback_model() {
    let path = ResourcePath::new("font").unwrap();
    let opts = options(Container::AppleDouble);
    let first = nfnt::write_bundle(&read(&wire()), &opts, &path).unwrap();
    let sidecar = &first
        .output
        .assets()
        .iter()
        .find(|a| a.path != path)
        .unwrap()
        .bytes;
    let source = read(sidecar);
    assert!(source.properties.get("nfnt.data-fork").is_none());
    let written = nfnt::write_bundle(&source, &opts, &path).unwrap();
    assert!(
        written
            .report
            .entries
            .iter()
            .any(|e| e.code == "nfnt.property_added")
    );
    let assets = Assets(
        written
            .output
            .assets()
            .iter()
            .map(|a| (a.path.to_string(), a.bytes.clone()))
            .collect(),
    );
    let input = Input::new(&[]).with_resources(&assets);
    let limits = ResourceLimits::default();
    let mut context = ReadContext::new(&input, &limits).unwrap();
    let codec = NfntCodec {
        read_options: NfntReadOptions {
            apple_double_resource: Some(ResourcePath::new("._font").unwrap()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (source, _) = codec.read(&mut context, &ReadOptions::default()).unwrap();
    let written = nfnt::write_bundle(&source, &opts, &path).unwrap();
    assert!(!written.report.has_loss(), "{}", written.report);
}

#[test]
fn detector_requires_resource_and_raster_structure() {
    use fontpixel::formats::Confidence;
    // These bytes met the previous MacBinary classifier but contain no fork.
    let mut unrelated = vec![0; 158];
    unrelated[1] = 1;
    unrelated[87..91].copy_from_slice(&30u32.to_be_bytes());
    assert_eq!(nfnt::detect(&unrelated), Confidence::NoMatch);
    let source = read(&wire());
    let (mut fork, _) = nfnt::write(&source, &options(Container::Dfont)).unwrap();
    let map = u32::from_be_bytes(fork[4..8].try_into().unwrap()) as usize;
    let type_start =
        map + u16::from_be_bytes(fork[map + 24..map + 26].try_into().unwrap()) as usize;
    let types = usize::from(u16::from_be_bytes(
        fork[type_start..type_start + 2].try_into().unwrap(),
    )) + 1;
    for t in 0..types {
        let at = type_start + 2 + t * 8;
        if &fork[at..at + 4] == b"NFNT" {
            fork[at..at + 4].copy_from_slice(b"sfnt");
        }
    }
    assert_eq!(nfnt::detect(&fork), Confidence::NoMatch);
    let mut apple = vec![0, 5, 0x16, 0, 0, 2, 0, 0];
    apple.extend_from_slice(&[0; 16]);
    apple.extend_from_slice(&[0, 1, 0, 0, 0, 2, 0, 0, 0, 38]);
    apple.extend_from_slice(&(fork.len() as u32).to_be_bytes());
    apple.extend_from_slice(&fork);
    assert_eq!(nfnt::detect(&apple), Confidence::NoMatch);
    // A plausible fontType/range alone must not identify a raw resource.
    for at in [14, 16, 24, 34, 46] {
        let mut raw = wire();
        raw[at..at + 2].copy_from_slice(&(if at == 46 { 0u16 } else { 65535 }).to_be_bytes());
        assert_eq!(nfnt::detect(&raw), Confidence::NoMatch, "bad field {at}");
    }
}

#[test]
fn detector_keeps_all_font_containers_and_extended_tables() {
    use fontpixel::formats::Confidence;
    let source = read(&wire());
    for c in [
        Container::Bare,
        Container::ResourceFork,
        Container::Dfont,
        Container::Suitcase,
        Container::MacBinary,
        Container::AppleSingle,
        Container::BinHex,
        Container::AppleDouble,
    ] {
        let written =
            nfnt::write_bundle(&source, &options(c), &ResourcePath::new("font").unwrap()).unwrap();
        let asset = written
            .output
            .assets()
            .iter()
            .find(|a| !a.bytes.is_empty())
            .unwrap();
        assert_ne!(nfnt::detect(&asset.bytes), Confidence::NoMatch, "{c:?}");
        for n in 0..asset.bytes.len() {
            if nfnt::read(&asset.bytes[..n], &Default::default(), &Default::default()).is_err() {
                assert_eq!(
                    nfnt::detect(&asset.bytes[..n]),
                    Confidence::NoMatch,
                    "{c:?} truncation {n}"
                );
            }
        }
        if c == Container::MacBinary {
            for version in [0, 129, 130] {
                let mut bytes = asset.bytes.clone();
                bytes[122] = version;
                bytes[123] = if version == 0 { 0 } else { 129 };
                if version == 130 {
                    bytes[102..106].copy_from_slice(b"mBIN");
                }
                let mut crc = 0u16;
                for &b in &bytes[..124] {
                    crc ^= u16::from(b) << 8;
                    for _ in 0..8 {
                        crc = if crc & 0x8000 != 0 {
                            (crc << 1) ^ 0x1021
                        } else {
                            crc << 1
                        };
                    }
                }
                bytes[124..126].copy_from_slice(&crc.to_be_bytes());
                assert_eq!(
                    nfnt::detect(&bytes),
                    Confidence::Certain,
                    "MacBinary {version}"
                );
            }
        }
        if matches!(c, Container::AppleSingle | Container::AppleDouble) {
            let mut bytes = asset.bytes.clone();
            bytes[4..8].copy_from_slice(&0x10000u32.to_be_bytes());
            assert_eq!(nfnt::detect(&bytes), Confidence::Certain);
            if c == Container::AppleSingle {
                bytes[26..30].copy_from_slice(&1u32.to_be_bytes());
                assert_eq!(nfnt::detect(&bytes), Confidence::Certain);
            }
        }
    }
    assert_ne!(nfnt::detect(&classic_font_fork()), Confidence::NoMatch);
    assert_ne!(
        nfnt::detect(&color_fork(&[(0, 65535, 65535, 65535)], 0)),
        Confidence::NoMatch
    );
    let mut raw = wire()[..26].to_vec();
    raw[14..16].copy_from_slice(&8192u16.to_be_bytes());
    raw[18..20].copy_from_slice(&8191u16.to_be_bytes());
    raw[24..26].copy_from_slice(&8u16.to_be_bytes());
    let ow = (26 + 8192 * 16 + 8 - 16) / 2;
    raw[10..12].copy_from_slice(&((ow >> 16) as u16).to_be_bytes());
    raw[16..18].copy_from_slice(&(ow as u16).to_be_bytes());
    raw.resize(26 + 8192 * 16, 0);
    raw.extend_from_slice(&wire()[32..]);
    assert!(nfnt::read(&raw, &Default::default(), &Default::default()).is_ok());
    assert_eq!(nfnt::detect(&raw), Confidence::Possible);
}

#[cfg(feature = "vector-raster")]
#[test]
fn detector_rejects_generated_plain_and_embedded_opentype() {
    use fontpixel::formats::{
        Confidence,
        vector_raster::{self, WriteOptions},
    };
    use fontpixel::variants::OutlineFlavor;
    for width in [1, 3, 8] {
        let mut f = fontpixel::Font::default();
        f.metrics.pixel_size = Some(8);
        f.metrics.ascent = Some(7);
        f.metrics.descent = Some(1);
        f.names.family = Some(format!("Detection {width}"));
        let mut bitmap: fontpixel::Bitmap = fontpixel::Bitmap::new(width, 3).unwrap();
        bitmap.set_pixel(0, 0, 1).unwrap();
        let mut g = fontpixel::Glyph::new(bitmap);
        g.codepoints.push(Codepoint::Unicode(65));
        g.metrics.advance = Some(Vector::new(i64::from(width) + 1, 0));
        f.add_glyph(g).unwrap();
        for (flavor, id) in [
            (OutlineFlavor::TrueType, "ttf"),
            (OutlineFlavor::Cff, "otf"),
        ] {
            for embedded in [false, true] {
                let mut o = WriteOptions::default();
                o.outline.flavor = flavor;
                o.outline.embed_bitmaps = embedded;
                let (bytes, _) = vector_raster::write(&f.clone().into(), id, &o).unwrap();
                assert_eq!(
                    nfnt::detect(&bytes),
                    Confidence::NoMatch,
                    "{id} embedded={embedded} width={width}"
                );
            }
        }
    }
}

// Independent literal-only HQX authoring; CRC follows the definition's bit-in
// register followed by sixteen zero bits, unlike production's byte-XOR loop.
fn binhex_crc(bytes: &[u8]) -> u16 {
    let mut register = 0u16;
    for byte in bytes.iter().copied().chain([0, 0]) {
        for bit in (0..8).rev() {
            let carry = register & 0x8000 != 0;
            register = (register << 1) | u16::from((byte >> bit) & 1);
            if carry {
                register ^= 0x1021;
            }
        }
    }
    register
}
fn hqx_rle(bytes: &[u8]) -> Vec<u8> {
    let alphabet = b"!\"#$%&'()*+,-012345689@ABCDEFGHIJKLMNPQRSTUVXYZ[`abcdefhijklmpqr";
    let mut out =
        b"Description survives.\n(This file must be converted with BinHex 4.0)\r\n:".to_vec();
    for bit in (0..bytes.len() * 8).step_by(6) {
        let mut value = 0usize;
        for n in bit..bit + 6 {
            value = (value << 1)
                | if n < bytes.len() * 8 {
                    usize::from((bytes[n / 8] >> (7 - n % 8)) & 1)
                } else {
                    0
                };
        }
        out.push(alphabet[value]);
    }
    out.extend_from_slice(b":\nTransport footer\n");
    out
}
fn hqx_plain(bytes: &[u8]) -> Vec<u8> {
    let escaped: Vec<_> = bytes
        .iter()
        .flat_map(|&v| if v == 0x90 { vec![v, 0] } else { vec![v] })
        .collect();
    hqx_rle(&escaped)
}
fn binhex_wire() -> (Vec<u8>, Vec<u8>) {
    let (fork, _) = nfnt::write(&read(&wire()), &options(Container::ResourceFork)).unwrap();
    let data = [0x90, 0x90, 0x90, 0, 0xff];
    let mut decoded = b"\x04Test\0FFILABCD\xa5\x5a".to_vec();
    decoded.extend_from_slice(&(data.len() as u32).to_be_bytes());
    decoded.extend_from_slice(&(fork.len() as u32).to_be_bytes());
    decoded.extend_from_slice(&binhex_crc(&decoded).to_be_bytes());
    decoded.extend_from_slice(&data);
    decoded.extend_from_slice(&binhex_crc(&data).to_be_bytes());
    decoded.extend_from_slice(&fork);
    decoded.extend_from_slice(&binhex_crc(&fork).to_be_bytes());
    (hqx_plain(&decoded), decoded)
}
#[test]
fn binhex_original_literal_wire_preserves_both_forks_and_finder_fields() {
    let (input, _) = binhex_wire();
    let s = read(&input);
    assert_eq!(
        s.properties.get("nfnt.binhex-header"),
        Some(&PropertyValue::Bytes(
            b"\x04Test\0FFILABCD\xa5\x5a".to_vec()
        ))
    );
    assert_eq!(
        s.properties.get("nfnt.data-fork"),
        Some(&PropertyValue::Bytes(vec![0x90, 0x90, 0x90, 0, 0xff]))
    );
    let (out, r) = nfnt::write(&s, &options(Container::BinHex)).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(read(&out), s);
    assert_eq!(nfnt::detect(&out), fontpixel::formats::Confidence::Certain);
}
#[test]
fn binhex_three_crcs_alphabet_truncation_and_rle_errors() {
    let (input, decoded) = binhex_wire();
    for at in [24, 31, decoded.len() - 1] {
        let mut bad = decoded.clone();
        bad[at] ^= 1;
        assert!(
            nfnt::read(&hqx_plain(&bad), &Default::default(), &Default::default()).is_err(),
            "CRC {at}"
        );
    }
    for malformed in [vec![0x90], vec![0x90, 4], vec![0, 0x90, 0]] {
        assert!(
            nfnt::read(
                &hqx_rle(&malformed),
                &Default::default(),
                &Default::default()
            )
            .is_err()
        );
    }
    let start = input.iter().position(|v| *v == b':').unwrap();
    let end = input.iter().rposition(|v| *v == b':').unwrap();
    for n in start + 1..=end {
        assert!(nfnt::read(&input[..n], &Default::default(), &Default::default()).is_err());
    }
    let mut bad = input;
    bad[start + 5] = b'~';
    assert!(nfnt::read(&bad, &Default::default(), &Default::default()).is_err());
}
#[test]
fn binhex_optional_exclamation_and_same_asset_segment_boundaries() {
    let (input, _) = binhex_wire();
    let end = input.iter().rposition(|v| *v == b':').unwrap();
    let mut old = input.clone();
    old.insert(end, b'!');
    assert_eq!(read(&old), read(&input));
    let start = input.iter().position(|v| *v == b':').unwrap();
    let mut segmented = input;
    segmented.splice(
        start + 41..start + 41,
        b"\n--- end of part 01 ---\nmail text ignored\n---\n"
            .iter()
            .copied(),
    );
    assert_eq!(read(&segmented).fonts, read(&old).fonts);
}
#[test]
fn binhex_declared_expansion_and_same_limit_output_contract() {
    let (_, mut decoded) = binhex_wire();
    decoded[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    let crc = binhex_crc(&decoded[..24]);
    decoded[24..26].copy_from_slice(&crc.to_be_bytes());
    assert!(
        nfnt::read(
            &hqx_plain(&decoded),
            &Default::default(),
            &ResourceLimits {
                max_total_bytes: 4096,
                ..Default::default()
            }
        )
        .is_err()
    );
    let (input, _) = binhex_wire();
    let s = read(&input);
    for max in [0, 1000, 4096, 10000, 100000] {
        let o = NfntWriteOptions {
            container: Container::BinHex,
            limits: ResourceLimits {
                max_total_bytes: max,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok((out, _)) = nfnt::write(&s, &o) {
            assert!(nfnt::read(&out, &Default::default(), &o.limits).is_ok());
        }
    }
}
