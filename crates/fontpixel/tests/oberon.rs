//! Oberon native run, signed metric and raster regressions.
#![cfg(feature = "oberon")]
use fontpixel::model::{Codepoint, Glyph, Property, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(fontpixel::formats::oberon::read(&mut ReadContext::new(&i, l)?, &Default::default())?.0)
}
fn source() -> FontSet {
    let mut f = Font::default();
    f.metrics.line_height = Some(12);
    f.properties.push(Property::new("oberon.family", 19i64));
    for code in [0, 65, 66, 255] {
        let mut g = Glyph::new("#.......#\n.##......\n.....####".parse::<Bitmap>().unwrap());
        g.metrics.bounds.x = -2;
        g.metrics.bounds.y = -1;
        g.metrics.advance = Some(Vector::new(8, 0));
        g.codepoints.push(Codepoint::Legacy(code));
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn write(s: &FontSet) -> Vec<u8> {
    fontpixel::formats::oberon::write(
        s,
        &WriteOptions::new(ResourcePath::new("f").unwrap()),
        &Default::default(),
    )
    .unwrap()
    .output
    .into_single()
    .unwrap()
}
#[test]
fn all_words_runs_and_pixels() {
    let s = source();
    let b = write(&s);
    let back = read(&b, &Default::default()).unwrap();
    assert_eq!(b[14], 3);
    assert_eq!(&b[24..28], &[255, 0, 0, 1]);
    assert_eq!(
        s.fonts[0].metrics.line_height,
        back.fonts[0].metrics.line_height
    );
    for (_, g) in s.fonts[0].glyphs() {
        let got = back.fonts[0].glyph_for_codepoint(g.codepoints[0]).unwrap();
        assert_eq!(g.bitmap, got.bitmap);
        assert_eq!(g.metrics, got.metrics);
    }
    assert_eq!(b, write(&back));
}
#[test]
fn hostile_tables_and_limits() {
    let b = write(&source());
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix {n}");
    }
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_pixels_per_glyph: 26,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_glyphs: 3,
                ..Default::default()
            }
        )
        .is_err()
    );
    for (at, v) in [(1, 1), (14, 17), (18, 0), (26, 1)] {
        let mut bad = b.clone();
        bad[at] = v;
        assert!(read(&bad, &Default::default()).is_err(), "byte {at}");
    }
}
