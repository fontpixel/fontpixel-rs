//! Independent wire fixtures and bounded codec roundtrips.
#![cfg(feature = "gf")]
use fontpixel::ResourceLimits;
use fontpixel::formats::gf::{self, GfWriteOptions};
use fontpixel::model::Codepoint;
fn fixture() -> Vec<u8> {
    let mut v = vec![247, 131, 0];
    let at = v.len();
    v.push(67);
    for n in [65i32, -1, -1, 2, 0, 2] {
        v.extend_from_slice(&n.to_be_bytes())
    }
    v.extend_from_slice(&[0, 3, 70, 0, 1, 1, 1, 70, 0, 3, 69]);
    let boundary = v.len();
    let post = v.len();
    v.push(248);
    for n in [
        boundary as u32,
        10 << 20,
        17,
        65536,
        65536,
        u32::MAX,
        2,
        0,
        2,
    ] {
        v.extend_from_slice(&n.to_be_bytes())
    }
    v.extend_from_slice(&[246, 65, 4]);
    v.extend_from_slice(&0x80000u32.to_be_bytes());
    v.extend_from_slice(&(at as u32).to_be_bytes());
    v.push(249);
    v.extend_from_slice(&(post as u32).to_be_bytes());
    v.extend_from_slice(&[131, 223, 223, 223, 223]);
    v
}
#[test]
fn independent_commands_and_metrics() {
    let l = ResourceLimits::default();
    let (f, r) = gf::read(&fixture(), &l).unwrap();
    assert!(!r.has_loss());
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[0xe0, 0xa0, 0xe0]);
    assert_eq!(g.metrics.bounds.width, 3);
    assert_eq!(g.metrics.bounds.x, -1);
    assert_eq!(g.metrics.advance.unwrap().x, 4);
    let (out, r) = gf::write(&f, &GfWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(gf::read(&out, &l).unwrap().0, f);
}
#[test]
fn truncated_stream_and_invalid_pointer() {
    let v = fixture();
    for n in 0..v.len() {
        assert!(
            gf::read(&v[..n], &ResourceLimits::default()).is_err(),
            "length {n}"
        )
    }
    let mut v = v;
    v[8] = 0;
    assert!(gf::read(&v, &ResourceLimits::default()).is_err());
}
#[test]
fn bounds_and_run_limits() {
    let mut v = fixture();
    v[29] = 4;
    assert!(gf::read(&v, &ResourceLimits::default()).is_err());
    let l = ResourceLimits {
        max_pixels_per_glyph: 8,
        ..Default::default()
    };
    assert!(gf::read(&fixture(), &l).is_err())
}
#[test]
fn residue_metric_conflicts_and_alias_identity() {
    let (mut f, _) = gf::read(&fixture(), &ResourceLimits::default()).unwrap();
    let mut g = f.glyphs().next().unwrap().1.clone();
    g.codepoints = vec![Codepoint::Legacy(321)];
    g.metrics.advance.as_mut().unwrap().x = 5;
    f.add_glyph(g).unwrap();
    let (out, r) = gf::write(&f, &GfWriteOptions::default()).unwrap();
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "gf.shared_metric" && e.location.codepoint == Some(321))
    );
    assert_eq!(
        gf::read(&out, &ResourceLimits::default()).unwrap().0.len(),
        1
    );
}
#[test]
fn shared_residue_backpointers() {
    let (mut f, _) = gf::read(&fixture(), &ResourceLimits::default()).unwrap();
    let mut g = f.glyphs().next().unwrap().1.clone();
    g.codepoints = vec![Codepoint::Legacy(321)];
    f.add_glyph(g).unwrap();
    let (out, r) = gf::write(&f, &GfWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(gf::read(&out, &ResourceLimits::default()).unwrap().0, f);
}

#[test]
fn special_ownership_and_pointer_command_boundaries() {
    let mut v = vec![247, 131, 0, 239, 2, 244, 244];
    v.push(67);
    for n in [65i32, -1, 0, 3, 0, 0] {
        v.extend_from_slice(&n.to_be_bytes());
    }
    v.extend_from_slice(&[0, 3, 69]);
    let boundary = v.len();
    v.extend_from_slice(&[239, 6]);
    v.extend_from_slice(b"global");
    let post = v.len();
    v.push(248);
    for n in [boundary as u32, 10 << 20, 0, 65536, 65536, 0, 3, 0, 0] {
        v.extend_from_slice(&n.to_be_bytes());
    }
    v.extend_from_slice(&[246, 65, 4]);
    v.extend_from_slice(&0x80000u32.to_be_bytes());
    let ptr = v.len();
    v.extend_from_slice(&3u32.to_be_bytes());
    v.push(249);
    v.extend_from_slice(&(post as u32).to_be_bytes());
    v.extend_from_slice(&[131, 223, 223, 223, 223]);
    let (f, _) = gf::read(&v, &ResourceLimits::default()).unwrap();
    assert!(f.properties.get("gf.special").is_some());
    assert!(
        f.glyphs()
            .next()
            .unwrap()
            .1
            .properties
            .get("gf.special")
            .is_some()
    );
    let (out, r) = gf::write(&f, &GfWriteOptions::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    assert_eq!(gf::read(&out, &ResourceLimits::default()).unwrap().0, f);
    v[ptr..ptr + 4].copy_from_slice(&5u32.to_be_bytes());
    assert!(gf::read(&v, &ResourceLimits::default()).is_err());
}
