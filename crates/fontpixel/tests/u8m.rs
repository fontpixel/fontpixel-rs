//! Byte-authored U8/M maps, page boundaries, aliases and continuous rows.
#![cfg(feature = "u8m")]
use fontpixel::formats::u8m::{self, U8mWriteOptions};
use fontpixel::model::Codepoint;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn p16(b: &mut [u8], p: usize, n: u16) {
    b[p..p + 2].copy_from_slice(&n.to_le_bytes());
}
fn p32(b: &mut [u8], p: usize, n: u32) {
    b[p..p + 4].copy_from_slice(&n.to_le_bytes());
}
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 780];
    b[..4].copy_from_slice(b"U8/M");
    b[4] = 3;
    b[5..8].copy_from_slice(b"MIT");
    p16(&mut b, 124, 42);
    b[126] = 1;
    b[127] = 8;
    p16(&mut b, 128, 2);
    p16(&mut b, 130, 3);
    p16(&mut b, 132, 1);
    p16(&mut b, 134, 4);
    p16(&mut b, 138, 1);
    p16(&mut b, 146, 1);
    p16(&mut b, 240, 2);
    b[252..256].copy_from_slice(&[6, 2, 1, 9]);
    for (i, at) in [(1, 272), (2, 276), (3, 280)] {
        p32(&mut b, 256 + i * 4, at as u32 | 1 << 24);
    }
    b[272..284].copy_from_slice(&[1, 1, 1, 0, 32, 32, 3, 0, 0, 0, 1, 0]);
    p32(&mut b, 512, 4 << 24);
    p32(&mut b, 516, 768 | 10 << 24);
    p32(&mut b, 520, 775 | 4 << 24);
    b[768..775].copy_from_slice(&[253, 255, 2, 9, 0xaa, 0xa9, 0x40]);
    b[775..780].copy_from_slice(&[254, 2, 1, 2, 0x80]);
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    u8m::read(&mut ReadContext::new(&i, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.u8m").unwrap())
}
#[test]
fn native_unicode_astral_aliases_and_unencoded() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 3);
    let g = f.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert_eq!(
        g.codepoints,
        [
            Codepoint::Unicode(65),
            Codepoint::Unicode(0x20001),
            Codepoint::Legacy(65)
        ]
    );
    assert_eq!(g.bitmap.as_bytes(), [0xaa, 0x80, 0x52, 0x80]);
    assert_eq!((g.metrics.bounds.x, g.metrics.bounds.y), (-1, 1));
    let mut o = options();
    o.strict = true;
    let w = u8m::write(&s, &o, &Default::default()).unwrap();
    assert_eq!(
        read(&w.output.into_single().unwrap(), &Default::default()).unwrap(),
        s
    );
}
#[test]
fn prefixed_load_address_uses_font_relative_offsets() {
    let mut b = vec![0, 0x80];
    b.extend(fixture());
    let s = read(&b, &Default::default()).unwrap();
    let mut o = options();
    o.strict = true;
    let out = u8m::write(&s, &o, &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(&out[..6], b"\0\x80U8/M");
    assert_eq!(read(&out, &Default::default()).unwrap(), s);
}
#[test]
fn malformed_maps_pages_and_same_limits() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
    for (at, x) in [
        (272, 2),
        (273, 64),
        (274, 99),
        (256 + 4 + 3, 65),
        (768 + 2, 255),
    ] {
        let mut bad = b.clone();
        bad[at] = x;
        assert!(read(&bad, &Default::default()).is_err(), "{at}");
    }
    let mut bad = b.clone();
    p32(&mut bad, 516, 1019 | 10 << 24);
    assert!(read(&bad, &Default::default()).is_err());
    let s = read(&b, &Default::default()).unwrap();
    for total in [0, 8192, 32000, 128000] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(w) = u8m::write(
            &s,
            &options(),
            &U8mWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &l).is_ok());
        }
    }
}

#[test]
fn registry_dispatch_and_strict_metadata_loss() {
    let bytes = fixture();
    let registry = fontpixel::registry();
    let mut loaded = registry
        .read(
            &Input::new(&bytes),
            &fontpixel::ReadOptions {
                format: Some("u8m".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(loaded.fonts, read(&bytes, &Default::default()).unwrap());
    loaded.fonts.fonts[0]
        .properties
        .push(fontpixel::model::Property::new(
            "e1.unrepresentable",
            123i64,
        ));
    let mut options = options();
    let output = registry.write(&loaded.fonts, "u8m", &options).unwrap();
    assert!(output.report.has_loss());
    options.strict = true;
    assert!(registry.write(&loaded.fonts, "u8m", &options).is_err());
}
