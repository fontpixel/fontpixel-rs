# Native outline geometry

`contours` generates clockwise merged-square contours, independent squares, or
Tong's four-quadratic round dots. Inputs are packed Mono bitmaps with explicit
scale, shape, diameter and baseline-relative origin. Budgets bound pixels,
tracing edges, points and raster intersections. `pixel_contour` and
`pixel_origins` support one reusable circle in native glyf composites.

`spec_bitmap_contours` is the shared bdfparser-compatible adapter: it retains the
SPEC start/order/rounding rules and eight-off-curve-circle convention. The facade
continues to use its historical 16-chord rasterizer for exact parity.

Native `rasterize` uses nonzero winding, implicit quadratic midpoints and direct
quadratic evaluation on monotone curve sections, without chord approximation.
It is an unhinted geometric fill, not a font-grid detector or TrueType interpreter.
