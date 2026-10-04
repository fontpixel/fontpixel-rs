//! Independent GDOS fixtures and model round trips.
#![cfg(feature = "gem")]
use fontpixel::bitmap::{Bitmap, ByteOrder};
use fontpixel::formats::gem::{self, GemReadOptions, GemWriteOptions};
use fontpixel::model::{Codepoint, Font, FontSet, Glyph, Vector};
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
fn fixture(e: ByteOrder, swapped: bool) -> Vec<u8> {
    let mut b = vec![0; 108];
    b[4..8].copy_from_slice(b"Wire");
    for (at, v) in [
        (2, 3),
        (36, 65),
        (38, 67),
        (40, 1),
        (42, 1),
        (44, 1),
        (46, 1),
        (48, 1),
        (50, 5),
        (52, 5),
        (58, 1),
        (60, 1),
        (62, 0x5555),
        (64, 0x5555),
        (66, 2 | if swapped { 0 } else { 4 }),
        (80, 2),
        (82, 3),
    ] {
        p16(&mut b, at, v, e);
    }
    for (at, v) in [(68, 88), (72, 94), (76, 102)] {
        p32(&mut b, at, v, e);
    }
    b[88] = 1;
    b[89] = 255;
    for (i, v) in [0, 3, 8, 8].into_iter().enumerate() {
        p16(&mut b, 94 + i * 2, v, e);
    }
    b[102..].copy_from_slice(&[0xa7, 0, 0x5b, 0, 0xe1, 0]);
    if swapped {
        for pair in b[102..].as_chunks_mut::<2>().0 {
            pair.swap(0, 1);
        }
    }
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(gem::read(&mut c, &Default::default())?.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("test.gem").unwrap())
}
#[test]
fn independent_byte_orders_and_zero_width() {
    for e in [ByteOrder::Big, ByteOrder::Little] {
        for swapped in [false, true] {
            let s = read(&fixture(e, swapped), &Default::default()).unwrap();
            let f = s.single().unwrap();
            assert_eq!(f.len(), 3);
            let a = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
            assert_eq!(a.bitmap.as_bytes(), [0xa0, 0x40, 0xe0]);
            assert_eq!(a.metrics.bounds.x, -1);
            assert_eq!(a.metrics.advance, Some(Vector::new(3, 0)));
            assert_eq!(
                f.glyph_for_codepoint(Codepoint::Legacy(67))
                    .unwrap()
                    .bitmap
                    .width(),
                0
            );
            let w = gem::write(
                &s,
                &options(),
                &GemWriteOptions {
                    byte_order: e,
                    bitmap_word_order: if swapped {
                        ByteOrder::Little
                    } else {
                        ByteOrder::Big
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(!w.report.has_loss(), "{}", w.report);
            assert_eq!(
                read(&w.output.into_single().unwrap(), &Default::default()).unwrap(),
                s
            );
        }
    }
}
#[test]
fn all_truncations_and_mutation_smoke() {
    let b = fixture(ByteOrder::Big, false);
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
    for i in 0..b.len() {
        for value in [0, 255] {
            let mut bad = b.clone();
            bad[i] = value;
            let _ = read(
                &bad,
                &ResourceLimits {
                    max_total_bytes: 100000,
                    max_glyphs: 300,
                    ..Default::default()
                },
            );
        }
    }
}
#[test]
fn budgets_and_fatal_mapping_report() {
    let s = read(&fixture(ByteOrder::Big, false), &Default::default()).unwrap();
    for max in [0, 100, 5000, 65536] {
        let o = GemWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: max,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok(w) = gem::write(&s, &options(), &o) {
            assert!(read(&w.output.into_single().unwrap(), &o.limits).is_ok());
        }
    }
    let mut g = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::new(1, 1).unwrap());
    g.codepoints.push(Codepoint::Unicode(0x10000));
    let e = gem::write(
        &Font::from_glyphs([g]).unwrap().into(),
        &options(),
        &Default::default(),
    )
    .unwrap_err();
    assert!(
        e.report()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.code.ends_with("unrepresentable_mapping"))
    );
}
#[test]
fn metadata_aliases_slots_and_grayscale_are_diagnosed() {
    let mut s = read(&fixture(ByteOrder::Big, false), &Default::default()).unwrap();
    let f = &mut s.fonts[0];
    let id = f.glyph_id(Codepoint::Legacy(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.name = Some("A".into());
        g.codepoints.push(Codepoint::Legacy(70));
        g.unicode_sequences.push(vec![65, 66]);
        Ok(())
    })
    .unwrap();
    let removed = f.glyph_id(Codepoint::Legacy(66)).unwrap();
    f.remove_glyph(removed);
    let w = gem::write(&s, &options(), &Default::default()).unwrap();
    for key in ["glyph_name", "sequences", "aliases", "slots"] {
        assert!(
            w.report.entries.iter().any(|e| e.code.ends_with(key)),
            "{key}: {}",
            w.report
        );
    }
    let mut gray = Glyph::new(
        Bitmap::<fontpixel::bitmap::Gray8>::from_raw(1, 1, &[128], Default::default()).unwrap(),
    );
    gray.codepoints.push(Codepoint::Legacy(65));
    let s = FontSet::from(Font::from_glyphs([gray]).unwrap());
    assert!(gem::write(&s, &options(), &Default::default()).is_err());
    assert!(
        gem::write(
            &s,
            &options(),
            &GemWriteOptions {
                gray_threshold: Some(128),
                ..Default::default()
            }
        )
        .unwrap()
        .report
        .has_loss()
    );
}
#[test]
fn explicit_order_controls_detection() {
    let b = fixture(ByteOrder::Little, false);
    let input = Input::new(&b);
    let l = ResourceLimits::default();
    assert!(
        gem::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &GemReadOptions {
                byte_order: Some(ByteOrder::Big),
                ..Default::default()
            }
        )
        .is_err()
    );
}
proptest::proptest! {#[test]fn randomized_idempotent_fonts(widths in proptest::collection::vec(0u32..20,1..20),height in 1u32..10,seed in proptest::num::u32::ANY){let mut state=seed;let mut glyphs=Vec::new();for(i,w)in widths.iter().enumerate(){let n=(*w as usize).div_ceil(8)*height as usize;let bytes:Vec<_>=(0..n).map(|_|{state=state.wrapping_mul(1664525).wrapping_add(1013904223);(state>>24)as u8}).collect();let mut g=Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::from_raw(*w,height,&bytes,Default::default()).unwrap());g.codepoints.push(Codepoint::Legacy(32+i as u32));g.metrics.advance=Some(Vector::new(i64::from(*w)+1,0));glyphs.push(g);}let s=FontSet::from(Font::from_glyphs(glyphs).unwrap());let w=gem::write(&s,&options(),&Default::default()).unwrap();let s=read(&w.output.into_single().unwrap(),&Default::default()).unwrap();let w=gem::write(&s,&options(),&Default::default()).unwrap();proptest::prop_assert!(!w.report.has_loss(),"{}",w.report);proptest::prop_assert_eq!(read(&w.output.into_single().unwrap(),&Default::default()).unwrap(),s);}}

fn compressed_section(cp: u16, base: usize, next: usize, e: ByteOrder) -> Vec<u8> {
    let mut b = vec![0; 158];
    for (at, v) in [
        (2, 3),
        (36, cp),
        (38, cp),
        (40, 1),
        (42, 1),
        (46, 1),
        (48, 1),
        (62, 0x5555),
        (64, 0x5555),
        (66, 36),
        (80, 2),
        (82, 3),
    ] {
        p16(&mut b, at, v, e);
    }
    p32(&mut b, 72, (base + 152) as u32, e);
    p32(&mut b, 76, (base + 156) as u32, e);
    p32(&mut b, 88, next as u32, e);
    p16(&mut b, 154, 3, e);
    b[156..].copy_from_slice(if e == ByteOrder::Big {
        &[0x18, 0]
    } else {
        &[0, 0x18]
    });
    b
}
#[test]
fn independent_compressed_zero_run_and_linked_sections() {
    for e in [ByteOrder::Big, ByteOrder::Little] {
        let b = compressed_section(65, 0, 0, e);
        let f = read(&b, &Default::default()).unwrap();
        assert_eq!(
            f.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
            [0, 0, 0]
        );
        let mut b = compressed_section(65, 0, 158, e);
        b.extend(compressed_section(0x141, 158, 0, e));
        let f = read(&b, &Default::default()).unwrap();
        assert_eq!(f.fonts[0].len(), 2);
        assert!(
            f.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(0x141))
                .is_some()
        );
        let w = gem::write(&f, &options(), &Default::default()).unwrap();
        assert!(w.report.has_loss());
        let again = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
        assert!(
            again.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(0x141))
                .is_some()
        );
        p32(&mut b, 158 + 88, 158, e);
        assert!(read(&b, &Default::default()).is_err());
    }
}
#[test]
fn partial_omissions_survive_later_failure_and_strict_returns_no_bytes() {
    let mut s = read(&fixture(ByteOrder::Big, false), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.codepoints.push(Codepoint::Unicode(0x10000));
            g.metrics.bounds.x = 1000;
            Ok(())
        })
        .unwrap();
    let err = gem::write(&s, &options(), &Default::default()).unwrap_err();
    assert!(
        err.report()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.code.ends_with("unrepresentable_mapping"))
    );
    let s = read(&fixture(ByteOrder::Big, false), &Default::default()).unwrap();
    let mut altered = s;
    altered.fonts[0].names.family = Some("lost".into());
    let mut o = options();
    o.strict = true;
    assert!(matches!(
        gem::write(&altered, &o, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
