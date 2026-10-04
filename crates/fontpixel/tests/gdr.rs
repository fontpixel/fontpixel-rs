//! Native GDR metrics, mapping, compression and resource-bound regressions.
#![cfg(feature = "gdr")]
use fontpixel::formats::gdr::*;
use fontpixel::model::{Codepoint, Glyph, Property, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn source() -> FontSet {
    let mut set = FontSet::default();
    for n in 0..2 {
        let mut f = Font::default();
        f.names.family = Some("Original Ελληνικά 日本語".into());
        f.names.style = Some("Bold".into());
        f.metrics.ascent = Some(5);
        f.metrics.descent = Some(2);
        for cp in [65, 67, 0x100, 0xfffd] {
            let mut g = Glyph::new("#.#\n###\n###\n...\n#.#".parse::<Bitmap>().unwrap());
            g.codepoints.push(Codepoint::Unicode(cp));
            g.metrics.bounds.x = -1;
            g.metrics.bounds.y = -2;
            g.metrics.advance = Some(Vector::new(4, 0));
            f.add_glyph(g).unwrap();
        }
        f.properties
            .push(Property::new("gdr.uid", 100 + i64::from(n)));
        set.fonts.push(f);
    }
    set
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    Ok(fontpixel::formats::gdr::read(&mut ReadContext::new(&input, l)?, &Default::default())?.0)
}
fn write(s: &FontSet, v: GdrVersion) -> Vec<u8> {
    fontpixel::formats::gdr::write(
        s,
        &WriteOptions::new(ResourcePath::new("test.gdr").unwrap()),
        &GdrWriteOptions {
            version: v,
            ..Default::default()
        },
    )
    .unwrap()
    .output
    .assets()[0]
        .bytes
        .clone()
}
#[test]
fn native_multistrike_metrics_pixels_and_names() {
    for v in [
        GdrVersion::V39,
        GdrVersion::V40,
        GdrVersion::V41,
        GdrVersion::V42,
    ] {
        let s = source();
        let b = write(&s, v);
        let got = read(&b, &Default::default()).unwrap();
        assert_eq!(got.fonts.len(), 2);
        for (a, b) in s.fonts.iter().zip(&got.fonts) {
            assert_eq!(a.names.family, b.names.family);
            for cp in [65, 67, 0x100, 0xfffd] {
                let a = a.glyph_for_codepoint(Codepoint::Unicode(cp)).unwrap();
                let b = b.glyph_for_codepoint(Codepoint::Unicode(cp)).unwrap();
                assert_eq!(a, b);
            }
        }
    }
}
#[test]
fn truncation_counts_and_compressed_runs() {
    let b = write(&source(), GdrVersion::V41);
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix {n}");
    }
    let l = ResourceLimits {
        max_glyphs: 7,
        ..Default::default()
    };
    assert!(read(&b, &l).is_err());
    let mut corrupt = b.clone();
    corrupt[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read(&corrupt, &Default::default()).is_err());
}
#[test]
fn losses_are_not_silent() {
    let mut s = source();
    let mut g = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::new(0, 0).unwrap());
    g.codepoints.push(Codepoint::Unicode(0x10000));
    g.name = Some("extra".into());
    s.fonts[0].add_glyph(g).unwrap();
    let w = WriteOptions::new(ResourcePath::new("x").unwrap());
    let result = fontpixel::formats::gdr::write(&s, &w, &Default::default()).unwrap();
    assert!(
        result
            .report
            .entries
            .iter()
            .any(|e| e.code == "gdr.write.mapping")
    );
    let mut w = w;
    w.strict = true;
    assert!(fontpixel::formats::gdr::write(&s, &w, &Default::default()).is_err());
}
#[test]
fn dense_wide_metric_indices() {
    let mut f = Font::default();
    for cp in 0..140 {
        let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
        g.codepoints.push(Codepoint::Unicode(cp));
        g.metrics.bounds.x = i64::from(cp % 127) - 63;
        g.metrics.bounds.y = i64::from(cp / 127);
        g.metrics.advance = Some(Vector::new(4, 0));
        f.add_glyph(g).unwrap();
    }
    let s = FontSet::from(f);
    let b = write(&s, GdrVersion::V41);
    let got = read(&b, &Default::default()).unwrap();
    for cp in 0..140 {
        assert_eq!(
            s.fonts[0].glyph_for_codepoint(Codepoint::Unicode(cp)),
            got.fonts[0].glyph_for_codepoint(Codepoint::Unicode(cp))
        );
    }
}
#[test]
fn extreme_cell_metrics_and_zero_size_inline() {
    let mut s = source();
    s.fonts[0].metrics.pixel_size = None;
    s.fonts[0].metrics.ascent = Some(i64::MAX);
    s.fonts[0].metrics.descent = Some(i64::MAX);
    assert!(
        fontpixel::formats::gdr::write(
            &s,
            &WriteOptions::new(ResourcePath::new("x").unwrap()),
            &Default::default()
        )
        .is_err()
    );
    let mut f = Font::default();
    let mut g = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::new(0, 0).unwrap());
    g.codepoints.push(Codepoint::Unicode(32));
    g.metrics.advance = Some(Vector::new(2, 0));
    f.add_glyph(g).unwrap();
    let b = write(&f.into(), GdrVersion::V40);
    assert_eq!(read(&b, &Default::default()).unwrap().fonts[0].len(), 1);
}

#[test]
fn inline_native_empty_bitmap_stream() {
    let mut f = Font::default();
    let mut g = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::new(0, 0).unwrap());
    g.codepoints.push(Codepoint::Unicode(32));
    g.metrics.advance = Some(Vector::new(2, 0));
    f.add_glyph(g).unwrap();
    let mut b = write(&f.into(), GdrVersion::V40);
    let data = u32::from_le_bytes(b[32..36].try_into().unwrap()) as usize;
    let section = data + 4 + 19;
    let bitmap = u32::from_le_bytes(b[section + 8..section + 12].try_into().unwrap()) as usize;
    b[bitmap..bitmap + 4].fill(0);
    b.truncate(bitmap + 4);
    let back = read(&b, &Default::default()).unwrap();
    assert_eq!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(32))
            .unwrap()
            .bitmap
            .width(),
        0
    );
}
