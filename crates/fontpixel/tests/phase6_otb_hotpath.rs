//! Canonical raster copying must match the former scalar sbit serializer.
#![cfg(feature = "sfnt")]

use fontpixel::bitmap::{Bitmap, Gray8};
use fontpixel::model::GlyphBitmap;
use fontpixel::sfnt::bitmap::{
    self as sbit, Bearing, BitmapGlyph, Kind, Limits, LineMetrics, Metrics, Strike, Tables,
};
use std::collections::BTreeMap;

// Independent reference: retain the former per-sample packing, including row
// alignment and quantization. This must not call the optimized encoder.
fn scalar_glyph(g: &BitmapGlyph, depth: u8, flags: u8) -> (u16, Vec<u8>) {
    let m = g.metrics;
    let mut encoded = vec![m.height, m.width];
    let format = match (m.horizontal, m.vertical) {
        (Some(h), Some(v)) => {
            encoded.extend([
                h.x as u8, h.y as u8, h.advance, v.x as u8, v.y as u8, v.advance,
            ]);
            6
        }
        (Some(h), None) if flags == 1 => {
            encoded.extend([h.x as u8, h.y as u8, h.advance]);
            1
        }
        (None, Some(v)) if flags == 2 => {
            encoded.extend([v.y as u8, v.x as u8, v.advance]);
            1
        }
        _ => panic!("invalid reference fixture metrics"),
    };
    let depth = depth as usize;
    let stride = (g.bitmap.width() as usize * depth).div_ceil(8);
    let header = encoded.len();
    encoded.resize(header + stride * g.bitmap.height() as usize, 0);
    let step = 255 / ((1u16 << depth) - 1) as u8;
    for y in 0..g.bitmap.height() {
        for x in 0..g.bitmap.width() {
            let v = match &g.bitmap {
                GlyphBitmap::Mono(b) => b.get_pixel(x, y).unwrap() * 255,
                GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap(),
            };
            assert!(v.is_multiple_of(step));
            let bit = x as usize * depth;
            encoded[header + y as usize * stride + bit / 8] |= (v / step) << (8 - depth - bit % 8);
        }
    }
    (format, encoded)
}
fn scalar_tables(strikes: &[Strike], kind: Kind) -> Tables {
    assert!(matches!(kind, Kind::OpenType | Kind::Apple));
    let mut data = 0x20000u32.to_be_bytes().to_vec();
    let mut location = vec![0; 8 + 48 * strikes.len()];
    location[..4].copy_from_slice(&0x20000u32.to_be_bytes());
    location[4..8].copy_from_slice(&(strikes.len() as u32).to_be_bytes());
    let mut ordered: Vec<_> = strikes.iter().collect();
    if kind == Kind::Apple {
        ordered.sort_by_key(|s| (s.ppem_y, s.ppem_x));
    }
    for (i, s) in ordered.into_iter().enumerate() {
        let mut groups: Vec<(u16, u16, u16, usize, Vec<u32>)> = Vec::new();
        for (&gid, g) in &s.glyphs {
            let (format, encoded) = scalar_glyph(g, s.bit_depth, s.flags);
            if !groups
                .last()
                .is_some_and(|g| g.1.checked_add(1) == Some(gid) && g.2 == format)
            {
                groups.push((gid, gid, format, data.len(), vec![0]));
            }
            data.extend(encoded);
            let group = groups.last_mut().unwrap();
            group.1 = gid;
            group.4.push((data.len() - group.3) as u32);
        }
        let start = location.len();
        location.resize(start + groups.len() * 8, 0);
        for (j, (lo, hi, format, base, offsets)) in groups.iter().enumerate() {
            let pos = start + j * 8;
            location[pos..pos + 2].copy_from_slice(&lo.to_be_bytes());
            location[pos + 2..pos + 4].copy_from_slice(&hi.to_be_bytes());
            let offset = (location.len() - start) as u32;
            location[pos + 4..pos + 8].copy_from_slice(&offset.to_be_bytes());
            location.extend(1u16.to_be_bytes());
            location.extend(format.to_be_bytes());
            location.extend((*base as u32).to_be_bytes());
            for offset in offsets {
                location.extend(offset.to_be_bytes());
            }
        }
        let size = (location.len() - start) as u32;
        let pos = 8 + i * 48;
        location[pos..pos + 4].copy_from_slice(&(start as u32).to_be_bytes());
        location[pos + 4..pos + 8].copy_from_slice(&size.to_be_bytes());
        location[pos + 8..pos + 12].copy_from_slice(&(groups.len() as u32).to_be_bytes());
        location[pos + 16..pos + 28].copy_from_slice(&s.horizontal.0);
        location[pos + 28..pos + 40].copy_from_slice(&s.vertical.0);
        location[pos + 40..pos + 42]
            .copy_from_slice(&s.glyphs.first_key_value().unwrap().0.to_be_bytes());
        location[pos + 42..pos + 44]
            .copy_from_slice(&s.glyphs.last_key_value().unwrap().0.to_be_bytes());
        location[pos + 44..pos + 48].copy_from_slice(&[s.ppem_x, s.ppem_y, s.bit_depth, s.flags]);
    }
    Tables { data, location }
}
fn glyph(bitmap: GlyphBitmap, flags: u8) -> BitmapGlyph {
    BitmapGlyph {
        metrics: Metrics {
            width: bitmap.width() as u8,
            height: bitmap.height() as u8,
            horizontal: (flags != 2).then_some(Bearing {
                x: -2,
                y: 3,
                advance: 19,
            }),
            vertical: (flags != 1).then_some(Bearing {
                x: 4,
                y: -5,
                advance: 20,
            }),
        },
        bitmap,
    }
}
fn strike(g: BitmapGlyph, depth: u8, flags: u8) -> Strike {
    Strike {
        ppem_x: 14,
        ppem_y: 14,
        bit_depth: depth,
        flags,
        horizontal: LineMetrics([11, 253, 17, 1, 0, 0, 254, 0, 3, 253, 0, 0]),
        vertical: LineMetrics::default(),
        glyphs: BTreeMap::from([(1, g)]),
    }
}

#[test]
fn canonical_rows_match_scalar_bytes_for_all_edge_widths_and_metrics() {
    for width in [0u32, 1, 7, 8, 9, 13, 14, 16, 17] {
        for height in [0u32, 1, 3, 17, 255] {
            for nonzero in [false, true] {
                for padding in [false, true] {
                    let mut raw = (0..width.div_ceil(8) as usize * height as usize)
                        .map(|i| {
                            if nonzero {
                                (i as u8).wrapping_mul(73).wrapping_add(165)
                            } else {
                                0
                            }
                        })
                        .collect::<Vec<_>>();
                    if width != 0 && !width.is_multiple_of(8) {
                        let mask = 0xff << (8 - width % 8);
                        for row in raw.chunks_exact_mut(width.div_ceil(8) as usize) {
                            let last = row.last_mut().unwrap();
                            *last = (*last & mask) | if padding { !mask } else { 0 };
                        }
                    }
                    let mono = Bitmap::from_raw(width, height, &raw, Default::default()).unwrap();
                    let gray_raw = (0..width as usize * height as usize)
                        .map(|i| {
                            if nonzero {
                                (i as u8).wrapping_mul(73).wrapping_add(165)
                            } else {
                                0
                            }
                        })
                        .collect::<Vec<_>>();
                    let gray =
                        Bitmap::<Gray8>::from_raw(width, height, &gray_raw, Default::default())
                            .unwrap();
                    for (bitmap, depth) in
                        [(GlyphBitmap::Mono(mono), 1), (GlyphBitmap::Gray8(gray), 8)]
                    {
                        for flags in [1, 2, 3] {
                            let s = strike(glyph(bitmap.clone(), flags), depth, flags);
                            for kind in [Kind::OpenType, Kind::Apple] {
                                let expected = scalar_tables(std::slice::from_ref(&s), kind);
                                let actual = sbit::encode(
                                    std::slice::from_ref(&s),
                                    kind,
                                    2,
                                    &Limits::default(),
                                )
                                .unwrap();
                                assert_eq!(
                                    actual, expected,
                                    "{width}x{height}, depth={depth}, flags={flags}, nonzero={nonzero}, padding={padding}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn mixed_storage_depths_sparse_groups_and_apple_order_match_scalar() {
    let mono: Bitmap = "#.#.#.#.#\n.#.#.#.#.".parse().unwrap();
    for depth in [1u8, 2, 4, 8] {
        let step = 255 / ((1u16 << depth) - 1) as u8;
        let bytes = (0..18)
            .map(|i| (i % (1usize << depth)) as u8 * step)
            .collect::<Vec<_>>();
        let gray = Bitmap::<Gray8>::from_raw(9, 2, &bytes, Default::default()).unwrap();
        let mut first = strike(glyph(GlyphBitmap::Mono(mono.clone()), 1), depth, 1);
        first.glyphs.insert(2, glyph(GlyphBitmap::Gray8(gray), 3));
        first
            .glyphs
            .insert(4, glyph(GlyphBitmap::Mono(Bitmap::new(0, 3).unwrap()), 1));
        let mut second = first.clone();
        second.ppem_x = 12;
        second.ppem_y = 12;
        let strikes = [first, second];
        for kind in [Kind::OpenType, Kind::Apple] {
            assert_eq!(
                sbit::encode(&strikes, kind, 5, &Limits::default()).unwrap(),
                scalar_tables(&strikes, kind)
            );
        }
    }
    for depth in [1, 2, 4] {
        let gray = Bitmap::<Gray8>::from_raw(1, 1, &[1], Default::default()).unwrap();
        let s = strike(glyph(GlyphBitmap::Gray8(gray), 1), depth, 1);
        assert_eq!(
            sbit::encode(&[s], Kind::OpenType, 2, &Limits::default())
                .unwrap_err()
                .to_string(),
            "sfnt: bitmap gray value cannot be represented at requested depth"
        );
    }
}

#[test]
fn direct_copy_retains_exact_work_model_and_table_boundaries() {
    for (bitmap, depth) in [
        (GlyphBitmap::Mono(Bitmap::new(9, 3).unwrap()), 1),
        (GlyphBitmap::Gray8(Bitmap::<Gray8>::new(9, 3).unwrap()), 8),
    ] {
        let s = strike(glyph(bitmap, 1), depth, 1);
        let mut limits = Limits {
            max_strikes: 1,
            max_pixel_work: 27,
            max_visits: 0,
            ..Default::default()
        };
        limits.sfnt.resources.max_glyphs = 2;
        limits.sfnt.resources.max_pixels_per_glyph = 27;
        limits.sfnt.resources.max_total_bytes = 512 + 27;
        limits.sfnt.resources.max_table_bytes = 80;
        // Ordinary raw encoding has never consumed graph visits. Retain that
        // behavior while still charging every glyph and all raster pixels.
        let expected = scalar_tables(std::slice::from_ref(&s), Kind::OpenType);
        assert_eq!(
            sbit::encode(std::slice::from_ref(&s), Kind::OpenType, 2, &limits).unwrap(),
            expected
        );
        for (field, resource, limit) in [
            (0, "embedded bitmap strikes", 0),
            (1, "embedded bitmap pixel work", 26),
            (2, "embedded glyph pixels", 26),
            (3, "embedded bitmap model bytes", 538),
            (4, "sfnt table bytes", 79),
            (5, "sfnt glyphs", 1),
        ] {
            let mut tight = limits.clone();
            match field {
                0 => tight.max_strikes = limit,
                1 => tight.max_pixel_work = limit,
                2 => tight.sfnt.resources.max_pixels_per_glyph = limit,
                3 => tight.sfnt.resources.max_total_bytes = limit,
                4 => tight.sfnt.resources.max_table_bytes = limit,
                _ => tight.sfnt.resources.max_glyphs = limit,
            }
            let err =
                sbit::encode(std::slice::from_ref(&s), Kind::OpenType, 2, &tight).unwrap_err();
            assert!(
                matches!(err, fontpixel::Error::ResourceLimit { resource: actual, limit: actual_limit } if actual == resource && actual_limit == limit),
                "{err:?}"
            );
        }
        let mut mismatched = s.clone();
        mismatched.glyphs.get_mut(&1).unwrap().metrics.width = 8;
        assert!(matches!(
            sbit::encode(&[mismatched], Kind::OpenType, 2, &limits),
            Err(fontpixel::Error::InvalidInput(_))
        ));
    }
}

#[cfg(feature = "otb")]
#[test]
fn native_otb_matches_scalar_tables_golden_payload_and_all_loss_entries() {
    use fontpixel::model::{Codepoint, Property, Vector};
    use fontpixel::report::{Impact, Level, Location, ReportEntry};
    use fontpixel::sfnt::{Sfnt, SfntBuilder};
    use fontpixel::{Font, FontSet, Glyph, ResourcePath, WriteOptions};
    for mixed_gray in [false, true] {
        let mut font = Font::default();
        font.metrics.pixel_size = Some(14);
        font.metrics.ascent = Some(11);
        font.metrics.descent = Some(3);
        font.properties.push(Property::new("source", 7i64));
        let mut a = Glyph::new("#.#\n.#.".parse::<Bitmap>().unwrap());
        a.name = Some("A".into());
        a.codepoints = vec![Codepoint::Unicode(65), Codepoint::Unicode(0x10000)];
        a.metrics.advance = Some(Vector::new(4, 0));
        a.metrics.bounds.x = -1;
        a.metrics.bounds.y = -1;
        a.metrics.scalable_advance = Some(Vector::new(500, 0));
        let mut b = Glyph::new(if mixed_gray {
            GlyphBitmap::Gray8(
                Bitmap::<Gray8>::from_raw(
                    9,
                    1,
                    &[0, 17, 34, 51, 68, 85, 102, 119, 136],
                    Default::default(),
                )
                .unwrap(),
            )
        } else {
            GlyphBitmap::Mono("#.......#".parse::<Bitmap>().unwrap())
        });
        b.name = Some("bad/name".into());
        b.codepoints = vec![Codepoint::Unicode(66)];
        b.metrics.advance = Some(Vector::new(10, 0));
        b.properties.push(Property::new("meta", 3i64));
        let zero = BitmapGlyph {
            metrics: Metrics {
                horizontal: Some(Bearing::default()),
                ..Default::default()
            },
            bitmap: GlyphBitmap::Mono(Bitmap::default()),
        };
        let source_strike = Strike {
            ppem_x: 14,
            ppem_y: 14,
            bit_depth: if mixed_gray { 8 } else { 1 },
            flags: 1,
            horizontal: LineMetrics([11, 253, 9, 1, 0, 0, 255, 0, 1, 255, 0, 0]),
            vertical: LineMetrics::default(),
            glyphs: BTreeMap::from([
                (0, zero),
                (
                    1,
                    BitmapGlyph {
                        metrics: Metrics {
                            width: 3,
                            height: 2,
                            horizontal: Some(Bearing {
                                x: -1,
                                y: 1,
                                advance: 4,
                            }),
                            vertical: None,
                        },
                        bitmap: a.bitmap.clone(),
                    },
                ),
                (
                    2,
                    BitmapGlyph {
                        metrics: Metrics {
                            width: 9,
                            height: 1,
                            horizontal: Some(Bearing {
                                x: 0,
                                y: 1,
                                advance: 10,
                            }),
                            vertical: None,
                        },
                        bitmap: b.bitmap.clone(),
                    },
                ),
            ]),
        };
        font.add_glyph(a).unwrap();
        font.add_glyph(b).unwrap();
        let set = FontSet::from(font);
        let mut options = WriteOptions::new(ResourcePath::new("out.otb").unwrap());
        let written = fontpixel::write(&set, "otb", &options).unwrap();
        let mut expected_report = vec![
            (
                "source_property",
                None,
                "Source property \"source\" is not represented",
            ),
            (
                "glyph_metric",
                Some(0),
                "Independent scalable_advance is not preserved",
            ),
            (
                "glyph_property",
                Some(1),
                "Glyph property \"meta\" is not represented",
            ),
            (
                "glyph_name",
                Some(1),
                "Glyph name cannot be encoded in post format 2",
            ),
        ];
        if mixed_gray {
            expected_report.push((
                "depth_normalized",
                Some(1),
                "Mono represented as Gray8 coverage without changing samples",
            ));
        }
        let entries = expected_report
            .into_iter()
            .map(|(code, glyph_index, message)| ReportEntry {
                level: Level::Warning,
                impact: Impact::Loss,
                code: format!("embedded.{code}"),
                location: Location {
                    font_index: Some(0),
                    glyph_index,
                    ..Default::default()
                },
                message: message.into(),
            })
            .collect::<Vec<_>>();
        assert_eq!(written.report.entries, entries);
        let bytes = written.output.into_single().unwrap();
        let limits = Default::default();
        let sfnt = Sfnt::parse(&bytes, &limits).unwrap();
        sfnt.validate_checksums().unwrap();
        let expected = scalar_tables(&[source_strike], Kind::OpenType);
        if !mixed_gray {
            assert_eq!(
                expected.data,
                [
                    0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 3, 255, 1, 4, 160, 64, 1, 9, 0, 1, 10, 128, 128
                ]
            );
        }
        assert_eq!(sfnt.table(*b"EBDT").unwrap(), expected.data);
        assert_eq!(sfnt.table(*b"EBLC").unwrap(), expected.location);
        // Rebuild the complete native sfnt with independently scalar-packed
        // bitmap tables, covering directory offsets and checksum adjustment.
        let mut reference = SfntBuilder::new(sfnt.flavor());
        for table in sfnt.tables() {
            let data = match &table.tag {
                b"EBDT" => expected.data.clone(),
                b"EBLC" => expected.location.clone(),
                _ => table.data.to_vec(),
            };
            reference.insert(table.tag, data).unwrap();
        }
        assert_eq!(bytes, reference.build(&limits).unwrap());
        options.strict = true;
        let fontpixel::Error::StrictLoss(report) =
            fontpixel::write(&set, "otb", &options).unwrap_err()
        else {
            panic!("lossy native write must fail strict mode")
        };
        assert_eq!(report.entries, entries);
    }
}
