use fontpixel::model::{GlyphMetrics, Properties, PropertyValue, Vector};
use fontpixel::{Font, Glyph};
use serde_json::{Map, Value, json};
fn value(v: &PropertyValue) -> Value {
    match v {
        PropertyValue::Integer(v) => json!(v),
        PropertyValue::Unsigned(v) => json!(v),
        PropertyValue::Text(v) => json!(v),
        PropertyValue::Decimal(v) => json!(v.parse::<f64>().unwrap()),
        PropertyValue::Flag => Value::Null,
        PropertyValue::List(v) => v.iter().map(value).collect(),
        PropertyValue::Bytes(v) => json!(v),
    }
}
fn metric(m: &GlyphMetrics, key: &str) -> Option<Vector> {
    match key {
        "SWIDTH" => m.scalable_advance,
        "DWIDTH" => m.advance,
        "SWIDTH1" => m.vertical_scalable_advance,
        "DWIDTH1" => m.vertical_advance,
        _ => m.vertical_origin,
    }
}
fn add_metrics(obj: &mut Map<String, Value>, m: &GlyphMetrics) {
    for (kw, a, b) in [
        ("SWIDTH", "swx0", "swy0"),
        ("DWIDTH", "dwx0", "dwy0"),
        ("SWIDTH1", "swx1", "swy1"),
        ("DWIDTH1", "dwx1", "dwy1"),
        ("VVECTOR", "vvectorx", "vvectory"),
    ] {
        if let Some(v) = metric(m, kw) {
            obj.insert(a.into(), json!(v.x));
            obj.insert(b.into(), json!(v.y));
        }
    }
}
fn rows(g: &Glyph) -> Vec<String> {
    if let Some(rows) = fontpixel::formats::bdf::source_rows(g) {
        rows.map(str::to_owned).collect()
    } else {
        match &g.bitmap {
            fontpixel::model::GlyphBitmap::Mono(b) => b.to_hex_rows().unwrap(),
            _ => vec![],
        }
    }
}
fn glyph(g: &Glyph, cp: i64) -> Value {
    let b = g.metrics.bounds;
    let mut values = vec![
        json!(g.name),
        json!(cp),
        json!(b.width),
        json!(b.height),
        json!(b.x),
        json!(b.y),
    ];
    for key in ["SWIDTH", "DWIDTH", "SWIDTH1", "DWIDTH1", "VVECTOR"] {
        let m = metric(&g.metrics, key);
        values.push(json!(m.map(|v| v.x)));
        values.push(json!(m.map(|v| v.y)));
    }
    values.push(json!(rows(g)));
    Value::Array(values)
}
fn comments(p: &Properties) -> Vec<Value> {
    p.iter()
        .filter(|p| p.name.eq_ignore_ascii_case("COMMENT"))
        .map(|p| value(&p.value))
        .collect()
}
pub fn snapshot(font: &Font) -> Value {
    let mut h = Map::new();
    for p in font.headers.iter() {
        let key = match p.name.as_str() {
            "STARTFONT" => "bdfversion",
            "FONT" => "fontname",
            "METRICSSET" => "metricsset",
            "CONTENTVERSION" => "contentversion",
            _ => continue,
        };
        h.insert(key.into(), value(&p.value));
    }
    for (name, keys) in [
        ("SIZE", vec!["pointsize", "xres", "yres"]),
        (
            "FONTBOUNDINGBOX",
            vec!["fbbx", "fbby", "fbbxoff", "fbbyoff"],
        ),
    ] {
        if let Some(PropertyValue::List(v)) = font.headers.get(name) {
            for (key, v) in keys.iter().zip(v) {
                h.insert((*key).into(), value(v));
            }
        }
    }
    add_metrics(&mut h, &font.metrics.glyph_defaults);
    let c = comments(&font.headers);
    if !c.is_empty() {
        h.insert("comment".into(), Value::Array(c));
    }
    let mut p = Map::new();
    let mut names = Map::new();
    for prop in font
        .properties
        .iter()
        .filter(|p| !p.name.eq_ignore_ascii_case("COMMENT"))
    {
        p.insert(prop.name.to_lowercase(), value(&prop.value));
        names.insert(prop.name.to_lowercase(), json!(prop.name));
    }
    let c = comments(&font.properties);
    if !c.is_empty() {
        p.insert("comment".into(), Value::Array(c));
    }
    let mut glyphs = Vec::new();
    let mut unencoded = Vec::new();
    for (id, g) in font.glyphs() {
        if g.codepoints.is_empty() {
            unencoded.push(
                json!({"altcode":g.properties.get("bdf.altcode").map(value),"meta":glyph(g,-1)}),
            );
        } else {
            for &cp in &g.codepoints {
                if font.glyph_id(cp) == Some(id) {
                    glyphs.push(glyph(g, i64::from(cp.value())));
                }
            }
        }
    }
    json!({"headers":h,"props":p,"propnames":names,"glyphs":glyphs,"unencoded":unencoded})
}
