//! Historical COM font payloads and original executable installation programs.
use super::w5a2_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
mod stubs;
/// Historical loader envelope, independent from font payload contents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ComfontVariant {
    /// PC Magazine FONTEDIT wrapper.
    Fontedit,
    /// Fontraption plain DOS loader.
    #[default]
    Frapt,
    /// Resident DOS loader, reinstalls after text mode changes.
    FraptTsr,
    /// PCW LETAFONT wrapper.
    Letafont,
    /// REXXCOM Font Mania wrapper.
    Mania,
    /// PSF2AMS polyglot wrapper with native CP/M installer.
    Psfcom,
    /// PCW UDG wrapper.
    Udg,
}
/// CP/M machine whose documented native font memory is installed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CpmTarget {
    /// Amstrad PCW screen environment, B800h.
    #[default]
    Pcw,
    /// Amstrad CPC CP/M Plus bank zero, 8000h.
    Cpc,
    /// Spectrum +3 CP/M Plus bank zero, 3000h.
    Spectrum,
}
/// Native payload interpretation; no input executable is run.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ComfontReadOptions {
    /// Require an envelope variant; default checks signatures.
    pub variant: Option<ComfontVariant>,
    /// Baseline below the cell top.
    pub baseline: Option<i64>,
    /// Explicit native legacy encoding metadata.
    pub charset: Option<String>,
}
/// Original executable output profile.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ComfontWriteOptions {
    /// Historical envelope.
    pub variant: ComfontVariant,
    /// CP/M installation target (CP/M variants only).
    pub cpm_target: CpmTarget,
    /// Optional native cell rectangle.
    pub cell: Option<Bounds>,
    /// Explicit Gray8 threshold.
    pub gray_threshold: Option<u8>,
    /// Permit numeric Unicode-to-legacy projection.
    pub unicode_as_legacy: bool,
    /// Aggregate source/output/readback bounds.
    pub limits: ResourceLimits,
}
impl ComfontWriteOptions {
    fn native(&self) -> NativeWriteOptions {
        NativeWriteOptions {
            cell: self.cell,
            gray_threshold: self.gray_threshold,
            unicode_as_legacy: self.unicode_as_legacy,
            limits: self.limits.clone(),
        }
    }
}
/// Native executable font codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ComfontCodec {
    /// Input choices.
    pub read_options: ComfontReadOptions,
    /// Output choices.
    pub write_options: ComfontWriteOptions,
}
impl FormatDetector for ComfontCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for ComfontCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for ComfontCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
const FONTEDIT_MAGIC: &[u8] = b"\xeb\x33\x90\r   \r\n PC Magazine \xfe Michael J. Mefford\0\x1a";
const LETAFONT_MAGIC: &[u8] = &[
    0x21, 0x0e, 1, 0x11, 0x90, 0xe2, 1, 0x64, 8, 0xed, 0xb0, 0xc3, 0x90, 0xe2, 0x2a, 1,
];
const UDG_MAGIC: &[u8] = &[
    0x21, 0x0e, 1, 0x11, 0x2c, 0xe2, 1, 0x34, 8, 0xed, 0xb0, 0xc3, 0x2c, 0xe2, 0x2a, 1,
];
const PSFCOM8: &[u8] = b"\rFont converted with PSF2AMS\r\n\x1a";
const PSFCOM16: &[u8] = b"\rFont Converted with PSF2AMS\r\n\x1a";
fn variant(s: &[u8]) -> Result<ComfontVariant> {
    if s.starts_with(FONTEDIT_MAGIC) {
        Ok(ComfontVariant::Fontedit)
    } else if s.starts_with(b"VILE\x1a") {
        Ok(ComfontVariant::Frapt)
    } else if s.starts_with(b"\xe9\x60") && s.get(40..45) == Some(b"VILE\x1a") {
        Ok(ComfontVariant::FraptTsr)
    } else if s.get(8..27) == Some(b"FONT MANIA, VERSION") {
        Ok(ComfontVariant::Mania)
    } else if s.starts_with(LETAFONT_MAGIC) {
        Ok(ComfontVariant::Letafont)
    } else if s.starts_with(UDG_MAGIC) {
        Ok(ComfontVariant::Udg)
    } else if s.starts_with(b"\xeb\x04\xeb\xc3")
        && (s.get(15..46) == Some(PSFCOM8) || s.get(15..46) == Some(PSFCOM16))
    {
        Ok(ComfontVariant::Psfcom)
    } else {
        Err(invalid("unrecognized COM font envelope"))
    }
}
/// Recognize native signatures and minimally validate their font spans.
pub fn detect(s: &[u8]) -> Confidence {
    if layout(s, None).is_ok() {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn layout(
    s: &[u8],
    required: Option<ComfontVariant>,
) -> Result<(ComfontVariant, usize, u8, usize)> {
    if s.len() > 0xff00 {
        return Err(invalid("COM file exceeds 16-bit transient program space"));
    }
    let v = variant(s)?;
    if required.is_some_and(|x| x != v) {
        return Err(invalid("COM envelope disagrees with required variant"));
    }
    let (offset, height, count) = match v {
        ComfontVariant::Fontedit => (
            99,
            *s.get(50)
                .ok_or_else(|| invalid("truncated FONTEDIT header"))?,
            256,
        ),
        ComfontVariant::Frapt => (
            25,
            *s.get(21)
                .ok_or_else(|| invalid("truncated Fontraption header"))?,
            256,
        ),
        ComfontVariant::FraptTsr => (
            99,
            *s.get(93).ok_or_else(|| invalid("truncated TSR header"))?,
            256,
        ),
        ComfontVariant::Letafont => (44, 8, 256),
        ComfontVariant::Udg => (114, 8, 256),
        ComfontVariant::Mania => {
            let off = usize::from(u16le(s, 2)?);
            let size = usize::from(u16le(s, 4)?);
            if off < 70 || size % 256 != 0 {
                return Err(invalid("invalid Font Mania offset or size"));
            }
            (off, (size / 256) as u8, 256)
        }
        ComfontVariant::Psfcom => {
            let h = if s.get(15..46) == Some(PSFCOM16) {
                16
            } else {
                8
            };
            let address = usize::from(u16le(s, 48)?);
            let size = usize::from(u16le(s, 46)?);
            if address < 0x100 + 52 || size == 0 || size % h != 0 || size / h > 512 {
                return Err(invalid("invalid PSFCOM font address or size"));
            }
            (address - 0x100, h as u8, size / h)
        }
    };
    if !(1..=32).contains(&height) {
        return Err(invalid("COM font height outside 1..32"));
    }
    take(s, offset, count * usize::from(height))?;
    if matches!(v, ComfontVariant::Frapt | ComfontVariant::Psfcom)
        && s.len() != offset + count * usize::from(height)
    {
        return Err(invalid("unexpected bytes after COM font payload"));
    }
    Ok((v, offset, height, count))
}
fn parse(
    s: &[u8],
    o: &ComfontReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "COM input bytes")?;
    let (_, offset, height, count) = layout(s, o.variant)?;
    check(count, l.max_glyphs, "COM glyph count")?;
    b.add(o.charset.as_ref().map_or(0, String::len))?;
    let opts = NativeReadOptions {
        baseline: o.baseline,
        charset: o.charset.clone(),
    };
    let mut f = base("comfont", &opts, b)?;
    let end = offset + count * usize::from(height);
    for (i, raw) in s[offset..end].chunks_exact(usize::from(height)).enumerate() {
        glyph(
            &mut f,
            Codepoint::Legacy(i as u32),
            8,
            u32::from(height),
            raw,
            &opts,
            l,
            b,
        )?;
    }
    for (name, bytes) in [
        ("comfont.prefix", &s[..offset]),
        ("comfont.suffix", &s[end..]),
    ] {
        b.add(bytes.len().saturating_mul(2) + 256)?;
        f.properties
            .push(Property::new(name, PropertyValue::Bytes(bytes.to_vec())));
    }
    Ok((complete(f, "comfont")?, Report::default()))
}
/// Decode only native font data; executable bytes are retained as opaque properties.
pub fn read(c: &mut ReadContext<'_>, o: &ComfontReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
/// Create an original loader that installs the font on the selected native target.
pub fn write(s: &FontSet, o: &WriteOptions, p: &ComfontWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let native = p.native();
        let mut b = source(s, &native)?;
        b.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let source = convert_gray(s, native.gray_threshold, &p.limits, &mut b)?;
        let f = source.single()?;
        let mut c = cell(f, &native, None)?;
        if p.cell.is_some_and(|v| v.width != 8) {
            return Err(invalid("COM cell width must be eight"));
        }
        c.width = 8;
        let cp_m = matches!(
            p.variant,
            ComfontVariant::Letafont | ComfontVariant::Udg | ComfontVariant::Psfcom
        );
        if !cp_m && p.cpm_target != CpmTarget::Pcw {
            return Err(invalid("CP/M target is inapplicable to DOS wrappers"));
        }
        match p.variant {
            ComfontVariant::Letafont | ComfontVariant::Udg => {
                if p.cell.is_some_and(|v| v.height != 8) {
                    return Err(invalid("LETAFONT and UDG cells are eight rows"));
                }
                c.height = 8;
            }
            ComfontVariant::Psfcom => {
                let h = if c.height <= 8 { 8 } else { 16 };
                if c.height > 16 || p.cell.is_some_and(|v| v.height != h) {
                    return Err(invalid("PSFCOM height must be eight or sixteen"));
                }
                c.height = h;
            }
            _ => {
                if c.height > 32 {
                    return Err(invalid("DOS VGA character height exceeds 32"));
                }
            }
        }
        let max = if p.variant == ComfontVariant::Psfcom && p.cpm_target == CpmTarget::Spectrum {
            511
        } else {
            255
        };
        let map = legacy_map(f, &native, max, &mut b)?;
        let count = if max == 511 && map.keys().any(|v| *v >= 256) {
            512
        } else {
            256
        };
        if p.variant == ComfontVariant::Psfcom && c.height == 16 {
            b.loss(
                &mut report,
                "comfont",
                "installation.scanlines",
                Some(0),
                None,
                "Selected 8-row CP/M target installs alternate scanlines of the native 16-row payload",
            )?;
        }
        if p.variant == ComfontVariant::Psfcom
            && p.cpm_target == CpmTarget::Spectrum
            && count == 512
        {
            b.loss(
                &mut report,
                "comfont",
                "installation.narrow-font",
                Some(0),
                None,
                "Spectrum second font bank is displayed as 5x8 cells although native payload rows retain eight bits",
            )?;
        }
        let mut rows = allocated(count * c.height as usize, &mut b)?;
        for (i, raw) in rows.chunks_exact_mut(c.height as usize).enumerate() {
            raw.copy_from_slice(
                raster(map.get(&(i as u32)).copied(), c, &p.limits, &mut b)?.as_bytes(),
            );
        }
        let bytes = stubs::wrap(&rows, c.height as u8, p.variant, p.cpm_target, &mut b)?;
        output_check(bytes.len(), &p.limits, &mut b)?;
        let (back, r) = parse(&bytes, &Default::default(), &p.limits, &mut b)?;
        report.extend(r);
        audit(s, &back, "comfont", &mut report, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), bytes))
    })();
    finish(result, report, "comfont", o.strict)
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&ComfontVariant::Fontedit), "fontedit");
        assert_eq!(round_trip(&ComfontVariant::Frapt), "frapt");
        assert_eq!(round_trip(&ComfontVariant::FraptTsr), "frapt_tsr");
        assert_eq!(round_trip(&ComfontVariant::Letafont), "letafont");
        assert_eq!(round_trip(&ComfontVariant::Mania), "mania");
        assert_eq!(round_trip(&ComfontVariant::Psfcom), "psfcom");
        assert_eq!(round_trip(&ComfontVariant::Udg), "udg");
        rejects::<ComfontVariant>(r#""not_a_variant""#);
        assert_eq!(round_trip(&CpmTarget::Pcw), "pcw");
        assert_eq!(round_trip(&CpmTarget::Cpc), "cpc");
        assert_eq!(round_trip(&CpmTarget::Spectrum), "spectrum");
        rejects::<CpmTarget>(r#""not_a_variant""#);
        defaults::<ComfontReadOptions>();
        defaults::<ComfontWriteOptions>();
        defaults::<ComfontCodec>();
    }
}
