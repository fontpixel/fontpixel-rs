use super::embedded::*;
use super::*;
use std::fmt::Write as _;
fn selected_array<'a>(ds: &'a [Declaration<'a>], o: &CReadOptions) -> Result<&'a Declaration<'a>> {
    let mut iter = ds.iter().filter(|d| {
        matches!(d.ty, "uint8_t" | "u8g_fntpgm_uint8_t" | "u_char")
            && o.array_name.as_deref().is_none_or(|n| n == d.name)
    });
    let d = iter
        .next()
        .ok_or_else(|| invalid("missing legacy C font array"))?;
    if iter.next().is_some() {
        return Err(invalid("ambiguous legacy C font array"));
    }
    Ok(d)
}
fn charset(o: &CReadOptions) -> &str {
    o.generic.as_ref().map_or("unicode", |v| v.charset.as_str())
}
pub(super) fn read(
    t: &[Token<'_>],
    ds: &[Declaration<'_>],
    family: Family,
    o: &CReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Font> {
    if family == Family::Netbsd {
        return netbsd(t, ds, o, l, b);
    }
    let d = selected_array(ds, o)?;
    let src = blob(t, d, b)?;
    check(src.len(), l.max_table_bytes, "legacy C font bytes")?;
    if family == Family::U8x8 {
        return u8x8(&src, o, l, b);
    }
    let h = src
        .get(..17)
        .ok_or_else(|| invalid("truncated U8glib header"))?;
    if h[0] > 2 || h[10] > h[11] {
        return Err(invalid("unsupported U8glib format or code range"));
    }
    let mut f = Font::default();
    let mut at = 17;
    let mut positions = BTreeMap::new();
    for cp in u32::from(h[10])..=u32::from(h[11]) {
        positions.insert(cp, at);
        let first = *src
            .get(at)
            .ok_or_else(|| invalid("truncated U8glib glyph"))?;
        if first == 255 {
            at += 1;
            continue;
        }
        let size = if h[0] == 1 { 3 } else { 6 };
        let rec = src
            .get(at..at + size)
            .ok_or_else(|| invalid("truncated U8glib metrics"))?;
        let (w, ht, x, y, adv, len) = if h[0] == 1 {
            (
                u32::from(rec[1] >> 4),
                u32::from(rec[1] & 15),
                i64::from(rec[0] >> 4),
                i64::from(rec[0] & 15) - 2,
                i64::from(rec[2] >> 4),
                usize::from(rec[2] & 15),
            )
        } else {
            (
                u32::from(rec[0]),
                u32::from(rec[1]),
                i64::from(rec[4] as i8),
                i64::from(rec[5] as i8),
                i64::from(rec[3] as i8),
                usize::from(rec[2]),
            )
        };
        let bpp = if h[0] == 2 { 2 } else { 1 };
        let stride = (w as usize * bpp).div_ceil(8);
        if len != stride * ht as usize {
            return Err(invalid("U8glib bitmap size disagrees with metrics"));
        }
        check(
            w as usize * ht as usize,
            l.max_pixels_per_glyph,
            "U8glib glyph pixels",
        )?;
        b.glyph(w as usize * ht as usize * 2, f.len() + 1, l)?;
        let raw = src
            .get(at + size..at + size + len)
            .ok_or_else(|| invalid("truncated U8glib bitmap"))?;
        let mut g = if bpp == 1 {
            Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
                w,
                ht,
                raw,
                bitmap_limits(l),
            )?)
        } else {
            let mut pixels = Vec::with_capacity(w as usize * ht as usize);
            for y in 0..ht as usize {
                for x in 0..w as usize {
                    pixels.push(((raw[y * stride + x / 4] >> (6 - (x % 4) * 2)) & 3) * 85);
                }
            }
            Glyph::new(Bitmap::<crate::bitmap::Gray8>::from_raw(
                w,
                ht,
                &pixels,
                bitmap_limits(l),
            )?)
        };
        g.metrics.bounds.x = x;
        g.metrics.bounds.y = y;
        g.metrics.advance = Some(Vector::new(adv, 0));
        g.codepoints.push(if charset(o) == "unicode" {
            Codepoint::Unicode(cp)
        } else {
            Codepoint::Legacy(cp)
        });
        f.add_glyph(g)?;
        at += size + len;
    }
    for (cp, p) in [(65, 6), (97, 8)] {
        let offset = usize::from(u16::from_be_bytes([h[p], h[p + 1]]));
        if offset != 0 && positions.get(&cp) != Some(&offset) {
            return Err(invalid(
                "U8glib accelerator does not point to corresponding glyph",
            ));
        }
    }
    if src[at..].iter().any(|&v| v != 0) {
        return Err(invalid("unexpected trailing U8glib bytes"));
    }
    f.metrics.bounds = Some(Bounds::new(
        u32::from(h[1]),
        u32::from(h[2]),
        i64::from(h[3] as i8),
        i64::from(h[4] as i8),
    ));
    f.metrics.ascent = Some(i64::from(h[13] as i8));
    f.metrics.descent = Some(-i64::from(h[14] as i8));
    f.charset.name = Some(charset(o).into());
    f.source_format = Some("c".into());
    Ok(f)
}
fn u8x8(src: &[u8], o: &CReadOptions, l: &ResourceLimits, b: &mut Budget) -> Result<Font> {
    let n = if o.u8x8_legacy { 2 } else { 4 };
    let h = src
        .get(..n)
        .ok_or_else(|| invalid("truncated U8x8 header"))?;
    let (tw, th) = if o.u8x8_legacy {
        (1, 1)
    } else {
        (u32::from(h[2]), u32::from(h[3]))
    };
    if h[0] > h[1] || tw == 0 || th == 0 || tw * th > 255 {
        return Err(invalid("invalid U8x8 tile geometry or code range"));
    }
    let size = tw as usize * th as usize * 8;
    let count = usize::from(h[1] - h[0]) + 1;
    let expected = n + count * size;
    if src.len() < expected || src[expected..].iter().any(|&v| v != 0) {
        return Err(invalid("U8x8 bitmap array length mismatch"));
    }
    let (w, ht) = (tw * 8, th * 8);
    check(
        w as usize * ht as usize,
        l.max_pixels_per_glyph,
        "U8x8 pixels",
    )?;
    let geometry = ArrayOptions {
        width: w,
        height: ht,
        first_code: u32::from(h[0]),
        charset: charset(o).into(),
        baseline: o.generic.as_ref().map_or(i64::from(ht), |v| v.baseline),
        lsb_first: false,
    };
    let mut f = Font::default();
    for (i, raw) in src[n..expected].chunks_exact(size).enumerate() {
        b.glyph(size, count, l)?;
        let mut rows = vec![0u8; size];
        for ty in 0..th as usize {
            for tx in 0..tw as usize {
                for x in 0..8 {
                    let column = raw[(ty * tw as usize + tx) * 8 + x];
                    for y in 0..8 {
                        if column & (1 << y) != 0 {
                            rows[(ty * 8 + y) * tw as usize + tx] |= 128 >> x;
                        }
                    }
                }
            }
        }
        f.add_glyph(cell_glyph(&rows, u32::from(h[0]) + i as u32, &geometry, l)?)?;
    }
    cell_metrics(&mut f, &geometry);
    Ok(f)
}
fn constant(t: &[Token<'_>]) -> Result<i64> {
    let atom = |v: &[Token<'_>], at: &mut usize| -> Result<i64> {
        if let Some(Token {
            kind: Kind::String(s),
            ..
        }) = v.get(*at)
        {
            if s.len() != 1 {
                return Err(invalid("NetBSD character literal must be one byte"));
            }
            *at += 1;
            Ok(i64::from(s.as_bytes()[0]))
        } else {
            lexer::integer(v, at)
        }
    };
    let mut at = 0;
    let mut value = atom(t, &mut at)?;
    while at < t.len() {
        let op = t[at].kind;
        if !matches!(op, Kind::Punct(b'+' | b'-')) {
            return Err(invalid(
                "NetBSD constant permits only literal addition/subtraction",
            ));
        }
        at += 1;
        let rhs = atom(t, &mut at)?;
        value = if op == Kind::Punct(b'+') {
            value.checked_add(rhs)
        } else {
            value.checked_sub(rhs)
        }
        .ok_or_else(|| invalid("NetBSD constant overflow"))?;
    }
    Ok(value)
}
fn native_const(t: &[Token<'_>]) -> Result<i64> {
    if let [
        Token {
            kind: Kind::Ident(s),
            ..
        },
    ] = t
    {
        return match *s {
            "WSDISPLAY_FONTENC_ISO" | "WSDISPLAY_FONTORDER_KNOWN" => Ok(0),
            "WSDISPLAY_FONTENC_IBM" | "WSDISPLAY_FONTORDER_L2R" => Ok(1),
            "WSDISPLAY_FONTENC_PCVT" | "WSDISPLAY_FONTORDER_R2L" => Ok(2),
            "WSDISPLAY_FONTENC_ISO7" => Ok(3),
            "WSDISPLAY_FONTENC_ISO2" => Ok(4),
            "WSDISPLAY_FONTENC_KOI8_R" => Ok(5),
            _ => Err(invalid("unknown NetBSD native constant")),
        };
    }
    constant(t)
}
fn netbsd(
    t: &[Token<'_>],
    ds: &[Declaration<'_>],
    o: &CReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Font> {
    let d = select(ds, "wsdisplay_font", o.array_name.as_deref())?;
    let fields = if t
        .get(d.start + 1)
        .is_some_and(|v| v.kind == Kind::Punct(b'.'))
    {
        fields(t, d.start, d.end)?
    } else {
        let keys = [
            "name",
            "firstchar",
            "numchars",
            "encoding",
            "fontwidth",
            "fontheight",
            "stride",
            "bitorder",
            "byteorder",
            "data",
        ];
        let mut map = BTreeMap::new();
        let mut at = d.start + 1;
        for key in keys {
            let start = at;
            while at < d.end && t[at].kind != Kind::Punct(b',') {
                at += 1;
            }
            map.insert(key, &t[start..at]);
            if at < d.end {
                at += 1;
            }
        }
        if at != d.end {
            return Err(invalid("invalid positional wsdisplay_font"));
        }
        map
    };
    let get = |key: &str| -> Result<i64> {
        native_const(
            fields
                .get(key)
                .ok_or_else(|| invalid("missing NetBSD metadata"))?,
        )
    };
    let (first, count) = (
        unsigned(get("firstchar")?, u32::MAX)?,
        unsigned(get("numchars")?, i32::MAX as u32)? as usize,
    );
    check(count, l.max_glyphs, "NetBSD glyph count")?;
    let (w, h) = (
        unsigned(get("fontwidth")?, u32::MAX)?,
        unsigned(get("fontheight")?, u32::MAX)?,
    );
    if w == 0 || h == 0 {
        return Err(invalid("NetBSD cell must be positive"));
    }
    let stride = unsigned(get("stride")?, u32::MAX)? as usize;
    let row = (w as usize).div_ceil(8);
    if stride < row {
        return Err(invalid("NetBSD stride shorter than glyph row"));
    }
    let (bo, by) = (get("bitorder")?, get("byteorder")?);
    if ![1, 2].contains(&bo) || ![1, 2].contains(&by) {
        return Err(invalid(
            "NetBSD KNOWN or invalid bit/byte order is platform dependent",
        ));
    }
    let encoding = get("encoding")?;
    let cs = match encoding {
        0 => "unicode",
        1 => "cp437",
        2 => "wsfont-pcvt",
        3 => "iso8859-7",
        4 => "iso8859-2",
        5 => "koi8-r",
        _ => return Err(invalid("unknown NetBSD encoding")),
    };
    let geometry = ArrayOptions {
        width: w,
        height: h,
        first_code: first,
        charset: cs.into(),
        baseline: o.generic.as_ref().map_or(i64::from(h), |v| v.baseline),
        lsb_first: false,
    };
    let size = cell_size(&geometry, l)?;
    let raw = blob(t, named(ds, ref_field(&fields, "data")?)?, b)?;
    let native_size = stride
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("NetBSD cell size overflow"))?;
    if count.checked_mul(native_size) != Some(raw.len()) {
        return Err(invalid("NetBSD bitmap data length mismatch"));
    }
    let mut f = Font::default();
    for (i, src) in raw.chunks_exact(native_size).enumerate() {
        b.glyph(size, i + 1, l)?;
        let mut dst = vec![0u8; size];
        for y in 0..h as usize {
            for x in 0..row {
                let v = src[y * stride + if by == 1 { x } else { stride - 1 - x }];
                dst[y * row + x] = if bo == 1 { v } else { v.reverse_bits() };
            }
        }
        f.add_glyph(cell_glyph(
            &dst,
            first
                .checked_add(i as u32)
                .ok_or_else(|| invalid("NetBSD character code overflow"))?,
            &geometry,
            l,
        )?)?;
    }
    cell_metrics(&mut f, &geometry);
    if let Some(name) = fields.get("name") {
        if !name.is_empty() && name.iter().all(|v| matches!(v.kind, Kind::String(_))) {
            let d = Declaration {
                name: "name",
                ty: "char",
                dimensions: vec![],
                start: 0,
                end: name.len() - 1,
            };
            let mut bytes = blob(name, &d, b)?;
            bytes.pop();
            if bytes.contains(&0) {
                return Err(invalid("embedded NUL in NetBSD font name"));
            }
            f.names.full = Some(
                String::from_utf8(bytes).map_err(|_| invalid("NetBSD font name is not UTF-8"))?,
            );
        } else if reference(name)?.is_some() {
            return Err(invalid("NetBSD name must be literal or NULL"));
        }
    }
    Ok(f)
}
pub(super) fn write(
    f: &Font,
    selected: &BTreeMap<u32, (usize, &Glyph)>,
    o: &CWriteOptions,
    b: &mut Budget,
) -> Result<String> {
    let first = o
        .range
        .map_or(*selected.first_key_value().expect("selected").0, |v| v.0);
    let last = o
        .range
        .map_or(*selected.last_key_value().expect("selected").0, |v| v.1);
    if first > last {
        return Err(invalid("reversed embedded C range"));
    }
    let count = last
        .checked_sub(first)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("embedded C range overflow"))? as usize;
    check(count, o.limits.max_glyphs, "embedded C glyph count")?;
    if o.family != Family::Netbsd && last > 255 {
        return Err(invalid("legacy U8glib/U8x8 only accepts 8-bit codes"));
    }
    let cell = if let Some(v) = &o.generic {
        Bounds::new(
            v.width,
            v.height,
            0,
            v.baseline
                .checked_sub(i64::from(v.height))
                .ok_or_else(|| invalid("C cell baseline overflow"))?,
        )
    } else {
        f.metrics.bounds.unwrap_or(Bounds::new(8, 8, 0, 0))
    };
    if o.family == Family::U8glib {
        return write_u8glib(f, selected, o, b, first, last, cell);
    }
    let (w, h) = (cell.width, cell.height);
    if w == 0 || h == 0 {
        return Err(invalid("embedded C fixed cell must be positive"));
    }
    let size = cell_size(
        &ArrayOptions {
            width: w,
            height: h,
            first_code: first,
            charset: "unicode".into(),
            baseline: cell.y + i64::from(h),
            lsb_first: false,
        },
        &o.limits,
    )?;
    b.add(count.saturating_mul(size).saturating_mul(16) + count.saturating_mul(256))?;
    let mut bytes = Vec::new();
    if o.family == Family::U8x8 {
        if w % 8 != 0
            || h % 8 != 0
            || w / 8 > 255
            || h / 8 > 255
            || (w / 8) * (h / 8) > 255
            || (o.u8x8_legacy && (w != 8 || h != 8))
        {
            return Err(invalid(
                "U8x8 dimensions require positive 8x8 tiles within u8 tile count",
            ));
        }
        bytes.extend([first as u8, last as u8]);
        if !o.u8x8_legacy {
            bytes.extend([(w / 8) as u8, (h / 8) as u8]);
        }
    }
    for cp in first..=last {
        let bitmap = selected
            .get(&cp)
            .map(|(_, g)| writer::pixels(g, cell, o.gray_threshold, &o.limits))
            .transpose()?;
        if o.family == Family::Netbsd {
            if let Some(bitmap) = bitmap {
                bytes.extend_from_slice(bitmap.as_bytes());
            } else {
                bytes.resize(bytes.len() + size, 0);
            }
        } else {
            for ty in 0..h / 8 {
                for tx in 0..w / 8 {
                    for x in 0..8 {
                        let mut v = 0;
                        for y in 0..8 {
                            if bitmap.as_ref().is_some_and(|bm| {
                                bm.get_pixel(tx * 8 + x, ty * 8 + y).unwrap_or(0) != 0
                            }) {
                                v |= 1 << y;
                            }
                        }
                        bytes.push(v);
                    }
                }
            }
        }
    }
    let symbol = &o.array_name;
    let mut out = String::from("#include <stdint.h>\n");
    if o.family == Family::Netbsd {
        let cs = o.generic.as_ref().map_or("unicode", |v| v.charset.as_str());
        let enc = match cs {
            "unicode" => 0,
            "cp437" => 1,
            "wsfont-pcvt" => 2,
            "iso8859-7" => 3,
            "iso8859-2" => 4,
            "koi8-r" => 5,
            _ => return Err(invalid("charset has no NetBSD font encoding number")),
        };
        out.push_str("#include <sys/types.h>\n#include <dev/wscons/wsconsio.h>\n");
        emit_bytes(&mut out, &format!("{symbol}_data"), &bytes);
        let name = f.names.full.as_deref().unwrap_or(symbol);
        if name.as_bytes().contains(&0) {
            return Err(invalid("NetBSD name cannot contain NUL"));
        }
        b.add(name.len().saturating_mul(8))?;
        let mut escaped = String::new();
        for byte in name.bytes() {
            if (32..=126).contains(&byte) && byte != b'"' && byte != b'\\' {
                escaped.push(char::from(byte));
            } else {
                write!(escaped, "\\{byte:03o}").expect("String write");
            }
        }
        let name = &escaped;
        writeln!(out,"struct wsdisplay_font {symbol} = {{.name=\"{name}\",.firstchar={first},.numchars={count},.encoding={enc},.fontwidth={w},.fontheight={h},.stride={},.bitorder=1,.byteorder=1,.data={symbol}_data}};",(w as usize).div_ceil(8)).expect("String write");
    } else {
        out.push_str("#include <u8x8.h>\n/* fontpixel-u8x8 */\n");
        emit_bytes(&mut out, symbol, &bytes);
    }
    Ok(out)
}
fn write_u8glib(
    f: &Font,
    selected: &BTreeMap<u32, (usize, &Glyph)>,
    o: &CWriteOptions,
    b: &mut Budget,
    first: u32,
    last: u32,
    cell: Bounds,
) -> Result<String> {
    let format = match o.u8glib_format {
        U8glibFormat::Full => 0,
        U8glibFormat::Compact => 1,
        U8glibFormat::Gray2 => 2,
    };
    if cell.width > 255
        || cell.height > 255
        || !(-128..=127).contains(&cell.x)
        || !(-128..=127).contains(&cell.y)
    {
        return Err(invalid("U8glib font bounds out of range"));
    }
    let asc = f.metrics.ascent.unwrap_or(cell.y + i64::from(cell.height));
    let desc = f.metrics.descent.unwrap_or(-cell.y);
    if !(-128..=127).contains(&asc) || !(-127..=128).contains(&desc) {
        return Err(invalid("U8glib ascent/descent out of range"));
    }
    let mut bytes = vec![
        format,
        cell.width as u8,
        cell.height as u8,
        cell.x as u8,
        cell.y as u8,
        asc as u8,
        0,
        0,
        0,
        0,
        first as u8,
        last as u8,
        (-desc) as u8,
        asc as u8,
        (-desc) as u8,
        asc as u8,
        (-desc) as u8,
    ];
    for cp in first..=last {
        if cp == 65 || cp == 97 {
            let offset = u16::try_from(bytes.len())
                .map_err(|_| invalid("U8glib accelerator exceeds u16"))?;
            let at = if cp == 65 { 6 } else { 8 };
            bytes[at..at + 2].copy_from_slice(&offset.to_be_bytes());
        }
        let Some((_, g)) = selected.get(&cp) else {
            bytes.push(255);
            continue;
        };
        let bb = g.metrics.bounds;
        let a = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .unwrap_or(Vector::new(i64::from(bb.width), 0))
            .x;
        if bb.width >= 255
            || bb.height > 255
            || !(-128..=127).contains(&bb.x)
            || !(-128..=127).contains(&bb.y)
            || !(-128..=127).contains(&a)
        {
            return Err(invalid("U8glib glyph metric exceeds native range"));
        }
        let stride = (bb.width as usize * if format == 2 { 2 } else { 1 }).div_ceil(8);
        let size = stride * bb.height as usize;
        if size > 255 {
            return Err(invalid("U8glib glyph data length exceeds u8"));
        }
        b.add(size * 16 + 512)?;
        if format == 1 {
            if bb.width > 15
                || bb.height > 15
                || !(0..=15).contains(&bb.x)
                || !(-2..=13).contains(&bb.y)
                || !(0..=15).contains(&a)
                || size > 15
                || (bb.x == 15 && bb.y == 13)
            {
                return Err(invalid("U8glib compact metrics exceed native nibbles"));
            }
            bytes.extend([
                (bb.x as u8) << 4 | (bb.y + 2) as u8,
                (bb.width as u8) << 4 | bb.height as u8,
                (a as u8) << 4 | size as u8,
            ]);
        } else {
            bytes.extend([
                bb.width as u8,
                bb.height as u8,
                size as u8,
                a as u8,
                bb.x as u8,
                bb.y as u8,
            ]);
        }
        if format != 2 {
            let bitmap = writer::pixels(g, bb, o.gray_threshold, &o.limits)?;
            bytes.extend_from_slice(bitmap.as_bytes());
        } else {
            let start = bytes.len();
            bytes.resize(start + size, 0);
            for y in 0..bb.height as usize {
                for x in 0..bb.width as usize {
                    let p = match &g.bitmap {
                        GlyphBitmap::Mono(v) => {
                            if v.get_pixel(x as u32, y as u32)? == 0 {
                                0
                            } else {
                                255
                            }
                        }
                        GlyphBitmap::Gray8(v) => v.get_pixel(x as u32, y as u32)?,
                    };
                    bytes[start + y * stride + x / 4] |=
                        (((u16::from(p) * 3 + 127) / 255) as u8) << (6 - (x % 4) * 2);
                }
            }
        }
    }
    let mut out = String::from("#include <stdint.h>\n#include <u8g.h>\n");
    emit_bytes(&mut out, &o.array_name, &bytes);
    Ok(out.replacen("uint8_t", "u8g_fntpgm_uint8_t", 1))
}
