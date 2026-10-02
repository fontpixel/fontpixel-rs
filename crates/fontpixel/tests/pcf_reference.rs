//! Synthetic Python PCF byte oracles, without a reference checkout at test time.
#![cfg(all(feature = "bdf", feature = "pcf"))]
use fontpixel::{
    ResourceLimits,
    formats::{
        bdf,
        pcf::{self, GlyphPad, Order, PcfWriteOptions, ScanUnit},
    },
};
use std::io::Read;
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn python_pcf_writer_and_reader_byte_parity() {
    let mut input = String::new();
    flate2::read::GzDecoder::new(include_bytes!("data/pcf_reference.json.gz").as_slice())
        .read_to_string(&mut input)
        .unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&input).unwrap();
    assert!(cases.len() > 350);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let layout = &c["options"];
        let options = PcfWriteOptions {
            byte_order: if layout["byteorder"] == "lsb" {
                Order::Lsb
            } else {
                Order::Msb
            },
            bit_order: if layout["bitorder"] == "lsb" {
                Order::Lsb
            } else {
                Order::Msb
            },
            glyph_pad: match layout["glyphpad"].as_u64() {
                Some(1) => GlyphPad::One,
                Some(2) => GlyphPad::Two,
                Some(8) => GlyphPad::Eight,
                _ => GlyphPad::Four,
            },
            scan_unit: match layout["scanunit"].as_u64() {
                Some(2) => ScanUnit::Two,
                Some(4) => ScanUnit::Four,
                _ => ScanUnit::One,
            },
            ..PcfWriteOptions::default()
        };
        if c.get("source").is_none() {
            let wire = unhex(c["pcf"].as_str().unwrap());
            let (font, _) = pcf::read(&wire, &ResourceLimits::default())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let text = bdf::write(&font, &bdf::BdfWriteOptions::default())
                .unwrap()
                .0;
            assert_eq!(text, c["bdf"].as_str().unwrap().as_bytes(), "{name}: read");
            assert_eq!(
                pcf::write(&font, &options).unwrap().0,
                unhex(c["rewrite"].as_str().unwrap()),
                "{name}: rewrite"
            );
            continue;
        }
        let (font, _) = bdf::read(
            c["source"].as_str().unwrap().as_bytes(),
            &ResourceLimits::default(),
        )
        .unwrap();
        let result = pcf::write(&font, &options);
        if let Some(error) = c.get("error") {
            assert_eq!(
                result.unwrap_err().to_string(),
                error.as_str().unwrap(),
                "{name}"
            );
            continue;
        }
        let expected = unhex(c["pcf"].as_str().unwrap());
        let actual = result.unwrap_or_else(|e| panic!("{name}: {e}")).0;
        assert!(
            actual == expected,
            "{name} {layout}: PCF bytes differ; first {:?}, lengths {}/{}",
            actual.iter().zip(&expected).position(|(a, b)| a != b),
            actual.len(),
            expected.len()
        );
        let (back, _) = pcf::read(&actual, &ResourceLimits::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let text = bdf::write(&back, &bdf::BdfWriteOptions::default())
            .unwrap()
            .0;
        assert_eq!(
            String::from_utf8(text).unwrap(),
            c["bdf"].as_str().unwrap(),
            "{name}: PCF reconstruction"
        );
    }
}
