//! Typed per-format options: registry setters, codecs and normalization.
//!
//! Every per-format option struct, codec struct and option enum of
//! `fontpixel::formats` derives serde; JS passes them as plain objects and
//! strings with the Rust field names (snake_case). Unknown keys and
//! out-of-range values throw `InvalidInputError`.
//!
//! - `Registry.setReadOptions(format, options)` / `setWriteOptions` dispatch
//!   by format ID to the typed `Registry::set_*_options` setter; each setter
//!   is also a method of its own (`Registry.setFzxWriteOptions(options)`),
//!   compiled into the builds whose profile has the format.
//! - `Registry.setOptions(format, {read, write})` and
//!   `Registry.setCodec(format, {read_options, write_options})`.
//! - `formats.normalize(typeName, value)` / `formats.optionTypes()` (also
//!   top-level `normalize`/`optionTypes`, see [`crate::normalize`]) and
//!   `formats.configurableFormats()`.
use crate::conv;
use crate::errors::{self, IntoJs, JsResult, invalid};
use crate::formats::JsRegistry;
use fontpixel::FontRegistry;
use fontpixel::formats as f;
use js_sys::{Object, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

fn no_setter(format: &str, side: &str, alternative: &str) -> JsValue {
    invalid(format!(
        "format '{format}' has no registry {side} options{alternative}"
    ))
}

/// The catalogue entry of `format`: `UnknownFormatError` for an unknown ID,
/// `FeatureDisabledError` when this build compiled the format out.
fn known(format: &str) -> JsResult<()> {
    match fontpixel::formats::catalogue::CATALOGUE
        .iter()
        .find(|f| f.id == format)
    {
        None => Err(errors::to_js(fontpixel::Error::UnknownFormat(
            format.into(),
        ))),
        Some(info) if !info.feature_enabled => {
            Err(errors::to_js(fontpixel::Error::FeatureDisabled {
                format: format.into(),
            }))
        }
        Some(_) => Ok(()),
    }
}

fn read_fallback(format: &str) -> JsValue {
    if let Err(error) = known(format) {
        return error;
    }
    match format {
        "otb" | "bdat" | "color" => no_setter(
            format,
            "read",
            "; use formats.embedded.read(data, kind, {options})",
        ),
        "bdf" | "pcf" | "psf" | "yaff" | "draw" | "sfd" | "kbits" | "pk" | "gf" | "plan9"
        | "hbf" => no_setter(format, "read", " (its reader takes no options)"),
        id => no_setter(id, "read", ""),
    }
}

fn write_fallback(format: &str) -> JsValue {
    if let Err(error) = known(format) {
        return error;
    }
    match format {
        "bdf" => no_setter(format, "write", "; use formats.bdf.write(font, {options})"),
        id => no_setter(id, "write", ""),
    }
}

fn apply<T: serde::de::DeserializeOwned>(
    reg: &mut FontRegistry,
    options: &JsValue,
    setter: impl FnOnce(&mut FontRegistry, T) -> JsResult<()>,
) -> JsResult<()> {
    let value: T = conv::from_js(options)?;
    setter(reg, value)
}

/// Generates the read/write dispatch tables and one `Registry` method per
/// typed setter; each row carries the binding features that compile it.
macro_rules! setters {
    (
        read { $($(#[$rm:meta])* $($rid:literal)|+ => $rset:ident ($rjs:literal, $rts:literal) : $rty:ty),* $(,)? }
        write { $($(#[$wm:meta])* $($wid:literal)|+ => $wset:ident ($wjs:literal, $wts:literal) : $wty:ty),* $(,)? }
        checked_write { $($(#[$cm:meta])* $cid:literal => $cset:ident ($cjs:literal, $cts:literal) : $cty:ty),* $(,)? }
    ) => {
        /// Apply read options (plain data) to `format`'s reader.
        pub fn set_read(reg: &mut FontRegistry, format: &str, options: &JsValue) -> JsResult<()> {
            known(format)?;
            match format {
                $($(#[$rm])* $($rid)|+ => apply(reg, options, |r, v: $rty| {
                    r.$rset(v);
                    Ok(())
                }),)*
                other => {
                    let _ = (reg, options);
                    Err(read_fallback(other))
                }
            }
        }
        /// Apply write options (plain data) to `format`'s writer.
        pub fn set_write(reg: &mut FontRegistry, format: &str, options: &JsValue) -> JsResult<()> {
            known(format)?;
            match format {
                $($(#[$wm])* $($wid)|+ => apply(reg, options, |r, v: $wty| {
                    r.$wset(v);
                    Ok(())
                }),)*
                $($(#[$cm])* $cid => apply(reg, options, |r, v: $cty| r.$cset(v).js()),)*
                other => Err(write_fallback(other)),
            }
        }
        /// Format IDs whose registry readers take typed options in this build.
        pub const READ_IDS: &[&[&str]] = &[$($(#[$rm])* &[$($rid),+],)*];
        /// Format IDs whose registry writers take typed options in this build.
        pub const WRITE_IDS: &[&[&str]] = &[$($(#[$wm])* &[$($wid),+],)* $($(#[$cm])* &[$cid],)*];

        #[wasm_bindgen(js_class = Registry)]
        impl JsRegistry {
            $(
                $(#[$rm])*
                #[doc = concat!("`Registry::", stringify!($rset), "`: apply `", stringify!($rty), "` options (plain data).")]
                #[wasm_bindgen(js_name = $rjs)]
                pub fn $rset(&self, #[wasm_bindgen(unchecked_param_type = $rts)] options: JsValue) -> JsResult<()> {
                    let value: $rty = conv::from_js(&options)?;
                    self.reg_mut()?.$rset(value);
                    Ok(())
                }
            )*
            $(
                $(#[$wm])*
                #[doc = concat!("`Registry::", stringify!($wset), "`: apply `", stringify!($wty), "` options (plain data).")]
                #[wasm_bindgen(js_name = $wjs)]
                pub fn $wset(&self, #[wasm_bindgen(unchecked_param_type = $wts)] options: JsValue) -> JsResult<()> {
                    let value: $wty = conv::from_js(&options)?;
                    self.reg_mut()?.$wset(value);
                    Ok(())
                }
            )*
            $(
                $(#[$cm])*
                #[doc = concat!("`Registry::", stringify!($cset), "`: apply `", stringify!($cty), "` options (plain data); invalid combinations and a missing writer throw.")]
                #[wasm_bindgen(js_name = $cjs)]
                pub fn $cset(&self, #[wasm_bindgen(unchecked_param_type = $cts)] options: JsValue) -> JsResult<()> {
                    let value: $cty = conv::from_js(&options)?;
                    self.reg_mut()?.$cset(value).js()
                }
            )*
        }
    };
}

setters! {
    read {
        #[cfg(feature = "all-formats")]
        "pf2" => set_pf2_read_options("setPf2ReadOptions", "SerdeTypes['formats::pf2::Pf2ReadOptions']"): f::pf2::Pf2ReadOptions,
        #[cfg(feature = "all-formats")]
        "fontx2" => set_fontx2_read_options("setFontx2ReadOptions", "SerdeTypes['formats::fontx2::Fontx2ReadOptions']"): f::fontx2::Fontx2ReadOptions,
        #[cfg(feature = "all-formats")]
        "hzk" => set_hzk_read_options("setHzkReadOptions", "SerdeTypes['formats::hzk::HzkReadOptions']"): f::hzk::HzkReadOptions,
        #[cfg(feature = "all-formats")]
        "raw" => set_raw_read_options("setRawReadOptions", "SerdeTypes['formats::raw::RawReadOptions']"): f::raw::RawReadOptions,
        #[cfg(feature = "all-formats")]
        "cpi" => set_cpi_read_options("setCpiReadOptions", "SerdeTypes['formats::cpi::CpiReadOptions']"): f::cpi::CpiReadOptions,
        #[cfg(feature = "all-formats")]
        "nfnt" => set_nfnt_read_options("setNfntReadOptions", "SerdeTypes['formats::nfnt::NfntReadOptions']"): f::nfnt::NfntReadOptions,
        #[cfg(feature = "all-formats")]
        "bmfont" => set_bmfont_read_options("setBmfontReadOptions", "SerdeTypes['formats::bmfont::BmfontReadOptions']"): f::bmfont::BmfontReadOptions,
        #[cfg(feature = "all-formats")]
        "amiga" => set_amiga_read_options("setAmigaReadOptions", "SerdeTypes['formats::amiga::AmigaReadOptions']"): f::amiga::AmigaReadOptions,
        #[cfg(feature = "all-formats")]
        "gem" => set_gem_read_options("setGemReadOptions", "SerdeTypes['formats::gem::GemReadOptions']"): f::gem::GemReadOptions,
        #[cfg(feature = "all-formats")]
        "snf" => set_snf_read_options("setSnfReadOptions", "SerdeTypes['formats::snf::SnfReadOptions']"): f::snf::SnfReadOptions,
        #[cfg(feature = "all-formats")]
        "svg" => set_svg_read_options("setSvgReadOptions", "SerdeTypes['formats::svg::ReadOptions']"): f::svg::ReadOptions,
        #[cfg(feature = "all-formats")]
        "hex" => set_hex_read_options("setHexReadOptions", "SerdeTypes['formats::hex::HexReadOptions']"): f::hex::HexReadOptions,
        #[cfg(feature = "all-formats")]
        "image" => set_image_read_options("setImageReadOptions", "SerdeTypes['formats::image::ImageReadOptions']"): f::image::ImageReadOptions,
        #[cfg(feature = "all-formats")]
        "fnt" | "fon" => set_fnt_read_options("setFntReadOptions", "SerdeTypes['formats::fnt::FntReadOptions']"): f::fnt::FntReadOptions,
        #[cfg(feature = "all-formats")]
        "c" => set_c_read_options("setCReadOptions", "SerdeTypes['formats::c::CReadOptions']"): f::c::CReadOptions,
        #[cfg(feature = "all-formats")]
        "kbitx" => set_kbitx_read_options("setKbitxReadOptions", "SerdeTypes['formats::kbitx::KbitxReadOptions']"): f::kbitx::KbitxReadOptions,
        #[cfg(feature = "all-formats")]
        "pbf" => set_pbf_read_options("setPbfReadOptions", "SerdeTypes['formats::pbf::PbfReadOptions']"): f::pbf::PbfReadOptions,
        #[cfg(any(feature = "all-formats", feature = "merge"))]
        "ttf" | "otf" | "woff" | "woff2" | "ttc" | "vector-raster" => set_vector_read_options("setVectorReadOptions", "SerdeTypes['formats::vector_raster::Options']"): f::vector_raster::Options,
        #[cfg(feature = "all-formats")]
        "hanfont" => set_hanfont_read_options("setHanfontReadOptions", "SerdeTypes['formats::hanfont::HanfontReadOptions']"): f::hanfont::HanfontReadOptions,
        #[cfg(feature = "all-formats")]
        "iigs" => set_iigs_read_options("setIigsReadOptions", "SerdeTypes['formats::iigs::IigsReadOptions']"): f::iigs::IigsReadOptions,
        #[cfg(feature = "all-formats")]
        "vfont" => set_vfont_read_options("setVfontReadOptions", "SerdeTypes['formats::vfont::VfontReadOptions']"): f::vfont::VfontReadOptions,
        #[cfg(feature = "all-formats")]
        "fzx" => set_fzx_read_options("setFzxReadOptions", "SerdeTypes['formats::fzx::FzxReadOptions']"): f::fzx::FzxReadOptions,
        #[cfg(feature = "all-formats")]
        "figlet" => set_figlet_read_options("setFigletReadOptions", "SerdeTypes['formats::figlet::FigletReadOptions']"): f::figlet::FigletReadOptions,
        #[cfg(feature = "all-formats")]
        "palm" => set_palm_read_options("setPalmReadOptions", "SerdeTypes['formats::palm::PalmReadOptions']"): f::palm::PalmReadOptions,
        #[cfg(feature = "all-formats")]
        "gdr" => set_gdr_read_options("setGdrReadOptions", "SerdeTypes['formats::gdr::GdrReadOptions']"): f::gdr::GdrReadOptions,
        #[cfg(feature = "all-formats")]
        "drcs" => set_drcs_read_options("setDrcsReadOptions", "SerdeTypes['formats::drcs::DrcsReadOptions']"): f::drcs::DrcsReadOptions,
        #[cfg(feature = "all-formats")]
        "dosstart" => set_dosstart_read_options("setDosstartReadOptions", "SerdeTypes['formats::dosstart::DosstartReadOptions']"): f::dosstart::DosstartReadOptions,
        #[cfg(feature = "all-formats")]
        "edwin" => set_edwin_read_options("setEdwinReadOptions", "SerdeTypes['formats::edwin::EdwinReadOptions']"): f::edwin::EdwinReadOptions,
        #[cfg(feature = "all-formats")]
        "mkwinfont" => set_mkwinfont_read_options("setMkwinfontReadOptions", "SerdeTypes['formats::mkwinfont::MkwinfontReadOptions']"): f::mkwinfont::MkwinfontReadOptions,
        #[cfg(feature = "all-formats")]
        "psf2txt" => set_psf2txt_read_options("setPsf2txtReadOptions", "SerdeTypes['formats::psf2txt::Psf2txtReadOptions']"): f::psf2txt::Psf2txtReadOptions,
        #[cfg(feature = "all-formats")]
        "pcr" => set_pcr_read_options("setPcrReadOptions", "SerdeTypes['formats::pcr::PcrReadOptions']"): f::pcr::PcrReadOptions,
        #[cfg(feature = "all-formats")]
        "plus3dos" => set_plus3dos_read_options("setPlus3dosReadOptions", "SerdeTypes['formats::plus3dos::Plus3dosReadOptions']"): f::plus3dos::Plus3dosReadOptions,
        #[cfg(feature = "all-formats")]
        "drhalo" => set_drhalo_read_options("setDrhaloReadOptions", "SerdeTypes['formats::drhalo::DrhaloReadOptions']"): f::drhalo::DrhaloReadOptions,
        #[cfg(feature = "all-formats")]
        "writeon" => set_writeon_read_options("setWriteonReadOptions", "SerdeTypes['formats::writeon::WriteonReadOptions']"): f::writeon::WriteonReadOptions,
        #[cfg(feature = "all-formats")]
        "daisy" => set_daisy_read_options("setDaisyReadOptions", "SerdeTypes['formats::daisy::DaisyReadOptions']"): f::daisy::DaisyReadOptions,
        #[cfg(feature = "all-formats")]
        "polyprint" => set_polyprint_read_options("setPolyprintReadOptions", "SerdeTypes['formats::polyprint::PolyprintReadOptions']"): f::polyprint::PolyprintReadOptions,
        #[cfg(feature = "all-formats")]
        "printshop" => set_printshop_read_options("setPrintshopReadOptions", "SerdeTypes['formats::printshop::PrintshopReadOptions']"): f::printshop::PrintshopReadOptions,
        #[cfg(feature = "all-formats")]
        "tasprint" => set_tasprint_read_options("setTasprintReadOptions", "SerdeTypes['formats::tasprint::TasprintReadOptions']"): f::tasprint::TasprintReadOptions,
        #[cfg(feature = "all-formats")]
        "pxl" => set_pxl_read_options("setPxlReadOptions", "SerdeTypes['formats::pxl::PxlReadOptions']"): f::pxl::PxlReadOptions,
        #[cfg(feature = "all-formats")]
        "signum" => set_signum_read_options("setSignumReadOptions", "SerdeTypes['formats::signum::SignumReadOptions']"): f::signum::SignumReadOptions,
        #[cfg(feature = "all-formats")]
        "rockbox" => set_rockbox_read_options("setRockboxReadOptions", "SerdeTypes['formats::rockbox::RockboxReadOptions']"): f::rockbox::RockboxReadOptions,
        #[cfg(feature = "all-formats")]
        "u8m" => set_u8m_read_options("setU8mReadOptions", "SerdeTypes['formats::u8m::U8mReadOptions']"): f::u8m::U8mReadOptions,
        #[cfg(feature = "all-formats")]
        "cybiko" => set_cybiko_read_options("setCybikoReadOptions", "SerdeTypes['formats::cybiko::CybikoReadOptions']"): f::cybiko::CybikoReadOptions,
        #[cfg(feature = "all-formats")]
        "playdate" => set_playdate_read_options("setPlaydateReadOptions", "SerdeTypes['formats::playdate::PlaydateReadOptions']"): f::playdate::PlaydateReadOptions,
        #[cfg(feature = "all-formats")]
        "hmzk" => set_hmzk_read_options("setHmzkReadOptions", "SerdeTypes['formats::hmzk::HmzkReadOptions']"): f::hmzk::HmzkReadOptions,
        #[cfg(feature = "all-formats")]
        "sbf" => set_sbf_read_options("setSbfReadOptions", "SerdeTypes['formats::sbf::SbfReadOptions']"): f::sbf::SbfReadOptions,
        #[cfg(feature = "all-formats")]
        "mgtk" => set_mgtk_read_options("setMgtkReadOptions", "SerdeTypes['formats::mgtk::MgtkReadOptions']"): f::mgtk::MgtkReadOptions,
        #[cfg(feature = "all-formats")]
        "qpf" => set_qpf_read_options("setQpfReadOptions", "SerdeTypes['formats::qpf::QpfReadOptions']"): f::qpf::QpfReadOptions,
        #[cfg(feature = "all-formats")]
        "symbos" => set_symbos_read_options("setSymbosReadOptions", "SerdeTypes['formats::symbos::SymbosReadOptions']"): f::symbos::SymbosReadOptions,
        #[cfg(feature = "all-formats")]
        "qpf2" => set_qpf2_read_options("setQpf2ReadOptions", "SerdeTypes['formats::qpf2::Qpf2ReadOptions']"): f::qpf2::Qpf2ReadOptions,
        #[cfg(feature = "all-formats")]
        "oberon" => set_oberon_read_options("setOberonReadOptions", "SerdeTypes['formats::oberon::OberonReadOptions']"): f::oberon::OberonReadOptions,
        #[cfg(feature = "all-formats")]
        "fbit" => set_fbit_read_options("setFbitReadOptions", "SerdeTypes['formats::fbit::FbitReadOptions']"): f::fbit::FbitReadOptions,
        #[cfg(feature = "all-formats")]
        "lisa" => set_lisa_read_options("setLisaReadOptions", "SerdeTypes['formats::lisa::LisaReadOptions']"): f::lisa::LisaReadOptions,
        #[cfg(feature = "all-formats")]
        "wsfont" => set_wsfont_read_options("setWsfontReadOptions", "SerdeTypes['formats::wsfont::WsfontReadOptions']"): f::wsfont::WsfontReadOptions,
        #[cfg(feature = "all-formats")]
        "beos" => set_beos_read_options("setBeosReadOptions", "SerdeTypes['formats::beos::BeosReadOptions']"): f::beos::BeosReadOptions,
        #[cfg(feature = "all-formats")]
        "bbc" => set_bbc_read_options("setBbcReadOptions", "SerdeTypes['formats::bbc::BbcReadOptions']"): f::bbc::BbcReadOptions,
        #[cfg(feature = "all-formats")]
        "comfont" => set_comfont_read_options("setComfontReadOptions", "SerdeTypes['formats::comfont::ComfontReadOptions']"): f::comfont::ComfontReadOptions,
        #[cfg(feature = "all-formats")]
        "consoleet" => set_consoleet_read_options("setConsoleetReadOptions", "SerdeTypes['formats::consoleet::ConsoleetReadOptions']"): f::consoleet::ConsoleetReadOptions,
        #[cfg(feature = "all-formats")]
        "grasp" => set_grasp_read_options("setGraspReadOptions", "SerdeTypes['formats::grasp::GraspReadOptions']"): f::grasp::GraspReadOptions,
        #[cfg(feature = "all-formats")]
        "hrcg" => set_hrcg_read_options("setHrcgReadOptions", "SerdeTypes['formats::hrcg::HrcgReadOptions']"): f::hrcg::HrcgReadOptions,
        #[cfg(feature = "all-formats")]
        "wyse" => set_wyse_read_options("setWyseReadOptions", "SerdeTypes['formats::wyse::WyseReadOptions']"): f::wyse::WyseReadOptions,
        #[cfg(feature = "all-formats")]
        "xbin" => set_xbin_read_options("setXbinReadOptions", "SerdeTypes['formats::xbin::XbinReadOptions']"): f::xbin::XbinReadOptions,
        #[cfg(feature = "all-formats")]
        "zapfont" => set_zapfont_read_options("setZapfontReadOptions", "SerdeTypes['formats::zapfont::ZapfontReadOptions']"): f::zapfont::ZapfontReadOptions,
        #[cfg(feature = "all-formats")]
        "catalyst" => set_catalyst_read_options("setCatalystReadOptions", "SerdeTypes['formats::catalyst::CatalystReadOptions']"): f::catalyst::CatalystReadOptions,
        #[cfg(feature = "all-formats")]
        "chiwriter" => set_chiwriter_read_options("setChiwriterReadOptions", "SerdeTypes['formats::chiwriter::ChiwriterReadOptions']"): f::chiwriter::ChiwriterReadOptions,
        #[cfg(feature = "all-formats")]
        "vtfont" => set_vtfont_read_options("setVtfontReadOptions", "SerdeTypes['formats::vtfont::VtfontReadOptions']"): f::vtfont::VtfontReadOptions,
        #[cfg(feature = "all-formats")]
        "hppcl" => set_hppcl_read_options("setHppclReadOptions", "SerdeTypes['formats::hppcl::HppclReadOptions']"): f::hppcl::HppclReadOptions,
        #[cfg(feature = "all-formats")]
        "dashen" => set_dashen_read_options("setDashenReadOptions", "SerdeTypes['formats::dashen::DashenReadOptions']"): f::dashen::DashenReadOptions,
        #[cfg(feature = "all-formats")]
        "vlw" => set_vlw_read_options("setVlwReadOptions", "SerdeTypes['formats::vlw::VlwReadOptions']"): f::vlw::VlwReadOptions,
        #[cfg(feature = "all-formats")]
        "pilfont" => set_pilfont_read_options("setPilfontReadOptions", "SerdeTypes['formats::pilfont::PilfontReadOptions']"): f::pilfont::PilfontReadOptions,
        #[cfg(feature = "all-formats")]
        "prebuilt" => set_prebuilt_read_options("setPrebuiltReadOptions", "SerdeTypes['formats::prebuilt::PrebuiltReadOptions']"): f::prebuilt::PrebuiltReadOptions,
        #[cfg(feature = "all-formats")]
        "geos" => set_geos_read_options("setGeosReadOptions", "SerdeTypes['formats::geos::GeosReadOptions']"): f::geos::GeosReadOptions,
        #[cfg(feature = "all-formats")]
        "gpi" => set_gpi_read_options("setGpiReadOptions", "SerdeTypes['formats::gpi::GpiReadOptions']"): f::gpi::GpiReadOptions,
        #[cfg(feature = "all-formats")]
        "riscos" => set_riscos_read_options("setRiscosReadOptions", "SerdeTypes['formats::riscos::RiscosReadOptions']"): f::riscos::RiscosReadOptions,
        #[cfg(feature = "all-formats")]
        "sfont" => set_sfont_read_options("setSfontReadOptions", "SerdeTypes['formats::sfont::SfontReadOptions']"): f::sfont::SfontReadOptions,
        #[cfg(feature = "all-formats")]
        "pfr" => set_pfr_read_options("setPfrReadOptions", "SerdeTypes['formats::pfr::PfrReadOptions']"): f::pfr::PfrReadOptions,
        #[cfg(feature = "all-formats")]
        "pcgeos" => set_pcgeos_read_options("setPcgeosReadOptions", "SerdeTypes['formats::pcgeos::PcgeosReadOptions']"): f::pcgeos::PcgeosReadOptions,
        #[cfg(feature = "all-formats")]
        "xerox" => set_xerox_read_options("setXeroxReadOptions", "SerdeTypes['formats::xerox::XeroxReadOptions']"): f::xerox::XeroxReadOptions,
        #[cfg(feature = "all-formats")]
        "ssfn" => set_ssfn_read_options("setSsfnReadOptions", "SerdeTypes['formats::ssfn::SsfnReadOptions']"): f::ssfn::SsfnReadOptions,
    }
    write {
        #[cfg(feature = "all-formats")]
        "pf2" => set_pf2_write_options("setPf2WriteOptions", "SerdeTypes['formats::pf2::Pf2WriteOptions']"): f::pf2::Pf2WriteOptions,
        #[cfg(feature = "all-formats")]
        "fontx2" => set_fontx2_write_options("setFontx2WriteOptions", "SerdeTypes['formats::fontx2::Fontx2WriteOptions']"): f::fontx2::Fontx2WriteOptions,
        #[cfg(feature = "all-formats")]
        "hzk" => set_hzk_write_options("setHzkWriteOptions", "SerdeTypes['formats::hzk::HzkWriteOptions']"): f::hzk::HzkWriteOptions,
        #[cfg(feature = "all-formats")]
        "draw" => set_draw_write_options("setDrawWriteOptions", "SerdeTypes['formats::draw::DrawWriteOptions']"): f::draw::DrawWriteOptions,
        #[cfg(feature = "all-formats")]
        "raw" => set_raw_write_options("setRawWriteOptions", "SerdeTypes['formats::raw::RawWriteOptions']"): f::raw::RawWriteOptions,
        #[cfg(feature = "all-formats")]
        "cpi" => set_cpi_write_options("setCpiWriteOptions", "SerdeTypes['formats::cpi::CpiWriteOptions']"): f::cpi::CpiWriteOptions,
        #[cfg(feature = "all-formats")]
        "nfnt" => set_nfnt_write_options("setNfntWriteOptions", "SerdeTypes['formats::nfnt::NfntWriteOptions']"): f::nfnt::NfntWriteOptions,
        #[cfg(feature = "all-formats")]
        "bmfont" => set_bmfont_write_options("setBmfontWriteOptions", "SerdeTypes['formats::bmfont::BmfontWriteOptions']"): f::bmfont::BmfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "sfd" => set_sfd_write_options("setSfdWriteOptions", "SerdeTypes['formats::sfd::SfdWriteOptions']"): f::sfd::SfdWriteOptions,
        #[cfg(feature = "all-formats")]
        "kbits" => set_kbits_write_options("setKbitsWriteOptions", "SerdeTypes['formats::kbits::KbitsWriteOptions']"): f::kbits::KbitsWriteOptions,
        #[cfg(feature = "all-formats")]
        "pk" => set_pk_write_options("setPkWriteOptions", "SerdeTypes['formats::pk::PkWriteOptions']"): f::pk::PkWriteOptions,
        #[cfg(feature = "all-formats")]
        "gf" => set_gf_write_options("setGfWriteOptions", "SerdeTypes['formats::gf::GfWriteOptions']"): f::gf::GfWriteOptions,
        #[cfg(feature = "all-formats")]
        "plan9" => set_plan9_write_options("setPlan9WriteOptions", "SerdeTypes['formats::plan9::Plan9WriteOptions']"): f::plan9::Plan9WriteOptions,
        #[cfg(feature = "all-formats")]
        "amiga" => set_amiga_write_options("setAmigaWriteOptions", "SerdeTypes['formats::amiga::AmigaWriteOptions']"): f::amiga::AmigaWriteOptions,
        #[cfg(feature = "all-formats")]
        "gem" => set_gem_write_options("setGemWriteOptions", "SerdeTypes['formats::gem::GemWriteOptions']"): f::gem::GemWriteOptions,
        #[cfg(feature = "all-formats")]
        "snf" => set_snf_write_options("setSnfWriteOptions", "SerdeTypes['formats::snf::SnfWriteOptions']"): f::snf::SnfWriteOptions,
        #[cfg(any(feature = "all-formats", feature = "merge"))]
        "otb" | "bdat" => set_embedded_write_options("setEmbeddedWriteOptions", "SerdeTypes['formats::embedded::EmbeddedWriteOptions']"): f::embedded::EmbeddedWriteOptions,
        #[cfg(feature = "all-formats")]
        "color" => set_color_write_options("setColorWriteOptions", "SerdeTypes['formats::embedded::ColorWriteOptions']"): f::embedded::ColorWriteOptions,
        #[cfg(any(feature = "all-formats", feature = "merge"))]
        "ttf" | "otf" | "woff" | "woff2" | "ttc" | "vector-raster" => set_vector_write_options("setVectorWriteOptions", "SerdeTypes['formats::vector_raster::WriteOptions']"): f::vector_raster::WriteOptions,
        #[cfg(feature = "all-formats")]
        "svg" => set_svg_write_options("setSvgWriteOptions", "SerdeTypes['formats::svg::WriteOptions']"): f::svg::WriteOptions,
        #[cfg(feature = "all-formats")]
        "hex" => set_hex_write_options("setHexWriteOptions", "SerdeTypes['formats::hex::HexWriteOptions']"): f::hex::HexWriteOptions,
        #[cfg(feature = "all-formats")]
        "psf" => set_psf_write_options("setPsfWriteOptions", "SerdeTypes['formats::psf::PsfWriteOptions']"): f::psf::PsfWriteOptions,
        #[cfg(feature = "all-formats")]
        "image" => set_image_write_options("setImageWriteOptions", "SerdeTypes['formats::image::ImageWriteOptions']"): f::image::ImageWriteOptions,
        #[cfg(feature = "all-formats")]
        "fnt" | "fon" => set_fnt_write_options("setFntWriteOptions", "SerdeTypes['formats::fnt::FntWriteOptions']"): f::fnt::FntWriteOptions,
        #[cfg(feature = "all-formats")]
        "c" => set_c_write_options("setCWriteOptions", "SerdeTypes['formats::c::CWriteOptions']"): f::c::CWriteOptions,
        #[cfg(feature = "all-formats")]
        "kbitx" => set_kbitx_write_options("setKbitxWriteOptions", "SerdeTypes['formats::kbitx::KbitxWriteOptions']"): f::kbitx::KbitxWriteOptions,
        #[cfg(feature = "all-formats")]
        "pbf" => set_pbf_write_options("setPbfWriteOptions", "SerdeTypes['formats::pbf::PbfWriteOptions']"): f::pbf::PbfWriteOptions,
        #[cfg(feature = "all-formats")]
        "hbf" => set_hbf_write_options("setHbfWriteOptions", "SerdeTypes['formats::hbf::HbfWriteOptions']"): f::hbf::HbfWriteOptions,
        #[cfg(feature = "all-formats")]
        "hanfont" => set_hanfont_write_options("setHanfontWriteOptions", "SerdeTypes['formats::hanfont::HanfontWriteOptions']"): f::hanfont::HanfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "iigs" => set_iigs_write_options("setIigsWriteOptions", "SerdeTypes['formats::iigs::IigsWriteOptions']"): f::iigs::IigsWriteOptions,
        #[cfg(feature = "all-formats")]
        "vfont" => set_vfont_write_options("setVfontWriteOptions", "SerdeTypes['formats::vfont::VfontWriteOptions']"): f::vfont::VfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "fzx" => set_fzx_write_options("setFzxWriteOptions", "SerdeTypes['formats::fzx::FzxWriteOptions']"): f::fzx::FzxWriteOptions,
        #[cfg(feature = "all-formats")]
        "figlet" => set_figlet_write_options("setFigletWriteOptions", "SerdeTypes['formats::figlet::FigletWriteOptions']"): f::figlet::FigletWriteOptions,
        #[cfg(feature = "all-formats")]
        "palm" => set_palm_write_options("setPalmWriteOptions", "SerdeTypes['formats::palm::PalmWriteOptions']"): f::palm::PalmWriteOptions,
        #[cfg(feature = "all-formats")]
        "gdr" => set_gdr_write_options("setGdrWriteOptions", "SerdeTypes['formats::gdr::GdrWriteOptions']"): f::gdr::GdrWriteOptions,
        #[cfg(feature = "all-formats")]
        "drcs" => set_drcs_write_options("setDrcsWriteOptions", "SerdeTypes['formats::drcs::DrcsWriteOptions']"): f::drcs::DrcsWriteOptions,
        #[cfg(feature = "all-formats")]
        "dosstart" => set_dosstart_write_options("setDosstartWriteOptions", "SerdeTypes['formats::dosstart::DosstartWriteOptions']"): f::dosstart::DosstartWriteOptions,
        #[cfg(feature = "all-formats")]
        "edwin" => set_edwin_write_options("setEdwinWriteOptions", "SerdeTypes['formats::edwin::EdwinWriteOptions']"): f::edwin::EdwinWriteOptions,
        #[cfg(feature = "all-formats")]
        "mkwinfont" => set_mkwinfont_write_options("setMkwinfontWriteOptions", "SerdeTypes['formats::mkwinfont::MkwinfontWriteOptions']"): f::mkwinfont::MkwinfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "psf2txt" => set_psf2txt_write_options("setPsf2txtWriteOptions", "SerdeTypes['formats::psf2txt::Psf2txtWriteOptions']"): f::psf2txt::Psf2txtWriteOptions,
        #[cfg(feature = "all-formats")]
        "pcr" => set_pcr_write_options("setPcrWriteOptions", "SerdeTypes['formats::pcr::PcrWriteOptions']"): f::pcr::PcrWriteOptions,
        #[cfg(feature = "all-formats")]
        "plus3dos" => set_plus3dos_write_options("setPlus3dosWriteOptions", "SerdeTypes['formats::plus3dos::Plus3dosWriteOptions']"): f::plus3dos::Plus3dosWriteOptions,
        #[cfg(feature = "all-formats")]
        "drhalo" => set_drhalo_write_options("setDrhaloWriteOptions", "SerdeTypes['formats::drhalo::DrhaloWriteOptions']"): f::drhalo::DrhaloWriteOptions,
        #[cfg(feature = "all-formats")]
        "writeon" => set_writeon_write_options("setWriteonWriteOptions", "SerdeTypes['formats::writeon::WriteonWriteOptions']"): f::writeon::WriteonWriteOptions,
        #[cfg(feature = "all-formats")]
        "daisy" => set_daisy_write_options("setDaisyWriteOptions", "SerdeTypes['formats::daisy::DaisyWriteOptions']"): f::daisy::DaisyWriteOptions,
        #[cfg(feature = "all-formats")]
        "polyprint" => set_polyprint_write_options("setPolyprintWriteOptions", "SerdeTypes['formats::polyprint::PolyprintWriteOptions']"): f::polyprint::PolyprintWriteOptions,
        #[cfg(feature = "all-formats")]
        "printshop" => set_printshop_write_options("setPrintshopWriteOptions", "SerdeTypes['formats::printshop::PrintshopWriteOptions']"): f::printshop::PrintshopWriteOptions,
        #[cfg(feature = "all-formats")]
        "tasprint" => set_tasprint_write_options("setTasprintWriteOptions", "SerdeTypes['formats::tasprint::TasprintWriteOptions']"): f::tasprint::TasprintWriteOptions,
        #[cfg(feature = "all-formats")]
        "pxl" => set_pxl_write_options("setPxlWriteOptions", "SerdeTypes['formats::pxl::PxlWriteOptions']"): f::pxl::PxlWriteOptions,
        #[cfg(feature = "all-formats")]
        "signum" => set_signum_write_options("setSignumWriteOptions", "SerdeTypes['formats::signum::SignumWriteOptions']"): f::signum::SignumWriteOptions,
        #[cfg(feature = "all-formats")]
        "rockbox" => set_rockbox_write_options("setRockboxWriteOptions", "SerdeTypes['formats::rockbox::RockboxWriteOptions']"): f::rockbox::RockboxWriteOptions,
        #[cfg(feature = "all-formats")]
        "u8m" => set_u8m_write_options("setU8mWriteOptions", "SerdeTypes['formats::u8m::U8mWriteOptions']"): f::u8m::U8mWriteOptions,
        #[cfg(feature = "all-formats")]
        "cybiko" => set_cybiko_write_options("setCybikoWriteOptions", "SerdeTypes['formats::cybiko::CybikoWriteOptions']"): f::cybiko::CybikoWriteOptions,
        #[cfg(feature = "all-formats")]
        "playdate" => set_playdate_write_options("setPlaydateWriteOptions", "SerdeTypes['formats::playdate::PlaydateWriteOptions']"): f::playdate::PlaydateWriteOptions,
        #[cfg(feature = "all-formats")]
        "hmzk" => set_hmzk_write_options("setHmzkWriteOptions", "SerdeTypes['formats::hmzk::HmzkWriteOptions']"): f::hmzk::HmzkWriteOptions,
        #[cfg(feature = "all-formats")]
        "sbf" => set_sbf_write_options("setSbfWriteOptions", "SerdeTypes['formats::sbf::SbfWriteOptions']"): f::sbf::SbfWriteOptions,
        #[cfg(feature = "all-formats")]
        "mgtk" => set_mgtk_write_options("setMgtkWriteOptions", "SerdeTypes['formats::mgtk::MgtkWriteOptions']"): f::mgtk::MgtkWriteOptions,
        #[cfg(feature = "all-formats")]
        "qpf" => set_qpf_write_options("setQpfWriteOptions", "SerdeTypes['formats::qpf::QpfWriteOptions']"): f::qpf::QpfWriteOptions,
        #[cfg(feature = "all-formats")]
        "symbos" => set_symbos_write_options("setSymbosWriteOptions", "SerdeTypes['formats::symbos::SymbosWriteOptions']"): f::symbos::SymbosWriteOptions,
        #[cfg(feature = "all-formats")]
        "qpf2" => set_qpf2_write_options("setQpf2WriteOptions", "SerdeTypes['formats::qpf2::Qpf2WriteOptions']"): f::qpf2::Qpf2WriteOptions,
        #[cfg(feature = "all-formats")]
        "oberon" => set_oberon_write_options("setOberonWriteOptions", "SerdeTypes['formats::oberon::OberonWriteOptions']"): f::oberon::OberonWriteOptions,
        #[cfg(feature = "all-formats")]
        "fbit" => set_fbit_write_options("setFbitWriteOptions", "SerdeTypes['formats::fbit::FbitWriteOptions']"): f::fbit::FbitWriteOptions,
        #[cfg(feature = "all-formats")]
        "lisa" => set_lisa_write_options("setLisaWriteOptions", "SerdeTypes['formats::lisa::LisaWriteOptions']"): f::lisa::LisaWriteOptions,
        #[cfg(feature = "all-formats")]
        "wsfont" => set_wsfont_write_options("setWsfontWriteOptions", "SerdeTypes['formats::wsfont::WsfontWriteOptions']"): f::wsfont::WsfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "beos" => set_beos_write_options("setBeosWriteOptions", "SerdeTypes['formats::beos::BeosWriteOptions']"): f::beos::BeosWriteOptions,
        #[cfg(feature = "all-formats")]
        "bbc" => set_bbc_write_options("setBbcWriteOptions", "SerdeTypes['formats::bbc::BbcWriteOptions']"): f::bbc::BbcWriteOptions,
        #[cfg(feature = "all-formats")]
        "comfont" => set_comfont_write_options("setComfontWriteOptions", "SerdeTypes['formats::comfont::ComfontWriteOptions']"): f::comfont::ComfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "consoleet" => set_consoleet_write_options("setConsoleetWriteOptions", "SerdeTypes['formats::consoleet::ConsoleetWriteOptions']"): f::consoleet::ConsoleetWriteOptions,
        #[cfg(feature = "all-formats")]
        "grasp" => set_grasp_write_options("setGraspWriteOptions", "SerdeTypes['formats::grasp::GraspWriteOptions']"): f::grasp::GraspWriteOptions,
        #[cfg(feature = "all-formats")]
        "hrcg" => set_hrcg_write_options("setHrcgWriteOptions", "SerdeTypes['formats::hrcg::HrcgWriteOptions']"): f::hrcg::HrcgWriteOptions,
        #[cfg(feature = "all-formats")]
        "wyse" => set_wyse_write_options("setWyseWriteOptions", "SerdeTypes['formats::wyse::WyseWriteOptions']"): f::wyse::WyseWriteOptions,
        #[cfg(feature = "all-formats")]
        "xbin" => set_xbin_write_options("setXbinWriteOptions", "SerdeTypes['formats::xbin::XbinWriteOptions']"): f::xbin::XbinWriteOptions,
        #[cfg(feature = "all-formats")]
        "zapfont" => set_zapfont_write_options("setZapfontWriteOptions", "SerdeTypes['formats::zapfont::ZapfontWriteOptions']"): f::zapfont::ZapfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "catalyst" => set_catalyst_write_options("setCatalystWriteOptions", "SerdeTypes['formats::catalyst::CatalystWriteOptions']"): f::catalyst::CatalystWriteOptions,
        #[cfg(feature = "all-formats")]
        "chiwriter" => set_chiwriter_write_options("setChiwriterWriteOptions", "SerdeTypes['formats::chiwriter::ChiwriterWriteOptions']"): f::chiwriter::ChiwriterWriteOptions,
        #[cfg(feature = "all-formats")]
        "vtfont" => set_vtfont_write_options("setVtfontWriteOptions", "SerdeTypes['formats::vtfont::VtfontWriteOptions']"): f::vtfont::VtfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "hppcl" => set_hppcl_write_options("setHppclWriteOptions", "SerdeTypes['formats::hppcl::HppclWriteOptions']"): f::hppcl::HppclWriteOptions,
        #[cfg(feature = "all-formats")]
        "dashen" => set_dashen_write_options("setDashenWriteOptions", "SerdeTypes['formats::dashen::DashenWriteOptions']"): f::dashen::DashenWriteOptions,
        #[cfg(feature = "all-formats")]
        "vlw" => set_vlw_write_options("setVlwWriteOptions", "SerdeTypes['formats::vlw::VlwWriteOptions']"): f::vlw::VlwWriteOptions,
        #[cfg(feature = "all-formats")]
        "pilfont" => set_pilfont_write_options("setPilfontWriteOptions", "SerdeTypes['formats::pilfont::PilfontWriteOptions']"): f::pilfont::PilfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "prebuilt" => set_prebuilt_write_options("setPrebuiltWriteOptions", "SerdeTypes['formats::prebuilt::PrebuiltWriteOptions']"): f::prebuilt::PrebuiltWriteOptions,
        #[cfg(feature = "all-formats")]
        "geos" => set_geos_write_options("setGeosWriteOptions", "SerdeTypes['formats::geos::GeosWriteOptions']"): f::geos::GeosWriteOptions,
        #[cfg(feature = "all-formats")]
        "gpi" => set_gpi_write_options("setGpiWriteOptions", "SerdeTypes['formats::gpi::GpiWriteOptions']"): f::gpi::GpiWriteOptions,
        #[cfg(feature = "all-formats")]
        "riscos" => set_riscos_write_options("setRiscosWriteOptions", "SerdeTypes['formats::riscos::RiscosWriteOptions']"): f::riscos::RiscosWriteOptions,
        #[cfg(feature = "all-formats")]
        "sfont" => set_sfont_write_options("setSfontWriteOptions", "SerdeTypes['formats::sfont::SfontWriteOptions']"): f::sfont::SfontWriteOptions,
        #[cfg(feature = "all-formats")]
        "pfr" => set_pfr_write_options("setPfrWriteOptions", "SerdeTypes['formats::pfr::PfrWriteOptions']"): f::pfr::PfrWriteOptions,
        #[cfg(feature = "all-formats")]
        "pcgeos" => set_pcgeos_write_options("setPcgeosWriteOptions", "SerdeTypes['formats::pcgeos::PcgeosWriteOptions']"): f::pcgeos::PcgeosWriteOptions,
        #[cfg(feature = "all-formats")]
        "xerox" => set_xerox_write_options("setXeroxWriteOptions", "SerdeTypes['formats::xerox::XeroxWriteOptions']"): f::xerox::XeroxWriteOptions,
        #[cfg(feature = "all-formats")]
        "ssfn" => set_ssfn_write_options("setSsfnWriteOptions", "SerdeTypes['formats::ssfn::SsfnWriteOptions']"): f::ssfn::SsfnWriteOptions,
    }
    checked_write {
        #[cfg(any(feature = "all-formats", feature = "minimal", feature = "merge"))]
        "pcf" => set_pcf_write_options("setPcfWriteOptions", "SerdeTypes['formats::pcf::PcfWriteOptions']"): f::pcf::PcfWriteOptions,
        #[cfg(any(feature = "all-formats", feature = "minimal"))]
        "yaff" => set_yaff_write_options("setYaffWriteOptions", "SerdeTypes['formats::yaff::YaffWriteOptions']"): f::yaff::YaffWriteOptions,
    }
}

/// Apply a codec (`{read_options, write_options}`, the format's `*Codec`
/// struct); missing sides keep their current configuration.
pub fn set_codec(reg: &mut FontRegistry, format: &str, codec: &JsValue) -> JsResult<()> {
    let object = codec
        .dyn_ref::<Object>()
        .filter(|o| !js_sys::Array::is_array(o))
        .ok_or_else(|| invalid("a codec is a plain object {read_options, write_options}"))?;
    for key in Object::keys(object).iter() {
        let name = key.as_string().unwrap_or_default();
        let value = Reflect::get(object, &key)?;
        if value.is_undefined() {
            continue;
        }
        match name.as_str() {
            "read_options" => set_read(reg, format, &value)?,
            "write_options" => set_write(reg, format, &value)?,
            other => {
                return Err(invalid(format!(
                    "unknown codec field `{other}`, expected `read_options` or `write_options`"
                )));
            }
        }
    }
    Ok(())
}

#[wasm_bindgen(js_class = Registry)]
impl JsRegistry {
    /// Configure the typed reader options of `format` (the format's
    /// `*ReadOptions` fields; unknown keys throw `InvalidInputError`).
    #[wasm_bindgen(js_name = setReadOptions)]
    pub fn set_read_options(
        &self,
        format: &str,
        #[wasm_bindgen(unchecked_param_type = "object")] options: JsValue,
    ) -> JsResult<()> {
        set_read(&mut *self.reg_mut()?, format, &options)
    }

    /// Configure the typed writer options of `format` (the format's
    /// `*WriteOptions` fields; unknown keys throw `InvalidInputError`).
    #[wasm_bindgen(js_name = setWriteOptions)]
    pub fn set_write_options(
        &self,
        format: &str,
        #[wasm_bindgen(unchecked_param_type = "object")] options: JsValue,
    ) -> JsResult<()> {
        set_write(&mut *self.reg_mut()?, format, &options)
    }

    /// Configure both sides of `format`; an absent side stays unchanged.
    #[wasm_bindgen(js_name = setOptions)]
    pub fn set_options(
        &self,
        format: &str,
        #[wasm_bindgen(unchecked_param_type = "{ read?: object; write?: object }")]
        options: JsValue,
    ) -> JsResult<()> {
        let options = conv::Options::new(&options, &["read", "write"], "setOptions options")?;
        let mut reg = self.reg_mut()?;
        if let Some(read) = options.get("read")? {
            set_read(&mut reg, format, &read)?;
        }
        if let Some(write) = options.get("write")? {
            set_write(&mut reg, format, &write)?;
        }
        Ok(())
    }

    /// Install a codec from its plain form (`{read_options, write_options}`,
    /// the format's `*Codec` struct); missing sides keep their configuration.
    #[wasm_bindgen(js_name = setCodec)]
    pub fn set_codec(
        &self,
        format: &str,
        #[wasm_bindgen(unchecked_param_type = "{ read_options?: object; write_options?: object }")]
        codec: JsValue,
    ) -> JsResult<()> {
        set_codec(&mut *self.reg_mut()?, format, &codec)
    }
}

/// Format IDs whose registry readers / writers take typed options in this
/// build: `{read: [...], write: [...]}`.
#[wasm_bindgen(js_namespace = formats, js_name = configurableFormats, unchecked_return_type = "{ read: string[]; write: string[] }")]
pub fn configurable_formats() -> JsResult<JsValue> {
    let read: Vec<&str> = READ_IDS
        .iter()
        .flat_map(|ids| ids.iter().copied())
        .filter(|id| {
            fontpixel::formats::catalogue::CATALOGUE
                .iter()
                .any(|f| f.id == *id && f.feature_enabled)
        })
        .collect();
    let write: Vec<&str> = WRITE_IDS
        .iter()
        .flat_map(|ids| ids.iter().copied())
        .filter(|id| {
            fontpixel::formats::catalogue::CATALOGUE
                .iter()
                .any(|f| f.id == *id && f.feature_enabled)
        })
        .collect();
    let out = Object::new();
    Reflect::set(&out, &JsValue::from_str("read"), &conv::to_js(&read)?)?;
    Reflect::set(&out, &JsValue::from_str("write"), &conv::to_js(&write)?)?;
    Ok(out.into())
}

/// `formats.normalize(typeName, value)`: the top-level `normalize`, under
/// the Python spelling `fontpixel.formats.normalize`.
#[wasm_bindgen(js_namespace = formats, js_name = normalize, unchecked_return_type = "unknown")]
pub fn formats_normalize(
    #[wasm_bindgen(js_name = typeName)] type_name: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "unknown")] value: Option<JsValue>,
) -> JsResult<JsValue> {
    crate::normalize::normalize(type_name, value)
}

/// `formats.optionTypes()`: every type name `normalize` accepts in this
/// build (the top-level `optionTypes`).
#[wasm_bindgen(js_namespace = formats, js_name = optionTypes)]
pub fn formats_option_types() -> Vec<String> {
    crate::normalize::option_types()
}
