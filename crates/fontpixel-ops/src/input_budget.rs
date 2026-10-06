//! Cumulative retained-model limits across CLI inputs, before they are merged.
use fontpixel::formats::Loaded;
use fontpixel::model::{Font, Property};
use fontpixel::{Error, FontSet, Report, ReportEntry, ResourceLimits, Result};
use std::mem::size_of;

#[derive(Debug)]
/// Cumulative retained-model and glyph-slot ledger shared by several decoded inputs.
pub struct InputBudget {
    limits: ResourceLimits,
    retained_bytes: usize,
    fonts: usize,
    glyphs: usize,
    glyph_slots: usize,
}
impl InputBudget {
    /// A ledger over the given limits.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            retained_bytes: 0,
            fonts: 0,
            glyphs: 0,
            glyph_slots: 0,
        }
    }
    /// Use for both FileInput::open and Registry::read. Already retained models,
    /// diagnostics and merge reserves reduce the next decoder's allowance.
    pub fn remaining_limits(&self) -> Result<ResourceLimits> {
        let mut limits = self.limits.clone();
        limits.max_total_bytes = limits
            .max_total_bytes
            .checked_sub(self.retained_bytes)
            .ok_or_else(|| self.byte_error())?;
        limits.max_input_bytes = limits.max_input_bytes.min(limits.max_total_bytes);
        limits.max_resource_bytes = limits.max_resource_bytes.min(limits.max_total_bytes);
        limits.max_decompressed_bytes = limits.max_decompressed_bytes.min(limits.max_total_bytes);
        limits.max_table_bytes = limits.max_table_bytes.min(limits.max_total_bytes);
        limits.max_glyphs = limits
            .max_glyphs
            .checked_sub(self.glyph_slots)
            .ok_or_else(|| self.glyph_error())?;
        Ok(limits)
    }
    fn byte_error(&self) -> Error {
        Error::ResourceLimit {
            resource: "retained CLI input bytes",
            limit: self.limits.max_total_bytes,
        }
    }
    fn glyph_error(&self) -> Error {
        Error::ResourceLimit {
            resource: "retained CLI glyph slots",
            limit: self.limits.max_glyphs,
        }
    }
    fn add_bytes(&self, total: &mut usize, bytes: usize) -> Result<()> {
        *total = total.checked_add(bytes).ok_or_else(|| self.byte_error())?;
        if *total > self.limits.max_total_bytes {
            return Err(self.byte_error());
        }
        Ok(())
    }
    fn report_bytes(&self, report: &Report, total: &mut usize) -> Result<()> {
        self.add_bytes(total, size_of::<Report>())?;
        self.add_bytes(
            total,
            report
                .entries
                .capacity()
                .checked_mul(size_of::<ReportEntry>())
                .ok_or_else(|| self.byte_error())?,
        )?;
        for entry in &report.entries {
            self.add_bytes(total, entry.code.capacity())?;
            self.add_bytes(total, entry.message.capacity())?;
            self.add_bytes(
                total,
                entry.location.asset.as_ref().map_or(0, String::capacity),
            )?;
        }
        Ok(())
    }
    fn merge_reserve<T>(&self, total: &mut usize, count: usize) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        // Source allocations are already charged. An additional four array
        // slots per incoming element covers the receiving Vec's geometric
        // growth, its minimum capacity, and old/new arrays during reallocation.
        // Charging each input once avoids scanning the growing merged model.
        let reserve = count
            .checked_mul(4)
            .and_then(|v| v.checked_mul(size_of::<T>()))
            .ok_or_else(|| self.byte_error())?;
        self.add_bytes(total, reserve)
    }
    /// Reserve an additional owned strike before cloning it for a language route.
    #[cfg(feature = "opentype")]
    /// Reserve one additional deep copy of `font`.
    pub fn reserve_font_clone(&mut self, font: &Font) -> Result<()> {
        let usage = font.cloned_usage(
            self.limits
                .max_total_bytes
                .saturating_sub(self.retained_bytes),
        )?;
        let glyphs = self
            .glyphs
            .checked_add(usage.glyphs)
            .ok_or_else(|| self.glyph_error())?;
        let slots = self
            .glyph_slots
            .checked_add(usage.glyph_slots)
            .ok_or_else(|| self.glyph_error())?;
        if glyphs > self.limits.max_glyphs || slots > self.limits.max_glyphs {
            return Err(self.glyph_error());
        }
        let fonts = self.fonts.checked_add(1).ok_or_else(|| self.byte_error())?;
        let mut bytes = self.retained_bytes;
        self.add_bytes(&mut bytes, usage.bytes)?;
        self.merge_reserve::<Font>(&mut bytes, 1)?;
        self.retained_bytes = bytes;
        self.glyphs = glyphs;
        self.glyph_slots = slots;
        self.fonts = fonts;
        Ok(())
    }
    /// Check a newly decoded input before moving any part into the aggregate.
    /// On failure this object is unchanged, so an error cannot consume budget.
    /// Charge a decoded input (model, report and glyph slots).
    pub fn admit(&mut self, loaded: &Loaded<FontSet>) -> Result<()> {
        let remaining = self
            .limits
            .max_total_bytes
            .checked_sub(self.retained_bytes)
            .ok_or_else(|| self.byte_error())?;
        let usage = loaded.fonts.retained_usage(remaining)?;
        let glyphs = self
            .glyphs
            .checked_add(usage.glyphs)
            .ok_or_else(|| self.glyph_error())?;
        let glyph_slots = self
            .glyph_slots
            .checked_add(usage.glyph_slots)
            .ok_or_else(|| self.glyph_error())?;
        if glyphs > self.limits.max_glyphs || glyph_slots > self.limits.max_glyphs {
            return Err(self.glyph_error());
        }
        let fonts = self
            .fonts
            .checked_add(usage.fonts)
            .ok_or_else(|| self.byte_error())?;
        let mut bytes = self.retained_bytes;
        self.add_bytes(&mut bytes, usage.bytes)?;
        self.report_bytes(&loaded.report, &mut bytes)?;
        self.merge_reserve::<Font>(&mut bytes, usage.fonts)?;
        self.merge_reserve::<Property>(&mut bytes, loaded.fonts.properties.len())?;
        self.merge_reserve::<ReportEntry>(&mut bytes, loaded.report.entries.len())?;
        self.retained_bytes = bytes;
        self.fonts = fonts;
        self.glyphs = glyphs;
        self.glyph_slots = glyph_slots;
        Ok(())
    }
}
