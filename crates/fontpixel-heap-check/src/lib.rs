//! Process-wide counting allocator for measuring the peak heap of one call.
//!
//! Every allocation in the process (all threads) is counted by requested
//! layout size. [`measure`] serializes measurements and reports the highest
//! simultaneous total reached during the call, minus the total live when it
//! started. It measures requested bytes, not allocator overhead or RSS.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

/// Counting wrapper around the system allocator.
pub struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

fn grow(bytes: usize) {
    let now = CURRENT.fetch_add(bytes, SeqCst) + bytes;
    PEAK.fetch_max(now, SeqCst);
}

// SAFETY: every method forwards to `System` with the caller's layout and
// pointer unchanged; the counters never affect the returned memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded contract of `GlobalAlloc::alloc`.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded contract of `GlobalAlloc::alloc_zeroed`.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded contract of `GlobalAlloc::dealloc`.
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), SeqCst);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // A reallocation may need old and new storage at once; count both
        // while it is in flight, as a ledger must.
        grow(new_size);
        // SAFETY: forwarded contract of `GlobalAlloc::realloc`.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if p.is_null() {
            CURRENT.fetch_sub(new_size, SeqCst);
        } else {
            CURRENT.fetch_sub(layout.size(), SeqCst);
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Run `f` and return its result with the peak number of additional heap
/// bytes that were live at any moment during the call (all threads).
pub fn measure<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let base = CURRENT.load(SeqCst);
    PEAK.store(base, SeqCst);
    let result = f();
    let peak = PEAK.load(SeqCst).saturating_sub(base);
    (result, peak)
}

/// Bytes currently live in the process.
pub fn live() -> usize {
    CURRENT.load(SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_peak_including_freed_storage() {
        let (_, peak) = measure(|| {
            let a = vec![0u8; 1 << 20];
            drop(a);
            let b = vec![0u8; 1 << 10];
            b.len()
        });
        assert!(peak >= 1 << 20);
        assert!(peak < (1 << 20) + (64 << 10));
    }
}
