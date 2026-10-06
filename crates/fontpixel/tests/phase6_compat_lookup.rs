//! The indexed SPEC numeric lookup must preserve the former full-map scan.
#![cfg(feature = "compat-bdfparser")]

use fontpixel::bitmap::Bounds;
use fontpixel::compat::bdfparser::{Bitmap, Font};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::render::{MissingGlyph, RenderMode, RenderOptions, WritingDirection};
use fontpixel::{Bitmap as CoreBitmap, Font as CoreFont, Glyph as CoreGlyph};
use serde_json::{Map, Value, json};

fn glyph(name: &str, codes: &[Codepoint], rows: &str, advance: i64) -> CoreGlyph {
    let mut glyph = CoreGlyph::new(rows.parse::<CoreBitmap>().unwrap());
    glyph.name = Some(name.into());
    glyph.codepoints = codes.to_vec();
    glyph.metrics.bounds.x = -1;
    glyph.metrics.bounds.y = -1;
    glyph.metrics.advance = Some(Vector::new(advance, 0));
    glyph.metrics.vertical_advance = Some(Vector::new(2, 0));
    glyph
}

fn mixed_font() -> Font {
    use Codepoint::{Legacy as L, Unicode as U};
    let mut core = CoreFont::default();
    core.metrics.bounds = Some(Bounds::new(4, 2, -1, -1));
    core.charset.name = Some("iso10646".into());
    for glyph in [
        glyph("legacy-a", &[L(65)], "#...\n.#..", 4),
        glyph("overwritten-a", &[U(65)], ".#..\n..#.", 1),
        glyph("unicode-max", &[U(u32::MAX)], "...#\n#...", 3),
        glyph("surrogate", &[U(0xd800)], "..#.\n.#..", 2),
        glyph("legacy-nonscalar", &[L(0x110000)], ".###\n#...", 1),
        glyph("unicode-nonscalar", &[U(0x110001)], "#..#\n..#.", 4),
        glyph("legacy-high", &[L(0x8000_0000)], "#.##\n...#", 2),
        glyph("legacy-zero", &[L(0)], "####\n####", 4),
        glyph("unicode-zero", &[U(0)], "....\n....", 0),
        glyph("effective-a", &[U(65)], "#.#.\n.#.#", 3),
        glyph("alias", &[U(66), U(67)], "..##\n##..", 2),
        glyph("effective-c", &[U(67)], ".#.#\n#.#.", 4),
        glyph("legacy-b", &[L(66)], "##..\n..##", 1),
        glyph("legacy-max", &[L(u32::MAX)], ".##.\n.##.", 1),
        glyph("unencoded", &[], "####\n####", 4),
    ] {
        core.add_glyph(glyph).unwrap();
    }
    Font::from_core(core).unwrap()
}

// Retain the exact pre-optimization lookup as an independent regression oracle.
fn scanned_code(core: &CoreFont, number: i64) -> Option<Codepoint> {
    core.codepoints()
        .find(|(cp, _)| i64::from(cp.value()) == number)
        .map(|(cp, _)| cp)
}

fn scan_render(
    font: &Font,
    cps: &[i64],
    kw: &Map<String, Value>,
    missing: Option<&CoreGlyph>,
) -> fontpixel::Result<Bitmap> {
    // Synthetic fixtures have no source properties and already use the metric
    // projection drawcps produces. This isolates lookup while using the same
    // public layout engine, without reproducing private projection machinery.
    let core = font.core();
    let options = RenderOptions {
        line_limit: kw.get("linelimit").and_then(Value::as_i64).unwrap_or(512),
        mode: if kw.get("mode").and_then(Value::as_i64).unwrap_or(1) == 1 {
            RenderMode::Advance
        } else {
            RenderMode::Cell
        },
        direction: WritingDirection::from_spec(
            kw.get("direction").and_then(Value::as_str).unwrap_or("lr"),
        ),
        use_current_glyph_spacing: kw.get("ucgs").and_then(Value::as_bool).unwrap_or(false),
        missing: missing.map(MissingGlyph::Glyph).unwrap_or_default(),
        ..RenderOptions::default()
    };
    let rendered = core.draw_codepoints(cps.iter().map(|n| scanned_code(&core, *n)), &options)?;
    let rows = (0..rendered.bitmap.height())
        .map(|y| {
            (0..rendered.bitmap.width())
                .map(|x| char::from(b'0' + rendered.bitmap.get_pixel(x, y).unwrap()))
                .collect()
        })
        .collect::<Vec<_>>();
    Bitmap::new(&rows)
}

fn assert_scan_matches(
    font: &Font,
    cps: &[i64],
    kw: &Map<String, Value>,
    missing: Option<&CoreGlyph>,
) {
    let actual = font.drawcps(cps, kw);
    let expected = scan_render(font, cps, kw, missing);
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected, "cps={cps:?}, kw={kw:?}"),
        (Err(actual), Err(expected)) => {
            let (actual, expected) = (actual.to_string(), expected.to_string());
            if let Some(rest) = expected.strip_prefix("line_limit (") {
                // The compatibility layer reports the same check with bdfparser's text.
                let limit = rest.split(')').next().unwrap();
                let width = expected.rsplit("width ").next().unwrap();
                assert!(
                    actual.starts_with(&format!("`linelimit` ({limit}) is too small"))
                        && actual.ends_with(&format!("width: {width})")),
                    "{actual} versus {expected}"
                );
            } else {
                assert_eq!(actual, expected);
            }
        }
        (actual, expected) => panic!("lookup mismatch: {actual:?} versus {expected:?}, kw={kw:?}"),
    }
}

#[test]
fn numeric_domains_extremes_and_missing_match_the_scan() {
    use Codepoint::{Legacy as L, Unicode as U};
    let mut font = mixed_font();
    let core = font.core();
    // Ord is domain-first, not a merge by numeric value or source insertion.
    assert!(U(u32::MAX) < L(0));
    for (number, expected) in [
        (0, Some(U(0))),
        (65, Some(U(65))),
        (66, Some(U(66))),
        (0xd800, Some(U(0xd800))),
        (0x110000, Some(L(0x110000))),
        (0x110001, Some(U(0x110001))),
        (0x8000_0000, Some(L(0x8000_0000))),
        (i64::from(u32::MAX), Some(U(u32::MAX))),
        (68, None),
        (-1, None),
        (i64::MIN, None),
        (i64::from(u32::MAX) + 1, None),
        (i64::from(u32::MAX) + 66, None),
        (i64::MAX, None),
    ] {
        assert_eq!(scanned_code(&core, number), expected, "{number}");
        assert_scan_matches(&font, &[number], &Map::new(), None);
    }
    // Numeric access does not convert through, or prefer, the active charset.
    font.encoding("cp1252");
    assert_scan_matches(
        &font,
        &[0, 65, 66, 0xd800, i64::from(u32::MAX)],
        &Map::new(),
        None,
    );
}

#[test]
fn duplicate_effective_mapping_and_alias_expansion_match_the_scan() {
    let font = mixed_font();
    let core = font.core();
    for (number, name) in [(65, "effective-a"), (66, "alias"), (67, "effective-c")] {
        let code = scanned_code(&core, number).unwrap();
        assert_eq!(
            core.glyph_for_codepoint(code).unwrap().name.as_deref(),
            Some(name)
        );
        assert_scan_matches(&font, &[number, number], &Map::new(), None);
    }
    // Alias expansion leaves both copies of U+0043 in the native model; lookup
    // must use the last effective mapping, not the first matching glyph slot.
    assert_eq!(
        core.glyph_ids_for_codepoint(Codepoint::Unicode(67)).len(),
        2
    );
}

#[test]
fn layout_directions_modes_spacing_and_missing_metadata_match_the_scan() {
    let font = mixed_font();
    let cps = [65, 66, 67, 65, 0, -1, 68, 0xd800, 0x110000, i64::MAX];
    let missing = glyph("missing", &[], "#..#\n.##.", 3);
    let metadata = json!({
        "codepoint": -1, "bbw": 4, "bbh": 2, "bbxoff": -1, "bbyoff": -1,
        "dwx0": 3, "dwy0": 0, "dwx1": 2, "dwy1": 0, "hexdata": ["90", "60"]
    });
    for first in ["lr", "rl", "tb", "bt"] {
        for second in ["lr", "rl", "tb", "bt"] {
            for mode in [0, 1, 2] {
                for current in [false, true] {
                    for limit in [7, 512] {
                        let mut kw = json!({
                            "direction": format!("{first}{second}"), "mode": mode,
                            "ucgs": current, "linelimit": limit
                        })
                        .as_object()
                        .unwrap()
                        .clone();
                        assert_scan_matches(&font, &cps, &kw, None);
                        kw.insert("missing".into(), metadata.clone());
                        assert_scan_matches(&font, &cps, &kw, Some(&missing));
                    }
                }
            }
        }
    }
}

#[test]
fn drawall_orders_ranges_and_default_cell_mode_match_the_scan() {
    let font = mixed_font();
    for order in [-1, 0, 1, 2] {
        for range in [
            Value::Null,
            json!(67),
            json!([65, 67]),
            json!([[0, 0], [66, 67]]),
        ] {
            for mode in [None, Some(0), Some(1)] {
                let mut kw = json!({"order": order, "r": range, "linelimit": 11})
                    .as_object()
                    .unwrap()
                    .clone();
                if let Some(mode) = mode {
                    kw.insert("mode".into(), json!(mode));
                }
                let cps = font.itercps(order, &range).unwrap();
                let actual = font.drawall(&kw).unwrap();
                kw.entry("mode").or_insert(json!(0));
                assert_eq!(actual, scan_render(&font, &cps, &kw, None).unwrap());
            }
        }
    }
}

#[test]
fn mutations_use_the_current_index_and_fall_back_to_legacy() {
    let mut font = mixed_font();
    font.removeglyph(i64::from(u32::MAX));
    assert_eq!(
        scanned_code(&font.core(), i64::from(u32::MAX)),
        Some(Codepoint::Legacy(u32::MAX))
    );
    assert_scan_matches(&font, &[i64::from(u32::MAX)], &Map::new(), None);
    font.removeglyph(65);
    assert_eq!(scanned_code(&font.core(), 65), Some(Codepoint::Legacy(65)));
    assert_scan_matches(&font, &[65, 66, 65], &Map::new(), None);
    font.removeglyph(65);
    assert_eq!(scanned_code(&font.core(), 65), None);
    assert_scan_matches(&font, &[65, 66, 65], &Map::new(), None);
    font.addglyph(
        json!({
            "codepoint": 65, "bbw": 4, "bbh": 2, "bbxoff": -1, "bbyoff": -1,
            "dwx0": 3, "dwy0": 0, "dwx1": 2, "dwy1": 0, "hexdata": ["F0", "90"]
        })
        .as_object()
        .unwrap(),
        true,
    )
    .unwrap();
    assert_eq!(scanned_code(&font.core(), 65), Some(Codepoint::Unicode(65)));
    assert_scan_matches(&font, &[65, 66, 65], &Map::new(), None);
    font.glyphbycp(65)
        .unwrap()
        .setmeta(json!({"dwx0": 1}).as_object().unwrap())
        .unwrap();
    assert_scan_matches(&font, &[65, 66, 65], &Map::new(), None);
}

#[test]
fn empty_requests_empty_fonts_and_layout_errors_match_the_scan() {
    let font = mixed_font();
    for cps in [&[][..], &[-1, i64::MIN, i64::MAX][..], &[65, 66][..]] {
        for limit in [-1, 0, 1, i64::MAX] {
            let kw = json!({"linelimit": limit}).as_object().unwrap().clone();
            assert_scan_matches(&font, cps, &kw, None);
        }
        assert_scan_matches(&Font::default(), cps, &Map::new(), None);
    }
}
