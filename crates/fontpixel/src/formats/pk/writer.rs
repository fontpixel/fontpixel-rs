use super::{PkWriteOptions, reader};
use crate::formats::tex_common::*;
use crate::model::Font;
use crate::{Report, Result};
/// Encode deterministic PK packets and report every changed model field.
pub fn write(f: &Font, o: &PkWriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut report = Report::default();
    match write_inner(f, o, &mut report) {
        Ok(bytes) => Ok((bytes, report)),
        Err(error) => Err(write_error(error, report, "pk")),
    }
}
fn write_inner(f: &Font, o: &PkWriteOptions, r: &mut Report) -> Result<Vec<u8>> {
    let mut budget = model_budget(f, &o.limits)?;
    let maps = mappings(f, "pk", r, &o.limits)?;
    budget.add(maps.len().saturating_mul(32))?;
    let mut out = Output::new(
        o.max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(budget.remaining() / 2),
    );
    let (ds, cs, hp, vp) = header_values(f, o)?;
    let comment = bytes(&f.headers, "tex.comment").unwrap_or(&[]);
    check(comment.len(), 255, "PK comment bytes")?;
    out.put(&[247, 89, comment.len() as u8])?;
    out.put(comment)?;
    for v in [ds, cs, hp, vp] {
        out.u(v, 4)?
    }
    write_specials(&f.properties, "pk", 240, &mut out, &o.limits)?;
    for &(_, cp, g) in &maps {
        let bitmap = mono(g, o.gray_threshold, &o.limits)?;
        let (w, h) = (bitmap.width() as usize, bitmap.height() as usize);
        let area = w.checked_mul(h).ok_or_else(limit)?;
        let n = area.div_ceil(8);
        let b = g.metrics.bounds;
        let hoff = i32::try_from(-i128::from(b.x))
            .map_err(|_| invalid("PK horizontal offset outside i32"))?;
        let voff = i32::try_from(i128::from(b.y) + i128::from(b.height) - 1)
            .map_err(|_| invalid("PK vertical offset outside i32"))?;
        let (tfm, dx, dy) = glyph_values(g, hp, ds)?;
        let short_common =
            cp <= 255 && (0..=0xffffff).contains(&tfm) && dy == 0 && dx >= 0 && dx % 65536 == 0;
        if short_common
            && dx / 65536 <= 255
            && b.width <= 255
            && b.height <= 255
            && i8::try_from(hoff).is_ok()
            && i8::try_from(voff).is_ok()
            && n + 8 <= 1023
        {
            let pl = n + 8;
            check(pl, o.limits.max_table_bytes, "PK packet bytes")?;
            out.byte(224 + (pl >> 8) as u8)?;
            out.u(pl as u32, 1)?;
            out.u(cp, 1)?;
            out.u(tfm as u32, 3)?;
            for v in [
                (dx / 65536) as u32,
                b.width,
                b.height,
                hoff as u32,
                voff as u32,
            ] {
                out.u(v, 1)?;
            }
        } else if short_common
            && dx / 65536 <= 65535
            && b.width <= 65535
            && b.height <= 65535
            && i16::try_from(hoff).is_ok()
            && i16::try_from(voff).is_ok()
            && n + 13 <= 196607
        {
            let pl = n + 13;
            check(pl, o.limits.max_table_bytes, "PK packet bytes")?;
            out.byte(228 + (pl >> 16) as u8)?;
            out.u(pl as u32, 2)?;
            out.u(cp, 1)?;
            out.u(tfm as u32, 3)?;
            for v in [
                (dx / 65536) as u32,
                b.width,
                b.height,
                hoff as u32,
                voff as u32,
            ] {
                out.u(v, 2)?;
            }
        } else {
            let dx = i32::try_from(dx).map_err(|_| invalid("PK long dx outside i32"))?;
            let dy = i32::try_from(dy).map_err(|_| invalid("PK long dy outside i32"))?;
            check(n + 28, o.limits.max_table_bytes, "PK packet bytes")?;
            out.byte(231)?;
            out.u(
                u32::try_from(n + 28).map_err(|_| invalid("PK packet exceeds u32"))?,
                4,
            )?;
            out.u(cp, 4)?;
            for v in [
                tfm as u32,
                dx as u32,
                dy as u32,
                b.width,
                b.height,
                hoff as u32,
                voff as u32,
            ] {
                out.u(v, 4)?;
            }
        }
        let start = out.b.len();
        out.put(&vec![0; n])?;
        if w > 0 {
            for y in 0..h {
                copy_bits(
                    &bitmap.as_bytes()[y * w.div_ceil(8)..(y + 1) * w.div_ceil(8)],
                    0,
                    &mut out.b[start..],
                    y * w,
                    w,
                );
            }
        }
    }
    out.byte(245)?;
    while !out.b.len().is_multiple_of(4) {
        out.byte(246)?
    }
    budget.add(out.b.len())?;
    let mut reread = o.limits.clone();
    reread.max_total_bytes = budget.remaining();
    let (target, rr) = reader::read(&out.b, &reread)?;
    r.extend(rr);
    compare_font(f, &target, "pk", r);
    compare_mapped(&target, &maps, "pk", r);
    report_cost(r, &mut budget)?;
    Ok(out.b)
}
