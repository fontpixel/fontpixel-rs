//! Model handles: `FontSet`, `Font`, `Glyph` (`fontpixel::model`).
//!
//! Foundation pilot: construction, plain-data round trips, views and the
//! basic edits. The model area extends these classes in this file (and may
//! add `model_*.rs` files with further `#[wasm_bindgen(js_class = …)]`
//! blocks).
use crate::conv::{self, Options};
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::{FontLoc, GlyphLoc, JsBitmap, JsFont, JsFontSet, JsGlyph};
use fontpixel::model::{Codepoint, Font, FontSet, Glyph, GlyphId};
use js_sys::Array;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// The `GlyphId` with index `id`. `GlyphId` has no public constructor; its
/// serde form is the bare index, which is what plain data uses too.
pub fn glyph_id(id: usize) -> GlyphId {
    serde_json::from_value(serde_json::Value::from(id)).expect("GlyphId deserializes from an index")
}

/// `Some(id)` when `id` names a live glyph of `font`.
fn live(font: &Font, id: usize) -> Option<GlyphId> {
    let id = glyph_id(id);
    font.glyph(id).map(|_| id)
}

/// A codepoint argument: a number (Unicode) or the plain-data
/// `{"Unicode": n}` / `{"Legacy": n}`.
pub fn codepoint_arg(value: &JsValue) -> JsResult<Codepoint> {
    if let Some(n) = value.as_f64() {
        return Ok(Codepoint::Unicode(conv::int(n, "codepoint")?));
    }
    conv::from_js(value)
}

// ---------------------------------------------------------------- FontSet

#[wasm_bindgen(js_class = FontSet)]
impl JsFontSet {
    /// An empty set.
    #[wasm_bindgen(constructor)]
    pub fn constructor() -> JsFontSet {
        JsFontSet::new(FontSet::default())
    }
    /// Build from the plain-data form produced by `toDict()`.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "FontSetDict")] data: JsValue,
    ) -> JsResult<JsFontSet> {
        Ok(JsFontSet::new(conv::from_js(&data)?))
    }
    /// The complete plain-data form; copies every pixel.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "FontSetDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// Number of strikes.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> JsResult<usize> {
        self.read(|s| Ok(s.fonts.len()))
    }
    /// A live view of strike `index`; edits through it change this set.
    pub fn font(&self, index: f64) -> JsResult<JsFont> {
        let index: usize = conv::int(index, "index")?;
        let len = self.read(|s| Ok(s.fonts.len()))?;
        if index >= len {
            return Err(invalid(format!(
                "font index {index} out of range for {len} strikes"
            )));
        }
        Ok(JsFont::view(self.storage(), index))
    }
    /// Append a copy of `font`; returns its index.
    pub fn append(&self, font: &JsFont) -> JsResult<usize> {
        let font = font.snapshot()?;
        self.write(|s| {
            s.fonts.push(font);
            Ok(s.fonts.len() - 1)
        })
    }
    /// Remove and return strike `index` (default: the last) as an owned
    /// font. Views of later strikes now address the following font.
    pub fn pop(&self, index: Option<f64>) -> JsResult<JsFont> {
        self.write(|s| {
            let len = s.fonts.len();
            let index = match index {
                Some(i) => conv::int::<usize>(i, "index")?,
                None => len
                    .checked_sub(1)
                    .ok_or_else(|| invalid("pop from an empty FontSet"))?,
            };
            if index >= len {
                return Err(invalid(format!(
                    "font index {index} out of range for {len} strikes"
                )));
            }
            Ok(JsFont::new(s.fonts.remove(index)))
        })
    }
    /// An independent deep copy.
    pub fn copy(&self) -> JsResult<JsFontSet> {
        Ok(JsFontSet::new(self.snapshot()?))
    }
}

// ---------------------------------------------------------------- Font

#[wasm_bindgen(js_class = Font)]
impl JsFont {
    /// An empty strike.
    #[wasm_bindgen(constructor)]
    pub fn constructor() -> JsResult<JsFont> {
        Ok(JsFont::new(Font::from_glyphs(Vec::new()).js()?))
    }
    /// Build from the plain-data form produced by `toDict()` (removed glyph
    /// slots stay `null`, so IDs survive).
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "FontDict")] data: JsValue,
    ) -> JsResult<JsFont> {
        Ok(JsFont::new(conv::from_js(&data)?))
    }
    /// The complete plain-data form; copies every pixel.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "FontDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// Whether this handle is a view into a `FontSet`.
    #[wasm_bindgen(getter, js_name = isView)]
    pub fn is_view(&self) -> bool {
        matches!(self.loc, FontLoc::InSet(..))
    }
    /// Number of live glyphs (`Font::len`).
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> JsResult<usize> {
        self.read(|f| Ok(f.len()))
    }
    /// IDs of every live glyph in order.
    #[wasm_bindgen(js_name = glyphIds, unchecked_return_type = "number[]")]
    pub fn glyph_ids(&self) -> JsResult<Array> {
        self.read(|f| {
            Ok(f.glyphs()
                .map(|(id, _)| JsValue::from_f64(id.index() as f64))
                .collect())
        })
    }
    /// A live view of glyph `id`, or `undefined` (`Font::glyph`).
    pub fn glyph(&self, id: f64) -> JsResult<Option<JsGlyph>> {
        let id: usize = conv::int(id, "id")?;
        let id = self.read(|f| Ok(live(f, id)))?;
        Ok(id.map(|id| JsGlyph::view(&self.loc, id)))
    }
    /// Add a copy of `glyph`; returns its new ID (`Font::add_glyph`).
    #[wasm_bindgen(js_name = addGlyph)]
    pub fn add_glyph(&self, glyph: &JsGlyph) -> JsResult<usize> {
        let glyph = glyph.snapshot()?;
        self.write(|f| f.add_glyph(glyph).js().map(GlyphId::index))
    }
    /// Remove glyph `id`; returns it as an owned handle, or `undefined`.
    /// Its ID is never reused (`Font::remove_glyph`).
    #[wasm_bindgen(js_name = removeGlyph)]
    pub fn remove_glyph(&self, id: f64) -> JsResult<Option<JsGlyph>> {
        let id: usize = conv::int(id, "id")?;
        self.write(|f| {
            Ok(live(f, id)
                .and_then(|id| f.remove_glyph(id))
                .map(JsGlyph::new))
        })
    }
    /// An independent owned deep copy (also of a view).
    pub fn copy(&self) -> JsResult<JsFont> {
        Ok(JsFont::new(self.snapshot()?))
    }
}

// ---------------------------------------------------------------- Glyph

#[wasm_bindgen(js_class = Glyph)]
impl JsGlyph {
    /// A glyph holding a copy of `bitmap`; `options.codepoints` lists
    /// numbers (Unicode) or `{Unicode: n}` / `{Legacy: n}` values.
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        bitmap: &JsBitmap,
        #[wasm_bindgen(unchecked_optional_param_type = "{ codepoints?: CodepointLike[] }")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsGlyph> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["codepoints"],
            "Glyph options",
        )?;
        let mut glyph = Glyph::new(bitmap.snapshot()?);
        if let Some(list) = options.get("codepoints")? {
            let list: Array = list
                .dyn_into()
                .map_err(|_| invalid("Glyph options: `codepoints` must be an array"))?;
            glyph.codepoints = list
                .iter()
                .map(|v| codepoint_arg(&v))
                .collect::<JsResult<_>>()?;
        }
        Ok(JsGlyph::new(glyph))
    }
    /// Build from the plain-data form produced by `toDict()`.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "GlyphDict")] data: JsValue,
    ) -> JsResult<JsGlyph> {
        Ok(JsGlyph::new(conv::from_js(&data)?))
    }
    /// The complete plain-data form.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "GlyphDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// Whether this handle is a view into a font.
    #[wasm_bindgen(getter, js_name = isView)]
    pub fn is_view(&self) -> bool {
        matches!(self.loc, GlyphLoc::InFont(..))
    }
    /// A copy of the raster; assign a bitmap to edit the glyph.
    #[wasm_bindgen(getter)]
    pub fn bitmap(&self) -> JsResult<JsBitmap> {
        self.read(|g| Ok(JsBitmap::new(g.bitmap.clone())))
    }
    /// Replace the raster with a copy of `bitmap`.
    #[wasm_bindgen(setter)]
    pub fn set_bitmap(&self, bitmap: &JsBitmap) -> JsResult<()> {
        let bitmap = bitmap.snapshot()?;
        self.write(|g| {
            g.bitmap = bitmap;
            Ok(())
        })
    }
    /// The mapped codepoints (`{Unicode: n}` / `{Legacy: n}` plain data).
    #[wasm_bindgen(getter, unchecked_return_type = "CodepointDict[]")]
    pub fn codepoints(&self) -> JsResult<JsValue> {
        self.read(|g| conv::to_js(&g.codepoints))
    }
    /// An independent owned deep copy (also of a view).
    pub fn copy(&self) -> JsResult<JsGlyph> {
        Ok(JsGlyph::new(self.snapshot()?))
    }
}
