//! Native Zap range, chunk ordinals, aliases and offset validation.
#![cfg(feature = "zapfont")]
use fontpixel::formats::{WriteOptions, zapfont::*};
use fontpixel::resources::ReadContext;
use fontpixel::{Input, ResourceLimits, ResourcePath};
fn put(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn old() -> Vec<u8> {
    let mut b = vec![0; 36];
    b[..8].copy_from_slice(b"ZapFont\r");
    for (at, v) in [(8, 9), (12, 1), (16, 65), (20, 67)] {
        put(&mut b, at, v);
    }
    b[32..].copy_from_slice(&[1, 1, 2, 0]);
    b
}
fn redraw() -> Vec<u8> {
    let mut b = vec![0; 64 + 256 + 256 + 2];
    b[..4].copy_from_slice(b"ZRUF");
    for (at, v) in [
        (8, 8),
        (12, 1),
        (16, 32),
        (32, 0),
        (36, 256),
        (40, 64),
        (48, 0x2600),
        (52, 2),
        (56, 320),
    ] {
        put(&mut b, at, v);
    }
    b[321] = 2;
    b[322] = 1;
    b[323] = 2;
    b[576] = 1;
    b[577] = 2;
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
fn native_old_and_redraw() {
    for (b, variant) in [(old(), ZapVariant::Old), (redraw(), ZapVariant::Redraw)] {
        let (s, _) = parse(&b, &Default::default()).unwrap();
        let w = write(
            &s,
            &WriteOptions::new(ResourcePath::new("f.zap").unwrap()),
            &ZapfontWriteOptions {
                variant,
                ..Default::default()
            },
        )
        .unwrap();
        let (back, _) = parse(&w.output.into_single().unwrap(), &Default::default()).unwrap();
        if variant == ZapVariant::Old {
            assert_eq!(s, back);
        } else {
            let g = s.fonts[0]
                .glyphs()
                .find(|(_, g)| {
                    g.codepoints
                        .contains(&fontpixel::model::Codepoint::Unicode(0x2601))
                })
                .unwrap()
                .1;
            assert_eq!(g.codepoints.len(), 2);
            assert_eq!(
                match &g.bitmap {
                    fontpixel::model::GlyphBitmap::Mono(m) => m.as_bytes(),
                    _ => unreachable!(),
                },
                &[0x40]
            );
        }
    }
}
#[test]
fn malformed_tables_offsets_and_limits() {
    let b = redraw();
    for at in [0, 31, 63, 319, 575, 577] {
        assert!(parse(&b[..at], &Default::default()).is_err());
    }
    let mut bad = b.clone();
    put(&mut bad, 56, 64);
    assert!(parse(&bad, &Default::default()).is_err());
    bad = b.clone();
    bad[321] = 7;
    assert!(parse(&bad, &Default::default()).is_err());
    bad = b.clone();
    put(&mut bad, 48, 0xd800);
    assert!(parse(&bad, &Default::default()).is_err());
    assert!(
        parse(
            &b,
            &ResourceLimits {
                max_glyphs: 257,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn strict_and_same_limits() {
    let (s, _) = parse(&old(), &Default::default()).unwrap();
    for cap in [4096, 65536, 1048576] {
        let limits = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = write(
            &s,
            &WriteOptions::new(ResourcePath::new("f").unwrap()),
            &ZapfontWriteOptions {
                variant: ZapVariant::Old,
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            parse(&w.output.into_single().unwrap(), &limits).unwrap();
        }
    }
}
#[test]
fn padding_null_alias_and_unreferenced_ordinals() {
    let mut b = redraw();
    b.push(0);
    let (_, r) = parse(&b, &Default::default()).unwrap();
    assert!(r.has_loss());
    b = redraw();
    put(&mut b, 20, 64);
    let (s, _) = parse(&b, &Default::default()).unwrap();
    assert!(s.fonts[0].metrics.default_glyph.is_some());
    b = redraw();
    b[322] = 0;
    let (s, _) = parse(&b, &Default::default()).unwrap();
    assert!(s.fonts[0].glyphs().any(|(_, g)| g.codepoints.is_empty()));
}
