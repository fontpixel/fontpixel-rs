//! Typed, callable fontpixel operations shared by the command-line tool and the
//! language bindings.
//!
//! Every command-line capability is a typed request and a function returning a
//! typed response: [`convert()`], [`batch::run`], [`merge()`], [`variants`],
//! [`subset`], [`charset`], [`validate`], [`diff`], [`info`], [`preview`],
//! [`render`] and [`formats`]. Option groups mirror the command-line argument
//! groups field for field (see [`options`] and the format option modules).
//! [`merge_with_progress`] and [`convert_with_progress`] add a progress
//! observer that is called at step boundaries and may cancel the operation
//! ([`Progress`], `fontpixel::Error::Cancelled`).
//!
//! Operations run against a [`Storage`]:
//!
//! - [`Storage::Memory`] (always available, including `wasm32`): request paths
//!   are keys into a caller-supplied [`FileProvider`] such as [`MemoryFiles`];
//!   sidecars resolve beside their primary key; outputs are validated as one
//!   group and returned in [`Completed::outputs`]. Nothing touches the filesystem.
//! - `Storage::Filesystem` (feature `fs`): real paths, sidecar discovery,
//!   source-alias protection and the transactional multi-output persistence of
//!   the command-line tool.
//!
//! Resource limits are request fields. A standalone call partitions its total
//! byte limit into a fixed source-identity slice and the working allowance
//! exactly as the command line does, in both storage modes.
//!
//! Responses render the command-line documents with `write_json` / `write_text`
//! (pretty JSON for standalone commands, compact JSON for batch tasks).
//!
//! Serialized requests (feature `serde`) are flat maps: every option group is
//! flattened, so the keys are the command-line argument ids (the long option
//! names in snake_case, e.g. `max_total_bytes`, `otb_upm`), and absent keys
//! take the command-line defaults. Merge parts and outputs are lists of
//! `{input, take, dy, from, font_index}` and `{destination, format}` maps; a
//! batch task is a request map with a `kind` tag (`convert`, `vectorize`,
//! `variants`, `subset`, `merge`, `charset`, `validate`, `diff`). Colors and
//! writing directions use the core shapes (`{red, green, blue, alpha}`,
//! `{glyphs, lines}`), merge selections their expression string. Deserialization
//! is strict: unknown keys are rejected at every level, and the type's
//! `validate()` runs, so choice lists, numeric ranges, value syntax and the
//! relational command-line rules (`requires`, `conflicts_with`, required
//! arguments) fail with the parser's messages. Choices whose core enum has the
//! command-line spelling are typed (`shape`, merge `spacing` and
//! `geometric_width`); the others are strings checked against the same lists.
//!
//! Features: per-format features forward to `fontpixel`; `fs` enables the
//! filesystem mode; `clap` derives the command-line parser on the request types
//! ([`cli`]); `toml` adds the batch job-file reader ([`batch::toml`]); `serde`
//! derives `Serialize`/`Deserialize` for every request and option group.
#![forbid(unsafe_code)]

#[cfg(feature = "fs")]
mod assets;
pub mod batch;
#[cfg(feature = "clap")]
pub mod cli;
mod commands;
mod context;
pub mod convert;
pub mod extra_formats;
pub mod image_formats;
pub mod input_budget;
mod inspect;
pub mod json;
pub mod memory;
pub mod merge;
pub mod metadata;
mod name_records;
#[cfg(feature = "opentype")]
mod opentype;
pub mod options;
#[cfg(feature = "opentype")]
pub mod outline_metadata;
pub mod p2_formats;
pub mod p3_formats;
mod render;
#[cfg(feature = "serde")]
mod serde_keys;

pub use batch::{BatchRequest, BatchResponse, BatchTask, TaskAction, TaskKind, TaskRequest};
pub use commands::{
    CharsetRequest, CharsetResponse, DiffRequest, DiffResponse, Difference, FaceValidation,
    ValidateRequest, ValidateResponse, ValidationKind,
};
pub use context::PreparedOutput;
pub use convert::{ConvertRequest, ConvertResponse};
pub use inspect::{FormatsRequest, FormatsResponse, InfoRequest, InfoResponse};
pub use memory::{EntryKind, FileProvider, MemoryFiles, Storage};
pub use merge::{MergeInput, MergeOutput, MergeRequest, MergeResponse, MergeSettings};
#[cfg(feature = "opentype")]
pub use opentype::{
    ResourceOptions, SfntOutputOptions, SfntResponse, SubsetRequest, VariantsRequest,
};
pub use options::ReportStyle;
pub use render::{PreviewRequest, PreviewResponse, RenderRequest, RenderResponse};

use fontpixel::{Error, Result};
use std::ops::ControlFlow;

/// One completed step of a long operation, passed to the progress observer of
/// [`merge_with_progress`] and [`convert_with_progress`].
///
/// A merge reports one `read` per part, one `merge`, then one `encode` per
/// output. A convert reports, for each output in turn, one `read` per input
/// of that output followed by its `encode` (with `each`, reads and encodes
/// alternate). `encode` means encoded and prepared, not yet committed: the
/// output group is committed only after the last event, so a cancellation
/// never leaves an output committed or returned. Serialized as a plain map
/// `{step, done, total, item, glyphs, bytes}` with `step` in snake_case.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    /// The step that has just completed.
    pub step: ProgressStep,
    /// Completed items of this step so far, starting at 1.
    pub done: usize,
    /// Items this step has in total.
    pub total: usize,
    /// The input path (`read`) or output destination (`encode`); `None` for
    /// `merge`. Non-UTF-8 path bytes are replaced.
    pub item: Option<String>,
    /// Glyphs decoded from the input or selected from the part (`read`), or
    /// glyphs in the merged font (`merge`).
    pub glyphs: Option<usize>,
    /// Bytes of every asset of the encoded output (`encode`).
    pub bytes: Option<usize>,
}
/// Step kinds reported in [`Progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStep {
    /// One input (convert) or merge part was read and admitted.
    Read,
    /// The merge plan was applied and the merged font is complete.
    Merge,
    /// One output was encoded; the group is committed after the last one.
    Encode,
}
/// The caller's observer inside an operation; a no-op for the plain entry
/// points, so their results cannot depend on it.
type ObserverFn<'a> = dyn FnMut(&Progress) -> ControlFlow<()> + 'a;
pub(crate) struct Observer<'a>(Option<&'a mut ObserverFn<'a>>);
impl<'a> Observer<'a> {
    pub(crate) fn none() -> Self {
        Self(None)
    }
    fn new(observer: &'a mut dyn FnMut(&Progress) -> ControlFlow<()>) -> Self {
        Self(Some(observer))
    }
    /// Report a completed step (built only when someone observes it).
    /// `Break` from the observer becomes [`Error::Cancelled`].
    pub(crate) fn report(&mut self, progress: impl FnOnce() -> Progress) -> Result<()> {
        let Some(observer) = self.0.as_mut() else {
            return Ok(());
        };
        match observer(&progress()) {
            ControlFlow::Continue(()) => Ok(()),
            ControlFlow::Break(()) => Err(Error::Cancelled),
        }
    }
}

/// A response together with the outputs it prepared. With memory storage the
/// outputs are returned here; with filesystem storage they were already
/// committed and this list is empty.
#[derive(Debug, Clone)]
pub struct Completed<T> {
    /// The typed operation result.
    pub response: T,
    /// Prepared outputs (memory storage only).
    pub outputs: Vec<PreparedOutput>,
}

/// Platform capabilities of this build, for explicit feature queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Filesystem storage is compiled in (`fs` feature).
    pub filesystem: bool,
    /// Batch tasks can run on parallel worker threads. Without threads a
    /// batch whose admission would use more than one worker fails with an
    /// `Unsupported` I/O error instead of silently running serially.
    pub threads: bool,
    /// The command-line parser and the batch TOML reader are compiled in.
    pub toml_batch: bool,
}

/// Capabilities of this build and target.
pub fn capabilities() -> Capabilities {
    Capabilities {
        filesystem: cfg!(feature = "fs"),
        threads: !cfg!(target_family = "wasm"),
        toml_batch: cfg!(feature = "toml"),
    }
}

fn standalone<T>(
    storage: &Storage,
    total: usize,
    f: impl FnOnce() -> Result<T>,
) -> Result<Completed<T>> {
    let (response, outputs) = context::with_bounded_source_scope(storage, total, f)?;
    Ok(Completed { response, outputs })
}

/// Convert inputs into one output, or each input into its own output.
pub fn convert(request: ConvertRequest, storage: &Storage) -> Result<Completed<ConvertResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || {
        convert::run(request, &mut Observer::none())
    })
}
/// [`convert()`] reporting each completed step to `progress`: for each output,
/// one [`ProgressStep::Read`] per input it combines, then its
/// [`ProgressStep::Encode`] (see [`Progress`]). The observer runs on the calling thread between steps (never inside
/// a codec). Returning [`ControlFlow::Break`] cancels the operation at that
/// boundary with [`Error::Cancelled`]; nothing is committed or returned.
/// Results are identical to [`convert()`]'s whenever the observer continues.
pub fn convert_with_progress(
    request: ConvertRequest,
    storage: &Storage,
    progress: &mut dyn FnMut(&Progress) -> ControlFlow<()>,
) -> Result<Completed<ConvertResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || {
        convert::run(request, &mut Observer::new(progress))
    })
}
/// Merge ordered parts and write every requested output as one group.
pub fn merge(request: MergeRequest, storage: &Storage) -> Result<Completed<MergeResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || {
        merge::run(request, &mut Observer::none())
    })
}
/// [`merge()`] reporting each completed step to `progress`: one
/// [`ProgressStep::Read`] per part, one [`ProgressStep::Merge`], then one
/// [`ProgressStep::Encode`] per output. The observer runs on the calling
/// thread between steps (never inside a codec), so a long merge in a Web
/// Worker can post progress and poll a cancel flag. Returning
/// [`ControlFlow::Break`] cancels at that boundary with [`Error::Cancelled`];
/// no output is committed or returned. Results are identical to
/// [`merge()`]'s whenever the observer continues.
pub fn merge_with_progress(
    request: MergeRequest,
    storage: &Storage,
    progress: &mut dyn FnMut(&Progress) -> ControlFlow<()>,
) -> Result<Completed<MergeResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || {
        merge::run(request, &mut Observer::new(progress))
    })
}
/// Build a locl-enabled OpenType font from regional and script faces.
#[cfg(feature = "opentype")]
pub fn variants(request: VariantsRequest, storage: &Storage) -> Result<Completed<SfntResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || opentype::variants(request))
}
/// Subset an sfnt, WOFF or WOFF2 font.
#[cfg(feature = "opentype")]
pub fn subset(request: SubsetRequest, storage: &Storage) -> Result<Completed<SfntResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || opentype::subset(request))
}
/// Remap glyph codes into another charset and write the result.
pub fn charset(request: CharsetRequest, storage: &Storage) -> Result<Completed<CharsetResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || commands::charset(request))
}
/// Validate one input.
pub fn validate(request: ValidateRequest, storage: &Storage) -> Result<ValidateResponse> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    Ok(standalone(storage, total, || commands::validate(request))?.response)
}
/// Compare two inputs. Differences are reported in the response, not as an error.
pub fn diff(request: DiffRequest, storage: &Storage) -> Result<DiffResponse> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    Ok(standalone(storage, total, || commands::diff(request))?.response)
}
/// Decode one input and describe its fonts.
pub fn info(request: InfoRequest, storage: &Storage) -> Result<InfoResponse> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    Ok(standalone(storage, total, || inspect::info(request))?.response)
}
/// Lay out text (or every mapping) as character art.
pub fn preview(request: PreviewRequest, storage: &Storage) -> Result<PreviewResponse> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    Ok(standalone(storage, total, || render::preview(request))?.response)
}
/// Lay out text (or every mapping) into a PNG or SVG image.
pub fn render(request: RenderRequest, storage: &Storage) -> Result<Completed<RenderResponse>> {
    request.validate()?;
    let total = request.source_identity_total_bytes();
    standalone(storage, total, || render::render(request))
}
/// List every catalogue format with the reader/writer actually compiled in.
pub fn formats(request: &FormatsRequest) -> FormatsResponse {
    inspect::formats(request)
}
