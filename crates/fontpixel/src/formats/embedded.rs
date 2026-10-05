//! Native FontSet projection for bitmap-only sfnt, Apple sbits, and color bitmaps.
//!
//! These hooks extract physical strikes. They do not claim general vector font
//! rasterization or resource-fork support. Low-level editable tables live in
//! [`crate::sfnt::bitmap`].
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds};
use crate::model::{
    Codepoint, Font, FontNames, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Spacing,
    Vector,
};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::sfnt::{
    self, Flavor, Sfnt, SfntBuilder, bitmap as sbit, cmap, metrics, name, os2, post,
};
use crate::{Report, Result};
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

mod metadata;
pub use metadata::{
    EmbeddedAverageWidth, EmbeddedCmapUnicodeFull, EmbeddedIndexLayout, EmbeddedMetadataOptions,
    EmbeddedPostNames, EmbeddedUnicodeRanges, EmbeddedWidthMax,
};

/// Options for bitmap FontSet extraction.
#[derive(Clone, Debug, Default)]
pub struct EmbeddedReadOptions {
    /// All sfnt, expansion and image budgets.
    pub limits: sbit::Limits,
    /// Color coverage is alpha by default; true uses alpha-weighted darkness.
    pub color_darkness: bool,
    /// Explicit Gray8-to-Mono threshold. None retains exact gray coverage.
    pub gray_threshold: Option<u8>,
}
/// Options for native OTB/bdat output.
#[derive(Clone, Debug, Default)]
pub struct EmbeddedWriteOptions {
    /// All table, bitmap, glyph and output budgets.
    pub limits: sbit::Limits,
    /// Explicit EBSC instructions; an empty list uses retained source EBSC, if any.
    pub scaling: Vec<sbit::Scale>,
    /// Explicit design units per em in 16..=16384; None preserves automatic scaling.
    /// Supported for OTB/bdat, not color output.
    pub units_per_em: Option<u16>,
    /// Source priority for common design metrics; physical strike order is unchanged.
    pub reference_strike: EmbeddedReferenceStrike,
    /// Pixel-to-design scaling only; the OS/2 average retains ties-up rounding.
    pub rounding: EmbeddedRounding,
    /// Explicit head/OS/2/post/name/strike-table metadata; the default keeps
    /// native output byte for byte.
    pub metadata: EmbeddedMetadataOptions,
}
/// Source of shared design metrics for native OTB/bdat output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmbeddedReferenceStrike {
    /// Use the first source strike, then original input order for missing glyphs.
    #[default]
    First,
    /// Greatest effective y-ppem, with the last original input winning ties.
    /// Missing glyphs use descending y-ppem, then descending original input index.
    LargestPpem,
    /// Zero-based FontSet strike index, then all other strikes in original order.
    Index(usize),
}
/// Rounding of signed pixel metrics scaled into design units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmbeddedRounding {
    /// Nearest integer; exact halfway values go away from zero (existing default).
    #[default]
    NearestTiesAwayFromZero,
    /// Nearest integer; exact halfway values go to the even integer.
    /// Uses exact rational arithmetic, not floating-point intermediate values.
    NearestTiesEven,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct DesignOptions {
    units_per_em: Option<u16>,
    reference_strike: EmbeddedReferenceStrike,
    rounding: EmbeddedRounding,
}
impl From<&EmbeddedWriteOptions> for DesignOptions {
    fn from(options: &EmbeddedWriteOptions) -> Self {
        Self {
            units_per_em: options.units_per_em,
            reference_strike: options.reference_strike,
            rounding: options.rounding,
        }
    }
}
impl DesignOptions {
    fn validate(self, fonts: usize, kind: sbit::Kind) -> Result<()> {
        if self
            .units_per_em
            .is_some_and(|upem| !(16..=16384).contains(&upem))
        {
            return Err(invalid("units_per_em must be in 16..=16384"));
        }
        if let EmbeddedReferenceStrike::Index(index) = self.reference_strike
            && index >= fonts
        {
            return Err(invalid("reference strike index is outside FontSet"));
        }
        if kind == sbit::Kind::Color && self != Self::default() {
            return Err(invalid(
                "custom embedded metric controls require otb or bdat",
            ));
        }
        Ok(())
    }
    fn priority(self, strikes: &[DesignStrike], limits: &sfnt::Limits) -> Result<Vec<usize>> {
        if matches!(
            self.reference_strike,
            EmbeddedReferenceStrike::First | EmbeddedReferenceStrike::Index(0)
        ) {
            return Ok(Vec::new());
        }
        let bytes = strikes
            .len()
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or_else(|| invalid("metric-order size overflow"))?;
        sfnt::bounded(
            bytes,
            limits.resources.max_total_bytes,
            "embedded metric-order bytes",
        )?;
        let mut order = Vec::new();
        order
            .try_reserve_exact(strikes.len())
            .map_err(|_| invalid("metric-order allocation failed"))?;
        order.extend(0..strikes.len());
        match self.reference_strike {
            EmbeddedReferenceStrike::LargestPpem => {
                order.sort_unstable_by_key(|&index| {
                    std::cmp::Reverse((strikes[index].ppem[1], index))
                });
            }
            EmbeddedReferenceStrike::Index(index) => order[..=index].rotate_right(1),
            EmbeddedReferenceStrike::First => unreachable!(),
        }
        Ok(order)
    }
}
// Positive denominators come from validated physical ppem. Remainders, unlike
// the numerator's absolute value, remain safe even for the most negative i128.
fn round_design(numerator: i128, denominator: i128, rounding: EmbeddedRounding) -> i128 {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let twice = remainder.abs() * 2;
    let up = twice > denominator
        || (twice == denominator
            && (rounding == EmbeddedRounding::NearestTiesAwayFromZero || quotient % 2 != 0));
    quotient + if up { numerator.signum() } else { 0 }
}
/// Native color bitmap container, with black RGB and exact coverage alpha.
#[cfg(feature = "color")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorFlavor {
    /// CBDT/CBLC version 3, with exact per-strike pixel metrics.
    #[default]
    Cbdt,
    /// sbix version 1, with shared design-space horizontal advances.
    Sbix,
}
/// Options for native color bitmap output.
#[cfg(feature = "color")]
#[derive(Clone, Debug, Default)]
pub struct ColorWriteOptions {
    /// Table, PNG, model, output and native readback budgets.
    pub limits: sbit::Limits,
    /// Output table family. The default is CBDT/CBLC.
    pub flavor: ColorFlavor,
}
/// Feature-selected registry hooks for all three embedded families.
#[derive(Clone, Debug)]
pub struct EmbeddedCodec {
    /// The intended bitmap table family.
    pub kind: sbit::Kind,
    /// Detailed extraction choices.
    pub read_options: EmbeddedReadOptions,
    /// Detailed OTB/bdat output choices.
    pub write_options: EmbeddedWriteOptions,
    /// Color container choice and budgets.
    #[cfg(feature = "color")]
    pub color_write_options: ColorWriteOptions,
}
impl EmbeddedCodec {
    /// Construct one family with default budgets and exact gray preservation.
    pub fn new(kind: sbit::Kind) -> Self {
        Self {
            kind,
            read_options: EmbeddedReadOptions::default(),
            write_options: EmbeddedWriteOptions::default(),
            #[cfg(feature = "color")]
            color_write_options: ColorWriteOptions::default(),
        }
    }
}
impl FormatDetector for EmbeddedCodec {
    fn detect(&self, input: &mut ReadContext<'_>) -> Result<Confidence> {
        let limits = sfnt::Limits {
            resources: input.limits().clone(),
            ..Default::default()
        };
        let Ok(font) = Sfnt::parse(input.bytes(), &limits) else {
            return Ok(Confidence::NoMatch);
        };
        if cfg!(feature = "ttf") && font.table(*b"glyf").is_some()
            || cfg!(feature = "otf")
                && (font.table(*b"CFF ").is_some() || font.table(*b"CFF2").is_some())
        {
            // Defer only to a canonical outline detector that is actually
            // installed; enabling a web wrapper also enables vector-raster,
            // but need not enable standalone TTF/OTF registry entries.
            return Ok(Confidence::NoMatch);
        }
        let (data, location) = self.kind.tags();
        let matches = font.table(data).is_some() && font.table(location).is_some()
            || self.kind == sbit::Kind::Color && font.table(*b"sbix").is_some();
        Ok(if matches {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for EmbeddedCodec {
    fn read(
        &self,
        input: &mut ReadContext<'_>,
        _options: &ReadOptions,
    ) -> Result<(FontSet, Report)> {
        let mut options = self.read_options.clone();
        options.limits.sfnt.resources = input.limits().clone();
        read(input.bytes(), self.kind, &options)
    }
}
impl FormatWriter<FontSet> for EmbeddedCodec {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        #[cfg(feature = "color")]
        if self.kind == sbit::Kind::Color {
            let (bytes, report) = write_color(fonts, &self.color_write_options)?;
            return Ok(Written {
                output: OutputBundle::single(options.primary_name.clone(), bytes),
                report,
            });
        }
        let (bytes, report) = write(fonts, self.kind, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(options.primary_name.clone(), bytes),
            report,
        })
    }
}
fn invalid(s: impl std::fmt::Display) -> crate::Error {
    crate::Error::InvalidInput(format!("embedded bitmap: {s}"))
}
fn u16at(data: &[u8], at: usize) -> Result<u16> {
    let b = data
        .get(at..at + 2)
        .ok_or_else(|| invalid("truncated common sfnt table"))?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}
#[cfg(feature = "color")]
fn u32at(data: &[u8], at: usize) -> Result<u32> {
    let b = data
        .get(at..at + 4)
        .ok_or_else(|| invalid("truncated common sfnt table"))?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
fn required<'a>(font: &Sfnt<'a>, tag: [u8; 4]) -> Result<&'a [u8]> {
    font.table(tag)
        .ok_or_else(|| invalid(format!("missing {}", String::from_utf8_lossy(&tag))))
}
// Diagnostics have their own pre-allocation budget. A sparse cmap can generate
// many missing-image notices even when there are almost no bitmap payload bytes.
struct Diagnostics {
    report: Report,
    bytes: usize,
    limit: usize,
    exhausted: bool,
}
impl Diagnostics {
    fn new(total: usize) -> Self {
        Self {
            report: Report::default(),
            bytes: 0,
            limit: (total / 4).min(64 * 1024 * 1024),
            exhausted: false,
        }
    }
    fn finish(self) -> Result<Report> {
        if self.exhausted {
            Err(crate::Error::ResourceLimit {
                resource: "embedded diagnostic bytes",
                limit: self.limit,
            })
        } else {
            Ok(self.report)
        }
    }
}
fn loss(
    report: &mut Diagnostics,
    code: &str,
    fi: Option<usize>,
    gi: Option<usize>,
    message: impl Into<String>,
) {
    push(
        report,
        (Level::Warning, Impact::Loss),
        code,
        Location {
            font_index: fi,
            glyph_index: gi,
            ..Default::default()
        },
        message.into(),
    );
}
/// Informational audit with no source information changed.
fn note(report: &mut Diagnostics, code: &str, message: &str) {
    push(
        report,
        (Level::Info, Impact::None),
        code,
        Location::default(),
        message.to_owned(),
    );
}
fn push(
    report: &mut Diagnostics,
    (level, impact): (Level, Impact),
    code: &str,
    location: Location,
    message: String,
) {
    let size = 256usize
        .saturating_add(message.len())
        .saturating_add(code.len());
    if report.exhausted || size > report.limit.saturating_sub(report.bytes) {
        report.exhausted = true;
        return;
    }
    report.bytes += size;
    report.report.push(ReportEntry {
        level,
        impact,
        code: format!("embedded.{code}"),
        location,
        message,
    });
}
fn family_id(kind: sbit::Kind) -> &'static str {
    match kind {
        sbit::Kind::OpenType => "otb",
        sbit::Kind::Apple => "bdat",
        sbit::Kind::Color => "color",
    }
}
pub(crate) fn names(table: Option<&[u8]>, limits: &sfnt::Limits) -> Result<FontNames> {
    let mut out = FontNames::default();
    if let Some(table) = table {
        let table = name::NameTable::decode(table, limits)?;
        // English Windows/Unicode preference, without assuming Macintosh bytes are UTF-8.
        let mut records: Vec<_> = table.records.iter().collect();
        records.sort_by_key(|r| (r.language_id != 0x409, r.platform_id != 3));
        for r in records {
            let field = match r.name_id {
                0 => &mut out.copyright,
                1 => &mut out.family,
                2 => &mut out.style,
                4 => &mut out.full,
                5 => &mut out.version,
                6 => &mut out.postscript,
                8 => &mut out.foundry,
                13 => &mut out.license,
                14 => &mut out.license_url,
                _ => continue,
            };
            if field.is_none() {
                *field = r.text()?;
            }
        }
    }
    Ok(out)
}
fn apply_names(
    g: &mut Glyph,
    gid: u16,
    mapping: &BTreeMap<u16, Vec<u32>>,
    post_names: Option<&[String]>,
) {
    if let Some(cps) = mapping.get(&gid) {
        g.codepoints
            .extend(cps.iter().copied().map(Codepoint::Unicode));
    }
    g.name = post_names
        .and_then(|n| n.get(gid as usize))
        .cloned()
        .or_else(|| (gid == 0).then(|| ".notdef".into()));
    g.properties
        .push(Property::new("sfnt.glyph_id", i64::from(gid)));
}
fn threshold(
    bitmap: GlyphBitmap,
    options: &EmbeddedReadOptions,
    report: &mut Diagnostics,
    fi: usize,
    gi: usize,
) -> Result<GlyphBitmap> {
    match (bitmap, options.gray_threshold) {
        (GlyphBitmap::Gray8(b), Some(t)) => {
            loss(
                report,
                "gray_threshold",
                Some(fi),
                Some(gi),
                format!("Gray coverage thresholded at {t}"),
            );
            Ok(GlyphBitmap::Mono(b.to_mono(t)?))
        }
        (b, _) => Ok(b),
    }
}
fn name_bytes(n: &FontNames) -> usize {
    [
        &n.family,
        &n.style,
        &n.full,
        &n.postscript,
        &n.version,
        &n.copyright,
        &n.license,
        &n.license_url,
        &n.foundry,
        &n.xlfd,
    ]
    .iter()
    .map(|v| v.as_ref().map_or(0, String::len))
    .sum()
}
fn charge(total: &mut usize, n: usize, limits: &sfnt::Limits) -> Result<()> {
    *total = total
        .checked_add(n)
        .ok_or_else(|| invalid("model budget overflow"))?;
    if *total > limits.resources.max_total_bytes {
        return Err(crate::Error::ResourceLimit {
            resource: "embedded native model bytes",
            limit: limits.resources.max_total_bytes,
        });
    }
    Ok(())
}
fn write_preflight(fonts: &FontSet, limits: &sfnt::Limits) -> Result<()> {
    let mut total = 0;
    let mut props = |p: &crate::model::Properties| -> Result<()> {
        for p in p.iter() {
            charge(
                &mut total,
                p.name.len().saturating_mul(2).saturating_add(256),
                limits,
            )?;
        }
        Ok(())
    };
    props(&fonts.properties)?;
    for f in &fonts.fonts {
        props(&f.properties)?;
        props(&f.headers)?;
        props(&f.names.localized)?;
        for (_, g) in f.glyphs() {
            props(&g.properties)?;
        }
    }
    for f in &fonts.fonts {
        charge(&mut total, name_bytes(&f.names).saturating_mul(4), limits)?;
        for (_, g) in f.glyphs() {
            charge(
                &mut total,
                2048usize
                    .saturating_add(g.name.as_ref().map_or(0, String::len).saturating_mul(4))
                    .saturating_add(g.codepoints.len().saturating_mul(128))
                    .saturating_add(g.bitmap.as_bytes().len().saturating_mul(4)),
                limits,
            )?;
        }
    }
    Ok(())
}
/// Decode a standalone sfnt into physical bitmap strikes. All native losses are
/// returned, including optional threshold/color projection and omitted outlines.
pub fn read(
    data: &[u8],
    kind: sbit::Kind,
    options: &EmbeddedReadOptions,
) -> Result<(FontSet, Report)> {
    let limits = &options.limits;
    let font = Sfnt::parse(data, &limits.sfnt)?;
    let maxp = required(&font, *b"maxp")?;
    let count = u16at(maxp, 4)?;
    let mapping = cmap::decode(required(&font, *b"cmap")?, count, &limits.sfnt)?;
    let mut reverse = BTreeMap::<u16, Vec<u32>>::new();
    for (cp, gid) in mapping {
        reverse.entry(gid).or_default().push(cp);
    }
    let common_names = names(font.table(*b"name"), &limits.sfnt)?;
    let post = font
        .table(*b"post")
        .map(|p| post::PostTable::decode(p, count, &limits.sfnt))
        .transpose()?;
    let post_names = post.as_ref().and_then(|p| p.names.as_deref());
    let mut model_bytes = name_bytes(&common_names);
    charge(
        &mut model_bytes,
        reverse.values().map(|v| v.len() * 32 + 64).sum(),
        &limits.sfnt,
    )?;
    let mut report = Diagnostics::new(limits.sfnt.resources.max_total_bytes);
    let mut out = FontSet {
        source_format: Some(family_id(kind).into()),
        ..Default::default()
    };
    if kind == sbit::Kind::Color && font.table(*b"CBDT").is_none() {
        #[cfg(feature = "color")]
        read_sbix(
            &font,
            count,
            &reverse,
            post_names,
            &common_names,
            options,
            &mut out,
            &mut report,
        )?;
        #[cfg(not(feature = "color"))]
        return Err(invalid("color feature is disabled"));
    } else {
        let (dt, lt) = kind.tags();
        #[cfg(feature = "color")]
        let strikes = sbit::decode_with_color(
            required(&font, lt)?,
            required(&font, dt)?,
            kind,
            count,
            limits,
            options.color_darkness,
        )?;
        #[cfg(not(feature = "color"))]
        let strikes = sbit::decode(
            required(&font, lt)?,
            required(&font, dt)?,
            kind,
            count,
            limits,
        )?;
        for (fi, strike) in strikes.into_iter().enumerate() {
            charge(
                &mut model_bytes,
                name_bytes(&common_names) + 1024,
                &limits.sfnt,
            )?;
            for (gid, image) in &strike.glyphs {
                charge(
                    &mut model_bytes,
                    1024 + image.bitmap.as_bytes().len()
                        + reverse.get(gid).map_or(0, |v| v.len() * 32)
                        + post_names
                            .and_then(|v| v.get(*gid as usize))
                            .map_or(0, String::len),
                    &limits.sfnt,
                )?;
            }
            let mut f = Font::default();
            f.names = common_names.clone();
            f.source_format = Some(family_id(kind).into());
            f.metrics.pixel_size = Some(strike.ppem_y as u32);
            f.metrics.ppem_x = Some(strike.ppem_x as u32);
            f.metrics.ppem_y = Some(strike.ppem_y as u32);
            f.metrics.ascent = Some(strike.horizontal.ascender() as i64);
            f.metrics.descent = Some(-(strike.horizontal.descender() as i64));
            f.properties.push(Property::new(
                "sfnt.sbit_horizontal",
                PropertyValue::Bytes(strike.horizontal.0.to_vec()),
            ));
            f.properties.push(Property::new(
                "sfnt.sbit_vertical",
                PropertyValue::Bytes(strike.vertical.0.to_vec()),
            ));
            f.properties
                .push(Property::new("sfnt.sbit_depth", strike.bit_depth as i64));
            f.properties
                .push(Property::new("sfnt.sbit_flags", strike.flags as i64));
            for (gid, image) in strike.glyphs {
                let mut g = Glyph::new(threshold(
                    image.bitmap,
                    options,
                    &mut report,
                    fi,
                    gid as usize,
                )?);
                let m = image.metrics;
                let h = m.horizontal;
                let v = m.vertical;
                let origin = h.or(v).ok_or_else(|| invalid("bitmap missing metrics"))?;
                g.metrics.bounds = Bounds::new(
                    m.width as u32,
                    m.height as u32,
                    origin.x as i64,
                    origin.y as i64 - m.height as i64,
                );
                if let Some(h) = h {
                    g.metrics.advance = Some(Vector::new(h.advance as i64, 0));
                }
                if let Some(v) = v {
                    g.metrics.vertical_advance = Some(Vector::new(0, -(v.advance as i64)));
                    // Bounds relative to vertical origin for vertical-only small metrics.
                    g.metrics.vertical_origin = Some(Vector::new(
                        origin.x as i64 - v.x as i64,
                        origin.y as i64 - v.y as i64,
                    ));
                }
                apply_names(&mut g, gid, &reverse, post_names);
                f.add_glyph(g)?;
            }
            finish_font(&mut f)?;
            out.fonts.push(f);
        }
        if let Some(ebsc) = font.table(*b"EBSC") {
            let scales = sbit::decode_scaling(ebsc, limits)?;
            for scale in &scales {
                if !out.fonts.iter().any(|f| {
                    f.metrics.ppem_x == Some(scale.source[0] as u32)
                        && f.metrics.ppem_y == Some(scale.source[1] as u32)
                }) {
                    return Err(invalid("EBSC references absent physical strike"));
                }
            }
            out.properties.push(Property::new(
                "sfnt.EBSC",
                PropertyValue::Bytes(ebsc.to_vec()),
            ));
        }
    }
    if out.fonts.is_empty() {
        return Err(invalid("no physical bitmap strikes"));
    }
    if kind == sbit::Kind::Color {
        loss(
            &mut report,
            "color_projection",
            None,
            None,
            if options.color_darkness {
                "Color projected to alpha-weighted darkness; RGB and image metadata are not preserved"
            } else {
                "Color projected to alpha coverage; RGB and image metadata are not preserved"
            },
        );
    }
    if let Some(head) = font.table(*b"head").or_else(|| font.table(*b"bhed")) {
        let upem = u16at(head, 18)?;
        if !(16..=16384).contains(&upem) {
            return Err(invalid("invalid bitmap font unitsPerEm"));
        }
        for (fi, f) in out.fonts.iter_mut().enumerate() {
            let ppem = f.metrics.ppem_y.unwrap_or(0) as i64;
            let mut scale = |n: i16, field: &str| -> i64 {
                let value = n as i64 * ppem;
                if value % upem as i64 != 0 {
                    loss(
                        &mut report,
                        "metric_rounding",
                        Some(fi),
                        None,
                        format!("{field} rounded from design units to strike pixels"),
                    );
                }
                (value
                    + if value >= 0 {
                        upem as i64 / 2
                    } else {
                        -(upem as i64 / 2)
                    })
                    / upem as i64
            };
            if let Some(hhea) = font.table(*b"hhea") {
                f.metrics.line_gap = Some(scale(u16at(hhea, 8)? as i16, "line gap"));
            }
            if let Some(post) = &post {
                f.metrics.underline_position =
                    Some(scale(post.header.underline_position, "underline position"));
                f.metrics.underline_thickness = Some(scale(
                    post.header.underline_thickness,
                    "underline thickness",
                ));
                if post.header.is_fixed_pitch != 0 {
                    f.metrics.spacing = Some(Spacing::Monospaced);
                }
            }
        }
    }
    // Keep original common/unknown tables as bounded provenance. They cannot
    // override edited model fields on write; each unhandled table is reported.
    let mut stored = 0usize;
    let (dt, lt) = kind.tags();
    for table in font.tables() {
        if [dt, lt, *b"sbix", *b"EBSC"].contains(&table.tag) {
            continue;
        }
        stored = stored
            .checked_add(table.data.len())
            .ok_or_else(|| invalid("provenance size overflow"))?;
        if stored > limits.sfnt.resources.max_total_bytes / 2 {
            return Err(invalid("sfnt provenance exceeds model budget"));
        }
        out.properties.push(Property::new(
            format!("sfnt.table.{}", String::from_utf8_lossy(&table.tag)),
            PropertyValue::Bytes(table.data.to_vec()),
        ));
        if ![
            *b"head", *b"bhed", *b"maxp", *b"cmap", *b"name", *b"post", *b"hhea", *b"hmtx",
            *b"OS/2",
        ]
        .contains(&table.tag)
        {
            loss(
                &mut report,
                "unprojected_table",
                None,
                None,
                format!(
                    "{} retained as provenance, not interpreted as bitmap semantics",
                    String::from_utf8_lossy(&table.tag)
                ),
            );
        }
    }
    for (fi, f) in out.fonts.iter().enumerate() {
        let present: BTreeSet<_> = f
            .glyphs()
            .filter_map(|(_, g)| match g.properties.get("sfnt.glyph_id") {
                Some(PropertyValue::Integer(gid)) => Some(*gid as u16),
                _ => None,
            })
            .collect();
        for (&gid, cps) in &reverse {
            if !present.contains(&gid) {
                loss(
                    &mut report,
                    "missing_image",
                    Some(fi),
                    Some(gid as usize),
                    format!(
                        "No bitmap for {} nominal mappings of glyph {gid}; outline fallback is not rasterized",
                        cps.len()
                    ),
                );
            }
        }
    }
    // Charge native indexing/provenance as well as raster allocations.
    let native = out.fonts.iter().try_fold(stored, |n, f| {
        n.checked_add(f.len() * 768)
            .and_then(|n| {
                n.checked_add(
                    f.glyphs()
                        .map(|(_, g)| {
                            g.bitmap.as_bytes().len()
                                + g.codepoints.len() * 32
                                + g.name.as_ref().map_or(0, String::len)
                        })
                        .sum::<usize>(),
                )
            })
            .ok_or_else(|| invalid("native bitmap model size overflow"))
    })?;
    if native > limits.sfnt.resources.max_total_bytes {
        return Err(invalid("native bitmap model budget exceeded"));
    }
    Ok((out, report.finish()?))
}
pub(crate) fn finish_font(font: &mut Font) -> Result<()> {
    font.update_bounds()?;
    let spacing = {
        let mut advances = font
            .glyphs()
            .filter(|(_, g)| !g.codepoints.is_empty())
            .filter_map(|(_, g)| g.metrics.advance.map(|v| v.x));
        advances.next().map(|first| {
            if advances.all(|a| a == first) {
                Spacing::Monospaced
            } else {
                Spacing::Proportional
            }
        })
    };
    font.metrics.spacing = spacing;
    Ok(())
}
#[cfg(feature = "color")]
#[allow(clippy::too_many_arguments)]
fn read_sbix(
    font: &Sfnt<'_>,
    count: u16,
    mapping: &BTreeMap<u16, Vec<u32>>,
    post_names: Option<&[String]>,
    names: &FontNames,
    options: &EmbeddedReadOptions,
    out: &mut FontSet,
    report: &mut Diagnostics,
) -> Result<()> {
    let sbix = sbit::color::decode_sbix(
        required(font, *b"sbix")?,
        count,
        &options.limits,
        options.color_darkness,
    )?;
    if sbix.draw_outlines {
        loss(
            report,
            "sbix_overlay",
            None,
            None,
            "sbix outline overlay is not composited",
        );
    }
    let head = required(font, *b"head")?;
    let upem = u16at(head, 18)?;
    if !(16..=16384).contains(&upem) {
        return Err(invalid("invalid sbix unitsPerEm"));
    }
    let (header, metrics) = metrics::decode(
        metrics::Axis::Horizontal,
        required(font, *b"hhea")?,
        required(font, *b"hmtx")?,
        count,
        &options.limits.sfnt,
    )?;
    // Only headers/loca are needed for placement, not outline execution.
    let mut origins = vec![(0i16, 0i16); count as usize];
    if let Some(glyf) = font.table(*b"glyf") {
        let loca = required(font, *b"loca")?;
        let long = match u16at(head, 50)? {
            0 => false,
            1 => true,
            _ => return Err(invalid("invalid sbix loca format")),
        };
        for (gid, origin) in origins.iter_mut().enumerate() {
            let at = |i| {
                if long {
                    u32at(loca, i * 4).map(|v| v as usize)
                } else {
                    u16at(loca, i * 2).map(|v| v as usize * 2)
                }
            };
            let a = at(gid)?;
            let b = at(gid + 1)?;
            if b < a {
                return Err(invalid("nonmonotone sbix loca"));
            }
            let glyph = glyf
                .get(a..b)
                .ok_or_else(|| invalid("sbix loca beyond glyf"))?;
            if !glyph.is_empty() && u16at(glyph, 0)? != 0 {
                *origin = (u16at(glyph, 2)? as i16, u16at(glyph, 4)? as i16);
            }
        }
    } else if font.table(*b"CFF ").is_some() || font.table(*b"CFF2").is_some() {
        return Err(invalid("sbix CFF-origin placement is not supported"));
    }
    let mut model_bytes = 0;
    for (fi, strike) in sbix.strikes.into_iter().enumerate() {
        charge(
            &mut model_bytes,
            name_bytes(names) + 1024,
            &options.limits.sfnt,
        )?;
        for (gid, image) in &strike.glyphs {
            charge(
                &mut model_bytes,
                1024 + image.bitmap.as_bytes().len()
                    + mapping.get(gid).map_or(0, |v| v.len() * 32)
                    + post_names
                        .and_then(|v| v.get(*gid as usize))
                        .map_or(0, String::len),
                &options.limits.sfnt,
            )?;
        }
        let scale = |n: i64| {
            (n * strike.ppem as i64
                + if n >= 0 {
                    upem as i64 / 2
                } else {
                    -(upem as i64 / 2)
                })
                / upem as i64
        };
        let mut f = Font::default();
        f.names = names.clone();
        f.source_format = Some("color".into());
        f.metrics.pixel_size = Some(strike.ppem as u32);
        f.metrics.ppem_x = Some(strike.ppem as u32);
        f.metrics.ppem_y = Some(strike.ppem as u32);
        f.metrics.resolution = Some(Vector::new(strike.ppi as i64, strike.ppi as i64));
        f.metrics.ascent = Some(scale(header.ascender as i64));
        f.metrics.descent = Some(-scale(header.descender as i64));
        for (gid, image) in strike.glyphs {
            let origin = origins[gid as usize];
            let adv = metrics[gid as usize].advance;
            if [origin.0 as i64, origin.1 as i64, adv as i64]
                .iter()
                .any(|v| (v * strike.ppem as i64) % upem as i64 != 0)
            {
                loss(
                    report,
                    "sbix_rounding",
                    Some(fi),
                    Some(gid as usize),
                    "Scaled sbix design metrics rounded to nearest pixel",
                );
            }
            let mut g = Glyph::new(threshold(image.bitmap, options, report, fi, gid as usize)?);
            g.metrics.bounds = Bounds::new(
                g.bitmap.width(),
                g.bitmap.height(),
                image.origin[0] as i64 + scale(origin.0 as i64),
                image.origin[1] as i64 + scale(origin.1 as i64),
            );
            g.metrics.advance = Some(Vector::new(scale(adv as i64), 0));
            apply_names(&mut g, gid, mapping, post_names);
            f.add_glyph(g)?;
        }
        finish_font(&mut f)?;
        out.fonts.push(f);
    }
    Ok(())
}

#[derive(Clone, Debug)]
enum Key {
    Notdef,
    Code(u32),
    Codes(Vec<u32>),
    Name(String),
    Slot(usize),
}
impl Key {
    fn codepoints(&self) -> Option<&[u32]> {
        match self {
            Self::Code(cp) => Some(std::slice::from_ref(cp)),
            Self::Codes(cps) => Some(cps),
            _ => None,
        }
    }
}
impl Ord for Key {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::{Equal, Greater, Less};
        // Preserve the former derived order: Notdef < Codes < Name < Slot.
        // An inline code is a one-element sequence, not a separate category.
        match (self, other) {
            (Self::Notdef, Self::Notdef) => Equal,
            (Self::Notdef, _) => Less,
            (_, Self::Notdef) => Greater,
            (Self::Code(a), Self::Code(b)) => a.cmp(b),
            (Self::Code(a), Self::Codes(b)) => std::slice::from_ref(a).cmp(b.as_slice()),
            (Self::Codes(a), Self::Code(b)) => a.as_slice().cmp(std::slice::from_ref(b)),
            (Self::Codes(a), Self::Codes(b)) => a.cmp(b),
            (Self::Code(_) | Self::Codes(_), _) => Less,
            (_, Self::Code(_) | Self::Codes(_)) => Greater,
            (Self::Name(a), Self::Name(b)) => a.cmp(b),
            (Self::Name(_), Self::Slot(_)) => Less,
            (Self::Slot(_), Self::Name(_)) => Greater,
            (Self::Slot(a), Self::Slot(b)) => a.cmp(b),
        }
    }
}
impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Key {}
// Internal sfnt IDs are bounded by u16. Unlike a Key tree, this set neither
// clones Unicode alias vectors/names nor allocates one tree node per group.
// Even the largest possible ID needs at most 1024 words (8 KiB).
#[derive(Default)]
struct GlyphIdSet(Vec<u64>);
impl GlyphIdSet {
    fn insert(&mut self, gid: u16) -> bool {
        let word = usize::from(gid) / 64;
        if self.0.len() <= word {
            // Power-of-two lengths also bound Vec's geometric capacity growth
            // when a sparse strike first encounters a large existing ID.
            self.0.resize((word + 1).next_power_of_two(), 0);
        }
        let mask = 1u64 << (gid % 64);
        let fresh = self.0[word] & mask == 0;
        self.0[word] |= mask;
        fresh
    }
}
fn key(g: &Glyph, index: usize, fi: usize, report: &mut Diagnostics) -> Key {
    if let [Codepoint::Unicode(cp)] = g.codepoints.as_slice()
        && char::from_u32(*cp).is_some()
    {
        return Key::Code(*cp);
    }
    let mut cps = Vec::new();
    for cp in &g.codepoints {
        if let Codepoint::Unicode(cp) = cp
            && char::from_u32(*cp).is_some()
        {
            cps.push(*cp);
        } else {
            loss(
                report,
                "codepoint",
                Some(fi),
                Some(index),
                format!("Cannot put {cp:?} in a Unicode cmap"),
            );
        }
    }
    cps.sort_unstable();
    cps.dedup();
    if !cps.is_empty() {
        Key::Codes(cps)
    } else if g.name.as_deref() == Some(".notdef") {
        Key::Notdef
    } else if let Some(name) = &g.name {
        Key::Name(name.clone())
    } else {
        Key::Slot(index)
    }
}
fn byte(n: i64, what: &str) -> Result<u8> {
    u8::try_from(n).map_err(|_| invalid(format!("{what} does not fit uint8")))
}
fn signed(n: i64, what: &str) -> Result<i8> {
    i8::try_from(n).map_err(|_| invalid(format!("{what} does not fit int8")))
}
fn int16(n: i64, what: &str) -> Result<i16> {
    i16::try_from(n).map_err(|_| invalid(format!("{what} does not fit int16")))
}
fn sbit_metrics(g: &Glyph, f: &Font) -> Result<sbit::Metrics> {
    let b = g.metrics.bounds;
    let top =
        b.y.checked_add(b.height as i64)
            .ok_or_else(|| invalid("glyph top overflow"))?;
    let horizontal = if let Some(advance) = g.metrics.advance.or(f.metrics.glyph_defaults.advance) {
        if advance.y != 0 {
            return Err(invalid("nonhorizontal DWIDTH cannot be represented"));
        }
        Some(sbit::Bearing {
            x: signed(b.x, "horizontal bearing x")?,
            y: signed(top, "horizontal bearing y")?,
            advance: byte(advance.x, "horizontal advance")?,
        })
    } else {
        None
    };
    let vertical = if let Some(advance) = g
        .metrics
        .vertical_advance
        .or(f.metrics.glyph_defaults.vertical_advance)
    {
        if advance.x != 0 || advance.y > 0 {
            return Err(invalid("vertical advance must be downward"));
        }
        let origin = g
            .metrics
            .vertical_origin
            .or(f.metrics.glyph_defaults.vertical_origin)
            .unwrap_or(Vector::new(0, 0));
        Some(sbit::Bearing {
            x: signed(
                b.x.checked_sub(origin.x)
                    .ok_or_else(|| invalid("vertical bearing overflow"))?,
                "vertical bearing x",
            )?,
            y: signed(
                top.checked_sub(origin.y)
                    .ok_or_else(|| invalid("vertical bearing overflow"))?,
                "vertical bearing y",
            )?,
            advance: byte(
                advance
                    .y
                    .checked_neg()
                    .ok_or_else(|| invalid("vertical advance overflow"))?,
                "vertical advance",
            )?,
        })
    } else {
        None
    };
    Ok(sbit::Metrics {
        width: byte(b.width as i64, "width")?,
        height: byte(b.height as i64, "height")?,
        horizontal,
        vertical,
    })
}
fn line_record(f: &Font, strike: &sbit::Strike, vertical: bool) -> Result<sbit::LineMetrics> {
    let key = if vertical {
        "sfnt.sbit_vertical"
    } else {
        "sfnt.sbit_horizontal"
    };
    let mut line = [0; 12];
    if let Some(PropertyValue::Bytes(source)) = f.properties.get(key) {
        if source.len() != 12 {
            return Err(invalid("invalid retained line record"));
        }
        line.copy_from_slice(source);
    }
    if !vertical {
        let ascent = f.metrics.ascent.unwrap_or_else(|| {
            strike
                .glyphs
                .values()
                .filter_map(|g| g.metrics.horizontal.map(|h| h.y as i64))
                .max()
                .unwrap_or(0)
        });
        let descent = f.metrics.descent.unwrap_or_else(|| {
            strike
                .glyphs
                .values()
                .filter_map(|g| {
                    g.metrics
                        .horizontal
                        .map(|h| g.metrics.height as i64 - h.y as i64)
                })
                .max()
                .unwrap_or(0)
        });
        line[0] = signed(ascent, "ascent")? as u8;
        line[1] = signed(
            descent
                .checked_neg()
                .ok_or_else(|| invalid("descent overflow"))?,
            "descent",
        )? as u8;
        line[2] = strike
            .glyphs
            .values()
            .map(|g| g.metrics.width)
            .max()
            .unwrap_or(0);
        let hs: Vec<_> = strike
            .glyphs
            .values()
            .filter_map(|g| {
                g.metrics
                    .horizontal
                    .map(|h| (h, g.metrics.width, g.metrics.height))
            })
            .collect();
        for (at, n) in [
            (6, hs.iter().map(|(h, _, _)| h.x as i64).min().unwrap_or(0)),
            (
                7,
                hs.iter()
                    .map(|(h, w, _)| h.advance as i64 - h.x as i64 - *w as i64)
                    .min()
                    .unwrap_or(0),
            ),
            (8, hs.iter().map(|(h, _, _)| h.y as i64).max().unwrap_or(0)),
            (
                9,
                hs.iter()
                    .map(|(h, _, height)| h.y as i64 - *height as i64)
                    .min()
                    .unwrap_or(0),
            ),
        ] {
            line[at] = signed(n, "sbit line extent")? as u8;
        }
        line[3] = 1;
    }
    Ok(sbit::LineMetrics(line))
}
fn ignored_fields(f: &Font, fi: usize, report: &mut Diagnostics, sbix: bool) {
    for (source, props) in [("header", &f.headers), ("property", &f.properties)] {
        for p in props.iter() {
            if !p.name.starts_with("sfnt.sbit_") {
                loss(
                    report,
                    "source_property",
                    Some(fi),
                    None,
                    format!("Source {source} {:?} is not represented", p.name),
                );
            }
        }
    }
    let m = &f.metrics;
    for (name, present) in [
        ("point_size", m.point_size.is_some()),
        ("resolution", m.resolution.is_some() && !sbix),
        ("bounds", m.bounds.is_some()),
        ("line_height", m.line_height.is_some()),
        ("default_character", m.default_character.is_some()),
        ("space_width", m.space_width.is_some()),
        ("metrics_set", m.metrics_set.is_some()),
        ("default_glyph", m.default_glyph.is_some()),
    ] {
        if present {
            loss(
                report,
                "font_metric",
                Some(fi),
                None,
                format!("Independent {name} is not preserved"),
            );
        }
    }
    if f.metrics.spacing == Some(Spacing::CharacterCell) {
        loss(
            report,
            "spacing",
            Some(fi),
            None,
            "Character-cell classification is represented as fixed-pitch only",
        );
    }
    let defaults = &f.metrics.glyph_defaults;
    if defaults.scalable_advance.is_some()
        || defaults.vertical_scalable_advance.is_some()
        || defaults.abc.is_some()
        || defaults.left_bearing.is_some()
        || defaults.right_bearing.is_some()
        || defaults.image_offset.is_some()
    {
        loss(
            report,
            "glyph_defaults",
            Some(fi),
            None,
            "Independent font-level scalable/spacing defaults are not preserved",
        );
    }
    if f.charset != Default::default() {
        loss(
            report,
            "charset",
            Some(fi),
            None,
            "Source charset metadata is not represented by Unicode cmap",
        );
    }
    if f.names.xlfd.is_some() || !f.names.xlfd_fields.is_empty() || !f.names.localized.is_empty() {
        loss(
            report,
            "source_names",
            Some(fi),
            None,
            "XLFD/localized source name properties are not preserved",
        );
    }
    for (id, g) in f.glyphs() {
        for p in g.properties.iter() {
            if p.name != "sfnt.glyph_id" {
                loss(
                    report,
                    "glyph_property",
                    Some(fi),
                    Some(id.index()),
                    format!("Glyph property {:?} is not represented", p.name),
                );
            }
        }
        if !g.unicode_sequences.is_empty() {
            loss(
                report,
                "unicode_sequence",
                Some(fi),
                Some(id.index()),
                "Unicode sequences need shaping tables and are not represented",
            );
        }
        let m = &g.metrics;
        for (name, present) in [
            ("scalable_advance", m.scalable_advance.is_some()),
            (
                "vertical_scalable_advance",
                m.vertical_scalable_advance.is_some(),
            ),
            ("abc", m.abc.is_some()),
            ("left_bearing", m.left_bearing.is_some()),
            ("right_bearing", m.right_bearing.is_some()),
            ("image_offset", m.image_offset.is_some()),
        ] {
            if present {
                loss(
                    report,
                    "glyph_metric",
                    Some(fi),
                    Some(id.index()),
                    format!("Independent {name} is not preserved"),
                );
            }
        }
    }
}
/// Write one or multiple strikes as a bitmap-only sfnt. Shared Unicode aliases
/// must agree across sizes; impossible layouts and out-of-range metrics error.
pub fn write(
    fonts: &FontSet,
    kind: sbit::Kind,
    options: &EmbeddedWriteOptions,
) -> Result<(Vec<u8>, Report)> {
    let result = write_impl(fonts, kind, options, false)?;
    #[cfg(feature = "color")]
    if kind == sbit::Kind::Color {
        color_readback(&result.0, &options.limits)?;
    }
    Ok(result)
}

/// Write coverage as black PNG alpha, preserving aliases and physical strikes.
/// The returned loss report includes representation and shared-metric changes.
#[cfg(feature = "color")]
pub fn write_color(fonts: &FontSet, options: &ColorWriteOptions) -> Result<(Vec<u8>, Report)> {
    let result = write_impl(
        fonts,
        sbit::Kind::Color,
        &EmbeddedWriteOptions {
            limits: options.limits.clone(),
            ..Default::default()
        },
        options.flavor == ColorFlavor::Sbix,
    )?;
    color_readback(&result.0, &options.limits)?;
    Ok(result)
}
#[cfg(feature = "color")]
fn color_readback(bytes: &[u8], limits: &sbit::Limits) -> Result<()> {
    read(
        bytes,
        sbit::Kind::Color,
        &EmbeddedReadOptions {
            limits: limits.clone(),
            ..Default::default()
        },
    )?;
    Ok(())
}
fn write_impl(
    fonts: &FontSet,
    kind: sbit::Kind,
    options: &EmbeddedWriteOptions,
    sbix: bool,
) -> Result<(Vec<u8>, Report)> {
    if kind == sbit::Kind::Color && !cfg!(feature = "color") {
        return Err(invalid("color writing requires the color feature"));
    }
    let design_options = DesignOptions::from(options);
    design_options.validate(fonts.fonts.len(), kind)?;
    options.metadata.validate(kind)?;
    let prepared = options.metadata.prepare(fonts, &options.limits)?;
    let limits = prepared.limits.as_ref().unwrap_or(&options.limits);
    write_preflight(fonts, &limits.sfnt)?;
    #[cfg(feature = "color")]
    if kind == sbit::Kind::Color {
        color_memory_preflight(fonts, limits)?;
    }
    let first = fonts
        .fonts
        .first()
        .ok_or_else(|| invalid("empty FontSet"))?;
    if fonts.fonts.len() > limits.max_strikes {
        return Err(invalid("too many strikes"));
    }
    let mut report = Diagnostics::new(limits.sfnt.resources.max_total_bytes);
    options.metadata.audit(fonts, &mut report);
    let mut ids = BTreeMap::from([(Key::Notdef, 0u16)]);
    let mut mapping = cmap::Mapping::new();
    let mut assigned = Vec::new();
    let mut names = vec![".notdef".to_owned()];
    let mut total = 0usize;
    for (fi, f) in fonts.fonts.iter().enumerate() {
        f.validate(&limits.sfnt.resources)?;
        ignored_fields(f, fi, &mut report, sbix);
        #[cfg(feature = "color")]
        if kind == sbit::Kind::Color {
            color_global_losses(first, f, fi, &mut report)?;
        }
        if fi > 0 && f.names != first.names {
            loss(
                &mut report,
                "strike_names",
                Some(fi),
                None,
                "sfnt uses the first strike's global names",
            );
        }
        total = total
            .checked_add(f.len())
            .ok_or_else(|| invalid("glyph total overflow"))?;
        if total > limits.sfnt.resources.max_glyphs
            || total.saturating_mul(2048) > limits.sfnt.resources.max_total_bytes
        {
            return Err(invalid("bitmap model/glyph budget exceeded"));
        }
        let mut selected = Vec::new();
        let mut seen = GlyphIdSet::default();
        for (id, g) in f.glyphs() {
            let key = key(g, id.index(), fi, &mut report);
            let next_id = ids.len();
            let entry = match ids.entry(key) {
                Entry::Occupied(entry) => {
                    let gid = *entry.get();
                    if !seen.insert(gid) {
                        return Err(invalid("ambiguous duplicate glyph labels within strike"));
                    }
                    if kind == sbit::Kind::Color
                        && g.name.as_ref().is_some_and(|n| n != &names[gid as usize])
                    {
                        loss(
                            &mut report,
                            "strike_glyph_name",
                            Some(fi),
                            Some(id.index()),
                            "Shared post name comes from the first strike containing this glyph",
                        );
                    }
                    entry
                }
                Entry::Vacant(entry) => {
                    let gid =
                        u16::try_from(next_id).map_err(|_| invalid("too many sfnt glyphs"))?;
                    if gid == u16::MAX {
                        return Err(invalid("sfnt requires at most 65535 glyphs"));
                    }
                    let entry = entry.insert_entry(gid);
                    // A newly allocated ID cannot have appeared in this strike.
                    seen.insert(gid);
                    let name = g.name.as_deref().filter(|n| {
                        !n.is_empty()
                            && n.len() <= 63
                            && n.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
                    });
                    if g.name.is_some() && name.is_none() {
                        loss(
                            &mut report,
                            "glyph_name",
                            Some(fi),
                            Some(id.index()),
                            "Glyph name cannot be encoded in post format 2",
                        );
                    }
                    names.push(name.map_or_else(|| format!("glyph{gid}"), str::to_owned));
                    entry
                }
            };
            let gid = *entry.get();
            if let Some(cps) = entry.key().codepoints() {
                for &cp in cps {
                    if let Some(old) = mapping.insert(cp, gid)
                        && old != gid
                    {
                        return Err(invalid("conflicting Unicode aliases across glyphs/strikes"));
                    }
                }
            }
            selected.push((gid, g));
        }
        assigned.push(selected);
    }
    if mapping.len() > limits.sfnt.max_cmap_mappings {
        return Err(invalid("cmap mapping budget exceeded"));
    }
    let count = u16::try_from(ids.len()).map_err(|_| invalid("too many sfnt glyphs"))?;
    if sbix {
        #[cfg(feature = "color")]
        return write_native_sbix(fonts, &assigned, count, &mapping, names, limits, report);
        #[cfg(not(feature = "color"))]
        return Err(invalid("sbix requires the color feature"));
    }
    let mut strikes = Vec::new();
    for (fi, (f, selected)) in fonts.fonts.iter().zip(&assigned).enumerate() {
        let ppem = f
            .metrics
            .pixel_size
            .or(f.metrics.ppem_y)
            .ok_or_else(|| invalid("missing pixel size"))?;
        let x = byte(f.metrics.ppem_x.unwrap_or(ppem) as i64, "ppem x")?;
        let y = byte(f.metrics.ppem_y.unwrap_or(ppem) as i64, "ppem y")?;
        let depth = if kind == sbit::Kind::Color {
            32
        } else if selected
            .iter()
            .any(|(_, g)| matches!(g.bitmap, GlyphBitmap::Gray8(_)))
        {
            8
        } else {
            1
        };
        let mut glyphs = BTreeMap::new();
        let mut horiz = false;
        let mut vert = false;
        for &(gid, g) in selected {
            let metrics = sbit_metrics(g, f)?;
            horiz |= metrics.horizontal.is_some();
            vert |= metrics.vertical.is_some();
            if [8, 32].contains(&depth) && matches!(g.bitmap, GlyphBitmap::Mono(_)) {
                loss(
                    &mut report,
                    "depth_normalized",
                    Some(fi),
                    Some(gid as usize),
                    "Mono represented as Gray8 coverage without changing samples",
                );
            }
            glyphs.insert(
                gid,
                sbit::BitmapGlyph {
                    metrics,
                    bitmap: g.bitmap.clone(),
                },
            );
        }
        let small_h = glyphs
            .values()
            .any(|g| g.metrics.horizontal.is_some() && g.metrics.vertical.is_none());
        let small_v = glyphs
            .values()
            .any(|g| g.metrics.vertical.is_some() && g.metrics.horizontal.is_none());
        if small_h && small_v {
            return Err(invalid(
                "one strike cannot mix horizontal-only and vertical-only small metrics",
            ));
        }
        let flags = if small_h {
            1
        } else if small_v {
            2
        } else if horiz && vert {
            3
        } else {
            1
        };
        glyphs.entry(0).or_insert_with(|| sbit::BitmapGlyph {
            metrics: sbit::Metrics {
                horizontal: Some(sbit::Bearing::default()),
                vertical: (flags != 1).then(sbit::Bearing::default),
                ..Default::default()
            },
            bitmap: GlyphBitmap::Mono(Bitmap::default()),
        });
        // A strike with mixed small directions cannot carry one direction flag.
        // Promote only metrics actually available: reject rather than fabricate.
        let mut strike = sbit::Strike {
            ppem_x: x,
            ppem_y: y,
            bit_depth: depth,
            flags,
            horizontal: sbit::LineMetrics::default(),
            vertical: sbit::LineMetrics::default(),
            glyphs,
        };
        strike.horizontal = line_record(f, &strike, false)?;
        strike.vertical = line_record(f, &strike, true)?;
        if options.metadata.width_max == EmbeddedWidthMax::Advance {
            strike.horizontal.0[2] = strike
                .glyphs
                .values()
                .filter_map(|g| g.metrics.horizontal.map(|h| h.advance))
                .max()
                .unwrap_or(0);
        }
        strikes.push(strike);
    }
    let mut sizes = BTreeSet::new();
    for s in &strikes {
        if !sizes.insert((s.ppem_x, s.ppem_y)) {
            return Err(invalid("duplicate physical strike size"));
        }
    }
    let tables = match options.metadata.index_layout {
        EmbeddedIndexLayout::Canonical => sbit::encode(&strikes, kind, count, limits)?,
        EmbeddedIndexLayout::SharedMetricRuns { minimum_run } => {
            sbit::encode_shared_metric_runs(&strikes, kind, count, minimum_run, limits)?.tables
        }
    };
    let mut builder = SfntBuilder::new(Flavor::TrueType);
    let (dt, lt) = kind.tags();
    builder.insert(dt, tables.data)?;
    builder.insert(lt, tables.location)?;
    let source_scaling = fonts.properties.get("sfnt.EBSC");
    let scales = if options.scaling.is_empty() {
        if let Some(PropertyValue::Bytes(bytes)) = source_scaling {
            sbit::decode_scaling(bytes, limits)?
        } else {
            Vec::new()
        }
    } else {
        options.scaling.clone()
    };
    for s in &scales {
        if !sizes.contains(&(s.source[0], s.source[1])) {
            return Err(invalid("EBSC references missing physical size"));
        }
    }
    if !scales.is_empty() {
        builder.insert(*b"EBSC", sbit::encode_scaling(&scales, limits)?)?;
    }
    for p in fonts.properties.iter() {
        if p.name != "sfnt.EBSC" {
            loss(
                &mut report,
                "container_property",
                None,
                None,
                format!(
                    "Container property {:?} is not copied into regenerated sfnt tables",
                    p.name
                ),
            );
        }
    }
    common_tables(
        &mut builder,
        &fonts.fonts,
        &strikes,
        count,
        &mapping,
        names,
        &limits.sfnt,
        &mut report,
        CommonOptions {
            design: design_options,
            metadata: &options.metadata,
            names: prepared.names,
        },
    )?;
    let report = report.finish()?;
    Ok((builder.build(&limits.sfnt)?, report))
}
#[cfg(feature = "color")]
fn color_global_losses(first: &Font, f: &Font, fi: usize, report: &mut Diagnostics) -> Result<()> {
    let ppem = |f: &Font| {
        f.metrics
            .ppem_y
            .or(f.metrics.pixel_size)
            .filter(|p| *p != 0)
            .ok_or_else(|| invalid("missing or zero ppem"))
    };
    let reference = i128::from(ppem(first)?);
    let current = i128::from(ppem(f)?);
    let scale = if reference <= 163 {
        100
    } else if reference <= 255 {
        64
    } else {
        (16384 / reference).max(1)
    };
    let upem = (reference * scale).min(16384);
    let round = |n: i128, d: i128| (n + if n >= 0 { d / 2 } else { -(d / 2) }) / d;
    for (field, base, actual) in [
        ("line gap", first.metrics.line_gap, f.metrics.line_gap),
        (
            "underline position",
            first.metrics.underline_position,
            f.metrics.underline_position,
        ),
        (
            "underline thickness",
            first.metrics.underline_thickness,
            f.metrics.underline_thickness,
        ),
    ] {
        if let Some(actual) = actual {
            let encoded = round(i128::from(base.unwrap_or(0)) * upem, reference);
            let decoded = round(encoded * current, upem);
            if decoded != i128::from(actual) {
                loss(
                    report,
                    "shared_line_metric",
                    Some(fi),
                    None,
                    format!("Per-strike {field} changed to the rounded shared sfnt metric"),
                );
            }
        }
    }
    Ok(())
}
#[cfg(feature = "color")]
fn color_memory_preflight(fonts: &FontSet, limits: &sbit::Limits) -> Result<()> {
    // Conservative cumulative envelope, including source/assigned rasters,
    // stored PNG scratch and payloads, table assembly copies, native readback,
    // decoder row/inflate buffers, per-glyph indexes and bounded diagnostics.
    // Every multiplication is saturating so impossible sizes fail closed.
    // Reserve the complete diagnostic allowance alongside the allocation envelope.
    let mut total = 32768 + (limits.sfnt.resources.max_total_bytes / 4).min(64 * 1024 * 1024);
    let mut pixels = 0usize;
    for f in &fonts.fonts {
        charge(
            &mut total,
            8192 + name_bytes(&f.names).saturating_mul(8),
            &limits.sfnt,
        )?;
        for (_, g) in f.glyphs() {
            let area = (g.bitmap.width() as usize).saturating_mul(g.bitmap.height() as usize);
            pixels = pixels.saturating_add(area.saturating_mul(3));
            charge(
                &mut total,
                4096usize
                    .saturating_add(area.saturating_mul(48))
                    .saturating_add(g.bitmap.height() as usize * 8)
                    .saturating_add(g.codepoints.len().saturating_mul(256))
                    .saturating_add(g.name.as_ref().map_or(0, String::len).saturating_mul(8)),
                &limits.sfnt,
            )?;
        }
    }
    crate::sfnt::bounded(
        pixels,
        limits.max_pixel_work,
        "color write/readback pixel work",
    )?;
    Ok(())
}
#[cfg(feature = "color")]
#[allow(clippy::too_many_arguments)]
fn write_native_sbix(
    fonts: &FontSet,
    assigned: &[Vec<(u16, &Glyph)>],
    count: u16,
    mapping: &cmap::Mapping,
    names: Vec<String>,
    limits: &sbit::Limits,
    mut report: Diagnostics,
) -> Result<(Vec<u8>, Report)> {
    use sbit::color::{Sbix, SbixGlyph, SbixStrike};
    let mut strikes = Vec::new();
    let mut design = Vec::new();
    for (fi, (f, selected)) in fonts.fonts.iter().zip(assigned).enumerate() {
        let ppem = f
            .metrics
            .ppem_y
            .or(f.metrics.pixel_size)
            .ok_or_else(|| invalid("missing sbix ppem"))?;
        let ppem = u16::try_from(ppem)
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| invalid("sbix ppem must fit nonzero uint16"))?;
        if f.metrics.ppem_x.is_some_and(|x| x != u32::from(ppem)) {
            loss(
                &mut report,
                "sbix_ppem",
                Some(fi),
                None,
                "sbix uses the vertical ppem for both axes",
            );
        }
        let ppi = match f.metrics.resolution {
            Some(v) if v.x == v.y && v.x > 0 => {
                u16::try_from(v.x).map_err(|_| invalid("sbix PPI does not fit uint16"))?
            }
            Some(_) => {
                loss(
                    &mut report,
                    "sbix_ppi",
                    Some(fi),
                    None,
                    "Non-square or non-positive resolution replaced by 72 PPI",
                );
                72
            }
            None => 72,
        };
        let mut glyphs = BTreeMap::new();
        let mut design_glyphs = Vec::new();
        for &(gid, g) in selected {
            let b = g.metrics.bounds;
            let advance = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .ok_or_else(|| invalid("sbix requires a horizontal advance"))?;
            if advance.y != 0 || advance.x < 0 {
                return Err(invalid("sbix requires a nonnegative horizontal advance"));
            }
            // Even at maximum ppem and minimum legal upem, a uint16 hmtx
            // advance cannot exceed int32 pixels. Bound before design scaling.
            i32::try_from(advance.x).map_err(|_| invalid("sbix advance overflow"))?;
            let origin = [int16(b.x, "sbix origin x")?, int16(b.y, "sbix origin y")?];
            let top =
                b.y.checked_add(i64::from(b.height))
                    .ok_or_else(|| invalid("sbix top overflow"))?;
            design_glyphs.push(DesignGlyph {
                id: gid,
                width: b.width,
                height: b.height,
                horizontal: Some([b.x, top, advance.x]),
                vertical: None,
            });
            if g.metrics
                .vertical_advance
                .or(f.metrics.glyph_defaults.vertical_advance)
                .is_some()
                || g.metrics
                    .vertical_origin
                    .or(f.metrics.glyph_defaults.vertical_origin)
                    .is_some()
            {
                loss(
                    &mut report,
                    "sbix_vertical",
                    Some(fi),
                    Some(gid as usize),
                    "Native sbix projection does not retain vertical glyph metrics",
                );
            }
            if b.width == 0 || b.height == 0 {
                loss(
                    &mut report,
                    "sbix_empty",
                    Some(fi),
                    Some(gid as usize),
                    "Zero-area sbix glyph has an absent image; its explicit rectangle and strike membership are not retained",
                );
                continue;
            }
            if matches!(g.bitmap, GlyphBitmap::Mono(_)) {
                loss(
                    &mut report,
                    "depth_normalized",
                    Some(fi),
                    Some(gid as usize),
                    "Mono represented as Gray8 coverage without changing samples",
                );
            }
            glyphs.insert(
                gid,
                SbixGlyph {
                    origin,
                    bitmap: g.bitmap.clone(),
                },
            );
        }
        let ascent = f.metrics.ascent.unwrap_or_else(|| {
            design_glyphs
                .iter()
                .filter_map(|g| g.horizontal.map(|h| h[1]))
                .max()
                .unwrap_or(0)
        });
        let descent = f.metrics.descent.unwrap_or_else(|| {
            design_glyphs
                .iter()
                .filter_map(|g| g.horizontal.map(|h| i64::from(g.height) - h[1]))
                .max()
                .unwrap_or(0)
        });
        design.push(DesignStrike {
            ppem: [u32::from(ppem); 2],
            ascent,
            descent,
            glyphs: design_glyphs,
        });
        strikes.push(SbixStrike { ppem, ppi, glyphs });
    }
    for p in fonts.properties.iter() {
        loss(
            &mut report,
            "container_property",
            None,
            None,
            format!(
                "Container property {:?} is not copied into regenerated sbix tables",
                p.name
            ),
        );
    }
    let sbix = Sbix {
        draw_outlines: false,
        strikes,
    };
    let mut builder = SfntBuilder::new(Flavor::TrueType);
    builder.insert(*b"sbix", sbit::color::encode_sbix(&sbix, count, limits)?)?;
    common_design_tables(
        &mut builder,
        &fonts.fonts,
        &design,
        count,
        mapping,
        names,
        &limits.sfnt,
        &mut report,
        CommonOptions {
            design: DesignOptions::default(),
            metadata: &EmbeddedMetadataOptions::default(),
            names: None,
        },
    )?;
    let bytes = builder.build(&limits.sfnt)?;
    let font = Sfnt::parse(&bytes, &limits.sfnt)?;
    let upem = i64::from(u16at(required(&font, *b"head")?, 18)?);
    let (header, metrics) = metrics::decode(
        metrics::Axis::Horizontal,
        required(&font, *b"hhea")?,
        required(&font, *b"hmtx")?,
        count,
        &limits.sfnt,
    )?;
    for (fi, s) in design.iter().enumerate() {
        let scale = |n: i64| {
            let value = n * i64::from(s.ppem[1]);
            (value + if value >= 0 { upem / 2 } else { -(upem / 2) }) / upem
        };
        for g in &s.glyphs {
            if let Some(h) = g.horizontal
                && h[2] != scale(i64::from(metrics[g.id as usize].advance))
            {
                loss(
                    &mut report,
                    "sbix_advance",
                    Some(fi),
                    Some(g.id as usize),
                    "Per-strike advance changed to the rounded shared hmtx advance",
                );
            }
        }
        let f = &fonts.fonts[fi];
        for (field, expected, actual) in [
            ("ascent", s.ascent, scale(i64::from(header.ascender))),
            ("descent", s.descent, -scale(i64::from(header.descender))),
            (
                "line_gap",
                f.metrics.line_gap.unwrap_or(0),
                scale(i64::from(header.line_gap)),
            ),
        ] {
            if expected != actual {
                loss(
                    &mut report,
                    "sbix_line_metric",
                    Some(fi),
                    None,
                    format!("Per-strike {field} changed to the rounded shared design metric"),
                );
            }
        }
    }
    Ok((bytes, report.finish()?))
}
struct DesignGlyph {
    id: u16,
    width: u32,
    height: u32,
    horizontal: Option<[i64; 3]>,
    vertical: Option<[i64; 3]>,
}
struct DesignStrike {
    ppem: [u32; 2],
    ascent: i64,
    descent: i64,
    glyphs: Vec<DesignGlyph>,
}
/// Design-metric policy, explicit metadata and a prepared explicit name table.
struct CommonOptions<'a> {
    design: DesignOptions,
    metadata: &'a EmbeddedMetadataOptions,
    names: Option<Vec<u8>>,
}
#[allow(clippy::too_many_arguments)]
fn common_tables(
    builder: &mut SfntBuilder,
    fonts: &[Font],
    strikes: &[sbit::Strike],
    count: u16,
    mapping: &cmap::Mapping,
    glyph_names: Vec<String>,
    limits: &sfnt::Limits,
    report: &mut Diagnostics,
    options: CommonOptions<'_>,
) -> Result<()> {
    let design: Vec<_> = strikes
        .iter()
        .map(|s| DesignStrike {
            ppem: [u32::from(s.ppem_x), u32::from(s.ppem_y)],
            ascent: i64::from(s.horizontal.ascender()),
            descent: -i64::from(s.horizontal.descender()),
            glyphs: s
                .glyphs
                .iter()
                .map(|(&id, g)| {
                    let bearing =
                        |b: sbit::Bearing| [i64::from(b.x), i64::from(b.y), i64::from(b.advance)];
                    DesignGlyph {
                        id,
                        width: u32::from(g.metrics.width),
                        height: u32::from(g.metrics.height),
                        horizontal: g.metrics.horizontal.map(bearing),
                        vertical: g.metrics.vertical.map(bearing),
                    }
                })
                .collect(),
        })
        .collect();
    common_design_tables(
        builder,
        fonts,
        &design,
        count,
        mapping,
        glyph_names,
        limits,
        report,
        options,
    )
}
#[allow(clippy::too_many_arguments)]
fn common_design_tables(
    builder: &mut SfntBuilder,
    fonts: &[Font],
    strikes: &[DesignStrike],
    count: u16,
    mapping: &cmap::Mapping,
    glyph_names: Vec<String>,
    limits: &sfnt::Limits,
    report: &mut Diagnostics,
    common: CommonOptions<'_>,
) -> Result<()> {
    let CommonOptions {
        design: options,
        metadata,
        names: explicit_names,
    } = common;
    let order = options.priority(strikes, limits)?;
    let reference = order.first().copied().unwrap_or(0);
    let strike = &strikes[reference];
    let metric_font = &fonts[reference];
    // Names retain their original first-font provenance, independently of metrics.
    let f = &fonts[0];
    let ppem = i64::from(strike.ppem[1]);
    let scale = if ppem <= 163 {
        100
    } else if ppem <= 255 {
        64
    } else {
        (16384 / ppem).max(1)
    };
    let upem = options
        .units_per_em
        .unwrap_or((ppem * scale).min(16384) as u16);
    let global = |n: i64| -> Result<i16> {
        let value = i128::from(n) * i128::from(upem);
        i16::try_from(round_design(value, i128::from(ppem), options.rounding))
            .map_err(|_| invalid("design metric does not fit int16"))
    };
    let mut horizontal = vec![metrics::LongMetric::default(); count as usize];
    let mut ink = vec![None; count as usize];
    let mut seen = GlyphIdSet::default();
    let (mut left, mut bottom, mut right, mut top) = (0i64, 0i64, 0i64, 0i64);
    for index in 0..strikes.len() {
        let source = &strikes[order.get(index).copied().unwrap_or(index)];
        for g in &source.glyphs {
            let gid = g.id;
            if !seen.insert(gid) {
                continue;
            }
            let scaled = |n: i64, ppem: u32| {
                if options.rounding == EmbeddedRounding::NearestTiesAwayFromZero {
                    // Glyph coordinates/advances were bounded before reaching
                    // DesignGlyph (sbits: int8/uint8; sbix: int16/int32/uint32).
                    // Their products with a legal UPM fit i64. Keep the existing
                    // fast arithmetic on the default per-glyph path.
                    let value = n * i64::from(upem);
                    let denominator = i64::from(ppem);
                    return Ok((value
                        + if value >= 0 {
                            denominator / 2
                        } else {
                            -(denominator / 2)
                        })
                        / denominator);
                }
                i64::try_from(round_design(
                    i128::from(n) * i128::from(upem),
                    i128::from(ppem),
                    options.rounding,
                ))
                .map_err(|_| invalid("design metric overflow"))
            };
            let sx = |n: i64| scaled(n, source.ppem[0]);
            let sy = |n: i64| scaled(n, source.ppem[1]);
            let m = g;
            if let Some(h) = m.horizontal {
                if [h[2], h[0], m.width as i64]
                    .iter()
                    .any(|n| n * upem as i64 % source.ppem[0] as i64 != 0)
                {
                    loss(
                        report,
                        "design_rounding",
                        None,
                        Some(gid as usize),
                        "Global design metrics rounded from strike pixels; embedded pixel metrics remain exact",
                    );
                }
                horizontal[gid as usize] = metrics::LongMetric {
                    advance: u16::try_from(sx(h[2])?)
                        .map_err(|_| invalid("design advance overflow"))?,
                    bearing: int16(sx(h[0])?, "design bearing")?,
                };
            }
            if let Some(h) = m.horizontal.or(m.vertical) {
                left = left.min(sx(h[0])?);
                right = right.max(sx(h[0] + m.width as i64)?);
                bottom = bottom.min(sy(h[1] - m.height as i64)?);
                top = top.max(sy(h[1])?);
            }
            if m.width > 0 && m.height > 0 {
                ink[gid as usize] = Some(
                    u16::try_from(sx(m.width as i64)?)
                        .map_err(|_| invalid("design width overflow"))?,
                );
            }
        }
    }
    let mut header = metrics::Header::new(metrics::Axis::Horizontal);
    header.ascender = global(strike.ascent)?;
    header.descender = global(
        strike
            .descent
            .checked_neg()
            .ok_or_else(|| invalid("descent overflow"))?,
    )?;
    header.line_gap = global(metric_font.metrics.line_gap.unwrap_or(0))?;
    header.recompute(&horizontal, &ink)?;
    let hm = metrics::encode(
        metrics::Axis::Horizontal,
        &header,
        &horizontal,
        false,
        limits,
    )?;
    builder.insert(*b"hhea", hm.header)?;
    builder.insert(*b"hmtx", hm.metrics)?;
    let mut head = vec![0; 54];
    head[0..4].copy_from_slice(&0x10000u32.to_be_bytes());
    head[4..8].copy_from_slice(&0x10000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
    head[16..18].copy_from_slice(&0x0bu16.to_be_bytes());
    head[18..20].copy_from_slice(&upem.to_be_bytes());

    for (at, n) in [(36, left), (38, bottom), (40, right), (42, top)] {
        head[at..at + 2].copy_from_slice(&int16(n, "head bounds")?.to_be_bytes());
    }
    head[46..48].copy_from_slice(&8u16.to_be_bytes());
    head[48..50].copy_from_slice(&2u16.to_be_bytes());
    metadata.apply_head(&mut head);
    builder.insert(*b"head", head)?;
    let mut maxp = 0x5000u32.to_be_bytes().to_vec();
    maxp.extend(count.to_be_bytes());
    builder.insert(*b"maxp", maxp)?;
    let encoded = cmap::encode_records(
        mapping,
        count,
        limits,
        metadata.cmap_unicode_full == EmbeddedCmapUnicodeFull::Always,
    )?;
    builder.insert(*b"cmap", encoded.data)?;
    let mut os2 = os2::Os2Table::default();
    os2.set_unicode_coverage(&mapping.keys().copied().collect())?;
    os2.base.average_width = metadata.average(horizontal.iter().map(|m| m.advance))?;
    os2.base.typo_metrics = [header.ascender, header.descender, header.line_gap];
    os2.base.windows_metrics = [
        header.ascender.max(0) as u16,
        (-i32::from(header.descender)).max(0) as u16,
    ];
    metadata.apply_os2(
        &mut os2,
        mapping.last_key_value().is_some_and(|(&cp, _)| cp > 0xffff),
    )?;
    builder.insert(*b"OS/2", os2.encode(limits)?)?;
    if let Some(bytes) = explicit_names {
        builder.insert(*b"name", bytes)?;
    } else {
        let family = f.names.family.as_deref().unwrap_or("Fontpixel");
        let style = f.names.style.as_deref().unwrap_or("Regular");
        let full = format!("{family} {style}");
        let mut nt = name::NameTable::default();
        for (id, value) in [
            (0, f.names.copyright.as_deref()),
            (1, Some(family)),
            (2, Some(style)),
            (4, Some(f.names.full.as_deref().unwrap_or(&full))),
            (5, Some(f.names.version.as_deref().unwrap_or("Version 1.0"))),
            (6, f.names.postscript.as_deref()),
            (8, f.names.foundry.as_deref()),
            (13, f.names.license.as_deref()),
            (14, f.names.license_url.as_deref()),
        ] {
            if let Some(value) = value {
                nt.records.push(name::NameRecord::unicode(id, 0x409, value));
            }
        }
        builder.insert(*b"name", nt.encode(limits)?)?;
    }
    let mut post = post::PostTable {
        names: (metadata.post_names == EmbeddedPostNames::Keep).then_some(glyph_names),
        ..Default::default()
    };
    post.header.is_fixed_pitch = u32::from(metadata.fixed_pitch.unwrap_or(matches!(
        metric_font.metrics.spacing,
        Some(Spacing::Monospaced | Spacing::CharacterCell)
    )));
    post.header.underline_position = global(metric_font.metrics.underline_position.unwrap_or(0))?;
    post.header.underline_thickness = global(metric_font.metrics.underline_thickness.unwrap_or(0))?;
    builder.insert(*b"post", post.encode(count, limits)?)?;
    if strikes
        .iter()
        .any(|s| s.glyphs.iter().any(|g| g.vertical.is_some()))
    {
        loss(
            report,
            "design_vertical_metrics",
            Some(0),
            None,
            "Vertical pixel metrics are retained in sbits; design-space vhea/vmtx not generated",
        );
    }
    Ok(())
}
#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn design_rounding_preserves_legacy_and_handles_signed_exact_ties() {
        use EmbeddedRounding::{NearestTiesAwayFromZero as Away, NearestTiesEven as Even};
        for value in [
            i64::MIN,
            i64::MIN + 1,
            -1001,
            -52,
            -1,
            0,
            1,
            52,
            1001,
            i64::MAX,
        ] {
            for upem in [16, 1000, 16384] {
                let numerator = i128::from(value) * upem;
                for denominator in 1..=255 {
                    let legacy = (numerator
                        + if numerator >= 0 {
                            denominator / 2
                        } else {
                            -(denominator / 2)
                        })
                        / denominator;
                    assert_eq!(round_design(numerator, denominator, Away), legacy);
                }
            }
        }
        for (numerator, even, away) in [
            (1, 0, 1),
            (3, 2, 2),
            (5, 2, 3),
            (7, 4, 4),
            (-1, 0, -1),
            (-3, -2, -2),
            (-5, -2, -3),
            (-7, -4, -4),
        ] {
            assert_eq!(round_design(numerator, 2, Even), even);
            assert_eq!(round_design(numerator, 2, Away), away);
        }
        assert_eq!(round_design(52000, 64, Even), 812);
        assert_eq!(round_design(-52000, 64, Even), -812);
        assert_eq!(round_design(i128::MIN, 1, Even), i128::MIN);
        assert_eq!(round_design(i128::MAX, 1, Away), i128::MAX);
        assert_eq!(round_design(i128::MAX, 2, Even), 1i128 << 126);
        assert_eq!(round_design(i128::MIN + 1, 2, Even), -(1i128 << 126));
    }

    #[test]
    fn glyph_id_bitset_matches_tree_at_every_id_and_sparse_growth_boundary() {
        for first in [0, 63, 64, 127, 128, 1023, 32767, 64000, u16::MAX] {
            let mut seen = GlyphIdSet::default();
            let mut tree = BTreeSet::new();
            assert_eq!(seen.insert(first), tree.insert(first));
            for gid in 0..=u16::MAX {
                assert_eq!(seen.insert(gid), tree.insert(gid));
            }
            for gid in (0..=u16::MAX).rev() {
                assert_eq!(seen.insert(gid), tree.insert(gid));
            }
            assert!(seen.0.len() <= 1024);
            assert!(seen.0.capacity() <= 1024);
        }
    }

    // The exact former representation and derived category/sequence ordering.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum OldKey {
        Notdef,
        Codes(Vec<u32>),
        Name(String),
        Slot(usize),
    }
    fn old(key: &Key) -> OldKey {
        match key {
            Key::Notdef => OldKey::Notdef,
            Key::Code(cp) => OldKey::Codes(vec![*cp]),
            Key::Codes(cps) => OldKey::Codes(cps.clone()),
            Key::Name(name) => OldKey::Name(name.clone()),
            Key::Slot(index) => OldKey::Slot(*index),
        }
    }
    fn keys() -> Vec<Key> {
        let mut keys = vec![Key::Notdef, Key::Codes(Vec::new())];
        for cp in [0, 1, 2, 65, 66, 0xd7ff, 0xe000, 0x10000, 0x10ffff, u32::MAX] {
            keys.push(Key::Code(cp));
            keys.push(Key::Codes(vec![cp]));
            keys.push(Key::Codes(vec![cp, cp]));
            if cp < u32::MAX {
                keys.push(Key::Codes(vec![cp, cp + 1]));
            }
        }
        for name in ["", ".notdef", "A", "a", "bad/name", "永"] {
            keys.push(Key::Name(name.into()));
        }
        for index in [0, 1, 65535, usize::MAX] {
            keys.push(Key::Slot(index));
        }
        keys
    }

    #[test]
    fn inline_key_pairwise_order_and_equality_match_old_derived_order() {
        let keys = keys();
        for a in &keys {
            for b in &keys {
                assert_eq!(a.cmp(b), old(a).cmp(&old(b)), "{a:?} versus {b:?}");
                assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
                assert_eq!(a == b, old(a) == old(b), "{a:?} versus {b:?}");
            }
        }
        assert_eq!(Key::Code(65), Key::Codes(vec![65]));
        assert!(Key::Code(65) < Key::Codes(vec![65, 66]));
        assert!(Key::Codes(vec![2, 9]) < Key::Code(3));
    }

    #[test]
    fn inline_key_btree_overwrite_and_duplicate_identity_match_old_keys() {
        for reverse in [false, true] {
            let mut keys = keys();
            if reverse {
                keys.reverse();
            }
            let mut map = BTreeMap::new();
            let mut old_map = BTreeMap::new();
            let mut seen = BTreeSet::new();
            let mut old_seen = BTreeSet::new();
            for (value, key) in keys.into_iter().enumerate() {
                let old_key = old(&key);
                assert_eq!(seen.insert(key.clone()), old_seen.insert(old_key.clone()));
                assert_eq!(
                    map.insert(key.clone(), value),
                    old_map.insert(old_key, value)
                );
                assert_eq!(map.get(&key), old_map.get(&old(&key)));
            }
            assert_eq!(
                map.iter()
                    .map(|(key, value)| (old(key), *value))
                    .collect::<Vec<_>>(),
                old_map.into_iter().collect::<Vec<_>>()
            );
            assert_eq!(
                seen.iter().map(old).collect::<Vec<_>>(),
                old_seen.into_iter().collect::<Vec<_>>()
            );
        }
    }

    fn old_key(g: &Glyph, index: usize, fi: usize, report: &mut Diagnostics) -> OldKey {
        let mut cps = Vec::new();
        for cp in &g.codepoints {
            if let Codepoint::Unicode(cp) = cp
                && char::from_u32(*cp).is_some()
            {
                cps.push(*cp);
            } else {
                loss(
                    report,
                    "codepoint",
                    Some(fi),
                    Some(index),
                    format!("Cannot put {cp:?} in a Unicode cmap"),
                );
            }
        }
        cps.sort_unstable();
        cps.dedup();
        if !cps.is_empty() {
            OldKey::Codes(cps)
        } else if g.name.as_deref() == Some(".notdef") {
            OldKey::Notdef
        } else if let Some(name) = &g.name {
            OldKey::Name(name.clone())
        } else {
            OldKey::Slot(index)
        }
    }

    #[test]
    fn inline_key_selection_preserves_filtering_notdef_and_diagnostics() {
        use Codepoint::{Legacy as L, Unicode as U};
        for codes in [
            vec![],
            vec![U(0)],
            vec![U(65)],
            vec![U(0x10ffff)],
            vec![U(65), U(65)],
            vec![U(66), U(65), U(66)],
            vec![L(65)],
            vec![U(0xd800)],
            vec![U(0x110000)],
            vec![L(65), U(65), U(0xd800), U(65), U(0x110000)],
        ] {
            for name in [None, Some(".notdef"), Some(""), Some("A"), Some("bad/name")] {
                let mut glyph = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
                glyph.codepoints = codes.clone();
                glyph.name = name.map(str::to_owned);
                for budget in [0, 1024, 16 * 1024 * 1024] {
                    let mut actual_report = Diagnostics::new(budget);
                    let mut expected_report = Diagnostics::new(budget);
                    let actual = key(&glyph, 42, 3, &mut actual_report);
                    let expected = old_key(&glyph, 42, 3, &mut expected_report);
                    assert_eq!(old(&actual), expected);
                    if let [U(cp)] = codes.as_slice()
                        && char::from_u32(*cp).is_some()
                    {
                        assert!(matches!(actual, Key::Code(_)));
                    }
                    match (actual_report.finish(), expected_report.finish()) {
                        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
                        (Err(actual), Err(expected)) => {
                            assert_eq!(actual.to_string(), expected.to_string())
                        }
                        pair => panic!("diagnostic budget mismatch: {pair:?}"),
                    }
                }
            }
        }
    }
}
