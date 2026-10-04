//! AngelCode text/XML/binary v3 and Jam3 JSON descriptors with bounded PNG pages.
use super::editor_fidelity as fidelity;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Gray8};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Vector};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Error, Report, ResourceLimits, Result};
use fidelity::{Budget, audit, bound, loss};
use std::collections::{BTreeMap, BTreeSet};
mod json;
mod reader;
mod wire;
mod writer;
/// AngelCode descriptor serialization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BmfontEncoding {
    /// Standard key/value lines.
    #[default]
    Text,
    /// Standard XML document.
    Xml,
    /// Version 3 little-endian binary blocks.
    Binary,
    /// Jam3 JSON descriptor with ordered string pages and glyph arrays.
    Json,
}
/// Explicit coverage conversion on input.
#[derive(Debug, Clone, Default)]
pub struct BmfontReadOptions {
    /// Convert Gray8 coverage to mono at this threshold.
    pub gray_threshold: Option<u8>,
}
/// Deterministic shelf-packing and serialization controls.
#[derive(Debug, Clone)]
pub struct BmfontWriteOptions {
    /// Descriptor encoding.
    pub encoding: BmfontEncoding,
    /// Width of each PNG atlas.
    pub page_width: u32,
    /// Height of each PNG atlas.
    pub page_height: u32,
    /// Gap between packed rectangles.
    pub padding: u32,
    /// Aggregate input, model, raster and output limits.
    pub limits: ResourceLimits,
}
impl Default for BmfontWriteOptions {
    fn default() -> Self {
        Self {
            encoding: BmfontEncoding::Text,
            page_width: 512,
            page_height: 512,
            padding: 1,
            limits: ResourceLimits::default(),
        }
    }
}
/// Native BMFont registry adapter.
#[derive(Debug, Clone, Default)]
pub struct BmfontCodec {
    /// Input conversion controls.
    pub read_options: BmfontReadOptions,
    /// Output packing controls.
    pub write_options: BmfontWriteOptions,
}
impl FormatDetector for BmfontCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for BmfontCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (f, mut r) = read(c, &self.read_options)?;
        fidelity::attach_asset(
            &mut r,
            c.name(),
            c.retained_input_bytes(),
            fidelity::model_bytes(&f)?,
            c.limits(),
        )?;
        let mut set = FontSet::from(f);
        set.source_format = Some("bmfont".into());
        Ok((set, r))
    }
}
impl FormatWriter<FontSet> for BmfontCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let changed = !s.properties.is_empty() || s.source_format.as_deref() != Some("bmfont");
        let reserve = fidelity::properties_bytes(&s.properties)?
            .saturating_add(s.source_format.as_ref().map_or(0, String::capacity))
            .saturating_add(
                s.fonts
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Font>()),
            )
            .saturating_add(if changed { 512 } else { 0 });
        let mut options = self.write_options.clone();
        options.limits.max_total_bytes = Budget::new(reserve, &options.limits)?.remaining();
        let font = s.single().map_err(|e| {
            fidelity::failed_write(
                e,
                "bmfont",
                s.fonts.iter().enumerate(),
                reserve,
                &self.write_options.limits,
            )
        })?;
        let mut w = write(font, &o.primary_name, &options)?;
        if changed {
            loss(
                &mut w.report,
                "bmfont",
                "container_metadata",
                0,
                None,
                "Container metadata or source format changed",
            );
        }
        if o.strict && w.report.has_loss() {
            return Err(Error::StrictLoss(w.report));
        }
        Ok(w)
    }
}
/// Recognize binary/text/XML signatures or a structurally complete JSON descriptor.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"BMF") {
        return Confidence::Certain;
    }
    let json_head = b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b);
    if json_head.iter().find(|v| !v.is_ascii_whitespace()) == Some(&b'{') {
        return json::detect(b);
    }
    let s = std::str::from_utf8(&b[..b.len().min(8192)])
        .unwrap_or("")
        .trim_start_matches('\u{feff}')
        .trim_start();
    if s.starts_with("info ") {
        return Confidence::Certain;
    }
    for t in xmlparser::Tokenizer::from(s) {
        if let Ok(xmlparser::Token::ElementStart { prefix, local, .. }) = t {
            return if prefix.is_empty() && local.as_str() == "font" {
                Confidence::Certain
            } else {
                Confidence::NoMatch
            };
        }
    }
    Confidence::NoMatch
}
/// Decode a descriptor and its explicitly scoped PNG resources.
pub fn read(c: &mut ReadContext<'_>, o: &BmfontReadOptions) -> Result<(Font, Report)> {
    reader::read(c, o)
}
/// Encode one strike and PNG pages as an in-memory output bundle.
pub fn write(f: &Font, primary: &ResourcePath, o: &BmfontWriteOptions) -> Result<Written> {
    writer::write(f, primary, o)
        .map_err(|e| fidelity::failed_write(e, "bmfont", std::iter::once((0, f)), 0, &o.limits))
}
fn invalid(m: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("BMFONT: {m}"))
}
#[derive(Debug, Clone)]
struct Record {
    tag: String,
    fields: BTreeMap<String, String>,
    // Only nonstandard scalar types need provenance; standard fields have fixed types.
    json_types: BTreeMap<String, String>,
}
impl Record {
    fn new(tag: &str) -> Self {
        Self {
            tag: tag.into(),
            fields: BTreeMap::new(),
            json_types: BTreeMap::new(),
        }
    }
    fn set(&mut self, k: &str, v: impl ToString) {
        self.fields.insert(k.into(), v.to_string());
    }
    fn text(&self, k: &str) -> Result<&str> {
        self.fields
            .get(k)
            .map(String::as_str)
            .ok_or_else(|| invalid(format!("{} lacks {k}", self.tag)))
    }
    fn num(&self, k: &str) -> Result<i64> {
        self.text(k)?.parse().map_err(invalid)
    }
    fn number(&self, k: &str, default: i64) -> Result<i64> {
        self.fields.get(k).map_or(Ok(default), |v| {
            if [
                "unicode",
                "bold",
                "italic",
                "smooth",
                "packed",
                "fixedHeight",
            ]
            .contains(&k)
            {
                if v.eq_ignore_ascii_case("true") {
                    return Ok(1);
                }
                if v.eq_ignore_ascii_case("false") {
                    return Ok(0);
                }
            }
            v.parse().map_err(invalid)
        })
    }
    fn property(&self) -> Property {
        Property::new(
            format!("bmfont.{}", self.tag),
            PropertyValue::List(
                self.fields
                    .iter()
                    .map(|(k, v)| {
                        let mut pair = vec![
                            PropertyValue::Text(k.clone()),
                            PropertyValue::Text(v.clone()),
                        ];
                        if let Some(kind) = self.json_types.get(k) {
                            pair.push(PropertyValue::Text(format!("json:{kind}")));
                        }
                        PropertyValue::List(pair)
                    })
                    .collect(),
            ),
        )
    }
    fn from_property(p: &Property) -> Result<Self> {
        let tag = p
            .name
            .strip_prefix("bmfont.")
            .ok_or_else(|| invalid("native property name"))?;
        let mut r = Self::new(tag);
        if let PropertyValue::List(items) = &p.value {
            for item in items {
                if let PropertyValue::List(pair) = item
                    && (pair.len() == 2 || pair.len() == 3)
                    && let (PropertyValue::Text(k), PropertyValue::Text(v)) = (&pair[0], &pair[1])
                {
                    if pair.len() == 3 {
                        let PropertyValue::Text(marker) = &pair[2] else {
                            return Err(invalid("native JSON field type"));
                        };
                        let kind = marker
                            .strip_prefix("json:")
                            .filter(|v| ["number", "bool", "numbers"].contains(v))
                            .ok_or_else(|| invalid("native JSON field type"))?;
                        r.json_types.insert(k.clone(), kind.into());
                    }
                    if r.fields.insert(k.clone(), v.clone()).is_some() {
                        return Err(invalid("duplicate native property field"));
                    }
                    continue;
                }
                return Err(invalid("native property pair"));
            }
        } else {
            return Err(invalid("native record property"));
        }
        Ok(r)
    }
}
fn bl(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: u16::MAX as u32,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
    }
}
