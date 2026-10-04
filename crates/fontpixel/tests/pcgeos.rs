//! Native BSWF offsets, exact points, DBCS sections and fractional metrics.
#![cfg(feature = "pcgeos")]
use fontpixel::formats::pcgeos::{self, PcgeosReadOptions, PcgeosVariant, PcgeosWriteOptions};
use fontpixel::model::{Codepoint, Glyph, Property, PropertyValue, Rational, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn source(dbcs: bool) -> FontSet {
    let mut f = Font::default();
    f.names.family = Some(if dbcs { "测试" } else { "Original" }.into());
    f.names.style = Some("Bold Italic".into());
    f.metrics.point_size = Some(if dbcs {
        Rational::integer(12)
    } else {
        Rational::new(25, 2).unwrap()
    });
    for cp in if dbcs { [0x3400, 0x3402] } else { [65, 67] } {
        let mut g = Glyph::new("#.......#\n.###.....\n.....####".parse::<Bitmap>().unwrap());
        g.metrics.bounds.x = -2;
        g.metrics.bounds.y = -1;
        g.metrics.advance = Some(Vector::new(8, 0));
        g.codepoints.push(if dbcs {
            Codepoint::Unicode(cp)
        } else {
            Codepoint::Legacy(cp)
        });
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn read(b: &[u8], round: bool, l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(pcgeos::read(
        &mut ReadContext::new(&i, l)?,
        &PcgeosReadOptions {
            round_fractional_metrics: round,
            ..Default::default()
        },
    )?
    .0)
}
fn write(s: &FontSet, o: &PcgeosWriteOptions) -> Vec<u8> {
    pcgeos::write(s, &WriteOptions::new(ResourcePath::new("f").unwrap()), o)
        .unwrap()
        .output
        .into_single()
        .unwrap()
}
#[test]
fn native_layouts_and_exact_points() {
    for variant in [PcgeosVariant::Sbcs, PcgeosVariant::Dbcs] {
        let dbcs = variant == PcgeosVariant::Dbcs;
        let s = source(dbcs);
        let b = write(
            &s,
            &PcgeosWriteOptions {
                variant: Some(variant),
                ..Default::default()
            },
        );
        let back = read(&b, false, &Default::default()).unwrap();
        assert_eq!(b[if dbcs { 65 } else { 41 }], 0x30);
        if !dbcs {
            assert_eq!(&b[42..45], &[128, 12, 0]);
        }
        assert_eq!(back.fonts[0].names, s.fonts[0].names);
        for (_, g) in s.fonts[0].glyphs() {
            let got = back.fonts[0].glyph_for_codepoint(g.codepoints[0]).unwrap();
            assert_eq!(got.bitmap, g.bitmap);
            assert_eq!(got.metrics, g.metrics);
            assert_eq!(
                got.properties.get("pcgeos.flags"),
                Some(&PropertyValue::Integer(0x40))
            );
        }
        let strict = WriteOptions {
            strict: true,
            ..WriteOptions::new(ResourcePath::new("f").unwrap())
        };
        assert_eq!(
            b,
            pcgeos::write(&back, &strict, &Default::default())
                .unwrap()
                .output
                .into_single()
                .unwrap()
        );
        let rebuilt = write(
            &back,
            &PcgeosWriteOptions {
                preserve_native: false,
                ..Default::default()
            },
        );
        let again = read(&rebuilt, false, &Default::default()).unwrap();
        assert_eq!(again.fonts[0].metrics, back.fonts[0].metrics);
    }
}
#[test]
fn fractional_metrics_are_explicit_and_preserved() {
    let mut b = write(&source(false), &Default::default());
    let pos = u32::from_le_bytes(b[47..51].try_into().unwrap()) as usize;
    b[pos + 25] = 128;
    b[pos + 71 + 2] = 128;
    assert!(read(&b, false, &Default::default()).is_err());
    let s = read(&b, true, &Default::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.metrics.bounds.y, 0);
    assert_eq!(g.metrics.advance, Some(Vector::new(8, 0)));
    assert_eq!(b, write(&s, &Default::default()));
}
#[test]
fn kerning_and_native_multistrike() {
    let mut s = source(false);
    s.fonts[0].properties.push(Property::new(
        "pcgeos.kern_pairs",
        PropertyValue::Bytes(vec![67, 65]),
    ));
    s.fonts[0].properties.push(Property::new(
        "pcgeos.kern_values",
        PropertyValue::Bytes(vec![128, 255]),
    ));
    let mut second = s.fonts[0].clone();
    second.metrics.point_size = Some(Rational::integer(14));
    s.fonts.push(second);
    let b = write(&s, &Default::default());
    let back = read(&b, false, &Default::default()).unwrap();
    assert_eq!(back.fonts.len(), 2);
    for f in &back.fonts {
        assert_eq!(
            f.properties.get("pcgeos.kern_values"),
            Some(&PropertyValue::Bytes(vec![128, 255]))
        );
        assert_eq!(
            f.glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap()
                .properties
                .get("pcgeos.flags"),
            Some(&PropertyValue::Integer(0x44))
        );
    }
}
#[test]
fn bounds_and_budget() {
    let b = write(&source(false), &Default::default());
    for n in 0..b.len() {
        assert!(
            read(&b[..n], false, &Default::default()).is_err(),
            "prefix {n}"
        );
    }
    for l in [
        ResourceLimits {
            max_glyphs: 2,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 26,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: b.len(),
            ..Default::default()
        },
    ] {
        assert!(read(&b, false, &l).is_err());
    }
    let mut bad = b;
    bad[33..35].fill(255);
    assert!(read(&bad, false, &Default::default()).is_err());
}

#[test]
fn extreme_font_metrics_return_errors() {
    for value in [i64::MIN, i64::MAX] {
        let mut s = source(false);
        s.fonts[0].metrics.ascent = Some(value);
        assert!(
            pcgeos::write(
                &s,
                &WriteOptions::new(ResourcePath::new("f").unwrap()),
                &Default::default()
            )
            .is_err()
        );
    }
}
