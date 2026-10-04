//! Native bbc font reader and writer.
/// Native read interpretation.
pub use super::w5a2_common::NativeReadOptions as BbcReadOptions;
/// Native write projection.
pub use super::w5a2_common::NativeWriteOptions as BbcWriteOptions;
use super::w5a2_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
/// Native bbc codec.
#[derive(Debug, Clone, Default)]
pub struct BbcCodec {
    /// Reader interpretation.
    pub read_options: BbcReadOptions,
    /// Writer projection and bounds.
    pub write_options: BbcWriteOptions,
}
impl FormatDetector for BbcCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for BbcCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for BbcCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Decode native data under shared resource limits.
pub fn read(c: &mut ReadContext<'_>, o: &BbcReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
/// Encode a deterministic native file and audit its decoded model.
pub fn write(s: &FontSet, o: &WriteOptions, p: &BbcWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = (|| {
        let mut b = source(s, p)?;
        b.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let set = convert_gray(s, p.gray_threshold, &p.limits, &mut b)?;
        let bytes = encode(set.single()?, p, &mut b)?;
        output_check(bytes.len(), &p.limits, &mut b)?;
        let (back, rr) = parse(&bytes, &Default::default(), &p.limits, &mut b)?;
        r.extend(rr);
        audit(s, &back, "bbc", &mut r, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), bytes))
    })();
    finish(result, r, "bbc", o.strict)
}
/// Recognize a complete stream of printable VDU 23 definitions.
pub fn detect(s: &[u8]) -> Confidence {
    if !s.is_empty()
        && s.len().is_multiple_of(10)
        && s.as_chunks::<10>()
            .0
            .iter()
            .all(|v| v[0] == 23 && v[1] >= 32)
    {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn parse(
    s: &[u8],
    o: &BbcReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "BBC input bytes")?;
    if detect(s) == Confidence::NoMatch {
        return Err(invalid(
            "BBC requires complete printable VDU 23 records only",
        ));
    }
    let mut f = base("bbc", o, b)?;
    let mut r = Report::default();
    let mut records = std::collections::BTreeMap::new();
    for v in s.as_chunks::<10>().0.iter() {
        b.add(64)?;
        if records.insert(v[1], &v[2..]).is_some() {
            b.loss(
                &mut r,
                "bbc",
                "read.redefined",
                Some(0),
                None,
                "Later VDU 23 definition replaces an earlier character",
            )?;
        }
    }
    for (cp, raw) in records {
        glyph(&mut f, Codepoint::Legacy(u32::from(cp)), 8, 8, raw, o, l, b)?;
    }
    Ok((complete(f, "bbc")?, r))
}
fn encode(f: &Font, o: &BbcWriteOptions, b: &mut Budget) -> Result<Vec<u8>> {
    let c = cell(f, o, Some((8, 8)))?;
    let map = legacy_map(f, o, 255, b)?;
    let count = map.range(32..).count();
    if count == 0 {
        return Err(invalid("BBC has no programmable characters"));
    }
    let mut out = allocated(count * 10, b)?;
    for ((cp, g), v) in map.range(32..).zip(out.as_chunks_mut::<10>().0.iter_mut()) {
        v[0] = 23;
        v[1] = *cp as u8;
        v[2..].copy_from_slice(raster(Some(g), c, &o.limits, b)?.as_bytes());
    }
    Ok(out)
}
