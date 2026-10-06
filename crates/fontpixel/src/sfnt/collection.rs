//! Bounded TTC/OTC v1/v2 containers, face extraction and deterministic table sharing.
use super::{
    Flavor, Sfnt, SfntBuilder, TableRecord, bounded, bytes, checksum, invalid, u16_at, u32_at,
};
use crate::{
    Report, ReportEntry, Result,
    report::{Impact, Level},
};
use std::collections::{BTreeMap, BTreeSet};

/// Collection-wide budgets, including repeated references and extracted faces.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct Limits {
    /// Per-face common limits.
    pub sfnt: super::Limits,
    /// Maximum collection faces.
    pub max_faces: usize,
    /// Maximum cumulative table-directory records.
    pub max_table_references: usize,
    /// Maximum sum of extracted standalone face sizes.
    pub max_expanded_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sfnt: super::Limits::default(),
            max_faces: 1024,
            max_table_references: 65536,
            max_expanded_bytes: 256 * 1024 * 1024,
        }
    }
}
/// A borrowed face, retaining exact table bytes and collection-relative offsets.
#[derive(Debug)]
pub struct Face<'a> {
    /// Face outline flavor.
    pub flavor: Flavor,
    /// Source face directory offset.
    pub offset: u32,
    /// Source-order table records.
    pub tables: Vec<TableRecord<'a>>,
}
/// Checked collection. Parsing checks every face before exposing any of them.
#[derive(Debug)]
pub struct Collection<'a> {
    /// TTC version 0x00010000 or 0x00020000.
    pub version: u32,
    /// Ordered collection faces.
    pub faces: Vec<Face<'a>>,
    /// Bounded opaque v2 signature; no claim of cryptographic validity.
    pub signature: Option<&'a [u8]>,
}
fn tag_ok(tag: [u8; 4]) -> bool {
    tag[0] != b' '
        && tag.iter().all(|b| (0x20..=0x7e).contains(b))
        && tag
            .iter()
            .position(|b| *b == b' ')
            .is_none_or(|i| tag[i..].iter().all(|b| *b == b' '))
}
fn flavor(n: u32) -> Result<Flavor> {
    match n {
        0x10000 => Ok(Flavor::TrueType),
        0x4f54544f => Ok(Flavor::Cff),
        0x74727565 => Ok(Flavor::AppleTrueType),
        _ => Err(invalid("invalid collection face flavor")),
    }
}
fn align(n: usize) -> Result<usize> {
    Ok(n.checked_add(3)
        .ok_or_else(|| invalid("collection length overflow"))?
        & !3)
}
fn note(report: &mut Report) {
    report.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: "ttc.dsig_removed".into(),
        location: Default::default(),
        message: "Reassembly invalidates the digital signature; DSIG was removed".into(),
    });
}
impl<'a> Collection<'a> {
    /// Validate structure and aggregate allocation bounds. Table checksums are
    /// intentionally separate, like the standalone sfnt reader.
    pub fn parse(data: &'a [u8], l: &Limits) -> Result<Self> {
        bounded(
            data.len(),
            l.sfnt.resources.max_input_bytes,
            "TTC input bytes",
        )?;
        if bytes(data, 0, 4)? != b"ttcf" {
            return Err(invalid("not a TTC/OTC collection"));
        }
        let version = u32_at(data, 4)?;
        if ![0x10000, 0x20000].contains(&version) {
            return Err(invalid("unsupported TTC version"));
        }
        let count = u32_at(data, 8)? as usize;
        bounded(count, l.max_faces, "TTC faces")?;
        if count == 0 {
            return Err(invalid("empty TTC collection"));
        }
        let head_end = 12usize
            .checked_add(
                count
                    .checked_mul(4)
                    .ok_or_else(|| invalid("TTC count overflow"))?,
            )
            .and_then(|n| n.checked_add(if version == 0x20000 { 12 } else { 0 }))
            .ok_or_else(|| invalid("TTC header overflow"))?;
        bytes(data, 0, head_end)?;
        // (start,end,kind): directory/header=0, table=1, signature=2.
        let mut spans = vec![(0, head_end, 0u8)];
        let signature = if version == 0x20000 {
            let p = 12 + count * 4;
            let tag = u32_at(data, p)?;
            let len = u32_at(data, p + 4)? as usize;
            let off = u32_at(data, p + 8)? as usize;
            if tag == 0 && len == 0 && off == 0 {
                None
            } else {
                if tag != u32::from_be_bytes(*b"DSIG") || len == 0 || !off.is_multiple_of(4) {
                    return Err(invalid("invalid TTC DSIG fields"));
                }
                l.sfnt.table(len)?;
                let sig = bytes(data, off, len)?;
                spans.push((off, off + len, 2));
                Some(sig)
            }
        } else {
            None
        };
        let mut faces = Vec::with_capacity(count);
        let mut refs = 0usize;
        let mut expanded = 0usize;
        for i in 0..count {
            let off = u32_at(data, 12 + i * 4)? as usize;
            if !off.is_multiple_of(4) {
                return Err(invalid("unaligned TTC face directory"));
            }
            let f = flavor(u32_at(data, off)?)?;
            let n = u16_at(data, off + 4)? as usize;
            bounded(n, l.sfnt.max_tables.min(4095), "TTC face tables")?;
            if n == 0 {
                return Err(invalid("empty TTC face directory"));
            }
            refs = refs
                .checked_add(n)
                .ok_or_else(|| invalid("TTC records overflow"))?;
            bounded(refs, l.max_table_references, "TTC table references")?;
            let end = off
                .checked_add(12 + n * 16)
                .ok_or_else(|| invalid("TTC directory overflow"))?;
            bytes(data, off, end - off)?;
            spans.push((off, end, 0));
            expanded = expanded
                .checked_add(12 + n * 16)
                .ok_or_else(|| invalid("TTC expansion overflow"))?;
            let mut tables = Vec::with_capacity(n);
            let mut tags = BTreeSet::new();
            for j in 0..n {
                let p = off + 12 + j * 16;
                let tag: [u8; 4] = bytes(data, p, 4)?.try_into().map_err(invalid)?;
                if !tag_ok(tag) || !tags.insert(tag) {
                    return Err(invalid("invalid/duplicate TTC table tag"));
                }
                let start = u32_at(data, p + 8)? as usize;
                let len = u32_at(data, p + 12)? as usize;
                l.sfnt.table(len)?;
                if !start.is_multiple_of(4) {
                    return Err(invalid("unaligned TTC table"));
                }
                bytes(data, start, align(len)?)?;
                let table = bytes(data, start, len)?;
                if len > 0 {
                    spans.push((start, start + len, 1));
                }
                expanded = expanded
                    .checked_add(align(len)?)
                    .ok_or_else(|| invalid("TTC expansion overflow"))?;
                bounded(
                    expanded,
                    l.max_expanded_bytes.min(l.sfnt.resources.max_total_bytes),
                    "TTC expanded bytes",
                )?;
                tables.push(TableRecord {
                    tag,
                    checksum: u32_at(data, p + 4)?,
                    offset: start as u32,
                    data: table,
                });
            }
            faces.push(Face {
                flavor: f,
                offset: off as u32,
                tables,
            });
        }
        spans.sort_unstable();
        for s in spans.windows(2) {
            if s[0].1 > s[1].0 && !(s[0] == s[1] && s[0].2 != 2) {
                return Err(invalid("overlapping TTC structures or partial table alias"));
            }
        }
        Ok(Self {
            version,
            faces,
            signature,
        })
    }
    /// Verify each table checksum (head adjustment is excluded by specification).
    pub fn validate_checksums(&self) -> Result<()> {
        for face in &self.faces {
            for r in &face.tables {
                let sum = if r.tag == *b"head" {
                    checksum(r.data).wrapping_sub(u32_at(r.data, 8)?)
                } else {
                    checksum(r.data)
                };
                if sum != r.checksum {
                    return Err(invalid("TTC table checksum mismatch"));
                }
            }
        }
        Ok(())
    }
    /// Extract any face to a standalone sfnt, repairing envelope checksums and
    /// removing stale signatures with explicit diagnostics.
    pub fn extract(&self, index: usize, l: &Limits) -> Result<(Vec<u8>, Report)> {
        let face = self
            .faces
            .get(index)
            .ok_or_else(|| invalid("TTC face index out of range"))?;
        let mut builder = SfntBuilder::new(face.flavor);
        let mut report = Report::default();
        let mut signed = self.signature.is_some();
        for t in &face.tables {
            if t.tag == *b"DSIG" {
                signed = true;
            } else {
                builder.insert(t.tag, t.data.to_vec())?;
            }
        }
        if signed {
            note(&mut report);
        }
        Ok((builder.build(&l.sfnt)?, report))
    }
}
/// Deterministically package standalone sfnts. Exact same-tag table payloads
/// share storage. v2 output has an absent DSIG triple; signatures are not forged.
pub fn encode(fonts: &[&[u8]], version: u32, l: &Limits) -> Result<(Vec<u8>, Report)> {
    if ![0x10000, 0x20000].contains(&version) || fonts.is_empty() {
        return Err(invalid("invalid TTC version or no faces"));
    }
    bounded(fonts.len(), l.max_faces, "TTC faces")?;
    let mut input_total = 0usize;
    for f in fonts {
        input_total = input_total
            .checked_add(f.len())
            .ok_or_else(|| invalid("TTC input overflow"))?;
    }
    bounded(
        input_total,
        l.max_expanded_bytes.min(l.sfnt.resources.max_total_bytes),
        "TTC expanded bytes",
    )?;
    let parsed = fonts
        .iter()
        .map(|f| Sfnt::parse(f, &l.sfnt))
        .collect::<Result<Vec<_>>>()?;
    let mut report = Report::default();
    let mut refs = 0usize;
    let mut directory_end = 12 + fonts.len() * 4 + if version == 0x20000 { 12 } else { 0 };
    let mut records = Vec::new();
    let mut shared = BTreeMap::<([u8; 4], usize, u64), Vec<usize>>::new();
    let mut payloads = Vec::<([u8; 4], Vec<u8>)>::new();
    for f in &parsed {
        let mut rows = Vec::new();
        for t in f.tables() {
            if t.tag == *b"DSIG" {
                if !report.has_loss() {
                    note(&mut report);
                }
                continue;
            }
            let mut data = t.data.to_vec();
            if t.tag == *b"head" {
                bytes(&data, 0, 12)?;
                data[8..12].fill(0);
            }
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            data.hash(&mut hash);
            let key = (t.tag, data.len(), hash.finish());
            let id = if let Some(id) = shared
                .get(&key)
                .and_then(|ids| ids.iter().find(|&&i| payloads[i].1 == data))
            {
                *id
            } else {
                let id = payloads.len();
                payloads.push((t.tag, data));
                shared.entry(key).or_default().push(id);
                id
            };
            rows.push((t.tag, id));
        }
        rows.sort_unstable();
        refs = refs
            .checked_add(rows.len())
            .ok_or_else(|| invalid("TTC record overflow"))?;
        bounded(refs, l.max_table_references, "TTC table references")?;
        directory_end = directory_end
            .checked_add(12 + rows.len() * 16)
            .ok_or_else(|| invalid("TTC directory overflow"))?;
        records.push(rows);
    }
    let mut total = directory_end;
    let mut offsets = Vec::with_capacity(payloads.len());
    for (_, p) in &payloads {
        offsets.push(total);
        total = total
            .checked_add(align(p.len())?)
            .ok_or_else(|| invalid("TTC size overflow"))?;
    }
    bounded(total, l.sfnt.resources.max_total_bytes, "TTC output bytes")?;
    let _ = u32::try_from(total).map_err(invalid)?;
    let mut out = vec![0; total];
    out[..4].copy_from_slice(b"ttcf");
    out[4..8].copy_from_slice(&version.to_be_bytes());
    out[8..12].copy_from_slice(&(fonts.len() as u32).to_be_bytes());
    let mut off = 12 + fonts.len() * 4 + if version == 0x20000 { 12 } else { 0 };
    for (i, (f, rows)) in parsed.iter().zip(records).enumerate() {
        out[12 + i * 4..16 + i * 4].copy_from_slice(&(off as u32).to_be_bytes());
        out[off..off + 4].copy_from_slice(&f.flavor().version().to_be_bytes());
        let n = rows.len();
        if n == 0 {
            return Err(invalid("no unsigned TTC face tables"));
        }
        let selector = n.ilog2() as u16;
        let range = 16u16 * (1 << selector);
        for (at, v) in [
            (4, n as u16),
            (6, range),
            (8, selector),
            (10, n as u16 * 16 - range),
        ] {
            out[off + at..off + at + 2].copy_from_slice(&v.to_be_bytes());
        }
        for (j, (tag, id)) in rows.iter().enumerate() {
            let p = off + 12 + j * 16;
            let data = &payloads[*id].1;
            out[p..p + 4].copy_from_slice(tag);
            out[p + 4..p + 8].copy_from_slice(&checksum(data).to_be_bytes());
            out[p + 8..p + 12].copy_from_slice(&(offsets[*id] as u32).to_be_bytes());
            out[p + 12..p + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        }
        off += 12 + n * 16;
    }
    for ((_, p), off) in payloads.iter().zip(offsets) {
        out[off..off + p.len()].copy_from_slice(p);
    }
    Ok((out, report))
}
