//! Direct Rust goldens and attachment/resource invariants for SPEC-v3.
#![cfg(feature = "compat-bdfparser")]
use fontpixel::compat::bdfparser::{
    Bitmap, CreateOptions, Font, outline,
    ttf::{self, TtfOptions},
};
use serde_json::{Value, json};
const MINI: &str = include_str!("../../../tests/data/compat/mini.bdf");
fn mini() -> Font {
    let mut f = Font::default();
    f.load_string(MINI).unwrap();
    f
}
#[test]
fn exact_reference_bdf_svg_and_ttf_goldens() {
    let f = mini();
    assert_eq!(f.tobdf().unwrap(), MINI);
    assert_eq!(
        f.tosvgfont(0.0, 0.0, false, "hexncr").unwrap(),
        include_str!("../../../tests/data/compat/mini.svg")
    );
    let manifest: Value =
        serde_json::from_str(include_str!("../../../tests/data/compat/ttf.json")).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/compat");
    for rec in manifest.as_array().unwrap() {
        let o = &rec["options"];
        let options = TtfOptions {
            shape: o["shape"].as_str().unwrap().into(),
            dotsize: o["dotsize"].as_f64().unwrap(),
            ppu: 64,
            metadata: o["metadata"].as_bool().unwrap(),
            timestamp: 0,
        };
        let name = rec["name"].as_str().unwrap();
        let bytes = f.tottf(&options).unwrap();
        assert_eq!(
            bytes,
            std::fs::read(dir.join(format!("{name}.ttf"))).unwrap(),
            "{name}"
        );
        assert_eq!(
            ttf::read_ttf(&bytes, Some(8)).unwrap().tobdf().unwrap(),
            std::fs::read_to_string(dir.join(format!("{name}.bdf"))).unwrap(),
            "{name}"
        );
    }
}
#[test]
fn stale_handles_merge_changed_fields_and_do_not_delete_replacements() {
    let mut f = mini();
    let mut a = f.glyphbycp(65).unwrap();
    let mut b = f.glyphbycp(65).unwrap();
    b.shift(2, 3).unwrap();
    a.setmeta(json!({"dwx0":9}).as_object().unwrap()).unwrap();
    assert_eq!(a.meta()[4], json!(2));
    assert_eq!(a.meta()[8], json!(9));
    f.newglyph(65, None, 0, None, None).unwrap();
    a.shift(1, 1).unwrap();
    f.remove_handle(&mut a);
    assert!(f.glyphbycp(65).is_some());
    assert!(!a.is_attached());
}
#[test]
fn detached_clone_removal_follows_python_reference() {
    let mut f = mini();
    let mut detached = f.glyphbycp(65).unwrap().clone();
    f.remove_handle(&mut detached);
    assert!(f.glyphbycp(65).is_none());
}
#[test]
fn clones_and_subsets_own_independent_native_models() {
    let f = mini();
    let mut c = f.clone();
    c.removeglyph(65);
    assert_eq!(f.length(), 3);
    assert_eq!(c.length(), 2);
    let subset = f.subset(&json!([65, 66])).unwrap();
    assert_eq!(subset.length(), 2);
    assert_eq!(subset.core().len(), 2);
}
#[test]
fn integer_rows_are_exact_beyond_64_bits() {
    let b = Bitmap::new(&["1".repeat(128)]).unwrap();
    assert_eq!(
        b.todata(5).unwrap().to_string(),
        "[340282366920938463463374607431768211455]"
    );
    assert_eq!(
        b.todata(4).unwrap(),
        json!(["ffffffffffffffffffffffffffffffff"])
    );
}
#[test]
fn invalid_ragged_input_is_rejected_without_changing_raster() {
    let mut b = Bitmap::new(&["110".into(), "010".into()]).unwrap();
    let before = b.clone();
    assert!(b.replace("11", "").is_err());
    assert_eq!(b, before);
    assert!(Bitmap::new(&["110".into(), "1".into()]).is_err());
    assert!(b.bold(-1).is_err());
}
#[test]
fn empty_shapes_and_fromstring_whitespace() {
    assert_eq!(Bitmap::blank(7, 0).unwrap().width(), 0);
    assert_eq!(
        Bitmap::fromstring("\n \r\n#.\n.#\n \n").unwrap().bindata(),
        vec!["10", "01"]
    );
    assert!(Bitmap::fromstring(" #\n##").is_err());
    let mut b = Bitmap::blank(0, 2).unwrap();
    b.rotate(90).unwrap();
    assert_eq!(b.bindata(), Vec::<String>::new());
}
#[test]
fn outline_holes_pinches_and_pixel_centres_match_reference() {
    let rows = ["111".into(), "101".into(), "111".into()];
    let cs = outline::bitmap_contours(&rows, "square", 1.0, 1, 0, 0).unwrap();
    assert_eq!(
        cs,
        vec![
            vec![
                (0., 0., true),
                (0., 3., true),
                (3., 3., true),
                (3., 0., true)
            ],
            vec![
                (1., 1., true),
                (2., 1., true),
                (2., 2., true),
                (1., 2., true)
            ]
        ]
    );
    assert_eq!(
        outline::rasterize(&cs, 1., 0, 0, 3, 3).unwrap().bindata(),
        rows
    );
    for shape in ["square", "circle"] {
        let cs = outline::bitmap_contours(&rows, shape, 1.0, 64, -2, -1).unwrap();
        assert_eq!(
            outline::rasterize(&cs, 64., -2, -1, 3, 3)
                .unwrap()
                .bindata(),
            rows
        );
    }
}
#[test]
fn unknown_charset_warns_once_and_surrogates_have_numeric_access() {
    let mut f = Font::create(&CreateOptions::default()).unwrap();
    f.encoding("unknown-legacy");
    assert_eq!(f.cp2unicode(0xd800).unwrap(), Some(0xd800));
    assert_eq!(f.cp2char(0xd800).unwrap(), None);
    f.char2cp("a").unwrap();
    assert_eq!(
        f.report()
            .entries
            .iter()
            .filter(|e| e.code == "bdfparser.warning")
            .count(),
        1
    );
}
#[test]
fn bounded_corrupt_true_type_and_option_errors() {
    for len in 0..100 {
        let data = vec![0; len];
        assert!(ttf::read_ttf(&data, Some(8)).is_err());
    }
    assert!(
        mini()
            .tottf(&TtfOptions {
                ppu: 3,
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        mini()
            .tottf(&TtfOptions {
                timestamp: i64::MAX,
                ..Default::default()
            })
            .is_err()
    );
    assert!(Bitmap::blank(i64::MAX, 2).is_err());
}
#[test]
fn encoding_override_is_not_a_pcf_property_edit() {
    let mut f = mini();
    let before = f.topcf(&Default::default()).unwrap();
    f.encoding("cp936");
    assert_eq!(before, f.topcf(&Default::default()).unwrap());
}
#[test]
fn io_byte_palettes_and_custom_encoding() {
    use fontpixel::compat::bdfparser::{Converter, SaveOptions};
    let mut f = Font::default();
    f.load_lines(MINI.lines().map(str::to_owned)).unwrap();
    assert_eq!(f.tobdf().unwrap(), MINI);
    let mut out = Vec::new();
    f.save_to(&mut out, &SaveOptions::Bdf).unwrap();
    assert_eq!(out, MINI.as_bytes());
    let b = Bitmap::new(&["012".into()]).unwrap();
    assert_eq!(b.tobytes("L", None).unwrap(), vec![255, 0, 127]);
    assert_eq!(b.tobytes("1", None).unwrap(), vec![0x9f]);
    f.set_converters(
        Some(Converter::new(|u| (u == 'a' as u32).then_some(65))),
        Some(Converter::new(|cp| (cp == 65).then_some('a' as u32))),
    );
    assert_eq!(f.glyph("a").unwrap().unwrap().cp(), 65);
    assert_eq!(f.cp2char(65).unwrap(), Some('a'));
    let c = f.clone();
    assert_eq!(c.char2cp("a").unwrap(), Some(65));
    f.charsetconvert("iso10646").unwrap();
    assert!(f.glyphbycp(97).is_some());
    assert_eq!(f.char2cp("a").unwrap(), Some(97));
}
