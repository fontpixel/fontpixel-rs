use super::*;
use crate::bitmap::{Bitmap, Gray8, Mono};
use crate::model::GlyphBitmap;
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn sextet(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
fn base64(s: &str) -> Result<Vec<u8>> {
    let n = s.bytes().filter(|b| !b.is_ascii_whitespace()).count();
    let mut out = Vec::new();
    out.try_reserve_exact(n / 4 * 3 + 3).map_err(invalid)?;
    let (mut acc, mut bits, mut padding, mut symbols) = (0u32, 0u32, 0usize, 0usize);
    for b in s.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if b == b'=' {
            padding += 1;
            if padding > 2 {
                return Err(invalid("invalid base64 padding"));
            }
            continue;
        }
        if padding != 0 {
            return Err(invalid("base64 data after padding"));
        }
        let v = sextet(b).ok_or_else(|| invalid("invalid base64 character"))?;
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        symbols += 1;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if symbols % 4 == 1
        || acc != 0
        || (padding != 0 && (n % 4 != 0 || padding != (4 - symbols % 4) % 4))
    {
        return Err(invalid("invalid base64 tail"));
    }
    Ok(out)
}
fn uleb(raw: &[u8], pos: &mut usize) -> Result<u32> {
    let mut v = 0u32;
    for shift in (0..35).step_by(7) {
        let b = *raw
            .get(*pos)
            .ok_or_else(|| invalid("truncated bitmap dimension"))?;
        *pos += 1;
        if shift == 28 && b > 7 {
            return Err(invalid("bitmap dimension exceeds signed 32-bit range"));
        }
        v |= u32::from(b & 127) << shift;
        if b & 128 == 0 {
            return Ok(v);
        }
    }
    Err(invalid("oversized bitmap dimension"))
}
pub(super) fn decode(
    s: &str,
    threshold: Option<u8>,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(GlyphBitmap, bool)> {
    if s.bytes().all(|b| b.is_ascii_whitespace()) {
        return Ok((GlyphBitmap::default(), false));
    }
    bound(s.len(), l.max_table_bytes, "KBITX bitmap attribute bytes")?;
    let encoded_max = s.len() / 4 * 3 + 3;
    budget.temporary(encoded_max)?;
    let raw = base64(s)?;
    let mut pos = 0;
    let h = uleb(&raw, &mut pos)?;
    let w = uleb(&raw, &mut pos)?;
    let count = usize::try_from(u64::from(w) * u64::from(h)).map_err(invalid)?;
    bound(count, l.max_pixels_per_glyph, "KBITX glyph pixels")?;
    budget.temporary(encoded_max.saturating_add(count.saturating_mul(2)))?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(count).map_err(invalid)?;
    while pos < raw.len() {
        let op = raw[pos];
        pos += 1;
        let len = usize::from(op & 31) * if op & 32 != 0 { 32 } else { 1 };
        if len == 0 || pixels.len().saturating_add(len) > count {
            return Err(invalid("zero-length or overflowing WIB run"));
        }
        match op & 192 {
            0 => pixels.resize(pixels.len() + len, 0),
            64 => pixels.resize(pixels.len() + len, 255),
            128 => {
                let value = *raw
                    .get(pos)
                    .ok_or_else(|| invalid("truncated repeated WIB run"))?;
                pos += 1;
                pixels.resize(pixels.len() + len, value);
            }
            _ => {
                let slice = raw
                    .get(pos..pos + len)
                    .ok_or_else(|| invalid("truncated literal WIB run"))?;
                pixels.extend_from_slice(slice);
                pos += len;
            }
        }
    }
    if pixels.len() != count {
        return Err(invalid("WIB runs do not fill bitmap"));
    }
    let gray = pixels.iter().any(|&v| v != 0 && v != 255);
    let loss = threshold.is_some_and(|t| pixels.iter().any(|&v| v != if v >= t { 255 } else { 0 }));
    let result = if gray && threshold.is_none() {
        GlyphBitmap::Gray8(Bitmap::<Gray8>::from_raw(w, h, &pixels, bitmap_limits(l))?)
    } else {
        let mut mono = Bitmap::<Mono>::with_limits(w, h, bitmap_limits(l))?;
        let t = threshold.unwrap_or(128);
        if w > 0 {
            for (i, &v) in pixels.iter().enumerate() {
                if v >= t {
                    mono.set_pixel((i % w as usize) as u32, (i / w as usize) as u32, 1)?;
                }
            }
        }
        GlyphBitmap::Mono(mono)
    };
    budget.add(result.as_bytes().len())?;
    Ok((result, loss))
}
fn push_uleb(mut n: u32, out: &mut Vec<u8>) {
    while n >= 128 {
        out.push(n as u8 | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
/// Original deterministic encoder using bounded repeat and literal runs.
pub(super) fn encode(bitmap: &GlyphBitmap) -> Result<Vec<u8>> {
    let w = bitmap.width();
    let h = bitmap.height();
    let count = usize::try_from(u64::from(w) * u64::from(h)).map_err(invalid)?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(count.saturating_mul(2).saturating_add(10))
        .map_err(invalid)?;
    push_uleb(h, &mut raw);
    push_uleb(w, &mut raw);
    let value = |i: usize| -> u8 {
        match bitmap {
            GlyphBitmap::Gray8(b) => b.as_bytes()[i],
            GlyphBitmap::Mono(b) => {
                if b.as_bytes()[i / w as usize * (w as usize).div_ceil(8) + (i % w as usize) / 8]
                    & (0x80 >> (i % w as usize % 8))
                    != 0
                {
                    255
                } else {
                    0
                }
            }
        }
    };
    let mut i = 0;
    while i < count {
        let v = value(i);
        let mut end = i + 1;
        while end < count && value(end) == v && end - i < 992 {
            end += 1;
        }
        let run = end - i;
        if v != 0 && v != 255 && run < 3 {
            let mut literals = [0u8; 31];
            let mut count_literal = 0;
            while i + count_literal < count && count_literal < literals.len() {
                let p = i + count_literal;
                let sample = value(p);
                if sample == 0
                    || sample == 255
                    || p + 2 < count && value(p + 1) == sample && value(p + 2) == sample
                {
                    break;
                }
                literals[count_literal] = sample;
                count_literal += 1;
            }
            raw.push(192 | count_literal as u8);
            raw.extend_from_slice(&literals[..count_literal]);
            i += count_literal;
            continue;
        }
        let (len, code) = if run >= 32 {
            (run / 32 * 32, (run / 32) as u8 | 32)
        } else {
            (run, run as u8)
        };
        raw.push(
            code | if v == 0 {
                0
            } else if v == 255 {
                64
            } else {
                128
            },
        );
        if v != 0 && v != 255 {
            raw.push(v);
        }
        i += len;
    }
    let mut out = Vec::new();
    out.try_reserve_exact(raw.len().div_ceil(3) * 4)
        .map_err(invalid)?;
    for chunk in raw.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[(n >> 18) as usize]);
        out.push(ALPHABET[((n >> 12) & 63) as usize]);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 63) as usize]);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 63) as usize]);
        }
    }
    Ok(out)
}
