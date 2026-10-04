//! Original byte-authored vfont fixtures (project MIT license).
#![cfg(feature = "vfont")]
use fontpixel::bitmap::ByteOrder;
use fontpixel::formats::vfont::{self, VfontReadOptions, VfontWriteOptions};
use fontpixel::model::{Codepoint, FontSet, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn p16(b: &mut [u8], at: usize, v: u16, e: ByteOrder) {
    b[at..at + 2].copy_from_slice(&if e == ByteOrder::Little {
        v.to_le_bytes()
    } else {
        v.to_be_bytes()
    });
}
fn fixture(e: ByteOrder) -> Vec<u8> {
    let mut b = vec![0; 2573];
    p16(&mut b, 0, 0o436, e);
    p16(&mut b, 2, 3, e);
    p16(&mut b, 4, 3, e);
    p16(&mut b, 6, 3, e);
    p16(&mut b, 8, 7, e);
    let at = 10 + 65 * 10;
    p16(&mut b, at + 2, 3, e);
    b[at + 4..at + 8].copy_from_slice(&[2, 1, 1, 2]);
    p16(&mut b, at + 8, 5, e);
    b[2570..].copy_from_slice(&[0xa0, 0x40, 0xe0]);
    let space = 10 + 32 * 10;
    p16(&mut b, space + 8, 3, e);
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(vfont::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("test.vfont").unwrap())
}
#[test]
fn both_orders_signed_metrics_and_derived_font_metrics() {
    for e in [ByteOrder::Little, ByteOrder::Big] {
        let b = fixture(e);
        let s = read(&b, &Default::default()).unwrap();
        let f = s.single().unwrap();
        assert_eq!(f.len(), 2);
        let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
        assert_eq!(g.bitmap.as_bytes(), [0xa0, 0x40, 0xe0]);
        assert_eq!(g.metrics.bounds.x, -1);
        assert_eq!(g.metrics.bounds.y, -1);
        assert_eq!(g.metrics.advance, Some(Vector::new(5, 0)));
        assert_eq!(g.metrics.left_bearing, Some(-1));
        assert_eq!(g.metrics.right_bearing, Some(3));
        assert_eq!(f.metrics.ascent, Some(2));
        assert_eq!(f.metrics.descent, Some(1));
        assert_eq!(f.metrics.line_height, Some(3));
        assert_eq!(f.metrics.pixel_size, Some(3));
        assert_eq!(f.metrics.space_width, Some(3));
        let out = vfont::write(
            &s,
            &opts(),
            &VfontWriteOptions {
                byte_order: e,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!out.report.has_loss(), "{}", out.report);
        assert_eq!(out.output.into_single().unwrap(), b);
        let input = Input::new(&b);
        let l = ResourceLimits::default();
        let mut c = ReadContext::new(&input, &l).unwrap();
        assert!(
            vfont::read(
                &mut c,
                &VfontReadOptions {
                    byte_order: Some(if e == ByteOrder::Little {
                        ByteOrder::Big
                    } else {
                        ByteOrder::Little
                    })
                }
            )
            .is_err()
        );
    }
}
#[test]
fn bounded_truncations_and_invalid_lengths() {
    let b = fixture(ByteOrder::Little);
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
    for (at, value) in [
        (10 + 65 * 10 + 2, 0xff),
        (10 + 65 * 10 + 4, 0x80),
        (10 + 65 * 10, 0xff),
    ] {
        let mut bad = b.clone();
        bad[at] = value;
        assert!(read(&bad, &Default::default()).is_err());
    }
}
#[test]
fn zero_byte_rectangles_and_padding_provenance() {
    let mut b = fixture(ByteOrder::Little);
    let at = 10 + 32 * 10;
    b[at + 4] = 2;
    b[at + 7] = 3;
    let s = read(&b, &Default::default()).unwrap();
    let g = s
        .single()
        .unwrap()
        .glyph_for_codepoint(Codepoint::Legacy(32))
        .unwrap();
    assert_eq!(g.bitmap.as_bytes(), [0, 0]);
    let mut pad = fixture(ByteOrder::Little);
    pad[2570] |= 1;
    let input = Input::new(&pad);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    assert!(
        vfont::read(&mut c, &Default::default())
            .unwrap()
            .1
            .has_loss()
    );
}
#[test]
fn identical_limits_for_successful_writes() {
    let s = read(&fixture(ByteOrder::Little), &Default::default()).unwrap();
    for total in [0, 5000, 16000, 64000, 256000] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(out) = vfont::write(
            &s,
            &opts(),
            &VfontWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&out.output.into_single().unwrap(), &l).is_ok());
        }
    }
    let mut strict = opts();
    strict.strict = true;
    assert!(
        vfont::write(
            &s,
            &strict,
            &VfontWriteOptions {
                byte_order: ByteOrder::Big,
                ..Default::default()
            }
        )
        .is_err()
    );
}
