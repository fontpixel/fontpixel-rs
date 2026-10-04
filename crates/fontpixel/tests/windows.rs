//! Independent Windows raster/container fixtures and loss/bounds regressions.
#![cfg(feature = "fnt")]
use fontpixel::formats::fnt::{self, FntWriteOptions, Version};
use fontpixel::model::{Codepoint, Property, PropertyValue, Vector};
use fontpixel::{Bitmap, Font, FontSet, Glyph, ResourceLimits};
fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn sample() -> Font {
    let mut f = Font::default();
    f.names.family = Some("Test".into());
    f.metrics.ascent = Some(2);
    f.metrics.descent = Some(1);
    for (cp, pixels) in [
        (65, "#.......#\n.#######.\n........."),
        (66, "#####\n#...#\n#####"),
    ] {
        let mut g = Glyph::new(pixels.parse::<Bitmap>().unwrap());
        g.codepoints.push(Codepoint::Unicode(cp));
        g.metrics.bounds.y = -1;
        g.metrics.advance = Some(Vector::new(i64::from(g.bitmap.width()), 0));
        f.add_glyph(g).unwrap();
    }
    f
}
// Independently authored field-by-field fixture from documented byte layout.
fn fixture(version: Version, fixed: bool) -> Vec<u8> {
    let h = match version {
        Version::One => 117,
        Version::Two => 118,
        Version::Three => 148,
    };
    let entry = match version {
        Version::One => {
            if fixed {
                0
            } else {
                2
            }
        }
        Version::Two => 4,
        Version::Three => 6,
    };
    let bits = h + 3 * entry;
    let n = if version == Version::One { 12 } else { 18 };
    let mut b = vec![0; bits + n + 5];
    put16(
        &mut b,
        0,
        match version {
            Version::One => 0x100,
            Version::Two => 0x200,
            Version::Three => 0x300,
        },
    );
    let size = b.len();
    put32(&mut b, 2, size as u32);
    put16(&mut b, 68, 3);
    put16(&mut b, 70, 72);
    put16(&mut b, 72, 72);
    put16(&mut b, 74, 2);
    put16(&mut b, 83, 400);
    put16(&mut b, 86, if fixed { 9 } else { 0 });
    put16(&mut b, 88, 3);
    b[90] = u8::from(!fixed);
    put16(&mut b, 91, 9);
    put16(&mut b, 93, 9);
    b[95] = 65;
    b[96] = 66;
    put16(&mut b, 99, 4);
    put32(&mut b, 105, (bits + n) as u32);
    put32(&mut b, 113, bits as u32);
    if version == Version::One {
        if !fixed {
            put16(&mut b, h, 0);
            put16(&mut b, h + 2, 9);
            put16(&mut b, h + 4, 18);
        }
        b[bits..bits + 12]
            .copy_from_slice(&[0x80, 0xe0, 0x40, 0, 0x7f, 0x20, 0x40, 0, 0, 0x60, 0x40, 0]);
    } else {
        for i in 0..3 {
            put16(&mut b, h + i * entry, 9);
            if entry == 4 {
                put16(&mut b, h + i * entry + 2, (bits + i * 6) as u16);
            } else {
                put32(&mut b, h + i * entry + 2, (bits + i * 6) as u32);
            }
        }
        b[bits..bits + 12].copy_from_slice(&[
            0x80, 0x7f, 0, 0x80, 0, 0, 0xc0, 0x40, 0xc0, 0x80, 0x80, 0x80,
        ]);
    }
    b[bits + n..].copy_from_slice(b"Test\0");
    b
}
#[test]
fn independent_versions_and_lossless_native_roundtrips() {
    for v in [Version::One, Version::Two, Version::Three] {
        for fixed in [false, true] {
            let b = fixture(v, fixed);
            assert_eq!(fnt::detect(&b), fontpixel::formats::Confidence::Certain);
            let (font, _) = fnt::read_default(&b, &ResourceLimits::default()).unwrap();
            assert_eq!(font.len(), 2);
            let g = font.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
            assert_eq!(g.bitmap.as_bytes(), &[0x80, 0x80, 0x7f, 0, 0, 0]);
            assert_eq!(g.metrics.bounds.y, -1);
            let (out, r) = fnt::write(&font, &FntWriteOptions::default()).unwrap();
            assert!(!r.has_loss(), "{v:?}: {:?}", r);
            let (back, _) = fnt::read_default(&out, &ResourceLimits::default()).unwrap();
            assert_eq!(font, back);
            assert_eq!(
                out,
                fnt::write(&font, &FntWriteOptions::default()).unwrap().0
            );
        }
    }
}
#[test]
fn write_versions_mapping_and_geometry() {
    for v in [Version::One, Version::Two, Version::Three] {
        let f = sample();
        let (b, _) = fnt::write(
            &f,
            &FntWriteOptions {
                version: Some(v),
                ..Default::default()
            },
        )
        .unwrap();
        let (back, _) = fnt::read_default(&b, &ResourceLimits::default()).unwrap();
        for (id, g) in f.glyphs() {
            let bg = back
                .glyph_for_codepoint(Codepoint::Legacy(g.codepoints[0].value()))
                .unwrap();
            assert_eq!(g.bitmap, bg.bitmap, "{id:?} {v:?}");
            assert_eq!(g.metrics, bg.metrics);
        }
    }
}
#[test]
fn cp1252_maps_euro_and_reports_nonrepresentable_aliases() {
    let mut f = sample();
    let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
    g.codepoints = vec![Codepoint::Unicode(0x20ac), Codepoint::Unicode(0x4e00)];
    f.add_glyph(g).unwrap();
    let (b, r) = fnt::write(&f, &Default::default()).unwrap();
    assert!(
        r.entries
            .iter()
            .any(|e| e.location.codepoint == Some(0x4e00))
    );
    let (back, _) = fnt::read_default(&b, &Default::default()).unwrap();
    assert!(back.glyph_for_codepoint(Codepoint::Legacy(0x80)).is_some());
    assert_eq!(back.charset.windows_charset, Some(0));
}
#[test]
fn unknown_charset_is_legacy_and_never_unicode_guessed() {
    let mut b = fixture(Version::Two, false);
    b[85] = 255;
    let (f, _) = fnt::read_default(&b, &Default::default()).unwrap();
    assert_eq!(f.charset.name.as_deref(), Some("unknown"));
    let (out, r) = fnt::write(&f, &Default::default()).unwrap();
    assert!(!r.has_loss(), "{r:?}");
    assert_eq!(out[85], 255);
    let mut f = sample();
    f.charset.windows_charset = Some(255);
    assert!(fnt::write(&f, &Default::default()).is_err());
}
#[test]
fn unsupported_layouts_offsets_and_truncations_fail() {
    for v in [Version::One, Version::Two, Version::Three] {
        let b = fixture(v, false);
        for n in 0..b.len() {
            assert!(
                fnt::read_default(&b[..n], &Default::default()).is_err(),
                "{v:?} {n}"
            );
        }
        let mut bad = b.clone();
        put16(&mut bad, 66, 1);
        assert!(fnt::read_default(&bad, &Default::default()).is_err());
        let mut bad = b.clone();
        put32(&mut bad, 113, 1);
        assert!(fnt::read_default(&bad, &Default::default()).is_err());
        if v == Version::Three {
            let mut bad = b;
            put32(&mut bad, 118, 4);
            assert!(fnt::read_default(&bad, &Default::default()).is_err());
        }
    }
}
#[test]
fn budgets_precede_allocations() {
    let b = fixture(Version::Two, false);
    for l in [
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 26,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: b.len(),
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 5,
            ..Default::default()
        },
    ] {
        assert!(fnt::read_default(&b, &l).is_err());
    }
    assert!(
        fnt::write(
            &sample(),
            &FntWriteOptions {
                max_output_bytes: 4,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn foreign_metadata_gray_and_strict_loss() {
    let mut f = sample();
    f.names.license = Some("MIT".into());
    f.properties.push(Property::new(
        "foreign",
        PropertyValue::Text("keep?".into()),
    ));
    let (b, r) = fnt::write(&f, &Default::default()).unwrap();
    assert!(!b.is_empty());
    assert!(r.has_loss());
    let opts = fontpixel::WriteOptions {
        strict: true,
        ..fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("test.fnt").unwrap())
    };
    assert!(matches!(
        fontpixel::registry().write(&FontSet::from(f), "fnt", &opts),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}

#[cfg(feature = "fon")]
mod containers {
    use super::*;
    use fontpixel::formats::fon;
    fn pe(payload: &[u8], plus: bool) -> Vec<u8> {
        let opt = if plus { 240 } else { 224 };
        let mut b = vec![0; 1024 + payload.len()];
        b[..2].copy_from_slice(b"MZ");
        put32(&mut b, 60, 128);
        b[128..132].copy_from_slice(b"PE\0\0");
        put16(&mut b, 134, 1);
        put16(&mut b, 148, opt);
        let o = 152;
        put16(&mut b, o, if plus { 0x20b } else { 0x10b });
        let dirs = if plus { 112 } else { 96 };
        put32(&mut b, o + dirs - 4, 16);
        put32(&mut b, o + dirs + 16, 0x2000);
        put32(&mut b, o + dirs + 20, 88);
        let s = o + usize::from(opt);
        b[s..s + 5].copy_from_slice(b".rsrc");
        let size = b.len() - 512;
        put32(&mut b, s + 8, size as u32);
        put32(&mut b, s + 12, 0x2000);
        let size = b.len() - 512;
        put32(&mut b, s + 16, size as u32);
        put32(&mut b, s + 20, 512);
        put16(&mut b, 526, 1);
        put32(&mut b, 528, 8);
        put32(&mut b, 532, 0x80000018);
        put16(&mut b, 550, 1);
        put32(&mut b, 552, 1);
        put32(&mut b, 556, 0x80000030);
        put16(&mut b, 574, 1);
        put32(&mut b, 576, 0);
        put32(&mut b, 580, 72);
        put32(&mut b, 584, 0x2200);
        put32(&mut b, 588, payload.len() as u32);
        b[1024..].copy_from_slice(payload);
        b
    }
    #[test]
    fn ne_multifont_and_fontdir_structure() {
        let (a, _) = fnt::read_default(&fixture(Version::Two, false), &Default::default()).unwrap();
        let (b, _) =
            fnt::read_default(&fixture(Version::Three, true), &Default::default()).unwrap();
        let set = FontSet {
            fonts: vec![a, b],
            ..Default::default()
        };
        let (out, r) = fon::write(&set, &Default::default()).unwrap();
        assert!(!r.has_loss(), "{r:?}");
        let (back, r) = fon::read(&out, &Default::default(), &Default::default()).unwrap();
        assert!(!r.has_loss(), "{r:?}");
        assert_eq!(back.fonts, set.fonts);
        assert_eq!(out, fon::write(&set, &Default::default()).unwrap().0);
        assert_eq!(fon::detect(&out), fontpixel::formats::Confidence::Certain);
    }
    #[test]
    fn independently_built_pe32_and_pe64_resources() {
        for plus in [false, true] {
            let payload = fixture(Version::Two, false);
            let b = pe(&payload, plus);
            let (set, r) = fon::read(&b, &Default::default(), &Default::default()).unwrap();
            assert!(!r.has_loss(), "{r:?}");
            assert_eq!(
                set.fonts[0],
                fnt::read_default(&payload, &Default::default()).unwrap().0
            );
            let mut cycle = b.clone();
            put32(&mut cycle, 532, 0x80000000);
            assert!(fon::read(&cycle, &Default::default(), &Default::default()).is_err());
            let mut bad = b.clone();
            put32(&mut bad, 584, 0xffffffff);
            assert!(fon::read(&bad, &Default::default(), &Default::default()).is_err());
            for len in [0, 63, 130, 180, 511, 550, 1024, b.len() - 1] {
                assert!(fon::read(&b[..len], &Default::default(), &Default::default()).is_err());
            }
        }
    }
    #[test]
    fn cumulative_limits_and_shift_overflow() {
        let f = fnt::read_default(&fixture(Version::Two, false), &Default::default())
            .unwrap()
            .0;
        let (mut b, _) = fon::write(
            &FontSet {
                fonts: vec![f.clone(), f],
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
        assert!(
            fon::read(
                &b,
                &Default::default(),
                &ResourceLimits {
                    max_glyphs: 3,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            fon::read(
                &b,
                &Default::default(),
                &ResourceLimits {
                    max_resources: 2,
                    ..Default::default()
                }
            )
            .is_err()
        );
        put16(&mut b, 192, 255);
        assert!(fon::read(&b, &Default::default(), &Default::default()).is_err());
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(160))]
    #[test]
    fn generated_byte_cells_roundtrip(width in 1u16..33, height in 1u16..33, descent in 0u16..32, pattern in proptest::collection::vec(proptest::num::u8::ANY, 128)) {
        let height=height.max(descent+1);
        let n=usize::from(width).div_ceil(8)*usize::from(height);
        let data:Vec<u8>=(0..n).map(|i|pattern[i%pattern.len()]).collect();
        let bitmap=Bitmap::<fontpixel::bitmap::Mono>::from_raw(u32::from(width),u32::from(height),&data,Default::default()).unwrap();
        let mut f=Font::default();f.charset.name=Some("cp1252".into());f.metrics.ascent=Some(i64::from(height-descent));f.metrics.descent=Some(i64::from(descent));
        let mut g=Glyph::new(bitmap.clone());g.codepoints.push(Codepoint::Legacy(0));g.metrics.bounds.y = -i64::from(descent);g.metrics.advance=Some(Vector::new(i64::from(width),0));f.add_glyph(g).unwrap();
        for version in [Version::One,Version::Two,Version::Three] {
            let(bytes,_)=fnt::write(&f,&FntWriteOptions{version:Some(version),..Default::default()}).unwrap();let(back,_)=fnt::read_default(&bytes,&Default::default()).unwrap();
            let actual=back.glyph_for_codepoint(Codepoint::Legacy(0)).unwrap();proptest::prop_assert_eq!(&actual.bitmap,&fontpixel::model::GlyphBitmap::Mono(bitmap.clone()));
            let(round,r)=fnt::write(&back,&Default::default()).unwrap();proptest::prop_assert!(!r.has_loss(),"{:?}",r);proptest::prop_assert_eq!(back,fnt::read_default(&round,&Default::default()).unwrap().0);
        }
    }
}

#[test]
fn edits_and_aliases_have_honest_reports() {
    let (mut f, _) =
        fnt::read_default(&fixture(Version::Three, false), &Default::default()).unwrap();
    let id = f.glyph_id(Codepoint::Legacy(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Legacy(67));
        g.name = Some("A".into());
        g.unicode_sequences.push(vec![65, 66]);
        Ok(())
    })
    .unwrap();
    let (_, r) = fnt::write(&f, &Default::default()).unwrap();
    for code in ["fnt.alias_identity", "fnt.glyph_name", "fnt.sequence"] {
        assert!(r.entries.iter().any(|e| e.code == code), "{code}: {r:?}");
    }
    f.names.localized.push(Property::new("zh", "名字"));
    let (_, r) = fnt::write(&f, &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "fnt.localized_names"));
}

#[test]
fn source_metadata_and_output_work_are_bounded() {
    let mut f = sample();
    f.names.family = Some("x".repeat(10000));
    assert!(
        fnt::write(
            &f,
            &FntWriteOptions {
                limits: ResourceLimits {
                    max_table_bytes: 100,
                    ..Default::default()
                },
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut nested = PropertyValue::Flag;
    for _ in 0..40 {
        nested = PropertyValue::List(vec![nested]);
    }
    f.properties.push(Property::new("deep", nested));
    assert!(fnt::write(&f, &Default::default()).is_err());
}

#[test]
fn cjk_legacy_bytes_roundtrip_without_conversion_tables() {
    for charset in [128, 129, 130, 134, 136] {
        let mut wire = fixture(Version::Two, false);
        wire[85] = charset;
        let (f, _) = fnt::read_default(&wire, &Default::default()).unwrap();
        let (out, r) = fnt::write(&f, &Default::default()).unwrap();
        assert!(!r.has_loss(), "{charset}: {r:?}");
        assert_eq!(f, fnt::read_default(&out, &Default::default()).unwrap().0);
    }
}

#[cfg(feature = "yaff")]
#[test]
fn malformed_deserialized_rational_is_rejected_without_panic() {
    let mut f = sample();
    f.metrics.point_size =
        Some(serde_json::from_str(r#"{"numerator":12,"denominator":0}"#).unwrap());
    assert!(fnt::write(&f, &Default::default()).is_err());
}

#[test]
fn zero_advance_mapping_is_reported_without_blocking_other_glyphs() {
    let mut f = sample();
    let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
    g.codepoints.push(Codepoint::Unicode(67));
    g.metrics.advance = Some(Vector::new(0, 0));
    f.add_glyph(g).unwrap();
    let (b, r) = fnt::write(&f, &Default::default()).unwrap();
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "fnt.zero_width_code" && e.location.codepoint == Some(67))
    );
    assert_eq!(
        fnt::read_default(&b, &Default::default()).unwrap().0.len(),
        2
    );
}

#[test]
fn large_unicode_sources_report_omissions_with_default_budget() {
    let mut f = sample();
    for cp in 0x10000..0x10000 + 38653 {
        if char::from_u32(cp).is_none() {
            continue;
        }
        let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
        g.codepoints.push(Codepoint::Unicode(cp));
        f.add_glyph(g).unwrap();
    }
    let (bytes, r) = fnt::write(&f, &Default::default()).unwrap();
    assert_eq!(
        fnt::read_default(&bytes, &Default::default())
            .unwrap()
            .0
            .len(),
        2
    );
    assert_eq!(
        r.entries
            .iter()
            .filter(|e| e.code == "fnt.unmapped_code")
            .count(),
        38653
    );
}

#[test]
fn wholly_unrepresentable_fonts_keep_mapping_diagnostics() {
    for zero_width in [false, true] {
        let mut f = Font::default();
        let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
        g.codepoints = vec![Codepoint::Unicode(65), Codepoint::Unicode(0x4e00)];
        if zero_width {
            g.metrics.advance = Some(Vector::new(0, 0));
        } else {
            g.codepoints.remove(0);
        }
        f.add_glyph(g).unwrap();
        let error = fnt::write(&f, &Default::default()).unwrap_err();
        let report = error
            .report()
            .expect("fatal conversion preserves omissions");
        assert!(report.has_errors());
        for cp in if zero_width {
            vec![65, 0x4e00]
        } else {
            vec![0x4e00]
        } {
            assert!(
                report
                    .entries
                    .iter()
                    .any(|e| e.location.codepoint == Some(cp))
            );
        }
        assert!(report.entries.iter().any(|e| e.code == "fnt.omitted_glyph"));
        assert!(
            report
                .entries
                .iter()
                .any(|e| e.code == "fnt.no_representable_glyphs")
        );
    }
}

#[test]
fn container_metadata_cannot_bypass_fnt_budget() {
    let mut set = FontSet::from(sample());
    set.properties
        .push(Property::new("x".repeat(1024 * 1024), "value"));
    let mut registry = fontpixel::registry();
    registry.set_fnt_write_options(FntWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 128 * 1024,
            ..Default::default()
        },
        ..Default::default()
    });
    let options = fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("font.fnt").unwrap());
    assert!(matches!(
        registry.write(&set, "fnt", &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    #[cfg(feature = "fon")]
    assert!(matches!(
        registry.write(&set, "fon", &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[cfg(feature = "fon")]
#[test]
fn fon_retained_sources_and_reports_share_one_budget() {
    use fontpixel::formats::fon;
    let mut f = sample();
    f.headers.push(Property::new("x".repeat(20_000), "omitted"));
    let options = FntWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 256 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(fon::write(&FontSet::from(f.clone()), &options).is_ok());
    let set = FontSet {
        fonts: vec![f; 16],
        ..Default::default()
    };
    assert!(matches!(
        fon::write(&set, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[cfg(feature = "fon")]
#[test]
fn fon_payload_and_alignment_obey_reader_resource_cap() {
    use fontpixel::formats::fon;
    let set = FontSet::from(sample());
    let (out, _) = fon::write(&set, &Default::default()).unwrap();
    // First FONT entry starts at resource table + 30; its size includes padding.
    let table = 192;
    let shift = u16::from_le_bytes(out[table..table + 2].try_into().unwrap());
    let size = usize::from(u16::from_le_bytes(
        out[table + 32..table + 34].try_into().unwrap(),
    )) << shift;
    let options = FntWriteOptions {
        limits: ResourceLimits {
            max_resource_bytes: size - 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        fon::write(&set, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    let options = FntWriteOptions {
        limits: ResourceLimits {
            max_resource_bytes: size,
            ..Default::default()
        },
        ..Default::default()
    };
    let (out, _) = fon::write(&set, &options).unwrap();
    assert!(fon::read(&out, &Default::default(), &options.limits).is_ok());
}

#[cfg(feature = "fon")]
#[test]
fn fon_failed_font_preserves_index_and_prior_losses() {
    let mut first = sample();
    first.names.license = Some("MIT".into());
    let mut second = sample();
    for id in second.glyphs().map(|(id, _)| id).collect::<Vec<_>>() {
        second
            .edit_glyph(id, |g| {
                g.metrics.advance = Some(Vector::new(0, 0));
                Ok(())
            })
            .unwrap();
    }
    let set = FontSet {
        fonts: vec![first, second],
        ..Default::default()
    };
    let error = fontpixel::formats::fon::write(&set, &Default::default()).unwrap_err();
    let report = error.report().expect("font failure keeps report");
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.location.font_index == Some(0))
    );
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "fnt.no_representable_glyphs" && e.location.font_index == Some(1))
    );
}

#[cfg(feature = "fon")]
#[test]
fn repeated_ne_payloads_still_consume_glyph_budget() {
    use fontpixel::formats::fon;
    let set = FontSet {
        fonts: vec![sample(), sample()],
        ..Default::default()
    };
    let (mut bytes, _) = fon::write(&set, &Default::default()).unwrap();
    // Make two separate resource entries refer to the same encoded FONT.
    let first = bytes[222..226].to_vec();
    bytes[234..238].copy_from_slice(&first);
    assert_eq!(
        fon::read(&bytes, &Default::default(), &Default::default())
            .unwrap()
            .0
            .fonts
            .len(),
        2
    );
    assert!(matches!(
        fon::read(
            &bytes,
            &Default::default(),
            &ResourceLimits {
                max_glyphs: 3,
                ..Default::default()
            }
        ),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn full_byte_range_aliases_are_readable_and_strict_reports_identity() {
    let mut font = Font::default();
    font.charset.name = Some("unknown".into());
    font.charset.windows_charset = Some(255);
    let mut glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
    glyph.codepoints = (0..=255).map(Codepoint::Legacy).collect();
    font.add_glyph(glyph).unwrap();
    for version in [Version::One, Version::Two, Version::Three] {
        let (bytes, report) = fnt::write(
            &font,
            &FntWriteOptions {
                version: Some(version),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            fnt::read_default(&bytes, &Default::default())
                .unwrap()
                .0
                .len(),
            256
        );
        assert!(
            report
                .entries
                .iter()
                .any(|e| e.code == "fnt.alias_identity")
        );
    }
    let options = fontpixel::WriteOptions {
        strict: true,
        ..fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("font.fnt").unwrap())
    };
    assert!(matches!(
        fontpixel::registry().write(&FontSet::from(font), "fnt", &options),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
