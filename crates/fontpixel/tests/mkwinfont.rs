//! Native mkwinfont bounded parsing and complete loss-contract tests.
#![cfg(feature = "mkwinfont")]
use fontpixel::formats::mkwinfont::{self, MkwinfontWriteOptions};
use fontpixel::model::{FontSet, Property};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
include!("dosstart/fixtures.rs");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(mkwinfont::read(&mut c, &Default::default())?.0)
}
fn opts() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.mkwinfont").unwrap())
}
#[test]
fn original_wire_roundtrip_and_strict() {
    let b = fixture("mkwinfont");
    let set = read(&b, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap.as_bytes()[0],
        if "mkwinfont" == "dosstart"
            || "mkwinfont" == "edwin"
            || "mkwinfont" == "mkwinfont"
            || "mkwinfont" == "psf2txt"
        {
            0xa0
        } else {
            0xa5
        }
    );
    let mut options = opts();
    options.strict = true;
    let w = mkwinfont::write(&set, &options, &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let reread = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    assert_eq!(set, reread);
}
#[test]
fn metadata_loss_is_audited_and_strict_rejects() {
    let mut set = read(&fixture("mkwinfont"), &Default::default()).unwrap();
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
    let w = mkwinfont::write(&set, &opts(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
    let mut options = opts();
    options.strict = true;
    assert!(matches!(
        mkwinfont::write(&set, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn cumulative_limits_and_output_reread_contract() {
    let b = fixture("mkwinfont");
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
        let options = MkwinfontWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        };
        if let Ok(w) = mkwinfont::write(&set, &opts(), &options) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    assert!(
        mkwinfont::write(
            &set,
            &opts(),
            &MkwinfontWriteOptions {
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
    let b = fixture("mkwinfont");
    for n in 0..b.len().min(24) {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
}
#[test]
fn dimensions_and_extreme_metrics_are_bounded() {
    for ascent in [i64::MIN, i64::MIN + 1, i64::MAX] {
        let text = format!("height 1\nascent {ascent}\nchar 65\nwidth 1\n1\n");
        let _ = read(text.as_bytes(), &Default::default());
    }
    assert!(
        read(
            b"height 2\nascent 1\nchar 65\nwidth 3\n11\n11\n",
            &Default::default()
        )
        .is_err()
    );
    assert!(
        read(
            b"height 2\nascent 1\nchar 256\nwidth 3\n111\n111\n",
            &Default::default()
        )
        .is_err()
    );
}
#[test]
fn utf8_family_and_comments_survive() {
    let b = "facename 字型 Café\nheight 1\nascent 1\n# 注释\nchar 65\nwidth 1\n1\n".as_bytes();
    let s = read(b, &Default::default()).unwrap();
    assert_eq!(s.fonts[0].names.family.as_deref(), Some("字型 Café"));
    let mut o = opts();
    o.strict = true;
    let w = mkwinfont::write(&s, &o, &Default::default()).unwrap();
    assert_eq!(
        read(&w.output.into_single().unwrap(), &Default::default()).unwrap(),
        s
    );
}
