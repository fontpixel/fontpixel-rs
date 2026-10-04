//! Original Kbits wire cases and property checks.
#![cfg(feature = "kbits")]
use fontpixel::bitmap::{BitmapLimits, Gray8};
use fontpixel::formats::kbits::{self, KbitsWriteOptions};
use fontpixel::model::{Codepoint, Property, Vector};
use fontpixel::{Bitmap, Font, Glyph, ResourceLimits};
fn fixture() -> Vec<u8> {
    let mut b = b"KBnPbits".to_vec();
    for n in [1i32, 9, 3, 10, 4, 2, 5] {
        b.extend_from_slice(&n.to_be_bytes());
    }
    b.extend_from_slice(b"name");
    for n in [1i32, 1] {
        b.extend_from_slice(&n.to_be_bytes());
    }
    b.extend_from_slice(&7u16.to_be_bytes());
    b.extend_from_slice(b"A\xc0\x80\xed\xa0\xbd\xed");
    b.truncate(b.len() - 7);
    b.extend_from_slice(b"Example");
    b.extend_from_slice(b"char");
    for n in [1i32, 65, 7, -2, 4, 2, 3] {
        b.extend_from_slice(&n.to_be_bytes());
    }
    b.extend_from_slice(&[0, 127, 255]);
    b.extend_from_slice(&3i32.to_be_bytes());
    b.extend_from_slice(&[1, 128, 254]);
    b.extend_from_slice(b"fin.");
    b
}
#[test]
fn native_gray_and_metrics_roundtrip() {
    let b = fixture();
    let (f, r) = kbits::read(&b, &ResourceLimits::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(f.names.family.as_deref(), Some("Example"));
    let g = f.glyphs().next().unwrap().1;
    assert_eq!(g.metrics.bounds.x, -2);
    assert_eq!(g.metrics.bounds.y, 2);
    assert_eq!(g.bitmap.as_bytes(), [0, 127, 255, 1, 128, 254]);
    let (out, r) = kbits::write(&f, &KbitsWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(b, out);
}
#[test]
fn failed_mapping_retains_source_aliases() {
    let (mut f, _) = kbits::read(&fixture(), &ResourceLimits::default()).unwrap();
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.codepoints = vec![Codepoint::Legacy(65), Codepoint::Unicode(66)];
        Ok(())
    })
    .unwrap();
    let error = kbits::write(&f, &KbitsWriteOptions::default()).unwrap_err();
    let r = error.report().unwrap();
    assert!(r.has_errors());
    let codes: Vec<_> = r
        .entries
        .iter()
        .filter(|e| e.code == "kbits.glyph_omitted")
        .map(|e| e.location.codepoint)
        .collect();
    assert_eq!(codes, [Some(65), Some(66)]);
}
#[test]
fn java_modified_utf8_nul_and_supplementary() {
    let mut b = fixture();
    let begin = 50;
    assert_eq!(&b[begin..begin + 7], b"Example");
    b.splice(
        begin..begin + 7,
        b"A\xc0\x80\xed\xa0\xbd\xed\xb8\x80".iter().copied(),
    );
    b[48..50].copy_from_slice(&9u16.to_be_bytes());
    let (f, _) = kbits::read(&b, &ResourceLimits::default()).unwrap();
    assert_eq!(f.names.family.as_deref(), Some("A\0😀"));
    let (out, r) = kbits::write(&f, &KbitsWriteOptions::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(out, b);
}
#[test]
fn every_truncated_prefix_fails() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(
            kbits::read(&b[..n], &ResourceLimits::default()).is_err(),
            "prefix {n}"
        );
    }
}
#[test]
fn counts_and_utf8_are_bounded() {
    let mut b = fixture();
    b[50] = 0;
    assert!(kbits::read(&b, &ResourceLimits::default()).is_err());
    let mut l = ResourceLimits {
        max_glyphs: 0,
        ..Default::default()
    };
    assert!(kbits::read(&fixture(), &l).is_err());
    l = ResourceLimits::default();
    l.max_pixels_per_glyph = 5;
    assert!(kbits::read(&fixture(), &l).is_err());
}
#[test]
fn lost_metadata_and_aliases_are_reported() {
    let (mut f, _) = kbits::read(&fixture(), &ResourceLimits::default()).unwrap();
    f.names.xlfd = Some("extra".into());
    f.metrics.underline_position = Some(-1);
    f.properties.push(Property::new("unknown", "abc"));
    let id = f.glyphs().next().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(66));
        g.name = Some("A".into());
        Ok(())
    })
    .unwrap();
    let (_, r) = kbits::write(&f, &KbitsWriteOptions::default()).unwrap();
    for code in [
        "font_names",
        "font_metrics",
        "properties",
        "codepoints",
        "glyph_name",
    ] {
        assert!(r.entries.iter().any(|e| e.code.ends_with(code)), "{r}");
    }
}
proptest::proptest! {#[test]fn random_gray_native_roundtrip(w in 1u32..20,h in 1u32..20,data in proptest::collection::vec(proptest::prelude::any::<u8>(),400),x in -20i64..20,y in -20i64..20){let mut f=Font::default();let b=Bitmap::<Gray8>::from_raw(w,h,&data[..(w*h)as usize],BitmapLimits::default()).unwrap();let mut g=Glyph::new(b);g.codepoints.push(Codepoint::Unicode(65));g.metrics.bounds.x=x;g.metrics.bounds.y=y;g.metrics.advance=Some(Vector::new(i64::from(w)+1,0));f.add_glyph(g).unwrap();let(a,_)=kbits::write(&f,&KbitsWriteOptions::default()).unwrap();let(f,_)=kbits::read(&a,&ResourceLimits::default()).unwrap();let(b,r)=kbits::write(&f,&KbitsWriteOptions::default()).unwrap();proptest::prop_assert!(!r.has_loss(),"{r}");proptest::prop_assert_eq!(a,b);}}

#[test]
fn result_is_readable_under_identical_small_limits() {
    let (f, _) = kbits::read(&fixture(), &ResourceLimits::default()).unwrap();
    for max_total_bytes in [128, 1024, 8192, 32768] {
        let l = ResourceLimits {
            max_total_bytes,
            ..Default::default()
        };
        if let Ok((bytes, _)) = kbits::write(&f, &KbitsWriteOptions { limits: l.clone() }) {
            assert!(kbits::read(&bytes, &l).is_ok());
        }
    }
}

#[test]
fn deeply_nested_caller_metadata_is_rejected_before_recursion() {
    let (mut f, _) = kbits::read(&fixture(), &ResourceLimits::default()).unwrap();
    let mut value = fontpixel::model::PropertyValue::Flag;
    for _ in 0..80 {
        value = fontpixel::model::PropertyValue::List(vec![value]);
    }
    f.properties.push(Property::new("deep", value));
    assert!(matches!(
        kbits::write(&f, &KbitsWriteOptions::default()),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}
