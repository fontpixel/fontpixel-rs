//! CLI-owned authority and persistence; the library never opens filesystem paths.
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use fontpixel::resources::{OutputBundle, ResourceResolver};
use fontpixel::{Error, Input, ResourceLimits, ResourcePath, Result};
use same_file::Handle;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Default)]
pub(crate) struct Sources(Arc<Mutex<Vec<Handle>>>);
impl Sources {
    pub fn is_single_primary(&self) -> bool {
        self.0.lock().is_ok_and(|s| s.len() == 1)
    }
    fn remember(&self, file: &File) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| invalid("input identity lock poisoned"))?
            .push(Handle::from_file(file.try_clone()?)?);
        Ok(())
    }
    fn contains(&self, file: File) -> Result<bool> {
        let handle = Handle::from_file(file)?;
        Ok(self
            .0
            .lock()
            .map_err(|_| invalid("input identity lock poisoned"))?
            .contains(&handle))
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
}
impl FileInput {
    pub fn open(path: &Path, limits: &ResourceLimits, sources: Sources) -> Result<Self> {
        let root = Dir::open_ambient_dir(parent(path), ambient_authority())?;
        let file = File::open(path)?;
        sources.remember(&file)?;
        let bytes = bounded(file, limits.max_input_bytes.min(limits.max_total_bytes))?;
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
        })
    }
    pub fn input(&self) -> Input<'_> {
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
    dir: Dir,
    target: String,
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
fn preflight(root: &Dir, path: &Path, sources: &Sources, outputs: &mut Vec<Handle>) -> Result<()> {
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
                    let handle = Handle::from_file(file)?;
                    if outputs.contains(&handle) {
                        return Err(invalid("output assets alias each other"));
                    }
                    outputs.push(handle);
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
    let mut outputs = Vec::new();
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
        let dir = tx.root.open_dir(parent(path))?;
        let target = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("invalid output filename"))?
            .to_owned();
        let (temp, mut file) = temporary(&dir, "stage")?;
        tx.stages.push(Stage {
            dir,
            target,
            temp,
            backup: None,
            installed: false,
        });
        file.write_all(&a.bytes)?;
        file.sync_all()?;
    }
    // Publish supporting assets before the entry point.
    tx.stages
        .sort_by_key(|s| s.target == bundle.primary().as_str());
    for (i, s) in tx.stages.iter_mut().enumerate() {
        if fail_after == Some(i) {
            return Err(invalid("injected commit failure"));
        }
        // Recheck the final directory entry before any replacement.
        preflight(&s.dir, Path::new(&s.target), sources, &mut Vec::new())?;
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
