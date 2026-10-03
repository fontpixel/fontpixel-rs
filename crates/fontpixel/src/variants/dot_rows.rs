//! Reuse existing one-row glyphs without adding glyph IDs or changing pixels.
use crate::{
    Bitmap, Glyph, Result,
    model::GlyphBitmap,
    outline,
    sfnt::{self, glyf},
};

#[derive(Clone, Copy, Debug)]
struct Row {
    glyph: u16,
    x: i16,
    y: i16,
    width: usize,
    mask: u64,
}

/// Selected row glyphs themselves stay direct circle composites, preventing cycles.
pub(super) struct Catalogue {
    rows: Vec<Row>,
    selected: Vec<bool>,
}
impl Catalogue {
    pub(super) fn new(sources: &[(&str, &Glyph)], scale: i64) -> Self {
        let mut patterns = std::collections::BTreeMap::new();
        for (id, (_, glyph)) in sources.iter().enumerate().skip(1) {
            let GlyphBitmap::Mono(bitmap) = &glyph.bitmap else {
                continue;
            };
            let ink = bitmap.bbox();
            if ink.height != 1 || !(2..=64).contains(&ink.width) {
                continue;
            }
            let row = bitmap.height() - 1 - ink.y as u32;
            let mut mask = 0u64;
            for x in 0..ink.width {
                if bitmap
                    .get_pixel(ink.x as u32 + x, row)
                    .is_ok_and(|p| p != 0)
                {
                    mask |= 1u64 << x;
                }
            }
            if mask.count_ones() < 2
                || patterns.contains_key(&(ink.width, mask))
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
                (ink.width, mask),
                Row {
                    glyph,
                    x,
                    y,
                    width: ink.width as usize,
                    mask,
                },
            );
        }
        let mut rows = patterns.into_values().collect::<Vec<_>>();
        rows.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.mask.count_ones()),
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
        let mut components = Vec::with_capacity(origins.len());
        let mut start = 0;
        while start < origins.len() {
            let mut end = start + 1;
            while end < origins.len() && origins[end].1 == origins[start].1 {
                end += 1;
            }
            let (x0, y) = origins[start];
            let span =
                (i32::from(origins[end - 1].0) - i32::from(x0)) / options.units_per_pixel + 1;
            if span > 64 {
                for &(x, y) in &origins[start..end] {
                    components.push(glyf::Component::translated(dot, x, y));
                }
                start = end;
                continue;
            }
            let mut remaining = 0u64;
            for &(x, _) in &origins[start..end] {
                remaining |= 1u64 << ((i32::from(x) - i32::from(x0)) / options.units_per_pixel);
            }
            while remaining != 0 {
                let first = remaining.trailing_zeros() as usize;
                let x = i32::from(x0) + first as i32 * options.units_per_pixel;
                let mut chosen = None;
                for helper in &self.rows {
                    if first + helper.width > span as usize {
                        continue;
                    }
                    let mask = helper.mask << first;
                    if remaining & mask != mask {
                        continue;
                    }
                    if let (Ok(dx), Ok(dy)) = (
                        i16::try_from(x - i32::from(helper.x)),
                        i16::try_from(i32::from(y) - i32::from(helper.y)),
                    ) {
                        chosen = Some((helper.glyph, dx, dy, mask));
                        break;
                    }
                }
                if let Some((glyph, dx, dy, mask)) = chosen {
                    components.push(glyf::Component::translated(glyph, dx, dy));
                    remaining &= !mask;
                } else {
                    components.push(glyf::Component::translated(dot, x as i16, y));
                    remaining &= !(1u64 << first);
                }
            }
            start = end;
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
    fn nested_rows_keep_glyph_count_and_exact_expanded_contours() {
        let empty = Glyph::new(Bitmap::<crate::bitmap::Mono>::default());
        let mut row = Glyph::new("##".parse::<Bitmap>().unwrap());
        row.metrics.bounds.x = -2;
        row.metrics.bounds.y = 3;
        let mut target = Glyph::new("#####\n##.##".parse::<Bitmap>().unwrap());
        target.metrics.bounds.x = -1;
        target.metrics.bounds.y = -2;
        let sources = [(".notdef", &empty), ("row", &row), ("target", &target)];
        let catalogue = Catalogue::new(&sources, 100);
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
        let target = Glyph::new("###\n.#.".parse::<Bitmap>().unwrap());
        let sources = [(".notdef", &empty), ("row", &row), ("target", &target)];
        let catalogue = Catalogue::new(&sources, 100);
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
        assert_eq!(glyph.components.len(), 3);
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
