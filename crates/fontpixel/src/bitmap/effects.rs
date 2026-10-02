use super::*;

impl<F: PixelFormat> Bitmap<F> {
    /// Binary inversion: zero becomes full ink, every nonzero value becomes zero.
    /// Gray8 continuous tone inversion is available separately as `negate`.
    pub fn invert(&mut self) {
        self.invert_with(F::MAX).expect("format maximum is valid");
    }
    /// Binary inversion with an explicit foreground value (e.g. label 1).
    pub fn invert_with(&mut self, ink: u8) -> Result<()> {
        Self::check_value(ink)?;
        if F::PACKED {
            for b in &mut self.data {
                *b = if ink == 0 { 0 } else { !*b };
            }
            self.clear_padding();
        } else {
            for b in &mut self.data {
                *b = if *b == 0 { ink } else { 0 };
            }
        }
        Ok(())
    }
    /// Replace a single sample value, preserving dimensions and padding.
    pub fn replace(&mut self, old: u8, new: u8) -> Result<()> {
        Self::check_value(old)?;
        Self::check_value(new)?;
        if old == new {
            return Ok(());
        }
        if F::PACKED {
            self.data.fill(if new == 0 { 0 } else { 255 });
            self.clear_padding();
        } else {
            for b in &mut self.data {
                if *b == old {
                    *b = new;
                }
            }
        }
        Ok(())
    }
    /// Non-overlapping row-wise pattern replacement, matching Python string
    /// replacement. Empty patterns insert at every boundary. A ragged result is
    /// rejected because the core raster is always rectangular.
    pub fn replace_pattern(&self, old: &[u8], new: &[u8]) -> Result<Self> {
        for &value in old.iter().chain(new) {
            Self::check_value(value)?;
        }
        let row_overhead = (self.height as usize)
            .checked_mul(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(|| invalid("replacement row count overflows"))?;
        if row_overhead > self.limits.max_bytes {
            return Err(limit("bitmap replacement rows", self.limits.max_bytes));
        }
        let mut rows: Vec<Vec<u8>> = Vec::new();
        rows.try_reserve_exact(self.height as usize)
            .map_err(|_| limit("bitmap replacement allocation", self.limits.max_bytes))?;
        let mut total = row_overhead;
        let mut new_width = None;
        for y in 0..self.height {
            let mut row = Vec::new();
            let mut x = 0;
            loop {
                let found = old.is_empty()
                    || (old.len() <= (self.width - x) as usize
                        && old
                            .iter()
                            .enumerate()
                            .all(|(i, &p)| self.pixel(x + i as u32, y) == p));
                let addition = if found { new } else { &[] };
                let next = row
                    .len()
                    .checked_add(addition.len())
                    .ok_or_else(|| invalid("replacement width overflows"))?;
                if next > self.limits.max_dimension as usize
                    || total.saturating_add(next) > self.limits.max_bytes
                {
                    return Err(limit("bitmap replacement bytes", self.limits.max_bytes));
                }
                row.try_reserve(addition.len().saturating_add(1))
                    .map_err(|_| limit("bitmap replacement allocation", self.limits.max_bytes))?;
                row.extend_from_slice(addition);
                if x == self.width {
                    break;
                }
                if found && !old.is_empty() {
                    x += old.len() as u32;
                } else {
                    row.push(self.pixel(x, y));
                    x += 1;
                }
                if x == self.width && !old.is_empty() {
                    break;
                }
            }
            if new_width.is_some_and(|w| w != row.len()) {
                return Err(invalid("pattern replacement produces ragged rows"));
            }
            new_width = Some(row.len());
            total = total
                .checked_add(row.len())
                .ok_or_else(|| invalid("replacement size overflows"))?;
            rows.push(row);
        }
        let width = u32::try_from(new_width.unwrap_or(self.width as usize))
            .map_err(|_| invalid("replacement width overflows"))?;
        let mut out = Self::with_limits(width, self.height, self.limits)?;
        for (y, row) in rows.iter().enumerate() {
            for (x, &p) in row.iter().enumerate() {
                out.put(x as u32, y as u32, p);
            }
        }
        Ok(out)
    }
    /// Add a displaced silhouette behind the original. `dx` moves right and
    /// `dy` moves up. All nonzero shadow pixels take `value`; original pixels win.
    /// Gray8 with values 0/1 and shadow value 2 reproduces SPEC's labels exactly.
    pub fn shadow(&self, dx: i64, dy: i64, value: u8) -> Result<Self> {
        Self::check_value(value)?;
        let width = u32::try_from(
            u64::from(self.width)
                .checked_add(dx.unsigned_abs())
                .ok_or_else(|| invalid("shadow width overflows"))?,
        )
        .map_err(|_| invalid("shadow width overflows"))?;
        let height = u32::try_from(
            u64::from(self.height)
                .checked_add(dy.unsigned_abs())
                .ok_or_else(|| invalid("shadow height overflows"))?,
        )
        .map_err(|_| invalid("shadow height overflows"))?;
        let mut out = Self::with_limits(width, height, self.limits)?;
        let mut silhouette = Self::from_raw(self.width, self.height, &self.data, self.limits)?;
        if F::PACKED {
            if value == 0 {
                silhouette.data.fill(0);
            }
        } else {
            for p in &mut silhouette.data {
                if *p != 0 {
                    *p = value;
                }
            }
        }
        let ox = -dx.min(0);
        let oy = dy.max(0);
        out.paste(&silhouette, ox + dx, oy - dy);
        out.paste(self, ox, oy);
        Ok(out)
    }
    /// One-pixel glow around samples exactly equal to `ink`. Orthogonal neighbors
    /// are always included; `diagonals` adds corners. Existing nonzero pixels win.
    /// Use `ink=1,value=2` for SPEC; repeated glow does not spread old label-2 ink.
    pub fn glow(&self, diagonals: bool, ink: u8, value: u8) -> Result<Self> {
        Self::check_value(ink)?;
        Self::check_value(value)?;
        if ink == 0 {
            return Err(invalid("glow source must be nonzero"));
        }
        let mut out = self.extend(1, 1, 1, 1)?;
        let mut silhouette = Self::with_limits(self.width, self.height, self.limits)?;
        if F::PACKED {
            if value != 0 {
                silhouette.data.copy_from_slice(&self.data);
            }
        } else {
            for (a, &b) in silhouette.data.iter_mut().zip(&self.data) {
                *a = if b == ink { value } else { 0 };
            }
        }
        // Put glow into a blank result, then overlay the original to retain priority.
        out.data.fill(0);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            out.paste(&silhouette, 1 + dx, 1 + dy);
        }
        if diagonals {
            for (dx, dy) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
                out.paste(&silhouette, 1 + dx, 1 + dy);
            }
        }
        out.paste(self, 1, 1);
        Ok(out)
    }
    /// Rightward emboldening by `n` pixels. At overlaps the furthest-right copy
    /// wins, preserving SPEC's labels. Mono uses logarithmic byte-wise dilation;
    /// Gray8 uses a linear first-nonzero sliding window.
    pub fn bold(&self, n: u32) -> Result<Self> {
        let width = self
            .width
            .checked_add(n)
            .ok_or_else(|| invalid("bold width overflows"))?;
        let mut out = Self::with_limits(width, self.height, self.limits)?;
        if self.width == 0 || self.height == 0 {
            return Ok(out);
        }
        if F::PACKED {
            out.paste(self, 0, 0);
            let mut span = 1u32;
            let mut remaining = n;
            while remaining > 0 {
                let shift = span.min(remaining);
                let snapshot = Self::from_raw(out.width, out.height, &out.data, out.limits)?;
                out.paste(&snapshot, i64::from(shift), 0);
                remaining -= shift;
                span = span.saturating_add(shift);
            }
        } else {
            for y in 0..self.height {
                let row = self.row(y);
                let target = out.row_mut(y);
                let mut first = 0usize;
                for (x, pixel) in target.iter_mut().enumerate() {
                    first = first.max(x.saturating_sub(n as usize));
                    while first < row.len() && first <= x && row[first] == 0 {
                        first += 1;
                    }
                    if first < row.len() && first <= x {
                        *pixel = row[first];
                    }
                }
            }
        }
        Ok(out)
    }
    /// Slant by floor((row_from_bottom-base)/step), widening to contain every row.
    /// `step` must be positive; negative bases and shifts use mathematical floor.
    pub fn italic(&self, step: i64, base: i64) -> Result<Self> {
        if step <= 0 {
            return Err(invalid("italic step must be positive"));
        }
        if self.height == 0 {
            return Self::from_raw(self.width, self.height, &self.data, self.limits);
        }
        let lo = (-i128::from(base)).div_euclid(i128::from(step));
        let hi = (i128::from(self.height - 1) - i128::from(base)).div_euclid(i128::from(step));
        let width = u32::try_from(i128::from(self.width) + hi - lo)
            .map_err(|_| invalid("italic width overflows"))?;
        let mut out = Self::with_limits(width, self.height, self.limits)?;
        for y in 0..self.height {
            let shift = (i128::from(self.height - 1 - y) - i128::from(base))
                .div_euclid(i128::from(step))
                - lo;
            blit::<F>(
                out.row_mut(y),
                self.row(y),
                0,
                shift as u32,
                self.width,
                CompositeMode::Replace,
            );
        }
        Ok(out)
    }
}
