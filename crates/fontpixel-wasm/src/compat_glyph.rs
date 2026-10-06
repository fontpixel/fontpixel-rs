//! Handle over `fontpixel::compat::bdfparser::Glyph` (`BdfparserRawGlyph`,
//! wrapped by the bdfparser-js `Glyph` of `fontpixel/compat/bdfparser`).
//!
//! A handle obtained from a font is attached: its edits write through to the
//! font's model while the original slot exists (followed through codepoint
//! moves made by any handle on the same glyph). A handle built with
//! `new BdfparserRawGlyph(meta, font)` or `clone()` is detached.
use crate::compat::{JsCompatFont, done, js2obj, v2js};
use crate::compat_bitmap::JsCompatBitmap;
use crate::conv;
use crate::errors::JsResult;
use crate::handles;
use fontpixel::compat::bdfparser as bp;
use js_sys::Function;
use serde_json::Value as V;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

/// Glyph metadata snapshot with an optional live attachment.
#[wasm_bindgen(js_name = BdfparserRawGlyph)]
pub struct JsCompatGlyph {
    pub(crate) glyph: RefCell<bp::Glyph>,
}

impl JsCompatGlyph {
    /// Wrap a Rust compatibility glyph handle.
    pub fn wrap(glyph: bp::Glyph) -> Self {
        Self {
            glyph: RefCell::new(glyph),
        }
    }
    fn read<R>(&self, f: impl FnOnce(&bp::Glyph) -> JsResult<R>) -> JsResult<R> {
        f(&*handles::borrow(&self.glyph, "BdfparserRawGlyph")?)
    }
    fn write<R>(&self, f: impl FnOnce(&mut bp::Glyph) -> JsResult<R>) -> JsResult<R> {
        f(&mut *handles::borrow_mut(&self.glyph, "BdfparserRawGlyph")?)
    }
}

/// A `[w, h, xoff, yoff]` box argument (`undefined`/`null` → None).
fn bounds(bb: &JsValue) -> JsResult<Option<[i64; 4]>> {
    if bb.is_undefined() || bb.is_null() {
        return Ok(None);
    }
    let v: Vec<i64> = conv::from_js(bb)?;
    <[i64; 4]>::try_from(v)
        .map(Some)
        .map_err(|_| crate::errors::invalid("bb must be [w, h, xoff, yoff]"))
}

#[wasm_bindgen(js_class = BdfparserRawGlyph)]
impl JsCompatGlyph {
    /// A detached glyph from a metadata object owned by `font` (`Glyph::new`;
    /// keys outside the 17 `META_TITLES` are ignored).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] meta: JsValue,
        font: &JsCompatFont,
    ) -> JsResult<JsCompatGlyph> {
        let m = js2obj(&meta)?;
        font.read(|f| done(bp::Glyph::new(&m, f)).map(Self::wrap))
    }

    /// The 17 metadata slots in `META_TITLES` order (`Glyph::meta`).
    #[wasm_bindgen(unchecked_return_type = "unknown[]")]
    pub fn meta(&self) -> JsResult<JsValue> {
        self.read(|g| v2js(&V::Array(g.meta().clone())))
    }

    /// A handle sharing the owning font's model (`Glyph::font`).
    pub fn font(&self) -> JsResult<JsCompatFont> {
        self.read(|g| Ok(JsCompatFont::wrap(g.font())))
    }

    /// Whether the original slot still exists unreplaced (`Glyph::is_attached`).
    #[wasm_bindgen(js_name = isAttached)]
    pub fn is_attached(&self) -> JsResult<bool> {
        self.read(|g| Ok(g.is_attached()))
    }

    /// Code; unencoded glyphs (and a null codepoint) give -1 (`Glyph::cp`).
    pub fn cp(&self) -> JsResult<JsValue> {
        self.read(|g| Ok(conv::int_to_js(i128::from(g.cp()))))
    }

    /// Character through the owner's charset (`Glyph::chr`).
    pub fn chr(&self) -> JsResult<Option<String>> {
        self.read(|g| done(g.chr()).map(|c| c.map(String::from)))
    }

    /// Draw: mode 0 font box, 1 glyph box, 2 raw hex rows, -1 custom `bb`
    /// `[w, h, xoff, yoff]` (`Glyph::draw`).
    pub fn draw(
        &self,
        mode: f64,
        #[wasm_bindgen(
            unchecked_param_type = "[number, number, number, number] | null | undefined"
        )]
        bb: JsValue,
    ) -> JsResult<JsCompatBitmap> {
        let mode: i64 = conv::int(mode, "mode")?;
        let bb = bounds(&bb)?;
        self.read(|g| done(g.draw(mode, bb)).map(JsCompatBitmap::wrap))
    }

    /// Origin displacement `[x, y]` for a draw mode (`Glyph::origin`).
    #[wasm_bindgen(unchecked_return_type = "[number, number]")]
    pub fn origin(
        &self,
        mode: f64,
        fromorigin: bool,
        xoff: JsValue,
        yoff: JsValue,
    ) -> JsResult<JsValue> {
        let mode: i64 = conv::int(mode, "mode")?;
        let xoff = crate::compat::opt_int(&xoff, "xoff")?;
        let yoff = crate::compat::opt_int(&yoff, "yoff")?;
        self.read(|g| conv::to_js(&done(g.origin(mode, fromorigin, xoff, yoff))?))
    }

    /// Set metadata keys, writing through when attached (`Glyph::setmeta`).
    pub fn setmeta(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] fields: JsValue,
    ) -> JsResult<()> {
        let m = js2obj(&fields)?;
        self.write(|g| done(g.setmeta(&m).map(|_| ())))
    }

    /// Move the ink offsets (`Glyph::shift`).
    pub fn shift(&self, dx: JsValue, dy: JsValue) -> JsResult<()> {
        let dx: i64 = conv::int_from_js(&dx, "dx")?;
        let dy: i64 = conv::int_from_js(&dy, "dy")?;
        self.write(|g| done(g.shift(dx, dy).map(|_| ())))
    }

    /// Replace the raster (`Glyph::setbitmap`).
    pub fn setbitmap(
        &self,
        bitmap: &JsCompatBitmap,
        mode: f64,
        #[wasm_bindgen(
            unchecked_param_type = "[number, number, number, number] | null | undefined"
        )]
        bb: JsValue,
        trim: bool,
    ) -> JsResult<()> {
        let mode: i64 = conv::int(mode, "mode")?;
        let bb = bounds(&bb)?;
        let b = bitmap.snapshot()?;
        self.write(|g| done(g.setbitmap(&b, mode, bb, trim).map(|_| ())))
    }

    /// Draw, apply a JS bitmap function `(raw: BdfparserRawBitmap) => void`
    /// that edits the handle it receives, and set the result (`Glyph::transform`).
    pub fn transform(&self, function: Function, mode: f64) -> JsResult<()> {
        let mode: i64 = conv::int(mode, "mode")?;
        self.write(|g| {
            done(
                g.transform(|b| crate::compat_bitmap::call_bitmap_fn(&function, b), mode)
                    .map(|_| ()),
            )
        })
    }

    /// Detached copy sharing the font (`Glyph::clone`).
    #[wasm_bindgen(js_name = clone)]
    pub fn clone_glyph(&self) -> JsResult<JsCompatGlyph> {
        self.read(|g| Ok(Self::wrap(g.clone())))
    }

    /// The glyph drawn in the font box as a `#`/`.` picture (`Display`).
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        self.read(|g| done(g.draw(0, None)).map(|b| b.to_string()))
    }
}
