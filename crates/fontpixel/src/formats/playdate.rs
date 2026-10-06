//! Native Playdate source atlases and compiled PFT fonts.
use super::device_common::*;
use super::{Confidence, WriteOptions};
use crate::model::{Font, FontSet, Glyph, Property, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
mod fnt;
mod pft;
mod png;
/// Native output representations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PlaydateFlavor {
    /// Standalone FNT with base64 PNG.
    #[default]
    #[cfg_attr(feature = "serde", serde(rename = "fnt-embedded"))]
    FntEmbedded,
    /// FNT plus a native PNG image-table sidecar.
    #[cfg_attr(feature = "serde", serde(rename = "fnt-png"))]
    FntPng,
    /// Uncompressed compiled font accepted by the runtime.
    Pft,
}
/// Explicit sidecar choice for source files without cell dimensions.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PlaydateReadOptions {
    /// Caller-authorized PNG atlas path. If absent, derive from FNT name and dimensions.
    pub atlas: Option<ResourcePath>,
    /// Override cell width when the native FNT omits it.
    pub cell_width: Option<u32>,
    /// Override cell height when the native FNT omits it.
    pub cell_height: Option<u32>,
}
/// Bounded native output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PlaydateWriteOptions {
    /// Native source or compiled representation.
    pub flavor: PlaydateFlavor,
    /// Override atlas cell width.
    pub cell_width: Option<u32>,
    /// Override atlas cell height.
    pub cell_height: Option<u32>,
    /// Override spacing added to each glyph's native width.
    pub tracking: Option<i16>,
    /// Explicit grayscale-to-monochrome conversion threshold.
    pub gray_threshold: Option<u8>,
    /// Cumulative source, image, output and readback budget.
    pub limits: ResourceLimits,
    /// Additional primary/aggregate output cap.
    pub max_output_bytes: usize,
}
impl Default for PlaydateWriteOptions {
    fn default() -> Self {
        Self {
            flavor: PlaydateFlavor::FntEmbedded,
            cell_width: None,
            cell_height: None,
            tracking: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(PlaydateCodec, PlaydateReadOptions, PlaydateWriteOptions);
/// Detect PFT magic or a structurally plausible FNT source.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"Playdate FNT") {
        Confidence::Certain
    } else if std::str::from_utf8(b).is_ok_and(|s| {
        s.lines().any(|s| s.trim().starts_with("tracking="))
            && s.lines()
                .any(|s| s.trim().starts_with("data=") || s.trim().starts_with("--metrics="))
    }) {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Read a native standalone font or caller-resolved atlas.
pub fn read(c: &mut ReadContext<'_>, v: &PlaydateReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    if c.bytes().starts_with(b"Playdate FNT") {
        pft::parse(c.bytes(), c.limits(), &mut budget)
    } else {
        fnt::read(c, v, &mut budget)
    }
}
struct Entry<'a> {
    cp: u32,
    glyph: &'a Glyph,
    width: u8,
}
struct Prepared<'a> {
    entries: Vec<Entry<'a>>,
    width: u32,
    height: u32,
    ascent: i64,
    tracking: i16,
    kern: BTreeMap<(u32, u32), i8>,
}
fn kern_property(kern: &BTreeMap<(u32, u32), i8>) -> Property {
    Property::new(
        "playdate.kerning",
        PropertyValue::List(
            kern.iter()
                .map(|(&(a, b), &v)| {
                    PropertyValue::List(vec![
                        PropertyValue::Integer(i64::from(a)),
                        PropertyValue::Integer(i64::from(b)),
                        PropertyValue::Integer(i64::from(v)),
                    ])
                })
                .collect(),
        ),
    )
}
fn native_font(
    w: u32,
    h: u32,
    ascent: i64,
    tracking: i16,
    kern: &BTreeMap<(u32, u32), i8>,
) -> Font {
    let mut f = Font::default();
    f.metrics.ascent = Some(ascent);
    f.metrics.descent = Some(i64::from(h) - ascent);
    f.metrics.pixel_size = Some(h);
    f.metrics.line_height = Some(i64::from(h));
    f.properties
        .push(Property::new("playdate.tracking", i64::from(tracking)));
    f.properties
        .push(Property::new("playdate.cell_width", i64::from(w)));
    f.properties
        .push(Property::new("playdate.cell_height", i64::from(h)));
    f.properties.push(kern_property(kern));
    f
}
fn prepare<'a>(f: &'a Font, v: &PlaydateWriteOptions, budget: &mut Budget) -> Result<Prepared<'a>> {
    let map = select(f, 0x3ffff, true, false, budget)?;
    check(map.len(), v.limits.max_glyphs, "Playdate glyphs")?;
    let tracking = v.tracking.unwrap_or(
        i16::try_from(int(&f.properties, "playdate.tracking", 0)?)
            .map_err(|_| invalid("Playdate tracking exceeds i16"))?,
    );
    let width = v.cell_width.unwrap_or(
        u32::try_from(int(
            &f.properties,
            "playdate.cell_width",
            map.values()
                .map(|g| g.metrics.bounds.x + i64::from(g.bitmap.width()))
                .max()
                .unwrap_or(1)
                .max(1),
        )?)
        .map_err(|_| invalid("invalid Playdate cell width"))?,
    );
    let height = v.cell_height.unwrap_or(
        u32::try_from(int(
            &f.properties,
            "playdate.cell_height",
            f.metrics
                .line_height
                .or(f.metrics.pixel_size.map(i64::from))
                .unwrap_or_else(|| {
                    map.values()
                        .map(|g| i64::from(g.bitmap.height()))
                        .max()
                        .unwrap_or(1)
                }),
        )?)
        .map_err(|_| invalid("invalid Playdate cell height"))?,
    );
    if width == 0 || height == 0 {
        return Err(invalid("Playdate cell must have positive dimensions"));
    }
    check(
        (width as usize).saturating_mul(height as usize),
        v.limits.max_pixels_per_glyph,
        "Playdate cell pixels",
    )?;
    let ascent = f.metrics.ascent.unwrap_or(i64::from(height));
    i32::try_from(ascent).map_err(|_| invalid("Playdate baseline exceeds i32"))?;
    budget.items::<Entry<'_>>(map.len())?;
    let mut entries = Vec::with_capacity(map.len());
    for (cp, g) in map {
        let adv = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .map_or(i64::from(g.bitmap.width()), |a| a.x);
        entries.push(Entry {
            cp,
            glyph: g,
            width: byte(
                adv.checked_sub(i64::from(tracking))
                    .ok_or_else(|| invalid("Playdate advance overflow"))?,
            )?,
        });
    }
    let mut kern = BTreeMap::new();
    if let Some(value) = f.properties.get("playdate.kerning") {
        let PropertyValue::List(list) = value else {
            return Err(invalid("playdate.kerning must be a list"));
        };
        for pair in list {
            let PropertyValue::List(triple) = pair else {
                return Err(invalid("Playdate kerning entry must be a triple"));
            };
            let [
                PropertyValue::Integer(a),
                PropertyValue::Integer(b),
                PropertyValue::Integer(k),
            ] = triple.as_slice()
            else {
                return Err(invalid("Playdate kerning triple must contain integers"));
            };
            let (a, b, k) = (
                u32::try_from(*a).map_err(|_| invalid("invalid kerning codepoint"))?,
                u32::try_from(*b).map_err(|_| invalid("invalid kerning codepoint"))?,
                i8::try_from(*k).map_err(|_| invalid("Playdate kerning exceeds i8"))?,
            );
            if a > 0x3ffff
                || b > 0x3ffff
                || char::from_u32(a).is_none()
                || char::from_u32(b).is_none()
            {
                return Err(invalid("Playdate kerning codepoint is out of range"));
            }
            budget.add(128)?;
            kern.insert((a, b), k);
        }
    }
    Ok(Prepared {
        entries,
        width,
        height,
        ascent,
        tracking,
        kern,
    })
}
/// Write real source/compiled data and audit a cumulative-budget native readback.
pub fn write(s: &FontSet, o: &WriteOptions, v: &PlaydateWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let prepared = prepare(f, v, &mut budget)?;
        let (out, target, r) = if v.flavor == PlaydateFlavor::Pft {
            let bytes = pft::encode(&prepared, v, &mut budget)?;
            let (target, r) = pft::parse(&bytes, &v.limits, &mut budget)?;
            (
                OutputBundle::single(o.primary_name.clone(), bytes),
                target,
                r,
            )
        } else {
            fnt::encode(f, &prepared, o, v, &mut budget)?
        };
        report.extend(r);
        audit(s, &target, "playdate", &mut report, &mut budget)?;
        Ok(out)
    })();
    finish(result, report, "playdate", o.strict)
}
