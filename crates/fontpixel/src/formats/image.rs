//! Explicitly mapped image atlases and native glyph-image directories.
//!
//! [`Mapping`] accepts the catalogue's grid/cell schema. Native writes additionally
//! retain exact font and glyph metadata; all resource access stays caller-scoped.
mod budget;
mod imageset;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Bounds, Gray8};
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, FontNames, FontSet, Glyph, GlyphBitmap,
    GlyphMetrics, Properties, Rational, Vector,
};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, ResourcePath, Result};
use budget::Budget;
use image_codec::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
pub use imageset::{ImageLayout, ImageSetBackground, ImageSetDomain, ImageSetOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::{Cursor, Write};

fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
fn add(total: &mut usize, n: usize, max: usize) -> Result<()> {
    *total = total.checked_add(n).ok_or(Error::ResourceLimit {
        resource: "image aggregate bytes",
        limit: max,
    })?;
    check(*total, max, "image aggregate bytes")
}
fn diag(report: &mut Report, code: &str, index: usize, message: &str) {
    report.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("image.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: Some(index),
            ..Default::default()
        },
        message: message.into(),
    });
}
fn bl(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
    }
}

/// A catalogue-compatible mapping, optionally carrying native metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    /// Native schema version; absent for external catalogue maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fontpixel_image: Option<u8>,
    /// Safe image name when this mapping is the entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Nominal font pixel size (required for external mappings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel_size: Option<u32>,
    /// Baseline measured down from the cell top; defaults to pixel_size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<i64>,
    /// Selected opaque RGB colors; empty means any nontransparent pixel.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreground: Vec<[u8; 3]>,
    /// Regular cell width and height, both positive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_size: Option<[u32; 2]>,
    /// Regular row-major Unicode sequence, one scalar per cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters: Option<String>,
    /// Explicit top-left image rectangles; mutually exclusive with grid options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cells: Option<Vec<Cell>>,
    /// Skip blank non-whitespace external cells, reporting every omission.
    #[serde(default)]
    pub omit_blank: bool,
    /// Optional catalogue variant name (not a font family assignment).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Exact native font/container metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<NativeMetadata>,
}
/// A glyph rectangle and its external or native labels/metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    /// [left, top, width, height], in image coordinates.
    pub rect: [u32; 4],
    /// Exactly one Unicode scalar, or empty for an unencoded glyph.
    #[serde(default, rename = "char", skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// Glyph name; required for unencoded external glyphs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Horizontal advance, default rectangle width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advance: Option<i64>,
    /// Baseline from this cell's top, overriding the mapping baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<i64>,
    /// Horizontal BBX bearing, default zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearing_x: Option<i64>,
    /// Native complete glyph fields, without bitmap bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<NativeGlyph>,
}
impl Cell {
    /// Construct an external Unicode cell with default metrics.
    pub fn character(rect: [u32; 4], c: char) -> Self {
        Self {
            rect,
            character: Some(c.to_string()),
            name: None,
            advance: None,
            baseline: None,
            bearing_x: None,
            native: None,
        }
    }
}
/// Exact native strike and container metadata. Versioned by the enclosing mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMetadata {
    /// Strike metrics, including stable default-glyph ID.
    pub metrics: FontMetrics,
    /// Human-readable metadata.
    pub names: FontNames,
    /// Original charset tags.
    pub charset: FontCharset,
    /// Original format provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_format: Option<String>,
    /// Ordered original headers.
    pub headers: Properties,
    /// Ordered original properties.
    pub properties: Properties,
    /// Total slots, including removed glyphs.
    pub slots: usize,
    /// Ordered container properties.
    pub container_properties: Properties,
    /// Container provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_source: Option<String>,
}
/// Complete native glyph metadata. Pixel bytes live only in the image.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeGlyph {
    /// Original stable glyph-table index.
    pub id: usize,
    /// Original aliases with explicit Unicode/legacy tags.
    pub codepoints: Vec<Codepoint>,
    /// Original Unicode sequence mappings.
    pub unicode_sequences: Vec<Vec<u32>>,
    /// Exact glyph name, including absent versus empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Exact horizontal and vertical metrics.
    pub metrics: GlyphMetrics,
    /// True for byte-for-byte grayscale pixels, false for monochrome.
    pub gray: bool,
    /// Ordered per-glyph properties.
    pub properties: Properties,
}
/// Atlas mapping or native directory interpretation.
#[derive(Debug, Clone, Default)]
pub struct ImageReadOptions {
    /// Filename labels and coverage for a native directory input.
    pub imageset: ImageSetOptions,
    /// Explicit mapping; mutually exclusive with mapping_path.
    pub mapping: Option<Mapping>,
    /// Caller-authorized relative JSON/TOML mapping asset.
    pub mapping_path: Option<ResourcePath>,
}
/// Mapping serialization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MappingFormat {
    /// Compact deterministic JSON.
    #[default]
    Json,
    /// Deterministic TOML.
    Toml,
}
/// Image container choice; default infers it from the output name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// PNG, lossless and grayscale-capable.
    Png,
    /// Windows bitmap, grayscale-capable.
    Bmp,
    /// Single-frame monochrome GIF.
    Gif,
    /// Truevision TGA, grayscale-capable.
    Tga,
    /// Binary portable bitmap (P4), monochrome.
    Pbm,
    /// Binary portable graymap (P5).
    Pgm,
}
impl Encoding {
    fn format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Bmp => ImageFormat::Bmp,
            Self::Gif => ImageFormat::Gif,
            Self::Tga => ImageFormat::Tga,
            Self::Pbm | Self::Pgm => ImageFormat::Pnm,
        }
    }
    fn from_name(n: &str) -> Result<Self> {
        match n
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => Ok(Self::Png),
            "bmp" => Ok(Self::Bmp),
            "gif" => Ok(Self::Gif),
            "tga" => Ok(Self::Tga),
            "pbm" => Ok(Self::Pbm),
            "pgm" | "pnm" => Ok(Self::Pgm),
            _ => Err(invalid(
                "image output requires png/bmp/gif/tga/pbm/pgm extension or explicit encoding",
            )),
        }
    }
}
/// Bounded deterministic atlas or native directory output.
#[derive(Debug, Clone)]
pub struct ImageWriteOptions {
    /// Atlas with mapping, or native separately labelled glyph images.
    pub layout: ImageLayout,
    /// Native directory labels, coverage and optional external baseline.
    pub imageset: ImageSetOptions,
    /// Container override; otherwise use output extension.
    pub encoding: Option<Encoding>,
    /// JSON by default.
    pub mapping_format: MappingFormat,
    /// Number of fixed-size atlas cells in each row; default 16.
    pub columns: u32,
    /// Explicit threshold required for Gray8 to GIF/PBM conversion.
    pub gray_threshold: Option<u8>,
    /// Output and intermediate resource budgets.
    pub limits: ResourceLimits,
}
impl Default for ImageWriteOptions {
    fn default() -> Self {
        Self {
            layout: ImageLayout::Atlas,
            imageset: ImageSetOptions::default(),
            encoding: None,
            mapping_format: MappingFormat::Json,
            columns: 16,
            gray_threshold: None,
            limits: ResourceLimits::default(),
        }
    }
}
/// Image codec installed by the `image` feature.
#[derive(Debug, Clone, Default)]
pub struct ImageCodec {
    /// Input mapping and native directory profile.
    pub read_options: ImageReadOptions,
    /// Output packing and limits.
    pub write_options: ImageWriteOptions,
}
fn is_map(name: &str) -> bool {
    name.ends_with(".json") || name.ends_with(".toml")
}
fn sidecar(name: &str, ext: &str) -> Result<ResourcePath> {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    ResourcePath::new(format!("{stem}.{ext}"))
}
fn parse_map(bytes: &[u8], toml: bool, l: &ResourceLimits, retained: usize) -> Result<Mapping> {
    check(bytes.len(), l.max_table_bytes, "image mapping bytes")?;
    if !toml {
        budget::json(bytes, l, retained)?;
    }
    // TOML's native envelope is one string, with bounded unescaping scratch.
    // Ordinary external TOML retains a conservative parser/tree allowance.
    if toml {
        let envelope = bytes.windows(12).any(|s| s == b"payload_json");
        check(
            bytes.len().saturating_mul(if envelope { 6 } else { 32 }),
            l.max_total_bytes.saturating_sub(retained),
            "image TOML allocation bound",
        )?;
    }
    let s = std::str::from_utf8(bytes).map_err(|_| invalid("image mapping is not UTF-8"))?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        fontpixel_image: u8,
        payload_json: String,
    }
    let map = if toml {
        if let Ok(e) = toml::from_str::<Envelope>(s) {
            if e.fontpixel_image != 1 {
                return Err(invalid("unsupported native TOML version"));
            }
            budget::json(
                e.payload_json.as_bytes(),
                l,
                retained.saturating_add(e.payload_json.len()),
            )?;
            let map: Mapping = serde_json::from_str(&e.payload_json)
                .map_err(|e| invalid(format!("image TOML payload: {e}")))?;
            if map.fontpixel_image != Some(1) {
                return Err(invalid("TOML envelope requires native payload"));
            }
            Budget::new(l, retained)?.mapping(&map)?;
            return Ok(map);
        }
        check(
            bytes.len().saturating_mul(32),
            l.max_total_bytes.saturating_sub(retained),
            "image TOML allocation bound",
        )?;
        toml::from_str(s).map_err(|e| invalid(format!("image TOML: {e}")))
    } else {
        serde_json::from_str(s).map_err(|e| invalid(format!("image JSON: {e}")))
    }?;
    Budget::new(l, retained)?.mapping(&map)?;
    Ok(map)
}
impl FormatDetector for ImageCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        let b = c.bytes();
        if image_codec::guess_format(b).is_ok_and(|f| {
            matches!(
                f,
                ImageFormat::Png | ImageFormat::Bmp | ImageFormat::Gif | ImageFormat::Pnm
            )
        }) {
            return Ok(Confidence::Possible);
        }
        if b.windows(15).any(|v| v == b"fontpixel_image") {
            return Ok(Confidence::Possible);
        }
        Ok(Confidence::NoMatch)
    }
}
impl FormatReader<FontSet> for ImageCodec {
    fn read(&self, c: &mut ReadContext<'_>, o: &ReadOptions) -> Result<(FontSet, Report)> {
        if c.is_directory() {
            if self.read_options.mapping.is_some() || self.read_options.mapping_path.is_some() {
                return Err(invalid("imageset input cannot use an atlas mapping"));
            }
            return imageset::read(c, &self.read_options.imageset);
        }
        if self.read_options.mapping.is_some() && self.read_options.mapping_path.is_some() {
            return Err(invalid("choose mapping or mapping_path, not both"));
        }
        let name = c
            .name()
            .unwrap_or("atlas.png")
            .rsplit('/')
            .next()
            .unwrap_or("atlas.png")
            .to_owned();
        let (mapping, resource, image_name) = if is_map(&name) {
            if self.read_options.mapping.is_some() || self.read_options.mapping_path.is_some() {
                return Err(invalid(
                    "mapping entry point cannot also use a mapping override",
                ));
            }
            let map = parse_map(
                c.bytes(),
                name.ends_with(".toml"),
                &o.limits,
                c.retained_input_bytes(),
            )?;
            let p = ResourcePath::new(
                map.image
                    .as_ref()
                    .ok_or_else(|| invalid("mapping entry point needs image field"))?
                    .clone(),
            )?;
            let mut budget = Budget::new(&o.limits, c.retained_input_bytes())?;
            budget.mapping(&map)?;
            let b = c.resource_limited(&p, o.limits.max_total_bytes - budget.used)?;
            (map, Some(b), p.to_string())
        } else {
            let map = if let Some(map) = &self.read_options.mapping {
                Budget::new(&o.limits, c.retained_input_bytes())?.mapping(map)?;
                map.clone()
            } else {
                let (b, t) = if let Some(p) = &self.read_options.mapping_path {
                    (c.resource(p)?, p.as_str().ends_with(".toml"))
                } else {
                    match c.resource(&sidecar(&name, "json")?) {
                        Ok(b) => (b, false),
                        Err(Error::MissingResource(_)) => {
                            (c.resource(&sidecar(&name, "toml")?)?, true)
                        }
                        Err(e) => return Err(e),
                    }
                };
                parse_map(&b, t, &o.limits, c.retained_input_bytes())?
            };
            (map, None, name)
        };
        decode(
            resource.as_deref().unwrap_or(c.bytes()),
            &image_name,
            mapping,
            &o.limits,
            c.retained_input_bytes(),
        )
    }
}
fn gif_single_frame(b: &[u8]) -> Result<()> {
    if b.len() < 13 {
        return Err(invalid("truncated GIF"));
    }
    let mut p = 13usize;
    if b[10] & 128 != 0 {
        p += 3 * (1usize << ((b[10] & 7) + 1));
    }
    let mut frames = 0;
    loop {
        let marker = *b.get(p).ok_or_else(|| invalid("truncated GIF blocks"))?;
        p += 1;
        match marker {
            0x3b => {
                return if frames == 1 {
                    Ok(())
                } else {
                    Err(invalid("GIF has no image"))
                };
            }
            0x21 => {
                p = p.checked_add(1).ok_or_else(|| invalid("GIF overflow"))?;
            }
            0x2c => {
                frames += 1;
                if frames > 1 {
                    return Err(invalid("animated GIF is not a single font atlas"));
                }
                let d = b
                    .get(p..p + 9)
                    .ok_or_else(|| invalid("truncated GIF descriptor"))?;
                p += 9;
                if d[8] & 128 != 0 {
                    p += 3 * (1usize << ((d[8] & 7) + 1));
                }
                p += 1;
            }
            _ => return Err(invalid("invalid GIF block")),
        }
        loop {
            let n = usize::from(*b.get(p).ok_or_else(|| invalid("truncated GIF subblock"))?);
            p += 1;
            if n == 0 {
                break;
            }
            p = p.checked_add(n).ok_or_else(|| invalid("GIF overflow"))?;
            if p > b.len() {
                return Err(invalid("truncated GIF data"));
            }
        }
    }
}
// Reject animation control even when an APNG's fallback image differs from frame 0.
fn png_single_frame(b: &[u8]) -> Result<()> {
    let mut p = 8usize;
    while p < b.len() {
        let header = b
            .get(p..p.saturating_add(8))
            .ok_or_else(|| invalid("truncated PNG chunk"))?;
        let n =
            u32::from_be_bytes(header[..4].try_into().map_err(|_| invalid("PNG length"))?) as usize;
        if &header[4..] == b"acTL" {
            return Err(invalid("animated PNG is not a single font atlas"));
        }
        p = p
            .checked_add(n)
            .and_then(|v| v.checked_add(12))
            .ok_or_else(|| invalid("PNG chunk overflow"))?;
        if p > b.len() {
            return Err(invalid("truncated PNG payload"));
        }
        if &header[4..] == b"IEND" {
            break;
        }
    }
    Ok(())
}
fn decode(
    bytes: &[u8],
    name: &str,
    mut map: Mapping,
    l: &ResourceLimits,
    retained: usize,
) -> Result<(FontSet, Report)> {
    let format = image_codec::guess_format(bytes)
        .or_else(|_| ImageFormat::from_path(name))
        .map_err(|e| invalid(format!("image format: {e}")))?;
    if !matches!(
        format,
        ImageFormat::Png
            | ImageFormat::Bmp
            | ImageFormat::Gif
            | ImageFormat::Tga
            | ImageFormat::Pnm
    ) {
        return Err(invalid("unsupported atlas image format"));
    }
    if format == ImageFormat::Gif {
        gif_single_frame(bytes)?;
    }
    if format == ImageFormat::Png {
        png_single_frame(bytes)?;
    }
    let mut budget = Budget::new(l, retained)?;
    budget.mapping(&map)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut lim = image_codec::Limits::default();
    lim.max_alloc = Some(
        l.max_decompressed_bytes
            .min(l.max_total_bytes - budget.used) as u64,
    );
    reader.limits(lim);
    let decoder = reader
        .into_decoder()
        .map_err(|e| invalid(format!("image header: {e}")))?;
    let (w, h) = decoder.dimensions();
    let pixels = usize::try_from(u64::from(w) * u64::from(h))
        .map_err(|_| invalid("atlas dimensions overflow"))?;
    let rgba = pixels
        .checked_mul(4)
        .ok_or_else(|| invalid("atlas byte size overflow"))?;
    check(rgba, l.max_decompressed_bytes, "decoded image bytes")?;
    let decoded = usize::try_from(decoder.total_bytes())
        .map_err(|_| invalid("decoded image size overflow"))?;
    check(decoded, l.max_decompressed_bytes, "decoded image bytes")?;
    let mut used = budget.used;
    add(&mut used, decoded, l.max_total_bytes)?;
    add(&mut used, rgba, l.max_total_bytes)?;
    add(&mut used, pixels, l.max_total_bytes)?;
    let img = DynamicImage::from_decoder(decoder)
        .map_err(|e| invalid(format!("image decode: {e}")))?
        .to_rgba8();
    let native = map.fontpixel_image.is_some();
    if native && (map.fontpixel_image != Some(1) || map.native.is_none()) {
        return Err(invalid("unsupported/incomplete native image metadata"));
    }
    if !native && map.native.is_some() {
        return Err(invalid("native metadata requires fontpixel_image version"));
    }
    if map.name.as_ref().is_some_and(|s| {
        s.is_empty()
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }) {
        return Err(invalid("invalid atlas variant name"));
    }
    if map.cells.is_some() && (map.cell_size.is_some() || map.characters.is_some()) {
        return Err(invalid("choose cells or regular grid, not both"));
    }
    let size = if native {
        0
    } else {
        map.pixel_size
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("external mapping needs positive pixel_size"))?
    };
    let baseline = map.baseline.unwrap_or(i64::from(size));
    map.foreground.sort_unstable();
    map.foreground.dedup();
    let lit = |p: &image_codec::Rgba<u8>| {
        if native {
            p[0] != 0 || p[1] != 0 || p[2] != 0
        } else {
            p[3] != 0
                && (map.foreground.is_empty()
                    || map.foreground.binary_search(&[p[0], p[1], p[2]]).is_ok())
        }
    };
    let cells = if let Some(cells) = map.cells.take() {
        cells
    } else {
        if native {
            return Err(invalid("native map needs explicit cells"));
        }
        let [cw, ch] = map
            .cell_size
            .ok_or_else(|| invalid("mapping needs cells or cell_size"))?;
        if cw == 0 || ch == 0 || w % cw != 0 || h % ch != 0 {
            return Err(invalid(
                "atlas dimensions must be multiples of positive cell size",
            ));
        }
        let cols = w / cw;
        let count = usize::try_from(u64::from(cols) * u64::from(h / ch))
            .map_err(|_| invalid("grid count overflow"))?;
        check(count, l.max_glyphs, "image grid count")?;
        let chars = map
            .characters
            .as_deref()
            .ok_or_else(|| invalid("grid needs characters"))?;
        add(
            &mut used,
            chars
                .chars()
                .count()
                .saturating_mul(std::mem::size_of::<Cell>() * 2 + 8),
            l.max_total_bytes,
        )?;
        let mut out = Vec::new();
        for (i, chv) in chars.chars().enumerate() {
            if i >= count {
                return Err(invalid("character map extends beyond atlas"));
            }
            out.push(Cell::character(
                [(i as u32 % cols) * cw, (i as u32 / cols) * ch, cw, ch],
                chv,
            ));
        }
        out
    };
    check(cells.len(), l.max_glyphs, "image glyph count")?;
    add(
        &mut used,
        cells
            .len()
            .checked_mul(if native {
                256
            } else {
                std::mem::size_of::<Glyph>() * 4 + 512
            })
            .ok_or_else(|| invalid("glyph count overflow"))?,
        l.max_total_bytes,
    )?;
    // Native cells already have stable order: move their metadata directly into
    // the font instead of retaining a second complete glyph vector.
    let slots = map.native.as_ref().map(|n| n.slots);
    if let Some(slots) = slots {
        add(
            &mut used,
            slots.saturating_mul(std::mem::size_of::<Option<Glyph>>() * 2),
            l.max_total_bytes,
        )?;
    }
    let mut font = Font::default();
    let mut occupied = vec![false; pixels];
    let mut glyphs = Vec::new();
    let mut seen = BTreeSet::new();
    let mut report = Report::default();
    for (i, cell) in cells.into_iter().enumerate() {
        let [x, y, cw, ch] = cell.rect;
        if x.checked_add(cw).is_none_or(|r| r > w) || y.checked_add(ch).is_none_or(|b| b > h) {
            return Err(invalid(format!("cell {i} is outside atlas")));
        }
        let area = u64::from(cw) * u64::from(ch);
        if area > l.max_pixels_per_glyph as u64 {
            return Err(Error::ResourceLimit {
                resource: "image glyph pixels",
                limit: l.max_pixels_per_glyph,
            });
        }
        let gray = cell.native.as_ref().is_some_and(|n| n.gray);
        let stride = if gray {
            cw as usize
        } else {
            (cw as usize).div_ceil(8)
        };
        let len = stride
            .checked_mul(ch as usize)
            .ok_or_else(|| invalid("glyph bytes overflow"))?;
        add(&mut used, len.saturating_mul(2), l.max_total_bytes)?;
        let mut b = vec![0u8; len];
        for py in 0..if cw == 0 { 0 } else { ch } {
            for px in 0..cw {
                let p = img.get_pixel(x + px, y + py);
                let pos = ((y + py) as usize) * w as usize + (x + px) as usize;
                if occupied[pos] {
                    return Err(invalid("overlapping atlas rectangles"));
                }
                occupied[pos] = true;
                if native
                    && (p[0] != p[1]
                        || p[1] != p[2]
                        || p[3] != 255
                        || (!gray && p[0] != 0 && p[0] != 255))
                {
                    return Err(invalid(
                        "native atlas pixels do not match declared grayscale/mono depth",
                    ));
                }
                if gray {
                    b[py as usize * stride + px as usize] = p[0];
                } else if lit(p) {
                    b[py as usize * stride + px as usize / 8] |= 128 >> (px % 8);
                }
            }
        }
        let bitmap = if gray {
            GlyphBitmap::Gray8(Bitmap::<Gray8>::from_raw(cw, ch, &b, bl(l))?)
        } else {
            GlyphBitmap::Mono(Bitmap::from_raw(cw, ch, &b, bl(l))?)
        };
        let (id, glyph) = if native {
            if cell.character.is_some()
                || cell.advance.is_some()
                || cell.baseline.is_some()
                || cell.bearing_x.is_some()
                || cell.name.is_some()
            {
                return Err(invalid("native cells cannot also supply external fields"));
            }
            let n = cell
                .native
                .ok_or_else(|| invalid("native cell lacks metadata"))?;
            (
                n.id,
                Glyph {
                    codepoints: n.codepoints,
                    unicode_sequences: n.unicode_sequences,
                    name: n.name,
                    metrics: n.metrics,
                    bitmap,
                    properties: n.properties,
                },
            )
        } else {
            if cell.native.is_some() {
                return Err(invalid("external cell contains native fields"));
            }
            let mut chars = cell.character.as_deref().unwrap_or("").chars();
            let chv = chars.next();
            if chars.next().is_some() {
                return Err(invalid("cell char must be one Unicode scalar"));
            }
            if chv.is_none() && cell.name.is_none() {
                return Err(invalid("unencoded cell needs a name"));
            }
            if map.omit_blank
                && bitmap.as_bytes().iter().all(|v| *v == 0)
                && chv.is_none_or(|c| !c.is_whitespace())
            {
                diag(
                    &mut report,
                    "read.omit_blank",
                    i,
                    "Explicit omit_blank removed this cell",
                );
                continue;
            }
            if let Some(c) = chv
                && !seen.insert(c)
            {
                return Err(invalid("duplicate image Unicode mapping"));
            }
            let mut glyph = Glyph::new(bitmap);
            glyph.codepoints = chv
                .map(|c| vec![Codepoint::Unicode(c as u32)])
                .unwrap_or_default();
            glyph.name = cell.name.or_else(|| {
                Some(chv.map_or_else(|| format!("cell{i}"), |c| format!("uni{:04X}", c as u32)))
            });
            glyph.metrics.bounds = Bounds::new(
                cw,
                ch,
                cell.bearing_x.unwrap_or(0),
                cell.baseline
                    .unwrap_or(baseline)
                    .checked_sub(i64::from(ch))
                    .ok_or_else(|| invalid("baseline overflow"))?,
            );
            glyph.metrics.advance = Some(Vector::new(cell.advance.unwrap_or(i64::from(cw)), 0));
            (glyphs.len(), glyph)
        };
        glyph.validate()?;
        if let Some(slots) = slots {
            if id < font.slot_count() || id >= slots {
                return Err(invalid(
                    "native glyph IDs must be unique, increasing and below slots",
                ));
            }
            while font.slot_count() < id {
                let hole = font.add_glyph(Glyph::default())?;
                font.remove_glyph(hole);
            }
            font.add_glyph(glyph)?;
        } else {
            glyphs.push((id, glyph));
        }
    }
    for (i, p) in img.pixels().enumerate() {
        if !occupied[i] && lit(p) {
            return Err(invalid("visible atlas pixels have no character mapping"));
        }
    }
    let mut set = FontSet::default();
    if let Some(n) = map.native {
        while font.slot_count() < n.slots {
            let hole = font.add_glyph(Glyph::default())?;
            font.remove_glyph(hole);
        }
        font.metrics = n.metrics;
        font.names = n.names;
        font.charset = n.charset;
        font.headers = n.headers;
        font.properties = n.properties;
        font.source_format = n.source_format;
        set.properties = n.container_properties;
        set.source_format = n.container_source;
    } else {
        if glyphs.is_empty() {
            return Err(invalid("atlas has no glyphs"));
        }
        // Catalogue import places encoded glyphs in Unicode order, then unencoded.
        glyphs.sort_by_key(|(_, g)| g.codepoints.first().map_or(u32::MAX, |c| c.value()));
        for (_, g) in glyphs {
            font.add_glyph(g)?;
        }
        font.update_bounds()?;
        let b = font.metrics.bounds.unwrap_or_default();
        font.metrics.pixel_size = Some(size);
        font.metrics.point_size = Some(Rational::integer(i64::from(size)));
        font.metrics.resolution = Some(Vector::new(72, 72));
        font.metrics.ascent = Some((b.y + i64::from(b.height)).max(0));
        font.metrics.descent = Some(
            b.y.checked_neg()
                .ok_or_else(|| invalid("descent overflow"))?
                .max(0),
        );
        font.charset.name = Some("unicode".into());
        font.source_format = Some("image".into());
    }
    font.validate(l)?;
    set.fonts.push(font);
    Ok((set, report))
}
// A bounded seekable buffer is required by image encoders. Refuse growth before
// handing memory to a codec rather than checking a completed unbounded Vec.
struct Sink {
    cursor: Cursor<Vec<u8>>,
    max: usize,
}
impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let end = self
            .cursor
            .position()
            .checked_add(b.len() as u64)
            .ok_or_else(|| std::io::Error::other("image output overflow"))?;
        if end > self.max as u64 {
            return Err(std::io::Error::other("image output limit"));
        }
        // Cursor's default geometric growth can overshoot the byte budget.
        let required = end as usize;
        let data = self.cursor.get_mut();
        if required > data.capacity() {
            let capacity = required
                .max(data.capacity().saturating_mul(2))
                .min(self.max);
            data.try_reserve_exact(capacity - data.len())
                .map_err(std::io::Error::other)?;
        }
        self.cursor.write(b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Seek for Sink {
    fn seek(&mut self, p: std::io::SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(p)
    }
}
impl FormatWriter<FontSet> for ImageCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        if self.write_options.layout == ImageLayout::Imageset {
            return imageset::write(set, o, &self.write_options);
        }
        let l = &self.write_options.limits;
        set.validate(l)?;
        let [font] = set.fonts.as_slice() else {
            return Err(invalid("image atlas needs exactly one font"));
        };
        if font.is_empty() {
            return Err(invalid("cannot write empty atlas"));
        }
        let enc = self
            .write_options
            .encoding
            .map(Ok)
            .unwrap_or_else(|| Encoding::from_name(o.primary_name.as_str()))?;
        let cols = self
            .write_options
            .columns
            .min(u32::try_from(font.len()).map_err(|_| invalid("atlas glyph count overflow"))?);
        if cols == 0 {
            return Err(invalid("atlas columns must be positive"));
        }
        let cw = font
            .glyphs()
            .map(|(_, g)| g.bitmap.width())
            .max()
            .unwrap_or(0)
            .max(1);
        let ch = font
            .glyphs()
            .map(|(_, g)| g.bitmap.height())
            .max()
            .unwrap_or(0)
            .max(1);
        let w = cw
            .checked_mul(cols)
            .ok_or_else(|| invalid("atlas width overflow"))?;
        let h = ch
            .checked_mul((font.len() as u32).div_ceil(cols))
            .ok_or_else(|| invalid("atlas height overflow"))?;
        let n = usize::try_from(u64::from(w) * u64::from(h))
            .map_err(|_| invalid("atlas size overflow"))?;
        check(
            n.saturating_mul(4),
            l.max_decompressed_bytes,
            "atlas decoded bytes",
        )?;
        let mut budget = Budget::new(l, 0)?;
        budget.source(set, font)?;
        let mut used = budget.used;
        // Charge simultaneously live encoder buffers for the selected codec.
        // Grayscale PNG needs no RGBA copy; raw formats stream borrowed pixels.
        let factor = match enc {
            Encoding::Png => 4,
            Encoding::Bmp => 5,
            Encoding::Gif => 12,
            Encoding::Tga | Encoding::Pbm | Encoding::Pgm => 1,
        };
        add(
            &mut used,
            n.saturating_mul(factor).saturating_add(512 * 1024),
            l.max_total_bytes,
        )?;
        let mut atlas = image_codec::GrayImage::new(w, h);
        let mut cells = Vec::new();
        let mut report = Report::default();
        for (i, (id, g)) in font.glyphs().enumerate() {
            let x = (i as u32 % cols) * cw;
            let y = (i as u32 / cols) * ch;
            let mut gray = matches!(g.bitmap, GlyphBitmap::Gray8(_));
            let threshold = if gray && matches!(enc, Encoding::Gif | Encoding::Pbm) {
                gray = false;
                diag(
                    &mut report,
                    "write.gray_threshold",
                    id.index(),
                    "Gray8 pixels were thresholded for monochrome output",
                );
                Some(
                    self.write_options
                        .gray_threshold
                        .ok_or_else(|| invalid("Gray8 GIF/PBM requires explicit gray_threshold"))?,
                )
            } else {
                None
            };
            for py in 0..g.bitmap.height() {
                for px in 0..g.bitmap.width() {
                    let v = match &g.bitmap {
                        GlyphBitmap::Mono(b) => {
                            if b.get_pixel(px, py)? != 0 {
                                255
                            } else {
                                0
                            }
                        }
                        GlyphBitmap::Gray8(b) => {
                            let v = b.get_pixel(px, py)?;
                            threshold.map_or(v, |t| if v >= t { 255 } else { 0 })
                        }
                    };
                    atlas.put_pixel(x + px, y + py, image_codec::Luma([v]));
                }
            }
            cells.push(Cell {
                rect: [x, y, g.bitmap.width(), g.bitmap.height()],
                character: None,
                name: None,
                advance: None,
                baseline: None,
                bearing_x: None,
                native: Some(NativeGlyph {
                    id: id.index(),
                    codepoints: g.codepoints.clone(),
                    unicode_sequences: g.unicode_sequences.clone(),
                    name: g.name.clone(),
                    metrics: g.metrics.clone(),
                    gray,
                    properties: g.properties.clone(),
                }),
            });
        }
        let mapping = Mapping {
            fontpixel_image: Some(1),
            image: Some(
                o.primary_name
                    .as_str()
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .into(),
            ),
            cells: Some(cells),
            native: Some(NativeMetadata {
                metrics: font.metrics.clone(),
                names: font.names.clone(),
                charset: font.charset.clone(),
                source_format: font.source_format.clone(),
                headers: font.headers.clone(),
                properties: font.properties.clone(),
                slots: font.slot_count(),
                container_properties: set.properties.clone(),
                container_source: set.source_format.clone(),
            }),
            ..Default::default()
        };
        // A successful write must also fit the selected native read budget.
        // This is a separate peak from encoder scratch, not additional live data.
        let mut readable = Budget::new(l, 0)?;
        readable.mapping(&mapping)?;
        let decoded_channels = if matches!(enc, Encoding::Bmp | Encoding::Gif) {
            4
        } else {
            1
        };
        readable.bytes(n.saturating_mul(decoded_channels + 5))?;
        readable.items::<Option<Glyph>>(font.slot_count())?;
        readable.bytes(font.len().saturating_mul(256))?;
        for (_, g) in font.glyphs() {
            readable.bytes(g.bitmap.as_bytes().len().saturating_mul(2))?;
        }
        let mapping_max = l
            .max_table_bytes
            .min((l.max_total_bytes.saturating_sub(used)) / 2);
        let (mapping_bytes, ext) = match self.write_options.mapping_format {
            MappingFormat::Json => {
                let mut sink = Sink {
                    cursor: Cursor::new(Vec::new()),
                    max: mapping_max,
                };
                serde_json::to_writer(&mut sink, &mapping).map_err(|e| invalid(e.to_string()))?;
                (sink.cursor.into_inner(), "json")
            }
            MappingFormat::Toml => {
                let mut json = Sink {
                    cursor: Cursor::new(Vec::new()),
                    max: mapping_max / 2,
                };
                serde_json::to_writer(&mut json, &mapping).map_err(|e| invalid(e.to_string()))?;
                let bytes = json.cursor.into_inner();
                let payload = std::str::from_utf8(&bytes).map_err(|e| invalid(e.to_string()))?;
                let mut sink = Sink {
                    cursor: Cursor::new(Vec::new()),
                    max: mapping_max,
                };
                sink.write_all(b"fontpixel_image = 1\npayload_json = ")?;
                serde_json::to_writer(&mut sink, payload).map_err(|e| invalid(e.to_string()))?;
                sink.write_all(b"\n")?;
                (sink.cursor.into_inner(), "toml")
            }
        };
        // Serialization has finished: release cloned metadata before encoding.
        drop(mapping);
        add(&mut used, mapping_bytes.len(), l.max_total_bytes)?;
        let mut sink = Sink {
            cursor: Cursor::new(Vec::new()),
            max: l
                .max_total_bytes
                .saturating_sub(used)
                .min(l.max_input_bytes),
        };
        if enc == Encoding::Pbm {
            write!(sink, "P4\n{w} {h}\n")?;
            let stride = (w as usize).div_ceil(8);
            let mut row = vec![0u8; stride];
            for y in 0..h {
                row.fill(0);
                for x in 0..w {
                    // PBM 1 is black; native black is background.
                    if atlas.get_pixel(x, y)[0] == 0 {
                        row[x as usize / 8] |= 128 >> (x % 8);
                    }
                }
                sink.write_all(&row)?;
            }
            drop(atlas);
        } else if enc == Encoding::Tga {
            // image's default RLE can span scanlines, which Pillow rejects.
            // Uncompressed grayscale is deterministic and avoids an RGBA copy.
            image_codec::codecs::tga::TgaEncoder::new(&mut sink)
                .disable_rle()
                .encode(atlas.as_raw(), w, h, image_codec::ExtendedColorType::L8)
                .map_err(|e| invalid(format!("TGA encode: {e}")))?;
            drop(atlas);
        } else if enc == Encoding::Pgm {
            write!(sink, "P5\n{w} {h}\n255\n")?;
            sink.write_all(atlas.as_raw())?;
            drop(atlas);
        } else {
            let image = DynamicImage::ImageLuma8(atlas);
            let image = if matches!(enc, Encoding::Gif | Encoding::Bmp) {
                DynamicImage::ImageRgba8(image.to_rgba8())
            } else {
                image
            };
            image
                .write_to(&mut sink, enc.format())
                .map_err(|e| invalid(format!("image encode: {e}")))?;
        }
        // Verify mapping read limits after encoder buffers and cloned metadata
        // have been released, matching the reader's actual lifetimes.
        let retained = sink
            .cursor
            .get_ref()
            .len()
            .saturating_add(mapping_bytes.len());
        readable.bytes(retained)?;
        parse_map(&mapping_bytes, ext == "toml", l, retained)?;
        let map_name = sidecar(o.primary_name.as_str(), ext)?;
        let output = OutputBundle::new(
            o.primary_name.clone(),
            vec![
                OutputAsset {
                    path: o.primary_name.clone(),
                    bytes: sink.cursor.into_inner(),
                },
                OutputAsset {
                    path: map_name,
                    bytes: mapping_bytes,
                },
            ],
        )?;
        Ok(Written { output, report })
    }
}
