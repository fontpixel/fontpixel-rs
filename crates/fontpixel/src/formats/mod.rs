//! Capability registration and model-independent dispatch infrastructure.
//!
//! The generic registry starts with planned entries. `crate::registry()` installs
//! actual FontSet codecs under their Cargo features.
use crate::resources::{Input, ReadContext, ResourceLimits, ResourcePath, Written};
use crate::{Error, Operation, Report, Result};

#[cfg(feature = "bdf")]
pub mod bdf;
pub mod catalogue;
#[cfg(feature = "image")]
pub mod image;
#[cfg(any(feature = "hex", feature = "psf"))]
mod console;
#[cfg(any(
    feature = "otb",
    feature = "bdat",
    feature = "color",
    feature = "vector-raster"
))]
pub mod embedded;
#[cfg(feature = "hex")]
pub mod hex;
#[cfg(feature = "pcf")]
pub mod pcf;
#[cfg(feature = "psf")]
pub mod psf;
#[cfg(feature = "svg")]
pub mod svg;
#[cfg(feature = "vector-raster")]
pub mod vector_raster;
#[cfg(any(feature = "bdf", feature = "pcf"))]
pub(crate) mod x11;
#[cfg(feature = "yaff")]
pub mod yaff;

/// Static format metadata, including compiled feature state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FormatInfo {
    /// Stable CLI/API identifier.
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Possible extensions without dots; hints only, including compound suffixes.
    pub extensions: &'static [&'static str],
    /// Whether this build enables the format's Cargo feature.
    pub feature_enabled: bool,
}

/// Available hooks, separate from catalogue membership and feature selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FormatStatus {
    /// Catalogue information.
    pub format: FormatInfo,
    /// An actual content detector is registered.
    pub detect: bool,
    /// An actual reader is registered.
    pub read: bool,
    /// An actual writer is registered.
    pub write: bool,
}

/// Increasing strength of structural evidence, with filename hints below content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// No content evidence.
    NoMatch,
    /// Extension-only hint, returned by the registry (not a detector).
    Extension,
    /// Plausible content; may be ambiguous.
    Possible,
    /// Strong magic and/or validated structure.
    Certain,
}

/// Successful detection, explicitly preserving the strength of its evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    /// Selected format.
    pub format: FormatInfo,
    /// How the selection was made.
    pub confidence: Confidence,
}

/// Common options; each implemented format will add strongly typed options.
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// Explicit ID bypassing auto-detection.
    pub format: Option<String>,
    /// Input resource limits.
    pub limits: ResourceLimits,
}

/// Common write options. Format-specific options belong to later format modules.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    /// Refuse to return assets if the combined read/write report has loss.
    pub strict: bool,
    /// Primary relative output name; codecs may add sibling/nested sidecars.
    pub primary_name: ResourcePath,
}

impl WriteOptions {
    /// Create non-strict options with a validated primary asset name.
    pub fn new(primary_name: ResourcePath) -> Self {
        Self {
            strict: false,
            primary_name,
        }
    }
}

/// A decoded model and all read diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded<M> {
    /// Decoded model (`FontSet` for the built-in font registry).
    pub fonts: M,
    /// Reader diagnostics.
    pub report: Report,
    /// The actual format used to decode the input.
    pub format: &'static str,
}

/// Format content-recognition hook. No filesystem writes are allowed.
pub trait FormatDetector: Send + Sync {
    /// Inspect bounded input; do not treat its filename as content evidence.
    fn detect(&self, input: &mut ReadContext<'_>) -> Result<Confidence>;
}

/// Format reader over a shared model, including caller-supplied sidecars.
pub trait FormatReader<M>: Send + Sync {
    /// Parse one complete input; diagnostics must describe repairs and loss.
    fn read(&self, input: &mut ReadContext<'_>, options: &ReadOptions) -> Result<(M, Report)>;
}

/// Side-effect-free writer returning all assets and loss diagnostics.
pub trait FormatWriter<M>: Send + Sync {
    /// Encode the model entirely in memory. Never persist output here: strict
    /// rejection happens in the registry after the full report is available.
    fn write(&self, fonts: &M, options: &WriteOptions) -> Result<Written>;
}

/// One catalogue entry with independently installable capabilities.
pub struct Registration<M> {
    /// Static metadata.
    pub format: FormatInfo,
    /// Optional content detector.
    pub detector: Option<Box<dyn FormatDetector>>,
    /// Optional model reader.
    pub reader: Option<Box<dyn FormatReader<M>>>,
    /// Optional model writer.
    pub writer: Option<Box<dyn FormatWriter<M>>>,
}

impl<M> Registration<M> {
    /// An honest unimplemented entry with no capability hooks.
    pub fn planned(format: FormatInfo) -> Self {
        Self {
            format,
            detector: None,
            reader: None,
            writer: None,
        }
    }

    /// Runtime capabilities of this entry.
    pub fn status(&self) -> FormatStatus {
        FormatStatus {
            format: self.format,
            detect: self.detector.is_some(),
            read: self.reader.is_some(),
            write: self.writer.is_some(),
        }
    }
}

/// Extensible model-independent format registry. [`crate::FontRegistry`]
/// specializes it with the real FontSet without duplicating dispatch contracts.
pub struct Registry<M> {
    entries: Vec<Registration<M>>,
}

impl<M> Default for Registry<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> Registry<M> {
    /// Generic planned catalogue. Use `crate::registry()` for built-in FontSet codecs.
    pub fn new() -> Self {
        Self {
            entries: catalogue::CATALOGUE
                .iter()
                .copied()
                .map(Registration::planned)
                .collect(),
        }
    }

    /// Iterate all formats in catalogue/registration order, including unavailable ones.
    pub fn formats(&self) -> impl ExactSizeIterator<Item = FormatStatus> + '_ {
        self.entries.iter().map(Registration::status)
    }

    /// Look up metadata and actual capabilities by exact stable ID.
    pub fn status(&self, id: &str) -> Result<FormatStatus> {
        Ok(self.entry(id)?.status())
    }

    fn entry(&self, id: &str) -> Result<&Registration<M>> {
        self.entries
            .iter()
            .find(|entry| entry.format.id == id)
            .ok_or_else(|| Error::UnknownFormat(id.into()))
    }

    /// Install a new format or replace an empty planned entry. Duplicate live
    /// registrations and hooks for a disabled feature are rejected.
    pub fn register(&mut self, registration: Registration<M>) -> Result<()> {
        let id = registration.format.id;
        if id.is_empty()
            || !id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(Error::InvalidInput(
                "format IDs must use lowercase ASCII letters, digits, and hyphens".into(),
            ));
        }
        if !registration.format.feature_enabled {
            return Err(Error::FeatureDisabled { format: id.into() });
        }
        if let Some(existing) = self.entries.iter_mut().find(|entry| entry.format.id == id) {
            if existing.detector.is_some() || existing.reader.is_some() || existing.writer.is_some()
            {
                return Err(Error::InvalidInput(format!(
                    "format '{id}' is already registered"
                )));
            }
            if existing.format != registration.format {
                return Err(Error::InvalidInput(format!(
                    "registration changes catalogue metadata for '{id}'"
                )));
            }
            *existing = registration;
        } else {
            self.entries.push(registration);
        }
        Ok(())
    }

    /// Longest-suffix filename hints. Collisions remain visible, never pick the
    /// first matching `.fnt`, `.font`, `.ttf`, etc. from catalogue order.
    pub fn extension_candidates(&self, name: &str) -> Vec<FormatInfo> {
        let name = name.to_ascii_lowercase();
        let mut longest = 0;
        let mut candidates = Vec::new();
        for entry in &self.entries {
            let length = entry
                .format
                .extensions
                .iter()
                .filter(|ext| name.ends_with(&format!(".{ext}")))
                .map(|ext| ext.len())
                .max()
                .unwrap_or(0);
            if length > longest {
                longest = length;
                candidates.clear();
            }
            if length > 0 && length == longest {
                candidates.push(entry.format);
            }
        }
        candidates
    }

    /// Infer a unique filename hint, preserving ambiguity as an error.
    pub fn format_from_name(&self, name: &str) -> Result<FormatInfo> {
        match self.extension_candidates(name).as_slice() {
            [] => Err(Error::UnrecognizedFormat),
            [format] => Ok(*format),
            many => Err(Error::AmbiguousFormat {
                candidates: many.iter().map(|format| format.id.into()).collect(),
            }),
        }
    }

    /// Run content detectors first, falling back to filename hints only when
    /// no detector matches. Catalogue hints do not imply a reader exists.
    pub fn detect(&self, input: &Input<'_>, limits: &ResourceLimits) -> Result<Detected> {
        self.detect_context(&mut ReadContext::new(input, limits)?)
    }

    fn detect_context(&self, context: &mut ReadContext<'_>) -> Result<Detected> {
        let mut best = Confidence::NoMatch;
        let mut candidates = Vec::new();
        for entry in &self.entries {
            if let Some(detector) = &entry.detector {
                let confidence = detector.detect(context)?;
                if confidence == Confidence::Extension {
                    return Err(Error::InvalidInput(format!(
                        "detector '{}' returned filename-only evidence",
                        entry.format.id
                    )));
                }
                if confidence > best {
                    best = confidence;
                    candidates.clear();
                }
                if confidence != Confidence::NoMatch && confidence == best {
                    candidates.push(entry.format);
                }
            }
        }
        if candidates.is_empty() {
            return context
                .name()
                .ok_or(Error::UnrecognizedFormat)
                .and_then(|name| self.format_from_name(name))
                .map(|format| Detected {
                    format,
                    confidence: Confidence::Extension,
                });
        }
        match candidates.as_slice() {
            [format] => Ok(Detected {
                format: *format,
                confidence: best,
            }),
            many => Err(Error::AmbiguousFormat {
                candidates: many.iter().map(|format| format.id.into()).collect(),
            }),
        }
    }

    /// Fail early if a writer is unavailable, before a CLI opens any output.
    pub fn ensure_writable(&self, id: &str) -> Result<()> {
        let entry = self.entry(id)?;
        if entry.writer.is_none() {
            return Err(Error::NotImplemented {
                format: id.into(),
                operation: Operation::Write,
            });
        }
        Ok(())
    }

    /// Decode through an explicit reader or structural/filename detection.
    pub fn read(&self, input: &Input<'_>, options: &ReadOptions) -> Result<Loaded<M>> {
        let mut context = ReadContext::new(input, &options.limits)?;
        let id = if let Some(id) = &options.format {
            id.as_str()
        } else {
            self.detect_context(&mut context)?.format.id
        };
        let entry = self.entry(id)?;
        let reader = entry.reader.as_ref().ok_or_else(|| Error::NotImplemented {
            format: id.into(),
            operation: Operation::Read,
        })?;
        let (fonts, report) = reader.read(&mut context, options)?;
        if report.has_errors() {
            return Err(Error::ReportedErrors(report));
        }
        Ok(Loaded {
            fonts,
            report,
            format: entry.format.id,
        })
    }

    /// Encode with no preceding read diagnostics.
    pub fn write(&self, fonts: &M, id: &str, options: &WriteOptions) -> Result<Written> {
        self.write_with_report(fonts, id, options, Report::default())
    }

    /// Combine earlier read diagnostics with write diagnostics before checking
    /// strict mode, so loss during decoding cannot be silently overlooked.
    pub fn write_with_report(
        &self,
        fonts: &M,
        id: &str,
        options: &WriteOptions,
        mut report: Report,
    ) -> Result<Written> {
        self.ensure_writable(id)?;
        if report.has_errors() {
            return Err(Error::ReportedErrors(report));
        }
        if options.strict && report.has_loss() {
            return Err(Error::StrictLoss(report));
        }
        let writer = self
            .entry(id)?
            .writer
            .as_ref()
            .ok_or_else(|| Error::NotImplemented {
                format: id.into(),
                operation: Operation::Write,
            })?;
        let written = writer.write(fonts, options)?;
        report.extend(written.report);
        if report.has_errors() {
            return Err(Error::ReportedErrors(report));
        }
        if options.strict && report.has_loss() {
            return Err(Error::StrictLoss(report));
        }
        Ok(Written {
            output: crate::resources::encode_transport(written.output)?,
            report,
        })
    }
}

#[cfg(feature = "bdf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_bdf(&mut self) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == "bdf") {
            entry.detector = Some(Box::new(bdf::BdfCodec::default()));
            entry.reader = Some(Box::new(bdf::BdfCodec::default()));
            entry.writer = Some(Box::new(bdf::BdfCodec::default()));
        }
    }
}

#[cfg(feature = "pcf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_pcf(&mut self) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == "pcf") {
            entry.detector = Some(Box::new(pcf::PcfCodec::default()));
            entry.reader = Some(Box::new(pcf::PcfCodec::default()));
            entry.writer = Some(Box::new(pcf::PcfCodec::default()));
        }
    }
    /// Configure the built-in PCF writer without altering its detector/reader.
    pub fn set_pcf_write_options(&mut self, options: pcf::PcfWriteOptions) -> Result<()> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "pcf" && e.writer.is_some())
            .ok_or_else(|| Error::NotImplemented {
                format: "pcf".into(),
                operation: Operation::Write,
            })?;
        entry.writer = Some(Box::new(pcf::PcfCodec {
            write_options: options,
        }));
        Ok(())
    }
}

#[cfg(feature = "yaff")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_yaff(&mut self) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == "yaff") {
            entry.detector = Some(Box::new(yaff::YaffCodec::default()));
            entry.reader = Some(Box::new(yaff::YaffCodec::default()));
            entry.writer = Some(Box::new(yaff::YaffCodec::default()));
        }
    }
    /// Configure bounded YAFF writes independently of other codecs.
    pub fn set_yaff_write_options(&mut self, options: yaff::YaffWriteOptions) -> Result<()> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "yaff" && e.writer.is_some())
            .ok_or_else(|| Error::NotImplemented {
                format: "yaff".into(),
                operation: Operation::Write,
            })?;
        entry.writer = Some(Box::new(yaff::YaffCodec {
            write_options: options,
        }));
        Ok(())
    }
}

#[cfg(any(feature = "otb", feature = "bdat", feature = "color"))]
impl Registry<crate::FontSet> {
    pub(crate) fn install_embedded(
        &mut self,
        id: &'static str,
        kind: crate::sfnt::bitmap::Kind,
        write: bool,
    ) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == id) {
            entry.detector = Some(Box::new(embedded::EmbeddedCodec::new(kind)));
            entry.reader = Some(Box::new(embedded::EmbeddedCodec::new(kind)));
            if write {
                entry.writer = Some(Box::new(embedded::EmbeddedCodec::new(kind)));
            }
        }
    }
}

#[cfg(feature = "vector-raster")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_vector(&mut self, id: &'static str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == id) {
            let codec = vector_raster::Codec::new(id);
            if id != "vector-raster" {
                entry.detector = Some(Box::new(codec.clone()));
            }
            entry.reader = Some(Box::new(codec.clone()));
            entry.writer = Some(Box::new(codec));
        }
    }
    /// Configure all registered outline/container readers consistently.
    pub fn set_vector_read_options(&mut self, options: vector_raster::Options) {
        for id in ["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"] {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|e| e.format.id == id && e.reader.is_some())
            {
                let mut codec = vector_raster::Codec::new(entry.format.id);
                codec.read_options = options.clone();
                entry.reader = Some(Box::new(codec));
            }
        }
    }
    /// Configure all registered outline/container writers consistently.
    pub fn set_vector_write_options(&mut self, options: vector_raster::WriteOptions) {
        for id in ["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"] {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|e| e.format.id == id && e.writer.is_some())
            {
                let mut codec = vector_raster::Codec::new(entry.format.id);
                codec.write_options = options.clone();
                entry.writer = Some(Box::new(codec));
            }
        }
    }
}

#[cfg(feature = "svg")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_svg(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "svg") {
            e.detector = Some(Box::new(svg::SvgCodec::default()));
            e.reader = Some(Box::new(svg::SvgCodec::default()));
            e.writer = Some(Box::new(svg::SvgCodec::default()));
        }
    }
    /// Configure the SVG font reader/detector independently of its writer.
    pub fn set_svg_read_options(&mut self, options: svg::ReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "svg" && e.reader.is_some())
        {
            e.detector = Some(Box::new(svg::SvgCodec {
                read_options: options.clone(),
                ..Default::default()
            }));
            e.reader = Some(Box::new(svg::SvgCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure deterministic SVG font output independently of its reader.
    pub fn set_svg_write_options(&mut self, options: svg::WriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "svg" && e.writer.is_some())
        {
            e.writer = Some(Box::new(svg::SvgCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "hex")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_hex(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "hex") {
            e.detector = Some(Box::new(hex::HexCodec::default()));
            e.reader = Some(Box::new(hex::HexCodec::default()));
            e.writer = Some(Box::new(hex::HexCodec::default()));
        }
    }
    /// Set explicit HEX input geometry without changing output choices.
    pub fn set_hex_read_options(&mut self, options: hex::HexReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "hex" && e.reader.is_some())
        {
            e.reader = Some(Box::new(hex::HexCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure bounded deterministic HEX output.
    pub fn set_hex_write_options(&mut self, options: hex::HexWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "hex" && e.writer.is_some())
        {
            e.writer = Some(Box::new(hex::HexCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}
#[cfg(feature = "psf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_psf(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "psf") {
            e.detector = Some(Box::new(psf::PsfCodec::default()));
            e.reader = Some(Box::new(psf::PsfCodec::default()));
            e.writer = Some(Box::new(psf::PsfCodec::default()));
        }
    }
    /// Configure bounded PSF1/PSF2 output and Unicode-table policy.
    pub fn set_psf_write_options(&mut self, options: psf::PsfWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "psf" && e.writer.is_some())
        {
            e.writer = Some(Box::new(psf::PsfCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "image")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_image(&mut self) {
        if let Some(e)=self.entries.iter_mut().find(|e|e.format.id=="image") {
            e.detector=Some(Box::new(image::ImageCodec::default()));
            e.reader=Some(Box::new(image::ImageCodec::default()));
            e.writer=Some(Box::new(image::ImageCodec::default()));
        }
    }
    /// Select an explicit image mapping or caller-scoped mapping asset.
    pub fn set_image_read_options(&mut self, options:image::ImageReadOptions) {
        if let Some(e)=self.entries.iter_mut().find(|e|e.format.id=="image" && e.reader.is_some()) {
            e.reader=Some(Box::new(image::ImageCodec{read_options:options,..Default::default()}));
        }
    }
    /// Configure atlas image container, packing and mapping serialization.
    pub fn set_image_write_options(&mut self, options:image::ImageWriteOptions) {
        if let Some(e)=self.entries.iter_mut().find(|e|e.format.id=="image" && e.writer.is_some()) {
            e.writer=Some(Box::new(image::ImageCodec{write_options:options,..Default::default()}));
        }
    }
}
