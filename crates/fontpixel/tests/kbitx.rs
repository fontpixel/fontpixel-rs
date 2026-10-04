//! Native KBITX wire, metadata, XML safety and budget tests.
#![cfg(feature = "kbitx")]
use fontpixel::bitmap::{BitmapLimits, Gray8};
use fontpixel::formats::{
    self,
    kbitx::{self, KbitxReadOptions, KbitxWriteOptions},
};
use fontpixel::model::{Codepoint, GlyphBitmap, Property, Vector};
use fontpixel::{Bitmap, Error, FontSet, ResourceLimits};
fn read(s: &str) -> fontpixel::Font {
    kbitx::read(
        s.as_bytes(),
        &KbitxReadOptions::default(),
        &ResourceLimits::default(),
    )
    .unwrap()
    .0
}
fn bad(s: &str) {
    assert!(
        kbitx::read(
            s.as_bytes(),
            &KbitxReadOptions::default(),
            &ResourceLimits::default()
        )
        .is_err(),
        "accepted {s}"
    );
}
fn glyph(d: &str) -> String {
    format!("<kbits><g u=\"65\" x=\"-2\" y=\"12\" w=\"9\" d=\"{d}\"/></kbits>")
}
#[test]
fn native_roundtrip_preserves_names_metadata_and_gray() {
    let source = r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE kbits SYSTEM "https://invalid.test/unfetched.dtd"><kbits>
<prop id="emAscent" value="10"/><prop id="emDescent" value="2"/><prop id="lineAscent" value="11"/><prop id="lineGap" value="3"/><prop id="future" value="17"/>
<name id="1" value="A &amp; B"/><name id="99" value="Unknown&#10;name"/><g u="65" n="Alpha" x="-2" y="2" w="9" d="AgPGAH+A//8A"/>
<g n="empty" x="2" y="-3" w="5" d="AAA"/>
<k lu="65" rn="empty" o="-1"/><feat name="liga">sub A B by C;</feat><puaa><pt prop="Block"><pv range="0041..0042" value="Latin"/></pt></puaa>
</kbits>"#;
    let font = read(source);
    assert_eq!(font.metrics.pixel_size, Some(12));
    assert_eq!(font.metrics.line_gap, Some(3));
    assert_eq!(font.names.family.as_deref(), Some("A & B"));
    let g = font.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert!(matches!(g.bitmap, GlyphBitmap::Gray8(_)));
    assert_eq!(g.bitmap.as_bytes(), [0, 127, 128, 255, 255, 0]);
    assert_eq!(g.metrics.bounds.y, 0);
    let (out, report) = kbitx::write(&font, &KbitxWriteOptions::default()).unwrap();
    assert!(!report.has_loss(), "{report}");
    let back = read(std::str::from_utf8(&out).unwrap());
    assert_eq!(font, back);
    assert_eq!(
        out,
        kbitx::write(&back, &KbitxWriteOptions::default())
            .unwrap()
            .0
    );
}
#[test]
fn wib_long_runs_and_uleb_are_exact() {
    let cases = [
        (
            "IgEhQg",
            34,
            vec![0; 32]
                .into_iter()
                .chain([128, 128])
                .collect::<Vec<_>>(),
        ),
        (
            "IgFhAUE",
            34,
            vec![128; 32].into_iter().chain([0, 128]).collect(),
        ),
        (
            "IgGhgAFB",
            34,
            vec![128; 32].into_iter().chain([0, 128]).collect(),
        ),
        (
            "IgGhf0I",
            34,
            vec![0; 32].into_iter().chain([128, 128]).collect(),
        ),
    ];
    for (d, h, expected) in cases {
        let (font, report) = kbitx::read(
            glyph(d).as_bytes(),
            &KbitxReadOptions {
                gray_threshold: Some(128),
            },
            &ResourceLimits::default(),
        )
        .unwrap();
        let g = font.glyphs().next().unwrap().1;
        assert_eq!((g.bitmap.width(), g.bitmap.height()), (1, h));
        assert_eq!(g.bitmap.as_bytes(), expected);
        if d == "IgGhf0I" {
            assert!(report.has_loss());
        }
    }
    let font = read(&glyph("DAhj"));
    let g = font.glyphs().next().unwrap().1;
    assert_eq!((g.bitmap.width(), g.bitmap.height()), (8, 12));
    assert_eq!(g.bitmap.as_bytes(), [255; 12]);
    let font = read(&glyph("AYIBZEI"));
    let g = font.glyphs().next().unwrap().1;
    assert_eq!((g.bitmap.width(), g.bitmap.height()), (130, 1));
    assert_eq!(g.bitmap.as_bytes(), [vec![255; 16], vec![192]].concat());
}
#[test]
fn rejects_malformed_wib_base64_and_oversize() {
    for d in [
        "A",
        "!",
        "AB",
        "AAAA",
        "AAE=AAAA",
        "gIA",
        "//////8PAA",
        "AQE",
        "AQFA",
        "AQFC",
        "AQGA",
        "AQHBAA==AAAA",
        "AQHA",
        "AQEAAQ",
    ] {
        bad(&glyph(d));
    }
    let limits = ResourceLimits {
        max_pixels_per_glyph: 95,
        ..Default::default()
    };
    assert!(matches!(
        kbitx::read(
            glyph("DAhj").as_bytes(),
            &KbitxReadOptions::default(),
            &limits
        ),
        Err(Error::ResourceLimit { .. })
    ));
}
#[test]
fn external_doctype_is_inert_internal_and_entities_are_forbidden() {
    read("<!DOCTYPE kbits PUBLIC 'test' 'file:///etc/passwd'><kbits/>");
    for s in [
        "<!DOCTYPE kbits [<!ENTITY e 'secret'>]><kbits><name id='1' value='&e;'/></kbits>",
        "<!DOCTYPE kbits []><kbits/>",
        "<?execute code?><kbits/>",
        "<?xml version='1.0' encoding='ISO-8859-1'?><kbits/>",
        "<kbits><name id='1' value='&unknown;'/></kbits>",
        "<other/>",
        "<kbits><g u='55296' d='AAA'/></kbits>",
    ] {
        bad(s);
    }
    bad(&format!(
        "<kbits>{}{}{}</kbits>",
        "<puaa>".repeat(32),
        "<pv/>",
        "</puaa>".repeat(32)
    ));
}
#[test]
fn duplicate_mapping_and_scalar_records_survive() {
    let source = "<kbits><prop id='emAscent' value='1'/><prop id='emAscent' value='2'/><g u='65' d='AQFB'/><g u='65' d='AQEB'/></kbits>";
    let (font, report) = kbitx::read(
        source.as_bytes(),
        &KbitxReadOptions::default(),
        &ResourceLimits::default(),
    )
    .unwrap();
    assert!(!report.has_loss());
    assert_eq!(font.metrics.ascent, Some(2));
    assert_eq!(
        font.glyph_ids_for_codepoint(Codepoint::Unicode(65)).len(),
        2
    );
    let (out, report) = kbitx::write(&font, &KbitxWriteOptions::default()).unwrap();
    assert!(!report.has_loss(), "{report}");
    assert_eq!(read(std::str::from_utf8(&out).unwrap()), font);
}
#[test]
fn losses_are_explicit_and_strict_registry_rejects() {
    let mut font = read("<kbits><g u='65' n='A' d='AQFB'/></kbits>");
    let id = font.glyph_id(Codepoint::Unicode(65)).unwrap();
    font.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(66));
        g.codepoints.push(Codepoint::Legacy(67));
        g.unicode_sequences.push(vec![65, 66]);
        g.metrics.scalable_advance = Some(Vector::new(500, 0));
        g.properties.push(Property::new("private", "value"));
        Ok(())
    })
    .unwrap();
    let (_, report) = kbitx::write(&font, &KbitxWriteOptions::default()).unwrap();
    assert!(report.has_loss());
    for code in [
        "aliases",
        "codepoint",
        "sequences",
        "glyph_metrics",
        "glyph_properties",
    ] {
        assert!(
            report
                .entries
                .iter()
                .any(|e| e.code == format!("kbitx.write.{code}"))
        );
    }
    let registry = fontpixel::registry();
    let options = formats::WriteOptions {
        strict: true,
        transport_limits: None,
        primary_name: fontpixel::resources::ResourcePath::new("font.kbitx").unwrap(),
    };
    assert!(matches!(
        registry.write(&FontSet::from(font), "kbitx", &options),
        Err(Error::StrictLoss(_))
    ));
    let (_, report) = kbitx::read(
        b"<kbits unknown='x'><g u='65' d='AQFB' strange='y'/><unknown/></kbits>",
        &KbitxReadOptions::default(),
        &ResourceLimits::default(),
    )
    .unwrap();
    assert_eq!(
        report
            .entries
            .iter()
            .filter(|e| e.impact == fontpixel::report::Impact::Loss)
            .count(),
        3
    );
}
#[test]
fn edited_native_metadata_and_unsafe_retained_xml_are_checked() {
    let mut f = read("<kbits><prop id='emAscent' value='1'/><name id='1' value='Before'/></kbits>");
    f.names.family = Some("After \" & < > \t\n".into());
    f.metrics.ascent = Some(12);
    let (out, report) = kbitx::write(&f, &KbitxWriteOptions::default()).unwrap();
    assert!(report.has_loss());
    let back = read(std::str::from_utf8(&out).unwrap());
    assert_eq!(back.names.family, f.names.family);
    assert_eq!(back.metrics.ascent, Some(12));
    for xml in [
        "<!DOCTYPE kbits [<!ENTITY x 'secret'>]><feat>&x;</feat>",
        "<evil/>",
        "<puaa><evil/></puaa>",
    ] {
        f.properties.push(Property::new("kbitx.element", xml));
        assert!(kbitx::write(&f, &KbitxWriteOptions::default()).is_err());
        f.properties.remove("kbitx.element");
    }
}
#[test]
fn budgets_cover_many_empty_records_and_output() {
    let input = format!("<kbits>{}</kbits>", "<g n='empty' d='AAA'/>".repeat(100));
    let l = ResourceLimits {
        max_total_bytes: 32 * 1024,
        ..Default::default()
    };
    assert!(matches!(
        kbitx::read(input.as_bytes(), &KbitxReadOptions::default(), &l),
        Err(Error::ResourceLimit { .. })
    ));
    let f = read(&glyph("DAhj"));
    assert!(
        kbitx::write(
            &f,
            &KbitxWriteOptions {
                max_output_bytes: 16,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn gray_roundtrip_and_zero_dimensions_are_exact() {
    let mut f = read("<kbits><g u='65' d='AAA'/></kbits>");
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.set_bitmap(
            Bitmap::<Gray8>::from_raw(3, 1, &[1, 127, 254], BitmapLimits::default())?,
            0,
            0,
            false,
        )?;
        Ok(())
    })
    .unwrap();
    f.update_bounds().unwrap();
    let (out, report) = kbitx::write(&f, &KbitxWriteOptions::default()).unwrap();
    assert!(!report.has_loss(), "{report}");
    assert_eq!(f, read(std::str::from_utf8(&out).unwrap()));
    for d in ["AAA", "AAU", "BQA"] {
        let f = read(&glyph(d));
        let (out, report) = kbitx::write(&f, &KbitxWriteOptions::default()).unwrap();
        assert!(!report.has_loss(), "{report}");
        assert_eq!(f, read(std::str::from_utf8(&out).unwrap()));
    }
}

proptest::proptest! {
    #[test]
    fn generated_native_rasters_roundtrip(w in 1u32..40,h in 1u32..40,seed in 0u32..u32::MAX,gray in proptest::bool::ANY) {
        let mut f=read("<kbits><g u='65' n='A' d='AAA'/></kbits>");let id=f.glyph_id(Codepoint::Unicode(65)).unwrap();
        let data:Vec<u8>=(0..w*h).map(|i|((i.wrapping_mul(1103515245).wrapping_add(seed))>>16)as u8).collect();
        let bitmap=if gray {GlyphBitmap::Gray8(Bitmap::<Gray8>::from_raw(w,h,&data,BitmapLimits::default()).unwrap())}else{let mut b=Bitmap::new(w,h).unwrap();for(i,v)in data.iter().enumerate(){b.set_pixel(i as u32%w,i as u32/w,u8::from(*v>=128)).unwrap();}GlyphBitmap::Mono(b)};
        f.edit_glyph(id,|g|g.set_bitmap(bitmap,-1,-2,false)).unwrap();f.update_bounds().unwrap();
        let(out,report)=kbitx::write(&f,&KbitxWriteOptions::default()).unwrap();let back=read(std::str::from_utf8(&out).unwrap());
        if !report.has_loss(){proptest::prop_assert_eq!(&f,&back);}
        proptest::prop_assert_eq!(out,kbitx::write(&back,&KbitxWriteOptions::default()).unwrap().0);
    }
}

#[test]
fn detection_handles_utf8_cut_at_sniff_boundary() {
    let source = format!(
        "<kbits><name id='1' value='{}é'/></kbits>",
        "a".repeat(8167)
    );
    assert_eq!(
        kbitx::detect(source.as_bytes()),
        formats::Confidence::Certain
    );
    assert_eq!(read("<kbits><g n='empty' d=' \n\t '/></kbits>").len(), 1);
}

#[test]
fn metadata_order_and_reader_output_limits_are_explicit() {
    let f = read(
        "<kbits><name id='1' value='Font'/><prop id='emAscent' value='9'/><feat>feature</feat><k ln='A' rn='B' o='-1'/></kbits>",
    );
    let (out, report) = kbitx::write(&f, &KbitxWriteOptions::default()).unwrap();
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.code == "kbitx.write.property_order")
    );
    let text = std::str::from_utf8(&out).unwrap();
    assert!(text.find("<k ").unwrap() < text.find("<feat>").unwrap());
    let limits = ResourceLimits {
        max_input_bytes: 64,
        ..Default::default()
    };
    assert!(
        kbitx::write(
            &f,
            &KbitxWriteOptions {
                limits,
                ..Default::default()
            }
        )
        .is_err()
    );
    bad("<kbits><puaa/><puaa/></kbits>");
    let f = read("<kbits><name id='1' value='&amp;&amp;&amp;&amp;'/></kbits>");
    let limits = ResourceLimits {
        max_table_bytes: 8,
        ..Default::default()
    };
    assert!(
        kbitx::write(
            &f,
            &KbitxWriteOptions {
                limits,
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn numeric_name_ids_are_semantic_integers() {
    assert_eq!(
        read("<kbits><name id='+01' value='Only'/></kbits>")
            .names
            .family
            .as_deref(),
        Some("Only")
    );
    assert_eq!(
        read("<kbits><name id='1' value='Earlier'/><name id='+01' value='Later'/></kbits>")
            .names
            .family
            .as_deref(),
        Some("Later")
    );
    let f = read("<kbits><name id='+01' value='Earlier'/><name id='1' value='Later'/></kbits>");
    assert_eq!(f.names.family.as_deref(), Some("Later"));
    let (out, report) = kbitx::write(&f, &KbitxWriteOptions::default()).unwrap();
    assert!(!report.has_loss(), "{report}");
    assert_eq!(f, read(std::str::from_utf8(&out).unwrap()));
}
