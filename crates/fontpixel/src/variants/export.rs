use super::tables;
use super::*;
use crate::{
    Bitmap, Report, ReportEntry,
    outline::{self, Shape},
    report::{Impact, Level},
    sfnt::{
        Flavor, bitmap as sbit, glyf,
        metrics::{self, Axis, Header, LongMetric},
        name::{NameRecord, NameTable},
        os2::Os2Table,
        post::PostTable,
    },
};
use std::{borrow::Cow, fmt::Write as _, mem::size_of};
/// Outline encoding inside the sfnt container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutlineFlavor {
    /// Quadratic outlines with shared-pixel composites for dots.
    #[default]
    TrueType,
    /// Cubic CFF1 outlines (requires `cff`).
    Cff,
}
/// Native default-scale dot composite representation; pixels and glyph IDs agree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DotReuse {
    /// One circle component per pixel, favoring repetitive webfont compression.
    Literal,
    /// Reuse existing one-row patterns, balancing sfnt size and compression ratio.
    Rows,
    /// Reuse bounded rows, columns and tiny 2D patterns for fastest native export.
    #[default]
    Patterns,
}
/// Native variant output choices, independent from the outer webfont container.
#[derive(Clone, Debug)]
pub struct Options {
    /// Outline shape.
    pub shape: Shape,
    /// Pixel diameter/side; merged squares require 1.
    pub dot_size: f64,
    /// Dot representation at 100 units/pixel and diameter 0.9; default Patterns.
    pub dot_reuse: DotReuse,
    /// Font design units per source pixel.
    pub units_per_pixel: u16,
    /// Override units per em; default pixel size times scale.
    pub units_per_em: Option<u16>,
    /// TrueType or CFF.
    pub flavor: OutlineFlavor,
    /// Include a matching monochrome EBDT/EBLC strike.
    pub embed_bitmaps: bool,
    /// Explicit Gray8 conversion; absence rejects gray outlines.
    pub gray_threshold: Option<u8>,
    /// Override full family name.
    pub family: Option<String>,
    /// Style name; default Square/SquareSeparate/Dot.
    pub style: Option<String>,
    /// Legacy subfamily (name ID 2); absent preserves `Regular`, independent of ID 17.
    /// Ignored when an exact `names` table is supplied.
    pub legacy_style: Option<String>,
    /// Exact full display name; absent retains family/style generation.
    /// Ignored when an exact `names` table is supplied.
    pub full_name: Option<String>,
    /// Exact PostScript name; absent retains existing generation.
    /// Ignored when an exact `names` table is supplied.
    pub postscript_name: Option<String>,
    /// Deterministic version text without the `Version ` prefix.
    pub version: String,
    /// Four-byte vendor identifier.
    pub vendor: [u8; 4],
    /// Optional copyright override.
    pub copyright: Option<String>,
    /// Optional license override.
    pub license: Option<String>,
    /// Optional license URL override.
    pub license_url: Option<String>,
    /// Replace generated names with an exact caller-supplied name table.
    pub names: Option<NameTable>,
    /// Glyph 0 policy; the default keeps the source/generated `.notdef`.
    pub notdef: super::NotdefPolicy,
    /// Explicit head/OS/2/post/maxp/gasp/cmap and glyph-order controls; the
    /// default reproduces the native tables.
    pub metadata: super::MetadataOptions,
    /// Layout, source and output limits.
    pub limits: LayoutLimits,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            shape: Shape::Square,
            dot_size: 1.,
            dot_reuse: DotReuse::default(),
            units_per_pixel: 100,
            units_per_em: None,
            flavor: OutlineFlavor::TrueType,
            embed_bitmaps: false,
            gray_threshold: None,
            family: None,
            style: None,
            legacy_style: None,
            full_name: None,
            postscript_name: None,
            version: "1.0".into(),
            vendor: *b"NONE",
            copyright: None,
            license: None,
            license_url: None,
            names: None,
            notdef: super::NotdefPolicy::Preserve,
            metadata: super::MetadataOptions::default(),
            limits: LayoutLimits::default(),
        }
    }
}
/// Complete standalone sfnt output and conversion diagnostics.
#[derive(Clone, Debug)]
pub struct Encoded {
    /// TTF or OTF bytes with native GSUB.
    pub bytes: Vec<u8>,
    /// Source information that could not be represented.
    pub report: Report,
}
fn validate_names(o: &Options) -> Result<()> {
    o.metadata.validate(o.flavor, o.shape)?;
    if o.names.is_some() {
        return Ok(());
    }
    for (value, resource, missing) in [
        (
            &o.legacy_style,
            "legacy style bytes",
            "legacy_style requires nonempty identity text",
        ),
        (
            &o.full_name,
            "full name bytes",
            "full_name requires nonempty identity text",
        ),
        (
            &o.postscript_name,
            "PostScript name bytes",
            "postscript_name requires nonempty identity text",
        ),
    ] {
        if let Some(value) = value {
            sfnt::bounded(
                value.len(),
                o.limits.sfnt.resources.max_table_bytes,
                resource,
            )?;
            if value.trim().is_empty() {
                return Err(invalid(missing));
            }
        }
    }
    Ok(())
}
fn i16n(v: i64) -> Result<i16> {
    i16::try_from(v).map_err(|_| invalid("metric exceeds signed 16-bit range"))
}
fn u16n(v: i64) -> Result<u16> {
    u16::try_from(v).map_err(|_| invalid("advance exceeds unsigned 16-bit range"))
}
/// Loss diagnostics whose entries and strings are admitted before allocation.
struct Notes {
    entries: Nested<ReportEntry>,
}
impl Notes {
    fn new(budget: &Budget) -> Result<Self> {
        Ok(Self {
            entries: Nested::with_capacity(budget, 0)?,
        })
    }
    fn has(&self, code: &str) -> bool {
        self.entries.iter().any(|e| e.code == code)
    }
    fn note(&mut self, budget: &Budget, code: &str, message: &str) -> Result<()> {
        self.push(budget, Level::Warning, Impact::Loss, code, message)
    }
    /// An informational audit entry (no loss).
    fn info(&mut self, budget: &Budget, code: &str, message: &str) -> Result<()> {
        self.push(budget, Level::Info, Impact::None, code, message)
    }
    fn push(
        &mut self,
        budget: &Budget,
        level: Level,
        impact: Impact,
        code: &str,
        message: &str,
    ) -> Result<()> {
        let (code, mut lease) = ledger::string(budget, code)?.into_parts();
        let (message, mut message_lease) = ledger::string(budget, message)?.into_parts();
        lease.merge(&mut message_lease)?;
        self.entries.push(Owned::from_parts(
            ReportEntry {
                level,
                impact,
                code,
                location: Default::default(),
                message,
            },
            lease,
        ))
    }
    fn into_report(self) -> Result<Owned<Report>> {
        let (entries, lease) = self.entries.into_owned()?.into_parts();
        Ok(Owned::from_parts(Report { entries }, lease))
    }
}
/// Borrow monochrome rows; admit an explicit Gray8 threshold copy first.
fn mono<'a>(
    g: &'a Glyph,
    threshold: Option<u8>,
    notes: &mut Notes,
    budget: &Budget,
) -> Result<Owned<Cow<'a, Bitmap>>> {
    match &g.bitmap {
        GlyphBitmap::Mono(b) => Ok(Owned::from_parts(Cow::Borrowed(b), budget.reserve(0)?)),
        GlyphBitmap::Gray8(b) => {
            let t =
                threshold.ok_or_else(|| invalid("gray outline requires an explicit threshold"))?;
            if !notes.has("variants.gray_threshold") {
                notes.note(
                    budget,
                    "variants.gray_threshold",
                    "Gray coverage was explicitly thresholded for monochrome outlines",
                )?;
            }
            let rows = (b.width() as usize)
                .div_ceil(8)
                .checked_mul(b.height() as usize)
                .ok_or_else(|| budget.error())?;
            let lease = budget.reserve(rows)?;
            Ok(Owned::from_parts(Cow::Owned(b.to_mono(t)?), lease))
        }
    }
}
fn notdef(budget: &Budget) -> Result<Owned<Glyph>> {
    // One packed byte per row of the 5x10 box.
    let lease = budget.reserve(10)?;
    let mut b: Bitmap = Bitmap::new(5, 10)?;
    for y in 0..10 {
        for x in 0..5 {
            if x == 0 || x == 4 || y == 0 || y == 9 {
                b.set_pixel(x, y, 1)?;
            }
        }
    }
    let mut g = Glyph::new(b);
    g.metrics.bounds.x = 1;
    g.metrics.advance = Some(crate::model::Vector::new(7, 0));
    Ok(Owned::from_parts(g, lease))
}
/// Generated PostScript name: family's ASCII alphanumerics and '-', then style.
struct PostScriptName<'a> {
    family: &'a str,
    style: &'a str,
}
impl std::fmt::Display for PostScriptName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self
            .family
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        {
            f.write_char(c)?;
        }
        write!(f, "-{}", self.style)
    }
}
/// `glyph<index>` followed by `extra` underscores.
struct FallbackName {
    index: usize,
    extra: usize,
}
impl std::fmt::Display for FallbackName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "glyph{}", self.index)?;
        for _ in 0..self.extra {
            f.write_char('_')?;
        }
        Ok(())
    }
}
/// Admitted Windows Unicode (UTF-16BE) or Macintosh Roman ASCII name records.
fn name_records(
    budget: &Budget,
    records: &mut Nested<NameRecord>,
    id: u16,
    value: &str,
) -> Result<()> {
    if value.is_ascii() {
        let mut bytes = Vector::with_capacity(budget, value.len())?;
        bytes.extend(value.as_bytes())?;
        let (value, lease) = bytes.into_parts();
        records.push(Owned::from_parts(
            NameRecord {
                platform_id: 1,
                encoding_id: 0,
                language_id: 0,
                name_id: id,
                value,
            },
            lease,
        ))?;
    }
    let units = value.encode_utf16().count();
    let mut bytes =
        Vector::with_capacity(budget, units.checked_mul(2).ok_or_else(|| budget.error())?)?;
    for unit in value.encode_utf16() {
        bytes.extend(&unit.to_be_bytes())?;
    }
    let (value, lease) = bytes.into_parts();
    // Same record as `NameRecord::unicode(id, 0x409, value)`.
    records.push(Owned::from_parts(
        NameRecord {
            platform_id: 3,
            encoding_id: 1,
            language_id: 0x409,
            name_id: id,
            value,
        },
        lease,
    ))
}
/// Whether `record` holds exactly `value`, compared without allocating:
/// UTF-16BE for Unicode and Windows Unicode encodings, ASCII for Mac Roman.
fn record_holds(record: &NameRecord, value: &str) -> bool {
    match (record.platform_id, record.encoding_id) {
        (0, _) | (3, 0 | 1 | 10) => record
            .value
            .iter()
            .copied()
            .eq(value.encode_utf16().flat_map(u16::to_be_bytes)),
        (1, 0) => value.is_ascii() && record.value == value.as_bytes(),
        _ => false,
    }
}
/// Info audit for explicit table controls, and Loss for each source name the
/// default table would carry (family without `family`, copyright/license/URL
/// without their overrides) that an exact name table omits or replaces.
fn audit_metadata(base: &Font, o: &Options, notes: &mut Notes, budget: &Budget) -> Result<()> {
    if o.metadata.requested() {
        notes.info(
            budget,
            "variants.metadata_configuration",
            "Explicit outline table controls applied; source model unchanged",
        )?;
    }
    let Some(table) = &o.names else {
        return Ok(());
    };
    for (id, source, overridden) in [
        (0u16, &base.names.copyright, o.copyright.is_some()),
        (1, &base.names.family, o.family.is_some()),
        (13, &base.names.license, o.license.is_some()),
        (14, &base.names.license_url, o.license_url.is_some()),
    ] {
        let Some(value) = source.as_deref().filter(|v| !v.is_empty() && !overridden) else {
            continue;
        };
        if !table
            .records
            .iter()
            .any(|r| r.name_id == id && record_holds(r, value))
        {
            notes.note(
                budget,
                "variants.metadata_name",
                "The exact name table omits or replaces a source name field; source model unchanged",
            )?;
        }
    }
    Ok(())
}
/// Admitted deep copy of a caller-supplied name table.
fn clone_names(budget: &Budget, names: &NameTable) -> Result<Owned<NameTable>> {
    let mut bytes = names
        .records
        .len()
        .checked_mul(size_of::<NameRecord>())
        .and_then(|n| n.checked_add(names.language_tags.len().checked_mul(size_of::<String>())?))
        .ok_or_else(|| budget.error())?;
    for value in names
        .records
        .iter()
        .map(|r| r.value.len())
        .chain(names.language_tags.iter().map(String::len))
    {
        bytes = bytes.checked_add(value).ok_or_else(|| budget.error())?;
    }
    let lease = budget.reserve(bytes)?;
    Ok(Owned::from_parts(names.clone(), lease))
}
/// Vectorize planned variants using the native, independently checked table codecs.
///
/// Planning and export share one working ledger of
/// `o.limits.sfnt.resources.max_total_bytes`; the plan stays charged while it
/// is exported.
pub fn encode(variants: &Variants, o: &Options) -> Result<Encoded> {
    validate_names(o)?;
    if !(o.dot_size > 0. && o.dot_size <= 1.5) || (o.shape == Shape::Square && o.dot_size != 1.) {
        return Err(invalid("invalid dot size for the chosen outline shape"));
    }
    let budget = ledger::budget(o.limits.sfnt.resources.max_total_bytes);
    let plan = variants.plan_with_budget(&o.limits, &budget)?;
    let encoded = encode_planned(&variants.base, o, &plan, None, &budget)?;
    drop(plan);
    Ok(encoded.into_parts().0)
}
/// [`Variants::encode_owned`]: plan while dropping source faces, then export
/// with only the plan and the base metadata live.
pub(super) fn encode_owned(variants: Variants, o: &Options) -> Result<Encoded> {
    validate_names(o)?;
    if !(o.dot_size > 0. && o.dot_size <= 1.5) || (o.shape == Shape::Square && o.dot_size != 1.) {
        return Err(invalid("invalid dot size for the chosen outline shape"));
    }
    let budget = ledger::budget(o.limits.sfnt.resources.max_total_bytes);
    let (plan, metadata, sources) = variants.consume(&o.limits, &budget)?;
    let encoded = encode_planned(&metadata, o, &plan, None, &budget)?;
    drop(plan);
    drop(metadata);
    drop(sources);
    Ok(encoded.into_parts().0)
}
pub(super) fn encode_prepared(
    base: &Font,
    o: &Options,
    plan: &Plan,
    budget: &Budget,
) -> Result<Encoded> {
    Ok(encode_planned(base, o, plan, None, budget)?.into_parts().0)
}
fn encode_planned(
    base: &Font,
    o: &Options,
    plan: &Plan,
    source_notdef: Option<Owned<Glyph>>,
    budget: &Budget,
) -> Result<Owned<Encoded>> {
    if !(o.dot_size > 0. && o.dot_size <= 1.5) || (o.shape == Shape::Square && o.dot_size != 1.) {
        return Err(invalid("invalid dot size for the chosen outline shape"));
    }
    let l = &o.limits.sfnt;
    validate_names(o)?;
    let scale = i64::from(o.units_per_pixel);
    if scale == 0 {
        return Err(invalid("units_per_pixel must be positive"));
    }
    let upm = o.units_per_em.unwrap_or_else(|| {
        u16::try_from(
            base.metrics
                .pixel_size
                .unwrap_or(14)
                .saturating_mul(u32::from(o.units_per_pixel)),
        )
        .unwrap_or(0)
    });
    if !(16..=16384).contains(&upm) {
        return Err(invalid("units_per_em outside 16..=16384"));
    }
    let ascent = i16n(
        base.metrics
            .ascent
            .unwrap_or(i64::from(base.metrics.pixel_size.unwrap_or(14)))
            .checked_mul(scale)
            .ok_or_else(|| invalid("ascent overflow"))?,
    )?;
    let descent = i16n(
        base.metrics
            .descent
            .unwrap_or(0)
            .checked_mul(-scale)
            .ok_or_else(|| invalid("descent overflow"))?,
    )?;
    let family = o
        .family
        .as_deref()
        .or(base.names.family.as_deref())
        .unwrap_or("Fontpixel");
    let style = o.style.as_deref().unwrap_or(match o.shape {
        Shape::Square => "Square",
        Shape::SquareSeparate => "SquareSeparate",
        Shape::Dot => "Dot",
    });
    let fullname = match o.full_name.as_ref().filter(|_| o.names.is_none()) {
        Some(value) => ledger::string(budget, value)?,
        None => ledger::string(budget, format_args!("{family} {style}"))?,
    };
    let psname = match o.postscript_name.as_ref().filter(|_| o.names.is_none()) {
        Some(value) => ledger::string(budget, value)?,
        None => ledger::string(budget, PostScriptName { family, style })?,
    };
    let mut notes = Notes::new(budget)?;
    let retain_names = source_notdef.is_some();
    let empty_notdef = o.notdef.empty_metrics();
    let notdef = match (empty_notdef, source_notdef) {
        (Some((advance, _)), _) => {
            // The empty glyph's only heap storage is its ".notdef" name.
            let lease = budget.reserve(".notdef".len())?;
            Owned::from_parts(
                super::notdef::empty_glyph(advance, o.units_per_pixel, o.embed_bitmaps)?,
                lease,
            )
        }
        (None, Some(g)) => g,
        (None, None) => notdef(budget)?,
    };
    audit_metadata(base, o, &mut notes, budget)?;
    let mut sources = Vector::with_capacity(budget, plan.glyphs.len().saturating_add(1))?;
    sources.push((".notdef", &*notdef))?;
    for (n, g) in &plan.glyphs {
        sources.push((n.as_str(), g))?;
    }
    let reorder = o.metadata.glyph_order == super::GlyphOrder::Codepoint;
    let after_notdef = o.metadata.dot_glyph == super::DotGlyph::AfterNotdef;
    if (reorder || after_notdef) && !plan.gsub.features.is_empty() {
        return Err(invalid(
            "glyph order and dot position controls do not support regional alternates",
        ));
    }
    // positions[p]: index in `sources` of planned glyph ID p (only when reordered).
    let mut positions =
        Vector::<u16>::with_capacity(budget, if reorder { sources.len() } else { 0 })?;
    if reorder {
        let mut keys = Vector::<(u32, u16)>::with_capacity(budget, plan.glyphs.len())?;
        for id in 1..sources.len() {
            keys.push((u32::MAX, id as u16))?;
        }
        for (&cp, &gid) in &plan.cmap {
            if let Some(key) = keys.get_mut(usize::from(gid).wrapping_sub(1)) {
                key.0 = key.0.min(cp);
            }
        }
        keys.sort_unstable();
        positions.resize(sources.len(), 0)?;
        let mut ordered = Vector::with_capacity(budget, sources.len())?;
        ordered.push(sources[0])?;
        for (rank, &(_, id)) in keys.iter().enumerate() {
            positions[usize::from(id)] = (rank + 1) as u16;
            ordered.push(sources[usize::from(id)])?;
        }
        drop(keys);
        sources = ordered;
    }
    // Final glyph ID of `sources[i]`: the dot moves to 1 and shifts the rest.
    let final_id = |i: usize| -> u16 {
        if after_notdef && i >= 1 {
            i as u16 + 1
        } else {
            i as u16
        }
    };
    let shared = o.flavor == OutlineFlavor::TrueType && o.shape == Shape::Dot;
    let count = sources.len() + usize::from(shared);
    l.glyphs(count)?;
    // Existing small ink-pattern glyphs shorten default Tong dot composites without
    // introducing any new glyph IDs. Other scales/diameters retain direct dots.
    let dot_rows = if shared
        && o.units_per_pixel == 100
        && o.dot_size == 0.9
        && o.dot_reuse != DotReuse::Literal
    {
        Some(super::dot_rows::Catalogue::new(
            &sources,
            scale,
            o.dot_reuse == DotReuse::Patterns,
            budget,
        )?)
    } else {
        None
    };
    let dot_id = sources.len() as u16;
    let vertical = base.metrics.glyph_defaults.vertical_advance.is_some()
        || sources.iter().any(|(_, g)| {
            g.metrics.vertical_advance.is_some() || g.metrics.vertical_origin.is_some()
        });
    let vertical_count = if vertical { count } else { 0 };
    let mut vm = Vector::with_capacity(budget, vertical_count)?;
    let mut vertical_extents = Vector::with_capacity(budget, vertical_count)?;
    let mut hm = Vector::with_capacity(budget, count)?;
    let mut extents = Vector::with_capacity(budget, count)?;
    let gl = glyf::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    let mut total_points = 0usize;
    let mut total_components = 0usize;
    let mut total_dot_pixels = 0usize;
    #[cfg(feature = "cff")]
    let mut total_commands = 0usize;
    let truetype = o.flavor == OutlineFlavor::TrueType;
    let mut tt = Nested::<glyf::Glyph>::with_capacity(budget, if truetype { count } else { 0 })?;
    // CFF programs are encoded as each glyph is built, so only their bytes
    // (not every glyph's path commands) stay live until the table is laid out.
    #[cfg(feature = "cff")]
    let cff_limits = sfnt::cff::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    #[cfg(feature = "cff")]
    let cff_full_name: &str = if fullname.is_ascii() || (o.names.is_none() && o.full_name.is_some())
    {
        &fullname
    } else {
        &psname
    };
    #[cfg(feature = "cff")]
    let mut cff = if truetype {
        None
    } else {
        Some(sfnt::cff::Encoder::new(
            &psname,
            cff_full_name,
            upm,
            sfnt::cff::DotOptions {
                cell_size: scale as f64,
                diameter: o.dot_size * scale as f64,
            },
            &cff_limits,
            budget,
        )?)
    };
    let mut bitmaps_tree = budget.reserve(0)?;
    let mut bitmaps_heap = budget.reserve(0)?;
    let mut bitmaps = BTreeMap::new();
    for (i, &(name, g)) in sources.iter().enumerate() {
        let bitmap = mono(g, o.gray_threshold, &mut notes, budget)?;
        let x = i32::try_from(g.metrics.bounds.x).map_err(|_| invalid("glyph x exceeds int32"))?;
        let y = i32::try_from(g.metrics.bounds.y).map_err(|_| invalid("glyph y exceeds int32"))?;
        let adv = g
            .metrics
            .advance
            .unwrap_or(crate::model::Vector::new(i64::from(bitmap.width()), 0));
        if adv.y != 0 {
            return Err(invalid(
                "sfnt cannot represent nonzero horizontal advance y",
            ));
        }
        let advance = u16n(
            adv.x
                .checked_mul(scale)
                .ok_or_else(|| invalid("advance overflow"))?,
        )?;
        let generated_notdef = i == 0 && !retain_names;
        let shape = if generated_notdef {
            Shape::Square
        } else {
            o.shape
        };
        let opts = outline::Options {
            shape,
            units_per_pixel: i32::from(o.units_per_pixel),
            dot_size: if generated_notdef { 1. } else { o.dot_size },
            origin: (x, y),
        };
        let ink = bitmap.bbox();
        let inset = if shape == Shape::Square {
            0.
        } else {
            (1. - opts.dot_size) * f64::from(o.units_per_pixel) / 2.
        };
        let bearing = if generated_notdef {
            i16n(scale)?
        } else if ink.width == 0 {
            0
        } else {
            i16n(((g.metrics.bounds.x + ink.x) as f64 * scale as f64 + inset).round() as i64)?
        };
        let width = if ink.width == 0 {
            None
        } else {
            Some(u16n(
                (f64::from(ink.width) * scale as f64 - 2. * inset).round() as i64,
            )?)
        };
        let mut bitmap_vertical = None;
        if vertical {
            let va = g
                .metrics
                .vertical_advance
                .unwrap_or(crate::model::Vector::new(
                    0,
                    -i64::from(base.metrics.pixel_size.unwrap_or(14)),
                ));
            if va.x != 0 {
                return Err(invalid("vertical advance x cannot be represented by vmtx"));
            }
            let vadvance = u16n(
                va.y.checked_mul(-scale)
                    .ok_or_else(|| invalid("vertical advance overflow"))?,
            )?;
            let origin = if generated_notdef {
                None
            } else {
                g.metrics.vertical_origin
            };
            if origin.is_some_and(|p| p.x.checked_mul(2) != Some(adv.x)) {
                return Err(invalid(
                    "non-centered vertical origin x cannot be represented by vmtx",
                ));
            }
            let origin_y = if let Some(p) = origin {
                p.y.checked_mul(scale)
                    .ok_or_else(|| invalid("vertical origin overflow"))?
            } else {
                i64::from(ascent)
            };
            let top = if ink.height == 0 && retain_names {
                0
            } else {
                (g.metrics.bounds.y + ink.y + i64::from(ink.height))
                    .checked_mul(scale)
                    .ok_or_else(|| invalid("vertical top overflow"))?
            };
            let bearing = if ink.height == 0 && !retain_names {
                0
            } else {
                i16n(
                    origin_y
                        .checked_sub(top)
                        .ok_or_else(|| invalid("vertical bearing overflow"))?
                        .checked_add(if ink.height == 0 {
                            0
                        } else {
                            inset.round() as i64
                        })
                        .ok_or_else(|| invalid("vertical bearing overflow"))?,
                )?
            };
            vm.push(LongMetric {
                advance: vadvance,
                bearing,
            })?;
            vertical_extents.push(if ink.height == 0 {
                None
            } else {
                Some(u16n(
                    (f64::from(ink.height) * scale as f64 - 2. * inset).round() as i64,
                )?)
            })?;
            if o.embed_bitmaps {
                let pixel_origin_y = origin.map_or(i64::from(ascent) / scale, |p| p.y);
                let pixel_origin_x = origin.map_or(adv.x / 2, |p| p.x);
                let narrow = |v| {
                    i8::try_from(v).map_err(|_| invalid("embedded vertical bearing exceeds int8"))
                };
                bitmap_vertical = Some(sbit::Bearing {
                    x: narrow(
                        g.metrics
                            .bounds
                            .x
                            .checked_sub(pixel_origin_x)
                            .ok_or_else(|| invalid("embedded vertical bearing overflow"))?,
                    )?,
                    y: narrow(
                        g.metrics
                            .bounds
                            .y
                            .checked_add(i64::from(bitmap.height()))
                            .and_then(|top| top.checked_sub(pixel_origin_y))
                            .ok_or_else(|| invalid("embedded vertical bearing overflow"))?,
                    )?,
                    advance: u8::try_from(
                        va.y.checked_neg()
                            .ok_or_else(|| invalid("embedded vertical advance overflow"))?,
                    )
                    .map_err(|_| invalid("embedded vertical advance exceeds uint8"))?,
                });
            }
        }
        let (advance, bearing) = match empty_notdef {
            Some(exact) if i == 0 => exact,
            _ => (advance, bearing),
        };
        hm.push(LongMetric { advance, bearing })?;
        extents.push(width)?;
        match o.flavor {
            OutlineFlavor::TrueType => {
                if shared && !generated_notdef {
                    // Sharing must not bypass the former literal-dot work budget.
                    total_dot_pixels = total_dot_pixels.saturating_add(
                        bitmap
                            .as_bytes()
                            .iter()
                            .map(|byte| byte.count_ones() as usize)
                            .sum::<usize>(),
                    );
                    sfnt::bounded(
                        total_dot_pixels,
                        gl.max_total_components,
                        "variant outline components",
                    )?;
                }
                let glyph = if shared && !generated_notdef {
                    let mut glyph = if let Some(rows) = &dot_rows {
                        rows.glyph(i, &bitmap, &opts, dot_id, &gl, budget)?
                    } else {
                        glyf::Glyph::pixels_with_budget(&bitmap, &opts, dot_id, &gl, budget)?
                    };
                    if after_notdef && let glyf::Glyph::Composite(c) = &mut *glyph {
                        // Components name the dot and source indices; move both
                        // to their final IDs (dot first, the rest up by one).
                        for k in &mut c.components {
                            k.glyph_id = if k.glyph_id == dot_id {
                                1
                            } else {
                                final_id(usize::from(k.glyph_id))
                            };
                        }
                    }
                    glyph
                } else {
                    let (contours, lease) =
                        outline::contours_with_budget(&bitmap, &opts, &gl.geometry, budget)?
                            .into_parts();
                    Owned::from_parts(glyf::Glyph::simple(contours), lease)
                };
                match &*glyph {
                    glyf::Glyph::Simple(g) => {
                        total_points = total_points
                            .saturating_add(g.contours.iter().map(Vec::len).sum::<usize>())
                    }
                    glyf::Glyph::Composite(g) => {
                        total_components = total_components.saturating_add(g.components.len())
                    }
                    _ => (),
                }
                sfnt::bounded(total_points, gl.max_total_points, "variant outline points")?;
                sfnt::bounded(
                    total_components,
                    gl.max_total_components,
                    "variant outline components",
                )?;
                tt.push(glyph)?;
            }
            OutlineFlavor::Cff => {
                #[cfg(feature = "cff")]
                {
                    let cl = sfnt::cff::Limits {
                        sfnt: l.clone(),
                        ..Default::default()
                    };
                    let glyph = sfnt::cff::Glyph::from_bitmap_with_budget(
                        ledger::string(budget, name)?,
                        f64::from(advance),
                        &bitmap,
                        &opts,
                        &cl,
                        budget,
                    )?;
                    let commands = match &glyph.outline {
                        sfnt::cff::Outline::Path(p) => p.len(),
                        sfnt::cff::Outline::Dots(p) => p.len().saturating_mul(6),
                    };
                    total_commands = total_commands.saturating_add(commands);
                    sfnt::bounded(
                        total_commands,
                        cl.max_total_commands,
                        "variant CFF commands",
                    )?;
                    if let Some(cff) = cff.as_mut() {
                        cff.glyph(&glyph)?;
                    }
                    // The path and its charge drop here; only the program stays.
                    drop(glyph);
                }
                #[cfg(not(feature = "cff"))]
                {
                    let _ = name;
                    return Err(Error::FeatureDisabled {
                        format: "cff".into(),
                    });
                }
            }
        }
        if o.embed_bitmaps {
            let narrow = |v: i64| {
                i8::try_from(v).map_err(|_| invalid("embedded bitmap bearing exceeds int8"))
            };
            let metrics = sbit::Metrics {
                width: u8::try_from(bitmap.width())
                    .map_err(|_| invalid("embedded bitmap width exceeds uint8"))?,
                height: u8::try_from(bitmap.height())
                    .map_err(|_| invalid("embedded bitmap height exceeds uint8"))?,
                horizontal: Some(sbit::Bearing {
                    x: narrow(g.metrics.bounds.x)?,
                    y: narrow(g.metrics.bounds.y + i64::from(bitmap.height()))?,
                    advance: u8::try_from(adv.x)
                        .map_err(|_| invalid("embedded bitmap advance exceeds uint8"))?,
                }),
                vertical: bitmap_vertical,
            };
            let (rows, mut lease) = bitmap.into_parts();
            let rows = match rows {
                Cow::Owned(rows) => rows,
                Cow::Borrowed(rows) => {
                    lease.grow_to(rows.as_bytes().len())?;
                    rows.clone()
                }
            };
            ledger::tree::<u16, sbit::BitmapGlyph>(&mut bitmaps_tree, budget, bitmaps.len() + 1)?;
            bitmaps_heap.merge(&mut lease)?;
            bitmaps.insert(
                final_id(i),
                sbit::BitmapGlyph {
                    metrics,
                    bitmap: GlyphBitmap::Mono(rows),
                },
            );
        }
    }
    drop(dot_rows);
    if shared {
        let (contour, mut lease) = outline::pixel_contour_with_budget(
            &outline::Options {
                shape: Shape::Dot,
                units_per_pixel: i32::from(o.units_per_pixel),
                dot_size: o.dot_size,
                origin: (0, 0),
            },
            budget,
        )?
        .into_parts();
        let mut contours = Vector::with_capacity(budget, 1)?;
        contours.push(contour)?;
        let (contours, mut contours_lease) = contours.into_parts();
        lease.merge(&mut contours_lease)?;
        tt.push(Owned::from_parts(glyf::Glyph::simple(contours), lease))?;
        hm.push(LongMetric {
            advance: o.units_per_pixel,
            bearing: i16n(((1. - o.dot_size) * scale as f64 / 2.).round() as i64)?,
        })?;
        extents.push(Some(u16n((o.dot_size * scale as f64).round() as i64)?))?;
    }
    if vertical && shared {
        vm.push(LongMetric {
            advance: upm,
            bearing: i16n(i64::from(ascent) - (o.dot_size * scale as f64).round() as i64)?,
        })?;
        vertical_extents.push(Some(u16n((o.dot_size * scale as f64).round() as i64)?))?;
    }
    if after_notdef {
        // The dot was appended last; move it to glyph 1 in place.
        tt[1..].rotate_right(1);
        hm[1..].rotate_right(1);
        extents[1..].rotate_right(1);
        if vertical {
            vm[1..].rotate_right(1);
            vertical_extents[1..].rotate_right(1);
        }
    }
    let mut b = tables::Builder::new(
        if truetype {
            Flavor::TrueType
        } else {
            Flavor::Cff
        },
        budget,
    )?;
    let (bounds, loca_format) = match o.flavor {
        OutlineFlavor::TrueType => {
            let (glyphs, glyphs_lease) = tt.into_owned()?.into_parts();
            let table = glyf::GlyfTable { glyphs };
            let (e, lease) = tables::glyf(&table, &gl, budget)?.into_parts();
            // Outline models are dropped once their encoded bytes are charged.
            drop(table);
            drop(glyphs_lease);
            b.charge(lease)?;
            b.insert_charged(*b"glyf", e.glyf)?;
            b.insert_charged(*b"loca", e.loca)?;
            b.insert(
                *b"maxp",
                tables::small(budget, || {
                    let mut maxp = e.statistics.unhinted_maxp(count as u16)?;
                    if let Some(zones) = o.metadata.max_zones {
                        // maxZones follows version, numGlyphs and four maxima.
                        maxp[14..16].copy_from_slice(&zones.to_be_bytes());
                    }
                    Ok(maxp)
                })?,
            )?;
            (e.bounds, e.index_to_loc_format)
        }
        OutlineFlavor::Cff => {
            #[cfg(feature = "cff")]
            {
                let encoder = cff.take().ok_or_else(|| invalid("CFF encoder missing"))?;
                // Every encoder buffer but the table and maxp has dropped here.
                let (e, table_lease) = encoder.finish()?.into_parts();
                b.charge(table_lease)?;
                b.insert_charged(*b"CFF ", e.data)?;
                b.insert_charged(*b"maxp", e.maxp)?;
                (e.bounds, 0)
            }
            #[cfg(not(feature = "cff"))]
            return Err(Error::FeatureDisabled {
                format: "cff".into(),
            });
        }
    };
    let mut head = Vector::with_capacity(budget, 54)?;
    head.resize(54, 0u8)?;
    head[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    head[4..8].copy_from_slice(&0x10000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
    head[16..18].copy_from_slice(&3u16.to_be_bytes());
    head[18..20].copy_from_slice(&upm.to_be_bytes());
    for (i, n) in bounds.iter().enumerate() {
        head[36 + 2 * i..38 + 2 * i].copy_from_slice(&n.to_be_bytes());
    }
    head[46..48].copy_from_slice(&3u16.to_be_bytes());
    head[48..50].copy_from_slice(&2i16.to_be_bytes());
    head[50..52].copy_from_slice(&loca_format.to_be_bytes());
    if let Some([created, modified]) = o.metadata.timestamps {
        head[20..28].copy_from_slice(&created.to_be_bytes());
        head[28..36].copy_from_slice(&modified.to_be_bytes());
    }
    b.insert(*b"head", head.into_owned())?;
    let mut h = Header::new(Axis::Horizontal);
    h.ascender = ascent;
    h.descender = descent;
    h.line_gap = i16n(
        base.metrics
            .line_gap
            .unwrap_or(0)
            .checked_mul(scale)
            .ok_or_else(|| invalid("line gap overflow"))?,
    )?;
    h.recompute(&hm, &extents)?;
    let (he, lease) =
        metrics::encode_with_budget(Axis::Horizontal, &h, &hm, true, l, budget)?.into_parts();
    b.charge(lease)?;
    b.insert_charged(*b"hhea", he.header)?;
    b.insert_charged(*b"hmtx", he.metrics)?;
    if vertical {
        let mut vh = Header::new(Axis::Vertical);
        vh.ascender = (upm / 2) as i16;
        vh.descender = -((upm - upm / 2) as i16);
        vh.recompute(&vm, &vertical_extents)?;
        let (e, lease) =
            metrics::encode_with_budget(Axis::Vertical, &vh, &vm, true, l, budget)?.into_parts();
        b.charge(lease)?;
        b.insert_charged(*b"vhea", e.header)?;
        b.insert_charged(*b"vmtx", e.metrics)?;
    }
    let mut os = Os2Table {
        version: 3,
        ..Default::default()
    };
    os.base.vendor_id = o.vendor;
    os.base.typo_metrics = [ascent, descent, h.line_gap];
    os.base.windows_metrics = [ascent.unsigned_abs(), descent.unsigned_abs()];
    let (mut advances, mut nonzero) = (0u64, 0usize);
    for m in hm.iter().filter(|m| m.advance != 0) {
        advances += u64::from(m.advance);
        nonzero += 1;
    }
    os.base.average_width = if nonzero == 0 {
        0
    } else {
        i16n((advances as f64 / nonzero as f64).round() as i64)?
    };
    os.extension.as_mut().unwrap().max_context = u16::from(!plan.gsub.features.is_empty());
    {
        let cps_lease = ledger::reserve_tree::<u32, ()>(budget, plan.cmap.len())?;
        let cps = plan.cmap.keys().copied().collect::<BTreeSet<_>>();
        os.set_unicode_coverage(&cps)?;
        const PAGES: [u16; 15] = [
            1252, 1250, 1251, 1253, 1254, 1255, 1256, 1257, 1258, 874, 932, 936, 949, 950, 1361,
        ];
        let pages = if cfg!(feature = "cjk-charsets") {
            &PAGES[..]
        } else {
            &PAGES[..10]
        };
        os.code_page_ranges = Some(tables::code_pages(budget, || {
            sfnt::os2::code_page_ranges(&cps, pages)
        })?);
        drop(cps);
        drop(cps_lease);
    }
    let m = &o.metadata;
    if let Some(ranges) = m.code_page_ranges {
        os.code_page_ranges = Some(ranges);
    }
    if let Some(flags) = m.embedding_flags {
        os.base.embedding_flags = flags;
    }
    if let Some(flags) = m.selection_flags {
        os.base.selection_flags = flags;
    }
    if let (Some(value), Some(extension)) = (m.break_char, os.extension.as_mut()) {
        extension.break_char = value;
    }
    b.insert(*b"OS/2", os.encode_with_budget(l, budget)?)?;
    {
        // Reordered or shifted glyph IDs: an admitted copy of the mapping.
        let remapped = if reorder || after_notdef {
            let lease = ledger::reserve_tree::<u32, u16>(budget, plan.cmap.len())?;
            let map = plan
                .cmap
                .iter()
                .map(|(&cp, &id)| {
                    let index = if reorder {
                        usize::from(positions[usize::from(id)])
                    } else {
                        usize::from(id)
                    };
                    (cp, final_id(index))
                })
                .collect::<BTreeMap<_, _>>();
            Some(Owned::from_parts(map, lease))
        } else {
            None
        };
        let mapping = remapped.as_deref().unwrap_or(&plan.cmap);
        let (cmap, lease) = sfnt::cmap::encode_records_with_budget(
            mapping,
            count as u16,
            l,
            budget,
            m.cmap_full_records,
        )?
        .into_parts();
        drop(remapped);
        b.charge(lease)?;
        b.insert_charged(*b"cmap", cmap.data)?;
    }
    drop(positions);
    {
        let mut post = PostTable::default();
        let mut names_lease = budget.reserve(0)?;
        if retain_names {
            post.header.underline_position = i16n(
                base.metrics
                    .underline_position
                    .unwrap_or(0)
                    .checked_mul(scale)
                    .ok_or_else(|| invalid("underline position overflow"))?,
            )?;
            post.header.underline_thickness = i16n(
                base.metrics
                    .underline_thickness
                    .unwrap_or(0)
                    .checked_mul(scale)
                    .ok_or_else(|| invalid("underline thickness overflow"))?,
            )?;
            let mut names = Nested::<String>::with_capacity(budget, count)?;
            for (name, _) in sources.iter() {
                names.push(ledger::string(budget, name)?)?;
            }
            if shared {
                names.push(ledger::string(budget, "fontpixel.dot")?)?;
                if after_notdef {
                    names[1..].rotate_right(1);
                }
            }
            let (names, mut lease) = names.into_owned()?.into_parts();
            names_lease.merge(&mut lease)?;
            post.names = Some(names);
        }
        post.header.is_fixed_pitch = u32::from(m.fixed_pitch.unwrap_or(matches!(
            base.metrics.spacing,
            Some(crate::model::Spacing::Monospaced | crate::model::Spacing::CharacterCell)
        )));
        b.insert(*b"post", post.encode_with_budget(count as u16, l, budget)?)?;
        drop(post);
        drop(names_lease);
    }
    {
        let names = if let Some(n) = &o.names {
            clone_names(budget, n)?
        } else {
            let mut records = Nested::<NameRecord>::with_capacity(budget, 22)?;
            let unique = ledger::string(budget, format_args!("{family} {style} {}", o.version))?;
            let version = ledger::string(budget, format_args!("Version {}", o.version))?;
            for (id, value) in [
                (1, family),
                (2, o.legacy_style.as_deref().unwrap_or("Regular")),
                (3, unique.as_str()),
                (4, fullname.as_str()),
                (5, version.as_str()),
                (6, psname.as_str()),
                (16, family),
                (17, style),
            ] {
                name_records(budget, &mut records, id, value)?;
            }
            for (id, text) in [
                (0, o.copyright.as_ref().or(base.names.copyright.as_ref())),
                (13, o.license.as_ref().or(base.names.license.as_ref())),
                (
                    14,
                    o.license_url.as_ref().or(base.names.license_url.as_ref()),
                ),
            ] {
                if let Some(s) = text {
                    name_records(budget, &mut records, id, s)?;
                }
            }
            drop(unique);
            drop(version);
            let (records, lease) = records.into_owned()?.into_parts();
            Owned::from_parts(
                NameTable {
                    records,
                    language_tags: Vec::new(),
                },
                lease,
            )
        };
        b.insert(*b"name", tables::name(&names, l, budget)?)?;
    }
    if !plan.gsub.features.is_empty() {
        b.insert(
            *b"GSUB",
            plan.gsub
                .encode_with_budget(count as u16, &o.limits, budget)?,
        )?;
    }
    if o.metadata.gasp == super::GaspPolicy::Write {
        let mut ranges = Vector::with_capacity(budget, 1)?;
        ranges.push(sfnt::gasp::Range {
            max_ppem: 65535,
            behavior: 0,
        })?;
        let (ranges, ranges_lease) = ranges.into_parts();
        let gasp = sfnt::gasp::GaspTable { version: 1, ranges };
        b.insert(*b"gasp", tables::small(budget, || gasp.encode(l))?)?;
        drop(gasp);
        drop(ranges_lease);
    }
    if o.embed_bitmaps {
        let ppem = u8::try_from(base.metrics.pixel_size.unwrap_or(14))
            .map_err(|_| invalid("bitmap ppem exceeds uint8"))?;
        let mut line = [0; 12];
        line[0] = i8::try_from(i64::from(ascent) / scale)
            .map_err(|_| invalid("bitmap ascent exceeds int8"))? as u8;
        line[1] = i8::try_from(i64::from(descent) / scale)
            .map_err(|_| invalid("bitmap descent exceeds int8"))? as u8;
        let strike = sbit::Strike {
            ppem_x: ppem,
            ppem_y: ppem,
            bit_depth: 1,
            flags: if vertical { 3 } else { 1 },
            horizontal: sbit::LineMetrics(line),
            vertical: sbit::LineMetrics::default(),
            glyphs: std::mem::take(&mut bitmaps),
        };
        let (e, lease) = sbit::encode_with_budget(
            std::slice::from_ref(&strike),
            sbit::Kind::OpenType,
            count as u16,
            &sbit::Limits {
                sfnt: l.clone(),
                ..Default::default()
            },
            budget,
        )?
        .into_parts();
        drop(strike);
        b.charge(lease)?;
        b.insert_charged(*b"EBDT", e.data)?;
        b.insert_charged(*b"EBLC", e.location)?;
    }
    drop(bitmaps);
    drop(bitmaps_heap);
    drop(bitmaps_tree);
    let (bytes, mut lease) = b.build(l, budget)?.into_parts();
    let (report, mut report_lease) = notes.into_report()?.into_parts();
    lease.merge(&mut report_lease)?;
    Ok(Owned::from_parts(Encoded { bytes, report }, lease))
}

/// Vectorize one complete Font while retaining aliases and unencoded glyphs.
/// Unlike regional planning this preserves physical glyph identity and order.
///
/// Glyph copies, names, mapping, diagnostics and export storage share one
/// working ledger of `o.limits.sfnt.resources.max_total_bytes`. The borrowed
/// font itself is outside that ledger; the caller accounts for it.
pub fn encode_single(font: &Font, o: &Options) -> Result<Encoded> {
    let budget = ledger::budget(o.limits.sfnt.resources.max_total_bytes);
    let planned = plan_single(font, o, &budget)?;
    finish_single(font, o, planned, &budget)
}
/// As [`encode_single`], consuming the font so that it is dropped as soon as
/// the plan has copied its glyphs. Output and report are identical.
///
/// `o.limits.sfnt.resources.max_total_bytes` must include this font: its
/// [`Font::retained_usage`] is admitted first on the same working ledger as
/// planning and export. Once the glyph copies exist, everything except the
/// metrics and names (all that export reads) is dropped, and exactly that
/// share of the charge is released before any table is encoded. The metadata
/// stays charged until the encoded bytes exist.
pub fn encode_single_owned(mut font: Font, o: &Options) -> Result<Encoded> {
    let max = o.limits.sfnt.resources.max_total_bytes;
    let budget = ledger::budget(max);
    let charge = font.retained_usage(max)?.bytes;
    let mut source = budget.reserve(charge)?;
    let planned = plan_single(&font, o, &budget)?;
    let mut metadata = Font::default();
    metadata.metrics = std::mem::take(&mut font.metrics);
    metadata.names = std::mem::take(&mut font.names);
    drop(font);
    let kept = metadata.retained_usage(max)?.bytes;
    let released = charge
        .checked_sub(kept)
        .ok_or_else(|| invalid("retained metadata exceeds its font charge"))?;
    drop(source.split(released)?);
    let encoded = finish_single(&metadata, o, planned, &budget)?;
    drop(metadata);
    drop(source);
    Ok(encoded)
}
/// Planned single-font glyph order, its leases, glyph 0 and diagnostics.
struct SinglePlan {
    plan: Plan,
    glyphs_lease: Lease,
    mapping_lease: Lease,
    source_notdef: Owned<Glyph>,
    notes: Notes,
}
/// Copy every glyph, name and mapping of `font` into an admitted plan. Export
/// afterwards reads only the font's metrics and names.
fn plan_single(font: &Font, o: &Options, budget: &Budget) -> Result<SinglePlan> {
    let budget = budget.clone();
    validate_names(o)?;
    font.validate(&o.limits.sfnt.resources)?;
    let mut native_bytes = 0usize;
    for (_, g) in font.glyphs() {
        native_bytes = native_bytes
            .checked_add(
                1024usize
                    .saturating_add(g.bitmap.as_bytes().len().saturating_mul(2))
                    .saturating_add(g.codepoints.len().saturating_mul(64))
                    .saturating_add(g.name.as_ref().map_or(0, String::len)),
            )
            .ok_or_else(|| invalid("native source size overflow"))?;
        sfnt::bounded(
            native_bytes,
            o.limits.sfnt.resources.max_total_bytes,
            "native outline source bytes",
        )?;
    }
    let mut mapping_lease = budget.reserve(0)?;
    let mut mapping = BTreeMap::new();
    let mut glyphs = Nested::<(String, Glyph)>::with_capacity(&budget, font.len())?;
    let notdef_id = font
        .glyphs()
        .find(|(_, g)| g.name.as_deref() == Some(".notdef"))
        .map(|(id, _)| id);
    o.limits
        .sfnt
        .glyphs(font.len().saturating_add(usize::from(notdef_id.is_none())))?;
    let source_notdef = match notdef_id.and_then(|id| font.glyph(id)) {
        Some(g) => ledger::View::new(font, g).retain(&budget)?,
        None => {
            let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
            g.metrics.advance = Some(crate::model::Vector::new(0, 0));
            Owned::from_parts(g, budget.reserve(0)?)
        }
    };
    let mut notes = Notes::new(&budget)?;
    if notdef_id.is_some() && o.notdef.empty_metrics().is_some() {
        notes.note(
            &budget,
            "variants.notdef.replaced",
            "The source .notdef glyph was replaced by the explicit empty .notdef policy",
        )?;
    }
    {
        let mut names_tree = budget.reserve(0)?;
        let mut names_heap = budget.reserve(0)?;
        let mut names = BTreeSet::<String>::new();
        let mut reserve_name = |names: &BTreeSet<String>, value: &str| -> Result<String> {
            ledger::tree::<String, ()>(&mut names_tree, &budget, names.len() + 1)?;
            let (value, mut lease) = ledger::string(&budget, value)?.into_parts();
            names_heap.merge(&mut lease)?;
            Ok(value)
        };
        let value = reserve_name(&names, ".notdef")?;
        names.insert(value);
        if o.flavor == OutlineFlavor::TrueType && o.shape == Shape::Dot {
            let value = reserve_name(&names, "fontpixel.dot")?;
            names.insert(value);
        }
        for (id, g) in font.glyphs() {
            let gid = if Some(id) == notdef_id {
                0
            } else {
                o.limits.sfnt.glyphs(glyphs.len().saturating_add(2))?;
                let source = g.name.as_deref().filter(|n| {
                    !n.is_empty()
                        && n.len() <= 63
                        && n.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
                });
                let mut name = match source {
                    Some(n) => ledger::string(&budget, n)?,
                    None => ledger::string(
                        &budget,
                        FallbackName {
                            index: id.index(),
                            extra: 0,
                        },
                    )?,
                };
                if names.contains(name.as_str()) {
                    let mut extra = 0;
                    name = ledger::string(
                        &budget,
                        FallbackName {
                            index: id.index(),
                            extra,
                        },
                    )?;
                    while names.contains(name.as_str()) {
                        extra += 1;
                        name = ledger::string(
                            &budget,
                            FallbackName {
                                index: id.index(),
                                extra,
                            },
                        )?;
                    }
                }
                if g.name.as_deref() != Some(name.as_str()) && g.name.is_some() {
                    notes.note(
                        &budget,
                        "opentype.glyph_name",
                        "Unrepresentable/duplicate glyph names were replaced with unique PostScript names",
                    )?;
                }
                let copy = reserve_name(&names, &name)?;
                names.insert(copy);
                let glyph = ledger::View::new(font, g).retain(&budget)?;
                glyphs.push(ledger::record(name, glyph)?)?;
                glyphs.len() as u16
            };
            for cp in &g.codepoints {
                let Codepoint::Unicode(cp) = *cp else {
                    return Err(invalid(
                        "native OpenType output requires Unicode codepoints",
                    ));
                };
                if char::from_u32(cp).is_none() {
                    return Err(invalid("non-scalar Unicode codepoint"));
                }
                if !mapping.contains_key(&cp) {
                    ledger::tree::<u32, u16>(&mut mapping_lease, &budget, mapping.len() + 1)?;
                }
                if mapping.insert(cp, gid).is_some() && !notes.has("opentype.duplicate_codepoint") {
                    notes.note(
                        &budget,
                        "opentype.duplicate_codepoint",
                        "Duplicate cmap entries resolve to the last source glyph",
                    )?;
                }
                sfnt::bounded(
                    mapping.len(),
                    o.limits.sfnt.max_cmap_mappings,
                    "native Unicode mappings",
                )?;
            }
        }
        drop(names);
    }
    let (glyphs, glyphs_lease) = glyphs.into_owned()?.into_parts();
    let plan = Plan {
        glyphs,
        cmap: mapping,
        gsub: Gsub::default(),
    };
    Ok(SinglePlan {
        plan,
        glyphs_lease,
        mapping_lease,
        source_notdef,
        notes,
    })
}
/// Export a single-font plan; `base` supplies only metrics and names.
fn finish_single(
    base: &Font,
    o: &Options,
    planned: SinglePlan,
    budget: &Budget,
) -> Result<Encoded> {
    let SinglePlan {
        plan,
        glyphs_lease,
        mapping_lease,
        source_notdef,
        notes,
    } = planned;
    let (mut encoded, encoded_lease) =
        encode_planned(base, o, &plan, Some(source_notdef), budget)?.into_parts();
    drop(plan);
    drop(glyphs_lease);
    drop(mapping_lease);
    let (report, report_lease) = notes.into_report()?.into_parts();
    // The output report's entry array may reallocate to hold both reports.
    let combined = encoded
        .report
        .entries
        .len()
        .checked_add(report.entries.len())
        .ok_or_else(|| budget.error())?;
    let growth = budget.reserve_array::<ReportEntry>(combined)?;
    encoded
        .report
        .entries
        .try_reserve_exact(report.entries.len())
        .map_err(|_| budget.error())?;
    encoded.report.extend(report);
    drop(growth);
    drop(report_lease);
    drop(encoded_lease);
    Ok(encoded)
}
