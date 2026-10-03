//! Bounded standalone sfnt ↔ WOFF1/WOFF2 containers.
//!
//! These codecs preserve sfnt tables and auxiliary data. They do not rasterize
//! outlines or imply that a FontSet projection is available for every sfnt.
use crate::report::{Impact, Level};
use crate::{Error, Report, ReportEntry, Result, sfnt};
use std::io::Write;
#[cfg(feature = "woff")]
pub mod woff1;
#[cfg(feature = "woff2")]
pub mod woff2;

/// Byte and transform budgets; auxiliary blocks share the output budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Common sfnt/input/decompression/output/table/glyph limits.
    pub sfnt: sfnt::Limits,
    /// Maximum points in one glyph (TrueType also imposes 65536).
    pub max_points_per_glyph: usize,
    /// Cumulative simple points across one font.
    pub max_total_points: usize,
    /// Cumulative component records, without recursively expanding them.
    pub max_total_components: usize,
    /// Cumulative instruction bytes.
    pub max_total_instructions: usize,
    /// Maximum uncompressed metadata and private-data bytes each.
    pub max_auxiliary_bytes: usize,
    /// Maximum fonts queued by a multi-font compression call.
    pub max_batch_fonts: usize,
    /// Maximum aggregate input bytes and, separately, retained output bytes for
    /// one multi-font call. Per-font working memory still scales with workers.
    pub max_batch_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sfnt: sfnt::Limits::default(),
            max_points_per_glyph: 65536,
            max_total_points: 16 * 1024 * 1024,
            max_total_components: 4 * 1024 * 1024,
            max_total_instructions: 16 * 1024 * 1024,
            max_auxiliary_bytes: 16 * 1024 * 1024,
            max_batch_fonts: 1024,
            max_batch_bytes: 256 * 1024 * 1024,
        }
    }
}
/// Non-sfnt WOFF header fields and optional data, independently editable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Auxiliary {
    /// Major version, default zero (no timestamps).
    pub major_version: u16,
    /// Minor version, default zero.
    pub minor_version: u16,
    /// Uncompressed WOFF metadata XML. Callers own XML schema validity.
    pub metadata: Option<Vec<u8>>,
    /// Opaque vendor-private block.
    pub private_data: Option<Vec<u8>>,
}
/// Reconstructed standalone sfnt plus preserved envelope fields and diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// Canonical sfnt with recomputed checksums.
    pub sfnt: Vec<u8>,
    /// Version, metadata and vendor-private bytes.
    pub auxiliary: Auxiliary,
    /// Metadata decoding issues; invalid font data is always an error.
    pub report: Report,
}
/// Encoded webfont and any unavoidable conversion losses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    /// Complete webfont file bytes.
    pub bytes: Vec<u8>,
    /// For example, removal of an invalidated digital signature.
    pub report: Report,
}
fn invalid(s: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("WOFF: {s}"))
}
fn bounded(n: usize, limit: usize, resource: &'static str) -> Result<()> {
    if n > limit {
        Err(Error::ResourceLimit { resource, limit })
    } else {
        Ok(())
    }
}
fn slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|e| data.get(offset..e))
        .ok_or_else(|| invalid("truncated data or offset overflow"))
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        slice(data, offset, 2)?.try_into().map_err(invalid)?,
    ))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        slice(data, offset, 4)?.try_into().map_err(invalid)?,
    ))
}
#[cfg(feature = "woff2")]
fn p16(v: &mut Vec<u8>, n: u16) {
    v.extend(n.to_be_bytes());
}
#[cfg(feature = "woff2")]
fn p32(v: &mut Vec<u8>, n: u32) {
    v.extend(n.to_be_bytes());
}
fn as32(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| invalid("32-bit length overflow"))
}
fn align(n: usize) -> Result<usize> {
    Ok(n.checked_add(3).ok_or_else(|| invalid("length overflow"))? & !3)
}
fn buffer(n: usize) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| invalid("allocation failed"))?;
    Ok(v)
}
fn loss(r: &mut Report, code: &str, message: impl Into<String>) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: code.into(),
        location: Default::default(),
        message: message.into(),
    });
}
fn flavor(n: u32) -> Result<sfnt::Flavor> {
    match n {
        0x10000 => Ok(sfnt::Flavor::TrueType),
        0x4f54544f => Ok(sfnt::Flavor::Cff),
        0x74727565 => Ok(sfnt::Flavor::AppleTrueType),
        0x74746366 => Err(invalid(
            "WOFF collections require the collection codec (W2.7)",
        )),
        _ => Err(invalid("unsupported sfnt flavor")),
    }
}
fn valid_tag(t: [u8; 4]) -> bool {
    t[0] != b' '
        && t.iter().all(|b| (0x20..=0x7e).contains(b))
        && t.iter()
            .position(|b| *b == b' ')
            .is_none_or(|i| t[i..].iter().all(|b| *b == b' '))
}
fn input(data: &[u8], magic: &[u8; 4], header: usize, l: &Limits) -> Result<(sfnt::Flavor, usize)> {
    bounded(
        data.len(),
        l.sfnt.resources.max_input_bytes,
        "WOFF input bytes",
    )?;
    if slice(data, 0, header)?[..4] != *magic || u32_at(data, 8)? as usize != data.len() {
        return Err(invalid("wrong signature or file length"));
    }
    let f = flavor(u32_at(data, 4)?)?;
    let n = u16_at(data, 12)? as usize;
    bounded(n, l.sfnt.max_tables.min(4095), "WOFF tables")?;
    if n == 0 {
        return Err(invalid("empty directory"));
    }
    Ok((f, n))
}
fn table_sum(n: usize, lengths: impl Iterator<Item = usize>, l: &Limits) -> Result<usize> {
    bounded(n, l.sfnt.max_tables.min(4095), "WOFF tables")?;
    let mut total = 12 + n * 16;
    for len in lengths {
        bounded(len, l.sfnt.resources.max_table_bytes, "WOFF table bytes")?;
        total = total
            .checked_add(align(len)?)
            .ok_or_else(|| invalid("font size overflow"))?;
    }
    bounded(
        total,
        l.sfnt.resources.max_total_bytes,
        "WOFF reconstructed bytes",
    )?;
    Ok(total)
}
/// Shared bounded compressed writer, preventing allocation beyond output limit.
struct Output {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Output {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if data.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("WOFF compressed output limit"));
        }
        self.bytes
            .try_reserve(data.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[cfg(feature = "woff")]
fn checksum(tag: [u8; 4], data: &[u8]) -> Result<u32> {
    Ok(sfnt::checksum(data).wrapping_sub(if tag == *b"head" { u32_at(data, 8)? } else { 0 }))
}
fn pad(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}
fn set32(out: &mut [u8], pos: usize, value: usize) -> Result<()> {
    out[pos..pos + 4].copy_from_slice(&as32(value)?.to_be_bytes());
    Ok(())
}
fn checked_aux(a: &Auxiliary, l: &Limits) -> Result<()> {
    for bytes in [&a.metadata, &a.private_data].into_iter().flatten() {
        bounded(bytes.len(), l.max_auxiliary_bytes, "WOFF auxiliary bytes")?;
        if bytes.is_empty() {
            return Err(invalid("present auxiliary blocks must not be empty"));
        }
    }
    Ok(())
}
// Optional block offsets/layout are structural, while invalid metadata compression
// only drops metadata. Resource-limit errors still stop before decompression.
fn read_aux(
    data: &[u8],
    header_offset: usize,
    mut end: usize,
    l: &Limits,
    decompress: impl Fn(&[u8], usize) -> Result<Vec<u8>>,
) -> Result<(Auxiliary, Report)> {
    let o = header_offset;
    let mut a = Auxiliary {
        major_version: u16_at(data, o)?,
        minor_version: u16_at(data, o + 2)?,
        ..Default::default()
    };
    let (mo, ml, original) = (
        u32_at(data, o + 4)? as usize,
        u32_at(data, o + 8)? as usize,
        u32_at(data, o + 12)? as usize,
    );
    let (po, pl) = (
        u32_at(data, o + 16)? as usize,
        u32_at(data, o + 20)? as usize,
    );
    let mut report = Report::default();
    if mo == 0 {
        if ml != 0 || original != 0 {
            return Err(invalid("inconsistent metadata fields"));
        }
    } else {
        bounded(original, l.max_auxiliary_bytes, "WOFF metadata bytes")?;
        if ml == 0 || original == 0 || mo != align(end)? {
            return Err(invalid("invalid metadata span"));
        }
        if slice(data, end, mo - end)?.iter().any(|b| *b != 0) {
            return Err(invalid("nonzero block padding"));
        }
        match decompress(slice(data, mo, ml)?, original) {
            Ok(meta) => a.metadata = Some(meta),
            Err(e) => loss(
                &mut report,
                "woff.invalid_metadata",
                format!("Invalid compressed metadata omitted: {e}"),
            ),
        }
        end = mo + ml;
    }
    if po == 0 {
        if pl != 0 {
            return Err(invalid("inconsistent private-data fields"));
        }
    } else {
        bounded(pl, l.max_auxiliary_bytes, "WOFF private-data bytes")?;
        if pl == 0 || po != align(end)? {
            return Err(invalid("invalid private-data span"));
        }
        if slice(data, end, po - end)?.iter().any(|b| *b != 0) {
            return Err(invalid("nonzero block padding"));
        }
        a.private_data = Some(slice(data, po, pl)?.to_vec());
        end = po + pl;
    }
    // WOFF1 table padding or a final optional block may have up to 3 zero bytes.
    if data.len() != end
        && (po != 0
            || data.len() != align(end)?
            || slice(data, end, data.len().saturating_sub(end))?
                .iter()
                .any(|b| *b != 0))
    {
        return Err(invalid("extraneous trailing data"));
    }
    Ok((a, report))
}
fn write_aux(
    out: &mut Vec<u8>,
    offset: usize,
    a: &Auxiliary,
    l: &Limits,
    compress: impl Fn(&[u8]) -> Result<Vec<u8>>,
) -> Result<()> {
    checked_aux(a, l)?;
    out[offset..offset + 2].copy_from_slice(&a.major_version.to_be_bytes());
    out[offset + 2..offset + 4].copy_from_slice(&a.minor_version.to_be_bytes());
    if let Some(meta) = &a.metadata {
        let encoded = compress(meta)?;
        pad(out);
        let pos = out.len();
        set32(out, offset + 4, pos)?;
        set32(out, offset + 8, encoded.len())?;
        set32(out, offset + 12, meta.len())?;
        bounded(
            out.len().saturating_add(encoded.len()),
            l.sfnt.resources.max_total_bytes,
            "WOFF output bytes",
        )?;
        out.extend(encoded);
    }
    if let Some(private) = &a.private_data {
        pad(out);
        let pos = out.len();
        set32(out, offset + 16, pos)?;
        set32(out, offset + 20, private.len())?;
        bounded(
            out.len().saturating_add(private.len()),
            l.sfnt.resources.max_total_bytes,
            "WOFF output bytes",
        )?;
        out.extend(private);
    }
    let size = out.len();
    bounded(size, l.sfnt.resources.max_total_bytes, "WOFF output bytes")?;
    set32(out, 8, size)?;
    Ok(())
}
