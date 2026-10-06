//! Explicit global metadata for native OTB/bdat output.
//!
//! Every control is opt-in; [`EmbeddedMetadataOptions::default`] reproduces the
//! native tables byte for byte. No value is inferred from family names, vendors
//! or other application conventions: callers state each field.
use super::{Diagnostics, invalid, loss, name, note, os2, sbit};
use crate::Result;
use crate::model::{FontNames, FontSet};
use std::sync::Arc;

/// Arithmetic for OS/2 `xAvgCharWidth` over nonzero design advances.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedAverageWidth {
    /// Nearest integer, halfway values rounded up (native default).
    #[default]
    Nearest,
    /// Integer floor of the mean.
    Floor,
}
/// Whether the post table carries glyph names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedPostNames {
    /// Format 2 with one name per glyph (native default).
    #[default]
    Keep,
    /// Nameless format 3. Source glyph names that would otherwise be written are
    /// reported as loss.
    Omit,
}
/// OS/2 `ulUnicodeRange1..4` policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedUnicodeRanges {
    /// Every range bit covered by the actual cmap (native default).
    #[default]
    Coverage,
    /// Only bit 57 (non-plane-0), set exactly when the cmap maps a
    /// supplementary-plane scalar. All block bits are left clear.
    NonBmpOnly,
    /// Exact caller bits. Reserved bits 123..=127 must be clear.
    Exact([u32; 4]),
}
/// Full-repertoire cmap records: `Always` (native default) lists format 12
/// under (0, 4) and (3, 10); `NonBmpOnly` omits the redundant (0, 4) record
/// for BMP-only maps; `WhenNeeded` writes format 12 only when format 4 cannot
/// hold the map, under (3, 10) only. Shared with outline export.
pub type EmbeddedCmapUnicodeFull = crate::sfnt::cmap::FullRecords;
/// Source of the EBLC/bloc horizontal `widthMax` line metric.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedWidthMax {
    /// Greatest physical bitmap width in the strike (native default).
    #[default]
    BitmapWidth,
    /// Greatest horizontal pixel advance in the strike. Glyph images, metrics
    /// and the other line metrics are unchanged.
    Advance,
}
/// EBLC/bloc index subtable organisation; never changes decoded glyphs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(rename_all = "snake_case", deny_unknown_fields)
)]
pub enum EmbeddedIndexLayout {
    /// Contiguous same-format runs in index format 1 with byte-aligned
    /// images (native default).
    #[default]
    Canonical,
    /// Runs of at least `minimum_run` consecutive glyph IDs with identical
    /// small metrics and nonempty images use index format 2/image format 5;
    /// all other glyphs use index format 3/image format 2, both bit-aligned.
    /// Requires horizontal-only small metrics; run metrics carry zero vertical
    /// fields, which is reported.
    SharedMetricRuns {
        /// Shortest run stored with shared metrics, at least 1.
        minimum_run: u16,
    },
    /// Like `Canonical`, but one index subtable spans glyph IDs that are absent
    /// from the strike, giving them zero-length entries (allowed for index
    /// format 1), so a strike with gaps needs one subtable per image format run.
    Spanning,
}
/// Optional TrueType outline tables of a bitmap-only font.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedOutlineTables {
    /// No `glyf`/`loca`; `maxp` version 0.5 (native OTB default).
    #[default]
    None,
    /// Empty `glyf` (one zero byte, as OTS and Windows require of an all-empty
    /// table) with a short `loca` and `maxp` version 1.0. Every outline is
    /// empty, so the outline-derived fields follow: `head` bounds, hmtx left
    /// side bearings and the hhea extrema are zero. Bitmap metrics are
    /// unchanged in the strikes.
    EmptyGlyf,
}
/// Glyph 0 of the bitmap font.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(rename_all = "snake_case", deny_unknown_fields)
)]
pub enum EmbeddedNotdef {
    /// The source `.notdef` when present, otherwise an empty zero-advance
    /// bitmap in every strike (native default).
    #[default]
    Preserve,
    /// No bitmap in any strike and this hmtx advance in design units. Replacing
    /// a source `.notdef` glyph is reported as loss.
    Empty {
        /// hmtx advance width in design units.
        advance_width: u16,
    },
}
/// EBLC/bloc `minOriginSB`, `minAdvanceSB`, `maxBeforeBL`, `minAfterBL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedLineExtremes {
    /// Computed from the strike's glyph metrics (native default).
    #[default]
    Computed,
    /// Written as zero (no extreme information).
    Zero,
}
/// EBLC/bloc vertical line metrics of strikes without vertical glyph metrics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedVerticalLineMetrics {
    /// Zero (native default).
    #[default]
    Zero,
    /// A copy of the horizontal record. Strikes with vertical glyph metrics
    /// keep their computed record.
    MirrorHorizontal,
}
/// hmtx storage of trailing glyphs that share the last advance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedHmtx {
    /// One long metric per glyph (native default).
    #[default]
    Full,
    /// Trailing glyphs whose advance equals the last long metric store only
    /// their bearing; decoded advances are identical.
    CompactTrailing,
}
/// Glyph ID order of the bitmap glyphs (glyph 0 stays `.notdef`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EmbeddedGlyphOrder {
    /// First appearance across the FontSet's strikes (native default).
    #[default]
    FirstSeen,
    /// Encoded glyphs in ascending order of their smallest Unicode scalar,
    /// then unencoded glyphs in first-seen order.
    Codepoint,
}
/// Explicit head, OS/2, post, name and strike-table controls for OTB/bdat.
///
/// An explicit name table replaces the generated table completely: no IDs are
/// added, empty records stay records, and the table is encoded without string
/// sharing. Source models are never modified. Applying any control produces an
/// `Info` audit entry; source name fields absent from an explicit table and
/// omitted source glyph names are reported as loss. Color output rejects any
/// nondefault value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct EmbeddedMetadataOptions {
    /// Complete name table. The `Arc` lets registry and transport clones share
    /// one allocation, which the writer charges once to its own budget.
    #[cfg_attr(feature = "serde", serde(with = "shared_names"))]
    pub names: Option<Arc<name::NameTable>>,
    /// Exact `head.flags`; reserved bit 15 must be clear.
    pub head_flags: Option<u16>,
    /// `head.lowestRecPPEM`, at least 1.
    pub lowest_rec_ppem: Option<u16>,
    /// `head.created` and `head.modified`, seconds since 1904-01-01 00:00 UTC.
    /// Absent writes zero (deterministic native default).
    pub timestamps: Option<[i64; 2]>,
    /// OS/2 table version 3 or 4 (identical layout; native default 4).
    pub os2_version: Option<u16>,
    /// Four printable-ASCII bytes for `achVendID` (native default `NONE`).
    pub vendor_id: Option<[u8; 4]>,
    /// Ten PANOSE bytes (native default all zero).
    pub panose: Option<[u8; 10]>,
    /// `ulUnicodeRange1..4` policy.
    pub unicode_ranges: EmbeddedUnicodeRanges,
    /// `usBreakChar` (native default 0).
    pub break_char: Option<u16>,
    /// `fsType` embedding permissions; only defined bits, at most one usage
    /// permission among bits 1..=3.
    pub embedding_flags: Option<u16>,
    /// `fsSelection`; bits 10..=15 must be clear, and bits 7..=9 require
    /// OS/2 version 4.
    pub selection_flags: Option<u16>,
    /// `xAvgCharWidth` arithmetic; advances themselves are unchanged.
    pub average_width: EmbeddedAverageWidth,
    /// Post glyph names (format 2) or nameless format 3.
    pub post_names: EmbeddedPostNames,
    /// `post.isFixedPitch` override; absent follows the source spacing.
    pub fixed_pitch: Option<bool>,
    /// cmap (0, 4) record policy for BMP-only maps.
    pub cmap_unicode_full: EmbeddedCmapUnicodeFull,
    /// EBLC/bloc horizontal `widthMax` source.
    pub width_max: EmbeddedWidthMax,
    /// EBLC/bloc index subtable organisation.
    pub index_layout: EmbeddedIndexLayout,
    /// Optional empty TrueType outline tables.
    pub outline_tables: EmbeddedOutlineTables,
    /// `maxp.maxZones` (1 or 2) with [`EmbeddedOutlineTables::EmptyGlyf`];
    /// absent writes 1 (no instructions use the twilight zone).
    pub max_zones: Option<u16>,
    /// Glyph 0 policy.
    pub notdef: EmbeddedNotdef,
    /// EBLC/bloc caret slope numerator and denominator for every strike's
    /// horizontal record (native default 1, 0: upright).
    pub caret_slope: Option<[i8; 2]>,
    /// EBLC/bloc line-metric extremes.
    pub line_extremes: EmbeddedLineExtremes,
    /// EBLC/bloc vertical record of horizontal-only strikes.
    pub vertical_line_metrics: EmbeddedVerticalLineMetrics,
    /// `post.underlinePosition` and `underlineThickness` in design units.
    /// Replacing a source underline metric is reported as loss.
    pub underline: Option<[i16; 2]>,
    /// hmtx storage.
    pub hmtx: EmbeddedHmtx,
    /// Glyph ID order.
    pub glyph_order: EmbeddedGlyphOrder,
}
/// Serde for the shared name table: the `Arc` is transparent, so the table
/// serializes as its contents and each deserialized table gets a fresh `Arc`.
#[cfg(feature = "serde")]
mod shared_names {
    use super::{Arc, name::NameTable};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(
        value: &Option<Arc<NameTable>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_deref().serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Arc<NameTable>>, D::Error> {
        Ok(Option::<NameTable>::deserialize(deserializer)?.map(Arc::new))
    }
}

/// Bytes of an `Arc` allocation header (strong and weak counts).
const ARC_HEADER: usize = 2 * std::mem::size_of::<usize>();

/// Name-table work prepared before the native writer runs.
pub(super) struct Prepared {
    /// Remaining writer budget after the retained configuration and output.
    pub limits: Option<sbit::Limits>,
    /// Encoded explicit name table.
    pub names: Option<Vec<u8>>,
}

impl EmbeddedMetadataOptions {
    /// Reject impossible or reserved field values. Color output accepts only
    /// the default configuration.
    pub fn validate(&self, kind: sbit::Kind) -> Result<()> {
        if kind == sbit::Kind::Color && self != &Self::default() {
            return Err(invalid("custom embedded metadata requires otb or bdat"));
        }
        if self.head_flags.is_some_and(|flags| flags & 0x8000 != 0) {
            return Err(invalid("head flags bit 15 is reserved"));
        }
        if self.lowest_rec_ppem == Some(0) {
            return Err(invalid("lowest recommended ppem must be positive"));
        }
        if let Some(zones) = self.max_zones {
            if !(1..=2).contains(&zones) {
                return Err(invalid("maxZones must be 1 or 2"));
            }
            if self.outline_tables != EmbeddedOutlineTables::EmptyGlyf {
                return Err(invalid("maxZones requires the empty glyf outline tables"));
            }
        }
        if self.caret_slope == Some([0, 0]) {
            return Err(invalid(
                "caret slope numerator and denominator cannot both be 0",
            ));
        }
        if self
            .os2_version
            .is_some_and(|version| !(3..=4).contains(&version))
        {
            return Err(invalid("OS/2 version must be 3 or 4"));
        }
        if self
            .vendor_id
            .is_some_and(|id| !id.iter().all(|byte| (0x20..=0x7e).contains(byte)))
        {
            return Err(invalid("OS/2 vendor ID must be four printable ASCII bytes"));
        }
        if let EmbeddedUnicodeRanges::Exact(ranges) = self.unicode_ranges
            && ranges[3] & 0xf800_0000 != 0
        {
            return Err(invalid("OS/2 Unicode range bits 123..=127 are reserved"));
        }
        if let Some(flags) = self.embedding_flags {
            os2::check_embedding_flags(flags).map_err(invalid)?;
        }
        if let Some(flags) = self.selection_flags {
            os2::check_selection_flags(flags, self.os2_version.unwrap_or(4)).map_err(invalid)?;
        }
        if self.index_layout == (EmbeddedIndexLayout::SharedMetricRuns { minimum_run: 0 }) {
            return Err(invalid("shared-metric run length must be positive"));
        }
        Ok(())
    }
    /// Retained bytes of caller-owned configuration (the explicit name table
    /// and its `Arc` header), for callers that reserve configuration storage.
    pub fn retained_bytes(&self, maximum: usize) -> Result<usize> {
        let Some(table) = &self.names else {
            return Ok(0);
        };
        table
            .retained_bytes(maximum)?
            .checked_add(ARC_HEADER)
            .filter(|n| *n <= maximum)
            .ok_or(crate::Error::ResourceLimit {
                resource: "embedded metadata bytes",
                limit: maximum,
            })
    }
    /// Encode an explicit name table while the source model, the retained
    /// table and its encoding scratch coexist; then reserve the configuration
    /// and encoded output for the rest of the writer. Without explicit names
    /// the writer budget is untouched.
    /// A model the caller has reserved (`charge`) is not charged again.
    pub(super) fn prepare(
        &self,
        fonts: &FontSet,
        limits: &sbit::Limits,
        charge: crate::formats::ModelCharge,
    ) -> Result<Prepared> {
        let maximum = limits.sfnt.resources.max_total_bytes;
        let error = || crate::Error::ResourceLimit {
            resource: "embedded metadata bytes",
            limit: maximum,
        };
        let extra = self.working_bytes(fonts).ok_or_else(error)?;
        let Some(table) = &self.names else {
            if extra == 0 {
                return Ok(Prepared {
                    limits: None,
                    names: None,
                });
            }
            let mut remaining = limits.clone();
            remaining.sfnt.resources.max_total_bytes =
                maximum.checked_sub(extra).ok_or_else(error)?;
            return Ok(Prepared {
                limits: Some(remaining),
                names: None,
            });
        };
        let configured = self.retained_bytes(maximum)?;
        let available = maximum.checked_sub(configured).ok_or_else(error)?;
        let source = match charge {
            crate::formats::ModelCharge::Estimate => fonts.retained_usage(available)?.bytes,
            crate::formats::ModelCharge::Reserved => 0,
        };
        let mut encoding = limits.sfnt.clone();
        encoding.resources.max_total_bytes = maximum
            .checked_sub(source)
            .and_then(|n| n.checked_sub(ARC_HEADER))
            .ok_or_else(error)?;
        let bytes = table.encode_configured(&encoding)?;
        let mut remaining = limits.clone();
        remaining.sfnt.resources.max_total_bytes = maximum
            .checked_sub(configured)
            .and_then(|n| n.checked_sub(bytes.capacity()))
            .and_then(|n| n.checked_sub(extra))
            .ok_or_else(error)?;
        Ok(Prepared {
            limits: Some(remaining),
            names: Some(bytes),
        })
    }
    /// Upper bound of the writer storage these controls add, reserved before
    /// the writer runs: the empty outline tables (one `glyf` byte, a short
    /// `loca` of `glyphs + 1` entries, a 32-byte `maxp`) and the codepoint
    /// reordering scratch (one sort key and one new ID per glyph, plus the
    /// reordered name array). `glyphs` is bounded by every source glyph plus
    /// `.notdef`, at most 65,536. `None` on overflow.
    pub(super) fn working_bytes(&self, fonts: &FontSet) -> Option<usize> {
        use std::mem::size_of;
        let glyphs = fonts
            .fonts
            .iter()
            .try_fold(1usize, |n, font| n.checked_add(font.len()))?
            .min(65_536);
        let mut extra = 0usize;
        if self.outline_tables == EmbeddedOutlineTables::EmptyGlyf {
            extra = glyphs.checked_add(1)?.checked_mul(2)?.checked_add(1 + 32)?;
        }
        if self.glyph_order == EmbeddedGlyphOrder::Codepoint {
            let per_glyph = size_of::<(u32, u16)>() + size_of::<u16>() + size_of::<String>();
            extra = extra.checked_add(glyphs.checked_mul(per_glyph)?)?;
        }
        Some(extra)
    }
    /// Record the explicit configuration and every source value it drops.
    pub(super) fn audit(&self, fonts: &FontSet, report: &mut Diagnostics) {
        if self == &Self::default() {
            return;
        }
        note(
            report,
            "metadata_configuration",
            "Explicit OTB/bdat metadata controls applied; source model unchanged",
        );
        if let (Some(table), Some(first)) = (&self.names, fonts.fonts.first()) {
            for (id, value) in source_names(&first.names) {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    continue;
                };
                let retained = table.records.iter().any(|record| {
                    record.name_id == id && record.text().ok().flatten().as_deref() == Some(value)
                });
                if !retained {
                    loss(
                        report,
                        "metadata_name",
                        Some(0),
                        None,
                        format!(
                            "Explicit name table omits or replaces source name ID {id}; \
                             source model unchanged"
                        ),
                    );
                }
            }
        }
        if self.post_names == EmbeddedPostNames::Omit {
            let named = fonts
                .fonts
                .iter()
                .flat_map(|font| font.glyphs().map(|(_, glyph)| glyph))
                .filter(|glyph| {
                    glyph
                        .name
                        .as_deref()
                        .is_some_and(|name| !name.is_empty() && name != ".notdef")
                })
                .count();
            if named > 0 {
                loss(
                    report,
                    "post_names",
                    None,
                    None,
                    format!("Nameless post format 3 omits {named} source glyph names"),
                );
            }
        }
        if let EmbeddedNotdef::Empty { .. } = self.notdef {
            for (fi, font) in fonts.fonts.iter().enumerate() {
                if let Some((id, _)) = font.glyphs().find(|(_, glyph)| {
                    // The same rule that assigns a source glyph to glyph 0.
                    glyph.name.as_deref() == Some(".notdef")
                        && !glyph.codepoints.iter().any(|cp| {
                            matches!(cp, crate::model::Codepoint::Unicode(c)
                                if char::from_u32(*c).is_some())
                        })
                }) {
                    loss(
                        report,
                        "notdef_replaced",
                        Some(fi),
                        Some(id.index()),
                        "The source .notdef glyph was replaced by the explicit empty .notdef \
                         policy; source model unchanged",
                    );
                }
            }
        }
    }
    /// OS/2 average width under the selected arithmetic.
    pub(super) fn average(&self, advances: impl Iterator<Item = u16>) -> Result<i16> {
        if self.average_width == EmbeddedAverageWidth::Nearest {
            return os2::average_width(advances);
        }
        let (mut sum, mut count) = (0u64, 0u64);
        for advance in advances.filter(|n| *n != 0) {
            sum = sum
                .checked_add(u64::from(advance))
                .ok_or_else(|| invalid("average width overflow"))?;
            count += 1;
        }
        i16::try_from(sum.checked_div(count).unwrap_or(0))
            .map_err(|_| invalid("average width exceeds int16"))
    }
    /// Apply OS/2 overrides after native derivation.
    pub(super) fn apply_os2(&self, table: &mut os2::Os2Table, non_bmp: bool) -> Result<()> {
        if let Some(version) = self.os2_version {
            table.version = version;
        }
        if let Some(id) = self.vendor_id {
            table.base.vendor_id = id;
        }
        if let Some(panose) = self.panose {
            table.base.panose = panose;
        }
        match self.unicode_ranges {
            EmbeddedUnicodeRanges::Coverage => {}
            EmbeddedUnicodeRanges::NonBmpOnly => {
                table.base.unicode_ranges = [0, if non_bmp { 1 << 25 } else { 0 }, 0, 0];
            }
            EmbeddedUnicodeRanges::Exact(ranges) => table.base.unicode_ranges = ranges,
        }
        if let Some(flags) = self.embedding_flags {
            table.base.embedding_flags = flags;
        }
        if let Some(flags) = self.selection_flags {
            table.base.selection_flags = flags;
        }
        if let Some(value) = self.break_char {
            table
                .extension
                .as_mut()
                .ok_or_else(|| invalid("OS/2 break character requires version 2 or later"))?
                .break_char = value;
        }
        Ok(())
    }
    /// Apply head overrides to the native 54-byte table.
    pub(super) fn apply_head(&self, head: &mut [u8]) {
        if let Some(flags) = self.head_flags {
            head[16..18].copy_from_slice(&flags.to_be_bytes());
        }
        if let Some([created, modified]) = self.timestamps {
            head[20..28].copy_from_slice(&created.to_be_bytes());
            head[28..36].copy_from_slice(&modified.to_be_bytes());
        }
        if let Some(ppem) = self.lowest_rec_ppem {
            head[46..48].copy_from_slice(&ppem.to_be_bytes());
        }
    }
}

/// Source name fields that the native default writes, by name ID.
fn source_names(names: &FontNames) -> [(u16, Option<&str>); 9] {
    [
        (0, names.copyright.as_deref()),
        (1, names.family.as_deref()),
        (2, names.style.as_deref()),
        (4, names.full.as_deref()),
        (5, names.version.as_deref()),
        (6, names.postscript.as_deref()),
        (8, names.foundry.as_deref()),
        (13, names.license.as_deref()),
        (14, names.license_url.as_deref()),
    ]
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&EmbeddedAverageWidth::Nearest), "nearest");
        assert_eq!(round_trip(&EmbeddedAverageWidth::Floor), "floor");
        rejects::<EmbeddedAverageWidth>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedPostNames::Keep), "keep");
        assert_eq!(round_trip(&EmbeddedPostNames::Omit), "omit");
        rejects::<EmbeddedPostNames>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedUnicodeRanges::Coverage), "coverage");
        assert_eq!(
            round_trip(&EmbeddedUnicodeRanges::NonBmpOnly),
            "non_bmp_only"
        );
        rejects::<EmbeddedUnicodeRanges>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedWidthMax::BitmapWidth), "bitmap_width");
        assert_eq!(round_trip(&EmbeddedWidthMax::Advance), "advance");
        rejects::<EmbeddedWidthMax>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedIndexLayout::Canonical), "canonical");
        assert_eq!(round_trip(&EmbeddedIndexLayout::Spanning), "spanning");
        rejects::<EmbeddedIndexLayout>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedOutlineTables::None), "none");
        assert_eq!(round_trip(&EmbeddedOutlineTables::EmptyGlyf), "empty_glyf");
        rejects::<EmbeddedOutlineTables>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedNotdef::Preserve), "preserve");
        rejects::<EmbeddedNotdef>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedLineExtremes::Computed), "computed");
        assert_eq!(round_trip(&EmbeddedLineExtremes::Zero), "zero");
        rejects::<EmbeddedLineExtremes>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedVerticalLineMetrics::Zero), "zero");
        assert_eq!(
            round_trip(&EmbeddedVerticalLineMetrics::MirrorHorizontal),
            "mirror_horizontal"
        );
        rejects::<EmbeddedVerticalLineMetrics>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedHmtx::Full), "full");
        assert_eq!(
            round_trip(&EmbeddedHmtx::CompactTrailing),
            "compact_trailing"
        );
        rejects::<EmbeddedHmtx>(r#""not_a_variant""#);
        assert_eq!(round_trip(&EmbeddedGlyphOrder::FirstSeen), "first_seen");
        assert_eq!(round_trip(&EmbeddedGlyphOrder::Codepoint), "codepoint");
        rejects::<EmbeddedGlyphOrder>(r#""not_a_variant""#);
        defaults::<EmbeddedMetadataOptions>();
        assert_eq!(
            round_trip(&EmbeddedUnicodeRanges::Exact([1, 2, 3, 4])),
            serde_json::json!({"exact": [1, 2, 3, 4]})
        );
        rejects::<EmbeddedUnicodeRanges>(r#"{"exact":[1,2,3]}"#);
        assert_eq!(
            round_trip(&EmbeddedIndexLayout::SharedMetricRuns { minimum_run: 3 }),
            serde_json::json!({"shared_metric_runs": {"minimum_run": 3}})
        );
        rejects::<EmbeddedIndexLayout>(r#"{"shared_metric_runs":{"minimum_run":3,"extra":1}}"#);
        assert_eq!(
            round_trip(&EmbeddedNotdef::Empty { advance_width: 8 }),
            serde_json::json!({"empty": {"advance_width": 8}})
        );
        rejects::<EmbeddedNotdef>(r#"{"empty":{}}"#);
        let named = EmbeddedMetadataOptions {
            names: Some(Arc::new(name::NameTable::default())),
            ..Default::default()
        };
        let json = round_trip(&named);
        let back: EmbeddedMetadataOptions = serde_json::from_value(json).unwrap();
        assert_eq!(back, named);
    }
}
