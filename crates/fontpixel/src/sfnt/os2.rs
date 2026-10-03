//! Complete OS/2 versions 0–5 and conservative, explicit coverage calculation.
use super::{Limits, buffer, data::UNICODE_RANGES, invalid, p16, p32, pi16};
use crate::{Result, charset::CharsetCodec};
use read_fonts::{FontData, FontRead, tables::os2::Os2};
use std::collections::BTreeSet;
/// Fields present in every supported OS/2 version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    /// Average nonzero horizontal advance, in design units.
    pub average_width: i16,
    /// Weight class, 1..=1000.
    pub weight_class: u16,
    /// Width class, 1..=9.
    pub width_class: u16,
    /// Embedding permissions, retained exactly. No permission is inferred.
    pub embedding_flags: u16,
    /// Subscript x size, y size, x offset, y offset.
    pub subscript: [i16; 4],
    /// Superscript x size, y size, x offset, y offset.
    pub superscript: [i16; 4],
    /// Strikeout thickness and position.
    pub strikeout: [i16; 2],
    /// IBM font-family classification.
    pub family_class: i16,
    /// Ten PANOSE bytes.
    pub panose: [u8; 10],
    /// Four 32-bit Unicode coverage bit fields.
    pub unicode_ranges: [u32; 4],
    /// Four-byte vendor ID (not constrained to a table-tag syntax).
    pub vendor_id: [u8; 4],
    /// Style selection flags.
    pub selection_flags: u16,
    /// Lowest encoded scalar, capped at 65535; zero for an empty map.
    pub first_char: u16,
    /// Highest encoded scalar, capped at 65535; zero for an empty map.
    pub last_char: u16,
    /// Typographic ascent, descent, line gap.
    pub typo_metrics: [i16; 3],
    /// Windows ascent/descent (both unsigned magnitudes).
    pub windows_metrics: [u16; 2],
}
impl Default for Base {
    fn default() -> Self {
        Self {
            average_width: 0,
            weight_class: 400,
            width_class: 5,
            embedding_flags: 0,
            subscript: [0; 4],
            superscript: [0; 4],
            strikeout: [0; 2],
            family_class: 0,
            panose: [0; 10],
            unicode_ranges: [0; 4],
            vendor_id: *b"NONE",
            selection_flags: 0x40,
            first_char: 0,
            last_char: 0,
            typo_metrics: [0; 3],
            windows_metrics: [0; 2],
        }
    }
}
/// OS/2 version-2-and-later fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extension {
    /// x-height, in design units.
    pub x_height: i16,
    /// Capital height, in design units.
    pub cap_height: i16,
    /// Default character code.
    pub default_char: u16,
    /// Break character code.
    pub break_char: u16,
    /// Maximum context length for shaping lookups.
    pub max_context: u16,
}
/// Editable, version-aware OS/2 table. Unsupported extension loss is an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Os2Table {
    /// Version 0 through 5.
    pub version: u16,
    /// Original common fields.
    pub base: Base,
    /// Required for version 1+, forbidden for version 0.
    pub code_page_ranges: Option<[u32; 2]>,
    /// Required for version 2+, forbidden in older versions.
    pub extension: Option<Extension>,
    /// Version 5 optical point-size bounds, in twentieths of a point.
    pub optical_sizes: Option<[u16; 2]>,
}
impl Default for Os2Table {
    fn default() -> Self {
        Self {
            version: 4,
            base: Base::default(),
            code_page_ranges: Some([0; 2]),
            extension: Some(Extension::default()),
            optical_sizes: None,
        }
    }
}
impl Os2Table {
    /// Set coverage bits and first/last character indexes from actual mappings.
    /// Code-page bits are separate and never guessed from a block intersection.
    pub fn set_unicode_coverage(&mut self, codepoints: &BTreeSet<u32>) -> Result<()> {
        self.base.unicode_ranges = unicode_ranges(codepoints)?;
        self.base.first_char = codepoints.first().copied().unwrap_or(0).min(65535) as u16;
        self.base.last_char = codepoints.last().copied().unwrap_or(0).min(65535) as u16;
        Ok(())
    }
    /// Read all version-defined fields through Fontations, preserving bit flags.
    pub fn decode(data: &[u8], limits: &Limits) -> Result<Self> {
        limits.table(data.len())?;
        let t = Os2::read(FontData::new(data)).map_err(invalid)?;
        if t.version() > 5 {
            return Err(invalid("unsupported OS/2 version"));
        }
        let required = match t.version() {
            0 => 78,
            1 => 86,
            2..=4 => 96,
            _ => 100,
        };
        super::bytes(data, 0, required)?;
        let b = Base {
            average_width: t.x_avg_char_width(),
            weight_class: t.us_weight_class(),
            width_class: t.us_width_class(),
            embedding_flags: t.fs_type(),
            subscript: [
                t.y_subscript_x_size(),
                t.y_subscript_y_size(),
                t.y_subscript_x_offset(),
                t.y_subscript_y_offset(),
            ],
            superscript: [
                t.y_superscript_x_size(),
                t.y_superscript_y_size(),
                t.y_superscript_x_offset(),
                t.y_superscript_y_offset(),
            ],
            strikeout: [t.y_strikeout_size(), t.y_strikeout_position()],
            family_class: t.s_family_class(),
            panose: t
                .panose_10()
                .try_into()
                .map_err(|_| invalid("invalid PANOSE length"))?,
            unicode_ranges: [
                t.ul_unicode_range_1(),
                t.ul_unicode_range_2(),
                t.ul_unicode_range_3(),
                t.ul_unicode_range_4(),
            ],
            vendor_id: t.ach_vend_id().to_be_bytes(),
            selection_flags: super::u16_at(data, 62)?,
            first_char: t.us_first_char_index(),
            last_char: t.us_last_char_index(),
            typo_metrics: [
                t.s_typo_ascender(),
                t.s_typo_descender(),
                t.s_typo_line_gap(),
            ],
            windows_metrics: [t.us_win_ascent(), t.us_win_descent()],
        };
        let code_page_ranges = if t.version() >= 1 {
            Some([
                t.ul_code_page_range_1()
                    .ok_or_else(|| invalid("missing codepage range"))?,
                t.ul_code_page_range_2()
                    .ok_or_else(|| invalid("missing codepage range"))?,
            ])
        } else {
            None
        };
        let extension = if t.version() >= 2 {
            Some(Extension {
                x_height: t.sx_height().ok_or_else(|| invalid("missing x-height"))?,
                cap_height: t
                    .s_cap_height()
                    .ok_or_else(|| invalid("missing cap height"))?,
                default_char: t
                    .us_default_char()
                    .ok_or_else(|| invalid("missing default character"))?,
                break_char: t
                    .us_break_char()
                    .ok_or_else(|| invalid("missing break character"))?,
                max_context: t
                    .us_max_context()
                    .ok_or_else(|| invalid("missing max context"))?,
            })
        } else {
            None
        };
        let optical_sizes = if t.version() == 5 {
            Some([
                t.us_lower_optical_point_size()
                    .ok_or_else(|| invalid("missing optical size"))?,
                t.us_upper_optical_point_size()
                    .ok_or_else(|| invalid("missing optical size"))?,
            ])
        } else {
            None
        };
        Ok(Self {
            version: t.version(),
            base: b,
            code_page_ranges,
            extension,
            optical_sizes,
        })
    }
    /// Serialize the exact requested version. Invalid classes and combinations
    /// of version/extension fields fail rather than discarding metadata.
    pub fn encode(&self, limits: &Limits) -> Result<Vec<u8>> {
        if self.version > 5
            || (self.version >= 1) != self.code_page_ranges.is_some()
            || (self.version >= 2) != self.extension.is_some()
            || (self.version == 5) != self.optical_sizes.is_some()
        {
            return Err(invalid("OS/2 version/extension mismatch"));
        }
        let b = &self.base;
        if !(1..=1000).contains(&b.weight_class) || !(1..=9).contains(&b.width_class) {
            return Err(invalid("invalid OS/2 weight/width class"));
        }
        if self.optical_sizes.is_some_and(|[lo, hi]| lo > hi) {
            return Err(invalid("reversed optical size bounds"));
        }
        let size = match self.version {
            0 => 78,
            1 => 86,
            2..=4 => 96,
            _ => 100,
        };
        limits.table(size)?;
        let mut out = buffer(size)?;
        p16(&mut out, self.version);
        pi16(&mut out, b.average_width);
        for n in [b.weight_class, b.width_class, b.embedding_flags] {
            p16(&mut out, n);
        }
        for n in b
            .subscript
            .into_iter()
            .chain(b.superscript)
            .chain(b.strikeout)
            .chain([b.family_class])
        {
            pi16(&mut out, n);
        }
        out.extend(b.panose);
        for n in b.unicode_ranges {
            p32(&mut out, n);
        }
        out.extend(b.vendor_id);
        for n in [b.selection_flags, b.first_char, b.last_char] {
            p16(&mut out, n);
        }
        for n in b.typo_metrics {
            pi16(&mut out, n);
        }
        for n in b.windows_metrics {
            p16(&mut out, n);
        }
        if let Some(r) = self.code_page_ranges {
            for n in r {
                p32(&mut out, n);
            }
        }
        if let Some(e) = &self.extension {
            pi16(&mut out, e.x_height);
            pi16(&mut out, e.cap_height);
            for n in [e.default_char, e.break_char, e.max_context] {
                p16(&mut out, n);
            }
        }
        if let Some(r) = self.optical_sizes {
            for n in r {
                p16(&mut out, n);
            }
        }
        Ok(out)
    }
}
/// OpenType Unicode-range bits: any scalar in a block sets its bit. Bit 57
/// additionally means supplementary-plane coverage, not surrogate coverage.
pub fn unicode_ranges(codepoints: &BTreeSet<u32>) -> Result<[u32; 4]> {
    if codepoints.iter().any(|cp| char::from_u32(*cp).is_none()) {
        return Err(invalid("coverage contains non-scalar codepoint"));
    }
    let mut result = [0; 4];
    for &(start, end, bit) in UNICODE_RANGES {
        if codepoints.range(start..=end).next().is_some() {
            result[usize::from(bit / 32)] |= 1 << (bit % 32);
        }
    }
    if codepoints.range(0x10000..).next().is_some() {
        result[1] |= 1 << 25;
    }
    Ok(result)
}
/// Arithmetic mean of positive advances, rounded to nearest integer (ties up).
/// A representable signed OS/2 average is required; empty/zero input returns zero.
pub fn average_width(advances: impl IntoIterator<Item = u16>) -> Result<i16> {
    let (mut sum, mut count) = (0u64, 0u64);
    for n in advances {
        if n != 0 {
            sum = sum
                .checked_add(u64::from(n))
                .ok_or_else(|| invalid("average width overflow"))?;
            count += 1;
        }
    }
    let avg = sum
        .checked_div(count)
        .map_or(0, |q| q + u64::from(sum % count >= count.div_ceil(2)));
    i16::try_from(avg).map_err(|_| invalid("average width exceeds int16"))
}
/// Calculate selected OS/2 code-page bits from full printable repertoire
/// coverage. Supported numeric pages are 1250–1258, 874, 932, 936, 949, 950,
/// 1361, 10000 (Mac Roman), 437, 850, 852 and 866. CJK pages require the explicit
/// `cjk-charsets` feature. Other and special OEM/symbol bits remain caller policy.
pub fn code_page_ranges(codepoints: &BTreeSet<u32>, pages: &[u16]) -> Result<[u32; 2]> {
    if codepoints.iter().any(|cp| char::from_u32(*cp).is_none()) {
        return Err(invalid("coverage contains non-scalar codepoint"));
    }
    let mut out = [0; 2];
    for page in pages {
        let (bit, name, max) = match page {
            1252 => (0, "cp1252", 255),
            1250 => (1, "cp1250", 255),
            1251 => (2, "cp1251", 255),
            1253 => (3, "cp1253", 255),
            1254 => (4, "cp1254", 255),
            1255 => (5, "cp1255", 255),
            1256 => (6, "cp1256", 255),
            1257 => (7, "cp1257", 255),
            1258 => (8, "cp1258", 255),
            874 => (16, "cp874", 255),
            932 => (17, "cp932", 65535),
            936 => (18, "cp936", 65535),
            949 => (19, "cp949", 65535),
            950 => (20, "cp950", 65535),
            1361 => (21, "johab", 65535),
            10000 => (29, "macroman", 255),
            866 => (49, "cp866", 255),
            852 => (58, "cp852", 255),
            850 => (62, "cp850", 255),
            437 => (63, "cp437", 255),
            _ => return Err(invalid(format!("unsupported OS/2 code page {page}"))),
        };
        let codec = CharsetCodec::new(name, None).map_err(invalid)?;
        let supported = (0..=max)
            .filter_map(|cp| codec.cp2char(cp))
            .filter(|c| !c.is_control())
            .all(|c| codepoints.contains(&(c as u32)));
        if supported {
            out[bit / 32] |= 1 << (bit % 32);
        }
    }
    Ok(out)
}
