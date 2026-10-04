//! Apple Japanese fbit/HFNT resources and native Macintosh envelopes.
use super::nfnt::{self, NfntWriteOptions};
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions,
    w5d_common::*,
};
use crate::bitmap::{Bounds, Mono};
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, Glyph, GlyphBitmap, Property, PropertyValue,
    Spacing, Vector,
};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Bitmap, FontSet, Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Native record kind. HFNT has no magic and needs explicit selection for bare input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResourceKind {
    /// Resolvable Kanji bitmap resource.
    #[default]
    Fbit,
    /// Legacy HFNT record with native u16 code table.
    Hfnt,
}
/// Supported native font-definition storage layouts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Definition {
    /// fdef 0, inline bitmap bytes.
    #[default]
    Inline,
    /// fdef 5, bitmaps at data-fork offset zero.
    DataFork5,
    /// fdef 7, bitmaps at the extension's explicit offset.
    DataFork7,
}
/// Explicit record and external data-fork interpretation.
#[derive(Debug, Clone, Default)]
pub struct FbitReadOptions {
    /// Kind of a bare resource; envelope resource types are authoritative.
    pub kind: ResourceKind,
    /// Explicit companion for a bare fdef 5/7 resource.
    pub data_fork: Option<ResourcePath>,
    /// AppleDouble resource companion when the primary asset is its data fork.
    pub apple_double_resource: Option<ResourcePath>,
    /// Explicit HFNT cell width when the empirical square-root rule is unsuitable.
    pub hfnt_width: Option<u16>,
}
/// Bounded native output policy.
#[derive(Debug, Clone)]
pub struct FbitWriteOptions {
    /// Resource kind for newly generated output.
    pub kind: ResourceKind,
    /// Outer native Macintosh envelope; Bare is the default.
    pub container: nfnt::Container,
    /// fbit bitmap storage. HFNT always stores bytes inline.
    pub definition: Definition,
    /// Companion name for bare split output; default is primary name plus `.data`.
    pub data_fork: Option<ResourcePath>,
    /// Required explicit Gray8 reduction.
    pub gray_threshold: Option<u8>,
    /// Cumulative resource and allocation limits.
    pub limits: ResourceLimits,
    /// Maximum bytes across the output bundle.
    pub max_output_bytes: usize,
}
impl Default for FbitWriteOptions {
    fn default() -> Self {
        Self {
            kind: ResourceKind::Fbit,
            container: nfnt::Container::Bare,
            definition: Definition::Inline,
            data_fork: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 << 20,
        }
    }
}
/// Registry adapter for fbit and explicitly selected HFNT.
#[derive(Debug, Clone, Default)]
pub struct FbitCodec {
    /// Native input policy.
    pub read_options: FbitReadOptions,
    /// Native output policy.
    pub write_options: FbitWriteOptions,
}
impl FormatDetector for FbitCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for FbitCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read_context(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for FbitCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write_bundle(s, &self.write_options, &o.primary_name)
    }
}
/// Identify fbit's explicit resource tag. Headerless HFNT requires explicit input.
pub fn detect(data: &[u8]) -> Confidence {
    if !nfnt::bridge::envelope_signature(data)
        && data.get(2..6) == Some(b"fbit")
        && data.len() >= 38
    {
        return Confidence::Certain;
    }
    let limits = ResourceLimits {
        max_input_bytes: 1 << 20,
        max_resource_bytes: 1 << 20,
        max_table_bytes: 1 << 20,
        max_total_bytes: 8 << 20,
        max_resources: 256,
        ..Default::default()
    };
    if data.len() > limits.max_input_bytes {
        return Confidence::NoMatch;
    }
    let mut used = data.len();
    if let Ok((set, _)) = nfnt::bridge::resources(data, &limits, &mut used)
        && set.properties.iter().any(|p| p.name == "nfnt.resource" && matches!(&p.value, PropertyValue::Bytes(raw) if raw.starts_with(b"fbit") || raw.starts_with(b"HFNT"))) {
        return Confidence::Possible;
    }
    Confidence::NoMatch
}
/// Decode a standalone record or native Macintosh envelope.
pub fn read(data: &[u8], o: &FbitReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    let input = crate::resources::Input::new(data);
    let mut c = ReadContext::new(&input, l)?;
    read_context(&mut c, o)
}
/// Decode with an explicitly scoped data-fork companion.
pub fn read_context(c: &mut ReadContext<'_>, o: &FbitReadOptions) -> Result<(FontSet, Report)> {
    if let Some(path) = &o.apple_double_resource {
        if o.data_fork.is_some() {
            return Err(invalid(
                "AppleDouble primary data fork conflicts with explicit data-fork option",
            ));
        }
        let resource = c.resource(path)?;
        if !resource.starts_with(&[0, 5, 0x16, 7]) {
            return Err(invalid("fbit companion is not AppleDouble"));
        }
        let mut b = Budget::new(c.limits());
        b.add(c.retained_input_bytes())?;
        return decode(&resource, Some(c.bytes()), o, &mut b);
    }
    let external = if let Some(path) = &o.data_fork {
        Some(c.resource(path)?)
    } else {
        None
    };
    let mut b = Budget::new(c.limits());
    b.add(c.retained_input_bytes())?;
    decode(c.bytes(), external.as_deref(), o, &mut b)
}
fn record(raw: &[u8]) -> Result<([u8; 4], &[u8], &[u8])> {
    let n = u16be(raw, 11)?;
    let n = if n == 65535 { 0 } else { usize::from(n) };
    let meta = slice(raw, 0, 13 + n)?;
    Ok((
        raw[..4].try_into().expect("metadata checked"),
        meta,
        &raw[13 + n..],
    ))
}
fn decode(
    data: &[u8],
    external: Option<&[u8]>,
    o: &FbitReadOptions,
    b: &mut Budget<'_>,
) -> Result<(FontSet, Report)> {
    if !nfnt::bridge::envelope_signature(data)
        && (data.get(2..6) == Some(b"fbit") || o.kind == ResourceKind::Hfnt)
    {
        let f = decode_record(data, external, o.kind, o, b, true)?;
        let mut s = FontSet::from(f);
        s.source_format = Some("fbit".into());
        return Ok((s, Report::default()));
    }
    let (mut native, r) = nfnt::bridge::resources(data, b.limits, &mut b.used)?;
    let datafork = external.or_else(|| bytes(&native.properties, "nfnt.data-fork"));
    let mut fonts = Vec::new();
    let mut glyphs = 0usize;
    for p in native
        .properties
        .iter()
        .filter(|p| p.name == "nfnt.resource")
    {
        let PropertyValue::Bytes(raw) = &p.value else {
            return Err(invalid("fbit retained resource is not bytes"));
        };
        let (kind, meta, payload) = record(raw)?;
        let kind = match &kind {
            b"fbit" => ResourceKind::Fbit,
            b"HFNT" => ResourceKind::Hfnt,
            _ => continue,
        };
        let mut f = decode_record(payload, datafork, kind, o, b, false)?;
        glyphs = glyphs
            .checked_add(f.len())
            .ok_or_else(|| invalid("fbit glyph count overflow"))?;
        check(glyphs, b.limits.max_glyphs, "fbit collection glyphs")?;
        b.table(meta.len())?;
        f.properties.push(Property::new(
            "fbit.resource",
            PropertyValue::Bytes(meta.to_vec()),
        ));
        fonts.push(f);
    }
    if fonts.is_empty() {
        return Err(invalid("resource fork has no fbit/HFNT fonts"));
    }
    native.properties = std::mem::take(&mut native.properties).into_vec().into_iter().filter(|p|p.name!="nfnt.resource"|| !matches!(&p.value,PropertyValue::Bytes(raw) if raw.starts_with(b"fbit")||raw.starts_with(b"HFNT"))).collect();
    if let Some(external) = external {
        if let Some(stored) = bytes(&native.properties, "nfnt.data-fork") {
            if stored != external {
                return Err(invalid(
                    "explicit data fork conflicts with embedded data fork",
                ));
            }
        } else {
            b.table(external.len())?;
            native.properties.push(Property::new(
                "nfnt.data-fork",
                PropertyValue::Bytes(external.to_vec()),
            ));
        }
    }
    native.fonts = fonts;
    native.source_format = Some("fbit".into());
    Ok((native, r))
}
fn ordinal(cp: u32) -> Option<u16> {
    let lead = cp >> 8;
    let trail = cp & 255;
    let page = match lead {
        0x81..=0x9f => lead - 0x81,
        0xe0..=0xfc => lead - 0xe0 + 31,
        _ => return None,
    };
    let pos = match trail {
        0x40..=0x7e => trail - 0x40,
        0x80..=0xfc => trail - 0x41,
        _ => return None,
    };
    Some((page * 188 + pos) as u16)
}
fn code(ord: u16) -> Result<u32> {
    let page = u32::from(ord) / 188;
    let pos = u32::from(ord) % 188;
    if page >= 60 {
        return Err(invalid("fbit ordinal outside Shift-JIS pages"));
    }
    Ok(
        ((page + 0x81 + if page >= 31 { 64 } else { 0 }) << 8)
            + (pos + 0x40 + u32::from(pos >= 63)),
    )
}
fn decode_record(
    data: &[u8],
    external: Option<&[u8]>,
    kind: ResourceKind,
    o: &FbitReadOptions,
    b: &mut Budget<'_>,
    retain_external: bool,
) -> Result<Font> {
    check(
        data.len(),
        b.limits.max_resource_bytes,
        "fbit native resource",
    )?;
    b.table(data.len())?;
    b.add(4096)?;
    let (w, h, cell, codes, raw, continuous) = match kind {
        ResourceKind::Fbit => {
            if slice(data, 2, 4)? != b"fbit" {
                return Err(invalid("fbit resource tag mismatch"));
            }
            let definition = u16be(data, 10)?;
            let extension = match definition {
                0 => 0,
                5 => 78,
                7 => 64,
                _ => return Err(invalid("unsupported fbit definition ID")),
            };
            slice(data, 0, 36 + extension)?;
            let h = usize::from(u16be(data, 24)?);
            let w = usize::from(u16be(data, 26)?);
            if w == 0 || h == 0 {
                return Err(invalid("fbit cell dimensions must be nonzero"));
            }
            let cell = w
                .checked_mul(h)
                .ok_or_else(|| invalid("fbit cell overflow"))?
                .div_ceil(8);
            let runs = usize::from(u16be(data, 36 + extension)?);
            check(runs, b.limits.max_resources, "fbit runs")?;
            let end = 38 + extension + runs * 6;
            slice(data, 0, end)?;
            let mut codes = Vec::new();
            let mut prior = None;
            for i in 0..runs {
                let at = 38 + extension + i * 6;
                let first = u16be(data, at)?;
                let last = u16be(data, at + 2)?;
                let n = usize::from(u16be(data, at + 4)?);
                if first > last
                    || n != usize::from(last - first) + 1
                    || prior.is_some_and(|p| p >= first)
                {
                    return Err(invalid("fbit overlapping or inconsistent run"));
                }
                prior = Some(last);
                check(codes.len() + n, b.limits.max_glyphs, "fbit glyphs")?;
                b.add(n * 128)?;
                for v in first..=last {
                    codes.push(code(v)?);
                }
            }
            let raw = if definition == 0 {
                &data[end..]
            } else {
                let ext = external.ok_or_else(|| invalid("fbit requires an explicit data fork"))?;
                let offset = if definition == 7 {
                    u32be(data, 52)? as usize
                } else {
                    0
                };
                slice(
                    ext,
                    offset,
                    ext.len()
                        .checked_sub(offset)
                        .ok_or_else(|| invalid("fbit data offset beyond fork"))?,
                )?
            };
            (w, h, cell, codes, raw, true)
        }
        ResourceKind::Hfnt => {
            slice(data, 0, 10)?;
            let cell = usize::from(u16be(data, 6)?) * 2;
            let n = usize::from(u16be(data, 8)?);
            if cell == 0 || n % 2 != 0 {
                return Err(invalid("HFNT cell or encoding table length invalid"));
            }
            let count = n / 2;
            check(count, b.limits.max_glyphs, "HFNT glyphs")?;
            b.add(count * 128)?;
            let mut w = 1usize;
            while (w + 1) * (w + 1) <= cell * 8 {
                w += 1;
            }
            if let Some(width) = o.hfnt_width {
                w = usize::from(width);
            }
            if w == 0 {
                return Err(invalid("HFNT width cannot be zero"));
            }
            let stride = w.div_ceil(8);
            if cell % stride != 0 {
                return Err(invalid(
                    "HFNT cell does not contain complete rows at selected width",
                ));
            }
            let h = cell / stride;
            let mut codes = Vec::with_capacity(count);
            for i in 0..count {
                codes.push(u32::from(u16be(data, 10 + i * 2)?));
            }
            let end = 12 + n;
            slice(data, 0, end)?;
            (w, h, cell, codes, &data[end..], false)
        }
    };
    let count = codes.len();
    if count == 0 {
        return Err(invalid("fbit/HFNT has no glyphs"));
    }
    let length = cell
        .checked_mul(count)
        .ok_or_else(|| invalid("fbit raster length overflow"))?;
    slice(raw, 0, length)?;
    let mut f = Font::default();
    f.source_format = Some("fbit".into());
    f.charset = FontCharset {
        name: Some("mac-japanese".into()),
        ..Default::default()
    };
    f.metrics = FontMetrics {
        pixel_size: Some(h as u32),
        ascent: Some(h as i64),
        descent: Some(0),
        bounds: Some(Bounds::new(w as u32, h as u32, 0, 0)),
        spacing: Some(Spacing::Monospaced),
        ..Default::default()
    };
    f.properties.push(Property::new(
        "fbit.source",
        PropertyValue::Bytes(data.to_vec()),
    ));
    f.properties.push(Property::new(
        "fbit.kind",
        if kind == ResourceKind::Fbit {
            "fbit"
        } else {
            "HFNT"
        },
    ));
    if retain_external && kind == ResourceKind::Fbit && u16be(data, 10)? != 0 {
        let ext = external.expect("validated external");
        b.table(ext.len())?;
        f.properties.push(Property::new(
            "fbit.data-fork",
            PropertyValue::Bytes(ext.to_vec()),
        ));
    }
    for (i, cp) in codes.into_iter().enumerate() {
        b.glyph(w, h)?;
        let stride = w.div_ceil(8);
        b.table(stride * h)?;
        let mut packed = vec![0; stride * h];
        let native = &raw[i * cell..(i + 1) * cell];
        if continuous {
            for y in 0..h {
                for x in 0..w {
                    let at = y * w + x;
                    if native[at / 8] & (0x80 >> (at % 8)) != 0 {
                        packed[y * stride + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
        } else {
            packed.copy_from_slice(native);
        }
        let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
            w as u32,
            h as u32,
            &packed,
            bitmap_limits(b.limits),
        )?);
        g.codepoints.push(Codepoint::Legacy(cp));
        g.metrics.advance = Some(Vector::new(w as i64, 0));
        f.add_glyph(g)?;
    }
    Ok(f)
}
/// Write a single asset. Split bare resources require [`write_bundle`].
pub fn write(set: &FontSet, o: &FbitWriteOptions) -> Result<(Vec<u8>, Report)> {
    let w = write_bundle(set, o, &ResourcePath::new("font.fbit")?)?;
    Ok((w.output.into_single()?, w.report))
}
/// Emit all native forks and verify the output with the same cumulative limits.
pub fn write_bundle(set: &FontSet, o: &FbitWriteOptions, path: &ResourcePath) -> Result<Written> {
    let mut b = Budget::new(&o.limits);
    b.model(set)?;
    b.table(path.as_str().len())?;
    if set.fonts.is_empty() {
        return Err(invalid("fbit collection is empty"));
    }
    check(set.fonts.len(), o.limits.max_resources, "fbit font count")?;
    if o.kind == ResourceKind::Hfnt && o.definition != Definition::Inline {
        return Err(invalid("HFNT does not have external fdef storage"));
    }
    let mut records = Vec::new();
    let data = bytes(&set.properties, "nfnt.data-fork").unwrap_or(&[]);
    b.table(data.len())?;
    let mut combined = data.to_vec();
    if o.definition != Definition::Inline && set.fonts.len() != 1 {
        return Err(invalid(
            "split fbit output requires one strike per data fork",
        ));
    }
    for f in &set.fonts {
        let (raw, external) =
            encode_record(f, o, &mut b, bytes(&set.properties, "nfnt.data-fork"))?;
        if let Some(ext) = external {
            combined = ext;
        }
        records.push(raw);
    }
    let output = if o.container == nfnt::Container::Bare {
        set.single()?;
        let primary = records.pop().expect("one record");
        if o.definition == Definition::Inline {
            OutputBundle::single(path.clone(), primary)
        } else {
            let companion = if let Some(p) = &o.data_fork {
                p.clone()
            } else {
                ResourcePath::new(format!("{}.data", path.as_str()))?
            };
            OutputBundle::new(
                path.clone(),
                vec![
                    OutputAsset {
                        path: path.clone(),
                        bytes: primary,
                    },
                    OutputAsset {
                        path: companion,
                        bytes: combined,
                    },
                ],
            )?
        }
    } else {
        b.model(set)?;
        let mut native = FontSet {
            properties: set.properties.clone(),
            ..Default::default()
        };
        native.properties.remove("nfnt.data-fork");
        native.properties.push(Property::new(
            "nfnt.data-fork",
            PropertyValue::Bytes(combined),
        ));
        for (i, (f, payload)) in set.fonts.iter().zip(records).enumerate() {
            let mut raw = if let Some(m) = bytes(&f.properties, "fbit.resource") {
                record(m)?;
                m.to_vec()
            } else {
                let mut m = vec![0; 13];
                m[..4].copy_from_slice(if o.kind == ResourceKind::Fbit {
                    b"fbit"
                } else {
                    b"HFNT"
                });
                let id =
                    u16::try_from(i + 128).map_err(|_| invalid("fbit resource ID overflow"))?;
                m[4..6].copy_from_slice(&id.to_be_bytes());
                m[11..13].copy_from_slice(&u16::MAX.to_be_bytes());
                m
            };
            raw[..4].copy_from_slice(if o.kind == ResourceKind::Fbit {
                b"fbit"
            } else {
                b"HFNT"
            });
            b.table(raw.len() + payload.len())?;
            raw.extend_from_slice(&payload);
            native
                .properties
                .push(Property::new("nfnt.resource", PropertyValue::Bytes(raw)));
        }
        nfnt::bridge::wrap_resources(
            &native,
            &NfntWriteOptions {
                container: o.container,
                limits: o.limits.clone(),
                max_output_bytes: o.max_output_bytes,
                ..Default::default()
            },
            path,
            &mut b.used,
        )?
    };
    let total = output.assets().iter().try_fold(0usize, |n, a| {
        n.checked_add(a.bytes.len())
            .ok_or_else(|| invalid("fbit output bundle overflow"))
    })?;
    check(total, o.max_output_bytes, "fbit output bytes")?;
    for a in output.assets() {
        check(
            a.bytes.len(),
            o.limits.max_input_bytes,
            "fbit readable asset",
        )?;
    }
    let primary = output
        .assets()
        .iter()
        .find(|a| a.path == *path)
        .ok_or_else(|| invalid("fbit missing primary"))?;
    let external = if o.container == nfnt::Container::Bare {
        output
            .assets()
            .iter()
            .find(|a| a.path != *path)
            .map(|a| a.bytes.as_slice())
    } else {
        None
    };
    let (decoded, mut report) = if o.container == nfnt::Container::AppleDouble {
        let resource = output
            .assets()
            .iter()
            .find(|a| a.path != *path)
            .ok_or_else(|| invalid("fbit AppleDouble companion missing"))?;
        decode(
            &resource.bytes,
            Some(&primary.bytes),
            &FbitReadOptions::default(),
            &mut b,
        )?
    } else {
        decode(
            &primary.bytes,
            external,
            &FbitReadOptions {
                kind: if o.container == nfnt::Container::Bare {
                    o.kind
                } else {
                    ResourceKind::Fbit
                },
                ..Default::default()
            },
            &mut b,
        )?
    };
    compare(set, &decoded, "fbit", &mut b, &mut report)?;
    Ok(Written { output, report })
}
fn encode_record(
    f: &Font,
    o: &FbitWriteOptions,
    b: &mut Budget<'_>,
    envelope_data: Option<&[u8]>,
) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
    let def = match o.definition {
        Definition::Inline => 0u16,
        Definition::DataFork5 => 5,
        Definition::DataFork7 => 7,
    };
    if let Some(raw) = bytes(&f.properties, "fbit.source") {
        let native_kind = match f.properties.get("fbit.kind") {
            Some(PropertyValue::Text(kind)) if kind == "HFNT" => ResourceKind::Hfnt,
            Some(PropertyValue::Text(kind)) if kind == "fbit" => ResourceKind::Fbit,
            _ => {
                return Err(invalid(
                    "fbit source requires its original native resource kind",
                ));
            }
        };
        let external = bytes(&f.properties, "fbit.data-fork").or(envelope_data);
        let mut original = decode_record(
            raw,
            external,
            native_kind,
            &FbitReadOptions::default(),
            b,
            bytes(&f.properties, "fbit.data-fork").is_some(),
        )?;
        if let Some(meta) = bytes(&f.properties, "fbit.resource") {
            b.table(meta.len())?;
            original.properties.push(Property::new(
                "fbit.resource",
                PropertyValue::Bytes(meta.to_vec()),
            ));
        }
        if native_kind == o.kind
            && (o.kind == ResourceKind::Hfnt || u16be(raw, 10)? == def)
            && *f == original
            && o.gray_threshold.is_none()
        {
            b.table(raw.len())?;
            if let Some(e) = external {
                b.table(e.len())?;
            }
            return Ok((raw.to_vec(), external.map(<[u8]>::to_vec)));
        }
    }
    let mut mapping = BTreeMap::new();
    let mut width = 0usize;
    let mut height = 0usize;
    for (_, g) in f.glyphs() {
        for cp in &g.codepoints {
            if let Codepoint::Legacy(cp) = *cp {
                let key = if o.kind == ResourceKind::Fbit {
                    ordinal(cp)
                } else {
                    u16::try_from(cp).ok()
                };
                if let Some(key) = key {
                    b.add(128)?;
                    mapping.insert(key, g);
                }
            }
        }
        width = width.max(g.bitmap.width() as usize);
        height = height.max(g.bitmap.height() as usize);
        if matches!(g.bitmap, GlyphBitmap::Gray8(_)) && o.gray_threshold.is_none() {
            return Err(invalid("fbit Gray8 reduction requires threshold"));
        }
    }
    if mapping.is_empty() || width == 0 || height == 0 {
        return Err(invalid("fbit needs mapped nonempty cells"));
    }
    check(mapping.len(), b.limits.max_glyphs, "fbit output glyphs")?;
    if o.kind == ResourceKind::Hfnt {
        width = width.max(height).div_ceil(8) * 8;
        height = width;
    }
    let cell = width
        .checked_mul(height)
        .ok_or_else(|| invalid("fbit cell overflow"))?
        .div_ceil(8);
    b.glyph(width, height)?;
    let mut runs = Vec::<(u16, u16)>::new();
    for &cp in mapping.keys() {
        if let Some((_, end)) = runs.last_mut()
            && u32::from(*end) + 1 == u32::from(cp)
        {
            *end = cp;
        } else {
            b.add(32)?;
            runs.push((cp, cp));
        }
    }
    let mut header = match o.kind {
        ResourceKind::Fbit => {
            let ext = match def {
                5 => 78,
                7 => 64,
                _ => 0,
            };
            let mut h = vec![0; 38 + ext + runs.len() * 6];
            if let Some(old) =
                bytes(&f.properties, "fbit.source").filter(|s| s.get(2..6) == Some(b"fbit"))
            {
                h[..36].copy_from_slice(slice(old, 0, 36)?);
                if u16be(old, 10)? == def && ext > 0 {
                    h[36..36 + ext].copy_from_slice(slice(old, 36, ext)?);
                }
            }
            h[2..6].copy_from_slice(b"fbit");
            h[10..12].copy_from_slice(&def.to_be_bytes());
            h[24..26].copy_from_slice(
                &u16::try_from(height)
                    .map_err(|_| invalid("fbit height exceeds u16"))?
                    .to_be_bytes(),
            );
            h[26..28].copy_from_slice(
                &u16::try_from(width)
                    .map_err(|_| invalid("fbit width exceeds u16"))?
                    .to_be_bytes(),
            );
            h[32..34].copy_from_slice(
                &(code(*mapping.first_key_value().expect("mapping").0)? as u16).to_be_bytes(),
            );
            h[34..36].copy_from_slice(
                &(code(*mapping.last_key_value().expect("mapping").0)? as u16).to_be_bytes(),
            );
            h[36 + ext..38 + ext].copy_from_slice(
                &u16::try_from(runs.len())
                    .map_err(|_| invalid("fbit run count exceeds u16"))?
                    .to_be_bytes(),
            );
            for (i, (first, last)) in runs.iter().enumerate() {
                let at = 38 + ext + i * 6;
                h[at..at + 2].copy_from_slice(&first.to_be_bytes());
                h[at + 2..at + 4].copy_from_slice(&last.to_be_bytes());
                h[at + 4..at + 6].copy_from_slice(&(last - first + 1).to_be_bytes());
            }
            h
        }
        ResourceKind::Hfnt => {
            let mut h = vec![0; 12 + mapping.len() * 2];
            if let Some(old) =
                bytes(&f.properties, "fbit.source").filter(|s| s.get(2..6) != Some(b"fbit"))
            {
                h[..6].copy_from_slice(slice(old, 0, 6)?);
            }
            h[6..8].copy_from_slice(
                &u16::try_from(cell / 2)
                    .map_err(|_| invalid("HFNT cell word count exceeds u16"))?
                    .to_be_bytes(),
            );
            h[8..10].copy_from_slice(
                &u16::try_from(mapping.len() * 2)
                    .map_err(|_| invalid("HFNT code table exceeds u16"))?
                    .to_be_bytes(),
            );
            for (i, &cp) in mapping.keys().enumerate() {
                h[10 + i * 2..12 + i * 2].copy_from_slice(&cp.to_be_bytes());
            }
            h
        }
    };
    let raster_len = cell
        .checked_mul(mapping.len())
        .ok_or_else(|| invalid("fbit raster length overflow"))?;
    check(
        raster_len,
        b.limits.max_resource_bytes,
        "fbit output raster",
    )?;
    b.table(raster_len + header.len())?;
    let mut raster = vec![0; raster_len];
    for (i, &g) in mapping.values().enumerate() {
        let gw = g.bitmap.width() as usize;
        let gh = g.bitmap.height() as usize;
        for y in 0..gh {
            for x in 0..gw {
                let on = match &g.bitmap {
                    GlyphBitmap::Mono(m) => {
                        m.as_bytes()[y * m.stride() + x / 8] & (0x80 >> (x % 8)) != 0
                    }
                    GlyphBitmap::Gray8(m) => {
                        m.as_bytes()[y * m.stride() + x]
                            >= o.gray_threshold.expect("validated threshold")
                    }
                };
                if on {
                    let bit = (height - gh + y) * width + x;
                    raster[i * cell + bit / 8] |= 0x80 >> (bit % 8);
                }
            }
        }
    }
    if def == 0 {
        b.table(raster.len())?;
        header.extend_from_slice(&raster);
        Ok((header, None))
    } else {
        let offset = if def == 7 {
            u32be(&header, 52)? as usize
        } else {
            0
        };
        let end = offset
            .checked_add(raster.len())
            .ok_or_else(|| invalid("fbit external offset overflow"))?;
        check(end, b.limits.max_resource_bytes, "fbit data fork")?;
        let original = bytes(&f.properties, "fbit.data-fork")
            .or(envelope_data)
            .unwrap_or(&[]);
        check(
            end.max(original.len()),
            b.limits.max_resource_bytes,
            "fbit complete data fork",
        )?;
        b.table(end.max(original.len()))?;
        let mut external = original.to_vec();
        external.resize(end.max(external.len()), 0);
        external[offset..end].copy_from_slice(&raster);
        Ok((header, Some(external)))
    }
}
