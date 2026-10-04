use super::*;
use crate::model::PropertyValue;
use std::mem::size_of;

pub(super) struct Budget<'a> {
    pub used: usize,
    l: &'a ResourceLimits,
}
impl<'a> Budget<'a> {
    pub fn new(l: &'a ResourceLimits, used: usize) -> Result<Self> {
        check(used, l.max_total_bytes, "image aggregate bytes")?;
        Ok(Self { used, l })
    }
    pub fn bytes(&mut self, n: usize) -> Result<()> {
        add(&mut self.used, n, self.l.max_total_bytes)
    }
    pub fn items<T>(&mut self, n: usize) -> Result<()> {
        // Vec growth and temporary model/index storage are charged before use.
        self.bytes(n.saturating_mul(size_of::<T>()).saturating_mul(2))
    }
    fn text(&mut self, s: &str) -> Result<()> {
        check(s.len(), self.l.max_table_bytes, "image metadata string")?;
        self.bytes(s.len())
    }
    fn texts<'s>(&mut self, ss: impl IntoIterator<Item = &'s Option<String>>) -> Result<()> {
        for s in ss.into_iter().flatten() {
            self.text(s)?;
        }
        Ok(())
    }
    fn value(&mut self, v: &PropertyValue, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(invalid("image property nesting exceeds 32"));
        }
        match v {
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => self.text(s)?,
            PropertyValue::Bytes(b) => {
                check(b.len(), self.l.max_table_bytes, "image property bytes")?;
                self.bytes(b.len())?;
            }
            PropertyValue::List(vs) => {
                self.items::<PropertyValue>(vs.len())?;
                for v in vs {
                    self.value(v, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn properties(&mut self, ps: &Properties) -> Result<()> {
        self.items::<crate::model::Property>(ps.len())?;
        for p in ps.iter() {
            self.text(&p.name)?;
            self.value(&p.value, 0)?;
            if let Some(b) = &p.raw {
                check(b.len(), self.l.max_table_bytes, "image raw property bytes")?;
                self.bytes(b.len())?;
            }
        }
        Ok(())
    }
    fn names(&mut self, n: &FontNames, c: &FontCharset) -> Result<()> {
        self.texts([
            &n.family,
            &n.style,
            &n.full,
            &n.postscript,
            &n.version,
            &n.copyright,
            &n.license,
            &n.license_url,
            &n.foundry,
            &n.xlfd,
            &c.name,
            &c.registry,
            &c.encoding,
        ])?;
        self.items::<String>(n.xlfd_fields.len())?;
        for s in &n.xlfd_fields {
            self.text(s)?;
        }
        self.properties(&n.localized)?;
        self.items::<(u32, u32)>(c.code_ranges.len())
    }
    fn glyph(
        &mut self,
        cp: &[Codepoint],
        seq: &[Vec<u32>],
        name: &Option<String>,
        p: &Properties,
    ) -> Result<()> {
        // Codepoint and sequence maps allocate ordered index nodes as glyphs enter Font.
        self.bytes(cp.len().saturating_mul(96))?;
        self.items::<Vec<u32>>(seq.len())?;
        for s in seq {
            self.bytes(96)?;
            self.items::<u32>(s.len().saturating_mul(2))?;
        }
        self.texts([name])?;
        self.properties(p)
    }
    pub fn mapping(&mut self, m: &Mapping) -> Result<()> {
        self.bytes(size_of::<Mapping>())?;
        self.texts([&m.image, &m.characters, &m.name])?;
        self.items::<[u8; 3]>(m.foreground.len())?;
        if let Some(cs) = &m.cells {
            check(cs.len(), self.l.max_glyphs, "image glyph count")?;
            self.items::<Cell>(cs.len())?;
            for c in cs {
                self.texts([&c.character, &c.name])?;
                if let Some(g) = &c.native {
                    self.glyph(&g.codepoints, &g.unicode_sequences, &g.name, &g.properties)?;
                }
            }
        }
        if let Some(n) = &m.native {
            check(n.slots, self.l.max_glyphs, "image native slots")?;
            self.names(&n.names, &n.charset)?;
            self.texts([&n.source_format, &n.container_source])?;
            self.properties(&n.headers)?;
            self.properties(&n.properties)?;
            self.properties(&n.container_properties)?;
            if n.metrics.point_size.is_some_and(|v| v.denominator() == 0) {
                return Err(invalid("zero point-size denominator"));
            }
        }
        Ok(())
    }
    pub fn source(&mut self, s: &FontSet, f: &Font) -> Result<()> {
        self.names(&f.names, &f.charset)?;
        self.texts([&f.source_format, &s.source_format])?;
        for p in [&f.headers, &f.properties, &s.properties] {
            self.properties(p)?;
        }
        self.items::<Cell>(f.len())?;
        check(f.slot_count(), self.l.max_glyphs, "image native slots")?;
        if f.metrics.point_size.is_some_and(|v| v.denominator() == 0) {
            return Err(invalid("zero point-size denominator"));
        }
        for (_, g) in f.glyphs() {
            self.glyph(&g.codepoints, &g.unicode_sequences, &g.name, &g.properties)?;
            // Worst case one threshold-loss diagnostic per glyph.
            self.bytes(256)?;
        }
        Ok(())
    }
}

// Allocation-free JSON traversal before typed deserialization. Charging objects
// and sequence entries avoids rejecting large native maps solely because JSON
// contains whitespace or long source strings. Escaped-string parser scratch is
// reserved up front; object storage and vector growth are charged incrementally.
pub(super) fn json(bytes: &[u8], l: &ResourceLimits, retained: usize) -> Result<()> {
    use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
    const ITEM: usize = size_of::<PropertyValue>() * 2;
    struct Scan<'a, 'b> {
        budget: &'a mut Budget<'b>,
        depth: usize,
        item_bytes: usize,
    }
    impl<'de> DeserializeSeed<'de> for Scan<'_, '_> {
        type Value = usize;
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            d: D,
        ) -> std::result::Result<usize, D::Error> {
            if self.depth > 64 {
                return Err(serde::de::Error::custom("image JSON nesting exceeds 64"));
            }
            d.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Scan<'_, '_> {
        type Value = usize;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded image mapping")
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<usize, E> {
            Ok(ITEM)
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<usize, E> {
            Ok(ITEM)
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<usize, E> {
            Ok(ITEM)
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<usize, E> {
            Ok(ITEM)
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<usize, E> {
            Ok(ITEM)
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> std::result::Result<usize, E> {
            self.budget.text(s).map_err(E::custom)?;
            // Map keys determine the actual element storage. In particular raw
            // byte properties must not be charged as full enum values per byte.
            Ok(match s {
                "cells" => size_of::<Cell>() * 2,
                "properties" | "headers" | "localized" | "container_properties" => {
                    size_of::<crate::model::Property>() * 2
                }
                "Bytes" | "raw" => 2,
                "codepoints" => size_of::<Codepoint>() * 2,
                "unicode_sequences" => size_of::<Vec<u32>>() * 2,
                "xlfd_fields" => size_of::<String>() * 2,
                _ => ITEM,
            })
        }
        fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<usize, A::Error> {
            self.budget.bytes(128).map_err(serde::de::Error::custom)?;
            while let Some(item_bytes) = a.next_key_seed(Scan {
                budget: self.budget,
                depth: self.depth + 1,
                item_bytes: ITEM,
            })? {
                a.next_value_seed(Scan {
                    budget: self.budget,
                    depth: self.depth + 1,
                    item_bytes,
                })?;
            }
            Ok(ITEM)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<usize, A::Error> {
            self.budget.bytes(48).map_err(serde::de::Error::custom)?;
            while a
                .next_element_seed(Scan {
                    budget: self.budget,
                    depth: self.depth + 1,
                    item_bytes: ITEM,
                })?
                .is_some()
            {
                self.budget
                    .bytes(self.item_bytes)
                    .map_err(serde::de::Error::custom)?;
            }
            Ok(ITEM)
        }
    }
    let mut b = Budget::new(l, retained)?;
    b.bytes(size_of::<Mapping>())?;
    b.bytes(bytes.len().saturating_mul(2))?;
    let mut d = serde_json::Deserializer::from_slice(bytes);
    Scan {
        budget: &mut b,
        depth: 0,
        item_bytes: ITEM,
    }
    .deserialize(&mut d)
    .map_err(|e| invalid(format!("image JSON preflight: {e}")))?;
    d.end()
        .map_err(|e| invalid(format!("image JSON trailing data: {e}")))
}
