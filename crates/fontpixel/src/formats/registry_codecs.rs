//! Common registration for codecs with independent typed read/write options.

#[cfg(any(
    feature = "hanfont",
    feature = "iigs",
    feature = "vfont",
    feature = "fzx",
    feature = "figlet",
    feature = "palm",
    feature = "gdr",
    feature = "drcs",
    feature = "dosstart",
    feature = "edwin",
    feature = "mkwinfont",
    feature = "psf2txt",
    feature = "pcr",
    feature = "plus3dos",
    feature = "drhalo",
    feature = "writeon",
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "pxl",
    feature = "signum",
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf",
    feature = "mgtk",
    feature = "qpf",
    feature = "symbos",
    feature = "qpf2",
    feature = "oberon",
    feature = "fbit",
    feature = "lisa",
    feature = "wsfont",
    feature = "beos",
    feature = "bbc",
    feature = "comfont",
    feature = "consoleet",
    feature = "grasp",
    feature = "hrcg",
    feature = "wyse",
    feature = "xbin",
    feature = "zapfont",
    feature = "catalyst",
    feature = "chiwriter",
    feature = "vtfont",
    feature = "hppcl",
    feature = "dashen",
    feature = "vlw",
    feature = "pilfont",
    feature = "prebuilt",
    feature = "geos",
    feature = "gpi",
    feature = "riscos",
    feature = "sfont",
    feature = "pfr",
    feature = "pcgeos",
    feature = "xerox",
    feature = "ssfn"
))]
macro_rules! register_codec {
    ($module:ident, $codec:ident, $read:ident, $write:ident,
     $install:ident, $set_read:ident, $set_write:ident, $detect:expr) => {
        impl super::Registry<crate::FontSet> {
            pub(crate) fn $install(&mut self) {
                if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == stringify!($module)) {
                    if $detect {
                        entry.detector = Some(Box::new(super::$module::$codec::default()));
                    }
                    entry.reader = Some(Box::new(super::$module::$codec::default()));
                    entry.writer = Some(super::transport::writer(super::$module::$codec::default()));
                }
            }
            #[doc = concat!("Configure the ", stringify!($module), " reader and its structural detector.")]
            pub fn $set_read(&mut self, options: super::$module::$read) {
                if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == stringify!($module) && e.reader.is_some()) {
                    if entry.detector.is_some() {
                        entry.detector = Some(Box::new(super::$module::$codec {
                            read_options: options.clone(),
                            ..Default::default()
                        }));
                    }
                    entry.reader = Some(Box::new(super::$module::$codec {
                        read_options: options,
                        ..Default::default()
                    }));
                }
            }
            #[doc = concat!("Configure native ", stringify!($module), " output.")]
            pub fn $set_write(&mut self, options: super::$module::$write) {
                if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == stringify!($module) && e.writer.is_some()) {
                    entry.writer = Some(super::transport::writer(super::$module::$codec {
                        write_options: options,
                        ..Default::default()
                    }));
                }
            }
        }
    };
}

#[cfg(feature = "hanfont")]
register_codec!(
    hanfont,
    HanfontCodec,
    HanfontReadOptions,
    HanfontWriteOptions,
    install_hanfont,
    set_hanfont_read_options,
    set_hanfont_write_options,
    false
);

#[cfg(feature = "iigs")]
register_codec!(
    iigs,
    IigsCodec,
    IigsReadOptions,
    IigsWriteOptions,
    install_iigs,
    set_iigs_read_options,
    set_iigs_write_options,
    true
);

#[cfg(feature = "color")]
impl super::Registry<crate::FontSet> {
    /// Configure CBDT/CBLC or sbix bitmap output and its resource budgets.
    pub fn set_color_write_options(&mut self, options: super::embedded::ColorWriteOptions) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "color" && e.writer.is_some())
        {
            let mut codec = super::embedded::EmbeddedCodec::new(crate::sfnt::bitmap::Kind::Color);
            codec.color_write_options = options;
            entry.writer = Some(super::transport::writer(codec));
        }
    }
}

#[cfg(feature = "vfont")]
register_codec!(
    vfont,
    VfontCodec,
    VfontReadOptions,
    VfontWriteOptions,
    install_vfont,
    set_vfont_read_options,
    set_vfont_write_options,
    true
);

#[cfg(feature = "fzx")]
register_codec!(
    fzx,
    FzxCodec,
    FzxReadOptions,
    FzxWriteOptions,
    install_fzx,
    set_fzx_read_options,
    set_fzx_write_options,
    true
);

#[cfg(feature = "figlet")]
register_codec!(
    figlet,
    FigletCodec,
    FigletReadOptions,
    FigletWriteOptions,
    install_figlet,
    set_figlet_read_options,
    set_figlet_write_options,
    true
);

#[cfg(feature = "palm")]
register_codec!(
    palm,
    PalmCodec,
    PalmReadOptions,
    PalmWriteOptions,
    install_palm,
    set_palm_read_options,
    set_palm_write_options,
    true
);

#[cfg(feature = "gdr")]
register_codec!(
    gdr,
    GdrCodec,
    GdrReadOptions,
    GdrWriteOptions,
    install_gdr,
    set_gdr_read_options,
    set_gdr_write_options,
    true
);

#[cfg(feature = "drcs")]
register_codec!(
    drcs,
    DrcsCodec,
    DrcsReadOptions,
    DrcsWriteOptions,
    install_drcs,
    set_drcs_read_options,
    set_drcs_write_options,
    true
);

#[cfg(feature = "dosstart")]
register_codec!(
    dosstart,
    DosstartCodec,
    DosstartReadOptions,
    DosstartWriteOptions,
    install_dosstart,
    set_dosstart_read_options,
    set_dosstart_write_options,
    true
);

#[cfg(feature = "edwin")]
register_codec!(
    edwin,
    EdwinCodec,
    EdwinReadOptions,
    EdwinWriteOptions,
    install_edwin,
    set_edwin_read_options,
    set_edwin_write_options,
    false
);

#[cfg(feature = "mkwinfont")]
register_codec!(
    mkwinfont,
    MkwinfontCodec,
    MkwinfontReadOptions,
    MkwinfontWriteOptions,
    install_mkwinfont,
    set_mkwinfont_read_options,
    set_mkwinfont_write_options,
    false
);

#[cfg(feature = "psf2txt")]
register_codec!(
    psf2txt,
    Psf2txtCodec,
    Psf2txtReadOptions,
    Psf2txtWriteOptions,
    install_psf2txt,
    set_psf2txt_read_options,
    set_psf2txt_write_options,
    true
);

#[cfg(feature = "pcr")]
register_codec!(
    pcr,
    PcrCodec,
    PcrReadOptions,
    PcrWriteOptions,
    install_pcr,
    set_pcr_read_options,
    set_pcr_write_options,
    true
);

#[cfg(feature = "plus3dos")]
register_codec!(
    plus3dos,
    Plus3dosCodec,
    Plus3dosReadOptions,
    Plus3dosWriteOptions,
    install_plus3dos,
    set_plus3dos_read_options,
    set_plus3dos_write_options,
    true
);

#[cfg(feature = "drhalo")]
register_codec!(
    drhalo,
    DrhaloCodec,
    DrhaloReadOptions,
    DrhaloWriteOptions,
    install_drhalo,
    set_drhalo_read_options,
    set_drhalo_write_options,
    true
);

#[cfg(feature = "writeon")]
register_codec!(
    writeon,
    WriteonCodec,
    WriteonReadOptions,
    WriteonWriteOptions,
    install_writeon,
    set_writeon_read_options,
    set_writeon_write_options,
    true
);

#[cfg(feature = "daisy")]
register_codec!(
    daisy,
    DaisyCodec,
    DaisyReadOptions,
    DaisyWriteOptions,
    install_daisy,
    set_daisy_read_options,
    set_daisy_write_options,
    true
);

#[cfg(feature = "polyprint")]
register_codec!(
    polyprint,
    PolyprintCodec,
    PolyprintReadOptions,
    PolyprintWriteOptions,
    install_polyprint,
    set_polyprint_read_options,
    set_polyprint_write_options,
    false
);

#[cfg(feature = "printshop")]
register_codec!(
    printshop,
    PrintshopCodec,
    PrintshopReadOptions,
    PrintshopWriteOptions,
    install_printshop,
    set_printshop_read_options,
    set_printshop_write_options,
    true
);

#[cfg(feature = "tasprint")]
register_codec!(
    tasprint,
    TasprintCodec,
    TasprintReadOptions,
    TasprintWriteOptions,
    install_tasprint,
    set_tasprint_read_options,
    set_tasprint_write_options,
    true
);

#[cfg(feature = "pxl")]
register_codec!(
    pxl,
    PxlCodec,
    PxlReadOptions,
    PxlWriteOptions,
    install_pxl,
    set_pxl_read_options,
    set_pxl_write_options,
    true
);

#[cfg(feature = "signum")]
register_codec!(
    signum,
    SignumCodec,
    SignumReadOptions,
    SignumWriteOptions,
    install_signum,
    set_signum_read_options,
    set_signum_write_options,
    true
);

#[cfg(feature = "rockbox")]
register_codec!(
    rockbox,
    RockboxCodec,
    RockboxReadOptions,
    RockboxWriteOptions,
    install_rockbox,
    set_rockbox_read_options,
    set_rockbox_write_options,
    true
);

#[cfg(feature = "u8m")]
register_codec!(
    u8m,
    U8mCodec,
    U8mReadOptions,
    U8mWriteOptions,
    install_u8m,
    set_u8m_read_options,
    set_u8m_write_options,
    true
);

#[cfg(feature = "cybiko")]
register_codec!(
    cybiko,
    CybikoCodec,
    CybikoReadOptions,
    CybikoWriteOptions,
    install_cybiko,
    set_cybiko_read_options,
    set_cybiko_write_options,
    true
);

#[cfg(feature = "playdate")]
register_codec!(
    playdate,
    PlaydateCodec,
    PlaydateReadOptions,
    PlaydateWriteOptions,
    install_playdate,
    set_playdate_read_options,
    set_playdate_write_options,
    true
);

#[cfg(feature = "hmzk")]
register_codec!(
    hmzk,
    HmzkCodec,
    HmzkReadOptions,
    HmzkWriteOptions,
    install_hmzk,
    set_hmzk_read_options,
    set_hmzk_write_options,
    true
);

#[cfg(feature = "sbf")]
register_codec!(
    sbf,
    SbfCodec,
    SbfReadOptions,
    SbfWriteOptions,
    install_sbf,
    set_sbf_read_options,
    set_sbf_write_options,
    true
);

#[cfg(feature = "mgtk")]
register_codec!(
    mgtk,
    MgtkCodec,
    MgtkReadOptions,
    MgtkWriteOptions,
    install_mgtk,
    set_mgtk_read_options,
    set_mgtk_write_options,
    true
);

#[cfg(feature = "qpf")]
register_codec!(
    qpf,
    QpfCodec,
    QpfReadOptions,
    QpfWriteOptions,
    install_qpf,
    set_qpf_read_options,
    set_qpf_write_options,
    true
);

#[cfg(feature = "symbos")]
register_codec!(
    symbos,
    SymbosCodec,
    SymbosReadOptions,
    SymbosWriteOptions,
    install_symbos,
    set_symbos_read_options,
    set_symbos_write_options,
    true
);

#[cfg(feature = "qpf2")]
register_codec!(
    qpf2,
    Qpf2Codec,
    Qpf2ReadOptions,
    Qpf2WriteOptions,
    install_qpf2,
    set_qpf2_read_options,
    set_qpf2_write_options,
    true
);

#[cfg(feature = "oberon")]
register_codec!(
    oberon,
    OberonCodec,
    OberonReadOptions,
    OberonWriteOptions,
    install_oberon,
    set_oberon_read_options,
    set_oberon_write_options,
    true
);

#[cfg(feature = "fbit")]
register_codec!(
    fbit,
    FbitCodec,
    FbitReadOptions,
    FbitWriteOptions,
    install_fbit,
    set_fbit_read_options,
    set_fbit_write_options,
    true
);

#[cfg(feature = "lisa")]
register_codec!(
    lisa,
    LisaCodec,
    LisaReadOptions,
    LisaWriteOptions,
    install_lisa,
    set_lisa_read_options,
    set_lisa_write_options,
    true
);

#[cfg(feature = "wsfont")]
register_codec!(
    wsfont,
    WsfontCodec,
    WsfontReadOptions,
    WsfontWriteOptions,
    install_wsfont,
    set_wsfont_read_options,
    set_wsfont_write_options,
    true
);

#[cfg(feature = "beos")]
register_codec!(
    beos,
    BeosCodec,
    BeosReadOptions,
    BeosWriteOptions,
    install_beos,
    set_beos_read_options,
    set_beos_write_options,
    true
);

#[cfg(feature = "bbc")]
register_codec!(
    bbc,
    BbcCodec,
    BbcReadOptions,
    BbcWriteOptions,
    install_bbc,
    set_bbc_read_options,
    set_bbc_write_options,
    true
);

#[cfg(feature = "comfont")]
register_codec!(
    comfont,
    ComfontCodec,
    ComfontReadOptions,
    ComfontWriteOptions,
    install_comfont,
    set_comfont_read_options,
    set_comfont_write_options,
    true
);

#[cfg(feature = "consoleet")]
register_codec!(
    consoleet,
    ConsoleetCodec,
    ConsoleetReadOptions,
    ConsoleetWriteOptions,
    install_consoleet,
    set_consoleet_read_options,
    set_consoleet_write_options,
    false
);

#[cfg(feature = "grasp")]
register_codec!(
    grasp,
    GraspCodec,
    GraspReadOptions,
    GraspWriteOptions,
    install_grasp,
    set_grasp_read_options,
    set_grasp_write_options,
    true
);

#[cfg(feature = "hrcg")]
register_codec!(
    hrcg,
    HrcgCodec,
    HrcgReadOptions,
    HrcgWriteOptions,
    install_hrcg,
    set_hrcg_read_options,
    set_hrcg_write_options,
    false
);

#[cfg(feature = "wyse")]
register_codec!(
    wyse,
    WyseCodec,
    WyseReadOptions,
    WyseWriteOptions,
    install_wyse,
    set_wyse_read_options,
    set_wyse_write_options,
    true
);

#[cfg(feature = "xbin")]
register_codec!(
    xbin,
    XbinCodec,
    XbinReadOptions,
    XbinWriteOptions,
    install_xbin,
    set_xbin_read_options,
    set_xbin_write_options,
    true
);

#[cfg(feature = "zapfont")]
register_codec!(
    zapfont,
    ZapfontCodec,
    ZapfontReadOptions,
    ZapfontWriteOptions,
    install_zapfont,
    set_zapfont_read_options,
    set_zapfont_write_options,
    true
);

#[cfg(feature = "catalyst")]
register_codec!(
    catalyst,
    CatalystCodec,
    CatalystReadOptions,
    CatalystWriteOptions,
    install_catalyst,
    set_catalyst_read_options,
    set_catalyst_write_options,
    false
);

#[cfg(feature = "chiwriter")]
register_codec!(
    chiwriter,
    ChiwriterCodec,
    ChiwriterReadOptions,
    ChiwriterWriteOptions,
    install_chiwriter,
    set_chiwriter_read_options,
    set_chiwriter_write_options,
    true
);

#[cfg(feature = "vtfont")]
register_codec!(
    vtfont,
    VtfontCodec,
    VtfontReadOptions,
    VtfontWriteOptions,
    install_vtfont,
    set_vtfont_read_options,
    set_vtfont_write_options,
    true
);

#[cfg(feature = "hppcl")]
register_codec!(
    hppcl,
    HppclCodec,
    HppclReadOptions,
    HppclWriteOptions,
    install_hppcl,
    set_hppcl_read_options,
    set_hppcl_write_options,
    true
);

#[cfg(feature = "dashen")]
register_codec!(
    dashen,
    DashenCodec,
    DashenReadOptions,
    DashenWriteOptions,
    install_dashen,
    set_dashen_read_options,
    set_dashen_write_options,
    true
);

#[cfg(feature = "vlw")]
register_codec!(
    vlw,
    VlwCodec,
    VlwReadOptions,
    VlwWriteOptions,
    install_vlw,
    set_vlw_read_options,
    set_vlw_write_options,
    true
);

#[cfg(feature = "pilfont")]
register_codec!(
    pilfont,
    PilfontCodec,
    PilfontReadOptions,
    PilfontWriteOptions,
    install_pilfont,
    set_pilfont_read_options,
    set_pilfont_write_options,
    true
);

#[cfg(feature = "prebuilt")]
register_codec!(
    prebuilt,
    PrebuiltCodec,
    PrebuiltReadOptions,
    PrebuiltWriteOptions,
    install_prebuilt,
    set_prebuilt_read_options,
    set_prebuilt_write_options,
    true
);

#[cfg(feature = "geos")]
register_codec!(
    geos,
    GeosCodec,
    GeosReadOptions,
    GeosWriteOptions,
    install_geos,
    set_geos_read_options,
    set_geos_write_options,
    true
);

#[cfg(feature = "gpi")]
register_codec!(
    gpi,
    GpiCodec,
    GpiReadOptions,
    GpiWriteOptions,
    install_gpi,
    set_gpi_read_options,
    set_gpi_write_options,
    true
);

#[cfg(feature = "riscos")]
register_codec!(
    riscos,
    RiscosCodec,
    RiscosReadOptions,
    RiscosWriteOptions,
    install_riscos,
    set_riscos_read_options,
    set_riscos_write_options,
    true
);

#[cfg(feature = "sfont")]
register_codec!(
    sfont,
    SfontCodec,
    SfontReadOptions,
    SfontWriteOptions,
    install_sfont,
    set_sfont_read_options,
    set_sfont_write_options,
    true
);

#[cfg(feature = "pfr")]
register_codec!(
    pfr,
    PfrCodec,
    PfrReadOptions,
    PfrWriteOptions,
    install_pfr,
    set_pfr_read_options,
    set_pfr_write_options,
    true
);

#[cfg(feature = "pcgeos")]
register_codec!(
    pcgeos,
    PcgeosCodec,
    PcgeosReadOptions,
    PcgeosWriteOptions,
    install_pcgeos,
    set_pcgeos_read_options,
    set_pcgeos_write_options,
    true
);

#[cfg(feature = "xerox")]
register_codec!(
    xerox,
    XeroxCodec,
    XeroxReadOptions,
    XeroxWriteOptions,
    install_xerox,
    set_xerox_read_options,
    set_xerox_write_options,
    true
);

#[cfg(feature = "ssfn")]
register_codec!(
    ssfn,
    SsfnCodec,
    SsfnReadOptions,
    SsfnWriteOptions,
    install_ssfn,
    set_ssfn_read_options,
    set_ssfn_write_options,
    true
);
