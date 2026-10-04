//! Native XY and FONT generation-specific directories.
use super::*;
fn grow(out: &mut Vec<u8>, n: usize, v: &RiscosWriteOptions, budget: &mut Budget) -> Result<usize> {
    let at = out.len();
    let total = at
        .checked_add(n)
        .ok_or_else(|| invalid("RISC OS output size overflow"))?;
    check(
        total,
        v.max_output_bytes
            .min(v.limits.max_input_bytes)
            .min(v.limits.max_table_bytes),
        "RISC OS output bytes",
    )?;
    budget.add(n.saturating_mul(2))?;
    out.resize(total, 0);
    Ok(at)
}
fn append(
    out: &mut Vec<u8>,
    b: &[u8],
    v: &RiscosWriteOptions,
    budget: &mut Budget,
) -> Result<usize> {
    let at = grow(out, b.len(), v, budget)?;
    out[at..].copy_from_slice(b);
    Ok(at)
}
fn align(out: &mut Vec<u8>, v: &RiscosWriteOptions, budget: &mut Budget) -> Result<()> {
    grow(out, (4 - out.len() % 4) % 4, v, budget)?;
    Ok(())
}
fn setup(f: &Font, xy: bool) -> Result<(u16, u16, u16, u16)> {
    let point = f.metrics.point_size.unwrap_or(Rational::integer(i64::from(
        f.metrics.pixel_size.unwrap_or(12),
    )));
    let p16 = point
        .numerator()
        .checked_mul(16)
        .ok_or_else(|| invalid("RISC OS point-size overflow"))?;
    if p16 % i64::from(point.denominator()) != 0 {
        return Err(invalid(
            "RISC OS point size requires sixteenth-point precision",
        ));
    }
    let yp = u16::try_from(p16 / i64::from(point.denominator()))
        .map_err(|_| invalid("RISC OS point size outside u16 range"))?;
    let xp = u16::try_from(int(&f.properties, "riscos.x_size", i64::from(yp))?)
        .map_err(|_| invalid("RISC OS horizontal point size exceeds u16"))?;
    let dpi = f
        .metrics
        .resolution
        .unwrap_or(Vector::new(90, if xy { 45 } else { 90 }));
    let dx =
        u16::try_from(dpi.x).map_err(|_| invalid("RISC OS horizontal resolution exceeds u16"))?;
    let dy =
        u16::try_from(dpi.y).map_err(|_| invalid("RISC OS vertical resolution exceeds u16"))?;
    if xp == 0 || yp == 0 || dx == 0 || dy == 0 {
        return Err(invalid(
            "RISC OS point size and resolution must be positive",
        ));
    }
    Ok((xp, dx, yp, dy))
}
fn bbox<'a>(fonts: impl Iterator<Item = &'a Font>) -> Result<Bounds> {
    let mut extent: Option<(i64, i64, i64, i64)> = None;
    for f in fonts {
        if let Some(b) = f.metrics.bounds {
            let right =
                b.x.checked_add(i64::from(b.width))
                    .ok_or_else(|| invalid("RISC OS font bbox overflow"))?;
            let top =
                b.y.checked_add(i64::from(b.height))
                    .ok_or_else(|| invalid("RISC OS font bbox overflow"))?;
            extent = Some(match extent {
                Some((x, y, r, t)) => (x.min(b.x), y.min(b.y), r.max(right), t.max(top)),
                None => (b.x, b.y, right, top),
            });
        }
        for (_, g) in f.glyphs() {
            let b = g.metrics.bounds;
            let right = b.x + i64::from(b.width);
            let top = b.y + i64::from(b.height);
            extent = Some(match extent {
                Some((x, y, r, t)) => (x.min(b.x), y.min(b.y), r.max(right), t.max(top)),
                None => (b.x, b.y, right, top),
            });
        }
    }
    let (x, y, r, t) = extent.unwrap_or((0, 0, 0, 0));
    Ok(Bounds::new(
        u32::try_from(
            r.checked_sub(x)
                .ok_or_else(|| invalid("RISC OS bbox width overflow"))?,
        )
        .map_err(|_| invalid("RISC OS bbox width exceeds u32"))?,
        u32::try_from(
            t.checked_sub(y)
                .ok_or_else(|| invalid("RISC OS bbox height overflow"))?,
        )
        .map_err(|_| invalid("RISC OS bbox height exceeds u32"))?,
        x,
        y,
    ))
}
fn pair(out: &mut Vec<u8>, x: i64, y: i64) {
    let a = x as u16 & 4095;
    let b = y as u16 & 4095;
    out.extend([
        a as u8,
        ((a >> 8) & 15) as u8 | ((b & 15) << 4) as u8,
        (b >> 4) as u8,
    ]);
}
pub(super) fn encode(
    s: &FontSet,
    v: &RiscosWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    if s.fonts.is_empty() {
        return Err(invalid("RISC OS output needs at least one font"));
    }
    if v.version == RiscosVersion::Xy {
        encode_xy(s, v, budget, report)
    } else {
        encode_new(s, v, budget, report)
    }
}
fn encode_xy(
    s: &FontSet,
    v: &RiscosWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    if v.depth != RiscosDepth::Gray4 || v.compression != RiscosCompression::Raw {
        return Err(invalid("RISC OS XY requires raw 4-bpp output"));
    }
    let mut out = Vec::new();
    grow(
        &mut out,
        s.fonts
            .len()
            .checked_mul(16)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| invalid("RISC OS XY index size overflow"))?,
        v,
        budget,
    )?;
    align(&mut out, v, budget)?;
    let mut count = 0;
    for (fi, f) in s.fonts.iter().enumerate() {
        let (_, dx, yp, dy) = setup(f, true)?;
        if yp % 16 != 0 || yp / 16 > 255 || dx > 255 || dy > 255 {
            return Err(invalid(
                "RISC OS XY requires integer 1..255 points and byte-sized DPI",
            ));
        }
        let slots = select(f, 255, v.unicode_as_legacy, "riscos", report, budget)?;
        count += slots.len();
        check(count, v.limits.max_glyphs, "RISC OS aggregate glyphs")?;
        let bounds = bbox(std::iter::once(f))?;
        let start = out.len();
        grow(&mut out, 532, v, budget)?;
        put32(
            &mut out,
            start,
            u32::from(yp) * u32::from(dx),
            ByteOrder::Little,
        );
        put32(
            &mut out,
            start + 4,
            u32::from(yp) * u32::from(dy),
            ByteOrder::Little,
        );
        put32(&mut out, start + 8, u32::from(dx), ByteOrder::Little);
        put32(&mut out, start + 12, u32::from(dy), ByteOrder::Little);
        for (i, value) in [
            bounds.x,
            bounds.y,
            bounds.x + i64::from(bounds.width),
            bounds.y + i64::from(bounds.height),
        ]
        .into_iter()
        .enumerate()
        {
            out[start + 16 + i] = i8::try_from(value)
                .map_err(|_| invalid("RISC OS XY font bbox exceeds signed bytes"))?
                as u8;
        }
        let mut rounded = false;
        for (&cp, g) in &slots {
            if g.bitmap.width() > 127 || g.bitmap.height() > 127 {
                return Err(invalid("RISC OS XY glyph dimensions exceed 127"));
            }
            let x = i8::try_from(g.metrics.bounds.x)
                .map_err(|_| invalid("RISC OS XY x offset exceeds i8"))?;
            let y = i8::try_from(g.metrics.bounds.y)
                .map_err(|_| invalid("RISC OS XY y offset exceeds i8"))?;
            let pointer = out.len() - start - 20;
            put16(
                &mut out,
                start + 20 + cp as usize * 2,
                u16::try_from(pointer)
                    .map_err(|_| invalid("RISC OS XY glyph offset exceeds u16"))?,
                ByteOrder::Little,
            );
            append(
                &mut out,
                &[
                    x as u8,
                    y as u8,
                    g.bitmap.width() as u8,
                    g.bitmap.height() as u8,
                ],
                v,
                budget,
            )?;
            let (data, lossy) = bitmap::encode(g, 4, v, budget)?;
            rounded |= lossy;
            append(&mut out, &data, v, budget)?;
            align(&mut out, v, budget)?;
            if out.len() - start > 65536 {
                return Err(invalid("RISC OS XY strike exceeds 64 KiB"));
            }
        }
        if rounded {
            loss(
                report,
                "gray_quantization",
                "Gray8 samples were explicitly rounded to sixteen native levels.",
                fi,
            );
        }
        let size = out.len() - start;
        out[fi * 16] = (yp / 16) as u8;
        out[fi * 16 + 1] = 4;
        out[fi * 16 + 2] = dx as u8;
        out[fi * 16 + 3] = dy as u8;
        put32(
            &mut out,
            fi * 16 + 8,
            u32::try_from(start).map_err(|_| invalid("RISC OS XY offset exceeds u32"))?,
            ByteOrder::Little,
        );
        put32(&mut out, fi * 16 + 12, size as u32, ByteOrder::Little);
    }
    Ok(out)
}
fn encode_new(
    s: &FontSet,
    v: &RiscosWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    let version = match v.version {
        RiscosVersion::V4 => 4,
        RiscosVersion::V5 => 5,
        RiscosVersion::V6 => 6,
        RiscosVersion::V7 => 7,
        RiscosVersion::V8 => 8,
        RiscosVersion::Xy => return Err(invalid("RISC OS FONT writer received XY profile")),
    };
    let depth = if v.depth == RiscosDepth::Mono { 1 } else { 4 };
    if depth == 4 && v.compression == RiscosCompression::Packed {
        return Err(invalid(
            "RISC OS packed encoding is only defined for one bit",
        ));
    }
    let (xp, dx, yp, dy) = setup(&s.fonts[0], false)?;
    let mut phases = BTreeMap::new();
    let mut nx = 1;
    let mut ny = 1;
    let mut count = 0;
    let mut maximum = 0u32;
    budget.add(s.fonts.len().saturating_mul(512))?;
    for (fi, f) in s.fonts.iter().enumerate() {
        if setup(f, false)? != (xp, dx, yp, dy) {
            return Err(invalid(
                "RISC OS FONT phases must share point size and resolution",
            ));
        }
        let px = u8::try_from(int(&f.properties, "riscos.phase_x", 0)?)
            .map_err(|_| invalid("RISC OS horizontal phase exceeds u8"))?;
        let py = u8::try_from(int(&f.properties, "riscos.phase_y", 0)?)
            .map_err(|_| invalid("RISC OS vertical phase exceeds u8"))?;
        if px > 3 || py > 3 {
            return Err(invalid("RISC OS phase must be 0..3"));
        }
        if px > 0 {
            nx = 4;
        }
        if py > 0 {
            ny = 4;
        }
        let slots = select(
            f,
            if version == 8 { 65535 } else { 255 },
            v.unicode_as_legacy,
            "riscos",
            report,
            budget,
        )?;
        count += slots.len();
        check(count, v.limits.max_glyphs, "RISC OS aggregate phase glyphs")?;
        maximum = maximum.max(
            *slots
                .keys()
                .next_back()
                .ok_or_else(|| invalid("RISC OS phase needs encoded glyphs"))?,
        );
        if phases.insert((px, py), (fi, slots)).is_some() {
            return Err(invalid("RISC OS duplicate phase coordinates"));
        }
    }
    if phases.len() != nx * ny {
        return Err(invalid(
            "RISC OS requires all four placements in every enabled subpixel dimension",
        ));
    }
    let chunks = if version == 8 {
        (maximum as usize + 1).div_ceil(32)
    } else {
        8
    };
    let bounds = bbox(s.fonts.iter())?;
    let mut out = Vec::new();
    grow(&mut out, 62, v, budget)?;
    out[..4].copy_from_slice(b"FONT");
    out[4] = depth;
    out[5] = version;
    let phaseflags = u16::from(nx > 1) | (u16::from(ny > 1) * 2);
    put16(
        &mut out,
        6,
        phaseflags | if version >= 7 { 64 } else { 0 },
        ByteOrder::Little,
    );
    for (i, value) in [
        bounds.x,
        bounds.y,
        i64::from(bounds.width),
        i64::from(bounds.height),
    ]
    .into_iter()
    .enumerate()
    {
        put16(
            &mut out,
            8 + i * 2,
            i16::try_from(value).map_err(|_| invalid("RISC OS FONT bbox exceeds i16"))? as u16,
            ByteOrder::Little,
        );
    }
    for (at, value) in [(52, 10), (54, xp), (56, dx), (58, yp), (60, dy)] {
        put16(&mut out, at, value, ByteOrder::Little);
    }
    if let Some(description) = bytes(&s.fonts[0], "riscos.description") {
        append(&mut out, description, v, budget)?;
    } else {
        let name = s.fonts[0].names.family.as_deref().unwrap_or("Font");
        if name.chars().any(|c| c as u32 > 255 || c == '\0') {
            return Err(invalid("RISC OS description needs Latin-1 without NUL"));
        }
        budget.add(name.len() + 64)?;
        let mut description: Vec<u8> = name.chars().map(|c| c as u8).collect();
        description.extend(b"\0Bitmaps\0\0");
        append(&mut out, &description, v, budget)?;
    }
    align(&mut out, v, budget)?;
    let directory = if version == 8 {
        let directory = out.len();
        grow(&mut out, (chunks + 1) * 4, v, budget)?;
        put32(&mut out, 16, directory as u32, ByteOrder::Little);
        put32(&mut out, 20, chunks as u32, ByteOrder::Little);
        directory
    } else {
        16
    };
    let mut rounded = [false; 16];
    for chunk in 0..chunks {
        let start = out.len();
        put32(
            &mut out,
            directory + chunk * 4,
            u32::try_from(start).map_err(|_| invalid("RISC OS chunk offset exceeds u32"))?,
            ByteOrder::Little,
        );
        let lo = (chunk * 32) as u32;
        let hi = lo + 31;
        if phases
            .values()
            .all(|(_, slots)| slots.range(lo..=hi).next().is_none())
        {
            continue;
        }
        let flagbytes = usize::from(version >= 7) * 4;
        let table = 128 * nx * ny;
        grow(&mut out, flagbytes + table, v, budget)?;
        if version >= 7 {
            put32(
                &mut out,
                start,
                0x8000_0000 | u32::from(phaseflags),
                ByteOrder::Little,
            );
        }
        let anchor = start + flagbytes;
        for px in 0..nx {
            for py in 0..ny {
                let (fi, slots) = phases
                    .get(&(px as u8, py as u8))
                    .ok_or_else(|| invalid("RISC OS incomplete phase grid"))?;
                for ci in 0..32 {
                    let Some(g) = slots.get(&(lo + ci as u32)) else {
                        continue;
                    };
                    let at = out.len();
                    put32(
                        &mut out,
                        anchor + ((px * ny + py) * 32 + ci) * 4,
                        u32::try_from(at - anchor)
                            .map_err(|_| invalid("RISC OS glyph offset exceeds u32"))?,
                        ByteOrder::Little,
                    );
                    let coords = [
                        g.metrics.bounds.x,
                        g.metrics.bounds.y,
                        i64::from(g.bitmap.width()),
                        i64::from(g.bitmap.height()),
                    ];
                    if coords.iter().any(|n| !(-2048..=2047).contains(n)) {
                        return Err(invalid(
                            "RISC OS glyph coordinates exceed signed 12-bit range",
                        ));
                    }
                    let wide = coords.iter().any(|n| i8::try_from(*n).is_err());
                    let mut tag = u8::from(wide) | if depth == 1 { 2 } else { 0 };
                    let (raw, lossy) = bitmap::encode(g, depth, v, budget)?;
                    rounded[*fi] |= lossy;
                    let (data, black) = if v.compression == RiscosCompression::Packed {
                        bitmap::pack(
                            &raw,
                            g.bitmap.width() as usize * g.bitmap.height() as usize,
                            budget,
                        )?
                    } else {
                        (raw, false)
                    };
                    if v.compression == RiscosCompression::Packed {
                        tag |= 12 << 4;
                        if black {
                            tag |= 4;
                        }
                    }
                    let mut record = Vec::with_capacity(7);
                    record.push(tag);
                    if wide {
                        pair(&mut record, coords[0], coords[1]);
                        pair(&mut record, coords[2], coords[3]);
                    } else {
                        record.extend(coords.into_iter().map(|n| n as i8 as u8));
                    }
                    append(&mut out, &record, v, budget)?;
                    append(&mut out, &data, v, budget)?;
                    align(&mut out, v, budget)?;
                }
            }
        }
    }
    let end = out.len();
    put32(
        &mut out,
        directory + chunks * 4,
        u32::try_from(end).map_err(|_| invalid("RISC OS file length exceeds u32"))?,
        ByteOrder::Little,
    );
    for (fi, value) in rounded.into_iter().enumerate() {
        if value {
            loss(
                report,
                "gray_quantization",
                "Gray samples were explicitly thresholded or rounded to the selected native depth.",
                fi,
            );
        }
    }
    Ok(out)
}
