//! Font inspection and conversion through the feature-gated real codec registry.
//! Parsing, dispatch and terminal/file I/O over the typed `fontpixel-ops` API.
#![forbid(unsafe_code)]
use clap::Parser;
use fontpixel::Error;
use fontpixel_ops::cli::{Cli, Command};
use fontpixel_ops::{ReportStyle, Storage};
use std::{
    io::{self, Write},
    process::ExitCode,
};

/// Pretty JSON, as every standalone command prints it.
const PRETTY: bool = true;

fn emit_json<T: serde::Serialize>(output: &mut dyn Write, value: &T) -> fontpixel::Result<()> {
    fontpixel_ops::json::write(output, value, PRETTY)
}

fn execute(command: Command, output: &mut dyn Write) -> fontpixel::Result<()> {
    let storage = Storage::Filesystem;
    match command {
        Command::Batch(_) => Err(Error::InvalidInput("nested batch is not supported".into())),
        Command::Merge(request) => {
            let style = request.report_style();
            fontpixel_ops::merge(*request, &storage)?
                .response
                .write(style, output, PRETTY)
        }
        Command::Diff(request) => {
            let json = request.json;
            let response = fontpixel_ops::diff(*request, &storage)?;
            if json {
                response.write_json(output, PRETTY)?;
            } else {
                response.write_text(output)?;
            }
            response.status()
        }
        Command::Validate(request) => {
            let json = request.json;
            let response = fontpixel_ops::validate(*request, &storage)?;
            if json {
                response.write_json(output, PRETTY)
            } else {
                response.write_text(output)
            }
        }
        Command::Charset(request) => {
            let style = request.report;
            fontpixel_ops::charset(*request, &storage)?
                .response
                .write(style, output, PRETTY)
        }
        #[cfg(feature = "opentype")]
        Command::Variants(request) => {
            let style = request.output.report;
            fontpixel_ops::variants(*request, &storage)?
                .response
                .write(style, output, PRETTY)
        }
        #[cfg(feature = "opentype")]
        Command::Subset(request) => {
            let style = request.output.report;
            fontpixel_ops::subset(*request, &storage)?
                .response
                .write(style, output, PRETTY)
        }
        Command::Formats(request) => {
            let response = fontpixel_ops::formats(&request);
            if request.json {
                response.write_json(output, PRETTY)
            } else {
                response.write_text(output)
            }
        }
        Command::Info(request) => {
            let json = request.json;
            let response = fontpixel_ops::info(*request, &storage)?;
            if json {
                response.write_json(output, PRETTY)
            } else {
                response.write_text(output, PRETTY)
            }
        }
        Command::Preview(request) => {
            let style = request.layout.report;
            let response = fontpixel_ops::preview(*request, &storage)?;
            // Never interleave diagnostics with the bitmap's stdout stream.
            if !response.report.entries.is_empty() {
                response.write_report(style, &mut io::stderr().lock(), PRETTY)?;
            }
            response.write_text(output)
        }
        Command::Render(request) => {
            let style = request.layout.report;
            fontpixel_ops::render(*request, &storage)?
                .response
                .write(style, output, PRETTY)
        }
        Command::Convert(request) => {
            let style = request.report;
            fontpixel_ops::convert(*request, &storage)?
                .response
                .write(style, output, PRETTY)
        }
    }
}

fn emit_error(error: &Error, style: ReportStyle, output: &mut dyn Write) -> fontpixel::Result<()> {
    match style {
        ReportStyle::Json => {
            #[derive(serde::Serialize)]
            struct ErrorDetails {
                exit_code: u8,
                message: String,
            }
            #[derive(serde::Serialize)]
            struct ErrorSummary<'a> {
                error: ErrorDetails,
                report: Option<&'a fontpixel::Report>,
            }
            emit_json(
                output,
                &ErrorSummary {
                    error: ErrorDetails {
                        exit_code: error.exit_code(),
                        message: error.to_string(),
                    },
                    report: error.report(),
                },
            )
        }
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
    let command = match cli.command {
        Command::Batch(args) => {
            return ExitCode::from(fontpixel_ops::batch::toml::run_file(
                &args.file,
                args.jobs,
                &mut io::stdout().lock(),
            ));
        }
        command => command,
    };
    let style = command.report_style();
    match execute(command, &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = emit_error(&error, style, &mut io::stderr().lock());
            ExitCode::from(error.exit_code())
        }
    }
}
