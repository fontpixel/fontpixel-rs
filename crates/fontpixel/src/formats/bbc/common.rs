//! Shared bounded cell, budget and projection support for W5.A2 native codecs.
pub(super) use super::legacy_common::{
    Budget, audit, bitmap_limits, check, convert_gray, finish, invalid, mono, source_budget,
};
pub(super) use crate::bitmap::{Bitmap, Bounds, Mono};
pub(super) use crate::model::{Codepoint, Font, FontSet, Glyph, Spacing, Vector};
#[cfg(any(feature = "hrcg", feature = "xbin", feature = "comfont"))]
pub(super) use crate::model::{Property, PropertyValue};
pub(super) use crate::{Report, ResourceLimits, Result};
#[cfg(any(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "hrcg",
    feature = "xbin",
    feature = "zapfont",
    feature = "comfont"
))]
use std::collections::BTreeMap;
/// Interpretation of native legacy cell positions.
#[derive(Debug, Clone, Default)]
pub struct NativeReadOptions {
    /// Baseline below the top; default places the baseline at the cell bottom.
    pub baseline: Option<i64>,
    /// Explicit charset name; this never converts numeric legacy positions.
    pub charset: Option<String>,
}
/// Explicit native-cell output projection.
#[derive(Debug, Clone, Default)]
pub struct NativeWriteOptions {
    /// Optional output cell rectangle in model coordinates.
    pub cell: Option<Bounds>,
    /// Explicit threshold for Gray8 input.
    pub gray_threshold: Option<u8>,
    /// Permit Unicode numeric labels to be projected into native legacy slots.
    pub unicode_as_legacy: bool,
    /// Aggregate source, scratch, output and readback bounds.
    pub limits: ResourceLimits,
}
#[cfg(any(
    feature = "grasp",
    feature = "xbin",
    feature = "zapfont",
    feature = "comfont"
))]
pub(super) fn take(s: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    s.get(
        at..at
            .checked_add(n)
            .ok_or_else(|| invalid("native byte offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated native data"))
}
#[cfg(any(feature = "grasp", feature = "xbin", feature = "comfont"))]
pub(super) fn u16le(s: &[u8], at: usize) -> Result<u16> {
    let v = take(s, at, 2)?;
    Ok(u16::from_le_bytes([v[0], v[1]]))
}
#[cfg(feature = "zapfont")]
pub(super) fn u32le(s: &[u8], at: usize) -> Result<u32> {
    let v = take(s, at, 4)?;
    Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}
#[cfg(feature = "zapfont")]
pub(super) fn put32(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes())
}
pub(super) fn size(w: u32, h: u32, l: &ResourceLimits) -> Result<usize> {
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("native pixel count overflow"))?;
    check(pixels, l.max_pixels_per_glyph, "native glyph pixels")?;
    let n = (w as usize)
        .div_ceil(8)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("native raster size overflow"))?;
    check(n, l.max_table_bytes, "native raster bytes")?;
    Ok(n)
}
pub(super) fn allocated(n: usize, b: &mut Budget) -> Result<Vec<u8>> {
    b.add(n.saturating_mul(2))?;
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| invalid("native allocation failed"))?;
    v.resize(n, 0);
    Ok(v)
}
pub(super) fn base(id: &str, o: &NativeReadOptions, b: &mut Budget) -> Result<Font> {
    b.items::<Font>(1)?;
    b.add(512 + o.charset.as_ref().map_or(0, String::len))?;
    let mut f = Font::default();
    f.source_format = Some(id.into());
    f.charset.name = o.charset.clone();
    Ok(f)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn glyph(
    f: &mut Font,
    cp: Codepoint,
    w: u32,
    h: u32,
    raw: &[u8],
    o: &NativeReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<crate::model::GlyphId> {
    check(
        f.len().saturating_add(1),
        l.max_glyphs,
        "native glyph count",
    )?;
    let n = size(w, h, l)?;
    b.add(n.saturating_mul(2) + 2048)?;
    let mut g = Glyph::new(Bitmap::<Mono>::from_raw(w, h, raw, bitmap_limits(l))?);
    g.codepoints.push(cp);
    g.metrics.bounds.y = o
        .baseline
        .unwrap_or(i64::from(h))
        .checked_sub(i64::from(h))
        .ok_or_else(|| invalid("baseline overflow"))?;
    g.metrics.advance = Some(Vector::new(i64::from(w), 0));
    f.add_glyph(g)
}
pub(super) fn complete(mut f: Font, id: &str) -> Result<FontSet> {
    f.update_bounds()?;
    if let Some(bounds) = f.metrics.bounds {
        f.metrics.ascent = Some(bounds.y + i64::from(bounds.height));
        f.metrics.descent = Some(
            bounds
                .y
                .checked_neg()
                .ok_or_else(|| invalid("native descent overflow"))?,
        );
        f.metrics.line_height = Some(i64::from(bounds.height));
    }
    f.metrics.spacing = Some(Spacing::CharacterCell);
    let mut s = FontSet::from(f);
    s.source_format = Some(id.into());
    Ok(s)
}
#[cfg(any(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "hrcg",
    feature = "xbin",
    feature = "zapfont",
    feature = "comfont"
))]
pub(super) fn cell(f: &Font, o: &NativeWriteOptions, fixed: Option<(u32, u32)>) -> Result<Bounds> {
    let mut c = o.cell.unwrap_or(f.metrics.bounds.unwrap_or_default());
    if o.cell.is_none() && f.metrics.bounds.is_none() {
        for (_, g) in f.glyphs() {
            c.width = c.width.max(g.bitmap.width());
            c.height = c.height.max(g.bitmap.height());
        }
    }
    if let Some((w, h)) = fixed {
        if o.cell.is_some() && (c.width != w || c.height != h) {
            return Err(invalid("explicit cell disagrees with native dimensions"));
        }
        c.width = w;
        c.height = h;
    }
    if c.width == 0 || c.height == 0 {
        return Err(invalid("native cell dimensions must be positive"));
    }
    size(c.width, c.height, &o.limits)?;
    Ok(c)
}
pub(super) fn raster(
    g: Option<&Glyph>,
    c: Bounds,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<Bitmap<Mono>> {
    let n = size(c.width, c.height, l)?;
    b.add(n.saturating_mul(2))?;
    match g {
        Some(g) => mono(g)?.crop(
            c.width,
            c.height,
            c.x.checked_sub(g.metrics.bounds.x)
                .ok_or_else(|| invalid("cell x overflow"))?,
            c.y.checked_sub(g.metrics.bounds.y)
                .ok_or_else(|| invalid("cell y overflow"))?,
        ),
        None => Bitmap::<Mono>::with_limits(c.width, c.height, bitmap_limits(l)),
    }
}
#[cfg(any(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "hrcg",
    feature = "xbin",
    feature = "zapfont",
    feature = "comfont"
))]
pub(super) fn legacy_map<'a>(
    f: &'a Font,
    o: &NativeWriteOptions,
    max: u32,
    b: &mut Budget,
) -> Result<BTreeMap<u32, &'a Glyph>> {
    let mut m = BTreeMap::new();
    for (_, g) in f.glyphs() {
        for cp in &g.codepoints {
            let n = match cp {
                Codepoint::Legacy(n) => *n,
                Codepoint::Unicode(n) if o.unicode_as_legacy => *n,
                _ => continue,
            };
            if n <= max {
                b.add(128)?;
                if m.insert(n, g).is_some() {
                    return Err(invalid("conflicting native slot mappings"));
                }
            }
        }
    }
    if m.is_empty() {
        return Err(invalid(
            "native output requires legacy mappings or explicit unicode_as_legacy",
        ));
    }
    Ok(m)
}
pub(super) fn source(set: &FontSet, o: &NativeWriteOptions) -> Result<Budget> {
    let mut b = source_budget(set, &o.limits)?;
    b.add(set.source_format.as_ref().map_or(0, String::len))?;
    Ok(b)
}
#[cfg(any(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "hrcg",
    feature = "xbin",
    feature = "zapfont",
    feature = "comfont"
))]
pub(super) fn output_check(n: usize, l: &ResourceLimits, b: &mut Budget) -> Result<()> {
    check(n, l.max_input_bytes, "native output readback input bytes")?;
    b.add(n.saturating_mul(2))
}
