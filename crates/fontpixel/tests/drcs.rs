//! DEC DRCS native download and boundary tests.
#![cfg(feature = "drcs")]
use fontpixel::ResourceLimits;
use fontpixel::formats::drcs::*;
use fontpixel::model::Codepoint;
const SOURCE: &[u8] = b"\x1bP0;33;2;5;0;2;7;0{ @~?@??/@????;?????/?????\x1b\\";
#[test]
fn sixel_pixels_metrics_and_mapping() {
    let (s, r) = read(
        SOURCE,
        &DrcsReadOptions::default(),
        &ResourceLimits::default(),
    )
    .unwrap();
    assert!(!r.has_loss());
    let f = &s.fonts[0];
    assert_eq!(f.len(), 2);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[160, 128, 128, 128, 128, 128, 128]);
    assert_eq!(g.metrics.bounds.height, 7);
    assert_eq!(f.metrics.ascent, Some(7));
    let (bytes, r) = write(&s, &DrcsWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(
        read(
            &bytes,
            &DrcsReadOptions::default(),
            &ResourceLimits::default()
        )
        .unwrap()
        .0,
        s
    );
}
#[test]
fn c1_controls_and_96_character_set() {
    let src = b"\x900;0;2;5;0;2;1;1{ @?????;@@@@@\x9c";
    let (s, _) = read(src, &DrcsReadOptions::default(), &ResourceLimits::default()).unwrap();
    assert!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(32))
            .is_some()
    );
    let (bytes, _) = write(
        &s,
        &DrcsWriteOptions {
            charset: DrcsCharset::NinetySix,
            eight_bit_controls: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bytes[0], 0x90);
    assert_eq!(*bytes.last().unwrap(), 0x9c);
}
#[test]
fn invalid_controls_partial_geometry_and_truncations() {
    for src in [
        b"\x1bP0;0;1;5;0;2;7;0{ @?\x1b\\".as_slice(),
        b"\x1bP0;94;2;5;0;2;7;0{ @?;?\x1b\\",
        b"\x1bP0;0;2;5;0;2;1;0{ @A\x1b\\",
        b"\x1bP0;0;2;5;0;2;1;0{ @??????\x1b\\",
        b"\x1bP0;0;2;999999999;0;2;1;0{ @?\x1b\\",
        b"\x1bP0;0;2;5;0;2;1;0{ @?\x1b\\\x1b[H",
    ] {
        assert!(read(src, &DrcsReadOptions::default(), &ResourceLimits::default()).is_err());
    }
    for n in 0..SOURCE.len() {
        assert!(
            read(
                &SOURCE[..n],
                &DrcsReadOptions::default(),
                &ResourceLimits::default()
            )
            .is_err(),
            "{n}"
        );
    }
}
#[test]
fn profile_defaults_and_vt220_selectors() {
    let (a, _) = read(
        b"\x1bP{ @?\x1b\\",
        &DrcsReadOptions::default(),
        &ResourceLimits::default(),
    )
    .unwrap();
    assert_eq!(a.fonts[0].metrics.bounds.unwrap().width, 15);
    let (b, _) = read(
        b"\x1bP{ @?\x1b\\",
        &DrcsReadOptions {
            terminal: Terminal::Vt510,
            ..Default::default()
        },
        &ResourceLimits::default(),
    )
    .unwrap();
    assert_eq!(b.fonts[0].metrics.bounds.unwrap().height, 16);
    let (c, _) = read(
        b"\x1bP0;1;0;4{ @?\x1b\\",
        &DrcsReadOptions::default(),
        &ResourceLimits::default(),
    )
    .unwrap();
    assert_eq!(c.fonts[0].metrics.bounds.unwrap().width, 7);
    assert_eq!(c.fonts[0].metrics.bounds.unwrap().height, 10);
}
#[test]
fn limits_are_enforced_on_read_and_successful_write_readback() {
    let (s, _) = read(
        SOURCE,
        &DrcsReadOptions::default(),
        &ResourceLimits::default(),
    )
    .unwrap();
    for limits in [
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 34,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 1024,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 8,
            ..Default::default()
        },
    ] {
        assert!(read(SOURCE, &DrcsReadOptions::default(), &limits).is_err());
        assert!(
            write(
                &s,
                &DrcsWriteOptions {
                    limits,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    for max in [1000, 5000, 10000, 20000, 50000] {
        let limits = ResourceLimits {
            max_total_bytes: max,
            ..Default::default()
        };
        if let Ok((bytes, _)) = write(
            &s,
            &DrcsWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&bytes, &DrcsReadOptions::default(), &limits).is_ok());
        }
    }
}

#[test]
fn full_reload_terminating_separator_and_matching_designation() {
    let mut bytes = b"\x1bP0;1;1;5;0;2;1;0{ @".to_vec();
    for _ in 0..94 {
        bytes.extend_from_slice(b"?????;\n");
    }
    bytes.extend_from_slice(b"\x1b\\ \x1b( @\n");
    let (set, _) = read(&bytes, &Default::default(), &Default::default()).unwrap();
    assert_eq!(set.fonts[0].len(), 94);
    let last = bytes.len() - 2;
    bytes[last] = b'A';
    assert!(read(&bytes, &Default::default(), &Default::default()).is_err());
}
