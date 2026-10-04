//! Original byte-authored FZX fixtures (project MIT license).
#![cfg(feature = "fzx")]
use fontpixel::formats::fzx::{self, FzxWriteOptions};
use fontpixel::model::{Codepoint, FontSet, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    vec![
        8, 2, 34, 11, 0, 2, 8, 0xc0, 0x18, 9, 0, 0x21, 10, 0, 0x80, 0, 0x40, 0x80, 0xc0, 0, 0x80,
        0x40,
    ]
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(fzx::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("test.fzx").unwrap())
}
#[test]
fn entry_relative_offsets_width9_kern_shift_tracking() {
    let b = fixture();
    let s = read(&b, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 3);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(33)).unwrap();
    assert_eq!(g.bitmap.width(), 9);
    assert_eq!(g.bitmap.height(), 2);
    assert_eq!(g.bitmap.as_bytes(), [0x80, 0, 0x40, 0x80]);
    assert_eq!(g.metrics.bounds.x, -3);
    assert_eq!(g.metrics.bounds.y, 5);
    assert_eq!(g.metrics.advance, Some(Vector::new(8, 0)));
    assert_eq!(f.metrics.line_height, Some(8));
    let out = fzx::write(&s, &opts(), &Default::default()).unwrap();
    assert!(!out.report.has_loss(), "{}", out.report);
    assert_eq!(out.output.into_single().unwrap(), b);
}
#[test]
fn malformed_offsets_and_partial_rows() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
    for (at, val) in [(0, 0), (2, 31), (3, 0), (9, 8), (12, 255)] {
        let mut bad = b.clone();
        bad[at] = val;
        assert!(read(&bad, &Default::default()).is_err(), "{at}");
    }
}
#[test]
fn header_tracking_is_unsigned() {
    let mut b = fixture();
    b[1] = 200;
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        s.single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Legacy(33))
            .unwrap()
            .metrics
            .advance,
        Some(Vector::new(206, 0))
    );
    assert_eq!(
        fzx::write(&s, &opts(), &Default::default())
            .unwrap()
            .output
            .into_single()
            .unwrap(),
        b
    );
}
#[test]
fn top_padding_and_same_limits() {
    let mut s = read(&fixture(), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(33)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.bounds.y -= 30;
            Ok(())
        })
        .unwrap();
    let out = fzx::write(&s, &opts(), &Default::default()).unwrap();
    assert!(out.report.has_loss());
    let target = read(&out.output.into_single().unwrap(), &Default::default()).unwrap();
    let g = target
        .single()
        .unwrap()
        .glyph_for_codepoint(Codepoint::Legacy(33))
        .unwrap();
    assert_eq!(g.metrics.bounds.y, -25);
    assert_eq!(g.bitmap.height(), 18);
    for total in [0, 1024, 16000, 128000] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(w) = fzx::write(
            &s,
            &opts(),
            &FzxWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &l).is_ok());
        }
    }
}
#[test]
fn width_zero_space_preserves_advance() {
    use fontpixel::bitmap::{Bitmap, Mono};
    use fontpixel::model::{Font, Glyph};
    let mut space = Glyph::new(Bitmap::<Mono>::new(0, 0).unwrap());
    space.codepoints.push(Codepoint::Legacy(32));
    let mut font = Font::from_glyphs([space]).unwrap();
    font.metrics.glyph_defaults.advance = Some(Vector::new(3, 0));
    let out = fzx::write(
        &font.into(),
        &opts(),
        &FzxWriteOptions {
            height: Some(8),
            tracking: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(out.report.has_loss());
    let s = read(&out.output.into_single().unwrap(), &Default::default()).unwrap();
    let g = s
        .single()
        .unwrap()
        .glyph_for_codepoint(Codepoint::Legacy(32))
        .unwrap();
    assert_eq!(g.metrics.advance, Some(Vector::new(3, 0)));
    assert_eq!(g.bitmap.width(), 1);
    assert_eq!(g.bitmap.height(), 0);
}
#[test]
fn positive_left_bearing_is_padded_without_moving_ink() {
    let mut s = read(&fixture(), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(33)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.bounds.x = 2;
            g.metrics.advance = Some(Vector::new(13, 0));
            Ok(())
        })
        .unwrap();
    let out = fzx::write(&s, &opts(), &Default::default()).unwrap();
    assert!(out.report.has_loss());
    let back = read(&out.output.into_single().unwrap(), &Default::default()).unwrap();
    let g = back
        .single()
        .unwrap()
        .glyph_for_codepoint(Codepoint::Legacy(33))
        .unwrap();
    assert_eq!(g.metrics.bounds.x, 0);
    assert_eq!(g.bitmap.width(), 11);
    assert_eq!(g.bitmap.as_bytes(), [0x20, 0, 0x10, 0x20]);
    assert_eq!(g.metrics.advance, Some(Vector::new(13, 0)));
}
