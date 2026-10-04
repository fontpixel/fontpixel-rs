//! Native FNT text, Caps authoring metadata, and caller-controlled PNG sidecars.
use super::*;
use crate::bitmap::{Bitmap, Bounds, Gray8};
use crate::model::{Codepoint, Glyph, Vector};
use crate::resources::OutputAsset;
use std::fmt::Write as _;
struct Source<'a> {
    width: Option<u32>,
    height: Option<u32>,
    tracking: i16,
    data: Option<&'a str>,
    data_len: Option<usize>,
    name: Option<&'a str>,
    metrics: Option<&'a str>,
    comments: Vec<&'a str>,
    glyphs: Vec<(u32, u8)>,
    kern: BTreeMap<(u32, u32), i8>,
}
fn token(mut s: &str) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    while !s.is_empty() {
        let (cp, tail) = if let Some(t) = s.strip_prefix("space") {
            (32, t)
        } else if let Some(t) = s.strip_prefix("U+") {
            let n = t.bytes().take_while(u8::is_ascii_hexdigit).count();
            if n == 0 || n > 6 {
                return Err(invalid("invalid FNT Unicode token"));
            }
            (
                u32::from_str_radix(&t[..n], 16)
                    .map_err(|_| invalid("invalid FNT Unicode token"))?,
                &t[n..],
            )
        } else {
            let c = s.chars().next().unwrap();
            (u32::from(c), &s[c.len_utf8()..])
        };
        if cp > 0x3ffff || char::from_u32(cp).is_none() {
            return Err(invalid("FNT codepoint is outside the native range"));
        }
        out.push(cp);
        if out.len() > 2 {
            return Err(invalid("FNT token contains more than two characters"));
        }
        s = tail;
    }
    Ok(out)
}
fn source<'a>(b: &'a [u8], l: &ResourceLimits, budget: &mut Budget) -> Result<Source<'a>> {
    check(b.len(), l.max_table_bytes, "FNT source bytes")?;
    let text = std::str::from_utf8(b).map_err(|_| invalid("FNT is not UTF-8"))?;
    let mut s = Source {
        width: None,
        height: None,
        tracking: 0,
        data: None,
        data_len: None,
        name: None,
        metrics: None,
        comments: Vec::new(),
        glyphs: Vec::new(),
        kern: BTreeMap::new(),
    };
    let mut seen = std::collections::BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("--name=") {
            if s.name.replace(value).is_some() {
                return Err(invalid("duplicate FNT name"));
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("--metrics=") {
            if s.metrics.replace(value).is_some() {
                return Err(invalid("duplicate FNT metrics"));
            }
            budget.add(value.len().saturating_mul(12))?;
            let json: serde_json::Value =
                serde_json::from_str(value).map_err(|_| invalid("invalid FNT metrics JSON"))?;
            if !json.is_object() {
                return Err(invalid("FNT metrics must be an object"));
            }
            continue;
        }
        if line.starts_with("--") {
            budget.add(line.len() + 64)?;
            s.comments.push(line);
            continue;
        }
        if let Some((key, value)) = line.split_once('=').filter(|(key, _)| {
            matches!(
                key.trim(),
                "width" | "height" | "tracking" | "datalen" | "data"
            )
        }) {
            match key.trim() {
                "width" => {
                    s.width = Some(
                        value
                            .trim()
                            .parse()
                            .map_err(|_| invalid("invalid FNT width"))?,
                    )
                }
                "height" => {
                    s.height = Some(
                        value
                            .trim()
                            .parse()
                            .map_err(|_| invalid("invalid FNT height"))?,
                    )
                }
                "tracking" => {
                    s.tracking = value
                        .trim()
                        .parse()
                        .map_err(|_| invalid("invalid FNT tracking"))?
                }
                "datalen" => {
                    s.data_len = Some(
                        value
                            .trim()
                            .parse()
                            .map_err(|_| invalid("invalid FNT data length"))?,
                    )
                }
                "data" => {
                    if s.data.replace(value.trim()).is_some() {
                        return Err(invalid("duplicate FNT data"));
                    }
                }
                _ => return Err(invalid("unsupported FNT property")),
            };
            continue;
        }
        let mut fields = line.split_whitespace();
        let key = fields.next().ok_or_else(|| invalid("missing FNT token"))?;
        let val = fields
            .next()
            .ok_or_else(|| invalid("missing FNT width/kerning"))?;
        if fields.next().is_some() {
            return Err(invalid("trailing FNT fields"));
        }
        let cps = token(key)?;
        match cps.as_slice() {
            [cp] => {
                check(s.glyphs.len() + 1, l.max_glyphs, "FNT glyphs")?;
                budget.add(192)?;
                if !seen.insert(*cp) {
                    return Err(invalid("duplicate FNT mapping"));
                }
                s.glyphs.push((
                    *cp,
                    val.parse()
                        .map_err(|_| invalid("FNT glyph width exceeds byte"))?,
                ));
            }
            [a, b] => {
                budget.add(128)?;
                if s.kern
                    .insert(
                        (*a, *b),
                        val.parse().map_err(|_| invalid("FNT kerning exceeds i8"))?,
                    )
                    .is_some()
                {
                    return Err(invalid("duplicate FNT kerning pair"));
                }
            }
            _ => return Err(invalid("empty FNT token")),
        }
    }
    if s.glyphs.is_empty() {
        return Err(invalid("FNT contains no glyphs"));
    }
    if s.data_len
        .is_some_and(|n| s.data.is_none_or(|d| d.len() != n))
    {
        return Err(invalid("FNT datalen mismatch"));
    }
    Ok(s)
}
fn atlas_dimensions(path: &str) -> Option<(u32, u32)> {
    let (_, tail) = path.rsplit_once("-table-")?;
    let (w, h) = tail.strip_suffix(".png")?.split_once('-')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}
fn atlas_name(name: &str, w: u32, h: u32) -> Result<ResourcePath> {
    let stem = name.strip_suffix(".fnt").unwrap_or(name);
    ResourcePath::new(format!("{stem}-table-{w}-{h}.png"))
}
pub(super) fn read(
    c: &mut ReadContext<'_>,
    v: &PlaydateReadOptions,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    // Own the small text view so a caller-authorized sidecar can be resolved mutably.
    budget.add(c.bytes().len())?;
    let bytes = c.bytes().to_vec();
    let mut s = source(&bytes, c.limits(), budget)?;
    s.width = v.cell_width.or(s.width);
    s.height = v.cell_height.or(s.height);
    let png = if let Some(data) = s.data {
        decode64(data, c.limits(), budget)?
    } else {
        let path = if let Some(path) = &v.atlas {
            path.clone()
        } else if let (Some(w), Some(h), Some(name)) = (s.width, s.height, c.name()) {
            atlas_name(name, w, h)?
        } else {
            let name = c
                .name()
                .ok_or_else(|| invalid("FNT sidecar requires a name or explicit atlas path"))?
                .to_owned();
            let stem = name.strip_suffix(".fnt").unwrap_or(&name);
            let prefix = format!("{stem}-table-");
            let parent = name
                .rsplit_once('/')
                .map(|(p, _)| ResourcePath::new(p))
                .transpose()?;
            let list = c.list_resources_limited(
                parent.as_ref(),
                c.limits().max_resources,
                c.limits().max_table_bytes,
            )?;
            budget.add(list.iter().map(|p| p.as_str().len() + 128).sum())?;
            let mut found = list.into_iter().filter(|p| {
                p.as_str().starts_with(&prefix) && atlas_dimensions(p.as_str()).is_some()
            });
            let path = found
                .next()
                .ok_or_else(|| invalid("FNT PNG atlas was not found"))?;
            if found.next().is_some() {
                return Err(invalid(
                    "FNT PNG atlas is ambiguous; supply an explicit path",
                ));
            }
            path
        };
        if let Some((w, h)) = atlas_dimensions(path.as_str()) {
            s.width = s.width.or(Some(w));
            s.height = s.height.or(Some(h));
        }
        let png = c.resource(&path)?;
        budget.add(png.len())?;
        png
    };
    decode(&s, &png, c.limits(), budget)
}
fn decode(
    s: &Source<'_>,
    png: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let mut report = Report::default();
    let (w, h, pixels) = super::png::decode(png, l, budget, &mut report)?;
    let cw = s
        .width
        .ok_or_else(|| invalid("FNT cell width is missing"))?;
    let ch = s
        .height
        .ok_or_else(|| invalid("FNT cell height is missing"))?;
    if cw == 0 || ch == 0 || !w.is_multiple_of(cw) || !h.is_multiple_of(ch) {
        return Err(invalid("FNT atlas is not an integer cell grid"));
    }
    let cols = w / cw;
    let cells = u64::from(cols) * u64::from(h / ch);
    if cells < s.glyphs.len() as u64 {
        return Err(invalid("FNT atlas has too few cells"));
    }
    check(
        (cw as usize).saturating_mul(ch as usize),
        l.max_pixels_per_glyph,
        "FNT glyph pixels",
    )?;
    allocation(s.glyphs.len(), l, budget)?;
    let baseline = if let Some(metrics) = s.metrics {
        budget.add(metrics.len().saturating_mul(12))?;
        let json: serde_json::Value =
            serde_json::from_str(metrics).map_err(|_| invalid("invalid FNT metrics JSON"))?;
        json.get("baseline")
            .map(|v| {
                v.as_i64()
                    .ok_or_else(|| invalid("FNT baseline must be an integer"))
            })
            .transpose()?
    } else {
        None
    };
    let ascent = baseline.map_or(i64::from(ch), |n| if n > 0 { n } else { i64::from(ch) + n });
    i32::try_from(ascent).map_err(|_| invalid("FNT baseline exceeds i32"))?;
    let mut f = native_font(cw, ch, ascent, s.tracking, &s.kern);
    f.names.family = s.name.map(str::to_owned);
    if let Some(metrics) = s.metrics {
        f.properties
            .push(Property::new("playdate.metrics", metrics));
    }
    if !s.comments.is_empty() {
        f.properties
            .push(Property::new("playdate.comments", s.comments.join("\n")));
    }
    for (i, &(cp, width)) in s.glyphs.iter().enumerate() {
        let left = (i as u32 % cols) * cw;
        let top = (i as u32 / cols) * ch;
        let area = (cw as usize).saturating_mul(ch as usize);
        budget.add(area.saturating_mul(2))?;
        let mut samples = Vec::with_capacity(area);
        let mut binary = true;
        for y in 0..ch {
            let row = ((top + y) * w + left) as usize;
            for &p in &pixels[row..row + cw as usize] {
                binary &= p == 0 || p == 255;
                samples.push(p);
            }
        }
        let advance = i64::from(width) + i64::from(s.tracking);
        let mut g = if binary {
            let mut bits = raster(cw, ch, l, budget)?;
            let stride = (cw as usize).div_ceil(8);
            for y in 0..ch as usize {
                for x in 0..cw as usize {
                    if samples[y * cw as usize + x] != 0 {
                        bits[y * stride + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            glyph(cw, ch, 0, ascent - i64::from(ch), advance, &bits, l)?
        } else {
            let mut g = Glyph::new(Bitmap::<Gray8>::from_raw(
                cw,
                ch,
                &samples,
                bitmap_limits(l),
            )?);
            g.metrics.bounds = Bounds::new(cw, ch, 0, ascent - i64::from(ch));
            g.metrics.advance = Some(Vector::new(advance, 0));
            g.metrics.left_bearing = Some(0);
            g.metrics.right_bearing = Some(advance - i64::from(cw));
            g
        };
        g.codepoints.push(Codepoint::Unicode(cp));
        f.add_glyph(g)?;
    }
    Ok((set(f, "playdate"), report))
}
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn decode64(s: &str, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(4) {
        return Err(invalid("FNT base64 length is not divisible by four"));
    }
    let cap = s.len() / 4 * 3;
    check(
        cap,
        l.max_resource_bytes.min(l.max_table_bytes),
        "FNT embedded PNG bytes",
    )?;
    budget.add(cap)?;
    let mut out = Vec::with_capacity(cap);
    for (i, b) in s.as_bytes().as_chunks::<4>().0.iter().enumerate() {
        let mut v = [0u8; 4];
        let mut padding = 0;
        for j in 0..4 {
            if b[j] == b'=' {
                if j < 2 {
                    return Err(invalid("invalid FNT base64 padding"));
                }
                padding += 1;
            } else {
                if padding != 0 {
                    return Err(invalid("invalid FNT base64 padding"));
                }
                v[j] = match b[j] {
                    b'A'..=b'Z' => b[j] - b'A',
                    b'a'..=b'z' => b[j] - b'a' + 26,
                    b'0'..=b'9' => b[j] - b'0' + 52,
                    b'+' => 62,
                    b'/' => 63,
                    _ => return Err(invalid("invalid FNT base64 character")),
                };
            }
        }
        if padding != 0
            && (i + 1 != s.len() / 4
                || (padding == 2 && v[1] & 15 != 0)
                || (padding == 1 && v[2] & 3 != 0))
        {
            return Err(invalid("noncanonical FNT base64 padding"));
        }
        out.push((v[0] << 2) | (v[1] >> 4));
        if padding < 2 {
            out.push((v[1] << 4) | (v[2] >> 2));
        }
        if padding == 0 {
            out.push((v[2] << 6) | v[3]);
        }
    }
    Ok(out)
}
fn encode64(b: &[u8], out: &mut String) {
    for p in b.chunks(3) {
        let a = p[0];
        let c = p.get(1).copied().unwrap_or(0);
        let d = p.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(a >> 2) as usize] as char);
        out.push(ALPHABET[((a & 3) << 4 | c >> 4) as usize] as char);
        out.push(if p.len() > 1 {
            ALPHABET[((c & 15) << 2 | d >> 6) as usize] as char
        } else {
            '='
        });
        out.push(if p.len() > 2 {
            ALPHABET[(d & 63) as usize] as char
        } else {
            '='
        });
    }
}
pub(super) fn encode(
    f: &Font,
    p: &Prepared<'_>,
    o: &WriteOptions,
    v: &PlaydateWriteOptions,
    budget: &mut Budget,
) -> Result<(OutputBundle, FontSet, Report)> {
    let cols = p.entries.len().min(16) as u32;
    let rows = p.entries.len().div_ceil(cols as usize) as u32;
    let w = p
        .width
        .checked_mul(cols)
        .ok_or_else(|| invalid("Playdate atlas width overflow"))?;
    let h = p
        .height
        .checked_mul(rows)
        .ok_or_else(|| invalid("Playdate atlas height overflow"))?;
    let area = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("Playdate atlas size overflow"))?;
    check(
        area.saturating_mul(4),
        v.limits.max_decompressed_bytes,
        "Playdate atlas pixels",
    )?;
    budget.add(area)?;
    let mut alpha = vec![0; area];
    for (i, e) in p.entries.iter().enumerate() {
        let bm = canvas(e.glyph, p.width, p.height, p.ascent, &v.limits, budget)?;
        let stride = (p.width as usize).div_ceil(8);
        let left = (i as u32 % cols) * p.width;
        let top = (i as u32 / cols) * p.height;
        for y in 0..p.height {
            for x in 0..p.width {
                if bm[y as usize * stride + x as usize / 8] & (0x80 >> (x % 8)) != 0 {
                    alpha[((top + y) * w + left + x) as usize] = 255;
                }
            }
        }
    }
    let png = super::png::encode(w, h, &alpha, &v.limits, budget)?;
    let mut metadata = if let Some(PropertyValue::Text(raw)) = f.properties.get("playdate.metrics")
    {
        budget.add(raw.len().saturating_mul(12))?;
        serde_json::from_str::<serde_json::Value>(raw)
            .map_err(|_| invalid("invalid playdate.metrics JSON"))?
    } else {
        serde_json::json!({})
    };
    let object = metadata
        .as_object_mut()
        .ok_or_else(|| invalid("playdate.metrics must be an object"))?;
    let current = object
        .get("baseline")
        .and_then(|b| b.as_i64())
        .map(|b| if b > 0 { b } else { i64::from(p.height) + b });
    if current != Some(p.ascent) {
        object.insert(
            "baseline".into(),
            serde_json::json!(if p.ascent <= i64::from(p.height) {
                p.ascent - i64::from(p.height)
            } else {
                p.ascent
            }),
        );
    }
    let metrics = if current == Some(p.ascent) {
        match f.properties.get("playdate.metrics") {
            Some(PropertyValue::Text(raw)) => raw.clone(),
            _ => metadata.to_string(),
        }
    } else {
        metadata.to_string()
    };
    let comments = match f.properties.get("playdate.comments") {
        Some(PropertyValue::Text(raw))
            if raw.lines().all(|s| {
                s.starts_with("--") && !s.starts_with("--name=") && !s.starts_with("--metrics=")
            }) =>
        {
            raw.as_str()
        }
        None => "",
        _ => {
            return Err(invalid(
                "playdate.comments must contain ordinary native comments",
            ));
        }
    };
    let name = f.names.family.as_deref().unwrap_or("");
    if name.contains(['\r', '\n']) {
        return Err(invalid("Playdate name contains a newline"));
    }
    let embedded = v.flavor == PlaydateFlavor::FntEmbedded;
    let encoded = if embedded {
        png.len().div_ceil(3) * 4
    } else {
        0
    };
    let estimate = encoded
        .saturating_add(p.entries.len() * 32)
        .saturating_add(p.kern.len() * 48)
        .saturating_add(name.len() + metrics.len() + comments.len() + 256);
    check(
        estimate,
        v.max_output_bytes
            .min(v.limits.max_input_bytes)
            .min(v.limits.max_table_bytes),
        "FNT output text bytes",
    )?;
    budget.add(estimate)?;
    let mut text = String::with_capacity(estimate);
    if f.names.family.is_some() {
        writeln!(text, "--name={name}").unwrap();
    }
    writeln!(text, "--metrics={metrics}").unwrap();
    if !comments.is_empty() {
        writeln!(text, "{comments}").unwrap();
    }
    if embedded {
        writeln!(text, "datalen={encoded}").unwrap();
        text.push_str("data=");
        encode64(&png, &mut text);
        text.push('\n');
    }
    writeln!(
        text,
        "width={}\nheight={}\ntracking={}",
        p.width, p.height, p.tracking
    )
    .unwrap();
    for e in &p.entries {
        writeln!(text, "U+{:06X}\t{}", e.cp, e.width).unwrap();
    }
    for (&(a, b), &k) in &p.kern {
        writeln!(text, "U+{a:06X}U+{b:06X}\t{k}").unwrap();
    }
    // Include the reader's owned text view and decode the bytes actually embedded
    // in the output, so the same-limits check covers transport as well as pixels.
    budget.add(text.len())?;
    let parsed = source(text.as_bytes(), &v.limits, budget)?;
    let embedded_png = parsed
        .data
        .map(|data| decode64(data, &v.limits, budget))
        .transpose()?;
    let (target, report) = decode(
        &parsed,
        embedded_png.as_deref().unwrap_or(&png),
        &v.limits,
        budget,
    )?;
    let bundle = if embedded {
        OutputBundle::single(o.primary_name.clone(), text.into_bytes())
    } else {
        check(
            png.len(),
            v.limits.max_resource_bytes,
            "Playdate PNG sidecar bytes",
        )?;
        check(1, v.limits.max_resources, "Playdate sidecar count")?;
        check(
            text.len().saturating_add(png.len()),
            v.max_output_bytes,
            "Playdate output bundle bytes",
        )?;
        OutputBundle::new(
            o.primary_name.clone(),
            vec![
                OutputAsset {
                    path: atlas_name(o.primary_name.as_str(), p.width, p.height)?,
                    bytes: png,
                },
                OutputAsset {
                    path: o.primary_name.clone(),
                    bytes: text.into_bytes(),
                },
            ],
        )?
    };
    Ok((bundle, target, report))
}
