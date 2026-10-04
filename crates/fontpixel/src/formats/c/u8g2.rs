use super::embedded::{blob, emit_bytes, select};
use super::*;

struct Bits<'a> {
    src: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn get(&mut self, n: u8) -> Result<u32> {
        if n > 8 || self.at.saturating_add(n as usize) > self.src.len().saturating_mul(8) {
            return Err(invalid("truncated U8g2 bitfield"));
        }
        let mut v = 0;
        for i in 0..n {
            v |= u32::from((self.src[self.at / 8] >> (self.at % 8)) & 1) << i;
            self.at += 1;
        }
        Ok(v)
    }
    fn signed(&mut self, n: u8) -> Result<i64> {
        if n == 0 {
            return Err(invalid("zero-width U8g2 signed field"));
        }
        Ok(i64::from(self.get(n)?) - (1i64 << (n - 1)))
    }
}
fn word(src: &[u8], at: usize) -> Result<usize> {
    let s = src
        .get(at..at + 2)
        .ok_or_else(|| invalid("truncated U8g2 word"))?;
    Ok(usize::from(u16::from_be_bytes([s[0], s[1]])))
}
fn glyph(
    src: &[u8],
    h: &[u8],
    cp: u32,
    l: &ResourceLimits,
    b: &mut Budget,
    count: usize,
) -> Result<Glyph> {
    let mut bits = Bits { src, at: 0 };
    let w = bits.get(h[4])?;
    let ht = bits.get(h[5])?;
    if ht > 127 || (w > 0 && ht == 0) {
        return Err(invalid(
            "U8g2 runtime requires height <=127 and positive height for nonzero width",
        ));
    }
    let x = bits.signed(h[6])?;
    let y = bits.signed(h[7])?;
    let advance = bits.signed(h[8])?;
    let pixels = w as usize * ht as usize;
    check(pixels, l.max_pixels_per_glyph, "U8g2 glyph pixels")?;
    let stride = (w as usize).div_ceil(8);
    let size = stride * ht as usize;
    b.glyph(size, count, l)?;
    let mut dst = vec![0u8; size];
    let mut pos = 0usize;
    while pos < pixels {
        let zero = bits.get(h[2])? as usize;
        let one = bits.get(h[3])? as usize;
        if zero + one == 0 {
            return Err(invalid("U8g2 zero-length run pair"));
        }
        loop {
            if zero + one > pixels - pos {
                return Err(invalid("U8g2 run exceeds glyph"));
            }
            pos += zero;
            for _ in 0..one {
                dst[(pos / w as usize) * stride + (pos % w as usize) / 8] |=
                    128 >> (pos % w as usize % 8);
                pos += 1;
            }
            if bits.get(1)? == 0 {
                break;
            }
        }
    }
    let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
        w,
        ht,
        &dst,
        bitmap_limits(l),
    )?);
    g.codepoints.push(Codepoint::Unicode(cp));
    g.metrics.bounds.x = x;
    g.metrics.bounds.y = y;
    g.metrics.advance = Some(Vector::new(advance, 0));
    Ok(g)
}
pub(super) fn read(
    t: &[Token<'_>],
    ds: &[Declaration<'_>],
    o: &CReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Font> {
    let d = select(ds, "uint8_t", o.array_name.as_deref())?;
    let src = blob(t, d, b)?;
    check(src.len(), l.max_table_bytes, "U8g2 font bytes")?;
    let h = src
        .get(..23)
        .ok_or_else(|| invalid("truncated U8g2 header"))?;
    if h[2..9].iter().any(|&v| v > 8) || h[6..9].contains(&0) || h[2..4] == [0, 0] {
        return Err(invalid("invalid U8g2 bit widths"));
    }
    let mut f = Font::default();
    let mut at = 23;
    let mut prev = None;
    let mut ascii_starts = BTreeMap::new();
    loop {
        let code = *src
            .get(at)
            .ok_or_else(|| invalid("missing U8g2 ASCII terminator"))?;
        let jump = *src
            .get(at + 1)
            .ok_or_else(|| invalid("truncated U8g2 ASCII jump"))? as usize;
        ascii_starts.insert(at - 23, if jump == 0 { 256 } else { u32::from(code) });
        if jump == 0 {
            break;
        }
        if jump < 2 || prev.is_some_and(|p| code <= p) {
            return Err(invalid("invalid U8g2 ASCII order/jump"));
        }
        let rec = src
            .get(at + 2..at + jump)
            .ok_or_else(|| invalid("U8g2 ASCII jump outside array"))?;
        let g = glyph(rec, h, u32::from(code), l, b, f.len() + 1)?;
        f.add_glyph(g)?;
        prev = Some(code);
        at += jump;
    }
    for (p, threshold) in [(17, 65), (19, 97)] {
        let offset = word(h, p)?;
        if !ascii_starts.contains_key(&offset)
            || ascii_starts
                .range(..offset)
                .any(|(_, &cp)| cp >= threshold && cp < 256)
        {
            return Err(invalid("U8g2 ASCII accelerator is not a glyph boundary"));
        }
    }
    let unicode = 23 + word(h, 21)?;
    if unicode < at + 2 {
        return Err(invalid("U8g2 Unicode block overlaps ASCII glyphs"));
    }
    let mut table = unicode;
    let mut block = unicode;
    let mut last = 0usize;
    let mut blocks = Vec::new();
    loop {
        let delta = word(&src, table)?;
        let max = word(&src, table + 2)?;
        if delta == 0 || max <= last {
            return Err(invalid("invalid U8g2 Unicode block jump/order"));
        }
        block = block
            .checked_add(delta)
            .ok_or_else(|| invalid("U8g2 block overflow"))?;
        if block > src.len() {
            return Err(invalid("U8g2 Unicode block outside array"));
        }
        b.add(64)?;
        blocks.push((block, max));
        table += 4;
        last = max;
        if max == 65535 {
            break;
        }
    }
    if blocks[0].0 < table {
        return Err(invalid("U8g2 Unicode glyphs overlap index"));
    }
    at = blocks[0].0;
    let mut unicode_prev = 255usize;
    let mut bi = 0;
    loop {
        let cp = word(&src, at)?;
        if cp == 0 {
            break;
        }
        if cp <= unicode_prev || char::from_u32(cp as u32).is_none() {
            return Err(invalid("invalid U8g2 Unicode code/order"));
        }
        while cp > blocks[bi].1 {
            bi += 1;
            if bi >= blocks.len() || blocks[bi].0 != at {
                return Err(invalid("U8g2 Unicode accelerator mismatch"));
            }
        }
        let jump = *src
            .get(at + 2)
            .ok_or_else(|| invalid("truncated U8g2 Unicode jump"))? as usize;
        if jump < 3 {
            return Err(invalid("invalid U8g2 Unicode record size"));
        }
        let rec = src
            .get(at + 3..at + jump)
            .ok_or_else(|| invalid("U8g2 Unicode glyph outside array"))?;
        let g = glyph(rec, h, cp as u32, l, b, f.len() + 1)?;
        f.add_glyph(g)?;
        unicode_prev = cp;
        at += jump;
    }
    for &(start, _) in &blocks[bi + 1..] {
        if start != at {
            return Err(invalid(
                "unused U8g2 Unicode block must point to terminator",
            ));
        }
    }
    if src[at + 2..].iter().any(|&v| v != 0) {
        return Err(invalid("unexpected trailing U8g2 bytes"));
    }
    f.metrics.bounds = Some(Bounds::new(
        u32::from(h[9]),
        u32::from(h[10]),
        i64::from(h[11] as i8),
        i64::from(h[12] as i8),
    ));
    f.metrics.ascent = Some(i64::from((h[13] as i8).max(h[15] as i8)));
    f.metrics.descent = Some(-i64::from((h[14] as i8).min(h[16] as i8)));
    f.charset.name = Some("unicode".into());
    f.source_format = Some("c".into());
    Ok(f)
}
struct Put {
    bytes: Vec<u8>,
    at: usize,
}
impl Put {
    fn put(&mut self, v: u32, n: u8) {
        for i in 0..n {
            if self.at.is_multiple_of(8) {
                self.bytes.push(0);
            }
            self.bytes[self.at / 8] |= (((v >> i) & 1) as u8) << (self.at % 8);
            self.at += 1;
        }
    }
}
fn ubits(v: u32) -> u8 {
    (32 - v.leading_zeros()).max(1) as u8
}
fn sbits(v: i64) -> Result<u8> {
    (1..=8)
        .find(|&n| v >= -(1i64 << (n - 1)) && v < (1i64 << (n - 1)))
        .ok_or_else(|| invalid("U8g2 signed metric exceeds i8"))
}
pub(super) fn write(
    f: &Font,
    selected: &BTreeMap<u32, (usize, &Glyph)>,
    o: &CWriteOptions,
    b: &mut Budget,
) -> Result<String> {
    let mut header = [0u8; 23];
    header[0] = selected.len() as u8;
    header[2] = 4;
    header[3] = 4;
    header[4..9].fill(1);
    let bounds = f.metrics.bounds.unwrap_or_else(|| {
        selected
            .values()
            .next()
            .expect("selected nonempty")
            .1
            .metrics
            .bounds
    });
    if bounds.width > 255
        || bounds.height > 255
        || !(-128..=127).contains(&bounds.x)
        || !(-128..=127).contains(&bounds.y)
    {
        return Err(invalid("U8g2 font bounds exceed native header"));
    }
    header[9] = bounds.width as u8;
    header[10] = bounds.height as u8;
    header[11] = bounds.x as u8;
    header[12] = bounds.y as u8;
    for (&cp, (_, g)) in selected {
        if cp > 65535 {
            return Err(invalid("U8g2 only supports BMP Unicode"));
        }
        if g.bitmap.width() > 255
            || g.bitmap.height() > 127
            || (g.bitmap.width() > 0 && g.bitmap.height() == 0)
        {
            return Err(invalid("U8g2 glyph dimensions exceed u8"));
        }
        header[4] = header[4].max(ubits(g.bitmap.width()));
        header[5] = header[5].max(ubits(g.bitmap.height()));
        header[6] = header[6].max(sbits(g.metrics.bounds.x)?);
        header[7] = header[7].max(sbits(g.metrics.bounds.y)?);
        let advance = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .unwrap_or(Vector::new(i64::from(g.bitmap.width()), 0));
        header[8] = header[8].max(sbits(advance.x)?);
    }
    let asc = f
        .metrics
        .ascent
        .unwrap_or(bounds.y + i64::from(bounds.height));
    let desc = f.metrics.descent.unwrap_or(-bounds.y);
    if !(-128..=127).contains(&asc) || !(-127..=128).contains(&desc) {
        return Err(invalid("U8g2 ascent/descent exceeds native i8"));
    }
    header[13] = asc as u8;
    header[15] = asc as u8;
    header[14] = (-desc) as u8;
    header[16] = (-desc) as u8;
    let mut data = Vec::new();
    let mut ascii_done = false;
    let mut upper = None;
    let mut lower = None;
    for (&cp, (_, g)) in selected {
        if cp >= 256 && !ascii_done {
            data.extend_from_slice(&[0, 0]);
            let pos =
                u16::try_from(data.len()).map_err(|_| invalid("U8g2 ASCII section too large"))?;
            header[21..23].copy_from_slice(&pos.to_be_bytes());
            data.extend_from_slice(&[0, 4, 255, 255]);
            ascii_done = true;
        }
        if cp < 256 {
            if cp >= 65 && upper.is_none() {
                upper = Some(data.len());
            }
            if cp >= 97 && lower.is_none() {
                lower = Some(data.len());
            }
        }
        let pixels = writer::pixels(g, g.metrics.bounds, o.gray_threshold, &o.limits)?;
        b.add(pixels.as_bytes().len().saturating_mul(2) + 2048)?;
        let mut bits = Put {
            bytes: Vec::new(),
            at: 0,
        };
        bits.put(g.bitmap.width(), header[4]);
        bits.put(g.bitmap.height(), header[5]);
        let adv = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .unwrap_or(Vector::new(i64::from(g.bitmap.width()), 0));
        for (v, n) in [
            (g.metrics.bounds.x, header[6]),
            (g.metrics.bounds.y, header[7]),
            (adv.x, header[8]),
        ] {
            bits.put((v + (1i64 << (n - 1))) as u32, n);
        }
        let count = g.bitmap.width() as usize * g.bitmap.height() as usize;
        let w = g.bitmap.width();
        let mut pos = 0;
        let mut previous = None;
        while pos < count {
            let mut zero = 0;
            while pos < count
                && zero < 15
                && pixels.get_pixel((pos % w as usize) as u32, (pos / w as usize) as u32)? == 0
            {
                zero += 1;
                pos += 1;
            }
            let mut one = 0;
            while pos < count
                && one < 15
                && pixels.get_pixel((pos % w as usize) as u32, (pos / w as usize) as u32)? != 0
            {
                one += 1;
                pos += 1;
            }
            if previous == Some((zero, one)) {
                bits.put(1, 1);
            } else {
                if previous.is_some() {
                    bits.put(0, 1);
                }
                bits.put(zero, 4);
                bits.put(one, 4);
                previous = Some((zero, one));
            }
            if bits.bytes.len() > 252 {
                return Err(invalid(
                    "U8g2 compressed glyph exceeds native record length",
                ));
            }
        }
        if previous.is_some() {
            bits.put(0, 1);
        }
        let prefix = if cp < 256 { 2 } else { 3 };
        let len = u8::try_from(bits.bytes.len() + prefix)
            .map_err(|_| invalid("U8g2 compressed glyph exceeds 255-byte native record"))?;
        if cp < 256 {
            data.push(cp as u8);
        } else {
            data.extend_from_slice(&(cp as u16).to_be_bytes());
        }
        data.push(len);
        data.extend(bits.bytes);
    }
    let ascii_end = if ascii_done {
        usize::from(u16::from_be_bytes([header[21], header[22]])) - 2
    } else {
        data.len()
    };
    for (p, v) in [
        (17, upper.unwrap_or(ascii_end)),
        (19, lower.unwrap_or(ascii_end)),
    ] {
        header[p..p + 2].copy_from_slice(
            &u16::try_from(v)
                .map_err(|_| invalid("U8g2 ASCII accelerator exceeds u16"))?
                .to_be_bytes(),
        );
    }
    if !ascii_done {
        data.extend_from_slice(&[0, 0]);
        header[21..23].copy_from_slice(
            &u16::try_from(data.len())
                .map_err(|_| invalid("U8g2 ASCII section exceeds u16"))?
                .to_be_bytes(),
        );
        data.extend_from_slice(&[0, 4, 255, 255]);
    }
    data.extend_from_slice(&[0, 0]);
    let mut all = header.to_vec();
    all.extend(data);
    b.add(all.len().saturating_mul(12) + o.array_name.len() * 2)?;
    let mut out = String::from("#include <stdint.h>\n#include <u8g2.h>\n");
    emit_bytes(&mut out, &o.array_name, &all);
    let old = format!("{}[] =", o.array_name);
    let new = format!(
        "{}[] U8G2_FONT_SECTION(\"{}\") =",
        o.array_name, o.array_name
    );
    Ok(out.replacen(&old, &new, 1))
}
