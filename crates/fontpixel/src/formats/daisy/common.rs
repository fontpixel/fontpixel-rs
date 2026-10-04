//! Bounded projection helpers for historical printer and workstation formats.
pub(crate) use super::legacy_common::{
    Budget, audit, bitmap_limits, check, convert_gray, finish, invalid, mono, select, source_budget,
};
#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "signum"
))]
use crate::bitmap::Bounds;
#[cfg(any(
    feature = "tasprint",
    feature = "pxl",
    feature = "signum",
    feature = "printshop"
))]
pub use crate::bitmap::ByteOrder;
use crate::bitmap::{Bitmap, Mono};
#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "pxl",
    feature = "signum"
))]
use crate::model::FontSet;
#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "signum"
))]
use crate::model::Spacing;
use crate::model::{Codepoint, Font, Glyph, Property, PropertyValue, Vector};
use crate::{ResourceLimits, Result};

pub fn source(
    format: &str,
    bytes: &[u8],
    count: usize,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Font> {
    check(bytes.len(), l.max_table_bytes, "historical source bytes")?;
    check(count, l.max_glyphs, "historical glyphs")?;
    budget.items::<Font>(1)?;
    budget.items::<Option<Glyph>>(count)?;
    budget.add(
        bytes
            .len()
            .saturating_add(count.saturating_mul(512))
            .saturating_add(2048),
    )?;
    let mut f = Font::default();
    f.source_format = Some(format.into());
    f.properties.push(Property::new(
        format!("{format}.source"),
        PropertyValue::Bytes(bytes.to_vec()),
    ));
    Ok(f)
}
#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "pxl",
    feature = "signum"
))]
pub fn set(f: Font, format: &str) -> FontSet {
    let mut s = FontSet::from(f);
    s.source_format = Some(format.into());
    s
}
#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "signum"
))]
pub fn bytes<'a>(f: &'a Font, key: &str) -> Option<&'a [u8]> {
    match f.properties.get(key) {
        Some(PropertyValue::Bytes(v)) => Some(v),
        _ => None,
    }
}
pub fn glyph(
    f: &mut Font,
    bitmap: Bitmap<Mono>,
    code: u32,
    x: i64,
    y: i64,
    advance: i64,
) -> Result<()> {
    let mut g = Glyph::new(bitmap);
    g.codepoints.push(Codepoint::Legacy(code));
    g.metrics.bounds.x = x;
    g.metrics.bounds.y = y;
    g.metrics.advance = Some(Vector::new(advance, 0));
    g.metrics.left_bearing = Some(x);
    g.metrics.right_bearing = Some(advance - x - i64::from(g.bitmap.width()));
    f.add_glyph(g)?;
    Ok(())
}
#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "signum"
))]
pub fn metrics(f: &mut Font, baseline: i64, height: i64) {
    f.metrics.ascent = Some(baseline);
    f.metrics.descent = Some(height - baseline);
    f.metrics.line_height = Some(height);
    f.metrics.pixel_size = u32::try_from(height).ok();
    f.metrics.spacing = Some(Spacing::Proportional);
    f.metrics.space_width = f
        .glyph_for_codepoint(Codepoint::Legacy(32))
        .and_then(|g| g.metrics.advance.map(|v| v.x));
    let mut left = 0;
    let mut right = 0;
    let mut bottom = baseline - height;
    let mut top = baseline;
    for (_, g) in f.glyphs() {
        left = left.min(g.metrics.bounds.x);
        right = right.max(g.metrics.bounds.x + i64::from(g.bitmap.width()));
        bottom = bottom.min(g.metrics.bounds.y);
        top = top.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
    }
    f.metrics.bounds = Some(Bounds::new(
        (right - left) as u32,
        (top - bottom).max(0) as u32,
        left,
        bottom,
    ));
}
pub fn raster(
    w: u32,
    h: u32,
    raw: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Bitmap<Mono>> {
    check(
        (w as usize)
            .checked_mul(h as usize)
            .ok_or_else(|| invalid("historical pixel overflow"))?,
        l.max_pixels_per_glyph,
        "historical glyph pixels",
    )?;
    budget.add(raw.len())?;
    Bitmap::<Mono>::from_raw(w, h, raw, bitmap_limits(l))
}
#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "tasprint",
    feature = "signum"
))]
pub fn project(
    g: Option<&Glyph>,
    bounds: Bounds,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let stride = (bounds.width as usize).div_ceil(8);
    let n = stride
        .checked_mul(bounds.height as usize)
        .ok_or_else(|| invalid("historical raster overflow"))?;
    check(
        (bounds.width as usize)
            .checked_mul(bounds.height as usize)
            .ok_or_else(|| invalid("historical pixels overflow"))?,
        l.max_pixels_per_glyph,
        "historical glyph pixels",
    )?;
    budget.add(n)?;
    let mut out = vec![0; n];
    let Some(g) = g else {
        return Ok(out);
    };
    let b = mono(g)?;
    if b.width() == 0 || b.height() == 0 {
        return Ok(out);
    }
    let left = g.metrics.bounds.x - bounds.x;
    let top = bounds.y + i64::from(bounds.height) - g.metrics.bounds.y - i64::from(b.height());
    if left < 0
        || top < 0
        || left + i64::from(b.width()) > i64::from(bounds.width)
        || top + i64::from(b.height()) > i64::from(bounds.height)
    {
        return Err(invalid("historical target cell would clip glyph"));
    }
    let left = left as usize;
    let top = top as usize;
    let shift = left % 8;
    for y in 0..b.height() as usize {
        for (j, &v) in b.as_bytes()[y * b.stride()..(y + 1) * b.stride()]
            .iter()
            .enumerate()
        {
            let at = (top + y) * stride + left / 8 + j;
            out[at] |= v >> shift;
            if shift != 0 && left / 8 + j + 1 < stride {
                out[at + 1] |= v << (8 - shift);
            }
        }
    }
    Ok(out)
}
#[cfg(any(feature = "daisy", feature = "polyprint", feature = "tasprint"))]
pub fn from_columns(
    data: &[u8],
    w: u32,
    h: u32,
    interlaced: bool,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Bitmap<Mono>> {
    let stride = (w as usize).div_ceil(8);
    let n = stride
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("column raster overflow"))?;
    check(
        (w as usize)
            .checked_mul(h as usize)
            .ok_or_else(|| invalid("column pixel overflow"))?,
        l.max_pixels_per_glyph,
        "column glyph pixels",
    )?;
    let expected = (w as usize)
        .checked_mul((h as usize).div_ceil(8))
        .ok_or_else(|| invalid("column byte overflow"))?;
    if data.len() != expected {
        return Err(invalid("column raster byte count mismatch"));
    }
    budget.add(n)?;
    let mut packed = vec![0; n];
    for y in 0..h as usize {
        let (pass, bit) = if interlaced {
            ((y / 16) * 2 + y % 2, (y % 16) / 2)
        } else {
            (y / 8, y % 8)
        };
        for x in 0..w as usize {
            if data[pass * w as usize + x] & (128 >> bit) != 0 {
                packed[y * stride + x / 8] |= 128 >> (x % 8);
            }
        }
    }
    raster(w, h, &packed, l, budget)
}
#[cfg(any(feature = "daisy", feature = "polyprint", feature = "tasprint"))]
pub fn to_columns(
    raw: &[u8],
    w: u32,
    h: u32,
    interlaced: bool,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let stride = (w as usize).div_ceil(8);
    let n = w as usize * (h as usize).div_ceil(8);
    budget.add(n)?;
    let mut out = vec![0; n];
    for y in 0..h as usize {
        let (pass, bit) = if interlaced {
            ((y / 16) * 2 + y % 2, (y % 16) / 2)
        } else {
            (y / 8, y % 8)
        };
        for x in 0..w as usize {
            if raw[y * stride + x / 8] & (128 >> (x % 8)) != 0 {
                out[pass * w as usize + x] |= 128 >> bit;
            }
        }
    }
    Ok(out)
}
pub fn output_limit(n: usize, max: usize, l: &ResourceLimits, budget: &mut Budget) -> Result<()> {
    check(
        n,
        max.min(l.max_input_bytes).min(l.max_table_bytes),
        "historical output bytes",
    )?;
    budget.add(n)
}

/// Check a bounded byte range before deriving any slices.
pub fn slice(b: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    b.get(
        at..at
            .checked_add(n)
            .ok_or_else(|| invalid("historical offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated historical font"))
}
#[cfg(any(feature = "tasprint", feature = "pxl",))]
pub fn u16_at(b: &[u8], at: usize, order: ByteOrder) -> Result<u16> {
    let p = slice(b, at, 2)?;
    Ok(match order {
        ByteOrder::Little => u16::from_le_bytes([p[0], p[1]]),
        ByteOrder::Big => u16::from_be_bytes([p[0], p[1]]),
    })
}
#[cfg(any(feature = "tasprint", feature = "pxl", feature = "signum"))]
pub fn u32_at(b: &[u8], at: usize, order: ByteOrder) -> Result<u32> {
    let p = slice(b, at, 4)?;
    Ok(match order {
        ByteOrder::Little => u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
        ByteOrder::Big => u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
    })
}
#[cfg(any(feature = "tasprint", feature = "pxl", feature = "printshop"))]
pub fn put16(b: &mut [u8], at: usize, value: u16, order: ByteOrder) {
    b[at..at + 2].copy_from_slice(&match order {
        ByteOrder::Little => value.to_le_bytes(),
        ByteOrder::Big => value.to_be_bytes(),
    });
}
#[cfg(any(feature = "tasprint", feature = "pxl", feature = "signum"))]
pub fn put32(b: &mut [u8], at: usize, value: u32, order: ByteOrder) {
    b[at..at + 4].copy_from_slice(&match order {
        ByteOrder::Little => value.to_le_bytes(),
        ByteOrder::Big => value.to_be_bytes(),
    });
}
#[cfg(any(feature = "polyprint", feature = "printshop"))]
pub fn latin(b: &[u8]) -> String {
    b.iter().map(|&v| char::from(v)).collect()
}
