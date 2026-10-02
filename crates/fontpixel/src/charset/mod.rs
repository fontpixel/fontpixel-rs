//! Exact per-glyph charset conversion, generated from CPython's strict codecs.
//!
//! These are font glyph codes, not encoded text streams. In particular,
//! `iso2022cn` is SPEC-v3's GB2312 GL designation; ISO-2022 escape/shift state is
//! not accepted here. EUC-JP's three-byte JIS X 0212 and multi-character Big5-HKSCS
//! sequences are deliberately unmapped under SPEC-v3 §11.1.
//!
//! Enable `cjk-charsets` for exact multi-byte maps. Without it, opening a CJK
//! codec returns [`CharsetError::FeatureDisabled`], never an approximate mapping.
//! Unknown names retain the reference's Unicode identity fallback and are
//! distinguishable using [`CharsetCodec::known`]; a font/compatibility layer must
//! attach its own warning once per name. This module does not implement fonts.
//!
//! ```
//! use fontpixel::charset::CharsetCodec;
//! let latin = CharsetCodec::new("Windows-1252", None)?;
//! assert_eq!(latin.cp2char(0x80), Some('€'));
//! assert_eq!(latin.char2cp('€'), Some(0x80));
//! # Ok::<(), fontpixel::charset::CharsetError>(())
//! ```

use std::fmt;

#[cfg(feature = "cjk-charsets")]
mod cjk;
mod core;
mod registry;

/// Permitted per-glyph code lengths, independent of a text-stream encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// Identity mapping of Unicode code points.
    Unicode,
    /// One-byte glyph codes only.
    SingleByte,
    /// One- or two-byte big-endian glyph codes.
    DoubleByte,
    /// One-, two-, or four-byte big-endian GB18030 glyph codes.
    Gb18030,
}

#[derive(Clone, Copy, Debug)]
enum TableId {
    Core(usize),
    Cjk(usize),
}

/// A canonical SPEC-v3 charset's metadata; always visible in every build.
#[derive(Clone, Copy, Debug)]
pub struct CharsetInfo {
    /// Canonical key, such as `iso88591` or `cp932`.
    pub key: &'static str,
    /// CPython codec supplying the exact map, or `None` for Unicode identity.
    pub python_codec: Option<&'static str>,
    /// Allowed per-glyph byte lengths.
    pub kind: Kind,
    /// Whether font properties/glyph codes may select the 94×94 GL form.
    pub gl_capable: bool,
    /// Whether GL is selected by default (`iso2022cn`).
    pub force_gl: bool,
    /// `CHARSET_REGISTRY`, `CHARSET_ENCODING` written by charset conversion.
    pub properties: (&'static str, &'static str),
    /// Additional cleaned aliases; code-page folding rules are separate.
    pub aliases: &'static [&'static str],
    table: Option<TableId>,
}

impl CharsetInfo {
    /// Whether the exact table is compiled into this build.
    pub const fn available(self) -> bool {
        !matches!(self.table, Some(TableId::Cjk(_))) || cfg!(feature = "cjk-charsets")
    }
}

/// Every explicit canonical key from SPEC-v3, including Unicode identity keys.
pub fn charset_table() -> &'static [CharsetInfo] {
    registry::CHARSETS
}

fn info(key: &str) -> Option<&'static CharsetInfo> {
    charset_table()
        .iter()
        .find(|entry| entry.key == key || entry.aliases.contains(&key))
}

/// Whether a canonical key denotes Unicode identity (including UTF prefixes).
pub fn is_unicode_charset(key: &str) -> bool {
    key.is_empty()
        || registry::UNICODE_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix))
        || info(key).is_some_and(|entry| entry.kind == Kind::Unicode)
}

fn whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}')
}

fn strip(name: &str) -> String {
    name.chars()
        .filter(|&c| c != '-' && c != '_' && !whitespace(c))
        .collect::<String>()
        .to_lowercase()
}

fn digits(s: &str) -> Option<&str> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        let normalized = s.trim_start_matches('0');
        Some(if normalized.is_empty() {
            "0"
        } else {
            normalized
        })
    }
}

fn cp_key(s: &str) -> Option<&'static str> {
    let number = ["cp", "windows", "win", "ms", "ibm"]
        .iter()
        .find_map(|prefix| s.strip_prefix(prefix))
        .unwrap_or(s);
    let number = digits(number)?;
    let key = format!("cp{number}");
    info(&key).map(|entry| entry.key)
}

/// Canonicalize BDF registry/encoding properties using the reference's rules.
///
/// Only ASCII whitespace, `-`, and `_` are removed; a registry's dot suffix is
/// discarded. Registry-specific encoding values fold into the canonical key.
/// Unknown keys are retained rather than guessed. Leading zeros do not overflow.
pub fn sanitize_charset(registry: Option<&str>, encoding: Option<&str>) -> String {
    let name = registry.unwrap_or("").trim_matches(whitespace);
    let name = name.strip_prefix('-').unwrap_or(name);
    let name = name.split('.').next().unwrap_or("");
    let reg = strip(name);
    let enc = strip(encoding.unwrap_or(""));
    match reg.as_str() {
        "" => "iso10646".into(),
        "iso8859" => {
            let n = digits(&enc).filter(|&s| s != "0").unwrap_or("1");
            format!("iso8859{n}")
        }
        "koi8" => format!("koi8{}", if enc.is_empty() { "r" } else { &enc }),
        "microsoft" | "windows" | "ms" | "ibm" => cp_key(&enc).map_or(reg.clone(), String::from),
        _ => {
            if digits(&reg).is_none()
                && let Some(key) = cp_key(&reg)
            {
                return key.into();
            }
            info(&reg).map_or(reg.clone(), |entry| entry.key.into())
        }
    }
}

fn known(key: &str) -> bool {
    is_unicode_charset(key) || info(key).is_some()
}

/// Canonicalize a user-facing name, including final `-encoding` property forms.
pub fn canonical_charset(name: &str) -> String {
    let key = sanitize_charset(Some(name), None);
    if known(&key) {
        return key;
    }
    let name = name.trim_matches(whitespace);
    if let Some((reg, enc)) = name.rsplit_once('-')
        && !reg.is_empty()
    {
        let key2 = sanitize_charset(Some(reg), Some(enc));
        if known(&key2) {
            return key2;
        }
    }
    key
}

/// Property values for a canonical key, including unknown keys and aliases.
pub fn charset_props(key: &str) -> (String, String) {
    if is_unicode_charset(key) {
        return ("ISO10646".into(), "1".into());
    }
    if let Some(n) = key.strip_prefix("iso8859").and_then(digits) {
        return ("ISO8859".into(), n.into());
    }
    if let Some(entry) = info(key) {
        return (entry.properties.0.into(), entry.properties.1.into());
    }
    if key.strip_prefix("cp").and_then(digits).is_some() {
        return ("MICROSOFT".into(), key.to_uppercase());
    }
    (key.to_uppercase(), "0".into())
}

/// Error resolving a known charset whose precise table is not compiled in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CharsetError {
    /// Enable `cjk-charsets` to use this canonical charset.
    FeatureDisabled(String),
}

impl fmt::Display for CharsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FeatureDisabled(key) => {
                write!(f, "charset '{key}' requires feature 'cjk-charsets'")
            }
        }
    }
}

impl std::error::Error for CharsetError {}

#[derive(Debug)]
struct Segment {
    start: u32,
    end: u32,
    // High bit: index into dense data. Otherwise: first mapped value of a run.
    value: u32,
}

#[derive(Debug)]
struct Map {
    segments: &'static [Segment],
    data: &'static [u32],
}

impl Map {
    fn get(&self, code: u32) -> Option<u32> {
        let index = self.segments.partition_point(|s| s.start <= code);
        let segment = self.segments.get(index.checked_sub(1)?)?;
        if code > segment.end {
            return None;
        }
        let offset = code - segment.start;
        if segment.value & 0x8000_0000 == 0 {
            Some(segment.value + offset)
        } else {
            self.data
                .get(((segment.value & 0x7fff_ffff) + offset) as usize)
                .copied()
        }
    }
}

#[derive(Debug)]
struct Table {
    decode: Map,
    encode: Map,
}

/// Exact, allocation-free lookup after name resolution.
///
/// [`Self::cp2unicode`] preserves Python's Unicode-identity surrogate behavior
/// as numeric code points; [`Self::cp2char`] only returns valid Rust scalars.
#[derive(Clone, Debug)]
pub struct CharsetCodec {
    key: String,
    entry: Option<&'static CharsetInfo>,
    table: Option<&'static Table>,
    gl: bool,
    known: bool,
}

impl CharsetCodec {
    /// Resolve a user-facing charset name and optional GL mode.
    ///
    /// `None` selects GL for `iso2022cn`; explicitly passing `Some(false)`
    /// overrides it, matching Python. GL is ignored for non-94×94 charsets.
    pub fn new(name: &str, gl: Option<bool>) -> Result<Self, CharsetError> {
        Self::from_key(&canonical_charset(name), gl)
    }

    /// Resolve a canonical key or its cleaned alias without re-sanitizing it.
    pub fn from_key(key: &str, gl: Option<bool>) -> Result<Self, CharsetError> {
        let entry = info(key);
        let key = entry.map_or(key, |e| e.key).to_string();
        let known = known(&key);
        let mut table = None;
        if let Some(id) = entry.and_then(|e| e.table) {
            table = Some(match id {
                TableId::Core(index) => &core::TABLES[index],
                TableId::Cjk(index) => {
                    #[cfg(feature = "cjk-charsets")]
                    {
                        &cjk::TABLES[index]
                    }
                    #[cfg(not(feature = "cjk-charsets"))]
                    {
                        let _ = index;
                        return Err(CharsetError::FeatureDisabled(key));
                    }
                }
            });
        }
        let gl = entry.is_some_and(|e| gl.unwrap_or(e.force_gl) && (e.gl_capable || e.force_gl));
        Ok(Self {
            key,
            entry,
            table,
            gl,
            known,
        })
    }

    /// Canonical name, or the cleaned unknown name retained for diagnostics.
    pub fn name(&self) -> &str {
        &self.key
    }

    /// Whether the name is recognized (false means Unicode identity fallback).
    pub fn known(&self) -> bool {
        self.known
    }

    /// Allowed per-glyph byte lengths; unknown names are Unicode identity.
    pub fn kind(&self) -> Kind {
        self.entry.map_or(Kind::Unicode, |e| e.kind)
    }

    /// Whether this codec uses 94×94 GL codes.
    pub fn gl(&self) -> bool {
        self.gl
    }

    /// Convert a glyph code to a numeric Unicode code point.
    ///
    /// Unicode identity preserves 0xD800..=0xDFFF for SPEC/Python parity.
    /// Other tables only produce Unicode scalars. Invalid byte sequences,
    /// forbidden lengths, and multi-character decodes return `None`.
    pub fn cp2unicode(&self, mut code: u32) -> Option<u32> {
        let Some(table) = self.table else {
            return (code <= 0x10ffff).then_some(code);
        };
        if code > 0xffff {
            #[cfg(feature = "cjk-charsets")]
            if self.kind() == Kind::Gb18030 {
                return gb_decode(code);
            }
            return None;
        }
        if self.gl && code >= 256 {
            if !(0x21..=0x7e).contains(&(code >> 8)) || !(0x21..=0x7e).contains(&(code & 255)) {
                return None;
            }
            code += 0x8080;
        }
        table.decode.get(code)
    }

    /// Convert a glyph code to a Rust character, rejecting Unicode surrogates.
    pub fn cp2char(&self, code: u32) -> Option<char> {
        self.cp2unicode(code).and_then(char::from_u32)
    }

    /// Encode a numeric Unicode code point using Python's preferred code.
    ///
    /// The result always round-trips through [`Self::cp2unicode`]. Unicode
    /// identity accepts surrogates numerically; other charsets reject them.
    pub fn unicode2cp(&self, value: u32) -> Option<u32> {
        let Some(table) = self.table else {
            return (value <= 0x10ffff).then_some(value);
        };
        let code = table.encode.get(value);
        #[cfg(feature = "cjk-charsets")]
        let code = code.or_else(|| {
            (self.kind() == Kind::Gb18030)
                .then(|| gb_encode(value))
                .flatten()
        });
        let mut code = code?;
        if self.gl && code >= 256 {
            code = code.checked_sub(0x8080)?;
        }
        (self.cp2unicode(code) == Some(value)).then_some(code)
    }

    /// Encode one Rust character, accepting only a strict round-trip mapping.
    pub fn char2cp(&self, value: char) -> Option<u32> {
        self.unicode2cp(value as u32)
    }
}

/// Infer the font's GL form using SPEC-v3's smallest multi-byte glyph rule.
///
/// The iterator need not be sorted. Call again when charset properties or the
/// glyph set change; this stateless helper does not introduce a stale font cache.
pub fn detect_gl(key: &str, encoding: Option<&str>, codes: impl IntoIterator<Item = u32>) -> bool {
    let Some(entry) = info(key) else {
        return false;
    };
    if entry.force_gl {
        return true;
    }
    if !entry.gl_capable {
        return false;
    }
    if encoding == Some("0") {
        return true;
    }
    codes
        .into_iter()
        .filter(|&cp| (256..=0xffff).contains(&cp))
        .min()
        .is_some_and(|cp| (0x21..=0x7e).contains(&(cp >> 8)) && (0x21..=0x7e).contains(&(cp & 255)))
}

#[cfg(feature = "cjk-charsets")]
fn gb_decode(code: u32) -> Option<u32> {
    let [a, b, c, d] = code.to_be_bytes();
    if !(0x81..=0xfe).contains(&a)
        || !(0x30..=0x39).contains(&b)
        || !(0x81..=0xfe).contains(&c)
        || !(0x30..=0x39).contains(&d)
    {
        return None;
    }
    let index = (((u32::from(a) - 0x81) * 10 + u32::from(b) - 0x30) * 126 + u32::from(c) - 0x81)
        * 10
        + u32::from(d)
        - 0x30;
    let i = cjk::GB18030
        .partition_point(|&(start, _, _)| start <= index)
        .checked_sub(1)?;
    let (start, value, length) = cjk::GB18030[i];
    (index - start < length).then_some(value + index - start)
}

#[cfg(feature = "cjk-charsets")]
fn gb_encode(value: u32) -> Option<u32> {
    let i = cjk::GB18030
        .partition_point(|&(_, start, _)| start <= value)
        .checked_sub(1)?;
    let (index, start, length) = cjk::GB18030[i];
    if value - start >= length {
        return None;
    }
    let mut index = index + value - start;
    let d = index % 10 + 0x30;
    index /= 10;
    let c = index % 126 + 0x81;
    index /= 126;
    let b = index % 10 + 0x30;
    let a = index / 10 + 0x81;
    Some((a << 24) | (b << 16) | (c << 8) | d)
}

/// A Windows FNT `dfCharSet` requirement; contextual/symbol sets aren't guessed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsCharset {
    /// A deterministic code-page mapping supported by this module.
    CodePage(&'static str),
    /// DEFAULT_CHARSET needs the font's original system ANSI code page.
    SystemAnsi,
    /// OEM_CHARSET needs the font's original system OEM code page.
    SystemOem,
    /// MAC_CHARSET needs its Macintosh script/code page (Roman is `macroman`).
    SystemMac,
    /// SYMBOL_CHARSET requires an explicit font-specific mapping.
    Symbol,
    /// The byte is not one of the documented Windows charset values.
    Unknown(u8),
}

/// Resolve documented `dfCharSet` values without guessing a machine locale.
///
/// CJK Windows charsets use Windows CP932/936/949/950, not their subtly different
/// standard Shift-JIS/GB2312/EUC-KR/Big5 tables. This only resolves metadata;
/// a future FNT codec must enforce its own glyph-code width and loss reporting.
pub const fn windows_charset(value: u8) -> WindowsCharset {
    use WindowsCharset::{CodePage, Symbol, SystemAnsi, SystemMac, SystemOem, Unknown};
    match value {
        0 => CodePage("cp1252"),
        1 => SystemAnsi,
        2 => Symbol,
        77 => SystemMac,
        128 => CodePage("cp932"),
        129 => CodePage("cp949"),
        130 => CodePage("johab"),
        134 => CodePage("cp936"),
        136 => CodePage("cp950"),
        161 => CodePage("cp1253"),
        162 => CodePage("cp1254"),
        163 => CodePage("cp1258"),
        177 => CodePage("cp1255"),
        178 => CodePage("cp1256"),
        186 => CodePage("cp1257"),
        204 => CodePage("cp1251"),
        222 => CodePage("cp874"),
        238 => CodePage("cp1250"),
        255 => SystemOem,
        other => Unknown(other),
    }
}

/// Convert one-based 94×94 HZK/HBF row and cell numbers to an EUC/GB2312 code.
/// This validates the code grid, not whether a Unicode character is assigned.
pub const fn kuten_to_euc(row: u8, cell: u8) -> Option<u16> {
    if row < 1 || row > 94 || cell < 1 || cell > 94 {
        return None;
    }
    Some((((row as u16) + 0xa0) << 8) | (cell as u16 + 0xa0))
}

/// Convert a 94×94 EUC/GB2312 code to one-based HZK/HBF row and cell numbers.
pub const fn euc_to_kuten(code: u16) -> Option<(u8, u8)> {
    let [a, b] = code.to_be_bytes();
    if a < 0xa1 || a > 0xfe || b < 0xa1 || b > 0xfe {
        return None;
    }
    Some((a - 0xa0, b - 0xa0))
}
