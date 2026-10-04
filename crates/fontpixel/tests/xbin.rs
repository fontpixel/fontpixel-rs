//! Original XBIN screen-compression modes and retained palette/font payloads.
#![cfg(feature = "xbin")]
use fontpixel::formats::{WriteOptions, xbin::*};
use fontpixel::resources::ReadContext;
use fontpixel::{Input, ResourceLimits, ResourcePath};
fn data(screen: &[u8], count512: bool) -> Vec<u8> {
    let mut b = b"XBIN\x1a".to_vec();
    b.extend_from_slice(&[4, 0, 1, 0, 2, 7 | if count512 { 16 } else { 0 }]);
    b.extend_from_slice(&[31; 48]);
    b.extend((0..if count512 { 1024 } else { 512 }).map(|i| i as u8));
    b.extend_from_slice(screen);
    b
}
fn parse(
    b: &[u8],
    l: &ResourceLimits,
) -> fontpixel::Result<(fontpixel::FontSet, fontpixel::Report)> {
    read(
        &mut ReadContext::new(&Input::new(b), l)?,
        &Default::default(),
    )
}
#[test]
fn all_rle_modes_and_preserve() {
    for screen in [
        &[3, 65, 7, 66, 7, 67, 7, 68, 7][..],
        &[0x43, 65, 1, 2, 3, 4],
        &[0x83, 7, 65, 66, 67, 68],
        &[0xc3, 65, 7],
    ] {
        for big in [false, true] {
            let b = data(screen, big);
            let (s, _) = parse(&b, &Default::default()).unwrap();
            assert_eq!(s.fonts[0].len(), if big { 512 } else { 256 });
            let w = write(
                &s,
                &WriteOptions::new(ResourcePath::new("font.xb").unwrap()),
                &Default::default(),
            )
            .unwrap();
            assert!(!w.report.has_loss(), "{}", w.report);
            assert_eq!(b, w.output.into_single().unwrap());
        }
    }
}
#[test]
fn malformed_and_bounds() {
    let b = data(&[0xc3, 65, 7], false);
    for end in 0..b.len() {
        assert!(parse(&b[..end], &Default::default()).is_err());
    }
    let mut bad = b.clone();
    bad[10] |= 0x80;
    assert!(parse(&bad, &Default::default()).is_err());
    bad = b.clone();
    bad[11] = 64;
    assert!(parse(&bad, &Default::default()).is_err());
    bad = b.clone();
    *bad.get_mut(b.len() - 3).unwrap() = 0xc4;
    assert!(parse(&bad, &Default::default()).is_err());
    assert!(
        parse(
            &b,
            &ResourceLimits {
                max_glyphs: 255,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn same_limit_reopen_and_loss() {
    let b = data(&[0xc3, 65, 7], false);
    let (s, _) = parse(&b, &Default::default()).unwrap();
    for cap in [65536, 1048576, 4194304] {
        let limits = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = write(
            &s,
            &WriteOptions::new(ResourcePath::new("font.xb").unwrap()),
            &XbinWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            parse(&w.output.into_single().unwrap(), &limits).unwrap();
        }
    }
    let mut o = WriteOptions::new(ResourcePath::new("f.xb").unwrap());
    o.strict = true;
    assert!(matches!(
        write(
            &s,
            &o,
            &XbinWriteOptions {
                screen: ScreenMode::FontOnly,
                ..Default::default()
            }
        ),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
