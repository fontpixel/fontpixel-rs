//! WOFF 1.0 table-by-table zlib coding.
//!
//! Each public call shares one working ledger (`max_total_bytes`) across every
//! owned stage: directories, decompressed tables, canonical sfnt, compressed
//! tables, the fixed zlib workspace and the growable output.
use super::zlib::{compress, decompress};
use super::*;
use crate::working::Vector;

/// Deterministic WOFF1 output options.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct Options {
    /// zlib level in 0..=9 (default 9).
    pub compression_level: u32,
    /// Envelope version and optional blocks.
    pub auxiliary: Auxiliary,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            compression_level: 9,
            auxiliary: Auxiliary::default(),
        }
    }
}
type Borrowed<'a> = ([u8; 4], &'a [u8], usize);
// WOFF1 §6 preserves physical table order independently of its tag-sorted
// directory. SfntBuilder intentionally sorts physical tables, so this wrapper
// needs an order-preserving assembler instead. Returns the new head adjustment.
fn assemble_ordered(
    flavor: sfnt::Flavor,
    tables: &[Borrowed<'_>],
    l: &Limits,
    budget: &Budget,
) -> Result<(Owned<Vec<u8>>, u32)> {
    let n = tables.len();
    if n == 0 {
        return Err(invalid("empty sfnt"));
    }
    let total = table_sum(n, tables.iter().map(|t| t.1.len()), l)?;
    as32(total)?;
    let mut physical = Vector::with_capacity(budget, n)?;
    let mut directory = Vector::with_capacity(budget, n)?;
    for i in 0..n {
        physical.push(i)?;
        directory.push(i)?;
    }
    physical.sort_unstable_by_key(|&i| (tables[i].2, tables[i].1.len(), tables[i].0));
    directory.sort_unstable_by_key(|&i| tables[i].0);
    let mut offsets = Vector::with_capacity(budget, n)?;
    offsets.resize(n, 0)?;
    let mut offset = 12 + n * 16;
    for &i in physical.iter() {
        offsets[i] = offset;
        offset += align(tables[i].1.len())?;
    }
    let head = tables
        .iter()
        .position(|t| t.0 == *b"head")
        .ok_or_else(|| invalid("missing head"))?;
    slice(tables[head].1, 0, 12)?;
    let head_offset = offsets[head];
    let selector = n.ilog2() as u16;
    let search_range = 16usize << selector;
    let mut out = Buffer::with_capacity(budget, total)?;
    p32(&mut out, flavor.version())?;
    p16(&mut out, n as u16)?;
    p16(&mut out, search_range as u16)?;
    p16(&mut out, selector)?;
    p16(&mut out, (n * 16 - search_range) as u16)?;
    for &i in directory.iter() {
        let (tag, data, _) = tables[i];
        put(&mut out, &tag)?;
        p32(&mut out, checksum(tag, data)?)?;
        p32(&mut out, as32(offsets[i])?)?;
        p32(&mut out, as32(data.len())?)?;
    }
    for &i in physical.iter() {
        put(&mut out, tables[i].1)?;
        pad(&mut out)?;
    }
    out[head_offset + 8..head_offset + 12].fill(0);
    let adjustment = 0xb1b0_afbau32.wrapping_sub(sfnt::checksum(&out));
    out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok((out.into_owned(), adjustment))
}
fn source_tables<'a>(f: &sfnt::Sfnt<'a>, budget: &Budget) -> Result<Vector<Borrowed<'a>>> {
    let mut tables = Vector::with_capacity(budget, f.tables().len())?;
    for t in f.tables() {
        tables.push((t.tag, t.data, t.offset as usize))?;
    }
    Ok(tables)
}
/// Remove DSIG in place, preserving the order of all other entries.
fn without_dsig(tables: &mut Vector<Borrowed<'_>>) {
    let mut kept = 0;
    for i in 0..tables.len() {
        if tables[i].0 != *b"DSIG" {
            tables[kept] = tables[i];
            kept += 1;
        }
    }
    tables.truncate(kept);
}
fn hidden_data(data: &[u8], f: &sfnt::Sfnt<'_>, budget: &Budget) -> Result<bool> {
    let mut tables = Vector::with_capacity(budget, f.tables().len())?;
    for t in f.tables() {
        tables.push(t)?;
    }
    tables.sort_unstable_by_key(|t| t.offset);
    let mut end = 12 + tables.len() * 16;
    for t in tables.iter() {
        let start = t.offset as usize;
        if start > end && data[end..start].iter().any(|b| *b != 0) {
            return Ok(true);
        }
        end = end.max(start + t.data.len());
    }
    Ok(data[end..].iter().any(|b| *b != 0))
}
/// Decode WOFF1 into standalone sfnt, preserving optional blocks.
pub fn decode(data: &[u8], l: &Limits) -> Result<Decoded> {
    decode_with_budget(data, l, &ledger(l))
}
pub(super) fn decode_with_budget(data: &[u8], l: &Limits, budget: &Budget) -> Result<Decoded> {
    let (flavor, n) = input(data, b"wOFF", 44, l)?;
    if u16_at(data, 14)? != 0 {
        return Err(invalid("nonzero WOFF1 reserved field"));
    }
    let directory_end = 44 + n * 20;
    slice(data, 0, directory_end)?;
    let mut records = Vector::with_capacity(budget, n)?;
    let mut previous = None;
    for i in 0..n {
        let o = 44 + i * 20;
        let tag = slice(data, o, 4)?.try_into().map_err(invalid)?;
        if !valid_tag(tag) || previous.is_some_and(|p| p >= tag) {
            return Err(invalid("invalid, duplicate or unsorted WOFF1 tag"));
        }
        previous = Some(tag);
        let offset = u32_at(data, o + 4)? as usize;
        let compressed = u32_at(data, o + 8)? as usize;
        let original = u32_at(data, o + 12)? as usize;
        if !offset.is_multiple_of(4) || offset < directory_end || compressed > original {
            return Err(invalid("invalid WOFF1 table span"));
        }
        slice(data, offset, compressed)?;
        records.push((tag, offset, compressed, original, u32_at(data, o + 16)?))?;
    }
    let total = table_sum(n, records.iter().map(|r| r.3), l)?;
    bounded(
        total,
        l.sfnt.resources.max_decompressed_bytes,
        "WOFF decompressed bytes",
    )?;
    if total != u32_at(data, 16)? as usize {
        return Err(invalid("incorrect totalSfntSize"));
    }
    let mut spans = Vector::with_capacity(budget, n)?;
    for r in records.iter() {
        spans.push((r.1, r.2))?;
    }
    spans.sort_unstable();
    let mut end = directory_end;
    for &(offset, len) in spans.iter() {
        if offset != align(end)? {
            return Err(invalid("overlapping tables or extraneous gap"));
        }
        if slice(data, end, offset - end)?.iter().any(|b| *b != 0) {
            return Err(invalid("nonzero padding"));
        }
        end = offset + len;
    }
    drop(spans);
    // WOFF1 pads every table, including the final table. WOFF2 instead permits
    // an unpadded final compressed block, so keep this check outside read_aux.
    let padded_end = align(end)?;
    if slice(data, end, padded_end - end)?.iter().any(|b| *b != 0) {
        return Err(invalid("nonzero final table padding"));
    }
    let (auxiliary, mut report) = read_aux(data, 20, padded_end, l, budget, |data, expected| {
        decompress(data, expected, budget)
    })?;
    let mut tables = Vector::with_capacity(budget, n)?;
    for &(tag, offset, compressed, original, sum) in records.iter() {
        let raw = slice(data, offset, compressed)?;
        let table = if compressed == original {
            copy(budget, raw)?
        } else {
            decompress(raw, original, budget)?
        };
        if checksum(tag, &table)? != sum {
            return Err(invalid("WOFF1 table checksum mismatch"));
        }
        tables.push((tag, table, offset))?;
    }
    drop(records);
    let mut borrowed = Vector::with_capacity(budget, n)?;
    for (tag, table, offset) in tables.iter() {
        borrowed.push((*tag, table.as_slice(), *offset))?;
    }
    let (mut sfnt, adjustment) = assemble_ordered(flavor, &borrowed, l, budget)?;
    let original_adjustment = u32_at(
        borrowed
            .iter()
            .find(|t| t.0 == *b"head")
            .ok_or_else(|| invalid("missing head"))?
            .1,
        8,
    )?;
    if adjustment != original_adjustment {
        report.push(ReportEntry {
            level: Level::Info,
            impact: Impact::Repair,
            code: "woff1.checksum_adjustment_repaired".into(),
            location: Default::default(),
            message: "Recomputed an incorrect incoming sfnt checksum adjustment".into(),
        });
    }
    if adjustment != original_adjustment && borrowed.iter().any(|t| t.0 == *b"DSIG") {
        without_dsig(&mut borrowed);
        // The superseded sfnt and then its charge are released before reassembly.
        drop(sfnt);
        sfnt = assemble_ordered(flavor, &borrowed, l, budget)?.0;
        loss(
            &mut report,
            "woff1.dsig_removed",
            "Checksum/layout repair invalidates the sfnt digital signature; DSIG removed",
        );
    }
    bounded(
        sfnt.len() + auxiliary.bytes(),
        l.sfnt.resources.max_total_bytes,
        "WOFF decoded bytes",
    )?;
    Ok(Decoded {
        sfnt: release(sfnt),
        auxiliary: auxiliary.release(),
        report,
    })
}
/// Encode any standalone supported sfnt. Table contents remain byte-identical
/// apart from canonical head.checkSumAdjustment on reconstruction.
pub fn encode(data: &[u8], options: &Options, l: &Limits) -> Result<Encoded> {
    encode_with_budget(data, options, l, &ledger(l))
}
pub(super) fn encode_with_budget(
    data: &[u8],
    options: &Options,
    l: &Limits,
    budget: &Budget,
) -> Result<Encoded> {
    if options.compression_level > 9 {
        return Err(invalid("zlib quality must be 0..=9"));
    }
    checked_aux(&options.auxiliary, l)?;
    let original = sfnt::Sfnt::parse_with_budget(data, &l.sfnt, budget)?;
    // WOFF1 §5 explicitly requires creators to validate and reject incorrect
    // table/whole-font checksums before any packaging normalization.
    for table in original.tables() {
        if checksum(table.tag, table.data)? != table.checksum {
            return Err(invalid("source sfnt table checksum mismatch"));
        }
    }
    if sfnt::checksum(data) != 0xb1b0_afba {
        return Err(invalid("source sfnt whole-font checksum mismatch"));
    }
    let mut report = Report::default();
    let mut tables = source_tables(&original, budget)?;
    let mut canonical = assemble_ordered(original.flavor(), &tables, l, budget)?.0;
    if canonical.as_slice() != data {
        if hidden_data(data, &original, budget)? {
            loss(
                &mut report,
                "woff1.non_table_data_removed",
                "Nonzero data outside sfnt tables cannot be represented by WOFF1",
            );
        } else {
            report.push(ReportEntry {
                level: Level::Info,
                impact: Impact::Repair,
                code: "woff1.sfnt_normalized".into(),
                location: Default::default(),
                message: "checksum-valid sfnt directory or padding normalized before packaging"
                    .into(),
            });
        }
        if tables.iter().any(|t| t.0 == *b"DSIG") {
            without_dsig(&mut tables);
            // Drop the superseded canonical sfnt before its charge is released.
            drop(canonical);
            canonical = assemble_ordered(original.flavor(), &tables, l, budget)?.0;
            loss(
                &mut report,
                "woff1.dsig_removed",
                "sfnt normalization invalidates the digital signature; DSIG removed",
            );
        }
    }
    drop(tables);
    let mut output_limits = l.sfnt.clone();
    output_limits.resources.max_input_bytes = output_limits.resources.max_total_bytes;
    let f = sfnt::Sfnt::parse_with_budget(&canonical, &output_limits, budget)?;
    let mut records = Vector::with_capacity(budget, f.tables().len())?;
    for r in f.tables() {
        records.push(r)?;
    }
    records.sort_unstable_by_key(|r| r.tag);
    let total = table_sum(records.len(), records.iter().map(|r| r.data.len()), l)?;
    let header = 44 + records.len() * 20;
    let mut out = Buffer::with_capacity(budget, header)?;
    out.resize(header, 0)?;
    out[..4].copy_from_slice(b"wOFF");
    out[4..8].copy_from_slice(&f.flavor().version().to_be_bytes());
    out[12..14].copy_from_slice(&(records.len() as u16).to_be_bytes());
    set32(&mut out, 16, total)?;
    let mut physical = Vector::with_capacity(budget, records.len())?;
    for (i, r) in records.iter().enumerate() {
        physical.push((i, *r))?;
    }
    physical.sort_unstable_by_key(|(_, r)| (r.offset, r.data.len(), r.tag));
    for &(i, r) in physical.iter() {
        let compressed = compress(r.data, options.compression_level, budget)?;
        let body = if compressed.len() < r.data.len() {
            compressed.as_slice()
        } else {
            r.data
        };
        pad(&mut out)?;
        let o = 44 + i * 20;
        out[o..o + 4].copy_from_slice(&r.tag);
        let offset = out.len();
        set32(&mut out, o + 4, offset)?;
        set32(&mut out, o + 8, body.len())?;
        set32(&mut out, o + 12, r.data.len())?;
        out[o + 16..o + 20].copy_from_slice(&checksum(r.tag, r.data)?.to_be_bytes());
        bounded(
            out.len().saturating_add(body.len()),
            l.sfnt.resources.max_total_bytes,
            "WOFF output bytes",
        )?;
        put(&mut out, body)?;
    }
    // WOFF1 requires each table, including the last, to be padded.
    pad(&mut out)?;
    write_aux(&mut out, 20, &options.auxiliary, l, |d| {
        compress(d, options.compression_level, budget)
    })?;
    Ok(Encoded {
        bytes: release(out.into_owned()),
        report,
    })
}
