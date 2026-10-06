//! Explicit OTB/bdat metadata flags. Every flag is opt-in; without them native
//! output is unchanged. Parsing stays available in feature-reduced builds so an
//! explicit request reports the disabled feature instead of an unknown option.
#[cfg(any(feature = "otb", feature = "bdat"))]
use crate::name_records::Records;
use crate::name_records::{number, record_hex, record_text};
#[cfg(any(feature = "otb", feature = "bdat"))]
use fontpixel::{Error, Result};

fn word(value: &str) -> std::result::Result<u16, String> {
    number(value)
}
fn positive_word(value: &str) -> std::result::Result<u16, String> {
    number(value)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "expected a number in 1..=65535".into())
}
fn vendor(value: &str) -> std::result::Result<String, String> {
    if value.len() == 4 && value.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        Ok(value.to_owned())
    } else {
        Err("expected exactly four printable ASCII characters".into())
    }
}
fn panose(value: &str) -> std::result::Result<[u8; 10], String> {
    let bytes = value
        .split(',')
        .map(number::<u8>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    bytes
        .try_into()
        .map_err(|_| "expected ten comma-separated PANOSE bytes".into())
}
fn unicode_ranges(value: &str) -> std::result::Result<String, String> {
    if matches!(value, "coverage" | "non-bmp") || ranges(value).is_ok() {
        Ok(value.to_owned())
    } else {
        Err("expected coverage, non-bmp, or four comma-separated 32-bit numbers".into())
    }
}
fn ranges(value: &str) -> std::result::Result<[u32; 4], String> {
    let values = value
        .split(',')
        .map(number::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    values
        .try_into()
        .map_err(|_| "expected four Unicode range words".into())
}
/// Exact name-table records and OS/2 fields for OTB/bdat output.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct MetadataArguments {
    /// Complete OTB/bdat name table record PLATFORM/ENCODING/LANGUAGE/NAME_ID=TEXT.
    ///
    /// Repeatable; numbers are decimal or 0x hexadecimal. Any record (or
    /// --otb-name-record-hex/--otb-name-language-tag) replaces the whole generated
    /// table: no IDs are added. TEXT is UTF-16BE for platform 0 and Windows 3/0,
    /// 3/1, 3/10, and Mac Roman for 1/0; other encodings need
    /// --otb-name-record-hex. Source names that the table drops are reported as
    /// loss. Example (export-rs-style Tong OTB):
    ///
    ///   fontpixel convert Face.bdf -o Face.otb
    ///     --otb-name-record 3/1/0x409/1='Tong Pixel Sans 14 Large JP'
    ///     --otb-name-record 3/1/0x409/2=Regular --otb-vendor TONG
    ///     --otb-panose 2,11,5,3,0,0,0,0,0,0 --otb-head-flags 3
    ///     --otb-lowest-rec-ppem 14 --otb-break-char 32 --otb-average-width floor
    ///     --otb-post-names omit --otb-unicode-ranges non-bmp
    ///     --otb-width-max advance --otb-index-layout shared-runs
    ///
    /// Batch options use underscores: otb_name_record = ["3/1/0x409/1=..."].
    #[cfg_attr(feature = "clap", arg(long, value_parser = record_text))]
    pub otb_name_record: Vec<String>,
    /// Name record with exact bytes: PLATFORM/ENCODING/LANGUAGE/NAME_ID=HEX.
    #[cfg_attr(feature = "clap", arg(long, value_parser = record_hex))]
    pub otb_name_record_hex: Vec<String>,
    /// BCP 47 language tag for name format 1; the Nth tag is language 0x8000+N.
    #[cfg_attr(feature = "clap", arg(long))]
    pub otb_name_language_tag: Vec<String>,
    /// Exact head.flags (bit 15 is reserved); native default 11.
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub otb_head_flags: Option<u16>,
    /// head.lowestRecPPEM, 1..=65535; native default 8.
    #[cfg_attr(feature = "clap", arg(long, value_parser = positive_word))]
    pub otb_lowest_rec_ppem: Option<u16>,
    /// head.created, seconds since 1904-01-01 UTC; requires --otb-modified.
    #[cfg_attr(
        feature = "clap",
        arg(long, requires = "otb_modified", allow_hyphen_values = true)
    )]
    pub otb_created: Option<i64>,
    /// head.modified, seconds since 1904-01-01 UTC; requires --otb-created.
    #[cfg_attr(
        feature = "clap",
        arg(long, requires = "otb_created", allow_hyphen_values = true)
    )]
    pub otb_modified: Option<i64>,
    /// OS/2 table version (same layout); native default 4.
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["3", "4"]))]
    pub otb_os2_version: Option<String>,
    /// OS/2 achVendID, exactly four printable ASCII characters; native NONE.
    #[cfg_attr(feature = "clap", arg(long, value_parser = vendor))]
    pub otb_vendor: Option<String>,
    /// Ten comma-separated PANOSE bytes; native default all zero.
    #[cfg_attr(feature = "clap", arg(long, value_parser = panose))]
    pub otb_panose: Option<[u8; 10]>,
    /// OS/2 Unicode ranges: coverage (native), non-bmp (only bit 57, when the
    /// cmap maps a supplementary scalar), or four comma-separated words.
    #[cfg_attr(feature = "clap", arg(long, value_parser = unicode_ranges))]
    pub otb_unicode_ranges: Option<String>,
    /// OS/2 usBreakChar; native default 0.
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub otb_break_char: Option<u16>,
    /// OS/2 fsType embedding permissions; native default 0.
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub otb_fs_type: Option<u16>,
    /// OS/2 fsSelection; native default 0x40 (regular).
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub otb_fs_selection: Option<u16>,
    /// OS/2 xAvgCharWidth arithmetic over nonzero advances (native nearest).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["nearest", "floor"]))]
    pub otb_average_width: Option<String>,
    /// post glyph names: keep (format 2, native) or omit (format 3, reported loss).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["keep", "omit"]))]
    pub otb_post_names: Option<String>,
    /// post.isFixedPitch override; absent follows the source spacing.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(bool)))]
    pub otb_fixed_pitch: Option<bool>,
    /// cmap format-12 records: always (native: (0,4) and (3,10)), non-bmp
    /// (omit (0,4) for BMP-only maps) or when-needed (format 12 only when
    /// format 4 cannot hold the map, under (3,10) only). Mappings never change.
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["always", "non-bmp", "when-needed"]))]
    pub otb_cmap_unicode_full: Option<String>,
    /// EBLC/bloc widthMax: bitmap (largest bitmap width, native) or advance.
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["bitmap", "advance"]))]
    pub otb_width_max: Option<String>,
    /// EBLC/bloc index layout: canonical (native), shared-runs, or spanning
    /// (one format-1 subtable across glyph IDs absent from a strike).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["canonical", "shared-runs", "spanning"]))]
    pub otb_index_layout: Option<String>,
    /// Shortest shared-metric run with --otb-index-layout shared-runs (default 8).
    #[cfg_attr(feature = "clap", arg(long, requires = "otb_index_layout", value_parser = positive_word))]
    pub otb_index_min_run: Option<u16>,
    /// TrueType outline tables: none (native OTB) or empty-glyf (one-byte glyf,
    /// short loca, maxp 1.0; outline-derived bounds and bearings become zero).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["none", "empty-glyf"]))]
    pub otb_outline_tables: Option<String>,
    /// maxp.maxZones (1 or 2) with --otb-outline-tables empty-glyf (default 1).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["1", "2"]))]
    pub otb_max_zones: Option<String>,
    /// Glyph 0: preserve (native) or empty (no bitmap in any strike, hmtx
    /// advance from --otb-notdef-advance; replacing a source .notdef is loss).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["preserve", "empty"]))]
    pub otb_notdef: Option<String>,
    /// Empty .notdef advance width in design units.
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub otb_notdef_advance: Option<u16>,
    /// EBLC/bloc caret slope RISE,RUN of every strike (native 1,0: upright).
    #[cfg_attr(feature = "clap", arg(long, value_parser = caret_slope, allow_hyphen_values = true))]
    pub otb_caret_slope: Option<[i8; 2]>,
    /// EBLC/bloc minOriginSB/minAdvanceSB/maxBeforeBL/minAfterBL: computed (native) or zero.
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["computed", "zero"]))]
    pub otb_line_extremes: Option<String>,
    /// Vertical line record of horizontal-only strikes: zero (native) or
    /// mirror-horizontal (a copy of the horizontal record).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["zero", "mirror-horizontal"]))]
    pub otb_vertical_line_metrics: Option<String>,
    /// post underline POSITION,THICKNESS in design units; replacing source
    /// underline metrics is reported as loss.
    #[cfg_attr(feature = "clap", arg(long, value_parser = underline, allow_hyphen_values = true))]
    pub otb_underline: Option<[i16; 2]>,
    /// hmtx: full (native, one long metric per glyph) or compact-trailing.
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["full", "compact-trailing"]))]
    pub otb_hmtx: Option<String>,
    /// Glyph IDs: first-seen (native) or codepoint (encoded glyphs by smallest
    /// scalar, then unencoded glyphs in first-seen order).
    #[cfg_attr(feature = "clap", arg(long, value_parser = ["first-seen", "codepoint"]))]
    pub otb_glyph_order: Option<String>,
}
fn pair<T: std::str::FromStr>(value: &str, what: &str) -> std::result::Result<[T; 2], String> {
    let parts = value
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<T>()
                .map_err(|_| format!("invalid {what} {part:?}"))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    parts
        .try_into()
        .map_err(|_| format!("expected two comma-separated {what} values"))
}
fn caret_slope(value: &str) -> std::result::Result<[i8; 2], String> {
    let slope = pair::<i8>(value, "caret slope")?;
    if slope == [0, 0] {
        return Err("caret slope rise and run cannot both be 0".into());
    }
    Ok(slope)
}
fn underline(value: &str) -> std::result::Result<[i16; 2], String> {
    pair::<i16>(value, "underline")
}

impl MetadataArguments {
    pub(crate) fn requested(&self) -> bool {
        !self.otb_name_record.is_empty()
            || !self.otb_name_record_hex.is_empty()
            || !self.otb_name_language_tag.is_empty()
            || self.otb_head_flags.is_some()
            || self.otb_lowest_rec_ppem.is_some()
            || self.otb_created.is_some()
            || self.otb_modified.is_some()
            || self.otb_os2_version.is_some()
            || self.otb_vendor.is_some()
            || self.otb_panose.is_some()
            || self.otb_unicode_ranges.is_some()
            || self.otb_break_char.is_some()
            || self.otb_fs_type.is_some()
            || self.otb_fs_selection.is_some()
            || self.otb_average_width.is_some()
            || self.otb_post_names.is_some()
            || self.otb_fixed_pitch.is_some()
            || self.otb_cmap_unicode_full.is_some()
            || self.otb_width_max.is_some()
            || self.otb_index_layout.is_some()
            || self.otb_index_min_run.is_some()
            || self.otb_outline_tables.is_some()
            || self.otb_max_zones.is_some()
            || self.otb_notdef.is_some()
            || self.otb_notdef_advance.is_some()
            || self.otb_caret_slope.is_some()
            || self.otb_line_extremes.is_some()
            || self.otb_vertical_line_metrics.is_some()
            || self.otb_underline.is_some()
            || self.otb_hmtx.is_some()
            || self.otb_glyph_order.is_some()
    }
    /// Bytes the caller retains for these arguments and one built name table,
    /// including spare capacity and the shared table's `Arc` header.
    #[cfg(any(feature = "otb", feature = "bdat"))]
    pub(crate) fn reserved_bytes(&self, limit: usize) -> Result<usize> {
        let error = || Error::ResourceLimit {
            resource: "OTB/bdat metadata arguments",
            limit,
        };
        let mut total = 0usize;
        let mut add = |n: Option<usize>| -> Result<()> {
            total = n
                .and_then(|n| total.checked_add(n))
                .filter(|n| *n <= limit)
                .ok_or_else(error)?;
            Ok(())
        };
        add(Some(self.records().argument_bytes(limit)?))?;
        for value in [
            &self.otb_os2_version,
            &self.otb_vendor,
            &self.otb_unicode_ranges,
            &self.otb_average_width,
            &self.otb_post_names,
            &self.otb_cmap_unicode_full,
            &self.otb_width_max,
            &self.otb_index_layout,
            &self.otb_outline_tables,
            &self.otb_max_zones,
            &self.otb_notdef,
            &self.otb_line_extremes,
            &self.otb_vertical_line_metrics,
            &self.otb_hmtx,
            &self.otb_glyph_order,
        ] {
            add(Some(value.as_ref().map_or(0, String::capacity)))?;
        }
        if let Some(table) = self.names(limit)? {
            add(Some(table.retained_bytes(limit)?))?;
            add(Some(2 * size_of::<usize>()))?;
        }
        Ok(total)
    }
    #[cfg(any(feature = "otb", feature = "bdat"))]
    fn records(&self) -> Records<'_> {
        Records {
            text: &self.otb_name_record,
            hex: &self.otb_name_record_hex,
            tags: &self.otb_name_language_tag,
            resource: "OTB/bdat metadata arguments",
            hex_flag: "--otb-name-record-hex",
        }
    }
    #[cfg(any(feature = "otb", feature = "bdat"))]
    fn names(&self, limit: usize) -> Result<Option<fontpixel::sfnt::name::NameTable>> {
        self.records().table(limit)
    }
    /// Typed library options. Validation errors surface before any input is read.
    #[cfg(any(feature = "otb", feature = "bdat"))]
    pub(crate) fn options(
        &self,
        limits: &fontpixel::ResourceLimits,
    ) -> Result<fontpixel::formats::embedded::EmbeddedMetadataOptions> {
        use fontpixel::formats::embedded::{
            EmbeddedAverageWidth, EmbeddedCmapUnicodeFull, EmbeddedGlyphOrder, EmbeddedHmtx,
            EmbeddedIndexLayout, EmbeddedLineExtremes, EmbeddedMetadataOptions, EmbeddedNotdef,
            EmbeddedOutlineTables, EmbeddedPostNames, EmbeddedUnicodeRanges,
            EmbeddedVerticalLineMetrics, EmbeddedWidthMax,
        };
        let names = self.names(limits.max_total_bytes)?;
        if let Some(table) = &names {
            table.validate_configured(&fontpixel::sfnt::Limits {
                resources: limits.clone(),
                ..Default::default()
            })?;
        }
        let options = EmbeddedMetadataOptions {
            names: names.map(std::sync::Arc::new),
            head_flags: self.otb_head_flags,
            lowest_rec_ppem: self.otb_lowest_rec_ppem,
            timestamps: self.otb_created.zip(self.otb_modified).map(|(c, m)| [c, m]),
            os2_version: self
                .otb_os2_version
                .as_deref()
                .map(|v| if v == "3" { 3 } else { 4 }),
            vendor_id: self
                .otb_vendor
                .as_deref()
                .and_then(|v| v.as_bytes().try_into().ok()),
            panose: self.otb_panose,
            unicode_ranges: match self.otb_unicode_ranges.as_deref() {
                None | Some("coverage") => EmbeddedUnicodeRanges::Coverage,
                Some("non-bmp") => EmbeddedUnicodeRanges::NonBmpOnly,
                Some(value) => {
                    EmbeddedUnicodeRanges::Exact(ranges(value).map_err(Error::InvalidInput)?)
                }
            },
            break_char: self.otb_break_char,
            embedding_flags: self.otb_fs_type,
            selection_flags: self.otb_fs_selection,
            average_width: if self.otb_average_width.as_deref() == Some("floor") {
                EmbeddedAverageWidth::Floor
            } else {
                EmbeddedAverageWidth::Nearest
            },
            post_names: if self.otb_post_names.as_deref() == Some("omit") {
                EmbeddedPostNames::Omit
            } else {
                EmbeddedPostNames::Keep
            },
            fixed_pitch: self.otb_fixed_pitch,
            cmap_unicode_full: match self.otb_cmap_unicode_full.as_deref() {
                Some("non-bmp") => EmbeddedCmapUnicodeFull::NonBmpOnly,
                Some("when-needed") => EmbeddedCmapUnicodeFull::WhenNeeded,
                _ => EmbeddedCmapUnicodeFull::Always,
            },
            width_max: if self.otb_width_max.as_deref() == Some("advance") {
                EmbeddedWidthMax::Advance
            } else {
                EmbeddedWidthMax::BitmapWidth
            },
            index_layout: match self.otb_index_layout.as_deref() {
                Some("shared-runs") => EmbeddedIndexLayout::SharedMetricRuns {
                    minimum_run: self.otb_index_min_run.unwrap_or(8),
                },
                Some("spanning") => EmbeddedIndexLayout::Spanning,
                _ => EmbeddedIndexLayout::Canonical,
            },
            outline_tables: if self.otb_outline_tables.as_deref() == Some("empty-glyf") {
                EmbeddedOutlineTables::EmptyGlyf
            } else {
                EmbeddedOutlineTables::None
            },
            max_zones: self
                .otb_max_zones
                .as_deref()
                .map(|z| if z == "2" { 2 } else { 1 }),
            notdef: match (self.otb_notdef.as_deref(), self.otb_notdef_advance) {
                (Some("empty"), Some(advance_width)) => EmbeddedNotdef::Empty { advance_width },
                (Some("empty"), None) => {
                    return Err(Error::InvalidInput(
                        "--otb-notdef empty requires --otb-notdef-advance".into(),
                    ));
                }
                (_, None) => EmbeddedNotdef::Preserve,
                (_, Some(_)) => {
                    return Err(Error::InvalidInput(
                        "--otb-notdef-advance requires --otb-notdef empty".into(),
                    ));
                }
            },
            caret_slope: self.otb_caret_slope,
            line_extremes: if self.otb_line_extremes.as_deref() == Some("zero") {
                EmbeddedLineExtremes::Zero
            } else {
                EmbeddedLineExtremes::Computed
            },
            vertical_line_metrics: if self.otb_vertical_line_metrics.as_deref()
                == Some("mirror-horizontal")
            {
                EmbeddedVerticalLineMetrics::MirrorHorizontal
            } else {
                EmbeddedVerticalLineMetrics::Zero
            },
            underline: self.otb_underline,
            hmtx: if self.otb_hmtx.as_deref() == Some("compact-trailing") {
                EmbeddedHmtx::CompactTrailing
            } else {
                EmbeddedHmtx::Full
            },
            glyph_order: if self.otb_glyph_order.as_deref() == Some("codepoint") {
                EmbeddedGlyphOrder::Codepoint
            } else {
                EmbeddedGlyphOrder::FirstSeen
            },
        };
        if self.otb_index_min_run.is_some()
            && !matches!(
                options.index_layout,
                EmbeddedIndexLayout::SharedMetricRuns { .. }
            )
        {
            return Err(Error::InvalidInput(
                "--otb-index-min-run requires --otb-index-layout shared-runs".into(),
            ));
        }
        // OTB and bdat validate identically; color never reaches this path.
        options.validate(fontpixel::sfnt::bitmap::Kind::OpenType)?;
        Ok(options)
    }
}
