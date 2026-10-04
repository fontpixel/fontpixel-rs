//! Original PILfont metrics and PNG masks; native multi-resource behavior.
#![cfg(feature = "pilfont")]
use fontpixel::formats::pilfont::{self, PilfontReadOptions, PilfontWriteOptions};
use fontpixel::resources::{Input, OutputAsset, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
use image_codec::ImageEncoder;
struct Assets(Vec<OutputAsset>);
impl ResourceResolver for Assets {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let bytes = &self
            .0
            .iter()
            .find(|a| a.path == *p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .bytes;
        if bytes.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test mask",
                limit: max,
            });
        }
        Ok(bytes.clone())
    }
}
fn fixture() -> Assets {
    let mut metrics = b"PILfont\n;;;;;;7;\nOriginal info\nDATA\n".to_vec();
    let start = metrics.len();
    metrics.resize(start + 5120, 0);
    for (cp, record) in [
        (32, [4, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (65, [5, 1, -1, -4, 2, -2, 0, 0, 3, 2]),
    ] {
        for (i, n) in record.into_iter().enumerate() {
            metrics[start + cp * 20 + i * 2..start + cp * 20 + i * 2 + 2]
                .copy_from_slice(&i16::to_be_bytes(n));
        }
    }
    let mut png = Vec::new();
    image_codec::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[0, 17, 255, 64, 128, 240],
            3,
            2,
            image_codec::ExtendedColorType::L8,
        )
        .unwrap();
    Assets(vec![
        OutputAsset {
            path: ResourcePath::new("original.pil").unwrap(),
            bytes: metrics,
        },
        OutputAsset {
            path: ResourcePath::new("original.pbm").unwrap(),
            bytes: png,
        },
    ])
}
fn read(assets: &Assets, l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(
        &assets
            .0
            .iter()
            .find(|a| a.path.as_str() == "original.pil")
            .unwrap()
            .bytes,
    )
    .with_name("original.pil")
    .with_resources(assets);
    pilfont::read(&mut ReadContext::new(&input, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("original.pil").unwrap())
}
#[test]
fn signed_metrics_blank_slot_gray_mask_and_strict() {
    let a = fixture();
    let s = read(&a, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 2);
    let g = f
        .glyph_for_codepoint(fontpixel::model::Codepoint::Legacy(65))
        .unwrap();
    assert_eq!((g.metrics.bounds.x, g.metrics.bounds.y), (-1, 2));
    assert_eq!(g.metrics.advance.unwrap().y, -1);
    assert_eq!(g.bitmap.as_bytes(), [0, 17, 255, 64, 128, 240]);
    let mut o = options();
    o.strict = true;
    let out = pilfont::write(&s, &o, &Default::default()).unwrap();
    assert_eq!(
        read(&Assets(out.output.assets().to_vec()), &Default::default()).unwrap(),
        s
    );
}
#[test]
fn missing_resources_truncations_rectangles_and_limits() {
    let a = fixture();
    for n in 0..a.0[0].bytes.len() {
        let mut bad = fixture();
        bad.0[0].bytes.truncate(n);
        assert!(read(&bad, &Default::default()).is_err(), "{n}");
    }
    let l = ResourceLimits {
        max_resources: 1,
        ..Default::default()
    };
    assert!(read(&a, &l).is_err());
    let input = Input::new(&a.0[0].bytes).with_resources(&a);
    assert!(
        pilfont::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &PilfontReadOptions {
                atlas: Some(ResourcePath::new("original.pbm").unwrap())
            }
        )
        .is_ok()
    );
    let s = read(&a, &Default::default()).unwrap();
    assert!(
        pilfont::write(
            &s,
            &options(),
            &PilfontWriteOptions {
                limits: l,
                ..Default::default()
            }
        )
        .is_err()
    );
    for cap in [0, 10000, 65536, 1000000] {
        let l = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(out) = pilfont::write(
            &s,
            &options(),
            &PilfontWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&Assets(out.output.assets().to_vec()), &l).is_ok());
        }
    }
    let mut bad = fixture();
    let start = bad.0[0].bytes.len() - 5120;
    bad.0[0].bytes[start + 65 * 20 + 16..start + 65 * 20 + 18].copy_from_slice(&4i16.to_be_bytes());
    assert!(read(&bad, &Default::default()).is_err());
}
#[test]
fn registry_loss_and_duplicate_determinism() {
    let a = fixture();
    let mut s = read(&a, &Default::default()).unwrap();
    let registry = fontpixel::registry();
    let mut o = options();
    o.strict = true;
    let out = registry.write(&s, "pilfont", &o).unwrap();
    assert_eq!(registry.write(&s, "pilfont", &o).unwrap(), out);
    s.fonts[0].names.family = Some("unrepresentable".into());
    assert!(registry.write(&s, "pilfont", &o).is_err());
}

#[test]
fn output_name_does_not_shadow_native_atlas_lookup() {
    let s = read(&fixture(), &Default::default()).unwrap();
    for name in ["font.png", "font.gif", "font.pbm"] {
        assert!(
            pilfont::write(
                &s,
                &WriteOptions::new(ResourcePath::new(name).unwrap()),
                &Default::default()
            )
            .is_err()
        );
    }
    let hidden = pilfont::write(
        &s,
        &WriteOptions::new(ResourcePath::new(".pil").unwrap()),
        &Default::default(),
    )
    .unwrap();
    assert!(
        hidden
            .output
            .assets()
            .iter()
            .any(|a| a.path.as_str() == ".pil.pbm")
    );
    let out = pilfont::write(
        &s,
        &WriteOptions::new(ResourcePath::new("dir.with.dots/font").unwrap()),
        &Default::default(),
    )
    .unwrap();
    assert!(
        out.output
            .assets()
            .iter()
            .any(|a| a.path.as_str() == "dir.with.dots/font.pbm")
    );
}
