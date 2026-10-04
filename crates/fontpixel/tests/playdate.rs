//! Original glyphs compiled by the public Playdate SDK, plus bounded native writes.
#![cfg(feature = "playdate")]
use fontpixel::bitmap::{Bitmap, Mono};
use fontpixel::formats::playdate::{self, PlaydateFlavor, PlaydateWriteOptions};
use fontpixel::model::{Codepoint, Font, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
const FNT: &[u8] = include_bytes!("fixtures/playdate/original.fnt");
const RAW: &[u8] = include_bytes!("fixtures/playdate/sdk-3.1.2-uncompressed.pft");
const ZLIB: &[u8] = include_bytes!("fixtures/playdate/sdk-3.1.2-compressed.pft");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    playdate::read(&mut ReadContext::new(&i, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("fixture.fnt").unwrap())
}
#[test]
fn official_sdk_pages_unicode_kerning_all_pixels() {
    for b in [RAW, ZLIB] {
        let s = read(b, &Default::default()).unwrap();
        let f = s.single().unwrap();
        assert_eq!(f.len(), 5);
        assert_eq!(f.metrics.ascent, Some(8));
        for (i, cp) in [32, 65, 66, 256, 0x20000].into_iter().enumerate() {
            let g = f.glyph_for_codepoint(Codepoint::Unicode(cp)).unwrap();
            assert_eq!(g.metrics.advance.unwrap().x, [6, 9, 10, 8, 7][i]);
            for y in 0..8 {
                for x in 0..8 {
                    assert_eq!(
                        g.bitmap.as_bytes()[y] & (0x80 >> x) != 0,
                        (x * 3 + y * 5 + i) % 7 < 2,
                        "{cp}/{x}/{y}"
                    );
                }
            }
        }
        assert_eq!(
            f.properties.get("playdate.kerning").unwrap(),
            read(RAW, &Default::default())
                .unwrap()
                .single()
                .unwrap()
                .properties
                .get("playdate.kerning")
                .unwrap()
        );
        let mut o = options();
        o.strict = true;
        let out = playdate::write(
            &s,
            &o,
            &PlaydateWriteOptions {
                flavor: PlaydateFlavor::Pft,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!out.report.has_loss());
        assert_eq!(
            read(&out.output.into_single().unwrap(), &Default::default()).unwrap(),
            s
        );
    }
}
#[test]
fn source_caps_baseline_embedded_strict() {
    let s = read(FNT, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.names.family.as_deref(), Some("Original E1"));
    assert_eq!(f.metrics.ascent, Some(6));
    assert_eq!(f.metrics.descent, Some(2));
    let mut o = options();
    o.strict = true;
    let bytes = playdate::write(&s, &o, &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(read(&bytes, &Default::default()).unwrap(), s);
}
struct Assets(Vec<fontpixel::resources::OutputAsset>);
impl ResourceResolver for Assets {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = &self
            .0
            .iter()
            .find(|a| a.path == *p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .bytes;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test resource",
                limit: max,
            });
        }
        Ok(b.clone())
    }
    fn list(
        &self,
        _: Option<&ResourcePath>,
        count: usize,
        bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        assert!(self.0.len() <= count);
        assert!(self.0.iter().map(|a| a.path.as_str().len()).sum::<usize>() <= bytes);
        Ok(self.0.iter().map(|a| a.path.clone()).collect())
    }
}
#[test]
fn native_split_atlas_explicit_and_listed() {
    let s = read(FNT, &Default::default()).unwrap();
    let out = playdate::write(
        &s,
        &options(),
        &PlaydateWriteOptions {
            flavor: PlaydateFlavor::FntPng,
            ..Default::default()
        },
    )
    .unwrap();
    let assets = Assets(out.output.assets().to_vec());
    let src = &assets
        .0
        .iter()
        .find(|a| a.path.as_str() == "fixture.fnt")
        .unwrap()
        .bytes;
    let l = ResourceLimits::default();
    let input = Input::new(src)
        .with_name("fixture.fnt")
        .with_resources(&assets);
    assert_eq!(
        playdate::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &Default::default()
        )
        .unwrap()
        .0,
        s
    );
    let no_dimensions = String::from_utf8(src.clone())
        .unwrap()
        .lines()
        .filter(|s| !s.starts_with("width=") && !s.starts_with("height="))
        .collect::<Vec<_>>()
        .join("\n");
    let input = Input::new(no_dimensions.as_bytes())
        .with_name("fixture.fnt")
        .with_resources(&assets);
    assert_eq!(
        playdate::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &Default::default()
        )
        .unwrap()
        .0,
        s
    );
}
#[test]
fn full_page_256_glyph_count_wrap() {
    let mut f = Font::default();
    f.metrics.ascent = Some(1);
    f.metrics.line_height = Some(1);
    for cp in 0..256 {
        let mut g = Glyph::new(
            Bitmap::<Mono>::from_raw(
                1,
                1,
                &[if cp % 2 == 0 { 128 } else { 0 }],
                Default::default(),
            )
            .unwrap(),
        );
        g.codepoints.push(Codepoint::Unicode(cp));
        g.metrics.advance = Some(Vector::new(1, 0));
        f.add_glyph(g).unwrap();
    }
    let out = playdate::write(
        &f.into(),
        &options(),
        &PlaydateWriteOptions {
            flavor: PlaydateFlavor::Pft,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        read(&out.output.into_single().unwrap(), &Default::default())
            .unwrap()
            .single()
            .unwrap()
            .len(),
        256
    );
}
#[test]
fn malformed_pft_and_aggregate_limits() {
    for b in [RAW, ZLIB] {
        for n in 0..b.len() {
            assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
        }
    }
    let mut bad = RAW.to_vec();
    bad[12] = 2;
    assert!(read(&bad, &Default::default()).is_err());
    let s = read(FNT, &Default::default()).unwrap();
    for flavor in [PlaydateFlavor::FntEmbedded, PlaydateFlavor::Pft] {
        for total in [0, 8192, 65536, 1024 * 1024] {
            let l = ResourceLimits {
                max_total_bytes: total,
                ..Default::default()
            };
            if let Ok(out) = playdate::write(
                &s,
                &options(),
                &PlaydateWriteOptions {
                    limits: l.clone(),
                    flavor,
                    ..Default::default()
                },
            ) {
                assert!(read(&out.output.into_single().unwrap(), &l).is_ok());
            }
        }
    }
}

#[test]
fn registry_dispatch_and_strict_metadata_loss() {
    let bytes = FNT.to_vec();
    let registry = fontpixel::registry();
    let mut loaded = registry
        .read(
            &Input::new(&bytes),
            &fontpixel::ReadOptions {
                format: Some("playdate".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(loaded.fonts, read(&bytes, &Default::default()).unwrap());
    loaded.fonts.fonts[0]
        .properties
        .push(fontpixel::model::Property::new(
            "e1.unrepresentable",
            123i64,
        ));
    let mut options = options();
    let output = registry.write(&loaded.fonts, "playdate", &options).unwrap();
    assert!(output.report.has_loss());
    options.strict = true;
    assert!(registry.write(&loaded.fonts, "playdate", &options).is_err());
}

#[test]
fn sixteen_bit_png_reports_precision_loss_and_extreme_metrics_fail() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::GrayscaleAlpha);
        encoder.set_depth(png::BitDepth::Sixteen);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0, 0, 0x80, 1]).unwrap();
    }
    let assets = Assets(vec![fontpixel::resources::OutputAsset {
        path: ResourcePath::new("sample-table-1-1.png").unwrap(),
        bytes,
    }]);
    let input = Input::new(b"width=1\nheight=1\ntracking=0\nA 1\n")
        .with_name("sample.fnt")
        .with_resources(&assets);
    let limits = ResourceLimits::default();
    let (font, report) = playdate::read(
        &mut ReadContext::new(&input, &limits).unwrap(),
        &Default::default(),
    )
    .unwrap();
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "playdate.read.png_depth")
    );
    assert_eq!(
        font.single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[128]
    );
    let mut source = read(FNT, &Default::default()).unwrap();
    source.fonts[0].metrics.ascent = Some(i64::MAX);
    assert!(playdate::write(&source, &options(), &Default::default()).is_err());
    source.fonts[0].metrics.ascent = Some(6);
    let id = source.fonts[0].glyph_id(Codepoint::Unicode(65)).unwrap();
    source.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.advance = Some(Vector::new(i64::MIN, 0));
            Ok(())
        })
        .unwrap();
    assert!(playdate::write(&source, &options(), &Default::default()).is_err());
}
