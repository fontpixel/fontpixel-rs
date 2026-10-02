//! Bounded YAFF 1.0 text interchange with lossless native-metadata extensions.
//!
//! Ordinary labels, pixels and horizontal metrics interoperate with monobit.
//! Namespaced JSON metadata preserves native fields not representable in YAFF.
//! See `docs/formats/yaff.md` for reconciliation, limits and crosscheck details.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Gray8, Mono};
use crate::model::*;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
use serde::{Deserialize, Serialize};
use std::io::Write as _;

mod parser;
mod writer;

/// Resource bounds for deterministic YAFF serialization.
#[derive(Debug, Clone)]
pub struct YaffWriteOptions {
    /// Maximum encoded UTF-8 bytes, enforced as bytes are written.
    pub max_output_bytes: usize,
    /// Input-model and metadata allocation limits.
    pub limits: ResourceLimits,
}
impl Default for YaffWriteOptions {
    fn default() -> Self {
        Self {
            max_output_bytes: 256 * 1024 * 1024,
            limits: ResourceLimits::default(),
        }
    }
}
/// Registered YAFF detector, reader and writer.
#[derive(Debug, Clone, Default)]
pub struct YaffCodec {
    /// Bounded write configuration.
    pub write_options: YaffWriteOptions,
}
impl FormatDetector for YaffCodec {
    fn detect(&self, input: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(input.bytes()))
    }
}
impl FormatReader<FontSet> for YaffCodec {
    fn read(&self, input: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        parser::read(input.bytes(), input.limits())
    }
}
impl FormatWriter<FontSet> for YaffCodec {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(fonts, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(options.primary_name.clone(), bytes),
            report,
        })
    }
}
/// Recognize signature or plausible label-plus-raster content without filename hints.
pub fn detect(bytes: &[u8]) -> Confidence {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Confidence::NoMatch;
    };
    let mut label = false;
    for line in text
        .trim_start_matches('\u{feff}')
        .split(['\n', '\r'])
        .take(128)
    {
        if let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("yaff")
            && !value.trim().is_empty()
        {
            return Confidence::Certain;
        }
        if line.starts_with([' ', '\t'])
            && label
            && line.trim().bytes().all(|b| matches!(b, b'@' | b'.' | b'-'))
            && !line.trim().is_empty()
        {
            return Confidence::Possible;
        }
        if !line.starts_with([' ', '\t', '#']) && line.trim_end().ends_with(':') {
            label = true;
        }
    }
    Confidence::NoMatch
}
/// Decode a standalone YAFF file. Use the registry for gzip transport.
pub fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(FontSet, Report)> {
    if bytes.len() > limits.max_input_bytes {
        return Err(limit("YAFF input bytes", limits.max_input_bytes));
    }
    parser::read(bytes, limits)
}
/// Serialize a complete collection with exact native metadata and visible pixels.
pub fn write(fonts: &FontSet, options: &YaffWriteOptions) -> Result<(Vec<u8>, Report)> {
    writer::write(fonts, options)
}
fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(format!("YAFF: {}", message.into()))
}
fn limit(resource: &'static str, maximum: usize) -> Error {
    Error::ResourceLimit {
        resource,
        limit: maximum,
    }
}
fn key(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace('_', "-")
}
fn valid_text(s: &str) -> bool {
    !s.chars().any(|c| {
        (c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            || (0xfdd0..=0xfdef).contains(&(c as u32))
            || c as u32 & 0xffff >= 0xfffe
    })
}
fn int(s: &str) -> Result<i64> {
    s.trim()
        .parse()
        .map_err(|_| invalid("invalid signed integer"))
}
fn unsigned(s: &str) -> Result<u32> {
    let s = s.trim();
    let value = if s.starts_with("0x") || s.starts_with("0X") {
        u32::from_str_radix(&s[2..], 16)
    } else if s.starts_with("0o") || s.starts_with("0O") {
        u32::from_str_radix(&s[2..], 8)
    } else {
        s.parse()
    };
    value.map_err(|_| invalid("invalid unsigned integer"))
}
fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or_else(|| invalid("metric overflow"))
}
fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or_else(|| invalid("metric overflow"))
}
fn rational(s: &str) -> Result<Rational> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once('.') {
        if b.is_empty() || !b.bytes().all(|c| c.is_ascii_digit()) {
            return Err(invalid("invalid point-size"));
        }
        let den = 10_u32
            .checked_pow(u32::try_from(b.len()).map_err(|_| invalid("point-size overflow"))?)
            .ok_or_else(|| invalid("point-size precision exceeds exact range"))?;
        let integer = if a == "-" || a.is_empty() { 0 } else { int(a)? };
        let fraction = int(b)? * if s.starts_with('-') { -1 } else { 1 };
        Rational::new(
            add(
                integer
                    .checked_mul(i64::from(den))
                    .ok_or_else(|| invalid("point-size overflow"))?,
                fraction,
            )?,
            den,
        )
    } else {
        Ok(Rational::integer(int(s)?))
    }
}
fn point_text(r: Rational) -> String {
    // YAFF portable consumers accept decimal floats; the extension retains exact fractions.
    if r.denominator() == 1 {
        r.numerator().to_string()
    } else {
        format!("{:.6}", r.numerator() as f64 / f64::from(r.denominator()))
    }
}
fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .rev()
        .find(|(k, _)| key(k) == name)
        .map(|(_, v)| v.as_str())
}
fn props_depth(props: &Properties) -> Result<()> {
    fn value(v: &PropertyValue, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(invalid("property nesting exceeds 32"));
        }
        if let PropertyValue::List(items) = v {
            for item in items {
                value(item, depth + 1)?;
            }
        }
        Ok(())
    }
    for prop in props.iter() {
        value(&prop.value, 0)?;
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FontMeta {
    version: u8,
    metrics: FontMetrics,
    names: FontNames,
    charset: FontCharset,
    source_format: Option<String>,
    headers: Properties,
    properties: Properties,
    slots: usize,
}
impl FontMeta {
    fn restore(self) -> Result<Font> {
        if self.version != 1 {
            return Err(invalid("unsupported font metadata version"));
        }
        if self
            .metrics
            .point_size
            .is_some_and(|v| v.denominator() == 0)
        {
            return Err(invalid("zero rational denominator"));
        }
        props_depth(&self.headers)?;
        props_depth(&self.properties)?;
        props_depth(&self.names.localized)?;
        let mut font = Font::default();
        font.metrics = self.metrics;
        font.names = self.names;
        font.charset = self.charset;
        font.source_format = self.source_format;
        font.headers = self.headers;
        font.properties = self.properties;
        Ok(font)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlyphMeta {
    version: u8,
    id: usize,
    codepoints: Vec<Codepoint>,
    unicode_sequences: Vec<Vec<u32>>,
    name: Option<String>,
    metrics: GlyphMetrics,
    gray: bool,
    properties: Properties,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContainerMeta {
    version: u8,
    properties: Properties,
    source_format: Option<String>,
    empty: bool,
}

#[derive(Serialize)]
struct FontMetaRef<'a> {
    version: u8,
    metrics: &'a FontMetrics,
    names: &'a FontNames,
    charset: &'a FontCharset,
    source_format: &'a Option<String>,
    headers: &'a Properties,
    properties: &'a Properties,
    slots: usize,
}
#[derive(Serialize)]
struct GlyphMetaRef<'a> {
    version: u8,
    id: usize,
    codepoints: &'a [Codepoint],
    unicode_sequences: &'a [Vec<u32>],
    name: &'a Option<String>,
    metrics: &'a GlyphMetrics,
    gray: bool,
    properties: &'a Properties,
}
#[derive(Serialize)]
struct ContainerMetaRef<'a> {
    version: u8,
    properties: &'a Properties,
    source_format: &'a Option<String>,
    empty: bool,
}

fn font_fields(f: &Font) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v
            && valid_text(&v)
            && !v.contains('\r')
        {
            fields.push((k.into(), v));
        }
    };
    for (k, v) in [
        ("family", &f.names.family),
        ("subfamily", &f.names.style),
        ("name", &f.names.full),
        ("revision", &f.names.version),
        ("copyright", &f.names.copyright),
        ("notice", &f.names.license),
        ("foundry", &f.names.foundry),
    ] {
        put(k, v.clone());
    }
    for (k, v) in [
        ("pixel-size", f.metrics.pixel_size.map(i64::from)),
        ("ascent", f.metrics.ascent),
        ("descent", f.metrics.descent),
        ("leading", f.metrics.line_gap),
        ("line-height", f.metrics.line_height),
        ("underline-thickness", f.metrics.underline_thickness),
        ("word-space", f.metrics.space_width),
    ] {
        put(k, v.map(|v| v.to_string()));
    }
    put(
        "underline-descent",
        f.metrics
            .underline_position
            .and_then(i64::checked_neg)
            .map(|v| v.to_string()),
    );
    put("point-size", f.metrics.point_size.map(point_text));
    put(
        "dpi",
        f.metrics.resolution.map(|v| format!("{} {}", v.x, v.y)),
    );
    put(
        "encoding",
        f.charset.name.clone().or_else(|| {
            f.charset
                .registry
                .as_ref()
                .map(|_| f.charset.canonical_name())
        }),
    );
    put(
        "spacing",
        f.metrics.spacing.map(|s| {
            match s {
                Spacing::Proportional => "proportional",
                Spacing::Monospaced => "monospace",
                Spacing::CharacterCell => "character-cell",
            }
            .into()
        }),
    );
    put(
        "default-char",
        f.metrics
            .default_character
            .filter(|cp| !matches!(cp, Codepoint::Unicode(v) if char::from_u32(*v).is_none()))
            .map(cp_label),
    );
    fields
}
fn cp_label(cp: Codepoint) -> String {
    match cp {
        Codepoint::Unicode(v) => format!("u+{v:04X}"),
        Codepoint::Legacy(v) => format!("0x{v:X}"),
    }
}
fn labels(g: &Glyph) -> Vec<String> {
    let mut labels: Vec<String> = g
        .codepoints
        .iter()
        .copied()
        .filter(|cp| !matches!(cp, Codepoint::Unicode(v) if char::from_u32(*v).is_none()))
        .map(cp_label)
        .collect();
    for sequence in &g.unicode_sequences {
        if sequence.iter().all(|&c| char::from_u32(c).is_some()) {
            labels.push(
                sequence
                    .iter()
                    .map(|&c| cp_label(Codepoint::Unicode(c)))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
    if let Some(name) = &g.name
        && valid_text(name)
        && !name.contains(['\n', '\r'])
    {
        labels.push(format!("\"{name}\""));
    }
    if labels.is_empty() {
        labels.push(String::new());
    }
    labels
}
fn glyph_fields(g: &Glyph, defaults: &GlyphMetrics) -> Result<Vec<(String, String)>> {
    let b = g.metrics.bounds;
    let advance = g
        .metrics
        .advance
        .or(defaults.advance)
        .unwrap_or(Vector::new(add(b.x, i64::from(b.width))?, 0));
    Ok(vec![
        ("left-bearing".into(), b.x.to_string()),
        ("shift-up".into(), b.y.to_string()),
        (
            "right-bearing".into(),
            sub(sub(advance.x, b.x)?, i64::from(b.width))?.to_string(),
        ),
    ])
}
struct Budget {
    used: usize,
    max: usize,
    record_max: usize,
}
impl Budget {
    fn charge(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| limit("YAFF allocation budget", self.max))?;
        if self.used > self.max {
            return Err(limit("YAFF allocation budget", self.max));
        }
        Ok(())
    }
}
