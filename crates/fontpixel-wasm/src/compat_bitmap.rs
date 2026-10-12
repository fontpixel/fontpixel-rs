//! `BdfparserRawBitmap`: the SPEC-v3 bitmap over the Rust
//! `compat::bdfparser::Bitmap` (a native Gray8 raster of labels 0, 1 and 2),
//! wrapped by the bdfparser-js `Bitmap` of `fontpixel/compat/bdfparser`.
//!
//! The pixels stay in WASM memory; `bindata` builds the row strings on each
//! read (a new array every time; assign `setBindata` to replace the raster).
//! Editing methods mutate in place (the JS wrapper returns itself for chaining).
use crate::compat::{callback_failed, done, stash, v2js};
use crate::conv;
use crate::errors::{self, JsResult};
use crate::handles::{self, Shared, shared};
use fontpixel::compat::bdfparser as bp;
use js_sys::{Function, Uint8Array};
use wasm_bindgen::prelude::*;

/// A bitmap of `'0'`/`'1'`/`'2'` rows (row 0 at the top) backed by a native raster.
#[wasm_bindgen(js_name = BdfparserRawBitmap)]
pub struct JsCompatBitmap {
    pub(crate) inner: Shared<bp::Bitmap>,
}

impl JsCompatBitmap {
    /// Wrap a Rust compatibility bitmap.
    pub fn wrap(inner: bp::Bitmap) -> Self {
        Self {
            inner: shared(inner),
        }
    }
    /// An independent copy of the raster.
    pub fn snapshot(&self) -> JsResult<bp::Bitmap> {
        self.read(|b| Ok(b.clone()))
    }
    fn read<R>(&self, f: impl FnOnce(&bp::Bitmap) -> JsResult<R>) -> JsResult<R> {
        f(&*handles::borrow(&self.inner, "BdfparserRawBitmap")?)
    }
    fn write<R>(&self, f: impl FnOnce(&mut bp::Bitmap) -> JsResult<R>) -> JsResult<R> {
        f(&mut *handles::borrow_mut(
            &self.inner,
            "BdfparserRawBitmap",
        )?)
    }
    fn edit(&self, f: impl FnOnce(&mut bp::Bitmap) -> fontpixel::Result<()>) -> JsResult<()> {
        self.write(|b| done(f(b)))
    }
}

/// Call a JS bitmap function `(raw: BdfparserRawBitmap) => void` on a Rust
/// bitmap (`transform`): the function receives a handle on a copy and edits it
/// in place (the JS wrapper copies a returned bitmap into it); the edited copy
/// is written back. A thrown exception is recorded and rethrown by `done`.
pub(crate) fn call_bitmap_fn(function: &Function, b: &mut bp::Bitmap) -> fontpixel::Result<()> {
    let cell = shared(b.clone());
    let handle = JsCompatBitmap {
        inner: cell.clone(),
    };
    if let Err(e) = function.call1(&JsValue::UNDEFINED, &JsValue::from(handle)) {
        stash(e);
        return Err(callback_failed("transform function"));
    }
    match cell.try_borrow() {
        Ok(result) => {
            *b = result.clone();
            Ok(())
        }
        Err(_) => Err(callback_failed("transform function")),
    }
}

fn rows_of(value: &JsValue) -> JsResult<Vec<String>> {
    conv::from_js(value)
}

/// A list of bitmaps for `concatall` (bitmaps are copied in with `push`, so
/// the JS handles stay valid).
#[wasm_bindgen(js_name = BdfparserRawBitmapList)]
#[derive(Default)]
pub struct JsCompatBitmapList {
    items: std::cell::RefCell<Vec<bp::Bitmap>>,
}

#[wasm_bindgen(js_class = BdfparserRawBitmapList)]
impl JsCompatBitmapList {
    /// An empty list.
    #[wasm_bindgen(constructor)]
    pub fn constructor() -> JsCompatBitmapList {
        Self::default()
    }
    /// Append a copy of `bitmap`.
    pub fn push(&self, bitmap: &JsCompatBitmap) -> JsResult<()> {
        let b = bitmap.snapshot()?;
        handles::borrow_mut(&self.items, "BdfparserRawBitmapList")?.push(b);
        Ok(())
    }
    /// Concatenate the bitmaps: `direction` 1 right, 2 left, 0 down, -1 up;
    /// `align` 1 bottom/left, 0 top/right; `offsets` gaps between neighbours
    /// (`Bitmap::concatall`).
    pub fn concatall(
        &self,
        direction: f64,
        align: f64,
        offsets: Vec<f64>,
    ) -> JsResult<JsCompatBitmap> {
        let direction: i64 = conv::int(direction, "direction")?;
        let align: i64 = conv::int(align, "align")?;
        let offsets = offsets
            .into_iter()
            .map(|o| conv::int::<i64>(o, "offsetlist"))
            .collect::<JsResult<Vec<_>>>()?;
        let items = handles::borrow(&self.items, "BdfparserRawBitmapList")?;
        done(bp::Bitmap::concatall(&items, direction, align, &offsets)).map(JsCompatBitmap::wrap)
    }
}

#[wasm_bindgen(js_class = BdfparserRawBitmap)]
impl JsCompatBitmap {
    /// A bitmap from rows of equal length made of `'0'`, `'1'`, `'2'` (`Bitmap::new`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "string[]")] rows: JsValue,
    ) -> JsResult<JsCompatBitmap> {
        let rows = rows_of(&rows)?;
        done(bp::Bitmap::new(&rows)).map(Self::wrap)
    }

    /// The rows as a new array of strings (`Bitmap::bindata`).
    pub fn bindata(&self) -> JsResult<Vec<String>> {
        self.read(|b| Ok(b.bindata()))
    }

    /// The rows of [`bindata`](Self::bindata) joined by `\n` (one string
    /// across the boundary; an empty bitmap and a single empty row both
    /// give `''`, told apart by `height()`).
    #[wasm_bindgen(js_name = bindataText)]
    pub fn bindata_text(&self) -> JsResult<String> {
        self.read(|b| Ok(b.bindata().join("\n")))
    }

    /// The constructor's raster from its rows joined by `\n` and the row
    /// count (one string across the boundary; same checks).
    #[wasm_bindgen(js_name = fromRowsText)]
    pub fn from_rows_text(text: &str, height: u32) -> JsResult<JsCompatBitmap> {
        let rows: Vec<String> = if height == 0 {
            Vec::new()
        } else {
            text.split('\n').map(str::to_owned).collect()
        };
        if rows.len() != height as usize {
            return Err(errors::invalid(
                "Bitmap rows must be rectangular labels 0, 1, 2",
            ));
        }
        done(bp::Bitmap::new(&rows)).map(Self::wrap)
    }

    /// Replace the raster with new rows (`Bitmap::new`).
    #[wasm_bindgen(js_name = setBindata)]
    pub fn set_bindata(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string[]")] rows: JsValue,
    ) -> JsResult<()> {
        let rows = rows_of(&rows)?;
        let b = done(bp::Bitmap::new(&rows))?;
        self.write(|inner| {
            *inner = b;
            Ok(())
        })
    }

    /// Replace the raster with a copy of another bitmap's.
    #[wasm_bindgen(js_name = assign)]
    pub fn assign(&self, other: &JsCompatBitmap) -> JsResult<()> {
        let b = other.snapshot()?;
        self.write(|inner| {
            *inner = b;
            Ok(())
        })
    }

    /// Parse a `toString()` picture (`#`=1, `.`=0, `&`=2, digits accepted;
    /// outer empty lines ignored) (`Bitmap::fromstring`).
    pub fn fromstring(text: &str) -> JsResult<JsCompatBitmap> {
        done(bp::Bitmap::fromstring(text)).map(Self::wrap)
    }

    /// An all-zero `w`×`h` bitmap (`Bitmap::blank`).
    pub fn blank(w: f64, h: f64) -> JsResult<JsCompatBitmap> {
        let (w, h) = (conv::int::<i64>(w, "w")?, conv::int::<i64>(h, "h")?);
        done(bp::Bitmap::blank(w, h)).map(Self::wrap)
    }

    /// Width in pixels (`Bitmap::width`).
    pub fn width(&self) -> JsResult<u32> {
        self.read(|b| Ok(b.width()))
    }

    /// Number of rows (`Bitmap::height`).
    pub fn height(&self) -> JsResult<u32> {
        self.read(|b| Ok(b.height()))
    }

    /// Independent copy.
    #[wasm_bindgen(js_name = clone)]
    pub fn clone_bitmap(&self) -> JsResult<JsCompatBitmap> {
        Ok(Self::wrap(self.snapshot()?))
    }

    /// Whether both bitmaps have the same rows (`Bitmap::equals`).
    pub fn equals(&self, other: &JsCompatBitmap) -> JsResult<bool> {
        let o = other.snapshot()?;
        self.read(|b| Ok(b.equals(&o)))
    }

    /// `#`/`.`/`&` picture of the rows (`Display`).
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        self.read(|b| Ok(b.to_string()))
    }

    /// Python-style representation (`Bitmap::repr`).
    pub fn repr(&self) -> JsResult<String> {
        self.read(|b| Ok(b.repr()))
    }

    /// Crop/extend to `w`×`h`; `(xoff, yoff)` is the new bottom-left corner
    /// relative to the current one, y up (`Bitmap::crop`).
    pub fn crop(&self, w: f64, h: f64, xoff: f64, yoff: f64) -> JsResult<()> {
        let (w, h) = (conv::int::<i64>(w, "w")?, conv::int::<i64>(h, "h")?);
        let (x, y) = (
            conv::int::<i64>(xoff, "xoff")?,
            conv::int::<i64>(yoff, "yoff")?,
        );
        self.edit(|b| b.crop(w, h, x, y).map(|_| ()))
    }

    /// Overlay another bitmap aligned top-left, keeping this size (`Bitmap::overlay`).
    pub fn overlay(&self, other: &JsCompatBitmap) -> JsResult<()> {
        let o = other.snapshot()?;
        self.write(|b| {
            b.overlay(&o);
            Ok(())
        })
    }

    /// Concatenate another bitmap in place (`Bitmap::concat`).
    pub fn concat(
        &self,
        other: &JsCompatBitmap,
        direction: f64,
        align: f64,
        offset: f64,
    ) -> JsResult<()> {
        let o = other.snapshot()?;
        let direction: i64 = conv::int(direction, "direction")?;
        let align: i64 = conv::int(align, "align")?;
        let offset: i64 = conv::int(offset, "offset")?;
        self.edit(|b| b.concat(&o, direction, align, offset).map(|_| ()))
    }

    /// Integer scale (`Bitmap::enlarge`).
    pub fn enlarge(&self, x: f64, y: f64) -> JsResult<()> {
        let (x, y) = (conv::int::<i64>(x, "x")?, conv::int::<i64>(y, "y")?);
        self.edit(|b| b.enlarge(x, y).map(|_| ()))
    }

    /// Replace a label substring in every row (`Bitmap::replace`).
    pub fn replace(&self, substr: &str, newsubstr: &str) -> JsResult<()> {
        self.edit(|b| b.replace(substr, newsubstr).map(|_| ()))
    }

    /// Add a label-2 shadow (`Bitmap::shadow`).
    pub fn shadow(&self, xoff: f64, yoff: f64) -> JsResult<()> {
        let (x, y) = (
            conv::int::<i64>(xoff, "xoff")?,
            conv::int::<i64>(yoff, "yoff")?,
        );
        self.edit(|b| b.shadow(x, y).map(|_| ()))
    }

    /// Add a label-2 glow (`mode` 0: 4-neighbour, 1: 8-neighbour) (`Bitmap::glow`).
    pub fn glow(&self, mode: f64) -> JsResult<()> {
        let mode: i64 = conv::int(mode, "mode")?;
        self.edit(|b| b.glow(mode).map(|_| ()))
    }

    /// Pad the width to a multiple of `bits` (`Bitmap::bytepad`).
    pub fn bytepad(&self, bits: f64) -> JsResult<()> {
        let bits: i64 = conv::int(bits, "bits")?;
        self.edit(|b| b.bytepad(bits).map(|_| ()))
    }

    /// Export data type 0 (string), 1 (rows), 2 (number rows), 3 (flat
    /// numbers), 4 (hex rows) or 5 (number per row; `bigint` beyond 2^53 − 1)
    /// (`Bitmap::todata`).
    pub fn todata(&self, datatype: f64) -> JsResult<JsValue> {
        let datatype: i64 = conv::int(datatype, "datatype")?;
        self.read(|b| v2js(&done(b.todata(datatype))?))
    }

    /// The hex (`todata(4)`) or decimal (`todata(5)`) text of each row
    /// (`Bitmap::row_number`).
    #[wasm_bindgen(js_name = rowNumbers)]
    pub fn row_numbers(&self, hex: bool) -> JsResult<Vec<String>> {
        self.read(|b| {
            b.label_rows()
                .map(|r| done(bp::Bitmap::row_number(r, hex)))
                .collect()
        })
    }

    /// The pixel labels (0, 1, 2) row by row, top to bottom, one byte each
    /// (`Bitmap::label_rows`).
    #[wasm_bindgen(js_name = labels)]
    pub fn labels(&self) -> JsResult<Uint8Array> {
        self.read(|b| {
            Ok(conv::bytes_to_js(
                &b.label_rows().flatten().copied().collect::<Vec<_>>(),
            ))
        })
    }

    /// Pillow `Image.frombytes` data for mode `'1'`, `'L'`, `'RGB'` or
    /// `'RGBA'`; `palette` maps labels 0/1/2 to byte records (single bits for
    /// mode `'1'`) (`Bitmap::tobytes`).
    pub fn tobytes(
        &self,
        mode: &str,
        #[wasm_bindgen(unchecked_param_type = "[number[], number[], number[]] | null | undefined")]
        palette: JsValue,
    ) -> JsResult<Uint8Array> {
        let palette: Option<[Vec<u8>; 3]> = conv::from_js_opt(&palette)?;
        self.read(|b| done(b.tobytes(mode, palette.as_ref())).map(|v| conv::bytes_to_js(&v)))
    }

    /// Pixel label at `(x, y)` (top-left origin) (`Bitmap::getpixel`).
    pub fn getpixel(&self, x: f64, y: f64) -> JsResult<u8> {
        let (x, y) = (conv::int::<i64>(x, "x")?, conv::int::<i64>(y, "y")?);
        self.read(|b| done(b.getpixel(x, y)))
    }

    /// Set the pixel at `(x, y)` to 0, 1 or 2 (`Bitmap::setpixel`).
    pub fn setpixel(&self, x: f64, y: f64, value: f64) -> JsResult<()> {
        let (x, y) = (conv::int::<i64>(x, "x")?, conv::int::<i64>(y, "y")?);
        let v: i64 = conv::int(value, "value")?;
        self.edit(|b| b.setpixel(x, y, v).map(|_| ()))
    }

    /// Mirror left-right (`Bitmap::fliph`).
    pub fn fliph(&self) -> JsResult<()> {
        self.edit(|b| b.fliph().map(|_| ()))
    }

    /// Mirror top-bottom (`Bitmap::flipv`).
    pub fn flipv(&self) -> JsResult<()> {
        self.edit(|b| b.flipv().map(|_| ()))
    }

    /// Rotate clockwise by a multiple of 90 degrees (`Bitmap::rotate`).
    pub fn rotate(&self, angle: f64) -> JsResult<()> {
        let angle: i64 = conv::int(angle, "angle")?;
        self.edit(|b| b.rotate(angle).map(|_| ()))
    }

    /// Swap ink and background (`Bitmap::invert`).
    pub fn invert(&self) -> JsResult<()> {
        self.write(|b| {
            b.invert();
            Ok(())
        })
    }

    /// Move the content by `(dx, dy)` (y up), keeping the size (`Bitmap::shift`).
    pub fn shift(&self, dx: f64, dy: f64) -> JsResult<()> {
        let (x, y) = (conv::int::<i64>(dx, "dx")?, conv::int::<i64>(dy, "dy")?);
        self.edit(|b| b.shift(x, y).map(|_| ()))
    }

    /// Ink bounding box `[w, h, xoff, yoff]`, bottom-left origin (`Bitmap::bbox`).
    #[wasm_bindgen(unchecked_return_type = "[number, number, number, number]")]
    pub fn bbox(&self) -> JsResult<JsValue> {
        self.read(|b| {
            let bb = b.bbox();
            conv::to_js(&(bb.width, bb.height, bb.x, bb.y))
        })
    }

    /// Remove empty outer rows and columns (`Bitmap::trim`).
    pub fn trim(&self) -> JsResult<()> {
        self.edit(|b| b.trim().map(|_| ()))
    }

    /// Embolden by `n` pixels (`Bitmap::bold`).
    pub fn bold(&self, n: f64) -> JsResult<()> {
        let n: i64 = conv::int(n, "n")?;
        self.edit(|b| b.bold(n).map(|_| ()))
    }

    /// Shear rows: one pixel per `step` rows above `base` (`Bitmap::italic`).
    pub fn italic(&self, step: f64, base: f64) -> JsResult<()> {
        let (s, b0) = (
            conv::int::<i64>(step, "step")?,
            conv::int::<i64>(base, "base")?,
        );
        self.edit(|b| b.italic(s, b0).map(|_| ()))
    }

    /// Paste the non-zero pixels of `bitmap` at `(x, y)` (top-left origin) (`Bitmap::paste`).
    pub fn paste(&self, bitmap: &JsCompatBitmap, x: f64, y: f64) -> JsResult<()> {
        let o = bitmap.snapshot()?;
        let (x, y) = (conv::int::<i64>(x, "x")?, conv::int::<i64>(y, "y")?);
        self.write(|b| {
            b.paste(&o, x, y);
            Ok(())
        })
    }

    /// SVG output: `format` `'path'`, `'pathOnly'` or `'rect'`; `shape`
    /// `'square'` or `'circle'` (`Bitmap::tosvg`).
    pub fn tosvg(
        &self,
        pixelsize: f64,
        format: &str,
        inverted: bool,
        xoff: f64,
        yoff: f64,
        shape: &str,
    ) -> JsResult<String> {
        self.read(|b| done(b.tosvg(pixelsize, format, inverted, xoff, yoff, shape)))
    }
}

/// Check that a bitmap-row argument is an array of strings (used by the
/// module functions taking `bindata`).
pub(crate) fn string_rows(value: &JsValue, what: &str) -> JsResult<Vec<String>> {
    conv::from_js(value).map_err(|_| errors::invalid(format!("{what} must be an array of strings")))
}
