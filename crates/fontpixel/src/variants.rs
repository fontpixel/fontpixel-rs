//! Regional bitmap alternatives, deterministic deduplication and OpenType output.
use crate::model::{Codepoint, GlyphBitmap};
use crate::sfnt::{
    self,
    gsub::{Feature, Gsub, LanguageSystem, LayoutLimits, Script, Tag},
};
use crate::{Error, Font, Glyph, Result};
use std::{
    collections::{BTreeMap, HashMap},
    hash::{Hash, Hasher},
};
mod export;
pub use export::{Encoded, Options, OutlineFlavor, encode};

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
    pub fn plan(&self, limits: &LayoutLimits) -> Result<Plan> {
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
        let mut script_tags = std::collections::BTreeSet::new();
        for v in &self.scripts {
            for &tag in &v.scripts {
                if !script_tags.insert(tag) {
                    return Err(invalid("duplicate script variant"));
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
        let mut records = BTreeMap::<String, Glyph>::new();
        let mut cmap = BTreeMap::<u32, String>::new();
        for (cp, g) in effective(&self.base)? {
            limits.sfnt.glyphs(records.len().saturating_add(2))?;
            let name = gname(cp);
            cmap.insert(cp, name.clone());
            records.insert(name, planned_glyph(g));
        }
        let mut signatures = HashMap::<u64, Vec<String>>::new();
        let mut maps = Vec::new();
        for (font, label) in self
            .languages
            .iter()
            .map(|v| (&v.font, language_label(v.language)))
            .chain(self.scripts.iter().enumerate().map(|(i, v)| {
                (
                    &v.font,
                    if i == 0 {
                        "latin".into()
                    } else {
                        format!("script{i}")
                    },
                )
            }))
        {
            let mut substitutions = BTreeMap::new();
            for (cp, g) in effective(font)? {
                let name = if let Some(n) = cmap.get(&cp) {
                    n.clone()
                } else {
                    let n = gname(cp);
                    cmap.insert(cp, n.clone());
                    records.insert(n, planned_glyph(g));
                    continue;
                };
                let base = records
                    .get(&name)
                    .ok_or_else(|| invalid("missing planned glyph"))?;
                if equivalent(g, base) {
                    continue;
                }
                let hash = signature(g);
                let existing = signatures
                    .get(&hash)
                    .and_then(|names| {
                        names
                            .iter()
                            .find(|n| records.get(*n).is_some_and(|v| equivalent(v, g)))
                    })
                    .cloned();
                let replacement = if let Some(n) = existing {
                    n
                } else {
                    let mut n = format!("{}.{label}", gname(cp));
                    let mut suffix = 1;
                    while records.contains_key(&n) {
                        n = format!("{}.{label}{suffix}", gname(cp));
                        suffix += 1;
                    }
                    records.insert(n.clone(), planned_glyph(g));
                    signatures.entry(hash).or_default().push(n.clone());
                    n
                };
                substitutions.insert(name, replacement);
            }
            maps.push(substitutions);
            limits.sfnt.glyphs(records.len().saturating_add(1))?;
        }
        // Tong sorts by base name, then full alternate name; BTree string order agrees.
        let glyphs = records.into_iter().collect::<Vec<_>>();
        limits.sfnt.glyphs(glyphs.len().saturating_add(1))?;
        let ids = glyphs
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (name.clone(), (i + 1) as u16))
            .collect::<BTreeMap<_, _>>();
        let mut gsub = Gsub::default();
        for map in &maps {
            gsub.lookups
                .push(map.iter().map(|(a, b)| (ids[a], ids[b])).collect());
        }
        let mut feature_ids = BTreeMap::<Vec<u16>, u16>::new();
        let mut systems = BTreeMap::<(Tag, Option<Tag>), Vec<u16>>::new();
        for (i, v) in self.scripts.iter().enumerate() {
            for &script in &v.scripts {
                validate_tag(script)?;
                systems
                    .entry((script, None))
                    .or_default()
                    .push((self.languages.len() + i) as u16);
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
                let mut values = vec![i as u16];
                values.extend(systems.get(&(script, None)).into_iter().flatten());
                systems.insert(key, values);
            }
        }
        // Preserve explicit empty language systems: they prevent accidental fallback.
        for ((tag, language), ids) in systems {
            let ids = ids
                .into_iter()
                .filter(|&id| !gsub.lookups[usize::from(id)].is_empty())
                .collect::<Vec<_>>();
            let features = if ids.is_empty() {
                vec![]
            } else {
                let id = if let Some(&id) = feature_ids.get(&ids) {
                    id
                } else {
                    let id = sfnt::to16(gsub.features.len())?;
                    feature_ids.insert(ids.clone(), id);
                    gsub.features.push(Feature {
                        tag: *b"locl",
                        lookups: ids,
                    });
                    id
                };
                vec![id]
            };
            let s = gsub.scripts.entry(tag).or_insert_with(|| Script {
                default: Some(LanguageSystem::default()),
                languages: Default::default(),
            });
            let value = LanguageSystem {
                required: None,
                features,
            };
            if let Some(lang) = language {
                s.languages.insert(lang, value);
            } else {
                s.default = Some(value);
            }
        }
        let mapping = (0..=glyphs.len() as u16).map(|i| (i, i)).collect();
        let gsub = gsub.subset(&mapping);
        gsub.encode((glyphs.len() + 1) as u16, limits)?;
        Ok(Plan {
            glyphs,
            cmap: cmap.into_iter().map(|(cp, n)| (cp, ids[&n])).collect(),
            gsub,
        })
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
fn effective(font: &Font) -> Result<Vec<(u32, &Glyph)>> {
    font.codepoints()
        .map(|(cp, id)| {
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
        .collect()
}
fn planned_glyph(g: &Glyph) -> Glyph {
    Glyph {
        metrics: g.metrics.clone(),
        bitmap: g.bitmap.clone(),
        ..Default::default()
    }
}
fn equivalent(a: &Glyph, b: &Glyph) -> bool {
    a.bitmap == b.bitmap && a.metrics == b.metrics
}
fn signature(g: &Glyph) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(&g.bitmap).hash(&mut h);
    g.bitmap.width().hash(&mut h);
    g.bitmap.height().hash(&mut h);
    g.bitmap.as_bytes().hash(&mut h);
    // Hash only common metrics; full equality below resolves every collision and
    // also distinguishes all less-common metrics without copying source metadata.
    g.metrics.bounds.hash(&mut h);
    g.metrics.advance.hash(&mut h);
    g.metrics.vertical_advance.hash(&mut h);
    h.finish()
}
fn gname(cp: u32) -> String {
    if cp <= 0xffff {
        format!("uni{cp:04X}")
    } else {
        format!("u{cp:05X}")
    }
}
fn language_label(tag: Tag) -> String {
    match &tag {
        b"ZHT " | b"ZHH " => "tc".into(),
        b"JAN " => "jp".into(),
        b"KOR " => "kr".into(),
        _ => String::from_utf8_lossy(&tag).trim().to_ascii_lowercase(),
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
        if parts.contains(&"hk") || parts.contains(&"mo") {
            return Ok(*b"ZHH ");
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
