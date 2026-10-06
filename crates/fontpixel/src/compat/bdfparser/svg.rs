//! Exact SVG bitmap and SVG-font serialization.
use super::projection::{num, text};
use super::{Bitmap, Font, bitmap::invalid, outline::bitmap_contours};
use crate::Result;
use serde_json::{Value as V, json};
/// Python-compatible number text (bdfparser `fmtnum`): integral values without
/// a fraction (`3.0` -> `3`, `-0.0` -> `0`), others as Python `repr`.
pub fn fmtnum(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    if n.fract() == 0.0 {
        return format!("{n:.0}");
    }
    super::projection::py_float(n)
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
impl Bitmap {
    /// Serialize a SPEC-v3 bitmap SVG, rect decomposition, or path data string.
    pub fn tosvg(
        &self,
        pixelsize: f64,
        format: &str,
        inverted: bool,
        xoff: f64,
        yoff: f64,
        shape: &str,
    ) -> Result<String> {
        if !matches!(format, "path" | "pathOnly" | "rect") || !matches!(shape, "square" | "circle")
        {
            return Err(invalid("Unknown SVG format or shape"));
        }
        let (w, h) = (self.width() as usize, self.height() as usize);
        let rows = self.bindata();
        let head = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">",
            fmtnum(w as f64 * pixelsize),
            fmtnum(h as f64 * pixelsize)
        );
        if format == "rect" {
            if shape != "square" {
                return Err(invalid("format='rect' only supports shape='square'"));
            }
            let mut out = head;
            let mut seen = vec![vec![false; w]; h];
            for y in 0..h {
                for x in 0..w {
                    if seen[y][x] {
                        continue;
                    }
                    let v = rows[y].as_bytes()[x];
                    let mut mw = 1;
                    while x + mw < w && rows[y].as_bytes()[x + mw] == v && !seen[y][x + mw] {
                        mw += 1;
                    }
                    let mut mh = 1;
                    while y + mh < h
                        && (0..mw)
                            .all(|dx| rows[y + mh].as_bytes()[x + dx] == v && !seen[y + mh][x + dx])
                    {
                        mh += 1;
                    }
                    for row in seen.iter_mut().skip(y).take(mh) {
                        row[x..x + mw].fill(true);
                    }
                    if v != b'0' {
                        let rx = x as f64 * pixelsize + xoff;
                        let ry = if inverted {
                            h as f64 * pixelsize - y as f64 * pixelsize - mh as f64 * pixelsize
                                + yoff
                        } else {
                            y as f64 * pixelsize + yoff
                        };
                        out.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#000000\" />",fmtnum(rx),fmtnum(ry),fmtnum(mw as f64*pixelsize),fmtnum(mh as f64*pixelsize)));
                    }
                }
            }
            out.push_str("</svg>");
            return Ok(out);
        }
        let mut d = String::new();
        if shape == "circle" {
            let r = pixelsize / 2.0;
            let arc = format!("a{} {} 0 1 0 ", fmtnum(r), fmtnum(r));
            for rb in 0..h {
                for c in 0..w {
                    if rows[h - 1 - rb].as_bytes()[c] == b'0' {
                        continue;
                    }
                    let cx = (c as f64 + 0.5) * pixelsize + xoff;
                    let cy = if inverted {
                        rb as f64 + 0.5
                    } else {
                        h as f64 - rb as f64 - 0.5
                    } * pixelsize
                        + yoff;
                    d.push_str(&format!(
                        "M{} {}{arc}{} 0{arc}{} 0Z",
                        fmtnum(cx - r),
                        fmtnum(cy),
                        fmtnum(2.0 * r),
                        fmtnum(-2.0 * r)
                    ));
                }
            }
        } else if w != 0 && h != 0 {
            for contour in bitmap_contours(&rows, "square", 1.0, 1, 0, 0)? {
                let pts = contour
                    .iter()
                    .map(|(x, y, _)| {
                        (
                            x * pixelsize + xoff,
                            if inverted { *y } else { h as f64 - y } * pixelsize + yoff,
                        )
                    })
                    .collect::<Vec<_>>();
                d.push_str(&format!("M{} {}", fmtnum(pts[0].0), fmtnum(pts[0].1)));
                let mut prev = pts[0].1;
                for (x, y) in &pts[1..] {
                    if *y == prev {
                        d.push_str(&format!("H{}", fmtnum(*x)));
                    } else {
                        d.push_str(&format!("V{}", fmtnum(*y)));
                    }
                    prev = *y;
                }
                d.push('Z');
            }
        }
        Ok(if format == "pathOnly" {
            d
        } else if d.is_empty() {
            head + "</svg>"
        } else {
            format!("{head}<path d=\"{d}\" fill=\"#000000\" /></svg>")
        })
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::Null => false,
        V::Bool(b) => *b,
        V::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        V::String(s) => !s.is_empty(),
        V::Array(a) => !a.is_empty(),
        V::Object(o) => !o.is_empty(),
    }
}
fn parseint(v: &V) -> Option<V> {
    if v.is_number() {
        return Some(v.clone());
    }
    let s = v.as_str()?.trim_start();
    let end = s
        .char_indices()
        .find(|(i, c)| !c.is_ascii_digit() && !(*i == 0 && matches!(c, '+' | '-')))
        .map(|v| v.0)
        .unwrap_or(s.len());
    let s = s[..end].trim_start_matches('+');
    let negative = s.starts_with('-');
    let digits = s.trim_start_matches('-').trim_start_matches('0');
    if digits.is_empty() {
        return Some(json!(0));
    }
    let number = format!("{}{}", if negative { "-" } else { "" }, digits);
    serde_json::from_str(&number).ok()
}

fn pyxml(v: &V) -> String {
    xml(&if v.is_f64() {
        let n = v.as_f64().unwrap_or(0.0);
        fmtnum(n)
    } else {
        text(v)
    })
}
impl Font {
    /// Serialize the exact SPEC-v3 SVG-font string, using Unicode charset mappings.
    pub fn tosvgfont(
        &self,
        xoff: f64,
        yoff: f64,
        ccns: bool,
        unicodeformat: &str,
    ) -> Result<String> {
        let h = self.headers();
        let p = self.props();
        let get = |key: &str| p.get(key).cloned().unwrap_or(V::Null);
        let hv = |key: &str| h.get(key).cloned().unwrap_or(V::Null);
        let fallback = |v: V, d: V| if truth(&v) { v } else { d };
        let upem = num(&hv("fbbx")).max(num(&hv("fbby")));
        let upem = if upem == 0 { 100 } else { upem };
        let asc = parseint(&get("font_ascent"))
            .filter(truth)
            .unwrap_or(json!(upem as f64 * 0.8));
        let desc = parseint(&get("font_descent"))
            .filter(truth)
            .unwrap_or(json!(-(upem as f64) * 0.2));
        let dw = fallback(hv("dwx0"), json!(upem));
        let mut out = "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\">\n".to_owned();
        let mut metadata = Vec::new();
        let family = fallback(get("family_name"), get("family"));
        if truth(&family) {
            metadata.push(format!("Title: {}", text(&family)));
        }
        for (label, key) in [
            ("Weight", "weight_name"),
            ("Version", "font_version"),
            ("Foundry", "foundry"),
            ("Copyright", "copyright"),
            ("Notice", "notice"),
        ] {
            let v = get(key);
            if truth(&v) {
                metadata.push(format!("{label}: {}", text(&v)));
            }
        }
        if !metadata.is_empty() {
            out.push_str(&format!(
                "<metadata>\n  {}\n</metadata>\n",
                xml(&metadata.join("\n  "))
            ));
        }
        let mut cps = self
            .itercps(0, &V::Null)?
            .iter()
            .filter_map(|cp| self.cp2char(*cp).ok().flatten().map(|c| c as u32))
            .collect::<Vec<_>>();
        cps.sort_unstable();
        cps.dedup();
        let mut ranges = Vec::new();
        let mut i = 0;
        while i < cps.len() {
            let mut j = i;
            while j + 1 < cps.len() && cps[j + 1] == cps[j] + 1 {
                j += 1;
            }
            ranges.push(if i == j {
                format!("U+{:04X}", cps[i])
            } else {
                format!("U+{:04X}-{:04X}", cps[i], cps[j])
            });
            i = j + 1;
        }
        let name = hv("fontname").as_str().unwrap_or("").to_lowercase();
        let name = name.strip_prefix('-').unwrap_or(&name);
        out.push_str(&format!(
            "<defs>\n  <font id=\"{}\" horiz-adv-x=\"{}\">",
            xml(name),
            fmtnum(dw.as_f64().unwrap_or(0.0))
        ));
        let pi = |key: &str, d: V| parseint(&get(key)).filter(truth).unwrap_or(d);
        let face = vec![
            (
                "font-family",
                fallback(family, fallback(hv("fontname"), json!(""))),
            ),
            ("font-weight", fallback(get("weight_name"), json!("normal"))),
            ("font-style", fallback(get("slant"), json!("normal"))),
            ("units-per-em", json!(upem)),
            ("cap-height", pi("cap_height", asc.clone())),
            (
                "x-height",
                pi("x_height", json!(asc.as_f64().unwrap_or(0.0) * 0.5)),
            ),
            ("ascent", asc),
            ("descent", desc),
            (
                "bbox",
                json!(format!(
                    "{} {} {} {}",
                    num(&hv("fbbxoff")),
                    num(&hv("fbbyoff")),
                    text(&fallback(hv("fbbx"), json!(upem))),
                    text(&fallback(hv("fbby"), json!(upem)))
                )),
            ),
            (
                "underline-thickness",
                pi(
                    "underline_thickness",
                    json!((upem as f64 * 0.05 + 0.5).floor() as i64),
                ),
            ),
            (
                "underline-position",
                pi(
                    "underline_position",
                    json!((-(upem as f64) * 0.075 + 0.5).floor() as i64),
                ),
            ),
            ("unicode-range", json!(ranges.join(", "))),
        ];
        out.push_str("\n<font-face\n");
        for (k, v) in face {
            out.push_str(&format!("  {k}=\"{}\"\n", pyxml(&v)));
        }
        out.push_str("></font-face>\n");
        let u = upem.to_string();
        let boxpath = format!("M0,0h{u}v{u}h-{u}z");
        let adv = fmtnum(dw.as_f64().unwrap_or(0.0));
        out.push_str(&format!("<missing-glyph horiz-adv-x=\"{adv}\" d=\"{boxpath}\" />\n<glyph glyph-name=\".notdef\" horiz-adv-x=\"{adv}\" d=\"{boxpath}\" />\n"));
        for g in self.iterglyphs(1, &V::Null)? {
            let path = g
                .draw(0, None)?
                .tosvg(1.0, "pathOnly", true, xoff, yoff, "square")?;
            if path.is_empty() {
                continue;
            }
            let m = g.meta();
            let mut attrs = vec![("glyph-name", pyxml(&m[0]))];
            if let Some(ch) = self.cp2char(g.cp())?
                && (ccns || ch as u32 >= 32)
            {
                attrs.push((
                    "unicode",
                    match unicodeformat {
                        "decncr" => format!("&#{};", ch as u32),
                        "char" => xml(&ch.to_string()),
                        _ => format!("&#x{:x};", ch as u32),
                    },
                ));
            }
            attrs.push((
                "horiz-adv-x",
                pyxml(&fallback(m[8].clone(), fallback(hv("dwx0"), json!(upem)))),
            ));
            attrs.push(("d", xml(&path)));
            for (key, idx) in [
                ("vert-adv-y", 7),
                ("vert-origin-x", 14),
                ("vert-origin-y", 15),
            ] {
                if truth(&m[idx]) {
                    attrs.push((key, pyxml(&m[idx])));
                }
            }
            out.push_str("<glyph\n");
            for (k, v) in attrs {
                out.push_str(&format!("  {k}=\"{v}\"\n"));
            }
            out.push_str("></glyph>\n");
        }
        out.push_str("</font>\n</defs>\n</svg>");
        Ok(out)
    }
}
