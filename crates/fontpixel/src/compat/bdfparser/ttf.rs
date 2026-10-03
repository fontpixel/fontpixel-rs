//! Deterministic pixel TrueType compatibility codec, SPEC-v3 §8.
//! This narrowly-scoped codec serves parity; native sfnt integration is separate.
use super::projection::{meta, num, props, text};
use super::{
    Font, Meta,
    bitmap::invalid,
    defaultglyphname,
    outline::{Contour, bitmap_contours, rasterize},
};
use crate::Result;
use serde_json::{Value as V, json};
use std::collections::{BTreeMap, BTreeSet};
/// SPEC TrueType output options.
#[derive(Debug, Clone)]
pub struct TtfOptions {
    /// Square merged outlines or individual circles.
    pub shape: String,
    /// Pixel-relative dot diameter, in (0, 1.5].
    pub dotsize: f64,
    /// Even integer font units per pixel, at least 2.
    pub ppu: i64,
    /// Include lossless BDFP metadata.
    pub metadata: bool,
    /// Unix timestamp; zero is deterministic.
    pub timestamp: i64,
}
impl Default for TtfOptions {
    fn default() -> Self {
        Self {
            shape: "square".into(),
            dotsize: 1.0,
            ppu: 64,
            metadata: true,
            timestamp: 0,
        }
    }
}
fn pack(fmt: &str, vals: &[i64]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut it = vals.iter();
    for c in fmt.chars() {
        if c == '>' {
            continue;
        }
        let n = *it.next().ok_or_else(|| invalid("missing sfnt field"))?;
        match c {
            'H' => out.extend(
                u16::try_from(n)
                    .map_err(|_| invalid("TrueType field exceeds uint16"))?
                    .to_be_bytes(),
            ),
            'h' => out.extend(
                i16::try_from(n)
                    .map_err(|_| invalid("TrueType field exceeds int16"))?
                    .to_be_bytes(),
            ),
            'I' => out.extend(
                u32::try_from(n)
                    .map_err(|_| invalid("TrueType field exceeds uint32"))?
                    .to_be_bytes(),
            ),
            'i' => out.extend(
                i32::try_from(n)
                    .map_err(|_| invalid("TrueType field exceeds int32"))?
                    .to_be_bytes(),
            ),
            'q' => out.extend(n.to_be_bytes()),
            _ => return Err(invalid("invalid internal sfnt field")),
        }
    }
    if it.next().is_some() {
        return Err(invalid("extra sfnt fields"));
    }
    Ok(out)
}
fn pad4(mut data: Vec<u8>) -> Vec<u8> {
    data.resize(data.len().next_multiple_of(4), 0);
    data
}
#[derive(Clone)]
struct G {
    cs: Vec<Contour>,
    advance: i64,
    bounds: Option<[i64; 4]>,
    name: String,
    unicode: Option<u32>,
}
impl G {
    fn new(cs: Vec<Contour>, advance: i64) -> Self {
        let mut bounds = None::<[i64; 4]>;
        for p in cs.iter().flatten() {
            let (x, y) = (p.0 as i64, p.1 as i64);
            if let Some(b) = bounds.as_mut() {
                b[0] = b[0].min(x);
                b[1] = b[1].min(y);
                b[2] = b[2].max(x);
                b[3] = b[3].max(y);
            } else {
                bounds = Some([x, y, x, y]);
            }
        }
        Self {
            cs,
            advance,
            bounds,
            name: String::new(),
            unicode: None,
        }
    }
}
fn encode_glyf(g: &G) -> Result<Vec<u8>> {
    if g.cs.is_empty() {
        return Ok(vec![]);
    }
    let b = g.bounds.ok_or_else(|| invalid("empty contour"))?;
    let mut out = pack("hhhhh", &[g.cs.len() as i64, b[0], b[1], b[2], b[3]])?;
    let mut end = -1;
    for c in &g.cs {
        end += c.len() as i64;
        out.extend(pack("H", &[end])?);
    }
    out.extend([0, 0]);
    let (mut flags, mut xb, mut yb) = (Vec::new(), Vec::new(), Vec::new());
    let (mut px, mut py) = (0, 0);
    for &(x, y, on) in g.cs.iter().flatten() {
        let (x, y) = (x as i64, y as i64);
        let (dx, dy) = (x - px, y - py);
        px = x;
        py = y;
        let mut f = u8::from(on);
        if dx == 0 {
            f |= 0x10;
        } else if (-255..=255).contains(&dx) {
            f |= 2 | if dx > 0 { 0x10 } else { 0 };
            xb.push(dx.unsigned_abs() as u8);
        } else {
            xb.extend(pack("h", &[dx])?);
        }
        if dy == 0 {
            f |= 0x20;
        } else if (-255..=255).contains(&dy) {
            f |= 4 | if dy > 0 { 0x20 } else { 0 };
            yb.push(dy.unsigned_abs() as u8);
        } else {
            yb.extend(pack("h", &[dy])?);
        }
        flags.push(f);
    }
    out.extend(flags);
    out.extend(xb);
    out.extend(yb);
    Ok(pad4(out))
}
fn runs(mapping: &[(u32, usize)]) -> Vec<(u32, u32, usize)> {
    let mut out = Vec::<(u32, u32, usize)>::new();
    for &(c, g) in mapping {
        if let Some((s, e, sg)) = out.last_mut()
            && c == *e + 1
            && g == *sg + (c - *s) as usize
        {
            *e = c;
            continue;
        }
        out.push((c, c, g));
    }
    out
}
fn cmap(mapping: &[(u32, usize)]) -> Result<Vec<u8>> {
    let filtered = mapping
        .iter()
        .copied()
        .filter(|v| v.0 <= 0xfffe)
        .collect::<Vec<_>>();
    let mut segs = runs(&filtered)
        .into_iter()
        .take(8188)
        .map(|(s, e, g)| (s as i64, e as i64, (g as i64 - s as i64).rem_euclid(65536)))
        .collect::<Vec<_>>();
    segs.push((65535, 65535, 1));
    let n = segs.len() as i64;
    let lg = n.ilog2() as i64;
    let sr = 2 * (1 << lg);
    let mut sub4 = pack("HHHHHHH", &[4, 16 + 8 * n, 0, 2 * n, sr, lg, 2 * n - sr])?;
    for s in &segs {
        sub4.extend(pack("H", &[s.1])?);
    }
    sub4.extend([0, 0]);
    for s in &segs {
        sub4.extend(pack("H", &[s.0])?);
    }
    for s in &segs {
        sub4.extend(pack("H", &[s.2])?);
    }
    sub4.extend(vec![0; 2 * n as usize]);
    let r = runs(mapping);
    let mut sub12 = pack(
        "HHIII",
        &[12, 0, 16 + 12 * r.len() as i64, 0, r.len() as i64],
    )?;
    for (s, e, g) in r {
        sub12.extend(pack("III", &[s as i64, e as i64, g as i64])?);
    }
    let mut out = pack("HHHHIHHI", &[0, 2, 3, 1, 20, 3, 10, 20 + sub4.len() as i64])?;
    out.extend(sub4);
    out.extend(sub12);
    Ok(out)
}
fn name_table(records: &[(i64, String)]) -> Result<Vec<u8>> {
    let mut out = pack(
        "HHH",
        &[0, records.len() as i64, 6 + 12 * records.len() as i64],
    )?;
    let mut strings = Vec::new();
    for (n, s) in records {
        let b = s
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        out.extend(pack(
            "HHHHHH",
            &[3, 1, 0x409, *n, b.len() as i64, strings.len() as i64],
        )?);
        strings.extend(b);
    }
    out.extend(strings);
    Ok(out)
}
fn assemble(tables: BTreeMap<&str, Vec<u8>>) -> Result<Vec<u8>> {
    let mut builder = crate::sfnt::SfntBuilder::new(crate::sfnt::Flavor::TrueType);
    for (tag, data) in tables {
        builder.insert(
            tag.as_bytes()
                .try_into()
                .map_err(|_| invalid("invalid sfnt tag"))?,
            data,
        )?;
    }
    builder.build(&crate::sfnt::Limits::default())
}
fn source_bitmap(m: &Meta) -> Result<Vec<String>> {
    let (w, h) = (num(&m[2]), num(&m[3]));
    if w <= 0 || h <= 0 {
        return Ok(vec![]);
    }
    if w as u64 * h as u64 > 1 << 24 {
        return Err(invalid("TrueType glyph exceeds pixel budget"));
    }
    let mut out = Vec::new();
    let hex = m[16].as_array().map(Vec::as_slice).unwrap_or(&[]);
    for y in 0..h as usize {
        let row = hex.get(y).and_then(V::as_str).unwrap_or("");
        let mut bits = String::new();
        for c in row.chars() {
            let v = c
                .to_digit(16)
                .ok_or_else(|| invalid("non-hex character in glyph bitmap"))?;
            bits.push_str(&format!("{v:04b}"));
        }
        bits.truncate(w as usize);
        bits.push_str(&"0".repeat(w as usize - bits.len()));
        out.push(bits);
    }
    Ok(out)
}
/// Write exact deterministic SPEC TrueType bytes without an external executable.
pub fn write_ttf(font: &Font, o: &TtfOptions) -> Result<Vec<u8>> {
    let ppu = o.ppu;
    if ppu < 2 || ppu % 2 != 0 {
        return Err(invalid("ppu must be an even integer >= 2"));
    }
    bitmap_contours(&[], &o.shape, o.dotsize, ppu, 0, 0)?;
    let h = font.headers();
    let p = font.props();
    let hn = |k: &str| h.get(k).map(num).unwrap_or(0);
    let ip = |k: &str| p.get(k).and_then(V::as_i64);
    let sp = |k: &str| p.get(k).and_then(V::as_str);
    let (fbbx, fbby, fbbyoff) = (hn("fbbx"), hn("fbby"), hn("fbbyoff"));
    let ascent = ip("font_ascent").unwrap_or(fbby + fbbyoff);
    let descent = ip("font_descent").unwrap_or(-fbbyoff);
    let em = ip("pixel_size")
        .filter(|v| *v > 0)
        .unwrap_or(if ascent + descent > 0 {
            ascent + descent
        } else {
            fbby
        });
    let upem = em
        .checked_mul(ppu)
        .ok_or_else(|| invalid("unitsPerEm overflow"))?;
    if !(16..=16384).contains(&upem) {
        return Err(invalid("unitsPerEm is outside 16..16384"));
    }
    let bold = sp("weight_name").is_some_and(|s| s.to_lowercase().contains("bold"));
    let italic = sp("slant").is_some_and(|s| matches!(s.to_uppercase().as_str(), "I" | "O"));
    let make = |m: &Meta| -> Result<G> {
        let adv = m[8]
            .as_i64()
            .or_else(|| h.get("dwx0").and_then(V::as_i64))
            .unwrap_or((num(&m[4]) + num(&m[2])).max(0));
        Ok(G::new(
            bitmap_contours(
                &source_bitmap(m)?,
                &o.shape,
                o.dotsize,
                ppu,
                num(&m[4]),
                num(&m[5]),
            )?,
            adv.checked_mul(ppu)
                .ok_or_else(|| invalid("advance overflow"))?,
        ))
    };
    let mut point_budget = 0usize;
    let mut glyphs = vec![
        if let Some(g) = ip("default_char").and_then(|cp| font.glyphbycp(cp)) {
            make(g.meta())?
        } else {
            G::new(vec![], fbbx * ppu)
        },
    ];
    glyphs[0].name = ".notdef".into();
    let mut order = font
        .iterglyphs(1, &V::Null)?
        .iter()
        .map(|g| (g.cp(), g.meta().clone(), V::Null))
        .collect::<Vec<_>>();
    let core = font.core();
    order.extend(
        core.glyphs()
            .filter(|(_, g)| g.codepoints.is_empty())
            .map(|(_, g)| {
                (
                    -1,
                    meta(g),
                    g.properties
                        .get(crate::formats::x11::ALT)
                        .map(super::projection::value)
                        .unwrap_or(V::Null),
                )
            }),
    );
    let nencoded = font.length();
    let mut gids = BTreeMap::new();
    for (i, (cp, m, _)) in order.iter().enumerate() {
        let mut g = make(m)?;
        point_budget += g.cs.iter().map(Vec::len).sum::<usize>();
        if point_budget > 4 * 1024 * 1024 {
            return Err(invalid("TrueType font point budget exceeded"));
        }
        if i < nencoded {
            g.unicode = font.cp2char(*cp)?.map(|c| c as u32);
            gids.insert(*cp, i + 1);
        }
        glyphs.push(g);
    }
    let n = glyphs.len() as i64;
    if n > 65535 {
        return Err(invalid("too many glyphs for TrueType"));
    }
    let mut used = BTreeSet::from([".notdef".to_owned()]);
    for (i, (_, m, _)) in order.iter().enumerate() {
        let gid = i + 1;
        let g = &mut glyphs[gid];
        let bname = m[0].as_str().unwrap_or("");
        let valid = !bname.is_empty()
            && bname.len() <= 63
            && bname
                .as_bytes()
                .first()
                .is_some_and(|c| c.is_ascii_alphabetic() || b"_.".contains(c))
            && bname
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.".contains(&c));
        let fallback = g.unicode.map(|cp| defaultglyphname(cp as i64, true));
        g.name = if valid && !used.contains(bname) {
            bname.into()
        } else if let Some(n) = fallback.filter(|n| !used.contains(n)) {
            n
        } else {
            format!("glyph{gid}")
        };
        used.insert(g.name.clone());
    }
    let mut umap = BTreeMap::new();
    for (gid, g) in glyphs.iter().enumerate() {
        if let Some(u) = g.unicode {
            umap.entry(u).or_insert(gid);
        }
    }
    let mapping = umap.into_iter().collect::<Vec<_>>();
    let ink = glyphs
        .iter()
        .filter_map(|g| g.bounds.map(|b| (g, b)))
        .collect::<Vec<_>>();
    let min = |f: fn(&G, [i64; 4]) -> i64| ink.iter().map(|(g, b)| f(g, *b)).min().unwrap_or(0);
    let max = |f: fn(&G, [i64; 4]) -> i64| ink.iter().map(|(g, b)| f(g, *b)).max().unwrap_or(0);
    let (xmin, ymin, xmax, ymax) = (
        min(|_, b| b[0]),
        min(|_, b| b[1]),
        max(|_, b| b[2]),
        max(|_, b| b[3]),
    );
    let min_lsb = xmin;
    let min_rsb = min(|g, b| g.advance - b[2]);
    let adv_max = glyphs.iter().map(|g| g.advance).max().unwrap_or(0);
    let max_points = glyphs
        .iter()
        .map(|g| g.cs.iter().map(Vec::len).sum::<usize>())
        .max()
        .unwrap_or(0) as i64;
    let max_contours = glyphs.iter().map(|g| g.cs.len()).max().unwrap_or(0) as i64;
    let hasc = ascent * ppu;
    let hdesc = -descent * ppu;
    let ts = o
        .timestamp
        .checked_add(2082844800)
        .ok_or_else(|| invalid("timestamp overflow"))?;
    let mut tables = BTreeMap::new();
    tables.insert(
        "head",
        pack(
            "IIIIHHqqhhhhHHhhh",
            &[
                0x10000,
                0x10000,
                0,
                0x5f0f3cf5,
                0xb,
                upem,
                ts,
                ts,
                xmin,
                ymin,
                xmax,
                ymax,
                i64::from(bold) | (i64::from(italic) * 2),
                em,
                2,
                1,
                0,
            ],
        )?,
    );
    tables.insert(
        "hhea",
        pack(
            "IhhhHhhhhhhhhhhhH",
            &[
                0x10000, hasc, hdesc, 0, adv_max, min_lsb, min_rsb, xmax, 1, 0, 0, 0, 0, 0, 0, 0, n,
            ],
        )?,
    );
    tables.insert(
        "maxp",
        pack(
            "IHHHHHHHHHHHHHH",
            &[
                0x10000,
                n,
                max_points,
                max_contours,
                0,
                0,
                2,
                0,
                0,
                0,
                0,
                1,
                0,
                0,
                0,
            ],
        )?,
    );
    let advs = glyphs
        .iter()
        .filter(|g| g.advance > 0)
        .map(|g| g.advance)
        .collect::<Vec<_>>();
    let avg = if advs.is_empty() {
        0
    } else {
        (2 * advs.iter().sum::<i64>() + advs.len() as i64) / (2 * advs.len() as i64)
    };
    let xh = ip("x_height").filter(|v| *v > 0);
    let cap = ip("cap_height");
    let fs_sel = i64::from(italic)
        | if bold { 0x20 } else { 0 }
        | if !bold && !italic { 0x40 } else { 0 }
        | 0x80;
    let first = mapping.first().map(|v| v.0.min(65535)).unwrap_or(0) as i64;
    let last = mapping.last().map(|v| v.0.min(65535)).unwrap_or(0) as i64;
    let sub = upem * 65 / 100;
    let mut os2 = pack(
        "HhHHHhhhhhhhhhhh",
        &[
            4,
            avg,
            if bold { 700 } else { 400 },
            5,
            0,
            sub,
            sub,
            0,
            upem * 14 / 100,
            sub,
            sub,
            0,
            upem * 48 / 100,
            ppu,
            xh.map(|v| (v * ppu).div_euclid(2))
                .unwrap_or((ascent * ppu).div_euclid(3)),
            0,
        ],
    )?;
    os2.extend(vec![0; 26]);
    os2.extend(b"BDFP");
    os2.extend(pack(
        "HHHhhhHHIIhhHHH",
        &[
            fs_sel,
            first,
            last,
            hasc,
            hdesc,
            0,
            hasc.max(ymax),
            (-hdesc).max(-ymin).max(0),
            1,
            0,
            xh.map(|v| v * ppu).unwrap_or(0),
            cap.unwrap_or(ascent) * ppu,
            0,
            32,
            0,
        ],
    )?);
    tables.insert("OS/2", os2);
    tables.insert("cmap", cmap(&mapping)?);
    let (mut glyf, mut loca, mut hmtx) = (Vec::new(), pack("I", &[0])?, Vec::new());
    for g in &glyphs {
        glyf.extend(encode_glyf(g)?);
        loca.extend(pack("I", &[glyf.len() as i64])?);
        hmtx.extend(pack(
            "Hh",
            &[g.advance, g.bounds.map(|b| b[0]).unwrap_or(0)],
        )?);
    }
    tables.insert("glyf", glyf);
    tables.insert("loca", loca);
    tables.insert("hmtx", hmtx);
    let present = |v: Option<&V>| {
        v.filter(|v| !v.is_null())
            .map(text)
            .filter(|s| !s.is_empty())
    };
    let family = present(p.get("family_name"))
        .or_else(|| present(h.get("fontname")))
        .unwrap_or("Untitled".into());
    let subfamily = match (bold, italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        _ => "Bold Italic",
    };
    let full = if subfamily == "Regular" {
        family.clone()
    } else {
        format!("{family} {subfamily}")
    };
    let ps = full
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(63)
        .collect::<String>();
    let ps = if ps.is_empty() { "Untitled".into() } else { ps };
    let version = sp("font_version")
        .filter(|s| !s.is_empty())
        .map(|s| format!("Version {s}"))
        .unwrap_or("Version 1.000".into());
    let mut records = Vec::new();
    if let Some(s) = present(p.get("copyright")) {
        records.push((0, s));
    }
    records.extend([
        (1, family),
        (2, subfamily.into()),
        (3, ps.clone()),
        (4, full),
        (5, version),
        (6, ps),
    ]);
    if let Some(s) = present(p.get("notice")) {
        records.push((10, s));
    }
    tables.insert("name", name_table(&records)?);
    let fixed =
        i64::from(sp("spacing").is_some_and(|s| matches!(s.to_uppercase().as_str(), "M" | "C")));
    let mut post = pack(
        "IIhhIIIIIH",
        &[
            0x20000,
            0,
            -ip("underline_position").unwrap_or(1) * ppu,
            ip("underline_thickness").unwrap_or(1) * ppu,
            fixed,
            0,
            0,
            0,
            0,
            n,
        ],
    )?;
    post.extend(pack("H", &[0])?);
    for i in 0..n - 1 {
        post.extend(pack("H", &[258 + i])?);
    }
    for g in &glyphs[1..] {
        post.push(g.name.len() as u8);
        post.extend(g.name.as_bytes());
    }
    tables.insert("post", post);
    tables.insert("prep", vec![0xb8, 1, 0xff, 0x85, 0xb0, 4, 0x8d]);
    if o.metadata {
        let mut h = h.clone();
        if let Some(v) = h.get_mut("bdfversion") {
            let n = v.as_f64().unwrap_or(2.1);
            *v = json!(if n.fract() == 0.0 {
                format!("{n:.1}")
            } else {
                super::svg::fmtnum(n)
            });
        }
        let p = p
            .into_iter()
            .filter(|(k, _)| k != "comment")
            .collect::<serde_json::Map<_, _>>();
        let pn = props(&core).1;
        let mut entries = Vec::new();
        for (_, g) in core.glyphs().filter(|(_, g)| !g.codepoints.is_empty()) {
            let m = meta(g);
            let cp = num(&m[1]);
            let mut e = vec![json!(gids[&cp]), m[1].clone(), m[0].clone()];
            e.extend_from_slice(&m[2..16]);
            e.push(V::Null);
            entries.push(e);
        }
        for (i, (_, m, alt)) in order.iter().enumerate().skip(nencoded) {
            let mut e = vec![json!(i + 1), json!(-1), m[0].clone()];
            e.extend_from_slice(&m[2..16]);
            e.push(alt.clone());
            entries.push(e);
        }
        let data = format!(
            "{{\"version\":1,\"ppu\":{ppu},\"shape\":{},\"dotsize\":{},\"headers\":{},\"props\":{},\"propnames\":{},\"glyphs\":{}}}",
            json!(o.shape),
            super::svg::fmtnum(o.dotsize),
            V::Object(h),
            V::Object(p),
            V::Object(pn),
            json!(entries)
        );
        tables.insert("BDFP", data.into_bytes());
    }
    assemble(tables)
}
impl Font {
    /// Deterministic SPEC TrueType output.
    pub fn tottf(&self, options: &TtfOptions) -> Result<Vec<u8>> {
        write_ttf(self, options)
    }
}
#[derive(Clone, Copy)]
struct Reader<'a> {
    b: &'a [u8],
}
impl<'a> Reader<'a> {
    fn bytes(self, o: usize, n: usize) -> Result<&'a [u8]> {
        self.b
            .get(
                o..o.checked_add(n)
                    .ok_or_else(|| invalid("TrueType offset overflow"))?,
            )
            .ok_or_else(|| invalid("Malformed TrueType font: table is truncated"))
    }
    fn u8(self, o: usize) -> Result<u8> {
        Ok(self.bytes(o, 1)?[0])
    }
    fn u16(self, o: usize) -> Result<u16> {
        let b = self.bytes(o, 2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn i16(self, o: usize) -> Result<i16> {
        Ok(self.u16(o)? as i16)
    }
    fn u32(self, o: usize) -> Result<u32> {
        let b = self.bytes(o, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}
fn parse_tables(data: &[u8]) -> Result<BTreeMap<String, &[u8]>> {
    let r = Reader { b: data };
    let tag = r.bytes(0, 4)?;
    if tag == b"OTTO" {
        return Err(invalid("CFF-based OpenType is not supported"));
    }
    if tag != b"\0\x01\0\0" && tag != b"true" {
        return Err(invalid("Not a TrueType font"));
    }
    let n = r.u16(4)? as usize;
    let mut out = BTreeMap::new();
    for i in 0..n {
        let off = 12 + 16 * i;
        let tag = String::from_utf8_lossy(r.bytes(off, 4)?).into_owned();
        let start = r.u32(off + 8)? as usize;
        let len = r.u32(off + 12)? as usize;
        out.insert(tag, r.bytes(start, len)?);
    }
    for key in ["head", "maxp", "hhea", "hmtx", "loca", "glyf"] {
        if !out.contains_key(key) {
            return Err(invalid("Malformed TrueType font: required table missing"));
        }
    }
    Ok(out)
}
struct Glyf<'a> {
    r: Reader<'a>,
    loca: Vec<usize>,
}
impl Glyf<'_> {
    fn contours(&self, gid: usize, depth: usize, budget: &mut ReadBudget) -> Result<Vec<Contour>> {
        let mut work = 0usize;
        let contours = self.contours_inner(gid, depth, &mut work)?;
        budget.work = budget.work.saturating_add(work);
        if budget.work > 32 * 1024 * 1024 {
            return Err(invalid("TrueType cumulative expansion budget exceeded"));
        }
        Ok(contours)
    }
    fn contours_inner(&self, gid: usize, depth: usize, work: &mut usize) -> Result<Vec<Contour>> {
        *work += 1;
        if *work > 1_048_576 {
            return Err(invalid("composite expansion work budget exceeded"));
        }
        if depth > 32 {
            return Err(invalid("composite glyph nesting too deep"));
        }
        let start = *self
            .loca
            .get(gid)
            .ok_or_else(|| invalid("glyph index out of range"))?;
        let end = *self
            .loca
            .get(gid + 1)
            .ok_or_else(|| invalid("glyph index out of range"))?;
        if end < start {
            return Err(invalid("descending loca offsets"));
        }
        if end == start {
            return Ok(vec![]);
        }
        let r = Reader {
            b: self.r.bytes(start, end - start)?,
        };
        r.bytes(0, 10)?;
        let nc = r.i16(0)?;
        if nc >= 0 {
            self.simple(r, 0, nc as usize)
        } else {
            self.composite(r, 0, depth, work)
        }
    }
    fn simple(&self, r: Reader<'_>, off: usize, nc: usize) -> Result<Vec<Contour>> {
        let mut p = off + 10;
        let ends = (0..nc)
            .map(|i| r.u16(p + 2 * i).map(usize::from))
            .collect::<Result<Vec<_>>>()?;
        if ends.windows(2).any(|w| w[0] >= w[1]) {
            return Err(invalid("bad endPtsOfContours"));
        }
        p += 2 * nc;
        let n = ends.last().map(|v| v + 1).unwrap_or(0);
        p += 2 + r.u16(p)? as usize;
        let mut flags = Vec::new();
        while flags.len() < n {
            let f = r.u8(p)?;
            p += 1;
            flags.push(f);
            if f & 8 != 0 {
                let repeat = r.u8(p)? as usize;
                p += 1;
                if repeat > n - flags.len() {
                    return Err(invalid("flag run exceeds point count"));
                }
                flags.extend(std::iter::repeat_n(f, repeat));
            }
        }
        flags.truncate(n);
        let mut coords = Vec::new();
        for (short, same) in [(2, 0x10), (4, 0x20)] {
            let mut v = 0i64;
            let mut axis = Vec::new();
            for f in &flags {
                if f & short != 0 {
                    let d = r.u8(p)? as i64;
                    p += 1;
                    v += if f & same != 0 { d } else { -d };
                } else if f & same == 0 {
                    v += r.i16(p)? as i64;
                    p += 2;
                }
                axis.push(v as f64);
            }
            coords.push(axis);
        }
        r.bytes(p, 0)?;
        let mut start = 0;
        let mut out = Vec::new();
        for end in ends {
            if end + 1 < start || end >= n {
                return Err(invalid("bad endPtsOfContours"));
            }
            out.push(
                (start..=end)
                    .map(|i| (coords[0][i], coords[1][i], flags[i] & 1 != 0))
                    .collect(),
            );
            start = end + 1;
        }
        Ok(out)
    }
    fn composite(
        &self,
        r: Reader<'_>,
        off: usize,
        depth: usize,
        work: &mut usize,
    ) -> Result<Vec<Contour>> {
        let mut p = off + 10;
        let mut result = Vec::<Contour>::new();
        let mut components = 0;
        let mut point_count = 0usize;
        loop {
            components += 1;
            if components > 65536 {
                return Err(invalid("too many composite components"));
            }
            let flags = r.u16(p)?;
            let gid = r.u16(p + 2)? as usize;
            p += 4;
            let (a1, a2) = if flags & 1 != 0 {
                let a = if flags & 2 != 0 {
                    (r.i16(p)? as i64, r.i16(p + 2)? as i64)
                } else {
                    (r.u16(p)? as i64, r.u16(p + 2)? as i64)
                };
                p += 4;
                a
            } else {
                let a = if flags & 2 != 0 {
                    (r.u8(p)? as i8 as i64, r.u8(p + 1)? as i8 as i64)
                } else {
                    (r.u8(p)? as i64, r.u8(p + 1)? as i64)
                };
                p += 2;
                a
            };
            let (mut xx, mut xy, mut yx, mut yy) = (1.0, 0.0, 0.0, 1.0);
            if flags & 8 != 0 {
                xx = r.i16(p)? as f64 / 16384.0;
                yy = xx;
                p += 2;
            } else if flags & 0x40 != 0 {
                xx = r.i16(p)? as f64 / 16384.0;
                yy = r.i16(p + 2)? as f64 / 16384.0;
                p += 4;
            } else if flags & 0x80 != 0 {
                xx = r.i16(p)? as f64 / 16384.0;
                xy = r.i16(p + 2)? as f64 / 16384.0;
                yx = r.i16(p + 4)? as f64 / 16384.0;
                yy = r.i16(p + 6)? as f64 / 16384.0;
                p += 8;
            }
            let mut tr = self.contours_inner(gid, depth + 1, work)?;
            for c in &mut tr {
                for (x, y, _) in c {
                    let (nx, ny) = (*x * xx + *y * yx, *x * xy + *y * yy);
                    *x = nx;
                    *y = ny;
                }
            }
            let (dx, dy) = if flags & 2 != 0 {
                (a1 as f64, a2 as f64)
            } else {
                let parent = result
                    .iter()
                    .flatten()
                    .nth(a1 as usize)
                    .ok_or_else(|| invalid("bad composite point index"))?;
                let child = tr
                    .iter()
                    .flatten()
                    .nth(a2 as usize)
                    .ok_or_else(|| invalid("bad composite point index"))?;
                (parent.0 - child.0, parent.1 - child.1)
            };
            for c in &mut tr {
                for (x, y, _) in c {
                    *x += dx;
                    *y += dy;
                }
            }
            point_count = point_count.saturating_add(tr.iter().map(Vec::len).sum::<usize>());
            if point_count > 1 << 20 {
                return Err(invalid("composite point budget exceeded"));
            }
            result.extend(tr);
            if flags & 0x20 == 0 {
                break;
            }
        }
        Ok(result)
    }
}
#[derive(Default)]
struct ReadBudget {
    pixels: u64,
    bytes: u64,
    points: usize,
    work: usize,
}
impl ReadBudget {
    fn charge(&mut self, cs: &[Contour], w: i64, h: i64) -> Result<()> {
        self.points = self
            .points
            .saturating_add(cs.iter().map(Vec::len).sum::<usize>());
        let (w, h) = (w.max(0) as u64, h.max(0) as u64);
        self.pixels = self.pixels.saturating_add(w.saturating_mul(h));
        // Gray raster, packed/hex provenance and per-row string/vector overhead.
        self.bytes = self.bytes.saturating_add(
            w.saturating_add(w.div_ceil(4))
                .saturating_add(64)
                .saturating_mul(h)
                .saturating_add(512),
        );
        if self.pixels > 64 * 1024 * 1024
            || self.bytes > 256 * 1024 * 1024
            || self.points > 64 * 1024 * 1024
        {
            return Err(invalid("TrueType cumulative model budget exceeded"));
        }
        Ok(())
    }
}
fn raster_meta(
    cs: &[Contour],
    ppu: f64,
    box_: Option<[i64; 4]>,
    budget: &mut ReadBudget,
) -> Result<Meta> {
    let mut m = vec![V::Null; 17];
    let explicit = box_.is_some();
    let b = if let Some(b) = box_ {
        b
    } else if cs.iter().all(Vec::is_empty) {
        [0, 0, 0, 0]
    } else {
        let pts = cs.iter().flatten().collect::<Vec<_>>();
        let x0 = (pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) / ppu).floor() as i64;
        let y0 = (pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) / ppu).floor() as i64;
        let x1 = (pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max) / ppu).ceil() as i64;
        let y1 = (pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) / ppu).ceil() as i64;
        [x1 - x0, y1 - y0, x0, y0]
    };
    let (w, h, mut x, mut y) = (b[0], b[1], b[2], b[3]);
    budget.charge(cs, w, h)?;
    if w.max(1) as u64 * h.max(1) as u64 > 1 << 24 {
        return Err(invalid(
            "Malformed TrueType font: glyph bounding box too large",
        ));
    }
    let mut bm = rasterize(cs, ppu, x, y, w.max(0), h.max(0))?;
    if !explicit {
        let bb = bm.bbox();
        if bb.width == 0 || bb.height == 0 {
            x = 0;
            y = 0;
        } else {
            x += bb.x;
            y += bb.y;
        }
        bm.trim()?;
    }
    m[2] = json!(if explicit { w } else { bm.width() as i64 });
    m[3] = json!(if explicit { h } else { bm.height() as i64 });
    m[4] = json!(x);
    m[5] = json!(y);
    m[16] = json!(bm.raster.to_mono(1)?.to_hex_rows()?);
    Ok(m)
}
fn parse_cmap(data: &[u8]) -> Result<BTreeMap<u32, usize>> {
    let r = Reader { b: data };
    let n = r.u16(2)? as usize;
    let mut recs = Vec::new();
    for i in 0..n {
        let p = 4 + 8 * i;
        recs.push((r.u16(p)?, r.u16(p + 2)?, r.u32(p + 4)? as usize));
    }
    let mut order = Vec::new();
    for pref in [(3, 10), (0, 4), (0, 6), (3, 1), (0, 3)] {
        order.extend(recs.iter().copied().filter(|x| (x.0, x.1) == pref));
    }
    for rec in recs {
        if !order.contains(&rec) {
            order.push(rec);
        }
    }
    let mut out = BTreeMap::new();
    let mut work = 0u64;
    for (_, _, off) in order {
        let fmt = r.u16(off)?;
        match fmt {
            0 => {
                for c in 0..256 {
                    let g = r.u8(off + 6 + c)? as usize;
                    if g > 0 {
                        out.insert(c as u32, g);
                    }
                }
            }
            6 => {
                let first = r.u16(off + 6)? as u32;
                let count = r.u16(off + 8)? as usize;
                for i in 0..count {
                    let g = r.u16(off + 10 + 2 * i)? as usize;
                    if g > 0 {
                        out.insert(first + i as u32, g);
                    }
                }
            }
            4 => {
                let segx2 = r.u16(off + 6)? as usize;
                let n = segx2 / 2;
                let rb = off + 16 + 3 * segx2;
                for i in 0..n {
                    let e = r.u16(off + 14 + 2 * i)? as u32;
                    let s = r.u16(off + 16 + segx2 + 2 * i)? as u32;
                    let d = r.u16(off + 16 + 2 * segx2 + 2 * i)? as u32;
                    let ro = r.u16(rb + 2 * i)? as usize;
                    work += u64::from(e.saturating_sub(s)) + 1;
                    if work > 2_097_152 {
                        return Err(invalid("cmap expansion budget exceeded"));
                    }
                    for c in s..=e {
                        if c == 65535 {
                            continue;
                        }
                        let g = if ro == 0 {
                            (c + d) & 65535
                        } else {
                            let g = r.u16(rb + 2 * i + ro + 2 * (c - s) as usize)? as u32;
                            if g == 0 { 0 } else { (g + d) & 65535 }
                        };
                        if g > 0 {
                            out.insert(c, g as usize);
                        }
                    }
                }
            }
            12 => {
                let n = r.u32(off + 12)? as usize;
                r.bytes(off + 16, 12 * n)?;
                for i in 0..n {
                    let p = off + 16 + 12 * i;
                    let s = r.u32(p)?;
                    let e = r.u32(p + 4)?;
                    let sg = r.u32(p + 8)?;
                    if e < s || e > 0x10ffff {
                        return Err(invalid("bad cmap format 12 group"));
                    }
                    work += u64::from(e.saturating_sub(s)) + 1;
                    if work > 2_097_152 {
                        return Err(invalid("cmap expansion budget exceeded"));
                    }
                    for c in s..=e {
                        let g = sg as u64 + c as u64 - s as u64;
                        if g > 0 {
                            out.insert(c, g as usize);
                        }
                    }
                }
            }
            _ => continue,
        }
        break;
    }
    Ok(out)
}
fn parse_name(data: &[u8]) -> Result<BTreeMap<u16, String>> {
    let r = Reader { b: data };
    let n = r.u16(2)? as usize;
    let soff = r.u16(4)? as usize;
    let mut found = BTreeMap::<u16, (i32, String)>::new();
    for i in 0..n {
        let p = 6 + 12 * i;
        let (pid, eid, lid, nid) = (r.u16(p)?, r.u16(p + 2)?, r.u16(p + 4)?, r.u16(p + 6)?);
        let len = r.u16(p + 8)? as usize;
        let off = r.u16(p + 10)? as usize;
        let priority = if pid == 3 && [0, 1, 10].contains(&eid) {
            if lid == 0x409 { 0 } else { 1 }
        } else if pid == 0 {
            2
        } else if pid == 1 && eid == 0 {
            3
        } else {
            continue;
        };
        let raw = r.bytes(soff + off, len)?;
        let s = if priority == 3 {
            let codec = crate::charset::CharsetCodec::new("macroman", None)
                .map_err(|e| invalid(&e.to_string()))?;
            raw.iter()
                .filter_map(|b| codec.cp2char(*b as u32))
                .collect()
        } else {
            if raw.len() % 2 != 0 {
                continue;
            }
            let units = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect::<Vec<_>>();
            let Ok(s) = String::from_utf16(&units) else {
                continue;
            };
            s
        };
        if found.get(&nid).is_none_or(|v| priority < v.0) {
            found.insert(nid, (priority, s));
        }
    }
    Ok(found.into_iter().map(|(k, v)| (k, v.1)).collect())
}
fn parse_post(data: &[u8], numglyphs: usize) -> Result<Vec<String>> {
    let r = Reader { b: data };
    let mac = MAC_NAMES.split_whitespace().collect::<Vec<_>>();
    let ver = r.u32(0)?;
    let mut out = vec![String::new(); numglyphs];
    if ver == 0x10000 {
        for (i, n) in mac.iter().take(numglyphs).enumerate() {
            out[i] = (*n).into();
        }
        return Ok(out);
    }
    if ver != 0x20000 {
        return Ok(out);
    }
    let n = r.u16(32)? as usize;
    let mut p = 34 + 2 * n;
    let mut custom = Vec::new();
    while p < data.len() {
        let len = r.u8(p)? as usize;
        custom.push(
            data[p + 1..(p + 1 + len).min(data.len())]
                .iter()
                .map(|b| char::from(*b))
                .collect::<String>(),
        );
        p += 1 + len;
    }
    for (i, s) in out.iter_mut().take(n).enumerate() {
        let k = r.u16(34 + 2 * i)? as usize;
        if k < 258 {
            *s = mac.get(k).copied().unwrap_or("").into();
        } else {
            *s = custom.get(k - 258).cloned().unwrap_or_default();
        }
    }
    Ok(out)
}
/// Read a glyf-based TrueType font, requiring pixelsize unless BDFP is present.
pub fn read_ttf(data: &[u8], pixelsize: Option<i64>) -> Result<Font> {
    if data.len() > 256 * 1024 * 1024 {
        return Err(invalid("TrueType input exceeds byte budget"));
    }
    let tables = parse_tables(data)?;
    let mut budget = ReadBudget::default();
    let head = Reader { b: tables["head"] };
    let upem = head.u16(18)? as i64;
    if upem == 0 {
        return Err(invalid("zero unitsPerEm"));
    }
    let lf = head.i16(50)?;
    let n = Reader { b: tables["maxp"] }.u16(4)? as usize;
    let hh = Reader { b: tables["hhea"] };
    let (asc, desc) = (hh.i16(4)? as i64, hh.i16(6)? as i64);
    let nhm = hh.u16(34)? as usize;
    if nhm == 0 || nhm > n {
        return Err(invalid("bad hhea.numberOfHMetrics"));
    }
    let hm = Reader { b: tables["hmtx"] };
    let mut advances = (0..nhm)
        .map(|i| hm.u16(4 * i).map(i64::from))
        .collect::<Result<Vec<_>>>()?;
    advances.resize(n, *advances.last().ok_or_else(|| invalid("no advance"))?);
    let lr = Reader { b: tables["loca"] };
    let loca = (0..=n)
        .map(|i| match lf {
            0 => lr.u16(i * 2).map(|v| v as usize * 2),
            1 => lr.u32(i * 4).map(|v| v as usize),
            _ => Err(invalid("bad indexToLocFormat")),
        })
        .collect::<Result<Vec<_>>>()?;
    let gr = Glyf {
        r: Reader { b: tables["glyf"] },
        loca,
    };
    if let Some(raw) = tables.get("BDFP") {
        let obj: V = serde_json::from_slice(raw)
            .map_err(|e| invalid(&format!("Malformed BDFP table: {e}")))?;
        let ppu = obj["ppu"]
            .as_f64()
            .filter(|v| *v > 0.0 && v.is_finite())
            .ok_or_else(|| invalid("Malformed BDFP ppu"))?;
        let mut h = obj["headers"]
            .as_object()
            .cloned()
            .ok_or_else(|| invalid("Malformed BDFP headers"))?;
        if let Some(v) = h.get_mut("bdfversion") {
            *v = json!(
                v.as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .or_else(|| v.as_f64())
                    .unwrap_or(2.1)
            );
        }
        let p = obj["props"]
            .as_object()
            .ok_or_else(|| invalid("Malformed BDFP props"))?;
        let names = obj["propnames"]
            .as_object()
            .ok_or_else(|| invalid("Malformed BDFP names"))?;
        let mut f = Font::from_parts(&h, p, names)?;
        let entries = obj["glyphs"]
            .as_array()
            .ok_or_else(|| invalid("Malformed BDFP glyphs"))?;
        for e in entries {
            let e = e
                .as_array()
                .filter(|e| e.len() == 3 || e.len() == 18)
                .ok_or_else(|| invalid("Malformed BDFP glyph entry"))?;
            let gid = e[0]
                .as_u64()
                .filter(|v| *v > 0 && *v < n as u64)
                .ok_or_else(|| invalid("Malformed BDFP gid"))? as usize;
            let cp = e[1]
                .as_i64()
                .ok_or_else(|| invalid("Malformed BDFP encoding"))?;
            let cs = gr.contours(gid, 0, &mut budget)?;
            let (mut m, alt) = if e.len() == 18 {
                let mut m = raster_meta(
                    &cs,
                    ppu,
                    Some([num(&e[3]), num(&e[4]), num(&e[5]), num(&e[6])]),
                    &mut budget,
                )?;
                m[6..16].clone_from_slice(&e[7..17]);
                (m, e[17].clone())
            } else {
                let mut m = raster_meta(&cs, ppu, None, &mut budget)?;
                let dw = (advances[gid] as f64 / ppu + 0.5).floor() as i64;
                m[6] = json!(f.swidth(dw));
                m[7] = json!(0);
                m[8] = json!(dw);
                m[9] = json!(0);
                (m, V::Null)
            };
            m[0] = e[2].clone();
            m[1] = json!(cp);
            f.put_with_alt(m, alt)?;
        }
        return Ok(f);
    }
    let px = pixelsize
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("pixelsize is required to read TrueType without BDFP metadata"))?;
    let ppu = upem as f64 / px as f64;
    if ppu <= 0.0 {
        return Err(invalid("bad unitsPerEm"));
    }
    let names = tables
        .get("name")
        .map(|b| parse_name(b))
        .transpose()?
        .unwrap_or_default();
    let cm = tables
        .get("cmap")
        .map(|b| parse_cmap(b))
        .transpose()?
        .unwrap_or_default();
    let post = tables
        .get("post")
        .map(|b| parse_post(b, n))
        .transpose()?
        .unwrap_or(vec![String::new(); n]);
    let mut cps = BTreeMap::new();
    for (c, g) in cm {
        if g > 0 && g < n {
            cps.entry(g).or_insert(c);
        }
    }
    let name = names
        .get(&6)
        .filter(|s| !s.is_empty())
        .or_else(|| names.get(&4).filter(|s| !s.is_empty()))
        .cloned()
        .unwrap_or("Untitled".into());
    let ascent = (asc as f64 / ppu + 0.5).floor() as i64;
    let descent = (-desc as f64 / ppu + 0.5).floor() as i64;
    let mut f = Font::create(&super::CreateOptions {
        pixelsize: px,
        ascent: Some(ascent),
        descent: Some(descent),
        fontname: Some(name.clone()),
        familyname: names
            .get(&1)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or(name),
        ..Default::default()
    })?;
    if let Some(c) = names.get(&0).filter(|s| !s.is_empty()) {
        let mut p = f.props();
        let mut new = serde_json::Map::new();
        if let Some(v) = p.shift_remove("family_name") {
            new.insert("family_name".into(), v);
        }
        new.insert("copyright".into(), json!(c));
        new.extend(p);
        let names = new
            .keys()
            .map(|k| (k.clone(), json!(k.to_uppercase())))
            .collect();
        f = Font::from_parts(&f.headers(), &new, &names)?;
    }
    for gid in 1..n {
        let cp = cps.get(&gid).copied();
        let name = if !post[gid].is_empty() {
            post[gid].clone()
        } else if let Some(cp) = cp {
            defaultglyphname(cp as i64, true)
        } else {
            format!("glyph{gid}")
        };
        let mut m = raster_meta(&gr.contours(gid, 0, &mut budget)?, ppu, None, &mut budget)?;
        let dw = (advances[gid] as f64 / ppu + 0.5).floor() as i64;
        m[0] = json!(name);
        m[1] = json!(cp.map(i64::from).unwrap_or(-1));
        m[6] = json!(f.swidth(dw));
        m[7] = json!(0);
        m[8] = json!(dw);
        m[9] = json!(0);
        f.put_with_alt(m, V::Null)?;
    }
    f.updatebbox()?;
    Ok(f)
}

const MAC_NAMES: &str = ".notdef .null nonmarkingreturn space exclam quotedbl numbersign dollar percent ampersand quotesingle parenleft parenright asterisk plus comma hyphen period slash zero one two three four five six seven eight nine colon semicolon less equal greater question at A B C D E F G H I J K L M N O P Q R S T U V W X Y Z bracketleft backslash bracketright asciicircum underscore grave a b c d e f g h i j k l m n o p q r s t u v w x y z braceleft bar braceright asciitilde Adieresis Aring Ccedilla Eacute Ntilde Odieresis Udieresis aacute agrave acircumflex adieresis atilde aring ccedilla eacute egrave ecircumflex edieresis iacute igrave icircumflex idieresis ntilde oacute ograve ocircumflex odieresis otilde uacute ugrave ucircumflex udieresis dagger degree cent sterling section bullet paragraph germandbls registered copyright trademark acute dieresis notequal AE Oslash infinity plusminus lessequal greaterequal yen mu partialdiff summation product pi integral ordfeminine ordmasculine Omega ae oslash questiondown exclamdown logicalnot radical florin approxequal Delta guillemotleft guillemotright ellipsis nonbreakingspace Agrave Atilde Otilde OE oe endash emdash quotedblleft quotedblright quoteleft quoteright divide lozenge ydieresis Ydieresis fraction currency guilsinglleft guilsinglright fi fl daggerdbl periodcentered quotesinglbase quotedblbase perthousand Acircumflex Ecircumflex Aacute Edieresis Egrave Iacute Icircumflex Idieresis Igrave Oacute Ocircumflex apple Ograve Uacute Ucircumflex Ugrave dotlessi circumflex tilde macron breve dotaccent ring cedilla hungarumlaut ogonek caron Lslash lslash Scaron scaron Zcaron zcaron brokenbar Eth eth Yacute yacute Thorn thorn minus multiply onesuperior twosuperior threesuperior onehalf onequarter threequarters franc Gbreve gbreve Idotaccent Scedilla scedilla Cacute cacute Ccaron ccaron dcroat";
