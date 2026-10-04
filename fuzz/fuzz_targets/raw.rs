#![no_main]
use fontpixel::bitmap::{BitOrder, RowOrder};
use fontpixel::{
    ResourceLimits,
    formats::raw::{self, RawByteOrder, RawLayout, RawPacking, RawReadOptions, RawWriteOptions},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    if bytes.len() < 4 {
        return;
    }
    let mut layout = RawLayout::new(u32::from(bytes[0]) + 1, u32::from(bytes[1]) + 1);
    layout.row_padding = usize::from(bytes[2] & 7);
    layout.bit_order = if bytes[2] & 8 == 0 {
        BitOrder::MsbFirst
    } else {
        BitOrder::LsbFirst
    };
    layout.row_order = if bytes[2] & 16 == 0 {
        RowOrder::TopDown
    } else {
        RowOrder::BottomUp
    };
    layout.offset = usize::from(bytes[3]);
    if bytes[2] & 32 != 0 {
        layout.packing = RawPacking::BitAligned;
        layout.row_padding = 0;
    } else {
        if bytes[2] & 64 != 0 {
            layout.byte_order = RawByteOrder::ColumnMajor;
        }
        if bytes[2] & 128 != 0 {
            layout.strike_count = [2, 3, 8, 256, 512][usize::from(bytes[3] % 5)];
        }
    }
    let o = RawReadOptions {
        layout: Some(layout.clone()),
    };
    let l = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    if let Ok((font, _)) = raw::read(&bytes[4..], &o, &l) {
        if let Ok((b, r)) = raw::write(
            &font,
            &RawWriteOptions {
                layout: Some(layout),
                limits: l.clone(),
                max_output_bytes: 65536,
                ..Default::default()
            },
        ) {
            let back = raw::read(&b, &o, &l)
                .expect("successful raw output must decode")
                .0;
            if !r.has_loss() {
                assert_eq!(font, back);
            }
        }
    }
});
