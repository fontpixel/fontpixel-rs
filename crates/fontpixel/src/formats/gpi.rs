//! OS/2 GPI bitmap records and NE/LX resource modules.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Mono};
use crate::model::{
    AbcWidths, Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Rational, Spacing, Vector,
};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};
mod container;
mod reader;
mod writer;
/// Native GPI resource packaging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GpiContainer {
    /// One raw font record stream.
    Bare,
    /// OS/2 16-bit resource-only NE module.
    Ne,
    /// OS/2 32-bit resource-only LX module.
    Lx,
}
/// Native character increment representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GpiKind {
    /// Type 1, fixed increment.
    Fixed,
    /// Type 2, per-character width and increment.
    Proportional,
    /// Type 3, signed A/B/C spacing.
    Abc,
}
/// Native optional-glyph policy.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct GpiReadOptions {
    /// Retain the extra unmapped .null descriptor when the table has one.
    pub include_unmapped_null: bool,
}
impl Default for GpiReadOptions {
    fn default() -> Self {
        Self {
            include_unmapped_null: true,
        }
    }
}
/// Bounded native output policy.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct GpiWriteOptions {
    /// Container; absent preserves source, otherwise bare for one strike and LX for many.
    pub container: Option<GpiContainer>,
    /// Native increment kind; absent preserves source, otherwise type 3.
    pub kind: Option<GpiKind>,
    /// OS/2 code-page field; zero and 850 identify UGL without reinterpreting codes.
    pub codepage: Option<u16>,
    /// Registered native font identifier.
    pub registry_id: Option<u16>,
    /// Reuse bytes only after a full same-limit model comparison.
    pub preserve_native: bool,
    /// Explicit permission to use Unicode numbers as native legacy codes.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale threshold.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for GpiWriteOptions {
    fn default() -> Self {
        Self {
            container: None,
            kind: None,
            codepage: None,
            registry_id: None,
            preserve_native: true,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Native registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct GpiCodec {
    /// Input policy.
    pub read_options: GpiReadOptions,
    /// Output policy.
    pub write_options: GpiWriteOptions,
}
impl FormatDetector for GpiCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        let b = c.bytes();
        Ok(
            if b.starts_with(&[254, 255, 255, 255]) && b.get(8..17) == Some(b"OS/2 FONT") {
                Confidence::Certain
            } else if container::kind(b).is_ok() {
                Confidence::Possible
            } else {
                Confidence::NoMatch
            },
        )
    }
}
impl FormatReader<FontSet> for GpiCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for GpiCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
fn u16v(b: &[u8], at: usize) -> Result<u16> {
    u16_at(b, at, ByteOrder::Little)
}
fn i16v(b: &[u8], at: usize) -> Result<i64> {
    Ok(i64::from(u16v(b, at)? as i16))
}
fn u32v(b: &[u8], at: usize) -> Result<u32> {
    u32_at(b, at, ByteOrder::Little)
}
fn p16(b: &mut [u8], at: usize, v: u16) {
    put16(b, at, v, ByteOrder::Little)
}
fn p32(b: &mut [u8], at: usize, v: usize) -> Result<()> {
    put32(
        b,
        at,
        u32::try_from(v).map_err(|_| invalid("GPI field exceeds dword"))?,
        ByteOrder::Little,
    );
    Ok(())
}
fn ps(b: &mut [u8], at: usize, v: i64) -> Result<()> {
    p16(
        b,
        at,
        i16::try_from(v).map_err(|_| invalid("GPI metric exceeds signed word"))? as u16,
    );
    Ok(())
}
/// Decode native records, byte-column rasters and bounded OS/2 resource pages.
pub fn read(c: &mut ReadContext<'_>, o: &GpiReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    reader::parse(c.bytes(), o, c.limits(), &mut b)
}
/// Write native records or resource-only modules and audit a same-limit reread.
pub fn write(s: &FontSet, w: &WriteOptions, o: &GpiWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = writer::encode(s, w, o, &mut r);
    finish(result, r, "gpi", w.strict)
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&GpiContainer::Bare), "bare");
        assert_eq!(round_trip(&GpiContainer::Ne), "ne");
        assert_eq!(round_trip(&GpiContainer::Lx), "lx");
        rejects::<GpiContainer>(r#""not_a_variant""#);
        assert_eq!(round_trip(&GpiKind::Fixed), "fixed");
        assert_eq!(round_trip(&GpiKind::Proportional), "proportional");
        assert_eq!(round_trip(&GpiKind::Abc), "abc");
        rejects::<GpiKind>(r#""not_a_variant""#);
        defaults::<GpiReadOptions>();
        defaults::<GpiWriteOptions>();
        defaults::<GpiCodec>();
    }
}
