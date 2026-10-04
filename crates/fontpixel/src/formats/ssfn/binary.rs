use super::{SsfnWriteOptions, c};
use crate::bitmap::{Bitmap, Gray8, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, PropertyValue as V, Vector};
use crate::{Report, ResourceLimits, Result};
use c::{
    Budget, add, alloc, append, area, bytes, check, invalid, mul, number, property, put, scalar,
};
use std::collections::BTreeMap;
#[derive(Clone, Copy)]
struct Header {
    frag: usize,
    chars: usize,
    lig: usize,
    kern: usize,
    palette: usize,
    end: usize,
}
fn header(b: &[u8]) -> Result<Header> {
    if b.len() < 42
        || !b.starts_with(b"SFN2")
        || number(b, 4, 4, true)? as usize != b.len()
        || !b.ends_with(b"2NFS")
        || b[9] != 0
        || b[8] & 15 > 4
    {
        return Err(invalid("invalid SSFN header/version/trailer"));
    }
    let h = Header {
        frag: number(b, 14, 2, true)? as usize,
        chars: number(b, 16, 4, true)? as usize,
        lig: number(b, 20, 4, true)? as usize,
        kern: number(b, 24, 4, true)? as usize,
        palette: number(b, 28, 4, true)? as usize,
        end: b.len() - 4,
    };
    let mut prev = 32;
    for pos in [h.frag, h.chars, h.lig, h.kern, h.palette, h.end] {
        if pos == 0 {
            continue;
        }
        if pos < prev || pos > h.end {
            return Err(invalid("SSFN sections are not ordered"));
        }
        prev = pos;
    }
    if h.frag < 38 || h.chars < h.frag {
        return Err(invalid("SSFN fragment/string offset"));
    }
    if h.palette != 0 && !(h.end - h.palette).is_multiple_of(4) {
        return Err(invalid("SSFN palette size"));
    }
    if h.palette != 0 && (h.end - h.palette) / 4 > 254 {
        return Err(invalid("SSFN palette has too many colors"));
    }
    Ok(h)
}
pub(super) fn probe(b: &[u8]) -> Result<()> {
    if b.starts_with(b"SFNC") {
        if number(b, 4, 4, true)? as usize != b.len() {
            return Err(invalid("SSFN collection size"));
        }
        let mut off = 8;
        let mut count = 0;
        while off < b.len() {
            let n = number(b, off + 4, 4, true)? as usize;
            header(bytes(b, off, n)?)?;
            off = add(off, n)?;
            count += 1;
        }
        if count == 0 {
            return Err(invalid("empty SSFN collection"));
        }
        Ok(())
    } else {
        header(b).map(|_| ())
    }
}
fn section_end(h: Header, start: usize) -> usize {
    [h.lig, h.kern, h.palette, h.end]
        .into_iter()
        .filter(|&p| p > start)
        .min()
        .unwrap_or(h.end)
}
fn text(b: &[u8]) -> Result<&str> {
    if b.len() > 255 || b.iter().any(|&c| c < 32) {
        return Err(invalid("SSFN string length/control character"));
    }
    std::str::from_utf8(b).map_err(|_| invalid("SSFN string is not UTF-8"))
}
pub(super) fn set_names(f: &mut Font, n: [String; 6]) {
    let [a, b, c, d, e, z] = n;
    f.names.full = Some(a);
    f.names.family = Some(b);
    f.names.style = Some(c);
    f.names.version = Some(d);
    f.names.foundry = Some(e);
    f.names.license = Some(z);
}
pub(super) fn names(f: &Font) -> [&str; 6] {
    [
        f.names.full.as_deref().unwrap_or(""),
        f.names.family.as_deref().unwrap_or(""),
        f.names.style.as_deref().unwrap_or(""),
        f.names.version.as_deref().unwrap_or(""),
        f.names.foundry.as_deref().unwrap_or(""),
        f.names.license.as_deref().unwrap_or(""),
    ]
}
fn fragment_len(b: &[u8], at: usize) -> Result<usize> {
    let first = *bytes(b, at, 1)?
        .first()
        .ok_or_else(|| invalid("SSFN fragment"))?;
    let n = match first >> 5 {
        0..=3 => {
            let wide = first & 64 != 0;
            let count = if wide {
                ((usize::from(first & 63) << 8) | usize::from(bytes(b, at + 1, 1)?[0])) + 1
            } else {
                usize::from(first & 63) + 1
            };
            let base = 1 + usize::from(wide);
            let command = bytes(b, at + base, count.div_ceil(4))?;
            let mut args = 0;
            for i in 0..count {
                args += match (command[i / 4] >> (i % 4 * 2)) & 3 {
                    0 if i != 0 => 0,
                    0 | 1 => 2,
                    2 => 4,
                    _ => 6,
                };
            }
            base + command.len() + args
        }
        4 => 2 + (usize::from(first & 31) + 1) * (usize::from(bytes(b, at + 1, 1)?[0]) + 1),
        5 => 4 + ((usize::from(first & 31) << 8) | usize::from(bytes(b, at + 1, 1)?[0])) + 1,
        6 => 2 + (((usize::from(first & 3) << 8) | usize::from(bytes(b, at + 1, 1)?[0])) + 1) * 8,
        7 => 1 + usize::from(first & 31),
        _ => unreachable!(),
    };
    bytes(b, at, n)?;
    Ok(n)
}
fn color(index: u8, b: &[u8], h: Header) -> Result<(u8, bool)> {
    if index == 255 {
        return Ok((0, false));
    }
    if index == 254 {
        return Ok((255, false));
    }
    if h.palette == 0 {
        return Err(invalid("SSFN color index without palette"));
    }
    let p = bytes(b, add(h.palette, usize::from(index) * 4)?, 4)?;
    if h.palette + usize::from(index) * 4 + 4 > h.end {
        return Err(invalid("SSFN palette index"));
    }
    Ok((p[3], p[..3] != [255, 255, 255]))
}
#[allow(clippy::too_many_arguments)] // Fragment composition uses explicit native bounds and shared resource accounting.
fn compose(
    data: &[u8],
    dx: usize,
    dy: usize,
    out: &mut [u8],
    w: usize,
    ht: usize,
    fg: u8,
    b: &[u8],
    h: Header,
    budget: &mut Budget,
) -> Result<(bool, bool, bool)> {
    let kind = data[0] >> 5;
    let mut gray = false;
    let mut colored = false;
    let mut outline = false;
    let mut paint = |x: usize, y: usize, value: u8| -> Result<()> {
        if value == 0 {
            return Ok(());
        }
        let xx = add(dx, x)?;
        let yy = add(dy, y)?;
        if xx >= w || yy >= ht {
            return Err(invalid("SSFN fragment ink outside glyph canvas"));
        }
        out[yy * w + xx] = value;
        Ok(())
    };
    match kind {
        4 => {
            let pitch = usize::from(data[0] & 31) + 1;
            let height = usize::from(data[1]) + 1;
            budget.add(mul(pitch * 8, height)?)?;
            let (ink, rgb) = color(fg, b, h)?;
            colored |= rgb;
            gray |= ink != 255;
            for y in 0..height {
                for x in 0..pitch * 8 {
                    if data[2 + y * pitch + x / 8] & (1 << (x % 8)) != 0 {
                        paint(x, y, ink)?
                    }
                }
            }
        }
        5 => {
            gray = true;
            let width = usize::from(data[2]) + 1;
            let height = usize::from(data[3]) + 1;
            let total = width * height;
            budget.add(total)?;
            let mut off = 4;
            let mut pos = 0;
            while off < data.len() {
                let packet = data[off];
                off += 1;
                let n = usize::from(packet & 127) + 1;
                if add(pos, n)? > total {
                    return Err(invalid("SSFN pixmap packet overflow"));
                }
                let size = if packet & 128 != 0 { 1 } else { n };
                let payload = bytes(data, off, size)?;
                for i in 0..n {
                    let idx = payload[if size == 1 { 0 } else { i }];
                    let (ink, rgb) = color(idx, b, h)?;
                    colored |= rgb;
                    paint((pos + i) % width, (pos + i) / width, ink)?;
                }
                off += size;
                pos += n;
            }
            if pos != total {
                return Err(invalid("SSFN pixmap is incomplete"));
            }
        }
        0..=3 => outline = true,
        6 | 7 => {}
        _ => return Err(invalid("SSFN fragment kind")),
    }
    Ok((gray, colored, outline))
}
pub(super) fn decode(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
    rpt: &mut Report,
) -> Result<FontSet> {
    probe(b)?;
    let mut set = FontSet {
        source_format: Some("ssfn".into()),
        ..Default::default()
    };
    let mut offset = if b.starts_with(b"SFNC") { 8 } else { 0 };
    let mut glyph_count = 0;
    loop {
        check(set.fonts.len() + 1, l.max_resources, "SSFN font count")?;
        let n = number(b, offset + 4, 4, true)? as usize;
        let f = font(
            bytes(b, offset, n)?,
            l,
            budget,
            rpt,
            set.fonts.len(),
            &mut glyph_count,
        )?;
        set.fonts.push(f);
        offset = add(offset, n)?;
        if offset == b.len() {
            break;
        }
    }
    Ok(set)
}
fn font(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
    rpt: &mut Report,
    fi: usize,
    total: &mut usize,
) -> Result<Font> {
    let h = header(b)?;
    budget.items::<Font>(1)?;
    budget.add(512)?;
    let mut f = Font::default();
    f.source_format = Some("ssfn".into());
    let mut ns: [String; 6] = Default::default();
    let mut at = 32;
    for name in &mut ns {
        let len = b[at..h.frag]
            .iter()
            .position(|&v| v == 0)
            .ok_or_else(|| invalid("SSFN unterminated name"))?;
        let value = text(&b[at..at + len])?;
        budget.add(value.len())?;
        *name = value.to_owned();
        at += len + 1;
    }
    set_names(&mut f, ns);
    f.metrics.pixel_size = Some(u32::from(b[11]));
    f.metrics.ascent = Some(i64::from(b[12]));
    f.metrics.descent = Some(i64::from(b[11]) - i64::from(b[12]));
    for (key, v) in [
        ("ssfn.type", b[8]),
        ("ssfn.width", b[10]),
        ("ssfn.underline", b[13]),
    ] {
        property(&mut f.properties, key, V::Integer(i64::from(v)), budget)?;
    }
    let mut fragments = BTreeMap::new();
    let mut off = h.frag;
    while off < h.chars {
        let n = fragment_len(&b[..h.chars], off)?;
        budget.items::<(usize, usize)>(1)?;
        fragments.insert(off, n);
        off = add(off, n)?;
    }
    let mut ligatures = BTreeMap::new();
    if h.lig != 0 {
        let end = section_end(h, h.lig);
        let mut off = h.lig;
        let mut cp = 0xf000;
        let mut terminated = false;
        while off < end {
            let ptr = number(b, off, 2, true)? as usize;
            off += 2;
            if ptr == 0 {
                terminated = true;
                break;
            }
            if cp > 0xf8ff || ptr < at || ptr >= h.frag {
                return Err(invalid("SSFN ligature string offset/code"));
            }
            let len = b[ptr..h.frag]
                .iter()
                .position(|&v| v == 0)
                .ok_or_else(|| invalid("SSFN ligature termination"))?;
            let s = text(&b[ptr..ptr + len])?;
            budget.items::<u32>(s.chars().count())?;
            budget.items::<(u32, Vec<u32>)>(1)?;
            ligatures.insert(cp, s.chars().map(u32::from).collect::<Vec<_>>());
            cp += 1;
        }
        if off != end {
            return Err(invalid("SSFN ligature table termination"));
        }
        if !terminated {
            budget.diagnostic(rpt, "ssfn", "author-ligature-termination", crate::report::Impact::Repair, Some(fi), None, "Accepted author converter ligatures ending at the section boundary without the documented zero terminator")?;
        }
    }
    let end = section_end(h, h.chars);
    let mut off = h.chars;
    let mut cp = 0u32;
    let mut had_raster = false;
    let mut had_outline = false;
    while off < end {
        let flag = b[off];
        off += 1;
        if flag == 255 {
            cp = cp
                .checked_add(65536)
                .ok_or_else(|| invalid("SSFN skip overflow"))?;
        } else if flag & 192 == 192 {
            let low = bytes(b, off, 1)?[0];
            off += 1;
            cp += ((u32::from(flag & 63) << 8) | u32::from(low)) + 1;
        } else if flag & 128 != 0 {
            cp += u32::from(flag & 63) + 1;
        } else {
            if char::from_u32(cp).is_none() {
                return Err(invalid("SSFN glyph has invalid Unicode scalar"));
            }
            let ghead = bytes(b, off, 5)?;
            off += 5;
            let count = usize::from(ghead[0]);
            let w = u32::from(ghead[1]);
            let ht = u32::from(ghead[2]);
            let n = area(w, ht, l)?;
            *total = add(*total, 1)?;
            check(*total, l.max_glyphs, "SSFN total glyphs")?;
            budget.items::<Option<Glyph>>(1)?;
            budget.add(512)?;
            let mut pixels = alloc(n, budget)?;
            let mut gray = false;
            let mut colored = false;
            let mut outline = false;
            let mut special = false;
            let mut fg = 254;
            for _ in 0..count {
                let size = if flag & 64 != 0 { 6 } else { 5 };
                let d = bytes(b, off, size)?;
                off += size;
                if off > end {
                    return Err(invalid("SSFN descriptors exceed character section"));
                }
                if d[0] == 255 && d[1] == 255 {
                    if d[3..].iter().any(|&v| v != 0) {
                        return Err(invalid("SSFN color command reserved bytes"));
                    }
                    fg = d[2];
                    continue;
                }
                let ptr = number(d, 2, size - 2, true)? as usize;
                let len = *fragments
                    .get(&ptr)
                    .ok_or_else(|| invalid("SSFN reference is not a fragment boundary"))?;
                let data = bytes(b, ptr, len)?;
                let kind = data[0] >> 5;
                if kind == 6 {
                    if data[0] & 28 != 0 {
                        return Err(invalid("SSFN unsupported kerning context"));
                    }
                    if h.kern == 0 {
                        return Err(invalid("SSFN kerning fragment without table"));
                    }
                    for range in data[2..].as_chunks::<8>().0 {
                        let first = number(range, 0, 2, true)? | u32::from(range[2] & 15) << 16;
                        let last = number(range, 3, 2, true)? | u32::from(range[5] & 15) << 16;
                        if first > last {
                            return Err(invalid("SSFN reversed kerning range"));
                        }
                        let kp = number(range, 6, 2, true)?
                            | u32::from(range[5] >> 4) << 16
                            | u32::from(range[2] >> 4) << 24;
                        let mut ko = add(h.kern, kp as usize)?;
                        let kend = section_end(h, h.kern);
                        let mut produced = 0usize;
                        let needed = (last - first + 1) as usize;
                        budget.add(needed)?;
                        while produced < needed {
                            let command = bytes(&b[..kend], ko, 1)?[0];
                            ko += 1;
                            let n = usize::from(command & 127) + 1;
                            if produced + n > needed {
                                return Err(invalid("SSFN kerning packet overrun"));
                            }
                            let take = if command & 128 != 0 { 1 } else { n };
                            bytes(&b[..kend], ko, take)?;
                            ko += take;
                            produced += n;
                        }
                    }
                    special = true;
                } else if kind == 7 {
                    special = true;
                }
                let (a, c, o) = compose(
                    data,
                    usize::from(d[0]),
                    usize::from(d[1]),
                    &mut pixels,
                    w as usize,
                    ht as usize,
                    fg,
                    b,
                    h,
                    budget,
                )?;
                gray |= a;
                colored |= c;
                outline |= o;
                had_raster |= kind == 4 || kind == 5;
                fg = 254;
            }
            let bitmap = bitmap(w, ht, &pixels, gray, l, budget)?;
            let mut g = Glyph::new(bitmap);
            g.codepoints.push(Codepoint::Unicode(cp));
            if let Some(seq) = ligatures.remove(&cp) {
                g.unicode_sequences.push(seq);
            }
            g.metrics.bounds.x = -i64::from(flag & 63);
            g.metrics.bounds.y = i64::from(b[12]) - i64::from(ht);
            g.metrics.advance = Some(Vector::new(i64::from(ghead[3]), -i64::from(ghead[4])));
            let gi = f.len();
            if colored {
                budget.loss(
                    rpt,
                    "ssfn",
                    "color-reduced",
                    Some(fi),
                    Some(gi),
                    "RGB layer colors reduced to alpha coverage",
                )?;
            }
            if outline {
                had_outline = true;
                budget.loss(
                    rpt,
                    "ssfn",
                    "contours-omitted",
                    Some(fi),
                    Some(gi),
                    "Vector contours omitted from native raster fragments",
                )?;
            }
            if special {
                budget.loss(
                    rpt,
                    "ssfn",
                    "layout-data-omitted",
                    Some(fi),
                    Some(gi),
                    "Native kerning/hint fragments omitted from raster model",
                )?;
            }
            f.add_glyph(g)?;
            cp += 1;
        }
        if cp > 0x110000 && !(cp == 0x110001 && off == end && flag >= 128) {
            return Err(invalid("SSFN character skip exceeds Unicode range"));
        }
    }
    if ![0x110000, 0x110001].contains(&cp) || off != end {
        return Err(invalid(
            "SSFN character stream does not cover Unicode range",
        ));
    }
    if cp == 0x110001 {
        budget.diagnostic(rpt, "ssfn", "author-terminal-skip", crate::report::Impact::Repair, Some(fi), None, "Accepted the author converter's one-position terminal skip overshoot; no glyph occupies that position")?;
    }
    if had_outline && !had_raster {
        return Err(invalid("SSFN contains contours but no raster fragments"));
    }
    if !ligatures.is_empty() {
        return Err(invalid("SSFN ligature references absent glyph"));
    }
    Ok(f)
}
pub(super) fn bitmap(
    w: u32,
    h: u32,
    pixels: &[u8],
    gray: bool,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<GlyphBitmap> {
    if gray {
        budget.add(pixels.len())?;
        Ok(Bitmap::<Gray8>::from_raw(w, h, pixels, c::bitmap_limits(l))?.into())
    } else {
        let pitch = (w as usize).div_ceil(8);
        let mut packed = alloc(mul(pitch, h as usize)?, budget)?;
        for y in 0..h as usize {
            for x in 0..w as usize {
                if pixels[y * w as usize + x] != 0 {
                    packed[y * pitch + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        budget.add(packed.len())?;
        Ok(Bitmap::<Mono>::from_raw(w, h, &packed, c::bitmap_limits(l))?.into())
    }
}
pub(super) fn grid(f: &Font) -> Result<(u8, u8, u8, u8, u8)> {
    let height = f
        .metrics
        .pixel_size
        .or_else(|| f.glyphs().map(|(_, g)| g.bitmap.height()).max())
        .unwrap_or(0);
    let ascent = f.metrics.ascent.unwrap_or(i64::from(height));
    let width = scalar(
        &f.properties,
        "ssfn.width",
        f.glyphs()
            .map(|(_, g)| i64::from(g.bitmap.width()) + g.metrics.bounds.x.max(0))
            .max()
            .unwrap_or(0),
    )?;
    let ty = scalar(&f.properties, "ssfn.type", 0)?;
    if !(0..=255).contains(&ty) || ty & 15 > 4 {
        return Err(invalid("SSFN family/style byte"));
    }
    let field =
        |v: i64| u8::try_from(v).map_err(|_| invalid("SSFN font grid exceeds byte coordinates"));
    Ok((
        field(width)?,
        field(i64::from(height))?,
        field(ascent)?,
        field(scalar(&f.properties, "ssfn.underline", ascent)?)?,
        ty as u8,
    ))
}
pub(super) fn codes<'a>(
    f: &'a Font,
    p: &SsfnWriteOptions,
    budget: &mut Budget,
) -> Result<BTreeMap<u32, &'a Glyph>> {
    budget.items::<(u32, &Glyph)>(f.len())?;
    let mut out = BTreeMap::new();
    for (_, g) in f.glyphs() {
        let cp = g
            .codepoints
            .first()
            .ok_or_else(|| invalid("SSFN requires Unicode-mapped glyphs"))?;
        if matches!(cp, Codepoint::Legacy(_)) && !p.legacy_as_unicode {
            return Err(invalid(
                "SSFN legacy-to-Unicode conversion must be explicit",
            ));
        }
        let n = cp.value();
        if char::from_u32(n).is_none() || out.insert(n, g).is_some() {
            return Err(invalid("SSFN duplicate or invalid Unicode mapping"));
        }
    }
    Ok(out)
}
pub(super) fn canvas(
    g: &Glyph,
    base: u8,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(u8, u8, u8, Vec<u8>)> {
    let left = g.metrics.bounds.x.max(0);
    let overlap = (-g.metrics.bounds.x).max(0);
    let top = i64::from(base) - g.metrics.bounds.y - i64::from(g.bitmap.height());
    if top < 0 || overlap > 63 {
        return Err(invalid("SSFN bearing outside native overlap/grid"));
    }
    let w = u8::try_from(left + i64::from(g.bitmap.width()))
        .map_err(|_| invalid("SSFN glyph width exceeds byte"))?;
    let h = u8::try_from(top + i64::from(g.bitmap.height()))
        .map_err(|_| invalid("SSFN glyph height exceeds byte"))?;
    let mut v = alloc(area(u32::from(w), u32::from(h), l)?, budget)?;
    for y in 0..g.bitmap.height() {
        for x in 0..g.bitmap.width() {
            v[(y as usize + top as usize) * usize::from(w) + x as usize + left as usize] =
                match &g.bitmap {
                    GlyphBitmap::Mono(b) => {
                        if b.get_pixel(x, y).unwrap_or(0) == 0 {
                            0
                        } else {
                            255
                        }
                    }
                    GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap_or(0),
                };
        }
    }
    Ok((w, h, overlap as u8, v))
}
fn skip(v: &mut Vec<u8>, mut n: u32, budget: &mut Budget) -> Result<()> {
    while n > 0 {
        if n >= 65536 {
            append(v, &[255], budget)?;
            n -= 65536;
        } else if n <= 64 {
            append(v, &[128 + (n - 1) as u8], budget)?;
            n = 0;
        } else {
            let count = n.min(16128);
            let value = count - 1;
            append(v, &[192 + ((value >> 8) as u8), value as u8], budget)?;
            n -= count;
        }
    }
    Ok(())
}
pub(super) fn encode(
    s: &FontSet,
    p: &SsfnWriteOptions,
    budget: &mut Budget,
    _r: &mut Report,
) -> Result<Vec<u8>> {
    if s.fonts.is_empty() {
        return Err(invalid("SSFN requires a font"));
    }
    let mut out = Vec::new();
    if s.fonts.len() > 1 {
        append(&mut out, b"SFNC\0\0\0\0", budget)?;
    }
    for f in &s.fonts {
        let b = encode_font(f, p, budget)?;
        append(&mut out, &b, budget)?;
    }
    if s.fonts.len() > 1 {
        let n = u32::try_from(out.len()).map_err(|_| invalid("SSFN collection size"))?;
        put(&mut out, 4, 4, n, true)?;
    }
    check(
        out.len(),
        p.max_output_bytes
            .min(p.limits.max_input_bytes)
            .min(p.limits.max_table_bytes),
        "SSFN output bytes",
    )?;
    Ok(out)
}
fn encode_font(f: &Font, p: &SsfnWriteOptions, budget: &mut Budget) -> Result<Vec<u8>> {
    let (w, h, baseline, underline, ty) = grid(f)?;
    let code = codes(f, p, budget)?;
    let mut out = alloc(32, budget)?;
    out[..4].copy_from_slice(b"SFN2");
    out[8] = ty;
    out[10] = w;
    out[11] = h;
    out[12] = baseline;
    out[13] = underline;
    for s in names(f) {
        text(s.as_bytes())?;
        append(&mut out, s.as_bytes(), budget)?;
        append(&mut out, &[0], budget)?;
    }
    let mut ligs = BTreeMap::new();
    for (&cp, g) in &code {
        if let Some(seq) = g.unicode_sequences.first() {
            if !(0xf000..=0xf8ff).contains(&cp) {
                continue;
            }
            let mut string = String::new();
            budget.add(mul(seq.len(), 4)?)?;
            for &v in seq {
                string.push(char::from_u32(v).ok_or_else(|| invalid("SSFN ligature scalar"))?);
            }
            text(string.as_bytes())?;
            let offset = u16::try_from(out.len())
                .map_err(|_| invalid("SSFN string table exceeds 16 bits"))?;
            budget.items::<(u32, u16)>(1)?;
            ligs.insert(cp, offset);
            append(&mut out, string.as_bytes(), budget)?;
            append(&mut out, &[0], budget)?;
        }
    }
    let frag =
        u16::try_from(out.len()).map_err(|_| invalid("SSFN fragment offset exceeds 16 bits"))?;
    put(&mut out, 14, 2, u32::from(frag), true)?;
    let mut chars = Vec::new();
    let mut previous = 0u32;
    let mut any_gray = false;
    for (&cp, g) in &code {
        skip(&mut chars, cp - previous, budget)?;
        let (gw, gh, overlap, pixels) = canvas(g, baseline, &p.limits, budget)?;
        let adv = g.metrics.advance.unwrap_or(Vector::new(i64::from(gw), 0));
        let ax =
            u8::try_from(adv.x).map_err(|_| invalid("SSFN horizontal advance exceeds byte"))?;
        let ay = u8::try_from(-adv.y).map_err(|_| invalid("SSFN vertical advance exceeds byte"))?;
        let gray = matches!(g.bitmap, GlyphBitmap::Gray8(_));
        any_gray |= gray;
        let mut descriptors = Vec::new();
        if gw != 0 && gh != 0 {
            if gray {
                let rows = (7900 / usize::from(gw)).min(256);
                let mut y = 0usize;
                while y < usize::from(gh) {
                    let height = rows.min(usize::from(gh) - y);
                    let values = &pixels[y * usize::from(gw)..(y + height) * usize::from(gw)];
                    let size = values.len() + values.len().div_ceil(128);
                    let n = size - 1;
                    let offset = out.len();
                    append(
                        &mut out,
                        &[0xa0 | ((n >> 8) as u8), n as u8, gw - 1, (height - 1) as u8],
                        budget,
                    )?;
                    for chunk in values.chunks(128) {
                        append(&mut out, &[(chunk.len() - 1) as u8], budget)?;
                        for &v in chunk {
                            append(
                                &mut out,
                                &[match v {
                                    0 => 255,
                                    255 => 254,
                                    v => v - 1,
                                }],
                                budget,
                            )?;
                        }
                    }
                    append(&mut descriptors, &[0, y as u8], budget)?;
                    let mut off = [0; 4];
                    put(
                        &mut off,
                        0,
                        4,
                        u32::try_from(offset).map_err(|_| invalid("SSFN fragment offset"))?,
                        true,
                    )?;
                    append(&mut descriptors, &off, budget)?;
                    y += height;
                }
            } else {
                let pitch = usize::from(gw).div_ceil(8);
                let offset = out.len();
                append(&mut out, &[0x80 | (pitch - 1) as u8, gh - 1], budget)?;
                let mut packed = alloc(pitch * usize::from(gh), budget)?;
                for y in 0..usize::from(gh) {
                    for x in 0..usize::from(gw) {
                        if pixels[y * usize::from(gw) + x] != 0 {
                            packed[y * pitch + x / 8] |= 1 << (x % 8);
                        }
                    }
                }
                append(&mut out, &packed, budget)?;
                append(&mut descriptors, &[0, 0], budget)?;
                let mut off = [0; 4];
                put(
                    &mut off,
                    0,
                    4,
                    u32::try_from(offset).map_err(|_| invalid("SSFN fragment offset"))?,
                    true,
                )?;
                append(&mut descriptors, &off, budget)?;
            }
        }
        append(
            &mut chars,
            &[64 | overlap, (descriptors.len() / 6) as u8, gw, gh, ax, ay],
            budget,
        )?;
        append(&mut chars, &descriptors, budget)?;
        previous = cp + 1;
    }
    skip(&mut chars, 0x110000 - previous, budget)?;
    let off = u32::try_from(out.len()).map_err(|_| invalid("SSFN char offset"))?;
    put(&mut out, 16, 4, off, true)?;
    append(&mut out, &chars, budget)?;
    if !ligs.is_empty() {
        let last = *ligs
            .keys()
            .next_back()
            .ok_or_else(|| invalid("SSFN ligatures"))?;
        if (0xf000..=last).any(|cp| !ligs.contains_key(&cp)) {
            return Err(invalid(
                "SSFN ligature positions must form a consecutive sequence from U+F000",
            ));
        }
        let off = out.len() as u32;
        put(&mut out, 20, 4, off, true)?;
        for (_, offset) in ligs {
            append(&mut out, &offset.to_le_bytes(), budget)?;
        }
        append(&mut out, &[0, 0], budget)?;
    }
    if any_gray {
        let off = out.len() as u32;
        put(&mut out, 28, 4, off, true)?;
        for alpha in 1..=254u8 {
            append(&mut out, &[255, 255, 255, alpha], budget)?;
        }
    }
    append(&mut out, b"2NFS", budget)?;
    let n = u32::try_from(out.len()).map_err(|_| invalid("SSFN size exceeds 32 bits"))?;
    put(&mut out, 4, 4, n, true)?;
    Ok(out)
}
