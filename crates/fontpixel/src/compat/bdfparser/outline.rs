//! Exact SPEC-v3 outline ordering and pixel-centre rasterization.
use super::{Bitmap, bitmap::invalid};
use crate::Result;
use std::collections::BTreeMap;
/// TrueType point (x, y, on-curve), y upward.
pub type Point = (f64, f64, bool);
/// Closed TrueType contour without a repeated final point.
pub type Contour = Vec<Point>;
/// Generate merged square or individual square/circle contours in SPEC order.
pub fn bitmap_contours(
    rows: &[String],
    shape: &str,
    dotsize: f64,
    ppu: i64,
    xoff: i64,
    yoff: i64,
) -> Result<Vec<Contour>> {
    if !matches!(shape, "square" | "circle") || !(0.0 < dotsize && dotsize <= 1.5) || ppu < 1 {
        return Err(invalid("invalid shape, dotsize or ppu"));
    }
    let total = rows
        .iter()
        .try_fold(0usize, |n, r| n.checked_add(r.len()))
        .ok_or_else(|| invalid("outline input overflows"))?;
    if total > 16 * 1024 * 1024 {
        return Err(invalid("outline input exceeds pixel budget"));
    }
    if ppu > i32::MAX as i64
        || xoff.unsigned_abs() > i32::MAX as u64
        || yoff.unsigned_abs() > i32::MAX as u64
    {
        return Err(invalid("outline coordinate budget exceeded"));
    }
    let rows = rows
        .iter()
        .rev()
        .map(|s| s.bytes().map(|b| b != b'0').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if shape != "square" || dotsize != 1.0 {
        let mut out = Vec::new();
        let s = dotsize * ppu as f64 / 2.0;
        let rr = s * 1.082392200292394;
        let d = std::f64::consts::FRAC_1_SQRT_2;
        let dirs = [
            (0.0, 1.0),
            (d, d),
            (1.0, 0.0),
            (d, -d),
            (0.0, -1.0),
            (-d, -d),
            (-1.0, 0.0),
            (-d, d),
        ];
        let round = |n: f64| (n + 0.5).floor();
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                if !v {
                    continue;
                }
                let cx = (xoff as f64 + c as f64 + 0.5) * ppu as f64;
                let cy = (yoff as f64 + r as f64 + 0.5) * ppu as f64;
                if out.len() >= 262144 {
                    return Err(invalid("outline point budget exceeded"));
                }
                out.push(if shape == "square" {
                    vec![
                        (round(cx - s), round(cy - s), true),
                        (round(cx - s), round(cy + s), true),
                        (round(cx + s), round(cy + s), true),
                        (round(cx + s), round(cy - s), true),
                    ]
                } else {
                    dirs.iter()
                        .map(|(x, y)| (round(cx + rr * x), round(cy + rr * y), false))
                        .collect()
                });
            }
        }
        return Ok(out);
    }
    let ink = |c: i64, r: i64| {
        r >= 0
            && c >= 0
            && rows
                .get(r as usize)
                .and_then(|row| row.get(c as usize))
                .copied()
                .unwrap_or(false)
    };
    let mut edges = Vec::<(i64, i64, i64, i64)>::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if !v {
                continue;
            }
            let (c, r) = (c as i64, r as i64);
            if edges.len() >= 2 * 1024 * 1024 {
                return Err(invalid("outline edge budget exceeded"));
            }
            if !ink(c, r - 1) {
                edges.push((c + 1, r, -1, 0));
            }
            if !ink(c - 1, r) {
                edges.push((c, r, 0, 1));
            }
            if !ink(c, r + 1) {
                edges.push((c, r + 1, 1, 0));
            }
            if !ink(c + 1, r) {
                edges.push((c + 1, r + 1, 0, -1));
            }
        }
    }
    let mut outgoing = BTreeMap::<(i64, i64), Vec<usize>>::new();
    for (i, e) in edges.iter().enumerate() {
        outgoing.entry((e.0, e.1)).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut out = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        let (sx, sy, mut dx, mut dy) = edges[first];
        let (mut x, mut y) = (sx + dx, sy + dy);
        let mut walk = vec![first];
        used[first] = true;
        while (x, y) != (sx, sy) {
            let candidates = outgoing
                .get(&(x, y))
                .ok_or_else(|| invalid("outline vertex has no outgoing edge"))?;
            let mut chosen = None;
            for want in [(dy, -dx), (dx, dy), (-dy, dx)] {
                for i in candidates {
                    if !used[*i] && (edges[*i].2, edges[*i].3) == want {
                        chosen = Some(*i);
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }
            let i = chosen.ok_or_else(|| invalid("outline tracing failed"))?;
            used[i] = true;
            walk.push(i);
            dx = edges[i].2;
            dy = edges[i].3;
            x += dx;
            y += dy;
        }
        let mut verts = Vec::new();
        for k in 0..walk.len() {
            let e = edges[walk[k]];
            let p = edges[walk[(k + walk.len() - 1) % walk.len()]];
            if (p.2, p.3) != (e.2, e.3) {
                verts.push((e.0, e.1));
            }
        }
        let best = verts
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| (v.1, v.0))
            .map(|(i, _)| i)
            .unwrap_or(0);
        verts.rotate_left(best);
        out.push(
            verts
                .into_iter()
                .map(|(x, y)| (((xoff + x) * ppu) as f64, ((yoff + y) * ppu) as f64, true))
                .collect(),
        );
    }
    Ok(out)
}
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
    let mut out = Bitmap::blank(w.max(0), h.max(0))?;
    if w <= 0 || h <= 0 {
        return Ok(out);
    }
    let mut crossings = vec![Vec::<(f64, i64)>::new(); h as usize];
    let mut edge = |x0: f64, y0: f64, x1: f64, y1: f64| {
        if y0 == y1 {
            return;
        }
        let (lo, hi, dir) = if y0 < y1 { (y0, y1, 1) } else { (y1, y0, -1) };
        for r in 0..h {
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
