//! Apple Lisa font.lib directories and backwards-compressed bitmap resources.
use super::nfnt::{self, NfntWriteOptions};
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions,
    w5d_common::*,
};
use crate::model::{Font, Property, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Explicit legacy encoding label for bitmap codes.
#[derive(Debug, Clone, Default)]
pub struct LisaReadOptions {
    /// Optional charset label; codes remain native bytes.
    pub charset: Option<String>,
}
/// Native per-resource compression.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Compression {
    /// Preserve native representation; compress newly authored records.
    #[default]
    Preserve,
    /// Store ordinary 9000/B000 bitmap records.
    Uncompressed,
    /// Store 9080/B080 zero-mask and row-XOR records.
    Compressed,
}
/// Bounded Lisa font library output.
#[derive(Debug, Clone)]
pub struct LisaWriteOptions {
    /// Native compression policy.
    pub compression: Compression,
    /// Explicit Gray8-to-Mono threshold.
    pub gray_threshold: Option<u8>,
    /// Cumulative source/model/expansion/output/reread limits.
    pub limits: ResourceLimits,
    /// Maximum output bytes.
    pub max_output_bytes: usize,
}
impl Default for LisaWriteOptions {
    fn default() -> Self {
        Self {
            compression: Compression::Preserve,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 << 20,
        }
    }
}
/// Registry implementation of native Lisa font.lib.
#[derive(Debug, Clone, Default)]
pub struct LisaCodec {
    /// Input interpretation.
    pub read_options: LisaReadOptions,
    /// Native output policy.
    pub write_options: LisaWriteOptions,
}
impl FormatDetector for LisaCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for LisaCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c.bytes(), &self.read_options, c.limits())
    }
}
impl FormatWriter<FontSet> for LisaCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
struct Entry<'a> {
    name: &'a [u8],
    resource: &'a [u8],
    metrics: &'a [u8],
}
fn directory<'a>(data: &'a [u8], b: &mut Budget<'_>) -> Result<Vec<Entry<'a>>> {
    let end = 4 + usize::from(u16be(data, 0)?) * 2;
    let count = usize::from(u16be(data, 2)?);
    check(count, b.limits.max_resources, "Lisa directory entries")?;
    if count == 0 {
        return Err(invalid("Lisa empty library"));
    }
    slice(data, 0, end)?;
    b.add(count * 256)?;
    let mut entries = Vec::with_capacity(count);
    let mut ranges = Vec::with_capacity(count * 2);
    let mut at = 4;
    for _ in 0..count {
        let n = usize::from(*slice(data, at, 1)?.first().expect("one byte"));
        let name = slice(data, at + 1, n)?;
        at = (at + 1 + n + 1) & !1;
        let fields = slice(data, at, 16)?;
        at += 16;
        if at > end {
            return Err(invalid("Lisa directory exceeds declared size"));
        }
        let mut offset = [0usize; 4];
        for (i, v) in offset.iter_mut().enumerate() {
            *v = (u32be(fields, i * 4)? as usize)
                .checked_mul(2)
                .and_then(|n| n.checked_add(end))
                .ok_or_else(|| invalid("Lisa word offset overflow"))?;
        }
        if offset[1] < offset[0] || offset[3] < offset[2] {
            return Err(invalid("Lisa reversed resource range"));
        }
        let resource = slice(data, offset[0], offset[1] - offset[0])?;
        let metrics = slice(data, offset[2], offset[3] - offset[2])?;
        check(
            resource.len(),
            b.limits.max_resource_bytes,
            "Lisa bitmap resource",
        )?;
        check(
            metrics.len(),
            b.limits.max_resource_bytes,
            "Lisa metric resource",
        )?;
        for pair in [[offset[0], offset[1]], [offset[2], offset[3]]] {
            if pair[0] != pair[1] {
                ranges.push(pair);
            }
        }
        entries.push(Entry {
            name,
            resource,
            metrics,
        });
    }
    ranges.sort_unstable();
    ranges.dedup();
    if ranges.windows(2).any(|p| p[0][1] > p[1][0]) {
        return Err(invalid("Lisa partially overlapping resources"));
    }
    Ok(entries)
}
/// Structural detection validates the whole directory and bitmap record tags.
pub fn detect(data: &[u8]) -> Confidence {
    let l = ResourceLimits {
        max_resources: 256,
        max_total_bytes: 1 << 20,
        ..Default::default()
    };
    let mut b = Budget::new(&l);
    if directory(data, &mut b).is_ok_and(|entries| {
        entries
            .iter()
            .any(|e| matches!(u16be(e.resource, 0), Ok(0x9000 | 0xb000 | 0x9080 | 0xb080)))
    }) {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
fn name(raw: &[u8]) -> Result<String> {
    let codec =
        crate::charset::CharsetCodec::new("macroman", None).map_err(|e| invalid(e.to_string()))?;
    raw.iter()
        .map(|v| {
            codec
                .cp2char(u32::from(*v))
                .ok_or_else(|| invalid("Lisa invalid MacRoman name"))
        })
        .collect()
}
fn native_name(text: &str) -> Result<Vec<u8>> {
    let codec =
        crate::charset::CharsetCodec::new("macroman", None).map_err(|e| invalid(e.to_string()))?;
    text.chars()
        .map(|c| {
            codec
                .char2cp(c)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(|| invalid("Lisa name is not MacRoman"))
        })
        .collect()
}
fn retain(f: &mut Font, key: &str, v: &[u8], b: &mut Budget<'_>) -> Result<()> {
    b.table(v.len())?;
    b.add(128)?;
    f.properties
        .push(Property::new(key, PropertyValue::Bytes(v.to_vec())));
    Ok(())
}
// The published Lisa decoder copies the compressed bytes into the output and
// expands backwards. Each write must stay beyond the unread input prefix.
fn compression_safe(payload: &[u8], rounded: usize) -> bool {
    let mut input = payload.len();
    let mut output = rounded;
    if input > output {
        return false;
    }
    while output > 0 {
        let Some(previous) = input.checked_sub(1) else {
            return false;
        };
        input = previous;
        let mask = payload[input];
        for bit in 0..8 {
            if output == 0 {
                break;
            }
            output -= 1;
            if mask & (1 << bit) == 0 {
                let Some(previous) = input.checked_sub(1) else {
                    return false;
                };
                input = previous;
            }
            if output < input {
                return false;
            }
        }
    }
    input == 0
}
fn expanded(data: &[u8], b: &mut Budget<'_>) -> Result<Vec<u8>> {
    let kind = u16be(data, 0)?;
    if !matches!(kind, 0x9000 | 0xb000 | 0x9080 | 0xb080) {
        return Err(invalid("Lisa unsupported bitmap resource type"));
    }
    if kind & 0x80 == 0 {
        b.table(data.len())?;
        return Ok(data.to_vec());
    }
    let compressed = u32be(data, 2)? as usize;
    let length = u32be(data, 6)? as usize;
    let rounded = length
        .checked_add(7)
        .ok_or_else(|| invalid("Lisa expansion length overflow"))?
        & !7;
    check(
        rounded + 2,
        b.limits.max_resource_bytes,
        "Lisa expanded bitmap resource",
    )?;
    b.expansion(rounded + 2)?;
    b.table(rounded + 2)?;
    let payload = slice(data, 10, compressed)?;
    if !compression_safe(payload, rounded) {
        return Err(invalid(
            "Lisa compressed stream cannot expand safely in place",
        ));
    }
    let mut out = vec![0; rounded + 2];
    out[..2].copy_from_slice(&(kind & !0x80).to_be_bytes());
    let mut input = payload.len();
    let mut output = out.len();
    while output > 2 {
        input = input
            .checked_sub(1)
            .ok_or_else(|| invalid("Lisa truncated compression mask"))?;
        let mask = payload[input];
        for bit in 0..8 {
            if output == 2 {
                break;
            }
            output -= 1;
            if mask & (1 << bit) == 0 {
                input = input
                    .checked_sub(1)
                    .ok_or_else(|| invalid("Lisa truncated compression literal"))?;
                out[output] = payload[input];
            }
        }
    }
    if input != 0 {
        return Err(invalid("Lisa trailing unconsumed compressed bytes"));
    }
    out.truncate(length + 2);
    let row = usize::from(u16be(&out, 24)?) * 2;
    let height = usize::from(u16be(&out, 14)?);
    let size = row
        .checked_mul(height)
        .ok_or_else(|| invalid("Lisa raster length overflow"))?;
    slice(&out, 26, size)?;
    for y in 1..height {
        for x in 0..row {
            out[26 + y * row + x] ^= out[26 + (y - 1) * row + x];
        }
    }
    Ok(out)
}
fn decode_font(
    e: &Entry<'_>,
    index: usize,
    o: &LisaReadOptions,
    b: &mut Budget<'_>,
) -> Result<Font> {
    let mut raster = expanded(e.resource, b)?;
    // Lisa has a 16-bit owTLoc, and need not include Macintosh's FFFF terminator.
    if u16be(&raster, 10)? as i16 > 0 {
        raster[10..12].fill(0);
    }
    let first = u16be(&raster, 2)?;
    let last = u16be(&raster, 4)?;
    if first > last || last > 255 {
        return Err(invalid("Lisa invalid byte character range"));
    }
    let count = usize::from(last - first) + 2;
    let ow = 16 + usize::from(u16be(&raster, 16)?) * 2;
    let end = ow
        .checked_add(count * 2)
        .ok_or_else(|| invalid("Lisa width table overflow"))?;
    slice(&raster, ow, count * 2)?;
    if raster.len() < end + 2 {
        b.add(end + 2 - raster.len())?;
        raster.resize(end + 2, 0);
    }
    raster[end..end + 2].copy_from_slice(&[255, 255]);
    let mut f = nfnt::bridge::raster(&raster, o.charset.clone(), b.limits, &mut b.used)?;
    f.source_format = Some("lisa".into());
    b.table(e.name.len() * 4)?;
    f.names.full = Some(name(e.name)?);
    retain(&mut f, "lisa.name", e.name, b)?;
    retain(&mut f, "lisa.metrics", e.metrics, b)?;
    retain(&mut f, "lisa.resource", e.resource, b)?;
    f.properties.push(Property::new("lisa.index", index as i64));
    Ok(f)
}
/// Read a complete library, retaining unknown printer records as opaque entries.
pub fn read(data: &[u8], o: &LisaReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    check(data.len(), l.max_input_bytes, "Lisa input bytes")?;
    let mut b = Budget::new(l);
    b.add(data.len())?;
    decode(data, o, &mut b)
}
fn decode(data: &[u8], o: &LisaReadOptions, b: &mut Budget<'_>) -> Result<(FontSet, Report)> {
    let entries = directory(data, b)?;
    b.table(data.len())?;
    let mut set = FontSet {
        source_format: Some("lisa".into()),
        ..Default::default()
    };
    set.properties.push(Property::new(
        "lisa.source",
        PropertyValue::Bytes(data.to_vec()),
    ));
    let mut report = Report::default();
    let mut glyphs = 0usize;
    for (i, e) in entries.iter().enumerate() {
        if matches!(u16be(e.resource, 0)?, 0x9000 | 0xb000 | 0x9080 | 0xb080) {
            let f = decode_font(e, i, o, b)?;
            glyphs += f.len();
            check(glyphs, b.limits.max_glyphs, "Lisa cumulative glyph count")?;
            set.fonts.push(f);
        } else {
            let size = 13 + e.name.len() + e.resource.len() + e.metrics.len();
            b.table(size)?;
            let mut raw = Vec::with_capacity(size);
            raw.extend_from_slice(&(i as u32).to_be_bytes());
            raw.push(e.name.len() as u8);
            raw.extend_from_slice(e.name);
            raw.extend_from_slice(&(e.resource.len() as u32).to_be_bytes());
            raw.extend_from_slice(&(e.metrics.len() as u32).to_be_bytes());
            raw.extend_from_slice(e.resource);
            raw.extend_from_slice(e.metrics);
            set.properties
                .push(Property::new("lisa.opaque", PropertyValue::Bytes(raw)));
            b.loss(
                &mut report,
                "lisa.opaque_printer_resource",
                "Non-bitmap Lisa resource is retained as native opaque data",
            )?;
        }
    }
    if set.fonts.is_empty() {
        return Err(invalid("Lisa library has no bitmap fonts"));
    }
    Ok((set, report))
}
fn compressed(raster: &[u8], b: &mut Budget<'_>) -> Result<Option<Vec<u8>>> {
    let length = raster
        .len()
        .checked_sub(2)
        .ok_or_else(|| invalid("Lisa bitmap missing type"))?;
    let rounded = (length + 7) & !7;
    b.table(rounded)?;
    let mut body = vec![0; rounded];
    body[..length].copy_from_slice(&raster[2..]);
    let row = usize::from(u16be(raster, 24)?) * 2;
    let height = usize::from(u16be(raster, 14)?);
    slice(raster, 26, row * height)?;
    for y in (1..height).rev() {
        for x in 0..row {
            body[24 + y * row + x] ^= body[24 + (y - 1) * row + x];
        }
    }
    let size = 10 + body.iter().filter(|v| **v != 0).count() + rounded / 8;
    b.table(size + (size % 2))?;
    let mut out = Vec::with_capacity(size + size % 2);
    out.extend_from_slice(&(u16be(raster, 0)? | 0x80).to_be_bytes());
    out.extend_from_slice(
        &u32::try_from(size - 10)
            .map_err(|_| invalid("Lisa compressed length exceeds u32"))?
            .to_be_bytes(),
    );
    out.extend_from_slice(
        &u32::try_from(length)
            .map_err(|_| invalid("Lisa expanded length exceeds u32"))?
            .to_be_bytes(),
    );
    for chunk in body.chunks(8) {
        let mut mask = 0u8;
        for (i, &v) in chunk.iter().enumerate() {
            if v == 0 {
                mask |= 0x80 >> i;
            } else {
                out.push(v);
            }
        }
        out.push(mask);
    }
    if !compression_safe(&out[10..], rounded) {
        return Ok(None);
    }
    if out.len() % 2 != 0 {
        out.push(0);
    }
    Ok(Some(out))
}
struct Owned {
    name: Vec<u8>,
    resource: Vec<u8>,
    metrics: Vec<u8>,
}
/// Encode native resource directories and verify the actual same-limit reread.
pub fn write(set: &FontSet, o: &LisaWriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut b = Budget::new(&o.limits);
    b.model(set)?;
    if let Some(raw) = bytes(&set.properties, "lisa.source") {
        let (original, _) = decode(raw, &LisaReadOptions::default(), &mut b)?;
        let selected = directory(raw, &mut b)?
            .iter()
            .filter(|e| matches!(u16be(e.resource, 0), Ok(0x9000 | 0xb000 | 0x9080 | 0xb080)))
            .all(|e| match o.compression {
                Compression::Preserve => true,
                Compression::Compressed => e.resource[1] & 0x80 != 0,
                Compression::Uncompressed => e.resource[1] & 0x80 == 0,
            });
        if original == *set && selected && o.gray_threshold.is_none() {
            check(
                raw.len(),
                o.max_output_bytes.min(o.limits.max_input_bytes),
                "Lisa output bytes",
            )?;
            b.table(raw.len())?;
            let (decoded, mut report) = decode(raw, &Default::default(), &mut b)?;
            compare(set, &decoded, "lisa", &mut b, &mut report)?;
            return Ok((raw.to_vec(), report));
        }
    }
    let mut entries = BTreeMap::new();
    for p in set.properties.iter().filter(|p| p.name == "lisa.opaque") {
        let PropertyValue::Bytes(raw) = &p.value else {
            return Err(invalid("Lisa opaque entry must be bytes"));
        };
        let index = u32be(raw, 0)? as usize;
        let n = usize::from(*slice(raw, 4, 1)?.first().expect("byte"));
        let at = 5 + n;
        let rn = u32be(raw, at)? as usize;
        let mn = u32be(raw, at + 4)? as usize;
        b.table(n + rn + mn)?;
        let item = Owned {
            name: slice(raw, 5, n)?.to_vec(),
            resource: slice(raw, at + 8, rn)?.to_vec(),
            metrics: slice(raw, at + 8 + rn, mn)?.to_vec(),
        };
        if entries.insert(index, item).is_some() {
            return Err(invalid("Lisa duplicate entry index"));
        }
    }
    for (fi, f) in set.fonts.iter().enumerate() {
        let index = match f.properties.get("lisa.index") {
            Some(PropertyValue::Integer(v)) => {
                usize::try_from(*v).map_err(|_| invalid("Lisa negative entry index"))?
            }
            None => fi,
            _ => return Err(invalid("Lisa entry index must be integer")),
        };
        let text = f
            .names
            .full
            .as_deref()
            .or(f.names.family.as_deref())
            .unwrap_or("FONT.F");
        b.table(text.len())?;
        let name = native_name(text)?;
        check(name.len(), 255, "Lisa Pascal name")?;
        let raster = nfnt::bridge::encode_raster(
            f,
            &NfntWriteOptions {
                container: nfnt::Container::Bare,
                gray_threshold: o.gray_threshold,
                limits: o.limits.clone(),
                max_output_bytes: o.max_output_bytes,
                ..Default::default()
            },
            &mut b.used,
        )?;
        if u16be(&raster, 0)? & 0xdfff != 0x9000 {
            return Err(invalid("Lisa requires monochrome native NFNT raster"));
        }
        let use_compression = match o.compression {
            Compression::Compressed => true,
            Compression::Uncompressed => false,
            Compression::Preserve => bytes(&f.properties, "lisa.resource")
                .is_none_or(|v| v.get(1).is_some_and(|n| n & 0x80 != 0)),
        };
        let compressed = if use_compression {
            compressed(&raster, &mut b)?
        } else {
            None
        };
        if use_compression && compressed.is_none() && o.compression == Compression::Compressed {
            return Err(invalid(
                "Lisa bitmap cannot be compressed safely for the native in-place decoder",
            ));
        }
        let resource = if let Some(compressed) = compressed {
            compressed
        } else {
            b.table(raster.len())?;
            raster.clone()
        };
        let count = usize::from(u16be(&raster, 4)? - u16be(&raster, 2)?) + 2;
        let ow = 16 + usize::from(u16be(&raster, 16)?) * 2;
        b.table(26 + count * 2)?;
        let mut metrics = raster[..26].to_vec();
        metrics[16..18].copy_from_slice(&5u16.to_be_bytes());
        metrics[24..26].fill(0);
        metrics.extend_from_slice(slice(&raster, ow, count * 2)?);
        if entries
            .insert(
                index,
                Owned {
                    name,
                    resource,
                    metrics,
                },
            )
            .is_some()
        {
            return Err(invalid("Lisa bitmap and opaque entry index collide"));
        }
    }
    check(
        entries.len(),
        o.limits.max_resources,
        "Lisa directory count",
    )?;
    if entries.is_empty() {
        return Err(invalid("Lisa cannot write empty library"));
    }
    let dir = entries.values().try_fold(0usize, |n, e| {
        n.checked_add((e.name.len() + 2) & !1)
            .and_then(|n| n.checked_add(16))
            .ok_or_else(|| invalid("Lisa directory overflow"))
    })?;
    let words = u16::try_from(dir / 2).map_err(|_| invalid("Lisa directory exceeds u16 words"))?;
    let size = entries.values().try_fold(4 + dir, |n, e| {
        n.checked_add((e.resource.len() + 1) & !1)
            .and_then(|n| n.checked_add((e.metrics.len() + 1) & !1))
            .ok_or_else(|| invalid("Lisa file size overflow"))
    })?;
    check(
        size,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "Lisa output bytes",
    )?;
    b.table(size)?;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&words.to_be_bytes());
    out.extend_from_slice(
        &u16::try_from(entries.len())
            .map_err(|_| invalid("Lisa entry count exceeds u16"))?
            .to_be_bytes(),
    );
    let mut offset = 0usize;
    for e in entries.values() {
        out.push(e.name.len() as u8);
        out.extend_from_slice(&e.name);
        if out.len() % 2 != 0 {
            out.push(0);
        }
        let end = offset + e.resource.len().div_ceil(2);
        let mend = end + e.metrics.len().div_ceil(2);
        for n in [offset, end, end, mend] {
            out.extend_from_slice(
                &u32::try_from(n)
                    .map_err(|_| invalid("Lisa offset exceeds u32 words"))?
                    .to_be_bytes(),
            );
        }
        offset = mend;
    }
    for e in entries.into_values() {
        out.extend_from_slice(&e.resource);
        if out.len() % 2 != 0 {
            out.push(0);
        }
        out.extend_from_slice(&e.metrics);
        if out.len() % 2 != 0 {
            out.push(0);
        }
    }
    let (decoded, mut r) = decode(&out, &Default::default(), &mut b)?;
    compare(set, &decoded, "lisa", &mut b, &mut r)?;
    Ok((out, r))
}
