//! Original numeric-name glyph directories; no mapping document is involved.
#![cfg(feature = "image")]
use fontpixel::formats::image::{
    Encoding, ImageCodec, ImageLayout, ImageReadOptions, ImageSetBackground, ImageSetDomain,
    ImageSetOptions, ImageWriteOptions,
};
use fontpixel::formats::{FormatReader, FormatWriter};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{FontSet, ReadOptions, ResourceLimits, WriteOptions};
use image_codec::ImageEncoder;
use std::collections::BTreeMap;
struct Directory(BTreeMap<String, Vec<u8>>);
impl ResourceResolver for Directory {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = self
            .0
            .get(p.as_str())
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test file",
                limit: max,
            });
        }
        Ok(b.clone())
    }
    fn list(
        &self,
        p: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        assert!(p.is_none());
        if self.0.len() > max || self.0.keys().map(|s| s.len() * 2 + 64).sum::<usize>() > bytes {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test listing",
                limit: max,
            });
        }
        self.0.keys().rev().map(ResourcePath::new).collect()
    }
}
fn png(data: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image_codec::codecs::png::PngEncoder::new(&mut out)
        .write_image(data, w, h, image_codec::ExtendedColorType::L8)
        .unwrap();
    out
}
fn original() -> Directory {
    Directory(BTreeMap::from([
        ("0041.png".into(), png(&[0, 17, 255, 64, 128, 240], 3, 2)),
        ("0100.png".into(), png(&[255, 0, 0, 255], 2, 2)),
    ]))
}
fn read(d: &Directory, profile: ImageSetOptions, l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::directory("original", d);
    let mut ctx = ReadContext::new(&input, l)?;
    ImageCodec {
        read_options: ImageReadOptions {
            imageset: profile,
            ..Default::default()
        },
        ..Default::default()
    }
    .read(&mut ctx, &ReadOptions::default())
    .map(|x| x.0)
}
#[test]
fn native_directory_all_containers_labels_pixels_and_strict() {
    let s = read(&original(), Default::default(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 2);
    assert_eq!(
        f.glyph_for_codepoint(fontpixel::model::Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        [0, 17, 255, 64, 128, 240]
    );
    for enc in [Encoding::Png, Encoding::Bmp, Encoding::Tga, Encoding::Pgm] {
        for background in [ImageSetBackground::Black, ImageSetBackground::White] {
            let profile = ImageSetOptions {
                prefix: "cp".into(),
                radix: 10,
                background,
                ..Default::default()
            };
            let codec = ImageCodec {
                write_options: ImageWriteOptions {
                    layout: ImageLayout::Imageset,
                    encoding: Some(enc),
                    imageset: profile.clone(),
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut o = WriteOptions::new(ResourcePath::new("out").unwrap());
            o.strict = true;
            let w = codec.write(&s, &o).unwrap();
            assert!(w.output.is_directory());
            assert_eq!(w.output.assets().len(), 2);
            assert!(!w.report.has_loss());
            let d = Directory(
                w.output
                    .assets()
                    .iter()
                    .map(|a| {
                        (
                            a.path.as_str().strip_prefix("out/").unwrap().into(),
                            a.bytes.clone(),
                        )
                    })
                    .collect(),
            );
            assert_eq!(read(&d, profile, &Default::default()).unwrap(), s);
            assert_eq!(codec.write(&s, &o).unwrap().output, w.output);
        }
    }
    let codec = ImageCodec {
        write_options: ImageWriteOptions {
            layout: ImageLayout::Imageset,
            imageset: ImageSetOptions {
                background: ImageSetBackground::Alpha,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let out = codec
        .write(&s, &WriteOptions::new(ResourcePath::new("alpha").unwrap()))
        .unwrap();
    assert!(!out.report.has_loss());
}
#[test]
fn duplicate_labels_bad_names_resources_and_readback_budget() {
    let mut d = original();
    d.0.insert("41.png".into(), png(&[255], 1, 1));
    assert!(read(&d, Default::default(), &Default::default()).is_err());
    for name in ["xyz.png", "-1.png", "+1.png", "1.txt", "110000.png"] {
        let d = Directory(BTreeMap::from([(name.into(), png(&[255], 1, 1))]));
        assert!(
            read(
                &d,
                ImageSetOptions {
                    domain: ImageSetDomain::Unicode,
                    ..Default::default()
                },
                &Default::default()
            )
            .is_err(),
            "{name}"
        );
    }
    let s = read(&original(), Default::default(), &Default::default()).unwrap();
    for cap in [0, 1, 10000, 100000, 1000000] {
        let l = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        let codec = ImageCodec {
            write_options: ImageWriteOptions {
                layout: ImageLayout::Imageset,
                limits: l.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok(out) = codec.write(&s, &WriteOptions::new(ResourcePath::new("out").unwrap())) {
            let d = Directory(
                out.output
                    .assets()
                    .iter()
                    .map(|a| {
                        (
                            a.path.as_str().strip_prefix("out/").unwrap().into(),
                            a.bytes.clone(),
                        )
                    })
                    .collect(),
            );
            assert!(read(&d, Default::default(), &l).is_ok());
        }
    }
    let l = ResourceLimits {
        max_resources: 2,
        ..Default::default()
    };
    assert!(read(&original(), Default::default(), &l).is_err());
}
#[test]
fn absent_metrics_are_loss_and_directory_mapping_is_rejected() {
    let mut s = read(&original(), Default::default(), &Default::default()).unwrap();
    s.fonts[0].names.family = Some("unrepresentable".into());
    let codec = ImageCodec {
        write_options: ImageWriteOptions {
            layout: ImageLayout::Imageset,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut o = WriteOptions::new(ResourcePath::new("out").unwrap());
    o.strict = true;
    assert!(matches!(
        codec.write(&s, &o),
        Err(fontpixel::Error::StrictLoss(_))
    ));
    let d = original();
    let input = Input::directory("original", &d);
    let l = ResourceLimits::default();
    let mut ctx = ReadContext::new(&input, &l).unwrap();
    let codec = ImageCodec {
        read_options: ImageReadOptions {
            mapping: Some(Default::default()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(codec.read(&mut ctx, &ReadOptions::default()).is_err());
}

#[test]
fn explicit_color_and_sample_depth_projections_report_loss() {
    for (kind, data, profile, expected) in [
        (
            image_codec::ExtendedColorType::Rgba8,
            vec![255, 0, 0, 128],
            ImageSetBackground::Black,
            Some(38),
        ),
        (
            image_codec::ExtendedColorType::Rgba8,
            vec![255, 0, 0, 128],
            ImageSetBackground::Alpha,
            Some(128),
        ),
        (
            image_codec::ExtendedColorType::L16,
            vec![128, 0],
            ImageSetBackground::Black,
            None,
        ),
    ] {
        let mut png = Vec::new();
        image_codec::codecs::png::PngEncoder::new(&mut png)
            .write_image(&data, 1, 1, kind)
            .unwrap();
        let d = Directory(BTreeMap::from([("41.png".into(), png)]));
        let input = Input::directory("glyphs", &d);
        let l = ResourceLimits::default();
        let mut c = ReadContext::new(&input, &l).unwrap();
        let codec = ImageCodec {
            read_options: ImageReadOptions {
                imageset: ImageSetOptions {
                    background: profile,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let (set, report) = codec.read(&mut c, &ReadOptions::default()).unwrap();
        assert!(report.has_loss());
        if let Some(value) = expected {
            assert_eq!(
                set.fonts[0]
                    .glyph_for_codepoint(fontpixel::model::Codepoint::Legacy(65))
                    .unwrap()
                    .bitmap
                    .as_bytes(),
                [value]
            );
        }
    }
}
