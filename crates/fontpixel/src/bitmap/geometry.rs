use super::*;

/// Concatenation direction. Left/up reverse placement, not the supplied list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Direction {
    /// Left to right (SPEC direction 1).
    Right,
    /// Right to left (SPEC other positive direction).
    Left,
    /// Top to bottom (SPEC direction 0).
    Down,
    /// Bottom to top (SPEC negative direction).
    Up,
}
/// Alignment on the axis perpendicular to concatenation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Alignment {
    /// Top for horizontal, left for vertical.
    Start,
    /// Bottom for horizontal, right for vertical.
    End,
}

fn dimension(value: i128) -> Result<u32> {
    u32::try_from(value).map_err(|_| invalid("bitmap dimensions overflow"))
}
fn coordinate(value: i128) -> i64 {
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

impl<F: PixelFormat> Bitmap<F> {
    /// Crop or zero-extend to the rectangle whose bottom-left offset is `(x, y)`.
    /// Offsets can lie outside the source. Dimensions, arithmetic and allocation
    /// are bounded before copying; there is no implicit trimming.
    pub fn crop(&self, width: u32, height: u32, x: i64, y: i64) -> Result<Self> {
        let mut out = Self::with_limits(width, height, self.limits)?;
        out.composite(
            self,
            coordinate(-i128::from(x)),
            coordinate(i128::from(height) - i128::from(self.height) + i128::from(y)),
            CompositeMode::Replace,
        );
        Ok(out)
    }
    /// Crop using a bottom-left bounding rectangle.
    pub fn crop_bounds(&self, bounds: Bounds) -> Result<Self> {
        self.crop(bounds.width, bounds.height, bounds.x, bounds.y)
    }
    /// Add blank margins on all four sides, with no change to existing pixels.
    pub fn extend(&self, left: u32, right: u32, top: u32, bottom: u32) -> Result<Self> {
        let width = dimension(i128::from(self.width) + i128::from(left) + i128::from(right))?;
        let height = dimension(i128::from(self.height) + i128::from(top) + i128::from(bottom))?;
        self.crop(width, height, -i64::from(left), -i64::from(bottom))
    }
    /// Move pixels right/up within the original dimensions, filling vacated pixels with 0.
    pub fn shift(&self, dx: i64, dy: i64) -> Result<Self> {
        let mut out = Self::with_limits(self.width, self.height, self.limits)?;
        out.paste(self, dx, coordinate(-i128::from(dy)));
        Ok(out)
    }
    /// Tight nonzero-ink rectangle, in bottom-left coordinates; empty is `(0,0,0,0)`.
    /// Mono scans bytes using leading/trailing zero counts, not individual pixels.
    pub fn bbox(&self) -> Bounds {
        // A zero-width raster can retain an arbitrarily large logical height.
        // No stored pixels means no ink, without walking every empty row.
        if self.data.is_empty() {
            return Bounds::default();
        }
        let (mut left, mut right, mut top, mut bottom) = (self.width, 0, self.height, 0);
        for y in 0..self.height {
            for (i, &b) in self.row(y).iter().enumerate() {
                if b == 0 {
                    continue;
                }
                let (l, r) = if F::PACKED {
                    (
                        i as u32 * 8 + b.leading_zeros(),
                        i as u32 * 8 + 8 - b.trailing_zeros(),
                    )
                } else {
                    (i as u32, i as u32 + 1)
                };
                left = left.min(l);
                right = right.max(r);
                top = top.min(y);
                bottom = y + 1;
            }
        }
        if top == self.height {
            Bounds::default()
        } else {
            Bounds::new(
                right - left,
                bottom - top,
                i64::from(left),
                i64::from(self.height - bottom),
            )
        }
    }
    /// Remove all zero margins; any no-ink raster becomes exactly 0×0.
    pub fn trim(&self) -> Result<Self> {
        self.crop_bounds(self.bbox())
    }
    /// Test whether any nonzero ink exists (ignores dimension-only empty pixels).
    pub fn has_ink(&self) -> bool {
        self.data.iter().any(|&b| b != 0)
    }

    /// Mirror left/right without changing dimensions.
    pub fn flip_horizontal(&self) -> Result<Self> {
        let mut out = Self::with_limits(self.width, self.height, self.limits)?;
        for y in 0..self.height {
            let source = self.row(y);
            let target = out.row_mut(y);
            if !F::PACKED {
                for (a, b) in target.iter_mut().zip(source.iter().rev()) {
                    *a = *b;
                }
            } else {
                let pad = (8 - self.width % 8) % 8;
                for (i, target_byte) in target.iter_mut().enumerate() {
                    let index = source.len() - 1 - i;
                    let a = source[index].reverse_bits();
                    *target_byte = if pad == 0 {
                        a
                    } else {
                        (a << pad)
                            | (if index > 0 {
                                source[index - 1].reverse_bits()
                            } else {
                                0
                            } >> (8 - pad))
                    };
                }
            }
        }
        Ok(out)
    }
    /// Mirror top/bottom without changing dimensions.
    pub fn flip_vertical(&self) -> Result<Self> {
        let mut out = Self::with_limits(self.width, self.height, self.limits)?;
        for y in 0..self.height {
            out.row_mut(y)
                .copy_from_slice(self.row(self.height - 1 - y));
        }
        Ok(out)
    }
    /// Rotate clockwise by a signed multiple of 90 degrees; other angles fail.
    pub fn rotate(&self, degrees: i32) -> Result<Self> {
        if degrees % 90 != 0 {
            return Err(invalid("rotation angle must be a multiple of 90"));
        }
        let turns = (degrees / 90).rem_euclid(4);
        if turns == 0 {
            return Self::from_raw(self.width, self.height, &self.data, self.limits);
        }
        if turns == 2 {
            return self.flip_horizontal()?.flip_vertical();
        }
        let mut out = Self::with_limits(self.height, self.width, self.limits)?;
        // Enumerate ink bits only for packed rasters; gray samples preserve values.
        for y in 0..self.height {
            if F::PACKED {
                for (i, &b) in self.row(y).iter().enumerate() {
                    let mut ink = b;
                    while ink != 0 {
                        let bit = ink.leading_zeros();
                        let x = i as u32 * 8 + bit;
                        ink &= !(0x80 >> bit);
                        let (nx, ny) = if turns == 1 {
                            (self.height - 1 - y, x)
                        } else {
                            (y, self.width - 1 - x)
                        };
                        out.put(nx, ny, 1);
                    }
                }
            } else {
                for x in 0..self.width {
                    let (nx, ny) = if turns == 1 {
                        (self.height - 1 - y, x)
                    } else {
                        (y, self.width - 1 - x)
                    };
                    out.put(nx, ny, self.pixel(x, y));
                }
            }
        }
        Ok(out)
    }
    /// Integer nearest-neighbor enlargement; both factors must be at least one.
    /// Width/height are checked before allocation. Compatibility adapters can map
    /// SPEC-v2 factors <=1 to 1 before calling this strongly typed API.
    pub fn enlarge(&self, x: u32, y: u32) -> Result<Self> {
        if x == 0 || y == 0 {
            return Err(invalid("enlargement factors must be positive"));
        }
        let width = self
            .width
            .checked_mul(x)
            .ok_or_else(|| invalid("enlarged width overflows"))?;
        let height = self
            .height
            .checked_mul(y)
            .ok_or_else(|| invalid("enlarged height overflows"))?;
        let mut out = Self::with_limits(width, height, self.limits)?;
        for sy in 0..self.height {
            let dy = sy * y;
            if x == 1 {
                out.row_mut(dy).copy_from_slice(self.row(sy));
            } else if !F::PACKED {
                for (source, target) in self
                    .row(sy)
                    .iter()
                    .zip(out.row_mut(dy).chunks_exact_mut(x as usize))
                {
                    target.fill(*source);
                }
            } else {
                for sx in 0..self.width {
                    if self.pixel(sx, sy) != 0 {
                        fill_bits(out.row_mut(dy), sx * x, x);
                    }
                }
            }
            let stride = out.stride();
            for repeat in 1..y {
                let src = dy as usize * stride;
                let dst = (dy + repeat) as usize * stride;
                out.data.copy_within(src..src + stride, dst);
            }
        }
        Ok(out)
    }
    /// Pad the right edge to a positive pixel multiple (default callers use 8).
    pub fn byte_pad(&self, pixels: u32) -> Result<Self> {
        if pixels == 0 {
            return Err(invalid("padding multiple must be positive"));
        }
        let width = self
            .width
            .div_ceil(pixels)
            .checked_mul(pixels)
            .ok_or_else(|| invalid("padded width overflows"))?;
        self.crop(width, self.height, 0, 0)
    }

    /// Concatenate two bitmaps; negative gaps overlap and can extend past either end.
    /// Overlap follows SPEC: the right/bottom operand wins wherever nonzero.
    pub fn concat(
        &self,
        other: &Self,
        direction: Direction,
        alignment: Alignment,
        gap: i64,
    ) -> Result<Self> {
        Self::concat_all(&[self, other], direction, alignment, &[gap], self.limits)
    }

    /// Concatenate in one output allocation. Offsets are either empty (all zero)
    /// or exactly `bitmaps.len()-1`. Vertical concatenation skips zero-width or
    /// zero-height inputs and their preceding offsets, as SPEC-v3 §11.4 requires.
    /// Horizontal zero-height inputs retain their explicit width in the core API.
    pub fn concat_all(
        bitmaps: &[&Self],
        direction: Direction,
        alignment: Alignment,
        gaps: &[i64],
        limits: BitmapLimits,
    ) -> Result<Self> {
        if !gaps.is_empty() && gaps.len() != bitmaps.len().saturating_sub(1) {
            return Err(invalid("concatenation requires one gap per input boundary"));
        }
        let horizontal = matches!(direction, Direction::Right | Direction::Left);
        let forward = matches!(direction, Direction::Right | Direction::Down);
        let mut positions = Vec::new();
        let bytes = bitmaps
            .len()
            .checked_mul(std::mem::size_of::<(usize, i128)>())
            .ok_or_else(|| invalid("too many concatenation inputs"))?;
        if bytes > limits.max_bytes {
            return Err(limit("concatenation input bookkeeping", limits.max_bytes));
        }
        positions
            .try_reserve_exact(bitmaps.len())
            .map_err(|_| limit("concatenation allocation", limits.max_bytes))?;
        let (mut lo, mut hi, mut cross) = (0i128, 0i128, 0u32);
        for (index, bitmap) in bitmaps.iter().enumerate() {
            if !horizontal && (bitmap.width == 0 || bitmap.height == 0) {
                continue;
            }
            let length = i128::from(if horizontal {
                bitmap.width
            } else {
                bitmap.height
            });
            let gap = if positions.is_empty() || gaps.is_empty() {
                0
            } else {
                i128::from(gaps[index - 1])
            };
            let start = if forward { hi + gap } else { lo - gap - length };
            lo = lo.min(start);
            hi = hi.max(start + length);
            cross = cross.max(if horizontal {
                bitmap.height
            } else {
                bitmap.width
            });
            let along = dimension(hi - lo)?;
            if along > limits.max_dimension || cross > limits.max_dimension {
                return Err(limit("bitmap dimension", limits.max_dimension as usize));
            }
            positions.push((index, start));
        }
        let along = dimension(hi - lo)?;
        let (width, height) = if horizontal {
            (along, cross)
        } else {
            (cross, along)
        };
        let mut out = Self::with_limits(width, height, limits)?;
        // In left/up concatenation all earlier inputs are on top of later inputs.
        if !forward {
            positions.reverse();
        }
        for (index, start) in positions {
            let bitmap = bitmaps[index];
            let across = if alignment == Alignment::Start {
                0
            } else {
                cross
                    - if horizontal {
                        bitmap.height
                    } else {
                        bitmap.width
                    }
            };
            let (x, y) = if horizontal {
                ((start - lo) as i64, i64::from(across))
            } else {
                (i64::from(across), (start - lo) as i64)
            };
            out.paste(bitmap, x, y);
        }
        Ok(out)
    }
}
fn fill_bits(row: &mut [u8], start: u32, width: u32) {
    let end = start + width;
    let mut pos = start;
    while pos < end {
        let off = pos % 8;
        let n = (8 - off).min(end - pos);
        row[pos as usize / 8] |= (0xffu8 >> off) & (0xffu8 << (8 - off - n));
        pos += n;
    }
}
