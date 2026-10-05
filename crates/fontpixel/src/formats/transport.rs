//! Registry-only budget adapters; frozen native codecs keep their own profiles.
use super::{FormatWriter, OwnedWrite, TransportLimits, WriteOptions};
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
    /// This codec's limits under the caller's common allowance: its own
    /// budget intersected with `common`, so a common setting cannot enlarge
    /// it. The outline writers override this (see their impl).
    fn limits_for(&self, common: &TransportLimits) -> TransportLimits {
        self.budget().intersect(common)
    }
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
    /// Whether [`write_owned_native`](Self::write_owned_native) is a real
    /// owned route that charges the model itself.
    fn owns_model(&self) -> bool {
        false
    }
    /// Owned route with the model inside this codec's (restricted) limits.
    /// Only called when [`owns_model`](Self::owns_model) is true.
    fn write_owned_native(&self, fonts: FontSet, options: &WriteOptions) -> Result<Written> {
        self.write(&fonts, options)
    }
    /// Whether [`write_reserved_native`](Self::write_reserved_native) skips this codec's
    /// own estimate of the borrowed model.
    fn reserves_model(&self) -> bool {
        false
    }
    /// As [`write`](FormatWriter::write) when the caller has already reserved
    /// the model's exact `retained_usage` outside this codec's limits: the
    /// codec does not charge its estimate of that model again
    /// ([`ModelCharge::Reserved`](super::ModelCharge::Reserved)). Only called
    /// when [`reserves_model`](Self::reserves_model) is true.
    fn write_reserved_native(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        self.write(fonts, options)
    }
    /// [`validate_native`](Self::validate_native) on the reserved route.
    fn validate_native_reserved(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> Result<()> {
        if !self.reserves_model() {
            return self.validate_native(fonts, options, limits);
        }
        drop(
            self.restricted(limits)
                .write_reserved_native(fonts, options)?,
        );
        Ok(())
    }
}

#[allow(dead_code)]
struct NativeWriter<C>(C);
impl<C: Budget> FormatWriter<FontSet> for NativeWriter<C> {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        if let Some(common) = &options.transport_limits {
            let limits = self.0.limits_for(common);
            fonts.validate(&limits.resources)?;
            self.0.restricted(&limits).write(fonts, options)
        } else {
            self.0.write(fonts, options)
        }
    }
    fn transport_limits(&self) -> TransportLimits {
        self.0.budget()
    }
    fn common_transport_limits(&self, common: &TransportLimits) -> TransportLimits {
        self.0.limits_for(common)
    }
    fn write_reserved(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        // The caller keeps the model's exact retained usage reserved outside
        // these limits, so the codec is told not to charge its estimate.
        let Some(common) = &options.transport_limits else {
            return self.write(fonts, options);
        };
        if !self.0.reserves_model() {
            return self.write(fonts, options);
        }
        let limits = self.0.limits_for(common);
        fonts.validate(&limits.resources)?;
        self.0
            .restricted(&limits)
            .write_reserved_native(fonts, options)
    }
    fn validate_transport_reserved(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> Result<()> {
        self.0.validate_native_reserved(fonts, options, limits)
    }
    fn validate_transport(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> Result<()> {
        self.0.validate_native(fonts, options, limits)
    }
    fn write_owned(&self, fonts: FontSet, options: &WriteOptions) -> OwnedWrite<FontSet> {
        if self.0.reserves_model() {
            // The registry's borrowing route with the same limits (the
            // allowance less the model's exact retained usage); the model stays
            // live until the writer returns. The codec is told the model is
            // reserved, so it charges no estimate of it on top.
            let Some(common) = &options.transport_limits else {
                return OwnedWrite::Declined(fonts);
            };
            let result = (|| {
                let model = fonts
                    .retained_usage(common.resources.max_total_bytes)?
                    .bytes;
                let mut reserved = options.clone();
                reserved.transport_limits = Some(common.without_model(model)?);
                self.write_reserved(&fonts, &reserved)
            })();
            drop(fonts);
            return OwnedWrite::Written(result);
        }
        if !self.0.owns_model() {
            return OwnedWrite::Declined(fonts);
        }
        // The owned route's single allowance: the model's retained usage plus
        // exactly what the borrowing route would give the codec, i.e. its own
        // budget intersected with the common limits less the model. The codec's
        // budget therefore keeps bounding its working stages as before, and the
        // model's charge can be reused once the codec has dropped it.
        let owned_limits = || -> Result<TransportLimits> {
            let (model, mut limits) = if let Some(common) = &options.transport_limits {
                let total = common.resources.max_total_bytes;
                let model = fonts.retained_usage(total)?.bytes;
                let error = || crate::Error::ResourceLimit {
                    resource: "writer model bytes",
                    limit: total,
                };
                let mut working = common.clone();
                working.resources.max_total_bytes = total.checked_sub(model).ok_or_else(error)?;
                working.max_output_bytes = common
                    .max_output_bytes
                    .checked_sub(model)
                    .ok_or_else(error)?;
                (model, self.0.limits_for(&working))
            } else {
                let model = fonts.retained_usage(usize::MAX)?.bytes;
                (model, self.0.budget())
            };
            let total = limits.resources.max_total_bytes;
            limits.resources.max_total_bytes =
                total
                    .checked_add(model)
                    .ok_or(crate::Error::ResourceLimit {
                        resource: "writer model bytes",
                        limit: total,
                    })?;
            Ok(limits)
        };
        OwnedWrite::Written(owned_limits().and_then(|limits| {
            fonts.validate(&limits.resources)?;
            self.0
                .restricted(&limits)
                .write_owned_native(fonts, options)
        }))
    }
}

#[allow(dead_code, private_bounds)]
pub(super) fn writer<C: Budget + 'static>(codec: C) -> Box<dyn FormatWriter<FontSet>> {
    Box::new(NativeWriter(codec))
}

#[allow(unused_macros)]
macro_rules! resource_codec {
    ($module:ident, $codec:ident, $($path:ident).+ $(; $output:ident)? $(=> $reserved:ident)?) => {
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
            $(
                fn reserves_model(&self) -> bool {
                    true
                }
                fn write_reserved_native(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
                    self.$reserved(fonts, options)
                }
            )?
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
resource_codec!(beos, BeosCodec, write_options.limits; max_output_bytes => write_reserved);

#[cfg(feature = "bmfont")]
resource_codec!(bmfont, BmfontCodec, write_options.limits => write_reserved);

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
resource_codec!(gdr, GdrCodec, write_options.limits; max_output_bytes => write_reserved);

#[cfg(feature = "gem")]
resource_codec!(gem, GemCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "geos")]
resource_codec!(geos, GeosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "gf")]
resource_codec!(gf, GfCodec, write_options.limits; max_output_bytes => write_reserved);

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
resource_codec!(kbits, KbitsCodec, write_options.limits => write_reserved);

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
resource_codec!(pf2, Pf2Codec, write_options.limits; max_output_bytes => write_reserved);

#[cfg(feature = "pfr")]
resource_codec!(pfr, PfrCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pilfont")]
resource_codec!(pilfont, PilfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "pk")]
resource_codec!(pk, PkCodec, write_options.limits; max_output_bytes => write_reserved);

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
resource_codec!(psf2txt, Psf2txtCodec, write_options.limits; max_output_bytes => write_reserved);

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
resource_codec!(sfd, SfdCodec, write_options.limits => write_reserved);

#[cfg(feature = "sfont")]
resource_codec!(sfont, SfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "signum")]
resource_codec!(signum, SignumCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "snf")]
resource_codec!(snf, SnfCodec, write_options.limits; max_output_bytes => write_reserved);

#[cfg(feature = "ssfn")]
resource_codec!(ssfn, SsfnCodec, write_options.limits; max_output_bytes => write_reserved);

#[cfg(feature = "symbos")]
resource_codec!(symbos, SymbosCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "tasprint")]
resource_codec!(tasprint, TasprintCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "u8m")]
resource_codec!(u8m, U8mCodec, write_options.limits; max_output_bytes => write_reserved);

#[cfg(feature = "vfont")]
resource_codec!(vfont, VfontCodec, write_options.limits; max_output_bytes);

#[cfg(feature = "vlw")]
resource_codec!(vlw, VlwCodec, write_options.limits; max_output_bytes => write_reserved);

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
resource_codec!(zapfont, ZapfontCodec, write_options.limits => write_reserved);

#[cfg(feature = "svg")]
resource_codec!(svg, SvgCodec, write_options.limits.resources; max_output_bytes);
#[cfg(feature = "vector-raster")]
impl Budget for super::vector_raster::Codec {
    fn budget(&self) -> TransportLimits {
        TransportLimits {
            resources: self.write_options.outline.limits.sfnt.resources.clone(),
            max_output_bytes: usize::MAX,
        }
    }
    fn restricted(&self, limits: &TransportLimits) -> Self {
        let mut codec = self.clone();
        codec.write_options.outline.limits.sfnt.resources = limits.resources.clone();
        codec
    }
    /// A caller's common total larger than this codec's own total replaces
    /// it when that own total is still the library default (256 MiB): an
    /// explicit larger budget (CLI `--max-total-bytes`, batch
    /// `max_total_bytes` or a library `TransportLimits`) can then admit a
    /// genuinely larger outline or webfont job. A configured codec total
    /// other than the default is never enlarged, every other field is still
    /// intersected, and without common limits the default stays 256 MiB.
    fn limits_for(&self, common: &TransportLimits) -> TransportLimits {
        let mut budget = self.budget();
        let total = &mut budget.resources.max_total_bytes;
        if *total == crate::ResourceLimits::default().max_total_bytes {
            *total = (*total).max(common.resources.max_total_bytes);
        }
        budget.intersect(common)
    }
    fn owns_model(&self) -> bool {
        true
    }
    fn write_owned_native(&self, fonts: FontSet, options: &WriteOptions) -> Result<Written> {
        let (bytes, report) =
            super::vector_raster::write_owned(fonts, self.id, &self.write_options)?;
        Ok(Written {
            output: crate::OutputBundle::single(options.primary_name.clone(), bytes),
            report,
        })
    }
}

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
    fn reserves_model(&self) -> bool {
        true
    }
    fn write_reserved_native(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        super::embedded::EmbeddedCodec::write_reserved(self, fonts, options)
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

#[cfg(all(test, feature = "vector-raster"))]
mod outline_budget_tests {
    //! The outline writers' common-limit rule ([`Budget::limits_for`]).
    use super::{Budget, TransportLimits};
    use crate::ResourceLimits;

    fn common(total: usize) -> TransportLimits {
        TransportLimits {
            resources: ResourceLimits {
                max_total_bytes: total,
                ..ResourceLimits::default()
            },
            max_output_bytes: total,
        }
    }

    #[test]
    fn a_larger_explicit_total_raises_only_the_default_outline_budget() {
        let default = ResourceLimits::default().max_total_bytes;
        let codec = super::super::vector_raster::Codec::new("woff2");
        assert_eq!(codec.budget().resources.max_total_bytes, default);
        // Without common limits (or at or below the default) nothing changes.
        for total in [1, default - 1, default] {
            assert_eq!(
                codec.limits_for(&common(total)),
                codec.budget().intersect(&common(total))
            );
        }
        // A larger explicit total becomes the codec total; every other field
        // still intersects (the default 64 MiB table limit stays).
        let raised = codec.limits_for(&common(default + 1));
        assert_eq!(raised.resources.max_total_bytes, default + 1);
        assert_eq!(
            raised.resources.max_table_bytes,
            ResourceLimits::default().max_table_bytes
        );
        assert_eq!(raised.max_output_bytes, default + 1);
        // A configured, non-default codec total is never enlarged.
        let mut small = codec.clone();
        small
            .write_options
            .outline
            .limits
            .sfnt
            .resources
            .max_total_bytes = 4096;
        assert_eq!(
            small.limits_for(&common(1 << 30)).resources.max_total_bytes,
            4096
        );
        // Other codecs keep the plain intersection.
        #[cfg(feature = "snf")]
        {
            let snf = super::super::snf::SnfCodec::default();
            assert_eq!(
                snf.limits_for(&common(1 << 30)).resources.max_total_bytes,
                default
            );
        }
    }
}

#[cfg(all(
    test,
    feature = "snf",
    feature = "u8m",
    feature = "kbits",
    feature = "pf2",
    feature = "color"
))]
mod tests {
    //! `Registry::write_owned` for codecs that keep the model until they
    //! return: the model's exact retained usage is reserved once, the codec
    //! charges no estimate of it, and output and report equal the borrowing
    //! route.
    use super::super::{TransportLimits, WriteOptions};
    use crate::model::{Codepoint, Font, Glyph, Vector};
    use crate::{Bitmap, Error, FontSet, Report, ResourceLimits, ResourcePath, Result};

    const DEFAULT: usize = 256 * 1024 * 1024;

    /// Smallest accepted total; its output equals the output at `DEFAULT`
    /// and one byte less is a `ResourceLimit` error.
    fn minimal<T: PartialEq + std::fmt::Debug>(run: impl Fn(usize) -> Result<T>) -> (usize, T) {
        let expected = run(DEFAULT).unwrap();
        let (mut lo, mut hi) = (0usize, DEFAULT);
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if run(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let at = run(hi).unwrap();
        assert_eq!(at, expected, "minimal total changed the output");
        // Legacy writers carry the limit inside their omission report; the
        // embedded writers' glyph-count gate is an input error.
        match run(hi - 1) {
            Err(Error::ResourceLimit { .. }) => {}
            Err(Error::InvalidInput(m)) if m.contains("budget exceeded") => {}
            Err(Error::ReportedErrors(r))
                if r.entries
                    .last()
                    .is_some_and(|e| e.message.contains("exceeds configured limit")) => {}
            other => panic!("one byte short must be a resource error: {other:?}"),
        }
        (hi, at)
    }

    fn options(name: &str, total: usize) -> WriteOptions {
        let mut o = WriteOptions::new(ResourcePath::new(name).unwrap());
        o.transport_limits = Some(TransportLimits {
            resources: ResourceLimits {
                max_total_bytes: total,
                ..ResourceLimits::default()
            },
            max_output_bytes: total,
        });
        o
    }

    /// One strike whose clones retain exactly what it does.
    fn model() -> FontSet {
        let mut f = Font::default();
        f.metrics.pixel_size = Some(8);
        f.metrics.ascent = Some(7);
        f.metrics.descent = Some(1);
        f.names.family = Some("Ledger".into());
        for (i, pattern) in ["#.\n##", "###\n#.#\n###", "#.#\n.#.\n#.#", "##\n##"]
            .into_iter()
            .enumerate()
        {
            let mut g = Glyph::new(pattern.parse::<Bitmap>().unwrap());
            g.codepoints = vec![Codepoint::Unicode(0x41 + i as u32)];
            g.metrics.advance = Some(Vector::new(4, 0));
            f.add_glyph(g).unwrap();
        }
        FontSet::from(f).clone()
    }

    #[test]
    fn reserved_writers_charge_the_model_exactly_once() {
        let registry = crate::registry();
        let set = model();
        let used = set.retained_usage(usize::MAX).unwrap().bytes;
        let limits = ResourceLimits::default();
        let font = set.single().unwrap();
        // What each codec charges for the model on its own ledger when the
        // caller has not reserved it.
        let legacy = super::super::legacy_common::source_budget(&set, &limits)
            .unwrap()
            .used
            - 1024;
        let editor = super::super::editor_fidelity::model_bytes(font).unwrap()
            + super::super::editor_fidelity::container_bytes(&set).unwrap();
        for (id, name, estimate) in [
            ("snf", "a.snf", legacy),
            ("u8m", "a.u8m", legacy),
            ("kbits", "a.kbits", editor),
        ] {
            let (owned, written) = minimal(|total| {
                registry.write_owned(set.clone(), id, &options(name, total), Report::default())
            });
            let (borrowed, expected) = minimal(|total| {
                registry.write_with_report(&set, id, &options(name, total), Report::default())
            });
            assert_eq!(written, expected, "{id}");
            // The borrowing route still charges the codec's estimate; the
            // owned route charges the exact model in its place.
            assert_eq!(owned, borrowed - estimate + used, "{id}");
        }
    }

    #[test]
    fn pf2_charges_its_working_storage_exactly() {
        // PF2 writes on one exact working ledger. With container properties
        // whose (exactly charged) diagnostics bind, the owned minimum is the
        // borrowing minimum less the codec's model and container estimates
        // plus the model's exact usage: everything else is charged the same.
        let registry = crate::registry();
        let mut set = model();
        for i in 0..8 {
            let name = format!("opaque-{i}-{}", "x".repeat(4000));
            set.properties.push(crate::model::Property::new(
                name,
                crate::model::PropertyValue::Flag,
            ));
        }
        let used = set.retained_usage(usize::MAX).unwrap().bytes;
        let limits = ResourceLimits::default();
        let estimate = super::super::w42_common::source_cost(set.single().unwrap(), &limits)
            .unwrap()
            + super::super::w42_common::container_cost(&set).unwrap();
        let (owned, written) = minimal(|total| {
            registry.write_owned(
                set.clone(),
                "pf2",
                &options("a.pf2", total),
                Report::default(),
            )
        });
        let (borrowed, expected) = minimal(|total| {
            registry.write_with_report(&set, "pf2", &options("a.pf2", total), Report::default())
        });
        assert_eq!(written, expected);
        assert_eq!(owned, borrowed - estimate + used);
        // Only the diagnostics' exact strings and entries (no 512-byte
        // per-entry allowance, no doubled model) remain beyond the model.
        let strings: usize = written
            .report
            .entries
            .iter()
            .map(|e| e.code.len() + e.message.len())
            .sum();
        assert!(owned - used >= strings, "{owned} {used} {strings}");
        assert!(
            owned - used < strings + 16 * 1024,
            "{owned} {used} {strings}"
        );
    }

    #[test]
    fn tex_readback_glyph_allowance_covers_its_fixed_storage() {
        // While a pk/gf readback parses (before its exact `retained_usage`
        // replaces the parser's count), each decoded glyph is admitted at
        // 2048 (pk) or 2304 (gf) bytes plus twice its raster (decoded rows
        // and the bitmap copy). The fixed per-glyph storage it covers: the
        // glyph slot, at most three per glyph while the slot array doubles
        // (old and new arrays overlap); a one-code vector (minimum capacity
        // four); one full B-tree node per cmap entry (the toolchain-
        // independent bound) and that entry's id vector; and the three TeX
        // glyph properties (array of capacity four and their names).
        use crate::model::{Codepoint, GlyphId, Property};
        use std::mem::size_of;
        let slot = 3 * size_of::<Option<Glyph>>();
        let codes = 4 * size_of::<Codepoint>();
        // On the audited toolchain the model's B-tree bound is one node per
        // five entries plus one, and that extra node fits the reader's
        // initial 4096-byte allowance; elsewhere one node per entry.
        let full =
            16 * (size_of::<Codepoint>() + size_of::<Vec<GlyphId>>() + size_of::<usize>()) + 128;
        assert!(full <= 4096);
        let node = if cfg!(fontpixel_verified_btree_1_99) {
            full.div_ceil(5)
        } else {
            full
        };
        let ids = 4 * size_of::<GlyphId>();
        let properties = 4 * size_of::<Property>() + "tex.tfm_width tex.dx tex.dy".len();
        let fixed = slot + codes + node + ids + properties;
        // Off the audited toolchain the per-entry node bound alone does not
        // fit (documented in docs/w6.1-writer-budget.md).
        assert!(
            !cfg!(fontpixel_verified_btree_1_99) || fixed <= 2048,
            "{fixed}"
        );
    }

    #[test]
    #[cfg(all(feature = "pk", feature = "gf"))]
    fn tex_writers_charge_the_exact_model_on_either_route() {
        // pk and gf charge the model's exact retained usage on their own
        // ledger when it is borrowed, and nothing when it is reserved, so the
        // owned (reserved) and borrowing minima are equal, with equal output.
        let registry = crate::registry();
        let mut set = model();
        for font in &mut set.fonts {
            font.metrics.point_size = Some(crate::model::Rational::new(8, 1).unwrap());
        }
        for (id, name) in [("pk", "a.pk"), ("gf", "a.gf")] {
            let (owned, written) = minimal(|total| {
                registry.write_owned(set.clone(), id, &options(name, total), Report::default())
            });
            let (borrowed, expected) = minimal(|total| {
                registry.write_with_report(&set, id, &options(name, total), Report::default())
            });
            assert_eq!(written, expected, "{id}");
            assert_eq!(owned, borrowed, "{id}");
        }
    }

    #[test]
    #[cfg(feature = "zapfont")]
    fn zapfont_releases_rasters_and_readback_estimates() {
        // Exact boundary on both routes: the smallest total succeeds with the
        // default output and one byte less is a resource error. Released
        // per-cell rasters and the readback true-up keep the owned minimum
        // within the borrowing minimum less the legacy model estimate plus
        // the model's exact usage (the binding stage may differ by a few
        // bytes, since the estimate is charged before validation).
        let registry = crate::registry();
        let mut set = model();
        for font in &mut set.fonts {
            let ids: Vec<_> = font.glyphs().map(|(id, _)| id).collect();
            for (i, id) in ids.into_iter().enumerate() {
                font.edit_glyph(id, |g| {
                    g.codepoints = vec![Codepoint::Legacy(0x41 + i as u32)];
                    Ok(())
                })
                .unwrap();
            }
        }
        let used = set.retained_usage(usize::MAX).unwrap().bytes;
        let legacy = super::super::legacy_common::source_budget(&set, &ResourceLimits::default())
            .unwrap()
            .used
            - 1024;
        let (owned, written) = minimal(|total| {
            registry.write_owned(
                set.clone(),
                "zapfont",
                &options("a.zap", total),
                Report::default(),
            )
        });
        let (borrowed, expected) = minimal(|total| {
            registry.write_with_report(&set, "zapfont", &options("a.zap", total), Report::default())
        });
        assert_eq!(written, expected);
        assert!(owned > used, "{owned} {used}");
        assert!(
            owned <= borrowed - legacy + used,
            "{owned} {borrowed} {legacy} {used}"
        );
    }

    #[test]
    fn reserved_embedded_writers_skip_only_the_model_admission() {
        // `write_preflight` admits an estimate of the model alone; the owned
        // route skips it. For OTB it binds, so the owned route needs strictly
        // less than the borrowing route plus the model. Color charges the
        // model's exact usage on its working ledger only when borrowing, so
        // the owned route also needs less than the borrowing route plus it.
        let registry = crate::registry();
        let set = model();
        let used = set.retained_usage(usize::MAX).unwrap().bytes;
        for (id, name, strict) in [("otb", "a.otb", true), ("color", "a.ttf", true)] {
            let (owned, written) = minimal(|total| {
                registry.write_owned(set.clone(), id, &options(name, total), Report::default())
            });
            let (borrowed, expected) = minimal(|total| {
                registry.write_with_report(&set, id, &options(name, total), Report::default())
            });
            assert_eq!(written, expected, "{id}");
            assert!(owned > used, "{id}: {owned} {used}");
            if strict {
                assert!(owned < borrowed + used, "{id}: {owned} {borrowed} {used}");
            } else {
                assert!(owned <= borrowed + used, "{id}: {owned} {borrowed} {used}");
            }
        }
    }
}
