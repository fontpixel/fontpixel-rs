//! Native psf2txt bounded parsing and complete loss-contract tests.
#![cfg(feature = "psf2txt")]
use fontpixel::formats::psf2txt::{self, Psf2txtWriteOptions};
use fontpixel::model::{FontSet, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
include!("dosstart/fixtures.rs");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(psf2txt::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.psf2txt").unwrap())
}
#[test]
fn original_wire_roundtrip_and_strict() {
    let b = fixture("psf2txt");
    let set = read(&b, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap.as_bytes()[0],
        if "psf2txt" == "dosstart"
            || "psf2txt" == "edwin"
            || "psf2txt" == "mkwinfont"
            || "psf2txt" == "psf2txt"
        {
            0xa0
        } else {
            0xa5
        }
    );
    let mut options = opts();
    options.strict = true;
    let w = psf2txt::write(&set, &options, &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let reread = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(set, reread);
}
#[test]
fn metadata_loss_is_audited_and_strict_rejects() {
    let mut set = read(&fixture("psf2txt"), &Default::default()).unwrap();
    let id = set.fonts[0].glyphs().next().unwrap().0;
    set.fonts[0]
        .edit_glyph(id, |g| {
            g.name = Some("extra-name".into());
            g.properties
                .push(Property::new("unrepresentable", "keep me"));
            g.metrics.vertical_origin = Some(fontpixel::model::Vector::new(1, 2));
            Ok(())
        })
        .unwrap();
    let w = psf2txt::write(&set, &opts(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut options = opts();
    options.strict = true;
    assert!(matches!(
        psf2txt::write(&set, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn cumulative_limits_and_output_reread_contract() {
    let b = fixture("psf2txt");
    let set = read(&b, &Default::default()).unwrap();
    for limits in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: b.len() + 1,
            ..Default::default()
        },
    ] {
        assert!(read(&b, &limits).is_err());
    }
    for total in [1024, 8192, 131072, 1048576] {
        let limits = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        let options = Psf2txtWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        };
        if let Ok(w) = psf2txt::write(&set, &opts(), &options) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    assert!(
        psf2txt::write(
            &set,
            &opts(),
            &Psf2txtWriteOptions {
                max_output_bytes: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn invalid_signature_and_short_inputs_do_not_panic() {
    assert!(read(&[], &Default::default()).is_err());
    assert!(read(b"not a bitmap font", &Default::default()).is_err());
    let b = fixture("psf2txt");
    for n in 0..b.len().min(24) {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
}
#[test]
fn unicode_alias_sequence_and_positional_slot_survive() {
    use fontpixel::model::Codepoint;
    let s = read(&fixture("psf2txt"), &Default::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Unicode(65))
        .unwrap();
    assert_eq!(
        g.codepoints,
        [Codepoint::Unicode(65), Codepoint::Unicode(0x391)]
    );
    assert_eq!(g.unicode_sequences, [vec![0x61, 0x301]]);
    assert!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(1))
            .is_some()
    );
    let bad = String::from_utf8(fixture("psf2txt"))
        .unwrap()
        .replace("0041", "d800");
    assert!(read(bad.as_bytes(), &Default::default()).is_err());
    let bad = String::from_utf8(fixture("psf2txt"))
        .unwrap()
        .replace("Length: 2", "Length: 3");
    assert!(read(bad.as_bytes(), &Default::default()).is_err());
}
#[test]
fn grayscale_requires_threshold_and_reports_quantization() {
    let mut s = read(&fixture("psf2txt"), &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            if let fontpixel::model::GlyphBitmap::Mono(b) = &g.bitmap {
                let mut gray = b.to_gray8()?;
                gray.set_pixel(0, 0, 100)?;
                g.bitmap = gray.into();
            }
            Ok(())
        })
        .unwrap();
    assert!(psf2txt::write(&s, &opts(), &Default::default()).is_err());
    let p = Psf2txtWriteOptions {
        gray_threshold: Some(128),
        ..Default::default()
    };
    let out = psf2txt::write(&s, &opts(), &p).unwrap();
    assert!(out.report.has_loss());
    let back = read(&out.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(
        back.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes()[0],
        0x20
    );
    let mut o = opts();
    o.strict = true;
    assert!(matches!(
        psf2txt::write(&s, &o, &p),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn utf8_bom_and_native_comments_are_preserved() {
    let b="\u{feff}%PSF2\nVersion: 0\nFlags: 0\nLength: 1\nWidth: 1\nHeight: 1\n// 注释\n%\nBitmap: #\n// glyph 字形\n".as_bytes();
    assert_eq!(psf2txt::detect(b), fontpixel::formats::Confidence::Certain);
    let s = read(b, &Default::default()).unwrap();
    let mut o = opts();
    o.strict = true;
    let w = psf2txt::write(&s, &o, &Default::default()).unwrap();
    assert_eq!(
        read(&w.output.into_single().unwrap(), &Default::default()).unwrap(),
        s
    );
}
