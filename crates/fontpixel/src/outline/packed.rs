//! Packed small-bitmap tracing with the exact shared SPEC edge/walk order.
use super::{Contour, Limits, Options, invalid};
use crate::{Bitmap, Result};

/// Bound the dense vertex map. Large/sparse inputs retain the general tracer.
pub(super) const MAX_VERTICES: usize = 4096;

pub(super) fn square(bitmap: &Bitmap, o: &Options, limits: &Limits) -> Result<Vec<Contour>> {
    if o.origin.0 == i32::MIN || o.origin.1 == i32::MIN {
        return Err(invalid("outline coordinate budget exceeded"));
    }
    let w = bitmap.width() as usize;
    let h = bitmap.height() as usize;
    let stride = bitmap.stride();
    let data = bitmap.as_bytes();
    let ink = |x: usize, y: usize| data[(h - 1 - y) * stride + x / 8] & (0x80 >> (x % 8)) != 0;
    let mut edges =
        Vec::<(usize, usize, i8, i8)>::with_capacity((w * h * 2 + w + h).min(limits.max_edges));
    // Identical bottom-to-top scan and bottom/left/top/right insertion order.
    for r in 0..h {
        for c in 0..w {
            if !ink(c, r) {
                continue;
            }
            if edges.len().saturating_add(4) > limits.max_edges {
                return Err(invalid("outline edge budget exceeded"));
            }
            if r == 0 || !ink(c, r - 1) {
                edges.push((c + 1, r, -1, 0));
            }
            if c == 0 || !ink(c - 1, r) {
                edges.push((c, r, 0, 1));
            }
            if r + 1 == h || !ink(c, r + 1) {
                edges.push((c, r + 1, 1, 0));
            }
            if c + 1 == w || !ink(c + 1, r) {
                edges.push((c + 1, r + 1, 0, -1));
            }
        }
    }
    if edges.is_empty() {
        return Ok(Vec::new());
    }
    // A rectilinear pixel-boundary vertex has at most two outgoing edges.
    // Preserve insertion order for the corner-touching tie rule.
    let mut outgoing = vec![[usize::MAX; 2]; (w + 1) * (h + 1)];
    for (i, &(x, y, _, _)) in edges.iter().enumerate() {
        let at = &mut outgoing[y * (w + 1) + x];
        if at[0] == usize::MAX {
            at[0] = i;
        } else if at[1] == usize::MAX {
            at[1] = i;
        } else {
            return Err(invalid("outline vertex has too many outgoing edges"));
        }
    }
    let mut used = vec![false; edges.len()];
    let mut out = Vec::new();
    let mut points = 0usize;
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        let (sx, sy, mut dx, mut dy) = edges[first];
        let (mut x, mut y) = (
            sx.checked_add_signed(isize::from(dx))
                .ok_or_else(|| invalid("outline tracing failed"))?,
            sy.checked_add_signed(isize::from(dy))
                .ok_or_else(|| invalid("outline tracing failed"))?,
        );
        let mut walk = vec![first];
        used[first] = true;
        while (x, y) != (sx, sy) {
            let candidates = outgoing
                .get(y * (w + 1) + x)
                .ok_or_else(|| invalid("outline vertex has no outgoing edge"))?;
            let mut chosen = None;
            for want in [(dy, -dx), (dx, dy), (-dy, dx)] {
                for &i in candidates {
                    if i != usize::MAX && !used[i] && (edges[i].2, edges[i].3) == want {
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
            walk.push(i);
            dx = edges[i].2;
            dy = edges[i].3;
            x = x
                .checked_add_signed(isize::from(dx))
                .ok_or_else(|| invalid("outline tracing failed"))?;
            y = y
                .checked_add_signed(isize::from(dy))
                .ok_or_else(|| invalid("outline tracing failed"))?;
        }
        let mut vertices = Vec::new();
        for k in 0..walk.len() {
            let e = edges[walk[k]];
            let previous = edges[walk[(k + walk.len() - 1) % walk.len()]];
            if (previous.2, previous.3) != (e.2, e.3) {
                vertices.push((e.0, e.1));
            }
        }
        let best = vertices
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| (v.1, v.0))
            .map(|(i, _)| i)
            .unwrap_or(0);
        vertices.rotate_left(best);
        points = points.saturating_add(vertices.len());
        if points > limits.max_points {
            return Err(invalid("outline point budget exceeded"));
        }
        out.push(
            vertices
                .into_iter()
                .map(|(x, y)| {
                    (
                        ((i64::from(o.origin.0) + x as i64) * i64::from(o.units_per_pixel)) as f64,
                        ((i64::from(o.origin.1) + y as i64) * i64::from(o.units_per_pixel)) as f64,
                        true,
                    )
                })
                .collect(),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::spec;

    #[test]
    fn every_three_by_three_bitmap_has_exact_spec_order() {
        for bits in 0..512u32 {
            let mut bitmap = Bitmap::new(3, 3).unwrap();
            let mut rows = Vec::new();
            for y in 0..3 {
                let mut row = String::new();
                for x in 0..3 {
                    let ink = ((bits >> (y * 3 + x)) & 1) as u8;
                    bitmap.set_pixel(x, y, ink).unwrap();
                    row.push(if ink == 0 { '0' } else { '1' });
                }
                rows.push(row);
            }
            let o = Options {
                origin: (-4, 3),
                ..Default::default()
            };
            let limits = Limits::default();
            assert_eq!(
                square(&bitmap, &o, &limits).unwrap(),
                spec::spec_contours(&rows, "square", 1., 100, -4, 3, &limits).unwrap(),
                "{bits}"
            );
        }
    }

    #[test]
    fn conservative_edge_and_point_limits_match_reference() {
        let bitmap: Bitmap = "#.#.#\n.###.\n##.##\n.###.\n#.#.#".parse().unwrap();
        let rows = ["10101", "01110", "11011", "01110", "10101"].map(String::from);
        for edges in 0..84 {
            for points in [0, 4, 12, 32, 100] {
                let limits = Limits {
                    max_edges: edges,
                    max_points: points,
                    ..Default::default()
                };
                let a = square(&bitmap, &Options::default(), &limits);
                let b = spec::spec_contours(&rows, "square", 1., 100, 0, 0, &limits);
                assert_eq!(format!("{a:?}"), format!("{b:?}"));
            }
        }
    }
}
