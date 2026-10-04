//! Original field-authored VLW coverage, modified UTF and native versions.
#![cfg(feature = "vlw")]
use fontpixel::formats::vlw::{self, VlwVersion, VlwWriteOptions};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn fixture(version: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for n in [3, version, 9, 123, 7, 2] {
        out.extend(n.to_be_bytes())
    }
    for (cp, w, h, adv, top, left, pointer) in [
        (65, 3, 2, 5, 4, -2, 456),
        (256, 2, 3, 4, 6, 1, 0),
        (0x20001, 0, 0, 7, 0, 0, 0),
    ] {
        for n in [cp, h, w, adv, top, left, pointer] {
            out.extend(i32::to_be_bytes(n))
        }
    }
    out.extend([0, 17, 255, 64, 128, 240, 255, 0, 1, 254, 128, 0]);
    if version >= 10 {
        let name = b"Original\xc0\x80\xed\xa0\xbd\xed\xb9\x82";
        out.extend((name.len() as u16).to_be_bytes());
        out.extend(name);
        out.extend([0, 10]);
        out.extend(b"OriginalPS");
    }
    if version == 11 {
        out.push(1)
    }
    out
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    vlw::read(&mut ReadContext::new(&i, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("original.vlw").unwrap())
}
#[test]
fn original_versions_gray_metrics_and_java_strings() {
    for (ver, version) in [
        (8, VlwVersion::V8),
        (9, VlwVersion::V9),
        (10, VlwVersion::V10),
        (11, VlwVersion::V11),
    ] {
        let bytes = fixture(ver);
        let s = read(&bytes, &Default::default()).unwrap();
        let f = s.single().unwrap();
        assert_eq!(f.len(), 3);
        let g = f
            .glyph_for_codepoint(fontpixel::model::Codepoint::Unicode(65))
            .unwrap();
        assert_eq!((g.metrics.bounds.x, g.metrics.bounds.y), (-2, 2));
        assert_eq!(g.bitmap.as_bytes(), [0, 17, 255, 64, 128, 240]);
        if ver >= 10 {
            assert_eq!(f.names.full.as_deref(), Some("Original\0🙂"));
        }
        let mut o = options();
        o.strict = true;
        let out = vlw::write(
            &s,
            &o,
            &VlwWriteOptions {
                version,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!out.report.has_loss());
        assert_eq!(out.output.into_single().unwrap(), bytes);
    }
}
#[test]
fn malformed_directories_strings_and_all_truncations() {
    let b = fixture(11);
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
    for (at, value) in [(4, 7u32), (24, 0xd800), (24 + 28, 65), (24 + 4, u32::MAX)] {
        let mut bad = b.clone();
        bad[at..at + 4].copy_from_slice(&value.to_be_bytes());
        assert!(read(&bad, &Default::default()).is_err());
    }
    let mut bad = b.clone();
    *bad.last_mut().unwrap() = 2;
    assert!(read(&bad, &Default::default()).is_err());
    let mut bad = b;
    let at = 24 + 3 * 28 + 12 + 2 + 8;
    bad[at] = 0;
    assert!(read(&bad, &Default::default()).is_err());
}
#[test]
fn limits_metadata_loss_and_native_registry() {
    let s = read(&fixture(11), &Default::default()).unwrap();
    for cap in [0, 64, 512, 4096, 65536] {
        let l = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(out) = vlw::write(
            &s,
            &options(),
            &VlwWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&out.output.into_single().unwrap(), &l).is_ok());
        }
    }
    let r = fontpixel::registry();
    let mut o = options();
    o.strict = true;
    assert!(r.write(&s, "vlw", &o).is_ok());
    let mut source = s;
    source.fonts[0].names.copyright = Some("cannot be stored".into());
    assert!(r.write(&source, "vlw", &o).is_err());
    o.strict = false;
    assert!(r.write(&source, "vlw", &o).unwrap().report.has_loss());
}
