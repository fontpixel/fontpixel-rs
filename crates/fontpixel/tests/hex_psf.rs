//! Independent console wire fixtures, roundtrips, resource and loss boundaries.
#![cfg(all(feature = "hex", feature = "psf"))]
use fontpixel::bitmap::{BitmapLimits, Bounds, Gray8};
use fontpixel::formats::{
    Confidence,
    hex::{self, HexReadOptions, HexWriteOptions},
    psf::{self, PsfWriteOptions, UnicodeTable, Version},
};
use fontpixel::model::{Codepoint, Property, Vector};
use fontpixel::{
    Bitmap, Font, FontSet, Glyph, Input, ReadOptions, ResourceLimits, ResourcePath, WriteOptions,
};
use proptest::prelude::*;
fn limits() -> ResourceLimits {
    ResourceLimits::default()
}
fn psf2(w: u32, h: u32, rows: &[Vec<u8>], table: Option<&[u8]>) -> Vec<u8> {
    let mut b = b"\x72\xb5\x4a\x86".to_vec();
    for n in [
        0,
        32,
        u32::from(table.is_some()),
        rows.len() as u32,
        w.div_ceil(8) * h,
        h,
        w,
    ] {
        b.extend(n.to_le_bytes());
    }
    for r in rows {
        b.extend(r);
    }
    if let Some(t) = table {
        b.extend(t);
    }
    b
}
fn psf1(count: usize, h: u8, mode: u8, table: &[u8]) -> Vec<u8> {
    let mut b = vec![0x36, 4, mode, h];
    b.resize(4 + count * usize::from(h), 0);
    b.extend(table);
    b
}
fn hx(cp: u32, w: usize, h: usize) -> String {
    format!("{cp:04X}:{}\n", "A5".repeat(w / 8 * h))
}
fn read_hex(b: &[u8]) -> Font {
    hex::read(b, &HexReadOptions::default(), &limits())
        .unwrap()
        .0
}
fn read_psf(b: &[u8]) -> Font {
    psf::read(b, &limits()).unwrap().0
}
fn encoded_psf(f: &Font) -> Vec<u8> {
    psf::write(f, &PsfWriteOptions::default()).unwrap().0
}
#[test]
fn hex_all_widths_crlf_comments_duplicates_and_scalars() {
    let data = format!(
        "\u{feff}# hello\r\n\r\n{}{}{}{}{}",
        hx(65, 8, 16),
        hx(0x6c38, 16, 16),
        hx(0x1f600, 24, 16),
        hx(0x10ffff, 32, 16),
        hx(65, 8, 16)
    );
    assert_eq!(hex::detect(data.as_bytes()), Confidence::Possible);
    let (f, r) = hex::read(data.as_bytes(), &HexReadOptions::default(), &limits()).unwrap();
    assert_eq!(f.len(), 5);
    assert_eq!(r.entries.len(), 1);
    let (b, r) = hex::write(&f, &HexWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(read_hex(&b), f);
    assert_eq!(b, hex::write(&f, &HexWriteOptions::default()).unwrap().0);
    assert_eq!(
        f.glyphs()
            .map(|(_, g)| g.bitmap.width())
            .collect::<Vec<_>>(),
        [8, 16, 24, 32, 8]
    );
}
#[test]
fn hex_height_is_explicit_not_guessed() {
    let b = hx(65, 8, 24);
    assert!(hex::read(b.as_bytes(), &HexReadOptions::default(), &limits()).is_err());
    let o = HexReadOptions {
        height: 24,
        descent: 3,
    };
    let (f, _) = hex::read(b.as_bytes(), &o, &limits()).unwrap();
    assert_eq!(f.metrics.ascent, Some(21));
    let (out, r) = hex::write(
        &f,
        &HexWriteOptions {
            geometry: o.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!r.has_loss());
    assert_eq!(b.as_bytes(), out);
    assert_eq!(f, hex::read(&out, &o, &limits()).unwrap().0);
    // 8x32 and 16x16 have identical data lengths: only the option decides.
    let ambiguous = hx(65, 8, 32);
    assert_eq!(
        read_hex(ambiguous.as_bytes())
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .width(),
        16
    );
    assert_eq!(
        hex::read(
            ambiguous.as_bytes(),
            &HexReadOptions {
                height: 32,
                descent: 0
            },
            &limits()
        )
        .unwrap()
        .0
        .glyphs()
        .next()
        .unwrap()
        .1
        .bitmap
        .width(),
        8
    );
}
#[test]
fn hex_rejects_bad_records_and_invalid_geometry() {
    for b in [
        "GG:00",
        "D800:00000000000000000000000000000000",
        "110000:00000000000000000000000000000000",
        "0041:0",
        "0041:00:00",
        "0041:",
        "0041:ZZ000000000000000000000000000000",
        "0041:0000000000000000000000000000000G",
    ] {
        assert!(
            hex::read(b.as_bytes(), &HexReadOptions::default(), &limits()).is_err(),
            "{b}"
        );
    }
    assert!(hex::read(&[0xff], &HexReadOptions::default(), &limits()).is_err());
    for o in [
        HexReadOptions {
            height: 0,
            descent: 0,
        },
        HexReadOptions {
            height: 1,
            descent: 2,
        },
        HexReadOptions {
            height: u32::MAX,
            descent: 0,
        },
    ] {
        assert!(hex::read(b"", &o, &limits()).is_err());
    }
    assert_eq!(read_hex(b"").len(), 0);
    assert_eq!(hex::detect(b""), Confidence::NoMatch);
}
#[test]
fn psf2_all_aliases_multiple_sequences_sequence_only_and_noncharacters() {
    let b = psf2(
        9,
        2,
        &[
            vec![0x80, 0x80, 0, 0],
            vec![0, 0, 0, 0],
            vec![0xff, 0x80, 0, 0],
        ],
        Some("AÅ\u{212b}\u{fffe}\u{ffff}😀".as_bytes()),
    );
    let mut b = b;
    b.extend(b"\xfeA\xcc\x8a\xfef\xef\xb8\x8f\xff\xfe\xf0\x9f\x98\x80\xe2\x80\x8d\xf0\x9f\x98\x81\xff\xff");
    let f = read_psf(&b);
    let glyphs = f.glyphs().map(|(_, g)| g).collect::<Vec<_>>();
    assert_eq!(
        glyphs[0]
            .codepoints
            .iter()
            .map(|c| c.value())
            .collect::<Vec<_>>(),
        [65, 197, 0x212b, 0xfffe, 0xffff, 0x1f600]
    );
    assert_eq!(
        glyphs[0].unicode_sequences,
        [vec![65, 0x30a], vec![102, 0xfe0f]]
    );
    assert!(glyphs[1].codepoints.is_empty());
    assert_eq!(
        glyphs[1].unicode_sequences,
        [vec![0x1f600, 0x200d, 0x1f601]]
    );
    assert!(glyphs[2].codepoints.is_empty());
    let (out, r) = psf::write(&f, &PsfWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(out, b);
    assert_eq!(read_psf(&out), f);
    assert!(f.glyph_for_sequence(&[65, 0x30a]).is_some());
}
#[test]
fn psf1_modes_counts_and_true_little_endian_sequence_markers() {
    for count in [256, 512] {
        for mode in [0u8, 2, 4, 6] {
            let mode = mode | u8::from(count == 512);
            let mut table = vec![];
            if mode & 6 != 0 {
                for cp in [
                    0x00c5u16, 0x212b, 0xfffe, 0x0041, 0x030a, 0xfffe, 0x0041, 0x0342, 0xffff,
                ] {
                    table.extend(cp.to_le_bytes());
                }
                for _ in 1..count {
                    table.extend([0xff, 0xff]);
                }
            }
            let b = psf1(count, 3, mode, &table);
            let f = read_psf(&b);
            assert_eq!(f.len(), count);
            if mode & 6 != 0 {
                assert_eq!(
                    f.glyphs().next().unwrap().1.unicode_sequences,
                    [vec![65, 0x30a], vec![65, 0x342]]
                );
            } else {
                assert_eq!(
                    f.glyphs().nth(1).unwrap().1.codepoints,
                    [Codepoint::Legacy(1)]
                );
            }
            let o = PsfWriteOptions {
                version: Version::One,
                ..Default::default()
            };
            let (out, r) = psf::write(&f, &o).unwrap();
            assert!(!r.has_loss(), "{r}");
            assert_eq!(read_psf(&out), f);
            assert_eq!(out, psf::write(&f, &o).unwrap().0);
        }
    }
}
#[test]
fn psf_without_table_never_invents_unicode_and_unencoded_auto_has_table() {
    let b = psf2(1, 1, &[vec![0x80], vec![0]], None);
    let f = read_psf(&b);
    assert_eq!(f.charset.name.as_deref(), Some("unknown"));
    assert!(!f.charset.codec().unwrap().known());
    assert_eq!(
        f.glyphs().next().unwrap().1.codepoints,
        [Codepoint::Legacy(0)]
    );
    assert!(f.glyph_for_codepoint(Codepoint::Unicode(0)).is_none());
    assert_eq!(encoded_psf(&f), b);
    let b = psf2(1, 1, &[vec![0x80]], Some(b"\xff"));
    let f = read_psf(&b);
    assert_eq!(encoded_psf(&f), b);
    assert!(f.glyphs().next().unwrap().1.codepoints.is_empty());
}
#[test]
fn psf2_arbitrary_width_padding_and_extended_header() {
    let b = psf2(17, 2, &[vec![0xaa, 0x55, 0xff, 0, 0, 0xff]], None);
    let (f, r) = psf::read(&b, &limits()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(r.entries[0].code, "psf.read.padding");
    assert_eq!(
        f.glyphs().next().unwrap().1.bitmap.as_bytes(),
        [0xaa, 0x55, 0x80, 0, 0, 0x80]
    );
    let mut extended = psf2(8, 1, &[vec![0]], None);
    extended[8..12].copy_from_slice(&36u32.to_le_bytes());
    extended.splice(32..32, [1, 2, 3, 4]);
    let (_, r) = psf::read(&extended, &limits()).unwrap();
    assert!(r.has_loss());
}
#[test]
fn psf_rejects_all_truncations_except_complete_headerless_tables() {
    let b = psf2(
        9,
        3,
        &[vec![0; 6], vec![0; 6]],
        Some(b"A\xff\xfea\xcc\x81\xff"),
    );
    for n in 0..b.len() {
        assert!(psf::read(&b[..n], &limits()).is_err(), "length {n}");
    }
    let b = psf1(256, 2, 2, &vec![0xff; 512]);
    for n in 0..b.len() {
        assert!(psf::read(&b[..n], &limits()).is_err(), "length {n}");
    }
}
#[test]
fn psf_rejects_bad_headers_utf8_sequences_and_trailing_bytes() {
    for (at, value) in [
        (4, 1),
        (8, 31),
        (8, u32::MAX),
        (12, 2),
        (16, 0),
        (16, u32::MAX),
        (20, 2),
        (24, 0),
        (28, 0),
        (24, u32::MAX),
        (28, u32::MAX),
    ] {
        let mut b = psf2(8, 1, &[vec![0]], None);
        b[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(psf::read(&b, &limits()).is_err(), "{at}={value}");
    }
    for t in [
        &b"\xfe\xff"[..],
        b"\xfeA\xfe\xff",
        b"\xc0\x80\xff",
        b"\xed\xa0\x80\xff",
        b"\xf4\x90\x80\x80\xff",
        b"\x80\xff",
        b"\xc2\xff",
        b"A",
        b"\xffx",
    ] {
        assert!(
            psf::read(&psf2(8, 1, &[vec![0]], Some(t)), &limits()).is_err(),
            "{t:?}"
        );
    }
    let mut b = psf2(8, 1, &[vec![0]], None);
    b.push(0);
    assert!(psf::read(&b, &limits()).is_err());
    for mode in [8, 0xff] {
        assert!(psf::read(&psf1(256, 1, mode, &[]), &limits()).is_err());
    }
    let mut t = vec![0, 0xd8, 0xff, 0xff];
    t.resize(514, 0xff);
    assert!(psf::read(&psf1(256, 1, 2, &t), &limits()).is_err());
}
#[test]
fn read_resource_budgets_cover_wire_model_tables_pixels_and_mappings() {
    let b = psf2(8, 1, &[vec![0]], Some(b"A\xff"));
    for l in [
        ResourceLimits {
            max_input_bytes: 1,
            ..limits()
        },
        ResourceLimits {
            max_glyphs: 0,
            ..limits()
        },
        ResourceLimits {
            max_table_bytes: 1,
            ..limits()
        },
        ResourceLimits {
            max_pixels_per_glyph: 7,
            ..limits()
        },
        ResourceLimits {
            max_total_bytes: b.len() + 1024,
            ..limits()
        },
    ] {
        assert!(psf::read(&b, &l).is_err());
    }
    let mapped = psf2(8, 1, &[vec![0]], Some(&[b'A'; 10000]));
    assert!(matches!(
        psf::read(
            &mapped,
            &ResourceLimits {
                max_total_bytes: 12000,
                ..limits()
            }
        ),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    let b = hx(65, 8, 16);
    for l in [
        ResourceLimits {
            max_input_bytes: 1,
            ..limits()
        },
        ResourceLimits {
            max_glyphs: 0,
            ..limits()
        },
        ResourceLimits {
            max_table_bytes: 8,
            ..limits()
        },
        ResourceLimits {
            max_pixels_per_glyph: 127,
            ..limits()
        },
        ResourceLimits {
            max_total_bytes: b.len() + 1024,
            ..limits()
        },
    ] {
        assert!(hex::read(b.as_bytes(), &HexReadOptions::default(), &l).is_err());
    }
    assert!(
        hex::read(
            b"#".repeat(1000).as_slice(),
            &HexReadOptions::default(),
            &ResourceLimits {
                max_table_bytes: 999,
                ..limits()
            }
        )
        .is_err()
    );
}
#[test]
fn writer_limits_empty_psf_multiple_fonts_and_unrepresentable_geometry() {
    let f = read_psf(&psf2(9, 1, &[vec![0, 0]], Some(b"A\xff")));
    assert!(
        psf::write(
            &f,
            &PsfWriteOptions {
                version: Version::One,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        psf::write(
            &f,
            &PsfWriteOptions {
                psf1_count: Some(256),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(psf::write(&Font::default(), &PsfWriteOptions::default()).is_err());
    for o in [
        PsfWriteOptions {
            max_output_bytes: 0,
            ..Default::default()
        },
        PsfWriteOptions {
            limits: ResourceLimits {
                max_table_bytes: 1,
                ..limits()
            },
            ..Default::default()
        },
        PsfWriteOptions {
            limits: ResourceLimits {
                max_glyphs: 0,
                ..limits()
            },
            ..Default::default()
        },
    ] {
        assert!(psf::write(&f, &o).is_err());
    }
    let f = read_hex(hx(65, 8, 16).as_bytes());
    assert!(
        hex::write(
            &f,
            &HexWriteOptions {
                max_output_bytes: 10,
                ..Default::default()
            }
        )
        .is_err()
    );
    let set = FontSet {
        fonts: vec![f.clone(), f],
        ..Default::default()
    };
    for id in ["hex", "psf"] {
        assert!(
            fontpixel::write(
                &set,
                id,
                &WriteOptions::new(ResourcePath::new("out").unwrap())
            )
            .is_err()
        );
    }
}
#[test]
fn metadata_mapping_gray_and_psf1_losses_are_explicit() {
    let mut f = read_hex(hx(65, 8, 16).as_bytes());
    f.names.family = Some("family".into());
    f.properties.push(Property::new("CUSTOM", "value"));
    let id = f.glyphs().next().unwrap().0;
    let mut g = f.glyph(id).unwrap().clone();
    g.codepoints.extend([
        Codepoint::Unicode(0x1f600),
        Codepoint::Unicode(0xffff),
        Codepoint::Legacy(9),
    ]);
    g.unicode_sequences = vec![vec![65, 0x30a], vec![0x1f600, 0x200d]];
    g.name = Some("A".into());
    g.metrics.vertical_advance = Some(Vector::new(0, -16));
    f.replace_glyph(id, g).unwrap();
    let (_, r) = hex::write(&f, &HexWriteOptions::default()).unwrap();
    for code in [
        "hex.write.names",
        "hex.write.property",
        "hex.write.aliases",
        "hex.write.codepoint",
        "hex.write.sequences",
        "hex.write.glyph_name",
        "hex.write.glyph_metric",
    ] {
        assert!(r.entries.iter().any(|e| e.code == code), "{code}");
    }
    let (b, r) = psf::write(
        &f,
        &PsfWriteOptions {
            version: Version::One,
            ..Default::default()
        },
    )
    .unwrap();
    for code in [
        "psf.write.codepoint",
        "psf.write.sequence",
        "psf.write.padding_glyphs",
    ] {
        assert!(r.entries.iter().any(|e| e.code == code), "{code}");
    }
    assert_eq!(read_psf(&b).len(), 256);
    let (_, r) = psf::write(
        &f,
        &PsfWriteOptions {
            unicode_table: UnicodeTable::Omit,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.entries.iter().any(|e| e.code == "psf.write.mapping"));
    let gray = Bitmap::<Gray8>::from_raw(8, 16, &[127; 128], BitmapLimits::default()).unwrap();
    let mut g = Glyph::new(gray);
    g.codepoints.push(Codepoint::Unicode(65));
    let f = Font::from_glyphs([g]).unwrap();
    assert!(hex::write(&f, &HexWriteOptions::default()).is_err());
    assert!(psf::write(&f, &PsfWriteOptions::default()).is_err());
    let (b, r) = hex::write(
        &f,
        &HexWriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.has_loss());
    assert!(
        read_hex(&b)
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes()
            .iter()
            .all(|b| *b == 0)
    );
}
#[test]
fn cell_alignment_keeps_negative_bearings_and_descent_pixels() {
    let mut g = Glyph::new("#.#\n.#.".parse::<Bitmap>().unwrap());
    g.metrics.bounds = Bounds::new(3, 2, -1, -1);
    g.metrics.advance = Some(Vector::new(3, 0));
    g.codepoints.push(Codepoint::Unicode(65));
    let f = Font::from_glyphs([g]).unwrap();
    let (b, r) = psf::write(&f, &PsfWriteOptions::default()).unwrap();
    assert!(r.has_loss());
    let f = read_psf(&b);
    let g = f.glyphs().next().unwrap().1;
    assert_eq!(g.bitmap.width(), 4);
    assert_eq!(g.bitmap.as_bytes(), [0xa0, 0x40]);
    let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
    g.metrics.bounds = Bounds::new(1, 1, 0, -1);
    let f = Font::from_glyphs([g]).unwrap();
    assert!(hex::write(&f, &HexWriteOptions::default()).is_err());
}
#[test]
fn registry_detection_gzip_strict_and_repeatable_output() {
    let b = psf2(8, 1, &[vec![0x80]], Some(b"A\xff"));
    let registry = fontpixel::registry();
    for id in ["hex", "psf"] {
        let status = registry.status(id).unwrap();
        assert!(status.read && status.write && status.detect);
    }
    let loaded = registry
        .read(
            &Input::new(&b).with_name("wrong.hex"),
            &ReadOptions::default(),
        )
        .unwrap();
    assert_eq!(loaded.format, "psf");
    let o = WriteOptions::new(ResourcePath::new("font.psf.gz").unwrap());
    let a = registry
        .write(&loaded.fonts, "psf", &o)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let b = registry
        .write(&loaded.fonts, "psf", &o)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(
        fontpixel::read(&a, &ReadOptions::default()).unwrap().fonts,
        loaded.fonts
    );
    let mut f = loaded.fonts;
    f.fonts[0].names.family = Some("lost".into());
    let mut o = o;
    o.strict = true;
    assert!(matches!(
        registry.write(&f, "psf", &o),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
    #[test]
    fn psf2_native_roundtrip(width in 1u32..65,height in 1u32..33,seed in prop::collection::vec(any::<u8>(),1..200),codes in prop::collection::vec(0u32..=0x10ffff,1..12)){
        let codes=codes.into_iter().filter(|&cp|char::from_u32(cp).is_some()).collect::<Vec<_>>();prop_assume!(!codes.is_empty());let size=width.div_ceil(8) as usize*height as usize;
        let rows=codes.iter().enumerate().map(|(i,_)|(0..size).map(|j|seed[(i+j)%seed.len()]).collect()).collect::<Vec<Vec<u8>>>();let mut table=vec![];
        for (i,&cp) in codes.iter().enumerate(){let ch=char::from_u32(cp).unwrap();let mut buf=[0;4];table.extend(ch.encode_utf8(&mut buf).as_bytes());if i%2==0 {table.push(0xfe);table.extend(b"A\xcc\x81");}table.push(0xff);}
        let f=read_psf(&psf2(width,height,&rows,Some(&table)));let (b,r)=psf::write(&f,&PsfWriteOptions::default()).unwrap();prop_assert!(!r.has_loss());prop_assert_eq!(read_psf(&b),f);prop_assert_eq!(psf::write(&read_psf(&b),&PsfWriteOptions::default()).unwrap().0,b);
    }
    #[test]
    fn hex_native_roundtrip(width in 1usize..=4,height in 1u32..65,descent in 0u32..65,cp in 0u32..=0x10ffff,bytes in prop::collection::vec(any::<u8>(),1..65)){
        prop_assume!(char::from_u32(cp).is_some());let descent=descent.min(height);let data=(0..width*height as usize).map(|i|format!("{:02X}",bytes[i%bytes.len()])).collect::<String>();let wire=format!("{cp:04X}:{data}\n");let o=HexReadOptions{height,descent};let f=hex::read(wire.as_bytes(),&o,&limits()).unwrap().0;let (b,r)=hex::write(&f,&HexWriteOptions{geometry:o.clone(),..Default::default()}).unwrap();prop_assert!(!r.has_loss());prop_assert_eq!(&b,wire.as_bytes());prop_assert_eq!(hex::read(&b,&o,&limits()).unwrap().0,f);
    }
}

#[test]
fn output_totals_alias_expansion_and_edited_comment_loss() {
    let mut font = read_hex(hx(65, 8, 16).as_bytes());
    let id = font.glyphs().next().unwrap().0;
    font.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(66));
        Ok(())
    })
    .unwrap();
    assert!(
        hex::write(
            &font,
            &HexWriteOptions {
                limits: ResourceLimits {
                    max_glyphs: 1,
                    ..limits()
                },
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        hex::write(
            &font,
            &HexWriteOptions {
                limits: ResourceLimits {
                    max_total_bytes: 32,
                    ..limits()
                },
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        psf::write(
            &font,
            &PsfWriteOptions {
                limits: ResourceLimits {
                    max_total_bytes: 32,
                    ..limits()
                },
                ..Default::default()
            }
        )
        .is_err()
    );
    font.headers
        .push(Property::new("hex.comment", "edited\ncomment "));
    let (_, report) = hex::write(&font, &HexWriteOptions::default()).unwrap();
    assert!(report.entries.iter().any(|e| e.code == "hex.write.comment"));
}

#[test]
fn gzip_wire_and_decompressed_limits_are_independent() {
    let font = read_psf(&psf2(8, 16, &vec![vec![0; 16]; 64], None));
    let set = FontSet::from(font);
    let opts = WriteOptions::new(ResourcePath::new("test.psf.gz").unwrap());
    let compressed = fontpixel::write(&set, "psf", &opts)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let plain = encoded_psf(&set.fonts[0]);
    assert!(compressed.len() < plain.len());
    let options = ReadOptions {
        limits: ResourceLimits {
            max_input_bytes: compressed.len(),
            max_decompressed_bytes: plain.len(),
            ..limits()
        },
        ..Default::default()
    };
    assert_eq!(
        fontpixel::read(&compressed, &options).unwrap().fonts.fonts,
        set.fonts
    );
    assert!(psf::read(&plain, &options.limits).is_err());
    let options = ReadOptions {
        limits: ResourceLimits {
            max_decompressed_bytes: plain.len() - 1,
            ..options.limits
        },
        ..Default::default()
    };
    assert!(fontpixel::read(&compressed, &options).is_err());
}

#[test]
fn embedded_carriage_return_in_hex_comment_is_lossless() {
    let (font, _) = hex::read(b"# before\rafter\n", &HexReadOptions::default(), &limits()).unwrap();
    let (bytes, report) = hex::write(&font, &HexWriteOptions::default()).unwrap();
    assert!(!report.has_loss());
    assert_eq!(
        hex::read(&bytes, &HexReadOptions::default(), &limits())
            .unwrap()
            .0,
        font
    );
}

#[test]
fn review_hex_comment_records_obey_writer_table_limit() {
    let mut font = Font::default();
    font.headers
        .push(Property::new("hex.comment", "x".repeat(256)));
    let options = HexWriteOptions {
        limits: ResourceLimits {
            max_table_bytes: 64,
            ..limits()
        },
        ..Default::default()
    };
    assert!(matches!(
        hex::write(&font, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn review_gzip_model_budget_includes_retained_compressed_input() {
    let plain = psf2(8, 16, &[vec![0; 16]], Some(b"A\xff"));
    let font = read_psf(&plain);
    let compressed = fontpixel::write(
        &FontSet::from(font),
        "psf",
        &WriteOptions::new(ResourcePath::new("font.psf.gz").unwrap()),
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    // Exact logical charge: decoded bytes + glyph bookkeeping + raster + alias.
    let charged = plain.len() + 1024 + 16 + 128;
    assert!(
        psf::read(
            &plain,
            &ResourceLimits {
                max_total_bytes: charged,
                ..limits()
            }
        )
        .is_ok()
    );
    let options = ReadOptions {
        limits: ResourceLimits {
            max_total_bytes: charged,
            ..limits()
        },
        ..Default::default()
    };
    assert!(matches!(
        fontpixel::read(&compressed, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    let options = ReadOptions {
        limits: ResourceLimits {
            max_total_bytes: charged + compressed.len(),
            ..limits()
        },
        ..Default::default()
    };
    assert!(fontpixel::read(&compressed, &options).is_ok());
}

#[test]
fn review_hex_gzip_budget_matches_exact_raw_and_transport_boundaries() {
    let plain = hx(65, 8, 16).into_bytes();
    let compressed = fontpixel::write(
        &FontSet::from(read_hex(&plain)),
        "hex",
        &WriteOptions::new(ResourcePath::new("font.hex.gz").unwrap()),
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    let charged = plain.len() + 1152 + 16;
    let limits = ResourceLimits {
        max_total_bytes: charged,
        ..limits()
    };
    assert!(hex::read(&plain, &HexReadOptions::default(), &limits).is_ok());
    assert!(matches!(
        fontpixel::read(
            &compressed,
            &ReadOptions {
                limits: limits.clone(),
                ..Default::default()
            }
        ),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    assert!(
        fontpixel::read(
            &compressed,
            &ReadOptions {
                limits: ResourceLimits {
                    max_total_bytes: charged + compressed.len(),
                    ..limits
                },
                ..Default::default()
            }
        )
        .is_ok()
    );
}

#[test]
fn review_hex_comment_record_limit_is_inclusive_and_counts_hash() {
    let mut font = Font::default();
    font.headers
        .push(Property::new("hex.comment", "x".repeat(63)));
    let options = HexWriteOptions {
        limits: ResourceLimits {
            max_table_bytes: 64,
            ..limits()
        },
        ..Default::default()
    };
    let bytes = hex::write(&font, &options).unwrap().0;
    assert_eq!(bytes.len(), 65); // The 64-byte record plus its LF delimiter.
    assert!(hex::read(&bytes, &options.geometry, &options.limits).is_ok());
    let options = HexWriteOptions {
        limits: ResourceLimits {
            max_table_bytes: 63,
            ..limits()
        },
        ..Default::default()
    };
    assert!(matches!(
        hex::write(&font, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}
