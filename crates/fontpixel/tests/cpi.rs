//! Independent fixtures and property checks for cpi.
#![cfg(feature = "cpi")]
use fontpixel::formats::cpi::{self, CpiReadOptions, CpiWriteOptions, PointerMode, Version};
use fontpixel::{FontSet, ResourceLimits};
use proptest::prelude::*;
fn word(b: &mut Vec<u8>, n: u16) {
    b.extend(n.to_le_bytes());
}
fn dword(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_le_bytes());
}
fn cp(page: u16, height: u8, count: u16) -> Vec<u8> {
    let mut b = Vec::new();
    word(&mut b, 28);
    dword(&mut b, 0);
    word(&mut b, 1);
    b.extend(b"EGA     ");
    word(&mut b, page);
    b.extend([0; 6]);
    dword(&mut b, 0);
    word(&mut b, 1);
    word(&mut b, 1);
    word(&mut b, 6 + u16::from(height) * count);
    b.extend([height, 8, 0, 0]);
    word(&mut b, count);
    for c in 0..count {
        for y in 0..height {
            b.push((c as u8).wrapping_add(y).rotate_left(1));
        }
    }
    b
}
fn fixture(page: u16, height: u8, count: u16) -> FontSet {
    cpi::read(
        &cp(page, height, count),
        &Default::default(),
        &Default::default(),
    )
    .unwrap()
    .0
}
#[test]
fn independent_cp_layout() {
    let b = cp(437, 8, 256);
    let (s, r) = cpi::read(&b, &Default::default(), &Default::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(s.fonts[0].len(), 256);
    assert_eq!(s.fonts[0].charset.code_page, Some(437));
    assert_eq!(
        s.fonts[0].glyphs().nth(1).unwrap().1.bitmap.as_bytes(),
        &[2, 4, 6, 8, 10, 12, 14, 16]
    );
}
#[test]
fn all_uncompressed_versions_multi_page_multi_size() {
    let mut s = fixture(437, 8, 256);
    s.fonts.push(fixture(437, 16, 256).fonts.remove(0));
    s.fonts.push(fixture(850, 8, 256).fonts.remove(0));
    s.fonts.push(fixture(850, 16, 256).fonts.remove(0));
    for version in [Version::Font, Version::FontNt, Version::DrFont] {
        let (b, r) = cpi::write(
            &s,
            &CpiWriteOptions {
                version,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.has_loss(), "{version:?} {r}");
        let (back, r) = cpi::read(&b, &Default::default(), &Default::default()).unwrap();
        assert!(!r.has_loss());
        assert_eq!(s, back);
    }
}
#[test]
fn cp_writer_and_aspect_tail() {
    let mut s = fixture(437, 8, 5);
    s.fonts[0]
        .headers
        .push(fontpixel::model::Property::new("cpi.xaspect", 3i64));
    s.properties.push(fontpixel::model::Property::new(
        "cpi.trailing",
        fontpixel::model::PropertyValue::Bytes(b"generated fixture".to_vec()),
    ));
    let (b, r) = cpi::write(
        &s,
        &CpiWriteOptions {
            version: Version::Cp,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(
        s,
        cpi::read(&b, &Default::default(), &Default::default())
            .unwrap()
            .0
    );
}
#[test]
fn corrupt_chain_dr_indices_and_budget() {
    let s = fixture(437, 8, 256);
    let (mut b, _) = cpi::write(
        &s,
        &CpiWriteOptions {
            version: Version::DrFont,
            ..Default::default()
        },
    )
    .unwrap();
    let start = 31 + 28 + 6 + 6;
    b[start..start + 2].copy_from_slice(&65535u16.to_le_bytes());
    assert!(cpi::read(&b, &Default::default(), &Default::default()).is_err());
    let (b, _) = cpi::write(&s, &Default::default()).unwrap();
    for len in [0, 22, 24, 35, 70, b.len() - 1] {
        assert!(cpi::read(&b[..len], &Default::default(), &Default::default()).is_err());
    }
    let l = ResourceLimits {
        max_glyphs: 255,
        ..Default::default()
    };
    assert!(cpi::read(&b, &Default::default(), &l).is_err());
    assert!(
        cpi::write(
            &s,
            &CpiWriteOptions {
                limits: ResourceLimits {
                    max_total_bytes: 10000,
                    ..Default::default()
                },
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn segmented_pointer_is_explicit() {
    let s = fixture(437, 8, 3);
    let (mut b, _) = cpi::write(&s, &Default::default()).unwrap();
    b[19..23].copy_from_slice(&0x10007u32.to_le_bytes());
    b[49..53].copy_from_slice(&0x30005u32.to_le_bytes());
    assert!(cpi::read(&b, &Default::default(), &Default::default()).is_err());
    assert_eq!(
        s,
        cpi::read(
            &b,
            &CpiReadOptions {
                pointer_mode: PointerMode::Segmented
            },
            &Default::default()
        )
        .unwrap()
        .0
    );
}
proptest! {#[test]fn random_valid_strikes_roundtrip(page in 1u16..65534,height in 1u8..32,count in 1u16..128,version in 0u8..3){let s=fixture(page,height,count);let version=match version{0=>Version::Font,1=>Version::FontNt,_=>Version::Cp};let(b,r)=cpi::write(&s,&CpiWriteOptions{version,..Default::default()}).unwrap();prop_assert!(!r.has_loss(),"{r}");prop_assert_eq!(s,cpi::read(&b,&Default::default(),&Default::default()).unwrap().0);}}

fn adler(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &c in data {
        a = (a + u32::from(c)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
fn literal_cpx(data: &[u8], filter: bool) -> Vec<u8> {
    struct Writer {
        b: Vec<u8>,
        at: usize,
        used: u8,
    }
    impl Writer {
        fn bit(&mut self, one: bool) {
            if self.used == 0 {
                self.at = self.b.len();
                self.b.extend([0; 2]);
            }
            let mut word = u16::from_le_bytes([self.b[self.at], self.b[self.at + 1]]);
            if one {
                word |= 1 << (15 - self.used);
            }
            self.b[self.at..self.at + 2].copy_from_slice(&word.to_le_bytes());
            self.used = (self.used + 1) % 16;
        }
    }
    let mut filtered = data.to_vec();
    if filter {
        let mut at = 0;
        while at + 2 < filtered.len() {
            let opcode = filtered[at];
            at += 1;
            if opcode == 0xe8 || opcode == 0xe9 {
                let value = u16::from_le_bytes([filtered[at], filtered[at + 1]])
                    .wrapping_add((at as u16).wrapping_add(0x100));
                filtered[at..at + 2].copy_from_slice(&value.to_be_bytes());
                at += 2;
            }
        }
    }
    let mut w = Writer {
        b: Vec::new(),
        at: 0,
        used: 0,
    };
    for &byte in &filtered {
        w.bit(true);
        w.b.push(byte);
    }
    w.bit(false);
    let end = 0x0100_0002u32;
    for bit in (0..24).rev() {
        w.bit(end & (1 << bit) != 0);
        w.bit(bit == 0);
    }
    w.b.push(255);
    let mut b = b"UPX!".to_vec();
    b.extend([11, 1, 4, 1]);
    dword(&mut b, adler(&filtered));
    dword(&mut b, adler(&w.b));
    word(&mut b, data.len() as u16);
    word(&mut b, w.b.len() as u16);
    b.push(if filter { 6 } else { 0 });
    b.push((b[4..].iter().map(|&x| u32::from(x)).sum::<u32>() % 251) as u8);
    b.extend(w.b);
    b
}
#[test]
fn native_cpx_literals_filter_checksums_and_limits() {
    let s = fixture(437, 8, 256);
    let (b, _) = cpi::write(&s, &Default::default()).unwrap();
    for filter in [false, true] {
        let packed = literal_cpx(&b, filter);
        let (mut back, r) = cpi::read(&packed, &Default::default(), &Default::default()).unwrap();
        assert!(!r.has_loss());
        back.properties = Default::default();
        assert_eq!(s, back);
        let mut corrupted = packed.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 1;
        assert!(cpi::read(&corrupted, &Default::default(), &Default::default()).is_err());
        let limits = ResourceLimits {
            max_decompressed_bytes: b.len() - 1,
            ..Default::default()
        };
        assert!(cpi::read(&packed, &Default::default(), &limits).is_err());
    }
}
#[test]
fn fatal_export_preserves_mapping_property_report() {
    let mut s = fixture(437, 255, 256);
    s.fonts[0]
        .headers
        .push(fontpixel::model::Property::new("unsupported", "value"));
    let e = cpi::write(
        &s,
        &CpiWriteOptions {
            version: Version::Cpx,
            ..Default::default()
        },
    )
    .unwrap_err();
    let report = e.report().unwrap();
    assert!(report.entries.iter().any(|e| e.code == "cpi.write.mapping"));
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "cpi.write.property")
    );
}

#[test]
fn native_cpx_writer_is_readable_and_repeatable() {
    let source = fixture(437, 16, 256);
    let options = CpiWriteOptions {
        version: Version::Cpx,
        ..Default::default()
    };
    let (packed, report) = cpi::write(&source, &options).unwrap();
    assert_eq!(&packed[..2], &[0x81, 0xfc]);
    assert_eq!(&packed[4..8], &[0x77, 2, 0xcd, 0x20]);
    let at = packed.windows(4).position(|b| b == b"UPX!").unwrap();
    assert!(at < 96);
    assert_eq!(&packed[at + 24..at + 27], b"\xffFO");
    assert!(report.has_loss()); // New CPX container identity is explicit.
    let (restored, report) = cpi::read(&packed, &Default::default(), &options.limits).unwrap();
    assert!(report.entries.is_empty());
    assert_eq!(restored.fonts, source.fonts);
    let (again, report) = cpi::write(&restored, &options).unwrap();
    assert_eq!(again, packed);
    assert!(!report.has_loss(), "{report}");
    assert_eq!(
        cpi::read(&again, &Default::default(), &options.limits)
            .unwrap()
            .0,
        restored
    );
}

#[test]
fn cpx_output_limit_applies_to_packed_bytes() {
    let source = fixture(437, 16, 256);
    let plain = cpi::write(&source, &Default::default()).unwrap().0;
    let (packed, _) = cpi::write(
        &source,
        &CpiWriteOptions {
            version: Version::Cpx,
            max_output_bytes: plain.len() - 1,
            ..Default::default()
        },
    )
    .unwrap();
    let options = CpiWriteOptions {
        version: Version::Cpx,
        max_output_bytes: packed.len() - 1,
        ..Default::default()
    };
    assert!(matches!(
        cpi::write(&source, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
    let options = CpiWriteOptions {
        version: Version::Cpx,
        limits: ResourceLimits {
            max_decompressed_bytes: plain.len() - 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        cpi::write(&source, &options),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn cpx_incompressible_data_keeps_complete_failure_diagnostics() {
    let mut bytes = cp(437, 16, 256);
    let mut state = 0x12345678u32;
    for byte in &mut bytes[40..] {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = state as u8;
    }
    let (mut source, _) = cpi::read(&bytes, &Default::default(), &Default::default()).unwrap();
    source.fonts[0]
        .headers
        .push(fontpixel::model::Property::new(
            "unsupported",
            "kept in failure report",
        ));
    let error = cpi::write(
        &source,
        &CpiWriteOptions {
            version: Version::Cpx,
            ..Default::default()
        },
    )
    .unwrap_err();
    let report = error.report().unwrap();
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.message.contains("not compressible"))
    );
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "cpi.write.property")
    );
    assert_eq!(
        report
            .entries
            .iter()
            .filter(|e| e.code == "cpi.write.mapping")
            .count(),
        256
    );
}

proptest! {
    #[test]
    fn cpx_writer_varied_strikes(page in 1u16..65534, height in 8u8..32, count in 32u16..256) {
        let source = fixture(page, height, count);
        let options = CpiWriteOptions { version: Version::Cpx, ..Default::default() };
        let (packed, _) = cpi::write(&source, &options).unwrap();
        let (restored, report) = cpi::read(&packed, &Default::default(), &options.limits).unwrap();
        prop_assert!(report.entries.is_empty());
        prop_assert_eq!(&restored.fonts, &source.fonts);
        let (again, report) = cpi::write(&restored, &options).unwrap();
        prop_assert!(!report.has_loss(), "{report}");
        prop_assert_eq!(packed, again);
    }
}
#[test]
fn independent_drfont_index_table_reorders_shared_bitmaps() {
    let mut b = b"\x7fDRFONT ".to_vec();
    b.extend([0; 8]);
    word(&mut b, 1);
    b.push(1);
    dword(&mut b, 29);
    b.extend([1, 8]);
    dword(&mut b, 583);
    word(&mut b, 1);
    let mut body = cp(437, 8, 256);
    body[24..28].copy_from_slice(&59u32.to_le_bytes());
    body[28..34].copy_from_slice(&[2, 0, 1, 0, 6, 0]);
    b.extend(&body[..40]);
    for i in (0u16..256).rev() {
        word(&mut b, i);
    }
    b.extend(&body[40..]);
    let (s, r) = cpi::read(&b, &Default::default(), &Default::default()).unwrap();
    assert!(!r.has_loss());
    let f = &s.fonts[0];
    assert_eq!(
        f.glyphs().next().unwrap().1.bitmap.as_bytes(),
        &[255u8.rotate_left(1), 0, 2, 4, 6, 8, 10, 12]
    );
    assert_eq!(
        f.glyphs().last().unwrap().1.bitmap.as_bytes(),
        &[0, 2, 4, 6, 8, 10, 12, 14]
    );
}
#[test]
fn sparse_cell_plan_is_budgeted_before_allocating() {
    let mut s = fixture(437, 8, 1);
    let id = s.fonts[0].glyphs().next().unwrap().0;
    let mut g = s.fonts[0].glyph(id).unwrap().clone();
    g.codepoints = vec![fontpixel::model::Codepoint::Legacy(60000)];
    s.fonts[0].replace_glyph(id, g).unwrap();
    let e = cpi::write(
        &s,
        &CpiWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: 512 * 1024,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(e, fontpixel::Error::ResourceLimit { .. }));
}
#[test]
fn cpi_diagnostic_asset_names_are_in_the_read_budget() {
    let mut bytes = cp(437, 1, 1);
    bytes[28] = 0;
    let limits = ResourceLimits {
        max_total_bytes: 16384,
        ..Default::default()
    };
    assert_eq!(
        cpi::read(&bytes, &Default::default(), &limits)
            .unwrap()
            .1
            .entries
            .len(),
        1
    );
    let name = "x".repeat(32768);
    let input = fontpixel::Input::new(&bytes).with_name(&name);
    let e = fontpixel::registry()
        .read(
            &input,
            &fontpixel::formats::ReadOptions {
                format: Some("cpi".into()),
                limits,
            },
        )
        .unwrap_err();
    assert!(matches!(e, fontpixel::Error::ResourceLimit { .. }));
}
