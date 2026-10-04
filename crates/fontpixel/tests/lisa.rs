//! Original Lisa directories, native rasters, and literal-only compression fixtures.
#![cfg(feature = "lisa")]
use fontpixel::formats::lisa::{self, Compression, LisaWriteOptions};
use fontpixel::{FontSet, ResourceLimits};
fn raster() -> Vec<u8> {
    vec![
        0x90, 0, 0, 65, 0, 66, 0, 3, 255, 255, 255, 255, 0, 3, 0, 3, 0, 12, 0, 2, 0, 1, 0, 1, 0, 1,
        0xb8, 0, 0xc8, 0, 0xb8, 0, 0, 0, 0, 2, 0, 3, 0, 5, 0, 3, 1, 2, 0, 3,
    ]
}
fn library(resource: &[u8]) -> Vec<u8> {
    let mut b = vec![0, 11, 0, 1, 3, b'F', b'.', b'F'];
    for n in [
        0u32,
        resource.len().div_ceil(2) as u32,
        resource.len().div_ceil(2) as u32,
        resource.len().div_ceil(2) as u32,
    ] {
        b.extend_from_slice(&n.to_be_bytes());
    }
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(resource);
    if b.len() % 2 != 0 {
        b.push(0);
    }
    b
}
fn packed() -> Vec<u8> {
    let mut r = raster();
    r[30] ^= r[28];
    r[28] ^= r[26];
    let n = r.len() - 2;
    let mut body = r[2..].to_vec();
    body.resize(n.div_ceil(8) * 8, 0);
    let mut payload = Vec::new();
    for (i, c) in body.chunks(8).enumerate() {
        let mut mask = 0u8;
        for (bit, &byte) in c.iter().enumerate() {
            if i < 3 && byte == 0 {
                mask |= 0x80 >> bit;
            } else {
                payload.push(byte);
            }
        }
        payload.push(mask);
    }
    let mut out = vec![0x90, 0x80];
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&(n as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    out
}
fn read(b: &[u8]) -> FontSet {
    lisa::read(b, &Default::default(), &Default::default())
        .unwrap()
        .0
}
#[test]
fn native_directory_without_mac_terminator_and_exact_replay() {
    let b = library(&raster());
    let s = read(&b);
    assert_eq!(s.fonts[0].len(), 3);
    assert_eq!(s.fonts[0].names.full.as_deref(), Some("F.F"));
    let (out, r) = lisa::write(&s, &Default::default()).unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn reverse_mixed_controls_and_xor_are_independently_authored() {
    let b = library(&packed());
    let s = read(&b);
    let plain = read(&library(&raster()));
    for ((_, g), (_, p)) in s.fonts[0].glyphs().zip(plain.fonts[0].glyphs()) {
        assert_eq!(g.bitmap, p.bitmap);
        assert_eq!(g.metrics, p.metrics);
    }
    let (out, r) = lisa::write(&s, &Default::default()).unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn writer_both_native_compression_modes_and_metrics_records() {
    let s = read(&library(&raster()));
    for compression in [Compression::Compressed, Compression::Uncompressed] {
        let (b, r) = lisa::write(
            &s,
            &LisaWriteOptions {
                compression,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.has_loss() || compression == Compression::Uncompressed);
        let parsed = read(&b);
        assert_eq!(parsed.fonts[0].len(), 3);
        for ((_, a), (_, z)) in s.fonts[0].glyphs().zip(parsed.fonts[0].glyphs()) {
            assert_eq!(a.bitmap, z.bitmap);
        }
        let (out, _) = lisa::write(
            &parsed,
            &LisaWriteOptions {
                compression,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out, b);
    }
}
#[test]
fn malformed_directory_compression_and_expansion_limits() {
    let b = library(&packed());
    for n in 0..b.len() - 1 {
        assert!(
            lisa::read(&b[..n], &Default::default(), &Default::default()).is_err(),
            "prefix {n}"
        );
    }
    let mut malformed = b.clone();
    malformed[1] = 1;
    assert!(lisa::read(&malformed, &Default::default(), &Default::default()).is_err());
    let mut exploding = packed();
    exploding[6..10].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(
        lisa::read(
            &library(&exploding),
            &Default::default(),
            &ResourceLimits {
                max_total_bytes: 1 << 20,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut bad = packed();
    bad[2..6].copy_from_slice(&1u32.to_be_bytes());
    assert!(lisa::read(&library(&bad), &Default::default(), &Default::default()).is_err());
}
#[test]
fn writer_success_is_readable_under_same_cumulative_budget() {
    let s = read(&library(&raster()));
    for max in [0, 1000, 4096, 10000, 100000] {
        let o = LisaWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: max,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok((b, _)) = lisa::write(&s, &o) {
            assert!(lisa::read(&b, &Default::default(), &o.limits).is_ok());
        }
    }
}

#[test]
fn native_in_place_overlap_is_rejected_and_default_writer_can_use_plain() {
    let raw = raster();
    let mut body = raw[2..].to_vec();
    let length = body.len();
    body.resize(length.div_ceil(8) * 8, 0);
    let mut payload = Vec::new();
    for block in body.chunks(8) {
        payload.extend_from_slice(block);
        payload.push(0);
    }
    let mut invalid = vec![0x90, 0x80];
    invalid.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    invalid.extend_from_slice(&(length as u32).to_be_bytes());
    invalid.extend_from_slice(&payload);
    assert!(lisa::read(&library(&invalid), &Default::default(), &Default::default()).is_err());
    let mut font = fontpixel::Font::default();
    let mut glyph = fontpixel::model::Glyph::new(
        fontpixel::Bitmap::<fontpixel::bitmap::Mono>::from_raw(
            200,
            200,
            &(0..5000)
                .map(|n| if n / 25 % 2 == 0 { 255 } else { 85 })
                .collect::<Vec<_>>(),
            Default::default(),
        )
        .unwrap(),
    );
    glyph
        .codepoints
        .push(fontpixel::model::Codepoint::Legacy(65));
    font.add_glyph(glyph).unwrap();
    let set = FontSet::from(font);
    assert!(
        lisa::write(
            &set,
            &LisaWriteOptions {
                compression: Compression::Compressed,
                ..Default::default()
            }
        )
        .is_err()
    );
    let (plain, _) = lisa::write(&set, &Default::default()).unwrap();
    let end = 4 + usize::from(u16::from_be_bytes([plain[0], plain[1]])) * 2;
    assert_eq!(plain[end + 1] & 0x80, 0);
    let limits = ResourceLimits {
        max_decompressed_bytes: 0,
        ..Default::default()
    };
    assert!(lisa::read(&plain, &Default::default(), &limits).is_ok());
    assert!(lisa::read(&library(&packed()), &Default::default(), &limits).is_err());
}

#[test]
fn strict_registry_rejects_unrepresented_metadata_and_removed_slots() {
    let mut set = read(&library(&raster()));
    set.fonts[0]
        .properties
        .push(fontpixel::model::Property::new(
            "application-note",
            "must report",
        ));
    let id = set.fonts[0]
        .add_glyph(fontpixel::model::Glyph::default())
        .unwrap();
    set.fonts[0].remove_glyph(id);
    let mut options =
        fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("out.lisa").unwrap());
    options.strict = true;
    assert!(matches!(
        fontpixel::write(&set, "lisa", &options),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}

#[test]
fn shared_compressed_ranges_charge_each_expansion_and_keep_opaque_entries() {
    let mut compressed = packed();
    if !compressed.len().is_multiple_of(2) {
        compressed.push(0);
    }
    let end = (compressed.len() / 2) as u32;
    let mut data = vec![0, 30, 0, 3];
    for (name, offsets) in [
        (b"A.F", [0, end, end, end]),
        (b"B.F", [0, end, end, end]),
        (b"X.P", [end, end + 2, end + 2, end + 2]),
    ] {
        data.push(3);
        data.extend_from_slice(name);
        for value in offsets {
            data.extend_from_slice(&value.to_be_bytes());
        }
    }
    data.extend_from_slice(&compressed);
    data.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    let (mut set, read_report) =
        lisa::read(&data, &Default::default(), &Default::default()).unwrap();
    assert_eq!(set.fonts.len(), 2);
    assert!(read_report.has_loss());
    let opaque = set.properties.get("lisa.opaque").unwrap().clone();
    let (exact, report) = lisa::write(&set, &Default::default()).unwrap();
    assert_eq!(exact, data);
    assert!(report.has_loss());
    set.properties.remove("lisa.source");
    let (canonical, report) = lisa::write(&set, &Default::default()).unwrap();
    assert!(report.has_loss());
    assert_eq!(
        read(&canonical).properties.get("lisa.opaque"),
        Some(&opaque)
    );
    let limits = ResourceLimits {
        max_decompressed_bytes: 50,
        ..Default::default()
    };
    assert!(lisa::read(&library(&packed()), &Default::default(), &limits).is_ok());
    assert!(lisa::read(&data, &Default::default(), &limits).is_err());
}
