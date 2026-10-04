//! Headerless layouts and DOS console output choices.
use super::{require_feature, require_target};
use clap::Args;
#[cfg(any(feature = "raw", feature = "cpi"))]
use fontpixel::Error;
use fontpixel::{FontRegistry, ResourceLimits, Result};

#[derive(Args, Default)]
#[group(skip)]
pub(super) struct ReadArguments {
    /// Explicit raw layout preset (VGA, C64, Spectrum, CH8, MSX or Harlekin).
    #[arg(long)]
    raw_preset: Option<String>,
    /// Raw glyph width; overrides a selected preset.
    #[arg(long)]
    raw_width: Option<u32>,
    /// Raw glyph height; overrides a selected preset.
    #[arg(long)]
    raw_height: Option<u32>,
    /// Exact raw glyph count; otherwise consume complete records.
    #[arg(long)]
    raw_count: Option<usize>,
    /// First raw slot code, e.g. U+0020 or 32.
    #[arg(long)]
    raw_first: Option<String>,
    /// Bit order within each raw byte (default msb).
    #[arg(long, value_parser = ["msb", "lsb"])]
    raw_bit_order: Option<String>,
    /// Raw row order (default top-down).
    #[arg(long, value_parser = ["top-down", "bottom-up"])]
    raw_row_order: Option<String>,
    /// Raw alignment: byte-aligned rows or continuous packed bits.
    #[arg(long, value_parser = ["byte-rows", "bit-aligned"])]
    raw_packing: Option<String>,
    /// Row-major or byte-column-major storage within each horizontal strip.
    #[arg(long, value_parser = ["row-major", "column-major"])]
    raw_byte_order: Option<String>,
    /// Glyph cells placed side by side in each raw horizontal strip (default 1).
    #[arg(long)]
    raw_strike_count: Option<usize>,
    /// Extra padding bytes after every raw row.
    #[arg(long)]
    raw_row_padding: Option<usize>,
    /// Prefix bytes before the first raw glyph.
    #[arg(long)]
    raw_offset: Option<usize>,
    /// Fallback two-byte little-endian load address when no matching source prefix exists.
    #[arg(long)]
    raw_default_load_address: Option<String>,
    /// Explicit charset identity of raw slot codes.
    #[arg(long)]
    raw_charset: Option<String>,
    /// DOS CPI input pointers (default linear).
    #[arg(long, value_parser = ["linear", "segmented"])]
    cpi_pointer_mode: Option<String>,
}
impl ReadArguments {
    fn raw_requested(&self) -> bool {
        self.raw_preset.is_some()
            || self.raw_width.is_some()
            || self.raw_height.is_some()
            || self.raw_count.is_some()
            || self.raw_first.is_some()
            || self.raw_bit_order.is_some()
            || self.raw_row_order.is_some()
            || self.raw_packing.is_some()
            || self.raw_byte_order.is_some()
            || self.raw_strike_count.is_some()
            || self.raw_row_padding.is_some()
            || self.raw_offset.is_some()
            || self.raw_default_load_address.is_some()
            || self.raw_charset.is_some()
    }
    #[cfg(feature = "raw")]
    fn layout(&self) -> Result<Option<fontpixel::formats::raw::RawLayout>> {
        use fontpixel::bitmap::{BitOrder, RowOrder};
        use fontpixel::formats::raw::{RawByteOrder, RawLayout, RawPacking, RawPreset};
        if !self.raw_requested() {
            return Ok(None);
        }
        let mut layout = if let Some(preset) = &self.raw_preset {
            preset.parse::<RawPreset>()?.layout()
        } else {
            RawLayout::new(
                self.raw_width.ok_or_else(|| {
                    Error::InvalidInput(
                        "Raw layout needs --raw-width and --raw-height, or --raw-preset".into(),
                    )
                })?,
                self.raw_height.ok_or_else(|| {
                    Error::InvalidInput(
                        "Raw layout needs --raw-width and --raw-height, or --raw-preset".into(),
                    )
                })?,
            )
        };
        if let Some(width) = self.raw_width {
            layout.width = width;
        }
        if let Some(height) = self.raw_height {
            layout.height = height;
        }
        if let Some(count) = self.raw_count {
            layout.count = Some(count);
        }
        if let Some(first) = &self.raw_first {
            layout.first_codepoint = crate::parse_code(first)?;
        }
        if let Some(order) = &self.raw_bit_order {
            layout.bit_order = if order == "lsb" {
                BitOrder::LsbFirst
            } else {
                BitOrder::MsbFirst
            };
        }
        if let Some(order) = &self.raw_row_order {
            layout.row_order = if order == "bottom-up" {
                RowOrder::BottomUp
            } else {
                RowOrder::TopDown
            };
        }
        if let Some(padding) = self.raw_row_padding {
            layout.row_padding = padding;
        }
        if let Some(packing) = &self.raw_packing {
            layout.packing = if packing == "bit-aligned" {
                RawPacking::BitAligned
            } else {
                RawPacking::ByteAlignedRows
            };
        }
        if let Some(order) = &self.raw_byte_order {
            layout.byte_order = if order == "column-major" {
                RawByteOrder::ColumnMajor
            } else {
                RawByteOrder::RowMajor
            };
        }
        if let Some(count) = self.raw_strike_count {
            if count == 0 {
                return Err(Error::InvalidInput(
                    "raw strip glyph count must be positive".into(),
                ));
            }
            layout.strike_count = count;
        }
        if let Some(offset) = self.raw_offset {
            layout.offset = offset;
            if offset != 2 {
                layout.default_load_address = None;
            }
        }
        if let Some(address) = &self.raw_default_load_address {
            layout.default_load_address =
                Some(u16::try_from(crate::parse_code(address)?).map_err(|_| {
                    Error::InvalidInput("Raw default load address exceeds u16".into())
                })?);
        }
        if let Some(charset) = &self.raw_charset {
            layout.charset = Some(charset.clone());
        }
        Ok(Some(layout))
    }
    pub fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        require_feature(self.raw_requested(), cfg!(feature = "raw"), "raw")?;
        require_feature(
            self.cpi_pointer_mode.is_some(),
            cfg!(feature = "cpi"),
            "cpi",
        )?;
        #[cfg(feature = "raw")]
        registry.set_raw_read_options(fontpixel::formats::raw::RawReadOptions {
            layout: self.layout()?,
        });
        #[cfg(feature = "cpi")]
        registry.set_cpi_read_options(fontpixel::formats::cpi::CpiReadOptions {
            pointer_mode: if self.cpi_pointer_mode.as_deref() == Some("segmented") {
                fontpixel::formats::cpi::PointerMode::Segmented
            } else {
                fontpixel::formats::cpi::PointerMode::Linear
            },
        });
        let _ = registry;
        Ok(())
    }
}

#[derive(Args, Default)]
#[group(skip)]
pub(super) struct WriteArguments {
    /// DOS output layout (default CP for .cp, CPX for .cpx, otherwise FONT).
    #[arg(long, value_parser = ["font", "font-nt", "drfont", "cp", "cpx"])]
    cpi_version: Option<String>,
    /// DOS code page for fonts without native code-page metadata.
    #[arg(long)]
    cpi_code_page: Option<u16>,
}
impl WriteArguments {
    pub fn apply(
        &self,
        id: &str,
        destination: &std::path::Path,
        input: &ReadArguments,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let cpi_requested = self.cpi_version.is_some() || self.cpi_code_page.is_some();
        require_target(cpi_requested, id, "cpi")?;
        require_feature(cpi_requested || id == "cpi", cfg!(feature = "cpi"), "cpi")?;
        require_feature(id == "draw", cfg!(feature = "draw"), "draw")?;
        require_feature(id == "raw", cfg!(feature = "raw"), "raw")?;
        #[cfg(feature = "draw")]
        if id == "draw" {
            registry.set_draw_write_options(fontpixel::formats::draw::DrawWriteOptions {
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "raw")]
        if id == "raw" {
            registry.set_raw_write_options(fontpixel::formats::raw::RawWriteOptions {
                layout: input.layout()?,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "cpi")]
        if id == "cpi" {
            use fontpixel::formats::cpi::{CpiWriteOptions, Version};
            let extension = destination.extension().and_then(|v| v.to_str());
            if extension.is_some_and(|v| v.eq_ignore_ascii_case("cpx"))
                && self.cpi_version.as_deref().is_some_and(|v| v != "cpx")
            {
                return Err(Error::InvalidInput(
                    "A .cpx destination requires --cpi-version cpx".into(),
                ));
            }
            registry.set_cpi_write_options(CpiWriteOptions {
                version: match self.cpi_version.as_deref() {
                    Some("font-nt") => Version::FontNt,
                    Some("drfont") => Version::DrFont,
                    Some("cp") => Version::Cp,
                    Some("cpx") => Version::Cpx,
                    None if extension.is_some_and(|v| v.eq_ignore_ascii_case("cpx")) => {
                        Version::Cpx
                    }
                    None if extension.is_some_and(|v| v.eq_ignore_ascii_case("cp")) => Version::Cp,
                    _ => Version::Font,
                },
                code_page: self.cpi_code_page,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (destination, input, limits, threshold, registry);
        Ok(())
    }
}
