//! Native metadata roundtrips and independently authored atlas failure fixtures.
#![cfg(feature = "image")]
use fontpixel::formats::image::{
    Cell, ImageCodec, ImageReadOptions, ImageWriteOptions, Mapping, MappingFormat,
};
use fontpixel::formats::{FormatReader, FormatWriter, ReadOptions, WriteOptions};
use fontpixel::model::{Codepoint, Property, PropertyValue, Vector};
use fontpixel::resources::{OutputBundle, ReadContext, ResourceResolver};
use fontpixel::{Bitmap, Error, Font, FontSet, Glyph, Input, ResourceLimits, ResourcePath};

struct Assets(OutputBundle);
impl ResourceResolver for Assets {
    fn resolve(&self, path: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let a = self
            .0
            .assets()
            .iter()
            .find(|a| &a.path == path)
            .ok_or_else(|| Error::MissingResource(path.to_string()))?;
        if a.bytes.len() > max {
            return Err(Error::ResourceLimit {
                resource: "fixture sidecar",
                limit: max,
            });
        }
        Ok(a.bytes.clone())
    }
}
fn sample(gray: bool) -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("Atlas \"家族\"\n\\test".into());
    f.names.license = Some("control:\u{000b} 😀".into());
    f.properties
        .push(Property::new("unsigned", PropertyValue::Unsigned(u64::MAX)));
    f.properties.push(Property::new(
        "list",
        PropertyValue::List(vec![
            PropertyValue::Flag,
            PropertyValue::Bytes(vec![0, 255]),
        ]),
    ));
    f.headers.push(Property {
        name: "raw".into(),
        value: "value".into(),
        raw: Some(vec![255, 0]),
    });
    let mut g = Glyph::new("#.......#\n.#######.\n.........".parse::<Bitmap>().unwrap());
    g.codepoints = vec![
        Codepoint::Unicode(65),
        Codepoint::Unicode(0x4e00),
        Codepoint::Legacy(32),
    ];
    g.unicode_sequences = vec![vec![65, 0x301]];
    g.name = Some(String::new());
    g.metrics.bounds.x = -2;
    g.metrics.bounds.y = -1;
    g.metrics.advance = Some(Vector::new(11, 0));
    let first = f.add_glyph(g).unwrap();
    let hole = f.add_glyph(Glyph::default()).unwrap();
    f.remove_glyph(hole);
    let mut empty = Glyph::default();
    empty.codepoints.push(Codepoint::Unicode(32));
    empty.metrics.advance = Some(Vector::new(4, 0));
    f.add_glyph(empty).unwrap();
    if gray {
        use fontpixel::bitmap::{BitmapLimits, Gray8};
        let g = Glyph::new(
            Bitmap::<Gray8>::from_raw(3, 2, &[0, 1, 127, 128, 254, 255], BitmapLimits::default())
                .unwrap(),
        );
        f.add_glyph(g).unwrap();
    }
    f.metrics.default_glyph = Some(first);
    f.update_bounds().unwrap();
    FontSet {
        fonts: vec![f],
        source_format: Some("test-container".into()),
        ..Default::default()
    }
}
fn read_bundle(assets: &Assets, entry: &str) -> fontpixel::Result<FontSet> {
    let bytes = assets
        .0
        .assets()
        .iter()
        .find(|a| a.path.as_str() == entry)
        .unwrap();
    let input = Input::new(&bytes.bytes)
        .with_name(entry)
        .with_resources(assets);
    let o = ReadOptions::default();
    Ok(ImageCodec::default()
        .read(&mut ReadContext::new(&input, &o.limits)?, &o)?
        .0)
}
#[test]
fn native_roundtrip_all_six_containers_both_mappings_and_entry_points() {
    for ext in ["png", "bmp", "gif", "tga", "pbm", "pgm"] {
        for mapping_format in [MappingFormat::Json, MappingFormat::Toml] {
            let src = sample(!matches!(ext, "gif" | "pbm"));
            let c = ImageCodec {
                write_options: ImageWriteOptions {
                    mapping_format,
                    columns: 2,
                    ..Default::default()
                },
                ..Default::default()
            };
            let name = format!("atlas.{ext}");
            let o = WriteOptions::new(ResourcePath::new(&name).unwrap());
            let a = c
                .write(&src, &o)
                .unwrap_or_else(|e| panic!("{ext} {mapping_format:?}: {e}"));
            let b = c.write(&src, &o).unwrap();
            assert_eq!(a.output, b.output);
            assert!(!a.report.has_loss());
            let assets = Assets(a.output);
            if ext == "tga" {
                let bytes = &assets
                    .0
                    .assets()
                    .iter()
                    .find(|a| a.path.as_str() == name)
                    .unwrap()
                    .bytes;
                assert_eq!(
                    bytes[2], 3,
                    "TGA stays uncompressed grayscale for independent consumers"
                );
                assert_eq!(bytes[16], 8);
            }
            assert_eq!(
                read_bundle(&assets, &name).unwrap(),
                src,
                "{ext} {mapping_format:?}"
            );
            let sidecar = format!(
                "atlas.{}",
                if mapping_format == MappingFormat::Json {
                    "json"
                } else {
                    "toml"
                }
            );
            assert_eq!(
                read_bundle(&assets, &sidecar).unwrap(),
                src,
                "{ext} mapping entry"
            );
        }
    }
}
fn external(
    bytes: &[u8],
    map: Mapping,
    limits: ResourceLimits,
) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    let c = ImageCodec {
        read_options: ImageReadOptions {
            mapping: Some(map),
            mapping_path: None,
        },
        ..Default::default()
    };
    let input = Input::new(bytes).with_name("atlas.pgm");
    let o = ReadOptions {
        limits,
        ..Default::default()
    };
    c.read(&mut ReadContext::new(&input, &o.limits)?, &o)
}
fn grid(chars: &str) -> Mapping {
    Mapping {
        pixel_size: Some(2),
        baseline: Some(1),
        cell_size: Some([2, 2]),
        characters: Some(chars.into()),
        foreground: vec![[255; 3]],
        ..Default::default()
    }
}
const PGM: &[u8] = b"P5\n4 2\n255\n\xff\x00\x00\xff\x00\xff\xff\x00";
#[test]
fn external_grid_preserves_order_pixels_and_baseline() {
    let (s, r) = external(PGM, grid("BA"), ResourceLimits::default()).unwrap();
    assert!(!r.has_loss());
    let gs: Vec<_> = s.fonts[0].glyphs().map(|(_, g)| g).collect();
    assert_eq!(gs[0].codepoints, [Codepoint::Unicode(65)]);
    assert_eq!(gs[0].bitmap.as_bytes(), &[0x40, 0x80]);
    assert_eq!(gs[1].bitmap.as_bytes(), &[0x80, 0x40]);
    assert_eq!(gs[0].metrics.bounds.y, -1);
    assert_eq!(s.fonts[0].metrics.ascent, Some(1));
    assert_eq!(s.fonts[0].metrics.descent, Some(1));
}
#[test]
fn external_rejects_unmapped_duplicate_overlapping_and_outside_cells() {
    for chars in ["A", "AA", "ABC", ""] {
        assert!(external(PGM, grid(chars), ResourceLimits::default()).is_err());
    }
    for cells in [
        vec![
            Cell::character([0, 0, 4, 2], 'A'),
            Cell::character([1, 0, 1, 1], 'B'),
        ],
        vec![Cell::character([3, 0, 2, 2], 'A')],
    ] {
        let map = Mapping {
            pixel_size: Some(2),
            cells: Some(cells),
            ..Default::default()
        };
        assert!(external(PGM, map, ResourceLimits::default()).is_err());
    }
}
#[test]
fn external_blank_omission_is_explicit_and_keeps_whitespace() {
    let mut m = grid(" A");
    m.omit_blank = true;
    let (s, r) = external(
        b"P5\n4 2\n255\n\0\0\0\0\0\0\0\0",
        m,
        ResourceLimits::default(),
    )
    .unwrap();
    assert_eq!(s.fonts[0].len(), 1);
    assert!(r.has_loss());
}
#[test]
fn budgets_cover_mapping_pixels_slots_and_source_metadata() {
    for limits in [
        ResourceLimits {
            max_total_bytes: 1024,
            ..Default::default()
        },
        ResourceLimits {
            max_decompressed_bytes: 31,
            ..Default::default()
        },
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 3,
            ..Default::default()
        },
    ] {
        assert!(external(PGM, grid("AB"), limits).is_err());
    }
    let mut m = grid("AB");
    m.name = Some("x".repeat(4096));
    assert!(
        external(
            PGM,
            m,
            ResourceLimits {
                max_table_bytes: 1024,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut s = sample(false);
    let mut v = PropertyValue::Flag;
    for _ in 0..40 {
        v = PropertyValue::List(vec![v]);
    }
    s.fonts[0].properties.push(Property::new("deep", v));
    assert!(
        ImageCodec::default()
            .write(&s, &WriteOptions::new(ResourcePath::new("a.png").unwrap()))
            .is_err()
    );
}
#[test]
fn gray_to_mono_requires_threshold_and_reports_loss() {
    let s = sample(true);
    for ext in ["gif", "pbm"] {
        let o = WriteOptions::new(ResourcePath::new(format!("a.{ext}")).unwrap());
        assert!(ImageCodec::default().write(&s, &o).is_err());
        let c = ImageCodec {
            write_options: ImageWriteOptions {
                gray_threshold: Some(128),
                ..Default::default()
            },
            ..Default::default()
        };
        let w = c.write(&s, &o).unwrap();
        assert!(w.report.has_loss());
        let back = read_bundle(&Assets(w.output), o.primary_name.as_str()).unwrap();
        assert_eq!(
            back.fonts[0].glyphs().last().unwrap().1.bitmap.as_bytes(),
            &[0, 0xe0]
        );
    }
}
#[test]
fn malformed_json_does_not_fall_back_to_toml() {
    let a = Assets(
        OutputBundle::new(
            ResourcePath::new("atlas.pgm").unwrap(),
            vec![
                fontpixel::resources::OutputAsset {
                    path: ResourcePath::new("atlas.pgm").unwrap(),
                    bytes: PGM.to_vec(),
                },
                fontpixel::resources::OutputAsset {
                    path: ResourcePath::new("atlas.json").unwrap(),
                    bytes: b"not json".to_vec(),
                },
                fontpixel::resources::OutputAsset {
                    path: ResourcePath::new("atlas.toml").unwrap(),
                    bytes: b"pixel_size = 2\ncell_size = [2,2]\ncharacters = 'AB'\n".to_vec(),
                },
            ],
        )
        .unwrap(),
    );
    assert!(read_bundle(&a, "atlas.pgm").is_err());
}
#[test]
fn native_metadata_rejects_invalid_slot_order_and_zero_denominator() {
    let o = WriteOptions::new(ResourcePath::new("atlas.png").unwrap());
    let w = ImageCodec::default().write(&sample(false), &o).unwrap();
    for field in ["slots", "point_size", "id"] {
        let mut assets = w.output.clone().into_assets();
        let mapping = assets
            .iter_mut()
            .find(|a| a.path.as_str() == "atlas.json")
            .unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&mapping.bytes).unwrap();
        match field {
            "slots" => v["native"]["slots"] = serde_json::json!(usize::MAX),
            "point_size" => {
                v["native"]["metrics"]["point_size"] =
                    serde_json::json!({"numerator": 1, "denominator": 0})
            }
            _ => v["cells"][0]["native"]["id"] = serde_json::json!(2),
        }
        mapping.bytes = serde_json::to_vec(&v).unwrap();
        let a = Assets(OutputBundle::new(o.primary_name.clone(), assets).unwrap());
        assert!(read_bundle(&a, "atlas.png").is_err(), "{field}");
    }
}

#[test]
fn large_native_atlas_fits_aggregate_budget_and_preserves_raw_properties() {
    let mut font = Font::default();
    let pixels = vec![0x80; 54];
    for cp in 0..4000 {
        let mut glyph = Glyph::new(
            Bitmap::<fontpixel::bitmap::Mono>::from_raw(21, 18, &pixels, Default::default())
                .unwrap(),
        );
        glyph.codepoints.push(Codepoint::Unicode(cp));
        glyph.properties.push(Property::new(
            "source.rows",
            PropertyValue::Text("800000\n".repeat(18)),
        ));
        glyph.properties.push(Property::new(
            "source.image",
            PropertyValue::Bytes(pixels.clone()),
        ));
        font.add_glyph(glyph).unwrap();
    }
    let set = FontSet {
        fonts: vec![font],
        ..Default::default()
    };
    let limits = ResourceLimits {
        max_total_bytes: 32 * 1024 * 1024,
        ..Default::default()
    };
    let codec = ImageCodec {
        write_options: ImageWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        },
        ..Default::default()
    };
    let assets = Assets(
        codec
            .write(
                &set,
                &WriteOptions::new(ResourcePath::new("large.png").unwrap()),
            )
            .unwrap()
            .output,
    );
    let bytes = &assets
        .0
        .assets()
        .iter()
        .find(|a| a.path.as_str() == "large.png")
        .unwrap()
        .bytes;
    let input = Input::new(bytes)
        .with_name("large.png")
        .with_resources(&assets);
    let options = ReadOptions {
        limits,
        ..Default::default()
    };
    let (back, report) = codec
        .read(
            &mut ReadContext::new(&input, &options.limits).unwrap(),
            &options,
        )
        .unwrap();
    assert!(!report.has_loss());
    assert_eq!(back, set);
}
