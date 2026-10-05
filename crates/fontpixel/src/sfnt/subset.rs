//! Compact Unicode subsets with GSUB/composite closure and explicit table safety.
//!
//! All storage newly owned by a subset operation is admitted to one working
//! ledger before it is allocated and stays charged until it drops: directory,
//! selected cmap, GSUB model/closure/remap, glyf validation temporaries and
//! composite dependencies, keep/mapping trees, CFF model and output, metrics,
//! OS/2, post, embedded strikes, auxiliary tables, copied global tables,
//! builder nodes, the final sfnt and the report. Borrowed input bytes and the
//! caller's selection options are the caller's to account for.
use super::{
    glyf::Dependencies,
    gsub::{Gsub, LayoutLimits},
    metrics::{self, Axis},
    working::{Budget, Buffer, Lease, Owned, Vector as WorkingVector},
    *,
};
use crate::{
    Report, ReportEntry,
    report::{Impact, Level},
};
use std::collections::{BTreeMap, BTreeSet};
/// Options for subsetting a standalone sfnt.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Retained Unicode scalars; all other cmap entries are removed.
    pub codepoints: BTreeSet<u32>,
    /// Explicitly discard these otherwise unsupported tables, reporting loss.
    pub drop_tables: BTreeSet<[u8; 4]>,
    /// Shared container and layout budgets.
    pub limits: LayoutLimits,
}
impl Options {
    /// Select every distinct scalar in a Unicode text string.
    pub fn text(text: &str) -> Self {
        Self {
            codepoints: text.chars().map(u32::from).collect(),
            ..Default::default()
        }
    }
}
/// A complete subset and the stable old-to-new glyph correspondence.
#[derive(Clone, Debug)]
pub struct Subset {
    /// Standalone sfnt bytes; callers may apply WOFF wrappers.
    pub bytes: Vec<u8>,
    /// Original glyph IDs mapped to compact subset IDs, including dependencies.
    pub glyph_mapping: BTreeMap<u16, u16>,
    /// Signature removals and explicitly requested table losses.
    pub report: Report,
}
fn get<'a>(f: &Sfnt<'a>, t: [u8; 4]) -> Result<&'a [u8]> {
    f.table(t)
        .ok_or_else(|| invalid(format!("subset missing {}", String::from_utf8_lossy(&t))))
}
/// Report entries and their owned strings, each admitted before allocation.
/// Field order drops the entries before their charges.
struct Notes {
    entries: WorkingVector<ReportEntry>,
    text: Lease,
}
impl Notes {
    fn new(budget: &Budget) -> Result<Self> {
        Ok(Self {
            entries: WorkingVector::new(budget)?,
            text: budget.reserve(0)?,
        })
    }
    fn push(
        &mut self,
        budget: &Budget,
        (level, impact): (Level, Impact),
        code: &str,
        message: &[&str],
    ) -> Result<()> {
        let length = message
            .iter()
            .try_fold(0usize, |n, part| n.checked_add(part.len()))
            .ok_or_else(|| budget.error())?;
        let mut charge = budget.reserve(
            code.len()
                .checked_add(length)
                .ok_or_else(|| budget.error())?,
        )?;
        self.entries.reserve_exact(1)?;
        self.text.merge(&mut charge)?;
        let mut text = String::with_capacity(length);
        for part in message {
            text.push_str(part);
        }
        self.entries.push(ReportEntry {
            level,
            impact,
            code: code.into(),
            location: Default::default(),
            message: text,
        })
    }
    fn loss(&mut self, budget: &Budget, code: &str, message: &[&str]) -> Result<()> {
        self.push(budget, (Level::Warning, Impact::Loss), code, message)
    }
    fn into_parts(self) -> Result<(Report, Lease)> {
        let (entries, mut lease) = self.entries.into_parts();
        let mut text = self.text;
        let report = Report { entries };
        if let Err(error) = lease.merge(&mut text) {
            drop(report);
            return Err(error);
        }
        Ok((report, lease))
    }
}
/// Output tables in the final builder. The builder (and every table it owns)
/// drops before the lease that admitted its nodes and table bytes.
struct Tables<'a> {
    builder: SfntBuilder,
    lease: Lease,
    dropped: &'a BTreeSet<[u8; 4]>,
}
impl<'a> Tables<'a> {
    fn new(
        flavor: Flavor,
        count: usize,
        dropped: &'a BTreeSet<[u8; 4]>,
        budget: &Budget,
    ) -> Result<Self> {
        // Output tags are a subset of the source directory's tags.
        Ok(Self {
            lease: budget.reserve_tree::<[u8; 4], Vec<u8>>(count)?,
            builder: SfntBuilder::new(flavor),
            dropped,
        })
    }
    fn put(&mut self, tag: [u8; 4], table: Owned<Vec<u8>>) -> Result<()> {
        let (data, lease) = table.into_parts();
        self.put_parts([(tag, data)], lease)
    }
    /// Insert tables sharing one charge; explicitly dropped tags are discarded
    /// (their storage drops before the charge does).
    fn put_parts<const N: usize>(
        &mut self,
        parts: [([u8; 4], Vec<u8>); N],
        mut lease: Lease,
    ) -> Result<()> {
        if let Err(error) = self.lease.merge(&mut lease) {
            drop(parts);
            return Err(error);
        }
        for (tag, data) in parts {
            if !self.dropped.contains(&tag) {
                self.builder.insert(tag, data)?;
            }
        }
        Ok(())
    }
}
/// Copy borrowed table bytes into an exact ledger buffer.
fn copy(budget: &Budget, data: &[u8]) -> Result<Owned<Vec<u8>>> {
    let mut out = Buffer::with_capacity(budget, data.len())?;
    out.extend(data)?;
    Ok(out.into_owned())
}
/// Subset TTF, CFF1 OTF, bitmap-only sfnt and matching embedded bitmap strikes.
/// Unknown table semantics are rejected unless the exact tag is in `drop_tables`.
///
/// `limits.sfnt.resources.max_total_bytes` is the caller's remaining working
/// allowance: every allocation the subset makes is charged to one ledger with
/// that capacity, and failure is `Error::ResourceLimit`.
pub fn subset(data: &[u8], o: &Options) -> Result<Subset> {
    let working = Budget::new(
        o.limits.sfnt.resources.max_total_bytes,
        "sfnt subset working bytes",
    );
    let (result, _lease) = subset_with_budget(data, o, &working)?.into_parts();
    Ok(result)
}
/// Subset on a caller's shared ledger. The returned output bytes, glyph
/// mapping and report keep their charge in the returned lease.
pub(crate) fn subset_with_budget(
    data: &[u8],
    o: &Options,
    working: &Budget,
) -> Result<Owned<Subset>> {
    let l = &o.limits.sfnt;
    bounded(o.codepoints.len(), l.max_cmap_mappings, "subset codepoints")?;
    if o.codepoints.iter().any(|&cp| char::from_u32(cp).is_none()) {
        return Err(invalid("subset requires Unicode scalars"));
    }
    let f = Sfnt::parse_with_budget(data, l, working)?;
    let count = u16_at(get(&f, *b"maxp")?, 4)?;
    l.glyphs(usize::from(count))?;
    if o.drop_tables.contains(b"vhea") != o.drop_tables.contains(b"vmtx") {
        return Err(invalid("vhea and vmtx must be explicitly dropped together"));
    }
    let required = [
        *b"head", *b"hhea", *b"hmtx", *b"maxp", *b"cmap", *b"name", *b"OS/2", *b"post",
    ];
    if o.drop_tables.iter().any(|t| {
        required.contains(t)
            || [
                *b"glyf", *b"loca", *b"CFF ", *b"EBDT", *b"EBLC", *b"bdat", *b"bloc",
            ]
            .contains(t)
    }) {
        return Err(invalid(
            "cannot explicitly drop a required or paired glyph table",
        ));
    }
    let mut report = Notes::new(working)?;
    let rewritten = [
        *b"head", *b"maxp", *b"cmap", *b"hhea", *b"hmtx", *b"vhea", *b"vmtx", *b"glyf", *b"loca",
        *b"CFF ", *b"GSUB", *b"post", *b"OS/2", *b"EBDT", *b"EBLC", *b"bdat", *b"bloc", *b"VORG",
        *b"LTSH", *b"hdmx", *b"EBSC",
    ];
    // These tables have no glyph-indexed payload. Preserve every byte.
    let globals = [
        *b"name", *b"gasp", *b"cvt ", *b"fpgm", *b"prep", *b"VDMX", *b"PCLT", *b"meta",
    ];
    for t in f.tables() {
        if t.tag == *b"DSIG" {
            report.loss(
                working,
                "subset.signature_removed",
                &["Subsetting invalidates the source digital signature"],
            )?;
        } else if o.drop_tables.contains(&t.tag) {
            report.loss(
                working,
                "subset.table_dropped",
                &[
                    "Explicitly discarded table ",
                    &String::from_utf8_lossy(&t.tag),
                ],
            )?;
        } else if !rewritten.contains(&t.tag) && !globals.contains(&t.tag) {
            return Err(invalid(format!(
                "cannot safely remap table {}; explicitly drop that tag to acknowledge loss",
                String::from_utf8_lossy(&t.tag)
            )));
        }
    }
    // A Unicode nominal projection cannot silently discard UVS or legacy maps.
    // Explicit table dropping cannot remove cmap, because it is core output data.
    let cmap_data = get(&f, *b"cmap")?;
    let records = usize::from(u16_at(cmap_data, 2)?);
    bytes(cmap_data, 4, records * 8)?;
    for i in 0..records {
        let at = 4 + i * 8;
        let platform = u16_at(cmap_data, at)?;
        let encoding = u16_at(cmap_data, at + 2)?;
        let format = u16_at(cmap_data, u32_at(cmap_data, at + 4)? as usize)?;
        if format == 14 || !(platform == 0 || (platform == 3 && [1, 10].contains(&encoding))) {
            return Err(invalid(
                "subset cannot yet preserve variation or non-Unicode cmap records",
            ));
        }
    }
    let mut cmap =
        cmap::decode_with_budget(get(&f, *b"cmap")?, count, l, Some(&o.codepoints), working)?;
    let gsub = if o.drop_tables.contains(b"GSUB") {
        None
    } else {
        f.table(*b"GSUB")
            .map(|b| Gsub::decode_with_budget(b, count, &o.limits, working))
            .transpose()?
    };
    if gsub
        .as_ref()
        .and_then(|g| g.scripts.get(b"DFLT"))
        .is_some_and(|s| s.default.is_none())
    {
        report.push(
            working,
            (Level::Warning, Impact::Repair),
            "subset.dflt_langsys_repaired",
            &[
                "Added the required empty DFLT default language system; explicit language routing is unchanged",
            ],
        )?;
    }
    let mut keep_lease = working.reserve(0)?;
    let mut keep = BTreeSet::new();
    for glyph in std::iter::once(0).chain(cmap.values().copied()) {
        if !keep.contains(&glyph) {
            keep_lease.grow_to(working.tree_bytes::<u16, ()>(keep.len() + 1)?)?;
            keep.insert(glyph);
        }
    }
    if let Some(g) = &gsub {
        g.closure_with_budget(&mut keep, &mut keep_lease, working)?;
    }
    let gl = glyf::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    // Every glyph is fully validated; only composite child IDs are retained.
    let dependencies = if f.table(*b"glyf").is_some() {
        Some(Dependencies::from_sfnt_with_budget(&f, &gl, working)?)
    } else {
        None
    };
    if let Some(table) = &dependencies {
        let mut queue = WorkingVector::with_capacity(working, keep.len())?;
        for &glyph in &keep {
            queue.push(glyph)?;
        }
        let mut next = 0;
        while next < queue.len() {
            let gid = queue[next];
            next += 1;
            for &child in table.children(usize::from(gid)) {
                if !keep.contains(&child) {
                    keep_lease.grow_to(working.tree_bytes::<u16, ()>(keep.len() + 1)?)?;
                    queue.reserve(1)?;
                    keep.insert(child);
                    queue.push(child)?;
                }
            }
        }
    }
    // The mapping is returned; its charge moves into the returned lease.
    let mut mapping_lease = working.reserve_tree::<u16, u16>(keep.len())?;
    let mut mapping = BTreeMap::new();
    for (index, &glyph) in keep.iter().enumerate() {
        mapping.insert(glyph, index as u16);
    }
    let n = keep.len() as u16;
    let mut tables = Tables::new(f.flavor(), f.tables().len(), &o.drop_tables, working)?;
    let mut head = copy(working, get(&f, *b"head")?)?;
    bytes(&head, 0, 54)?;
    let mut maxp = copy(working, get(&f, *b"maxp")?)?;
    maxp[4..6].copy_from_slice(&n.to_be_bytes());
    if let Some(table) = dependencies {
        let raw = subset_glyf(&f, &table, &keep, &mapping, l, working)?;
        drop(table);
        head[50..52].copy_from_slice(&raw.index_to_loc_format.to_be_bytes());
        // Keep conservative original maxp maxima and the original global bbox.
        // Hint programs may use stored glyph bboxes through phantom points.
        tables.put(*b"glyf", raw.glyf)?;
        tables.put(*b"loca", raw.loca)?;
    } else if f.table(*b"CFF ").is_some() {
        #[cfg(feature = "cff")]
        {
            let cl = cff::Limits {
                sfnt: l.clone(),
                ..Default::default()
            };
            let c = cff::Cff::from_sfnt_with_budget(&f, &cl, working)?;
            let mut order_lease = working.reserve(0)?;
            let mut order = working.vec_into::<u16>(keep.len(), &mut order_lease)?;
            order.extend(keep.iter().copied());
            let table = c.subset_with_budget(&order, &cl, working)?;
            drop(order);
            drop(order_lease);
            drop(c);
            tables.put(*b"CFF ", table)?;
        }
        #[cfg(not(feature = "cff"))]
        return Err(crate::Error::FeatureDisabled {
            format: "cff".into(),
        });
    } else if f.table(*b"EBDT").is_none() && f.table(*b"bdat").is_none() {
        return Err(invalid("subset has no supported glyph data"));
    }
    tables.put(*b"head", head)?;
    tables.put(*b"maxp", maxp)?;
    for (axis, ht, mt) in [
        (Axis::Horizontal, *b"hhea", *b"hmtx"),
        (Axis::Vertical, *b"vhea", *b"vmtx"),
    ] {
        if axis == Axis::Horizontal || f.table(ht).is_some() || f.table(mt).is_some() {
            let (h, m) = metrics::decode_selected_with_budget(
                axis,
                get(&f, ht)?,
                get(&f, mt)?,
                count,
                &keep,
                l,
                working,
            )?;
            let encoded = metrics::encode_with_budget(axis, &h, &m, true, l, working)?;
            drop(m);
            let (encoded, lease) = encoded.into_parts();
            tables.put_parts([(ht, encoded.header), (mt, encoded.metrics)], lease)?;
        }
    }
    for value in cmap.values_mut() {
        *value = mapping[value];
    }
    let (encoded_cmap, cmap_lease) = cmap::encode_with_budget(&cmap, n, l, working)?.into_parts();
    tables.put_parts([(*b"cmap", encoded_cmap.data)], cmap_lease)?;
    let mut os = os2::Os2Table::decode(get(&f, *b"OS/2")?, l)?;
    os.set_unicode_coverage_map(&cmap)?;
    drop(cmap);
    tables.put(*b"OS/2", os.encode_with_budget(l, working)?)?;
    let post =
        post::PostTable::decode_selected_with_budget(get(&f, *b"post")?, count, &keep, l, working)?;
    tables.put(*b"post", post.encode_with_budget(n, l, working)?)?;
    drop(post);
    if let Some(source) = gsub {
        let g = source.subset_with_budget(&mapping, working)?;
        drop(source);
        if !g.features.is_empty() {
            tables.put(*b"GSUB", g.encode_with_budget(n, &o.limits, working)?)?;
        }
    }
    let bl = bitmap::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    let mut ppems_lease = working.reserve(0)?;
    let mut retained_ppems = BTreeSet::new();
    for kind in [bitmap::Kind::OpenType, bitmap::Kind::Apple] {
        let (dt, lt) = kind.tags();
        if f.table(dt).is_some() || f.table(lt).is_some() {
            let mut strikes = bitmap::decode_selected_with_budget(
                get(&f, lt)?,
                get(&f, dt)?,
                kind,
                count,
                &bl,
                &mapping,
                working,
            )?;
            strikes.retain(|s| !s.glyphs.is_empty());
            for s in strikes.iter() {
                let ppem = [s.ppem_x, s.ppem_y];
                if !retained_ppems.contains(&ppem) {
                    ppems_lease
                        .grow_to(working.tree_bytes::<[u8; 2], ()>(retained_ppems.len() + 1)?)?;
                    retained_ppems.insert(ppem);
                }
            }
            if !strikes.is_empty() {
                let (e, lease) =
                    bitmap::encode_with_budget(&strikes, kind, n, &bl, working)?.into_parts();
                drop(strikes);
                tables.put_parts([(dt, e.data), (lt, e.location)], lease)?;
            }
        }
    }
    if let Some(data) = f.table(*b"EBSC") {
        let mut scales = bitmap::decode_scaling_with_budget(data, &bl, working)?;
        scales.retain(|s| retained_ppems.contains(&s.source));
        if !scales.is_empty() {
            let encoded = bitmap::encode_scaling_with_budget(&scales, &bl, working)?;
            drop(scales);
            tables.put(*b"EBSC", encoded)?;
        }
    }
    drop(retained_ppems);
    drop(ppems_lease);
    if let Some(v) = f.table(*b"VORG") {
        tables.put(*b"VORG", vorg(v, count, &mapping, working)?)?;
    }
    if let Some(v) = f.table(*b"LTSH") {
        if u16_at(v, 0)? != 0 || u16_at(v, 2)? != count {
            return Err(invalid("invalid LTSH"));
        }
        bytes(v, 4, usize::from(count))?;
        let mut out = Buffer::with_capacity(working, 4 + keep.len())?;
        out.extend(&[0, 0])?;
        out.extend(&n.to_be_bytes())?;
        for &g in &keep {
            out.push(v[4 + usize::from(g)])?;
        }
        tables.put(*b"LTSH", out.into_owned())?;
    }
    if let Some(v) = f.table(*b"hdmx") {
        tables.put(*b"hdmx", hdmx(v, count, &keep, l, working)?)?;
    }
    drop(keep);
    drop(keep_lease);
    // Unchanged global tables are copied last, while every replacement is
    // still live, and all of them coexist with the final output buffer.
    for t in f.tables() {
        if globals.contains(&t.tag) && !o.drop_tables.contains(&t.tag) {
            tables.put(t.tag, copy(working, t.data)?)?;
        }
    }
    let output = tables.builder.build_with_budget(l, working)?;
    drop(tables);
    drop(f);
    let (bytes, mut lease) = output.into_parts();
    let (report, mut report_lease) = report.into_parts()?;
    let result = Subset {
        bytes,
        glyph_mapping: mapping,
        report,
    };
    if let Err(error) = lease
        .merge(&mut mapping_lease)
        .and_then(|()| lease.merge(&mut report_lease))
    {
        drop(result);
        return Err(error);
    }
    Ok(Owned::from_parts(result, lease))
}
fn vorg(
    data: &[u8],
    count: u16,
    map: &BTreeMap<u16, u16>,
    budget: &Budget,
) -> Result<Owned<Vec<u8>>> {
    if u32_at(data, 0)? != 0x10000 {
        return Err(invalid("unsupported VORG version"));
    }
    let n = usize::from(u16_at(data, 6)?);
    bytes(data, 8, 4 * n)?;
    let mut prev = None;
    let mut kept = 0usize;
    for i in 0..n {
        let id = u16_at(data, 8 + 4 * i)?;
        if id >= count || prev.is_some_and(|p| p >= id) {
            return Err(invalid("invalid VORG record"));
        }
        prev = Some(id);
        if map.contains_key(&id) {
            kept += 1;
        }
    }
    let mut out = Buffer::with_capacity(budget, 8 + 4 * kept)?;
    out.extend(&data[..6])?;
    out.extend(&to16(kept)?.to_be_bytes())?;
    for i in 0..n {
        if let Some(&new) = map.get(&u16_at(data, 8 + 4 * i)?) {
            out.extend(&new.to_be_bytes())?;
            out.extend(&data[10 + 4 * i..12 + 4 * i])?;
        }
    }
    Ok(out.into_owned())
}
fn hdmx(
    data: &[u8],
    count: u16,
    keep: &BTreeSet<u16>,
    l: &Limits,
    budget: &Budget,
) -> Result<Owned<Vec<u8>>> {
    if u16_at(data, 0)? != 0 {
        return Err(invalid("unsupported hdmx version"));
    }
    let n = usize::from(u16_at(data, 2)?);
    let stride = u32_at(data, 4)? as usize;
    if stride < usize::from(count) + 2 {
        return Err(invalid("short hdmx records"));
    }
    bytes(
        data,
        8,
        n.checked_mul(stride)
            .ok_or_else(|| invalid("hdmx size overflow"))?,
    )?;
    let size = (keep.len() + 5) & !3;
    l.table(8 + n * size)?;
    let mut out = Buffer::with_capacity(budget, 8 + n * size)?;
    out.resize(8 + n * size, 0)?;
    out[2..4].copy_from_slice(&(n as u16).to_be_bytes());
    out[4..8].copy_from_slice(&to32(size)?.to_be_bytes());
    for i in 0..n {
        let src = &data[8 + i * stride..];
        let dst = &mut out[8 + i * size..];
        dst[0] = src[0];
        for (j, &g) in keep.iter().enumerate() {
            dst[2 + j] = src[2 + usize::from(g)];
        }
        dst[1] = *dst[2..2 + keep.len()].iter().max().unwrap_or(&0);
    }
    Ok(out.into_owned())
}

/// Raw glyf/loca replacement tables and the matching head.indexToLocFormat.
struct GlyfSubset {
    glyf: Owned<Vec<u8>>,
    loca: Owned<Vec<u8>>,
    index_to_loc_format: i16,
}
/// Copy selected raw glyph records into exact pre-sized glyf/loca buffers,
/// renumbering composite children in place. Instructions, bounding boxes and
/// all other record bytes are preserved exactly.
fn subset_glyf(
    f: &Sfnt<'_>,
    dependencies: &Dependencies,
    keep: &BTreeSet<u16>,
    mapping: &BTreeMap<u16, u16>,
    l: &Limits,
    budget: &Budget,
) -> Result<GlyfSubset> {
    let loca = get(f, *b"loca")?;
    let glyf = get(f, *b"glyf")?;
    let original_format = u16_at(get(f, *b"head")?, 50)?;
    let offset = |gid: usize| -> Result<usize> {
        match original_format {
            0 => Ok(usize::from(u16_at(loca, 2 * gid)?) * 2),
            1 => Ok(u32_at(loca, 4 * gid)? as usize),
            _ => Err(invalid("invalid loca format")),
        }
    };
    let span = |gid: u16| -> Result<(usize, usize)> {
        let start = offset(usize::from(gid))?;
        let end = offset(usize::from(gid) + 1)?;
        let size = end
            .checked_sub(start)
            .ok_or_else(|| invalid("decreasing loca"))?;
        Ok((start, size))
    };
    // Size the output with the same per-record checks, in the same order, as
    // incremental writing would apply; every record is two-byte aligned.
    let mut total = 0usize;
    for &gid in keep {
        let (start, size) = span(gid)?;
        l.table(total.saturating_add(size).saturating_add(1))?;
        bytes(glyf, start, size)?;
        total += size + size % 2;
    }
    let mut out = Buffer::with_capacity(budget, total)?;
    for &gid in keep {
        let (start, size) = span(gid)?;
        let at = out.len();
        out.extend(&glyf[start..start + size])?;
        let children = dependencies.children(usize::from(gid));
        if !children.is_empty() {
            let record = &mut out[at..];
            let mut cursor = 10;
            // The shared glyf validator already checked all flags, arguments,
            // programs, graph edges and compound expansion budgets.
            for &child in children {
                let flags = u16_at(record, cursor)?;
                if u16_at(record, cursor + 2)? != child {
                    return Err(invalid("composite preflight mismatch"));
                }
                record[cursor + 2..cursor + 4].copy_from_slice(&mapping[&child].to_be_bytes());
                cursor += 4 + if flags & 1 != 0 { 4 } else { 2 };
                cursor += if flags & 8 != 0 {
                    2
                } else if flags & 64 != 0 {
                    4
                } else if flags & 128 != 0 {
                    8
                } else {
                    0
                };
            }
        }
        if !out.len().is_multiple_of(2) {
            out.push(0)?;
        }
    }
    let short = total / 2 <= usize::from(u16::MAX);
    let entry = if short { 2 } else { 4 };
    let size = (keep.len() + 1)
        .checked_mul(entry)
        .ok_or_else(|| invalid("loca size overflow"))?;
    if !short {
        to32(total)?;
    }
    l.table(size)?;
    let mut locations = Buffer::with_capacity(budget, size)?;
    let mut write = |position: usize| -> Result<()> {
        if short {
            locations.extend(&to16(position / 2)?.to_be_bytes())
        } else {
            locations.extend(&to32(position)?.to_be_bytes())
        }
    };
    let mut position = 0usize;
    for &gid in keep {
        write(position)?;
        let (_, size) = span(gid)?;
        position += size + size % 2;
    }
    write(position)?;
    Ok(GlyfSubset {
        glyf: out.into_owned(),
        loca: locations.into_owned(),
        index_to_loc_format: i16::from(!short),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, variants};

    const GOLDEN: &[u8] = include_bytes!("../../../../tests/data/compat/mini-square-1.0-0.ttf");
    const LEDGER: &str = "test shared subset ledger";

    fn sources() -> Vec<Vec<u8>> {
        let mut font = crate::Font::default();
        font.metrics.pixel_size = Some(14);
        font.metrics.ascent = Some(14);
        font.metrics.descent = Some(4);
        for (cp, pattern) in [(65, "#.\n##"), (66, "##\n#.")] {
            let mut glyph = crate::Glyph::new(pattern.parse::<crate::Bitmap>().unwrap());
            glyph.codepoints = vec![crate::model::Codepoint::Unicode(cp)];
            glyph.metrics.advance = Some(crate::model::Vector::new(3, 0));
            font.add_glyph(glyph).unwrap();
        }
        let v = crate::Variants::new(font);
        let dots = variants::Options {
            shape: crate::outline::Shape::Dot,
            dot_size: 0.9,
            embed_bitmaps: true,
            ..Default::default()
        };
        let mut sources = vec![GOLDEN.to_vec(), variants::encode(&v, &dots).unwrap().bytes];
        if cfg!(feature = "cff") {
            let cff = variants::Options {
                flavor: variants::OutlineFlavor::Cff,
                embed_bitmaps: false,
                ..dots
            };
            sources.push(variants::encode(&v, &cff).unwrap().bytes);
        }
        sources
    }

    /// Smallest capacity of a fresh shared ledger with which subsetting works.
    fn peak(source: &[u8], options: &Options) -> usize {
        let (mut low, mut high) = (0usize, 16 << 20);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            let budget = Budget::new(middle, LEDGER);
            match subset_with_budget(source, options, &budget) {
                Ok(_) => high = middle,
                Err(Error::ResourceLimit { .. }) => low = middle,
                Err(other) => panic!("{other:?}"),
            }
            assert_eq!(budget.remaining(), middle, "every charge is released");
        }
        high
    }

    #[test]
    fn a_live_caller_lease_reduces_the_shared_allowance() {
        const HELD: usize = 4096;
        let options = Options::text("A");
        for source in sources() {
            let peak = peak(&source, &options);
            // Exactly enough beside a caller's still-live lease on one ledger.
            let budget = Budget::new(peak + HELD, LEDGER);
            let held = budget.reserve(HELD).unwrap();
            let result = subset_with_budget(&source, &options, &budget).unwrap();
            // Output bytes and the glyph mapping stay charged to the caller
            // until the returned value drops.
            let retained = peak - budget.remaining();
            let mapping = budget
                .tree_bytes::<u16, u16>(result.glyph_mapping.len())
                .unwrap();
            assert!(retained >= result.bytes.len() + mapping, "{retained}");
            drop(result);
            assert_eq!(budget.remaining(), peak);
            drop(held);
            // One byte less beside the same live lease is this ledger's error,
            // and a failed subset leaves only the caller's lease charged.
            let budget = Budget::new(peak + HELD - 1, LEDGER);
            let _held = budget.reserve(HELD).unwrap();
            assert!(matches!(
                subset_with_budget(&source, &options, &budget),
                Err(Error::ResourceLimit {
                    resource: LEDGER,
                    ..
                })
            ));
            assert_eq!(budget.remaining(), peak - 1);
        }
    }
}
