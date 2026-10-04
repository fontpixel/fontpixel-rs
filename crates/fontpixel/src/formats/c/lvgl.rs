use super::embedded::*;
use super::*;
use std::fmt::Write as _;

type Fields<'a> = BTreeMap<&'a str, &'a [Token<'a>]>;
fn enums(t: &[Token<'_>]) -> Result<i64> {
    if let [
        Token {
            kind: Kind::Ident(s),
            ..
        },
    ] = t
    {
        return match *s {
            "LV_FONT_FMT_TXT_CMAP_FORMAT0_FULL" => Ok(0),
            "LV_FONT_FMT_TXT_CMAP_SPARSE_FULL" => Ok(1),
            "LV_FONT_FMT_TXT_CMAP_FORMAT0_TINY" => Ok(2),
            "LV_FONT_FMT_TXT_CMAP_SPARSE_TINY" => Ok(3),
            "LV_FONT_FMT_TXT_PLAIN" | "LV_FONT_SUBPX_NONE" => Ok(0),
            "LV_FONT_FMT_TXT_COMPRESSED" => Ok(1),
            "LV_FONT_FMT_TXT_COMPRESSED_NO_PREFILTER" => Err(invalid(
                "ambiguous upstream LVGL NO_PREFILTER enum; use numeric 2",
            )),
            _ => Err(invalid("unsupported LVGL symbolic field")),
        };
    }
    scalar(t)
}
fn efield(f: &Fields<'_>, name: &str, default: i64) -> Result<i64> {
    f.get(name).map_or(Ok(default), |v| enums(v))
}
fn values(t: &[Token<'_>], ds: &[Declaration<'_>], name: &str, b: &mut Budget) -> Result<Vec<i64>> {
    Ok(lexer::rows(t, named(ds, name)?, b)?
        .into_iter()
        .flat_map(|r| r.values)
        .collect())
}
fn array_ref(
    t: &[Token<'_>],
    ds: &[Declaration<'_>],
    f: &Fields<'_>,
    name: &str,
    b: &mut Budget,
) -> Result<Option<Vec<i64>>> {
    let Some(v) = f.get(name) else {
        return Ok(None);
    };
    reference(v)?.map(|s| values(t, ds, s, b)).transpose()
}
struct Bits<'a> {
    src: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn get(&mut self, n: u8) -> Result<u8> {
        if self.at.saturating_add(n as usize) > self.src.len().saturating_mul(8) {
            return Err(invalid("truncated LVGL bitmap"));
        }
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | ((self.src[self.at / 8] >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Ok(v)
    }
}
fn unpack(src: &[u8], w: usize, h: usize, bpp: u8, format: u8) -> Result<Vec<u8>> {
    let count = w * h;
    let mut bits = Bits { src, at: 0 };
    let mut out = Vec::with_capacity(count);
    let mut previous = None;
    let mut state = 0u8;
    let mut repeats = 0u8;
    for _ in 0..count {
        let value = if format == 0 {
            bits.get(bpp)?
        } else {
            match state {
                0 => {
                    let v = bits.get(bpp)?;
                    if previous == Some(v) {
                        state = 1;
                        repeats = 0;
                    }
                    previous = Some(v);
                    v
                }
                1 => {
                    repeats += 1;
                    if bits.get(1)? == 0 {
                        let v = bits.get(bpp)?;
                        previous = Some(v);
                        state = 0;
                        v
                    } else if repeats == 11 {
                        repeats = bits.get(6)?;
                        if repeats == 0 {
                            let v = bits.get(bpp)?;
                            previous = Some(v);
                            state = 0;
                            v
                        } else {
                            state = 2;
                            previous.unwrap_or(0)
                        }
                    } else {
                        previous.unwrap_or(0)
                    }
                }
                _ => {
                    repeats -= 1;
                    if repeats == 0 {
                        let v = bits.get(bpp)?;
                        previous = Some(v);
                        state = 0;
                        v
                    } else {
                        previous.unwrap_or(0)
                    }
                }
            }
        };
        let pos = out.len();
        let value = if format == 1 && pos >= w {
            value ^ out[pos - w]
        } else {
            value
        };
        out.push(value);
    }
    Ok(out)
}
pub(super) fn read(
    t: &[Token<'_>],
    ds: &[Declaration<'_>],
    o: &CReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
    r: &mut Report,
) -> Result<Font> {
    let root = select(ds, "lv_font_t", o.array_name.as_deref())?;
    let rf = fields(t, root.start, root.end)?;
    for callback in ["get_glyph_dsc", "get_glyph_bitmap"] {
        let expected = if callback == "get_glyph_dsc" {
            "lv_font_get_glyph_dsc_fmt_txt"
        } else {
            "lv_font_get_bitmap_fmt_txt"
        };
        if ref_field(&rf, callback)? != expected {
            return Err(invalid("LVGL custom callbacks are unsupported"));
        }
    }
    if efield(&rf, "subpx", 0)? != 0 {
        return Err(invalid("LVGL subpixel fonts are unsupported"));
    }
    if let Some(v) = rf.get("fallback")
        && reference(v)?.is_some()
    {
        return Err(invalid("LVGL fallback font chains are unsupported"));
    }
    let desc = named(ds, ref_field(&rf, "dsc")?)?;
    let df = fields(t, desc.start, desc.end)?;
    unsigned(number(&df, "kern_scale", Some(0))?, 65535)?;
    unsigned(number(&df, "kern_classes", Some(0))?, 1)?;
    let bpp = unsigned(number(&df, "bpp", None)?, 8)? as u8;
    if ![1, 2, 3, 4, 8].contains(&bpp) {
        return Err(invalid("unsupported LVGL bpp"));
    }
    let format = unsigned(efield(&df, "bitmap_format", 0)?, 2)? as u8;
    let bits = blob(t, named(ds, ref_field(&df, "glyph_bitmap")?)?, b)?;
    check(bits.len(), l.max_table_bytes, "LVGL bitmap bytes")?;
    let gd = named(ds, ref_field(&df, "glyph_dsc")?)?;
    let recs = records(t, gd)?;
    if recs.is_empty() {
        return Err(invalid("LVGL glyph descriptor array is empty"));
    }
    check(recs.len() - 1, l.max_glyphs, "LVGL glyph count")?;
    b.add(recs.len().saturating_mul(256))?;
    let cd = named(ds, ref_field(&df, "cmaps")?)?;
    let cmaps = records(t, cd)?;
    if cmaps.len() > 511 || number(&df, "cmap_num", None)? != cmaps.len() as i64 {
        return Err(invalid("LVGL cmap count mismatch"));
    }
    let mut mapping = BTreeMap::<usize, Vec<u32>>::new();
    let mut seen = BTreeSet::new();
    let mut end = 0u32;
    for (a, z) in cmaps {
        let cf = fields(t, a, z)?;
        let start = unsigned(number(&cf, "range_start", None)?, 0x10ffff)?;
        let len = unsigned(number(&cf, "range_length", None)?, 65535)?;
        let first = unsigned(number(&cf, "glyph_id_start", None)?, 65535)?;
        if len == 0 || start < end {
            return Err(invalid("LVGL cmap ranges overlap or are empty"));
        }
        end = start
            .checked_add(len)
            .ok_or_else(|| invalid("LVGL cmap overflow"))?;
        if end > 0x110000 {
            return Err(invalid("LVGL cmap exceeds Unicode"));
        }
        let ty = efield(&cf, "type", 0)?;
        if !(0..=3).contains(&ty) {
            return Err(invalid("unknown LVGL cmap type"));
        }
        let ul = array_ref(t, ds, &cf, "unicode_list", b)?;
        let offsets = array_ref(t, ds, &cf, "glyph_id_ofs_list", b)?;
        let listlen = unsigned(number(&cf, "list_length", Some(0))?, 65535)? as usize;
        let sparse = ty == 1 || ty == 3;
        let full = ty == 0 || ty == 1;
        if sparse != ul.is_some() || full != offsets.is_some() {
            return Err(invalid("LVGL cmap type and arrays disagree"));
        }
        let count = if sparse { listlen } else { len as usize };
        b.add(count.saturating_mul(96))?;
        if ul.as_ref().is_some_and(|v| v.len() != count)
            || offsets.as_ref().is_some_and(|v| v.len() != count)
        {
            return Err(invalid("LVGL cmap array length mismatch"));
        }
        let mut last = None;
        for i in 0..count {
            let relative = ul
                .as_ref()
                .map_or(Ok(i as u32), |v| unsigned(v[i], 65535))?;
            if relative >= len || last.is_some_and(|v| relative <= v) {
                return Err(invalid("LVGL sparse cmap is unsorted or outside range"));
            }
            last = Some(relative);
            let offset = offsets.as_ref().map_or(Ok(i as u32), |v| {
                unsigned(v[i], if sparse { 65535 } else { 255 })
            })?;
            let gid = first
                .checked_add(offset)
                .ok_or_else(|| invalid("LVGL glyph index overflow"))?
                as usize;
            if gid >= recs.len() {
                return Err(invalid("LVGL cmap glyph index outside descriptors"));
            }
            let cp = start + relative;
            if cp == 0 || char::from_u32(cp).is_none() {
                return Err(invalid("LVGL cmap contains non-renderable Unicode"));
            }
            if !seen.insert(cp) {
                return Err(invalid("duplicate LVGL mapping"));
            }
            if gid != 0 {
                mapping.entry(gid).or_default().push(cp);
            }
        }
    }
    let mut f = Font::default();
    let mut descriptors = Vec::with_capacity(recs.len());
    for (a, z) in recs {
        let gf = fields(t, a, z)?;
        let off = unsigned(number(&gf, "bitmap_index", None)?, u32::MAX)? as usize;
        let adv = unsigned(number(&gf, "adv_w", None)?, u32::MAX)?;
        let w = unsigned(number(&gf, "box_w", None)?, 65535)?;
        let h = unsigned(number(&gf, "box_h", None)?, 65535)?;
        let x = number(&gf, "ofs_x", None)?;
        let y = number(&gf, "ofs_y", None)?;
        if !(-32768..=32767).contains(&x) || !(-32768..=32767).contains(&y) {
            return Err(invalid("LVGL glyph bearing exceeds i16"));
        }
        descriptors.push((off, adv, w, h, x, y));
    }
    for (gid, &(off, adv, w, h, x, y)) in descriptors.iter().enumerate() {
        if gid == 0 {
            if (off, adv, w, h, x, y) != (0, 0, 0, 0, 0, 0) {
                return Err(invalid("LVGL reserved glyph zero is not empty"));
            }
            continue;
        }
        let count = (w as usize)
            .checked_mul(h as usize)
            .ok_or_else(|| invalid("LVGL pixel count overflow"))?;
        check(count, l.max_pixels_per_glyph, "LVGL glyph pixels")?;
        b.glyph(count.saturating_mul(3), gid, l)?;
        let end = descriptors.get(gid + 1).map_or(bits.len(), |d| d.0);
        if end < off || end > bits.len() {
            return Err(invalid("LVGL bitmap offsets are unsorted or out of range"));
        }
        let raw = unpack(&bits[off..end], w as usize, h as usize, bpp, format)?;
        let mut g = if bpp == 1 {
            let stride = (w as usize).div_ceil(8);
            let mut dst = vec![0u8; stride * h as usize];
            for (i, &v) in raw.iter().enumerate() {
                if v != 0 {
                    dst[i / w as usize * stride + (i % w as usize) / 8] |=
                        128 >> (i % w as usize % 8);
                }
            }
            Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
                w,
                h,
                &dst,
                bitmap_limits(l),
            )?)
        } else {
            let max = (1u16 << bpp) - 1;
            let dst: Vec<u8> = raw
                .into_iter()
                .map(|v| {
                    if bpp == 3 && format != 0 {
                        [0, 34, 68, 102, 153, 187, 221, 255][v as usize]
                    } else {
                        ((u16::from(v) * 255 + max / 2) / max) as u8
                    }
                })
                .collect();
            Glyph::new(Bitmap::<crate::bitmap::Gray8>::from_raw(
                w,
                h,
                &dst,
                bitmap_limits(l),
            )?)
        };
        g.metrics.bounds.x = x;
        g.metrics.bounds.y = y;
        g.metrics.advance = Some(Vector::new((i64::from(adv) + 8) / 16, 0));
        if adv % 16 != 0 {
            loss(
                r,
                "read.lvgl.fractional_advance",
                Some(gid - 1),
                "LVGL 8.4 advance rounded to integer pixels by native rule",
            );
        }
        g.codepoints = mapping
            .remove(&gid)
            .unwrap_or_default()
            .into_iter()
            .map(Codepoint::Unicode)
            .collect();
        f.add_glyph(g)?;
    }
    if let Some(kern) = df.get("kern_dsc")
        && let Some(name) = reference(kern)?
    {
        validate_kerning(
            t,
            ds,
            name,
            number(&df, "kern_classes", Some(0))?,
            descriptors.len(),
            b,
        )?;
        loss(
            r,
            "read.lvgl.kerning",
            None,
            "LVGL pair/class kerning cannot be represented by the core glyph model",
        );
    }
    f.update_bounds()?;
    let height = number(&rf, "line_height", None)?;
    let baseline = number(&rf, "base_line", None)?;
    if !(0..=32767).contains(&height) || !(-32768..=32767).contains(&baseline) {
        return Err(invalid("LVGL line metrics out of range"));
    }
    f.metrics.line_height = Some(height);
    f.metrics.ascent = Some(height - baseline);
    f.metrics.descent = Some(baseline);
    f.metrics.underline_position = rf
        .get("underline_position")
        .map(|v| scalar(v))
        .transpose()?;
    f.metrics.underline_thickness = rf
        .get("underline_thickness")
        .map(|v| scalar(v))
        .transpose()?;
    if f.metrics
        .underline_position
        .is_some_and(|v| !(-128..=127).contains(&v))
        || f.metrics
            .underline_thickness
            .is_some_and(|v| !(0..=255).contains(&v))
    {
        return Err(invalid("LVGL underline metric exceeds native byte fields"));
    }
    f.charset.name = Some("unicode".into());
    f.source_format = Some("c".into());
    Ok(f)
}
fn validate_kerning(
    t: &[Token<'_>],
    ds: &[Declaration<'_>],
    name: &str,
    classes: i64,
    glyphs: usize,
    b: &mut Budget,
) -> Result<()> {
    let d = named(ds, name)?;
    let f = fields(t, d.start, d.end)?;
    if classes == 0 {
        let count = unsigned(number(&f, "pair_cnt", None)?, 0x3fffffff)? as usize;
        let size = unsigned(number(&f, "glyph_ids_size", Some(0))?, 1)?;
        let ids = values(t, ds, ref_field(&f, "glyph_ids")?, b)?;
        let vals = values(t, ds, ref_field(&f, "values")?, b)?;
        if ids.len() != count.saturating_mul(2) || vals.len() != count {
            return Err(invalid("LVGL kerning pair length mismatch"));
        }
        let mut prev = None;
        for pair in ids.as_chunks::<2>().0 {
            let a = unsigned(pair[0], if size == 0 { 255 } else { 65535 })? as usize;
            let z = unsigned(pair[1], if size == 0 { 255 } else { 65535 })? as usize;
            if a >= glyphs || z >= glyphs || prev.is_some_and(|p| (a, z) <= p) {
                return Err(invalid("invalid LVGL kerning pair order/index"));
            }
            prev = Some((a, z));
        }
        if vals.iter().any(|v| !(-128..=127).contains(v)) {
            return Err(invalid("LVGL kern value out of i8 range"));
        }
    } else if classes == 1 {
        let left = unsigned(number(&f, "left_class_cnt", None)?, 255)?;
        let right = unsigned(number(&f, "right_class_cnt", None)?, 255)?;
        for (name, max) in [("left_class_mapping", left), ("right_class_mapping", right)] {
            let v = values(t, ds, ref_field(&f, name)?, b)?;
            if v.len() != glyphs || v.iter().any(|&n| n < 0 || n > i64::from(max)) {
                return Err(invalid("LVGL kern class map mismatch"));
            }
        }
        let vals = values(t, ds, ref_field(&f, "class_pair_values")?, b)?;
        if vals.len() != left as usize * right as usize
            || vals.iter().any(|v| !(-128..=127).contains(v))
        {
            return Err(invalid("LVGL kern class values mismatch"));
        }
    } else {
        return Err(invalid("invalid LVGL kern_classes flag"));
    }
    Ok(())
}
struct Put {
    bytes: Vec<u8>,
    at: usize,
}
impl Put {
    fn put(&mut self, v: u8, n: u8) {
        for bit in (0..n).rev() {
            if self.at.is_multiple_of(8) {
                self.bytes.push(0);
            }
            self.bytes[self.at / 8] |= ((v >> bit) & 1) << (7 - self.at % 8);
            self.at += 1;
        }
    }
}
// Native RLE: two equal literals enter repeat mode; ten single repeat bits
// precede the eleventh bit and six-bit count. Each step consumes input pixels.
fn pack(pixels: &[u8], bpp: u8, compressed: bool) -> Vec<u8> {
    let mut out = Put {
        bytes: Vec::new(),
        at: 0,
    };
    let mut previous = None;
    let mut repeat = false;
    let mut repeats = 0u8;
    let mut p = 0usize;
    while p < pixels.len() {
        let v = pixels[p];
        if !compressed || !repeat {
            out.put(v, bpp);
            p += 1;
            if compressed && previous == Some(v) {
                repeat = true;
                repeats = 0;
            }
            previous = Some(v);
        } else if previous != Some(v) {
            out.put(0, 1);
            out.put(v, bpp);
            p += 1;
            previous = Some(v);
            repeat = false;
        } else if repeats < 10 {
            out.put(1, 1);
            p += 1;
            repeats += 1;
        } else {
            let n = pixels[p..].iter().take(63).take_while(|&&x| x == v).count();
            out.put(1, 1);
            out.put(n as u8, 6);
            p += n;
            if p < pixels.len() {
                let next = pixels[p];
                out.put(next, bpp);
                p += 1;
                previous = Some(next);
            }
            repeat = false;
        }
    }
    out.bytes
}
pub(super) fn write(
    f: &Font,
    selected: &BTreeMap<u32, (usize, &Glyph)>,
    o: &CWriteOptions,
    b: &mut Budget,
) -> Result<String> {
    let n = selected.len();
    if n >= 65535 {
        return Err(invalid(
            "LVGL output glyph count exceeds native u16 mapping",
        ));
    }
    let bpp = match o.lvgl_bpp {
        LvglBpp::One => 1,
        LvglBpp::Two => 2,
        LvglBpp::Four => 4,
        LvglBpp::Eight => 8,
    };
    let format = match o.lvgl_compression {
        LvglCompression::Plain => 0,
        LvglCompression::RleXor => 1,
        LvglCompression::Rle => 2,
    };
    let mut bitmap = Vec::new();
    let mut records = String::new();
    records.push_str("{.bitmap_index=0,.adv_w=0,.box_w=0,.box_h=0,.ofs_x=0,.ofs_y=0},\n");
    for (&cp, (_, g)) in selected {
        if cp == 0 || cp == 9 {
            return Err(invalid(
                "LVGL reserves NUL and transforms TAB; cannot encode directly",
            ));
        }
        let bb = g.metrics.bounds;
        let adv = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .unwrap_or(Vector::new(i64::from(bb.width), 0));
        if bb.width > 255
            || bb.height > 255
            || !(-128..=127).contains(&bb.x)
            || !(-128..=127).contains(&bb.y)
            || !(0..=255).contains(&adv.x)
            || bitmap.len() > 0xfffff
        {
            return Err(invalid("LVGL output exceeds normal fmt_txt field range"));
        }
        let count = bb.width as usize * bb.height as usize;
        b.add(count.saturating_mul(12) + 1024)?;
        let max = (1u16 << bpp) - 1;
        let mut pixels = Vec::with_capacity(count);
        if bpp == 1 && matches!(g.bitmap, GlyphBitmap::Gray8(_)) && o.gray_threshold.is_none() {
            return Err(invalid("Gray8 to LVGL 1bpp requires gray_threshold"));
        }
        for y in 0..bb.height {
            for x in 0..bb.width {
                let gray = match &g.bitmap {
                    GlyphBitmap::Mono(v) => {
                        if v.get_pixel(x, y)? == 0 {
                            0
                        } else {
                            255
                        }
                    }
                    GlyphBitmap::Gray8(v) => v.get_pixel(x, y)?,
                };
                pixels.push(if bpp == 1 {
                    u8::from(gray >= o.gray_threshold.unwrap_or(128))
                } else {
                    ((u16::from(gray) * max + 127) / 255) as u8
                });
            }
        }
        if format == 1 {
            for i in (bb.width as usize..pixels.len()).rev() {
                pixels[i] ^= pixels[i - bb.width as usize];
            }
        }
        writeln!(
            records,
            "{{.bitmap_index={},.adv_w={},.box_w={},.box_h={},.ofs_x={},.ofs_y={}}},",
            bitmap.len(),
            adv.x * 16,
            bb.width,
            bb.height,
            bb.x,
            bb.y
        )
        .expect("String write");
        bitmap.extend(pack(&pixels, bpp, format != 0));
    }
    let symbol = &o.array_name;
    let mut out = String::from("#include <stdint.h>\n#include <lvgl.h>\n");
    b.add(
        bitmap.len().saturating_mul(12) + records.len().saturating_mul(2) + n.saturating_mul(64),
    )?;
    emit_bytes(&mut out, &format!("{symbol}_bitmap"), &bitmap);
    writeln!(
        out,
        "static const lv_font_fmt_txt_glyph_dsc_t {symbol}_glyphs[] = {{\n{records}}};"
    )
    .expect("String write");
    let mut groups: Vec<Vec<(u32, usize)>> = Vec::new();
    for (i, (&cp, _)) in selected.iter().enumerate() {
        if groups.last().is_none_or(|v| cp - v[0].0 >= 65535) {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("group created").push((cp, i + 1));
    }
    if groups.len() > 511 {
        return Err(invalid("LVGL cmap count exceeds native 9-bit field"));
    }
    for (i, group) in groups.iter().enumerate() {
        writeln!(out, "static const uint16_t {symbol}_unicode_{i}[] = {{").expect("String write");
        for &(cp, _) in group {
            write!(out, "{},", cp - group[0].0).expect("String write");
        }
        out.push_str("};\n");
    }
    writeln!(
        out,
        "static const lv_font_fmt_txt_cmap_t {symbol}_cmaps[] = {{"
    )
    .expect("String write");
    for (i, group) in groups.iter().enumerate() {
        writeln!(out,"{{.range_start={},.range_length={},.glyph_id_start={},.unicode_list={symbol}_unicode_{i},.glyph_id_ofs_list=NULL,.list_length={},.type=LV_FONT_FMT_TXT_CMAP_SPARSE_TINY}},",group[0].0,group.last().expect("group nonempty").0-group[0].0+1,group[0].1,group.len()).expect("String write");
    }
    out.push_str("};\n");
    writeln!(out,"static const lv_font_fmt_txt_dsc_t {symbol}_dsc = {{.glyph_bitmap={symbol}_bitmap,.glyph_dsc={symbol}_glyphs,.cmaps={symbol}_cmaps,.kern_dsc=NULL,.kern_scale=0,.cmap_num={},.bpp={bpp},.kern_classes=0,.bitmap_format={format},.cache=NULL}};",groups.len()).expect("String write");
    let baseline = f.metrics.descent.unwrap_or(0);
    let height = f.metrics.line_height.unwrap_or(
        f.metrics
            .ascent
            .unwrap_or_else(|| f.metrics.bounds.map_or(1, |v| i64::from(v.height)))
            + baseline,
    );
    if !(0..=32767).contains(&height) || !(-32768..=32767).contains(&baseline) {
        return Err(invalid("LVGL global metrics out of range"));
    }
    let up = f.metrics.underline_position.unwrap_or(0);
    let ut = f.metrics.underline_thickness.unwrap_or(0);
    if !(-128..=127).contains(&up) || ut > 255 {
        return Err(invalid("LVGL underline metric out of range"));
    }
    writeln!(out,"const lv_font_t {symbol} = {{.get_glyph_dsc=lv_font_get_glyph_dsc_fmt_txt,.get_glyph_bitmap=lv_font_get_bitmap_fmt_txt,.line_height={height},.base_line={baseline},.subpx=LV_FONT_SUBPX_NONE,.underline_position={up},.underline_thickness={ut},.dsc=&{symbol}_dsc}};").expect("String write");
    Ok(out)
}
