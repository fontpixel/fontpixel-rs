//! Native plus3dos bounded parsing and complete loss-contract tests.
#![cfg(feature = "plus3dos")]
use fontpixel::formats::plus3dos::{self, Plus3dosWriteOptions};
use fontpixel::model::{FontSet, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
include!("dosstart/fixtures.rs");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(plus3dos::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.plus3dos").unwrap())
}
#[test]
fn original_wire_roundtrip_and_strict() {
    let b = fixture("plus3dos");
    let set = read(&b, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap.as_bytes()[0],
        if "plus3dos" == "dosstart"
            || "plus3dos" == "edwin"
            || "plus3dos" == "mkwinfont"
            || "plus3dos" == "psf2txt"
        {
            0xa0
        } else {
            0xa5
        }
    );
    let mut options = opts();
    options.strict = true;
    let w = plus3dos::write(&set, &options, &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let reread = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(set, reread);
}
#[test]
fn metadata_loss_is_audited_and_strict_rejects() {
    let mut set = read(&fixture("plus3dos"), &Default::default()).unwrap();
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
    let w = plus3dos::write(&set, &opts(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut options = opts();
    options.strict = true;
    assert!(matches!(
        plus3dos::write(&set, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn cumulative_limits_and_output_reread_contract() {
    let b = fixture("plus3dos");
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
        let options = Plus3dosWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        };
        if let Ok(w) = plus3dos::write(&set, &opts(), &options) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    assert!(
        plus3dos::write(
            &set,
            &opts(),
            &Plus3dosWriteOptions {
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
    let b = fixture("plus3dos");
    for n in 0..b.len().min(24) {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
}
#[test]
fn checksum_and_native_length_fields_are_enforced() {
    for offset in [11, 15, 16, 127] {
        let mut b = fixture("plus3dos");
        b[offset] ^= 1;
        assert!(read(&b, &Default::default()).is_err());
    }
    let mut b = fixture("plus3dos");
    b.push(0);
    assert!(read(&b, &Default::default()).is_err());
}
