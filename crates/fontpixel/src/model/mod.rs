//! Loss-preserving font models with explicit strike, encoding and bitmap types.
//!
//! File order, duplicate mappings, unencoded glyphs and original property spelling
//! are retained. The last glyph mapped to a code wins ordinary lookup, while
//! [`Font::glyph_ids_for_codepoint`] exposes all duplicates for diagnostics.
//! Editing the private glyph table goes through validated, index-updating methods.
//!
//! ```
//! use fontpixel::{Bitmap, Font, FontSet, Glyph};
//! use fontpixel::model::Codepoint;
//! let mut font = Font::default();
//! let mut glyph = Glyph::new(".#.\n###".parse::<Bitmap>()?);
//! glyph.codepoints.push(Codepoint::Unicode(65));
//! let id = font.add_glyph(glyph)?;
//! assert_eq!(font.glyph_id(Codepoint::Unicode(65)), Some(id));
//! assert_eq!(FontSet::from(font).into_single()?.len(), 1);
//! # Ok::<(), fontpixel::Error>(())
//! ```

use crate::bitmap::{Bitmap, Bounds, Gray8, Mono};
use crate::charset::{CharsetCodec, CharsetError, canonical_charset, sanitize_charset};
use crate::{Error, ResourceLimits, Result};
use std::collections::BTreeMap;

mod properties;
pub use properties::{Properties, Property, PropertyValue};
pub(crate) mod budget;
pub use budget::ModelUsage;

/// A Unicode number or a code in the owning font's explicitly identified charset.
/// Unicode-tagged raw source numbers, including surrogates and values above
/// U+10FFFF, are retained for tolerant BDF/SPEC parity. Use `scalar()` to validate
/// scalar interpretation; writers must report unsupported codes rather than drop them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum Codepoint {
    /// Number declared to use Unicode, possibly invalid in a malformed source.
    Unicode(u32),
    /// Charset-specific code; not implicitly interpreted as Unicode.
    Legacy(u32),
}
impl Codepoint {
    /// The numeric code, without changing its encoding domain.
    pub const fn value(self) -> u32 {
        match self {
            Self::Unicode(v) | Self::Legacy(v) => v,
        }
    }
    /// Interpret a Unicode-tagged value as a Rust scalar, returning None for
    /// surrogates, out-of-range source numbers and charset-specific codes.
    pub fn scalar(self) -> Option<char> {
        match self {
            Self::Unicode(value) => char::from_u32(value),
            Self::Legacy(_) => None,
        }
    }
}

/// A signed two-dimensional font-space vector (y up).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    serde(deny_unknown_fields)
)]
pub struct Vector {
    /// Horizontal component.
    pub x: i64,
    /// Vertical component.
    pub y: i64,
}
impl Vector {
    /// Construct a font-space vector.
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// Exact rational metric, avoiding float rounding and preserving equality.
/// Unreduced fractions are allowed so the source's scale remains expressible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    serde(try_from = "RationalRaw")
)]
pub struct Rational {
    numerator: i64,
    denominator: u32,
}
/// Unvalidated serde shape of [`Rational`]; conversion rejects a zero denominator.
#[cfg(any(feature = "serde", feature = "yaff", feature = "image"))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RationalRaw {
    numerator: i64,
    denominator: u32,
}
#[cfg(any(feature = "serde", feature = "yaff", feature = "image"))]
impl TryFrom<RationalRaw> for Rational {
    type Error = Error;
    fn try_from(raw: RationalRaw) -> Result<Self> {
        Self::new(raw.numerator, raw.denominator)
    }
}
impl Rational {
    /// Construct an exact fraction; a zero denominator is invalid.
    pub fn new(numerator: i64, denominator: u32) -> Result<Self> {
        if denominator == 0 {
            return Err(Error::InvalidInput(
                "metric denominator must be nonzero".into(),
            ));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
    /// Construct an integer-valued metric.
    pub const fn integer(value: i64) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }
    /// Signed numerator.
    pub const fn numerator(self) -> i64 {
        self.numerator
    }
    /// Positive denominator.
    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

/// FNT-style A/B/C widths, independently preserved even if redundant with BBX.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    serde(deny_unknown_fields)
)]
pub struct AbcWidths {
    /// Signed left space.
    pub a: i64,
    /// Black-body width.
    pub b: i64,
    /// Signed right space.
    pub c: i64,
}

/// Glyph metrics. `None` means absent, distinct from an explicit zero vector.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    serde(deny_unknown_fields)
)]
pub struct GlyphMetrics {
    /// Ink rectangle dimensions must match the bitmap; offsets are relative to origin.
    pub bounds: Bounds,
    /// Horizontal device advance (BDF DWIDTH).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub advance: Option<Vector>,
    /// Horizontal scalable advance (BDF SWIDTH, normally per 1000 units).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub scalable_advance: Option<Vector>,
    /// Vertical device advance (BDF DWIDTH1).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub vertical_advance: Option<Vector>,
    /// Vertical scalable advance (BDF SWIDTH1).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub vertical_scalable_advance: Option<Vector>,
    /// BDF VVECTOR (vector from horizontal to vertical origin).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub vertical_origin: Option<Vector>,
    /// Optional independent FNT ABC spacing.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub abc: Option<AbcWidths>,
    /// Other formats' left side bearing, when explicitly supplied.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub left_bearing: Option<i64>,
    /// Other formats' right side bearing, when explicitly supplied.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub right_bearing: Option<i64>,
    /// Top-left image-relative offset (e.g. BMFont); not confused with y-up BBX.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub image_offset: Option<Vector>,
}

/// Runtime bitmap depth for a glyph. Conversions must be requested explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GlyphBitmap {
    /// Compact monochrome raster.
    Mono(Bitmap<Mono>),
    /// Byte-per-pixel grayscale raster.
    Gray8(Bitmap<Gray8>),
}
impl Default for GlyphBitmap {
    fn default() -> Self {
        Self::Mono(Bitmap::default())
    }
}
impl From<Bitmap<Mono>> for GlyphBitmap {
    fn from(bitmap: Bitmap<Mono>) -> Self {
        Self::Mono(bitmap)
    }
}
impl From<Bitmap<Gray8>> for GlyphBitmap {
    fn from(bitmap: Bitmap<Gray8>) -> Self {
        Self::Gray8(bitmap)
    }
}
impl GlyphBitmap {
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        match self {
            Self::Mono(b) => b.width(),
            Self::Gray8(b) => b.width(),
        }
    }
    /// Height in pixels.
    pub fn height(&self) -> u32 {
        match self {
            Self::Mono(b) => b.height(),
            Self::Gray8(b) => b.height(),
        }
    }
    /// Stored canonical bytes; interpretation follows the enum variant.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Mono(b) => b.as_bytes(),
            Self::Gray8(b) => b.as_bytes(),
        }
    }
    /// Nonzero ink bounds in bottom-left coordinates.
    pub fn bbox(&self) -> Bounds {
        match self {
            Self::Mono(b) => b.bbox(),
            Self::Gray8(b) => b.bbox(),
        }
    }
    /// Crop/extend without changing depth or implicitly thresholding.
    pub fn crop(&self, bounds: Bounds) -> Result<Self> {
        match self {
            Self::Mono(b) => Ok(b.crop_bounds(bounds)?.into()),
            Self::Gray8(b) => Ok(b.crop_bounds(bounds)?.into()),
        }
    }
}

/// One glyph, including unencoded glyphs, multiple mappings and Unicode sequences.
/// Extra source attributes retain insertion order, spelling, duplicates and raw data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Glyph {
    /// Zero, one or several encodings in their original order.
    pub codepoints: Vec<Codepoint>,
    /// Unicode sequences (e.g. PSF combining entries), distinct from aliases.
    pub unicode_sequences: Vec<Vec<u32>>,
    /// Original STARTCHAR/PostScript/format-specific name, if present.
    pub name: Option<String>,
    /// Horizontal, vertical and optional source-specific metrics.
    pub metrics: GlyphMetrics,
    /// Only the source BBX rectangle, not a whole-font cell.
    pub bitmap: GlyphBitmap,
    /// Original per-glyph fields, including unencoded alternate codes if needed.
    pub properties: Properties,
}
impl Glyph {
    /// Construct a glyph with this bitmap's dimensions and zero origin offset.
    pub fn new(bitmap: impl Into<GlyphBitmap>) -> Self {
        let bitmap = bitmap.into();
        let metrics = GlyphMetrics {
            bounds: Bounds::new(bitmap.width(), bitmap.height(), 0, 0),
            ..GlyphMetrics::default()
        };
        Self {
            bitmap,
            metrics,
            ..Self::default()
        }
    }
    /// Validate dimensions and Unicode sequences without rejecting
    /// representable raw metadata. An empty Unicode sequence is not a mapping.
    pub fn validate(&self) -> Result<()> {
        if self.metrics.bounds.width != self.bitmap.width()
            || self.metrics.bounds.height != self.bitmap.height()
        {
            return Err(Error::InvalidInput(
                "glyph BBX dimensions differ from bitmap".into(),
            ));
        }
        self.metrics
            .bounds
            .x
            .checked_add(i64::from(self.metrics.bounds.width))
            .ok_or_else(|| Error::InvalidInput("glyph BBX x overflows".into()))?;
        self.metrics
            .bounds
            .y
            .checked_add(i64::from(self.metrics.bounds.height))
            .ok_or_else(|| Error::InvalidInput("glyph BBX y overflows".into()))?;
        for sequence in &self.unicode_sequences {
            if sequence.is_empty() {
                return Err(Error::InvalidInput("empty Unicode glyph sequence".into()));
            }
            for &code in sequence {
                if code > 0x10ffff {
                    return Err(Error::InvalidInput(
                        "Unicode sequence value exceeds U+10FFFF".into(),
                    ));
                }
            }
        }
        Ok(())
    }
    /// Shift the glyph origin-relative BBX, leaving advance/scalable metrics alone.
    pub fn shift(&mut self, dx: i64, dy: i64) -> Result<()> {
        let x = self
            .metrics
            .bounds
            .x
            .checked_add(dx)
            .ok_or_else(|| Error::InvalidInput("glyph x offset overflows".into()))?;
        let y = self
            .metrics
            .bounds
            .y
            .checked_add(dy)
            .ok_or_else(|| Error::InvalidInput("glyph y offset overflows".into()))?;
        x.checked_add(i64::from(self.bitmap.width()))
            .ok_or_else(|| Error::InvalidInput("glyph right edge overflows".into()))?;
        y.checked_add(i64::from(self.bitmap.height()))
            .ok_or_else(|| Error::InvalidInput("glyph top edge overflows".into()))?;
        self.metrics.bounds.x = x;
        self.metrics.bounds.y = y;
        Ok(())
    }
    /// Get a whole-cell raster whose bottom-left is `cell.x,cell.y` relative to
    /// the same glyph origin. Negative bearings and below-baseline ink are kept.
    pub fn bitmap_in_cell(&self, cell: Bounds) -> Result<GlyphBitmap> {
        self.validate()?;
        let x = cell
            .x
            .checked_sub(self.metrics.bounds.x)
            .ok_or_else(|| Error::InvalidInput("cell x offset overflows".into()))?;
        let y = cell
            .y
            .checked_sub(self.metrics.bounds.y)
            .ok_or_else(|| Error::InvalidInput("cell y offset overflows".into()))?;
        self.bitmap.crop(Bounds::new(cell.width, cell.height, x, y))
    }
    /// Set bitmap and bottom-left origin-relative location; optionally trim zero
    /// margins. Advances are unchanged. A trimmed empty bitmap gets a zero BBX.
    pub fn set_bitmap(
        &mut self,
        bitmap: impl Into<GlyphBitmap>,
        x: i64,
        y: i64,
        trim: bool,
    ) -> Result<()> {
        let mut bitmap = bitmap.into();
        let mut bounds = Bounds::new(bitmap.width(), bitmap.height(), x, y);
        if trim {
            let ink = bitmap.bbox();
            bitmap = bitmap.crop(ink)?;
            bounds = if ink.width == 0 {
                Bounds::default()
            } else {
                Bounds::new(
                    ink.width,
                    ink.height,
                    x.checked_add(ink.x)
                        .ok_or_else(|| Error::InvalidInput("trimmed glyph x overflows".into()))?,
                    y.checked_add(ink.y)
                        .ok_or_else(|| Error::InvalidInput("trimmed glyph y overflows".into()))?,
                )
            };
        }
        bounds
            .x
            .checked_add(i64::from(bounds.width))
            .ok_or_else(|| Error::InvalidInput("glyph BBX right edge overflows".into()))?;
        bounds
            .y
            .checked_add(i64::from(bounds.height))
            .ok_or_else(|| Error::InvalidInput("glyph BBX top edge overflows".into()))?;
        self.bitmap = bitmap;
        self.metrics.bounds = bounds;
        Ok(())
    }
}

/// Spacing classification (BDF P/M/C), without inventing a default when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum Spacing {
    /// Proportional advances.
    Proportional,
    /// Monospaced advances.
    Monospaced,
    /// Character-cell font.
    CharacterCell,
}

/// Stable glyph-table index, assigned in original file order.
/// Removing glyphs leaves holes rather than silently retargeting saved IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct GlyphId(usize);
impl GlyphId {
    /// Numerical table index. Use `Font::glyph` to check whether it is still live.
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Per-strike metrics; absence is distinct from a specified value of zero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    serde(deny_unknown_fields)
)]
pub struct FontMetrics {
    /// Nominal pixel size.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub pixel_size: Option<u32>,
    /// Horizontal pixels per em.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub ppem_x: Option<u32>,
    /// Vertical pixels per em.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub ppem_y: Option<u32>,
    /// Exact typographic point size.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub point_size: Option<Rational>,
    /// Horizontal/vertical dots per inch.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub resolution: Option<Vector>,
    /// Baseline-to-top distance.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub ascent: Option<i64>,
    /// Baseline-to-bottom distance (usually positive).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub descent: Option<i64>,
    /// Font cell/ink bounds, exactly as supplied by the source.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub bounds: Option<Bounds>,
    /// Additional line gap.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub line_gap: Option<i64>,
    /// Explicit baseline-to-baseline distance, independent of line gap.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub line_height: Option<i64>,
    /// Underline position relative to baseline.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub underline_position: Option<i64>,
    /// Underline thickness.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub underline_thickness: Option<i64>,
    /// Default glyph ID in this font (if separately specified).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub default_glyph: Option<GlyphId>,
    /// Default code in its explicit encoding domain.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub default_character: Option<Codepoint>,
    /// Space advance if separately specified.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub space_width: Option<i64>,
    /// P/M/C spacing classification.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub spacing: Option<Spacing>,
    /// Font-level BDF defaults, inherited by codecs when individual metrics are absent.
    pub glyph_defaults: GlyphMetrics,
    /// BDF METRICSSET (0, 1 or 2); unknown source values remain in raw headers.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub metrics_set: Option<u8>,
}

/// Names and human-readable metadata, without discarding their original properties.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    serde(deny_unknown_fields)
)]
pub struct FontNames {
    /// Family name.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub family: Option<String>,
    /// Style/subfamily name.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub style: Option<String>,
    /// Full display name.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub full: Option<String>,
    /// PostScript name.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub postscript: Option<String>,
    /// Version string.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub version: Option<String>,
    /// Copyright statement.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub copyright: Option<String>,
    /// License text or identifier.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub license: Option<String>,
    /// License URL if separate from the text.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub license_url: Option<String>,
    /// Foundry/vendor name.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub foundry: Option<String>,
    /// Original BDF FONT/XLFD name (not regenerated from normalized names).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub xlfd: Option<String>,
    /// Parsed XLFD fields in source order, retaining empty fields.
    pub xlfd_fields: Vec<String>,
    /// Additional language/platform-specific names in source order.
    pub localized: Properties,
}

/// Charset identity and source metadata. Glyph legacy codes belong to this charset;
/// Unicode aliases are independently tagged and never implicitly remapped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    any(feature = "serde", feature = "yaff", feature = "image"),
    serde(deny_unknown_fields)
)]
pub struct FontCharset {
    /// Original CHARSET_REGISTRY spelling/value.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub registry: Option<String>,
    /// Original CHARSET_ENCODING spelling/value.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub encoding: Option<String>,
    /// Optional explicit codec override (canonicalization performed by CharsetCodec).
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub name: Option<String>,
    /// Explicit GL interpretation when the source establishes it.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub gl: Option<bool>,
    /// Original Windows/FNT character-set byte, if present.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub windows_charset: Option<u8>,
    /// Explicit source code page when supplied.
    #[cfg_attr(
        any(feature = "serde", feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub code_page: Option<u32>,
    /// Original HBF/other code ranges, inclusive and in source order.
    pub code_ranges: Vec<(u32, u32)>,
}
impl FontCharset {
    /// Canonical name using the existing exact charset module; raw fields stay intact.
    pub fn canonical_name(&self) -> String {
        self.name.as_ref().map_or_else(
            || sanitize_charset(self.registry.as_deref(), self.encoding.as_deref()),
            |name| canonical_charset(name),
        )
    }
    /// Open the exact mapping. A disabled CJK table remains an explicit failure;
    /// unknown names remain distinguishable by `codec.known()` for caller reporting.
    pub fn codec(&self) -> std::result::Result<CharsetCodec, CharsetError> {
        CharsetCodec::new(&self.canonical_name(), self.gl)
    }
}

/// A single size/style strike with file-order glyphs and an indexed cmap.
/// Private storage prevents stale indexes after edits; clone preserves stable IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Font {
    glyphs: Vec<Option<Glyph>>,
    cmap: BTreeMap<Codepoint, Vec<GlyphId>>,
    sequence_map: BTreeMap<Vec<u32>, Vec<GlyphId>>,
    live: usize,
    /// Global strike metrics.
    pub metrics: FontMetrics,
    /// Normalized names plus retained original XLFD.
    pub names: FontNames,
    /// Explicit source charset identity.
    pub charset: FontCharset,
    /// Original format identifier when known.
    pub source_format: Option<String>,
    /// Original ordered header fields, including comments and unknown fields.
    pub headers: Properties,
    /// Original ordered font properties, including duplicates and unknown fields.
    pub properties: Properties,
}
impl Font {
    /// Build an indexed strike from glyphs in file order. No mapping collision is
    /// silently deleted: lookups use the last mapping and all duplicates survive.
    pub fn from_glyphs(glyphs: impl IntoIterator<Item = Glyph>) -> Result<Self> {
        let mut font = Self::default();
        for glyph in glyphs {
            font.add_glyph(glyph)?;
        }
        Ok(font)
    }
    /// Number of allocated glyph IDs, including holes left by removed glyphs.
    /// Codecs use this to preserve stable IDs or report compaction explicitly.
    pub const fn slot_count(&self) -> usize {
        self.glyphs.len()
    }
    /// Number of live glyphs, including unencoded glyphs and duplicate codes.
    pub const fn len(&self) -> usize {
        self.live
    }
    /// Whether this strike has no live glyphs.
    pub const fn is_empty(&self) -> bool {
        self.live == 0
    }
    /// Glyphs in original insertion/file order, without removed slots.
    pub fn glyphs(&self) -> impl Iterator<Item = (GlyphId, &Glyph)> {
        self.glyphs
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.as_ref().map(|g| (GlyphId(i), g)))
    }
    /// Look up a stable glyph ID (returns None if it was removed).
    pub fn glyph(&self, id: GlyphId) -> Option<&Glyph> {
        self.glyphs.get(id.0)?.as_ref()
    }
    /// Fast O(log n) lookup of the last file-order mapping for a code.
    pub fn glyph_id(&self, codepoint: Codepoint) -> Option<GlyphId> {
        self.cmap.get(&codepoint)?.last().copied()
    }
    /// Look up a glyph directly by code; never treats an unencoded glyph as encoded.
    pub fn glyph_for_codepoint(&self, codepoint: Codepoint) -> Option<&Glyph> {
        self.glyph(self.glyph_id(codepoint)?)
    }
    /// All mappings of a code, in file order, for loss/duplicate diagnostics.
    pub fn glyph_ids_for_codepoint(&self, codepoint: Codepoint) -> &[GlyphId] {
        self.cmap.get(&codepoint).map_or(&[], Vec::as_slice)
    }
    /// Iterate distinct codes in domain/numeric order with effective glyph IDs.
    pub fn codepoints(&self) -> impl DoubleEndedIterator<Item = (Codepoint, GlyphId)> + '_ {
        self.cmap
            .iter()
            .filter_map(|(&cp, ids)| ids.last().copied().map(|id| (cp, id)))
    }
    /// Ordered, inclusive range of codes in the same or different encoding domains.
    /// Reversed ranges return empty instead of panicking in BTreeMap::range.
    pub fn codepoint_range(
        &self,
        start: Codepoint,
        end: Codepoint,
    ) -> impl Iterator<Item = (Codepoint, GlyphId)> + '_ {
        let range = if start <= end {
            Some(self.cmap.range(start..=end))
        } else {
            None
        };
        range
            .into_iter()
            .flatten()
            .filter_map(|(&cp, ids)| ids.last().copied().map(|id| (cp, id)))
    }
    /// Lookup a whole Unicode sequence (not a list of independent aliases).
    pub fn glyph_for_sequence(&self, sequence: &[u32]) -> Option<&Glyph> {
        self.glyph(*self.sequence_map.get(sequence)?.last()?)
    }
    /// Append a validated glyph, retaining duplicates and all source metadata.
    pub fn add_glyph(&mut self, glyph: Glyph) -> Result<GlyphId> {
        glyph.validate()?;
        self.glyphs
            .try_reserve(1)
            .map_err(|_| Error::ResourceLimit {
                resource: "glyph allocation",
                limit: self.glyphs.len(),
            })?;
        let id = GlyphId(self.glyphs.len());
        self.index_glyph(id, &glyph);
        self.glyphs.push(Some(glyph));
        self.live += 1;
        Ok(id)
    }
    fn index_glyph(&mut self, id: GlyphId, glyph: &Glyph) {
        for &cp in &glyph.codepoints {
            let ids = self.cmap.entry(cp).or_default();
            if let Err(index) = ids.binary_search(&id) {
                ids.insert(index, id);
            }
        }
        for sequence in &glyph.unicode_sequences {
            let ids = self.sequence_map.entry(sequence.clone()).or_default();
            if let Err(index) = ids.binary_search(&id) {
                ids.insert(index, id);
            }
        }
    }
    fn unindex_glyph(&mut self, id: GlyphId, glyph: &Glyph) {
        for cp in &glyph.codepoints {
            if let Some(ids) = self.cmap.get_mut(cp) {
                ids.retain(|&i| i != id);
                if ids.is_empty() {
                    self.cmap.remove(cp);
                }
            }
        }
        for sequence in &glyph.unicode_sequences {
            if let Some(ids) = self.sequence_map.get_mut(sequence) {
                ids.retain(|&i| i != id);
                if ids.is_empty() {
                    self.sequence_map.remove(sequence);
                }
            }
        }
    }
    /// Replace a glyph at its existing file-order position. Validation happens
    /// before changing anything; changed codes/sequences update both indexes.
    pub fn replace_glyph(&mut self, id: GlyphId, glyph: Glyph) -> Result<Glyph> {
        glyph.validate()?;
        let old = self
            .glyphs
            .get_mut(id.0)
            .and_then(Option::take)
            .ok_or_else(|| Error::InvalidInput("unknown or removed glyph ID".into()))?;
        self.unindex_glyph(id, &old);
        self.index_glyph(id, &glyph);
        self.glyphs[id.0] = Some(glyph);
        Ok(old)
    }
    /// Transactionally edit a glyph clone, then validate and update indexes.
    /// An error returned by the closure or validation leaves the font unchanged.
    pub fn edit_glyph(
        &mut self,
        id: GlyphId,
        edit: impl FnOnce(&mut Glyph) -> Result<()>,
    ) -> Result<()> {
        let mut glyph = self
            .glyph(id)
            .ok_or_else(|| Error::InvalidInput("unknown or removed glyph ID".into()))?
            .clone();
        edit(&mut glyph)?;
        self.replace_glyph(id, glyph)?;
        Ok(())
    }
    /// Commit an already staged scalable-advance edit without allocating or
    /// touching glyph indexes. The projection transaction must validate every
    /// ID while exclusively borrowing the FontSet and must not remove glyphs
    /// between validation and this infallible commit phase.
    pub(crate) fn set_scalable_advance_prevalidated(&mut self, id: GlyphId, value: Option<Vector>) {
        self.glyphs[id.0]
            .as_mut()
            .expect("projection prevalidated live glyph")
            .metrics
            .scalable_advance = value;
    }
    /// Remove a glyph without renumbering other IDs. Default-glyph reference is
    /// cleared if it points here; the original default-character code is retained.
    pub fn remove_glyph(&mut self, id: GlyphId) -> Option<Glyph> {
        let old = self.glyphs.get_mut(id.0)?.take()?;
        self.unindex_glyph(id, &old);
        self.live -= 1;
        if self.metrics.default_glyph == Some(id) {
            self.metrics.default_glyph = None;
        }
        Some(old)
    }
    /// Font-cell aligned bitmap for a glyph. Missing font bounds are an error.
    pub fn glyph_in_cell(&self, id: GlyphId) -> Result<GlyphBitmap> {
        let cell = self
            .metrics
            .bounds
            .ok_or_else(|| Error::InvalidInput("font has no cell bounds".into()))?;
        self.glyph(id)
            .ok_or_else(|| Error::InvalidInput("unknown or removed glyph ID".into()))?
            .bitmap_in_cell(cell)
    }
    /// Recompute union of nonzero-area glyph rectangles (including unencoded).
    /// If none exist, leave the source's font bounds unchanged, as SPEC requires.
    pub fn update_bounds(&mut self) -> Result<()> {
        let mut union: Option<(i64, i64, i64, i64)> = None;
        for (_, glyph) in self.glyphs() {
            glyph.validate()?;
            let b = glyph.metrics.bounds;
            if b.width == 0 || b.height == 0 {
                continue;
            }
            let r = b.x + i64::from(b.width);
            let t = b.y + i64::from(b.height);
            union = Some(match union {
                None => (b.x, b.y, r, t),
                Some((x, y, right, top)) => (x.min(b.x), y.min(b.y), right.max(r), top.max(t)),
            });
        }
        if let Some((x, y, r, t)) = union {
            let w = u32::try_from(i128::from(r) - i128::from(x))
                .map_err(|_| Error::InvalidInput("font union width overflows".into()))?;
            let h = u32::try_from(i128::from(t) - i128::from(y))
                .map_err(|_| Error::InvalidInput("font union height overflows".into()))?;
            self.metrics.bounds = Some(Bounds::new(w, h, x, y));
        }
        Ok(())
    }
    /// Validate references, glyph consistency and per-glyph/model resource budgets.
    /// Codecs must additionally enforce these limits before allocation while parsing.
    pub fn validate(&self, limits: &ResourceLimits) -> Result<()> {
        if self.len() > limits.max_glyphs {
            return Err(Error::ResourceLimit {
                resource: "glyph count",
                limit: limits.max_glyphs,
            });
        }
        if self
            .metrics
            .default_glyph
            .is_some_and(|id| self.glyph(id).is_none())
        {
            return Err(Error::InvalidInput(
                "default glyph refers to a removed or nonexistent glyph".into(),
            ));
        }
        let mut bytes = 0usize;
        for (_, glyph) in self.glyphs() {
            glyph.validate()?;
            if u64::from(glyph.bitmap.width()) * u64::from(glyph.bitmap.height())
                > limits.max_pixels_per_glyph as u64
            {
                return Err(Error::ResourceLimit {
                    resource: "glyph pixels",
                    limit: limits.max_pixels_per_glyph,
                });
            }
            bytes =
                bytes
                    .checked_add(glyph.bitmap.as_bytes().len())
                    .ok_or(Error::ResourceLimit {
                        resource: "model bitmap bytes",
                        limit: limits.max_total_bytes,
                    })?;
            if bytes > limits.max_total_bytes {
                return Err(Error::ResourceLimit {
                    resource: "model bitmap bytes",
                    limit: limits.max_total_bytes,
                });
            }
        }
        Ok(())
    }
}

/// All strikes/styles in one logical input, in source file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct FontSet {
    /// Ordered strikes. No single-strike format may silently select one.
    pub fonts: Vec<Font>,
    /// Original container metadata in file order.
    pub properties: Properties,
    /// Container/source format identifier, if known.
    pub source_format: Option<String>,
}
impl From<Font> for FontSet {
    fn from(font: Font) -> Self {
        Self {
            fonts: vec![font],
            ..Self::default()
        }
    }
}
impl FontSet {
    /// Borrow the sole strike; empty and multi-strike sets fail explicitly.
    pub fn single(&self) -> Result<&Font> {
        if self.fonts.len() != 1 {
            return Err(Error::InvalidInput(format!(
                "expected one font, found {}",
                self.fonts.len()
            )));
        }
        Ok(&self.fonts[0])
    }
    /// Extract the sole strike; never silently drops additional strikes.
    pub fn into_single(mut self) -> Result<Font> {
        if self.fonts.len() != 1 {
            return Err(Error::InvalidInput(format!(
                "expected one font, found {}",
                self.fonts.len()
            )));
        }
        self.fonts
            .pop()
            .ok_or_else(|| Error::InvalidInput("expected one font".into()))
    }
    /// Validate each strike plus aggregate glyph count/bitmap-byte budgets.
    pub fn validate(&self, limits: &ResourceLimits) -> Result<()> {
        let mut count = 0usize;
        let mut bytes = 0usize;
        for font in &self.fonts {
            font.validate(limits)?;
            count = count.checked_add(font.len()).ok_or(Error::ResourceLimit {
                resource: "glyph count",
                limit: limits.max_glyphs,
            })?;
            for (_, glyph) in font.glyphs() {
                bytes = bytes.checked_add(glyph.bitmap.as_bytes().len()).ok_or(
                    Error::ResourceLimit {
                        resource: "model bitmap bytes",
                        limit: limits.max_total_bytes,
                    },
                )?;
            }
            if count > limits.max_glyphs {
                return Err(Error::ResourceLimit {
                    resource: "glyph count",
                    limit: limits.max_glyphs,
                });
            }
            if bytes > limits.max_total_bytes {
                return Err(Error::ResourceLimit {
                    resource: "model bitmap bytes",
                    limit: limits.max_total_bytes,
                });
            }
        }
        Ok(())
    }
}

/// Serde form of [`Font`]: `glyphs` lists every glyph slot in ID order, with
/// `null` for a removed slot, so stable [`GlyphId`]s survive a round trip. The
/// code and sequence indexes are rebuilt rather than serialized. Each glyph is
/// validated as by [`Font::add_glyph`], and a `default_glyph` must name a live
/// slot.
#[cfg(feature = "serde")]
mod font_serde {
    use super::{Font, FontCharset, FontMetrics, FontNames, Glyph, Properties};
    use crate::{Error, Result};

    #[derive(serde::Serialize)]
    struct FontRef<'a> {
        glyphs: &'a [Option<Glyph>],
        metrics: &'a FontMetrics,
        names: &'a FontNames,
        charset: &'a FontCharset,
        source_format: &'a Option<String>,
        headers: &'a Properties,
        properties: &'a Properties,
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FontRaw {
        glyphs: Vec<Option<Glyph>>,
        metrics: FontMetrics,
        names: FontNames,
        charset: FontCharset,
        source_format: Option<String>,
        headers: Properties,
        properties: Properties,
    }

    impl serde::Serialize for Font {
        fn serialize<S: serde::Serializer>(
            &self,
            serializer: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            FontRef {
                glyphs: &self.glyphs,
                metrics: &self.metrics,
                names: &self.names,
                charset: &self.charset,
                source_format: &self.source_format,
                headers: &self.headers,
                properties: &self.properties,
            }
            .serialize(serializer)
        }
    }

    impl TryFrom<FontRaw> for Font {
        type Error = Error;
        fn try_from(raw: FontRaw) -> Result<Self> {
            let mut font = Self::default();
            for slot in raw.glyphs {
                match slot {
                    Some(glyph) => {
                        font.add_glyph(glyph)?;
                    }
                    None => {
                        let id = font.add_glyph(Glyph::default())?;
                        font.remove_glyph(id);
                    }
                }
            }
            if raw
                .metrics
                .default_glyph
                .is_some_and(|id| font.glyph(id).is_none())
            {
                return Err(Error::InvalidInput(
                    "default glyph refers to a removed or nonexistent glyph".into(),
                ));
            }
            font.metrics = raw.metrics;
            font.names = raw.names;
            font.charset = raw.charset;
            font.source_format = raw.source_format;
            font.headers = raw.headers;
            font.properties = raw.properties;
            Ok(font)
        }
    }

    impl<'de> serde::Deserialize<'de> for Font {
        fn deserialize<D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            let raw = FontRaw::deserialize(deserializer)?;
            Self::try_from(raw).map_err(serde::de::Error::custom)
        }
    }
}

impl crate::formats::Loaded<FontSet> {
    /// Extract the sole strike while retaining format and read diagnostics.
    pub fn into_single(self) -> Result<crate::formats::Loaded<Font>> {
        Ok(crate::formats::Loaded {
            fonts: self.fonts.into_single()?,
            report: self.report,
            format: self.format,
        })
    }
}
