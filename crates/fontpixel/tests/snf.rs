//! Original SNF wire fixtures; no production writer is used to create them.
#![cfg(feature = "snf")]
use fontpixel::bitmap::{ByteLayout, ByteOrder};
use fontpixel::formats::snf::{self, SnfReadOptions, SnfWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn p16(b: &mut [u8], at: usize, v: u16, e: ByteOrder) {
    b[at..at + 2].copy_from_slice(&if e == ByteOrder::Big {
        v.to_be_bytes()
    } else {
        v.to_le_bytes()
    });
}
fn p32(b: &mut [u8], at: usize, v: u32, e: ByteOrder) {
    b[at..at + 4].copy_from_slice(&if e == ByteOrder::Big {
        v.to_be_bytes()
    } else {
        v.to_le_bytes()
    });
}
fn fixture(e: ByteOrder, ink: bool) -> Vec<u8> {
    let mut b = vec![0; 148 + if ink { 48 } else { 0 }];
    for (at, v) in [
        (0, 4),
        (4, 1),
        (28, 65),
        (32, 65),
        (44, 1),
        (48, 8),
        (52, 65),
        (56, 1),
        (60, 2),
        (96, 1),
        (100, 1),
        (104, 4),
    ] {
        p32(&mut b, at, v, e);
    }
    for at in [64, 80, 108] {
        for (i, v) in [65535, 2, 4, 2, 1, 3].into_iter().enumerate() {
            p16(&mut b, at + i * 2, v, e);
        }
    }
    p32(
        &mut b,
        120,
        if e == ByteOrder::Little { 1 << 24 } else { 128 },
        e,
    );
    p32(
        &mut b,
        92,
        if e == ByteOrder::Little { 3 } else { 3 << 8 },
        e,
    );
    b[124..127].copy_from_slice(&[0xa0, 0x40, 0xe0]);
    p32(&mut b, 128, 0, e);
    p32(&mut b, 132, 17, e);
    b[140..148].copy_from_slice(b"TESTING\0");
    if ink {
        p32(
            &mut b,
            24,
            if e == ByteOrder::Little {
                8
            } else {
                0x10000000
            },
            e,
        );
        for at in [148, 164, 180] {
            for (i, v) in [65535, 2, 4, 2, 1, 0].into_iter().enumerate() {
                p16(&mut b, at + i * 2, v, e);
            }
        }
    }
    b
}
fn load(b: &[u8], o: SnfReadOptions, l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    Ok(snf::read(&mut ReadContext::new(&input, l)?, &o)?.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("test.snf").unwrap())
}
#[test]
fn independent_endian_bitfields_and_ink_metrics() {
    for e in [ByteOrder::Little, ByteOrder::Big] {
        for ink in [false, true] {
            let s = load(
                &fixture(e, ink),
                SnfReadOptions {
                    bitmap: Some(ByteLayout::default()),
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap();
            let f = s.single().unwrap();
            let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
            assert_eq!(g.bitmap.as_bytes(), [0xa0, 0x40, 0xe0]);
            assert_eq!(g.metrics.bounds.x, -1);
            assert_eq!(g.metrics.bounds.y, -1);
            assert_eq!(g.metrics.advance.unwrap().x, 4);
            assert_eq!(g.properties.get("snf.ink_metrics").is_some(), ink);
            let w = snf::write(
                &s,
                &options(),
                &SnfWriteOptions {
                    byte_order: e,
                    include_layout_property: false,
                    bitmap: ByteLayout::default(),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(!w.report.has_loss(), "{}", w.report);
            assert_eq!(
                load(
                    &w.output.into_single().unwrap(),
                    SnfReadOptions {
                        bitmap: Some(ByteLayout::default()),
                        ..Default::default()
                    },
                    &Default::default()
                )
                .unwrap(),
                s
            );
        }
    }
}
#[test]
fn unrecorded_layout_is_not_guessed_and_default_output_is_portable() {
    let b = fixture(ByteOrder::Little, false);
    assert!(load(&b, Default::default(), &Default::default()).is_err());
    let s = load(
        &b,
        SnfReadOptions {
            bitmap: Some(ByteLayout::default()),
            ..Default::default()
        },
        &Default::default(),
    )
    .unwrap();
    for e in [ByteOrder::Little, ByteOrder::Big] {
        for alignment in [1, 2, 4, 8] {
            for bit in [
                fontpixel::bitmap::BitOrder::MsbFirst,
                fontpixel::bitmap::BitOrder::LsbFirst,
            ] {
                let layout = ByteLayout {
                    bit_order: bit,
                    byte_order: e,
                    row_alignment: alignment,
                    scan_unit: alignment,
                    ..Default::default()
                };
                let w = snf::write(
                    &s,
                    &options(),
                    &SnfWriteOptions {
                        bitmap: layout,
                        byte_order: e,
                        ..Default::default()
                    },
                )
                .unwrap();
                let first = load(
                    &w.output.into_single().unwrap(),
                    Default::default(),
                    &Default::default(),
                )
                .unwrap();
                assert_eq!(
                    first.fonts[0].glyphs().next().unwrap().1.bitmap,
                    s.fonts[0].glyphs().next().unwrap().1.bitmap
                );
                let w = snf::write(
                    &first,
                    &options(),
                    &SnfWriteOptions {
                        bitmap: layout,
                        byte_order: e,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(!w.report.has_loss(), "{}", w.report);
                assert_eq!(
                    load(
                        &w.output.into_single().unwrap(),
                        Default::default(),
                        &Default::default()
                    )
                    .unwrap(),
                    first
                );
            }
        }
    }
}
#[test]
fn truncations_invalid_offsets_and_limits() {
    let b = fixture(ByteOrder::Little, true);
    let o = SnfReadOptions {
        bitmap: Some(ByteLayout::default()),
        ..Default::default()
    };
    for n in 0..b.len() {
        assert!(
            load(&b[..n], o.clone(), &Default::default()).is_err(),
            "{n}"
        );
    }
    for i in 0..b.len() {
        for v in [0, 255] {
            let mut bad = b.clone();
            bad[i] = v;
            let _ = load(
                &bad,
                o.clone(),
                &ResourceLimits {
                    max_glyphs: 300,
                    max_total_bytes: 100000,
                    ..Default::default()
                },
            );
        }
    }
    let mut bad = b;
    p32(&mut bad, 120, 0x01ffffff, ByteOrder::Little);
    assert!(load(&bad, o, &Default::default()).is_err());
}

proptest::proptest! {
 #[test]
 fn random_sparse_rectangles(widths in proptest::collection::vec(0u32..20,1..20),height in 0u32..10,seed in proptest::num::u32::ANY){
  let mut state=seed;let mut glyphs=Vec::new();for(i,w)in widths.iter().enumerate(){let bytes:Vec<_>=(0..(*w as usize).div_ceil(8)*height as usize).map(|_|{state=state.wrapping_mul(1664525).wrapping_add(1013904223);(state>>24)as u8}).collect();let mut g=fontpixel::model::Glyph::new(fontpixel::bitmap::Bitmap::<fontpixel::bitmap::Mono>::from_raw(*w,height,&bytes,Default::default()).unwrap());g.codepoints.push(Codepoint::Legacy(0x141+i as u32*257));g.metrics.advance=Some(fontpixel::model::Vector::new(i64::from(*w),0));glyphs.push(g);}
  let s=FontSet::from(fontpixel::Font::from_glyphs(glyphs).unwrap());let w=snf::write(&s,&options(),&Default::default()).unwrap();let s=load(&w.output.into_single().unwrap(),Default::default(),&Default::default()).unwrap();let w=snf::write(&s,&options(),&Default::default()).unwrap();proptest::prop_assert!(!w.report.has_loss(),"{}",w.report);proptest::prop_assert_eq!(load(&w.output.into_single().unwrap(),Default::default(),&Default::default()).unwrap(),s);
 }
}
