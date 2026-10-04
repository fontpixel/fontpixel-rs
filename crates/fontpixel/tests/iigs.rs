//! Original QuickDraw II wire fixtures generated directly from the Apple layout.
#![cfg(feature = "iigs")]
use fontpixel::formats::iigs::{self, IigsReadOptions, IigsWriteOptions, Version};
use fontpixel::model::{Codepoint, Glyph, Property, PropertyValue, Vector};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits};
fn put(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn wire() -> Vec<u8> {
    let mut b = vec![0; 1 + 4 + 12 + 26 + 6 + 8 + 8];
    b[..5].copy_from_slice(b"\x04Test");
    let h = [6, 42, 3, 9, 0x101, 4];
    for (i, v) in h.into_iter().enumerate() {
        put(&mut b, 5 + i * 2, v);
    }
    let mf = [0xffff, 65, 66, 3, 0xffff, 0xffff, 3, 3, 12, 2, 1, 1, 1];
    for (i, v) in mf.into_iter().enumerate() {
        put(&mut b, 17 + i * 2, v);
    }
    b[43..49].copy_from_slice(&[0xb8, 0, 0xc8, 0, 0xb8, 0]);
    for (i, v) in [0, 2, 3, 5, 3, 0x102, 3, 0xffff].into_iter().enumerate() {
        put(&mut b, 49 + i * 2, v);
    }
    b
}
fn read(b: &[u8]) -> FontSet {
    iigs::read(b, &Default::default(), &ResourceLimits::default())
        .unwrap()
        .0
}
#[test]
fn apple_wire_has_unsigned_width_offset_and_unswapped_bitmap() {
    let s = read(&wire());
    let f = s.single().unwrap();
    assert_eq!(f.names.family.as_deref(), Some("Test"));
    assert_eq!(f.names.style.as_deref(), Some("Bold Italic"));
    assert_eq!(f.len(), 3);
    let a = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!(a.metrics.bounds.x, -1);
    assert_eq!(a.metrics.bounds.y, -1);
    assert_eq!(a.metrics.advance, Some(Vector::new(3, 0)));
    assert_eq!(a.bitmap.as_bytes(), [0x80, 0xc0, 0x80]);
    assert!(
        f.glyph(f.metrics.default_glyph.unwrap())
            .unwrap()
            .codepoints
            .is_empty()
    );
    assert_eq!(
        f.properties.get("iigs.family-id"),
        Some(&PropertyValue::Integer(42))
    );
    let (out, r) = iigs::write(&s, &Default::default()).unwrap();
    assert_eq!(out, wire());
    assert!(!r.has_loss(), "{r}");
    assert_eq!(read(&out), s);
}
#[test]
fn native_unknown_header_extension_and_trailing_bytes_roundtrip() {
    let mut b = wire();
    b.splice(17..17, [0x88, 0x77, 0x66, 0x55]);
    put(&mut b, 5, 8);
    b.extend([1, 2, 3]);
    let s = read(&b);
    let (out, r) = iigs::write(&s, &Default::default()).unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn location_table_follows_offset_pointer_across_padding() {
    let mut b = wire();
    // Apple specifies locTable immediately before owTable; owTLoc, rather than
    // the raster size, locates both tables. This gap is legal and retained.
    b.splice(49..49, [0xde, 0xad, 0xbe, 0xef]);
    put(&mut b, 33, 14);
    let s = read(&b);
    let expected = read(&wire());
    assert!(s.fonts[0].glyphs().eq(expected.fonts[0].glyphs()));
    let (out, r) = iigs::write(&s, &Default::default()).unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn edited_native_extent_cannot_undersize_text_buffer() {
    let mut s = read(&wire());
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.advance = Some(Vector::new(13, 0));
            Ok(())
        })
        .unwrap();
    let (out, r) = iigs::write(&s, &Default::default()).unwrap();
    assert_eq!(
        read(&out).fonts[0].properties.get("iigs.fbr-extent"),
        Some(&PropertyValue::Integer(13))
    );
    assert!(r.entries.iter().any(|e| e.code == "iigs.properties"));
}
#[test]
fn successful_writer_rereads_at_its_exact_cumulative_budget() {
    for container in [iigs::Container::Bare, iigs::Container::BinScii] {
        let s = read(&wire());
        let mut lower = 0;
        let mut upper = 1 << 20;
        while lower + 1 < upper {
            let middle = lower + (upper - lower) / 2;
            let options = IigsWriteOptions {
                container,
                limits: ResourceLimits {
                    max_total_bytes: middle,
                    ..Default::default()
                },
                ..Default::default()
            };
            if iigs::write(&s, &options).is_ok() {
                upper = middle;
            } else {
                lower = middle;
            }
        }
        let limits = ResourceLimits {
            max_total_bytes: upper,
            ..Default::default()
        };
        let (out, _) = iigs::write(
            &s,
            &IigsWriteOptions {
                container,
                limits: limits.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        iigs::read(&out, &Default::default(), &limits).unwrap();
        assert!(
            iigs::write(
                &s,
                &IigsWriteOptions {
                    container,
                    limits: ResourceLimits {
                        max_total_bytes: upper - 1,
                        ..limits
                    },
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
#[test]
fn canonical_versions_reopen_and_preserve_pixels() {
    let mut s = read(&wire());
    s.fonts[0].properties.remove("iigs.source");
    for version in [Version::V1_1, Version::V1_5] {
        let (out, _) = iigs::write(
            &s,
            &IigsWriteOptions {
                version,
                ..Default::default()
            },
        )
        .unwrap();
        let t = read(&out);
        for ((_, a), (_, b)) in s.fonts[0].glyphs().zip(t.fonts[0].glyphs()) {
            assert_eq!(a, b);
        }
        let (again, r) = iigs::write(&t, &Default::default()).unwrap();
        assert_eq!(out, again);
        assert!(!r.has_loss(), "{r}");
    }
}
#[test]
fn extended_32_bit_width_offset_and_explicit_legacy_version_failure() {
    let mut f = Font::default();
    let mut g = Glyph::new(
        Bitmap::<fontpixel::bitmap::Mono>::from_raw(
            128,
            5000,
            &vec![0xa5; 16 * 5000],
            Default::default(),
        )
        .unwrap(),
    );
    g.metrics.advance = Some(Vector::new(128, 0));
    g.codepoints.push(Codepoint::Legacy(65));
    f.add_glyph(g.clone()).unwrap();
    g.codepoints.clear();
    let id = f.add_glyph(g).unwrap();
    f.metrics.default_glyph = Some(id);
    let s = FontSet::from(f);
    let (out, _) = iigs::write(&s, &Default::default()).unwrap();
    let anchor = 1 + out[0] as usize;
    assert_eq!(
        u16::from_le_bytes([out[anchor + 8], out[anchor + 9]]),
        0x105
    );
    assert_eq!(out[anchor + 12], 1);
    assert_eq!(read(&out).fonts[0].len(), 2);
    assert!(
        iigs::write(
            &s,
            &IigsWriteOptions {
                version: Version::V1_1,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn malformed_tables_and_every_truncation_fail() {
    let b = wire();
    for end in 0..b.len() {
        assert!(
            iigs::read(&b[..end], &Default::default(), &ResourceLimits::default()).is_err(),
            "end={end}"
        );
    }
    for (at, v) in [
        (5, 5),
        (13, 0x106),
        (19, 67),
        (21, 256),
        (33, 1),
        (49, 6),
        (51, 4),
        (61, 0xffff),
        (63, 0),
        (57, 0xff00),
        (57, 0x00ff),
    ] {
        let mut b = wire();
        put(&mut b, at, v);
        assert!(
            iigs::read(&b, &Default::default(), &ResourceLimits::default()).is_err(),
            "at={at}"
        );
    }
}
#[test]
fn signed_vertical_metrics_allow_rectangles_outside_baseline() {
    // Apple IIgs Toolbox Reference II p16-48 permits negative ascent/descent
    // and positive kernMax/nDescent; neither axis must touch the origin.
    for (ascent, descent, leading) in [(8i16, -5i16, -2i16), (-2, 5, 1)] {
        let mut bytes = wire();
        put(&mut bytes, 25, 2); // positive kernMax
        put(&mut bytes, 27, (-descent) as u16);
        put(&mut bytes, 35, ascent as u16);
        put(&mut bytes, 37, descent as u16);
        put(&mut bytes, 39, leading as u16);
        let mut s = read(&bytes);
        let f = &s.fonts[0];
        assert_eq!(f.metrics.ascent, Some(i64::from(ascent)));
        assert_eq!(f.metrics.descent, Some(i64::from(descent)));
        assert_eq!(f.metrics.line_gap, Some(i64::from(leading)));
        assert_eq!(f.metrics.pixel_size, Some(3));
        for (_, g) in f.glyphs() {
            assert_eq!(g.metrics.bounds.y, -i64::from(descent));
            assert!(g.metrics.bounds.x >= 2);
        }
        let (out, r) = iigs::write(&s, &Default::default()).unwrap();
        assert_eq!(out, bytes);
        assert!(!r.has_loss(), "{r}");
        s.fonts[0].properties.remove("iigs.source");
        for version in [Version::V1_1, Version::V1_5] {
            let (out, _) = iigs::write(
                &s,
                &IigsWriteOptions {
                    version,
                    ..Default::default()
                },
            )
            .unwrap();
            let t = read(&out);
            assert_eq!(t.fonts[0].metrics, s.fonts[0].metrics);
            assert!(t.fonts[0].glyphs().eq(s.fonts[0].glyphs()));
            assert!(!iigs::write(&t, &Default::default()).unwrap().1.has_loss());
        }
    }
}
#[test]
fn unmapped_unicode_and_unrepresentable_bearings_are_explicit() {
    let mut s = read(&wire());
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.codepoints = [Codepoint::Unicode(65)].to_vec();
            Ok(())
        })
        .unwrap();
    let (_, r) = iigs::write(&s, &Default::default()).unwrap();
    assert!(r.has_loss());
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.codepoints = [Codepoint::Legacy(65)].to_vec();
            g.metrics.bounds.x = 1000;
            Ok(())
        })
        .unwrap();
    assert!(iigs::write(&s, &Default::default()).is_err());
}
#[test]
fn native_edits_are_not_hidden_and_all_metadata_is_audited() {
    let mut s = read(&wire());
    s.fonts[0].names.family = Some("Changed".into());
    s.fonts[0].names.license = Some("MIT".into());
    s.fonts[0].headers.push(Property::new("source-header", "x"));
    s.fonts[0].metrics.ppem_x = Some(17);
    s.properties.push(Property::new("set-property", "x"));
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.name = Some("A".into());
            g.unicode_sequences.push(vec![65, 66]);
            g.metrics.vertical_advance = Some(Vector::new(0, 8));
            Ok(())
        })
        .unwrap();
    let (out, r) = iigs::write(&s, &Default::default()).unwrap();
    assert_eq!(read(&out).fonts[0].names.family.as_deref(), Some("Changed"));
    for code in [
        "iigs.collection_metadata",
        "iigs.names",
        "iigs.properties",
        "iigs.font_metrics",
        "iigs.glyphs",
    ] {
        assert!(r.entries.iter().any(|e| e.code == code), "{code}: {r}");
    }
}
#[test]
fn reader_charset_is_explicit_and_not_serialized_silently() {
    let s = iigs::read(
        &wire(),
        &IigsReadOptions {
            charset: Some("macroman".into()),
        },
        &Default::default(),
    )
    .unwrap()
    .0;
    assert_eq!(
        s.fonts[0].glyphs().next().unwrap().1.codepoints,
        [Codepoint::Legacy(65)]
    );
    let (_, r) = iigs::write(&s, &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "iigs.charset"));
}
#[test]
fn tight_limits_enforce_source_output_and_readback_budget() {
    let s = read(&wire());
    for l in [
        ResourceLimits {
            max_total_bytes: 2000,
            ..Default::default()
        },
        ResourceLimits {
            max_glyphs: 2,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 5,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 20,
            ..Default::default()
        },
        ResourceLimits {
            max_input_bytes: 20,
            ..Default::default()
        },
    ] {
        assert!(iigs::read(&wire(), &Default::default(), &l).is_err());
        assert!(
            iigs::write(
                &s,
                &IigsWriteOptions {
                    limits: l,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    assert!(
        iigs::write(
            &s,
            &IigsWriteOptions {
                max_output_bytes: 10,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn grayscale_requires_explicit_conversion_and_reports_loss() {
    let mut s = read(&wire());
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.bitmap = Bitmap::<fontpixel::bitmap::Gray8>::from_raw(
                2,
                3,
                &[0, 127, 128, 255, 255, 0],
                Default::default(),
            )?
            .into();
            Ok(())
        })
        .unwrap();
    assert!(iigs::write(&s, &Default::default()).is_err());
    let (out, r) = iigs::write(
        &s,
        &IigsWriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.has_loss());
    assert_eq!(
        read(&out).fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        [0, 0xc0, 0x80]
    );
}
#[test]
fn binscii_segments_reorder_roundtrip_crc_and_completeness() {
    use iigs::Container;
    let mut raw = wire();
    raw.extend(vec![0x5a; 30000]);
    let s = read(&raw);
    let opts = IigsWriteOptions {
        container: Container::BinScii,
        ..Default::default()
    };
    let (text, initial_report) = iigs::write(&s, &opts).unwrap();
    assert!(!initial_report.has_loss(), "{initial_report}");
    assert!(text.starts_with(b"FiLeStArTfIlEsTaRt\n"));
    let decoded = read(&text);
    assert_eq!(s.fonts, decoded.fonts);
    let (again, r) = iigs::write(&decoded, &opts).unwrap();
    assert_eq!(text, again);
    assert!(!r.has_loss(), "{r}");
    let (_, r) = iigs::write(&decoded, &Default::default()).unwrap();
    assert!(r.has_loss());
    let source = std::str::from_utf8(&text).unwrap();
    let segments: Vec<_> = source
        .split("FiLeStArTfIlEsTaRt\n")
        .skip(1)
        .map(|s| format!("FiLeStArTfIlEsTaRt\n{s}"))
        .collect();
    assert_eq!(segments.len(), 3);
    let reverse = segments.iter().rev().cloned().collect::<String>();
    assert_eq!(read(reverse.as_bytes()).fonts, s.fonts);
    for broken in [
        segments[..2].join(""),
        format!("{}{}{}", segments[0], segments[0], segments[1]),
        format!("{}{}", segments[0], segments[2]),
    ] {
        assert!(iigs::read(broken.as_bytes(), &Default::default(), &Default::default()).is_err());
    }
    let mut bad = text.clone();
    let header_start = 19 + 65 + 16;
    bad[header_start] = if bad[header_start] == b'A' {
        b'B'
    } else {
        b'A'
    };
    assert!(iigs::read(&bad, &Default::default(), &Default::default()).is_err());
    let mut bad = text.clone();
    let raster_start = 19 + 65 + 53;
    bad[raster_start] = if bad[raster_start] == b'A' {
        b'B'
    } else {
        b'A'
    };
    assert!(iigs::read(&bad, &Default::default(), &Default::default()).is_err());
    let mut bad = text.clone();
    bad[20] = bad[19];
    assert!(iigs::read(&bad, &Default::default(), &Default::default()).is_err());
    for l in [
        ResourceLimits {
            max_resources: 2,
            ..Default::default()
        },
        ResourceLimits {
            max_decompressed_bytes: 100,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 100_000,
            ..Default::default()
        },
    ] {
        assert!(iigs::read(&text, &Default::default(), &l).is_err());
        assert!(
            iigs::write(
                &s,
                &IigsWriteOptions {
                    limits: l,
                    container: Container::BinScii,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
#[test]
fn binscii_custom_dictionary_and_cr_line_ends() {
    let (text, _) = iigs::write(
        &read(&wire()),
        &IigsWriteOptions {
            container: iigs::Container::BinScii,
            ..Default::default()
        },
    )
    .unwrap();
    let standard = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789()";
    let mut lines: Vec<Vec<u8>> = text
        .split(|&c| c == b'\n')
        .filter(|l| !l.is_empty())
        .map(<[u8]>::to_vec)
        .collect();
    lines[1].reverse();
    for (i, line) in lines.iter_mut().enumerate().skip(2) {
        for byte in if i == 2 {
            &mut line[16..]
        } else {
            line.as_mut_slice()
        } {
            let at = standard.iter().position(|c| c == byte).unwrap();
            *byte = standard[63 - at];
        }
    }
    let joined = lines.join(&b'\r');
    assert_eq!(read(&joined).fonts, read(&wire()).fonts);
}
#[test]
fn registry_strict_mode_rejects_complete_model_loss() {
    let mut s = read(&wire());
    let mut options =
        fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("test.fon").unwrap());
    options.strict = true;
    assert!(fontpixel::write(&s, "iigs", &options).is_ok());
    s.fonts[0].names.license = Some("source license".into());
    match fontpixel::write(&s, "iigs", &options) {
        Err(fontpixel::Error::StrictLoss(report)) => assert!(report.has_loss()),
        other => panic!("expected StrictLoss, got {other:?}"),
    }
}

#[test]
fn removed_trailing_slot_is_reported_even_when_live_ids_are_unchanged() {
    let mut set = read(&wire());
    let original_slots = set.fonts[0].slot_count();
    let removed = set.fonts[0].add_glyph(Glyph::default()).unwrap();
    set.fonts[0].remove_glyph(removed);
    assert_eq!(set.fonts[0].slot_count(), original_slots + 1);
    let (_, report) = iigs::write(&set, &Default::default()).unwrap();
    assert!(report.has_loss(), "{report}");
}
