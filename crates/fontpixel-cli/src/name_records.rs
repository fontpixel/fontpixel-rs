//! Exact name-table record syntax shared by every sfnt writer flag set:
//! `PLATFORM/ENCODING/LANGUAGE/NAME_ID=TEXT`, `PLATFORM/ENCODING/LANGUAGE/NAME_ID=HEX`
//! and BCP 47 language tags for name format 1. Used by the OTB/bdat
//! `--otb-name-record*` flags and the outline `variants --name-record*` flags,
//! so both accept identical values and build identical tables.
use fontpixel::{Error, Result};

/// Decimal or `0x` hexadecimal number in the target type's range.
pub(crate) fn number<T: TryFrom<u64>>(value: &str) -> std::result::Result<T, String> {
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
/// `P/E/L/ID` record key: platform, encoding, language and name IDs.
fn key(value: &str) -> Result<[u16; 4]> {
    let fields = value
        .split('/')
        .map(number::<u16>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::InvalidInput)?;
    fields.try_into().map_err(|_| {
        Error::InvalidInput(format!(
            "name record key {value:?} must be PLATFORM/ENCODING/LANGUAGE/NAME_ID"
        ))
    })
}
/// Clap value parser for `P/E/L/ID=TEXT`.
pub(crate) fn record_text(value: &str) -> std::result::Result<String, String> {
    let (left, _) = value
        .split_once('=')
        .ok_or("expected PLATFORM/ENCODING/LANGUAGE/NAME_ID=TEXT")?;
    key(left).map_err(|e| e.to_string())?;
    Ok(value.to_owned())
}
/// Clap value parser for `P/E/L/ID=HEX`.
pub(crate) fn record_hex(value: &str) -> std::result::Result<String, String> {
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

/// The three repeatable record arguments of one flag set, borrowed from the
/// parsed command line, plus the wording its errors use.
#[cfg(any(feature = "opentype", feature = "otb", feature = "bdat"))]
pub(crate) struct Records<'a> {
    /// `P/E/L/ID=TEXT` values.
    pub text: &'a Vec<String>,
    /// `P/E/L/ID=HEX` values.
    pub hex: &'a Vec<String>,
    /// Language tags; the Nth is language ID 0x8000+N.
    pub tags: &'a Vec<String>,
    /// Resource name for budget errors.
    pub resource: &'static str,
    /// Flag that carries exact bytes, named in encoding errors.
    pub hex_flag: &'static str,
}

#[cfg(any(feature = "opentype", feature = "otb", feature = "bdat"))]
impl Records<'_> {
    /// Whether any record or tag was given (which replaces the whole table).
    pub(crate) fn requested(&self) -> bool {
        !self.text.is_empty() || !self.hex.is_empty() || !self.tags.is_empty()
    }
    fn error(&self, limit: usize) -> Error {
        Error::ResourceLimit {
            resource: self.resource,
            limit,
        }
    }
    /// Caller-retained bytes of the argument lists (spare capacity included).
    pub(crate) fn argument_bytes(&self, limit: usize) -> Result<usize> {
        let mut total = 0usize;
        for list in [self.text, self.hex, self.tags] {
            total = list
                .capacity()
                .checked_mul(size_of::<String>())
                .and_then(|n| total.checked_add(n))
                .filter(|n| *n <= limit)
                .ok_or_else(|| self.error(limit))?;
            for value in list {
                total = total
                    .checked_add(value.capacity())
                    .filter(|n| *n <= limit)
                    .ok_or_else(|| self.error(limit))?;
            }
        }
        Ok(total)
    }
    /// Build the complete table, or `None` when no record or tag was given.
    /// TEXT is UTF-16BE for platform 0 and Windows 3/0, 3/1, 3/10, and strict
    /// Mac Roman for 1/0; other encodings need the hex flag. The record array
    /// is reserved before it grows; the caller charges the finished table.
    pub(crate) fn table(&self, limit: usize) -> Result<Option<fontpixel::sfnt::name::NameTable>> {
        use fontpixel::sfnt::name::{NameRecord, NameTable};
        if !self.requested() {
            return Ok(None);
        }
        let count = self.text.len() + self.hex.len();
        if count.saturating_mul(size_of::<NameRecord>()) > limit {
            return Err(self.error(limit));
        }
        let mut table = NameTable::default();
        table
            .records
            .try_reserve_exact(count)
            .map_err(|_| self.error(limit))?;
        let mac_roman = fontpixel::charset::CharsetCodec::from_key("macroman", None)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        for value in self.text {
            let (left, text) = value
                .split_once('=')
                .ok_or_else(|| Error::InvalidInput("name record requires =".into()))?;
            let [platform_id, encoding_id, language_id, name_id] = key(left)?;
            let unicode =
                platform_id == 0 || (platform_id == 3 && [0, 1, 10].contains(&encoding_id));
            let mac = |c: char| {
                mac_roman
                    .char2cp(c)
                    .and_then(|code| u8::try_from(code).ok())
                    .ok_or_else(|| {
                        Error::InvalidInput(format!(
                            "name record {left}: {c:?} is not representable in Mac Roman"
                        ))
                    })
            };
            let bytes: Vec<u8> = if unicode {
                text.encode_utf16().flat_map(u16::to_be_bytes).collect()
            } else if (platform_id, encoding_id) == (1, 0) {
                text.chars().map(mac).collect::<Result<_>>()?
            } else {
                return Err(Error::InvalidInput(format!(
                    "name record {left}: use {} for this platform encoding",
                    self.hex_flag
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
        for value in self.hex {
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
            .try_reserve_exact(self.tags.len())
            .map_err(|_| self.error(limit))?;
        table.language_tags.extend(self.tags.iter().cloned());
        Ok(Some(table))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_syntax_is_validated_before_any_table() {
        assert!(record_text("3/1/0x409/1=Family").is_ok());
        assert!(record_text("3/1/0x409=Family").is_err());
        assert!(record_text("Family").is_err());
        assert!(record_hex("3/1/0x409/1=0046").is_ok());
        assert!(record_hex("3/1/0x409/1=046").is_err());
        assert!(record_hex("3/1/0x409/1=zz").is_err());
        assert_eq!(number::<u16>("0x10"), Ok(16));
        assert!(number::<u16>("65536").is_err());
    }

    #[cfg(any(feature = "opentype", feature = "otb", feature = "bdat"))]
    #[test]
    fn tables_encode_windows_mac_and_hex_records_and_reject_other_text() {
        let text = vec![
            "3/1/0x409/0=A \u{2014} B".to_owned(),
            "1/0/0/0=A \u{2014} B".to_owned(),
        ];
        let hex = vec!["3/1/0x409/1=0058".to_owned()];
        let tags = Vec::new();
        let records = Records {
            text: &text,
            hex: &hex,
            tags: &tags,
            resource: "test name records",
            hex_flag: "--name-record-hex",
        };
        let table = records.table(1 << 20).unwrap().unwrap();
        assert_eq!(
            table.records[0].value,
            [0, b'A', 0, b' ', 0x20, 0x14, 0, b' ', 0, b'B']
        );
        assert_eq!(table.records[1].value, b"A \xd1 B");
        assert_eq!(table.records[2].value, [0, b'X']);
        assert!(records.table(2).is_err());
        let other = vec!["3/2/0/1=X".to_owned()];
        let error = Records {
            text: &other,
            ..records
        }
        .table(1 << 20)
        .unwrap_err()
        .to_string();
        assert!(error.contains("--name-record-hex"), "{error}");
        let none = Vec::new();
        let empty = Records {
            text: &none,
            hex: &none,
            tags: &none,
            resource: "test name records",
            hex_flag: "--name-record-hex",
        };
        assert!(!empty.requested());
        assert!(empty.table(1 << 20).unwrap().is_none());
        assert_eq!(empty.argument_bytes(1 << 20).unwrap(), 0);
    }
}
