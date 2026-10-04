//! Original Amiga hunk fixture and descriptor/resource tests.
#![cfg(feature = "amiga")]
use fontpixel::formats::amiga::{self, AmigaWriteOptions, Packaging};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{ResourceLimits, WriteOptions};
fn p16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_be_bytes());
}
fn p32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_be_bytes());
}
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 172];
    for (at, v) in [
        (0, 0x3f3),
        (8, 1),
        (20, 34),
        (24, 0x3e9),
        (28, 34),
        (32, 0x70ff4e75),
        (168, 0x3f2),
    ] {
        p32(&mut b, at, v);
    }
    let h = &mut b[32..168];
    p16(h, 18, 0xf80);
    h[26..30].copy_from_slice(b"Wire");
    p16(h, 78, 3);
    h[81] = 0x62;
    p16(h, 82, 4);
    p16(h, 84, 1);
    p16(h, 86, 1);
    h[90] = 65;
    h[91] = 65;
    p32(h, 92, 110);
    p16(h, 96, 2);
    p32(h, 98, 116);
    p32(h, 102, 124);
    p32(h, 106, 128);
    h[110..116].copy_from_slice(&[0xa0, 0, 0x40, 0, 0xe0, 0]);
    for (at, v) in [
        (116, 0),
        (118, 3),
        (120, 3),
        (122, 0),
        (124, 4),
        (126, 0),
        (128, 65535),
    ] {
        p16(h, at, v);
    }
    b
}
fn load(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    Ok(amiga::read(&mut ReadContext::new(&input, l)?, &Default::default())?.0)
}
fn options(name: &str) -> WriteOptions {
    WriteOptions::new(ResourcePath::new(name).unwrap())
}
#[test]
fn independent_hunk_offsets_baseline_default_and_roundtrip() {
    let s = load(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 2);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), [0xa0, 0x40, 0xe0]);
    assert_eq!(g.metrics.bounds.x, -1);
    assert_eq!(g.metrics.bounds.y, -1);
    assert_eq!(g.metrics.advance.unwrap().x, 4);
    assert!(
        f.glyph(f.metrics.default_glyph.unwrap())
            .unwrap()
            .codepoints
            .is_empty()
    );
    let w = amiga::write(
        &s,
        &options("3"),
        &AmigaWriteOptions {
            packaging: Packaging::Strike,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    assert_eq!(
        load(&w.output.into_single().unwrap(), &Default::default()).unwrap(),
        s
    );
}
struct Resolver(Vec<(String, Vec<u8>)>);
impl ResourceResolver for Resolver {
    fn resolve(&self, p: &ResourcePath, n: usize) -> fontpixel::Result<Vec<u8>> {
        let b = &self
            .0
            .iter()
            .find(|(s, _)| s == p.as_str())
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .1;
        if b.len() > n {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fixture",
                limit: n,
            });
        }
        Ok(b.clone())
    }
}
#[test]
fn tagged_multistrike_bundle_and_shared_budgets() {
    let mut s = load(&fixture(), &Default::default()).unwrap();
    s.fonts[0].metrics.resolution = Some(fontpixel::model::Vector::new(72, 144));
    s.fonts.push(s.fonts[0].clone());
    let w = amiga::write(&s, &options("wire.font"), &Default::default()).unwrap();
    assert_eq!(w.output.assets().len(), 3);
    let primary = w
        .output
        .assets()
        .iter()
        .find(|a| a.path.as_str() == "wire.font")
        .unwrap()
        .bytes
        .clone();
    let resolver = Resolver(
        w.output
            .into_assets()
            .into_iter()
            .filter(|a| a.path.as_str() != "wire.font")
            .map(|a| (a.path.to_string(), a.bytes))
            .collect(),
    );
    let input = Input::new(&primary).with_resources(&resolver);
    let l = ResourceLimits::default();
    let decoded = amiga::read(
        &mut ReadContext::new(&input, &l).unwrap(),
        &Default::default(),
    )
    .unwrap()
    .0;
    assert_eq!(decoded.fonts.len(), 2);
    assert_eq!(
        decoded.fonts[1].metrics.resolution,
        s.fonts[1].metrics.resolution
    );
    let w = amiga::write(&decoded, &options("wire.font"), &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    let l = ResourceLimits {
        max_glyphs: 3,
        ..Default::default()
    };
    assert!(
        amiga::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &Default::default()
        )
        .is_err()
    );
}
#[test]
fn malformed_hunks_resources_and_all_truncations() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(load(&b[..n], &Default::default()).is_err(), "{n}");
    }
    for i in 0..b.len() {
        for v in [0, 255] {
            let mut bad = b.clone();
            bad[i] = v;
            let _ = load(
                &bad,
                &ResourceLimits {
                    max_glyphs: 300,
                    max_total_bytes: 100000,
                    ..Default::default()
                },
            );
        }
    }
    let mut contents = vec![0; 264];
    p16(&mut contents, 0, 0xf00);
    p16(&mut contents, 2, 1);
    contents[4..8].copy_from_slice(b"../x");
    assert!(load(&contents, &Default::default()).is_err());
}

#[test]
fn independent_color_planes_use_coverage_and_retain_source_indices() {
    let plain = fixture();
    let mut data = plain[32..142].to_vec();
    data.resize(202, 0);
    data[80] = 64;
    p16(&mut data, 110, 1);
    data[112] = 2;
    data[113] = 255;
    data[115] = 3;
    data[116] = 255;
    p32(&mut data, 118, 154);
    p32(&mut data, 122, 170);
    p32(&mut data, 126, 176);
    p16(&mut data, 156, 4);
    p32(&mut data, 158, 162);
    for (i, v) in [0, 0xf00, 0x0f0, 0xfff].into_iter().enumerate() {
        p16(&mut data, 162 + i * 2, v);
    }
    data[170..182].copy_from_slice(&[0x40, 0, 0xa0, 0, 0xe0, 0, 0x20, 0, 0xc0, 0, 0x60, 0]);
    p32(&mut data, 92, 170);
    p32(&mut data, 98, 182);
    p32(&mut data, 102, 190);
    p32(&mut data, 106, 194);
    p16(&mut data, 184, 3);
    p16(&mut data, 186, 3);
    p16(&mut data, 190, 4);
    p16(&mut data, 194, 65535);
    data.resize(204, 0);
    let mut b = plain[..32].to_vec();
    p32(&mut b, 20, 51);
    p32(&mut b, 28, 51);
    b.extend(data);
    b.extend(0x3f2u32.to_be_bytes());
    let s = load(&b, &Default::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(
        g.bitmap.as_bytes(),
        [0, 76, 150, 255, 150, 76, 76, 255, 255]
    );
    assert!(g.properties.get("amiga.color_indices").is_some());
    let w = amiga::write(&s, &options("3"), &Default::default()).unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    assert_eq!(
        load(&w.output.into_single().unwrap(), &Default::default()).unwrap(),
        s
    );
}

proptest::proptest! {
 #[test]
 fn random_packed_rows_roundtrip(widths in proptest::collection::vec(0u32..16,1..15),height in 1u32..10,seed in proptest::num::u32::ANY){
  let mut state=seed;let mut glyphs=Vec::new();for(i,w)in widths.iter().enumerate(){let bytes:Vec<_>=(0..(*w as usize).div_ceil(8)*height as usize).map(|_|{state=state.wrapping_mul(1664525).wrapping_add(1013904223);(state>>24)as u8}).collect();let mut g=fontpixel::model::Glyph::new(fontpixel::bitmap::Bitmap::<fontpixel::bitmap::Mono>::from_raw(*w,height,&bytes,Default::default()).unwrap());g.codepoints.push(Codepoint::Legacy(32+i as u32));g.metrics.advance=Some(fontpixel::model::Vector::new(i64::from(*w),0));glyphs.push(g);}
  let s=FontSet::from(fontpixel::Font::from_glyphs(glyphs).unwrap());let w=amiga::write(&s,&options("3"),&Default::default()).unwrap();let s=load(&w.output.into_single().unwrap(),&Default::default()).unwrap();let w=amiga::write(&s,&options("3"),&Default::default()).unwrap();proptest::prop_assert!(!w.report.has_loss(),"{}",w.report);proptest::prop_assert_eq!(load(&w.output.into_single().unwrap(),&Default::default()).unwrap(),s);
 }
}

#[test]
fn cross_hunk_relocations_short_relocations_and_bss_are_data_only() {
    let expected = load(&fixture(), &Default::default()).unwrap();
    for short in [false, true] {
        let raw = fixture();
        let data = &raw[32..168];
        let mut b = Vec::new();
        for v in [0x3f3u32, 0, 2, 0, 1, 34, 34, 0x3e9, 34] {
            b.extend(v.to_be_bytes());
        }
        b.extend(data);
        b.extend(if short { 0x3fcu32 } else { 0x3ec }.to_be_bytes());
        if short {
            for v in [4u16, 1, 92, 98, 102, 106, 0, 0] {
                b.extend(v.to_be_bytes());
            }
        } else {
            for v in [4u32, 1, 92, 98, 102, 106, 0] {
                b.extend(v.to_be_bytes());
            }
        }
        b.extend(0x3f2u32.to_be_bytes());
        b.extend(0x3eau32.to_be_bytes());
        b.extend(34u32.to_be_bytes());
        b.extend(data);
        b.extend(0x3f2u32.to_be_bytes());
        assert_eq!(load(&b, &Default::default()).unwrap(), expected);
    }
    let raw = fixture();
    let mut b = Vec::new();
    for v in [0x3f3u32, 0, 2, 0, 1, 34, 34, 0x3e9, 34] {
        b.extend(v.to_be_bytes());
    }
    b.extend(&raw[32..168]);
    for v in [0x3f2u32, 0x3eb, 34, 0x3f2] {
        b.extend(v.to_be_bytes());
    }
    assert_eq!(load(&b, &Default::default()).unwrap(), expected);
}

#[test]
fn partial_omissions_keep_strike_coordinates_on_failure() {
    let mut s = load(&fixture(), &Default::default()).unwrap();
    assert!(
        amiga::write(
            &s,
            &options("3"),
            &AmigaWriteOptions {
                color_depth: Some(0),
                ..Default::default()
            }
        )
        .is_err()
    );
    s.fonts.push(s.fonts[0].clone());
    let id = s.fonts[1].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[1]
        .edit_glyph(id, |g| {
            g.codepoints.push(Codepoint::Unicode(0x10000));
            g.metrics.bounds.x = 40000;
            Ok(())
        })
        .unwrap();
    let error = amiga::write(&s, &options("font.font"), &Default::default()).unwrap_err();
    let report = error.report().unwrap();
    assert!(
        report.entries.iter().any(
            |e| e.code.ends_with("unrepresentable_mapping") && e.location.font_index == Some(1)
        ),
        "{report}"
    );
}
