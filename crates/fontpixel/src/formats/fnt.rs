//! Windows raster FNT v1/v2/v3, with explicit legacy charset semantics.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::FontSet;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Font, Report, ResourceLimits, Result};
pub(crate) mod reader;
pub(crate) mod support;
mod writer;
pub use reader::read;
use support::*;
pub use writer::write;

/// Windows raster font version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Version {
    /// Windows 1.x shared row-major strike.
    One,
    /// Windows 2.x, 16-bit offsets and a 64 KiB file limit.
    #[default]
    Two,
    /// Windows 3.x, 32-bit offsets.
    Three,
}
impl Version {
    pub(crate) fn word(self) -> u16 {
        match self {
            Self::One => 0x100,
            Self::Two => 0x200,
            Self::Three => 0x300,
        }
    }
    pub(crate) fn header(self) -> usize {
        match self {
            Self::One => 117,
            Self::Two => 118,
            Self::Three => 148,
        }
    }
    pub(crate) fn from_word(v: u16) -> Result<Self> {
        match v {
            0x100 => Ok(Self::One),
            0x200 => Ok(Self::Two),
            0x300 => Ok(Self::Three),
            _ => Err(invalid("unsupported FNT version")),
        }
    }
}
/// FNT/FON read interpretation; bytes always remain legacy glyph codes.
#[derive(Debug, Clone, Default)]
pub struct FntReadOptions {
    /// Explicit encoding for OEM, symbol, unknown or historical ANSI charsets.
    /// It labels legacy codes and does not change their numbers.
    pub code_page: Option<String>,
}
/// Bounded deterministic FNT output.
#[derive(Debug, Clone)]
pub struct FntWriteOptions {
    /// None preserves a source FNT version, otherwise defaults to version 2.
    pub version: Option<Version>,
    /// Windows dfCharSet identifier. None preserves source or infers a known page.
    pub charset: Option<u8>,
    /// Explicit code page for Unicode mapping, including OEM/symbol charsets.
    pub code_page: Option<String>,
    /// Required for Gray8 input. Conversion is reported as a loss.
    pub gray_threshold: Option<u8>,
    /// Limits on source/model/output work.
    pub limits: ResourceLimits,
    /// Maximum output bytes, independently of aggregate working memory.
    pub max_output_bytes: usize,
}
impl Default for FntWriteOptions {
    fn default() -> Self {
        Self {
            version: None,
            charset: None,
            code_page: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Configurable registry implementation for Windows raster fonts.
#[derive(Debug, Clone, Default)]
pub struct FntCodec {
    /// Legacy encoding interpretation.
    pub read_options: FntReadOptions,
    /// Output version, code page and limits.
    pub write_options: FntWriteOptions,
}
impl FormatDetector for FntCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for FntCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
        let (font, mut report) =
            reader::parse(c.bytes(), &self.read_options, c.limits(), &mut budget)?;
        for e in &mut report.entries {
            e.location.asset = c.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("fnt".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for FntCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        let font = set.single()?;
        let mut options = self.write_options.clone();
        let container_cost = properties_cost(&set.properties, &options.limits)?;
        options.limits.max_total_bytes -= container_cost;
        let (bytes, mut report) = write(font, &options)?;
        for p in set.properties.iter() {
            loss(
                &mut report,
                "container_property",
                None,
                None,
                format!("Container property {:?} has no FNT field", p.name),
            );
        }
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
/// Structural recognition, avoiding the ambiguous `.fnt` extension.
pub fn detect(bytes: &[u8]) -> Confidence {
    if bytes.len() < 117 {
        return Confidence::NoMatch;
    }
    let Ok(v) = get16(bytes, 0).and_then(Version::from_word) else {
        return Confidence::NoMatch;
    };
    let Ok(n) = get32(bytes, 2) else {
        return Confidence::NoMatch;
    };
    if n as usize >= v.header()
        && n as usize <= bytes.len()
        && bytes[95] <= bytes[96]
        && get16(bytes, 88).is_ok_and(|h| h > 0)
    {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Windows charset identifiers with exact supported code-page names.
/// OEM, symbol, DEFAULT and unknown identifiers deliberately return None.
pub fn charset_name(id: u8) -> Option<&'static str> {
    match id {
        0 => Some("cp1252"),
        77 => Some("macroman"),
        128 => Some("cp932"),
        129 => Some("cp949"),
        130 => Some("johab"),
        134 => Some("cp936"),
        136 => Some("cp950"),
        161 => Some("cp1253"),
        162 => Some("cp1254"),
        163 => Some("cp1258"),
        177 => Some("cp1255"),
        178 => Some("cp1256"),
        186 => Some("cp1257"),
        204 => Some("cp1251"),
        222 => Some("cp874"),
        238 => Some("cp1250"),
        _ => None,
    }
}
/// Decode with default charset interpretation and explicit resource limits.
pub fn read_default(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    read(bytes, &FntReadOptions::default(), limits)
}
