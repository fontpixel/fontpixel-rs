//! Original MIT PCL commands authored from HP's descriptor tables.
#![cfg(feature = "hppcl")]
use fontpixel::formats::hppcl::{
    self, HppclCompression, HppclOrientation, HppclReadOptions, HppclWriteOptions,
};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn command(out: &mut Vec<u8>, prefix: &str, data: &[u8]) {
    out.extend(format!("\x1b{prefix}{}W", data.len()).bytes());
    out.extend(data);
}
fn fixture() -> Vec<u8> {
    let mut h = vec![0; 64];
    h[1] = 64;
    h[3] = 2;
    h[7] = 3;
    h[9] = 9;
    h[11] = 4;
    h[13] = 1;
    h[19] = 16;
    h[33] = 16;
    h[37] = 65;
    h[39] = 65;
    h[48..52].copy_from_slice(b"TEST");
    let mut d = vec![4, 0, 14, 1, 0, 0, 255, 255, 0, 1, 0, 9, 0, 3, 0, 40];
    d.extend([128, 128, 64, 0, 255, 128]);
    let mut out = Vec::new();
    command(&mut out, ")s", &h);
    out.extend(b"\x1b*c65E");
    command(&mut out, "(s", &d);
    out
}
fn read(b: &[u8], l: &ResourceLimits, o: &HppclReadOptions) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(hppcl::read(&mut c, o)?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.sfp").unwrap())
}
#[test]
fn official_descriptor_pixels_baseline_and_advance() {
    let s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[128, 128, 64, 0, 255, 128]);
    assert_eq!(g.metrics.bounds.x, -1);
    assert_eq!(g.metrics.bounds.y, -1);
    assert_eq!(g.metrics.advance.unwrap().x, 10);
    assert_eq!(f.metrics.ascent, Some(3));
    assert_eq!(f.metrics.descent, Some(1));
}
#[test]
fn four_orientations_two_encodings_and_continuations() {
    let s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    for orientation in [
        HppclOrientation::Portrait,
        HppclOrientation::Landscape,
        HppclOrientation::ReversePortrait,
        HppclOrientation::ReverseLandscape,
    ] {
        for compression in [HppclCompression::Raw, HppclCompression::Rle] {
            let v = HppclWriteOptions {
                orientation,
                compression,
                continuation_bytes: 17,
                ..Default::default()
            };
            let out = hppcl::write(&s, &output(), &v)
                .unwrap()
                .output
                .into_single()
                .unwrap();
            let back = read(&out, &Default::default(), &Default::default()).unwrap();
            let h = back.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap();
            assert_eq!(g.bitmap, h.bitmap, "{orientation:?}/{compression:?}");
            assert_eq!(g.metrics.bounds, h.metrics.bounds);
            assert_eq!(g.metrics.advance, h.metrics.advance);
        }
    }
}
#[test]
fn rle_repeat_counts_long_runs_and_zero_run() {
    let mut s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            let mut raw = vec![255; 3 * 75];
            raw[0] = 0;
            raw[75] = 0;
            raw[150] = 0;
            g.bitmap = fontpixel::bitmap::Bitmap::<fontpixel::bitmap::Mono>::from_raw(
                600,
                3,
                &raw,
                Default::default(),
            )?
            .into();
            g.metrics.bounds.width = 600;
            g.metrics.advance = Some(fontpixel::model::Vector::new(600, 0));
            Ok(())
        })
        .unwrap();
    let v = HppclWriteOptions {
        compression: HppclCompression::Rle,
        ..Default::default()
    };
    let out = hppcl::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default(), &Default::default()).unwrap();
    assert_eq!(
        s.fonts[0].glyph(id).unwrap().bitmap,
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
    );
}
#[test]
fn malformed_descriptor_and_legacy_compatibility() {
    let mut b = fixture();
    let at = b.len() - 22;
    b[at + 2] = 16;
    assert!(read(&b, &Default::default(), &Default::default()).is_err());
    assert!(
        read(
            &b,
            &Default::default(),
            &HppclReadOptions {
                allow_legacy_descriptor_size: true
            }
        )
        .is_ok()
    );
    b[at + 2] = 13;
    assert!(read(&b, &Default::default(), &Default::default()).is_err());
}
#[test]
fn truncated_records_bad_rle_and_resource_limits() {
    let b = fixture();
    for n in 0..b.len() {
        let result = read(&b[..n], &Default::default(), &Default::default());
        if let Ok(s) = result {
            assert!(
                s.fonts[0].is_empty(),
                "only a complete header may precede truncated character: {n}"
            );
        }
    }
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
        assert!(read(&b, &l, &Default::default()).is_err());
    }
    let mut bad = b;
    let at = bad.len() - 22;
    bad[at + 3] = 2;
    bad[at + 16] = 255;
    assert!(read(&bad, &Default::default(), &Default::default()).is_err());
}
#[test]
fn multiple_fonts_aggregate_budget_and_same_limit_writer() {
    let mut b = fixture();
    b.extend(fixture());
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_glyphs: 1,
                ..Default::default()
            },
            &Default::default()
        )
        .is_err()
    );
    let s = read(&b, &Default::default(), &Default::default()).unwrap();
    let v = HppclWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 1024 * 1024,
            ..Default::default()
        },
        resolution: Some((600, 300)),
        ..Default::default()
    };
    let out = hppcl::write(&s, &output(), &v)
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
fn unknown_commands_and_fractional_advance_are_explicit() {
    let mut b = fixture();
    let at = b.len() - 22;
    b[at + 15] = 41;
    b.extend(b"\x1bE");
    let input = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    let (s, r) = hppcl::read(&mut c, &Default::default()).unwrap();
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "hppcl.fractional_advance")
    );
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "hppcl.commands_not_modelled")
    );
    let mut o = output();
    o.strict = true;
    assert!(hppcl::write(&s, &o, &Default::default()).is_err());
}
#[test]
fn extreme_font_metrics_return_error() {
    let mut s = read(&fixture(), &Default::default(), &Default::default()).unwrap();
    s.fonts[0].metrics.ascent = Some(i64::MAX);
    s.fonts[0].metrics.descent = Some(i64::MAX);
    assert!(hppcl::write(&s, &output(), &Default::default()).is_err());
}
