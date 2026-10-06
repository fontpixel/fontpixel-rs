//! Explicit outline-export (`variants`) metadata flags. Every flag is opt-in;
//! without them the output is unchanged. Name records use exactly the
//! `--otb-name-record` syntax and parser (`name_records`).
#[cfg(feature = "clap")]
use crate::name_records::number;
use crate::name_records::{Records, record_hex, record_text};
use crate::options::{check_choice, check_parsed};
use fontpixel::{
    Error, Result,
    sfnt::cmap::FullRecords,
    variants::{DotGlyph, GaspPolicy, GlyphOrder, MetadataOptions},
};

#[cfg(feature = "clap")]
fn word(value: &str) -> std::result::Result<u16, String> {
    number(value)
}
#[cfg(feature = "clap")]
fn code_pages(value: &str) -> std::result::Result<[u32; 2], String> {
    let values = value
        .split(',')
        .map(number::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    values
        .try_into()
        .map_err(|_| "expected two comma-separated code page range words".into())
}

const OUTLINE_METADATA_OPTIONS_MAX_ZONES: [&str; 2] = ["1", "2"];
const OUTLINE_METADATA_OPTIONS_GASP: [&str; 2] = ["keep", "omit"];
const OUTLINE_METADATA_OPTIONS_CMAP_UNICODE_FULL: [&str; 3] = ["always", "non-bmp", "when-needed"];
const OUTLINE_METADATA_OPTIONS_GLYPH_ORDER: [&str; 2] = ["planned", "codepoint"];
const OUTLINE_METADATA_OPTIONS_DOT_GLYPH: [&str; 2] = ["last", "after-notdef"];
/// Exact name-table records and OS/2 fields for outline exports.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct OutlineMetadataOptions {
    /// Exact name record PLATFORM/ENCODING/LANGUAGE/NAME_ID=TEXT (the
    /// --otb-name-record syntax). Repeatable; any record (or --name-record-hex /
    /// --name-language-tag) replaces the whole generated table and the
    /// --family/--style/--full-name/--postscript-name/--copyright name texts.
    /// Source notices the table drops are reported as loss. Example:
    ///
    ///   fontpixel variants --base Face.bdf -o Face.ttf
    ///     --name-record 3/1/0x409/1='Face' --name-record 1/0/0/1='Face'
    ///     --fs-type 4 --fs-selection 0 --break-char 32 --created 3660681600
    ///     --modified 3660681600 --gasp omit --cmap-unicode-full when-needed
    ///
    /// Batch options use underscores: name_record = ["3/1/0x409/1=..."].
    #[cfg_attr(feature = "clap", arg(long, value_parser = record_text))]
    pub name_record: Vec<String>,
    /// Name record with exact bytes: PLATFORM/ENCODING/LANGUAGE/NAME_ID=HEX.
    #[cfg_attr(feature = "clap", arg(long, value_parser = record_hex))]
    pub name_record_hex: Vec<String>,
    /// BCP 47 language tag for name format 1; the Nth tag is language 0x8000+N.
    #[cfg_attr(feature = "clap", arg(long))]
    pub name_language_tag: Vec<String>,
    /// OS/2 fsType embedding permissions; native default 0.
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub fs_type: Option<u16>,
    /// OS/2 fsSelection (version 3: bits 7..=9 rejected); native default 0x40.
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub fs_selection: Option<u16>,
    /// OS/2 usBreakChar; native default 0.
    #[cfg_attr(feature = "clap", arg(long, value_parser = word))]
    pub break_char: Option<u16>,
    /// Exact OS/2 ulCodePageRange1,ulCodePageRange2 instead of the cmap-derived bits.
    #[cfg_attr(feature = "clap", arg(long, value_parser = code_pages))]
    pub code_page_ranges: Option<[u32; 2]>,
    /// head.created, seconds since 1904-01-01 UTC; requires --modified.
    #[cfg_attr(
        feature = "clap",
        arg(long, requires = "modified", allow_hyphen_values = true)
    )]
    pub created: Option<i64>,
    /// head.modified, seconds since 1904-01-01 UTC; requires --created.
    #[cfg_attr(
        feature = "clap",
        arg(long, requires = "created", allow_hyphen_values = true)
    )]
    pub modified: Option<i64>,
    /// post.isFixedPitch override; absent follows the source spacing.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(bool)))]
    pub fixed_pitch: Option<bool>,
    /// maxp.maxZones of TrueType output, 1 (native) or 2.
    #[cfg_attr(feature = "clap", arg(long, value_parser = OUTLINE_METADATA_OPTIONS_MAX_ZONES))]
    pub max_zones: Option<String>,
    /// gasp table: keep (native) or omit.
    #[cfg_attr(feature = "clap", arg(long, value_parser = OUTLINE_METADATA_OPTIONS_GASP))]
    pub gasp: Option<String>,
    /// cmap format-12 records: always (native), non-bmp, or when-needed
    /// (format 12 only when format 4 cannot hold the map, under (3,10) only).
    #[cfg_attr(feature = "clap", arg(long, value_parser = OUTLINE_METADATA_OPTIONS_CMAP_UNICODE_FULL))]
    pub cmap_unicode_full: Option<String>,
    /// Glyph IDs: planned (native, by glyph name) or codepoint (encoded glyphs
    /// by smallest scalar, then alternates in planned order).
    #[cfg_attr(feature = "clap", arg(long, value_parser = OUTLINE_METADATA_OPTIONS_GLYPH_ORDER))]
    pub glyph_order: Option<String>,
    /// Shared dot glyph of TrueType dot output: last (native) or after-notdef (glyph 1).
    #[cfg_attr(feature = "clap", arg(long, value_parser = OUTLINE_METADATA_OPTIONS_DOT_GLYPH))]
    pub dot_glyph: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(OutlineMetadataOptions);
impl OutlineMetadataOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        for value in &self.name_record {
            check_parsed("name_record", value, record_text)?;
        }
        for value in &self.name_record_hex {
            check_parsed("name_record_hex", value, record_hex)?;
        }
        if let Some(value) = self.max_zones.as_deref() {
            check_choice("max_zones", value, &OUTLINE_METADATA_OPTIONS_MAX_ZONES)?;
        }
        if let Some(value) = self.gasp.as_deref() {
            check_choice("gasp", value, &OUTLINE_METADATA_OPTIONS_GASP)?;
        }
        if let Some(value) = self.cmap_unicode_full.as_deref() {
            check_choice(
                "cmap_unicode_full",
                value,
                &OUTLINE_METADATA_OPTIONS_CMAP_UNICODE_FULL,
            )?;
        }
        if let Some(value) = self.glyph_order.as_deref() {
            check_choice("glyph_order", value, &OUTLINE_METADATA_OPTIONS_GLYPH_ORDER)?;
        }
        if let Some(value) = self.dot_glyph.as_deref() {
            check_choice("dot_glyph", value, &OUTLINE_METADATA_OPTIONS_DOT_GLYPH)?;
        }
        crate::options::check_requires(
            self.created.is_some(),
            &[(self.modified.is_some(), "--modified <MODIFIED>")],
        )?;
        crate::options::check_requires(
            self.modified.is_some(),
            &[(self.created.is_some(), "--created <CREATED>")],
        )?;
        Ok(())
    }
}

impl OutlineMetadataOptions {
    fn records(&self) -> Records<'_> {
        Records {
            text: &self.name_record,
            hex: &self.name_record_hex,
            tags: &self.name_language_tag,
            resource: "outline metadata arguments",
            hex_flag: "--name-record-hex",
        }
    }
    /// Bytes the caller retains for these arguments and the name table built
    /// from them (which `variants::Options` owns while exporting).
    pub(crate) fn reserved_bytes(&self, limit: usize) -> Result<usize> {
        let error = || Error::ResourceLimit {
            resource: "outline metadata arguments",
            limit,
        };
        let mut total = self.records().argument_bytes(limit)?;
        for value in [
            &self.max_zones,
            &self.gasp,
            &self.cmap_unicode_full,
            &self.glyph_order,
            &self.dot_glyph,
        ] {
            total = total
                .checked_add(value.as_ref().map_or(0, String::capacity))
                .filter(|n| *n <= limit)
                .ok_or_else(error)?;
        }
        if let Some(table) = self.records().table(limit)? {
            total = total
                .checked_add(table.retained_bytes(limit)?)
                .filter(|n| *n <= limit)
                .ok_or_else(error)?;
        }
        Ok(total)
    }
    /// The exact name table, validated against the output limits.
    pub(crate) fn names(
        &self,
        limits: &fontpixel::ResourceLimits,
    ) -> Result<Option<fontpixel::sfnt::name::NameTable>> {
        let table = self.records().table(limits.max_total_bytes)?;
        if let Some(table) = &table {
            table.validate_configured(&fontpixel::sfnt::Limits {
                resources: limits.clone(),
                ..Default::default()
            })?;
        }
        Ok(table)
    }
    /// Typed library controls; validation runs again in the library.
    pub(crate) fn options(&self) -> MetadataOptions {
        MetadataOptions {
            timestamps: self.created.zip(self.modified).map(|(c, m)| [c, m]),
            embedding_flags: self.fs_type,
            selection_flags: self.fs_selection,
            break_char: self.break_char,
            code_page_ranges: self.code_page_ranges,
            fixed_pitch: self.fixed_pitch,
            max_zones: self
                .max_zones
                .as_deref()
                .map(|z| if z == "2" { 2 } else { 1 }),
            gasp: if self.gasp.as_deref() == Some("omit") {
                GaspPolicy::Omit
            } else {
                GaspPolicy::Write
            },
            cmap_full_records: match self.cmap_unicode_full.as_deref() {
                Some("non-bmp") => FullRecords::NonBmpOnly,
                Some("when-needed") => FullRecords::WhenNeeded,
                _ => FullRecords::Always,
            },
            glyph_order: if self.glyph_order.as_deref() == Some("codepoint") {
                GlyphOrder::Codepoint
            } else {
                GlyphOrder::Planned
            },
            dot_glyph: if self.dot_glyph.as_deref() == Some("after-notdef") {
                DotGlyph::AfterNotdef
            } else {
                DotGlyph::Last
            },
        }
    }
}

#[cfg(all(test, feature = "clap"))]
mod tests {
    use super::*;
    use clap::Parser;

    #[cfg_attr(feature = "clap", derive(clap::Parser))]
    struct Parse {
        #[cfg_attr(feature = "clap", command(flatten))]
        metadata: OutlineMetadataOptions,
    }

    #[test]
    fn defaults_are_native_and_flags_map_exactly() {
        let none = Parse::try_parse_from(["t"]).unwrap().metadata;
        assert_eq!(none.options(), MetadataOptions::default());
        assert_eq!(none.reserved_bytes(usize::MAX).unwrap(), 0);
        assert!(none.names(&Default::default()).unwrap().is_none());
        let all = Parse::try_parse_from([
            "t",
            "--fs-type",
            "4",
            "--fs-selection",
            "0",
            "--break-char",
            "0x20",
            "--code-page-ranges",
            "0,0",
            "--created",
            "3660681600",
            "--modified",
            "3660681600",
            "--fixed-pitch",
            "false",
            "--max-zones",
            "2",
            "--gasp",
            "omit",
            "--cmap-unicode-full",
            "when-needed",
            "--glyph-order",
            "codepoint",
            "--dot-glyph",
            "after-notdef",
        ])
        .unwrap()
        .metadata;
        assert_eq!(
            all.options(),
            MetadataOptions {
                timestamps: Some([3660681600; 2]),
                embedding_flags: Some(4),
                selection_flags: Some(0),
                break_char: Some(32),
                code_page_ranges: Some([0, 0]),
                fixed_pitch: Some(false),
                max_zones: Some(2),
                gasp: GaspPolicy::Omit,
                cmap_full_records: FullRecords::WhenNeeded,
                glyph_order: GlyphOrder::Codepoint,
                dot_glyph: DotGlyph::AfterNotdef,
            }
        );
        assert!(Parse::try_parse_from(["t", "--created", "1"]).is_err());
        assert!(Parse::try_parse_from(["t", "--code-page-ranges", "1"]).is_err());
        assert!(Parse::try_parse_from(["t", "--max-zones", "3"]).is_err());
        assert!(Parse::try_parse_from(["t", "--name-record", "3/1/1=X"]).is_err());
    }

    #[test]
    fn name_arguments_and_built_table_are_reserved_exactly() {
        let parsed = Parse::try_parse_from([
            "t",
            "--name-record",
            "3/1/0x409/1=Face",
            "--name-record",
            "1/0/0/1=Face",
        ])
        .unwrap()
        .metadata;
        let table = parsed.names(&Default::default()).unwrap().unwrap();
        assert_eq!(table.records.len(), 2);
        let exact = parsed.records().argument_bytes(usize::MAX).unwrap()
            + table.retained_bytes(usize::MAX).unwrap();
        assert_eq!(parsed.reserved_bytes(usize::MAX).unwrap(), exact);
        assert_eq!(parsed.reserved_bytes(exact).unwrap(), exact);
        assert!(matches!(
            parsed.reserved_bytes(exact - 1),
            Err(Error::ResourceLimit { .. })
        ));
    }
}
