//! Packed small-bitmap tracing with the exact shared SPEC edge/walk order.
use super::{Contour, Limits, Options, invalid};
use crate::{
    Bitmap, Result,
    working::{Budget, Nested, Owned, Vector},
};

/// Bound the dense vertex map. Large/sparse inputs retain the general tracer.
pub(super) const MAX_VERTICES: usize = 4096;

/// Check additions as well as multiplication, including zero-area bitmaps whose
/// explicitly raised dimensions can reach usize::MAX on 32-bit targets.
pub(super) fn vertex_count(width: usize, height: usize) -> Option<usize> {
    width.checked_add(1)?.checked_mul(height.checked_add(1)?)
}

#[cfg(test)]
pub(super) fn square(bitmap: &Bitmap, o: &Options, limits: &Limits) -> Result<Vec<Contour>> {
    let budget = Budget::new(usize::MAX, "outline working bytes");
    Ok(square_with_budget(bitmap, o, limits, &budget)?
        .into_parts()
        .0)
}

pub(super) fn square_with_budget(
    bitmap: &Bitmap,
    o: &Options,
    limits: &Limits,
    budget: &Budget,
) -> Result<Owned<Vec<Contour>>> {
    if o.origin.0 == i32::MIN || o.origin.1 == i32::MIN {
        return Err(invalid("outline coordinate budget exceeded"));
    }
    let w = bitmap.width() as usize;
    let h = bitmap.height() as usize;
    let stride = bitmap.stride();
    let data = bitmap.as_bytes();
    let ink = |x: usize, y: usize| data[(h - 1 - y) * stride + x / 8] & (0x80 >> (x % 8)) != 0;
    let mut edges = Vector::<(usize, usize, i8, i8)>::with_capacity(
        budget,
        (w * h * 2 + w + h).min(limits.max_edges),
    )?;
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
                edges.push((c + 1, r, -1, 0))?;
            }
            if c == 0 || !ink(c - 1, r) {
                edges.push((c, r, 0, 1))?;
            }
            if r + 1 == h || !ink(c, r + 1) {
                edges.push((c, r + 1, 1, 0))?;
            }
            if c + 1 == w || !ink(c + 1, r) {
                edges.push((c + 1, r + 1, 0, -1))?;
            }
        }
    }
    if edges.is_empty() {
        return Ok(Vector::new(budget)?.into_owned());
    }
    // A rectilinear pixel-boundary vertex has at most two outgoing edges.
    // Preserve insertion order for the corner-touching tie rule.
    let mut outgoing = Vector::with_capacity(budget, (w + 1) * (h + 1))?;
    outgoing.resize((w + 1) * (h + 1), [usize::MAX; 2])?;
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
        let (mut x, mut y) = (
            sx.checked_add_signed(isize::from(dx))
                .ok_or_else(|| invalid("outline tracing failed"))?,
            sy.checked_add_signed(isize::from(dy))
                .ok_or_else(|| invalid("outline tracing failed"))?,
        );
        walk.truncate(0);
        walk.push(first)?;
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
            walk.push(i)?;
            dx = edges[i].2;
            dy = edges[i].3;
            x = x
                .checked_add_signed(isize::from(dx))
                .ok_or_else(|| invalid("outline tracing failed"))?;
            y = y
                .checked_add_signed(isize::from(dy))
                .ok_or_else(|| invalid("outline tracing failed"))?;
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
        let mut contour = Vector::with_capacity(budget, count)?;
        // Choose the start in the original integer grid, before f64 conversion.
        let first_corner = (0..walk.len())
            .filter_map(|k| corner(k).map(|v| (k, v)))
            .min_by_key(|(_, (x, y))| (*y, *x))
            .map(|(k, _)| k)
            .unwrap_or(0);
        for offset in 0..walk.len() {
            if let Some((x, y)) = corner((first_corner + offset) % walk.len()) {
                contour.push((
                    ((i64::from(o.origin.0) + x as i64) * i64::from(o.units_per_pixel)) as f64,
                    ((i64::from(o.origin.1) + y as i64) * i64::from(o.units_per_pixel)) as f64,
                    true,
                ))?;
            }
        }
        out.push(contour.into_owned())?;
    }
    out.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::spec;

    #[test]
    fn dense_grid_dimensions_are_checked_before_arithmetic() {
        for (width, height) in [
            (usize::MAX, 0),
            (0, usize::MAX),
            (usize::MAX, usize::MAX),
            (usize::MAX - 1, 1),
        ] {
            assert_eq!(vertex_count(width, height), None);
        }
        assert_eq!(vertex_count(0, 0), Some(1));
        assert_eq!(vertex_count(63, 63), Some(MAX_VERTICES));
        assert_eq!(vertex_count(4095, 0), Some(MAX_VERTICES));
        assert_eq!(vertex_count(4096, 0), Some(MAX_VERTICES + 1));
    }

    #[test]
    fn maximum_width_empty_bitmap_has_no_outline() {
        let bitmap = Bitmap::with_limits(
            u32::MAX,
            0,
            crate::bitmap::BitmapLimits {
                max_dimension: u32::MAX,
                max_pixels: 0,
                max_bytes: 0,
            },
        )
        .unwrap();
        assert!(
            crate::outline::contours(&bitmap, &Options::default(), &Limits::default())
                .unwrap()
                .is_empty()
        );
    }

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
