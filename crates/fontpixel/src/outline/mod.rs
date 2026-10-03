//! Deterministic bitmap contours in upward-facing font design coordinates.
//!
//! Geometry is independent of sfnt. TrueType encoding requires integral int16
//! coordinates; composite flattening and rasterization can retain fractions.
use crate::{Bitmap, Error, Result, bitmap::BitmapLimits};
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
    validate(options)?;
    bound(
        (bitmap.width() as usize)
            .checked_mul(bitmap.height() as usize)
            .ok_or_else(|| invalid("outline dimensions overflow"))?,
        limits.max_pixels,
        "outline pixels",
    )?;
    if options.shape == Shape::Square {
        let vertices = packed::vertex_count(bitmap.width() as usize, bitmap.height() as usize);
        if vertices.is_some_and(|n| n <= packed::MAX_VERTICES) {
            return packed::square(bitmap, options, limits);
        }
        let rows = (0..bitmap.height())
            .map(|y| {
                (0..bitmap.width())
                    .map(|x| {
                        if bitmap.get_pixel(x, y).unwrap_or(0) == 0 {
                            '0'
                        } else {
                            '1'
                        }
                    })
                    .collect()
            })
            .collect::<Vec<String>>();
        return spec::spec_contours(
            &rows,
            "square",
            1.0,
            i64::from(options.units_per_pixel),
            i64::from(options.origin.0),
            i64::from(options.origin.1),
            limits,
        );
    }
    let mut out = Vec::new();
    let base = pixel_contour(options)?;
    for (x, y) in pixel_origins(bitmap, options, limits)? {
        bound(
            (out.len() + 1)
                .checked_mul(base.len())
                .ok_or_else(|| invalid("point count overflow"))?,
            limits.max_points,
            "outline points",
        )?;
        out.push(base.iter().map(|p| (p.0 + x, p.1 + y, p.2)).collect());
    }
    Ok(out)
}
/// A single independent square/dot inside the cell beginning at `(0, 0)`.
/// Dot control points and integer rounding match Tong's Python exporter.
pub fn pixel_contour(o: &Options) -> Result<Contour> {
    validate(o)?;
    let center = f64::from(o.units_per_pixel) / 2.0;
    let radius = o.dot_size * center;
    let round = |v: f64| (v + 0.5).floor();
    if o.shape != Shape::Dot {
        return Ok(vec![
            (round(center - radius), round(center - radius), true),
            (round(center - radius), round(center + radius), true),
            (round(center + radius), round(center + radius), true),
            (round(center + radius), round(center - radius), true),
        ]);
    }
    let c = 0.9142135623730951 * radius;
    Ok([
        (radius, 0.0, true),
        (c, c, false),
        (0.0, radius, true),
        (-c, c, false),
        (-radius, 0.0, true),
        (-c, -c, false),
        (0.0, -radius, true),
        (c, -c, false),
    ]
    .into_iter()
    .map(|(x, y, on)| (round(center + x), round(center + y), on))
    .collect())
}
/// Lower-left positions of ink cells, in stable top-to-bottom source order.
/// Useful for shared-dot composite glyphs.
pub fn pixel_origins(bitmap: &Bitmap, o: &Options, limits: &Limits) -> Result<Vec<(f64, f64)>> {
    validate(o)?;
    bound(
        (bitmap.width() as usize)
            .checked_mul(bitmap.height() as usize)
            .ok_or_else(|| invalid("outline dimensions overflow"))?,
        limits.max_pixels,
        "outline pixels",
    )?;
    let mut result = Vec::new();
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            if bitmap.get_pixel(x, y)? != 0 {
                bound(result.len() + 1, limits.max_points, "outline components")?;
                result.push((
                    (f64::from(o.origin.0) + f64::from(x)) * f64::from(o.units_per_pixel),
                    (f64::from(o.origin.1) + f64::from(bitmap.height() - y - 1))
                        * f64::from(o.units_per_pixel),
                ));
            }
        }
    }
    Ok(result)
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
