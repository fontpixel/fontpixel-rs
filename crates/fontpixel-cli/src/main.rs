//! Font inspection and conversion through the feature-gated real codec registry.
//! Phase-6 integration reconstructed after workspace loss; pending renewed validation.
#![forbid(unsafe_code)]
mod assets;
mod batch;
mod commands;
mod merge;
mod metadata;
mod image_formats;
mod input_budget;
use image_formats::WriteArguments as ImageArguments;
mod extra_formats;
#[cfg(feature = "opentype")]
mod opentype;
mod p2_formats;
mod p3_formats;
use clap::{Args, Parser, Subcommand, ValueEnum, builder::PossibleValuesParser};
use fontpixel::{Error, ReadOptions, ResourceLimits};
#[cfg(feature = "opentype")]
use std::{fs::File, io::Read};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    name = "fontpixel",
    version,
    about = "Bitmap font inspection and conversion",
    after_help = "Examples:\n  fontpixel info font.bdf --json\n  fontpixel convert font.bdf -o font.pcf\n  fontpixel batch exports.toml > report.json"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute bounded parallel TOML tasks with transactional outputs.
    Batch(batch::BatchArgs),
    /// Merge selected glyphs from ordered font parts.
    Merge(merge::MergeArgs),
    /// Compare decoded font data.
    Diff(commands::DiffArgs),
    /// Validate native fonts and supported sfnt structure.
    Validate(commands::ValidateArgs),
    /// Inspect or transform explicit character mappings.
    Charset(commands::CharsetArgs),
    /// Combine regional/script faces into a locl-enabled OpenType font.
    #[cfg(feature = "opentype")]
    Variants(opentype::VariantsArgs),
    /// Subset sfnt/WOFF/WOFF2 by text or Unicode ranges.
    #[cfg(feature = "opentype")]
    Subset(opentype::SubsetArgs),
    /// List all planned formats and their actual implementation status.
    #[command(after_help="Example: fontpixel formats --json")]
    Formats {
        /// Emit machine-readable metadata and capability flags.
        #[arg(long)]
        json: bool,
    },
    /// Inspect a font, including its metrics, properties and diagnostics.
    #[command(after_help="Example: fontpixel info font.bdf --json")]
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
    #[command(after_help="Example: fontpixel preview font.bdf Hello --chars '#.'")]
    Preview {
        #[command(flatten)]
        layout: LayoutArguments,
        /// Exactly two characters: ink then paper.
        #[arg(long, default_value = "#.")]
        chars: String,
    },
    /// Render text (or --all) to a PNG or SVG image.
    #[command(after_help="Example: fontpixel render font.bdf Hello -o hello.svg")]
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
    #[command(visible_alias = "vectorize", after_help="Examples:\n  fontpixel convert input.bdf -o output.pcf\n  fontpixel convert one.bdf two.bdf --each --to pcf -o existing-directory\n  fontpixel vectorize input.bdf -o output.ttf --shape square") ]
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
        /// Convert each input separately into an existing output directory; requires --to.
        #[arg(long, requires = "to")]
        each: bool,
        /// Select a zero-based combined strike, or one strike from each input with --each.
        #[arg(long)]
        font_index: Option<usize>,
        #[command(flatten)]
        names: metadata::NameArguments,
        /// Diagnostic format.
        #[arg(long, value_enum, default_value = "text")]
        report: ReportStyle,
        #[command(flatten)]
        pcf: PcfArguments,
        #[command(flatten)]
        console: ConsoleArguments,
        #[command(flatten)]
        image: ImageArguments,
        #[command(flatten)]
        c: COutputArguments,
        #[command(flatten)]
        extra: extra_formats::WriteArguments,
        #[command(flatten)]
        p2: Box<p2_formats::WriteArguments>,
        #[command(flatten)]
        p3: Box<p3_formats::WriteArguments>,
        #[command(flatten)]
        raster: Box<RasterArguments>,
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

#[derive(Args, Default)]
struct ConsoleArguments {
    /// PSF output version, 1 or 2 (default 2).
    #[arg(long, value_parser=["1","2"])]
    psf_version: Option<String>,
    /// PSF1 cells, 256 or 512; default chooses the smallest fit.
    #[arg(long, value_parser=clap::value_parser!(u16))]
    psf1_count: Option<u16>,
    /// PSF Unicode table policy (default auto).
    #[arg(long, value_parser=["auto","include","omit"])]
    psf_unicode_table: Option<String>,
    /// Windows raster output version; defaults to source version or 2.
    #[arg(long, value_parser=["1","2","3"])]
    fnt_version: Option<String>,
    /// Windows dfCharSet identifier, 0..255 (ANSI=0, OEM=255).
    #[arg(long)]
    fnt_charset: Option<u8>,
    /// Explicit target page for OEM, symbol, or historical Windows mappings.
    #[arg(long)]
    fnt_code_page: Option<String>,
}
impl ConsoleArguments {
    fn apply(
        &self,
        id: &str,
        raster: &RasterArguments,
        threshold: Option<u8>,
        registry: &mut fontpixel::FontRegistry,
    ) -> fontpixel::Result<()> {
        let fnt_requested = self.fnt_version.is_some()
            || self.fnt_charset.is_some()
            || self.fnt_code_page.is_some();
        if fnt_requested && !["fnt", "fon"].contains(&id) {
            return Err(Error::InvalidInput(
                "FNT options require --to fnt or fon".into(),
            ));
        }
        #[cfg(feature = "fnt")]
        if ["fnt", "fon"].contains(&id) {
            use fontpixel::formats::fnt::{FntWriteOptions, Version};
            registry.set_fnt_write_options(FntWriteOptions {
                version: self.fnt_version.as_deref().map(|v| match v {
                    "1" => Version::One,
                    "3" => Version::Three,
                    _ => Version::Two,
                }),
                charset: self.fnt_charset,
                code_page: self.fnt_code_page.clone(),
                gray_threshold: threshold,
                ..Default::default()
            });
        }
        #[cfg(not(feature = "fnt"))]
        if fnt_requested {
            return Err(Error::FeatureDisabled {
                format: "fnt".into(),
            });
        }
        let requested = self.psf_version.is_some()
            || self.psf1_count.is_some()
            || self.psf_unicode_table.is_some();
        if requested && id != "psf" {
            return Err(Error::InvalidInput("PSF options require --to psf".into()));
        }
        #[cfg(feature = "hex")]
        if id == "hex" {
            registry.set_hex_write_options(fontpixel::formats::hex::HexWriteOptions {
                geometry: fontpixel::formats::hex::HexReadOptions {
                    height: raster.hex_height.unwrap_or(16),
                    descent: raster.hex_descent.unwrap_or(0),
                },
                gray_threshold: threshold,
                ..Default::default()
            });
        }
        #[cfg(feature = "psf")]
        if id == "psf" {
            use fontpixel::formats::psf::{PsfWriteOptions, UnicodeTable, Version};
            registry.set_psf_write_options(PsfWriteOptions {
                version: if self.psf_version.as_deref() == Some("1") {
                    Version::One
                } else {
                    Version::Two
                },
                psf1_count: self.psf1_count,
                unicode_table: match self.psf_unicode_table.as_deref() {
                    Some("include") => UnicodeTable::Include,
                    Some("omit") => UnicodeTable::Omit,
                    _ => UnicodeTable::Auto,
                },
                gray_threshold: threshold,
                ..Default::default()
            });
        }
        let _ = (raster, threshold, registry);
        #[cfg(not(feature = "psf"))]
        if requested {
            return Err(Error::FeatureDisabled {
                format: "psf".into(),
            });
        }
        Ok(())
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
        Command::Batch(_) => ReportStyle::Json,
        Command::Merge(args) => args.report_style(),
        Command::Diff(args) => args.report_style(),
        Command::Validate(args) => args.report_style(),
        Command::Charset(args) => args.report_style(),
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

fn emit_json<T: serde::Serialize>(output: &mut dyn Write, value: &T) -> fontpixel::Result<()> {
    let result = if batch::is_active() {
        serde_json::to_writer(&mut *output, value)
    } else {
        serde_json::to_writer_pretty(&mut *output, value)
    };
    result.map_err(|error| Error::InvalidInput(format!("cannot serialize JSON: {error}")))?;
    writeln!(output)?;
    Ok(())
}

#[cfg(feature = "opentype")]
fn read_primary(path: &Path, limits: &ResourceLimits) -> fontpixel::Result<Vec<u8>> {
    let file = File::open(path)?;
    batch::sources().remember(&file)?;
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
        Command::Batch(_) => Err(Error::InvalidInput("nested batch is not supported".into())),
        Command::Merge(args) => merge::run(args, output),
        Command::Diff(args) => commands::diff(args, output),
        Command::Validate(args) => commands::validate(args, output),
        Command::Charset(args) => commands::charset(args, output),
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
            let file =
                assets::FileInput::open(&input, &options.limits, batch::sources())?;
            let loaded = registry.read(&file.input(), &options)?;
            drop(file);
            let mut retained=loaded.fonts.retained_usage(options.limits.max_total_bytes)?.bytes;
            conversion_charge(&mut retained,conversion_report_bytes(&loaded.report,options.limits.max_total_bytes)?,options.limits.max_total_bytes)?;
            let name_scratch=loaded.fonts.fonts.iter().map(|f|f.charset.registry.as_ref().map_or(0,String::len).saturating_add(f.charset.encoding.as_ref().map_or(0,String::len)).saturating_add(f.charset.name.as_ref().map_or(0,String::len))).max().unwrap_or(0);
            conversion_charge(&mut retained,name_scratch.saturating_mul(4),options.limits.max_total_bytes)?;
            conversion_charge(&mut retained,fontpixel::charset::merge_categories::UNICODE_BLOCKS.len().saturating_mul(std::mem::size_of::<usize>()),options.limits.max_total_bytes)?;
            if json {
                emit_json(output,&InfoSummary{font_count:loaded.fonts.fonts.len(),fonts:FontInfos(&loaded.fonts.fonts),format:loaded.format,report:&loaded.report})
            } else {
                writeln!(output,"Format: {}",loaded.format)?;writeln!(output,"Fonts: {}",loaded.fonts.fonts.len())?;
                for font in &loaded.fonts.fonts {emit_json(output,&FontInfo(font))?;}
                write!(output,"{}",loaded.report)?;Ok(())
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
            let byte_count = bytes.len();
            persist(&destination, bytes)?;
            match layout.report {
                ReportStyle::Json => {
                    #[derive(serde::Serialize)]
                    struct RenderSummary<'a>{bytes:usize,format:&'a str,height:u32,output:&'a Path,report:&'a fontpixel::Report,scale:u32,width:u32}
                    emit_json(output,&RenderSummary{bytes:byte_count,format:&kind,height:rendered.bitmap.height(),output:&destination,report:&rendered.report,scale,width:rendered.bitmap.width()})
                },
                ReportStyle::Text => {
                    write!(output, "{}", rendered.report)?;
                    writeln!(
                        output,
                        "Wrote {} bytes to {}",
                        byte_count,
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
            font_index,
            each,
            names,
            report: style,
            pcf,
            console,
            image,
            c,
            extra,
            p2,
            p3,
            raster,
            vector,
        } => {
            // Reconstructed phase-6 grouped conversion; pending compiler validation.
            if inputs.len()>1024 {return Err(Error::InvalidInput("convert accepts at most 1024 inputs".into()));}
            let names=names.into_overrides();
            let limits=raster.input_limits();
            let id=match to {
                Some(id)=>id,
                None if each=>return Err(Error::InvalidInput("--each requires an explicit --to format".into())),
                None=>infer_output_format(&registry,&destination)?,
            };
            registry.ensure_writable(&id)?;
            let groups=conversion_groups(&inputs,&destination,&id,each,limits.max_total_bytes)?;
            let slots=groups.len().checked_mul(std::mem::size_of::<assets::PendingOutput>()+std::mem::size_of::<ConversionSummary>()).ok_or_else(||conversion_limit(limits.max_total_bytes))?;
            let mut reservation=groups.capacity().checked_mul(std::mem::size_of::<ConversionGroup>()).ok_or_else(||conversion_limit(limits.max_total_bytes))?;
            for group in &groups{conversion_charge(&mut reservation,group.output.capacity(),limits.max_total_bytes)?;}
            conversion_charge(&mut reservation,slots,limits.max_total_bytes)?;
            let mut pending=Vec::new();pending.try_reserve_exact(groups.len()).map_err(|_|conversion_limit(limits.max_total_bytes))?;
            let mut summaries=Vec::new();summaries.try_reserve_exact(groups.len()).map_err(|_|conversion_limit(limits.max_total_bytes))?;
            let mut retained=groups.capacity().checked_mul(std::mem::size_of::<ConversionGroup>()).ok_or_else(||conversion_limit(limits.max_total_bytes))?;
            conversion_charge(&mut retained,pending.capacity().checked_mul(std::mem::size_of::<assets::PendingOutput>()).ok_or_else(||conversion_limit(limits.max_total_bytes))?,limits.max_total_bytes)?;
            conversion_charge(&mut retained,summaries.capacity().checked_mul(std::mem::size_of::<ConversionSummary>()).ok_or_else(||conversion_limit(limits.max_total_bytes))?,limits.max_total_bytes)?;
            for group in &groups {conversion_charge(&mut retained,group.output.capacity(),limits.max_total_bytes)?;}
            let sources=batch::sources();let mut allow_single_in_place=false;
            for group in &groups {
                let group_limits=conversion_remaining(&limits,retained)?;
                let mut registry=fontpixel::registry();
                pcf.apply(&id,&mut registry)?;raster.apply(&mut registry)?;vector.apply(&id,&mut registry)?;
                console.apply(&id,&raster,vector.gray_threshold,&mut registry)?;
                image.apply(&id,&raster,vector.gray_threshold,&mut registry)?;
                c.apply(&id,&raster,vector.gray_threshold,&mut registry)?;
                extra.apply(&id,&raster,vector.gray_threshold,&mut registry)?;
                p2.apply(&id,&group.output,&raster.p2,&group_limits,vector.gray_threshold,&mut registry)?;
                p3.apply(&id,&raster.p3,&group_limits,vector.gray_threshold,&mut registry)?;
                registry.ensure_writable(&id)?;
                let mut fonts=fontpixel::FontSet::default();let mut report=fontpixel::Report::default();
                let mut input_budget=input_budget::InputBudget::new(group_limits.clone());
                for input in &inputs[group.start..group.end] {
                    let options=ReadOptions{format:from.clone(),limits:input_budget.remaining_limits()?};
                    let file=assets::FileInput::open(input,&options.limits,sources.clone())?;
                    let mut loaded=registry.read(&file.input(),&options)?;drop(file);input_budget.admit(&loaded)?;
                    for entry in &mut loaded.report.entries {
                        if let Some(index)=&mut entry.location.font_index {*index=index.checked_add(fonts.fonts.len()).ok_or_else(||conversion_limit(limits.max_total_bytes))?;}
                    }
                    report.extend(loaded.report);
                    if group.end-group.start==1 {fonts.source_format=loaded.fonts.source_format.take();}
                    fonts.fonts.extend(loaded.fonts.fonts);
                    for property in loaded.fonts.properties.into_vec(){fonts.properties.push(property);}
                }
                if let Some(index)=font_index {
                    if index>=fonts.fonts.len(){return Err(Error::InvalidInput(format!("font index {index} is out of range for {}",group.output.display())));}
                    let selected=fonts.fonts.into_iter().nth(index).ok_or_else(||Error::InvalidInput("selected font is unavailable".into()))?;
                    fonts.fonts=vec![selected];
                }
                metadata::apply(&mut fonts,&names,&mut report,&group_limits)?;
                let filename=group.output.file_name().and_then(|s|s.to_str()).ok_or_else(||Error::InvalidInput("output must have a UTF-8 filename".into()))?;
                let mut write_options=fontpixel::WriteOptions::new(fontpixel::ResourcePath::new(filename)?);write_options.strict=strict;
                // Reserve this live model and incoming diagnostics in addition
                // to earlier --each outputs/reports already removed from group_limits.
                // Native source estimates receive no guessed credit; see the
                // shared helper's explicit conservative-double-counting contract.
                let (writer_limits,report)=commands::caller_writer_limits(&fonts,&group_limits,report,strict)?;
                write_options.transport_limits=Some(fontpixel::formats::TransportLimits{max_output_bytes:writer_limits.max_total_bytes,resources:writer_limits});
                let written=registry.write_with_report(&fonts,&id,&write_options,report)?;
                drop(fonts);
                let bytes=written.output.assets().iter().try_fold(0usize,|n,a|n.checked_add(a.bytes.len()).ok_or_else(||conversion_limit(limits.max_total_bytes)))?;
                allow_single_in_place=!batch::is_active()&&!each&&inputs.len()==1&&inputs[0]==group.output&&!written.output.is_directory()&&written.output.assets().len()==1&&sources.is_single_primary();
                conversion_charge(&mut retained,written.output.retained_bytes(limits.max_total_bytes)?.saturating_sub(std::mem::size_of::<fontpixel::OutputBundle>()),limits.max_total_bytes)?;
                conversion_charge(&mut retained,conversion_report_bytes(&written.report,limits.max_total_bytes)?.saturating_sub(std::mem::size_of::<fontpixel::Report>()),limits.max_total_bytes)?;
                let path_bytes=written.output.assets().iter().try_fold(0usize,|n,a|n.checked_add(a.path.as_str().len()).ok_or_else(||conversion_limit(limits.max_total_bytes)))?;
                let summary_slots=written.output.assets().len().checked_mul(std::mem::size_of::<AssetSummary>()).ok_or_else(||conversion_limit(limits.max_total_bytes))?;
                conversion_charge(&mut retained,path_bytes.checked_add(summary_slots).ok_or_else(||conversion_limit(limits.max_total_bytes))?,limits.max_total_bytes)?;
                conversion_charge(&mut retained,group.output.as_os_str().len().checked_mul(2).and_then(|n|n.checked_add(id.len())).ok_or_else(||conversion_limit(limits.max_total_bytes))?,limits.max_total_bytes)?;
                let mut assets=Vec::new();assets.try_reserve_exact(written.output.assets().len()).map_err(|_|conversion_limit(limits.max_total_bytes))?;
                assets.extend(written.output.assets().iter().map(|a|AssetSummary{bytes:a.bytes.len(),path:a.path.as_str().to_owned()}));
                let summary=ConversionSummary{assets,bytes,format:id.clone(),output:group.output.clone(),report:written.report};
                let destination=group.output.clone();
                let estimated=path_bytes.checked_add(summary_slots).and_then(|n|n.checked_add(group.output.as_os_str().len()*2+id.len())).ok_or_else(||conversion_limit(limits.max_total_bytes))?;
                let mut actual=summary.assets.capacity().checked_mul(std::mem::size_of::<AssetSummary>()).ok_or_else(||conversion_limit(limits.max_total_bytes))?;
                for asset in &summary.assets{conversion_charge(&mut actual,asset.path.capacity(),limits.max_total_bytes)?;}
                conversion_charge(&mut actual,summary.format.capacity(),limits.max_total_bytes)?;conversion_charge(&mut actual,summary.output.capacity(),limits.max_total_bytes)?;conversion_charge(&mut actual,destination.capacity(),limits.max_total_bytes)?;
                if actual>estimated{conversion_charge(&mut retained,actual-estimated,limits.max_total_bytes)?;}
                pending.push(assets::PendingOutput{destination,bundle:written.output});summaries.push(summary);
            }
            let protection=if allow_single_in_place{assets::Sources::default()}else{sources};
            batch::persist_outputs(pending,&protection)?;
            match style {
                ReportStyle::Json if each=>emit_json(output,&ConversionList{outputs:&summaries}),
                ReportStyle::Json=>emit_json(output,&summaries[0]),
                ReportStyle::Text=>{for summary in &summaries {write!(output,"{}",summary.report)?;writeln!(output,"Wrote {} bytes to {}",summary.bytes,summary.output.display())?;}Ok(())}
            }
        }
    }
}

#[derive(serde::Serialize)]
struct AssetSummary { bytes:usize,path:String }
#[derive(serde::Serialize)]
struct ConversionSummary {assets:Vec<AssetSummary>,bytes:usize,format:String,output:PathBuf,report:fontpixel::Report}
#[derive(serde::Serialize)]
struct ConversionList<'a>{outputs:&'a[ConversionSummary]}
struct ConversionGroup{start:usize,end:usize,output:PathBuf}
fn conversion_limit(limit:usize)->Error{Error::ResourceLimit{resource:"retained conversion output/report bytes",limit}}
fn conversion_charge(used:&mut usize,bytes:usize,limit:usize)->fontpixel::Result<()>{*used=used.checked_add(bytes).filter(|n|*n<=limit).ok_or_else(||conversion_limit(limit))?;Ok(())}
fn conversion_remaining(limits:&ResourceLimits,used:usize)->fontpixel::Result<ResourceLimits>{
    let mut remaining=limits.clone();remaining.max_total_bytes=remaining.max_total_bytes.checked_sub(used).ok_or_else(||conversion_limit(limits.max_total_bytes))?;
    remaining.max_input_bytes=remaining.max_input_bytes.min(remaining.max_total_bytes);remaining.max_resource_bytes=remaining.max_resource_bytes.min(remaining.max_total_bytes);remaining.max_decompressed_bytes=remaining.max_decompressed_bytes.min(remaining.max_total_bytes);remaining.max_table_bytes=remaining.max_table_bytes.min(remaining.max_total_bytes);Ok(remaining)
}
fn conversion_report_bytes(report:&fontpixel::Report,limit:usize)->fontpixel::Result<usize>{
    let mut bytes=std::mem::size_of::<fontpixel::Report>();conversion_charge(&mut bytes,report.entries.capacity().checked_mul(std::mem::size_of::<fontpixel::ReportEntry>()).ok_or_else(||conversion_limit(limit))?,limit)?;
    for entry in &report.entries {conversion_charge(&mut bytes,entry.code.capacity(),limit)?;conversion_charge(&mut bytes,entry.message.capacity(),limit)?;conversion_charge(&mut bytes,entry.location.asset.as_ref().map_or(0,String::capacity),limit)?;}Ok(bytes)
}
fn infer_output_format(registry:&fontpixel::FontRegistry,destination:&Path)->fontpixel::Result<String>{
    Ok(match destination.extension().and_then(|e|e.to_str()).map(str::to_ascii_lowercase).as_deref(){Some("ttf")=>"ttf",Some("otf")=>"otf",Some("ttc"|"otc")=>"ttc",Some("woff")=>"woff",Some("woff2")=>"woff2",_=>registry.format_from_name(&destination.to_string_lossy())?.id}.into())
}
fn conversion_groups(inputs:&[PathBuf],destination:&Path,id:&str,each:bool,limit:usize)->fontpixel::Result<Vec<ConversionGroup>>{
    if !each{
        let mut used=std::mem::size_of::<ConversionGroup>();conversion_charge(&mut used,destination.as_os_str().len(),limit)?;
        return Ok(vec![ConversionGroup{start:0,end:inputs.len(),output:destination.to_owned()}]);
    }
    if !destination.is_dir(){return Err(Error::InvalidInput("--each output must be an existing directory".into()));}
    let extension=fontpixel::formats::catalogue::CATALOGUE.iter().find(|f|f.id==id).and_then(|f|f.extensions.first()).copied().unwrap_or(id);
    // Check repeated parent/path copies and collision-set scratch before building
    // any destination. A long output parent multiplied by many inputs is bounded.
    let mut planned=inputs.len().checked_mul(std::mem::size_of::<ConversionGroup>()+128).ok_or_else(||conversion_limit(limit))?;
    for input in inputs {
        let name=input.file_name().ok_or_else(||Error::InvalidInput("--each input requires a filename".into()))?;
        let path_bytes=destination.as_os_str().len().checked_add(name.len()).and_then(|n|n.checked_add(extension.len()+2)).and_then(|n|n.checked_mul(6)).ok_or_else(||conversion_limit(limit))?;
        conversion_charge(&mut planned,path_bytes,limit)?;
    }
    let mut paths=std::collections::BTreeSet::new();let mut groups=Vec::new();groups.try_reserve_exact(inputs.len()).map_err(|_|conversion_limit(limit))?;
    for(index,input)in inputs.iter().enumerate(){
        let filename=input.file_name().and_then(|s|s.to_str()).ok_or_else(||Error::InvalidInput("--each input requires a UTF-8 filename".into()))?;
        let filename=filename.strip_suffix(".gz").unwrap_or(filename);
        let stem=Path::new(filename).file_stem().and_then(|s|s.to_str()).filter(|s|!s.is_empty()).ok_or_else(||Error::InvalidInput("--each cannot derive an output filename".into()))?;
        let output=destination.join(format!("{stem}.{extension}"));
        if !paths.insert(output.clone()){return Err(Error::InvalidInput("--each inputs produce duplicate destination names".into()));}
        groups.push(ConversionGroup{start:index,end:index+1,output});
    }Ok(groups)
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
    let file = assets::FileInput::open(
        &args.input,
        &read_options.limits,
        batch::sources(),
    )?;
    let mut registry = fontpixel::registry();
    args.raster.apply(&mut registry)?;
    let loaded = registry.read(&file.input(), &read_options)?;
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
        ReportStyle::Json => {
            #[derive(serde::Serialize)] struct ReportSummary<'a>{report:&'a fontpixel::Report}
            emit_json(output,&ReportSummary{report})
        },
        ReportStyle::Text => {
            write!(output, "{report}")?;
            Ok(())
        }
    }
}

// Borrow the model and report rather than cloning arbitrarily large JSON trees.
struct PropertyValueRef<'a>(&'a fontpixel::model::PropertyValue);
impl serde::Serialize for PropertyValueRef<'_>{
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
        use fontpixel::model::PropertyValue::*;use serde::ser::{SerializeSeq,SerializeStruct};
        match self.0 {
            Integer(v)=>serializer.serialize_i64(*v),Unsigned(v)=>serializer.serialize_u64(*v),Decimal(v)|Text(v)=>serializer.serialize_str(v),Flag=>serializer.serialize_none(),
            Bytes(v)=>{let mut map=serializer.serialize_struct("Bytes",1)?;map.serialize_field("bytes",v)?;map.end()},
            List(v)=>{let mut seq=serializer.serialize_seq(Some(v.len()))?;for value in v{seq.serialize_element(&PropertyValueRef(value))?;}seq.end()},
        }
    }
}
struct PropertiesRef<'a>(&'a fontpixel::model::Properties);
impl serde::Serialize for PropertiesRef<'_>{
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
        use serde::ser::SerializeSeq;
        #[derive(serde::Serialize)]struct PropertyRef<'a>{name:&'a str,value:PropertyValueRef<'a>}
        let mut seq=serializer.serialize_seq(None)?;
        for property in self.0.iter().filter(|p|!p.name.starts_with("bdf.")){seq.serialize_element(&PropertyRef{name:&property.name,value:PropertyValueRef(&property.value)})?;}seq.end()
    }
}
struct UnicodeBlocks<'a>(&'a fontpixel::Font);
impl serde::Serialize for UnicodeBlocks<'_>{
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
        use serde::ser::SerializeSeq;use fontpixel::charset::merge_categories::{UNICODE_BLOCKS,unicode_block};
        #[derive(serde::Serialize)]struct Block {count:usize,end:u32,name:&'static str,start:u32}
        let mut counts=vec![0usize;UNICODE_BLOCKS.len()];let codec=self.0.charset.codec().ok().filter(|c|c.known());
        for(cp,_)in self.0.codepoints(){let scalar=match cp{fontpixel::model::Codepoint::Unicode(value)=>char::from_u32(value).map(|c|c as u32),fontpixel::model::Codepoint::Legacy(value)=>codec.as_ref().and_then(|c|c.cp2unicode(value))};
            if let Some(block)=scalar.and_then(unicode_block){if let Ok(index)=UNICODE_BLOCKS.binary_search_by_key(&block.start,|b|b.start){counts[index]+=1;}}
        }
        let mut seq=serializer.serialize_seq(None)?;for(block,count)in UNICODE_BLOCKS.iter().zip(counts){if count!=0{seq.serialize_element(&Block{count,end:block.end,name:block.name,start:block.start})?;}}seq.end()
    }
}
struct FontInfo<'a>(&'a fontpixel::Font);
impl serde::Serialize for FontInfo<'_>{
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
        use serde::ser::SerializeStruct;
        #[derive(serde::Serialize)]struct Fraction {denominator:u32,numerator:i64}
        let font=self.0;let unencoded=font.glyphs().filter(|(_,g)|g.codepoints.is_empty()).count();
        let count=font.codepoints().count();let first=font.codepoints().next().map(|(cp,_)|cp.value());let last=font.codepoints().next_back().map(|(cp,_)|cp.value());
        let bounds=font.metrics.bounds.map(|b|[i64::from(b.width),i64::from(b.height),b.x,b.y]);
        let mut map=serializer.serialize_struct("FontInfo",21)?;
        map.serialize_field("ascent",&font.metrics.ascent)?;map.serialize_field("bounds",&bounds)?;
        map.serialize_field("charset",&font.charset.canonical_name())?;map.serialize_field("charset_encoding",&font.charset.encoding)?;map.serialize_field("charset_registry",&font.charset.registry)?;
        map.serialize_field("descent",&font.metrics.descent)?;map.serialize_field("distinct_codes",&count)?;map.serialize_field("encoded_count",&(font.len()-unencoded))?;
        map.serialize_field("family",&font.names.family)?;map.serialize_field("first_code",&first)?;map.serialize_field("gl",&font.charset.gl)?;map.serialize_field("glyph_count",&font.len())?;
        map.serialize_field("headers",&PropertiesRef(&font.headers))?;map.serialize_field("last_code",&last)?;map.serialize_field("name",&font.names.xlfd)?;map.serialize_field("pixel_size",&font.metrics.pixel_size)?;
        map.serialize_field("point_size",&font.metrics.point_size.map(|v|Fraction{denominator:v.denominator(),numerator:v.numerator()}))?;
        map.serialize_field("properties",&PropertiesRef(&font.properties))?;map.serialize_field("resolution",&font.metrics.resolution.map(|v|[v.x,v.y]))?;
        map.serialize_field("unencoded_count",&unencoded)?;map.serialize_field("unicode_blocks",&UnicodeBlocks(font))?;map.end()
    }
}
struct FontInfos<'a>(&'a[fontpixel::Font]);
impl serde::Serialize for FontInfos<'_>{
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{use serde::ser::SerializeSeq;let mut seq=serializer.serialize_seq(Some(self.0.len()))?;for font in self.0{seq.serialize_element(&FontInfo(font))?;}seq.end()}
}
#[derive(serde::Serialize)]
struct InfoSummary<'a>{font_count:usize,fonts:FontInfos<'a>,format:&'a str,report:&'a fontpixel::Report}

fn persist(destination: &Path, bytes: Vec<u8>) -> fontpixel::Result<()> {
    let name = destination.file_name().and_then(|name| name.to_str())
        .ok_or_else(|| Error::InvalidInput("output must have a UTF-8 filename".into()))?;
    let bundle = fontpixel::resources::OutputBundle::single(
        fontpixel::ResourcePath::new(name)?, bytes,
    );
    batch::persist_outputs(vec![assets::PendingOutput {
        destination: destination.to_owned(), bundle,
    }], &batch::sources())
}

fn emit_error(error: &Error, style: ReportStyle, output: &mut dyn Write) -> fontpixel::Result<()> {
    match style {
        ReportStyle::Json => {
            #[derive(serde::Serialize)]
            struct ErrorDetails { exit_code: u8, message: String }
            #[derive(serde::Serialize)]
            struct ErrorSummary<'a> { error: ErrorDetails, report: Option<&'a fontpixel::Report> }
            emit_json(output, &ErrorSummary {
                error: ErrorDetails { exit_code: error.exit_code(), message: error.to_string() },
                report: error.report(),
            })
        },
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
        Command::Batch(args) => return ExitCode::from(batch::run(args, &mut io::stdout().lock())),
        command => command,
    };
    let style = report_style(&command);
    match batch::with_source_scope(|| execute(command, &mut io::stdout().lock())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = emit_error(&error, style, &mut io::stderr().lock());
            ExitCode::from(error.exit_code())
        }
    }
}

#[derive(Args, Default)]
struct RasterArguments {
    #[command(flatten)]
    p2: Box<p2_formats::ReadArguments>,
    #[command(flatten)]
    p3: Box<p3_formats::ReadArguments>,
    #[command(flatten)]
    extra: extra_formats::ReadArguments,
    /// C header dialect (default auto).
    #[arg(long, value_parser = ["auto", "font8x8", "tmat", "generic", "gfx", "u8g2", "lvgl", "netbsd", "u8glib", "u8x8"])]
    c_read_family: Option<String>,
    /// Explicit C source or LVGL binary input container (default auto).
    #[arg(long, value_parser = ["source", "lvgl-binary"])]
    c_read_container: Option<String>,
    /// Read the pre-2019 two-byte U8x8 header.
    #[arg(long)]
    c_read_u8x8_legacy: bool,
    /// C input array or GFXfont symbol to select.
    #[arg(long)]
    c_array: Option<String>,
    /// Generic C cell width; requires height, first code and charset.
    #[arg(long, requires_all = ["c_height", "c_first", "c_charset"], value_parser = clap::value_parser!(u32).range(1..))]
    c_width: Option<u32>,
    #[arg(long, requires = "c_width", value_parser = clap::value_parser!(u32).range(1..))]
    c_height: Option<u32>,
    /// First code in generic C arrays, e.g. U+0020 or 32.
    #[arg(long, requires = "c_width")]
    c_first: Option<String>,
    /// unicode or an explicit legacy charset name.
    #[arg(long, requires = "c_width")]
    c_charset: Option<String>,
    /// Generic C baseline down from cell top (default height).
    #[arg(long, requires = "c_width", allow_hyphen_values = true)]
    c_baseline: Option<i64>,
    /// Generic C bytes store the leftmost pixel in bit zero.
    #[arg(long, requires = "c_width")]
    c_lsb_first: bool,
    #[command(flatten)]
    image: image_formats::ReadArguments,
    /// HEX input/output rows (default 16); never inferred from record length.
    #[arg(long, value_parser=clap::value_parser!(u32).range(1..))]
    hex_height: Option<u32>,
    /// HEX baseline-to-bottom distance (default 0).
    #[arg(long)]
    hex_descent: Option<u32>,
    /// Explicit source FNT/FON legacy code page (does not remap byte positions).
    #[arg(long)]
    fnt_read_code_page: Option<String>,
    /// Primary input byte budget (default 64 MiB).
    #[arg(long)]
    max_input_bytes: Option<usize>,
    /// Individual companion-file byte budget.
    #[arg(long)]
    max_resource_bytes: Option<usize>,
    /// Aggregate resource and codec allocation budget (default 256 MiB).
    #[arg(long)]
    max_total_bytes: Option<usize>,
    /// Maximum companion reads and directory entries.
    #[arg(long)]
    max_resources: Option<usize>,
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
        if let Some(maximum) = self.max_resource_bytes {
            limits.max_resource_bytes = maximum;
        }
        if let Some(maximum) = self.max_total_bytes {
            limits.max_total_bytes = maximum;
        }
        if let Some(maximum) = self.max_resources {
            limits.max_resources = maximum;
        }
        limits
    }
    fn apply(&self, registry: &mut fontpixel::FontRegistry) -> fontpixel::Result<()> {
        self.extra.apply(registry)?;
        self.p2.apply(registry)?;
        self.p3.apply(registry)?;
        #[cfg(feature = "c")]
        registry.set_c_read_options(fontpixel::formats::c::CReadOptions {
            family: c_family(self.c_read_family.as_deref().unwrap_or("auto")),
            array_name: self.c_array.clone(),
            generic: self.c_geometry()?,
            container: self.c_read_container.as_deref().map(c_container),
            u8x8_legacy: self.c_read_u8x8_legacy,
        });
        #[cfg(not(feature = "c"))]
        if self.c_read_family.is_some()
            || self.c_array.is_some()
            || self.c_width.is_some()
            || self.c_read_container.is_some()
            || self.c_read_u8x8_legacy
        {
            return Err(Error::FeatureDisabled { format: "c".into() });
        }
        self.image.apply(registry)?;
        #[cfg(feature = "fnt")]
        registry.set_fnt_read_options(fontpixel::formats::fnt::FntReadOptions {
            code_page: self.fnt_read_code_page.clone(),
        });
        #[cfg(not(feature = "fnt"))]
        if self.fnt_read_code_page.is_some() {
            return Err(Error::FeatureDisabled {
                format: "fnt".into(),
            });
        }
        #[cfg(feature = "hex")]
        registry.set_hex_read_options(fontpixel::formats::hex::HexReadOptions {
            height: self.hex_height.unwrap_or(16),
            descent: self.hex_descent.unwrap_or(0),
        });
        #[cfg(not(feature = "hex"))]
        if self.hex_height.is_some() || self.hex_descent.is_some() {
            return Err(Error::FeatureDisabled {
                format: "hex".into(),
            });
        }
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
    /// Intra-font Brotli workers (1..16), independent of batch task workers.
    #[arg(long, value_parser=clap::value_parser!(u32).range(1..=16))]
    woff2_threads: Option<u32>,
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
            || (self.gray_threshold.is_some()
                && ![
                    "hex",
                    "psf",
                    "fnt",
                    "fon",
                    "image",
                    "c",
                    "pbf",
                    "hbf",
                    "draw",
                    "raw",
                    "cpi",
                    "pf2",
                    "fontx2",
                    "hzk",
                    "nfnt",
                    "pk",
                    "gf",
                    "plan9",
                    "amiga",
                    "gem",
                    "snf",
                    "hanfont",
                    "iigs",
                    "vfont",
                    "fzx",
                    "figlet",
                    "palm",
                    "gdr",
                    "drcs",
                    "dosstart",
                    "edwin",
                    "mkwinfont",
                    "psf2txt",
                    "pcr",
                    "plus3dos",
                    "drhalo",
                    "writeon",
                    "daisy",
                    "polyprint",
                    "printshop",
                    "tasprint",
                    "pxl",
                    "signum",
                    "rockbox",
                    "u8m",
                    "cybiko",
                    "playdate",
                    "hmzk",
                    "sbf",
                    "mgtk",
                    "qpf",
                    "symbos",
                    "qpf2",
                    "oberon",
                    "fbit",
                    "lisa",
                    "wsfont",
                    "ssfn",
                    "xerox",
                    "pcgeos",
                    "pfr",
                    "riscos",
                    "gpi",
                    "geos",
                    "prebuilt",
                    "dashen",
                    "hppcl",
                    "vtfont",
                    "chiwriter",
                    "catalyst",
                    "bbc",
                    "comfont",
                    "consoleet",
                    "grasp",
                    "hrcg",
                    "wyse",
                    "xbin",
                    "zapfont",
                ]
                .contains(&id))
            || self.woff2_source.is_some()
            || self.woff2_quality.is_some()
            || self.woff2_threads.is_some()
            || self.collection_v2;
        #[cfg(feature = "svg")]
        if id == "svg" {
            if self.upm.is_some()
                || self.dot_reuse.is_some()
                || self.embed_bitmaps
                || self.woff2_source.is_some()
                || self.woff2_quality.is_some()
            || self.woff2_threads.is_some()
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
            o.woff2.compression_threads = self.woff2_threads.unwrap_or(2) as usize;
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

#[cfg(feature = "c")]
fn c_family(s: &str) -> fontpixel::formats::c::Family {
    use fontpixel::formats::c::Family;
    match s {
        "font8x8" => Family::Font8x8,
        "tmat" => Family::Tmat,
        "generic" => Family::Generic,
        "gfx" => Family::Gfx,
        "u8g2" => Family::U8g2,
        "lvgl" => Family::Lvgl,
        "netbsd" => Family::Netbsd,
        "u8glib" => Family::U8glib,
        "u8x8" => Family::U8x8,
        _ => Family::Auto,
    }
}
#[cfg(feature = "c")]
fn c_container(value: &str) -> fontpixel::formats::c::CContainer {
    if value == "lvgl-binary" {
        fontpixel::formats::c::CContainer::LvglBinary
    } else {
        fontpixel::formats::c::CContainer::Source
    }
}
#[cfg(feature = "c")]
impl RasterArguments {
    fn c_geometry(&self) -> fontpixel::Result<Option<fontpixel::formats::c::ArrayOptions>> {
        let Some(width) = self.c_width else {
            return Ok(None);
        };
        let height = self
            .c_height
            .ok_or_else(|| Error::InvalidInput("C height is required".into()))?;
        let first = self
            .c_first
            .as_deref()
            .ok_or_else(|| Error::InvalidInput("C first code is required".into()))?;
        Ok(Some(fontpixel::formats::c::ArrayOptions {
            width,
            height,
            first_code: parse_code(first)?,
            charset: self
                .c_charset
                .clone()
                .ok_or_else(|| Error::InvalidInput("C charset is required".into()))?,
            baseline: self.c_baseline.unwrap_or(i64::from(height)),
            lsb_first: self.c_lsb_first,
        }))
    }
}
#[derive(Args, Default)]
struct COutputArguments {
    /// Output C dialect (default generic).
    #[arg(long, value_parser = ["generic", "gfx", "u8g2", "lvgl", "netbsd", "u8glib", "u8x8"])]
    c_family: Option<String>,
    /// C source (default) or native LVGL binary output.
    #[arg(long, value_parser = ["source", "lvgl-binary"])]
    c_container: Option<String>,
    /// U8glib native font record format (default full).
    #[arg(long, value_parser = ["full", "compact", "gray2"])]
    u8glib_format: Option<String>,
    /// Write the pre-2019 two-byte U8x8 header.
    #[arg(long)]
    u8x8_legacy: bool,
    /// Output C symbol (default font).
    #[arg(long)]
    c_name: Option<String>,
    /// Inclusive C output range, e.g. U+0020-U+007E.
    #[arg(long)]
    c_range: Option<String>,
    /// LVGL coverage bit depth (default 1).
    #[arg(long, value_parser = ["1", "2", "4", "8"])]
    lvgl_bpp: Option<String>,
    /// LVGL bitmap encoding (default plain).
    #[arg(long, value_parser = ["plain", "rle", "rle-xor"])]
    lvgl_compression: Option<String>,
}
impl COutputArguments {
    fn apply(
        &self,
        id: &str,
        raster: &RasterArguments,
        threshold: Option<u8>,
        registry: &mut fontpixel::FontRegistry,
    ) -> fontpixel::Result<()> {
        let explicit = self.c_container.is_some()
            || self.u8glib_format.is_some()
            || self.u8x8_legacy
            || self.c_family.is_some()
            || self.c_name.is_some()
            || self.c_range.is_some()
            || self.lvgl_bpp.is_some()
            || self.lvgl_compression.is_some();
        if id != "c" && explicit {
            return Err(Error::InvalidInput(
                "C output options require --to c".into(),
            ));
        }
        if (self.lvgl_bpp.is_some() || self.lvgl_compression.is_some())
            && self.c_family.as_deref() != Some("lvgl")
        {
            return Err(Error::InvalidInput(
                "LVGL output options require --c-family lvgl".into(),
            ));
        }
        if self.c_container.as_deref() == Some("lvgl-binary")
            && self.c_family.as_deref() != Some("lvgl")
        {
            return Err(Error::InvalidInput(
                "LVGL binary output requires --c-family lvgl".into(),
            ));
        }
        if self.u8glib_format.is_some() && self.c_family.as_deref() != Some("u8glib") {
            return Err(Error::InvalidInput(
                "--u8glib-format requires --c-family u8glib".into(),
            ));
        }
        if self.u8x8_legacy && self.c_family.as_deref() != Some("u8x8") {
            return Err(Error::InvalidInput(
                "--u8x8-legacy requires --c-family u8x8".into(),
            ));
        }
        #[cfg(feature = "c")]
        if id == "c" {
            let range = self
                .c_range
                .as_deref()
                .map(|s| {
                    let (a, b) = s
                        .split_once('-')
                        .ok_or_else(|| Error::InvalidInput("C range requires FIRST-LAST".into()))?;
                    Ok::<_, Error>((parse_code(a)?, parse_code(b)?))
                })
                .transpose()?;
            registry.set_c_write_options(fontpixel::formats::c::CWriteOptions {
                family: c_family(self.c_family.as_deref().unwrap_or("generic")),
                container: c_container(self.c_container.as_deref().unwrap_or("source")),
                u8x8_legacy: self.u8x8_legacy,
                u8glib_format: match self.u8glib_format.as_deref() {
                    Some("compact") => fontpixel::formats::c::U8glibFormat::Compact,
                    Some("gray2") => fontpixel::formats::c::U8glibFormat::Gray2,
                    _ => fontpixel::formats::c::U8glibFormat::Full,
                },
                array_name: self.c_name.clone().unwrap_or_else(|| "font".into()),
                generic: raster.c_geometry()?,
                range,
                gray_threshold: threshold,
                limits: raster.input_limits(),
                lvgl_bpp: match self.lvgl_bpp.as_deref() {
                    Some("2") => fontpixel::formats::c::LvglBpp::Two,
                    Some("4") => fontpixel::formats::c::LvglBpp::Four,
                    Some("8") => fontpixel::formats::c::LvglBpp::Eight,
                    _ => fontpixel::formats::c::LvglBpp::One,
                },
                lvgl_compression: match self.lvgl_compression.as_deref() {
                    Some("rle") => fontpixel::formats::c::LvglCompression::Rle,
                    Some("rle-xor") => fontpixel::formats::c::LvglCompression::RleXor,
                    _ => fontpixel::formats::c::LvglCompression::Plain,
                },
            });
        }
        #[cfg(not(feature = "c"))]
        {
            let _ = (raster, threshold, registry);
            if explicit || id == "c" {
                return Err(Error::FeatureDisabled { format: "c".into() });
            }
        }
        Ok(())
    }
}
