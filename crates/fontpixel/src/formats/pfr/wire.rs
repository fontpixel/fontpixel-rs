use super::{PfrCompression, PfrDialect, PfrWriteOptions, c};
use crate::bitmap::{Bitmap, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, PropertyValue as V, Vector};
use crate::{Report, ResourceLimits, Result};
use c::{
    Budget, Cursor, add, alloc, append, area, blob, bytes, check, invalid, mul, number, property,
    put, scalar,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) struct Header {
    dir: usize,
    dir_size: usize,
    log: (usize, usize),
    phy: (usize, usize),
    gps: (usize, usize),
    flags: u8,
    high: bool,
}
fn within(span: (usize, usize), off: usize, n: usize) -> Result<()> {
    if off < span.0 || add(off, n)? > add(span.0, span.1)? {
        Err(invalid("PFR record outside its section"))
    } else {
        Ok(())
    }
}
pub(super) fn header(b: &[u8]) -> Result<Header> {
    if b.len() < 66
        || bytes(b, 0, 10)? != b"PFR0\0\x04\r\n\0\x3a"
        || bytes(b, b.len() - 5, 5)? != b"$PFR$"
        || number(b, b.len() - 8, 3, false)? as usize != b.len()
    {
        return Err(invalid("invalid PFR signature/version/trailer"));
    }
    let span = |p| -> Result<(usize, usize)> {
        let n = number(b, p, 3, false)? as usize;
        let o = number(b, p + 3, 3, false)? as usize;
        if o < 58 || add(o, n)? > b.len() - 8 {
            return Err(invalid("PFR section bounds"));
        }
        Ok((o, n))
    };
    let h = Header {
        dir: number(b, 12, 2, false)? as usize,
        dir_size: number(b, 10, 2, false)? as usize,
        log: span(16)?,
        phy: span(24)?,
        gps: span(32)?,
        flags: b[42],
        high: b[41] != 0,
    };
    if h.flags & !3 != 0
        || h.dir < 58
        || add(h.dir, h.dir_size)? > h.log.0
        || add(h.log.0, h.log.1)? > h.phy.0
        || add(h.phy.0, h.phy.1)? > h.gps.0
        || add(h.gps.0, h.gps.1)? > b.len() - 8
    {
        return Err(invalid("PFR sections overlap"));
    }
    let n = number(b, h.dir, 2, false)? as usize;
    if n == 0 || h.dir_size != add(2, mul(n, 5)?)? {
        return Err(invalid("PFR logical directory size"));
    }
    for i in 0..n {
        let p = h.dir + 2 + i * 5;
        within(
            h.log,
            number(b, p + 2, 3, false)? as usize,
            number(b, p, 2, false)? as usize,
        )?;
    }
    Ok(h)
}
fn logical(b: &[u8], h: Header, i: usize) -> Result<(&[u8], usize, usize)> {
    let p = h.dir + 2 + i * 5;
    let data = bytes(
        b,
        number(b, p + 2, 3, false)? as usize,
        number(b, p, 2, false)? as usize,
    )?;
    let mut r = Cursor::new(data, false);
    r.take(12)?;
    let flags = r.u(1)?;
    if flags & 128 != 0 {
        return Err(invalid("PFR logical flags"));
    }
    if flags & 4 != 0 {
        r.take(if flags & 8 != 0 { 2 } else { 1 })?;
        if flags & 3 == 0 {
            r.take(3)?;
        }
    } else if flags & 16 != 0 {
        r.take(if flags & 32 != 0 { 2 } else { 1 })?;
    }
    if flags & 64 != 0 {
        let n = r.u(1)?;
        for _ in 0..n {
            let size = r.u(1)? as usize;
            r.take(1)?;
            r.take(size)?;
        }
    }
    let prefix = &data[..r.at];
    let mut size = r.u(2)? as usize;
    let off = r.u(3)? as usize;
    if h.high {
        size |= (r.u(1)? as usize) << 16;
    }
    r.end()?;
    within(h.phy, off, size)?;
    Ok((prefix, off, size))
}
#[derive(Clone, Copy)]
struct Strike {
    dialect: PfrDialect,
    x: u32,
    y: u32,
    flags: u32,
    off: usize,
    size: usize,
    count: usize,
}
#[derive(Clone, Copy)]
struct Char {
    code: u32,
    width: i32,
    ascii: Option<u32>,
}
struct Physical<'a> {
    base: &'a [u8],
    extras: Vec<(u8, &'a [u8])>,
    tail: &'a [u8],
    chars: Vec<Char>,
    strikes: Vec<Strike>,
    vertical: bool,
    family: Option<String>,
}
#[allow(clippy::too_many_arguments)] // Native section context, dialect, limits and diagnostics are explicit.
fn physical<'a>(
    data: &'a [u8],
    all: &[u8],
    h: Header,
    off: usize,
    budget: &mut Budget,
    rpt: &mut Report,
    fi: usize,
    l: &ResourceLimits,
    dialect: Option<PfrDialect>,
) -> Result<Physical<'a>> {
    let mut r = Cursor::new(data, false);
    let base = r.take(14)?;
    if number(base, 2, 2, false)? == 0 || number(base, 4, 2, false)? == 0 {
        return Err(invalid("PFR zero coordinate resolution"));
    }
    let flags = r.u(1)?;
    if flags & 64 != 0 {
        return Err(invalid("PFR physical reserved flags"));
    }
    let standard = if flags & 4 == 0 { r.i(2)? } else { 0 };
    let mut extras = Vec::new();
    let mut strikes = Vec::new();
    let mut family = None;
    let mut bct_total = None;
    if flags & 128 != 0 {
        let count = r.u(1)?;
        budget.items::<(u8, &[u8])>(count as usize)?;
        for _ in 0..count {
            let size = r.u(1)? as usize;
            let kind = r.u(1)? as u8;
            let value = r.take(size)?;
            if kind == 1 {
                let mut e = Cursor::new(value, false);
                let total = e.u(3)? as usize;
                if bct_total.is_some_and(|old| old != total) {
                    return Err(invalid("PFR inconsistent BCT total"));
                }
                bct_total = Some(total);
                let f = e.u(1)?;
                if f & !31 != 0 {
                    return Err(invalid("PFR bitmap-info flags"));
                }
                let count = e.u(1)? as usize;
                check(add(strikes.len(), count)?, l.max_resources, "PFR strikes")?;
                budget.items::<Strike>(count)?;
                let mut last = None;
                for _ in 0..count {
                    let x = e.u(1 + usize::from(f & 1 != 0))?;
                    let y = e.u(1 + usize::from(f & 2 != 0))?;
                    let sf = e.u(1)?;
                    let size = e.u(2 + usize::from(f & 4 != 0))? as usize;
                    let rel = e.u(2 + usize::from(f & 8 != 0))? as usize;
                    let count = e.u(1 + usize::from(f & 16 != 0))? as usize;
                    if x == 0 || y == 0 || sf & !7 != 0 || last.is_some_and(|v| v >= (y, x)) {
                        return Err(invalid("PFR invalid strike order/flags/ppem"));
                    }
                    last = Some((y, x));
                    let dialect = dialect.unwrap_or(if rel < data.len() {
                        PfrDialect::FreeType
                    } else {
                        PfrDialect::Davic
                    });
                    let bo = add(
                        add(
                            off,
                            if dialect == PfrDialect::FreeType {
                                data.len()
                            } else {
                                0
                            },
                        )?,
                        rel,
                    )?;
                    within(h.phy, bo, size)?;
                    if bo < off + data.len() {
                        return Err(invalid("PFR BCT overlaps physical record"));
                    }
                    let record = 1
                        + usize::from(sf & 1 != 0)
                        + 1
                        + usize::from(sf & 2 != 0)
                        + 2
                        + usize::from(sf & 4 != 0);
                    if size != mul(count, record)? {
                        return Err(invalid("PFR BCT size mismatch"));
                    }
                    strikes.push(Strike {
                        dialect,
                        x,
                        y,
                        flags: sf,
                        off: bo,
                        size,
                        count,
                    });
                }
                e.end()?;
            } else {
                if kind == 2 {
                    if value.len() < 2
                        || value.last() != Some(&0)
                        || value[..value.len() - 1].contains(&0)
                    {
                        return Err(invalid("PFR invalid font ID"));
                    }
                    budget.add(mul(value.len(), 2)?)?;
                    family = Some(
                        value[..value.len() - 1]
                            .iter()
                            .map(|&v| char::from(v))
                            .collect(),
                    );
                }
                extras.push((kind, value));
            }
        }
    }
    if bct_total != Some(strikes.iter().try_fold(0usize, |n, s| add(n, s.size))?)
        && !strikes.is_empty()
    {
        return Err(invalid("PFR BCT total mismatch"));
    }
    let tail_start = r.at;
    let aux = r.u(3)? as usize;
    r.take(aux)?;
    let blue = r.u(1)? as usize;
    r.take(add(mul(blue, 2)?, 6)?)?;
    let tail = &data[tail_start..r.at];
    let n = r.u(2)? as usize;
    check(n, l.max_glyphs, "PFR character count")?;
    budget.items::<Char>(n)?;
    let mut chars = Vec::with_capacity(n);
    let mut previous = None;
    let mut outlines = 0usize;
    for _ in 0..n {
        let code = r.u(1 + usize::from(flags & 2 != 0))?;
        if previous.is_some_and(|p| p >= code) {
            return Err(invalid("PFR character table is not strictly sorted"));
        }
        previous = Some(code);
        let width = if flags & 4 != 0 { r.i(2)? } else { standard };
        let ascii = if flags & 8 != 0 { Some(r.u(1)?) } else { None };
        let size = r.u(1 + usize::from(flags & 16 != 0))? as usize;
        let offset = r.u(2 + usize::from(flags & 32 != 0))? as usize;
        bytes(all, add(h.gps.0, offset)?, size)?;
        within(h.gps, add(h.gps.0, offset)?, size)?;
        outlines += usize::from(size != 0);
        chars.push(Char { code, width, ascii });
    }
    r.end()?;
    if outlines != 0 {
        budget.loss(
            rpt,
            "pfr",
            "outlines-omitted",
            Some(fi),
            None,
            format!("{outlines} outline glyph programs are not bitmap strikes"),
        )?;
    }
    Ok(Physical {
        base,
        extras,
        tail,
        chars,
        strikes,
        vertical: flags & 1 != 0,
        family,
    })
}
fn metadata(p: &Physical<'_>, budget: &mut Budget) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    append(&mut v, p.base, budget)?;
    append(
        &mut v,
        &[u8::from(p.vertical), p.extras.len() as u8],
        budget,
    )?;
    for &(kind, b) in &p.extras {
        append(&mut v, &[b.len() as u8, kind], budget)?;
        append(&mut v, b, budget)?;
    }
    append(&mut v, p.tail, budget)?;
    Ok(v)
}
pub(super) fn decode(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
    rpt: &mut Report,
) -> Result<FontSet> {
    decode_with(b, l, budget, rpt, None)
}
pub(super) fn decode_with(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
    rpt: &mut Report,
    dialect: Option<PfrDialect>,
) -> Result<FontSet> {
    let h = header(b)?;
    let n = number(b, h.dir, 2, false)? as usize;
    check(n, l.max_resources, "PFR logical fonts")?;
    let mut set = FontSet {
        source_format: Some("pfr".into()),
        ..Default::default()
    };
    let mut total_glyphs = 0usize;
    for logical_index in 0..n {
        let (prefix, off, size) = logical(b, h, logical_index)?;
        let phy = physical(
            bytes(b, off, size)?,
            b,
            h,
            off,
            budget,
            rpt,
            logical_index,
            l,
            dialect,
        )?;
        if phy.strikes.is_empty() {
            budget.loss(
                rpt,
                "pfr",
                "no-bitmap-strike",
                Some(logical_index),
                None,
                "Logical font has no embedded bitmap strike",
            )?;
            continue;
        }
        for st in &phy.strikes {
            check(
                add(set.fonts.len(), 1)?,
                l.max_resources,
                "PFR font strikes",
            )?;
            total_glyphs = add(total_glyphs, st.count)?;
            check(total_glyphs, l.max_glyphs, "PFR total glyphs")?;
            budget.items::<Font>(1)?;
            budget.add(512)?;
            let mut font = Font::default();
            font.source_format = Some("pfr".into());
            budget.add(phy.family.as_ref().map_or(0, String::len))?;
            font.names.family = phy.family.clone();
            font.metrics.pixel_size = Some(st.y);
            property(
                &mut font.properties,
                "pfr.dialect",
                V::Integer(i64::from(st.dialect == PfrDialect::FreeType)),
                budget,
            )?;
            property(
                &mut font.properties,
                "pfr.x-ppem",
                V::Integer(i64::from(st.x)),
                budget,
            )?;
            let meta = metadata(&phy, budget)?;
            property(
                &mut font.properties,
                "pfr.physical-metadata",
                V::Bytes(meta),
                budget,
            )?;
            budget.add(prefix.len())?;
            property(
                &mut font.properties,
                "pfr.logical-metadata",
                V::Bytes(prefix.to_vec()),
                budget,
            )?;
            let mut r = Cursor::new(bytes(b, st.off, st.size)?, false);
            let mut prev = None;
            for gi in 0..st.count {
                let code = r.u(1 + usize::from(st.flags & 1 != 0))?;
                if prev.is_some_and(|p| p >= code) {
                    return Err(invalid("PFR BCT not strictly sorted"));
                }
                prev = Some(code);
                let size = r.u(1 + usize::from(st.flags & 2 != 0))? as usize;
                let off = r.u(2 + usize::from(st.flags & 4 != 0))? as usize;
                within(h.gps, add(h.gps.0, off)?, size)?;
                let ch = phy
                    .chars
                    .binary_search_by_key(&code, |c| c.code)
                    .ok()
                    .map(|i| phy.chars[i])
                    .ok_or_else(|| {
                        invalid("PFR bitmap code absent from physical character table")
                    })?;
                budget.items::<Option<Glyph>>(1)?;
                budget.add(512)?;
                let mut g = glyph(
                    bytes(b, add(h.gps.0, off)?, size)?,
                    h.flags,
                    *st,
                    ch,
                    number(phy.base, 4, 2, false)?,
                    phy.vertical,
                    l,
                    budget,
                    rpt,
                    (set.fonts.len(), gi),
                )?;
                g.codepoints.push(Codepoint::Legacy(code));
                property(
                    &mut g.properties,
                    "pfr.outline-advance",
                    V::Integer(i64::from(ch.width)),
                    budget,
                )?;
                if let Some(v) = ch.ascii {
                    property(
                        &mut g.properties,
                        "pfr.ascii",
                        V::Integer(i64::from(v)),
                        budget,
                    )?;
                }
                font.add_glyph(g)?;
            }
            r.end()?;
            set.fonts.push(font);
        }
    }
    if set.fonts.is_empty() {
        return Err(invalid("PFR has no embedded bitmap strikes"));
    }
    Ok(set)
}
#[allow(clippy::too_many_arguments)] // Wire context, limits, budget and diagnostic coordinates are independent.
fn glyph(
    b: &[u8],
    flags: u8,
    st: Strike,
    ch: Char,
    res: u32,
    vertical: bool,
    l: &ResourceLimits,
    budget: &mut Budget,
    rpt: &mut Report,
    loc: (usize, usize),
) -> Result<Glyph> {
    let mut r = Cursor::new(b, false);
    let head = r.u(1)?;
    if head >> 6 == 3 {
        return Err(invalid("PFR invalid image format"));
    }
    let (x, y) = match head & 3 {
        0 => {
            let v = r.u(1)? as u8;
            (
                (i32::from(v as i8) >> 4) * 256,
                ((i32::from(v & 15) << 28) >> 28) * 256,
            )
        }
        1 => (r.i(1)? * 256, r.i(1)? * 256),
        n => {
            let scale = if st.dialect == PfrDialect::FreeType {
                256
            } else {
                1
            };
            (r.i(n as usize)? * scale, r.i(n as usize)? * scale)
        }
    };
    let (w, h) = match (head >> 2) & 3 {
        0 => (0, 0),
        1 => {
            let v = r.u(1)?;
            (v >> 4, v & 15)
        }
        n => (r.u((n - 1) as usize)?, r.u((n - 1) as usize)?),
    };
    let adv = match (head >> 4) & 3 {
        0 => {
            i64::from(ch.width) * i64::from(if vertical { st.y } else { st.x }) * 256
                / i64::from(res)
        }
        1 => i64::from(r.i(1)?) * 256,
        n => i64::from(r.i(n as usize)?),
    };
    let n = area(w, h, l)?;
    let pitch = (w as usize).div_ceil(8);
    let mut out = alloc(mul(pitch, h as usize)?, budget)?;
    let mut pixel = 0usize;
    let set = |start: usize, count: usize, out: &mut [u8]| -> Result<()> {
        let end = add(start, count)?;
        if end > n {
            return Err(invalid("PFR run exceeds bitmap"));
        }
        for p in start..end {
            let row = p / w as usize;
            let row = if flags & 2 != 0 {
                row
            } else {
                h as usize - 1 - row
            };
            out[row * pitch + (p % w as usize) / 8] |= 0x80 >> (p % w as usize % 8);
        }
        Ok(())
    };
    if head >> 6 == 0 {
        let data = r.take(n.div_ceil(8))?;
        for p in 0..n {
            if ((data[p / 8] >> (7 - p % 8)) & 1 != 0)
                == (st.dialect == PfrDialect::FreeType || flags & 1 != 0)
            {
                set(p, 1, &mut out)?;
            }
        }
    } else {
        while pixel < n {
            let (white, black) = if head >> 6 == 1 {
                let v = r.u(1)? as usize;
                (v >> 4, v & 15)
            } else {
                (r.u(1)? as usize, r.u(1)? as usize)
            };
            if white + black == 0 {
                return Err(invalid("PFR empty run pair"));
            }
            pixel = add(pixel, white)?;
            set(pixel, black, &mut out)?;
            pixel = add(pixel, black)?;
        }
    }
    r.end()?;
    budget.add(out.len())?;
    let bitmap = Bitmap::<Mono>::from_raw(w, h, &out, c::bitmap_limits(l))?;
    let mut g = Glyph::new(bitmap);
    g.metrics.bounds.x = i64::from(x).div_euclid(256);
    let ypos = i64::from(y).div_euclid(256);
    g.metrics.bounds.y = if st.dialect == PfrDialect::Davic && flags & 2 != 0 && h != 0 {
        ypos - i64::from(h) + 1
    } else {
        ypos
    };
    let a = adv.div_euclid(256);
    g.metrics.advance = Some(if vertical {
        Vector::new(0, a)
    } else {
        Vector::new(a, 0)
    });
    if x % 256 != 0 || y % 256 != 0 || adv % 256 != 0 {
        property(
            &mut g.properties,
            "pfr.fractional-metrics",
            V::List(vec![
                V::Integer(i64::from(x)),
                V::Integer(i64::from(y)),
                V::Integer(adv),
            ]),
            budget,
        )?;
        budget.loss(
            rpt,
            "pfr",
            "fractional-metrics",
            Some(loc.0),
            Some(loc.1),
            "Native 1/256-pixel metrics retained; integer bounds and advance use floor",
        )?;
    }
    Ok(g)
}
fn emit(v: &mut Vec<u8>, n: usize, x: i64, budget: &mut Budget) -> Result<()> {
    let signed_min = -(1i64 << (n * 8 - 1));
    let unsigned_max = (1i64 << (n * 8)) - 1;
    if x < signed_min || x > unsigned_max {
        return Err(invalid("PFR scalar field overflow"));
    }
    let value = (x as u64) & ((1u64 << (n * 8)) - 1);
    let mut b = [0u8; 4];
    put(&mut b[..n], 0, n, value as u32, false)?;
    append(v, &b[..n], budget)
}
fn signed(x: i64, n: usize) -> Result<i64> {
    if x < -(1i64 << (n * 8 - 1)) || x >= (1i64 << (n * 8 - 1)) {
        Err(invalid("PFR signed metric overflow"))
    } else {
        Ok(x)
    }
}
fn program(
    g: &Glyph,
    p: &PfrWriteOptions,
    vertical: bool,
    dialect: PfrDialect,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let GlyphBitmap::Mono(b) = &g.bitmap else {
        return Err(invalid("PFR requires explicit grayscale threshold"));
    };
    let w = b.width();
    let h = b.height();
    if w > 65535 || h > 65535 {
        return Err(invalid("PFR dimensions exceed 16 bits"));
    }
    let a = g.metrics.advance.unwrap_or(Vector::new(i64::from(w), 0));
    if (vertical && a.x != 0) || (!vertical && a.y != 0) {
        return Err(invalid("PFR advance must follow its font direction"));
    }
    let scale = if dialect == PfrDialect::Davic { 256 } else { 1 };
    let x = signed(g.metrics.bounds.x * scale, 3)?;
    let y = signed(
        (g.metrics.bounds.y
            + if dialect == PfrDialect::Davic && p.top_down && h != 0 {
                i64::from(h) - 1
            } else {
                0
            })
            * scale,
        3,
    )?;
    let adv = signed(if vertical { a.y } else { a.x } * 256, 3)?;
    let mut v = Vec::new();
    append(
        &mut v,
        &[(match p.compression {
            PfrCompression::Raw => 0,
            PfrCompression::Rle4 => 1,
            PfrCompression::Rle8 => 2,
        } << 6)
            | 0x3f],
        budget,
    )?;
    emit(&mut v, 3, x, budget)?;
    emit(&mut v, 3, y, budget)?;
    emit(&mut v, 2, i64::from(w), budget)?;
    emit(&mut v, 2, i64::from(h), budget)?;
    emit(&mut v, 3, adv, budget)?;
    let n = mul(w as usize, h as usize)?;
    let ink = |i: usize| -> bool {
        let y = i / w as usize;
        let y = if p.top_down { y } else { h as usize - 1 - y };
        b.get_pixel((i % w as usize) as u32, y as u32).unwrap_or(0) != 0
    };
    if p.compression == PfrCompression::Raw {
        let mut raw = alloc(n.div_ceil(8), budget)?;
        for i in 0..n {
            if ink(i) == (dialect == PfrDialect::FreeType || p.black_pixel) {
                raw[i / 8] |= 0x80 >> (i % 8)
            }
        }
        append(&mut v, &raw, budget)?;
    } else {
        let max = if p.compression == PfrCompression::Rle4 {
            15
        } else {
            255
        };
        let mut pos = 0;
        while pos < n {
            let mut white = 0;
            while pos < n && !ink(pos) && white < max {
                white += 1;
                pos += 1;
            }
            let mut black = 0;
            if white < max || pos == n || ink(pos) {
                while pos < n && ink(pos) && black < max {
                    black += 1;
                    pos += 1;
                }
            }
            if p.compression == PfrCompression::Rle4 {
                append(&mut v, &[((white << 4) | black) as u8], budget)?
            } else {
                append(&mut v, &[white as u8, black as u8], budget)?
            }
        }
    }
    check(v.len(), 65535, "PFR bitmap glyph program")?;
    Ok(v)
}
struct Encoded {
    logical: Vec<u8>,
    physical: Vec<u8>,
    bct: Vec<u8>,
    count: usize,
}
pub(super) fn encode(
    source: &FontSet,
    p: &PfrWriteOptions,
    budget: &mut Budget,
    _report: &mut Report,
) -> Result<Vec<u8>> {
    let converted = c::convert_gray(source, p.gray_threshold, &p.limits, budget)?;
    let s = converted.as_ref();
    let count = s.fonts.len();
    if count == 0 {
        return Err(invalid("PFR requires at least one font"));
    }
    check(count, 65535, "PFR logical count")?;
    let mut gps = Vec::new();
    budget.items::<Encoded>(count)?;
    let mut records = Vec::with_capacity(count);
    let mut max_gps = 0;
    for (fi, f) in s.fonts.iter().enumerate() {
        let dialect = p
            .dialect
            .unwrap_or(if scalar(&f.properties, "pfr.dialect", 1)? == 0 {
                PfrDialect::Davic
            } else {
                PfrDialect::FreeType
            });
        let y = f
            .metrics
            .pixel_size
            .or(f.metrics.bounds.map(|b| b.height))
            .or_else(|| f.glyphs().map(|(_, g)| g.bitmap.height()).max())
            .unwrap_or(0);
        let x = scalar(&f.properties, "pfr.x-ppem", i64::from(y))?;
        if y == 0 || y > 65535 || x <= 0 || x > 65535 {
            return Err(invalid("PFR ppem outside 1..65535"));
        }
        let mut logical = Vec::new();
        if let Some(v) = blob(&f.properties, "pfr.logical-metadata")? {
            append(&mut logical, v, budget)?;
        } else {
            append(
                &mut logical,
                &[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0],
                budget,
            )?;
        }
        let mut base = [0u8; 14];
        put(&mut base, 2, 2, y, false)?;
        put(&mut base, 4, 2, y, false)?;
        put(&mut base, 10, 2, x as u32, false)?;
        put(&mut base, 12, 2, y, false)?;
        let mut vertical = false;
        let mut extras: Vec<(u8, &[u8])> = Vec::new();
        let default_tail = [0u8; 10];
        let mut tail = &default_tail[..];
        if let Some(v) = blob(&f.properties, "pfr.physical-metadata")? {
            let mut r = Cursor::new(v, false);
            base.copy_from_slice(r.take(14)?);
            let flag = r.u(1)?;
            if flag > 1 {
                return Err(invalid("PFR native direction flag"));
            }
            vertical = flag != 0;
            let n = r.u(1)? as usize;
            budget.items::<(u8, &[u8])>(n)?;
            for _ in 0..n {
                let len = r.u(1)? as usize;
                let kind = r.u(1)? as u8;
                let b = r.take(len)?;
                if kind == 1 {
                    return Err(invalid("PFR metadata contains stale bitmap offsets"));
                }
                if kind != 2 {
                    extras.push((kind, b));
                }
            }
            tail = &v[r.at..];
            let mut t = Cursor::new(tail, false);
            let aux = t.u(3)? as usize;
            t.take(aux)?;
            let n = t.u(1)? as usize;
            t.take(n * 2 + 6)?;
            t.end()?;
        }
        put(&mut base, 0, 2, fi as u32, false)?;
        let name = f.names.family.as_deref().unwrap_or("Fontpixel");
        let mut name_bytes = Vec::new();
        budget.add(mul(name.len() + 1, 2)?)?;
        for ch in name.chars() {
            let v =
                u8::try_from(ch as u32).map_err(|_| invalid("PFR font ID is an 8-bit string"))?;
            if v == 0 {
                return Err(invalid("PFR font ID contains NUL"));
            }
            name_bytes.push(v);
        }
        name_bytes.push(0);
        if name_bytes.len() < 2 || name_bytes.len() > 255 {
            return Err(invalid("PFR font ID size"));
        }
        extras.push((2, &name_bytes));
        if extras.len() > 254 {
            return Err(invalid("PFR too many extra records"));
        }
        let mut codes = BTreeMap::new();
        budget.items::<(u32, &Glyph)>(f.len())?;
        for (_, g) in f.glyphs() {
            let cp = g
                .codepoints
                .first()
                .ok_or_else(|| invalid("PFR needs encoded glyphs"))?;
            if matches!(cp, Codepoint::Unicode(_)) && !p.unicode_as_legacy {
                return Err(invalid("PFR Unicode-to-legacy conversion must be explicit"));
            }
            let cp = cp.value();
            if cp > 65535 || codes.insert(cp, g).is_some() {
                return Err(invalid("PFR duplicate or oversized code"));
            }
        }
        check(codes.len(), 65535, "PFR character count")?;
        let ascii = codes
            .values()
            .any(|g| g.properties.get("pfr.ascii").is_some());
        let mut chars = Vec::new();
        let mut bct = Vec::new();
        for (&cp, &g) in &codes {
            let program = program(g, p, vertical, dialect, budget)?;
            max_gps = max_gps.max(program.len());
            emit(&mut bct, 2, i64::from(cp), budget)?;
            emit(&mut bct, 2, program.len() as i64, budget)?;
            emit(&mut bct, 3, gps.len() as i64, budget)?;
            append(&mut gps, &program, budget)?;
            emit(&mut chars, 2, i64::from(cp), budget)?;
            let a = g.metrics.advance.unwrap_or_default();
            emit(
                &mut chars,
                2,
                signed(
                    scalar(
                        &g.properties,
                        "pfr.outline-advance",
                        if vertical { a.y } else { a.x },
                    )?,
                    2,
                )?,
                budget,
            )?;
            if ascii {
                emit(
                    &mut chars,
                    1,
                    scalar(&g.properties, "pfr.ascii", 0)?,
                    budget,
                )?;
            }
            append(&mut chars, &[0, 0, 0, 0, 0], budget)?;
        }
        let phy_size = 14
            + 1
            + 1
            + 2
            + 18
            + extras.iter().map(|(_, v)| 2 + v.len()).sum::<usize>()
            + tail.len()
            + 2
            + chars.len();
        check(phy_size, 0xffffff, "PFR physical record bytes")?;
        let mut physical = Vec::new();
        append(&mut physical, &base, budget)?;
        append(
            &mut physical,
            &[
                0xb6 | u8::from(vertical) | if ascii { 8 } else { 0 },
                (extras.len() + 1) as u8,
                18,
                1,
            ],
            budget,
        )?;
        emit(&mut physical, 3, bct.len() as i64, budget)?;
        append(&mut physical, &[31, 1], budget)?;
        emit(&mut physical, 2, x, budget)?;
        emit(&mut physical, 2, i64::from(y), budget)?;
        append(&mut physical, &[7], budget)?;
        emit(&mut physical, 3, bct.len() as i64, budget)?;
        emit(
            &mut physical,
            3,
            if dialect == PfrDialect::Davic {
                phy_size as i64
            } else {
                0
            },
            budget,
        )?;
        emit(&mut physical, 2, codes.len() as i64, budget)?;
        for (kind, v) in extras {
            append(&mut physical, &[v.len() as u8, kind], budget)?;
            append(&mut physical, v, budget)?;
        }
        append(&mut physical, tail, budget)?;
        emit(&mut physical, 2, codes.len() as i64, budget)?;
        append(&mut physical, &chars, budget)?;
        if physical.len() != phy_size {
            return Err(invalid("internal PFR physical size"));
        }
        records.push(Encoded {
            logical,
            physical,
            bct,
            count: codes.len(),
        });
    }
    let max_phy = records.iter().map(|r| r.physical.len()).max().unwrap_or(0);
    let high = max_phy > 65535;
    let log_tail = 5 + usize::from(high);
    let dir_size = 2 + count * 5;
    let log_off = 58 + dir_size;
    let log_size = records
        .iter()
        .try_fold(0usize, |n, r| add(n, add(r.logical.len(), log_tail)?))?;
    let phy_off = add(log_off, log_size)?;
    let phy_size = records
        .iter()
        .try_fold(0usize, |n, r| add(n, add(r.physical.len(), r.bct.len())?))?;
    let gps_off = add(phy_off, phy_size)?;
    let total = add(add(gps_off, gps.len())?, 8)?;
    check(
        total,
        p.max_output_bytes
            .min(p.limits.max_input_bytes)
            .min(p.limits.max_table_bytes)
            .min(0xffffff),
        "PFR output bytes",
    )?;
    check(dir_size, 65535, "PFR directory bytes")?;
    let mut out = alloc(total, budget)?;
    out[..10].copy_from_slice(b"PFR0\0\x04\r\n\0\x3a");
    put(&mut out, 10, 2, dir_size as u32, false)?;
    put(&mut out, 12, 2, 58, false)?;
    put(
        &mut out,
        14,
        2,
        records
            .iter()
            .map(|r| r.logical.len() + log_tail)
            .max()
            .unwrap_or(0) as u32,
        false,
    )?;
    for (at, n, off) in [
        (16, log_size, log_off),
        (24, phy_size, phy_off),
        (32, gps.len(), gps_off),
    ] {
        put(&mut out, at, 3, n as u32, false)?;
        put(&mut out, at + 3, 3, off as u32, false)?;
    }
    put(&mut out, 22, 2, (max_phy & 65535) as u32, false)?;
    out[41] = (max_phy >> 16) as u8;
    put(&mut out, 30, 2, max_gps as u32, false)?;
    out[42] = u8::from(p.black_pixel) | (u8::from(p.top_down) * 2);
    let max_bct = records.iter().map(|r| r.bct.len()).max().unwrap_or(0) as u32;
    for at in [43, 46, 49] {
        put(&mut out, at, 3, max_bct, false)?;
    }
    put(&mut out, 52, 2, count as u32, false)?;
    put(
        &mut out,
        56,
        2,
        records.iter().map(|r| r.count).max().unwrap_or(0) as u32,
        false,
    )?;
    put(&mut out, 58, 2, count as u32, false)?;
    let mut lo = log_off;
    let mut po = phy_off;
    for (i, r) in records.iter().enumerate() {
        put(
            &mut out,
            60 + i * 5,
            2,
            (r.logical.len() + log_tail) as u32,
            false,
        )?;
        put(&mut out, 62 + i * 5, 3, lo as u32, false)?;
        out[lo..lo + r.logical.len()].copy_from_slice(&r.logical);
        lo += r.logical.len();
        put(&mut out, lo, 2, (r.physical.len() & 65535) as u32, false)?;
        put(&mut out, lo + 2, 3, po as u32, false)?;
        if high {
            out[lo + 5] = (r.physical.len() >> 16) as u8;
        }
        lo += log_tail;
        out[po..po + r.physical.len()].copy_from_slice(&r.physical);
        po += r.physical.len();
        out[po..po + r.bct.len()].copy_from_slice(&r.bct);
        po += r.bct.len();
    }
    out[gps_off..gps_off + gps.len()].copy_from_slice(&gps);
    put(&mut out, total - 8, 3, total as u32, false)?;
    out[total - 5..].copy_from_slice(b"$PFR$");
    Ok(out)
}
