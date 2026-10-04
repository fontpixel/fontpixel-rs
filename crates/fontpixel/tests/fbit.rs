//! Original native fbit and HFNT fixtures.
#![cfg(feature = "fbit")]
use fontpixel::formats::fbit::{self, Definition, FbitReadOptions, FbitWriteOptions, ResourceKind};
use fontpixel::formats::nfnt::Container;
use fontpixel::model::Codepoint;
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{FontSet, ResourceLimits, Result};
fn wire() -> Vec<u8> {
    let mut b = vec![0; 44];
    b[2..6].copy_from_slice(b"fbit");
    b[6..8].copy_from_slice(&128u16.to_be_bytes());
    b[24..26].copy_from_slice(&3u16.to_be_bytes());
    b[26..28].copy_from_slice(&3u16.to_be_bytes());
    b[32..34].copy_from_slice(&0x817eu16.to_be_bytes());
    b[34..36].copy_from_slice(&0x8180u16.to_be_bytes());
    b[36..38].copy_from_slice(&1u16.to_be_bytes());
    b[38..40].copy_from_slice(&62u16.to_be_bytes());
    b[40..42].copy_from_slice(&63u16.to_be_bytes());
    b[42..44].copy_from_slice(&2u16.to_be_bytes());
    b.extend_from_slice(&[0xaf, 0x80, 0x72, 0]);
    b
}
fn read(b: &[u8]) -> FontSet {
    fbit::read(b, &Default::default(), &Default::default())
        .unwrap()
        .0
}
#[test]
fn continuous_cell_bits_and_shiftjis_discontinuity() {
    let b = wire();
    let s = read(&b);
    let f = s.single().unwrap();
    assert_eq!(f.len(), 2);
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Legacy(0x817e))
            .unwrap()
            .bitmap
            .as_bytes(),
        [0xa0, 0x60, 0xe0]
    );
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Legacy(0x8180))
            .unwrap()
            .bitmap
            .as_bytes(),
        [0x60, 0x80, 0x80]
    );
    let (out, r) = fbit::write(&s, &Default::default()).unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn hfnt_native_codes_and_explicit_width() {
    let mut b = vec![0, 1, 0, 2, 0, 3, 0, 4, 0, 4, 0, 65, 0x81, 0x40, 0, 0];
    b.extend_from_slice(&[
        0x81, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x81, 255, 0, 255, 0, 255, 0, 255, 0,
    ]);
    let o = FbitReadOptions {
        kind: ResourceKind::Hfnt,
        ..Default::default()
    };
    let (s, _) = fbit::read(&b, &o, &Default::default()).unwrap();
    let (out, r) = fbit::write(
        &s,
        &FbitWriteOptions {
            kind: ResourceKind::Hfnt,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
    let mut wider = o;
    wider.hfnt_width = Some(16);
    let (custom, _) = fbit::read(&b, &wider, &Default::default()).unwrap();
    assert_eq!(
        custom.fonts[0].glyphs().next().unwrap().1.bitmap.width(),
        16
    );
}
struct Resolver(Vec<u8>);
impl ResourceResolver for Resolver {
    fn resolve(&self, _: &ResourcePath, _: usize) -> Result<Vec<u8>> {
        Ok(self.0.clone())
    }
}
#[test]
fn both_native_external_fdefs_bundle_and_inline_conversion() {
    let source = read(&wire());
    for definition in [Definition::DataFork5, Definition::DataFork7] {
        let path = ResourcePath::new("font.fbit").unwrap();
        let o = FbitWriteOptions {
            definition,
            ..Default::default()
        };
        let written = fbit::write_bundle(&source, &o, &path).unwrap();
        assert_eq!(written.output.assets().len(), 2);
        assert!(fbit::write(&source, &o).is_err());
        let primary = written
            .output
            .assets()
            .iter()
            .find(|a| a.path == path)
            .unwrap();
        let data = written
            .output
            .assets()
            .iter()
            .find(|a| a.path != path)
            .unwrap();
        let resolver = Resolver(data.bytes.clone());
        let input = Input::new(&primary.bytes).with_resources(&resolver);
        let l = ResourceLimits::default();
        let mut context = ReadContext::new(&input, &l).unwrap();
        let opts = FbitReadOptions {
            data_fork: Some(data.path.clone()),
            ..Default::default()
        };
        let (parsed, _) = fbit::read_context(&mut context, &opts).unwrap();
        assert_eq!(parsed.fonts[0].len(), 2);
        assert!(fbit::read(&primary.bytes, &Default::default(), &l).is_err());
        let (inline, r) = fbit::write(&parsed, &Default::default()).unwrap();
        assert!(r.has_loss());
        assert_eq!(read(&inline).fonts[0].len(), 2);
    }
}
#[test]
fn all_native_envelopes_keep_resource_fields() {
    let s = read(&wire());
    for container in [
        Container::ResourceFork,
        Container::MacBinary,
        Container::AppleSingle,
        Container::BinHex,
    ] {
        let o = FbitWriteOptions {
            container,
            ..Default::default()
        };
        let (b, _) = fbit::write(&s, &o).unwrap();
        let parsed = read(&b);
        assert_eq!(parsed.fonts[0].len(), 2);
        let (out, r) = fbit::write(&parsed, &o).unwrap();
        assert!(!r.has_loss(), "{container:?}: {r}");
        assert_eq!(read(&out), parsed);
    }
}
#[test]
fn envelope_type_precedes_filename_tag_and_explicit_bare_kind() {
    let s = read(&wire());
    let path = ResourcePath::new("fbit-macbinary.bin").unwrap();
    let written = fbit::write_bundle(
        &s,
        &FbitWriteOptions {
            container: Container::MacBinary,
            ..Default::default()
        },
        &path,
    )
    .unwrap();
    let data = &written.output.assets()[0].bytes;
    assert_eq!(&data[2..6], b"fbit");
    assert_ne!(fbit::detect(data), fontpixel::formats::Confidence::NoMatch);
    for kind in [ResourceKind::Fbit, ResourceKind::Hfnt] {
        let (parsed, _) = fbit::read(
            data,
            &FbitReadOptions {
                kind,
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
        assert_eq!(parsed.fonts[0].len(), 2);
        assert_eq!(
            parsed.fonts[0].glyphs().next().unwrap().1.bitmap,
            s.fonts[0].glyphs().next().unwrap().1.bitmap
        );
    }
    // HFNT's first three words are opaque, and may themselves spell fbit.
    let mut raw = vec![0, 0, b'f', b'b', b'i', b't', 0, 4, 0, 2, 0, 65, 0, 0];
    raw.extend_from_slice(&[0x81; 8]);
    let (hfnt, _) = fbit::read(
        &raw,
        &FbitReadOptions {
            kind: ResourceKind::Hfnt,
            ..Default::default()
        },
        &Default::default(),
    )
    .unwrap();
    let (out, report) = fbit::write(
        &hfnt,
        &FbitWriteOptions {
            kind: ResourceKind::Hfnt,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(out, raw);
    assert!(!report.has_loss(), "{report}");
}
#[test]
fn all_truncations_runs_unknown_code_and_aggregate_limits() {
    let b = wire();
    for n in 0..b.len() {
        assert!(
            fbit::read(&b[..n], &Default::default(), &Default::default()).is_err(),
            "prefix {n}"
        );
    }
    for (at, v) in [(11, 9), (40, 0xff), (43, 3), (26, 0xff)] {
        let mut bad = b.clone();
        bad[at] = v;
        assert!(fbit::read(&bad, &Default::default(), &Default::default()).is_err());
    }
    let s = read(&b);
    for max in [0, 1000, 4096, 10000, 100000] {
        let o = FbitWriteOptions {
            limits: ResourceLimits {
                max_total_bytes: max,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok((out, _)) = fbit::write(&s, &o) {
            assert!(fbit::read(&out, &Default::default(), &o.limits).is_ok());
        }
    }
}
#[test]
fn appledouble_pair_resolves_explicitly_and_preserves_external_rasters() {
    let source = read(&wire());
    let path = ResourcePath::new("fbit-double").unwrap();
    let written = fbit::write_bundle(
        &source,
        &FbitWriteOptions {
            container: Container::AppleDouble,
            definition: Definition::DataFork7,
            ..Default::default()
        },
        &path,
    )
    .unwrap();
    assert_eq!(written.output.assets().len(), 2);
    let data = written
        .output
        .assets()
        .iter()
        .find(|a| a.path == path)
        .unwrap();
    let resource = written
        .output
        .assets()
        .iter()
        .find(|a| a.path != path)
        .unwrap();
    let resolver = Resolver(resource.bytes.clone());
    let input = Input::new(&data.bytes).with_resources(&resolver);
    let limits = ResourceLimits::default();
    let mut context = ReadContext::new(&input, &limits).unwrap();
    let (parsed, _) = fbit::read_context(
        &mut context,
        &FbitReadOptions {
            apple_double_resource: Some(resource.path.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        parsed.fonts[0].glyphs().next().unwrap().1.bitmap,
        source.fonts[0].glyphs().next().unwrap().1.bitmap
    );
    assert_eq!(
        parsed.properties.get("nfnt.data-fork"),
        Some(&fontpixel::model::PropertyValue::Bytes(data.bytes.clone()))
    );
}

#[test]
fn strict_registry_rejects_unrepresented_metadata_and_removed_slots() {
    let mut set = read(&wire());
    set.fonts[0]
        .properties
        .push(fontpixel::model::Property::new(
            "application-note",
            "must report",
        ));
    let id = set.fonts[0]
        .add_glyph(fontpixel::model::Glyph::default())
        .unwrap();
    set.fonts[0].remove_glyph(id);
    let mut options =
        fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("out.fbit").unwrap());
    options.strict = true;
    assert!(matches!(
        fontpixel::write(&set, "fbit", &options),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
