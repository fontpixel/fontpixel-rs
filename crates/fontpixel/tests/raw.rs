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
