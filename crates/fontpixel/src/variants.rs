//! Regional bitmap alternatives, deterministic deduplication and OpenType output.
use crate::model::{Codepoint, GlyphBitmap};
use crate::sfnt::{
    self,
    gsub::{Feature, Gsub, LanguageSystem, LayoutLimits, Script, Tag},
};
use crate::working::{Budget, Nested, Owned, Vector};
use crate::{Error, Font, Glyph, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
mod dot_rows;
mod export;
mod ledger;
mod tables;
pub use export::{DotReuse, Encoded, Options, OutlineFlavor, encode, encode_single};

/// A language-specific face with explicit OpenType script selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageVariant {
    /// OpenType language-system tag; use `language_tag` for BCP-47 conversion.
    pub language: Tag,
    /// Scripts that activate this alternative.
    pub scripts: Vec<Tag>,
    /// Alternative glyphs; absent codepoints fall back to the base.
    pub font: Font,
}
/// A face selected by script, regardless of the shaping language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptVariant {
    /// Scripts that activate this alternative (e.g. latn/grek/cyrl).
    pub scripts: Vec<Tag>,
    /// Alternative glyphs.
    pub font: Font,
}
/// One default face plus ordered region and script alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variants {
    /// Default face and global metadata source.
    pub base: Font,
    /// Language alternatives, applied in this order.
    pub languages: Vec<LanguageVariant>,
    /// Script alternatives, applied after language planning.
    pub scripts: Vec<ScriptVariant>,
}
impl Variants {
    /// Start with a default face.
    pub fn new(base: Font) -> Self {
        Self {
            base,
            languages: Vec::new(),
            scripts: Vec::new(),
        }
    }
    /// Validate and plan once for repeated square/dot and TTF/CFF exports.
    ///
    /// The immutable plan can be shared across bounded caller-owned threads.
    /// Encoding options must retain these exact layout/resource limits; prepare
    /// again to change them, rather than bypassing source-validation budgets.
    ///
    /// Planning runs on one working ledger of `limits.sfnt.resources.max_total_bytes`.
    /// The retained plan's admitted size is recorded and charged once to every
    /// later [`PreparedVariants::encode`] ledger.
    pub fn prepare(&self, limits: &LayoutLimits) -> Result<PreparedVariants<'_>> {
        let budget = ledger::budget(limits.sfnt.resources.max_total_bytes);
        let (plan, lease) = self.plan_with_budget(limits, &budget)?.into_parts();
        Ok(PreparedVariants {
            base: &self.base,
            plan,
            limits: limits.clone(),
            retained: lease.bytes(),
        })
    }
    /// Add a BCP-47/OpenType language with Tong's default CJK script scope.
    pub fn add_language(&mut self, language: &str, font: Font) -> Result<()> {
        self.languages.push(LanguageVariant {
            language: language_tag(language)?,
            scripts: vec![*b"DFLT", *b"hani", *b"kana", *b"hang", *b"bopo"],
            font,
        });
        Ok(())
    }
    /// Build Unicode glyph order, alternate deduplication and exact locl routing.
    ///
    /// Planning storage is admitted on one working ledger whose maximum is
    /// `limits.sfnt.resources.max_total_bytes`; the returned plan is then owned
    /// by the caller.
    pub fn plan(&self, limits: &LayoutLimits) -> Result<Plan> {
        let budget = ledger::budget(limits.sfnt.resources.max_total_bytes);
        Ok(self.plan_with_budget(limits, &budget)?.into_parts().0)
    }
    /// Plan on a caller's ledger. The returned lease covers the retained plan.
    pub(crate) fn plan_with_budget(
        &self,
        limits: &LayoutLimits,
        budget: &Budget,
    ) -> Result<Owned<Plan>> {
        let alternatives = self
            .languages
            .len()
            .checked_add(self.scripts.len())
            .ok_or_else(|| invalid("variant count overflow"))?;
        sfnt::to16(alternatives)?;
        sfnt::bounded(alternatives, limits.max_records, "variant alternatives")?;
        let mut routes = 0usize;
        for scripts in self
            .languages
            .iter()
            .map(|v| &v.scripts)
            .chain(self.scripts.iter().map(|v| &v.scripts))
        {
            routes = routes.saturating_add(scripts.len());
            sfnt::bounded(routes, limits.max_records, "variant script routes")?;
        }
        {
            let mut lease = budget.reserve(0)?;
            let mut script_tags = BTreeSet::new();
            for v in &self.scripts {
                for &tag in &v.scripts {
                    if script_tags.contains(&tag) {
                        return Err(invalid("duplicate script variant"));
                    }
                    ledger::tree::<Tag, ()>(&mut lease, budget, script_tags.len() + 1)?;
                    script_tags.insert(tag);
                }
            }
        }
        let mut count = 0usize;
        let mut bytes = 0usize;
        for f in std::iter::once(&self.base)
            .chain(self.languages.iter().map(|v| &v.font))
            .chain(self.scripts.iter().map(|v| &v.font))
        {
            f.validate(&limits.sfnt.resources)?;
            count = count
                .checked_add(f.len())
                .ok_or_else(|| invalid("source glyph overflow"))?;
            for (_, g) in f.glyphs() {
                bytes = bytes.saturating_add(g.bitmap.as_bytes().len());
            }
            sfnt::bounded(count, limits.max_records, "variant source glyphs")?;
            sfnt::bounded(
                bytes,
                limits.sfnt.resources.max_total_bytes,
                "variant source bitmap bytes",
            )?;
            if f.metrics.pixel_size != self.base.metrics.pixel_size {
                return Err(invalid("variant pixel sizes differ"));
            }
        }
        // Records stay in insertion order as admitted names plus borrowed source
        // glyphs (with their face, for metric defaults). Bitmaps are copied only
        // once, into the final glyph order. Indexes stand in for names in every
        // temporary map, so no name is copied twice.
        let mut names = Nested::<String>::with_capacity(budget, 0)?;
        let mut records = Vector::<(&Glyph, &Font)>::new(budget)?;
        let mut cmap_lease = budget.reserve(0)?;
        let mut cmap = BTreeMap::<u32, u32>::new();
        for entry in effective(&self.base) {
            let (cp, g) = entry?;
            limits.sfnt.glyphs(records.len().saturating_add(2))?;
            sfnt::bounded(
                cmap.len().saturating_add(1),
                limits.sfnt.max_cmap_mappings,
                "variant Unicode mappings",
            )?;
            let index = index(records.len())?;
            names.push(ledger::string(budget, GlyphName::base(cp))?)?;
            records.push((g, &self.base))?;
            ledger::tree::<u32, u32>(&mut cmap_lease, budget, cmap.len() + 1)?;
            cmap.insert(cp, index);
        }
        let mut maps = Nested::<BTreeMap<u32, u32>>::with_capacity(budget, alternatives)?;
        {
            // Ordered (signature, record) buckets; full equality resolves collisions.
            let mut signatures_lease = budget.reserve(0)?;
            let mut signatures = BTreeSet::<(u64, u32)>::new();
            for alternative in 0..alternatives {
                let (font, label) = if let Some(v) = self.languages.get(alternative) {
                    (
                        &v.font,
                        ledger::string(budget, Label::Language(v.language))?,
                    )
                } else {
                    let i = alternative - self.languages.len();
                    (
                        &self.scripts[i].font,
                        ledger::string(budget, Label::Script(i))?,
                    )
                };
                let mut substitutions_lease = budget.reserve(0)?;
                let mut substitutions = BTreeMap::<u32, u32>::new();
                for entry in effective(font) {
                    let (cp, source) = entry?;
                    if !cmap.contains_key(&cp) {
                        limits.sfnt.glyphs(records.len().saturating_add(2))?;
                        sfnt::bounded(
                            cmap.len().saturating_add(1),
                            limits.sfnt.max_cmap_mappings,
                            "variant Unicode mappings",
                        )?;
                    }
                    let view = ledger::View::new(font, source);
                    let Some(&base) = cmap.get(&cp) else {
                        let index = index(records.len())?;
                        names.push(ledger::string(budget, GlyphName::base(cp))?)?;
                        records.push((source, font))?;
                        ledger::tree::<u32, u32>(&mut cmap_lease, budget, cmap.len() + 1)?;
                        cmap.insert(cp, index);
                        continue;
                    };
                    let planned = |i: u32| {
                        let (glyph, face) = records[i as usize];
                        ledger::View::new(face, glyph)
                    };
                    if view.matches(&planned(base)) {
                        continue;
                    }
                    let hash = view.signature();
                    let existing = signatures
                        .range((hash, 0)..=(hash, u32::MAX))
                        .map(|&(_, i)| i)
                        .find(|&i| view.matches(&planned(i)));
                    let replacement = if let Some(i) = existing {
                        i
                    } else {
                        limits.sfnt.glyphs(records.len().saturating_add(2))?;
                        // A glyph name's prefix before its first '.' is the unique
                        // base name of its codepoint, so only alternates already
                        // routed from this base can share a candidate name.
                        let mut suffix = 0usize;
                        let name = loop {
                            let name = ledger::string(
                                budget,
                                GlyphName {
                                    cp,
                                    label: Some((label.as_str(), suffix)),
                                },
                            )?;
                            if !maps
                                .iter()
                                .filter_map(|m| m.get(&base))
                                .any(|&i| names[i as usize] == *name)
                            {
                                break name;
                            }
                            suffix += 1;
                        };
                        let i = index(records.len())?;
                        ledger::tree::<(u64, u32), ()>(
                            &mut signatures_lease,
                            budget,
                            signatures.len() + 1,
                        )?;
                        names.push(name)?;
                        records.push((source, font))?;
                        signatures.insert((hash, i));
                        i
                    };
                    ledger::tree::<u32, u32>(
                        &mut substitutions_lease,
                        budget,
                        substitutions.len() + 1,
                    )?;
                    substitutions.insert(base, replacement);
                }
                maps.push(Owned::from_parts(substitutions, substitutions_lease))?;
                limits.sfnt.glyphs(records.len().saturating_add(1))?;
            }
        }
        // Tong sorts by base name, then full alternate name; names are unique,
        // so an in-place unstable sort yields the same order without scratch.
        let n = records.len();
        limits.sfnt.glyphs(n.saturating_add(1))?;
        let mut order = Vector::<u32>::with_capacity(budget, n)?;
        for i in 0..n {
            order.push(index(i)?)?;
        }
        order.sort_unstable_by(|&a, &b| names[a as usize].cmp(&names[b as usize]));
        let mut ids = Vector::<u16>::with_capacity(budget, n)?;
        ids.resize(n, 0)?;
        for (rank, &i) in order.iter().enumerate() {
            ids[i as usize] = (rank + 1) as u16;
        }
        let mut gsub_lease = budget.reserve(0)?;
        let mut gsub = Gsub::default();
        {
            let mut lookups = Nested::<BTreeMap<u16, u16>>::with_capacity(budget, maps.len())?;
            for map in maps.iter() {
                let lease = ledger::reserve_tree::<u16, u16>(budget, map.len())?;
                let pairs = map
                    .iter()
                    .map(|(&a, &b)| (ids[a as usize], ids[b as usize]))
                    .collect();
                lookups.push(Owned::from_parts(pairs, lease))?;
            }
            drop(maps);
            let (lookups, mut lease) = lookups.into_owned()?.into_parts();
            gsub.lookups = lookups;
            gsub_lease.merge(&mut lease)?;
        }
        {
            let mut systems_lease = budget.reserve(0)?;
            let mut systems = BTreeMap::<(Tag, Option<Tag>), Vector<u16>>::new();
            for (i, v) in self.scripts.iter().enumerate() {
                for &script in &v.scripts {
                    validate_tag(script)?;
                    let key = (script, None);
                    if !systems.contains_key(&key) {
                        ledger::tree::<(Tag, Option<Tag>), Vector<u16>>(
                            &mut systems_lease,
                            budget,
                            systems.len() + 1,
                        )?;
                        systems.insert(key, Vector::new(budget)?);
                    }
                    if let Some(values) = systems.get_mut(&key) {
                        values.push((self.languages.len() + i) as u16)?;
                    }
                }
            }
            for (i, v) in self.languages.iter().enumerate() {
                validate_tag(v.language)?;
                for &script in &v.scripts {
                    validate_tag(script)?;
                    let key = (script, Some(v.language));
                    if systems.contains_key(&key) {
                        return Err(invalid("duplicate script/language variant"));
                    }
                    let inherited = systems.get(&(script, None)).map_or(&[][..], |v| &v[..]);
                    let mut values = Vector::with_capacity(budget, 1 + inherited.len())?;
                    values.push(i as u16)?;
                    values.extend(inherited)?;
                    ledger::tree::<(Tag, Option<Tag>), Vector<u16>>(
                        &mut systems_lease,
                        budget,
                        systems.len() + 1,
                    )?;
                    systems.insert(key, values);
                }
            }
            // Feature lists retained by the GSUB model, keyed by their lookups.
            let mut features = Nested::<Feature>::with_capacity(budget, 0)?;
            let mut feature_ids_lease = budget.reserve(0)?;
            let mut feature_keys = budget.reserve(0)?;
            let mut feature_ids = BTreeMap::<Vec<u16>, u16>::new();
            let mut scripts_lease = budget.reserve(0)?;
            let mut systems_heap = budget.reserve(0)?;
            // Preserve explicit empty language systems: they prevent accidental fallback.
            for ((tag, language), routes) in systems {
                let retained = routes
                    .iter()
                    .filter(|&&id| !gsub.lookups[usize::from(id)].is_empty())
                    .count();
                let mut lookups = Vector::with_capacity(budget, retained)?;
                for &id in routes.iter() {
                    if !gsub.lookups[usize::from(id)].is_empty() {
                        lookups.push(id)?;
                    }
                }
                drop(routes);
                let selected = if lookups.is_empty() {
                    Vector::new(budget)?
                } else {
                    let id = if let Some(&id) = feature_ids.get(&lookups[..]) {
                        id
                    } else {
                        let id = sfnt::to16(features.len())?;
                        let mut key_lease = budget.reserve_array::<u16>(lookups.len())?;
                        ledger::tree::<Vec<u16>, u16>(
                            &mut feature_ids_lease,
                            budget,
                            feature_ids.len() + 1,
                        )?;
                        feature_keys.merge(&mut key_lease)?;
                        feature_ids.insert(lookups.to_vec(), id);
                        let (lookups, lease) = lookups.into_parts();
                        features.push(Owned::from_parts(
                            Feature {
                                tag: *b"locl",
                                lookups,
                            },
                            lease,
                        ))?;
                        id
                    };
                    let mut selected = Vector::with_capacity(budget, 1)?;
                    selected.push(id)?;
                    selected
                };
                let (features, mut lease) = selected.into_parts();
                if !gsub.scripts.contains_key(&tag) {
                    ledger::tree::<Tag, Script>(
                        &mut scripts_lease,
                        budget,
                        gsub.scripts.len() + 1,
                    )?;
                }
                let s = gsub.scripts.entry(tag).or_insert_with(|| Script {
                    default: Some(LanguageSystem::default()),
                    languages: Default::default(),
                });
                let value = LanguageSystem {
                    required: None,
                    features,
                };
                if let Some(lang) = language {
                    ledger::grow::<Tag, LanguageSystem>(
                        &mut systems_heap,
                        budget,
                        s.languages.len(),
                    )?;
                    s.languages.insert(lang, value);
                } else {
                    s.default = Some(value);
                }
                systems_heap.merge(&mut lease)?;
            }
            drop(feature_ids);
            drop(feature_keys);
            drop(feature_ids_lease);
            let (features, mut lease) = features.into_owned()?.into_parts();
            gsub.features = features;
            for l in [&mut lease, &mut scripts_lease, &mut systems_heap] {
                gsub_lease.merge(l)?;
            }
        }
        // Prune empty lookups/features through the identity subset, then release
        // the unpruned model and mapping by dropping them before the encode check.
        let subset = {
            let mapping_lease = ledger::reserve_tree::<u16, u16>(budget, n + 1)?;
            let mapping = (0..=n as u16).map(|i| (i, i)).collect();
            let subset = gsub.subset_with_budget(&mapping, budget)?;
            drop(mapping);
            drop(mapping_lease);
            subset
        };
        drop(gsub);
        drop(gsub_lease);
        let (gsub, mut lease) = subset.into_parts();
        drop(gsub.encode_with_budget((n + 1) as u16, limits, budget)?);
        let mut cmap_out_lease = ledger::reserve_tree::<u32, u16>(budget, cmap.len())?;
        let cmap_out = cmap
            .iter()
            .map(|(&cp, &i)| (cp, ids[i as usize]))
            .collect::<BTreeMap<_, _>>();
        drop(cmap);
        drop(cmap_lease);
        drop(ids);
        // Retain each planned glyph once, in final order, with an exact array.
        let mut glyphs = Nested::<(String, Glyph)>::with_capacity(budget, n)?;
        for &i in order.iter() {
            let (glyph, face) = records[i as usize];
            let name = ledger::string(budget, &names[i as usize])?;
            let glyph = ledger::View::new(face, glyph).retain(budget)?;
            glyphs.push(ledger::record(name, glyph)?)?;
        }
        drop(order);
        drop(records);
        drop(names);
        let (glyphs, mut glyphs_lease) = glyphs.into_owned()?.into_parts();
        lease.merge(&mut cmap_out_lease)?;
        lease.merge(&mut glyphs_lease)?;
        Ok(Owned::from_parts(
            Plan {
                glyphs,
                cmap: cmap_out,
                gsub,
            },
            lease,
        ))
    }
}
/// Reusable immutable plan borrowing its source font for global metadata.
#[derive(Debug)]
pub struct PreparedVariants<'a> {
    base: &'a Font,
    plan: Plan,
    limits: LayoutLimits,
    /// Admitted bytes of the retained plan, charged once per encode ledger.
    retained: usize,
}
impl PreparedVariants<'_> {
    /// Inspect the exact immutable glyph order and layout plan.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }
    /// Admitted bytes retained by the shared plan.
    pub fn retained_bytes(&self) -> usize {
        self.retained
    }
    /// Working bytes one [`encode`](Self::encode) call may admit besides the plan:
    /// `limits.sfnt.resources.max_total_bytes` minus [`retained_bytes`](Self::retained_bytes).
    ///
    /// Each call has its own ledger. Concurrent calls together may admit this
    /// allowance once per call in addition to the single shared plan; callers that
    /// need one process-wide total must divide it between their threads.
    pub fn encode_allowance(&self) -> usize {
        self.limits
            .sfnt
            .resources
            .max_total_bytes
            .saturating_sub(self.retained)
    }
    /// Encode with identical safety limits, without replanning or cloning glyphs.
    ///
    /// The call's ledger charges the retained plan once, then admits export
    /// storage against the remaining [`encode_allowance`](Self::encode_allowance).
    pub fn encode(&self, options: &Options) -> Result<Encoded> {
        if options.limits != self.limits {
            return Err(invalid(
                "prepared variants require matching layout limits; prepare again to change limits",
            ));
        }
        let budget = ledger::budget(self.limits.sfnt.resources.max_total_bytes);
        let _plan = budget.reserve(self.retained)?;
        export::encode_prepared(self.base, options, &self.plan, &budget)
    }
}
/// Planned encoded and unencoded glyphs; implicit glyph 0 is `.notdef`.
#[derive(Clone, Debug)]
pub struct Plan {
    /// Glyph IDs 1 onward, with deterministic PostScript names.
    pub glyphs: Vec<(String, Glyph)>,
    /// Default Unicode mapping.
    pub cmap: BTreeMap<u32, u16>,
    /// Script/language alternatives.
    pub gsub: Gsub,
}
fn invalid(s: &str) -> Error {
    Error::InvalidInput(format!("variants: {s}"))
}
fn effective(font: &Font) -> impl Iterator<Item = Result<(u32, &Glyph)>> {
    font.codepoints().map(|(cp, id)| {
        let Codepoint::Unicode(cp) = cp else {
            return Err(invalid("variants require Unicode input"));
        };
        if char::from_u32(cp).is_none() {
            return Err(invalid("non-scalar variant codepoint"));
        }
        Ok((
            cp,
            font.glyph(id)
                .ok_or_else(|| invalid("invalid glyph index"))?,
        ))
    })
}
fn index(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| invalid("planned glyph index overflow"))
}
/// A deterministic PostScript glyph name, formatted without temporary strings.
struct GlyphName<'a> {
    cp: u32,
    /// Alternate label and collision suffix (0 means no suffix).
    label: Option<(&'a str, usize)>,
}
impl GlyphName<'_> {
    fn base(cp: u32) -> Self {
        Self { cp, label: None }
    }
}
impl fmt::Display for GlyphName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.cp <= 0xffff {
            write!(f, "uni{:04X}", self.cp)?;
        } else {
            write!(f, "u{:05X}", self.cp)?;
        }
        match self.label {
            None => Ok(()),
            Some((label, 0)) => write!(f, ".{label}"),
            Some((label, suffix)) => write!(f, ".{label}{suffix}"),
        }
    }
}
/// Alternate-name label for a language or script face.
enum Label {
    Language(Tag),
    Script(usize),
}
impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Language(tag) => match tag {
                b"ZHT " | b"ZHH " => f.write_str("tc"),
                b"JAN " => f.write_str("jp"),
                b"KOR " => f.write_str("kr"),
                // Lossy UTF-8, trimmed and ASCII-lowercased; at most four chars.
                _ => {
                    let mut chars = ['\0'; 4];
                    let mut len = 0;
                    for chunk in tag.utf8_chunks() {
                        let replacement = (!chunk.invalid().is_empty()).then_some('\u{fffd}');
                        for c in chunk.valid().chars().chain(replacement) {
                            chars[len] = c;
                            len += 1;
                        }
                    }
                    let chars = &chars[..len];
                    let start = chars.iter().position(|c| !c.is_whitespace());
                    let end = chars.iter().rposition(|c| !c.is_whitespace());
                    if let (Some(start), Some(end)) = (start, end) {
                        for c in &chars[start..=end] {
                            write!(f, "{}", c.to_ascii_lowercase())?;
                        }
                    }
                    Ok(())
                }
            },
            Self::Script(0) => f.write_str("latin"),
            Self::Script(i) => write!(f, "script{i}"),
        }
    }
}
fn validate_tag(tag: Tag) -> Result<()> {
    if tag[0] == b' ' || tag.iter().any(|v| !(32..=126).contains(v)) {
        Err(invalid("invalid OpenType tag"))
    } else {
        Ok(())
    }
}
/// Parse an explicit four-letter script tag.
pub fn script_tag(value: &str) -> Result<Tag> {
    let t = value
        .as_bytes()
        .try_into()
        .map_err(|_| invalid("script must have four ASCII bytes"))?;
    validate_tag(t)?;
    Ok(t)
}
/// Convert OpenType or BCP-47 language tags using the OpenType language registry.
/// Chinese script/region distinctions are explicit; unknown tags are errors.
pub fn language_tag(value: &str) -> Result<Tag> {
    if (3..=4).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b == b' ' || b.is_ascii_digit())
    {
        let mut tag = [b' '; 4];
        tag[..value.len()].copy_from_slice(value.as_bytes());
        validate_tag(tag)?;
        return Ok(tag);
    }
    if value.is_empty()
        || value.len() > 255
        || value
            .split('-')
            .any(|s| s.is_empty() || s.len() > 8 || !s.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        return Err(invalid("invalid BCP-47 language tag"));
    }
    let tag = value.to_ascii_lowercase();
    let parts = tag.split('-').collect::<Vec<_>>();
    if parts[0] == "zh" || parts[0] == "zho" || parts[0] == "cmn" {
        if parts.contains(&"hans") {
            return Ok(*b"ZHS ");
        }
        if parts.contains(&"hk") {
            return Ok(*b"ZHH ");
        }
        if parts.contains(&"mo") {
            return Ok(*b"ZHTM");
        }
        if parts.contains(&"hant") || parts.contains(&"tw") {
            return Ok(*b"ZHT ");
        }
        return Ok(*b"ZHS ");
    }
    let i = sfnt::languages::LANGUAGES
        .binary_search_by_key(&parts[0], |(key, _)| *key)
        .map_err(|_| invalid("unmapped BCP-47 language; provide an OpenType tag"))?;
    Ok(sfnt::languages::LANGUAGES[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_names_format_without_temporary_strings() {
        // The generic branch matches the former lossy/trim/lowercase chain.
        for tag in [
            *b"ZHS ",
            *b"A.B ",
            *b"Ab C",
            [0xff, b'A', b' ', b' '],
            [b'x', 0xc3, 0xa9, b'Y'],
            [b' ', b'Q', 0x80, b' '],
        ] {
            assert_eq!(
                Label::Language(tag).to_string(),
                String::from_utf8_lossy(&tag).trim().to_ascii_lowercase(),
                "{tag:?}"
            );
        }
        for (tag, label) in [
            (*b"ZHT ", "tc"),
            (*b"ZHH ", "tc"),
            (*b"JAN ", "jp"),
            (*b"KOR ", "kr"),
        ] {
            assert_eq!(Label::Language(tag).to_string(), label);
        }
        assert_eq!(Label::Script(0).to_string(), "latin");
        assert_eq!(Label::Script(3).to_string(), "script3");
        for (cp, label, name) in [
            (0x41, None, "uni0041"),
            (0x1f600, None, "u1F600"),
            (0x10ffff, None, "u10FFFF"),
            (0x4e00, Some(("tc", 0)), "uni4E00.tc"),
            (0x20000, Some(("tc", 12)), "u20000.tc12"),
        ] {
            assert_eq!(GlyphName { cp, label }.to_string(), name);
        }
    }
}
