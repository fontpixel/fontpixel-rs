//! Original SFN2/SFNC and ASCII source raster fixtures.
#![cfg(feature = "ssfn")]
use fontpixel::formats::ssfn::{self, SsfnEncoding, SsfnWriteOptions};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
const BIN: &[u8] = include_bytes!("ssfn/ssfn-original.sfn");
const ASC: &[u8] = include_bytes!("ssfn/ssfn-original.asc");
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    let i = Input::new(b);
    let mut c = ReadContext::new(&i, l)?;
    ssfn::read(&mut c, &Default::default())
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.sfn").unwrap())
}
#[test]
fn original_binary_ascii_identical_metrics_pixels() {
    let (s, r) = read(BIN, &Default::default()).unwrap();
    assert!(!r.has_loss());
    assert_eq!(s, read(ASC, &Default::default()).unwrap().0);
    let g = s.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        (
            g.metrics.bounds.x,
            g.metrics.bounds.y,
            g.bitmap.width(),
            g.bitmap.height()
        ),
        (-2, -1, 9, 5)
    );
    assert_eq!(
        g.bitmap.as_bytes(),
        [145, 128, 65, 0, 50, 0, 132, 0, 80, 128]
    );
    assert_eq!(g.metrics.advance.unwrap().x, 11);
}
#[test]
fn both_encodings_strict_and_gray_coverage() {
    for source in [BIN, include_bytes!("ssfn/ssfn-gray.asc").as_slice()] {
        let (s, r) = read(source, &Default::default()).unwrap();
        assert!(!r.has_loss());
        for encoding in [SsfnEncoding::Binary, SsfnEncoding::Ascii] {
            let mut o = options();
            o.strict = true;
            let p = SsfnWriteOptions {
                encoding,
                ..Default::default()
            };
            let w = ssfn::write(&s, &o, &p).unwrap();
            assert!(!w.report.has_loss());
            assert_eq!(
                s,
                read(&w.output.into_single().unwrap(), &p.limits).unwrap().0
            );
        }
    }
}
#[test]
fn ligature_binary_ascii_and_collection() {
    let (s, _) = read(
        include_bytes!("ssfn/ssfn-ligature.asc"),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        s.fonts[0].glyphs().next().unwrap().1.unicode_sequences,
        [vec![102, 105]]
    );
    for encoding in [SsfnEncoding::Binary, SsfnEncoding::Ascii] {
        let w = ssfn::write(
            &s,
            &options(),
            &SsfnWriteOptions {
                encoding,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            s,
            read(&w.output.into_single().unwrap(), &Default::default())
                .unwrap()
                .0
        );
    }
    let (s, _) = read(
        include_bytes!("ssfn/ssfn-collection.sfn"),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(s.fonts.len(), 2);
    let w = ssfn::write(&s, &options(), &Default::default()).unwrap();
    assert_eq!(
        s,
        read(&w.output.into_single().unwrap(), &Default::default())
            .unwrap()
            .0
    );
}
#[test]
fn metadata_alias_loss_and_strict() {
    let (mut s, _) = read(BIN, &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.name = Some("bitmap-name".into());
            g.codepoints.push(fontpixel::model::Codepoint::Unicode(66));
            g.metrics.vertical_origin = Some(fontpixel::model::Vector::new(1, 2));
            g.properties
                .push(fontpixel::model::Property::new("private", "metadata"));
            Ok(())
        })
        .unwrap();
    assert!(
        ssfn::write(&s, &options(), &Default::default())
            .unwrap()
            .report
            .has_loss()
    );
    let mut o = options();
    o.strict = true;
    assert!(matches!(
        ssfn::write(&s, &o, &Default::default()),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn mixed_vector_and_color_are_explicit_loss() {
    let source = std::str::from_utf8(ASC).unwrap();
    let mixed = source.replace("# End #", "m 0,0\nl 4,4\nH 1 3\nk U+0042 -2 0\n# End #");
    let (_, r) = read(mixed.as_bytes(), &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "ssfn.contours-omitted"));
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "ssfn.layout-data-omitted")
    );
    let gray = std::str::from_utf8(include_bytes!("ssfn/ssfn-gray.asc"))
        .unwrap()
        .replace("FFFFFF", "00FF00");
    assert!(
        read(gray.as_bytes(), &Default::default())
            .unwrap()
            .1
            .has_loss()
    );
    let pure = source.split("X..X...XX").next().unwrap().to_owned() + "m 0,0\nl 4,4\n# End #\n";
    assert!(read(pure.as_bytes(), &Default::default()).is_err());
}
#[test]
fn ascii_byte_padded_rows_and_invalid_padding() {
    let source = std::str::from_utf8(ASC).unwrap();
    let padded = source
        .lines()
        .map(|s| {
            if s.starts_with(['X', '.']) {
                format!("{s}.......")
            } else {
                s.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        read(padded.as_bytes(), &Default::default()).unwrap().0,
        read(ASC, &Default::default()).unwrap().0
    );
    let bad = padded.replace("X..X...XX.......", "X..X...XX......X");
    assert!(read(bad.as_bytes(), &Default::default()).is_err());
}
#[test]
fn every_binary_truncation_and_resource_limit() {
    for n in 0..BIN.len() {
        assert!(read(&BIN[..n], &Default::default()).is_err(), "{n}");
    }
    for l in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 44,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 10,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: BIN.len() + 1,
            ..Default::default()
        },
        ResourceLimits {
            max_resources: 0,
            ..Default::default()
        },
    ] {
        assert!(read(BIN, &l).is_err());
    }
    let (s, _) = read(BIN, &Default::default()).unwrap();
    assert!(
        ssfn::write(
            &s,
            &options(),
            &SsfnWriteOptions {
                max_output_bytes: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    for total in [1024, 4096, 16384, 65536] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(w) = ssfn::write(
            &s,
            &options(),
            &SsfnWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            read(&w.output.into_single().unwrap(), &l).unwrap();
        }
    }
}
#[test]
fn negative_canvas_top_rejected_and_detector() {
    assert_eq!(ssfn::detect(BIN), fontpixel::formats::Confidence::Certain);
    let (mut s, _) = read(BIN, &Default::default()).unwrap();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.bounds.y = 100;
            Ok(())
        })
        .unwrap();
    assert!(ssfn::write(&s, &options(), &Default::default()).is_err());
    let mut broken = BIN.to_vec();
    broken[16..20].fill(255);
    assert_eq!(
        ssfn::detect(&broken),
        fontpixel::formats::Confidence::NoMatch
    );
}
#[test]
fn author_terminal_skip_overshoot_is_a_bounded_repair() {
    let mut b = BIN.to_vec();
    let end = b.len() - 4;
    b[end - 1] += 1;
    let (s, r) = read(&b, &Default::default()).unwrap();
    assert_eq!(s.fonts[0].len(), 1);
    assert!(!r.has_loss());
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "ssfn.author-terminal-skip")
    );
    b[end - 1] += 1;
    assert!(read(&b, &Default::default()).is_err());
}
#[test]
fn ascii_unknown_property_and_font_budget() {
    let source = std::str::from_utf8(ASC)
        .unwrap()
        .replace("$type 3", "$unknown keep me\n$type 3");
    let (s, _) = read(source.as_bytes(), &Default::default()).unwrap();
    assert!(s.fonts[0].properties.get("ssfn.ascii.unknown").is_some());
    assert!(
        ssfn::write(&s, &options(), &Default::default())
            .unwrap()
            .report
            .has_loss()
    );
    assert!(
        read(
            ASC,
            &ResourceLimits {
                max_resources: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
}
