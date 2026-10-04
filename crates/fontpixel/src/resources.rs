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
    /// Maximum decompressed size, enforced by the shared gzip transport.
    pub max_decompressed_bytes: usize,
    /// Maximum glyph count, enforced by model readers.
    pub max_glyphs: usize,
    /// Maximum pixels in one glyph, enforced by model readers.
    pub max_pixels_per_glyph: usize,
    /// Maximum font table or text record size, enforced by format readers.
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

/// Additional native-write and registry-applied envelope budgets.
///
/// These include the encoded bytes, their uncompressed representation and
/// retained diagnostics. Built-in writers expose their actual typed limits;
/// common write options may further restrict, but never enlarge, those limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportLimits {
    /// Limits used by the matching reader and native writer validation.
    pub resources: ResourceLimits,
    /// Maximum sum of serialized asset lengths for plain output, or maximum
    /// serialized envelope size including its header and trailer for gzip.
    /// Plain output is checked before returning assets, after error/strict
    /// diagnostics. A format without a native output cap may finish encoding
    /// under its resource budget before this aggregate returned-output check.
    pub max_output_bytes: usize,
}

impl Default for TransportLimits {
    fn default() -> Self {
        Self {
            resources: ResourceLimits::default(),
            max_output_bytes: ResourceLimits::default().max_input_bytes,
        }
    }
}

impl TransportLimits {
    pub(crate) fn intersect(&self, other: &Self) -> Self {
        let a = &self.resources;
        let b = &other.resources;
        Self {
            resources: ResourceLimits {
                max_input_bytes: a.max_input_bytes.min(b.max_input_bytes),
                max_resource_bytes: a.max_resource_bytes.min(b.max_resource_bytes),
                max_total_bytes: a.max_total_bytes.min(b.max_total_bytes),
                max_resources: a.max_resources.min(b.max_resources),
                max_decompressed_bytes: a.max_decompressed_bytes.min(b.max_decompressed_bytes),
                max_glyphs: a.max_glyphs.min(b.max_glyphs),
                max_pixels_per_glyph: a.max_pixels_per_glyph.min(b.max_pixels_per_glyph),
                max_table_bytes: a.max_table_bytes.min(b.max_table_bytes),
            },
            max_output_bytes: self.max_output_bytes.min(other.max_output_bytes),
        }
    }

    /// Reserve already retained envelope/report memory for native validation.
    pub(crate) fn without_retained(&self, retained: usize) -> Result<Self> {
        let mut limits = self.clone();
        limits.resources.max_total_bytes = self
            .resources
            .max_total_bytes
            .checked_sub(retained)
            .ok_or(Error::ResourceLimit {
                resource: "gzip retained bytes",
                limit: self.resources.max_total_bytes,
            })?;
        Ok(limits)
    }
}

/// Resolves only resources explicitly made available by the caller.
/// Implementations must bound the returned vector's allocation capacity by
/// `max_bytes`, not just its length.
pub trait ResourceResolver: Send + Sync {
    /// Retrieve one validated relative resource, subject to the byte budget.
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>>;

    /// Enumerate immediate regular-file resources beneath a directory (`None`
    /// means the resolver root). Paths are relative to the resolver root.
    /// Bound visited directory entries by `max_entries` and allocation by
    /// `max_bytes`, charging twice `size_of::<ResourcePath>() + path.len()` for
    /// each returned path. Do not recurse or follow directory symlinks.
    fn list(
        &self,
        _directory: Option<&ResourcePath>,
        _max_entries: usize,
        _max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        Err(Error::InvalidInput(
            "resource enumeration is unavailable".into(),
        ))
    }
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
    directory: bool,
}

impl<'a> Input<'a> {
    /// Standalone input without filename hints or sidecars.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            name: None,
            resolver: &NO_RESOURCES,
            directory: false,
        }
    }

    /// Directory entry point without a synthetic primary file. The resolver
    /// root is this directory; callers must select its format explicitly.
    pub fn directory(name: &'a str, resolver: &'a dyn ResourceResolver) -> Self {
        Self {
            bytes: &[],
            name: Some(name),
            resolver,
            directory: true,
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
    decoded: Option<Vec<u8>>,
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
        let decoded = decode_transport(input.bytes, limits)?;
        let total_bytes = input
            .bytes
            .len()
            .checked_add(decoded.as_ref().map_or(0, Vec::capacity))
            .ok_or(Error::ResourceLimit {
                resource: "total input bytes",
                limit: limits.max_total_bytes,
            })?;
        if total_bytes > limits.max_total_bytes {
            return Err(Error::ResourceLimit {
                resource: "total input bytes",
                limit: limits.max_total_bytes,
            });
        }
        Ok(Self {
            decoded,
            input,
            limits,
            total_bytes,
            resource_count: 0,
        })
    }

    /// Primary bytes, already checked against input and total limits.
    pub fn bytes(&self) -> &[u8] {
        self.decoded.as_deref().unwrap_or(self.input.bytes)
    }

    /// Bytes retained by the input context, including gzip wire and decoded capacity.
    /// Includes every resolved resource so custom codecs can budget their model
    /// and scratch allocations against the remaining aggregate allowance.
    pub fn retained_input_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Optional filename hint.
    pub fn name(&self) -> Option<&str> {
        self.input.name
    }

    /// Whether the caller supplied a native directory entry point.
    pub fn is_directory(&self) -> bool {
        self.input.directory
    }

    /// Budgets for parsers to check before format-specific allocation.
    pub fn limits(&self) -> &ResourceLimits {
        self.limits
    }

    /// Resolve a sidecar and account for its allocation capacity, including repeats.
    pub fn resource(&mut self, path: &ResourcePath) -> Result<Vec<u8>> {
        self.resource_limited(path, usize::MAX)
    }

    /// Resolve with an additional codec allowance for retained model and scratch bytes.
    pub fn resource_limited(&mut self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        if self.resource_count >= self.limits.max_resources {
            return Err(Error::ResourceLimit {
                resource: "resource count",
                limit: self.limits.max_resources,
            });
        }
        self.resource_count += 1;
        let remaining = self.limits.max_total_bytes - self.total_bytes;
        let budget = self.limits.max_resource_bytes.min(remaining).min(max_bytes);
        let bytes = self.input.resolver.resolve(path, budget)?;
        if bytes.capacity() > budget {
            return Err(Error::ResourceLimit {
                resource: "resource bytes",
                limit: budget,
            });
        }
        self.total_bytes += bytes.capacity();
        Ok(bytes)
    }

    /// List immediate file resources with deterministic order and shared input
    /// accounting. `max_bytes` further reserves space for a codec's live model.
    /// The listing is one resource operation; each later read is another.
    pub fn list_resources_limited(
        &mut self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        if self.resource_count >= self.limits.max_resources {
            return Err(Error::ResourceLimit {
                resource: "resource count",
                limit: self.limits.max_resources,
            });
        }
        self.resource_count += 1;
        let maximum = max_bytes.min(self.limits.max_total_bytes - self.total_bytes);
        let mut paths = self.input.resolver.list(directory, max_entries, maximum)?;
        if paths.len() > max_entries {
            return Err(Error::ResourceLimit {
                resource: "directory entries",
                limit: max_entries,
            });
        }
        let mut used = 0usize;
        for path in &paths {
            let relative = match directory {
                Some(parent) => path
                    .as_str()
                    .strip_prefix(parent.as_str())
                    .and_then(|s| s.strip_prefix('/')),
                None => Some(path.as_str()),
            };
            if relative.is_none_or(|s| s.is_empty() || s.contains('/')) {
                return Err(Error::InvalidInput(
                    "resource listing escapes its immediate directory".into(),
                ));
            }
            used = used.saturating_add(
                std::mem::size_of::<ResourcePath>()
                    .saturating_add(path.as_str().len())
                    .saturating_mul(2),
            );
            if used > maximum {
                return Err(Error::ResourceLimit {
                    resource: "directory listing bytes",
                    limit: maximum,
                });
            }
        }
        paths.sort_unstable();
        if paths.windows(2).any(|p| p[0] == p[1]) {
            return Err(Error::InvalidInput(
                "duplicate resource in directory listing".into(),
            ));
        }
        self.total_bytes += used;
        Ok(paths)
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
    directory: bool,
}

impl OutputBundle {
    /// Reject empty bundles, absent entry points, duplicates, and file/directory
    /// collisions before any filesystem writer receives the assets.
    pub fn new(primary: ResourcePath, assets: Vec<OutputAsset>) -> Result<Self> {
        Self::validated(primary, assets, false)
    }

    /// Native directory output. Every asset must be strictly beneath the given
    /// entry point, and at least one asset must exist. Paths stay relative to the
    /// entry point's parent, as for a file bundle.
    pub fn directory(primary: ResourcePath, assets: Vec<OutputAsset>) -> Result<Self> {
        Self::validated(primary, assets, true)
    }

    fn validated(
        primary: ResourcePath,
        mut assets: Vec<OutputAsset>,
        directory: bool,
    ) -> Result<Self> {
        let mut paths = BTreeSet::new();
        for asset in &assets {
            if !paths.insert(asset.path.as_str()) {
                return Err(Error::InvalidInput(format!(
                    "duplicate output asset '{}'",
                    asset.path
                )));
            }
        }
        if directory
            && (paths.is_empty()
                || paths.iter().any(|p| {
                    p.strip_prefix(primary.as_str())
                        .is_none_or(|tail| !tail.starts_with('/'))
                }))
        {
            return Err(Error::InvalidInput(
                "directory output requires assets beneath its entry point".into(),
            ));
        }
        if !directory && !paths.contains(primary.as_str()) {
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
        Ok(Self {
            primary,
            assets,
            directory,
        })
    }

    /// Construct a one-file bundle.
    pub fn single(path: ResourcePath, bytes: Vec<u8>) -> Self {
        Self {
            primary: path.clone(),
            assets: vec![OutputAsset { path, bytes }],
            directory: false,
        }
    }

    /// Whether the entry point is a directory rather than one encoded file.
    pub fn is_directory(&self) -> bool {
        self.directory
    }

    /// Entry-point path to give the consuming application.
    pub fn primary(&self) -> &ResourcePath {
        &self.primary
    }

    /// Assets sorted by path for deterministic traversal.
    pub fn assets(&self) -> &[OutputAsset] {
        &self.assets
    }

    /// Count this bundle's owned storage, including spare vector/string capacity.
    ///
    /// Includes the inline bundle, entry-point string, asset vector and each
    /// asset's path and byte buffer. Allocator bookkeeping and shared external
    /// storage are outside this owned-storage estimate. No allocation occurs.
    pub fn retained_bytes(&self, max_bytes: usize) -> Result<usize> {
        fn add(total: &mut usize, amount: usize, limit: usize) -> Result<()> {
            *total = total.checked_add(amount).ok_or(Error::ResourceLimit {
                resource: "retained output bundle bytes", limit,
            })?;
            if *total > limit {
                return Err(Error::ResourceLimit {
                    resource: "retained output bundle bytes", limit,
                });
            }
            Ok(())
        }
        let mut total = 0;
        add(&mut total, std::mem::size_of::<Self>(), max_bytes)?;
        add(&mut total, self.primary.0.capacity(), max_bytes)?;
        let slots = self.assets.capacity()
            .checked_mul(std::mem::size_of::<OutputAsset>())
            .ok_or(Error::ResourceLimit {
                resource: "retained output bundle bytes", limit: max_bytes,
            })?;
        add(&mut total, slots, max_bytes)?;
        for asset in &self.assets {
            add(&mut total, asset.path.0.capacity(), max_bytes)?;
            add(&mut total, asset.bytes.capacity(), max_bytes)?;
        }
        Ok(total)
    }

    /// Move the assets to a persistence layer after strict validation succeeds.
    pub fn into_assets(self) -> Vec<OutputAsset> {
        self.assets
    }

    /// Single-file convenience API; never silently discard sidecars.
    pub fn into_single(self) -> Result<Vec<u8>> {
        if self.directory {
            return Err(Error::InvalidInput(
                "directory output cannot be flattened into a single file".into(),
            ));
        }
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

/// Decode a gzip envelope independently of font format detection. Concatenated
/// members are supported; CRC/trailer corruption and trailing junk fail. Nested
/// compression is not recursively expanded. The output cap is enforced in chunks.
fn decode_transport(bytes: &[u8], limits: &ResourceLimits) -> Result<Option<Vec<u8>>> {
    if !bytes.starts_with(&[0x1f, 0x8b]) {
        return Ok(None);
    }
    #[cfg(not(feature = "gzip"))]
    {
        let _ = limits;
        Err(Error::FeatureDisabled {
            format: "gzip".into(),
        })
    }
    #[cfg(feature = "gzip")]
    {
        use std::io::Read;
        let maximum = limits
            .max_decompressed_bytes
            .min(limits.max_total_bytes.saturating_sub(bytes.len()));
        let mut decoder = flate2::read::MultiGzDecoder::new(bytes);
        let mut decoded = Vec::new();
        let mut chunk = [0_u8; 8192];
        loop {
            let budget = maximum
                .saturating_sub(decoded.len())
                .saturating_add(1)
                .min(chunk.len());
            let count = decoder
                .read(&mut chunk[..budget])
                .map_err(|e| Error::InvalidInput(format!("invalid gzip input: {e}")))?;
            if count == 0 {
                break;
            }
            if decoded.len().saturating_add(count) > maximum {
                return Err(Error::ResourceLimit {
                    resource: "decompressed bytes",
                    limit: maximum,
                });
            }
            let required = decoded.len() + count;
            if required > decoded.capacity() {
                // Bound geometric growth as well as logical decoded length.
                // Exact chunk-by-chunk growth would make large inputs quadratic.
                let capacity = required
                    .max(decoded.capacity().saturating_mul(2))
                    .min(maximum);
                decoded
                    .try_reserve_exact(capacity - decoded.len())
                    .map_err(|_| Error::ResourceLimit {
                        resource: "decompression allocation",
                        limit: maximum,
                    })?;
            }
            decoded.extend_from_slice(&chunk[..count]);
        }
        if decoded.starts_with(&[0x1f, 0x8b]) {
            return Err(Error::InvalidInput(
                "nested gzip envelopes are not supported".into(),
            ));
        }
        Ok(Some(decoded))
    }
}

/// Whether a bundle requests the shared single-file gzip envelope.
pub(crate) fn uses_gzip(bundle: &OutputBundle) -> bool {
    !bundle.is_directory()
        && bundle
            .primary()
            .as_str()
            .rsplit_once('.')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("gz"))
}

/// Capacity retained by the merged report during compression and native audit.
pub(crate) fn report_bytes(report: &Report, limit: usize) -> Result<usize> {
    let mut total = report
        .entries
        .capacity()
        .checked_mul(std::mem::size_of::<crate::report::ReportEntry>())
        .ok_or(Error::ResourceLimit {
            resource: "gzip retained diagnostics",
            limit,
        })?;
    for entry in &report.entries {
        for size in [
            entry.code.capacity(),
            entry.message.capacity(),
            entry.location.asset.as_ref().map_or(0, String::capacity),
        ] {
            total = total.checked_add(size).ok_or(Error::ResourceLimit {
                resource: "gzip retained diagnostics",
                limit,
            })?;
        }
    }
    if total > limit {
        return Err(Error::ResourceLimit {
            resource: "gzip retained diagnostics",
            limit,
        });
    }
    Ok(total)
}

/// Apply deterministic gzip after strict validation. Native source bytes are
/// released on return; the caller then audits its native profile while retaining
/// only this compressed result and the already combined diagnostics.
pub(crate) fn encode_transport(
    bundle: OutputBundle,
    limits: &TransportLimits,
    retained_report: usize,
) -> Result<OutputBundle> {
    if !uses_gzip(&bundle) {
        return Ok(bundle);
    }
    #[cfg(not(feature = "gzip"))]
    {
        let _ = (bundle, limits, retained_report);
        Err(Error::FeatureDisabled {
            format: "gzip".into(),
        })
    }
    #[cfg(feature = "gzip")]
    {
        use std::io::Write;
        let path = bundle.primary().clone();
        let bytes = bundle.into_single()?;
        let resources = &limits.resources;
        if bytes.len() > resources.max_decompressed_bytes {
            return Err(Error::ResourceLimit {
                resource: "gzip decompressed bytes",
                limit: resources.max_decompressed_bytes,
            });
        }
        if bytes.starts_with(&[0x1f, 0x8b]) {
            return Err(Error::InvalidInput(
                "nested gzip envelopes are not supported".into(),
            ));
        }
        let available = resources
            .max_total_bytes
            .checked_sub(retained_report)
            .and_then(|n| n.checked_sub(bytes.capacity()))
            .ok_or(Error::ResourceLimit {
                resource: "gzip retained bytes",
                limit: resources.max_total_bytes,
            })?;
        let maximum = limits
            .max_output_bytes
            .min(resources.max_input_bytes)
            .min(available);
        let sink = GzipOutput {
            bytes: Vec::new(),
            maximum,
        };
        let mut encoder = flate2::GzBuilder::new()
            .mtime(0)
            .operating_system(255)
            .write(sink, flate2::Compression::default());
        encoder.write_all(&bytes).map_err(gzip_error)?;
        let output = encoder.finish().map_err(gzip_error)?.bytes;
        // `bytes` drops before the registry runs native validation again.
        Ok(OutputBundle::single(path, output))
    }
}

#[cfg(feature = "gzip")]
#[derive(Debug)]
struct GzipBudgetError {
    resource: &'static str,
    limit: usize,
}
#[cfg(feature = "gzip")]
impl fmt::Display for GzipBudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} limit {}", self.resource, self.limit)
    }
}
#[cfg(feature = "gzip")]
impl std::error::Error for GzipBudgetError {}
#[cfg(feature = "gzip")]
fn gzip_error(error: std::io::Error) -> Error {
    if let Some(budget) = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<GzipBudgetError>())
    {
        Error::ResourceLimit {
            resource: budget.resource,
            limit: budget.limit,
        }
    } else {
        Error::Io(error)
    }
}
#[cfg(feature = "gzip")]
struct GzipOutput {
    bytes: Vec<u8>,
    maximum: usize,
}
#[cfg(feature = "gzip")]
impl std::io::Write for GzipOutput {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        if input.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(GzipBudgetError {
                resource: "gzip output bytes",
                limit: self.maximum,
            }));
        }
        let required = self.bytes.len() + input.len();
        if required > self.bytes.capacity() {
            // Geometric growth remains capped by the already reserved byte
            // budget; exact growth per compressor chunk would be quadratic.
            let capacity = required
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.maximum);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| {
                    std::io::Error::other(GzipBudgetError {
                        resource: "gzip allocation",
                        limit: self.maximum,
                    })
                })?;
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
