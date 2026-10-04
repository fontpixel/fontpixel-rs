//! Native dosstart bounded parsing and complete loss-contract tests.
#![cfg(feature = "dosstart")]
use fontpixel::formats::dosstart::{self, DosstartWriteOptions};
use fontpixel::model::{FontSet, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
include!("dosstart/fixtures.rs");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(dosstart::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.dosstart").unwrap())
}
#[test]
fn original_wire_roundtrip_and_strict() {
    let b = fixture("dosstart");
    let set = read(&b, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap.as_bytes()[0],
        if "dosstart" == "dosstart"
            || "dosstart" == "edwin"
            || "dosstart" == "mkwinfont"
            || "dosstart" == "psf2txt"
        {
            0xa0
        } else {
            0xa5
        }
    );
    let mut options = opts();
    options.strict = true;
    let w = dosstart::write(&set, &options, &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let reread = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(set, reread);
}
#[test]
fn metadata_loss_is_audited_and_strict_rejects() {
    let mut set = read(&fixture("dosstart"), &Default::default()).unwrap();
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
    let w = dosstart::write(&set, &opts(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut options = opts();
    options.strict = true;
    assert!(matches!(
        dosstart::write(&set, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn cumulative_limits_and_output_reread_contract() {
    let b = fixture("dosstart");
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
        let options = DosstartWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        };
        if let Ok(w) = dosstart::write(&set, &opts(), &options) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    assert!(
        dosstart::write(
            &set,
            &opts(),
            &DosstartWriteOptions {
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
    let b = fixture("dosstart");
    for n in 0..b.len().min(24) {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
}
#[test]
fn short_historical_raster_and_ascent_overflow() {
    let b = b"DosStartFont\nOriginal\n1, 2\n003002HHH, 4\n";
    let input = Input::new(b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    let (s, r) = dosstart::read(&mut c, &Default::default()).unwrap();
    assert!(r.has_loss());
    assert_eq!(
        s.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
        [0, 0xe0]
    );
    for ascent in [i64::MIN, i64::MIN + 1, i64::MAX] {
        let text = format!("DosStartFont\nOriginal\n1, {ascent}\n001001H, 1\n");
        let _ = read(text.as_bytes(), &Default::default());
    }
    assert!(read(b"DosStartFont\nOriginal\n0, 2\n", &Default::default()).is_err());
}
