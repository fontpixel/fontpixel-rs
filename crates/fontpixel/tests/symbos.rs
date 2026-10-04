//! SymbOS native medium/small/big and extended-header regression checks.
#![cfg(feature = "symbos")]
use fontpixel::formats::symbos::*;
use fontpixel::formats::{Confidence, FormatDetector};
use fontpixel::model::{Codepoint, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn source(wide: bool) -> FontSet {
    let mut f = Font::default();
    let b = if wide {
        "#.......##\n.##.......\n####..#..."
    } else {
        "#.##\n.##.\n####"
    };
    let mut g = Glyph::new(b.parse::<Bitmap>().unwrap());
    g.codepoints.push(Codepoint::Legacy(65));
    g.metrics.advance = Some(Vector::new(i64::from(g.bitmap.width()), 0));
    f.add_glyph(g).unwrap();
    f.into()
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(fontpixel::formats::symbos::read(&mut ReadContext::new(&i, l)?, &Default::default())?.0)
}
fn write(s: &FontSet, v: SymbosVersion, extended: Option<SymbosExtendedInfo>) -> Vec<u8> {
    fontpixel::formats::symbos::write(
        s,
        &WriteOptions::new(ResourcePath::new("font.fnt").unwrap()),
        &SymbosWriteOptions {
            version: Some(v),
            extended,
            ..Default::default()
        },
    )
    .unwrap()
    .output
    .into_single()
    .unwrap()
}
#[test]
fn all_native_layouts() {
    for v in [
        SymbosVersion::Medium,
        SymbosVersion::Small,
        SymbosVersion::Big,
    ] {
        let s = source(v == SymbosVersion::Big);
        for ext in [
            None,
            Some(SymbosExtendedInfo::WritingSystem(2)),
            Some(SymbosExtendedInfo::Layout { wysiwyg: true }),
        ] {
            let b = write(&s, v, ext);
            assert_eq!(detect(&b, false), Confidence::Possible);
            let got = read(&b, &Default::default()).unwrap();
            assert_eq!(got.fonts[0].len(), if ext.is_some() { 255 } else { 96 });
            let a = s.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap();
            let g = got.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap();
            assert_eq!(a.bitmap, g.bitmap);
            assert_eq!(a.metrics, g.metrics);
            assert_eq!(
                b,
                fontpixel::formats::symbos::write(
                    &got,
                    &WriteOptions::new(ResourcePath::new("f").unwrap()),
                    &Default::default()
                )
                .unwrap()
                .output
                .into_single()
                .unwrap()
            );
        }
    }
}
#[test]
fn limits_and_malformed_native_data() {
    for v in [
        SymbosVersion::Small,
        SymbosVersion::Medium,
        SymbosVersion::Big,
    ] {
        let b = write(&source(false), v, None);
        for n in 0..b.len() {
            assert!(
                read(&b[..n], &Default::default()).is_err(),
                "{v:?} prefix {n}"
            );
            assert_eq!(
                detect(&b[..n], false),
                Confidence::NoMatch,
                "{v:?} prefix {n}"
            );
        }
        assert!(
            read(
                &b,
                &ResourceLimits {
                    max_glyphs: 95,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    let mut b = write(&source(true), SymbosVersion::Big, None);
    b[2..4].fill(0);
    assert!(read(&b, &Default::default()).is_err());
}
#[test]
fn strict_and_explicit_gray() {
    let mut s = source(false);
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            if let fontpixel::model::GlyphBitmap::Mono(b) = &g.bitmap {
                g.bitmap = b.to_gray8()?.into();
            }
            Ok(())
        })
        .unwrap();
    let w = WriteOptions::new(ResourcePath::new("f").unwrap());
    assert!(fontpixel::formats::symbos::write(&s, &w, &Default::default()).is_err());
    let mut strict = w.clone();
    strict.strict = true;
    assert!(
        fontpixel::formats::symbos::write(&source(false), &strict, &Default::default()).is_err()
    );
}

fn detect(b: &[u8], allow_short_range: bool) -> Confidence {
    let input = Input::new(b);
    let limits = ResourceLimits::default();
    SymbosCodec {
        read_options: SymbosReadOptions { allow_short_range },
        ..Default::default()
    }
    .detect(&mut ReadContext::new(&input, &limits).unwrap())
    .unwrap()
}
#[test]
fn detection_requires_complete_bounded_native_records() {
    assert_eq!(
        detect(
            b"const uint8_t Cells[2][2]={{0xA0,0x40},{0x40,0xA0}};",
            false
        ),
        Confidence::NoMatch
    );
    for version in [
        SymbosVersion::Medium,
        SymbosVersion::Small,
        SymbosVersion::Big,
    ] {
        let mut b = write(&source(false), version, None);
        let mut trailing = b.clone();
        trailing.push(0);
        assert_eq!(detect(&trailing, false), Confidence::NoMatch);
        if version == SymbosVersion::Big {
            let at = 3 + usize::from(u16::from_le_bytes([b[2], b[3]]));
            b[at] = 8;
            b[at + 1] = 9;
        } else {
            b[2] = 9;
        }
        assert_eq!(detect(&b, false), Confidence::NoMatch);
    }
    let b = write(&source(false), SymbosVersion::Small, None);
    assert_eq!(detect(&b[..11], false), Confidence::NoMatch);
    assert_eq!(detect(&b[..11], true), Confidence::Possible);
}
#[test]
fn original_pebble_versions_do_not_match_symbos() {
    assert_eq!(
        detect(
            &[
                1, 8, 3, 0, 65, 0, 65, 0, 1, 0, 66, 0, 1, 0, 67, 0, 4, 0, 0, 0, 0, 0, 9, 3, 254, 3,
                0, 0, 0, 10, 165, 57, 29, 1, 7, 2, 1, 254, 0, 0, 0, 253, 0, 0, 0, 0
            ],
            false
        ),
        Confidence::NoMatch
    );
    assert_eq!(
        detect(
            &[
                2, 8, 3, 0, 65, 0, 1, 2, 0, 3, 0, 0, 65, 0, 4, 0, 66, 0, 4, 0, 67, 0, 13, 0, 0, 0,
                0, 0, 9, 3, 254, 3, 10, 165, 57, 29, 1, 7, 2, 1, 254, 253, 0, 0
            ],
            false
        ),
        Confidence::NoMatch
    );
    assert_eq!(
        detect(
            &[
                3, 8, 3, 0, 65, 0, 1, 2, 10, 1, 0, 3, 0, 0, 65, 0, 4, 0, 66, 0, 4, 0, 67, 0, 13, 0,
                0, 0, 0, 0, 9, 3, 254, 3, 10, 165, 57, 29, 1, 7, 2, 1, 254, 253, 0, 0
            ],
            false
        ),
        Confidence::NoMatch
    );
}
