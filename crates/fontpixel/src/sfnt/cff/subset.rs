//! Preserve raw Type 2 programs, hints, widths and subroutines during compaction.
use super::*;
use read_fonts::ps::cff::dict::Entry;
use std::collections::BTreeMap;
// DICT operators encoded as byte or 0x0c00 | escaped byte.
const CHARSET: u16 = 15;
const CHARSTRINGS: u16 = 17;
const PRIVATE: u16 = 18;
const SUBRS: u16 = 19;
const FDARRAY: u16 = 0x0c24;
const FDSELECT: u16 = 0x0c25;
fn integer(v: usize) -> Result<Vec<u8>> {
    let n = i32::try_from(v).map_err(|_| invalid("CFF offset overflow"))?;
    let mut b = vec![29];
    b.extend(n.to_be_bytes());
    Ok(b)
}
fn arguments(values: &[usize]) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    for &v in values {
        b.extend(integer(v)?);
    }
    Ok(b)
}
/// Split validated DICT entries without changing unknown numeric representations.
fn parts(data: &[u8]) -> Result<Vec<(u16, &[u8])>> {
    let mut pos = 0;
    let mut start = 0;
    let mut out = Vec::new();
    while pos < data.len() {
        let b = data[pos];
        pos += 1;
        match b {
            28 => {
                bytes(data, pos, 2)?;
                pos += 2
            }
            29 => {
                bytes(data, pos, 4)?;
                pos += 4
            }
            30 => loop {
                let n = *bytes(data, pos, 1)?.first().unwrap();
                pos += 1;
                if n & 15 == 15 || n >> 4 == 15 {
                    break;
                }
            },
            32..=246 => (),
            247..=254 => {
                bytes(data, pos, 1)?;
                pos += 1
            }
            0..=27 => {
                let op = if b == 12 {
                    let n = *bytes(data, pos, 1)?.first().unwrap();
                    pos += 1;
                    0x0c00 | u16::from(n)
                } else {
                    u16::from(b)
                };
                out.push((op, &data[start..pos]));
                start = pos;
            }
            _ => return Err(invalid("invalid CFF DICT token")),
        }
    }
    if start != data.len() {
        return Err(invalid("unterminated CFF DICT"));
    }
    Ok(out)
}
fn rewrite(data: &[u8], replacements: &BTreeMap<u16, Vec<u8>>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut left = replacements.clone();
    for (op, raw) in parts(data)? {
        if let Some(value) = left.remove(&op) {
            out.extend(value);
            if op > 255 {
                out.push(12);
            }
            out.push(op as u8);
        } else {
            out.extend(raw);
        }
    }
    for (op, value) in left {
        out.extend(value);
        if op > 255 {
            out.push(12);
        }
        out.push(op as u8);
    }
    Ok(out)
}
fn entries(data: &[u8], l: &Limits, total: &mut usize) -> Result<Vec<Entry>> {
    super::read::dictionary(data, l, total)
}
fn private_blob(
    data: &[u8],
    range: std::ops::Range<usize>,
    l: &Limits,
    total: &mut usize,
) -> Result<(Vec<u8>, usize)> {
    let original = data
        .get(range.clone())
        .ok_or_else(|| invalid("CFF private range outside table"))?;
    let e = entries(original, l, total)?;
    let sub = e.iter().find_map(|e| {
        if let Entry::SubrsOffset(n) = e {
            Some(*n)
        } else {
            None
        }
    });
    if let Some(off) = sub {
        let at = range
            .start
            .checked_add(off)
            .ok_or_else(|| invalid("CFF Subrs offset overflow"))?;
        let (_, end) = super::read::index_at(data, at)?;
        let mut change = BTreeMap::from([(SUBRS, arguments(&[0])?)]);
        let len = rewrite(original, &change)?.len();
        change.insert(SUBRS, arguments(&[len])?);
        let mut out = rewrite(original, &change)?;
        out.extend(bytes(data, at, end - at)?);
        Ok((out, len))
    } else {
        Ok((original.to_vec(), original.len()))
    }
}
impl Cff<'_> {
    /// Compact selected glyphs, retaining Type 2 bytecode, hints and all subroutines.
    /// `glyphs` is new glyph order and must begin with original glyph zero.
    /// Legacy nondefault CFF encodings are rejected instead of silently changed.
    pub fn subset(&self, glyphs: &[u16], l: &Limits) -> Result<Vec<u8>> {
        l.sfnt.glyphs(glyphs.len())?;
        if glyphs.first() != Some(&0)
            || glyphs.iter().any(|&g| usize::from(g) >= self.len())
            || glyphs
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != glyphs.len()
        {
            return Err(invalid("invalid CFF subset glyph order"));
        }
        let data = self.data();
        let (names, end) = super::read::index_at(data, usize::from(data[2]))?;
        let (top, end) = super::read::index_at(data, end)?;
        let (strings, end) = super::read::index_at(data, end)?;
        let (global, _) = super::read::index_at(data, end)?;
        let original = top.get(0).ok_or_else(|| invalid("missing CFF Top DICT"))?;
        let mut dict_work = 0;
        let e = entries(original, l, &mut dict_work)?;
        let mut cs = 0;
        let mut fdarray = None;
        let mut private = None;
        for item in &e {
            match item {
                Entry::CharstringsOffset(n) => cs = *n,
                Entry::FdArrayOffset(n) => fdarray = Some(*n),
                Entry::PrivateDictRange(r) => private = Some(r.clone()),
                Entry::Encoding(n) if *n != 0 => {
                    return Err(invalid("subset of explicit CFF encoding is unsupported"));
                }
                _ => (),
            }
        }
        let (charstrings, _) = super::read::index_at(data, cs)?;
        // Validate every retained program under aggregate execution limits, before
        // copying raw bytes. Unused local/global programs remain inert and intact.
        self.validate_selected(glyphs, l)?;
        let items = glyphs
            .iter()
            .map(|&id| {
                charstrings
                    .get(usize::from(id))
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| invalid("missing CFF charstring"))
            })
            .collect::<Result<Vec<_>>>()?;
        let cs = super::write::index(&items, l)?;
        let index_copy = |index: Index<'_>| -> Result<Vec<u8>> {
            let items = (0..index.count())
                .map(|i| {
                    index
                        .get(i as usize)
                        .map(<[u8]>::to_vec)
                        .ok_or_else(|| invalid("invalid CFF INDEX"))
                })
                .collect::<Result<Vec<_>>>()?;
            super::write::index(&items, l)
        };
        let names = index_copy(names)?;
        let strings = index_copy(strings)?;
        let global = index_copy(global)?;
        let mut charset = vec![0];
        for &gid in &glyphs[1..] {
            p16(
                &mut charset,
                self.identifier(gid)
                    .ok_or_else(|| invalid("missing CFF charset identifier"))?,
            );
        }
        let mut changes =
            BTreeMap::from([(CHARSET, arguments(&[0])?), (CHARSTRINGS, arguments(&[0])?)]);
        // Some valid CID fonts also retain an unused Top DICT Private entry.
        // Its bytes still need relocation: leaving an old offset can invalidate
        // the whole CFF table even though glyph execution uses only FD privates.
        let top_private = if fdarray.is_some() {
            private
                .clone()
                .map(|range| private_blob(data, range, l, &mut dict_work))
                .transpose()?
        } else {
            None
        };
        if let Some((_, len)) = &top_private {
            changes.insert(PRIVATE, arguments(&[*len, 0])?);
        }
        let mut private_bytes = top_private.as_ref().map_or(0, |(data, _)| data.len());
        let mut private_blocks = Vec::new();
        let mut font_dicts = Vec::new();
        let mut selectors = Vec::new();
        if let Some(off) = fdarray {
            let (index, _) = super::read::index_at(data, off)?;
            // Retaining every FD avoids changing FDSelect identity and preserves
            // per-FD matrices/metadata even when some become unused.
            for i in 0..index.count() {
                let raw = index
                    .get(i as usize)
                    .ok_or_else(|| invalid("missing CFF FD"))?;
                let e = entries(raw, l, &mut dict_work)?;
                let range = e
                    .into_iter()
                    .find_map(|e| {
                        if let Entry::PrivateDictRange(r) = e {
                            Some(r)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| invalid("missing CFF FD private dict"))?;
                let blob = private_blob(data, range, l, &mut dict_work)?;
                add_budget(
                    &mut private_bytes,
                    blob.0.len(),
                    l.sfnt.resources.max_table_bytes,
                    "CFF retained private bytes",
                )?;
                private_blocks.push(blob);
                font_dicts.push(raw.to_vec());
            }
            selectors.push(0);
            for &gid in glyphs {
                selectors.push(self.subfont_selector(gid)?);
            }
            changes.insert(FDARRAY, arguments(&[0])?);
            changes.insert(FDSELECT, arguments(&[0])?);
        } else {
            private_blocks.push(private_blob(
                data,
                private.ok_or_else(|| invalid("missing CFF private dict"))?,
                l,
                &mut dict_work,
            )?);
            changes.insert(PRIVATE, arguments(&[private_blocks[0].1, 0])?);
        }
        let template = super::write::index(&[rewrite(original, &changes)?], l)?;
        let prefix = 4 + names.len() + template.len() + strings.len() + global.len();
        let charset_at = prefix;
        let cs_at = charset_at + charset.len();
        let select_at = cs_at + cs.len();
        let array_at = select_at + selectors.len();
        let placeholder_fds = font_dicts
            .iter()
            .enumerate()
            .map(|(i, raw)| {
                rewrite(
                    raw,
                    &BTreeMap::from([(PRIVATE, arguments(&[private_blocks[i].1, 0])?)]),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let fd_template = if font_dicts.is_empty() {
            Vec::new()
        } else {
            super::write::index(&placeholder_fds, l)?
        };
        let mut private_at = array_at + fd_template.len();
        let mut new_fds = Vec::new();
        for (i, raw) in font_dicts.iter().enumerate() {
            new_fds.push(rewrite(
                raw,
                &BTreeMap::from([(PRIVATE, arguments(&[private_blocks[i].1, private_at])?)]),
            )?);
            private_at += private_blocks[i].0.len();
        }
        if let Some((_, len)) = &top_private {
            changes.insert(PRIVATE, arguments(&[*len, private_at])?);
        }
        changes.insert(CHARSET, arguments(&[charset_at])?);
        changes.insert(CHARSTRINGS, arguments(&[cs_at])?);
        if font_dicts.is_empty() {
            changes.insert(PRIVATE, arguments(&[private_blocks[0].1, array_at])?);
        } else {
            changes.insert(FDARRAY, arguments(&[array_at])?);
            changes.insert(FDSELECT, arguments(&[select_at])?);
        }
        let mut out = vec![1, 0, 4, 4];
        out.extend(names);
        out.extend(super::write::index(&[rewrite(original, &changes)?], l)?);
        out.extend(strings);
        out.extend(global);
        out.extend(charset);
        out.extend(cs);
        out.extend(selectors);
        if !new_fds.is_empty() {
            out.extend(super::write::index(&new_fds, l)?);
        }
        for (b, _) in private_blocks {
            l.sfnt.table(out.len().saturating_add(b.len()))?;
            out.extend(b);
        }
        if let Some((data, _)) = top_private {
            l.sfnt.table(out.len().saturating_add(data.len()))?;
            out.extend(data);
        }
        l.sfnt.table(out.len())?;
        Ok(out)
    }
}
