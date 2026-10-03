//! WOFF 1.0 table-by-table zlib coding.
use super::*;
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};

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
    let mut e = ZlibEncoder::new(
        Output {
            bytes: Vec::new(),
            limit,
        },
        Compression::new(level),
    );
    e.write_all(data).map_err(invalid)?;
    Ok(e.finish().map_err(invalid)?.bytes)
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
        if offset % 4 != 0 || offset < directory_end || compressed > original {
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
    let (auxiliary, report) = read_aux(data, 20, end, l, decompress)?;
    let mut builder = sfnt::SfntBuilder::new(flavor);
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
        builder.insert(tag, table)?;
    }
    let sfnt = builder.build(&l.sfnt)?;
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
    let f = sfnt::Sfnt::parse(data, &l.sfnt)?;
    let mut records = f.tables().iter().collect::<Vec<_>>();
    records.sort_unstable_by_key(|r| r.tag);
    let total = table_sum(records.len(), records.iter().map(|r| r.data.len()), l)?;
    let mut out = vec![0; 44 + records.len() * 20];
    out[..4].copy_from_slice(b"wOFF");
    out[4..8].copy_from_slice(&f.flavor().version().to_be_bytes());
    out[12..14].copy_from_slice(&(records.len() as u16).to_be_bytes());
    set32(&mut out, 16, total)?;
    for (i, r) in records.iter().enumerate() {
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
    Ok(Encoded {
        bytes: out,
        report: Report::default(),
    })
}
