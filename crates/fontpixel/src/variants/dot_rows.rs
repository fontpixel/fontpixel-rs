//! Reuse existing bounded ink patterns without adding glyph IDs or changing pixels.
use crate::{
    Bitmap, Glyph, Result,
    model::GlyphBitmap,
    outline,
    sfnt::{self, glyf},
};

#[derive(Clone, Debug)]
struct Row {
    glyph: u16,
    x: i16,
    y: i16,
    width: usize,
    masks: Vec<u64>,
    ink: u32,
}

/// Selected helpers themselves stay direct circle composites, preventing cycles.
pub(super) struct Catalogue {
    rows: Vec<Row>,
    selected: Vec<bool>,
}
impl Catalogue {
    pub(super) fn new(sources: &[(&str, &Glyph)], scale: i64, include_2d: bool) -> Self {
        let mut patterns = std::collections::BTreeMap::new();
        for (id, (_, glyph)) in sources.iter().enumerate().skip(1) {
            let GlyphBitmap::Mono(bitmap) = &glyph.bitmap else {
                continue;
            };
            let ink = bitmap.bbox();
            if !include_2d && ink.height != 1 {
                continue;
            }
            if ink.width == 0
                || ink.height == 0
                || ink.width > 64
                || ink.height > 64
                || (ink.width > 1 && ink.height > 1 && (ink.width > 4 || ink.height > 4))
            {
                continue;
            }
            let mut masks = vec![0u64; ink.height as usize];
            for y in 0..ink.height {
                let row = bitmap.height() - 1 - ink.y as u32 - y;
                for x in 0..ink.width {
                    if bitmap
                        .get_pixel(ink.x as u32 + x, row)
                        .is_ok_and(|p| p != 0)
                    {
                        masks[y as usize] |= 1u64 << x;
                    }
                }
            }
            let pixels = masks.iter().map(|m| m.count_ones()).sum::<u32>();
            if pixels < 2
                || patterns.contains_key(&(ink.width, masks.clone()))
                || patterns.len() >= 256
            {
                continue;
            }
            let Some(x) = glyph
                .metrics
                .bounds
                .x
                .checked_add(ink.x)
                .and_then(|v| v.checked_mul(scale))
            else {
                continue;
            };
            let Some(y) = glyph
                .metrics
                .bounds
                .y
                .checked_add(ink.y)
                .and_then(|v| v.checked_mul(scale))
            else {
                continue;
            };
            let (Ok(x), Ok(y), Ok(glyph)) = (i16::try_from(x), i16::try_from(y), u16::try_from(id))
            else {
                continue;
            };
            patterns.insert(
                (ink.width, masks.clone()),
                Row {
                    glyph,
                    x,
                    y,
                    width: ink.width as usize,
                    masks,
                    ink: pixels,
                },
            );
        }
        let mut rows = patterns.into_values().collect::<Vec<_>>();
        rows.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.ink),
                std::cmp::Reverse(r.width),
                r.glyph,
            )
        });
        let mut selected = vec![false; sources.len()];
        for row in &rows {
            selected[usize::from(row.glyph)] = true;
        }
        Self { rows, selected }
    }

    pub(super) fn glyph(
        &self,
        id: usize,
        bitmap: &Bitmap,
        options: &outline::Options,
        dot: u16,
        limits: &glyf::Limits,
    ) -> Result<glyf::Glyph> {
        if self.selected[id] || self.rows.is_empty() {
            return glyf::Glyph::pixels(bitmap, options, dot, limits);
        }
        let origins = outline::pixel_origins(bitmap, options, &limits.geometry)?;
        // Retain the direct writer's pre-optimization per-glyph work budget.
        sfnt::bounded(origins.len(), limits.max_components, "glyf components")?;
        let coordinate = |v: f64| {
            if !v.is_finite()
                || v.fract() != 0.
                || v < f64::from(i16::MIN)
                || v > f64::from(i16::MAX)
            {
                Err(sfnt::invalid("glyf coordinate is not an integral int16"))
            } else {
                Ok(v as i16)
            }
        };
        let origins = origins
            .into_iter()
            .map(|(x, y)| Ok((coordinate(x)?, coordinate(y)?)))
            .collect::<Result<Vec<_>>>()?;
        if origins.is_empty() {
            return Ok(glyf::Glyph::Empty);
        }
        let x0 = origins.iter().map(|p| p.0).min().unwrap();
        let y0 = origins.iter().map(|p| p.1).min().unwrap();
        let x1 = origins.iter().map(|p| p.0).max().unwrap();
        let y1 = origins.iter().map(|p| p.1).max().unwrap();
        let width = ((i32::from(x1) - i32::from(x0)) / options.units_per_pixel + 1) as usize;
        let height = ((i32::from(y1) - i32::from(y0)) / options.units_per_pixel + 1) as usize;
        if width > 64 || height > 64 {
            return glyf::Glyph::pixels(bitmap, options, dot, limits);
        }
        let mut remaining = vec![0u64; height];
        for &(x, y) in &origins {
            let x = ((i32::from(x) - i32::from(x0)) / options.units_per_pixel) as usize;
            let y = ((i32::from(y) - i32::from(y0)) / options.units_per_pixel) as usize;
            remaining[y] |= 1u64 << x;
        }
        let mut components = Vec::with_capacity(origins.len());
        for y in (0..height).rev() {
            while remaining[y] != 0 {
                let first = remaining[y].trailing_zeros() as usize;
                let mut chosen = None;
                for helper in &self.rows {
                    // Top row always has ink. Its first bit anchors the match;
                    // bounding-box x may lie left of that first top-row pixel.
                    let anchor = helper.masks.last().unwrap().trailing_zeros() as usize;
                    let Some(x) = first.checked_sub(anchor) else {
                        continue;
                    };
                    let Some(start_y) = (y + 1).checked_sub(helper.masks.len()) else {
                        continue;
                    };
                    if x + helper.width > width {
                        continue;
                    }
                    if !helper
                        .masks
                        .iter()
                        .enumerate()
                        .all(|(j, mask)| remaining[start_y + j] & (mask << x) == mask << x)
                    {
                        continue;
                    }
                    if let (Ok(dx), Ok(dy)) = (
                        i16::try_from(
                            i32::from(x0) + x as i32 * options.units_per_pixel
                                - i32::from(helper.x),
                        ),
                        i16::try_from(
                            i32::from(y0) + start_y as i32 * options.units_per_pixel
                                - i32::from(helper.y),
                        ),
                    ) {
                        chosen = Some((helper, x, start_y, dx, dy));
                        break;
                    }
                }
                if let Some((helper, x, start_y, dx, dy)) = chosen {
                    components.push(glyf::Component::translated(helper.glyph, dx, dy));
                    for (j, mask) in helper.masks.iter().enumerate() {
                        remaining[start_y + j] &= !(mask << x);
                    }
                } else {
                    let px = (i32::from(x0) + first as i32 * options.units_per_pixel) as i16;
                    let py = (i32::from(y0) + y as i32 * options.units_per_pixel) as i16;
                    components.push(glyf::Component::translated(dot, px, py));
                    remaining[y] &= !(1u64 << first);
                }
            }
        }
        Ok(if components.is_empty() {
            glyf::Glyph::Empty
        } else {
            glyf::Glyph::Composite(glyf::CompositeGlyph {
                components,
                instructions: Vec::new(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_two_dimensional_patterns_preserve_every_3x3_mask() {
        let mut sources = vec![Glyph::new(Bitmap::<crate::bitmap::Mono>::default())];
        for text in ["##", "#\n#", ".#\n##", "##\n.#", "#.#"] {
            let mut glyph = Glyph::new(text.parse::<Bitmap>().unwrap());
            glyph.metrics.bounds.x = -2;
            glyph.metrics.bounds.y = -1;
            sources.push(glyph);
        }
        sources.push(Glyph::new(
            "######\n######\n######\n######\n######"
                .parse::<Bitmap>()
                .unwrap(),
        ));
        let catalogue = Catalogue::new(
            &sources.iter().map(|g| ("", g)).collect::<Vec<_>>(),
            100,
            true,
        );
        let limits = glyf::Limits::default();
        let options = outline::Options {
            shape: outline::Shape::Dot,
            dot_size: 0.9,
            origin: (-3, 2),
            ..Default::default()
        };
        let dot = sources.len() as u16;
        let mut glyphs = vec![glyf::Glyph::Empty];
        for glyph in &sources[1..sources.len() - 1] {
            let GlyphBitmap::Mono(bitmap) = &glyph.bitmap else {
                unreachable!()
            };
            glyphs.push(
                glyf::Glyph::pixels(
                    bitmap,
                    &outline::Options {
                        origin: (-2, -1),
                        ..options.clone()
                    },
                    dot,
                    &limits,
                )
                .unwrap(),
            );
        }
        let target = glyphs.len();
        glyphs.push(glyf::Glyph::Empty);
        glyphs.push(glyf::Glyph::simple(vec![
            outline::pixel_contour(&options).unwrap(),
        ]));
        let mut smaller = false;
        for mask in 0..512 {
            let mut bitmap = Bitmap::new(6, 5).unwrap();
            bitmap.set_pixel(5, 4, 1).unwrap();
            for y in 0..3 {
                for x in 0..3 {
                    if mask & (1 << (y * 3 + x)) != 0 {
                        bitmap.set_pixel(x + 1, y + 1, 1).unwrap();
                    }
                }
            }
            let mut old = glyf::GlyfTable {
                glyphs: glyphs.clone(),
            };
            old.glyphs[target] = glyf::Glyph::pixels(&bitmap, &options, dot, &limits).unwrap();
            let mut new = glyf::GlyfTable {
                glyphs: glyphs.clone(),
            };
            new.glyphs[target] = catalogue
                .glyph(target, &bitmap, &options, dot, &limits)
                .unwrap();
            let canonical = |table: &glyf::GlyfTable| {
                let mut contours = table
                    .contours(target as u16, &limits)
                    .unwrap()
                    .into_iter()
                    .map(|c| format!("{c:?}"))
                    .collect::<Vec<_>>();
                contours.sort();
                contours
            };
            assert_eq!(canonical(&old), canonical(&new), "mask {mask}");
            if let (glyf::Glyph::Composite(a), glyf::Glyph::Composite(b)) =
                (&old.glyphs[target], &new.glyphs[target])
            {
                smaller |= b.components.len() < a.components.len();
            }
        }
        assert!(smaller);
    }
    #[test]
    fn nested_rows_keep_glyph_count_and_exact_expanded_contours() {
        let empty = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
        let mut row = Glyph::new("##".parse::<Bitmap>().unwrap());
        row.metrics.bounds.x = -2;
        row.metrics.bounds.y = 3;
        let mut target = Glyph::new("#####\n##.##".parse::<Bitmap>().unwrap());
        target.metrics.bounds.x = -1;
        target.metrics.bounds.y = -2;
        let sources = [(".notdef", &empty), ("row", &row), ("target", &target)];
        let catalogue = Catalogue::new(&sources, 100, true);
        let limits = glyf::Limits::default();
        let base = outline::Options {
            shape: outline::Shape::Dot,
            dot_size: 0.9,
            ..Default::default()
        };
        let mut before = vec![glyf::Glyph::Empty];
        let mut after = before.clone();
        for (id, (_, glyph)) in sources.iter().enumerate().skip(1) {
            let GlyphBitmap::Mono(bitmap) = &glyph.bitmap else {
                unreachable!()
            };
            let options = outline::Options {
                origin: (glyph.metrics.bounds.x as i32, glyph.metrics.bounds.y as i32),
                ..base.clone()
            };
            before.push(glyf::Glyph::pixels(bitmap, &options, 3, &limits).unwrap());
            after.push(catalogue.glyph(id, bitmap, &options, 3, &limits).unwrap());
        }
        let circle = glyf::Glyph::simple(vec![outline::pixel_contour(&base).unwrap()]);
        before.push(circle.clone());
        after.push(circle);
        let old = glyf::GlyfTable { glyphs: before };
        let new = glyf::GlyfTable { glyphs: after };
        assert_eq!(old.glyphs.len(), new.glyphs.len());
        for id in 0..4 {
            assert_eq!(
                old.contours(id, &limits).unwrap(),
                new.contours(id, &limits).unwrap()
            );
        }
        let (glyf::Glyph::Composite(a), glyf::Glyph::Composite(b)) =
            (&old.glyphs[2], &new.glyphs[2])
        else {
            panic!("expected composites")
        };
        assert!(b.components.len() < a.components.len());
        new.encode(Default::default(), &limits).unwrap();
    }

    #[test]
    fn patterned_rows_cover_ink_once_and_keep_original_work_limits() {
        let empty = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
        let row = Glyph::new("#.#".parse::<Bitmap>().unwrap());
        let target = Glyph::new("###..#\n.#....".parse::<Bitmap>().unwrap());
        let sources = [(".notdef", &empty), ("row", &row), ("target", &target)];
        let catalogue = Catalogue::new(&sources, 100, true);
        let options = outline::Options {
            shape: outline::Shape::Dot,
            dot_size: 0.9,
            ..Default::default()
        };
        let limits = glyf::Limits::default();
        let GlyphBitmap::Mono(bitmap) = &target.bitmap else {
            unreachable!()
        };
        let GlyphBitmap::Mono(row_bitmap) = &row.bitmap else {
            unreachable!()
        };
        let row_glyph = glyf::Glyph::pixels(row_bitmap, &options, 3, &limits).unwrap();
        let circle = glyf::Glyph::simple(vec![outline::pixel_contour(&options).unwrap()]);
        let original = glyf::GlyfTable {
            glyphs: vec![
                glyf::Glyph::Empty,
                row_glyph.clone(),
                glyf::Glyph::pixels(bitmap, &options, 3, &limits).unwrap(),
                circle.clone(),
            ],
        };
        let optimized = glyf::GlyfTable {
            glyphs: vec![
                glyf::Glyph::Empty,
                row_glyph,
                catalogue.glyph(2, bitmap, &options, 3, &limits).unwrap(),
                circle,
            ],
        };
        let canon = |table: &glyf::GlyfTable| {
            let mut contours = table
                .contours(2, &limits)
                .unwrap()
                .into_iter()
                .map(|c| format!("{c:?}"))
                .collect::<Vec<_>>();
            contours.sort();
            contours
        };
        assert_eq!(canon(&original), canon(&optimized));
        let glyf::Glyph::Composite(glyph) = &optimized.glyphs[2] else {
            panic!("composite")
        };
        assert_eq!(glyph.components.len(), 4);
        let strict = glyf::Limits {
            max_components: 3,
            ..limits
        };
        assert!(catalogue.glyph(2, bitmap, &options, 3, &strict).is_err());
    }

    #[test]
    fn unrepresentable_helper_translation_uses_direct_circles() {
        let empty = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
        let mut row = Glyph::new("##".parse::<Bitmap>().unwrap());
        row.metrics.bounds.x = 325;
        let mut target = Glyph::new("##\n.#".parse::<Bitmap>().unwrap());
        target.metrics.bounds.x = -326;
        let catalogue = Catalogue::new(
            &[(".notdef", &empty), ("row", &row), ("target", &target)],
            100,
            true,
        );
        let options = outline::Options {
            shape: outline::Shape::Dot,
            dot_size: 0.9,
            origin: (-326, 0),
            ..Default::default()
        };
        let GlyphBitmap::Mono(bitmap) = &target.bitmap else {
            unreachable!()
        };
        assert_eq!(
            catalogue
                .glyph(2, bitmap, &options, 3, &Default::default())
                .unwrap(),
            glyf::Glyph::pixels(bitmap, &options, 3, &Default::default()).unwrap()
        );
    }
}
