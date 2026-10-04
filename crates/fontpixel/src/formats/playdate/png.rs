//! Predictable stored-DEFLATE PNG output and bounded coverage projection.
use super::*;
use std::io::Cursor;
const CRC: [u32; 256] = {
    let mut t = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut n = i as u32;
        let mut j = 0;
        while j < 8 {
            n = (n >> 1) ^ if n & 1 != 0 { 0xedb88320 } else { 0 };
            j += 1;
        }
        t[i] = n;
        i += 1;
    }
    t
};
fn crc(b: &[u8]) -> u32 {
    !b.iter()
        .fold(!0u32, |v, c| CRC[((v as u8) ^ c) as usize] ^ (v >> 8))
}
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], b: &[u8]) {
    out.extend((b.len() as u32).to_be_bytes());
    let p = out.len();
    out.extend(tag);
    out.extend(b);
    out.extend(crc(&out[p..]).to_be_bytes());
}
pub(super) fn encode(
    w: u32,
    h: u32,
    alpha: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let stride = (w as usize)
        .checked_mul(2)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("Playdate PNG row overflow"))?;
    let raw_len = stride
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("Playdate PNG size overflow"))?;
    check(raw_len, l.max_decompressed_bytes, "Playdate PNG expansion")?;
    let zlen = raw_len.saturating_add(raw_len.div_ceil(65535) * 5 + 6);
    let size = zlen.saturating_add(57);
    check(
        size,
        l.max_table_bytes.min(u32::MAX as usize),
        "Playdate PNG bytes",
    )?;
    budget.add(raw_len.saturating_add(size))?;
    let mut raw = vec![0; raw_len];
    for y in 0..h as usize {
        for x in 0..w as usize {
            raw[y * stride + 2 + x * 2] = alpha[y * w as usize + x];
        }
    }
    let mut out = Vec::with_capacity(size);
    out.extend(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0; 13];
    ihdr[..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 4;
    chunk(&mut out, b"IHDR", &ihdr);
    out.extend((zlen as u32).to_be_bytes());
    let start = out.len();
    out.extend(b"IDAT");
    out.extend([0x78, 1]);
    let (mut a, mut b) = (1u32, 0u32);
    let blocks = raw.len().div_ceil(65535);
    for (i, block) in raw.chunks(65535).enumerate() {
        out.push(u8::from(i + 1 == blocks));
        let n = block.len() as u16;
        out.extend(n.to_le_bytes());
        out.extend((!n).to_le_bytes());
        out.extend(block);
        for &v in block {
            a = (a + u32::from(v)) % 65521;
            b = (b + a) % 65521;
        }
    }
    out.extend(((b << 16) | a).to_be_bytes());
    out.extend(crc(&out[start..]).to_be_bytes());
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}
pub(super) fn decode(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<(u32, u32, Vec<u8>)> {
    if !b.starts_with(b"\x89PNG\r\n\x1a\n") || slice(b, 12, 4)? != b"IHDR" {
        return Err(invalid("Playdate atlas is not PNG"));
    }
    let w = u32::from_be_bytes(slice(b, 16, 4)?.try_into().unwrap());
    let h = u32::from_be_bytes(slice(b, 20, 4)?.try_into().unwrap());
    if w == 0 || h == 0 {
        return Err(invalid("zero-size Playdate PNG"));
    }
    let area = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("PNG pixel overflow"))?;
    check(
        area.saturating_mul(4),
        l.max_decompressed_bytes,
        "Playdate PNG expansion",
    )?;
    let scratch = area
        .saturating_mul(16)
        .saturating_add(b.len().saturating_mul(2))
        .saturating_add(65536);
    budget.add(scratch)?;
    let mut decoder = ::png::Decoder::new(Cursor::new(b));
    decoder.set_limits(::png::Limits { bytes: scratch });
    decoder.set_ignore_text_chunk(true);
    decoder.set_transformations(::png::Transformations::EXPAND | ::png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| invalid(format!("Playdate PNG: {e}")))?;
    if reader.info().bit_depth == ::png::BitDepth::Sixteen {
        budget.loss(
            report,
            "playdate",
            "read.png_depth",
            Some(0),
            None,
            "Sixteen-bit PNG samples are reduced to eight-bit coverage",
        )?;
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| invalid("PNG decoded buffer overflow"))?;
    check(size, area.saturating_mul(4), "Playdate PNG decoded bytes")?;
    let mut data = vec![0; size];
    let info = reader
        .next_frame(&mut data)
        .map_err(|e| invalid(format!("Playdate PNG: {e}")))?;
    if info.width != w || info.height != h || info.bit_depth != ::png::BitDepth::Eight {
        return Err(invalid("unsupported Playdate PNG frame"));
    }
    let channels = info.color_type.samples();
    let mut out = Vec::with_capacity(area);
    let mut projected = false;
    for p in data[..info.buffer_size()].chunks_exact(channels) {
        let (r, g, b, a) = match info.color_type {
            ::png::ColorType::Grayscale => (p[0], p[0], p[0], 255),
            ::png::ColorType::GrayscaleAlpha => (p[0], p[0], p[0], p[1]),
            ::png::ColorType::Rgb => (p[0], p[1], p[2], 255),
            ::png::ColorType::Rgba => (p[0], p[1], p[2], p[3]),
            _ => return Err(invalid("unexpanded PNG palette")),
        };
        if a != 0 && (r != 0 || g != 0 || b != 0) {
            projected = true;
        }
        let light = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 + 500) / 1000;
        out.push(((255 - light) * u32::from(a) + 127).div_euclid(255) as u8);
    }
    if projected {
        budget.loss(
            report,
            "playdate",
            "read.png_projection",
            Some(0),
            None,
            "PNG color and opaque white are projected to alpha-weighted darkness",
        )?;
    }
    Ok((w, h, out))
}
