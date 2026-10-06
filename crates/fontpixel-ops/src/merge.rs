//! Ordered multi-part merge CLI with typed diagnostics and transactional output.
//! Reconstructed after the 2026-10-04 workspace replacement; validation pending.
use crate::context::{self, PreparedOutput};
use crate::input_budget::InputBudget;
#[cfg(feature = "clap")]
use crate::options::format_parser;
use crate::options::{
    COutputOptions, ConsoleOptions, PcfOptions, RasterOptions, ReportStyle, VectorOptions,
};
use crate::options::{check_choice, check_parsed, check_range};
use crate::{extra_formats, image_formats, p2_formats, p3_formats};
#[cfg(feature = "clap")]
use clap::{ArgAction, ArgMatches, Args, Command, FromArgMatches, error::ErrorKind};
use fontpixel::merge::{GeometricWidth, MergeSpacing, Selection};
use fontpixel::merge::{MergeOptions, MergePart, MergeReport};
use fontpixel::report::{Impact, Level, Location};
use fontpixel::{Error, Font, FontSet, ReadOptions, Report, ReportEntry, Result};
use serde::{
    Serialize, Serializer,
    ser::{SerializeSeq, SerializeStruct},
};
use std::{
    io::Write,
    mem::size_of,
    path::{Path, PathBuf},
};

#[cfg(feature = "clap")]
const HELP: &str = "Each --take, --dy, --from and --font-index applies to the immediately preceding --part. Each part requires --take. Each --to applies to the immediately preceding -o/--output.\n\nExamples:\n  fontpixel merge --family Mix --part CJK.bdf --take cjk --part Latin.bdf --take western,geometric --dy=-1 -o Mix.bdf -o Mix.woff2 --spacing prop\n  fontpixel merge --family Mix --part input.bdf --take all -o ambiguous.dat --to pcf";

fn family_parser(value: &str) -> std::result::Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err("--family must not be empty".into())
    } else {
        Ok(value.to_owned())
    }
}

#[cfg(feature = "clap")]
#[derive(clap::Args)]
#[command(after_long_help = HELP)]
struct RawMergeArgs {
    #[arg(long = "part", required = true, action = ArgAction::Append)]
    parts: Vec<PathBuf>,
    #[arg(long, action = ArgAction::Append)]
    take: Vec<String>,
    #[arg(long, action = ArgAction::Append, allow_hyphen_values = true)]
    dy: Vec<i64>,
    #[arg(long, action = ArgAction::Append, value_parser = format_parser())]
    from: Vec<String>,
    #[arg(long, action = ArgAction::Append)]
    font_index: Vec<usize>,
    #[arg(short = 'o', long = "output", required = true, action = ArgAction::Append)]
    outputs: Vec<PathBuf>,
    #[arg(long, action = ArgAction::Append, value_parser = format_parser())]
    to: Vec<String>,
    #[command(flatten)]
    settings: MergeSettings,
}

const MERGE_SETTINGS_SPACING: [&str; 3] = ["auto", "mono", "prop"];
const MERGE_SETTINGS_GEOMETRIC_WIDTH: [&str; 2] = ["half", "full"];
/// Merge-wide settings and writer options shared by every output.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct MergeSettings {
    /// Required output family name; surrounding whitespace is trimmed.
    #[cfg_attr(feature = "clap", arg(long, required = true, value_parser = family_parser, help = None::<&str>, long_help = None::<&str>))]
    pub family: String,
    /// Spacing policy: `auto`, `mono` or `prop`.
    #[cfg_attr(feature = "clap", arg(long, default_value = "auto", value_parser = MERGE_SETTINGS_SPACING, help = None::<&str>, long_help = None::<&str>))]
    pub spacing: String,
    /// Width of geometric symbols: `half` or `full`.
    #[cfg_attr(feature = "clap", arg(long, default_value = "half", value_parser = MERGE_SETTINGS_GEOMETRIC_WIDTH, help = None::<&str>, long_help = None::<&str>))]
    pub geometric_width: String,
    /// Accept parts whose baselines or cells do not align.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub allow_misaligned: bool,
    /// Let later parts fill codes missing from earlier parts.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub fallback: bool,
    /// Explicit output ascent in pixels.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(i64).range(0..), help = None::<&str>, long_help = None::<&str>))]
    pub ascent: Option<i64>,
    /// Explicit output descent in pixels.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(i64).range(0..), help = None::<&str>, long_help = None::<&str>))]
    pub descent: Option<i64>,
    /// Reject read, merge or write loss before any output is written.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub strict: bool,
    /// Diagnostic format.
    #[cfg_attr(feature = "clap", arg(long, value_enum, default_value = "text", help = None::<&str>, long_help = None::<&str>))]
    pub report: ReportStyle,
    /// PCF output options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub pcf: PcfOptions,
    /// PSF and FNT/FON output options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub console: ConsoleOptions,
    /// Image atlas and directory output options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub image: image_formats::WriteOptions,
    /// C header output options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub c: COutputOptions,
    /// Additional format output options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub extra: extra_formats::WriteOptions,
    /// Second-priority format output options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub p2: Box<p2_formats::WriteOptions>,
    /// Third-priority format output options, including OTB/bdat metadata.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub p3: Box<p3_formats::WriteOptions>,
    /// Input decoding options and resource limits.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub raster: Box<RasterOptions>,
    /// SVG and OpenType outline output options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub vector: VectorOptions,
}
impl MergeSettings {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        {
            let value = self.family.as_str();
            check_parsed("family", value, family_parser)?;
        }
        {
            let value = self.spacing.as_str();
            check_choice("spacing", value, &MERGE_SETTINGS_SPACING)?;
        }
        {
            let value = self.geometric_width.as_str();
            check_choice("geometric_width", value, &MERGE_SETTINGS_GEOMETRIC_WIDTH)?;
        }
        if let Some(value) = self.ascent {
            check_range("ascent", i128::from(value), 0, i128::from(i64::MAX))?;
        }
        if let Some(value) = self.descent {
            check_range("descent", i128::from(value), 0, i128::from(i64::MAX))?;
        }
        self.pcf.validate()?;
        self.console.validate()?;
        self.image.validate()?;
        self.c.validate()?;
        self.extra.validate()?;
        self.p2.validate()?;
        self.p3.validate()?;
        self.raster.validate()?;
        self.vector.validate()?;
        Ok(())
    }
}
impl Default for MergeSettings {
    /// The command-line defaults; `family` is empty and must be set.
    fn default() -> Self {
        Self {
            family: String::new(),
            spacing: "auto".into(),
            geometric_width: "half".into(),
            allow_misaligned: false,
            fallback: false,
            ascent: None,
            descent: None,
            strict: false,
            report: ReportStyle::Text,
            pcf: PcfOptions::default(),
            console: ConsoleOptions::default(),
            image: image_formats::WriteOptions::default(),
            c: COutputOptions::default(),
            extra: extra_formats::WriteOptions::default(),
            p2: Box::default(),
            p3: Box::default(),
            raster: Box::default(),
            vector: VectorOptions::default(),
        }
    }
}

/// One ordered merge part.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MergeInput {
    /// Input font.
    pub input: PathBuf,
    /// Selected codes, e.g. `cjk` or `western,geometric`.
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_text::selection"))]
    pub take: Selection,
    /// Vertical offset in pixels (positive moves glyphs up).
    #[cfg_attr(feature = "serde", serde(default))]
    pub dy: i64,
    /// Explicit input format.
    #[cfg_attr(feature = "serde", serde(default))]
    pub from: Option<String>,
    /// Zero-based strike, required when the input holds several fonts.
    #[cfg_attr(feature = "serde", serde(default))]
    pub font_index: Option<usize>,
}
/// One merge output.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MergeOutput {
    /// Output entry point.
    pub destination: PathBuf,
    /// Explicit output format; otherwise inferred from the destination.
    #[cfg_attr(feature = "serde", serde(default))]
    pub format: Option<String>,
}
/// Merge selected glyphs from ordered parts and write one or more outputs.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct MergeRequest {
    /// Ordered parts; earlier parts win for overlapping codes.
    pub parts: Vec<MergeInput>,
    /// Outputs, all written in one transaction.
    pub outputs: Vec<MergeOutput>,
    /// Merge-wide settings and writer options.
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub settings: MergeSettings,
}
#[cfg(feature = "clap")]
impl Args for MergeRequest {
    fn augment_args(command: Command) -> Command {
        RawMergeArgs::augment_args(command)
    }
    fn augment_args_for_update(command: Command) -> Command {
        RawMergeArgs::augment_args_for_update(command)
    }
}
#[cfg(feature = "clap")]
impl FromArgMatches for MergeRequest {
    fn from_arg_matches(matches: &ArgMatches) -> std::result::Result<Self, clap::Error> {
        let parts = parse_parts(matches)?;
        let outputs = parse_outputs(matches)?;
        // Ordering has been reconstructed into typed parts/outputs; clap's
        // parallel value arrays are dropped instead of retaining duplicates.
        let raw = RawMergeArgs::from_arg_matches(matches)?;
        let RawMergeArgs { settings, .. } = raw;
        Ok(Self {
            parts,
            outputs,
            settings,
        })
    }
    fn update_from_arg_matches(
        &mut self,
        matches: &ArgMatches,
    ) -> std::result::Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}
impl MergeRequest {
    /// Check the value constraints the command-line parser enforces for every
    /// part, output and setting; operations call this first.
    pub fn validate(&self) -> Result<()> {
        for part in &self.parts {
            if let Some(from) = part.from.as_deref() {
                crate::options::check_format("from", from)?;
            }
        }
        for output in &self.outputs {
            if let Some(format) = output.format.as_deref() {
                crate::options::check_format("format", format)?;
            }
        }
        self.settings.validate()
    }
    /// Presentation selected by `report`.
    pub fn report_style(&self) -> ReportStyle {
        self.settings.report
    }
    /// The original total byte limit, before the standalone identity partition.
    pub fn source_identity_total_bytes(&self) -> usize {
        self.settings.raster.original_total_bytes()
    }
    fn spacing(&self) -> MergeSpacing {
        match self.settings.spacing.as_str() {
            "mono" => MergeSpacing::Mono,
            "prop" => MergeSpacing::Prop,
            _ => MergeSpacing::Auto,
        }
    }
    fn geometric_width(&self) -> GeometricWidth {
        match self.settings.geometric_width.as_str() {
            "full" => GeometricWidth::Full,
            _ => GeometricWidth::Half,
        }
    }
}
#[cfg(feature = "clap")]
fn usage(message: impl Into<String>) -> clap::Error {
    clap::Error::raw(ErrorKind::InvalidValue, message.into())
}

// The original module used this ordered-index approach. Exact original local
// type names and implementation bytes have not survived.
#[cfg(feature = "clap")]
fn indexed<T: Clone + Send + Sync + 'static>(matches: &ArgMatches, id: &str) -> Vec<(usize, T)> {
    match (matches.indices_of(id), matches.get_many::<T>(id)) {
        (Some(indices), Some(values)) => indices.zip(values.cloned()).collect(),
        _ => Vec::new(),
    }
}
#[cfg(feature = "clap")]
enum PartEvent {
    Part(PathBuf),
    Take(String),
    Dy(i64),
    From(String),
    FontIndex(usize),
}
#[cfg(feature = "clap")]
#[derive(Default)]
struct PendingPart {
    input: PathBuf,
    take: Option<Selection>,
    dy: Option<i64>,
    from: Option<String>,
    font_index: Option<usize>,
}
#[cfg(feature = "clap")]
impl PendingPart {
    fn finish(self) -> std::result::Result<MergeInput, clap::Error> {
        Ok(MergeInput {
            input: self.input,
            take: self
                .take
                .ok_or_else(|| usage("every --part requires --take"))?,
            dy: self.dy.unwrap_or(0),
            from: self.from,
            font_index: self.font_index,
        })
    }
}
#[cfg(feature = "clap")]
fn parse_parts(matches: &ArgMatches) -> std::result::Result<Vec<MergeInput>, clap::Error> {
    let mut events = Vec::new();
    events.extend(
        indexed::<PathBuf>(matches, "parts")
            .into_iter()
            .map(|(i, v)| (i, PartEvent::Part(v))),
    );
    events.extend(
        indexed::<String>(matches, "take")
            .into_iter()
            .map(|(i, v)| (i, PartEvent::Take(v))),
    );
    events.extend(
        indexed::<i64>(matches, "dy")
            .into_iter()
            .map(|(i, v)| (i, PartEvent::Dy(v))),
    );
    events.extend(
        indexed::<String>(matches, "from")
            .into_iter()
            .map(|(i, v)| (i, PartEvent::From(v))),
    );
    events.extend(
        indexed::<usize>(matches, "font_index")
            .into_iter()
            .map(|(i, v)| (i, PartEvent::FontIndex(v))),
    );
    events.sort_by_key(|(index, _)| *index);
    let mut result = Vec::new();
    let mut pending: Option<PendingPart> = None;
    for (_, event) in events {
        if let PartEvent::Part(input) = event {
            if let Some(previous) = pending.take() {
                result.push(previous.finish()?);
            }
            pending = Some(PendingPart {
                input,
                ..Default::default()
            });
            continue;
        }
        let part = pending.as_mut().ok_or_else(|| {
            usage("--take, --dy, --from and --font-index require a preceding --part")
        })?;
        match event {
            PartEvent::Take(value) => {
                if part.take.is_some() {
                    return Err(usage("--take may occur once per --part"));
                }
                part.take = Some(Selection::parse(&value).map_err(|e| usage(e.to_string()))?);
            }
            PartEvent::Dy(value) => {
                if part.dy.replace(value).is_some() {
                    return Err(usage("--dy may occur once per --part"));
                }
            }
            PartEvent::From(value) => {
                if part.from.replace(value).is_some() {
                    return Err(usage("--from may occur once per --part"));
                }
            }
            PartEvent::FontIndex(value) => {
                if part.font_index.replace(value).is_some() {
                    return Err(usage("--font-index may occur once per --part"));
                }
            }
            PartEvent::Part(_) => unreachable!(),
        }
    }
    if let Some(part) = pending {
        result.push(part.finish()?);
    }
    if result.is_empty() {
        return Err(usage("at least one --part is required"));
    }
    Ok(result)
}
#[cfg(feature = "clap")]
fn parse_outputs(matches: &ArgMatches) -> std::result::Result<Vec<MergeOutput>, clap::Error> {
    enum Event {
        Output(PathBuf),
        Format(String),
    }
    let mut events = Vec::new();
    events.extend(
        indexed::<PathBuf>(matches, "outputs")
            .into_iter()
            .map(|(i, v)| (i, Event::Output(v))),
    );
    events.extend(
        indexed::<String>(matches, "to")
            .into_iter()
            .map(|(i, v)| (i, Event::Format(v))),
    );
    events.sort_by_key(|(index, _)| *index);
    let mut result: Vec<MergeOutput> = Vec::new();
    for (_, event) in events {
        match event {
            Event::Output(destination) => result.push(MergeOutput {
                destination,
                format: None,
            }),
            Event::Format(format) => {
                let last = result
                    .last_mut()
                    .ok_or_else(|| usage("--to requires a preceding -o/--output"))?;
                if last.format.replace(format).is_some() {
                    return Err(usage("--to may occur once per output"));
                }
            }
        }
    }
    if result.is_empty() {
        return Err(usage("at least one -o/--output is required"));
    }
    Ok(result)
}

fn limit_error(limit: usize) -> Error {
    Error::ResourceLimit {
        resource: "retained merge CLI bytes",
        limit,
    }
}
fn add_bytes(total: &mut usize, bytes: usize, limit: usize) -> Result<()> {
    *total = total.checked_add(bytes).ok_or_else(|| limit_error(limit))?;
    if *total > limit {
        return Err(limit_error(limit));
    }
    Ok(())
}
fn vector_bytes<T>(values: &Vec<T>, total: &mut usize, limit: usize) -> Result<()> {
    add_bytes(
        total,
        values
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or_else(|| limit_error(limit))?,
        limit,
    )
}
fn report_allocations(report: &Report, total: &mut usize, limit: usize) -> Result<()> {
    vector_bytes(&report.entries, total, limit)?;
    for entry in &report.entries {
        add_bytes(total, entry.code.capacity(), limit)?;
        add_bytes(total, entry.message.capacity(), limit)?;
        add_bytes(
            total,
            entry.location.asset.as_ref().map_or(0, String::capacity),
            limit,
        )?;
    }
    Ok(())
}
fn merge_report_bytes(report: &MergeReport, limit: usize) -> Result<usize> {
    let mut total = size_of::<MergeReport>();
    add_bytes(&mut total, report.unicode_version.capacity(), limit)?;
    add_bytes(&mut total, report.family.capacity(), limit)?;
    vector_bytes(&report.parts, &mut total, limit)?;
    for part in &report.parts {
        add_bytes(&mut total, part.label.capacity(), limit)?;
        add_bytes(&mut total, part.selection.capacity(), limit)?;
        vector_bytes(&part.missing_ranges, &mut total, limit)?;
        vector_bytes(&part.missing_blocks, &mut total, limit)?;
        for block in &part.missing_blocks {
            add_bytes(&mut total, block.block.capacity(), limit)?;
        }
    }
    vector_bytes(&report.adjustments, &mut total, limit)?;
    for adjustment in &report.adjustments {
        vector_bytes(&adjustment.codepoints, &mut total, limit)?;
        add_bytes(&mut total, adjustment.reason.capacity(), limit)?;
    }
    vector_bytes(&report.metrics.parts, &mut total, limit)?;
    vector_bytes(&report.licenses, &mut total, limit)?;
    for license in &report.licenses {
        add_bytes(&mut total, license.label.capacity(), limit)?;
        add_bytes(&mut total, license.provenance.capacity(), limit)?;
        for value in [&license.copyright, &license.license, &license.license_url] {
            add_bytes(
                &mut total,
                value.as_ref().map_or(0, String::capacity),
                limit,
            )?;
        }
        vector_bytes(&license.reserved_names, &mut total, limit)?;
        for name in &license.reserved_names {
            add_bytes(&mut total, name.capacity(), limit)?;
        }
    }
    report_allocations(&report.diagnostics, &mut total, limit)?;
    Ok(total)
}
fn reports_bytes(reports: &Vec<Report>, limit: usize) -> Result<usize> {
    let mut total = size_of::<Vec<Report>>();
    vector_bytes(reports, &mut total, limit)?;
    for report in reports {
        report_allocations(report, &mut total, limit)?;
    }
    Ok(total)
}
fn reserve<T>(values: &mut Vec<T>, count: usize, maximum: usize) -> Result<()> {
    let bytes = count
        .checked_mul(size_of::<T>())
        .ok_or_else(|| limit_error(maximum))?;
    if bytes > maximum {
        return Err(limit_error(maximum));
    }
    values
        .try_reserve_exact(count)
        .map_err(|_| limit_error(maximum))
}
fn input_staging(
    parts: usize,
    limits: &fontpixel::ResourceLimits,
) -> Result<(Vec<Font>, Vec<Report>, fontpixel::ResourceLimits)> {
    let maximum = limits.max_total_bytes;
    let headers = size_of::<Vec<Font>>() + size_of::<Vec<Report>>();
    let requested = parts
        .checked_mul(size_of::<Font>() + size_of::<Report>())
        .and_then(|bytes| bytes.checked_add(headers))
        .ok_or_else(|| limit_error(maximum))?;
    if requested > maximum {
        return Err(limit_error(maximum));
    }
    // Check the joint request before either allocation. The actual first array
    // remains charged while reserving the second, including unexpected spare.
    let mut retained = headers;
    let mut fonts = Vec::new();
    reserve(&mut fonts, parts, maximum - retained)?;
    vector_bytes(&fonts, &mut retained, maximum)?;
    let mut reports = Vec::new();
    reserve(&mut reports, parts, maximum - retained)?;
    vector_bytes(&reports, &mut retained, maximum)?;
    let mut available = limits.clone();
    available.max_total_bytes = maximum - retained;
    // InputBudget keeps this deduction through decoding and library merge.
    // Its existing generic reserves/dead-strike charges remain conservative.
    Ok((fonts, reports, available))
}
fn writer_limits(
    limits: &fontpixel::ResourceLimits,
    retained: usize,
) -> Result<fontpixel::formats::TransportLimits> {
    let available = limits
        .max_total_bytes
        .checked_sub(retained)
        .ok_or_else(|| limit_error(limits.max_total_bytes))?;
    let mut resources = limits.clone();
    // `retained` holds the reports, earlier bundles and, for a borrowing
    // output, the model, which the writer is told is reserved.
    resources.max_total_bytes = available;
    Ok(fontpixel::formats::TransportLimits {
        resources,
        max_output_bytes: available,
    })
}
fn combined_error(error: Error, reads: Vec<Report>, merge: Option<Report>) -> Error {
    let (strict, mut last) = match error {
        Error::StrictLoss(report) => (true, report),
        Error::ReportedErrors(report) => (false, report),
        other => return other,
    };
    let mut report = Report::default();
    for read in reads {
        report.extend(read);
    }
    if let Some(merge) = merge {
        report.extend(merge);
    }
    report.entries.append(&mut last.entries);
    if strict {
        Error::StrictLoss(report)
    } else {
        Error::ReportedErrors(report)
    }
}
fn output_format(output: &MergeOutput) -> Result<String> {
    if let Some(format) = &output.format {
        return Ok(format.clone());
    }
    let extension = output
        .destination
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    let special = match extension.as_deref() {
        Some("ttf") => Some("ttf"),
        Some("otf") => Some("otf"),
        Some("ttc" | "otc") => Some("ttc"),
        Some("woff") => Some("woff"),
        Some("woff2") => Some("woff2"),
        _ => None,
    };
    if let Some(id) = special {
        return Ok(id.into());
    }
    Ok(fontpixel::registry()
        .format_from_name(&output.destination.to_string_lossy())?
        .id
        .into())
}
fn is_vector(id: &str) -> bool {
    matches!(
        id,
        "svg" | "ttf" | "otf" | "ttc" | "woff" | "woff2" | "vector-raster"
    )
}
fn writer_registry(
    raw: &MergeSettings,
    id: &str,
    destination: &Path,
    any_vector: bool,
) -> Result<fontpixel::FontRegistry> {
    let mut registry = fontpixel::registry();
    raw.pcf.apply(id, &mut registry)?;
    raw.raster.apply(&mut registry)?;
    // Full vector settings govern vector members in a mixed output group. With
    // no vector target, retain convert's rejection of inapplicable settings.
    if is_vector(id) || !any_vector {
        raw.vector.apply(id, &mut registry)?;
    }
    raw.console
        .apply(id, &raw.raster, raw.vector.gray_threshold, &mut registry)?;
    raw.image
        .apply(id, &raw.raster, raw.vector.gray_threshold, &mut registry)?;
    raw.c
        .apply(id, &raw.raster, raw.vector.gray_threshold, &mut registry)?;
    raw.extra
        .apply(id, &raw.raster, raw.vector.gray_threshold, &mut registry)?;
    raw.p2.apply(
        id,
        destination,
        &raw.raster.p2,
        &raw.raster.input_limits(),
        raw.vector.gray_threshold,
        &mut registry,
    )?;
    raw.p3.apply(
        id,
        &raw.raster.p3,
        &raw.raster.input_limits(),
        raw.vector.gray_threshold,
        &mut registry,
    )?;
    registry.ensure_writable(id)?;
    Ok(registry)
}

/// One asset of a merge output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetSummary {
    /// Asset path relative to the output's directory.
    pub path: String,
    /// Serialized length in bytes.
    pub bytes: usize,
}
/// One written merge output.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSummary {
    /// Total serialized bytes of every asset.
    pub bytes: usize,
    /// Every asset in the output bundle.
    pub assets: Vec<AssetSummary>,
    /// Writer diagnostics of this output.
    pub report: Report,
}
struct CombinedReport<'a> {
    reads: &'a [Report],
    merge: &'a Report,
    written: &'a Report,
}
struct CombinedEntries<'a>(&'a CombinedReport<'a>);
impl Serialize for CombinedEntries<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(None)?;
        for entry in self
            .0
            .reads
            .iter()
            .flat_map(|r| &r.entries)
            .chain(&self.0.merge.entries)
            .chain(&self.0.written.entries)
        {
            sequence.serialize_element(entry)?;
        }
        sequence.end()
    }
}
impl Serialize for CombinedReport<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct("Report", 1)?;
        record.serialize_field("entries", &CombinedEntries(self))?;
        record.end()
    }
}
#[derive(Serialize)]
struct OutputReport<'a> {
    output: &'a Path,
    format: &'a str,
    bytes: usize,
    assets: &'a [AssetSummary],
    report: CombinedReport<'a>,
}
struct OutputReports<'a> {
    specs: &'a [MergeOutput],
    formats: &'a [String],
    summaries: &'a [OutputSummary],
    reads: &'a [Report],
    merge: &'a Report,
}
impl Serialize for OutputReports<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.summaries.len()))?;
        for ((spec, format), summary) in self.specs.iter().zip(self.formats).zip(self.summaries) {
            sequence.serialize_element(&OutputReport {
                output: &spec.destination,
                format,
                bytes: summary.bytes,
                assets: &summary.assets,
                report: CombinedReport {
                    reads: self.reads,
                    merge: self.merge,
                    written: &summary.report,
                },
            })?;
        }
        sequence.end()
    }
}
#[derive(Serialize)]
struct CompleteReport<'a> {
    family: &'a str,
    glyphs: usize,
    merge: &'a MergeReport,
    outputs: OutputReports<'a>,
}

/// A completed merge: the merged model summary and every written output.
#[derive(Debug, Clone)]
pub struct MergeResponse {
    /// Output family name.
    pub family: String,
    /// Glyphs in the merged font.
    pub glyphs: usize,
    /// Per-part selection statistics and merge diagnostics.
    pub merge: MergeReport,
    /// Requested outputs, in order.
    pub outputs: Vec<MergeOutput>,
    /// Resolved format of each output.
    pub formats: Vec<String>,
    /// Written size, assets and writer diagnostics of each output.
    pub summaries: Vec<OutputSummary>,
    /// Read diagnostics of each part.
    pub reads: Vec<Report>,
}
impl MergeResponse {
    /// Write the command-line JSON document.
    pub fn write_json(&self, out: &mut dyn Write, pretty: bool) -> Result<()> {
        crate::json::write(
            out,
            &CompleteReport {
                family: &self.family,
                glyphs: self.glyphs,
                merge: &self.merge,
                outputs: OutputReports {
                    specs: &self.outputs,
                    formats: &self.formats,
                    summaries: &self.summaries,
                    reads: &self.reads,
                    merge: &self.merge.diagnostics,
                },
            },
            pretty,
        )
    }
    /// Write the command-line text report.
    pub fn write_text(&self, out: &mut dyn Write) -> Result<()> {
        for report in &self.reads {
            write!(out, "{report}")?;
        }
        write!(out, "{}", self.merge)?;
        for ((spec, id), summary) in self.outputs.iter().zip(&self.formats).zip(&self.summaries) {
            write!(out, "{}", summary.report)?;
            writeln!(
                out,
                "Wrote {} bytes as {} to {}",
                summary.bytes,
                id,
                spec.destination.display()
            )?;
        }
        Ok(())
    }
    /// Write in the requested style.
    pub fn write(&self, style: ReportStyle, out: &mut dyn Write, pretty: bool) -> Result<()> {
        match style {
            ReportStyle::Json => self.write_json(out, pretty),
            ReportStyle::Text => self.write_text(out),
        }
    }
}
pub(crate) fn run(args: MergeRequest) -> Result<MergeResponse> {
    let spacing = args.spacing();
    let geometric_width = args.geometric_width();
    let MergeRequest {
        parts,
        outputs,
        settings: raw,
    } = args;
    let limits = raw.raster.input_limits();
    if parts.len() > limits.max_resources || outputs.len() > limits.max_resources {
        return Err(Error::ResourceLimit {
            resource: "merge parts/outputs",
            limit: limits.max_resources,
        });
    }
    // Resolve every writer before any input work. No file is opened for output.
    let formats: Vec<String> = outputs.iter().map(output_format).collect::<Result<_>>()?;
    let any_vector = formats.iter().any(|id| is_vector(id));
    for (spec, id) in outputs.iter().zip(&formats) {
        let filename = spec
            .destination
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| Error::InvalidInput("output must have a UTF-8 filename".into()))?;
        fontpixel::ResourcePath::new(filename)?;
        if matches!(raw.report, ReportStyle::Json) && spec.destination.to_str().is_none() {
            return Err(Error::InvalidInput(
                "JSON output paths must be valid UTF-8".into(),
            ));
        }
        drop(writer_registry(&raw, id, &spec.destination, any_vector)?);
    }
    let sources = context::sources();
    let (mut selected_fonts, mut read_reports, read_limits) = input_staging(parts.len(), &limits)?;
    let mut input_budget = InputBudget::new(read_limits);
    let mut registry = fontpixel::registry();
    raw.raster.apply(&mut registry)?;
    let mut selected_bytes = 0usize;
    for (part_index, part) in parts.iter().enumerate() {
        let remaining = input_budget.remaining_limits()?;
        let file = context::open_input(&part.input, &remaining, sources.clone())?;
        let mut loaded = registry.read(
            &file.input(),
            &ReadOptions {
                format: part.from.clone(),
                limits: remaining.clone(),
            },
        )?;
        drop(file);
        let index = match (part.font_index, loaded.fonts.fonts.len()) {
            (Some(index), count) if index < count => index,
            (None, 1) => 0,
            (None, 0) => {
                return Err(Error::InvalidInput(format!(
                    "part {part_index} contains no fonts"
                )));
            }
            (None, count) => {
                return Err(Error::InvalidInput(format!(
                    "part {part_index} contains {count} fonts; select one with --font-index"
                )));
            }
            (Some(index), count) => {
                return Err(Error::InvalidInput(format!(
                    "part {part_index} --font-index {index} is outside {count} fonts"
                )));
            }
        };
        if !loaded.fonts.properties.is_empty() {
            loaded.report.push(ReportEntry {
                level: Level::Warning, impact: Impact::Loss, code: "merge.container_properties".into(),
                location: Location { font_index: Some(index), ..Location::default() },
                message: "Container properties cannot be assigned to the selected merge part and are omitted".into(),
            });
        }
        // Check missing provenance string storage before duplicating it. Actual
        // capacities, including diagnostics and reserve arrays, are admitted next.
        let label = part.input.to_string_lossy();
        let labels = loaded
            .report
            .entries
            .iter()
            .filter(|entry| entry.location.asset.is_none())
            .count();
        let mut decoded_bytes = loaded
            .fonts
            .retained_usage(remaining.max_total_bytes)?
            .bytes;
        report_allocations(
            &loaded.report,
            &mut decoded_bytes,
            remaining.max_total_bytes,
        )?;
        add_bytes(
            &mut decoded_bytes,
            labels
                .checked_mul(label.len())
                .ok_or_else(|| limit_error(remaining.max_total_bytes))?,
            remaining.max_total_bytes,
        )?;
        for entry in &mut loaded.report.entries {
            if entry.location.asset.is_none() {
                entry.location.asset = Some(label.to_string());
            }
        }
        input_budget.admit(&loaded)?;
        let font = loaded
            .fonts
            .fonts
            .into_iter()
            .nth(index)
            .ok_or_else(|| Error::InvalidInput("selected merge part disappeared".into()))?;
        add_bytes(
            &mut selected_bytes,
            font.retained_usage(limits.max_total_bytes)?.bytes,
            limits.max_total_bytes,
        )?;
        selected_fonts.push(font);
        read_reports.push(loaded.report);
    }
    drop(registry);
    if read_reports.iter().any(Report::has_errors) {
        return Err(combined_error(
            Error::ReportedErrors(Report::default()),
            read_reports,
            None,
        ));
    }
    if raw.strict && read_reports.iter().any(Report::has_loss) {
        return Err(combined_error(
            Error::StrictLoss(Report::default()),
            read_reports,
            None,
        ));
    }
    // The library charges the borrowed selected models itself. Restore exactly
    // those already-admitted bytes and original aggregate glyph allowance for
    // its invocation; reader reports and other read reserves stay deducted.
    let remaining = input_budget.remaining_limits()?;
    let mut merge_limits = limits.clone();
    merge_limits.max_total_bytes = remaining
        .max_total_bytes
        .checked_add(selected_bytes)
        .filter(|n| *n <= limits.max_total_bytes)
        .ok_or_else(|| limit_error(limits.max_total_bytes))?;
    let merged = {
        let merge_parts: Vec<_> = parts
            .into_iter()
            .zip(&selected_fonts)
            .map(|(part, font)| {
                let mut source = MergePart::new(font, part.take);
                source.dy = part.dy;
                source
            })
            .collect();
        let options = MergeOptions {
            family: raw.family.clone(),
            spacing,
            geometric_width,
            allow_misaligned: raw.allow_misaligned,
            fallback: raw.fallback,
            ascent: raw.ascent,
            descent: raw.descent,
            strict: raw.strict,
            limits: merge_limits,
        };
        match fontpixel::merge::merge(&merge_parts, &options) {
            Ok(merged) => merged,
            Err(error) => return Err(combined_error(error, read_reports, None)),
        }
    };
    // No writer borrows the originals. Release them before computing the exact
    // remaining live-model/report/output allowance, rather than charging dead data.
    drop(selected_fonts);
    let fonts = FontSet::from(merged.font);
    let model_bytes = fonts.retained_usage(limits.max_total_bytes)?.bytes;
    let mut retained = model_bytes;
    add_bytes(
        &mut retained,
        merge_report_bytes(&merged.report, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    add_bytes(
        &mut retained,
        reports_bytes(&read_reports, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    let mut pending: Vec<PreparedOutput> = Vec::new();
    let mut summaries: Vec<OutputSummary> = Vec::new();
    add_bytes(
        &mut retained,
        size_of::<Vec<PreparedOutput>>(),
        limits.max_total_bytes,
    )?;
    add_bytes(
        &mut retained,
        size_of::<Vec<OutputSummary>>(),
        limits.max_total_bytes,
    )?;
    reserve(
        &mut pending,
        outputs.len(),
        limits.max_total_bytes.saturating_sub(retained),
    )?;
    vector_bytes(&pending, &mut retained, limits.max_total_bytes)?;
    reserve(
        &mut summaries,
        outputs.len(),
        limits.max_total_bytes.saturating_sub(retained),
    )?;
    vector_bytes(&summaries, &mut retained, limits.max_total_bytes)?;
    let glyphs = fonts.fonts[0].len();
    // The last output takes the merged model, so the writer can release it
    // before its later stages; earlier outputs borrow it reserved.
    let mut fonts = Some(fonts);
    let last = outputs.len().saturating_sub(1);
    for (index, (spec, id)) in outputs.iter().zip(&formats).enumerate() {
        let filename = spec
            .destination
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| Error::InvalidInput("output must have a UTF-8 filename".into()))?;
        let mut write_options =
            fontpixel::WriteOptions::new(fontpixel::ResourcePath::new(filename)?);
        write_options.strict = raw.strict;
        let registry = writer_registry(&raw, id, &spec.destination, any_vector)?;
        let result = if index == last {
            // The allowance includes the model, which the owned route (or the
            // registry's reserved fallback) charges exactly and then drops.
            let model = fonts
                .take()
                .ok_or_else(|| limit_error(limits.max_total_bytes))?;
            retained = retained
                .checked_sub(model_bytes)
                .ok_or_else(|| limit_error(limits.max_total_bytes))?;
            write_options.transport_limits = Some(writer_limits(&limits, retained)?);
            registry.write_owned(model, id, &write_options, Report::default())
        } else {
            let model = fonts
                .as_ref()
                .ok_or_else(|| limit_error(limits.max_total_bytes))?;
            write_options.transport_limits = Some(writer_limits(&limits, retained)?);
            registry.write_reserved_with_report(model, id, &write_options, Report::default())
        };
        let written = match result {
            Ok(written) => written,
            Err(error) => {
                return Err(combined_error(
                    error,
                    read_reports,
                    Some(merged.report.diagnostics),
                ));
            }
        };
        drop(registry);
        // PendingOutput's inline bundle is already in pending's charged slots.
        let bundle_heap = written
            .output
            .retained_bytes(limits.max_total_bytes)?
            .checked_sub(size_of::<fontpixel::OutputBundle>())
            .ok_or_else(|| limit_error(limits.max_total_bytes))?;
        add_bytes(&mut retained, bundle_heap, limits.max_total_bytes)?;
        report_allocations(&written.report, &mut retained, limits.max_total_bytes)?;
        let mut assets = Vec::new();
        reserve(
            &mut assets,
            written.output.assets().len(),
            limits.max_total_bytes.saturating_sub(retained),
        )?;
        vector_bytes(&assets, &mut retained, limits.max_total_bytes)?;
        let mut bytes = 0usize;
        for asset in written.output.assets() {
            // String::from copies exactly the visible validated asset path;
            // reserve its bytes before allocation, then charge actual capacity.
            let remaining = limits.max_total_bytes.saturating_sub(retained);
            if asset.path.as_str().len() > remaining {
                return Err(limit_error(limits.max_total_bytes));
            }
            let path = asset.path.as_str().to_owned();
            add_bytes(&mut retained, path.capacity(), limits.max_total_bytes)?;
            bytes = bytes
                .checked_add(asset.bytes.len())
                .ok_or_else(|| limit_error(limits.max_total_bytes))?;
            assets.push(AssetSummary {
                path,
                bytes: asset.bytes.len(),
            });
        }
        if spec.destination.as_os_str().len() > limits.max_total_bytes.saturating_sub(retained) {
            return Err(limit_error(limits.max_total_bytes));
        }
        let destination = spec.destination.clone();
        add_bytes(
            &mut retained,
            destination.capacity(),
            limits.max_total_bytes,
        )?;
        summaries.push(OutputSummary {
            bytes,
            assets,
            report: written.report,
        });
        pending.push(PreparedOutput {
            destination,
            bundle: written.output,
        });
    }
    // Complete every writer and strict check before handing off the output group.
    // Batch defers this call; standalone execution commits the same transaction.
    context::persist_outputs(pending, &sources)?;
    Ok(MergeResponse {
        family: raw.family,
        glyphs,
        merge: merged.report,
        outputs,
        formats,
        summaries,
        reads: read_reports,
    })
}

#[cfg(test)]
mod writer_budget_tests {
    use super::*;

    #[test]
    fn writer_allowance_keeps_all_live_owners_reserved() {
        let limits = fontpixel::ResourceLimits {
            max_total_bytes: 1024,
            max_glyphs: 17,
            max_input_bytes: 8192,
            ..Default::default()
        };
        let retained_model = 256;
        let retained_reports = 128;
        let prior_bundles = 512;
        let allowance =
            writer_limits(&limits, retained_model + retained_reports + prior_bundles).unwrap();
        assert_eq!(allowance.resources.max_total_bytes, 128);
        assert_eq!(allowance.max_output_bytes, 128);
        let mut expected = limits.clone();
        expected.max_total_bytes = 128;
        assert_eq!(allowance.resources, expected);
    }

    #[test]
    fn writer_allowance_has_exact_and_one_byte_short_boundaries() {
        let limits = fontpixel::ResourceLimits {
            max_total_bytes: 4096,
            ..Default::default()
        };
        assert_eq!(writer_limits(&limits, 4095).unwrap().max_output_bytes, 1);
        assert_eq!(
            writer_limits(&limits, 4096)
                .unwrap()
                .resources
                .max_total_bytes,
            0
        );
        assert!(matches!(
            writer_limits(&limits, 4097),
            Err(Error::ResourceLimit {
                resource: "retained merge CLI bytes",
                limit: 4096
            })
        ));
        assert!(writer_limits(&limits, usize::MAX).is_err());
    }

    #[test]
    fn spare_model_capacity_is_not_given_back_to_a_writer() {
        let mut font = Font::default();
        let mut family = String::with_capacity(65536);
        family.push('x');
        font.names.family = Some(family);
        let fonts = FontSet::from(font);
        let model = fonts.retained_usage(usize::MAX).unwrap().bytes;
        let limits = fontpixel::ResourceLimits {
            max_total_bytes: model + 31,
            ..Default::default()
        };
        let allowance = writer_limits(&limits, model).unwrap();
        assert_eq!(allowance.resources.max_total_bytes, 31);
        assert_eq!(allowance.max_output_bytes, 31);
    }
}

#[cfg(test)]
mod input_staging_tests {
    use super::*;

    fn requested(parts: usize) -> usize {
        size_of::<Vec<Font>>()
            + size_of::<Vec<Report>>()
            + parts * (size_of::<Font>() + size_of::<Report>())
    }

    #[test]
    fn staging_arrays_have_exact_and_one_byte_short_boundaries() {
        for parts in [0, 1, 3, 4, 5, 33] {
            let probe_limits = fontpixel::ResourceLimits {
                max_total_bytes: 1_000_000,
                ..Default::default()
            };
            let (fonts, reports, available) = input_staging(parts, &probe_limits).unwrap();
            assert!(fonts.is_empty() && reports.is_empty());
            assert!(fonts.capacity() >= parts && reports.capacity() >= parts);
            let actual = size_of::<Vec<Font>>()
                + size_of::<Vec<Report>>()
                + fonts.capacity() * size_of::<Font>()
                + reports.capacity() * size_of::<Report>();
            assert_eq!(
                probe_limits.max_total_bytes - available.max_total_bytes,
                actual
            );
            drop((fonts, reports));
            let exact = fontpixel::ResourceLimits {
                max_total_bytes: actual,
                ..Default::default()
            };
            let (_, _, remaining) = input_staging(parts, &exact).unwrap();
            assert_eq!(remaining.max_total_bytes, 0);
            let short = fontpixel::ResourceLimits {
                max_total_bytes: actual - 1,
                ..Default::default()
            };
            assert!(input_staging(parts, &short).is_err());
            let one = fontpixel::ResourceLimits {
                max_total_bytes: actual + 1,
                ..Default::default()
            };
            assert_eq!(input_staging(parts, &one).unwrap().2.max_total_bytes, 1);
        }
    }

    #[test]
    fn individually_fitting_arrays_must_fit_together() {
        let parts = 5;
        let headers = size_of::<Vec<Font>>() + size_of::<Vec<Report>>();
        let fonts = parts * size_of::<Font>();
        let reports = parts * size_of::<Report>();
        let limit = headers + fonts.max(reports);
        assert!(headers + fonts <= limit && headers + reports <= limit);
        assert!(headers + fonts + reports > limit);
        let limits = fontpixel::ResourceLimits {
            max_total_bytes: limit,
            ..Default::default()
        };
        assert!(matches!(
            input_staging(parts, &limits),
            Err(Error::ResourceLimit {
                resource: "retained merge CLI bytes",
                ..
            })
        ));
    }

    #[test]
    fn first_decoder_receives_only_post_staging_bytes() {
        let limits = fontpixel::ResourceLimits {
            max_total_bytes: requested(3) + 37,
            max_input_bytes: 99,
            max_resource_bytes: 50,
            max_decompressed_bytes: 19,
            max_table_bytes: 11,
            max_glyphs: 7,
            max_resources: 9,
            ..Default::default()
        };
        let (fonts, reports, available) = input_staging(3, &limits).unwrap();
        let budget = InputBudget::new(available);
        let reader = budget.remaining_limits().unwrap();
        let actual = size_of::<Vec<Font>>()
            + size_of::<Vec<Report>>()
            + fonts.capacity() * size_of::<Font>()
            + reports.capacity() * size_of::<Report>();
        assert_eq!(reader.max_total_bytes, limits.max_total_bytes - actual);
        assert_eq!(reader.max_input_bytes, reader.max_total_bytes.min(99));
        assert_eq!(reader.max_resource_bytes, reader.max_total_bytes.min(50));
        assert_eq!(
            reader.max_decompressed_bytes,
            reader.max_total_bytes.min(19)
        );
        assert_eq!(reader.max_table_bytes, reader.max_total_bytes.min(11));
        assert_eq!(reader.max_glyphs, 7);
        assert_eq!(reader.max_resources, 9);
        assert_eq!(reader.max_pixels_per_glyph, limits.max_pixels_per_glyph);
    }

    #[test]
    fn zero_cap_and_staging_size_overflow_reject_without_reads() {
        let zero = fontpixel::ResourceLimits {
            max_total_bytes: 0,
            ..Default::default()
        };
        assert!(input_staging(0, &zero).is_err());
        assert!(input_staging(1, &zero).is_err());
        assert!(
            input_staging(
                usize::MAX,
                &fontpixel::ResourceLimits {
                    max_total_bytes: usize::MAX,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
