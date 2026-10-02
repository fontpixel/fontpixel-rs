use super::bitmap::invalid;
use crate::Result;
use crate::bitmap::{Bitmap as Raster, Bounds, Mono};
use crate::formats::x11;
use crate::model::{Codepoint, Font as CoreFont, Glyph as CoreGlyph, PropertyValue as P, Vector};
use serde_json::{Map, Value as V, json};

/// Seventeen ordered SPEC-v3 glyph metadata slots; null metrics remain distinct.
pub type Meta = Vec<V>;
pub(super) const META: &str = "compat.bdfparser.meta";
pub(super) fn value(p: &P) -> V {
    match p {
        P::Integer(n) => json!(n),
        P::Unsigned(n) => json!(n),
        P::Decimal(s) => serde_json::from_str(s)
            .ok()
            .or_else(|| s.parse::<f64>().ok().map(|n| json!(n)))
            .unwrap_or(json!(s)),
        P::Text(s) => json!(s),
        P::Flag => V::Null,
        P::List(v) => V::Array(v.iter().map(value).collect()),
        P::Bytes(b) => json!(b),
    }
}
pub(super) fn property(v: &V) -> P {
    match v {
        V::Null => P::Flag,
        V::Bool(b) => P::Integer(i64::from(*b)),
        V::Number(n) => n
            .as_i64()
            .map(P::Integer)
            .or_else(|| n.as_u64().map(P::Unsigned))
            .unwrap_or(P::Decimal(n.to_string())),
        V::String(s) => P::Text(s.clone()),
        V::Array(a) => P::List(a.iter().map(property).collect()),
        V::Object(_) => P::Text(v.to_string()),
    }
}
pub(super) fn integer(v: &V) -> Result<i64> {
    v.as_i64().ok_or_else(|| invalid("expected signed integer"))
}
pub(super) fn num(v: &V) -> i64 {
    v.as_i64().unwrap_or(0)
}
pub(super) fn text(v: &V) -> String {
    match v {
        V::String(s) => s.clone(),
        V::Null => "None".into(),
        V::Bool(b) => if *b { "True" } else { "False" }.into(),
        _ => v.to_string(),
    }
}
pub(super) fn headers(f: &CoreFont) -> Map<String, V> {
    let mut h = Map::new();
    for p in f.headers.iter() {
        let titles: &[&str] = match p.name.as_str() {
            "STARTFONT" => &["bdfversion"],
            "FONT" => &["fontname"],
            "SIZE" => &["pointsize", "xres", "yres"],
            "FONTBOUNDINGBOX" => &["fbbx", "fbby", "fbbxoff", "fbbyoff"],
            "SWIDTH" => &["swx0", "swy0"],
            "DWIDTH" => &["dwx0", "dwy0"],
            "SWIDTH1" => &["swx1", "swy1"],
            "DWIDTH1" => &["dwx1", "dwy1"],
            "VVECTOR" => &["vvectorx", "vvectory"],
            "METRICSSET" => &["metricsset"],
            "CONTENTVERSION" => &["contentversion"],
            "COMMENT" => {
                let a = h.entry("comment").or_insert(json!([]));
                if let Some(a) = a.as_array_mut() {
                    match value(&p.value) {
                        V::Array(v) => a.extend(v),
                        v => a.push(v),
                    }
                }
                continue;
            }
            _ => continue,
        };
        let v = value(&p.value);
        let vals = if titles.len() == 1 {
            vec![v]
        } else {
            v.as_array().cloned().unwrap_or_default()
        };
        for (k, v) in titles.iter().zip(vals) {
            h.insert((*k).into(), v);
        }
    }
    h
}
pub(super) fn props(f: &CoreFont) -> (Map<String, V>, Map<String, V>) {
    let (mut p, mut names) = (Map::new(), Map::new());
    for prop in f.properties.iter() {
        let key = prop.name.to_lowercase();
        if key == "comment" && prop.name == "comment" && prop.raw.is_none() {
            p.insert(key, value(&prop.value));
        } else if key == "comment" {
            let e = p.entry(key).or_insert(json!([]));
            if let Some(a) = e.as_array_mut() {
                match value(&prop.value) {
                    V::Array(v) => a.extend(v),
                    v => a.push(v),
                }
            }
        } else if !key.starts_with("bdf.")
            && !key.starts_with("pcf.")
            && !key.starts_with("compat.")
        {
            p.insert(key.clone(), value(&prop.value));
            names.insert(key, json!(prop.name));
        }
    }
    (p, names)
}
pub(super) fn meta(g: &CoreGlyph) -> Meta {
    if let Some(P::List(m)) = g.properties.get(META) {
        return m.iter().map(value).collect();
    }
    let b = g.metrics.bounds;
    let mut m = vec![
        json!(g.name),
        json!(
            g.codepoints
                .first()
                .map(|c| i64::from(c.value()))
                .unwrap_or(-1)
        ),
        json!(b.width),
        json!(b.height),
        json!(b.x),
        json!(b.y),
    ];
    for v in [
        g.metrics.scalable_advance,
        g.metrics.advance,
        g.metrics.vertical_scalable_advance,
        g.metrics.vertical_advance,
        g.metrics.vertical_origin,
    ] {
        m.push(v.map(|v| json!(v.x)).unwrap_or(V::Null));
        m.push(v.map(|v| json!(v.y)).unwrap_or(V::Null));
    }
    let rows = if g.metrics.bounds.width == 0 && g.properties.get("pcf.attributes").is_some() {
        vec![]
    } else {
        x11::source_rows(g)
            .map(|r| r.map(str::to_owned).collect::<Vec<_>>())
            .unwrap_or_else(|| match &g.bitmap {
                crate::model::GlyphBitmap::Mono(b) => b.to_hex_rows().unwrap_or_default(),
                _ => vec![],
            })
    };
    m.push(json!(rows));
    m
}
pub(super) fn core_glyph(m: &Meta) -> Result<CoreGlyph> {
    if m.len() != 17 {
        return Err(invalid("glyph needs 17 metadata slots"));
    }
    let w = u32::try_from(num(&m[2])).unwrap_or(0);
    let h = u32::try_from(num(&m[3])).unwrap_or(0);
    let mut b = Raster::<Mono>::new(w, h)?;
    let rows = m[16].as_array().cloned().unwrap_or_default();
    for (y, row) in rows.iter().take(h as usize).enumerate() {
        if let Some(s) = row.as_str() {
            for (n, c) in s.chars().enumerate() {
                let Some(v) = c.to_digit(16) else { continue };
                for i in 0..4 {
                    let x = n * 4 + i;
                    if x < w as usize && v & (8 >> i) != 0 {
                        b.set_pixel(x as u32, y as u32, 1)?;
                    }
                }
            }
        }
    }
    let mut g = CoreGlyph::new(b);
    g.name = m[0].as_str().map(str::to_owned);
    if let Some(cp) = m[1].as_u64().and_then(|n| u32::try_from(n).ok()) {
        g.codepoints.push(Codepoint::Unicode(cp));
    }
    g.metrics.bounds = Bounds::new(w, h, num(&m[4]), num(&m[5]));
    for (i, k) in [
        (6, "SWIDTH"),
        (8, "DWIDTH"),
        (10, "SWIDTH1"),
        (12, "DWIDTH1"),
        (14, "VVECTOR"),
    ] {
        if let (Some(x), Some(y)) = (m[i].as_i64(), m[i + 1].as_i64()) {
            x11::set_vector(&mut g.metrics, k, Vector::new(x, y));
        }
    }
    g.properties
        .set(META, P::List(m.iter().map(property).collect()));
    x11::save_bitmap(
        &mut g,
        rows.iter()
            .filter_map(V::as_str)
            .map(|s| format!("{s}\n"))
            .collect(),
    );
    g.properties.set(x11::ENCODING, num(&m[1]));
    Ok(g)
}
