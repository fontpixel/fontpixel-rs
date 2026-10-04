use crate::bitmap::{Bitmap, Bounds};
use crate::formats::tex_common::*;
use crate::model::{Codepoint, Font, Glyph, Property, PropertyValue};
use crate::{Report, ResourceLimits, Result};

/// Decode a complete PK stream, including all packed-number representations.
pub fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    check(bytes.len(), limits.max_input_bytes, "PK input bytes")?;
    parse(bytes, limits, bytes.len())
}
pub(super) fn parse(bytes: &[u8], l: &ResourceLimits, retained: usize) -> Result<(Font, Report)> {
    let mut b = Budget::new(retained, l)?;
    b.add(4096)?;
    let mut c = Cursor::new(bytes);
    if c.take(2)? != [247, 89] {
        return Err(invalid("invalid PK preamble"));
    }
    let n = usize::from(c.byte()?);
    b.add(n + 256)?;
    let comment = c.take(n)?.to_vec();
    let mut f = Font::default();
    f.source_format = Some("pk".into());
    f.headers
        .push(Property::new("tex.comment", PropertyValue::Bytes(comment)));
    let (ds, cs, hp, vp) = (c.u(4)?, c.u(4)?, c.u(4)?, c.u(4)?);
    tex_header(&mut f, ds, cs, hp, vp)?;
    loop {
        let op = c.byte()?;
        match op {
            0..=239 => {
                check(f.len() + 1, l.max_glyphs, "PK glyphs")?;
                b.add(2048)?;
                let kind = op & 7;
                let (pl, cp) = if kind == 7 {
                    (c.u(4)? as usize, c.u(4)?)
                } else if kind >= 4 {
                    (usize::from(kind - 4) * 65536 + c.u(2)? as usize, c.u(1)?)
                } else {
                    (usize::from(kind) * 256 + c.u(1)? as usize, c.u(1)?)
                };
                check(pl, l.max_table_bytes, "PK packet bytes")?;
                let mut p = Cursor::new(c.take(pl)?);
                let (tfm, dx, dy, w, h, hoff, voff) = if kind == 7 {
                    (
                        p.s(4)? as i32,
                        p.s(4)?,
                        p.s(4)?,
                        p.u(4)?,
                        p.u(4)?,
                        p.s(4)?,
                        p.s(4)?,
                    )
                } else {
                    let tfm = p.u(3)? as i32;
                    let n = if kind >= 4 { 2 } else { 1 };
                    let dx = p.u(n)?;
                    let w = p.u(n)?;
                    let h = p.u(n)?;
                    (tfm, i64::from(dx) * 65536, 0, w, h, p.s(n)?, p.s(n)?)
                };
                let size = bitmap_size(w, h, l)?;
                b.add(size.saturating_mul(2))?;
                let raw = p.take(pl - p.p)?;
                let raster = decode(raw, w, h, op >> 4, op & 8 != 0, size)?;
                let bitmap: Bitmap = Bitmap::from_raw(w, h, &raster, bitmap_limits(l))?;
                let mut g = Glyph::new(bitmap);
                g.codepoints.push(Codepoint::Legacy(cp));
                g.metrics.bounds = Bounds::new(w, h, -hoff, voff - i64::from(h) + 1);
                tex_glyph(&mut g, tfm, dx, dy);
                f.add_glyph(g)?;
            }
            240..=244 => special(&mut c, op, 240, "pk", &mut f.properties, l, &mut b)?,
            245 => {
                if c.take(bytes.len() - c.p)?.iter().any(|&v| v != 246) {
                    return Err(invalid("PK trailing bytes after postamble"));
                }
                break;
            }
            246 => {}
            _ => return Err(invalid("invalid PK command")),
        }
    }
    f.update_bounds()?;
    Ok((f, Report::default()))
}
struct Nibbles<'a> {
    b: &'a [u8],
    p: usize,
}
impl Nibbles<'_> {
    fn next(&mut self) -> Result<u8> {
        let b = *self
            .b
            .get(self.p / 2)
            .ok_or_else(|| invalid("truncated PK packed number"))?;
        let n = if self.p.is_multiple_of(2) {
            b >> 4
        } else {
            b & 15
        };
        self.p += 1;
        Ok(n)
    }
    fn num(&mut self, first: u8, d: u8, max: usize) -> Result<usize> {
        let value = if first == 0 {
            let mut zeros = 1;
            let mut n = usize::from(self.next()?);
            while n == 0 {
                zeros += 1;
                if zeros > 16 {
                    return Err(invalid("PK packed number overflow"));
                }
                n = usize::from(self.next()?);
            }
            for _ in 0..zeros {
                n = n
                    .checked_mul(16)
                    .and_then(|v| v.checked_add(usize::from(self.next().ok()?)))
                    .ok_or_else(|| invalid("PK packed number overflow/truncation"))?;
            }
            n.checked_sub(15)
                .and_then(|v| v.checked_add(usize::from(13 - d) * 16 + usize::from(d)))
                .ok_or_else(|| invalid("PK packed number overflow"))?
        } else if first <= d {
            usize::from(first)
        } else if first <= 13 {
            usize::from(first - d - 1) * 16 + usize::from(self.next()?) + usize::from(d) + 1
        } else {
            return Err(invalid("nested PK repeat marker"));
        };
        check(value, max, "PK packed run/repeat")?;
        if value == 0 {
            return Err(invalid("zero PK packed run"));
        }
        Ok(value)
    }
}
fn decode(raw: &[u8], w: u32, h: u32, d: u8, mut ink: bool, size: usize) -> Result<Vec<u8>> {
    let mut out = vec![0; size];
    let (w, h) = (w as usize, h as usize);
    let stride = w.div_ceil(8);
    let area = w.checked_mul(h).ok_or_else(limit)?;
    if d == 14 {
        if raw.len() != area.div_ceil(8) {
            return Err(invalid("PK raw bitmap length disagrees with dimensions"));
        }
        if w > 0 {
            for y in 0..h {
                copy_bits(raw, y * w, &mut out[y * stride..(y + 1) * stride], 0, w);
            }
        }
        return Ok(out);
    }
    if area == 0 {
        if !raw.is_empty() {
            return Err(invalid("packed bytes for empty PK bitmap"));
        }
        return Ok(out);
    }
    let mut n = Nibbles { b: raw, p: 0 };
    let (mut x, mut y, mut repeat) = (0usize, 0usize, 0usize);
    while y < h {
        let first = n.next()?;
        let first = if first >= 14 {
            if repeat != 0 {
                return Err(invalid("multiple PK repeats in one row"));
            }
            repeat = if first == 15 {
                1
            } else {
                let z = n.next()?;
                n.num(z, d, h - y - 1)?
            };
            n.next()?
        } else {
            first
        };
        let mut run = n.num(first, d, area)?;
        while run > 0 {
            if y >= h {
                return Err(invalid("PK run exceeds glyph raster"));
            }
            let take = run.min(w - x);
            if ink {
                black(&mut out[y * stride..(y + 1) * stride], x, take);
            }
            x += take;
            run -= take;
            if x == w {
                if repeat > h - y - 1 {
                    return Err(invalid("PK repeated rows exceed glyph height"));
                }
                for row in y + 1..=y + repeat {
                    let (src, dst) = out.split_at_mut(row * stride);
                    dst[..stride].copy_from_slice(&src[y * stride..(y + 1) * stride]);
                }
                y += repeat + 1;
                x = 0;
                repeat = 0;
            }
        }
        ink = !ink;
    }
    if x != 0 || repeat != 0 || n.p.div_ceil(2) != raw.len() {
        return Err(invalid("PK packed bitmap has extra runs"));
    }
    if n.p % 2 == 1 && raw.last().is_some_and(|v| v & 15 != 0) {
        return Err(invalid("PK nonzero trailing packed nibble"));
    }
    Ok(out)
}
