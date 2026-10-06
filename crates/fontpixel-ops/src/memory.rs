//! In-memory storage: named input buffers, explicit sidecar resolution and
//! returned output bundles. Nothing here touches the filesystem.
//!
//! Paths in requests are keys into a [`FileProvider`]. A primary input `a/b.fnt`
//! resolves its sidecars relative to `a/`, exactly as the filesystem mode
//! resolves them beside the primary file; a key with members below it
//! (`a/font/65.png`) is a directory input. Reads are bounded by the same input
//! and resource limits as files, and the returned buffers never exceed them.
use crate::context::PreparedOutput;
use fontpixel::resources::ResourceResolver;
use fontpixel::{Error, Input, ResourceLimits, ResourcePath, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, io,
    path::{Component, Path},
    sync::Arc,
};

/// Where an operation reads its inputs and delivers its outputs.
#[derive(Clone)]
pub enum Storage {
    /// Inputs come from a caller-supplied file set; outputs are validated as
    /// one group and returned with the response instead of being written.
    Memory(Arc<dyn FileProvider>),
    /// Real paths with sidecar discovery, source-alias protection and the
    /// transactional multi-output persistence of the command-line tool.
    #[cfg(feature = "fs")]
    Filesystem,
}
impl Storage {
    /// Memory storage over the given file set.
    pub fn memory(files: impl FileProvider + 'static) -> Self {
        Self::Memory(Arc::new(files))
    }
}
impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory(_) => f.write_str("Storage::Memory"),
            #[cfg(feature = "fs")]
            Self::Filesystem => f.write_str("Storage::Filesystem"),
        }
    }
}

/// Kind of a named entry in a [`FileProvider`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file of the given length in bytes.
    File {
        /// Length in bytes.
        len: u64,
    },
    /// A directory containing further entries.
    Directory,
}

/// Explicit resolver for memory storage. Keys are normalized `/`-separated
/// paths without `.` components (see [`key`]); the empty key is the root.
pub trait FileProvider: Send + Sync {
    /// The entry at `path`, if any.
    fn entry(&self, path: &str) -> Option<EntryKind>;
    /// The complete contents of the file at `path`. Called only after
    /// [`FileProvider::entry`] reported a file within the caller's limit.
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    /// Names (not paths) of the entries directly inside `directory`, in a
    /// deterministic order.
    fn list(&self, directory: &str) -> Result<Vec<String>>;
    /// Whether `path` names a regular file.
    fn is_file(&self, path: &str) -> bool {
        matches!(self.entry(path), Some(EntryKind::File { .. }))
    }
}

/// A simple named-file set. Directories exist implicitly for every file key
/// that has `/`-separated parents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryFiles {
    files: BTreeMap<String, Vec<u8>>,
}
impl MemoryFiles {
    /// An empty file set.
    pub fn new() -> Self {
        Self::default()
    }
    /// Add or replace one file. The path is normalized with [`key`].
    pub fn insert(&mut self, path: impl AsRef<Path>, bytes: impl Into<Vec<u8>>) -> &mut Self {
        self.files.insert(key(path.as_ref()), bytes.into());
        self
    }
    /// Builder form of [`MemoryFiles::insert`].
    pub fn with(mut self, path: impl AsRef<Path>, bytes: impl Into<Vec<u8>>) -> Self {
        self.insert(path, bytes);
        self
    }
    /// The bytes of one file.
    pub fn get(&self, path: impl AsRef<Path>) -> Option<&[u8]> {
        self.files.get(&key(path.as_ref())).map(Vec::as_slice)
    }
    /// Remove one file, returning its bytes.
    pub fn remove(&mut self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
        self.files.remove(&key(path.as_ref()))
    }
    /// Every file key and its bytes, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }
    /// Number of files.
    pub fn len(&self) -> usize {
        self.files.len()
    }
    /// Whether the set contains no files.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
    fn children(&self, directory: &str) -> impl Iterator<Item = (&str, bool)> {
        let prefix = if directory.is_empty() {
            String::new()
        } else {
            format!("{directory}/")
        };
        let mut seen = BTreeSet::new();
        self.files
            .range(prefix.clone()..)
            .take_while(move |(k, _)| k.starts_with(&prefix))
            .filter_map(move |(k, _)| {
                let rest = &k[prefix_len(directory)..];
                let (name, file) = match rest.split_once('/') {
                    Some((name, _)) => (name, false),
                    None => (rest, true),
                };
                seen.insert(name).then_some((name, file))
            })
    }
}
fn prefix_len(directory: &str) -> usize {
    if directory.is_empty() {
        0
    } else {
        directory.len() + 1
    }
}
impl FileProvider for MemoryFiles {
    fn entry(&self, path: &str) -> Option<EntryKind> {
        if let Some(bytes) = self.files.get(path) {
            return Some(EntryKind::File {
                len: bytes.len() as u64,
            });
        }
        (path.is_empty() || self.children(path).next().is_some()).then_some(EntryKind::Directory)
    }
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let bytes = self.files.get(path).ok_or_else(|| not_found(path))?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(bytes.len())
            .map_err(|_| Error::ResourceLimit {
                resource: "input asset bytes",
                limit: bytes.len(),
            })?;
        copy.extend_from_slice(bytes);
        Ok(copy)
    }
    fn list(&self, directory: &str) -> Result<Vec<String>> {
        Ok(self
            .children(directory)
            .map(|(name, _)| name.to_owned())
            .collect())
    }
}

/// Normalize a request path into a provider key: `.` components are removed,
/// `..` removes the previous component where one exists, separators become `/`
/// and an absolute path keeps a leading `/`.
pub fn key(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut absolute = false;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|p| p != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..".into());
                }
            }
            Component::RootDir => absolute = true,
            Component::Prefix(prefix) => parts.push(prefix.as_os_str().to_string_lossy().into()),
            Component::Normal(name) => parts.push(name.to_string_lossy().into()),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}
fn join(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_owned()
    } else if directory == "/" {
        format!("/{name}")
    } else {
        format!("{directory}/{name}")
    }
}
fn parent_key(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".into(),
        Some(index) => path[..index].to_owned(),
        None => String::new(),
    }
}
fn not_found(path: &str) -> Error {
    Error::Io(io::Error::new(
        io::ErrorKind::NotFound,
        format!("no such memory file: {path}"),
    ))
}
fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
fn input_limit(maximum: usize) -> Error {
    Error::ResourceLimit {
        resource: "input asset bytes",
        limit: maximum,
    }
}
/// Bounded copy with the filesystem reader's checks and capacity guarantee.
fn bounded(files: &dyn FileProvider, path: &str, maximum: usize) -> Result<Vec<u8>> {
    match files.entry(path) {
        None => Err(not_found(path)),
        Some(EntryKind::Directory) => Err(invalid("font input must be a regular file")),
        Some(EntryKind::File { len }) => {
            if len > maximum as u64 {
                return Err(input_limit(maximum));
            }
            let bytes = files.read(path)?;
            if bytes.len() > maximum || bytes.capacity() > maximum {
                return Err(input_limit(maximum));
            }
            Ok(bytes)
        }
    }
}

/// One opened memory input: a primary file with its sibling resources, or a
/// directory of member files.
pub(crate) struct MemoryInput {
    files: Arc<dyn FileProvider>,
    bytes: Vec<u8>,
    name: String,
    root: String,
    directory: bool,
}
impl MemoryInput {
    pub(crate) fn open(
        files: Arc<dyn FileProvider>,
        path: &Path,
        limits: &ResourceLimits,
    ) -> Result<Self> {
        let full = key(path);
        let directory = match files.entry(&full) {
            None => return Err(not_found(&full)),
            Some(EntryKind::Directory) => true,
            Some(EntryKind::File { .. }) => false,
        };
        let (root, bytes) = if directory {
            (full.clone(), Vec::new())
        } else {
            (
                parent_key(&full),
                bounded(
                    files.as_ref(),
                    &full,
                    limits.max_input_bytes.min(limits.max_total_bytes),
                )?,
            )
        };
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("input must have a UTF-8 filename"))?
            .to_owned();
        Ok(Self {
            files,
            bytes,
            name,
            root,
            directory,
        })
    }
    pub(crate) fn input(&self) -> Input<'_> {
        if self.directory {
            return Input::directory(&self.name, self);
        }
        Input::new(&self.bytes)
            .with_name(&self.name)
            .with_resources(self)
    }
}
impl ResourceResolver for MemoryInput {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        let full = join(&self.root, path.as_str());
        if self.files.entry(&full).is_none() {
            return Err(Error::MissingResource(path.to_string()));
        }
        bounded(self.files.as_ref(), &full, max_bytes)
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        let base = match directory {
            Some(path) => join(&self.root, path.as_str()),
            None => self.root.clone(),
        };
        if !matches!(self.files.entry(&base), Some(EntryKind::Directory)) {
            return Err(not_found(&base));
        }
        let mut paths = Vec::new();
        let mut used = 0usize;
        for (index, name) in self.files.list(&base)?.into_iter().enumerate() {
            if index >= max_entries {
                return Err(Error::ResourceLimit {
                    resource: "directory entries",
                    limit: max_entries,
                });
            }
            if !self.files.is_file(&join(&base, &name)) {
                continue;
            }
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

/// Read one complete primary input without sidecars.
#[cfg(feature = "opentype")]
pub(crate) fn read_primary(
    files: &Arc<dyn FileProvider>,
    path: &Path,
    limits: &ResourceLimits,
) -> Result<Vec<u8>> {
    bounded(
        files.as_ref(),
        &key(path),
        limits.max_input_bytes.min(limits.max_total_bytes),
    )
    .map_err(|error| match error {
        Error::ResourceLimit { limit, .. } => Error::ResourceLimit {
            resource: "primary input bytes",
            limit,
        },
        error => error,
    })
}

impl PreparedOutput {
    /// Every asset with its storage key: the destination's parent key joined
    /// with the asset's bundle path (the primary asset is the destination).
    pub fn keyed_assets(&self) -> impl Iterator<Item = (String, &[u8])> + '_ {
        let parent = self.destination.parent().map(key).unwrap_or_default();
        self.bundle
            .assets()
            .iter()
            .map(move |asset| (join(&parent, asset.path.as_str()), asset.bytes.as_slice()))
    }
}
/// Every asset key a prepared output group would create.
pub(crate) fn asset_keys(outputs: &[PreparedOutput]) -> impl Iterator<Item = String> + '_ {
    outputs
        .iter()
        .flat_map(|output| output.keyed_assets().map(|(key, _)| key))
}
/// Validate a memory output group as one unit: every asset needs a distinct
/// destination that is neither an ancestor nor a member of another asset.
pub(crate) fn check_group(outputs: &[PreparedOutput]) -> Result<()> {
    let mut paths = BTreeSet::new();
    for path in asset_keys(outputs) {
        let overlaps = Path::new(&path)
            .ancestors()
            .any(|ancestor| paths.contains(&key(ancestor)))
            || paths
                .range(path.clone()..)
                .next()
                .is_some_and(|next: &String| {
                    next.starts_with(&format!("{path}/")) || *next == path
                });
        if overlaps {
            return Err(invalid(
                "output paths are duplicates or overlap an ancestor target",
            ));
        }
        paths.insert(path);
    }
    Ok(())
}
