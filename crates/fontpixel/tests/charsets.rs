//! Exact charset regression tests and CPython-generated mapping digests.
use fontpixel::charset::{
    CharsetCodec, CharsetError, Kind, WindowsCharset, canonical_charset, charset_props,
    charset_table, detect_gl, euc_to_kuten, is_unicode_charset, kuten_to_euc, sanitize_charset,
    windows_charset,
};
use serde_json::Value;

fn text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => panic!("unexpected canonical fixture value"),
    }
}

#[test]
fn canonicalization_matches_reference_fixture() {
    let fixture: Value = serde_json::from_str(include_str!("data/charset_canonical.json")).unwrap();
    for row in fixture["names"].as_array().unwrap() {
        let name = row[0].as_str().unwrap();
        assert_eq!(
            sanitize_charset(Some(name), None),
            row[1],
            "sanitize {name}"
        );
        assert_eq!(canonical_charset(name), row[2], "canonical {name}");
    }
    for row in fixture["pairs"].as_array().unwrap() {
        let registry = text(&row[0]);
        let encoding = text(&row[1]);
        assert_eq!(
            sanitize_charset(registry.as_deref(), encoding.as_deref()),
            row[2],
            "{row}"
        );
    }
    for row in fixture["props"].as_array().unwrap() {
        let (reg, enc) = charset_props(row[0].as_str().unwrap());
        assert_eq!(reg, row[1][0], "{row}");
        assert_eq!(enc, row[1][1], "{row}");
    }
}

#[test]
fn names_are_not_approximated_or_parsed_with_integer_overflow() {
    assert_eq!(
        canonical_charset("ISO8859-000000000000000000000000000000000001"),
        "iso88591"
    );
    let huge = "9".repeat(1000);
    assert_eq!(
        sanitize_charset(Some("ISO8859"), Some(&huge)),
        format!("iso8859{huge}")
    );
    assert_eq!(canonical_charset("\u{85}ASCII\u{85}"), "\u{85}ascii\u{85}");
    assert_eq!(canonical_charset("ISO8859-١"), "iso88591");
    for name in ["HZ", "iso2022jp", "FontSpecific", "KOI8-RU"] {
        let codec = CharsetCodec::new(name, None).unwrap();
        assert!(!codec.known());
        assert_eq!(codec.cp2unicode(0x1f600), Some(0x1f600));
    }
}

#[test]
fn all_registry_keys_are_present_with_honest_feature_availability() {
    assert_eq!(charset_table().len(), 62);
    for entry in charset_table() {
        let result = CharsetCodec::from_key(entry.key, None);
        let cjk = matches!(entry.kind, Kind::DoubleByte | Kind::Gb18030);
        if cjk && !cfg!(feature = "cjk-charsets") {
            assert!(!entry.available());
            assert_eq!(
                result.unwrap_err(),
                CharsetError::FeatureDisabled(entry.key.into())
            );
        } else {
            assert!(entry.available());
            let codec = result.unwrap();
            assert!(codec.known());
            assert_eq!(codec.kind(), entry.kind);
            assert_eq!(codec.gl(), entry.force_gl);
            assert_eq!(codec.cp2char(65), Some('A'));
        }
    }
}

fn digest(pairs: impl IntoIterator<Item = (u32, u32)>) -> (usize, String) {
    let mut hash = 0xcbf29ce484222325u64;
    let mut count = 0;
    for (key, value) in pairs {
        for byte in key.to_le_bytes().into_iter().chain(value.to_le_bytes()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        count += 1;
    }
    (count, format!("{hash:016x}"))
}

fn compare_digest(pairs: impl IntoIterator<Item = (u32, u32)>, expected: &Value, context: &str) {
    let (count, hash) = digest(pairs);
    assert_eq!(
        count as u64,
        expected["count"].as_u64().unwrap(),
        "{context}"
    );
    assert_eq!(hash, expected["fnv1a64"].as_str().unwrap(), "{context}");
}

#[test]
fn every_compiled_table_matches_python_generated_exhaustive_digests() {
    let fixture: Value = serde_json::from_str(include_str!("data/charset_maps.json")).unwrap();
    for (name, expected) in fixture.as_object().unwrap() {
        if name == "gb18030/4" {
            continue;
        }
        let (key, gl) = name
            .strip_suffix("/gl")
            .map_or((name.as_str(), false), |key| (key, true));
        let codec = match CharsetCodec::from_key(key, Some(gl)) {
            Ok(codec) => codec,
            Err(CharsetError::FeatureDisabled(_)) => {
                assert!(
                    !charset_table()
                        .iter()
                        .find(|e| e.key == key)
                        .unwrap()
                        .available()
                );
                continue;
            }
        };
        compare_digest(
            (0..=0xffff).filter_map(|code| codec.cp2unicode(code).map(|u| (code, u))),
            &expected["decode"],
            name,
        );
        compare_digest(
            (0..=0x10ffff).filter_map(|u| {
                codec
                    .unicode2cp(u)
                    .filter(|&code| code <= 0xffff)
                    .map(|code| (u, code))
            }),
            &expected["encode"],
            name,
        );
    }
}

#[test]
fn identity_preserves_reference_numeric_surrogates_but_rust_char_rejects_them() {
    for key in ["unicode", "utf16be", "ucs2anything", "unknown"] {
        let codec = CharsetCodec::from_key(key, None).unwrap();
        for code in [0, 65, 0xd800, 0xdfff, 0x10000, 0x10ffff] {
            assert_eq!(codec.cp2unicode(code), Some(code));
            assert_eq!(codec.unicode2cp(code), Some(code));
        }
        for code in [0xd800, 0xdfff, 0x110000, u32::MAX] {
            assert_eq!(codec.cp2char(code), None);
        }
        assert_eq!(codec.unicode2cp(0x110000), None);
        assert_eq!(codec.cp2unicode(u32::MAX), None);
    }
    assert!(is_unicode_charset("utf32le"));
    assert!(!is_unicode_charset("utf9"));
}

#[test]
fn single_byte_strictness_and_exact_variants() {
    let latin = CharsetCodec::new("ISO-8859-1", None).unwrap();
    let windows = CharsetCodec::new("cp1252", None).unwrap();
    assert_eq!(latin.cp2char(0x80), Some('\u{80}'));
    assert_eq!(windows.cp2char(0x80), Some('€'));
    assert_eq!(windows.cp2char(0x81), None);
    assert_eq!(windows.char2cp('\u{81}'), None);
    assert_eq!(latin.cp2char(256), None);
    assert_eq!(windows.cp2char(0x4142), None);
    let jis = CharsetCodec::new("jisx0201", Some(true)).unwrap();
    assert!(!jis.gl());
    assert_eq!(jis.char2cp('¥'), None);
    assert_eq!(jis.char2cp('‾'), None);
    assert_eq!(jis.cp2char(0x5c), Some('\\'));
    assert_eq!(jis.cp2char(0xa1), Some('｡'));
    assert_eq!(jis.cp2char(0xe0), None);
}

#[test]
fn gl_detection_is_order_independent_and_only_applies_to_94_by_94_sets() {
    assert!(detect_gl("gb2312", None, [0xc4e3, 65, 0x3021]));
    assert!(!detect_gl("gb2312", None, [0x0100, 0x3021]));
    assert!(detect_gl("gb2312", Some("0"), []));
    assert!(!detect_gl("gbk", Some("0"), [0x3021]));
    assert!(detect_gl("iso2022cn", None, []));
    assert!(!detect_gl("unknown", Some("0"), [0x3021]));
}

#[test]
fn windows_charset_metadata_does_not_guess_contextual_or_symbol_maps() {
    use WindowsCharset::*;
    assert_eq!(windows_charset(0), CodePage("cp1252"));
    assert_eq!(windows_charset(1), SystemAnsi);
    assert_eq!(windows_charset(2), Symbol);
    assert_eq!(windows_charset(77), SystemMac);
    assert_eq!(windows_charset(255), SystemOem);
    assert_eq!(windows_charset(128), CodePage("cp932"));
    assert_eq!(windows_charset(134), CodePage("cp936"));
    assert_eq!(windows_charset(129), CodePage("cp949"));
    assert_eq!(windows_charset(136), CodePage("cp950"));
    assert_eq!(windows_charset(130), CodePage("johab"));
    assert_eq!(windows_charset(222), CodePage("cp874"));
    assert_eq!(windows_charset(42), Unknown(42));
    for byte in 0..=255 {
        if let CodePage(key) = windows_charset(byte) {
            assert!(charset_table().iter().any(|entry| entry.key == key));
        }
    }
}

#[test]
fn hzk_hbf_grid_round_trips_every_cell() {
    for row in 1..=94 {
        for cell in 1..=94 {
            assert_eq!(
                euc_to_kuten(kuten_to_euc(row, cell).unwrap()),
                Some((row, cell))
            );
        }
    }
    assert_eq!(kuten_to_euc(16, 1), Some(0xb0a1));
    assert_eq!(kuten_to_euc(0, 1), None);
    assert_eq!(kuten_to_euc(1, 95), None);
    assert_eq!(euc_to_kuten(0x3021), None);
}

#[cfg(feature = "cjk-charsets")]
#[test]
fn strict_cjk_edge_cases_and_encoder_preferences() {
    let sjis = CharsetCodec::new("shiftjis", None).unwrap();
    let cp932 = CharsetCodec::new("cp932", None).unwrap();
    assert_eq!(sjis.cp2char(0x8160), Some('〜'));
    assert_eq!(cp932.cp2char(0x8160), Some('～'));
    assert_eq!(sjis.char2cp('¥'), None);
    assert_eq!(cp932.char2cp('〜'), None);
    assert_eq!(cp932.char2cp('ⅰ'), Some(0xeeef));
    assert_eq!(cp932.cp2char(0xfa40), Some('ⅰ'));
    let hkscs = CharsetCodec::new("big5hkscs", None).unwrap();
    assert_eq!(hkscs.cp2unicode(0x8862), None); // two Unicode characters
    assert_eq!(hkscs.cp2unicode(0x8745), Some(0x27267));
    let euc = CharsetCodec::new("eucjp", None).unwrap();
    assert_eq!(euc.cp2char(0x8fa2c2), None); // JIS X 0212 needs three bytes
    assert_eq!(euc.char2cp('¡'), None);
    assert_eq!(euc.cp2char(0x4142), None); // two ASCII characters
    let gl = CharsetCodec::new("gb2312", Some(true)).unwrap();
    assert_eq!(gl.cp2char(0x3021), Some('啊'));
    assert_eq!(gl.char2cp('啊'), Some(0x3021));
    assert_eq!(gl.cp2char(0xb0a1), None);
    let jis_gl = CharsetCodec::new("jisx0208", Some(true)).unwrap();
    assert_eq!(jis_gl.char2cp('｡'), None); // SS2 is outside GL
    let iso = CharsetCodec::new("iso2022cn", None).unwrap();
    assert_eq!(iso.cp2char(0x4463), Some('你'));
    assert_eq!(iso.cp2unicode(0x1b244129), None); // not a stateful stream API
    assert_eq!(
        CharsetCodec::new("iso2022cn", Some(false))
            .unwrap()
            .cp2char(0xc4e3),
        Some('你')
    );
}

#[cfg(feature = "cjk-charsets")]
#[test]
fn all_gb18030_four_byte_sequences_and_unicode_reverse_mappings() {
    let codec = CharsetCodec::new("gb18030", None).unwrap();
    let fixture: Value = serde_json::from_str(include_str!("data/charset_maps.json")).unwrap();
    let codes = (0u32..126 * 10 * 126 * 10).map(|index| {
        let d = index % 10 + 0x30;
        let c = (index / 10) % 126 + 0x81;
        let b = (index / (10 * 126)) % 10 + 0x30;
        let a = index / (10 * 126 * 10) + 0x81;
        (a << 24) | (b << 16) | (c << 8) | d
    });
    compare_digest(
        codes.filter_map(|code| codec.cp2unicode(code).map(|u| (code, u))),
        &fixture["gb18030/4"]["decode"],
        "GB18030 4-byte decode",
    );
    compare_digest(
        (0..=0x10ffff).filter_map(|u| {
            codec
                .unicode2cp(u)
                .filter(|&code| code > 0xffff)
                .map(|code| (u, code))
        }),
        &fixture["gb18030/4"]["encode"],
        "GB18030 4-byte encode",
    );
    assert_eq!(codec.cp2char(0x81308130), Some('\u{80}'));
    assert_eq!(codec.cp2char(0x90308130), Some('\u{10000}'));
    assert_eq!(codec.char2cp('\u{10ffff}'), Some(0xe3329a35));
    for invalid in [
        0x80308130,
        0x8130812f,
        0x813a8130,
        0x8130ff30,
        0x85308130,
        0xe339fe39,
        u32::MAX,
    ] {
        assert_eq!(codec.cp2unicode(invalid), None, "{invalid:x}");
    }
}
