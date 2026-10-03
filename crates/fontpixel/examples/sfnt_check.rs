//! Generate and re-read sfnt common-table integration fixtures for external tools.
use fontpixel::sfnt::read_fonts::TableProvider;
use fontpixel::sfnt::{
    Flavor, Limits, Sfnt, SfntBuilder, cmap, gasp,
    metrics::{self, Axis, Header, LongMetric},
    name::{NameRecord, NameTable},
    os2::Os2Table,
    post::PostTable,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
fn rewrite(source: &[u8], path: &Path) -> fontpixel::Result<()> {
    let l = Limits::default();
    let font = Sfnt::parse(source, &l)?;
    font.validate_checksums()?;
    let count = font
        .font_ref()
        .maxp()
        .map_err(|e| fontpixel::Error::InvalidInput(e.to_string()))?
        .num_glyphs();
    let get = |tag| {
        font.table(tag)
            .ok_or_else(|| fontpixel::Error::InvalidInput("missing fixture table".into()))
    };
    let mut replacements = BTreeMap::new();
    let mut map = cmap::decode(get(*b"cmap")?, count, &l)?;
    map.insert(0xffff, 2);
    map.insert(0x10000, 2);
    map.insert(0x1f600, 3);
    replacements.insert(*b"cmap", cmap::encode(&map, count, &l)?.data);
    let (h, m) = metrics::decode(Axis::Horizontal, get(*b"hhea")?, get(*b"hmtx")?, count, &l)?;
    let e = metrics::encode(Axis::Horizontal, &h, &m, true, &l)?;
    replacements.insert(*b"hhea", e.header);
    replacements.insert(*b"hmtx", e.metrics);
    let vertical: Vec<_> = (0..count)
        .map(|_| LongMetric {
            advance: 512,
            bearing: 64,
        })
        .collect();
    let mut vh = Header::new(Axis::Vertical);
    vh.ascender = 256;
    vh.descender = -256;
    vh.recompute(&vertical, &vec![Some(448); count as usize])?;
    let ve = metrics::encode(Axis::Vertical, &vh, &vertical, true, &l)?;
    replacements.insert(*b"vhea", ve.header);
    replacements.insert(*b"vmtx", ve.metrics);
    let mut os2 = Os2Table::decode(get(*b"OS/2")?, &l)?;
    os2.set_unicode_coverage(&map.keys().copied().collect::<BTreeSet<_>>())?;
    replacements.insert(*b"OS/2", os2.encode(&l)?);
    let mut names = NameTable::decode(get(*b"name")?, &l)?;
    names
        .records
        .push(NameRecord::unicode(16, 0x411, "点阵ミニ"));
    replacements.insert(*b"name", names.encode(&l)?);
    let post = PostTable::decode(get(*b"post")?, count, &l)?;
    replacements.insert(*b"post", post.encode(count, &l)?);
    replacements.insert(
        *b"gasp",
        gasp::GaspTable {
            version: 1,
            ranges: vec![gasp::Range {
                max_ppem: 65535,
                behavior: 0,
            }],
        }
        .encode(&l)?,
    );
    let mut builder = SfntBuilder::new(Flavor::TrueType);
    for t in font.tables() {
        if !replacements.contains_key(&t.tag) {
            builder.insert(t.tag, t.data.to_vec())?;
        }
    }
    for (tag, data) in replacements {
        builder.insert(tag, data)?;
    }
    let data = builder.build(&l)?;
    let reread = Sfnt::parse(&data, &l)?;
    reread.validate_checksums()?;
    assert_eq!(
        cmap::decode(reread.table(*b"cmap").unwrap(), count, &l)?,
        map
    );
    assert_eq!(Os2Table::decode(reread.table(*b"OS/2").unwrap(), &l)?, os2);
    assert_eq!(
        PostTable::decode(reread.table(*b"post").unwrap(), count, &l)?,
        post
    );
    let (_, vm) = metrics::decode(
        Axis::Vertical,
        reread.table(*b"vhea").unwrap(),
        reread.table(*b"vmtx").unwrap(),
        count,
        &l,
    )?;
    assert_eq!(vm, vertical);
    std::fs::write(path, data)?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--inspect" {
        let data = std::fs::read(&args[2])?;
        let l = Limits::default();
        let f = Sfnt::parse(&data, &l)?;
        f.validate_checksums()?;
        let count = f.font_ref().maxp()?.num_glyphs();
        let get = |t| f.table(t).unwrap();
        let map = cmap::decode(get(*b"cmap"), count, &l)?;
        let n = NameTable::decode(get(*b"name"), &l)?;
        let o = Os2Table::decode(get(*b"OS/2"), &l)?;
        let p = PostTable::decode(get(*b"post"), count, &l)?;
        let (_, hm) = metrics::decode(Axis::Horizontal, get(*b"hhea"), get(*b"hmtx"), count, &l)?;
        let (_, vm) = metrics::decode(Axis::Vertical, get(*b"vhea"), get(*b"vmtx"), count, &l)?;
        let g = gasp::GaspTable::decode(get(*b"gasp"), &l)?;
        println!(
            "{}",
            serde_json::json!({"glyphs":count,"cmap":map,"names":n.records.iter().map(|r|serde_json::json!([r.platform_id,r.encoding_id,r.language_id,r.name_id,r.text().unwrap_or(None)])).collect::<Vec<_>>(),"unicode_ranges":o.base.unicode_ranges,"post_names":p.names,"hmtx":hm.iter().map(|m|[i32::from(m.advance),i32::from(m.bearing)]).collect::<Vec<_>>(),"vmtx":vm.iter().map(|m|[i32::from(m.advance),i32::from(m.bearing)]).collect::<Vec<_>>(),"gasp_ranges":g.ranges.iter().map(|r|[r.max_ppem,r.behavior]).collect::<Vec<_>>()})
        );
        return Ok(());
    }
    let dir = args
        .get(1)
        .ok_or("usage: sfnt_check OUTPUT_DIR | --inspect FONT")?;
    std::fs::create_dir_all(dir)?;
    rewrite(
        include_bytes!("../../../tests/data/compat/mini-square-1.0-0.ttf"),
        &Path::new(dir).join("common.ttf"),
    )?;
    // Also validate the common layer over freshly generated compatibility
    // outlines; this preserves W2.1's boundary rather than adding a new codec.
    let mut font = fontpixel::compat::bdfparser::Font::default();
    font.load_string(include_str!("../../../tests/data/compat/mini.bdf"))?;
    let fresh = font.tottf(&Default::default())?;
    rewrite(&fresh, &Path::new(dir).join("fresh-common.ttf"))?;
    let common = std::fs::read(Path::new(dir).join("common.ttf"))?;
    let l = Limits::default();
    let source = Sfnt::parse(&common, &l)?;
    for (label, map) in [
        (
            "dense",
            (0..1000)
                .map(|cp| (cp, 1 + (cp % 3) as u16))
                .collect::<BTreeMap<_, _>>(),
        ),
        ("sparse", (0..0xd800).step_by(2).map(|cp| (cp, 2)).collect()),
    ] {
        let mut b = SfntBuilder::new(Flavor::TrueType);
        let encoded = cmap::encode(&map, 4, &l)?;
        let mut os2 = Os2Table::decode(source.table(*b"OS/2").unwrap(), &l)?;
        os2.set_unicode_coverage(&map.keys().copied().collect())?;
        for t in source.tables() {
            if t.tag != *b"cmap" && t.tag != *b"OS/2" {
                b.insert(t.tag, t.data.to_vec())?;
            }
        }
        b.insert(*b"cmap", encoded.data)?;
        b.insert(*b"OS/2", os2.encode(&l)?)?;
        let out = b.build(&l)?;
        Sfnt::parse(&out, &l)?.validate_checksums()?;
        std::fs::write(Path::new(dir).join(format!("{label}.ttf")), out)?;
    }
    Ok(())
}
