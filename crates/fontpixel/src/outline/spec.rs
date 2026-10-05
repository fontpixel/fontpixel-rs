//! Shared SPEC contour tracing; ordering is a byte-compatibility contract.
use super::{Contour, invalid};
use crate::{
    Result,
    working::{Budget, Nested, Owned, Vector},
};
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
    let budget = Budget::new(usize::MAX, "outline working bytes");
    Ok(
        spec_contours_with_budget(rows, shape, dotsize, ppu, (xoff, yoff), limits, &budget)?
            .into_parts()
            .0,
    )
}

/// As [`spec_contours`], charging working storage to `budget`; `origin` is (x, y) offset.
pub(crate) fn spec_contours_with_budget(
    rows: &[String],
    shape: &str,
    dotsize: f64,
    ppu: i64,
    origin: (i64, i64),
    limits: &super::Limits,
    budget: &Budget,
) -> Result<Owned<Vec<Contour>>> {
    let (xoff, yoff) = origin;
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
    if shape != "square" || dotsize != 1.0 {
        let count = rows
            .iter()
            .flat_map(|r| r.bytes())
            .filter(|&b| b != b'0')
            .count();
        // Preserve SPEC's historical four-point admission even for circles.
        if count > limits.max_points / 4 {
            return Err(invalid("outline point budget exceeded"));
        }
        let per_contour = if shape == "square" { 4 } else { 8 };
        let points = count
            .checked_mul(per_contour)
            .ok_or_else(|| invalid("outline point count overflow"))?;
        let mut out = super::IndependentContours::new(count, points, budget)?;
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
        for (r, row) in rows.iter().rev().enumerate() {
            for (c, v) in row.bytes().enumerate() {
                if v == b'0' {
                    continue;
                }
                let cx = (xoff as f64 + c as f64 + 0.5) * ppu as f64;
                let cy = (yoff as f64 + r as f64 + 0.5) * ppu as f64;
                if shape == "square" {
                    out.push(
                        4,
                        [
                            (round(cx - s), round(cy - s), true),
                            (round(cx - s), round(cy + s), true),
                            (round(cx + s), round(cy + s), true),
                            (round(cx + s), round(cy - s), true),
                        ]
                        .into_iter(),
                    )?;
                } else {
                    out.push(
                        8,
                        dirs.iter()
                            .map(|(x, y)| (round(cx + rr * x), round(cy + rr * y), false)),
                    )?;
                }
            }
        }
        return out.finish();
    }
    let ink = |c: i64, r: i64| {
        r >= 0
            && c >= 0
            && rows
                .len()
                .checked_sub(r as usize + 1)
                .and_then(|i| rows.get(i))
                .and_then(|row| row.as_bytes().get(c as usize))
                .is_some_and(|&b| b != b'0')
    };
    let mut edges = Vector::<(i64, i64, i64, i64)>::new(budget)?;
    for (r, row) in rows.iter().rev().enumerate() {
        for (c, v) in row.bytes().enumerate() {
            if v == b'0' {
                continue;
            }
            let (c, r) = (c as i64, r as i64);
            if edges.len().saturating_add(4) > limits.max_edges {
                return Err(invalid("outline edge budget exceeded"));
            }
            if !ink(c, r - 1) {
                edges.push((c + 1, r, -1, 0))?;
            }
            if !ink(c - 1, r) {
                edges.push((c, r, 0, 1))?;
            }
            if !ink(c, r + 1) {
                edges.push((c, r + 1, 1, 0))?;
            }
            if !ink(c + 1, r) {
                edges.push((c + 1, r + 1, 0, -1))?;
            }
        }
    }
    // Sorting by vertex and original edge index preserves candidate insertion
    // order without an allocation for every outgoing-edge tree entry.
    let mut outgoing = Vector::with_capacity(budget, edges.len())?;
    for (i, e) in edges.iter().enumerate() {
        outgoing.push((e.0, e.1, i))?;
    }
    outgoing.sort_unstable();
    let mut used = Vector::with_capacity(budget, edges.len())?;
    used.resize(edges.len(), false)?;
    let mut walk = Vector::with_capacity(budget, edges.len())?;
    let mut out = Nested::with_capacity(budget, 0)?;
    let mut points = 0usize;
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        let (sx, sy, mut dx, mut dy) = edges[first];
        let (mut x, mut y) = (sx + dx, sy + dy);
        walk.truncate(0);
        walk.push(first)?;
        used[first] = true;
        while (x, y) != (sx, sy) {
            let start = outgoing.partition_point(|&(a, b, _)| (a, b) < (x, y));
            let end = start + outgoing[start..].partition_point(|&(a, b, _)| (a, b) == (x, y));
            let candidates = &outgoing[start..end];
            if candidates.is_empty() {
                return Err(invalid("outline vertex has no outgoing edge"));
            }
            let mut chosen = None;
            for want in [(dy, -dx), (dx, dy), (-dy, dx)] {
                for &(_, _, i) in candidates {
                    if !used[i] && (edges[i].2, edges[i].3) == want {
                        chosen = Some(i);
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }
            let i = chosen.ok_or_else(|| invalid("outline tracing failed"))?;
            used[i] = true;
            walk.push(i)?;
            dx = edges[i].2;
            dy = edges[i].3;
            x += dx;
            y += dy;
        }
        let corner = |k: usize| {
            let e = edges[walk[k]];
            let previous = edges[walk[(k + walk.len() - 1) % walk.len()]];
            ((previous.2, previous.3) != (e.2, e.3)).then_some((e.0, e.1))
        };
        let count = (0..walk.len()).filter_map(corner).count();
        points = points.saturating_add(count);
        if points > limits.max_points {
            return Err(invalid("outline point budget exceeded"));
        }
        let first_corner = (0..walk.len())
            .filter_map(|k| corner(k).map(|v| (k, v)))
            .min_by_key(|(_, (x, y))| (*y, *x))
            .map(|(k, _)| k)
            .unwrap_or(0);
        let mut contour = Vector::with_capacity(budget, count)?;
        for offset in 0..walk.len() {
            if let Some((x, y)) = corner((first_corner + offset) % walk.len()) {
                contour.push((((xoff + x) * ppu) as f64, ((yoff + y) * ppu) as f64, true))?;
            }
        }
        out.push(contour.into_owned())?;
    }
    out.into_owned()
}
