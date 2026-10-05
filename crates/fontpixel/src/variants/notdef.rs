//! Explicit missing-glyph (`.notdef`, glyph 0) policy for outline export.
//!
//! The default keeps the existing behaviour: a source glyph named `.notdef`
//! when the export retains source glyphs, otherwise the generated hollow box.
//! [`NotdefPolicy::Empty`] instead writes an empty outline whose horizontal
//! metrics are given directly in font design units, because an application's
//! half-em advance (for example 494 units at 76 units per pixel) is often not a
//! whole number of pixels. Replacing a source `.notdef` glyph this way is
//! reported as loss; replacing the generated box is not, since it carries no
//! source information.
use crate::model::{Glyph, Vector};
use crate::{Bitmap, Error, Result};

/// Glyph 0 of an outline export.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NotdefPolicy {
    /// Source `.notdef` when retained, otherwise the generated box (default).
    #[default]
    Preserve,
    /// Empty outline with exact design-unit horizontal metrics.
    Empty {
        /// hmtx advance width in design units.
        advance_width: u16,
        /// hmtx left side bearing in design units.
        left_side_bearing: i16,
    },
}
impl NotdefPolicy {
    /// Exact `(advance, left side bearing)` of an empty `.notdef`, if selected.
    pub const fn empty_metrics(self) -> Option<(u16, i16)> {
        match self {
            Self::Preserve => None,
            Self::Empty {
                advance_width,
                left_side_bearing,
            } => Some((advance_width, left_side_bearing)),
        }
    }
}

/// The empty glyph used for [`NotdefPolicy::Empty`]. Its pixel advance only
/// matters for an embedded bitmap strike, which needs a whole pixel count.
pub(super) fn empty_glyph(
    advance_width: u16,
    units_per_pixel: u16,
    embed_bitmaps: bool,
) -> Result<Glyph> {
    let scale = u32::from(units_per_pixel.max(1));
    let width = u32::from(advance_width);
    if embed_bitmaps && width % scale != 0 {
        return Err(Error::InvalidInput(format!(
            "variants: empty .notdef advance {advance_width} is not a whole number of {units_per_pixel}-unit pixels, so it cannot be embedded in a bitmap strike"
        )));
    }
    let mut glyph = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
    glyph.metrics.advance = Some(Vector::new(i64::from(width / scale), 0));
    glyph.name = Some(".notdef".into());
    Ok(glyph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preserves_and_empty_reports_exact_metrics() {
        assert_eq!(NotdefPolicy::default(), NotdefPolicy::Preserve);
        assert_eq!(NotdefPolicy::Preserve.empty_metrics(), None);
        let empty = NotdefPolicy::Empty {
            advance_width: 494,
            left_side_bearing: 0,
        };
        assert_eq!(empty.empty_metrics(), Some((494, 0)));
    }

    #[test]
    fn embedded_strike_requires_whole_pixel_advance() {
        assert!(empty_glyph(494, 76, true).is_err());
        let glyph = empty_glyph(494, 76, false).unwrap();
        assert_eq!(glyph.bitmap.width(), 0);
        assert_eq!(glyph.metrics.advance, Some(Vector::new(6, 0)));
        let glyph = empty_glyph(456, 76, true).unwrap();
        assert_eq!(glyph.metrics.advance, Some(Vector::new(6, 0)));
    }
}
