//! Field-level provenance and normalized metadata. Never caches complete BDF output.
use super::*;
use crate::bitmap::Bounds;
use crate::charset::{charset_table, is_unicode_charset};
use crate::model::{
    Codepoint, FontMetrics, GlyphMetrics, Properties, Property, Rational, Spacing, Vector,
};

pub(super) const ROWS: &str = "bdf.bitmap.rows";
pub(super) const IMAGE: &str = "bdf.bitmap.image";
pub(super) const ENCODING: &str = "bdf.encoding";
pub(super) const ALT: &str = "bdf.altcode";
pub(super) const SOURCE_HEADERS: &str = "bdf.source.headers";
pub(super) const SOURCE_PROPS: &str = "bdf.source.properties";
pub(super) const SOURCE_GL: &str = "bdf.source.gl";
pub(super) const METRICS: [&str; 5] = ["SWIDTH", "DWIDTH", "SWIDTH1", "DWIDTH1", "VVECTOR"];

pub(super) fn vector(metrics: &GlyphMetrics, key: &str) -> Option<Vector> {
    match key {
        "SWIDTH" => metrics.scalable_advance,
        "DWIDTH" => metrics.advance,
        "SWIDTH1" => metrics.vertical_scalable_advance,
        "DWIDTH1" => metrics.vertical_advance,
        "VVECTOR" => metrics.vertical_origin,
        _ => None,
    }
}
pub(super) fn set_vector(metrics: &mut GlyphMetrics, key: &str, v: Vector) {
    match key {
        "SWIDTH" => metrics.scalable_advance = Some(v),
        "DWIDTH" => metrics.advance = Some(v),
        "SWIDTH1" => metrics.vertical_scalable_advance = Some(v),
        "DWIDTH1" => metrics.vertical_advance = Some(v),
        "VVECTOR" => metrics.vertical_origin = Some(v),
        _ => {}
    }
}
pub(super) fn values(p: &Properties, key: &str) -> Option<Vec<i64>> {
    match p.get(key)? {
        PropertyValue::List(v) => v.iter().map(integer).collect(),
        v => integer(v).map(|n| vec![n]),
    }
}
pub(super) fn prop_int(p: &Properties, key: &str) -> Option<i64> {
    p.get_case_insensitive(key).and_then(integer)
}
pub(super) fn prop_text(p: &Properties, key: &str) -> Option<String> {
    p.get_case_insensitive(key).and_then(text_value)
}
pub(super) fn known_unicode(font: &Font) -> bool {
    let key = font.charset.canonical_name();
    is_unicode_charset(&key) || !charset_table().iter().any(|info| info.key == key)
}
pub(super) fn project(font: &mut Font) {
    let h = &font.headers;
    font.names.xlfd = h.get("FONT").and_then(text_value);
    font.names.xlfd_fields = font
        .names
        .xlfd
        .as_ref()
        .filter(|s| s.starts_with('-'))
        .map(|s| s[1..].split('-').map(str::to_owned).collect())
        .unwrap_or_default();
    if let Some(v) = values(h, "SIZE")
        && v.len() == 3
    {
        font.metrics.point_size = Some(Rational::integer(v[0]));
        font.metrics.resolution = Some(Vector::new(v[1], v[2]));
    }
    if let Some(v) = values(h, "FONTBOUNDINGBOX")
        && v.len() == 4
    {
        font.metrics.bounds = Some(Bounds::new(v[0] as u32, v[1] as u32, v[2], v[3]));
    }
    font.metrics.metrics_set = h
        .get("METRICSSET")
        .and_then(integer)
        .and_then(|v| v.try_into().ok());
    for key in METRICS {
        if let Some(v) = values(h, key)
            && v.len() == 2
        {
            set_vector(
                &mut font.metrics.glyph_defaults,
                key,
                Vector::new(v[0], v[1]),
            );
        }
    }
    let p = &font.properties;
    font.charset.registry = prop_text(p, "CHARSET_REGISTRY");
    font.charset.encoding = prop_text(p, "CHARSET_ENCODING");
    font.names.family = prop_text(p, "FAMILY_NAME");
    font.names.style = prop_text(p, "STYLE_NAME");
    font.names.full = prop_text(p, "FACE_NAME");
    font.names.postscript = prop_text(p, "POSTSCRIPT_NAME");
    font.names.version = prop_text(p, "FONT_VERSION");
    font.names.copyright = prop_text(p, "COPYRIGHT");
    font.names.license = prop_text(p, "LICENSE");
    font.names.license_url = prop_text(p, "LICENSE_URL");
    font.names.foundry = prop_text(p, "FOUNDRY");
    font.metrics.pixel_size = prop_int(p, "PIXEL_SIZE").and_then(|v| v.try_into().ok());
    font.metrics.ascent = prop_int(p, "FONT_ASCENT");
    font.metrics.descent = prop_int(p, "FONT_DESCENT");
    font.metrics.underline_position = prop_int(p, "UNDERLINE_POSITION");
    font.metrics.underline_thickness = prop_int(p, "UNDERLINE_THICKNESS");
    font.metrics.space_width = prop_int(p, "SPACE_WIDTH");
    font.metrics.line_gap = prop_int(p, "LINE_GAP");
    font.metrics.line_height = prop_int(p, "LINE_HEIGHT");
    font.metrics.spacing =
        prop_text(p, "SPACING").and_then(|s| match s.to_ascii_uppercase().as_str() {
            "P" => Some(Spacing::Proportional),
            "M" => Some(Spacing::Monospaced),
            "C" => Some(Spacing::CharacterCell),
            _ => None,
        });
    font.metrics.default_character = prop_int(p, "DEFAULT_CHAR")
        .and_then(|v| u32::try_from(v).ok())
        .map(|v| {
            if known_unicode(font) {
                Codepoint::Unicode(v)
            } else {
                Codepoint::Legacy(v)
            }
        });
}
fn snapshot(properties: &Properties) -> PropertyValue {
    PropertyValue::List(
        properties
            .iter()
            .filter(|p| p.raw.is_some() || !p.name.starts_with("bdf."))
            .map(|p| {
                PropertyValue::List(vec![PropertyValue::Text(p.name.clone()), p.value.clone()])
            })
            .collect(),
    )
}
fn restore(value: Option<&PropertyValue>) -> Properties {
    let mut out = Properties::default();
    if let Some(PropertyValue::List(records)) = value {
        for record in records {
            if let PropertyValue::List(pair) = record
                && let [PropertyValue::Text(name), value] = pair.as_slice()
            {
                out.push(Property::new(name, value.clone()));
            }
        }
    }
    out
}
pub(super) fn save_snapshot(font: &mut Font) {
    font.headers
        .push(Property::new(SOURCE_HEADERS, snapshot(&font.headers)));
    font.headers
        .push(Property::new(SOURCE_PROPS, snapshot(&font.properties)));
}
pub(super) fn original(font: &Font) -> Font {
    let mut base = Font::default();
    base.headers = restore(font.headers.get(SOURCE_HEADERS));
    base.properties = restore(font.headers.get(SOURCE_PROPS));
    project(&mut base);
    derive(&mut base);
    base
}
pub(super) fn derive(font: &mut Font) {
    // These are normalized views only; missing source fields are not silently added to BDF.
    if let Some(b) = font.metrics.bounds {
        font.metrics.ascent.get_or_insert(i64::from(b.height) + b.y);
        font.metrics.descent.get_or_insert(-b.y);
    }
    if font.metrics.pixel_size.is_none() {
        font.metrics.pixel_size = font
            .names
            .xlfd_fields
            .get(6)
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|&v| v > 0)
            .or_else(|| {
                let size = font.metrics.point_size?;
                let dpi = font.metrics.resolution?.y;
                let n = i128::from(size.numerator()) * i128::from(dpi);
                let d = i128::from(size.denominator()) * 72;
                u32::try_from((n * 2 + d).div_euclid(d * 2))
                    .ok()
                    .filter(|&v| v > 0)
            })
            .or(font.metrics.bounds.map(|b| b.height));
    }
}

/// Original stripped hex rows, if source provenance exists. `None` means the
/// glyph was newly authored or provenance was removed; rows are not assumed valid.
pub fn source_rows(glyph: &Glyph) -> Option<impl Iterator<Item = &str>> {
    let PropertyValue::Text(s) = glyph.properties.get(ROWS)? else {
        return None;
    };
    // Each stored row has a delimiter, so zero rows and one empty row stay distinct.
    Some(s.split_terminator('\n'))
}
pub(super) fn unchanged_bitmap(glyph: &Glyph) -> bool {
    let Some(PropertyValue::Bytes(bytes)) = glyph.properties.get(IMAGE) else {
        return false;
    };
    let GlyphBitmap::Mono(b) = &glyph.bitmap else {
        return false;
    };
    bytes.get(..4) == Some(b.width().to_le_bytes().as_slice())
        && bytes.get(4..8) == Some(b.height().to_le_bytes().as_slice())
        && bytes.get(8..) == Some(b.as_bytes())
}
pub(super) fn save_bitmap(glyph: &mut Glyph, rows: String) {
    let mut image = Vec::with_capacity(8 + glyph.bitmap.as_bytes().len());
    image.extend_from_slice(&glyph.bitmap.width().to_le_bytes());
    image.extend_from_slice(&glyph.bitmap.height().to_le_bytes());
    image.extend_from_slice(glyph.bitmap.as_bytes());
    glyph
        .properties
        .push(Property::new(ROWS, PropertyValue::Text(rows)));
    glyph
        .properties
        .push(Property::new(IMAGE, PropertyValue::Bytes(image)));
}

/// Reject invalid source hex with SPEC's exact glyph/row error before drawing.
/// Edited rasters supersede the source rows. Missing/short rows are valid and
/// already zero-padded in the core model; source bytes remain available to writers.
pub fn validate_bitmap(glyph: &Glyph) -> Result<()> {
    if !unchanged_bitmap(glyph) {
        return Ok(());
    }
    if let Some(rows) = source_rows(glyph) {
        for (i, row) in rows.enumerate() {
            if !row.bytes().all(|b| b.is_ascii_hexdigit()) {
                let name = glyph.name.as_deref().unwrap_or("None");
                let code = glyph
                    .codepoints
                    .first()
                    .map_or(-1, |cp| i64::from(cp.value()));
                return Err(Error::InvalidInput(format!(
                    "Glyph \"{name}\" (codepoint {code}): bitmap row {i} {} contains a non-hex character",
                    json_quote(row)
                )));
            }
        }
    }
    Ok(())
}
fn json_quote(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn model_defaults_changed(a: &FontMetrics, b: &FontMetrics) -> bool {
    a.ppem_x != b.ppem_x || a.ppem_y != b.ppem_y || a.default_glyph != b.default_glyph
}
