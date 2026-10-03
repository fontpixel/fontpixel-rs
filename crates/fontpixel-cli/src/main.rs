//! Font inspection and conversion through the feature-gated real codec registry.
#![forbid(unsafe_code)]
#[cfg(feature = "opentype")]
mod opentype;
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
    /// Combine regional/script faces into a locl-enabled OpenType font.
    #[cfg(feature = "opentype")]
    Variants(opentype::VariantsArgs),
    /// Subset sfnt/WOFF/WOFF2 by text or Unicode ranges.
    #[cfg(feature = "opentype")]
    Subset(opentype::SubsetArgs),
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
        #[command(flatten)]
        raster: RasterArguments,
    },
    /// Render text (or --all) as terminal character art.
    Preview {
        #[command(flatten)]
        layout: LayoutArguments,
        /// Exactly two characters: ink then paper.
        #[arg(long, default_value = "#.")]
        chars: String,
    },
    /// Render text (or --all) to a PNG or SVG image.
    Render {
        #[command(flatten)]
        layout: LayoutArguments,
        /// PNG or SVG output; written only after successful validation.
        #[arg(short, long)]
        output: PathBuf,
        /// Explicit image type; otherwise inferred from output extension.
        #[arg(long, value_parser = ["png", "svg"])]
        to: Option<String>,
        /// Positive integer nearest-neighbor scale.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
        scale: u32,
        /// Ink color: #RGB, #RGBA, #RRGGBB, #RRGGBBAA, or transparent.
        #[arg(long, default_value = "#000000")]
        color: fontpixel::render::Rgba,
        /// Paper color, in the same syntax as --color.
        #[arg(long, default_value = "#ffffff")]
        background: fontpixel::render::Rgba,
        /// SVG pixel shape (PNG uses square pixels).
        #[arg(long, value_parser = ["square", "square-separate", "dot"])]
        shape: Option<String>,
        /// SVG dot diameter or separate-square side relative to one pixel.
        #[arg(long)]
        dotsize: Option<f64>,
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
        #[command(flatten)]
        raster: RasterArguments,
        #[command(flatten)]
        vector: VectorArguments,
    },
}

#[derive(Args)]
struct LayoutArguments {
    #[command(flatten)]
    raster: RasterArguments,
    /// Input font (BDF/PCF, including gzip when enabled).
    input: PathBuf,
    /// Text in logical input order. Newline is an ordinary glyph, as in SPEC.
    #[arg(required_unless_present = "all", conflicts_with = "all")]
    text: Option<String>,
    /// Render every effective encoded mapping (drawall).
    #[arg(long)]
    all: bool,
    /// Inclusive code ranges, e.g. U+0020-007E,U+4E00-4EFF; requires --all.
    #[arg(long, requires = "all")]
    range: Option<String>,
    /// Ordering for --all.
    #[arg(long, value_enum, default_value = "ascending", requires = "all")]
    order: DrawOrder,
    /// Input format override.
    #[arg(long, value_parser = format_parser())]
    from: Option<String>,
    /// Zero-based strike index; required for inputs containing multiple fonts.
    #[arg(long)]
    font_index: Option<usize>,
    /// lr, rl, tb, bt, or a four-letter pair such as lrbt or tbrl.
    #[arg(long, default_value = "lrtb")]
    direction: fontpixel::render::WritingDirection,
    /// Maximum accumulated width-plus-offset per line (SPEC vertical behavior).
    #[arg(
        long,
        alias = "linelimit",
        default_value_t = 512,
        allow_hyphen_values = true
    )]
    line_limit: i64,
    /// Advance (1), complete font cell (0), or tight ink; --all defaults to cell.
    #[arg(long, value_enum)]
    mode: Option<DrawMode>,
    /// Use the current glyph's spacing at boundaries instead of the previous glyph.
    #[arg(long)]
    use_current_glyph_spacing: bool,
    /// Additional signed gap between glyphs.
    #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
    letter_spacing: i64,
    /// Additional signed gap between lines/columns.
    #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
    line_spacing: i64,
    /// Replacement glyph code, e.g. U+003F, 0x3f, or 63.
    #[arg(long, conflicts_with = "fail_missing")]
    missing: Option<String>,
    /// Fail on any absent/unrepresentable character.
    #[arg(long)]
    fail_missing: bool,
    /// Explicit grayscale-to-monochrome threshold (0..255).
    #[arg(long)]
    gray_threshold: Option<u8>,
    /// Reject read/render loss before displaying or saving output.
    #[arg(long)]
    strict: bool,
    /// Maximum rendered/scaled pixel count; increase for large --all atlases.
    #[arg(long, default_value_t = 16 * 1024 * 1024)]
    max_pixels: usize,
    /// Maximum rendered width or height.
    #[arg(long, default_value_t = 1_048_576)]
    max_dimension: u32,
    /// Maximum layout bookkeeping bytes.
    #[arg(long, default_value_t = 128 * 1024 * 1024)]
    max_layout_bytes: usize,
    /// Maximum diagnostic text and deduplication bookkeeping bytes.
    #[arg(long, default_value_t = 16 * 1024 * 1024)]
    max_report_bytes: usize,
    /// Maximum number of input glyphs, including missing ones.
    #[arg(long, default_value_t = 1_000_000)]
    max_glyphs: usize,
    /// Maximum encoded image/text bytes.
    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    max_output_bytes: usize,
    /// Diagnostics format (preview diagnostics go to stderr).
    #[arg(long, value_enum, default_value = "text")]
    report: ReportStyle,
}
#[derive(Clone, Copy, ValueEnum)]
enum DrawMode {
    #[value(alias = "1")]
    Advance,
    #[value(alias = "0")]
    Cell,
    Ink,
}
#[derive(Clone, Copy, ValueEnum)]
enum DrawOrder {
    Ascending,
    Descending,
    File,
    ReverseFile,
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
        #[cfg(feature = "opentype")]
        Command::Variants(args) => args.report_style(),
        #[cfg(feature = "opentype")]
        Command::Subset(args) => args.report_style(),
        Command::Info { json: true, .. } | Command::Formats { json: true } => ReportStyle::Json,
        Command::Convert { report, .. } => *report,
        Command::Preview { layout, .. } | Command::Render { layout, .. } => layout.report,
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
        #[cfg(feature = "opentype")]
        Command::Variants(args) => opentype::variants(args, output),
        #[cfg(feature = "opentype")]
        Command::Subset(args) => opentype::subset(args, output),
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
            json,
            raster,
        } => {
            raster.apply(&mut registry)?;
            let options = ReadOptions {
                format: from,
                limits: raster.input_limits(),
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
        Command::Preview { layout, chars } => {
            let palette: Vec<_> = chars.chars().collect();
            if palette.len() != 2 {
                return Err(Error::InvalidInput(
                    "--chars must contain exactly two characters (ink then paper)".into(),
                ));
            }
            let rendered = render_layout(&layout)?;
            let text = rendered
                .bitmap
                .to_text(&[(1, palette[0]), (0, palette[1])])?;
            // The terminal stream includes its final LF as well as UTF-8 pixels.
            if !text.is_empty() && text.len().saturating_add(1) > layout.max_output_bytes {
                return Err(Error::ResourceLimit {
                    resource: "preview output bytes",
                    limit: layout.max_output_bytes,
                });
            }
            // Never interleave diagnostics with the bitmap's stdout stream.
            if !rendered.report.entries.is_empty() {
                emit_render_report(&rendered.report, layout.report, &mut io::stderr().lock())?;
            }
            if !text.is_empty() {
                writeln!(output, "{text}")?;
            }
            Ok(())
        }
        Command::Render {
            layout,
            output: destination,
            to,
            scale,
            color,
            background,
            shape,
            dotsize,
        } => {
            let kind = to
                .or_else(|| {
                    destination
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase)
                })
                .ok_or_else(|| {
                    Error::InvalidInput("output needs .png/.svg or --to png/svg".into())
                })?;
            if kind != "png" && kind != "svg" {
                return Err(Error::InvalidInput(
                    "render output must be PNG or SVG".into(),
                ));
            }
            #[cfg(not(feature = "png"))]
            if kind == "png" {
                return Err(Error::FeatureDisabled {
                    format: "png".into(),
                });
            }
            if kind != "svg" && (shape.is_some() || dotsize.is_some()) {
                return Err(Error::InvalidInput(
                    "--shape/--dotsize require SVG image output".into(),
                ));
            }
            let rendered = render_layout(&layout)?;
            let image_options = fontpixel::render::ImageOptions {
                scale,
                foreground: color,
                background,
                limits: render_bitmap_limits(&layout),
            };
            let bytes = if kind == "svg" {
                #[cfg(feature = "svg")]
                {
                    let shape = svg_shape(shape.as_deref());
                    fontpixel::formats::svg::image(
                        &rendered.bitmap,
                        &fontpixel::formats::svg::ImageOptions {
                            image: image_options,
                            shape,
                            dot_size: dotsize.unwrap_or(
                                if shape == fontpixel::outline::Shape::Dot {
                                    0.9
                                } else {
                                    1.
                                },
                            ),
                            ..Default::default()
                        },
                    )?
                    .into_bytes()
                }
                #[cfg(not(feature = "svg"))]
                {
                    if shape.is_some() || dotsize.is_some() {
                        return Err(Error::FeatureDisabled {
                            format: "svg".into(),
                        });
                    }
                    fontpixel::render::to_svg(&rendered.bitmap, &image_options)?.into_bytes()
                }
            } else {
                #[cfg(feature = "png")]
                {
                    fontpixel::render::to_png(&rendered.bitmap, &image_options)?
                }
                #[cfg(not(feature = "png"))]
                {
                    return Err(Error::FeatureDisabled {
                        format: "png".into(),
                    });
                }
            };
            persist(&destination, &bytes)?;
            match layout.report {
                ReportStyle::Json => emit_json(
                    output,
                    &serde_json::json!({"output":destination,"format":kind,"bytes":bytes.len(),"width":rendered.bitmap.width(),"height":rendered.bitmap.height(),"scale":scale,"report":rendered.report}),
                ),
                ReportStyle::Text => {
                    write!(output, "{}", rendered.report)?;
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
        Command::Convert {
            inputs,
            output: destination,
            from,
            to,
            strict,
            report: style,
            pcf,
            raster,
            vector,
        } => {
            let id = match to {
                Some(id) => id,
                None => match destination
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                {
                    Some("ttf") => "ttf".into(),
                    Some("otf") => "otf".into(),
                    Some("ttc" | "otc") => "ttc".into(),
                    Some("woff") => "woff".into(),
                    Some("woff2") => "woff2".into(),
                    _ => registry
                        .format_from_name(&destination.to_string_lossy())?
                        .id
                        .into(),
                },
            };
            pcf.apply(&id, &mut registry)?;
            raster.apply(&mut registry)?;
            vector.apply(&id, &mut registry)?;
            // Check the target before doing work, and never create/truncate output.
            registry.ensure_writable(&id)?;
            let options = ReadOptions {
                format: from,
                limits: raster.input_limits(),
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
                if inputs.len() == 1 {
                    fonts.source_format = loaded.fonts.source_format.take();
                }
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

fn parse_code(value: &str) -> fontpixel::Result<u32> {
    let parsed = if let Some(v) = value
        .strip_prefix("U+")
        .or_else(|| value.strip_prefix("u+"))
        .or_else(|| value.strip_prefix("0x"))
    {
        u32::from_str_radix(v, 16)
    } else {
        value.parse()
    };
    parsed.map_err(|_| Error::InvalidInput(format!("invalid codepoint {value:?}")))
}
fn render_bitmap_limits(args: &LayoutArguments) -> fontpixel::bitmap::BitmapLimits {
    fontpixel::bitmap::BitmapLimits {
        max_dimension: args.max_dimension,
        max_pixels: args.max_pixels,
        max_bytes: args.max_output_bytes,
    }
}
fn render_layout(args: &LayoutArguments) -> fontpixel::Result<fontpixel::render::Rendered> {
    use fontpixel::model::Codepoint;
    use fontpixel::render::{GlyphOrder, MissingGlyph, RenderMode, RenderOptions};
    let read_options = ReadOptions {
        format: args.from.clone(),
        limits: args.raster.input_limits(),
    };
    let bytes = read_primary(&args.input, &read_options.limits)?;
    let mut registry = fontpixel::registry();
    args.raster.apply(&mut registry)?;
    let loaded = registry.read(
        &Input::new(&bytes).with_name(&args.input.to_string_lossy()),
        &read_options,
    )?;
    let index = match args.font_index {
        Some(index) => index,
        None if loaded.fonts.fonts.len() == 1 => 0,
        None => {
            return Err(Error::InvalidInput(
                "input must contain one font; choose --font-index".into(),
            ));
        }
    };
    let font = loaded
        .fonts
        .fonts
        .get(index)
        .ok_or_else(|| Error::InvalidInput("font index out of range".into()))?;
    if args.strict && loaded.report.has_loss() {
        return Err(Error::StrictLoss(loaded.report));
    }
    let missing = if let Some(cp) = &args.missing {
        let cp = parse_code(cp)?;
        let glyph = font
            .glyph_for_codepoint(Codepoint::Unicode(cp))
            .or_else(|| font.glyph_for_codepoint(Codepoint::Legacy(cp)))
            .ok_or_else(|| {
                Error::InvalidInput("replacement glyph is not present in the font".into())
            })?;
        MissingGlyph::Glyph(glyph)
    } else if args.fail_missing {
        MissingGlyph::Error
    } else {
        MissingGlyph::Blank
    };
    let mode = match args.mode {
        Some(DrawMode::Advance) => RenderMode::Advance,
        Some(DrawMode::Ink) => RenderMode::Ink,
        Some(DrawMode::Cell) => RenderMode::Cell,
        None if args.all => RenderMode::Cell,
        None => RenderMode::Advance,
    };
    let mut options = RenderOptions {
        line_limit: args.line_limit,
        mode,
        direction: args.direction,
        use_current_glyph_spacing: args.use_current_glyph_spacing,
        letter_spacing: args.letter_spacing,
        line_spacing: args.line_spacing,
        missing,
        gray_threshold: args.gray_threshold,
        strict: false,
        ..Default::default()
    };
    options.limits.max_glyphs = args.max_glyphs;
    options.limits.max_layout_bytes = args.max_layout_bytes;
    options.limits.max_report_bytes = args.max_report_bytes;
    options.limits.bitmap = render_bitmap_limits(args);
    let mut rendered = if args.all {
        let ranges = args
            .range
            .as_deref()
            .map(|s| {
                s.split(',')
                    .map(|r| {
                        let (a, b) = r.split_once('-').unwrap_or((r, r));
                        let start = parse_code(a)?;
                        // Bare end of a U+/0x range uses the same hexadecimal notation.
                        let end =
                            if (a.starts_with("U+") || a.starts_with("u+") || a.starts_with("0x"))
                                && !b.starts_with("U+")
                                && !b.starts_with("u+")
                                && !b.starts_with("0x")
                            {
                                u32::from_str_radix(b, 16)
                                    .map_err(|_| Error::InvalidInput("invalid range end".into()))?
                            } else {
                                parse_code(b)?
                            };
                        if start > end {
                            return Err(Error::InvalidInput("range start exceeds end".into()));
                        }
                        Ok((start, end))
                    })
                    .collect::<fontpixel::Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let order = match args.order {
            DrawOrder::Ascending => GlyphOrder::Ascending,
            DrawOrder::Descending => GlyphOrder::Descending,
            DrawOrder::File => GlyphOrder::File,
            DrawOrder::ReverseFile => GlyphOrder::ReverseFile,
        };
        font.draw_all(order, &ranges, &options)?
    } else {
        font.draw(args.text.as_deref().unwrap_or(""), &options)?
    };
    let mut report = loaded.report;
    for entry in &mut rendered.report.entries {
        entry.location.font_index = Some(index);
    }
    report.extend(rendered.report);
    if args.strict && report.has_loss() {
        return Err(Error::StrictLoss(report));
    }
    rendered.report = report;
    Ok(rendered)
}
fn emit_render_report(
    report: &fontpixel::Report,
    style: ReportStyle,
    output: &mut dyn Write,
) -> fontpixel::Result<()> {
    match style {
        ReportStyle::Json => emit_json(output, &serde_json::json!({"report":report})),
        ReportStyle::Text => {
            write!(output, "{report}")?;
            Ok(())
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

#[derive(Args, Default)]
struct RasterArguments {
    /// Primary input byte budget (default 64 MiB); aggregate 256 MiB cap remains.
    #[arg(long)]
    max_input_bytes: Option<usize>,
    /// Explicit raster size for reviewed off-grid/curved outlines.
    #[arg(long)]
    ppem: Option<u16>,
    /// Select a TTC/OTC face (zero based); otherwise read every face.
    #[arg(long)]
    face_index: Option<usize>,
    /// Rasterize outlines even when matching embedded images exist.
    #[arg(long)]
    no_embedded_bitmaps: bool,
    /// Explicit design units per pixel; requires --ppem.
    #[arg(long, requires = "ppem")]
    grid_units: Option<u16>,
    /// Reviewed horizontal and vertical translations in pixel units.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pixel_offset_x: f64,
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pixel_offset_y: f64,
}
impl RasterArguments {
    fn input_limits(&self) -> ResourceLimits {
        let mut limits = ResourceLimits::default();
        if let Some(maximum) = self.max_input_bytes {
            limits.max_input_bytes = maximum;
        }
        limits
    }
    fn apply(&self, registry: &mut fontpixel::FontRegistry) -> fontpixel::Result<()> {
        #[cfg(feature = "svg")]
        registry.set_svg_read_options(fontpixel::formats::svg::ReadOptions {
            ppem: self.ppem,
            font_index: self.face_index,
            limits: fontpixel::formats::svg::Limits {
                resources: self.input_limits(),
                ..Default::default()
            },
        });
        #[cfg(feature = "opentype")]
        registry.set_vector_read_options(fontpixel::formats::vector_raster::Options {
            ppem: self.ppem,
            face_index: self.face_index,
            prefer_embedded: !self.no_embedded_bitmaps,
            grid_units: self.grid_units,
            pixel_offset: (self.pixel_offset_x, self.pixel_offset_y),
            ..Default::default()
        });
        #[cfg(not(feature = "opentype"))]
        {
            let _ = registry;
            if (!cfg!(feature = "svg") && (self.ppem.is_some() || self.face_index.is_some()))
                || self.no_embedded_bitmaps
                || self.grid_units.is_some()
                || self.pixel_offset_x != 0.
                || self.pixel_offset_y != 0.
            {
                return Err(Error::InvalidInput("OpenType feature is disabled".into()));
            }
        }
        Ok(())
    }
}
#[derive(Args, Default)]
struct VectorArguments {
    /// Outline shape for SVG/TTF/OTF/WOFF/WOFF2/TTC conversion.
    #[arg(long,value_parser=["square","square-separate","dot"])]
    shape: Option<String>,
    #[arg(long)]
    dotsize: Option<f64>,
    /// Native dot reuse; default patterns. Literal favors smaller webfonts.
    #[arg(long,value_parser=["literal","rows","patterns"])]
    dot_reuse: Option<String>,
    /// Design units per source pixel (default 100).
    #[arg(long)]
    ppu: Option<u16>,
    #[arg(long)]
    upm: Option<u16>,
    #[arg(long)]
    embed_bitmaps: bool,
    #[arg(long)]
    gray_threshold: Option<u8>,
    /// Outline source in webfonts/TTC (auto selects CFF squares, TrueType dots).
    #[arg(long,value_parser=["auto","cff","ttf"])]
    woff2_source: Option<String>,
    #[arg(long,value_parser=clap::value_parser!(u32).range(0..=11))]
    woff2_quality: Option<u32>,
    #[arg(long)]
    collection_v2: bool,
}
impl VectorArguments {
    fn apply(&self, id: &str, registry: &mut fontpixel::FontRegistry) -> fontpixel::Result<()> {
        let explicit = self.shape.is_some()
            || self.dotsize.is_some()
            || self.dot_reuse.is_some()
            || self.ppu.is_some()
            || self.upm.is_some()
            || self.embed_bitmaps
            || self.gray_threshold.is_some()
            || self.woff2_source.is_some()
            || self.woff2_quality.is_some()
            || self.collection_v2;
        #[cfg(feature = "svg")]
        if id == "svg" {
            if self.upm.is_some()
                || self.dot_reuse.is_some()
                || self.embed_bitmaps
                || self.woff2_source.is_some()
                || self.woff2_quality.is_some()
                || self.collection_v2
            {
                return Err(Error::InvalidInput(
                    "OpenType container options cannot apply to SVG output".into(),
                ));
            }
            let shape = svg_shape(self.shape.as_deref());
            registry.set_svg_write_options(fontpixel::formats::svg::WriteOptions {
                units_per_pixel: self.ppu.unwrap_or(100),
                shape,
                dot_size: self
                    .dotsize
                    .unwrap_or(if shape == fontpixel::outline::Shape::Dot {
                        0.9
                    } else {
                        1.
                    }),
                gray_threshold: self.gray_threshold,
                ..Default::default()
            });
            return Ok(());
        }
        if explicit && !["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"].contains(&id) {
            return Err(Error::InvalidInput(
                "outline options require an OpenType output".into(),
            ));
        }
        #[cfg(feature = "opentype")]
        {
            use fontpixel::{outline::Shape, variants::OutlineFlavor};
            let mut o = fontpixel::formats::vector_raster::WriteOptions::default();
            o.outline.shape = match self.shape.as_deref() {
                Some("dot") => Shape::Dot,
                Some("square-separate") => Shape::SquareSeparate,
                _ => Shape::Square,
            };
            o.outline.dot_size = self.dotsize.unwrap_or(if o.outline.shape == Shape::Dot {
                0.9
            } else {
                1.
            });
            o.outline.units_per_pixel = self.ppu.unwrap_or(100);
            o.outline.dot_reuse = opentype::dot_reuse(self.dot_reuse.as_deref());
            o.outline.units_per_em = self.upm;
            o.outline.embed_bitmaps = self.embed_bitmaps;
            o.outline.gray_threshold = self.gray_threshold;
            o.outline.flavor = match self.woff2_source.as_deref() {
                Some("ttf") => OutlineFlavor::TrueType,
                Some("cff") => OutlineFlavor::Cff,
                _ => {
                    if o.outline.shape == Shape::Dot {
                        OutlineFlavor::TrueType
                    } else {
                        OutlineFlavor::Cff
                    }
                }
            };
            o.woff2.quality = self.woff2_quality.unwrap_or(11);
            o.collection_v2 = self.collection_v2;
            registry.set_vector_write_options(o);
        }
        #[cfg(not(feature = "opentype"))]
        {
            let _ = registry;
            if explicit {
                return Err(Error::InvalidInput("OpenType feature is disabled".into()));
            }
        }
        Ok(())
    }
}

#[cfg(feature = "svg")]
fn svg_shape(shape: Option<&str>) -> fontpixel::outline::Shape {
    match shape {
        Some("dot") => fontpixel::outline::Shape::Dot,
        Some("square-separate") => fontpixel::outline::Shape::SquareSeparate,
        _ => fontpixel::outline::Shape::Square,
    }
}
