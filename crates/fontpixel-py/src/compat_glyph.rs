//! Handle over `fontpixel::compat::bdfparser::Glyph` (wrapped by the Python
//! `fontpixel.compat.bdfparser.Glyph`).
//!
//! A handle obtained from a font is attached: its edits write through to the
//! font's native model while the original slot exists. A handle built with
//! `RawGlyph(meta, font)` or `clone()` is detached.
use crate::compat::{RawFont, done, py2obj, v2py};
use crate::compat_bitmap::Bitmap;
use fontpixel::compat::bdfparser as bp;
use pyo3::prelude::*;
use serde_json::Value as V;

/// Glyph metadata snapshot with an optional live attachment.
#[pyclass(name = "RawGlyph", module = "fontpixel._native._bdfparser", unsendable)]
pub struct RawGlyph {
    /// The Rust compatibility glyph handle.
    pub glyph: bp::Glyph,
}

impl RawGlyph {
    pub(crate) fn wrap(glyph: bp::Glyph) -> Self {
        Self { glyph }
    }
}

fn bounds(bb: Option<(i64, i64, i64, i64)>) -> Option<[i64; 4]> {
    bb.map(|(a, b, c, d)| [a, b, c, d])
}

#[pymethods]
impl RawGlyph {
    /// Detached glyph from a metadata dict owned by `font` (`Glyph::new`).
    #[new]
    fn new(meta: &Bound<'_, PyAny>, font: PyRef<'_, RawFont>) -> PyResult<Self> {
        let m = py2obj(meta)?;
        done(bp::Glyph::new(&m, &font.font)).map(Self::wrap)
    }

    /// The 17 metadata slots in `META_TITLES` order (`Glyph::meta`).
    fn meta<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        v2py(py, &V::Array(self.glyph.meta().clone()))
    }

    /// Handle sharing the owning font's model (`Glyph::font`).
    fn font(&self) -> RawFont {
        RawFont::wrap(self.glyph.font())
    }

    /// Whether the original slot still exists unreplaced (`Glyph::is_attached`).
    fn is_attached(&self) -> bool {
        self.glyph.is_attached()
    }

    /// Code; unencoded glyphs have -1 (`Glyph::cp`).
    fn cp(&self) -> i64 {
        self.glyph.cp()
    }

    /// Character through the owner's charset (`Glyph::chr`).
    fn chr(&self) -> PyResult<Option<char>> {
        done(self.glyph.chr())
    }

    /// Draw: mode 0 font box, 1 glyph box, 2 raw hex rows, -1 custom `bb`
    /// (`Glyph::draw`).
    #[pyo3(signature = (mode=0, bb=None))]
    fn draw(&self, mode: i64, bb: Option<(i64, i64, i64, i64)>) -> PyResult<Bitmap> {
        done(self.glyph.draw(mode, bounds(bb))).map(Bitmap::wrap)
    }

    /// Origin displacement for a draw mode (`Glyph::origin`).
    #[pyo3(signature = (mode=0, fromorigin=false, xoff=None, yoff=None))]
    fn origin(
        &self,
        mode: i64,
        fromorigin: bool,
        xoff: Option<i64>,
        yoff: Option<i64>,
    ) -> PyResult<(i64, i64)> {
        done(self.glyph.origin(mode, fromorigin, xoff, yoff)).map(|[x, y]| (x, y))
    }

    /// Set metadata keys, writing through when attached (`Glyph::setmeta`).
    fn setmeta(&mut self, fields: &Bound<'_, PyAny>) -> PyResult<()> {
        let m = py2obj(fields)?;
        done(self.glyph.setmeta(&m).map(|_| ()))
    }

    /// Move the ink offsets (`Glyph::shift`).
    #[pyo3(signature = (dx=0, dy=0))]
    fn shift(&mut self, dx: i64, dy: i64) -> PyResult<()> {
        done(self.glyph.shift(dx, dy).map(|_| ()))
    }

    /// Replace the raster (`Glyph::setbitmap`).
    #[pyo3(signature = (bitmap, mode=0, bb=None, trim=true))]
    fn setbitmap(
        &mut self,
        bitmap: PyRef<'_, Bitmap>,
        mode: i64,
        bb: Option<(i64, i64, i64, i64)>,
        trim: bool,
    ) -> PyResult<()> {
        let b = bitmap.inner.clone();
        drop(bitmap);
        done(self.glyph.setbitmap(&b, mode, bounds(bb), trim).map(|_| ()))
    }

    /// Draw, apply a Python bitmap function, set the result (`Glyph::transform`).
    #[pyo3(signature = (function, mode=0))]
    fn transform(&mut self, function: Py<PyAny>, mode: i64) -> PyResult<()> {
        done(
            self.glyph
                .transform(|b| crate::compat_bitmap::call_bitmap_fn(&function, b), mode)
                .map(|_| ()),
        )
    }

    /// Detached copy sharing the font (`Glyph::clone`).
    fn clone(&self) -> Self {
        Self::wrap(self.glyph.clone())
    }

    /// The glyph drawn in the font box as a `#`/`.` picture.
    fn __str__(&self) -> PyResult<String> {
        done(self.glyph.draw(0, None)).map(|b| b.to_string())
    }
}
