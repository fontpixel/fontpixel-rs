//! Version-1 TOML job files. Task options are command-line flags: each task is
//! converted to an argument list and parsed by the command-line definition, so
//! option validation and error text match `fontpixel <kind>` exactly.
use super::{BatchRequest, BatchResponse, BatchTask, TaskAction, TaskKind, TaskRequest};
use crate::cli::{Cli, Command};
use crate::memory::Storage;
use clap::Parser;
use fontpixel::{Error, Result};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
};
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
    kind: TaskKind,
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
    fn command(&self, base: &Path) -> Result<Command> {
        if self.id.trim().is_empty() || self.id.len() > 256 {
            return Err(invalid("task id must contain 1..256 UTF-8 bytes"));
        }
        let mut args = vec![OsString::from("fontpixel"), self.kind.name().into()];
        let merge = self.kind == TaskKind::Merge;
        let variants = self.kind == TaskKind::Variants;
        let readonly = matches!(self.kind, TaskKind::Validate | TaskKind::Diff);
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
            && !matches!(self.kind, TaskKind::Convert | TaskKind::Vectorize)
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
            let required = if self.kind == TaskKind::Diff { 2 } else { 1 };
            if self.inputs.len() < required
                || (!matches!(self.kind, TaskKind::Convert | TaskKind::Vectorize)
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
impl Task {
    /// Every declared path in protection order: inputs, base, language and
    /// script fonts, merge parts, then `text_file` options.
    fn declared_inputs(&self, base: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self
            .inputs
            .iter()
            .chain(self.base.iter())
            .chain(self.languages.values())
            .chain(self.scripts.values())
            .chain(self.parts.iter().map(|p| &p.input))
            .map(|input| path(base, input))
            .collect();
        for (key, value) in &self.options {
            if key.replace('_', "-") == "text-file" {
                match value {
                    toml::Value::String(p) => paths.push(path(base, Path::new(p))),
                    toml::Value::Array(values) => {
                        for value in values {
                            if let toml::Value::String(p) = value {
                                paths.push(path(base, Path::new(p)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        paths
    }
    fn action(&self, base: &Path) -> TaskAction {
        let prepared = self.command(base).and_then(|command| {
            let weight = self.weight()?;
            Ok((task_request(command)?, weight))
        });
        match prepared {
            Ok((request, weight)) => TaskAction::Run { request, weight },
            Err(error) => TaskAction::Invalid(error.to_string()),
        }
    }
}
fn task_request(command: Command) -> Result<TaskRequest> {
    Ok(match command {
        Command::Convert(request) => TaskRequest::Convert(request),
        #[cfg(feature = "opentype")]
        Command::Variants(request) => TaskRequest::Variants(request),
        #[cfg(feature = "opentype")]
        Command::Subset(request) => TaskRequest::Subset(request),
        Command::Merge(request) => TaskRequest::Merge(request),
        Command::Charset(request) => TaskRequest::Charset(request),
        Command::Validate(request) => TaskRequest::Validate(request),
        Command::Diff(request) => TaskRequest::Diff(request),
        _ => return Err(invalid("command is not a batch task")),
    })
}
/// Parse a version-1 job file. Relative paths resolve against `base` (the
/// job file's directory). Tasks that fail preparation become
/// [`TaskAction::Invalid`] and are reported with exit code 2 when run.
pub fn parse(text: &str, base: &Path) -> Result<BatchRequest> {
    let config: JobFile =
        toml::from_str(text).map_err(|e| invalid(format!("invalid batch TOML: {e}")))?;
    if config.version != 1 {
        return Err(invalid("only batch version=1 is supported"));
    }
    if config.tasks.is_empty() || config.tasks.len() > 10_000 {
        return Err(invalid("batch must contain 1..10000 tasks"));
    }
    let tasks = config
        .tasks
        .iter()
        .map(|task| BatchTask {
            id: task.id.clone(),
            kind: task.kind,
            declared_inputs: task.declared_inputs(base),
            action: task.action(base),
        })
        .collect();
    Ok(BatchRequest {
        jobs: config.jobs,
        max_working_bytes: config.max_working_bytes,
        max_pending_bytes: config.max_pending_bytes,
        max_report_bytes: config.max_report_bytes,
        max_task_report_bytes: config.max_task_report_bytes,
        protected_paths: Vec::new(),
        tasks,
    })
}
/// Parse a job file's text and run it in `storage`. `jobs` overrides the
/// file's worker ceiling.
pub fn run_text(
    text: &str,
    base: &Path,
    jobs: Option<usize>,
    storage: &Storage,
    output: &mut dyn Write,
) -> Result<BatchResponse> {
    let mut request = parse(text, base)?;
    request.jobs = jobs.or(request.jobs);
    super::run(request, storage, output)
}
/// Run a job file from the filesystem exactly as `fontpixel batch` does,
/// including the whole-batch failure document; returns the exit code.
#[cfg(feature = "fs")]
pub fn run_file(file: &Path, jobs: Option<u32>, output: &mut dyn Write) -> u8 {
    match run_file_inner(file, jobs, output) {
        Ok(code) => code,
        Err(error) => super::write_failure(output, &error),
    }
}
#[cfg(feature = "fs")]
fn run_file_inner(file: &Path, jobs: Option<u32>, output: &mut dyn Write) -> Result<u8> {
    let sources = crate::context::Sources::default();
    let absolute = if file.is_absolute() {
        file.to_owned()
    } else {
        std::env::current_dir()?.join(file)
    };
    let base = absolute.parent().unwrap_or(Path::new("."));
    let text = super::load_text(&absolute, &sources)?;
    let mut request = parse(&text, base)?;
    drop(text);
    request.jobs = jobs.map(|v| v as usize).or(request.jobs);
    request.protected_paths.insert(0, absolute.clone());
    Ok(super::run_with_sources(request, &Storage::Filesystem, sources, output)?.exit_code)
}
