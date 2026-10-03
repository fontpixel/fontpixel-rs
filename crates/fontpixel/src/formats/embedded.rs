//! Native FontSet projection for bitmap-only sfnt, Apple sbits, and color reads.
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
use std::collections::{BTreeMap, BTreeSet};

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
}
/// Feature-selected registry hooks; color exposes only read/detect.
#[derive(Clone, Debug)]
pub struct EmbeddedCodec {
    /// The intended bitmap table family.
    pub kind: sbit::Kind,
    /// Detailed extraction choices.
    pub read_options: EmbeddedReadOptions,
    /// Detailed OTB/bdat output choices.
    pub write_options: EmbeddedWriteOptions,
}
impl EmbeddedCodec {
    /// Construct one family with default budgets and exact gray preservation.
    pub fn new(kind: sbit::Kind) -> Self {
        Self {
            kind,
            read_options: EmbeddedReadOptions::default(),
            write_options: EmbeddedWriteOptions::default(),
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
    let message = message.into();
    let size = 256usize
        .saturating_add(message.len())
        .saturating_add(code.len());
    if report.exhausted || size > report.limit.saturating_sub(report.bytes) {
        report.exhausted = true;
        return;
    }
    report.bytes += size;
    report.report.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("embedded.{code}"),
        location: Location {
            font_index: fi,
            glyph_index: gi,
            ..Default::default()
        },
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
fn names(table: Option<&[u8]>, limits: &sfnt::Limits) -> Result<FontNames> {
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
fn finish_font(font: &mut Font) -> Result<()> {
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Notdef,
    Codes(Vec<u32>),
    Name(String),
    Slot(usize),
}
fn key(g: &Glyph, index: usize, fi: usize, report: &mut Diagnostics) -> Key {
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
fn ignored_fields(f: &Font, fi: usize, report: &mut Diagnostics) {
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
        ("resolution", m.resolution.is_some()),
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
    if kind == sbit::Kind::Color {
        return Err(invalid("color writing is not implemented"));
    }
    let limits = &options.limits;
    write_preflight(fonts, &limits.sfnt)?;
    let first = fonts
        .fonts
        .first()
        .ok_or_else(|| invalid("empty FontSet"))?;
    if fonts.fonts.len() > limits.max_strikes {
        return Err(invalid("too many strikes"));
    }
    let mut report = Diagnostics::new(limits.sfnt.resources.max_total_bytes);
    let mut ids = BTreeMap::from([(Key::Notdef, 0u16)]);
    let mut mapping = cmap::Mapping::new();
    let mut assigned = Vec::new();
    let mut names = vec![".notdef".to_owned()];
    let mut total = 0usize;
    for (fi, f) in fonts.fonts.iter().enumerate() {
        f.validate(&limits.sfnt.resources)?;
        ignored_fields(f, fi, &mut report);
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
        let mut seen = BTreeSet::new();
        for (id, g) in f.glyphs() {
            let key = key(g, id.index(), fi, &mut report);
            if !seen.insert(key.clone()) {
                return Err(invalid("ambiguous duplicate glyph labels within strike"));
            }
            let gid = if let Some(gid) = ids.get(&key) {
                *gid
            } else {
                let gid = u16::try_from(ids.len()).map_err(|_| invalid("too many sfnt glyphs"))?;
                if gid == u16::MAX {
                    return Err(invalid("sfnt requires at most 65535 glyphs"));
                }
                ids.insert(key.clone(), gid);
                let fallback = format!("glyph{gid}");
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
                names.push(name.unwrap_or(&fallback).to_owned());
                gid
            };
            if let Key::Codes(cps) = key {
                for cp in cps {
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
    let mut strikes = Vec::new();
    for (fi, (f, selected)) in fonts.fonts.iter().zip(&assigned).enumerate() {
        let ppem = f
            .metrics
            .pixel_size
            .or(f.metrics.ppem_y)
            .ok_or_else(|| invalid("missing pixel size"))?;
        let x = byte(f.metrics.ppem_x.unwrap_or(ppem) as i64, "ppem x")?;
        let y = byte(f.metrics.ppem_y.unwrap_or(ppem) as i64, "ppem y")?;
        let depth = if selected
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
            if depth == 8 && matches!(g.bitmap, GlyphBitmap::Mono(_)) {
                loss(
                    &mut report,
                    "depth_normalized",
                    Some(fi),
                    Some(gid as usize),
                    "Mono represented at shared 8-bit strike depth without changing coverage",
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
        strikes.push(strike);
    }
    let mut sizes = BTreeSet::new();
    for s in &strikes {
        if !sizes.insert((s.ppem_x, s.ppem_y)) {
            return Err(invalid("duplicate physical strike size"));
        }
    }
    let tables = sbit::encode(&strikes, kind, count, limits)?;
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
        first,
        &strikes,
        count,
        &mapping,
        names,
        &limits.sfnt,
        &mut report,
    )?;
    let report = report.finish()?;
    Ok((builder.build(&limits.sfnt)?, report))
}
#[allow(clippy::too_many_arguments)]
fn common_tables(
    builder: &mut SfntBuilder,
    f: &Font,
    strikes: &[sbit::Strike],
    count: u16,
    mapping: &cmap::Mapping,
    glyph_names: Vec<String>,
    limits: &sfnt::Limits,
    report: &mut Diagnostics,
) -> Result<()> {
    let strike = &strikes[0];
    let scale = if strike.ppem_y <= 163 { 100i64 } else { 64 };
    let upem = strike.ppem_y as u16 * scale as u16;
    let mut horizontal = vec![metrics::LongMetric::default(); count as usize];
    let mut ink = vec![None; count as usize];
    let mut seen = BTreeSet::new();
    let (mut left, mut bottom, mut right, mut top) = (0i64, 0i64, 0i64, 0i64);
    for source in strikes {
        for (&gid, g) in &source.glyphs {
            if !seen.insert(gid) {
                continue;
            }
            let sx = |n: i64| {
                (n * upem as i64
                    + if n >= 0 {
                        source.ppem_x as i64 / 2
                    } else {
                        -(source.ppem_x as i64 / 2)
                    })
                    / source.ppem_x as i64
            };
            let sy = |n: i64| {
                (n * upem as i64
                    + if n >= 0 {
                        source.ppem_y as i64 / 2
                    } else {
                        -(source.ppem_y as i64 / 2)
                    })
                    / source.ppem_y as i64
            };
            let m = g.metrics;
            if let Some(h) = m.horizontal {
                if [h.advance as i64, h.x as i64, m.width as i64]
                    .iter()
                    .any(|n| n * upem as i64 % source.ppem_x as i64 != 0)
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
                    advance: u16::try_from(sx(h.advance as i64))
                        .map_err(|_| invalid("design advance overflow"))?,
                    bearing: int16(sx(h.x as i64), "design bearing")?,
                };
            }
            if let Some(h) = m.horizontal.or(m.vertical) {
                left = left.min(sx(h.x as i64));
                right = right.max(sx(h.x as i64 + m.width as i64));
                bottom = bottom.min(sy(h.y as i64 - m.height as i64));
                top = top.max(sy(h.y as i64));
            }
            if m.width > 0 && m.height > 0 {
                ink[gid as usize] = Some(
                    u16::try_from(sx(m.width as i64))
                        .map_err(|_| invalid("design width overflow"))?,
                );
            }
        }
    }
    let mut header = metrics::Header::new(metrics::Axis::Horizontal);
    header.ascender = int16(strike.horizontal.ascender() as i64 * scale, "design ascent")?;
    header.descender = int16(
        strike.horizontal.descender() as i64 * scale,
        "design descent",
    )?;
    header.line_gap = int16(
        f.metrics
            .line_gap
            .unwrap_or(0)
            .checked_mul(scale)
            .ok_or_else(|| invalid("line gap overflow"))?,
        "line gap",
    )?;
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
    builder.insert(*b"head", head)?;
    let mut maxp = 0x5000u32.to_be_bytes().to_vec();
    maxp.extend(count.to_be_bytes());
    builder.insert(*b"maxp", maxp)?;
    let encoded = cmap::encode(mapping, count, limits)?;
    builder.insert(*b"cmap", encoded.data)?;
    let mut os2 = os2::Os2Table::default();
    os2.set_unicode_coverage(&mapping.keys().copied().collect())?;
    os2.base.average_width = os2::average_width(horizontal.iter().map(|m| m.advance))?;
    os2.base.typo_metrics = [header.ascender, header.descender, header.line_gap];
    os2.base.windows_metrics = [
        header.ascender.max(0) as u16,
        (-i32::from(header.descender)).max(0) as u16,
    ];
    builder.insert(*b"OS/2", os2.encode(limits)?)?;
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
    let mut post = post::PostTable {
        names: Some(glyph_names),
        ..Default::default()
    };
    post.header.is_fixed_pitch = u32::from(matches!(
        f.metrics.spacing,
        Some(Spacing::Monospaced | Spacing::CharacterCell)
    ));
    post.header.underline_position = int16(
        f.metrics
            .underline_position
            .unwrap_or(0)
            .checked_mul(scale)
            .ok_or_else(|| invalid("underline overflow"))?,
        "underline position",
    )?;
    post.header.underline_thickness = int16(
        f.metrics
            .underline_thickness
            .unwrap_or(0)
            .checked_mul(scale)
            .ok_or_else(|| invalid("underline overflow"))?,
        "underline thickness",
    )?;
    builder.insert(*b"post", post.encode(count, limits)?)?;
    if strikes
        .iter()
        .any(|s| s.glyphs.values().any(|g| g.metrics.vertical.is_some()))
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
