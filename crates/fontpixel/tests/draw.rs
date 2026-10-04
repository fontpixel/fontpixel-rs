//! Independent fixtures and property checks for draw.
#![cfg(feature = "draw")]
use fontpixel::formats::draw;
use fontpixel::model::{Codepoint, Property};
use fontpixel::{Bitmap, Glyph, ResourceLimits};
use proptest::prelude::*;
#[test]
fn actual_hexdraw_grammar() {
    let bytes = b"% font comment\n0041:\t--##----\n\t-#--#---\n\n1F600:\n ########\n #------#\n\n";
    let (s, r) = draw::read(bytes, &Default::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(s.fonts[0].len(), 2);
    let (b, r) = draw::write(&s, &Default::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(draw::read(&b, &Default::default()).unwrap().0, s);
}
#[test]
fn malformed_rows_and_scalars() {
    for b in [
        b"0041:\n-#\n-\n".as_slice(),
        b"D800:\n-#\n",
        b"110000:\n-#\n",
        b"0041:\n",
        b"-#\n",
        b"0041:\n.#\n",
    ] {
        assert!(draw::read(b, &Default::default()).is_err(), "{b:?}");
    }
}
#[test]
fn alias_property_metrics_and_source_losses() {
    let (mut s, _) = draw::read(b"0041:\t#-\n\n", &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    let mut g = s.fonts[0].glyph(id).unwrap().clone();
    g.codepoints
        .extend([Codepoint::Unicode(66), Codepoint::Legacy(77)]);
    g.properties.push(Property::new("source", "opaque"));
    g.metrics.advance = None;
    s.fonts[0].replace_glyph(id, g).unwrap();
    s.source_format = Some("bdf".into());
    let (_, r) = draw::write(&s, &Default::default()).unwrap();
    for c in [
        "write.mapping",
        "write.property",
        "write.glyph_metric",
        "write.source_format",
    ] {
        assert!(
            r.entries.iter().any(|e| e.code.ends_with(c)),
            "missing {c}: {r}"
        );
    }
}
#[test]
fn strict_readback_limits_and_empty_output() {
    let (s, _) = draw::read(b"0041:\t#-\n\n", &Default::default()).unwrap();
    let limits = ResourceLimits {
        max_total_bytes: 100,
        ..Default::default()
    };
    assert!(
        draw::write(
            &s,
            &draw::DrawWriteOptions {
                limits,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut f = fontpixel::Font::default();
    f.add_glyph(Glyph::new("#".parse::<Bitmap>().unwrap()))
        .unwrap();
    let e = draw::write(&f.into(), &Default::default()).unwrap_err();
    assert!(e.report().unwrap().has_loss());
}
proptest! {#[test]fn arbitrary_rectangular_unicode_glyphs(width in 1usize..32,height in 1usize..32,code in 0u32..0xD800,values in prop::collection::vec(any::<bool>(),1024)){
 let mut text=format!("{code:04X}:");for y in 0..height{text.push('\t');for x in 0..width{text.push(if values[y*width+x]{'#'}else{'-'});}text.push('\n');}let(s,_)=draw::read(text.as_bytes(),&Default::default()).unwrap();let(bytes,report)=draw::write(&s,&Default::default()).unwrap();prop_assert!(!report.has_loss(),"{report}");prop_assert_eq!(s,draw::read(&bytes,&Default::default()).unwrap().0);
}}
#[test]
fn diagnostic_storage_and_escaped_names_share_write_budget() {
    let (mut s, _) = draw::read(b"0041:\t#-\n\n", &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    let mut g = s.fonts[0].glyph(id).unwrap().clone();
    g.codepoints.extend((1000..1100).map(Codepoint::Legacy));
    s.fonts[0].replace_glyph(id, g).unwrap();
    let limits = ResourceLimits {
        max_total_bytes: 30000,
        ..Default::default()
    };
    let error = draw::write(
        &s,
        &draw::DrawWriteOptions {
            limits,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, fontpixel::Error::ResourceLimit { .. }));
    let (_, report) = draw::write(
        &s,
        &draw::DrawWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: 200000,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        report
            .entries
            .iter()
            .filter(|e| e.code == "draw.write.mapping")
            .count(),
        100
    );
    let mut g = s.fonts[0].glyph(id).unwrap().clone();
    g.properties
        .push(Property::new("\u{7f}".repeat(4000), "value"));
    s.fonts[0].replace_glyph(id, g).unwrap();
    let e = draw::write(
        &s,
        &draw::DrawWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: 50000,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(e, fontpixel::Error::ResourceLimit { .. }));
}
