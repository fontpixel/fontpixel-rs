//! Typed admission helpers for variant planning and export.
//!
//! Every helper admits new owned storage on the caller's single working ledger
//! before allocating it. Charges are released only when the matching storage has
//! actually dropped. Tree storage uses the conservative `Budget::tree_bytes`
//! estimate (one node per entry plus a root); allocator overhead is excluded.
use crate::{
    Error, Font, Glyph, Result,
    model::{GlyphBitmap, GlyphMetrics},
    working::{Budget, Lease, Owned},
};
use std::{
    fmt::{self, Display, Write},
    hash::{Hash, Hasher},
};

/// Resource name reported when the variants ledger is exhausted.
pub(super) const RESOURCE: &str = "variants working bytes";

/// One ledger for a complete planning or export operation.
pub(super) fn budget(maximum: usize) -> Budget {
    Budget::new(maximum, RESOURCE)
}

struct Count(usize);
impl Write for Count {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 = self.0.checked_add(s.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

fn formatting() -> Error {
    Error::InvalidInput("variants: name formatting failed".into())
}

/// Format into a string whose exact capacity is admitted before allocation.
pub(super) fn string(budget: &Budget, value: impl Display) -> Result<Owned<String>> {
    let mut count = Count(0);
    write!(count, "{value}").map_err(|_| formatting())?;
    let mut lease = budget.reserve(count.0)?;
    let mut out = String::new();
    out.try_reserve_exact(count.0).map_err(|_| budget.error())?;
    // An allocator may report more than the requested capacity.
    lease.grow_to(out.capacity())?;
    let capacity = out.capacity();
    write!(out, "{value}").map_err(|_| formatting())?;
    if out.capacity() != capacity {
        return Err(formatting());
    }
    Ok(Owned::from_parts(out, lease))
}

/// Nodes a standard-library tree of `entries` may own. On the exactly audited
/// toolchain (`fontpixel_verified_btree_1_99`, set only by build.rs), every
/// non-root node holds at least five entries, as `model::budget` relies on;
/// other builds keep the one-node-per-entry-plus-root fallback.
fn tree_nodes(entries: usize) -> Option<usize> {
    if cfg!(fontpixel_verified_btree_1_99) {
        1usize.checked_add(entries.saturating_sub(1) / 5)
    } else {
        entries.checked_add(1)
    }
}

/// Conservative tree storage (`Budget::tree_bytes` node size, audited node count).
pub(super) fn tree_bytes<K, V>(budget: &Budget, entries: usize) -> Result<usize> {
    let node = budget.tree_bytes::<K, V>(0)?;
    tree_nodes(entries)
        .and_then(|n| n.checked_mul(node))
        .ok_or_else(|| budget.error())
}

/// Admit a tree that will hold `entries`; `lease` charges only this tree.
pub(super) fn tree<K, V>(lease: &mut Lease, budget: &Budget, entries: usize) -> Result<()> {
    lease.grow_to(tree_bytes::<K, V>(budget, entries)?)
}

/// Admit one more entry in one of several trees sharing `lease`, given the
/// tree's current length.
pub(super) fn grow<K, V>(lease: &mut Lease, budget: &Budget, before: usize) -> Result<()> {
    let after = tree_bytes::<K, V>(budget, before.saturating_add(1))?;
    let current = if before == 0 {
        0
    } else {
        tree_bytes::<K, V>(budget, before)?
    };
    lease.grow_to(
        lease
            .bytes()
            .checked_add(after - current)
            .ok_or_else(|| budget.error())?,
    )
}

/// Admit a whole tree of `entries` before building it.
pub(super) fn reserve_tree<K, V>(budget: &Budget, entries: usize) -> Result<Lease> {
    budget.reserve(tree_bytes::<K, V>(budget, entries)?)
}

/// Bytes a bitmap clone allocates (its rows, cloned with exact length).
pub(super) fn bitmap_bytes(bitmap: &GlyphBitmap) -> usize {
    bitmap.as_bytes().len()
}

/// Clone a bitmap after admitting its exact row storage.
pub(super) fn clone_bitmap(budget: &Budget, bitmap: &GlyphBitmap) -> Result<Owned<GlyphBitmap>> {
    let lease = budget.reserve(bitmap_bytes(bitmap))?;
    Ok(Owned::from_parts(bitmap.clone(), lease))
}

/// A source glyph with this face's metric defaults resolved, borrowing its
/// bitmap. Comparison and signatures never copy rows or source metadata.
pub(super) struct View<'a> {
    pub(super) metrics: GlyphMetrics,
    pub(super) bitmap: &'a GlyphBitmap,
}

impl<'a> View<'a> {
    pub(super) fn new(font: &Font, g: &'a Glyph) -> Self {
        // Defaults belong to each source face, not to the eventual combined font.
        // Resolve them before comparison so inherited and explicit equal metrics
        // deduplicate, while different regional defaults remain distinct.
        let d = &font.metrics.glyph_defaults;
        let m = &g.metrics;
        Self {
            metrics: GlyphMetrics {
                bounds: m.bounds,
                advance: m.advance.or(d.advance),
                scalable_advance: m.scalable_advance.or(d.scalable_advance),
                vertical_advance: m.vertical_advance.or(d.vertical_advance),
                vertical_scalable_advance: m
                    .vertical_scalable_advance
                    .or(d.vertical_scalable_advance),
                vertical_origin: m.vertical_origin.or(d.vertical_origin),
                abc: m.abc.or(d.abc),
                left_bearing: m.left_bearing.or(d.left_bearing),
                right_bearing: m.right_bearing.or(d.right_bearing),
                image_offset: m.image_offset.or(d.image_offset),
            },
            bitmap: &g.bitmap,
        }
    }

    /// Bitmap and every resolved metric are equal.
    pub(super) fn matches(&self, other: &View<'_>) -> bool {
        *self.bitmap == *other.bitmap && self.metrics == other.metrics
    }

    pub(super) fn signature(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(self.bitmap).hash(&mut h);
        self.bitmap.width().hash(&mut h);
        self.bitmap.height().hash(&mut h);
        self.bitmap.as_bytes().hash(&mut h);
        // Hash only common metrics; full equality resolves every collision and
        // also distinguishes all less-common metrics without copying metadata.
        self.metrics.bounds.hash(&mut h);
        self.metrics.advance.hash(&mut h);
        self.metrics.vertical_advance.hash(&mut h);
        h.finish()
    }

    /// Retain a planned glyph: resolved metrics plus an admitted bitmap copy.
    pub(super) fn retain(self, budget: &Budget) -> Result<Owned<Glyph>> {
        let (bitmap, lease) = clone_bitmap(budget, self.bitmap)?.into_parts();
        Ok(Owned::from_parts(
            Glyph {
                metrics: self.metrics,
                bitmap,
                ..Default::default()
            },
            lease,
        ))
    }
}

/// Pair an admitted name with an admitted glyph as one retained record.
pub(super) fn record(name: Owned<String>, glyph: Owned<Glyph>) -> Result<Owned<(String, Glyph)>> {
    let (name, mut lease) = name.into_parts();
    let (glyph, mut glyph_lease) = glyph.into_parts();
    lease.merge(&mut glyph_lease)?;
    Ok(Owned::from_parts((name, glyph), lease))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_admit_their_exact_capacity_before_allocation() {
        let budget = budget(7);
        let name = string(&budget, format_args!("uni{:04X}", 0x4e00)).unwrap();
        assert_eq!(name.as_str(), "uni4E00");
        assert_eq!(budget.remaining(), 0);
        drop(name);
        let short = super::budget(6);
        assert!(matches!(
            string(&short, format_args!("uni{:04X}", 0x4e00)),
            Err(Error::ResourceLimit { .. })
        ));
        assert_eq!(short.remaining(), 6);
    }

    #[test]
    fn tree_charges_follow_the_build_selected_node_bound() {
        let budget = budget(usize::MAX);
        let node = budget.tree_bytes::<u32, u32>(0).unwrap();
        let nodes = |entries: usize| {
            if cfg!(fontpixel_verified_btree_1_99) {
                1 + entries.saturating_sub(1) / 5
            } else {
                entries + 1
            }
        };
        for entries in [0, 1, 5, 6, 11, 1000] {
            assert_eq!(
                tree_bytes::<u32, u32>(&budget, entries).unwrap(),
                nodes(entries) * node
            );
        }
        assert!(tree_bytes::<u32, u32>(&budget, usize::MAX).is_err());
        let mut lease = budget.reserve(0).unwrap();
        tree::<u32, u32>(&mut lease, &budget, 6).unwrap();
        assert_eq!(lease.bytes(), nodes(6) * node);
        // Two trees sharing one lease: each is charged its own bound.
        let mut shared = budget.reserve(0).unwrap();
        for before in 0..7 {
            grow::<u32, u32>(&mut shared, &budget, before).unwrap();
        }
        grow::<u32, u32>(&mut shared, &budget, 0).unwrap();
        assert_eq!(shared.bytes(), (nodes(7) + nodes(1)) * node);
    }
}
