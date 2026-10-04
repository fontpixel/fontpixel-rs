//! Original MIT fixtures for the Catalyst resource directory.
#![cfg(feature = "catalyst")]
use fontpixel::formats::catalyst::{
    self, CatalystOffsetBase, CatalystReadOptions, CatalystWriteOptions,
};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 399];
    b[0] = 1;
    b[1..3].copy_from_slice(&396u16.to_le_bytes());
    let body = &mut b[3..];
    for cp in 0..128 {
        body[cp * 2..cp * 2 + 2].copy_from_slice(&387u16.to_le_bytes());
    }
    body[130..132].copy_from_slice(&387u16.to_le_bytes());
    body[132..134].copy_from_slice(&393u16.to_le_bytes());
    body[321] = 9;
    body[322] = 136;
    body[384..387].copy_from_slice(&[3, 2, 1]);
    body[387..].copy_from_slice(&[0x81, 1, 2, 0, 0x7f, 1, 0x7f, 1, 2]);
    b
}
fn read(b: &[u8], l: &ResourceLimits, o: &CatalystReadOptions) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(catalyst::read(&mut c, o)?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("SYSTEM.FONTS").unwrap())
}
#[test]
fn pixels_offsets_color_and_shift() {
    let s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 128);
    let a = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!(a.bitmap.width(), 8);
    assert_eq!(a.bitmap.as_bytes(), &[0x81, 0x40, 0xff]);
    let b = f.glyph_for_codepoint(Codepoint::Legacy(66)).unwrap();
    assert_eq!(b.metrics.bounds.y, -1);
    assert_eq!(b.metrics.advance.unwrap().x, 8);
    let out = catalyst::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(out[390] & 128, 128);
    let back = read(&out, &Default::default(), &Default::default()).unwrap();
    assert_eq!(
        a.bitmap,
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
    );
}
#[test]
fn authoritative_offsets_allow_alias_and_reorder() {
    let mut b = fixture();
    b[3 + 132..3 + 134].copy_from_slice(&387u16.to_le_bytes());
    let s = read(&b, &Default::default(), &Default::default()).unwrap();
    assert_eq!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(66))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[0x80, 0x80, 0x40]
    );
    b[3 + 132..3 + 134].copy_from_slice(&386u16.to_le_bytes());
    assert!(read(&b, &Default::default(), &Default::default()).is_err());
}
#[test]
fn explicit_data_relative_offsets() {
    let s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    let v = CatalystWriteOptions {
        offset_base: CatalystOffsetBase::GlyphData,
        ..Default::default()
    };
    let out = catalyst::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(read(&out, &Default::default(), &Default::default()).is_err());
    assert!(
        read(
            &out,
            &Default::default(),
            &CatalystReadOptions {
                offset_base: CatalystOffsetBase::GlyphData
            }
        )
        .is_ok()
    );
}
#[test]
fn every_truncation_and_limits() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(
            read(&b[..n], &Default::default(), &Default::default()).is_err(),
            "prefix {n}"
        );
    }
    for l in [
        ResourceLimits {
            max_glyphs: 127,
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
}
#[test]
fn collection_budget_is_aggregate_and_output_readable() {
    let mut b = fixture();
    let part = b[1..].to_vec();
    b[0] = 2;
    b.extend(part);
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_glyphs: 255,
                ..Default::default()
            },
            &Default::default()
        )
        .is_err()
    );
    let s = read(&b, &Default::default(), &Default::default()).unwrap();
    let v = CatalystWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 8 * 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = catalyst::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(
        read(&out, &v.limits, &Default::default())
            .unwrap()
            .fonts
            .len(),
        2
    );
}
#[test]
fn strict_reports_metadata_and_bad_metrics() {
    let mut s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    s.fonts[0].names.copyright = Some("test".into());
    let mut o = output();
    o.strict = true;
    assert!(catalyst::write(&s, &o, &Default::default()).is_err());
    assert!(
        catalyst::write(&s, &output(), &Default::default())
            .unwrap()
            .report
            .has_loss()
    );
}
#[test]
fn extreme_line_height_returns_error_and_override_is_authoritative() {
    let mut s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    s.fonts[0].properties.remove("catalyst.metrics");
    s.fonts[0].metrics.line_height = Some(i64::MIN);
    assert!(catalyst::write(&s, &output(), &Default::default()).is_err());
    assert!(
        catalyst::write(
            &s,
            &output(),
            &CatalystWriteOptions {
                leading: Some(2),
                ..Default::default()
            }
        )
        .is_ok()
    );
}
