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
