//! Allocation accounting without cloning or serializing a font model.
use super::*;
use std::mem::size_of;

/// Conservative retained model storage and aggregate element counts.
///
/// Bytes include the model header, allocated vector/string capacities (including
/// vacant glyph slots and unused capacity), bitmap storage and both mapping
/// indexes. B-tree nodes use a deliberately generous bound: for each entry plus
/// a possibly retained empty root, 16 key/value/edge slots and 128 header bytes.
/// This exceeds the standard library's current 11-entry, 12-edge node layout.
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
        let nodes = entries.checked_add(1).ok_or_else(|| self.error())?;
        self.add(nodes.checked_mul(node).ok_or_else(|| self.error())?)
    }
    fn font(&mut self, font: &Font) -> Result<()> {
        self.array::<Option<Glyph>>(font.glyphs.capacity())?;
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
        let before = FontSet::from(font.clone())
            .retained_usage(usize::MAX)
            .unwrap();
        let id = font.glyph_id(Codepoint::Unicode(65)).unwrap();
        font.edit_glyph(id, |g| {
            g.codepoints.clear();
            g.unicode_sequences.clear();
            Ok(())
        })
        .unwrap();
        let after = FontSet::from(font).retained_usage(usize::MAX).unwrap();
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
