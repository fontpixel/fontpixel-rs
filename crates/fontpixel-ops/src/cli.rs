//! The complete command-line definition over the typed requests.
//!
//! `fontpixel-cli` parses this definition and performs terminal and file I/O;
//! the batch TOML reader parses each task with the same definition so its
//! option handling and messages match the command line exactly.
use crate::options::ReportStyle;
use crate::{
    CharsetRequest, ConvertRequest, DiffRequest, FormatsRequest, InfoRequest, MergeRequest,
    PreviewRequest, RenderRequest, ValidateRequest,
};
#[cfg(feature = "opentype")]
use crate::{SubsetRequest, VariantsRequest};
use clap::{Args, Parser, Subcommand};
use fontpixel::ResourceLimits;
use std::path::PathBuf;

/// Bitmap font inspection and conversion.
#[derive(Debug, Parser)]
#[command(
    name = "fontpixel",
    version,
    about = "Bitmap font inspection and conversion",
    after_help = "Examples:\n  fontpixel info font.bdf --json\n  fontpixel convert font.bdf -o font.pcf\n  fontpixel batch exports.toml > report.json"
)]
pub struct Cli {
    /// The selected command.
    #[command(subcommand)]
    pub command: Command,
}

/// One command-line command and its typed request.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Execute bounded parallel TOML tasks with transactional outputs.
    Batch(BatchArgs),
    /// Merge selected glyphs from ordered font parts.
    Merge(Box<MergeRequest>),
    /// Compare decoded font data.
    Diff(Box<DiffRequest>),
    /// Validate native fonts and supported sfnt structure.
    Validate(Box<ValidateRequest>),
    /// Inspect or transform explicit character mappings.
    Charset(Box<CharsetRequest>),
    /// Combine regional/script faces into a locl-enabled OpenType font.
    #[cfg(feature = "opentype")]
    Variants(Box<VariantsRequest>),
    /// Subset sfnt/WOFF/WOFF2 by text or Unicode ranges.
    #[cfg(feature = "opentype")]
    Subset(Box<SubsetRequest>),
    /// List all planned formats and their actual implementation status.
    #[command(after_help = "Example: fontpixel formats --json")]
    Formats(FormatsRequest),
    /// Inspect a font, including its metrics, properties and diagnostics.
    #[command(after_help = "Example: fontpixel info font.bdf --json")]
    Info(Box<InfoRequest>),
    /// Render text (or --all) as terminal character art.
    #[command(after_help = "Example: fontpixel preview font.bdf Hello --chars '#.'")]
    Preview(Box<PreviewRequest>),
    /// Render text (or --all) to a PNG or SVG image.
    #[command(after_help = "Example: fontpixel render font.bdf Hello -o hello.svg")]
    Render(Box<RenderRequest>),
    /// Convert fonts with loss reporting and safe output replacement.
    #[command(
        visible_alias = "vectorize",
        after_help = "Examples:\n  fontpixel convert input.bdf -o output.pcf\n  fontpixel convert input.bdf -o output.bdf --swidth-ppem 13\n  fontpixel convert one.bdf two.bdf --each --to pcf -o existing-directory\n  fontpixel vectorize input.bdf -o output.ttf --shape square"
    )]
    Convert(Box<ConvertRequest>),
}

/// Arguments of `fontpixel batch`.
#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  fontpixel batch exports.toml --jobs 4 > report.json\n\nTOML schema and transaction rules: docs/w6.1-cli.md. Relative paths use the job file's directory. Tasks cannot depend on another task's output."
)]
pub struct BatchArgs {
    /// Version-1 TOML task file. Every task and the aggregate emit JSON.
    pub file: PathBuf,
    /// Maximum worker threads, 1..64; memory reservations may reduce concurrency.
    #[arg(long,value_parser=clap::value_parser!(u32).range(1..=64))]
    pub jobs: Option<u32>,
}

impl Command {
    /// The diagnostic style used for this command's error document.
    pub fn report_style(&self) -> ReportStyle {
        match self {
            Self::Batch(_) => ReportStyle::Json,
            Self::Merge(args) => args.report_style(),
            Self::Diff(args) => args.report_style(),
            Self::Validate(args) => args.report_style(),
            Self::Charset(args) => args.report,
            #[cfg(feature = "opentype")]
            Self::Variants(args) => args.output.report,
            #[cfg(feature = "opentype")]
            Self::Subset(args) => args.output.report,
            Self::Info(args) if args.json => ReportStyle::Json,
            Self::Formats(args) if args.json => ReportStyle::Json,
            Self::Convert(args) => args.report,
            Self::Preview(args) => args.layout.report,
            Self::Render(args) => args.layout.report,
            _ => ReportStyle::Text,
        }
    }
    /// The original total byte limit, before the standalone identity partition.
    pub fn source_identity_total_bytes(&self) -> usize {
        match self {
            Self::Merge(args) => args.source_identity_total_bytes(),
            Self::Diff(args) => args.source_identity_total_bytes(),
            Self::Validate(args) => args.source_identity_total_bytes(),
            Self::Charset(args) => args.source_identity_total_bytes(),
            #[cfg(feature = "opentype")]
            Self::Variants(args) => args.source_identity_total_bytes(),
            #[cfg(feature = "opentype")]
            Self::Subset(args) => args.source_identity_total_bytes(),
            Self::Info(args) => args.source_identity_total_bytes(),
            Self::Convert(args) => args.source_identity_total_bytes(),
            Self::Preview(args) => args.source_identity_total_bytes(),
            Self::Render(args) => args.source_identity_total_bytes(),
            Self::Formats(_) | Self::Batch(_) => ResourceLimits::default().max_total_bytes,
        }
    }
}
