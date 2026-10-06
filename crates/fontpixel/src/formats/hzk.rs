//! Explicit-geometry HZK/ASC raw fonts; filenames provide no structural evidence.
mod presets;
use super::w42_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Mono};
use crate::model::{Codepoint, Font, FontCharset, FontSet, Glyph};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
pub use presets::Preset;
/// Slot numbering independent of bitmap storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CodeSpace {
    /// GB2312 EUC bytes, 94 positions per zone, including unassigned slots.
    Gb2312,
    /// Single-byte ASC slots. Values above 127 are retained without interpretation.
    Ascii,
}
/// Byte-padded raster orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Storage {
    /// Top-to-bottom rows, MSB at the left.
    Rows,
    /// Left-to-right columns, MSB at the top (matrix transpose, no reflection).
    Columns,
}
/// Complete interpretation of a headerless file; no field is inferred from its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Geometry {
    /// Logical bitmap width, excluding low padding bits.
    pub width: u16,
    /// Logical bitmap height.
    pub height: u16,
    /// First raw slot code (e.g. 0xa1a1 or 0xb0a1).
    pub first_code: u16,
    /// Final permitted raw slot code, inclusive.
    pub last_code: u16,
    /// GB2312 grid or ASC linear slots.
    pub code_space: CodeSpace,
    /// Row-major or transposed column-major bytes.
    pub storage: Storage,
}
impl Geometry {
    fn index(self, cp: u32) -> Option<usize> {
        match self.code_space {
            CodeSpace::Ascii => (cp <= 255).then_some(cp as usize),
            CodeSpace::Gb2312 => u16::try_from(cp)
                .ok()
                .and_then(crate::charset::euc_to_kuten)
                .map(|(r, c)| usize::from(r - 1) * 94 + usize::from(c - 1)),
        }
    }
    fn bounds(self) -> Result<(usize, usize)> {
        if self.width == 0 || self.height == 0 {
            return Err(invalid("HZK width and height must be nonzero"));
        }
        let a = self
            .index(u32::from(self.first_code))
            .ok_or_else(|| invalid("Invalid HZK first code"))?;
        let z = self
            .index(u32::from(self.last_code))
            .ok_or_else(|| invalid("Invalid HZK last code"))?;
        if a > z {
            return Err(invalid("Reversed HZK code range"));
        }
        Ok((a, z))
    }
    fn code(self, index: usize) -> u32 {
        match self.code_space {
            CodeSpace::Ascii => index as u32,
            CodeSpace::Gb2312 => (((index / 94 + 0xa1) as u32) << 8) | (index % 94 + 0xa1) as u32,
        }
    }
    fn wire(self) -> (u32, u32) {
        match self.storage {
            Storage::Rows => (u32::from(self.width), u32::from(self.height)),
            Storage::Columns => (u32::from(self.height), u32::from(self.width)),
        }
    }
    fn size(self) -> usize {
        let (w, h) = self.wire();
        (w as usize).div_ceil(8) * h as usize
    }
    fn charset(self) -> &'static str {
        match self.code_space {
            CodeSpace::Ascii => "ascii",
            CodeSpace::Gb2312 => "gb2312",
        }
    }
}
/// Explicit HZK/ASC read configuration. Defaults intentionally require a preset.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct HzkReadOptions {
    /// Conventional named layout.
    pub preset: Option<Preset>,
    /// Complete geometry, overriding the preset when present.
    pub geometry: Option<Geometry>,
    /// Require an exact slot count; otherwise infer count from whole glyph records.
    pub glyph_count: Option<usize>,
}
/// Deterministic raw-font output configuration.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct HzkWriteOptions {
    /// Conventional named layout.
    pub preset: Option<Preset>,
    /// Complete geometry, overriding the preset when present.
    pub geometry: Option<Geometry>,
    /// Explicit output slots; otherwise stop at the last mapped slot.
    pub glyph_count: Option<usize>,
    /// Explicit threshold for grayscale input.
    pub gray_threshold: Option<u8>,
    /// Readback-compatible resource limits.
    pub limits: ResourceLimits,
    /// Maximum output size.
    pub max_output_bytes: usize,
}
impl Default for HzkWriteOptions {
    fn default() -> Self {
        Self {
            preset: None,
            geometry: None,
            glyph_count: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec. The registry deliberately does not install a content detector.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct HzkCodec {
    /// Explicit input interpretation.
    pub read_options: HzkReadOptions,
    /// Explicit output interpretation.
    pub write_options: HzkWriteOptions,
}
impl FormatDetector for HzkCodec {
    fn detect(&self, _: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(Confidence::NoMatch)
    }
}
impl FormatReader<FontSet> for HzkCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (f, mut r) = parse(
            c.bytes(),
            &self.read_options,
            c.limits(),
            c.retained_input_bytes(),
            c.name().map_or(0, str::len),
        )?;
        for e in &mut r.entries {
            e.location.asset = c.name().map(str::to_owned);
        }
        let mut set = FontSet::from(f);
        set.source_format = Some("hzk".into());
        Ok((set, r))
    }
}
impl FormatWriter<FontSet> for HzkCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        let mut d = Diagnostics::new("hzk", &self.write_options.limits);
        container(set, &mut d)?;
        let mut options = self.write_options.clone();
        options.limits.max_total_bytes = options.limits.max_total_bytes.saturating_sub(d.cost());
        let (bytes, r) = match write(set.single()?, &options) {
            Ok(out) => out,
            Err(crate::Error::ReportedErrors(report)) => {
                d.report.extend(report);
                return Err(crate::Error::ReportedErrors(d.report));
            }
            Err(e) => return Err(e),
        };
        d.report.extend(r);
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report: d.report,
        })
    }
}
/// Headerless data has no content signature, including when a filename is known.
pub fn detect(_: &[u8]) -> Confidence {
    Confidence::NoMatch
}
fn geometry(preset: Option<Preset>, explicit: Option<Geometry>) -> Result<Geometry> {
    explicit
        .or_else(|| preset.map(Preset::geometry))
        .ok_or_else(|| invalid("HZK/ASC requires an explicit preset or complete geometry"))
}
fn base(g: Geometry) -> Font {
    let mut f = Font::default();
    f.source_format = Some("hzk".into());
    f.charset = FontCharset {
        name: Some(g.charset().into()),
        gl: if g.code_space == CodeSpace::Gb2312 {
            Some(false)
        } else {
            None
        },
        ..Default::default()
    };
    f.metrics = cell_font_metrics(u32::from(g.width), u32::from(g.height));
    f
}
/// Decode whole fixed-size records with explicit placement and orientation.
pub fn read(b: &[u8], o: &HzkReadOptions, l: &ResourceLimits) -> Result<(Font, Report)> {
    check(b.len(), l.max_input_bytes, "HZK input")?;
    parse(b, o, l, b.len(), 0)
}
fn parse(
    b: &[u8],
    o: &HzkReadOptions,
    l: &ResourceLimits,
    retained: usize,
    asset_len: usize,
) -> Result<(Font, Report)> {
    let g = geometry(o.preset, o.geometry)?;
    let (first, last) = g.bounds()?;
    let size = g.size();
    check(
        usize::from(g.width) * usize::from(g.height),
        l.max_pixels_per_glyph,
        "HZK glyph pixels",
    )?;
    if b.is_empty() || !b.len().is_multiple_of(size) {
        return Err(invalid(
            "HZK input must contain whole, nonempty glyph records",
        ));
    }
    let count = b.len() / size;
    if count > last - first + 1 || o.glyph_count.is_some_and(|n| n != count) {
        return Err(invalid(
            "HZK slot count does not match its declared code range or glyph_count",
        ));
    }
    let mut budget = Budget::new(retained, count, l)?;
    let decoded = usize::from(g.width).div_ceil(8) * usize::from(g.height);
    budget.add(
        count
            .saturating_mul(decoded)
            .saturating_mul(3)
            .saturating_add(size)
            .saturating_add(1024),
    )?;
    let mut f = base(g);
    let mut d = Diagnostics::new("hzk", l);
    let (w, h) = g.wire();
    for (index, raw) in b.chunks_exact(size).enumerate() {
        let cp = g.code(first + index);
        let bm = match g.storage {
            Storage::Rows => Bitmap::<Mono>::from_raw(
                u32::from(g.width),
                u32::from(g.height),
                raw,
                bitmap_limits(l),
            )?,
            Storage::Columns => Bitmap::<Mono>::from_raw(
                u32::from(g.width),
                u32::from(g.height),
                &transpose(raw, w, h),
                bitmap_limits(l),
            )?,
        };
        if padding_nonzero(raw, w, h) {
            d.loss(
                "read.padding",
                Some(index),
                Some(cp),
                "Nonzero padding bits cleared",
            )?;
        }
        let mut glyph = Glyph::new(bm);
        glyph.metrics = cell_metrics(u32::from(g.width), u32::from(g.height));
        glyph.codepoints.push(Codepoint::Legacy(cp));
        f.add_glyph(glyph)?;
    }
    budget.add(
        d.cost()
            .saturating_add(d.report.entries.len().saturating_mul(asset_len)),
    )?;
    Ok((f, d.report))
}
/// Encode GB2312/ASC slots, reporting every out-of-range or unmappable source code.
pub fn write(f: &Font, o: &HzkWriteOptions) -> Result<(Vec<u8>, Report)> {
    f.validate(&o.limits)?;
    let g = geometry(o.preset, o.geometry)?;
    let (first, last) = g.bounds()?;
    check(
        usize::from(g.width) * usize::from(g.height),
        o.limits.max_pixels_per_glyph,
        "HZK glyph pixels",
    )?;
    let mut d = Diagnostics::for_font("hzk", f, &o.limits)?;
    let max_count = o.glyph_count.unwrap_or(last - first + 1);
    if max_count == 0 || max_count > last - first + 1 {
        return Err(d.fatal("HZK glyph_count is outside the configured grid"));
    }
    let map = mappings(
        f,
        g.charset(),
        |v| {
            g.index(v)
                .is_some_and(|i| i >= first && i - first < max_count)
        },
        &mut d,
    )?;
    if map.is_empty() {
        return Err(d.fatal("HZK has no representable mappings in its declared repertoire"));
    }
    let end = map
        .last_key_value()
        .and_then(|(&cp, _)| g.index(cp))
        .ok_or_else(|| invalid("HZK mapping outside grid"))?;
    let count = o.glyph_count.unwrap_or(end - first + 1);
    let size = g.size();
    let total = count
        .checked_mul(size)
        .ok_or_else(|| invalid("HZK output size overflow"))?;
    check(
        total,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "HZK output bytes",
    )?;
    let decoded = usize::from(g.width).div_ceil(8) * usize::from(g.height);
    let mut budget = Budget::new(total, count, &o.limits)?;
    budget.add(
        count
            .saturating_mul(decoded)
            .saturating_mul(3)
            .saturating_add(size)
            .saturating_add(1024),
    )?;
    d.reserve(
        total
            .saturating_mul(2)
            .saturating_add(map.len().saturating_mul(128))
            .saturating_add(usize::from(g.width) * usize::from(g.height))
            .saturating_add(2048),
    )?;
    let mut out = vec![0; total];
    let mut used = std::collections::BTreeSet::new();
    let mut order = Vec::new();
    let mut rejected = false;
    let metrics = cell_metrics(u32::from(g.width), u32::from(g.height));
    for (&cp, &id) in &map {
        let source = f
            .glyph(id)
            .ok_or_else(|| invalid("HZK stale glyph mapping"))?;
        if used.insert(id) {
            glyph_metadata(source, &metrics, id.index(), &mut d)?;
            order.push(id.index());
            if source.codepoints.len() != 1 {
                d.loss(
                    "write.aliases",
                    Some(id.index()),
                    Some(cp),
                    "Aliases become separate raw glyph slots",
                )?;
            }
        }
        let bm = match mono(
            source,
            Some(metrics.bounds),
            o.gray_threshold,
            id.index(),
            &mut d,
        ) {
            Ok(bitmap) => bitmap,
            Err(error @ Error::ResourceLimit { .. }) => return Err(error),
            Err(error) => {
                d.loss(
                    "write.omitted",
                    Some(id.index()),
                    Some(cp),
                    error.to_string(),
                )?;
                rejected = true;
                continue;
            }
        };
        let index = g
            .index(cp)
            .ok_or_else(|| invalid("HZK mapping outside grid"))?
            - first;
        let target = &mut out[index * size..(index + 1) * size];
        match g.storage {
            Storage::Rows => target.copy_from_slice(bm.as_bytes()),
            Storage::Columns => target.copy_from_slice(&transpose(
                bm.as_bytes(),
                u32::from(g.width),
                u32::from(g.height),
            )),
        }
    }
    if rejected {
        return Err(d.fatal("HZK glyph raster conversion failed; output was not emitted"));
    }
    if map.len() != count {
        d.loss(
            "write.blank_slots",
            None,
            None,
            format!("Inserted {} blank glyph slots", count - map.len()),
        )?;
    }
    let source_order = f
        .glyphs()
        .filter(|(id, _)| used.contains(id))
        .map(|(id, _)| id.index())
        .collect::<Vec<_>>();
    if order != source_order {
        d.loss(
            "write.glyph_order",
            None,
            None,
            "Glyphs reordered to raw code-slot order",
        )?;
    }
    metadata(f, &base(g), &mut d)?;

    Ok((out, d.report))
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&CodeSpace::Gb2312), "gb2312");
        assert_eq!(round_trip(&CodeSpace::Ascii), "ascii");
        rejects::<CodeSpace>(r#""not_a_variant""#);
        assert_eq!(round_trip(&Storage::Rows), "rows");
        assert_eq!(round_trip(&Storage::Columns), "columns");
        rejects::<Storage>(r#""not_a_variant""#);
        defaults::<HzkReadOptions>();
        defaults::<HzkWriteOptions>();
        defaults::<HzkCodec>();
    }

    #[test]
    fn preset_names_match_from_str_and_geometry_is_complete() {
        for preset in Preset::ALL {
            assert_eq!(round_trip(preset), preset.name());
        }
        rejects::<Preset>(r#""hzk16_f""#);
        let geometry = Preset::Hzk24.geometry();
        let json = round_trip(&geometry);
        assert_eq!(json["code_space"], "gb2312");
        assert_eq!(json["storage"], "columns");
        rejects::<Geometry>(r#"{"width":16,"height":16}"#);
        rejects::<Geometry>(
            r#"{"width":-1,"height":16,"first_code":0,"last_code":1,"code_space":"ascii","storage":"rows"}"#,
        );
        let options: HzkReadOptions = serde_json::from_str(r#"{"preset":"hzk16f"}"#).unwrap();
        assert_eq!(options.preset, Some(Preset::Hzk16F));
    }
}
