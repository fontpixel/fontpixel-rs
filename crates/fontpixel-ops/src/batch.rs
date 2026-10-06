//! Typed parallel jobs, bounded reports and transactional output preparation.
//!
//! A [`BatchRequest`] holds independent typed tasks. Each runs under the shared
//! working ledger, its JSON result is bounded and (in filesystem storage) large
//! results are spooled to private temporary storage, and every successful
//! writing task joins one rollback-capable output transaction. Memory storage
//! returns the committed outputs instead and never spools. Without threads
//! (`wasm32`), a batch that would need more than one worker fails explicitly.
use crate::context::{CAPTURE, Capture, CaptureGuard, PreparedOutput, Sources};
use crate::memory::{self, Storage};
use crate::{
    CharsetRequest, ConvertRequest, DiffRequest, MergeRequest, ValidateRequest,
    options::ReportStyle,
};
#[cfg(feature = "opentype")]
use crate::{SubsetRequest, VariantsRequest};
use fontpixel::{Error, Result};
use serde::Serialize;
#[cfg(feature = "fs")]
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, VecDeque},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[cfg(feature = "fs")]
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    sync::atomic::AtomicU64,
};
#[cfg(feature = "toml")]
pub mod toml;

/// Task kind as reported in the batch document.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(any(feature = "serde", feature = "toml"), derive(serde::Deserialize))]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// `convert`.
    Convert,
    /// `vectorize` (an alias of convert).
    Vectorize,
    /// `variants`.
    Variants,
    /// `subset`.
    Subset,
    /// `merge`.
    Merge,
    /// `charset`.
    Charset,
    /// `validate` (read-only).
    Validate,
    /// `diff` (read-only).
    Diff,
}
impl TaskKind {
    /// The command name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Convert => "convert",
            Self::Vectorize => "vectorize",
            Self::Variants => "variants",
            Self::Subset => "subset",
            Self::Merge => "merge",
            Self::Charset => "charset",
            Self::Validate => "validate",
            Self::Diff => "diff",
        }
    }
}
/// One typed task operation. Its result is always rendered as compact JSON.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "kind", rename_all = "lowercase")
)]
pub enum TaskRequest {
    /// Convert.
    Convert(Box<ConvertRequest>),
    /// Vectorize: the same operation as convert, reported as `vectorize`.
    Vectorize(Box<ConvertRequest>),
    /// Regional OpenType variants.
    #[cfg(feature = "opentype")]
    Variants(Box<VariantsRequest>),
    /// sfnt subsetting.
    #[cfg(feature = "opentype")]
    Subset(Box<SubsetRequest>),
    /// Multi-part merge.
    Merge(Box<MergeRequest>),
    /// Charset remapping.
    Charset(Box<CharsetRequest>),
    /// Validation.
    Validate(Box<ValidateRequest>),
    /// Comparison.
    Diff(Box<DiffRequest>),
}
impl TaskRequest {
    /// The task kind reported in the batch document.
    pub fn kind(&self) -> TaskKind {
        match self {
            Self::Convert(_) => TaskKind::Convert,
            Self::Vectorize(_) => TaskKind::Vectorize,
            #[cfg(feature = "opentype")]
            Self::Variants(_) => TaskKind::Variants,
            #[cfg(feature = "opentype")]
            Self::Subset(_) => TaskKind::Subset,
            Self::Merge(_) => TaskKind::Merge,
            Self::Charset(_) => TaskKind::Charset,
            Self::Validate(_) => TaskKind::Validate,
            Self::Diff(_) => TaskKind::Diff,
        }
    }
    /// The task's original total byte limit, its working-ledger weight.
    pub fn weight(&self) -> usize {
        match self {
            Self::Convert(r) | Self::Vectorize(r) => r.source_identity_total_bytes(),
            #[cfg(feature = "opentype")]
            Self::Variants(r) => r.source_identity_total_bytes(),
            #[cfg(feature = "opentype")]
            Self::Subset(r) => r.source_identity_total_bytes(),
            Self::Merge(r) => r.source_identity_total_bytes(),
            Self::Charset(r) => r.source_identity_total_bytes(),
            Self::Validate(r) => r.source_identity_total_bytes(),
            Self::Diff(r) => r.source_identity_total_bytes(),
        }
    }
    /// Every input path the task reads, protected from all batch outputs.
    pub fn declared_inputs(&self) -> Vec<PathBuf> {
        match self {
            Self::Convert(r) | Self::Vectorize(r) => r.inputs.clone(),
            #[cfg(feature = "opentype")]
            Self::Variants(r) => {
                let mut paths = vec![r.base.clone()];
                for value in r.languages.iter().chain(&r.scripts) {
                    if let Some((_, path)) = value.split_once('=') {
                        paths.push(PathBuf::from(path));
                    }
                }
                paths
            }
            #[cfg(feature = "opentype")]
            Self::Subset(r) => std::iter::once(r.input.clone())
                .chain(r.text_file.clone())
                .collect(),
            Self::Merge(r) => r.parts.iter().map(|part| part.input.clone()).collect(),
            Self::Charset(r) => vec![r.input.clone()],
            Self::Validate(r) => vec![r.input.clone()],
            Self::Diff(r) => vec![r.a.clone(), r.b.clone()],
        }
    }
    /// Check the value constraints of the wrapped request.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Convert(r) | Self::Vectorize(r) => r.validate(),
            #[cfg(feature = "opentype")]
            Self::Variants(r) => r.validate(),
            #[cfg(feature = "opentype")]
            Self::Subset(r) => r.validate(),
            Self::Merge(r) => r.validate(),
            Self::Charset(r) => r.validate(),
            Self::Validate(r) => r.validate(),
            Self::Diff(r) => r.validate(),
        }
    }
    /// Force machine-readable output, as every batch task reports JSON.
    fn into_json(mut self) -> Self {
        match &mut self {
            Self::Convert(r) | Self::Vectorize(r) => r.report = ReportStyle::Json,
            #[cfg(feature = "opentype")]
            Self::Variants(r) => r.output.report = ReportStyle::Json,
            #[cfg(feature = "opentype")]
            Self::Subset(r) => r.output.report = ReportStyle::Json,
            Self::Merge(r) => r.settings.report = ReportStyle::Json,
            Self::Charset(r) => r.report = ReportStyle::Json,
            Self::Validate(r) => r.json = true,
            Self::Diff(r) => r.json = true,
        }
        self
    }
    /// Run the operation in the active batch capture and write its compact
    /// JSON result. A diff with differences writes its result, then fails.
    fn execute(self, out: &mut dyn Write) -> Result<()> {
        self.validate()?;
        match self {
            Self::Convert(r) | Self::Vectorize(r) => {
                crate::convert::run(*r)?.write_json(out, false)
            }
            #[cfg(feature = "opentype")]
            Self::Variants(r) => {
                crate::opentype::variants(*r)?.write(ReportStyle::Json, out, false)
            }
            #[cfg(feature = "opentype")]
            Self::Subset(r) => crate::opentype::subset(*r)?.write(ReportStyle::Json, out, false),
            Self::Merge(r) => crate::merge::run(*r)?.write_json(out, false),
            Self::Charset(r) => crate::commands::charset(*r)?.write_json(out, false),
            Self::Validate(r) => crate::commands::validate(*r)?.write_json(out, false),
            Self::Diff(r) => {
                let response = crate::commands::diff(*r)?;
                response.write_json(out, false)?;
                response.status()
            }
        }
    }
}
/// What a task runs: a typed request, or a preparation error that fails the
/// task with exit code 2 without affecting other tasks.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskAction {
    /// Run this request; `weight` bytes are reserved from the working ledger.
    Run {
        /// The operation.
        request: TaskRequest,
        /// Working-ledger reservation (normally the task's total byte limit).
        weight: usize,
    },
    /// The task could not be prepared; reported with exit code 2.
    Invalid(String),
}
/// One identified batch task.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchTask {
    /// Unique non-empty identifier of at most 256 UTF-8 bytes.
    pub id: String,
    /// Reported kind.
    pub kind: TaskKind,
    /// Every path this task reads, including inputs of an invalid task;
    /// no batch output may alias or contain any of them.
    pub declared_inputs: Vec<PathBuf>,
    /// The operation or its preparation error.
    pub action: TaskAction,
}
impl BatchTask {
    /// A task running `request`, with its kind, weight and declared inputs
    /// derived from the request.
    /// A request that fails [`TaskRequest::validate`] becomes
    /// [`TaskAction::Invalid`], as a rejected command line would.
    pub fn new(id: impl Into<String>, request: TaskRequest) -> Self {
        let kind = request.kind();
        let declared_inputs = request.declared_inputs();
        let action = match request.validate() {
            Ok(()) => TaskAction::Run {
                weight: request.weight(),
                request,
            },
            Err(error) => TaskAction::Invalid(error.to_string()),
        };
        Self {
            id: id.into(),
            kind,
            declared_inputs,
            action,
        }
    }
}
/// Default aggregate working reservation (1 GiB).
pub const DEFAULT_MAX_WORKING_BYTES: usize = 1024 * 1024 * 1024;
/// Default retained transaction state (512 MiB).
pub const DEFAULT_MAX_PENDING_BYTES: usize = 512 * 1024 * 1024;
/// Default aggregate report bytes (2 GiB).
pub const DEFAULT_MAX_REPORT_BYTES: usize = 2 * 1024 * 1024 * 1024;
/// Default per-task report bytes (64 MiB).
pub const DEFAULT_MAX_TASK_REPORT_BYTES: usize = 64 * 1024 * 1024;
/// A complete batch: ordered tasks, worker ceiling and budgets.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchRequest {
    /// Requested worker ceiling (1..64); defaults to available CPUs capped at 32.
    pub jobs: Option<usize>,
    /// Aggregate working reservation for concurrently running tasks.
    pub max_working_bytes: usize,
    /// Retained transaction state: prepared outputs and source identities.
    pub max_pending_bytes: usize,
    /// Aggregate raw task result/report bytes plus metadata.
    pub max_report_bytes: usize,
    /// Result plus diagnostics bytes of one task.
    pub max_task_report_bytes: usize,
    /// Paths protected before any task input (for example the job file).
    pub protected_paths: Vec<PathBuf>,
    /// Tasks in report order.
    pub tasks: Vec<BatchTask>,
}
impl Default for BatchRequest {
    fn default() -> Self {
        Self {
            jobs: None,
            max_working_bytes: DEFAULT_MAX_WORKING_BYTES,
            max_pending_bytes: DEFAULT_MAX_PENDING_BYTES,
            max_report_bytes: DEFAULT_MAX_REPORT_BYTES,
            max_task_report_bytes: DEFAULT_MAX_TASK_REPORT_BYTES,
            protected_paths: Vec::new(),
            tasks: Vec::new(),
        }
    }
}
/// Outcome of one task, without its result payloads (those are in the document).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    /// Position in the request.
    pub index: usize,
    /// Task identifier.
    pub id: String,
    /// Reported kind.
    pub kind: TaskKind,
    /// Whether the task succeeded (and, for a writer, committed).
    pub success: bool,
    /// Command-line exit code of the task.
    pub exit_code: u8,
    /// Whether the task's outputs were committed.
    pub committed: bool,
    /// Whether results or diagnostics were dropped or truncated.
    pub diagnostics_truncated: bool,
    /// Error message, if the task failed.
    pub error: Option<String>,
}
/// A completed batch. The full JSON document was written to the output stream.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchResponse {
    /// Aggregate exit code (precedence 2 > 1 > 3 > 0).
    pub exit_code: u8,
    /// Requested worker ceiling, capped by the task count.
    pub jobs: usize,
    /// Worker threads actually used.
    pub workers: usize,
    /// Per-task outcomes in request order.
    pub tasks: Vec<TaskSummary>,
    /// Committed outputs (memory storage only; filesystem outputs are written).
    pub outputs: Vec<PreparedOutput>,
}
fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
struct Working {
    used: Mutex<usize>,
    ready: Condvar,
    limit: usize,
}
struct Reservation<'a> {
    owner: &'a Working,
    bytes: usize,
}
impl Working {
    fn reserve(&self, bytes: usize) -> Result<Reservation<'_>> {
        if bytes == 0 || bytes > self.limit {
            return Err(invalid("invalid task working reservation"));
        }
        let mut used = self
            .used
            .lock()
            .map_err(|_| invalid("working budget poisoned"))?;
        while *used > self.limit - bytes {
            used = self
                .ready
                .wait(used)
                .map_err(|_| invalid("working budget poisoned"))?;
        }
        *used += bytes;
        Ok(Reservation { owner: self, bytes })
    }
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if let Ok(mut used) = self.owner.used.lock() {
            *used = used.saturating_sub(self.bytes);
            self.owner.ready.notify_all();
        }
    }
}
struct LimitedOutput {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for LimitedOutput {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(data.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(io::Error::other("batch task report exceeds byte limit"));
        }
        self.bytes
            .try_reserve(data.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg_attr(test, derive(Serialize))]
struct TaskReport {
    index: usize,
    id: String,
    kind: TaskKind,
    success: bool,
    exit_code: u8,
    committed: bool,
    result: Option<ReportPayload>,
    report: Option<ReportPayload>,
    diagnostics_truncated: bool,
    error: Option<String>,
}
impl TaskReport {
    fn new(index: usize, task: &BatchTask) -> Self {
        Self {
            index,
            id: task.id.clone(),
            kind: task.kind,
            success: false,
            exit_code: 0,
            committed: false,
            result: None,
            report: None,
            diagnostics_truncated: false,
            error: None,
        }
    }
}
fn bound_error(report: &mut TaskReport) {
    if let Some(error) = &mut report.error
        && error.len() > 1024
    {
        let mut end = 1024;
        while !error.is_char_boundary(end) {
            end -= 1;
        }
        error.truncate(end);
        report.diagnostics_truncated = true;
    }
}
#[derive(Serialize)]
struct Summary {
    total: usize,
    succeeded: usize,
    failed: usize,
    committed: usize,
}
#[cfg_attr(test, derive(Serialize))]
struct Document<'a> {
    version: u32,
    success: bool,
    exit_code: u8,
    jobs: usize,
    workers: usize,
    summary: Summary,
    tasks: &'a [TaskReport],
}
fn document(tasks: &[TaskReport], jobs: usize, workers: usize) -> Document<'_> {
    let exit_code = tasks
        .iter()
        .fold(0, |code, t| precedence(code, t.exit_code));
    let succeeded = tasks.iter().filter(|t| t.success).count();
    Document {
        version: 1,
        success: exit_code == 0,
        exit_code,
        jobs: jobs.min(tasks.len()),
        workers,
        summary: Summary {
            total: tasks.len(),
            succeeded,
            failed: tasks.len() - succeeded,
            committed: tasks.iter().filter(|t| t.committed).count(),
        },
        tasks,
    }
}
struct IndentedMetadata<'a> {
    output: &'a mut dyn Write,
    indent: &'static [u8],
    continuation: bool,
}
impl Write for IndentedMetadata<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for part in bytes.split_inclusive(|byte| *byte == b'\n') {
            if self.continuation {
                self.output.write_all(self.indent)?;
            }
            self.output.write_all(part)?;
            self.continuation = part.last() == Some(&b'\n');
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
fn field_prefix(
    output: &mut dyn Write,
    first: bool,
    indent: &'static [u8],
    name: &str,
) -> io::Result<()> {
    output.write_all(if first { b"\n" } else { b",\n" })?;
    output.write_all(indent)?;
    serde_json::to_writer(&mut *output, name).map_err(io::Error::other)?;
    output.write_all(b": ")
}
fn metadata_field<T: Serialize + ?Sized>(
    output: &mut dyn Write,
    first: bool,
    indent: &'static [u8],
    name: &str,
    value: &T,
) -> io::Result<()> {
    field_prefix(output, first, indent, name)?;
    serde_json::to_writer_pretty(
        IndentedMetadata {
            output,
            indent,
            continuation: false,
        },
        value,
    )
    .map_err(io::Error::other)
}
fn payload_field(
    output: &mut dyn Write,
    name: &str,
    payload: Option<&ReportPayload>,
) -> io::Result<()> {
    field_prefix(output, false, b"      ", name)?;
    match payload {
        Some(payload) => payload.write(output),
        None => output.write_all(b"null"),
    }
}
fn write_document(output: &mut dyn Write, doc: &Document<'_>) -> io::Result<()> {
    output.write_all(b"{")?;
    metadata_field(output, true, b"  ", "version", &doc.version)?;
    metadata_field(output, false, b"  ", "success", &doc.success)?;
    metadata_field(output, false, b"  ", "exit_code", &doc.exit_code)?;
    metadata_field(output, false, b"  ", "jobs", &doc.jobs)?;
    metadata_field(output, false, b"  ", "workers", &doc.workers)?;
    metadata_field(output, false, b"  ", "summary", &doc.summary)?;
    field_prefix(output, false, b"  ", "tasks")?;
    output.write_all(b"[")?;
    for (index, task) in doc.tasks.iter().enumerate() {
        output.write_all(if index == 0 { b"\n    {" } else { b",\n    {" })?;
        metadata_field(output, true, b"      ", "index", &task.index)?;
        metadata_field(output, false, b"      ", "id", &task.id)?;
        metadata_field(output, false, b"      ", "kind", &task.kind)?;
        metadata_field(output, false, b"      ", "success", &task.success)?;
        metadata_field(output, false, b"      ", "exit_code", &task.exit_code)?;
        metadata_field(output, false, b"      ", "committed", &task.committed)?;
        payload_field(output, "result", task.result.as_ref())?;
        payload_field(output, "report", task.report.as_ref())?;
        metadata_field(
            output,
            false,
            b"      ",
            "diagnostics_truncated",
            &task.diagnostics_truncated,
        )?;
        metadata_field(output, false, b"      ", "error", &task.error)?;
        output.write_all(b"\n    }")?;
    }
    if !doc.tasks.is_empty() {
        output.write_all(b"\n  ")?;
    }
    output.write_all(b"]\n}")
}
struct CountOutput {
    used: usize,
    limit: usize,
    exceeded: bool,
}
impl Write for CountOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .used
            .checked_add(bytes.len())
            .filter(|v| *v <= self.limit);
        let Some(next) = next else {
            self.exceeded = true;
            return Err(io::Error::other("batch aggregate JSON exceeds byte limit"));
        };
        self.used = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn preflight_document(doc: &Document<'_>, limit: usize) -> Result<usize> {
    let mut counter = CountOutput {
        used: 0,
        limit,
        exceeded: false,
    };
    write_document(&mut counter, doc).map_err(|error| {
        if counter.exceeded {
            Error::ResourceLimit {
                resource: "batch serialized report bytes before commit",
                limit,
            }
        } else {
            Error::Io(io::Error::other(format!(
                "batch report preflight failed: {error}"
            )))
        }
    })?;
    Ok(counter.used)
}
struct ReportBudget {
    used: Mutex<usize>,
    limit: usize,
    exceeded: AtomicBool,
    storage_error: Mutex<Option<String>>,
    /// Private report storage; memory storage never spools.
    #[cfg(feature = "fs")]
    spool: Option<Arc<ReportSpool>>,
}
#[cfg(feature = "fs")]
const SPOOL_THRESHOLD: usize = 1024 * 1024;
#[cfg(feature = "fs")]
struct ReportSpool {
    state: Mutex<Option<SpoolFile>>,
    root: PathBuf,
    protected: Arc<BTreeSet<PathBuf>>,
    sources: Sources,
    limit: usize,
}
#[cfg(feature = "fs")]
struct SpoolFile {
    file: Option<File>,
    directory: PathBuf,
}
#[cfg(feature = "fs")]
impl SpoolFile {
    fn create(root: &Path, protected: &BTreeSet<PathBuf>, sources: &Sources) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let directory = root.join(format!(
                ".fontpixel-batch-reports-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let identity = identity_path(&directory).map_err(io::Error::other)?;
            if overlaps_declared(&identity, protected) {
                return Err(io::Error::other(
                    "report temporary directory overlaps a declared input",
                ));
            }
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&directory) {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
                Ok(()) => {}
            }
            let mut spool = Self {
                file: None,
                directory,
            };
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            spool.file = Some(options.open(spool.directory.join("payloads.json"))?);
            sources
                .remember(spool.file.as_ref().expect("open spool"))
                .map_err(io::Error::other)?;
            return Ok(spool);
        }
        Err(io::Error::other(
            "cannot create private batch report storage",
        ))
    }
}
#[cfg(feature = "fs")]
impl Drop for SpoolFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(self.directory.join("payloads.json"));
        let _ = std::fs::remove_dir(&self.directory);
    }
}
#[cfg(feature = "fs")]
impl ReportSpool {
    fn directory(&self) -> Result<Option<PathBuf>> {
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("report storage poisoned"))?;
        state
            .as_ref()
            .map(|spool| identity_path(&spool.directory))
            .transpose()
    }
    fn append(&self, bytes: &[u8]) -> io::Result<(u64, [u8; 32])> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("report storage poisoned"))?;
        if state.is_none() {
            *state = Some(SpoolFile::create(
                &self.root,
                &self.protected,
                &self.sources,
            )?);
        }
        let file = state
            .as_mut()
            .expect("initialized spool")
            .file
            .as_mut()
            .expect("open spool");
        let offset = file.seek(SeekFrom::End(0))?;
        if offset
            .checked_add(bytes.len() as u64)
            .is_none_or(|end| end > self.limit as u64)
        {
            return Err(io::Error::other(
                "batch report storage exceeds aggregate byte limit",
            ));
        }
        let digest = Sha256::digest(bytes).into();
        file.write_all(bytes)?;
        Ok((offset, digest))
    }
    fn stream(
        &self,
        offset: u64,
        length: usize,
        expected_digest: &[u8; 32],
        output: &mut dyn Write,
    ) -> io::Result<()> {
        if offset
            .checked_add(length as u64)
            .is_none_or(|end| end > self.limit as u64)
        {
            return Err(io::Error::other("invalid batch report storage range"));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("report storage poisoned"))?;
        let file = state
            .as_mut()
            .and_then(|spool| spool.file.as_mut())
            .ok_or_else(|| io::Error::other("batch report storage is unavailable"))?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buffer = [0u8; 64 * 1024];
        let mut remaining = length;
        let mut digest = Sha256::new();
        while remaining != 0 {
            let count = remaining.min(buffer.len());
            file.read_exact(&mut buffer[..count])?;
            digest.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
            remaining -= count;
        }
        let actual_digest: [u8; 32] = digest.finalize().into();
        if &actual_digest != expected_digest {
            return Err(io::Error::other("batch report storage SHA-256 mismatch"));
        }
        Ok(())
    }
    #[cfg(test)]
    fn read(&self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        if offset
            .checked_add(length as u64)
            .is_none_or(|end| end > self.limit as u64)
        {
            return Err(io::Error::other("invalid batch report storage range"));
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).map_err(io::Error::other)?;
        bytes.resize(length, 0);
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("report storage poisoned"))?;
        let file = state
            .as_mut()
            .and_then(|spool| spool.file.as_mut())
            .ok_or_else(|| io::Error::other("batch report storage is unavailable"))?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}
enum ReportPayload {
    Inline(Box<serde_json::value::RawValue>),
    #[cfg(feature = "fs")]
    Spooled {
        spool: Arc<ReportSpool>,
        offset: u64,
        length: usize,
        digest: [u8; 32],
    },
}
impl ReportPayload {
    fn len(&self) -> usize {
        match self {
            Self::Inline(raw) => raw.get().len(),
            #[cfg(feature = "fs")]
            Self::Spooled { length, .. } => *length,
        }
    }
    #[cfg(feature = "fs")]
    fn spill(&mut self, spool: &Arc<ReportSpool>) -> io::Result<()> {
        if let Self::Inline(raw) = self
            && raw.get().len() >= SPOOL_THRESHOLD
        {
            let length = raw.get().len();
            let (offset, digest) = spool.append(raw.get().as_bytes())?;
            *self = Self::Spooled {
                spool: Arc::clone(spool),
                offset,
                length,
                digest,
            };
        }
        Ok(())
    }
    fn write(&self, output: &mut dyn Write) -> io::Result<()> {
        match self {
            Self::Inline(raw) => output.write_all(raw.get().as_bytes()),
            #[cfg(feature = "fs")]
            Self::Spooled {
                spool,
                offset,
                length,
                digest,
            } => spool.stream(*offset, *length, digest, output),
        }
    }
}
#[cfg(test)]
impl Serialize for ReportPayload {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Inline(raw) => raw.serialize(serializer),
            #[cfg(feature = "fs")]
            Self::Spooled {
                spool,
                offset,
                length,
                ..
            } => {
                let bytes = spool
                    .read(*offset, *length)
                    .map_err(serde::ser::Error::custom)?;
                let raw = captured_json(bytes)
                    .ok_or_else(|| serde::ser::Error::custom("invalid spooled task JSON"))?;
                raw.serialize(serializer)
            }
        }
    }
}
fn captured_json(mut bytes: Vec<u8>) -> Option<Box<serde_json::value::RawValue>> {
    while bytes
        .last()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .ok()
        .and_then(|text| serde_json::value::RawValue::from_string(text).ok())
}
fn worker_count(jobs: usize, limit: usize, queued: usize, minimum_weight: Option<usize>) -> usize {
    minimum_weight
        .filter(|weight| *weight != 0)
        .map_or(0, |weight| jobs.min(queued).min(limit / weight))
}
struct Prepared {
    report: TaskReport,
    outputs: Vec<PreparedOutput>,
}
fn precedence(a: u8, b: u8) -> u8 {
    if a == 2 || b == 2 {
        2
    } else if a == 1 || b == 1 {
        1
    } else if a == 3 || b == 3 {
        3
    } else {
        0
    }
}
#[cfg(feature = "fs")]
fn identity_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut ancestor = absolute.as_path();
    let mut tails = Vec::new();
    let mut canonical = loop {
        match std::fs::canonicalize(ancestor) {
            Ok(p) => break p,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                let name = ancestor
                    .file_name()
                    .ok_or_else(|| invalid("cannot normalize batch path"))?;
                tails.push(name.to_owned());
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| invalid("cannot normalize batch path parent"))?;
            }
            Err(error) => return Err(error.into()),
        }
    };
    for name in tails.into_iter().rev() {
        canonical.push(name);
    }
    Ok(canonical)
}
/// The protection identity of a declared or output path: the canonical
/// filesystem path, or the normalized memory key.
fn declared_path(storage: &Storage, path: &Path) -> Result<PathBuf> {
    match storage {
        #[cfg(feature = "fs")]
        Storage::Filesystem => identity_path(path),
        Storage::Memory(_) => Ok(PathBuf::from(memory::key(path))),
    }
}
fn overlaps_declared(target: &Path, protected: &BTreeSet<PathBuf>) -> bool {
    target
        .ancestors()
        .any(|ancestor| protected.contains(ancestor))
        || protected
            .range(target.to_owned()..)
            .next()
            .is_some_and(|input| input.starts_with(target))
}
fn check_declared_outputs(
    storage: &Storage,
    outputs: &[PreparedOutput],
    protected: &BTreeSet<PathBuf>,
    spool_directory: Option<&Path>,
) -> Result<()> {
    for output in outputs {
        let overlaps_spool = |target: &Path| {
            spool_directory
                .is_some_and(|spool| target.starts_with(spool) || spool.starts_with(target))
        };
        if overlaps_spool(&declared_path(storage, &output.destination)?) {
            return Err(invalid("output overlaps private batch report storage"));
        }
        let parent = output.destination.parent().unwrap_or(Path::new("."));
        for asset in output.bundle.assets() {
            let target = declared_path(storage, &parent.join(asset.path.as_str()))?;
            if overlaps_spool(&target) {
                return Err(invalid("output overlaps private batch report storage"));
            }
            if overlaps_declared(&target, protected) {
                return Err(invalid(
                    "output overlaps a declared batch input (including a failed or missing input)",
                ));
            }
        }
    }
    Ok(())
}
fn remember_declared(
    storage: &Storage,
    path: &Path,
    sources: &Sources,
    protected: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    if protected.len() >= 100_000 {
        return Err(Error::ResourceLimit {
            resource: "batch declared input paths",
            limit: 100_000,
        });
    }
    protected.insert(declared_path(storage, path)?);
    #[cfg(feature = "fs")]
    if matches!(storage, Storage::Filesystem) {
        return remember_file(path, sources);
    }
    let _ = sources;
    Ok(())
}
#[cfg(feature = "fs")]
fn remember_file(path: &Path, sources: &Sources) -> Result<()> {
    match File::open(path) {
        Ok(file) => {
            if file.metadata()?.is_file() || file.metadata()?.is_dir() {
                sources.remember(&file)?;
            }
            Ok(())
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
#[cfg(all(feature = "fs", feature = "toml"))]
pub(crate) fn load_text(file: &Path, sources: &Sources) -> Result<String> {
    const MAXIMUM: usize = 8 * 1024 * 1024;
    let file = File::open(file)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("batch input must be a regular TOML file"));
    }
    sources.remember(&file)?;
    if file.metadata()?.len() > MAXIMUM as u64 {
        return Err(Error::ResourceLimit {
            resource: "batch TOML bytes",
            limit: MAXIMUM,
        });
    }
    let mut bytes = Vec::new();
    file.take((MAXIMUM + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAXIMUM {
        return Err(Error::ResourceLimit {
            resource: "batch TOML bytes",
            limit: MAXIMUM,
        });
    }
    String::from_utf8(bytes).map_err(|e| invalid(e.utf8_error().to_string()))
}
/// The shared state one task's capture attaches to.
#[derive(Clone)]
struct TaskScope {
    storage: Storage,
    sources: Sources,
    pending: Arc<Mutex<usize>>,
    pending_limit: usize,
}
fn prepare(
    execute: impl FnOnce(&mut dyn Write) -> Result<()>,
    mut report: TaskReport,
    scope: TaskScope,
    reports: &ReportBudget,
    report_limit: usize,
) -> Prepared {
    // Only task identity belongs to the caller. This execution determines all
    // outcome fields; stale success/commit state must never retain its outputs.
    report.success = false;
    report.exit_code = 0;
    report.committed = false;
    report.result = None;
    report.report = None;
    report.diagnostics_truncated = false;
    report.error = None;
    CAPTURE.with(|cell| {
        *cell.borrow_mut() = Some(Capture {
            storage: scope.storage,
            sources: scope.sources,
            outputs: Vec::new(),
            pending: scope.pending,
            limit: scope.pending_limit,
            charged: 0,
        })
    });
    let guard = CaptureGuard;
    let mut output = LimitedOutput {
        bytes: Vec::new(),
        limit: report_limit,
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(&mut output)));
    let capture_had_bytes = !output.bytes.is_empty();
    report.result = captured_json(output.bytes).map(ReportPayload::Inline);
    match outcome {
        Ok(Ok(())) => {
            if report.result.is_some() {
                report.success = true;
            } else {
                report.exit_code = 1;
                report.error = Some(
                    if capture_had_bytes {
                        "task produced invalid JSON"
                    } else {
                        "task produced no JSON report"
                    }
                    .into(),
                );
            }
        }
        Ok(Err(error)) => {
            report.exit_code = error.exit_code();
            report.error = Some(error.to_string());
            if let Some(diagnostics) = error.report() {
                let mut encoded = LimitedOutput {
                    bytes: Vec::new(),
                    limit: report_limit
                        .saturating_sub(report.result.as_ref().map_or(0, ReportPayload::len)),
                };
                if serde_json::to_writer(&mut encoded, diagnostics).is_ok() {
                    report.report = captured_json(encoded.bytes).map(ReportPayload::Inline);
                } else {
                    report.diagnostics_truncated = true;
                }
            }
        }
        Err(_) => {
            report.exit_code = 1;
            report.error = Some("task panicked; no outputs from this task were committed".into());
        }
    }
    bound_error(&mut report);
    let raw_bytes = report
        .result
        .as_ref()
        .map_or(0, ReportPayload::len)
        .saturating_add(report.report.as_ref().map_or(0, ReportPayload::len));
    let admitted = reports.used.lock().is_ok_and(|mut used| {
        if let Some(next) = used
            .checked_add(raw_bytes)
            .filter(|next| *next <= reports.limit)
        {
            *used = next;
            true
        } else {
            false
        }
    });
    if !admitted {
        reports.exceeded.store(true, Ordering::Relaxed);
        report.success = false;
        report.exit_code = 1;
        report.error =
            Some("batch aggregate report byte limit exceeded; no outputs will be committed".into());
        report.result = None;
        report.report = None;
    }
    #[cfg(feature = "fs")]
    if admitted && let Some(spool) = &reports.spool {
        let spilled = report
            .result
            .iter_mut()
            .chain(report.report.iter_mut())
            .try_for_each(|payload| payload.spill(spool));
        if let Err(error) = spilled {
            let mut message =
                format!("batch report storage failed; no outputs will be committed: {error}");
            if message.len() > 512 {
                let mut end = 512;
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message.truncate(end);
            }
            if let Ok(mut stored) = reports.storage_error.lock()
                && stored.is_none()
            {
                *stored = Some(message.clone());
            }
            reports.exceeded.store(true, Ordering::Relaxed);
            report.success = false;
            report.exit_code = 1;
            report.diagnostics_truncated = true;
            report.result = None;
            report.report = None;
            report.error = Some(message);
        }
    }
    let outputs = if report.success {
        CAPTURE.with(|cell| {
            let mut capture = cell.borrow_mut();
            let c = capture.as_mut().expect("active capture");
            c.charged = 0;
            std::mem::take(&mut c.outputs)
        })
    } else {
        Vec::new()
    };
    drop(guard);
    Prepared { report, outputs }
}
/// Run a typed batch: every task under the shared working, pending and report
/// ledgers, one output transaction for every successful writer, and the
/// complete JSON document written to `output`. An `Err` is a whole-batch
/// failure (see [`write_failure`]); task failures are reported per task.
pub fn run(
    request: BatchRequest,
    storage: &Storage,
    output: &mut dyn Write,
) -> Result<BatchResponse> {
    run_with_sources(request, storage, Sources::default(), output)
}
/// Write the whole-batch failure document and return its exit code
/// (1 for I/O and resource limits, otherwise 2).
pub fn write_failure(output: &mut dyn Write, error: &Error) -> u8 {
    let code = if matches!(error, Error::Io(_) | Error::ResourceLimit { .. }) {
        1
    } else {
        2
    };
    let _ = crate::json::write(
        output,
        &serde_json::json!({"version":1,"success":false,"exit_code":code,"error":error.to_string(),"tasks":[]}),
        true,
    );
    code
}
type Queued = (TaskRequest, TaskReport, usize);
pub(crate) fn run_with_sources(
    request: BatchRequest,
    storage: &Storage,
    sources: Sources,
    output: &mut dyn Write,
) -> Result<BatchResponse> {
    let BatchRequest {
        jobs,
        max_working_bytes,
        max_pending_bytes,
        max_report_bytes,
        max_task_report_bytes,
        protected_paths,
        tasks: batch_tasks,
    } = request;
    if batch_tasks.is_empty() || batch_tasks.len() > 10_000 {
        return Err(invalid("batch must contain 1..10000 tasks"));
    }
    let jobs = jobs.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(32)
    });
    if !(1..=64).contains(&jobs) {
        return Err(invalid("jobs must be in 1..64"));
    }
    if max_working_bytes == 0
        || max_pending_bytes == 0
        || max_report_bytes == 0
        || max_task_report_bytes == 0
    {
        return Err(invalid("batch budgets must be positive"));
    }
    if batch_tasks
        .len()
        .checked_mul(max_task_report_bytes.min(4096))
        .is_none_or(|n| n > max_report_bytes)
    {
        return Err(invalid("batch report budget too small for task metadata"));
    }
    let metadata_reserve = batch_tasks
        .len()
        .checked_mul(4096)
        .ok_or_else(|| invalid("batch report metadata overflow"))?;
    let raw_allowance = max_report_bytes
        .checked_sub(metadata_reserve)
        .ok_or_else(|| invalid("batch report budget too small for task metadata"))?;
    let report_limit = max_task_report_bytes;
    let pending = Arc::new(Mutex::new(0usize));
    sources.set_retained_budget(Arc::clone(&pending), max_pending_bytes)?;
    let mut ids = BTreeSet::new();
    for task in &batch_tasks {
        if task.id.trim().is_empty() || task.id.len() > 256 {
            return Err(invalid("task id must contain 1..256 UTF-8 bytes"));
        }
        if !ids.insert(&task.id) {
            return Err(invalid(format!("duplicate task id {:?}", task.id)));
        }
    }
    drop(ids);
    let mut protected = BTreeSet::new();
    for path in &protected_paths {
        remember_declared(storage, path, &sources, &mut protected)?;
    }
    for task in &batch_tasks {
        for input in &task.declared_inputs {
            remember_declared(storage, input, &sources, &mut protected)?;
        }
    }
    let protected = Arc::new(protected);
    let reports = ReportBudget {
        used: Mutex::new(0),
        limit: raw_allowance,
        exceeded: AtomicBool::new(false),
        storage_error: Mutex::new(None),
        #[cfg(feature = "fs")]
        spool: match storage {
            Storage::Filesystem => Some(Arc::new(ReportSpool {
                state: Mutex::new(None),
                root: std::env::temp_dir(),
                protected: Arc::clone(&protected),
                sources: sources.clone(),
                limit: raw_allowance,
            })),
            Storage::Memory(_) => None,
        },
    };
    let mut results: Vec<Option<Prepared>> = (0..batch_tasks.len()).map(|_| None).collect();
    let mut queue: VecDeque<Queued> = VecDeque::new();
    for (index, task) in batch_tasks.into_iter().enumerate() {
        let mut report = TaskReport::new(index, &task);
        let prepared = match task.action {
            TaskAction::Run { request, weight } => {
                if weight > max_working_bytes {
                    Err(invalid("task max_total_bytes exceeds batch max_working_bytes").to_string())
                } else {
                    Ok((request.into_json(), weight))
                }
            }
            TaskAction::Invalid(error) => Err(error),
        };
        match prepared {
            Ok((request, weight)) => queue.push_back((request, report, weight)),
            Err(error) => {
                report.exit_code = 2;
                report.error = Some(error);
                bound_error(&mut report);
                results[index] = Some(Prepared {
                    report,
                    outputs: Vec::new(),
                });
            }
        }
    }
    let workers = worker_count(
        jobs,
        max_working_bytes,
        queue.len(),
        queue.iter().map(|(_, _, weight)| *weight).min(),
    );
    if workers > 1 && !crate::capabilities().threads {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::Unsupported,
            "batch parallelism needs threads, which this target lacks; set jobs = 1",
        )));
    }
    let queue = Mutex::new(queue);
    let results = Mutex::new(results);
    let working = Working {
        used: Mutex::new(0),
        ready: Condvar::new(),
        limit: max_working_bytes,
    };
    let scope = TaskScope {
        storage: storage.clone(),
        sources: sources.clone(),
        pending: Arc::clone(&pending),
        pending_limit: max_pending_bytes,
    };
    let worker = |scope: TaskScope| {
        loop {
            let item = queue.lock().expect("batch queue lock").pop_front();
            let Some((request, mut report, weight)) = item else {
                break;
            };
            let index = report.index;
            let prepared = match working.reserve(weight) {
                Ok(_reservation) => prepare(
                    |out| request.execute(out),
                    report,
                    scope.clone(),
                    &reports,
                    report_limit,
                ),
                Err(error) => {
                    report.exit_code = 1;
                    report.error = Some(error.to_string());
                    Prepared {
                        report,
                        outputs: Vec::new(),
                    }
                }
            };
            results.lock().expect("batch results lock")[index] = Some(prepared);
        }
    };
    if crate::capabilities().threads {
        std::thread::scope(|threads| {
            for _ in 0..workers {
                let (worker, scope) = (&worker, scope.clone());
                threads.spawn(move || worker(scope));
            }
        });
    } else if workers == 1 {
        worker(scope.clone());
    }
    drop(scope);
    let results = results
        .into_inner()
        .map_err(|_| invalid("batch results poisoned"))?;
    let mut prepared: Vec<_> = results
        .into_iter()
        .map(|p| p.ok_or_else(|| invalid("batch worker did not return a result")))
        .collect::<Result<_>>()?;
    if reports.exceeded.load(Ordering::Relaxed) {
        let message = reports
            .storage_error
            .lock()
            .map_err(|_| invalid("report storage error lock poisoned"))?
            .clone()
            .unwrap_or_else(|| {
                "batch aggregate report byte limit exceeded; no outputs were committed".into()
            });
        for task in &mut prepared {
            task.outputs.clear();
            task.report.diagnostics_truncated |=
                task.report.result.is_some() || task.report.report.is_some();
            task.report.result = None;
            task.report.report = None;
            task.report.success = false;
            if task.report.exit_code != 2 {
                task.report.exit_code = 1;
            }
            task.report.error = Some(message.clone());
        }
    }
    let output_count = prepared.iter().try_fold(0usize, |n, p| {
        n.checked_add(p.outputs.len())
            .ok_or_else(|| invalid("pending output count overflow"))
    })?;
    let array_reserve = output_count
        .checked_mul(std::mem::size_of::<PreparedOutput>())
        .ok_or_else(|| invalid("pending output array overflow"))?;
    let pending_used = *pending
        .lock()
        .map_err(|_| invalid("pending budget poisoned"))?;
    if pending_used
        .checked_add(array_reserve)
        .is_none_or(|n| n > max_pending_bytes)
    {
        return Err(Error::ResourceLimit {
            resource: "batch pending output aggregation",
            limit: max_pending_bytes,
        });
    }
    let mut outputs = Vec::new();
    outputs
        .try_reserve_exact(output_count)
        .map_err(|_| invalid("pending output aggregation allocation failed"))?;
    if outputs
        .capacity()
        .checked_mul(std::mem::size_of::<PreparedOutput>())
        .is_none_or(|n| n > array_reserve)
    {
        return Err(Error::ResourceLimit {
            resource: "batch pending output aggregation",
            limit: max_pending_bytes,
        });
    }
    let mut writers = Vec::new();
    for (index, p) in prepared.iter_mut().enumerate() {
        if !p.outputs.is_empty() {
            writers.push(index);
            outputs.append(&mut p.outputs);
        }
    }
    for p in &mut prepared {
        bound_error(&mut p.report);
    }
    let mut tasks: Vec<_> = prepared.into_iter().map(|p| p.report).collect();
    let reserve = writers.len().saturating_mul(2048).saturating_add(256);
    let limit = max_report_bytes
        .checked_sub(reserve)
        .ok_or(Error::ResourceLimit {
            resource: "batch report commit reserve",
            limit: max_report_bytes,
        })?;
    preflight_document(&document(&tasks, jobs, workers), limit)?;
    #[cfg(feature = "fs")]
    let spool_directory = match &reports.spool {
        Some(spool) => spool.directory()?,
        None => None,
    };
    #[cfg(not(feature = "fs"))]
    let spool_directory: Option<PathBuf> = None;
    let mut committed_outputs = Vec::new();
    if !outputs.is_empty() {
        match check_declared_outputs(storage, &outputs, &protected, spool_directory.as_deref())
            .and_then(|()| commit(storage, outputs, &sources))
        {
            Ok(returned) => {
                committed_outputs = returned;
                for index in &writers {
                    tasks[*index].committed = true;
                }
            }
            Err(error) => {
                for index in &writers {
                    let r = &mut tasks[*index];
                    r.success = false;
                    r.exit_code = error.exit_code();
                    let mut message = format!("batch output transaction failed: {error}");
                    if message.len() > 256 {
                        let mut end = 256;
                        while !message.is_char_boundary(end) {
                            end -= 1;
                        }
                        message.truncate(end);
                        r.diagnostics_truncated = true;
                    }
                    r.error = Some(message);
                }
            }
        }
    }
    let document = document(&tasks, jobs, workers);
    let exit_code = document.exit_code;
    let jobs = document.jobs;
    write_document(output, &document)?;
    writeln!(output)?;
    Ok(BatchResponse {
        exit_code,
        jobs,
        workers,
        tasks: tasks
            .into_iter()
            .map(|task| TaskSummary {
                index: task.index,
                id: task.id,
                kind: task.kind,
                success: task.success,
                exit_code: task.exit_code,
                committed: task.committed,
                diagnostics_truncated: task.diagnostics_truncated,
                error: task.error,
            })
            .collect(),
        outputs: committed_outputs,
    })
}
/// Commit the combined output group: a filesystem transaction, or a validated
/// memory group returned to the caller.
fn commit(
    storage: &Storage,
    outputs: Vec<PreparedOutput>,
    sources: &Sources,
) -> Result<Vec<PreparedOutput>> {
    match storage {
        #[cfg(feature = "fs")]
        Storage::Filesystem => {
            crate::assets::persist_bundles(&outputs, sources)?;
            Ok(Vec::new())
        }
        Storage::Memory(_) => {
            let _ = sources;
            memory::check_group(&outputs)?;
            Ok(outputs)
        }
    }
}

// Reconstructed isolated regression module from this worker's recorded successful
// patch actions. Not an exact full batch.rs snapshot; never run after recovery.
#[cfg(all(test, feature = "fs"))]
mod tests {
    use super::{ReportPayload, ReportSpool, SPOOL_THRESHOLD, captured_json, worker_count};
    use crate::memory::Storage;
    use std::{
        collections::BTreeSet,
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
                    "{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn store(&self, limit: usize) -> Arc<ReportSpool> {
            Arc::new(ReportSpool {
                state: Mutex::new(None),
                root: self.0.clone(),
                protected: Arc::new(BTreeSet::new()),
                sources: crate::assets::Sources::default(),
                limit,
            })
        }
    }
    impl Drop for SpoolRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn captured_json_preserves_interior_whitespace_and_report_order() {
        let value = r#"{ "report": { "entries": [{"message":"  first\n ","code":"z"}, {"message":"second\t  ","code":"a"}] } }"#;
        let captured = captured_json(format!("{value}\r\n\t ").into_bytes()).unwrap();
        assert_eq!(captured.get(), value);
        assert_eq!(serde_json::to_string(&captured).unwrap(), value);
    }
    #[test]
    fn captured_json_empty_invalid_and_scalar_behavior_is_unchanged() {
        for value in ["", " \r\n\t", "{", "{} trailing\n", "{} {}\n"] {
            assert!(captured_json(value.as_bytes().to_vec()).is_none());
        }
        assert!(captured_json(vec![0xff, b'\n']).is_none());
        for value in ["null", "true", "42", r#"" string with trailing spaces  ""#] {
            let captured = captured_json(format!("{value}\n").into_bytes()).unwrap();
            assert_eq!(captured.get(), value);
        }
        assert_eq!(captured_json(b" \n{}\n".to_vec()).unwrap().get(), "{}");
    }
    #[test]
    fn workers_never_reduce_feasible_admission_for_mixed_weights() {
        for weights in [&[2, 2, 2, 2, 2][..], &[1, 2, 5, 8], &[3, 7, 7, 9]] {
            for limit in 1..=32 {
                for jobs in 1..=8 {
                    let eligible: Vec<_> =
                        weights.iter().copied().filter(|v| *v <= limit).collect();
                    let workers =
                        worker_count(jobs, limit, eligible.len(), eligible.iter().copied().min());
                    for subset in 0..(1usize << eligible.len()) {
                        let (count, sum) = eligible
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| subset & (1 << i) != 0)
                            .fold((0, 0), |(count, sum), (_, weight)| {
                                (count + 1, sum + weight)
                            });
                        if count <= jobs && sum <= limit {
                            assert!(count <= workers);
                        }
                    }
                }
            }
        }
        assert_eq!(worker_count(9, 1024, 64, Some(256)), 4);
        assert_eq!(worker_count(64, usize::MAX, 10, Some(1)), 10);
        assert_eq!(worker_count(9, 1024, 0, None), 0);
    }
    #[test]
    fn spooled_payloads_preserve_order_bytes_and_use_one_cleaned_file() {
        let root = SpoolRoot::new();
        let store = root.store(4 * SPOOL_THRESHOLD);
        let a = format!(
            r#"{{ "ordinal": 1, "message": "{}\n  " }}"#,
            "x".repeat(SPOOL_THRESHOLD)
        );
        let b = format!(
            r#"{{ "ordinal": 2, "message": "{}\t  " }}"#,
            "y".repeat(SPOOL_THRESHOLD)
        );
        let mut first = ReportPayload::Inline(captured_json(a.as_bytes().to_vec()).unwrap());
        let mut second = ReportPayload::Inline(captured_json(b.as_bytes().to_vec()).unwrap());
        second.spill(&store).unwrap();
        first.spill(&store).unwrap();
        assert!(matches!(first, ReportPayload::Spooled { .. }));
        assert!(matches!(second, ReportPayload::Spooled { .. }));
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
        let directory = store
            .state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .directory
            .clone();
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert_eq!(
            serde_json::to_string(&[&first, &second]).unwrap(),
            format!("[{a},{b}]")
        );
        drop(first);
        drop(second);
        drop(store);
        assert!(!directory.exists());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
    #[test]
    fn spool_quota_and_io_failures_do_not_grow_or_leak_storage() {
        let root = SpoolRoot::new();
        let store = root.store(2);
        assert_eq!(store.append(b"{}").unwrap().0, 0);
        assert!(store.append(b"[]").is_err());
        let directory = store
            .state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .directory
            .clone();
        assert_eq!(
            fs::metadata(directory.join("payloads.json")).unwrap().len(),
            2
        );
        assert!(store.read(0, 3).is_err());
        {
            let mut state = store.state.lock().unwrap();
            state.as_mut().unwrap().file =
                Some(fs::File::open(directory.join("payloads.json")).unwrap());
        }
        let mut owner = Arc::try_unwrap(store).ok().unwrap();
        owner.limit = 4;
        assert!(owner.append(b"[]").is_err());
        assert_eq!(
            fs::metadata(directory.join("payloads.json")).unwrap().len(),
            2
        );
        fs::write(directory.join("payloads.json"), b"{").unwrap();
        assert!(owner.read(0, 2).is_err());
        drop(owner);
        assert!(!directory.exists());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
    #[test]
    fn report_spool_never_creates_files_beneath_declared_inputs() {
        let root = SpoolRoot::new();
        let mut protected = BTreeSet::new();
        protected.insert(fs::canonicalize(&root.0).unwrap());
        let store = ReportSpool {
            state: Mutex::new(None),
            root: root.0.clone(),
            protected: Arc::new(protected),
            sources: crate::assets::Sources::default(),
            limit: 1024,
        };
        assert!(store.append(b"{}").is_err());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
    #[test]
    fn report_spool_identity_cannot_be_replaced_by_output_transaction() {
        let root = SpoolRoot::new();
        let store = root.store(1024);
        store.append(b"{}").unwrap();
        let destination = store
            .state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .directory
            .join("payloads.json");
        let output = crate::context::PreparedOutput {
            destination,
            bundle: fontpixel::OutputBundle::single(
                fontpixel::ResourcePath::new("payloads.json").unwrap(),
                b"not report JSON".to_vec(),
            ),
        };
        assert!(crate::assets::persist_bundles(&[output], &store.sources).is_err());
        assert_eq!(store.read(0, 2).unwrap(), b"{}");
        drop(store);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
    #[test]
    fn report_spool_directory_ancestors_and_descendants_are_protected() {
        let root = SpoolRoot::new();
        let store = root.store(1024);
        store.append(b"{}").unwrap();
        let private = store.directory().unwrap().unwrap();
        for destination in [private.clone(), fs::canonicalize(&root.0).unwrap()] {
            let name = destination.file_name().unwrap().to_str().unwrap();
            let bundle = fontpixel::OutputBundle::directory(
                fontpixel::ResourcePath::new(name).unwrap(),
                vec![fontpixel::OutputAsset {
                    path: fontpixel::ResourcePath::new(format!("{name}/font.bdf")).unwrap(),
                    bytes: b"user output".to_vec(),
                }],
            )
            .unwrap();
            let outputs = [crate::context::PreparedOutput {
                destination,
                bundle,
            }];
            assert!(
                super::check_declared_outputs(
                    &Storage::Filesystem,
                    &outputs,
                    &BTreeSet::new(),
                    Some(&private)
                )
                .is_err()
            );
        }
        let outputs = [crate::context::PreparedOutput {
            destination: private.join("other.bdf"),
            bundle: fontpixel::OutputBundle::single(
                fontpixel::ResourcePath::new("other.bdf").unwrap(),
                b"user output".to_vec(),
            ),
        }];
        assert!(
            super::check_declared_outputs(
                &Storage::Filesystem,
                &outputs,
                &BTreeSet::new(),
                Some(&private)
            )
            .is_err()
        );
        assert_eq!(store.read(0, 2).unwrap(), b"{}");
        drop(store);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
    fn document_task(
        index: usize,
        code: u8,
        result: Option<ReportPayload>,
        report: Option<ReportPayload>,
    ) -> super::TaskReport {
        super::TaskReport {
            index,
            id: format!("task {index}: 字\n\t\"\\\0"),
            kind: [
                super::TaskKind::Convert,
                super::TaskKind::Variants,
                super::TaskKind::Diff,
                super::TaskKind::Merge,
            ][usize::from(code)],
            success: code == 0,
            exit_code: code,
            committed: index.is_multiple_of(2),
            result,
            report,
            diagnostics_truncated: !index.is_multiple_of(2),
            error: (code != 0).then(|| "error\r\n 字 \" \\ \u{0001}".into()),
        }
    }
    fn inline_json(value: &str) -> ReportPayload {
        ReportPayload::Inline(captured_json(value.as_bytes().to_vec()).unwrap())
    }
    #[test]
    fn streaming_documents_are_byte_identical_to_safe_serde_reference() {
        let root = SpoolRoot::new();
        let store = root.store(4 * SPOOL_THRESHOLD);
        let first_json = format!(
            "{{\n  \"message\": \"{}\\n  \", \"ordinal\": 1\n}}",
            "a".repeat(SPOOL_THRESHOLD)
        );
        let second_json = format!(
            "{{ \"message\": \"{}\\t 字 \", \"ordinal\": 2 }}",
            "b".repeat(SPOOL_THRESHOLD)
        );
        let mut first = inline_json(&first_json);
        let mut second = inline_json(&second_json);
        second.spill(&store).unwrap();
        first.spill(&store).unwrap();
        let tasks = vec![
            document_task(0, 0, Some(first), None),
            document_task(1, 1, None, Some(second)),
            document_task(
                2,
                2,
                Some(inline_json("[null, true, 42, {\"字\": \"  \\n\"}]")),
                Some(inline_json("{ \"entries\": [] }")),
            ),
            document_task(3, 3, None, None),
        ];
        for slice in [
            &tasks[..],
            &tasks[0..1],
            &tasks[1..3],
            &tasks[3..],
            &tasks[0..0],
        ] {
            let doc = super::document(slice, 9, 4);
            let expected = serde_json::to_vec_pretty(&doc).unwrap();
            let mut actual = Vec::new();
            super::write_document(&mut actual, &doc).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                super::preflight_document(&doc, expected.len()).unwrap(),
                expected.len()
            );
            assert!(matches!(
                super::preflight_document(&doc, expected.len() - 1),
                Err(fontpixel::Error::ResourceLimit { .. })
            ));
        }
        drop(tasks);
        drop(store);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
    #[test]
    fn spooled_integrity_and_length_failures_abort_before_output_commit() {
        let original = b"{\"id\":1}";
        for changed in [&b"{\"id\":2}"[..], &b"{\"id"[..]] {
            let root = SpoolRoot::new();
            let store = root.store(1024);
            let (offset, digest) = store.append(original).unwrap();
            let directory = store.directory().unwrap().unwrap();
            let payload = ReportPayload::Spooled {
                spool: Arc::clone(&store),
                offset,
                length: original.len(),
                digest,
            };
            let tasks = [document_task(0, 0, Some(payload), None)];
            let doc = super::document(&tasks, 1, 1);
            super::preflight_document(&doc, 4096).unwrap();
            fs::write(directory.join("payloads.json"), changed).unwrap();
            let destination = root.0.join("out.bdf");
            fs::write(&destination, b"previous output").unwrap();
            let outputs = [crate::context::PreparedOutput {
                destination: destination.clone(),
                bundle: fontpixel::OutputBundle::single(
                    fontpixel::ResourcePath::new("out.bdf").unwrap(),
                    b"new output".to_vec(),
                ),
            }];
            let mut commits = 0;
            let result = super::preflight_document(&doc, 4096).and_then(|_| {
                commits += 1;
                crate::assets::persist_bundles(&outputs, &store.sources)
            });
            assert!(matches!(result, Err(fontpixel::Error::Io(_))));
            assert_eq!(commits, 0);
            assert_eq!(fs::read(&destination).unwrap(), b"previous output");
            assert!(super::write_document(&mut Vec::new(), &doc).is_err());
            drop(tasks);
            drop(store);
            assert!(!directory.exists());
        }
    }
    #[test]
    fn streaming_document_propagates_output_errors_and_cleans_storage() {
        use std::io::{self, Write};
        struct FailingOutput(usize);
        impl Write for FailingOutput {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0 == 0 {
                    return Err(io::Error::other("injected output failure"));
                }
                let count = self.0.min(bytes.len());
                self.0 -= count;
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let root = SpoolRoot::new();
        let store = root.store(2 * SPOOL_THRESHOLD);
        let mut payload = inline_json(&format!("\"{}\"", "x".repeat(SPOOL_THRESHOLD)));
        payload.spill(&store).unwrap();
        let directory = store.directory().unwrap().unwrap();
        let tasks = [document_task(0, 0, Some(payload), None)];
        let doc = super::document(&tasks, 1, 1);
        let length = super::preflight_document(&doc, 2 * SPOOL_THRESHOLD).unwrap();
        for limit in [0, 1, 128, 64 * 1024, length - 1] {
            let error = super::write_document(&mut FailingOutput(limit), &doc).unwrap_err();
            assert!(error.to_string().contains("injected output failure"));
        }
        drop(tasks);
        drop(store);
        assert!(!directory.exists());
    }
    #[test]
    fn successful_execution_without_json_is_not_a_successful_batch_task() {
        use std::sync::atomic::AtomicBool;
        // Ensure this fixture exercises a successful command with non-JSON
        // output, rather than an unrelated LimitedOutput overflow.
        let mut output = Vec::new();
        let text_only = |out: &mut dyn std::io::Write| {
            crate::formats(&crate::FormatsRequest::default()).write_text(out)
        };
        text_only(&mut output).unwrap();
        let report_limit = output.len();
        assert!(report_limit > 0);
        assert!(captured_json(output).is_none());
        let root = SpoolRoot::new();
        let store = root.store(4096);
        let reports = super::ReportBudget {
            used: Mutex::new(0),
            limit: 4096,
            exceeded: AtomicBool::new(false),
            storage_error: Mutex::new(None),
            spool: Some(store),
        };
        let pending = Arc::new(Mutex::new(0));
        let task = super::prepare(
            text_only,
            document_task(0, 0, None, None),
            super::TaskScope {
                storage: Storage::Filesystem,
                sources: crate::assets::Sources::default(),
                pending: Arc::clone(&pending),
                pending_limit: 4096,
            },
            &reports,
            report_limit,
        );
        assert!(!task.report.success);
        assert_eq!(task.report.exit_code, 1);
        assert!(!task.report.committed);
        assert!(task.report.result.is_none());
        assert!(task.report.report.is_none());
        assert_eq!(
            task.report.error.as_deref(),
            Some("task produced invalid JSON")
        );
        assert!(task.outputs.is_empty());
        assert_eq!(*pending.lock().unwrap(), 0);
        assert!(!crate::context::is_active());
    }
}
