//! Original bounded helpers for the small device-font codecs.
#[cfg(any(
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "sbf",
    feature = "playdate"
))]
pub use super::legacy_common::int;
#[cfg(any(
    feature = "rockbox",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf"
))]
pub use super::legacy_common::source_budget;
#[cfg(feature = "u8m")]
pub use super::legacy_common::source_budget_for;
pub use super::legacy_common::{
    Budget, audit, bitmap_limits, check, convert_gray, finish, invalid, mono,
};
use crate::bitmap::{Bitmap, Bounds, Mono};
#[cfg(any(
    feature = "rockbox",
    feature = "hmzk",
    feature = "cybiko",
    feature = "sbf",
    feature = "playdate"
))]
use crate::model::Codepoint;
use crate::model::{Font, FontSet, Glyph, Vector};
use crate::{ResourceLimits, Result};
#[cfg(any(
    feature = "rockbox",
    feature = "hmzk",
    feature = "cybiko",
    feature = "sbf",
    feature = "playdate"
))]
use std::collections::BTreeMap;

pub fn slice(b: &[u8], p: usize, n: usize) -> Result<&[u8]> {
    b.get(
        p..p.checked_add(n)
            .ok_or_else(|| invalid("device font offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated device font"))
}
#[cfg(any(
    feature = "rockbox",
    feature = "u8m",
    feature = "hmzk",
    feature = "sbf",
    feature = "playdate"
))]
pub fn r16(b: &[u8], p: usize) -> Result<u16> {
    let v = slice(b, p, 2)?;
    Ok(u16::from_le_bytes([v[0], v[1]]))
}
#[cfg(any(feature = "rockbox", feature = "u8m", feature = "playdate"))]
pub fn r32(b: &[u8], p: usize) -> Result<u32> {
    let v = slice(b, p, 4)?;
    Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}
#[cfg(any(
    feature = "rockbox",
    feature = "u8m",
    feature = "hmzk",
    feature = "sbf",
    feature = "playdate"
))]
pub fn p16(b: &mut [u8], p: usize, v: u16) {
    b[p..p + 2].copy_from_slice(&v.to_le_bytes());
}
#[cfg(any(feature = "rockbox", feature = "u8m", feature = "playdate"))]
pub fn p32(b: &mut [u8], p: usize, v: u32) {
    b[p..p + 4].copy_from_slice(&v.to_le_bytes());
}
#[cfg(any(
    feature = "u8m",
    feature = "cybiko",
    feature = "sbf",
    feature = "playdate"
))]
pub fn byte(n: i64) -> Result<u8> {
    u8::try_from(n).map_err(|_| invalid("device font value exceeds unsigned byte"))
}
pub fn allocation(n: usize, l: &ResourceLimits, budget: &mut Budget) -> Result<()> {
    check(n, l.max_glyphs, "device font glyphs")?;
    budget.items::<Font>(1)?;
    budget.items::<Option<Glyph>>(n)?;
    budget.add(n.saturating_mul(512).saturating_add(2048))
}
#[cfg(any(
    feature = "rockbox",
    feature = "u8m",
    feature = "hmzk",
    feature = "playdate"
))]
pub fn raster(w: u32, h: u32, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<u8>> {
    check(
        (w as usize).saturating_mul(h as usize),
        l.max_pixels_per_glyph,
        "device glyph pixels",
    )?;
    let n = (w as usize)
        .div_ceil(8)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("device raster overflow"))?;
    budget.add(n.saturating_mul(2))?;
    Ok(vec![0; n])
}
pub fn glyph(
    w: u32,
    h: u32,
    x: i64,
    y: i64,
    advance: i64,
    bytes: &[u8],
    l: &ResourceLimits,
) -> Result<Glyph> {
    let mut g = Glyph::new(Bitmap::<Mono>::from_raw(w, h, bytes, bitmap_limits(l))?);
    g.metrics.bounds = Bounds::new(w, h, x, y);
    g.metrics.advance = Some(Vector::new(advance, 0));
    g.metrics.left_bearing = Some(x);
    g.metrics.right_bearing = Some(advance - x - i64::from(w));
    Ok(g)
}
pub fn set(f: Font, id: &str) -> FontSet {
    let mut f = f;
    f.source_format = Some(id.into());
    let mut s = FontSet::from(f);
    s.source_format = Some(id.into());
    s
}
pub fn output(size: usize, cap: usize, l: &ResourceLimits, budget: &mut Budget) -> Result<Vec<u8>> {
    check(
        size,
        cap.min(l.max_input_bytes).min(l.max_table_bytes),
        "device output bytes",
    )?;
    budget.add(size)?;
    Ok(vec![0; size])
}
/// Last mapping wins just as in the model lookup; the readback audit records omissions.
#[cfg(any(
    feature = "rockbox",
    feature = "hmzk",
    feature = "cybiko",
    feature = "sbf",
    feature = "playdate"
))]
pub fn select<'a>(
    f: &'a Font,
    max: u32,
    unicode: bool,
    coerce: bool,
    budget: &mut Budget,
) -> Result<BTreeMap<u32, &'a Glyph>> {
    let mut map = BTreeMap::new();
    for (_, g) in f.glyphs() {
        for cp in &g.codepoints {
            let code = match (*cp, unicode) {
                (Codepoint::Unicode(c), true) | (Codepoint::Legacy(c), false) => Some(c),
                (Codepoint::Unicode(c), false) if coerce => Some(c),
                _ => None,
            };
            if let Some(c) = code.filter(|&c| c <= max) {
                budget.add(96)?;
                map.insert(c, g);
            }
        }
    }
    if map.is_empty() {
        return Err(invalid("no representable device font mappings"));
    }
    Ok(map)
}
/// Place a complete model glyph in a native cell. Geometry is never silently clipped.
#[cfg(any(feature = "rockbox", feature = "hmzk", feature = "playdate"))]
pub fn canvas(
    g: &Glyph,
    w: u32,
    h: u32,
    ascent: i64,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let b = mono(g)?;
    let x = g.metrics.bounds.x;
    let top = ascent
        .checked_sub(g.metrics.bounds.y)
        .and_then(|n| n.checked_sub(i64::from(b.height())))
        .ok_or_else(|| invalid("device cell baseline overflow"))?;
    if b.width() != 0
        && b.height() != 0
        && (x < 0
            || top < 0
            || x + i64::from(b.width()) > i64::from(w)
            || top > i64::from(h) - i64::from(b.height()))
    {
        return Err(invalid(
            "glyph does not fit native device cell without clipping",
        ));
    }
    let mut out = raster(w, h, l, budget)?;
    if b.width() == 0 || b.height() == 0 {
        return Ok(out);
    }
    let stride = (w as usize).div_ceil(8);
    let x = x as usize;
    for y in 0..b.height() as usize {
        for (j, &v) in b.as_bytes()[y * b.stride()..(y + 1) * b.stride()]
            .iter()
            .enumerate()
        {
            let at = (top as usize + y) * stride + x / 8 + j;
            out[at] |= v >> (x % 8);
            if !x.is_multiple_of(8) && x / 8 + j + 1 < stride {
                out[at + 1] |= v << (8 - x % 8);
            }
        }
    }
    Ok(out)
}
macro_rules! codec {
    ($codec:ident, $read:ident, $write:ident) => {
        /// Registry codec with explicit native input and output choices.
        #[derive(Debug, Clone, Default)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
        pub struct $codec {
            /// Native input options.
            pub read_options: $read,
            /// Native output options.
            pub write_options: $write,
        }
        impl super::FormatDetector for $codec {
            fn detect(
                &self,
                c: &mut crate::resources::ReadContext<'_>,
            ) -> crate::Result<super::Confidence> {
                Ok(detect(c.bytes()))
            }
        }
        impl super::FormatReader<crate::FontSet> for $codec {
            fn read(
                &self,
                c: &mut crate::resources::ReadContext<'_>,
                _: &super::ReadOptions,
            ) -> crate::Result<(crate::FontSet, crate::Report)> {
                read(c, &self.read_options)
            }
        }
        impl super::FormatWriter<crate::FontSet> for $codec {
            fn write(
                &self,
                s: &crate::FontSet,
                o: &super::WriteOptions,
            ) -> crate::Result<crate::resources::Written> {
                write(s, o, &self.write_options)
            }
        }
    };
}
pub(crate) use codec;
