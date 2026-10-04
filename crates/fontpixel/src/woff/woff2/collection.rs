//! WOFF2 collection directory, shared table records, and paired transforms.
use super::*;
use crate::sfnt::collection::{Collection, Limits as CollectionLimits};
fn limits(l: &Limits) -> CollectionLimits {
    CollectionLimits {
        sfnt: l.sfnt.clone(),
        max_faces: l.max_batch_fonts,
        max_table_references: 65536,
        max_expanded_bytes: l.max_batch_bytes,
    }
}
fn read_records(cursor: &mut Cursor<'_>, n: usize, l: &Limits) -> Result<(Vec<Record>, usize)> {
    let mut records = Vec::with_capacity(n);
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
        });
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
pub(super) fn decode(data: &[u8], l: &Limits) -> Result<Decoded> {
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
    let (records, size) = read_records(&mut cursor, n, l)?;
    let version = u32::from_be_bytes(cursor.take(4)?.try_into().map_err(invalid)?);
    if ![0x10000, 0x20000].contains(&version) {
        return Err(invalid("unsupported collection version"));
    }
    let count = cursor.ushort255()? as usize;
    bounded(count, l.max_batch_fonts, "WOFF2 collection faces")?;
    if count == 0 {
        return Err(invalid("empty collection"));
    }
    let mut faces = Vec::with_capacity(count);
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
        let mut indices = Vec::with_capacity(nt);
        let mut tags = std::collections::BTreeSet::new();
        let (mut g, mut a) = (None, None);
        for _ in 0..nt {
            let i = cursor.ushort255()? as usize;
            let r = records
                .get(i)
                .ok_or_else(|| invalid("collection table index out of range"))?;
            if !tags.insert(r.tag) {
                return Err(invalid("duplicate collection face tag"));
            }
            if r.tag == *b"glyf" {
                g = Some(i);
            }
            if r.tag == *b"loca" {
                a = Some(i);
            }
            indices.push(i);
        }
        if g.map(|i| i + 1) != a {
            return Err(invalid("mismatched collection glyf/loca indices"));
        }
        faces.push((f, indices));
    }
    let raw = decompress(
        cursor.take(u32_at(data, 20)? as usize)?,
        size,
        l.sfnt.resources.max_total_bytes,
    )?;
    let (auxiliary, mut report) = read_aux(data, 24, cursor.pos, l, |data, expected| {
        decompress(data, expected, l.sfnt.resources.max_total_bytes)
    })?;
    let mut offsets = Vec::with_capacity(n);
    let mut offset = 0;
    for r in &records {
        offsets.push(offset);
        offset += r.length;
    }
    let mut fonts = Vec::with_capacity(count);
    let mut total = 0usize;
    for (f, indices) in faces {
        let mut tables = BTreeMap::new();
        let mut original = BTreeMap::new();
        let mut transformed = std::collections::BTreeSet::new();
        let (mut expanded, mut estimate) = (0usize, 12 + indices.len() * 16);
        for i in indices {
            let r = &records[i];
            estimate = estimate
                .checked_add(align(r.original)?)
                .ok_or_else(|| invalid("collection expansion overflow"))?;
            bounded(
                estimate,
                l.max_batch_bytes.min(l.sfnt.resources.max_total_bytes),
                "WOFF2 face bytes",
            )?;
            tables.insert(r.tag, slice(&raw, offsets[i], r.length)?.to_vec());
            original.insert(r.tag, r.original);
            if r.transformed {
                transformed.insert(r.tag);
            }
        }
        if transformed.contains(b"glyf") {
            let ng = u16_at(
                tables.get(b"maxp").ok_or_else(|| invalid("missing maxp"))?,
                4,
            )?;
            let e = transform::decode(
                tables.get(b"glyf").ok_or_else(|| invalid("missing glyf"))?,
                ng,
                l,
            )?;
            if original.get(b"loca") != Some(&e.loca.len()) {
                return Err(invalid("wrong collection original loca length"));
            }
            let head = tables
                .get_mut(b"head")
                .ok_or_else(|| invalid("missing head"))?;
            slice(head, 0, 54)?;
            head[50..52].copy_from_slice(&e.index_format.to_be_bytes());
            tables.insert(*b"glyf", e.glyf);
            tables.insert(*b"loca", e.loca);
        }
        if transformed.contains(b"hmtx") {
            let xmins = transform::xmins(&tables, l)?;
            let h = u16_at(
                tables.get(b"hhea").ok_or_else(|| invalid("missing hhea"))?,
                34,
            )? as usize;
            let hmtx = transform::decode_hmtx(
                tables.get(b"hmtx").ok_or_else(|| invalid("missing hmtx"))?,
                &xmins,
                h,
            )?;
            if original.get(b"hmtx") != Some(&hmtx.len()) {
                return Err(invalid("wrong collection hmtx length"));
            }
            tables.insert(*b"hmtx", hmtx);
        }
        if tables.remove(b"DSIG").is_some() {
            loss(
                &mut report,
                "woff2.dsig_removed",
                "Collection reconstruction invalidates face digital signature; DSIG removed",
            );
        }
        let mut builder = sfnt::SfntBuilder::new(f);
        for (tag, t) in tables {
            expanded = expanded
                .checked_add(t.len())
                .ok_or_else(|| invalid("collection expanded overflow"))?;
            bounded(
                expanded,
                l.sfnt.resources.max_total_bytes,
                "WOFF2 face expanded bytes",
            )?;
            builder.insert(tag, t)?;
        }
        let bytes = builder.build(&l.sfnt)?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| invalid("collection total overflow"))?;
        bounded(
            total,
            l.max_batch_bytes.min(l.sfnt.resources.max_total_bytes),
            "WOFF2 total face bytes",
        )?;
        fonts.push(bytes);
    }
    let refs = fonts.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let (sfnt, r) = crate::sfnt::collection::encode(&refs, version, &limits(l))?;
    report.extend(r);
    bounded(
        sfnt.len()
            + auxiliary.metadata.as_ref().map_or(0, Vec::len)
            + auxiliary.private_data.as_ref().map_or(0, Vec::len),
        l.sfnt.resources.max_total_bytes,
        "WOFF2 decoded collection bytes",
    )?;
    Ok(Decoded {
        sfnt,
        auxiliary,
        report,
    })
}
struct Payload {
    record: Record,
    bytes: Vec<u8>,
}
pub(super) fn encode(data: &[u8], o: &Options, l: &Limits) -> Result<Encoded> {
    check_options(o, l)?;
    let c = Collection::parse(data, &limits(l))?;
    let mut report = Report::default();
    if c.signature.is_some() {
        loss(
            &mut report,
            "woff2.dsig_removed",
            "WOFF2 invalidates TTC signature; removed",
        );
    }
    // WOFF2 requires sharing to preserve complete associated glyf/loca pairs.
    let (mut g_to_l, mut l_to_g) = (BTreeMap::new(), BTreeMap::new());
    for f in &c.faces {
        let g = f.tables.iter().find(|t| t.tag == *b"glyf");
        let a = f.tables.iter().find(|t| t.tag == *b"loca");
        match (g, a) {
            (Some(g), Some(a)) => {
                if g_to_l
                    .insert(g.offset, a.offset)
                    .is_some_and(|v| v != a.offset)
                    || l_to_g
                        .insert(a.offset, g.offset)
                        .is_some_and(|v| v != g.offset)
                {
                    return Err(invalid(
                        "WOFF2 collections require both glyf and loca to share as one pair",
                    ));
                }
            }
            (None, None) => {}
            _ => return Err(invalid("missing collection glyf/loca pair")),
        }
    }
    let mut payloads = Vec::<Payload>::new();
    let mut shared = BTreeMap::<([u8; 4], u32), usize>::new();
    let mut faces = Vec::new();
    let (mut raw_size, mut total_sfnt) = (
        0usize,
        12 + c.faces.len() * 4 + if c.version == 0x20000 { 12 } else { 0 },
    );
    for face in &c.faces {
        let mut tables = face
            .tables
            .iter()
            .map(|r| (r.tag, r.data.to_vec()))
            .collect::<BTreeMap<_, _>>();
        if tables.remove(b"DSIG").is_some() {
            loss(
                &mut report,
                "woff2.dsig_removed",
                "WOFF2 invalidates face digital signature; DSIG removed",
            );
        }
        let have_glyf = tables.contains_key(b"glyf");
        let mut transformed = BTreeMap::new();
        if have_glyf && !o.transform_glyf {
            return Err(invalid(
                "WOFF2 collection glyf transforms are required by specification",
            ));
        }
        if have_glyf {
            transform::normalize(&mut tables, l)?;
            transformed.insert(*b"glyf", transform::encode(&tables, l)?);
            transformed.insert(*b"loca", Vec::new());
        }
        // hmtx null transforms avoid sharing a context-dependent transform across faces.
        let head = tables
            .get_mut(b"head")
            .ok_or_else(|| invalid("missing head"))?;
        let flags = u16_at(head, 16)? | 0x0800;
        head[16..18].copy_from_slice(&flags.to_be_bytes());
        slice(head, 0, 12)?;
        head[8..12].fill(0);
        let mut order = tables
            .keys()
            .copied()
            .filter(|t| *t != *b"loca")
            .collect::<Vec<_>>();
        if have_glyf {
            let i = order
                .iter()
                .position(|t| *t == *b"glyf")
                .ok_or_else(|| invalid("glyf"))?;
            order.insert(i + 1, *b"loca");
        }
        let mut indices = Vec::new();
        total_sfnt = total_sfnt
            .checked_add(12 + order.len() * 16)
            .ok_or_else(|| invalid("collection size overflow"))?;
        for tag in order {
            let original = tables[&tag].len();
            let transformed_flag = transformed.contains_key(&tag);
            let body = transformed
                .remove(&tag)
                .unwrap_or_else(|| tables.remove(&tag).unwrap_or_default());
            let source = face
                .tables
                .iter()
                .find(|r| r.tag == tag)
                .ok_or_else(|| invalid("missing source table"))?;
            let key = (tag, source.offset);
            let id = if let Some(&id) = shared.get(&key) {
                let p = &payloads[id];
                if p.record.original != original || p.bytes != body {
                    return Err(invalid("shared table has inconsistent transformed context"));
                }
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
                payloads.push(Payload {
                    record: Record {
                        tag,
                        original,
                        length: body.len(),
                        transformed: transformed_flag,
                    },
                    bytes: body,
                });
                shared.insert(key, id);
                id
            };
            indices.push(id);
        }
        faces.push((face.flavor, indices));
    }
    let mut out = vec![0; 48];
    out[..4].copy_from_slice(b"wOF2");
    out[4..8].copy_from_slice(b"ttcf");
    out[12..14].copy_from_slice(&(payloads.len() as u16).to_be_bytes());
    set32(&mut out, 16, total_sfnt)?;
    let mut raw = Vec::with_capacity(raw_size);
    for p in &payloads {
        let r = &p.record;
        let version = if [*b"glyf", *b"loca"].contains(&r.tag) {
            if r.transformed { 0 } else { 3 }
        } else if r.transformed {
            1
        } else {
            0
        };
        let index = TAGS.iter().position(|t| *t == r.tag).unwrap_or(63);
        out.push(index as u8 | version << 6);
        if index == 63 {
            out.extend(r.tag);
        }
        base128(&mut out, as32(r.original)?);
        if r.transformed {
            base128(&mut out, as32(r.length)?);
        }
        raw.extend(&p.bytes);
    }
    p32(&mut out, c.version);
    ushort255(&mut out, u16::try_from(faces.len()).map_err(invalid)?);
    for (f, indices) in faces {
        ushort255(&mut out, indices.len() as u16);
        p32(&mut out, f.version());
        for i in indices {
            ushort255(&mut out, i as u16);
        }
    }
    let compressed = compress(&raw, o, l)?;
    set32(&mut out, 20, compressed.len())?;
    bounded(
        out.len().saturating_add(compressed.len()),
        l.sfnt.resources.max_total_bytes,
        "WOFF2 collection output bytes",
    )?;
    out.extend(compressed);
    out.resize(align(out.len())?, 0);
    write_aux(&mut out, 24, &o.auxiliary, l, |d| compress(d, o, l))?;
    let len = out.len();
    set32(&mut out, 8, len)?;
    Ok(Encoded { bytes: out, report })
}
