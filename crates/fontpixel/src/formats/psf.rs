//! Linux PSF1/PSF2 cell fonts with complete aliases and Unicode sequences.
use super::console::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap};
use crate::report::Impact;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};

/// PSF wire format, independent of the source font's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Version {
    /// Eight-wide, 256/512 cells, BMP-only mappings.
    One,
    /// Arbitrary dimensions/count and full Unicode scalars.
    #[default]
    Two,
}
/// Whether to emit a Unicode table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnicodeTable {
    /// Omit only for precisely positional legacy mappings; otherwise include.
    #[default]
    Auto,
    /// Include, even if all entries are unencoded.
    Include,
    /// Omit and report any changed mappings.
    Omit,
}
/// Bounded deterministic PSF writer options.
#[derive(Debug, Clone)]
pub struct PsfWriteOptions {
    /// PSF version (default 2).
    pub version: Version,
    /// Optional PSF1 count, exactly 256 or 512. None chooses the smallest fit.
    pub psf1_count: Option<u16>,
    /// Unicode-table policy.
    pub unicode_table: UnicodeTable,
    /// Required explicit threshold for Gray8 input.
    pub gray_threshold: Option<u8>,
    /// Glyph, pixel, bitmap-table, mapping-table and cumulative budgets.
    pub limits: ResourceLimits,
    /// Maximum generated file bytes.
    pub max_output_bytes: usize,
}
impl Default for PsfWriteOptions {
    fn default() -> Self {
        Self {
            version: Version::Two,
            psf1_count: None,
            unicode_table: UnicodeTable::Auto,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 256 * 1024 * 1024,
        }
    }
}
/// Immutable registry codec for PSF1 and PSF2, including gzip transport.
#[derive(Debug, Clone, Default)]
pub struct PsfCodec {
    /// Writer options; reads use the caller's shared resource limits.
    pub write_options: PsfWriteOptions,
}
impl FormatDetector for PsfCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PsfCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (font, mut report) = parse(c.bytes(), c.limits())?;
        for e in &mut report.entries {
            e.location.asset = c.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("psf".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for PsfCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, mut report) = write(set.single()?, &self.write_options)?;
        for p in set.properties.iter() {
            diag(
                &mut report,
                "psf",
                "write.container_property",
                Impact::Loss,
                None,
                None,
                format!("Container property {:?} has no PSF field", p.name),
            );
        }
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
/// Recognize either PSF magic even if the following header is malformed.
pub fn detect(bytes: &[u8]) -> Confidence {
    if bytes.starts_with(b"\x36\x04") || bytes.starts_with(b"\x72\xb5\x4a\x86") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let b = bytes
        .get(at..at + 4)
        .ok_or_else(|| invalid("truncated PSF2 header"))?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
struct Header {
    version: Version,
    offset: usize,
    count: usize,
    width: u32,
    height: u32,
    charsize: usize,
    table: bool,
}
fn header(bytes: &[u8]) -> Result<Header> {
    if bytes.starts_with(b"\x36\x04") {
        let h = bytes
            .get(..4)
            .ok_or_else(|| invalid("truncated PSF1 header"))?;
        if h[2] & !7 != 0 {
            return Err(invalid("unknown PSF1 mode bits"));
        }
        Ok(Header {
            version: Version::One,
            offset: 4,
            count: if h[2] & 1 == 0 { 256 } else { 512 },
            width: 8,
            height: u32::from(h[3]),
            charsize: usize::from(h[3]),
            table: h[2] & 6 != 0,
        })
    } else if bytes.starts_with(b"\x72\xb5\x4a\x86") {
        if u32_at(bytes, 4)? != 0 {
            return Err(invalid("unsupported PSF2 version"));
        }
        let offset = u32_at(bytes, 8)? as usize;
        if offset < 32 || offset > bytes.len() {
            return Err(invalid("invalid PSF2 header size"));
        }
        let flags = u32_at(bytes, 12)?;
        if flags & !1 != 0 {
            return Err(invalid("unknown PSF2 flags"));
        }
        Ok(Header {
            version: Version::Two,
            offset,
            count: u32_at(bytes, 16)? as usize,
            charsize: u32_at(bytes, 20)? as usize,
            height: u32_at(bytes, 24)?,
            width: u32_at(bytes, 28)?,
            table: flags & 1 != 0,
        })
    } else {
        Err(invalid("not a PSF1/PSF2 font"))
    }
}
/// Read strict PSF geometry and Unicode grammar without guessing an encoding.
pub fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    check(bytes.len(), limits.max_input_bytes, "PSF input bytes")?;
    parse(bytes, limits)
}

fn parse(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    let mut budget = Budget::new(bytes, limits)?;
    let h = header(bytes)?;
    if h.count == 0 {
        return Err(invalid("PSF requires at least one glyph"));
    }
    check(h.count, limits.max_glyphs, "PSF glyph count")?;
    check(h.offset, limits.max_table_bytes, "PSF header bytes")?;
    let charsize = cell_size(h.width, h.height, limits)?;
    if charsize != h.charsize {
        return Err(invalid("PSF charsize disagrees with width and height"));
    }
    let size = h
        .count
        .checked_mul(charsize)
        .ok_or_else(|| limit("PSF bitmap table bytes", limits.max_table_bytes))?;
    check(size, limits.max_table_bytes, "PSF bitmap table bytes")?;
    let end = h
        .offset
        .checked_add(size)
        .ok_or_else(|| invalid("PSF bitmap offset overflow"))?;
    let bitmaps = bytes
        .get(h.offset..end)
        .ok_or_else(|| invalid("truncated PSF bitmap table"))?;
    let table = &bytes[end..];
    check(
        table.len(),
        limits.max_table_bytes,
        "PSF Unicode table bytes",
    )?;
    if !h.table && !table.is_empty() {
        return Err(invalid("unexpected bytes after table-less PSF font"));
    }
    budget.add(
        h.count
            .checked_mul(1024)
            .and_then(|n| n.checked_add(size))
            .ok_or_else(|| limit("PSF model bytes", limits.max_total_bytes))?,
    )?;
    let mut font = Font::default();
    font.source_format = Some("psf".into());
    font.metrics = metrics(h.width, h.height, 0);
    if h.table {
        font.charset.name = Some("unicode".into());
    }
    let mut report = Report::default();
    if h.offset > 32 {
        diag(
            &mut report,
            "psf",
            "read.extended_header",
            Impact::Loss,
            None,
            None,
            "PSF2 extended header bytes are not interpreted or retained",
        );
    }
    let mut at = 0;
    let stride = (h.width as usize).div_ceil(8);
    for (index, raw) in bitmaps.chunks_exact(charsize).enumerate() {
        let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
            h.width,
            h.height,
            raw,
            bitmap_limits(limits),
        )?);
        g.metrics = glyph_metrics(Bounds::new(h.width, h.height, 0, 0));
        if h.width % 8 != 0
            && raw
                .chunks_exact(stride)
                .any(|row| row[stride - 1] & ((1u8 << (8 - h.width % 8)) - 1) != 0)
        {
            budget.add(256)?;
            diag(
                &mut report,
                "psf",
                "read.padding",
                Impact::Repair,
                Some(index),
                None,
                "Unused row padding bits cleared",
            );
        }
        if h.table {
            read_entry(table, &mut at, h.version, &mut g, &mut budget)?;
        } else {
            budget.add(128)?;
            g.codepoints.push(Codepoint::Legacy(index as u32));
        }
        font.add_glyph(g)?;
    }
    if h.table && at != table.len() {
        return Err(invalid("trailing bytes after final PSF Unicode entry"));
    }
    Ok((font, report))
}
fn read_entry(
    table: &[u8],
    at: &mut usize,
    version: Version,
    g: &mut Glyph,
    budget: &mut Budget,
) -> Result<()> {
    let mut sequence: Option<Vec<u32>> = None;
    loop {
        let cp = match version {
            Version::One => {
                let b = table
                    .get(*at..at.saturating_add(2))
                    .ok_or_else(|| invalid("truncated PSF1 Unicode entry"))?;
                *at += 2;
                u32::from(u16::from_le_bytes([b[0], b[1]]))
            }
            Version::Two => {
                let first = *table
                    .get(*at)
                    .ok_or_else(|| invalid("unterminated PSF2 Unicode entry"))?;
                *at += 1;
                if first >= 0xfe {
                    u32::from(first) + 0xff00
                } else {
                    let len = match first {
                        0..=0x7f => 1,
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => return Err(invalid("invalid PSF2 UTF-8 lead byte")),
                    };
                    let end = at
                        .checked_add(len - 1)
                        .ok_or_else(|| invalid("PSF2 UTF-8 offset overflow"))?;
                    let raw = table
                        .get(*at - 1..end)
                        .ok_or_else(|| invalid("truncated PSF2 UTF-8 scalar"))?;
                    let text = std::str::from_utf8(raw)
                        .map_err(|_| invalid("invalid PSF2 UTF-8 scalar"))?;
                    *at = end;
                    // The validated lead-byte length guarantees exactly one scalar.
                    text.chars()
                        .next()
                        .ok_or_else(|| invalid("empty PSF2 scalar"))? as u32
                }
            }
        };
        // PSF2 U+FFFE/U+FFFF encoded as UTF-8 are valid scalars, not markers.
        let marker = match version {
            Version::One => cp >= 0xfffe,
            Version::Two => table.get(*at - 1).is_some_and(|b| *b == 0xfe || *b == 0xff),
        };
        if marker && (cp == 0xfffe || cp == 0xffff) {
            if let Some(seq) = sequence.take() {
                if seq.is_empty() {
                    return Err(invalid("empty PSF Unicode sequence"));
                }
                g.unicode_sequences.push(seq);
            }
            if cp == 0xffff {
                return Ok(());
            }
            budget.add(128)?;
            sequence = Some(Vec::new());
            continue;
        }
        if char::from_u32(cp).is_none() {
            return Err(invalid("invalid Unicode scalar in PSF table"));
        }
        budget.add(128)?;
        if let Some(seq) = &mut sequence {
            seq.try_reserve(1)
                .map_err(|_| invalid("PSF sequence allocation failed"))?;
            seq.push(cp);
        } else {
            g.codepoints
                .try_reserve(1)
                .map_err(|_| invalid("PSF alias allocation failed"))?;
            g.codepoints.push(Codepoint::Unicode(cp));
        }
    }
}
fn cell(font: &Font, options: &PsfWriteOptions) -> Result<Bounds> {
    let (mut left, mut bottom, mut right, mut top) = (0i64, 0i64, 0i64, 0i64);
    if let Some(b) = font.metrics.bounds {
        left = left.min(b.x);
        bottom = bottom.min(b.y);
        right = right.max(
            b.x.checked_add(i64::from(b.width))
                .ok_or_else(|| invalid("PSF font right edge overflow"))?,
        );
        top = top.max(
            b.y.checked_add(i64::from(b.height))
                .ok_or_else(|| invalid("PSF font top edge overflow"))?,
        );
    }
    for (_, g) in font.glyphs() {
        let b = g.metrics.bounds;
        left = left.min(b.x);
        bottom = bottom.min(b.y);
        right = right.max(b.x + i64::from(b.width));
        top = top.max(b.y + i64::from(b.height));
        if let Some(v) = g.metrics.advance.or(font.metrics.glyph_defaults.advance) {
            right = right.max(v.x);
        }
    }
    let mut width = u32::try_from(
        right
            .checked_sub(left)
            .ok_or_else(|| invalid("PSF cell width overflow"))?,
    )
    .map_err(|_| invalid("PSF cell width exceeds u32"))?;
    let height = u32::try_from(
        top.checked_sub(bottom)
            .ok_or_else(|| invalid("PSF cell height overflow"))?,
    )
    .map_err(|_| invalid("PSF cell height exceeds u32"))?;
    if options.version == Version::One {
        if width > 8 || height > 255 {
            return Err(invalid("PSF1 requires width <= 8 and height <= 255"));
        }
        width = 8;
    }
    cell_size(width, height, &options.limits)?;
    Ok(Bounds::new(width, height, left, bottom))
}
/// Write stable glyph positions and complete Unicode entries, returning every target loss.
pub fn write(font: &Font, options: &PsfWriteOptions) -> Result<(Vec<u8>, Report)> {
    font.validate(&options.limits)?;
    if font.is_empty() {
        return Err(invalid("PSF requires at least one glyph"));
    }
    let count = match options.version {
        Version::One => {
            let count = usize::from(options.psf1_count.unwrap_or(if font.len() <= 256 {
                256
            } else {
                512
            }));
            if ![256, 512].contains(&count) || font.len() > count {
                return Err(invalid(
                    "PSF1 requires 256 or 512 cells and cannot discard extra glyphs",
                ));
            }
            count
        }
        Version::Two => {
            if options.psf1_count.is_some() {
                return Err(invalid("psf1_count requires PSF1 output"));
            }
            font.len()
        }
    };
    check(count, options.limits.max_glyphs, "PSF output glyph count")?;
    let count32 = u32::try_from(count).map_err(|_| invalid("PSF glyph count exceeds u32"))?;
    let positional = font.glyphs().enumerate().all(|(i, (_, g))| {
        g.codepoints == [Codepoint::Legacy(i as u32)] && g.unicode_sequences.is_empty()
    });
    let table = match options.unicode_table {
        UnicodeTable::Auto => !positional,
        UnicodeTable::Include => true,
        UnicodeTable::Omit => false,
    };
    let c = cell(font, options)?;
    let charsize = cell_size(c.width, c.height, &options.limits)?;
    let size = count
        .checked_mul(charsize)
        .ok_or_else(|| limit("PSF output bitmap bytes", options.limits.max_table_bytes))?;
    check(
        size,
        options.limits.max_table_bytes,
        "PSF output bitmap bytes",
    )?;
    let header_size = if options.version == Version::One {
        4
    } else {
        32
    };
    let output_limit = options.max_output_bytes.min(options.limits.max_total_bytes);
    let fixed = size
        .checked_add(header_size)
        .ok_or_else(|| limit("PSF output bytes", output_limit))?;
    check(fixed, output_limit, "PSF output bytes")?;
    let mut report = Report::default();
    let mut out = Output::new(output_limit);
    let mut unicode = Output::new(options.limits.max_table_bytes.min(output_limit - fixed));
    let mut has_sequences = false;
    for (position, (id, g)) in font.glyphs().enumerate() {
        let index = id.index();
        glyph_losses(
            g,
            &glyph_metrics(Bounds::new(c.width, c.height, 0, 0)),
            "psf",
            index,
            &mut report,
        );
        if matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
            diag(
                &mut report,
                "psf",
                "write.gray",
                Impact::Loss,
                Some(index),
                None,
                "Gray8 raster thresholded to monochrome",
            );
        }
        if table {
            for &cp in &g.codepoints {
                if let Some(ch) = cp.scalar() {
                    if representable(ch as u32, options.version) {
                        scalar(&mut unicode, ch, options.version)?;
                    } else {
                        diag(
                            &mut report,
                            "psf",
                            "write.codepoint",
                            Impact::Loss,
                            Some(index),
                            Some(cp.value()),
                            "PSF1 cannot store this scalar",
                        );
                    }
                } else {
                    diag(
                        &mut report,
                        "psf",
                        "write.codepoint",
                        Impact::Loss,
                        Some(index),
                        Some(cp.value()),
                        "Non-Unicode or invalid mapping omitted from PSF Unicode table",
                    );
                }
            }
            for seq in &g.unicode_sequences {
                if seq.iter().any(|&cp| char::from_u32(cp).is_none()) {
                    return Err(invalid("invalid Unicode scalar in model sequence"));
                }
                if seq.iter().any(|&cp| !representable(cp, options.version)) {
                    diag(
                        &mut report,
                        "psf",
                        "write.sequence",
                        Impact::Loss,
                        Some(index),
                        None,
                        "PSF1 cannot represent the complete Unicode sequence; omitted",
                    );
                    continue;
                }
                marker(&mut unicode, 0xfe, options.version)?;
                has_sequences = true;
                for &cp in seq {
                    scalar(
                        &mut unicode,
                        char::from_u32(cp).ok_or_else(|| invalid("invalid sequence scalar"))?,
                        options.version,
                    )?;
                }
            }
            marker(&mut unicode, 0xff, options.version)?;
        } else if g.codepoints != [Codepoint::Legacy(position as u32)]
            || !g.unicode_sequences.is_empty()
        {
            diag(
                &mut report,
                "psf",
                "write.mapping",
                Impact::Loss,
                Some(index),
                None,
                "Table-less PSF replaces mappings/sequences with the positional legacy code",
            );
        }
    }
    if count > font.len() {
        diag(
            &mut report,
            "psf",
            "write.padding_glyphs",
            Impact::Loss,
            None,
            None,
            format!(
                "PSF1 adds {} blank cells to reach {count} positions",
                count - font.len()
            ),
        );
        if table {
            for _ in font.len()..count {
                marker(&mut unicode, 0xff, options.version)?;
            }
        }
    }
    match options.version {
        Version::One => {
            let mode = u8::from(count == 512)
                | if table {
                    if has_sequences { 4 } else { 2 }
                } else {
                    0
                };
            out.add(&[0x36, 4, mode, c.height as u8])?;
        }
        Version::Two => {
            out.add(b"\x72\xb5\x4a\x86")?;
            for value in [
                0,
                32,
                u32::from(table),
                count32,
                u32::try_from(charsize).map_err(|_| invalid("PSF charsize exceeds u32"))?,
                c.height,
                c.width,
            ] {
                out.add(&value.to_le_bytes())?;
            }
        }
    }
    for (_, g) in font.glyphs() {
        out.add(raster(g, c, options.gray_threshold, &options.limits)?.as_bytes())?;
    }
    // Pad using a small fixed chunk, avoiding a second count*charsize allocation.
    let mut padding = (count - font.len()) * charsize;
    while padding != 0 {
        let n = padding.min(1024);
        out.add(&[0; 1024][..n])?;
        padding -= n;
    }
    out.add(&unicode.bytes)?;
    font_losses(
        font,
        &metrics(c.width, c.height, 0),
        "psf",
        &mut report,
        false,
    );
    Ok((out.bytes, report))
}
fn representable(cp: u32, version: Version) -> bool {
    version == Version::Two || cp < 0xfffe
}
fn scalar(out: &mut Output, c: char, version: Version) -> Result<()> {
    match version {
        Version::One => out.add(&(c as u16).to_le_bytes()),
        Version::Two => {
            let mut b = [0; 4];
            out.add(c.encode_utf8(&mut b).as_bytes())
        }
    }
}
fn marker(out: &mut Output, b: u8, version: Version) -> Result<()> {
    match version {
        Version::One => out.add(&[b, 0xff]),
        Version::Two => out.add(&[b]),
    }
}
