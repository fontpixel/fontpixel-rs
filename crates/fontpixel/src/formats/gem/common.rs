//! Private bounded wire and diagnostic helpers shared by legacy binary codecs.
pub use crate::bitmap::ByteOrder;
use crate::bitmap::{Bitmap, BitmapLimits, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Properties, PropertyValue};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};

pub fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
pub fn check(n: usize, limit: usize, resource: &'static str) -> Result<()> {
    if n > limit {
        Err(Error::ResourceLimit { resource, limit })
    } else {
        Ok(())
    }
}
pub fn slice(b: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    b.get(
        at..at
            .checked_add(n)
            .ok_or_else(|| invalid("offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated font data"))
}
pub fn u16_at(b: &[u8], at: usize, e: ByteOrder) -> Result<u16> {
    let b = slice(b, at, 2)?;
    Ok(match e {
        ByteOrder::Big => u16::from_be_bytes([b[0], b[1]]),
        ByteOrder::Little => u16::from_le_bytes([b[0], b[1]]),
    })
}
pub fn u32_at(b: &[u8], at: usize, e: ByteOrder) -> Result<u32> {
    let b = slice(b, at, 4)?;
    Ok(match e {
        ByteOrder::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        ByteOrder::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    })
}
pub fn put16(b: &mut [u8], at: usize, v: u16, e: ByteOrder) {
    b[at..at + 2].copy_from_slice(&match e {
        ByteOrder::Big => v.to_be_bytes(),
        ByteOrder::Little => v.to_le_bytes(),
    });
}
pub fn put32(b: &mut [u8], at: usize, v: u32, e: ByteOrder) {
    b[at..at + 4].copy_from_slice(&match e {
        ByteOrder::Big => v.to_be_bytes(),
        ByteOrder::Little => v.to_le_bytes(),
    });
}
pub fn latin(b: &[u8]) -> String {
    b.iter().map(|&x| char::from(x)).collect()
}
#[cfg(any(feature = "amiga", feature = "snf"))]
pub fn latin_bytes(s: &str) -> Result<Vec<u8>> {
    s.chars()
        .map(|c| {
            u8::try_from(u32::from(c)).map_err(|_| invalid("name is not representable as Latin-1"))
        })
        .collect()
}
pub fn int(p: &Properties, k: &str, default: i64) -> Result<i64> {
    match p.get(k) {
        None => Ok(default),
        Some(PropertyValue::Integer(v)) => Ok(*v),
        Some(PropertyValue::Unsigned(v)) => {
            i64::try_from(*v).map_err(|_| invalid(format!("{k} is out of range")))
        }
        _ => Err(invalid(format!("{k} must be an integer"))),
    }
}

pub struct Budget {
    pub used: usize,
    pub max: usize,
}
impl Budget {
    pub fn new(used: usize, l: &ResourceLimits) -> Result<Self> {
        check(used, l.max_total_bytes, "legacy aggregate bytes")?;
        Ok(Self {
            used,
            max: l.max_total_bytes,
        })
    }
    pub fn add(&mut self, n: usize) -> Result<()> {
        self.used = self.used.checked_add(n).ok_or(Error::ResourceLimit {
            resource: "legacy aggregate bytes",
            limit: self.max,
        })?;
        check(self.used, self.max, "legacy aggregate bytes")
    }
    pub fn items<T>(&mut self, n: usize) -> Result<()> {
        self.add(
            n.checked_mul(std::mem::size_of::<T>())
                .and_then(|n| n.checked_mul(2))
                .ok_or(Error::ResourceLimit {
                    resource: "legacy allocation",
                    limit: self.max,
                })?,
        )
    }
    // All diagnostic coordinates are explicit at each call site.
    #[allow(clippy::too_many_arguments)]
    pub fn diagnostic(
        &mut self,
        r: &mut Report,
        format: &str,
        code: &str,
        impact: Impact,
        font: Option<usize>,
        glyph: Option<usize>,
        text: impl Into<String>,
    ) -> Result<()> {
        let message = text.into();
        self.add(
            2 * std::mem::size_of::<ReportEntry>()
                + 2 * (format.len() + code.len() + message.len() + 1),
        )?;
        r.push(ReportEntry {
            level: Level::Warning,
            impact,
            code: format!("{format}.{code}"),
            location: Location {
                font_index: font,
                glyph_index: glyph,
                ..Default::default()
            },
            message,
        });
        Ok(())
    }
    pub fn loss(
        &mut self,
        r: &mut Report,
        format: &str,
        code: &str,
        font: Option<usize>,
        glyph: Option<usize>,
        text: impl Into<String>,
    ) -> Result<()> {
        self.diagnostic(r, format, code, Impact::Loss, font, glyph, text)
    }
}
pub fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
        ..Default::default()
    }
}
pub fn mono(g: &Glyph) -> Result<&Bitmap<Mono>> {
    match &g.bitmap {
        crate::model::GlyphBitmap::Mono(b) => Ok(b),
        _ => Err(invalid(
            "monochrome font requires an explicit grayscale conversion",
        )),
    }
}
/// Extract a non-byte-aligned range without expanding a strip into pixels.
#[cfg(any(feature = "gem", feature = "amiga"))]
pub fn extract(
    strip: &[u8],
    stride: usize,
    x: usize,
    w: u32,
    h: u32,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Bitmap<Mono>> {
    if x.checked_add(w as usize)
        .is_none_or(|v| v > stride.saturating_mul(8))
    {
        return Err(invalid("glyph extends past bitmap strip"));
    }
    check(
        (w as usize)
            .checked_mul(h as usize)
            .ok_or_else(|| invalid("glyph area overflow"))?,
        l.max_pixels_per_glyph,
        "glyph pixels",
    )?;
    let row = (w as usize).div_ceil(8);
    let n = row
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("bitmap overflow"))?;
    budget.add(n.saturating_mul(2))?;
    let mut out = vec![0; n];
    for y in 0..h as usize {
        let src = slice(strip, y * stride, stride)?;
        for j in 0..row {
            let at = x / 8 + j;
            let shift = x % 8;
            out[y * row + j] = (src.get(at).copied().unwrap_or(0) << shift)
                | if shift == 0 {
                    0
                } else {
                    src.get(at + 1).copied().unwrap_or(0) >> (8 - shift)
                };
        }
    }
    Bitmap::from_raw(w, h, &out, bitmap_limits(l))
}
/// Paste a packed glyph into a strip; source bounds and total allocation are checked by caller.
#[cfg(any(feature = "gem", feature = "amiga"))]
pub fn insert(strip: &mut [u8], stride: usize, x: usize, top: usize, b: &Bitmap<Mono>) {
    let shift = x % 8;
    for y in 0..b.height() as usize {
        for (j, &v) in b.as_bytes()[y * b.stride()..(y + 1) * b.stride()]
            .iter()
            .enumerate()
        {
            let at = (top + y) * stride + x / 8 + j;
            strip[at] |= v >> shift;
            if shift != 0 && at + 1 < (top + y + 1) * stride {
                strip[at + 1] |= v << (8 - shift);
            }
        }
    }
}
fn value_cost(v: &PropertyValue) -> usize {
    value_cost_at(v, 0)
}
fn value_cost_at(v: &PropertyValue, depth: usize) -> usize {
    if depth > 128 {
        return usize::MAX;
    }
    match v {
        PropertyValue::Text(s) | PropertyValue::Decimal(s) => s.len(),
        PropertyValue::Bytes(b) => b.len(),
        PropertyValue::List(v) => v.iter().fold(
            v.len().saturating_mul(std::mem::size_of::<PropertyValue>()),
            |n, v| n.saturating_add(value_cost_at(v, depth + 1)),
        ),
        _ => 0,
    }
}
fn property_cost(p: &Properties) -> usize {
    p.iter().fold(
        p.len()
            .saturating_mul(std::mem::size_of::<crate::model::Property>()),
        |n, p| {
            n.saturating_add(p.name.len())
                .saturating_add(p.raw.as_ref().map_or(0, Vec::len))
                .saturating_add(value_cost(&p.value))
        },
    )
}
pub fn source_budget(set: &FontSet, l: &ResourceLimits) -> Result<Budget> {
    set.validate(l)?;
    let mut b = Budget::new(property_cost(&set.properties).saturating_mul(8), l)?;
    // Keep capacity for the final error diagnostic if encoding fails after omissions.
    b.add(1024)?;
    b.items::<Font>(set.fonts.len())?;
    for f in &set.fonts {
        b.items::<Option<Glyph>>(f.slot_count())?;
        b.items::<(u32, u32)>(f.charset.code_ranges.len())?;
        b.items::<String>(f.names.xlfd_fields.len())?;
        b.add(
            property_cost(&f.headers)
                .saturating_add(property_cost(&f.properties))
                .saturating_add(property_cost(&f.names.localized))
                .saturating_mul(8),
        )?;
        for s in [
            &f.names.family,
            &f.names.style,
            &f.names.full,
            &f.names.postscript,
            &f.names.version,
            &f.names.copyright,
            &f.names.license,
            &f.names.license_url,
            &f.names.foundry,
            &f.names.xlfd,
            &f.charset.registry,
            &f.charset.encoding,
            &f.charset.name,
            &f.source_format,
        ]
        .into_iter()
        .flatten()
        {
            b.add(s.len())?;
        }
        b.add(f.names.xlfd_fields.iter().map(String::len).sum::<usize>())?;
        for (_, g) in f.glyphs() {
            if i32::try_from(g.metrics.bounds.x).is_err()
                || i32::try_from(g.metrics.bounds.y).is_err()
                || g.metrics
                    .advance
                    .is_some_and(|v| i32::try_from(v.x).is_err())
            {
                return Err(invalid(
                    "legacy font glyph metrics exceed safe 32-bit coordinates",
                ));
            }
            b.add(
                g.bitmap
                    .as_bytes()
                    .len()
                    .saturating_add(property_cost(&g.properties).saturating_mul(8))
                    .saturating_add(g.name.as_ref().map_or(0, String::len)),
            )?;
            b.add(g.codepoints.len().saturating_mul(128))?;
            for s in &g.unicode_sequences {
                b.add(s.len().saturating_mul(24))?;
            }
        }
    }
    Ok(b)
}
// The two property sequences share the audit location and output budget.
#[allow(clippy::too_many_arguments)]
fn properties(
    a: &Properties,
    b: &Properties,
    label: &str,
    format: &str,
    fi: usize,
    gi: Option<usize>,
    r: &mut Report,
    budget: &mut Budget,
) -> Result<()> {
    let mut bi = b.iter();
    for (i, p) in a.iter().enumerate() {
        if bi.next() != Some(p) {
            budget.loss(
                r,
                format,
                "write.property",
                Some(fi),
                gi,
                format!(
                    "{label} property {} ({:?}) is changed or omitted",
                    i, p.name
                ),
            )?;
        }
    }
    if b.len() > a.len() {
        budget.loss(
            r,
            format,
            "write.property_added",
            Some(fi),
            gi,
            format!("{label} gains {} format properties", b.len() - a.len()),
        )?;
    }
    Ok(())
}
/// Compare all public model fields with the actual decoded target. No-loss means model equality, excluding source-format provenance.
pub fn audit(
    a: &FontSet,
    b: &FontSet,
    format: &str,
    r: &mut Report,
    budget: &mut Budget,
) -> Result<()> {
    properties(
        &a.properties,
        &b.properties,
        "container",
        format,
        0,
        None,
        r,
        budget,
    )?;
    if a.fonts.len() != b.fonts.len() {
        budget.loss(
            r,
            format,
            "write.font_count",
            None,
            None,
            "Strike count changed",
        )?;
    }
    for (fi, (a, b)) in a.fonts.iter().zip(&b.fonts).enumerate() {
        macro_rules! fields {($scope:ident,$code:literal,$($field:ident),*)=>{$(if a.$scope.$field!=b.$scope.$field{budget.loss(r,format,$code,Some(fi),None,concat!(stringify!($scope),".",stringify!($field)," changed"))?;})*}}
        fields!(
            metrics,
            "write.font_metric",
            pixel_size,
            ppem_x,
            ppem_y,
            point_size,
            resolution,
            ascent,
            descent,
            bounds,
            line_gap,
            line_height,
            underline_position,
            underline_thickness,
            default_glyph,
            default_character,
            space_width,
            spacing,
            glyph_defaults,
            metrics_set
        );
        fields!(
            names,
            "write.name",
            family,
            style,
            full,
            postscript,
            version,
            copyright,
            license,
            license_url,
            foundry,
            xlfd,
            xlfd_fields
        );
        properties(
            &a.names.localized,
            &b.names.localized,
            "localized name",
            format,
            fi,
            None,
            r,
            budget,
        )?;
        if a.charset != b.charset {
            budget.loss(
                r,
                format,
                "write.charset",
                Some(fi),
                None,
                "Charset metadata changed",
            )?;
        }
        properties(
            &a.headers, &b.headers, "header", format, fi, None, r, budget,
        )?;
        properties(
            &a.properties,
            &b.properties,
            "font",
            format,
            fi,
            None,
            r,
            budget,
        )?;
        if a.slot_count() != a.len() {
            budget.loss(
                r,
                format,
                "write.slots",
                Some(fi),
                None,
                "Removed glyph slots are compacted",
            )?;
        }
        budget.items::<&Glyph>(b.len())?;
        let target_glyphs: Vec<_> = b.glyphs().map(|(_, g)| g).collect();
        for (id, g) in a.glyphs() {
            let target = g
                .codepoints
                .iter()
                .find_map(|c| b.glyph_for_codepoint(*c))
                .or_else(|| target_glyphs.get(id.index()).copied());
            let Some(h) = target else {
                budget.loss(
                    r,
                    format,
                    "write.glyph_omitted",
                    Some(fi),
                    Some(id.index()),
                    "Glyph is omitted",
                )?;
                continue;
            };
            if target_glyphs.get(id.index()).copied() != Some(h) {
                budget.loss(
                    r,
                    format,
                    "write.glyph_order",
                    Some(fi),
                    Some(id.index()),
                    "Glyph moves to another slot",
                )?;
            }
            for cp in &g.codepoints {
                if !h.codepoints.contains(cp) {
                    budget.loss(
                        r,
                        format,
                        "write.mapping",
                        Some(fi),
                        Some(id.index()),
                        format!("Mapping {cp:?} is omitted or changes domain"),
                    )?;
                }
            }
            if g.codepoints != h.codepoints {
                budget.loss(
                    r,
                    format,
                    "write.aliases",
                    Some(fi),
                    Some(id.index()),
                    "Mapping order or alias topology changes",
                )?;
            }
            if g.unicode_sequences != h.unicode_sequences {
                budget.loss(
                    r,
                    format,
                    "write.sequences",
                    Some(fi),
                    Some(id.index()),
                    "Unicode sequences changed",
                )?;
            }
            if g.name != h.name {
                budget.loss(
                    r,
                    format,
                    "write.glyph_name",
                    Some(fi),
                    Some(id.index()),
                    "Glyph name changed",
                )?;
            }
            macro_rules! gm {($($field:ident),*)=>{$(if g.metrics.$field!=h.metrics.$field{budget.loss(r,format,"write.glyph_metric",Some(fi),Some(id.index()),concat!("Glyph metric ",stringify!($field)," changed"))?;})*}}
            gm!(
                bounds,
                advance,
                scalable_advance,
                vertical_advance,
                vertical_scalable_advance,
                vertical_origin,
                abc,
                left_bearing,
                right_bearing,
                image_offset
            );
            if g.bitmap != h.bitmap {
                budget.loss(
                    r,
                    format,
                    "write.bitmap",
                    Some(fi),
                    Some(id.index()),
                    "Glyph bitmap rectangle or pixels changed",
                )?;
            }
            properties(
                &g.properties,
                &h.properties,
                "glyph",
                format,
                fi,
                Some(id.index()),
                r,
                budget,
            )?;
        }
        if a.len() != b.len() {
            budget.loss(
                r,
                format,
                "write.glyph_count",
                Some(fi),
                None,
                "Glyph count changed (range gaps or alias copies)",
            )?;
        }
    }
    Ok(())
}
pub fn select<'a>(
    f: &'a Font,
    max: u32,
    unicode: bool,
    format: &str,
    r: &mut Report,
    b: &mut Budget,
) -> Result<std::collections::BTreeMap<u32, &'a Glyph>> {
    let mut slots = std::collections::BTreeMap::new();
    for (id, g) in f.glyphs() {
        if g.codepoints.is_empty() && !(format == "amiga" && f.metrics.default_glyph == Some(id)) {
            b.loss(
                r,
                format,
                "write.unencoded_glyph",
                Some(0),
                Some(id.index()),
                "Unencoded glyph has no target character slot",
            )?;
        }
        for cp in &g.codepoints {
            let code = match cp {
                Codepoint::Legacy(v) => Some(*v),
                Codepoint::Unicode(v) if unicode => Some(*v),
                _ => None,
            };
            if let Some(code) = code.filter(|v| *v <= max) {
                b.add(64)?;
                if slots.insert(code, g).is_some() {
                    b.loss(
                        r,
                        format,
                        "write.duplicate_mapping",
                        Some(0),
                        Some(id.index()),
                        format!("Duplicate mapping {cp:?}; last glyph is used"),
                    )?;
                }
            } else {
                b.loss(
                    r,
                    format,
                    "write.unrepresentable_mapping",
                    Some(0),
                    Some(id.index()),
                    format!("Mapping {cp:?} cannot be encoded"),
                )?;
            }
        }
    }
    if slots.is_empty() {
        b.diagnostic(
            r,
            format,
            "no_representable_glyphs",
            Impact::Loss,
            Some(0),
            None,
            "No representable character mappings remain",
        )?;
        if let Some(e) = r.entries.last_mut() {
            e.level = Level::Error;
        }
        return Err(Error::ReportedErrors(std::mem::take(r)));
    }
    Ok(slots)
}

/// Clone only when the caller has explicitly requested thresholding gray input.
pub fn convert_gray<'a>(
    s: &'a FontSet,
    threshold: Option<u8>,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<std::borrow::Cow<'a, FontSet>> {
    if !s.fonts.iter().any(|f| {
        f.glyphs()
            .any(|(_, g)| matches!(g.bitmap, crate::model::GlyphBitmap::Gray8(_)))
    }) {
        return Ok(std::borrow::Cow::Borrowed(s));
    }
    let threshold = threshold.ok_or_else(|| invalid("grayscale input requires gray_threshold"))?;
    budget.add(source_budget(s, l)?.used)?;
    let mut out = s.clone();
    for f in &mut out.fonts {
        budget.items::<crate::model::GlyphId>(f.len())?;
        let ids: Vec<_> = f
            .glyphs()
            .filter_map(|(id, g)| {
                matches!(g.bitmap, crate::model::GlyphBitmap::Gray8(_)).then_some(id)
            })
            .collect();
        for id in ids {
            f.edit_glyph(id, |g| {
                if let crate::model::GlyphBitmap::Gray8(b) = &g.bitmap {
                    g.bitmap = b.to_mono(threshold)?.into();
                }
                Ok(())
            })?;
        }
    }
    Ok(std::borrow::Cow::Owned(out))
}

/// Preserve per-mapping omissions when a later encoding step fails.
pub fn finish(
    result: Result<crate::resources::OutputBundle>,
    mut report: Report,
    format: &str,
    strict: bool,
) -> Result<crate::resources::Written> {
    match result {
        Ok(output) => {
            if strict && report.has_loss() {
                Err(Error::StrictLoss(report))
            } else {
                Ok(crate::resources::Written { output, report })
            }
        }
        Err(Error::ReportedErrors(extra)) => {
            report.extend(extra);
            Err(Error::ReportedErrors(report))
        }
        Err(error) => {
            if report.entries.is_empty() {
                return Err(error);
            }
            report.push(ReportEntry {
                level: Level::Error,
                impact: Impact::Loss,
                code: format!("{format}.write.failed"),
                location: Location::default(),
                message: error.to_string(),
            });
            Err(Error::ReportedErrors(report))
        }
    }
}
