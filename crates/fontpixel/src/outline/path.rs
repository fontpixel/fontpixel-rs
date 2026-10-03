//! Exact-center unhinted line/quadratic/cubic filling, without flattening.
use super::{Limits, bound, invalid};
use crate::{Bitmap, Result, bitmap::BitmapLimits};
/// Upward-facing absolute point.
pub type Point = (f64, f64);
/// A directed Bézier edge. Callers explicitly supply contour-closing edges.
#[derive(Clone, Copy, Debug)]
pub enum Edge {
    /// Straight segment.
    Line(Point, Point),
    /// Quadratic segment.
    Quad(Point, Point, Point),
    /// Cubic segment.
    Cubic(Point, Point, Point, Point),
}
impl Edge {
    /// Endpoints and controls, in path order.
    pub fn points(&self) -> Vec<Point> {
        match *self {
            Self::Line(a, b) => vec![a, b],
            Self::Quad(a, b, c) => vec![a, b, c],
            Self::Cubic(a, b, c, d) => vec![a, b, c, d],
        }
    }
    fn eval(self, t: f64) -> Point {
        let s = 1. - t;
        match self {
            Self::Line(a, b) => (s * a.0 + t * b.0, s * a.1 + t * b.1),
            Self::Quad(a, b, c) => (
                s * s * a.0 + 2. * s * t * b.0 + t * t * c.0,
                s * s * a.1 + 2. * s * t * b.1 + t * t * c.1,
            ),
            Self::Cubic(a, b, c, d) => (
                s * s * s * a.0 + 3. * s * s * t * b.0 + 3. * s * t * t * c.0 + t * t * t * d.0,
                s * s * s * a.1 + 3. * s * s * t * b.1 + 3. * s * t * t * c.1 + t * t * t * d.1,
            ),
        }
    }
    fn extrema(self) -> Vec<f64> {
        let mut t = vec![0., 1.];
        match self {
            Self::Line(..) => {}
            Self::Quad(a, b, c) => {
                let d = a.1 - 2. * b.1 + c.1;
                if d != 0. {
                    t.push((a.1 - b.1) / d);
                }
            }
            Self::Cubic(a, b, c, d) => {
                let aa = -a.1 + 3. * b.1 - 3. * c.1 + d.1;
                let bb = 2. * (a.1 - 2. * b.1 + c.1);
                let cc = b.1 - a.1;
                if aa == 0. {
                    if bb != 0. {
                        t.push(-cc / bb);
                    }
                } else {
                    let disc = bb * bb - 4. * aa * cc;
                    if disc >= 0. {
                        let root = disc.sqrt();
                        let q = -0.5 * (bb + root.copysign(bb));
                        if q != 0. {
                            t.push(q / aa);
                            t.push(cc / q);
                        } else {
                            t.push(-bb / (2. * aa));
                        }
                    }
                }
            }
        }
        t.retain(|t| t.is_finite() && (0.0..=1.0).contains(t));
        t.sort_by(f64::total_cmp);
        t.dedup();
        t
    }
}
/// Nonzero-winding fill at exact pixel centers. A boundary crossing at the center
/// is included on its right, excluded on its left, consistently with SPEC §7.4.
pub fn rasterize(
    edges: &[Edge],
    scale: f64,
    origin: Point,
    width: u32,
    height: u32,
    l: &Limits,
) -> Result<Bitmap> {
    if !scale.is_finite() || scale <= 0. || !origin.0.is_finite() || !origin.1.is_finite() {
        return Err(invalid("invalid path raster scale/origin"));
    }
    bound(edges.len(), l.max_edges, "path edges")?;
    // Include row-vector overhead so a zero-width huge-height request is bounded.
    bound(
        (width as usize)
            .saturating_mul(height as usize)
            .saturating_add(height as usize),
        l.max_pixels,
        "path pixels and rows",
    )?;
    let mut out = Bitmap::with_limits(
        width,
        height,
        BitmapLimits {
            max_pixels: l.max_pixels,
            ..Default::default()
        },
    )?;
    let mut rows = vec![Vec::<(f64, i32)>::new(); height as usize];
    let mut intersections = 0usize;
    for &e in edges {
        if e.points()
            .iter()
            .any(|p| !p.0.is_finite() || !p.1.is_finite() || p.0.abs() > 1e9 || p.1.abs() > 1e9)
        {
            return Err(invalid("nonfinite/excessive path coordinate"));
        }
        for t in e.extrema().windows(2) {
            let a = e.eval(t[0]);
            let b = e.eval(t[1]);
            let low = a.1.min(b.1);
            let high = a.1.max(b.1);
            if low == high {
                continue;
            }
            let begin =
                (((low / scale) - origin.1 - 0.5).ceil().max(0.) as usize).min(height as usize);
            let end =
                (((high / scale) - origin.1 - 0.5).ceil().max(0.) as usize).min(height as usize);
            intersections = intersections
                .checked_add(end - begin)
                .ok_or_else(|| invalid("intersection overflow"))?;
            bound(intersections, l.max_intersections, "path intersections")?;
            for (row, crossings) in rows.iter_mut().enumerate().take(end).skip(begin) {
                let y = (origin.1 + row as f64 + 0.5) * scale;
                let x = match e {
                    Edge::Line(p, q) => p.0 + (y - p.1) / (q.1 - p.1) * (q.0 - p.0),
                    _ => {
                        let (mut lo, mut hi) = (t[0], t[1]);
                        for _ in 0..54 {
                            let m = (lo + hi) * 0.5;
                            if (e.eval(m).1 < y) == (a.1 < b.1) {
                                lo = m;
                            } else {
                                hi = m;
                            }
                        }
                        e.eval((lo + hi) * 0.5).0
                    }
                };
                crossings.push((x, if a.1 < b.1 { 1 } else { -1 }));
            }
        }
    }
    for (y, row) in rows.iter_mut().enumerate() {
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (mut winding, mut i) = (0i32, 0usize);
        for x in 0..width {
            let center = (origin.0 + x as f64 + 0.5) * scale;
            while i < row.len() && row[i].0 <= center {
                winding += row[i].1;
                i += 1;
            }
            if winding != 0 {
                out.set_pixel(x, height - 1 - y as u32, 1)?;
            }
        }
    }
    Ok(out)
}
