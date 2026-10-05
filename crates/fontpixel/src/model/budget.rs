//! Allocation accounting without cloning or serializing a font model.
use super::*;
use std::mem::size_of;

/// Conservative retained model storage and aggregate element counts.
///
/// Bytes include the model header, allocated vector/string capacities (including
/// vacant glyph slots and unused capacity), bitmap storage and both mapping
/// indexes. Each B-tree node reserves 16 key/value/edge slots and 128 header
/// bytes, exceeding the audited 11-key/12-edge layout. On the exactly verified
/// Rust 1.99.0 x86-64 Linux toolchain, minimum occupancy bounds the node count;
/// other builds retain the prior conservative per-entry-plus-root estimate.
/// These estimates assume the documented supported standard-library layout.
/// Allocator bookkeeping, stack usage and process-wide library state are not
/// included; this is an owned-storage budget, not an estimate of process RSS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelUsage {
    /// Retained storage in bytes, conservatively accounting for indexes.
    pub bytes: usize,
    /// Number of strikes, including empty strikes.
    pub fonts: usize,
    /// Number of live glyphs, including unencoded glyphs.
    pub glyphs: usize,
    /// Number of assigned glyph IDs, including deleted slots.
    pub glyph_slots: usize,
}

// Rust 1.99.0 alloc/collections/btree/map.rs:27–36 and node.rs:41–62:
// B=6 and MIN_LEN=5; every completed non-root node has at least five keys.
// A nonempty root has >=1 key, hence nodes <= 1+floor((entries-1)/5).
// An empty map may retain one empty leaf. Accounting borrows completed private
// Font indexes; no intermediate insertion/removal state is observable here.
// https://raw.githubusercontent.com/rust-lang/rust/1.99.0/library/alloc/src/collections/btree/map.rs
// https://raw.githubusercontent.com/rust-lang/rust/1.99.0/library/alloc/src/collections/btree/node.rs
// build.rs enables the tight bound only for the audited compiler/target identity.
fn tree_node_bound(entries: usize, verified: bool) -> Option<usize> {
    if verified {
        1usize.checked_add(entries.saturating_sub(1) / 5)
    } else {
        entries.checked_add(1)
    }
}

struct Counter {
    used: usize,
    maximum: usize,
}
impl Counter {
    fn error(&self) -> Error {
        Error::ResourceLimit {
            resource: "retained model bytes",
            limit: self.maximum,
        }
    }
    fn add(&mut self, bytes: usize) -> Result<()> {
        self.used = self.used.checked_add(bytes).ok_or_else(|| self.error())?;
        if self.used > self.maximum {
            return Err(self.error());
        }
        Ok(())
    }
    fn array<T>(&mut self, capacity: usize) -> Result<()> {
        self.add(
            capacity
                .checked_mul(size_of::<T>())
                .ok_or_else(|| self.error())?,
        )
    }
    fn string(&mut self, value: &Option<String>) -> Result<()> {
        self.add(value.as_ref().map_or(0, String::capacity))
    }
    fn value(&mut self, value: &PropertyValue, depth: usize) -> Result<()> {
        match value {
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => self.add(s.capacity()),
            PropertyValue::Bytes(bytes) => self.add(bytes.capacity()),
            PropertyValue::List(values) => {
                // A fixed traversal-depth bound avoids a hostile in-memory
                // property tree overflowing the thread stack during accounting.
                if depth >= 128 {
                    return Err(Error::ResourceLimit {
                        resource: "retained property nesting",
                        limit: 128,
                    });
                }
                self.array::<PropertyValue>(values.capacity())?;
                for value in values {
                    self.value(value, depth + 1)?;
                }
                Ok(())
            }
            PropertyValue::Integer(_) | PropertyValue::Unsigned(_) | PropertyValue::Flag => Ok(()),
        }
    }
    fn properties(&mut self, properties: &Properties) -> Result<()> {
        self.array::<Property>(properties.allocation_capacity())?;
        for property in properties.iter() {
            self.add(property.name.capacity())?;
            self.add(property.raw.as_ref().map_or(0, Vec::capacity))?;
            self.value(&property.value, 0)?;
        }
        Ok(())
    }
    fn tree<K, V>(&mut self, entries: usize) -> Result<()> {
        let slot = size_of::<K>() + size_of::<V>() + size_of::<usize>();
        let node = slot * 16 + 128;
        let nodes = tree_node_bound(entries, cfg!(fontpixel_verified_btree_1_99))
            .ok_or_else(|| self.error())?;
        self.add(nodes.checked_mul(node).ok_or_else(|| self.error())?)
    }
    fn font(&mut self, font: &Font) -> Result<()> {
        self.font_with_slots(font, font.glyphs.capacity())
    }
    fn font_with_slots(&mut self, font: &Font, slots: usize) -> Result<()> {
        self.array::<Option<Glyph>>(slots)?;
        self.tree::<Codepoint, Vec<GlyphId>>(font.cmap.len())?;
        for ids in font.cmap.values() {
            self.array::<GlyphId>(ids.capacity())?;
        }
        self.tree::<Vec<u32>, Vec<GlyphId>>(font.sequence_map.len())?;
        for (sequence, ids) in &font.sequence_map {
            self.array::<u32>(sequence.capacity())?;
            self.array::<GlyphId>(ids.capacity())?;
        }
        for (_, glyph) in font.glyphs() {
            self.array::<Codepoint>(glyph.codepoints.capacity())?;
            self.array::<Vec<u32>>(glyph.unicode_sequences.capacity())?;
            for sequence in &glyph.unicode_sequences {
                self.array::<u32>(sequence.capacity())?;
            }
            self.string(&glyph.name)?;
            let capacity = match &glyph.bitmap {
                GlyphBitmap::Mono(bitmap) => bitmap.allocation_capacity(),
                GlyphBitmap::Gray8(bitmap) => bitmap.allocation_capacity(),
            };
            self.add(capacity)?;
            self.properties(&glyph.properties)?;
        }
        let names = &font.names;
        for name in [
            &names.family,
            &names.style,
            &names.full,
            &names.postscript,
            &names.version,
            &names.copyright,
            &names.license,
            &names.license_url,
            &names.foundry,
            &names.xlfd,
            &font.charset.registry,
            &font.charset.encoding,
            &font.charset.name,
            &font.source_format,
        ] {
            self.string(name)?;
        }
        self.array::<String>(names.xlfd_fields.capacity())?;
        for field in &names.xlfd_fields {
            self.add(field.capacity())?;
        }
        self.properties(&names.localized)?;
        self.array::<(u32, u32)>(font.charset.code_ranges.capacity())?;
        self.properties(&font.headers)?;
        self.properties(&font.properties)
    }
}

impl Font {
    /// Count this strike's retained storage without cloning its bitmap or metadata.
    ///
    /// Uses the same conservative allocation accounting and nesting limit as
    /// [`FontSet::retained_usage`], including vacant glyph slots and map indexes.
    pub fn retained_usage(&self, max_bytes: usize) -> Result<ModelUsage> {
        let mut counter = Counter {
            used: 0,
            maximum: max_bytes,
        };
        counter.add(size_of::<Self>())?;
        counter.font(self)?;
        Ok(ModelUsage {
            bytes: counter.used,
            fonts: 1,
            glyphs: self.len(),
            glyph_slots: self.slot_count(),
        })
    }
    /// Conservatively preflight storage for a new owned clone without allocating it.
    ///
    /// On the exactly audited toolchain, Vec cloning allocates only the initialized
    /// outer glyph-slot length, including deleted slots. The source's spare slots
    /// remain charged by `retained_usage`. All other nested capacities keep their
    /// conservative source charges. Unverified builds retain the old full-capacity
    /// estimate. Allocator overhead is excluded, as for `retained_usage`.
    pub fn cloned_usage(&self, max_bytes: usize) -> Result<ModelUsage> {
        let mut counter = Counter {
            used: 0,
            maximum: max_bytes,
        };
        counter.add(size_of::<Self>())?;
        // Audited Rust 1.99 alloc/src/vec/mod.rs Clone -> slice::to_vec_in,
        // slice.rs -> Vec::with_capacity_in(len), raw_vec/mod.rs stores the
        // requested capacity, and Global reports exactly the requested layout.
        let slots = if cfg!(fontpixel_verified_btree_1_99) {
            self.glyphs.len()
        } else {
            self.glyphs.capacity()
        };
        counter.font_with_slots(self, slots)?;
        Ok(ModelUsage {
            bytes: counter.used,
            fonts: 1,
            glyphs: self.len(),
            glyph_slots: self.slot_count(),
        })
    }
}

impl FontSet {
    /// Count retained allocations in one linear traversal without copying the
    /// model. Fail immediately when `max_bytes` is exceeded or arithmetic would
    /// overflow. Property-list nesting is limited to 128 levels during traversal.
    pub fn retained_usage(&self, max_bytes: usize) -> Result<ModelUsage> {
        let mut counter = Counter {
            used: 0,
            maximum: max_bytes,
        };
        counter.add(size_of::<Self>())?;
        counter.array::<Font>(self.fonts.capacity())?;
        counter.properties(&self.properties)?;
        counter.string(&self.source_format)?;
        let mut usage = ModelUsage {
            fonts: self.fonts.len(),
            ..ModelUsage::default()
        };
        for font in &self.fonts {
            usage.glyphs = usage
                .glyphs
                .checked_add(font.len())
                .ok_or_else(|| counter.error())?;
            usage.glyph_slots = usage
                .glyph_slots
                .checked_add(font.slot_count())
                .ok_or_else(|| counter.error())?;
            counter.font(font)?;
        }
        usage.bytes = counter.used;
        Ok(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_preflight_counts_initialized_slots_and_preserves_source_capacity() {
        let mut font = Font::from_glyphs([Glyph::default(), Glyph::default()]).unwrap();
        let id = font.glyphs().next().unwrap().0;
        font.remove_glyph(id).unwrap();
        font.glyphs.reserve(1000);
        let source = font.retained_usage(usize::MAX).unwrap();
        let bound = font.cloned_usage(usize::MAX).unwrap();
        let clone = font.clone();
        let actual = clone.retained_usage(usize::MAX).unwrap();
        assert_eq!(font, clone);
        assert_eq!(bound.glyph_slots, 2);
        assert!(actual.bytes <= bound.bytes);
        assert!(bound.bytes <= source.bytes);
        assert_eq!(font.cloned_usage(bound.bytes).unwrap(), bound);
        assert!(font.cloned_usage(bound.bytes - 1).is_err());
        assert_eq!(font.retained_usage(usize::MAX).unwrap(), source);
        if cfg!(fontpixel_verified_btree_1_99) {
            assert_eq!(clone.glyphs.capacity(), font.glyphs.len());
            assert_eq!(
                source.bytes - bound.bytes,
                (font.glyphs.capacity() - font.glyphs.len()) * size_of::<Option<Glyph>>()
            );
            assert!(font.retained_usage(bound.bytes).is_err());
        } else {
            assert_eq!(source.bytes, bound.bytes);
        }
    }
    #[test]
    fn clone_preflight_covers_empty_deleted_and_nested_spare_allocations() {
        for count in [0, 1, 3, 4, 5, 33, 1000] {
            for deleted in [false, true] {
                let mut font = Font::default();
                let mut ids = Vec::new();
                for _ in 0..count {
                    let mut glyph = Glyph::default();
                    glyph.codepoints.reserve(20);
                    ids.push(font.add_glyph(glyph).unwrap());
                }
                if deleted {
                    for id in ids {
                        font.remove_glyph(id).unwrap();
                    }
                }
                font.glyphs.reserve(1001);
                let mut text = String::with_capacity(4096);
                text.push('x');
                font.names.family = Some(text);
                let before = font.retained_usage(usize::MAX).unwrap();
                let bound = font.cloned_usage(usize::MAX).unwrap();
                let copy = font.clone();
                assert_eq!(copy, font);
                assert_eq!(bound.glyph_slots, count);
                assert!(copy.retained_usage(usize::MAX).unwrap().bytes <= bound.bytes);
                assert_eq!(font.retained_usage(usize::MAX).unwrap(), before);
                assert_eq!(font.cloned_usage(bound.bytes).unwrap(), bound);
                assert!(font.cloned_usage(bound.bytes - 1).is_err());
                if cfg!(fontpixel_verified_btree_1_99) {
                    assert_eq!(copy.glyphs.capacity(), count);
                }
            }
        }
    }
    #[test]
    fn tree_node_bounds_cover_empty_roots_and_overflow() {
        for (entries, nodes) in [
            (0, 1),
            (1, 1),
            (5, 1),
            (6, 2),
            (10, 2),
            (11, 3),
            (12, 3),
            (71, 15),
            (72, 15),
        ] {
            assert_eq!(tree_node_bound(entries, true), Some(nodes));
            assert_eq!(tree_node_bound(entries, false), Some(entries + 1));
        }
        assert_eq!(
            tree_node_bound(usize::MAX, true),
            Some(1 + (usize::MAX - 1) / 5)
        );
        assert_eq!(tree_node_bound(usize::MAX, false), None);
        let mut c = Counter {
            used: 0,
            maximum: usize::MAX,
        };
        assert!(c.tree::<Codepoint, Vec<GlyphId>>(usize::MAX).is_err());
    }
    #[test]
    fn tree_byte_limits_are_exact_for_both_index_shapes() {
        fn check<K, V>() {
            for entries in [0, 1, 5, 6, 11, 12, 71, 72, 1000] {
                let node = (size_of::<K>() + size_of::<V>() + size_of::<usize>()) * 16 + 128;
                let expected =
                    tree_node_bound(entries, cfg!(fontpixel_verified_btree_1_99)).unwrap() * node;
                let mut exact = Counter {
                    used: 0,
                    maximum: expected,
                };
                exact.tree::<K, V>(entries).unwrap();
                assert_eq!(exact.used, expected);
                let mut short = Counter {
                    used: 0,
                    maximum: expected - 1,
                };
                assert!(short.tree::<K, V>(entries).is_err());
            }
        }
        check::<Codepoint, Vec<GlyphId>>();
        check::<Vec<u32>, Vec<GlyphId>>();
    }
    #[test]
    fn index_accounting_handles_order_duplicates_and_root_collapse() {
        for order in 0..3 {
            let mut font = Font::default();
            let mut ids = Vec::new();
            for i in 0..120u32 {
                let cp = 1000
                    + match order {
                        0 => i,
                        1 => 119 - i,
                        _ => (i * 37) % 120,
                    };
                let g = Glyph {
                    codepoints: vec![Codepoint::Unicode(cp), Codepoint::Unicode(cp)],
                    unicode_sequences: vec![vec![cp, 0xfe0f], vec![cp, 0xfe0f]],
                    ..Glyph::default()
                };
                ids.push(font.add_glyph(g).unwrap());
                let usage = font.retained_usage(usize::MAX).unwrap();
                assert_eq!(font.retained_usage(usage.bytes).unwrap(), usage);
                assert!(font.retained_usage(usage.bytes - 1).is_err());
            }
            let slots = font.slot_count();
            for id in ids {
                let same = font.glyph(id).unwrap().clone();
                font.replace_glyph(id, same).unwrap();
                font.remove_glyph(id).unwrap();
                let usage = font.retained_usage(usize::MAX).unwrap();
                assert_eq!(usage.glyph_slots, slots);
                assert!(usage.bytes >= font.glyphs.capacity() * size_of::<Option<Glyph>>());
                assert_eq!(font.retained_usage(usage.bytes).unwrap(), usage);
            }
            assert!(font.cmap.is_empty());
            assert!(font.sequence_map.is_empty());
        }
    }
    #[test]
    fn borrowed_font_accounting_is_bounded_and_matches_one_strike() {
        let mut font = Font::default();
        font.names.family = Some("Borrowed model".into());
        let usage = font.retained_usage(usize::MAX).unwrap();
        assert_eq!(usage.fonts, 1);
        assert_eq!(font.retained_usage(usage.bytes).unwrap(), usage);
        assert!(font.retained_usage(usage.bytes - 1).is_err());
        let set_usage = FontSet::from(font).retained_usage(usize::MAX).unwrap();
        assert!(set_usage.bytes >= usage.bytes);
    }
    #[test]
    fn removed_glyphs_and_empty_property_capacity_remain_charged() {
        let mut font = Font::default();
        let removed = font.add_glyph(Glyph::default()).unwrap();
        font.remove_glyph(removed).unwrap();
        let mut properties = Vec::with_capacity(40);
        properties.push(Property::new("gone", "value"));
        font.properties = properties.into();
        font.properties.remove("gone");
        let set = FontSet::from(font);
        let usage = set.retained_usage(usize::MAX).unwrap();
        assert_eq!((usage.fonts, usage.glyphs, usage.glyph_slots), (1, 0, 1));
        assert!(usage.bytes >= 40 * size_of::<Property>() + size_of::<Option<Glyph>>());
        assert!(set.retained_usage(usage.bytes - 1).is_err());
        assert_eq!(set.retained_usage(usage.bytes).unwrap(), usage);
    }

    #[test]
    fn spare_capacity_raw_properties_and_nested_values_are_counted() {
        let mut set = FontSet::default();
        let initial = set.retained_usage(usize::MAX).unwrap().bytes;
        let mut text = String::with_capacity(4096);
        text.push('x');
        let mut raw = Vec::with_capacity(8192);
        raw.push(1);
        let mut payload = Vec::with_capacity(16384);
        payload.push(2);
        set.properties.push(Property {
            name: text,
            raw: Some(raw),
            value: PropertyValue::List(vec![PropertyValue::Bytes(payload)]),
        });
        let usage = set.retained_usage(usize::MAX).unwrap();
        assert!(usage.bytes - initial >= 4096 + 8192 + 16384);
        assert!(set.retained_usage(initial + 4096 + 8192).is_err());
    }

    #[test]
    fn aliases_and_sequences_include_their_index_copies() {
        let mut glyph = Glyph::new(Bitmap::<Gray8>::new(9, 7).unwrap());
        glyph.codepoints = vec![Codepoint::Legacy(65), Codepoint::Unicode(65)];
        glyph.unicode_sequences = vec![vec![65, 0x301], vec![65, 0x302]];
        let mut font = Font::from_glyphs([glyph]).unwrap();
        // Compare the same owned allocation before/after the edit. Cloning only
        // the before side would shrink its outer Vec and confound index charges.
        let before = font.retained_usage(usize::MAX).unwrap();
        let id = font.glyph_id(Codepoint::Unicode(65)).unwrap();
        font.edit_glyph(id, |g| {
            g.codepoints.clear();
            g.unicode_sequences.clear();
            Ok(())
        })
        .unwrap();
        let after = font.retained_usage(usize::MAX).unwrap();
        assert_eq!((before.glyphs, before.glyph_slots), (1, 1));
        assert!(before.bytes > after.bytes);
        assert!(after.bytes >= 9 * 7);
    }

    #[test]
    fn traversal_rejects_excessive_property_nesting_without_deep_recursion() {
        let mut value = PropertyValue::Flag;
        for _ in 0..130 {
            value = PropertyValue::List(vec![value]);
        }
        let mut set = FontSet::default();
        set.properties.push(Property::new("nested", value));
        assert!(matches!(
            set.retained_usage(usize::MAX),
            Err(Error::ResourceLimit {
                resource: "retained property nesting",
                limit: 128
            })
        ));
    }
}
