//! Bounded OpenType single substitutions, script/language routing and closure.
use super::{Limits, bounded, bytes, invalid, p16, p32, to16, to32, u16_at, u32_at};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};

/// A printable four-byte OpenType tag.
pub type Tag = [u8; 4];
/// Feature selection for one language system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LanguageSystem {
    /// Required feature, or none.
    pub required: Option<u16>,
    /// Optional features, in source order.
    pub features: Vec<u16>,
}
/// A script's default and explicitly tagged languages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    /// Default language system (not a language named `dflt`).
    pub default: Option<LanguageSystem>,
    /// Ordered language systems.
    pub languages: BTreeMap<Tag, LanguageSystem>,
}
/// One feature and its ordered lookup indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    /// Usually `locl` for regional fonts.
    pub tag: Tag,
    /// Indices into `Gsub::lookups`.
    pub lookups: Vec<u16>,
}
/// Lossless semantics of GSUB 1.0/1.1 without variation or filtering behavior.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gsub {
    /// Script and language feature routing.
    pub scripts: BTreeMap<Tag, Script>,
    /// Feature records, with duplicates allowed as OpenType requires.
    pub features: Vec<Feature>,
    /// Single-substitution maps, in application order.
    pub lookups: Vec<BTreeMap<u16, u16>>,
}
/// Additional cumulative traversal budgets, including aliasing offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
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
) -> Result<Vec<u16>> {
    charge(total, count, l)?;
    bytes(data, at, count * 2)?;
    (0..count)
        .map(|i| {
            let id = u16_at(data, at + 2 * i)?;
            if usize::from(id) >= limit {
                Err(invalid("layout index out of bounds"))
            } else {
                Ok(id)
            }
        })
        .collect()
}
fn language(
    data: &[u8],
    at: usize,
    features: usize,
    total: &mut usize,
    l: &LayoutLimits,
) -> Result<LanguageSystem> {
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
    let f = ids(data, at + 6, count, features, total, l)?;
    Ok(LanguageSystem {
        required,
        features: f,
    })
}
fn coverage(
    data: &[u8],
    at: usize,
    count: u16,
    total: &mut usize,
    l: &LayoutLimits,
) -> Result<Vec<u16>> {
    let n = usize::from(u16_at(data, at + 2)?);
    match u16_at(data, at)? {
        1 => {
            let out = ids(data, at + 4, n, usize::from(count), total, l)?;
            if out.windows(2).any(|p| p[0] >= p[1]) {
                return Err(invalid("unsorted GSUB coverage"));
            }
            Ok(out)
        }
        2 => {
            charge(total, n, l)?;
            bytes(data, at + 4, n * 6)?;
            let mut out = Vec::new();
            for i in 0..n {
                let o = at + 4 + i * 6;
                let a = u16_at(data, o)?;
                let b = u16_at(data, o + 2)?;
                if a > b
                    || b >= count
                    || usize::from(u16_at(data, o + 4)?) != out.len()
                    || out.last().is_some_and(|&p| p >= a)
                {
                    return Err(invalid("invalid GSUB coverage range"));
                }
                charge(total, usize::from(b - a) + 1, l)?;
                out.extend(a..=b);
            }
            Ok(out)
        }
        _ => Err(invalid("unsupported GSUB coverage format")),
    }
}
impl Gsub {
    /// Parse single lookups and extension wrappers; unsupported behavior fails.
    pub fn decode(data: &[u8], num_glyphs: u16, l: &LayoutLimits) -> Result<Self> {
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
        let mut result = Self::default();
        let nl = usize::from(u16_at(data, ll)?);
        charge(&mut total, nl, l)?;
        bytes(data, ll + 2, nl * 2)?;
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
                )?;
                let values = match u16_at(data, sub)? {
                    1 => {
                        let d = u16_at(data, sub + 4)?;
                        cov.iter().map(|a| a.wrapping_add(d)).collect::<Vec<_>>()
                    }
                    2 => {
                        if usize::from(u16_at(data, sub + 4)?) != cov.len() {
                            return Err(invalid("GSUB coverage/substitute counts differ"));
                        }
                        ids(
                            data,
                            sub + 6,
                            cov.len(),
                            usize::from(num_glyphs),
                            &mut total,
                            l,
                        )?
                    }
                    _ => return Err(invalid("unsupported single substitution format")),
                };
                for (a, b) in cov.into_iter().zip(values) {
                    if b >= num_glyphs {
                        return Err(invalid("GSUB replacement out of bounds"));
                    }
                    // OpenType applies only the first matching subtable.
                    map.entry(a).or_insert(b);
                }
            }
            result.lookups.push(map);
        }
        let nf = usize::from(u16_at(data, fl)?);
        charge(&mut total, nf, l)?;
        bytes(data, fl + 2, nf * 6)?;
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
            result.features.push(Feature {
                tag: t,
                lookups: ids(data, at + 4, n, nl, &mut total, l)?,
            });
        }
        let ns = usize::from(u16_at(data, sl)?);
        charge(&mut total, ns, l)?;
        bytes(data, sl + 2, ns * 6)?;
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
                Some(language(data, relative(data, at, at)?, nf, &mut total, l)?)
            };
            let n = usize::from(u16_at(data, at + 2)?);
            charge(&mut total, n, l)?;
            bytes(data, at + 4, n * 6)?;
            let mut languages = BTreeMap::new();
            let mut previous = None;
            for j in 0..n {
                let t = tag(data, at + 4 + j * 6)?;
                if previous.is_some_and(|p| p >= t) {
                    return Err(invalid("unsorted language records"));
                }
                previous = Some(t);
                languages.insert(
                    t,
                    language(data, relative(data, at, at + 8 + j * 6)?, nf, &mut total, l)?,
                );
            }
            result.scripts.insert(t, Script { default, languages });
        }
        Ok(result)
    }
    /// Encode with extension lookups so large CJK tables have valid offsets.
    pub fn encode(&self, num_glyphs: u16, l: &LayoutLimits) -> Result<Vec<u8>> {
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
        let mut scripts = vec![0; 2 + 6 * self.scripts.len()];
        scripts[..2].copy_from_slice(&to16(self.scripts.len())?.to_be_bytes());
        for (i, (t, s)) in self.scripts.iter().enumerate() {
            let start = scripts.len();
            scripts[2 + 6 * i..6 + 6 * i].copy_from_slice(t);
            put16(&mut scripts, 6 + 6 * i, start)?;
            let mut sub = vec![0; 4 + 6 * s.languages.len()];
            put16(&mut sub, 2, s.languages.len())?;
            if let Some(d) = &s.default {
                let n = sub.len();
                put16(&mut sub, 0, n)?;
                write_language(&mut sub, d)?;
            }
            for (j, (t, d)) in s.languages.iter().enumerate() {
                sub[4 + j * 6..8 + j * 6].copy_from_slice(t);
                let n = sub.len();
                put16(&mut sub, 8 + j * 6, n)?;
                write_language(&mut sub, d)?;
            }
            scripts.extend(sub);
        }
        let mut features = vec![0; 2 + 6 * self.features.len()];
        put16(&mut features, 0, self.features.len())?;
        for (i, f) in self.features.iter().enumerate() {
            features[2 + 6 * i..6 + 6 * i].copy_from_slice(&f.tag);
            let n = features.len();
            put16(&mut features, 6 + 6 * i, n)?;
            p16(&mut features, 0);
            p16(&mut features, to16(f.lookups.len())?);
            for &id in &f.lookups {
                p16(&mut features, id);
            }
        }
        let mut lookups = vec![0; 2 + 2 * self.lookups.len()];
        put16(&mut lookups, 0, self.lookups.len())?;
        let mut payloads = Vec::new();
        for (i, map) in self.lookups.iter().enumerate() {
            let start = lookups.len();
            put16(&mut lookups, 2 + 2 * i, start)?;
            let pairs = map.iter().map(|(&a, &b)| (a, b)).collect::<Vec<_>>();
            let chunks = pairs.len().div_ceil(16000).max(1);
            p16(&mut lookups, 7);
            p16(&mut lookups, 0);
            p16(&mut lookups, to16(chunks)?);
            let offsets = lookups.len();
            lookups.resize(offsets + 2 * chunks, 0);
            for j in 0..chunks {
                let ext = lookups.len();
                put16(&mut lookups, offsets + 2 * j, ext - start)?;
                p16(&mut lookups, 1);
                p16(&mut lookups, 1);
                p32(&mut lookups, 0);
                let slice =
                    &pairs[(j * 16000).min(pairs.len())..((j + 1) * 16000).min(pairs.len())];
                let mut p = Vec::new();
                p16(&mut p, 2);
                p16(&mut p, to16(6 + 2 * slice.len())?);
                p16(&mut p, to16(slice.len())?);
                for &(_, b) in slice {
                    p16(&mut p, b);
                }
                p16(&mut p, 1);
                p16(&mut p, to16(slice.len())?);
                for &(a, _) in slice {
                    p16(&mut p, a);
                }
                payloads.push((ext, p));
            }
        }
        for (ext, p) in payloads {
            let off = to32(lookups.len() - ext)?;
            lookups[ext + 4..ext + 8].copy_from_slice(&off.to_be_bytes());
            lookups.extend(p);
        }
        let mut out = Vec::new();
        p32(&mut out, 0x10000);
        p16(&mut out, 10);
        p16(&mut out, to16(10 + scripts.len())?);
        p16(&mut out, to16(10 + scripts.len() + features.len())?);
        out.extend(scripts);
        out.extend(features);
        out.extend(lookups);
        l.sfnt.table(out.len())?;
        // Apply exactly the reader's integrity/work checks to caller-made models.
        Self::decode(&out, num_glyphs, l)?;
        Ok(out)
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
fn write_language(out: &mut Vec<u8>, s: &LanguageSystem) -> Result<()> {
    p16(out, 0);
    p16(out, s.required.unwrap_or(0xffff));
    p16(out, to16(s.features.len())?);
    for &f in &s.features {
        p16(out, f);
    }
    Ok(())
}
