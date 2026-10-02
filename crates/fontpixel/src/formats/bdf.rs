//! Adobe BDF 2.1/2.2 with SPEC-v3/Python canonical output and source provenance.
//!
//! ```
//! use fontpixel::{read, write, ReadOptions, WriteOptions, ResourcePath};
//! let input = b"STARTFONT 2.1\nFONT demo\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 8 0 0\nCHARS 0\nENDFONT\n";
//! let loaded = read(input, &ReadOptions::default())?;
//! let options = WriteOptions::new(ResourcePath::new("demo.bdf")?);
//! let result = write(&loaded.fonts, "bdf", &options)?;
//! assert!(result.output.into_single()?.starts_with(b"STARTFONT 2.1\n"));
//! # Ok::<(), fontpixel::Error>(())
//! ```
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Mono};
use crate::model::{Font, FontSet, Glyph, GlyphBitmap, PropertyValue};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};

mod nonprintable;
mod parser;
mod source;
mod writer;
pub use crate::formats::x11::{source_rows, validate_bitmap};

/// BDF-specific output choices. Mono is lossless; Gray8 conversion is explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdfWriteOptions {
    /// Threshold for Gray8 conversion (ink >= threshold); None refuses Gray8.
    pub gray_threshold: Option<u8>,
    /// Maximum UTF-8 output bytes, checked while serializing.
    pub max_output_bytes: usize,
}
impl Default for BdfWriteOptions {
    fn default() -> Self {
        Self {
            gray_threshold: None,
            max_output_bytes: 256 * 1024 * 1024,
        }
    }
}

/// Actual BDF detector/reader/writer used by the built-in registry.
#[derive(Debug, Clone, Default)]
pub struct BdfCodec {
    /// Options for registry-driven writes; direct callers may use `write`.
    pub write_options: BdfWriteOptions,
}
impl FormatDetector for BdfCodec {
    fn detect(&self, input: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(input.bytes()))
    }
}
impl FormatReader<FontSet> for BdfCodec {
    fn read(
        &self,
        input: &mut ReadContext<'_>,
        _options: &ReadOptions,
    ) -> Result<(FontSet, Report)> {
        let (font, mut report) = parser::read(input.bytes(), input.limits())?;
        for entry in &mut report.entries {
            entry.location.asset = input.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("bdf".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for BdfCodec {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        let (bytes, mut report) = write(fonts.single()?, &self.write_options)?;
        for property in fonts.properties.iter() {
            diagnostic(
                &mut report,
                Level::Warning,
                Impact::Loss,
                "bdf.write.container_property",
                None,
                format!("BDF cannot store container property {:?}", property.name),
            );
        }
        Ok(Written {
            output: OutputBundle::single(options.primary_name.clone(), bytes),
            report,
        })
    }
}

/// Recognize a STARTFONT token after an optional UTF-8 BOM/ASCII whitespace.
/// Invalid version syntax is still detected as BDF, so the reader can diagnose it.
pub fn detect(bytes: &[u8]) -> Confidence {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let start = bytes
        .iter()
        .position(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c))
        .unwrap_or(bytes.len());
    let bytes = &bytes[start..];
    if bytes.starts_with(b"STARTFONT")
        && bytes
            .get(9)
            .is_none_or(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c))
    {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}

/// Parse BDF bytes into a bounded, source-preserving strike and ordered report.
/// UTF-8 errors use replacement characters as in Python `load_bytes`.
pub fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    if bytes.len() > limits.max_input_bytes || bytes.len() > limits.max_total_bytes {
        return Err(Error::ResourceLimit {
            resource: "BDF input bytes",
            limit: limits.max_input_bytes.min(limits.max_total_bytes),
        });
    }
    parser::read(bytes, limits)
}

/// Serialize one strike to deterministic SPEC-v3 UTF-8 BDF bytes.
/// Direct calls return losses; use registry strict mode to reject them.
pub fn write(font: &Font, options: &BdfWriteOptions) -> Result<(Vec<u8>, Report)> {
    writer::write(font, options)
}

fn diagnostic(
    report: &mut Report,
    level: Level,
    impact: Impact,
    code: &str,
    glyph: Option<(usize, Option<u32>)>,
    message: String,
) {
    report.push(ReportEntry {
        level,
        impact,
        code: code.into(),
        location: Location {
            font_index: Some(0),
            glyph_index: glyph.map(|g| g.0),
            codepoint: glyph.and_then(|g| g.1),
            asset: None,
        },
        message,
    });
}
fn ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{b}' | '\u{c}')
}
fn kv(line: &str) -> (&str, &str) {
    line.find(ws).map_or((line, ""), |i| {
        (&line[..i], line[i..].trim_start_matches(ws))
    })
}
fn int_syntax(value: &str) -> bool {
    let s = value.strip_prefix(['+', '-']).unwrap_or(value);
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}
fn integer(v: &PropertyValue) -> Option<i64> {
    match v {
        PropertyValue::Integer(v) => Some(*v),
        PropertyValue::Unsigned(v) => (*v).try_into().ok(),
        _ => None,
    }
}
fn text_value(v: &PropertyValue) -> Option<String> {
    match v {
        PropertyValue::Text(v) | PropertyValue::Decimal(v) => Some(v.clone()),
        PropertyValue::Integer(v) => Some(v.to_string()),
        PropertyValue::Unsigned(v) => Some(v.to_string()),
        _ => None,
    }
}
fn py_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if nonprintable(c) => {
                use std::fmt::Write;
                if (c as u32) < 256 {
                    let _ = write!(out, "\\x{:02x}", c as u32);
                } else if (c as u32) < 65536 {
                    let _ = write!(out, "\\u{:04x}", c as u32);
                } else {
                    let _ = write!(out, "\\U{:08x}", c as u32);
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
fn bitmap_limits(limits: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: i32::MAX as u32,
        max_pixels: limits.max_pixels_per_glyph,
        max_bytes: limits.max_table_bytes.min(limits.max_total_bytes),
    }
}
fn mono(glyph: &Glyph, threshold: Option<u8>) -> Result<Bitmap<Mono>> {
    match &glyph.bitmap {
        GlyphBitmap::Mono(b) => Ok(b.clone()),
        GlyphBitmap::Gray8(b) => b.to_mono(threshold.ok_or_else(|| {
            Error::InvalidInput("BDF cannot write Gray8 without an explicit gray_threshold".into())
        })?),
    }
}

fn nonprintable(c: char) -> bool {
    let n = c as u32;
    let ranges = nonprintable::NONPRINTABLE;
    let i = ranges.partition_point(|&(start, _)| start <= n);
    i > 0 && n <= ranges[i - 1].1
}
