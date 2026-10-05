//! Bitmap-font core with bounded Mono/Gray8 operations, loss-preserving strikes,
//! and exact charset maps. Native format profiles are feature-gated; query the
//! registry for enabled capabilities.
#![doc = include_str!("../../../docs/api-rustdoc.md")]
#![forbid(unsafe_code)]

pub mod bitmap;
pub mod charset;
#[cfg(feature = "compat-bdfparser")]
pub mod compat;
pub mod error;
pub mod formats;
pub mod merge;
pub mod metadata;
pub mod model;
pub use merge::merge;
pub mod outline;
pub mod projection;
pub mod render;
pub mod report;
pub mod resources;
#[cfg(feature = "sfnt")]
pub mod sfnt;
#[cfg(feature = "sfnt")]
pub mod variants;
mod working;

pub use error::{Error, Operation, Result};
pub use formats::{ReadOptions, Registry, WriteOptions};
pub use report::{Report, ReportEntry};
pub use resources::{
    Input, OutputAsset, OutputBundle, ResourceLimits, ResourcePath, ResourceResolver,
};

/// Core bitmap with packed monochrome storage by default.
pub use bitmap::Bitmap;
pub use model::{Font, FontSet, Glyph};
#[cfg(feature = "sfnt")]
pub use variants::Variants;

/// Registry specialized to the real font model.
pub type FontRegistry = Registry<FontSet>;

/// Construct the built-in font registry with feature-gated implemented codecs.
pub fn registry() -> FontRegistry {
    #[allow(unused_mut)]
    let mut registry = FontRegistry::new();
    #[cfg(feature = "bdf")]
    registry.install_bdf();
    #[cfg(feature = "pcf")]
    registry.install_pcf();
    #[cfg(feature = "yaff")]
    registry.install_yaff();
    #[cfg(feature = "otb")]
    registry.install_embedded("otb", sfnt::bitmap::Kind::OpenType, true);
    #[cfg(feature = "bdat")]
    registry.install_embedded("bdat", sfnt::bitmap::Kind::Apple, true);
    #[cfg(feature = "color")]
    registry.install_embedded("color", sfnt::bitmap::Kind::Color, true);
    #[cfg(feature = "ttf")]
    registry.install_vector("ttf");
    #[cfg(feature = "otf")]
    registry.install_vector("otf");
    #[cfg(feature = "woff")]
    registry.install_vector("woff");
    #[cfg(feature = "woff2")]
    registry.install_vector("woff2");
    #[cfg(feature = "ttc")]
    registry.install_vector("ttc");
    #[cfg(feature = "vector-raster")]
    registry.install_vector("vector-raster");
    #[cfg(feature = "svg")]
    registry.install_svg();
    #[cfg(feature = "hex")]
    registry.install_hex();
    #[cfg(feature = "psf")]
    registry.install_psf();
    #[cfg(feature = "c")]
    registry.install_c();
    #[cfg(feature = "image")]
    registry.install_image();

    #[cfg(feature = "fnt")]
    registry.install_fnt();
    #[cfg(feature = "fon")]
    registry.install_fon();
    #[cfg(feature = "kbitx")]
    registry.install_kbitx();
    #[cfg(feature = "pbf")]
    registry.install_pbf();
    #[cfg(feature = "hbf")]
    registry.install_hbf();
    #[cfg(feature = "pf2")]
    registry.install_pf2();
    #[cfg(feature = "fontx2")]
    registry.install_fontx2();
    #[cfg(feature = "hzk")]
    registry.install_hzk();
    #[cfg(feature = "draw")]
    registry.install_draw();
    #[cfg(feature = "raw")]
    registry.install_raw();
    #[cfg(feature = "cpi")]
    registry.install_cpi();
    #[cfg(feature = "nfnt")]
    registry.install_nfnt();
    #[cfg(feature = "bmfont")]
    registry.install_bmfont();
    #[cfg(feature = "sfd")]
    registry.install_sfd();
    #[cfg(feature = "kbits")]
    registry.install_kbits();
    #[cfg(feature = "pk")]
    registry.install_pk();
    #[cfg(feature = "gf")]
    registry.install_gf();
    #[cfg(feature = "plan9")]
    registry.install_plan9();
    #[cfg(feature = "amiga")]
    registry.install_amiga();
    #[cfg(feature = "gem")]
    registry.install_gem();
    #[cfg(feature = "snf")]
    registry.install_snf();
    #[cfg(feature = "hanfont")]
    registry.install_hanfont();
    #[cfg(feature = "iigs")]
    registry.install_iigs();
    #[cfg(feature = "vfont")]
    registry.install_vfont();
    #[cfg(feature = "fzx")]
    registry.install_fzx();
    #[cfg(feature = "figlet")]
    registry.install_figlet();
    #[cfg(feature = "palm")]
    registry.install_palm();
    #[cfg(feature = "gdr")]
    registry.install_gdr();
    #[cfg(feature = "drcs")]
    registry.install_drcs();
    #[cfg(feature = "dosstart")]
    registry.install_dosstart();
    #[cfg(feature = "edwin")]
    registry.install_edwin();
    #[cfg(feature = "mkwinfont")]
    registry.install_mkwinfont();
    #[cfg(feature = "psf2txt")]
    registry.install_psf2txt();
    #[cfg(feature = "pcr")]
    registry.install_pcr();
    #[cfg(feature = "plus3dos")]
    registry.install_plus3dos();
    #[cfg(feature = "drhalo")]
    registry.install_drhalo();
    #[cfg(feature = "writeon")]
    registry.install_writeon();
    #[cfg(feature = "daisy")]
    registry.install_daisy();
    #[cfg(feature = "polyprint")]
    registry.install_polyprint();
    #[cfg(feature = "printshop")]
    registry.install_printshop();
    #[cfg(feature = "tasprint")]
    registry.install_tasprint();
    #[cfg(feature = "pxl")]
    registry.install_pxl();
    #[cfg(feature = "signum")]
    registry.install_signum();
    #[cfg(feature = "rockbox")]
    registry.install_rockbox();
    #[cfg(feature = "u8m")]
    registry.install_u8m();
    #[cfg(feature = "cybiko")]
    registry.install_cybiko();
    #[cfg(feature = "playdate")]
    registry.install_playdate();
    #[cfg(feature = "hmzk")]
    registry.install_hmzk();
    #[cfg(feature = "sbf")]
    registry.install_sbf();
    #[cfg(feature = "mgtk")]
    registry.install_mgtk();
    #[cfg(feature = "qpf")]
    registry.install_qpf();
    #[cfg(feature = "symbos")]
    registry.install_symbos();
    #[cfg(feature = "qpf2")]
    registry.install_qpf2();
    #[cfg(feature = "oberon")]
    registry.install_oberon();
    #[cfg(feature = "fbit")]
    registry.install_fbit();
    #[cfg(feature = "lisa")]
    registry.install_lisa();
    #[cfg(feature = "wsfont")]
    registry.install_wsfont();
    #[cfg(feature = "beos")]
    registry.install_beos();
    #[cfg(feature = "bbc")]
    registry.install_bbc();
    #[cfg(feature = "comfont")]
    registry.install_comfont();
    #[cfg(feature = "consoleet")]
    registry.install_consoleet();
    #[cfg(feature = "grasp")]
    registry.install_grasp();
    #[cfg(feature = "hrcg")]
    registry.install_hrcg();
    #[cfg(feature = "wyse")]
    registry.install_wyse();
    #[cfg(feature = "xbin")]
    registry.install_xbin();
    #[cfg(feature = "zapfont")]
    registry.install_zapfont();
    #[cfg(feature = "catalyst")]
    registry.install_catalyst();
    #[cfg(feature = "chiwriter")]
    registry.install_chiwriter();
    #[cfg(feature = "vtfont")]
    registry.install_vtfont();
    #[cfg(feature = "hppcl")]
    registry.install_hppcl();
    #[cfg(feature = "dashen")]
    registry.install_dashen();
    #[cfg(feature = "vlw")]
    registry.install_vlw();
    #[cfg(feature = "pilfont")]
    registry.install_pilfont();
    #[cfg(feature = "prebuilt")]
    registry.install_prebuilt();
    #[cfg(feature = "geos")]
    registry.install_geos();
    #[cfg(feature = "gpi")]
    registry.install_gpi();
    #[cfg(feature = "riscos")]
    registry.install_riscos();
    #[cfg(feature = "sfont")]
    registry.install_sfont();
    #[cfg(feature = "pfr")]
    registry.install_pfr();
    #[cfg(feature = "pcgeos")]
    registry.install_pcgeos();
    #[cfg(feature = "xerox")]
    registry.install_xerox();
    #[cfg(feature = "ssfn")]
    registry.install_ssfn();
    registry
}

/// Decode bounded standalone bytes using the real font model and built-in registry.
/// Content recognition precedes filename hints; errors preserve explicit diagnostics.
pub fn read(bytes: &[u8], options: &ReadOptions) -> Result<formats::Loaded<FontSet>> {
    registry().read(&Input::new(bytes), options)
}

/// Encode a complete font set into validated in-memory assets with diagnostics.
/// Strict mode is enforced by the registry before any output is returned.
/// BDF and PCF use their respective features; planned formats remain explicit errors.
pub fn write(fonts: &FontSet, format: &str, options: &WriteOptions) -> Result<resources::Written> {
    registry().write(fonts, format, options)
}

#[cfg(any(feature = "woff", feature = "woff2"))]
pub mod woff;
