//! Native wyse original raster, strictness and allocation regressions.
#![cfg(feature = "wyse")]
use fontpixel::formats::{WriteOptions, wyse::*};
use fontpixel::resources::ReadContext;
use fontpixel::{Input, ResourceLimits, ResourcePath};
fn fixture() -> Vec<u8> {
    let mut v = b"\x1bcA04180402010080402010000000000000000".to_vec();
    v.push(25);
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
        &WriteOptions::new(ResourcePath::new("font.wyse").unwrap()),
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
        if true && end > 0 && end % 39 == 0 {
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
            &WriteOptions::new(ResourcePath::new("font.wyse").unwrap()),
            &WyseWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            assert!(parse(&w.output.into_single().unwrap(), &limits).is_ok());
        }
    }
    let mut named = set.clone();
    named.fonts[0].names.family = Some("original test".into());
    let mut options = WriteOptions::new(ResourcePath::new("font.wyse").unwrap());
    options.strict = true;
    assert!(matches!(
        write(&named, &options, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn terminal_bank_position_and_terminator_validation() {
    let original = fixture();
    for (at, value) in [(3, b'4'), (4, b'8'), (38, 0)] {
        let mut b = original.clone();
        b[at] = value;
        assert!(parse(&b, &Default::default()).is_err());
    }
    let mut repeated = original.clone();
    repeated.extend_from_slice(&original);
    let (s, r) = parse(&repeated, &Default::default()).unwrap();
    assert_eq!(s.fonts[0].len(), 1);
    assert!(r.has_loss());
}
