//! Daisy-Dot printer fonts, including scoped magnified sidecars.
use super::historical_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
use crate::model::{FontSet, Property, PropertyValue};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Report, ResourceLimits, Result};
/// Daisy-Dot file generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum DaisyVersion {
    /// Daisy-Dot I/II, sixteen rows.
    V2,
    /// Daisy-Dot III, at most thirty-two rows.
    #[default]
    V3,
    /// Up to four thirty-two-row files.
    Magnified,
}
/// Read interpretation; no filesystem search is performed.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DaisyReadOptions {
    /// Require a particular generation.
    pub version: Option<DaisyVersion>,
}
/// Explicit bounded output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DaisyWriteOptions {
    /// Output generation.
    pub version: DaisyVersion,
    /// Actual height marker plus one; absent uses source metadata or cell height.
    pub height: Option<u8>,
    /// Underline row used to derive baseline.
    pub underline: Option<u8>,
    /// Permit graphics bytes forbidden by the original manual.
    pub allow_control_bytes: bool,
    /// Permit Unicode numeric positions.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Aggregate output cap.
    pub max_output_bytes: usize,
}
impl Default for DaisyWriteOptions {
    fn default() -> Self {
        Self {
            version: DaisyVersion::V3,
            height: None,
            underline: None,
            allow_control_bytes: false,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DaisyCodec {
    /// Reading options.
    pub read_options: DaisyReadOptions,
    /// Writing options.
    pub write_options: DaisyWriteOptions,
}
impl FormatDetector for DaisyCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for DaisyCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for DaisyCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
const V2: &[u8] = b"DAISY-DOT NLQ FONT\x9b";
fn variant(b: &[u8]) -> Result<DaisyVersion> {
    if b.starts_with(V2) {
        Ok(DaisyVersion::V2)
    } else if b.starts_with(b"3\x9b") {
        Ok(DaisyVersion::V3)
    } else if b.starts_with(b"B\x9b") {
        Ok(DaisyVersion::Magnified)
    } else {
        Err(invalid("Daisy-Dot signature mismatch"))
    }
}
/// Recognize an explicit format signature.
pub fn detect(b: &[u8]) -> Confidence {
    if variant(b).is_ok() {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn codes() -> impl Iterator<Item = u32> {
    (32..125).filter(|c| *c != 96 && *c != 123)
}
fn sidepath(name: &str, index: usize) -> Result<ResourcePath> {
    let (last, _) = name
        .char_indices()
        .last()
        .ok_or_else(|| invalid("Daisy magnified input needs a filename"))?;
    ResourcePath::new(format!("{}{index}", &name[..last]))
}
struct Part<'a> {
    records: Vec<(u32, u32, &'a [u8])>,
    footer: [u8; 3],
    height: u32,
    controls: bool,
}
fn part<'a>(
    b: &'a [u8],
    v: DaisyVersion,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Part<'a>> {
    check(b.len(), l.max_table_bytes, "Daisy table bytes")?;
    variant(b)?;
    let mut at = if v == DaisyVersion::V2 { V2.len() } else { 2 };
    budget.items::<(u32, u32, &[u8])>(91)?;
    let mut records = Vec::with_capacity(91);
    let mut height = 16;
    let mut controls = false;
    for cp in codes() {
        if cp == 32 && v != DaisyVersion::V2 {
            continue;
        }
        let tag = slice(b, at, 1)?[0];
        at += 1;
        let w = if v == DaisyVersion::V2 { tag } else { tag & 63 };
        if w == 0
            || w > if v == DaisyVersion::V2 { 19 } else { 32 }
            || v != DaisyVersion::V2 && tag & 128 != 0
        {
            return Err(invalid("Daisy glyph width/flags outside format range"));
        }
        let h = if v != DaisyVersion::V2 && tag & 64 != 0 {
            32
        } else {
            16
        };
        height = height.max(h);
        let n = usize::from(w) * (h as usize / 8);
        let raw = slice(b, at, n)?;
        at += n;
        controls |= raw.iter().any(|v| *v == 13 || *v == 155);
        records.push((u32::from(w), h, raw));
        if v == DaisyVersion::V2 {
            if slice(b, at, 1)?[0] != 155 {
                return Err(invalid("Daisy II record terminator mismatch"));
            }
            at += 1;
        }
    }
    let footer = if v == DaisyVersion::V2 {
        [15, 17, 1]
    } else {
        let f = slice(b, at, 3)?;
        at += 3;
        [f[0], f[1], f[2]]
    };
    if at != b.len() {
        return Err(invalid("Daisy trailing data"));
    }
    if v != DaisyVersion::V2
        && (footer[0] == 0
            || footer[0]
                > if v == DaisyVersion::Magnified {
                    127
                } else {
                    31
                }
            || footer[1] == 0
            || footer[1]
                > if v == DaisyVersion::Magnified {
                    127
                } else {
                    31
                }
            || footer[2] == 0
            || footer[2] > 32)
    {
        return Err(invalid("Daisy footer outside supported range"));
    }
    Ok(Part {
        records,
        footer,
        height,
        controls,
    })
}
/// Read a single font; magnified fragments use the context resolver.
pub fn read(c: &mut ReadContext<'_>, o: &DaisyReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    let v = variant(c.bytes())?;
    if o.version.is_some_and(|x| x != v) {
        return Err(invalid("Daisy generation differs from requested variant"));
    }
    let p = part(c.bytes(), v, c.limits(), &mut budget)?;
    let n = if v == DaisyVersion::Magnified {
        usize::from(p.footer[0]) + 1
    } else {
        1
    };
    let count = if v == DaisyVersion::Magnified {
        n.div_ceil(32)
    } else {
        1
    };
    drop(p);
    let mut extras = Vec::new();
    budget.items::<Vec<u8>>(count.saturating_sub(1))?;
    for i in 2..=count {
        let path = sidepath(
            c.name()
                .ok_or_else(|| invalid("magnified Daisy needs a resource filename"))?,
            i,
        )?;
        let raw = c.resource_limited(&path, budget.max.saturating_sub(budget.used))?;
        budget.add(raw.len())?;
        extras.push(raw);
    }
    let mut all = vec![c.bytes()];
    all.extend(extras.iter().map(Vec::as_slice));
    parse(&all, v, c.limits(), &mut budget)
}
fn parse(
    all: &[&[u8]],
    v: DaisyVersion,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let parts: Vec<_> = all
        .iter()
        .map(|b| part(b, v, l, budget))
        .collect::<Result<_>>()?;
    let first = parts
        .first()
        .ok_or_else(|| invalid("Daisy missing primary"))?;
    let expected = if v == DaisyVersion::Magnified {
        (usize::from(first.footer[0]) + 1).div_ceil(32)
    } else {
        1
    };
    if parts.len() != expected {
        return Err(invalid("Daisy fragment count mismatch"));
    }
    let total: u32 = parts.iter().map(|p| p.height).sum();
    if total > 128 {
        return Err(invalid("Daisy height exceeds 128"));
    }
    let baseline = if v == DaisyVersion::V2 {
        16
    } else {
        i64::from(first.footer[1]) - 1
    };
    let pixel = if v == DaisyVersion::V2 {
        16
    } else {
        i64::from(first.footer[0]) + 1
    };
    let mut f = source("daisy", all[0], 91, l, budget)?;
    let mut report = Report::default();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            budget.add(all[i].len())?;
            f.properties.push(Property::new(
                format!("daisy.part{}", i + 1),
                PropertyValue::Bytes(all[i].to_vec()),
            ));
        }
        if p.controls {
            budget.loss(&mut report,"daisy","read.control_bytes",Some(0),None,"Original graphics contains bytes 13 or 155 forbidden by the manual; pixels and original bytes retained")?;
        }
    }
    f.properties.push(Property::new(
        "daisy.footer",
        PropertyValue::Bytes(first.footer.to_vec()),
    ));
    let mut ix = 0;
    for cp in codes() {
        let w = if cp == 32 && v != DaisyVersion::V2 {
            u32::from(first.footer[2])
        } else {
            first.records[ix].0
        };
        let stride = (w as usize).div_ceil(8);
        let n = stride * total as usize;
        budget.add(n)?;
        let mut packed = vec![0; n];
        let mut top = 0;
        if cp != 32 || v == DaisyVersion::V2 {
            for p in &parts {
                let (pw, h, raw) = p.records[ix];
                if pw != w {
                    return Err(invalid("Daisy fragment glyph widths disagree"));
                }
                let b = from_columns(raw, w, h, true, l, budget)?;
                packed[top * stride..top * stride + b.as_bytes().len()]
                    .copy_from_slice(b.as_bytes());
                top += p.height as usize;
            }
            ix += 1;
        }
        let bitmap = raster(w, total, &packed, l, budget)?;
        glyph(
            &mut f,
            bitmap,
            cp,
            0,
            baseline - i64::from(total),
            i64::from(w) + 1,
        )?;
    }
    metrics(&mut f, baseline, pixel + 4);
    f.metrics.pixel_size = Some(pixel as u32);
    f.metrics.descent = Some(pixel - baseline);
    f.metrics.underline_position = Some(-1);
    Ok((set(f, "daisy"), report))
}
/// Write a font and every magnification fragment as one validated bundle.
pub fn write(s: &FontSet, o: &WriteOptions, v: &DaisyWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = select(
            f,
            124,
            v.unicode_as_legacy,
            "daisy",
            &mut report,
            &mut budget,
        )?;
        let old = bytes(f, "daisy.footer");
        let defaultheight = old
            .and_then(|b| b.first())
            .map(|h| u32::from(*h) + 1)
            .unwrap_or_else(|| {
                f.glyphs()
                    .map(|(_, g)| g.bitmap.height())
                    .max()
                    .unwrap_or(16)
            });
        let pixel = u32::from(v.height.unwrap_or(
            u8::try_from(defaultheight).map_err(|_| invalid("Daisy height exceeds u8"))?,
        ));
        let max = match v.version {
            DaisyVersion::V2 => 16,
            DaisyVersion::V3 => 32,
            DaisyVersion::Magnified => 128,
        };
        if pixel < 2 || pixel > max {
            return Err(invalid("Daisy output height outside generation range"));
        }
        let parts = if v.version == DaisyVersion::Magnified {
            pixel.div_ceil(32) as usize
        } else {
            1
        };
        check(parts - 1, v.limits.max_resources, "Daisy resources")?;
        let storage =
            if v.version == DaisyVersion::V2 || v.version == DaisyVersion::V3 && pixel <= 16 {
                16
            } else {
                32
            };
        let total = storage * parts as u32;
        let underline = v
            .underline
            .or_else(|| old.and_then(|b| b.get(1)).copied())
            .unwrap_or((pixel - 1) as u8);
        if underline == 0
            || underline
                > if v.version == DaisyVersion::Magnified {
                    127
                } else {
                    31
                }
        {
            return Err(invalid("Daisy underline outside 1..127"));
        }
        let baseline = if v.version == DaisyVersion::V2 {
            16
        } else {
            i64::from(underline) - 1
        };
        let mut widths = Vec::with_capacity(91);
        for cp in codes() {
            let g = slots.get(&cp).copied();
            let width = g
                .and_then(|g| g.metrics.advance.or(f.metrics.glyph_defaults.advance))
                .map(|a| a.x - 1)
                .unwrap_or_else(|| g.map_or(1, |g| i64::from(g.bitmap.width()).max(1)));
            let w = u32::try_from(width).map_err(|_| invalid("Daisy nonpositive glyph width"))?;
            if w == 0
                || w > if v.version == DaisyVersion::V2 {
                    19
                } else {
                    32
                }
            {
                return Err(invalid("Daisy glyph width exceeds format range"));
            }
            widths.push(w);
        }
        let per = if v.version == DaisyVersion::V2 {
            V2.len() + widths.iter().map(|w| *w as usize * 2 + 2).sum::<usize>()
        } else {
            5 + widths[1..]
                .iter()
                .map(|w| 1 + *w as usize * storage as usize / 8)
                .sum::<usize>()
        };
        output_limit(per * parts, v.max_output_bytes, &v.limits, &mut budget)?;
        check(
            per,
            v.limits
                .max_resource_bytes
                .max(if parts == 1 { per } else { 0 }),
            "Daisy fragment bytes",
        )?;
        let mut assets = Vec::with_capacity(parts);
        for i in 0..parts {
            let mut out = Vec::with_capacity(per);
            out.extend_from_slice(match v.version {
                DaisyVersion::V2 => V2,
                DaisyVersion::V3 => b"3\x9b",
                DaisyVersion::Magnified => b"B\x9b",
            });
            for (cp, w) in codes().zip(&widths) {
                if cp == 32 && v.version != DaisyVersion::V2 {
                    continue;
                }
                let g = slots.get(&cp).copied();
                let packed = project(
                    g,
                    Bounds::new(*w, total, 0, baseline - i64::from(total)),
                    &v.limits,
                    &mut budget,
                )?;
                let stride = (*w as usize).div_ceil(8);
                let start = i * storage as usize * stride;
                let columns = to_columns(
                    &packed[start..start + storage as usize * stride],
                    *w,
                    storage,
                    true,
                    &mut budget,
                )?;
                if !v.allow_control_bytes && columns.iter().any(|b| *b == 13 || *b == 155) {
                    return Err(invalid(
                        "Daisy graphics contains forbidden byte 13 or 155; allow_control_bytes is required",
                    ));
                }
                out.push(*w as u8 + if storage == 32 { 64 } else { 0 });
                out.extend_from_slice(&columns);
                if v.version == DaisyVersion::V2 {
                    out.push(155)
                }
            }
            if v.version != DaisyVersion::V2 {
                out.extend_from_slice(&[
                    if i == 0 { pixel as u8 - 1 } else { 31 },
                    underline,
                    widths[0] as u8,
                ]);
            }
            let path = if i == 0 {
                o.primary_name.clone()
            } else {
                sidepath(o.primary_name.as_str(), i + 1)?
            };
            assets.push(OutputAsset { path, bytes: out });
        }
        let refs: Vec<_> = assets.iter().map(|a| a.bytes.as_slice()).collect();
        let (actual, readreport) = parse(&refs, v.version, &v.limits, &mut budget)?;
        report.extend(readreport);
        audit(s, &actual, "daisy", &mut report, &mut budget)?;
        OutputBundle::new(o.primary_name.clone(), assets)
    })();
    finish(result, report, "daisy", o.strict)
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&DaisyVersion::V2), "v2");
        assert_eq!(round_trip(&DaisyVersion::V3), "v3");
        assert_eq!(round_trip(&DaisyVersion::Magnified), "magnified");
        rejects::<DaisyVersion>(r#""not_a_variant""#);
        defaults::<DaisyReadOptions>();
        defaults::<DaisyWriteOptions>();
        defaults::<DaisyCodec>();
    }
}
