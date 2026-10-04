//! CLI-owned authority and persistence; the library never opens filesystem paths.
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use fontpixel::resources::{OutputBundle, ResourceResolver};
use fontpixel::{Error, Input, ResourceLimits, ResourcePath, Result};
#[cfg(not(unix))]
use same_file::Handle;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::File,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(unix)]
#[derive(PartialEq, Eq, Hash)]
struct FileIdentity(u64, u64);
#[cfg(not(unix))]
#[derive(PartialEq, Eq, Hash)]
struct FileIdentity(Handle);
impl FileIdentity {
    fn of(file: &File) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = file.metadata()?;
            Ok(Self(metadata.dev(), metadata.ino()))
        }
        #[cfg(not(unix))]
        {
            Ok(Self(Handle::from_file(file.try_clone()?)?))
        }
    }
}
// The source ledger uses explicit slots rather than assuming std::HashSet's
// private bucket/control-byte layout. Its budget is Rust-owned storage; allocator
// headers and operating-system memory for non-Unix handles are not measurable
// through these APIs and are excluded, as in the model/output ledgers.
#[derive(Default)]
struct IdentityTable {
    slots: Vec<Option<FileIdentity>>,
    len: usize,
    hash: std::collections::hash_map::RandomState,
}
impl IdentityTable {
    fn bytes_for(capacity: usize, limit: usize) -> Result<usize> {
        capacity.checked_mul(std::mem::size_of::<Option<FileIdentity>>())
            .ok_or_else(|| identity_limit(limit))
    }
    fn bytes(&self, limit: usize) -> Result<usize> {
        Self::bytes_for(self.slots.capacity(), limit)
    }
    fn index(&self, identity: &FileIdentity, capacity: usize) -> usize {
        use std::hash::BuildHasher;
        (self.hash.hash_one(identity) as usize) & (capacity - 1)
    }
    fn contains(&self, identity: &FileIdentity) -> bool {
        if self.slots.is_empty() {
            return false;
        }
        let mut index = self.index(identity, self.slots.len());
        loop {
            match &self.slots[index] {
                Some(found) if found == identity => return true,
                Some(_) => index = (index + 1) & (self.slots.len() - 1),
                None => return false,
            }
        }
    }
    fn next_capacity(&self, limit: usize) -> Result<Option<usize>> {
        let required = self.len.checked_add(1)
            .and_then(|len| len.checked_mul(2))
            .ok_or_else(|| identity_limit(limit))?;
        if required <= self.slots.len() {
            return Ok(None);
        }
        required.checked_next_power_of_two().map(Some)
            .ok_or_else(|| identity_limit(limit))
    }
    fn allocate(capacity: usize, limit: usize) -> Result<Vec<Option<FileIdentity>>> {
        let mut slots = Vec::new();
        slots.try_reserve_exact(capacity).map_err(|_| identity_limit(limit))?;
        // Vec promises at least the requested capacity, not an exact upper
        // bound. Never retain an unexpectedly enlarged allocation uncharged.
        if slots.capacity() != capacity {
            return Err(identity_limit(limit));
        }
        slots.resize_with(capacity, || None);
        Ok(slots)
    }
    fn insert_into(&self, slots: &mut [Option<FileIdentity>], identity: FileIdentity) {
        let mut index = self.index(&identity, slots.len());
        while slots[index].is_some() {
            index = (index + 1) & (slots.len() - 1);
        }
        slots[index] = Some(identity);
    }
    fn replace_slots(&mut self, mut replacement: Vec<Option<FileIdentity>>) {
        let mut old = std::mem::take(&mut self.slots);
        for slot in &mut old {
            if let Some(identity) = slot.take() {
                // Move owning non-Unix handles; rehashing never clones them.
                self.insert_into(&mut replacement, identity);
            }
        }
        self.slots = replacement;
        drop(old);
    }
    fn insert_unique(&mut self, identity: FileIdentity) {
        // Admission maintains a nonempty power-of-two array at <= 1/2 load.
        let mut index = self.index(&identity, self.slots.len());
        while self.slots[index].is_some() {
            index = (index + 1) & (self.slots.len() - 1);
        }
        self.slots[index] = Some(identity);
        self.len += 1;
    }
}
fn identity_limit(limit: usize) -> Error {
    Error::ResourceLimit { resource: "retained input identity bytes", limit }
}
#[derive(Clone)]
struct IdentityBudget {
    used: Arc<Mutex<usize>>,
    limit: usize,
}
// Lock order is always SourceState -> shared retained ledger. Output capture
// holds only the latter and must never call Sources while holding that lock.
// Holding the ledger through allocation makes old+new admission atomic with
// concurrent pending-output reservations and gives failure a rollback guard.
struct IdentityReservation<'a> {
    used: std::sync::MutexGuard<'a, usize>,
    previous: usize,
    retained: usize,
    committed: bool,
}
impl IdentityBudget {
    fn reserve(
        &self,
        additional: usize,
        released: usize,
        existing_charge: usize,
    ) -> Result<IdentityReservation<'_>> {
        let mut used = self.used.lock().map_err(|_| invalid("retained identity budget lock poisoned"))?;
        if *used < existing_charge || released > existing_charge {
            return Err(invalid("retained identity accounting is inconsistent"));
        }
        let previous = *used;
        let peak = previous.checked_add(additional)
            .filter(|bytes| *bytes <= self.limit)
            .ok_or_else(|| identity_limit(self.limit))?;
        let retained = peak.checked_sub(released).ok_or_else(|| identity_limit(self.limit))?;
        *used = peak;
        Ok(IdentityReservation { used, previous, retained, committed: false })
    }
}
impl IdentityReservation<'_> {
    fn commit(mut self) {
        *self.used = self.retained;
        self.committed = true;
    }
}
impl Drop for IdentityReservation<'_> {
    fn drop(&mut self) {
        if !self.committed {
            *self.used = self.previous;
        }
    }
}
#[derive(Default)]
struct SourceState {
    identities: IdentityTable,
    reads: usize,
    has_directory: bool,
    incomplete: bool,
    budget: Option<IdentityBudget>,
    charged: usize,
}
impl SourceState {
    fn retained_bytes(&self, limit: usize) -> Result<usize> {
        std::mem::size_of::<Self>().checked_add(self.identities.bytes(limit)?)
            .ok_or_else(|| identity_limit(limit))
    }
    fn ensure_complete(&self) -> Result<()> {
        if self.incomplete {
            return Err(invalid("input identity protection is incomplete; no outputs can be committed"));
        }
        if let Some(budget) = &self.budget {
            let used = budget.used.lock().map_err(|_| invalid("retained identity budget lock poisoned"))?;
            if *used < self.charged || *used > budget.limit {
                return Err(invalid("retained identity accounting is inconsistent"));
            }
        }
        Ok(())
    }
    fn remember_identity(&mut self, identity: FileIdentity, directory: bool) -> Result<()> {
        self.ensure_complete()?;
        if !self.identities.contains(&identity) {
            let limit = self.budget.as_ref().map_or(usize::MAX, |budget| budget.limit);
            if let Some(capacity) = self.identities.next_capacity(limit)? {
                let old_bytes = self.identities.bytes(limit)?;
                let new_bytes = IdentityTable::bytes_for(capacity, limit)?;
                let charged = if self.budget.is_some() {
                    self.charged.checked_add(new_bytes)
                        .and_then(|bytes| bytes.checked_sub(old_bytes))
                        .ok_or_else(|| identity_limit(limit))?
                } else {
                    0
                };
                let budget = self.budget.clone();
                let reservation = budget.as_ref()
                    .map(|budget| budget.reserve(new_bytes, old_bytes, self.charged))
                    .transpose()?;
                let slots = IdentityTable::allocate(capacity, limit)?;
                self.identities.replace_slots(slots);
                self.charged = charged;
                if let Some(reservation) = reservation {
                    reservation.commit();
                }
            }
            self.identities.insert_unique(identity);
        }
        self.reads = self.reads.saturating_add(1);
        self.has_directory |= directory;
        Ok(())
    }
}
impl Drop for SourceState {
    fn drop(&mut self) {
        // Release owning handles and the slots array before making its charge
        // available again. Cloned Sources share this one final-owner release.
        drop(std::mem::take(&mut self.identities.slots));
        if let Some(budget) = &self.budget
            && let Ok(mut used) = budget.used.lock()
            && let Some(remaining) = used.checked_sub(self.charged)
        {
            *used = remaining;
        }
        // A poisoned/inconsistent ledger is not reset to zero: leaving a charge
        // unreleased fails conservatively rather than admitting excess work.
    }
}
#[derive(Clone, Default)]
pub(crate) struct Sources(Arc<Mutex<SourceState>>);
impl Sources {
    pub fn is_single_primary(&self) -> bool {
        self.0.lock().is_ok_and(|state| {
            state.reads == 1 && !state.has_directory && !state.incomplete
        })
    }
    /// Attach the batch's retained ledger once, before declarations/workers.
    /// The initial TOML identity is charged here. Standalone scopes deliberately
    /// have no new implicit cap; callers may attach a command-derived ledger.
    pub(crate) fn set_retained_budget(&self, used: Arc<Mutex<usize>>, limit: usize) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| invalid("input identity lock poisoned"))?;
        state.ensure_complete()?;
        if let Some(existing) = &state.budget {
            if Arc::ptr_eq(&existing.used, &used) && existing.limit == limit {
                return Ok(());
            }
            return Err(invalid("input identity budget is already attached"));
        }
        let result = (|| {
            let bytes = state.retained_bytes(limit)?;
            let budget = IdentityBudget { used, limit };
            let reservation = budget.reserve(bytes, 0, 0)?;
            state.budget = Some(budget.clone());
            state.charged = bytes;
            reservation.commit();
            Ok(())
        })();
        if result.is_err() {
            state.incomplete = true;
        }
        result
    }
    pub(crate) fn remember(&self, file: &File) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| invalid("input identity lock poisoned"))?;
        let result = (|| {
            let identity = FileIdentity::of(file)?;
            let directory = file.metadata()?.is_dir();
            state.remember_identity(identity, directory)
        })();
        if result.is_err() {
            // A failed sidecar task must not leave other tasks free to overwrite
            // that untracked source. The whole transaction now fails closed.
            state.incomplete = true;
        }
        result
    }
    fn ensure_complete(&self) -> Result<()> {
        self.0.lock().map_err(|_| invalid("input identity lock poisoned"))?
            .ensure_complete()
    }
    fn contains(&self, file: File) -> Result<bool> {
        let identity = FileIdentity::of(&file)?;
        let state = self.0.lock().map_err(|_| invalid("input identity lock poisoned"))?;
        state.ensure_complete()?;
        Ok(state.identities.contains(&identity))
    }
}

fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
fn bounded(mut file: File, maximum: usize) -> Result<Vec<u8>> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("font input must be a regular file"));
    }
    if metadata.len() > maximum as u64 {
        return Err(Error::ResourceLimit {
            resource: "input asset bytes",
            limit: maximum,
        });
    }
    let mut bytes = Vec::with_capacity((metadata.len() as usize).min(maximum));
    (&mut file)
        .take((maximum as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(Error::ResourceLimit {
            resource: "input asset bytes",
            limit: maximum,
        });
    }
    Ok(bytes)
}
pub(crate) struct FileInput {
    bytes: Vec<u8>,
    name: String,
    root: Dir,
    sources: Sources,
    directory: bool,
}
impl FileInput {
    pub fn open(path: &Path, limits: &ResourceLimits, sources: Sources) -> Result<Self> {
        let file = File::open(path)?;
        let directory = file.metadata()?.is_dir();
        sources.remember(&file)?;
        let (root, bytes) = if directory {
            (Dir::from_std_file(file), Vec::new())
        } else {
            (
                Dir::open_ambient_dir(parent(path), ambient_authority())?,
                bounded(file, limits.max_input_bytes.min(limits.max_total_bytes))?,
            )
        };
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("input must have a UTF-8 filename"))?
            .to_owned();
        Ok(Self {
            bytes,
            name,
            root,
            sources,
            directory,
        })
    }
    // Retain directory identities too: a source directory may be renamed after
    // opening, and lexical input-path checks alone cannot protect its new name.
    fn remember_directories(&self, path: &Path) -> Result<()> {
        if self.directory {
            let mut prefix = PathBuf::new();
            for component in path.components() {
                if component == Component::CurDir {
                    continue;
                }
                prefix.push(component);
                let directory = self.root.open_dir(&prefix)?;
                self.sources.remember(&directory.into_std_file())?;
            }
        }
        Ok(())
    }
    pub fn input(&self) -> Input<'_> {
        if self.directory {
            return Input::directory(&self.name, self);
        }
        Input::new(&self.bytes)
            .with_name(&self.name)
            .with_resources(self)
    }
}
impl ResourceResolver for FileInput {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        let file = match self.root.open(path.as_str()) {
            Ok(f) => f.into_std(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(Error::MissingResource(path.to_string()));
            }
            Err(e) => return Err(e.into()),
        };
        self.remember_directories(parent(Path::new(path.as_str())))?;
        self.sources.remember(&file)?;
        bounded(file, max_bytes)
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        if let Some(directory) = directory {
            self.remember_directories(Path::new(directory.as_str()))?;
        }
        let nested = directory
            .map(|path| self.root.open_dir(path.as_str()))
            .transpose()?;
        let root = nested.as_ref().unwrap_or(&self.root);
        let mut paths = Vec::new();
        let mut used = 0usize;
        for (index, entry) in root.entries()?.enumerate() {
            if index >= max_entries {
                return Err(Error::ResourceLimit {
                    resource: "directory entries",
                    limit: max_entries,
                });
            }
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("resource filenames must be UTF-8"))?;
            let prefix = directory.map_or(0, |path| path.as_str().len().saturating_add(1));
            used = used.saturating_add(
                std::mem::size_of::<ResourcePath>()
                    .saturating_add(prefix)
                    .saturating_add(name.len())
                    .saturating_mul(2),
            );
            if used > max_bytes {
                return Err(Error::ResourceLimit {
                    resource: "directory listing bytes",
                    limit: max_bytes,
                });
            }
            let name = match directory {
                Some(path) => format!("{path}/{name}"),
                None => name,
            };
            paths.push(ResourcePath::new(name)?);
        }
        Ok(paths)
    }
}
static NEXT: AtomicU64 = AtomicU64::new(0);
fn temporary(dir: &Dir, suffix: &str) -> Result<(String, cap_std::fs::File)> {
    for _ in 0..100 {
        let name = format!(
            ".fontpixel-{}-{}.{suffix}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        match dir.open_with(&name, OpenOptions::new().write(true).create_new(true)) {
            Ok(f) => return Ok((name, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(invalid("cannot allocate unique output staging file"))
}
/// One prepared output. Nothing is published until the complete group passes
/// preflight and every asset has been written to a private staging file.
pub(crate) struct PendingOutput {
    pub destination: PathBuf,
    pub bundle: OutputBundle,
}

struct Stage {
    dir: Arc<Dir>,
    directory: PathBuf,
    directory_identity: FileIdentity,
    target: String,
    original: Option<FileIdentity>,
    is_primary: bool,
    temp: String,
    temp_identity: FileIdentity,
    backup: Option<String>,
    installed: bool,
}
struct CreatedDirectory {
    parent: Arc<Dir>,
    name: std::ffi::OsString,
    identity: FileIdentity,
}
struct Transaction {
    stages: Vec<Stage>,
    created: Vec<CreatedDirectory>,
    committed: bool,
}
fn directory_identity(dir: &Dir) -> Result<FileIdentity> {
    FileIdentity::of(&dir.open_dir(".")?.into_std_file())
}
fn entry_identity(dir: &Dir, name: &Path) -> Result<Option<FileIdentity>> {
    match dir.symlink_metadata(name) {
        Ok(metadata) => {
            if metadata.is_symlink() || !metadata.is_file() {
                return Err(invalid("output target must be a regular file, not a symlink"));
            }
            let file = dir.open(name)?.into_std();
            if !file.metadata()?.is_file() {
                return Err(invalid("output target is not a regular file"));
            }
            Ok(Some(FileIdentity::of(&file)?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn remove_owned_file(dir: &Dir, name: &str, identity: &FileIdentity) {
    if entry_identity(dir, Path::new(name))
        .is_ok_and(|found| found.as_ref() == Some(identity))
    {
        let _ = dir.remove_file(name);
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        for stage in self.stages.iter_mut().rev() {
            if !self.committed {
                if stage.installed {
                    remove_owned_file(&stage.dir, &stage.target, &stage.temp_identity);
                }
                if let Some(backup) = &stage.backup {
                    // Never overwrite a concurrent replacement during rollback.
                    // Keep the backup on disk if recovery itself fails.
                    if matches!(
                        stage.dir.symlink_metadata(&stage.target),
                        Err(error) if error.kind() == io::ErrorKind::NotFound
                    ) && entry_identity(&stage.dir, Path::new(backup))
                        .is_ok_and(|found| found == stage.original)
                        && stage.dir.rename(backup, &stage.dir, &stage.target).is_ok()
                    {
                        stage.backup = None;
                    }
                }
            }
            remove_owned_file(&stage.dir, &stage.temp, &stage.temp_identity);
            if self.committed
                && let (Some(backup), Some(identity)) = (&stage.backup, &stage.original)
            {
                remove_owned_file(&stage.dir, backup, identity);
            }
        }
        if !self.committed {
            for created in self.created.iter().rev() {
                // The pathname might have been replaced since creation. Only
                // remove the exact empty directory made by this transaction.
                let matches = created.parent.symlink_metadata(&created.name)
                    .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
                    && created.parent.open_dir(&created.name)
                        .ok()
                        .and_then(|dir| directory_identity(&dir).ok())
                        .is_some_and(|identity| identity == created.identity);
                if matches {
                    let _ = created.parent.remove_dir(&created.name);
                }
            }
        }
    }
}

fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    })
}
fn normalized(path: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for component in absolute(path)?.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    Ok(result)
}
// Check the unnormalized spelling first. Collapsing `link/..` before checking
// would otherwise hide a symlink in the user-supplied output path.
fn checked_directory(path: &Path, sources: &Sources) -> Result<Dir> {
    let mut prefix = PathBuf::new();
    let mut last = None;
    for component in absolute(path)?.components() {
        prefix.push(component.as_os_str());
        // A Windows prefix (for example C:) is not a complete root by itself.
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&prefix)?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(invalid(format!(
                "output parent must be a real directory, not a symlink: {}",
                prefix.display()
            )));
        }
        let dir = Dir::open_ambient_dir(&prefix, ambient_authority())?;
        if sources.contains(dir.open_dir(".")?.into_std_file())? {
            return Err(invalid("output overlaps a primary input or sidecar directory"));
        }
        last = Some(dir);
    }
    last.ok_or_else(|| invalid("output has no directory root"))
}

struct PreparedBundle<'a> {
    root: Arc<Dir>,
    root_path: PathBuf,
    root_identity: FileIdentity,
    bundle: &'a OutputBundle,
}
struct Target<'a> {
    bundle_index: usize,
    path: &'a Path,
    original: Option<FileIdentity>,
    bytes: &'a [u8],
    is_primary: bool,
}
fn insert_file_target(paths: &mut BTreeSet<PathBuf>, path: PathBuf) -> Result<()> {
    if path.ancestors().any(|ancestor| paths.contains(ancestor))
        || paths.range(path.clone()..).next().is_some_and(|next| next.starts_with(&path))
    {
        return Err(invalid("output paths are duplicates or overlap an ancestor target"));
    }
    paths.insert(path);
    Ok(())
}
fn preflight(
    root: &Dir,
    path: &Path,
    sources: &Sources,
    outputs: &mut HashSet<FileIdentity>,
) -> Result<Option<FileIdentity>> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        match root.symlink_metadata(&prefix) {
            Ok(metadata) => {
                if metadata.is_symlink() {
                    return Err(invalid(format!(
                        "output symlink is not allowed: {}",
                        prefix.display()
                    )));
                }
                if prefix != path {
                    if !metadata.is_dir() {
                        return Err(invalid("output parent is not a directory"));
                    }
                    if sources.contains(root.open_dir(&prefix)?.into_std_file())? {
                        return Err(invalid("output overlaps a primary input or sidecar directory"));
                    }
                } else {
                    if !metadata.is_file() {
                        return Err(invalid("output target is not a regular file"));
                    }
                    let file = root.open(path)?.into_std();
                    if sources.contains(file.try_clone()?)? {
                        return Err(invalid("output aliases a primary input or sidecar"));
                    }
                    if !outputs.insert(FileIdentity::of(&file)?) {
                        return Err(invalid("output assets alias each other"));
                    }
                    return Ok(Some(FileIdentity::of(&file)?));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

// A directory is a complete font, so unrelated old members cannot be merged
// into it. During the post-staging check only our exact staging files are
// exempted; their identities are checked separately before publication.
fn preflight_directory(
    prepared: &PreparedBundle<'_>,
    staging: &BTreeSet<PathBuf>,
) -> Result<()> {
    let bundle = prepared.bundle;
    if !bundle.is_directory() {
        return Ok(());
    }
    let base = Path::new(bundle.primary().as_str());
    let mut expected = BTreeMap::<PathBuf, HashSet<std::ffi::OsString>>::new();
    for asset in bundle.assets() {
        let mut path = Path::new(asset.path.as_str());
        while path != base {
            let name = path.file_name().ok_or_else(|| invalid("invalid directory asset"))?;
            let directory = path.parent().ok_or_else(|| invalid("invalid directory asset parent"))?;
            expected.entry(directory.to_owned()).or_default().insert(name.to_owned());
            path = directory;
        }
    }
    for (path, names) in expected {
        let metadata = match prepared.root.symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(invalid("directory output target must be a real directory"));
        }
        for entry in prepared.root.read_dir(&path)? {
            let entry = entry?;
            if !names.contains(&entry.file_name())
                && !staging.contains(&prepared.root_path.join(&path).join(entry.file_name()))
            {
                return Err(invalid(
                    "output font directory contains extra members; use a new or empty directory",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn persist_bundle(
    destination: &Path,
    bundle: &OutputBundle,
    sources: &Sources,
) -> Result<()> {
    persist_impl(destination, bundle, sources, None)
}
pub(crate) fn persist_bundles(outputs: &[PendingOutput], sources: &Sources) -> Result<()> {
    persist_group(
        outputs.iter().map(|output| (output.destination.as_path(), &output.bundle)),
        sources,
        |_| Ok(()),
    )
}
fn persist_impl(
    destination: &Path,
    bundle: &OutputBundle,
    sources: &Sources,
    fail_after: Option<usize>,
) -> Result<()> {
    persist_group(std::iter::once((destination, bundle)), sources, |index| {
        if fail_after == Some(index) {
            Err(invalid("injected commit failure"))
        } else {
            Ok(())
        }
    })
}

// This is an error-rollback transaction, not crash-atomic multi-file visibility.
// Every bundle is validated and staged before *any* bundle is published. All
// roots share the same rollback guard and supporting assets precede primaries.
fn persist_group<'a>(
    outputs: impl IntoIterator<Item = (&'a Path, &'a OutputBundle)>,
    sources: &Sources,
    mut before_commit: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    sources.ensure_complete()?;
    let mut prepared = Vec::new();
    let mut destinations = BTreeSet::new();
    for (destination, bundle) in outputs {
        if destination.file_name().and_then(|name| name.to_str()) != Some(bundle.primary().as_str()) {
            return Err(invalid("output bundle entry point differs from destination"));
        }
        let root = Arc::new(checked_directory(parent(destination), sources)?);
        let root_path = normalized(parent(destination))?;
        insert_file_target(&mut destinations, root_path.join(bundle.primary().as_str()))?;
        prepared.push(PreparedBundle {
            root_identity: directory_identity(&root)?,
            root,
            root_path,
            bundle,
        });
    }
    let mut output_paths = BTreeSet::new();
    let mut output_identities = HashSet::new();
    let mut directory_claims = BTreeMap::new();
    for (index, output) in prepared.iter().enumerate() {
        if output.bundle.is_directory() {
            directory_claims.insert(output.root_path.join(output.bundle.primary().as_str()), index);
        }
    }
    let mut targets = Vec::new();
    for (index, output) in prepared.iter().enumerate() {
        preflight_directory(output, &BTreeSet::new())?;
        for asset in output.bundle.assets() {
            let path = Path::new(asset.path.as_str());
            let absolute = output.root_path.join(path);
            insert_file_target(&mut output_paths, absolute.clone())?;
            if absolute.ancestors().any(|ancestor| {
                directory_claims.get(ancestor).is_some_and(|owner| *owner != index)
            }) || directory_claims.range(absolute.clone()..).next().is_some_and(|(directory, owner)| {
                *owner != index && directory.starts_with(&absolute)
            }) {
                return Err(invalid("output overlaps another output font directory"));
            }
            targets.push(Target {
                bundle_index: index,
                path,
                original: preflight(&output.root, path, sources, &mut output_identities)?,
                bytes: &asset.bytes,
                is_primary: !output.bundle.is_directory() && &asset.path == output.bundle.primary(),
            });
        }
    }

    let mut tx = Transaction {
        stages: Vec::new(),
        created: Vec::new(),
        committed: false,
    };
    let mut directories = BTreeMap::<PathBuf, Arc<Dir>>::new();
    let mut staging = BTreeSet::new();
    for target in targets {
        let output = &prepared[target.bundle_index];
        // Reopening the ambient name also checks source-directory provenance.
        let current = checked_directory(&output.root_path, sources)?;
        if directory_identity(&current)? != output.root_identity {
            return Err(invalid("output root changed during preparation"));
        }
        let mut dir = Arc::clone(&output.root);
        let mut directory = output.root_path.clone();
        for component in parent(target.path).components() {
            if component == Component::CurDir {
                continue;
            }
            directory.push(component.as_os_str());
            if let Some(existing) = directories.get(&directory) {
                dir = Arc::clone(existing);
                continue;
            }
            let name = component.as_os_str();
            let created = match dir.create_dir(name) {
                Ok(()) => true,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
                Err(error) => return Err(error.into()),
            };
            let opened = (|| -> Result<Dir> {
                let metadata = dir.symlink_metadata(name)?;
                if metadata.is_symlink() || !metadata.is_dir() {
                    return Err(invalid("output parent must be a real directory"));
                }
                let opened = dir.open_dir(name)?;
                if sources.contains(opened.open_dir(".")?.into_std_file())? {
                    return Err(invalid("output overlaps a primary input or sidecar directory"));
                }
                Ok(opened)
            })();
            let opened = match opened {
                Ok(opened) => Arc::new(opened),
                // If opening failed we have no identity proof. Leaving an
                // empty new directory is safer than removing a replacement.
                Err(error) => return Err(error),
            };
            if created {
                tx.created.push(CreatedDirectory {
                    parent: Arc::clone(&dir),
                    name: name.to_owned(),
                    identity: directory_identity(&opened)?,
                });
            }
            directories.insert(directory.clone(), Arc::clone(&opened));
            dir = opened;
        }
        let current = checked_directory(&directory, sources)?;
        let directory_identity = directory_identity(&dir)?;
        if self::directory_identity(&current)? != directory_identity {
            return Err(invalid("output parent changed during preparation"));
        }
        let name = target.path.file_name().and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid output filename"))?.to_owned();
        if entry_identity(&dir, Path::new(&name))? != target.original {
            return Err(invalid("output target changed during preparation"));
        }
        let (temp, mut file) = temporary(&dir, "stage")?;
        let temp_identity = match file.try_clone().map_err(Error::from)
            .and_then(|file| FileIdentity::of(&file.into_std()))
        {
            Ok(identity) => identity,
            Err(error) => {
                drop(file);
                let _ = dir.remove_file(&temp);
                return Err(error);
            }
        };
        staging.insert(directory.join(&temp));
        tx.stages.push(Stage {
            dir,
            directory,
            directory_identity,
            target: name,
            original: target.original,
            is_primary: target.is_primary,
            temp,
            temp_identity,
            backup: None,
            installed: false,
        });
        file.write_all(target.bytes)?;
        file.sync_all()?;
    }
    sources.ensure_complete()?;
    // Staging names are the only additional directory members we accept.
    for output in &prepared {
        preflight_directory(output, &staging)?;
    }
    tx.stages.sort_by_key(|stage| stage.is_primary);
    for (index, stage) in tx.stages.iter_mut().enumerate() {
        before_commit(index)?;
        sources.ensure_complete()?;
        let current = checked_directory(&stage.directory, sources)?;
        if directory_identity(&current)? != stage.directory_identity {
            return Err(invalid("output parent changed before replacement"));
        }
        if entry_identity(&stage.dir, Path::new(&stage.target))? != stage.original {
            return Err(invalid("output target changed before replacement"));
        }
        preflight(&stage.dir, Path::new(&stage.target), sources, &mut HashSet::new())?;
        if entry_identity(&stage.dir, Path::new(&stage.temp))?.as_ref() != Some(&stage.temp_identity) {
            return Err(invalid("output staging file changed before replacement"));
        }
        if stage.original.is_some() {
            let (backup, file) = temporary(&stage.dir, "backup")?;
            let reserved_identity = match FileIdentity::of(&file.into_std()) {
                Ok(identity) => identity,
                Err(error) => {
                    let _ = stage.dir.remove_file(&backup);
                    return Err(error);
                }
            };
            if let Err(error) = stage.dir.rename(&stage.target, &stage.dir, &backup) {
                remove_owned_file(&stage.dir, &backup, &reserved_identity);
                return Err(error.into());
            }
            staging.insert(stage.directory.join(&backup));
            stage.backup = Some(backup);
            let found = match &stage.backup {
                Some(backup) => entry_identity(&stage.dir, Path::new(backup))?,
                None => return Err(invalid("output backup was not recorded")),
            };
            if found != stage.original {
                // It is still the entry we just moved, so rollback may safely
                // restore it if the destination is absent.
                stage.original = found;
                return Err(invalid("output target changed during replacement"));
            }
        }
        stage.dir.rename(&stage.temp, &stage.dir, &stage.target)?;
        stage.installed = true;
    }
    // Check the complete directory trees again before accepting the group. A
    // late extra member or parent swap causes the same cross-root rollback.
    for output in &prepared {
        preflight_directory(output, &staging)?;
    }
    for stage in &tx.stages {
        let current = checked_directory(&stage.directory, sources)?;
        if directory_identity(&current)? != stage.directory_identity
            || entry_identity(&stage.dir, Path::new(&stage.target))?.as_ref()
                != Some(&stage.temp_identity)
        {
            return Err(invalid("output changed before the group completed"));
        }
    }
    sources.ensure_complete()?;
    tx.committed = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontpixel::resources::OutputAsset;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "fontpixel-assets-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn bundle() -> OutputBundle {
        OutputBundle::new(
            ResourcePath::new("font.json").unwrap(),
            ["font.json", "font.png", "nested/extra.bin"]
                .into_iter()
                .map(|s| OutputAsset {
                    path: ResourcePath::new(s).unwrap(),
                    bytes: format!("new {s}").into_bytes(),
                })
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn commit_failure_restores_all_originals_and_removes_new_directories() {
        for fail in 0..3 {
            let t = Temp::new();
            for name in ["font.json", "font.png"] {
                std::fs::write(t.0.join(name), format!("old {name}")).unwrap();
            }
            assert!(
                persist_impl(
                    &t.0.join("font.json"),
                    &bundle(),
                    &Sources::default(),
                    Some(fail)
                )
                .is_err()
            );
            for name in ["font.json", "font.png"] {
                assert_eq!(
                    std::fs::read(t.0.join(name)).unwrap(),
                    format!("old {name}").as_bytes()
                );
            }
            assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 2);
        }
    }
    #[test]
    fn complete_bundle_replaces_existing_and_installs_nested_assets() {
        let t = Temp::new();
        std::fs::write(t.0.join("font.png"), "old").unwrap();
        persist_bundle(&t.0.join("font.json"), &bundle(), &Sources::default()).unwrap();
        for a in bundle().assets() {
            assert_eq!(std::fs::read(t.0.join(a.path.as_str())).unwrap(), a.bytes);
        }
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 3);
    }

    #[test]
    fn directory_bundle_transaction_and_bounded_input_enumeration() {
        let t = Temp::new();
        let target = t.0.join("font.imageset");
        let bundle = OutputBundle::directory(
            ResourcePath::new("font.imageset").unwrap(),
            ["b.png", "a.png"]
                .into_iter()
                .map(|name| OutputAsset {
                    path: ResourcePath::new(format!("font.imageset/{name}")).unwrap(),
                    bytes: name.as_bytes().to_vec(),
                })
                .collect(),
        )
        .unwrap();
        assert!(persist_impl(&target, &bundle, &Sources::default(), Some(1)).is_err());
        assert!(!target.exists());
        persist_bundle(&target, &bundle, &Sources::default()).unwrap();
        let limits = ResourceLimits::default();
        let file = FileInput::open(&target, &limits, Sources::default()).unwrap();
        let input = file.input();
        let mut context = fontpixel::resources::ReadContext::new(&input, &limits).unwrap();
        assert!(context.is_directory());
        let paths = context.list_resources_limited(None, 2, 1000).unwrap();
        assert_eq!(
            paths.iter().map(ResourcePath::as_str).collect::<Vec<_>>(),
            ["a.png", "b.png"]
        );
        assert_eq!(context.resource(&paths[0]).unwrap(), b"a.png");
        assert!(file.list(None, 1, 1000).is_err());
        assert!(file.list(None, 2, 1).is_err());
        std::fs::create_dir(target.join("nested")).unwrap();
        std::fs::write(target.join("nested/c.png"), "nested").unwrap();
        assert_eq!(file.list(None, 3, 1000).unwrap().len(), 2);
        assert_eq!(
            file.list(Some(&ResourcePath::new("nested").unwrap()), 1, 1000)
                .unwrap(),
            [ResourcePath::new("nested/c.png").unwrap()]
        );
        assert!(persist_bundle(&target, &bundle, &file.sources).is_err());
    }

    #[test]
    fn directory_output_rejects_stale_members_before_modifying_files() {
        let t = Temp::new();
        let target = t.0.join("font");
        std::fs::create_dir_all(target.join("nested")).unwrap();
        std::fs::write(target.join("65.png"), b"old A").unwrap();
        std::fs::write(target.join("nested/66.png"), b"old B").unwrap();
        let bundle = OutputBundle::directory(
            ResourcePath::new("font").unwrap(),
            ["font/65.png", "font/nested/66.png"]
                .into_iter()
                .map(|p| OutputAsset {
                    path: ResourcePath::new(p).unwrap(),
                    bytes: b"new".to_vec(),
                })
                .collect(),
        )
        .unwrap();
        for stale in ["67.png", "nested/68.png", "notes.txt"] {
            let path = target.join(stale);
            std::fs::write(&path, b"keep").unwrap();
            let error = persist_bundle(&target, &bundle, &Sources::default()).unwrap_err();
            assert!(error.to_string().contains("extra members"));
            assert_eq!(std::fs::read(target.join("65.png")).unwrap(), b"old A");
            assert_eq!(
                std::fs::read(target.join("nested/66.png")).unwrap(),
                b"old B"
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"keep");
            std::fs::remove_file(path).unwrap();
        }
        persist_bundle(&target, &bundle, &Sources::default()).unwrap();
        assert_eq!(std::fs::read(target.join("65.png")).unwrap(), b"new");
        assert_eq!(std::fs::read(target.join("nested/66.png")).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn directory_enumeration_does_not_expose_external_symlinks() {
        let root = Temp::new();
        let outside = Temp::new();
        std::fs::write(outside.0.join("secret"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.0.join("secret"), root.0.join("escape")).unwrap();
        std::os::unix::fs::symlink(&outside.0, root.0.join("external")).unwrap();
        let file =
            FileInput::open(&root.0, &ResourceLimits::default(), Sources::default()).unwrap();
        assert!(file.list(None, 2, 1000).unwrap().is_empty());
        assert!(
            file.list(Some(&ResourcePath::new("external").unwrap()), 2, 1000)
                .is_err()
        );
        assert!(
            file.resolve(&ResourcePath::new("escape").unwrap(), 1000)
                .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn large_directory_inputs_do_not_retain_one_file_descriptor_per_glyph() {
        let t = Temp::new();
        let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
        let sources = Sources::default();
        for i in 0..2048 {
            let path = t.0.join(i.to_string());
            std::fs::write(&path, [i as u8]).unwrap();
            sources.remember(&File::open(path).unwrap()).unwrap();
        }
        let after = std::fs::read_dir("/proc/self/fd").unwrap().count();
        // Other unit tests may open a few files concurrently. A retained handle
        // per glyph would exceed this margin by more than an order of magnitude.
        assert!(after < before + 128, "before={before}, after={after}");
        assert!(!sources.is_single_primary());
        assert!(
            sources
                .contains(File::open(t.0.join("2047")).unwrap())
                .unwrap()
        );
    }
    #[test]
    fn hardlinked_inputs_sidecars_and_output_aliases_fail_before_changes() {
        let t = Temp::new();
        std::fs::write(t.0.join("source"), "source").unwrap();
        std::fs::hard_link(t.0.join("source"), t.0.join("font.png")).unwrap();
        let sources = Sources::default();
        sources
            .remember(&File::open(t.0.join("source")).unwrap())
            .unwrap();
        assert!(persist_bundle(&t.0.join("font.json"), &bundle(), &sources).is_err());
        assert!(!t.0.join("font.json").exists());
        assert_eq!(std::fs::read(t.0.join("source")).unwrap(), b"source");
        std::fs::hard_link(t.0.join("source"), t.0.join("font.json")).unwrap();
        assert!(persist_bundle(&t.0.join("font.json"), &bundle(), &Sources::default()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn sidecar_and_output_symlinks_cannot_escape_root() {
        use std::os::unix::fs::symlink;
        let t = Temp::new();
        let outside = Temp::new();
        std::fs::write(t.0.join("source"), "source").unwrap();
        std::fs::write(outside.0.join("secret"), "outside").unwrap();
        symlink(&outside.0, t.0.join("nested")).unwrap();
        let input = FileInput::open(
            &t.0.join("source"),
            &ResourceLimits::default(),
            Sources::default(),
        )
        .unwrap();
        assert!(
            input
                .resolve(&ResourcePath::new("nested/secret").unwrap(), 1024)
                .is_err()
        );
        assert!(persist_bundle(&t.0.join("font.json"), &bundle(), &Sources::default()).is_err());
        assert_eq!(std::fs::read(outside.0.join("secret")).unwrap(), b"outside");
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 2);
    }
    fn pending(destination: PathBuf, bytes: &[u8]) -> PendingOutput {
        let name = destination.file_name().unwrap().to_str().unwrap();
        let bundle = OutputBundle::single(ResourcePath::new(name).unwrap(), bytes.to_vec());
        PendingOutput { destination, bundle }
    }
    fn run_group(
        outputs: &[PendingOutput],
        sources: &Sources,
        hook: impl FnMut(usize) -> Result<()>,
    ) -> Result<()> {
        persist_group(
            outputs.iter().map(|output| (output.destination.as_path(), &output.bundle)),
            sources,
            hook,
        )
    }
    fn assert_no_temporaries(path: &Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            assert!(!entry.file_name().to_string_lossy().starts_with(".fontpixel-"));
            if entry.file_type().unwrap().is_dir() {
                assert_no_temporaries(&entry.path());
            }
        }
    }
    #[test]
    fn empty_group_is_a_no_op() {
        persist_bundles(&[], &Sources::default()).unwrap();
    }
    #[test]
    fn group_failure_rolls_back_every_root_and_new_directory() {
        for fail_after in 0..6 {
            let a = Temp::new();
            let b = Temp::new();
            for root in [&a.0, &b.0] {
                for name in ["font.json", "font.png"] {
                    std::fs::write(root.join(name), format!("old {name}")).unwrap();
                }
            }
            let outputs = [
                PendingOutput { destination: a.0.join("font.json"), bundle: bundle() },
                PendingOutput { destination: b.0.join("font.json"), bundle: bundle() },
            ];
            assert!(run_group(&outputs, &Sources::default(), |index| {
                if index == fail_after { Err(invalid("injected group failure")) } else { Ok(()) }
            }).is_err());
            for root in [&a.0, &b.0] {
                for name in ["font.json", "font.png"] {
                    assert_eq!(std::fs::read(root.join(name)).unwrap(), format!("old {name}").as_bytes());
                }
                assert!(!root.join("nested").exists());
                assert_eq!(std::fs::read_dir(root).unwrap().count(), 2);
            }
        }
    }
    #[test]
    fn group_publishes_all_supporting_assets_before_any_primary() {
        let a = Temp::new();
        let b = Temp::new();
        for root in [&a.0, &b.0] {
            std::fs::write(root.join("font.json"), b"old primary").unwrap();
        }
        let outputs = [
            PendingOutput { destination: a.0.join("font.json"), bundle: bundle() },
            PendingOutput { destination: b.0.join("font.json"), bundle: bundle() },
        ];
        let mut checked = false;
        run_group(&outputs, &Sources::default(), |index| {
            if index == 4 {
                for root in [&a.0, &b.0] {
                    assert_eq!(std::fs::read(root.join("font.json")).unwrap(), b"old primary");
                    assert_eq!(std::fs::read(root.join("font.png")).unwrap(), b"new font.png");
                    assert_eq!(std::fs::read(root.join("nested/extra.bin")).unwrap(), b"new nested/extra.bin");
                }
                checked = true;
            }
            Ok(())
        }).unwrap();
        assert!(checked);
        for root in [&a.0, &b.0] {
            assert_eq!(std::fs::read(root.join("font.json")).unwrap(), b"new font.json");
            assert_no_temporaries(root);
        }
    }
    #[test]
    fn invalid_later_entry_point_prevents_all_staging() {
        let a = Temp::new();
        let b = Temp::new();
        std::fs::write(a.0.join("font.json"), b"old").unwrap();
        let outputs = [
            PendingOutput { destination: a.0.join("font.json"), bundle: bundle() },
            PendingOutput { destination: b.0.join("wrong.json"), bundle: bundle() },
        ];
        assert!(persist_bundles(&outputs, &Sources::default()).is_err());
        assert_eq!(std::fs::read(a.0.join("font.json")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(&a.0).unwrap().count(), 1);
        assert_eq!(std::fs::read_dir(&b.0).unwrap().count(), 0);
    }
    #[test]
    fn normalized_duplicate_destinations_are_rejected() {
        let t = Temp::new();
        std::fs::create_dir(t.0.join("inner")).unwrap();
        let outputs = [
            pending(t.0.join("font.bin"), b"first"),
            pending(t.0.join("inner/../font.bin"), b"second"),
        ];
        let error = persist_bundles(&outputs, &Sources::default()).unwrap_err();
        assert!(error.to_string().contains("duplicates"));
        assert!(!t.0.join("font.bin").exists());
        assert_no_temporaries(&t.0);
    }
    #[test]
    fn cross_bundle_sidecar_duplicate_and_ancestor_paths_are_rejected() {
        for second in ["shared", "shared/child"] {
            let t = Temp::new();
            let make = |name: &str, sidecar: &str| {
                OutputBundle::new(
                    ResourcePath::new(name).unwrap(),
                    [name, sidecar].into_iter().map(|path| OutputAsset {
                        path: ResourcePath::new(path).unwrap(), bytes: b"new".to_vec(),
                    }).collect(),
                ).unwrap()
            };
            let outputs = [
                PendingOutput { destination: t.0.join("a.json"), bundle: make("a.json", "shared") },
                PendingOutput { destination: t.0.join("b.json"), bundle: make("b.json", second) },
            ];
            assert!(persist_bundles(&outputs, &Sources::default()).is_err());
            assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 0);
        }
    }
    #[test]
    fn directory_bundle_excludes_other_outputs_and_their_sidecars() {
        let t = Temp::new();
        let directory = OutputBundle::directory(
            ResourcePath::new("font").unwrap(),
            vec![OutputAsset { path: ResourcePath::new("font/a").unwrap(), bytes: vec![1] }],
        ).unwrap();
        let other = OutputBundle::new(
            ResourcePath::new("outside").unwrap(),
            ["outside", "font/b"].into_iter().map(|path| OutputAsset {
                path: ResourcePath::new(path).unwrap(), bytes: vec![2],
            }).collect(),
        ).unwrap();
        let outputs = [
            PendingOutput { destination: t.0.join("font"), bundle: directory },
            PendingOutput { destination: t.0.join("outside"), bundle: other },
        ];
        assert!(persist_bundles(&outputs, &Sources::default()).is_err());
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 0);
    }
    #[test]
    fn cross_root_hardlinks_are_rejected_before_any_changes() {
        let a = Temp::new();
        let b = Temp::new();
        std::fs::write(a.0.join("a.bin"), b"old").unwrap();
        std::fs::hard_link(a.0.join("a.bin"), b.0.join("b.bin")).unwrap();
        let outputs = [pending(a.0.join("a.bin"), b"a"), pending(b.0.join("b.bin"), b"b")];
        assert!(persist_bundles(&outputs, &Sources::default()).is_err());
        assert_eq!(std::fs::read(a.0.join("a.bin")).unwrap(), b"old");
        assert_eq!(std::fs::read(b.0.join("b.bin")).unwrap(), b"old");
        assert_no_temporaries(&a.0);
        assert_no_temporaries(&b.0);
    }
    #[test]
    fn unread_directory_source_remains_protected_after_rename() {
        let t = Temp::new();
        let source = t.0.join("source");
        let moved = t.0.join("moved");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("a"), b"source").unwrap();
        let sources = Sources::default();
        let _input = FileInput::open(&source, &ResourceLimits::default(), sources.clone()).unwrap();
        assert!(!sources.is_single_primary());
        std::fs::rename(&source, &moved).unwrap();
        let outputs = [pending(moved.join("new.bin"), b"new")];
        assert!(persist_bundles(&outputs, &sources).is_err());
        assert_eq!(std::fs::read(moved.join("a")).unwrap(), b"source");
        assert!(!moved.join("new.bin").exists());
        assert_no_temporaries(&t.0);
    }
    #[test]
    fn observed_nested_source_directory_remains_protected_after_rename() {
        let t = Temp::new();
        let source = t.0.join("source");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/a"), b"source").unwrap();
        let sources = Sources::default();
        let input = FileInput::open(&source, &ResourceLimits::default(), sources.clone()).unwrap();
        assert_eq!(input.resolve(&ResourcePath::new("nested/a").unwrap(), 100).unwrap(), b"source");
        let moved = t.0.join("moved");
        std::fs::rename(source.join("nested"), &moved).unwrap();
        assert!(persist_bundles(&[pending(moved.join("new.bin"), b"new")], &sources).is_err());
        assert_eq!(std::fs::read(moved.join("a")).unwrap(), b"source");
        assert!(!moved.join("new.bin").exists());
    }
    #[test]
    fn remembered_sidecar_identity_protects_another_output_root() {
        let a = Temp::new();
        let b = Temp::new();
        std::fs::write(a.0.join("source"), b"source").unwrap();
        std::fs::write(a.0.join("sidecar"), b"sidecar").unwrap();
        std::fs::hard_link(a.0.join("sidecar"), b.0.join("target")).unwrap();
        let sources = Sources::default();
        let input = FileInput::open(&a.0.join("source"), &ResourceLimits::default(), sources.clone()).unwrap();
        input.resolve(&ResourcePath::new("sidecar").unwrap(), 100).unwrap();
        let outputs = [pending(a.0.join("new"), b"new"), pending(b.0.join("target"), b"bad")];
        assert!(persist_bundles(&outputs, &sources).is_err());
        assert!(!a.0.join("new").exists());
        assert_eq!(std::fs::read(b.0.join("target")).unwrap(), b"sidecar");
        assert_no_temporaries(&a.0);
        assert_no_temporaries(&b.0);
    }
    #[test]
    fn target_replaced_by_input_hardlink_during_commit_rolls_back_other_root() {
        let a = Temp::new();
        let b = Temp::new();
        std::fs::write(a.0.join("first"), b"old first").unwrap();
        std::fs::write(a.0.join("source"), b"source").unwrap();
        let sources = Sources::default();
        sources.remember(&File::open(a.0.join("source")).unwrap()).unwrap();
        let outputs = [pending(a.0.join("first"), b"new"), pending(b.0.join("second"), b"new")];
        assert!(run_group(&outputs, &sources, |index| {
            if index == 1 {
                std::fs::hard_link(a.0.join("source"), b.0.join("second"))?;
            }
            Ok(())
        }).is_err());
        assert_eq!(std::fs::read(a.0.join("first")).unwrap(), b"old first");
        assert_eq!(std::fs::read(a.0.join("source")).unwrap(), b"source");
        assert_eq!(std::fs::read(b.0.join("second")).unwrap(), b"source");
        assert_no_temporaries(&a.0);
        assert_no_temporaries(&b.0);
    }
    #[test]
    fn newly_remembered_input_is_rechecked_before_replacement() {
        let t = Temp::new();
        for name in ["a", "b"] {
            std::fs::write(t.0.join(name), b"old").unwrap();
        }
        let sources = Sources::default();
        let outputs = [pending(t.0.join("a"), b"new"), pending(t.0.join("b"), b"new")];
        assert!(run_group(&outputs, &sources, |index| {
            if index == 1 { sources.remember(&File::open(t.0.join("b"))?)?; }
            Ok(())
        }).is_err());
        for name in ["a", "b"] {
            assert_eq!(std::fs::read(t.0.join(name)).unwrap(), b"old");
        }
        assert_no_temporaries(&t.0);
    }
    #[cfg(unix)]
    #[test]
    fn parent_symlinks_are_rejected_even_before_dot_dot_normalization() {
        let t = Temp::new();
        let outside = Temp::new();
        std::os::unix::fs::symlink(&outside.0, t.0.join("link")).unwrap();
        for destination in [t.0.join("link/font.bin"), t.0.join("link/../font.bin")] {
            assert!(persist_bundles(&[pending(destination, b"new")], &Sources::default()).is_err());
        }
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn swapped_parent_during_commit_rolls_back_through_retained_capabilities() {
        let t = Temp::new();
        let outside = Temp::new();
        for dir in ["a", "b"] {
            std::fs::create_dir(t.0.join(dir)).unwrap();
            std::fs::write(t.0.join(dir).join("font.bin"), b"old").unwrap();
        }
        let outputs = [pending(t.0.join("a/font.bin"), b"a"), pending(t.0.join("b/font.bin"), b"b")];
        assert!(run_group(&outputs, &Sources::default(), |index| {
            if index == 1 {
                std::fs::rename(t.0.join("b"), t.0.join("moved"))?;
                std::os::unix::fs::symlink(&outside.0, t.0.join("b"))?;
            }
            Ok(())
        }).is_err());
        assert_eq!(std::fs::read(t.0.join("a/font.bin")).unwrap(), b"old");
        assert_eq!(std::fs::read(t.0.join("moved/font.bin")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
        assert_no_temporaries(&t.0);
    }
    #[test]
    fn cleanup_preserves_replacements_of_transaction_created_directories() {
        let t = Temp::new();
        let outputs = [PendingOutput { destination: t.0.join("font.json"), bundle: bundle() }];
        assert!(run_group(&outputs, &Sources::default(), |index| {
            if index == 0 {
                std::fs::rename(t.0.join("nested"), t.0.join("moved"))?;
                std::fs::create_dir(t.0.join("nested"))?;
                std::fs::write(t.0.join("nested/keep"), b"external")?;
            }
            Ok(())
        }).is_err());
        assert_eq!(std::fs::read(t.0.join("nested/keep")).unwrap(), b"external");
        assert!(!t.0.join("font.json").exists());
        assert!(!t.0.join("font.png").exists());
        assert_eq!(std::fs::read_dir(t.0.join("moved")).unwrap().count(), 0);
        assert_no_temporaries(&t.0);
    }
    #[test]
    fn late_extra_directory_member_rolls_back_the_complete_group() {
        let t = Temp::new();
        std::fs::create_dir(t.0.join("font")).unwrap();
        std::fs::write(t.0.join("font/a"), b"old a").unwrap();
        std::fs::write(t.0.join("outside"), b"old outside").unwrap();
        let directory = OutputBundle::directory(
            ResourcePath::new("font").unwrap(),
            vec![OutputAsset { path: ResourcePath::new("font/a").unwrap(), bytes: b"new a".to_vec() }],
        ).unwrap();
        let outputs = [
            PendingOutput { destination: t.0.join("font"), bundle: directory },
            pending(t.0.join("outside"), b"new outside"),
        ];
        assert!(run_group(&outputs, &Sources::default(), |index| {
            if index == 1 { std::fs::write(t.0.join("font/keep"), b"external")?; }
            Ok(())
        }).is_err());
        assert_eq!(std::fs::read(t.0.join("font/a")).unwrap(), b"old a");
        assert_eq!(std::fs::read(t.0.join("outside")).unwrap(), b"old outside");
        assert_eq!(std::fs::read(t.0.join("font/keep")).unwrap(), b"external");
        assert_no_temporaries(&t.0);
    }

}
