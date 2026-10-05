//! Shared horizontal/vertical headers and compact advance/bearing arrays.
use super::working::{Budget, Owned, Vector};
use super::{Limits, buffer, invalid, p16, p32, pi16, to16, u16_at, u32_at};
use crate::Result;
use read_fonts::{
    FontData, FontRead,
    tables::{hhea::Hhea, vhea::Vhea},
};

/// Metric direction; fields are left/right for horizontal and top/bottom for vertical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// `hhea`/`hmtx`.
    Horizontal,
    /// `vhea`/`vmtx` (version 1.1 by default).
    Vertical,
}
/// A long metric, in font design units, in glyph-ID order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LongMetric {
    /// Advance width or height.
    pub advance: u16,
    /// Left or top side bearing.
    pub bearing: i16,
}
/// Axis-independent metric header. Caret vector is (rise, run), as on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Version: 0x10000 for horizontal, 0x10000 or 0x11000 for vertical.
    pub version: u32,
    /// Ascender, in font units.
    pub ascender: i16,
    /// Descender, generally negative.
    pub descender: i16,
    /// Line gap.
    pub line_gap: i16,
    /// Largest advance, including empty glyphs.
    pub advance_max: u16,
    /// Minimum left/top bearing over glyphs with contours.
    pub min_side_bearing: i16,
    /// Minimum right/bottom bearing over glyphs with contours.
    pub min_other_side_bearing: i16,
    /// Largest bearing + ink extent over glyphs with contours.
    pub max_extent: i16,
    /// Caret slope rise.
    pub caret_slope_rise: i16,
    /// Caret slope run.
    pub caret_slope_run: i16,
    /// Caret offset.
    pub caret_offset: i16,
    /// Number of full advance/bearing pairs, recomputed by `encode`.
    pub num_long_metrics: u16,
}
impl Header {
    /// Zero metrics and a conventional upright caret, with explicit direction.
    pub fn new(axis: Axis) -> Self {
        Self {
            version: if axis == Axis::Horizontal {
                0x10000
            } else {
                0x11000
            },
            ascender: 0,
            descender: 0,
            line_gap: 0,
            advance_max: 0,
            min_side_bearing: 0,
            min_other_side_bearing: 0,
            max_extent: 0,
            caret_slope_rise: i16::from(axis == Axis::Horizontal),
            caret_slope_run: i16::from(axis == Axis::Vertical),
            caret_offset: 0,
            num_long_metrics: 0,
        }
    }
    /// Recompute max advance and ink extrema from matching glyph-order arrays.
    /// `None` means no contours and is excluded from bearing/extent extrema.
    pub fn recompute(&mut self, metrics: &[LongMetric], ink_extents: &[Option<u16>]) -> Result<()> {
        if metrics.len() != ink_extents.len() {
            return Err(invalid("metric/ink counts differ"));
        }
        let mut min = None::<i32>;
        let mut other = None::<i32>;
        let mut max = None::<i32>;
        self.advance_max = metrics.iter().map(|m| m.advance).max().unwrap_or(0);
        for (m, size) in metrics.iter().zip(ink_extents) {
            if let Some(size) = size {
                let a = i32::from(m.bearing);
                let e = a + i32::from(*size);
                let b = i32::from(m.advance) - e;
                min = Some(min.map_or(a, |v| v.min(a)));
                other = Some(other.map_or(b, |v| v.min(b)));
                max = Some(max.map_or(e, |v| v.max(e)));
            }
        }
        let narrow = |n: Option<i32>| {
            i16::try_from(n.unwrap_or(0)).map_err(|_| invalid("metric extent exceeds int16"))
        };
        self.min_side_bearing = narrow(min)?;
        self.min_other_side_bearing = narrow(other)?;
        self.max_extent = narrow(max)?;
        Ok(())
    }
}
fn validate_version(axis: Axis, version: u32) -> Result<()> {
    if version != 0x10000 && !(axis == Axis::Vertical && version == 0x11000) {
        Err(invalid("unsupported metric-header version"))
    } else {
        Ok(())
    }
}
/// Header + metric array bytes. The header's count matches compact storage.
#[derive(Clone, Debug)]
pub struct EncodedMetrics {
    /// hhea or vhea bytes.
    pub header: Vec<u8>,
    /// hmtx or vmtx bytes.
    pub metrics: Vec<u8>,
    /// Count of full pairs before trailing bearing-only entries.
    pub num_long_metrics: u16,
}
/// Encode one direction; optional compaction shares only identical trailing advances.
pub fn encode(
    axis: Axis,
    header: &Header,
    metrics: &[LongMetric],
    compact: bool,
    limits: &Limits,
) -> Result<EncodedMetrics> {
    encode_into(axis, header, metrics, compact, limits, &mut buffer)
}
/// Encode into exact-capacity buffers charged to the caller's shared ledger.
pub(crate) fn encode_with_budget(
    axis: Axis,
    header: &Header,
    metrics: &[LongMetric],
    compact: bool,
    limits: &Limits,
    budget: &Budget,
) -> Result<Owned<EncodedMetrics>> {
    let mut lease = budget.reserve(0)?;
    let encoded = encode_into(axis, header, metrics, compact, limits, &mut |n| {
        budget.vec_into(n, &mut lease)
    })?;
    Ok(Owned::from_parts(encoded, lease))
}
fn encode_into(
    axis: Axis,
    header: &Header,
    metrics: &[LongMetric],
    compact: bool,
    limits: &Limits,
    buffer: &mut dyn FnMut(usize) -> Result<Vec<u8>>,
) -> Result<EncodedMetrics> {
    validate_version(axis, header.version)?;
    limits.glyphs(metrics.len())?;
    let mut count = metrics.len();
    if compact {
        while count > 1 && metrics[count - 1].advance == metrics[count - 2].advance {
            count -= 1;
        }
    }
    let size = count * 4 + (metrics.len() - count) * 2;
    limits.table(size)?;
    limits.table(36)?;
    let mut h = buffer(36)?;
    p32(&mut h, header.version);
    for n in [header.ascender, header.descender, header.line_gap] {
        pi16(&mut h, n);
    }
    p16(&mut h, header.advance_max);
    for n in [
        header.min_side_bearing,
        header.min_other_side_bearing,
        header.max_extent,
        header.caret_slope_rise,
        header.caret_slope_run,
        header.caret_offset,
        0,
        0,
        0,
        0,
        0,
    ] {
        pi16(&mut h, n);
    }
    p16(&mut h, to16(count)?);
    let mut out = buffer(size)?;
    for (i, m) in metrics.iter().enumerate() {
        if i < count {
            p16(&mut out, m.advance);
        }
        pi16(&mut out, m.bearing);
    }
    Ok(EncodedMetrics {
        header: h,
        metrics: out,
        num_long_metrics: to16(count)?,
    })
}
/// Read hhea/hmtx or vhea/vmtx, expanding shared advances under a glyph budget.
/// Count comes from maxp, never inferred from trailing input bytes.
pub fn decode(
    axis: Axis,
    header: &[u8],
    data: &[u8],
    num_glyphs: u16,
    limits: &Limits,
) -> Result<(Header, Vec<LongMetric>)> {
    let h = decode_header(axis, header, data, num_glyphs, limits)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(num_glyphs as usize)
        .map_err(|_| invalid("metric allocation failed"))?;
    for gid in 0..num_glyphs {
        result.push(metric(data, h.num_long_metrics, gid)?);
    }
    Ok((h, result))
}
/// Validate exactly what `decode` validates, including the complete array
/// length for all `num_glyphs`, but allocate only the selected glyphs' metrics
/// (in ascending glyph order) under the caller's shared ledger.
pub(crate) fn decode_selected_with_budget(
    axis: Axis,
    header: &[u8],
    data: &[u8],
    num_glyphs: u16,
    selected: &std::collections::BTreeSet<u16>,
    limits: &Limits,
    budget: &Budget,
) -> Result<(Header, Owned<Vec<LongMetric>>)> {
    let h = decode_header(axis, header, data, num_glyphs, limits)?;
    let mut result = Vector::with_capacity(budget, selected.len())?;
    for &gid in selected {
        if gid >= num_glyphs {
            return Err(invalid("selected metric glyph out of range"));
        }
        result.push(metric(data, h.num_long_metrics, gid)?)?;
    }
    Ok((h, result.into_owned()))
}
/// One expanded metric; trailing entries share the last long advance.
fn metric(data: &[u8], count: u16, gid: u16) -> Result<LongMetric> {
    Ok(if gid < count {
        let off = usize::from(gid) * 4;
        LongMetric {
            advance: u16_at(data, off)?,
            bearing: u16_at(data, off + 2)? as i16,
        }
    } else {
        LongMetric {
            advance: u16_at(data, usize::from(count - 1) * 4)?,
            bearing: u16_at(data, usize::from(count) * 4 + usize::from(gid - count) * 2)? as i16,
        }
    })
}
fn decode_header(
    axis: Axis,
    header: &[u8],
    data: &[u8],
    num_glyphs: u16,
    limits: &Limits,
) -> Result<Header> {
    limits.table(header.len())?;
    limits.table(data.len())?;
    limits.glyphs(num_glyphs as usize)?;
    match axis {
        Axis::Horizontal => {
            Hhea::read(FontData::new(header)).map_err(invalid)?;
        }
        Axis::Vertical => {
            Vhea::read(FontData::new(header)).map_err(invalid)?;
        }
    }
    let version = u32_at(header, 0)?;
    validate_version(axis, version)?;
    if u16_at(header, 32)? != 0 {
        return Err(invalid("unsupported metric data format"));
    }
    let count = u16_at(header, 34)?;
    if count == 0 || count > num_glyphs {
        return Err(invalid("invalid number of long metrics"));
    }
    let required = usize::from(count) * 4 + usize::from(num_glyphs - count) * 2;
    super::bytes(data, 0, required)?;
    let i = |o| u16_at(header, o).map(|v| v as i16);
    let h = Header {
        version,
        ascender: i(4)?,
        descender: i(6)?,
        line_gap: i(8)?,
        advance_max: u16_at(header, 10)?,
        min_side_bearing: i(12)?,
        min_other_side_bearing: i(14)?,
        max_extent: i(16)?,
        caret_slope_rise: i(18)?,
        caret_slope_run: i(20)?,
        caret_offset: i(22)?,
        num_long_metrics: count,
    };
    Ok(h)
}
