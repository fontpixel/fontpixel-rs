//! Continuous low-bit-first pixels and bounded packed-number runs.
use super::*;
pub(super) fn decode(
    raw: &[u8],
    w: u32,
    h: u32,
    depth: u8,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<GlyphBitmap> {
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("RISC OS pixel count overflow"))?;
    check(pixels, l.max_pixels_per_glyph, "RISC OS glyph pixels")?;
    let bits = pixels
        .checked_mul(usize::from(depth))
        .ok_or_else(|| invalid("RISC OS bit count overflow"))?;
    slice(raw, 0, bits.div_ceil(8))?;
    let row = if depth == 1 {
        (w as usize).div_ceil(8)
    } else {
        w as usize
    };
    budget.add(row * h as usize * 2)?;
    let mut packed = vec![0; row * h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let bit = (y * w as usize + x) * usize::from(depth);
            let value = (raw[bit / 8] >> (bit % 8)) & if depth == 1 { 1 } else { 15 };
            let dy = h as usize - 1 - y;
            if depth == 1 {
                if value != 0 {
                    packed[dy * row + x / 8] |= 128 >> (x % 8);
                }
            } else {
                packed[dy * row + x] = value * 17;
            }
        }
    }
    if depth == 1 {
        Ok(GlyphBitmap::Mono(raster(w, h, &packed, l, budget)?))
    } else {
        Ok(GlyphBitmap::Gray8(Bitmap::<Gray8>::from_raw(
            w,
            h,
            &packed,
            bitmap_limits(l),
        )?))
    }
}
struct Nibbles<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Nibbles<'_> {
    fn next(&mut self) -> Result<usize> {
        let byte = *self
            .bytes
            .get(self.at / 2)
            .ok_or_else(|| invalid("RISC OS truncated packed number"))?;
        let value = usize::from((byte >> ((self.at % 2) * 4)) & 15);
        self.at += 1;
        Ok(value)
    }
    fn number(&mut self, first: usize, f: usize) -> Result<usize> {
        if first == 0 {
            let mut zeros = 1;
            let mut value = self.next()?;
            while value == 0 {
                zeros += 1;
                if zeros > 8 {
                    return Err(invalid("RISC OS packed number exceeds 32 bits"));
                }
                value = self.next()?;
            }
            for _ in 0..zeros {
                value = value
                    .checked_mul(16)
                    .and_then(|v| self.next().ok().and_then(|n| v.checked_add(n)))
                    .ok_or_else(|| invalid("RISC OS packed number overflow or truncation"))?;
            }
            value
                .checked_add((13 - f) * 16 + f + 1)
                .and_then(|v| v.checked_sub(16))
                .ok_or_else(|| invalid("RISC OS packed number overflow"))
        } else if first <= f {
            Ok(first)
        } else if first <= 13 {
            Ok((first - f - 1) * 16 + self.next()? + f + 1)
        } else {
            Err(invalid("RISC OS repeat marker inside repeat count"))
        }
    }
}
pub(super) fn unpack(
    raw: &[u8],
    w: u32,
    h: u32,
    f: u8,
    black: bool,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<GlyphBitmap> {
    if !(1..=13).contains(&f) {
        return Err(invalid("RISC OS packed f must be 1..13"));
    }
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("RISC OS packed dimensions overflow"))?;
    check(pixels, l.max_pixels_per_glyph, "RISC OS packed pixels")?;
    let row = (w as usize).div_ceil(8);
    budget.add(row * h as usize * 2)?;
    let mut packed = vec![0; row * h as usize];
    let mut nyb = Nibbles { bytes: raw, at: 0 };
    let mut at = 0;
    let mut repeat = 0usize;
    let mut black = black;
    while at < pixels {
        let mut first = nyb.next()?;
        if first >= 14 {
            if repeat != 0 {
                return Err(invalid("RISC OS two repeat counts target the same row"));
            }
            repeat = if first == 15 {
                1
            } else {
                let n = nyb.next()?;
                nyb.number(n, usize::from(f))?
            };
            if repeat == 0 {
                return Err(invalid("RISC OS zero row repeat"));
            }
            first = nyb.next()?;
        }
        let mut run = nyb.number(first, usize::from(f))?;
        if run == 0 || run > pixels - at {
            return Err(invalid("RISC OS run exceeds glyph pixels"));
        }
        while run > 0 {
            let y = at / w as usize;
            let x = at % w as usize;
            let take = run.min(w as usize - x);
            if black {
                let dy = h as usize - 1 - y;
                for dx in x..x + take {
                    packed[dy * row + dx / 8] |= 128 >> (dx % 8);
                }
            }
            at += take;
            run -= take;
            if at % w as usize == 0 && repeat > 0 {
                let completed = at / w as usize;
                if repeat > h as usize - completed {
                    return Err(invalid("RISC OS row repetitions exceed glyph"));
                }
                let from = (h as usize - completed) * row;
                for k in 1..=repeat {
                    let dest = from - k * row;
                    let (before, after) = packed.split_at_mut(from);
                    before[dest..dest + row].copy_from_slice(&after[..row]);
                }
                at += repeat * w as usize;
                repeat = 0;
                if run > pixels - at {
                    return Err(invalid("RISC OS run after repetition exceeds glyph"));
                }
            }
        }
        black = !black;
    }
    if repeat != 0 {
        return Err(invalid("RISC OS unused row repetition"));
    }
    let consumed = nyb.at.div_ceil(2);
    if raw[consumed..].iter().any(|v| *v != 0) || raw.len() - consumed > 3 {
        return Err(invalid("RISC OS excess packed data beyond word padding"));
    }
    Ok(GlyphBitmap::Mono(raster(w, h, &packed, l, budget)?))
}
fn sample(g: &Glyph, x: usize, y: usize) -> u8 {
    match &g.bitmap {
        GlyphBitmap::Mono(b) => {
            if b.as_bytes()[y * b.stride() + x / 8] & (128 >> (x % 8)) == 0 {
                0
            } else {
                255
            }
        }
        GlyphBitmap::Gray8(b) => b.as_bytes()[y * b.stride() + x],
    }
}
pub(super) fn encode(
    g: &Glyph,
    depth: u8,
    v: &RiscosWriteOptions,
    budget: &mut Budget,
) -> Result<(Vec<u8>, bool)> {
    let w = g.bitmap.width() as usize;
    let h = g.bitmap.height() as usize;
    let size = w
        .checked_mul(h)
        .and_then(|v| v.checked_mul(usize::from(depth)))
        .ok_or_else(|| invalid("RISC OS bitmap size overflow"))?
        .div_ceil(8);
    budget.add(size)?;
    let mut raw = vec![0; size];
    let mut rounded = false;
    for y in 0..h {
        for x in 0..w {
            let sample = sample(g, x, h - 1 - y);
            let value = if depth == 1 {
                if matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
                    let threshold = v.gray_threshold.ok_or_else(|| {
                        invalid("RISC OS Gray8 to mono needs an explicit threshold")
                    })?;
                    rounded |= sample != 0 && sample != 255;
                    u8::from(sample >= threshold)
                } else {
                    sample / 255
                }
            } else {
                if !sample.is_multiple_of(17) {
                    if !v.quantize {
                        return Err(invalid(
                            "RISC OS 4-bpp output needs exact levels or explicit quantization",
                        ));
                    }
                    rounded = true;
                }
                ((u16::from(sample) + 8) / 17) as u8
            };
            let bit = (y * w + x) * usize::from(depth);
            raw[bit / 8] |= value << (bit % 8);
        }
    }
    Ok((raw, rounded))
}
pub(super) fn pack(raw: &[u8], pixels: usize, budget: &mut Budget) -> Result<(Vec<u8>, bool)> {
    if pixels == 0 {
        return Ok((Vec::new(), false));
    }
    budget.add(pixels.saturating_mul(2).saturating_add(64))?;
    let mut nybbles = Vec::with_capacity(pixels + 32);
    let black = raw[0] & 1 != 0;
    let mut at = 0;
    while at < pixels {
        let ink = raw[at / 8] & (1 << (at % 8)) != 0;
        let start = at;
        while at < pixels && (raw[at / 8] & (1 << (at % 8)) != 0) == ink {
            at += 1;
        }
        let run = at - start;
        if run <= 12 {
            nybbles.push(run as u8);
        } else if run <= 28 {
            nybbles.extend([13, (run - 13) as u8]);
        } else {
            let mut value = run - 13;
            let mut digits = [0u8; 16];
            let mut n = 0;
            while value > 0 {
                digits[n] = (value & 15) as u8;
                value >>= 4;
                n += 1;
            }
            nybbles.extend(std::iter::repeat_n(0, n - 1));
            nybbles.extend(digits[..n].iter().rev());
        }
    }
    let mut out = vec![0; nybbles.len().div_ceil(2)];
    for (i, n) in nybbles.into_iter().enumerate() {
        out[i / 2] |= n << ((i % 2) * 4);
    }
    Ok((out, black))
}
