//! Typed per-format options: registry setters and serde normalization.
//!
//! Every per-format option struct, codec struct and option enum of
//! `fontpixel::formats` (and the `fontpixel::resources` plain-data types)
//! derives serde. Python passes them as dicts/strings with the Rust field
//! names; unknown keys and out-of-range values raise `ValueError`.
//!
//! - `Registry.set_read_options(format, dict)` / `set_write_options` dispatch
//!   by format ID to the typed `Registry::set_*_options` setter; each setter is
//!   also a method of its own (`Registry.set_fzx_write_options(dict)`).
//! - `fontpixel.formats.normalize(type, value=None)` deserializes `value` into
//!   the named Rust type (by its Rust path, e.g.
//!   `"formats::fzx::FzxWriteOptions"`) and returns its serialized form;
//!   `value=None` means `{}`, i.e. the documented defaults.
use crate::conv;
use crate::errors::{IntoPy, InvalidInputError};
use crate::formats::PyRegistry;
use fontpixel::FontRegistry;
use fontpixel::formats as f;
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn no_setter(format: &str, side: &str, alternative: &str) -> PyErr {
    InvalidInputError::new_err(format!(
        "format '{format}' has no registry {side} options{alternative}"
    ))
}

fn unknown(format: &str) -> PyErr {
    crate::errors::to_py(fontpixel::Error::UnknownFormat(format.into()))
}

/// Infallible setter adapter.
trait Apply<T> {
    fn apply(self, reg: &mut FontRegistry, value: T) -> PyResult<()>;
}
impl<T> Apply<T> for fn(&mut FontRegistry, T) {
    fn apply(self, reg: &mut FontRegistry, value: T) -> PyResult<()> {
        self(reg, value);
        Ok(())
    }
}
/// Fallible setter adapter (`set_pcf_write_options`, `set_yaff_write_options`).
struct Checked<T>(fn(&mut FontRegistry, T) -> fontpixel::Result<()>);
impl<T> Apply<T> for Checked<T> {
    fn apply(self, reg: &mut FontRegistry, value: T) -> PyResult<()> {
        (self.0)(reg, value).py()
    }
}

fn apply<T: serde::de::DeserializeOwned>(
    reg: &mut FontRegistry,
    options: &Bound<'_, PyAny>,
    setter: impl Apply<T>,
) -> PyResult<()> {
    let value: T = conv::from_py_required(options)?;
    setter.apply(reg, value)
}

/// Generates the read/write dispatch tables and one `Registry` method per
/// typed setter.
macro_rules! setters {
    (
        read { $($($rid:literal)|+ => $rset:ident : $rty:ty),* $(,)? }
        write { $($($wid:literal)|+ => $wset:ident : $wty:ty),* $(,)? }
        checked_write { $($cid:literal => $cset:ident : $cty:ty),* $(,)? }
    ) => {
        /// Apply a read-options dict to `format`'s reader.
        pub fn set_read(reg: &mut FontRegistry, format: &str, options: &Bound<'_, PyAny>) -> PyResult<()> {
            match format {
                $($($rid)|+ => apply(reg, options, FontRegistry::$rset as fn(&mut FontRegistry, $rty)),)*
                other => Err(read_fallback(other)),
            }
        }
        /// Apply a write-options dict to `format`'s writer.
        pub fn set_write(reg: &mut FontRegistry, format: &str, options: &Bound<'_, PyAny>) -> PyResult<()> {
            match format {
                $($($wid)|+ => apply(reg, options, FontRegistry::$wset as fn(&mut FontRegistry, $wty)),)*
                $($cid => apply(reg, options, Checked::<$cty>(FontRegistry::$cset)),)*
                other => Err(write_fallback(other)),
            }
        }
        /// Format IDs with a registry read-options setter.
        pub const READ_IDS: &[&str] = &[$($($rid,)+)*];
        /// Format IDs with a registry write-options setter.
        pub const WRITE_IDS: &[&str] = &[$($($wid,)+)* $($cid),*];

        setters!(@methods $($rset : $rty,)* $($wset : $wty,)*);
        setters!(@checked $($cset : $cty,)*);
    };
    (@methods $($set:ident : $ty:ty,)*) => {
        #[pymethods]
        impl PyRegistry {
            $(
                #[doc = concat!("`Registry::", stringify!($set), "`: apply a `", stringify!($ty), "` dict.")]
                fn $set(&mut self, options: &Bound<'_, PyAny>) -> PyResult<()> {
                    apply(&mut self.registry, options, FontRegistry::$set as fn(&mut FontRegistry, $ty))
                }
            )*
        }
    };
    (@checked $($set:ident : $ty:ty,)*) => {
        #[pymethods]
        impl PyRegistry {
            $(
                #[doc = concat!("`Registry::", stringify!($set), "`: apply a `", stringify!($ty), "` dict; invalid combinations raise.")]
                fn $set(&mut self, options: &Bound<'_, PyAny>) -> PyResult<()> {
                    apply(&mut self.registry, options, Checked::<$ty>(FontRegistry::$set))
                }
            )*
        }
    };
}

fn read_fallback(format: &str) -> PyErr {
    match format {
        "otb" | "bdat" | "color" => no_setter(
            format,
            "read",
            "; use fontpixel.formats.embedded.read(data, kind, options)",
        ),
        "bdf" | "pcf" | "psf" | "yaff" | "draw" | "sfd" | "kbits" | "pk" | "gf" | "plan9"
        | "hbf" => no_setter(format, "read", " (its reader takes no options)"),
        id if fontpixel::formats::catalogue::CATALOGUE
            .iter()
            .any(|f| f.id == id) =>
        {
            no_setter(id, "read", "")
        }
        other => unknown(other),
    }
}

fn write_fallback(format: &str) -> PyErr {
    match format {
        "bdf" => no_setter(
            format,
            "write",
            "; use fontpixel.formats.bdf.write(font, options)",
        ),
        id if fontpixel::formats::catalogue::CATALOGUE
            .iter()
            .any(|f| f.id == id) =>
        {
            no_setter(id, "write", "")
        }
        other => unknown(other),
    }
}

setters! {
    read {
        "pf2" => set_pf2_read_options: f::pf2::Pf2ReadOptions,
        "fontx2" => set_fontx2_read_options: f::fontx2::Fontx2ReadOptions,
        "hzk" => set_hzk_read_options: f::hzk::HzkReadOptions,
        "raw" => set_raw_read_options: f::raw::RawReadOptions,
        "cpi" => set_cpi_read_options: f::cpi::CpiReadOptions,
        "nfnt" => set_nfnt_read_options: f::nfnt::NfntReadOptions,
        "bmfont" => set_bmfont_read_options: f::bmfont::BmfontReadOptions,
        "amiga" => set_amiga_read_options: f::amiga::AmigaReadOptions,
        "gem" => set_gem_read_options: f::gem::GemReadOptions,
        "snf" => set_snf_read_options: f::snf::SnfReadOptions,
        "svg" => set_svg_read_options: f::svg::ReadOptions,
        "hex" => set_hex_read_options: f::hex::HexReadOptions,
        "image" => set_image_read_options: f::image::ImageReadOptions,
        "fnt" | "fon" => set_fnt_read_options: f::fnt::FntReadOptions,
        "c" => set_c_read_options: f::c::CReadOptions,
        "kbitx" => set_kbitx_read_options: f::kbitx::KbitxReadOptions,
        "pbf" => set_pbf_read_options: f::pbf::PbfReadOptions,
        "ttf" | "otf" | "woff" | "woff2" | "ttc" | "vector-raster" => set_vector_read_options: f::vector_raster::Options,
        "hanfont" => set_hanfont_read_options: f::hanfont::HanfontReadOptions,
        "iigs" => set_iigs_read_options: f::iigs::IigsReadOptions,
        "vfont" => set_vfont_read_options: f::vfont::VfontReadOptions,
        "fzx" => set_fzx_read_options: f::fzx::FzxReadOptions,
        "figlet" => set_figlet_read_options: f::figlet::FigletReadOptions,
        "palm" => set_palm_read_options: f::palm::PalmReadOptions,
        "gdr" => set_gdr_read_options: f::gdr::GdrReadOptions,
        "drcs" => set_drcs_read_options: f::drcs::DrcsReadOptions,
        "dosstart" => set_dosstart_read_options: f::dosstart::DosstartReadOptions,
        "edwin" => set_edwin_read_options: f::edwin::EdwinReadOptions,
        "mkwinfont" => set_mkwinfont_read_options: f::mkwinfont::MkwinfontReadOptions,
        "psf2txt" => set_psf2txt_read_options: f::psf2txt::Psf2txtReadOptions,
        "pcr" => set_pcr_read_options: f::pcr::PcrReadOptions,
        "plus3dos" => set_plus3dos_read_options: f::plus3dos::Plus3dosReadOptions,
        "drhalo" => set_drhalo_read_options: f::drhalo::DrhaloReadOptions,
        "writeon" => set_writeon_read_options: f::writeon::WriteonReadOptions,
        "daisy" => set_daisy_read_options: f::daisy::DaisyReadOptions,
        "polyprint" => set_polyprint_read_options: f::polyprint::PolyprintReadOptions,
        "printshop" => set_printshop_read_options: f::printshop::PrintshopReadOptions,
        "tasprint" => set_tasprint_read_options: f::tasprint::TasprintReadOptions,
        "pxl" => set_pxl_read_options: f::pxl::PxlReadOptions,
        "signum" => set_signum_read_options: f::signum::SignumReadOptions,
        "rockbox" => set_rockbox_read_options: f::rockbox::RockboxReadOptions,
        "u8m" => set_u8m_read_options: f::u8m::U8mReadOptions,
        "cybiko" => set_cybiko_read_options: f::cybiko::CybikoReadOptions,
        "playdate" => set_playdate_read_options: f::playdate::PlaydateReadOptions,
        "hmzk" => set_hmzk_read_options: f::hmzk::HmzkReadOptions,
        "sbf" => set_sbf_read_options: f::sbf::SbfReadOptions,
        "mgtk" => set_mgtk_read_options: f::mgtk::MgtkReadOptions,
        "qpf" => set_qpf_read_options: f::qpf::QpfReadOptions,
        "symbos" => set_symbos_read_options: f::symbos::SymbosReadOptions,
        "qpf2" => set_qpf2_read_options: f::qpf2::Qpf2ReadOptions,
        "oberon" => set_oberon_read_options: f::oberon::OberonReadOptions,
        "fbit" => set_fbit_read_options: f::fbit::FbitReadOptions,
        "lisa" => set_lisa_read_options: f::lisa::LisaReadOptions,
        "wsfont" => set_wsfont_read_options: f::wsfont::WsfontReadOptions,
        "beos" => set_beos_read_options: f::beos::BeosReadOptions,
        "bbc" => set_bbc_read_options: f::bbc::BbcReadOptions,
        "comfont" => set_comfont_read_options: f::comfont::ComfontReadOptions,
        "consoleet" => set_consoleet_read_options: f::consoleet::ConsoleetReadOptions,
        "grasp" => set_grasp_read_options: f::grasp::GraspReadOptions,
        "hrcg" => set_hrcg_read_options: f::hrcg::HrcgReadOptions,
        "wyse" => set_wyse_read_options: f::wyse::WyseReadOptions,
        "xbin" => set_xbin_read_options: f::xbin::XbinReadOptions,
        "zapfont" => set_zapfont_read_options: f::zapfont::ZapfontReadOptions,
        "catalyst" => set_catalyst_read_options: f::catalyst::CatalystReadOptions,
        "chiwriter" => set_chiwriter_read_options: f::chiwriter::ChiwriterReadOptions,
        "vtfont" => set_vtfont_read_options: f::vtfont::VtfontReadOptions,
        "hppcl" => set_hppcl_read_options: f::hppcl::HppclReadOptions,
        "dashen" => set_dashen_read_options: f::dashen::DashenReadOptions,
        "vlw" => set_vlw_read_options: f::vlw::VlwReadOptions,
        "pilfont" => set_pilfont_read_options: f::pilfont::PilfontReadOptions,
        "prebuilt" => set_prebuilt_read_options: f::prebuilt::PrebuiltReadOptions,
        "geos" => set_geos_read_options: f::geos::GeosReadOptions,
        "gpi" => set_gpi_read_options: f::gpi::GpiReadOptions,
        "riscos" => set_riscos_read_options: f::riscos::RiscosReadOptions,
        "sfont" => set_sfont_read_options: f::sfont::SfontReadOptions,
        "pfr" => set_pfr_read_options: f::pfr::PfrReadOptions,
        "pcgeos" => set_pcgeos_read_options: f::pcgeos::PcgeosReadOptions,
        "xerox" => set_xerox_read_options: f::xerox::XeroxReadOptions,
        "ssfn" => set_ssfn_read_options: f::ssfn::SsfnReadOptions,
    }
    write {
        "pf2" => set_pf2_write_options: f::pf2::Pf2WriteOptions,
        "fontx2" => set_fontx2_write_options: f::fontx2::Fontx2WriteOptions,
        "hzk" => set_hzk_write_options: f::hzk::HzkWriteOptions,
        "draw" => set_draw_write_options: f::draw::DrawWriteOptions,
        "raw" => set_raw_write_options: f::raw::RawWriteOptions,
        "cpi" => set_cpi_write_options: f::cpi::CpiWriteOptions,
        "nfnt" => set_nfnt_write_options: f::nfnt::NfntWriteOptions,
        "bmfont" => set_bmfont_write_options: f::bmfont::BmfontWriteOptions,
        "sfd" => set_sfd_write_options: f::sfd::SfdWriteOptions,
        "kbits" => set_kbits_write_options: f::kbits::KbitsWriteOptions,
        "pk" => set_pk_write_options: f::pk::PkWriteOptions,
        "gf" => set_gf_write_options: f::gf::GfWriteOptions,
        "plan9" => set_plan9_write_options: f::plan9::Plan9WriteOptions,
        "amiga" => set_amiga_write_options: f::amiga::AmigaWriteOptions,
        "gem" => set_gem_write_options: f::gem::GemWriteOptions,
        "snf" => set_snf_write_options: f::snf::SnfWriteOptions,
        "otb" | "bdat" => set_embedded_write_options: f::embedded::EmbeddedWriteOptions,
        "color" => set_color_write_options: f::embedded::ColorWriteOptions,
        "ttf" | "otf" | "woff" | "woff2" | "ttc" | "vector-raster" => set_vector_write_options: f::vector_raster::WriteOptions,
        "svg" => set_svg_write_options: f::svg::WriteOptions,
        "hex" => set_hex_write_options: f::hex::HexWriteOptions,
        "psf" => set_psf_write_options: f::psf::PsfWriteOptions,
        "image" => set_image_write_options: f::image::ImageWriteOptions,
        "fnt" | "fon" => set_fnt_write_options: f::fnt::FntWriteOptions,
        "c" => set_c_write_options: f::c::CWriteOptions,
        "kbitx" => set_kbitx_write_options: f::kbitx::KbitxWriteOptions,
        "pbf" => set_pbf_write_options: f::pbf::PbfWriteOptions,
        "hbf" => set_hbf_write_options: f::hbf::HbfWriteOptions,
        "hanfont" => set_hanfont_write_options: f::hanfont::HanfontWriteOptions,
        "iigs" => set_iigs_write_options: f::iigs::IigsWriteOptions,
        "vfont" => set_vfont_write_options: f::vfont::VfontWriteOptions,
        "fzx" => set_fzx_write_options: f::fzx::FzxWriteOptions,
        "figlet" => set_figlet_write_options: f::figlet::FigletWriteOptions,
        "palm" => set_palm_write_options: f::palm::PalmWriteOptions,
        "gdr" => set_gdr_write_options: f::gdr::GdrWriteOptions,
        "drcs" => set_drcs_write_options: f::drcs::DrcsWriteOptions,
        "dosstart" => set_dosstart_write_options: f::dosstart::DosstartWriteOptions,
        "edwin" => set_edwin_write_options: f::edwin::EdwinWriteOptions,
        "mkwinfont" => set_mkwinfont_write_options: f::mkwinfont::MkwinfontWriteOptions,
        "psf2txt" => set_psf2txt_write_options: f::psf2txt::Psf2txtWriteOptions,
        "pcr" => set_pcr_write_options: f::pcr::PcrWriteOptions,
        "plus3dos" => set_plus3dos_write_options: f::plus3dos::Plus3dosWriteOptions,
        "drhalo" => set_drhalo_write_options: f::drhalo::DrhaloWriteOptions,
        "writeon" => set_writeon_write_options: f::writeon::WriteonWriteOptions,
        "daisy" => set_daisy_write_options: f::daisy::DaisyWriteOptions,
        "polyprint" => set_polyprint_write_options: f::polyprint::PolyprintWriteOptions,
        "printshop" => set_printshop_write_options: f::printshop::PrintshopWriteOptions,
        "tasprint" => set_tasprint_write_options: f::tasprint::TasprintWriteOptions,
        "pxl" => set_pxl_write_options: f::pxl::PxlWriteOptions,
        "signum" => set_signum_write_options: f::signum::SignumWriteOptions,
        "rockbox" => set_rockbox_write_options: f::rockbox::RockboxWriteOptions,
        "u8m" => set_u8m_write_options: f::u8m::U8mWriteOptions,
        "cybiko" => set_cybiko_write_options: f::cybiko::CybikoWriteOptions,
        "playdate" => set_playdate_write_options: f::playdate::PlaydateWriteOptions,
        "hmzk" => set_hmzk_write_options: f::hmzk::HmzkWriteOptions,
        "sbf" => set_sbf_write_options: f::sbf::SbfWriteOptions,
        "mgtk" => set_mgtk_write_options: f::mgtk::MgtkWriteOptions,
        "qpf" => set_qpf_write_options: f::qpf::QpfWriteOptions,
        "symbos" => set_symbos_write_options: f::symbos::SymbosWriteOptions,
        "qpf2" => set_qpf2_write_options: f::qpf2::Qpf2WriteOptions,
        "oberon" => set_oberon_write_options: f::oberon::OberonWriteOptions,
        "fbit" => set_fbit_write_options: f::fbit::FbitWriteOptions,
        "lisa" => set_lisa_write_options: f::lisa::LisaWriteOptions,
        "wsfont" => set_wsfont_write_options: f::wsfont::WsfontWriteOptions,
        "beos" => set_beos_write_options: f::beos::BeosWriteOptions,
        "bbc" => set_bbc_write_options: f::bbc::BbcWriteOptions,
        "comfont" => set_comfont_write_options: f::comfont::ComfontWriteOptions,
        "consoleet" => set_consoleet_write_options: f::consoleet::ConsoleetWriteOptions,
        "grasp" => set_grasp_write_options: f::grasp::GraspWriteOptions,
        "hrcg" => set_hrcg_write_options: f::hrcg::HrcgWriteOptions,
        "wyse" => set_wyse_write_options: f::wyse::WyseWriteOptions,
        "xbin" => set_xbin_write_options: f::xbin::XbinWriteOptions,
        "zapfont" => set_zapfont_write_options: f::zapfont::ZapfontWriteOptions,
        "catalyst" => set_catalyst_write_options: f::catalyst::CatalystWriteOptions,
        "chiwriter" => set_chiwriter_write_options: f::chiwriter::ChiwriterWriteOptions,
        "vtfont" => set_vtfont_write_options: f::vtfont::VtfontWriteOptions,
        "hppcl" => set_hppcl_write_options: f::hppcl::HppclWriteOptions,
        "dashen" => set_dashen_write_options: f::dashen::DashenWriteOptions,
        "vlw" => set_vlw_write_options: f::vlw::VlwWriteOptions,
        "pilfont" => set_pilfont_write_options: f::pilfont::PilfontWriteOptions,
        "prebuilt" => set_prebuilt_write_options: f::prebuilt::PrebuiltWriteOptions,
        "geos" => set_geos_write_options: f::geos::GeosWriteOptions,
        "gpi" => set_gpi_write_options: f::gpi::GpiWriteOptions,
        "riscos" => set_riscos_write_options: f::riscos::RiscosWriteOptions,
        "sfont" => set_sfont_write_options: f::sfont::SfontWriteOptions,
        "pfr" => set_pfr_write_options: f::pfr::PfrWriteOptions,
        "pcgeos" => set_pcgeos_write_options: f::pcgeos::PcgeosWriteOptions,
        "xerox" => set_xerox_write_options: f::xerox::XeroxWriteOptions,
        "ssfn" => set_ssfn_write_options: f::ssfn::SsfnWriteOptions,
    }
    checked_write {
        "bdf" => set_bdf_write_options: f::bdf::BdfWriteOptions,
        "pcf" => set_pcf_write_options: f::pcf::PcfWriteOptions,
        "yaff" => set_yaff_write_options: f::yaff::YaffWriteOptions,
    }
}

/// Apply a codec dict (`{"read_options": …, "write_options": …}`).
pub fn set_codec(reg: &mut FontRegistry, format: &str, codec: &Bound<'_, PyAny>) -> PyResult<()> {
    let dict = codec
        .cast::<PyDict>()
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("a codec is a dict"))?;
    for (key, value) in dict.iter() {
        let key: String = key.extract()?;
        match key.as_str() {
            "read_options" => set_read(reg, format, &value)?,
            "write_options" => set_write(reg, format, &value)?,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown codec field `{other}`, expected `read_options` or `write_options`"
                )));
            }
        }
    }
    Ok(())
}

fn roundtrip<'py, T>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let parsed: T = conv::from_py_required(value)?;
    conv::to_py(py, &parsed)
}

macro_rules! normalize_table {
    ($($key:literal => $ty:ty,)*) => {
        /// Rust paths (relative to the crate) of every normalizable type.
        pub const TYPES: &[&str] = &[$($key),*];
        fn dispatch<'py>(py: Python<'py>, key: &str, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
            match key {
                $($key => roundtrip::<$ty>(py, value),)*
                other => Err(pyo3::exceptions::PyKeyError::new_err(format!(
                    "unknown option type `{other}`; see fontpixel.formats.option_types()"
                ))),
            }
        }
    };
}
include!("formats_types.rs");

/// Validate `value` as the Rust type `type_name` (a path such as
/// `"formats::fzx::FzxWriteOptions"` or `"resources::ResourceLimits"`) and
/// return its serialized form. `value=None` normalizes `{}`, giving the
/// documented defaults of option structs. Unknown keys, wrong types, unknown
/// enum names and out-of-range integers raise `ValueError`.
#[pyfunction]
#[pyo3(signature = (type_name, value = None))]
fn normalize<'py>(
    py: Python<'py>,
    type_name: &str,
    value: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let empty;
    let value = match value {
        Some(v) if !v.is_none() => v,
        _ => {
            empty = PyDict::new(py).into_any();
            &empty
        }
    };
    dispatch(py, type_name, value)
}

/// Every type name `normalize` accepts.
#[pyfunction]
fn option_types() -> Vec<&'static str> {
    TYPES.to_vec()
}

/// Format IDs whose registry readers / writers take typed options:
/// `{"read": [...], "write": [...]}`.
#[pyfunction]
fn configurable_formats<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("read", READ_IDS.to_vec())?;
    d.set_item("write", WRITE_IDS.to_vec())?;
    Ok(d)
}

/// Register `normalize`, `option_types` and `configurable_formats`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(normalize, m)?)?;
    m.add_function(wrap_pyfunction!(option_types, m)?)?;
    m.add_function(wrap_pyfunction!(configurable_formats, m)?)?;
    Ok(())
}
