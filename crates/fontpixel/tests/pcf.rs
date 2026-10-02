//! PCF wire safety, real-model edits, reports, feature gating and transport.
#![cfg(feature = "pcf")]
use fontpixel::bitmap::Bounds;
use fontpixel::formats::pcf::{self, GlyphPad, Order, PcfWriteOptions, ScanUnit};
use fontpixel::model::{Codepoint, Property, PropertyValue, Rational, Vector};
use fontpixel::{
    Bitmap, Error, Font, FontSet, Glyph, Input, ReadOptions, ResourceLimits, ResourcePath,
    WriteOptions,
};
fn font() -> Font {
    let mut f = Font::default();
    f.metrics.point_size = Some(Rational::integer(8));
    f.metrics.resolution = Some(Vector::new(75, 75));
    f.names.xlfd = Some("test".into());
    f.charset.registry = Some("ISO10646".into());
    f.charset.encoding = Some("1".into());
    for (cp, text, width) in [(65, "#.#\n.#.", 3), (66, "###\n..#", 5)] {
        let mut g = Glyph::new(text.parse::<Bitmap>().unwrap());
        g.name = Some(format!("g{cp}"));
        g.codepoints = vec![Codepoint::Unicode(cp)];
        g.metrics.advance = Some(Vector::new(width, 0));
        g.metrics.scalable_advance = Some(Vector::new(width * 100, 0));
        f.add_glyph(g).unwrap();
    }
    f.metrics.bounds = Some(Bounds::new(3, 2, 0, 0));
    f.metrics.ascent = Some(2);
    f.metrics.descent = Some(0);
    f
}
fn encoded(f: &Font) -> Vec<u8> {
    pcf::write(f, &PcfWriteOptions::default()).unwrap().0
}
fn toc(b: &[u8]) -> Vec<(u32, u32, usize, usize)> {
    let u = |p| u32::from_le_bytes(b[p..p + 4].try_into().unwrap());
    (0..u(4) as usize)
        .map(|i| {
            let p = 8 + 16 * i;
            (u(p), u(p + 4), u(p + 8) as usize, u(p + 12) as usize)
        })
        .collect()
}
fn off(b: &[u8], kind: u32) -> usize {
    toc(b).into_iter().find(|t| t.0 == kind).unwrap().3
}
fn strip(b: &[u8], drop: &[u32]) -> Vec<u8> {
    let records: Vec<_> = toc(b)
        .into_iter()
        .filter(|t| !drop.contains(&t.0))
        .collect();
    let mut out = b.to_vec();
    out[4..8].copy_from_slice(&(records.len() as u32).to_le_bytes());
    for (i, (k, f, s, o)) in records.into_iter().enumerate() {
        for (j, v) in [k, f, s as u32, o as u32].into_iter().enumerate() {
            out[8 + 16 * i + j * 4..12 + 16 * i + j * 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    out
}
#[test]
fn registry_gzip_and_layouts_are_real() {
    let registry = fontpixel::registry();
    let status = registry.status("pcf").unwrap();
    assert!(status.read && status.write && status.detect);
    let b = encoded(&font());
    let loaded = registry
        .read(
            &Input::new(&b).with_name("wrong.bdf"),
            &ReadOptions::default(),
        )
        .unwrap();
    assert_eq!(loaded.format, "pcf");
    let o = WriteOptions::new(ResourcePath::new("test.pcf.gz").unwrap());
    let compressed = registry
        .write(&loaded.fonts, "pcf", &o)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(compressed.starts_with(&[31, 139]));
    assert_eq!(
        fontpixel::read(&compressed, &ReadOptions::default())
            .unwrap()
            .fonts
            .fonts[0]
            .len(),
        2
    );
    assert_eq!(
        compressed,
        registry
            .write(&loaded.fonts, "pcf", &o)
            .unwrap()
            .output
            .into_single()
            .unwrap()
    );
    for byte_order in [Order::Msb, Order::Lsb] {
        for bit_order in [Order::Msb, Order::Lsb] {
            for glyph_pad in [
                GlyphPad::One,
                GlyphPad::Two,
                GlyphPad::Four,
                GlyphPad::Eight,
            ] {
                for scan_unit in [ScanUnit::One, ScanUnit::Two, ScanUnit::Four] {
                    let o = PcfWriteOptions {
                        byte_order,
                        bit_order,
                        glyph_pad,
                        scan_unit,
                        ..PcfWriteOptions::default()
                    };
                    let invalid = byte_order != bit_order
                        && matches!(
                            (glyph_pad, scan_unit),
                            (GlyphPad::One, ScanUnit::Two | ScanUnit::Four)
                                | (GlyphPad::Two, ScanUnit::Four)
                        );
                    let b = pcf::write(&font(), &o);
                    if invalid {
                        assert!(b.is_err());
                    } else {
                        let (f, _) = pcf::read(&b.unwrap().0, &ResourceLimits::default()).unwrap();
                        assert_eq!(
                            f.glyphs().next().unwrap().1.bitmap,
                            font().glyphs().next().unwrap().1.bitmap
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn core_aliases_and_unencoded_records_survive() {
    let mut f = font();
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(300));
        Ok(())
    })
    .unwrap();
    let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
    g.name = Some("orphan".into());
    f.add_glyph(g).unwrap();
    let b = encoded(&f);
    let (back, _) = pcf::read(&b, &ResourceLimits::default()).unwrap();
    assert_eq!(back.len(), 3);
    assert_eq!(back.glyphs().next().unwrap().1.codepoints.len(), 2);
    assert_eq!(
        back.glyph_for_codepoint(Codepoint::Unicode(300)),
        back.glyph_for_codepoint(Codepoint::Unicode(65))
    );
    assert_eq!(encoded(&back), b);
}
#[test]
fn optional_tables_have_spec_fallbacks() {
    let b = encoded(&font());
    for absent in [
        vec![64],
        vec![128],
        vec![2, 256],
        vec![2],
        vec![1, 16],
        vec![32, 128],
        vec![1, 2, 16, 32, 64, 128, 256],
    ] {
        let (f, _) = pcf::read(&strip(&b, &absent), &ResourceLimits::default()).unwrap();
        assert_eq!(f.len(), 2);
        if absent.contains(&128) {
            assert_eq!(
                f.glyphs().next().unwrap().1.name.as_deref(),
                Some(if absent.contains(&32) {
                    "glyph0"
                } else {
                    "uni0041"
                })
            );
        }
        if absent.contains(&32) {
            assert_eq!(f.codepoints().count(), 0);
        }
        if absent.contains(&64) {
            assert_eq!(
                f.glyphs().next().unwrap().1.metrics.scalable_advance,
                Some(Vector::new(if absent.contains(&1) { 1440 } else { 360 }, 0))
            );
        }
    }
}
#[test]
fn truncation_counts_offsets_formats_and_negative_sizes_fail() {
    let good = encoded(&font());
    for len in 0..good.len() {
        assert!(
            pcf::read(&good[..len], &ResourceLimits::default()).is_err(),
            "truncation {len}"
        );
    }
    for (offset, value) in [
        (4, u32::MAX),
        (off(&good, 4) + 4, 0xffffffff),
        (off(&good, 8) + 4, 99),
        (off(&good, 8) + 8, u32::MAX),
        (off(&good, 1) + 4, u32::MAX),
        (off(&good, 1) + 8, u32::MAX),
        (off(&good, 64) + 4, 99),
        (off(&good, 128) + 4, 99),
    ] {
        let mut b = good.clone();
        b[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert!(
            pcf::read(&b, &ResourceLimits::default()).is_err(),
            "bad field {offset}"
        );
    }
    for t in [4, 8] {
        assert!(pcf::read(&strip(&good, &[t]), &ResourceLimits::default()).is_err());
    }
    let mut b = good.clone();
    let o = off(&b, 8);
    b[o] |= 0x30;
    assert!(
        pcf::read(&b, &ResourceLimits::default())
            .unwrap_err()
            .to_string()
            .contains("scan unit")
    );
    let mut b = good.clone();
    let o = off(&b, 32);
    b[o + 14..o + 16].copy_from_slice(&500u16.to_be_bytes());
    assert!(pcf::read(&b, &ResourceLimits::default()).is_err());
    let mut b = good.clone();
    let o = off(&b, 4);
    b[o + 6] = 0x90;
    b[o + 7] = 0x80;
    assert!(
        pcf::read(&b, &ResourceLimits::default())
            .unwrap_err()
            .to_string()
            .contains("negative")
    );
}
#[test]
fn all_resource_budgets_apply() {
    let b = encoded(&font());
    for limits in [
        ResourceLimits {
            max_input_bytes: b.len() - 1,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_total_bytes: 10,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_glyphs: 1,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 5,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_table_bytes: 20,
            ..ResourceLimits::default()
        },
    ] {
        assert!(matches!(
            pcf::read(&b, &limits),
            Err(Error::ResourceLimit { .. })
        ));
    }
    let options = PcfWriteOptions {
        max_output_bytes: b.len() - 1,
        ..PcfWriteOptions::default()
    };
    assert!(pcf::write(&font(), &options).is_err());
    let options = PcfWriteOptions {
        max_output_bytes: b.len(),
        ..PcfWriteOptions::default()
    };
    assert_eq!(pcf::write(&font(), &options).unwrap().0, b);
    let options = PcfWriteOptions {
        limits: ResourceLimits {
            max_table_bytes: 20,
            ..ResourceLimits::default()
        },
        ..PcfWriteOptions::default()
    };
    assert!(pcf::write(&font(), &options).is_err());
}
#[test]
fn semantic_and_property_edits_win_over_provenance() {
    let (mut f, _) = pcf::read(&encoded(&font()), &ResourceLimits::default()).unwrap();
    f.names.family = Some("New family".into());
    f.names.xlfd = Some("edited-xlfd".into());
    f.metrics.point_size = Some(Rational::integer(11));
    f.metrics.resolution = Some(Vector::new(100, 101));
    f.properties.set("CUSTOM", 42i64);
    f.metrics.ascent = Some(9);
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.name = Some("renamed".into());
        g.bitmap = Glyph::new("...\n###".parse::<Bitmap>().unwrap()).bitmap;
        g.metrics.advance = Some(Vector::new(7, 0));
        Ok(())
    })
    .unwrap();
    let (f, _) = pcf::read(&encoded(&f), &ResourceLimits::default()).unwrap();
    assert_eq!(f.names.family.as_deref(), Some("New family"));
    assert_eq!(f.names.xlfd.as_deref(), Some("edited-xlfd"));
    assert_eq!(f.metrics.point_size, Some(Rational::integer(11)));
    assert_eq!(f.metrics.resolution, Some(Vector::new(100, 101)));
    assert_eq!(f.metrics.ascent, Some(9));
    assert_eq!(
        f.properties.get("CUSTOM"),
        Some(&PropertyValue::Integer(42))
    );
    assert_eq!(
        f.glyphs().next().unwrap().1.name.as_deref(),
        Some("renamed")
    );
    assert_eq!(
        f.glyphs().next().unwrap().1.metrics.advance,
        Some(Vector::new(7, 0))
    );
    let mut f = f;
    f.properties.set("FAMILY_NAME", "Property edit");
    assert_eq!(
        pcf::read(&encoded(&f), &ResourceLimits::default())
            .unwrap()
            .0
            .names
            .family
            .as_deref(),
        Some("Property edit")
    );
}
#[test]
fn metrics_attributes_and_ink_provenance_are_preserved_or_reported() {
    let mut f = font();
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.properties.push(Property::new("pcf.attributes", 65535i64));
        Ok(())
    })
    .unwrap();
    let b = encoded(&f);
    let (mut f, _) = pcf::read(&b, &ResourceLimits::default()).unwrap();
    assert_eq!(
        f.glyphs()
            .next()
            .unwrap()
            .1
            .properties
            .get("pcf.attributes"),
        Some(&PropertyValue::Integer(65535))
    );
    assert_eq!(encoded(&f), b);
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.properties.push(Property::new(
            "pcf.ink_metrics",
            PropertyValue::List(vec![0i64.into(); 6]),
        ));
        Ok(())
    })
    .unwrap();
    assert!(
        pcf::write(&f, &PcfWriteOptions::default())
            .unwrap()
            .1
            .entries
            .iter()
            .any(|e| e.code == "pcf.write.ink_metrics")
    );
}
#[test]
fn strict_loss_combines_read_and_write_and_refuses_collections() {
    let mut f = font();
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(0x10000));
        g.unicode_sequences.push(vec![65, 66]);
        g.metrics.vertical_advance = Some(Vector::new(0, -8));
        Ok(())
    })
    .unwrap();
    let mut options = WriteOptions::new(ResourcePath::new("out.pcf").unwrap());
    options.strict = true;
    assert!(matches!(
        fontpixel::write(&FontSet::from(f), "pcf", &options),
        Err(Error::StrictLoss(_))
    ));
    let set = FontSet {
        fonts: vec![font(), font()],
        ..FontSet::default()
    };
    assert!(fontpixel::write(&set, "pcf", &options).is_err());
}
#[test]
fn malformed_bdf_hex_uses_lenient_nibbles_then_edited_pixels() {
    #[cfg(feature = "bdf")]
    {
        let source=b"STARTFONT 2.1\nFONT odd\nSIZE 8 75 75\nFONTBOUNDINGBOX 12 2 0 0\nSTARTPROPERTIES 2\nFONT_ASCENT 2\nFONT_DESCENT 0\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 0 0\nBBX 12 2 0 0\nBITMAP\nZZ12\n1\nENDCHAR\nENDFONT\n";
        let (mut f, _) = fontpixel::formats::bdf::read(source, &ResourceLimits::default()).unwrap();
        let (b, r) = pcf::write(&f, &PcfWriteOptions::default()).unwrap();
        assert!(r.entries.iter().any(|e| e.code == "pcf.write.lenient_hex"));
        let (back, _) = pcf::read(&b, &ResourceLimits::default()).unwrap();
        assert_eq!(
            back.glyphs().next().unwrap().1.bitmap.as_bytes(),
            &[0, 0x10, 0x10, 0]
        );
        let id = f.glyphs().next().unwrap().0;
        f.edit_glyph(id, |g| {
            g.bitmap = Bitmap::<fontpixel::bitmap::Mono>::new(12, 2)?.into();
            Ok(())
        })
        .unwrap();
        let (b, r) = pcf::write(&f, &PcfWriteOptions::default()).unwrap();
        assert!(!r.entries.iter().any(|e| e.code == "pcf.write.lenient_hex"));
        assert_eq!(
            pcf::read(&b, &ResourceLimits::default())
                .unwrap()
                .0
                .glyphs()
                .next()
                .unwrap()
                .1
                .bitmap
                .as_bytes(),
            &[0, 0, 0, 0]
        );
    }
}
#[test]
fn empty_zero_area_and_large_uncompressed_metrics() {
    for f in [
        Font::default(),
        Font::from_glyphs([Glyph::new(
            Bitmap::<fontpixel::bitmap::Mono>::new(0, 4).unwrap(),
        )])
        .unwrap(),
    ] {
        let b = encoded(&f);
        assert_eq!(
            pcf::read(&b, &ResourceLimits::default()).unwrap().0.len(),
            f.len()
        );
    }
    let mut f = font();
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.metrics.advance = Some(Vector::new(200, 0));
        Ok(())
    })
    .unwrap();
    let b = encoded(&f);
    assert_eq!(toc(&b).into_iter().find(|t| t.0 == 4).unwrap().1 & 0x100, 0);
    assert_eq!(
        encoded(&pcf::read(&b, &ResourceLimits::default()).unwrap().0),
        b
    );
}

#[test]
fn source_tables_strings_partial_units_and_sentinels_are_honest() {
    let good = encoded(&font());
    // Missing terminal NUL is legal: strings end at their declared string block.
    let mut b = good.clone();
    let o = off(&b, 128);
    let n = u32::from_be_bytes(b[o + 4..o + 8].try_into().unwrap()) as usize;
    let p = o + 8 + 4 * n;
    let len = u32::from_be_bytes(b[p..p + 4].try_into().unwrap());
    b[p..p + 4].copy_from_slice(&(len - 1).to_be_bytes());
    assert_eq!(
        pcf::read(&b, &ResourceLimits::default())
            .unwrap()
            .0
            .glyphs()
            .last()
            .unwrap()
            .1
            .name
            .as_deref(),
        Some("g66")
    );
    let mut b = good.clone();
    let p = b.windows(4).position(|b| b == b"g65\0").unwrap();
    b[p + 1] = 0xe9;
    assert_eq!(
        pcf::read(&b, &ResourceLimits::default())
            .unwrap()
            .0
            .glyphs()
            .next()
            .unwrap()
            .1
            .name
            .as_deref(),
        Some("gé5")
    );
    // Relabel a one-byte padded buffer with a two-byte swap unit; the reader
    // swaps the whole block (not each row or each glyph), including partial units.
    let mut b = pcf::write(
        &font(),
        &PcfWriteOptions {
            glyph_pad: GlyphPad::One,
            bit_order: Order::Lsb,
            ..PcfWriteOptions::default()
        },
    )
    .unwrap()
    .0;
    let o = off(&b, 8);
    b[o] |= 0x10;
    assert!(pcf::read(&b, &ResourceLimits::default()).is_ok());
    let mut b = good.clone();
    let o = off(&b, 32);
    b[o + 12..o + 14].copy_from_slice(&65535u16.to_be_bytes());
    let (f, _) = pcf::read(&b, &ResourceLimits::default()).unwrap();
    assert!(
        pcf::write(&f, &PcfWriteOptions::default())
            .unwrap()
            .1
            .entries
            .iter()
            .any(|e| e.code == "pcf.write.default_character")
    );
    // Unknown table content stays in provenance; writing it away is a loss.
    let mut b = good.clone();
    let i = toc(&b).iter().position(|t| t.0 == 64).unwrap();
    b[8 + 16 * i..12 + 16 * i].copy_from_slice(&512u32.to_le_bytes());
    let (f, r) = pcf::read(&b, &ResourceLimits::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "pcf.read.unknown_table"));
    assert!(f.headers.get("pcf.table.512").is_some());
    assert!(
        pcf::write(&f, &PcfWriteOptions::default())
            .unwrap()
            .1
            .entries
            .iter()
            .any(|e| e.code == "pcf.write.header")
    );
    // A duplicate table has no semantic slot; report the discarded record as loss.
    let mut b = good.clone();
    let i = toc(&b).iter().position(|t| t.0 == 64).unwrap();
    let record = b[8..24].to_vec();
    b[8 + 16 * i..24 + 16 * i].copy_from_slice(&record);
    let (_, r) = pcf::read(&b, &ResourceLimits::default()).unwrap();
    assert!(r.has_loss());
}

#[test]
fn gray_threshold_and_uncompressed_ink_tables() {
    use fontpixel::bitmap::{BitmapLimits, Gray8, Mono};
    let mut f = font();
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.bitmap =
            Bitmap::<Gray8>::from_raw(3, 2, &[127, 128, 255, 0, 20, 200], BitmapLimits::default())?
                .into();
        Ok(())
    })
    .unwrap();
    assert!(pcf::write(&f, &PcfWriteOptions::default()).is_err());
    let (b, r) = pcf::write(
        &f,
        &PcfWriteOptions {
            gray_threshold: Some(128),
            ..PcfWriteOptions::default()
        },
    )
    .unwrap();
    assert!(r.entries.iter().any(|e| e.code == "pcf.write.grayscale"));
    let (back, _) = pcf::read(&b, &ResourceLimits::default()).unwrap();
    assert_eq!(
        back.glyphs().next().unwrap().1.bitmap.as_bytes(),
        &[0x60, 0x20]
    );
    let mut f = Font::default();
    f.metrics.ascent = Some(3);
    f.metrics.descent = Some(0);
    for cp in [65, 66] {
        let mut bytes = vec![0u8; 51];
        bytes[0] = 0x80;
        bytes[33] = 0x40;
        let mut g =
            Glyph::new(Bitmap::<Mono>::from_raw(130, 3, &bytes, BitmapLimits::default()).unwrap());
        g.codepoints.push(Codepoint::Unicode(cp));
        g.metrics.advance = Some(Vector::new(130, 0));
        f.add_glyph(g).unwrap();
    }
    let b = encoded(&f);
    let t = toc(&b);
    assert_eq!(t.iter().find(|t| t.0 == 4).unwrap().1 & 0x100, 0);
    assert_eq!(t.iter().find(|t| t.0 == 16).unwrap().1 & 0x100, 0);
    assert_ne!(t.iter().find(|t| t.0 == 2).unwrap().1 & 0x100, 0);
    assert_eq!(
        pcf::read(&b, &ResourceLimits::default()).unwrap().0.len(),
        2
    );
}

#[test]
fn accelerator_loss_cannot_hide_behind_byte_order_changes() {
    let mut bytes = encoded(&font());
    let o = off(&bytes, 2);
    bytes[o + 4] ^= 1;
    let (f, _) = pcf::read(&bytes, &ResourceLimits::default()).unwrap();
    let options = PcfWriteOptions {
        byte_order: Order::Lsb,
        ..PcfWriteOptions::default()
    };
    assert!(
        pcf::write(&f, &options)
            .unwrap()
            .1
            .entries
            .iter()
            .any(|e| e.code == "pcf.write.accelerators")
    );
    // Accelerators have independent byte orders, not necessarily the bitmap order.
    let mut bytes = encoded(&font());
    let o = off(&bytes, 2);
    let fmt = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    bytes[o..o + 4].copy_from_slice(&(fmt & !4).to_le_bytes());
    for word in bytes[o + 12..o + 24].as_chunks_mut::<4>().0 {
        word.reverse();
    }
    for word in bytes[o + 24..o + 48].as_chunks_mut::<2>().0 {
        word.reverse();
    }
    let (f, _) = pcf::read(&bytes, &ResourceLimits::default()).unwrap();
    assert!(
        !pcf::write(&f, &PcfWriteOptions::default())
            .unwrap()
            .1
            .entries
            .iter()
            .any(|e| e.code == "pcf.write.accelerators")
    );
}

#[test]
fn unknown_pcf_charset_stays_legacy_for_fallback_names_and_default() {
    let mut f = font();
    f.charset.registry = Some("UNKNOWN".into());
    f.charset.encoding = Some("CUSTOM".into());
    f.metrics.default_character = Some(Codepoint::Legacy(65));
    let b = strip(&encoded(&f), &[128]);
    let (back, _) = pcf::read(&b, &ResourceLimits::default()).unwrap();
    let g = back.glyphs().next().unwrap().1;
    assert_eq!(g.name.as_deref(), Some("char65"));
    assert_eq!(g.codepoints, vec![Codepoint::Legacy(65)]);
    assert_eq!(back.metrics.default_character, Some(Codepoint::Legacy(65)));
}

#[test]
fn conflicting_x11_headers_and_properties_report_all_losses() {
    #[cfg(feature = "bdf")]
    {
        let source=b"STARTFONT 2.2\nFONT source-name\nSIZE 8 75 75\nFONTBOUNDINGBOX 1 1 0 0\nDWIDTH 2 0\nSTARTPROPERTIES 3\nFONT \"property-name\"\nRESOLUTION_X 100\nFONT_ASCENT 1\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n";
        let (f, _) = fontpixel::formats::bdf::read(source, &ResourceLimits::default()).unwrap();
        let (_, r) = pcf::write(&f, &PcfWriteOptions::default()).unwrap();
        for code in [
            "pcf.write.bdf_version",
            "pcf.write.font_name",
            "pcf.write.resolution",
            "pcf.write.font_field",
        ] {
            assert!(r.entries.iter().any(|e| e.code == code), "{code}");
        }
    }
}
