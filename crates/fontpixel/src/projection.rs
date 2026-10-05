//! Explicit, transactional model projections applied between reading and writing.
//!
//! A projection changes values a target application expects (for example a
//! catalogue that derives BDF `SWIDTH` from the device advance) without
//! altering normal conversion defaults. Every changed value is returned in a
//! complete per-glyph audit; overwriting an effective source value is reported
//! as [`Impact::Loss`] so strict mode still rejects it, while deriving a value
//! that was absent everywhere is a [`Impact::Repair`].
//!
//! The current slice covers horizontal scalable advances. Font names, size and
//! property edits are separate future options and are not implied here.
//!
//! ```
//! use fontpixel::model::{Font, FontSet, Glyph, Vector};
//! use fontpixel::projection::{BitmapProjection, apply_bitmap_projection};
//!
//! let mut glyph = Glyph::new("#".parse::<fontpixel::Bitmap>()?);
//! glyph.metrics.advance = Some(Vector::new(6, 0));
//! glyph.metrics.scalable_advance = Some(Vector::new(500, 0));
//! let mut set = FontSet::from(Font::from_glyphs([glyph])?);
//!
//! // A 13px catalogue convention: SWIDTH = round_half_even(6 * 1000 / 13).
//! let projection = BitmapProjection::scalable_from_device(1000, 13);
//! let outcome = apply_bitmap_projection(&mut set, &projection, 1 << 20)?;
//! let (_, glyph) = set.fonts[0].glyphs().next().unwrap();
//! assert_eq!(glyph.metrics.scalable_advance, Some(Vector::new(462, 0)));
//! assert_eq!(outcome.scalable_advance_changes.len(), 1);
//! assert!(outcome.report.has_loss()); // strict mode rejects the replacement
//! # Ok::<(), fontpixel::Error>(())
//! ```
use crate::model::{FontSet, GlyphId, Vector};
use crate::report::{Impact, Level, Location, Report, ReportEntry};
use crate::{Error, Result};
use std::mem::size_of;

/// How horizontal scalable advances (BDF `SWIDTH` x) are produced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScalableAdvance {
    /// Keep every source value unchanged (the default).
    #[default]
    Preserve,
    /// Derive `round_half_even(device_x * units_per_em / reference_ppem)` for
    /// every live glyph, including unencoded and duplicate-code records.
    /// Vertical components and all other metrics are kept.
    FromDevice {
        /// Scalable units per em; BDF uses 1000.
        units_per_em: u32,
        /// Pixels per em the device advance is measured at; must be positive.
        reference_ppem: u32,
    },
}

/// Typed projection options. The default changes nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BitmapProjection {
    /// Horizontal scalable-advance policy.
    pub scalable_advance: ScalableAdvance,
}
impl BitmapProjection {
    /// Options deriving scalable advances from device advances.
    pub const fn scalable_from_device(units_per_em: u32, reference_ppem: u32) -> Self {
        Self {
            scalable_advance: ScalableAdvance::FromDevice {
                units_per_em,
                reference_ppem,
            },
        }
    }
}

/// One committed scalable-advance change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalableAdvanceChange {
    /// Strike index in the projected set.
    pub font_index: usize,
    /// Stable glyph ID within that strike.
    pub glyph: GlyphId,
    /// Glyph-level source value (`None` when absent on the glyph).
    pub glyph_value: Option<Vector>,
    /// Effective source value, falling back to the strike's glyph defaults.
    pub effective_before: Option<Vector>,
    /// Device advance x the new value was derived from.
    pub device_advance: i64,
    /// New glyph-level value.
    pub after: Vector,
}

/// Complete result of a committed projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectionOutcome {
    /// Aggregated diagnostics, one entry per strike and impact class.
    pub report: Report,
    /// Every changed scalable advance, in strike then glyph-ID order.
    pub scalable_advance_changes: Vec<ScalableAdvanceChange>,
}
impl ProjectionOutcome {
    /// Bytes retained by this outcome's own allocations (capacities included),
    /// for callers that keep it alive while writing and serializing reports.
    pub fn retained_bytes(&self) -> usize {
        let entries = self.report.entries.capacity() * size_of::<ReportEntry>();
        let strings: usize = self
            .report
            .entries
            .iter()
            .map(|e| {
                e.code.capacity()
                    + e.message.capacity()
                    + e.location.asset.as_ref().map_or(0, String::capacity)
            })
            .sum();
        let changes = self.scalable_advance_changes.capacity() * size_of::<ScalableAdvanceChange>();
        entries + strings + changes
    }
}

fn limit_error(limit: usize) -> Error {
    Error::ResourceLimit {
        resource: "projection working bytes",
        limit,
    }
}

/// `round_half_even(value * units / ppem)` in exact integer arithmetic.
fn scaled_half_even(value: i64, units: u32, ppem: u32) -> Result<i64> {
    let numerator = i128::from(value) * i128::from(units);
    let denominator = i128::from(ppem);
    let mut quotient = numerator.div_euclid(denominator);
    let twice_remainder = 2 * numerator.rem_euclid(denominator);
    if twice_remainder > denominator || (twice_remainder == denominator && quotient % 2 != 0) {
        quotient += 1;
    }
    i64::try_from(quotient).map_err(|_| {
        Error::InvalidInput(format!(
            "scalable advance {value}*{units}/{ppem} exceeds the metric range"
        ))
    })
}

/// Apply `projection` to every strike as one transaction.
///
/// All glyphs are validated and every change is staged before the first
/// commit; on any error `set` is unchanged. `max_working_bytes` bounds the
/// staged changes and diagnostics, which become the returned outcome; callers
/// charge [`ProjectionOutcome::retained_bytes`] to their pipeline ledger.
pub fn apply_bitmap_projection(
    set: &mut FontSet,
    projection: &BitmapProjection,
    max_working_bytes: usize,
) -> Result<ProjectionOutcome> {
    let ScalableAdvance::FromDevice {
        units_per_em,
        reference_ppem,
    } = projection.scalable_advance
    else {
        return Ok(ProjectionOutcome::default());
    };
    if reference_ppem == 0 {
        return Err(Error::InvalidInput(
            "scalable-advance projection requires a positive reference ppem".into(),
        ));
    }
    if units_per_em == 0 {
        return Err(Error::InvalidInput(
            "scalable-advance projection requires positive units per em".into(),
        ));
    }

    // Pass 1: exact staged count, so storage is admitted before allocation.
    let mut count = 0usize;
    for font in &set.fonts {
        let defaults = &font.metrics.glyph_defaults;
        for (_, glyph) in font.glyphs() {
            let Some(device) = glyph.metrics.advance.or(defaults.advance) else {
                continue;
            };
            let after_x = scaled_half_even(device.x, units_per_em, reference_ppem)?;
            let before = glyph.metrics.scalable_advance.or(defaults.scalable_advance);
            let y = before.map_or(0, |v| v.y);
            if before != Some(Vector::new(after_x, y)) {
                count += 1;
            }
        }
    }
    let change_bytes = count
        .checked_mul(size_of::<ScalableAdvanceChange>())
        .ok_or_else(|| limit_error(max_working_bytes))?;
    // At most three aggregate entries (loss, repair, missing device advance) per strike.
    let entry_capacity = set
        .fonts
        .len()
        .checked_mul(3)
        .ok_or_else(|| limit_error(max_working_bytes))?;
    const MESSAGE_BOUND: usize = 192;
    let report_bytes = entry_capacity
        .checked_mul(size_of::<ReportEntry>() + MESSAGE_BOUND + 48)
        .ok_or_else(|| limit_error(max_working_bytes))?;
    if change_bytes
        .checked_add(report_bytes)
        .is_none_or(|total| total > max_working_bytes)
    {
        return Err(limit_error(max_working_bytes));
    }
    let mut changes = Vec::new();
    changes
        .try_reserve_exact(count)
        .map_err(|_| limit_error(max_working_bytes))?;
    let mut report = Report::default();
    report
        .entries
        .try_reserve_exact(entry_capacity)
        .map_err(|_| limit_error(max_working_bytes))?;

    // Pass 2: stage changes and diagnostics without touching the model.
    for (font_index, font) in set.fonts.iter().enumerate() {
        let defaults = &font.metrics.glyph_defaults;
        let (mut losses, mut repairs, mut missing) = (0usize, 0usize, 0usize);
        for (id, glyph) in font.glyphs() {
            let Some(device) = glyph.metrics.advance.or(defaults.advance) else {
                missing += 1;
                continue;
            };
            let after_x = scaled_half_even(device.x, units_per_em, reference_ppem)?;
            let before = glyph.metrics.scalable_advance.or(defaults.scalable_advance);
            let after = Vector::new(after_x, before.map_or(0, |v| v.y));
            if before == Some(after) {
                continue;
            }
            if before.is_some() {
                losses += 1;
            } else {
                repairs += 1;
            }
            changes.push(ScalableAdvanceChange {
                font_index,
                glyph: id,
                glyph_value: glyph.metrics.scalable_advance,
                effective_before: before,
                device_advance: device.x,
                after,
            });
        }
        let location = Location {
            font_index: Some(font_index),
            ..Location::default()
        };
        let entry = |level, impact, code: &str, message: String| ReportEntry {
            level,
            impact,
            code: code.into(),
            location: location.clone(),
            message,
        };
        if losses > 0 {
            report.push(entry(
                Level::Warning,
                Impact::Loss,
                "projection.scalable_advance.replaced",
                format!(
                    "{losses} source scalable advance(s) replaced by device advance * \
                     {units_per_em} / {reference_ppem} (half-even)"
                ),
            ));
        }
        if repairs > 0 {
            report.push(entry(
                Level::Info,
                Impact::Repair,
                "projection.scalable_advance.derived",
                format!(
                    "{repairs} absent scalable advance(s) derived from device advance * \
                     {units_per_em} / {reference_ppem} (half-even)"
                ),
            ));
        }
        if missing > 0 {
            report.push(entry(
                Level::Warning,
                Impact::None,
                "projection.scalable_advance.no_device_advance",
                format!("{missing} glyph(s) have no device advance; scalable advance kept"),
            ));
        }
    }

    // Commit: no allocation and no fallible step; IDs were validated above
    // under the same exclusive borrow.
    for change in &changes {
        set.fonts[change.font_index]
            .set_scalable_advance_prevalidated(change.glyph, Some(change.after));
    }
    Ok(ProjectionOutcome {
        report,
        scalable_advance_changes: changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Font, Glyph, GlyphMetrics};

    fn glyph(advance: Option<i64>, scalable: Option<i64>) -> Glyph {
        let mut g = Glyph::new(crate::bitmap::Bitmap::<crate::bitmap::Mono>::new(1, 1).unwrap());
        g.metrics.advance = advance.map(|x| Vector::new(x, 0));
        g.metrics.scalable_advance = scalable.map(|x| Vector::new(x, 0));
        g
    }

    #[test]
    fn half_even_matches_python_round() {
        assert_eq!(scaled_half_even(6, 1000, 13).unwrap(), 462);
        assert_eq!(scaled_half_even(10, 1000, 18).unwrap(), 556);
        assert_eq!(scaled_half_even(1, 1, 2).unwrap(), 0);
        assert_eq!(scaled_half_even(-1, 1, 2).unwrap(), 0);
        assert_eq!(scaled_half_even(3, 1, 2).unwrap(), 2);
        assert_eq!(scaled_half_even(-3, 1, 2).unwrap(), -2);
        assert_eq!(scaled_half_even(5, 1, 2).unwrap(), 2);
        assert_eq!(scaled_half_even(0, 1000, 7).unwrap(), 0);
        assert_eq!(scaled_half_even(-6, 1000, 13).unwrap(), -462);
        assert!(scaled_half_even(i64::MAX, u32::MAX, 1).is_err());
    }

    #[test]
    fn rejects_nonpositive_parameters_without_change() {
        let mut set = FontSet::from(Font::from_glyphs([glyph(Some(6), Some(500))]).unwrap());
        let before = set.clone();
        assert!(
            apply_bitmap_projection(
                &mut set,
                &BitmapProjection::scalable_from_device(1000, 0),
                usize::MAX
            )
            .is_err()
        );
        assert!(
            apply_bitmap_projection(
                &mut set,
                &BitmapProjection::scalable_from_device(0, 13),
                usize::MAX
            )
            .is_err()
        );
        assert_eq!(set, before);
    }

    #[test]
    fn default_preserves_everything() {
        let mut set = FontSet::from(Font::from_glyphs([glyph(Some(6), Some(500))]).unwrap());
        let before = set.clone();
        let outcome = apply_bitmap_projection(&mut set, &BitmapProjection::default(), 0).unwrap();
        assert_eq!(outcome, ProjectionOutcome::default());
        assert_eq!(set, before);
    }

    #[test]
    fn projects_all_physical_records_with_full_audit() {
        // Encoded, duplicate-code, and unencoded records, plus one already exact.
        let mut a = glyph(Some(6), Some(500));
        a.metrics.vertical_scalable_advance = Some(Vector::new(0, -1000));
        a.codepoints = vec![crate::model::Codepoint::Unicode(0x41)];
        let mut dup = glyph(Some(6), Some(500));
        dup.codepoints = vec![crate::model::Codepoint::Unicode(0x41)];
        let unencoded = glyph(Some(12), None);
        let exact = glyph(Some(6), Some(462));
        let font = Font::from_glyphs([a, dup, unencoded, exact]).unwrap();
        let mut set = FontSet::from(font);
        let outcome = apply_bitmap_projection(
            &mut set,
            &BitmapProjection::scalable_from_device(1000, 13),
            usize::MAX,
        )
        .unwrap();
        assert_eq!(outcome.scalable_advance_changes.len(), 3);
        let font = &set.fonts[0];
        let values: Vec<_> = font
            .glyphs()
            .map(|(_, g)| g.metrics.scalable_advance.unwrap().x)
            .collect();
        assert_eq!(values, [462, 462, 923, 462]);
        assert_eq!(
            font.glyphs()
                .next()
                .unwrap()
                .1
                .metrics
                .vertical_scalable_advance,
            Some(Vector::new(0, -1000))
        );
        assert_eq!(
            font.glyph_ids_for_codepoint(crate::model::Codepoint::Unicode(0x41))
                .len(),
            2
        );
        let impacts: Vec<_> = outcome.report.entries.iter().map(|e| e.impact).collect();
        assert_eq!(impacts, [Impact::Loss, Impact::Repair]);
        assert!(outcome.report.has_loss());
        assert_eq!(outcome.scalable_advance_changes[2].effective_before, None);
        assert!(outcome.retained_bytes() > 0);
    }

    #[test]
    fn inherits_defaults_and_reports_missing_device_advance() {
        let mut font = Font::from_glyphs([glyph(None, None), glyph(Some(6), None)]).unwrap();
        font.metrics.glyph_defaults = GlyphMetrics {
            scalable_advance: Some(Vector::new(462, 0)),
            ..GlyphMetrics::default()
        };
        let mut set = FontSet::from(font);
        let outcome = apply_bitmap_projection(
            &mut set,
            &BitmapProjection::scalable_from_device(1000, 13),
            usize::MAX,
        )
        .unwrap();
        // Glyph 2 inherits exactly 462: no change; glyph 1 has no device advance.
        assert!(outcome.scalable_advance_changes.is_empty());
        assert_eq!(outcome.report.entries.len(), 1);
        assert_eq!(
            outcome.report.entries[0].code,
            "projection.scalable_advance.no_device_advance"
        );
        assert!(
            set.fonts[0]
                .glyphs()
                .all(|(_, g)| g.metrics.scalable_advance.is_none())
        );
    }

    #[test]
    fn failure_in_a_later_strike_leaves_every_strike_unchanged() {
        let ok = Font::from_glyphs([glyph(Some(6), Some(500))]).unwrap();
        let overflow = Font::from_glyphs([glyph(Some(i64::MAX), None)]).unwrap();
        let mut set = FontSet {
            fonts: vec![ok, overflow],
            ..FontSet::default()
        };
        let before = set.clone();
        assert!(
            apply_bitmap_projection(
                &mut set,
                &BitmapProjection::scalable_from_device(1000, 1),
                usize::MAX
            )
            .is_err()
        );
        assert_eq!(set, before);
        // Working-byte admission failure is also atomic.
        let err = apply_bitmap_projection(
            &mut FontSet::from(Font::from_glyphs([glyph(Some(6), Some(500))]).unwrap()),
            &BitmapProjection::scalable_from_device(1000, 13),
            16,
        );
        assert!(matches!(err, Err(Error::ResourceLimit { .. })));
    }
}
