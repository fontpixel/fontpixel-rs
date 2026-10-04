//! Bits'N'Picas XML fonts with exact grayscale and bounded WIB decoding.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::BitmapLimits;
use crate::model::{Font, FontSet};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
mod reader;
mod wire;
mod writer;

/// Explicit conversion of native grayscale, if requested by a caller.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KbitxReadOptions {
    /// Samples at least this value become ink; `None` retains exact grayscale.
    pub gray_threshold: Option<u8>,
}
/// Bounded deterministic native XML output.
#[derive(Debug, Clone)]
pub struct KbitxWriteOptions {
    /// Model, bitmap and cumulative allocation limits.
    pub limits: ResourceLimits,
    /// Maximum generated XML size.
    pub max_output_bytes: usize,
}
impl Default for KbitxWriteOptions {
    fn default() -> Self {
        Self {
            limits: ResourceLimits::default(),
            max_output_bytes: 256 * 1024 * 1024,
        }
    }
}
/// Native KBITX registry reader, detector and writer.
#[derive(Debug, Clone, Default)]
pub struct KbitxCodec {
    /// Independent input choices.
    pub read_options: KbitxReadOptions,
    /// Independent output choices.
    pub write_options: KbitxWriteOptions,
}
impl FormatDetector for KbitxCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for KbitxCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (font, mut report) = reader::read(
            c.bytes(),
            &self.read_options,
            c.limits(),
            c.retained_input_bytes(),
        )?;
        for entry in &mut report.entries {
            entry.location.asset = c.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("kbitx".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for KbitxCodec {
    fn write(&self, set: &FontSet, options: &WriteOptions) -> Result<Written> {
        let (bytes, mut report) = write(set.single()?, &self.write_options)?;
        if !set.properties.is_empty() {
            diag(
                &mut report,
                "write.container_properties",
                Impact::Loss,
                None,
                "Container properties have no KBITX representation",
            );
        }
        Ok(Written {
            output: OutputBundle::single(options.primary_name.clone(), bytes),
            report,
        })
    }
}
/// Bounded lexical detection; XML validity is established by the reader.
pub fn detect(bytes: &[u8]) -> Confidence {
    let prefix = &bytes[..bytes.len().min(8192)];
    let s = match std::str::from_utf8(prefix) {
        Ok(s) => s,
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&prefix[..error.valid_up_to()]).unwrap_or("")
        }
        Err(_) => return Confidence::NoMatch,
    };
    for token in xmlparser::Tokenizer::from(s.trim_start_matches('\u{feff}')) {
        match token {
            Ok(xmlparser::Token::ElementStart { prefix, local, .. }) => {
                return if prefix.is_empty() && local.as_str() == "kbits" {
                    Confidence::Certain
                } else {
                    Confidence::NoMatch
                };
            }
            Err(_) => return Confidence::NoMatch,
            _ => {}
        }
    }
    Confidence::NoMatch
}
/// Read one bounded UTF-8 XML font, retaining ordered native metadata.
pub fn read(
    bytes: &[u8],
    options: &KbitxReadOptions,
    limits: &ResourceLimits,
) -> Result<(Font, Report)> {
    reader::read(bytes, options, limits, bytes.len())
}
/// Encode one font as standard KBITX; unsupported model fields have loss reports.
pub fn write(font: &Font, options: &KbitxWriteOptions) -> Result<(Vec<u8>, Report)> {
    writer::write(font, options)
}
fn invalid(message: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("KBITX: {message}"))
}
fn bound(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
fn checked_i32(n: i64) -> Result<i64> {
    i32::try_from(n)
        .map(i64::from)
        .map_err(|_| invalid("integer exceeds signed 32-bit native range"))
}
struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn new(used: usize, max: usize) -> Result<Self> {
        bound(used, max, "KBITX aggregate bytes")?;
        Ok(Self { used, max })
    }
    fn temporary(&self, n: usize) -> Result<()> {
        bound(
            self.used.saturating_add(n),
            self.max,
            "KBITX aggregate bytes",
        )
    }
    fn add(&mut self, n: usize) -> Result<()> {
        self.temporary(n)?;
        self.used += n;
        Ok(())
    }
}
fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
        max_dimension: i32::MAX as u32,
    }
}
fn diag(
    report: &mut Report,
    code: &str,
    impact: Impact,
    glyph: Option<usize>,
    message: impl Into<String>,
) {
    report.push(ReportEntry {
        level: Level::Warning,
        impact,
        code: format!("kbitx.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: glyph,
            ..Default::default()
        },
        message: message.into(),
    });
}
