use crate::bitmap::{Bitmap, Gray8};
use crate::formats::tex_common::*;
use crate::model::GlyphBitmap;
use crate::{ResourceLimits, Result};
pub(super) struct Image {
    pub bitmap: GlyphBitmap,
    pub minx: i64,
    pub miny: i64,
    pub converted: bool,
}
pub(super) fn field<'a>(c: &mut Cursor<'a>) -> Result<&'a str> {
    let b = c.take(12)?;
    if b[11] != b' ' {
        return Err(invalid("Plan 9 field lacks required trailing space"));
    }
    std::str::from_utf8(&b[..11])
        .map(str::trim)
        .map_err(|_| invalid("Plan 9 header is not ASCII"))
}
pub(super) fn number(c: &mut Cursor<'_>) -> Result<i64> {
    let s = field(c)?;
    if s.is_empty()
        || !s
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_digit() || (i == 0 && b == b'-'))
    {
        return Err(invalid("invalid Plan 9 decimal field"));
    }
    s.parse()
        .map_err(|_| invalid("Plan 9 decimal field overflow"))
}
pub(super) fn header(o: &mut Output, s: &str) -> Result<()> {
    if s.len() > 11 {
        return Err(invalid("Plan 9 header field exceeds eleven bytes"));
    }
    o.put(format!("{s:>11} ").as_bytes())
}
pub(super) fn looks(b: &[u8]) -> bool {
    if b.starts_with(b"compressed\n") {
        return true;
    }
    let Some(v) = b.get(..12) else { return false };
    v[11] == b' ' && std::str::from_utf8(&v[..11]).is_ok_and(|v| channels(v.trim()).is_ok())
}
type ChannelLayout = (Vec<(u8, u8)>, u8, bool);
fn channels(s: &str) -> Result<ChannelLayout> {
    let (old, s) = match s {
        "0" => (true, "k1"),
        "1" => (true, "k2"),
        "2" => (true, "k4"),
        "3" => (true, "m8"),
        _ => (false, s),
    };
    if s.is_empty() || s.len() > 11 || s.len() % 2 != 0 {
        return Err(invalid("invalid Plan 9 channel descriptor"));
    }
    let mut channels = Vec::new();
    let mut bits = 0u8;
    for pair in s.as_bytes().as_chunks::<2>().0 {
        let (tag, n) = (pair[0], pair[1].wrapping_sub(b'0'));
        if !b"rgbakmx".contains(&tag)
            || !(1..=8).contains(&n)
            || tag != b'x' && channels.iter().any(|&(t, _)| t == tag)
        {
            return Err(invalid("invalid or duplicate Plan 9 channel"));
        }
        bits = bits
            .checked_add(n)
            .ok_or_else(|| invalid("Plan 9 channel depth overflow"))?;
        channels.push((tag, n));
    }
    if bits > 32 || !(bits == 1 || bits == 2 || bits == 4 || bits.is_multiple_of(8)) {
        return Err(invalid("unsupported Plan 9 channel depth"));
    }
    let has = |tag| channels.iter().any(|&(t, _)| t == tag);
    if !has(b'k') && !has(b'm') && !(has(b'r') && has(b'g') && has(b'b')) {
        return Err(invalid("Plan 9 image needs grey, palette or RGB channels"));
    }
    if let Some(&(_, a)) = channels.iter().find(|&&(t, _)| t == b'a')
        && channels.iter().any(|&(t, n)| t != b'x' && n > a)
    {
        return Err(invalid("Plan 9 alpha channel is too shallow"));
    }
    Ok((channels, bits, old))
}
pub(super) fn read(c: &mut Cursor<'_>, l: &ResourceLimits, budget: &mut Budget) -> Result<Image> {
    let compressed = c.b[c.p..].starts_with(b"compressed\n");
    if compressed {
        c.take(11)?;
    }
    let descriptor = field(c)?;
    let (channels, depth, old) = channels(descriptor)?;
    let (minx, miny, maxx, maxy) = (number(c)?, number(c)?, number(c)?, number(c)?);
    for v in [minx, miny, maxx, maxy] {
        i32::try_from(v).map_err(|_| invalid("Plan 9 rectangle coordinate outside i32"))?;
    }
    let w = u32::try_from(maxx - minx).map_err(|_| invalid("Plan 9 image width invalid"))?;
    let h = u32::try_from(maxy - miny).map_err(|_| invalid("Plan 9 image height invalid"))?;
    let stride = usize::try_from(
        (maxx * i64::from(depth) + 7).div_euclid(8) - (minx * i64::from(depth)).div_euclid(8),
    )
    .map_err(|_| invalid("Plan 9 row size invalid"))?;
    let size = stride.checked_mul(h as usize).ok_or_else(limit)?;
    check(size, l.max_table_bytes, "Plan 9 image bytes")?;
    check(
        size,
        l.max_decompressed_bytes,
        "Plan 9 decompressed image bytes",
    )?;
    budget.add(size.saturating_mul(2))?;
    let mut raw = Vec::with_capacity(size);
    if compressed {
        let mut y = miny;
        while y < maxy {
            let end = number(c)?;
            let n = number(c)?;
            if end <= y || end > maxy || n <= 0 || n > 6000 {
                return Err(invalid("invalid Plan 9 compression block header"));
            }
            let encoded = c.take(n as usize)?;
            let count = usize::try_from(end - y)
                .ok()
                .and_then(|v| v.checked_mul(stride))
                .ok_or_else(limit)?;
            decode(encoded, count, stride, &mut raw)?;
            y = end;
        }
    } else {
        raw.extend_from_slice(c.take(size)?)
    }
    if old {
        for v in &mut raw {
            *v = !*v
        }
    }
    let mono = channels == [(b'k', 1)];
    let pixels = usize::try_from(u64::from(w) * u64::from(h)).map_err(|_| limit())?;
    let output = if mono {
        (w as usize)
            .div_ceil(8)
            .checked_mul(h as usize)
            .ok_or_else(limit)?
    } else {
        pixels
    };
    check(output, l.max_table_bytes, "Plan 9 decoded image bytes")?;
    budget.add(output.saturating_mul(2))?;
    let bl = crate::bitmap::BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: l.max_table_bytes.saturating_mul(8),
        max_bytes: l.max_total_bytes,
    };
    let mut raster = vec![0; output];
    let offset = (minx * i64::from(depth)).rem_euclid(8) as usize;
    if mono {
        let dest_stride = (w as usize).div_ceil(8);
        if w > 0 {
            for y in 0..h as usize {
                copy_bits(
                    &raw[y * stride..(y + 1) * stride],
                    offset,
                    &mut raster[y * dest_stride..(y + 1) * dest_stride],
                    0,
                    w as usize,
                )
            }
        }
    } else if channels == [(b'k', 8)] {
        raster.copy_from_slice(&raw)
    } else {
        for y in 0..h as usize {
            for x in 0..w as usize {
                let bit = offset + x * usize::from(depth);
                let value = if depth < 8 {
                    u32::from(raw[y * stride + bit / 8] >> (8 - usize::from(depth) - bit % 8))
                        & ((1 << depth) - 1)
                } else {
                    let mut v = 0;
                    for i in 0..usize::from(depth) / 8 {
                        v |= u32::from(raw[y * stride + bit / 8 + i]) << (8 * i)
                    }
                    v
                };
                let mut left = depth;
                let mut rgba = [0u32; 4];
                let mut grey = None;
                let mut alpha = None;
                for &(tag, n) in &channels {
                    left -= n;
                    let component = (value >> left) & ((1 << n) - 1);
                    let sample = component * 255 / ((1 << n) - 1);
                    match tag {
                        b'k' => grey = Some(sample),
                        b'a' => alpha = Some(sample),
                        b'r' => rgba[0] = sample,
                        b'g' => rgba[1] = sample,
                        b'b' => rgba[2] = sample,
                        b'm' => {
                            let i = component as u8;
                            let r = u32::from(i >> 6);
                            let v = u32::from((i >> 4) & 3);
                            let j = i.wrapping_sub(i >> 4) & 15;
                            let g = u32::from(j >> 2);
                            let bl = u32::from(j & 3);
                            let den = r.max(g).max(bl);
                            if den == 0 {
                                rgba = [v * 17; 4]
                            } else {
                                let num = 17 * (4 * den + v);
                                rgba = [r * num / den, g * num / den, bl * num / den, 0]
                            }
                        }
                        _ => {}
                    }
                }
                raster[y * w as usize + x] = alpha
                    .or(grey)
                    .unwrap_or((77 * rgba[0] + 150 * rgba[1] + 29 * rgba[2] + 128) / 256)
                    as u8;
            }
        }
    }
    let bitmap = if mono {
        GlyphBitmap::Mono(Bitmap::from_raw(w, h, &raster, bl)?)
    } else {
        GlyphBitmap::Gray8(Bitmap::<Gray8>::from_raw(w, h, &raster, bl)?)
    };
    let converted = channels.iter().any(|&(t, _)| t != b'k');
    Ok(Image {
        bitmap,
        minx,
        miny,
        converted,
    })
}
fn decode(input: &[u8], expected: usize, stride: usize, out: &mut Vec<u8>) -> Result<()> {
    let base = out.len();
    let end = base.checked_add(expected).ok_or_else(limit)?;
    let mut c = Cursor::new(input);
    while c.p < input.len() {
        let op = c.byte()?;
        if op >= 128 {
            let n = usize::from(op & 127) + 1;
            if n > end - out.len() || stride == 0 || n > stride - (out.len() - base) % stride {
                return Err(invalid("Plan 9 literal run exceeds block rows"));
            }
            out.extend_from_slice(c.take(n)?);
        } else {
            let n = usize::from(op >> 2) + 3;
            let distance = (usize::from(op & 3) << 8) + usize::from(c.byte()?) + 1;
            if distance > out.len() - base
                || n > end - out.len()
                || stride == 0
                || n > stride - (out.len() - base) % stride
            {
                return Err(invalid("Plan 9 backreference is outside compression block"));
            }
            for _ in 0..n {
                let v = out[out.len() - distance];
                out.push(v)
            }
        }
    }
    if out.len() != end {
        return Err(invalid("Plan 9 compression block does not fill its rows"));
    }
    Ok(())
}
