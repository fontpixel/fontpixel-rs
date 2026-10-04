//! Original DAVIC/FreeType fixtures, metric and bounded-output contracts.
#![cfg(feature = "pfr")]
use fontpixel::formats::pfr::{self, PfrCompression, PfrDialect, PfrWriteOptions};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
const FT: &[u8] = include_bytes!("pfr/pfr-ft-0-0-1.pfr");
const DAVIC: &[u8] = include_bytes!("pfr/pfr-1-1-0.pfr");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    let i = Input::new(b);
    let mut c = ReadContext::new(&i, l)?;
    pfr::read(&mut c, &Default::default())
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.pfr").unwrap())
}
#[test]
fn original_native_fields_and_strict() {
    for source in [FT, DAVIC] {
        let (s, r) = read(source, &Default::default()).unwrap();
        assert!(!r.has_loss());
        let g = s.fonts[0].glyphs().next().unwrap().1;
        assert_eq!(
            (
                g.metrics.bounds.x,
                g.metrics.bounds.y,
                g.bitmap.width(),
                g.bitmap.height()
            ),
            (-2, -1, 9, 5)
        );
        assert_eq!(g.metrics.advance.unwrap().x, 11);
        assert_eq!(
            g.bitmap.as_bytes(),
            [145, 128, 65, 0, 50, 0, 132, 0, 80, 128]
        );
        let mut o = options();
        o.strict = true;
        let w = pfr::write(&s, &o, &Default::default()).unwrap();
        assert!(!w.report.has_loss());
        assert_eq!(
            read(&w.output.into_single().unwrap(), &Default::default())
                .unwrap()
                .0,
            s
        );
    }
}
#[test]
fn all_compressions_polarities_orders_and_dialects() {
    let (s, _) = read(FT, &Default::default()).unwrap();
    for compression in [
        PfrCompression::Raw,
        PfrCompression::Rle4,
        PfrCompression::Rle8,
    ] {
        for top_down in [false, true] {
            for black_pixel in [false, true] {
                for dialect in [PfrDialect::Davic, PfrDialect::FreeType] {
                    let p = PfrWriteOptions {
                        compression,
                        top_down,
                        black_pixel,
                        dialect: Some(dialect),
                        ..Default::default()
                    };
                    let w = pfr::write(&s, &options(), &p).unwrap();
                    let (back, _) = read(&w.output.into_single().unwrap(), &p.limits).unwrap();
                    let original = s.fonts[0].glyphs().next().unwrap().1;
                    let g = back.fonts[0].glyphs().next().unwrap().1;
                    assert_eq!(original, g);
                }
            }
        }
    }
}
#[test]
fn multistrike_and_multifont_native() {
    let (s, r) = read(
        include_bytes!("pfr/pfr-ft-multistrike.pfr"),
        &Default::default(),
    )
    .unwrap();
    assert!(!r.has_loss());
    assert_eq!(s.fonts.len(), 2);
    assert_eq!(s.fonts[0].metrics.pixel_size, Some(12));
    assert_eq!(s.fonts[1].metrics.pixel_size, Some(13));
    let w = pfr::write(&s, &options(), &Default::default()).unwrap();
    let back = read(&w.output.into_single().unwrap(), &Default::default())
        .unwrap()
        .0;
    assert_eq!(back.fonts.len(), 2);
    for (i, f) in back.fonts.iter().enumerate() {
        assert_eq!(
            f.glyphs().next().unwrap().1,
            s.fonts[i].glyphs().next().unwrap().1
        );
    }
}
#[test]
fn fractional_metrics_preserved_and_reported() {
    let (s, r) = read(
        include_bytes!("pfr/pfr-fractional.pfr"),
        &Default::default(),
    )
    .unwrap();
    assert!(r.has_loss());
    let g = s.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(g.metrics.bounds.x, -2);
    assert_eq!(g.metrics.advance.unwrap().x, 8);
    assert!(g.properties.get("pfr.fractional-metrics").is_some());
    let mut o = options();
    o.strict = true;
    assert!(matches!(
        pfr::write(&s, &o, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn every_metadata_category_is_audited() {
    let (mut s, _) = read(FT, &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.name = Some("Unencoded name".into());
            g.unicode_sequences.push(vec![65, 66]);
            g.codepoints.push(fontpixel::model::Codepoint::Legacy(66));
            g.metrics.vertical_origin = Some(fontpixel::model::Vector::new(2, 3));
            g.properties
                .push(fontpixel::model::Property::new("original-field", "test"));
            Ok(())
        })
        .unwrap();
    let w = pfr::write(&s, &options(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut o = options();
    o.strict = true;
    assert!(matches!(
        pfr::write(&s, &o, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn truncations_boundaries_and_limits() {
    for n in 0..FT.len() {
        assert!(read(&FT[..n], &Default::default()).is_err(), "{n}");
    }
    for limit in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 44,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: FT.len() + 1,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 8,
            ..Default::default()
        },
        ResourceLimits {
            max_resources: 0,
            ..Default::default()
        },
    ] {
        assert!(read(FT, &limit).is_err());
    }
    let (s, _) = read(FT, &Default::default()).unwrap();
    assert!(
        pfr::write(
            &s,
            &options(),
            &PfrWriteOptions {
                max_output_bytes: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    for total in [1024, 4096, 16384, 65536] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(w) = pfr::write(
            &s,
            &options(),
            &PfrWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            read(&w.output.into_single().unwrap(), &l).unwrap();
        }
    }
}
#[test]
fn mixed_outline_is_explicit_loss() {
    let mut source = FT.to_vec();
    let phyoff = u32::from_be_bytes([0, source[27], source[28], source[29]]) as usize;
    let size = u16::from_be_bytes([source[22], source[23]]) as usize;
    source[phyoff + size - 3] = 1;
    let (s, r) = read(&source, &Default::default()).unwrap();
    assert_eq!(s.fonts.len(), 1);
    assert!(r.entries.iter().any(|e| e.code == "pfr.outlines-omitted"));
}
#[test]
fn invalid_section_detector_and_zero_progress_run() {
    assert_eq!(pfr::detect(FT), fontpixel::formats::Confidence::Certain);
    let mut b = FT.to_vec();
    b[35..38].copy_from_slice(&[0, 0, 1]);
    assert!(read(&b, &Default::default()).is_err());
    assert_eq!(pfr::detect(&b), fontpixel::formats::Confidence::NoMatch);
    let mut b = DAVIC.to_vec();
    let off = u32::from_be_bytes([0, b[35], b[36], b[37]]) as usize;
    b[off + 6] = 0;
    assert!(read(&b, &Default::default()).is_err());
}
#[test]
fn physical_records_larger_than_64k_and_sparse_16bit_codes() {
    let (source, _) = read(FT, &Default::default()).unwrap();
    let mut font = fontpixel::Font::default();
    font.metrics.pixel_size = Some(12);
    let original = source.fonts[0].glyphs().next().unwrap().1;
    for code in 0..7400 {
        let mut g = original.clone();
        g.codepoints = vec![fontpixel::model::Codepoint::Legacy(code)];
        font.add_glyph(g).unwrap();
    }
    let s = FontSet::from(font);
    let w = pfr::write(&s, &options(), &Default::default()).unwrap();
    let data = w.output.into_single().unwrap();
    assert_ne!(data[41], 0);
    let back = read(&data, &Default::default()).unwrap().0;
    assert_eq!(back.fonts[0].len(), 7400);
    assert_eq!(
        back.fonts[0].glyphs().last().unwrap().1.codepoints[0].value(),
        7399
    );
}
