//! Sandboxed, caller-supplied sidecar inputs and validated multi-asset outputs.
//!
//! Codecs must use [`ReadContext::resource`] for sidecars and return bytes rather
//! than writing to disk. Resolvers own their authority boundary: a filesystem
//! resolver must also prevent symlink escape; lexical validation alone is not a
//! filesystem sandbox. No filesystem resolver is supplied at this stage.
use crate::{Error, Report, Result};
use std::{collections::BTreeSet, fmt};

/// Portable relative asset name with no empty, parent, or current components.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourcePath(String);

impl ResourcePath {
    /// Validate an asset path. `/` separates components on every platform.
    pub fn new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        if path.is_empty()
            || path.contains(['\\', ':'])
            || path.chars().any(char::is_control)
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(Error::InvalidInput(format!(
                "unsafe relative asset path '{path}'"
            )));
        }
        Ok(Self(path))
    }

    /// Borrow the validated portable relative path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Configurable input budgets. Format implementations must also enforce the
/// decompression, glyph, pixel, and table limits before allocating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Maximum primary input size.
    pub max_input_bytes: usize,
    /// Maximum size of one sidecar.
    pub max_resource_bytes: usize,
    /// Maximum primary + cumulative resolved sidecar bytes.
    pub max_total_bytes: usize,
    /// Maximum number of resource resolutions (including repeats).
    pub max_resources: usize,
    /// Maximum decompressed size, enforced by future compression readers.
    pub max_decompressed_bytes: usize,
    /// Maximum glyph count, enforced by future model readers.
    pub max_glyphs: usize,
    /// Maximum pixels in one glyph, enforced by future model readers.
    pub max_pixels_per_glyph: usize,
    /// Maximum font table size, enforced by future table readers.
    pub max_table_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_resource_bytes: 64 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            max_resources: 1024,
            max_decompressed_bytes: 256 * 1024 * 1024,
            max_glyphs: 1_000_000,
            max_pixels_per_glyph: 16 * 1024 * 1024,
            max_table_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Resolves only resources explicitly made available by the caller.
/// Implementations must honor `max_bytes` before allocating an unbounded result.
pub trait ResourceResolver: Send + Sync {
    /// Retrieve one validated relative resource, subject to the byte budget.
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>>;
}

/// Resolver for standalone files; sidecar requests fail explicitly.
#[derive(Debug, Default)]
pub struct NoResources;
impl ResourceResolver for NoResources {
    fn resolve(&self, path: &ResourcePath, _max_bytes: usize) -> Result<Vec<u8>> {
        Err(Error::MissingResource(path.to_string()))
    }
}
static NO_RESOURCES: NoResources = NoResources;

/// Primary bytes, a filename hint, and a bounded caller-controlled resource source.
pub struct Input<'a> {
    bytes: &'a [u8],
    name: Option<&'a str>,
    resolver: &'a dyn ResourceResolver,
}

impl<'a> Input<'a> {
    /// Standalone input without filename hints or sidecars.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            name: None,
            resolver: &NO_RESOURCES,
        }
    }

    /// Attach a filename hint; it never overrides stronger content evidence.
    pub fn with_name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }

    /// Make explicitly scoped sidecar resources available.
    pub fn with_resources(mut self, resolver: &'a dyn ResourceResolver) -> Self {
        self.resolver = resolver;
        self
    }
}

/// Shared resource accounting for detection and decoding of one input.
pub struct ReadContext<'a> {
    input: &'a Input<'a>,
    limits: &'a ResourceLimits,
    total_bytes: usize,
    resource_count: usize,
}

impl<'a> ReadContext<'a> {
    /// Validate primary input and initialize the resource budget.
    pub fn new(input: &'a Input<'a>, limits: &'a ResourceLimits) -> Result<Self> {
        if input.bytes.len() > limits.max_input_bytes {
            return Err(Error::ResourceLimit {
                resource: "primary input bytes",
                limit: limits.max_input_bytes,
            });
        }
        if input.bytes.len() > limits.max_total_bytes {
            return Err(Error::ResourceLimit {
                resource: "total input bytes",
                limit: limits.max_total_bytes,
            });
        }
        Ok(Self {
            input,
            limits,
            total_bytes: input.bytes.len(),
            resource_count: 0,
        })
    }

    /// Primary bytes, already checked against input and total limits.
    pub fn bytes(&self) -> &[u8] {
        self.input.bytes
    }

    /// Optional filename hint.
    pub fn name(&self) -> Option<&str> {
        self.input.name
    }

    /// Budgets for parsers to check before format-specific allocation.
    pub fn limits(&self) -> &ResourceLimits {
        self.limits
    }

    /// Resolve a sidecar and account for every returned byte, including repeats.
    pub fn resource(&mut self, path: &ResourcePath) -> Result<Vec<u8>> {
        if self.resource_count >= self.limits.max_resources {
            return Err(Error::ResourceLimit {
                resource: "resource count",
                limit: self.limits.max_resources,
            });
        }
        self.resource_count += 1;
        let remaining = self.limits.max_total_bytes - self.total_bytes;
        let budget = self.limits.max_resource_bytes.min(remaining);
        let bytes = self.input.resolver.resolve(path, budget)?;
        if bytes.len() > budget {
            return Err(Error::ResourceLimit {
                resource: "resource bytes",
                limit: budget,
            });
        }
        self.total_bytes += bytes.len();
        Ok(bytes)
    }
}

/// One in-memory output asset; the caller decides where to persist it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputAsset {
    /// Validated relative path, including any subdirectory.
    pub path: ResourcePath,
    /// Exact bytes, with no implicit timestamps or transformations.
    pub bytes: Vec<u8>,
}

/// Complete validated output, with one entry point and zero or more sidecars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputBundle {
    primary: ResourcePath,
    assets: Vec<OutputAsset>,
}

impl OutputBundle {
    /// Reject empty bundles, absent entry points, duplicates, and file/directory
    /// collisions before any filesystem writer receives the assets.
    pub fn new(primary: ResourcePath, mut assets: Vec<OutputAsset>) -> Result<Self> {
        let mut paths = BTreeSet::new();
        for asset in &assets {
            if !paths.insert(asset.path.as_str()) {
                return Err(Error::InvalidInput(format!(
                    "duplicate output asset '{}'",
                    asset.path
                )));
            }
        }
        if !paths.contains(primary.as_str()) {
            return Err(Error::InvalidInput(
                "output bundle is missing its primary asset".into(),
            ));
        }
        for path in &paths {
            for (index, _) in path.match_indices('/') {
                if paths.contains(&path[..index]) {
                    return Err(Error::InvalidInput(format!(
                        "output asset '{path}' conflicts with an ancestor file"
                    )));
                }
            }
        }
        assets.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { primary, assets })
    }

    /// Construct a one-file bundle.
    pub fn single(path: ResourcePath, bytes: Vec<u8>) -> Self {
        Self {
            primary: path.clone(),
            assets: vec![OutputAsset { path, bytes }],
        }
    }

    /// Entry-point path to give the consuming application.
    pub fn primary(&self) -> &ResourcePath {
        &self.primary
    }

    /// Assets sorted by path for deterministic traversal.
    pub fn assets(&self) -> &[OutputAsset] {
        &self.assets
    }

    /// Move the assets to a persistence layer after strict validation succeeds.
    pub fn into_assets(self) -> Vec<OutputAsset> {
        self.assets
    }

    /// Single-file convenience API; never silently discard sidecars.
    pub fn into_single(self) -> Result<Vec<u8>> {
        if self.assets.len() != 1 {
            return Err(Error::MultipleAssets {
                count: self.assets.len(),
            });
        }
        self.assets
            .into_iter()
            .next()
            .map(|asset| asset.bytes)
            .ok_or_else(|| Error::InvalidInput("output bundle is empty".into()))
    }
}

/// Result from a format writer, still wholly in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// All encoded assets.
    pub output: OutputBundle,
    /// Conversion diagnostics.
    pub report: Report,
}
