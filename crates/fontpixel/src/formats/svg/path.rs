use super::*;
use crate::outline::path::{Edge, Point};
/// Parse all SVG path commands; validate operands before arc conversion.
pub(super) fn parse(s: &str, l: &Limits) -> Result<Vec<Edge>> {
    use svgtypes::{PathSegment as P, SimplePathSegment as S};
    bound(s.len(), l.resources.max_table_bytes, "SVG path bytes")?;
    let (mut current, mut first) = ((0.0f64, 0.0f64), (0.0f64, 0.0f64));
    for (i, segment) in svgtypes::PathParser::from(s).enumerate() {
        bound(i + 1, l.geometry.max_edges, "SVG path commands")?;
        let segment = segment.map_err(invalid)?;
        let values = match segment {
            P::MoveTo { x, y, .. } | P::LineTo { x, y, .. } | P::SmoothQuadratic { x, y, .. } => {
                vec![x, y]
            }
            P::HorizontalLineTo { x, .. } | P::VerticalLineTo { y: x, .. } => vec![x],
            P::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
                ..
            } => vec![x1, y1, x2, y2, x, y],
            P::SmoothCurveTo { x2, y2, x, y, .. } => vec![x2, y2, x, y],
            P::Quadratic { x1, y1, x, y, .. } => vec![x1, y1, x, y],
            P::EllipticalArc {
                rx,
                ry,
                x_axis_rotation,
                x,
                y,
                ..
            } => vec![rx, ry, x_axis_rotation, x, y],
            P::ClosePath { .. } => Vec::new(),
        };
        if values.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
            return Err(invalid("nonfinite/excessive path operand"));
        }
        let endpoint = match segment {
            P::MoveTo { abs, x, y }
            | P::LineTo { abs, x, y }
            | P::SmoothQuadratic { abs, x, y }
            | P::CurveTo { abs, x, y, .. }
            | P::SmoothCurveTo { abs, x, y, .. }
            | P::Quadratic { abs, x, y, .. }
            | P::EllipticalArc { abs, x, y, .. } => {
                if abs {
                    (x, y)
                } else {
                    (current.0 + x, current.1 + y)
                }
            }
            P::HorizontalLineTo { abs, x } => (if abs { x } else { current.0 + x }, current.1),
            P::VerticalLineTo { abs, y } => (current.0, if abs { y } else { current.1 + y }),
            P::ClosePath { .. } => first,
        };
        if !endpoint.0.is_finite()
            || !endpoint.1.is_finite()
            || endpoint.0.abs() > 1e9
            || endpoint.1.abs() > 1e9
        {
            return Err(invalid("excessive resolved endpoint"));
        }
        if let P::EllipticalArc { rx, ry, .. } = segment {
            let (small, large) = (rx.abs().min(ry.abs()), rx.abs().max(ry.abs()));
            // Endpoint-to-center correction can enlarge tiny/anisotropic radii.
            // Bound it before svgtypes/kurbo allocates its subdivision buffer.
            if small > 0. {
                let distance = (endpoint.0 - current.0).hypot(endpoint.1 - current.1) / 2.;
                let expanded = large * (distance / small).max(1.);
                if !expanded.is_finite() || expanded > 1e9 {
                    return Err(invalid("arc radius expansion exceeds coordinate budget"));
                }
                let subdivisions = (1.1163 * expanded / 0.1).powf(1. / 6.).max(4.).ceil() as usize;
                bound(subdivisions, l.geometry.max_edges, "SVG arc subdivisions")?;
            }
        }
        if matches!(segment, P::MoveTo { .. }) {
            first = endpoint;
        }
        current = endpoint;
    }
    let mut out = Vec::new();
    let (mut p, mut start, mut open) = ((0., 0.), (0., 0.), false);
    let push = |out: &mut Vec<Edge>, e: Edge| -> Result<()> {
        bound(out.len() + 1, l.geometry.max_edges, "SVG path edges")?;
        if e.points()
            .iter()
            .any(|&(x, y)| !x.is_finite() || !y.is_finite() || x.abs() > 1e9 || y.abs() > 1e9)
        {
            return Err(invalid("nonfinite/excessive expanded path"));
        }
        out.push(e);
        Ok(())
    };
    for (i, segment) in svgtypes::SimplifyingPathParser::from(s).enumerate() {
        bound(i + 1, l.geometry.max_points, "SVG expanded commands")?;
        match segment.map_err(invalid)? {
            S::MoveTo { x, y } => {
                if !x.is_finite() || !y.is_finite() || x.abs() > 1e9 || y.abs() > 1e9 {
                    return Err(invalid("excessive path move"));
                }
                if open && p != start {
                    push(&mut out, Edge::Line(p, start))?;
                }
                p = (x, y);
                start = p;
                open = true;
            }
            S::LineTo { x, y } => {
                push(&mut out, Edge::Line(p, (x, y)))?;
                p = (x, y);
            }
            S::Quadratic { x1, y1, x, y } => {
                push(&mut out, Edge::Quad(p, (x1, y1), (x, y)))?;
                p = (x, y);
            }
            S::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                push(&mut out, Edge::Cubic(p, (x1, y1), (x2, y2), (x, y)))?;
                p = (x, y);
            }
            S::ClosePath => {
                if p != start {
                    push(&mut out, Edge::Line(p, start))?;
                }
                p = start;
                open = false;
            }
        }
    }
    if open && p != start {
        push(&mut out, Edge::Line(p, start))?;
    }
    Ok(out)
}
pub(super) fn shift(edges: &mut [Edge], origin: Point) {
    let p = |p: &mut Point| {
        p.0 -= origin.0;
        p.1 -= origin.1;
    };
    for e in edges {
        match e {
            Edge::Line(a, b) => {
                p(a);
                p(b);
            }
            Edge::Quad(a, b, c) => {
                p(a);
                p(b);
                p(c);
            }
            Edge::Cubic(a, b, c, d) => {
                p(a);
                p(b);
                p(c);
                p(d);
            }
        }
    }
}
