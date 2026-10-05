//! Loss-preserving name records, formats 0 and 1.
use super::{Limits, bounded, buffer, bytes, invalid, p16, to16};
use crate::Result;
use read_fonts::{FontData, FontRead, tables::name::Name};
use std::collections::BTreeMap;

/// One original name record; non-Unicode encodings remain exact byte strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRecord {
    /// Platform ID.
    pub platform_id: u16,
    /// Platform encoding ID.
    pub encoding_id: u16,
    /// Platform language ID or format-1 tag index plus 0x8000.
    pub language_id: u16,
    /// OpenType name ID.
    pub name_id: u16,
    /// Original encoded bytes, without a trailing terminator.
    pub value: Vec<u8>,
}
impl NameRecord {
    /// Windows Unicode (encoding 1), explicitly chosen language and name ID.
    pub fn unicode(name_id: u16, language_id: u16, value: &str) -> Self {
        Self {
            platform_id: 3,
            encoding_id: 1,
            language_id,
            name_id,
            value: utf16(value),
        }
    }
    /// Decode a Unicode-platform or Windows Unicode record strictly. Other
    /// platforms return `None`, so unknown encodings are never guessed or lost.
    pub fn text(&self) -> Result<Option<String>> {
        if self.platform_id == 0
            || (self.platform_id == 3 && [0, 1, 10].contains(&self.encoding_id))
        {
            Ok(Some(decode_utf16(&self.value)?))
        } else {
            Ok(None)
        }
    }
    fn key(&self) -> (u16, u16, u16, u16) {
        (
            self.platform_id,
            self.encoding_id,
            self.language_id,
            self.name_id,
        )
    }
}
/// Owned editable name table. Language tags select format 1 when nonempty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameTable {
    /// Name records, preserving source order when decoded.
    pub records: Vec<NameRecord>,
    /// BCP-47 language strings indexed from 0x8000; stored as UTF-16BE on wire.
    pub language_tags: Vec<String>,
}
fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}
fn decode_utf16(data: &[u8]) -> Result<String> {
    if !data.len().is_multiple_of(2) {
        return Err(invalid("odd UTF-16 byte length"));
    }
    let units: Vec<_> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| invalid("invalid UTF-16 name string"))
}
fn language_valid(id: u16, count: usize) -> bool {
    id < 0x8000 || usize::from(id - 0x8000) < count
}
impl NameTable {
    /// Parse all records without normalizing their raw values. UTF-16 language
    /// tags are validated; record text decoding is an explicit fallible step.
    pub fn decode(data: &[u8], limits: &Limits) -> Result<Self> {
        limits.table(data.len())?;
        let t = Name::read(FontData::new(data)).map_err(invalid)?;
        if t.version() > 1 {
            return Err(invalid("unsupported name version"));
        }
        let lang_count = t.lang_tag_count().unwrap_or(0) as usize;
        if t.name_record().len() != usize::from(t.count())
            || t.lang_tag_record().unwrap_or(&[]).len() != lang_count
        {
            return Err(invalid("truncated name records"));
        }
        bounded(
            t.count() as usize + lang_count,
            limits.max_name_records,
            "sfnt name records",
        )?;
        let directory_end = 6
            + 12 * usize::from(t.count())
            + if t.version() == 1 {
                2 + 4 * lang_count
            } else {
                0
            };
        if usize::from(t.storage_offset()) < directory_end {
            return Err(invalid("name strings overlap records"));
        }
        let storage = data
            .get(t.storage_offset() as usize..)
            .ok_or_else(|| invalid("invalid name storage offset"))?;
        let mut total = 0usize;
        for r in t.name_record() {
            total += usize::from(r.length());
        }
        for r in t.lang_tag_record().unwrap_or(&[]) {
            total += usize::from(r.length());
        }
        limits.table(total)?;
        let mut language_tags = Vec::new();
        for r in t.lang_tag_record().unwrap_or(&[]) {
            let raw = bytes(
                storage,
                r.lang_tag_offset().to_u32() as usize,
                r.length() as usize,
            )?;
            language_tags.push(decode_utf16(raw)?);
        }
        let mut records = Vec::new();
        for r in t.name_record() {
            if !language_valid(r.language_id(), lang_count) {
                return Err(invalid("name language index out of bounds"));
            }
            let value = bytes(
                storage,
                r.string_offset().to_u32() as usize,
                r.length() as usize,
            )?
            .to_vec();
            records.push(NameRecord {
                platform_id: r.platform_id(),
                encoding_id: r.encoding_id(),
                language_id: r.language_id(),
                name_id: r.name_id().to_u16(),
                value,
            });
        }
        Ok(Self {
            records,
            language_tags,
        })
    }
    /// Sort records and deduplicate identical strings. Duplicate keys are kept
    /// in stable order; no string or record is silently omitted.
    pub fn encode(&self, limits: &Limits) -> Result<Vec<u8>> {
        bounded(
            self.records.len() + self.language_tags.len(),
            limits.max_name_records,
            "sfnt name records",
        )?;
        to16(self.records.len())?;
        if self.language_tags.len() > 0x8000 {
            return Err(invalid("too many language tags"));
        }
        let version = u16::from(!self.language_tags.is_empty());
        let storage_start = 6
            + 12 * self.records.len()
            + if version == 1 {
                2 + 4 * self.language_tags.len()
            } else {
                0
            };
        to16(storage_start)?;
        limits.table(storage_start)?;
        let mut size = storage_start;
        for r in &self.records {
            to16(r.value.len())?;
            if !language_valid(r.language_id, self.language_tags.len()) {
                return Err(invalid("name language index out of bounds"));
            }
            size = size
                .checked_add(r.value.len())
                .ok_or_else(|| invalid("name size overflow"))?;
        }
        for tag in &self.language_tags {
            size = size
                .checked_add(
                    tag.len()
                        .checked_mul(2)
                        .ok_or_else(|| invalid("name size overflow"))?,
                )
                .ok_or_else(|| invalid("name size overflow"))?;
        }
        limits.table(size)?;
        let mut out = buffer(size)?;
        p16(&mut out, version);
        p16(&mut out, to16(self.records.len())?);
        p16(&mut out, to16(storage_start)?);
        let mut records: Vec<_> = self.records.iter().collect();
        records.sort_by_key(|r| r.key());
        let mut strings = Vec::new();
        let mut offsets = BTreeMap::<Vec<u8>, u16>::new();
        fn store(
            raw: &[u8],
            data: &mut Vec<u8>,
            offsets: &mut BTreeMap<Vec<u8>, u16>,
        ) -> Result<u16> {
            if let Some(&off) = offsets.get(raw) {
                return Ok(off);
            }
            let off = to16(data.len())?;
            to16(raw.len())?;
            data.extend(raw);
            offsets.insert(raw.to_vec(), off);
            Ok(off)
        }
        for r in records {
            for n in [
                r.platform_id,
                r.encoding_id,
                r.language_id,
                r.name_id,
                to16(r.value.len())?,
                store(&r.value, &mut strings, &mut offsets)?,
            ] {
                p16(&mut out, n);
            }
        }
        if version == 1 {
            p16(&mut out, to16(self.language_tags.len())?);
            for tag in &self.language_tags {
                let raw = utf16(tag);
                p16(&mut out, to16(raw.len())?);
                p16(&mut out, store(&raw, &mut strings, &mut offsets)?);
            }
        }
        out.extend(strings);
        Ok(out)
    }
}

/// RFC 5646 well-formedness only: no registry lookup and no language inference.
pub fn language_tag_well_formed(tag: &str) -> bool {
    const IRREGULAR: &[&str] = &[
        "en-GB-oed",
        "i-ami",
        "i-bnn",
        "i-default",
        "i-enochian",
        "i-hak",
        "i-klingon",
        "i-lux",
        "i-mingo",
        "i-navajo",
        "i-pwn",
        "i-tao",
        "i-tay",
        "i-tsu",
        "sgn-BE-FR",
        "sgn-BE-NL",
        "sgn-CH-DE",
        "art-lojban",
        "cel-gaulish",
        "no-bok",
        "no-nyn",
        "zh-guoyu",
        "zh-hakka",
        "zh-min",
        "zh-min-nan",
        "zh-xiang",
    ];
    if IRREGULAR.iter().any(|old| tag.eq_ignore_ascii_case(old)) {
        return true;
    }
    fn alpha(s: &str) -> bool {
        s.bytes().all(|b| b.is_ascii_alphabetic())
    }
    fn alnum(s: &str) -> bool {
        s.bytes().all(|b| b.is_ascii_alphanumeric())
    }
    fn private<'a>(rest: impl Iterator<Item = &'a str>) -> bool {
        let mut count = 0;
        for s in rest {
            if !(1..=8).contains(&s.len()) || !alnum(s) {
                return false;
            }
            count += 1;
        }
        count > 0
    }
    let mut parts = tag.split('-').peekable();
    let Some(language) = parts.next() else {
        return false;
    };
    if language.eq_ignore_ascii_case("x") {
        return private(parts);
    }
    if !(2..=8).contains(&language.len()) || !alpha(language) {
        return false;
    }
    if language.len() <= 3 {
        // Up to three extended language subtags.
        for _ in 0..3 {
            if parts.peek().is_some_and(|s| s.len() == 3 && alpha(s)) {
                parts.next();
            } else {
                break;
            }
        }
    }
    if parts.peek().is_some_and(|s| s.len() == 4 && alpha(s)) {
        parts.next();
    }
    if parts.peek().is_some_and(|s| {
        (s.len() == 2 && alpha(s)) || (s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
    }) {
        parts.next();
    }
    let variant = |s: &str| {
        alnum(s)
            && ((5..=8).contains(&s.len()) || (s.len() == 4 && s.as_bytes()[0].is_ascii_digit()))
    };
    while parts.peek().is_some_and(|s| variant(s)) {
        parts.next();
    }
    let mut singletons = 0u64;
    while let Some(singleton) = parts.next() {
        if singleton.eq_ignore_ascii_case("x") {
            return private(parts);
        }
        if singleton.len() != 1 || !alnum(singleton) {
            return false;
        }
        let value = singleton.as_bytes()[0].to_ascii_lowercase();
        let bit = if value.is_ascii_digit() {
            value - b'0'
        } else {
            value - b'a' + 10
        };
        if singletons & (1u64 << bit) != 0 {
            return false;
        }
        singletons |= 1u64 << bit;
        let mut count = 0;
        while parts
            .peek()
            .is_some_and(|s| (2..=8).contains(&s.len()) && alnum(s))
        {
            parts.next();
            count += 1;
        }
        if count == 0 {
            return false;
        }
    }
    true
}

fn postscript_byte(byte: u8) -> bool {
    (33..=126).contains(&byte) && !b"[](){}<>/%".contains(&byte)
}
fn utf16_platform(record: &NameRecord) -> bool {
    record.platform_id == 0 || (record.platform_id == 3 && [0, 1, 10].contains(&record.encoding_id))
}
fn working_limit(maximum: usize) -> crate::Error {
    crate::Error::ResourceLimit {
        resource: "sfnt name working bytes",
        limit: maximum,
    }
}

impl NameTable {
    /// Validate an explicitly configured table before any wire allocation.
    ///
    /// Platforms 0 (Unicode, encodings 0..=4), 1 (Macintosh, 0..=32) and 3
    /// (Windows, 0..=6 and 10) are accepted. Unicode-platform and Windows
    /// Unicode values must be well-formed UTF-16BE; other encodings keep their
    /// exact bytes. Name ID 6 must be a 1..=63-character PostScript name.
    /// Language-tag indexes must exist, tags must be well-formed BCP 47, and a
    /// (platform, encoding, language, name ID) key may occur only once.
    pub fn validate_configured(&self, limits: &Limits) -> Result<()> {
        bounded(
            self.records
                .len()
                .checked_add(self.language_tags.len())
                .ok_or_else(|| invalid("name count overflow"))?,
            limits.max_name_records,
            "sfnt name records",
        )?;
        to16(self.records.len())?;
        if self.language_tags.len() > 0x8000 {
            return Err(invalid("too many language tags"));
        }
        for tag in &self.language_tags {
            if !language_tag_well_formed(tag) {
                return Err(invalid("invalid BCP 47 name language tag"));
            }
        }
        for record in &self.records {
            let known = match record.platform_id {
                0 => record.encoding_id <= 4,
                1 => record.encoding_id <= 32,
                3 => record.encoding_id <= 6 || record.encoding_id == 10,
                _ => false,
            };
            if !known {
                return Err(invalid("unsupported name platform/encoding pair"));
            }
            if !language_valid(record.language_id, self.language_tags.len()) {
                return Err(invalid("name language index out of bounds"));
            }
            if record.platform_id == 0 && (1..0x8000).contains(&record.language_id) {
                return Err(invalid(
                    "Unicode-platform name language must be zero or a language-tag index",
                ));
            }
            to16(record.value.len())?;
            if utf16_platform(record) {
                decode_utf16(&record.value)?;
                if record.name_id == 6
                    && (record.value.is_empty()
                        || record.value.len() > 126
                        || !record
                            .value
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .all(|unit| unit[0] == 0 && postscript_byte(unit[1])))
                {
                    return Err(invalid("invalid PostScript name record"));
                }
            } else if record.name_id == 6
                && (record.value.is_empty()
                    || record.value.len() > 63
                    || !record.value.iter().copied().all(postscript_byte))
            {
                return Err(invalid("invalid PostScript name record"));
            }
        }
        let mut keys = Vec::new();
        keys.try_reserve_exact(self.records.len())
            .map_err(|_| invalid("name key allocation failed"))?;
        keys.extend(self.records.iter().map(NameRecord::key));
        keys.sort_unstable();
        if keys.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid("duplicate name record key"));
        }
        Ok(())
    }
    /// Retained logical storage of this table, including spare capacity of
    /// every record value, tag string and both vectors. Allocator overhead is
    /// outside the logical budget.
    pub fn retained_bytes(&self, maximum: usize) -> Result<usize> {
        let mut bytes = std::mem::size_of::<Self>();
        let mut add = |amount: Option<usize>| -> Result<()> {
            bytes = amount
                .and_then(|amount| bytes.checked_add(amount))
                .filter(|n| *n <= maximum)
                .ok_or_else(|| working_limit(maximum))?;
            Ok(())
        };
        add(self
            .records
            .capacity()
            .checked_mul(std::mem::size_of::<NameRecord>()))?;
        add(self
            .language_tags
            .capacity()
            .checked_mul(std::mem::size_of::<String>()))?;
        for record in &self.records {
            add(Some(record.value.capacity()))?;
        }
        for tag in &self.language_tags {
            add(Some(tag.capacity()))?;
        }
        Ok(bytes)
    }
    /// Validate and serialize an explicitly configured table.
    ///
    /// The table's retained storage, the validation key array, one sort-order
    /// array and the exact output allocation are charged together against
    /// `limits.resources.max_total_bytes`. Records are written in canonical key
    /// order. Strings are stored once per record, in that order, without
    /// deduplication; language tags follow. This differs from [`Self::encode`]
    /// only in string storage layout.
    pub fn encode_configured(&self, limits: &Limits) -> Result<Vec<u8>> {
        let maximum = limits.resources.max_total_bytes;
        let mut used = self.retained_bytes(maximum)?;
        let mut charge = |amount: Option<usize>| -> Result<()> {
            used = amount
                .and_then(|amount| used.checked_add(amount))
                .filter(|n| *n <= maximum)
                .ok_or_else(|| working_limit(maximum))?;
            Ok(())
        };
        // Validation sorts one key per record; the key array is freed before
        // the order array below is allocated, and both have the same bound.
        let slot = std::mem::size_of::<(u16, u16, u16, u16)>().max(std::mem::size_of::<usize>());
        charge(self.records.len().checked_mul(slot))?;
        self.validate_configured(limits)?;
        let version = u16::from(!self.language_tags.is_empty());
        let start = 6
            + 12 * self.records.len()
            + if version == 1 {
                2 + 4 * self.language_tags.len()
            } else {
                0
            };
        to16(start)?;
        let mut size = start;
        for record in &self.records {
            size = size
                .checked_add(record.value.len())
                .ok_or_else(|| invalid("name size overflow"))?;
        }
        for tag in &self.language_tags {
            size = size
                .checked_add(tag.encode_utf16().count() * 2)
                .ok_or_else(|| invalid("name size overflow"))?;
        }
        limits.table(size)?;
        charge(Some(size))?;
        let mut order = Vec::<usize>::new();
        order
            .try_reserve_exact(self.records.len())
            .map_err(|_| invalid("name order allocation failed"))?;
        charge((order.capacity() - self.records.len()).checked_mul(std::mem::size_of::<usize>()))?;
        order.extend(0..self.records.len());
        // Keys are unique after validation, so an unstable sort is deterministic.
        order.sort_unstable_by_key(|&index| self.records[index].key());
        let mut out = buffer(size)?;
        charge(Some(out.capacity() - size))?;
        p16(&mut out, version);
        p16(&mut out, to16(self.records.len())?);
        p16(&mut out, to16(start)?);
        let mut offset = 0usize;
        for &index in &order {
            let record = &self.records[index];
            for value in [
                record.platform_id,
                record.encoding_id,
                record.language_id,
                record.name_id,
                to16(record.value.len())?,
                to16(offset)?,
            ] {
                p16(&mut out, value);
            }
            offset += record.value.len();
        }
        if version == 1 {
            p16(&mut out, to16(self.language_tags.len())?);
            for tag in &self.language_tags {
                let length = tag.encode_utf16().count() * 2;
                p16(&mut out, to16(length)?);
                p16(&mut out, to16(offset)?);
                offset += length;
            }
        }
        for &index in &order {
            out.extend_from_slice(&self.records[index].value);
        }
        for tag in &self.language_tags {
            for unit in tag.encode_utf16() {
                out.extend_from_slice(&unit.to_be_bytes());
            }
        }
        debug_assert_eq!(out.len(), size);
        Ok(out)
    }
}

#[cfg(test)]
mod configured_tests {
    use super::*;

    #[test]
    fn configured_names_keep_exact_records_tags_and_storage_order() {
        let table = NameTable {
            records: vec![
                NameRecord::unicode(17, 0x8000, "Regular"),
                NameRecord {
                    platform_id: 1,
                    encoding_id: 0,
                    language_id: 0,
                    name_id: 0,
                    value: vec![0xd1],
                },
                NameRecord::unicode(16, 0x409, "Family"),
                NameRecord::unicode(1, 0x409, "Family"),
            ],
            language_tags: vec!["zh-Hant-HK".into()],
        };
        let bytes = table.encode_configured(&Limits::default()).unwrap();
        let decoded = NameTable::decode(&bytes, &Limits::default()).unwrap();
        let mut expected = table.clone();
        expected.records.sort_by_key(NameRecord::key);
        assert_eq!(decoded, expected);
        // No string sharing: IDs 1 and 16 each store "Family".
        let shared = table.encode(&Limits::default()).unwrap();
        assert_eq!(bytes.len(), shared.len() + 12);
        assert_eq!(
            NameTable::decode(&shared, &Limits::default()).unwrap(),
            decoded
        );
    }

    #[test]
    fn configured_names_charge_spare_capacity_at_the_exact_boundary() {
        let mut table = NameTable {
            records: vec![NameRecord::unicode(0, 0x409, "notice")],
            ..Default::default()
        };
        table.records[0].value.reserve_exact(32768);
        let retained = table.retained_bytes(usize::MAX).unwrap();
        assert!(retained > 32768);
        let (mut low, mut high) = (0, retained + 4096);
        while low < high {
            let mid = low + (high - low) / 2;
            let mut limits = Limits::default();
            limits.resources.max_total_bytes = mid;
            if table.encode_configured(&limits).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let size = 6 + 12 + "notice".len() * 2;
        let slot = std::mem::size_of::<(u16, u16, u16, u16)>().max(std::mem::size_of::<usize>());
        assert_eq!(low, retained + size + slot);
        let mut limits = Limits::default();
        limits.resources.max_total_bytes = low;
        table.encode_configured(&limits).unwrap();
        limits.resources.max_total_bytes -= 1;
        assert!(matches!(
            table.encode_configured(&limits),
            Err(crate::Error::ResourceLimit {
                resource: "sfnt name working bytes",
                ..
            })
        ));
    }

    #[test]
    fn configured_names_reject_invalid_keys_text_tags_and_postscript_names() {
        let valid = NameRecord::unicode(1, 0x409, "Family");
        let mut bad = Vec::new();
        for (platform, encoding) in [(2, 0), (4, 0), (0, 5), (1, 33), (3, 7), (240, 0)] {
            let mut record = valid.clone();
            record.platform_id = platform;
            record.encoding_id = encoding;
            bad.push(vec![record]);
        }
        let mut record = valid.clone();
        record.language_id = 0x8000;
        bad.push(vec![record]);
        let mut record = valid.clone();
        record.platform_id = 0;
        record.encoding_id = 3;
        record.language_id = 0x409;
        bad.push(vec![record]);
        for value in [vec![0], vec![0xd8, 0]] {
            let mut record = valid.clone();
            record.value = value;
            bad.push(vec![record]);
        }
        bad.push(vec![NameRecord::unicode(6, 0x409, "Invalid Name")]);
        bad.push(vec![NameRecord::unicode(6, 0x409, "")]);
        bad.push(vec![NameRecord::unicode(6, 0x409, &"A".repeat(64))]);
        bad.push(vec![NameRecord {
            platform_id: 1,
            encoding_id: 0,
            language_id: 0,
            name_id: 6,
            value: b"Bad(Name)".to_vec(),
        }]);
        bad.push(vec![valid.clone(), valid.clone()]);
        for records in bad {
            let table = NameTable {
                records,
                ..Default::default()
            };
            assert!(table.validate_configured(&Limits::default()).is_err());
            assert!(table.encode_configured(&Limits::default()).is_err());
        }
        NameTable {
            records: vec![NameRecord::unicode(6, 0x409, &"A".repeat(63))],
            ..Default::default()
        }
        .encode_configured(&Limits::default())
        .unwrap();
        for tag in ["", "en_US", "en-", "en-a", "en-a-foo-a-bar", "x", "1234"] {
            assert!(!language_tag_well_formed(tag), "{tag}");
            let table = NameTable {
                records: vec![NameRecord::unicode(1, 0x8000, "Family")],
                language_tags: vec![tag.into()],
            };
            assert!(table.encode_configured(&Limits::default()).is_err());
        }
        for tag in [
            "en",
            "zh-Hant-HK",
            "de-CH-1901",
            "en-a-foo-x-test",
            "x-private",
            "i-klingon",
            "es-419",
            "zh-yue-HK",
        ] {
            assert!(language_tag_well_formed(tag), "{tag}");
        }
    }

    #[test]
    fn configured_names_reject_sixteen_bit_offsets_without_truncation() {
        let table = NameTable {
            records: vec![
                NameRecord::unicode(0, 0x409, &"a".repeat(32767)),
                NameRecord::unicode(1, 0x409, "ab"),
                NameRecord::unicode(2, 0x409, "c"),
            ],
            ..Default::default()
        };
        assert!(table.encode_configured(&Limits::default()).is_err());
    }
}
