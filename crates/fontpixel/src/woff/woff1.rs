//! WOFF 1.0 table-by-table zlib coding.
use super::*;
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use std::io::Write;

/// Deterministic WOFF1 output options.
#[derive(Clone, Debug, PartialEq, Eq)]
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
fn decompress(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut out = buffer(
        expected
            .checked_add(1)
            .ok_or_else(|| invalid("length overflow"))?,
    )?;
    out.resize(expected + 1, 0);
    let mut d = Decompress::new(true);
    let status = d
        .decompress(data, &mut out, FlushDecompress::Finish)
        .map_err(invalid)?;
    if status != Status::StreamEnd
        || d.total_in() != data.len() as u64
        || d.total_out() != expected as u64
    {
        return Err(invalid("zlib stream or decoded length mismatch"));
    }
    out.truncate(expected);
    Ok(out)
}
fn compress(data: &[u8], level: u32, limit: usize) -> Result<Vec<u8>> {
    let budget = sfnt::working::Budget::new(limit, "WOFF compressed working bytes");
    let mut e = ZlibEncoder::new(
        sfnt::working::Buffer::new(&budget)?,
        Compression::new(level),
    );
    e.write_all(data).map_err(invalid)?;
    let (bytes, _lease) = e.finish().map_err(invalid)?.into_parts();
    Ok(bytes)
}
// WOFF1 §6 preserves physical table order independently of its tag-sorted
// directory. SfntBuilder intentionally sorts physical tables, so this wrapper
// needs an order-preserving assembler instead. Returns the new head adjustment.
fn assemble_ordered(
    flavor: sfnt::Flavor,
    tables: &[([u8; 4], &[u8], usize)],
    l: &Limits,
) -> Result<(Vec<u8>, u32)> {
    let n = tables.len();
    if n == 0 {
        return Err(invalid("empty sfnt"));
    }
    let total = table_sum(n, tables.iter().map(|t| t.1.len()), l)?;
    as32(total)?;
    let mut physical = (0..n).collect::<Vec<_>>();
    physical.sort_unstable_by_key(|&i| (tables[i].2, tables[i].1.len(), tables[i].0));
    let mut directory = (0..n).collect::<Vec<_>>();
    directory.sort_unstable_by_key(|&i| tables[i].0);
    let mut offsets = vec![0; n];
    let mut offset = 12 + n * 16;
    for &i in &physical {
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
    let mut out = buffer(total)?;
    p32(&mut out, flavor.version());
    p16(&mut out, n as u16);
    p16(&mut out, search_range as u16);
    p16(&mut out, selector);
    p16(&mut out, (n * 16 - search_range) as u16);
    for &i in &directory {
        let (tag, data, _) = tables[i];
        out.extend(tag);
        p32(&mut out, checksum(tag, data)?);
        p32(&mut out, as32(offsets[i])?);
        p32(&mut out, as32(data.len())?);
    }
    for &i in &physical {
        out.extend(tables[i].1);
        pad(&mut out);
    }
    out[head_offset + 8..head_offset + 12].fill(0);
    let adjustment = 0xb1b0_afbau32.wrapping_sub(sfnt::checksum(&out));
    out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok((out, adjustment))
}
fn source_tables<'a>(f: &sfnt::Sfnt<'a>) -> Vec<([u8; 4], &'a [u8], usize)> {
    f.tables()
        .iter()
        .map(|t| (t.tag, t.data, t.offset as usize))
        .collect()
}
fn hidden_data(data: &[u8], f: &sfnt::Sfnt<'_>) -> bool {
    let mut tables = f.tables().iter().collect::<Vec<_>>();
    tables.sort_unstable_by_key(|t| t.offset);
    let mut end = 12 + tables.len() * 16;
    for t in tables {
        let start = t.offset as usize;
        if start > end && data[end..start].iter().any(|b| *b != 0) {
            return true;
        }
        end = end.max(start + t.data.len());
    }
    data[end..].iter().any(|b| *b != 0)
}
/// Decode WOFF1 into standalone sfnt, preserving optional blocks.
pub fn decode(data: &[u8], l: &Limits) -> Result<Decoded> {
    let (flavor, n) = input(data, b"wOFF", 44, l)?;
    if u16_at(data, 14)? != 0 {
        return Err(invalid("nonzero WOFF1 reserved field"));
    }
    let directory_end = 44 + n * 20;
    slice(data, 0, directory_end)?;
    let mut records = Vec::with_capacity(n);
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
        records.push((tag, offset, compressed, original, u32_at(data, o + 16)?));
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
    let mut spans = records.iter().map(|r| (r.1, r.2)).collect::<Vec<_>>();
    spans.sort_unstable();
    let mut end = directory_end;
    for (offset, len) in spans {
        if offset != align(end)? {
            return Err(invalid("overlapping tables or extraneous gap"));
        }
        if slice(data, end, offset - end)?.iter().any(|b| *b != 0) {
            return Err(invalid("nonzero padding"));
        }
        end = offset + len;
    }
    // WOFF1 pads every table, including the final table. WOFF2 instead permits
    // an unpadded final compressed block, so keep this check outside read_aux.
    let padded_end = align(end)?;
    if slice(data, end, padded_end - end)?.iter().any(|b| *b != 0) {
        return Err(invalid("nonzero final table padding"));
    }
    let (auxiliary, mut report) = read_aux(data, 20, padded_end, l, decompress)?;
    let mut tables = Vec::with_capacity(n);
    for (tag, offset, compressed, original, sum) in records {
        let raw = slice(data, offset, compressed)?;
        let table = if compressed == original {
            raw.to_vec()
        } else {
            decompress(raw, original)?
        };
        if checksum(tag, &table)? != sum {
            return Err(invalid("WOFF1 table checksum mismatch"));
        }
        tables.push((tag, table, offset));
    }
    let mut borrowed = tables
        .iter()
        .map(|(tag, data, offset)| (*tag, data.as_slice(), *offset))
        .collect::<Vec<_>>();
    let (mut sfnt, adjustment) = assemble_ordered(flavor, &borrowed, l)?;
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
        borrowed.retain(|t| t.0 != *b"DSIG");
        sfnt = assemble_ordered(flavor, &borrowed, l)?.0;
        loss(
            &mut report,
            "woff1.dsig_removed",
            "Checksum/layout repair invalidates the sfnt digital signature; DSIG removed",
        );
    }
    bounded(
        sfnt.len()
            + auxiliary.metadata.as_ref().map_or(0, Vec::len)
            + auxiliary.private_data.as_ref().map_or(0, Vec::len),
        l.sfnt.resources.max_total_bytes,
        "WOFF decoded bytes",
    )?;
    Ok(Decoded {
        sfnt,
        auxiliary,
        report,
    })
}
/// Encode any standalone supported sfnt. Table contents remain byte-identical
/// apart from canonical head.checkSumAdjustment on reconstruction.
pub fn encode(data: &[u8], options: &Options, l: &Limits) -> Result<Encoded> {
    if options.compression_level > 9 {
        return Err(invalid("zlib quality must be 0..=9"));
    }
    checked_aux(&options.auxiliary, l)?;
    let original = sfnt::Sfnt::parse(data, &l.sfnt)?;
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
    let mut tables = source_tables(&original);
    let mut canonical = assemble_ordered(original.flavor(), &tables, l)?.0;
    if canonical != data {
        if hidden_data(data, &original) {
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
            tables.retain(|t| t.0 != *b"DSIG");
            canonical = assemble_ordered(original.flavor(), &tables, l)?.0;
            loss(
                &mut report,
                "woff1.dsig_removed",
                "sfnt normalization invalidates the digital signature; DSIG removed",
            );
        }
    }
    let mut output_limits = l.sfnt.clone();
    output_limits.resources.max_input_bytes = output_limits.resources.max_total_bytes;
    let f = sfnt::Sfnt::parse(&canonical, &output_limits)?;
    let mut records = f.tables().iter().collect::<Vec<_>>();
    records.sort_unstable_by_key(|r| r.tag);
    let total = table_sum(records.len(), records.iter().map(|r| r.data.len()), l)?;
    let mut out = vec![0; 44 + records.len() * 20];
    out[..4].copy_from_slice(b"wOFF");
    out[4..8].copy_from_slice(&f.flavor().version().to_be_bytes());
    out[12..14].copy_from_slice(&(records.len() as u16).to_be_bytes());
    set32(&mut out, 16, total)?;
    let mut physical = records.iter().enumerate().collect::<Vec<_>>();
    physical.sort_unstable_by_key(|(_, r)| (r.offset, r.data.len(), r.tag));
    for (i, r) in physical {
        let compressed = compress(
            r.data,
            options.compression_level,
            l.sfnt.resources.max_total_bytes,
        )?;
        let body = if compressed.len() < r.data.len() {
            compressed.as_slice()
        } else {
            r.data
        };
        pad(&mut out);
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
        out.extend(body);
    }
    // WOFF1 requires each table, including the last, to be padded.
    pad(&mut out);
    write_aux(&mut out, 20, &options.auxiliary, l, |d| {
        compress(
            d,
            options.compression_level,
            l.sfnt.resources.max_total_bytes,
        )
    })?;
    Ok(Encoded { bytes: out, report })
}
