//! Native bbc original raster, strictness and allocation regressions.
#![cfg(feature = "bbc")]
use fontpixel::formats::{WriteOptions, bbc::*};
use fontpixel::resources::ReadContext;
use fontpixel::{Input, ResourceLimits, ResourcePath};
fn fixture() -> Vec<u8> {
    let mut v = vec![23, 65];
    v.extend_from_slice(&[0x80, 0x40, 0x20, 0x10, 8, 4, 2, 1]);
    v
}
fn parse(
    v: &[u8],
    l: &ResourceLimits,
) -> fontpixel::Result<(fontpixel::FontSet, fontpixel::Report)> {
    let input = Input::new(v);
    read(&mut ReadContext::new(&input, l)?, &Default::default())
}
#[test]
fn native_pixels_mapping_and_readback() {
    let data = fixture();
    let (set, r) = parse(&data, &Default::default()).unwrap();
    assert!(!r.has_loss());
    let font = &set.fonts[0];
    assert!(!font.is_empty());
    let w = write(
        &set,
        &WriteOptions::new(ResourcePath::new("font.bbc").unwrap()),
        &Default::default(),
    )
    .unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let output = w.output.into_single().unwrap();
    let (back, _) = parse(&output, &Default::default()).unwrap();
    assert_eq!(set, back);
}
#[test]
fn truncation_and_resource_limits() {
    let data = fixture();
    for end in 0..data.len() {
        if end > 0 && end % 10 == 0 {
            continue;
        }
        assert!(
            parse(&data[..end], &Default::default()).is_err(),
            "accepted prefix {end}"
        );
    }
    let l = ResourceLimits {
        max_total_bytes: 1024,
        ..Default::default()
    };
    assert!(parse(&data, &l).is_err());
    let l = ResourceLimits {
        max_glyphs: 0,
        ..Default::default()
    };
    assert!(parse(&data, &l).is_err());
}
#[test]
fn successful_write_reopens_at_same_limits_and_strict_reports_metadata() {
    let (set, _) = parse(&fixture(), &Default::default()).unwrap();
    for cap in [4096, 16384, 262144, 1048576] {
        let limits = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = write(
            &set,
            &WriteOptions::new(ResourcePath::new("font.bbc").unwrap()),
            &BbcWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            assert!(parse(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    let mut named = set.clone();
    named.fonts[0].names.family = Some("original test".into());
    let mut options = WriteOptions::new(ResourcePath::new("font.bbc").unwrap());
    options.strict = true;
    assert!(matches!(
        write(&named, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn redefinitions_controls_and_extreme_baseline() {
    let mut b = fixture();
    let mut second = fixture();
    second[2] = 0xff;
    b.extend_from_slice(&second);
    let (s, r) = parse(&b, &Default::default()).unwrap();
    assert_eq!(s.fonts[0].len(), 1);
    assert!(r.has_loss());
    b[1] = 0;
    assert!(parse(&b, &Default::default()).is_err());
    let bytes = fixture();
    let input = Input::new(&bytes);
    let limits = ResourceLimits::default();
    assert!(
        read(
            &mut ReadContext::new(&input, &limits).unwrap(),
            &BbcReadOptions {
                baseline: Some(i64::MIN),
                ..Default::default()
            }
        )
        .is_err()
    );
}
