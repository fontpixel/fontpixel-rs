use fontpixel::bitmap::{Alignment, Bitmap, BitmapLimits, Direction, Gray8};
use serde_json::Value;

fn bitmap(rows: &Value) -> Bitmap<Gray8> {
    let rows: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    let width = rows.first().map_or(0, |row| row.len()) as u32;
    let data: Vec<u8> = rows
        .iter()
        .flat_map(|r| r.bytes().map(|b| b - b'0'))
        .collect();
    Bitmap::from_raw(width, rows.len() as u32, &data, BitmapLimits::default()).unwrap()
}
pub fn check(case: &Value) {
    let source = bitmap(&case["rows"]);
    let args: Vec<i64> = case["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let result = match case["op"].as_str().unwrap() {
        "crop" => source
            .crop(args[0] as u32, args[1] as u32, args[2], args[3])
            .unwrap(),
        "shift" => source.shift(args[0], args[1]).unwrap(),
        "fliph" => source.flip_horizontal().unwrap(),
        "flipv" => source.flip_vertical().unwrap(),
        "rotate" => source.rotate(args[0] as i32).unwrap(),
        "invert" => {
            let mut out = source;
            out.invert_with(1).unwrap();
            out
        }
        "trim" => source.trim().unwrap(),
        "bold" => source.bold(args[0] as u32).unwrap(),
        "italic" => source.italic(args[0], args[1]).unwrap(),
        "shadow" => source.shadow(args[0], args[1], 2).unwrap(),
        "glow" => source.glow(args[0] == 1, 1, 2).unwrap(),
        "bytepad" => source.byte_pad(args[0] as u32).unwrap(),
        "enlarge" => source
            .enlarge(args[0].max(1) as u32, args[1].max(1) as u32)
            .unwrap(),
        "paste" => {
            let mut out = source;
            out.paste(&bitmap(&case["other"]), args[0], args[1]);
            out
        }
        "overlay" => {
            let mut out = source;
            out.overlay(&bitmap(&case["other"]));
            out
        }
        "replace" => source
            .replace_pattern(
                &case["old"]
                    .as_str()
                    .unwrap()
                    .bytes()
                    .map(|v| v - b'0')
                    .collect::<Vec<_>>(),
                &case["new"]
                    .as_str()
                    .unwrap()
                    .bytes()
                    .map(|v| v - b'0')
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        "concatall" => {
            let bitmaps: Vec<_> = case["bitmaps"]
                .as_array()
                .unwrap()
                .iter()
                .map(bitmap)
                .collect();
            let refs: Vec<_> = bitmaps.iter().collect();
            let direction = match args[0] {
                1 => Direction::Right,
                2 => Direction::Left,
                0 => Direction::Down,
                _ => Direction::Up,
            };
            let start = if args[0] > 0 {
                args[1] == 0
            } else {
                args[1] == 1
            };
            let align = if start {
                Alignment::Start
            } else {
                Alignment::End
            };
            let gaps: Vec<_> = case["gaps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect();
            Bitmap::concat_all(&refs, direction, align, &gaps, BitmapLimits::default()).unwrap()
        }
        other => panic!("unknown fixture operation {other}"),
    };
    let rows: Vec<String> = (0..result.height())
        .map(|y| {
            (0..result.width())
                .map(|x| char::from(b'0' + result.get_pixel(x, y).unwrap()))
                .collect()
        })
        .collect();
    let expected: Vec<&str> = case["expected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(rows, expected, "case {}: {}", case["id"], case);
    let bbox = result.bbox();
    assert_eq!(
        serde_json::json!([bbox.width, bbox.height, bbox.x, bbox.y]),
        case["bbox"],
        "bbox case {}",
        case["id"]
    );
}
