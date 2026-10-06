//! Native wyse font reader and writer.
/// Native read interpretation.
pub use super::w5a2_common::NativeReadOptions as WyseReadOptions;
/// Native write projection.
pub use super::w5a2_common::NativeWriteOptions as WyseWriteOptions;
use super::w5a2_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
/// Native wyse codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct WyseCodec {
    /// Reader interpretation.
    pub read_options: WyseReadOptions,
    /// Writer projection and bounds.
    pub write_options: WyseWriteOptions,
}
impl FormatDetector for WyseCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for WyseCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for WyseCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Decode native data under shared resource limits.
pub fn read(c: &mut ReadContext<'_>, o: &WyseReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
/// Encode a deterministic native file and audit its decoded model.
pub fn write(s: &FontSet, o: &WriteOptions, p: &WyseWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = (|| {
        let mut b = source(s, p)?;
        b.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let set = convert_gray(s, p.gray_threshold, &p.limits, &mut b)?;
        let bytes = encode(set.single()?, p, &mut b)?;
        output_check(bytes.len(), &p.limits, &mut b)?;
        let (back, rr) = parse(&bytes, &Default::default(), &p.limits, &mut b)?;
        r.extend(rr);
        audit(s, &back, "wyse", &mut r, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), bytes))
    })();
    finish(result, r, "wyse", o.strict)
}
fn nibble(v: u8) -> Result<u8> {
    match v {
        b'0'..=b'9' => Ok(v - b'0'),
        b'a'..=b'f' => Ok(v - b'a' + 10),
        b'A'..=b'F' => Ok(v - b'A' + 10),
        _ => Err(invalid("invalid Wyse hexadecimal digit")),
    }
}
fn hex(v: &[u8]) -> Result<u8> {
    Ok((nibble(v[0])? << 4) | nibble(v[1])?)
}
/// Recognize native download commands, without terminal execution.
pub fn detect(s: &[u8]) -> Confidence {
    if s.starts_with(b"\x1bcA") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn parse(
    s: &[u8],
    o: &WyseReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "Wyse input bytes")?;
    if s.is_empty() || !s.len().is_multiple_of(39) {
        return Err(invalid("truncated Wyse download command"));
    }
    let mut f = base("wyse", o, b)?;
    let mut r = Report::default();
    let mut map = std::collections::BTreeMap::new();
    for v in s.as_chunks::<39>().0.iter() {
        if &v[..3] != b"\x1bcA" || !(b'0'..=b'3').contains(&v[3]) || v[38] != 25 {
            return Err(invalid("invalid Wyse command, bank or terminator"));
        }
        let cp = hex(&v[4..6])?;
        if cp > 127 {
            return Err(invalid("Wyse character position exceeds 7f"));
        }
        let mut raw = [0u8; 16];
        for (row, pair) in raw.iter_mut().zip(v[6..38].as_chunks::<2>().0.iter()) {
            *row = hex(pair)?;
        }
        b.add(96)?;
        let code = u32::from(v[3] - b'0') * 256 + u32::from(cp);
        if map.insert(code, raw).is_some() {
            b.loss(
                &mut r,
                "wyse",
                "read.redefined",
                Some(0),
                None,
                "Later download replaces an earlier character",
            )?;
        }
    }
    for (cp, raw) in map {
        glyph(&mut f, Codepoint::Legacy(cp), 8, 16, &raw, o, l, b)?;
    }
    Ok((complete(f, "wyse")?, r))
}
fn encode(f: &Font, o: &WyseWriteOptions, b: &mut Budget) -> Result<Vec<u8>> {
    let c = cell(f, o, Some((8, 16)))?;
    let map = legacy_map(f, o, 0x37f, b)?;
    if map.keys().any(|cp| cp % 256 > 127) {
        return Err(invalid("Wyse character position exceeds 7f"));
    }
    let mut out = allocated(map.len() * 39, b)?;
    let digits = b"0123456789ABCDEF";
    for ((cp, g), v) in map.iter().zip(out.as_chunks_mut::<39>().0.iter_mut()) {
        v[..3].copy_from_slice(b"\x1bcA");
        v[3] = b'0' + (cp / 256) as u8;
        v[4] = digits[(cp % 256 / 16) as usize];
        v[5] = digits[(cp % 16) as usize];
        for (raw, pair) in raster(Some(g), c, &o.limits, b)?
            .as_bytes()
            .iter()
            .zip(v[6..38].as_chunks_mut::<2>().0.iter_mut())
        {
            pair[0] = digits[(raw >> 4) as usize];
            pair[1] = digits[(raw & 15) as usize];
        }
        v[38] = 25;
    }
    Ok(out)
}
