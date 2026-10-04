//! Native bounded Polyprint printer fonts.
use super::historical_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
use crate::model::{FontSet, Property, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// Explicit Polyprint input interpretation.
#[derive(Debug, Clone, Default)]
pub struct PolyprintReadOptions {
    /// Baseline override; absent uses the documented format convention.
    pub baseline: Option<i64>,
}
/// Bounded Polyprint output choices.
#[derive(Debug, Clone)]
pub struct PolyprintWriteOptions {
    /// Explicitly allow Unicode numbers as legacy code positions.
    pub unicode_as_legacy: bool,
    /// Explicit Gray8 reduction threshold.
    pub gray_threshold: Option<u8>,
    /// Input/model/pixel/table budgets shared with rereading.
    pub limits: ResourceLimits,
    /// Output cap, additionally bounded by reader input limits.
    pub max_output_bytes: usize,
}
impl Default for PolyprintWriteOptions {
    fn default() -> Self {
        Self {
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec for Polyprint.
#[derive(Debug, Clone, Default)]
pub struct PolyprintCodec {
    /// Typed reading options.
    pub read_options: PolyprintReadOptions,
    /// Typed writing options.
    pub write_options: PolyprintWriteOptions,
}
impl FormatDetector for PolyprintCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PolyprintCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PolyprintCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
const SIZE: usize = 128 + 144 * 84;
/// A fixed-length structural check provides weak evidence only.
pub fn detect(b: &[u8]) -> Confidence {
    if (b.len() == SIZE || b.len() == SIZE.next_multiple_of(128)) && b[2] <= 25 {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Read all 144 legacy positions and retain opaque printer metadata.
pub fn read(c: &mut ReadContext<'_>, o: &PolyprintReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &PolyprintReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    if (b.len() != SIZE && b.len() != SIZE.next_multiple_of(128)) || b[2] > 25 {
        return Err(invalid("invalid PolyPrint header or length"));
    }
    let mut f = source("polyprint", b, 144, l, budget)?;
    f.names.family = Some(latin(&b[3..3 + usize::from(b[2])]).trim_end().into());
    f.properties.push(Property::new(
        "polyprint.header",
        PropertyValue::Bytes(b[..128].to_vec()),
    ));
    if b.len() > SIZE {
        f.properties.push(Property::new(
            "polyprint.trailer",
            PropertyValue::Bytes(b[SIZE..].to_vec()),
        ));
    }
    let line = if b[29] & 32 != 0 { 36 } else { 18 };
    let baseline = o.baseline.unwrap_or(if line == 36 { 32 } else { 18 });
    if !(-65535..=65535).contains(&baseline) {
        return Err(invalid("PolyPrint baseline exceeds bounded range"));
    }
    for i in 0..144 {
        let g = &b[128 + i * 84..128 + (i + 1) * 84];
        let mut columns = [0u8; 80];
        for j in 0..4 {
            columns[j * 20..(j + 1) * 20].copy_from_slice(&g[j * 21 + 1..j * 21 + 21]);
        }
        let bitmap = from_columns(&columns, 20, 32, true, l, budget)?;
        glyph(
            &mut f,
            bitmap,
            32 + i as u32,
            1 - i64::from(g[0]),
            baseline - 32,
            1 + i64::from(g[21]) + i64::from(g[42]),
        )?;
        let id = f
            .glyph_id(crate::model::Codepoint::Legacy(32 + i as u32))
            .ok_or_else(|| invalid("PolyPrint glyph index"))?;
        f.edit_glyph(id, |glyph| {
            glyph.properties.push(Property::new(
                "polyprint.fields",
                PropertyValue::Bytes(vec![g[0], g[21], g[42], g[63]]),
            ));
            Ok(())
        })?;
    }
    metrics(&mut f, baseline, line);
    Ok((set(f, "polyprint"), Report::default()))
}
/// Encode the fixed printer layout and audit its decoded model.
pub fn write(s: &FontSet, o: &WriteOptions, v: &PolyprintWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = select(
            f,
            175,
            v.unicode_as_legacy,
            "polyprint",
            &mut report,
            &mut budget,
        )?;
        let trailer = bytes(f, "polyprint.trailer").unwrap_or(&[]);
        if !trailer.is_empty() && trailer.len() != SIZE.next_multiple_of(128) - SIZE {
            return Err(invalid("PolyPrint CP/M trailer must be 64 bytes"));
        }
        output_limit(
            SIZE + trailer.len(),
            v.max_output_bytes,
            &v.limits,
            &mut budget,
        )?;
        let mut out = vec![0; SIZE + trailer.len()];
        out[SIZE..].copy_from_slice(trailer);
        if let Some(h) = bytes(f, "polyprint.header") {
            out[..128].copy_from_slice(slice(h, 0, 128)?);
        }
        out[2] = 25;
        out[3..28].fill(b' ');
        if let Some(name) = &f.names.family {
            if name.chars().count() > 25 {
                return Err(invalid("PolyPrint family exceeds 25 bytes"));
            }
            for (i, c) in name.chars().enumerate() {
                out[3 + i] = u8::try_from(u32::from(c))
                    .map_err(|_| invalid("PolyPrint name is not Latin-1"))?;
            }
        }
        let baseline = if out[29] & 32 != 0 { 32 } else { 18 };
        for code in 32..176 {
            let g = slots.get(&code).copied();
            let at = 128 + (code - 32) as usize * 84;
            let x = g.map_or(0, |g| g.metrics.bounds.x);
            let left =
                u8::try_from(1 - x).map_err(|_| invalid("PolyPrint left marker exceeds u8"))?;
            let advance = g
                .and_then(|g| g.metrics.advance.or(f.metrics.glyph_defaults.advance))
                .map_or(20, |a| a.x);
            let old = g.and_then(|g| match g.properties.get("polyprint.fields") {
                Some(PropertyValue::Bytes(b)) => Some(b.as_slice()),
                _ => None,
            });
            let width = old
                .and_then(|b| b.get(1))
                .copied()
                .unwrap_or_else(|| advance.saturating_sub(1).clamp(0, 255) as u8);
            let right = u8::try_from(advance - 1 - i64::from(width))
                .map_err(|_| invalid("PolyPrint advance cannot fit width/right bytes"))?;
            out[at] = left;
            out[at + 21] = width;
            out[at + 42] = right;
            out[at + 63] = old.and_then(|b| b.get(3)).copied().unwrap_or(0);
            let raw = project(
                g,
                Bounds::new(20, 32, x, baseline - 32),
                &v.limits,
                &mut budget,
            )?;
            let columns = to_columns(&raw, 20, 32, true, &mut budget)?;
            for j in 0..4 {
                out[at + j * 21 + 1..at + j * 21 + 21]
                    .copy_from_slice(&columns[j * 20..(j + 1) * 20]);
            }
        }
        let (target, rr) = parse(&out, &Default::default(), &v.limits, &mut budget)?;
        report.extend(rr);
        audit(s, &target, "polyprint", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "polyprint", o.strict)
}
