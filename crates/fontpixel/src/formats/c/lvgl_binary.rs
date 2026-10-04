//! LVGL's little-endian binary tables, projected through the native fmt_txt data model.
use super::embedded::*;
use super::*;
use std::fmt::Write as _;
fn u16le(s: &[u8], p: usize) -> Result<u16> {
    let v = s
        .get(p..p.saturating_add(2))
        .ok_or_else(|| invalid("truncated LVGL binary u16"))?;
    Ok(u16::from_le_bytes([v[0], v[1]]))
}
fn u32le(s: &[u8], p: usize) -> Result<u32> {
    let v = s
        .get(p..p.saturating_add(4))
        .ok_or_else(|| invalid("truncated LVGL binary u32"))?;
    Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}
fn section<'a>(s: &'a [u8], at: &mut usize, tag: &[u8; 4], l: &ResourceLimits) -> Result<&'a [u8]> {
    let len = u32le(s, *at)? as usize;
    if len < 8 || s.get(*at + 4..*at + 8) != Some(tag) {
        return Err(invalid("missing or invalid LVGL binary table"));
    }
    check(len, l.max_table_bytes, "LVGL binary table bytes")?;
    let end = at
        .checked_add(len)
        .ok_or_else(|| invalid("LVGL table length overflow"))?;
    let data = s
        .get(*at..end)
        .ok_or_else(|| invalid("LVGL table extends beyond input"))?;
    *at = end;
    Ok(data)
}
pub(super) fn detect(s: &[u8]) -> bool {
    if s.len() < 48 || s.get(4..8) != Some(b"head") || u32le(s, 8).ok() != Some(0) {
        return false;
    }
    let mut at = 0;
    let l = ResourceLimits::default();
    for tag in [b"head", b"cmap", b"loca", b"glyf"] {
        if section(s, &mut at, tag, &l).is_err() {
            return false;
        }
    }
    at == s.len()
        || (s.get(at + 4..at + 8) == Some(b"kern")
            && section(s, &mut at, b"kern", &l).is_ok()
            && at == s.len())
}
struct Bits<'a> {
    s: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn get(&mut self, n: u8) -> Result<u32> {
        if n > 32 || self.at.saturating_add(n as usize) > self.s.len().saturating_mul(8) {
            return Err(invalid("truncated LVGL glyph bitstream"));
        }
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | u32::from((self.s[self.at / 8] >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Ok(v)
    }
    fn signed(&mut self, n: u8) -> Result<i32> {
        if n == 0 {
            return Ok(0);
        }
        let v = self.get(n)?;
        Ok(if v & (1 << (n - 1)) != 0 {
            (i64::from(v) - (1i64 << n)) as i32
        } else {
            v as i32
        })
    }
}
fn emit_numbers(out: &mut String, ty: &str, name: &str, v: impl IntoIterator<Item = i64>) {
    write!(out, "static const {ty} {name}[]={{").expect("String write");
    for n in v {
        write!(out, "{n},").expect("String write");
    }
    out.push_str("};\n");
}
pub(super) fn read(s: &[u8], l: &ResourceLimits, retained: usize) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "LVGL binary input bytes")?;
    let mut b = Budget::new(retained, l)?;
    let mut at = 0;
    let h = section(s, &mut at, b"head", l)?;
    if h.len() < 48 || u32le(h, 8)? != 0 {
        return Err(invalid("unsupported LVGL binary header version"));
    }
    let count = u16le(h, 12)?;
    if ![3, 4].contains(&count) {
        return Err(invalid(
            "LVGL binary must have three or four additional tables",
        ));
    }
    let cmap = section(s, &mut at, b"cmap", l)?;
    let loca = section(s, &mut at, b"loca", l)?;
    let glyf = section(s, &mut at, b"glyf", l)?;
    let kern = if count == 4 {
        Some(section(s, &mut at, b"kern", l)?)
    } else {
        None
    };
    if at != s.len() {
        return Err(invalid("unexpected trailing LVGL binary tables"));
    }
    let (bpp, xy, wh, aw, compression) = (h[37], h[38], h[39], h[40], h[41]);
    if h[34] > 1
        || h[35] > 1
        || h[36] > 1
        || ![1, 2, 3, 4, 8].contains(&bpp)
        || xy > 16
        || wh > 16
        || aw > 32
        || compression > 2
        || h[42] != 0
    {
        return Err(invalid(
            "unsupported LVGL binary metric fields or subpixels",
        ));
    }
    let ng = u32le(loca, 8)? as usize;
    if ng == 0 {
        return Err(invalid("LVGL binary lacks reserved glyph"));
    }
    check(ng - 1, l.max_glyphs, "LVGL binary glyph count")?;
    let item = if h[34] == 0 { 2 } else { 4 };
    let needed = ng
        .checked_mul(item)
        .and_then(|v| v.checked_add(12))
        .ok_or_else(|| invalid("LVGL loca length overflow"))?;
    if needed > loca.len() {
        return Err(invalid("truncated LVGL loca entries"));
    }
    b.add(ng.saturating_mul(1024) + s.len().saturating_mul(16))?;
    let offsets = (0..ng)
        .map(|i| {
            if item == 2 {
                u16le(loca, 12 + i * 2).map(usize::from)
            } else {
                u32le(loca, 12 + i * 4).map(|v| v as usize)
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let mut bitmap = Vec::new();
    let mut records = String::new();
    for (i, &off) in offsets.iter().enumerate() {
        let end = offsets.get(i + 1).copied().unwrap_or(glyf.len());
        if off < 8 || end < off || end > glyf.len() {
            return Err(invalid("LVGL loca offsets invalid"));
        }
        let mut bits = Bits {
            s: &glyf[off..end],
            at: 0,
        };
        let mut adv = if aw == 0 {
            u32::from(u16le(h, 30)?)
        } else {
            bits.get(aw)?
        };
        if h[36] == 0 {
            adv = adv
                .checked_mul(16)
                .ok_or_else(|| invalid("LVGL advance overflow"))?;
        }
        let x = bits.signed(xy)?;
        let y = bits.signed(xy)?;
        let w = bits.get(wh)?;
        let ht = bits.get(wh)?;
        check(
            (w as usize).saturating_mul(ht as usize),
            l.max_pixels_per_glyph,
            "LVGL binary glyph pixels",
        )?;
        let start = bitmap.len();
        if i != 0 && w != 0 && ht != 0 {
            while bits.at < bits.s.len() * 8 {
                let n = (bits.s.len() * 8 - bits.at).min(8) as u8;
                bitmap.push((bits.get(n)? as u8) << (8 - n));
            }
        }
        let (adv, x, y, w, ht) = if i == 0 {
            (0, 0, 0, 0, 0)
        } else {
            (adv, x, y, w, ht)
        };
        writeln!(
            records,
            "{{.bitmap_index={start},.adv_w={adv},.box_w={w},.box_h={ht},.ofs_x={x},.ofs_y={y}}},"
        )
        .expect("String write");
    }
    let mut text = String::new();
    emit_bytes(&mut text, "font_bitmap", &bitmap);
    writeln!(
        text,
        "static const lv_font_fmt_txt_glyph_dsc_t font_glyphs[]={{\n{records}}};"
    )
    .expect("String write");
    let nc = u32le(cmap, 8)? as usize;
    if nc > 511 {
        return Err(invalid("LVGL cmap count exceeds 9-bit field"));
    }
    if 12 + nc * 16 > cmap.len() {
        return Err(invalid("truncated LVGL cmap headers"));
    }
    let mut cmaps = String::new();
    for i in 0..nc {
        let p = 12 + i * 16;
        let data = u32le(cmap, p)? as usize;
        let start = u32le(cmap, p + 4)?;
        let len = u16le(cmap, p + 8)?;
        let gid = u16le(cmap, p + 10)?;
        let entries = usize::from(u16le(cmap, p + 12)?);
        b.add(entries.saturating_mul(64) + 1024)?;
        let ty = cmap[p + 14];
        if ty > 3 {
            return Err(invalid("unknown LVGL binary cmap type"));
        }
        let mut unicode = "NULL".to_owned();
        let mut ofs = "NULL".to_owned();
        if ty != 2 {
            if data < 12 + nc * 16 || data > cmap.len() {
                return Err(invalid("LVGL cmap data overlaps headers"));
            }
            let size = if ty == 0 {
                entries
            } else if ty == 1 {
                entries * 4
            } else {
                entries * 2
            };
            if data + size > cmap.len() {
                return Err(invalid("LVGL cmap data outside table"));
            }
            if ty == 0 {
                if entries != usize::from(len) {
                    return Err(invalid("LVGL full cmap entries mismatch"));
                }
                ofs = format!("font_offsets_{i}");
                emit_numbers(
                    &mut text,
                    "uint8_t",
                    &ofs,
                    cmap[data..data + entries].iter().map(|&n| i64::from(n)),
                );
            } else {
                unicode = format!("font_unicode_{i}");
                let vals = (0..entries)
                    .map(|j| u16le(cmap, data + j * 2).map(i64::from))
                    .collect::<Result<Vec<_>>>()?;
                emit_numbers(&mut text, "uint16_t", &unicode, vals);
                if ty == 1 {
                    ofs = format!("font_offsets_{i}");
                    let vals = (0..entries)
                        .map(|j| u16le(cmap, data + entries * 2 + j * 2).map(i64::from))
                        .collect::<Result<Vec<_>>>()?;
                    emit_numbers(&mut text, "uint16_t", &ofs, vals);
                }
            }
        }
        writeln!(cmaps,"{{.range_start={start},.range_length={len},.glyph_id_start={gid},.unicode_list={unicode},.glyph_id_ofs_list={ofs},.list_length={entries},.type={ty}}},").expect("String write");
    }
    writeln!(
        text,
        "static const lv_font_fmt_txt_cmap_t font_cmaps[]={{\n{cmaps}}};"
    )
    .expect("String write");
    let mut kern_ref = "NULL";
    let mut classes = 0;
    if let Some(k) = kern {
        let kind = *k
            .get(8)
            .ok_or_else(|| invalid("truncated LVGL kern type"))?;
        kern_ref = "&font_kern";
        if kind == 0 {
            let n = u32le(k, 12)? as usize;
            let step = if h[35] == 0 { 1 } else { 2 };
            let size = n
                .checked_mul(step * 2 + 1)
                .and_then(|v| v.checked_add(16))
                .ok_or_else(|| invalid("LVGL kern length overflow"))?;
            if size > k.len() {
                return Err(invalid("truncated LVGL kern pairs"));
            }
            let ids = (0..n * 2)
                .map(|j| {
                    if step == 1 {
                        Ok(i64::from(k[16 + j]))
                    } else {
                        u16le(k, 16 + j * 2).map(i64::from)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            emit_numbers(
                &mut text,
                if step == 1 { "uint8_t" } else { "uint16_t" },
                "font_kern_ids",
                ids,
            );
            emit_numbers(
                &mut text,
                "int8_t",
                "font_kern_values",
                k[16 + n * step * 2..size]
                    .iter()
                    .map(|&v| i64::from(v as i8)),
            );
            writeln!(text,"static const lv_font_fmt_txt_kern_pair_t font_kern={{.glyph_ids=font_kern_ids,.values=font_kern_values,.pair_cnt={n},.glyph_ids_size={}}};",h[35]).expect("String write");
        } else if kind == 3 {
            classes = 1;
            let n = usize::from(u16le(k, 12)?);
            let left = *k.get(14).ok_or_else(|| invalid("truncated kern classes"))?;
            let right = *k.get(15).ok_or_else(|| invalid("truncated kern classes"))?;
            let size = 16 + n * 2 + usize::from(left) * usize::from(right);
            if size > k.len() {
                return Err(invalid("truncated LVGL kern class maps"));
            }
            emit_numbers(
                &mut text,
                "uint8_t",
                "font_left",
                k[16..16 + n].iter().map(|&v| i64::from(v)),
            );
            emit_numbers(
                &mut text,
                "uint8_t",
                "font_right",
                k[16 + n..16 + n * 2].iter().map(|&v| i64::from(v)),
            );
            emit_numbers(
                &mut text,
                "int8_t",
                "font_kern_values",
                k[16 + n * 2..size].iter().map(|&v| i64::from(v as i8)),
            );
            writeln!(text,"static const lv_font_fmt_txt_kern_classes_t font_kern={{.class_pair_values=font_kern_values,.left_class_mapping=font_left,.right_class_mapping=font_right,.left_class_cnt={left},.right_class_cnt={right}}};").expect("String write");
        } else {
            return Err(invalid("unsupported LVGL binary kern format"));
        }
    }
    let ascent = i64::from(u16le(h, 16)?);
    let descent = i64::from(u16le(h, 18)? as i16);
    let line = ascent - descent;
    let up = u16le(h, 44)? as i16;
    let ut = u16le(h, 46)?;
    writeln!(text,"static const lv_font_fmt_txt_dsc_t font_dsc={{.glyph_bitmap=font_bitmap,.glyph_dsc=font_glyphs,.cmaps=font_cmaps,.kern_dsc={kern_ref},.kern_scale={},.cmap_num={nc},.bpp={bpp},.kern_classes={classes},.bitmap_format={compression}}};\nconst lv_font_t font={{.get_glyph_dsc=lv_font_get_glyph_dsc_fmt_txt,.get_glyph_bitmap=lv_font_get_bitmap_fmt_txt,.line_height={line},.base_line={},.underline_position={up},.underline_thickness={ut},.dsc=&font_dsc}};",u16le(h,32)?,-descent).expect("String write");
    b.add(text.len().saturating_mul(2))?;
    let ro = ReadOptions {
        limits: ResourceLimits {
            max_input_bytes: l.max_total_bytes,
            max_table_bytes: l.max_total_bytes,
            max_total_bytes: l.max_total_bytes.saturating_sub(b.used),
            ..l.clone()
        },
        ..Default::default()
    };
    let input = crate::Input::new(text.as_bytes());
    let mut context = ReadContext::new(&input, &ro.limits)?;
    let codec = CCodec {
        read_options: CReadOptions {
            family: Family::Lvgl,
            container: Some(CContainer::Source),
            ..Default::default()
        },
        ..Default::default()
    };
    let (mut set, mut report) = codec.read(&mut context, &ro)?;
    set.fonts[0].metrics.pixel_size = Some(u32::from(u16le(h, 14)?));
    if u16le(h, 20)? != u16le(h, 16)? || u16le(h, 22)? != u16le(h, 18)? || u16le(h, 24)? != 0 {
        loss(
            &mut report,
            "read.lvgl.typo_metrics",
            None,
            "Separate binary typographic metrics are not retained in the native font projection",
        );
    }
    Ok((set, report))
}
fn put16(v: &mut [u8], p: usize, n: u16) {
    v[p..p + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(v: &mut [u8], p: usize, n: u32) {
    v[p..p + 4].copy_from_slice(&n.to_le_bytes());
}
fn table(tag: &[u8; 4], mut data: Vec<u8>) -> Result<Vec<u8>> {
    let len =
        u32::try_from(data.len() + 8).map_err(|_| invalid("LVGL binary table exceeds u32"))?;
    let mut out = len.to_le_bytes().to_vec();
    out.extend_from_slice(tag);
    out.append(&mut data);
    Ok(out)
}
pub(super) fn encode_source(source: &str, o: &CWriteOptions, b: &mut Budget) -> Result<Vec<u8>> {
    let t = lexer::lex(source, b)?;
    let ds = lexer::declarations(&t, b)?;
    let root = select(&ds, "lv_font_t", Some(&o.array_name))?;
    let rf = fields(&t, root.start, root.end)?;
    let desc = named(&ds, ref_field(&rf, "dsc")?)?;
    let df = fields(&t, desc.start, desc.end)?;
    let bitmap = blob(&t, named(&ds, ref_field(&df, "glyph_bitmap")?)?, b)?;
    let gd = named(&ds, ref_field(&df, "glyph_dsc")?)?;
    let recs = records(&t, gd)?;
    let mut glyphdata = Vec::new();
    let mut offsets = Vec::new();
    for (i, &(a, z)) in recs.iter().enumerate() {
        let gf = fields(&t, a, z)?;
        offsets.push(
            u32::try_from(glyphdata.len() + 8)
                .map_err(|_| invalid("LVGL glyph offset exceeds u32"))?,
        );
        let adv = unsigned(number(&gf, "adv_w", None)?, 65535)?;
        let x = number(&gf, "ofs_x", None)?;
        let y = number(&gf, "ofs_y", None)?;
        let w = unsigned(number(&gf, "box_w", None)?, 255)?;
        let h = unsigned(number(&gf, "box_h", None)?, 255)?;
        glyphdata.extend_from_slice(&(adv as u16).to_be_bytes());
        glyphdata.extend([x as u8, y as u8, w as u8, h as u8]);
        let start = number(&gf, "bitmap_index", None)? as usize;
        let end = if let Some(&(a, z)) = recs.get(i + 1) {
            number(&fields(&t, a, z)?, "bitmap_index", None)? as usize
        } else {
            bitmap.len()
        };
        if start > end || end > bitmap.len() {
            return Err(invalid("LVGL encoder bitmap offsets invalid"));
        }
        glyphdata.extend_from_slice(&bitmap[start..end]);
    }
    let glyf = table(b"glyf", glyphdata)?;
    let mut loca = (offsets.len() as u32).to_le_bytes().to_vec();
    for offset in offsets {
        loca.extend_from_slice(&offset.to_le_bytes());
    }
    let loca = table(b"loca", loca)?;
    let cd = named(&ds, ref_field(&df, "cmaps")?)?;
    let cms = records(&t, cd)?;
    let mut cmap = vec![0u8; 4 + cms.len() * 16];
    put32(&mut cmap, 0, cms.len() as u32);
    for (i, (a, z)) in cms.into_iter().enumerate() {
        let f = fields(&t, a, z)?;
        let arr = named(&ds, ref_field(&f, "unicode_list")?)?;
        let vals = lexer::rows(&t, arr, b)?;
        let p = 4 + i * 16;
        let dataoffset =
            u32::try_from(cmap.len() + 8).map_err(|_| invalid("LVGL cmap offset overflow"))?;
        put32(&mut cmap, p, dataoffset);
        put32(&mut cmap, p + 4, number(&f, "range_start", None)? as u32);
        put16(&mut cmap, p + 8, number(&f, "range_length", None)? as u16);
        put16(
            &mut cmap,
            p + 10,
            number(&f, "glyph_id_start", None)? as u16,
        );
        put16(&mut cmap, p + 12, number(&f, "list_length", None)? as u16);
        cmap[p + 14] = 3;
        for row in vals {
            for v in row.values {
                cmap.extend_from_slice(&(v as u16).to_le_bytes());
            }
        }
        while !(cmap.len() + 8).is_multiple_of(4) {
            cmap.push(0);
        }
    }
    let cmap = table(b"cmap", cmap)?;
    let mut header = vec![0u8; 40];
    put16(&mut header, 4, 3);
    let line = number(&rf, "line_height", None)?;
    let base = number(&rf, "base_line", None)?;
    if !(0..=65535).contains(&(line - base)) || !(-32768..=32767).contains(&(-base)) {
        return Err(invalid("LVGL binary ascent/descent out of range"));
    }
    put16(&mut header, 6, line as u16);
    put16(&mut header, 8, (line - base) as u16);
    put16(&mut header, 10, (-base) as u16);
    put16(&mut header, 12, (line - base) as u16);
    put16(&mut header, 14, (-base) as u16);
    header[26] = 1;
    header[27] = 1;
    header[28] = 1;
    header[29] = number(&df, "bpp", None)? as u8;
    header[30] = 8;
    header[31] = 8;
    header[32] = 16;
    header[33] = number(&df, "bitmap_format", None)? as u8;
    put16(
        &mut header,
        36,
        number(&rf, "underline_position", Some(0))? as u16,
    );
    put16(
        &mut header,
        38,
        number(&rf, "underline_thickness", Some(0))? as u16,
    );
    let mut out = table(b"head", header)?;
    out.extend(cmap);
    out.extend(loca);
    out.extend(glyf);
    check(
        out.len(),
        o.limits.max_input_bytes,
        "LVGL binary output bytes",
    )?;
    b.add(out.len().saturating_mul(2))?;
    Ok(out)
}
