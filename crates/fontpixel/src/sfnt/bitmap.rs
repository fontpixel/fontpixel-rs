//! Bounded EBDT/EBLC, bdat/bloc and EBSC table codecs.
//!
//! IDs are the enclosing sfnt's IDs, not Unicode values. Missing images remain
//! absent; EBSC records describe scaling without manufacturing new strikes.
use super::{
    Limits as SfntLimits, bounded, buffer, bytes, invalid, p16, p32, to32, u16_at, u32_at,
};
use crate::Result;
use crate::bitmap::{Bitmap, BitmapLimits, Gray8};
use crate::model::GlyphBitmap;
use std::collections::BTreeMap;

#[cfg(feature = "color")]
pub mod color;

/// Resource, graph and image-work budgets shared by embedded bitmap readers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Shared sfnt and model budgets.
    pub sfnt: SfntLimits,
    /// Maximum physical strikes or scaling instructions.
    pub max_strikes: usize,
    /// Maximum nested composite or duplicate references.
    pub max_depth: usize,
    /// Cumulative pixels decoded/copied, including repeated component work.
    pub max_pixel_work: usize,
    /// Cumulative glyph/component visits, including zero-area images.
    pub max_visits: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sfnt: SfntLimits::default(),
            max_strikes: 256,
            max_depth: 32,
            max_pixel_work: 256 * 1024 * 1024,
            max_visits: 1_000_000,
        }
    }
}
/// Wire family; color decoding requires the `color` feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// EBDT/EBLC version 2.
    OpenType,
    /// bdat/bloc version 2.
    Apple,
    /// CBDT/CBLC version 3 (PNG writing requires `color`).
    Color,
}
impl Kind {
    /// Data and location tags, respectively.
    pub const fn tags(self) -> ([u8; 4], [u8; 4]) {
        match self {
            Self::OpenType => (*b"EBDT", *b"EBLC"),
            Self::Apple => (*b"bdat", *b"bloc"),
            Self::Color => (*b"CBDT", *b"CBLC"),
        }
    }
    fn version(self) -> u32 {
        if self == Self::Color {
            0x30000
        } else {
            0x20000
        }
    }
}
/// Exact 12-byte sbit line record, including caret/extrema and padding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineMetrics(pub [u8; 12]);
impl LineMetrics {
    /// Signed ascent.
    pub fn ascender(self) -> i8 {
        self.0[0] as i8
    }
    /// Signed descent (normally negative).
    pub fn descender(self) -> i8 {
        self.0[1] as i8
    }
}
/// One layout direction's bearings and advance, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bearing {
    /// Left edge relative to that layout origin.
    pub x: i8,
    /// Top edge relative to that layout origin.
    pub y: i8,
    /// Positive advance magnitude; vertical text advances downward.
    pub advance: u8,
}
/// Native interpretation of small or big embedded glyph metrics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    /// Pixel width, including intentional white margins.
    pub width: u8,
    /// Pixel height.
    pub height: u8,
    /// Horizontal metrics, absent for vertical-only small metrics.
    pub horizontal: Option<Bearing>,
    /// Vertical metrics, absent for horizontal-only small metrics.
    pub vertical: Option<Bearing>,
}
/// A decoded glyph, without any claim to preserve original compression/layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapGlyph {
    /// Exact embedded metrics.
    pub metrics: Metrics,
    /// Mono or coverage Gray8, with original rectangle dimensions.
    pub bitmap: GlyphBitmap,
}
/// An embedded bitmap strike, keyed by enclosing sfnt glyph ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strike {
    /// Horizontal ppem, nonzero.
    pub ppem_x: u8,
    /// Vertical ppem, nonzero.
    pub ppem_y: u8,
    /// Source depth; write supports 1, 2, 4, 8, or 32 for CBDT.
    pub bit_depth: u8,
    /// Small-metric direction: 1 horizontal, 2 vertical; 3 only for big metrics.
    pub flags: u8,
    /// Horizontal line record.
    pub horizontal: LineMetrics,
    /// Vertical line record.
    pub vertical: LineMetrics,
    /// Only glyphs with actual embedded images; no synthesized missing glyphs.
    pub glyphs: BTreeMap<u16, BitmapGlyph>,
}
/// Encoded bitmap pair; insert using `kind.tags()` into a shared sfnt builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    /// EBDT or bdat bytes.
    pub data: Vec<u8>,
    /// EBLC or bloc bytes.
    pub location: Vec<u8>,
}
/// An EBSC target-size instruction; this is not an additional physical strike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scale {
    /// Horizontal target line metrics.
    pub horizontal: LineMetrics,
    /// Vertical target line metrics.
    pub vertical: LineMetrics,
    /// Requested horizontal/vertical ppem.
    pub target: [u8; 2],
    /// Existing source strike horizontal/vertical ppem.
    pub source: [u8; 2],
}
/// Read version-2 EBSC records without silently scaling glyph pixels.
pub fn decode_scaling(data: &[u8], limits: &Limits) -> Result<Vec<Scale>> {
    limits.sfnt.table(data.len())?;
    if u32_at(data, 0)? != 0x20000 {
        return Err(invalid("unsupported EBSC version"));
    }
    let n = u32_at(data, 4)? as usize;
    bounded(n, limits.max_strikes, "bitmap scaling records")?;
    if data.len() != 8 + n * 28 {
        return Err(invalid("invalid EBSC length"));
    }
    let mut out = Vec::new();
    let mut targets = std::collections::BTreeSet::new();
    for b in data[8..].as_chunks::<28>().0 {
        let target = [b[24], b[25]];
        let source = [b[26], b[27]];
        if target.contains(&0) || source.contains(&0) || !targets.insert(target) {
            return Err(invalid("invalid or duplicate EBSC size"));
        }
        out.push(Scale {
            horizontal: line(b, 0)?,
            vertical: line(b, 12)?,
            target,
            source,
        });
    }
    Ok(out)
}
/// Encode complete, editable EBSC records with deterministic ordering.
pub fn encode_scaling(scales: &[Scale], limits: &Limits) -> Result<Vec<u8>> {
    bounded(scales.len(), limits.max_strikes, "bitmap scaling records")?;
    limits.sfnt.table(8 + scales.len() * 28)?;
    let mut out = buffer(8 + scales.len() * 28)?;
    p32(&mut out, 0x20000);
    p32(&mut out, to32(scales.len())?);
    for s in scales {
        out.extend(s.horizontal.0);
        out.extend(s.vertical.0);
        out.extend(s.target);
        out.extend(s.source);
    }
    decode_scaling(&out, limits)?;
    Ok(out)
}
fn line(data: &[u8], at: usize) -> Result<LineMetrics> {
    let mut raw = [0; 12];
    raw.copy_from_slice(bytes(data, at, 12)?);
    Ok(LineMetrics(raw))
}
fn metrics(data: &[u8], big: bool, flags: u8) -> Result<Metrics> {
    let d = bytes(data, 0, if big { 8 } else { 5 })?;
    let bearing = Bearing {
        x: d[2] as i8,
        y: d[3] as i8,
        advance: d[4],
    };
    let (horizontal, vertical) = if big {
        (
            Some(bearing),
            Some(Bearing {
                x: d[5] as i8,
                y: d[6] as i8,
                advance: d[7],
            }),
        )
    } else if flags == 1 {
        (Some(bearing), None)
    } else if flags == 2 {
        (
            None,
            Some(Bearing {
                x: bearing.y,
                y: bearing.x,
                advance: bearing.advance,
            }),
        )
    } else {
        return Err(invalid("small metrics need one unambiguous direction"));
    };
    Ok(Metrics {
        width: d[1],
        height: d[0],
        horizontal,
        vertical,
    })
}
fn put_metrics(out: &mut Vec<u8>, m: Metrics, flags: u8) -> Result<u16> {
    out.extend([m.height, m.width]);
    match (m.horizontal, m.vertical) {
        (Some(h), Some(v)) => {
            out.extend([
                h.x as u8, h.y as u8, h.advance, v.x as u8, v.y as u8, v.advance,
            ]);
            Ok(6)
        }
        (Some(h), None) if flags == 1 => {
            out.extend([h.x as u8, h.y as u8, h.advance]);
            Ok(1)
        }
        (None, Some(v)) if flags == 2 => {
            out.extend([v.y as u8, v.x as u8, v.advance]);
            Ok(1)
        }
        _ => Err(invalid(
            "metrics are absent or inconsistent with strike direction",
        )),
    }
}
#[derive(Clone, Copy)]
struct Record<'a> {
    data: &'a [u8],
    format: u16,
    shared: Option<Metrics>,
}
struct Budget<'a> {
    limits: &'a Limits,
    bytes: usize,
    pixels: usize,
    visits: usize,
    glyphs: usize,
    #[cfg(feature = "color")]
    decompressed: usize,
}
impl<'a> Budget<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            bytes: 0,
            pixels: 0,
            visits: 0,
            glyphs: 0,
            #[cfg(feature = "color")]
            decompressed: 0,
        }
    }
    fn add(value: &mut usize, amount: usize, max: usize, name: &'static str) -> Result<()> {
        *value = value
            .checked_add(amount)
            .ok_or_else(|| invalid("bitmap budget overflow"))?;
        bounded(*value, max, name)
    }
    fn memory(&mut self, n: usize) -> Result<()> {
        Self::add(
            &mut self.bytes,
            n,
            self.limits.sfnt.resources.max_total_bytes,
            "embedded bitmap model bytes",
        )
    }
    #[cfg(feature = "color")]
    fn decompress(&mut self, n: usize) -> Result<()> {
        Self::add(
            &mut self.decompressed,
            n,
            self.limits.sfnt.resources.max_decompressed_bytes,
            "embedded PNG cumulative decompression bytes",
        )
    }
    fn work(&mut self, n: usize) -> Result<()> {
        Self::add(
            &mut self.pixels,
            n,
            self.limits.max_pixel_work,
            "embedded bitmap pixel work",
        )
    }
    fn visit(&mut self) -> Result<()> {
        Self::add(
            &mut self.visits,
            1,
            self.limits.max_visits,
            "embedded bitmap visits",
        )
    }
    fn glyph(&mut self) -> Result<()> {
        Self::add(
            &mut self.glyphs,
            1,
            self.limits.sfnt.resources.max_glyphs,
            "embedded bitmap glyphs",
        )?;
        self.memory(512)
    }
    fn raster(&mut self, w: u32, h: u32) -> Result<BitmapLimits> {
        let n = (w as usize)
            .checked_mul(h as usize)
            .ok_or_else(|| invalid("bitmap size overflow"))?;
        bounded(
            n,
            self.limits.sfnt.resources.max_pixels_per_glyph,
            "embedded glyph pixels",
        )?;
        self.work(n)?;
        self.memory(n)?;
        Ok(BitmapLimits {
            max_pixels: self.limits.sfnt.resources.max_pixels_per_glyph,
            max_bytes: self.limits.sfnt.resources.max_total_bytes,
            ..BitmapLimits::default()
        })
    }
}
/// Decode all physical strikes, including sparse indexes and bounded composites.
/// Color images default to alpha coverage and require `color`; see `decode_with_color`.
pub fn decode(
    location: &[u8],
    data: &[u8],
    kind: Kind,
    num_glyphs: u16,
    limits: &Limits,
) -> Result<Vec<Strike>> {
    decode_impl(location, data, kind, num_glyphs, limits, false)
}
/// Decode using alpha-weighted darkness rather than alpha for color images.
/// Color projection is intrinsically lossy; native codec reports that loss.
#[cfg(feature = "color")]
pub fn decode_with_color(
    location: &[u8],
    data: &[u8],
    kind: Kind,
    num_glyphs: u16,
    limits: &Limits,
    darkness: bool,
) -> Result<Vec<Strike>> {
    decode_impl(location, data, kind, num_glyphs, limits, darkness)
}
fn decode_impl(
    location: &[u8],
    data: &[u8],
    kind: Kind,
    num_glyphs: u16,
    limits: &Limits,
    darkness: bool,
) -> Result<Vec<Strike>> {
    limits.sfnt.glyphs(num_glyphs as usize)?;
    limits.sfnt.table(location.len())?;
    limits.sfnt.table(data.len())?;
    if u32_at(location, 0)? != kind.version() || u32_at(data, 0)? != kind.version() {
        return Err(invalid("bitmap table version mismatch"));
    }
    let count = u32_at(location, 4)? as usize;
    bounded(count, limits.max_strikes, "embedded bitmap strikes")?;
    let head_end = 8 + count * 48;
    bytes(location, 0, head_end)?;
    let mut budget = Budget::new(limits);
    budget.memory(count * std::mem::size_of::<Strike>())?;
    let mut result = Vec::new();
    for s in 0..count {
        let size = bytes(location, 8 + s * 48, 48)?;
        let offset = u32_at(size, 0)? as usize;
        let length = u32_at(size, 4)? as usize;
        let n = u32_at(size, 8)? as usize;
        bounded(n, num_glyphs as usize, "bitmap index subtables")?;
        if offset < head_end || u32_at(size, 12)? != 0 {
            return Err(invalid(
                "invalid bitmap index offset or unsupported colorRef",
            ));
        }
        let indexes = bytes(location, offset, length)?;
        bytes(indexes, 0, n * 8)?;
        let first = u16_at(size, 40)?;
        let last = u16_at(size, 42)?;
        if size[44] == 0
            || size[45] == 0
            || !(1..=3).contains(&size[47])
            || first > last
            || last >= num_glyphs
        {
            return Err(invalid("invalid bitmap strike size, flags or glyph range"));
        }
        let depth = size[46];
        if ![1, 2, 4, 8].contains(&depth) && !(kind == Kind::Color && depth == 32) {
            return Err(invalid("unsupported bitmap depth"));
        }
        budget.memory(
            num_glyphs as usize
                * (std::mem::size_of::<Option<Record<'_>>>()
                    + std::mem::size_of::<Option<BitmapGlyph>>()
                    + 1),
        )?;
        let mut records = vec![None; num_glyphs as usize];
        let mut previous = None;
        for i in 0..n {
            let a = i * 8;
            let lo = u16_at(indexes, a)?;
            let hi = u16_at(indexes, a + 2)?;
            let sub_offset = u32_at(indexes, a + 4)? as usize;
            if lo < first
                || hi > last
                || lo > hi
                || previous.is_some_and(|p| lo <= p)
                || sub_offset < n * 8
            {
                return Err(invalid(
                    "overlapping, unsorted or out-of-range bitmap index",
                ));
            }
            previous = Some(hi);
            let sub = bytes(
                indexes,
                sub_offset,
                indexes
                    .len()
                    .checked_sub(sub_offset)
                    .ok_or_else(|| invalid("bitmap index offset"))?,
            )?;
            let index_format = u16_at(sub, 0)?;
            let image_format = u16_at(sub, 2)?;
            if ![1, 2, 5, 6, 7, 8, 9].contains(&image_format)
                && !(kind == Kind::Color && [17, 18, 19].contains(&image_format))
            {
                return Err(invalid(format!(
                    "unsupported bitmap image format {image_format}"
                )));
            }
            let base = u32_at(sub, 4)? as usize;
            if base < 4 || base > data.len() {
                return Err(invalid("bitmap data offset outside image data"));
            }
            let mut add =
                |gid: u16, begin: usize, end: usize, shared: Option<Metrics>| -> Result<()> {
                    budget.visit()?;
                    if gid < lo || gid > hi || end < begin {
                        return Err(invalid("invalid bitmap glyph offset or ID"));
                    }
                    let at = base
                        .checked_add(begin)
                        .ok_or_else(|| invalid("bitmap offset overflow"))?;
                    let image = bytes(data, at, end - begin)?;
                    // Fixed-size empty images may still have real metrics; variable zero spans are absent.
                    if !image.is_empty() || shared.is_some() {
                        budget.glyph()?;
                        if records[gid as usize]
                            .replace(Record {
                                data: image,
                                format: image_format,
                                shared,
                            })
                            .is_some()
                        {
                            return Err(invalid("duplicate bitmap glyph"));
                        }
                    }
                    Ok(())
                };
            let total = usize::from(hi - lo) + 1;
            match index_format {
                1 | 3 => {
                    let width = if index_format == 1 { 4 } else { 2 };
                    bytes(sub, 8, (total + 1) * width)?;
                    let offset = |i| {
                        if width == 4 {
                            u32_at(sub, 8 + i * width).map(|x| x as usize)
                        } else {
                            u16_at(sub, 8 + i * width).map(usize::from)
                        }
                    };
                    for j in 0..total {
                        add(lo + j as u16, offset(j)?, offset(j + 1)?, None)?;
                    }
                }
                2 | 5 => {
                    let image_size = u32_at(sub, 8)? as usize;
                    let m = metrics(bytes(sub, 12, 8)?, true, size[47])?;
                    if ![5, 19].contains(&image_format) {
                        return Err(invalid("constant index requires shared-metric image"));
                    }
                    let count = if index_format == 2 {
                        total
                    } else {
                        u32_at(sub, 20)? as usize
                    };
                    if count > total {
                        return Err(invalid("too many sparse glyphs"));
                    }
                    if index_format == 5 {
                        bytes(sub, 24, count * 2)?;
                    }
                    let mut previous = None;
                    for j in 0..count {
                        let gid = if index_format == 2 {
                            lo + j as u16
                        } else {
                            u16_at(sub, 24 + j * 2)?
                        };
                        if previous.is_some_and(|p| gid <= p) {
                            return Err(invalid("unsorted sparse bitmap IDs"));
                        }
                        previous = Some(gid);
                        let begin = j
                            .checked_mul(image_size)
                            .ok_or_else(|| invalid("bitmap image multiplication overflow"))?;
                        let end = begin
                            .checked_add(image_size)
                            .ok_or_else(|| invalid("bitmap image end overflow"))?;
                        add(gid, begin, end, Some(m))?;
                    }
                }
                4 => {
                    let count = u32_at(sub, 8)? as usize;
                    if count > total {
                        return Err(invalid("too many sparse glyphs"));
                    }
                    bytes(sub, 12, (count + 1) * 4)?;
                    let mut previous = None;
                    for j in 0..count {
                        let gid = u16_at(sub, 12 + j * 4)?;
                        if previous.is_some_and(|p| gid <= p) {
                            return Err(invalid("unsorted sparse bitmap IDs"));
                        }
                        previous = Some(gid);
                        add(
                            gid,
                            u16_at(sub, 14 + j * 4)? as usize,
                            u16_at(sub, 18 + j * 4)? as usize,
                            None,
                        )?;
                    }
                }
                _ => return Err(invalid("unsupported bitmap index format")),
            }
        }
        let mut decoded = vec![None; num_glyphs as usize];
        let mut active = vec![false; num_glyphs as usize];
        let mut glyphs = BTreeMap::new();
        let mut resolver = Resolver {
            records: &records,
            decoded: &mut decoded,
            active: &mut active,
            depth,
            flags: size[47],
            darkness,
            budget: &mut budget,
        };
        for gid in 0..num_glyphs {
            if records[gid as usize].is_some() {
                resolver.resolve(gid, 0)?;
            }
        }
        for (gid, glyph) in decoded.into_iter().enumerate() {
            if let Some(glyph) = glyph {
                glyphs.insert(gid as u16, glyph);
            }
        }
        result.push(Strike {
            ppem_x: size[44],
            ppem_y: size[45],
            bit_depth: depth,
            flags: size[47],
            horizontal: line(size, 16)?,
            vertical: line(size, 28)?,
            glyphs,
        });
    }
    Ok(result)
}
struct Resolver<'a, 'b, 'c> {
    records: &'a [Option<Record<'a>>],
    decoded: &'b mut [Option<BitmapGlyph>],
    active: &'b mut [bool],
    depth: u8,
    flags: u8,
    darkness: bool,
    budget: &'b mut Budget<'c>,
}
impl Resolver<'_, '_, '_> {
    fn resolve(&mut self, gid: u16, nesting: usize) -> Result<()> {
        self.budget.visit()?;
        bounded(
            nesting,
            self.budget.limits.max_depth.min(64),
            "bitmap composite depth",
        )?;
        let at = gid as usize;
        let record = self
            .records
            .get(at)
            .copied()
            .flatten()
            .ok_or_else(|| invalid("missing bitmap component"))?;
        if self.active[at] {
            return Err(invalid("cyclic bitmap composite"));
        }
        if self.decoded[at].is_some() {
            return Ok(());
        }
        self.active[at] = true;
        let big = matches!(record.format, 6 | 7 | 9 | 18);
        let (m, offset) = if matches!(record.format, 5 | 19) {
            (
                record
                    .shared
                    .ok_or_else(|| invalid("missing shared bitmap metrics"))?,
                0,
            )
        } else {
            (
                metrics(record.data, big, self.flags)?,
                if big { 8 } else { 5 },
            )
        };
        let w = u32::from(m.width);
        let h = u32::from(m.height);
        let limits = self.budget.raster(w, h)?;
        let bitmap = if matches!(record.format, 8 | 9) {
            if self.depth == 32 && self.darkness {
                return Err(invalid(
                    "darkness projection of BGRA composites requires an RGBA model; use alpha coverage",
                ));
            }
            let pos = if record.format == 8 { 6 } else { 8 };
            let count = u16_at(record.data, pos)? as usize;
            let components = bytes(record.data, pos + 2, count * 4)?;
            let mut canvas = Bitmap::<Gray8>::with_limits(w, h, limits)?;
            for component in components.as_chunks::<4>().0 {
                let child = u16::from_be_bytes([component[0], component[1]]);
                self.resolve(child, nesting + 1)?;
                let child = self.decoded[child as usize]
                    .as_ref()
                    .ok_or_else(|| invalid("missing resolved bitmap"))?;
                let x = component[2] as i8 as i32;
                let y = component[3] as i8 as i32;
                if x < 0
                    || y < 0
                    || x as u32 + child.bitmap.width() > w
                    || y as u32 + child.bitmap.height() > h
                {
                    return Err(invalid("bitmap component outside parent rectangle"));
                }
                self.budget
                    .work(child.bitmap.width() as usize * child.bitmap.height() as usize)?;
                for yy in 0..child.bitmap.height() {
                    for xx in 0..child.bitmap.width() {
                        let v = sample(&child.bitmap, xx, yy)?;
                        let tx = x as u32 + xx;
                        let ty = y as u32 + yy;
                        canvas.set_pixel(tx, ty, canvas.get_pixel(tx, ty)? | v)?;
                    }
                }
            }
            if self.depth == 1 {
                GlyphBitmap::Mono(canvas.to_mono(128)?)
            } else {
                GlyphBitmap::Gray8(canvas)
            }
        } else if [17, 18, 19].contains(&record.format) {
            #[cfg(feature = "color")]
            {
                let size = u32_at(record.data, offset)? as usize;
                let png = bytes(record.data, offset + 4, size)?;
                color::decode_png(png, Some((w, h)), true, self.darkness, self.budget)?
            }
            #[cfg(not(feature = "color"))]
            {
                return Err(invalid("color PNG decoding requires color feature"));
            }
        } else {
            let bits = self.depth as usize;
            let row_bits = w as usize * bits;
            let stride_bits = if matches!(record.format, 1 | 6) {
                row_bits.div_ceil(8) * 8
            } else {
                row_bits
            };
            let n = (stride_bits * h as usize).div_ceil(8);
            let image = bytes(record.data, offset, n)?;
            let mut canvas = Bitmap::<Gray8>::with_limits(w, h, limits)?;
            for y in 0..h {
                for x in 0..w {
                    let bit = y as usize * stride_bits + x as usize * bits;
                    let v = if bits == 32 {
                        #[cfg(feature = "color")]
                        {
                            let b = bytes(image, bit / 8, 4)?;
                            color::bgra_coverage([b[0], b[1], b[2], b[3]], self.darkness)?
                        }
                        #[cfg(not(feature = "color"))]
                        {
                            return Err(invalid("BGRA decoding requires color feature"));
                        }
                    } else {
                        ((image[bit / 8] >> (8 - bits - bit % 8)) & ((1u16 << bits) - 1) as u8)
                            * (255 / ((1u16 << bits) - 1)) as u8
                    };
                    canvas.set_pixel(x, y, v)?;
                }
            }
            if self.depth == 1 {
                GlyphBitmap::Mono(canvas.to_mono(128)?)
            } else {
                GlyphBitmap::Gray8(canvas)
            }
        };
        self.decoded[at] = Some(BitmapGlyph { metrics: m, bitmap });
        self.active[at] = false;
        Ok(())
    }
}
fn sample(bitmap: &GlyphBitmap, x: u32, y: u32) -> Result<u8> {
    match bitmap {
        GlyphBitmap::Mono(b) => b.get_pixel(x, y).map(|v| v * 255),
        GlyphBitmap::Gray8(b) => b.get_pixel(x, y),
    }
}
/// Encode canonical variable-size indexes and byte-aligned images. Metrics and
/// exact gray values must fit; there is no implicit threshold or quantization.
pub fn encode(strikes: &[Strike], kind: Kind, num_glyphs: u16, limits: &Limits) -> Result<Tables> {
    if kind == Kind::Color && !cfg!(feature = "color") {
        return Err(invalid("color bitmap writing requires the color feature"));
    }
    limits.sfnt.glyphs(num_glyphs as usize)?;
    bounded(strikes.len(), limits.max_strikes, "embedded bitmap strikes")?;
    let mut budget = Budget::new(limits);
    let header_size = strikes
        .len()
        .checked_mul(48)
        .and_then(|n| n.checked_add(8))
        .ok_or_else(|| invalid("bitmap location header overflow"))?;
    limits.sfnt.table(header_size)?;
    if kind == Kind::Color {
        budget.memory(header_size.saturating_add(4))?;
    }
    let mut data = kind.version().to_be_bytes().to_vec();
    let mut location = buffer(header_size)?;
    p32(&mut location, kind.version());
    p32(&mut location, to32(strikes.len())?);
    location.resize(header_size, 0);
    let mut ordered: Vec<_> = strikes.iter().collect();
    if kind == Kind::Apple {
        ordered.sort_by_key(|s| (s.ppem_y, s.ppem_x));
    }
    for (i, s) in ordered.into_iter().enumerate() {
        if s.ppem_x == 0
            || s.ppem_y == 0
            || !(1..=3).contains(&s.flags)
            || !(if kind == Kind::Color {
                s.bit_depth == 32
            } else {
                [1, 2, 4, 8].contains(&s.bit_depth)
            })
            || s.glyphs.is_empty()
        {
            return Err(invalid("invalid or empty bitmap strike"));
        }
        let mut groups: Vec<(u16, u16, u16, usize, Vec<u32>)> = Vec::new();
        for (&gid, g) in &s.glyphs {
            if gid >= num_glyphs
                || u32::from(g.metrics.width) != g.bitmap.width()
                || u32::from(g.metrics.height) != g.bitmap.height()
            {
                return Err(invalid("bitmap ID/dimensions disagree with metrics"));
            }
            budget.glyph()?;
            budget.raster(g.bitmap.width(), g.bitmap.height())?;
            let mut encoded = Vec::new();
            let format = put_metrics(&mut encoded, g.metrics, s.flags)?;
            let format = if kind == Kind::Color {
                #[cfg(feature = "color")]
                {
                    if g.bitmap.width() == 0 || g.bitmap.height() == 0 {
                        // PNG cannot represent zero-area glyphs. CBDT permits
                        // the ordinary EBDT forms, including empty raw images.
                        format
                    } else {
                        let png = color::encode_png(&g.bitmap, &mut budget)?;
                        p32(&mut encoded, to32(png.len())?);
                        encoded.extend(png);
                        if format == 1 { 17 } else { 18 }
                    }
                }
                #[cfg(not(feature = "color"))]
                unreachable!()
            } else {
                let depth = s.bit_depth as usize;
                let stride = (g.bitmap.width() as usize * depth).div_ceil(8);
                let header = encoded.len();
                encoded.resize(header + stride * g.bitmap.height() as usize, 0);
                let step = 255 / ((1u16 << depth) - 1) as u8;
                for y in 0..g.bitmap.height() {
                    for x in 0..g.bitmap.width() {
                        let v = sample(&g.bitmap, x, y)?;
                        if !v.is_multiple_of(step) {
                            return Err(invalid(
                                "bitmap gray value cannot be represented at requested depth",
                            ));
                        }
                        let bit = x as usize * depth;
                        encoded[header + y as usize * stride + bit / 8] |=
                            (v / step) << (8 - depth - bit % 8);
                    }
                }
                format
            };
            limits.sfnt.table(
                data.len()
                    .checked_add(encoded.len())
                    .ok_or_else(|| invalid("bitmap output overflow"))?,
            )?;
            let extend = groups
                .last()
                .is_some_and(|g| g.1.checked_add(1) == Some(gid) && g.2 == format);
            if !extend {
                groups.push((gid, gid, format, data.len(), vec![0]));
            }
            let group = groups
                .last_mut()
                .ok_or_else(|| invalid("missing output group"))?;
            data.extend(encoded);
            group.1 = gid;
            group.4.push(to32(data.len() - group.3)?);
        }
        let start = location.len();
        let index_size: usize = groups.iter().map(|g| 16 + g.4.len() * 4).sum();
        limits.sfnt.table(start + index_size)?;
        location.resize(start + groups.len() * 8, 0);
        for (j, (lo, hi, format, base, offsets)) in groups.iter().enumerate() {
            let pos = start + j * 8;
            location[pos..pos + 2].copy_from_slice(&lo.to_be_bytes());
            location[pos + 2..pos + 4].copy_from_slice(&hi.to_be_bytes());
            let off = to32(location.len() - start)?;
            location[pos + 4..pos + 8].copy_from_slice(&off.to_be_bytes());
            p16(&mut location, 1);
            p16(&mut location, *format);
            p32(&mut location, to32(*base)?);
            for v in offsets {
                p32(&mut location, *v);
            }
        }
        let size = location.len() - start;
        let pos = 8 + i * 48;
        location[pos..pos + 4].copy_from_slice(&to32(start)?.to_be_bytes());
        location[pos + 4..pos + 8].copy_from_slice(&to32(size)?.to_be_bytes());
        location[pos + 8..pos + 12].copy_from_slice(&to32(groups.len())?.to_be_bytes());
        location[pos + 16..pos + 28].copy_from_slice(&s.horizontal.0);
        location[pos + 28..pos + 40].copy_from_slice(&s.vertical.0);
        let first = *s
            .glyphs
            .first_key_value()
            .ok_or_else(|| invalid("empty strike"))?
            .0;
        let last = *s
            .glyphs
            .last_key_value()
            .ok_or_else(|| invalid("empty strike"))?
            .0;
        location[pos + 40..pos + 42].copy_from_slice(&first.to_be_bytes());
        location[pos + 42..pos + 44].copy_from_slice(&last.to_be_bytes());
        location[pos + 44..pos + 48].copy_from_slice(&[s.ppem_x, s.ppem_y, s.bit_depth, s.flags]);
    }
    bounded(
        data.len() + location.len(),
        limits.sfnt.resources.max_total_bytes,
        "bitmap output bytes",
    )?;
    if kind == Kind::Color {
        budget.memory(data.len() + location.len())?;
        // This also applies the reader's PNG chunk/visit/decompression limits.
        decode(&location, &data, kind, num_glyphs, limits)?;
    }
    Ok(Tables { data, location })
}
