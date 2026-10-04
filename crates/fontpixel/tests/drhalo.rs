//! Native drhalo bounded parsing and complete loss-contract tests.
#![cfg(feature = "drhalo")]
use fontpixel::formats::drhalo::{self, DrhaloWriteOptions};
use fontpixel::model::{FontSet, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
include!("dosstart/fixtures.rs");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(drhalo::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.drhalo").unwrap())
}
#[test]
fn original_wire_roundtrip_and_strict() {
    let b = fixture("drhalo");
    let set = read(&b, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap.as_bytes()[0],
        if "drhalo" == "dosstart"
            || "drhalo" == "edwin"
            || "drhalo" == "mkwinfont"
            || "drhalo" == "psf2txt"
        {
            0xa0
        } else {
            0xa5
        }
    );
    let mut options = opts();
    options.strict = true;
    let w = drhalo::write(&set, &options, &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let reread = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(set, reread);
}
#[test]
fn metadata_loss_is_audited_and_strict_rejects() {
    let mut set = read(&fixture("drhalo"), &Default::default()).unwrap();
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
    let w = drhalo::write(&set, &opts(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut options = opts();
    options.strict = true;
    assert!(matches!(
        drhalo::write(&set, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn cumulative_limits_and_output_reread_contract() {
    let b = fixture("drhalo");
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
        let options = DrhaloWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        };
        if let Ok(w) = drhalo::write(&set, &opts(), &options) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    assert!(
        drhalo::write(
            &set,
            &opts(),
            &DrhaloWriteOptions {
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
    let b = fixture("drhalo");
    for n in 0..b.len().min(24) {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
}
#[test]
fn weak_magic_exact_size_and_explicit_first_position() {
    use fontpixel::formats::Confidence;
    assert_eq!(drhalo::detect(b"AH"), Confidence::NoMatch);
    assert_eq!(drhalo::detect(&fixture("drhalo")), Confidence::Possible);
    let b = fixture("drhalo");
    let l = ResourceLimits::default();
    let input = Input::new(&b);
    let mut c = ReadContext::new(&input, &l).unwrap();
    let (s, _) = drhalo::read(
        &mut c,
        &drhalo::DrhaloReadOptions {
            first_codepoint: Some(65),
        },
    )
    .unwrap();
    assert!(
        s.fonts[0]
            .glyph_for_codepoint(fontpixel::model::Codepoint::Legacy(65))
            .is_some()
    );
    let mut bad = b.clone();
    bad[12] = 0;
    assert!(read(&bad, &l).is_err());
    let mut bad = b.clone();
    bad[16] = 0;
    assert!(read(&bad, &l).is_err());
    let mut bad = b;
    bad.pop();
    assert!(read(&bad, &l).is_err());
}
