//! Native Palm resource, density, grayscale and container regression tests.
#![cfg(feature = "palm")]
use fontpixel::formats::palm::*;
use fontpixel::model::{Codepoint, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn source() -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("Original".into());
    f.metrics.ascent = Some(4);
    f.metrics.descent = Some(2);
    for cp in [65, 67] {
        let mut g = Glyph::new("##\n.#\n##\n..\n#.\n..".parse::<Bitmap>().unwrap());
        g.codepoints.push(Codepoint::Legacy(cp));
        g.metrics.bounds.y = -2;
        g.metrics.advance = Some(Vector::new(2, 0));
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    Ok(fontpixel::formats::palm::read(&mut ReadContext::new(&input, l)?, &Default::default())?.0)
}
fn write(s: &FontSet, container: PalmContainer, version: PalmVersion) -> Vec<u8> {
    fontpixel::formats::palm::write(
        s,
        &WriteOptions::new(ResourcePath::new("test.prc").unwrap()),
        &PalmWriteOptions {
            container,
            version,
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
fn all_native_layouts() {
    for container in [PalmContainer::Bare, PalmContainer::Pdb, PalmContainer::Prc] {
        for version in [
            PalmVersion::V1,
            PalmVersion::V2,
            PalmVersion::Afnx,
            PalmVersion::FontSubst,
        ] {
            let s = source();
            let bytes = write(&s, container, version);
            let got = read(&bytes, &Default::default()).unwrap();
            for cp in [65, 67] {
                let a = s.fonts[0]
                    .glyph_for_codepoint(Codepoint::Legacy(cp))
                    .unwrap();
                let b = got.fonts[0]
                    .glyph_for_codepoint(Codepoint::Legacy(cp))
                    .unwrap();
                assert_eq!(a.bitmap, b.bitmap);
                assert_eq!(a.metrics, b.metrics);
            }
        }
    }
}
#[test]
fn grayfont_preserves_pixels() {
    let mut s = source();
    for (id, g) in s.fonts[0]
        .glyphs()
        .map(|(id, g)| (id, g.clone()))
        .collect::<Vec<_>>()
    {
        let mut gray = match g.bitmap {
            fontpixel::model::GlyphBitmap::Mono(m) => m.to_gray8().unwrap(),
            _ => unreachable!(),
        };
        gray.set_pixel(0, 0, 85).unwrap();
        s.fonts[0]
            .edit_glyph(id, |g| {
                g.bitmap = gray.into();
                Ok(())
            })
            .unwrap();
    }
    for v in [PalmVersion::Gray1, PalmVersion::Gray2] {
        let b = write(&s, PalmContainer::Prc, v);
        let got = read(&b, &Default::default()).unwrap();
        for cp in [65, 67] {
            assert_eq!(
                s.fonts[0]
                    .glyph_for_codepoint(Codepoint::Legacy(cp))
                    .unwrap()
                    .bitmap,
                got.fonts[0]
                    .glyph_for_codepoint(Codepoint::Legacy(cp))
                    .unwrap()
                    .bitmap
            );
        }
    }
}
#[test]
fn truncations_and_aggregate_limits() {
    for v in [
        PalmVersion::V1,
        PalmVersion::V2,
        PalmVersion::Afnx,
        PalmVersion::Gray2,
    ] {
        let b = write(&source(), PalmContainer::Prc, v);
        for n in 0..b.len() {
            let _ = read(&b[..n], &Default::default());
        }
        let l = ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        };
        assert!(read(&b, &l).is_err());
    }
}
#[test]
fn corrupt_tables_and_chained_directory() {
    let mut b = write(&source(), PalmContainer::Prc, PalmVersion::V1);
    b[72..76].copy_from_slice(&72u32.to_be_bytes());
    assert!(read(&b, &Default::default()).is_err());
    let mut b = write(&source(), PalmContainer::Bare, PalmVersion::V1);
    b[16..18].copy_from_slice(&0u16.to_be_bytes());
    assert!(read(&b, &Default::default()).is_err());
}
#[test]
fn explicit_domain_and_output_limit() {
    let mut s = source();
    let ids: Vec<_> = s.fonts[0].glyphs().map(|(id, _)| id).collect();
    for id in ids {
        s.fonts[0]
            .edit_glyph(id, |g| {
                g.codepoints[0] = Codepoint::Unicode(g.codepoints[0].value());
                Ok(())
            })
            .unwrap();
    }
    let w = WriteOptions::new(ResourcePath::new("x").unwrap());
    assert!(fontpixel::formats::palm::write(&s, &w, &Default::default()).is_err());
    let s = source();
    assert!(
        fontpixel::formats::palm::write(
            &s,
            &w,
            &PalmWriteOptions {
                max_output_bytes: 2,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn every_native_density_and_extreme_metrics() {
    let w = WriteOptions::new(ResourcePath::new("x").unwrap());
    for density in [72u16, 108, 144, 216, 288] {
        let mut s = source();
        // Width/advance 2 and height 6 standard pixels remain integral at 108 dpi.
        let scale = u32::from(density);
        let ids: Vec<_> = s.fonts[0].glyphs().map(|(id, _)| id).collect();
        for id in ids {
            s.fonts[0]
                .edit_glyph(id, |g| {
                    let mut bm =
                        Bitmap::<fontpixel::bitmap::Mono>::new(2 * scale / 72, 6 * scale / 72)?;
                    bm.set_pixel(0, 0, 1)?;
                    g.bitmap = bm.into();
                    g.metrics.bounds.width = 2 * scale / 72;
                    g.metrics.bounds.height = 6 * scale / 72;
                    g.metrics.bounds.y = -i64::from(2 * scale / 72);
                    g.metrics.advance = Some(Vector::new(i64::from(2 * scale / 72), 0));
                    Ok(())
                })
                .unwrap();
        }
        s.fonts[0].metrics.ascent = Some(i64::from(4 * scale / 72));
        s.fonts[0].metrics.descent = Some(i64::from(2 * scale / 72));
        let got = fontpixel::formats::palm::write(
            &s,
            &w,
            &PalmWriteOptions {
                container: PalmContainer::Bare,
                version: PalmVersion::V2,
                densities: vec![density],
                ..Default::default()
            },
        )
        .unwrap();
        let back = read(&got.output.assets()[0].bytes, &Default::default()).unwrap();
        for cp in [65, 67] {
            assert_eq!(
                s.fonts[0]
                    .glyph_for_codepoint(Codepoint::Legacy(cp))
                    .unwrap()
                    .bitmap,
                back.fonts[0]
                    .glyph_for_codepoint(Codepoint::Legacy(cp))
                    .unwrap()
                    .bitmap
            );
        }
    }
    let mut s = source();
    s.fonts[0].metrics.ascent = Some(i64::MAX);
    s.fonts[0].metrics.descent = Some(i64::MAX);
    for version in [PalmVersion::V1, PalmVersion::V2, PalmVersion::Gray2] {
        assert!(
            fontpixel::formats::palm::write(
                &s,
                &w,
                &PalmWriteOptions {
                    version,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
#[test]
fn preserves_native_density_resource_group_on_rewrite() {
    let s = source();
    let mut b = write(&s, PalmContainer::Prc, PalmVersion::V2);
    let mut pair = s.clone();
    let mut high = s.fonts[0].clone();
    high.metrics.ascent = Some(8);
    high.metrics.descent = Some(4);
    high.properties
        .push(fontpixel::model::Property::new("palm.density", 144i64));
    let ids: Vec<_> = high.glyphs().map(|(id, _)| id).collect();
    for id in ids {
        high.edit_glyph(id, |g| {
            g.bitmap = Bitmap::<fontpixel::bitmap::Mono>::new(4, 12)?.into();
            g.metrics.bounds = fontpixel::bitmap::Bounds::new(4, 12, 0, -4);
            g.metrics.advance = Some(Vector::new(4, 0));
            Ok(())
        })
        .unwrap();
    }
    pair.fonts.push(high);
    let payload = write(&pair, PalmContainer::Bare, PalmVersion::V2);
    let at = u32::from_be_bytes(b[84..88].try_into().unwrap()) as usize;
    b.truncate(at);
    b.extend(payload);
    let input = read(&b, &Default::default()).unwrap();
    assert_eq!(input.fonts.len(), 2);
    let out = write(&input, PalmContainer::Prc, PalmVersion::V2);
    assert_eq!(b, out);
    let mut dup = b.clone();
    // Two equal resource directory identities must be rejected before payload parsing.
    dup[76..78].copy_from_slice(&2u16.to_be_bytes());
    let entry = dup[78..88].to_vec();
    dup.splice(88..88, entry);
    assert!(read(&dup, &Default::default()).is_err());
}
