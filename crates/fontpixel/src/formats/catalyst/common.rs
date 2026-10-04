//! Bounded wire/model helpers used only by the C2 workstation codecs.
#[cfg(any(
    feature = "catalyst",
    feature = "dashen",
    feature = "prebuilt",
    feature = "riscos",
    feature = "xerox"
))]
pub(crate) use super::legacy_common::int;
#[cfg(any(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "hppcl",
    feature = "riscos",
    feature = "xerox"
))]
pub(crate) use super::legacy_common::select;
pub(crate) use super::legacy_common::{
    Budget, audit, bitmap_limits, check, finish, invalid, source_budget,
};
#[cfg(any(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "hppcl",
    feature = "xerox"
))]
pub(crate) use super::legacy_common::{convert_gray, mono};
pub use crate::bitmap::ByteOrder;
use crate::bitmap::{Bitmap, Bounds, Mono};
#[cfg(any(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "xerox"
))]
use crate::model::Vector;
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue};
use crate::{ResourceLimits, Result};
pub fn slice(b: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    b.get(
        at..at
            .checked_add(n)
            .ok_or_else(|| invalid("workstation offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated workstation font"))
}
pub fn u16_at(b: &[u8], at: usize, e: ByteOrder) -> Result<u16> {
    let p = slice(b, at, 2)?;
    Ok(match e {
        ByteOrder::Big => u16::from_be_bytes([p[0], p[1]]),
        ByteOrder::Little => u16::from_le_bytes([p[0], p[1]]),
    })
}
pub fn put16(b: &mut [u8], at: usize, v: u16, e: ByteOrder) {
    b[at..at + 2].copy_from_slice(&match e {
        ByteOrder::Big => v.to_be_bytes(),
        ByteOrder::Little => v.to_le_bytes(),
    });
}
#[cfg(any(
    feature = "dashen",
    feature = "prebuilt",
    feature = "riscos",
    feature = "xerox"
))]
pub fn u32_at(b: &[u8], at: usize, e: ByteOrder) -> Result<u32> {
    let p = slice(b, at, 4)?;
    Ok(match e {
        ByteOrder::Big => u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
        ByteOrder::Little => u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
    })
}
#[cfg(any(
    feature = "dashen",
    feature = "prebuilt",
    feature = "riscos",
    feature = "xerox"
))]
pub fn put32(b: &mut [u8], at: usize, v: u32, e: ByteOrder) {
    b[at..at + 4].copy_from_slice(&match e {
        ByteOrder::Big => v.to_be_bytes(),
        ByteOrder::Little => v.to_le_bytes(),
    });
}
pub fn source(
    format: &str,
    b: &[u8],
    count: usize,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Font> {
    check(b.len(), l.max_table_bytes, "workstation table bytes")?;
    check(count, l.max_glyphs, "workstation glyphs")?;
    budget.items::<Font>(1)?;
    budget.items::<Option<Glyph>>(count)?;
    budget.add(
        b.len()
            .saturating_add(count.saturating_mul(512))
            .saturating_add(2048),
    )?;
    let mut f = Font::default();
    f.source_format = Some(format.into());
    f.properties.push(Property::new(
        format!("{format}.source"),
        PropertyValue::Bytes(b.to_vec()),
    ));
    Ok(f)
}
pub fn set(fonts: Vec<Font>, format: &str) -> FontSet {
    FontSet {
        fonts,
        source_format: Some(format.into()),
        ..Default::default()
    }
}
#[cfg(any(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "hppcl",
    feature = "riscos",
    feature = "xerox"
))]
pub fn bytes<'a>(f: &'a Font, key: &str) -> Option<&'a [u8]> {
    if let Some(PropertyValue::Bytes(b)) = f.properties.get(key) {
        Some(b)
    } else {
        None
    }
}
#[cfg(any(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "xerox"
))]
pub fn glyph(f: &mut Font, b: Bitmap<Mono>, code: u32, x: i64, y: i64, advance: i64) -> Result<()> {
    let mut g = Glyph::new(b);
    g.codepoints.push(Codepoint::Legacy(code));
    g.metrics.bounds.x = x;
    g.metrics.bounds.y = y;
    g.metrics.advance = Some(Vector::new(advance, 0));
    g.metrics.left_bearing = Some(x);
    g.metrics.right_bearing = Some(advance - x - i64::from(g.bitmap.width()));
    f.add_glyph(g)?;
    Ok(())
}
pub fn metrics(f: &mut Font, line: i64) {
    let mut left = 0;
    let mut right = 0;
    let mut bottom = 0;
    let mut top = 0;
    for (_, g) in f.glyphs() {
        left = left.min(g.metrics.bounds.x);
        right = right.max(g.metrics.bounds.x + i64::from(g.bitmap.width()));
        bottom = bottom.min(g.metrics.bounds.y);
        top = top.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
    }
    f.metrics.ascent = Some(top);
    f.metrics.descent = Some(-bottom);
    f.metrics.line_height = Some(line);
    f.metrics.pixel_size = u32::try_from(top - bottom).ok();
    f.metrics.bounds = Some(Bounds::new(
        (right - left) as u32,
        (top - bottom) as u32,
        left,
        bottom,
    ));
    f.metrics.space_width = f
        .glyph_for_codepoint(Codepoint::Legacy(32))
        .and_then(|g| g.metrics.advance.map(|v| v.x));
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
            .ok_or_else(|| invalid("workstation pixels overflow"))?,
        l.max_pixels_per_glyph,
        "workstation pixels",
    )?;
    budget.add(raw.len())?;
    Bitmap::<Mono>::from_raw(w, h, raw, bitmap_limits(l))
}
#[cfg(any(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "prebuilt",
    feature = "riscos",
    feature = "xerox"
))]
pub fn output_limit(n: usize, max: usize, l: &ResourceLimits, budget: &mut Budget) -> Result<()> {
    check(
        n,
        max.min(l.max_input_bytes).min(l.max_table_bytes),
        "workstation output bytes",
    )?;
    budget.add(n)
}
#[cfg(any(feature = "catalyst", feature = "chiwriter", feature = "xerox"))]
pub fn project(
    g: Option<&Glyph>,
    bounds: Bounds,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let row = (bounds.width as usize).div_ceil(8);
    let size = row
        .checked_mul(bounds.height as usize)
        .ok_or_else(|| invalid("workstation raster size overflow"))?;
    check(
        (bounds.width as usize)
            .checked_mul(bounds.height as usize)
            .ok_or_else(|| invalid("workstation area overflow"))?,
        l.max_pixels_per_glyph,
        "workstation projected pixels",
    )?;
    budget.add(size)?;
    let mut out = vec![0; size];
    let Some(g) = g else { return Ok(out) };
    let b = mono(g)?;
    if b.width() == 0 || b.height() == 0 {
        return Ok(out);
    }
    let x = g.metrics.bounds.x - bounds.x;
    let y = bounds.y + i64::from(bounds.height) - g.metrics.bounds.y - i64::from(b.height());
    if x < 0
        || y < 0
        || x + i64::from(b.width()) > i64::from(bounds.width)
        || y + i64::from(b.height()) > i64::from(bounds.height)
    {
        return Err(invalid("workstation target rectangle clips glyph"));
    }
    let x = x as usize;
    let y = y as usize;
    for iy in 0..b.height() as usize {
        for (ix, &value) in b.as_bytes()[iy * b.stride()..(iy + 1) * b.stride()]
            .iter()
            .enumerate()
        {
            let at = (iy + y) * row + x / 8 + ix;
            out[at] |= value >> (x % 8);
            if !x.is_multiple_of(8) && x / 8 + ix + 1 < row {
                out[at + 1] |= value << (8 - x % 8);
            }
        }
    }
    Ok(out)
}
#[cfg(any(
    feature = "chiwriter",
    feature = "hppcl",
    feature = "prebuilt",
    feature = "riscos",
    feature = "xerox"
))]
pub fn latin(b: &[u8]) -> String {
    b.iter().map(|&x| char::from(x)).collect()
}
