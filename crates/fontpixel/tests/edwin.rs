//! Native edwin bounded parsing and complete loss-contract tests.
#![cfg(feature = "edwin")]
use fontpixel::formats::edwin::{self, EdwinWriteOptions};
use fontpixel::model::{FontSet, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
include!("dosstart/fixtures.rs");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(edwin::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.edwin").unwrap())
}
#[test]
fn original_wire_roundtrip_and_strict() {
    let b = fixture("edwin");
    let set = read(&b, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap.as_bytes()[0],
        if "edwin" == "dosstart"
            || "edwin" == "edwin"
            || "edwin" == "mkwinfont"
            || "edwin" == "psf2txt"
        {
            0xa0
        } else {
            0xa5
        }
    );
    let mut options = opts();
    options.strict = true;
    let w = edwin::write(&set, &options, &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let reread = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(set, reread);
}
#[test]
fn metadata_loss_is_audited_and_strict_rejects() {
    let mut set = read(&fixture("edwin"), &Default::default()).unwrap();
    let id = set.fonts[0].glyphs().next().unwrap().0;
    set.fonts[0]
        .edit_glyph(id, |g| {
            g.name = Some("extra-name".into());
            g.properties
                .push(Property::new("unrepresentable", "keep me"));
            g.metrics.vertical_origin = Some(fontpixel::model::Vector::new(1, 2));
            Ok(())
        })
        .unwrap();
    let w = edwin::write(&set, &opts(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut options = opts();
    options.strict = true;
    assert!(matches!(
        edwin::write(&set, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn cumulative_limits_and_output_reread_contract() {
    let b = fixture("edwin");
    let set = read(&b, &Default::default()).unwrap();
    for limits in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: b.len() + 1,
            ..Default::default()
        },
    ] {
        assert!(read(&b, &limits).is_err());
    }
    for total in [1024, 8192, 131072, 1048576] {
        let limits = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        let options = EdwinWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        };
        if let Ok(w) = edwin::write(&set, &opts(), &options) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    assert!(
        edwin::write(
            &set,
            &opts(),
            &EdwinWriteOptions {
                max_output_bytes: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn invalid_signature_and_short_inputs_do_not_panic() {
    assert!(read(&[], &Default::default()).is_err());
    assert!(read(b"not a bitmap font", &Default::default()).is_err());
    let b = fixture("edwin");
    for n in 0..b.len().min(24) {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
}
#[test]
fn arbitrary_precision_rows_and_width_validation() {
    let b = b"65 65\nwide: 2 129 340282366920938463463374607431768211457 1\n";
    let s = read(b, &Default::default()).unwrap();
    let g = s.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(g.bitmap.width(), 129);
    assert_eq!(g.bitmap.as_bytes()[0], 0x80);
    assert_eq!(g.bitmap.as_bytes()[16], 0x80);
    let w = edwin::write(&s, &opts(), &Default::default()).unwrap();
    assert_eq!(w.output.into_single().unwrap(), b);
    assert!(read(b"65 65\n65: 1 3 8\n", &Default::default()).is_err());
    assert!(read(b"65 65\n65: 1 3 -1\n", &Default::default()).is_err());
    assert!(read(b"0 4294967295\n", &Default::default()).is_err());
}
#[test]
fn utf8_nondigit_label_survives() {
    let b = "65 65\n字形: 1 3 5\n".as_bytes();
    let s = read(b, &Default::default()).unwrap();
    assert_eq!(
        s.fonts[0].glyphs().next().unwrap().1.name.as_deref(),
        Some("字形")
    );
    let mut o = opts();
    o.strict = true;
    let w = edwin::write(&s, &o, &Default::default()).unwrap();
    assert_eq!(w.output.into_single().unwrap(), b);
}
