//! Shared owned-allocation leases for a single codec or outline operation.
//!
//! This counts registered owned capacity, including the old/new overlap during
//! buffer growth. It does not measure allocator overhead, stack, or unregistered
//! dependency workspace. Borrowed caller storage is reserved by the caller.
use crate::{Error, Result};
use std::{
    mem::size_of,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct State {
    used: AtomicUsize,
    maximum: usize,
    resource: &'static str,
    /// High-water mark of admitted bytes, for exact-capacity regressions.
    #[cfg(test)]
    peak: AtomicUsize,
}

#[derive(Clone)]
pub(crate) struct Budget(Arc<State>);

impl Budget {
    pub(crate) fn new(maximum: usize, resource: &'static str) -> Self {
        Self(Arc::new(State {
            used: AtomicUsize::new(0),
            maximum,
            resource,
            #[cfg(test)]
            peak: AtomicUsize::new(0),
        }))
    }

    /// Highest simultaneous admitted total observed by this ledger.
    #[cfg(test)]
    pub(crate) fn peak(&self) -> usize {
        self.0.peak.load(Ordering::Relaxed)
    }

    pub(crate) fn error(&self) -> Error {
        Error::ResourceLimit {
            resource: self.0.resource,
            limit: self.0.maximum,
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.0.maximum - self.0.used.load(Ordering::Relaxed)
    }

    pub(crate) fn reserve(&self, bytes: usize) -> Result<Lease> {
        self.0
            .used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes)
                    .filter(|&next| next <= self.0.maximum)
            })
            .map_err(|_| self.error())?;
        #[cfg(test)]
        self.0
            .peak
            .fetch_max(self.0.used.load(Ordering::Relaxed), Ordering::Relaxed);
        Ok(Lease {
            budget: self.clone(),
            bytes,
        })
    }

    pub(crate) fn reserve_array<T>(&self, count: usize) -> Result<Lease> {
        self.reserve(
            count
                .checked_mul(size_of::<T>())
                .ok_or_else(|| self.error())?,
        )
    }

    /// Conservative standard-library tree storage, without relying on a
    /// toolchain-specific minimum node occupancy. Nested key/value allocations
    /// are separate. Include an additional root while insertions are in flight.
    #[cfg(any(feature = "sfnt", test))]
    pub(crate) fn tree_bytes<K, V>(&self, entries: usize) -> Result<usize> {
        let slot = size_of::<K>()
            .checked_add(size_of::<V>())
            .and_then(|n| n.checked_add(size_of::<usize>()))
            .ok_or_else(|| self.error())?;
        let node = slot
            .checked_mul(16)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(|| self.error())?;
        entries
            .checked_add(1)
            .and_then(|n| n.checked_mul(node))
            .ok_or_else(|| self.error())
    }

    #[cfg(any(feature = "sfnt", test))]
    pub(crate) fn reserve_tree<K, V>(&self, entries: usize) -> Result<Lease> {
        self.reserve(self.tree_bytes::<K, V>(entries)?)
    }

    /// Allocate exactly `capacity` elements after admitting them, and move the
    /// charge (for the actual capacity) into `lease`. The caller must keep
    /// `lease` alive until the returned vector has dropped, and must not grow
    /// the vector beyond this capacity.
    #[cfg(any(feature = "sfnt", test))]
    pub(crate) fn vec_into<T>(&self, capacity: usize, lease: &mut Lease) -> Result<Vec<T>> {
        let (values, mut charge) = Vector::with_capacity(self, capacity)?.into_parts();
        if let Err(error) = lease.merge(&mut charge) {
            drop(values);
            return Err(error);
        }
        Ok(values)
    }
}

/// Make room for `additional` more elements in a plain `Vec` whose array
/// storage `lease` charges. A new array is admitted while the old one is
/// still live; growth is geometric unless that would not fit. Nested element
/// allocations need their own leases.
#[cfg(any(
    feature = "otb",
    feature = "bdat",
    feature = "color",
    feature = "vector-raster"
))]
pub(crate) fn grow_vec<T>(
    budget: &Budget,
    values: &mut Vec<T>,
    lease: &mut Lease,
    additional: usize,
) -> Result<()> {
    let size = size_of::<T>().max(1);
    let required = values
        .len()
        .checked_add(additional)
        .ok_or_else(|| budget.error())?;
    if required <= values.capacity() {
        return Ok(());
    }
    let capacity = required
        .max(values.capacity().saturating_mul(2))
        .max(4)
        .min(budget.remaining() / size)
        .max(required);
    let mut next_lease = budget.reserve_array::<T>(capacity)?;
    let mut next = Vec::new();
    next.try_reserve_exact(capacity)
        .map_err(|_| budget.error())?;
    next_lease.grow_to(
        next.capacity()
            .checked_mul(size_of::<T>())
            .ok_or_else(|| budget.error())?,
    )?;
    next.append(values);
    drop(std::mem::replace(values, next));
    // Only after the previous array is gone may its charge go.
    *lease = next_lease;
    Ok(())
}

/// Format `parts` into an exactly sized string whose bytes are added to
/// `lease` before the string is allocated.
#[cfg(any(
    feature = "otb",
    feature = "bdat",
    feature = "color",
    feature = "vector-raster"
))]
pub(crate) fn exact_string(lease: &mut Lease, parts: std::fmt::Arguments<'_>) -> Result<String> {
    struct Count(usize);
    impl std::fmt::Write for Count {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0 += s.len();
            Ok(())
        }
    }
    let text_error = || Error::InvalidInput("working string formatting failed".into());
    let mut count = Count(0);
    std::fmt::Write::write_fmt(&mut count, parts).map_err(|_| text_error())?;
    let before = lease.bytes();
    let error = lease.budget.error();
    lease.grow_to(before.checked_add(count.0).ok_or(error)?)?;
    let mut text = String::new();
    text.try_reserve_exact(count.0)
        .map_err(|_| lease.budget.error())?;
    std::fmt::Write::write_fmt(&mut text, parts).map_err(|_| text_error())?;
    let error = lease.budget.error();
    lease.grow_to(before.checked_add(text.capacity()).ok_or(error)?)?;
    Ok(text)
}

/// Keep this lease alive until its corresponding storage has actually dropped.
pub(crate) struct Lease {
    budget: Budget,
    bytes: usize,
}

impl Lease {
    /// Pre-admit additional owned storage. This method never refunds storage.
    pub(crate) fn grow_to(&mut self, bytes: usize) -> Result<()> {
        if bytes > self.bytes {
            let mut additional = self.budget.reserve(bytes - self.bytes)?;
            self.bytes = bytes;
            // Transfer the new charge into this lease without releasing it.
            additional.bytes = 0;
        }
        Ok(())
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Combine charges after the associated owned values move into one stage.
    /// On failure both leases retain their charges and caller storage remains
    /// admitted. Taking a mutable reference prevents an error from dropping it.
    pub(crate) fn merge(&mut self, other: &mut Self) -> Result<()> {
        if !Arc::ptr_eq(&self.budget.0, &other.budget.0) {
            return Err(Error::InvalidInput(
                "working leases belong to different budgets".into(),
            ));
        }
        self.bytes = self
            .bytes
            .checked_add(other.bytes)
            .ok_or_else(|| self.budget.error())?;
        other.bytes = 0;
        Ok(())
    }

    /// Move `bytes` of this live charge into a new lease on the same ledger,
    /// without releasing anything. Use it when part of the covered storage
    /// moves to an owner with a different lifetime.
    #[cfg(any(feature = "sfnt", test))]
    pub(crate) fn split(&mut self, bytes: usize) -> Result<Self> {
        if bytes > self.bytes {
            return Err(Error::InvalidInput(
                "working lease split exceeds its charge".into(),
            ));
        }
        self.bytes -= bytes;
        Ok(Self {
            budget: self.budget.clone(),
            bytes,
        })
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.budget.0.used.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

/// Owned storage and its admission charge share a lifetime. Field order ensures
/// the storage is destroyed before its lease releases the reserved capacity.
pub(crate) struct Owned<T> {
    value: T,
    lease: Lease,
}

impl<T> Owned<T> {
    /// The lease must be admitted before allocating the corresponding value.
    pub(crate) fn from_parts(value: T, lease: Lease) -> Self {
        Self { value, lease }
    }

    pub(crate) fn into_parts(self) -> (T, Lease) {
        (self.value, self.lease)
    }
}

impl<T> Deref for Owned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Owned<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Fallible array storage; nested element allocations need their own leases.
/// Only a mutable slice is exposed, so callers cannot grow capacity unchecked.
pub(crate) struct Vector<T> {
    values: Vec<T>,
    lease: Lease,
}

pub(crate) type Buffer = Vector<u8>;

/// A fallible outer array whose elements carry additional owned allocations.
/// Drop the complete array (including nested values) before refunding its charge.
pub(crate) struct Nested<T> {
    values: Vector<T>,
    nested: Lease,
}

impl<T> Nested<T> {
    pub(crate) fn with_capacity(budget: &Budget, capacity: usize) -> Result<Self> {
        Ok(Self {
            values: Vector::with_capacity(budget, capacity)?,
            nested: budget.reserve(0)?,
        })
    }

    pub(crate) fn push(&mut self, mut value: Owned<T>) -> Result<()> {
        self.values.reserve(1)?;
        self.nested.merge(&mut value.lease)?;
        // Capacity was admitted above, so moving the value cannot allocate.
        self.values.values.push(value.value);
        Ok(())
    }

    pub(crate) fn into_owned(mut self) -> Result<Owned<Vec<T>>> {
        self.values.lease.merge(&mut self.nested)?;
        Ok(self.values.into_owned())
    }
}

impl<T> Deref for Nested<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.values
    }
}

impl<T> DerefMut for Nested<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.values
    }
}

impl<T> Vector<T> {
    pub(crate) fn new(budget: &Budget) -> Result<Self> {
        Ok(Self {
            values: Vec::new(),
            lease: budget.reserve(0)?,
        })
    }

    pub(crate) fn with_capacity(budget: &Budget, capacity: usize) -> Result<Self> {
        let mut result = Self::new(budget)?;
        result.reserve_exact(capacity)?;
        Ok(result)
    }

    /// Admit and copy borrowed values into new owned storage.
    #[cfg(any(feature = "woff", feature = "woff2", test))]
    pub(crate) fn from_slice(budget: &Budget, values: &[T]) -> Result<Self>
    where
        T: Copy,
    {
        let mut result = Self::with_capacity(budget, values.len())?;
        result.extend(values)?;
        Ok(result)
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.values
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.values
    }

    pub(crate) fn reserve_exact(&mut self, additional: usize) -> Result<()> {
        self.reserve_inner(additional, false)
    }

    pub(crate) fn reserve(&mut self, additional: usize) -> Result<()> {
        self.reserve_inner(additional, true)
    }

    fn reserve_inner(&mut self, additional: usize, geometric: bool) -> Result<()> {
        let required = self
            .values
            .len()
            .checked_add(additional)
            .ok_or_else(|| self.lease.budget.error())?;
        if required > self.values.capacity() {
            let budget = &self.lease.budget;
            // A new buffer is charged alongside the still-live old buffer.
            // Prefer geometric growth, but fall back to the exact required
            // capacity when spare capacity would exceed the working allowance.
            let available = if size_of::<T>() == 0 {
                usize::MAX
            } else {
                budget.remaining() / size_of::<T>()
            };
            let capacity = if geometric {
                required
                    .max(self.values.capacity().saturating_mul(2))
                    .min(available)
            } else {
                required
            };
            if capacity < required || capacity > available {
                return Err(budget.error());
            }
            let mut lease = budget.reserve_array::<T>(capacity)?;
            let mut next = Vec::new();
            next.try_reserve_exact(capacity)
                .map_err(|_| budget.error())?;
            lease.grow_to(
                next.capacity()
                    .checked_mul(size_of::<T>())
                    .ok_or_else(|| budget.error())?,
            )?;
            next.append(&mut self.values);
            let old = std::mem::replace(&mut self.values, next);
            drop(old);
            // Only after the previous allocation is gone may its charge die.
            self.lease = lease;
        }
        Ok(())
    }

    pub(crate) fn push(&mut self, value: T) -> Result<()> {
        self.reserve(1)?;
        self.values.push(value);
        Ok(())
    }

    /// Let an infallible encoder append to (or truncate) the underlying
    /// vector in place. Callers [`reserve`](Self::reserve) the exact bytes
    /// the encoder will add first, so no reallocation happens here; if the
    /// encoder grew the vector anyway, its new capacity is charged before
    /// returning (an error if the allowance cannot cover it).
    #[cfg(feature = "cff")]
    pub(crate) fn append_with<R>(&mut self, write: impl FnOnce(&mut Vec<T>) -> R) -> Result<R> {
        let result = write(&mut self.values);
        let bytes = self
            .values
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or_else(|| self.lease.budget.error())?;
        self.lease.grow_to(bytes)?;
        Ok(result)
    }

    pub(crate) fn extend(&mut self, values: &[T]) -> Result<()>
    where
        T: Copy,
    {
        self.reserve(values.len())?;
        self.values.extend_from_slice(values);
        Ok(())
    }

    pub(crate) fn resize(&mut self, length: usize, value: T) -> Result<()>
    where
        T: Copy,
    {
        if length > self.values.len() {
            self.reserve(length - self.values.len())?;
        }
        self.values.resize(length, value);
        Ok(())
    }

    pub(crate) fn truncate(&mut self, length: usize) {
        self.values.truncate(length);
    }

    /// Transfer storage and its live charge together to another codec stage.
    pub(crate) fn into_parts(self) -> (Vec<T>, Lease) {
        (self.values, self.lease)
    }

    pub(crate) fn into_owned(self) -> Owned<Vec<T>> {
        Owned::from_parts(self.values, self.lease)
    }
}

impl<T> Deref for Vector<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T> DerefMut for Vector<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl std::io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.extend(bytes).map_err(std::io::Error::other)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leases_share_admission_and_release_only_their_own_charge() {
        let budget = Budget::new(10, "test working bytes");
        let first = budget.reserve(6).unwrap();
        let second = budget.clone().reserve(4).unwrap();
        assert!(budget.reserve(1).is_err());
        drop(first);
        assert_eq!(budget.remaining(), 6);
        drop(second);
        assert_eq!(budget.remaining(), 10);
        assert!(budget.reserve(usize::MAX).is_err());
        assert_eq!(budget.remaining(), 10);
    }

    #[test]
    fn peak_records_the_highest_simultaneous_admission() {
        let budget = Budget::new(100, "test working bytes");
        let first = budget.reserve(30).unwrap();
        let second = budget.reserve(50).unwrap();
        drop(first);
        let copied = Vector::from_slice(&budget, b"0123456789").unwrap();
        assert_eq!(copied.as_slice(), b"0123456789");
        drop((second, copied));
        assert_eq!((budget.peak(), budget.remaining()), (80, 100));
    }

    #[test]
    fn growth_accounts_for_live_old_and_new_storage() {
        let budget = Budget::new(11, "test working bytes");
        let mut buffer = Buffer::new(&budget).unwrap();
        buffer.extend(b"1234").unwrap();
        // 4 old bytes plus 8 requested bytes cannot coexist under eleven.
        assert!(buffer.extend(b"5678").is_err());
        assert_eq!(buffer.as_slice(), b"1234");
        assert_eq!(budget.remaining(), 7);
        // An exact five-byte replacement fits alongside the old four bytes.
        buffer.extend(b"5").unwrap();
        let (bytes, lease) = buffer.into_parts();
        assert_eq!(bytes, b"12345");
        assert!(bytes.capacity() <= 7);
        drop(bytes);
        drop(lease);
        assert_eq!(budget.remaining(), 11);
    }

    #[test]
    fn typed_capacity_uses_element_size_and_keeps_its_charge_after_transfer() {
        let budget = Budget::new(3 * size_of::<u64>(), "test working bytes");
        let mut values = Vector::<u64>::with_capacity(&budget, 2).unwrap();
        values.push(7).unwrap();
        values.push(9).unwrap();
        assert_eq!(budget.remaining(), size_of::<u64>());
        assert!(values.push(11).is_err());
        let owned = values.into_owned();
        assert_eq!(owned.as_slice(), &[7, 9]);
        assert_eq!(budget.remaining(), size_of::<u64>());
        drop(owned);
        assert_eq!(budget.remaining(), 3 * size_of::<u64>());
        assert!(budget.reserve_array::<u64>(usize::MAX).is_err());
    }

    #[test]
    fn owned_value_drops_before_its_lease() {
        struct ObserveDrop(Budget);
        impl Drop for ObserveDrop {
            fn drop(&mut self) {
                assert_eq!(self.0.remaining(), 0);
            }
        }
        let budget = Budget::new(1, "test working bytes");
        let lease = budget.reserve(1).unwrap();
        let owned = Owned::from_parts(ObserveDrop(budget.clone()), lease);
        drop(owned);
        assert_eq!(budget.remaining(), 1);
    }

    #[test]
    fn tree_estimate_preserves_the_root_and_checks_count_overflow() {
        let node = 16 * (size_of::<u32>() + size_of::<u16>() + size_of::<usize>()) + 128;
        let budget = Budget::new(2 * node, "test working bytes");
        assert_eq!(budget.tree_bytes::<u32, u16>(0).unwrap(), node);
        let lease = budget.reserve_tree::<u32, u16>(1).unwrap();
        assert_eq!(lease.bytes(), 2 * node);
        assert_eq!(budget.remaining(), 0);
        assert!(budget.tree_bytes::<u32, u16>(usize::MAX).is_err());
        drop(lease);
        assert_eq!(budget.remaining(), 2 * node);
    }

    #[test]
    fn rejected_merge_preserves_both_live_charges() {
        let first = Budget::new(9, "first budget");
        let second = Budget::new(7, "second budget");
        let mut a = first.reserve(9).unwrap();
        let mut b = second.reserve(7).unwrap();
        assert!(matches!(a.merge(&mut b), Err(Error::InvalidInput(_))));
        assert_eq!((first.remaining(), second.remaining()), (0, 0));
        assert_eq!((a.bytes(), b.bytes()), (9, 7));
        drop(b);
        assert_eq!((first.remaining(), second.remaining()), (0, 7));
        drop(a);
        assert_eq!(first.remaining(), 9);
    }

    #[test]
    fn exact_vectors_merge_their_charge_into_a_shared_lease() {
        let budget = Budget::new(3 * size_of::<u32>() + 2, "exact budget");
        let mut lease = budget.reserve(0).unwrap();
        let values = budget.vec_into::<u32>(3, &mut lease).unwrap();
        assert!(values.capacity() >= 3);
        assert_eq!(lease.bytes(), values.capacity() * size_of::<u32>());
        let bytes = budget.vec_into::<u8>(2, &mut lease).unwrap();
        assert_eq!(budget.remaining(), 0);
        // A refused allocation leaves the existing charges unchanged.
        assert!(budget.vec_into::<u8>(1, &mut lease).is_err());
        assert_eq!(lease.bytes(), 3 * size_of::<u32>() + 2);
        drop((values, bytes));
        drop(lease);
        assert_eq!(budget.remaining(), 3 * size_of::<u32>() + 2);
    }

    #[test]
    fn split_moves_part_of_a_live_charge_without_release() {
        let budget = Budget::new(10, "split budget");
        let mut lease = budget.reserve(10).unwrap();
        let part = lease.split(4).unwrap();
        assert_eq!((lease.bytes(), part.bytes(), budget.remaining()), (6, 4, 0));
        assert!(matches!(lease.split(7), Err(Error::InvalidInput(_))));
        assert_eq!((lease.bytes(), budget.remaining()), (6, 0));
        drop(lease);
        assert_eq!(budget.remaining(), 6);
        drop(part);
        assert_eq!(budget.remaining(), 10);
    }

    #[test]
    fn nested_storage_retains_its_capacity_until_the_output_drops() {
        let maximum = size_of::<Vec<u8>>() + 5;
        let budget = Budget::new(maximum, "nested budget");
        let mut values = Nested::with_capacity(&budget, 1).unwrap();
        let mut bytes = Buffer::with_capacity(&budget, 5).unwrap();
        bytes.extend(b"hello").unwrap();
        values.push(bytes.into_owned()).unwrap();
        let output = values.into_owned().unwrap();
        assert_eq!(&output[0], b"hello");
        assert_eq!(budget.remaining(), 0);
        drop(output);
        assert_eq!(budget.remaining(), maximum);
    }
}
