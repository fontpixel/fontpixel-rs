//! Shared option groups mirroring the command-line argument groups.
//!
//! Each group is a plain typed struct. With the `clap` feature the same structs
//! derive `clap::Args`, so the command-line parser and the library share one
//! definition; with the `serde` feature they deserialize from flat key maps.
use crate::{context, extra_formats, image_formats, p2_formats, p3_formats};
#[cfg(feature = "clap")]
use clap::builder::PossibleValuesParser;
use fontpixel::{Error, ResourceLimits};
use std::path::PathBuf;

/// Input, text layout and image budgets shared by `preview` and `render`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct LayoutOptions {
    /// Input decoding options and resource limits.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub raster: RasterOptions,
    /// Input font (BDF/PCF, including gzip when enabled).
    pub input: PathBuf,
    /// Text in logical input order. Newline is an ordinary glyph, as in SPEC.
    #[cfg_attr(
        feature = "clap",
        arg(required_unless_present = "all", conflicts_with = "all")
    )]
    pub text: Option<String>,
    /// Render every effective encoded mapping (drawall).
    #[cfg_attr(feature = "clap", arg(long))]
    pub all: bool,
    /// Inclusive code ranges, e.g. U+0020-007E,U+4E00-4EFF; requires --all.
    #[cfg_attr(feature = "clap", arg(long, requires = "all"))]
    pub range: Option<String>,
    /// Ordering for --all.
    #[cfg_attr(
        feature = "clap",
        arg(long, value_enum, default_value = "ascending", requires = "all")
    )]
    pub order: DrawOrder,
    /// Input format override.
    #[cfg_attr(feature = "clap", arg(long, value_parser = format_parser()))]
    pub from: Option<String>,
    /// Zero-based strike index; required for inputs containing multiple fonts.
    #[cfg_attr(feature = "clap", arg(long))]
    pub font_index: Option<usize>,
    /// lr, rl, tb, bt, or a four-letter pair such as lrbt or tbrl.
    #[cfg_attr(feature = "clap", arg(long, default_value = "lrtb"))]
    pub direction: fontpixel::render::WritingDirection,
    /// Maximum accumulated width-plus-offset per line (SPEC vertical behavior).
    #[cfg_attr(
        feature = "clap",
        arg(
            long,
            alias = "linelimit",
            default_value_t = 512,
            allow_hyphen_values = true
        )
    )]
    pub line_limit: i64,
    /// Advance (1), complete font cell (0), or tight ink; --all defaults to cell.
    #[cfg_attr(feature = "clap", arg(long, value_enum))]
    pub mode: Option<DrawMode>,
    /// Use the current glyph's spacing at boundaries instead of the previous glyph.
    #[cfg_attr(feature = "clap", arg(long))]
    pub use_current_glyph_spacing: bool,
    /// Additional signed gap between glyphs.
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value_t = 0, allow_hyphen_values = true)
    )]
    pub letter_spacing: i64,
    /// Additional signed gap between lines/columns.
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value_t = 0, allow_hyphen_values = true)
    )]
    pub line_spacing: i64,
    /// Replacement glyph code, e.g. U+003F, 0x3f, or 63.
    #[cfg_attr(feature = "clap", arg(long, conflicts_with = "fail_missing"))]
    pub missing: Option<String>,
    /// Fail on any absent/unrepresentable character.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fail_missing: bool,
    /// Explicit grayscale-to-monochrome threshold (0..255).
    #[cfg_attr(feature = "clap", arg(long))]
    pub gray_threshold: Option<u8>,
    /// Reject read/render loss before displaying or saving output.
    #[cfg_attr(feature = "clap", arg(long))]
    pub strict: bool,
    /// Maximum rendered/scaled pixel count; increase for large --all atlases.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 16 * 1024 * 1024))]
    pub max_pixels: usize,
    /// Maximum rendered width or height.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 1_048_576))]
    pub max_dimension: u32,
    /// Maximum layout bookkeeping bytes.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 128 * 1024 * 1024))]
    pub max_layout_bytes: usize,
    /// Maximum diagnostic text and deduplication bookkeeping bytes.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 16 * 1024 * 1024))]
    pub max_report_bytes: usize,
    /// Maximum number of input glyphs, including missing ones.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 1_000_000))]
    pub max_glyphs: usize,
    /// Maximum encoded image/text bytes.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 64 * 1024 * 1024))]
    pub max_output_bytes: usize,
    /// Diagnostics format (preview diagnostics go to stderr).
    #[cfg_attr(feature = "clap", arg(long, value_enum, default_value = "text"))]
    pub report: ReportStyle,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(LayoutOptions);
impl LayoutOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        self.raster.validate()?;
        if let Some(value) = self.from.as_deref() {
            check_format("from", value)?;
        }
        crate::options::check_conflict((self.text.is_some(), "[TEXT]"), (self.all, "--all"))?;
        crate::options::check_conflict(
            (self.missing.is_some(), "--missing <MISSING>"),
            (self.fail_missing, "--fail-missing"),
        )?;
        crate::options::check_required(&[
            (crate::options::given(&self.input), "<INPUT>"),
            // `--range`/`--order` require `--all`, which the parser waives
            // when the conflicting text is present.
            (
                self.all
                    || self.text.is_some()
                    || (self.range.is_none() && self.order == DrawOrder::Ascending),
                "--all",
            ),
            (self.all || self.text.is_some(), "<TEXT>"),
        ])?;
        Ok(())
    }
}
impl Default for LayoutOptions {
    /// The command-line defaults; `input` is empty and must be set.
    fn default() -> Self {
        Self {
            raster: RasterOptions::default(),
            input: PathBuf::new(),
            text: None,
            all: false,
            range: None,
            order: DrawOrder::Ascending,
            from: None,
            font_index: None,
            direction: fontpixel::render::WritingDirection::default(),
            line_limit: 512,
            mode: None,
            use_current_glyph_spacing: false,
            letter_spacing: 0,
            line_spacing: 0,
            missing: None,
            fail_missing: false,
            gray_threshold: None,
            strict: false,
            max_pixels: 16 * 1024 * 1024,
            max_dimension: 1_048_576,
            max_layout_bytes: 128 * 1024 * 1024,
            max_report_bytes: 16 * 1024 * 1024,
            max_glyphs: 1_000_000,
            max_output_bytes: 64 * 1024 * 1024,
            report: ReportStyle::Text,
        }
    }
}
/// Glyph spacing mode for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum DrawMode {
    /// Place glyphs by advance width (SPEC mode 1).
    #[cfg_attr(feature = "clap", value(alias = "1", help = None::<&str>))]
    #[cfg_attr(feature = "serde", serde(alias = "1"))]
    Advance,
    /// Concatenate complete font cells (SPEC mode 0).
    #[cfg_attr(feature = "clap", value(alias = "0", help = None::<&str>))]
    #[cfg_attr(feature = "serde", serde(alias = "0"))]
    Cell,
    /// Concatenate tight ink rectangles.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    Ink,
}
/// Glyph ordering for `--all` rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum DrawOrder {
    /// Ascending code order.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    Ascending,
    /// Descending code order.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    Descending,
    /// Source file order.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    File,
    /// Reverse source file order.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    ReverseFile,
}

const PCF_OPTIONS_GLYPH_PAD: [&str; 4] = ["1", "2", "4", "8"];
const PCF_OPTIONS_SCAN_UNIT: [&str; 3] = ["1", "2", "4"];
/// PCF output layout and metadata options (`--to pcf` only).
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct PcfOptions {
    /// PCF table integer byte order.
    #[cfg_attr(
        feature = "clap",
        arg(long = "byteorder", alias = "byte-order", value_enum)
    )]
    pub byte_order: Option<Significance>,
    /// PCF bitmap bit order.
    #[cfg_attr(
        feature = "clap",
        arg(long = "bitorder", alias = "bit-order", value_enum)
    )]
    pub bit_order: Option<Significance>,
    /// PCF row alignment in bytes (1, 2, 4 or 8).
    #[cfg_attr(feature = "clap", arg(long = "glyphpad", alias = "glyph-pad", value_parser = PCF_OPTIONS_GLYPH_PAD))]
    pub glyph_pad: Option<String>,
    /// PCF bitmap scan unit in bytes (1, 2 or 4).
    #[cfg_attr(feature = "clap", arg(long = "scanunit", alias = "scan-unit", value_parser = PCF_OPTIONS_SCAN_UNIT))]
    pub scan_unit: Option<String>,
    /// Whether edited size/resolution rewrite existing PCF POINT_SIZE and
    /// RESOLUTION* properties (normalized-edits, the default) or the existing
    /// properties are kept and each disagreement is reported as loss.
    #[cfg_attr(feature = "clap", arg(long, value_enum))]
    pub pcf_metadata_precedence: Option<PcfMetadataPrecedence>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(PcfOptions);
impl PcfOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.glyph_pad.as_deref() {
            check_choice("glyph_pad", value, &PCF_OPTIONS_GLYPH_PAD)?;
        }
        if let Some(value) = self.scan_unit.as_deref() {
            check_choice("scan_unit", value, &PCF_OPTIONS_SCAN_UNIT)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
/// Most- or least-significant-first ordering.
pub enum Significance {
    /// Most significant first.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    Msb,
    /// Least significant first.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    Lsb,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
/// Whether normalized size edits or existing PCF properties win.
pub enum PcfMetadataPrecedence {
    /// Rewrite POINT_SIZE/RESOLUTION* from normalized edits.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    NormalizedEdits,
    /// Keep explicit properties and report disagreements as loss.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    ExplicitProperties,
}
impl PcfOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        registry: &mut fontpixel::FontRegistry,
    ) -> fontpixel::Result<()> {
        let requested = self.byte_order.is_some()
            || self.bit_order.is_some()
            || self.glyph_pad.is_some()
            || self.scan_unit.is_some()
            || self.pcf_metadata_precedence.is_some();
        if !requested {
            return Ok(());
        }
        if id != "pcf" {
            return Err(Error::InvalidInput(
                "PCF layout and metadata options require --to pcf".into(),
            ));
        }
        #[cfg(feature = "pcf")]
        {
            use fontpixel::formats::pcf::{
                GlyphPad, MetadataPrecedence, Order, PcfWriteOptions, ScanUnit,
            };
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
                metadata_precedence: match self.pcf_metadata_precedence {
                    Some(PcfMetadataPrecedence::ExplicitProperties) => {
                        MetadataPrecedence::ExplicitProperties
                    }
                    _ => MetadataPrecedence::NormalizedEdits,
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

const CONSOLE_OPTIONS_PSF_VERSION: [&str; 2] = ["1", "2"];
const CONSOLE_OPTIONS_PSF_UNICODE_TABLE: [&str; 3] = ["auto", "include", "omit"];
const CONSOLE_OPTIONS_FNT_VERSION: [&str; 3] = ["1", "2", "3"];
/// PSF and Windows FNT/FON output options.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct ConsoleOptions {
    /// PSF output version, 1 or 2 (default 2).
    #[cfg_attr(feature = "clap", arg(long, value_parser = CONSOLE_OPTIONS_PSF_VERSION))]
    pub psf_version: Option<String>,
    /// PSF1 cells, 256 or 512; default chooses the smallest fit.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u16)))]
    pub psf1_count: Option<u16>,
    /// PSF Unicode table policy (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = CONSOLE_OPTIONS_PSF_UNICODE_TABLE))]
    pub psf_unicode_table: Option<String>,
    /// Windows raster output version; defaults to source version or 2.
    #[cfg_attr(feature = "clap", arg(long, value_parser = CONSOLE_OPTIONS_FNT_VERSION))]
    pub fnt_version: Option<String>,
    /// Windows dfCharSet identifier, 0..255 (ANSI=0, OEM=255).
    #[cfg_attr(feature = "clap", arg(long))]
    pub fnt_charset: Option<u8>,
    /// Explicit target page for OEM, symbol, or historical Windows mappings.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fnt_code_page: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(ConsoleOptions);
impl ConsoleOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.psf_version.as_deref() {
            check_choice("psf_version", value, &CONSOLE_OPTIONS_PSF_VERSION)?;
        }
        if let Some(value) = self.psf_unicode_table.as_deref() {
            check_choice(
                "psf_unicode_table",
                value,
                &CONSOLE_OPTIONS_PSF_UNICODE_TABLE,
            )?;
        }
        if let Some(value) = self.fnt_version.as_deref() {
            check_choice("fnt_version", value, &CONSOLE_OPTIONS_FNT_VERSION)?;
        }
        Ok(())
    }
}
impl ConsoleOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        raster: &RasterOptions,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
/// Diagnostic and summary presentation style.
pub enum ReportStyle {
    /// Human-readable text.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    #[default]
    Text,
    /// Machine-readable JSON.
    #[cfg_attr(feature = "clap", value(help = None::<&str>))]
    Json,
}

/// Command-line value parser accepting every catalogue format identifier.
#[cfg(feature = "clap")]
pub(crate) fn format_parser() -> PossibleValuesParser {
    PossibleValuesParser::new(
        fontpixel::formats::catalogue::CATALOGUE
            .iter()
            .map(|format| format.id),
    )
}

/// Reject a value outside a command-line choice list.
pub(crate) fn check_choice(name: &str, value: &str, allowed: &[&str]) -> fontpixel::Result<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "invalid value '{value}' for {name} [possible values: {}]",
            allowed.join(", ")
        )))
    }
}
/// Reject an unknown catalogue format identifier.
pub(crate) fn check_format(name: &str, value: &str) -> fontpixel::Result<()> {
    if fontpixel::formats::catalogue::CATALOGUE
        .iter()
        .any(|format| format.id == value)
    {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "invalid value '{value}' for {name}: unknown format identifier"
        )))
    }
}
/// Reject a value the command-line value parser rejects.
pub(crate) fn check_parsed<T>(
    name: &str,
    value: &str,
    parser: fn(&str) -> std::result::Result<T, String>,
) -> fontpixel::Result<()> {
    parser(value).map(drop).map_err(|error| {
        Error::InvalidInput(format!("invalid value '{value}' for {name}: {error}"))
    })
}
/// Reject absent required arguments with the command-line parser's text.
/// Each entry is `(present, display)`, e.g. `(false, "--to <TO>")`.
pub(crate) fn check_required(arguments: &[(bool, &str)]) -> fontpixel::Result<()> {
    let missing: String = arguments
        .iter()
        .filter(|(present, _)| !present)
        .map(|(_, display)| format!("\n  {display}"))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "the following required arguments were not provided:{missing}"
        )))
    }
}
/// When `present`, require every listed argument (clap `requires`).
pub(crate) fn check_requires(present: bool, required: &[(bool, &str)]) -> fontpixel::Result<()> {
    if present {
        check_required(required)
    } else {
        Ok(())
    }
}
/// Reject two arguments given together (clap `conflicts_with`).
pub(crate) fn check_conflict(first: (bool, &str), second: (bool, &str)) -> fontpixel::Result<()> {
    if first.0 && second.0 {
        Err(Error::InvalidInput(format!(
            "the argument '{}' cannot be used with '{}'",
            first.1, second.1
        )))
    } else {
        Ok(())
    }
}
/// Whether a required path argument was given (the parser rejects empty paths).
pub(crate) fn given(path: &std::path::Path) -> bool {
    !path.as_os_str().is_empty()
}
/// Reject a number outside the command-line range.
pub(crate) fn check_range(name: &str, value: i128, low: i128, high: i128) -> fontpixel::Result<()> {
    if (low..=high).contains(&value) {
        Ok(())
    } else {
        // The parser's text: the option as `--long-name <VALUE_NAME>`.
        Err(Error::InvalidInput(format!(
            "invalid value '{value}' for '--{} <{}>': {value} is not in {low}..={high}",
            name.replace('_', "-"),
            name.to_ascii_uppercase()
        )))
    }
}
pub(crate) fn parse_code(value: &str) -> fontpixel::Result<u32> {
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
pub(crate) fn render_bitmap_limits(args: &LayoutOptions) -> fontpixel::bitmap::BitmapLimits {
    fontpixel::bitmap::BitmapLimits {
        max_dimension: args.max_dimension,
        max_pixels: args.max_pixels,
        max_bytes: args.max_output_bytes,
    }
}
const RASTER_OPTIONS_C_READ_FAMILY: [&str; 10] = [
    "auto", "font8x8", "tmat", "generic", "gfx", "u8g2", "lvgl", "netbsd", "u8glib", "u8x8",
];
const RASTER_OPTIONS_C_READ_CONTAINER: [&str; 2] = ["source", "lvgl-binary"];
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
/// Input decoding options and resource limits shared by every reading command.
pub struct RasterOptions {
    /// Second-priority format read options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub p2: Box<p2_formats::ReadOptions>,
    /// Third-priority format read options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub p3: Box<p3_formats::ReadOptions>,
    /// Additional format read options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub extra: extra_formats::ReadOptions,
    /// C header dialect (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = RASTER_OPTIONS_C_READ_FAMILY))]
    pub c_read_family: Option<String>,
    /// Explicit C source or LVGL binary input container (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = RASTER_OPTIONS_C_READ_CONTAINER))]
    pub c_read_container: Option<String>,
    /// Read the pre-2019 two-byte U8x8 header.
    #[cfg_attr(feature = "clap", arg(long))]
    pub c_read_u8x8_legacy: bool,
    /// C input array or GFXfont symbol to select.
    #[cfg_attr(feature = "clap", arg(long))]
    pub c_array: Option<String>,
    /// Generic C cell width; requires height, first code and charset.
    #[cfg_attr(feature = "clap", arg(long, requires_all = ["c_height", "c_first", "c_charset"], value_parser = clap::value_parser!(u32).range(1..)))]
    pub c_width: Option<u32>,
    /// Generic C cell height; requires `c_width`.
    #[cfg_attr(feature = "clap", arg(long, requires = "c_width", value_parser = clap::value_parser!(u32).range(1..), help = None::<&str>, long_help = None::<&str>))]
    pub c_height: Option<u32>,
    /// First code in generic C arrays, e.g. U+0020 or 32.
    #[cfg_attr(feature = "clap", arg(long, requires = "c_width"))]
    pub c_first: Option<String>,
    /// unicode or an explicit legacy charset name.
    #[cfg_attr(feature = "clap", arg(long, requires = "c_width"))]
    pub c_charset: Option<String>,
    /// Generic C baseline down from cell top (default height).
    #[cfg_attr(
        feature = "clap",
        arg(long, requires = "c_width", allow_hyphen_values = true)
    )]
    pub c_baseline: Option<i64>,
    /// Generic C bytes store the leftmost pixel in bit zero.
    #[cfg_attr(feature = "clap", arg(long, requires = "c_width"))]
    pub c_lsb_first: bool,
    /// Image atlas and directory read options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub image: image_formats::ReadOptions,
    /// HEX input/output rows (default 16); never inferred from record length.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u32).range(1..)))]
    pub hex_height: Option<u32>,
    /// HEX baseline-to-bottom distance (default 0).
    #[cfg_attr(feature = "clap", arg(long))]
    pub hex_descent: Option<u32>,
    /// Explicit source FNT/FON legacy code page (does not remap byte positions).
    #[cfg_attr(feature = "clap", arg(long))]
    pub fnt_read_code_page: Option<String>,
    /// Primary input byte budget (default 64 MiB).
    #[cfg_attr(feature = "clap", arg(long))]
    pub max_input_bytes: Option<usize>,
    /// Individual companion-file byte budget.
    #[cfg_attr(feature = "clap", arg(long))]
    pub max_resource_bytes: Option<usize>,
    /// Aggregate resource and codec allocation budget (default 256 MiB).
    #[cfg_attr(feature = "clap", arg(long))]
    pub max_total_bytes: Option<usize>,
    /// Maximum companion reads and directory entries.
    #[cfg_attr(feature = "clap", arg(long))]
    pub max_resources: Option<usize>,
    /// Explicit raster size for reviewed off-grid/curved outlines.
    #[cfg_attr(feature = "clap", arg(long))]
    pub ppem: Option<u16>,
    /// Select a TTC/OTC face (zero based); otherwise read every face.
    #[cfg_attr(feature = "clap", arg(long))]
    pub face_index: Option<usize>,
    /// Rasterize outlines even when matching embedded images exist.
    #[cfg_attr(feature = "clap", arg(long))]
    pub no_embedded_bitmaps: bool,
    /// Explicit design units per pixel; requires --ppem.
    #[cfg_attr(feature = "clap", arg(long, requires = "ppem"))]
    pub grid_units: Option<u16>,
    /// Reviewed horizontal and vertical translations in pixel units.
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value_t = 0.0, allow_hyphen_values = true)
    )]
    pub pixel_offset_x: f64,
    /// Reviewed vertical translation in pixel units.
    #[cfg_attr(feature = "clap", arg(long, default_value_t = 0.0, allow_hyphen_values = true, help = None::<&str>, long_help = None::<&str>))]
    pub pixel_offset_y: f64,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(RasterOptions);
impl RasterOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        self.p2.validate()?;
        self.p3.validate()?;
        self.extra.validate()?;
        if let Some(value) = self.c_read_family.as_deref() {
            check_choice("c_read_family", value, &RASTER_OPTIONS_C_READ_FAMILY)?;
        }
        if let Some(value) = self.c_read_container.as_deref() {
            check_choice("c_read_container", value, &RASTER_OPTIONS_C_READ_CONTAINER)?;
        }
        if let Some(value) = self.c_width {
            check_range("c_width", i128::from(value), 1, i128::from(u32::MAX))?;
        }
        if let Some(value) = self.c_height {
            check_range("c_height", i128::from(value), 1, i128::from(u32::MAX))?;
        }
        self.image.validate()?;
        if let Some(value) = self.hex_height {
            check_range("hex_height", i128::from(value), 1, i128::from(u32::MAX))?;
        }
        // Every generic C option requires `--c-width`, which in turn requires
        // height, first code and charset; the parser lists the closure.
        let c_dependent = self.c_height.is_some()
            || self.c_first.is_some()
            || self.c_charset.is_some()
            || self.c_baseline.is_some()
            || self.c_lsb_first;
        crate::options::check_requires(
            self.c_width.is_some() || c_dependent,
            &[
                (self.c_height.is_some(), "--c-height <C_HEIGHT>"),
                (self.c_first.is_some(), "--c-first <C_FIRST>"),
                (self.c_charset.is_some(), "--c-charset <C_CHARSET>"),
                (self.c_width.is_some(), "--c-width <C_WIDTH>"),
            ],
        )?;
        crate::options::check_requires(
            self.grid_units.is_some(),
            &[(self.ppem.is_some(), "--ppem <PPEM>")],
        )?;
        Ok(())
    }
}
impl RasterOptions {
    /// The total byte limit as requested, before any standalone partition.
    pub fn original_total_bytes(&self) -> usize {
        self.max_total_bytes
            .unwrap_or_else(|| ResourceLimits::default().max_total_bytes)
    }
    pub(crate) fn input_limits(&self) -> ResourceLimits {
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
        context::standalone_working_limits(limits)
    }
    pub(crate) fn apply(&self, registry: &mut fontpixel::FontRegistry) -> fontpixel::Result<()> {
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
#[cfg(feature = "clap")]
const VECTOR_OPTIONS_SHAPE: [&str; 3] = ["square", "square-separate", "dot"];
const VECTOR_OPTIONS_DOT_REUSE: [&str; 3] = ["literal", "rows", "patterns"];
const VECTOR_OPTIONS_WOFF2_SOURCE: [&str; 3] = ["auto", "cff", "ttf"];
const VECTOR_OPTIONS_NOTDEF: [&str; 2] = ["preserve", "empty"];
/// SVG and OpenType outline output options.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct VectorOptions {
    /// Outline shape for SVG/TTF/OTF/WOFF/WOFF2/TTC conversion.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::builder::TypedValueParser::map(clap::builder::PossibleValuesParser::new(VECTOR_OPTIONS_SHAPE), crate::options::parse_shape)))]
    pub shape: Option<fontpixel::outline::Shape>,
    /// Dot diameter or separate-square side relative to one pixel.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub dotsize: Option<f64>,
    /// Native dot reuse; default patterns. Literal favors smaller webfonts.
    #[cfg_attr(feature = "clap", arg(long,value_parser = VECTOR_OPTIONS_DOT_REUSE))]
    pub dot_reuse: Option<String>,
    /// Design units per source pixel (default 100).
    #[cfg_attr(feature = "clap", arg(long))]
    pub ppu: Option<u16>,
    /// Explicit units per em for outline output.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub upm: Option<u16>,
    /// Embed a matching EBDT/EBLC bitmap strike in outline output.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub embed_bitmaps: bool,
    /// Explicit grayscale-to-monochrome threshold.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub gray_threshold: Option<u8>,
    /// Outline source in webfonts/TTC (auto selects CFF squares, TrueType dots).
    #[cfg_attr(feature = "clap", arg(long,value_parser = VECTOR_OPTIONS_WOFF2_SOURCE))]
    pub woff2_source: Option<String>,
    /// WOFF2 Brotli quality, 0 through 11 (default 11).
    #[cfg_attr(feature = "clap", arg(long,value_parser=clap::value_parser!(u32).range(0..=11), help = None::<&str>, long_help = None::<&str>))]
    pub woff2_quality: Option<u32>,
    /// Intra-font Brotli workers (1..16), independent of batch task workers.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u32).range(1..=16)))]
    pub woff2_threads: Option<u32>,
    /// Write a version 2 TTC/OTC header.
    #[cfg_attr(feature = "clap", arg(long, help = None::<&str>, long_help = None::<&str>))]
    pub collection_v2: bool,
    /// Glyph 0 of TTF/OTF/WOFF/WOFF2/TTC output: preserve (the source .notdef,
    /// otherwise an empty glyph) or empty (empty outline with exact design-unit
    /// metrics; requires --notdef-advance; replacing a source .notdef is loss).
    #[cfg_attr(feature = "clap", arg(long, value_parser = VECTOR_OPTIONS_NOTDEF))]
    pub notdef: Option<String>,
    /// Empty .notdef advance width in design units.
    #[cfg_attr(feature = "clap", arg(long))]
    pub notdef_advance: Option<u16>,
    /// Empty .notdef left side bearing in design units (default 0).
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub notdef_lsb: Option<i16>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(VectorOptions);
impl VectorOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.dot_reuse.as_deref() {
            check_choice("dot_reuse", value, &VECTOR_OPTIONS_DOT_REUSE)?;
        }
        if let Some(value) = self.woff2_source.as_deref() {
            check_choice("woff2_source", value, &VECTOR_OPTIONS_WOFF2_SOURCE)?;
        }
        if let Some(value) = self.woff2_quality {
            check_range("woff2_quality", i128::from(value), 0, 11)?;
        }
        if let Some(value) = self.woff2_threads {
            check_range("woff2_threads", i128::from(value), 1, 16)?;
        }
        if let Some(value) = self.notdef.as_deref() {
            check_choice("notdef", value, &VECTOR_OPTIONS_NOTDEF)?;
        }
        Ok(())
    }
}
impl VectorOptions {
    /// Explicit glyph-0 policy; validated before any input is read.
    #[cfg(feature = "opentype")]
    pub(crate) fn notdef(&self) -> fontpixel::Result<fontpixel::variants::NotdefPolicy> {
        use fontpixel::variants::NotdefPolicy;
        match (self.notdef.as_deref(), self.notdef_advance, self.notdef_lsb) {
            (Some("empty"), Some(advance_width), lsb) => Ok(NotdefPolicy::Empty {
                advance_width,
                left_side_bearing: lsb.unwrap_or(0),
            }),
            (Some("empty"), None, _) => Err(Error::InvalidInput(
                "--notdef empty requires --notdef-advance".into(),
            )),
            (_, None, None) => Ok(NotdefPolicy::Preserve),
            _ => Err(Error::InvalidInput(
                "--notdef-advance/--notdef-lsb require --notdef empty".into(),
            )),
        }
    }
    pub(crate) fn apply(
        &self,
        id: &str,
        registry: &mut fontpixel::FontRegistry,
    ) -> fontpixel::Result<()> {
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
            || self.collection_v2
            || self.notdef.is_some()
            || self.notdef_advance.is_some()
            || self.notdef_lsb.is_some();
        #[cfg(feature = "svg")]
        if id == "svg" {
            if self.upm.is_some()
                || self.dot_reuse.is_some()
                || self.embed_bitmaps
                || self.woff2_source.is_some()
                || self.woff2_quality.is_some()
                || self.woff2_threads.is_some()
                || self.collection_v2
                || self.notdef.is_some()
                || self.notdef_advance.is_some()
                || self.notdef_lsb.is_some()
            {
                return Err(Error::InvalidInput(
                    "OpenType container options cannot apply to SVG output".into(),
                ));
            }
            let shape = self.shape.unwrap_or(fontpixel::outline::Shape::Square);
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
            o.outline.shape = self.shape.unwrap_or(Shape::Square);
            o.outline.dot_size = self.dotsize.unwrap_or(if o.outline.shape == Shape::Dot {
                0.9
            } else {
                1.
            });
            o.outline.units_per_pixel = self.ppu.unwrap_or(100);
            o.outline.dot_reuse = crate::opentype::dot_reuse(self.dot_reuse.as_deref());
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
            o.outline.notdef = self.notdef()?;
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

/// Map a `shape` choice (one of the shape constants) to the outline shape.
#[cfg(feature = "clap")]
pub(crate) fn parse_shape(value: String) -> fontpixel::outline::Shape {
    match value.as_str() {
        "dot" => fontpixel::outline::Shape::Dot,
        "square-separate" => fontpixel::outline::Shape::SquareSeparate,
        _ => fontpixel::outline::Shape::Square,
    }
}
/// Map a merge `spacing` choice to the merge spacing policy.
#[cfg(feature = "clap")]
pub(crate) fn parse_spacing(value: String) -> fontpixel::merge::MergeSpacing {
    match value.as_str() {
        "mono" => fontpixel::merge::MergeSpacing::Mono,
        "prop" => fontpixel::merge::MergeSpacing::Prop,
        _ => fontpixel::merge::MergeSpacing::Auto,
    }
}
/// Map a merge `geometric_width` choice to the geometric width policy.
#[cfg(feature = "clap")]
pub(crate) fn parse_geometric_width(value: String) -> fontpixel::merge::GeometricWidth {
    match value.as_str() {
        "full" => fontpixel::merge::GeometricWidth::Full,
        _ => fontpixel::merge::GeometricWidth::Half,
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
impl RasterOptions {
    pub(crate) fn c_geometry(
        &self,
    ) -> fontpixel::Result<Option<fontpixel::formats::c::ArrayOptions>> {
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
const COUTPUT_OPTIONS_C_FAMILY: [&str; 7] =
    ["generic", "gfx", "u8g2", "lvgl", "netbsd", "u8glib", "u8x8"];
const COUTPUT_OPTIONS_C_CONTAINER: [&str; 2] = ["source", "lvgl-binary"];
const COUTPUT_OPTIONS_U8GLIB_FORMAT: [&str; 3] = ["full", "compact", "gray2"];
const COUTPUT_OPTIONS_LVGL_BPP: [&str; 4] = ["1", "2", "4", "8"];
const COUTPUT_OPTIONS_LVGL_COMPRESSION: [&str; 3] = ["plain", "rle", "rle-xor"];
/// C header and LVGL output options (`--to c` only).
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct COutputOptions {
    /// Output C dialect (default generic).
    #[cfg_attr(feature = "clap", arg(long, value_parser = COUTPUT_OPTIONS_C_FAMILY))]
    pub c_family: Option<String>,
    /// C source (default) or native LVGL binary output.
    #[cfg_attr(feature = "clap", arg(long, value_parser = COUTPUT_OPTIONS_C_CONTAINER))]
    pub c_container: Option<String>,
    /// U8glib native font record format (default full).
    #[cfg_attr(feature = "clap", arg(long, value_parser = COUTPUT_OPTIONS_U8GLIB_FORMAT))]
    pub u8glib_format: Option<String>,
    /// Write the pre-2019 two-byte U8x8 header.
    #[cfg_attr(feature = "clap", arg(long))]
    pub u8x8_legacy: bool,
    /// Output C symbol (default font).
    #[cfg_attr(feature = "clap", arg(long))]
    pub c_name: Option<String>,
    /// Inclusive C output range, e.g. U+0020-U+007E.
    #[cfg_attr(feature = "clap", arg(long))]
    pub c_range: Option<String>,
    /// LVGL coverage bit depth (default 1).
    #[cfg_attr(feature = "clap", arg(long, value_parser = COUTPUT_OPTIONS_LVGL_BPP))]
    pub lvgl_bpp: Option<String>,
    /// LVGL bitmap encoding (default plain).
    #[cfg_attr(feature = "clap", arg(long, value_parser = COUTPUT_OPTIONS_LVGL_COMPRESSION))]
    pub lvgl_compression: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(COutputOptions);
impl COutputOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.c_family.as_deref() {
            check_choice("c_family", value, &COUTPUT_OPTIONS_C_FAMILY)?;
        }
        if let Some(value) = self.c_container.as_deref() {
            check_choice("c_container", value, &COUTPUT_OPTIONS_C_CONTAINER)?;
        }
        if let Some(value) = self.u8glib_format.as_deref() {
            check_choice("u8glib_format", value, &COUTPUT_OPTIONS_U8GLIB_FORMAT)?;
        }
        if let Some(value) = self.lvgl_bpp.as_deref() {
            check_choice("lvgl_bpp", value, &COUTPUT_OPTIONS_LVGL_BPP)?;
        }
        if let Some(value) = self.lvgl_compression.as_deref() {
            check_choice("lvgl_compression", value, &COUTPUT_OPTIONS_LVGL_COMPRESSION)?;
        }
        Ok(())
    }
}
impl COutputOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        raster: &RasterOptions,
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
