//! The `fontpixel` command-line skeleton. Font codecs are not implemented yet.
#![forbid(unsafe_code)]
use clap::{Parser, Subcommand, ValueEnum, builder::PossibleValuesParser};
use fontpixel::{Error, Input, ReadOptions, ResourceLimits};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    name = "fontpixel",
    version,
    about = "Bitmap font tools (under development; no codecs implemented yet)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all planned formats and their actual implementation status.
    Formats {
        /// Emit machine-readable metadata and capability flags.
        #[arg(long)]
        json: bool,
    },
    /// Inspect a font (dispatch is present; font readers are not implemented).
    Info {
        /// Primary input asset.
        input: PathBuf,
        /// Explicit input format, otherwise use structural detection then hints.
        #[arg(long, value_parser = format_parser())]
        from: Option<String>,
        /// Emit JSON diagnostics (font information is not implemented yet).
        #[arg(long)]
        json: bool,
    },
    /// Convert fonts (dispatch is present; font readers/writers are not implemented).
    Convert {
        /// One or more primary inputs; multi-input model merging arrives with codecs.
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<PathBuf>,
        /// Output entry point. No file is opened until conversion succeeds.
        #[arg(short, long)]
        output: PathBuf,
        /// Explicit input format.
        #[arg(long, value_parser = format_parser())]
        from: Option<String>,
        /// Explicit output format; required if the extension is ambiguous.
        #[arg(long, value_parser = format_parser())]
        to: Option<String>,
        /// Reject any read/write information loss before creating output assets.
        #[arg(long)]
        strict: bool,
        /// Diagnostic format.
        #[arg(long, value_enum, default_value = "text")]
        report: ReportStyle,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportStyle {
    Text,
    Json,
}

fn format_parser() -> PossibleValuesParser {
    PossibleValuesParser::new(
        fontpixel::formats::catalogue::CATALOGUE
            .iter()
            .map(|format| format.id),
    )
}

fn report_style(command: &Command) -> ReportStyle {
    match command {
        Command::Info { json: true, .. } | Command::Formats { json: true } => ReportStyle::Json,
        Command::Convert { report, .. } => *report,
        _ => ReportStyle::Text,
    }
}

fn emit_json(output: &mut dyn Write, value: &serde_json::Value) -> fontpixel::Result<()> {
    serde_json::to_writer_pretty(&mut *output, value)
        .map_err(|error| Error::InvalidInput(format!("cannot serialize JSON: {error}")))?;
    writeln!(output)?;
    Ok(())
}

fn read_primary(path: &Path, limits: &ResourceLimits) -> fontpixel::Result<Vec<u8>> {
    let file = File::open(path)?;
    let maximum = limits.max_input_bytes.min(limits.max_total_bytes);
    let mut bytes = Vec::new();
    file.take((maximum as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(Error::ResourceLimit {
            resource: "primary input bytes",
            limit: maximum,
        });
    }
    Ok(bytes)
}

fn execute(command: Command, output: &mut dyn Write) -> fontpixel::Result<()> {
    let registry = fontpixel::registry();
    match command {
        Command::Formats { json } => {
            let statuses: Vec<_> = registry.formats().collect();
            if json {
                return emit_json(output, &serde_json::json!({ "formats": statuses }));
            }
            writeln!(output, "ID\tREAD\tWRITE\tFEATURE\tEXTENSIONS\tNAME")?;
            for status in statuses {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    status.format.id,
                    if status.read {
                        "implemented"
                    } else {
                        "not implemented"
                    },
                    if status.write {
                        "implemented"
                    } else {
                        "not implemented"
                    },
                    if status.format.feature_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    status
                        .format
                        .extensions
                        .iter()
                        .map(|ext| format!(".{ext}"))
                        .collect::<Vec<_>>()
                        .join(","),
                    status.format.name
                )?;
            }
            Ok(())
        }
        Command::Info {
            input,
            from,
            json: _,
        } => {
            let options = ReadOptions {
                format: from,
                ..ReadOptions::default()
            };
            let bytes = read_primary(&input, &options.limits)?;
            let name = input.to_string_lossy();
            let loaded = registry.read(&Input::new(&bytes).with_name(&name), &options)?;
            let _ = loaded;
            Err(Error::InvalidInput(
                "CLI font inspection/output is not implemented yet".into(),
            ))
        }
        Command::Convert {
            inputs,
            output,
            from,
            to,
            strict: _,
            report: _,
        } => {
            let id = match to {
                Some(id) => id,
                None => registry
                    .format_from_name(&output.to_string_lossy())?
                    .id
                    .into(),
            };
            // Check the target before doing work, and never create/truncate output.
            registry.ensure_writable(&id)?;
            let options = ReadOptions {
                format: from,
                ..ReadOptions::default()
            };
            let first = inputs
                .first()
                .ok_or_else(|| Error::InvalidInput("at least one input is required".into()))?;
            let bytes = read_primary(first, &options.limits)?;
            let name = first.to_string_lossy();
            let loaded = registry.read(&Input::new(&bytes).with_name(&name), &options)?;
            let _ = loaded;
            Err(Error::InvalidInput(
                "CLI font inspection/output is not implemented yet".into(),
            ))
        }
    }
}

fn emit_error(error: &Error, style: ReportStyle, output: &mut dyn Write) -> fontpixel::Result<()> {
    match style {
        ReportStyle::Json => emit_json(
            output,
            &serde_json::json!({
                "error": { "message": error.to_string(), "exit_code": error.exit_code() },
                "report": error.report(),
            }),
        ),
        ReportStyle::Text => {
            writeln!(output, "fontpixel: {error}")?;
            if let Some(report) = error.report() {
                write!(output, "{report}")?;
            }
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let style = report_style(&cli.command);
    match execute(cli.command, &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = emit_error(&error, style, &mut io::stderr().lock());
            ExitCode::from(error.exit_code())
        }
    }
}
