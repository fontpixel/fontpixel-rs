use super::*;
use crate::resources::{Input, ResourceResolver};
struct Resolver<'a>(&'a OutputBundle);
impl ResourceResolver for Resolver<'_> {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let bytes = &self
            .0
            .assets()
            .iter()
            .find(|a| &a.path == p)
            .ok_or_else(|| Error::MissingResource(p.to_string()))?
            .bytes;
        bound(bytes.len(), max, "BMFont resource")?;
        Ok(bytes.clone())
    }
}
struct Placement<'a> {
    glyph: &'a Glyph,
    x: u32,
    y: u32,
    page: usize,
}
pub(super) fn write(f: &Font, primary: &ResourcePath, o: &BmfontWriteOptions) -> Result<Written> {
    f.validate(&o.limits)?;
    if o.page_width == 0
        || o.page_height == 0
        || o.page_width > u16::MAX as u32
        || o.page_height > u16::MAX as u32
    {
        return Err(invalid("atlas dimensions must be 1..65535"));
    }
    let area =
        usize::try_from(u64::from(o.page_width) * u64::from(o.page_height)).map_err(invalid)?;
    bound(area, o.limits.max_decompressed_bytes, "BMFont atlas pixels")?;
    let source_bytes = fidelity::model_bytes(f)?;
    let mut budget = Budget::new(source_bytes, &o.limits)?;
    budget.add(fidelity::properties_bytes(&f.properties)?)?;
    let face_bytes = f.names.family.as_ref().map_or(0, String::len);
    bound(
        face_bytes,
        o.limits.max_table_bytes.min(o.limits.max_input_bytes),
        "BMFont family bytes",
    )?;
    budget.add(face_bytes)?;
    budget.temporary(area.saturating_mul(8))?;
    budget.add(area.saturating_mul(8).saturating_add(1024 * 1024))?;
    budget.add(f.len().saturating_mul(2048))?;
    for (_, g) in f.glyphs() {
        budget.add(g.bitmap.as_bytes().len())?;
    }
    let mut placements = Vec::with_capacity(f.len());
    let (mut x, mut y, mut row, mut page) = (0u32, 0u32, 0u32, 0usize);
    for (_, g) in f.glyphs() {
        let w = g.bitmap.width();
        let h = g.bitmap.height();
        if w > o.page_width || h > o.page_height {
            return Err(invalid("glyph exceeds atlas size"));
        }
        if x.checked_add(w).is_none_or(|v| v > o.page_width) {
            x = 0;
            y = y
                .checked_add(row)
                .and_then(|v| v.checked_add(o.padding))
                .ok_or_else(|| invalid("packing overflow"))?;
            row = 0;
        }
        if y.checked_add(h).is_none_or(|v| v > o.page_height) {
            page += 1;
            x = 0;
            y = 0;
            row = 0;
        }
        bound(page + 1, o.limits.max_resources.min(256), "BMFont pages")?;
        placements.push(Placement {
            glyph: g,
            x,
            y,
            page,
        });
        x = x
            .checked_add(w)
            .and_then(|v| v.checked_add(o.padding))
            .ok_or_else(|| invalid("packing overflow"))?;
        row = row.max(h);
    }
    let page_count = page + 1;
    let mut records = Vec::new();
    let mut info = Record::new("info");
    for p in f.properties.iter().filter(|p| p.name == "bmfont.info") {
        info = Record::from_property(p)?;
    }
    info.set("face", f.names.family.as_deref().unwrap_or("Untitled"));
    info.set("size", f.metrics.pixel_size.unwrap_or(0));
    let legacy = f
        .glyphs()
        .flat_map(|(_, g)| &g.codepoints)
        .any(|v| matches!(v, Codepoint::Legacy(_)));
    let unicode = f
        .glyphs()
        .flat_map(|(_, g)| &g.codepoints)
        .any(|v| matches!(v, Codepoint::Unicode(_)));
    if legacy && unicode {
        return Err(invalid("mixed encoding domains require explicit remapping"));
    }
    info.set("unicode", u8::from(!legacy));
    info.fields.entry("charset".into()).or_insert_with(|| {
        if legacy {
            f.charset.name.clone().unwrap_or_default()
        } else {
            String::new()
        }
    });
    for (k, v) in [
        ("bold", "0"),
        ("italic", "0"),
        ("stretchH", "100"),
        ("smooth", "1"),
        ("aa", "1"),
        ("padding", "0,0,0,0"),
        ("spacing", "0,0"),
        ("outline", "0"),
    ] {
        info.fields.entry(k.into()).or_insert_with(|| v.into());
    }
    records.push(info);
    let base = f
        .metrics
        .ascent
        .unwrap_or_else(|| i64::from(f.metrics.pixel_size.unwrap_or(0)));
    let line = match f.metrics.line_height {
        Some(n) => n,
        None => base
            .checked_add(f.metrics.descent.unwrap_or(0))
            .ok_or_else(|| invalid("line height overflow"))?,
    };
    let mut common = Record::new("common");
    for (k, v) in [
        ("base", base),
        ("lineHeight", line),
        ("scaleW", i64::from(o.page_width)),
        ("scaleH", i64::from(o.page_height)),
        ("pages", page_count as i64),
        ("packed", 0),
        ("alphaChnl", 4),
        ("redChnl", 0),
        ("greenChnl", 0),
        ("blueChnl", 0),
    ] {
        common.set(k, v);
    }
    records.push(common);
    let primary_text = primary.as_str();
    let (dir, file) = primary_text
        .rsplit_once('/')
        .map_or(("", primary_text), |(a, b)| (a, b));
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let mut assets = Vec::new();
    for page in 0..page_count {
        let file = format!("{stem}_{page:04}.png");
        let asset_name = if dir.is_empty() {
            file.clone()
        } else {
            format!("{dir}/{file}")
        };
        let path = ResourcePath::new(asset_name)?;
        let mut r = Record::new("page");
        r.set("id", page);
        r.set("file", file);
        records.push(r);
        let mut raster = vec![0u8; area];
        for p in placements.iter().filter(|p| p.page == page) {
            let w = p.glyph.bitmap.width() as usize;
            let h = p.glyph.bitmap.height() as usize;
            for yy in 0..h {
                let dest = (p.y as usize + yy) * o.page_width as usize + p.x as usize;
                match &p.glyph.bitmap {
                    GlyphBitmap::Gray8(b) => {
                        raster[dest..dest + w].copy_from_slice(&b.as_bytes()[yy * w..(yy + 1) * w])
                    }
                    GlyphBitmap::Mono(b) => {
                        let row = &b.as_bytes()[yy * w.div_ceil(8)..(yy + 1) * w.div_ceil(8)];
                        for xx in 0..w {
                            raster[dest + xx] = if row[xx / 8] & (0x80 >> (xx % 8)) != 0 {
                                255
                            } else {
                                0
                            };
                        }
                    }
                }
            }
        }
        let png_bound = budget.remaining().min(o.limits.max_resource_bytes);
        let mut sink = fidelity::Sink {
            bytes: Vec::new(),
            max: png_bound,
        };
        {
            let mut encoder = png::Encoder::new(&mut sink, o.page_width, o.page_height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut png = encoder.write_header().map_err(invalid)?;
            png.write_image_data(&raster).map_err(invalid)?;
            png.finish().map_err(invalid)?;
        }
        let bytes = sink.bytes;
        bound(
            bytes.len(),
            o.limits.max_resource_bytes,
            "BMFont PNG output",
        )?;
        budget.add(bytes.capacity())?;
        assets.push(OutputAsset { path, bytes });
    }
    let mut count = Record::new("chars");
    count.set("count", placements.len());
    records.push(count);
    for p in placements {
        let g = p.glyph;
        let mut r = Record::new("char");
        r.set("id", g.codepoints.first().map_or(u32::MAX, |cp| cp.value()));
        for (k, v) in [
            ("x", i64::from(p.x)),
            ("y", i64::from(p.y)),
            ("width", i64::from(g.bitmap.width())),
            ("height", i64::from(g.bitmap.height())),
            ("xoffset", g.metrics.bounds.x),
            (
                "yoffset",
                base.checked_sub(g.metrics.bounds.y)
                    .and_then(|n| n.checked_sub(i64::from(g.bitmap.height())))
                    .ok_or_else(|| invalid("glyph image offset overflow"))?,
            ),
            ("xadvance", g.metrics.advance.unwrap_or_default().x),
            ("page", p.page as i64),
            ("chnl", 15),
        ] {
            r.set(k, v);
        }
        records.push(r);
    }
    let mut kernings = Vec::new();
    for p in f.properties.iter().filter(|p| p.name == "bmfont.kerning") {
        kernings.push(Record::from_property(p)?);
    }
    if !kernings.is_empty() {
        let mut count = Record::new("kernings");
        count.set("count", kernings.len());
        records.push(count);
        records.extend(kernings);
    }
    let mut encoding_limits = o.limits.clone();
    encoding_limits.max_total_bytes = budget.remaining();
    let bytes = wire::encode(&records, o.encoding, &encoding_limits)?;
    budget.add(bytes.len())?;
    assets.push(OutputAsset {
        path: primary.clone(),
        bytes,
    });
    let output = OutputBundle::new(primary.clone(), assets)?;
    drop(records);
    // Resource paths in descriptors are relative to the primary's directory.
    let resolver = RelativeResolver {
        bundle: &output,
        dir,
    };
    let input = Input::new(
        output
            .assets()
            .iter()
            .find(|a| &a.path == primary)
            .ok_or_else(|| invalid("missing output primary"))?
            .bytes
            .as_slice(),
    )
    .with_resources(&resolver);
    let mut read_limits = o.limits.clone();
    let retained_output = output
        .assets()
        .iter()
        .map(|a| a.bytes.capacity())
        .sum::<usize>();
    read_limits.max_total_bytes = read_limits
        .max_total_bytes
        .checked_sub(source_bytes.saturating_add(retained_output))
        .ok_or_else(|| invalid("insufficient aggregate writer budget"))?;
    let mut context = ReadContext::new(&input, &read_limits)?;
    let (out, mut report) = read(&mut context, &BmfontReadOptions::default())?;
    let report_budget = Budget::new(
        source_bytes
            .saturating_add(retained_output)
            .saturating_add(fidelity::model_bytes(&out)?),
        &o.limits,
    )?
    .remaining();
    audit(f, &out, "bmfont", 0, &mut report, report_budget)?;
    Ok(Written { output, report })
}
struct RelativeResolver<'a> {
    bundle: &'a OutputBundle,
    dir: &'a str,
}
impl ResourceResolver for RelativeResolver<'_> {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let name = if self.dir.is_empty() {
            p.clone()
        } else {
            ResourcePath::new(format!("{}/{}", self.dir, p.as_str()))?
        };
        Resolver(self.bundle).resolve(&name, max)
    }
}
