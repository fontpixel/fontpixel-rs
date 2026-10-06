//! Reconstructed W6.5 API; CURRENTLY UNTESTED. See RECOVERY.md.
//! Earlier conservative accounting is retained; the later parent fix is unavailable.
mod metadata;
mod report;
mod selection;
use crate::bitmap::Bounds;
use crate::charset::merge_categories::{
    UNICODE_BLOCKS, UNICODE_VERSION, is_cjk, is_geometric, is_han, is_scalar,
};
use crate::model::{
    Codepoint, Font, Glyph, GlyphId, GlyphMetrics, PropertyValue, Rational, Spacing, Vector,
};
use crate::report::{Impact, Level, Location};
use crate::{Error, Report, ReportEntry, ResourceLimits, Result};
pub use report::{
    AdjustmentKind, LicenseReport, MergeReport, MetricsReport, MissingBlock, PartMetrics,
    PartReport, SpacingAdjustment,
};
pub use selection::{CodepointRange, Selection};
use std::collections::{BTreeMap, btree_map::Entry};

/// Requested/resolved spacing policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum MergeSpacing {
    /// Mono only if every part explicitly declares mono/cell.
    #[default]
    Auto,
    /// Selected Han mode determines W and Western W/2.
    Mono,
    /// Preserve source effective advances.
    Prop,
}
impl MergeSpacing {
    /// CLI/JSON spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mono => "mono",
            Self::Prop => "prop",
        }
    }
}
/// Explicit width policy for geometric symbols.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum GeometricWidth {
    /// Western W/2 treatment.
    #[default]
    Half,
    /// CJK treatment preserving original fullwidth metrics.
    Full,
}
impl GeometricWidth {
    /// CLI/JSON spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Half => "half",
            Self::Full => "full",
        }
    }
}
fn fullwidth_category(cp: u32, options: &MergeOptions) -> bool {
    is_cjk(cp) || (options.geometric_width == GeometricWidth::Full && is_geometric(cp))
}
/// Declared modification permission, not a legal warranty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum ModificationPermission {
    /// No recognized declaration.
    #[default]
    Unknown,
    /// Caller/property/standard license declares permission.
    Allowed,
    /// Explicit restriction.
    Prohibited,
}
impl ModificationPermission {
    /// JSON spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Allowed => "allowed",
            Self::Prohibited => "prohibited",
        }
    }
}
/// Supplemental notices never erase originals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PartLicense {
    /// Additional copyright.
    pub copyright: Option<String>,
    /// Additional license.
    pub license: Option<String>,
    /// Additional license URL.
    pub license_url: Option<String>,
    /// Explicit permission; Unknown permits metadata inspection.
    pub modification: ModificationPermission,
    /// Additional known reserved names.
    pub reserved_names: Vec<String>,
}
/// Borrowed single strike, selection and displacement.
#[derive(Clone, Debug)]
pub struct MergePart<'a> {
    /// Explicitly selected source strike.
    pub font: &'a Font,
    /// Unicode selector.
    pub selection: Selection,
    /// Vertical offset, positive up.
    pub dy: i64,
    /// Component identifier, never implicitly a path.
    pub label: Option<String>,
    /// Supplemental notices.
    pub license: PartLicense,
}
impl<'a> MergePart<'a> {
    /// Construct an undisplaced part.
    pub fn new(font: &'a Font, selection: Selection) -> Self {
        Self {
            font,
            selection,
            dy: 0,
            label: None,
            license: PartLicense::default(),
        }
    }
    fn label_text(&self, index: usize, budget: &mut Budget) -> Result<String> {
        budget.text(
            |out| match self.label.as_deref().or(self.font.names.family.as_deref()) {
                Some(label) => out.write_str(label),
                None => write!(out, "part {index}"),
            },
        )
    }
}
/// Explicit merge choices.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct MergeOptions {
    /// Required new family.
    pub family: String,
    /// Requested spacing.
    #[cfg_attr(feature = "serde", serde(default))]
    pub spacing: MergeSpacing,
    /// Independent geometric width class.
    #[cfg_attr(feature = "serde", serde(default))]
    pub geometric_width: GeometricWidth,
    /// Preserve/report incompatible Western glyphs.
    #[cfg_attr(feature = "serde", serde(default))]
    pub allow_misaligned: bool,
    /// Permit first other source after selected-present sources.
    #[cfg_attr(feature = "serde", serde(default))]
    pub fallback: bool,
    /// Nonnegative ascent override.
    #[cfg_attr(feature = "serde", serde(default))]
    pub ascent: Option<i64>,
    /// Nonnegative descent override.
    #[cfg_attr(feature = "serde", serde(default))]
    pub descent: Option<i64>,
    /// Reject unknown/prohibited licensing and information loss.
    #[cfg_attr(feature = "serde", serde(default))]
    pub strict: bool,
    /// Aggregate source/work/output limits.
    #[cfg_attr(feature = "serde", serde(default))]
    pub limits: ResourceLimits,
}
impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            family: String::new(),
            spacing: MergeSpacing::Auto,
            geometric_width: GeometricWidth::Half,
            allow_misaligned: false,
            fallback: false,
            ascent: None,
            descent: None,
            strict: false,
            limits: ResourceLimits::default(),
        }
    }
}
/// Owned output and deterministic report.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct MergeResult {
    /// New strike.
    pub font: Font,
    /// Complete report.
    pub report: MergeReport,
}
struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn charge(&mut self, amount: usize) -> Result<()> {
        self.used = self.used.checked_add(amount).ok_or(Error::ResourceLimit {
            resource: "merge working bytes",
            limit: self.max,
        })?;
        if self.used > self.max {
            return Err(Error::ResourceLimit {
                resource: "merge working bytes",
                limit: self.max,
            });
        }
        Ok(())
    }

    // Keep the original conservative reservations, and additionally reserve
    // actual owned notice buffers before allocating them. No storage is credited
    // back here: sources, previous buffers and allocation overlap stay charged.
    fn text(
        &mut self,
        render: impl Fn(&mut dyn std::fmt::Write) -> std::fmt::Result,
    ) -> Result<String> {
        let maximum = self.max;
        let error = || Error::ResourceLimit {
            resource: "merge working bytes",
            limit: maximum,
        };
        let mut measured = AuditTextSize { bytes: 0, maximum };
        render(&mut measured).map_err(|_| error())?;
        self.charge(measured.bytes)?;
        let mut text = String::new();
        text.try_reserve_exact(measured.bytes)
            .map_err(|_| error())?;
        self.charge(
            text.capacity()
                .checked_sub(measured.bytes)
                .ok_or_else(error)?,
        )?;
        render(&mut AuditText {
            text: &mut text,
            maximum: measured.bytes,
        })
        .map_err(|_| error())?;
        Ok(text)
    }

    fn reserve<T>(&mut self, values: &mut Vec<T>, additional: usize) -> Result<()> {
        let maximum = self.max;
        let error = || Error::ResourceLimit {
            resource: "merge working bytes",
            limit: maximum,
        };
        let target = values.len().checked_add(additional).ok_or_else(error)?;
        if target <= values.capacity() {
            return Ok(());
        }
        let target = target
            .max(values.capacity().checked_mul(2).ok_or_else(error)?)
            .max(4);
        // Reserve the complete new array while the old allocation remains live.
        self.charge(
            target
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(error)?,
        )?;
        values
            .try_reserve_exact(target - values.len())
            .map_err(|_| error())?;
        self.charge(
            values
                .capacity()
                .checked_sub(target)
                .and_then(|extra| extra.checked_mul(std::mem::size_of::<T>()))
                .ok_or_else(error)?,
        )
    }
}
fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn checked(value: Option<i64>, what: &str) -> Result<i64> {
    value.ok_or_else(|| invalid(format!("merge {what} overflows")))
}
fn diagnostic(report: &mut Report, index: usize, code: &str, impact: Impact, message: String) {
    report.push(ReportEntry {
        level: Level::Warning,
        impact,
        code: code.into(),
        location: Location {
            font_index: Some(index),
            ..Location::default()
        },
        message,
    });
}
#[derive(Clone, Copy, Debug)]
struct Source {
    part: usize,
    id: GlyphId,
    selected: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    part: usize,
    id: GlyphId,
    western: bool,
}
#[derive(Default)]
struct Group {
    codes: Vec<u32>,
    sequences: Vec<Vec<u32>>,
}
fn effective(glyph: &Glyph, font: &Font) -> GlyphMetrics {
    let mut m = glyph.metrics.clone();
    let d = &font.metrics.glyph_defaults;
    m.advance = m.advance.or(d.advance);
    m.scalable_advance = m.scalable_advance.or(d.scalable_advance);
    m.vertical_advance = m.vertical_advance.or(d.vertical_advance);
    m.vertical_scalable_advance = m.vertical_scalable_advance.or(d.vertical_scalable_advance);
    m.vertical_origin = m.vertical_origin.or(d.vertical_origin);
    m.abc = m.abc.or(d.abc);
    m.left_bearing = m.left_bearing.or(d.left_bearing);
    m.right_bearing = m.right_bearing.or(d.right_bearing);
    m.image_offset = m.image_offset.or(d.image_offset);
    m
}
fn value_bytes(value: &PropertyValue) -> usize {
    match value {
        PropertyValue::Text(s) | PropertyValue::Decimal(s) => s.len(),
        PropertyValue::Bytes(b) => b.len(),
        PropertyValue::List(values) => values.iter().fold(
            values
                .len()
                .saturating_mul(std::mem::size_of::<PropertyValue>()),
            |n, v| n.saturating_add(value_bytes(v)),
        ),
        _ => 0,
    }
}
fn glyph_bytes(glyph: &Glyph) -> usize {
    let props = glyph.properties.iter().fold(0usize, |n, p| {
        n.saturating_add(128)
            .saturating_add(p.name.len())
            .saturating_add(p.raw.as_ref().map_or(0, Vec::len))
            .saturating_add(value_bytes(&p.value))
    });
    std::mem::size_of::<Glyph>()
        .saturating_mul(2)
        .saturating_add(128)
        .saturating_add(glyph.bitmap.as_bytes().len())
        .saturating_add(glyph.name.as_ref().map_or(0, String::len))
        .saturating_add(props)
        .saturating_add(glyph.codepoints.len().saturating_mul(16))
        .saturating_add(glyph.unicode_sequences.iter().fold(0usize, |n, seq| {
            n.saturating_add(48)
                .saturating_add(seq.len().saturating_mul(16))
        }))
}
fn omitted_counts(font: &Font) -> (usize, usize, usize) {
    let (mut legacy, mut invalid, mut unencoded) = (0, 0, 0);
    for (_, glyph) in font.glyphs() {
        if glyph.codepoints.is_empty() && glyph.unicode_sequences.is_empty() {
            unencoded += 1;
        }
        for &code in &glyph.codepoints {
            match code {
                Codepoint::Legacy(_) => legacy += 1,
                Codepoint::Unicode(cp) if !is_scalar(cp) => invalid += 1,
                _ => {}
            }
        }
        invalid += glyph
            .unicode_sequences
            .iter()
            .filter(|s| s.iter().any(|&cp| !is_scalar(cp)))
            .count();
    }
    (legacy, invalid, unencoded)
}
fn missing(
    part: &MergePart<'_>,
    budget: &mut Budget,
) -> Result<(Vec<CodepointRange>, Vec<MissingBlock>, u64)> {
    let mut result = Vec::new();
    for &range in part.selection.ranges() {
        let mut cursor = range.start;
        for (code, _) in part.font.codepoint_range(
            Codepoint::Unicode(range.start),
            Codepoint::Unicode(range.end),
        ) {
            let cp = code.value();
            if cursor < cp {
                budget.charge(32)?;
                result.push(CodepointRange {
                    start: cursor,
                    end: cp - 1,
                });
            }
            cursor = cp + 1;
        }
        if cursor <= range.end {
            budget.charge(32)?;
            result.push(CodepointRange {
                start: cursor,
                end: range.end,
            });
        }
    }
    let total = result.iter().map(|r| u64::from(r.end - r.start) + 1).sum();
    let mut blocks: BTreeMap<&str, MissingBlock> = BTreeMap::new();
    for range in &result {
        let mut cp = range.start;
        while cp <= range.end {
            let index = UNICODE_BLOCKS.partition_point(|b| b.end < cp);
            let (name, end) = match UNICODE_BLOCKS.get(index) {
                Some(b) if b.start <= cp => (b.name, b.end.min(range.end)),
                Some(b) => ("No_Block", (b.start - 1).min(range.end)),
                None => ("No_Block", range.end),
            };
            match blocks.entry(name) {
                Entry::Vacant(entry) => {
                    budget.charge(256 + name.len())?;
                    entry.insert(MissingBlock {
                        block: name.into(),
                        first: cp,
                        last: end,
                        count: u64::from(end - cp) + 1,
                    });
                }
                Entry::Occupied(mut entry) => {
                    let b = entry.get_mut();
                    b.last = end;
                    b.count += u64::from(end - cp) + 1;
                }
            }
            cp = end + 1;
        }
    }
    let mut blocks: Vec<_> = blocks.into_values().collect();
    blocks.sort_by_key(|b| b.first);
    Ok((result, blocks, total))
}
fn metrics(parts: &[MergePart<'_>], options: &MergeOptions) -> Result<MetricsReport> {
    let mut report = MetricsReport::default();
    for (index, part) in parts.iter().enumerate() {
        let mut top = 0;
        let mut bottom = 0;
        for bounds in part
            .font
            .metrics
            .bounds
            .iter()
            .copied()
            .chain(part.font.glyphs().map(|(_, g)| g.metrics.bounds))
        {
            top = top.max(checked(
                bounds.y.checked_add(i64::from(bounds.height)),
                "source top",
            )?);
            bottom = bottom.min(bounds.y);
        }
        let ascent = part.font.metrics.ascent.unwrap_or(top.max(0));
        let descent = part
            .font
            .metrics
            .descent
            .unwrap_or(checked(bottom.checked_neg(), "source descent")?.max(0));
        if ascent < 0 || descent < 0 {
            return Err(invalid(format!(
                "merge part {index} has negative ascent/descent"
            )));
        }
        let shifted_ascent = checked(ascent.checked_add(part.dy), "shifted ascent")?.max(0);
        let shifted_descent = checked(descent.checked_sub(part.dy), "shifted descent")?.max(0);
        report.natural_ascent = report.natural_ascent.max(shifted_ascent);
        report.natural_descent = report.natural_descent.max(shifted_descent);
        report.parts.push(PartMetrics {
            part_index: index,
            original_ascent: part.font.metrics.ascent,
            original_descent: part.font.metrics.descent,
            shifted_ascent,
            shifted_descent,
        });
    }
    report.ascent = options.ascent.unwrap_or(report.natural_ascent);
    report.descent = options.descent.unwrap_or(report.natural_descent);
    if report.ascent < 0 || report.descent < 0 {
        return Err(invalid(
            "merge ascent and descent overrides must be nonnegative",
        ));
    }
    checked(report.ascent.checked_add(report.descent), "line height")?;
    report.ascent_overridden = options.ascent.is_some();
    report.descent_overridden = options.descent.is_some();
    Ok(report)
}
fn modal_width(
    parts: &[MergePart<'_>],
    codes: &BTreeMap<u32, Source>,
    budget: &mut Budget,
) -> Result<i64> {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for (&cp, source) in codes {
        if !is_han(cp) || !source.selected {
            continue;
        }
        let font = parts[source.part].font;
        let glyph = font
            .glyph(source.id)
            .ok_or_else(|| invalid("merge source glyph disappeared"))?;
        let advance = effective(glyph, font).advance.ok_or_else(|| {
            invalid(format!(
                "selected Han U+{cp:04X} has no explicit device advance"
            ))
        })?;
        if advance.x <= 0 || advance.y != 0 {
            return Err(invalid(format!(
                "selected Han U+{cp:04X} needs a positive horizontal advance"
            )));
        }
        if !counts.contains_key(&advance.x) {
            budget.charge(128)?;
        }
        *counts.entry(advance.x).or_default() += 1;
    }
    let maximum = counts.values().max().copied().ok_or_else(|| {
        invalid("mono merge requires selected Han ideographs to determine full width")
    })?;
    let mut modes = counts
        .iter()
        .filter(|(_, count)| **count == maximum)
        .map(|(&width, _)| width);
    let width = modes
        .next()
        .ok_or_else(|| invalid("missing modal Han width"))?;
    if modes.next().is_some() {
        return Err(invalid(
            "selected Han advances have a tied mode; choose unambiguous source selection or proportional spacing",
        ));
    }
    Ok(width)
}
fn pad(
    glyph: &mut Glyph,
    width: i64,
    key: GroupKey,
    codes: &[u32],
    options: &MergeOptions,
    report: &mut MergeReport,
) -> Result<()> {
    let advance = glyph.metrics.advance;
    let target = if width % 2 == 0 {
        Some(width / 2)
    } else {
        None
    };
    let bad = if target.is_none() {
        Some("odd full width has no integral halfwidth")
    } else if advance.is_none() {
        Some("Western device advance is absent")
    } else if advance.is_some_and(|a| a.y != 0 || a.x < 0) {
        Some("Western device advance is non-horizontal or negative")
    } else if advance.is_some_and(|a| a.x > target.unwrap_or(0)) {
        Some("Western advance exceeds halfwidth target")
    } else {
        None
    };
    if let Some(reason) = bad {
        if !options.allow_misaligned {
            return Err(invalid(format!(
                "mono merge part {}, glyph {}: {reason}; use allow_misaligned to retain it",
                key.part,
                key.id.index()
            )));
        }
        report.adjustments.push(SpacingAdjustment {
            part_index: key.part,
            glyph_index: key.id.index(),
            codepoints: codes.to_vec(),
            kind: AdjustmentKind::Misaligned,
            original_advance: advance.map(|a| a.x),
            target_advance: target,
            left_padding: 0,
            right_padding: 0,
            reason: reason.into(),
        });
        return Ok(());
    }
    let target = target.ok_or_else(|| invalid("missing halfwidth target"))?;
    let advance = advance.ok_or_else(|| invalid("missing Western advance"))?;
    if advance.x == target {
        return Ok(());
    }
    let extra = target - advance.x;
    let left = extra / 2;
    let right = extra - left;
    glyph.shift(left, 0)?;
    glyph.metrics.advance = Some(Vector::new(target, advance.y));
    if let Some(abc) = &mut glyph.metrics.abc {
        abc.a = checked(abc.a.checked_add(left), "ABC left padding")?;
        abc.c = checked(abc.c.checked_add(right), "ABC right padding")?;
    }
    if let Some(bearing) = &mut glyph.metrics.left_bearing {
        *bearing = checked(bearing.checked_add(left), "left padding")?;
    }
    if let Some(bearing) = &mut glyph.metrics.right_bearing {
        *bearing = checked(bearing.checked_add(right), "right padding")?;
    }
    if let Some(offset) = &mut glyph.metrics.image_offset {
        offset.x = checked(offset.x.checked_add(left), "image padding")?;
    }
    if let Some(scalable) = &mut glyph.metrics.scalable_advance {
        if advance.x == 0 {
            diagnostic(
                &mut report.diagnostics,
                key.part,
                "merge.metrics.scalable_zero_advance",
                Impact::None,
                format!(
                    "Glyph {} zero device advance padded to {target}; independent scalable advance {:?} retained because its ratio is undefined",
                    key.id.index(),
                    scalable
                ),
            );
        } else {
            let product = i128::from(scalable.x) * i128::from(target);
            let denominator = i128::from(advance.x);
            let quotient = product / denominator;
            let remainder = product % denominator;
            let rounded = quotient
                + if remainder.abs() * 2 >= denominator {
                    remainder.signum()
                } else {
                    0
                };
            let output =
                i64::try_from(rounded).map_err(|_| invalid("merge scalable advance overflows"))?;
            if remainder != 0 {
                diagnostic(
                    &mut report.diagnostics,
                    key.part,
                    "merge.metrics.scalable_rounding",
                    Impact::Loss,
                    format!(
                        "Glyph {} scalable x advance {} * {target} / {} rounded to {output} (nearest integer, ties away from zero)",
                        key.id.index(),
                        scalable.x,
                        advance.x
                    ),
                );
            }
            scalable.x = output;
        }
    }
    report.adjustments.push(SpacingAdjustment {
        part_index: key.part,
        glyph_index: key.id.index(),
        codepoints: codes.to_vec(),
        kind: AdjustmentKind::Padded,
        original_advance: Some(advance.x),
        target_advance: Some(target),
        left_padding: left,
        right_padding: right,
        reason: "blank advance space; original raster preserved; odd extra pixel placed on right"
            .into(),
    });
    Ok(())
}
// Count formatting without allocating, then write only within that reservation.
// In particular, Debug formatting may expand every byte in a property name.
struct AuditTextSize {
    bytes: usize,
    maximum: usize,
}
impl std::fmt::Write for AuditTextSize {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.bytes = self
            .bytes
            .checked_add(text.len())
            .filter(|bytes| *bytes <= self.maximum)
            .ok_or(std::fmt::Error)?;
        Ok(())
    }
}
struct AuditText<'a> {
    text: &'a mut String,
    maximum: usize,
}
impl std::fmt::Write for AuditText<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.text
            .len()
            .checked_add(text.len())
            .filter(|bytes| *bytes <= self.maximum)
            .ok_or(std::fmt::Error)?;
        self.text.push_str(text);
        Ok(())
    }
}
fn audit_entry(
    report: &mut Report,
    budget: &mut Budget,
    index: usize,
    code: &str,
    message: std::fmt::Arguments<'_>,
) -> Result<()> {
    budget_entry(
        report,
        budget,
        index,
        code,
        Level::Info,
        Impact::None,
        message,
    )
}
fn budget_entry(
    report: &mut Report,
    budget: &mut Budget,
    index: usize,
    code: &str,
    level: Level,
    impact: Impact,
    message: std::fmt::Arguments<'_>,
) -> Result<()> {
    let maximum = budget.max;
    let error = || Error::ResourceLimit {
        resource: "merge working bytes",
        limit: maximum,
    };
    let mut measured = AuditTextSize { bytes: 0, maximum };
    std::fmt::write(&mut measured, message).map_err(|_| error())?;
    let target = if report.entries.len() == report.entries.capacity() {
        report
            .entries
            .capacity()
            .checked_mul(2)
            .and_then(|capacity| {
                report
                    .entries
                    .len()
                    .checked_add(1)
                    .map(|len| capacity.max(len).max(4))
            })
            .ok_or_else(error)?
    } else {
        0
    };
    // Existing entries remain charged. Reserve the complete new array, not
    // only its added slots, so old/new arrays can coexist during reallocation.
    let array_bytes = target
        .checked_mul(std::mem::size_of::<ReportEntry>())
        .ok_or_else(error)?;
    let bytes = measured
        .bytes
        .checked_add(code.len())
        .and_then(|bytes| bytes.checked_add(array_bytes))
        .ok_or_else(error)?;
    budget.charge(bytes)?;
    let mut text = String::new();
    text.try_reserve_exact(measured.bytes)
        .map_err(|_| error())?;
    budget.charge(
        text.capacity()
            .checked_sub(measured.bytes)
            .ok_or_else(error)?,
    )?;
    std::fmt::write(
        &mut AuditText {
            text: &mut text,
            maximum: measured.bytes,
        },
        message,
    )
    .map_err(|_| error())?;
    let mut owned_code = String::new();
    owned_code
        .try_reserve_exact(code.len())
        .map_err(|_| error())?;
    budget.charge(
        owned_code
            .capacity()
            .checked_sub(code.len())
            .ok_or_else(error)?,
    )?;
    owned_code.push_str(code);
    if target != 0 {
        report
            .entries
            .try_reserve_exact(target - report.entries.len())
            .map_err(|_| error())?;
        let extra = report
            .entries
            .capacity()
            .checked_sub(target)
            .and_then(|slots| slots.checked_mul(std::mem::size_of::<ReportEntry>()))
            .ok_or_else(error)?;
        budget.charge(extra)?;
    }
    report.push(ReportEntry {
        level,
        impact,
        code: owned_code,
        location: Location {
            font_index: Some(index),
            ..Location::default()
        },
        message: text,
    });
    Ok(())
}
fn audit_global_metadata(
    parts: &[MergePart<'_>],
    font: &Font,
    report: &mut Report,
    budget: &mut Budget,
) -> Result<()> {
    for (index, part) in parts.iter().enumerate() {
        let a = &part.font.metrics;
        let b = &font.metrics;
        macro_rules! metric{($($field:ident),* $(,)?)=>{$({
            if a.$field!=b.$field{
                audit_entry(report,budget,index,concat!("merge.metrics.",stringify!($field)),
                    format_args!("Global {} from part {}: {:?} -> {:?}",stringify!($field),index,a.$field,b.$field))?;
            }
        })*};}
        metric!(
            pixel_size,
            ppem_x,
            ppem_y,
            point_size,
            resolution,
            ascent,
            descent,
            bounds,
            line_gap,
            line_height,
            underline_position,
            underline_thickness,
            default_character,
            space_width,
            spacing,
            metrics_set
        );
        if a.default_glyph.is_some() || a.glyph_defaults != GlyphMetrics::default() {
            audit_entry(
                report,
                budget,
                index,
                "merge.metrics.defaults",
                format_args!(
                    "Part {index} glyph defaults materialized per glyph; default glyph {:?} -> {:?} (output IDs are reassigned)",
                    a.default_glyph, b.default_glyph
                ),
            )?;
        }
        for (kind, props) in [
            ("header", &part.font.headers),
            ("property", &part.font.properties),
            ("localized_name", &part.font.names.localized),
        ] {
            for property in props.iter() {
                if kind == "property"
                    && [
                        "COPYRIGHT",
                        "LICENSE",
                        "LICENSE_TEXT",
                        "LICENSE_URL",
                        "SPDX_LICENSE_IDENTIFIER",
                    ]
                    .iter()
                    .any(|k| property.name.eq_ignore_ascii_case(k))
                {
                    continue;
                }
                audit_entry(
                    report,
                    budget,
                    index,
                    "merge.metadata.source_record",
                    format_args!(
                        "Part {index} source {kind} {:?} is not copied into regenerated global metadata; selected glyph properties are retained",
                        property.name
                    ),
                )?;
            }
        }
    }
    Ok(())
}

fn retained_result(
    parts: &[MergePart<'_>],
    options: &MergeOptions,
    font: &Font,
    report: &MergeReport,
) -> Result<()> {
    let maximum = options.limits.max_total_bytes;
    let error = || Error::ResourceLimit {
        resource: "merge working bytes",
        limit: maximum,
    };
    let mut live = Budget {
        used: 0,
        max: maximum,
    };
    live.charge(std::mem::size_of_val(parts))?;
    live.charge(std::mem::size_of::<MergeOptions>())?;
    live.charge(options.family.capacity())?;
    for part in parts {
        live.charge(part.font.retained_usage(maximum)?.bytes)?;
        live.charge(part.selection.allocation_bytes().ok_or_else(error)?)?;
        for text in part
            .label
            .iter()
            .chain(part.license.copyright.iter())
            .chain(part.license.license.iter())
            .chain(part.license.license_url.iter())
        {
            live.charge(text.capacity())?;
        }
        live.charge(
            part.license
                .reserved_names
                .capacity()
                .checked_mul(std::mem::size_of::<String>())
                .ok_or_else(error)?,
        )?;
        for name in &part.license.reserved_names {
            live.charge(name.capacity())?;
        }
    }
    live.charge(font.retained_usage(maximum)?.bytes)?;
    live.charge(report.retained_bytes(maximum)?)
}
/// Merge selected Unicode mappings without I/O, charset guessing or raster conversion.
pub fn merge(parts: &[MergePart<'_>], options: &MergeOptions) -> Result<MergeResult> {
    if parts.is_empty() {
        return Err(invalid("merge requires at least one part"));
    }
    if options.family.trim().is_empty()
        || options.family.trim() != options.family
        || options.family.chars().any(char::is_control)
    {
        return Err(invalid(
            "merge requires a nonempty new family without surrounding whitespace/control characters",
        ));
    }
    if parts.len() > options.limits.max_resources {
        return Err(Error::ResourceLimit {
            resource: "merge parts",
            limit: options.limits.max_resources,
        });
    }
    if options.family.len() > options.limits.max_table_bytes {
        return Err(Error::ResourceLimit {
            resource: "merge family bytes",
            limit: options.limits.max_table_bytes,
        });
    }
    let mut budget = Budget {
        used: 0,
        max: options.limits.max_total_bytes,
    };
    budget.charge(options.family.len().saturating_mul(8))?;
    let mut glyph_count = 0usize;
    for part in parts {
        part.font.validate(&options.limits)?;
        let usage = part.font.retained_usage(options.limits.max_total_bytes)?;
        budget.charge(usage.bytes)?;
        let notice_bytes = [
            &part.font.names.family,
            &part.font.names.copyright,
            &part.font.names.license,
            &part.font.names.license_url,
        ]
        .iter()
        .fold(0usize, |n, s| {
            n.saturating_add(s.as_ref().map_or(0, String::len))
        });
        let property_bytes = part.font.properties.iter().fold(0usize, |n, p| {
            n.saturating_add(p.name.len())
                .saturating_add(value_bytes(&p.value))
        });
        budget.charge(
            notice_bytes
                .saturating_add(property_bytes)
                .saturating_mul(16),
        )?;
        if part.license.reserved_names.len() > options.limits.max_resources {
            return Err(Error::ResourceLimit {
                resource: "merge reserved names",
                limit: options.limits.max_resources,
            });
        }
        budget.charge(part.license.reserved_names.len().saturating_mul(64))?;
        glyph_count = glyph_count
            .checked_add(usage.glyphs)
            .ok_or(Error::ResourceLimit {
                resource: "merge input glyphs",
                limit: options.limits.max_glyphs,
            })?;
        if glyph_count > options.limits.max_glyphs {
            return Err(Error::ResourceLimit {
                resource: "merge input glyphs",
                limit: options.limits.max_glyphs,
            });
        }
        for text in part
            .license
            .copyright
            .iter()
            .chain(part.license.license.iter())
            .chain(part.license.license_url.iter())
            .chain(part.license.reserved_names.iter())
            .chain(part.label.iter())
        {
            if text.len() > options.limits.max_table_bytes {
                return Err(Error::ResourceLimit {
                    resource: "merge metadata text",
                    limit: options.limits.max_table_bytes,
                });
            }
            budget.charge(text.len().saturating_mul(8))?;
        }
        budget.charge(
            2048 + part.selection.expression().len() + part.selection.ranges().len() * 32,
        )?;
    }
    let spacing = match options.spacing {
        MergeSpacing::Auto
            if parts.iter().all(|p| {
                matches!(
                    p.font.metrics.spacing,
                    Some(Spacing::Monospaced | Spacing::CharacterCell)
                )
            }) =>
        {
            MergeSpacing::Mono
        }
        MergeSpacing::Auto => MergeSpacing::Prop,
        mode => mode,
    };
    let mut diagnostics = Report::default();
    let licenses = metadata::licenses(parts, options, &mut diagnostics, &mut budget)?;
    let mut report = MergeReport {
        unicode_version: UNICODE_VERSION.into(),
        family: options.family.clone(),
        requested_spacing: options.spacing,
        effective_spacing: spacing,
        geometric_width: options.geometric_width,
        full_width: None,
        parts: Vec::new(),
        adjustments: Vec::new(),
        metrics: metrics(parts, options)?,
        licenses,
        diagnostics,
    };
    let mut codes: BTreeMap<u32, Source> = BTreeMap::new();
    let mut sequences: BTreeMap<&[u32], Source> = BTreeMap::new();
    for (index, part) in parts.iter().enumerate() {
        let (legacy, invalid_count, unencoded) = omitted_counts(part.font);
        for (count, code, kind) in [
            (legacy, "merge.mapping.legacy_omitted", "legacy mappings"),
            (
                invalid_count,
                "merge.mapping.invalid_omitted",
                "invalid scalar/sequence mappings",
            ),
            (
                unencoded,
                "merge.mapping.unencoded_omitted",
                "unencoded glyphs",
            ),
        ] {
            if count != 0 {
                diagnostic(
                    &mut report.diagnostics,
                    index,
                    code,
                    Impact::Loss,
                    format!(
                        "{count} {kind} omitted: merge selection is Unicode-only; explicitly convert or encode them before merging"
                    ),
                );
            }
        }
        let (missing_ranges, missing_blocks, selected_missing) = missing(part, &mut budget)?;
        report.parts.push(PartReport {
            index,
            label: part.label_text(index, &mut budget)?,
            selection: part.selection.expression().into(),
            dy: part.dy,
            taken_glyphs: 0,
            taken_codepoints: 0,
            taken_sequences: 0,
            fallback_codepoints: 0,
            fallback_sequences: 0,
            selected_missing,
            missing_ranges,
            missing_blocks,
            omitted_legacy_mappings: legacy,
            omitted_invalid_mappings: invalid_count,
            omitted_unencoded_glyphs: unencoded,
        });
        for (code, id) in part.font.codepoints() {
            let Codepoint::Unicode(cp) = code else {
                continue;
            };
            if !is_scalar(cp) {
                continue;
            }
            let selected = part.selection.contains(cp);
            if !selected && (!options.fallback || !parts.iter().any(|p| p.selection.contains(cp))) {
                continue;
            }
            let source = Source {
                part: index,
                id,
                selected,
            };
            match codes.entry(cp) {
                Entry::Vacant(e) => {
                    budget.charge(256)?;
                    e.insert(source);
                }
                Entry::Occupied(mut e) if selected && !e.get().selected => {
                    e.insert(source);
                }
                _ => {}
            }
        }
        for (id, glyph) in part.font.glyphs() {
            for sequence in &glyph.unicode_sequences {
                if sequence.iter().any(|&cp| !is_scalar(cp))
                    || !part
                        .font
                        .glyph_for_sequence(sequence)
                        .is_some_and(|g| std::ptr::eq(g, glyph))
                {
                    continue;
                }
                let selected = sequence.iter().all(|&cp| part.selection.contains(cp));
                if !selected
                    && (!options.fallback
                        || !parts
                            .iter()
                            .any(|p| sequence.iter().all(|&cp| p.selection.contains(cp))))
                {
                    continue;
                }
                let source = Source {
                    part: index,
                    id,
                    selected,
                };
                match sequences.entry(sequence) {
                    Entry::Vacant(e) => {
                        budget.charge(256)?;
                        e.insert(source);
                    }
                    Entry::Occupied(mut e) if selected && !e.get().selected => {
                        e.insert(source);
                    }
                    _ => {}
                }
            }
        }
    }
    if options.strict
        && (report.diagnostics.has_loss()
            || report
                .diagnostics
                .entries
                .iter()
                .any(|e| e.code.starts_with("merge.license.")))
    {
        return Err(Error::StrictLoss(report.diagnostics));
    }
    if spacing == MergeSpacing::Mono {
        report.full_width = Some(modal_width(parts, &codes, &mut budget)?);
    }
    let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
    for (&cp, &source) in &codes {
        let key = GroupKey {
            part: source.part,
            id: source.id,
            western: spacing == MergeSpacing::Mono && !fullwidth_category(cp, options),
        };
        if !groups.contains_key(&key) {
            budget.charge(256)?;
        }
        budget.charge(1024)?;
        groups.entry(key).or_default().codes.push(cp);
        report.parts[source.part].taken_codepoints += 1;
        if !source.selected {
            report.parts[source.part].fallback_codepoints += 1;
        }
    }
    for (sequence, source) in sequences {
        let key = GroupKey {
            part: source.part,
            id: source.id,
            western: spacing == MergeSpacing::Mono
                && !sequence.iter().any(|&cp| fullwidth_category(cp, options)),
        };
        if !groups.contains_key(&key) {
            budget.charge(256)?;
        }
        budget.charge(1024 + sequence.len().saturating_mul(16))?;
        groups
            .entry(key)
            .or_default()
            .sequences
            .push(sequence.to_vec());
        report.parts[source.part].taken_sequences += 1;
        if !source.selected {
            report.parts[source.part].fallback_sequences += 1;
        }
    }
    if groups.len() > options.limits.max_glyphs {
        return Err(Error::ResourceLimit {
            resource: "merge output glyphs",
            limit: options.limits.max_glyphs,
        });
    }
    let mut font = Font::default();
    let first = parts[0].font;
    font.metrics = first.metrics.clone();
    font.metrics.glyph_defaults = GlyphMetrics::default();
    font.metrics.default_glyph = None;
    font.metrics.default_character = None;
    font.metrics.point_size = first.metrics.point_size.or_else(|| {
        first
            .metrics
            .pixel_size
            .map(|v| Rational::integer(i64::from(v)))
    });
    font.metrics.resolution = first.metrics.resolution.or(Some(Vector::new(72, 72)));
    font.metrics.ascent = Some(report.metrics.ascent);
    font.metrics.descent = Some(report.metrics.descent);
    font.metrics.spacing = Some(if spacing == MergeSpacing::Mono {
        Spacing::Monospaced
    } else {
        Spacing::Proportional
    });
    let cell_height = checked(
        report.metrics.ascent.checked_add(report.metrics.descent),
        "line height",
    )?;
    font.metrics.line_height = Some(checked(
        cell_height.checked_add(first.metrics.line_gap.unwrap_or(0)),
        "line gap",
    )?);
    metadata::write_names(&mut font, options, &report.licenses, &mut budget)?;
    let mut left = 0i64;
    let mut right = 0i64;
    let mut bottom = 0i64;
    let mut top = 0i64;
    let mut last_source = None;
    for (key, group) in groups {
        let part = &parts[key.part];
        let original = part
            .font
            .glyph(key.id)
            .ok_or_else(|| invalid("merge source glyph disappeared"))?;
        budget.charge(glyph_bytes(original))?;
        let mut glyph = original.clone();
        glyph.metrics = effective(original, part.font);
        glyph.codepoints=original.codepoints.iter().copied().filter(|cp|matches!(cp,Codepoint::Unicode(value) if group.codes.binary_search(value).is_ok())).collect();
        glyph.unicode_sequences = original
            .unicode_sequences
            .iter()
            .filter(|s| group.sequences.binary_search(s).is_ok())
            .cloned()
            .collect();
        glyph.shift(0, part.dy)?;
        if let Some(offset) = &mut glyph.metrics.image_offset {
            offset.y = checked(offset.y.checked_sub(part.dy), "image vertical displacement")?;
        }
        if key.western {
            budget.charge(512 + group.codes.len() * 8)?;
            pad(
                &mut glyph,
                report
                    .full_width
                    .ok_or_else(|| invalid("missing full width"))?,
                key,
                &group.codes,
                options,
                &mut report,
            )?;
        }
        if last_source == Some((key.part, key.id)) {
            diagnostic(
                &mut report.diagnostics,
                key.part,
                "merge.mapping.alias_split",
                Impact::None,
                format!(
                    "Glyph {} split into fullwidth/halfwidth records to preserve both alias mappings",
                    key.id.index()
                ),
            );
        }
        last_source = Some((key.part, key.id));
        let b = glyph.metrics.bounds;
        left = left.min(b.x);
        bottom = bottom.min(b.y);
        right = right.max(checked(
            b.x.checked_add(i64::from(b.width)),
            "output right",
        )?);
        top = top.max(checked(b.y.checked_add(i64::from(b.height)), "output top")?);
        let new_id = font.add_glyph(glyph)?;
        if key.part == 0
            && Some(key.id) == first.metrics.default_glyph
            && font.metrics.default_glyph.is_none()
        {
            font.metrics.default_glyph = Some(new_id);
        }
        report.parts[key.part].taken_glyphs += 1;
    }
    let width = u32::try_from(checked(right.checked_sub(left), "font width")?)
        .map_err(|_| invalid("merge font bounds width exceeds u32"))?;
    let height = u32::try_from(checked(top.checked_sub(bottom), "font height")?)
        .map_err(|_| invalid("merge font bounds height exceeds u32"))?;
    font.metrics.bounds = Some(Bounds::new(width, height, left, bottom));
    if font.metrics.point_size.is_none() {
        font.metrics.point_size = Some(Rational::integer(font.metrics.line_height.unwrap_or(0)));
    }
    font.metrics.space_width = font
        .glyph_for_codepoint(Codepoint::Unicode(32))
        .and_then(|g| g.metrics.advance.map(|a| a.x));
    if let Some(code) = first.metrics.default_character
        && font.glyph_for_codepoint(code).is_some()
    {
        font.metrics.default_character = Some(code);
    }
    audit_global_metadata(parts, &font, &mut report.diagnostics, &mut budget)?;
    font.validate(&options.limits)?;
    // This second, nonallocating check covers the complete simultaneously
    // retained result and its borrowed caller inputs. It supplements prospective
    // allocations above; it does not grant credit against their reservations.
    retained_result(parts, options, &font, &report)?;
    if options.strict && report.diagnostics.has_loss() {
        return Err(Error::StrictLoss(report.diagnostics));
    }
    Ok(MergeResult { font, report })
}

#[cfg(test)]
mod audit_budget_tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn final_aggregate_counts_report_and_caller_spare_capacity() {
        let source = Font::default();
        let mut parts = [MergePart::new(&source, Selection::parse("U+0041").unwrap())];
        let mut options = MergeOptions {
            family: "New Family".into(),
            spacing: MergeSpacing::Prop,
            ..Default::default()
        };
        let mut result = merge(&parts, &options).unwrap();
        let before = result.report.retained_bytes(usize::MAX).unwrap();
        let old_capacity = result.report.parts[0].label.capacity();
        result.report.parts[0].label.reserve_exact(128 * 1024);
        let added = result.report.parts[0].label.capacity() - old_capacity;
        assert_eq!(
            result.report.retained_bytes(usize::MAX).unwrap(),
            before + added
        );
        options.limits.max_total_bytes = 64 * 1024;
        assert!(
            result
                .font
                .retained_usage(options.limits.max_total_bytes)
                .is_ok()
        );
        assert!(matches!(
            retained_result(&parts, &options, &result.font, &result.report),
            Err(Error::ResourceLimit { .. })
        ));
        options.limits.max_total_bytes = ResourceLimits::default().max_total_bytes;
        let result = merge(&parts, &options).unwrap();
        parts[0].label = Some(String::with_capacity(128 * 1024));
        options.limits.max_total_bytes = 64 * 1024;
        assert!(matches!(
            retained_result(&parts, &options, &result.font, &result.report),
            Err(Error::ResourceLimit { .. })
        ));
    }

    #[test]
    fn escaped_diagnostic_has_exact_and_one_byte_short_boundaries() {
        for name in ["plain".to_owned(), "\n\0\"\\\u{7f}é".repeat(1000)] {
            let expected = format!("Name {name:?}");
            let code = "merge.test.audit";
            let mut measured = Budget {
                used: 0,
                max: usize::MAX,
            };
            let mut report = Report::default();
            audit_entry(
                &mut report,
                &mut measured,
                2,
                code,
                format_args!("Name {name:?}"),
            )
            .unwrap();
            assert_eq!(report.entries[0].message, expected);
            assert_eq!(report.entries[0].code, code);
            assert_eq!(report.entries[0].location.font_index, Some(2));
            assert_eq!(report.entries[0].impact, Impact::None);
            assert_eq!(
                measured.used,
                report.entries.capacity() * std::mem::size_of::<ReportEntry>()
                    + report.entries[0].code.capacity()
                    + report.entries[0].message.capacity()
            );
            let mut exact = Budget {
                used: 0,
                max: measured.used,
            };
            let mut copy = Report::default();
            audit_entry(
                &mut copy,
                &mut exact,
                2,
                code,
                format_args!("Name {name:?}"),
            )
            .unwrap();
            assert_eq!(copy, report);
            let mut short = Budget {
                used: 0,
                max: measured.used - 1,
            };
            let mut rejected = Report::default();
            assert!(
                audit_entry(
                    &mut rejected,
                    &mut short,
                    2,
                    code,
                    format_args!("Name {name:?}")
                )
                .is_err()
            );
            assert_eq!(rejected.entries.capacity(), 0);
            assert!(rejected.entries.is_empty());
        }
    }

    fn full_report() -> Report {
        Report {
            entries: vec![ReportEntry {
                level: Level::Info,
                impact: Impact::None,
                code: String::new(),
                location: Location::default(),
                message: String::new(),
            }],
        }
    }

    #[test]
    fn diagnostic_growth_reserves_both_old_and_new_arrays() {
        let mut report = full_report();
        let before = report.clone();
        let old = report.entries.capacity() * std::mem::size_of::<ReportEntry>();
        let mut measured = Budget {
            used: old,
            max: usize::MAX,
        };
        audit_entry(
            &mut report,
            &mut measured,
            0,
            "audit",
            format_args!("message"),
        )
        .unwrap();
        let entry = &report.entries[1];
        assert_eq!(
            measured.used,
            old + report.entries.capacity() * std::mem::size_of::<ReportEntry>()
                + entry.code.capacity()
                + entry.message.capacity()
        );
        assert_eq!(report.entries[0], before.entries[0]);
        let mut exact = Budget {
            used: old,
            max: measured.used,
        };
        audit_entry(
            &mut full_report(),
            &mut exact,
            0,
            "audit",
            format_args!("message"),
        )
        .unwrap();
        let mut short = Budget {
            used: old,
            max: measured.used - 1,
        };
        let mut rejected = full_report();
        assert!(
            audit_entry(
                &mut rejected,
                &mut short,
                0,
                "audit",
                format_args!("message")
            )
            .is_err()
        );
        assert_eq!(rejected, before);
        assert_eq!(rejected.entries.capacity(), 1);
    }

    #[test]
    fn header_and_localized_audit_preserve_text_and_source_at_boundary() {
        let name = "\n\0\"\\".repeat(1000);
        let mut source = Font::default();
        source.headers.push(crate::model::Property::new(
            name.clone(),
            PropertyValue::Flag,
        ));
        source
            .names
            .localized
            .push(crate::model::Property::new(name.clone(), "value"));
        let before = source.clone();
        let target = Font::default();
        let parts = [MergePart::new(&source, Selection::parse("U+0041").unwrap())];
        let retained = source.retained_usage(usize::MAX).unwrap().bytes
            + target.retained_usage(usize::MAX).unwrap().bytes;
        let mut measured = Budget {
            used: retained,
            max: usize::MAX,
        };
        let mut report = Report::default();
        audit_global_metadata(&parts, &target, &mut report, &mut measured).unwrap();
        assert_eq!(report.entries.len(), 2);
        for (entry, kind) in report.entries.iter().zip(["header", "localized_name"]) {
            assert_eq!(
                entry.message,
                format!(
                    "Part 0 source {kind} {name:?} is not copied into regenerated global metadata; selected glyph properties are retained"
                )
            );
            assert_eq!(entry.code, "merge.metadata.source_record");
        }
        let mut exact = Budget {
            used: retained,
            max: measured.used,
        };
        let mut copy = Report::default();
        audit_global_metadata(&parts, &target, &mut copy, &mut exact).unwrap();
        assert_eq!(copy, report);
        let mut short = Budget {
            used: retained,
            max: measured.used - 1,
        };
        assert!(
            audit_global_metadata(&parts, &target, &mut Report::default(), &mut short).is_err()
        );
        assert_eq!(source, before);
    }

    #[test]
    fn audit_measurement_and_total_addition_reject_overflow() {
        let mut text = AuditTextSize {
            bytes: usize::MAX,
            maximum: usize::MAX,
        };
        assert!(text.write_str("x").is_err());
        let mut budget = Budget {
            used: usize::MAX,
            max: usize::MAX,
        };
        let mut report = Report::default();
        assert!(
            audit_entry(
                &mut report,
                &mut budget,
                0,
                "audit",
                format_args!("message")
            )
            .is_err()
        );
        assert_eq!(report.entries.capacity(), 0);
    }
}
