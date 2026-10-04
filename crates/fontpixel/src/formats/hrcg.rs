//! Native hrcg font reader and writer.
/// Native read interpretation.
pub use super::w5a2_common::NativeReadOptions as HrcgReadOptions;
/// Native write projection.
pub use super::w5a2_common::NativeWriteOptions as HrcgWriteOptions;
use super::w5a2_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
/// Native hrcg codec.
#[derive(Debug, Clone, Default)]
pub struct HrcgCodec {
    /// Reader interpretation.
    pub read_options: HrcgReadOptions,
    /// Writer projection and bounds.
    pub write_options: HrcgWriteOptions,
}
impl FormatDetector for HrcgCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for HrcgCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for HrcgCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Decode native data under shared resource limits.
pub fn read(c: &mut ReadContext<'_>, o: &HrcgReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
/// Encode a deterministic native file and audit its decoded model.
pub fn write(s: &FontSet, o: &WriteOptions, p: &HrcgWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = (|| {
        let mut b = source(s, p)?;
        b.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let set = convert_gray(s, p.gray_threshold, &p.limits, &mut b)?;
        let bytes = encode(set.single()?, p, &mut b)?;
        output_check(bytes.len(), &p.limits, &mut b)?;
        let (back, rr) = parse(&bytes, &Default::default(), &p.limits, &mut b)?;
        r.extend(rr);
        audit(s, &back, "hrcg", &mut r, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), bytes))
    })();
    finish(result, r, "hrcg", o.strict)
}
/// HRCG has no magic: its 768-byte size provides only weak evidence.
pub fn detect(s: &[u8]) -> Confidence {
    if s.len() == 768 {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
fn parse(
    s: &[u8],
    o: &HrcgReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "HRCG input bytes")?;
    if s.len() != 768 {
        return Err(invalid("HRCG requires exactly 768 bytes"));
    }
    let mut f = base("hrcg", o, b)?;
    for (i, rows) in s.as_chunks::<8>().0.iter().enumerate() {
        let mut raw = [0u8; 16];
        for (y, &v) in rows.iter().enumerate() {
            for x in 0..7 {
                if v & (1 << x) != 0 {
                    let column = 2 * x + usize::from(v >> 7);
                    raw[y * 2 + column / 8] |= 128 >> (column % 8);
                }
            }
        }
        let id = glyph(
            &mut f,
            Codepoint::Legacy(32 + i as u32),
            14,
            8,
            &raw,
            o,
            l,
            b,
        )?;
        b.add(192)?;
        f.edit_glyph(id, |g| {
            g.properties.push(Property::new(
                "hrcg.phase",
                PropertyValue::Bytes(rows.iter().map(|v| v >> 7).collect()),
            ));
            Ok(())
        })?;
    }
    Ok((complete(f, "hrcg")?, Report::default()))
}
fn encode(f: &Font, o: &HrcgWriteOptions, b: &mut Budget) -> Result<Vec<u8>> {
    let c = cell(f, o, Some((14, 8)))?;
    let map = legacy_map(f, o, 127, b)?;
    let mut out = allocated(768, b)?;
    for (cp, rows) in (32..128u32).zip(out.as_chunks_mut::<8>().0.iter_mut()) {
        let g = map.get(&cp).copied();
        let raster = raster(g, c, &o.limits, b)?;
        let saved = g.and_then(|g| match g.properties.get("hrcg.phase") {
            Some(PropertyValue::Bytes(v)) => Some(v.as_slice()),
            _ => None,
        });
        if saved.is_some_and(|v| v.len() != 8 || v.iter().any(|v| *v > 1)) {
            return Err(invalid("invalid HRCG native phase property"));
        }
        for (y, row) in rows.iter_mut().enumerate() {
            let mut even = 0u8;
            let mut odd = 0u8;
            for x in 0..7 {
                even |= raster.get_pixel(2 * x, y as u32).unwrap_or(0) << (x as u8);
                odd |= raster.get_pixel(2 * x + 1, y as u32).unwrap_or(0) << (x as u8);
            }
            if even != 0 && odd != 0 {
                return Err(invalid("HRCG row contains both half-dot phases"));
            }
            *row = if odd != 0 {
                odd | 128
            } else if even != 0 {
                even
            } else {
                saved.map_or(0, |v| v[y] << 7)
            };
        }
    }
    Ok(out)
}
