//! Font inspection and conversion through the feature-gated real codec registry.
#![forbid(unsafe_code)]
use clap::{Args, Parser, Subcommand, ValueEnum, builder::PossibleValuesParser};
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
    about = "Bitmap font inspection and conversion"
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
    /// Inspect a font, including its metrics, properties and diagnostics.
    Info {
        /// Primary input asset.
        input: PathBuf,
        /// Explicit input format, otherwise use structural detection then hints.
        #[arg(long, value_parser = format_parser())]
        from: Option<String>,
        /// Emit machine-readable font information and diagnostics.
        #[arg(long)]
        json: bool,
    },
    /// Convert fonts with loss reporting and safe output replacement.
    Convert {
        /// One or more inputs, combined as ordered strikes in a FontSet.
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
        #[command(flatten)]
        pcf: PcfArguments,
    },
}

#[derive(Args, Default)]
struct PcfArguments {
    /// PCF table integer byte order.
    #[arg(long = "byteorder", alias = "byte-order", value_enum)]
    byte_order: Option<Significance>,
    /// PCF bitmap bit order.
    #[arg(long = "bitorder", alias = "bit-order", value_enum)]
    bit_order: Option<Significance>,
    /// PCF row alignment in bytes (1, 2, 4 or 8).
    #[arg(long = "glyphpad", alias = "glyph-pad", value_parser = ["1", "2", "4", "8"])]
    glyph_pad: Option<String>,
    /// PCF bitmap scan unit in bytes (1, 2 or 4).
    #[arg(long = "scanunit", alias = "scan-unit", value_parser = ["1", "2", "4"])]
    scan_unit: Option<String>,
}
#[derive(Clone, Copy, ValueEnum)]
enum Significance {
    Msb,
    Lsb,
}
impl PcfArguments {
    fn apply(&self, id: &str, registry: &mut fontpixel::FontRegistry) -> fontpixel::Result<()> {
        let requested = self.byte_order.is_some()
            || self.bit_order.is_some()
            || self.glyph_pad.is_some()
            || self.scan_unit.is_some();
        if !requested {
            return Ok(());
        }
        if id != "pcf" {
            return Err(Error::InvalidInput(
                "PCF layout options require --to pcf".into(),
            ));
        }
        #[cfg(feature = "pcf")]
        {
            use fontpixel::formats::pcf::{GlyphPad, Order, PcfWriteOptions, ScanUnit};
            let order = |v| match v {
                Some(Significance::Lsb) => Order::Lsb,
                _ => Order::Msb,
            };
            registry.set_pcf_write_options(PcfWriteOptions {
                byte_order: order(self.byte_order),
                bit_order: order(self.bit_order),
                glyph_pad: match self.glyph_pad.as_deref() {
                    Some("1") => GlyphPad::One,
                    Some("2") => GlyphPad::Two,
                    Some("8") => GlyphPad::Eight,
                    _ => GlyphPad::Four,
                },
                scan_unit: match self.scan_unit.as_deref() {
                    Some("2") => ScanUnit::Two,
                    Some("4") => ScanUnit::Four,
                    _ => ScanUnit::One,
                },
                ..PcfWriteOptions::default()
            })
        }
        #[cfg(not(feature = "pcf"))]
        {
            let _ = registry;
            Err(Error::FeatureDisabled {
                format: "pcf".into(),
            })
        }
    }
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
    let mut registry = fontpixel::registry();
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
        Command::Info { input, from, json } => {
            let options = ReadOptions {
                format: from,
                ..ReadOptions::default()
            };
            let bytes = read_primary(&input, &options.limits)?;
            let name = input.to_string_lossy();
            let loaded = registry.read(&Input::new(&bytes).with_name(&name), &options)?;
            let fonts: Vec<_> = loaded.fonts.fonts.iter().map(font_info).collect();
            if json {
                emit_json(
                    output,
                    &serde_json::json!({"format":loaded.format,"font_count":fonts.len(),"fonts":fonts,"report":loaded.report}),
                )
            } else {
                writeln!(output, "Format: {}", loaded.format)?;
                writeln!(output, "Fonts: {}", fonts.len())?;
                for font in &fonts {
                    emit_json(output, font)?;
                }
                write!(output, "{}", loaded.report)?;
                Ok(())
            }
        }
        Command::Convert {
            inputs,
            output: destination,
            from,
            to,
            strict,
            report: style,
            pcf,
        } => {
            let id = match to {
                Some(id) => id,
                None => registry
                    .format_from_name(&destination.to_string_lossy())?
                    .id
                    .into(),
            };
            pcf.apply(&id, &mut registry)?;
            // Check the target before doing work, and never create/truncate output.
            registry.ensure_writable(&id)?;
            let options = ReadOptions {
                format: from,
                ..ReadOptions::default()
            };
            let mut fonts = fontpixel::FontSet::default();
            let mut report = fontpixel::Report::default();
            for input in &inputs {
                let bytes = read_primary(input, &options.limits)?;
                let name = input.to_string_lossy();
                let mut loaded = registry.read(&Input::new(&bytes).with_name(&name), &options)?;
                for entry in &mut loaded.report.entries {
                    if let Some(index) = &mut entry.location.font_index {
                        *index += fonts.fonts.len();
                    }
                }
                report.extend(loaded.report);
                fonts.fonts.extend(loaded.fonts.fonts);
                // Preserve container metadata too; unsupported target fields must be reported.
                for property in loaded.fonts.properties.into_vec() {
                    fonts.properties.push(property);
                }
            }
            let filename = destination
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| Error::InvalidInput("output must have a UTF-8 filename".into()))?;
            let mut write_options =
                fontpixel::WriteOptions::new(fontpixel::ResourcePath::new(filename)?);
            write_options.strict = strict;
            let written = registry.write_with_report(&fonts, &id, &write_options, report)?;
            let bytes = written.output.into_single()?;
            persist(&destination, &bytes)?;
            match style {
                ReportStyle::Json => emit_json(
                    output,
                    &serde_json::json!({"output":destination,"format":id,"bytes":bytes.len(),"report":written.report}),
                ),
                ReportStyle::Text => {
                    write!(output, "{}", written.report)?;
                    writeln!(
                        output,
                        "Wrote {} bytes to {}",
                        bytes.len(),
                        destination.display()
                    )?;
                    Ok(())
                }
            }
        }
    }
}

fn property_json(v: &fontpixel::model::PropertyValue) -> serde_json::Value {
    use fontpixel::model::PropertyValue::*;
    match v {
        Integer(v) => serde_json::json!(v),
        Unsigned(v) => serde_json::json!(v),
        Decimal(v) | Text(v) => serde_json::json!(v),
        Flag => serde_json::Value::Null,
        Bytes(v) => serde_json::json!({"bytes":v}),
        List(v) => v.iter().map(property_json).collect(),
    }
}
fn properties_json(p: &fontpixel::model::Properties) -> serde_json::Value {
    p.iter()
        .filter(|p| !p.name.starts_with("bdf."))
        .map(|p| serde_json::json!({"name":p.name,"value":property_json(&p.value)}))
        .collect()
}
fn font_info(font: &fontpixel::Font) -> serde_json::Value {
    let unencoded = font
        .glyphs()
        .filter(|(_, g)| g.codepoints.is_empty())
        .count();
    let codes: Vec<_> = font.codepoints().map(|(c, _)| c.value()).collect();
    let bbox = font
        .metrics
        .bounds
        .map(|b| [i64::from(b.width), i64::from(b.height), b.x, b.y]);
    serde_json::json!({
        "name":font.names.xlfd,"family":font.names.family,"glyph_count":font.len(),
        "encoded_count":font.len()-unencoded,"unencoded_count":unencoded,"distinct_codes":codes.len(),
        "first_code":codes.first(),"last_code":codes.last(),"pixel_size":font.metrics.pixel_size,
        "point_size":font.metrics.point_size.map(|v| serde_json::json!({"numerator":v.numerator(),"denominator":v.denominator()})),
        "resolution":font.metrics.resolution.map(|v| [v.x,v.y]),
        "ascent":font.metrics.ascent,"descent":font.metrics.descent,"bounds":bbox,
        "charset":font.charset.canonical_name(),"charset_registry":font.charset.registry,"charset_encoding":font.charset.encoding,"gl":font.charset.gl,
        "headers":properties_json(&font.headers),"properties":properties_json(&font.properties),
    })
}

fn persist(destination: &Path, bytes: &[u8]) -> fontpixel::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temporary(PathBuf);
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut chosen = None;
    for _ in 0..100 {
        let path = parent.join(format!(
            ".fontpixel-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                chosen = Some((Temporary(path), file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let (temporary, mut file) = chosen.ok_or_else(|| {
        Error::InvalidInput("cannot allocate a unique output temporary file".into())
    })?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary.0, destination)?;
    Ok(())
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
