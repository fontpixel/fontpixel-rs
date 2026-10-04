use crate::bitmap::{Bitmap, Bounds};
use crate::formats::tex_common::*;
use crate::model::{Codepoint, Font, Glyph, Property, PropertyValue};
use crate::{Report, ResourceLimits, Result};
/// Decode GF commands, bitmap runs, backpointers and postamble locators.
pub fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    check(bytes.len(), limits.max_input_bytes, "GF input bytes")?;
    parse(bytes, limits, bytes.len())
}
pub(super) fn parse(bytes: &[u8], l: &ResourceLimits, retained: usize) -> Result<(Font, Report)> {
    let mut budget = Budget::new(retained, l)?;
    budget.add(8192)?;
    let mut c = Cursor::new(bytes);
    if c.take(2)? != [247, 131] {
        return Err(invalid("invalid GF preamble"));
    }
    let n = usize::from(c.byte()?);
    budget.add(n + 256)?;
    let mut f = Font::default();
    f.source_format = Some("gf".into());
    f.headers.push(Property::new(
        "tex.comment",
        PropertyValue::Bytes(c.take(n)?.to_vec()),
    ));
    let mut pending = crate::model::Properties::default();
    let mut records = Vec::new();
    let mut last = [None; 256];
    let mut boundary = c.p;
    let post;
    loop {
        let at = c.p;
        let op = c.byte()?;
        match op {
            67 | 68 => {
                check(f.len() + 1, l.max_glyphs, "GF glyphs")?;
                budget.add(2304)?;
                let (cp, prev, minx, maxx, miny, maxy) = if op == 67 {
                    (c.u(4)?, c.s(4)?, c.s(4)?, c.s(4)?, c.s(4)?, c.s(4)?)
                } else {
                    let cp = c.u(1)?;
                    let dx = c.u(1)?;
                    let maxx = c.u(1)?;
                    let dy = c.u(1)?;
                    let maxy = c.u(1)?;
                    (
                        cp,
                        -1,
                        i64::from(maxx) - i64::from(dx),
                        i64::from(maxx),
                        i64::from(maxy) - i64::from(dy),
                        i64::from(maxy),
                    )
                };
                let residue = cp as u8 as usize;
                if let Some(i) = last[residue] {
                    let (lo, hi, _): (usize, usize, u32) = records[i];
                    if prev < 0 || !pointer(bytes, prev as usize, lo, hi)? {
                        return Err(invalid(
                            "GF character backpointer disagrees with previous residue",
                        ));
                    }
                } else if prev != -1 {
                    return Err(invalid("GF first residue must use -1 backpointer"));
                }
                let w = u32::try_from(maxx - minx)
                    .map_err(|_| invalid("GF horizontal bounds reversed or too large"))?;
                let h = u32::try_from(maxy - miny + 1)
                    .map_err(|_| invalid("GF vertical bounds reversed or too large"))?;
                let size = bitmap_size(w, h, l)?;
                budget.add(size.saturating_mul(2))?;
                let stride = (w as usize).div_ceil(8);
                let mut raster = vec![0; size];
                let (mut x, mut y, mut ink) = (0usize, 0usize, false);
                let mut properties = std::mem::take(&mut pending);
                loop {
                    let op = c.byte()?;
                    match op {
                        0..=66 => {
                            let run = if op <= 63 {
                                u32::from(op)
                            } else {
                                c.u(usize::from(op - 63))?
                            } as usize;
                            if y >= h as usize || run > w as usize - x {
                                return Err(invalid("GF paint exceeds character rectangle"));
                            }
                            if ink && run > 0 {
                                black(&mut raster[y * stride..(y + 1) * stride], x, run)
                            }
                            x += run;
                            ink = !ink;
                        }
                        69 => break,
                        70..=73 => {
                            let n = if op == 70 {
                                0
                            } else {
                                c.u(usize::from(op - 70))?
                            } as usize;
                            y = y
                                .checked_add(n + 1)
                                .ok_or_else(|| invalid("GF row overflow"))?;
                            if y >= h as usize {
                                return Err(invalid("GF skip exceeds character rectangle"));
                            }
                            x = 0;
                            ink = false;
                        }
                        74..=238 => {
                            y = y.checked_add(1).ok_or_else(|| invalid("GF row overflow"))?;
                            x = usize::from(op - 74);
                            if y >= h as usize || x > w as usize {
                                return Err(invalid("GF new-row exceeds character rectangle"));
                            }
                            ink = true;
                        }
                        239..=243 => {
                            special(&mut c, op, 239, "gf", &mut properties, l, &mut budget)?
                        }
                        244 => {}
                        _ => return Err(invalid("unexpected GF command within character")),
                    }
                }
                let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
                    w,
                    h,
                    &raster,
                    bitmap_limits(l),
                )?);
                g.codepoints.push(Codepoint::Legacy(cp));
                g.metrics.bounds = Bounds::new(w, h, minx, miny);
                g.properties = properties;
                records.push((boundary, at, cp));
                last[residue] = Some(f.len());
                f.add_glyph(g)?;
                boundary = c.p;
            }
            239..=243 => special(&mut c, op, 239, "gf", &mut pending, l, &mut budget)?,
            244 => {}
            248 => {
                post = at;
                break;
            }
            _ => return Err(invalid("unexpected GF command outside character")),
        }
    }
    f.properties = pending;
    if c.s(4)? != boundary as i64 {
        return Err(invalid(
            "GF post pointer is not the final character boundary",
        ));
    }
    let (ds, cs, hp, vp) = (c.u(4)?, c.u(4)?, c.u(4)?, c.u(4)?);
    tex_header(&mut f, ds, cs, hp, vp)?;
    let post_bounds = c.take(16)?;
    f.headers.push(Property::new(
        "gf.post_bounds",
        PropertyValue::Bytes(post_bounds.to_vec()),
    ));
    let mut bounds = Cursor::new(post_bounds);
    let (px, qx, py, qy) = (bounds.s(4)?, bounds.s(4)?, bounds.s(4)?, bounds.s(4)?);
    let mut metrics = [None; 256];
    loop {
        let op = c.byte()?;
        match op {
            245 | 246 => {
                let cp = usize::from(c.byte()?);
                let (dx, dy) = if op == 245 {
                    (c.s(4)? as i32, c.s(4)? as i32)
                } else {
                    (i32::from(c.byte()?) << 16, 0)
                };
                let tfm = c.s(4)? as i32;
                let p = c.s(4)?;
                if metrics[cp].is_some() {
                    return Err(invalid("duplicate GF character locator"));
                }
                let i = last[cp].ok_or_else(|| invalid("GF locator has no matching glyph"))?;
                let (lo, hi, _) = records[i];
                if p < 0 || !pointer(bytes, p as usize, lo, hi)? {
                    return Err(invalid(
                        "GF locator pointer does not match final residue glyph",
                    ));
                }
                metrics[cp] = Some((tfm, dx, dy));
            }
            244 => {}
            249 => {
                if c.u(4)? as usize != post || c.byte()? != 131 {
                    return Err(invalid("GF post-post pointer or identification is invalid"));
                }
                let tail = c.take(bytes.len() - c.p)?;
                if tail.len() < 4 || tail.iter().any(|&v| v != 223) {
                    return Err(invalid("GF requires at least four 223 trailer bytes"));
                }
                break;
            }
            _ => return Err(invalid("invalid GF postamble command")),
        }
    }
    let ids: Vec<_> = f.glyphs().map(|(id, _)| id).collect();
    for id in ids {
        f.edit_glyph(id, |g| {
            let cp = g.codepoints[0].value() as u8 as usize;
            let (tfm, dx, dy) =
                metrics[cp].ok_or_else(|| invalid("GF glyph has no postamble metrics"))?;
            tex_glyph(g, tfm, i64::from(dx), i64::from(dy));
            let b = g.metrics.bounds;
            if b.x < px || b.x + i64::from(b.width) > qx || b.y + i64::from(b.height) - 1 > qy {
                return Err(invalid("GF postamble bounds do not enclose glyphs"));
            }
            let ink = g.bitmap.bbox();
            if ink.height > 0 && b.y + ink.y < py {
                return Err(invalid("GF postamble lower bound clips ink"));
            }
            Ok(())
        })?;
    }
    f.update_bounds()?;
    Ok((f, Report::default()))
}
fn pointer(bytes: &[u8], p: usize, lo: usize, hi: usize) -> Result<bool> {
    if p < lo || p > hi {
        return Ok(false);
    }
    let mut c = Cursor::new(bytes);
    c.p = lo;
    let mut found = false;
    while c.p < hi {
        found |= c.p == p;
        let op = c.byte()?;
        match op {
            239..=242 => {
                let n = c.u(usize::from(op - 238))? as usize;
                c.take(n)?;
            }
            243 => {
                c.take(4)?;
            }
            244 => {}
            _ => return Ok(false),
        }
    }
    Ok(c.p == hi && (found || p == hi))
}
