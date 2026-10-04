use super::*;
use std::io::Cursor;
struct Page {
    id: i64,
    path: ResourcePath,
    chars: Vec<usize>,
}
pub(super) fn read(c: &mut ReadContext<'_>, o: &BmfontReadOptions) -> Result<(Font, Report)> {
    let l = c.limits().clone();
    let mut budget = Budget::new(c.retained_input_bytes(), &l)?;
    budget.add(4096)?;
    let records = wire::parse(c.bytes(), &l, &mut budget)?;
    let singleton = |tag: &str| -> Result<&Record> {
        let mut values = records.iter().filter(|v| v.tag == tag);
        let v = values
            .next()
            .ok_or_else(|| invalid(format!("missing {tag}")))?;
        if values.next().is_some() {
            return Err(invalid(format!("duplicate {tag}")));
        }
        Ok(v)
    };
    let info = singleton("info")?;
    let common = singleton("common")?;
    let base = common.num("base")?;
    let width = u32::try_from(common.num("scaleW")?).map_err(invalid)?;
    let height = u32::try_from(common.num("scaleH")?).map_err(invalid)?;
    if width == 0 || height == 0 {
        return Err(invalid("empty atlas dimensions"));
    }
    let unicode = info.number("unicode", 1)? != 0;
    let chars: Vec<_> = records.iter().filter(|r| r.tag == "char").collect();
    bound(chars.len(), l.max_glyphs, "BMFont glyphs")?;
    budget.add(chars.len().saturating_mul(1024))?;
    let mut pages = Vec::new();
    let mut page_index = BTreeMap::new();
    for r in records.iter().filter(|r| r.tag == "page") {
        let id = r.num("id")?;
        if id < 0 || page_index.insert(id, pages.len()).is_some() {
            return Err(invalid("invalid/duplicate page ID"));
        }
        let path = ResourcePath::new(r.text("file")?)?;
        pages.push(Page {
            id,
            path,
            chars: Vec::new(),
        });
        bound(pages.len(), l.max_resources, "BMFont pages")?;
    }
    if usize::try_from(common.num("pages")?).map_err(invalid)? != pages.len() {
        return Err(invalid("page count mismatch"));
    }
    for (i, r) in chars.iter().enumerate() {
        let p = *page_index
            .get(&r.num("page")?)
            .ok_or_else(|| invalid("glyph references absent page"))?;
        pages[p].chars.push(i);
        let area = r
            .num("width")?
            .checked_mul(r.num("height")?)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("negative or overflowing glyph area"))?;
        bound(area, l.max_pixels_per_glyph, "BMFont glyph pixels")?;
        budget.add(area.saturating_mul(2))?;
    }
    if let Some(count) = records.iter().find(|r| r.tag == "chars")
        && usize::try_from(count.num("count")?).map_err(invalid)? != chars.len()
    {
        return Err(invalid("char count mismatch"));
    }
    let kernings = records.iter().filter(|r| r.tag == "kerning").count();
    if let Some(count) = records.iter().find(|r| r.tag == "kernings")
        && usize::try_from(count.num("count")?).map_err(invalid)? != kernings
    {
        return Err(invalid("kerning count mismatch"));
    }
    let mut f = Font::default();
    f.source_format = Some("bmfont".into());
    let mut report = Report::default();
    f.names.family = Some(info.text("face")?.into());
    let size = info.num("size")?;
    f.metrics.pixel_size = Some(u32::try_from(size.unsigned_abs()).map_err(invalid)?);
    f.metrics.ascent = Some(base);
    f.metrics.line_height = Some(common.num("lineHeight")?);
    f.charset.name = Some(if unicode {
        "unicode".into()
    } else {
        info.text("charset")?.into()
    });
    for r in &records {
        match r.tag.as_str() {
            "info" | "common" | "page" | "kerning" => {
                if r.tag == "kerning" {
                    for k in ["first", "second"] {
                        u32::try_from(r.num(k)?).map_err(invalid)?;
                    }
                    r.num("amount")?;
                }
                f.properties.push(r.property());
            }
            "char" | "chars" | "kernings" => {}
            _ => {
                f.properties.push(r.property());
            }
        }
    }
    let mut glyphs = vec![None; chars.len()];
    for page in pages {
        let compressed = c.resource_limited(&page.path, budget.remaining())?;
        budget.add(compressed.len())?;
        let available = budget.remaining();
        if png_metadata(&compressed, &l)? {
            loss(
                &mut report,
                "bmfont",
                "png_metadata",
                0,
                None,
                format!("PNG metadata on page {} is omitted from coverage", page.id),
            );
        }

        let mut decoder = png::Decoder::new(Cursor::new(&compressed));
        decoder.set_ignore_text_chunk(true);
        decoder.set_ignore_iccp_chunk(true);
        decoder.set_limits(png::Limits {
            bytes: available.min(l.max_decompressed_bytes),
        });
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut png = decoder.read_info().map_err(invalid)?;
        if png.info().width != width || png.info().height != height {
            return Err(invalid("PNG dimensions disagree with common scale"));
        }
        if png.info().animation_control.is_some() {
            return Err(invalid("animated PNG pages are unsupported"));
        }
        let size = png
            .output_buffer_size()
            .ok_or_else(|| invalid("PNG buffer overflow"))?;
        bound(size, l.max_decompressed_bytes, "BMFont expanded PNG")?;
        bound(
            size.saturating_mul(3).saturating_add(65536),
            available,
            "BMFont PNG scratch",
        )?;
        let mut data = vec![0; size];
        let frame = png.next_frame(&mut data).map_err(invalid)?;
        png.finish().map_err(invalid)?;
        data.truncate(frame.buffer_size());
        let channels = match frame.color_type {
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            _ => return Err(invalid("unexpanded palette PNG")),
        };
        if frame.bit_depth != png::BitDepth::Eight {
            return Err(invalid("unexpected expanded PNG depth"));
        }
        if png.info().bit_depth == png::BitDepth::Sixteen {
            loss(
                &mut report,
                "bmfont",
                "png_depth",
                0,
                None,
                format!("Page {} reduced from 16-bit samples", page.id),
            );
        }
        for index in page.chars {
            let r = chars[index];
            let x = u32::try_from(r.num("x")?).map_err(invalid)?;
            let y = u32::try_from(r.num("y")?).map_err(invalid)?;
            let w = u32::try_from(r.num("width")?).map_err(invalid)?;
            let h = u32::try_from(r.num("height")?).map_err(invalid)?;
            if x.checked_add(w).is_none_or(|v| v > width)
                || y.checked_add(h).is_none_or(|v| v > height)
            {
                return Err(invalid("glyph rectangle outside page"));
            }
            let mask = u8::try_from(r.number("chnl", 15)?).map_err(invalid)?;
            if mask == 0 || mask > 15 {
                return Err(invalid("invalid channel mask"));
            }
            let mut coverage = Vec::with_capacity(w as usize * h as usize);
            let mut discarded_color = false;
            let mut outline = false;
            let single = mask.count_ones() == 1;
            let mut selected = [0usize; 4];
            let mut selected_count = 0;
            for (bit, ch, key) in [
                (8, 3, "alphaChnl"),
                (4, 0, "redChnl"),
                (2, 1, "greenChnl"),
                (1, 2, "blueChnl"),
            ] {
                let role = common.number(key, 0)?;
                if !(0..=4).contains(&role) {
                    return Err(invalid("invalid channel role"));
                }
                if mask & bit == 0 || ch == 3 && channels == 3 {
                    continue;
                }
                if single || role <= 2 {
                    selected[selected_count] = ch;
                    selected_count += 1;
                    outline |= role == 1 || role == 2;
                    if ch == 3 && !single {
                        break;
                    }
                }
            }
            if selected_count == 0 {
                return Err(invalid("selected channel unavailable or constant"));
            }
            for yy in y..y + h {
                let start = (yy as usize * width as usize + x as usize) * channels;
                for p in data[start..start + w as usize * channels].chunks_exact(channels) {
                    let rgba = match channels {
                        1 => [p[0], p[0], p[0], p[0]],
                        2 => [p[0], p[0], p[0], p[1]],
                        3 => [p[0], p[1], p[2], 255],
                        _ => [p[0], p[1], p[2], p[3]],
                    };
                    let mut sample = 0u8;
                    for &ch in &selected[..selected_count] {
                        sample = sample.max(rgba[ch]);
                    }
                    discarded_color |= channels >= 3 && (rgba[0] != rgba[1] || rgba[1] != rgba[2]);
                    coverage.push(sample);
                }
            }
            if discarded_color {
                loss(
                    &mut report,
                    "bmfont",
                    "color",
                    0,
                    Some(index),
                    "RGB color reduced to coverage",
                );
            }
            if outline {
                loss(
                    &mut report,
                    "bmfont",
                    "outline_channel",
                    0,
                    Some(index),
                    "Outline channel merged into coverage",
                );
            }
            let gray = Bitmap::<Gray8>::from_raw(w, h, &coverage, bl(&l))?;
            let bitmap: GlyphBitmap = if let Some(threshold) = o.gray_threshold {
                loss(
                    &mut report,
                    "bmfont",
                    "threshold",
                    0,
                    Some(index),
                    "Gray8 coverage converted to monochrome",
                );
                gray.to_mono(threshold)?.into()
            } else {
                gray.into()
            };
            let mut g = Glyph::new(bitmap);
            let xo = r.num("xoffset")?;
            let yo = r.num("yoffset")?;
            g.metrics.bounds.x = xo;
            g.metrics.bounds.y = base
                .checked_sub(yo)
                .and_then(|n| n.checked_sub(i64::from(h)))
                .ok_or_else(|| invalid("glyph y overflow"))?;
            g.metrics.image_offset = Some(Vector::new(xo, yo));
            g.metrics.advance = Some(Vector::new(r.num("xadvance")?, 0));
            let cp = r.num("id")?;
            if cp != -1 && cp != i64::from(u32::MAX) {
                let cp = u32::try_from(cp).map_err(invalid)?;
                g.codepoints.push(if unicode {
                    Codepoint::Unicode(cp)
                } else {
                    Codepoint::Legacy(cp)
                });
            }
            let mut native = r.clone();
            for key in ["id", "width", "height", "xoffset", "yoffset", "xadvance"] {
                native.fields.remove(key);
            }
            g.properties.push(native.property());
            glyphs[index] = Some(g);
        }
    }
    for g in glyphs {
        f.add_glyph(g.ok_or_else(|| invalid("missing decoded glyph"))?)?;
    }
    f.validate(&l)?;
    Ok((f, report))
}

fn png_metadata(b: &[u8], l: &ResourceLimits) -> Result<bool> {
    if !b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid("page is not PNG"));
    }
    let mut pos = 8usize;
    let mut metadata = false;
    let mut ended = false;
    while pos < b.len() {
        let head = b
            .get(pos..pos.saturating_add(8))
            .ok_or_else(|| invalid("truncated PNG chunk"))?;
        let n = u32::from_be_bytes(head[..4].try_into().map_err(invalid)?) as usize;
        bound(n, l.max_table_bytes, "BMFont PNG chunk")?;
        let end = pos
            .checked_add(12)
            .and_then(|p| p.checked_add(n))
            .filter(|&p| p <= b.len())
            .ok_or_else(|| invalid("PNG chunk extent"))?;
        let tag = &head[4..];
        metadata |= !matches!(tag, b"IHDR" | b"PLTE" | b"tRNS" | b"IDAT" | b"IEND");
        pos = end;
        if tag == b"IEND" {
            ended = true;
            break;
        }
    }
    if !ended || pos != b.len() {
        return Err(invalid("missing PNG end or trailing data"));
    }
    Ok(metadata)
}
