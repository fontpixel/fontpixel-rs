//! Shared handle classes for fonts, glyphs and bitmaps.
//!
//! Data stays in Rust. A handle either owns its value or is a *view* into a
//! containing value, so `font_set.fonts[0].glyph(gid).bitmap = …` edits the set
//! in place without copying the whole font. Views address their target by
//! index/ID; if the target disappears (a font removed from its set, a glyph
//! removed from its font) the view raises `InvalidInputError`.
//!
//! Area modules add Python-visible methods in their own `#[pymethods]` blocks
//! (pyo3 `multiple-pymethods`); this file only defines storage and the Rust-side
//! accessors every area uses: `read`/`write` closures and `snapshot` copies.
use crate::errors::InvalidInputError;
use fontpixel::model::{Font, FontSet, Glyph, GlyphBitmap, GlyphId};
use pyo3::prelude::*;
use std::sync::{Arc, RwLock};

/// Shared, lock-protected storage.
pub type Shared<T> = Arc<RwLock<T>>;

fn shared<T>(value: T) -> Shared<T> {
    Arc::new(RwLock::new(value))
}

fn poisoned() -> PyErr {
    InvalidInputError::new_err("handle state was poisoned by an earlier panic")
}

/// A set of strikes (`fontpixel::model::FontSet`).
#[pyclass(name = "FontSet", module = "fontpixel", frozen)]
pub struct PyFontSet {
    /// The shared set; views into it hold clones of this `Arc`.
    pub set: Shared<FontSet>,
}

impl PyFontSet {
    /// Wrap an owned set.
    pub fn new(set: FontSet) -> Self {
        Self { set: shared(set) }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&FontSet) -> PyResult<R>) -> PyResult<R> {
        f(&*self.set.read().map_err(|_| poisoned())?)
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut FontSet) -> PyResult<R>) -> PyResult<R> {
        f(&mut *self.set.write().map_err(|_| poisoned())?)
    }
    /// An independent copy of the set.
    pub fn snapshot(&self) -> PyResult<FontSet> {
        self.read(|s| Ok(s.clone()))
    }
}

/// Where a font handle's data lives.
#[derive(Clone)]
pub enum FontLoc {
    /// Owned by the handle.
    Owned(Shared<Font>),
    /// The `index`-th strike of a shared set.
    InSet(Shared<FontSet>, usize),
}

/// One strike (`fontpixel::model::Font`), owned or a view into a `FontSet`.
#[pyclass(name = "Font", module = "fontpixel", frozen)]
pub struct PyFont {
    /// Storage location.
    pub loc: FontLoc,
}

impl PyFont {
    /// Wrap an owned font.
    pub fn new(font: Font) -> Self {
        Self { loc: FontLoc::Owned(shared(font)) }
    }
    /// A view of strike `index` of `set`.
    pub fn view(set: &Shared<FontSet>, index: usize) -> Self {
        Self { loc: FontLoc::InSet(set.clone(), index) }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&Font) -> PyResult<R>) -> PyResult<R> {
        read_font(&self.loc, f)
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut Font) -> PyResult<R>) -> PyResult<R> {
        write_font(&self.loc, f)
    }
    /// An independent copy of the font.
    pub fn snapshot(&self) -> PyResult<Font> {
        self.read(|f| Ok(f.clone()))
    }
}

fn missing_font(index: usize) -> PyErr {
    InvalidInputError::new_err(format!("font {index} no longer exists in its set"))
}

/// Shared access to the font a location points at.
pub fn read_font<R>(loc: &FontLoc, f: impl FnOnce(&Font) -> PyResult<R>) -> PyResult<R> {
    match loc {
        FontLoc::Owned(font) => f(&*font.read().map_err(|_| poisoned())?),
        FontLoc::InSet(set, index) => {
            let set = set.read().map_err(|_| poisoned())?;
            f(set.fonts.get(*index).ok_or_else(|| missing_font(*index))?)
        }
    }
}

/// Exclusive access to the font a location points at.
pub fn write_font<R>(loc: &FontLoc, f: impl FnOnce(&mut Font) -> PyResult<R>) -> PyResult<R> {
    match loc {
        FontLoc::Owned(font) => f(&mut *font.write().map_err(|_| poisoned())?),
        FontLoc::InSet(set, index) => {
            let mut set = set.write().map_err(|_| poisoned())?;
            f(set.fonts.get_mut(*index).ok_or_else(|| missing_font(*index))?)
        }
    }
}

/// Where a glyph handle's data lives.
#[derive(Clone)]
pub enum GlyphLoc {
    /// Owned by the handle (not yet added to a font).
    Owned(Shared<Glyph>),
    /// Glyph `id` of a font location.
    InFont(FontLoc, GlyphId),
}

/// One glyph (`fontpixel::model::Glyph`), owned or a view into a font.
#[pyclass(name = "Glyph", module = "fontpixel", frozen)]
pub struct PyGlyph {
    /// Storage location.
    pub loc: GlyphLoc,
}

impl PyGlyph {
    /// Wrap an owned glyph.
    pub fn new(glyph: Glyph) -> Self {
        Self { loc: GlyphLoc::Owned(shared(glyph)) }
    }
    /// A view of glyph `id` in `font`.
    pub fn view(font: &FontLoc, id: GlyphId) -> Self {
        Self { loc: GlyphLoc::InFont(font.clone(), id) }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&Glyph) -> PyResult<R>) -> PyResult<R> {
        match &self.loc {
            GlyphLoc::Owned(g) => f(&*g.read().map_err(|_| poisoned())?),
            GlyphLoc::InFont(font, id) => read_font(font, |font| {
                f(font.glyph(*id).ok_or_else(|| missing_glyph(*id))?)
            }),
        }
    }
    /// Replace the glyph through `f`, keeping the containing font's indexes
    /// consistent (views go through `Font::edit_glyph`).
    pub fn write<R>(&self, f: impl FnOnce(&mut Glyph) -> PyResult<R>) -> PyResult<R> {
        match &self.loc {
            GlyphLoc::Owned(g) => f(&mut *g.write().map_err(|_| poisoned())?),
            GlyphLoc::InFont(font, id) => write_font(font, |font| {
                let mut glyph = font.glyph(*id).ok_or_else(|| missing_glyph(*id))?.clone();
                let out = f(&mut glyph)?;
                font.replace_glyph(*id, glyph).map_err(crate::errors::to_py)?;
                Ok(out)
            }),
        }
    }
    /// An independent copy of the glyph.
    pub fn snapshot(&self) -> PyResult<Glyph> {
        self.read(|g| Ok(g.clone()))
    }
}

fn missing_glyph(id: GlyphId) -> PyErr {
    InvalidInputError::new_err(format!("glyph {} no longer exists in its font", id.index()))
}

/// A Mono or Gray8 raster (`fontpixel::model::GlyphBitmap`). Always owned:
/// reading a glyph's bitmap returns a copy; assign it back to edit the glyph.
#[pyclass(name = "Bitmap", module = "fontpixel", frozen)]
pub struct PyBitmap {
    /// The raster.
    pub bitmap: Shared<GlyphBitmap>,
}

impl PyBitmap {
    /// Wrap an owned raster.
    pub fn new(bitmap: GlyphBitmap) -> Self {
        Self { bitmap: shared(bitmap) }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&GlyphBitmap) -> PyResult<R>) -> PyResult<R> {
        f(&*self.bitmap.read().map_err(|_| poisoned())?)
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut GlyphBitmap) -> PyResult<R>) -> PyResult<R> {
        f(&mut *self.bitmap.write().map_err(|_| poisoned())?)
    }
    /// An independent copy.
    pub fn snapshot(&self) -> PyResult<GlyphBitmap> {
        self.read(|b| Ok(b.clone()))
    }
}

/// Register the handle classes.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFontSet>()?;
    m.add_class::<PyFont>()?;
    m.add_class::<PyGlyph>()?;
    m.add_class::<PyBitmap>()?;
    Ok(())
}
