//! Original byte-authored FIGfont fixtures (project MIT license).
#![cfg(feature = "figlet")]
use fontpixel::formats::figlet::{self, FigletWriteOptions};
use fontpixel::model::{Codepoint, FontSet, PropertyValue, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture(art: bool) -> Vec<u8> {
    let mut b = b"flf2a$ 2 1 5 -1 1 0 0 3\nOriginal MIT test\n".to_vec();
    for code in (32..127).chain([196, 214, 220, 228, 246, 252, 223]) {
        if code == 65 {
            b.extend_from_slice(if art {
                b"A$ @\n ##@@\n"
            } else {
                b"## @\n ##@@\n"
            });
        } else {
            b.extend_from_slice(b"@\n@@\n");
        }
    }
    b.extend_from_slice(b"0 missing\n#@\n#@@\n0x1F600 face\n #@\n# @@\n-02 map\n#@\n @@\n");
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    figlet::read(&mut c, &Default::default())
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("test.flf").unwrap())
}
#[test]
fn signed_tags_non_bmp_metrics_and_original_art() {
    let b = fixture(true);
    let (s, r) = read(&b, &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "figlet.read.hardblank"));
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "figlet.read.character_art")
    );
    let f = s.single().unwrap();
    assert_eq!(f.len(), 105);
    assert!(f.glyph_for_codepoint(Codepoint::Unicode(0x1f600)).is_some());
    assert!(
        f.glyph_for_codepoint(Codepoint::Legacy((-2i32) as u32))
            .is_some()
    );
    assert_eq!(
        f.properties.get("figlet.source"),
        Some(&PropertyValue::Bytes(b))
    );
    let g = f.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), [0xc0, 0x60]);
    assert_eq!(g.metrics.bounds.y, -1);
    assert_eq!(g.metrics.advance, Some(Vector::new(3, 0)));
    assert_eq!(f.metrics.default_character, Some(Codepoint::Unicode(0)));
    let out = figlet::write(&s, &opts(), &Default::default()).unwrap();
    assert!(out.report.has_loss());
    let (target, rr) = read(&out.output.into_single().unwrap(), &Default::default()).unwrap();
    assert!(!rr.has_loss());
    assert_eq!(
        target
            .single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap,
        g.bitmap
    );
}
#[test]
fn canonical_is_stable_and_strict_preserves_loss() {
    let (s, _) = read(&fixture(false), &Default::default()).unwrap();
    let first = figlet::write(&s, &opts(), &Default::default()).unwrap();
    let b = first.output.into_single().unwrap();
    let (s, _) = read(&b, &Default::default()).unwrap();
    let mut o = opts();
    o.strict = true;
    let second = figlet::write(&s, &o, &Default::default()).unwrap();
    assert!(!second.report.has_loss(), "{}", second.report);
    assert_eq!(second.output.into_single().unwrap(), b);
    let (art, _) = read(&fixture(true), &Default::default()).unwrap();
    assert!(figlet::write(&art, &o, &Default::default()).is_err());
}
#[test]
fn missing_rows_bad_tags_and_widths_fail() {
    let b = fixture(false);
    for cut in [0, 5, 20, 50, b.len() - 5] {
        assert!(read(&b[..cut], &Default::default()).is_err(), "{cut}");
    }
    for needle in [b"-02 map".as_slice(), b"0x1F600 face"] {
        let mut bad = b.clone();
        let at = bad.windows(needle.len()).position(|w| w == needle).unwrap();
        bad.splice(at..at + needle.len(), b"-1 forbidden".iter().copied());
        assert!(read(&bad, &Default::default()).is_err());
    }
    let mut bad = b.clone();
    let at = bad.windows(4).position(|w| w == b"## @").unwrap();
    bad.remove(at);
    assert!(read(&bad, &Default::default()).is_err());
}
#[test]
fn crlf_preserved_and_same_limits() {
    let b = fixture(false);
    let crlf = b
        .iter()
        .flat_map(|v| {
            if *v == b'\n' {
                vec![b'\r', b'\n']
            } else {
                vec![*v]
            }
        })
        .collect::<Vec<_>>();
    let (s, _) = read(&crlf, &Default::default()).unwrap();
    assert_eq!(
        s.single().unwrap().properties.get("figlet.source"),
        Some(&PropertyValue::Bytes(crlf))
    );
    for total in [0, 10000, 200000, 2000000] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(out) = figlet::write(
            &s,
            &opts(),
            &FigletWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&out.output.into_single().unwrap(), &l).is_ok());
        }
    }
}
#[test]
fn glyph_count_and_pixel_budgets() {
    let b = fixture(false);
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_glyphs: 104,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_pixels_per_glyph: 5,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_table_bytes: 20,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn inherited_advance_and_extreme_baseline_are_checked() {
    let (mut s, _) = read(&fixture(false), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Unicode(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.advance = None;
            Ok(())
        })
        .unwrap();
    s.fonts[0].metrics.glyph_defaults.advance = Some(Vector::new(4, 0));
    let out = figlet::write(&s, &opts(), &Default::default()).unwrap();
    let (back, _) = read(&out.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(
        back.single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .metrics
            .advance,
        Some(Vector::new(4, 0))
    );
    s.fonts[0].metrics.ascent = Some(i64::MAX);
    assert!(figlet::write(&s, &opts(), &Default::default()).is_err());
}
