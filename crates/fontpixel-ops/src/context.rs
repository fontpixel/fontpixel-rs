//! Per-call execution scope: storage mode, source identities and output capture.
//!
//! Every public operation installs one scope for its duration. Standalone calls
//! partition their original total byte limit into a fixed source-identity slice
//! and the remaining working allowance; batch tasks capture prepared outputs in
//! the shared pending ledger instead. The scope is thread-local so that batch
//! workers each carry their own capture while sharing one source table.
use crate::memory::{self, Storage};
use fontpixel::resources::OutputBundle;
use fontpixel::{Error, Input, ResourceLimits, Result};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[cfg(feature = "fs")]
pub(crate) use crate::assets::Sources;

/// Without filesystem support no file identities exist to protect.
#[cfg(not(feature = "fs"))]
#[derive(Clone, Default)]
pub(crate) struct Sources {
    _private: (),
}
#[cfg(not(feature = "fs"))]
impl Sources {
    pub(crate) fn is_single_primary(&self) -> bool {
        false
    }
    pub(crate) fn set_retained_budget(
        &self,
        _used: Arc<Mutex<usize>>,
        _limit: usize,
    ) -> Result<()> {
        Ok(())
    }
}

/// One prepared output: an entry-point destination and every asset written
/// beside it. Nothing reaches storage until its whole group is accepted.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedOutput {
    /// Requested entry-point path (file name of the primary asset).
    pub destination: PathBuf,
    /// The primary asset plus any sidecars or directory members.
    pub bundle: OutputBundle,
}

pub(crate) struct Capture {
    pub(crate) storage: Storage,
    pub(crate) sources: Sources,
    pub(crate) outputs: Vec<PreparedOutput>,
    pub(crate) pending: Arc<Mutex<usize>>,
    pub(crate) limit: usize,
    pub(crate) charged: usize,
}
struct SourceScope {
    storage: Storage,
    sources: Sources,
    identity_reservation: usize,
    outputs: Vec<PreparedOutput>,
}
thread_local! {
    pub(crate) static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
    static SOURCE_SCOPE: RefCell<Option<SourceScope>> = const { RefCell::new(None) };
}
fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
fn with_scope<T>(scope: SourceScope, f: impl FnOnce() -> T) -> (T, Vec<PreparedOutput>) {
    struct Restore(Option<SourceScope>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SOURCE_SCOPE.with(|s| *s.borrow_mut() = self.0.take());
        }
    }
    let restore = Restore(SOURCE_SCOPE.with(|s| s.borrow_mut().replace(scope)));
    let result = f();
    let outputs = SOURCE_SCOPE.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(|scope| std::mem::take(&mut scope.outputs))
            .unwrap_or_default()
    });
    drop(restore);
    (result, outputs)
}
#[cfg(all(
    test,
    feature = "clap",
    feature = "opentype",
    feature = "bdf",
    feature = "fs"
))]
pub(crate) fn with_source_scope<T>(f: impl FnOnce() -> T) -> T {
    with_scope(
        SourceScope {
            storage: Storage::Filesystem,
            sources: Sources::default(),
            identity_reservation: 0,
            outputs: Vec::new(),
        },
        f,
    )
    .0
}
pub(crate) fn source_identity_reservation(original_total: usize) -> usize {
    (original_total / 256).min(1024 * 1024)
}
/// Reserve standalone source identities from the original command total before
/// decoding. Retain the complete slice through persistence: dynamic sidecars may
/// grow the identity table while a codec uses its remaining working allowance.
/// The default total remains 256 MiB, split into 1 MiB identities and 255 MiB work.
/// This closes source-table admission, not other codec transients or process RSS.
///
/// Memory storage applies the same partition so that resource outcomes match
/// the filesystem mode; its in-memory outputs are returned with the result.
pub(crate) fn with_bounded_source_scope<T>(
    storage: &Storage,
    original_total: usize,
    f: impl FnOnce() -> Result<T>,
) -> Result<(T, Vec<PreparedOutput>)> {
    if is_active() {
        return Err(invalid(
            "standalone source scope cannot replace batch capture",
        ));
    }
    let identity_reservation = source_identity_reservation(original_total);
    let sources = Sources::default();
    sources.set_retained_budget(Arc::new(Mutex::new(0)), identity_reservation)?;
    let (result, outputs) = with_scope(
        SourceScope {
            storage: storage.clone(),
            sources,
            identity_reservation,
            outputs: Vec::new(),
        },
        f,
    );
    result.map(|value| (value, outputs))
}
/// Apply only when constructing original command limits. Already reduced
/// InputBudget, writer and codec limits must never pass through this helper.
pub(crate) fn standalone_working_limits(mut limits: ResourceLimits) -> ResourceLimits {
    if !is_active() {
        SOURCE_SCOPE.with(|s| {
            if let Some(scope) = s.borrow().as_ref() {
                limits.max_total_bytes = limits
                    .max_total_bytes
                    .saturating_sub(scope.identity_reservation);
            }
        });
    }
    limits
}
pub(crate) fn is_active() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}
pub(crate) fn sources() -> Sources {
    CAPTURE.with(|c| {
        c.borrow()
            .as_ref()
            .map(|c| c.sources.clone())
            .unwrap_or_else(|| {
                SOURCE_SCOPE.with(|s| {
                    s.borrow()
                        .as_ref()
                        .map(|scope| scope.sources.clone())
                        .unwrap_or_default()
                })
            })
    })
}
/// The storage of the active scope. Outside any scope (direct internal use in
/// tests) the filesystem is used when available.
pub(crate) fn storage() -> Result<Storage> {
    let active = CAPTURE.with(|c| c.borrow().as_ref().map(|c| c.storage.clone()));
    if let Some(storage) = active {
        return Ok(storage);
    }
    let scoped = SOURCE_SCOPE.with(|s| s.borrow().as_ref().map(|s| s.storage.clone()));
    if let Some(storage) = scoped {
        return Ok(storage);
    }
    #[cfg(feature = "fs")]
    {
        Ok(Storage::Filesystem)
    }
    #[cfg(not(feature = "fs"))]
    {
        Err(invalid("no storage is active for this operation"))
    }
}

/// An opened primary input and its sidecar resolver.
pub(crate) enum OpenedInput {
    #[cfg(feature = "fs")]
    File(crate::assets::FileInput),
    Memory(memory::MemoryInput),
}
impl OpenedInput {
    pub(crate) fn input(&self) -> Input<'_> {
        match self {
            #[cfg(feature = "fs")]
            Self::File(file) => file.input(),
            Self::Memory(memory) => memory.input(),
        }
    }
}
/// Open one primary input (file or directory) through the active storage.
pub(crate) fn open_input(
    path: &Path,
    limits: &ResourceLimits,
    sources: Sources,
) -> Result<OpenedInput> {
    match storage()? {
        #[cfg(feature = "fs")]
        Storage::Filesystem => Ok(OpenedInput::File(crate::assets::FileInput::open(
            path, limits, sources,
        )?)),
        Storage::Memory(files) => {
            let _ = sources;
            Ok(OpenedInput::Memory(memory::MemoryInput::open(
                files, path, limits,
            )?))
        }
    }
}
/// Read one complete primary file (no sidecars) through the active storage.
#[cfg(feature = "opentype")]
pub(crate) fn read_primary(path: &Path, limits: &ResourceLimits) -> Result<Vec<u8>> {
    match storage()? {
        #[cfg(feature = "fs")]
        Storage::Filesystem => crate::assets::read_primary(path, limits, sources()),
        Storage::Memory(files) => memory::read_primary(&files, path, limits),
    }
}
/// Whether an existing directory can receive `--each` outputs.
pub(crate) fn is_output_directory(path: &Path) -> Result<bool> {
    match storage()? {
        #[cfg(feature = "fs")]
        Storage::Filesystem => Ok(path.is_dir()),
        // Memory outputs are returned, so any destination acts as a directory
        // unless the caller supplied a file at that exact path.
        Storage::Memory(files) => Ok(!files.is_file(&memory::key(path))),
    }
}
/// Hand a complete output group to the active scope: batch capture defers it
/// to the shared transaction, filesystem scopes persist it transactionally,
/// memory scopes validate and return it.
pub(crate) fn persist_outputs(mut outputs: Vec<PreparedOutput>, sources: &Sources) -> Result<()> {
    let captured = CAPTURE.with(|cell| {
        let mut state = cell.borrow_mut();
        let Some(c) = state.as_mut() else {
            return Ok(Some(std::mem::take(&mut outputs)));
        };
        let overflow = || Error::ResourceLimit {
            resource: "batch pending output bytes",
            limit: c.limit,
        };
        let mut charge = outputs
            .capacity()
            .checked_mul(std::mem::size_of::<PreparedOutput>())
            .ok_or_else(overflow)?;
        for output in &outputs {
            let owned = output.bundle.retained_bytes(c.limit)?;
            charge = charge
                .checked_add(owned.saturating_sub(std::mem::size_of::<OutputBundle>()))
                .and_then(|n| n.checked_add(output.destination.capacity()))
                .ok_or_else(overflow)?;
        }
        let mut used = c
            .pending
            .lock()
            .map_err(|_| invalid("pending budget poisoned"))?;
        // The common path moves the complete Vec, including its exact capacity.
        // Multiple save calls need both input and receiving arrays while merging.
        let receiving_reserve = if c.outputs.is_empty() {
            0
        } else {
            c.outputs
                .len()
                .checked_add(outputs.len())
                .map(|n| n.max(c.outputs.capacity()))
                .and_then(|n| n.checked_mul(std::mem::size_of::<PreparedOutput>()))
                .ok_or_else(overflow)?
        };
        let next = used
            .checked_add(charge)
            .and_then(|n| n.checked_add(receiving_reserve))
            .filter(|n| *n <= c.limit)
            .ok_or_else(overflow)?;
        if c.outputs.is_empty() {
            c.outputs = std::mem::take(&mut outputs);
        } else {
            c.outputs
                .try_reserve_exact(outputs.len())
                .map_err(|_| overflow())?;
            // Check actual capacity as well; allocator over-allocation cannot be
            // silently handed to the retained-output ledger.
            let receiving_bytes = c
                .outputs
                .capacity()
                .checked_mul(std::mem::size_of::<PreparedOutput>())
                .ok_or_else(overflow)?;
            if receiving_bytes > receiving_reserve {
                return Err(overflow());
            }
            c.outputs.append(&mut outputs);
        }
        *used = next;
        c.charged = c
            .charged
            .checked_add(charge)
            .and_then(|n| n.checked_add(receiving_reserve))
            .ok_or_else(overflow)?;
        Ok(None)
    })?;
    let Some(outputs) = captured else {
        return Ok(());
    };
    match storage()? {
        #[cfg(feature = "fs")]
        Storage::Filesystem => crate::assets::persist_bundles(&outputs, sources),
        Storage::Memory(_) => {
            let _ = sources;
            memory::check_group(&outputs)?;
            SOURCE_SCOPE.with(|s| {
                let mut slot = s.borrow_mut();
                let scope = slot
                    .as_mut()
                    .ok_or_else(|| invalid("memory outputs require an active scope"))?;
                scope.outputs.extend(outputs);
                Ok(())
            })
        }
    }
}
pub(crate) struct CaptureGuard;
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with(|cell| {
            if let Some(c) = cell.borrow_mut().take()
                && let Ok(mut used) = c.pending.lock()
            {
                *used = used.saturating_sub(c.charged);
            }
        });
    }
}

#[cfg(all(test, feature = "fs"))]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
    };
    struct SpoolRoot(PathBuf);
    impl SpoolRoot {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../local-work/w6.1-spool-tests")
                .join(format!(
                    "scope-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for SpoolRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn with_scope_fs<T>(
        total: usize,
        f: impl FnOnce() -> fontpixel::Result<T>,
    ) -> fontpixel::Result<T> {
        super::with_bounded_source_scope(&super::Storage::Filesystem, total, f)
            .map(|(value, _)| value)
    }
    #[test]
    fn standalone_partition_has_bounded_identity_reservation_without_raising_total() {
        for (total, identity) in [
            (0, 0),
            (255, 0),
            (256, 1),
            (256 * 1024 * 1024 - 1, 1024 * 1024 - 1),
            (256 * 1024 * 1024, 1024 * 1024),
            (512 * 1024 * 1024, 1024 * 1024),
            (usize::MAX, 1024 * 1024),
        ] {
            assert_eq!(super::source_identity_reservation(total), identity);
            assert_eq!((total - identity).checked_add(identity), Some(total));
        }
        let mut ran = false;
        let result = with_scope_fs(255, || {
            ran = true;
            Ok(())
        });
        assert!(matches!(
            result,
            Err(fontpixel::Error::ResourceLimit { .. })
        ));
        assert!(!ran);
        assert!(super::SOURCE_SCOPE.with(|s| s.borrow().is_none()));
    }
    #[test]
    fn standalone_working_limits_reserve_once_and_restore_nested_error_and_panic() {
        let original = fontpixel::ResourceLimits::default();
        let mut expected = original.clone();
        expected.max_total_bytes -= 1024 * 1024;
        with_scope_fs(original.max_total_bytes, || {
            for _ in 0..3 {
                assert_eq!(super::standalone_working_limits(original.clone()), expected);
            }
            let nested = with_scope_fs(1024 * 1024, || {
                assert_eq!(
                    super::standalone_working_limits(original.clone()).max_total_bytes,
                    original.max_total_bytes - 4096,
                );
                Err::<(), _>(super::invalid("nested failure"))
            });
            assert!(nested.is_err());
            assert_eq!(super::standalone_working_limits(original.clone()), expected);
            assert!(
                std::panic::catch_unwind(|| {
                    let _ = with_scope_fs(2 * 1024 * 1024, || -> fontpixel::Result<()> {
                        panic!("scope restoration probe");
                    });
                })
                .is_err()
            );
            assert_eq!(super::standalone_working_limits(original.clone()), expected);
            let child = std::thread::spawn(|| {
                let limits = fontpixel::ResourceLimits::default();
                assert_eq!(super::standalone_working_limits(limits.clone()), limits);
            });
            child.join().unwrap();
            Ok(())
        })
        .unwrap();
        assert_eq!(super::standalone_working_limits(original.clone()), original);
        assert!(super::SOURCE_SCOPE.with(|s| s.borrow().is_none()));
    }
    #[test]
    fn standalone_identity_exhaustion_stays_sticky_after_inputs_are_dropped() {
        let root = SpoolRoot::new();
        let destination = root.0.join("output");
        fs::write(&destination, b"previous output").unwrap();
        with_scope_fs(1024 * 1024, || {
            let limits = super::standalone_working_limits(fontpixel::ResourceLimits {
                max_total_bytes: 1024 * 1024,
                ..Default::default()
            });
            let mut denied = false;
            for index in 0..1024 {
                let path = root.0.join(format!("source-{index}"));
                fs::write(&path, b"input").unwrap();
                match crate::assets::FileInput::open(&path, &limits, super::sources()) {
                    Ok(file) => drop(file),
                    Err(fontpixel::Error::ResourceLimit { resource, limit }) => {
                        assert_eq!(resource, "retained input identity bytes");
                        assert_eq!(limit, 4096);
                        denied = true;
                        break;
                    }
                    Err(error) => panic!("unexpected source error: {error}"),
                }
            }
            assert!(denied, "identity table did not enforce its reserved slice");
            let sources = super::sources();
            assert!(!sources.is_single_primary());
            let result = super::persist_outputs(
                vec![super::PreparedOutput {
                    destination: destination.clone(),
                    bundle: fontpixel::OutputBundle::single(
                        fontpixel::ResourcePath::new("output")?,
                        b"replacement".to_vec(),
                    ),
                }],
                &sources,
            );
            assert!(result.is_err());
            assert_eq!(fs::read(&destination).unwrap(), b"previous output");
            assert!(fs::read_dir(&root.0).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".fontpixel-")
            }));
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn standalone_repeated_reads_share_identity_charge_and_keep_protection() {
        let root = SpoolRoot::new();
        let source = root.0.join("source");
        fs::write(&source, b"original source").unwrap();
        with_scope_fs(1024 * 1024, || {
            let limits = super::standalone_working_limits(fontpixel::ResourceLimits {
                max_total_bytes: 1024 * 1024,
                ..Default::default()
            });
            for _ in 0..1024 {
                drop(crate::assets::FileInput::open(
                    &source,
                    &limits,
                    super::sources(),
                )?);
            }
            let sources = super::sources();
            assert!(!sources.is_single_primary());
            let result = super::persist_outputs(
                vec![super::PreparedOutput {
                    destination: source.clone(),
                    bundle: fontpixel::OutputBundle::single(
                        fontpixel::ResourcePath::new("source")?,
                        b"replacement".to_vec(),
                    ),
                }],
                &sources,
            );
            assert!(result.is_err());
            assert_eq!(fs::read(&source).unwrap(), b"original source");
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn batch_capture_keeps_its_pending_ledger_and_full_task_working_allowance() {
        let limits = fontpixel::ResourceLimits::default();
        with_scope_fs(limits.max_total_bytes, || {
            let pending = Arc::new(Mutex::new(0));
            let sources = crate::assets::Sources::default();
            sources.set_retained_budget(Arc::clone(&pending), 512 * 1024 * 1024)?;
            super::CAPTURE.with(|cell| {
                *cell.borrow_mut() = Some(super::Capture {
                    storage: super::Storage::Filesystem,
                    sources,
                    outputs: Vec::new(),
                    pending: Arc::clone(&pending),
                    limit: 512 * 1024 * 1024,
                    charged: 0,
                });
            });
            let guard = super::CaptureGuard;
            assert_eq!(super::standalone_working_limits(limits.clone()), limits);
            let mut ran = false;
            assert!(
                with_scope_fs(limits.max_total_bytes, || {
                    ran = true;
                    Ok(())
                })
                .is_err()
            );
            assert!(!ran);
            assert!(*pending.lock().unwrap() > 0);
            drop(guard);
            assert_eq!(*pending.lock().unwrap(), 0);
            assert_eq!(
                super::standalone_working_limits(limits.clone()).max_total_bytes,
                limits.max_total_bytes - 1024 * 1024,
            );
            Ok(())
        })
        .unwrap();
    }
}
