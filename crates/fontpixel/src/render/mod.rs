//! Bounded text layout and rendering following bdfparser's `Font.draw`/`drawall`.
//!
//! Defaults reproduce SPEC-v3, including its width-based vertical line breaking,
//! full font cells, and missing-glyph zero advance. Layout stores borrowed glyph
//! references, then blits clipped packed rows into one output bitmap; no glyph
//! bitmap is cloned or allocated. Newlines are ordinary characters, as in SPEC.
//!
//! ```
//! use fontpixel::{Bitmap, Font, Glyph};
//! use fontpixel::bitmap::Bounds;
//! use fontpixel::model::{Codepoint, Vector};
//! use fontpixel::render::RenderOptions;
//! let mut font = Font::default();
//! font.metrics.bounds = Some(Bounds::new(3, 2, 0, 0));
//! let mut glyph = Glyph::new(".#.\n###".parse::<Bitmap>()?);
//! glyph.codepoints.push(Codepoint::Unicode('A' as u32));
//! glyph.metrics.advance = Some(Vector { x: 3, y: 0 });
//! font.add_glyph(glyph)?;
//! let rendered = font.draw("AA", &RenderOptions::default())?;
//! assert_eq!(rendered.bitmap.width(), 6);
//! # Ok::<(), fontpixel::Error>(())
//! ```

use crate::bitmap::{Bitmap, BitmapLimits, Bounds, CompositeMode, Direction};
use crate::model::{Codepoint, Font, Glyph, GlyphBitmap, GlyphId};
use crate::report::{Impact, Level, Location};
use crate::{Error, Report, ReportEntry, Result};
use std::{collections::HashSet, str::FromStr};

mod image;
#[cfg(feature = "png")]
pub use image::to_png;
pub use image::{ImageOptions, Rgba, to_svg};

/// Independent glyph and line progression. All sixteen combinations are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct WritingDirection {
    /// Progression within one line/column.
    pub glyphs: Direction,
    /// Progression between lines/columns.
    pub lines: Direction,
}
impl Default for WritingDirection {
    fn default() -> Self {
        Self {
            glyphs: Direction::Right,
            lines: Direction::Down,
        }
    }
}
impl WritingDirection {
    /// Parse SPEC shortcuts and four-letter pairs, falling back to `lrtb` for
    /// invalid strings. Like Python, a valid first four letters ignore a suffix.
    /// Use `FromStr` when invalid user input should be rejected instead.
    pub fn from_spec(value: &str) -> Self {
        let shortcut = match value {
            "lr" => "lrtb",
            "rl" => "rltb",
            "tb" => "tbrl",
            "bt" => "btrl",
            v => v,
        };
        shortcut
            .get(..4)
            .and_then(|v| v.parse().ok())
            .unwrap_or_default()
    }
}
impl FromStr for WritingDirection {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        let value = match value {
            "lr" => "lrtb",
            "rl" => "rltb",
            "tb" => "tbrl",
            "bt" => "btrl",
            v => v,
        };
        let part = |s| match s {
            "lr" => Some(Direction::Right),
            "rl" => Some(Direction::Left),
            "tb" => Some(Direction::Down),
            "bt" => Some(Direction::Up),
            _ => None,
        };
        if value.len() == 4
            && let (Some(glyphs), Some(lines)) =
                (value.get(..2).and_then(part), value.get(2..).and_then(part))
        {
            return Ok(Self { glyphs, lines });
        }
        Err(invalid(
            "direction must be lr, rl, tb, bt, or two of those pairs",
        ))
    }
}

/// Spacing mode. SPEC mode 0 concatenates font cells, not tight ink bounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RenderMode {
    /// Place font cells using DWIDTH/DWIDTH1 (SPEC mode 1).
    #[default]
    Advance,
    /// Concatenate complete font cells (SPEC mode 0 and other non-1 integers).
    Cell,
    /// Native extension: concatenate tight nonzero ink rectangles.
    Ink,
}

/// Policy for absent or unrepresentable input codes.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MissingGlyph<'a> {
    /// SPEC blank font cell, with an explicit zero advance.
    #[default]
    Blank,
    /// Fail rather than replacing a requested glyph.
    Error,
    /// Draw a borrowed replacement using the target font's cell and metrics.
    Glyph(&'a Glyph),
}
/// Accepts `"blank"` and `"error"`. A `{"glyph": ...}` replacement serializes,
/// but cannot deserialize into a borrow and is rejected with an explicit error;
/// callers supply the replacement glyph separately.
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for MissingGlyph<'_> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Raw {
            Blank,
            Error,
            Glyph(serde::de::IgnoredAny),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Blank => Ok(Self::Blank),
            Raw::Error => Ok(Self::Error),
            Raw::Glyph(_) => Err(serde::de::Error::custom(
                "a borrowed replacement glyph cannot be deserialized; \
                 pass the replacement glyph separately",
            )),
        }
    }
}

/// Limits for layout, rendering and diagnostic bookkeeping.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct RenderLimits {
    /// Maximum number of input characters/codes (including missing ones).
    pub max_glyphs: usize,
    /// Combined placement/line bookkeeping budget, separate from result pixels.
    pub max_layout_bytes: usize,
    /// Diagnostic entries, deduplication keys and text budget, independent of layout.
    pub max_report_bytes: usize,
    /// Output bitmap dimensions, pixel count and byte budget.
    pub bitmap: BitmapLimits,
}
impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_glyphs: 1_000_000,
            max_layout_bytes: 128 * 1024 * 1024,
            max_report_bytes: 16 * 1024 * 1024,
            bitmap: BitmapLimits::default(),
        }
    }
}

/// Text rendering controls. No implicit shaping, bidi reordering or line-feed
/// interpretation is performed; input order is placed in the selected direction.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct RenderOptions<'a> {
    /// Maximum accumulated width-plus-offset per line, matching SPEC exactly.
    pub line_limit: i64,
    /// Advance, cell or explicit tight-ink concatenation.
    pub mode: RenderMode,
    /// Glyph and line progression.
    pub direction: WritingDirection,
    /// Use the arriving glyph's advance for each boundary rather than the prior one.
    pub use_current_glyph_spacing: bool,
    /// Additional signed gap between glyphs; included in line-limit accounting.
    pub letter_spacing: i64,
    /// Additional signed gap between completed lines/columns.
    pub line_spacing: i64,
    /// Missing-code behavior (default: SPEC empty cell).
    pub missing: MissingGlyph<'a>,
    /// Optional explicit Gray8-to-Mono threshold. `None` rejects grayscale input.
    pub gray_threshold: Option<u8>,
    /// Reject clipping, missing replacements and grayscale conversion reports.
    pub strict: bool,
    /// Work and output budgets.
    pub limits: RenderLimits,
}
impl Default for RenderOptions<'_> {
    fn default() -> Self {
        Self {
            line_limit: 512,
            mode: RenderMode::Advance,
            direction: WritingDirection::default(),
            use_current_glyph_spacing: false,
            letter_spacing: 0,
            line_spacing: 0,
            missing: MissingGlyph::Blank,
            gray_threshold: None,
            strict: false,
            limits: RenderLimits::default(),
        }
    }
}

/// Ordering for `draw_all`; file order means last effective occurrence of each code, matching Python's BDF parser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GlyphOrder {
    /// Numeric/domain ascending order (SPEC 1).
    #[default]
    Ascending,
    /// Numeric/domain descending order (SPEC 2).
    Descending,
    /// Effective source-code occurrence (SPEC 0).
    File,
    /// Reverse effective source-code occurrence (SPEC -1).
    ReverseFile,
}

/// Result pixels and deterministic diagnostics. Missing and clipped ink are
/// reported even though those are valid SPEC rendering operations.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Rendered {
    /// Packed monochrome result, top-left origin.
    pub bitmap: Bitmap,
    /// Warnings, repairs and information loss.
    pub report: Report,
}

impl Font {
    /// Render Unicode text through the font's exact charset map. Missing mappings
    /// follow `options.missing`; CJK-disabled mappings fail instead of approximating.
    pub fn draw(&self, text: &str, options: &RenderOptions<'_>) -> Result<Rendered> {
        let codec = self.charset.codec().map_err(|e| invalid(&e.to_string()))?;
        let unicode = codec.kind() == crate::charset::Kind::Unicode;
        let codes = text.chars().map(|c| {
            // Explicit Unicode aliases remain usable in mixed-domain native fonts.
            if self.glyph_id(Codepoint::Unicode(c as u32)).is_some() {
                Some(Codepoint::Unicode(c as u32))
            } else {
                codec.char2cp(c).map(|v| {
                    if unicode {
                        Codepoint::Unicode(v)
                    } else {
                        Codepoint::Legacy(v)
                    }
                })
            }
        });
        render(self, codes, options, (!codec.known()).then(|| codec.name()))
    }

    /// Render explicitly tagged codes. `None` represents an unencodable character.
    /// Every input is accounted for, including missing glyphs and zero-size cells.
    pub fn draw_codepoints(
        &self,
        codes: impl IntoIterator<Item = Option<Codepoint>>,
        options: &RenderOptions<'_>,
    ) -> Result<Rendered> {
        render(self, codes, options, None)
    }

    /// Render effective encoded mappings only, optionally restricted to the union
    /// of inclusive numeric ranges. An empty range slice selects every code.
    /// For SPEC's drawall defaults use `RenderOptions { mode: RenderMode::Cell, ..Default::default() }`.
    pub fn draw_all(
        &self,
        order: GlyphOrder,
        ranges: &[(u32, u32)],
        options: &RenderOptions<'_>,
    ) -> Result<Rendered> {
        let selected = |cp: Codepoint| {
            ranges.is_empty()
                || ranges
                    .iter()
                    .any(|&(a, b)| a <= cp.value() && cp.value() <= b)
        };
        match order {
            GlyphOrder::Ascending => self.draw_codepoints(
                self.codepoints()
                    .filter(|&(cp, _)| selected(cp))
                    .map(|(cp, _)| Some(cp)),
                options,
            ),
            GlyphOrder::Descending => self.draw_codepoints(
                self.codepoints()
                    .rev()
                    .filter(|&(cp, _)| selected(cp))
                    .map(|(cp, _)| Some(cp)),
                options,
            ),
            GlyphOrder::File | GlyphOrder::ReverseFile => {
                // BDF parser deletes/reinserts duplicate codes; the last position wins.
                let mut codes = Vec::new();
                for (id, glyph) in self.glyphs() {
                    for (index, &cp) in glyph.codepoints.iter().enumerate() {
                        if selected(cp)
                            && self.glyph_id(cp) == Some(id)
                            && !glyph.codepoints[..index].contains(&cp)
                        {
                            reserve(&mut codes, options.limits)?;
                            codes.push(Some(cp));
                        }
                    }
                }
                if order == GlyphOrder::ReverseFile {
                    codes.reverse();
                }
                let mut options = options.clone();
                options.limits.max_layout_bytes = options
                    .limits
                    .max_layout_bytes
                    .saturating_sub(codes.capacity() * std::mem::size_of::<Option<Codepoint>>());
                self.draw_codepoints(codes, &options)
            }
        }
    }
}

fn invalid(s: &str) -> Error {
    Error::InvalidInput(s.into())
}
fn budget(resource: &'static str, limit: usize) -> Error {
    Error::ResourceLimit { resource, limit }
}
fn reserve<T>(items: &mut Vec<T>, limits: RenderLimits) -> Result<()> {
    if items.len() >= limits.max_glyphs {
        return Err(budget("render glyph count", limits.max_glyphs));
    }
    let bytes = (items.len() + 1)
        .checked_mul(std::mem::size_of::<T>())
        .ok_or_else(|| budget("render layout bytes", limits.max_layout_bytes))?;
    if bytes > limits.max_layout_bytes {
        return Err(budget("render layout bytes", limits.max_layout_bytes));
    }
    if items.len() == items.capacity() {
        let maximum = limits
            .max_glyphs
            .min(limits.max_layout_bytes / std::mem::size_of::<T>().max(1));
        let capacity = items.capacity().saturating_mul(2).max(16).min(maximum);
        items
            .try_reserve_exact(capacity - items.len())
            .map_err(|_| budget("render layout allocation", limits.max_layout_bytes))?;
    }
    Ok(())
}
fn horizontal(d: Direction) -> bool {
    matches!(d, Direction::Right | Direction::Left)
}
fn forward(d: Direction) -> bool {
    matches!(d, Direction::Right | Direction::Down)
}
fn dimension(n: i128, limits: BitmapLimits) -> Result<u32> {
    if n < 0 || n > i128::from(limits.max_dimension) {
        return Err(budget("bitmap dimension", limits.max_dimension as usize));
    }
    Ok(n as u32)
}

#[derive(Clone, Copy)]
struct Placement<'a> {
    glyph: Option<&'a Glyph>,
    cell: Bounds,
    offset: i128,
    along: i128,
}
struct Line {
    begin: usize,
    end: usize,
    lo: i128,
    width: u32,
    height: u32,
    along: i128,
}
#[derive(Default)]
struct Extent {
    lo: i128,
    hi: i128,
    cross: u32,
    kept: bool,
}
impl Extent {
    fn add(
        &mut self,
        width: u32,
        height: u32,
        gap: i128,
        direction: Direction,
        limits: BitmapLimits,
    ) -> Result<i128> {
        if !horizontal(direction) && (width == 0 || height == 0) {
            return Ok(0);
        }
        let (length, cross) = if horizontal(direction) {
            (width, height)
        } else {
            (height, width)
        };
        let gap = if self.kept { gap } else { 0 };
        let start = if forward(direction) {
            self.hi + gap
        } else {
            self.lo - gap - i128::from(length)
        };
        self.lo = self.lo.min(start);
        self.hi = self.hi.max(start + i128::from(length));
        self.cross = self.cross.max(cross);
        self.kept = true;
        dimension(self.hi - self.lo, limits)?;
        dimension(i128::from(self.cross), limits)?;
        Ok(start)
    }
    fn size(&self, direction: Direction, limits: BitmapLimits) -> Result<(u32, u32)> {
        let n = dimension(self.hi - self.lo, limits)?;
        let (w, h) = if horizontal(direction) {
            (n, self.cross)
        } else {
            (self.cross, n)
        };
        // SPEC's [] has width zero even if a requested cell had nonzero width.
        Ok((if h == 0 { 0 } else { w }, h))
    }
}

type DiagnosticKey = (&'static str, Option<usize>, Option<u32>);

#[derive(Default)]
struct Diagnostics {
    report: Report,
    seen: HashSet<DiagnosticKey>,
    text_bytes: usize,
}

fn diagnostic(
    diagnostics: &mut Diagnostics,
    code: &'static str,
    id: Option<GlyphId>,
    cp: Option<Codepoint>,
    impact: Impact,
    message: impl FnOnce() -> String,
    limits: RenderLimits,
) -> Result<()> {
    let glyph_index = id.map(GlyphId::index);
    let codepoint = cp.map(Codepoint::value);
    let key = (code, glyph_index, codepoint);
    if diagnostics.seen.contains(&key) {
        return Ok(());
    }
    // Messages have bounded formatting at their call sites. Charge a conservative
    // allowance for Vec growth, hash-table buckets/control bytes and owned text
    // before allocating either collection; no table is allocated for clean input.
    let message = message();
    let text_bytes = diagnostics
        .text_bytes
        .checked_add(message.capacity())
        .and_then(|n| n.checked_add(code.len()))
        .ok_or_else(|| budget("render diagnostic bytes", limits.max_report_bytes))?;
    let count = diagnostics.report.entries.len() + 1;
    let entry_budget =
        2 * std::mem::size_of::<ReportEntry>() + 4 * (std::mem::size_of::<DiagnosticKey>() + 1);
    let required = count
        .max(4)
        .checked_mul(entry_budget)
        .and_then(|n| n.checked_add(text_bytes))
        .ok_or_else(|| budget("render diagnostic bytes", limits.max_report_bytes))?;
    if required > limits.max_report_bytes {
        return Err(budget("render diagnostic bytes", limits.max_report_bytes));
    }
    diagnostics
        .report
        .entries
        .try_reserve(1)
        .map_err(|_| budget("render diagnostic allocation", limits.max_report_bytes))?;
    diagnostics
        .seen
        .try_reserve(1)
        .map_err(|_| budget("render diagnostic allocation", limits.max_report_bytes))?;
    diagnostics.seen.insert(key);
    diagnostics.text_bytes = text_bytes;
    diagnostics.report.push(ReportEntry {
        level: Level::Warning,
        impact,
        code: code.into(),
        location: Location {
            glyph_index,
            codepoint,
            ..Location::default()
        },
        message,
    });
    Ok(())
}

fn ink_bounds(glyph: &Glyph, threshold: Option<u8>) -> Result<Bounds> {
    let GlyphBitmap::Gray8(bitmap) = &glyph.bitmap else {
        return Ok(glyph.bitmap.bbox());
    };
    let threshold =
        threshold.ok_or_else(|| invalid("grayscale glyph requires an explicit gray_threshold"))?;
    let (mut left, mut right, mut top, mut bottom) = (bitmap.width(), 0, bitmap.height(), 0);
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            if bitmap.get_pixel(x, y)? >= threshold {
                left = left.min(x);
                right = right.max(x + 1);
                top = top.min(y);
                bottom = y + 1;
            }
        }
    }
    Ok(if top == bitmap.height() {
        Bounds::default()
    } else {
        Bounds::new(
            right - left,
            bottom - top,
            i64::from(left),
            i64::from(bitmap.height() - bottom),
        )
    })
}

fn prepare<'a>(
    font: &'a Font,
    cp: Option<Codepoint>,
    options: &'a RenderOptions<'a>,
    report: &mut Diagnostics,
) -> Result<Placement<'a>> {
    let id = cp.and_then(|cp| font.glyph_id(cp));
    let glyph = match id.and_then(|id| font.glyph(id)) {
        Some(g) => Some(g),
        None => {
            if matches!(options.missing, MissingGlyph::Error) {
                return Err(invalid(&format!("missing glyph for code {cp:?}")));
            }
            diagnostic(
                report,
                "render.missing_glyph",
                None,
                cp,
                Impact::Loss,
                || {
                    format!(
                        "Missing glyph {cp:?}; used {}",
                        if matches!(options.missing, MissingGlyph::Blank) {
                            "an empty font cell"
                        } else {
                            "the replacement glyph"
                        }
                    )
                },
                options.limits,
            )?;
            match options.missing {
                MissingGlyph::Glyph(g) => Some(g),
                _ => None,
            }
        }
    };
    let mut cell = font.metrics.bounds.unwrap_or_default();
    if let Some(g) = glyph {
        g.validate()?;
        #[cfg(any(feature = "bdf", feature = "pcf"))]
        crate::formats::x11::validate_bitmap(g)?;
        if matches!(g.bitmap, GlyphBitmap::Gray8(_)) {
            let threshold = options
                .gray_threshold
                .ok_or_else(|| invalid("grayscale glyph requires an explicit gray_threshold"))?;
            diagnostic(
                report,
                "render.grayscale_threshold",
                id,
                cp,
                Impact::Loss,
                || format!("Converted Gray8 glyph to Mono with threshold {threshold}"),
                options.limits,
            )?;
        }
        let ink = ink_bounds(g, options.gray_threshold)?;
        if options.mode == RenderMode::Ink {
            cell = Bounds::new(
                ink.width,
                ink.height,
                g.metrics.bounds.x.saturating_add(ink.x),
                g.metrics.bounds.y.saturating_add(ink.y),
            );
        } else if ink.width != 0 && ink.height != 0 {
            let left = i128::from(g.metrics.bounds.x) + i128::from(ink.x);
            let bottom = i128::from(g.metrics.bounds.y) + i128::from(ink.y);
            if left < i128::from(cell.x)
                || bottom < i128::from(cell.y)
                || left + i128::from(ink.width) > i128::from(cell.x) + i128::from(cell.width)
                || bottom + i128::from(ink.height) > i128::from(cell.y) + i128::from(cell.height)
            {
                diagnostic(
                    report,
                    "render.cell_clipping",
                    id,
                    cp,
                    Impact::Loss,
                    || {
                        "Ink outside the font bounding box was clipped, as required by SPEC font-cell drawing".into()
                    },
                    options.limits,
                )?;
            }
        }
        #[cfg(any(feature = "bdf", feature = "pcf"))]
        if crate::formats::x11::unchanged_bitmap(g)
            && let Some(rows) = crate::formats::x11::source_rows(g)
        {
            let n = rows.count();
            if n < g.metrics.bounds.height as usize {
                diagnostic(
                    report,
                    "render.short_bitmap",
                    id,
                    cp,
                    Impact::Repair,
                    || {
                        format!(
                            "Glyph \"{:.200}\" (codepoint {}): has {n} bitmap rows but its BBX height is {}; padded with empty rows",
                            g.name.as_deref().unwrap_or("None"),
                            cp.map_or(-1, |v| i64::from(v.value())),
                            g.metrics.bounds.height
                        )
                    },
                    options.limits,
                )?;
            }
        }
    } else if options.mode == RenderMode::Ink {
        cell = Bounds::default();
    }
    let mut offset = i128::from(options.letter_spacing);
    if options.mode == RenderMode::Advance {
        let h = horizontal(options.direction.glyphs);
        let vector =
            |g: &crate::model::GlyphMetrics| if h { g.advance } else { g.vertical_advance };
        // Python's glyph x-or-y differs from its global x-if-present rule.
        let advance = match glyph {
            None => Some(0),
            Some(g) => vector(&g.metrics)
                .map(|v| if v.x != 0 { v.x } else { v.y })
                .or_else(|| vector(&font.metrics.glyph_defaults).map(|v| v.x)),
        };
        if let Some(advance) = advance {
            offset += i128::from(advance) - i128::from(if h { cell.width } else { cell.height });
        }
    }
    if cell.height == 0 {
        cell.width = 0;
    }
    Ok(Placement {
        glyph,
        cell,
        offset,
        along: 0,
    })
}

fn render(
    font: &Font,
    codes: impl IntoIterator<Item = Option<Codepoint>>,
    options: &RenderOptions<'_>,
    unknown_charset: Option<&str>,
) -> Result<Rendered> {
    let layout_slots = options.limits.max_layout_bytes
        / (std::mem::size_of::<Placement<'_>>() + std::mem::size_of::<Line>());
    let placement_limits = RenderLimits {
        max_layout_bytes: layout_slots * std::mem::size_of::<Placement<'_>>(),
        ..options.limits
    };
    let mut report = Diagnostics::default();
    if let Some(name) = unknown_charset {
        let name = name
            .char_indices()
            .nth(200)
            .map_or(name, |(i, _)| &name[..i]);
        diagnostic(
            &mut report,
            "render.unknown_charset",
            None,
            None,
            Impact::None,
            || format!("Unknown charset {name:?}; using Unicode identity"),
            options.limits,
        )?;
    }
    let mut placements = Vec::new();
    let mut lines = Vec::new();
    let mut size = 0i128;
    let mut begin = 0;
    let mut extent = Extent::default();
    let mut previous_offset = 0;
    for cp in codes {
        if placements.len() >= layout_slots {
            return Err(budget(
                "render layout bytes",
                options.limits.max_layout_bytes,
            ));
        }
        reserve(&mut placements, placement_limits)?;
        let mut p = prepare(font, cp, options, &mut report)?;
        let cost = i128::from(p.cell.width) + p.offset;
        if size + cost > i128::from(options.line_limit) {
            if placements.len() == begin {
                return Err(invalid(&format!(
                    "line_limit ({}) cannot contain one glyph: code {cp:?}, width {}",
                    options.line_limit, p.cell.width
                )));
            }
            finish_line(&mut lines, begin, placements.len(), &extent, options)?;
            begin = placements.len();
            extent = Extent::default();
            size = 0;
            if cost > i128::from(options.line_limit) {
                return Err(invalid(&format!(
                    "line_limit ({}) cannot contain one glyph: code {cp:?}, width {}",
                    options.line_limit, p.cell.width
                )));
            }
        }
        let gap = if options.use_current_glyph_spacing {
            p.offset
        } else {
            previous_offset
        };
        p.along = extent.add(
            p.cell.width,
            p.cell.height,
            gap,
            options.direction.glyphs,
            options.limits.bitmap,
        )?;
        size += cost;
        previous_offset = p.offset;
        placements.push(p);
    }
    if placements.len() != begin {
        finish_line(&mut lines, begin, placements.len(), &extent, options)?;
    }
    let used = placements
        .len()
        .checked_mul(std::mem::size_of::<Placement<'_>>())
        .and_then(|n| n.checked_add(lines.len() * std::mem::size_of::<Line>()))
        .ok_or_else(|| budget("render layout bytes", options.limits.max_layout_bytes))?;
    if used > options.limits.max_layout_bytes {
        return Err(budget(
            "render layout bytes",
            options.limits.max_layout_bytes,
        ));
    }
    let mut all = Extent::default();
    for line in &mut lines {
        line.along = all.add(
            line.width,
            line.height,
            i128::from(options.line_spacing),
            options.direction.lines,
            options.limits.bitmap,
        )?;
    }
    let (width, height) = all.size(options.direction.lines, options.limits.bitmap)?;
    let report = report.report;
    if options.strict && report.has_loss() {
        return Err(Error::StrictLoss(report));
    }
    let mut bitmap = Bitmap::with_limits(width, height, options.limits.bitmap)?;
    for line in &lines {
        if line.width == 0 || line.height == 0 {
            continue;
        }
        let line_cross_end = if horizontal(options.direction.lines) {
            matches!(options.direction.glyphs, Direction::Right | Direction::Up)
        } else {
            matches!(options.direction.glyphs, Direction::Left | Direction::Down)
        };
        let cross = if horizontal(options.direction.lines) {
            line.height
        } else {
            line.width
        };
        let across = if line_cross_end { all.cross - cross } else { 0 };
        let (lx, ly) = if horizontal(options.direction.lines) {
            (line.along - all.lo, i128::from(across))
        } else {
            (i128::from(across), line.along - all.lo)
        };
        for p in &placements[line.begin..line.end] {
            let Some(glyph) = p.glyph else {
                continue;
            };
            if p.cell.width == 0 || p.cell.height == 0 {
                continue;
            }
            let glyph_cross_end = if horizontal(options.direction.glyphs) {
                matches!(options.direction.lines, Direction::Down | Direction::Left)
            } else {
                matches!(options.direction.lines, Direction::Right | Direction::Up)
            };
            let across = if glyph_cross_end {
                if horizontal(options.direction.glyphs) {
                    line.height - p.cell.height
                } else {
                    line.width - p.cell.width
                }
            } else {
                0
            };
            let (x, y) = if horizontal(options.direction.glyphs) {
                (p.along - line.lo, i128::from(across))
            } else {
                (i128::from(across), p.along - line.lo)
            };
            blit_cell(
                &mut bitmap,
                glyph,
                p.cell,
                lx + x,
                ly + y,
                options.gray_threshold,
            )?;
        }
    }
    Ok(Rendered { bitmap, report })
}
fn finish_line(
    lines: &mut Vec<Line>,
    begin: usize,
    end: usize,
    extent: &Extent,
    options: &RenderOptions<'_>,
) -> Result<()> {
    let (width, height) = extent.size(options.direction.glyphs, options.limits.bitmap)?;
    let slots = options.limits.max_layout_bytes
        / (std::mem::size_of::<Placement<'_>>() + std::mem::size_of::<Line>());
    reserve(
        lines,
        RenderLimits {
            max_layout_bytes: slots * std::mem::size_of::<Line>(),
            ..options.limits
        },
    )?;
    lines.push(Line {
        begin,
        end,
        lo: extent.lo,
        width,
        height,
        along: 0,
    });
    Ok(())
}

fn blit_cell(
    output: &mut Bitmap,
    glyph: &Glyph,
    cell: Bounds,
    x: i128,
    y: i128,
    threshold: Option<u8>,
) -> Result<()> {
    let b = glyph.metrics.bounds;
    let dx = i128::from(b.x) - i128::from(cell.x);
    let dy = i128::from(cell.height) - i128::from(b.height) + i128::from(cell.y) - i128::from(b.y);
    let left = dx.max(0);
    let top = dy.max(0);
    let right = (dx + i128::from(b.width)).min(i128::from(cell.width));
    let bottom = (dy + i128::from(b.height)).min(i128::from(cell.height));
    if left >= right || top >= bottom {
        return Ok(());
    }
    let sx = (left - dx) as u32;
    let sy = (top - dy) as u32;
    let w = (right - left) as u32;
    let h = (bottom - top) as u32;
    let tx = i64::try_from(x + left).map_err(|_| invalid("render x coordinate overflow"))?;
    let ty = i64::try_from(y + top).map_err(|_| invalid("render y coordinate overflow"))?;
    match &glyph.bitmap {
        GlyphBitmap::Mono(b) => output.composite_region(
            b,
            Bounds::new(w, h, i64::from(sx), i64::from(sy)),
            tx,
            ty,
            CompositeMode::Over,
        ),
        GlyphBitmap::Gray8(b) => {
            let threshold = threshold
                .ok_or_else(|| invalid("grayscale glyph requires an explicit gray_threshold"))?;
            for row in 0..h {
                for col in 0..w {
                    if b.get_pixel(sx + col, sy + row)? >= threshold {
                        let px = i128::from(tx) + i128::from(col);
                        let py = i128::from(ty) + i128::from(row);
                        if px >= 0
                            && py >= 0
                            && px < i128::from(output.width())
                            && py < i128::from(output.height())
                        {
                            output.set_pixel(px as u32, py as u32, 1)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
