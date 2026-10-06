//! Explicit GEM and SNF machine layout controls.
use super::{require_feature, require_target};
use crate::options::{check_choice, check_range};
use fontpixel::{FontRegistry, ResourceLimits, Result};

const READ_OPTIONS_GEM_READ_BYTE_ORDER: [&str; 2] = ["big", "little"];
const READ_OPTIONS_GEM_READ_WORD_ORDER: [&str; 2] = ["big", "little"];
const READ_OPTIONS_SNF_READ_BYTE_ORDER: [&str; 2] = ["big", "little"];
/// Read options: explicit GEM and SNF machine layout controls.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct ReadOptions {
    /// GEM input header integer order; default uses structural detection.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_GEM_READ_BYTE_ORDER))]
    pub gem_read_byte_order: Option<String>,
    /// GEM input bitmap word order; default follows its header flag.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_GEM_READ_WORD_ORDER))]
    pub gem_read_word_order: Option<String>,
    /// SNF input integer order; default uses version stamps.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_SNF_READ_BYTE_ORDER))]
    pub snf_read_byte_order: Option<String>,
    /// SNF input bitmap layout: bits,bytes,row-padding,scan-unit,rows; e.g. msb,big,4,1,top-down.
    #[cfg_attr(feature = "clap", arg(long))]
    pub snf_read_layout: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(ReadOptions);
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.gem_read_byte_order.as_deref() {
            check_choice(
                "gem_read_byte_order",
                value,
                &READ_OPTIONS_GEM_READ_BYTE_ORDER,
            )?;
        }
        if let Some(value) = self.gem_read_word_order.as_deref() {
            check_choice(
                "gem_read_word_order",
                value,
                &READ_OPTIONS_GEM_READ_WORD_ORDER,
            )?;
        }
        if let Some(value) = self.snf_read_byte_order.as_deref() {
            check_choice(
                "snf_read_byte_order",
                value,
                &READ_OPTIONS_SNF_READ_BYTE_ORDER,
            )?;
        }
        Ok(())
    }
}
#[cfg(any(feature = "gem", feature = "snf"))]
fn order(s: &str) -> fontpixel::bitmap::ByteOrder {
    if s == "little" {
        fontpixel::bitmap::ByteOrder::Little
    } else {
        fontpixel::bitmap::ByteOrder::Big
    }
}
#[cfg(feature = "snf")]
fn layout(text: &str) -> Result<fontpixel::bitmap::ByteLayout> {
    use fontpixel::bitmap::{BitOrder, ByteLayout, RowOrder};
    let parts: Vec<_> = text.split(',').collect();
    let invalid = || {
        fontpixel::Error::InvalidInput(
            "SNF layout requires msb|lsb,big|little,1|2|4|8,1|2|4|8,top-down|bottom-up".into(),
        )
    };
    if parts.len() != 5
        || !["msb", "lsb"].contains(&parts[0])
        || !["big", "little"].contains(&parts[1])
        || !["1", "2", "4", "8"].contains(&parts[2])
        || !["1", "2", "4", "8"].contains(&parts[3])
        || !["top-down", "bottom-up"].contains(&parts[4])
    {
        return Err(invalid());
    }
    Ok(ByteLayout {
        bit_order: if parts[0] == "lsb" {
            BitOrder::LsbFirst
        } else {
            BitOrder::MsbFirst
        },
        byte_order: order(parts[1]),
        row_alignment: parts[2].parse().map_err(|_| invalid())?,
        scan_unit: parts[3].parse().map_err(|_| invalid())?,
        row_order: if parts[4] == "bottom-up" {
            RowOrder::BottomUp
        } else {
            RowOrder::TopDown
        },
    })
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        require_feature(
            self.gem_read_byte_order.is_some() || self.gem_read_word_order.is_some(),
            cfg!(feature = "gem"),
            "gem",
        )?;
        require_feature(
            self.snf_read_byte_order.is_some() || self.snf_read_layout.is_some(),
            cfg!(feature = "snf"),
            "snf",
        )?;
        #[cfg(feature = "gem")]
        registry.set_gem_read_options(fontpixel::formats::gem::GemReadOptions {
            byte_order: self.gem_read_byte_order.as_deref().map(order),
            bitmap_word_order: self.gem_read_word_order.as_deref().map(order),
        });
        #[cfg(feature = "snf")]
        registry.set_snf_read_options(fontpixel::formats::snf::SnfReadOptions {
            byte_order: self.snf_read_byte_order.as_deref().map(order),
            bitmap: self.snf_read_layout.as_deref().map(layout).transpose()?,
        });
        let _ = registry;
        Ok(())
    }
}

const WRITE_OPTIONS_AMIGA_PACKAGING: [&str; 3] = ["auto", "contents", "strike"];
const WRITE_OPTIONS_GEM_BYTE_ORDER: [&str; 2] = ["big", "little"];
const WRITE_OPTIONS_GEM_WORD_ORDER: [&str; 2] = ["big", "little"];
const WRITE_OPTIONS_SNF_BYTE_ORDER: [&str; 2] = ["big", "little"];
/// Write options: explicit GEM and SNF machine layout controls.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct WriteOptions {
    /// Amiga output as one strike or a .font descriptor with size assets (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_AMIGA_PACKAGING))]
    pub amiga_packaging: Option<String>,
    /// Refuse interpreting representable Unicode characters as Amiga Latin-1 codes.
    #[cfg_attr(feature = "clap", arg(long))]
    pub amiga_no_unicode_latin1: bool,
    /// Amiga planar coverage depth, from 1 through 8 bits.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(u8).range(1..=8)))]
    pub amiga_color_depth: Option<u8>,
    /// GEM output header integer order (default big).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_GEM_BYTE_ORDER))]
    pub gem_byte_order: Option<String>,
    /// GEM output bitmap word order (default big).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_GEM_WORD_ORDER))]
    pub gem_word_order: Option<String>,
    /// Explicitly reinterpret GEM Unicode numbers as untagged legacy codes, with loss.
    #[cfg_attr(feature = "clap", arg(long))]
    pub gem_unicode_as_legacy: bool,
    /// SNF output integer order (default little).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_SNF_BYTE_ORDER))]
    pub snf_byte_order: Option<String>,
    /// SNF output bitmap layout: bits,bytes,row-padding,scan-unit,rows.
    #[cfg_attr(feature = "clap", arg(long))]
    pub snf_layout: Option<String>,
    /// Omit SNF's vendor layout property; reopening may require --snf-read-layout.
    #[cfg_attr(feature = "clap", arg(long))]
    pub snf_no_layout_property: bool,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(WriteOptions);
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.amiga_packaging.as_deref() {
            check_choice("amiga_packaging", value, &WRITE_OPTIONS_AMIGA_PACKAGING)?;
        }
        if let Some(value) = self.amiga_color_depth {
            check_range("amiga_color_depth", i128::from(value), 1, 8)?;
        }
        if let Some(value) = self.gem_byte_order.as_deref() {
            check_choice("gem_byte_order", value, &WRITE_OPTIONS_GEM_BYTE_ORDER)?;
        }
        if let Some(value) = self.gem_word_order.as_deref() {
            check_choice("gem_word_order", value, &WRITE_OPTIONS_GEM_WORD_ORDER)?;
        }
        if let Some(value) = self.snf_byte_order.as_deref() {
            check_choice("snf_byte_order", value, &WRITE_OPTIONS_SNF_BYTE_ORDER)?;
        }
        Ok(())
    }
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let amiga_requested = self.amiga_packaging.is_some()
            || self.amiga_no_unicode_latin1
            || self.amiga_color_depth.is_some();
        require_target(amiga_requested, id, "amiga")?;
        require_feature(
            amiga_requested || id == "amiga",
            cfg!(feature = "amiga"),
            "amiga",
        )?;
        #[cfg(feature = "amiga")]
        if id == "amiga" {
            use fontpixel::formats::amiga::{AmigaWriteOptions, Packaging};
            registry.set_amiga_write_options(AmigaWriteOptions {
                packaging: match self.amiga_packaging.as_deref() {
                    Some("contents") => Packaging::Contents,
                    Some("strike") => Packaging::Strike,
                    _ => Packaging::Auto,
                },
                unicode_as_latin1: !self.amiga_no_unicode_latin1,
                color_depth: self.amiga_color_depth,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let gem_requested = self.gem_byte_order.is_some()
            || self.gem_word_order.is_some()
            || self.gem_unicode_as_legacy;
        let snf_requested = self.snf_byte_order.is_some()
            || self.snf_layout.is_some()
            || self.snf_no_layout_property;
        require_target(gem_requested, id, "gem")?;
        require_target(snf_requested, id, "snf")?;
        require_feature(gem_requested || id == "gem", cfg!(feature = "gem"), "gem")?;
        require_feature(snf_requested || id == "snf", cfg!(feature = "snf"), "snf")?;
        #[cfg(feature = "gem")]
        if id == "gem" {
            registry.set_gem_write_options(fontpixel::formats::gem::GemWriteOptions {
                byte_order: order(self.gem_byte_order.as_deref().unwrap_or("big")),
                bitmap_word_order: order(self.gem_word_order.as_deref().unwrap_or("big")),
                unicode_as_legacy: self.gem_unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "snf")]
        if id == "snf" {
            let mut options = fontpixel::formats::snf::SnfWriteOptions {
                include_layout_property: !self.snf_no_layout_property,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            };
            if let Some(v) = &self.snf_byte_order {
                options.byte_order = order(v);
            }
            if let Some(v) = &self.snf_layout {
                options.bitmap = layout(v)?;
            }
            registry.set_snf_write_options(options);
        }
        let _ = (limits, threshold, registry);
        Ok(())
    }
}
