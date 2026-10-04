//! Original SFD bitmap records, ASCII85 and strike tests.
#![cfg(feature = "sfd")]
use fontpixel::formats::sfd::{self, SfdWriteOptions};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::{Bitmap, Font, FontSet, Glyph, ResourceLimits};
fn fixture() -> String {
    "SplineFontDB: 3.0\nFontName: Test\nFamilyName: Example\nAscent: 9\nDescent: 3\nBeginChars: 1 1\nStartChar: A\nEncoding: 65 65 0\nWidth: 8\nEndChar\nEndChars\nBitmapFont: 12 1 9 3 1\nBDFChar: 0 65 8 -2 5 -1 2\nz\nEndBitmapFont\nEndSplineFont\n".into()
}
#[test]
fn native_strike_and_identity() {
    let (f, r) = sfd::read(fixture().as_bytes(), &ResourceLimits::default()).unwrap();
    assert!(!r.has_loss());
    let g = f.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(g.metrics.bounds.width, 8);
    assert_eq!(g.metrics.bounds.height, 4);
    assert_eq!(g.metrics.bounds.x, -2);
    assert_eq!(g.metrics.bounds.y, -1);
    assert_eq!(g.codepoints, [Codepoint::Unicode(65)]);
    let (b, r) = sfd::write(&f, &SfdWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(sfd::read(&b, &ResourceLimits::default()).unwrap().0, f);
}
#[test]
fn failed_name_write_retains_source_aliases() {
    let (mut set, _) = sfd::read(fixture().as_bytes(), &ResourceLimits::default()).unwrap();
    let f = &mut set.fonts[0];
    f.names.family = Some("非ASCII".into());
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(66));
        Ok(())
    })
    .unwrap();
    let error = sfd::write(&set, &SfdWriteOptions::default()).unwrap_err();
    let r = error.report().unwrap();
    assert!(r.has_errors());
    let codes: Vec<_> = r
        .entries
        .iter()
        .filter(|e| e.code == "sfd.glyph_omitted")
        .map(|e| e.location.codepoint)
        .collect();
    assert_eq!(codes, [Some(65), Some(66)]);
}
#[test]
fn multiple_strikes() {
    let s = fixture().replace(
        "EndSplineFont",
        "BitmapFont: 16 1 12 4 1\nBDFChar: 0 65 8 0 7 0 3\nz\nEndBitmapFont\nEndSplineFont",
    );
    let (f, _) = sfd::read(s.as_bytes(), &ResourceLimits::default()).unwrap();
    assert_eq!(f.fonts.len(), 2);
    assert_eq!(
        sfd::read(
            &sfd::write(&f, &SfdWriteOptions::default()).unwrap().0,
            &ResourceLimits::default()
        )
        .unwrap()
        .0,
        f
    );
}
#[test]
fn outlines_are_explicit_loss() {
    let s = fixture().replace(
        "Width: 8\nEndChar",
        "Width: 8\nSplineSet\n0 0 m 1\nEndSplineSet\nEndChar",
    );
    let (_, r) = sfd::read(s.as_bytes(), &ResourceLimits::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "sfd.outlines"));
}
#[test]
fn malformed_or_composite_is_fatal() {
    for s in [
        fixture().replace("\nz\n", "\nuuuuu\n"),
        fixture().replace("\nz\n", "\n!\n"),
        fixture().replace("EndBitmapFont", "BDFRefChar: 0 1 0 0 N\nEndBitmapFont"),
        fixture().replace("EndSplineFont", ""),
        fixture().replace("BDFChar: 0 65 8 -2 5 -1 2", "BDFChar: 0 65 8 5 -2 0 3"),
    ] {
        assert!(
            sfd::read(s.as_bytes(), &ResourceLimits::default()).is_err(),
            "{s}"
        );
    }
}
#[test]
fn budgets_and_ascii85_padding() {
    let l = ResourceLimits {
        max_pixels_per_glyph: 31,
        ..Default::default()
    };
    assert!(sfd::read(fixture().as_bytes(), &l).is_err());
    let s = fixture().replace("-1 2", "-1 -1");
    let (f, _) = sfd::read(s.as_bytes(), &ResourceLimits::default()).unwrap();
    assert_eq!(f.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(), [0]);
}
proptest::proptest! {#[test]fn random_mono_roundtrip(w in 1u32..20,h in 1u32..20,data in proptest::collection::vec(proptest::prelude::any::<u8>(),60)){let size=w.div_ceil(8)as usize*h as usize;let b:Bitmap=Bitmap::from_raw(w,h,&data[..size],Default::default()).unwrap();let mut g=Glyph::new(b);g.codepoints.push(Codepoint::Unicode(65));g.metrics.advance=Some(Vector::new(23,0));let mut f=Font::default();f.metrics.pixel_size=Some(20);f.add_glyph(g).unwrap();let(a,_)=sfd::write(&FontSet::from(f),&SfdWriteOptions::default()).unwrap();let(f,_)=sfd::read(&a,&ResourceLimits::default()).unwrap();let(b,r)=sfd::write(&f,&SfdWriteOptions::default()).unwrap();proptest::prop_assert!(!r.has_loss(),"{r}");proptest::prop_assert_eq!(a,b);}}

#[test]
fn outline_widths_and_row_padding_are_not_silent() {
    let source = fixture().replace("Width: 8", "Width: 500\nVWidth: 1000");
    let (set, r) = sfd::read(source.as_bytes(), &ResourceLimits::default()).unwrap();
    assert!(!r.has_loss());
    let (bytes, r) = sfd::write(&set, &SfdWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("Width: 500\nVWidth: 1000")
    );
    let source = fixture()
        .replace("-2 5 -1 2", "-2 -2 -1 2")
        .replace("\nz\n", "\n!!!!\"\n");
    let (_, r) = sfd::read(source.as_bytes(), &ResourceLimits::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "sfd.padding_bits"));
}

#[test]
fn signed_extremes_fail_without_panicking() {
    let s = fixture().replace(
        "BDFChar: 0 65 8 -2 5 -1 2",
        "BDFChar: 0 65 8 -2 5 -1 2 -9223372036854775808",
    );
    assert!(sfd::read(s.as_bytes(), &ResourceLimits::default()).is_err());
    let (mut set, _) = sfd::read(fixture().as_bytes(), &ResourceLimits::default()).unwrap();
    set.fonts[0].metrics.pixel_size = None;
    set.fonts[0].metrics.ascent = Some(i64::MAX);
    set.fonts[0].metrics.descent = Some(1);
    let _ = sfd::write(&set, &SfdWriteOptions::default());
}

#[test]
fn native_gray_depth_is_preserved_when_samples_fit() {
    for (depth, data) in [(2, "zz"), (4, "zzzz"), (8, "zzzzzzzz")] {
        let source = fixture()
            .replace(
                "BitmapFont: 12 1 9 3 1",
                &format!("BitmapFont: 12 1 9 3 {depth}"),
            )
            .replace("\nz\n", &format!("\n{data}\n"));
        let (set, _) = sfd::read(source.as_bytes(), &ResourceLimits::default()).unwrap();
        let (bytes, report) = sfd::write(&set, &SfdWriteOptions::default()).unwrap();
        assert!(!report.has_loss(), "{report}");
        assert_eq!(
            sfd::read(&bytes, &ResourceLimits::default()).unwrap().0,
            set
        );
    }
}
