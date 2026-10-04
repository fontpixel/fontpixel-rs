//! Fixed-workspace zlib backend, audited against exactly miniz_oxide 0.9.1.
use super::invalid;
use crate::{
    Result,
    sfnt::working::{Budget, Buffer, Owned},
};
use miniz_oxide::{
    DataFormat, MZFlush, MZStatus,
    deflate::{core::CompressorOxide, stream::deflate},
    inflate::stream::{InflateState, inflate},
};
use std::mem::size_of;

// The pinned source has no variable-size heap allocations in these states.
// deflate/core.rs: CompressorOxide owns HuffmanOxide, ParamsOxide::local_buf,
// and DictOxide::HashBuffers; deflate/buffer.rs fixes all array lengths.
// On wasm32 only, LZOxide::codes is separately boxed rather than inline.
const DEFLATE_WORKSPACE: usize = size_of::<CompressorOxide>()
    + 3 * 288 * (size_of::<u16>() * 2 + size_of::<u8>())
    + (65_536 * 13) / 10
    + 32_768
    + 258
    + 2 * 32_768 * size_of::<u16>()
    + if cfg!(target_arch = "wasm32") {
        65_536
    } else {
        0
    };
const STREAM_BYTES: usize = 32 * 1024;

pub(super) fn compress(data: &[u8], level: u32, budget: &Budget) -> Result<Owned<Vec<u8>>> {
    if level > 9 {
        return Err(invalid("zlib quality must be 0..=9"));
    }
    let lease = budget.reserve(DEFLATE_WORKSPACE)?;
    let mut state = Owned::from_parts(Box::<CompressorOxide>::default(), lease);
    state.set_format_and_level(DataFormat::Zlib, level as u8);
    let mut scratch = Buffer::with_capacity(budget, STREAM_BYTES)?;
    scratch.resize(STREAM_BYTES, 0)?;
    let mut output = Buffer::new(budget)?;
    let mut remaining = data;
    // Match flate2::write::ZlibEncoder's 32 KiB writer: consume all input with
    // None, draining each output chunk, then Finish with an empty input slice.
    while !remaining.is_empty() {
        let result = deflate(&mut state, remaining, &mut scratch, MZFlush::None);
        if result.status.is_err() || (result.bytes_consumed == 0 && result.bytes_written == 0) {
            return Err(invalid("zlib compression made no progress"));
        }
        output.extend(&scratch[..result.bytes_written])?;
        remaining = &remaining[result.bytes_consumed..];
    }
    loop {
        let result = deflate(&mut state, &[], &mut scratch, MZFlush::Finish);
        output.extend(&scratch[..result.bytes_written])?;
        match result.status {
            Ok(MZStatus::StreamEnd) => break,
            Ok(MZStatus::Ok) if result.bytes_written != 0 => {}
            _ => return Err(invalid("zlib compression did not finish")),
        }
    }
    Ok(output.into_owned())
}

pub(super) fn decompress(data: &[u8], expected: usize, budget: &Budget) -> Result<Owned<Vec<u8>>> {
    let lease = budget.reserve(size_of::<InflateState>())?;
    let mut state = Owned::from_parts(InflateState::new_boxed(DataFormat::Zlib), lease);
    // One spare decoded byte detects streams larger than the declared length.
    let capacity = expected
        .checked_add(1)
        .ok_or_else(|| invalid("length overflow"))?;
    let mut output = Buffer::with_capacity(budget, capacity)?;
    output.resize(capacity, 0)?;
    let result = inflate(&mut state, data, &mut output, MZFlush::Finish);
    if result.status != Ok(MZStatus::StreamEnd)
        || result.bytes_consumed != data.len()
        || result.bytes_written != expected
    {
        return Err(invalid("zlib stream or decoded length mismatch"));
    }
    output.truncate(expected);
    Ok(output.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    #[test]
    fn pinned_backend_preserves_flate2_bytes_at_every_level() {
        let mut random = 0x7a69_513du32;
        for size in [0, 1, 32_767, 32_768, 32_769, 131_073] {
            let inputs = [
                vec![0; size],
                (0..size).map(|i| (i % 251) as u8).collect(),
                (0..size)
                    .map(|_| {
                        random ^= random << 13;
                        random ^= random >> 17;
                        random ^= random << 5;
                        random as u8
                    })
                    .collect(),
            ];
            for input in inputs {
                for level in 0..=9 {
                    let budget = Budget::new(8 * 1024 * 1024, "zlib test working bytes");
                    let encoded = compress(&input, level, &budget).unwrap();
                    let mut reference = ZlibEncoder::new(Vec::new(), Compression::new(level));
                    reference.write_all(&input).unwrap();
                    assert_eq!(
                        encoded.as_slice(),
                        reference.finish().unwrap(),
                        "size={size}, level={level}"
                    );
                    assert_eq!(
                        decompress(&encoded, input.len(), &budget)
                            .unwrap()
                            .as_slice(),
                        input
                    );
                }
            }
        }
    }

    #[test]
    fn workspace_admission_and_exact_stream_checks() {
        let budget = Budget::new(DEFLATE_WORKSPACE - 1, "zlib test working bytes");
        assert!(matches!(
            compress(b"a", 9, &budget),
            Err(crate::Error::ResourceLimit { .. })
        ));
        assert_eq!(budget.remaining(), DEFLATE_WORKSPACE - 1);
        let budget = Budget::new(1024 * 1024, "zlib test working bytes");
        let encoded = compress(b"hello", 9, &budget).unwrap();
        assert!(decompress(&encoded, 4, &budget).is_err());
        assert!(decompress(&encoded, 6, &budget).is_err());
        assert!(decompress(&encoded[..encoded.len() - 1], 5, &budget).is_err());
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert!(decompress(&trailing, 5, &budget).is_err());
    }
}
