//! WOFF2 collection directory, shared table records, and paired transforms.
//!
//! The collection paths share the caller operation's working ledger. A table
//! referenced by several faces moves (with its lease) into one payload and is
//! retained once; per-face working copies and transforms are charged while
//! they live. `crate::sfnt::collection` keeps its own internal ledgers, so its
//! complete heap use is reserved here from a source-derived bound before each
//! call (see [`parse_bound`] and [`assemble_bound`]).
use super::*;
use crate::sfnt::collection::{Collection, Face, Limits as CollectionLimits};
use std::mem::size_of;
fn limits(l: &Limits) -> CollectionLimits {
    CollectionLimits {
        sfnt: l.sfnt.clone(),
        max_faces: l.max_batch_fonts,
        max_table_references: 65536,
        max_expanded_bytes: l.max_batch_bytes,
    }
}
/// Live bytes of a `Vec<T>` grown by repeated `push` to `len` elements,
/// including the superseded buffer during its last reallocation. std doubles
/// capacity with a small minimum, so capacity <= max(2 * len, 8) and the old
/// buffer is at most half of it.
fn pushed_vec<T>(len: usize) -> Option<usize> {
    len.max(8)
        .checked_mul(3)?
        .checked_mul(size_of::<T>().max(1))
}
fn checked_sum(parts: &[Option<usize>]) -> Option<usize> {
    parts
        .iter()
        .try_fold(0usize, |total, part| total.checked_add((*part)?))
}
/// Conservative heap bound of `sfnt::collection::Collection::parse` for this
/// input, from that function's allocation sites: `spans` grows by one entry
/// for the header, the optional signature, each face directory and each
/// non-empty table; `faces` is `Vec::with_capacity(count)`; each face's records
/// are `Vec::with_capacity(n)` and its transient tag `BTreeSet` holds `n` tags.
/// Counts are read from the header exactly as the parser reads them, and the
/// parser's own face/table/reference caps are applied in the same order, so a
/// header that the parser rejects cannot allocate beyond this bound.
fn parse_bound(data: &[u8], l: &CollectionLimits, budget: &Budget) -> Result<usize> {
    let overflow = || invalid("TTC bound overflow");
    let count = (u32_at(data, 8).unwrap_or(0) as usize).min(l.max_faces);
    let table_cap = l.sfnt.max_tables.min(4095);
    let (mut references, mut largest) = (0usize, 0usize);
    for i in 0..count {
        let Ok(offset) = u32_at(data, 12 + i * 4) else {
            break;
        };
        let Ok(n) = u16_at(data, offset as usize + 4) else {
            break;
        };
        let n = (n as usize).min(table_cap);
        references = references.saturating_add(n);
        largest = largest.max(n);
    }
    let references = references.min(l.max_table_references);
    let spans = 2usize
        .checked_add(count)
        .and_then(|n| n.checked_add(references))
        .ok_or_else(overflow)?;
    checked_sum(&[
        pushed_vec::<(usize, usize, u8)>(spans),
        count.checked_mul(size_of::<Face<'_>>()),
        references.checked_mul(size_of::<sfnt::TableRecord<'_>>()),
        budget.tree_bytes::<[u8; 4], ()>(largest).ok(),
    ])
    .ok_or_else(overflow)
}
/// Conservative heap bound of `sfnt::collection::encode(fonts, ..)` (its own
/// ledgers are separate from this operation's). Sites, in that function's
/// order: the `parsed` vector of `Sfnt` (collected from a fallible iterator,
/// so it grows by push) plus each `Sfnt::parse` directory and its transient tag
/// tree/span list; one owned copy of every table (`t.data.to_vec()`), plus one
/// transient duplicate that is dropped after comparison; the `payloads`,
/// per-face `rows` and `records` vectors; the `shared` tree and its per-key
/// id vectors; `offsets`; and the zero-filled output of the final size.
fn assemble_bound(fonts: &[Owned<Vec<u8>>], budget: &Budget) -> Result<usize> {
    let overflow = || invalid("TTC bound overflow");
    let faces = fonts.len();
    let mut references = 0usize;
    let mut bytes = 0usize;
    let mut largest_table = 0usize;
    let mut largest_face = 0usize;
    for font in fonts {
        let n = u16_at(font, 4)? as usize;
        references = references.checked_add(n).ok_or_else(overflow)?;
        bytes = bytes.checked_add(font.len()).ok_or_else(overflow)?;
        largest_face = largest_face.max(n);
        largest_table = largest_table.max(font.len());
    }
    let parse_transient = checked_sum(&[
        budget.tree_bytes::<[u8; 4], ()>(largest_face).ok(),
        largest_face.checked_mul(size_of::<(usize, usize)>()),
    ]);
    let output = checked_sum(&[
        Some(24),
        faces.checked_mul(16),
        references.checked_mul(16 + 3),
        Some(bytes),
    ]);
    checked_sum(&[
        pushed_vec::<sfnt::Sfnt<'_>>(faces),
        references.checked_mul(size_of::<sfnt::TableRecord<'_>>()),
        parse_transient,
        Some(bytes),
        Some(largest_table),
        pushed_vec::<([u8; 4], Vec<u8>)>(references),
        pushed_vec::<([u8; 4], usize)>(references),
        faces.checked_mul(24 * 3),
        budget
            .tree_bytes::<([u8; 4], usize, u64), Vec<usize>>(references)
            .ok(),
        pushed_vec::<usize>(references).and_then(|v| v.checked_add(references.checked_mul(96)?)),
        references.checked_mul(size_of::<usize>()),
        output,
    ])
    .ok_or_else(overflow)
}
fn read_records(
    cursor: &mut Cursor<'_>,
    n: usize,
    l: &Limits,
    budget: &Budget,
) -> Result<(Vector<Record>, usize)> {
    let mut records = Vector::with_capacity(budget, n)?;
    let mut size = 0usize;
    for _ in 0..n {
        let flags = cursor.byte()?;
        let tag = if flags & 63 == 63 {
            cursor.take(4)?.try_into().map_err(invalid)?
        } else {
            TAGS[(flags & 63) as usize]
        };
        if !valid_tag(tag) {
            return Err(invalid("invalid WOFF2 collection tag"));
        }
        let transformed = match (&tag, flags >> 6) {
            (b"glyf" | b"loca", 0) => true,
            (b"glyf" | b"loca", 3) => false,
            (b"hmtx", 1) => true,
            (_, 0) => false,
            _ => return Err(invalid("unknown collection transform")),
        };
        let original = cursor.base128()? as usize;
        let length = if transformed {
            cursor.base128()? as usize
        } else {
            original
        };
        if tag == *b"loca" && transformed && length != 0 {
            return Err(invalid("nonempty transformed loca"));
        }
        bounded(
            original,
            l.sfnt.resources.max_table_bytes,
            "WOFF2 collection original table bytes",
        )?;
        bounded(
            length,
            l.sfnt.resources.max_table_bytes,
            "WOFF2 collection table bytes",
        )?;
        size = size
            .checked_add(length)
            .ok_or_else(|| invalid("collection stream overflow"))?;
        bounded(
            size,
            l.sfnt
                .resources
                .max_decompressed_bytes
                .min(l.sfnt.resources.max_total_bytes),
            "WOFF2 collection stream bytes",
        )?;
        records.push(Record {
            tag,
            original,
            length,
            transformed,
        })?;
    }
    // In collections every glyf must be followed immediately by its associated loca.
    for (i, r) in records.iter().enumerate() {
        if r.tag == *b"glyf"
            && records
                .get(i + 1)
                .is_none_or(|a| a.tag != *b"loca" || a.transformed != r.transformed)
            || r.tag == *b"loca" && (i == 0 || records[i - 1].tag != *b"glyf")
        {
            return Err(invalid("unpaired WOFF2 collection glyf/loca"));
        }
    }
    Ok((records, size))
}
/// A tag set whose tree nodes are admitted before each insertion.
struct TagSet {
    set: std::collections::BTreeSet<[u8; 4]>,
    nodes: crate::working::Lease,
}
impl TagSet {
    fn new(budget: &Budget) -> Result<Self> {
        Ok(Self {
            set: std::collections::BTreeSet::new(),
            nodes: budget.reserve(0)?,
        })
    }
    fn insert(&mut self, budget: &Budget, tag: [u8; 4]) -> Result<bool> {
        if !self.set.contains(&tag) {
            self.nodes
                .grow_to(budget.tree_bytes::<[u8; 4], ()>(self.set.len() + 1)?)?;
        }
        Ok(self.set.insert(tag))
    }
}
pub(in crate::woff) fn decode(data: &[u8], l: &Limits, budget: &Budget) -> Result<Decoded> {
    bounded(
        data.len(),
        l.sfnt.resources.max_input_bytes,
        "WOFF2 collection input bytes",
    )?;
    if slice(data, 0, 48)?[..4] != *b"wOF2" || u32_at(data, 8)? as usize != data.len() {
        return Err(invalid("invalid WOFF2 collection header"));
    }
    let n = u16_at(data, 12)? as usize;
    bounded(n, 65535, "WOFF2 collection tables")?;
    if n == 0 {
        return Err(invalid("empty collection directory"));
    }
    let mut cursor = Cursor { data, pos: 48 };
    let (records, size) = read_records(&mut cursor, n, l, budget)?;
    let version = u32::from_be_bytes(cursor.take(4)?.try_into().map_err(invalid)?);
    if ![0x10000, 0x20000].contains(&version) {
        return Err(invalid("unsupported collection version"));
    }
    let count = cursor.ushort255()? as usize;
    bounded(count, l.max_batch_fonts, "WOFF2 collection faces")?;
    if count == 0 {
        return Err(invalid("empty collection"));
    }
    let mut faces = Vector::with_capacity(budget, count)?;
    let mut reference_count = 0usize;
    for _ in 0..count {
        let nt = cursor.ushort255()? as usize;
        bounded(nt, l.sfnt.max_tables.min(4095), "WOFF2 face tables")?;
        if nt == 0 {
            return Err(invalid("empty WOFF2 face"));
        }
        reference_count = reference_count
            .checked_add(nt)
            .ok_or_else(|| invalid("collection reference overflow"))?;
        bounded(reference_count, 65536, "WOFF2 table references")?;
        let f = flavor(u32::from_be_bytes(
            cursor.take(4)?.try_into().map_err(invalid)?,
        ))?;
        let mut indices = Vector::with_capacity(budget, nt)?;
        let mut tags = TagSet::new(budget)?;
        let (mut g, mut a) = (None, None);
        for _ in 0..nt {
            let i = cursor.ushort255()? as usize;
            let r = records
                .get(i)
                .ok_or_else(|| invalid("collection table index out of range"))?;
            if !tags.insert(budget, r.tag)? {
                return Err(invalid("duplicate collection face tag"));
            }
            if r.tag == *b"glyf" {
                g = Some(i);
            }
            if r.tag == *b"loca" {
                a = Some(i);
            }
            indices.push(i)?;
        }
        if g.map(|i| i + 1) != a {
            return Err(invalid("mismatched collection glyf/loca indices"));
        }
        faces.push((f, indices))?;
    }
    let raw = decompress(cursor.take(u32_at(data, 20)? as usize)?, size, budget)?;
    let (auxiliary, mut report) = read_aux(data, 24, cursor.pos, l, budget, |data, expected| {
        decompress(data, expected, budget)
    })?;
    let mut offsets = Vector::with_capacity(budget, n)?;
    let mut offset = 0;
    for r in records.iter() {
        offsets.push(offset)?;
        offset += r.length;
    }
    let mut fonts = Vector::with_capacity(budget, count)?;
    let mut total = 0usize;
    for (f, indices) in faces.iter() {
        let mut tables = Tables::new(budget)?;
        let mut original = Vector::with_capacity(budget, indices.len())?;
        let mut transformed = TagSet::new(budget)?;
        let (mut expanded, mut estimate) = (0usize, 12 + indices.len() * 16);
        for &i in indices.iter() {
            let r = &records[i];
            estimate = estimate
                .checked_add(align(r.original)?)
                .ok_or_else(|| invalid("collection expansion overflow"))?;
            bounded(
                estimate,
                l.max_batch_bytes.min(l.sfnt.resources.max_total_bytes),
                "WOFF2 face bytes",
            )?;
            tables.insert(
                budget,
                r.tag,
                copy(budget, slice(&raw, offsets[i], r.length)?)?,
            )?;
            original.push((r.tag, r.original))?;
            if r.transformed {
                transformed.insert(budget, r.tag)?;
            }
        }
        let original_len = |tag: &[u8; 4]| original.iter().find(|(t, _)| t == tag).map(|(_, n)| *n);
        if transformed.set.contains(b"glyf") {
            let ng = u16_at(
                tables.get(b"maxp").ok_or_else(|| invalid("missing maxp"))?,
                4,
            )?;
            let e = transform::decode(
                tables.get(b"glyf").ok_or_else(|| invalid("missing glyf"))?,
                ng,
                l,
                budget,
            )?;
            if original_len(b"loca") != Some(e.loca.len()) {
                return Err(invalid("wrong collection original loca length"));
            }
            let head = tables
                .get_mut(b"head")
                .ok_or_else(|| invalid("missing head"))?;
            slice(head, 0, 54)?;
            head[50..52].copy_from_slice(&e.index_format.to_be_bytes());
            tables.insert(budget, *b"glyf", e.glyf)?;
            tables.insert(budget, *b"loca", e.loca)?;
        }
        if transformed.set.contains(b"hmtx") {
            let xmins = transform::xmins(&tables, l, budget)?;
            let h = u16_at(
                tables.get(b"hhea").ok_or_else(|| invalid("missing hhea"))?,
                34,
            )? as usize;
            let hmtx = transform::decode_hmtx(
                tables.get(b"hmtx").ok_or_else(|| invalid("missing hmtx"))?,
                &xmins,
                h,
                budget,
            )?;
            drop(xmins);
            if original_len(b"hmtx") != Some(hmtx.len()) {
                return Err(invalid("wrong collection hmtx length"));
            }
            tables.insert(budget, *b"hmtx", hmtx)?;
        }
        drop(transformed);
        drop(original);
        if tables.remove(b"DSIG").is_some() {
            loss(
                &mut report,
                "woff2.dsig_removed",
                "Collection reconstruction invalidates face digital signature; DSIG removed",
            );
        }
        for t in tables.map.values() {
            expanded = expanded
                .checked_add(t.len())
                .ok_or_else(|| invalid("collection expanded overflow"))?;
            bounded(
                expanded,
                l.sfnt.resources.max_total_bytes,
                "WOFF2 face expanded bytes",
            )?;
        }
        let (builder, table_lease) = tables.into_builder(budget, *f)?;
        let bytes = builder.build_with_budget(&l.sfnt, budget)?;
        drop(builder);
        drop(table_lease);
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| invalid("collection total overflow"))?;
        bounded(
            total,
            l.max_batch_bytes.min(l.sfnt.resources.max_total_bytes),
            "WOFF2 total face bytes",
        )?;
        fonts.push(bytes)?;
    }
    drop(offsets);
    drop(raw);
    drop(faces);
    drop(records);
    // The TTC assembler keeps its own ledgers; reserve its complete heap here.
    let assembler = budget.reserve(assemble_bound(&fonts, budget)?)?;
    let mut refs = Vector::with_capacity(budget, fonts.len())?;
    for font in fonts.iter() {
        refs.push(font.as_slice())?;
    }
    let (sfnt, r) = crate::sfnt::collection::encode(&refs, version, &limits(l))?;
    report.extend(r);
    bounded(
        sfnt.len() + auxiliary.bytes(),
        l.sfnt.resources.max_total_bytes,
        "WOFF2 decoded collection bytes",
    )?;
    drop(refs);
    drop(fonts);
    let result = Decoded {
        sfnt,
        auxiliary: auxiliary.release(),
        report,
    };
    // The assembled collection now belongs to the caller.
    drop(assembler);
    Ok(result)
}
struct Payload {
    record: Record,
    bytes: Owned<Vec<u8>>,
}
pub(in crate::woff) fn encode(
    data: &[u8],
    o: &Options,
    l: &Limits,
    budget: &Budget,
) -> Result<Encoded> {
    check_options(o, l)?;
    let collection_limits = limits(l);
    // The parsed directory is retained for the whole call; reserve its bound
    // before the parser allocates it.
    let parsed = budget.reserve(parse_bound(data, &collection_limits, budget)?)?;
    let c = Collection::parse(data, &collection_limits)?;
    let mut report = Report::default();
    if c.signature.is_some() {
        loss(
            &mut report,
            "woff2.dsig_removed",
            "WOFF2 invalidates TTC signature; removed",
        );
    }
    // WOFF2 requires sharing to preserve complete associated glyf/loca pairs.
    {
        let mut pairs = Vector::with_capacity(budget, c.faces.len())?;
        for f in &c.faces {
            let g = f.tables.iter().find(|t| t.tag == *b"glyf");
            let a = f.tables.iter().find(|t| t.tag == *b"loca");
            match (g, a) {
                (Some(g), Some(a)) => {
                    if pairs.iter().any(|&(pg, pa)| {
                        (pg == g.offset && pa != a.offset) || (pa == a.offset && pg != g.offset)
                    }) {
                        return Err(invalid(
                            "WOFF2 collections require both glyf and loca to share as one pair",
                        ));
                    }
                    pairs.push((g.offset, a.offset))?;
                }
                (None, None) => {}
                _ => return Err(invalid("missing collection glyf/loca pair")),
            }
        }
    }
    let mut payloads = Vector::<Payload>::new(budget)?;
    let mut shared = std::collections::BTreeMap::<([u8; 4], u32), usize>::new();
    let mut shared_nodes = budget.reserve(0)?;
    let mut faces = Vector::with_capacity(budget, c.faces.len())?;
    let (mut raw_size, mut total_sfnt) = (
        0usize,
        12 + c.faces.len() * 4 + if c.version == 0x20000 { 12 } else { 0 },
    );
    for face in &c.faces {
        let mut tables = Tables::new(budget)?;
        for r in &face.tables {
            tables.insert(budget, r.tag, copy(budget, r.data)?)?;
        }
        if tables.remove(b"DSIG").is_some() {
            loss(
                &mut report,
                "woff2.dsig_removed",
                "WOFF2 invalidates face digital signature; DSIG removed",
            );
        }
        let have_glyf = tables.contains_key(b"glyf");
        let mut transformed = Tables::new(budget)?;
        if have_glyf && !o.transform_glyf {
            return Err(invalid(
                "WOFF2 collection glyf transforms are required by specification",
            ));
        }
        if have_glyf {
            transform::normalize(&mut tables, l, budget)?;
            let glyf = transform::encode(&tables, l, budget)?;
            transformed.insert(budget, *b"glyf", glyf)?;
            transformed.insert(budget, *b"loca", Buffer::new(budget)?.into_owned())?;
        }
        // hmtx null transforms avoid sharing a context-dependent transform across faces.
        let head = tables
            .get_mut(b"head")
            .ok_or_else(|| invalid("missing head"))?;
        let flags = u16_at(head, 16)? | 0x0800;
        head[16..18].copy_from_slice(&flags.to_be_bytes());
        slice(head, 0, 12)?;
        head[8..12].fill(0);
        let mut order = Vector::with_capacity(budget, tables.len())?;
        for &tag in tables.map.keys() {
            if tag == *b"loca" {
                continue;
            }
            order.push(tag)?;
            if have_glyf && tag == *b"glyf" {
                order.push(*b"loca")?;
            }
        }
        let mut indices = Vector::with_capacity(budget, order.len())?;
        total_sfnt = total_sfnt
            .checked_add(12 + order.len() * 16)
            .ok_or_else(|| invalid("collection size overflow"))?;
        for &tag in order.iter() {
            let original = tables
                .get(&tag)
                .ok_or_else(|| invalid("missing table"))?
                .len();
            let transformed_flag = transformed.contains_key(&tag);
            let body = match transformed.remove(&tag) {
                Some(body) => body,
                None => tables
                    .remove(&tag)
                    .ok_or_else(|| invalid("missing table"))?,
            };
            let source = face
                .tables
                .iter()
                .find(|r| r.tag == tag)
                .ok_or_else(|| invalid("missing source table"))?;
            let key = (tag, source.offset);
            let id = if let Some(&id) = shared.get(&key) {
                let p = &payloads[id];
                if p.record.original != original || p.bytes.as_slice() != body.as_slice() {
                    return Err(invalid("shared table has inconsistent transformed context"));
                }
                // A repeated reference keeps only the payload's single copy.
                id
            } else {
                raw_size = raw_size
                    .checked_add(body.len())
                    .ok_or_else(|| invalid("collection raw overflow"))?;
                bounded(
                    raw_size,
                    l.sfnt
                        .resources
                        .max_decompressed_bytes
                        .min(l.sfnt.resources.max_total_bytes),
                    "WOFF2 collection transform bytes",
                )?;
                total_sfnt = total_sfnt
                    .checked_add(align(original)?)
                    .ok_or_else(|| invalid("collection size overflow"))?;
                let id = payloads.len();
                bounded(id + 1, 65535, "WOFF2 collection table count")?;
                shared_nodes
                    .grow_to(budget.tree_bytes::<([u8; 4], u32), usize>(shared.len() + 1)?)?;
                payloads.push(Payload {
                    record: Record {
                        tag,
                        original,
                        length: body.len(),
                        transformed: transformed_flag,
                    },
                    bytes: body,
                })?;
                shared.insert(key, id);
                id
            };
            indices.push(id)?;
        }
        faces.push((face.flavor, indices))?;
    }
    drop(shared);
    drop(shared_nodes);
    let mut out = Buffer::with_capacity(budget, 48)?;
    out.resize(48, 0)?;
    out[..4].copy_from_slice(b"wOF2");
    out[4..8].copy_from_slice(b"ttcf");
    out[12..14].copy_from_slice(&(payloads.len() as u16).to_be_bytes());
    set32(&mut out, 16, total_sfnt)?;
    let mut raw = Buffer::with_capacity(budget, raw_size)?;
    for p in payloads.iter() {
        let r = &p.record;
        directory_entry(
            &mut out,
            r.tag,
            r.original,
            r.transformed.then_some(r.length),
        )?;
        put(&mut raw, &p.bytes)?;
    }
    drop(payloads);
    p32(&mut out, c.version)?;
    ushort255(&mut out, u16::try_from(faces.len()).map_err(invalid)?)?;
    for (f, indices) in faces.iter() {
        ushort255(&mut out, indices.len() as u16)?;
        p32(&mut out, f.version())?;
        for &i in indices.iter() {
            ushort255(&mut out, i as u16)?;
        }
    }
    drop(faces);
    let compressed = compress(&raw, o, l, budget)?;
    drop(raw);
    set32(&mut out, 20, compressed.len())?;
    bounded(
        out.len().saturating_add(compressed.len()),
        l.sfnt.resources.max_total_bytes,
        "WOFF2 collection output bytes",
    )?;
    put(&mut out, &compressed)?;
    drop(compressed);
    pad(&mut out)?;
    write_aux(&mut out, 24, &o.auxiliary, l, |d| compress(d, o, l, budget))?;
    let len = out.len();
    set32(&mut out, 8, len)?;
    drop(c);
    drop(parsed);
    Ok(Encoded {
        bytes: release(out.into_owned()),
        report,
    })
}
