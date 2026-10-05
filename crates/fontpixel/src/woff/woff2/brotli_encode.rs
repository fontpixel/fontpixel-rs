//! Source-derived upper bound of the locked `brotli 9.0.0` encoder heap.
//!
//! The encoder allocates through `alloc_no_stdlib::Allocator`, whose
//! `alloc_cell` cannot fail, and indexes new cells immediately, so its storage
//! cannot be admitted cell by cell. fontpixel therefore reserves this bound in
//! the operation's working ledger before calling the encoder and keeps it until
//! every worker has been joined. The dependency is not patched or vendored.
//!
//! # Call shape covered
//!
//! `BROTLI_MODE_FONT`, `size_hint = input_len`, every other
//! `BrotliEncoderParams` field at its default (`log_meta_block = false`,
//! `favor_cpu_efficiency = false`, no custom dictionary, no large window).
//! A single `BrotliCompress` stream when `threads == 1` or
//! `input_len < 128 KiB`; otherwise `compress_multi` with `threads`
//! `StandardAlloc` workers, worker `i` encoding `get_range(i)` with the preceding
//! input as its custom dictionary. `StandardAlloc::alloc_cell(len)` allocates
//! exactly `len * size_of::<T>()` bytes (`alloc-stdlib-0.3.0/src/heap_alloc.rs`).
//!
//! # Method
//!
//! Every production allocation site reachable from that call shape is counted
//! at its largest possible size. State that lives across the whole call (ring
//! buffer, hasher, command array, output storage) is summed. Within a
//! metablock the backward-reference scratch is freed before the metablock is
//! built (`hq.rs:1444-1447`, `hq.rs:1042`), so the bound takes the larger of
//! that phase and the metablock phase; inside the metablock phase the three
//! `SplitByteVector` streams run one after another (`block_splitter.rs:870-930`)
//! and only their split arrays persist, so the larger stream transient is
//! taken. Everything else inside a phase is summed with no reuse credit. A
//! growable array counts its new and superseded cells together. Data-dependent counts use only proven ceilings: bytes per
//! backward-reference call `b <= min(2^lgblock, n)`, literals/commands per
//! metablock (`encode.rs` merge gate `num_* < max_length / 8` plus one more
//! block), block count `<= symbol count`, cluster count `<= block count`, and
//! H10 match records per position `<= 2 + 64 + 34`.
//!
//! Paths in `brotli-9.0.0/src/enc/` (line numbers of the locked source):
//!
//! | Term | Sites | Size used |
//! |---|---|---|
//! | ring buffer | `encode.rs:709-733`, `760-775` | first small cell `<= 2^lgblock + 9` plus regrown `2^rb + 2^lgblock + 9` (`rb = 1 + max(lgwin, lgblock)`) |
//! | hasher | `encode.rs:834-1060`, `1100-1112`; `hash_to_binary_tree.rs:73,156-188` | H2/H3/H4/H54/H5/H6/H9 table sizes chosen exactly as `ChooseHasher`; H10 `2^17 + 2 * 2^lgwin` u32 (never one-shot) |
//! | commands | `encode.rs:2399-2418` | old + new `Command` arrays of `min(max_length/8, n) + 3b/4 + 17` |
//! | output storage | `encode.rs:1637-1643`, `2255`, `2769-2785` | `2 * min(max_length, n) + 527` (q >= 2) or `2 * min(2^lgwin, n) + 503` (q <= 1); old cell freed first |
//! | fast paths (q <= 1) | `encode.rs:1674-1700`, `2735-2747`, `2339-2342`, `2930` | hash table `2^17` i32; two command/literal buffer pairs of `2^17` u32 + u8; catable workers (`i > 0`) skip `compress_stream_fast` and also use the ring buffer and metablock storage |
//! | Zopfli (q >= 10) | `hq.rs:205-216`, `1008`, `1384-1388` | `(b+1)` nodes, `(b+2) + (b+1128)` costs |
//! | HQ matches (q = 11) | `hq.rs:1259-1315` | `num_matches` b u32; `matches` grows from `4b` by doubling to cover `100(b-1)+128`, final + previous cell |
//! | block split (q >= 10) | `block_splitter.rs:700-810, 400-690, 860-930` | per stream (literal, command, distance): symbol copy, histograms, `block_ids`, insert costs, `switch_signal`, then `ClusterBlocks` with `nb = symbols`: symbols/lengths, all-histograms and cluster sizes (doubling, old+new), batch histograms, 2049 + `64*nb + 1` pairs, clusters, remap, persistent split types/lengths |
//! | metablock histograms (q >= 10) | `metablock.rs:226-300`; `cluster.rs:315-460` | `types*64` literal and `types*4` distance input histograms, `types` command histograms (`types <= min(256, symbols)`), context maps, clustered copies, cluster pairs and reindex scratch |
//! | greedy metablock (q 4-9) | `metablock.rs:390-560, 693, 845` | three splitters' type/length arrays (`symbols/min_block + 1`, doubling + old), `<= 269` literal, 257 command, 257 distance histograms, combine scratch, static context map |
//! | metablock store | `brotli_bit_stream.rs:963, 1802, 1872-1873, 2090` | Huffman trees, context-map RLE, depth/bit tables for `<= 269` literal and `<= 257` command/distance histograms |
//! | threads | `threading/mod.rs:339-356, 560-680`; `multithreading.rs` | per worker output `BrotliEncoderMaxCompressedSize(range)`, plus fixed thread/Arc bookkeeping |
//!
//! Unreachable for this call shape (and excluded): `LogMetaBlock`,
//! `CommandQueue`, stride/prior/CDF analysis (`log_meta_block` and the
//! detection qualities are 0), hasher `clone_with_alloc`
//! (`favor_cpu_efficiency` is false), `reader.rs`/`writer.rs`/`bin/`/tests.
//!
//! The constant [`BOOKKEEPING`] covers fixed-size cells (per-stream Huffman
//! trees, context-mode arrays) and std thread/Arc bookkeeping not modelled
//! term by term. It is a documented allowance, validated by the counting
//! allocator in `crates/fontpixel-heap-check`, not a derived quantity.
//!
//! # Consequence
//!
//! At quality >= 10 the block-split clustering terms are linear in the
//! metablock's symbol count with kilobyte constants, and the quality-11 match
//! array alone approaches 400 MiB for a 256 KiB block. A sound bound therefore
//! exceeds the 256 MiB default working ledger for nearly every real font at
//! the default quality 11. See `strict/ACCEPTANCE-A.md`.
use brotli::enc::{
    Mem256f, ZopfliNode,
    cluster::HistogramPair,
    command::Command,
    entropy_encode::HuffmanTree,
    floatX,
    histogram::{HistogramCommand, HistogramDistance, HistogramLiteral},
};
use std::mem::size_of;

/// Inputs below this length use a single Brotli stream (see `woff2::compress`).
pub(super) const PARALLEL_MIN_BYTES: usize = 128 * 1024;
/// Fixed allowance for unmodelled small cells and std thread bookkeeping, per
/// encoder instance.
pub(crate) const BOOKKEEPING: usize = 256 * 1024;
/// Distance alphabet upper bound (`BROTLI_NUM_DISTANCE_SYMBOLS`).
const DISTANCE_ALPHABET: usize = 1128;
/// `BROTLI_NUM_HISTOGRAM_DISTANCE_SYMBOLS` in the locked source.
const HISTOGRAM_DISTANCE_SYMBOLS: usize = 544;
/// `MAX_NUM_MATCHES_H10` slack and per-position match ceiling (2 short-range,
/// 64 binary-tree depth, 34 static-dictionary lengths 4..=37).
const MATCH_SLACK: usize = 128;
const MATCHES_PER_POSITION: usize = 2 + 64 + 34;

/// Checked arithmetic accumulator; `None` on overflow.
#[derive(Clone, Copy)]
struct Sum(Option<usize>);
impl Sum {
    fn add(self, bytes: Option<usize>) -> Self {
        Self(self.0.and_then(|t| t.checked_add(bytes?)))
    }
}
fn array<T>(count: usize) -> Option<usize> {
    count.checked_mul(size_of::<T>())
}
/// A doubling array that starts at `start` and must reach `need`: the final
/// cell plus the superseded one.
fn doubling<T>(start: usize, need: usize) -> Option<usize> {
    let mut size = start.max(1);
    while size < need {
        size = size.checked_mul(2)?;
    }
    let previous = if size > start.max(1) { size / 2 } else { 0 };
    array::<T>(size.checked_add(previous)?)
}

/// Transient heap of one `SplitByteVector` + `ClusterBlocks` run over `len`
/// symbols, including the symbol array copied by `BrotliSplitBlock` (all
/// summed, although the FindBlocks scratch is freed before clustering). The
/// persistent split arrays are [`split_arrays`].
fn split_stream<H, S>(
    len: usize,
    alphabet: usize,
    per_histogram: usize,
    max_histograms: usize,
) -> Option<usize> {
    let histograms = (len / per_histogram + 1).min(max_histograms);
    let bitmap = histograms.div_ceil(8);
    let nb = len.max(1);
    // ClusterBlocks: all_histograms/cluster_size start at 16*(nb+63)/64 and
    // double until they hold every batch cluster (at most nb).
    let expected = 16 * nb.checked_add(63)? / 64;
    Sum(Some(0))
        .add(array::<S>(len))
        .add(array::<H>(histograms))
        .add(array::<u8>(len))
        .add(array::<floatX>(alphabet.checked_mul(histograms)?))
        .add(array::<Mem256f>(bitmap))
        .add(array::<u8>(len.checked_mul(bitmap)?))
        .add(array::<u16>(histograms))
        .add(array::<u32>(nb.checked_mul(2)?))
        .add(doubling::<H>(expected, nb))
        .add(doubling::<u32>(expected, nb))
        .add(array::<H>(nb.min(64)))
        .add(array::<HistogramPair>(
            2049usize.checked_add(nb.checked_mul(64)?.checked_add(1)?)?,
        ))
        .add(array::<u32>(nb.checked_mul(2)?))
        .0
}

/// A block split's `types` (u8) and `lengths` (u32) arrays for up to `blocks`
/// blocks; they grow by doubling and persist until the metablock is stored.
fn split_arrays(blocks: usize) -> Option<usize> {
    Sum(doubling::<u8>(1, blocks))
        .add(doubling::<u32>(1, blocks))
        .0
}

/// `BrotliClusterHistograms` over `inputs` histograms of type `H`.
fn cluster_histograms<H>(inputs: usize) -> Option<usize> {
    let max_pairs = inputs.checked_mul(64)?.checked_add(1)?;
    Sum(Some(0))
        .add(array::<u32>(inputs.checked_mul(2)?))
        .add(array::<HistogramPair>(2049))
        .add(doubling::<HistogramPair>(2048, max_pairs))
        .add(array::<u32>(inputs))
        .add(array::<H>(inputs))
        .0
}

/// Hasher tables for the quality, exactly as `ChooseHasher`/`BrotliMakeHasher`.
fn hasher(quality: usize, lgwin: u32, size_hint: usize) -> Option<usize> {
    let h5 = |block_bits: u32, bucket_bits: u32| {
        Sum(array::<u32>(1usize << (block_bits + bucket_bits)))
            .add(array::<u16>(1 << bucket_bits))
            .0
    };
    match quality {
        0 | 1 => Some(0),
        2 => array::<u32>(65537 + 8),
        3 => array::<u32>(65538 + 8),
        4 if size_hint >= 1 << 20 => array::<u32>(1048580 + 8),
        4 => array::<u32>(131072 + 8),
        // H40-H42 are not implemented and fall back to H6 with the default
        // block/bucket bits (8, 15).
        5..=8 if lgwin <= 16 => h5(8, 15),
        5..=8 => {
            let block_bits = (quality as u32 - 1).min(9);
            let bucket_bits = if size_hint <= 1 << 22 && quality < 7 && size_hint <= 1 << 20 {
                14
            } else {
                15
            };
            h5(block_bits, bucket_bits)
        }
        9 => {
            Sum(array::<u16>(1 << 15))
                .add(array::<u32>(1 << (8 + 15)))
                .0
        }
        _ => array::<u32>((1usize << 17).checked_add(2usize.checked_mul(1 << lgwin)?)?),
    }
}

/// Heap bound of one encoder instance over `n` input bytes, optionally after a
/// custom dictionary as a catable worker (multi-threaded workers after the
/// first; `threading/mod.rs:357-370`).
fn instance(
    quality: usize,
    lgwin: u32,
    n: usize,
    size_hint: usize,
    catable: bool,
) -> Option<usize> {
    let lgblock = match quality {
        0 | 1 => lgwin,
        2 | 3 => 14,
        _ if quality >= 9 && lgwin > 16 => lgwin.min(18),
        _ => 16,
    };
    let block = 1usize << lgblock;
    let rb_bits = 1 + lgwin.max(lgblock);
    let max_length = 1usize << rb_bits.min(24);
    let b = block.min(n);
    let mut sum = Sum(Some(BOOKKEEPING))
        .add(hasher(quality, lgwin, size_hint))
        // Huffman trees for fast/trivial/store paths: 2 * 704 + 1 each, three
        // streams may be built in one call sequence.
        .add(array::<HuffmanTree>(3 * (2 * 704 + 1)));
    if quality <= 1 {
        // Hash table (`2^17` i32 at most) and two command/literal buffer
        // pairs (`2^17` u32 + u8 each).
        sum = sum
            .add(array::<i32>(1 << 17))
            .add(Some(2 * (1 << 17) * (size_of::<u32>() + 1)));
        if !catable {
            // compress_stream_fast (`encode.rs:2711-2860`): no ring buffer,
            // block storage of `2 * block + 503`.
            return sum
                .add((1usize << lgwin).min(n).checked_mul(2)?.checked_add(503))
                .0;
        }
        // Catable workers bypass compress_stream_fast (`encode.rs:2930`) and
        // go through the ring buffer and encode_data's q0/q1 branch
        // (`encode.rs:2337-2385`) with metablock storage.
        let ring = (1usize << rb_bits).checked_add(block)?.checked_add(9)?;
        return sum
            .add(Some(ring))
            .add(n.min(block).checked_add(9))
            .add(max_length.min(n).checked_mul(2)?.checked_add(527))
            .0;
    }
    // Ring buffer: small first cell plus the regrown full cell.
    let ring = (1usize << rb_bits).checked_add(block)?.checked_add(9)?;
    let first = if catable { block } else { n.min(block) };
    sum = sum.add(Some(ring)).add(first.checked_add(9));
    // Commands (old + new) and output storage.
    let commands = (max_length / 8).min(n).checked_add(3 * b / 4 + 17)?;
    sum = sum
        .add(array::<Command>(commands.checked_mul(2)?))
        .add(max_length.min(n).checked_mul(2)?.checked_add(527));
    let metablock = max_length.min(n);
    let literals = (max_length / 8).checked_add(b)?.min(metablock);
    let command_count = (max_length / 8)
        .checked_add(b)?
        .min(metablock)
        .checked_add(1)?;
    // store_meta_block: u8 depth + u16 bit tables of histograms x alphabet per
    // stream (clustered q >= 10: <= 256 each; greedy: <= 269 literal, <= 257
    // command/distance), plus context-map RLE scratch.
    let store = Sum(Some(3 * 269 * 256))
        .add(Some(3 * 257 * 704))
        .add(Some(3 * 257 * HISTOGRAM_DISTANCE_SYMBOLS))
        .add(array::<u32>(256 * 64 + 256 * 4))
        .0;
    if quality >= 10 {
        // Phase 1, backward references (freed before the metablock is built):
        // Zopfli nodes and cost model, and at q11 the HQ match arrays.
        let mut references = Sum(array::<ZopfliNode>(b.checked_add(1)?)).add(array::<floatX>(
            b.checked_add(2)?
                .checked_add(b.checked_add(DISTANCE_ALPHABET)?)?,
        ));
        if quality == 11 {
            let need = MATCHES_PER_POSITION
                .checked_mul(b.saturating_sub(1))?
                .checked_add(MATCH_SLACK)?;
            references = references
                .add(array::<u32>(b))
                .add(doubling::<u64>(4 * b, need));
        }
        // Phase 2, BrotliBuildMetaBlock: the literal, command and distance
        // splits run one after another and keep only their split arrays; then
        // the histogram/clustering stage and the store stage run while those
        // arrays (and the metablock histograms) are live.
        let splits = Sum(split_arrays(literals.max(1)))
            .add(split_arrays(command_count))
            .add(split_arrays(command_count))
            .0?;
        let split = split_stream::<HistogramLiteral, u8>(literals, 256, 544, 100)?
            .max(split_stream::<HistogramCommand, u16>(
                command_count,
                704,
                530,
                50,
            )?)
            .max(split_stream::<HistogramDistance, u16>(
                command_count,
                HISTOGRAM_DISTANCE_SYMBOLS,
                544,
                50,
            )?);
        // Block types per stream are final clusters: <= 256 and <= the
        // stream's block count (<= its symbol count). Then: context modes,
        // input literal (x64 contexts) and distance (x4) histograms, command
        // histograms, both context maps, clustered copies, both clusterings,
        // and the store scratch.
        let literal_types = literals.clamp(1, 256);
        let other_types = command_count.clamp(1, 256);
        let histograms = Sum(Some(256 * 8))
            .add(array::<HistogramLiteral>(literal_types * 64 * 2))
            .add(array::<HistogramDistance>(other_types * 4 * 2))
            .add(array::<HistogramCommand>(other_types))
            .add(array::<u32>(literal_types * 64 + other_types * 4))
            .add(cluster_histograms::<HistogramLiteral>(literal_types * 64))
            .add(cluster_histograms::<HistogramDistance>(other_types * 4))
            .add(store)
            .0?;
        let metablock = splits.checked_add(split.max(histograms))?;
        return sum.add(Some(references.0?.max(metablock))).0;
    } else if quality >= 4 {
        // BrotliBuildMetaBlockGreedy: three splitters and their histograms.
        let splits = |symbols: usize, min_block: usize| split_arrays(symbols / min_block + 1);
        sum = sum
            .add(splits(literals, 512))
            .add(splits(command_count, 1024))
            .add(splits(command_count, 512))
            .add(array::<HistogramLiteral>(269 + 26))
            .add(array::<HistogramCommand>(257))
            .add(array::<HistogramDistance>(257))
            .add(array::<u32>(256 * 64));
    }
    if quality >= 4 {
        sum = sum.add(store);
    }
    sum.0
}

/// Conservative upper bound, in bytes, of all heap the locked `brotli 9.0.0`
/// encoder can allocate while fontpixel compresses `input_len` bytes at
/// `quality` (0..=11) and window `lgwin` (clamped to 10..=24 as the encoder
/// does) with `threads` workers (1..=16). WOFF2 always uses `lgwin = 22`.
///
/// This covers the dependency's internal storage only. fontpixel's own
/// input copy, output buffer and allocator descriptors are admitted
/// separately. Returns `None` on arithmetic overflow. See the module
/// documentation for the per-site derivation.
pub fn brotli_encoder_heap_bound(
    quality: u32,
    lgwin: u32,
    input_len: usize,
    threads: usize,
) -> Option<usize> {
    let quality = quality.min(11) as usize;
    let lgwin = lgwin.clamp(10, 24);
    let threads = threads.max(1);
    if threads == 1 || input_len < PARALLEL_MIN_BYTES {
        return instance(quality, lgwin, input_len, input_len, false);
    }
    let mut sum = Sum(Some(0));
    for i in 0..threads {
        // threading/mod.rs get_range
        let start = i.checked_mul(input_len)? / threads;
        let end = (i + 1).checked_mul(input_len)? / threads;
        let n = end - start;
        sum = sum
            .add(instance(quality, lgwin, n, input_len, i != 0))
            .add(Some(brotli::enc::encode::BrotliEncoderMaxCompressedSize(n)));
    }
    sum.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_is_monotone_in_length_and_quality_relevant_terms_dominate() {
        for quality in 0..=11 {
            let mut previous = 0;
            for len in [0, 1, 4096, 65536, 1 << 20, 8 << 20] {
                let bound = brotli_encoder_heap_bound(quality, 22, len, 1).unwrap();
                assert!(bound >= previous, "q{quality} len {len}");
                previous = bound;
            }
        }
        // A full 256 KiB HQ block's match arrays alone exceed 300 MiB, so q11
        // can never be below that; the metablock phase is larger still.
        let q10 = brotli_encoder_heap_bound(10, 22, 1 << 18, 1).unwrap();
        let q11 = brotli_encoder_heap_bound(11, 22, 1 << 18, 1).unwrap();
        assert!(q11 >= q10 && q11 > 300 * 1024 * 1024);
        // Documented consequence: small inputs fit the 256 MiB default at
        // every quality, while q >= 10 exceeds it from a few tens of KiB.
        for quality in 0..=11 {
            assert!(brotli_encoder_heap_bound(quality, 22, 1024, 2).unwrap() < 256 << 20);
        }
        assert!(brotli_encoder_heap_bound(10, 22, 64 << 10, 1).unwrap() > 256 << 20);
        assert!(brotli_encoder_heap_bound(9, 22, 8 << 20, 2).unwrap() < 256 << 20);
        assert!(brotli_encoder_heap_bound(11, 22, usize::MAX, 2).is_none());
    }

    #[test]
    fn doubling_counts_the_final_and_superseded_cells() {
        assert_eq!(doubling::<u8>(4, 4), Some(4));
        assert_eq!(doubling::<u8>(4, 5), Some(8 + 4));
        assert_eq!(doubling::<u64>(4, 17), Some((32 + 16) * 8));
    }
}
