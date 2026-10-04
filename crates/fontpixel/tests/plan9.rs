//! Independent wire fixtures and bounded codec roundtrips.
#![cfg(feature = "plan9")]
use fontpixel::formats::plan9::{self, Container, Plan9WriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, OutputBundle, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{Error, ResourceLimits, Result};
use std::collections::BTreeMap;
fn field(v: impl ToString) -> Vec<u8> {
    format!("{:>11} ", v.to_string()).into_bytes()
}
fn fixture(compressed: bool, legacy: bool) -> Vec<u8> {
    let mut v = Vec::new();
    if compressed {
        v.extend_from_slice(b"compressed\n")
    }
    v.extend(field(if legacy { "0" } else { "k1" }));
    for n in [3, 1, 9, 4] {
        v.extend(field(n))
    }
    let mut raw = vec![0x1c, 0, 0x14, 0, 0x1c, 0];
    if legacy {
        for b in &mut raw {
            *b = !*b
        }
    }
    if compressed {
        v.extend(field(4));
        v.extend(field(9));
        for row in raw.as_chunks::<2>().0 {
            v.push(129);
            v.extend(row);
        }
    } else {
        v.extend(raw)
    }
    for n in [1, 4, 3] {
        v.extend(field(n))
    }
    v.extend_from_slice(&[3, 0, 1, 4, 255, 4, 6, 0, 0, 0, 0, 0]);
    v
}
struct Resources(BTreeMap<String, Vec<u8>>);
impl ResourceResolver for Resources {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let v = self
            .0
            .get(p.as_str())
            .ok_or_else(|| Error::MissingResource(p.to_string()))?;
        if v.len() > max {
            return Err(Error::ResourceLimit {
                resource: "fixture",
                limit: max,
            });
        }
        Ok(v.clone())
    }
}
fn read(
    b: &[u8],
    resources: &Resources,
    l: &ResourceLimits,
) -> Result<(fontpixel::Font, fontpixel::Report)> {
    let i = Input::new(b).with_resources(resources);
    let mut c = ReadContext::new(&i, l)?;
    plan9::read(&mut c)
}
fn bundle(b: &OutputBundle, l: &ResourceLimits) -> Result<(fontpixel::Font, fontpixel::Report)> {
    let assets = Resources(
        b.assets()
            .iter()
            .map(|a| (a.path.to_string(), a.bytes.clone()))
            .collect(),
    );
    let raw = &assets.0[b.primary().as_str()];
    read(raw, &assets, l)
}
#[test]
fn independent_images_coordinates_and_old_polarity() {
    let l = ResourceLimits::default();
    for compressed in [false, true] {
        for legacy in [false, true] {
            let (f, r) = read(
                &fixture(compressed, legacy),
                &Resources(BTreeMap::new()),
                &l,
            )
            .unwrap();
            assert!(!r.has_loss());
            let g = f.glyphs().next().unwrap().1;
            assert_eq!(g.bitmap.as_bytes(), &[0xe0, 0xa0, 0xe0]);
            assert_eq!(g.metrics.bounds.x, -1);
            assert_eq!(g.metrics.bounds.y, -1);
            let options = Plan9WriteOptions {
                container: Container::Subfont,
                ..Default::default()
            };
            let out = plan9::write(
                &FontSet::from(f.clone()),
                &ResourcePath::new("face.subfont").unwrap(),
                &options,
            )
            .unwrap();
            assert!(!out.report.has_loss(), "{}", out.report);
            assert_eq!(bundle(&out.output, &l).unwrap().0, f);
        }
    }
}
#[test]
fn manifest_multiple_resources_offsets_aliases_and_bundle() {
    let resources = Resources(BTreeMap::from([
        ("a".into(), fixture(false, false)),
        ("b".into(), fixture(true, false)),
    ]));
    let text = b"4 3\n0x41 0101 a\n0x42 0x42 0 b\n0x10000 0x10000 a\n";
    let (f, r) = read(text, &resources, &ResourceLimits::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(f.len(), 2);
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .codepoints,
        &[Codepoint::Unicode(65), Codepoint::Unicode(65536)]
    );
    let o = Plan9WriteOptions {
        max_glyphs_per_subfont: 1,
        ..Default::default()
    };
    let out = plan9::write(
        &FontSet::from(f),
        &ResourcePath::new("face.font").unwrap(),
        &o,
    )
    .unwrap();
    assert_eq!(out.output.assets().len(), 4);
    assert!(out.report.entries.iter().any(|e| e.code == "plan9.mapping"));
    let (round, _) = bundle(&out.output, &ResourceLimits::default()).unwrap();
    assert_eq!(round.len(), 3);
    let next = plan9::write(
        &FontSet::from(round.clone()),
        &ResourcePath::new("face.font").unwrap(),
        &o,
    )
    .unwrap();
    assert!(!next.report.has_loss(), "{}", next.report);
    assert_eq!(
        bundle(&next.output, &ResourceLimits::default()).unwrap().0,
        round
    );
}
#[test]
fn malformed_and_missing_resources() {
    let l = ResourceLimits::default();
    let resources = Resources(BTreeMap::new());
    for s in [
        b"4 3\n65 65 ../evil\n" as &[u8],
        b"4 3\n65 65 /evil\n",
        b"4 3\n65 65 missing\n",
        b"4 3\n0xd800 0xd800 a\n",
    ] {
        assert!(read(s, &resources, &l).is_err())
    }
    let f = fixture(false, false);
    for n in 0..f.len() {
        assert!(read(&f[..n], &resources, &l).is_err(), "length {n}");
    }
}
#[test]
fn bounded_lz77() {
    let mut v = fixture(true, false);
    let at = 11 + 60 + 24;
    v.splice(at..at + 9, [0, 0]);
    v[11 + 60 + 12..11 + 60 + 24].copy_from_slice(&field(2));
    assert!(read(&v, &Resources(BTreeMap::new()), &ResourceLimits::default()).is_err());
    let mut v = fixture(true, false);
    v[at] = 255;
    assert!(read(&v, &Resources(BTreeMap::new()), &ResourceLimits::default()).is_err());
}
#[test]
fn limits_cumulative() {
    let resources = Resources(BTreeMap::from([
        ("a".into(), fixture(false, false)),
        ("b".into(), fixture(false, false)),
    ]));
    let text = b"4 3\n65 65 a\n66 66 b\n";
    for l in [
        ResourceLimits {
            max_resources: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_resource_bytes: 10,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 1000,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 8,
            ..Default::default()
        },
    ] {
        assert!(read(text, &resources, &l).is_err())
    }
}
#[test]
fn grayscale_preserved() {
    let mut v = Vec::new();
    v.extend(field("k2"));
    for n in [0, 0, 4, 1] {
        v.extend(field(n))
    }
    v.push(0x1b);
    for n in [1, 1, 1] {
        v.extend(field(n))
    }
    v.extend_from_slice(&[0, 0, 0, 1, 0, 4, 4, 0, 0, 0, 0, 0]);
    let l = ResourceLimits::default();
    let (f, _) = read(&v, &Resources(BTreeMap::new()), &l).unwrap();
    assert_eq!(
        f.glyphs().next().unwrap().1.bitmap.as_bytes(),
        &[0, 85, 170, 255]
    );
    let o = Plan9WriteOptions {
        container: Container::Subfont,
        ..Default::default()
    };
    let out = plan9::write(&f.clone().into(), &ResourcePath::new("gray").unwrap(), &o).unwrap();
    assert!(!out.report.has_loss(), "{}", out.report);
    assert_eq!(bundle(&out.output, &l).unwrap().0, f);
}
#[test]
fn fatal_omission_report() {
    let (f, _) = read(
        &fixture(false, false),
        &Resources(BTreeMap::new()),
        &ResourceLimits::default(),
    )
    .unwrap();
    let err = plan9::write(
        &f.into(),
        &ResourcePath::new("out.font").unwrap(),
        &Plan9WriteOptions::default(),
    )
    .unwrap_err();
    assert!(
        err.report()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.code == "plan9.unrepresentable_mapping")
    );
}

#[test]
fn literal_compression_and_nonzero_extra_image_ink() {
    let l = ResourceLimits::default();
    let (f, _) = read(&fixture(false, false), &Resources(BTreeMap::new()), &l).unwrap();
    let o = Plan9WriteOptions {
        container: Container::Subfont,
        compression: plan9::Compression::Literal,
        ..Default::default()
    };
    let out = plan9::write(&f.clone().into(), &ResourcePath::new("font").unwrap(), &o).unwrap();
    assert!(!out.report.has_loss(), "{}", out.report);
    assert_eq!(bundle(&out.output, &l).unwrap().0, f);
    let mut bytes = fixture(false, false);
    bytes[60] |= 2;
    let (_, r) = read(&bytes, &Resources(BTreeMap::new()), &l).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "plan9.unmapped_image"));
}

#[test]
fn large_line_metrics_are_independent_of_fontchar_coordinates() {
    let mut wire = fixture(false, false);
    let header = 60 + 6;
    wire[header + 12..header + 24].copy_from_slice(&field(1000));
    wire[header + 24..header + 36].copy_from_slice(&field(800));
    let l = ResourceLimits::default();
    let (f, _) = read(&wire, &Resources(BTreeMap::new()), &l).unwrap();
    let options = Plan9WriteOptions {
        container: Container::Subfont,
        ..Default::default()
    };
    let out = plan9::write(
        &f.clone().into(),
        &ResourcePath::new("large").unwrap(),
        &options,
    )
    .unwrap();
    assert!(!out.report.has_loss(), "{}", out.report);
    assert_eq!(bundle(&out.output, &l).unwrap().0, f);
}
