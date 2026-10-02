//! Bitmap operations, resource boundaries, wire layouts and scalar differential tests.
use fontpixel::Result;
use fontpixel::bitmap::*;

fn mono(text: &str) -> Bitmap {
    text.parse().unwrap()
}
fn gray(rows: &[&[u8]]) -> Bitmap<Gray8> {
    let width = rows.first().map_or(0, |r| r.len()) as u32;
    Bitmap::from_raw(
        width,
        rows.len() as u32,
        &rows.concat(),
        BitmapLimits::default(),
    )
    .unwrap()
}

#[test]
fn canonical_storage_limits_and_invalid_input() -> Result<()> {
    let b = Bitmap::<Mono>::from_raw(9, 2, &[0xff, 0xff, 0x80, 0x7f], BitmapLimits::default())?;
    assert_eq!(b.as_bytes(), &[0xff, 0x80, 0x80, 0]);
    assert_eq!(b.stride(), 2);
    assert!(b.get_pixel(9, 0).is_err());
    assert!(b.get_pixel(0, 2).is_err());
    let mut c = b.clone();
    assert!(c.set_pixel(0, 0, 2).is_err());
    assert_eq!(b, c);
    assert!(Bitmap::<Mono>::new(u32::MAX, 0).is_err());
    assert!(Bitmap::<Mono>::new(u32::MAX, u32::MAX).is_err());
    assert!(Bitmap::<Mono>::from_raw(9, 2, &[0], BitmapLimits::default()).is_err());
    let small = BitmapLimits {
        max_dimension: 20,
        max_pixels: 20,
        max_bytes: 20,
    };
    assert!(Bitmap::<Mono>::with_limits(5, 5, small).is_err());
    assert!(Bitmap::<Gray8>::with_limits(4, 6, small).is_err());
    assert!(mono("#").crop(u32::MAX, 1, 0, 0).is_err());
    assert!(mono("#").enlarge(0, 1).is_err());
    assert!(mono("#").enlarge(u32::MAX, 2).is_err());
    assert!(mono("#").rotate(45).is_err());
    assert!(mono("#").italic(0, 0).is_err());
    assert!(mono("#").byte_pad(0).is_err());
    assert!(mono("#").shadow(i64::MIN, 0, 1).is_err());
    assert!(mono("#").shadow(0, i64::MAX, 1).is_err());
    assert!(mono("#").bold(u32::MAX).is_err());
    assert!(mono("#").extend(u32::MAX, 1, 0, 0).is_err());
    assert!(
        mono("#")
            .concat(&mono("#"), Direction::Right, Alignment::Start, i64::MAX)
            .is_err()
    );
    let a = mono("#");
    assert!(Bitmap::concat_all(&[&a], Direction::Right, Alignment::Start, &[0], small).is_err());
    Ok(())
}

#[test]
fn zero_dimensions_are_explicit() -> Result<()> {
    for (w, h) in [(0, 0), (0, 4), (5, 0)] {
        let b = Bitmap::<Mono>::new(w, h)?;
        assert_eq!((b.width(), b.height()), (w, h));
        assert_eq!(b.as_bytes(), &[]);
        assert_eq!(b.bbox(), Bounds::default());
        assert_eq!(b.trim()?, Bitmap::default());
        assert_eq!(b.rotate(90)?.rotate(-90)?, b);
        assert_eq!(b.flip_horizontal()?.flip_horizontal()?, b);
        assert_eq!(b.flip_vertical()?.flip_vertical()?, b);
        assert_eq!(b.to_gray8()?.to_mono(128)?, b);
        assert_eq!(b.to_bytes(ByteLayout::default())?, Vec::<u8>::new());
    }
    assert_ne!(Bitmap::<Mono>::new(1, 0)?, Bitmap::<Mono>::new(2, 0)?);
    let z = Bitmap::<Mono>::new(0, 3)?;
    assert_eq!(z.to_numeric_hex_rows()?, vec!["", "", ""]);
    assert_eq!(z.to_row_values()?, vec![0, 0, 0]);
    let a = mono("#");
    let b = Bitmap::concat_all(
        &[&z, &a, &z, &a],
        Direction::Down,
        Alignment::Start,
        &[999, 999, 0],
        BitmapLimits::default(),
    )?;
    assert_eq!(b.to_string(), "#\n#");
    Ok(())
}

#[test]
fn strings_palettes_hex_and_thresholds() -> Result<()> {
    let b: Bitmap = " \r\n.#\r\n10\n\n".parse()?;
    assert_eq!(b.to_string(), ".#\n#.");
    for text in ["#\n##", "&", "# #", "#\n\n#"] {
        assert!(text.parse::<Bitmap>().is_err());
    }
    assert!(
        Bitmap::<Gray8>::from_text("x", &[('x', 1), ('x', 2)], BitmapLimits::default()).is_err()
    );
    assert_eq!(b.to_text(&[(0, '白'), (1, '黑')])?, "白黑\n黑白");
    assert!(b.to_text(&[(0, '.')]).is_err());
    assert!(b.to_text(&[(0, '.'), (0, '#')]).is_err());
    assert_eq!(
        b.to_palette_bytes(&[(0, &[255, 255, 255]), (1, &[0, 0, 0])])?,
        vec![255, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255]
    );
    assert!(b.to_palette_bytes(&[(0, &[0]), (1, &[0, 0])]).is_err());
    assert!(b.to_palette_bytes(&[(0, &[0])]).is_err());
    assert!(b.to_palette_bytes(&[]).is_err());
    let b = mono("#...#....");
    assert_eq!(b.to_hex_rows()?, vec!["8800"]);
    assert_eq!(b.to_numeric_hex_rows()?, vec!["110"]);
    assert_eq!(b.to_row_values()?, vec![272]);
    assert_eq!(
        Bitmap::<Mono>::from_hex_rows(9, &["887f"], BitmapLimits::default())?,
        b
    );
    for rows in [vec!["zz"], vec!["0"], vec!["0000"]] {
        assert!(Bitmap::<Mono>::from_hex_rows(8, &rows, BitmapLimits::default()).is_err());
    }
    assert!(Bitmap::<Mono>::new(65, 1)?.to_row_values().is_err());
    let g = gray(&[&[0, 1, 127, 128, 254, 255]]);
    assert_eq!(g.to_mono(128)?.to_string(), "...###");
    assert_eq!(g.to_mono(0)?.to_string(), "######");
    assert_eq!(g.to_mono(255)?.to_string(), ".....#");
    let mut inverse = g.clone();
    inverse.invert();
    assert_eq!(inverse.as_bytes(), &[255, 0, 0, 0, 0, 0]);
    inverse = g.clone();
    inverse.negate();
    assert_eq!(inverse.as_bytes(), &[255, 254, 128, 127, 1, 0]);
    let mut b = mono("#.#");
    b.invert();
    assert_eq!(b.as_bytes(), &[0x40]);
    Ok(())
}

#[test]
fn checked_geometry_and_label_effects() -> Result<()> {
    let b = mono(".....\n..##.\n...#.\n.....");
    assert_eq!(b.bbox(), Bounds::new(2, 2, 2, 1));
    assert_eq!(b.trim()?.to_string(), "##\n.#");
    assert_eq!(mono("#").crop(3, 3, -1, -1)?.to_string(), "...\n.#.\n...");
    assert_eq!(mono("##\n.#").shift(1, 1)?.to_string(), "..\n..");
    assert!(!b.crop(2, 2, i64::MIN, i64::MAX)?.has_ink());
    assert!(!b.shift(i64::MIN, i64::MIN)?.has_ink());
    let mut c = b.clone();
    c.paste(&b, i64::MAX, i64::MIN);
    assert_eq!(b, c);
    assert_eq!(mono("#..\n.##").rotate(90)?.to_string(), ".#\n#.\n#.");
    assert_eq!(mono("#.").enlarge(3, 2)?.to_string(), "###...\n###...");
    assert_eq!(mono("#.#").byte_pad(8)?.to_string(), "#.#.....");
    let labels = Bitmap::<Gray8>::from_text(
        ".#.\n.&.",
        &[('.', 0), ('#', 1), ('&', 2)],
        BitmapLimits::default(),
    )?;
    assert_eq!(
        labels.shadow(1, -1, 2)?.as_bytes(),
        &[0, 1, 0, 0, 0, 2, 2, 0, 0, 0, 2, 0]
    );
    let glow = labels.glow(false, 1, 2)?;
    assert_eq!(
        glow.as_bytes(),
        &[0, 0, 2, 0, 0, 0, 2, 1, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(labels.bold(1)?.as_bytes(), &[0, 1, 1, 0, 0, 2, 2, 0]);
    assert_eq!(mono("#\n#\n#").italic(2, 1)?.to_string(), ".#\n.#\n#.");
    assert_eq!(mono("#\n#").italic(i64::MAX, i64::MIN)?.to_string(), "#\n#");
    let p = gray(&[&[1, 2, 1, 2], &[2, 1, 2, 1]]);
    assert_eq!(
        p.replace_pattern(&[1, 2], &[2, 1])?.as_bytes(),
        &[2, 1, 2, 1, 2, 2, 1, 1]
    );
    assert!(p.replace_pattern(&[1, 2], &[1]).is_err());
    assert_eq!(mono("#.").replace_pattern(&[], &[1])?.to_string(), "###.#");
    let mut p = mono("#.");
    p.replace(0, 1)?;
    assert_eq!(p.to_string(), "##");
    p.replace(1, 0)?;
    assert_eq!(p.to_string(), "..");
    Ok(())
}

fn random(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 16) as u32
}
fn operation(a: u8, b: u8, mode: CompositeMode, max: u8) -> u8 {
    match mode {
        CompositeMode::Over => {
            if b == 0 {
                a
            } else {
                b
            }
        }
        CompositeMode::Replace => b,
        CompositeMode::Or => a | b,
        CompositeMode::And => a & b,
        CompositeMode::Xor => a ^ b,
        CompositeMode::Clear => a & !b,
        CompositeMode::Add => a.saturating_add(b).min(max),
        CompositeMode::Subtract => a.saturating_sub(b),
        CompositeMode::Max => a.max(b),
        CompositeMode::Min => a.min(b),
        CompositeMode::Multiply => {
            ((u32::from(a) * u32::from(b) + u32::from(max) / 2) / u32::from(max)) as u8
        }
        CompositeMode::AlphaOver => {
            (u32::from(a) + u32::from(b)
                - (u32::from(a) * u32::from(b) + u32::from(max) / 2) / u32::from(max))
                as u8
        }
    }
}
fn differential<F: PixelFormat>() -> Result<()> {
    let mut seed = 0xcafef00d12345678;
    let modes = [
        CompositeMode::Over,
        CompositeMode::Replace,
        CompositeMode::Or,
        CompositeMode::And,
        CompositeMode::Xor,
        CompositeMode::Clear,
        CompositeMode::Add,
        CompositeMode::Subtract,
        CompositeMode::Max,
        CompositeMode::Min,
        CompositeMode::Multiply,
        CompositeMode::AlphaOver,
    ];
    for iteration in 0..4000 {
        let (w, h, sw, sh) = (
            random(&mut seed) % 65,
            random(&mut seed) % 12,
            random(&mut seed) % 65,
            random(&mut seed) % 12,
        );
        let mut a = Bitmap::<F>::new(w, h)?;
        let mut b = Bitmap::<F>::new(sw, sh)?;
        for y in 0..h {
            for x in 0..w {
                a.set_pixel(x, y, (random(&mut seed) % (u32::from(F::MAX) + 1)) as u8)?;
            }
        }
        for y in 0..sh {
            for x in 0..sw {
                b.set_pixel(x, y, (random(&mut seed) % (u32::from(F::MAX) + 1)) as u8)?;
            }
        }
        let dx = i64::from(random(&mut seed) % 101) - 50;
        let dy = i64::from(random(&mut seed) % 31) - 15;
        let mode = modes[iteration % modes.len()];
        let original = a.clone();
        a.composite(&b, dx, dy, mode);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (i64::from(x) - dx, i64::from(y) - dy);
                let mut expected = original.get_pixel(x, y)?;
                if sx >= 0 && sy >= 0 && sx < i64::from(sw) && sy < i64::from(sh) {
                    expected =
                        operation(expected, b.get_pixel(sx as u32, sy as u32)?, mode, F::MAX);
                }
                assert_eq!(
                    a.get_pixel(x, y)?,
                    expected,
                    "blit iteration {iteration}, {w}x{h}, source {sw}x{sh}, ({dx},{dy}), {mode:?}, pixel ({x},{y})"
                );
            }
        }
        assert_eq!(a.flip_horizontal()?.flip_horizontal()?, a);
        assert_eq!(a.flip_vertical()?.flip_vertical()?, a);
        assert_eq!(a.rotate(90)?.rotate(-90)?, a);
        let crop = a.crop(sw, sh, dx, dy)?;
        for y in 0..sh {
            for x in 0..sw {
                let sx = i64::from(x) + dx;
                let sy = i64::from(h) - dy - i64::from(sh) + i64::from(y);
                let expected = if sx >= 0 && sy >= 0 && sx < i64::from(w) && sy < i64::from(h) {
                    a.get_pixel(sx as u32, sy as u32)?
                } else {
                    0
                };
                assert_eq!(crop.get_pixel(x, y)?, expected);
            }
        }
        let n = random(&mut seed) % 12;
        let bold = a.bold(n)?;
        for y in 0..h {
            for x in 0..w + n {
                let expected = (0..=n)
                    .rev()
                    .find_map(|k| {
                        x.checked_sub(k).filter(|&sx| sx < w).and_then(|sx| {
                            let p = a.get_pixel(sx, y).unwrap();
                            (p != 0).then_some(p)
                        })
                    })
                    .unwrap_or(0);
                assert_eq!(bold.get_pixel(x, y)?, expected);
            }
        }
        if F::PACKED && !w.is_multiple_of(8) {
            for row in a.as_bytes().chunks_exact(a.stride()) {
                assert_eq!(row.last().unwrap() & ((1 << (8 - w % 8)) - 1), 0);
            }
        }
    }
    Ok(())
}
#[test]
fn packed_operations_match_scalar_reference() -> Result<()> {
    differential::<Mono>()
}
#[test]
fn grayscale_operations_match_scalar_reference() -> Result<()> {
    differential::<Gray8>()
}

#[test]
fn all_wire_layouts_and_known_vectors() -> Result<()> {
    let mut seed = 99;
    for w in [0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65] {
        let mut b = Bitmap::<Mono>::new(w, 5)?;
        for y in 0..5 {
            for x in 0..w {
                b.set_pixel(x, y, (random(&mut seed) & 1) as u8)?;
            }
        }
        for bit_order in [BitOrder::MsbFirst, BitOrder::LsbFirst] {
            for byte_order in [ByteOrder::Big, ByteOrder::Little] {
                for row_order in [RowOrder::TopDown, RowOrder::BottomUp] {
                    for row_alignment in [1, 2, 4, 8] {
                        for scan_unit in [1, 2, 4, 8] {
                            let l = ByteLayout {
                                bit_order,
                                byte_order,
                                row_order,
                                row_alignment,
                                scan_unit,
                            };
                            let invalid = (bit_order == BitOrder::MsbFirst)
                                != (byte_order == ByteOrder::Big)
                                && scan_unit > row_alignment;
                            if invalid {
                                assert!(b.to_bytes(l).is_err());
                                continue;
                            }
                            let bytes = b.to_bytes(l)?;
                            assert_eq!(
                                Bitmap::<Mono>::from_bytes(
                                    w,
                                    5,
                                    &bytes,
                                    l,
                                    BitmapLimits::default()
                                )?,
                                b
                            );
                            if !bytes.is_empty() {
                                assert!(
                                    Bitmap::<Mono>::from_bytes(
                                        w,
                                        5,
                                        &bytes[..bytes.len() - 1],
                                        l,
                                        BitmapLimits::default()
                                    )
                                    .is_err()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    let b = mono("#....... .#......".replace(' ', "").as_str());
    let layout = ByteLayout {
        byte_order: ByteOrder::Little,
        scan_unit: 2,
        row_alignment: 2,
        ..ByteLayout::default()
    };
    assert_eq!(b.to_bytes(layout)?, vec![0x40, 0x80]);
    let layout = ByteLayout {
        bit_order: BitOrder::LsbFirst,
        ..layout
    };
    assert_eq!(b.to_bytes(layout)?, vec![0x01, 0x02]);
    let g = gray(&[&[1, 2, 3], &[4, 5, 6]]);
    let layout = ByteLayout {
        row_alignment: 4,
        row_order: RowOrder::BottomUp,
        ..ByteLayout::default()
    };
    assert_eq!(g.to_bytes(layout)?, vec![4, 5, 6, 0, 1, 2, 3, 0]);
    assert_eq!(
        Bitmap::<Gray8>::from_bytes(3, 2, &g.to_bytes(layout)?, layout, BitmapLimits::default())?,
        g
    );
    Ok(())
}
