//! Original MIT RISC OS generation, phase, packed and scoped-metrics fixtures.
#![cfg(feature = "riscos")]
use fontpixel::formats::riscos::{
    self, RiscosCompression, RiscosDepth, RiscosMetricsVersion, RiscosReadOptions, RiscosVersion,
    RiscosWriteOptions,
};
use fontpixel::model::{Codepoint, FontSet, Property, PropertyValue, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{ResourceLimits, WriteOptions};
fn p16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn p32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn fixture(version: u8) -> Vec<u8> {
    let directory = if version == 8 { 64 } else { 16 };
    let first = if version == 8 { 100 } else { 64 };
    let flags = usize::from(version >= 7) * 4;
    let end = first + flags + 140;
    let mut b = vec![0; end];
    b[..4].copy_from_slice(b"FONT");
    b[4] = 1;
    b[5] = version;
    if version >= 7 {
        b[6] = 64;
    }
    for (at, n) in [
        (8, 65535),
        (10, 65535),
        (12, 9),
        (14, 3),
        (52, 10),
        (54, 192),
        (56, 72),
        (58, 192),
        (60, 72),
    ] {
        p16(&mut b, at, n);
    }
    if version == 8 {
        p32(&mut b, 16, 64);
        p32(&mut b, 20, 8);
    }
    for c in 0..9 {
        p32(
            &mut b,
            directory + c * 4,
            if c <= 2 { first as u32 } else { end as u32 },
        );
    }
    if flags != 0 {
        p32(&mut b, first, 0x80000000);
    }
    p32(&mut b, first + flags + 4, 128);
    let at = first + flags + 128;
    b[at..at + 5].copy_from_slice(&[2, 255, 255, 9, 3]);
    for y in 0..3 {
        for x in 0..9 {
            let ink = match y {
                0 => true,
                1 => x == 1,
                _ => x == 0 || x == 8,
            };
            if ink {
                let bit = y * 9 + x;
                b[at + 5 + bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    b
}
fn xy() -> Vec<u8> {
    let mut b = vec![0; 560];
    b[..4].copy_from_slice(&[12, 4, 90, 45]);
    p32(&mut b, 8, 20);
    p32(&mut b, 12, 540);
    for (at, n) in [(20, 12 * 16 * 90), (24, 12 * 16 * 45), (28, 90), (32, 45)] {
        p32(&mut b, at, n);
    }
    b[36..40].copy_from_slice(&[255, 255, 2, 1]);
    p16(&mut b, 40 + 65 * 2, 512);
    b[552..559].copy_from_slice(&[255, 255, 3, 2, 0x0f, 0, 0xff]);
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    let mut c = ReadContext::new(&i, l)?;
    Ok(riscos::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font").unwrap())
}
#[test]
fn every_font_generation_raw_and_packed_roundtrips() {
    for (version, tag) in [
        (RiscosVersion::V4, 4),
        (RiscosVersion::V5, 5),
        (RiscosVersion::V6, 6),
        (RiscosVersion::V7, 7),
        (RiscosVersion::V8, 8),
    ] {
        let s = read(&fixture(tag), &Default::default()).unwrap();
        let g = s.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap();
        assert_eq!(g.bitmap.as_bytes(), &[128, 128, 64, 0, 255, 128]);
        assert_eq!(g.metrics.bounds.x, -1);
        assert_eq!(g.metrics.bounds.y, -1);
        for compression in [RiscosCompression::Raw, RiscosCompression::Packed] {
            let v = RiscosWriteOptions {
                version,
                compression,
                ..Default::default()
            };
            let out = riscos::write(&s, &output(), &v)
                .unwrap()
                .output
                .into_single()
                .unwrap();
            let back = read(&out, &Default::default()).unwrap();
            let h = back.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap();
            assert_eq!(g.bitmap, h.bitmap, "{version:?}/{compression:?}");
            assert_eq!(g.metrics.bounds, h.metrics.bounds);
        }
    }
}
#[test]
fn old_xy_gray_bottom_up_nibbles_and_multistrike() {
    let mut s = read(&xy(), &Default::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[0, 255, 255, 255, 0, 0]);
    s.fonts.push(s.fonts[0].clone());
    let v = RiscosWriteOptions {
        version: RiscosVersion::Xy,
        depth: RiscosDepth::Gray4,
        ..Default::default()
    };
    let out = riscos::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default()).unwrap();
    assert_eq!(back.fonts.len(), 2);
    assert_eq!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[0, 255, 255, 255, 0, 0]
    );
}
#[test]
fn subpixel_grid_retains_all_sixteen_phases() {
    let base = read(&fixture(7), &Default::default()).unwrap();
    let mut s = FontSet::default();
    for x in 0..4 {
        for y in 0..4 {
            let mut f = base.fonts[0].clone();
            f.properties
                .set("riscos.phase_x", PropertyValue::Integer(x));
            f.properties
                .set("riscos.phase_y", PropertyValue::Integer(y));
            let id = f.glyph_id(Codepoint::Legacy(65)).unwrap();
            f.edit_glyph(id, |g| {
                g.metrics.bounds.x += x;
                g.metrics.bounds.y -= y;
                Ok(())
            })
            .unwrap();
            s.fonts.push(f);
        }
    }
    let v = RiscosWriteOptions {
        version: RiscosVersion::V7,
        ..Default::default()
    };
    let out = riscos::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default()).unwrap();
    assert_eq!(back.fonts.len(), 16);
    for (i, f) in back.fonts.iter().enumerate() {
        assert_eq!(
            f.glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap()
                .metrics
                .bounds,
            s.fonts[i]
                .glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap()
                .metrics
                .bounds
        );
    }
    assert!(
        read(
            &out,
            &ResourceLimits {
                max_glyphs: 15,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn signed_twelve_bit_coordinates_and_v8_extended_code() {
    let mut s = read(&fixture(8), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            let mut raw = vec![0; 51];
            raw[0] = 128;
            raw[50] = 64;
            g.bitmap = fontpixel::bitmap::Bitmap::<fontpixel::bitmap::Mono>::from_raw(
                130,
                3,
                &raw,
                Default::default(),
            )?
            .into();
            g.metrics.bounds = fontpixel::bitmap::Bounds::new(130, 3, -129, -130);
            g.codepoints = vec![Codepoint::Legacy(300)];
            Ok(())
        })
        .unwrap();
    let v = RiscosWriteOptions {
        version: RiscosVersion::V8,
        ..Default::default()
    };
    let out = riscos::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default()).unwrap();
    let g = back.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(300))
        .unwrap();
    assert_eq!(
        g.metrics.bounds,
        fontpixel::bitmap::Bounds::new(130, 3, -129, -130)
    );
    let expected = [0x7f, 0xef, 0xf7];
    assert!(out.windows(3).any(|w| w == expected));
}
#[test]
fn packed_repeat_count_adds_rows() {
    let mut b = fixture(6);
    b.truncate(200);
    for c in 3..9 {
        p32(&mut b, 16 + c * 4, 200);
    }
    b[192] = 0xc6;
    b[197..200].copy_from_slice(&[0x2e, 0x09, 0]);
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[255, 128, 255, 128, 255, 128]
    );
    b[197] = 0xfe;
    assert!(read(&b, &Default::default()).is_err());
}
struct Resources(Vec<(String, Vec<u8>)>);
impl ResourceResolver for Resources {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = &self
            .0
            .iter()
            .find(|(name, _)| name == p.as_str())
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .1;
        if b.len() > max {
            return Err(fontpixel::Error::InvalidInput(
                "test resource exceeds cap".into(),
            ));
        }
        Ok(b.clone())
    }
}
#[test]
fn metrics_v0_v2_and_scoped_master_resources() {
    let mut s = read(&fixture(6), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.scalable_advance = Some(Vector::new(500, 0));
            g.metrics.advance = Some(Vector::new(6, 0));
            g.metrics.right_bearing = Some(-2);
            Ok(())
        })
        .unwrap();
    for metrics_version in [RiscosMetricsVersion::V0, RiscosMetricsVersion::V2] {
        let path = ResourcePath::new("IntMetrics").unwrap();
        let v = RiscosWriteOptions {
            metrics_path: Some(path.clone()),
            metrics_version,
            ..Default::default()
        };
        let w = riscos::write(&s, &output(), &v).unwrap();
        let assets = w.output.into_assets();
        let data = assets
            .iter()
            .find(|a| a.path.as_str() == "font")
            .unwrap()
            .bytes
            .clone();
        let resources = Resources(
            assets
                .into_iter()
                .map(|a| (a.path.to_string(), a.bytes))
                .collect(),
        );
        let input = Input::new(&data).with_resources(&resources);
        let l = ResourceLimits::default();
        let mut c = ReadContext::new(&input, &l).unwrap();
        let back = riscos::read(
            &mut c,
            &RiscosReadOptions {
                metrics_path: Some(path),
                ..Default::default()
            },
        )
        .unwrap()
        .0;
        assert_eq!(
            back.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap()
                .metrics
                .advance,
            Some(Vector::new(6, 0))
        );
    }
    let resources = Resources(vec![("master".into(), fixture(6))]);
    let i = Input::new(b"master").with_resources(&resources);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&i, &l).unwrap();
    assert!(riscos::read(&mut c, &Default::default()).is_ok());
    let looped = Resources(vec![("loop".into(), b"loop".to_vec())]);
    let i = Input::new(b"loop").with_resources(&looped);
    let mut c = ReadContext::new(&i, &l).unwrap();
    assert!(riscos::read(&mut c, &Default::default()).is_err());
}
#[test]
fn every_truncation_offsets_and_undefined_outline() {
    let b = fixture(6);
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix{n}");
    }
    let mut b = b;
    b[4] = 0;
    assert!(read(&b, &Default::default()).is_err());
    b[4] = 1;
    p32(&mut b, 68, 127);
    assert!(read(&b, &Default::default()).is_err());
}
#[test]
fn budgets_same_limit_output_and_strict_metadata() {
    let b = fixture(6);
    for l in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: b.len(),
            ..Default::default()
        },
    ] {
        assert!(read(&b, &l).is_err());
    }
    let mut s = read(&b, &Default::default()).unwrap();
    let v = RiscosWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = riscos::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(read(&out, &v.limits).is_ok());
    s.fonts[0].properties.push(Property::new(
        "test.extra",
        PropertyValue::Text("metadata".into()),
    ));
    let mut o = output();
    o.strict = true;
    assert!(riscos::write(&s, &o, &v).is_err());
}
#[test]
fn metrics_scale_horizontal_and_vertical_sizes_independently() {
    let mut b = fixture(6);
    p16(&mut b, 54, 96);
    let mut metrics = vec![0; 332];
    metrics[..40].fill(13);
    metrics[..4].copy_from_slice(b"TEST");
    p32(&mut metrics, 40, 16);
    p32(&mut metrics, 44, 16);
    metrics[48] = 2;
    metrics[52 + 65] = 1;
    p16(&mut metrics, 326, 500);
    p16(&mut metrics, 330, 500);
    let resources = Resources(vec![("IntMetrics".into(), metrics)]);
    let input = Input::new(&b).with_resources(&resources);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    let s = riscos::read(
        &mut c,
        &RiscosReadOptions {
            metrics_path: Some(ResourcePath::new("IntMetrics").unwrap()),
            ..Default::default()
        },
    )
    .unwrap()
    .0;
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.metrics.advance, Some(Vector::new(3, 6)));
}
#[test]
fn detection_distinguishes_fontx2_and_requires_native_header() {
    use fontpixel::formats::Confidence;
    for version in 4..=8 {
        assert_eq!(riscos::detect(&fixture(version)), Confidence::Certain);
    }
    let mut fontx2 = vec![0; 17 + 256 * 16];
    fontx2[..6].copy_from_slice(b"FONTX2");
    fontx2[6..14].copy_from_slice(b"ORIGINAL");
    fontx2[14] = 8;
    fontx2[15] = 16;
    assert_eq!(riscos::detect(&fontx2), Confidence::NoMatch);
    let b = fixture(6);
    for n in 0..62 {
        assert_eq!(riscos::detect(&b[..n]), Confidence::NoMatch);
    }
    for (at, value) in [(4, 8), (5, 50), (6, 128), (52, 11), (56, 0)] {
        let mut b = b.clone();
        b[at] = value;
        assert_eq!(riscos::detect(&b), Confidence::NoMatch);
    }
}
