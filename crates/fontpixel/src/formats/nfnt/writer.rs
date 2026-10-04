use super::{
    Container, NfntReadOptions, NfntWriteOptions, ResourceType,
    container::{self, Resource},
    raster,
    support::*,
};
use crate::model::{
    Codepoint, Font, FontSet, Glyph, GlyphBitmap, Properties, PropertyValue, Spacing,
};
use crate::resources::{OutputAsset, OutputBundle, ResourcePath, Written};
use crate::{Error, Report, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Encode one asset. AppleDouble uses [`write_bundle`] because it has two assets.
pub fn write(set: &FontSet, o: &NfntWriteOptions) -> Result<(Vec<u8>, Report)> {
    let w = write_bundle(set, o, &ResourcePath::new("font.dfont")?)?;
    Ok((w.output.into_single()?, w.report))
}
/// Encode all assets, with readback verification under the selected resource limits.
pub fn write_bundle(set: &FontSet, o: &NfntWriteOptions, path: &ResourcePath) -> Result<Written> {
    let l = &o.limits;
    let mut budget = Budget::new(0, l)?;
    check(
        path.as_str().len(),
        l.max_table_bytes,
        "NFNT output asset name",
    )?;
    budget.add(path.as_str().len().saturating_mul(4))?;
    check(set.fonts.len(), l.max_resources, "NFNT output font count")?;
    model_cost(set, &mut budget, l)?;
    budget.glyphs = 0;
    let mut report = Report::default();
    if set.fonts.is_empty() {
        return Err(invalid("cannot write an empty font collection"));
    }
    let output = if o.container == Container::Bare {
        let f = set.single()?;
        let b = encode_font(f, o, &mut budget, &mut report, 0, None)?;
        OutputBundle::single(path.clone(), b)
    } else {
        let fork = encode_fork(set, o, &mut budget, &mut report)?;
        wrap(fork, set, o, path, &mut budget)?
    };
    let total = output.assets().iter().try_fold(0usize, |n, a| {
        n.checked_add(a.bytes.len())
            .ok_or_else(|| invalid("output size overflow"))
    })?;
    check(total, o.max_output_bytes, "NFNT output bytes")?;
    for asset in output.assets() {
        check(
            asset.bytes.len(),
            l.max_input_bytes,
            "NFNT output readable input",
        )?;
    }
    // Retain source, encoded resources and reports while verifying generated output.
    // This is one budget, not one fresh allowance per strike or resource.
    let parse_bytes = if o.container == Container::AppleDouble {
        output
            .assets()
            .iter()
            .find(|a| a.path != *path)
            .ok_or_else(|| invalid("AppleDouble companion missing"))?
    } else {
        output
            .assets()
            .iter()
            .find(|a| a.path == *path)
            .ok_or_else(|| invalid("primary missing"))?
    };
    if o.container == Container::AppleDouble {
        check(
            parse_bytes.bytes.len(),
            l.max_resource_bytes,
            "NFNT output companion bytes",
        )?;
        budget.resource(1, l)?;
    }
    let (mut decoded, rr) = container::parse(
        &parse_bytes.bytes,
        &NfntReadOptions::default(),
        l,
        &mut budget,
    )?;
    if o.container == Container::AppleDouble {
        let data = bytes(&set.properties, "nfnt.data-fork").unwrap_or(&[]);
        budget.add(data.len() + 128)?;
        decoded.properties.push(crate::model::Property::new(
            "nfnt.data-fork",
            PropertyValue::Bytes(data.to_vec()),
        ));
    }
    report.extend(rr);
    compare_sets(set, &decoded, &mut budget, &mut report)?;
    Ok(Written { output, report })
}
fn same_raster(f: &Font, g: &Font) -> bool {
    f.len() == g.len()
        && f.glyphs().zip(g.glyphs()).all(|((id, a), (nid, b))| {
            id == nid
                && a.codepoints == b.codepoints
                && a.bitmap == b.bitmap
                && a.metrics.bounds == b.metrics.bounds
                && a.metrics.advance == b.metrics.advance
        })
        && f.metrics.ascent == g.metrics.ascent
        && f.metrics.descent == g.metrics.descent
        && f.metrics.bounds == g.metrics.bounds
        && f.metrics.line_gap == g.metrics.line_gap
        && f.metrics.spacing == g.metrics.spacing
        && f.metrics.default_glyph == g.metrics.default_glyph
}
fn encode_font(
    f: &Font,
    o: &NfntWriteOptions,
    budget: &mut Budget,
    r: &mut Report,
    fi: usize,
    palette: Option<&super::palette::Palette>,
) -> Result<Vec<u8>> {
    if let Some(raw) = bytes(&f.properties, "nfnt.source")
        .filter(|raw| u16be(raw, 0).is_ok_and(|kind| kind & 0x280 == 0 || palette.is_some()))
    {
        let before = budget.used;
        let glyph_before = budget.glyphs;
        let original = raster::parse_with_palette(
            raw,
            &NfntReadOptions::default(),
            &o.limits,
            budget,
            palette,
        )?;
        let same = same_raster(f, &original)
            && !(o.gray_threshold.is_some()
                && f.glyphs()
                    .any(|(_, g)| matches!(g.bitmap, GlyphBitmap::Gray8(_))));
        drop(original);
        budget.used = before;
        budget.glyphs = glyph_before;
        if same {
            check(
                raw.len(),
                o.limits.max_resource_bytes,
                "NFNT output resource",
            )?;
            budget.add(raw.len())?;
            return Ok(raw.to_vec());
        }
    }
    let mut slots: [Option<(usize, &Glyph)>; 256] = [None; 256];
    let mut any = false;
    for (id, g) in f.glyphs() {
        for &cp in &g.codepoints {
            match cp {
                Codepoint::Legacy(v) if v <= 255 => {
                    if let Some((old, _)) = slots[v as usize].replace((id.index(), g)) {
                        budget.diagnostic(
                            r,
                            "duplicate_mapping",
                            Some(fi),
                            Some(old),
                            Some(v),
                            "A later glyph replaces this duplicate legacy mapping",
                        )?;
                    }
                    any = true;
                }
                _ => budget.diagnostic(
                    r,
                    "mapping_omitted",
                    Some(fi),
                    Some(id.index()),
                    Some(cp.value()),
                    "NFNT requires legacy byte codes; mapping omitted",
                )?,
            }
        }
        if g.codepoints.is_empty() && f.metrics.default_glyph != Some(id) {
            budget.diagnostic(
                r,
                "unencoded_omitted",
                Some(fi),
                Some(id.index()),
                None,
                "Only the designated missing glyph can remain unencoded",
            )?;
        }
    }
    let missing = f
        .metrics
        .default_glyph
        .and_then(|id| f.glyph(id).map(|g| (id.index(), g)));
    if !any && missing.is_none() {
        budget.diagnostic(
            r,
            "no_representable_glyphs",
            Some(fi),
            None,
            None,
            "No legacy byte mappings or designated missing glyph can be encoded",
        )?;
        if let Some(e) = r.entries.last_mut() {
            e.level = crate::report::Level::Error;
        }
        return Err(Error::ReportedErrors(std::mem::take(r)));
    }
    let first = slots.iter().position(Option::is_some).unwrap_or(0);
    let last = slots.iter().rposition(Option::is_some).unwrap_or(first);
    let selected = slots[first..=last].iter().copied().chain([missing]);
    let depth = if o.gray_threshold.is_none()
        && selected
            .clone()
            .flatten()
            .any(|(_, g)| matches!(g.bitmap, GlyphBitmap::Gray8(_)))
    {
        8
    } else {
        1
    };
    let mut xmin = 0i64;
    let mut top = 0i64;
    let mut bottom = 0i64;
    let mut xmax = 0i64;
    let mut bit_width = 0usize;
    let mut max_advance = 0i64;
    for (gi, g) in selected.clone().flatten() {
        let bb = g.metrics.bounds;
        xmin = xmin.min(bb.x);
        xmax = xmax.max(bb.x + i64::from(bb.width));
        top = top.max(bb.y + i64::from(bb.height));
        bottom = bottom.min(bb.y);
        bit_width = bit_width
            .checked_add(bb.width as usize)
            .ok_or_else(|| invalid("strike width overflow"))?;
        let a = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .map_or(i64::from(bb.width), |a| a.x);
        if !(0..=255).contains(&a) {
            return Err(invalid(format!("glyph {gi} advance outside 0..255")));
        }
        max_advance = max_advance.max(a);
    }
    if let Some(v) = f.metrics.ascent {
        top = top.max(v);
    }
    if let Some(v) = f.metrics.descent {
        bottom = bottom.min(v.checked_neg().ok_or_else(|| invalid("descent overflow"))?);
    }
    let height = top
        .checked_sub(bottom)
        .ok_or_else(|| invalid("height overflow"))?;
    let h = usize::from(word(height)?);
    let kern = i16::try_from(xmin).map_err(|_| invalid("left bearing exceeds int16"))?;
    if bit_width > 65535 {
        return Err(invalid("NFNT horizontal strike exceeds 65535 pixels"));
    }
    let row = align(bit_width * depth, 16)? / 8;
    let count = last - first + 2;
    let bitmap_len = row
        .checked_mul(h)
        .ok_or_else(|| invalid("strike overflow"))?;
    check(bitmap_len, o.limits.max_table_bytes, "NFNT output strike")?;
    let loc = 26 + bitmap_len;
    let ow = loc + (count + 1) * 2;
    let len = ow + (count + 1) * 2;
    check(len, o.limits.max_resource_bytes, "NFNT output resource")?;
    check(len, o.limits.max_table_bytes, "NFNT output table")?;
    check(len, o.max_output_bytes, "NFNT output bytes")?;
    budget.add(len)?;
    let mut out = vec![0u8; len];
    let kind = 0x9000
        | if depth == 8 { 12 } else { 0 }
        | if matches!(
            f.metrics.spacing,
            Some(Spacing::Monospaced | Spacing::CharacterCell)
        ) {
            0x2000
        } else {
            0
        };
    put16(&mut out, 0, kind);
    put16(&mut out, 2, first as u16);
    put16(&mut out, 4, last as u16);
    put16(&mut out, 6, word(max_advance)?);
    put16(&mut out, 8, kern as u16);
    let ow_word = (ow - 16) / 2;
    let high = ow_word >> 16;
    if high > i16::MAX as usize {
        return Err(invalid(
            "extended width-table offset exceeds signed high word",
        ));
    }
    put16(
        &mut out,
        10,
        if high > 0 {
            high as u16
        } else {
            (i16::try_from(bottom).map_err(|_| invalid("negative descent outside int16"))?) as u16
        },
    );
    put16(
        &mut out,
        12,
        word(
            xmax.checked_sub(xmin)
                .ok_or_else(|| invalid("font horizontal extent overflow"))?,
        )?,
    );
    put16(&mut out, 14, h as u16);
    put16(&mut out, 16, ow_word as u16);
    put16(&mut out, 18, word(top)?);
    put16(&mut out, 20, word(-bottom)?);
    put16(&mut out, 22, word(f.metrics.line_gap.unwrap_or(0))?);
    put16(&mut out, 24, (row / 2) as u16);
    let mut x = 0usize;
    for (i, entry) in selected.enumerate() {
        put16(&mut out, loc + i * 2, x as u16);
        if let Some((gi, g)) = entry {
            let advance = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .map_or(i64::from(g.bitmap.width()), |a| a.x);
            let offset = g.metrics.bounds.x - xmin;
            if !(0..=255).contains(&offset) || (offset == 255 && advance == 255) {
                return Err(invalid(
                    "glyph bearing/advance cannot fit width-offset word",
                ));
            }
            put16(
                &mut out,
                ow + i * 2,
                ((offset as u16) << 8) | advance as u16,
            );
            check(
                g.bitmap.width() as usize * h,
                o.limits.max_pixels_per_glyph,
                "NFNT output glyph pixels",
            )?;
            let y = usize::try_from(top - g.metrics.bounds.y - i64::from(g.bitmap.height()))
                .map_err(|_| invalid("glyph top offset"))?;
            if matches!(g.bitmap, GlyphBitmap::Gray8(_)) && depth == 1 {
                budget.diagnostic(
                    r,
                    "gray_threshold",
                    Some(fi),
                    Some(gi),
                    None,
                    "Gray8 reduced with the explicit threshold",
                )?;
            }
            for gy in 0..g.bitmap.height() as usize {
                let row_start = 26 + (y + gy) * row;
                match (&g.bitmap, depth) {
                    (GlyphBitmap::Mono(b), 1) => {
                        let shift = x % 8;
                        for (bi, &value) in b.as_bytes()[gy * b.stride()..(gy + 1) * b.stride()]
                            .iter()
                            .enumerate()
                        {
                            let at = row_start + x / 8 + bi;
                            out[at] |= value >> shift;
                            if shift != 0 && at + 1 < row_start + row {
                                out[at + 1] |= value << (8 - shift);
                            }
                        }
                    }
                    (GlyphBitmap::Gray8(b), 8) => out
                        [row_start + x..row_start + x + b.width() as usize]
                        .copy_from_slice(&b.as_bytes()[gy * b.stride()..(gy + 1) * b.stride()]),
                    _ => {
                        for gx in 0..g.bitmap.width() as usize {
                            let sample = match &g.bitmap {
                                GlyphBitmap::Mono(b) => {
                                    if b.as_bytes()[gy * b.stride() + gx / 8] & (0x80 >> (gx % 8))
                                        != 0
                                    {
                                        255
                                    } else {
                                        0
                                    }
                                }
                                GlyphBitmap::Gray8(b) => b.as_bytes()[gy * b.stride() + gx],
                            };
                            if depth == 8 {
                                out[row_start + x + gx] = sample;
                            } else if sample >= o.gray_threshold.unwrap_or(128) {
                                let bit = x + gx;
                                out[row_start + bit / 8] |= 0x80 >> (bit % 8);
                            }
                        }
                    }
                }
            }
            x += g.bitmap.width() as usize;
        } else {
            put16(&mut out, ow + i * 2, 0xffff);
        }
    }
    put16(&mut out, loc + count * 2, x as u16);
    put16(&mut out, ow + count * 2, 0xffff);
    Ok(out)
}
struct OwnedResource {
    metadata: Vec<u8>,
    data: Vec<u8>,
}
fn owned(resource: Resource<'_>, budget: &mut Budget) -> Result<OwnedResource> {
    let metadata = resource.metadata();
    budget.add(metadata.len() + resource.data.len() + 256)?;
    Ok(OwnedResource {
        metadata,
        data: resource.data.to_vec(),
    })
}
fn encode_name(s: &str) -> Result<Vec<u8>> {
    let codec =
        crate::charset::CharsetCodec::new("macroman", None).map_err(|e| invalid(e.to_string()))?;
    Ok(s.chars()
        .take(255)
        .map(|c| {
            codec
                .unicode2cp(u32::from(c))
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(b'?')
        })
        .collect())
}

fn style_code(f: &Font) -> u16 {
    if let Some(s) = number(&f.properties, "nfnt.fond-style") {
        return s as u16;
    }
    let s = f.names.style.as_deref().unwrap_or("").to_ascii_lowercase();
    let names = [
        "bold",
        "italic",
        "underline",
        "outline",
        "shadow",
        "condensed",
        "extended",
    ];
    names
        .iter()
        .enumerate()
        .fold(0, |v, (i, n)| v | if s.contains(n) { 1 << i } else { 0 })
}
fn point(f: &Font) -> Result<u16> {
    if let Some(p) = f.metrics.point_size
        && p.numerator() % i64::from(p.denominator()) == 0
    {
        return word(p.numerator() / i64::from(p.denominator()));
    }
    word(i64::from(f.metrics.pixel_size.unwrap_or(12)))
}
fn encode_fork(
    set: &FontSet,
    o: &NfntWriteOptions,
    budget: &mut Budget,
    r: &mut Report,
) -> Result<Vec<u8>> {
    let mut res = Vec::new();
    let mut ids = BTreeSet::new();
    for p in set.properties.iter().filter(|p| p.name == "nfnt.resource") {
        let PropertyValue::Bytes(raw) = &p.value else {
            return Err(invalid("nfnt.resource must be bytes"));
        };
        let resource = container::unpack_record(raw)?;
        if resource.kind == *b"NFNT" || (resource.kind == *b"FONT" && resource.id & 127 != 0) {
            return Err(invalid("opaque bitmap duplicates modeled resource"));
        }
        if !ids.insert((resource.kind, resource.id)) {
            return Err(invalid("duplicate retained resource identity"));
        }
        res.push(owned(resource, budget)?);
    }
    for (fi, f) in set.fonts.iter().enumerate() {
        let palette = if let Some(raw) = bytes(&f.properties, "nfnt.source")
            .filter(|raw| u16be(raw, 0).is_ok_and(|k| k & 0x280 != 0))
        {
            let kind = u16be(raw, 0)?;
            let id = bytes(&f.properties, "nfnt.resource").and_then(|r| u16be(r, 4).ok());
            let data = if kind & 0x80 != 0 {
                res.iter()
                    .find(|r| r.metadata[..4] == *b"fctb" && u16be(&r.metadata, 4).ok() == id)
                    .map(|r| &r.data[..])
            } else {
                None
            };
            Some(super::palette::Palette::parse(
                data,
                1usize << ((kind >> 2) & 3),
                &o.limits,
            )?)
        } else {
            None
        };
        let payload = encode_font(f, o, budget, r, fi, palette.as_ref())?;
        let association_style = (style_code(f) & !0x300) | ((u16be(&payload, 0)? & 12) << 6);
        let default_kind = match o.resource_type {
            ResourceType::Nfnt => *b"NFNT",
            ResourceType::Font => *b"FONT",
        };
        let mut classic_family = None;
        let mut metadata = if let Some(m) = bytes(&f.properties, "nfnt.resource") {
            let resource = container::unpack_record(m)?;
            if resource.kind != *b"NFNT" && resource.kind != *b"FONT" {
                return Err(invalid("bitmap resource has incompatible type"));
            }
            if !resource.data.is_empty() {
                return Err(invalid("bitmap resource metadata has trailing bytes"));
            }
            resource.metadata()
        } else {
            let id = if default_kind == *b"FONT" {
                let size = point(f)?;
                if !(1..=127).contains(&size) {
                    return Err(invalid("classic FONT point size must be 1..127"));
                }
                let family = (128..=255u16)
                    .find(|family| {
                        !ids.contains(&(*b"FOND", *family))
                            && !ids
                                .iter()
                                .any(|(kind, id)| *kind == *b"FONT" && id >> 7 == *family)
                    })
                    .ok_or_else(|| invalid("classic FONT family IDs exhausted"))?;
                classic_family = Some(family);
                family * 128 + size
            } else {
                (128..=65535u16)
                    .find(|id| !ids.contains(&(default_kind, *id)))
                    .ok_or_else(|| invalid("resource IDs exhausted"))?
            };
            let name = f.names.family.as_deref().map(encode_name).transpose()?;
            Resource {
                kind: default_kind,
                id,
                attr: 0,
                handle: [0; 4],
                name: name.as_deref(),
                data: &[],
            }
            .metadata()
        };
        let resource = container::unpack_record(&metadata)?;
        let id = resource.id;
        let kind = resource.kind;
        if !ids.insert((kind, id)) {
            return Err(invalid("duplicate bitmap resource identity"));
        }
        if let Some(name) = f
            .names
            .family
            .as_ref()
            .filter(|_| number(&f.properties, "nfnt.fond-id").is_none())
            .filter(|_| number(&f.properties, "nfnt.font-family-id").is_none())
        {
            let name = encode_name(name)?;
            metadata = Resource {
                kind,
                id,
                attr: resource.attr,
                handle: resource.handle,
                name: Some(&name),
                data: &[],
            }
            .metadata();
        }
        budget.add(metadata.len() + 256)?;
        res.push(OwnedResource {
            metadata,
            data: payload,
        });
        if let Some(family) = classic_family
            && let Some(name) = &f.names.family
        {
            let name = encode_name(name)?;
            let metadata = Resource {
                kind: *b"FONT",
                id: family * 128,
                attr: 0,
                handle: [0; 4],
                name: Some(&name),
                data: &[],
            }
            .metadata();
            ids.insert((*b"FONT", family * 128));
            budget.add(metadata.len() + 256)?;
            res.push(OwnedResource {
                metadata,
                data: Vec::new(),
            });
        }
        if let Some(fond_id) = number(&f.properties, "nfnt.fond-id")
            && let Some(fond) = res.iter_mut().find(|res| {
                res.metadata[..4] == *b"FOND"
                    && u16be(&res.metadata, 4).ok() == u16::try_from(fond_id).ok()
            })
        {
            let n = usize::from(u16be(&fond.data, 52)?.wrapping_add(1));
            slice(&fond.data, 54, n * 6)?;
            for i in 0..n {
                let at = 54 + i * 6;
                if u16be(&fond.data, at + 4)? == id {
                    put16(&mut fond.data, at, point(f)?);
                    put16(&mut fond.data, at + 2, association_style);
                }
            }
            if let Some(name) = &f.names.family {
                let name = encode_name(name)?;
                let old = container::unpack_record(&fond.metadata)?;
                fond.metadata = Resource {
                    kind: *b"FOND",
                    id: old.id,
                    attr: old.attr,
                    handle: old.handle,
                    name: Some(&name),
                    data: &[],
                }
                .metadata();
            }
            continue;
        }
        // Existing standalone bitmap resources need no invented family/style.
        if bytes(&f.properties, "nfnt.resource").is_some()
            && f.names.style.is_none()
            && let Some(raw) = bytes(&f.properties, "nfnt.source")
        {
            let nominal = if kind == *b"FONT" {
                i64::from(id & 127)
            } else {
                i64::from(u16be(raw, 18)?) + i64::from(u16be(raw, 20)?)
            };
            if f.metrics.point_size == Some(crate::model::Rational::integer(nominal))
                && f.metrics.pixel_size == Some(nominal as u32)
            {
                continue;
            }
        }
        let fid = classic_family
            .or_else(|| (128..=65535u16).find(|id| !ids.contains(&(*b"FOND", *id))))
            .ok_or_else(|| invalid("FOND IDs exhausted"))?;
        ids.insert((*b"FOND", fid));
        let name = f.names.family.as_deref().map(encode_name).transpose()?;
        let metadata = Resource {
            kind: *b"FOND",
            id: fid,
            attr: 0,
            handle: [0; 4],
            name: name.as_deref(),
            data: &[],
        }
        .metadata();
        let mut data = vec![0u8; 60];
        put16(&mut data, 0, 0x4000);
        put16(&mut data, 2, fid);
        put16(&mut data, 6, 255);
        put16(&mut data, 50, 2);
        put16(&mut data, 54, point(f)?);
        put16(&mut data, 56, association_style);
        put16(&mut data, 58, id);
        budget.add(data.len() + metadata.len() + 256)?;
        res.push(OwnedResource { metadata, data });
    }
    check(res.len(), o.limits.max_resources, "NFNT output resources")?;
    // Keep bitmap resource encounter order while grouping type-list entries.
    let mut types: BTreeMap<[u8; 4], Vec<usize>> = BTreeMap::new();
    for (i, r) in res.iter().enumerate() {
        let tag = r.metadata[..4]
            .try_into()
            .map_err(|_| invalid("resource type"))?;
        types.entry(tag).or_default().push(i);
    }
    if let Some(indexes) = types.get_mut(b"FONT") {
        // Name-only FONT records precede strikes, preserving strike encounter order.
        indexes.sort_by_key(|&i| {
            let id = u16::from_be_bytes([res[i].metadata[4], res[i].metadata[5]]);
            (
                id & 127 != 0,
                if id & 127 == 0 { usize::from(id) } else { i },
            )
        });
    }
    let mut data_len = 0usize;
    let mut names_len = 0usize;
    for r in &res {
        check(
            r.data.len(),
            o.limits.max_resource_bytes,
            "NFNT output resource",
        )?;
        data_len = data_len
            .checked_add(4 + r.data.len())
            .ok_or_else(|| invalid("resource data overflow"))?;
        let rr = container::unpack_record(&r.metadata)?;
        if let Some(name) = rr.name {
            names_len += 1 + name.len();
        }
    }
    let name_at = 28 + 2 + types.len() * 8 + res.len() * 12;
    let map_len = name_at + names_len;
    check(map_len, o.limits.max_table_bytes, "NFNT output map")?;
    if name_at > 65535 || names_len > 65535 {
        return Err(invalid("resource map offsets exceed 16 bits"));
    }
    budget.add(res.len().saturating_mul(16))?;
    let mut data_order = (0..res.len()).collect::<Vec<_>>();
    data_order.sort_by(|&a, &b| {
        res[a]
            .data
            .len()
            .cmp(&res[b].data.len())
            .then_with(|| res[a].metadata[..6].cmp(&res[b].metadata[..6]))
    });
    let mut data_offsets = vec![0usize; res.len()];
    let mut cursor = 0;
    for &i in &data_order {
        if cursor > 0xffffff {
            return Err(invalid("resource data offsets exceed 24 bits"));
        }
        data_offsets[i] = cursor;
        cursor += 4 + res[i].data.len();
    }

    let map_at = 256 + data_len;
    let len = map_at + map_len;
    check(len, o.max_output_bytes, "NFNT output bytes")?;
    check(len, o.limits.max_input_bytes, "NFNT output readable input")?;
    budget.add(len)?;
    let mut out = vec![0u8; len];
    put32(&mut out, 0, 256);
    put32(&mut out, 4, size(map_at)?);
    put32(&mut out, 8, size(data_len)?);
    put32(&mut out, 12, size(map_len)?);
    let header: [u8; 16] = out[..16].try_into().map_err(|_| invalid("fork header"))?;
    out[map_at..map_at + 16].copy_from_slice(&header);
    if let Some(fields) = bytes(&set.properties, "nfnt.map-fields") {
        if fields.len() != 8 {
            return Err(invalid("nfnt.map-fields length"));
        }
        out[map_at + 16..map_at + 24].copy_from_slice(fields);
    }
    put16(&mut out, map_at + 24, 28);
    put16(&mut out, map_at + 26, name_at as u16);
    put16(&mut out, map_at + 28, (types.len() - 1) as u16);
    let mut ref_at = 30 + types.len() * 8;
    let mut name_offset = 0;
    for (ti, (kind, indexes)) in types.into_iter().enumerate() {
        let at = map_at + 30 + ti * 8;
        out[at..at + 4].copy_from_slice(&kind);
        put16(&mut out, at + 4, (indexes.len() - 1) as u16);
        put16(&mut out, at + 6, (ref_at - 28) as u16);
        for index in indexes {
            let data_at = data_offsets[index];
            let r = &res[index];
            let rr = container::unpack_record(&r.metadata)?;
            let at = map_at + ref_at;
            put16(&mut out, at, rr.id);
            put16(
                &mut out,
                at + 2,
                if rr.name.is_some() {
                    name_offset as u16
                } else {
                    0xffff
                },
            );
            put32(
                &mut out,
                at + 4,
                (u32::from(rr.attr) << 24) | data_at as u32,
            );
            out[at + 8..at + 12].copy_from_slice(&rr.handle);
            if let Some(name) = rr.name {
                let at = map_at + name_at + name_offset;
                out[at] = name.len() as u8;
                out[at + 1..at + 1 + name.len()].copy_from_slice(name);
                name_offset += 1 + name.len();
            }
            put32(&mut out, 256 + data_at, size(r.data.len())?);
            out[260 + data_at..260 + data_at + r.data.len()].copy_from_slice(&r.data);
            ref_at += 12;
        }
    }
    Ok(out)
}
fn wrap(
    fork: Vec<u8>,
    set: &FontSet,
    o: &NfntWriteOptions,
    path: &ResourcePath,
    budget: &mut Budget,
) -> Result<OutputBundle> {
    let data = bytes(&set.properties, "nfnt.data-fork").unwrap_or(&[]);
    check(
        data.len(),
        o.limits.max_resource_bytes,
        "NFNT output data fork",
    )?;
    match o.container {
        Container::Bare => unreachable!("handled before fork encoding"),
        Container::ResourceFork | Container::Dfont | Container::Suitcase => {
            Ok(OutputBundle::single(path.clone(), fork))
        }
        Container::MacBinary => {
            let data_font = number(&set.properties, "nfnt.macbinary-font-fork") == Some(1);
            let (data, fork) = if data_font {
                (&fork[..], &[][..])
            } else {
                (data, &fork[..])
            };
            let secondary = bytes(&set.properties, "nfnt.macbinary-secondary").unwrap_or(&[]);
            let comment = bytes(&set.properties, "nfnt.macbinary-comment").unwrap_or(&[]);
            if secondary.len() > 65535 || comment.len() > 65535 {
                return Err(invalid("MacBinary secondary/comment too large"));
            }
            let data_at = 128 + align(secondary.len(), 128)?;
            let fork_at = data_at + align(data.len(), 128)?;
            let comment_at = fork_at + align(fork.len(), 128)?;
            let len = comment_at + comment.len();
            budget.add(len)?;
            check(len, o.max_output_bytes, "NFNT output bytes")?;
            check(
                len,
                o.limits.max_input_bytes,
                "NFNT output readable envelope",
            )?;
            let mut out = vec![0; len];
            if let Some(h) = bytes(&set.properties, "nfnt.macbinary-header") {
                if h.len() != 128 {
                    return Err(invalid("MacBinary header metadata length"));
                }
                out[..128].copy_from_slice(h);
            } else {
                let name = path.as_str().rsplit('/').next().unwrap_or("font");
                let name = encode_name(name)?;
                let n = name.len().min(63);
                out[1] = n as u8;
                out[2..2 + n].copy_from_slice(&name[..n]);
                out[65..69].copy_from_slice(b"FFIL");
                out[69..73].copy_from_slice(b"DMOV");
                out[122] = 129;
                out[123] = 129;
            }
            put32(&mut out, 83, size(data.len())?);
            put32(&mut out, 87, size(fork.len())?);
            put16(&mut out, 99, comment.len() as u16);
            put16(&mut out, 120, secondary.len() as u16);
            let crc = crc16(&out[..124]);
            if out[122] >= 129 {
                put16(&mut out, 124, crc);
            }
            out[128..128 + secondary.len()].copy_from_slice(secondary);
            out[data_at..data_at + data.len()].copy_from_slice(data);
            out[fork_at..fork_at + fork.len()].copy_from_slice(fork);
            out[comment_at..].copy_from_slice(comment);
            Ok(OutputBundle::single(path.clone(), out))
        }
        Container::AppleSingle | Container::AppleDouble => {
            let mut entries: Vec<(u32, &[u8])> = Vec::new();
            let double = o.container == Container::AppleDouble;
            if !double && number(&set.properties, "nfnt.apple-font-fork") == Some(1) {
                entries.push((1, &fork));
            } else {
                if !double && !data.is_empty() {
                    entries.push((1, data));
                }
                entries.push((2, &fork));
            }
            for p in set.properties.iter() {
                if let Some(id) = p.name.strip_prefix("nfnt.apple-entry.") {
                    let id = id
                        .parse::<u32>()
                        .map_err(|_| invalid("Apple entry property ID"))?;
                    if id == 1 || id == 2 {
                        return Err(invalid("Apple entry reserved ID"));
                    }
                    let PropertyValue::Bytes(b) = &p.value else {
                        return Err(invalid("Apple entry property must be bytes"));
                    };
                    entries.push((id, b));
                }
            }
            entries.sort_by_key(|v| v.0);
            if entries.windows(2).any(|w| w[0].0 == w[1].0) {
                return Err(invalid("duplicate Apple entry property"));
            }
            check(
                entries.len(),
                o.limits.max_resources,
                "NFNT Apple output entries",
            )?;
            if entries.len() > 65535 {
                return Err(invalid("Apple entry count exceeds 16 bits"));
            }
            let header_len = 26 + entries.len() * 12;
            let len = entries.iter().try_fold(header_len, |n, (_, b)| {
                n.checked_add(b.len())
                    .ok_or_else(|| invalid("Apple output length overflow"))
            })?;
            check(len, o.max_output_bytes, "NFNT output bytes")?;
            check(
                len,
                o.limits.max_input_bytes,
                "NFNT output readable envelope",
            )?;
            budget.add(len)?;
            let mut out = vec![0u8; len];
            if let Some(h) = bytes(&set.properties, "nfnt.apple-header") {
                if h.len() != 24 {
                    return Err(invalid("Apple header metadata length"));
                }
                out[..24].copy_from_slice(h);
            } else {
                put32(&mut out, 4, 0x20000);
            }
            put32(&mut out, 0, if double { 0x51607 } else { 0x51600 });
            put16(&mut out, 24, entries.len() as u16);
            let mut at = header_len;
            for (i, (id, b)) in entries.into_iter().enumerate() {
                check(
                    b.len(),
                    o.limits.max_resource_bytes,
                    "NFNT Apple output entry bytes",
                )?;
                put32(&mut out, 26 + i * 12, id);
                put32(&mut out, 30 + i * 12, size(at)?);
                put32(&mut out, 34 + i * 12, size(b.len())?);
                out[at..at + b.len()].copy_from_slice(b);
                at += b.len();
            }
            if double {
                check(
                    out.len(),
                    o.limits.max_resource_bytes,
                    "NFNT output companion bytes",
                )?;
                check(
                    data.len(),
                    o.limits.max_input_bytes,
                    "NFNT output primary data",
                )?;
                let (prefix, name) = path
                    .as_str()
                    .rsplit_once('/')
                    .map_or(("", path.as_str()), |(p, n)| (p, n));
                let companion = ResourcePath::new(if prefix.is_empty() {
                    format!("._{name}")
                } else {
                    format!("{prefix}/._{name}")
                })?;
                budget.add(data.len())?;
                OutputBundle::new(
                    path.clone(),
                    vec![
                        OutputAsset {
                            path: path.clone(),
                            bytes: data.to_vec(),
                        },
                        OutputAsset {
                            path: companion,
                            bytes: out,
                        },
                    ],
                )
            } else {
                Ok(OutputBundle::single(path.clone(), out))
            }
        }
    }
}
fn compare_props(
    a: &Properties,
    b: &Properties,
    budget: &mut Budget,
    r: &mut Report,
    fi: Option<usize>,
    gi: Option<usize>,
    scope: &str,
) -> Result<()> {
    let mut other = b.iter();
    for (i, p) in a.iter().enumerate() {
        if other.next() != Some(p) {
            budget.diagnostic(
                r,
                "property",
                fi,
                gi,
                None,
                format!(
                    "{scope} property {:.128} at index {i} changes or is omitted",
                    p.name
                ),
            )?;
        }
    }
    for p in other {
        budget.diagnostic(
            r,
            "property_added",
            fi,
            gi,
            None,
            format!("{scope} property {:.128} is added during encoding", p.name),
        )?;
    }
    Ok(())
}
fn compare_sets(a: &FontSet, b: &FontSet, budget: &mut Budget, r: &mut Report) -> Result<()> {
    if a.source_format != b.source_format {
        budget.diagnostic(
            r,
            "source_format",
            None,
            None,
            None,
            "Container source format attribution changes",
        )?;
    }
    compare_props(
        &a.properties,
        &b.properties,
        budget,
        r,
        None,
        None,
        "Container",
    )?;
    if a.fonts.len() != b.fonts.len() {
        budget.diagnostic(r, "font_count", None, None, None, "Font count changes")?;
    }
    for (fi, (a, b)) in a.fonts.iter().zip(&b.fonts).enumerate() {
        if a.source_format != b.source_format {
            budget.diagnostic(
                r,
                "source_format",
                Some(fi),
                None,
                None,
                "Font source format attribution changes",
            )?;
        }
        if a.slot_count() != b.slot_count() {
            budget.diagnostic(
                r,
                "glyph_slots",
                Some(fi),
                None,
                None,
                "Stable glyph slot count changes",
            )?;
        }
        macro_rules! field {
            ($field:ident) => {
                if a.metrics.$field != b.metrics.$field {
                    budget.diagnostic(
                        r,
                        concat!("metrics.", stringify!($field)),
                        Some(fi),
                        None,
                        None,
                        concat!("Font metric ", stringify!($field), " changes"),
                    )?;
                }
            };
        }
        field!(pixel_size);
        field!(ppem_x);
        field!(ppem_y);
        field!(point_size);
        field!(resolution);
        field!(ascent);
        field!(descent);
        field!(bounds);
        field!(line_gap);
        field!(line_height);
        field!(underline_position);
        field!(underline_thickness);
        field!(default_glyph);
        field!(default_character);
        field!(space_width);
        field!(spacing);
        field!(glyph_defaults);
        field!(metrics_set);
        macro_rules! name {
            ($field:ident) => {
                if a.names.$field != b.names.$field {
                    budget.diagnostic(
                        r,
                        concat!("names.", stringify!($field)),
                        Some(fi),
                        None,
                        None,
                        concat!("Font name ", stringify!($field), " changes"),
                    )?;
                }
            };
        }
        name!(family);
        name!(style);
        name!(full);
        name!(postscript);
        name!(version);
        name!(copyright);
        name!(license);
        name!(license_url);
        name!(foundry);
        name!(xlfd);
        name!(xlfd_fields);
        name!(localized);
        if a.charset != b.charset {
            budget.diagnostic(
                r,
                "charset",
                Some(fi),
                None,
                None,
                "Charset metadata changes; NFNT bytes do not declare an arbitrary encoding",
            )?;
        }
        compare_props(
            &a.headers,
            &b.headers,
            budget,
            r,
            Some(fi),
            None,
            "Font header",
        )?;
        compare_props(
            &a.properties,
            &b.properties,
            budget,
            r,
            Some(fi),
            None,
            "Font",
        )?;
        if a.len() != b.len() {
            budget.diagnostic(
                r,
                "glyph_count",
                Some(fi),
                None,
                None,
                "Glyph count changes after encoding aliases, omissions or missing glyph",
            )?;
        }
        let empty = Glyph::default();
        for (id, g) in a.glyphs() {
            let target = g
                .codepoints
                .iter()
                .find_map(|&cp| b.glyph_id(cp))
                .or_else(|| {
                    if a.metrics.default_glyph == Some(id) {
                        b.metrics.default_glyph
                    } else {
                        None
                    }
                });
            let n = target.and_then(|id| b.glyph(id)).unwrap_or(&empty);
            let gi = Some(id.index());
            if target != Some(id) {
                budget.diagnostic(
                    r,
                    "glyph_order",
                    Some(fi),
                    gi,
                    None,
                    "Stable glyph slots change",
                )?;
            }
            if g.codepoints != n.codepoints {
                budget.diagnostic(r, "mapping", Some(fi), gi, None, "Glyph mappings change")?;
            }
            if g.unicode_sequences != n.unicode_sequences {
                budget.diagnostic(
                    r,
                    "unicode_sequences",
                    Some(fi),
                    gi,
                    None,
                    "Unicode sequences omitted",
                )?;
            }
            if g.name != n.name {
                budget.diagnostic(r, "glyph_name", Some(fi), gi, None, "Glyph name omitted")?;
            }
            macro_rules! gm {
                ($field:ident) => {
                    if g.metrics.$field != n.metrics.$field {
                        budget.diagnostic(
                            r,
                            concat!("glyph_metrics.", stringify!($field)),
                            Some(fi),
                            gi,
                            None,
                            concat!("Glyph metric ", stringify!($field), " changes"),
                        )?;
                    }
                };
            }
            gm!(bounds);
            gm!(advance);
            gm!(scalable_advance);
            gm!(vertical_advance);
            gm!(vertical_scalable_advance);
            gm!(vertical_origin);
            gm!(abc);
            gm!(left_bearing);
            gm!(right_bearing);
            gm!(image_offset);
            if g.bitmap != n.bitmap {
                budget.diagnostic(
                    r,
                    "bitmap_geometry",
                    Some(fi),
                    gi,
                    None,
                    "Bitmap geometry or coverage changes",
                )?;
            }
            compare_props(
                &g.properties,
                &n.properties,
                budget,
                r,
                Some(fi),
                gi,
                "Glyph",
            )?;
        }
    }
    Ok(())
}
