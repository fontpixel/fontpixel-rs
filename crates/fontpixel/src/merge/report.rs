//! Reconstructed W6.5 report implementation; currently untested.
use super::{CodepointRange, GeometricWidth, MergeSpacing, ModificationPermission};
use crate::Report;
use std::fmt::{self, Write};

/// Missing selected scalar positions grouped by actual Unicode block.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MissingBlock {
    /// Block name or No_Block.
    pub block: String,
    /// First missing position.
    pub first: u32,
    /// Last missing position.
    pub last: u32,
    /// Exact missing count.
    pub count: u64,
}
/// Selection, contribution and omission totals for an input part.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PartReport {
    /// Zero-based index.
    pub index: usize,
    /// Component label.
    pub label: String,
    /// Selection expression.
    pub selection: String,
    /// Vertical shift, positive up.
    pub dy: i64,
    /// Output glyph records.
    pub taken_glyphs: usize,
    /// Distinct scalar mappings.
    pub taken_codepoints: usize,
    /// Distinct complete sequence mappings.
    pub taken_sequences: usize,
    /// Scalars supplied through fallback.
    pub fallback_codepoints: usize,
    /// Sequences supplied through fallback.
    pub fallback_sequences: usize,
    /// Selected positions absent from this source.
    pub selected_missing: u64,
    /// Exact missing intervals.
    pub missing_ranges: Vec<CodepointRange>,
    /// Missing block summaries.
    pub missing_blocks: Vec<MissingBlock>,
    /// Non-Unicode source mappings omitted.
    pub omitted_legacy_mappings: usize,
    /// Non-scalar mappings omitted.
    pub omitted_invalid_mappings: usize,
    /// Unencoded records omitted.
    pub omitted_unencoded_glyphs: usize,
}
/// Spacing adjustment type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
pub enum AdjustmentKind {
    /// Blank advance-space was added.
    Padded,
    /// Incompatible original metrics were explicitly retained.
    Misaligned,
}
/// One output glyph's halfwidth decision.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SpacingAdjustment {
    /// Source part.
    pub part_index: usize,
    /// Source stable glyph index.
    pub glyph_index: usize,
    /// Scalar aliases.
    pub codepoints: Vec<u32>,
    /// Decision.
    pub kind: AdjustmentKind,
    /// Original horizontal advance.
    pub original_advance: Option<i64>,
    /// Integral halfwidth target, absent for odd W.
    pub target_advance: Option<i64>,
    /// Added left advance-space.
    pub left_padding: i64,
    /// Added right advance-space.
    pub right_padding: i64,
    /// Human-readable explanation.
    pub reason: String,
}
/// Per-part baseline metric accounting.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PartMetrics {
    /// Source part.
    pub part_index: usize,
    /// Original explicit ascent.
    pub original_ascent: Option<i64>,
    /// Original explicit descent.
    pub original_descent: Option<i64>,
    /// Effective shifted ascent.
    pub shifted_ascent: i64,
    /// Effective shifted descent.
    pub shifted_descent: i64,
}
/// Natural and overridden output metrics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MetricsReport {
    /// Source metrics.
    pub parts: Vec<PartMetrics>,
    /// Natural maximum ascent.
    pub natural_ascent: i64,
    /// Natural maximum descent.
    pub natural_descent: i64,
    /// Final ascent.
    pub ascent: i64,
    /// Final descent.
    pub descent: i64,
    /// Caller supplied ascent.
    pub ascent_overridden: bool,
    /// Caller supplied descent.
    pub descent_overridden: bool,
}
/// Ordered preserved notices and limited permission assessment.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LicenseReport {
    /// Source part.
    pub part_index: usize,
    /// Component label.
    pub label: String,
    /// Original/supplemental copyright.
    pub copyright: Option<String>,
    /// Original/supplemental license.
    pub license: Option<String>,
    /// Original/supplemental URL.
    pub license_url: Option<String>,
    /// Declared modification status.
    pub modification: ModificationPermission,
    /// Origin of assessment.
    pub provenance: String,
    /// Explicit reserved names.
    pub reserved_names: Vec<String>,
}
/// Complete deterministic report.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MergeReport {
    /// Unicode version.
    pub unicode_version: String,
    /// New family.
    pub family: String,
    /// Requested mode.
    pub requested_spacing: MergeSpacing,
    /// Resolved mode.
    pub effective_spacing: MergeSpacing,
    /// Explicit geometric width policy.
    pub geometric_width: GeometricWidth,
    /// Modal Han width.
    pub full_width: Option<i64>,
    /// Input-order contribution reports.
    pub parts: Vec<PartReport>,
    /// All adjusted/misaligned Western glyphs.
    pub adjustments: Vec<SpacingAdjustment>,
    /// Baseline metric changes.
    pub metrics: MetricsReport,
    /// Input-order notices.
    pub licenses: Vec<LicenseReport>,
    /// Stable shared diagnostics.
    pub diagnostics: Report,
}

fn quote(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
fn text(out: &mut String, key: &str, value: &str) {
    quote(out, key);
    out.push(':');
    quote(out, value);
}
fn opt_text(out: &mut String, key: &str, value: &Option<String>) {
    quote(out, key);
    out.push(':');
    if let Some(value) = value {
        quote(out, value);
    } else {
        out.push_str("null");
    }
}
fn number(out: &mut String, key: &str, value: impl fmt::Display) {
    quote(out, key);
    let _ = write!(out, ":{value}");
}
fn optional(out: &mut String, key: &str, value: Option<i64>) {
    quote(out, key);
    out.push(':');
    if let Some(value) = value {
        let _ = write!(out, "{value}");
    } else {
        out.push_str("null");
    }
}
fn list<T>(out: &mut String, values: &[T], f: impl Fn(&mut String, &T)) {
    out.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        f(out, value);
    }
    out.push(']');
}
impl MergeReport {
    /// Count owned report storage without cloning or serializing it. Inline
    /// fields are included once in the report/array headers; capacities count
    /// spare allocation as well as initialized values.
    pub(super) fn retained_bytes(&self, maximum: usize) -> crate::Result<usize> {
        fn array<T>(budget: &mut super::Budget, values: &Vec<T>) -> crate::Result<()> {
            let bytes = values
                .capacity()
                .checked_mul(std::mem::size_of::<T>())
                .ok_or(crate::Error::ResourceLimit {
                    resource: "merge working bytes",
                    limit: budget.max,
                })?;
            budget.charge(bytes)
        }
        let mut budget = super::Budget {
            used: 0,
            max: maximum,
        };
        budget.charge(std::mem::size_of::<Self>())?;
        budget.charge(self.unicode_version.capacity())?;
        budget.charge(self.family.capacity())?;
        array(&mut budget, &self.parts)?;
        for part in &self.parts {
            budget.charge(part.label.capacity())?;
            budget.charge(part.selection.capacity())?;
            array(&mut budget, &part.missing_ranges)?;
            array(&mut budget, &part.missing_blocks)?;
            for block in &part.missing_blocks {
                budget.charge(block.block.capacity())?;
            }
        }
        array(&mut budget, &self.adjustments)?;
        for adjustment in &self.adjustments {
            array(&mut budget, &adjustment.codepoints)?;
            budget.charge(adjustment.reason.capacity())?;
        }
        array(&mut budget, &self.metrics.parts)?;
        array(&mut budget, &self.licenses)?;
        for license in &self.licenses {
            budget.charge(license.label.capacity())?;
            budget.charge(license.provenance.capacity())?;
            for value in [&license.copyright, &license.license, &license.license_url]
                .into_iter()
                .flatten()
            {
                budget.charge(value.capacity())?;
            }
            array(&mut budget, &license.reserved_names)?;
            for name in &license.reserved_names {
                budget.charge(name.capacity())?;
            }
        }
        array(&mut budget, &self.diagnostics.entries)?;
        for entry in &self.diagnostics.entries {
            budget.charge(entry.code.capacity())?;
            budget.charge(entry.message.capacity())?;
            if let Some(asset) = &entry.location.asset {
                budget.charge(asset.capacity())?;
            }
        }
        Ok(budget.used)
    }

    /// Full deterministic JSON without optional dependencies.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        text(&mut out, "unicode_version", &self.unicode_version);
        out.push(',');
        text(&mut out, "family", &self.family);
        out.push(',');
        text(
            &mut out,
            "requested_spacing",
            self.requested_spacing.as_str(),
        );
        out.push(',');
        text(
            &mut out,
            "effective_spacing",
            self.effective_spacing.as_str(),
        );
        out.push(',');
        optional(&mut out, "full_width", self.full_width);
        out.push(',');
        text(&mut out, "geometric_width", self.geometric_width.as_str());
        out.push_str(",\"parts\":");
        list(&mut out, &self.parts, |o, p| {
            o.push('{');
            number(o, "index", p.index);
            o.push(',');
            text(o, "label", &p.label);
            o.push(',');
            text(o, "selection", &p.selection);
            o.push(',');
            number(o, "dy", p.dy);
            for (k, v) in [
                ("taken_glyphs", p.taken_glyphs),
                ("taken_codepoints", p.taken_codepoints),
                ("taken_sequences", p.taken_sequences),
                ("fallback_codepoints", p.fallback_codepoints),
                ("fallback_sequences", p.fallback_sequences),
                ("omitted_legacy_mappings", p.omitted_legacy_mappings),
                ("omitted_invalid_mappings", p.omitted_invalid_mappings),
                ("omitted_unencoded_glyphs", p.omitted_unencoded_glyphs),
            ] {
                o.push(',');
                number(o, k, v);
            }
            o.push(',');
            number(o, "selected_missing", p.selected_missing);
            o.push_str(",\"missing_ranges\":");
            list(o, &p.missing_ranges, |o, r| {
                o.push('{');
                number(o, "start", r.start);
                o.push(',');
                number(o, "end", r.end);
                o.push('}');
            });
            o.push_str(",\"missing_blocks\":");
            list(o, &p.missing_blocks, |o, b| {
                o.push('{');
                text(o, "block", &b.block);
                o.push(',');
                number(o, "first", b.first);
                o.push(',');
                number(o, "last", b.last);
                o.push(',');
                number(o, "count", b.count);
                o.push('}');
            });
            o.push('}');
        });
        out.push_str(",\"adjustments\":");
        list(&mut out, &self.adjustments, |o, a| {
            o.push('{');
            number(o, "part_index", a.part_index);
            o.push(',');
            number(o, "glyph_index", a.glyph_index);
            o.push_str(",\"codepoints\":");
            list(o, &a.codepoints, |o, c| {
                let _ = write!(o, "{c}");
            });
            o.push(',');
            text(
                o,
                "kind",
                match a.kind {
                    AdjustmentKind::Padded => "padded",
                    AdjustmentKind::Misaligned => "misaligned",
                },
            );
            o.push(',');
            optional(o, "original_advance", a.original_advance);
            o.push(',');
            optional(o, "target_advance", a.target_advance);
            o.push(',');
            number(o, "left_padding", a.left_padding);
            o.push(',');
            number(o, "right_padding", a.right_padding);
            o.push(',');
            text(o, "reason", &a.reason);
            o.push('}');
        });
        let m = &self.metrics;
        out.push_str(",\"metrics\":{");
        number(&mut out, "natural_ascent", m.natural_ascent);
        out.push(',');
        number(&mut out, "natural_descent", m.natural_descent);
        out.push(',');
        number(&mut out, "ascent", m.ascent);
        out.push(',');
        number(&mut out, "descent", m.descent);
        out.push(',');
        number(&mut out, "ascent_overridden", m.ascent_overridden);
        out.push(',');
        number(&mut out, "descent_overridden", m.descent_overridden);
        out.push_str(",\"parts\":");
        list(&mut out, &m.parts, |o, p| {
            o.push('{');
            number(o, "part_index", p.part_index);
            o.push(',');
            optional(o, "original_ascent", p.original_ascent);
            o.push(',');
            optional(o, "original_descent", p.original_descent);
            o.push(',');
            number(o, "shifted_ascent", p.shifted_ascent);
            o.push(',');
            number(o, "shifted_descent", p.shifted_descent);
            o.push('}');
        });
        out.push('}');
        out.push_str(",\"licenses\":");
        list(&mut out, &self.licenses, |o, l| {
            o.push('{');
            number(o, "part_index", l.part_index);
            o.push(',');
            text(o, "label", &l.label);
            o.push(',');
            opt_text(o, "copyright", &l.copyright);
            o.push(',');
            opt_text(o, "license", &l.license);
            o.push(',');
            opt_text(o, "license_url", &l.license_url);
            o.push(',');
            text(o, "modification", l.modification.as_str());
            o.push(',');
            text(o, "provenance", &l.provenance);
            o.push_str(",\"reserved_names\":");
            list(o, &l.reserved_names, |o, s| quote(o, s));
            o.push('}');
        });
        out.push_str(",\"diagnostics\":{\"entries\":");
        list(&mut out, &self.diagnostics.entries, |o, e| {
            o.push('{');
            text(
                o,
                "level",
                match e.level {
                    crate::report::Level::Info => "info",
                    crate::report::Level::Warning => "warning",
                    crate::report::Level::Error => "error",
                },
            );
            o.push(',');
            text(
                o,
                "impact",
                match e.impact {
                    crate::report::Impact::None => "none",
                    crate::report::Impact::Repair => "repair",
                    crate::report::Impact::Loss => "loss",
                },
            );
            o.push(',');
            text(o, "code", &e.code);
            o.push_str(",\"location\":{");
            for (index, (key, v)) in [
                ("font_index", e.location.font_index),
                ("glyph_index", e.location.glyph_index),
                ("codepoint", e.location.codepoint.map(|v| v as usize)),
            ]
            .into_iter()
            .enumerate()
            {
                if index != 0 {
                    o.push(',');
                }
                quote(o, key);
                o.push(':');
                if let Some(v) = v {
                    let _ = write!(o, "{v}");
                } else {
                    o.push_str("null");
                }
            }
            o.push(',');
            opt_text(o, "asset", &e.location.asset);
            o.push('}');
            o.push(',');
            text(o, "message", &e.message);
            o.push('}');
        });
        out.push_str("}}");
        out
    }
}
impl fmt::Display for MergeReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Merged family: {}\nUnicode: {}\nSpacing: {} -> {} (full width: {:?})",
            self.family,
            self.unicode_version,
            self.requested_spacing.as_str(),
            self.effective_spacing.as_str(),
            self.full_width
        )?;
        writeln!(f, "Geometric width: {}", self.geometric_width.as_str())?;
        for p in &self.parts {
            writeln!(
                f,
                "Part {} [{}], take {}, dy {}: {} glyphs, {} scalars, {} sequences; fallback {} scalars/{} sequences; selected missing {}",
                p.index,
                p.label,
                p.selection,
                p.dy,
                p.taken_glyphs,
                p.taken_codepoints,
                p.taken_sequences,
                p.fallback_codepoints,
                p.fallback_sequences,
                p.selected_missing
            )?;
            for b in &p.missing_blocks {
                writeln!(
                    f,
                    "  Missing {}: {} (U+{:04X}..U+{:04X})",
                    b.block, b.count, b.first, b.last
                )?;
            }
            for r in &p.missing_ranges {
                writeln!(f, "  Missing range U+{:04X}..U+{:04X}", r.start, r.end)?;
            }
            writeln!(
                f,
                "  Omitted legacy {}, invalid {}, unencoded {}",
                p.omitted_legacy_mappings, p.omitted_invalid_mappings, p.omitted_unencoded_glyphs
            )?;
        }
        for a in &self.adjustments {
            writeln!(
                f,
                "Spacing {:?}: part {}, glyph {}, codes {:?}; {:?} -> {:?}, padding {}/{}: {}",
                a.kind,
                a.part_index,
                a.glyph_index,
                a.codepoints,
                a.original_advance,
                a.target_advance,
                a.left_padding,
                a.right_padding,
                a.reason
            )?;
        }
        writeln!(
            f,
            "Metrics: ascent {} -> {} (override {}), descent {} -> {} (override {})",
            self.metrics.natural_ascent,
            self.metrics.ascent,
            self.metrics.ascent_overridden,
            self.metrics.natural_descent,
            self.metrics.descent,
            self.metrics.descent_overridden
        )?;
        for p in &self.metrics.parts {
            writeln!(
                f,
                "  Part {}: ascent {:?} -> {}, descent {:?} -> {}",
                p.part_index,
                p.original_ascent,
                p.shifted_ascent,
                p.original_descent,
                p.shifted_descent
            )?;
        }
        for l in &self.licenses {
            writeln!(
                f,
                "License part {} [{}]: {} ({})\n  Copyright: {}\n  License: {}\n  URL: {}\n  Reserved names: {}",
                l.part_index,
                l.label,
                l.modification.as_str(),
                l.provenance,
                l.copyright.as_deref().unwrap_or("unknown"),
                l.license.as_deref().unwrap_or("unknown"),
                l.license_url.as_deref().unwrap_or("none"),
                l.reserved_names.join(", ")
            )?;
        }
        write!(f, "{}", self.diagnostics)
    }
}
