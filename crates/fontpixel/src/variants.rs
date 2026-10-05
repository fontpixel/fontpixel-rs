//! Regional bitmap alternatives, deterministic deduplication and OpenType output.
use crate::model::{Codepoint, GlyphBitmap};
use crate::sfnt::{
    self,
    gsub::{Feature, Gsub, LanguageSystem, LayoutLimits, Script, Tag},
};
use crate::working::{Budget, Lease, Nested, Owned, Vector};
use crate::{Error, Font, Glyph, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    mem::size_of,
};
mod dot_rows;
mod export;
mod ledger;
mod metadata;
mod notdef;
mod planner;
mod tables;
pub use export::{
    DotReuse, Encoded, Options, OutlineFlavor, encode, encode_single, encode_single_owned,
};
pub use metadata::{DotGlyph, GaspPolicy, GlyphOrder, MetadataOptions};
pub use notdef::NotdefPolicy;

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
            base: std::borrow::Cow::Borrowed(&self.base),
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
        let alternatives = self.check(limits, budget)?;
        let mut planner = planner::Planner::new(&self.base, alternatives, limits, budget)?;
        for v in &self.languages {
            planner.face(&v.font, Label::Language(v.language), limits, budget)?;
        }
        for (i, v) in self.scripts.iter().enumerate() {
            planner.face(&v.font, Label::Script(i), limits, budget)?;
        }
        planner.finish(&self.languages, &self.scripts, limits, budget)
    }
    /// Validate routes, duplicate script faces and every source face.
    fn check(&self, limits: &LayoutLimits, budget: &Budget) -> Result<usize> {
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
        Ok(alternatives)
    }
    /// Retained storage of this model as an owning caller charges it: the
    /// inline header, the language/script route arrays at capacity, their
    /// script-tag arrays, and each face's heap (`Font::retained_usage` without
    /// its inline header, which the header or a route array already counts).
    ///
    /// [`encode_owned`](Self::encode_owned) and [`into_prepared`](Self::into_prepared)
    /// admit exactly this amount and release each face's part when it drops.
    pub fn retained_usage(&self, max_bytes: usize) -> Result<usize> {
        let error = || Error::ResourceLimit {
            resource: "retained variants bytes",
            limit: max_bytes,
        };
        let mut used = size_of::<Self>();
        let mut add = |bytes: Option<usize>| -> Result<()> {
            used = bytes
                .and_then(|n| used.checked_add(n))
                .filter(|&n| n <= max_bytes)
                .ok_or_else(error)?;
            Ok(())
        };
        add(self
            .languages
            .capacity()
            .checked_mul(size_of::<LanguageVariant>()))?;
        add(self
            .scripts
            .capacity()
            .checked_mul(size_of::<ScriptVariant>()))?;
        for tags in self
            .languages
            .iter()
            .map(|v| &v.scripts)
            .chain(self.scripts.iter().map(|v| &v.scripts))
        {
            add(tags.capacity().checked_mul(size_of::<Tag>()))?;
        }
        for font in std::iter::once(&self.base)
            .chain(self.languages.iter().map(|v| &v.font))
            .chain(self.scripts.iter().map(|v| &v.font))
        {
            add(Some(face_heap(font, max_bytes)?))?;
        }
        Ok(used)
    }
    /// Plan while dropping each alternative face as soon as its pass ends, then
    /// keep only the base face's metrics and names for export.
    ///
    /// Admits [`retained_usage`](Self::retained_usage) first, then releases
    /// each face's exact share once that face has actually dropped. The base
    /// releases its share less the charge of the metadata kept. The returned
    /// lease still charges the metadata and the routing arrays until dropped.
    fn consume(
        mut self,
        limits: &LayoutLimits,
        budget: &Budget,
    ) -> Result<(Owned<Plan>, Font, Lease)> {
        let max = limits.sfnt.resources.max_total_bytes;
        let mut sources = budget.reserve(self.retained_usage(max)?)?;
        let alternatives = self.check(limits, budget)?;
        let plan = {
            let mut planner = planner::Planner::new(&self.base, alternatives, limits, budget)?;
            for i in 0..self.languages.len() {
                let font = std::mem::take(&mut self.languages[i].font);
                let charge = face_heap(&font, max)?;
                planner.face(
                    &font,
                    Label::Language(self.languages[i].language),
                    limits,
                    budget,
                )?;
                drop(font);
                drop(sources.split(charge)?);
            }
            for i in 0..self.scripts.len() {
                let font = std::mem::take(&mut self.scripts[i].font);
                let charge = face_heap(&font, max)?;
                planner.face(&font, Label::Script(i), limits, budget)?;
                drop(font);
                drop(sources.split(charge)?);
            }
            planner.finish(&self.languages, &self.scripts, limits, budget)?
        };
        let mut base = std::mem::take(&mut self.base);
        let charge = face_heap(&base, max)?;
        let mut metadata = Font::default();
        metadata.metrics = std::mem::take(&mut base.metrics);
        metadata.names = std::mem::take(&mut base.names);
        drop(base);
        let kept = face_heap(&metadata, max)?;
        let released = charge
            .checked_sub(kept)
            .ok_or_else(|| invalid("retained metadata exceeds its face charge"))?;
        drop(sources.split(released)?);
        Ok((plan, metadata, sources))
    }
    /// Plan and encode, dropping the source faces after planning.
    ///
    /// `o.limits.sfnt.resources.max_total_bytes` must include this model: its
    /// [`retained_usage`](Self::retained_usage) is admitted first on the same
    /// working ledger as planning and export. Each alternative face is dropped
    /// and its charge released after its planning pass; the base face is
    /// dropped once the plan has copied its glyphs, keeping only metrics and
    /// names. Output is identical to [`encode`].
    pub fn encode_owned(self, o: &Options) -> Result<Encoded> {
        export::encode_owned(self, o)
    }
    /// As [`prepare`](Self::prepare), but consuming the model: faces are
    /// dropped during planning as in [`encode_owned`](Self::encode_owned), and
    /// the prepared value owns the base metrics and names it needs.
    pub fn into_prepared(self, limits: &LayoutLimits) -> Result<PreparedVariants<'static>> {
        let budget = ledger::budget(limits.sfnt.resources.max_total_bytes);
        let (plan, metadata, sources) = self.consume(limits, &budget)?;
        let (plan, lease) = plan.into_parts();
        let kept = face_heap(&metadata, limits.sfnt.resources.max_total_bytes)?;
        let retained = lease
            .bytes()
            .checked_add(kept)
            .ok_or_else(|| budget.error())?;
        drop(lease);
        drop(sources);
        Ok(PreparedVariants {
            base: std::borrow::Cow::Owned(metadata),
            plan,
            limits: limits.clone(),
            retained,
        })
    }
}
/// Heap a face retains: `Font::retained_usage` without its inline header.
fn face_heap(font: &Font, max_bytes: usize) -> Result<usize> {
    font.retained_usage(max_bytes)?
        .bytes
        .checked_sub(size_of::<Font>())
        .ok_or_else(|| invalid("face usage below its inline header"))
}
/// Reusable immutable plan borrowing its source font for global metadata.
#[derive(Debug)]
pub struct PreparedVariants<'a> {
    base: std::borrow::Cow<'a, Font>,
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
        export::encode_prepared(&self.base, options, &self.plan, &budget)
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
