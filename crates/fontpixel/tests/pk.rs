//! Independent wire fixtures and bounded codec roundtrips.
#![cfg(feature = "pk")]
use fontpixel::formats::pk::{self, PkWriteOptions};
use fontpixel::model::{Codepoint, Property, PropertyValue};
use fontpixel::{Error, ResourceLimits};
fn pre() -> Vec<u8> {
    let mut v = vec![247, 89, 3, b'p', b'k', b'!'];
    for n in [10u32 << 20, 0x12345678, 65536, 131072] {
        v.extend_from_slice(&n.to_be_bytes())
    }
    v
}
fn short(flag: u8, w: u8, h: u8, raster: &[u8]) -> Vec<u8> {
    let mut v = pre();
    v.extend_from_slice(&[
        flag,
        (8 + raster.len()) as u8,
        65,
        8,
        0,
        0,
        4,
        w,
        h,
        1,
        h.saturating_sub(1),
    ]);
    v.extend_from_slice(raster);
    v.push(245);
    v
}
#[test]
fn independent_raw_and_packed() {
    let l = ResourceLimits::default();
    for raw in [
        short(224, 3, 3, &[0xf7, 0x80]),
        short(216, 3, 3, &[0x41, 0x40]),
    ] {
        let (f, r) = pk::read(&raw, &l).unwrap();
        assert!(!r.has_loss());
        let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
        assert_eq!(g.bitmap.as_bytes(), &[0xe0, 0xa0, 0xe0]);
        assert_eq!(g.metrics.bounds.x, -1);
        assert_eq!(g.metrics.bounds.y, 0);
        assert_eq!(f.metrics.resolution.unwrap().x, 72);
        assert_eq!(f.metrics.resolution.unwrap().y, 145);
        let (out, r) = pk::write(&f, &PkWriteOptions::default()).unwrap();
        assert!(!r.has_loss(), "{r}");
        assert_eq!(pk::read(&out, &l).unwrap().0, f);
    }
}
#[test]
fn repeat_rows_and_bounds() {
    let l = ResourceLimits::default();
    let raw = short(216, 3, 3, &[0xe2, 0x30]);
    let (f, _) = pk::read(&raw, &l).unwrap();
    assert_eq!(f.glyphs().next().unwrap().1.bitmap.as_bytes(), &[0xe0; 3]);
    for data in [
        &[0xe3, 0x30][..],
        &[0xee, 0x30],
        &[0x00; 32],
        &[0xf3, 0x40],
        &[0x41, 0x41],
    ] {
        assert!(pk::read(&short(216, 3, 3, data), &l).is_err())
    }
}
#[test]
fn extended_and_long_packets() {
    let mut v = pre();
    v.extend_from_slice(&[
        228, 0, 15, 201, 0, 0, 0, 0, 4, 0, 9, 0, 1, 0xff, 0xff, 0, 0, 0xff, 0x80, 245,
    ]);
    let (f, _) = pk::read(&v, &ResourceLimits::default()).unwrap();
    let g = f.glyphs().next().unwrap().1;
    assert_eq!(g.bitmap.width(), 9);
    assert_eq!(g.metrics.bounds.x, 1);
    assert_eq!(g.bitmap.as_bytes(), &[255, 128]);
    let (out, _) = pk::write(&f, &PkWriteOptions::default()).unwrap();
    assert_eq!(pk::read(&out, &ResourceLimits::default()).unwrap().0, f);
}
#[test]
fn all_truncations_and_limits() {
    let raw = short(224, 3, 3, &[0xf7, 0x80]);
    for n in 0..raw.len() {
        assert!(
            pk::read(&raw[..n], &ResourceLimits::default()).is_err(),
            "length {n}"
        );
    }
    for l in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 8,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 9,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 100,
            ..Default::default()
        },
    ] {
        assert!(pk::read(&raw, &l).is_err())
    }
}
#[test]
fn properties_aliases_slots_and_fatal_report() {
    let (mut f, _) =
        pk::read(&short(224, 3, 3, &[0xf7, 0x80]), &ResourceLimits::default()).unwrap();
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(66));
        g.properties.push(Property {
            name: "custom".into(),
            value: PropertyValue::Text("kept?".into()),
            raw: Some(vec![1]),
        });
        Ok(())
    })
    .unwrap();
    let (_, r) = pk::write(&f, &PkWriteOptions::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "pk.mapping"));
    assert!(r.entries.iter().any(|e| e.code == "pk.property"));
    f.edit_glyph(id, |g| {
        g.codepoints.clear();
        Ok(())
    })
    .unwrap();
    let e = pk::write(&f, &PkWriteOptions::default()).unwrap_err();
    assert!(matches!(e, Error::ReportedErrors(_)));
    assert!(
        e.report()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.code == "pk.unencoded")
    );
}
#[test]
fn special_roundtrip() {
    let mut v = pre();
    v.extend_from_slice(&[240, 3, 0, 255, 1, 244, 255, 255, 255, 255]);
    v.extend_from_slice(&short(224, 3, 3, &[0xf7, 0x80])[22..]);
    let (f, _) = pk::read(&v, &ResourceLimits::default()).unwrap();
    let (out, r) = pk::write(&f, &PkWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(pk::read(&out, &ResourceLimits::default()).unwrap().0, f);
}
#[test]
fn generated_bit_alignment_roundtrip() {
    for w in 0usize..=19 {
        for h in 0..=7 {
            let n = (w * h).div_ceil(8usize);
            let raw: Vec<u8> = (0..n).map(|i| ((i * 73 + 51) & 255) as u8).collect();
            let (f, _) = pk::read(
                &short(224, w as u8, h as u8, &raw),
                &ResourceLimits::default(),
            )
            .unwrap();
            let (out, r) = pk::write(&f, &PkWriteOptions::default()).unwrap();
            assert!(!r.has_loss(), "w {w} h {h}: {r}");
            assert_eq!(pk::read(&out, &ResourceLimits::default()).unwrap().0, f);
        }
    }
}

#[test]
fn full_extended_advance_and_fractional_long_metrics() {
    let mut v = pre();
    v.extend_from_slice(&[
        228, 0, 14, 7, 0, 0, 0, 255, 255, 0, 1, 0, 1, 0, 0, 0, 0, 128, 245,
    ]);
    let (f, _) = pk::read(&v, &ResourceLimits::default()).unwrap();
    assert_eq!(
        f.glyphs().next().unwrap().1.metrics.advance.unwrap().x,
        65535
    );
    let (out, r) = pk::write(&f, &PkWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(pk::read(&out, &ResourceLimits::default()).unwrap().0, f);
    let mut v = pre();
    v.push(231);
    v.extend_from_slice(&29u32.to_be_bytes());
    v.extend_from_slice(&0x12345u32.to_be_bytes());
    for n in [-262144i32, 98304, -32768, 1, 1, -1, 2] {
        v.extend_from_slice(&n.to_be_bytes());
    }
    v.extend_from_slice(&[128, 245]);
    let (f, _) = pk::read(&v, &ResourceLimits::default()).unwrap();
    let g = f.glyphs().next().unwrap().1;
    assert_eq!(
        g.metrics.advance.unwrap(),
        fontpixel::model::Vector::new(2, -1)
    );
    assert_eq!(g.metrics.scalable_advance.unwrap().x, -250);
    let (out, r) = pk::write(&f, &PkWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(pk::read(&out, &ResourceLimits::default()).unwrap().0, f);
}

#[test]
fn same_limit_successful_writes_reread() {
    let (f, _) = pk::read(&short(216, 3, 3, &[0xe2, 0x30]), &ResourceLimits::default()).unwrap();
    for total in [8192, 16384, 32768, 65536] {
        let o = PkWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: total,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok((b, _)) = pk::write(&f, &o) {
            assert!(pk::read(&b, &o.limits).is_ok());
        }
    }
}

#[test]
fn late_fatal_error_retains_prior_omissions() {
    let (mut f, _) =
        pk::read(&short(224, 3, 3, &[0xf7, 0x80]), &ResourceLimits::default()).unwrap();
    let id = f.glyphs().next().unwrap().0;
    let mut unencoded = f.glyph(id).unwrap().clone();
    unencoded.codepoints.clear();
    f.add_glyph(unencoded).unwrap();
    f.edit_glyph(id, |g| {
        g.metrics.bounds.x = i64::MIN;
        Ok(())
    })
    .unwrap();
    let error = pk::write(&f, &PkWriteOptions::default()).unwrap_err();
    let report = error.report().unwrap();
    assert!(report.entries.iter().any(|e| e.code == "pk.unencoded"));
    assert!(report.entries.iter().any(|e| e.code == "pk.write_failed"));
}

#[test]
fn source_provenance_and_escaped_properties_are_preflighted() {
    use fontpixel::formats::{FormatWriter, WriteOptions};
    use fontpixel::resources::ResourcePath;
    let (mut f, _) =
        pk::read(&short(224, 3, 3, &[0xf7, 0x80]), &ResourceLimits::default()).unwrap();
    let options = PkWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 65536,
            ..Default::default()
        },
        ..Default::default()
    };
    f.source_format = Some(String::with_capacity(100_000));
    assert!(pk::write(&f, &options).is_err());
    f.source_format = Some("pk".into());
    let mut set = fontpixel::FontSet::from(f.clone());
    set.source_format = Some(String::with_capacity(100_000));
    let codec = pk::PkCodec {
        write_options: options.clone(),
    };
    let common = WriteOptions::new(ResourcePath::new("f.pk").unwrap());
    assert!(codec.write(&set, &common).is_err());
    f.properties
        .push(Property::new("\x01".repeat(5000), PropertyValue::Flag));
    assert!(pk::write(&f, &options).is_err());
}

#[test]
fn alias_diagnostics_grow_linearly() {
    let (mut f, _) =
        pk::read(&short(224, 3, 3, &[0xf7, 0x80]), &ResourceLimits::default()).unwrap();
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints = (0..1000).map(Codepoint::Legacy).collect();
        Ok(())
    })
    .unwrap();
    let (bytes, report) = pk::write(&f, &PkWriteOptions::default()).unwrap();
    assert!(report.entries.len() < 4100);
    assert_eq!(
        pk::read(&bytes, &ResourceLimits::default())
            .unwrap()
            .0
            .len(),
        1000
    );
}
