//! Serde contract for the core (non-`formats`) public types used by bindings:
//! typical values round-trip through JSON, documented defaults fill omitted
//! fields, unknown fields are rejected and invariants are re-validated.
#![cfg(feature = "serde")]

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;

/// Serialize, parse back and require identical JSON (and so identical data).
fn round_trip<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let text = serde_json::to_string(value).expect("serialize");
    let back: T = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(value).unwrap()
    );
    back
}

/// Round trip and also compare with `PartialEq`.
fn round_trip_eq<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
    assert_eq!(&round_trip(value), value);
}

/// `{}` must produce the documented `Default`.
fn empty_is_default<T: Serialize + DeserializeOwned + Default>() {
    let parsed: T = serde_json::from_value(json!({})).expect("empty object");
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        serde_json::to_value(T::default()).unwrap()
    );
}

/// The value must be rejected, with the error mentioning `needle`.
fn rejects<T: DeserializeOwned + Debug>(value: Value, needle: &str) {
    let error = serde_json::from_value::<T>(value.clone())
        .expect_err(&format!("{value} should be rejected"))
        .to_string();
    assert!(error.contains(needle), "{error} does not mention {needle}");
}

/// Add a field to a serialized default object; deny_unknown_fields must reject it.
fn rejects_unknown_field<T: Serialize + DeserializeOwned + Debug>(value: &T) {
    let mut object = serde_json::to_value(value).unwrap();
    object
        .as_object_mut()
        .expect("struct serializes as an object")
        .insert("bogus".into(), json!(1));
    rejects::<T>(object, "unknown field");
}

mod bitmap {
    use super::*;
    use fontpixel::bitmap::*;

    #[test]
    fn bitmaps_round_trip_and_validate_storage() {
        let mono: Bitmap<Mono> = ".#.\n###".parse().unwrap();
        assert_eq!(
            serde_json::to_value(&mono).unwrap(),
            json!({"width": 3, "height": 2, "data": [0x40, 0xe0]})
        );
        round_trip_eq(&mono);
        round_trip_eq(&mono.to_gray8().unwrap());
        round_trip_eq(&Bitmap::<Mono>::default());
        rejects::<Bitmap<Mono>>(
            json!({"width": 3, "height": 2, "data": [0]}),
            "storage length",
        );
        rejects::<Bitmap<Gray8>>(
            json!({"width": 1, "height": 1, "data": [1], "limits": {}}),
            "unknown field",
        );
        rejects::<Bitmap<Mono>>(
            json!({"width": 2_000_000, "height": 0, "data": []}),
            "bitmap dimension",
        );
        // Padding bits are canonicalized exactly as by `from_raw`.
        let padded: Bitmap<Mono> =
            serde_json::from_value(json!({"width": 3, "height": 1, "data": [0xff]})).unwrap();
        assert_eq!(padded.as_bytes(), &[0xe0]);
    }

    #[test]
    fn options_and_enums() {
        empty_is_default::<BitmapLimits>();
        empty_is_default::<ByteLayout>();
        round_trip_eq(&BitmapLimits::default());
        rejects_unknown_field(&BitmapLimits::default());
        let layout = ByteLayout {
            bit_order: BitOrder::LsbFirst,
            byte_order: ByteOrder::Little,
            row_alignment: 4,
            scan_unit: 2,
            row_order: RowOrder::BottomUp,
        };
        round_trip_eq(&layout);
        rejects_unknown_field(&layout);
        assert_eq!(
            serde_json::to_value(layout).unwrap()["bit_order"],
            json!("lsb_first")
        );
        for mode in [CompositeMode::Over, CompositeMode::AlphaOver] {
            round_trip_eq(&mode);
        }
        assert_eq!(
            serde_json::to_value(CompositeMode::AlphaOver).unwrap(),
            json!("alpha_over")
        );
        round_trip_eq(&Direction::Up);
        round_trip_eq(&Alignment::End);
        round_trip_eq(&Bounds::new(3, 2, -1, -4));
        rejects::<Direction>(json!("sideways"), "unknown variant");
    }
}

mod model {
    use super::*;
    use fontpixel::bitmap::{Bitmap, Gray8};
    use fontpixel::model::*;

    fn glyph(code: u32) -> Glyph {
        let mut glyph = Glyph::new(".#.\n###".parse::<Bitmap>().unwrap());
        glyph.codepoints.push(Codepoint::Unicode(code));
        glyph.unicode_sequences.push(vec![code, 0x301]);
        glyph.metrics.advance = Some(Vector::new(4, 0));
        glyph.properties.push(Property::new("SOURCE", "x"));
        glyph
    }

    fn sample_font() -> Font {
        let mut font = Font::from_glyphs([glyph(65), glyph(66), glyph(67)]).unwrap();
        let mut gray = Glyph::new(Bitmap::<Gray8>::new(2, 1).unwrap());
        gray.codepoints.push(Codepoint::Legacy(0xa1));
        font.add_glyph(gray).unwrap();
        font.remove_glyph(font.glyph_id(Codepoint::Unicode(66)).unwrap());
        font.metrics.point_size = Some(Rational::new(21, 2).unwrap());
        font.metrics.default_glyph = font.glyph_id(Codepoint::Unicode(67));
        font.metrics.spacing = Some(Spacing::Proportional);
        font.names.family = Some("Family".into());
        font.charset.registry = Some("ISO10646".into());
        font.source_format = Some("bdf".into());
        font.headers
            .push(Property::new("COMMENT", PropertyValue::Flag));
        font.properties.set("PIXEL_SIZE", 12);
        font
    }

    #[test]
    fn font_round_trip_keeps_stable_ids_and_indexes() {
        let font = sample_font();
        let back = round_trip(&font);
        assert_eq!(back, font);
        assert_eq!(back.slot_count(), 4);
        assert_eq!(back.len(), 3);
        assert!(back.glyph_id(Codepoint::Unicode(66)).is_none());
        assert_eq!(
            back.glyph_id(Codepoint::Unicode(67)),
            font.glyph_id(Codepoint::Unicode(67))
        );
        assert!(back.glyph_for_sequence(&[65, 0x301]).is_some());
        assert_eq!(
            serde_json::to_value(&font).unwrap()["glyphs"][1],
            Value::Null
        );
        let set = FontSet {
            fonts: vec![font.clone(), Font::default()],
            properties: font.properties.clone(),
            source_format: Some("bdf".into()),
        };
        round_trip_eq(&set);
        rejects_unknown_field(&set);
        rejects_unknown_field(&font);
        round_trip_eq(&GlyphBitmap::Gray8(Bitmap::<Gray8>::new(2, 2).unwrap()));
        round_trip_eq(&ModelUsage {
            bytes: 10,
            fonts: 1,
            glyphs: 2,
            glyph_slots: 3,
        });
    }

    #[test]
    fn font_and_glyph_invariants_are_revalidated() {
        let mut value = serde_json::to_value(sample_font()).unwrap();
        value["metrics"]["default_glyph"] = json!(1);
        rejects::<Font>(value, "default glyph");

        let mut value = serde_json::to_value(sample_font()).unwrap();
        value["glyphs"][0]["metrics"]["bounds"]["width"] = json!(9);
        rejects::<Font>(value, "BBX dimensions");

        let mut value = serde_json::to_value(glyph(65)).unwrap();
        value["unicode_sequences"] = json!([[]]);
        let parsed: Glyph = serde_json::from_value(value).unwrap();
        let error = Font::from_glyphs([parsed]).unwrap_err().to_string();
        assert!(error.contains("empty Unicode"));

        rejects::<Rational>(json!({"numerator": 1, "denominator": 0}), "nonzero");
        rejects::<Rational>(json!({"numerator": 1, "denominator": 2, "x": 0}), "unknown");
        assert_eq!(
            serde_json::to_value(Rational::new(3, 4).unwrap()).unwrap(),
            json!({"numerator": 3, "denominator": 4})
        );
        rejects_unknown_field(&glyph(65));
    }
}

mod report {
    use super::*;
    use fontpixel::report::*;

    #[test]
    fn report_shape_is_stable_and_strict() {
        let mut report = Report::default();
        report.push(ReportEntry {
            level: Level::Warning,
            impact: Impact::Loss,
            code: "encoding.out_of_range".into(),
            location: Location {
                font_index: Some(0),
                glyph_index: None,
                codepoint: Some(65),
                asset: None,
            },
            message: "dropped".into(),
        });
        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            r#"{"entries":[{"level":"warning","impact":"loss","code":"encoding.out_of_range","location":{"font_index":0,"glyph_index":null,"codepoint":65,"asset":null},"message":"dropped"}]}"#
        );
        round_trip_eq(&report);
        rejects_unknown_field(&report);
        rejects_unknown_field(&report.entries[0]);
        rejects_unknown_field(&Location::default());
    }
}

mod merge {
    use super::*;
    use fontpixel::bitmap::Bitmap;
    use fontpixel::merge::*;
    use fontpixel::model::{Codepoint, Font, Glyph, Vector};

    #[test]
    fn merge_options_results_and_selection() {
        rejects::<MergeOptions>(json!({}), "missing field `family`");
        let options: MergeOptions = serde_json::from_value(json!({"family": "Mix"})).unwrap();
        assert_eq!(options.spacing, MergeSpacing::Auto);
        assert_eq!(options.limits, fontpixel::ResourceLimits::default());
        let options = MergeOptions {
            family: "Mix".into(),
            spacing: MergeSpacing::Prop,
            ..MergeOptions::default()
        };
        round_trip(&options);
        rejects_unknown_field(&options);
        empty_is_default::<PartLicense>();
        round_trip_eq(&PartLicense {
            copyright: Some("c".into()),
            modification: ModificationPermission::Allowed,
            reserved_names: vec!["Reserved".into()],
            ..PartLicense::default()
        });

        let selection = Selection::parse("western,U+4E00-4E0F,-U+0041").unwrap();
        assert_eq!(
            serde_json::to_value(&selection).unwrap(),
            json!("western,U+4E00-4E0F,-U+0041")
        );
        round_trip_eq(&selection);
        rejects::<Selection>(json!("no-such-category"), "");
        round_trip_eq(&CodepointRange { start: 1, end: 2 });

        let mut glyph = Glyph::new("#.#\n.##".parse::<Bitmap>().unwrap());
        glyph.codepoints.push(Codepoint::Unicode(65));
        glyph.metrics.advance = Some(Vector::new(4, 0));
        let mut font = Font::from_glyphs([glyph]).unwrap();
        font.names.family = Some("Part".into());
        font.names.license = Some("OFL-1.1".into());
        let result = merge(
            &[MergePart::new(&font, Selection::parse("western").unwrap())],
            &options,
        )
        .unwrap();
        let back = round_trip(&result);
        assert_eq!(back.font, result.font);
        assert_eq!(back.report, result.report);
        rejects_unknown_field(&result.report);
        rejects_unknown_field(&result.report.parts[0]);
        rejects_unknown_field(&result.report.metrics);
    }
}

mod resources {
    use super::*;
    use fontpixel::resources::*;

    #[test]
    fn paths_bundles_and_limits() {
        let path = ResourcePath::new("dir/font.bdf").unwrap();
        assert_eq!(serde_json::to_value(&path).unwrap(), json!("dir/font.bdf"));
        round_trip_eq(&path);
        rejects::<ResourcePath>(json!("../escape"), "unsafe relative asset path");
        rejects::<ResourcePath>(json!(""), "unsafe");

        let bundle = OutputBundle::new(
            ResourcePath::new("a.fnt").unwrap(),
            vec![
                OutputAsset {
                    path: ResourcePath::new("a.png").unwrap(),
                    bytes: vec![1, 2],
                },
                OutputAsset {
                    path: ResourcePath::new("a.fnt").unwrap(),
                    bytes: vec![3],
                },
            ],
        )
        .unwrap();
        round_trip_eq(&bundle);
        rejects_unknown_field(&bundle);
        let mut missing = serde_json::to_value(&bundle).unwrap();
        missing["primary"] = json!("other.fnt");
        rejects::<OutputBundle>(missing, "missing its primary asset");
        let mut duplicate = serde_json::to_value(&bundle).unwrap();
        duplicate["assets"][1]["path"] = json!("a.fnt");
        rejects::<OutputBundle>(duplicate, "duplicate output asset");
        let directory = OutputBundle::directory(
            ResourcePath::new("out").unwrap(),
            vec![OutputAsset {
                path: ResourcePath::new("out/x").unwrap(),
                bytes: vec![],
            }],
        )
        .unwrap();
        round_trip_eq(&directory);
        let written = Written {
            output: bundle,
            report: fontpixel::Report::default(),
        };
        round_trip_eq(&written);
        rejects_unknown_field(&written);

        empty_is_default::<ResourceLimits>();
        empty_is_default::<TransportLimits>();
        round_trip_eq(&TransportLimits::default());
        rejects_unknown_field(&ResourceLimits::default());
        let partial: ResourceLimits = serde_json::from_value(json!({"max_glyphs": 7})).unwrap();
        assert_eq!(partial.max_glyphs, 7);
        assert_eq!(
            partial.max_input_bytes,
            ResourceLimits::default().max_input_bytes
        );
    }
}

mod render {
    use super::*;
    use fontpixel::bitmap::{Bitmap, Direction};
    use fontpixel::model::Glyph;
    use fontpixel::render::*;

    #[test]
    fn render_options_and_results() {
        empty_is_default::<RenderOptions<'static>>();
        empty_is_default::<RenderLimits>();
        empty_is_default::<WritingDirection>();
        empty_is_default::<ImageOptions>();
        let options = RenderOptions {
            mode: RenderMode::Ink,
            direction: WritingDirection {
                glyphs: Direction::Left,
                lines: Direction::Up,
            },
            missing: MissingGlyph::Error,
            gray_threshold: Some(128),
            ..RenderOptions::default()
        };
        let back: RenderOptions<'static> = round_trip(&options);
        assert!(matches!(back.missing, MissingGlyph::Error));
        rejects_unknown_field(&options);

        // A borrowed replacement glyph serializes, but must be passed separately.
        let replacement = Glyph::new("#".parse::<Bitmap>().unwrap());
        let with_glyph = RenderOptions {
            missing: MissingGlyph::Glyph(&replacement),
            ..RenderOptions::default()
        };
        let value = serde_json::to_value(&with_glyph).unwrap();
        assert!(value["missing"]["glyph"].is_object());
        rejects::<RenderOptions<'static>>(value, "pass the replacement glyph separately");
        rejects::<MissingGlyph<'static>>(json!("skip"), "unknown variant");

        round_trip_eq(&GlyphOrder::ReverseFile);
        assert_eq!(
            serde_json::to_value(GlyphOrder::ReverseFile).unwrap(),
            json!("reverse_file")
        );
        let image = ImageOptions {
            scale: 3,
            foreground: Rgba::new(1, 2, 3, 4),
            ..ImageOptions::default()
        };
        round_trip(&image);
        rejects_unknown_field(&image);
        round_trip_eq(&Rgba::new(9, 8, 7, 6));
        let rendered = Rendered {
            bitmap: "#.\n.#".parse().unwrap(),
            report: fontpixel::Report::default(),
        };
        let back = round_trip(&rendered);
        assert_eq!(back.bitmap, rendered.bitmap);
        rejects_unknown_field(&rendered);
    }
}

mod projection_metadata_charset_outline {
    use super::*;
    use fontpixel::charset::merge_categories::{CJK_RANGES, EastAsianWidth};
    use fontpixel::charset::{self, CharsetError, Kind};
    use fontpixel::metadata::NameOverrides;
    use fontpixel::model::{GlyphId, PropertyValue, Rational, Vector};
    use fontpixel::outline;
    use fontpixel::projection::*;

    #[test]
    fn projection_outcomes() {
        round_trip_eq(&ScalableAdvance::FromDevice {
            units_per_em: 1000,
            reference_ppem: 16,
        });
        rejects::<ScalableAdvance>(
            json!({"from_device": {"units_per_em": 1, "reference_ppem": 2, "x": 3}}),
            "unknown field",
        );
        let size = SizeProjection {
            point_size: Rational::new(21, 2).unwrap(),
            resolution: Vector::new(72, 72),
        };
        round_trip_eq(&size);
        rejects_unknown_field(&size);
        let glyph: GlyphId = serde_json::from_value(json!(3)).unwrap();
        let outcome = ProjectionOutcome {
            report: fontpixel::Report::default(),
            scalable_advance_changes: vec![ScalableAdvanceChange {
                font_index: 0,
                glyph,
                glyph_value: None,
                effective_before: Some(Vector::new(500, 0)),
                device_advance: 8,
                after: Vector::new(667, 0),
            }],
            metadata_changes: vec![
                MetadataChange {
                    font_index: 0,
                    field: MetadataField::Property {
                        name: "FOUNDRY".into(),
                        record: 2,
                    },
                    before: None,
                    after: Some(MetadataValue::Property(PropertyValue::Text("x".into()))),
                },
                MetadataChange {
                    font_index: 1,
                    field: MetadataField::Metric(SyncedMetric::LineHeight),
                    before: Some(MetadataValue::Rational(Rational::integer(3))),
                    after: Some(MetadataValue::Vector(Vector::new(1, 2))),
                },
            ],
        };
        round_trip_eq(&outcome);
        rejects_unknown_field(&outcome);
        rejects_unknown_field(&outcome.metadata_changes[0]);
    }

    #[test]
    fn names_charsets_and_outline_options() {
        empty_is_default::<NameOverrides>();
        let names = NameOverrides {
            family: Some("F".into()),
            license_url: Some("https://example.invalid".into()),
            ..NameOverrides::default()
        };
        round_trip_eq(&names);
        rejects_unknown_field(&names);

        round_trip_eq(&Kind::Gb18030);
        assert_eq!(
            serde_json::to_value(Kind::DoubleByte).unwrap(),
            json!("double_byte")
        );
        round_trip_eq(&CharsetError::FeatureDisabled("cjk".into()));
        round_trip_eq(&EastAsianWidth::Ambiguous);
        // Static registry data is serialize-only.
        let info = serde_json::to_value(charset::charset_table()[0]).unwrap();
        assert!(info["key"].is_string() && info.get("table").is_none());
        assert!(serde_json::to_value(CJK_RANGES[0]).unwrap()["name"].is_string());
        assert_eq!(
            serde_json::to_value(charset::windows_charset(2)).unwrap(),
            json!("symbol")
        );
        round_trip_eq(&fontpixel::Operation::Write);

        empty_is_default::<outline::Options>();
        empty_is_default::<outline::Limits>();
        let options = outline::Options {
            shape: outline::Shape::SquareSeparate,
            dot_size: 0.75,
            origin: (-1, 2),
            ..outline::Options::default()
        };
        round_trip(&options);
        rejects_unknown_field(&options);
        round_trip_eq(&outline::Segment::Quad((0.0, 1.0), (2.5, 3.0), (4.0, -5.0)));
        let edge = outline::path::Edge::Cubic((0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0));
        assert!(serde_json::to_value(edge).unwrap()["cubic"].is_array());
        let _: outline::path::Edge =
            serde_json::from_value(json!({"line": [[0.0, 0.0], [1.0, 1.0]]})).unwrap();
    }
}

#[cfg(feature = "sfnt")]
mod sfnt {
    use super::*;
    use fontpixel::bitmap::Bitmap;
    use fontpixel::model::GlyphBitmap;
    use fontpixel::sfnt::{self, bitmap, cmap, gasp, glyf, gsub, metrics, name, os2, post, subset};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn limits_and_options() {
        empty_is_default::<sfnt::Limits>();
        empty_is_default::<bitmap::Limits>();
        empty_is_default::<glyf::Limits>();
        empty_is_default::<gsub::LayoutLimits>();
        empty_is_default::<subset::Options>();
        rejects_unknown_field(&sfnt::Limits::default());
        rejects_unknown_field(&glyf::Limits::default());
        let options = subset::Options {
            codepoints: BTreeSet::from([65, 0x4e00]),
            drop_tables: BTreeSet::from([*b"DSIG", *b"hdmx"]),
            ..subset::Options::default()
        };
        let value = serde_json::to_value(&options).unwrap();
        assert_eq!(value["drop_tables"], json!(["DSIG", "hdmx"]));
        let back = round_trip(&options);
        assert_eq!(back.drop_tables, options.drop_tables);
        rejects_unknown_field(&options);
        rejects::<subset::Options>(json!({"drop_tables": ["toolong"]}), "four characters");
        rejects::<subset::Options>(json!({"drop_tables": ["ab"]}), "four characters");
        rejects::<subset::Options>(json!({"drop_tables": ["ab\u{4e00}d"]}), "U+00FF");
        let result = subset::Subset {
            bytes: vec![0, 1],
            glyph_mapping: BTreeMap::from([(0, 0), (5, 1)]),
            report: fontpixel::Report::default(),
        };
        round_trip(&result);
        rejects_unknown_field(&result);
    }

    #[test]
    fn tables_round_trip() {
        let mut languages = BTreeMap::new();
        languages.insert(
            *b"JAN ",
            gsub::LanguageSystem {
                required: None,
                features: vec![0],
            },
        );
        let table = gsub::Gsub {
            scripts: BTreeMap::from([(
                *b"hani",
                gsub::Script {
                    default: Some(gsub::LanguageSystem::default()),
                    languages,
                },
            )]),
            features: vec![gsub::Feature {
                tag: *b"locl",
                lookups: vec![0],
            }],
            lookups: vec![BTreeMap::from([(1, 2)])],
        };
        let value = serde_json::to_value(&table).unwrap();
        assert!(value["scripts"]["hani"]["languages"]["JAN "].is_object());
        assert_eq!(value["features"][0]["tag"], json!("locl"));
        round_trip_eq(&table);
        rejects_unknown_field(&table);
        // Non-ASCII bytes stay lossless through the Latin-1 string form.
        let odd = gsub::Feature {
            tag: [0, 0x7f, 0xa0, 0xff],
            lookups: vec![],
        };
        round_trip_eq(&odd);

        let os2 = os2::Os2Table::default();
        round_trip_eq(&os2);
        rejects_unknown_field(&os2);
        round_trip_eq(&post::PostTable {
            header: post::Header::default(),
            names: Some(vec![".notdef".into()]),
        });
        round_trip_eq(&name::NameTable {
            records: vec![name::NameRecord {
                platform_id: 3,
                encoding_id: 1,
                language_id: 0x409,
                name_id: 1,
                value: vec![0, 65],
            }],
            language_tags: vec!["en".into()],
        });
        round_trip_eq(&gasp::GaspTable {
            version: 1,
            ranges: vec![gasp::Range {
                max_ppem: 0xffff,
                behavior: 0xa,
            }],
        });
        round_trip_eq(&metrics::LongMetric {
            advance: 1,
            bearing: -2,
        });
        round_trip_eq(&metrics::Axis::Vertical);
        round_trip_eq(&cmap::FullRecords::NonBmpOnly);
        round_trip_eq(&sfnt::Flavor::AppleTrueType);
        round_trip(&cmap::EncodedCmap {
            data: vec![1],
            has_bmp_subtable: true,
        });

        let glyphs = glyf::GlyfTable {
            glyphs: vec![
                glyf::Glyph::Empty,
                glyf::Glyph::Simple(glyf::SimpleGlyph {
                    contours: vec![vec![(0.0, 0.0, true), (1.0, 0.5, false)]],
                    instructions: vec![1],
                    overlap: false,
                }),
                glyf::Glyph::Composite(glyf::CompositeGlyph {
                    components: vec![glyf::Component {
                        glyph_id: 1,
                        anchor: glyf::Anchor::Point {
                            parent: 0,
                            child: 1,
                        },
                        transform: [0x4000, 0, 0, 0x4000],
                        offset_mode: glyf::OffsetMode::Scaled,
                        round_to_grid: true,
                        use_my_metrics: false,
                        overlap: false,
                        non_overlapping: true,
                    }],
                    instructions: vec![],
                }),
            ],
        };
        round_trip_eq(&glyphs);
        round_trip_eq(&glyf::LocaFormat::Long);
        round_trip_eq(&glyf::Statistics::default());

        let strike = bitmap::Strike {
            ppem_x: 12,
            ppem_y: 12,
            bit_depth: 1,
            flags: 1,
            horizontal: bitmap::LineMetrics([1; 12]),
            vertical: bitmap::LineMetrics::default(),
            glyphs: BTreeMap::from([(
                3,
                bitmap::BitmapGlyph {
                    metrics: bitmap::Metrics {
                        width: 2,
                        height: 1,
                        horizontal: Some(bitmap::Bearing {
                            x: -1,
                            y: 1,
                            advance: 3,
                        }),
                        vertical: None,
                    },
                    bitmap: GlyphBitmap::Mono("##".parse::<Bitmap>().unwrap()),
                },
            )]),
        };
        round_trip_eq(&strike);
        rejects_unknown_field(&strike);
        round_trip_eq(&bitmap::Kind::Apple);
    }

    #[test]
    fn variants_round_trip() {
        use fontpixel::model::{Codepoint, Font, Glyph};
        use fontpixel::variants::*;
        empty_is_default::<Options>();
        empty_is_default::<MetadataOptions>();
        let options = Options {
            vendor: *b"FPXL",
            dot_reuse: DotReuse::Rows,
            flavor: OutlineFlavor::Cff,
            notdef: NotdefPolicy::Empty {
                advance_width: 500,
                left_side_bearing: 0,
            },
            metadata: MetadataOptions {
                gasp: GaspPolicy::Omit,
                glyph_order: GlyphOrder::Codepoint,
                dot_glyph: DotGlyph::AfterNotdef,
                timestamps: Some([1, 2]),
                ..MetadataOptions::default()
            },
            ..Options::default()
        };
        let value = serde_json::to_value(&options).unwrap();
        assert_eq!(value["vendor"], json!("FPXL"));
        assert_eq!(value["notdef"]["empty"]["advance_width"], json!(500));
        round_trip(&options);
        rejects_unknown_field(&options);

        let mut glyph = Glyph::new("#".parse::<fontpixel::Bitmap>().unwrap());
        glyph.codepoints.push(Codepoint::Unicode(0x4e00));
        let font = Font::from_glyphs([glyph]).unwrap();
        let variants = Variants {
            base: font.clone(),
            languages: vec![LanguageVariant {
                language: *b"JAN ",
                scripts: vec![*b"hani", *b"kana"],
                font: font.clone(),
            }],
            scripts: vec![ScriptVariant {
                scripts: vec![*b"hang"],
                font,
            }],
        };
        let value = serde_json::to_value(&variants).unwrap();
        assert_eq!(value["languages"][0]["scripts"], json!(["hani", "kana"]));
        round_trip_eq(&variants);
        rejects_unknown_field(&variants);
        round_trip(&Encoded {
            bytes: vec![1],
            report: fontpixel::Report::default(),
        });
    }
}

#[cfg(feature = "cff")]
mod cff {
    use super::*;
    use fontpixel::sfnt::cff::*;

    #[test]
    fn cff_values() {
        empty_is_default::<Limits>();
        empty_is_default::<DotOptions>();
        rejects_unknown_field(&Limits::default());
        let font = Font {
            postscript_name: "Name".into(),
            full_name: "Full".into(),
            units_per_em: 1000,
            glyphs: vec![
                Glyph {
                    name: ".notdef".into(),
                    advance: 500.0,
                    outline: Outline::Path(vec![
                        Command::MoveTo((0.0, 0.0)),
                        Command::CurveTo((1.0, 1.0), (2.0, 2.0), (3.0, 0.5)),
                        Command::Close,
                    ]),
                },
                Glyph {
                    name: "a".into(),
                    advance: 250.5,
                    outline: Outline::Dots(vec![(1.0, 2.0)]),
                },
            ],
            dots: DotOptions::default(),
        };
        round_trip_eq(&font);
        rejects_unknown_field(&font);
        round_trip_eq(&Encoded {
            data: vec![1],
            bounds: [0, -1, 2, 3],
            maxp: vec![],
        });
    }
}

#[cfg(feature = "ttc")]
#[test]
fn collection_limits() {
    use fontpixel::sfnt::collection::Limits;
    empty_is_default::<Limits>();
    rejects_unknown_field(&Limits::default());
}

#[cfg(any(feature = "woff", feature = "woff2"))]
mod woff {
    use super::*;
    use fontpixel::woff::*;

    #[test]
    fn woff_values() {
        empty_is_default::<Limits>();
        rejects_unknown_field(&Limits::default());
        let auxiliary = Auxiliary {
            major_version: 1,
            minor_version: 2,
            metadata: Some(vec![1, 2]),
            private_data: None,
        };
        round_trip_eq(&auxiliary);
        rejects_unknown_field(&auxiliary);
        round_trip_eq(&Decoded {
            sfnt: vec![0, 1, 0, 0],
            auxiliary,
            report: fontpixel::Report::default(),
        });
        #[cfg(feature = "woff")]
        {
            empty_is_default::<woff1::Options>();
            rejects_unknown_field(&woff1::Options::default());
        }
        #[cfg(feature = "woff2")]
        {
            empty_is_default::<woff2::Options>();
            let partial: woff2::Options = serde_json::from_value(json!({"quality": 5})).unwrap();
            assert_eq!(partial.quality, 5);
            rejects_unknown_field(&woff2::Options::default());
        }
    }
}

#[cfg(feature = "compat-bdfparser")]
#[test]
fn compat_options() {
    use fontpixel::compat::bdfparser::{CreateOptions, ttf::TtfOptions};
    empty_is_default::<CreateOptions>();
    empty_is_default::<TtfOptions>();
    round_trip(&TtfOptions {
        shape: "dot".into(),
        dotsize: 0.5,
        ..TtfOptions::default()
    });
    rejects_unknown_field(&CreateOptions::default());
    rejects_unknown_field(&TtfOptions::default());
}
