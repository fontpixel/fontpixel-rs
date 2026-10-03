//! Exact SPEC-v3 outline ordering and pixel-centre rasterization.
use super::{Bitmap, bitmap::invalid};
use crate::Result;
pub use crate::outline::spec_bitmap_contours as bitmap_contours;
pub use crate::outline::{Contour, Point};
pub(crate) fn segments(c: &[Point]) -> Vec<Vec<f64>> {
    if c.len() < 2 {
        return vec![];
    }
    let mut seq = if let Some(i) = c.iter().position(|p| p.2) {
        c[i..].iter().chain(&c[..i]).copied().collect::<Vec<_>>()
    } else {
        let mut s = vec![(
            (c[c.len() - 1].0 + c[0].0) / 2.0,
            (c[c.len() - 1].1 + c[0].1) / 2.0,
            true,
        )];
        s.extend_from_slice(c);
        s
    };
    seq.push(seq[0]);
    let (mut x0, mut y0) = (seq[0].0, seq[0].1);
    let mut ctrl: Option<(f64, f64)> = None;
    let mut out = Vec::new();
    for (x, y, on) in &seq[1..] {
        if *on {
            if let Some((cx, cy)) = ctrl {
                out.push(vec![x0, y0, cx, cy, *x, *y]);
                ctrl = None;
            } else {
                out.push(vec![x0, y0, *x, *y]);
            }
            x0 = *x;
            y0 = *y;
        } else {
            if let Some((cx, cy)) = ctrl {
                let (mx, my) = ((cx + x) / 2.0, (cy + y) / 2.0);
                out.push(vec![x0, y0, cx, cy, mx, my]);
                x0 = mx;
                y0 = my;
            }
            ctrl = Some((*x, *y));
        }
    }
    out
}
/// Nonzero-winding rasterization with 16 chords per quadratic curve.
pub fn rasterize(cs: &[Contour], ppu: f64, xmin: i64, ymin: i64, w: i64, h: i64) -> Result<Bitmap> {
    if !ppu.is_finite() || ppu <= 0.0 {
        return Err(invalid("invalid raster scale"));
    }
    let mut out = Bitmap::blank(w.max(0), h.max(0))?;
    if w <= 0 || h <= 0 {
        return Ok(out);
    }
    let mut crossings = vec![Vec::<(f64, i64)>::new(); h as usize];
    let mut intersections = 0usize;
    let mut exceeded = false;
    let mut edge = |x0: f64, y0: f64, x1: f64, y1: f64| {
        if y0 == y1 {
            return;
        }
        let (lo, hi, dir) = if y0 < y1 { (y0, y1, 1) } else { (y1, y0, -1) };
        let start = (((lo / ppu - ymin as f64 - 0.5).ceil().max(0.0)) as i64).min(h);
        let end = (((hi / ppu - ymin as f64 - 0.5).ceil().max(0.0)) as i64).min(h);
        intersections = intersections.saturating_add((end - start) as usize);
        if intersections > 8 * 1024 * 1024 {
            exceeded = true;
            return;
        }
        for r in start..end {
            let y = (ymin as f64 + r as f64 + 0.5) * ppu;
            if lo <= y && y < hi {
                crossings[r as usize].push((x0 + (y - y0) * (x1 - x0) / (y1 - y0), dir));
            }
        }
    };
    for c in cs {
        for s in segments(c) {
            if s.len() == 4 {
                edge(s[0], s[1], s[2], s[3]);
            } else {
                let (mut x0, mut y0) = (s[0], s[1]);
                for i in 1..=16 {
                    let (x, y) = if i == 16 {
                        (s[4], s[5])
                    } else {
                        let t = i as f64 / 16.0;
                        let mt = 1.0 - t;
                        (
                            mt * mt * s[0] + 2.0 * mt * t * s[2] + t * t * s[4],
                            mt * mt * s[1] + 2.0 * mt * t * s[3] + t * t * s[5],
                        )
                    };
                    edge(x0, y0, x, y);
                    x0 = x;
                    y0 = y;
                }
            }
        }
    }
    if exceeded {
        return Err(invalid("raster intersection budget exceeded"));
    }
    for (r, cr) in crossings.iter_mut().enumerate() {
        cr.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut acc = cr.iter().map(|v| v.1).sum::<i64>();
        let mut k = 0;
        for c in 0..w {
            let x = (xmin as f64 + c as f64 + 0.5) * ppu;
            while k < cr.len() && cr[k].0 <= x {
                acc -= cr[k].1;
                k += 1;
            }
            if acc != 0 {
                out.setpixel(c, h - 1 - r as i64, 1)?;
            }
        }
    }
    Ok(out)
}
