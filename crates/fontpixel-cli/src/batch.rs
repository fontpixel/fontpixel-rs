//! Phase-6 reconstruction: pending compilation and renewed acceptance.
//! Typed parallel jobs, bounded reports and transactional output preparation.
use crate::{Cli, assets, execute};
use clap::{Args, Parser};
use fontpixel::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
#[derive(Args)]
#[command(
    after_help = "Examples:\n  fontpixel batch exports.toml --jobs 4 > report.json\n\nTOML schema and transaction rules: docs/w6.1-cli.md. Relative paths use the job file's directory. Tasks cannot depend on another task's output."
)]
pub(crate) struct BatchArgs {
    /// Version-1 TOML task file. Every task and the aggregate emit JSON.
    file: PathBuf,
    /// Maximum worker threads, 1..64; memory reservations may reduce concurrency.
    #[arg(long,value_parser=clap::value_parser!(u32).range(1..=64))]
    jobs: Option<u32>,
}
fn working() -> usize {
    1024 * 1024 * 1024
}
fn pending() -> usize {
    512 * 1024 * 1024
}
fn reports() -> usize {
    2 * 1024 * 1024 * 1024
}
fn task_reports() -> usize {
    64 * 1024 * 1024
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobFile {
    version: u32,
    jobs: Option<usize>,
    #[serde(default = "working")]
    max_working_bytes: usize,
    #[serde(default = "pending")]
    max_pending_bytes: usize,
    #[serde(default = "reports")]
    max_report_bytes: usize,
    #[serde(default = "task_reports")]
    max_task_report_bytes: usize,
    tasks: Vec<Task>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Convert,
    Vectorize,
    Variants,
    Subset,
    Merge,
    Charset,
    Validate,
    Diff,
}
impl Kind {
    fn name(self) -> &'static str {
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    input: PathBuf,
    take: String,
    dy: Option<i64>,
    from: Option<String>,
    font_index: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Task {
    id: String,
    kind: Kind,
    #[serde(default)]
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    #[serde(default)]
    outputs: Vec<PathBuf>,
    #[serde(default)]
    output_formats: Vec<String>,
    from: Option<String>,
    to: Option<String>,
    #[serde(default)]
    strict: bool,
    family: Option<String>,
    base: Option<PathBuf>,
    #[serde(default)]
    languages: BTreeMap<String, PathBuf>,
    #[serde(default)]
    scripts: BTreeMap<String, PathBuf>,
    #[serde(default)]
    parts: Vec<Part>,
    #[serde(default)]
    options: BTreeMap<String, toml::Value>,
}
fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
fn path(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_owned()
    } else {
        base.join(p)
    }
}
fn arg(args: &mut Vec<OsString>, key: &str, value: impl AsRef<std::ffi::OsStr>) {
    args.push(format!("--{key}").into());
    args.push(value.as_ref().to_owned());
}
fn option(args: &mut Vec<OsString>, key: &str, value: &toml::Value, base: &Path) -> Result<()> {
    let token = match value {
        toml::Value::Boolean(v) if key == "gl" => {
            args.push(format!("--{key}={v}").into());
            return Ok(());
        }
        toml::Value::Boolean(true) => {
            args.push(format!("--{key}").into());
            return Ok(());
        }
        toml::Value::Boolean(false) => return Ok(()),
        toml::Value::String(v) if key == "text-file" => {
            arg(args, key, path(base, Path::new(v)));
            return Ok(());
        }
        toml::Value::String(v) => v.clone(),
        toml::Value::Integer(v) => v.to_string(),
        toml::Value::Float(v) if v.is_finite() => v.to_string(),
        toml::Value::Array(values) => {
            for v in values {
                if matches!(v, toml::Value::Array(_)) {
                    return Err(invalid("nested option arrays are not allowed"));
                }
                option(args, key, v, base)?;
            }
            return Ok(());
        }
        _ => {
            return Err(invalid(format!(
                "option {key} must be a scalar or an array of scalars"
            )));
        }
    };
    args.push(format!("--{key}={token}").into());
    Ok(())
}
impl Task {
    fn command(&self, base: &Path) -> Result<crate::Command> {
        if self.id.trim().is_empty() || self.id.len() > 256 {
            return Err(invalid("task id must contain 1..256 UTF-8 bytes"));
        }
        let mut args = vec![OsString::from("fontpixel"), self.kind.name().into()];
        let merge = self.kind == Kind::Merge;
        let variants = self.kind == Kind::Variants;
        let readonly = matches!(self.kind, Kind::Validate | Kind::Diff);
        if !merge
            && (!self.parts.is_empty()
                || !self.outputs.is_empty()
                || !self.output_formats.is_empty())
        {
            return Err(invalid("parts/outputs require kind=merge"));
        }
        if !variants
            && (self.base.is_some() || !self.languages.is_empty() || !self.scripts.is_empty())
        {
            return Err(invalid("base/languages/scripts require kind=variants"));
        }
        if self.family.is_some()
            && !merge
            && !variants
            && !matches!(self.kind, Kind::Convert | Kind::Vectorize)
        {
            return Err(invalid(
                "family requires convert, vectorize, merge or variants",
            ));
        }
        if readonly && (self.output.is_some() || self.to.is_some()) {
            return Err(invalid("read-only task cannot have output/to"));
        }
        if merge {
            if self.output.is_some()
                || !self.inputs.is_empty()
                || self.from.is_some()
                || self.to.is_some()
            {
                return Err(invalid(
                    "merge uses parts and outputs, not inputs/output/from/to",
                ));
            }
            if self.parts.is_empty() || self.outputs.is_empty() {
                return Err(invalid("merge requires parts and outputs"));
            }
            if !self.output_formats.is_empty() && self.output_formats.len() != self.outputs.len() {
                return Err(invalid(
                    "output_formats must have exactly one format for each merge output",
                ));
            }
            for (index, output) in self.outputs.iter().enumerate() {
                arg(&mut args, "output", path(base, output));
                if let Some(format) = self.output_formats.get(index) {
                    arg(&mut args, "to", format);
                }
            }
            arg(
                &mut args,
                "family",
                self.family
                    .as_ref()
                    .ok_or_else(|| invalid("merge requires family"))?,
            );
            for part in &self.parts {
                arg(&mut args, "part", path(base, &part.input));
                args.push(format!("--take={}", part.take).into());
                if let Some(dy) = part.dy {
                    args.push(format!("--dy={dy}").into());
                }
                if let Some(from) = &part.from {
                    arg(&mut args, "from", from);
                }
                if let Some(index) = part.font_index {
                    arg(&mut args, "font-index", index.to_string());
                }
            }
        } else if variants {
            if !self.inputs.is_empty() || self.from.is_some() {
                return Err(invalid(
                    "variants uses base/languages/scripts instead of inputs/from",
                ));
            }
            arg(
                &mut args,
                "base",
                path(
                    base,
                    self.base
                        .as_deref()
                        .ok_or_else(|| invalid("variants requires base"))?,
                ),
            );
            for (key, value) in &self.languages {
                let value = path(base, value);
                arg(
                    &mut args,
                    "lang",
                    format!(
                        "{key}={}",
                        value
                            .to_str()
                            .ok_or_else(|| invalid("variant paths must be UTF-8"))?
                    ),
                );
            }
            for (key, value) in &self.scripts {
                let value = path(base, value);
                arg(
                    &mut args,
                    "script",
                    format!(
                        "{key}={}",
                        value
                            .to_str()
                            .ok_or_else(|| invalid("variant paths must be UTF-8"))?
                    ),
                );
            }
            if let Some(family) = &self.family {
                arg(&mut args, "family", family);
            }
        } else {
            let required = if self.kind == Kind::Diff { 2 } else { 1 };
            if self.inputs.len() < required
                || (!matches!(self.kind, Kind::Convert | Kind::Vectorize)
                    && self.inputs.len() != required)
            {
                return Err(invalid(format!(
                    "{} needs {} input(s)",
                    self.kind.name(),
                    required
                )));
            }
            for input in &self.inputs {
                args.push(path(base, input).into_os_string());
            }
            if let Some(from) = &self.from {
                arg(&mut args, "from", from);
            }
            if let Some(family) = &self.family {
                arg(&mut args, "family", family);
            }
        }
        if !readonly && !merge {
            arg(
                &mut args,
                "output",
                path(
                    base,
                    self.output
                        .as_deref()
                        .ok_or_else(|| invalid("task requires output"))?,
                ),
            );
            if let Some(to) = &self.to {
                arg(&mut args, "to", to);
            }
        }
        if self.strict {
            args.push("--strict".into());
        }
        if readonly {
            args.push("--json".into());
        } else {
            arg(&mut args, "report", "json");
        }
        let mut normalized = BTreeSet::new();
        for (key, value) in &self.options {
            let key = key.replace('_', "-");
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || !normalized.insert(key.clone())
            {
                return Err(invalid("invalid or duplicate normalized option name"));
            }
            if [
                "help", "version", "output", "report", "json", "strict", "from", "to", "base",
                "lang", "script", "part", "take", "dy", "family",
            ]
            .contains(&key.as_str())
                || (merge && key == "font-index")
            {
                return Err(invalid(format!(
                    "option {key} is reserved; use the typed task fields"
                )));
            }
            option(&mut args, &key, value, base)?;
        }
        Cli::try_parse_from(args)
            .map(|c| c.command)
            .map_err(|e| invalid(e.to_string()))
    }
    fn weight(&self) -> Result<usize> {
        match self
            .options
            .get("max_total_bytes")
            .or_else(|| self.options.get("max-total-bytes"))
        {
            None => Ok(fontpixel::ResourceLimits::default().max_total_bytes),
            Some(toml::Value::Integer(v)) => usize::try_from(*v)
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("max_total_bytes must be positive")),
            _ => Err(invalid("max_total_bytes must be an integer")),
        }
    }
}
struct Capture {
    sources: assets::Sources,
    outputs: Vec<assets::PendingOutput>,
    pending: Arc<Mutex<usize>>,
    limit: usize,
    charged: usize,
}
thread_local! {
    static CAPTURE:RefCell<Option<Capture>>=const{RefCell::new(None)};
    static SOURCE_SCOPE:RefCell<Option<assets::Sources>>=const{RefCell::new(None)};
}
pub(crate) fn with_source_scope<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(Option<assets::Sources>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SOURCE_SCOPE.with(|s| *s.borrow_mut() = self.0.take());
        }
    }
    let restore =
        Restore(SOURCE_SCOPE.with(|s| s.borrow_mut().replace(assets::Sources::default())));
    let result = f();
    drop(restore);
    result
}
pub(crate) fn is_active() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}
pub(crate) fn sources() -> assets::Sources {
    CAPTURE.with(|c| {
        c.borrow()
            .as_ref()
            .map(|c| c.sources.clone())
            .unwrap_or_else(|| SOURCE_SCOPE.with(|s| s.borrow().clone().unwrap_or_default()))
    })
}
pub(crate) fn persist_outputs(
    mut outputs: Vec<assets::PendingOutput>,
    sources: &assets::Sources,
) -> Result<()> {
    CAPTURE.with(|cell| {
        let mut state = cell.borrow_mut();
        let Some(c) = state.as_mut() else {
            return assets::persist_bundles(&outputs, sources);
        };
        let overflow = || Error::ResourceLimit {
            resource: "batch pending output bytes",
            limit: c.limit,
        };
        let mut charge = outputs
            .capacity()
            .checked_mul(std::mem::size_of::<assets::PendingOutput>())
            .ok_or_else(overflow)?;
        for output in &outputs {
            let owned = output.bundle.retained_bytes(c.limit)?;
            charge = charge
                .checked_add(owned.saturating_sub(std::mem::size_of::<fontpixel::OutputBundle>()))
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
                .and_then(|n| n.checked_mul(std::mem::size_of::<assets::PendingOutput>()))
                .ok_or_else(overflow)?
        };
        let next = used
            .checked_add(charge)
            .and_then(|n| n.checked_add(receiving_reserve))
            .filter(|n| *n <= c.limit)
            .ok_or_else(overflow)?;
        if c.outputs.is_empty() {
            c.outputs = outputs;
        } else {
            c.outputs
                .try_reserve_exact(outputs.len())
                .map_err(|_| overflow())?;
            // Check actual capacity as well; allocator over-allocation cannot be
            // silently handed to the retained-output ledger.
            let receiving_bytes = c
                .outputs
                .capacity()
                .checked_mul(std::mem::size_of::<assets::PendingOutput>())
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
        Ok(())
    })
}
struct CaptureGuard;
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
    kind: Kind,
    success: bool,
    exit_code: u8,
    committed: bool,
    result: Option<ReportPayload>,
    report: Option<ReportPayload>,
    diagnostics_truncated: bool,
    error: Option<String>,
}
impl TaskReport {
    fn new(index: usize, task: &Task) -> Self {
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
    spool: Arc<ReportSpool>,
}
const SPOOL_THRESHOLD: usize = 1024 * 1024;
struct ReportSpool {
    state: Mutex<Option<SpoolFile>>,
    root: PathBuf,
    protected: Arc<BTreeSet<PathBuf>>,
    sources: assets::Sources,
    limit: usize,
}
struct SpoolFile {
    file: Option<File>,
    directory: PathBuf,
}
impl SpoolFile {
    fn create(
        root: &Path,
        protected: &BTreeSet<PathBuf>,
        sources: &assets::Sources,
    ) -> io::Result<Self> {
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
impl Drop for SpoolFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(self.directory.join("payloads.json"));
        let _ = std::fs::remove_dir(&self.directory);
    }
}
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
            Self::Spooled { length, .. } => *length,
        }
    }
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
    outputs: Vec<assets::PendingOutput>,
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
    outputs: &[assets::PendingOutput],
    protected: &BTreeSet<PathBuf>,
    spool_directory: Option<&Path>,
) -> Result<()> {
    for output in outputs {
        let overlaps_spool = |target: &Path| {
            spool_directory
                .is_some_and(|spool| target.starts_with(spool) || spool.starts_with(target))
        };
        if overlaps_spool(&identity_path(&output.destination)?) {
            return Err(invalid("output overlaps private batch report storage"));
        }
        let parent = output.destination.parent().unwrap_or(Path::new("."));
        for asset in output.bundle.assets() {
            let target = identity_path(&parent.join(asset.path.as_str()))?;
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
    path: &Path,
    sources: &assets::Sources,
    protected: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    if protected.len() >= 100_000 {
        return Err(Error::ResourceLimit {
            resource: "batch declared input paths",
            limit: 100_000,
        });
    }
    protected.insert(identity_path(path)?);
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
fn load_file(file: &Path, sources: &assets::Sources) -> Result<JobFile> {
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
    let text = std::str::from_utf8(&bytes).map_err(|e| invalid(e.to_string()))?;
    toml::from_str(text).map_err(|e| invalid(format!("invalid batch TOML: {e}")))
}
fn prepare(
    command: crate::Command,
    mut report: TaskReport,
    sources: assets::Sources,
    pending: Arc<Mutex<usize>>,
    pending_limit: usize,
    reports: &ReportBudget,
    report_limit: usize,
) -> Prepared {
    CAPTURE.with(|cell| {
        *cell.borrow_mut() = Some(Capture {
            sources,
            outputs: Vec::new(),
            pending,
            limit: pending_limit,
            charged: 0,
        })
    });
    let guard = CaptureGuard;
    let mut output = LimitedOutput {
        bytes: Vec::new(),
        limit: report_limit,
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(command, &mut output)
    }));
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
    if admitted {
        let spilled = report
            .result
            .iter_mut()
            .chain(report.report.iter_mut())
            .try_for_each(|payload| payload.spill(&reports.spool));
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
            if let Ok(mut stored) = reports.storage_error.lock() {
                if stored.is_none() {
                    *stored = Some(message.clone());
                }
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
fn run_inner(args: BatchArgs, output: &mut dyn Write) -> Result<u8> {
    let sources = assets::Sources::default();
    let absolute = if args.file.is_absolute() {
        args.file
    } else {
        std::env::current_dir()?.join(args.file)
    };
    let base = absolute.parent().unwrap_or(Path::new("."));
    let config = load_file(&absolute, &sources)?;
    if config.version != 1 {
        return Err(invalid("only batch version=1 is supported"));
    }
    if config.tasks.is_empty() || config.tasks.len() > 10_000 {
        return Err(invalid("batch must contain 1..10000 tasks"));
    }
    let jobs = args
        .jobs
        .map(|v| v as usize)
        .or(config.jobs)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(32)
        });
    if !(1..=64).contains(&jobs) {
        return Err(invalid("jobs must be in 1..64"));
    }
    if config.max_working_bytes == 0
        || config.max_pending_bytes == 0
        || config.max_report_bytes == 0
        || config.max_task_report_bytes == 0
    {
        return Err(invalid("batch budgets must be positive"));
    }
    if config
        .tasks
        .len()
        .checked_mul(config.max_task_report_bytes.min(4096))
        .is_none_or(|n| n > config.max_report_bytes)
    {
        return Err(invalid("batch report budget too small for task metadata"));
    }
    let metadata_reserve = config
        .tasks
        .len()
        .checked_mul(4096)
        .ok_or_else(|| invalid("batch report metadata overflow"))?;
    let raw_allowance = config
        .max_report_bytes
        .checked_sub(metadata_reserve)
        .ok_or_else(|| invalid("batch report budget too small for task metadata"))?;
    let report_limit = config.max_task_report_bytes;
    let pending = Arc::new(Mutex::new(0usize));
    sources.set_retained_budget(Arc::clone(&pending), config.max_pending_bytes)?;
    let mut ids = BTreeSet::new();
    for task in &config.tasks {
        if task.id.trim().is_empty() || task.id.len() > 256 {
            return Err(invalid("task id must contain 1..256 UTF-8 bytes"));
        }
        if !ids.insert(&task.id) {
            return Err(invalid(format!("duplicate task id {:?}", task.id)));
        }
    }
    let mut protected = BTreeSet::new();
    remember_declared(&absolute, &sources, &mut protected)?;
    for task in &config.tasks {
        for input in task
            .inputs
            .iter()
            .chain(task.base.iter())
            .chain(task.languages.values())
            .chain(task.scripts.values())
            .chain(task.parts.iter().map(|p| &p.input))
        {
            remember_declared(&path(base, input), &sources, &mut protected)?;
        }
        for (key, value) in &task.options {
            if key.replace('_', "-") == "text-file" {
                match value {
                    toml::Value::String(p) => {
                        remember_declared(&path(base, Path::new(p)), &sources, &mut protected)?
                    }
                    toml::Value::Array(values) => {
                        for value in values {
                            if let toml::Value::String(p) = value {
                                remember_declared(
                                    &path(base, Path::new(p)),
                                    &sources,
                                    &mut protected,
                                )?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let protected = Arc::new(protected);
    let reports = ReportBudget {
        used: Mutex::new(0),
        limit: raw_allowance,
        exceeded: AtomicBool::new(false),
        storage_error: Mutex::new(None),
        spool: Arc::new(ReportSpool {
            state: Mutex::new(None),
            root: std::env::temp_dir(),
            protected: Arc::clone(&protected),
            sources: sources.clone(),
            limit: raw_allowance,
        }),
    };
    let mut results: Vec<Option<Prepared>> = (0..config.tasks.len()).map(|_| None).collect();
    let mut queue = VecDeque::new();
    for (index, task) in config.tasks.iter().enumerate() {
        let mut report = TaskReport::new(index, task);
        match task.command(base).and_then(|command| {
            let weight = task.weight()?;
            if weight > config.max_working_bytes {
                return Err(invalid(
                    "task max_total_bytes exceeds batch max_working_bytes",
                ));
            }
            Ok((command, weight))
        }) {
            Ok((command, weight)) => queue.push_back((command, report, weight)),
            Err(error) => {
                report.exit_code = 2;
                report.error = Some(error.to_string());
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
        config.max_working_bytes,
        queue.len(),
        queue.iter().map(|(_, _, weight)| *weight).min(),
    );
    let queue = Mutex::new(queue);
    let results = Mutex::new(results);
    let working = Working {
        used: Mutex::new(0),
        ready: Condvar::new(),
        limit: config.max_working_bytes,
    };
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (queue, results, working, pending, sources, reports) = (
                &queue,
                &results,
                &working,
                pending.clone(),
                sources.clone(),
                &reports,
            );
            scope.spawn(move || {
                loop {
                    let item = queue.lock().expect("batch queue lock").pop_front();
                    let Some((command, mut report, weight)) = item else {
                        break;
                    };
                    let index = report.index;
                    let prepared = match working.reserve(weight) {
                        Ok(_reservation) => prepare(
                            command,
                            report,
                            sources.clone(),
                            pending.clone(),
                            config.max_pending_bytes,
                            reports,
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
            });
        }
    });
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
        .checked_mul(std::mem::size_of::<assets::PendingOutput>())
        .ok_or_else(|| invalid("pending output array overflow"))?;
    let pending_used = *pending
        .lock()
        .map_err(|_| invalid("pending budget poisoned"))?;
    if pending_used
        .checked_add(array_reserve)
        .is_none_or(|n| n > config.max_pending_bytes)
    {
        return Err(Error::ResourceLimit {
            resource: "batch pending output aggregation",
            limit: config.max_pending_bytes,
        });
    }
    let mut outputs = Vec::new();
    outputs
        .try_reserve_exact(output_count)
        .map_err(|_| invalid("pending output aggregation allocation failed"))?;
    if outputs
        .capacity()
        .checked_mul(std::mem::size_of::<assets::PendingOutput>())
        .is_none_or(|n| n > array_reserve)
    {
        return Err(Error::ResourceLimit {
            resource: "batch pending output aggregation",
            limit: config.max_pending_bytes,
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
    let limit = config
        .max_report_bytes
        .checked_sub(reserve)
        .ok_or(Error::ResourceLimit {
            resource: "batch report commit reserve",
            limit: config.max_report_bytes,
        })?;
    preflight_document(&document(&tasks, jobs, workers), limit)?;
    let spool_directory = reports.spool.directory()?;
    if !outputs.is_empty() {
        match check_declared_outputs(&outputs, &protected, spool_directory.as_deref())
            .and_then(|()| assets::persist_bundles(&outputs, &sources))
        {
            Ok(()) => {
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
    write_document(output, &document)?;
    writeln!(output)?;
    Ok(exit_code)
}
pub(crate) fn run(args: BatchArgs, output: &mut dyn Write) -> u8 {
    match run_inner(args, output) {
        Ok(code) => code,
        Err(error) => {
            let code = if matches!(error, Error::Io(_) | Error::ResourceLimit { .. }) {
                1
            } else {
                2
            };
            let _ = crate::emit_json(
                output,
                &serde_json::json!({"version":1,"success":false,"exit_code":code,"error":error.to_string(),"tasks":[]}),
            );
            code
        }
    }
}

// Reconstructed isolated regression module from this worker's recorded successful
// patch actions. Not an exact full batch.rs snapshot; never run after recovery.
#[cfg(test)]
mod tests {
    use super::{ReportPayload, ReportSpool, SPOOL_THRESHOLD, captured_json, worker_count};
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
        let output = crate::assets::PendingOutput {
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
            let outputs = [crate::assets::PendingOutput {
                destination,
                bundle,
            }];
            assert!(
                super::check_declared_outputs(&outputs, &BTreeSet::new(), Some(&private)).is_err()
            );
        }
        let outputs = [crate::assets::PendingOutput {
            destination: private.join("other.bdf"),
            bundle: fontpixel::OutputBundle::single(
                fontpixel::ResourcePath::new("other.bdf").unwrap(),
                b"user output".to_vec(),
            ),
        }];
        assert!(super::check_declared_outputs(&outputs, &BTreeSet::new(), Some(&private)).is_err());
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
                super::Kind::Convert,
                super::Kind::Variants,
                super::Kind::Diff,
                super::Kind::Merge,
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
            let outputs = [crate::assets::PendingOutput {
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
        let root = SpoolRoot::new();
        let store = root.store(4096);
        let reports = super::ReportBudget {
            used: Mutex::new(0),
            limit: 4096,
            exceeded: AtomicBool::new(false),
            storage_error: Mutex::new(None),
            spool: store,
        };
        let pending = Arc::new(Mutex::new(0));
        let task = super::prepare(
            crate::Command::Formats { json: false },
            document_task(0, 0, None, None),
            crate::assets::Sources::default(),
            Arc::clone(&pending),
            4096,
            &reports,
            4096,
        );
        assert!(!task.report.success);
        assert_eq!(task.report.exit_code, 1);
        assert!(task.outputs.is_empty());
        assert_eq!(*pending.lock().unwrap(), 0);
        assert!(!super::is_active());
    }
}
