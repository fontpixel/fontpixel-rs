//! Explicit, transactional model projections applied between reading and writing.
//!
//! A projection changes values a target application expects (for example a
//! catalogue that derives BDF `SWIDTH` from the device advance, writes its own
//! `FONT` name and a fixed `SIZE`) without altering normal conversion defaults.
//! Every changed value is returned in a complete typed audit; overwriting or
//! removing an effective source value is reported as [`Impact::Loss`] so strict
//! mode still rejects it, while adding a value that was absent is an
//! [`Impact::Repair`]. Equal values are not changes and produce no diagnostics.
//!
//! The options are:
//!
//! - [`ScalableAdvance`]: horizontal scalable advances from device advances;
//! - `font_name`: the complete `FONT`/XLFD name, keeping the decomposed XLFD
//!   fields and any existing `FONT` property in sync;
//! - [`SizeProjection`]: point size and resolution (BDF `SIZE`);
//! - [`PropertyEdit`]: a bounded list of typed property sets/removals. Comments,
//!   notice/copyright/licence properties, internal provenance records and
//!   properties owned by normalized names or charsets cannot be edited here.
//!
//! The whole projection is one transaction over every strike: all options are
//! validated, the working storage is admitted against the caller's budget and
//! every change is staged before the model is touched; the commit only moves
//! staged values into place.
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
//!
//! Metadata options combine with the same transaction:
//!
//! ```
//! use fontpixel::model::{Font, FontSet, Rational, Vector};
//! use fontpixel::projection::{
//!     BitmapProjection, EditValue, PropertyEdit, SizeProjection, apply_bitmap_projection,
//! };
//!
//! let mut set = FontSet::from(Font::default());
//! let edits = [PropertyEdit::Set { name: "PIXEL_SIZE", value: EditValue::Integer(13) }];
//! let mut projection = BitmapProjection::default();
//! projection.font_name = Some("-opf-demo-medium-r-normal--13-130-75-75-p-130-iso10646-1");
//! projection.size = Some(SizeProjection {
//!     point_size: Rational::integer(13),
//!     resolution: Vector::new(75, 75),
//! });
//! projection.properties = &edits;
//! let outcome = apply_bitmap_projection(&mut set, &projection, 1 << 20)?;
//! let font = &set.fonts[0];
//! assert_eq!(font.names.xlfd_fields[6], "13");
//! assert_eq!(font.metrics.pixel_size, Some(13));
//! // Every value was absent, so each change is an explicit repair, not a loss.
//! assert!(!outcome.report.has_loss());
//! assert_eq!(outcome.metadata_changes.len(), 5);
//! # Ok::<(), fontpixel::Error>(())
//! ```
use crate::model::{Font, FontSet, GlyphId, Property, PropertyValue, Rational, Vector};
use crate::report::{Impact, Level, Location, Report, ReportEntry};
use crate::{Error, Result};
use std::mem::size_of;

/// Maximum number of [`PropertyEdit`]s in one projection.
pub const MAX_PROPERTY_EDITS: usize = 64;
/// Maximum bytes in an edited property name.
pub const MAX_PROPERTY_NAME_BYTES: usize = 255;
/// Maximum bytes in an edited text property value.
pub const MAX_PROPERTY_TEXT_BYTES: usize = 65_535;
/// Maximum bytes in a projected `FONT` name.
pub const MAX_FONT_NAME_BYTES: usize = 4096;

/// How horizontal scalable advances (BDF `SWIDTH` x) are produced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "snake_case")
)]
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

/// Explicit point size and resolution (BDF `SIZE`). Both values are replaced
/// together; each differing component is audited separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SizeProjection {
    /// Positive typographic point size. BDF output requires an integer.
    pub point_size: Rational,
    /// Positive horizontal and vertical dots per inch.
    pub resolution: Vector,
}

/// A typed property value for [`PropertyEdit::Set`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditValue<'a> {
    /// Integer within the signed/unsigned 32-bit range shared by BDF and PCF.
    Integer(i64),
    /// Single-line text without NUL.
    Text(&'a str),
}
impl EditValue<'_> {
    fn property(self) -> PropertyValue {
        match self {
            Self::Integer(value) => PropertyValue::Integer(value),
            Self::Text(text) => PropertyValue::Text(text.to_owned()),
        }
    }
    fn matches(self, value: &PropertyValue) -> bool {
        match (self, value) {
            (Self::Integer(a), PropertyValue::Integer(b)) => a == *b,
            (Self::Integer(a), PropertyValue::Unsigned(b)) => u64::try_from(a) == Ok(*b),
            (Self::Text(a), PropertyValue::Text(b)) => a == b,
            _ => false,
        }
    }
    fn heap_bytes(self) -> usize {
        match self {
            Self::Integer(_) => 0,
            Self::Text(text) => text.len(),
        }
    }
}

/// One explicit, bounded property edit. Names compare ASCII-case-insensitively,
/// as BDF properties do; an edit never touches a different property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyEdit<'a> {
    /// Replace the last record of `name` in place (keeping its spelling and
    /// position) or append a new record with this spelling.
    Set {
        /// Property name.
        name: &'a str,
        /// New value.
        value: EditValue<'a>,
    },
    /// Remove every record of `name`.
    Remove {
        /// Property name.
        name: &'a str,
    },
}
impl<'a> PropertyEdit<'a> {
    /// The edited property name.
    pub const fn name(&self) -> &'a str {
        match self {
            Self::Set { name, .. } | Self::Remove { name } => name,
        }
    }
}

/// Typed projection options. The default changes nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BitmapProjection<'a> {
    /// Horizontal scalable-advance policy.
    pub scalable_advance: ScalableAdvance,
    /// Complete `FONT`/XLFD name.
    pub font_name: Option<&'a str>,
    /// Point size and resolution.
    pub size: Option<SizeProjection>,
    /// Bounded typed property edits, applied in order after `font_name`.
    pub properties: &'a [PropertyEdit<'a>],
}
impl BitmapProjection<'_> {
    /// Options deriving scalable advances from device advances.
    pub const fn scalable_from_device(units_per_em: u32, reference_ppem: u32) -> Self {
        Self {
            scalable_advance: ScalableAdvance::FromDevice {
                units_per_em,
                reference_ppem,
            },
            font_name: None,
            size: None,
            properties: &[],
        }
    }
    /// Whether these options request no change at all.
    pub fn is_identity(&self) -> bool {
        self.scalable_advance == ScalableAdvance::Preserve
            && self.font_name.is_none()
            && self.size.is_none()
            && self.properties.is_empty()
    }
}

/// One committed scalable-advance change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
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

/// Normalized integer metric kept in sync with an edited property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SyncedMetric {
    /// `PIXEL_SIZE` and `FontMetrics::pixel_size`.
    PixelSize,
    /// `FONT_ASCENT` and `FontMetrics::ascent`.
    Ascent,
    /// `FONT_DESCENT` and `FontMetrics::descent`.
    Descent,
    /// `UNDERLINE_POSITION` and `FontMetrics::underline_position`.
    UnderlinePosition,
    /// `UNDERLINE_THICKNESS` and `FontMetrics::underline_thickness`.
    UnderlineThickness,
    /// `SPACE_WIDTH` and `FontMetrics::space_width`.
    SpaceWidth,
    /// `LINE_GAP` and `FontMetrics::line_gap`.
    LineGap,
    /// `LINE_HEIGHT` and `FontMetrics::line_height`.
    LineHeight,
}
const SYNCED: [(&str, SyncedMetric); 8] = [
    ("PIXEL_SIZE", SyncedMetric::PixelSize),
    ("FONT_ASCENT", SyncedMetric::Ascent),
    ("FONT_DESCENT", SyncedMetric::Descent),
    ("UNDERLINE_POSITION", SyncedMetric::UnderlinePosition),
    ("UNDERLINE_THICKNESS", SyncedMetric::UnderlineThickness),
    ("SPACE_WIDTH", SyncedMetric::SpaceWidth),
    ("LINE_GAP", SyncedMetric::LineGap),
    ("LINE_HEIGHT", SyncedMetric::LineHeight),
];
impl SyncedMetric {
    fn for_property(name: &str) -> Option<Self> {
        SYNCED
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, metric)| *metric)
    }
    const fn index(self) -> usize {
        self as usize
    }
    fn get(self, font: &Font) -> Option<i64> {
        let m = &font.metrics;
        match self {
            Self::PixelSize => m.pixel_size.map(i64::from),
            Self::Ascent => m.ascent,
            Self::Descent => m.descent,
            Self::UnderlinePosition => m.underline_position,
            Self::UnderlineThickness => m.underline_thickness,
            Self::SpaceWidth => m.space_width,
            Self::LineGap => m.line_gap,
            Self::LineHeight => m.line_height,
        }
    }
    /// Values were validated (pixel size positive and within u32) before staging.
    fn set(self, font: &mut Font, value: Option<i64>) {
        let m = &mut font.metrics;
        match self {
            Self::PixelSize => m.pixel_size = value.and_then(|v| u32::try_from(v).ok()),
            Self::Ascent => m.ascent = value,
            Self::Descent => m.descent = value,
            Self::UnderlinePosition => m.underline_position = value,
            Self::UnderlineThickness => m.underline_thickness = value,
            Self::SpaceWidth => m.space_width = value,
            Self::LineGap => m.line_gap = value,
            Self::LineHeight => m.line_height = value,
        }
    }
}

/// The model field a [`MetadataChange`] refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "snake_case")
)]
pub enum MetadataField {
    /// `FontNames::xlfd` (the BDF `FONT` header). The decomposed
    /// `FontNames::xlfd_fields` mirror is re-derived from the new name.
    FontName,
    /// `FontMetrics::point_size`.
    PointSize,
    /// `FontMetrics::resolution`.
    Resolution,
    /// One record of the ordered property list.
    Property {
        /// Exact record spelling.
        name: String,
        /// Record position in the property list just before this change.
        record: usize,
    },
    /// Normalized metric kept in sync with an integer property edit.
    Metric(SyncedMetric),
}

/// A value recorded in a [`MetadataChange`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MetadataValue {
    /// Font name text.
    Text(String),
    /// Normalized integer metric.
    Integer(i64),
    /// Exact point size.
    Rational(Rational),
    /// Resolution.
    Vector(Vector),
    /// Complete property value.
    Property(PropertyValue),
}
impl MetadataValue {
    fn heap_bytes(&self) -> usize {
        match self {
            Self::Text(text) => text.capacity(),
            Self::Property(value) => value_bytes(value),
            Self::Integer(_) | Self::Rational(_) | Self::Vector(_) => 0,
        }
    }
}

/// One committed metadata change with its complete before/after values.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct MetadataChange {
    /// Strike index in the projected set.
    pub font_index: usize,
    /// Changed field.
    pub field: MetadataField,
    /// Value before the projection (`None` when absent).
    pub before: Option<MetadataValue>,
    /// Value after the projection (`None` when removed).
    pub after: Option<MetadataValue>,
}
impl MetadataChange {
    fn heap_bytes(&self) -> usize {
        let field = match &self.field {
            MetadataField::Property { name, .. } => name.capacity(),
            _ => 0,
        };
        field
            + self.before.as_ref().map_or(0, MetadataValue::heap_bytes)
            + self.after.as_ref().map_or(0, MetadataValue::heap_bytes)
    }
}

/// Complete result of a committed projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ProjectionOutcome {
    /// Diagnostics: one entry per strike, option and impact class.
    pub report: Report,
    /// Every changed scalable advance, in strike then glyph-ID order.
    pub scalable_advance_changes: Vec<ScalableAdvanceChange>,
    /// Every changed metadata value, in strike then option order.
    pub metadata_changes: Vec<MetadataChange>,
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
        let metadata = self.metadata_changes.capacity() * size_of::<MetadataChange>()
            + self
                .metadata_changes
                .iter()
                .map(MetadataChange::heap_bytes)
                .sum::<usize>();
        entries + strings + changes + metadata
    }
}

fn limit_error(limit: usize) -> Error {
    Error::ResourceLimit {
        resource: "projection working bytes",
        limit,
    }
}

/// Heap bytes owned by a property value (capacities, recursively).
fn value_bytes(value: &PropertyValue) -> usize {
    match value {
        PropertyValue::Decimal(s) | PropertyValue::Text(s) => s.capacity(),
        PropertyValue::Bytes(b) => b.capacity(),
        PropertyValue::List(items) => {
            items.capacity() * size_of::<PropertyValue>()
                + items.iter().map(value_bytes).sum::<usize>()
        }
        PropertyValue::Integer(_) | PropertyValue::Unsigned(_) | PropertyValue::Flag => 0,
    }
}
fn property_bytes(property: &Property) -> usize {
    property.name.capacity()
        + value_bytes(&property.value)
        + property.raw.as_ref().map_or(0, Vec::capacity)
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

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn is_notice(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["COPYRIGHT", "NOTICE", "LICENSE", "LICENCE"]
        .iter()
        .any(|word| upper.contains(word))
}
/// Properties owned by normalized names, charsets, spacing or the default
/// character: use the dedicated name/charset options instead.
const NORMALIZED_TEXT: [&str; 10] = [
    "FAMILY_NAME",
    "STYLE_NAME",
    "FACE_NAME",
    "POSTSCRIPT_NAME",
    "FONT_VERSION",
    "FOUNDRY",
    "CHARSET_REGISTRY",
    "CHARSET_ENCODING",
    "SPACING",
    "DEFAULT_CHAR",
];
fn single_line(text: &str, what: &str, max: usize) -> Result<()> {
    if text.len() > max {
        return Err(invalid(format!("{what} exceeds {max} bytes")));
    }
    if text.contains(['\n', '\r', '\0']) {
        return Err(invalid(format!(
            "{what} must not contain a line break or NUL"
        )));
    }
    Ok(())
}
fn validate_options(projection: &BitmapProjection<'_>) -> Result<()> {
    if let ScalableAdvance::FromDevice {
        units_per_em,
        reference_ppem,
    } = projection.scalable_advance
    {
        if reference_ppem == 0 {
            return Err(invalid(
                "scalable-advance projection requires a positive reference ppem",
            ));
        }
        if units_per_em == 0 {
            return Err(invalid(
                "scalable-advance projection requires positive units per em",
            ));
        }
    }
    if let Some(name) = projection.font_name {
        single_line(name, "projected font name", MAX_FONT_NAME_BYTES)?;
        if name.trim().is_empty() || name.trim() != name {
            return Err(invalid(
                "projected font name must be nonempty without surrounding whitespace",
            ));
        }
    }
    if let Some(size) = projection.size {
        if size.point_size.numerator() <= 0 {
            return Err(invalid("projected point size must be positive"));
        }
        for dpi in [size.resolution.x, size.resolution.y] {
            if dpi <= 0 || i32::try_from(dpi).is_err() {
                return Err(invalid(
                    "projected resolution must be positive and fit 32 bits",
                ));
            }
        }
    }
    if projection.properties.len() > MAX_PROPERTY_EDITS {
        return Err(invalid(format!(
            "at most {MAX_PROPERTY_EDITS} property edits are allowed"
        )));
    }
    for (index, edit) in projection.properties.iter().enumerate() {
        let name = edit.name();
        if name.is_empty()
            || name.len() > MAX_PROPERTY_NAME_BYTES
            || !name.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(invalid(format!(
                "property edit name {name:?} must be 1..={MAX_PROPERTY_NAME_BYTES} printable ASCII bytes without spaces"
            )));
        }
        if projection.properties[..index]
            .iter()
            .any(|other| other.name().eq_ignore_ascii_case(name))
        {
            return Err(invalid(format!(
                "property {name:?} is edited more than once"
            )));
        }
        let lower = name.to_ascii_lowercase();
        if name.eq_ignore_ascii_case("COMMENT") || is_notice(name) {
            return Err(invalid(format!(
                "property {name:?} carries comments or notice text and cannot be edited by a projection"
            )));
        }
        if lower.starts_with("bdf.") || lower.starts_with("pcf.") {
            return Err(invalid(format!(
                "property {name:?} is internal source provenance"
            )));
        }
        if NORMALIZED_TEXT.iter().any(|k| k.eq_ignore_ascii_case(name)) {
            return Err(invalid(format!(
                "property {name:?} is owned by normalized names/charset; use the dedicated option"
            )));
        }
        if let PropertyEdit::Set { value, .. } = edit {
            if name.eq_ignore_ascii_case("FONT") {
                return Err(invalid(
                    "set the FONT name with the font-name projection, not a property edit",
                ));
            }
            match *value {
                EditValue::Integer(v) => {
                    if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&v) {
                        return Err(invalid(format!(
                            "property {name} value {v} is outside the 32-bit range"
                        )));
                    }
                    if SyncedMetric::for_property(name) == Some(SyncedMetric::PixelSize) && v <= 0 {
                        return Err(invalid("PIXEL_SIZE must be positive"));
                    }
                }
                EditValue::Text(text) => {
                    single_line(text, "property text", MAX_PROPERTY_TEXT_BYTES)?;
                    if SyncedMetric::for_property(name).is_some() {
                        return Err(invalid(format!(
                            "property {name} is an integer metric and requires an integer value"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Split an XLFD the way the BDF/PCF readers do: leading `-`, empty fields kept.
fn xlfd_fields(name: &str) -> impl Iterator<Item = &str> {
    name.strip_prefix('-')
        .into_iter()
        .flat_map(|rest| rest.split('-'))
}

/// Upper bounds for one strike's metadata work, computed without allocating.
#[derive(Default)]
struct Bound {
    changes: usize,
    entries: usize,
    bytes: usize,
}
fn checked(total: &mut usize, amount: usize, limit: usize) -> Result<()> {
    *total = total
        .checked_add(amount)
        .ok_or_else(|| limit_error(limit))?;
    Ok(())
}
fn metadata_bound(font: &Font, projection: &BitmapProjection<'_>, limit: usize) -> Result<Bound> {
    let mut b = Bound::default();
    let mut touches_properties = false;
    let mut appended = 0usize;
    if let Some(name) = projection.font_name {
        b.changes += 1;
        b.entries += 2;
        // Model string, its audit copy, the replaced value's audit copy and the fields.
        let fields = xlfd_fields(name).count();
        checked(&mut b.bytes, name.len().saturating_mul(3), limit)?;
        checked(
            &mut b.bytes,
            font.names.xlfd.as_ref().map_or(0, String::len),
            limit,
        )?;
        checked(
            &mut b.bytes,
            fields.saturating_mul(size_of::<String>()),
            limit,
        )?;
        for property in font.properties.iter() {
            if property.name.eq_ignore_ascii_case("FONT") {
                touches_properties = true;
                b.changes += 1;
                checked(&mut b.bytes, property.name.len(), limit)?;
                checked(&mut b.bytes, value_bytes(&property.value), limit)?;
                checked(&mut b.bytes, name.len().saturating_mul(2), limit)?;
            }
        }
    }
    if projection.size.is_some() {
        b.changes += 2;
        b.entries += 2;
    }
    for edit in projection.properties {
        let name = edit.name();
        b.entries += 1;
        let mut matched = 0usize;
        for property in font.properties.iter() {
            if property.name.eq_ignore_ascii_case(name) {
                matched += 1;
                checked(&mut b.bytes, property.name.len(), limit)?;
                checked(&mut b.bytes, value_bytes(&property.value), limit)?;
            }
        }
        // FONT records may have been rewritten by the font-name option first.
        if name.eq_ignore_ascii_case("FONT")
            && let Some(projected) = projection.font_name
        {
            checked(&mut b.bytes, projected.len().saturating_mul(matched), limit)?;
        }
        match edit {
            PropertyEdit::Set { value, .. } => {
                touches_properties = true;
                b.changes += 2;
                // The replacement list reserves one slot per Set edit.
                appended += 1;
                // New record name, value in the model and in the audit.
                checked(&mut b.bytes, name.len().saturating_mul(2), limit)?;
                checked(&mut b.bytes, value.heap_bytes().saturating_mul(2), limit)?;
            }
            PropertyEdit::Remove { .. } => {
                touches_properties |= matched > 0;
                b.changes = b.changes.saturating_add(matched).saturating_add(1);
                checked(&mut b.bytes, name.len().saturating_mul(matched), limit)?;
            }
        }
    }
    if touches_properties {
        // A complete replacement list: old and new records coexist until commit.
        let slots = font
            .properties
            .len()
            .checked_add(appended)
            .ok_or_else(|| limit_error(limit))?;
        checked(
            &mut b.bytes,
            slots
                .checked_mul(size_of::<Property>())
                .ok_or_else(|| limit_error(limit))?,
            limit,
        )?;
        for property in font.properties.iter() {
            checked(&mut b.bytes, property_bytes(property), limit)?;
        }
    }
    Ok(b)
}

/// One strike's staged metadata, moved into the model at commit.
struct Staged {
    font_index: usize,
    xlfd: Option<(String, Vec<String>)>,
    size: Option<(Option<Rational>, Option<Vector>)>,
    properties: Option<Vec<Property>>,
    metrics: [Option<Option<i64>>; 8],
}

struct Diagnostics<'r> {
    report: &'r mut Report,
    font_index: usize,
}
impl Diagnostics<'_> {
    fn push(&mut self, impact: Impact, code: &str, message: String) {
        self.report.push(ReportEntry {
            level: if impact == Impact::Loss {
                Level::Warning
            } else {
                Level::Info
            },
            impact,
            code: code.into(),
            location: Location {
                font_index: Some(self.font_index),
                ..Location::default()
            },
            message,
        });
    }
}

fn stage_metadata(
    font_index: usize,
    font: &Font,
    projection: &BitmapProjection<'_>,
    changes: &mut Vec<MetadataChange>,
    report: &mut Report,
) -> Result<Option<Staged>> {
    let mut staged = Staged {
        font_index,
        xlfd: None,
        size: None,
        properties: None,
        metrics: [None; 8],
    };
    let mut d = Diagnostics { report, font_index };
    let mut touched = false;
    // The replacement property list is cloned only when a record changes; its
    // capacity already covers every possible appended record.
    let mut props: Option<Vec<Property>> = None;
    let list = |props: &mut Option<Vec<Property>>| -> Result<()> {
        if props.is_none() {
            let appends = projection
                .properties
                .iter()
                .filter(|e| matches!(e, PropertyEdit::Set { .. }))
                .count();
            let mut v = Vec::new();
            v.try_reserve_exact(font.properties.len() + appends)
                .map_err(|_| limit_error(usize::MAX))?;
            v.extend(font.properties.iter().cloned());
            *props = Some(v);
        }
        Ok(())
    };
    if let Some(name) = projection.font_name {
        if font.names.xlfd.as_deref() != Some(name) {
            let before = font.names.xlfd.clone();
            let impact = if before.is_some() {
                Impact::Loss
            } else {
                Impact::Repair
            };
            changes.push(MetadataChange {
                font_index,
                field: MetadataField::FontName,
                before: before.map(MetadataValue::Text),
                after: Some(MetadataValue::Text(name.to_owned())),
            });
            staged.xlfd = Some((
                name.to_owned(),
                xlfd_fields(name).map(str::to_owned).collect(),
            ));
            touched = true;
            d.push(
                impact,
                if impact == Impact::Loss {
                    "projection.font_name.replaced"
                } else {
                    "projection.font_name.derived"
                },
                "FONT/XLFD name set by explicit projection; decomposed XLFD fields re-derived"
                    .into(),
            );
        }
        let stale = |p: &Property| {
            p.name.eq_ignore_ascii_case("FONT") && !EditValue::Text(name).matches(&p.value)
        };
        if font.properties.iter().any(stale) {
            list(&mut props)?;
            let mut synced = 0usize;
            for (index, record) in props.as_mut().expect("staged list").iter_mut().enumerate() {
                if !stale(record) {
                    continue;
                }
                let before = std::mem::replace(&mut record.value, PropertyValue::Text(name.into()));
                record.raw = None;
                changes.push(MetadataChange {
                    font_index,
                    field: MetadataField::Property {
                        name: record.name.clone(),
                        record: index,
                    },
                    before: Some(MetadataValue::Property(before)),
                    after: Some(MetadataValue::Property(PropertyValue::Text(name.into()))),
                });
                synced += 1;
            }
            touched = true;
            d.push(
                Impact::Loss,
                "projection.font_name.property_synced",
                format!("{synced} FONT property record(s) replaced to match the projected name"),
            );
        }
    }
    if let Some(size) = projection.size {
        let mut changed = false;
        for (field, before, after) in [
            (
                MetadataField::PointSize,
                font.metrics.point_size.map(MetadataValue::Rational),
                MetadataValue::Rational(size.point_size),
            ),
            (
                MetadataField::Resolution,
                font.metrics.resolution.map(MetadataValue::Vector),
                MetadataValue::Vector(size.resolution),
            ),
        ] {
            if before.as_ref() == Some(&after) {
                continue;
            }
            changed = true;
            let replaced = before.is_some();
            let code = match (&field, replaced) {
                (MetadataField::PointSize, true) => "projection.point_size.replaced",
                (MetadataField::PointSize, false) => "projection.point_size.derived",
                (_, true) => "projection.resolution.replaced",
                (_, false) => "projection.resolution.derived",
            };
            let label = if field == MetadataField::PointSize {
                "point size"
            } else {
                "resolution"
            };
            changes.push(MetadataChange {
                font_index,
                field,
                before,
                after: Some(after),
            });
            d.push(
                if replaced {
                    Impact::Loss
                } else {
                    Impact::Repair
                },
                code,
                format!("{label} set by explicit projection"),
            );
        }
        if changed {
            staged.size = Some((Some(size.point_size), Some(size.resolution)));
            touched = true;
        }
    }
    for edit in projection.properties {
        let name = edit.name();
        let named = |p: &Property| p.name.eq_ignore_ascii_case(name);
        let metric = SyncedMetric::for_property(name);
        match *edit {
            PropertyEdit::Set { value, .. } => {
                let last = match &props {
                    Some(v) => v
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, p)| named(p))
                        .map(|(i, p)| (i, value.matches(&p.value))),
                    None => font
                        .properties
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, p)| named(p))
                        .map(|(i, p)| (i, value.matches(&p.value))),
                };
                if !last.is_some_and(|(_, same)| same) {
                    list(&mut props)?;
                    let v = props.as_mut().expect("staged list");
                    let (index, before, spelling) = if let Some((i, _)) = last {
                        let before = std::mem::replace(&mut v[i].value, value.property());
                        v[i].raw = None;
                        (i, Some(before), v[i].name.clone())
                    } else {
                        // Capacity was reserved for every Set edit: no reallocation.
                        v.push(Property::new(name, value.property()));
                        (v.len() - 1, None, name.to_owned())
                    };
                    let replaced = before.is_some();
                    changes.push(MetadataChange {
                        font_index,
                        field: MetadataField::Property {
                            name: spelling,
                            record: index,
                        },
                        before: before.map(MetadataValue::Property),
                        after: Some(MetadataValue::Property(value.property())),
                    });
                    touched = true;
                    d.push(
                        if replaced {
                            Impact::Loss
                        } else {
                            Impact::Repair
                        },
                        if replaced {
                            "projection.property.replaced"
                        } else {
                            "projection.property.added"
                        },
                        format!("property {name} set by explicit projection"),
                    );
                }
                if let (Some(metric), EditValue::Integer(v)) = (metric, value) {
                    let before = metric.get(font);
                    if before != Some(v) {
                        staged.metrics[metric.index()] = Some(Some(v));
                        changes.push(MetadataChange {
                            font_index,
                            field: MetadataField::Metric(metric),
                            before: before.map(MetadataValue::Integer),
                            after: Some(MetadataValue::Integer(v)),
                        });
                        touched = true;
                    }
                }
            }
            PropertyEdit::Remove { .. } => {
                let present = match &props {
                    Some(v) => v.iter().any(named),
                    None => font.properties.iter().any(named),
                };
                if present {
                    list(&mut props)?;
                    let v = props.as_mut().expect("staged list");
                    let mut removed = 0usize;
                    let mut index = 0usize;
                    while index < v.len() {
                        if !named(&v[index]) {
                            index += 1;
                            continue;
                        }
                        let record = v.remove(index);
                        changes.push(MetadataChange {
                            font_index,
                            field: MetadataField::Property {
                                name: record.name,
                                record: index,
                            },
                            before: Some(MetadataValue::Property(record.value)),
                            after: None,
                        });
                        removed += 1;
                    }
                    touched = true;
                    d.push(
                        Impact::Loss,
                        "projection.property.removed",
                        format!(
                            "{removed} {name} property record(s) removed by explicit projection"
                        ),
                    );
                }
                if let Some(metric) = metric {
                    let before = metric.get(font);
                    if before.is_some() {
                        staged.metrics[metric.index()] = Some(None);
                        changes.push(MetadataChange {
                            font_index,
                            field: MetadataField::Metric(metric),
                            before: before.map(MetadataValue::Integer),
                            after: None,
                        });
                        touched = true;
                    }
                }
            }
        }
    }
    staged.properties = props;
    Ok(touched.then_some(staged))
}

fn commit_metadata(font: &mut Font, staged: Staged) {
    if let Some((name, fields)) = staged.xlfd {
        font.names.xlfd = Some(name);
        font.names.xlfd_fields = fields;
    }
    if let Some((point, resolution)) = staged.size {
        font.metrics.point_size = point;
        font.metrics.resolution = resolution;
    }
    if let Some(properties) = staged.properties {
        font.properties = properties.into();
    }
    for (index, value) in staged.metrics.into_iter().enumerate() {
        if let Some(value) = value {
            SYNCED[index].1.set(font, value);
        }
    }
}

/// Apply `projection` to every strike as one transaction.
///
/// All options and glyphs are validated and every change is staged before the
/// first commit; on any error `set` is unchanged. `max_working_bytes` bounds
/// the staged values, the replacement property lists and the returned audit
/// and diagnostics (upper bounds are admitted before any allocation); callers
/// charge [`ProjectionOutcome::retained_bytes`] to their pipeline ledger.
pub fn apply_bitmap_projection(
    set: &mut FontSet,
    projection: &BitmapProjection<'_>,
    max_working_bytes: usize,
) -> Result<ProjectionOutcome> {
    validate_options(projection)?;
    if projection.is_identity() {
        return Ok(ProjectionOutcome::default());
    }
    let limit = max_working_bytes;
    let swidth = match projection.scalable_advance {
        ScalableAdvance::FromDevice {
            units_per_em,
            reference_ppem,
        } => Some((units_per_em, reference_ppem)),
        ScalableAdvance::Preserve => None,
    };

    // Pass 1: exact SWIDTH count and metadata upper bounds, admitted before allocation.
    let mut count = 0usize;
    if let Some((units_per_em, reference_ppem)) = swidth {
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
    }
    let mut metadata = Bound::default();
    for font in &set.fonts {
        let b = metadata_bound(font, projection, limit)?;
        metadata.changes = metadata
            .changes
            .checked_add(b.changes)
            .ok_or_else(|| limit_error(limit))?;
        metadata.entries = metadata
            .entries
            .checked_add(b.entries)
            .ok_or_else(|| limit_error(limit))?;
        checked(&mut metadata.bytes, b.bytes, limit)?;
    }
    let change_bytes = count
        .checked_mul(size_of::<ScalableAdvanceChange>())
        .ok_or_else(|| limit_error(limit))?;
    // At most three SWIDTH entries (loss, repair, missing device advance) per strike.
    let entry_capacity = set
        .fonts
        .len()
        .checked_mul(if swidth.is_some() { 3 } else { 0 })
        .and_then(|n| n.checked_add(metadata.entries))
        .ok_or_else(|| limit_error(limit))?;
    const MESSAGE_BOUND: usize = 192 + MAX_PROPERTY_NAME_BYTES;
    let report_bytes = entry_capacity
        .checked_mul(size_of::<ReportEntry>() + MESSAGE_BOUND + 48)
        .ok_or_else(|| limit_error(limit))?;
    let metadata_bytes = metadata
        .changes
        .checked_mul(size_of::<MetadataChange>())
        .and_then(|n| n.checked_add(metadata.bytes))
        .and_then(|n| n.checked_add(set.fonts.len().checked_mul(size_of::<Staged>())?))
        .ok_or_else(|| limit_error(limit))?;
    if change_bytes
        .checked_add(report_bytes)
        .and_then(|n| n.checked_add(metadata_bytes))
        .is_none_or(|total| total > limit)
    {
        return Err(limit_error(limit));
    }
    let mut changes = Vec::new();
    changes
        .try_reserve_exact(count)
        .map_err(|_| limit_error(limit))?;
    let mut metadata_changes = Vec::new();
    metadata_changes
        .try_reserve_exact(metadata.changes)
        .map_err(|_| limit_error(limit))?;
    let mut staged = Vec::new();
    if metadata.entries > 0 {
        staged
            .try_reserve_exact(set.fonts.len())
            .map_err(|_| limit_error(limit))?;
    }
    let mut report = Report::default();
    report
        .entries
        .try_reserve_exact(entry_capacity)
        .map_err(|_| limit_error(limit))?;

    // Pass 2: stage changes and diagnostics without touching the model.
    for (font_index, font) in set.fonts.iter().enumerate() {
        if let Some(s) = stage_metadata(
            font_index,
            font,
            projection,
            &mut metadata_changes,
            &mut report,
        )
        .map_err(|e| match e {
            Error::ResourceLimit { resource, .. } => Error::ResourceLimit { resource, limit },
            e => e,
        })? {
            staged.push(s);
        }
        let Some((units_per_em, reference_ppem)) = swidth else {
            continue;
        };
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
        let mut d = Diagnostics {
            report: &mut report,
            font_index,
        };
        if losses > 0 {
            d.push(
                Impact::Loss,
                "projection.scalable_advance.replaced",
                format!(
                    "{losses} source scalable advance(s) replaced by device advance * \
                     {units_per_em} / {reference_ppem} (half-even)"
                ),
            );
        }
        if repairs > 0 {
            d.push(
                Impact::Repair,
                "projection.scalable_advance.derived",
                format!(
                    "{repairs} absent scalable advance(s) derived from device advance * \
                     {units_per_em} / {reference_ppem} (half-even)"
                ),
            );
        }
        if missing > 0 {
            d.report.push(ReportEntry {
                level: Level::Warning,
                impact: Impact::None,
                code: "projection.scalable_advance.no_device_advance".into(),
                location: Location {
                    font_index: Some(font_index),
                    ..Location::default()
                },
                message: format!(
                    "{missing} glyph(s) have no device advance; scalable advance kept"
                ),
            });
        }
    }

    // Commit: moves and numeric assignments only, no allocation and no fallible
    // step; IDs were validated above under the same exclusive borrow.
    for s in staged {
        let index = s.font_index;
        commit_metadata(&mut set.fonts[index], s);
    }
    for change in &changes {
        set.fonts[change.font_index]
            .set_scalable_advance_prevalidated(change.glyph, Some(change.after));
    }
    Ok(ProjectionOutcome {
        report,
        scalable_advance_changes: changes,
        metadata_changes,
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

    fn named_font(xlfd: Option<&str>, properties: &[(&str, PropertyValue)]) -> Font {
        let mut font = Font::from_glyphs([glyph(Some(6), Some(500))]).unwrap();
        font.names.xlfd = xlfd.map(str::to_owned);
        font.names.xlfd_fields = xlfd
            .map(|s| xlfd_fields(s).map(str::to_owned).collect())
            .unwrap_or_default();
        for (name, value) in properties {
            font.properties.push(Property::new(*name, value.clone()));
        }
        font
    }
    fn with<'a>(edit: impl FnOnce(&mut BitmapProjection<'a>)) -> BitmapProjection<'a> {
        let mut projection = BitmapProjection::default();
        edit(&mut projection);
        projection
    }

    #[test]
    fn font_name_resyncs_fields_and_font_property_with_audit() {
        let old = "-inesw-Cozette-Medium-R-Normal--13-120-75-75-M-60-ISO10646-1";
        let new = "-opf-cozette-medium-r-normal--13-130-75-75-p-130-iso10646-1";
        let mut set = FontSet::from(named_font(Some(old), &[("FONT", old.into())]));
        let projection = with(|p| p.font_name = Some(new));
        let outcome = apply_bitmap_projection(&mut set, &projection, 1 << 20).unwrap();
        let font = &set.fonts[0];
        assert_eq!(font.names.xlfd.as_deref(), Some(new));
        assert_eq!(font.names.xlfd_fields.len(), 14);
        assert_eq!(font.names.xlfd_fields[0], "opf");
        assert_eq!(font.names.xlfd_fields[5], "");
        assert_eq!(font.properties.get("FONT"), Some(&PropertyValue::from(new)));
        assert_eq!(outcome.metadata_changes.len(), 2);
        assert_eq!(outcome.metadata_changes[0].field, MetadataField::FontName);
        assert_eq!(
            outcome.metadata_changes[0].before,
            Some(MetadataValue::Text(old.into()))
        );
        let codes: Vec<_> = outcome
            .report
            .entries
            .iter()
            .map(|e| e.code.as_str())
            .collect();
        assert_eq!(
            codes,
            [
                "projection.font_name.replaced",
                "projection.font_name.property_synced"
            ]
        );
        assert!(outcome.report.has_loss());
        assert!(outcome.retained_bytes() > 0);
        // Applying the same projection again is not a change.
        let again = apply_bitmap_projection(&mut set, &projection, 1 << 20).unwrap();
        assert_eq!(again, ProjectionOutcome::default());
    }

    #[test]
    fn size_replacement_is_loss_and_absent_size_is_repair() {
        let size = SizeProjection {
            point_size: Rational::integer(13),
            resolution: Vector::new(75, 75),
        };
        let projection = with(|p| p.size = Some(size));
        let mut font = named_font(None, &[]);
        font.metrics.point_size = Some(Rational::integer(12));
        font.metrics.resolution = Some(Vector::new(75, 75));
        let mut set = FontSet::from(font);
        let outcome = apply_bitmap_projection(&mut set, &projection, 1 << 20).unwrap();
        // Only the point size differs; the equal resolution is not a change.
        assert_eq!(outcome.metadata_changes.len(), 1);
        assert_eq!(outcome.metadata_changes[0].field, MetadataField::PointSize);
        assert_eq!(
            outcome.report.entries[0].code,
            "projection.point_size.replaced"
        );
        assert!(outcome.report.has_loss());
        assert_eq!(set.fonts[0].metrics.point_size, Some(Rational::integer(13)));
        let mut absent = FontSet::from(named_font(None, &[]));
        let outcome = apply_bitmap_projection(&mut absent, &projection, 1 << 20).unwrap();
        assert_eq!(outcome.metadata_changes.len(), 2);
        assert!(!outcome.report.has_loss());
        assert!(
            outcome
                .report
                .entries
                .iter()
                .all(|e| e.impact == Impact::Repair)
        );
    }

    #[test]
    fn property_edits_classify_sync_metrics_and_keep_comments() {
        let mut font = named_font(
            Some("-windows-Demo-medium-r-normal--13-80-96-96-c-70-iso8859-1"),
            &[
                ("COMMENT", "kept".into()),
                (
                    "font",
                    "-windows-Demo-medium-r-normal--13-80-96-96-c-70-iso8859-1".into(),
                ),
                ("WEIGHT_NAME", "Medium".into()),
                ("FONT", "duplicate".into()),
                ("AVERAGE_WIDTH", PropertyValue::Integer(70)),
            ],
        );
        font.metrics.pixel_size = Some(13);
        let mut set = FontSet::from(font);
        let edits = [
            PropertyEdit::Remove { name: "FONT" },
            PropertyEdit::Set {
                name: "PIXEL_SIZE",
                value: EditValue::Integer(13),
            },
            PropertyEdit::Set {
                name: "weight_name",
                value: EditValue::Text("Bold"),
            },
            PropertyEdit::Set {
                name: "AVERAGE_WIDTH",
                value: EditValue::Integer(70),
            },
            PropertyEdit::Set {
                name: "UNDERLINE_POSITION",
                value: EditValue::Integer(-2),
            },
            PropertyEdit::Remove { name: "ABSENT" },
        ];
        let projection = with(|p| p.properties = &edits);
        let outcome = apply_bitmap_projection(&mut set, &projection, 1 << 20).unwrap();
        let font = &set.fonts[0];
        let names: Vec<_> = font.properties.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "COMMENT",
                "WEIGHT_NAME",
                "AVERAGE_WIDTH",
                "PIXEL_SIZE",
                "UNDERLINE_POSITION"
            ]
        );
        assert_eq!(
            font.properties.get("WEIGHT_NAME"),
            Some(&PropertyValue::from("Bold"))
        );
        // PIXEL_SIZE already matched the model; UNDERLINE_POSITION syncs it.
        assert_eq!(font.metrics.pixel_size, Some(13));
        assert_eq!(font.metrics.underline_position, Some(-2));
        let codes: Vec<_> = outcome
            .report
            .entries
            .iter()
            .map(|e| (e.code.as_str(), e.impact))
            .collect();
        assert_eq!(
            codes,
            [
                ("projection.property.removed", Impact::Loss),
                ("projection.property.added", Impact::Repair),
                ("projection.property.replaced", Impact::Loss),
                ("projection.property.added", Impact::Repair),
            ]
        );
        // Two removed FONT records, three set records and one synced metric.
        assert_eq!(outcome.metadata_changes.len(), 6);
        assert!(outcome.metadata_changes.iter().any(|c| c.field
            == MetadataField::Metric(SyncedMetric::UnderlinePosition)
            && c.before.is_none()));
        assert_eq!(
            outcome.metadata_changes[1].field,
            MetadataField::Property {
                name: "FONT".into(),
                record: 2
            }
        );
    }

    #[test]
    fn font_name_sync_precedes_font_property_removal() {
        let mut set = FontSet::from(named_font(Some("old"), &[("FONT", "old".into())]));
        let edits = [PropertyEdit::Remove { name: "FONT" }];
        let projection = with(|p| {
            p.font_name = Some("new");
            p.properties = &edits;
        });
        let outcome = apply_bitmap_projection(&mut set, &projection, 1 << 20).unwrap();
        assert!(set.fonts[0].properties.is_empty());
        assert!(set.fonts[0].names.xlfd_fields.is_empty());
        let last = outcome.metadata_changes.last().unwrap();
        assert_eq!(
            last.before,
            Some(MetadataValue::Property(PropertyValue::from("new")))
        );
        assert_eq!(last.after, None);
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_edits_without_change() {
        let font = named_font(
            Some("-a-b"),
            &[
                ("COPYRIGHT", "(c) author".into()),
                ("COMMENT", "notice".into()),
            ],
        );
        let reject = |edits: &[PropertyEdit<'_>], name: Option<&str>| {
            let mut set = FontSet::from(font.clone());
            let before = set.clone();
            let projection = with(|p| {
                p.properties = edits;
                p.font_name = name;
            });
            assert!(matches!(
                apply_bitmap_projection(&mut set, &projection, usize::MAX),
                Err(Error::InvalidInput(_))
            ));
            assert_eq!(set, before);
        };
        for name in [
            "COMMENT",
            "COPYRIGHT",
            "notice",
            "LICENSE_URL",
            "FAMILY_NAME",
            "charset_registry",
            "bdf.source.headers",
            "",
            "TWO WORDS",
        ] {
            reject(&[PropertyEdit::Remove { name }], None);
        }
        let text = |name, text| PropertyEdit::Set {
            name,
            value: EditValue::Text(text),
        };
        let int = |name, value| PropertyEdit::Set {
            name,
            value: EditValue::Integer(value),
        };
        reject(&[text("FONT", "x")], None);
        reject(&[text("X", "line\nbreak")], None);
        reject(&[text("PIXEL_SIZE", "13")], None);
        reject(&[int("PIXEL_SIZE", 0)], None);
        reject(&[int("X", i64::from(u32::MAX) + 1)], None);
        reject(&[int("X", 1), PropertyEdit::Remove { name: "x" }], None);
        reject(&[int("X", 1); MAX_PROPERTY_EDITS + 1], None);
        reject(&[], Some(""));
        reject(&[], Some(" padded"));
        reject(&[], Some("two\nlines"));
        let mut set = FontSet::from(font.clone());
        let before = set.clone();
        let bad_size = with(|p| {
            p.size = Some(SizeProjection {
                point_size: Rational::integer(0),
                resolution: Vector::new(75, 75),
            })
        });
        assert!(apply_bitmap_projection(&mut set, &bad_size, usize::MAX).is_err());
        assert_eq!(set, before);
    }

    #[test]
    fn later_strike_failure_and_budget_failure_commit_nothing() {
        let ok = named_font(Some("old"), &[("FONT", "old".into())]);
        let overflow = Font::from_glyphs([glyph(Some(i64::MAX), None)]).unwrap();
        let mut set = FontSet {
            fonts: vec![ok.clone(), overflow],
            ..FontSet::default()
        };
        let before = set.clone();
        let edits = [PropertyEdit::Set {
            name: "PIXEL_SIZE",
            value: EditValue::Integer(13),
        }];
        let mut projection = BitmapProjection::scalable_from_device(1000, 1);
        projection.font_name = Some("new");
        projection.properties = &edits;
        assert!(apply_bitmap_projection(&mut set, &projection, usize::MAX).is_err());
        assert_eq!(set, before);
        // Exactly at the admitted bound succeeds; one byte less fails atomically.
        let projection = BitmapProjection {
            font_name: Some("new"),
            properties: &edits,
            ..BitmapProjection::default()
        };
        let mut low = 0usize;
        let mut high = 1 << 20;
        while low < high {
            let mid = (low + high) / 2;
            let mut probe = FontSet::from(ok.clone());
            if apply_bitmap_projection(&mut probe, &projection, mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let mut failing = FontSet::from(ok.clone());
        let before = failing.clone();
        assert!(matches!(
            apply_bitmap_projection(&mut failing, &projection, low - 1),
            Err(Error::ResourceLimit { .. })
        ));
        assert_eq!(failing, before);
        let mut passing = FontSet::from(ok);
        let outcome = apply_bitmap_projection(&mut passing, &projection, low).unwrap();
        assert!(outcome.retained_bytes() <= low);
    }

    #[cfg(all(feature = "bdf", feature = "pcf"))]
    #[test]
    fn pcf_precedence_keeps_existing_size_properties_and_reports_loss() {
        use crate::formats::pcf::{self, MetadataPrecedence, PcfWriteOptions};
        let source = b"STARTFONT 2.1\nFONT -x-demo-medium-r-normal--13-120-72-72-c-60-iso10646-1\n\
SIZE 12 72 72\nFONTBOUNDINGBOX 6 13 0 -3\nCOMMENT \"(c) kept notice\"\nSTARTPROPERTIES 4\n\
POINT_SIZE 120\nRESOLUTION_X 72\nRESOLUTION_Y 72\nPIXEL_SIZE 13\nENDPROPERTIES\nCHARS 1\n\
STARTCHAR A\nENCODING 65\nSWIDTH 500 0\nDWIDTH 6 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n";
        let (font, _) =
            crate::formats::bdf::read(source, &crate::ResourceLimits::default()).unwrap();
        let mut set = FontSet::from(font);
        let mut projection = BitmapProjection::scalable_from_device(1000, 13);
        projection.font_name = Some("-opf-demo-medium-r-normal--13-130-75-75-p-130-iso10646-1");
        projection.size = Some(SizeProjection {
            point_size: Rational::integer(13),
            resolution: Vector::new(75, 75),
        });
        let outcome = apply_bitmap_projection(&mut set, &projection, 1 << 20).unwrap();
        assert!(outcome.report.has_loss());
        let font = &set.fonts[0];
        let property = |bytes: &[u8], name: &str| {
            let (read, _) = pcf::read(bytes, &crate::ResourceLimits::default()).unwrap();
            read.properties.get(name).cloned()
        };
        let (explicit, report) = pcf::write(
            font,
            &PcfWriteOptions {
                metadata_precedence: MetadataPrecedence::ExplicitProperties,
                ..PcfWriteOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            property(&explicit, "POINT_SIZE"),
            Some(PropertyValue::Integer(120))
        );
        assert_eq!(
            property(&explicit, "RESOLUTION_X"),
            Some(PropertyValue::Integer(72))
        );
        // Absent RESOLUTION is generated from the projected 75 dpi.
        assert_eq!(
            property(&explicit, "RESOLUTION"),
            Some(PropertyValue::Integer(103))
        );
        assert_eq!(
            property(&explicit, "FONT"),
            Some(PropertyValue::from(projection.font_name.unwrap()))
        );
        let precedence: Vec<_> = report
            .entries
            .iter()
            .filter(|e| e.code == "pcf.write.property_precedence")
            .collect();
        assert_eq!(precedence.len(), 3);
        assert!(precedence.iter().all(|e| e.impact == Impact::Loss));
        let (normalized, report) = pcf::write(font, &PcfWriteOptions::default()).unwrap();
        assert_eq!(
            property(&normalized, "POINT_SIZE"),
            Some(PropertyValue::Integer(130))
        );
        assert_eq!(
            property(&normalized, "RESOLUTION_X"),
            Some(PropertyValue::Integer(75))
        );
        assert!(
            !report
                .entries
                .iter()
                .any(|e| e.code == "pcf.write.property_precedence")
        );
        // BDF output: SIZE follows the projection, properties and COMMENT stay.
        let (bdf, _) = crate::formats::bdf::write(font, &Default::default()).unwrap();
        let text = String::from_utf8(bdf).unwrap();
        assert!(text.contains("\nFONT -opf-demo-medium-r-normal--13-130-75-75-p-130-iso10646-1\n"));
        assert!(text.contains("\nSIZE 13 75 75\n"));
        assert!(text.contains("COMMENT \"(c) kept notice\""));
        assert!(text.contains("\nPOINT_SIZE 120\n"));
        assert!(text.contains("\nSWIDTH 462 0\n"));
    }
}
