use super::{Limits, bounded, buffer, bytes, invalid, p16, p32, to16, to32, u16_at, u32_at};
use crate::Result;
use read_fonts::FontRef;
use std::collections::BTreeMap;

const CHECKSUM_MAGIC: u32 = 0xb1b0_afba;
/// Single-font sfnt flavor. Collection and web wrappers are handled separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// OpenType TrueType outlines or bitmap sfnt (0x00010000).
    TrueType,
    /// OpenType CFF/CFF2 (`OTTO`).
    Cff,
    /// Legacy Apple TrueType (`true`), accepted for lossless envelope rewriting.
    AppleTrueType,
}
impl Flavor {
    /// Four-byte sfnt version.
    pub const fn version(self) -> u32 {
        match self {
            Self::TrueType => 0x10000,
            Self::Cff => 0x4f54544f,
            Self::AppleTrueType => 0x74727565,
        }
    }
    fn parse(n: u32) -> Result<Self> {
        match n {
            0x10000 => Ok(Self::TrueType),
            0x4f54544f => Ok(Self::Cff),
            0x74727565 => Ok(Self::AppleTrueType),
            _ => Err(invalid(
                "unsupported sfnt version (collections/web wrappers require separate decoding)",
            )),
        }
    }
}
/// The wrapping sum of big-endian 32-bit words; a short last word is zero padded.
pub fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, word| {
        let mut b = [0; 4];
        b[..word.len()].copy_from_slice(word);
        sum.wrapping_add(u32::from_be_bytes(b))
    })
}
fn table_checksum(tag: [u8; 4], data: &[u8]) -> Result<u32> {
    let sum = checksum(data);
    if tag == *b"head" {
        Ok(sum.wrapping_sub(u32_at(data, 8)?))
    } else {
        Ok(sum)
    }
}
fn valid_tag(tag: [u8; 4]) -> bool {
    tag[0] != b' '
        && tag.iter().all(|b| (0x20..=0x7e).contains(b))
        && tag
            .iter()
            .position(|b| *b == b' ')
            .is_none_or(|i| tag[i..].iter().all(|b| *b == b' '))
}
fn search_fields(count: usize) -> Result<(u16, u16, u16)> {
    if count == 0 || count > 4095 {
        return Err(invalid("table count must be in 1..=4095"));
    }
    let selector = count.ilog2() as u16;
    let range = 16 * (1 << selector);
    Ok((range, selector, to16(count * 16)? - range))
}
/// A checked directory entry and its borrowed, unpadded bytes.
#[derive(Clone, Copy, Debug)]
pub struct TableRecord<'a> {
    /// Four-byte case-sensitive tag.
    pub tag: [u8; 4],
    /// Stored checksum (not trusted until explicit validation).
    pub checksum: u32,
    /// Offset from the beginning of this standalone sfnt.
    pub offset: u32,
    /// Unpadded table bytes.
    pub data: &'a [u8],
}
/// Borrowed single-font sfnt with fully bounded, nonoverlapping table directory.
pub struct Sfnt<'a> {
    data: &'a [u8],
    flavor: Flavor,
    records: Vec<TableRecord<'a>>,
    font: FontRef<'a>,
}
impl<'a> Sfnt<'a> {
    /// Read the complete directory before exposing any table. Checksums are an
    /// explicit second operation so stale-checksum source fonts remain readable.
    pub fn parse(data: &'a [u8], limits: &Limits) -> Result<Self> {
        bounded(
            data.len(),
            limits.resources.max_input_bytes,
            "sfnt input bytes",
        )?;
        let flavor = Flavor::parse(u32_at(data, 0)?)?;
        let count = u16_at(data, 4)? as usize;
        bounded(count, limits.max_tables, "sfnt tables")?;
        if count == 0 {
            return Err(invalid("empty table directory"));
        }
        let directory_end = 12 + count * 16;
        bytes(data, 0, directory_end)?;
        let font = FontRef::new(data).map_err(invalid)?;
        let mut records = Vec::new();
        records
            .try_reserve_exact(count)
            .map_err(|_| invalid("directory allocation failed"))?;
        let mut tags = std::collections::BTreeSet::new();
        for record in font.table_directory().table_records() {
            let tag = record.tag().to_be_bytes();
            if !valid_tag(tag) || !tags.insert(tag) {
                return Err(invalid("invalid or duplicate table tag"));
            }
            let size = record.length() as usize;
            limits.table(size)?;
            let offset = record.offset() as usize;
            if offset < directory_end {
                return Err(invalid("table overlaps directory"));
            }
            let table = bytes(data, offset, size)?;
            records.push(TableRecord {
                tag,
                checksum: record.checksum(),
                offset: record.offset(),
                data: table,
            });
        }
        let mut spans: Vec<_> = records
            .iter()
            .filter(|r| !r.data.is_empty())
            .map(|r| (r.offset as usize, r.data.len()))
            .collect();
        spans.sort_unstable();
        for pair in spans.windows(2) {
            if pair[0].0 + pair[0].1 > pair[1].0 {
                return Err(invalid("overlapping table data"));
            }
        }
        Ok(Self {
            data,
            flavor,
            records,
            font,
        })
    }
    /// Original flavor.
    pub fn flavor(&self) -> Flavor {
        self.flavor
    }
    /// Original directory order, including all unknown tags.
    pub fn tables(&self) -> &[TableRecord<'a>] {
        &self.records
    }
    /// Borrow an exact unpadded table; no copy or normalization.
    pub fn table(&self, tag: [u8; 4]) -> Option<&'a [u8]> {
        self.records.iter().find(|r| r.tag == tag).map(|r| r.data)
    }
    /// Fontations typed table access. Lazy subtable offsets still return errors;
    /// clients must apply expansion budgets when projecting data into owned maps.
    pub fn font_ref(&self) -> &FontRef<'a> {
        &self.font
    }
    /// Check the envelope, every checksum, padding, and whole-file adjustment.
    /// Does not validate outline programs or claim full OTS conformance.
    pub fn validate_checksums(&self) -> Result<()> {
        let (range, selector, shift) = search_fields(self.records.len())?;
        if (
            u16_at(self.data, 6)?,
            u16_at(self.data, 8)?,
            u16_at(self.data, 10)?,
        ) != (range, selector, shift)
        {
            return Err(invalid("incorrect directory search fields"));
        }
        let mut previous = None;
        for r in &self.records {
            if previous.is_some_and(|p| p >= r.tag) {
                return Err(invalid("unsorted directory"));
            }
            previous = Some(r.tag);
            if r.offset % 4 != 0 {
                return Err(invalid("unaligned table"));
            }
            if table_checksum(r.tag, r.data)? != r.checksum {
                return Err(invalid(format!("bad table checksum: {:?}", r.tag)));
            }
            let pad = (4 - r.data.len() % 4) % 4;
            let end = r.offset as usize + r.data.len();
            if bytes(self.data, end, pad)?.iter().any(|b| *b != 0) {
                return Err(invalid("nonzero table padding"));
            }
        }
        let head = self
            .table(*b"head")
            .ok_or_else(|| invalid("checksum adjustment requires head"))?;
        if head.len() < 54 || u32_at(head, 12)? != 0x5f0f3cf5 {
            return Err(invalid("invalid head header"));
        }
        if checksum(self.data) != CHECKSUM_MAGIC {
            return Err(invalid("bad whole-font checksum adjustment"));
        }
        Ok(())
    }
}
/// Deterministic sfnt assembler, retaining opaque tables when the caller chooses.
#[derive(Clone, Debug)]
pub struct SfntBuilder {
    flavor: Flavor,
    tables: BTreeMap<[u8; 4], Vec<u8>>,
}
impl SfntBuilder {
    /// Start with no tables. `build` requires a head table for checksum adjustment.
    pub fn new(flavor: Flavor) -> Self {
        Self {
            flavor,
            tables: BTreeMap::new(),
        }
    }
    /// Add a table, rejecting duplicates rather than silently replacing data.
    pub fn insert(&mut self, tag: [u8; 4], data: Vec<u8>) -> Result<()> {
        if !valid_tag(tag) {
            return Err(invalid("invalid table tag"));
        }
        if self.tables.contains_key(&tag) {
            return Err(invalid("duplicate table tag"));
        }
        self.tables.insert(tag, data);
        Ok(())
    }
    /// Assemble sorted, aligned tables with zero padding and correct checksums.
    /// Existing head.checkSumAdjustment is always recomputed. Outline-dependent
    /// required tables and their semantic consistency remain the caller's job.
    pub fn build(&self, limits: &Limits) -> Result<Vec<u8>> {
        bounded(self.tables.len(), limits.max_tables, "sfnt tables")?;
        let (range, selector, shift) = search_fields(self.tables.len())?;
        let head = self
            .tables
            .get(b"head")
            .ok_or_else(|| invalid("missing head for checksum adjustment"))?;
        if head.len() < 12 {
            return Err(invalid("truncated head"));
        }
        let mut length = 12 + self.tables.len() * 16;
        for data in self.tables.values() {
            limits.table(data.len())?;
            to32(data.len())?;
            length = length
                .checked_add(
                    data.len()
                        .checked_add(3)
                        .ok_or_else(|| invalid("size overflow"))?
                        & !3,
                )
                .ok_or_else(|| invalid("size overflow"))?;
        }
        to32(length)?;
        bounded(
            length,
            limits.resources.max_total_bytes,
            "sfnt output bytes",
        )?;
        let mut out = buffer(length)?;
        p32(&mut out, self.flavor.version());
        p16(&mut out, to16(self.tables.len())?);
        for n in [range, selector, shift] {
            p16(&mut out, n);
        }
        let mut offset = 12 + self.tables.len() * 16;
        let mut head_offset = 0;
        for (&tag, data) in &self.tables {
            if tag == *b"head" {
                head_offset = offset;
            }
            out.extend(tag);
            p32(&mut out, table_checksum(tag, data)?);
            p32(&mut out, to32(offset)?);
            p32(&mut out, to32(data.len())?);
            offset += (data.len() + 3) & !3;
        }
        for data in self.tables.values() {
            out.extend(data);
            out.resize((out.len() + 3) & !3, 0);
        }
        out[head_offset + 8..head_offset + 12].fill(0);
        let adjustment = CHECKSUM_MAGIC.wrapping_sub(checksum(&out));
        out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
        Ok(out)
    }
}
