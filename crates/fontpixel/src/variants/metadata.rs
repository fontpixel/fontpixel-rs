//! Explicit global table controls for outline export.
//!
//! Every control is opt-in: [`MetadataOptions::default`] reproduces the native
//! tables byte for byte. No value is inferred from family names, vendors or
//! application conventions; callers state each field. None of these controls
//! changes an outline, an advance or a mapped code point.
use super::OutlineFlavor;
use crate::outline::Shape;
use crate::sfnt::{cmap::FullRecords, os2};
use crate::{Error, Result};

/// Whether the export writes a `gasp` table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaspPolicy {
    /// One range up to ppem 65535 with no grid-fitting or smoothing request
    /// (native default).
    #[default]
    Write,
    /// No `gasp` table; rasterizers apply their own defaults.
    Omit,
}
/// Glyph ID order of the planned glyphs (glyph 0 is always `.notdef`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphOrder {
    /// The plan's order (native default: sorted by glyph name).
    #[default]
    Planned,
    /// Glyphs mapped by the default cmap in ascending order of their smallest
    /// Unicode scalar, then unmapped (alternate) glyphs in planned order.
    /// cmap and GSUB glyph IDs follow the new order.
    Codepoint,
}
/// Glyph ID of the shared dot glyph of TrueType dot outlines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DotGlyph {
    /// After every planned glyph (native default).
    #[default]
    Last,
    /// Glyph 1, directly after `.notdef`; every other glyph ID moves up by one.
    AfterNotdef,
}
/// Explicit head, OS/2, post, maxp, gasp, cmap and glyph-order controls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetadataOptions {
    /// `head.created` and `head.modified`, seconds since 1904-01-01 00:00 UTC.
    /// Absent writes zero (deterministic native default).
    pub timestamps: Option<[i64; 2]>,
    /// OS/2 `fsType` embedding permissions (native default 0).
    pub embedding_flags: Option<u16>,
    /// OS/2 `fsSelection` (native default 0x40, regular). The table is version
    /// 3, so bits 7..=9 are rejected.
    pub selection_flags: Option<u16>,
    /// OS/2 `usBreakChar` (native default 0).
    pub break_char: Option<u16>,
    /// Exact OS/2 `ulCodePageRange1..2` instead of the cmap-derived bits.
    pub code_page_ranges: Option<[u32; 2]>,
    /// `post.isFixedPitch` override; absent follows the source spacing.
    pub fixed_pitch: Option<bool>,
    /// `maxp.maxZones` (1 or 2) of TrueType output (native default 1: no
    /// instructions use the twilight zone).
    pub max_zones: Option<u16>,
    /// `gasp` table policy.
    pub gasp: GaspPolicy,
    /// Which cmap records carry the format-12 subtable.
    pub cmap_full_records: FullRecords,
    /// Glyph ID order.
    pub glyph_order: GlyphOrder,
    /// Position of the shared dot glyph.
    pub dot_glyph: DotGlyph,
}
fn invalid(message: &str) -> Error {
    Error::InvalidInput(format!("variants: {message}"))
}
impl MetadataOptions {
    /// Reject reserved values and controls that the chosen output cannot carry.
    pub fn validate(&self, flavor: OutlineFlavor, shape: Shape) -> Result<()> {
        if let Some(flags) = self.embedding_flags {
            os2::check_embedding_flags(flags).map_err(invalid)?;
        }
        if let Some(flags) = self.selection_flags {
            os2::check_selection_flags(flags, 3).map_err(invalid)?;
        }
        if let Some(ranges) = self.code_page_ranges {
            os2::check_code_page_ranges(ranges).map_err(invalid)?;
        }
        if let Some(zones) = self.max_zones {
            if !(1..=2).contains(&zones) {
                return Err(invalid("maxZones must be 1 or 2"));
            }
            if flavor != OutlineFlavor::TrueType {
                return Err(invalid("maxZones requires TrueType outlines"));
            }
        }
        if self.dot_glyph == DotGlyph::AfterNotdef
            && (flavor != OutlineFlavor::TrueType || shape != Shape::Dot)
        {
            return Err(invalid(
                "the dot glyph position requires TrueType dot outlines",
            ));
        }
        Ok(())
    }
    /// Whether any control differs from the native default.
    pub(super) fn requested(&self) -> bool {
        self != &Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_reserved_and_unsupported_values() {
        let ok = MetadataOptions::default();
        assert!(ok.validate(OutlineFlavor::Cff, Shape::Square).is_ok());
        assert!(!ok.requested());
        for bad in [
            MetadataOptions {
                embedding_flags: Some(1),
                ..Default::default()
            },
            MetadataOptions {
                embedding_flags: Some(6),
                ..Default::default()
            },
            MetadataOptions {
                selection_flags: Some(0x0400),
                ..Default::default()
            },
            MetadataOptions {
                selection_flags: Some(0x0080),
                ..Default::default()
            },
            MetadataOptions {
                code_page_ranges: Some([1 << 9, 0]),
                ..Default::default()
            },
            MetadataOptions {
                code_page_ranges: Some([0, 1]),
                ..Default::default()
            },
            MetadataOptions {
                max_zones: Some(3),
                ..Default::default()
            },
        ] {
            assert!(bad.validate(OutlineFlavor::TrueType, Shape::Dot).is_err());
        }
        let zones = MetadataOptions {
            max_zones: Some(2),
            ..Default::default()
        };
        assert!(
            zones
                .validate(OutlineFlavor::TrueType, Shape::Square)
                .is_ok()
        );
        assert!(zones.validate(OutlineFlavor::Cff, Shape::Square).is_err());
        let dot = MetadataOptions {
            dot_glyph: DotGlyph::AfterNotdef,
            ..Default::default()
        };
        assert!(dot.validate(OutlineFlavor::TrueType, Shape::Dot).is_ok());
        assert!(
            dot.validate(OutlineFlavor::TrueType, Shape::Square)
                .is_err()
        );
        assert!(dot.validate(OutlineFlavor::Cff, Shape::Dot).is_err());
        let fs = MetadataOptions {
            embedding_flags: Some(4),
            selection_flags: Some(0),
            code_page_ranges: Some([0x2000_019f, 0]),
            ..Default::default()
        };
        assert!(fs.validate(OutlineFlavor::TrueType, Shape::Square).is_ok());
    }
}
