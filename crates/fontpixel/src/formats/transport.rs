//! Registry-only budget adapters; frozen native codecs keep their own profiles.
use super::{FormatWriter, TransportLimits, WriteOptions};
use crate::{FontSet, Result, resources::Written};

pub(super) fn merge_validation_report(
    mut original: crate::Report,
    replayed: crate::Report,
    writer_start: usize,
) -> crate::Report {
    // A budget failure may occur after replaying an already reported native
    // loss. Keep the original read/write order and only append the new suffix.
    // Prefix comparison is linear and does not collapse distinct diagnostics.
    let shared = replayed
        .entries
        .iter()
        .zip(&original.entries[writer_start..])
        .take_while(|(a, b)| a == b)
        .count();
    original
        .entries
        .extend(replayed.entries.into_iter().skip(shared));
    original
}

// These helpers have no callers in builds with no native format features.
#[allow(dead_code)]
trait Budget: FormatWriter<FontSet> + Clone {
    fn budget(&self) -> TransportLimits;
    fn restricted(&self, limits: &TransportLimits) -> Self;
    fn validate_native(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> Result<()> {
        // This is the same writer and native profile, never the registry's
        // independently configured reader. Its normal readback/audit runs with
        // compressed bytes and the original merged report already reserved.
        drop(self.restricted(limits).write(fonts, options)?);
        Ok(())
    }
}

#[allow(dead_code)]
struct NativeWriter<C>(C);
impl<C: Budget> FormatWriter<FontSet> for NativeWriter<C> {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        self.0.write(fonts, options)
    }
    fn transport_limits(&self) -> TransportLimits {
        self.0.budget()
    }
    fn validate_transport(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> Result<()> {
        self.0.validate_native(fonts, options, limits)
    }
}

#[allow(dead_code, private_bounds)]
pub(super) fn writer<C: Budget + 'static>(codec: C) -> Box<dyn FormatWriter<FontSet>> {
    Box::new(NativeWriter(codec))
}

#[allow(unused_macros)]
macro_rules! resource_codec {
    ($module:ident, $codec:ident, $($path:ident).+ $(; $output:ident)?) => {
        impl Budget for super::$module::$codec {
            fn budget(&self) -> TransportLimits {
                TransportLimits {
                    resources: self.$($path).+.clone(),
                    max_output_bytes: resource_codec!(@output self $(, $output)?),
                }
            }
            fn restricted(&self, limits: &TransportLimits) -> Self {
                let mut codec = self.clone();
                codec.$($path).+ = limits.resources.clone();
                $(codec.write_options.$output = codec.write_options.$output.min(limits.max_output_bytes);)?
                codec
            }
        }
    };
    (@output $this:ident, $output:ident) => { $this.write_options.$output };
    (@output $this:ident) => { usize::MAX };
}

#[cfg(feature = "amiga")]
resource_codec!(amiga, AmigaCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "bbc")]
resource_codec!(bbc, BbcCodec, write_options.limits);

#[cfg(feature = "beos")]
resource_codec!(beos, BeosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "bmfont")]
resource_codec!(bmfont, BmfontCodec, write_options.limits);

#[cfg(feature = "c")]
resource_codec!(c, CCodec, write_options.limits);

#[cfg(feature = "catalyst")]
resource_codec!(catalyst, CatalystCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "chiwriter")]
resource_codec!(chiwriter, ChiwriterCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "comfont")]
resource_codec!(comfont, ComfontCodec, write_options.limits);

#[cfg(feature = "consoleet")]
resource_codec!(consoleet, ConsoleetCodec, write_options.limits);

#[cfg(feature = "cpi")]
resource_codec!(cpi, CpiCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "cybiko")]
resource_codec!(cybiko, CybikoCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "daisy")]
resource_codec!(daisy, DaisyCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "dashen")]
resource_codec!(dashen, DashenCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "dosstart")]
resource_codec!(dosstart, DosstartCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "draw")]
resource_codec!(draw, DrawCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "drcs")]
resource_codec!(drcs, DrcsCodec, write_options.limits);

#[cfg(feature = "drhalo")]
resource_codec!(drhalo, DrhaloCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "edwin")]
resource_codec!(edwin, EdwinCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "fbit")]
resource_codec!(fbit, FbitCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "figlet")]
resource_codec!(figlet, FigletCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "fnt")]
resource_codec!(fnt, FntCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "fon")]
resource_codec!(fon, FonCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "fontx2")]
resource_codec!(fontx2, Fontx2Codec, write_options.limits; max_output_bytes);

#[cfg(feature = "fzx")]
resource_codec!(fzx, FzxCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "gdr")]
resource_codec!(gdr, GdrCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "gem")]
resource_codec!(gem, GemCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "geos")]
resource_codec!(geos, GeosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "gf")]
resource_codec!(gf, GfCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "gpi")]
resource_codec!(gpi, GpiCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "grasp")]
resource_codec!(grasp, GraspCodec, write_options.limits);

#[cfg(feature = "hanfont")]
resource_codec!(hanfont, HanfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "hbf")]
resource_codec!(hbf, HbfCodec, write_options.limits);

#[cfg(feature = "hex")]
resource_codec!(hex, HexCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "hmzk")]
resource_codec!(hmzk, HmzkCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "hppcl")]
resource_codec!(hppcl, HppclCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "hrcg")]
resource_codec!(hrcg, HrcgCodec, write_options.limits);

#[cfg(feature = "hzk")]
resource_codec!(hzk, HzkCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "iigs")]
resource_codec!(iigs, IigsCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "image")]
resource_codec!(image, ImageCodec, write_options.limits);

#[cfg(feature = "kbits")]
resource_codec!(kbits, KbitsCodec, write_options.limits);

#[cfg(feature = "kbitx")]
resource_codec!(kbitx, KbitxCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "lisa")]
resource_codec!(lisa, LisaCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "mgtk")]
resource_codec!(mgtk, MgtkCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "mkwinfont")]
resource_codec!(mkwinfont, MkwinfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "nfnt")]
resource_codec!(nfnt, NfntCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "oberon")]
resource_codec!(oberon, OberonCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "palm")]
resource_codec!(palm, PalmCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pbf")]
resource_codec!(pbf, PbfCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pcf")]
resource_codec!(pcf, PcfCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pcgeos")]
resource_codec!(pcgeos, PcgeosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pcr")]
resource_codec!(pcr, PcrCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pf2")]
resource_codec!(pf2, Pf2Codec, write_options.limits; max_output_bytes);

#[cfg(feature = "pfr")]
resource_codec!(pfr, PfrCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pilfont")]
resource_codec!(pilfont, PilfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pk")]
resource_codec!(pk, PkCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "plan9")]
resource_codec!(plan9, Plan9Codec, write_options.limits; max_output_bytes);

#[cfg(feature = "playdate")]
resource_codec!(playdate, PlaydateCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "plus3dos")]
resource_codec!(plus3dos, Plus3dosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "polyprint")]
resource_codec!(polyprint, PolyprintCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "prebuilt")]
resource_codec!(prebuilt, PrebuiltCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "printshop")]
resource_codec!(printshop, PrintshopCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "psf")]
resource_codec!(psf, PsfCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "psf2txt")]
resource_codec!(psf2txt, Psf2txtCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pxl")]
resource_codec!(pxl, PxlCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "qpf")]
resource_codec!(qpf, QpfCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "qpf2")]
resource_codec!(qpf2, Qpf2Codec, write_options.limits; max_output_bytes);

#[cfg(feature = "raw")]
resource_codec!(raw, RawCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "riscos")]
resource_codec!(riscos, RiscosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "rockbox")]
resource_codec!(rockbox, RockboxCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "sbf")]
resource_codec!(sbf, SbfCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "sfd")]
resource_codec!(sfd, SfdCodec, write_options.limits);

#[cfg(feature = "sfont")]
resource_codec!(sfont, SfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "signum")]
resource_codec!(signum, SignumCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "snf")]
resource_codec!(snf, SnfCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "ssfn")]
resource_codec!(ssfn, SsfnCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "symbos")]
resource_codec!(symbos, SymbosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "tasprint")]
resource_codec!(tasprint, TasprintCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "u8m")]
resource_codec!(u8m, U8mCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "vfont")]
resource_codec!(vfont, VfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "vlw")]
resource_codec!(vlw, VlwCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "vtfont")]
resource_codec!(vtfont, VtfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "writeon")]
resource_codec!(writeon, WriteonCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "wsfont")]
resource_codec!(wsfont, WsfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "wyse")]
resource_codec!(wyse, WyseCodec, write_options.limits);

#[cfg(feature = "xbin")]
resource_codec!(xbin, XbinCodec, write_options.limits);

#[cfg(feature = "xerox")]
resource_codec!(xerox, XeroxCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "yaff")]
resource_codec!(yaff, YaffCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "zapfont")]
resource_codec!(zapfont, ZapfontCodec, write_options.limits);

#[cfg(feature = "svg")]
resource_codec!(svg, SvgCodec, write_options.limits.resources; max_output_bytes);
#[cfg(feature = "vector-raster")]
resource_codec!(
    vector_raster,
    Codec,
    write_options.outline.limits.sfnt.resources
);

#[cfg(any(feature = "otb", feature = "bdat", feature = "color"))]
impl Budget for super::embedded::EmbeddedCodec {
    fn budget(&self) -> TransportLimits {
        #[cfg(feature = "color")]
        if self.kind == crate::sfnt::bitmap::Kind::Color {
            return TransportLimits {
                resources: self.color_write_options.limits.sfnt.resources.clone(),
                max_output_bytes: usize::MAX,
            };
        }
        TransportLimits {
            resources: self.write_options.limits.sfnt.resources.clone(),
            max_output_bytes: usize::MAX,
        }
    }
    fn restricted(&self, limits: &TransportLimits) -> Self {
        let mut codec = self.clone();
        #[cfg(feature = "color")]
        if self.kind == crate::sfnt::bitmap::Kind::Color {
            codec.color_write_options.limits.sfnt.resources = limits.resources.clone();
            return codec;
        }
        codec.write_options.limits.sfnt.resources = limits.resources.clone();
        codec
    }
}

#[cfg(feature = "bdf")]
impl Budget for super::bdf::BdfCodec {
    fn budget(&self) -> TransportLimits {
        TransportLimits {
            resources: crate::ResourceLimits::default(),
            max_output_bytes: self.write_options.max_output_bytes,
        }
    }
    fn restricted(&self, limits: &TransportLimits) -> Self {
        let mut codec = self.clone();
        codec.write_options.max_output_bytes = codec
            .write_options
            .max_output_bytes
            .min(limits.max_output_bytes)
            .min(limits.resources.max_total_bytes);
        codec
    }
    fn validate_native(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> Result<()> {
        use super::FormatReader;
        fonts.validate(&limits.resources)?;
        let codec = self.restricted(limits);
        let written = codec.write(fonts, options)?;
        let bytes = written.output.into_single()?;
        drop(written.report);
        // BDF's older writer has no ResourceLimits field/native readback. BDF
        // has one self-describing profile, so this codec's own reader matches.
        let input = crate::Input::new(&bytes);
        let mut context = crate::resources::ReadContext::new(&input, &limits.resources)?;
        drop(codec.read(
            &mut context,
            &super::ReadOptions {
                limits: limits.resources.clone(),
                ..Default::default()
            },
        )?);
        Ok(())
    }
}
