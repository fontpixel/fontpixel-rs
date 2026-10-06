//! `convert`/`vectorize`: decode ordered inputs, apply names and projections,
//! and write one output (or one per input with `each`) as a single transaction.
use crate::context::{self, PreparedOutput};
use crate::options::check_format;
use crate::options::{
    COutputOptions, ConsoleOptions, PcfOptions, RasterOptions, ReportStyle, VectorOptions,
};
use crate::{
    commands, extra_formats, image_formats, input_budget, metadata, p2_formats, p3_formats,
};
use fontpixel::{Error, ReadOptions, ResourceLimits};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Convert one or more inputs into one output, or each input separately.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ConvertRequest {
    /// One or more inputs, combined as ordered strikes in a FontSet.
    #[cfg_attr(feature = "clap", arg(required = true, num_args = 1..))]
    pub inputs: Vec<PathBuf>,
    /// Output entry point. No file is opened until conversion succeeds.
    #[cfg_attr(feature = "clap", arg(short, long))]
    pub output: PathBuf,
    /// Explicit input format.
    #[cfg_attr(feature = "clap", arg(long, value_parser = crate::options::format_parser()))]
    pub from: Option<String>,
    /// Explicit output format; required if the extension is ambiguous.
    #[cfg_attr(feature = "clap", arg(long, value_parser = crate::options::format_parser()))]
    pub to: Option<String>,
    /// Reject any read/write information loss before creating output assets.
    #[cfg_attr(feature = "clap", arg(long))]
    pub strict: bool,
    /// Convert each input separately into an existing output directory; requires --to.
    #[cfg_attr(feature = "clap", arg(long, requires = "to"))]
    pub each: bool,
    /// Select a zero-based combined strike, or one strike from each input with --each.
    #[cfg_attr(feature = "clap", arg(long))]
    pub font_index: Option<usize>,
    /// Normalized naming overrides.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub names: metadata::NameOptions,
    /// Size, resolution and metric projection options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub projection: metadata::ProjectionOptions,
    /// Diagnostic format.
    #[cfg_attr(feature = "clap", arg(long, value_enum, default_value = "text"))]
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
impl ConvertRequest {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.from.as_deref() {
            check_format("from", value)?;
        }
        if let Some(value) = self.to.as_deref() {
            check_format("to", value)?;
        }
        self.names.validate()?;
        self.projection.validate()?;
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
impl ConvertRequest {
    /// The original total byte limit, before the standalone identity partition.
    pub fn source_identity_total_bytes(&self) -> usize {
        self.raster.original_total_bytes()
    }
}

/// Outputs of one conversion, in group order.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertResponse {
    /// Whether each input was converted separately.
    pub each: bool,
    /// One summary per written output.
    pub outputs: Vec<ConversionSummary>,
}
impl ConvertResponse {
    /// Write the command-line JSON document (an `outputs` list with `each`).
    pub fn write_json(&self, out: &mut dyn Write, pretty: bool) -> fontpixel::Result<()> {
        if self.each {
            crate::json::write(
                out,
                &ConversionList {
                    outputs: &self.outputs,
                },
                pretty,
            )
        } else {
            let first = self
                .outputs
                .first()
                .ok_or_else(|| Error::InvalidInput("conversion produced no output".into()))?;
            crate::json::write(out, first, pretty)
        }
    }
    /// Write the command-line text report.
    pub fn write_text(&self, out: &mut dyn Write) -> fontpixel::Result<()> {
        for summary in &self.outputs {
            write!(out, "{}", summary.report)?;
            writeln!(
                out,
                "Wrote {} bytes to {}",
                summary.bytes,
                summary.output.display()
            )?;
        }
        Ok(())
    }
    /// Write in the requested style.
    pub fn write(
        &self,
        style: ReportStyle,
        out: &mut dyn Write,
        pretty: bool,
    ) -> fontpixel::Result<()> {
        match style {
            ReportStyle::Json => self.write_json(out, pretty),
            ReportStyle::Text => self.write_text(out),
        }
    }
}

pub(crate) fn run(request: ConvertRequest) -> fontpixel::Result<ConvertResponse> {
    let ConvertRequest {
        inputs,
        output: destination,
        from,
        to,
        strict,
        each,
        font_index,
        names,
        projection,
        report: _,
        pcf,
        console,
        image,
        c,
        extra,
        p2,
        p3,
        raster,
        vector,
    } = request;
    let registry = fontpixel::registry();
    if inputs.len() > 1024 {
        return Err(Error::InvalidInput(
            "convert accepts at most 1024 inputs".into(),
        ));
    }
    let names = names.into_overrides();
    let limits = raster.input_limits();
    let id = match to {
        Some(id) => id,
        None if each => {
            return Err(Error::InvalidInput(
                "--each requires an explicit --to format".into(),
            ));
        }
        None => infer_output_format(&registry, &destination)?,
    };
    registry.ensure_writable(&id)?;
    let groups = conversion_groups(&inputs, &destination, &id, each, limits.max_total_bytes)?;
    let slots = groups
        .len()
        .checked_mul(
            std::mem::size_of::<PreparedOutput>() + std::mem::size_of::<ConversionSummary>(),
        )
        .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
    let mut reservation = groups
        .capacity()
        .checked_mul(std::mem::size_of::<ConversionGroup>())
        .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
    for group in &groups {
        conversion_charge(
            &mut reservation,
            group.output.capacity(),
            limits.max_total_bytes,
        )?;
    }
    conversion_charge(&mut reservation, slots, limits.max_total_bytes)?;
    let mut pending = Vec::new();
    pending
        .try_reserve_exact(groups.len())
        .map_err(|_| conversion_limit(limits.max_total_bytes))?;
    let mut summaries = Vec::new();
    summaries
        .try_reserve_exact(groups.len())
        .map_err(|_| conversion_limit(limits.max_total_bytes))?;
    let mut retained = groups
        .capacity()
        .checked_mul(std::mem::size_of::<ConversionGroup>())
        .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
    conversion_charge(
        &mut retained,
        pending
            .capacity()
            .checked_mul(std::mem::size_of::<PreparedOutput>())
            .ok_or_else(|| conversion_limit(limits.max_total_bytes))?,
        limits.max_total_bytes,
    )?;
    conversion_charge(
        &mut retained,
        summaries
            .capacity()
            .checked_mul(std::mem::size_of::<ConversionSummary>())
            .ok_or_else(|| conversion_limit(limits.max_total_bytes))?,
        limits.max_total_bytes,
    )?;
    for group in &groups {
        conversion_charge(
            &mut retained,
            group.output.capacity(),
            limits.max_total_bytes,
        )?;
    }
    // Explicit OTB/bdat metadata strings and the name table built from
    // them stay live through every group; reserve them before reading.
    #[cfg(any(feature = "otb", feature = "bdat"))]
    conversion_charge(
        &mut retained,
        p3.reserved_bytes(limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    let sources = context::sources();
    let mut allow_single_in_place = false;
    for group in &groups {
        let group_limits = conversion_remaining(&limits, retained)?;
        let mut registry = fontpixel::registry();
        pcf.apply(&id, &mut registry)?;
        raster.apply(&mut registry)?;
        vector.apply(&id, &mut registry)?;
        console.apply(&id, &raster, vector.gray_threshold, &mut registry)?;
        image.apply(&id, &raster, vector.gray_threshold, &mut registry)?;
        c.apply(&id, &raster, vector.gray_threshold, &mut registry)?;
        extra.apply(&id, &raster, vector.gray_threshold, &mut registry)?;
        p2.apply(
            &id,
            &group.output,
            &raster.p2,
            &group_limits,
            vector.gray_threshold,
            &mut registry,
        )?;
        p3.apply(
            &id,
            &raster.p3,
            &group_limits,
            vector.gray_threshold,
            &mut registry,
        )?;
        registry.ensure_writable(&id)?;
        let mut fonts = fontpixel::FontSet::default();
        let mut report = fontpixel::Report::default();
        let mut input_budget = input_budget::InputBudget::new(group_limits.clone());
        for input in &inputs[group.start..group.end] {
            let options = ReadOptions {
                format: from.clone(),
                limits: input_budget.remaining_limits()?,
            };
            let file = context::open_input(input, &options.limits, sources.clone())?;
            let mut loaded = registry.read(&file.input(), &options)?;
            drop(file);
            input_budget.admit(&loaded)?;
            for entry in &mut loaded.report.entries {
                if let Some(index) = &mut entry.location.font_index {
                    *index = index
                        .checked_add(fonts.fonts.len())
                        .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
                }
            }
            report.extend(loaded.report);
            if group.end - group.start == 1 {
                fonts.source_format = loaded.fonts.source_format.take();
            }
            fonts.fonts.extend(loaded.fonts.fonts);
            for property in loaded.fonts.properties.into_vec() {
                fonts.properties.push(property);
            }
        }
        if let Some(index) = font_index {
            if index >= fonts.fonts.len() {
                return Err(Error::InvalidInput(format!(
                    "font index {index} is out of range for {}",
                    group.output.display()
                )));
            }
            let selected = fonts
                .fonts
                .into_iter()
                .nth(index)
                .ok_or_else(|| Error::InvalidInput("selected font is unavailable".into()))?;
            fonts.fonts = vec![selected];
        }
        metadata::apply(&mut fonts, &names, &mut report, &group_limits)?;
        metadata::project(&mut fonts, &projection, &mut report, &group_limits)?;
        let filename = group
            .output
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::InvalidInput("output must have a UTF-8 filename".into()))?;
        let mut write_options =
            fontpixel::WriteOptions::new(fontpixel::ResourcePath::new(filename)?);
        write_options.strict = strict;
        // Reserve incoming diagnostics in addition to earlier --each
        // outputs/reports already removed from group_limits, then move
        // the model into the writer. The allowance passed on includes
        // the model: outline writers charge its exact retained usage
        // on their own ledger and drop it before encoding tables or
        // the webfont envelope; every other writer has it reserved by
        // the registry before the borrowing route. No guessed credit.
        let (writer_limits, report) =
            commands::caller_report_limits(&group_limits, report, strict)?;
        write_options.transport_limits = Some(fontpixel::formats::TransportLimits {
            max_output_bytes: writer_limits.max_total_bytes,
            resources: writer_limits,
        });
        let written = registry.write_owned(fonts, &id, &write_options, report)?;
        let bytes = written.output.assets().iter().try_fold(0usize, |n, a| {
            n.checked_add(a.bytes.len())
                .ok_or_else(|| conversion_limit(limits.max_total_bytes))
        })?;
        allow_single_in_place = !context::is_active()
            && !each
            && inputs.len() == 1
            && inputs[0] == group.output
            && !written.output.is_directory()
            && written.output.assets().len() == 1
            && sources.is_single_primary();
        conversion_charge(
            &mut retained,
            written
                .output
                .retained_bytes(limits.max_total_bytes)?
                .saturating_sub(std::mem::size_of::<fontpixel::OutputBundle>()),
            limits.max_total_bytes,
        )?;
        conversion_charge(
            &mut retained,
            conversion_report_bytes(&written.report, limits.max_total_bytes)?
                .saturating_sub(std::mem::size_of::<fontpixel::Report>()),
            limits.max_total_bytes,
        )?;
        let path_bytes = written.output.assets().iter().try_fold(0usize, |n, a| {
            n.checked_add(a.path.as_str().len())
                .ok_or_else(|| conversion_limit(limits.max_total_bytes))
        })?;
        let summary_slots = written
            .output
            .assets()
            .len()
            .checked_mul(std::mem::size_of::<AssetSummary>())
            .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
        conversion_charge(
            &mut retained,
            path_bytes
                .checked_add(summary_slots)
                .ok_or_else(|| conversion_limit(limits.max_total_bytes))?,
            limits.max_total_bytes,
        )?;
        conversion_charge(
            &mut retained,
            group
                .output
                .as_os_str()
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(id.len()))
                .ok_or_else(|| conversion_limit(limits.max_total_bytes))?,
            limits.max_total_bytes,
        )?;
        let mut assets = Vec::new();
        assets
            .try_reserve_exact(written.output.assets().len())
            .map_err(|_| conversion_limit(limits.max_total_bytes))?;
        assets.extend(written.output.assets().iter().map(|a| AssetSummary {
            bytes: a.bytes.len(),
            path: a.path.as_str().to_owned(),
        }));
        let summary = ConversionSummary {
            assets,
            bytes,
            format: id.clone(),
            output: group.output.clone(),
            report: written.report,
        };
        let destination = group.output.clone();
        let estimated = path_bytes
            .checked_add(summary_slots)
            .and_then(|n| n.checked_add(group.output.as_os_str().len() * 2 + id.len()))
            .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
        let mut actual = summary
            .assets
            .capacity()
            .checked_mul(std::mem::size_of::<AssetSummary>())
            .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
        for asset in &summary.assets {
            conversion_charge(&mut actual, asset.path.capacity(), limits.max_total_bytes)?;
        }
        conversion_charge(
            &mut actual,
            summary.format.capacity(),
            limits.max_total_bytes,
        )?;
        conversion_charge(
            &mut actual,
            summary.output.capacity(),
            limits.max_total_bytes,
        )?;
        conversion_charge(&mut actual, destination.capacity(), limits.max_total_bytes)?;
        if actual > estimated {
            conversion_charge(&mut retained, actual - estimated, limits.max_total_bytes)?;
        }
        pending.push(PreparedOutput {
            destination,
            bundle: written.output,
        });
        summaries.push(summary);
    }
    let protection = if allow_single_in_place {
        context::Sources::default()
    } else {
        sources
    };
    context::persist_outputs(pending, &protection)?;
    Ok(ConvertResponse {
        each,
        outputs: summaries,
    })
}

/// One asset of a written output.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AssetSummary {
    /// Serialized length in bytes.
    pub bytes: usize,
    /// Asset path relative to the output's directory.
    pub path: String,
}
/// Summary of one written output.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ConversionSummary {
    /// Every asset in the output bundle.
    pub assets: Vec<AssetSummary>,
    /// Total serialized bytes of every asset.
    pub bytes: usize,
    /// Output format identifier.
    pub format: String,
    /// Output entry point.
    pub output: PathBuf,
    /// Complete read, edit and write diagnostics.
    pub report: fontpixel::Report,
}
#[derive(serde::Serialize)]
struct ConversionList<'a> {
    outputs: &'a [ConversionSummary],
}
struct ConversionGroup {
    start: usize,
    end: usize,
    output: PathBuf,
}
fn conversion_limit(limit: usize) -> Error {
    Error::ResourceLimit {
        resource: "retained conversion output/report bytes",
        limit,
    }
}
pub(crate) fn conversion_charge(
    used: &mut usize,
    bytes: usize,
    limit: usize,
) -> fontpixel::Result<()> {
    *used = used
        .checked_add(bytes)
        .filter(|n| *n <= limit)
        .ok_or_else(|| conversion_limit(limit))?;
    Ok(())
}
fn conversion_remaining(limits: &ResourceLimits, used: usize) -> fontpixel::Result<ResourceLimits> {
    let mut remaining = limits.clone();
    remaining.max_total_bytes = remaining
        .max_total_bytes
        .checked_sub(used)
        .ok_or_else(|| conversion_limit(limits.max_total_bytes))?;
    remaining.max_input_bytes = remaining.max_input_bytes.min(remaining.max_total_bytes);
    remaining.max_resource_bytes = remaining.max_resource_bytes.min(remaining.max_total_bytes);
    remaining.max_decompressed_bytes = remaining
        .max_decompressed_bytes
        .min(remaining.max_total_bytes);
    remaining.max_table_bytes = remaining.max_table_bytes.min(remaining.max_total_bytes);
    Ok(remaining)
}
pub(crate) fn conversion_report_bytes(
    report: &fontpixel::Report,
    limit: usize,
) -> fontpixel::Result<usize> {
    let mut bytes = std::mem::size_of::<fontpixel::Report>();
    conversion_charge(
        &mut bytes,
        report
            .entries
            .capacity()
            .checked_mul(std::mem::size_of::<fontpixel::ReportEntry>())
            .ok_or_else(|| conversion_limit(limit))?,
        limit,
    )?;
    for entry in &report.entries {
        conversion_charge(&mut bytes, entry.code.capacity(), limit)?;
        conversion_charge(&mut bytes, entry.message.capacity(), limit)?;
        conversion_charge(
            &mut bytes,
            entry.location.asset.as_ref().map_or(0, String::capacity),
            limit,
        )?;
    }
    Ok(bytes)
}
/// The output format `convert` infers for a destination path: the OpenType
/// container extensions first, then the registry's filename hints.
pub fn output_format_for(destination: &Path) -> fontpixel::Result<String> {
    infer_output_format(&fontpixel::registry(), destination)
}
fn infer_output_format(
    registry: &fontpixel::FontRegistry,
    destination: &Path,
) -> fontpixel::Result<String> {
    Ok(match destination
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("ttf") => "ttf",
        Some("otf") => "otf",
        Some("ttc" | "otc") => "ttc",
        Some("woff") => "woff",
        Some("woff2") => "woff2",
        _ => {
            registry
                .format_from_name(&destination.to_string_lossy())?
                .id
        }
    }
    .into())
}
fn conversion_groups(
    inputs: &[PathBuf],
    destination: &Path,
    id: &str,
    each: bool,
    limit: usize,
) -> fontpixel::Result<Vec<ConversionGroup>> {
    if !each {
        let mut used = std::mem::size_of::<ConversionGroup>();
        conversion_charge(&mut used, destination.as_os_str().len(), limit)?;
        return Ok(vec![ConversionGroup {
            start: 0,
            end: inputs.len(),
            output: destination.to_owned(),
        }]);
    }
    if !context::is_output_directory(destination)? {
        return Err(Error::InvalidInput(
            "--each output must be an existing directory".into(),
        ));
    }
    let extension = fontpixel::formats::catalogue::CATALOGUE
        .iter()
        .find(|f| f.id == id)
        .and_then(|f| f.extensions.first())
        .copied()
        .unwrap_or(id);
    // Check repeated parent/path copies and collision-set scratch before building
    // any destination. A long output parent multiplied by many inputs is bounded.
    let mut planned = inputs
        .len()
        .checked_mul(std::mem::size_of::<ConversionGroup>() + 128)
        .ok_or_else(|| conversion_limit(limit))?;
    for input in inputs {
        let name = input
            .file_name()
            .ok_or_else(|| Error::InvalidInput("--each input requires a filename".into()))?;
        let path_bytes = destination
            .as_os_str()
            .len()
            .checked_add(name.len())
            .and_then(|n| n.checked_add(extension.len() + 2))
            .and_then(|n| n.checked_mul(6))
            .ok_or_else(|| conversion_limit(limit))?;
        conversion_charge(&mut planned, path_bytes, limit)?;
    }
    let mut paths = std::collections::BTreeSet::new();
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(inputs.len())
        .map_err(|_| conversion_limit(limit))?;
    for (index, input) in inputs.iter().enumerate() {
        let filename = input
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::InvalidInput("--each input requires a UTF-8 filename".into()))?;
        let filename = filename.strip_suffix(".gz").unwrap_or(filename);
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::InvalidInput("--each cannot derive an output filename".into()))?;
        let output = destination.join(format!("{stem}.{extension}"));
        if !paths.insert(output.clone()) {
            return Err(Error::InvalidInput(
                "--each inputs produce duplicate destination names".into(),
            ));
        }
        groups.push(ConversionGroup {
            start: index,
            end: index + 1,
            output,
        });
    }
    Ok(groups)
}
