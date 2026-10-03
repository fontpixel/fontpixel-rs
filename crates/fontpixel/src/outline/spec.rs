//! Shared SPEC contour tracing; ordering is a byte-compatibility contract.
use super::{Contour, invalid};
use crate::Result;
use std::collections::BTreeMap;
/// Generate merged square or individual square/circle contours in SPEC order.
pub(crate) fn spec_contours(
    rows: &[String],
    shape: &str,
    dotsize: f64,
    ppu: i64,
    xoff: i64,
    yoff: i64,
    limits: &super::Limits,
) -> Result<Vec<Contour>> {
    if !matches!(shape, "square" | "circle") || !(0.0 < dotsize && dotsize <= 1.5) || ppu < 1 {
        return Err(invalid("invalid shape, dotsize or ppu"));
    }
    let total = rows
        .iter()
        .try_fold(0usize, |n, r| n.checked_add(r.len()))
        .ok_or_else(|| invalid("outline input overflows"))?;
    if total > limits.max_pixels {
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
                if out.len() >= limits.max_points / 4 {
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
            if edges.len().saturating_add(4) > limits.max_edges {
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
    let mut points = 0usize;
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
        points = points.saturating_add(verts.len());
        if points > limits.max_points {
            return Err(invalid("outline point budget exceeded"));
        }
        out.push(
            verts
                .into_iter()
                .map(|(x, y)| (((xoff + x) * ppu) as f64, ((yoff + y) * ppu) as f64, true))
                .collect(),
        );
    }
    Ok(out)
}
