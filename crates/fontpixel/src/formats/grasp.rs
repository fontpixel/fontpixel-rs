//! Native grasp font reader and writer.
/// Native read interpretation.
pub use super::w5a2_common::NativeReadOptions as GraspReadOptions;
/// Native write projection.
pub use super::w5a2_common::NativeWriteOptions as GraspWriteOptions;
use super::w5a2_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
/// Native grasp codec.
#[derive(Debug, Clone, Default)]
pub struct GraspCodec {
    /// Reader interpretation.
    pub read_options: GraspReadOptions,
    /// Writer projection and bounds.
    pub write_options: GraspWriteOptions,
}
impl FormatDetector for GraspCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for GraspCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for GraspCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Decode native data under shared resource limits.
pub fn read(c: &mut ReadContext<'_>, o: &GraspReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
/// Encode a deterministic native file and audit its decoded model.
pub fn write(s: &FontSet, o: &WriteOptions, p: &GraspWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = (|| {
        let mut b = source(s, p)?;
        b.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let set = convert_gray(s, p.gray_threshold, &p.limits, &mut b)?;
        let bytes = encode(set.single()?, p, &mut b)?;
        output_check(bytes.len(), &p.limits, &mut b)?;
        let (back, rr) = parse(&bytes, &Default::default(), &p.limits, &mut b)?;
        r.extend(rr);
        audit(s, &back, "grasp", &mut r, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), bytes))
    })();
    finish(result, r, "grasp", o.strict)
}
fn layout(s: &[u8]) -> Result<(u32, u32, u32, usize)> {
    if s.len() < 7 {
        return Err(invalid("truncated GRASP header"));
    }
    let (w, h, count) = (u32::from(s[4]), u32::from(s[5]), usize::from(s[2]));
    let n = (w as usize).div_ceil(8) * h as usize;
    if w == 0
        || h == 0
        || count == 0
        || u32::from(s[3]) + count as u32 > 256
        || n != usize::from(s[6])
        || s.len() != 7 + n * count
        || usize::from(u16le(s, 0)?) != s.len()
    {
        return Err(invalid("invalid old GRASP geometry or length"));
    }
    Ok((w, h, u32::from(s[3]), count))
}
/// Detect a structurally valid old GRASP header and raster length.
pub fn detect(s: &[u8]) -> Confidence {
    if layout(s).is_ok() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
fn parse(
    s: &[u8],
    o: &GraspReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "GRASP input bytes")?;
    let (w, h, first, count) = layout(s)?;
    check(count, l.max_glyphs, "GRASP glyph count")?;
    let n = size(w, h, l)?;
    let mut f = base("grasp", o, b)?;
    for (i, raw) in s[7..].chunks_exact(n).enumerate() {
        glyph(
            &mut f,
            Codepoint::Legacy(first + i as u32),
            w,
            h,
            raw,
            o,
            l,
            b,
        )?;
    }
    Ok((complete(f, "grasp")?, Report::default()))
}
fn encode(f: &Font, o: &GraspWriteOptions, b: &mut Budget) -> Result<Vec<u8>> {
    let c = cell(f, o, None)?;
    let map = legacy_map(f, o, 255, b)?;
    let first = *map
        .first_key_value()
        .ok_or_else(|| invalid("empty GRASP font"))?
        .0;
    let last = *map
        .last_key_value()
        .ok_or_else(|| invalid("empty GRASP font"))?
        .0;
    let count = (last - first + 1) as usize;
    let n = size(c.width, c.height, &o.limits)?;
    let total = 7 + count * n;
    if c.width > 255 || c.height > 255 || count > 255 || n > 255 || total > 65535 {
        return Err(invalid(
            "GRASP byte fields cannot represent this cell/range",
        ));
    }
    let mut out = allocated(total, b)?;
    out[..2].copy_from_slice(&(total as u16).to_le_bytes());
    out[2..7].copy_from_slice(&[
        count as u8,
        first as u8,
        c.width as u8,
        c.height as u8,
        n as u8,
    ]);
    for (i, raw) in out[7..].chunks_exact_mut(n).enumerate() {
        raw.copy_from_slice(
            raster(map.get(&(first + i as u32)).copied(), c, &o.limits, b)?.as_bytes(),
        );
    }
    Ok(out)
}
