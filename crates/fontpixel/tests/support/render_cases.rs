use fontpixel::model::Codepoint;
use fontpixel::render::{GlyphOrder, MissingGlyph, RenderMode, RenderOptions, WritingDirection};
use fontpixel::{Font, ReadOptions};
use serde_json::Value;

pub fn run_case(font: &Font, case: &Value) -> fontpixel::Result<fontpixel::render::Rendered> {
    let get = |key, default| case[key].as_i64().unwrap_or(default);
    let mut options = RenderOptions {
        line_limit: get("limit", 512),
        mode: if get("mode", 1) == 1 {
            RenderMode::Advance
        } else {
            RenderMode::Cell
        },
        direction: WritingDirection::from_spec(case["direction"].as_str().unwrap_or("lrtb")),
        use_current_glyph_spacing: case["current"].as_bool().unwrap_or(false),
        missing: case["missing"]
            .as_u64()
            .and_then(|cp| font.glyph_for_codepoint(Codepoint::Unicode(cp as u32)))
            .map_or(MissingGlyph::Blank, MissingGlyph::Glyph),
        ..Default::default()
    };
    options.limits.bitmap.max_pixels = 64 * 1024 * 1024;
    if case.get("all").is_some() {
        let order = match get("all", 1) {
            0 => GlyphOrder::File,
            2 => GlyphOrder::Descending,
            -1 => GlyphOrder::ReverseFile,
            _ => GlyphOrder::Ascending,
        };
        let ranges: Vec<_> = case["ranges"]
            .as_array()
            .map(|r| {
                r.iter()
                    .map(|r| (r[0].as_u64().unwrap() as u32, r[1].as_u64().unwrap() as u32))
                    .collect()
            })
            .unwrap_or_default();
        font.draw_all(order, &ranges, &options)
    } else if let Some(text) = case["text"].as_str() {
        font.draw(text, &options)
    } else {
        font.draw_codepoints(
            case["cps"].as_array().unwrap().iter().map(|v| {
                v.as_u64().map(|cp| {
                    let c = Codepoint::Unicode(cp as u32);
                    if font.glyph_id(c).is_some() {
                        c
                    } else {
                        Codepoint::Legacy(cp as u32)
                    }
                })
            }),
            &options,
        )
    }
}
pub fn check(data: &Value) -> (usize, usize) {
    let fonts: Vec<_> = data["fonts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            fontpixel::read(
                f["bdf"].as_str().unwrap().as_bytes(),
                &ReadOptions {
                    format: Some("bdf".into()),
                    ..ReadOptions::default()
                },
            )
            .unwrap()
            .fonts
            .into_single()
            .unwrap()
        })
        .collect();
    let (mut good, mut errors) = (0, 0);
    for (i, case) in data["cases"].as_array().unwrap().iter().enumerate() {
        let actual = run_case(&fonts[case["font"].as_u64().unwrap() as usize], case);
        let expected = &case["expected"];
        if expected.get("error").is_some() {
            assert!(actual.is_err(), "case {i}: expected error {}", expected);
            errors += 1;
        } else {
            let r = actual.unwrap_or_else(|e| panic!("case {i}, font {}: {e}", case["font"]));
            let rows: Vec<_> = (0..r.bitmap.height())
                .map(|y| {
                    (0..r.bitmap.width())
                        .map(|x| {
                            if r.bitmap.get_pixel(x, y).unwrap() == 0 {
                                '0'
                            } else {
                                '1'
                            }
                        })
                        .collect::<String>()
                })
                .collect();
            assert_eq!(
                serde_json::json!({"rows":rows,"width":r.bitmap.width(),"height":r.bitmap.height()}),
                *expected,
                "case {i}, font {}",
                case["font"]
            );
            good += 1;
        }
    }
    (good, errors)
}
