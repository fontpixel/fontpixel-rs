//! Explicit options for embedded bitmaps, Korean components, IIgs, terminal and color fonts.
mod device;
mod embedded_metadata;
mod historical;
mod native_console;
mod portable;
mod system;
mod workstation;
use clap::Args;
use fontpixel::{Error, FontRegistry, ResourceLimits, Result};

#[derive(Args, Default)]
#[group(skip)]
pub(crate) struct ReadArguments {
    #[command(flatten)]
    device: device::ReadArguments,
    #[command(flatten)]
    portable: portable::ReadArguments,
    #[command(flatten)]
    historical: historical::ReadArguments,
    #[command(flatten)]
    system: system::ReadArguments,
    #[command(flatten)]
    workstation: workstation::ReadArguments,
    #[command(flatten)]
    native_console: native_console::ReadArguments,
    /// Override the inferred first legacy code in a Dr. Halo font.
    #[arg(long)]
    drhalo_read_first: Option<u32>,
    /// Korean 8×4×4 input: compose Hangul syllables or retain component slots.
    #[arg(long, value_parser = ["syllables", "components"])]
    hanfont_read_mode: Option<String>,
    /// Explicit charset label for Apple IIgs legacy character codes.
    #[arg(long)]
    iigs_read_charset: Option<String>,
    /// DRCS baseline down from cell top (default cell height).
    #[arg(long, allow_hyphen_values = true)]
    drcs_baseline: Option<i64>,
    /// DEC device matrix rules for both reading and writing DRCS.
    #[arg(long, value_parser = ["vt320", "vt510"])]
    drcs_terminal: Option<String>,
    /// Explicit Berkeley vfont input integer byte order.
    #[arg(long, value_parser = ["little", "big"])]
    vfont_read_byte_order: Option<String>,
    /// Require a Palm wrapper instead of recognizing its structure.
    #[arg(long, value_parser = ["bare", "pdb", "prc"])]
    palm_read_container: Option<String>,
    /// Explicit handling of fractional Palm density coordinates (default exact).
    #[arg(long, value_parser = ["exact", "floor"])]
    palm_rounding: Option<String>,
    /// Interpret GDR source codes as legacy positions instead of Unicode.
    #[arg(long)]
    gdr_read_legacy: bool,
}

impl ReadArguments {
    #[cfg(feature = "drcs")]
    fn terminal(&self) -> fontpixel::formats::drcs::Terminal {
        if self.drcs_terminal.as_deref() == Some("vt510") {
            fontpixel::formats::drcs::Terminal::Vt510
        } else {
            fontpixel::formats::drcs::Terminal::Vt320
        }
    }
    pub fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        self.portable.apply(registry)?;
        self.historical.apply(registry)?;
        self.device.apply(registry)?;
        self.workstation.apply(registry)?;
        self.native_console.apply(registry)?;
        self.system.apply(registry)?;
        feature(
            self.drhalo_read_first.is_some(),
            cfg!(feature = "drhalo"),
            "drhalo",
        )?;
        #[cfg(feature = "drhalo")]
        registry.set_drhalo_read_options(fontpixel::formats::drhalo::DrhaloReadOptions {
            first_codepoint: self.drhalo_read_first,
        });
        feature(
            self.hanfont_read_mode.is_some(),
            cfg!(feature = "hanfont"),
            "hanfont",
        )?;
        feature(
            self.iigs_read_charset.is_some(),
            cfg!(feature = "iigs"),
            "iigs",
        )?;
        feature(
            self.drcs_baseline.is_some() || self.drcs_terminal.is_some(),
            cfg!(feature = "drcs"),
            "drcs",
        )?;
        feature(
            self.vfont_read_byte_order.is_some(),
            cfg!(feature = "vfont"),
            "vfont",
        )?;
        feature(
            self.palm_read_container.is_some() || self.palm_rounding.is_some(),
            cfg!(feature = "palm"),
            "palm",
        )?;
        feature(self.gdr_read_legacy, cfg!(feature = "gdr"), "gdr")?;
        #[cfg(feature = "vfont")]
        registry.set_vfont_read_options(fontpixel::formats::vfont::VfontReadOptions {
            byte_order: self.vfont_read_byte_order.as_deref().map(order),
        });
        #[cfg(feature = "palm")]
        registry.set_palm_read_options(fontpixel::formats::palm::PalmReadOptions {
            container: self.palm_read_container.as_deref().map(palm_container),
            rounding: if self.palm_rounding.as_deref() == Some("floor") {
                fontpixel::formats::palm::PalmRounding::Floor
            } else {
                fontpixel::formats::palm::PalmRounding::Exact
            },
        });
        #[cfg(feature = "gdr")]
        registry.set_gdr_read_options(fontpixel::formats::gdr::GdrReadOptions {
            unicode: !self.gdr_read_legacy,
        });
        #[cfg(feature = "hanfont")]
        {
            use fontpixel::formats::hanfont::{HanfontReadOptions, ReadMode};
            registry.set_hanfont_read_options(HanfontReadOptions {
                mode: if self.hanfont_read_mode.as_deref() == Some("components") {
                    ReadMode::Components
                } else {
                    ReadMode::Syllables
                },
                ..Default::default()
            });
        }
        #[cfg(feature = "iigs")]
        registry.set_iigs_read_options(fontpixel::formats::iigs::IigsReadOptions {
            charset: self.iigs_read_charset.clone(),
        });
        #[cfg(feature = "drcs")]
        registry.set_drcs_read_options(fontpixel::formats::drcs::DrcsReadOptions {
            baseline: self.drcs_baseline,
            terminal: self.terminal(),
        });
        let _ = registry;
        Ok(())
    }
}

#[derive(Args, Default)]
#[group(skip)]
pub(crate) struct WriteArguments {
    #[command(flatten)]
    device: device::WriteArguments,
    #[command(flatten)]
    portable: portable::WriteArguments,
    #[command(flatten)]
    historical: historical::WriteArguments,
    #[command(flatten)]
    system: system::WriteArguments,
    #[command(flatten)]
    workstation: workstation::WriteArguments,
    #[command(flatten)]
    native_console: native_console::WriteArguments,
    /// Explicitly use Unicode numbers as legacy codes in small native formats.
    #[arg(long)]
    native_unicode_as_legacy: bool,
    /// Fixed cell width for supported native console and small font formats.
    #[arg(long, requires = "native_cell_height", value_parser = clap::value_parser!(u32).range(1..))]
    native_cell_width: Option<u32>,
    /// Fixed cell height, paired with --native-cell-width.
    #[arg(long, requires = "native_cell_width", value_parser = clap::value_parser!(u32).range(1..))]
    native_cell_height: Option<u32>,
    /// First legacy position in Dr. Halo output (zero or 32).
    #[arg(long)]
    drhalo_first: Option<u32>,
    /// Apple IIgs header version (default preserves the input version).
    #[arg(long, value_parser = ["auto", "1.1", "1.5"])]
    iigs_version: Option<String>,
    /// IIgs output envelope (default bare).
    #[arg(long, value_parser = ["bare", "binscii"])]
    iigs_container: Option<String>,
    /// Berkeley vfont output integer byte order (default little).
    #[arg(long, value_parser = ["little", "big"])]
    vfont_byte_order: Option<String>,
    /// Explicitly project Unicode numbers to vfont legacy byte positions.
    #[arg(long)]
    vfont_unicode_as_legacy: bool,
    /// FZX line height (otherwise inferred from font metrics).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..))]
    fzx_height: Option<u8>,
    /// FZX common tracking in pixels.
    #[arg(long)]
    fzx_tracking: Option<u8>,
    /// Explicitly project Unicode numbers to FZX legacy byte positions.
    #[arg(long)]
    fzx_unicode_as_legacy: bool,
    /// Common FIGlet character-art cell height.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    figlet_height: Option<u32>,
    /// FIGlet baseline measured in rows from the cell top.
    #[arg(long)]
    figlet_baseline: Option<u32>,
    /// Explicitly interpret legacy code numbers as FIGlet Unicode values.
    #[arg(long)]
    figlet_legacy_as_unicode: bool,
    /// Palm output wrapper (default prc).
    #[arg(long, value_parser = ["bare", "pdb", "prc"])]
    palm_container: Option<String>,
    /// Palm bitmap resource version (default 1).
    #[arg(long, value_parser = ["1", "2", "afnx", "fontsubst", "gray1", "gray2"])]
    palm_version: Option<String>,
    /// Comma-separated Palm densities, one per input strike.
    #[arg(long, value_delimiter = ',')]
    palm_densities: Vec<u16>,
    /// Explicitly project Unicode numbers to Palm legacy character codes.
    #[arg(long)]
    palm_unicode_as_legacy: bool,
    /// GDR strike descriptor version (default 41).
    #[arg(long, value_parser = ["39", "40", "41", "42"])]
    gdr_version: Option<String>,
    /// GDR collection UID in decimal or 0x hexadecimal notation.
    #[arg(long, value_parser = parse_uid)]
    gdr_collection_uid: Option<u32>,
    /// Use legacy character codes for GDR output instead of Unicode.
    #[arg(long)]
    gdr_write_legacy: bool,
    /// Color bitmap table family; both encode black pixels with coverage alpha.
    #[arg(long, value_parser = ["cbdt", "sbix"])]
    color_flavor: Option<String>,
    /// Explicit OTB/bdat design units per em; absent preserves automatic scaling.
    #[arg(long, value_parser = clap::value_parser!(u16).range(16..=16384))]
    otb_upm: Option<u16>,
    /// OTB/bdat metric source: first, largest y-ppem, or a zero-based strike index.
    #[arg(long, value_parser = embedded_reference)]
    otb_reference: Option<String>,
    /// OTB/bdat pixel-to-design rounding; absent preserves native half-away rounding.
    #[arg(long, value_parser = ["half-away", "ties-even"])]
    otb_rounding: Option<String>,
    #[command(flatten)]
    otb_metadata: embedded_metadata::MetadataArguments,
    /// DRCS output cell width; requires --drcs-cell-height.
    #[arg(long, requires = "drcs_cell_height", value_parser = clap::value_parser!(u32).range(1..))]
    drcs_cell_width: Option<u32>,
    /// DRCS output cell height; requires --drcs-cell-width.
    #[arg(long, requires = "drcs_cell_width", value_parser = clap::value_parser!(u32).range(1..))]
    drcs_cell_height: Option<u32>,
    /// DRCS cell left edge relative to the glyph origin (default zero).
    #[arg(long, requires = "drcs_cell_width", allow_hyphen_values = true)]
    drcs_cell_x: Option<i64>,
    /// DRCS cell bottom edge relative to the baseline (default zero).
    #[arg(long, requires = "drcs_cell_width", allow_hyphen_values = true)]
    drcs_cell_y: Option<i64>,
    /// DRCS character set size (default 94).
    #[arg(long, value_parser = ["94", "96"])]
    drcs_charset: Option<String>,
    /// DEC character set designator (default a space followed by @).
    #[arg(long)]
    drcs_designator: Option<String>,
    /// Use 8-bit DCS/ST control bytes instead of ESC P / ESC backslash.
    #[arg(long)]
    drcs_eight_bit_controls: bool,
}

impl WriteArguments {
    /// Caller-retained bytes of explicit OTB/bdat metadata arguments and the
    /// name table built from them; reserve once before reading inputs. Zero
    /// when no metadata flag is present.
    #[cfg(any(feature = "otb", feature = "bdat"))]
    pub fn reserved_bytes(&self, limit: usize) -> Result<usize> {
        if self.otb_metadata.requested() {
            self.otb_metadata.reserved_bytes(limit)
        } else {
            Ok(0)
        }
    }
    pub fn apply(
        &self,
        id: &str,
        input: &ReadArguments,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let embedded_requested = self.otb_upm.is_some()
            || self.otb_reference.is_some()
            || self.otb_rounding.is_some()
            || self.otb_metadata.requested();
        if embedded_requested {
            if !matches!(id, "otb" | "bdat") {
                return Err(Error::InvalidInput(
                    "--otb-* options (upm, reference, rounding and metadata) require otb or bdat output"
                        .into(),
                ));
            }
            feature(
                true,
                [
                    ("otb", cfg!(feature = "otb")),
                    ("bdat", cfg!(feature = "bdat")),
                ]
                .iter()
                .any(|&(format, enabled)| format == id && enabled),
                id,
            )?;
        }
        #[cfg(any(feature = "otb", feature = "bdat"))]
        if embedded_requested {
            use fontpixel::formats::embedded::{
                EmbeddedReferenceStrike, EmbeddedRounding, EmbeddedWriteOptions,
            };
            let reference_strike = match self.otb_reference.as_deref() {
                None | Some("first") => EmbeddedReferenceStrike::First,
                Some("largest") => EmbeddedReferenceStrike::LargestPpem,
                Some(index) => EmbeddedReferenceStrike::Index(index.parse().map_err(|_| {
                    Error::InvalidInput("invalid zero-based OTB/bdat strike index".into())
                })?),
            };
            let mut options = EmbeddedWriteOptions {
                units_per_em: self.otb_upm,
                reference_strike,
                rounding: if self.otb_rounding.as_deref() == Some("ties-even") {
                    EmbeddedRounding::NearestTiesEven
                } else {
                    EmbeddedRounding::NearestTiesAwayFromZero
                },
                metadata: self.otb_metadata.options(limits)?,
                ..Default::default()
            };
            options.limits.sfnt.resources = limits.clone();
            // The registry keeps these CLI-owned arguments and the built table
            // alive while the writer runs; the writer never receives that credit.
            let reserved = self.reserved_bytes(limits.max_total_bytes)?;
            options.limits.sfnt.resources.max_total_bytes = limits
                .max_total_bytes
                .checked_sub(reserved)
                .ok_or(Error::ResourceLimit {
                    resource: "OTB/bdat metadata arguments",
                    limit: limits.max_total_bytes,
                })?;
            registry.set_embedded_write_options(options);
        }
        self.portable.apply(
            id,
            limits,
            threshold,
            self.native_unicode_as_legacy,
            registry,
        )?;
        self.native_console.apply(
            id,
            limits,
            threshold,
            self.native_unicode_as_legacy,
            self.native_cell_width
                .zip(self.native_cell_height)
                .map(|(w, h)| fontpixel::bitmap::Bounds::new(w, h, 0, 0)),
            registry,
        )?;
        self.workstation.apply(
            id,
            limits,
            threshold,
            self.native_unicode_as_legacy,
            registry,
        )?;
        self.system.apply(
            id,
            limits,
            threshold,
            self.native_unicode_as_legacy,
            &input.system,
            registry,
        )?;
        self.historical.apply(
            id,
            limits,
            threshold,
            self.native_unicode_as_legacy,
            registry,
        )?;
        self.device.apply(
            id,
            limits,
            threshold,
            self.native_unicode_as_legacy,
            registry,
        )?;
        let native_requested =
            self.native_cell_width.is_some() || self.native_cell_height.is_some();
        if native_requested
            && ![
                "dosstart",
                "edwin",
                "mkwinfont",
                "psf2txt",
                "pcr",
                "plus3dos",
                "drhalo",
                "writeon",
                "bbc",
                "wyse",
                "grasp",
                "hrcg",
                "xbin",
                "comfont",
                "consoleet",
                "zapfont",
            ]
            .contains(&id)
        {
            return Err(Error::InvalidInput(
                "native cell/code options require a supported small native output format".into(),
            ));
        }
        if self.native_unicode_as_legacy
            && ![
                "dosstart",
                "edwin",
                "mkwinfont",
                "psf2txt",
                "pcr",
                "plus3dos",
                "drhalo",
                "writeon",
                "bbc",
                "wyse",
                "grasp",
                "hrcg",
                "xbin",
                "comfont",
                "consoleet",
                "zapfont",
                "daisy",
                "polyprint",
                "printshop",
                "tasprint",
                "pxl",
                "signum",
                "cybiko",
                "sbf",
                "pilfont",
                "geos",
                "catalyst",
                "chiwriter",
                "dashen",
                "hppcl",
                "riscos",
                "xerox",
                "gpi",
                "pcgeos",
                "pfr",
                "symbos",
                "oberon",
                "wsfont",
            ]
            .contains(&id)
        {
            return Err(Error::InvalidInput(
                "native code projection is unavailable for this output format".into(),
            ));
        }
        target(self.drhalo_first.is_some(), id, "drhalo")?;
        feature(id == "dosstart", cfg!(feature = "dosstart"), "dosstart")?;
        #[cfg(feature = "dosstart")]
        if id == "dosstart" {
            registry.set_dosstart_write_options(
                fontpixel::formats::dosstart::DosstartWriteOptions {
                    gray_threshold: threshold,
                    unicode_as_legacy: self.native_unicode_as_legacy,
                    cell_size: self.native_cell_width.zip(self.native_cell_height),
                    first_codepoint: self.drhalo_first,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        feature(id == "edwin", cfg!(feature = "edwin"), "edwin")?;
        #[cfg(feature = "edwin")]
        if id == "edwin" {
            registry.set_edwin_write_options(fontpixel::formats::edwin::EdwinWriteOptions {
                gray_threshold: threshold,
                unicode_as_legacy: self.native_unicode_as_legacy,
                cell_size: self.native_cell_width.zip(self.native_cell_height),
                first_codepoint: self.drhalo_first,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        feature(id == "mkwinfont", cfg!(feature = "mkwinfont"), "mkwinfont")?;
        #[cfg(feature = "mkwinfont")]
        if id == "mkwinfont" {
            registry.set_mkwinfont_write_options(
                fontpixel::formats::mkwinfont::MkwinfontWriteOptions {
                    gray_threshold: threshold,
                    unicode_as_legacy: self.native_unicode_as_legacy,
                    cell_size: self.native_cell_width.zip(self.native_cell_height),
                    first_codepoint: self.drhalo_first,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        feature(id == "psf2txt", cfg!(feature = "psf2txt"), "psf2txt")?;
        #[cfg(feature = "psf2txt")]
        if id == "psf2txt" {
            registry.set_psf2txt_write_options(fontpixel::formats::psf2txt::Psf2txtWriteOptions {
                gray_threshold: threshold,
                unicode_as_legacy: self.native_unicode_as_legacy,
                cell_size: self.native_cell_width.zip(self.native_cell_height),
                first_codepoint: self.drhalo_first,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        feature(id == "pcr", cfg!(feature = "pcr"), "pcr")?;
        #[cfg(feature = "pcr")]
        if id == "pcr" {
            registry.set_pcr_write_options(fontpixel::formats::pcr::PcrWriteOptions {
                gray_threshold: threshold,
                unicode_as_legacy: self.native_unicode_as_legacy,
                cell_size: self.native_cell_width.zip(self.native_cell_height),
                first_codepoint: self.drhalo_first,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        feature(id == "plus3dos", cfg!(feature = "plus3dos"), "plus3dos")?;
        #[cfg(feature = "plus3dos")]
        if id == "plus3dos" {
            registry.set_plus3dos_write_options(
                fontpixel::formats::plus3dos::Plus3dosWriteOptions {
                    gray_threshold: threshold,
                    unicode_as_legacy: self.native_unicode_as_legacy,
                    cell_size: self.native_cell_width.zip(self.native_cell_height),
                    first_codepoint: self.drhalo_first,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        feature(id == "drhalo", cfg!(feature = "drhalo"), "drhalo")?;
        #[cfg(feature = "drhalo")]
        if id == "drhalo" {
            registry.set_drhalo_write_options(fontpixel::formats::drhalo::DrhaloWriteOptions {
                gray_threshold: threshold,
                unicode_as_legacy: self.native_unicode_as_legacy,
                cell_size: self.native_cell_width.zip(self.native_cell_height),
                first_codepoint: self.drhalo_first,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        feature(id == "writeon", cfg!(feature = "writeon"), "writeon")?;
        #[cfg(feature = "writeon")]
        if id == "writeon" {
            registry.set_writeon_write_options(fontpixel::formats::writeon::WriteonWriteOptions {
                gray_threshold: threshold,
                unicode_as_legacy: self.native_unicode_as_legacy,
                cell_size: self.native_cell_width.zip(self.native_cell_height),
                first_codepoint: self.drhalo_first,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.iigs_version.is_some() || self.iigs_container.is_some(),
            id,
            "iigs",
        )?;
        target(
            self.vfont_byte_order.is_some() || self.vfont_unicode_as_legacy,
            id,
            "vfont",
        )?;
        target(
            self.fzx_height.is_some() || self.fzx_tracking.is_some() || self.fzx_unicode_as_legacy,
            id,
            "fzx",
        )?;
        target(
            self.figlet_height.is_some()
                || self.figlet_baseline.is_some()
                || self.figlet_legacy_as_unicode,
            id,
            "figlet",
        )?;
        target(
            self.palm_container.is_some()
                || self.palm_version.is_some()
                || !self.palm_densities.is_empty()
                || self.palm_unicode_as_legacy,
            id,
            "palm",
        )?;
        target(
            self.gdr_version.is_some()
                || self.gdr_collection_uid.is_some()
                || self.gdr_write_legacy,
            id,
            "gdr",
        )?;
        feature(id == "vfont", cfg!(feature = "vfont"), "vfont")?;
        feature(id == "fzx", cfg!(feature = "fzx"), "fzx")?;
        feature(id == "figlet", cfg!(feature = "figlet"), "figlet")?;
        feature(id == "palm", cfg!(feature = "palm"), "palm")?;
        feature(id == "gdr", cfg!(feature = "gdr"), "gdr")?;
        target(self.color_flavor.is_some(), id, "color")?;
        let drcs_requested = self.drcs_cell_width.is_some()
            || self.drcs_cell_height.is_some()
            || self.drcs_charset.is_some()
            || self.drcs_designator.is_some()
            || self.drcs_eight_bit_controls;
        target(drcs_requested, id, "drcs")?;
        feature(id == "hanfont", cfg!(feature = "hanfont"), "hanfont")?;
        feature(id == "iigs", cfg!(feature = "iigs"), "iigs")?;
        feature(id == "color", cfg!(feature = "color"), "color")?;
        feature(id == "drcs", cfg!(feature = "drcs"), "drcs")?;
        #[cfg(feature = "hanfont")]
        if id == "hanfont" {
            registry.set_hanfont_write_options(fontpixel::formats::hanfont::HanfontWriteOptions {
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "iigs")]
        if id == "iigs" {
            use fontpixel::formats::iigs::{IigsWriteOptions, Version};
            registry.set_iigs_write_options(IigsWriteOptions {
                container: if self.iigs_container.as_deref() == Some("binscii") {
                    fontpixel::formats::iigs::Container::BinScii
                } else {
                    fontpixel::formats::iigs::Container::Bare
                },
                version: match self.iigs_version.as_deref() {
                    Some("1.1") => Version::V1_1,
                    Some("1.5") => Version::V1_5,
                    _ => Version::Auto,
                },
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "color")]
        if id == "color" {
            use fontpixel::formats::embedded::{ColorFlavor, ColorWriteOptions};
            let mut options = ColorWriteOptions {
                flavor: if self.color_flavor.as_deref() == Some("sbix") {
                    ColorFlavor::Sbix
                } else {
                    ColorFlavor::Cbdt
                },
                ..Default::default()
            };
            options.limits.sfnt.resources = limits.clone();
            registry.set_color_write_options(options);
        }
        #[cfg(feature = "drcs")]
        if id == "drcs" {
            use fontpixel::formats::drcs::{DrcsCharset, DrcsWriteOptions};
            registry.set_drcs_write_options(DrcsWriteOptions {
                terminal: input.terminal(),
                cell: self
                    .drcs_cell_width
                    .zip(self.drcs_cell_height)
                    .map(|(w, h)| {
                        fontpixel::bitmap::Bounds::new(
                            w,
                            h,
                            self.drcs_cell_x.unwrap_or(0),
                            self.drcs_cell_y.unwrap_or(0),
                        )
                    }),
                charset: if self.drcs_charset.as_deref() == Some("96") {
                    DrcsCharset::NinetySix
                } else {
                    DrcsCharset::NinetyFour
                },
                designator: self.drcs_designator.clone().unwrap_or_else(|| " @".into()),
                eight_bit_controls: self.drcs_eight_bit_controls,
                gray_threshold: threshold,
                limits: limits.clone(),
            });
        }
        #[cfg(feature = "vfont")]
        if id == "vfont" {
            registry.set_vfont_write_options(fontpixel::formats::vfont::VfontWriteOptions {
                byte_order: order(self.vfont_byte_order.as_deref().unwrap_or("little")),
                unicode_as_legacy: self.vfont_unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "fzx")]
        if id == "fzx" {
            registry.set_fzx_write_options(fontpixel::formats::fzx::FzxWriteOptions {
                height: self.fzx_height,
                tracking: self.fzx_tracking,
                unicode_as_legacy: self.fzx_unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "figlet")]
        if id == "figlet" {
            registry.set_figlet_write_options(fontpixel::formats::figlet::FigletWriteOptions {
                height: self.figlet_height,
                baseline: self.figlet_baseline,
                legacy_as_unicode: self.figlet_legacy_as_unicode,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "palm")]
        if id == "palm" {
            registry.set_palm_write_options(fontpixel::formats::palm::PalmWriteOptions {
                container: palm_container(self.palm_container.as_deref().unwrap_or("prc")),
                version: match self.palm_version.as_deref() {
                    Some("2") => fontpixel::formats::palm::PalmVersion::V2,
                    Some("afnx") => fontpixel::formats::palm::PalmVersion::Afnx,
                    Some("fontsubst") => fontpixel::formats::palm::PalmVersion::FontSubst,
                    Some("gray1") => fontpixel::formats::palm::PalmVersion::Gray1,
                    Some("gray2") => fontpixel::formats::palm::PalmVersion::Gray2,
                    _ => fontpixel::formats::palm::PalmVersion::V1,
                },
                densities: self.palm_densities.clone(),
                unicode_as_legacy: self.palm_unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "gdr")]
        if id == "gdr" {
            registry.set_gdr_write_options(fontpixel::formats::gdr::GdrWriteOptions {
                version: match self.gdr_version.as_deref() {
                    Some("39") => fontpixel::formats::gdr::GdrVersion::V39,
                    Some("40") => fontpixel::formats::gdr::GdrVersion::V40,
                    Some("42") => fontpixel::formats::gdr::GdrVersion::V42,
                    _ => fontpixel::formats::gdr::GdrVersion::V41,
                },
                collection_uid: self.gdr_collection_uid,
                unicode: !self.gdr_write_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (input, limits, threshold, registry);
        Ok(())
    }
}

fn feature(requested: bool, enabled: bool, id: &str) -> Result<()> {
    if requested && !enabled {
        return Err(Error::FeatureDisabled { format: id.into() });
    }
    Ok(())
}

fn target(requested: bool, id: &str, expected: &str) -> Result<()> {
    if requested && id != expected {
        return Err(Error::InvalidInput(format!(
            "{expected} output options require --to {expected}"
        )));
    }
    Ok(())
}

#[cfg(feature = "vfont")]
fn order(value: &str) -> fontpixel::bitmap::ByteOrder {
    if value == "big" {
        fontpixel::bitmap::ByteOrder::Big
    } else {
        fontpixel::bitmap::ByteOrder::Little
    }
}
#[cfg(feature = "palm")]
fn palm_container(value: &str) -> fontpixel::formats::palm::PalmContainer {
    use fontpixel::formats::palm::PalmContainer;
    match value {
        "bare" => PalmContainer::Bare,
        "pdb" => PalmContainer::Pdb,
        _ => PalmContainer::Prc,
    }
}
fn parse_uid(value: &str) -> std::result::Result<u32, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16)
    } else {
        value.parse()
    }
    .map_err(|_| "GDR collection UID must fit an unsigned 32-bit number".into())
}

// Keep parsing available in feature-disabled builds so valid explicit controls
// receive the normal FeatureDisabled error rather than an unknown-option error.
fn embedded_reference(value: &str) -> std::result::Result<String, String> {
    if matches!(value, "first" | "largest")
        || (!value.is_empty()
            && value.bytes().all(|byte| byte.is_ascii_digit())
            && value.parse::<usize>().is_ok())
    {
        Ok(value.to_owned())
    } else {
        Err("expected first, largest, or a zero-based strike index".into())
    }
}
