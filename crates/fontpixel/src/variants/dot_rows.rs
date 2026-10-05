//! Reuse existing bounded ink patterns without adding glyph IDs or changing pixels.
use crate::{
    Bitmap, Glyph, Result,
    model::GlyphBitmap,
    outline,
    sfnt::{self, glyf},
    working::{Budget, Nested, Owned, Vector},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Row {
    glyph: u16,
    x: i16,
    y: i16,
    width: usize,
    masks: Vec<u64>,
    ink: u32,
}

/// Most catalogue patterns retained; selection stays deterministic.
const MAX_PATTERNS: usize = 256;
/// Largest admitted pattern dimension.
const MAX_SIDE: u32 = 64;

/// Selected helpers themselves stay direct circle composites, preventing cycles.
/// Rows and selection flags keep their admitted charges until the catalogue drops.
pub(super) struct Catalogue {
    rows: Owned<Vec<Row>>,
    selected: Vector<bool>,
}
impl Catalogue {
    pub(super) fn new(
        sources: &[(&str, &Glyph)],
        scale: i64,
        include_2d: bool,
        budget: &Budget,
    ) -> Result<Self> {
        // Patterns are keyed by `[width, masks...]`, so lookups borrow one
        // admitted scratch key instead of cloning masks for every candidate.
        let mut patterns_lease = budget.reserve(0)?;
        let mut keys_lease = budget.reserve(0)?;
        let mut patterns = BTreeMap::<Vec<u64>, Owned<Row>>::new();
        let mut key = Vector::<u64>::with_capacity(budget, MAX_SIDE as usize + 1)?;
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
                || ink.width > MAX_SIDE
                || ink.height > MAX_SIDE
                || (ink.width > 1 && ink.height > 1 && (ink.width > 4 || ink.height > 4))
            {
                continue;
            }
            key.truncate(0);
            key.push(u64::from(ink.width))?;
            key.resize(1 + ink.height as usize, 0)?;
            for y in 0..ink.height {
                let row = bitmap.height() - 1 - ink.y as u32 - y;
                for x in 0..ink.width {
                    if bitmap
                        .get_pixel(ink.x as u32 + x, row)
                        .is_ok_and(|p| p != 0)
                    {
                        key[1 + y as usize] |= 1u64 << x;
                    }
                }
            }
            let masks = &key[1..];
            let pixels = masks.iter().map(|m| m.count_ones()).sum::<u32>();
            if pixels < 2 || patterns.contains_key(&key[..]) || patterns.len() >= MAX_PATTERNS {
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
            let (stored, mut stored_lease) = copy(budget, &key)?.into_parts();
            keys_lease.merge(&mut stored_lease)?;
            let (masks, lease) = copy(budget, masks)?.into_parts();
            crate::variants::ledger::tree::<Vec<u64>, Owned<Row>>(
                &mut patterns_lease,
                budget,
                patterns.len() + 1,
            )?;
            patterns.insert(
                stored,
                Owned::from_parts(
                    Row {
                        glyph,
                        x,
                        y,
                        width: ink.width as usize,
                        masks,
                        ink: pixels,
                    },
                    lease,
                ),
            );
        }
        drop(key);
        let mut rows = Nested::with_capacity(budget, patterns.len())?;
        for (_, row) in std::mem::take(&mut patterns) {
            rows.push(row)?;
        }
        drop(keys_lease);
        drop(patterns_lease);
        // Glyph IDs are unique, so an in-place unstable sort is fully determined.
        rows.sort_unstable_by_key(|r| {
            (
                std::cmp::Reverse(r.ink),
                std::cmp::Reverse(r.width),
                r.glyph,
            )
        });
        let mut selected = Vector::with_capacity(budget, sources.len())?;
        selected.resize(sources.len(), false)?;
        for row in rows.iter() {
            selected[usize::from(row.glyph)] = true;
        }
        Ok(Self {
            rows: rows.into_owned()?,
            selected,
        })
    }

    pub(super) fn glyph(
        &self,
        id: usize,
        bitmap: &Bitmap,
        options: &outline::Options,
        dot: u16,
        limits: &glyf::Limits,
        budget: &Budget,
    ) -> Result<Owned<glyf::Glyph>> {
        if self.selected[id] || self.rows.is_empty() {
            return glyf::Glyph::pixels_with_budget(bitmap, options, dot, limits, budget);
        }
        let origins = {
            let origins =
                outline::pixel_origins_with_budget(bitmap, options, &limits.geometry, budget)?;
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
            let mut out = Vector::with_capacity(budget, origins.len())?;
            for &(x, y) in origins.iter() {
                out.push((coordinate(x)?, coordinate(y)?))?;
            }
            out
        };
        if origins.is_empty() {
            return Ok(Owned::from_parts(glyf::Glyph::Empty, budget.reserve(0)?));
        }
        let x0 = origins.iter().map(|p| p.0).min().unwrap();
        let y0 = origins.iter().map(|p| p.1).min().unwrap();
        let x1 = origins.iter().map(|p| p.0).max().unwrap();
        let y1 = origins.iter().map(|p| p.1).max().unwrap();
        let width = ((i32::from(x1) - i32::from(x0)) / options.units_per_pixel + 1) as usize;
        let height = ((i32::from(y1) - i32::from(y0)) / options.units_per_pixel + 1) as usize;
        if width > MAX_SIDE as usize || height > MAX_SIDE as usize {
            drop(origins);
            return glyf::Glyph::pixels_with_budget(bitmap, options, dot, limits, budget);
        }
        let mut remaining = Vector::with_capacity(budget, height)?;
        remaining.resize(height, 0u64)?;
        for &(x, y) in origins.iter() {
            let x = ((i32::from(x) - i32::from(x0)) / options.units_per_pixel) as usize;
            let y = ((i32::from(y) - i32::from(y0)) / options.units_per_pixel) as usize;
            remaining[y] |= 1u64 << x;
        }
        let mut components = Vector::with_capacity(budget, origins.len())?;
        drop(origins);
        for y in (0..height).rev() {
            while remaining[y] != 0 {
                let first = remaining[y].trailing_zeros() as usize;
                let mut chosen = None;
                for helper in self.rows.iter() {
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
                    components.push(glyf::Component::translated(helper.glyph, dx, dy))?;
                    for (j, mask) in helper.masks.iter().enumerate() {
                        remaining[start_y + j] &= !(mask << x);
                    }
                } else {
                    let px = (i32::from(x0) + first as i32 * options.units_per_pixel) as i16;
                    let py = (i32::from(y0) + y as i32 * options.units_per_pixel) as i16;
                    components.push(glyf::Component::translated(dot, px, py))?;
                    remaining[y] &= !(1u64 << first);
                }
            }
        }
        drop(remaining);
        let (components, lease) = components.into_parts();
        Ok(Owned::from_parts(
            if components.is_empty() {
                glyf::Glyph::Empty
            } else {
                glyf::Glyph::Composite(glyf::CompositeGlyph {
                    components,
                    instructions: Vec::new(),
                })
            },
            lease,
        ))
    }
}

/// Exact admitted copy of a mask slice.
fn copy(budget: &Budget, values: &[u64]) -> Result<Owned<Vec<u64>>> {
    let mut out = Vector::with_capacity(budget, values.len())?;
    out.extend(values)?;
    Ok(out.into_owned())
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
        let budget = Budget::new(usize::MAX, "test working bytes");
        let catalogue = Catalogue::new(
            &sources.iter().map(|g| ("", g)).collect::<Vec<_>>(),
            100,
            true,
            &budget,
        )
        .unwrap();
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
                .glyph(target, &bitmap, &options, dot, &limits, &budget)
                .unwrap()
                .into_parts()
                .0;
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
        let budget = Budget::new(usize::MAX, "test working bytes");
        let catalogue = Catalogue::new(&sources, 100, true, &budget).unwrap();
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
            after.push(
                catalogue
                    .glyph(id, bitmap, &options, 3, &limits, &budget)
                    .unwrap()
                    .into_parts()
                    .0,
            );
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
        let budget = Budget::new(usize::MAX, "test working bytes");
        let catalogue = Catalogue::new(&sources, 100, true, &budget).unwrap();
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
                catalogue
                    .glyph(2, bitmap, &options, 3, &limits, &budget)
                    .unwrap()
                    .into_parts()
                    .0,
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
        assert!(
            catalogue
                .glyph(2, bitmap, &options, 3, &strict, &budget)
                .is_err()
        );
    }

    #[test]
    fn unrepresentable_helper_translation_uses_direct_circles() {
        let empty = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
        let mut row = Glyph::new("##".parse::<Bitmap>().unwrap());
        row.metrics.bounds.x = 325;
        let mut target = Glyph::new("##\n.#".parse::<Bitmap>().unwrap());
        target.metrics.bounds.x = -326;
        let budget = Budget::new(usize::MAX, "test working bytes");
        let catalogue = Catalogue::new(
            &[(".notdef", &empty), ("row", &row), ("target", &target)],
            100,
            true,
            &budget,
        )
        .unwrap();
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
                .glyph(2, bitmap, &options, 3, &Default::default(), &budget)
                .unwrap()
                .into_parts()
                .0,
            glyf::Glyph::pixels(bitmap, &options, 3, &Default::default()).unwrap()
        );
    }
}
