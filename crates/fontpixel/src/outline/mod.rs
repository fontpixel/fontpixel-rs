//! Deterministic bitmap contours in upward-facing font design coordinates.
//!
//! Geometry is independent of sfnt. TrueType encoding requires integral int16
//! coordinates; composite flattening and rasterization can retain fractions.
use crate::{
    Bitmap, Error, Result,
    bitmap::BitmapLimits,
    working::{Budget, Lease, Owned, Vector},
};
mod packed;
pub mod path;
mod spec;
/// TrueType point `(x, y, on_curve)` in design units, with y upward.
pub type Point = (f64, f64, bool);
/// Closed contour, without a duplicated closing point.
pub type Contour = Vec<Point>;
/// Pixel outline shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Merge edge-connected full squares, preserving holes and separate corners.
    Square,
    /// Keep one independent square per ink pixel.
    SquareSeparate,
    /// Tong's four-quadratic circle approximation.
    Dot,
}
/// Bounded geometry generation and expansion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum source pixels or output raster pixels.
    pub max_pixels: usize,
    /// Maximum traced boundary edges.
    pub max_edges: usize,
    /// Maximum points in a generated or expanded glyph.
    pub max_points: usize,
    /// Maximum raster edge/scanline intersections.
    pub max_intersections: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pixels: 16 * 1024 * 1024,
            max_edges: 2 * 1024 * 1024,
            max_points: 1024 * 1024,
            max_intersections: 8 * 1024 * 1024,
        }
    }
}
/// Explicit bitmap-to-outline options.
#[derive(Clone, Debug)]
pub struct Options {
    /// Shape to generate.
    pub shape: Shape,
    /// Positive integer design units per pixel.
    pub units_per_pixel: i32,
    /// Dot/square side length relative to one pixel, in `(0, 1.5]`.
    pub dot_size: f64,
    /// Pixel-relative lower-left bitmap origin.
    pub origin: (i32, i32),
}
impl Default for Options {
    fn default() -> Self {
        Self {
            shape: Shape::Square,
            units_per_pixel: 100,
            dot_size: 1.0,
            origin: (0, 0),
        }
    }
}
pub(crate) fn invalid(s: &str) -> Error {
    Error::InvalidInput(s.into())
}
fn bound(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
fn validate(o: &Options) -> Result<()> {
    if o.units_per_pixel <= 0 || !(0.0 < o.dot_size && o.dot_size <= 1.5) {
        return Err(invalid("invalid outline scale or dot size"));
    }
    if o.shape == Shape::Square && o.dot_size != 1.0 {
        return Err(invalid(
            "merged squares require dot_size 1; use SquareSeparate",
        ));
    }
    Ok(())
}
/// Exact SPEC row adapter, shared with `compat::bdfparser`.
///
/// Rows use '0' for paper and any other byte for ink, matching SPEC's labels.
/// This adapter retains historical ragged-row and hard-budget behavior.
pub fn spec_bitmap_contours(
    rows: &[String],
    shape: &str,
    dotsize: f64,
    ppu: i64,
    xoff: i64,
    yoff: i64,
) -> Result<Vec<Contour>> {
    spec::spec_contours(rows, shape, dotsize, ppu, xoff, yoff, &Limits::default())
}
/// Trace a packed monochrome bitmap. Gray/label thresholding must be explicit.
pub fn contours(bitmap: &Bitmap, options: &Options, limits: &Limits) -> Result<Vec<Contour>> {
    let budget = Budget::new(usize::MAX, "outline working bytes");
    Ok(contours_with_budget(bitmap, options, limits, &budget)?
        .into_parts()
        .0)
}

pub(crate) fn contours_with_budget(
    bitmap: &Bitmap,
    options: &Options,
    limits: &Limits,
    budget: &Budget,
) -> Result<Owned<Vec<Contour>>> {
    validate(options)?;
    bound(
        (bitmap.width() as usize)
            .checked_mul(bitmap.height() as usize)
            .ok_or_else(|| invalid("outline dimensions overflow"))?,
        limits.max_pixels,
        "outline pixels",
    )?;
    if bitmap.as_bytes().is_empty() {
        // Preserve the Square origin validation even for zero-area rasters.
        if options.shape == Shape::Square
            && (options.origin.0 == i32::MIN || options.origin.1 == i32::MIN)
        {
            return Err(invalid("outline coordinate budget exceeded"));
        }
        return Ok(Vector::new(budget)?.into_owned());
    }
    if options.shape == Shape::Square {
        let vertices = packed::vertex_count(bitmap.width() as usize, bitmap.height() as usize);
        if vertices.is_some_and(|n| n <= packed::MAX_VERTICES) {
            return packed::square_with_budget(bitmap, options, limits, budget);
        }
        // The general tracer reads borrowed rows. Charge this packed-to-SPEC
        // adapter, including every row, until tracing has finished.
        let mut rows = crate::working::Nested::with_capacity(budget, bitmap.height() as usize)?;
        for y in 0..bitmap.height() {
            let mut row = Vector::with_capacity(budget, bitmap.width() as usize)?;
            for x in 0..bitmap.width() {
                row.push(if bitmap.get_pixel(x, y)? == 0 {
                    b'0'
                } else {
                    b'1'
                })?;
            }
            let (bytes, lease) = row.into_parts();
            let value = String::from_utf8(bytes).map_err(|_| invalid("invalid outline row"))?;
            rows.push(Owned::from_parts(value, lease))?;
        }
        return spec::spec_contours_with_budget(
            &rows,
            "square",
            1.0,
            i64::from(options.units_per_pixel),
            (i64::from(options.origin.0), i64::from(options.origin.1)),
            limits,
            budget,
        );
    }
    let base = pixel_contour_with_budget(options, budget)?;
    let origins = pixel_origins_with_budget(bitmap, options, limits, budget)?;
    let points = origins
        .len()
        .checked_mul(base.len())
        .ok_or_else(|| invalid("point count overflow"))?;
    bound(points, limits.max_points, "outline points")?;
    let mut out = IndependentContours::new(origins.len(), points, budget)?;
    for &(x, y) in origins.iter() {
        out.push(base.len(), base.iter().map(|p| (p.0 + x, p.1 + y, p.2)))?;
    }
    out.finish()
}

/// Independent contours admit all point storage once, instead of performing an
/// atomic admission for every ink pixel. Extra allocator capacity is checked
/// before insertion; fields drop all nested arrays before releasing the lease.
struct IndependentContours {
    values: Vector<Contour>,
    points: Lease,
    remaining: usize,
    budget: Budget,
}
impl IndependentContours {
    fn new(count: usize, points: usize, budget: &Budget) -> Result<Self> {
        let lease = budget.reserve_array::<Point>(points)?;
        Ok(Self {
            values: Vector::with_capacity(budget, count)?,
            points: lease,
            remaining: points,
            budget: budget.clone(),
        })
    }
    fn push(&mut self, count: usize, values: impl Iterator<Item = Point>) -> Result<()> {
        if count > self.remaining {
            return Err(invalid("outline point admission mismatch"));
        }
        let mut contour = Vec::new();
        contour
            .try_reserve_exact(count)
            .map_err(|_| self.budget.error())?;
        let extra = contour
            .capacity()
            .saturating_sub(count)
            .checked_mul(std::mem::size_of::<Point>())
            .ok_or_else(|| self.budget.error())?;
        self.points.grow_to(
            self.points
                .bytes()
                .checked_add(extra)
                .ok_or_else(|| self.budget.error())?,
        )?;
        for point in values {
            if contour.len() == count {
                return Err(invalid("outline point admission mismatch"));
            }
            contour.push(point);
        }
        if contour.len() != count {
            return Err(invalid("outline point admission mismatch"));
        }
        self.values.push(contour)?;
        self.remaining -= count;
        Ok(())
    }
    fn finish(mut self) -> Result<Owned<Vec<Contour>>> {
        let (values, mut lease) = self.values.into_parts();
        lease.merge(&mut self.points)?;
        Ok(Owned::from_parts(values, lease))
    }
}

/// A single independent square/dot inside the cell beginning at `(0, 0)`.
/// Dot control points and integer rounding match Tong's Python exporter.
pub fn pixel_contour(o: &Options) -> Result<Contour> {
    let budget = Budget::new(usize::MAX, "outline working bytes");
    Ok(pixel_contour_with_budget(o, &budget)?.into_parts().0)
}
pub(crate) fn pixel_contour_with_budget(o: &Options, budget: &Budget) -> Result<Owned<Contour>> {
    validate(o)?;
    let center = f64::from(o.units_per_pixel) / 2.0;
    let radius = o.dot_size * center;
    let round = |v: f64| (v + 0.5).floor();
    let mut out = Vector::with_capacity(budget, if o.shape == Shape::Dot { 8 } else { 4 })?;
    if o.shape != Shape::Dot {
        out.extend(&[
            (round(center - radius), round(center - radius), true),
            (round(center - radius), round(center + radius), true),
            (round(center + radius), round(center + radius), true),
            (round(center + radius), round(center - radius), true),
        ])?;
    } else {
        let c = 0.9142135623730951 * radius;
        for (x, y, on) in [
            (radius, 0.0, true),
            (c, c, false),
            (0.0, radius, true),
            (-c, c, false),
            (-radius, 0.0, true),
            (-c, -c, false),
            (0.0, -radius, true),
            (c, -c, false),
        ] {
            out.push((round(center + x), round(center + y), on))?;
        }
    }
    Ok(out.into_owned())
}
/// Lower-left positions of ink cells, in stable top-to-bottom source order.
/// Useful for shared-dot composite glyphs.
pub fn pixel_origins(bitmap: &Bitmap, o: &Options, limits: &Limits) -> Result<Vec<(f64, f64)>> {
    let budget = Budget::new(usize::MAX, "outline working bytes");
    Ok(pixel_origins_with_budget(bitmap, o, limits, &budget)?
        .into_parts()
        .0)
}
pub(crate) fn pixel_origins_with_budget(
    bitmap: &Bitmap,
    o: &Options,
    limits: &Limits,
    budget: &Budget,
) -> Result<Owned<Vec<(f64, f64)>>> {
    validate(o)?;
    bound(
        (bitmap.width() as usize)
            .checked_mul(bitmap.height() as usize)
            .ok_or_else(|| invalid("outline dimensions overflow"))?,
        limits.max_pixels,
        "outline pixels",
    )?;
    if bitmap.as_bytes().is_empty() {
        return Ok(Vector::new(budget)?.into_owned());
    }
    let mut count = 0usize;
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            count += usize::from(bitmap.get_pixel(x, y)? != 0);
        }
    }
    bound(count, limits.max_points, "outline components")?;
    let mut result = Vector::with_capacity(budget, count)?;
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            if bitmap.get_pixel(x, y)? != 0 {
                result.push((
                    (f64::from(o.origin.0) + f64::from(x)) * f64::from(o.units_per_pixel),
                    (f64::from(o.origin.1) + f64::from(bitmap.height() - y - 1))
                        * f64::from(o.units_per_pixel),
                ))?;
            }
        }
    }
    Ok(result.into_owned())
}
/// Outline segments with implied on-curve midpoints resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    /// Straight edge.
    Line((f64, f64), (f64, f64)),
    /// Quadratic Bézier start/control/end.
    Quad((f64, f64), (f64, f64), (f64, f64)),
}
/// Resolve a contour's closing segment and consecutive off-curve points.
pub fn segments(c: &[Point]) -> Result<Vec<Segment>> {
    if c.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
        return Err(invalid("non-finite outline point"));
    }
    if c.is_empty() {
        return Ok(Vec::new());
    }
    let (start, index) = if c[0].2 {
        ((c[0].0, c[0].1), 1)
    } else if c[c.len() - 1].2 {
        ((c[c.len() - 1].0, c[c.len() - 1].1), 0)
    } else {
        (
            (
                (c[c.len() - 1].0 + c[0].0) / 2.0,
                (c[c.len() - 1].1 + c[0].1) / 2.0,
            ),
            0,
        )
    };
    let mut prev = start;
    let mut control = None::<(f64, f64)>;
    let mut out = Vec::new();
    let end = if !c[0].2 && c[c.len() - 1].2 {
        c.len() - 1
    } else {
        c.len()
    };
    for &(x, y, on) in c[index..end]
        .iter()
        .chain(std::iter::once(&(start.0, start.1, true)))
    {
        if on {
            out.push(if let Some(ctrl) = control.take() {
                Segment::Quad(prev, ctrl, (x, y))
            } else {
                Segment::Line(prev, (x, y))
            });
            prev = (x, y);
        } else {
            if let Some(ctrl) = control.replace((x, y)) {
                let mid = ((ctrl.0 + x) / 2.0, (ctrl.1 + y) / 2.0);
                out.push(Segment::Quad(prev, ctrl, mid));
                prev = mid;
            }
        }
    }
    Ok(out)
}
/// Unhinted nonzero-winding pixel-centre fill with quadratic crossings (without chord approximation).
/// `origin` and dimensions are in pixels; `units_per_pixel` is positive.
pub fn rasterize(
    cs: &[Contour],
    units_per_pixel: f64,
    origin: (i32, i32),
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<Bitmap> {
    if !units_per_pixel.is_finite() || units_per_pixel <= 0.0 {
        return Err(invalid("invalid raster scale"));
    }
    let mut points = 0usize;
    for c in cs {
        points = points
            .checked_add(c.len())
            .ok_or_else(|| invalid("point count overflow"))?;
    }
    bound(points, limits.max_points, "raster points")?;
    let mut out = Bitmap::with_limits(
        width,
        height,
        BitmapLimits {
            max_pixels: limits.max_pixels,
            ..Default::default()
        },
    )?;
    let mut crossings = vec![Vec::<(f64, i32)>::new(); height as usize];
    let mut intersections = 0usize;
    let mut add = |a: (f64, f64), b: (f64, f64), control: Option<(f64, f64)>| -> Result<()> {
        let low = a.1.min(b.1);
        let high = a.1.max(b.1);
        if low == high {
            return Ok(());
        }
        let begin = (((low / units_per_pixel) - f64::from(origin.1) - 0.5)
            .ceil()
            .max(0.0) as usize)
            .min(height as usize);
        let end = (((high / units_per_pixel) - f64::from(origin.1) - 0.5)
            .ceil()
            .max(0.0) as usize)
            .min(height as usize);
        bound(
            intersections.saturating_add(end - begin),
            limits.max_intersections,
            "raster intersections",
        )?;
        intersections += end - begin;
        for (row, cr) in crossings.iter_mut().enumerate().take(end).skip(begin) {
            let y = (f64::from(origin.1) + row as f64 + 0.5) * units_per_pixel;
            let t = if let Some(c) = control {
                // Monotone quadratic subsegments: bisection avoids cancellation
                // near linear/degenerate roots and is deterministic.
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..54 {
                    let m = (lo + hi) / 2.0;
                    let v = (1.0 - m) * (1.0 - m) * a.1 + 2.0 * (1.0 - m) * m * c.1 + m * m * b.1;
                    if (v < y) == (a.1 < b.1) {
                        lo = m;
                    } else {
                        hi = m;
                    }
                }
                (lo + hi) / 2.0
            } else {
                (y - a.1) / (b.1 - a.1)
            };
            let x = if let Some(c) = control {
                (1.0 - t) * (1.0 - t) * a.0 + 2.0 * (1.0 - t) * t * c.0 + t * t * b.0
            } else {
                a.0 + t * (b.0 - a.0)
            };
            cr.push((x, if a.1 < b.1 { 1 } else { -1 }));
        }
        Ok(())
    };
    for c in cs {
        for segment in segments(c)? {
            match segment {
                Segment::Line(a, b) => add(a, b, None)?,
                Segment::Quad(a, c, b) => {
                    let denom = a.1 - 2.0 * c.1 + b.1;
                    let t = if denom == 0.0 {
                        0.0
                    } else {
                        (a.1 - c.1) / denom
                    };
                    if t > 0.0 && t < 1.0 {
                        let lerp = |a: (f64, f64), b: (f64, f64)| {
                            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
                        };
                        let ac = lerp(a, c);
                        let cb = lerp(c, b);
                        let mid = lerp(ac, cb);
                        add(a, mid, Some(ac))?;
                        add(mid, b, Some(cb))?;
                    } else {
                        add(a, b, Some(c))?;
                    }
                }
            }
        }
    }
    for (y, cr) in crossings.iter_mut().enumerate() {
        cr.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut winding = 0;
        let mut i = 0;
        for x in 0..width {
            let center = (f64::from(origin.0) + f64::from(x) + 0.5) * units_per_pixel;
            while i < cr.len() && cr[i].0 <= center {
                winding += cr[i].1;
                i += 1;
            }
            if winding != 0 {
                out.set_pixel(x, height - 1 - y as u32, 1)?;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod ledger_tests {
    use super::*;
    use std::mem::size_of;

    /// Run on an unlimited ledger, then prove its high-water mark is the exact
    /// admission threshold: the same result at the peak, `ResourceLimit` one
    /// byte below it, and every charge released afterwards.
    fn exact<T: std::fmt::Debug + PartialEq>(run: impl Fn(&Budget) -> Result<Owned<T>>) -> usize {
        let open = Budget::new(usize::MAX, "outline working bytes");
        let expected = run(&open).unwrap().into_parts().0;
        assert_eq!(open.remaining(), usize::MAX);
        let peak = open.peak();
        assert!(peak > 0);
        let fits = Budget::new(peak, "outline working bytes");
        assert_eq!(run(&fits).unwrap().into_parts().0, expected);
        assert_eq!(fits.remaining(), peak);
        let short = Budget::new(peak - 1, "outline working bytes");
        assert!(matches!(run(&short), Err(Error::ResourceLimit { .. })));
        assert_eq!(short.remaining(), peak - 1);
        peak
    }

    fn square(o: &Options) -> Options {
        Options {
            origin: (-2, 3),
            ..o.clone()
        }
    }

    #[test]
    fn packed_square_admits_its_grid_edges_and_contours_exactly() {
        let bitmap: Bitmap = "##\n##".parse().unwrap();
        let o = square(&Options::default());
        let peak = exact(|b| contours_with_budget(&bitmap, &o, &Limits::default(), b));
        // Edge capacity 2wh+w+h, the (w+1)(h+1) vertex grid, used/walk arrays
        // over the 8 boundary edges, one contour slot and its 4 points.
        let expected = 12 * size_of::<(usize, usize, i8, i8)>()
            + 9 * size_of::<[usize; 2]>()
            + 8
            + size_of::<[usize; 8]>()
            + size_of::<Contour>()
            + 4 * size_of::<Point>();
        assert_eq!(peak, expected);
    }

    #[test]
    fn general_square_tracer_admits_rows_edges_and_contours_exactly() {
        // 70x70 exceeds the packed vertex grid and uses the SPEC row adapter.
        let mut bitmap = Bitmap::new(70, 70).unwrap();
        for (x, y) in [(0, 0), (1, 0), (69, 69), (35, 20), (36, 21)] {
            bitmap.set_pixel(x, y, 1).unwrap();
        }
        let o = square(&Options::default());
        exact(|b| contours_with_budget(&bitmap, &o, &Limits::default(), b));
    }

    #[test]
    fn independent_squares_and_dots_admit_points_before_contours() {
        let bitmap: Bitmap = "#.\n.#".parse().unwrap();
        for (shape, dot_size, per) in [(Shape::SquareSeparate, 0.8, 4), (Shape::Dot, 0.9, 8)] {
            let o = Options {
                shape,
                dot_size,
                ..square(&Options::default())
            };
            let peak = exact(|b| contours_with_budget(&bitmap, &o, &Limits::default(), b));
            // Base contour, two origins, two contour slots and their points.
            let expected = per * size_of::<Point>()
                + 2 * size_of::<(f64, f64)>()
                + 2 * size_of::<Contour>()
                + 2 * per * size_of::<Point>();
            assert_eq!(peak, expected, "{shape:?}");
        }
    }

    #[test]
    fn origins_and_single_contours_admit_exact_arrays() {
        let bitmap: Bitmap = "#.#\n.#.".parse().unwrap();
        let o = Options {
            shape: Shape::Dot,
            dot_size: 0.9,
            ..Options::default()
        };
        assert_eq!(
            exact(|b| pixel_origins_with_budget(&bitmap, &o, &Limits::default(), b)),
            3 * size_of::<(f64, f64)>()
        );
        assert_eq!(
            exact(|b| pixel_contour_with_budget(&o, b)),
            8 * size_of::<Point>()
        );
    }

    #[test]
    fn point_admission_precedes_contour_allocation() {
        // A failed point admission leaves nothing charged and no partial output.
        let bitmap: Bitmap = "###\n###".parse().unwrap();
        let o = Options {
            shape: Shape::Dot,
            dot_size: 0.9,
            ..Options::default()
        };
        let open = Budget::new(usize::MAX, "outline working bytes");
        drop(contours_with_budget(&bitmap, &o, &Limits::default(), &open).unwrap());
        let points = 6 * 8 * size_of::<Point>();
        let tight = Budget::new(open.peak() - points, "outline working bytes");
        assert!(matches!(
            contours_with_budget(&bitmap, &o, &Limits::default(), &tight),
            Err(Error::ResourceLimit { .. })
        ));
        // Only the base contour and origins were admitted before the refusal.
        assert_eq!(
            tight.peak(),
            8 * size_of::<Point>() + 6 * size_of::<(f64, f64)>()
        );
        assert_eq!(tight.remaining(), open.peak() - points);
    }
}
