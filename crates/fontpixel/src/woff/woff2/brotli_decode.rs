//! Fallible cell accounting for the locked Brotli streaming decoder.
//! Encoder allocation paths have a different contract and do not use this code.
use super::invalid;
use crate::{
    Error, Result,
    sfnt::working::{Budget, Buffer, Owned, Vector},
};
use brotli::{Allocator, SliceWrapper, SliceWrapperMut};
#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;

struct Memory<T>(Option<Vector<T>>);
impl<T> Default for Memory<T> {
    fn default() -> Self {
        Self(None)
    }
}
impl<T> SliceWrapper<T> for Memory<T> {
    fn slice(&self) -> &[T] {
        self.0.as_ref().map_or(&[], Vector::as_slice)
    }
}
impl<T> SliceWrapperMut<T> for Memory<T> {
    fn slice_mut(&mut self) -> &mut [T] {
        self.0.as_mut().map_or(&mut [], Vector::as_mut_slice)
    }
}
#[derive(Clone, Copy)]
struct DecodeAllocator<'a> {
    budget: &'a Budget,
    failure: &'a RefCell<Option<Error>>,
    #[cfg(test)]
    attempts: &'a Cell<usize>,
    #[cfg(test)]
    fail_at: Option<usize>,
}
impl<T: Copy + Default> Allocator<T> for DecodeAllocator<'_> {
    type AllocatedMemory = Memory<T>;
    fn alloc_cell(&mut self, len: usize) -> Memory<T> {
        if len == 0 || self.failure.borrow().is_some() {
            return Memory::default();
        }
        #[cfg(test)]
        {
            let attempt = self.attempts.get();
            self.attempts.set(attempt + 1);
            if self.fail_at == Some(attempt) {
                *self.failure.borrow_mut() = Some(Error::ResourceLimit {
                    resource: "injected Brotli decoder allocation",
                    limit: attempt,
                });
                return Memory::default();
            }
        }
        let result = (|| {
            let mut memory = Vector::with_capacity(self.budget, len)?;
            memory.resize(len, T::default())?;
            Ok(memory)
        })();
        match result {
            Ok(memory) => Memory(Some(memory)),
            Err(error) => {
                *self.failure.borrow_mut() = Some(error);
                Memory::default()
            }
        }
    }
    fn free_cell(&mut self, memory: Memory<T>) {
        drop(memory);
    }
}

pub(super) fn decompress(data: &[u8], expected: usize, budget: &Budget) -> Result<Owned<Vec<u8>>> {
    let failure = RefCell::new(None);
    #[cfg(test)]
    let attempts = Cell::new(0);
    let allocator = DecodeAllocator {
        budget,
        failure: &failure,
        #[cfg(test)]
        attempts: &attempts,
        #[cfg(test)]
        fail_at: None,
    };
    decompress_with_allocator(data, expected, allocator)
}
fn decompress_with_allocator(
    data: &[u8],
    expected: usize,
    allocator: DecodeAllocator<'_>,
) -> Result<Owned<Vec<u8>>> {
    let length = expected
        .checked_add(1)
        .ok_or_else(|| invalid("length overflow"))?;
    let mut out = Buffer::with_capacity(allocator.budget, length)?;
    out.resize(length, 0)?;
    let mut state = brotli_decompressor::BrotliState::new(allocator, allocator, allocator);
    // BrotliState::new assigns context_map_table but does not validate its size.
    // Do not enter the stream decoder if that first allocation failed.
    if let Some(error) = allocator.failure.borrow_mut().take() {
        return Err(error);
    }
    let (mut avail_in, mut input_offset, mut avail_out, mut output_offset, mut total_out) =
        (data.len(), 0, out.len(), 0, 0);
    let result = brotli_decompressor::BrotliDecompressStream(
        &mut avail_in,
        &mut input_offset,
        data,
        &mut avail_out,
        &mut output_offset,
        &mut out,
        &mut total_out,
        &mut state,
    );
    // All remaining stream allocation sites check empty cells before indexing.
    // Preserve the first failure's ResourceLimit rather than a generic codec error.
    if let Some(error) = allocator.failure.borrow_mut().take() {
        return Err(error);
    }
    if !matches!(result, brotli_decompressor::BrotliResult::ResultSuccess)
        || input_offset != data.len()
        || output_offset != expected
    {
        return Err(invalid("Brotli stream or decoded length mismatch"));
    }
    out.truncate(expected);
    Ok(out.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_length_cells_need_no_budget_and_failure_is_sticky() {
        let budget = Budget::new(0, "Brotli decoder test bytes");
        let failure = RefCell::new(None);
        let attempts = Cell::new(0);
        let mut allocator = DecodeAllocator {
            budget: &budget,
            failure: &failure,
            attempts: &attempts,
            fail_at: None,
        };
        let empty = <DecodeAllocator<'_> as Allocator<u32>>::alloc_cell(&mut allocator, 0);
        assert!(empty.slice().is_empty());
        assert!(failure.borrow().is_none());
        assert_eq!(attempts.get(), 0);
        let failed = <DecodeAllocator<'_> as Allocator<u32>>::alloc_cell(&mut allocator, 1);
        assert!(failed.slice().is_empty());
        assert!(matches!(
            failure.borrow().as_ref(),
            Some(Error::ResourceLimit { .. })
        ));
        let failed_again = <DecodeAllocator<'_> as Allocator<u8>>::alloc_cell(&mut allocator, 1);
        assert!(failed_again.slice().is_empty());
        assert_eq!(attempts.get(), 1);
        drop((empty, failed, failed_again));
        assert_eq!(budget.remaining(), 0);
    }

    fn compressed(input: &[u8], quality: i32) -> Vec<u8> {
        let mut output = Vec::new();
        let params = brotli::enc::BrotliEncoderParams {
            quality,
            lgwin: 22,
            ..Default::default()
        };
        brotli::BrotliCompress(&mut &input[..], &mut output, &params).unwrap();
        output
    }
    #[test]
    fn every_requested_cell_can_fail_without_early_refund_or_error_erasure() {
        let mut random = 0x713a_649du32;
        let random_data = (0..70_000)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect::<Vec<_>>();
        for input in [
            Vec::new(),
            b"hello world hello world".to_vec(),
            vec![b'a'; 200_000],
            random_data,
        ] {
            for quality in [0, 6] {
                let wire = compressed(&input, quality);
                let maximum = 32 * 1024 * 1024;
                let budget = Budget::new(maximum, "Brotli decoder test bytes");
                let failure = RefCell::new(None);
                let attempts = Cell::new(0);
                let allocator = DecodeAllocator {
                    budget: &budget,
                    failure: &failure,
                    attempts: &attempts,
                    fail_at: None,
                };
                let output = decompress_with_allocator(&wire, input.len(), allocator).unwrap();
                assert_eq!(output.as_slice(), input);
                let count = attempts.get();
                drop(output);
                assert_eq!(budget.remaining(), maximum);
                for fail_at in 0..count {
                    attempts.set(0);
                    let allocator = DecodeAllocator {
                        fail_at: Some(fail_at),
                        ..allocator
                    };
                    let error = decompress_with_allocator(&wire, input.len(), allocator)
                        .err()
                        .unwrap();
                    assert!(
                        matches!(error, Error::ResourceLimit { resource: "injected Brotli decoder allocation", limit } if limit == fail_at)
                    );
                    assert_eq!(budget.remaining(), maximum);
                }
            }
        }
    }
    #[test]
    fn malformed_input_keeps_codec_error_and_capacity_failure_keeps_resource_error() {
        let wire = compressed(b"hello", 6);
        for maximum in [1, 4, 8] {
            let maximum = maximum * 1024 * 1024;
            let probe = Budget::new(maximum, "Brotli decoder probe bytes");
            let output = decompress(&wire, 5, &probe).unwrap();
            assert_eq!(output.as_slice(), b"hello");
            drop(output);
            assert_eq!(probe.remaining(), maximum);
        }
        // The complete short stream permits a small ring buffer. Truncating its
        // final byte makes the decoder request the declared window first, so a
        // low working allowance correctly wins over the eventual format error.
        let low = Budget::new(1024 * 1024, "Brotli decoder test bytes");
        let truncated = decompress(&wire[..wire.len() - 1], 5, &low)
            .err()
            .expect("a truncated Brotli stream must fail");
        assert!(
            matches!(
                &truncated,
                Error::ResourceLimit {
                    resource: "Brotli decoder test bytes",
                    limit: 1_048_576
                }
            ),
            "unexpected low-budget truncated-stream error: {truncated:?}"
        );
        assert_eq!(low.remaining(), 1024 * 1024);

        // Isolate format validation from workspace admission. This is only the
        // test fixture's allowance; no production limit or parameter changes.
        let maximum = 8 * 1024 * 1024;
        let budget = Budget::new(maximum, "Brotli decoder test bytes");
        let mut trailing = wire.clone();
        trailing.push(0);
        for (case, data, expected) in [
            ("truncated", &wire[..wire.len() - 1], 5),
            ("trailing", trailing.as_slice(), 5),
            ("short declared length", wire.as_slice(), 4),
            ("long declared length", wire.as_slice(), 6),
        ] {
            let error = decompress(data, expected, &budget)
                .err()
                .expect("malformed fixture must fail");
            assert!(
                matches!(&error, Error::InvalidInput(_)),
                "{case}: {error:?}"
            );
            assert_eq!(budget.remaining(), maximum);
        }
        let small = Budget::new(6, "Brotli decoder test bytes");
        assert!(matches!(
            decompress(&wire, 5, &small),
            Err(Error::ResourceLimit { .. })
        ));
        assert_eq!(small.remaining(), 6);
    }
}
