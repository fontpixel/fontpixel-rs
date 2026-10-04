//! Original MIT byte-authored pxl boundary and resource fixtures.
#![cfg(feature = "pxl")]
use fontpixel::formats::pxl::{self, PxlWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 2084];
    b[..4].copy_from_slice(&1001u32.to_be_bytes());
    b[4] = 0xa0;
    b[8] = 0x40;
    b[12] = 0xe0;
    let p = 16 + 65 * 16;
    b[p..p + 16].copy_from_slice(&[0, 3, 0, 3, 0, 1, 0, 2, 0, 0, 0, 1, 0, 8, 0, 0]);
    let p = b.len() - 20;
    for (i, v) in [0x12345678u32, 1000, 10 << 20, 4, 1001].iter().enumerate() {
        b[p + i * 4..p + (i + 1) * 4].copy_from_slice(&v.to_be_bytes());
    }
    b
}
fn read(b: &[u8], limits: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, limits)?;
    Ok(pxl::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.pxl").unwrap())
}
#[test]
fn original_fixture_pixels_and_native_roundtrip() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 1);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert!(g.bitmap.as_bytes().iter().any(|v| *v != 0));
    let w = pxl::write(&s, &output(), &Default::default()).unwrap();
    let back = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    let h = back
        .single()
        .unwrap()
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.bitmap, h.bitmap);
    assert_eq!(g.metrics.bounds, h.metrics.bounds);
    assert_eq!(g.metrics.advance, h.metrics.advance);
}
#[test]
fn every_truncation_and_trailing_byte_rejected() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix {n}");
    }
    let mut bad = b;
    bad.push(0);
    assert!(read(&bad, &Default::default()).is_err());
}
#[test]
fn reader_budgets_and_writer_same_limits() {
    let b = fixture();
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
            max_total_bytes: b.len(),
            ..Default::default()
        },
    ] {
        assert!(read(&b, &limits).is_err());
    }
    let s = read(&b, &Default::default()).unwrap();
    let mut successes = 0;
    for cap in [0, 1024, 65536, 262144, 1048576] {
        let limits = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = pxl::write(
            &s,
            &output(),
            &PxlWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
            successes += 1;
        }
    }
    assert!(successes > 0);
}
#[test]
fn strict_rejects_unsupported_metadata() {
    let mut s = read(&fixture(), &Default::default()).unwrap();
    s.fonts[0].names.copyright = Some("original test metadata".into());
    let mut opts = output();
    opts.strict = true;
    assert!(pxl::write(&s, &opts, &Default::default()).is_err());
    let w = pxl::write(&s, &output(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
}
struct Tfm(Vec<u8>);
impl fontpixel::resources::ResourceResolver for Tfm {
    fn resolve(&self, _: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        if self.0.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "TFM test",
                limit: max,
            });
        }
        Ok(self.0.clone())
    }
}
#[test]
fn matching_tfm_sidecar_and_validation() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let path = ResourcePath::new("font.tfm").unwrap();
    let opts = PxlWriteOptions {
        tfm: Some(path.clone()),
        ..Default::default()
    };
    let w = pxl::write(&s, &output(), &opts).unwrap();
    let primary = w
        .output
        .assets()
        .iter()
        .find(|a| a.path.as_str() == "font.pxl")
        .unwrap()
        .bytes
        .clone();
    let tfm = Tfm(w
        .output
        .assets()
        .iter()
        .find(|a| a.path == path)
        .unwrap()
        .bytes
        .clone());
    let l = ResourceLimits::default();
    let input = Input::new(&primary).with_resources(&tfm);
    let mut c = ReadContext::new(&input, &l).unwrap();
    let set = pxl::read(
        &mut c,
        &pxl::PxlReadOptions {
            tfm: Some(path.clone()),
            ..Default::default()
        },
    )
    .unwrap()
    .0;
    let g = set.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.metrics.advance.unwrap().x, 14);
    assert_eq!(g.metrics.scalable_advance.unwrap().x, 500);
    let mut bad = tfm.0;
    bad[24] ^= 1;
    let resolver = Tfm(bad);
    let input = Input::new(&primary).with_resources(&resolver);
    let mut c = ReadContext::new(&input, &l).unwrap();
    assert!(
        pxl::read(
            &mut c,
            &pxl::PxlReadOptions {
                tfm: Some(path),
                ..Default::default()
            }
        )
        .is_err()
    );
    let opts = PxlWriteOptions {
        limits: ResourceLimits {
            max_resources: 0,
            ..Default::default()
        },
        ..opts
    };
    assert!(pxl::write(&s, &output(), &opts).is_err());
}
#[test]
fn both_alignments_and_pointer_validation() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let w = pxl::write(
        &s,
        &output(),
        &PxlWriteOptions {
            version: pxl::PxlVersion::V1002,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(read(&w.output.into_single().unwrap(), &Default::default()).is_ok());
    for at in [0, 16 + 65 * 16 + 8, 2084 - 8, 2084 - 4] {
        let mut b = fixture();
        b[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(read(&b, &Default::default()).is_err());
    }
    let mut b = fixture();
    b[16 + 65 * 16 + 4..16 + 65 * 16 + 6].copy_from_slice(&(-2i16).to_be_bytes());
    assert_eq!(
        read(&b, &Default::default()).unwrap().fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .metrics
            .bounds
            .x,
        2
    );
}
#[test]
fn tfm_zero_width_is_present_and_zero_checksum_is_wildcard() {
    let mut s = read(&fixture(), &Default::default()).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.scalable_advance = Some(fontpixel::model::Vector::new(0, 0));
            g.metrics.advance = Some(fontpixel::model::Vector::new(0, 0));
            Ok(())
        })
        .unwrap();
    let path = ResourcePath::new("font.tfm").unwrap();
    let out = pxl::write(
        &s,
        &output(),
        &PxlWriteOptions {
            tfm: Some(path.clone()),
            ..Default::default()
        },
    )
    .unwrap()
    .output;
    let primary = &out
        .assets()
        .iter()
        .find(|a| a.path.as_str() == "font.pxl")
        .unwrap()
        .bytes;
    let mut t = out
        .assets()
        .iter()
        .find(|a| a.path == path)
        .unwrap()
        .bytes
        .clone();
    assert_ne!(
        t[32 + 65 * 4],
        0,
        "TFM width index zero denotes missing, not zero advance"
    );
    t[24..28].fill(0);
    let tfm = Tfm(t);
    let input = Input::new(primary).with_resources(&tfm);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    let back = pxl::read(
        &mut c,
        &pxl::PxlReadOptions {
            tfm: Some(path),
            ..Default::default()
        },
    )
    .unwrap()
    .0;
    assert_eq!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .metrics
            .advance
            .unwrap()
            .x,
        0
    );
}
