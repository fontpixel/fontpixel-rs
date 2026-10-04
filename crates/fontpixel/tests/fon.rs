//! Windows resource detection versus native OS/2 and damaged executable headers.
#![cfg(feature = "fon")]
use fontpixel::formats::{Confidence, FormatDetector, fnt, fon};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::resources::ReadContext;
use fontpixel::{Bitmap, Font, FontSet, Glyph, Input, ReadOptions, ResourceLimits};
fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn source() -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("Original detector fixture".into());
    f.metrics.ascent = Some(3);
    for cp in [65, 66, 67] {
        let mut g = Glyph::new("#......#\n.##..##.\n...##...".parse::<Bitmap>().unwrap());
        g.codepoints.push(Codepoint::Legacy(cp));
        g.metrics.advance = Some(Vector::new(8, 0));
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn payload() -> Vec<u8> {
    fnt::write(&source().fonts[0], &Default::default())
        .unwrap()
        .0
}
// Independently assembled resource tables; glyph payload belongs to this test.
fn ne(mz: bool) -> Vec<u8> {
    let payload = payload();
    let base = if mz { 64 } else { 0 };
    let data = base + 96;
    let mut b = vec![0; data + payload.len()];
    if mz {
        b[..2].copy_from_slice(b"MZ");
        put32(&mut b, 60, base as u32);
    }
    b[base..base + 2].copy_from_slice(b"NE");
    put16(&mut b, base + 36, 64);
    put16(&mut b, base + 38, 88);
    b[base + 54] = 2;
    let table = base + 64;
    put16(&mut b, table + 2, 0x8008);
    put16(&mut b, table + 4, 1);
    put16(&mut b, table + 10, data as u16);
    put16(&mut b, table + 12, payload.len() as u16);
    put16(&mut b, table + 16, 0x8001);
    b[data..].copy_from_slice(&payload);
    b
}
fn pe(plus: bool) -> Vec<u8> {
    let payload = payload();
    let mut b = vec![0; 1024 + payload.len()];
    b[..2].copy_from_slice(b"MZ");
    put32(&mut b, 60, 64);
    b[64..68].copy_from_slice(b"PE\0\0");
    put16(&mut b, 70, 1);
    let opt = if plus { 240 } else { 224 };
    put16(&mut b, 84, opt);
    let o = 88;
    put16(&mut b, o, if plus { 0x20b } else { 0x10b });
    let dirs = if plus { 112 } else { 96 };
    put32(&mut b, o + dirs - 4, 16);
    put32(&mut b, o + dirs + 16, 0x3000);
    put32(&mut b, o + dirs + 20, 88);
    let section = o + usize::from(opt);
    let size = (b.len() - 512) as u32;
    put32(&mut b, section + 8, size);
    put32(&mut b, section + 12, 0x3000);
    put32(&mut b, section + 16, size);
    put32(&mut b, section + 20, 512);
    for (directory, id, next) in [(0, 8, 0x80000018), (24, 1, 0x80000030), (48, 0, 72)] {
        put16(&mut b, 512 + directory + 14, 1);
        put32(&mut b, 512 + directory + 16, id);
        put32(&mut b, 512 + directory + 20, next);
    }
    put32(&mut b, 584, 0x3200);
    put32(&mut b, 588, payload.len() as u32);
    b[1024..].copy_from_slice(&payload);
    b
}
#[test]
fn genuine_ne_pe32_pe64_resources_are_certain_and_readable() {
    let mut bare_pe = pe(false);
    bare_pe.copy_within(64..64 + 24 + 224 + 40, 0);
    for b in [ne(false), ne(true), pe(false), pe(true), bare_pe] {
        assert_eq!(fon::detect(&b), Confidence::Certain);
        assert_eq!(
            fon::read(&b, &Default::default(), &Default::default())
                .unwrap()
                .0
                .fonts[0]
                .len(),
            3
        );
        let result = fontpixel::registry()
            .read(
                &Input::new(&b).with_name("misleading.gpi"),
                &ReadOptions::default(),
            )
            .unwrap();
        assert_eq!(result.format, "fon");
    }
    let written = fon::write(&source(), &Default::default()).unwrap().0;
    assert_eq!(fon::detect(&written), Confidence::Certain);
}
#[test]
fn bare_signatures_missing_fonts_and_broken_offsets_do_not_claim_fon() {
    for b in [&b"NE"[..], &b"PE\0\0"[..], &b"MZ"[..]] {
        assert_eq!(fon::detect(b), Confidence::NoMatch);
    }
    let original = ne(true);
    for mutation in 0..7 {
        let mut b = original.clone();
        match mutation {
            0 => put32(&mut b, 60, u32::MAX),
            1 => put16(&mut b, 100, u16::MAX),
            2 => put16(&mut b, 102, 64),
            3 => put16(&mut b, 128, 17),
            4 => put16(&mut b, 132, u16::MAX),
            5 => put16(&mut b, 130, 0x8007),
            _ => put16(&mut b, 138, u16::MAX),
        }
        assert_eq!(fon::detect(&b), Confidence::NoMatch, "mutation {mutation}");
    }
    for plus in [false, true] {
        let mut b = pe(plus);
        put32(&mut b, 528, 7);
        assert_eq!(fon::detect(&b), Confidence::NoMatch);
        let mut b = pe(plus);
        put32(&mut b, 532, 0x80000000);
        assert_eq!(fon::detect(&b), Confidence::NoMatch);
        let mut b = pe(plus);
        put32(&mut b, 584, u32::MAX);
        assert_eq!(fon::detect(&b), Confidence::NoMatch);
    }
}
#[test]
fn detector_honors_resource_table_and_retained_memory_budgets() {
    let b = ne(true);
    for kind in 0..4 {
        let mut limits = ResourceLimits::default();
        match kind {
            0 => limits.max_table_bytes = 1,
            1 => limits.max_resources = 0,
            2 => limits.max_resource_bytes = 1,
            _ => limits.max_total_bytes = b.len(),
        }
        let input = Input::new(&b);
        let mut c = ReadContext::new(&input, &limits).unwrap();
        assert_eq!(
            fon::FonCodec::default().detect(&mut c).unwrap(),
            Confidence::NoMatch
        );
    }
    // Every short prefix is bounded; an exact payload end is the first valid one.
    for end in 0..b.len() {
        assert_eq!(fon::detect(&b[..end]), Confidence::NoMatch);
    }
}
#[cfg(feature = "gpi")]
#[test]
fn native_os2_is_gpi_and_windows_fon_is_not_given_to_gpi() {
    use fontpixel::formats::gpi::{self, GpiContainer, GpiWriteOptions};
    let bytes = gpi::write(
        &source(),
        &fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("font").unwrap()),
        &GpiWriteOptions {
            container: Some(GpiContainer::Ne),
            ..Default::default()
        },
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    assert_eq!(fon::detect(&bytes), Confidence::NoMatch);
    let r = fontpixel::registry()
        .read(
            &Input::new(&bytes).with_name("misleading.fon"),
            &ReadOptions::default(),
        )
        .unwrap();
    assert_eq!(r.format, "gpi");
    assert_eq!(r.fonts.fonts[0].len(), 3);
    let windows = ne(true);
    let input = Input::new(&windows);
    let limits = ResourceLimits::default();
    assert_eq!(
        gpi::GpiCodec::default()
            .detect(&mut ReadContext::new(&input, &limits).unwrap())
            .unwrap(),
        Confidence::NoMatch
    );
    // A contradictory OS/2 target byte with valid Windows FONT resources stays
    // Possible, allowing the independent GPI Possible match to be ambiguous.
    let mut mixed = windows;
    mixed[64 + 54] = 1;
    assert_eq!(fon::detect(&mixed), Confidence::Possible);
    assert!(fon::read(&mixed, &Default::default(), &limits).is_ok());
    assert!(matches!(
        fontpixel::registry().detect(&Input::new(&mixed), &limits),
        Err(fontpixel::Error::AmbiguousFormat { .. })
    ));
}
