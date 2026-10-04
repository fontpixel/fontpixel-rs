//! Original BMFont variants, page boundaries and channel coverage cases.
#![cfg(feature = "bmfont")]
use fontpixel::bitmap::{BitmapLimits, Gray8};
use fontpixel::formats::bmfont::{self, BmfontEncoding, BmfontReadOptions, BmfontWriteOptions};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::resources::{Input, OutputBundle, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{Bitmap, Font, Glyph, ResourceLimits, Result};
struct Pages(std::collections::BTreeMap<String, Vec<u8>>);
impl ResourceResolver for Pages {
    fn resolve(&self, p: &ResourcePath, n: usize) -> Result<Vec<u8>> {
        let b = self
            .0
            .get(p.as_str())
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
        if b.len() > n {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test",
                limit: n,
            });
        }
        Ok(b.clone())
    }
}
fn load(bundle: &OutputBundle) -> Font {
    let primary = bundle
        .assets()
        .iter()
        .find(|a| &a.path == bundle.primary())
        .unwrap();
    let pages = Pages(
        bundle
            .assets()
            .iter()
            .map(|a| (a.path.as_str().to_owned(), a.bytes.clone()))
            .collect(),
    );
    let input = Input::new(&primary.bytes).with_resources(&pages);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits).unwrap();
    bmfont::read(&mut c, &BmfontReadOptions::default())
        .unwrap()
        .0
}
fn font() -> Font {
    let mut f = Font::default();
    f.names.family = Some("A & B".into());
    f.metrics.ascent = Some(4);
    f.metrics.line_height = Some(7);
    f.metrics.pixel_size = Some(6);
    for cp in 65..68 {
        let b =
            Bitmap::<Gray8>::from_raw(3, 2, &[0, 127, 255, 255, 128, 0], BitmapLimits::default())
                .unwrap();
        let mut g = Glyph::new(b);
        g.codepoints.push(Codepoint::Unicode(cp));
        g.metrics.bounds.x = -1;
        g.metrics.bounds.y = -2;
        g.metrics.advance = Some(Vector::new(5, 0));
        f.add_glyph(g).unwrap();
    }
    f
}
#[test]
fn failed_write_retains_every_glyph_and_alias_location() {
    let mut f = font();
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(1000));
        Ok(())
    })
    .unwrap();
    let error = bmfont::write(
        &f,
        &ResourcePath::new("failed.fnt").unwrap(),
        &BmfontWriteOptions {
            page_width: 2,
            ..Default::default()
        },
    )
    .unwrap_err();
    let r = error.report().unwrap();
    assert!(r.has_errors());
    let locations: Vec<_> = r
        .entries
        .iter()
        .filter(|e| e.code == "bmfont.glyph_omitted")
        .map(|e| (e.location.glyph_index, e.location.codepoint))
        .collect();
    assert_eq!(
        locations,
        [
            (Some(0), Some(65)),
            (Some(0), Some(1000)),
            (Some(1), Some(66)),
            (Some(2), Some(67))
        ]
    );
}
#[test]
fn all_encodings_multipage_semantic_roundtrip() {
    for encoding in [
        BmfontEncoding::Text,
        BmfontEncoding::Xml,
        BmfontEncoding::Binary,
        BmfontEncoding::Json,
    ] {
        let o = BmfontWriteOptions {
            encoding,
            page_width: 4,
            page_height: 3,
            padding: 1,
            ..Default::default()
        };
        let first = bmfont::write(&font(), &ResourcePath::new("test.fnt").unwrap(), &o).unwrap();
        assert_eq!(first.output.assets().len(), 4);
        let f = load(&first.output);
        for (_, g) in f.glyphs() {
            assert_eq!(g.bitmap.as_bytes(), [0, 127, 255, 255, 128, 0]);
            assert_eq!(g.metrics.bounds.y, -2);
        }
        let second = bmfont::write(&f, &ResourcePath::new("test.fnt").unwrap(), &o).unwrap();
        assert!(!second.report.has_loss(), "{encoding:?}: {}", second.report);
        assert_eq!(first.output, second.output);
    }
}
fn png(color: png::ColorType, data: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    {
        let mut e = png::Encoder::new(&mut b, 1, 1);
        e.set_color(color);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(data).unwrap();
    }
    b
}
fn descriptor(mask: u8) -> String {
    format!(
        "info face=\"Example\" size=8 unicode=1\ncommon lineHeight=8 base=6 scaleW=1 scaleH=1 pages=1 packed=1 alphaChnl=0 redChnl=0 greenChnl=0 blueChnl=0\npage id=0 file=\"page.png\"\nchars count=1\nchar id=65 x=0 y=0 width=1 height=1 xoffset=-1 yoffset=2 xadvance=3 page=0 chnl={mask}\n"
    )
}
#[test]
fn packed_channels_and_color_losses() {
    for (mask, expected) in [(1, 30), (2, 20), (4, 10), (8, 40), (15, 40)] {
        let s = descriptor(mask);
        let pages = Pages(
            [(
                "page.png".into(),
                png(png::ColorType::Rgba, &[10, 20, 30, 40]),
            )]
            .into(),
        );
        let input = Input::new(s.as_bytes()).with_resources(&pages);
        let l = ResourceLimits::default();
        let (f, r) = bmfont::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &Default::default(),
        )
        .unwrap();
        assert_eq!(f.glyphs().next().unwrap().1.bitmap.as_bytes(), [expected]);
        assert!(r.has_loss());
    }
}
#[test]
fn unsafe_paths_missing_pages_and_bad_rectangles_fail() {
    for s in [
        descriptor(15).replace("page.png", "../page.png"),
        descriptor(15).replace("page=0", "page=1"),
        descriptor(15).replace("width=1", "width=2"),
    ] {
        let pages = Pages([("page.png".into(), png(png::ColorType::Grayscale, &[255]))].into());
        let i = Input::new(s.as_bytes()).with_resources(&pages);
        let l = ResourceLimits::default();
        assert!(bmfont::read(&mut ReadContext::new(&i, &l).unwrap(), &Default::default()).is_err());
    }
}
#[test]
fn rejects_xml_entities_and_mismatched_tree() {
    for s in [
        "<!DOCTYPE font [<!ENTITY x 'y'>]><font/>",
        "<font><info></font></info>",
        "<font/><font/>",
    ] {
        let input = Input::new(s.as_bytes());
        let l = ResourceLimits::default();
        assert!(
            bmfont::read(
                &mut ReadContext::new(&input, &l).unwrap(),
                &Default::default()
            )
            .is_err()
        );
    }
}
#[test]
fn nested_primary_bundle_is_readable() {
    let o = BmfontWriteOptions {
        page_width: 8,
        page_height: 8,
        ..Default::default()
    };
    assert!(bmfont::write(&font(), &ResourcePath::new("sub/test.fnt").unwrap(), &o).is_ok());
}
#[test]
fn explicit_threshold_and_glyph_limits() {
    let s = descriptor(4);
    let pages = Pages(
        [(
            "page.png".into(),
            png(png::ColorType::Rgb, &[127, 127, 127]),
        )]
        .into(),
    );
    let input = Input::new(s.as_bytes()).with_resources(&pages);
    let l = ResourceLimits::default();
    let (f, r) = bmfont::read(
        &mut ReadContext::new(&input, &l).unwrap(),
        &BmfontReadOptions {
            gray_threshold: Some(128),
        },
    )
    .unwrap();
    assert_eq!(f.glyphs().next().unwrap().1.bitmap.as_bytes(), [0]);
    assert!(r.has_loss());
    let l = ResourceLimits {
        max_glyphs: 0,
        ..Default::default()
    };
    assert!(
        bmfont::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &Default::default()
        )
        .is_err()
    );
}

#[test]
fn sparse_slots_metrics_and_kerning_are_audited() {
    let mut f = font();
    let id = f.glyphs().next().unwrap().0;
    f.remove_glyph(id);
    f.metrics.ascent = Some(i64::MAX);
    f.metrics.descent = Some(1);
    f.metrics.line_height = None;
    assert!(
        bmfont::write(
            &f,
            &ResourcePath::new("bad.fnt").unwrap(),
            &Default::default()
        )
        .is_err()
    );
    f.metrics.ascent = Some(4);
    let w = bmfont::write(
        &f,
        &ResourcePath::new("slots.fnt").unwrap(),
        &BmfontWriteOptions {
            page_width: 8,
            page_height: 8,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        w.report
            .entries
            .iter()
            .any(|e| e.code == "bmfont.glyph_slots")
    );
}

#[test]
fn gray_png_alpha_alias_and_total_budget() {
    let s = descriptor(8);
    let pages = Pages([("page.png".into(), png(png::ColorType::Grayscale, &[173]))].into());
    let input = Input::new(s.as_bytes()).with_resources(&pages);
    let l = ResourceLimits::default();
    let (f, _) = bmfont::read(
        &mut ReadContext::new(&input, &l).unwrap(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(f.glyphs().next().unwrap().1.bitmap.as_bytes(), [173]);
    let l = ResourceLimits {
        max_total_bytes: 1000,
        ..Default::default()
    };
    assert!(
        bmfont::read(
            &mut ReadContext::new(&input, &l).unwrap(),
            &Default::default()
        )
        .is_err()
    );
}

proptest::proptest! {
    #[test]
    fn random_gray_atlas_roundtrip(w in 1u32..10,h in 1u32..10,data in proptest::collection::vec(proptest::prelude::any::<u8>(),100)) {
        let mut f=Font::default();
        let mut g=Glyph::new(Bitmap::<Gray8>::from_raw(w,h,&data[..(w*h)as usize],BitmapLimits::default()).unwrap());
        g.codepoints.push(Codepoint::Unicode(0x20000));
        g.metrics.bounds.x = -3;g.metrics.bounds.y = -2;g.metrics.advance=Some(Vector::new(12,0));f.add_glyph(g).unwrap();
        let options=BmfontWriteOptions{page_width:16,page_height:16,..Default::default()};
        let first=bmfont::write(&f,&ResourcePath::new("random.fnt").unwrap(),&options).unwrap();
        let native=load(&first.output);
        let second=bmfont::write(&native,&ResourcePath::new("random.fnt").unwrap(),&options).unwrap();
        proptest::prop_assert!(!second.report.has_loss(),"{}",second.report);
        proptest::prop_assert_eq!(first.output,second.output);
    }
}

#[test]
fn kerning_records_survive_every_descriptor_encoding() {
    let s = descriptor(15) + "kernings count=1\nkerning first=65 second=65 amount=-2\n";
    let pages = Pages([("page.png".into(), png(png::ColorType::Grayscale, &[255]))].into());
    let input = Input::new(s.as_bytes()).with_resources(&pages);
    let l = ResourceLimits::default();
    let (f, _) = bmfont::read(
        &mut ReadContext::new(&input, &l).unwrap(),
        &Default::default(),
    )
    .unwrap();
    for encoding in [
        BmfontEncoding::Text,
        BmfontEncoding::Xml,
        BmfontEncoding::Binary,
        BmfontEncoding::Json,
    ] {
        let out = bmfont::write(
            &f,
            &ResourcePath::new("kern.fnt").unwrap(),
            &BmfontWriteOptions {
                encoding,
                page_width: 2,
                page_height: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let back = load(&out.output);
        assert_eq!(
            f.properties.get("bmfont.kerning"),
            back.properties.get("bmfont.kerning")
        );
    }
}

#[test]
fn diagnostic_asset_names_share_the_total_budget() {
    use fontpixel::formats::{FormatReader, ReadOptions};
    let s = descriptor(15);
    let pages = Pages([("page.png".into(), png(png::ColorType::Grayscale, &[127]))].into());
    let name = "x".repeat(100_000);
    let input = Input::new(s.as_bytes())
        .with_resources(&pages)
        .with_name(&name);
    let limits = ResourceLimits {
        max_total_bytes: 100_000,
        ..Default::default()
    };
    let mut context = ReadContext::new(&input, &limits).unwrap();
    let codec = bmfont::BmfontCodec {
        read_options: BmfontReadOptions {
            gray_threshold: Some(128),
        },
        ..Default::default()
    };
    assert!(matches!(
        codec.read(&mut context, &ReadOptions::default()),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

fn json_fixture() -> serde_json::Value {
    serde_json::json!({"info":{"face":"Original \"JSON\" 字", "size":8,"unicode":true,"padding":[0,0,0,0],"spacing":[0,0]},"common":{"lineHeight":9,"base":7,"scaleW":1,"scaleH":1,"pages":1},"pages":["page.png"],"chars":[{"id":90,"x":0,"y":0,"width":1,"height":1,"xoffset":-2,"yoffset":8,"xadvance":4,"page":0,"chnl":15},{"id":65,"x":0,"y":0,"width":1,"height":1,"xoffset":3,"yoffset":2,"xadvance":5,"page":0,"chnl":15}],"kernings":[{"first":90,"second":65,"amount":-1}]})
}
fn read_json(bytes: &[u8], limits: &ResourceLimits) -> Result<Font> {
    let pages = Pages([("page.png".into(), png(png::ColorType::Grayscale, &[173]))].into());
    let input = Input::new(bytes).with_resources(&pages);
    bmfont::read(&mut ReadContext::new(&input, limits)?, &Default::default()).map(|v| v.0)
}
#[test]
fn json_order_coordinates_unicode_and_weak_probe() {
    use fontpixel::formats::Confidence;
    let bytes = serde_json::to_vec(&json_fixture()).unwrap();
    assert_eq!(bmfont::detect(&bytes), Confidence::Possible);
    let mut long = json_fixture();
    long["info"]["face"] = serde_json::json!("X");
    let short = serde_json::to_vec(&long).unwrap();
    let start = short.windows(3).position(|v| v == b"\"X\"").unwrap() + 1;
    long["info"]["face"] = serde_json::json!(format!("{}字", "x".repeat(8191 - start)));
    let long = serde_json::to_vec(&long).unwrap();
    assert_eq!(&long[8191..8194], "字".as_bytes());
    assert_eq!(bmfont::detect(&long), Confidence::Possible);
    assert!(read_json(&long, &Default::default()).is_ok());
    let f = read_json(&bytes, &Default::default()).unwrap();
    assert_eq!(f.names.family.as_deref(), Some("Original \"JSON\" 字"));
    let glyphs: Vec<_> = f
        .glyphs()
        .map(|(_, g)| {
            (
                g.codepoints.clone(),
                g.metrics.bounds.x,
                g.metrics.bounds.y,
                g.metrics.advance.unwrap().x,
                g.bitmap.as_bytes().to_vec(),
            )
        })
        .collect();
    assert_eq!(
        glyphs,
        vec![
            (vec![Codepoint::Unicode(90)], -2, -2, 4, vec![173]),
            (vec![Codepoint::Unicode(65)], 3, 4, 5, vec![173])
        ]
    );
    let mut bom = b"\xef\xbb\xbf  \n".to_vec();
    bom.extend(&bytes);
    assert_eq!(read_json(&bom, &Default::default()).unwrap(), f);
    for bytes in [
        b"{}".as_slice(),
        b"{\"info\":{},\"common\":{},\"pages\":[],\"chars\":[",
        b"{\"info\":{},\"common\":{},\"pages\":{},\"chars\":[]}",
        b"{\"info\":{},\"common\":{},\"pages\":[],\"chars\":[]} junk",
    ] {
        assert_eq!(bmfont::detect(bytes), Confidence::NoMatch);
    }
}
#[test]
fn json_rejects_duplicate_keys_bad_types_and_resource_excess() {
    let good = serde_json::to_string(&json_fixture()).unwrap();
    assert!(
        read_json(
            good.replacen("{", "{\"info\":{},", 1).as_bytes(),
            &Default::default()
        )
        .is_err()
    );
    for (field, value) in [
        ("padding", serde_json::json!([1, 2, 3])),
        ("spacing", serde_json::json!([1, 2, 3])),
        ("face", serde_json::json!({"nested":1})),
        ("size", serde_json::json!(1.5)),
    ] {
        let mut j = json_fixture();
        j["info"][field] = value;
        assert!(read_json(&serde_json::to_vec(&j).unwrap(), &Default::default()).is_err());
    }
    for limits in [
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_resources: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 10,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 5000,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            read_json(good.as_bytes(), &limits),
            Err(fontpixel::Error::ResourceLimit { .. })
        ));
    }
    let bad = good.replace("\"chars\":[", "\"chars\":[null,");
    assert!(read_json(bad.as_bytes(), &Default::default()).is_err());
    let mut j = json_fixture();
    j["pages"][0] = serde_json::json!("../page.png");
    assert!(read_json(&serde_json::to_vec(&j).unwrap(), &Default::default()).is_err());
    let nested = format!("{{\"info\":{}1{}}}", "[".repeat(30), "]".repeat(30));
    assert!(matches!(
        read_json(nested.as_bytes(), &Default::default()),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}
#[test]
fn json_unknown_scalar_types_survive_and_other_metadata_is_audited() {
    let mut j = json_fixture();
    j["info"]["vendorText"] = serde_json::json!("17");
    j["info"]["vendorNumber"] = serde_json::json!(17);
    j["info"]["vendorBool"] = serde_json::json!(true);
    j["info"]["vendorArray"] = serde_json::json!([17, -2]);
    j["info"]["vendorEmpty"] = serde_json::json!([]);
    j["common"]["vendorCommon"] = serde_json::json!("retained until repacking");
    let f = read_json(&serde_json::to_vec(&j).unwrap(), &Default::default()).unwrap();
    let o = BmfontWriteOptions {
        encoding: BmfontEncoding::Json,
        page_width: 4,
        page_height: 4,
        ..Default::default()
    };
    let first = bmfont::write(&f, &ResourcePath::new("types.fnt").unwrap(), &o).unwrap();
    assert!(
        first
            .report
            .entries
            .iter()
            .any(|e| e.code == "bmfont.properties")
    );
    let bytes = &first
        .output
        .assets()
        .iter()
        .find(|a| &a.path == first.output.primary())
        .unwrap()
        .bytes;
    let parsed: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    for key in [
        "vendorText",
        "vendorNumber",
        "vendorBool",
        "vendorArray",
        "vendorEmpty",
    ] {
        assert_eq!(parsed["info"][key], j["info"][key]);
    }
    let native = load(&first.output);
    let second = bmfont::write(&native, &ResourcePath::new("types.fnt").unwrap(), &o).unwrap();
    assert!(!second.report.has_loss(), "{}", second.report);
    assert_eq!(first.output, second.output);
    let limits = ResourceLimits {
        max_input_bytes: 64,
        ..Default::default()
    };
    assert!(matches!(
        bmfont::write(
            &native,
            &ResourcePath::new("types.fnt").unwrap(),
            &BmfontWriteOptions { limits, ..o }
        ),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn json_codec_strict_rejects_glyph_metadata_loss() {
    use fontpixel::formats::{FormatReader, FormatWriter, ReadOptions, WriteOptions};
    let options = BmfontWriteOptions {
        encoding: BmfontEncoding::Json,
        page_width: 8,
        page_height: 8,
        ..Default::default()
    };
    let w = bmfont::write(&font(), &ResourcePath::new("font.fnt").unwrap(), &options).unwrap();
    let primary = &w
        .output
        .assets()
        .iter()
        .find(|a| &a.path == w.output.primary())
        .unwrap()
        .bytes;
    let pages = Pages(
        w.output
            .assets()
            .iter()
            .map(|a| (a.path.as_str().into(), a.bytes.clone()))
            .collect(),
    );
    let input = Input::new(primary).with_resources(&pages);
    let limits = ResourceLimits::default();
    let codec = bmfont::BmfontCodec {
        write_options: options,
        ..Default::default()
    };
    let (mut s, _) = codec
        .read(
            &mut ReadContext::new(&input, &limits).unwrap(),
            &ReadOptions::default(),
        )
        .unwrap();
    let mut o = WriteOptions::new(ResourcePath::new("font.fnt").unwrap());
    o.strict = true;
    assert!(codec.write(&s, &o).is_ok());
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.name = Some("unrepresented".into());
            g.unicode_sequences.push(vec![65, 0x301]);
            Ok(())
        })
        .unwrap();
    let e = codec.write(&s, &o).unwrap_err();
    assert!(matches!(e, fontpixel::Error::StrictLoss(_)));
    let report = e.report().unwrap();
    assert!(report.entries.iter().any(|e| e.code == "bmfont.glyph_name"));
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "bmfont.unicode_sequences")
    );
}
