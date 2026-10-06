//! Shared handle classes for fonts, glyphs and bitmaps.
//!
//! Data stays in WASM memory. A handle either owns its value or is a *view*
//! into a containing value, so `set.font(0).glyph(id).bitmap = b` edits the
//! set in place without copying the whole font. Views address their target by
//! index/ID; if the target disappears (a font removed from its set, a glyph
//! removed from its font) the view throws `InvalidInputError`. Reading a
//! glyph's `bitmap` returns a copy; assign it back to edit the glyph.
//!
//! Storage is `Rc<RefCell<…>>` (WASM is single-threaded here). Every exported
//! method takes `&self`, so wasm-bindgen's own aliasing guard never fires;
//! access goes through [`borrow`]/[`borrow_mut`], which turn a re-entrant
//! borrow (a JS callback touching a handle that the running call holds) into
//! `InvalidInputError` instead of a panic. Snapshot handle arguments before
//! borrowing the receiver mutably (`font.addGlyph(font.glyph(0))` must work).
//!
//! Area modules add JS-visible methods in their own files with
//! `#[wasm_bindgen(js_class = Font)] impl JsFont { … }` blocks (several impl
//! blocks per class are supported by wasm-bindgen); this file defines storage
//! and the Rust-side accessors only.
use crate::errors::{self, JsResult};
use fontpixel::model::{Font, FontSet, Glyph, GlyphBitmap, GlyphId};
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Shared, interior-mutable storage.
pub type Shared<T> = Rc<RefCell<T>>;

/// Wrap a value in new shared storage.
pub fn shared<T>(value: T) -> Shared<T> {
    Rc::new(RefCell::new(value))
}

fn busy(what: &str) -> JsValue {
    errors::invalid(format!(
        "{what} is in use by the running fontpixel call (re-entrant access from a callback)"
    ))
}

/// Shared access, or `InvalidInputError` while a callback re-enters a value
/// the running call holds exclusively.
pub fn borrow<'a, T>(cell: &'a RefCell<T>, what: &str) -> JsResult<Ref<'a, T>> {
    cell.try_borrow().map_err(|_| busy(what))
}

/// Exclusive access, or `InvalidInputError` while the value is borrowed.
pub fn borrow_mut<'a, T>(cell: &'a RefCell<T>, what: &str) -> JsResult<RefMut<'a, T>> {
    cell.try_borrow_mut().map_err(|_| busy(what))
}

/// A set of strikes (`fontpixel::model::FontSet`).
#[wasm_bindgen(js_name = FontSet)]
pub struct JsFontSet {
    pub(crate) set: Shared<FontSet>,
}

impl JsFontSet {
    /// Wrap an owned set.
    pub fn new(set: FontSet) -> Self {
        Self { set: shared(set) }
    }
    /// The shared storage (views into this set clone it).
    pub fn storage(&self) -> &Shared<FontSet> {
        &self.set
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&FontSet) -> JsResult<R>) -> JsResult<R> {
        f(&*borrow(&self.set, "FontSet")?)
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut FontSet) -> JsResult<R>) -> JsResult<R> {
        f(&mut *borrow_mut(&self.set, "FontSet")?)
    }
    /// An independent copy of the set.
    pub fn snapshot(&self) -> JsResult<FontSet> {
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
#[wasm_bindgen(js_name = Font)]
pub struct JsFont {
    pub(crate) loc: FontLoc,
}

impl JsFont {
    /// Wrap an owned font.
    pub fn new(font: Font) -> Self {
        Self {
            loc: FontLoc::Owned(shared(font)),
        }
    }
    /// A view of strike `index` of `set`.
    pub fn view(set: &Shared<FontSet>, index: usize) -> Self {
        Self {
            loc: FontLoc::InSet(set.clone(), index),
        }
    }
    /// The storage location.
    pub fn loc(&self) -> &FontLoc {
        &self.loc
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&Font) -> JsResult<R>) -> JsResult<R> {
        read_font(&self.loc, f)
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut Font) -> JsResult<R>) -> JsResult<R> {
        write_font(&self.loc, f)
    }
    /// An independent copy of the font.
    pub fn snapshot(&self) -> JsResult<Font> {
        self.read(|f| Ok(f.clone()))
    }
}

fn missing_font(index: usize) -> JsValue {
    errors::invalid(format!("font {index} no longer exists in its set"))
}

/// Shared access to the font a location points at.
pub fn read_font<R>(loc: &FontLoc, f: impl FnOnce(&Font) -> JsResult<R>) -> JsResult<R> {
    match loc {
        FontLoc::Owned(font) => f(&*borrow(font, "Font")?),
        FontLoc::InSet(set, index) => {
            let set = borrow(set, "FontSet")?;
            f(set.fonts.get(*index).ok_or_else(|| missing_font(*index))?)
        }
    }
}

/// Exclusive access to the font a location points at.
pub fn write_font<R>(loc: &FontLoc, f: impl FnOnce(&mut Font) -> JsResult<R>) -> JsResult<R> {
    match loc {
        FontLoc::Owned(font) => f(&mut *borrow_mut(font, "Font")?),
        FontLoc::InSet(set, index) => {
            let mut set = borrow_mut(set, "FontSet")?;
            f(set
                .fonts
                .get_mut(*index)
                .ok_or_else(|| missing_font(*index))?)
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
#[wasm_bindgen(js_name = Glyph)]
pub struct JsGlyph {
    pub(crate) loc: GlyphLoc,
}

impl JsGlyph {
    /// Wrap an owned glyph.
    pub fn new(glyph: Glyph) -> Self {
        Self {
            loc: GlyphLoc::Owned(shared(glyph)),
        }
    }
    /// A view of glyph `id` in `font`.
    pub fn view(font: &FontLoc, id: GlyphId) -> Self {
        Self {
            loc: GlyphLoc::InFont(font.clone(), id),
        }
    }
    /// The storage location.
    pub fn loc(&self) -> &GlyphLoc {
        &self.loc
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&Glyph) -> JsResult<R>) -> JsResult<R> {
        match &self.loc {
            GlyphLoc::Owned(g) => f(&*borrow(g, "Glyph")?),
            GlyphLoc::InFont(font, id) => read_font(font, |font| {
                f(font.glyph(*id).ok_or_else(|| missing_glyph(*id))?)
            }),
        }
    }
    /// Edit the glyph through `f`. A view edits a copy and stores it back with
    /// `Font::replace_glyph`, keeping the font's code/name indexes consistent.
    pub fn write<R>(&self, f: impl FnOnce(&mut Glyph) -> JsResult<R>) -> JsResult<R> {
        match &self.loc {
            GlyphLoc::Owned(g) => f(&mut *borrow_mut(g, "Glyph")?),
            GlyphLoc::InFont(font, id) => write_font(font, |font| {
                let mut glyph = font.glyph(*id).ok_or_else(|| missing_glyph(*id))?.clone();
                let out = f(&mut glyph)?;
                font.replace_glyph(*id, glyph).map_err(errors::to_js)?;
                Ok(out)
            }),
        }
    }
    /// An independent copy of the glyph.
    pub fn snapshot(&self) -> JsResult<Glyph> {
        self.read(|g| Ok(g.clone()))
    }
}

fn missing_glyph(id: GlyphId) -> JsValue {
    errors::invalid(format!("glyph {} no longer exists in its font", id.index()))
}

/// A Mono or Gray8 raster (`fontpixel::model::GlyphBitmap`). Always owned:
/// reading a glyph's bitmap returns a copy; assign it back to edit the glyph.
#[wasm_bindgen(js_name = Bitmap)]
pub struct JsBitmap {
    pub(crate) bitmap: Shared<GlyphBitmap>,
}

impl JsBitmap {
    /// Wrap an owned raster.
    pub fn new(bitmap: GlyphBitmap) -> Self {
        Self {
            bitmap: shared(bitmap),
        }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&GlyphBitmap) -> JsResult<R>) -> JsResult<R> {
        f(&*borrow(&self.bitmap, "Bitmap")?)
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut GlyphBitmap) -> JsResult<R>) -> JsResult<R> {
        f(&mut *borrow_mut(&self.bitmap, "Bitmap")?)
    }
    /// An independent copy.
    pub fn snapshot(&self) -> JsResult<GlyphBitmap> {
        self.read(|b| Ok(b.clone()))
    }
}
