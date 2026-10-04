//! Original MIT ChiWriter generation and compatible-directory fixtures.
#![cfg(feature = "chiwriter")]
use fontpixel::formats::chiwriter::{
    self, ChiwriterLayout, ChiwriterReadOptions, ChiwriterVersion, ChiwriterWriteOptions,
};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture(v4: bool) -> Vec<u8> {
    let mut b = vec![0; 356];
    b[0] = if v4 { 17 } else { 16 };
    b[1..14].copy_from_slice(b"ORIGINAL.SET ");
    b[15] = 2;
    b[16] = 2;
    b[17] = 33;
    b[19] = 9;
    b[20] = 3;
    b[22] = 3;
    if !v4 {
        b[24] = 3;
    }
    let widths = if v4 { 153 } else { 250 };
    b[widths] = 9;
    b[widths + 1] = 5;
    b[344..].copy_from_slice(&[128, 128, 64, 0, 255, 128, 0xf8, 0, 0x80, 0, 0x08, 0]);
    b
}
fn read(b: &[u8], l: &ResourceLimits, o: &ChiwriterReadOptions) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(chiwriter::read(&mut c, o)?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.set").unwrap())
}
#[test]
fn generations_pixels_and_native_writes() {
    for version in [ChiwriterVersion::V3, ChiwriterVersion::V4] {
        let b = fixture(version == ChiwriterVersion::V4);
        let s = read(&b, &Default::default(), &Default::default()).unwrap();
        assert_eq!(s.fonts[0].len(), 3);
        let g = s.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(66))
            .unwrap();
        assert_eq!(g.bitmap.width(), 5);
        assert_eq!(g.bitmap.as_bytes(), &[0xf8, 0x80, 0x08]);
        assert_eq!(g.metrics.bounds.y, -1);
        let v = ChiwriterWriteOptions {
            version,
            ..Default::default()
        };
        let out = chiwriter::write(&s, &output(), &v)
            .unwrap()
            .output
            .into_single()
            .unwrap();
        let back = read(&out, &Default::default(), &Default::default()).unwrap();
        assert_eq!(
            g.bitmap,
            back.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(66))
                .unwrap()
                .bitmap
        );
    }
}
#[test]
fn explicit_offsets_stride_aliasing_and_reorder() {
    let s = read(&fixture(false), &Default::default(), &Default::default()).unwrap();
    for layout in [
        ChiwriterLayout::AbsoluteOffsets,
        ChiwriterLayout::RelativeOffsets,
    ] {
        let v = ChiwriterWriteOptions {
            version: ChiwriterVersion::V3,
            layout,
            stride: Some(3),
            ..Default::default()
        };
        let mut b = chiwriter::write(&s, &output(), &v)
            .unwrap()
            .output
            .into_single()
            .unwrap();
        let o = ChiwriterReadOptions {
            layout,
            stride: Some(3),
            ..Default::default()
        };
        assert!(read(&b, &Default::default(), &o).is_ok());
        let first = [b[61], b[62]];
        b[63..65].copy_from_slice(&first);
        assert!(read(&b, &Default::default(), &o).is_ok());
    }
}
#[test]
fn clipped_ink_is_diagnosed_and_retained() {
    let mut b = fixture(true);
    b[351] = 0x80;
    let input = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    let (s, r) = chiwriter::read(&mut c, &Default::default()).unwrap();
    assert!(r.has_loss());
    let out = chiwriter::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(out[351], 0x80);
}
#[test]
fn every_truncation_and_bad_directory() {
    let b = fixture(true);
    for n in 0..b.len() {
        assert!(
            read(&b[..n], &Default::default(), &Default::default()).is_err(),
            "prefix{n}"
        );
    }
    let mut b = b;
    b[17] = 250;
    assert!(read(&b, &Default::default(), &Default::default()).is_err());
}
#[test]
fn budgets_and_same_limit_write() {
    let b = fixture(true);
    for l in [
        ResourceLimits {
            max_glyphs: 2,
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
        assert!(read(&b, &l, &Default::default()).is_err());
    }
    let s = read(&b, &Default::default(), &Default::default()).unwrap();
    let v = ChiwriterWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = chiwriter::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(read(&out, &v.limits, &Default::default()).is_ok());
}
#[test]
fn strict_and_invalid_compatibility() {
    let mut s = read(&fixture(true), &Default::default(), &Default::default()).unwrap();
    s.fonts[0].names.copyright = Some("test".into());
    let mut o = output();
    o.strict = true;
    assert!(chiwriter::write(&s, &o, &Default::default()).is_err());
    assert!(
        chiwriter::write(
            &s,
            &output(),
            &ChiwriterWriteOptions {
                layout: ChiwriterLayout::RelativeOffsets,
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn new_font_default_filename_is_bounded_in_both_versions() {
    let mut s = read(&fixture(true), &Default::default(), &Default::default()).unwrap();
    s.fonts[0].properties = Default::default();
    for version in [ChiwriterVersion::V3, ChiwriterVersion::V4] {
        let v = ChiwriterWriteOptions {
            version,
            baseline: Some(2),
            stride: Some(2),
            ..Default::default()
        };
        let out = chiwriter::write(&s, &output(), &v)
            .unwrap()
            .output
            .into_single()
            .unwrap();
        assert_eq!(&out[1..14], b"FONT    .SET ");
        assert!(
            read(
                &out,
                &Default::default(),
                &ChiwriterReadOptions {
                    stride: Some(2),
                    ..Default::default()
                }
            )
            .is_ok()
        );
    }
}
#[test]
fn detection_requires_usable_records_and_does_not_claim_qpf_header() {
    use fontpixel::formats::Confidence;
    for v4 in [false, true] {
        let mut b = fixture(v4);
        assert_eq!(chiwriter::detect(&b), Confidence::Possible);
        b.push(0);
        assert_eq!(chiwriter::detect(&b), Confidence::NoMatch);
    }
    let mut qpf = vec![0; 6159];
    qpf[..25].copy_from_slice(&[
        16, 0, 0, 0, 8, 0, 0, 1, 1, 0, 1, 0, 1, 255, 0, 1, 8, 16, 0, 0, 8, 16, 0, 1, 8,
    ]);
    assert_eq!(chiwriter::detect(&qpf), Confidence::NoMatch);
    let s = read(&fixture(false), &Default::default(), &Default::default()).unwrap();
    for layout in [
        ChiwriterLayout::AbsoluteOffsets,
        ChiwriterLayout::RelativeOffsets,
    ] {
        let b = chiwriter::write(
            &s,
            &output(),
            &ChiwriterWriteOptions {
                version: ChiwriterVersion::V3,
                layout,
                stride: Some(3),
                ..Default::default()
            },
        )
        .unwrap()
        .output
        .into_single()
        .unwrap();
        assert_eq!(chiwriter::detect(&b), Confidence::Possible);
    }
}
