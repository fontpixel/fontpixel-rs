//! `preview` and `render`: text layout as terminal character art or an image.
use crate::context::{self, PreparedOutput};
use crate::options::{
    DrawMode, DrawOrder, LayoutOptions, ReportStyle, parse_code, render_bitmap_limits,
};
use fontpixel::{Error, ReadOptions, Report};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Render text (or every mapping) as terminal character art.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct PreviewRequest {
    /// Input, text, layout and budgets.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub layout: LayoutOptions,
    /// Exactly two characters: ink then paper.
    #[cfg_attr(feature = "clap", arg(long, default_value = "#."))]
    pub chars: String,
}
impl Default for PreviewRequest {
    fn default() -> Self {
        Self {
            layout: LayoutOptions::default(),
            chars: "#.".into(),
        }
    }
}
impl PreviewRequest {
    /// The original total byte limit, before the standalone identity partition.
    pub fn source_identity_total_bytes(&self) -> usize {
        self.layout.raster.original_total_bytes()
    }
}
/// Character art and diagnostics of one preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewResponse {
    /// Rows of ink/paper characters without a final newline; empty for no pixels.
    pub text: String,
    /// Read and render diagnostics.
    pub report: Report,
}
impl PreviewResponse {
    /// Write the character art with its final newline (nothing when empty).
    pub fn write_text(&self, out: &mut dyn Write) -> fontpixel::Result<()> {
        if !self.text.is_empty() {
            writeln!(out, "{}", self.text)?;
        }
        Ok(())
    }
    /// Write the diagnostics document (the command-line tool sends it to stderr,
    /// and only when it has entries).
    pub fn write_report(
        &self,
        style: ReportStyle,
        out: &mut dyn Write,
        pretty: bool,
    ) -> fontpixel::Result<()> {
        write_report_document(&self.report, style, out, pretty)
    }
}
pub(crate) fn preview(request: PreviewRequest) -> fontpixel::Result<PreviewResponse> {
    let PreviewRequest { layout, chars } = request;
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
    Ok(PreviewResponse {
        text,
        report: rendered.report,
    })
}

/// Render text (or every mapping) to a PNG or SVG image.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct RenderRequest {
    /// Input, text, layout and budgets.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub layout: LayoutOptions,
    /// PNG or SVG output; written only after successful validation.
    #[cfg_attr(feature = "clap", arg(short, long))]
    pub output: PathBuf,
    /// Explicit image type; otherwise inferred from output extension.
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["png", "svg"]))]
    pub to: Option<String>,
    /// Positive integer nearest-neighbor scale.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..)))]
    pub scale: u32,
    /// Ink color: #RGB, #RGBA, #RRGGBB, #RRGGBBAA, or transparent.
    #[cfg_attr(feature = "clap", arg(long, default_value = "#000000"))]
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_text::rgba"))]
    pub color: fontpixel::render::Rgba,
    /// Paper color, in the same syntax as --color.
    #[cfg_attr(feature = "clap", arg(long, default_value = "#ffffff"))]
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_text::rgba"))]
    pub background: fontpixel::render::Rgba,
    /// SVG pixel shape (PNG uses square pixels).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["square", "square-separate", "dot"]))]
    pub shape: Option<String>,
    /// SVG dot diameter or separate-square side relative to one pixel.
    #[cfg_attr(feature = "clap", arg(long))]
    pub dotsize: Option<f64>,
}
impl Default for RenderRequest {
    fn default() -> Self {
        Self {
            layout: LayoutOptions::default(),
            output: PathBuf::new(),
            to: None,
            scale: 1,
            color: fontpixel::render::Rgba::BLACK,
            background: fontpixel::render::Rgba::WHITE,
            shape: None,
            dotsize: None,
        }
    }
}
impl RenderRequest {
    /// The original total byte limit, before the standalone identity partition.
    pub fn source_identity_total_bytes(&self) -> usize {
        self.layout.raster.original_total_bytes()
    }
}
/// Summary of one rendered image.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RenderResponse {
    /// Encoded image bytes.
    pub bytes: usize,
    /// `png` or `svg`.
    pub format: String,
    /// Image height in source pixels.
    pub height: u32,
    /// Output path.
    pub output: PathBuf,
    /// Read and render diagnostics.
    pub report: Report,
    /// Nearest-neighbor scale.
    pub scale: u32,
    /// Image width in source pixels.
    pub width: u32,
}
impl RenderResponse {
    /// Write the command-line JSON document.
    pub fn write_json(&self, out: &mut dyn Write, pretty: bool) -> fontpixel::Result<()> {
        crate::json::write(out, self, pretty)
    }
    /// Write the command-line text report.
    pub fn write_text(&self, out: &mut dyn Write) -> fontpixel::Result<()> {
        write!(out, "{}", self.report)?;
        writeln!(
            out,
            "Wrote {} bytes to {}",
            self.bytes,
            self.output.display()
        )?;
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
pub(crate) fn render(request: RenderRequest) -> fontpixel::Result<RenderResponse> {
    let RenderRequest {
        layout,
        output: destination,
        to,
        scale,
        color,
        background,
        shape,
        dotsize,
    } = request;
    let kind = to
        .or_else(|| {
            destination
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
        })
        .ok_or_else(|| Error::InvalidInput("output needs .png/.svg or --to png/svg".into()))?;
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
            let shape = crate::options::svg_shape(shape.as_deref());
            fontpixel::formats::svg::image(
                &rendered.bitmap,
                &fontpixel::formats::svg::ImageOptions {
                    image: image_options,
                    shape,
                    dot_size: dotsize.unwrap_or(if shape == fontpixel::outline::Shape::Dot {
                        0.9
                    } else {
                        1.
                    }),
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
    Ok(RenderResponse {
        bytes: byte_count,
        format: kind,
        height: rendered.bitmap.height(),
        output: destination,
        report: rendered.report,
        scale,
        width: rendered.bitmap.width(),
    })
}

/// Persist one single-asset output through the active scope.
pub(crate) fn persist(destination: &Path, bytes: Vec<u8>) -> fontpixel::Result<()> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::InvalidInput("output must have a UTF-8 filename".into()))?;
    let bundle =
        fontpixel::resources::OutputBundle::single(fontpixel::ResourcePath::new(name)?, bytes);
    context::persist_outputs(
        vec![PreparedOutput {
            destination: destination.to_owned(),
            bundle,
        }],
        &context::sources(),
    )
}

/// Write a `{"report": ...}` document or the text report.
pub(crate) fn write_report_document(
    report: &Report,
    style: ReportStyle,
    output: &mut dyn Write,
    pretty: bool,
) -> fontpixel::Result<()> {
    match style {
        ReportStyle::Json => {
            #[derive(serde::Serialize)]
            struct ReportSummary<'a> {
                report: &'a Report,
            }
            crate::json::write(output, &ReportSummary { report }, pretty)
        }
        ReportStyle::Text => {
            write!(output, "{report}")?;
            Ok(())
        }
    }
}

pub(crate) fn render_layout(
    args: &LayoutOptions,
) -> fontpixel::Result<fontpixel::render::Rendered> {
    use fontpixel::model::Codepoint;
    use fontpixel::render::{GlyphOrder, MissingGlyph, RenderMode, RenderOptions};
    let read_options = ReadOptions {
        format: args.from.clone(),
        limits: args.raster.input_limits(),
    };
    let file = context::open_input(&args.input, &read_options.limits, context::sources())?;
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
