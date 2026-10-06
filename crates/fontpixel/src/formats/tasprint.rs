//! Native Tasprint platform font layouts.
use super::historical_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
use crate::model::{FontSet, Property, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// Tasprint platform layouts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TasprintVersion {
    /// Headerless Spectrum/QL 96-character strikes.
    #[default]
    Spectrum48k,
    /// +3DOS 16-column strike with width word.
    Plus3,
    /// AMSDOS 256-character strikes.
    Cpc,
    /// Headerless 16-column PCW strike, code positions start at one.
    Pcw,
}
/// Explicit interpretation for headerless input.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct TasprintReadOptions {
    /// Absent uses signatures and bounded size heuristics.
    pub version: Option<TasprintVersion>,
    /// Actual PCW width, 1..16.
    pub width: Option<u8>,
}
/// Bounded native encoding choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct TasprintWriteOptions {
    /// Platform layout.
    pub version: TasprintVersion,
    /// Actual +3/PCW width, 1..16; absent uses source or cell width.
    pub width: Option<u8>,
    /// PCW slots: original 127 or interoperable extended 128.
    pub pcw_count: u16,
    /// Permit Unicode numeric positions.
    pub unicode_as_legacy: bool,
    /// Explicit gray reduction.
    pub gray_threshold: Option<u8>,
    /// Shared resource budgets.
    pub limits: ResourceLimits,
    /// Output size cap.
    pub max_output_bytes: usize,
}
impl Default for TasprintWriteOptions {
    fn default() -> Self {
        Self {
            version: TasprintVersion::Spectrum48k,
            width: None,
            pcw_count: 128,
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
pub struct TasprintCodec {
    /// Read interpretation.
    pub read_options: TasprintReadOptions,
    /// Write choices.
    pub write_options: TasprintWriteOptions,
}
impl FormatDetector for TasprintCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(
            if matches!(
                infer(c.bytes()),
                Ok(TasprintVersion::Plus3 | TasprintVersion::Cpc)
            ) {
                detect(c.bytes())
            } else {
                Confidence::NoMatch
            },
        )
    }
}
impl FormatReader<FontSet> for TasprintCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for TasprintCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
fn infer(b: &[u8]) -> Result<TasprintVersion> {
    if b.starts_with(b"PLUS3DOS\x1a") {
        Ok(TasprintVersion::Plus3)
    } else if b.len() >= 128 && b[18] == 2 && b[24..26] == [0, 20] && b.len().is_multiple_of(5248) {
        Ok(TasprintVersion::Cpc)
    } else if !b.is_empty() && b.len().is_multiple_of(1920) {
        Ok(TasprintVersion::Spectrum48k)
    } else if b.len() == 4064 || b.len() == 4096 {
        Ok(TasprintVersion::Pcw)
    } else {
        Err(invalid(
            "unknown Tasprint layout; explicit version may be required",
        ))
    }
}
/// Headerless dimensions and AMSDOS matches provide weak evidence only.
pub fn detect(b: &[u8]) -> Confidence {
    match infer(b) {
        Ok(TasprintVersion::Plus3)
            if b.len() == 3202 && b[15] == 3 && b[16..20] == [2, 12, 48, 117] =>
        {
            Confidence::Certain
        }
        Ok(_) => Confidence::Possible,
        Err(_) => Confidence::NoMatch,
    }
}
/// Decode every complete strike, rejecting trailing or truncated data.
pub fn read(c: &mut ReadContext<'_>, o: &TasprintReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &TasprintReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let v = o.version.map(Ok).unwrap_or_else(|| infer(b))?;
    let (size, start, count, cols, first) = match v {
        TasprintVersion::Spectrum48k => (1920, 0, 96, 10, 32),
        TasprintVersion::Plus3 => (3202, 130, 96, 16, 32),
        TasprintVersion::Cpc => (5248, 128, 256, 10, 0),
        TasprintVersion::Pcw => {
            if b.len() != 4064 && b.len() != 4096 {
                return Err(invalid("PCW requires 127 or 128 complete glyphs"));
            }
            (b.len(), 0, b.len() / 32, 16, 1)
        }
    };
    if b.is_empty()
        || !b.len().is_multiple_of(size)
        || (v == TasprintVersion::Plus3 || v == TasprintVersion::Pcw) && b.len() != size
    {
        return Err(invalid("Tasprint strike length mismatch"));
    }
    let nf = b.len() / size;
    check(nf, l.max_glyphs, "Tasprint strikes")?;
    check(
        nf.saturating_mul(count),
        l.max_glyphs,
        "Tasprint aggregate glyphs",
    )?;
    budget.items::<crate::model::Font>(nf)?;
    let mut fonts = Vec::with_capacity(nf);
    let mut report = Report::default();
    for (fi, strike) in b.chunks_exact(size).enumerate() {
        let mut width = cols;
        if v == TasprintVersion::Plus3 {
            if !strike.starts_with(b"PLUS3DOS\x1a")
                || strike[15] != 3
                || u16_at(strike, 16, ByteOrder::Little)? != 3074
                || u16_at(strike, 18, ByteOrder::Little)? != 30000
                || u32_at(strike, 11, ByteOrder::Little)? != 3202
            {
                return Err(invalid("Tasprint +3 header mismatch"));
            }
            if strike[..127].iter().fold(0u8, |s, x| s.wrapping_add(*x)) != strike[127] {
                return Err(invalid("Tasprint +3 header checksum mismatch"));
            }
            width = usize::from(u16_at(strike, 128, ByteOrder::Little)?);
        }
        if v == TasprintVersion::Cpc {
            if strike[18] != 2
                || u16_at(strike, 24, ByteOrder::Little)? != 5120
                || u16_at(strike, 64, ByteOrder::Little)? != 5120
                || strike[66] != 0
            {
                return Err(invalid("Tasprint AMSDOS header length/type mismatch"));
            }
            if strike[..67].iter().map(|x| u16::from(*x)).sum::<u16>()
                != u16_at(strike, 67, ByteOrder::Little)?
            {
                return Err(invalid("Tasprint AMSDOS header checksum mismatch"));
            }
        }
        if v == TasprintVersion::Pcw {
            width = usize::from(o.width.unwrap_or(16));
        }
        if width == 0 || width > cols {
            return Err(invalid("Tasprint actual width outside stored column count"));
        }
        let mut f = source("tasprint", strike, count, l, budget)?;
        f.properties.push(Property::new(
            "tasprint.width",
            PropertyValue::Unsigned(width as u64),
        ));
        if start > 0 {
            f.properties.push(Property::new(
                "tasprint.header",
                PropertyValue::Bytes(strike[..start].to_vec()),
            ));
        }
        let mut clipped = false;
        for i in 0..count {
            let raw = &strike[start + i * cols * 2..start + (i + 1) * cols * 2];
            let full = from_columns(raw, cols as u32, 16, false, l, budget)?;
            clipped |= raw[width..cols]
                .iter()
                .chain(raw[cols + width..].iter())
                .any(|x| *x != 0);
            let bitmap = if width == cols {
                full
            } else {
                budget.add(width.div_ceil(8) * 16)?;
                full.crop_bounds(Bounds::new(width as u32, 16, 0, 0))?
            };
            glyph(&mut f, bitmap, (first + i) as u32, 0, 0, width as i64)?;
        }
        if clipped {
            budget.loss(
                &mut report,
                "tasprint",
                "read.hidden_columns",
                Some(fi),
                None,
                "Pixels outside declared width are retained only in the original source bytes",
            )?;
        }
        metrics(&mut f, 16, 16);
        fonts.push(f);
    }
    let set = FontSet {
        source_format: Some("tasprint".into()),
        fonts,
        ..Default::default()
    };
    Ok((set, report))
}
/// Write all strikes; +3 and PCW require exactly one font.
pub fn write(s: &FontSet, o: &WriteOptions, v: &TasprintWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        if converted.fonts.is_empty() {
            return Err(invalid("Tasprint needs a strike"));
        }
        if matches!(v.version, TasprintVersion::Plus3 | TasprintVersion::Pcw) {
            converted.single()?;
        }
        if v.version == TasprintVersion::Pcw && v.pcw_count != 127 && v.pcw_count != 128 {
            return Err(invalid("PCW count must be 127 or 128"));
        }
        let (size, start, count, cols, first) = match v.version {
            TasprintVersion::Spectrum48k => (1920, 0, 96, 10, 32),
            TasprintVersion::Plus3 => (3202, 130, 96, 16, 32),
            TasprintVersion::Cpc => (5248, 128, 256, 10, 0),
            TasprintVersion::Pcw => (
                usize::from(v.pcw_count) * 32,
                0,
                usize::from(v.pcw_count),
                16,
                1,
            ),
        };
        let n = size
            .checked_mul(converted.fonts.len())
            .ok_or_else(|| invalid("Tasprint size overflow"))?;
        output_limit(n, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = vec![0; n];
        let mut actualwidth = 16;
        for (fi, f) in converted.fonts.iter().enumerate() {
            let dst = &mut out[fi * size..(fi + 1) * size];
            let mut selection_report = Report::default();
            let selection = select(
                f,
                (first + count - 1) as u32,
                v.unicode_as_legacy,
                "tasprint",
                &mut selection_report,
                &mut budget,
            );
            for entry in &mut selection_report.entries {
                if entry.location.font_index.is_some() {
                    entry.location.font_index = Some(fi);
                }
            }
            report.extend(selection_report);
            let slots = selection?;
            let width = if cols == 10 {
                10
            } else {
                usize::from(v.width.unwrap_or_else(|| {
                    f.properties
                        .get("tasprint.width")
                        .and_then(|p| {
                            if let PropertyValue::Unsigned(x) = p {
                                u8::try_from(*x).ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(16)
                }))
            };
            if width == 0 || width > cols {
                return Err(invalid("Tasprint width outside target column count"));
            }
            actualwidth = width;
            if start > 0
                && let Some(h) = bytes(f, "tasprint.header")
            {
                let n = start.min(h.len());
                dst[..n].copy_from_slice(&h[..n]);
            }
            if v.version == TasprintVersion::Plus3 {
                dst[..9].copy_from_slice(b"PLUS3DOS\x1a");
                dst[9] = 1;
                dst[10] = 0;
                put32(dst, 11, 3202, ByteOrder::Little);
                dst[15] = 3;
                put16(dst, 16, 3074, ByteOrder::Little);
                put16(dst, 18, 30000, ByteOrder::Little);
                put16(dst, 20, 21506, ByteOrder::Little);
                put16(dst, 128, width as u16, ByteOrder::Little);
                dst[127] = dst[..127].iter().fold(0u8, |s, x| s.wrapping_add(*x));
            }
            if v.version == TasprintVersion::Cpc {
                dst[18] = 2;
                put16(dst, 24, 5120, ByteOrder::Little);
                put16(dst, 64, 5120, ByteOrder::Little);
                dst[66] = 0;
                let sum = dst[..67].iter().map(|x| u16::from(*x)).sum();
                put16(dst, 67, sum, ByteOrder::Little);
            }
            for cp in first..first + count {
                let g = slots.get(&(cp as u32)).copied();
                let raw = project(
                    g,
                    Bounds::new(width as u32, 16, 0, 0),
                    &v.limits,
                    &mut budget,
                )?;
                let columns = to_columns(&raw, width as u32, 16, false, &mut budget)?;
                let at = start + (cp - first) * cols * 2;
                dst[at..at + width].copy_from_slice(&columns[..width]);
                dst[at + cols..at + cols + width].copy_from_slice(&columns[width..]);
            }
        }
        let (actual, r) = parse(
            &out,
            &TasprintReadOptions {
                version: Some(v.version),
                width: Some(actualwidth as u8),
            },
            &v.limits,
            &mut budget,
        )?;
        report.extend(r);
        audit(s, &actual, "tasprint", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "tasprint", o.strict)
}
