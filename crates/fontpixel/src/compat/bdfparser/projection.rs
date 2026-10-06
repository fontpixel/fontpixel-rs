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
/// Python `repr(float)`: shortest round-trip digits, exponent form outside 1e-4..1e16.
pub(super) fn py_float(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.into();
    }
    let s = format!("{f:e}");
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let exp = e.parse::<i32>().unwrap_or(0);
    let sign = if m.starts_with('-') { "-" } else { "" };
    let digits = m.trim_start_matches('-').replace('.', "");
    let n = digits.len() as i32;
    if (-4..16).contains(&exp) {
        if exp < 0 {
            format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
        } else if n <= exp + 1 {
            format!("{sign}{digits}{}.0", "0".repeat((exp + 1 - n) as usize))
        } else {
            let (a, b) = digits.split_at((exp + 1) as usize);
            format!("{sign}{a}.{b}")
        }
    } else {
        let mantissa = if n > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits
        };
        let es = if exp < 0 { '-' } else { '+' };
        format!("{sign}{mantissa}e{es}{:02}", exp.abs())
    }
}
/// Python `repr(str)` for diagnostic texts (quote choice and escapes).
pub(super) fn py_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7f..=0xa0).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}
/// Whether a JSON number is an integer (Python `int`), including arbitrary precision.
pub(super) fn is_int(n: &serde_json::Number) -> bool {
    n.is_i64() || n.is_u64() || !n.to_string().contains(['.', 'e', 'E'])
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
