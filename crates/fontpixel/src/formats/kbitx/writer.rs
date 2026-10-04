use super::*;
use crate::bitmap::Bounds;
use crate::model::{
    FontCharset, FontMetrics, FontNames, GlyphBitmap, GlyphMetrics, PropertyValue, Vector,
};

struct Output {
    bytes: Vec<u8>,
    max: usize,
    max_attribute: usize,
}
impl Output {
    fn add(&mut self, s: &[u8]) -> Result<()> {
        let len = self
            .bytes
            .len()
            .checked_add(s.len())
            .ok_or_else(|| invalid("output length overflow"))?;
        bound(len, self.max, "KBITX output bytes")?;
        if len > self.bytes.capacity() {
            let capacity = len
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.max);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(invalid)?;
        }
        self.bytes.extend_from_slice(s);
        Ok(())
    }
    fn attr(&mut self, key: &str, value: &str) -> Result<()> {
        let escaped_bytes = value.chars().fold(0usize, |total, c| {
            total.saturating_add(match c {
                '&' => 5,
                '<' => 4,
                '"' => 6,
                '\n' | '\r' => 5,
                '\t' => 4,
                _ => c.len_utf8(),
            })
        });
        bound(
            escaped_bytes,
            self.max_attribute,
            "KBITX output attribute bytes",
        )?;
        self.add(b" ")?;
        self.add(key.as_bytes())?;
        self.add(b"=\"")?;
        let mut start = 0;
        for (index, c) in value.char_indices() {
            if !(matches!(c, '\t' | '\n' | '\r')
                || ('\u{20}'..='\u{d7ff}').contains(&c)
                || ('\u{e000}'..='\u{fffd}').contains(&c)
                || c >= '\u{10000}')
            {
                return Err(invalid("metadata contains forbidden XML character"));
            }
            let escaped = match c {
                '&' => Some("&amp;"),
                '<' => Some("&lt;"),
                '"' => Some("&quot;"),
                '\n' => Some("&#10;"),
                '\r' => Some("&#13;"),
                '\t' => Some("&#9;"),
                _ => None,
            };
            if let Some(escaped) = escaped {
                self.add(&value.as_bytes()[start..index])?;
                self.add(escaped.as_bytes())?;
                start = index + c.len_utf8();
            }
        }
        self.add(&value.as_bytes()[start..])?;
        self.add(b"\"")
    }
    fn scalar(&mut self, kind: &str, id: &str, value: &str) -> Result<()> {
        self.add(b"  <")?;
        self.add(kind.as_bytes())?;
        self.attr("id", id)?;
        self.attr("value", value)?;
        self.add(b"/>\n")
    }
}
fn known_names(n: &FontNames) -> [(&'static str, Option<&str>); 9] {
    [
        ("0", n.copyright.as_deref()),
        ("1", n.family.as_deref()),
        ("2", n.style.as_deref()),
        ("4", n.full.as_deref()),
        ("5", n.version.as_deref()),
        ("6", n.postscript.as_deref()),
        ("8", n.foundry.as_deref()),
        ("13", n.license.as_deref()),
        ("14", n.license_url.as_deref()),
    ]
}
fn metrics(f: &Font, report: &mut Report) -> Result<FontMetrics> {
    let a = f
        .metrics
        .ascent
        .or_else(|| reader::prop(f, "emAscent"))
        .or_else(|| reader::prop(f, "lineAscent"));
    let d = f
        .metrics
        .descent
        .or_else(|| reader::prop(f, "emDescent"))
        .or_else(|| reader::prop(f, "lineDescent"));
    let pixel = match (a, d) {
        (Some(a), Some(d)) => Some(
            u32::try_from(
                a.checked_add(d)
                    .ok_or_else(|| invalid("em size overflow"))?,
            )
            .map_err(invalid)?,
        ),
        _ => None,
    };
    let mut bounds: Option<Bounds> = None;
    for (_, g) in f.glyphs() {
        let v = g.metrics.bounds;
        if v.width == 0 || v.height == 0 {
            continue;
        }
        bounds = Some(match bounds {
            None => v,
            Some(b) => {
                let x = b.x.min(v.x);
                let y = b.y.min(v.y);
                let right = (b.x + i64::from(b.width)).max(v.x + i64::from(v.width));
                let top = (b.y + i64::from(b.height)).max(v.y + i64::from(v.height));
                Bounds::new(
                    u32::try_from(i128::from(right) - i128::from(x)).map_err(invalid)?,
                    u32::try_from(i128::from(top) - i128::from(y)).map_err(invalid)?,
                    x,
                    y,
                )
            }
        });
    }
    let expected = FontMetrics {
        ascent: a,
        descent: d,
        pixel_size: pixel,
        point_size: Some(crate::model::Rational::integer(i64::from(
            pixel.unwrap_or(0),
        ))),
        resolution: Some(Vector::new(72, 72)),
        line_gap: f.metrics.line_gap.or_else(|| reader::prop(f, "lineGap")),
        bounds,
        ..Default::default()
    };
    macro_rules! field {($($name:ident),*)=>{$(if f.metrics.$name!=expected.$name{diag(report,concat!("write.metric.",stringify!($name)),Impact::Loss,None,concat!("KBITX cannot preserve font metric ",stringify!($name)));})*};}
    field!(
        pixel_size,
        ppem_x,
        ppem_y,
        point_size,
        resolution,
        ascent,
        descent,
        bounds,
        line_gap,
        line_height,
        underline_position,
        underline_thickness,
        default_glyph,
        default_character,
        space_width,
        spacing,
        glyph_defaults,
        metrics_set
    );
    Ok(expected)
}
pub(super) fn write(f: &Font, o: &KbitxWriteOptions) -> Result<(Vec<u8>, Report)> {
    f.validate(&o.limits)?;
    let mut largest = 0usize;
    for (_, g) in f.glyphs() {
        checked_i32(i64::from(g.bitmap.width()))?;
        checked_i32(i64::from(g.bitmap.height()))?;
        let pixels = usize::try_from(u64::from(g.bitmap.width()) * u64::from(g.bitmap.height()))
            .map_err(invalid)?;
        largest = largest.max(pixels);
    }
    let scratch = largest.saturating_mul(5).saturating_add(4096);
    let records = f
        .len()
        .saturating_add(f.properties.len())
        .saturating_add(f.headers.len());
    let report_max = records.saturating_mul(4096).saturating_add(16384);
    let mut budget = Budget::new(scratch, o.limits.max_total_bytes)?;
    budget.add(report_max)?;
    let mut out = Output {
        bytes: Vec::new(),
        max_attribute: o.limits.max_table_bytes,
        max: o
            .max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(o.limits.max_total_bytes - budget.used),
    };
    let mut report = Report::default();
    let expected = metrics(f, &mut report)?;
    if f.slot_count() != f.len() {
        diag(
            &mut report,
            "write.slots",
            Impact::Loss,
            None,
            "Removed glyph slots are compacted",
        );
    }
    let charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    if f.charset != charset {
        diag(
            &mut report,
            "write.charset",
            Impact::Loss,
            None,
            "KBITX represents Unicode labels without source charset metadata",
        );
    }
    if f.names.xlfd.is_some() || !f.names.xlfd_fields.is_empty() || !f.names.localized.is_empty() {
        diag(
            &mut report,
            "write.names",
            Impact::Loss,
            None,
            "XLFD and localized source names have no standard KBITX field",
        );
    }
    if !f.headers.is_empty() {
        diag(
            &mut report,
            "write.headers",
            Impact::Loss,
            None,
            "Source headers have no KBITX field",
        );
    }
    out.add(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE kbits PUBLIC \"-//Kreative//DTD BitsNPicasBitmap 1.0//EN\" \"http://www.kreativekorp.com/dtd/kbitx.dtd\">\n<kbits>\n")?;
    let ascent_key =
        if reader::prop(f, "emAscent").is_some() || reader::prop(f, "lineAscent").is_none() {
            "emAscent"
        } else {
            "lineAscent"
        };
    let descent_key =
        if reader::prop(f, "emDescent").is_some() || reader::prop(f, "lineDescent").is_none() {
            "emDescent"
        } else {
            "lineDescent"
        };
    let normalized = [
        (ascent_key, expected.ascent),
        (descent_key, expected.descent),
        ("lineGap", expected.line_gap),
    ];
    let mut last = [None; 3];
    for (index, p) in f.properties.iter().enumerate() {
        if let Some(id) = p.name.strip_prefix("kbitx.prop.") {
            for (k, (key, _)) in normalized.iter().enumerate() {
                if id == *key {
                    last[k] = Some(index);
                }
            }
        }
    }
    for (index, p) in f.properties.iter().enumerate() {
        if let Some(id) = p.name.strip_prefix("kbitx.prop.") {
            let PropertyValue::Integer(mut value) = p.value else {
                return Err(invalid("native prop must be an integer"));
            };
            for (k, (_, v)) in normalized.iter().enumerate() {
                if last[k] == Some(index)
                    && let Some(v) = v
                {
                    if value != *v {
                        diag(
                            &mut report,
                            "write.native_property",
                            Impact::Loss,
                            None,
                            "Normalized metric replaces an inconsistent retained native value",
                        );
                    }
                    value = *v;
                }
            }
            checked_i32(value)?;
            out.scalar("prop", id, &value.to_string())?;
        }
    }
    for (k, (key, value)) in normalized.iter().enumerate() {
        if last[k].is_none()
            && let Some(value) = value
        {
            checked_i32(*value)?;
            out.scalar("prop", key, &value.to_string())?;
        }
    }
    let names = known_names(&f.names);
    let mut last_name = [None; 9];
    for (key, value) in names {
        if value.is_none() && f.properties.get(&format!("kbitx.name.{key}")).is_some() {
            diag(
                &mut report,
                "write.name_presence",
                Impact::Loss,
                None,
                "Retained native name restores an absent normalized name",
            );
        }
    }
    for (index, p) in f.properties.iter().enumerate() {
        if let Some(id) = p.name.strip_prefix("kbitx.name.") {
            for (k, (key, _)) in names.iter().enumerate() {
                if id == *key {
                    last_name[k] = Some(index);
                }
            }
        }
    }
    for (index, p) in f.properties.iter().enumerate() {
        if let Some(id) = p.name.strip_prefix("kbitx.name.") {
            let canonical = checked_i32(id.parse().map_err(invalid)?)?.to_string();
            if canonical != id {
                return Err(invalid(
                    "native name property keys require canonical integer IDs",
                ));
            }
            let PropertyValue::Text(value) = &p.value else {
                return Err(invalid("native name must be text"));
            };
            let mut selected = value.as_str();
            for (k, (_, v)) in names.iter().enumerate() {
                if last_name[k] == Some(index)
                    && let Some(v) = v
                {
                    if selected != *v {
                        diag(
                            &mut report,
                            "write.native_name",
                            Impact::Loss,
                            None,
                            "Normalized name replaces an inconsistent retained native value",
                        );
                    }
                    selected = v;
                }
            }
            out.scalar("name", id, selected)?;
        }
    }
    for (k, (key, value)) in names.iter().enumerate() {
        if last_name[k].is_none()
            && let Some(value) = value
        {
            out.scalar("name", key, value)?;
        }
    }
    let mut record_count = 0usize;
    let mut stored_rasters = 0usize;
    for (id, g) in f.glyphs() {
        let index = Some(id.index());
        let cps = g
            .codepoints
            .iter()
            .filter_map(|cp| cp.scalar().map(u32::from));
        let count = cps.clone().count();
        if count == 0 && g.name.is_none() {
            diag(
                &mut report,
                "write.unencoded",
                Impact::Loss,
                index,
                "Glyph without Unicode label or name omitted",
            );
            continue;
        }
        if count > 1 {
            diag(
                &mut report,
                "write.aliases",
                Impact::Loss,
                index,
                "Unicode aliases become independent glyph records",
            );
        }
        if count != g.codepoints.len() {
            diag(
                &mut report,
                "write.codepoint",
                Impact::Loss,
                index,
                "Legacy or invalid Unicode labels omitted",
            );
        }
        if !g.unicode_sequences.is_empty() {
            diag(
                &mut report,
                "write.sequences",
                Impact::Loss,
                index,
                "Unicode sequences have no KBITX field",
            );
        }
        if !g.properties.is_empty() {
            diag(
                &mut report,
                "write.glyph_properties",
                Impact::Loss,
                index,
                "Source glyph properties have no KBITX field",
            );
        }
        let advance = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .unwrap_or(Vector::new(i64::from(g.bitmap.width()), 0));
        let target = GlyphMetrics {
            bounds: g.metrics.bounds,
            advance: Some(Vector::new(advance.x, 0)),
            ..Default::default()
        };
        if g.metrics != target {
            diag(
                &mut report,
                "write.glyph_metrics",
                Impact::Loss,
                index,
                "Only horizontal advance and bitmap bounds survive KBITX output",
            );
        }
        if let GlyphBitmap::Gray8(b) = &g.bitmap
            && b.as_bytes().iter().all(|v| *v == 0 || *v == 255)
        {
            diag(
                &mut report,
                "write.depth",
                Impact::Loss,
                index,
                "Binary Gray8 raster will normalize to Mono on read",
            );
        }
        let top = g
            .metrics
            .bounds
            .y
            .checked_add(i64::from(g.bitmap.height()))
            .ok_or_else(|| invalid("top bearing overflow"))?;
        checked_i32(g.metrics.bounds.x)?;
        checked_i32(top)?;
        checked_i32(advance.x)?;
        record_count = record_count.saturating_add(count.max(1));
        stored_rasters =
            stored_rasters.saturating_add(g.bitmap.as_bytes().len().saturating_mul(count.max(1)));
        bound(
            record_count,
            o.limits.max_glyphs,
            "KBITX output glyph count",
        )?;
        let bytes = wire::encode(&g.bitmap)?;
        bound(
            bytes.len(),
            o.limits.max_table_bytes,
            "KBITX bitmap attribute bytes",
        )?;
        for cp in cps.map(Some).chain((count == 0).then_some(None)) {
            out.add(b"  <g")?;
            if let Some(cp) = cp {
                out.attr("u", &cp.to_string())?;
            }
            if let Some(name) = &g.name {
                out.attr("n", name)?;
            }
            out.attr("x", &g.metrics.bounds.x.to_string())?;
            out.attr("y", &top.to_string())?;
            out.attr("w", &advance.x.to_string())?;
            out.add(b" d=\"")?;
            out.add(&bytes)?;
            out.add(b"\"/>\n")?;
        }
    }
    let mut auxiliary_records: [Vec<&str>; 3] = std::array::from_fn(|_| Vec::new());
    let mut last_group = 0;
    let mut reordered = false;
    for p in f.properties.iter() {
        let group;
        if p.name == "kbitx.element" {
            let PropertyValue::Text(value) = &p.value else {
                return Err(invalid("retained auxiliary XML must be text"));
            };
            let mut local = Budget::new(
                out.bytes.capacity().saturating_add(report_max),
                o.limits.max_total_bytes,
            )?;
            bound(
                value.len(),
                o.limits.max_table_bytes,
                "KBITX auxiliary bytes",
            )?;
            let doc = reader::document(value, &o.limits, &mut local)?;
            let root = doc.root_element();
            if !matches!(root.tag_name().name(), "k" | "feat" | "puaa")
                || value.trim_start().starts_with("<?")
                || value.contains("<!DOCTYPE")
            {
                return Err(invalid("retained XML must be one auxiliary element"));
            }
            reader::auxiliary(root)?;
            let index = match root.tag_name().name() {
                "k" => 0,
                "feat" => 1,
                _ => 2,
            };
            if index == 2 && !auxiliary_records[index].is_empty() {
                return Err(invalid("only one PUAA element is permitted"));
            }
            auxiliary_records[index].push(value.as_str());
            group = index + 2;
        } else if p.name.starts_with("kbitx.prop.") {
            group = 0;
        } else if p.name.starts_with("kbitx.name.") {
            group = 1;
        } else {
            group = last_group;
            diag(
                &mut report,
                "write.property",
                Impact::Loss,
                None,
                "Source property has no KBITX field",
            );
        }
        reordered |= group < last_group;
        last_group = group;
        if p.raw.is_some() {
            diag(
                &mut report,
                "write.raw_property",
                Impact::Loss,
                None,
                "Raw property spelling is not retained in regenerated XML",
            );
        }
    }
    if reordered {
        diag(
            &mut report,
            "write.property_order",
            Impact::Loss,
            None,
            "Native metadata reordered to prop/name/kerning/feature/PUAA schema order",
        );
    }
    for group in auxiliary_records {
        for value in group {
            out.add(b"  ")?;
            out.add(value.as_bytes())?;
            out.add(b"\n")?;
        }
    }
    out.add(b"</kbits>\n")?;
    // Allocation-free upper bound of the reader's DOM, model and one WIB decode.
    // A successful writer never returns an asset unreadable under these limits.
    let mut reload = Budget::new(
        out.bytes.len().saturating_add(2048),
        o.limits.max_total_bytes,
    )?;
    reader::preflight(
        std::str::from_utf8(&out.bytes).map_err(invalid)?,
        &o.limits,
        &mut reload,
    )?;
    reload.add(record_count.saturating_mul(2048))?;
    reload.add(f.properties.len().saturating_add(16).saturating_mul(1024))?;
    reload.add(out.bytes.len().saturating_mul(2))?;
    reload.add(stored_rasters)?;
    reload.temporary(largest.saturating_mul(4).saturating_add(64))?;
    Ok((out.bytes, report))
}
