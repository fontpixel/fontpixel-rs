use super::{SsfnWriteOptions, binary, c};
use crate::model::{Codepoint, Font, FontSet, Glyph, PropertyValue as V, Vector};
use crate::{Report, ResourceLimits, Result};
use c::{Budget, alloc, append, area, check, invalid, property};
fn num<T: std::str::FromStr>(s: &str) -> Result<T> {
    s.parse().map_err(|_| invalid("SSFN ASCII integer"))
}
fn quoted(s: &str) -> Result<&str> {
    let s = s.trim_start();
    let rest = s
        .strip_prefix('"')
        .ok_or_else(|| invalid("SSFN ASCII quoted string"))?;
    let end = rest
        .find('"')
        .ok_or_else(|| invalid("SSFN ASCII unterminated string"))?;
    let value = &rest[..end];
    if value.len() > 255 || value.chars().any(char::is_control) {
        return Err(invalid("SSFN ASCII string length/control"));
    }
    Ok(value)
}
fn argb(s: &str) -> Result<(u8, bool)> {
    if s == "--------" {
        return Ok((0, false));
    }
    if s.len() != 8 {
        return Err(invalid("SSFN ARGB value width"));
    }
    let v = u32::from_str_radix(s, 16).map_err(|_| invalid("SSFN ARGB hex value"))?;
    if matches!(v, 0 | 0xff000000 | 0xff000100) {
        return Ok((0, false));
    }
    Ok(((v >> 24) as u8, v & 0xffffff != 0xffffff))
}
struct Record {
    cp: u32,
    w: u32,
    h: u32,
    x: u8,
    y: u8,
    o: u8,
    seq: Option<Vec<u32>>,
    pixels: Vec<u8>,
    gray: bool,
    color: bool,
    outline: bool,
    special: bool,
    raster: bool,
}
fn glyph_header(s: &str, l: &ResourceLimits, b: &mut Budget) -> Result<Record> {
    let s = s
        .strip_prefix("===U+")
        .ok_or_else(|| invalid("SSFN glyph header"))?;
    let (hex, rest) = s
        .split_once("===w")
        .ok_or_else(|| invalid("SSFN glyph header fields"))?;
    let cp = u32::from_str_radix(hex, 16).map_err(|_| invalid("SSFN glyph Unicode hex"))?;
    if char::from_u32(cp).is_none() {
        return Err(invalid("SSFN Unicode scalar"));
    }
    let (w, rest) = rest
        .split_once("=h")
        .ok_or_else(|| invalid("SSFN glyph height"))?;
    let (h, rest) = rest
        .split_once("=x")
        .ok_or_else(|| invalid("SSFN glyph advance"))?;
    let (x, rest) = rest
        .split_once("=y")
        .ok_or_else(|| invalid("SSFN glyph vertical advance"))?;
    let (y, rest) = rest
        .split_once("=o")
        .ok_or_else(|| invalid("SSFN glyph overlap"))?;
    let (over, rest) = rest
        .split_once('=')
        .ok_or_else(|| invalid("SSFN glyph header terminator"))?;
    let w: u8 = num(w)?;
    let h: u8 = num(h)?;
    let o: u8 = num(over)?;
    if o > 63 {
        return Err(invalid("SSFN overlap exceeds six bits"));
    }
    let seq = if (0xf000..=0xf8ff).contains(&cp) && rest.starts_with('"') {
        let s = quoted(rest)?;
        b.items::<u32>(s.chars().count())?;
        Some(s.chars().map(u32::from).collect())
    } else {
        None
    };
    let pixels = alloc(area(u32::from(w), u32::from(h), l)?, b)?;
    Ok(Record {
        cp,
        w: u32::from(w),
        h: u32::from(h),
        x: num(x)?,
        y: num(y)?,
        o,
        seq,
        pixels,
        gray: false,
        color: false,
        outline: false,
        special: false,
        raster: false,
    })
}
fn finish(
    f: &mut Font,
    g: Record,
    baseline: u8,
    l: &ResourceLimits,
    b: &mut Budget,
    r: &mut Report,
) -> Result<(bool, bool)> {
    check(f.len() + 1, l.max_glyphs, "SSFN glyph count")?;
    b.items::<Option<Glyph>>(1)?;
    b.add(512)?;
    let bitmap = binary::bitmap(g.w, g.h, &g.pixels, g.gray, l, b)?;
    let mut out = Glyph::new(bitmap);
    out.codepoints.push(Codepoint::Unicode(g.cp));
    out.metrics.bounds.x = -i64::from(g.o);
    out.metrics.bounds.y = i64::from(baseline) - i64::from(g.h);
    out.metrics.advance = Some(Vector::new(i64::from(g.x), -i64::from(g.y)));
    if let Some(seq) = g.seq {
        out.unicode_sequences.push(seq);
    }
    for (yes, code, msg) in [
        (
            g.color,
            "color-reduced",
            "ARGB colors reduced to alpha coverage",
        ),
        (
            g.outline,
            "contours-omitted",
            "ASCII vector contours omitted from raster content",
        ),
        (
            g.special,
            "layout-data-omitted",
            "ASCII kerning/hint commands omitted from raster model",
        ),
    ] {
        if yes {
            b.loss(r, "ssfn", code, Some(0), Some(f.len()), msg)?;
        }
    }
    f.add_glyph(out)?;
    Ok((g.raster, g.outline))
}
pub(super) fn decode(
    data: &[u8],
    l: &ResourceLimits,
    b: &mut Budget,
    r: &mut Report,
) -> Result<FontSet> {
    let text = std::str::from_utf8(data).map_err(|_| invalid("SSFN ASCII is not UTF-8"))?;
    let mut lines = text.lines().peekable();
    if lines.next() != Some("# Scalable Screen Font #") {
        return Err(invalid("SSFN ASCII magic"));
    }
    check(1, l.max_resources, "SSFN font count")?;
    b.items::<Font>(1)?;
    b.add(512)?;
    let mut f = Font::default();
    f.source_format = Some("ssfn".into());
    let mut names: [String; 6] = Default::default();
    let mut baseline = 0u8;
    let mut underline = None;
    let mut ty = 0u8;
    let mut dimension = None;
    let mut current: Option<Record> = None;
    let mut fg = 255;
    let mut fg_color = false;
    let mut ended = false;
    let mut raster = false;
    let mut outline = false;
    let mut codes = std::collections::BTreeSet::new();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "# End #" {
            ended = true;
            break;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(property_line) = line.strip_prefix('$') {
            if current.is_some() {
                return Err(invalid("SSFN properties after glyph data"));
            }
            let (key, value) = property_line
                .split_once(char::is_whitespace)
                .ok_or_else(|| invalid("SSFN property value"))?;
            let value = value.trim();
            let token = value.split_whitespace().next().unwrap_or("");
            match key {
                "type" => {
                    let v: u8 = num(token)?;
                    if v > 4 {
                        return Err(invalid("SSFN font family"));
                    }
                    ty = (ty & 240) | v;
                }
                "style" => {
                    ty &= 15;
                    if value.contains('b') {
                        ty |= 16;
                    }
                    if value.contains('i') {
                        ty |= 32;
                    }
                    if value.contains('1') {
                        ty |= 64;
                    }
                    if value.contains('2') {
                        ty |= 128;
                    }
                }
                "baseline" => baseline = num(token)?,
                "underline" => underline = Some(num::<u8>(token)?),
                "glyphdim" => {
                    let mut values = value.split_whitespace();
                    let w: u8 = num(values.next().ok_or_else(|| invalid("SSFN font width"))?)?;
                    let h: u8 = num(values.next().ok_or_else(|| invalid("SSFN font height"))?)?;
                    dimension = Some((w, h));
                }
                "name" | "family" | "subfamily" | "revision" | "manufacturer" | "license" => {
                    let index = match key {
                        "name" => 0,
                        "family" => 1,
                        "subfamily" => 2,
                        "revision" => 3,
                        "manufacturer" => 4,
                        _ => 5,
                    };
                    let v = quoted(value)?;
                    b.add(v.len())?;
                    names[index] = v.to_owned();
                }
                _ => {
                    b.add(value.len())?;
                    b.add(key.len())?;
                    property(
                        &mut f.properties,
                        &format!("ssfn.ascii.{key}"),
                        V::Text(value.to_owned()),
                        b,
                    )?;
                }
            }
            continue;
        }
        if line.starts_with("===U+") {
            if let Some(g) = current.take() {
                let (a, o) = finish(&mut f, g, baseline, l, b, r)?;
                raster |= a;
                outline |= o;
            }
            let g = glyph_header(line, l, b)?;
            b.items::<u32>(1)?;
            if !codes.insert(g.cp) {
                return Err(invalid("SSFN duplicate code"));
            }
            current = Some(g);
            fg = 255;
            fg_color = false;
            continue;
        }
        let g = current
            .as_mut()
            .ok_or_else(|| invalid("SSFN command before glyph"))?;
        if let Some(value) = line.strip_prefix("f ") {
            let (alpha, col) = argb(value.trim())?;
            fg = alpha;
            fg_color = col;
            continue;
        }
        if line.starts_with(['m', 'l', 'q', 'c']) {
            let (cmd, args) = line
                .split_once(char::is_whitespace)
                .ok_or_else(|| invalid("SSFN contour arguments"))?;
            let expected = match cmd {
                "m" | "l" => 1,
                "q" => 2,
                "c" => 3,
                _ => return Err(invalid("SSFN contour command")),
            };
            let mut n = 0;
            for pair in args.split_whitespace() {
                let (x, y) = pair
                    .split_once(',')
                    .ok_or_else(|| invalid("SSFN contour coordinate pair"))?;
                let _: u8 = num(x)?;
                let _: u8 = num(y)?;
                n += 1;
            }
            if n != expected {
                return Err(invalid("SSFN contour argument count"));
            }
            g.outline = true;
            continue;
        }
        if line.starts_with("H ") || line.starts_with("V ") {
            let mut n = 0;
            for v in line[2..].split_whitespace() {
                let _: u8 = num(v)?;
                n += 1;
            }
            if n > 32 {
                return Err(invalid("SSFN hint coordinate count"));
            }
            g.special = true;
            continue;
        }
        if let Some(args) = line.strip_prefix("k ") {
            for entry in args.split(',') {
                let mut args = entry.split_whitespace();
                let cp = args
                    .next()
                    .ok_or_else(|| invalid("SSFN kerning next code"))?;
                if let Some(h) = cp.strip_prefix("U+") {
                    let v = u32::from_str_radix(h, 16)
                        .map_err(|_| invalid("SSFN kerning Unicode hex"))?;
                    if char::from_u32(v).is_none() {
                        return Err(invalid("SSFN kerning Unicode scalar"));
                    }
                } else if cp.chars().count() != 1 {
                    return Err(invalid("SSFN kerning next scalar"));
                }
                let _: i8 = num(args.next().ok_or_else(|| invalid("SSFN kerning amount"))?)?;
                if let Some(v) = args.next() {
                    let _: i8 = num(v)?;
                }
                if args.next().is_some() {
                    return Err(invalid("SSFN kerning fields"));
                }
            }
            g.special = true;
            continue;
        }
        if g.h == 0 || g.w == 0 {
            return Err(invalid("SSFN raster layer on empty canvas"));
        }
        let mono = line.starts_with('.') || line.starts_with('X');
        g.raster = true;
        g.gray |= !mono || fg != 255;
        g.color |= mono && fg_color;
        let mut row = line;
        for y in 0..g.h as usize {
            if mono {
                let values = row.as_bytes();
                if values.len() != g.w as usize && values.len() != (g.w as usize).div_ceil(8) * 8 {
                    return Err(invalid("SSFN bitmap row length"));
                }
                for (x, &v) in values.iter().enumerate() {
                    if !matches!(v, b'.' | b'X') {
                        return Err(invalid("SSFN bitmap pixel"));
                    }
                    if x >= g.w as usize && v == b'X' {
                        return Err(invalid("SSFN nonzero bitmap padding"));
                    }
                    if v == b'X' && x < g.w as usize && fg != 0 {
                        g.pixels[y * g.w as usize + x] = fg;
                    }
                }
            } else {
                let mut x = 0;
                for value in row.split_whitespace() {
                    if x >= g.w as usize {
                        return Err(invalid("SSFN excess pixmap columns"));
                    }
                    let (alpha, col) = argb(value)?;
                    g.color |= col;
                    if alpha != 0 {
                        g.pixels[y * g.w as usize + x] = alpha;
                    }
                    x += 1;
                }
                if x != g.w as usize {
                    return Err(invalid("SSFN short pixmap row"));
                }
            }
            if y + 1 < g.h as usize {
                row = lines
                    .next()
                    .ok_or_else(|| invalid("SSFN truncated raster layer"))?
                    .trim();
            }
        }
        fg = 255;
        fg_color = false;
    }
    if !ended || lines.any(|s| !s.trim().is_empty()) {
        return Err(invalid("SSFN ASCII missing/trailing end tag"));
    }
    if let Some(g) = current {
        let (a, o) = finish(&mut f, g, baseline, l, b, r)?;
        raster |= a;
        outline |= o;
    }
    if outline && !raster {
        return Err(invalid("SSFN contains contours but no raster layer"));
    }
    binary::set_names(&mut f, names);
    let (w, h) = dimension.unwrap_or_else(|| {
        (
            f.glyphs()
                .map(|(_, g)| g.bitmap.width() as u8)
                .max()
                .unwrap_or(0),
            f.glyphs()
                .map(|(_, g)| g.bitmap.height() as u8)
                .max()
                .unwrap_or(0),
        )
    });
    f.metrics.pixel_size = Some(u32::from(h));
    f.metrics.ascent = Some(i64::from(baseline));
    f.metrics.descent = Some(i64::from(h) - i64::from(baseline));
    for (key, value) in [
        ("ssfn.type", ty),
        ("ssfn.width", w),
        ("ssfn.underline", underline.unwrap_or(baseline)),
    ] {
        property(&mut f.properties, key, V::Integer(i64::from(value)), b)?;
    }
    let mut set = FontSet::from(f);
    set.source_format = Some("ssfn".into());
    Ok(set)
}
fn line(out: &mut Vec<u8>, s: &str, b: &mut Budget) -> Result<()> {
    append(out, s.as_bytes(), b)?;
    append(out, b"\n", b)
}
pub(super) fn encode(
    s: &FontSet,
    p: &SsfnWriteOptions,
    b: &mut Budget,
    _r: &mut Report,
) -> Result<Vec<u8>> {
    if s.fonts.len() != 1 {
        return Err(invalid("SSFN ASCII represents exactly one font"));
    }
    let f = &s.fonts[0];
    let (w, h, base, under, ty) = binary::grid(f)?;
    let mut out = Vec::new();
    line(&mut out, "# Scalable Screen Font #", b)?;
    line(&mut out, &format!("$glyphdim {w} {h}"), b)?;
    line(&mut out, &format!("$type {}", ty & 15), b)?;
    line(
        &mut out,
        &format!(
            "$style {}{}{}{}",
            if ty & 16 != 0 { "bold" } else { "" },
            if ty & 32 != 0 { "italic" } else { "" },
            if ty & 64 != 0 { "1" } else { "" },
            if ty & 128 != 0 { "2" } else { "regular" }
        ),
        b,
    )?;
    line(&mut out, &format!("$baseline {base}"), b)?;
    line(&mut out, &format!("$underline {under}"), b)?;
    for (key, name) in [
        "name",
        "family",
        "subfamily",
        "revision",
        "manufacturer",
        "license",
    ]
    .into_iter()
    .zip(binary::names(f))
    {
        if name.len() > 255 || name.chars().any(|c| c == '"' || c.is_control()) {
            return Err(invalid(
                "SSFN ASCII string cannot contain quotes/control characters",
            ));
        }
        line(&mut out, &format!("${key} \"{name}\""), b)?;
    }
    for (cp, g) in binary::codes(f, p, b)? {
        let (w, h, over, pixels) = binary::canvas(g, base, &p.limits, b)?;
        let a = g.metrics.advance.unwrap_or(Vector::new(i64::from(w), 0));
        let x = u8::try_from(a.x).map_err(|_| invalid("SSFN advance byte"))?;
        let y = u8::try_from(-a.y).map_err(|_| invalid("SSFN vertical advance byte"))?;
        let mut seq = String::new();
        if (0xf000..=0xf8ff).contains(&cp)
            && let Some(s) = g.unicode_sequences.first()
        {
            b.add(s.len() * 4)?;
            for &v in s {
                seq.push(char::from_u32(v).ok_or_else(|| invalid("SSFN ligature scalar"))?);
            }
        }
        if seq.contains('"') || seq.chars().any(char::is_control) {
            return Err(invalid("SSFN ASCII ligature quote/control"));
        }
        line(
            &mut out,
            &format!("===U+{cp:06X}===w{w}=h{h}=x{x}=y{y}=o{over}=\"{seq}\"==="),
            b,
        )?;
        let gray = matches!(g.bitmap, crate::model::GlyphBitmap::Gray8(_));
        for row in pixels.chunks(usize::from(w).max(1)) {
            for (i, &value) in row.iter().enumerate() {
                if gray {
                    if i != 0 {
                        append(&mut out, b" ", b)?;
                    }
                    append(
                        &mut out,
                        if value == 0 {
                            b"--------"
                        } else {
                            let color = format!("{value:02X}FFFFFF");
                            append(&mut out, color.as_bytes(), b)?;
                            continue;
                        },
                        b,
                    )?;
                } else {
                    append(&mut out, if value == 0 { b"." } else { b"X" }, b)?;
                }
            }
            append(&mut out, b"\n", b)?;
        }
    }
    line(&mut out, "# End #", b)?;
    check(
        out.len(),
        p.max_output_bytes
            .min(p.limits.max_input_bytes)
            .min(p.limits.max_table_bytes),
        "SSFN ASCII output bytes",
    )?;
    Ok(out)
}
