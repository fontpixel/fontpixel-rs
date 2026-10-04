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
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
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
#[derive(Default)]
struct SourceState {
    identities: HashSet<FileIdentity>,
    reads: usize,
}
#[derive(Clone, Default)]
pub(crate) struct Sources(Arc<Mutex<SourceState>>);
impl Sources {
    pub fn is_single_primary(&self) -> bool {
        self.0.lock().is_ok_and(|s| s.reads == 1)
    }
    fn remember(&self, file: &File) -> Result<()> {
        let identity = FileIdentity::of(file)?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| invalid("input identity lock poisoned"))?;
        state.reads = state.reads.saturating_add(1);
        state.identities.insert(identity);
        Ok(())
    }
    fn contains(&self, file: File) -> Result<bool> {
        let identity = FileIdentity::of(&file)?;
        Ok(self
            .0
            .lock()
            .map_err(|_| invalid("input identity lock poisoned"))?
            .identities
            .contains(&identity))
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
        let (root, bytes) = if directory {
            (Dir::from_std_file(file), Vec::new())
        } else {
            sources.remember(&file)?;
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
        self.sources.remember(&file)?;
        bounded(file, max_bytes)
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
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
struct Stage {
    dir: Arc<Dir>,
    target: String,
    is_primary: bool,
    temp: String,
    backup: Option<String>,
    installed: bool,
}
struct Transaction {
    root: Dir,
    stages: Vec<Stage>,
    created: Vec<PathBuf>,
    committed: bool,
}
impl Drop for Transaction {
    fn drop(&mut self) {
        for s in self.stages.iter_mut().rev() {
            if !self.committed {
                if s.installed {
                    let _ = s.dir.remove_file(&s.target);
                }
                if let Some(b) = &s.backup {
                    // Keep a backup on disk if recovery itself fails.
                    if s.dir.rename(b, &s.dir, &s.target).is_ok() {
                        s.backup = None;
                    }
                }
            }
            let _ = s.dir.remove_file(&s.temp);
            if self.committed
                && let Some(b) = &s.backup
            {
                let _ = s.dir.remove_file(b);
            }
        }
        if !self.committed {
            for path in self.created.iter().rev() {
                let _ = self.root.remove_dir(path);
            }
        }
    }
}
fn preflight(
    root: &Dir,
    path: &Path,
    sources: &Sources,
    outputs: &mut HashSet<FileIdentity>,
) -> Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        match root.symlink_metadata(&prefix) {
            Ok(m) => {
                if m.file_type().is_symlink() {
                    return Err(invalid(format!(
                        "output symlink is not allowed: {}",
                        prefix.display()
                    )));
                }
                if prefix != path {
                    if !m.is_dir() {
                        return Err(invalid("output parent is not a directory"));
                    }
                } else {
                    if !m.is_file() {
                        return Err(invalid("output target is not a regular file"));
                    }
                    let file = root.open(path)?.into_std();
                    if sources.contains(file.try_clone()?)? {
                        return Err(invalid("output aliases a primary input or sidecar"));
                    }
                    let identity = FileIdentity::of(&file)?;
                    if !outputs.insert(identity) {
                        return Err(invalid("output assets alias each other"));
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(e) => return Err(e.into()),
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
// A directory is a complete font, so unrelated old members cannot be merged
// into it. Check each existing directory against the new asset tree before
// creating temporary files or replacing any existing asset.
fn preflight_directory(root: &Dir, bundle: &OutputBundle) -> Result<()> {
    if !bundle.is_directory() {
        return Ok(());
    }
    let base = Path::new(bundle.primary().as_str());
    let mut expected = BTreeMap::<PathBuf, HashSet<std::ffi::OsString>>::new();
    for asset in bundle.assets() {
        let mut path = Path::new(asset.path.as_str());
        while path != base {
            let name = path
                .file_name()
                .ok_or_else(|| invalid("invalid directory asset"))?;
            let directory = path
                .parent()
                .ok_or_else(|| invalid("invalid directory asset parent"))?;
            expected
                .entry(directory.to_owned())
                .or_default()
                .insert(name.to_owned());
            path = directory;
        }
    }
    for (path, names) in expected {
        let metadata = match root.symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(invalid("directory output target must be a real directory"));
        }
        for entry in root.read_dir(&path)? {
            let entry = entry?;
            if !names.contains(&entry.file_name()) {
                return Err(invalid(
                    "output font directory contains extra members; use a new or empty directory",
                ));
            }
        }
    }
    Ok(())
}
fn persist_impl(
    destination: &Path,
    bundle: &OutputBundle,
    sources: &Sources,
    fail_after: Option<usize>,
) -> Result<()> {
    if destination.file_name().and_then(|s| s.to_str()) != Some(bundle.primary().as_str()) {
        return Err(invalid(
            "output bundle entry point differs from destination",
        ));
    }
    let root = Dir::open_ambient_dir(parent(destination), ambient_authority())?;
    preflight_directory(&root, bundle)?;
    let mut outputs = HashSet::new();
    let mut directories = BTreeMap::<PathBuf, Arc<Dir>>::new();
    for a in bundle.assets() {
        preflight(&root, Path::new(a.path.as_str()), sources, &mut outputs)?;
    }
    let mut tx = Transaction {
        root,
        stages: Vec::new(),
        created: Vec::new(),
        committed: false,
    };
    for a in bundle.assets() {
        let path = Path::new(a.path.as_str());
        let mut prefix = PathBuf::new();
        for component in parent(path).components() {
            if component == std::path::Component::CurDir {
                continue;
            }
            prefix.push(component);
            match tx.root.create_dir(&prefix) {
                Ok(()) => tx.created.push(prefix.clone()),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        let dir = match directories.get(parent(path)) {
            Some(dir) => Arc::clone(dir),
            None => {
                let dir = Arc::new(tx.root.open_dir(parent(path))?);
                directories.insert(parent(path).to_owned(), Arc::clone(&dir));
                dir
            }
        };
        let target = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("invalid output filename"))?
            .to_owned();
        let (temp, mut file) = temporary(&dir, "stage")?;
        tx.stages.push(Stage {
            dir,
            target,
            is_primary: !bundle.is_directory() && &a.path == bundle.primary(),
            temp,
            backup: None,
            installed: false,
        });
        file.write_all(&a.bytes)?;
        file.sync_all()?;
    }
    // Publish supporting assets before the entry point.
    tx.stages.sort_by_key(|s| s.is_primary);
    for (i, s) in tx.stages.iter_mut().enumerate() {
        if fail_after == Some(i) {
            return Err(invalid("injected commit failure"));
        }
        // Recheck the final directory entry before any replacement.
        preflight(&s.dir, Path::new(&s.target), sources, &mut HashSet::new())?;
        if s.dir.try_exists(&s.target)? {
            let (backup, file) = temporary(&s.dir, "backup")?;
            drop(file);
            if let Err(e) = s.dir.rename(&s.target, &s.dir, &backup) {
                let _ = s.dir.remove_file(&backup);
                return Err(e.into());
            }
            s.backup = Some(backup);
        }
        s.dir.rename(&s.temp, &s.dir, &s.target)?;
        s.installed = true;
    }
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
}
