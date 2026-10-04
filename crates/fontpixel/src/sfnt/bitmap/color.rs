//! PNG and sbix codecs with explicit coverage projection and bounded aliases.
use super::{Budget, Limits};
use crate::Result;
use crate::bitmap::{Bitmap, Gray8};
use crate::model::GlyphBitmap;
use crate::sfnt::{bounded, buffer, bytes, invalid, p16, p32, to32, u16_at, u32_at};
use std::collections::BTreeMap;
use std::io::Cursor;

// PNG CRC-32, generated at compile time from the standard polynomial. No
// compression dictionary or unbounded encoder-internal allocations are needed.
const CRC: [u32; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = i as u32;
        let mut bit = 0;
        while bit < 8 {
            value = (value >> 1) ^ if value & 1 != 0 { 0xedb88320 } else { 0 };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
};
fn crc(data: &[u8]) -> u32 {
    !data
        .iter()
        .fold(!0u32, |v, b| CRC[((v as u8) ^ b) as usize] ^ (v >> 8))
}
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) -> Result<()> {
    p32(out, to32(data.len())?);
    let start = out.len();
    out.extend(tag);
    out.extend(data);
    p32(out, crc(&out[start..]));
    Ok(())
}

/// Predictable-size, black RGBA PNG using filter None and stored DEFLATE.
/// The caller has charged the raster and this function charges both scratch and
/// output before allocation. PNG is an interchange projection, not an RGB model.
pub(super) fn encode_png(bitmap: &GlyphBitmap, budget: &mut Budget<'_>) -> Result<Vec<u8>> {
    let (w, h) = (bitmap.width(), bitmap.height());
    if w == 0 || h == 0 {
        return Err(invalid("PNG cannot represent an empty rectangle"));
    }
    let stride = (w as usize)
        .checked_mul(4)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("PNG row overflow"))?;
    let raw_len = stride
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("PNG image overflow"))?;
    bounded(
        raw_len,
        budget.limits.sfnt.resources.max_decompressed_bytes,
        "PNG filtered bytes",
    )?;
    let zlen = raw_len
        .checked_add(raw_len.div_ceil(65535).saturating_mul(5))
        .and_then(|n| n.checked_add(6))
        .ok_or_else(|| invalid("PNG deflate overflow"))?;
    let png_len = zlen
        .checked_add(57)
        .ok_or_else(|| invalid("PNG output overflow"))?;
    to32(png_len)?;
    budget.limits.sfnt.table(png_len)?;
    budget.memory(raw_len)?;
    budget.memory(png_len)?;
    let mut raw = crate::bitmap::zeros(raw_len, budget.limits.sfnt.resources.max_total_bytes)?;
    for y in 0..h {
        for x in 0..w {
            raw[y as usize * stride + x as usize * 4 + 4] = super::sample(bitmap, x, y)?;
        }
    }
    let mut out = buffer(png_len)?;
    out.extend(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0; 13];
    ihdr[..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 6;
    chunk(&mut out, b"IHDR", &ihdr)?;
    p32(&mut out, to32(zlen)?);
    let idat = out.len();
    out.extend(b"IDAT");
    out.extend([0x78, 0x01]);
    let (mut a, mut b) = (1u32, 0u32);
    for (i, block) in raw.chunks(65535).enumerate() {
        out.push(u8::from(i + 1 == raw_len.div_ceil(65535)));
        let len = block.len() as u16;
        out.extend(len.to_le_bytes());
        out.extend((!len).to_le_bytes());
        out.extend(block);
        // 5552 bytes keeps both Adler sums inside u32 before each reduction.
        for batch in block.chunks(5552) {
            for byte in batch {
                a += u32::from(*byte);
                b += a;
            }
            a %= 65521;
            b %= 65521;
        }
    }
    p32(&mut out, b << 16 | a);
    let checksum = crc(&out[idat..]);
    p32(&mut out, checksum);
    chunk(&mut out, b"IEND", &[])?;
    debug_assert_eq!(out.len(), png_len);
    Ok(out)
}

/// Encode sparse PNG-only sbix strikes with canonical ppem/PPI ordering.
/// Zero-area records must be omitted by the caller, as required by sbix.
/// Coverage is written as black with alpha; the table's overlay flag is retained.
pub fn encode_sbix(sbix: &Sbix, num_glyphs: u16, limits: &Limits) -> Result<Vec<u8>> {
    limits.sfnt.glyphs(num_glyphs as usize)?;
    bounded(sbix.strikes.len(), limits.max_strikes, "sbix strikes")?;
    let mut budget = Budget::new(limits);
    budget.memory(sbix.strikes.len().saturating_mul(16))?;
    let mut ordered: Vec<_> = sbix.strikes.iter().collect();
    ordered.sort_by_key(|s| (s.ppem, s.ppi));
    if ordered
        .windows(2)
        .any(|v| (v[0].ppem, v[0].ppi) == (v[1].ppem, v[1].ppi))
    {
        return Err(invalid("duplicate sbix ppem/PPI strike"));
    }
    let header = 8 + ordered.len() * 4;
    limits.sfnt.table(header)?;
    budget.memory(header)?;
    let mut out = buffer(header)?;
    p16(&mut out, 1);
    p16(&mut out, 1 | u16::from(sbix.draw_outlines) << 1);
    p32(&mut out, to32(ordered.len())?);
    out.resize(header, 0);
    for (i, strike) in ordered.iter().enumerate() {
        if strike.ppem == 0 || strike.glyphs.keys().any(|gid| *gid >= num_glyphs) {
            return Err(invalid("invalid sbix ppem or glyph ID"));
        }
        let start = out.len();
        out[8 + i * 4..12 + i * 4].copy_from_slice(&to32(start)?.to_be_bytes());
        let index = 4 + (usize::from(num_glyphs) + 1) * 4;
        limits.sfnt.table(start.saturating_add(index))?;
        budget.memory(index + usize::from(num_glyphs) * 128)?;
        p16(&mut out, strike.ppem);
        p16(&mut out, strike.ppi);
        out.resize(start + index, 0);
        for gid in 0..=num_glyphs {
            budget.visit()?;
            let at = start + 4 + usize::from(gid) * 4;
            let offset = to32(out.len() - start)?;
            out[at..at + 4].copy_from_slice(&offset.to_be_bytes());
            if let Some(g) = strike.glyphs.get(&gid) {
                budget.glyph()?;
                budget.raster(g.bitmap.width(), g.bitmap.height())?;
                let png = encode_png(&g.bitmap, &mut budget)?;
                limits
                    .sfnt
                    .table(out.len().saturating_add(8).saturating_add(png.len()))?;
                budget.memory(8 + png.len())?;
                for origin in g.origin {
                    out.extend(origin.to_be_bytes());
                }
                out.extend(b"png ");
                out.extend(png);
            }
        }
    }
    decode_sbix(&out, num_glyphs, limits, false)?;
    Ok(out)
}

pub(super) fn bgra_coverage(b: [u8; 4], darkness: bool) -> Result<u8> {
    if b[..3].iter().any(|c| *c > b[3]) {
        return Err(invalid("non-premultiplied CBDT BGRA"));
    }
    Ok(if darkness {
        b[3] - ((77u32 * b[2] as u32 + 150u32 * b[1] as u32 + 29u32 * b[0] as u32 + 128) >> 8) as u8
    } else {
        b[3]
    })
}
pub(super) fn decode_png(
    data: &[u8],
    expected: Option<(u32, u32)>,
    cbdt: bool,
    darkness: bool,
    budget: &mut Budget<'_>,
) -> Result<GlyphBitmap> {
    if bytes(data, 0, 8)? != b"\x89PNG\r\n\x1a\n" {
        return Err(invalid("invalid embedded PNG signature"));
    }
    if bytes(data, 12, 4)? != b"IHDR" || u32_at(data, 8)? != 13 {
        return Err(invalid("missing embedded PNG IHDR"));
    }
    let w = u32_at(data, 16)?;
    let h = u32_at(data, 20)?;
    if expected.is_some_and(|e| e != (w, h)) || w == 0 || h == 0 {
        return Err(invalid(
            "embedded PNG dimensions disagree with bitmap metrics",
        ));
    }
    let limits = budget.raster(w, h)?;
    // Preflight all framing before the decoder can read ancillary/compressed data.
    let mut at = 8;
    let mut end = false;
    while at < data.len() {
        budget.visit()?;
        let length = u32_at(data, at)? as usize;
        let tag = bytes(data, at + 4, 4)?;
        let total = length
            .checked_add(12)
            .ok_or_else(|| invalid("PNG chunk size overflow"))?;
        bytes(data, at, total)?;
        if (cbdt
            && ![b"IHDR", b"PLTE", b"tRNS", b"sRGB", b"IDAT", b"IEND"]
                .iter()
                .any(|t| *t == tag))
            || [b"acTL", b"fcTL", b"fdAT"].iter().any(|t| *t == tag)
        {
            return Err(invalid("unsupported embedded PNG chunk/animation"));
        }
        at += total;
        if tag == b"IEND" {
            end = true;
            break;
        }
    }
    if !end || at != data.len() {
        return Err(invalid("truncated or trailing embedded PNG data"));
    }
    let max = budget
        .limits
        .sfnt
        .resources
        .max_decompressed_bytes
        .min(budget.limits.sfnt.resources.max_total_bytes);
    let size = (w as usize)
        .checked_mul(h as usize)
        .and_then(|x| x.checked_mul(4))
        .ok_or_else(|| invalid("PNG output size overflow"))?;
    bounded(size, max, "embedded PNG decompression bytes")?;
    budget.decompress(size)?;
    budget.memory(size)?;
    let decoder_limit = max.min(
        budget
            .limits
            .sfnt
            .resources
            .max_total_bytes
            .saturating_sub(budget.bytes),
    );
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(data),
        png::Limits {
            bytes: decoder_limit,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(invalid)?;
    let output_size = reader
        .output_buffer_size()
        .ok_or_else(|| invalid("PNG output overflow"))?;
    bounded(output_size, size, "embedded PNG output bytes")?;
    let mut out = crate::bitmap::zeros(output_size, max)?;
    let info = reader.next_frame(&mut out).map_err(invalid)?;
    reader.finish().map_err(invalid)?;
    if info.bit_depth != png::BitDepth::Eight || (info.width, info.height) != (w, h) {
        return Err(invalid("unsupported PNG decoded format"));
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(invalid("unexpanded PNG palette")),
    };
    let mut gray = Bitmap::<Gray8>::with_limits(w, h, limits)?;
    for (i, p) in out[..info.buffer_size()].chunks_exact(channels).enumerate() {
        let (r, g, b, a) = match channels {
            1 => (p[0], p[0], p[0], 255),
            2 => (p[0], p[0], p[0], p[1]),
            3 => (p[0], p[1], p[2], 255),
            _ => (p[0], p[1], p[2], p[3]),
        };
        let v = if darkness {
            let lum = (77u32 * r as u32 + 150u32 * g as u32 + 29u32 * b as u32 + 128) >> 8;
            (((255 - lum) * a as u32 + 127) / 255) as u8
        } else {
            a
        };
        gray.set_pixel((i % w as usize) as u32, (i / w as usize) as u32, v)?;
    }
    Ok(GlyphBitmap::Gray8(gray))
}
/// A decoded sbix glyph; placement offsets are pixels in the strike coordinate space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SbixGlyph {
    /// Signed left/bottom graphic origin offsets.
    pub origin: [i16; 2],
    /// Coverage image; RGB is projected explicitly by the selected rule.
    pub bitmap: GlyphBitmap,
}
/// One sbix strike, including only present images.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SbixStrike {
    /// Pixel size.
    pub ppem: u16,
    /// Device pixel density, preserved independently from ppem.
    pub ppi: u16,
    /// Sparse source glyph IDs.
    pub glyphs: BTreeMap<u16, SbixGlyph>,
}
/// Decoded sbix table, retaining the optional outline-overlay requirement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sbix {
    /// Whether source asks consumers to draw outlines over bitmap images.
    pub draw_outlines: bool,
    /// Physical strikes in original order.
    pub strikes: Vec<SbixStrike>,
}
/// Read PNG and `dupe` glyphs. Unsupported image tags error; aliases cannot cycle.
pub fn decode_sbix(data: &[u8], num_glyphs: u16, limits: &Limits, darkness: bool) -> Result<Sbix> {
    limits.sfnt.table(data.len())?;
    limits.sfnt.glyphs(num_glyphs as usize)?;
    if u16_at(data, 0)? != 1 {
        return Err(invalid("unsupported sbix version"));
    }
    let flags = u16_at(data, 2)?;
    if flags & !3 != 0 || flags & 1 == 0 {
        return Err(invalid("unsupported sbix flags"));
    }
    let n = u32_at(data, 4)? as usize;
    bounded(n, limits.max_strikes, "sbix strikes")?;
    let header_end = 8 + n * 4;
    bytes(data, 0, header_end)?;
    let mut budget = Budget::new(limits);
    let mut strikes = Vec::new();
    for i in 0..n {
        let start = u32_at(data, 8 + i * 4)? as usize;
        let stop = if i + 1 == n {
            data.len()
        } else {
            u32_at(data, 12 + i * 4)? as usize
        };
        if start < header_end || stop < start {
            return Err(invalid("invalid sbix strike offsets"));
        }
        let strike = bytes(data, start, stop - start)?;
        let ppem = u16_at(strike, 0)?;
        let ppi = u16_at(strike, 2)?;
        if ppem == 0 {
            return Err(invalid("zero sbix ppem"));
        }
        let head = 4 + (num_glyphs as usize + 1) * 4;
        bytes(strike, 0, head)?;
        budget.memory(num_glyphs as usize * (std::mem::size_of::<Option<SbixGlyph>>() + 32))?;
        let mut records = Vec::new();
        for gid in 0..num_glyphs as usize {
            let a = u32_at(strike, 4 + gid * 4)? as usize;
            let b = u32_at(strike, 8 + gid * 4)? as usize;
            if a < head || b < a {
                return Err(invalid("invalid sbix glyph offsets"));
            }
            let image = bytes(strike, a, b - a)?;
            if !image.is_empty() {
                budget.glyph()?;
                bytes(image, 0, 8)?;
            }
            records.push(image);
        }
        let mut decoded = vec![None; num_glyphs as usize];
        let mut active = vec![false; num_glyphs as usize];
        let mut r = SbixResolver {
            records: &records,
            decoded: &mut decoded,
            active: &mut active,
            budget: &mut budget,
            darkness,
        };
        for gid in 0..num_glyphs {
            if !records[gid as usize].is_empty() {
                r.resolve(gid, 0)?;
            }
        }
        strikes.push(SbixStrike {
            ppem,
            ppi,
            glyphs: decoded
                .into_iter()
                .enumerate()
                .filter_map(|(i, g)| g.map(|g| (i as u16, g)))
                .collect(),
        });
    }
    Ok(Sbix {
        draw_outlines: flags & 2 != 0,
        strikes,
    })
}
struct SbixResolver<'a, 'b, 'c> {
    records: &'a [&'a [u8]],
    decoded: &'b mut [Option<SbixGlyph>],
    active: &'b mut [bool],
    budget: &'b mut Budget<'c>,
    darkness: bool,
}
impl SbixResolver<'_, '_, '_> {
    fn resolve(&mut self, gid: u16, depth: usize) -> Result<()> {
        self.budget.visit()?;
        bounded(
            depth,
            self.budget.limits.max_depth.min(64),
            "sbix duplicate depth",
        )?;
        let at = gid as usize;
        let record = *self
            .records
            .get(at)
            .ok_or_else(|| invalid("sbix dupe out of range"))?;
        bytes(record, 0, 8)?;
        if self.active[at] {
            return Err(invalid("cyclic sbix dupe"));
        }
        if self.decoded[at].is_some() {
            return Ok(());
        }
        self.active[at] = true;
        let mut origin = [u16_at(record, 0)? as i16, u16_at(record, 2)? as i16];
        let bitmap = match bytes(record, 4, 4)? {
            b"png " => decode_png(&record[8..], None, false, self.darkness, self.budget)?,
            b"dupe" => {
                if record.len() != 10 {
                    return Err(invalid("invalid sbix dupe length"));
                }
                let target = u16_at(record, 8)?;
                self.resolve(target, depth + 1)?;
                let source = self.decoded[target as usize]
                    .as_ref()
                    .ok_or_else(|| invalid("missing sbix dupe image"))?;
                self.budget
                    .raster(source.bitmap.width(), source.bitmap.height())?;
                // FreeType and Apple's duplicate convention reuse the complete
                // source image record, including placement offsets.
                origin = source.origin;
                source.bitmap.clone()
            }
            _ => {
                return Err(invalid(
                    "unsupported sbix image tag (only PNG/dupe supported)",
                ));
            }
        };
        self.decoded[at] = Some(SbixGlyph { origin, bitmap });
        self.active[at] = false;
        Ok(())
    }
}
