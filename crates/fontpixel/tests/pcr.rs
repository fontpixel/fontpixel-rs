//! Native pcr bounded parsing and complete loss-contract tests.
#![cfg(feature = "pcr")]
use fontpixel::formats::pcr::{self, PcrWriteOptions};
use fontpixel::model::{FontSet, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
include!("dosstart/fixtures.rs");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(pcr::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.pcr").unwrap())
}
#[test]
fn original_wire_roundtrip_and_strict() {
    let b = fixture("pcr");
    let set = read(&b, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap.as_bytes()[0],
        if "pcr" == "dosstart" || "pcr" == "edwin" || "pcr" == "mkwinfont" || "pcr" == "psf2txt" {
            0xa0
        } else {
            0xa5
        }
    );
    let mut options = opts();
    options.strict = true;
    let w = pcr::write(&set, &options, &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let reread = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(set, reread);
}
#[test]
fn metadata_loss_is_audited_and_strict_rejects() {
    let mut set = read(&fixture("pcr"), &Default::default()).unwrap();
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
    let w = pcr::write(&set, &opts(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut options = opts();
    options.strict = true;
    assert!(matches!(
        pcr::write(&set, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn cumulative_limits_and_output_reread_contract() {
    let b = fixture("pcr");
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
        let options = PcrWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        };
        if let Ok(w) = pcr::write(&set, &opts(), &options) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    assert!(
        pcr::write(
            &set,
            &opts(),
            &PcrWriteOptions {
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
    let b = fixture("pcr");
    for n in 0..b.len().min(24) {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
}
#[test]
fn both_versions_and_exact_payload_length() {
    let mut b = fixture("pcr");
    b[4] = 1;
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        pcr::write(&s, &opts(), &Default::default())
            .unwrap()
            .output
            .into_single()
            .unwrap(),
        b
    );
    for offset in [4, 7, 9, 10] {
        let mut bad = b.clone();
        bad[offset] ^= 1;
        assert!(read(&bad, &Default::default()).is_err());
    }
}
