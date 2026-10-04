//! Original MIT prebuilt structures, endian masks and vertical vectors.
#![cfg(feature = "prebuilt")]
use fontpixel::formats::prebuilt::{
    self, PrebuiltByteOrder, PrebuiltDepth, PrebuiltReadOptions, PrebuiltWriteOptions,
};
use fontpixel::model::{FontSet, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture(little: bool, depth: u32, vertical: bool) -> Vec<u8> {
    let mut b = vec![0; 110 + if vertical { 12 } else { 0 } + 2 * (3 * depth as usize).div_ceil(8)];
    let p16 = |b: &mut [u8], at: usize, n: u16| {
        b[at..at + 2].copy_from_slice(&if little {
            n.to_le_bytes()
        } else {
            n.to_be_bytes()
        })
    };
    let p32 = |b: &mut [u8], at: usize, n: u32| {
        b[at..at + 4].copy_from_slice(&if little {
            n.to_le_bytes()
        } else {
            n.to_be_bytes()
        })
    };
    p16(&mut b, 0, if little { 0 } else { 1 });
    p16(&mut b, 2, 3);
    p32(&mut b, 4, 42);
    p32(&mut b, 8, 52);
    p32(&mut b, 12, 57);
    p16(&mut b, 16, 1);
    p16(&mut b, 18, 1);
    p32(&mut b, 20, 62);
    p32(&mut b, 24, 60);
    p32(&mut b, 28, 2);
    p32(&mut b, 32, 70);
    p32(&mut b, 36, 98);
    if vertical {
        p32(&mut b, 40, 110);
        p32(&mut b, 44, 118);
    }
    p32(&mut b, 48, (2 * (3 * depth as usize).div_ceil(8)) as u32);
    b[52..57].copy_from_slice(b"TEST\0");
    b[57..60].copy_from_slice(b"CS\0");
    b[60..62].copy_from_slice(b"A\0");
    p32(&mut b, 62, 32768);
    p32(&mut b, 70, 12 * 65536);
    p32(&mut b, 82, 12 * 65536);
    p32(&mut b, 94, depth);
    b[98..104].copy_from_slice(&[3, 2, 1, 1, 4, 1]);
    let start = 110 + if vertical { 12 } else { 0 };
    p32(&mut b, 106, start as u32);
    if vertical {
        p32(&mut b, 110, 0);
        p32(&mut b, 114, (-65536i32) as u32);
        b[118..122].copy_from_slice(&[2, 3, 0, 252]);
    }
    let row = (3 * depth as usize).div_ceil(8);
    let max = (1u16 << depth) - 1;
    for (y, values) in [[0, max, max], [max, 0, 0]].into_iter().enumerate() {
        for (x, v) in values.into_iter().enumerate() {
            let bit = x * depth as usize;
            let shift = 8 - depth as usize - bit % 8;
            b[start + y * row + bit / 8] |= (v as u8) << shift;
        }
    }
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    let mut c = ReadContext::new(&i, l)?;
    Ok(prebuilt::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.bepf").unwrap())
}
#[test]
fn both_orders_and_all_native_depths() {
    for little in [false, true] {
        for depth in [1, 2, 4, 8] {
            let s = read(&fixture(little, depth, false), &Default::default()).unwrap();
            let g = s.fonts[0].glyphs().next().unwrap().1;
            assert_eq!(g.name.as_deref(), Some("A"));
            assert_eq!(g.metrics.bounds.x, -1);
            assert_eq!(g.metrics.bounds.y, -1);
            assert_eq!(g.metrics.advance, Some(Vector::new(4, 1)));
            if depth == 1 {
                assert_eq!(g.bitmap.as_bytes(), &[0x60, 0x80]);
            } else {
                assert_eq!(g.bitmap.as_bytes(), &[0, 255, 255, 255, 0, 0]);
            }
            let v = PrebuiltWriteOptions {
                byte_order: if little {
                    PrebuiltByteOrder::Little
                } else {
                    PrebuiltByteOrder::Big
                },
                ..Default::default()
            };
            let out = prebuilt::write(&s, &output(), &v)
                .unwrap()
                .output
                .into_single()
                .unwrap();
            let back = read(&out, &Default::default()).unwrap();
            assert_eq!(g.bitmap, back.fonts[0].glyphs().next().unwrap().1.bitmap);
        }
    }
}
#[test]
fn vertical_vectors_preserved_and_diagnosed() {
    let b = fixture(false, 1, true);
    let i = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&i, &l).unwrap();
    let (s, r) = prebuilt::read(&mut c, &Default::default()).unwrap();
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "prebuilt.vertical_interpretation")
    );
    let g = s.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(g.metrics.vertical_origin, Some(Vector::new(1, -2)));
    assert_eq!(g.metrics.vertical_advance, Some(Vector::new(0, -4)));
    assert_eq!(
        g.metrics.vertical_scalable_advance,
        Some(Vector::new(0, -1000))
    );
    let out = prebuilt::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default()).unwrap();
    assert_eq!(
        g.metrics.vertical_origin,
        back.fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .metrics
            .vertical_origin
    );
}
#[test]
fn multiple_matrices_and_aggregate_limits() {
    let mut s = read(&fixture(false, 1, false), &Default::default()).unwrap();
    s.fonts.push(s.fonts[0].clone());
    let out = prebuilt::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(read(&out, &Default::default()).unwrap().fonts.len(), 2);
    assert!(
        read(
            &out,
            &ResourceLimits {
                max_glyphs: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn every_truncation_bad_offsets_and_version() {
    let b = fixture(false, 1, false);
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix{n}");
    }
    let mut b = b;
    b[3] = 4;
    assert!(read(&b, &Default::default()).is_err());
    b[3] = 3;
    b[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&b, &Default::default()).is_err());
}
#[test]
fn gray_quantization_requires_explicit_option() {
    let mut s = read(&fixture(false, 8, false), &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.bitmap = fontpixel::bitmap::Bitmap::<fontpixel::bitmap::Gray8>::from_raw(
                3,
                2,
                &[0, 100, 255, 255, 0, 0],
                Default::default(),
            )?
            .into();
            Ok(())
        })
        .unwrap();
    let mut v = PrebuiltWriteOptions {
        depth: Some(PrebuiltDepth::Gray2),
        ..Default::default()
    };
    assert!(prebuilt::write(&s, &output(), &v).is_err());
    v.quantize = true;
    assert!(
        prebuilt::write(&s, &output(), &v)
            .unwrap()
            .report
            .has_loss()
    );
    v.depth = Some(PrebuiltDepth::Mono);
    assert!(prebuilt::write(&s, &output(), &v).is_err());
    v.gray_threshold = Some(128);
    assert!(prebuilt::write(&s, &output(), &v).is_ok());
}
#[test]
fn budgets_output_readability_and_requested_order() {
    let b = fixture(false, 1, false);
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
    let s = read(&b, &Default::default()).unwrap();
    let v = PrebuiltWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = prebuilt::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(read(&out, &v.limits).is_ok());
    let i = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&i, &l).unwrap();
    assert!(
        prebuilt::read(
            &mut c,
            &PrebuiltReadOptions {
                byte_order: Some(PrebuiltByteOrder::Little),
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn extreme_vertical_origin_returns_error() {
    let mut s = read(&fixture(false, 1, true), &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.vertical_origin = Some(Vector::new(i64::MAX, i64::MIN));
            Ok(())
        })
        .unwrap();
    assert!(prebuilt::write(&s, &output(), &Default::default()).is_err());
}
#[test]
fn gray8_preserves_all_sample_bits() {
    let mut b = fixture(false, 8, false);
    let end = b.len();
    b[end - 6..].copy_from_slice(&[0, 17, 255, 128, 33, 1]);
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        s.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
        &[0, 17, 255, 128, 33, 1]
    );
    let out = prebuilt::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(&out[out.len() - 6..], &[0, 17, 255, 128, 33, 1]);
}
#[test]
fn little_endian_bitmap_is_native_msb_and_lsb_is_explicit() {
    let mut b = fixture(true, 8, false);
    let end = b.len();
    b[end - 6..].copy_from_slice(&[0, 17, 34, 119, 136, 255]);
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        s.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
        &[0, 17, 34, 119, 136, 255]
    );
    let mono = read(&fixture(true, 1, false), &Default::default()).unwrap();
    let out = prebuilt::write(
        &mono,
        &output(),
        &PrebuiltWriteOptions {
            byte_order: PrebuiltByteOrder::Little,
            bitmap_bit_order: prebuilt::PrebuiltBitOrder::LsbFirst,
            ..Default::default()
        },
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    assert_eq!(&out[out.len() - 2..], &[6, 1]);
    let input = Input::new(&out);
    let limits = ResourceLimits::default();
    let mut context = ReadContext::new(&input, &limits).unwrap();
    let back = prebuilt::read(
        &mut context,
        &PrebuiltReadOptions {
            bitmap_bit_order: prebuilt::PrebuiltBitOrder::LsbFirst,
            ..Default::default()
        },
    )
    .unwrap()
    .0;
    assert_eq!(
        mono.fonts[0].glyphs().next().unwrap().1.bitmap,
        back.fonts[0].glyphs().next().unwrap().1.bitmap
    );
}
#[test]
fn original_producer_little_endian_thirteen_pixel_rows() {
    let mut b = fixture(true, 1, false);
    b.resize(114, 0);
    b[48..52].copy_from_slice(&4u32.to_le_bytes());
    b[98] = 13;
    b[100] = 2;
    b[102] = 14;
    b[110..114].copy_from_slice(&[0x96, 0x88, 0x61, 0x20]);
    let s = read(&b, &Default::default()).unwrap();
    let g = s.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(g.bitmap.width(), 13);
    assert_eq!(g.bitmap.as_bytes(), &[0x96, 0x88, 0x61, 0x20]);
    assert_eq!(g.metrics.advance, Some(Vector::new(14, 1)));
    let out = prebuilt::write(
        &s,
        &output(),
        &PrebuiltWriteOptions {
            byte_order: PrebuiltByteOrder::Little,
            ..Default::default()
        },
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    assert_eq!(&out[out.len() - 4..], &[0x96, 0x88, 0x61, 0x20]);
}
