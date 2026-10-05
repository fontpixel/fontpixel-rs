//! WOFF2 Brotli coding, glyf/loca and hmtx transforms, and bounded parallel jobs.
//!
//! Each public font operation shares one working ledger (`max_total_bytes`)
//! across its directory, raw stream, table map, transforms, compressor buffers
//! and output.
use super::*;
use crate::working::Vector;
mod brotli_decode;
mod brotli_encode;
pub use brotli_encode::{brotli_encoder_heap_bound, brotli_encoder_proven_heap_bound};
#[cfg(feature = "ttc")]
pub(in crate::woff) mod collection;
mod transform;
#[rustfmt::skip]
const TAGS:[[u8;4];63]=[
 *b"cmap",*b"head",*b"hhea",*b"hmtx",*b"maxp",*b"name",*b"OS/2",*b"post",*b"cvt ",*b"fpgm",*b"glyf",*b"loca",*b"prep",*b"CFF ",*b"VORG",*b"EBDT",
 *b"EBLC",*b"gasp",*b"hdmx",*b"kern",*b"LTSH",*b"PCLT",*b"VDMX",*b"vhea",*b"vmtx",*b"BASE",*b"GDEF",*b"GPOS",*b"GSUB",*b"EBSC",*b"JSTF",*b"MATH",
 *b"CBDT",*b"CBLC",*b"COLR",*b"CPAL",*b"SVG ",*b"sbix",*b"acnt",*b"avar",*b"bdat",*b"bloc",*b"bsln",*b"cvar",*b"fdsc",*b"feat",*b"fmtx",*b"fvar",
 *b"gvar",*b"hsty",*b"just",*b"lcar",*b"mort",*b"morx",*b"opbd",*b"prop",*b"trak",*b"Zapf",*b"Silf",*b"Glat",*b"Gloc",*b"Feat",*b"Sill"];
/// Deterministic WOFF2 output options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Brotli quality in 0..=11; the required default is 11.
    pub quality: u32,
    /// Apply glyf/loca transforms whenever those tables exist (default true).
    pub transform_glyf: bool,
    /// Apply hmtx transform when a full LSB array is reconstructible (default true).
    pub transform_hmtx: bool,
    /// Intra-font Brotli workers, 1..=16. Default two; more changes compression
    /// decisions/size but still emits one standard Brotli stream.
    pub compression_threads: usize,
    /// Envelope version and optional blocks.
    pub auxiliary: Auxiliary,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            quality: 11,
            transform_glyf: true,
            transform_hmtx: true,
            compression_threads: 2,
            auxiliary: Auxiliary::default(),
        }
    }
}
#[derive(Clone, Copy)]
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let v = slice(self.data, self.pos, n)?;
        self.pos += n;
        Ok(v)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(invalid)?,
        ))
    }
    fn base128(&mut self) -> Result<u32> {
        let mut n = 0u32;
        for i in 0..5 {
            let b = self.byte()?;
            if (i == 0 && b == 0x80) || n & 0xfe00_0000 != 0 {
                return Err(invalid("invalid UIntBase128"));
            }
            n = (n << 7) | u32::from(b & 127);
            if b & 128 == 0 {
                return Ok(n);
            }
        }
        Err(invalid("overlong UIntBase128"))
    }
    fn ushort255(&mut self) -> Result<u16> {
        match self.byte()? {
            253 => self.word(),
            254 => Ok(506 + u16::from(self.byte()?)),
            255 => Ok(253 + u16::from(self.byte()?)),
            v => Ok(u16::from(v)),
        }
    }
    fn end(&self) -> Result<()> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(invalid("unused transform stream bytes"))
        }
    }
}
fn base128(out: &mut Buffer, n: u32) -> Result<()> {
    let bytes = ((32 - n.leading_zeros()).max(1) as usize).div_ceil(7);
    for i in (0..bytes).rev() {
        out.push(((n >> (i * 7)) & 127) as u8 | if i == 0 { 0 } else { 128 })?;
    }
    Ok(())
}
fn ushort255(out: &mut Buffer, n: u16) -> Result<()> {
    match n {
        0..=252 => out.push(n as u8),
        253..=505 => put(out, &[255, (n - 253) as u8]),
        506..=761 => put(out, &[254, (n - 506) as u8]),
        _ => {
            out.push(253)?;
            p16(out, n)
        }
    }
}
fn decompress(data: &[u8], expected: usize, budget: &Budget) -> Result<Owned<Vec<u8>>> {
    brotli_decode::decompress(data, expected, budget)
}
/// Brotli window used for every WOFF2 stream.
const LGWIN: u32 = 22;
fn compress(data: &[u8], o: &Options, l: &Limits, budget: &Budget) -> Result<Owned<Vec<u8>>> {
    use brotli::enc::{BrotliEncoderParams, backward_references::BrotliEncoderMode};
    let params = BrotliEncoderParams {
        quality: o.quality as i32,
        mode: BrotliEncoderMode::BROTLI_MODE_FONT,
        lgwin: LGWIN as i32,
        size_hint: data.len(),
        ..Default::default()
    };
    // The encoder's own heap cannot be refused cell by cell; reserve its
    // admitted bound (proven for q0-9, empirical for q10-11) first and hold it
    // until every worker has joined.
    let encoder = budget.reserve(
        brotli_encoder_heap_bound(o.quality, LGWIN, data.len(), o.compression_threads)
            .ok_or_else(|| budget.error())?,
    )?;
    let result = compress_admitted(data, o, l, budget, &params);
    drop(encoder);
    result
}
fn compress_admitted(
    data: &[u8],
    o: &Options,
    l: &Limits,
    budget: &Budget,
    params: &brotli::enc::BrotliEncoderParams,
) -> Result<Owned<Vec<u8>>> {
    // Small streams do not amortize worker creation/dictionary preparation.
    // The size-based choice is deterministic and independent of CPU count.
    if o.compression_threads == 1 || data.len() < brotli_encode::PARALLEL_MIN_BYTES {
        let mut out = Buffer::new(budget)?;
        // A ledger failure inside the writer comes back as the original error.
        brotli::BrotliCompress(&mut &data[..], &mut out, params).map_err(io_error)?;
        Ok(out.into_owned())
    } else {
        use brotli::enc::{SendAlloc, StandardAlloc, backward_references::UnionHasher};
        let size = brotli::enc::encode::BrotliEncoderMaxCompressedSizeMulti(
            data.len(),
            o.compression_threads,
        );
        bounded(
            size,
            l.sfnt.resources.max_total_bytes,
            "WOFF2 parallel compressed capacity",
        )?;
        // Output capacity, the allocator descriptors and the owned input clone
        // are admitted before any of them is allocated.
        let mut out = Buffer::with_capacity(budget, size)?;
        out.resize(size, 0)?;
        let mut allocators = Vector::with_capacity(budget, o.compression_threads)?;
        for _ in 0..o.compression_threads {
            allocators.push(SendAlloc::new(
                StandardAlloc::default(),
                UnionHasher::Uninit,
            ))?;
        }
        struct Bytes(Vec<u8>);
        impl brotli::SliceWrapper<u8> for Bytes {
            fn slice(&self) -> &[u8] {
                &self.0
            }
        }
        let (input, input_lease) = Buffer::from_slice(budget, data)?.into_parts();
        let mut owned = brotli::enc::Owned::new(Bytes(input));
        // compress_multi joins every worker before it returns.
        let size = brotli::enc::compress_multi(params, &mut owned, &mut out, &mut allocators)
            .map_err(|e| invalid(format!("parallel Brotli: {e:?}")))?;
        drop(owned);
        drop(input_lease);
        drop(allocators);
        out.truncate(size);
        Ok(out.into_owned())
    }
}
fn check_options(o: &Options, l: &Limits) -> Result<()> {
    if o.quality > 11 || !(1..=16).contains(&o.compression_threads) {
        return Err(invalid("Brotli quality must be 0..=11 and threads 1..=16"));
    }
    checked_aux(&o.auxiliary, l)
}
#[derive(Clone, Debug)]
struct Record {
    tag: [u8; 4],
    original: usize,
    length: usize,
    transformed: bool,
}
/// Decode WOFF2 into standalone sfnt. Advisory original glyf/font lengths are
/// not used as allocation sizes or strict equality checks.
pub fn decode(data: &[u8], l: &Limits) -> Result<Decoded> {
    let budget = ledger(l);
    #[cfg(feature = "ttc")]
    if data.get(4..8) == Some(b"ttcf") {
        return collection::decode(data, l, &budget);
    }
    decode_with_budget(data, l, &budget)
}
pub(super) fn decode_with_budget(data: &[u8], l: &Limits, budget: &Budget) -> Result<Decoded> {
    let (flavor, n) = input(data, b"wOF2", 48, l)?;
    let mut cursor = Cursor { data, pos: 48 };
    let mut records = Vector::with_capacity(budget, n)?;
    let mut tags_lease = budget.reserve(0)?;
    let mut tags = std::collections::BTreeSet::new();
    let mut size = 0usize;
    for _ in 0..n {
        let flags = cursor.byte()?;
        let tag = if flags & 63 == 63 {
            cursor.take(4)?.try_into().map_err(invalid)?
        } else {
            TAGS[(flags & 63) as usize]
        };
        if valid_tag(tag) && !tags.contains(&tag) {
            tags_lease.grow_to(budget.tree_bytes::<[u8; 4], ()>(tags.len() + 1)?)?;
        }
        if !valid_tag(tag) || !tags.insert(tag) {
            return Err(invalid("invalid or duplicate WOFF2 tag"));
        }
        let v = flags >> 6;
        let transformed = match (&tag, v) {
            (b"glyf" | b"loca", 0) => true,
            (b"glyf" | b"loca", 3) => false,
            (b"hmtx", 1) => true,
            (_, 0) => false,
            _ => return Err(invalid("unknown transform version")),
        };
        let original = cursor.base128()? as usize;
        let length = if transformed {
            cursor.base128()? as usize
        } else {
            original
        };
        if tag == *b"loca" && transformed && length != 0 {
            return Err(invalid("transformed loca length must be zero"));
        }
        bounded(
            length,
            l.sfnt.resources.max_table_bytes,
            "WOFF2 stream table bytes",
        )?;
        size = size
            .checked_add(length)
            .ok_or_else(|| invalid("stream length overflow"))?;
        bounded(
            size,
            l.sfnt
                .resources
                .max_decompressed_bytes
                .min(l.sfnt.resources.max_total_bytes),
            "WOFF2 decompressed bytes",
        )?;
        records.push(Record {
            tag,
            original,
            length,
            transformed,
        })?;
    }
    drop(tags);
    drop(tags_lease);
    let gi = records.iter().position(|r| r.tag == *b"glyf");
    let li = records.iter().position(|r| r.tag == *b"loca");
    match (gi, li) {
        (Some(g), Some(a)) if g < a && records[g].transformed == records[a].transformed => {}
        (None, None) => {}
        _ => return Err(invalid("missing, misordered or mismatched glyf/loca pair")),
    }
    let compressed = u32_at(data, 20)? as usize;
    let raw = decompress(cursor.take(compressed)?, size, budget)?;
    let (auxiliary, mut report) = read_aux(data, 24, cursor.pos, l, budget, |data, expected| {
        decompress(data, expected, budget)
    })?;
    // The raw stream stays live and charged while every table copies from it.
    let mut tables = Tables::new(budget)?;
    let mut offset = 0;
    for r in records.iter() {
        tables.insert(budget, r.tag, copy(budget, slice(&raw, offset, r.length)?)?)?;
        offset += r.length;
    }
    drop(raw);
    if let Some(g) = gi.filter(|g| records[*g].transformed) {
        let maxp = tables
            .get(b"maxp")
            .ok_or_else(|| invalid("transform missing maxp"))?;
        let ng = u16_at(maxp, 4)?;
        let e = transform::decode(
            tables.get(b"glyf").ok_or_else(|| invalid("glyf"))?,
            ng,
            l,
            budget,
        )?;
        let loca_record = &records[li.ok_or_else(|| invalid("loca"))?];
        if e.loca.len() != loca_record.original {
            return Err(invalid("wrong original loca length"));
        }
        let head = tables
            .get_mut(b"head")
            .ok_or_else(|| invalid("transform missing head"))?;
        slice(head, 0, 54)?;
        head[50..52].copy_from_slice(&e.index_format.to_be_bytes());
        // Each replacement was admitted while the transformed source was live.
        tables.insert(budget, records[g].tag, e.glyf)?;
        tables.insert(budget, *b"loca", e.loca)?;
    }
    if records.iter().any(|r| r.tag == *b"hmtx" && r.transformed) {
        let xmins = transform::xmins(&tables, l, budget)?;
        let h = u16_at(
            tables
                .get(b"hhea")
                .ok_or_else(|| invalid("hmtx missing hhea"))?,
            34,
        )? as usize;
        let hmtx = transform::decode_hmtx(
            tables.get(b"hmtx").ok_or_else(|| invalid("hmtx"))?,
            &xmins,
            h,
            budget,
        )?;
        drop(xmins);
        let expected = records
            .iter()
            .find(|r| r.tag == *b"hmtx")
            .ok_or_else(|| invalid("hmtx"))?
            .original;
        if hmtx.len() != expected {
            return Err(invalid("wrong original hmtx length"));
        }
        tables.insert(budget, *b"hmtx", hmtx)?;
    }
    drop(records);
    // WOFF2 signatures are invalid after table reconstruction, even null transforms.
    if tables.remove(b"DSIG").is_some() {
        loss(
            &mut report,
            "woff2.dsig_removed",
            "WOFF2 reconstruction invalidates the sfnt digital signature; DSIG removed",
        );
    }
    table_sum(tables.len(), tables.map.values().map(|t| t.len()), l)?;
    let (builder, table_lease) = tables.into_builder(budget, flavor)?;
    let sfnt = builder.build_with_budget(&l.sfnt, budget)?;
    // Table bytes are released only after the builder that owns them drops.
    drop(builder);
    drop(table_lease);
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
/// Encode a standalone sfnt with actual glyf/loca preprocessing. Unknown and
/// non-transformed tables are retained, except invalidated DSIG (reported).
pub fn encode(data: &[u8], o: &Options, l: &Limits) -> Result<Encoded> {
    let budget = ledger(l);
    #[cfg(feature = "ttc")]
    if data.starts_with(b"ttcf") {
        return collection::encode(data, o, l, &budget);
    }
    encode_with_budget(data, o, l, &budget)
}
/// Version bits of a directory entry for this tag and transform state.
fn transform_version(tag: [u8; 4], transformed: bool) -> u8 {
    if [*b"glyf", *b"loca"].contains(&tag) {
        if transformed { 0 } else { 3 }
    } else if transformed {
        1
    } else {
        0
    }
}
/// Append one WOFF2 table directory entry.
fn directory_entry(
    out: &mut Buffer,
    tag: [u8; 4],
    original: usize,
    transformed: Option<usize>,
) -> Result<()> {
    let index = TAGS.iter().position(|t| *t == tag).unwrap_or(63);
    out.push(index as u8 | transform_version(tag, transformed.is_some()) << 6)?;
    if index == 63 {
        put(out, &tag)?;
    }
    base128(out, as32(original)?)?;
    if let Some(length) = transformed {
        base128(out, as32(length)?)?;
    }
    Ok(())
}
pub(super) fn encode_with_budget(
    data: &[u8],
    o: &Options,
    l: &Limits,
    budget: &Budget,
) -> Result<Encoded> {
    check_options(o, l)?;
    let font = sfnt::Sfnt::parse_with_budget(data, &l.sfnt, budget)?;
    let mut tables = Tables::new(budget)?;
    for r in font.tables() {
        tables.insert(budget, r.tag, copy(budget, r.data)?)?;
    }
    let flavor = font.flavor();
    drop(font);
    let mut report = Report::default();
    if tables.remove(b"DSIG").is_some() {
        loss(
            &mut report,
            "woff2.dsig_removed",
            "WOFF2 invalidates the sfnt digital signature; DSIG removed",
        );
    }
    let have_glyf = tables.contains_key(b"glyf");
    if have_glyf != tables.contains_key(b"loca") {
        return Err(invalid("missing glyf/loca pair"));
    }
    if have_glyf && o.transform_glyf {
        transform::normalize(&mut tables, l, budget)?;
    }
    let total = table_sum(tables.len(), tables.map.values().map(|t| t.len()), l)?;
    let mut transformed = Tables::new(budget)?;
    if have_glyf && o.transform_hmtx && tables.contains_key(b"hmtx") {
        let xmins = transform::xmins(&tables, l, budget)?;
        let h = u16_at(
            tables.get(b"hhea").ok_or_else(|| invalid("missing hhea"))?,
            34,
        )? as usize;
        if let Some(t) = transform::encode_hmtx(
            tables.get(b"hmtx").ok_or_else(|| invalid("hmtx"))?,
            &xmins,
            h,
            budget,
        )? {
            transformed.insert(budget, *b"hmtx", t)?;
        }
    }
    if have_glyf && o.transform_glyf {
        let glyf = transform::encode(&tables, l, budget)?;
        transformed.insert(budget, *b"glyf", glyf)?;
        transformed.insert(budget, *b"loca", Buffer::new(budget)?.into_owned())?;
    }
    // WOFF2 §5 requires this flag for every encoded font, including CFF and
    // null-transform fonts, because envelope reconstruction can change bytes.
    {
        let head = tables
            .get_mut(b"head")
            .ok_or_else(|| invalid("missing head"))?;
        let flags = u16_at(head, 16)? | 0x0800;
        head[16..18].copy_from_slice(&flags.to_be_bytes());
    }
    // The writer emits adjacent transformed glyf/loca; decoder accepts the less
    // restrictive standalone order allowed by W3C §5.5.
    let mut order = Vector::with_capacity(budget, tables.len())?;
    for &tag in tables.map.keys() {
        if tag == *b"loca" {
            continue;
        }
        order.push(tag)?;
        if have_glyf && tag == *b"glyf" {
            order.push(*b"loca")?;
        }
    }
    let mut out = Buffer::with_capacity(budget, 48)?;
    out.resize(48, 0)?;
    out[..4].copy_from_slice(b"wOF2");
    out[4..8].copy_from_slice(&flavor.version().to_be_bytes());
    out[12..14].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    set32(&mut out, 16, total)?;
    // Check the stream budget in directory order before admitting its storage.
    let mut raw_size = 0usize;
    for tag in order.iter() {
        let body = transformed
            .get(tag)
            .or_else(|| tables.get(tag))
            .ok_or_else(|| invalid("missing ordered table"))?;
        bounded(
            raw_size.saturating_add(body.len()),
            l.sfnt
                .resources
                .max_decompressed_bytes
                .min(l.sfnt.resources.max_total_bytes),
            "WOFF2 transformed bytes",
        )?;
        raw_size += body.len();
    }
    let mut raw = Buffer::with_capacity(budget, raw_size)?;
    for &tag in order.iter() {
        let table = tables.get(&tag).ok_or_else(|| invalid("missing table"))?;
        let encoded = transformed.get(&tag);
        directory_entry(&mut out, tag, table.len(), encoded.map(<[u8]>::len))?;
        put(&mut raw, encoded.unwrap_or(table))?;
    }
    drop(order);
    drop(transformed);
    drop(tables);
    let compressed = compress(&raw, o, l, budget)?;
    drop(raw);
    set32(&mut out, 20, compressed.len())?;
    bounded(
        out.len().saturating_add(compressed.len()),
        l.sfnt.resources.max_total_bytes,
        "WOFF2 output bytes",
    )?;
    put(&mut out, &compressed)?;
    drop(compressed);
    // Pad the compressed block even without auxiliary blocks (Google decoder
    // advances its source cursor to a four-byte boundary).
    pad(&mut out)?;
    write_aux(&mut out, 24, &o.auxiliary, l, |d| compress(d, o, l, budget))?;
    Ok(Encoded {
        bytes: release(out.into_owned()),
        report,
    })
}
/// Encode several independent fonts with bounded worker count, retaining input
/// order. Jobs must be 1..=64; the product with compression_threads must be <=64.
/// Per-font limits and aggregate batch input/output/count budgets all apply.
/// Callers choose worker count for concurrent compressor working memory.
/// An error returns no partially successful output to the caller.
pub fn encode_many(inputs: &[&[u8]], o: &Options, l: &Limits, jobs: usize) -> Result<Vec<Encoded>> {
    check_options(o, l)?;
    if !(1..=64).contains(&jobs) || jobs.saturating_mul(o.compression_threads) > 64 {
        return Err(invalid("parallel worker product must be 1..=64"));
    }
    bounded(inputs.len(), l.max_batch_fonts, "WOFF2 batch fonts")?;
    let input_bytes = inputs.iter().try_fold(0usize, |n, b| {
        n.checked_add(b.len())
            .ok_or_else(|| invalid("batch byte count overflow"))
    })?;
    bounded(input_bytes, l.max_batch_bytes, "WOFF2 batch input bytes")?;
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let output_bytes = std::sync::atomic::AtomicUsize::new(0);
    let stop = std::sync::atomic::AtomicBool::new(false);
    let slots = std::sync::Mutex::new((0..inputs.len()).map(|_| None).collect::<Vec<_>>());
    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::new();
        for _ in 0..jobs.min(inputs.len()) {
            let next = &next;
            let slots = &slots;
            let output_bytes = &output_bytes;
            let stop = &stop;
            handles.push(
                std::thread::Builder::new()
                    .spawn_scoped(scope, move || -> Result<()> {
                        loop {
                            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                                break;
                            }
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if i >= inputs.len() {
                                break;
                            }
                            let value = encode(inputs[i], o, l).and_then(|encoded| {
                                output_bytes
                                    .try_update(
                                        std::sync::atomic::Ordering::Relaxed,
                                        std::sync::atomic::Ordering::Relaxed,
                                        |n| {
                                            n.checked_add(encoded.bytes.len())
                                                .filter(|n| *n <= l.max_batch_bytes)
                                        },
                                    )
                                    .map_err(|_| Error::ResourceLimit {
                                        resource: "WOFF2 batch output bytes",
                                        limit: l.max_batch_bytes,
                                    })?;
                                Ok(encoded)
                            });
                            if value.is_err() {
                                stop.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                            slots
                                .lock()
                                .map_err(|_| invalid("worker result lock poisoned"))?[i] =
                                Some(value);
                        }
                        Ok(())
                    })
                    .map_err(invalid)?,
            );
        }
        for h in handles {
            h.join()
                .map_err(|_| invalid("compression worker panicked"))??;
        }
        Ok(())
    })?;
    slots
        .into_inner()
        .map_err(|_| invalid("worker result lock poisoned"))?
        .into_iter()
        .map(|r| r.ok_or_else(|| invalid("missing compression result"))?)
        .collect()
}
#[cfg(test)]
mod tests;
