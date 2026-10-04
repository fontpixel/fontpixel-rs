//! Original control-pixel fixtures and bounded native readback contracts.
#![cfg(feature = "sfont")]
use fontpixel::formats::sfont::{self, SfontFlavor, SfontReadOptions, SfontWriteOptions};
use fontpixel::model::{Codepoint, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
use image_codec::ImageEncoder;
fn read(bytes: &[u8], flavor: SfontFlavor, l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(bytes).with_name("font.png");
    sfont::read(
        &mut ReadContext::new(&input, l)?,
        &SfontReadOptions {
            flavor: Some(flavor),
            ..Default::default()
        },
    )
    .map(|x| x.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.png").unwrap())
}
fn original_sfont() -> Vec<u8> {
    let w = 10;
    let h = 4;
    let mut p = vec![0; w * h * 4];
    for x in [0, 1, 4, 5, 8, 9] {
        p[x * 4..x * 4 + 4].copy_from_slice(&[255, 0, 255, 255]);
    }
    for (x, y, v) in [(2, 1, 255), (3, 2, 17), (6, 1, 128), (7, 3, 255)] {
        let i = (y * w + x) * 4;
        p[i..i + 4].copy_from_slice(&[255, 255, 255, v]);
    }
    let mut png = Vec::new();
    image_codec::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &p,
            w as u32,
            h as u32,
            image_codec::ExtendedColorType::Rgba8,
        )
        .unwrap();
    png
}
#[test]
fn sfont_markers_explicit_order_gray_and_strict() {
    let bytes = original_sfont();
    let s = read(&bytes, SfontFlavor::Sfont, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 2);
    let g = f.glyph_for_codepoint(Codepoint::Unicode(33)).unwrap();
    assert_eq!(
        (
            g.bitmap.width(),
            g.bitmap.height(),
            g.metrics.bounds.x,
            g.metrics.bounds.y
        ),
        (4, 3, -1, 0)
    );
    assert_eq!(g.metrics.advance.unwrap().x, 2);
    assert_eq!(g.bitmap.as_bytes(), [0, 255, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0]);
    let v = SfontWriteOptions {
        codepoints: Some(vec![Codepoint::Unicode(33), Codepoint::Unicode(34)]),
        ..Default::default()
    };
    let mut o = options();
    o.strict = true;
    let out = sfont::write(&s, &o, &v).unwrap();
    assert_eq!(
        read(
            &out.output.assets()[0].bytes,
            SfontFlavor::Sfont,
            &Default::default()
        )
        .unwrap(),
        s
    );
    let changed = sfont::write(&s, &options(), &Default::default()).unwrap();
    assert!(changed.report.has_loss());
    o.strict = true;
    assert!(matches!(
        sfont::write(&s, &o, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn rfont_external_original_pages_redundancy_aux_versions_and_gap_stability() {
    let bytes = include_bytes!("fixtures/w5e2/rfont-original.png");
    let mut s = read(bytes, SfontFlavor::Rfont, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 3);
    assert_eq!(f.names.family.as_deref(), Some("Original é"));
    assert_eq!(
        (
            f.metrics.ascent,
            f.metrics.descent,
            f.metrics.pixel_size,
            f.metrics.line_gap
        ),
        (Some(7), Some(3), Some(7), Some(1))
    );
    for version in [1, 2] {
        s.fonts[0].properties.set("rfont.vm_version", version);
        s.fonts[0].properties.set("rfont.hm_version", version);
        let mut o = options();
        o.strict = true;
        let v = SfontWriteOptions {
            flavor: SfontFlavor::Rfont,
            ..Default::default()
        };
        let out = sfont::write(&s, &o, &v).unwrap();
        let target = read(
            &out.output.assets()[0].bytes,
            SfontFlavor::Rfont,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(target, s);
        assert_eq!(sfont::write(&target, &o, &v).unwrap().output, out.output);
    }
    s.fonts[0].properties.push(Property::new("unknown", "lost"));
    let mut o = options();
    o.strict = true;
    assert!(matches!(
        sfont::write(
            &s,
            &o,
            &SfontWriteOptions {
                flavor: SfontFlavor::Rfont,
                ..Default::default()
            }
        ),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn malformed_images_codepoints_options_and_budgets() {
    let bytes = include_bytes!("fixtures/w5e2/rfont-original.png");
    for n in 0..bytes.len() {
        assert!(
            read(&bytes[..n], SfontFlavor::Rfont, &Default::default()).is_err(),
            "{n}"
        );
    }
    let mut bad = bytes.to_vec();
    let at = bad.windows(4).position(|p| p == b"rfVM").unwrap();
    bad[at + 5] ^= 1;
    assert!(read(&bad, SfontFlavor::Rfont, &Default::default()).is_err());
    let s = read(bytes, SfontFlavor::Rfont, &Default::default()).unwrap();
    for cap in [0, 10000, 65536, 1000000] {
        let l = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(out) = sfont::write(
            &s,
            &options(),
            &SfontWriteOptions {
                flavor: SfontFlavor::Rfont,
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&out.output.assets()[0].bytes, SfontFlavor::Rfont, &l).is_ok());
        }
    }
    let input = Input::new(bytes);
    let l = ResourceLimits::default();
    assert!(
        sfont::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &SfontReadOptions {
                flavor: Some(SfontFlavor::Rfont),
                baseline: Some(5),
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn native_png_names_and_unknown_text_roundtrip_and_baseline_overflow() {
    let bytes = original_sfont();
    let mut s = read(&bytes, SfontFlavor::Sfont, &Default::default()).unwrap();
    s.fonts[0].names.family = Some("Original é".into());
    s.fonts[0].names.style = Some("Regular".into());
    s.fonts[0].names.copyright = Some("MIT original".into());
    s.fonts[0].properties.push(Property::new(
        "sfont.png_text",
        fontpixel::model::PropertyValue::Bytes(b"Comment\0Original opaque metadata".to_vec()),
    ));
    let v = SfontWriteOptions {
        codepoints: Some(vec![Codepoint::Unicode(33), Codepoint::Unicode(34)]),
        ..Default::default()
    };
    let mut o = options();
    o.strict = true;
    let out = sfont::write(&s, &o, &v).unwrap();
    assert_eq!(
        read(
            &out.output.assets()[0].bytes,
            SfontFlavor::Sfont,
            &Default::default()
        )
        .unwrap(),
        s
    );
    s.fonts[0].names.family = Some("Unsupported 🙂".into());
    assert!(matches!(
        sfont::write(&s, &o, &v),
        Err(fontpixel::Error::StrictLoss(_))
    ));
    let input = Input::new(&bytes);
    let l = ResourceLimits::default();
    assert!(
        sfont::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &SfontReadOptions {
                baseline: Some(i64::MIN),
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[cfg(feature = "image")]
#[test]
fn registry_distinguishes_native_rfont_evidence_from_ambiguous_image_magic() {
    let registry = fontpixel::registry();
    let l = ResourceLimits::default();
    let native = include_bytes!("fixtures/w5e2/rfont-original.png");
    let d = registry
        .detect(&Input::new(native).with_name("misleading.bmp"), &l)
        .unwrap();
    assert_eq!(d.format.id, "sfont");
    assert_eq!(d.confidence, fontpixel::formats::Confidence::Certain);
    let plain = original_sfont();
    let result = registry.detect(&Input::new(&plain).with_name("misleading.pil"), &l);
    assert!(
        matches!(result,Err(fontpixel::Error::AmbiguousFormat{candidates})if candidates.iter().any(|s|s=="image")&&candidates.iter().any(|s|s=="sfont"))
    );
}
