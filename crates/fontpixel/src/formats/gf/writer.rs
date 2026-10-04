use super::{GfWriteOptions, reader};
use crate::formats::tex_common::*;
use crate::model::Font;
use crate::{Report, Result};
/// Encode GF run commands with complete per-residue metrics and backpointer chains.
pub fn write(f: &Font, o: &GfWriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut report = Report::default();
    match write_inner(f, o, &mut report) {
        Ok(bytes) => Ok((bytes, report)),
        Err(error) => Err(write_error(error, report, "gf")),
    }
}
fn write_inner(f: &Font, o: &GfWriteOptions, r: &mut Report) -> Result<Vec<u8>> {
    let mut budget = model_budget(f, &o.limits)?;
    let candidates = mappings(f, "gf", r, &o.limits)?;
    budget.add(candidates.len().saturating_mul(64))?;
    let (ds, cs, hp, vp) = header_values(f, o)?;
    let mut metrics = [None; 256];
    let mut maps = Vec::new();
    for item in candidates {
        let (tfm, dx, dy) = glyph_values(item.2, hp, ds)?;
        let metric = (
            tfm,
            i32::try_from(dx).map_err(|_| invalid("GF dx outside i32"))?,
            i32::try_from(dy).map_err(|_| invalid("GF dy outside i32"))?,
        );
        let residue = item.1 as u8 as usize;
        if metrics[residue].is_some_and(|v| v != metric) {
            loss(
                r,
                "gf",
                "shared_metric",
                Some(item.0),
                Some(item.1),
                "Glyph omitted because its modulo-256 residue has different GF metrics",
            );
        } else {
            metrics[residue] = Some(metric);
            maps.push(item);
        }
    }
    if maps.is_empty() {
        return Err(fatal(
            std::mem::take(r),
            "gf",
            "No glyphs have representable shared metrics",
        ));
    }
    let mut out = Output::new(
        o.max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(budget.remaining() / 2),
    );
    let comment = bytes(&f.headers, "tex.comment").unwrap_or(&[]);
    check(comment.len(), 255, "GF comment bytes")?;
    out.put(&[247, 131, comment.len() as u8])?;
    out.put(comment)?;
    let mut pointers = [-1i32; 256];
    let (mut minx, mut maxx, mut miny, mut maxy) = (i64::MAX, i64::MIN, i64::MAX, i64::MIN);
    let mut boundary = out.b.len();
    for &(_, cp, g) in &maps {
        let b = g.metrics.bounds;
        let x = i32::try_from(b.x).map_err(|_| invalid("GF x outside i32"))?;
        let right = i32::try_from(b.x + i64::from(b.width))
            .map_err(|_| invalid("GF right edge outside i32"))?;
        let y = i32::try_from(b.y).map_err(|_| invalid("GF y outside i32"))?;
        let top = i32::try_from(i128::from(b.y) + i128::from(b.height) - 1)
            .map_err(|_| invalid("GF top edge outside i32"))?;
        let residue = cp as u8 as usize;
        let pointer = i32::try_from(out.b.len()).map_err(|_| invalid("GF pointer outside i32"))?;
        out.byte(67)?;
        out.u(cp, 4)?;
        for v in [pointers[residue], x, right, y, top] {
            out.u(v as u32, 4)?
        }
        pointers[residue] = pointer;
        write_specials(&g.properties, "gf", 239, &mut out, &o.limits)?;
        let bitmap = mono(g, o.gray_threshold, &o.limits)?;
        let (w, h) = (b.width as usize, b.height as usize);
        let stride = w.div_ceil(8);
        if w > 0 {
            for row in 0..h {
                if row > 0 {
                    out.byte(70)?
                }
                let data = &bitmap.as_bytes()[row * stride..(row + 1) * stride];
                let mut x = 0;
                let mut ink = false;
                while x < w {
                    let count = run(data, x, w, ink);
                    paint(&mut out, count)?;
                    x += count;
                    ink = !ink;
                }
            }
        }
        out.byte(69)?;
        boundary = out.b.len();
        minx = minx.min(i64::from(x));
        maxx = maxx.max(i64::from(right));
        miny = miny.min(i64::from(y));
        maxy = maxy.max(i64::from(top));
    }
    write_specials(&f.properties, "gf", 239, &mut out, &o.limits)?;
    let post = u32::try_from(out.b.len()).map_err(|_| invalid("GF post pointer outside u32"))?;
    out.byte(248)?;
    out.u(
        u32::try_from(boundary).map_err(|_| invalid("GF boundary outside u32"))?,
        4,
    )?;
    for v in [ds, cs, hp, vp] {
        out.u(v, 4)?
    }
    let stored = bytes(&f.headers, "gf.post_bounds").filter(|v| v.len() == 16);
    let reuse = if let Some(v) = stored {
        let mut c = Cursor::new(v);
        let (a, b, c0, d) = (c.s(4)?, c.s(4)?, c.s(4)?, c.s(4)?);
        a <= minx && b >= maxx && c0 <= miny && d >= maxy
    } else {
        false
    };
    if reuse {
        out.put(stored.unwrap_or(&[]))?
    } else {
        for v in [minx, maxx, miny, maxy] {
            out.u(v as i32 as u32, 4)?
        }
    }
    for (cp, metric) in metrics.iter().enumerate() {
        if let Some((tfm, dx, dy)) = metric {
            out.byte(245)?;
            out.byte(cp as u8)?;
            for v in [*dx, *dy, *tfm, pointers[cp]] {
                out.u(v as u32, 4)?
            }
        }
    }
    out.byte(249)?;
    out.u(post, 4)?;
    out.byte(131)?;
    out.put(&[223; 4])?;
    while !out.b.len().is_multiple_of(4) {
        out.byte(223)?
    }
    budget.add(out.b.len())?;
    let mut reread = o.limits.clone();
    reread.max_total_bytes = budget.remaining();
    let (target, rr) = reader::read(&out.b, &reread)?;
    r.extend(rr);
    compare_font(f, &target, "gf", r);
    compare_mapped(&target, &maps, "gf", r);
    report_cost(r, &mut budget)?;
    Ok(out.b)
}
fn run(b: &[u8], mut x: usize, end: usize, ink: bool) -> usize {
    let start = x;
    while x < end {
        let shift = x % 8;
        let v = if ink { !b[x / 8] } else { b[x / 8] };
        let n = ((v << shift).leading_zeros() as usize)
            .min(8 - shift)
            .min(end - x);
        x += n;
        if n == 0 || !x.is_multiple_of(8) {
            break;
        }
    }
    x - start
}
fn paint(o: &mut Output, mut n: usize) -> Result<()> {
    while n > 0xffffff {
        paint(o, 0xffffff)?;
        o.byte(0)?;
        n -= 0xffffff;
    }
    if n <= 63 {
        o.byte(n as u8)
    } else if n <= 255 {
        o.byte(64)?;
        o.u(n as u32, 1)
    } else if n <= 65535 {
        o.byte(65)?;
        o.u(n as u32, 2)
    } else {
        o.byte(66)?;
        o.u(n as u32, 3)
    }
}
