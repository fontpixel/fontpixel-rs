//! GEOS CVT sector accounting, MEGA subsets and Unicode extension regressions.
#![cfg(feature = "geos")]
use fontpixel::formats::geos::{
    self, GeosContainer, GeosReadOptions, GeosVariant, GeosWriteOptions,
};
use fontpixel::model::{Codepoint, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn read(b: &[u8], include_del: bool, l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(geos::read(
        &mut ReadContext::new(&i, l)?,
        &GeosReadOptions { include_del },
    )?
    .0)
}
fn write(s: &FontSet, o: &GeosWriteOptions) -> fontpixel::Result<Vec<u8>> {
    Ok(
        geos::write(s, &WriteOptions::new(ResourcePath::new("f").unwrap()), o)?
            .output
            .into_single()
            .unwrap(),
    )
}
fn source(unicode: bool) -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("Original".into());
    f.metrics.ascent = Some(3);
    f.metrics.descent = Some(0);
    f.metrics.pixel_size = Some(12);
    let codes = if unicode {
        vec![
            32, 65, 126, 0x80, 0x7ff, 0x800, 0xffff, 0x10000, 0x40000, 0x10ffff,
        ]
    } else {
        vec![32, 65, 126]
    };
    for cp in codes {
        let mut g = Glyph::new("#...#\n.###.\n..#..".parse::<Bitmap>().unwrap());
        g.codepoints.push(if unicode {
            Codepoint::Unicode(cp)
        } else {
            Codepoint::Legacy(cp)
        });
        g.metrics.advance = Some(Vector::new(7, 0));
        g.metrics.bounds.x = -2;
        f.add_glyph(g).unwrap();
    }
    f.into()
}
#[test]
fn native_variants_and_unicode_levels() {
    for variant in [
        GeosVariant::Standard,
        GeosVariant::Extended,
        GeosVariant::Mega,
        GeosVariant::Utf8,
        GeosVariant::MegaUtf8,
    ] {
        let s = source(matches!(variant, GeosVariant::Utf8 | GeosVariant::MegaUtf8));
        let options = GeosWriteOptions {
            variant: Some(variant),
            ..Default::default()
        };
        let b = write(&s, &options).unwrap();
        let back = read(&b, false, &Default::default()).unwrap();
        assert_eq!(back.fonts.len(), 1);
        for (_, g) in s.fonts[0].glyphs() {
            let got = back.fonts[0].glyph_for_codepoint(g.codepoints[0]).unwrap();
            if variant != GeosVariant::Standard {
                assert_eq!(g.bitmap, got.bitmap);
                assert_eq!(g.metrics, got.metrics);
            } else {
                assert_eq!(got.bitmap.width(), 7);
            }
        }
        let strict = WriteOptions {
            strict: true,
            ..WriteOptions::new(ResourcePath::new("f").unwrap())
        };
        assert_eq!(
            b,
            geos::write(&back, &strict, &Default::default())
                .unwrap()
                .output
                .into_single()
                .unwrap()
        );
    }
}
#[test]
fn bare_original_95_and_explicit_del() {
    let o = GeosWriteOptions {
        container: Some(GeosContainer::Bare),
        variant: Some(GeosVariant::Extended),
        ..Default::default()
    };
    let s = source(false);
    let b = write(&s, &o).unwrap();
    assert_eq!(
        read(&b, false, &Default::default()).unwrap().fonts[0].len(),
        95
    );
    assert!(read(&b, true, &Default::default()).is_err());
    let b = write(
        &s,
        &GeosWriteOptions {
            include_del: Some(true),
            ..o
        },
    )
    .unwrap();
    assert_eq!(
        read(&b, true, &Default::default()).unwrap().fonts[0].len(),
        96
    );
}
#[test]
fn rebuilt_source_and_hostile_boundaries() {
    let s = source(true);
    let o = GeosWriteOptions {
        variant: Some(GeosVariant::Utf8),
        ..Default::default()
    };
    let b = write(&s, &o).unwrap();
    for n in [0, 7, 253, 507, 761, 762, b.len() / 2] {
        assert!(
            read(&b[..n], false, &Default::default()).is_err(),
            "prefix {n}"
        );
    }
    for l in [
        ResourceLimits {
            max_glyphs: 94,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 14,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: b.len() * 2,
            ..Default::default()
        },
    ] {
        assert!(read(&b, false, &l).is_err());
    }
    let back = read(&b, false, &Default::default()).unwrap();
    let rebuilt = write(
        &back,
        &GeosWriteOptions {
            preserve_native: false,
            ..Default::default()
        },
    )
    .unwrap();
    let twice = read(&rebuilt, false, &Default::default()).unwrap();
    for (_, g) in back.fonts[0].glyphs() {
        let got = twice.fonts[0].glyph_for_codepoint(g.codepoints[0]).unwrap();
        assert_eq!(g.bitmap, got.bitmap);
        assert_eq!(g.metrics, got.metrics);
    }
    let mut bad = b.clone();
    bad[509 + 12 * 2] = 1;
    assert!(read(&bad, false, &Default::default()).is_err());
}
#[test]
fn multistrike_and_sector_end_exact() {
    let mut s = source(false);
    s.fonts.push(s.fonts[0].clone());
    s.fonts[1].metrics.pixel_size = Some(14);
    let b = write(
        &s,
        &GeosWriteOptions {
            variant: Some(GeosVariant::Extended),
            ..Default::default()
        },
    )
    .unwrap();
    let back = read(&b, false, &Default::default()).unwrap();
    assert_eq!(back.fonts.len(), 2);
    assert_eq!(back.fonts[1].metrics.pixel_size, Some(14));
    let b = write(
        &source(false),
        &GeosWriteOptions {
            container: Some(GeosContainer::Bare),
            ..Default::default()
        },
    )
    .unwrap();
    // A fully occupied final 254-byte sector encodes last=255, not zero.
    let mut padded = b.clone();
    padded.resize(508, 0);
    let mut cvt = vec![0u8; 762];
    cvt[21] = 1;
    cvt[22] = 8;
    cvt[321] = 8;
    cvt[322] = 1;
    cvt[30..58].copy_from_slice(b"PRG formatted GEOS file V1.0");
    cvt[508] = 2;
    cvt[509] = 255;
    cvt.extend(padded);
    assert_eq!(
        read(&cvt, false, &Default::default()).unwrap().fonts.len(),
        1
    );
}

#[test]
fn extreme_font_metrics_return_errors() {
    for value in [i64::MIN, i64::MAX] {
        let mut s = source(false);
        s.fonts[0].metrics.ascent = Some(value);
        assert!(
            geos::write(
                &s,
                &WriteOptions::new(ResourcePath::new("f").unwrap()),
                &Default::default()
            )
            .is_err()
        );
    }
}

#[test]
fn native_cvt_signature_without_version_and_valid_date() {
    let mut b = write(&source(false), &Default::default()).unwrap();
    assert_eq!(&b[23..28], &[0, 1, 1, 0, 0]);
    b[53..58].fill(0);
    assert_eq!(read(&b, false, &Default::default()).unwrap().fonts.len(), 1);
}
