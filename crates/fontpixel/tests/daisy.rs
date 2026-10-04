//! Original MIT byte-authored daisy boundary and resource fixtures.
#![cfg(feature = "daisy")]
use fontpixel::formats::daisy::{self, DaisyWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = b"3\x9b".to_vec();
    for cp in 33..125 {
        if cp == 96 || cp == 123 {
            continue;
        }
        b.push(66);
        let mut g = [0; 8];
        if cp == 65 {
            g[0] = 0x80;
            g[1] = 0x40;
            g[6] = 0x20;
        }
        b.extend_from_slice(&g);
    }
    b.extend_from_slice(&[31, 30, 2]);
    b
}
fn read(b: &[u8], limits: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, limits)?;
    Ok(daisy::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.daisy").unwrap())
}
#[test]
fn original_fixture_pixels_and_native_roundtrip() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 91);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert!(g.bitmap.as_bytes().iter().any(|v| *v != 0));
    let w = daisy::write(&s, &output(), &Default::default()).unwrap();
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
        if let Ok(w) = daisy::write(
            &s,
            &output(),
            &DaisyWriteOptions {
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
    assert!(daisy::write(&s, &opts, &Default::default()).is_err());
    let w = daisy::write(&s, &output(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
}
struct Assets(Vec<fontpixel::resources::OutputAsset>);
impl fontpixel::resources::ResourceResolver for Assets {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = &self
            .0
            .iter()
            .find(|a| &a.path == p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .bytes;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test asset",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
#[test]
fn magnified_bundle_and_sidecar_limits() {
    use fontpixel::formats::daisy::{DaisyReadOptions, DaisyVersion};
    let s = read(&fixture(), &Default::default()).unwrap();
    let opts = DaisyWriteOptions {
        version: DaisyVersion::Magnified,
        height: Some(64),
        ..Default::default()
    };
    let primary = WriteOptions::new(ResourcePath::new("font.nlq").unwrap());
    let w = daisy::write(&s, &primary, &opts).unwrap();
    assert_eq!(w.output.assets().len(), 2);
    let assets = Assets(w.output.into_assets());
    let b = &assets
        .0
        .iter()
        .find(|a| a.path.as_str() == "font.nlq")
        .unwrap()
        .bytes;
    let input = Input::new(b).with_name("font.nlq").with_resources(&assets);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits).unwrap();
    let back = daisy::read(&mut c, &DaisyReadOptions::default()).unwrap().0;
    assert_eq!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .height(),
        64
    );
    let limits = ResourceLimits {
        max_resources: 0,
        ..Default::default()
    };
    let mut c = ReadContext::new(&input, &limits).unwrap();
    assert!(daisy::read(&mut c, &Default::default()).is_err());
    assert!(daisy::write(&s, &primary, &DaisyWriteOptions { limits, ..opts }).is_err());
    let input = Input::new(b).with_name("font.nlq");
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits).unwrap();
    assert!(daisy::read(&mut c, &Default::default()).is_err());
}
#[test]
fn graphics_control_bytes_require_explicit_write_option() {
    let mut b = fixture();
    b[3] = 13;
    let input = Input::new(&b);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits).unwrap();
    let (s, r) = daisy::read(&mut c, &Default::default()).unwrap();
    assert!(r.has_loss());
    assert!(daisy::write(&s, &output(), &Default::default()).is_err());
    assert!(
        daisy::write(
            &s,
            &output(),
            &DaisyWriteOptions {
                allow_control_bytes: true,
                ..Default::default()
            }
        )
        .is_ok()
    );
    b[2] = 0;
    assert!(read(&b, &limits).is_err());
}
