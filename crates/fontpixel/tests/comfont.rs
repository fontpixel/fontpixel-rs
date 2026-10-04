//! Seven native executable envelopes, malformed payloads and write/read limits.
#![cfg(feature = "comfont")]
use fontpixel::formats::{WriteOptions, comfont::*};
use fontpixel::resources::ReadContext;
use fontpixel::{FontSet, Input, ResourceLimits, ResourcePath};
fn source() -> FontSet {
    let mut f = fontpixel::Font::default();
    for i in 0..256u32 {
        let mut g = fontpixel::Glyph::new(
            fontpixel::Bitmap::<fontpixel::bitmap::Mono>::from_raw(
                8,
                8,
                &[i as u8; 8],
                Default::default(),
            )
            .unwrap(),
        );
        g.codepoints.push(fontpixel::model::Codepoint::Legacy(i));
        g.metrics.advance = Some(fontpixel::model::Vector::new(8, 0));
        f.add_glyph(g).unwrap();
    }
    f.update_bounds().unwrap();
    f.into()
}
fn parse(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    read(
        &mut ReadContext::new(&Input::new(b), l)?,
        &Default::default(),
    )
}
#[test]
fn all_seven_native_payloads_and_stable_rewrite() {
    for variant in [
        ComfontVariant::Fontedit,
        ComfontVariant::Frapt,
        ComfontVariant::FraptTsr,
        ComfontVariant::Letafont,
        ComfontVariant::Mania,
        ComfontVariant::Psfcom,
        ComfontVariant::Udg,
    ] {
        let p = ComfontWriteOptions {
            variant,
            ..Default::default()
        };
        let o = WriteOptions::new(ResourcePath::new("font.com").unwrap());
        let w = write(&source(), &o, &p).unwrap();
        let data = w.output.into_single().unwrap();
        let (s, _) = parse(&data, &Default::default()).unwrap();
        assert_eq!(s.fonts[0].len(), 256);
        let w = write(&s, &o, &p).unwrap();
        assert!(!w.report.has_loss(), "{variant:?}: {}", w.report);
        assert_eq!(data, w.output.into_single().unwrap());
        for cut in [0, 1, 14, 24, data.len() / 2] {
            assert!(parse(&data[..cut], &Default::default()).is_err());
        }
    }
}
#[test]
fn same_limits_and_bad_header() {
    let s = source();
    let o = WriteOptions::new(ResourcePath::new("f.com").unwrap());
    for cap in [1024, 65536, 1048576, 4194304] {
        let limits = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = write(
            &s,
            &o,
            &ComfontWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            assert!(parse(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    let mut b = write(&s, &o, &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    b[21] = 0;
    assert!(parse(&b, &Default::default()).is_err());
    b[21] = 255;
    assert!(parse(&b, &Default::default()).is_err());
}
#[test]
fn explicit_variant_and_strict_projection() {
    let s = source();
    let mut o = WriteOptions::new(ResourcePath::new("f.com").unwrap());
    let b = write(&s, &o, &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(
        read(
            &mut ReadContext::new(&Input::new(&b), &Default::default()).unwrap(),
            &ComfontReadOptions {
                variant: Some(ComfontVariant::Mania),
                ..Default::default()
            }
        )
        .is_err()
    );
    o.strict = true;
    assert!(matches!(
        write(&s, &o, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn sixteen_row_installation_projection_is_explicit() {
    let s = source();
    let mut o = WriteOptions::new(ResourcePath::new("f.com").unwrap());
    let options = ComfontWriteOptions {
        variant: ComfontVariant::Psfcom,
        cell: Some(fontpixel::bitmap::Bounds::new(8, 16, 0, 0)),
        ..Default::default()
    };
    let w = write(&s, &o, &options).unwrap();
    assert!(w.report.to_string().contains("installation.scanlines"));
    o.strict = true;
    assert!(matches!(
        write(&s, &o, &options),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
