//! Dimension-only empty rasters must not expand into rows or geometry.
use fontpixel::{
    Bitmap,
    bitmap::{BitmapLimits, Bounds, Gray8, Mono},
    outline::{self, Limits, Options, Shape},
};

fn limits() -> BitmapLimits {
    BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: 0,
        max_bytes: 0,
    }
}

#[test]
fn zero_area_bounding_boxes_preserve_dimensions_without_scanning_empty_rows() {
    for (width, height) in [(0, u32::MAX), (u32::MAX, 0), (0, 0)] {
        let mono = Bitmap::<Mono>::with_limits(width, height, limits()).unwrap();
        let gray = Bitmap::<Gray8>::with_limits(width, height, limits()).unwrap();
        assert_eq!(mono.bbox(), Bounds::default());
        assert_eq!(gray.bbox(), Bounds::default());
        assert_eq!((mono.width(), mono.height()), (width, height));
        assert_eq!((gray.width(), gray.height()), (width, height));
    }
}

#[test]
fn zero_area_outlines_need_no_pixel_edge_or_point_allowance() {
    let geometry = Limits {
        max_pixels: 0,
        max_edges: 0,
        max_points: 0,
        max_intersections: 0,
    };
    for (width, height) in [(0, u32::MAX), (u32::MAX, 0), (0, 0)] {
        let bitmap = Bitmap::<Mono>::with_limits(width, height, limits()).unwrap();
        for shape in [Shape::Square, Shape::SquareSeparate, Shape::Dot] {
            let options = Options {
                shape,
                ..Default::default()
            };
            assert!(
                outline::contours(&bitmap, &options, &geometry)
                    .unwrap()
                    .is_empty()
            );
            assert!(
                outline::pixel_origins(&bitmap, &options, &geometry)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn empty_outline_shortcuts_preserve_option_and_square_origin_validation() {
    let bitmap = Bitmap::<Mono>::with_limits(0, u32::MAX, limits()).unwrap();
    for origin in [(i32::MIN, 0), (0, i32::MIN)] {
        let options = Options {
            origin,
            ..Default::default()
        };
        assert!(outline::contours(&bitmap, &options, &Limits::default()).is_err());
    }
    for options in [
        Options {
            units_per_pixel: 0,
            ..Default::default()
        },
        Options {
            dot_size: 0.0,
            ..Default::default()
        },
    ] {
        assert!(outline::contours(&bitmap, &options, &Limits::default()).is_err());
        assert!(outline::pixel_origins(&bitmap, &options, &Limits::default()).is_err());
    }
}
