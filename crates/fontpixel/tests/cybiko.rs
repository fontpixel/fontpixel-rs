//! Original Cybiko unsigned geometry and missing-slot fixtures.
#![cfg(feature = "cybiko")]
use fontpixel::formats::cybiko::{self, CybikoWriteOptions};
use fontpixel::model::Codepoint;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![1, 35, 9, 7];
    for i in 0..35 {
        if i == 0 {
            b.extend([0, 5, 4, 0]);
        } else if i == 33 {
            b.extend([1, 1, 9, 2, 0xaa, 0x80, 0x55, 0]);
        } else if i == 34 {
            b.extend([0, 2, 3, 2, 0xa0, 0x60]);
        } else {
            b.extend([0, 0, 1, 1, 0]);
        }
    }
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    cybiko::read(&mut ReadContext::new(&i, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.cyf").unwrap())
}
#[test]
fn sparse_slots_space_baseline_width9() {
    let b = fixture();
    let s = read(&b, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 3);
    assert_eq!(f.metrics.ascent, Some(5));
    assert!(f.glyph_for_codepoint(Codepoint::Legacy(33)).is_none());
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!((g.metrics.bounds.x, g.metrics.bounds.y), (1, 2));
    assert_eq!(g.bitmap.as_bytes(), [0xaa, 0x80, 0x55, 0]);
    let mut o = options();
    o.strict = true;
    assert_eq!(
        cybiko::write(&s, &o, &Default::default())
            .unwrap()
            .output
            .into_single()
            .unwrap(),
        b
    );
}
#[test]
fn malformed_counts_padding_and_limits() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err());
    }
    let mut tail = b.clone();
    tail.push(0);
    assert!(read(&tail, &Default::default()).is_err());
    let s = read(&b, &Default::default()).unwrap();
    for total in [0, 1000, 10000, 100000] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(w) = cybiko::write(
            &s,
            &options(),
            &CybikoWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &l).is_ok());
        }
    }
}

#[test]
fn registry_dispatch_and_strict_metadata_loss() {
    let bytes = fixture();
    let registry = fontpixel::registry();
    let mut loaded = registry
        .read(
            &Input::new(&bytes),
            &fontpixel::ReadOptions {
                format: Some("cybiko".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(loaded.fonts, read(&bytes, &Default::default()).unwrap());
    loaded.fonts.fonts[0]
        .properties
        .push(fontpixel::model::Property::new(
            "e1.unrepresentable",
            123i64,
        ));
    let mut options = options();
    let output = registry.write(&loaded.fonts, "cybiko", &options).unwrap();
    assert!(output.report.has_loss());
    options.strict = true;
    assert!(registry.write(&loaded.fonts, "cybiko", &options).is_err());
}
