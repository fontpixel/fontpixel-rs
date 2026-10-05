//! Preserve raw Type 2 programs, hints, widths and subroutines during compaction.
//!
//! The output is laid out twice by one writer: first into a byte counter (every
//! rewritten DICT operand uses the fixed five-byte integer form, so lengths do
//! not depend on offset values), then into one exact, ledger-admitted buffer.
//! No INDEX item, DICT template or Private block is copied in between.
use super::read::{IdSet, dictionary, index_at};
use super::*;
use crate::working::Buffer;
use read_fonts::ps::cff::dict::Entry;
// DICT operators encoded as byte or 0x0c00 | escaped byte.
const CHARSET: u16 = 15;
const CHARSTRINGS: u16 = 17;
const PRIVATE: u16 = 18;
const SUBRS: u16 = 19;
const FDARRAY: u16 = 0x0c24;
const FDSELECT: u16 = 0x0c25;

/// Output sink: a layout counter or the final ledger buffer.
trait Out {
    fn put(&mut self, bytes: &[u8]) -> Result<()>;
    fn len(&self) -> usize;
}
struct Count(usize);
impl Out for Count {
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| invalid("CFF size overflow"))?;
        Ok(())
    }
    fn len(&self) -> usize {
        self.0
    }
}
impl Out for Buffer {
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        self.extend(bytes)
    }
    fn len(&self) -> usize {
        self.as_slice().len()
    }
}
fn integer(out: &mut impl Out, v: usize) -> Result<()> {
    let n = i32::try_from(v).map_err(|_| invalid("CFF offset overflow"))?;
    out.put(&[29])?;
    out.put(&n.to_be_bytes())
}
fn operator(out: &mut impl Out, op: u16, values: &[usize]) -> Result<()> {
    for &v in values {
        integer(out, v)?;
    }
    if op > 255 {
        out.put(&[12])?;
    }
    out.put(&[op as u8])
}
/// Visit validated DICT entries without changing unknown numeric representations.
fn for_parts<'a>(data: &'a [u8], mut f: impl FnMut(u16, &'a [u8]) -> Result<()>) -> Result<()> {
    let mut pos = 0;
    let mut start = 0;
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
                f(op, &data[start..pos])?;
                start = pos;
            }
            _ => return Err(invalid("invalid CFF DICT token")),
        }
    }
    if start != data.len() {
        return Err(invalid("unterminated CFF DICT"));
    }
    Ok(())
}
/// Replace operators in place; absent ones are appended in ascending operator
/// order. `replacements` must be sorted by operator.
fn rewrite(out: &mut impl Out, data: &[u8], replacements: &[(u16, &[usize])]) -> Result<()> {
    let mut used = [false; 8];
    for_parts(data, |op, raw| {
        match replacements.iter().position(|(o, _)| *o == op) {
            Some(i) if !used[i] => {
                used[i] = true;
                operator(out, op, replacements[i].1)
            }
            _ => out.put(raw),
        }
    })?;
    for (i, (op, values)) in replacements.iter().enumerate() {
        if !used[i] {
            operator(out, *op, values)?;
        }
    }
    Ok(())
}
fn rewritten_len(data: &[u8], replacements: &[(u16, &[usize])]) -> Result<usize> {
    let mut count = Count(0);
    rewrite(&mut count, data, replacements)?;
    Ok(count.0)
}
/// Canonical CFF1 INDEX with minimal offset size, identical to `write::index`.
fn index<O: Out>(
    out: &mut O,
    count: usize,
    length: impl Fn(usize) -> Result<usize>,
    mut item: impl FnMut(&mut O, usize) -> Result<()>,
    l: &Limits,
) -> Result<()> {
    to16(count)?;
    if count == 0 {
        return out.put(&[0, 0]);
    }
    let mut total = 1usize;
    for i in 0..count {
        total = total
            .checked_add(length(i)?)
            .ok_or_else(|| invalid("CFF INDEX overflow"))?;
    }
    let offsize = if total <= 0xff {
        1
    } else if total <= 0xffff {
        2
    } else if total <= 0xffffff {
        3
    } else {
        4
    };
    let len = 3usize
        .checked_add((count + 1) * offsize)
        .and_then(|n| n.checked_add(total - 1))
        .ok_or_else(|| invalid("CFF INDEX size overflow"))?;
    l.sfnt.table(len)?;
    out.put(&(count as u16).to_be_bytes())?;
    out.put(&[offsize as u8])?;
    let mut pos = 1usize;
    for i in 0..=count {
        let p = u32::try_from(pos).map_err(|_| invalid("CFF INDEX offset overflow"))?;
        out.put(&p.to_be_bytes()[4 - offsize..])?;
        if i < count {
            pos += length(i)?;
        }
    }
    for i in 0..count {
        item(out, i)?;
    }
    Ok(())
}
fn copy_index(out: &mut impl Out, source: &Index<'_>, l: &Limits) -> Result<()> {
    let get = |i: usize| source.get(i).ok_or_else(|| invalid("invalid CFF INDEX"));
    index(
        out,
        source.count() as usize,
        |i| get(i).map(<[u8]>::len),
        |out, i| out.put(get(i)?),
        l,
    )
}
/// A Private DICT and its raw local Subrs INDEX, relocated directly after it.
#[derive(Clone, Copy)]
struct Private<'a> {
    dict: &'a [u8],
    subrs: Option<&'a [u8]>,
    /// Length of the (possibly rewritten) DICT, excluding Subrs.
    dict_len: usize,
}
impl Private<'_> {
    fn len(&self) -> usize {
        self.dict_len + self.subrs.map_or(0, <[u8]>::len)
    }
    fn write(&self, out: &mut impl Out) -> Result<()> {
        match self.subrs {
            Some(subrs) => {
                rewrite(out, self.dict, &[(SUBRS, &[self.dict_len])])?;
                out.put(subrs)
            }
            None => out.put(self.dict),
        }
    }
}
fn private_info<'a>(
    data: &'a [u8],
    range: std::ops::Range<usize>,
    l: &Limits,
    total: &mut usize,
    budget: &Budget,
) -> Result<Private<'a>> {
    let original = data
        .get(range.clone())
        .ok_or_else(|| invalid("CFF private range outside table"))?;
    let e = dictionary(original, l, total, budget)?;
    let sub = e.iter().find_map(|e| {
        if let Entry::SubrsOffset(n) = e {
            Some(*n)
        } else {
            None
        }
    });
    drop(e);
    if let Some(off) = sub {
        let at = range
            .start
            .checked_add(off)
            .ok_or_else(|| invalid("CFF Subrs offset overflow"))?;
        let (_, end) = index_at(data, at)?;
        Ok(Private {
            dict: original,
            subrs: Some(bytes(data, at, end - at)?),
            dict_len: rewritten_len(original, &[(SUBRS, &[0])])?,
        })
    } else {
        Ok(Private {
            dict: original,
            subrs: None,
            dict_len: original.len(),
        })
    }
}
/// One retained Font DICT and its relocated Private block.
#[derive(Clone, Copy)]
struct Fd<'a> {
    raw: &'a [u8],
    /// Rewritten Font DICT length.
    len: usize,
    /// Private block position relative to the first Private block.
    relative: usize,
    private: Private<'a>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Offsets {
    charset: usize,
    charstrings: usize,
    select: usize,
    array: usize,
    privates: usize,
}
struct Plan<'a, 'c> {
    cff: &'c Cff<'a>,
    glyphs: &'c [u16],
    names: Index<'a>,
    strings: Index<'a>,
    global: Index<'a>,
    charstrings: Index<'a>,
    top: &'a [u8],
    top_len: usize,
    /// CID Font DICTs (all retained, preserving FDSelect identity).
    fds: Option<&'c [Fd<'a>]>,
    /// Non-CID Private block.
    private: Option<Private<'a>>,
    /// Unused CID Top DICT Private, still relocated so the table stays valid.
    top_private: Option<Private<'a>>,
}
impl Plan<'_, '_> {
    fn fd_private_total(&self) -> usize {
        self.fds
            .and_then(|fds| fds.last())
            .map_or(0, |fd| fd.relative + fd.private.len())
    }
    fn top_dict(&self, out: &mut impl Out, o: &Offsets) -> Result<()> {
        let charset = [o.charset];
        let charstrings = [o.charstrings];
        let array = [o.array];
        let select = [o.select];
        let top_private = self
            .top_private
            .map(|p| [p.dict_len, o.privates + self.fd_private_total()]);
        let private = self.private.map(|p| [p.dict_len, o.array]);
        let mut changes: [(u16, &[usize]); 5] = [(0, &[]); 5];
        let mut n = 0;
        let mut add = |op, values| {
            changes[n] = (op, values);
            n += 1;
        };
        add(CHARSET, &charset[..]);
        add(CHARSTRINGS, &charstrings[..]);
        if let Some(values) = &top_private {
            add(PRIVATE, &values[..]);
        }
        if let Some(values) = &private {
            add(PRIVATE, &values[..]);
        }
        if self.fds.is_some() {
            add(FDARRAY, &array[..]);
            add(FDSELECT, &select[..]);
        }
        rewrite(out, self.top, &changes[..n])
    }
    fn charstring(&self, i: usize) -> Result<&[u8]> {
        self.charstrings
            .get(usize::from(self.glyphs[i]))
            .ok_or_else(|| invalid("missing CFF charstring"))
    }
    /// Write the complete table, recording where each located section begins.
    fn write<O: Out>(
        &self,
        out: &mut O,
        o: &Offsets,
        l: &Limits,
        sections: &mut Offsets,
    ) -> Result<()> {
        out.put(&[1, 0, 4, 4])?;
        copy_index(out, &self.names, l)?;
        index(
            out,
            1,
            |_| Ok(self.top_len),
            |out, _| self.top_dict(out, o),
            l,
        )?;
        copy_index(out, &self.strings, l)?;
        copy_index(out, &self.global, l)?;
        sections.charset = out.len();
        out.put(&[0])?;
        for &gid in &self.glyphs[1..] {
            let id = self
                .cff
                .identifier(gid)
                .ok_or_else(|| invalid("missing CFF charset identifier"))?;
            out.put(&id.to_be_bytes())?;
        }
        sections.charstrings = out.len();
        index(
            out,
            self.glyphs.len(),
            |i| self.charstring(i).map(<[u8]>::len),
            |out, i| out.put(self.charstring(i)?),
            l,
        )?;
        sections.select = out.len();
        if self.fds.is_some() {
            out.put(&[0])?;
            for &gid in self.glyphs {
                out.put(&[self.cff.subfont_selector(gid)?])?;
            }
        }
        sections.array = out.len();
        if let Some(fds) = self.fds {
            index(
                out,
                fds.len(),
                |i| Ok(fds[i].len),
                |out, i| {
                    let fd = &fds[i];
                    let values = [fd.private.dict_len, o.privates + fd.relative];
                    rewrite(out, fd.raw, &[(PRIVATE, &values)])
                },
                l,
            )?;
        }
        sections.privates = out.len();
        let blocks = self
            .fds
            .into_iter()
            .flatten()
            .map(|fd| fd.private)
            .chain(self.private)
            .chain(self.top_private);
        for block in blocks {
            l.sfnt.table(out.len().saturating_add(block.len()))?;
            block.write(out)?;
        }
        l.sfnt.table(out.len())
    }
}
impl Cff<'_> {
    /// Compact selected glyphs, retaining Type 2 bytecode, hints and all subroutines.
    /// `glyphs` is new glyph order and must begin with original glyph zero.
    /// Legacy nondefault CFF encodings are rejected instead of silently changed.
    pub fn subset(&self, glyphs: &[u16], l: &Limits) -> Result<Vec<u8>> {
        let budget = super::read::root(l);
        let (out, _lease) = self.subset_with_budget(glyphs, l, &budget)?.into_parts();
        Ok(out)
    }
    /// Subset on a shared ledger. The parsed model is the caller's; DICT and
    /// Font DICT bookkeeping are charged while live, and the output is admitted
    /// at its exact final size before it is allocated.
    pub(crate) fn subset_with_budget(
        &self,
        glyphs: &[u16],
        l: &Limits,
        budget: &Budget,
    ) -> Result<Owned<Vec<u8>>> {
        l.sfnt.glyphs(glyphs.len())?;
        let mut seen = IdSet::new();
        if glyphs.first() != Some(&0)
            || glyphs.iter().any(|&g| usize::from(g) >= self.len())
            || !glyphs.iter().all(|&g| seen.insert(g))
        {
            return Err(invalid("invalid CFF subset glyph order"));
        }
        let data = self.data();
        let (names, end) = index_at(data, usize::from(data[2]))?;
        let (top, end) = index_at(data, end)?;
        let (strings, end) = index_at(data, end)?;
        let (global, _) = index_at(data, end)?;
        let original = top.get(0).ok_or_else(|| invalid("missing CFF Top DICT"))?;
        let mut dict_work = 0;
        let e = dictionary(original, l, &mut dict_work, budget)?;
        let mut cs = 0;
        let mut fdarray = None;
        let mut private = None;
        for item in e.iter() {
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
        drop(e);
        let (charstrings, _) = index_at(data, cs)?;
        // Validate every retained program under aggregate execution limits, before
        // copying raw bytes. Unused local/global programs remain inert and intact.
        self.validate_selected(glyphs, l, budget)?;
        // Some valid CID fonts also retain an unused Top DICT Private entry.
        // Its bytes still need relocation: leaving an old offset can invalidate
        // the whole CFF table even though glyph execution uses only FD privates.
        let top_private = if fdarray.is_some() {
            private
                .clone()
                .map(|range| private_info(data, range, l, &mut dict_work, budget))
                .transpose()?
        } else {
            None
        };
        let mut private_bytes = top_private.as_ref().map_or(0, Private::len);
        let mut fd_lease = budget.reserve(0)?;
        let mut fds = Vec::new();
        let single = if let Some(off) = fdarray {
            let (index, _) = index_at(data, off)?;
            fds = budget.vec_into(index.count() as usize, &mut fd_lease)?;
            // Retaining every FD avoids changing FDSelect identity and preserves
            // per-FD matrices/metadata even when some become unused.
            let mut relative = 0usize;
            for i in 0..index.count() {
                let raw = index
                    .get(i as usize)
                    .ok_or_else(|| invalid("missing CFF FD"))?;
                let e = dictionary(raw, l, &mut dict_work, budget)?;
                let range = e
                    .iter()
                    .find_map(|e| {
                        if let Entry::PrivateDictRange(r) = e {
                            Some(r.clone())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| invalid("missing CFF FD private dict"))?;
                drop(e);
                let p = private_info(data, range, l, &mut dict_work, budget)?;
                add_budget(
                    &mut private_bytes,
                    p.len(),
                    l.sfnt.resources.max_table_bytes,
                    "CFF retained private bytes",
                )?;
                let len = rewritten_len(raw, &[(PRIVATE, &[p.dict_len, 0])])?;
                fds.push(Fd {
                    raw,
                    len,
                    relative,
                    private: p,
                });
                relative = relative
                    .checked_add(p.len())
                    .ok_or_else(|| invalid("CFF size overflow"))?;
            }
            for &gid in glyphs {
                self.subfont_selector(gid)?;
            }
            None
        } else {
            Some(private_info(
                data,
                private.ok_or_else(|| invalid("missing CFF private dict"))?,
                l,
                &mut dict_work,
                budget,
            )?)
        };
        let mut plan = Plan {
            cff: self,
            glyphs,
            names,
            strings,
            global,
            charstrings,
            top: original,
            top_len: 0,
            fds: fdarray.map(|_| fds.as_slice()),
            private: single,
            top_private,
        };
        plan.top_len = {
            let mut count = Count(0);
            plan.top_dict(&mut count, &Offsets::default())?;
            count.0
        };
        // Layout pass: exact size and section positions, with every table
        // check applied before any output byte is allocated.
        let mut sections = Offsets::default();
        let mut count = Count(0);
        plan.write(&mut count, &Offsets::default(), l, &mut sections)?;
        let mut out = Buffer::with_capacity(budget, count.0)?;
        let mut written = Offsets::default();
        plan.write(&mut out, &sections, l, &mut written)?;
        if out.len() != count.0 || written != sections {
            return Err(invalid("CFF subset layout mismatch"));
        }
        drop(fds);
        drop(fd_lease);
        Ok(out.into_owned())
    }
}
