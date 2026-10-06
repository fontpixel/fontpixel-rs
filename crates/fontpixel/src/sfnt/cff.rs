//! Bounded CFF1 Type 2 outlines, deterministic encoding and shared cubic dots.
//!
//! This is an outline-table API, not a bitmap FontSet codec. See
//! `docs/formats/cff.md` for supported programs, numeric bounds and loss policy.
use super::working::{Budget, Lease, Owned};
use super::{bounded, bytes, invalid, to16, u16_at};
use crate::{Bitmap, Result, outline};
use read_fonts::{
    FontData,
    ps::{
        cff::{CffFontRef, Subfont, index::Index},
        cs::CommandSink,
    },
    types::{Fixed, GlyphId},
};
mod read;
mod subset;
mod validate;
mod write;
pub use read::Cff;
pub(crate) use write::Encoder;
#[cfg(test)]
pub(crate) use write::program_len;

/// CFF parsing, geometry and execution budgets.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct Limits {
    /// Shared sfnt byte/glyph budgets.
    pub sfnt: super::Limits,
    /// Bitmap-to-path geometry budgets.
    pub geometry: outline::Limits,
    /// Maximum commands in one expanded glyph.
    pub max_commands: usize,
    /// Maximum commands across a whole-font read or write.
    pub max_total_commands: usize,
    /// Maximum bytes in one DICT.
    pub max_dict_bytes: usize,
    /// Maximum DICT bytes visited across structural parsing (including aliases).
    pub max_total_dict_bytes: usize,
    /// Maximum executed program bytes per glyph, counting repeated subroutines.
    pub max_program_bytes: usize,
    /// Maximum executed program bytes across a whole-font read/write.
    pub max_total_program_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sfnt: super::Limits::default(),
            geometry: outline::Limits::default(),
            max_commands: 1024 * 1024,
            max_total_commands: 16 * 1024 * 1024,
            max_dict_bytes: 65536,
            max_total_dict_bytes: 4 * 1024 * 1024,
            max_program_bytes: 2 * 1024 * 1024,
            max_total_program_bytes: 256 * 1024 * 1024,
        }
    }
}
/// An absolute point in upward-facing design coordinates.
pub type Point = (f64, f64);
/// Absolute path commands. CFF contours close implicitly on move/endchar.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Command {
    /// Begin a contour.
    MoveTo(Point),
    /// Draw a straight segment.
    LineTo(Point),
    /// Draw a cubic Bézier using two controls and an endpoint.
    CurveTo(Point, Point, Point),
    /// Close the current contour.
    Close,
}
/// Writer geometry; dots reference one font-wide global subroutine.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Outline {
    /// Explicit closed paths; empty is a valid blank glyph.
    Path(Vec<Command>),
    /// Lower-left cell origins in design units.
    Dots(Vec<Point>),
}
/// One named glyph for canonical CFF output.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Glyph {
    /// Unique PostScript glyph name; first glyph must be `.notdef`.
    pub name: String,
    /// Nonnegative advance in design units, representable in Type 2 16.16.
    pub advance: f64,
    /// Explicit geometry or shared-dot positions.
    pub outline: Outline,
}
impl Glyph {
    /// Convert W2.2 bitmap geometry; Dot uses this font's shared cubic, not a quadratic.
    /// For Dot, set the matching cell/diameter in the writer's `DotOptions`.
    pub fn from_bitmap(
        name: impl Into<String>,
        advance: f64,
        bitmap: &Bitmap,
        options: &outline::Options,
        limits: &Limits,
    ) -> Result<Self> {
        let budget = super::working::Budget::new(usize::MAX, "outline working bytes");
        let name = name.into();
        let lease = budget.reserve(name.capacity())?;
        let name = super::working::Owned::from_parts(name, lease);
        Ok(
            Self::from_bitmap_with_budget(name, advance, bitmap, options, limits, &budget)?
                .into_parts()
                .0,
        )
    }
    /// As [`Glyph::from_bitmap`], admitting geometry and path commands on
    /// `budget` before allocation. `name` carries its own admitted storage.
    pub(crate) fn from_bitmap_with_budget(
        name: super::working::Owned<String>,
        advance: f64,
        bitmap: &Bitmap,
        options: &outline::Options,
        limits: &Limits,
        budget: &super::working::Budget,
    ) -> Result<super::working::Owned<Self>> {
        let (name, mut lease) = name.into_parts();
        let shape = if options.shape == outline::Shape::Dot {
            let (origins, mut origins_lease) =
                outline::pixel_origins_with_budget(bitmap, options, &limits.geometry, budget)?
                    .into_parts();
            lease.merge(&mut origins_lease)?;
            Outline::Dots(origins)
        } else {
            let cs = outline::contours_with_budget(bitmap, options, &limits.geometry, budget)?;
            // Preserve the incremental command checks, then admit the exact path.
            let mut length = 0usize;
            for c in cs.iter().filter(|c| !c.is_empty()) {
                bounded(
                    length.saturating_add(c.len()).saturating_add(1),
                    limits.max_commands,
                    "CFF commands",
                )?;
                length += c.len() + 1;
            }
            bounded(length, limits.max_commands, "CFF commands")?;
            let mut path = super::working::Vector::with_capacity(budget, length)?;
            for c in cs.iter() {
                if let Some(p) = c.first() {
                    path.push(Command::MoveTo((p.0, p.1)))?;
                    for p in &c[1..] {
                        path.push(Command::LineTo((p.0, p.1)))?;
                    }
                    path.push(Command::Close)?;
                }
            }
            drop(cs);
            let (path, mut path_lease) = path.into_parts();
            lease.merge(&mut path_lease)?;
            Outline::Path(path)
        };
        Ok(super::working::Owned::from_parts(
            Self {
                name,
                advance,
                outline: shape,
            },
            lease,
        ))
    }
}
/// Shared circle configuration, in design units.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DotOptions {
    /// Positive size of a pixel cell.
    pub cell_size: f64,
    /// Circle diameter, positive and at most 1.5 times the cell size.
    pub diameter: f64,
}
impl Default for DotOptions {
    fn default() -> Self {
        Self {
            cell_size: 100.,
            diameter: 90.,
        }
    }
}
/// A canonical, unhinted, name-keyed CFF1 font.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Font {
    /// PostScript font name.
    pub postscript_name: String,
    /// Full display name, stored as a CFF string.
    pub full_name: String,
    /// OpenType units per em in 16..=16384.
    pub units_per_em: u16,
    /// Stable glyph order including `.notdef` first.
    pub glyphs: Vec<Glyph>,
    /// Shared circle used by all `Outline::Dots` glyphs.
    pub dots: DotOptions,
}
/// Encoded CFF table plus matching sfnt fields.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Encoded {
    /// Bytes for the `CFF ` table.
    pub data: Vec<u8>,
    /// Outward-rounded geometric bounds for `head` and FontBBox.
    pub bounds: [i16; 4],
    /// Six-byte maxp version 0.5 table.
    pub maxp: Vec<u8>,
}
impl Font {
    /// Encode deterministically; unsupported or unrepresentable values are errors.
    pub fn encode(&self, limits: &Limits) -> Result<Encoded> {
        write::encode(self, limits)
    }
}
/// One evaluated unhinted glyph, with fractions retained.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct DecodedGlyph {
    /// Advance from Type 2/Private DICT, transformed to design space.
    pub advance: f64,
    /// Expanded path. Hint instructions are not applied.
    pub commands: Vec<Command>,
}
fn ps_error(e: impl std::fmt::Display) -> crate::Error {
    invalid(format!("CFF: {e}"))
}
fn add_budget(total: &mut usize, amount: usize, limit: usize, what: &'static str) -> Result<()> {
    *total = total
        .checked_add(amount)
        .ok_or_else(|| invalid("CFF budget overflow"))?;
    bounded(*total, limit, what)
}
fn coordinate(x: f64) -> Result<()> {
    if !x.is_finite() || !(-32768.0..32768.0).contains(&x) {
        Err(invalid("CFF coordinate/operand outside signed 16.16"))
    } else {
        Ok(())
    }
}
