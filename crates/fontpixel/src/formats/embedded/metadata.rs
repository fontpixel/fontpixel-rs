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
pub enum EmbeddedAverageWidth {
    /// Nearest integer, halfway values rounded up (native default).
    #[default]
    Nearest,
    /// Integer floor of the mean.
    Floor,
}
/// Whether the post table carries glyph names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
/// Unicode-platform full-repertoire cmap record (0, 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmbeddedCmapUnicodeFull {
    /// List the format-12 subtable under (0, 4) and (3, 10) (native default).
    #[default]
    Always,
    /// Omit the redundant (0, 4) record when the cmap has no supplementary
    /// scalar; (3, 10) and every decoded mapping are unchanged.
    NonBmpOnly,
}
/// Source of the EBLC/bloc horizontal `widthMax` line metric.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
pub struct EmbeddedMetadataOptions {
    /// Complete name table. The `Arc` lets registry and transport clones share
    /// one allocation, which the writer charges once to its own budget.
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
            if flags & !0x030e != 0 {
                return Err(invalid("OS/2 fsType sets a reserved bit"));
            }
            if (flags & 0x000e).count_ones() > 1 {
                return Err(invalid(
                    "OS/2 fsType may set at most one usage permission bit",
                ));
            }
        }
        if let Some(flags) = self.selection_flags {
            if flags & 0xfc00 != 0 {
                return Err(invalid("OS/2 fsSelection bits 10..=15 are reserved"));
            }
            if flags & 0x0380 != 0 && self.os2_version.unwrap_or(4) < 4 {
                return Err(invalid("OS/2 fsSelection bits 7..=9 require version 4"));
            }
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
    pub(super) fn prepare(&self, fonts: &FontSet, limits: &sbit::Limits) -> Result<Prepared> {
        let Some(table) = &self.names else {
            return Ok(Prepared {
                limits: None,
                names: None,
            });
        };
        let maximum = limits.sfnt.resources.max_total_bytes;
        let error = || crate::Error::ResourceLimit {
            resource: "embedded metadata bytes",
            limit: maximum,
        };
        let configured = self.retained_bytes(maximum)?;
        let source = fonts
            .retained_usage(maximum.checked_sub(configured).ok_or_else(error)?)?
            .bytes;
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
            .ok_or_else(error)?;
        Ok(Prepared {
            limits: Some(remaining),
            names: Some(bytes),
        })
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
