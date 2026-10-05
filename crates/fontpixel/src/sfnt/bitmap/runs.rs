//! Opt-in shared-metric-run index layout for EBLC/bloc.
//!
//! Consecutive glyph IDs with identical small metrics and nonempty images use
//! index format 2 with bit-aligned image format 5 (one `BigGlyphMetrics` in the
//! index). Every other glyph uses index format 3 with bit-aligned image format 2
//! (per-glyph small metrics, 16-bit offsets), split before 64 KiB. This is a
//! storage layout only: decoded pixels and horizontal metrics are unchanged.
use super::super::{buffer, p16};
use super::{Budget, Kind, Limits, Strike, Tables, bounded, invalid, p32, sample, to32};
use crate::Result;
use crate::model::GlyphBitmap;

/// Encoded tables plus the number of shared-metric (format 2/5) subtables.
/// Shared runs store zero vertical big metrics for horizontal-only glyphs.
pub struct RunTables {
    /// EBDT/EBLC or bdat/bloc pair.
    pub tables: Tables,
    /// Number of index format 2 subtables over all strikes.
    pub shared_runs: usize,
}

struct Item<'a> {
    gid: u16,
    metrics: [u8; 5],
    bytes: usize,
    bitmap: &'a GlyphBitmap,
}

fn pack(out: &mut Vec<u8>, bitmap: &GlyphBitmap, depth: usize, bytes: usize) -> Result<()> {
    let start = out.len();
    out.resize(start + bytes, 0);
    let step = 255 / ((1u16 << depth) - 1) as u8;
    let width = bitmap.width() as usize;
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            let value = sample(bitmap, x, y)?;
            if !value.is_multiple_of(step) {
                return Err(invalid(
                    "bitmap gray value cannot be represented at requested depth",
                ));
            }
            let bit = (y as usize * width + x as usize) * depth;
            out[start + bit / 8] |= (value / step) << (8 - depth - bit % 8);
        }
    }
    Ok(())
}

/// Encode OTB/bdat strikes with the shared-metric-run layout described in the
/// module documentation. Every glyph must carry horizontal-only small metrics
/// (strike flags 1); `minimum_run` is the shortest run stored as format 2/5.
pub fn encode_shared_metric_runs(
    strikes: &[Strike],
    kind: Kind,
    num_glyphs: u16,
    minimum_run: u16,
    limits: &Limits,
) -> Result<RunTables> {
    if kind == Kind::Color {
        return Err(invalid("shared-metric run layout requires EBDT or bdat"));
    }
    if minimum_run == 0 {
        return Err(invalid("shared-metric run length must be positive"));
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
    let mut data = kind.version().to_be_bytes().to_vec();
    let mut location = buffer(header_size)?;
    p32(&mut location, kind.version());
    p32(&mut location, to32(strikes.len())?);
    location.resize(header_size, 0);
    let mut ordered: Vec<_> = strikes.iter().collect();
    if kind == Kind::Apple {
        ordered.sort_by_key(|s| (s.ppem_y, s.ppem_x));
    }
    let mut shared_runs = 0usize;
    for (i, s) in ordered.into_iter().enumerate() {
        if s.ppem_x == 0
            || s.ppem_y == 0
            || s.flags != 1
            || ![1, 2, 4, 8].contains(&s.bit_depth)
            || s.glyphs.is_empty()
        {
            return Err(invalid(
                "shared-metric run layout requires a nonempty horizontal small-metric strike",
            ));
        }
        let depth = s.bit_depth as usize;
        budget.memory(
            s.glyphs
                .len()
                .checked_mul(std::mem::size_of::<Item<'_>>() + std::mem::size_of::<u32>())
                .ok_or_else(|| invalid("bitmap run bookkeeping overflow"))?,
        )?;
        let mut items = Vec::new();
        items
            .try_reserve_exact(s.glyphs.len())
            .map_err(|_| invalid("bitmap run allocation failed"))?;
        for (&gid, g) in &s.glyphs {
            let (Some(h), None) = (g.metrics.horizontal, g.metrics.vertical) else {
                return Err(invalid(
                    "shared-metric run layout requires horizontal-only glyph metrics",
                ));
            };
            if gid >= num_glyphs
                || u32::from(g.metrics.width) != g.bitmap.width()
                || u32::from(g.metrics.height) != g.bitmap.height()
            {
                return Err(invalid("bitmap ID/dimensions disagree with metrics"));
            }
            budget.glyph()?;
            budget.raster(g.bitmap.width(), g.bitmap.height())?;
            items.push(Item {
                gid,
                metrics: [
                    g.metrics.height,
                    g.metrics.width,
                    h.x as u8,
                    h.y as u8,
                    h.advance,
                ],
                bytes: (usize::from(g.metrics.width) * usize::from(g.metrics.height) * depth)
                    .div_ceil(8),
                bitmap: &g.bitmap,
            });
        }
        // Backward pass: the length of the contiguous identical-metric run
        // starting at each item, so selection is linear in the glyph count.
        let mut run = Vec::new();
        run.try_reserve_exact(items.len())
            .map_err(|_| invalid("bitmap run allocation failed"))?;
        run.resize(items.len(), 1u32);
        for j in (0..items.len().saturating_sub(1)).rev() {
            if items[j + 1].gid.checked_sub(1) == Some(items[j].gid)
                && items[j + 1].metrics == items[j].metrics
            {
                run[j] = run[j + 1] + 1;
            }
        }
        let shared = |j: usize| -> usize {
            if items[j].bytes == 0 || run[j] < u32::from(minimum_run) {
                0
            } else {
                run[j] as usize
            }
        };
        let mut subtables: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        let mut first = 0;
        while first < items.len() {
            let count = shared(first);
            let mut sub = Vec::new();
            let end;
            if count > 0 {
                p16(&mut sub, 2);
                p16(&mut sub, 5);
                p32(&mut sub, to32(data.len())?);
                p32(&mut sub, to32(items[first].bytes)?);
                sub.extend_from_slice(&items[first].metrics);
                sub.extend_from_slice(&[0; 3]);
                end = first + count;
                for item in &items[first..end] {
                    limits.sfnt.table(data.len().saturating_add(item.bytes))?;
                    pack(&mut data, item.bitmap, depth, item.bytes)?;
                }
                shared_runs += 1;
            } else {
                let base = data.len();
                p16(&mut sub, 3);
                p16(&mut sub, 2);
                p32(&mut sub, to32(base)?);
                let mut next = first;
                while next < items.len()
                    && shared(next) == 0
                    && (next == first
                        || items[next - 1].gid.checked_add(1) == Some(items[next].gid))
                {
                    let item = &items[next];
                    if data.len() - base + 5 + item.bytes > 65535 {
                        break;
                    }
                    limits.sfnt.table(data.len() + 5 + item.bytes)?;
                    p16(&mut sub, (data.len() - base) as u16);
                    data.extend_from_slice(&item.metrics);
                    pack(&mut data, item.bitmap, depth, item.bytes)?;
                    next += 1;
                }
                if next == first {
                    return Err(invalid("bitmap image exceeds 16-bit index offsets"));
                }
                end = next;
                p16(&mut sub, (data.len() - base) as u16);
                while !sub.len().is_multiple_of(4) {
                    sub.push(0);
                }
            }
            subtables.push((items[first].gid, items[end - 1].gid, sub));
            first = end;
        }
        let start = location.len();
        let array = subtables.len() * 8;
        let index_size = subtables
            .iter()
            .try_fold(array, |n, (_, _, sub)| n.checked_add(sub.len()))
            .ok_or_else(|| invalid("bitmap index size overflow"))?;
        limits.sfnt.table(start.saturating_add(index_size))?;
        let mut offset = array;
        for (lo, hi, sub) in &subtables {
            p16(&mut location, *lo);
            p16(&mut location, *hi);
            p32(&mut location, to32(offset)?);
            offset += sub.len();
        }
        for (_, _, sub) in &subtables {
            location.extend_from_slice(sub);
        }
        let pos = 8 + i * 48;
        location[pos..pos + 4].copy_from_slice(&to32(start)?.to_be_bytes());
        location[pos + 4..pos + 8].copy_from_slice(&to32(index_size)?.to_be_bytes());
        location[pos + 8..pos + 12].copy_from_slice(&to32(subtables.len())?.to_be_bytes());
        location[pos + 16..pos + 28].copy_from_slice(&s.horizontal.0);
        location[pos + 28..pos + 40].copy_from_slice(&s.vertical.0);
        location[pos + 40..pos + 42].copy_from_slice(&items[0].gid.to_be_bytes());
        location[pos + 42..pos + 44].copy_from_slice(&items[items.len() - 1].gid.to_be_bytes());
        location[pos + 44..pos + 48].copy_from_slice(&[s.ppem_x, s.ppem_y, s.bit_depth, s.flags]);
    }
    bounded(
        data.len() + location.len(),
        limits.sfnt.resources.max_total_bytes,
        "bitmap output bytes",
    )?;
    Ok(RunTables {
        tables: Tables { data, location },
        shared_runs,
    })
}
