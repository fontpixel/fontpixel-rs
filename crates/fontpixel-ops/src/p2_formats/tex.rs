//! TeX design metrics and Plan 9 subfont bundles.
use super::{require_feature, require_target};
use fontpixel::{Error, FontRegistry, ResourceLimits, Result};

/// Write options: teX design metrics and Plan 9 subfont bundles.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// TeX design size in points: integer or exact fraction, e.g. 10 or 21/2.
    #[cfg_attr(feature = "clap", arg(long))]
    pub tex_design_size: Option<String>,
    /// TeX horizontal and vertical resolution in dpi.
    #[cfg_attr(feature = "clap", arg(long))]
    pub tex_dpi: Option<u32>,
    /// TeX horizontal resolution override in dpi.
    #[cfg_attr(feature = "clap", arg(long))]
    pub tex_dpi_x: Option<u32>,
    /// TeX vertical resolution override in dpi.
    #[cfg_attr(feature = "clap", arg(long))]
    pub tex_dpi_y: Option<u32>,
    /// Plan 9 mapping bundle or one subfont image (default font).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["font", "subfont"]))]
    pub plan9_container: Option<String>,
    /// Maximum glyphs per generated Plan 9 subfont.
    #[cfg_attr(feature = "clap", arg(long))]
    pub plan9_max_glyphs: Option<usize>,
    /// Plan 9 image encoding: raw bytes or standard LZ77 literal blocks.
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["raw", "literal"]))]
    pub plan9_compression: Option<String>,
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let tex_requested = self.tex_design_size.is_some()
            || self.tex_dpi.is_some()
            || self.tex_dpi_x.is_some()
            || self.tex_dpi_y.is_some();
        if tex_requested && !matches!(id, "pk" | "gf") {
            return Err(Error::InvalidInput(
                "TeX output options require --to pk or --to gf".into(),
            ));
        }
        let p9_requested = self.plan9_container.is_some()
            || self.plan9_max_glyphs.is_some()
            || self.plan9_compression.is_some();
        require_target(p9_requested, id, "plan9")?;
        require_feature(id == "pk", cfg!(feature = "pk"), "pk")?;
        require_feature(id == "gf", cfg!(feature = "gf"), "gf")?;
        require_feature(
            id == "plan9" || p9_requested,
            cfg!(feature = "plan9"),
            "plan9",
        )?;
        #[cfg(any(feature = "pk", feature = "gf"))]
        if matches!(id, "pk" | "gf") {
            let design_size = self
                .tex_design_size
                .as_deref()
                .map(|text| {
                    let (n, d) = text.split_once('/').unwrap_or((text, "1"));
                    let n = n.parse::<i64>().map_err(|_| {
                        Error::InvalidInput("Invalid TeX design size numerator".into())
                    })?;
                    let d = d.parse::<u32>().map_err(|_| {
                        Error::InvalidInput("Invalid TeX design size denominator".into())
                    })?;
                    if n <= 0 || d == 0 {
                        return Err(Error::InvalidInput(
                            "TeX design size must be positive".into(),
                        ));
                    }
                    fontpixel::model::Rational::new(n, d)
                })
                .transpose()?;
            let resolution =
                if self.tex_dpi.is_some() || self.tex_dpi_x.is_some() || self.tex_dpi_y.is_some() {
                    let x = self.tex_dpi_x.or(self.tex_dpi).ok_or_else(|| {
                        Error::InvalidInput("Set both TeX dpi axes, or --tex-dpi".into())
                    })?;
                    let y = self.tex_dpi_y.or(self.tex_dpi).ok_or_else(|| {
                        Error::InvalidInput("Set both TeX dpi axes, or --tex-dpi".into())
                    })?;
                    if x == 0 || y == 0 {
                        return Err(Error::InvalidInput("TeX dpi must be positive".into()));
                    }
                    Some(fontpixel::model::Vector::new(i64::from(x), i64::from(y)))
                } else {
                    None
                };
            #[cfg(feature = "pk")]
            if id == "pk" {
                registry.set_pk_write_options(fontpixel::formats::pk::PkWriteOptions {
                    design_size,
                    resolution,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                });
            }
            #[cfg(feature = "gf")]
            if id == "gf" {
                registry.set_gf_write_options(fontpixel::formats::gf::GfWriteOptions {
                    design_size,
                    resolution,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                });
            }
        }
        #[cfg(feature = "plan9")]
        if id == "plan9" {
            use fontpixel::formats::plan9::{Container, Plan9WriteOptions};
            let mut options = Plan9WriteOptions {
                container: if self.plan9_container.as_deref() == Some("subfont") {
                    Container::Subfont
                } else {
                    Container::Font
                },
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            };
            if let Some(n) = self.plan9_max_glyphs {
                options.max_glyphs_per_subfont = n;
            }
            if self.plan9_compression.as_deref() == Some("literal") {
                options.compression = fontpixel::formats::plan9::Compression::Literal;
            }
            registry.set_plan9_write_options(options);
        }
        let _ = (limits, threshold, registry);
        Ok(())
    }
}
