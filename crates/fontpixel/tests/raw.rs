//! Independent fixtures and property checks for raw.
#![cfg(feature = "raw")]
use fontpixel::bitmap::{BitOrder, RowOrder};
use fontpixel::formats::raw::{self, RawLayout, RawPreset, RawReadOptions, RawWriteOptions};
use fontpixel::{Input, ResourceLimits};
use proptest::prelude::*;
#[test]
fn explicit_configuration_and_named_presets() {
    let l = ResourceLimits::default();
    assert!(raw::read(&[0; 16], &Default::default(), &l).is_err());
    assert_eq!(RawPreset::Vga16.layout().height, 16);
    assert_eq!(
        "spectrum"
            .parse::<RawPreset>()
            .unwrap()
            .layout()
            .first_codepoint,
        32
    );
    assert!(
        fontpixel::registry()
            .detect(&Input::new(&[0; 2048]), &l)
            .is_err()
    );
    let o = RawReadOptions {
        layout: Some(RawLayout::new(8, 8)),
    };
    let (s, _) = raw::read(&[0; 16], &o, &l).unwrap();
    assert_eq!(s.fonts[0].len(), 2);
}
#[test]
fn prefix_padding_and_exact_count() {
    let mut layout = RawLayout::new(9, 2);
    layout.offset = 2;
    layout.row_padding = 2;
    layout.count = Some(1);
    layout.bit_order = BitOrder::LsbFirst;
    layout.row_order = RowOrder::BottomUp;
    let bytes = [0, 0x38, 1, 0x81, 1, 0, 2, 1, 0, 0];
    let (s, r) = raw::read(
        &bytes,
        &RawReadOptions {
            layout: Some(layout.clone()),
        },
        &ResourceLimits::default(),
    )
    .unwrap();
    assert_eq!(r.entries.len(), 1);
    let (b, r) = raw::write(
        &s,
        &RawWriteOptions {
            layout: Some(layout),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(&b[..2], &[0, 0x38]);
    assert_eq!(&b[2..], &[1, 1, 0, 0, 2, 1, 0, 0]);
}
#[test]
fn read_and_write_limits_are_equal() {
    let layout = RawLayout::new(8, 8);
    let o = RawReadOptions {
        layout: Some(layout.clone()),
    };
    let (s, _) = raw::read(&[0; 16], &o, &Default::default()).unwrap();
    let limits = ResourceLimits {
        max_total_bytes: 2000,
        ..Default::default()
    };
    assert!(
        raw::write(
            &s,
            &RawWriteOptions {
                layout: Some(layout),
                limits,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn malformed_geometry_and_trailing_bytes() {
    for (width, height, bytes) in [(0, 8, 8), (8, 0, 8), (8, 7, 8)] {
        assert!(
            raw::read(
                &vec![0; bytes],
                &RawReadOptions {
                    layout: Some(RawLayout::new(width, height))
                },
                &Default::default()
            )
            .is_err()
        );
    }
    let mut l = RawLayout::new(8, 8);
    l.first_codepoint = u32::MAX;
    l.count = Some(2);
    assert!(
        raw::read(
            &[0; 16],
            &RawReadOptions { layout: Some(l) },
            &Default::default()
        )
        .is_err()
    );
}
proptest! {#[test]fn random_packed_rows_roundtrip(width in 1u32..33,height in 1u32..20,count in 1usize..12,lsb in any::<bool>(),bottom in any::<bool>(),padding in 0usize..4,values in prop::collection::vec(any::<u8>(),8192)){
 let mut layout=RawLayout::new(width,height);layout.count=Some(count);layout.bit_order=if lsb{BitOrder::LsbFirst}else{BitOrder::MsbFirst};layout.row_order=if bottom{RowOrder::BottomUp}else{RowOrder::TopDown};layout.row_padding=padding;let stride=(width as usize).div_ceil(8)+padding;let n=stride*height as usize*count;let input=&values[..n];let o=RawReadOptions{layout:Some(layout.clone())};let(s,_)=raw::read(input,&o,&Default::default()).unwrap();let(bytes,r)=raw::write(&s,&RawWriteOptions{layout:Some(layout),..Default::default()}).unwrap();prop_assert!(!r.has_loss(),"{r}");let(back,r)=raw::read(&bytes,&o,&Default::default()).unwrap();prop_assert_eq!(s,back);prop_assert!(!r.has_loss());
}}
#[test]
fn offset_wire_and_source_are_reserved_before_allocating() {
    let layout = RawLayout::new(8, 8);
    let (s, _) = raw::read(
        &[0; 8],
        &RawReadOptions {
            layout: Some(layout.clone()),
        },
        &Default::default(),
    )
    .unwrap();
    let mut large = layout;
    large.offset = 32768;
    let e = raw::write(
        &s,
        &RawWriteOptions {
            layout: Some(large),
            limits: ResourceLimits {
                max_total_bytes: 65536,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(e, fontpixel::Error::ResourceLimit { .. }));
}
#[test]
fn diagnostic_asset_names_are_in_the_read_budget() {
    let limits = ResourceLimits {
        max_total_bytes: 16384,
        ..Default::default()
    };
    let ro = RawReadOptions {
        layout: Some(RawLayout::new(1, 1)),
    };
    assert_eq!(raw::read(&[1], &ro, &limits).unwrap().1.entries.len(), 1);
    let mut registry = fontpixel::registry();
    registry.set_raw_read_options(ro);
    let name = "x".repeat(32768);
    let input = Input::new(&[1]).with_name(&name);
    let e = registry
        .read(
            &input,
            &fontpixel::formats::ReadOptions {
                format: Some("raw".into()),
                limits,
            },
        )
        .unwrap_err();
    assert!(matches!(e, fontpixel::Error::ResourceLimit { .. }));
}

#[test]
fn c64_prg_default_and_preserved_load_addresses() {
    let layout = RawPreset::C64Prg.layout();
    let source = RawReadOptions {
        layout: Some(RawLayout::new(8, 8)),
    };
    let pixels: Vec<u8> = (0..512).map(|i| (i * 37 + 11) as u8).collect();
    let (s, _) = raw::read(&pixels, &source, &Default::default()).unwrap();
    let (prg, report) = raw::write(
        &s,
        &RawWriteOptions {
            layout: Some(layout.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    // Independent PRG consumer: the first little-endian word is the destination.
    assert_eq!(u16::from_le_bytes([prg[0], prg[1]]), 0x3800);
    assert_eq!(&prg[2..], pixels);
    assert!(report.has_loss()); // Added prefix and explicit charset are reported.
    let mut at_2000 = prg;
    at_2000[1] = 0x20;
    let (loaded, _) = raw::read(
        &at_2000,
        &RawReadOptions {
            layout: Some(layout.clone()),
        },
        &Default::default(),
    )
    .unwrap();
    assert_eq!(loaded.fonts[0].len(), 64);
    let (again, report) = raw::write(
        &loaded,
        &RawWriteOptions {
            layout: Some(layout),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(again, at_2000);
    assert!(!report.has_loss(), "{report}");
}

// Independent fixture assembler: concatenate raster rows as strings, pack them,
// then transpose the resulting byte matrix. It does not use native address helpers.
fn fixture(layout: &RawLayout, count: usize) -> (Vec<u8>, Vec<Vec<u8>>) {
    use fontpixel::formats::raw::{RawByteOrder, RawPacking};
    let w = layout.width as usize;
    let h = layout.height as usize;
    let glyphs: Vec<Vec<u8>> = (0..count)
        .map(|g| {
            (0..w * h)
                .map(|p| u8::from((g * 11 + p * 7 + p / w * 3) % 13 < 6))
                .collect()
        })
        .collect();
    let mut rows = Vec::new();
    let groups = if layout.packing == RawPacking::BitAligned {
        count
    } else {
        count.div_ceil(layout.strike_count)
    };
    for group in 0..groups {
        for y in 0..h {
            let y = if layout.row_order == RowOrder::BottomUp {
                h - 1 - y
            } else {
                y
            };
            let mut row = Vec::new();
            let cells = if layout.packing == RawPacking::BitAligned {
                1
            } else {
                layout.strike_count
            };
            for cell in 0..cells {
                let index = group * cells + cell;
                row.extend_from_slice(
                    glyphs
                        .get(index)
                        .map_or(&vec![0; w][..], |g| &g[y * w..(y + 1) * w]),
                );
            }
            if layout.packing == RawPacking::ByteAlignedRows {
                row.resize(row.len().div_ceil(8) * 8 + layout.row_padding * 8, 0);
            }
            rows.push(row);
        }
    }
    let flat: Vec<u8> = rows.concat();
    let mut body: Vec<u8> = flat
        .chunks(8)
        .map(|bits| {
            bits.iter().enumerate().fold(0, |b, (i, &v)| {
                b | (v
                    << if layout.bit_order == BitOrder::MsbFirst {
                        7 - i
                    } else {
                        i
                    })
            })
        })
        .collect();
    if layout.byte_order == RawByteOrder::ColumnMajor {
        let stride = (w * layout.strike_count).div_ceil(8) + layout.row_padding;
        body = body
            .chunks(stride * h)
            .flat_map(|group| {
                (0..stride).flat_map(move |x| (0..h).map(move |y| group[y * stride + x]))
            })
            .collect();
    }
    let mut bytes = vec![0; layout.offset];
    bytes.extend(body);
    let canonical = glyphs
        .iter()
        .map(|g| {
            g.chunks(w)
                .flat_map(|r| {
                    r.chunks(8).map(|bits| {
                        bits.iter()
                            .enumerate()
                            .fold(0, |b, (i, &v)| b | (v << (7 - i)))
                    })
                })
                .collect()
        })
        .collect();
    (bytes, canonical)
}
#[test]
fn independently_packed_strikes_bits_and_byte_columns() {
    use fontpixel::formats::raw::{RawByteOrder, RawPacking};
    for width in [1, 3, 8, 9, 16] {
        for packing in [RawPacking::ByteAlignedRows, RawPacking::BitAligned] {
            for column in [false, true] {
                for lsb in [false, true] {
                    for bottom in [false, true] {
                        if packing == RawPacking::BitAligned && column {
                            continue;
                        }
                        let mut layout = RawLayout::new(width, 3);
                        layout.count = Some(5);
                        layout.offset = 2;
                        layout.packing = packing;
                        layout.byte_order = if column {
                            RawByteOrder::ColumnMajor
                        } else {
                            RawByteOrder::RowMajor
                        };
                        layout.bit_order = if lsb {
                            BitOrder::LsbFirst
                        } else {
                            BitOrder::MsbFirst
                        };
                        layout.row_order = if bottom {
                            RowOrder::BottomUp
                        } else {
                            RowOrder::TopDown
                        };
                        if packing == RawPacking::ByteAlignedRows {
                            layout.strike_count = 3;
                            layout.row_padding = 1;
                        }
                        let (wire, expected) = fixture(&layout, 5);
                        let (s, r) = raw::read(
                            &wire,
                            &RawReadOptions {
                                layout: Some(layout.clone()),
                            },
                            &Default::default(),
                        )
                        .unwrap();
                        assert!(r.entries.is_empty(), "{layout:?}: {r}");
                        for ((_, g), pixels) in s.fonts[0].glyphs().zip(expected) {
                            assert_eq!(g.bitmap.as_bytes(), pixels, "{layout:?}");
                        }
                        let (out, r) = raw::write(
                            &s,
                            &RawWriteOptions {
                                layout: Some(layout.clone()),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        assert!(!r.has_loss(), "{layout:?}: {r}");
                        assert_eq!(out, wire, "{layout:?}");
                    }
                }
            }
        }
    }
}
#[test]
fn new_layout_padding_counts_and_limits() {
    use fontpixel::formats::raw::{RawByteOrder, RawPacking};
    let mut bit = RawLayout::new(3, 3);
    bit.packing = RawPacking::BitAligned;
    bit.count = Some(1);
    let (s, r) = raw::read(
        &[0xff, 0xff],
        &RawReadOptions {
            layout: Some(bit.clone()),
        },
        &Default::default(),
    )
    .unwrap();
    assert_eq!(r.entries.len(), 1);
    let (b, r) = raw::write(
        &s,
        &RawWriteOptions {
            layout: Some(bit.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(b, [0xff, 0x80]);
    assert!(!r.has_loss());
    for change in 0..3 {
        let mut bad = bit.clone();
        match change {
            0 => bad.strike_count = 2,
            1 => bad.row_padding = 1,
            _ => bad.byte_order = RawByteOrder::ColumnMajor,
        };
        assert!(
            raw::read(
                &b,
                &RawReadOptions { layout: Some(bad) },
                &Default::default()
            )
            .is_err()
        );
    }
    for count in [Some(0), Some(2), Some(usize::MAX)] {
        let mut bad = bit.clone();
        bad.count = count;
        assert!(
            raw::read(
                &b,
                &RawReadOptions { layout: Some(bad) },
                &Default::default()
            )
            .is_err()
        );
    }
    let mut strike = RawLayout::new(3, 1);
    strike.count = Some(1);
    strike.strike_count = 2;
    strike.byte_order = RawByteOrder::ColumnMajor;
    let (s, r) = raw::read(
        &[0x1f],
        &RawReadOptions {
            layout: Some(strike.clone()),
        },
        &Default::default(),
    )
    .unwrap();
    assert_eq!(r.entries.len(), 1);
    assert_eq!(
        raw::write(
            &s,
            &RawWriteOptions {
                layout: Some(strike.clone()),
                ..Default::default()
            }
        )
        .unwrap()
        .0,
        [0]
    );
    strike.strike_count = usize::MAX;
    assert!(
        raw::read(
            &[0],
            &RawReadOptions {
                layout: Some(strike)
            },
            &Default::default()
        )
        .is_err()
    );
    let mut tall = RawLayout::new(1, 32768);
    tall.packing = RawPacking::BitAligned;
    tall.count = Some(1);
    let small = RawWriteOptions {
        layout: Some(tall.clone()),
        limits: ResourceLimits {
            max_total_bytes: 65536,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        raw::write(&s, &small),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    let generous = RawWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 262144,
            ..Default::default()
        },
        ..small
    };
    let (packed, _) = raw::write(&s, &generous).unwrap();
    assert_eq!(packed.len(), 4096);
    assert_eq!(
        raw::read(
            &packed,
            &RawReadOptions { layout: Some(tall) },
            &generous.limits
        )
        .unwrap()
        .0
        .fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes()
            .len(),
        32768
    );
    for limits in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 1024,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            raw::read(
                &b,
                &RawReadOptions {
                    layout: Some(bit.clone())
                },
                &limits
            ),
            Err(fontpixel::Error::ResourceLimit { .. })
        ));
    }
}
#[test]
fn harlekin_presets_have_distinct_documented_geometries() {
    for (name, count, height) in [
        ("harlekin-iii", 512, 8),
        ("harlekin8", 256, 8),
        ("harlekin16", 256, 16),
    ] {
        let layout = name.parse::<RawPreset>().unwrap().layout();
        assert_eq!(layout.count, Some(count));
        assert_eq!(layout.strike_count, count);
        assert_eq!(layout.height, height);
        let (wire, pixels) = fixture(&layout, count);
        let (s, _) = raw::read(
            &wire,
            &RawReadOptions {
                layout: Some(layout.clone()),
            },
            &Default::default(),
        )
        .unwrap();
        for ((_, g), p) in s.fonts[0].glyphs().zip(pixels) {
            assert_eq!(g.bitmap.as_bytes(), p)
        }
        assert_eq!(
            raw::write(
                &s,
                &RawWriteOptions {
                    layout: Some(layout),
                    ..Default::default()
                }
            )
            .unwrap()
            .0,
            wire
        );
    }
}

#[test]
fn new_layout_strict_metadata_and_context_budget() {
    use fontpixel::formats::raw::{RawCodec, RawPacking};
    use fontpixel::formats::{FormatReader, FormatWriter, ReadOptions, WriteOptions};
    use fontpixel::resources::{ReadContext, ResourcePath};
    let mut layout = RawLayout::new(3, 3);
    layout.packing = RawPacking::BitAligned;
    layout.count = Some(1);
    let codec = RawCodec {
        read_options: RawReadOptions {
            layout: Some(layout.clone()),
        },
        write_options: RawWriteOptions {
            layout: Some(layout),
            ..Default::default()
        },
    };
    let bytes = [0xa5, 0x80];
    let input = Input::new(&bytes);
    let limits = ResourceLimits::default();
    let (mut s, _) = codec
        .read(
            &mut ReadContext::new(&input, &limits).unwrap(),
            &ReadOptions::default(),
        )
        .unwrap();
    let mut o = WriteOptions::new(ResourcePath::new("font.raw").unwrap());
    o.strict = true;
    assert!(codec.write(&s, &o).is_ok());
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.name = Some("omitted".into());
            g.unicode_sequences.push(vec![65, 0x301]);
            Ok(())
        })
        .unwrap();
    let e = codec.write(&s, &o).unwrap_err();
    assert!(matches!(e, fontpixel::Error::StrictLoss(_)));
    let r = e.report().unwrap();
    assert!(r.entries.iter().any(|e| e.location.glyph_index == Some(0)));
    let small = ResourceLimits {
        max_glyphs: 0,
        ..Default::default()
    };
    assert!(matches!(
        codec.read(
            &mut ReadContext::new(&input, &small).unwrap(),
            &ReadOptions::default()
        ),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}
