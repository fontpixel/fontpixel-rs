//! Original packed Qt/Embedded QPF fixtures; no third-party glyph artwork.
#![cfg(feature = "qpf")]
use fontpixel::formats::qpf::{self, QpfReadOptions, QpfWriteOptions};
use fontpixel::model::{Codepoint, Property, Vector};
use fontpixel::{FontSet, ResourceLimits};
fn fixture() -> Vec<u8> {
    let mut b = vec![3, 1, 255, 0, 4, 255, 0, 1, 1, 7];
    // Root B, left A, right surrogate: all node records precede metrics.
    b.extend_from_slice(&[0, 66, 0, 66, 3, 0, 65, 0, 65, 0, 0xd8, 0, 0xd8, 0, 0]);
    b.extend_from_slice(&[
        2, 3, 2, 9, 255, 4, 3, 8, 1, 2, 1, 0, 0, 2, 1, 0, 1, 0, 3, 0, 0, 1, 0, 0,
    ]);
    b.extend_from_slice(&[0xa0, 0x77, 0x60, 0x88, 0xc0, 0x11, 0x22, 0x33]);
    b
}
fn read(b: &[u8]) -> FontSet {
    qpf::read(b, &QpfReadOptions::default(), &ResourceLimits::default())
        .unwrap()
        .0
}
#[test]
fn all_three_passes_signed_metrics_and_surrogates() {
    let wire = fixture();
    let s = read(&wire);
    let f = s.single().unwrap();
    assert_eq!(f.metrics.line_gap, Some(-1));
    let g = f.glyph_for_codepoint(Codepoint::Unicode(66)).unwrap();
    assert_eq!(g.metrics.bounds.x, -1);
    assert_eq!(g.metrics.bounds.y, 1);
    assert_eq!(g.bitmap.as_bytes(), [0xa0, 0x60]);
    assert_eq!(g.metrics.advance, Some(Vector::new(4, 0)));
    assert!(f.glyph_for_codepoint(Codepoint::Legacy(0xd800)).is_some());
    let (out, r) = qpf::write(&s, &QpfWriteOptions::default()).unwrap();
    assert_eq!(out, wire);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn canonical_regeneration_and_external_family_loss() {
    let mut s = read(&fixture());
    s.fonts[0].properties.remove("qpf.source");
    s.fonts[0].names.family = Some("External filename family".into());
    let (out, r) = qpf::write(&s, &QpfWriteOptions::default()).unwrap();
    assert!(r.has_loss());
    let d = read(&out);
    assert_eq!(
        d.single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Unicode(66))
            .unwrap()
            .bitmap,
        s.single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Unicode(66))
            .unwrap()
            .bitmap
    );
}
#[test]
fn gray8_padded_rows_and_explicit_threshold() {
    let mut b = vec![
        2, 0, 0, 0, 3, 0, 1, 0, 0, 0, 0, 65, 0, 65, 0, 4, 3, 2, 0, 0, 3, 2, 0,
    ];
    b.extend_from_slice(&[0, 127, 255, 9, 1, 128, 254, 8]);
    let s = read(&b);
    assert_eq!(
        s.single()
            .unwrap()
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes(),
        [0, 127, 255, 1, 128, 254]
    );
    let (out, _) = qpf::write(
        &s,
        &QpfWriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(out[6] & 1, 0);
    assert_eq!(
        read(&out)
            .single()
            .unwrap()
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes(),
        [0x20, 0x60]
    );
}
#[test]
fn malformed_tree_ranges_stride_and_all_truncations() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(
            qpf::read(&b[..n], &Default::default(), &Default::default()).is_err(),
            "prefix {n}"
        );
    }
    for (at, v) in [(10, 255), (20, 0), (25, 0)] {
        let mut bad = b.clone();
        bad[at] = v;
        assert!(
            qpf::read(&bad, &Default::default(), &Default::default()).is_err(),
            "mutation {at}"
        );
    }
}
#[test]
fn cumulative_limits_and_default_reader_contract() {
    let b = fixture();
    let s = read(&b);
    for n in [0, 100, 1000, 4096, 10000, 100000] {
        let o = QpfWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: n,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok((out, _)) = qpf::write(&s, &o) {
            assert!(qpf::read(&out, &Default::default(), &o.limits).is_ok());
        }
    }
    let mut s = s;
    s.fonts[0]
        .properties
        .push(Property::new("unsupported", 1i64));
    assert!(qpf::write(&s, &Default::default()).unwrap().1.has_loss());
}

#[test]
fn strict_registry_rejects_unrepresented_metadata_and_removed_slots() {
    let mut set = read(&fixture());
    set.fonts[0]
        .properties
        .push(Property::new("application-note", "must report"));
    let id = set.fonts[0]
        .add_glyph(fontpixel::model::Glyph::default())
        .unwrap();
    set.fonts[0].remove_glyph(id);
    let mut options =
        fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("out.qpf").unwrap());
    options.strict = true;
    assert!(matches!(
        fontpixel::write(&set, "qpf", &options),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
