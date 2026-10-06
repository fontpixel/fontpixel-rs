//! `formats` and `info`: registry listing and font inspection.
use crate::context;
use crate::convert::{conversion_charge, conversion_report_bytes};
use crate::options::RasterOptions;
use crate::options::check_format;
use fontpixel::formats::FormatStatus;
use fontpixel::{FontSet, ReadOptions, Report};
use std::{io::Write, path::PathBuf};

/// List every catalogue format with its compiled implementation status.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct FormatsRequest {
    /// Emit machine-readable metadata and capability flags.
    #[cfg_attr(feature = "clap", arg(long))]
    pub json: bool,
}
impl FormatsRequest {
    /// Check the value constraints the command-line parser enforces; this
    /// group has none beyond its field types.
    pub fn validate(&self) -> fontpixel::Result<()> {
        Ok(())
    }
}
/// Every catalogue format in registry order.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatsResponse {
    /// Catalogue metadata and the detector/reader/writer actually compiled in.
    pub formats: Vec<FormatStatus>,
}
impl FormatsResponse {
    /// Write the command-line JSON document.
    pub fn write_json(&self, out: &mut dyn Write, pretty: bool) -> fontpixel::Result<()> {
        crate::json::write(out, &serde_json::json!({ "formats": self.formats }), pretty)
    }
    /// Write the command-line tab-separated table.
    pub fn write_text(&self, out: &mut dyn Write) -> fontpixel::Result<()> {
        writeln!(out, "ID\tREAD\tWRITE\tFEATURE\tEXTENSIONS\tNAME")?;
        for status in &self.formats {
            writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}",
                status.format.id,
                if status.read {
                    "implemented"
                } else {
                    "not implemented"
                },
                if status.write {
                    "implemented"
                } else {
                    "not implemented"
                },
                if status.format.feature_enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                status
                    .format
                    .extensions
                    .iter()
                    .map(|ext| format!(".{ext}"))
                    .collect::<Vec<_>>()
                    .join(","),
                status.format.name
            )?;
        }
        Ok(())
    }
}
pub(crate) fn formats(_request: &FormatsRequest) -> FormatsResponse {
    FormatsResponse {
        formats: fontpixel::registry().formats().collect(),
    }
}

/// Inspect a font, including its metrics, properties and diagnostics.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct InfoRequest {
    /// Primary input asset.
    pub input: PathBuf,
    /// Explicit input format, otherwise use structural detection then hints.
    #[cfg_attr(feature = "clap", arg(long, value_parser = crate::options::format_parser()))]
    pub from: Option<String>,
    /// Emit machine-readable font information and diagnostics.
    #[cfg_attr(feature = "clap", arg(long))]
    pub json: bool,
    /// Input decoding options and resource limits.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub raster: RasterOptions,
}
impl InfoRequest {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.from.as_deref() {
            check_format("from", value)?;
        }
        self.raster.validate()?;
        Ok(())
    }
}
impl InfoRequest {
    /// The original total byte limit, before the standalone identity partition.
    pub fn source_identity_total_bytes(&self) -> usize {
        self.raster.original_total_bytes()
    }
}
/// The decoded fonts and read diagnostics of one input.
#[derive(Debug, Clone)]
pub struct InfoResponse {
    /// Detected or explicit format identifier.
    pub format: &'static str,
    /// Every decoded strike.
    pub fonts: FontSet,
    /// Read diagnostics.
    pub report: Report,
}
impl InfoResponse {
    /// Write the command-line JSON document.
    pub fn write_json(&self, out: &mut dyn Write, pretty: bool) -> fontpixel::Result<()> {
        crate::json::write(
            out,
            &InfoSummary {
                font_count: self.fonts.fonts.len(),
                fonts: FontInfos(&self.fonts.fonts),
                format: self.format,
                report: &self.report,
            },
            pretty,
        )
    }
    /// Write the command-line text report (one JSON object per font).
    pub fn write_text(&self, out: &mut dyn Write, pretty: bool) -> fontpixel::Result<()> {
        writeln!(out, "Format: {}", self.format)?;
        writeln!(out, "Fonts: {}", self.fonts.fonts.len())?;
        for font in &self.fonts.fonts {
            crate::json::write(out, &FontInfo(font), pretty)?;
        }
        write!(out, "{}", self.report)?;
        Ok(())
    }
    /// The JSON summary of one strike, as listed by [`InfoResponse::write_json`].
    pub fn font_json(&self, index: usize, pretty: bool) -> fontpixel::Result<String> {
        let font = self
            .fonts
            .fonts
            .get(index)
            .ok_or_else(|| fontpixel::Error::InvalidInput("font index out of range".into()))?;
        crate::json::to_string(&FontInfo(font), pretty)
    }
}
pub(crate) fn info(request: InfoRequest) -> fontpixel::Result<InfoResponse> {
    let InfoRequest {
        input,
        from,
        json: _,
        raster,
    } = request;
    let mut registry = fontpixel::registry();
    raster.apply(&mut registry)?;
    let options = ReadOptions {
        format: from,
        limits: raster.input_limits(),
    };
    let file = context::open_input(&input, &options.limits, context::sources())?;
    let loaded = registry.read(&file.input(), &options)?;
    drop(file);
    let mut retained = loaded
        .fonts
        .retained_usage(options.limits.max_total_bytes)?
        .bytes;
    conversion_charge(
        &mut retained,
        conversion_report_bytes(&loaded.report, options.limits.max_total_bytes)?,
        options.limits.max_total_bytes,
    )?;
    let name_scratch = loaded
        .fonts
        .fonts
        .iter()
        .map(|f| {
            f.charset
                .registry
                .as_ref()
                .map_or(0, String::len)
                .saturating_add(f.charset.encoding.as_ref().map_or(0, String::len))
                .saturating_add(f.charset.name.as_ref().map_or(0, String::len))
        })
        .max()
        .unwrap_or(0);
    conversion_charge(
        &mut retained,
        name_scratch.saturating_mul(4),
        options.limits.max_total_bytes,
    )?;
    conversion_charge(
        &mut retained,
        fontpixel::charset::merge_categories::UNICODE_BLOCKS
            .len()
            .saturating_mul(std::mem::size_of::<usize>()),
        options.limits.max_total_bytes,
    )?;
    Ok(InfoResponse {
        format: loaded.format,
        fonts: loaded.fonts,
        report: loaded.report,
    })
}

// Borrow the model and report rather than cloning arbitrarily large JSON trees.
struct PropertyValueRef<'a>(&'a fontpixel::model::PropertyValue);
impl serde::Serialize for PropertyValueRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use fontpixel::model::PropertyValue::*;
        use serde::ser::{SerializeSeq, SerializeStruct};
        match self.0 {
            Integer(v) => serializer.serialize_i64(*v),
            Unsigned(v) => serializer.serialize_u64(*v),
            Decimal(v) | Text(v) => serializer.serialize_str(v),
            Flag => serializer.serialize_none(),
            Bytes(v) => {
                let mut map = serializer.serialize_struct("Bytes", 1)?;
                map.serialize_field("bytes", v)?;
                map.end()
            }
            List(v) => {
                let mut seq = serializer.serialize_seq(Some(v.len()))?;
                for value in v {
                    seq.serialize_element(&PropertyValueRef(value))?;
                }
                seq.end()
            }
        }
    }
}
struct PropertiesRef<'a>(&'a fontpixel::model::Properties);
impl serde::Serialize for PropertiesRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        #[derive(serde::Serialize)]
        struct PropertyRef<'a> {
            name: &'a str,
            value: PropertyValueRef<'a>,
        }
        let mut seq = serializer.serialize_seq(None)?;
        for property in self.0.iter().filter(|p| !p.name.starts_with("bdf.")) {
            seq.serialize_element(&PropertyRef {
                name: &property.name,
                value: PropertyValueRef(&property.value),
            })?;
        }
        seq.end()
    }
}
struct UnicodeBlocks<'a>(&'a fontpixel::Font);
impl serde::Serialize for UnicodeBlocks<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use fontpixel::charset::merge_categories::{UNICODE_BLOCKS, unicode_block};
        use serde::ser::SerializeSeq;
        #[derive(serde::Serialize)]
        struct Block {
            count: usize,
            end: u32,
            name: &'static str,
            start: u32,
        }
        let mut counts = vec![0usize; UNICODE_BLOCKS.len()];
        let codec = self.0.charset.codec().ok().filter(|c| c.known());
        for (cp, _) in self.0.codepoints() {
            let scalar = match cp {
                fontpixel::model::Codepoint::Unicode(value) => {
                    char::from_u32(value).map(|c| c as u32)
                }
                fontpixel::model::Codepoint::Legacy(value) => {
                    codec.as_ref().and_then(|c| c.cp2unicode(value))
                }
            };
            if let Some(block) = scalar.and_then(unicode_block)
                && let Ok(index) = UNICODE_BLOCKS.binary_search_by_key(&block.start, |b| b.start)
            {
                counts[index] += 1;
            }
        }
        let mut seq = serializer.serialize_seq(None)?;
        for (block, count) in UNICODE_BLOCKS.iter().zip(counts) {
            if count != 0 {
                seq.serialize_element(&Block {
                    count,
                    end: block.end,
                    name: block.name,
                    start: block.start,
                })?;
            }
        }
        seq.end()
    }
}
struct FontInfo<'a>(&'a fontpixel::Font);
impl serde::Serialize for FontInfo<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        #[derive(serde::Serialize)]
        struct Fraction {
            denominator: u32,
            numerator: i64,
        }
        let font = self.0;
        let unencoded = font
            .glyphs()
            .filter(|(_, g)| g.codepoints.is_empty())
            .count();
        let count = font.codepoints().count();
        let first = font.codepoints().next().map(|(cp, _)| cp.value());
        let last = font.codepoints().next_back().map(|(cp, _)| cp.value());
        let bounds = font
            .metrics
            .bounds
            .map(|b| [i64::from(b.width), i64::from(b.height), b.x, b.y]);
        let mut map = serializer.serialize_struct("FontInfo", 21)?;
        map.serialize_field("ascent", &font.metrics.ascent)?;
        map.serialize_field("bounds", &bounds)?;
        map.serialize_field("charset", &font.charset.canonical_name())?;
        map.serialize_field("charset_encoding", &font.charset.encoding)?;
        map.serialize_field("charset_registry", &font.charset.registry)?;
        map.serialize_field("descent", &font.metrics.descent)?;
        map.serialize_field("distinct_codes", &count)?;
        map.serialize_field("encoded_count", &(font.len() - unencoded))?;
        map.serialize_field("family", &font.names.family)?;
        map.serialize_field("first_code", &first)?;
        map.serialize_field("gl", &font.charset.gl)?;
        map.serialize_field("glyph_count", &font.len())?;
        map.serialize_field("headers", &PropertiesRef(&font.headers))?;
        map.serialize_field("last_code", &last)?;
        map.serialize_field("name", &font.names.xlfd)?;
        map.serialize_field("pixel_size", &font.metrics.pixel_size)?;
        map.serialize_field(
            "point_size",
            &font.metrics.point_size.map(|v| Fraction {
                denominator: v.denominator(),
                numerator: v.numerator(),
            }),
        )?;
        map.serialize_field("properties", &PropertiesRef(&font.properties))?;
        map.serialize_field("resolution", &font.metrics.resolution.map(|v| [v.x, v.y]))?;
        map.serialize_field("unencoded_count", &unencoded)?;
        map.serialize_field("unicode_blocks", &UnicodeBlocks(font))?;
        map.end()
    }
}
struct FontInfos<'a>(&'a [fontpixel::Font]);
impl serde::Serialize for FontInfos<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for font in self.0 {
            seq.serialize_element(&FontInfo(font))?;
        }
        seq.end()
    }
}
#[derive(serde::Serialize)]
struct InfoSummary<'a> {
    font_count: usize,
    fonts: FontInfos<'a>,
    format: &'a str,
    report: &'a fontpixel::Report,
}
