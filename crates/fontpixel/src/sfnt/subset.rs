//! Compact Unicode subsets with GSUB/composite closure and explicit table safety.
use super::{
    glyf::{GlyfTable, Glyph, LocaFormat},
    gsub::{Gsub, LayoutLimits},
    metrics::{self, Axis},
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
fn loss(r: &mut Report, code: &str, msg: String) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: code.into(),
        location: Default::default(),
        message: msg,
    });
}
/// Subset TTF, CFF1 OTF, bitmap-only sfnt and matching embedded bitmap strikes.
/// Unknown table semantics are rejected unless the exact tag is in `drop_tables`.
pub fn subset(data: &[u8], o: &Options) -> Result<Subset> {
    let l = &o.limits.sfnt;
    bounded(o.codepoints.len(), l.max_cmap_mappings, "subset codepoints")?;
    if o.codepoints.iter().any(|&cp| char::from_u32(cp).is_none()) {
        return Err(invalid("subset requires Unicode scalars"));
    }
    let f = Sfnt::parse(data, l)?;
    let count = u16_at(get(&f, *b"maxp")?, 4)?;
    l.glyphs(usize::from(count))?;
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
    let mut report = Report::default();
    let rewritten = [
        *b"head", *b"maxp", *b"cmap", *b"hhea", *b"hmtx", *b"vhea", *b"vmtx", *b"glyf", *b"loca",
        *b"CFF ", *b"GSUB", *b"post", *b"OS/2", *b"EBDT", *b"EBLC", *b"bdat", *b"bloc", *b"VORG",
        *b"LTSH", *b"hdmx",
    ];
    // These tables have no glyph-indexed payload. Preserve every byte.
    let globals = [
        *b"name", *b"gasp", *b"cvt ", *b"fpgm", *b"prep", *b"VDMX", *b"PCLT", *b"EBSC", *b"meta",
    ];
    for t in f.tables() {
        if t.tag == *b"DSIG" {
            loss(
                &mut report,
                "subset.signature_removed",
                "Subsetting invalidates the source digital signature".into(),
            );
        } else if o.drop_tables.contains(&t.tag) {
            loss(
                &mut report,
                "subset.table_dropped",
                format!(
                    "Explicitly discarded table {}",
                    String::from_utf8_lossy(&t.tag)
                ),
            );
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
    let mut cmap = cmap::decode(get(&f, *b"cmap")?, count, l)?;
    cmap.retain(|cp, _| o.codepoints.contains(cp));
    let gsub = if o.drop_tables.contains(b"GSUB") {
        None
    } else {
        f.table(*b"GSUB")
            .map(|b| Gsub::decode(b, count, &o.limits))
            .transpose()?
    };
    let mut keep = cmap.values().copied().collect::<BTreeSet<_>>();
    keep.insert(0);
    if let Some(g) = &gsub {
        g.closure(&mut keep);
    }
    let gl = glyf::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    let mut glyf = if f.table(*b"glyf").is_some() {
        Some(GlyfTable::from_sfnt(&f, &gl)?)
    } else {
        None
    };
    if let Some(table) = &glyf {
        let mut queue = keep.iter().copied().collect::<Vec<_>>();
        while let Some(gid) = queue.pop() {
            if let Glyph::Composite(g) = &table.glyphs[usize::from(gid)] {
                for c in &g.components {
                    if keep.insert(c.glyph_id) {
                        queue.push(c.glyph_id);
                    }
                }
            }
        }
    }
    let mapping = keep
        .iter()
        .enumerate()
        .map(|(i, &g)| (g, i as u16))
        .collect::<BTreeMap<_, _>>();
    let n = keep.len() as u16;
    let mut replacements = BTreeMap::new();
    let mut head = get(&f, *b"head")?.to_vec();
    bytes(&head, 0, 54)?;
    let mut maxp = get(&f, *b"maxp")?.to_vec();
    maxp[4..6].copy_from_slice(&n.to_be_bytes());
    if let Some(table) = &mut glyf {
        table.glyphs = keep
            .iter()
            .map(|&id| table.glyphs[usize::from(id)].clone())
            .collect();
        for g in &mut table.glyphs {
            if let Glyph::Composite(g) = g {
                for c in &mut g.components {
                    c.glyph_id = mapping[&c.glyph_id];
                }
            }
        }
        let e = table.encode(LocaFormat::Auto, &gl)?;
        head[50..52].copy_from_slice(&e.index_to_loc_format.to_be_bytes());
        for (i, b) in e.bounds.iter().enumerate() {
            head[36 + 2 * i..38 + 2 * i].copy_from_slice(&b.to_be_bytes());
        }
        bytes(&maxp, 0, 32)?;
        for (at, v) in [
            (6, e.statistics.max_points),
            (8, e.statistics.max_contours),
            (10, e.statistics.max_composite_points),
            (12, e.statistics.max_composite_contours),
            (26, e.statistics.max_size_of_instructions),
            (28, e.statistics.max_component_elements),
            (30, e.statistics.max_component_depth),
        ] {
            maxp[at..at + 2].copy_from_slice(&v.to_be_bytes());
        }
        replacements.insert(*b"glyf", e.glyf);
        replacements.insert(*b"loca", e.loca);
    } else if f.table(*b"CFF ").is_some() {
        #[cfg(feature = "cff")]
        {
            let cl = cff::Limits {
                sfnt: l.clone(),
                ..Default::default()
            };
            let c = cff::Cff::from_sfnt(&f, &cl)?;
            replacements.insert(
                *b"CFF ",
                c.subset(&keep.iter().copied().collect::<Vec<_>>(), &cl)?,
            );
        }
        #[cfg(not(feature = "cff"))]
        return Err(crate::Error::FeatureDisabled {
            format: "cff".into(),
        });
    } else if f.table(*b"EBDT").is_none() && f.table(*b"bdat").is_none() {
        return Err(invalid("subset has no supported glyph data"));
    }
    replacements.insert(*b"head", head);
    replacements.insert(*b"maxp", maxp);
    for (axis, ht, mt) in [
        (Axis::Horizontal, *b"hhea", *b"hmtx"),
        (Axis::Vertical, *b"vhea", *b"vmtx"),
    ] {
        if axis == Axis::Horizontal || f.table(ht).is_some() || f.table(mt).is_some() {
            let (h, m) = metrics::decode(axis, get(&f, ht)?, get(&f, mt)?, count, l)?;
            let m = keep.iter().map(|&i| m[usize::from(i)]).collect::<Vec<_>>();
            let e = metrics::encode(axis, &h, &m, true, l)?;
            replacements.insert(ht, e.header);
            replacements.insert(mt, e.metrics);
        }
    }
    for value in cmap.values_mut() {
        *value = mapping[value];
    }
    replacements.insert(*b"cmap", cmap::encode(&cmap, n, l)?.data);
    let mut os = os2::Os2Table::decode(get(&f, *b"OS/2")?, l)?;
    os.set_unicode_coverage(&cmap.keys().copied().collect())?;
    replacements.insert(*b"OS/2", os.encode(l)?);
    let mut post = post::PostTable::decode(get(&f, *b"post")?, count, l)?;
    if let Some(names) = &mut post.names {
        *names = keep
            .iter()
            .map(|&i| names[usize::from(i)].clone())
            .collect();
    }
    replacements.insert(*b"post", post.encode(n, l)?);
    if let Some(g) = gsub {
        let g = g.subset(&mapping);
        if !g.features.is_empty() {
            replacements.insert(*b"GSUB", g.encode(n, &o.limits)?);
        }
    }
    let bl = bitmap::Limits {
        sfnt: l.clone(),
        ..Default::default()
    };
    for kind in [bitmap::Kind::OpenType, bitmap::Kind::Apple] {
        let (dt, lt) = kind.tags();
        if f.table(dt).is_some() || f.table(lt).is_some() {
            let mut strikes = bitmap::decode(get(&f, lt)?, get(&f, dt)?, kind, count, &bl)?;
            for s in &mut strikes {
                s.glyphs = std::mem::take(&mut s.glyphs)
                    .into_iter()
                    .filter_map(|(g, b)| mapping.get(&g).map(|&n| (n, b)))
                    .collect();
            }
            strikes.retain(|s| !s.glyphs.is_empty());
            if !strikes.is_empty() {
                let e = bitmap::encode(&strikes, kind, n, &bl)?;
                replacements.insert(dt, e.data);
                replacements.insert(lt, e.location);
            }
        }
    }
    if let Some(v) = f.table(*b"VORG") {
        replacements.insert(*b"VORG", vorg(v, count, &mapping)?);
    }
    if let Some(v) = f.table(*b"LTSH") {
        if u16_at(v, 0)? != 0 || u16_at(v, 2)? != count {
            return Err(invalid("invalid LTSH"));
        }
        bytes(v, 4, usize::from(count))?;
        let mut out = vec![0, 0];
        p16(&mut out, n);
        out.extend(keep.iter().map(|&g| v[4 + usize::from(g)]));
        replacements.insert(*b"LTSH", out);
    }
    if let Some(v) = f.table(*b"hdmx") {
        replacements.insert(*b"hdmx", hdmx(v, count, &keep, l)?);
    }
    let mut b = SfntBuilder::new(f.flavor());
    for t in f.tables() {
        if globals.contains(&t.tag) && !o.drop_tables.contains(&t.tag) {
            b.insert(t.tag, t.data.to_vec())?;
        }
    }
    for (t, v) in replacements {
        if !o.drop_tables.contains(&t) {
            b.insert(t, v)?;
        }
    }
    Ok(Subset {
        bytes: b.build(l)?,
        glyph_mapping: mapping,
        report,
    })
}
fn vorg(data: &[u8], count: u16, map: &BTreeMap<u16, u16>) -> Result<Vec<u8>> {
    if u32_at(data, 0)? != 0x10000 {
        return Err(invalid("unsupported VORG version"));
    }
    let n = usize::from(u16_at(data, 6)?);
    bytes(data, 8, 4 * n)?;
    let mut pairs = Vec::new();
    let mut prev = None;
    for i in 0..n {
        let id = u16_at(data, 8 + 4 * i)?;
        if id >= count || prev.is_some_and(|p| p >= id) {
            return Err(invalid("invalid VORG record"));
        }
        prev = Some(id);
        if let Some(&new) = map.get(&id) {
            pairs.push((new, u16_at(data, 10 + 4 * i)?));
        }
    }
    let mut out = data[..6].to_vec();
    p16(&mut out, to16(pairs.len())?);
    for (a, b) in pairs {
        p16(&mut out, a);
        p16(&mut out, b);
    }
    Ok(out)
}
fn hdmx(data: &[u8], count: u16, keep: &BTreeSet<u16>, l: &Limits) -> Result<Vec<u8>> {
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
    let mut out = vec![0; 8 + n * size];
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
    Ok(out)
}
