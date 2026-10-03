//! WOFF2 Brotli coding, glyf/loca and hmtx transforms, and bounded parallel jobs.
use super::*;
use std::collections::BTreeMap;
#[cfg(feature = "ttc")]
mod collection;
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
fn base128(out: &mut Vec<u8>, n: u32) {
    let bytes = ((32 - n.leading_zeros()).max(1) as usize).div_ceil(7);
    for i in (0..bytes).rev() {
        out.push(((n >> (i * 7)) & 127) as u8 | if i == 0 { 0 } else { 128 });
    }
}
fn ushort255(out: &mut Vec<u8>, n: u16) {
    match n {
        0..=252 => out.push(n as u8),
        253..=505 => out.extend([255, (n - 253) as u8]),
        506..=761 => out.extend([254, (n - 506) as u8]),
        _ => {
            out.push(253);
            p16(out, n);
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
    let mut state = brotli::BrotliState::new(
        brotli::HeapAlloc::<u8>::default(),
        brotli::HeapAlloc::<u32>::default(),
        brotli::HeapAlloc::<brotli::HuffmanCode>::default(),
    );
    let (mut avail_in, mut input_offset, mut avail_out, mut output_offset, mut total_out) =
        (data.len(), 0, out.len(), 0, 0);
    let result = brotli::BrotliDecompressStream(
        &mut avail_in,
        &mut input_offset,
        data,
        &mut avail_out,
        &mut output_offset,
        &mut out,
        &mut total_out,
        &mut state,
    );
    if !matches!(result, brotli::BrotliResult::ResultSuccess)
        || input_offset != data.len()
        || output_offset != expected
    {
        return Err(invalid("Brotli stream or decoded length mismatch"));
    }
    out.truncate(expected);
    Ok(out)
}
fn compress(data: &[u8], o: &Options, l: &Limits) -> Result<Vec<u8>> {
    use brotli::enc::{BrotliEncoderParams, backward_references::BrotliEncoderMode};
    let params = BrotliEncoderParams {
        quality: o.quality as i32,
        mode: BrotliEncoderMode::BROTLI_MODE_FONT,
        lgwin: 22,
        size_hint: data.len(),
        ..Default::default()
    };
    // Small streams do not amortize worker creation/dictionary preparation.
    // The size-based choice is deterministic and independent of CPU count.
    if o.compression_threads == 1 || data.len() < 128 * 1024 {
        let mut out = Output {
            bytes: Vec::new(),
            limit: l.sfnt.resources.max_total_bytes,
        };
        brotli::BrotliCompress(&mut &data[..], &mut out, &params).map_err(invalid)?;
        Ok(out.bytes)
    } else {
        use brotli::enc::{Owned, SendAlloc, StandardAlloc, backward_references::UnionHasher};
        let size = brotli::enc::encode::BrotliEncoderMaxCompressedSizeMulti(
            data.len(),
            o.compression_threads,
        );
        bounded(
            size,
            l.sfnt.resources.max_total_bytes,
            "WOFF2 parallel compressed capacity",
        )?;
        let mut out = buffer(size)?;
        out.resize(size, 0);
        let mut allocators = (0..o.compression_threads)
            .map(|_| SendAlloc::new(StandardAlloc::default(), UnionHasher::Uninit))
            .collect::<Vec<_>>();
        struct Bytes(Vec<u8>);
        impl brotli::SliceWrapper<u8> for Bytes {
            fn slice(&self) -> &[u8] {
                &self.0
            }
        }
        let mut owned = Owned::new(Bytes(data.to_vec()));
        let size = brotli::enc::compress_multi(&params, &mut owned, &mut out, &mut allocators)
            .map_err(|e| invalid(format!("parallel Brotli: {e:?}")))?;
        out.truncate(size);
        Ok(out)
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
    #[cfg(feature = "ttc")]
    if data.get(4..8) == Some(b"ttcf") {
        return collection::decode(data, l);
    }

    let (flavor, n) = input(data, b"wOF2", 48, l)?;
    let mut cursor = Cursor { data, pos: 48 };
    let mut records = Vec::with_capacity(n);
    let mut tags = std::collections::BTreeSet::new();
    let mut size = 0usize;
    for _ in 0..n {
        let flags = cursor.byte()?;
        let tag = if flags & 63 == 63 {
            cursor.take(4)?.try_into().map_err(invalid)?
        } else {
            TAGS[(flags & 63) as usize]
        };
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
        });
    }
    let gi = records.iter().position(|r| r.tag == *b"glyf");
    let li = records.iter().position(|r| r.tag == *b"loca");
    match (gi, li) {
        (Some(g), Some(a)) if g < a && records[g].transformed == records[a].transformed => {}
        (None, None) => {}
        _ => return Err(invalid("missing, misordered or mismatched glyf/loca pair")),
    }
    let compressed = u32_at(data, 20)? as usize;
    let raw = decompress(cursor.take(compressed)?, size)?;
    let (auxiliary, mut report) = read_aux(data, 24, cursor.pos, l, decompress)?;
    let mut tables = BTreeMap::new();
    let mut offset = 0;
    for r in &records {
        tables.insert(r.tag, slice(&raw, offset, r.length)?.to_vec());
        offset += r.length;
    }
    if let Some(g) = gi.filter(|g| records[*g].transformed) {
        let maxp = tables
            .get(b"maxp")
            .ok_or_else(|| invalid("transform missing maxp"))?;
        let ng = u16_at(maxp, 4)?;
        let e = transform::decode(tables.get(b"glyf").ok_or_else(|| invalid("glyf"))?, ng, l)?;
        let loca_record = &records[li.ok_or_else(|| invalid("loca"))?];
        if e.loca.len() != loca_record.original {
            return Err(invalid("wrong original loca length"));
        }
        let head = tables
            .get_mut(b"head")
            .ok_or_else(|| invalid("transform missing head"))?;
        slice(head, 0, 54)?;
        head[50..52].copy_from_slice(&e.index_format.to_be_bytes());
        tables.insert(records[g].tag, e.glyf);
        tables.insert(*b"loca", e.loca);
    }
    if records.iter().any(|r| r.tag == *b"hmtx" && r.transformed) {
        let xmins = transform::xmins(&tables, l)?;
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
        )?;
        let expected = records
            .iter()
            .find(|r| r.tag == *b"hmtx")
            .ok_or_else(|| invalid("hmtx"))?
            .original;
        if hmtx.len() != expected {
            return Err(invalid("wrong original hmtx length"));
        }
        tables.insert(*b"hmtx", hmtx);
    }
    // WOFF2 signatures are invalid after table reconstruction, even null transforms.
    if tables.remove(b"DSIG").is_some() {
        loss(
            &mut report,
            "woff2.dsig_removed",
            "WOFF2 reconstruction invalidates the sfnt digital signature; DSIG removed",
        );
    }
    let mut builder = sfnt::SfntBuilder::new(flavor);
    table_sum(tables.len(), tables.values().map(Vec::len), l)?;
    for (tag, table) in tables {
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
/// Encode a standalone sfnt with actual glyf/loca preprocessing. Unknown and
/// non-transformed tables are retained, except invalidated DSIG (reported).
pub fn encode(data: &[u8], o: &Options, l: &Limits) -> Result<Encoded> {
    #[cfg(feature = "ttc")]
    if data.starts_with(b"ttcf") {
        return collection::encode(data, o, l);
    }

    check_options(o, l)?;
    let font = sfnt::Sfnt::parse(data, &l.sfnt)?;
    let mut tables = font
        .tables()
        .iter()
        .map(|r| (r.tag, r.data.to_vec()))
        .collect::<BTreeMap<_, _>>();
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
        transform::normalize(&mut tables, l)?;
    }
    let total = table_sum(tables.len(), tables.values().map(Vec::len), l)?;
    let mut transformed = BTreeMap::new();
    if have_glyf && o.transform_hmtx && tables.contains_key(b"hmtx") {
        let xmins = transform::xmins(&tables, l)?;
        let h = u16_at(
            tables.get(b"hhea").ok_or_else(|| invalid("missing hhea"))?,
            34,
        )? as usize;
        if let Some(t) = transform::encode_hmtx(
            tables.get(b"hmtx").ok_or_else(|| invalid("hmtx"))?,
            &xmins,
            h,
        )? {
            transformed.insert(*b"hmtx", t);
        }
    }
    if have_glyf && o.transform_glyf {
        transformed.insert(*b"glyf", transform::encode(&tables, l)?);
        transformed.insert(*b"loca", Vec::new());
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
    let mut order = tables
        .keys()
        .copied()
        .filter(|t| *t != *b"loca")
        .collect::<Vec<_>>();
    if have_glyf {
        let pos = order
            .iter()
            .position(|t| *t == *b"glyf")
            .ok_or_else(|| invalid("glyf"))?;
        order.insert(pos + 1, *b"loca");
    }
    let mut out = vec![0; 48];
    out[..4].copy_from_slice(b"wOF2");
    out[4..8].copy_from_slice(&font.flavor().version().to_be_bytes());
    out[12..14].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    set32(&mut out, 16, total)?;
    let mut raw = Vec::new();
    for tag in order {
        let table = &tables[&tag];
        let encoded = transformed.get(&tag);
        let version = if [*b"glyf", *b"loca"].contains(&tag) {
            if encoded.is_some() { 0 } else { 3 }
        } else if encoded.is_some() {
            1
        } else {
            0
        };
        let index = TAGS.iter().position(|t| *t == tag).unwrap_or(63);
        out.push(index as u8 | version << 6);
        if index == 63 {
            out.extend(tag);
        }
        base128(&mut out, as32(table.len())?);
        if let Some(encoded) = encoded {
            base128(&mut out, as32(encoded.len())?);
        }
        let body = encoded.unwrap_or(table);
        bounded(
            raw.len().saturating_add(body.len()),
            l.sfnt
                .resources
                .max_decompressed_bytes
                .min(l.sfnt.resources.max_total_bytes),
            "WOFF2 transformed bytes",
        )?;
        raw.extend(body);
    }
    let compressed = compress(&raw, o, l)?;
    set32(&mut out, 20, compressed.len())?;
    bounded(
        out.len().saturating_add(compressed.len()),
        l.sfnt.resources.max_total_bytes,
        "WOFF2 output bytes",
    )?;
    out.extend(compressed);
    // Pad the compressed block even without auxiliary blocks (Google decoder
    // advances its source cursor to a four-byte boundary).
    pad(&mut out);
    write_aux(&mut out, 24, &o.auxiliary, l, |d| compress(d, o, l))?;
    Ok(Encoded { bytes: out, report })
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
