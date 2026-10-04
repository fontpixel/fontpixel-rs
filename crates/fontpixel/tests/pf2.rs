//! Independent wire fixtures, exact native roundtrips, and hostile PF2 inputs.
#![cfg(feature = "pf2")]
use fontpixel::formats::pf2::{self, Pf2ReadOptions, Pf2WriteOptions};
use fontpixel::model::{Codepoint, Property, PropertyValue, Vector};
use fontpixel::{Font, ResourceLimits};
fn section(b: &mut Vec<u8>, tag: &[u8], data: &[u8]) {
    b.extend(tag);
    b.extend((data.len() as u32).to_be_bytes());
    b.extend(data);
}
fn fixture(w: u16, h: u16, raw: &[u8], aliases: bool) -> Vec<u8> {
    let mut b = Vec::new();
    section(&mut b, b"FILE", b"PFF2");
    section(&mut b, b"NAME", b"W42\0");
    section(&mut b, b"FAMI", b"Test\0");
    section(&mut b, b"PTSZ", &9u16.to_be_bytes());
    section(&mut b, b"ASCE", &7u16.to_be_bytes());
    section(&mut b, b"DESC", &2u16.to_be_bytes());
    section(&mut b, b"WEIG", b"normal\0");
    section(&mut b, b"MAXW", &w.to_be_bytes());
    section(&mut b, b"TEST", &[3, 7]);
    let count = if aliases { 2 } else { 1 };
    let offset = b.len() + 8 + count * 9 + 8;
    let mut index = Vec::new();
    for cp in [65u32, 0x4e00].into_iter().take(count) {
        index.extend(cp.to_be_bytes());
        index.push(0);
        index.extend((offset as u32).to_be_bytes());
    }
    section(&mut b, b"CHIX", &index);
    b.extend(b"DATA\xff\xff\xff\xff");
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
    b.extend((-2i16).to_be_bytes());
    b.extend((-1i16).to_be_bytes());
    b.extend((w as i16 + 1).to_be_bytes());
    b.extend(raw);
    b
}
fn load(b: &[u8]) -> Font {
    pf2::read(b, &Pf2ReadOptions::default(), &ResourceLimits::default())
        .unwrap()
        .0
}
#[test]
fn independent_bit_packing_and_metrics() {
    let f = load(&fixture(3, 3, &[0xaa, 0x80], true));
    let g = f.glyphs().next().unwrap().1;
    assert_eq!(g.bitmap.as_bytes(), [0xa0, 0x40, 0xa0]);
    assert_eq!(
        g.metrics.bounds,
        fontpixel::bitmap::Bounds::new(3, 3, -2, -1)
    );
    assert_eq!(g.metrics.advance, Some(Vector::new(4, 0)));
    assert_eq!(
        g.codepoints,
        [Codepoint::Unicode(65), Codepoint::Unicode(0x4e00)]
    );
}
#[test]
fn independent_native_alias_roundtrip() {
    let f = load(&fixture(3, 3, &[0xaa, 0x80], true));
    let (b, r) = pf2::write(&f, &Pf2WriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(load(&b), f);
}
#[test]
fn randomized_continuous_bit_rows() {
    let mut state = 0x1a93u64;
    for w in 0..40u16 {
        for h in 0..11u16 {
            let pixels = w as usize * h as usize;
            let mut raw = vec![0; pixels.div_ceil(8)];
            for b in &mut raw {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                *b = (state >> 40) as u8;
            }
            if !pixels.is_multiple_of(8) {
                *raw.last_mut().unwrap() &= 255 << (8 - pixels % 8);
            }
            let f = load(&fixture(w, h, &raw, true));
            let (b, r) = pf2::write(&f, &Pf2WriteOptions::default()).unwrap();
            assert!(!r.has_loss(), "{r}");
            assert_eq!(load(&b), f);
        }
    }
}
#[test]
fn truncation_and_storage_flags_fail() {
    let good = fixture(3, 3, &[0xaa, 0x80], true);
    for end in 0..good.len() {
        assert!(
            pf2::read(&good[..end], &Default::default(), &Default::default()).is_err(),
            "{end}"
        );
    }
    let mut bad = good;
    let at = bad.windows(4).position(|v| v == b"CHIX").unwrap() + 12;
    bad[at] = 1;
    assert!(pf2::read(&bad, &Default::default(), &Default::default()).is_err());
}
#[test]
fn limits_and_same_limit_writer_readback() {
    let f = load(&fixture(3, 3, &[0xaa, 0x80], true));
    for max in [0, 1, 2048, 4096, 8192, 65536] {
        let mut o = Pf2WriteOptions::default();
        o.limits.max_total_bytes = max;
        if let Ok((b, _)) = pf2::write(&f, &o) {
            assert!(pf2::read(&b, &Default::default(), &o.limits).is_ok());
        }
    }
    let l = ResourceLimits {
        max_glyphs: 1,
        ..Default::default()
    };
    assert!(pf2::read(&fixture(3, 3, &[0xaa, 0x80], true), &Default::default(), &l).is_err());
}
#[test]
fn located_losses_and_fatal_omissions_survive() {
    let mut f = load(&fixture(3, 3, &[0xaa, 0x80], false));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.metrics.vertical_advance = Some(Vector::new(0, -3));
        g.properties
            .push(Property::new("extra", PropertyValue::Flag));
        g.codepoints.push(Codepoint::Unicode(0x110000));
        Ok(())
    })
    .unwrap();
    let (_, r) = pf2::write(&f, &Default::default()).unwrap();
    assert!(
        r.entries.iter().any(
            |e| e.code.ends_with("write.omitted") && e.location.glyph_index == Some(id.index())
        )
    );
    assert!(
        r.entries
            .iter()
            .any(|e| e.code.ends_with("vertical_advance"))
    );
    f.edit_glyph(id, |g| {
        g.codepoints = vec![Codepoint::Unicode(0x110000)];
        Ok(())
    })
    .unwrap();
    let e = pf2::write(&f, &Default::default()).unwrap_err();
    assert!(
        e.report()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.location.codepoint == Some(0x110000))
    );
}

#[test]
fn native_grub_joining_variants_are_not_discarded() {
    let mut b = fixture(3, 3, &[0xaa, 0x80], false);
    let pos = b.windows(4).position(|v| v == b"CHIX").unwrap() + 8;
    b[pos..pos + 4].copy_from_slice(&0xc0000628u32.to_be_bytes());
    let (f, r) = pf2::read(&b, &Default::default(), &Default::default()).unwrap();
    assert!(!r.has_loss());
    let cp = f.glyphs().next().unwrap().1.codepoints[0];
    assert_eq!(cp.value(), 0xc0000628);
    assert_eq!(cp.scalar(), None);
    let (out, r) = pf2::write(&f, &Default::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(load(&out), f);
}

#[test]
fn source_property_duplicates_and_spelling_have_loss() {
    let mut f = load(&fixture(3, 3, &[0xaa, 0x80], false));
    let value = f.headers.get("pf2.section.4D415857").unwrap().clone();
    f.headers.push(Property::new("pf2.section.4D415857", value));
    f.headers.push(Property::new(
        "pf2.section.5a5a5a5a",
        PropertyValue::Bytes(vec![7]),
    ));
    let (out, report) = pf2::write(&f, &Default::default()).unwrap();
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code.ends_with("property_order"))
    );
    assert!(pf2::read(&out, &Default::default(), &Default::default()).is_ok());
}
#[test]
fn malformed_source_metric_has_report_not_panic() {
    let mut f = load(&fixture(3, 3, &[0xaa, 0x80], false));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.metrics.bounds.y = i64::MIN;
        Ok(())
    })
    .unwrap();
    f.metrics.descent = None;
    let error = pf2::write(&f, &Default::default()).unwrap_err();
    assert!(
        error
            .report()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.location.glyph_index == Some(id.index()) && e.code.ends_with("omitted"))
    );
}
#[test]
fn retained_properties_are_budgeted_before_serializing() {
    let mut f = load(&fixture(3, 3, &[0xaa, 0x80], false));
    f.properties.push(Property::new(
        "opaque",
        PropertyValue::Bytes(vec![0; 32_000]),
    ));
    let mut options = Pf2WriteOptions::default();
    options.limits.max_total_bytes = 16_000;
    assert!(matches!(
        pf2::write(&f, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn missing_gray_threshold_reports_every_alias_before_failure() {
    let mut f = load(&fixture(3, 3, &[0xaa, 0x80], true));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        let fontpixel::model::GlyphBitmap::Mono(bitmap) = &g.bitmap else {
            unreachable!()
        };
        g.bitmap = bitmap.to_gray8()?.into();
        Ok(())
    })
    .unwrap();
    let error = pf2::write(&f, &Default::default()).unwrap_err();
    for cp in [65, 0x4e00] {
        assert!(error.report().unwrap().entries.iter().any(|entry| {
            entry.code.ends_with("write.omitted")
                && entry.location.glyph_index == Some(id.index())
                && entry.location.codepoint == Some(cp)
        }));
    }
}
