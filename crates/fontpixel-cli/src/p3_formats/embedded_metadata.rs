//! Explicit OTB/bdat metadata flags. Every flag is opt-in; without them native
//! output is unchanged. Parsing stays available in feature-reduced builds so an
//! explicit request reports the disabled feature instead of an unknown option.
use clap::Args;
use fontpixel::{Error, Result};

fn number<T: TryFrom<u64>>(value: &str) -> std::result::Result<T, String> {
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        value.parse()
    };
    parsed
        .ok()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| format!("{value:?} is not a decimal or 0x-hexadecimal number in range"))
}
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
/// `P/E/L/ID` record key: platform, encoding, language and name IDs.
fn key(value: &str) -> Result<[u16; 4]> {
    let fields = value
        .split('/')
        .map(word)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::InvalidInput)?;
    fields.try_into().map_err(|_| {
        Error::InvalidInput(format!(
            "name record key {value:?} must be PLATFORM/ENCODING/LANGUAGE/NAME_ID"
        ))
    })
}
fn record_text(value: &str) -> std::result::Result<String, String> {
    let (left, _) = value
        .split_once('=')
        .ok_or("expected PLATFORM/ENCODING/LANGUAGE/NAME_ID=TEXT")?;
    key(left).map_err(|e| e.to_string())?;
    Ok(value.to_owned())
}
fn record_hex(value: &str) -> std::result::Result<String, String> {
    let (left, right) = value
        .split_once('=')
        .ok_or("expected PLATFORM/ENCODING/LANGUAGE/NAME_ID=HEX")?;
    key(left).map_err(|e| e.to_string())?;
    hex(right)?;
    Ok(value.to_owned())
}
fn hex(value: &str) -> std::result::Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected an even number of hexadecimal digits".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[derive(Args, Default)]
#[group(skip)]
pub(super) struct MetadataArguments {
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
    #[arg(long, value_parser = record_text)]
    otb_name_record: Vec<String>,
    /// Name record with exact bytes: PLATFORM/ENCODING/LANGUAGE/NAME_ID=HEX.
    #[arg(long, value_parser = record_hex)]
    otb_name_record_hex: Vec<String>,
    /// BCP 47 language tag for name format 1; the Nth tag is language 0x8000+N.
    #[arg(long)]
    otb_name_language_tag: Vec<String>,
    /// Exact head.flags (bit 15 is reserved); native default 11.
    #[arg(long, value_parser = word)]
    otb_head_flags: Option<u16>,
    /// head.lowestRecPPEM, 1..=65535; native default 8.
    #[arg(long, value_parser = positive_word)]
    otb_lowest_rec_ppem: Option<u16>,
    /// head.created, seconds since 1904-01-01 UTC; requires --otb-modified.
    #[arg(long, requires = "otb_modified", allow_hyphen_values = true)]
    otb_created: Option<i64>,
    /// head.modified, seconds since 1904-01-01 UTC; requires --otb-created.
    #[arg(long, requires = "otb_created", allow_hyphen_values = true)]
    otb_modified: Option<i64>,
    /// OS/2 table version (same layout); native default 4.
    #[arg(long, value_parser = ["3", "4"])]
    otb_os2_version: Option<String>,
    /// OS/2 achVendID, exactly four printable ASCII characters; native NONE.
    #[arg(long, value_parser = vendor)]
    otb_vendor: Option<String>,
    /// Ten comma-separated PANOSE bytes; native default all zero.
    #[arg(long, value_parser = panose)]
    otb_panose: Option<[u8; 10]>,
    /// OS/2 Unicode ranges: coverage (native), non-bmp (only bit 57, when the
    /// cmap maps a supplementary scalar), or four comma-separated words.
    #[arg(long, value_parser = unicode_ranges)]
    otb_unicode_ranges: Option<String>,
    /// OS/2 usBreakChar; native default 0.
    #[arg(long, value_parser = word)]
    otb_break_char: Option<u16>,
    /// OS/2 fsType embedding permissions; native default 0.
    #[arg(long, value_parser = word)]
    otb_fs_type: Option<u16>,
    /// OS/2 fsSelection; native default 0x40 (regular).
    #[arg(long, value_parser = word)]
    otb_fs_selection: Option<u16>,
    /// OS/2 xAvgCharWidth arithmetic over nonzero advances (native nearest).
    #[arg(long, value_parser = ["nearest", "floor"])]
    otb_average_width: Option<String>,
    /// post glyph names: keep (format 2, native) or omit (format 3, reported loss).
    #[arg(long, value_parser = ["keep", "omit"])]
    otb_post_names: Option<String>,
    /// post.isFixedPitch override; absent follows the source spacing.
    #[arg(long, value_parser = clap::value_parser!(bool))]
    otb_fixed_pitch: Option<bool>,
    /// cmap (0,4) record: always (native) or non-bmp (omit it for BMP-only maps;
    /// (3,10) and all mappings are unchanged).
    #[arg(long, value_parser = ["always", "non-bmp"])]
    otb_cmap_unicode_full: Option<String>,
    /// EBLC/bloc widthMax: bitmap (largest bitmap width, native) or advance.
    #[arg(long, value_parser = ["bitmap", "advance"])]
    otb_width_max: Option<String>,
    /// EBLC/bloc index layout: canonical (native) or shared-runs.
    #[arg(long, value_parser = ["canonical", "shared-runs"])]
    otb_index_layout: Option<String>,
    /// Shortest shared-metric run with --otb-index-layout shared-runs (default 8).
    #[arg(long, requires = "otb_index_layout", value_parser = positive_word)]
    otb_index_min_run: Option<u16>,
}

impl MetadataArguments {
    pub(super) fn requested(&self) -> bool {
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
    }
    /// Bytes the caller retains for these arguments and one built name table,
    /// including spare capacity and the shared table's `Arc` header.
    #[cfg(any(feature = "otb", feature = "bdat"))]
    pub(super) fn reserved_bytes(&self, limit: usize) -> Result<usize> {
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
        for list in [
            &self.otb_name_record,
            &self.otb_name_record_hex,
            &self.otb_name_language_tag,
        ] {
            add(list.capacity().checked_mul(size_of::<String>()))?;
            for value in list {
                add(Some(value.capacity()))?;
            }
        }
        for value in [
            &self.otb_os2_version,
            &self.otb_vendor,
            &self.otb_unicode_ranges,
            &self.otb_average_width,
            &self.otb_post_names,
            &self.otb_cmap_unicode_full,
            &self.otb_width_max,
            &self.otb_index_layout,
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
    fn names(&self, limit: usize) -> Result<Option<fontpixel::sfnt::name::NameTable>> {
        use fontpixel::sfnt::name::{NameRecord, NameTable};
        if self.otb_name_record.is_empty()
            && self.otb_name_record_hex.is_empty()
            && self.otb_name_language_tag.is_empty()
        {
            return Ok(None);
        }
        let error = || Error::ResourceLimit {
            resource: "OTB/bdat metadata arguments",
            limit,
        };
        let count = self.otb_name_record.len() + self.otb_name_record_hex.len();
        if count.saturating_mul(size_of::<NameRecord>()) > limit {
            return Err(error());
        }
        let mut table = NameTable::default();
        table
            .records
            .try_reserve_exact(count)
            .map_err(|_| error())?;
        let mac_roman = fontpixel::charset::CharsetCodec::from_key("macroman", None)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        for value in &self.otb_name_record {
            let (left, text) = value
                .split_once('=')
                .ok_or_else(|| Error::InvalidInput("name record requires =".into()))?;
            let [platform_id, encoding_id, language_id, name_id] = key(left)?;
            let bytes: Vec<u8> =
                if platform_id == 0 || (platform_id == 3 && [0, 1, 10].contains(&encoding_id)) {
                    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
                } else if (platform_id, encoding_id) == (1, 0) {
                    text.chars()
                        .map(|c| {
                            mac_roman
                            .char2cp(c)
                            .and_then(|code| u8::try_from(code).ok())
                            .ok_or_else(|| {
                                Error::InvalidInput(format!(
                                    "name record {left}: {c:?} is not representable in Mac Roman"
                                ))
                            })
                        })
                        .collect::<Result<_>>()?
                } else {
                    return Err(Error::InvalidInput(format!(
                        "name record {left}: use --otb-name-record-hex for this platform encoding"
                    )));
                };
            table.records.push(NameRecord {
                platform_id,
                encoding_id,
                language_id,
                name_id,
                value: bytes,
            });
        }
        for value in &self.otb_name_record_hex {
            let (left, right) = value
                .split_once('=')
                .ok_or_else(|| Error::InvalidInput("name record requires =".into()))?;
            let [platform_id, encoding_id, language_id, name_id] = key(left)?;
            table.records.push(NameRecord {
                platform_id,
                encoding_id,
                language_id,
                name_id,
                value: hex(right).map_err(Error::InvalidInput)?,
            });
        }
        table
            .language_tags
            .try_reserve_exact(self.otb_name_language_tag.len())
            .map_err(|_| error())?;
        table
            .language_tags
            .extend(self.otb_name_language_tag.iter().cloned());
        Ok(Some(table))
    }
    /// Typed library options. Validation errors surface before any input is read.
    #[cfg(any(feature = "otb", feature = "bdat"))]
    pub(super) fn options(
        &self,
        limits: &fontpixel::ResourceLimits,
    ) -> Result<fontpixel::formats::embedded::EmbeddedMetadataOptions> {
        use fontpixel::formats::embedded::{
            EmbeddedAverageWidth, EmbeddedCmapUnicodeFull, EmbeddedIndexLayout,
            EmbeddedMetadataOptions, EmbeddedPostNames, EmbeddedUnicodeRanges, EmbeddedWidthMax,
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
            cmap_unicode_full: if self.otb_cmap_unicode_full.as_deref() == Some("non-bmp") {
                EmbeddedCmapUnicodeFull::NonBmpOnly
            } else {
                EmbeddedCmapUnicodeFull::Always
            },
            width_max: if self.otb_width_max.as_deref() == Some("advance") {
                EmbeddedWidthMax::Advance
            } else {
                EmbeddedWidthMax::BitmapWidth
            },
            index_layout: if self.otb_index_layout.as_deref() == Some("shared-runs") {
                EmbeddedIndexLayout::SharedMetricRuns {
                    minimum_run: self.otb_index_min_run.unwrap_or(8),
                }
            } else {
                EmbeddedIndexLayout::Canonical
            },
        };
        if self.otb_index_min_run.is_some()
            && options.index_layout == EmbeddedIndexLayout::Canonical
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
