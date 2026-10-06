//! Bounded OpenType single substitutions, script/language routing and closure.
use super::working::{Budget, Buffer, Lease, Owned, Vector};
use super::{Limits, bounded, bytes, invalid, to16, to32, u16_at, u32_at};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};

/// A printable four-byte OpenType tag.
pub type Tag = [u8; 4];
/// Feature selection for one language system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct LanguageSystem {
    /// Required feature, or none.
    pub required: Option<u16>,
    /// Optional features, in source order.
    pub features: Vec<u16>,
}
/// A script's default and explicitly tagged languages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Script {
    /// Default language system (not a language named `dflt`).
    pub default: Option<LanguageSystem>,
    /// Ordered language systems.
    #[cfg_attr(feature = "serde", serde(with = "crate::sfnt::serde_tag::map"))]
    pub languages: BTreeMap<Tag, LanguageSystem>,
}
/// One feature and its ordered lookup indices.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Feature {
    /// Usually `locl` for regional fonts.
    #[cfg_attr(feature = "serde", serde(with = "crate::sfnt::serde_tag::tag"))]
    pub tag: Tag,
    /// Indices into `Gsub::lookups`.
    pub lookups: Vec<u16>,
}
/// Lossless semantics of GSUB 1.0/1.1 without variation or filtering behavior.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Gsub {
    /// Script and language feature routing.
    #[cfg_attr(feature = "serde", serde(with = "crate::sfnt::serde_tag::map"))]
    pub scripts: BTreeMap<Tag, Script>,
    /// Feature records, with duplicates allowed as OpenType requires.
    pub features: Vec<Feature>,
    /// Single-substitution maps, in application order.
    pub lookups: Vec<BTreeMap<u16, u16>>,
}
/// Additional cumulative traversal budgets, including aliasing offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct LayoutLimits {
    /// Container and allocation limits.
    pub sfnt: Limits,
    /// Total records/pairs visited across all tables, including aliases.
    pub max_records: usize,
}
impl Default for LayoutLimits {
    fn default() -> Self {
        Self {
            sfnt: Limits::default(),
            max_records: 1_000_000,
        }
    }
}
fn tag(data: &[u8], at: usize) -> Result<Tag> {
    let t: Tag = bytes(data, at, 4)?.try_into().map_err(invalid)?;
    if t.iter().any(|v| !(32..=126).contains(v)) || t[0] == b' ' {
        return Err(invalid("invalid layout tag"));
    }
    Ok(t)
}
fn relative(data: &[u8], base: usize, at: usize) -> Result<usize> {
    let n = usize::from(u16_at(data, at)?);
    if n == 0 {
        return Err(invalid("NULL required layout offset"));
    }
    base.checked_add(n)
        .filter(|&n| n < data.len())
        .ok_or_else(|| invalid("layout offset outside table"))
}
fn charge(total: &mut usize, n: usize, l: &LayoutLimits) -> Result<()> {
    *total = total
        .checked_add(n)
        .ok_or_else(|| invalid("layout work overflow"))?;
    bounded(*total, l.max_records, "GSUB records")
}
fn ids(
    data: &[u8],
    at: usize,
    count: usize,
    limit: usize,
    total: &mut usize,
    l: &LayoutLimits,
    budget: &Budget,
) -> Result<Owned<Vec<u16>>> {
    charge(total, count, l)?;
    bytes(data, at, count * 2)?;
    let mut out = Vector::with_capacity(budget, count)?;
    for i in 0..count {
        let id = u16_at(data, at + 2 * i)?;
        if usize::from(id) >= limit {
            return Err(invalid("layout index out of bounds"));
        }
        out.push(id)?;
    }
    Ok(out.into_owned())
}
fn language(
    data: &[u8],
    at: usize,
    features: usize,
    total: &mut usize,
    l: &LayoutLimits,
    budget: &Budget,
) -> Result<Owned<LanguageSystem>> {
    if u16_at(data, at)? != 0 {
        return Err(invalid("nonzero LangSys lookupOrder"));
    }
    let r = u16_at(data, at + 2)?;
    let required = if r == 0xffff {
        None
    } else if usize::from(r) < features {
        Some(r)
    } else {
        return Err(invalid("required feature out of bounds"));
    };
    let count = usize::from(u16_at(data, at + 4)?);
    let (features, lease) = ids(data, at + 6, count, features, total, l, budget)?.into_parts();
    Ok(Owned::from_parts(
        LanguageSystem { required, features },
        lease,
    ))
}
fn coverage(
    data: &[u8],
    at: usize,
    count: u16,
    total: &mut usize,
    l: &LayoutLimits,
    budget: &Budget,
) -> Result<Owned<Vec<u16>>> {
    let n = usize::from(u16_at(data, at + 2)?);
    match u16_at(data, at)? {
        1 => {
            let out = ids(data, at + 4, n, usize::from(count), total, l, budget)?;
            if out.windows(2).any(|p| p[0] >= p[1]) {
                return Err(invalid("unsorted GSUB coverage"));
            }
            Ok(out)
        }
        2 => {
            charge(total, n, l)?;
            bytes(data, at + 4, n * 6)?;
            // Validate and count first, then allocate once for all expanded IDs.
            let mut length = 0;
            let mut previous = None;
            for i in 0..n {
                let o = at + 4 + i * 6;
                let a = u16_at(data, o)?;
                let b = u16_at(data, o + 2)?;
                if a > b
                    || b >= count
                    || usize::from(u16_at(data, o + 4)?) != length
                    || previous.is_some_and(|p| p >= a)
                {
                    return Err(invalid("invalid GSUB coverage range"));
                }
                let span = usize::from(b - a) + 1;
                charge(total, span, l)?;
                length += span;
                previous = Some(b);
            }
            let mut out = Vector::with_capacity(budget, length)?;
            for i in 0..n {
                let o = at + 4 + i * 6;
                for id in u16_at(data, o)?..=u16_at(data, o + 2)? {
                    out.push(id)?;
                }
            }
            Ok(out.into_owned())
        }
        _ => Err(invalid("unsupported GSUB coverage format")),
    }
}
impl Gsub {
    /// Parse single lookups and extension wrappers; unsupported behavior fails.
    pub fn decode(data: &[u8], num_glyphs: u16, l: &LayoutLimits) -> Result<Self> {
        let budget = Budget::new(l.sfnt.resources.max_total_bytes, "GSUB working bytes");
        let (value, _lease) = Self::decode_with_budget(data, num_glyphs, l, &budget)?.into_parts();
        Ok(value)
    }
    pub(crate) fn decode_with_budget(
        data: &[u8],
        num_glyphs: u16,
        l: &LayoutLimits,
        budget: &Budget,
    ) -> Result<Owned<Self>> {
        l.sfnt.table(data.len())?;
        l.sfnt.glyphs(usize::from(num_glyphs))?;
        let version = u32_at(data, 0)?;
        if version != 0x10000 && (version != 0x10001 || u32_at(data, 10)? != 0) {
            return Err(invalid("unsupported GSUB version or feature variations"));
        }
        let (sl, fl, ll) = (
            relative(data, 0, 4)?,
            relative(data, 0, 6)?,
            relative(data, 0, 8)?,
        );
        let mut total = 0;
        // Nested charges outlive the outer vectors and maps on every error path.
        let mut lease = budget.reserve(0)?;
        let nl = usize::from(u16_at(data, ll)?);
        charge(&mut total, nl, l)?;
        bytes(data, ll + 2, nl * 2)?;
        let mut lookups = Vector::with_capacity(budget, nl)?;
        for i in 0..nl {
            let at = relative(data, ll, ll + 2 + 2 * i)?;
            let kind = u16_at(data, at)?;
            if ![1, 7].contains(&kind) || u16_at(data, at + 2)? != 0 {
                return Err(invalid(
                    "GSUB supports only unfiltered single substitutions",
                ));
            }
            let n = usize::from(u16_at(data, at + 4)?);
            if n == 0 {
                return Err(invalid("empty GSUB lookup"));
            }
            charge(&mut total, n, l)?;
            bytes(data, at + 6, n * 2)?;
            let mut map_lease = budget.reserve(0)?;
            let mut map = BTreeMap::new();
            for j in 0..n {
                let mut sub = relative(data, at, at + 6 + 2 * j)?;
                if kind == 7 {
                    if u16_at(data, sub)? != 1 || u16_at(data, sub + 2)? != 1 {
                        return Err(invalid("unsupported GSUB extension"));
                    }
                    let off = u32_at(data, sub + 4)? as usize;
                    if off < 8 {
                        return Err(invalid("invalid GSUB extension offset"));
                    }
                    sub = sub
                        .checked_add(off)
                        .filter(|&n| n < data.len())
                        .ok_or_else(|| invalid("GSUB extension outside table"))?;
                }
                let cov = coverage(
                    data,
                    relative(data, sub, sub + 2)?,
                    num_glyphs,
                    &mut total,
                    l,
                    budget,
                )?;
                let (delta, values) = match u16_at(data, sub)? {
                    1 => (u16_at(data, sub + 4)?, None),
                    2 => {
                        if usize::from(u16_at(data, sub + 4)?) != cov.len() {
                            return Err(invalid("GSUB coverage/substitute counts differ"));
                        }
                        (
                            0,
                            Some(ids(
                                data,
                                sub + 6,
                                cov.len(),
                                usize::from(num_glyphs),
                                &mut total,
                                l,
                                budget,
                            )?),
                        )
                    }
                    _ => return Err(invalid("unsupported single substitution format")),
                };
                for (index, &a) in cov.iter().enumerate() {
                    let b = values
                        .as_ref()
                        .map_or_else(|| a.wrapping_add(delta), |v| v[index]);
                    if b >= num_glyphs {
                        return Err(invalid("GSUB replacement out of bounds"));
                    }
                    // OpenType applies only the first matching subtable.
                    if !map.contains_key(&a) {
                        map_lease.grow_to(budget.tree_bytes::<u16, u16>(map.len() + 1)?)?;
                        map.insert(a, b);
                    }
                }
            }
            lease.merge(&mut map_lease)?;
            lookups.push(map)?;
        }
        let nf = usize::from(u16_at(data, fl)?);
        charge(&mut total, nf, l)?;
        bytes(data, fl + 2, nf * 6)?;
        let mut features = Vector::with_capacity(budget, nf)?;
        let mut previous = None;
        for i in 0..nf {
            let t = tag(data, fl + 2 + i * 6)?;
            if previous.is_some_and(|p| p > t) {
                return Err(invalid("unsorted feature records"));
            }
            previous = Some(t);
            let at = relative(data, fl, fl + 6 + i * 6)?;
            if u16_at(data, at)? != 0 {
                return Err(invalid("unsupported GSUB feature parameters"));
            }
            let n = usize::from(u16_at(data, at + 2)?);
            let (ids, mut ids_lease) =
                ids(data, at + 4, n, nl, &mut total, l, budget)?.into_parts();
            lease.merge(&mut ids_lease)?;
            features.push(Feature {
                tag: t,
                lookups: ids,
            })?;
        }
        let ns = usize::from(u16_at(data, sl)?);
        charge(&mut total, ns, l)?;
        bytes(data, sl + 2, ns * 6)?;
        let mut scripts_lease = budget.reserve_tree::<Tag, Script>(ns)?;
        let mut scripts = BTreeMap::new();
        let mut previous = None;
        for i in 0..ns {
            let t = tag(data, sl + 2 + i * 6)?;
            if previous.is_some_and(|p| p >= t) {
                return Err(invalid("unsorted script records"));
            }
            previous = Some(t);
            let at = relative(data, sl, sl + 6 + i * 6)?;
            let default = if u16_at(data, at)? == 0 {
                None
            } else {
                let (lang, mut lang_lease) =
                    language(data, relative(data, at, at)?, nf, &mut total, l, budget)?
                        .into_parts();
                lease.merge(&mut lang_lease)?;
                Some(lang)
            };
            let n = usize::from(u16_at(data, at + 2)?);
            charge(&mut total, n, l)?;
            bytes(data, at + 4, n * 6)?;
            let mut languages_lease = budget.reserve_tree::<Tag, LanguageSystem>(n)?;
            let mut languages = BTreeMap::new();
            let mut previous = None;
            for j in 0..n {
                let t = tag(data, at + 4 + j * 6)?;
                if previous.is_some_and(|p| p >= t) {
                    return Err(invalid("unsorted language records"));
                }
                previous = Some(t);
                let (lang, mut lang_lease) = language(
                    data,
                    relative(data, at, at + 8 + j * 6)?,
                    nf,
                    &mut total,
                    l,
                    budget,
                )?
                .into_parts();
                lease.merge(&mut lang_lease)?;
                languages.insert(t, lang);
            }
            lease.merge(&mut languages_lease)?;
            scripts.insert(t, Script { default, languages });
        }
        let (lookups, mut lookups_lease) = lookups.into_parts();
        let (features, mut features_lease) = features.into_parts();
        lease.merge(&mut scripts_lease)?;
        lease.merge(&mut lookups_lease)?;
        lease.merge(&mut features_lease)?;
        Ok(Owned::from_parts(
            Self {
                scripts,
                features,
                lookups,
            },
            lease,
        ))
    }
    /// Encode with extension lookups so large CJK tables have valid offsets.
    pub fn encode(&self, num_glyphs: u16, l: &LayoutLimits) -> Result<Vec<u8>> {
        let budget = Budget::new(l.sfnt.resources.max_total_bytes, "GSUB working bytes");
        let (value, _lease) = self
            .encode_with_budget(num_glyphs, l, &budget)?
            .into_parts();
        Ok(value)
    }
    pub(crate) fn encode_with_budget(
        &self,
        num_glyphs: u16,
        l: &LayoutLimits,
        budget: &Budget,
    ) -> Result<Owned<Vec<u8>>> {
        l.sfnt.glyphs(usize::from(num_glyphs))?;
        let mut records = 0;
        for n in [self.scripts.len(), self.features.len(), self.lookups.len()] {
            to16(n)?;
            charge(&mut records, n, l)?;
        }
        for s in self.scripts.values() {
            to16(s.languages.len())?;
            charge(&mut records, s.languages.len(), l)?;
            for lang in s.default.iter().chain(s.languages.values()) {
                to16(lang.features.len())?;
                charge(&mut records, lang.features.len(), l)?;
            }
        }
        for f in &self.features {
            to16(f.lookups.len())?;
            charge(&mut records, f.lookups.len(), l)?;
        }
        for m in &self.lookups {
            charge(&mut records, m.len().saturating_mul(2), l)?;
        }
        l.sfnt.table(records.saturating_mul(8))?;
        // Compute final offsets without owning intermediate tables, payloads,
        // or a duplicate array of every substitution pair.
        let mut scripts_len = 2 + 6 * self.scripts.len();
        for (t, s) in &self.scripts {
            add_size(&mut scripts_len, 4 + 6 * s.languages.len())?;
            if let Some(d) = &s.default {
                add_size(&mut scripts_len, language_size(d))?;
            } else if *t == *b"DFLT" {
                add_size(&mut scripts_len, 6)?;
            }
            for d in s.languages.values() {
                add_size(&mut scripts_len, language_size(d))?;
            }
        }
        let mut features_len = 2 + 6 * self.features.len();
        for f in &self.features {
            add_size(&mut features_len, 4 + 2 * f.lookups.len())?;
        }
        let mut lookup_headers = 2 + 2 * self.lookups.len();
        let mut payload_size = 0;
        for map in &self.lookups {
            let chunks = map.len().div_ceil(16000).max(1);
            add_size(&mut lookup_headers, 6 + 10 * chunks)?;
            add_size(&mut payload_size, 10 * chunks + 4 * map.len())?;
        }
        let feature_base = 10usize
            .checked_add(scripts_len)
            .ok_or_else(|| invalid("GSUB size overflow"))?;
        let lookup_base = feature_base
            .checked_add(features_len)
            .ok_or_else(|| invalid("GSUB size overflow"))?;
        let mut size = lookup_base;
        add_size(&mut size, lookup_headers)?;
        add_size(&mut size, payload_size)?;
        l.sfnt.table(size)?;
        let mut out = Buffer::with_capacity(budget, size)?;
        write32(&mut out, 0x10000)?;
        write16(&mut out, 10)?;
        write16(&mut out, to16(feature_base)?)?;
        write16(&mut out, to16(lookup_base)?)?;
        let script_base = out.len();
        out.resize(script_base + 2 + 6 * self.scripts.len(), 0)?;
        put16(&mut out, script_base, self.scripts.len())?;
        for (i, (t, s)) in self.scripts.iter().enumerate() {
            let start = out.len();
            out[script_base + 2 + 6 * i..script_base + 6 + 6 * i].copy_from_slice(t);
            put16(&mut out, script_base + 6 + 6 * i, start - script_base)?;
            out.resize(start + 4 + 6 * s.languages.len(), 0)?;
            put16(&mut out, start + 2, s.languages.len())?;
            let empty_default = LanguageSystem::default();
            if let Some(d) = s
                .default
                .as_ref()
                .or_else(|| (*t == *b"DFLT").then_some(&empty_default))
            {
                let offset = out.len() - start;
                put16(&mut out, start, offset)?;
                write_language(&mut out, d)?;
            }
            for (j, (t, d)) in s.languages.iter().enumerate() {
                out[start + 4 + j * 6..start + 8 + j * 6].copy_from_slice(t);
                let offset = out.len() - start;
                put16(&mut out, start + 8 + j * 6, offset)?;
                write_language(&mut out, d)?;
            }
        }
        debug_assert_eq!(out.len(), feature_base);
        out.resize(feature_base + 2 + 6 * self.features.len(), 0)?;
        put16(&mut out, feature_base, self.features.len())?;
        for (i, f) in self.features.iter().enumerate() {
            out[feature_base + 2 + 6 * i..feature_base + 6 + 6 * i].copy_from_slice(&f.tag);
            let offset = out.len() - feature_base;
            put16(&mut out, feature_base + 6 + 6 * i, offset)?;
            write16(&mut out, 0)?;
            write16(&mut out, to16(f.lookups.len())?)?;
            for &id in &f.lookups {
                write16(&mut out, id)?;
            }
        }
        debug_assert_eq!(out.len(), lookup_base);
        out.resize(lookup_base + 2 + 2 * self.lookups.len(), 0)?;
        put16(&mut out, lookup_base, self.lookups.len())?;
        for (i, map) in self.lookups.iter().enumerate() {
            let start = out.len();
            put16(&mut out, lookup_base + 2 + 2 * i, start - lookup_base)?;
            let chunks = map.len().div_ceil(16000).max(1);
            write16(&mut out, 7)?;
            write16(&mut out, 0)?;
            write16(&mut out, to16(chunks)?)?;
            let offsets = out.len();
            out.resize(offsets + 2 * chunks, 0)?;
            for j in 0..chunks {
                let offset = out.len() - start;
                put16(&mut out, offsets + 2 * j, offset)?;
                write16(&mut out, 1)?;
                write16(&mut out, 1)?;
                write32(&mut out, 0)?;
            }
        }
        debug_assert_eq!(out.len(), lookup_base + lookup_headers);
        let mut start = lookup_base + 2 + 2 * self.lookups.len();
        for map in &self.lookups {
            let chunks = map.len().div_ceil(16000).max(1);
            let mut pairs = map.iter();
            let mut remaining = map.len();
            for j in 0..chunks {
                let ext = start + 6 + 2 * chunks + 8 * j;
                let offset = to32(out.len() - ext)?;
                out[ext + 4..ext + 8].copy_from_slice(&offset.to_be_bytes());
                let length = remaining.min(16000);
                write16(&mut out, 2)?;
                write16(&mut out, to16(6 + 2 * length)?)?;
                write16(&mut out, to16(length)?)?;
                for (_, &b) in pairs.clone().take(length) {
                    write16(&mut out, b)?;
                }
                write16(&mut out, 1)?;
                write16(&mut out, to16(length)?)?;
                for (&a, _) in pairs.by_ref().take(length) {
                    write16(&mut out, a)?;
                }
                remaining -= length;
            }
            start += 6 + 10 * chunks;
        }
        debug_assert_eq!(out.len(), size);
        // Preserve the reader's complete integrity/work checks. Its owned
        // validation model coexists with the final bytes in the same ledger.
        Self::decode_with_budget(&out, num_glyphs, l, budget)?;
        Ok(out.into_owned())
    }
    /// Expand a caller-owned glyph tree while preserving its existing lease.
    pub(crate) fn closure_with_budget(
        &self,
        glyphs: &mut BTreeSet<u16>,
        glyphs_lease: &mut Lease,
        budget: &Budget,
    ) -> Result<()> {
        let mut active = Vector::with_capacity(budget, self.lookups.len())?;
        active.resize(self.lookups.len(), false)?;
        for id in self
            .scripts
            .values()
            .flat_map(|s| s.default.iter().chain(s.languages.values()))
            .flat_map(|s| s.required.iter().chain(&s.features))
            .filter_map(|&i| self.features.get(usize::from(i)))
            .flat_map(|f| f.lookups.iter())
        {
            if let Some(slot) = active.get_mut(usize::from(*id)) {
                *slot = true;
            }
        }
        let mut edge_count = 0;
        for (map, &used) in self.lookups.iter().zip(active.iter()) {
            if used {
                add_size(&mut edge_count, map.len())?;
            }
        }
        let mut edges = Vector::with_capacity(budget, edge_count)?;
        for (map, &used) in self.lookups.iter().zip(active.iter()) {
            if used {
                for (&a, &b) in map {
                    edges.push((a, b))?;
                }
            }
        }
        edges.sort_unstable();
        drop(active);
        let mut queue = Vector::with_capacity(budget, glyphs.len())?;
        for &glyph in glyphs.iter() {
            queue.push(glyph)?;
        }
        let mut next = 0;
        while next < queue.len() {
            let a = queue[next];
            next += 1;
            let first = edges.partition_point(|&(from, _)| from < a);
            let end = edges.partition_point(|&(from, _)| from <= a);
            for &(_, b) in &edges[first..end] {
                if !glyphs.contains(&b) {
                    glyphs_lease.grow_to(budget.tree_bytes::<u16, ()>(glyphs.len() + 1)?)?;
                    queue.reserve(1)?;
                    glyphs.insert(b);
                    queue.push(b)?;
                }
            }
        }
        Ok(())
    }
    /// Build the retained model alongside its still-live source model.
    pub(crate) fn subset_with_budget(
        &self,
        mapping: &BTreeMap<u16, u16>,
        budget: &Budget,
    ) -> Result<Owned<Self>> {
        let mut lease = budget.reserve(0)?;
        // Dense source-index correspondence avoids temporary trees with owned
        // payload values. Every retained ID remains in original application order.
        let mut lookup_ids = Vector::with_capacity(budget, self.lookups.len())?;
        lookup_ids.resize(self.lookups.len(), None)?;
        let mut feature_ids = Vector::with_capacity(budget, self.features.len())?;
        feature_ids.resize(self.features.len(), None)?;
        let mut lookups = Vector::new(budget)?;
        for (i, source) in self.lookups.iter().enumerate() {
            let length = source
                .iter()
                .filter(|&(a, b)| mapping.contains_key(a) && mapping.contains_key(b))
                .count();
            if length == 0 {
                continue;
            }
            lookups.reserve(1)?;
            let mut pairs_lease = budget.reserve_tree::<u16, u16>(length)?;
            let mut pairs = BTreeMap::new();
            for (a, b) in source {
                if let (Some(&a), Some(&b)) = (mapping.get(a), mapping.get(b)) {
                    pairs.insert(a, b);
                }
            }
            lookup_ids[i] = Some(to16(lookups.len())?);
            lease.merge(&mut pairs_lease)?;
            lookups.push(pairs)?;
        }
        let mut features = Vector::new(budget)?;
        for (i, source) in self.features.iter().enumerate() {
            let retained = |id: &u16| lookup_ids.get(usize::from(*id)).copied().flatten();
            let length = source.lookups.iter().filter_map(retained).count();
            if length == 0 {
                continue;
            }
            features.reserve(1)?;
            let mut ids = Vector::with_capacity(budget, length)?;
            for id in source.lookups.iter().filter_map(retained) {
                ids.push(id)?;
            }
            feature_ids[i] = Some(to16(features.len())?);
            let (ids, mut ids_lease) = ids.into_parts();
            lease.merge(&mut ids_lease)?;
            features.push(Feature {
                tag: source.tag,
                lookups: ids,
            })?;
        }
        let mut scripts_lease = budget.reserve_tree::<Tag, Script>(self.scripts.len())?;
        let mut scripts = BTreeMap::new();
        for (&tag, source) in &self.scripts {
            let default = if let Some(source) = &source.default {
                let (lang, mut lang_lease) =
                    subset_language(source, &feature_ids, budget)?.into_parts();
                lease.merge(&mut lang_lease)?;
                Some(lang)
            } else {
                None
            };
            let mut languages_lease =
                budget.reserve_tree::<Tag, LanguageSystem>(source.languages.len())?;
            let mut languages = BTreeMap::new();
            for (&tag, source) in &source.languages {
                let (lang, mut lang_lease) =
                    subset_language(source, &feature_ids, budget)?.into_parts();
                lease.merge(&mut lang_lease)?;
                languages.insert(tag, lang);
            }
            lease.merge(&mut languages_lease)?;
            scripts.insert(tag, Script { default, languages });
        }
        let (lookups, mut lookups_lease) = lookups.into_parts();
        let (features, mut features_lease) = features.into_parts();
        lease.merge(&mut scripts_lease)?;
        lease.merge(&mut lookups_lease)?;
        lease.merge(&mut features_lease)?;
        Ok(Owned::from_parts(
            Self {
                scripts,
                features,
                lookups,
            },
            lease,
        ))
    }
    /// Close a glyph set over every active single-substitution chain in O(E+V).
    pub fn closure(&self, glyphs: &mut BTreeSet<u16>) {
        let mut edges = BTreeMap::<u16, Vec<u16>>::new();
        let active = self
            .scripts
            .values()
            .flat_map(|s| s.default.iter().chain(s.languages.values()))
            .flat_map(|s| s.required.iter().chain(&s.features))
            .filter_map(|&i| self.features.get(usize::from(i)))
            .flat_map(|f| f.lookups.iter())
            .copied()
            .collect::<BTreeSet<_>>();
        for i in active {
            if let Some(m) = self.lookups.get(usize::from(i)) {
                for (&a, &b) in m {
                    edges.entry(a).or_default().push(b);
                }
            }
        }
        let mut queue = glyphs.iter().copied().collect::<Vec<_>>();
        while let Some(a) = queue.pop() {
            if let Some(next) = edges.get(&a) {
                for &b in next {
                    if glyphs.insert(b) {
                        queue.push(b);
                    }
                }
            }
        }
    }
    /// Remap retained pairs and prune unused lookups/features; routing stays exact.
    pub fn subset(&self, mapping: &BTreeMap<u16, u16>) -> Self {
        let mut out = Self::default();
        let mut lookups = BTreeMap::new();
        let mut features = BTreeMap::new();
        for (i, m) in self.lookups.iter().enumerate() {
            let pairs = m
                .iter()
                .filter_map(|(a, b)| Some((*mapping.get(a)?, *mapping.get(b)?)))
                .collect::<BTreeMap<_, _>>();
            if !pairs.is_empty() {
                lookups.insert(i as u16, out.lookups.len() as u16);
                out.lookups.push(pairs);
            }
        }
        for (i, f) in self.features.iter().enumerate() {
            let ids = f
                .lookups
                .iter()
                .filter_map(|j| lookups.get(j).copied())
                .collect::<Vec<_>>();
            if !ids.is_empty() {
                features.insert(i as u16, out.features.len() as u16);
                out.features.push(Feature {
                    tag: f.tag,
                    lookups: ids,
                });
            }
        }
        let map_lang = |s: &LanguageSystem| LanguageSystem {
            required: s.required.and_then(|r| features.get(&r).copied()),
            features: s
                .features
                .iter()
                .filter_map(|f| features.get(f).copied())
                .collect(),
        };
        for (&tag, s) in &self.scripts {
            out.scripts.insert(
                tag,
                Script {
                    default: s.default.as_ref().map(map_lang),
                    languages: s.languages.iter().map(|(&t, l)| (t, map_lang(l))).collect(),
                },
            );
        }
        out
    }
}
fn put16(v: &mut [u8], at: usize, n: usize) -> Result<()> {
    v[at..at + 2].copy_from_slice(&to16(n)?.to_be_bytes());
    Ok(())
}
fn write16(out: &mut Buffer, n: u16) -> Result<()> {
    out.extend(&n.to_be_bytes())
}
fn write32(out: &mut Buffer, n: u32) -> Result<()> {
    out.extend(&n.to_be_bytes())
}
fn add_size(size: &mut usize, extra: usize) -> Result<()> {
    *size = size
        .checked_add(extra)
        .ok_or_else(|| invalid("GSUB size overflow"))?;
    Ok(())
}
fn language_size(s: &LanguageSystem) -> usize {
    6 + 2 * s.features.len()
}
fn write_language(out: &mut Buffer, s: &LanguageSystem) -> Result<()> {
    write16(out, 0)?;
    write16(out, s.required.unwrap_or(0xffff))?;
    write16(out, to16(s.features.len())?)?;
    for &f in &s.features {
        write16(out, f)?;
    }
    Ok(())
}

fn subset_language(
    source: &LanguageSystem,
    ids: &[Option<u16>],
    budget: &Budget,
) -> Result<Owned<LanguageSystem>> {
    let retained = |id: &u16| ids.get(usize::from(*id)).copied().flatten();
    let length = source.features.iter().filter_map(retained).count();
    let mut features = Vector::with_capacity(budget, length)?;
    for id in source.features.iter().filter_map(retained) {
        features.push(id)?;
    }
    let required = source.required.as_ref().and_then(retained);
    let (features, lease) = features.into_parts();
    Ok(Owned::from_parts(
        LanguageSystem { required, features },
        lease,
    ))
}

#[cfg(test)]
mod working_tests {
    use super::*;

    fn routed() -> Gsub {
        let language = LanguageSystem {
            required: Some(0),
            features: vec![0],
        };
        Gsub {
            scripts: BTreeMap::from([(
                *b"DFLT",
                Script {
                    default: Some(language.clone()),
                    languages: BTreeMap::from([(*b"JAN ", language)]),
                },
            )]),
            features: vec![
                Feature {
                    tag: *b"locl",
                    lookups: vec![0, 1],
                },
                Feature {
                    tag: *b"ss01",
                    lookups: vec![2],
                },
            ],
            lookups: vec![
                BTreeMap::from([(1, 2), (2, 3), (4, 5)]),
                BTreeMap::from([(3, 1), (5, 6)]),
                BTreeMap::from([(1, 7)]),
            ],
        }
    }

    #[test]
    fn decoded_model_keeps_nested_charges_until_drop() {
        let model = routed();
        let limits = LayoutLimits::default();
        let wire = model.encode(8, &limits).unwrap();
        let budget = Budget::new(1_000_000, "test GSUB bytes");
        let decoded = Gsub::decode_with_budget(&wire, 8, &limits, &budget).unwrap();
        assert_eq!(*decoded, model);
        let available = budget.remaining();
        assert!(available < 1_000_000);
        let occupied = budget.reserve(available).unwrap();
        assert!(matches!(
            Gsub::decode_with_budget(&wire, 8, &limits, &budget),
            Err(crate::Error::ResourceLimit { .. })
        ));
        assert_eq!(budget.remaining(), 0);
        assert_eq!(decoded.lookups[0][&1], 2);
        drop(occupied);
        assert_eq!(budget.remaining(), available);
        drop(decoded);
        assert_eq!(budget.remaining(), 1_000_000);
    }

    #[test]
    fn encoded_bytes_overlap_full_validation_model() {
        let model = routed();
        let limits = LayoutLimits::default();
        let wire = model.encode(8, &limits).unwrap();
        let budget = Budget::new(wire.len(), "test GSUB bytes");
        assert!(matches!(
            model.encode_with_budget(8, &limits, &budget),
            Err(crate::Error::ResourceLimit { .. })
        ));
        assert_eq!(budget.remaining(), wire.len());
        let budget = Budget::new(1_000_000, "test GSUB bytes");
        let encoded = model.encode_with_budget(8, &limits, &budget).unwrap();
        assert_eq!(encoded.as_slice(), wire);
        assert_eq!(budget.remaining(), 1_000_000 - encoded.capacity());
        drop(encoded);
        assert_eq!(budget.remaining(), 1_000_000);
    }

    #[test]
    fn closure_and_remap_preserve_routes_cycles_and_inactive_lookups() {
        let model = routed();
        let budget = Budget::new(1_000_000, "test GSUB bytes");
        let mut keep_lease = budget.reserve_tree::<u16, ()>(2).unwrap();
        let mut keep = BTreeSet::from([0, 1]);
        model
            .closure_with_budget(&mut keep, &mut keep_lease, &budget)
            .unwrap();
        assert_eq!(keep, BTreeSet::from([0, 1, 2, 3]));
        // Closure scratch is gone; only the caller's enlarged glyph tree remains.
        assert_eq!(budget.remaining(), 1_000_000 - keep_lease.bytes());
        let mapping = BTreeMap::from([(0, 0), (1, 3), (2, 1), (3, 2)]);
        let remapped = model.subset_with_budget(&mapping, &budget).unwrap();
        assert_eq!(*remapped, model.subset(&mapping));
        assert_eq!(
            remapped.lookups,
            vec![BTreeMap::from([(1, 2), (3, 1)]), BTreeMap::from([(2, 3)])]
        );
        assert_eq!(remapped.features.len(), 1);
        assert_eq!(
            remapped.scripts[b"DFLT"].languages[b"JAN "].required,
            Some(0)
        );
        drop(remapped);
        drop(keep);
        drop(keep_lease);
        assert_eq!(budget.remaining(), 1_000_000);
    }

    #[test]
    fn closure_admits_growth_before_mutating_the_callers_tree() {
        let model = routed();
        let budget = Budget::new(100_000, "test GSUB bytes");
        let mut keep_lease = budget.reserve_tree::<u16, ()>(2).unwrap();
        let mut keep = BTreeSet::from([0, 1]);
        let occupied = budget.reserve(budget.remaining()).unwrap();
        assert!(matches!(
            model.closure_with_budget(&mut keep, &mut keep_lease, &budget),
            Err(crate::Error::ResourceLimit { .. })
        ));
        assert_eq!(keep, BTreeSet::from([0, 1]));
        assert_eq!(budget.remaining(), 0);
        drop(occupied);
        drop(keep);
        drop(keep_lease);
        assert_eq!(budget.remaining(), 100_000);
    }

    #[test]
    fn duplicate_subtables_keep_first_match_and_alias_work_limits() {
        // One unreferenced single-substitution lookup, with two matching
        // subtables. Both must be validated although only the first pair wins.
        let words = [
            1u16, 0, 10, 12, 14, 0, 0, 1, 4, 1, 0, 2, 10, 24, 2, 8, 1, 2, 1, 1, 1, 2, 8, 1, 3, 1,
            1, 1,
        ];
        let wire: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
        let limits = LayoutLimits::default();
        let budget = Budget::new(100_000, "test GSUB bytes");
        let decoded = Gsub::decode_with_budget(&wire, 4, &limits, &budget).unwrap();
        assert_eq!(decoded.lookups, vec![BTreeMap::from([(1, 2)])]);
        drop(decoded);
        let mut alias = wire.clone();
        alias[26..28].copy_from_slice(&10u16.to_be_bytes());
        let limits = LayoutLimits {
            max_records: 6,
            ..Default::default()
        };
        assert!(matches!(
            Gsub::decode_with_budget(&alias, 4, &limits, &budget),
            Err(crate::Error::ResourceLimit {
                resource: "GSUB records",
                limit: 6
            })
        ));
        assert_eq!(budget.remaining(), 100_000);
    }
}
