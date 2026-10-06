//! Python methods of the `Bitmap` handle: every `fontpixel::bitmap::Bitmap`
//! operation for both pixel formats.
//!
//! One Python class covers `Bitmap<Mono>` and `Bitmap<Gray8>`; `depth` is
//! `"mono"` (packed 1-bit, values 0..1) or `"gray8"` (one byte per pixel,
//! values 0..255). Conversions between them are explicit (`to_gray8`,
//! `to_gray8_palette`, `to_mono`). Methods that only exist for one format
//! raise `InvalidInputError` on the other; binary operations require equal
//! depths. Coordinates are `(x, y)` from the top-left pixel; crop/bbox
//! rectangles use bottom-left offsets as in the Rust API. Pixel data stays in
//! Rust: `as_bytes`, `to_bytes`, `to_rows` and `to_list` copy it out.
//! Enum options are the serde strings: `CompositeMode` (`"over"`,
//! `"replace"`, …), `Direction` (`"right"`, `"left"`, `"down"`, `"up"`),
//! `Alignment` (`"start"`, `"end"`); `ByteLayout` and `BitmapLimits` are
//! dicts with omitted fields defaulted.
use crate::conv::{from_py, from_py_required, to_py};
use crate::errors::IntoPy;
use crate::handles::PyBitmap;
use crate::model::invalid;
use fontpixel::bitmap::{
    Alignment, Bitmap, BitmapLimits, Bounds, ByteLayout, CompositeMode, Direction, Gray8, Mono,
    PixelFormat,
};
use fontpixel::model::GlyphBitmap;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyString, PyTuple};

/// Apply `$body` to the typed bitmap inside a `GlyphBitmap`.
macro_rules! each {
    ($value:expr, |$b:ident| $body:expr) => {
        match $value {
            GlyphBitmap::Mono($b) => $body,
            GlyphBitmap::Gray8($b) => $body,
        }
    };
}

/// Pixel format selected by a `depth` argument.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// `Bitmap<Mono>`.
    Mono,
    /// `Bitmap<Gray8>`.
    Gray8,
}

/// Parse `"mono"`/`"gray8"`.
pub fn depth_arg(depth: &str) -> PyResult<Depth> {
    match depth {
        "mono" => Ok(Depth::Mono),
        "gray8" => Ok(Depth::Gray8),
        _ => Err(invalid(format!(
            "bitmap depth must be 'mono' or 'gray8', not '{depth}'"
        ))),
    }
}

fn depth_name(b: &GlyphBitmap) -> &'static str {
    match b {
        GlyphBitmap::Mono(_) => "mono",
        GlyphBitmap::Gray8(_) => "gray8",
    }
}

/// Copy a bitmap argument: a `Bitmap` handle or its plain-data dict
/// (`{"mono": {"width", "height", "data"}}`).
pub fn bitmap_arg(obj: &Bound<'_, PyAny>) -> PyResult<GlyphBitmap> {
    if let Ok(b) = obj.cast::<PyBitmap>() {
        return b.get().snapshot();
    }
    if obj.is_instance_of::<PyDict>() {
        return from_py_required(obj);
    }
    Err(PyTypeError::new_err("expected a Bitmap"))
}

/// Copy bytes from `bytes`, `bytearray`, `memoryview` (or any buffer) or a
/// sequence of ints.
pub fn bytes_arg(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(b) = obj.cast::<PyBytes>() {
        return Ok(b.as_bytes().to_vec());
    }
    if let Ok(b) = obj.cast::<PyByteArray>() {
        return Ok(b.to_vec());
    }
    if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
        return obj.extract();
    }
    if obj.is_instance_of::<PyString>() {
        return Err(PyTypeError::new_err("expected bytes, not str"));
    }
    // memoryview and other buffer objects: `bytes(obj)` copies through the
    // buffer protocol, which the limited API of Python 3.10 cannot reach.
    let builtins = obj.py().import("builtins")?;
    let copied = builtins.getattr("bytes")?.call1((obj,))?;
    Ok(copied.cast::<PyBytes>()?.as_bytes().to_vec())
}

fn limits_arg(limits: Option<&Bound<'_, PyAny>>) -> PyResult<BitmapLimits> {
    from_py(limits)
}

fn mismatch() -> PyErr {
    invalid("bitmaps have different depths; convert one explicitly first")
}

fn only(name: &str, depth: &str) -> PyErr {
    invalid(format!("{name} is only available for {depth} bitmaps"))
}

fn text_palette(palette: Option<&Bound<'_, PyAny>>, depth: Depth) -> PyResult<Vec<(char, u8)>> {
    match palette {
        Some(p) if !p.is_none() => {
            let pairs: Vec<(char, u8)> = if let Ok(d) = p.cast::<PyDict>() {
                d.iter()
                    .map(|(k, v)| Ok((k.extract()?, v.extract()?)))
                    .collect::<PyResult<_>>()?
            } else {
                p.extract()?
            };
            Ok(pairs)
        }
        _ if depth == Depth::Mono => Ok(vec![('#', 1), ('.', 0), ('1', 1), ('0', 0)]),
        _ => Err(invalid("a gray8 text bitmap needs an explicit palette")),
    }
}

fn output_palette(palette: Option<&Bound<'_, PyAny>>, depth: Depth) -> PyResult<Vec<(u8, char)>> {
    match palette {
        Some(p) if !p.is_none() => {
            if let Ok(d) = p.cast::<PyDict>() {
                d.iter()
                    .map(|(k, v)| Ok((k.extract()?, v.extract()?)))
                    .collect()
            } else {
                p.extract()
            }
        }
        _ if depth == Depth::Mono => Ok(vec![(0, '.'), (1, '#')]),
        _ => Err(invalid("a gray8 bitmap needs an explicit output palette")),
    }
}

fn enum_arg<T: serde::de::DeserializeOwned>(obj: &Bound<'_, PyAny>) -> PyResult<T> {
    from_py_required(obj)
}

fn mode_arg(mode: &Bound<'_, PyAny>) -> PyResult<CompositeMode> {
    enum_arg(mode)
}

impl PyBitmap {
    fn depth_of(&self) -> PyResult<Depth> {
        self.read(|b| {
            Ok(match b {
                GlyphBitmap::Mono(_) => Depth::Mono,
                GlyphBitmap::Gray8(_) => Depth::Gray8,
            })
        })
    }
    /// A new handle from `f` applied to the raster.
    fn derive(
        &self,
        f: impl FnOnce(&GlyphBitmap) -> fontpixel::Result<GlyphBitmap>,
    ) -> PyResult<Self> {
        self.read(|b| f(b).py().map(Self::new))
    }
}

fn from_rows<F: PixelFormat>(
    rows: &[Vec<u8>],
    limits: BitmapLimits,
) -> fontpixel::Result<Bitmap<F>> {
    let height = u32::try_from(rows.len())
        .map_err(|_| fontpixel::Error::InvalidInput("too many bitmap rows".into()))?;
    let width = rows.first().map_or(0, Vec::len);
    let width = u32::try_from(width)
        .map_err(|_| fontpixel::Error::InvalidInput("bitmap rows too wide".into()))?;
    let mut out = Bitmap::<F>::with_limits(width, height, limits)?;
    for (y, row) in rows.iter().enumerate() {
        if row.len() != width as usize {
            return Err(fontpixel::Error::InvalidInput(
                "bitmap rows have different lengths".into(),
            ));
        }
        for (x, &v) in row.iter().enumerate() {
            out.set_pixel(x as u32, y as u32, v)?;
        }
    }
    Ok(out)
}

fn pixels<F: PixelFormat>(b: &Bitmap<F>) -> Vec<Vec<u8>> {
    (0..b.height())
        .map(|y| {
            (0..b.width())
                .map(|x| b.get_pixel(x, y).expect("in bounds"))
                .collect()
        })
        .collect()
}

/// Pixel rows as `u16` so pyo3 builds lists of ints rather than `bytes`.
fn widen(rows: Vec<Vec<u8>>) -> Vec<Vec<u16>> {
    rows.into_iter()
        .map(|r| r.into_iter().map(u16::from).collect())
        .collect()
}

#[pymethods]
impl PyBitmap {
    /// Bitmap(width=0, height=0, depth="mono", limits=None)
    ///
    /// A blank raster (`Bitmap::new`, or `Bitmap::with_limits` when `limits`
    /// is given).
    #[new]
    #[pyo3(signature = (width = 0, height = 0, depth = "mono", limits = None))]
    fn py_new(
        width: u32,
        height: u32,
        depth: &str,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let b: GlyphBitmap = match (depth_arg(depth)?, limits.filter(|l| !l.is_none())) {
            (Depth::Mono, None) => Bitmap::<Mono>::new(width, height).py()?.into(),
            (Depth::Gray8, None) => Bitmap::<Gray8>::new(width, height).py()?.into(),
            (Depth::Mono, l) => Bitmap::<Mono>::with_limits(width, height, limits_arg(l)?)
                .py()?
                .into(),
            (Depth::Gray8, l) => Bitmap::<Gray8>::with_limits(width, height, limits_arg(l)?)
                .py()?
                .into(),
        };
        Ok(Self::new(b))
    }
    /// A blank raster under explicit limits (`Bitmap::with_limits`).
    #[staticmethod]
    #[pyo3(signature = (width, height, limits, depth = "mono"))]
    fn with_limits(
        width: u32,
        height: u32,
        limits: &Bound<'_, PyAny>,
        depth: &str,
    ) -> PyResult<Self> {
        Self::py_new(width, height, depth, Some(limits))
    }
    /// From canonical storage: top-down rows, Mono rows packed MSB-first and
    /// padded to whole bytes, Gray8 one byte per pixel (`Bitmap::from_raw`).
    #[staticmethod]
    #[pyo3(signature = (width, height, data, depth = "mono", limits = None))]
    fn from_raw(
        width: u32,
        height: u32,
        data: &Bound<'_, PyAny>,
        depth: &str,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let data = bytes_arg(data)?;
        let limits = limits_arg(limits)?;
        Ok(Self::new(match depth_arg(depth)? {
            Depth::Mono => Bitmap::<Mono>::from_raw(width, height, &data, limits)
                .py()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::from_raw(width, height, &data, limits)
                .py()?
                .into(),
        }))
    }
    /// From an exact-sized byte stream in `layout` (a `ByteLayout` dict:
    /// `bit_order` "msb"/"lsb", `byte_order` "big"/"little", `row_alignment`,
    /// `scan_unit`, `row_order` "top-down"/"bottom-up") (`Bitmap::from_bytes`).
    #[staticmethod]
    #[pyo3(signature = (width, height, data, layout = None, depth = "mono", limits = None))]
    fn from_bytes(
        width: u32,
        height: u32,
        data: &Bound<'_, PyAny>,
        layout: Option<&Bound<'_, PyAny>>,
        depth: &str,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let data = bytes_arg(data)?;
        let layout: ByteLayout = from_py(layout)?;
        let limits = limits_arg(limits)?;
        Ok(Self::new(match depth_arg(depth)? {
            Depth::Mono => Bitmap::<Mono>::from_bytes(width, height, &data, layout, limits)
                .py()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::from_bytes(width, height, &data, layout, limits)
                .py()?
                .into(),
        }))
    }
    /// Parse text rows through `palette` (dict `{char: value}` or pairs); the
    /// Mono default accepts `#`/`1` as ink and `.`/`0` as paper. Blank leading
    /// and trailing lines are ignored (`Bitmap::from_text`).
    #[staticmethod]
    #[pyo3(signature = (text, palette = None, depth = "mono", limits = None))]
    fn from_text(
        text: &str,
        palette: Option<&Bound<'_, PyAny>>,
        depth: &str,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let depth = depth_arg(depth)?;
        let palette = text_palette(palette, depth)?;
        let limits = limits_arg(limits)?;
        Ok(Self::new(match depth {
            Depth::Mono => Bitmap::<Mono>::from_text(text, &palette, limits)
                .py()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::from_text(text, &palette, limits)
                .py()?
                .into(),
        }))
    }
    /// Parse strict BDF-style byte-padded hex rows into a Mono bitmap
    /// (`Bitmap::<Mono>::from_hex_rows`).
    #[staticmethod]
    #[pyo3(signature = (width, rows, limits = None))]
    fn from_hex_rows(
        width: u32,
        rows: Vec<String>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        Ok(Self::new(
            Bitmap::<Mono>::from_hex_rows(width, &rows, limits_arg(limits)?)
                .py()?
                .into(),
        ))
    }
    /// From a list of rows of pixel values, top row first (inverse of `to_rows`).
    #[staticmethod]
    #[pyo3(signature = (rows, depth = "mono", limits = None))]
    fn from_rows(
        rows: Vec<Vec<u8>>,
        depth: &str,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let limits = limits_arg(limits)?;
        Ok(Self::new(match depth_arg(depth)? {
            Depth::Mono => from_rows::<Mono>(&rows, limits).py()?.into(),
            Depth::Gray8 => from_rows::<Gray8>(&rows, limits).py()?.into(),
        }))
    }
    /// From the plain-data form produced by `to_dict`.
    #[staticmethod]
    #[pyo3(signature = (data))]
    fn from_dict(data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::new(from_py_required(data)?))
    }
    /// Plain data: `{"mono" | "gray8": {"width", "height", "data": [...]}}`.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|b| to_py(py, b))
    }
    /// Concatenate several bitmaps in one allocation (`Bitmap::concat_all`).
    /// `gaps` is empty or has one entry per boundary.
    #[staticmethod]
    #[pyo3(signature = (bitmaps, direction = None, alignment = None, gaps = None, limits = None))]
    fn concat_all(
        bitmaps: &Bound<'_, PyAny>,
        direction: Option<&Bound<'_, PyAny>>,
        alignment: Option<&Bound<'_, PyAny>>,
        gaps: Option<Vec<i64>>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let parts = bitmaps
            .try_iter()?
            .map(|b| bitmap_arg(&b?))
            .collect::<PyResult<Vec<_>>>()?;
        let direction = direction_arg(direction)?;
        let alignment = alignment_arg(alignment)?;
        let gaps = gaps.unwrap_or_default();
        let limits = limits_arg(limits)?;
        let gray = parts.iter().any(|b| matches!(b, GlyphBitmap::Gray8(_)));
        let out: GlyphBitmap = if gray {
            let typed = parts
                .iter()
                .map(|b| match b {
                    GlyphBitmap::Gray8(b) => Ok(b),
                    GlyphBitmap::Mono(_) => Err(mismatch()),
                })
                .collect::<PyResult<Vec<_>>>()?;
            Bitmap::concat_all(&typed, direction, alignment, &gaps, limits)
                .py()?
                .into()
        } else {
            let typed = parts
                .iter()
                .map(|b| match b {
                    GlyphBitmap::Mono(b) => Ok(b),
                    GlyphBitmap::Gray8(_) => Err(mismatch()),
                })
                .collect::<PyResult<Vec<_>>>()?;
            Bitmap::concat_all(&typed, direction, alignment, &gaps, limits)
                .py()?
                .into()
        };
        Ok(Self::new(out))
    }

    /// `"mono"` or `"gray8"`.
    #[getter]
    fn depth(&self) -> PyResult<&'static str> {
        self.read(|b| Ok(depth_name(b)))
    }
    /// Width in pixels.
    #[getter]
    fn width(&self) -> PyResult<u32> {
        self.read(|b| Ok(b.width()))
    }
    /// Height in pixels.
    #[getter]
    fn height(&self) -> PyResult<u32> {
        self.read(|b| Ok(b.height()))
    }
    /// Bytes per canonical row (`Bitmap::stride`).
    #[getter]
    fn stride(&self) -> PyResult<usize> {
        self.read(|b| Ok(each!(b, |b| b.stride())))
    }
    /// Largest pixel value: 1 for Mono, 255 for Gray8 (`PixelFormat::MAX`).
    #[getter]
    fn max_value(&self) -> PyResult<u8> {
        self.read(|b| {
            Ok(match b {
                GlyphBitmap::Mono(_) => Mono::MAX,
                GlyphBitmap::Gray8(_) => Gray8::MAX,
            })
        })
    }
    /// Whether rows are bit-packed (`PixelFormat::PACKED`).
    #[getter]
    fn packed(&self) -> PyResult<bool> {
        self.read(|b| {
            Ok(match b {
                GlyphBitmap::Mono(_) => Mono::PACKED,
                GlyphBitmap::Gray8(_) => Gray8::PACKED,
            })
        })
    }
    /// The bitmap's resource limits as a dict (`Bitmap::limits`).
    #[getter]
    fn limits<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|b| to_py(py, &each!(b, |b| b.limits())))
    }
    /// Replace the limits after checking the current size against them
    /// (`Bitmap::set_limits`).
    #[pyo3(signature = (limits))]
    fn set_limits(&self, limits: &Bound<'_, PyAny>) -> PyResult<()> {
        let limits: BitmapLimits = from_py_required(limits)?;
        self.write(|b| each!(b, |b| b.set_limits(limits)).py())
    }
    /// Pixel value at `(x, y)` (`Bitmap::get_pixel`).
    #[pyo3(signature = (x, y))]
    fn get_pixel(&self, x: u32, y: u32) -> PyResult<u8> {
        self.read(|b| each!(b, |b| b.get_pixel(x, y)).py())
    }
    /// Set the pixel at `(x, y)`; the value must fit the depth (`Bitmap::set_pixel`).
    #[pyo3(signature = (x, y, value))]
    fn set_pixel(&self, x: u32, y: u32, value: u8) -> PyResult<()> {
        self.write(|b| each!(b, |b| b.set_pixel(x, y, value)).py())
    }
    fn __getitem__(&self, xy: (u32, u32)) -> PyResult<u8> {
        self.get_pixel(xy.0, xy.1)
    }
    fn __setitem__(&self, xy: (u32, u32), value: u8) -> PyResult<()> {
        self.set_pixel(xy.0, xy.1, value)
    }
    /// A copy of the canonical storage (`Bitmap::as_bytes`).
    fn as_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.read(|b| Ok(PyBytes::new(py, b.as_bytes())))
    }
    /// Serialize in an explicit `ByteLayout` (default: MSB-first, byte rows,
    /// top-down) (`Bitmap::to_bytes`).
    #[pyo3(signature = (layout = None))]
    fn to_bytes<'py>(
        &self,
        py: Python<'py>,
        layout: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let layout: ByteLayout = from_py(layout)?;
        self.read(|b| {
            let bytes = each!(b, |b| b.to_bytes(layout)).py()?;
            Ok(PyBytes::new(py, &bytes))
        })
    }
    /// Rows of pixel values, top row first.
    fn to_rows(&self) -> PyResult<Vec<Vec<u16>>> {
        self.read(|b| Ok(widen(each!(b, |b| pixels(b)))))
    }
    /// Every pixel value in row-major order.
    fn to_list(&self) -> PyResult<Vec<u16>> {
        self.read(|b| Ok(widen(each!(b, |b| pixels(b))).concat()))
    }
    /// Text rows through `palette` (dict `{value: char}` or pairs; Mono
    /// default `.`/`#`) (`Bitmap::to_text`).
    #[pyo3(signature = (palette = None))]
    fn to_text(&self, palette: Option<&Bound<'_, PyAny>>) -> PyResult<String> {
        let palette = output_palette(palette, self.depth_of()?)?;
        self.read(|b| each!(b, |b| b.to_text(&palette)).py())
    }
    /// One fixed-size byte string per pixel, from `palette` (dict
    /// `{value: bytes}` or pairs) (`Bitmap::to_palette_bytes`).
    #[pyo3(signature = (palette))]
    fn to_palette_bytes<'py>(
        &self,
        py: Python<'py>,
        palette: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let entries: Vec<(u8, Vec<u8>)> = if let Ok(d) = palette.cast::<PyDict>() {
            d.iter()
                .map(|(k, v)| Ok((k.extract()?, bytes_arg(&v)?)))
                .collect::<PyResult<_>>()?
        } else {
            palette
                .try_iter()?
                .map(|item| {
                    let item = item?;
                    let (k, v): (u8, Bound<'_, PyAny>) = item.extract()?;
                    Ok((k, bytes_arg(&v)?))
                })
                .collect::<PyResult<_>>()?
        };
        let borrowed: Vec<(u8, &[u8])> = entries.iter().map(|(k, v)| (*k, v.as_slice())).collect();
        self.read(|b| {
            let bytes = each!(b, |b| b.to_palette_bytes(&borrowed)).py()?;
            Ok(PyBytes::new(py, &bytes))
        })
    }
    /// Byte-padded uppercase hex rows (BDF storage) (`Bitmap::<Mono>::to_hex_rows`).
    fn to_hex_rows(&self) -> PyResult<Vec<String>> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => b.to_hex_rows().py(),
            GlyphBitmap::Gray8(_) => Err(only("to_hex_rows", "mono")),
        })
    }
    /// Right-aligned lowercase hex rows (`Bitmap::<Mono>::to_numeric_hex_rows`).
    fn to_numeric_hex_rows(&self) -> PyResult<Vec<String>> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => b.to_numeric_hex_rows().py(),
            GlyphBitmap::Gray8(_) => Err(only("to_numeric_hex_rows", "mono")),
        })
    }
    /// Rows as integers, leftmost pixel most significant; width ≤ 64
    /// (`Bitmap::<Mono>::to_row_values`).
    fn to_row_values(&self) -> PyResult<Vec<u64>> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => b.to_row_values().py(),
            GlyphBitmap::Gray8(_) => Err(only("to_row_values", "mono")),
        })
    }

    /// Crop or zero-extend to `width`×`height` at bottom-left offset `(x, y)`
    /// (`Bitmap::crop`).
    #[pyo3(signature = (width, height, x = 0, y = 0))]
    fn crop(&self, width: u32, height: u32, x: i64, y: i64) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.crop(width, height, x, y).map(GlyphBitmap::from)))
    }
    /// Crop to a bounds dict `{width, height, x, y}` (`Bitmap::crop_bounds`;
    /// also `GlyphBitmap::crop`).
    #[pyo3(signature = (bounds))]
    fn crop_bounds(&self, bounds: &Bound<'_, PyAny>) -> PyResult<Self> {
        let bounds: Bounds = from_py_required(bounds)?;
        self.derive(|b| b.crop(bounds))
    }
    /// Add blank margins (`Bitmap::extend`).
    #[pyo3(signature = (left = 0, right = 0, top = 0, bottom = 0))]
    fn extend(&self, left: u32, right: u32, top: u32, bottom: u32) -> PyResult<Self> {
        self.derive(|b| {
            each!(b, |b| b
                .extend(left, right, top, bottom)
                .map(GlyphBitmap::from))
        })
    }
    /// Move pixels right/up inside the same size, filling with 0 (`Bitmap::shift`).
    #[pyo3(signature = (dx, dy))]
    fn shift(&self, dx: i64, dy: i64) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.shift(dx, dy).map(GlyphBitmap::from)))
    }
    /// Tight ink rectangle in bottom-left coordinates, as a bounds dict
    /// (`Bitmap::bbox`).
    fn bbox<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|b| to_py(py, &b.bbox()))
    }
    /// Crop to the ink (`Bitmap::trim`).
    fn trim(&self) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.trim().map(GlyphBitmap::from)))
    }
    /// Whether any pixel is nonzero (`Bitmap::has_ink`).
    fn has_ink(&self) -> PyResult<bool> {
        self.read(|b| Ok(each!(b, |b| b.has_ink())))
    }
    /// Mirror left/right (`Bitmap::flip_horizontal`).
    fn flip_horizontal(&self) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.flip_horizontal().map(GlyphBitmap::from)))
    }
    /// Mirror top/bottom (`Bitmap::flip_vertical`).
    fn flip_vertical(&self) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.flip_vertical().map(GlyphBitmap::from)))
    }
    /// Rotate clockwise by a multiple of 90 degrees (`Bitmap::rotate`).
    #[pyo3(signature = (degrees))]
    fn rotate(&self, degrees: i32) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.rotate(degrees).map(GlyphBitmap::from)))
    }
    /// Scale by integer factors (`Bitmap::enlarge`; `y` defaults to `x`).
    #[pyo3(signature = (x, y = None))]
    fn enlarge(&self, x: u32, y: Option<u32>) -> PyResult<Self> {
        let y = y.unwrap_or(x);
        self.derive(|b| each!(b, |b| b.enlarge(x, y).map(GlyphBitmap::from)))
    }
    /// Pad the width up to a multiple of `pixels` (`Bitmap::byte_pad`).
    #[pyo3(signature = (pixels))]
    fn byte_pad(&self, pixels: u32) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.byte_pad(pixels).map(GlyphBitmap::from)))
    }
    /// Concatenate with `other` (`Bitmap::concat`). The later operand wins
    /// where both have ink.
    #[pyo3(signature = (other, direction = None, alignment = None, gap = 0))]
    fn concat(
        &self,
        other: &Bound<'_, PyAny>,
        direction: Option<&Bound<'_, PyAny>>,
        alignment: Option<&Bound<'_, PyAny>>,
        gap: i64,
    ) -> PyResult<Self> {
        let other = bitmap_arg(other)?;
        let direction = direction_arg(direction)?;
        let alignment = alignment_arg(alignment)?;
        self.read(|b| {
            let out: GlyphBitmap = match (b, &other) {
                (GlyphBitmap::Mono(a), GlyphBitmap::Mono(o)) => {
                    a.concat(o, direction, alignment, gap).py()?.into()
                }
                (GlyphBitmap::Gray8(a), GlyphBitmap::Gray8(o)) => {
                    a.concat(o, direction, alignment, gap).py()?.into()
                }
                _ => return Err(mismatch()),
            };
            Ok(Self::new(out))
        })
    }
    /// Combine `source` into this bitmap in place at `(x, y)` with `mode`
    /// (`Bitmap::composite`).
    #[pyo3(signature = (source, x = 0, y = 0, mode = None))]
    fn composite(
        &self,
        source: &Bound<'_, PyAny>,
        x: i64,
        y: i64,
        mode: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let source = bitmap_arg(source)?;
        let mode = mode
            .map(mode_arg)
            .transpose()?
            .unwrap_or(CompositeMode::Over);
        self.write(|b| match (b, &source) {
            (GlyphBitmap::Mono(a), GlyphBitmap::Mono(s)) => {
                a.composite(s, x, y, mode);
                Ok(())
            }
            (GlyphBitmap::Gray8(a), GlyphBitmap::Gray8(s)) => {
                a.composite(s, x, y, mode);
                Ok(())
            }
            _ => Err(mismatch()),
        })
    }
    /// Combine `region` (bounds dict, top-left offsets into `source`) of
    /// `source` in place at `(x, y)` (`Bitmap::composite_region`).
    #[pyo3(signature = (source, region, x = 0, y = 0, mode = None))]
    fn composite_region(
        &self,
        source: &Bound<'_, PyAny>,
        region: &Bound<'_, PyAny>,
        x: i64,
        y: i64,
        mode: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let source = bitmap_arg(source)?;
        let region: Bounds = from_py_required(region)?;
        let mode = mode
            .map(mode_arg)
            .transpose()?
            .unwrap_or(CompositeMode::Over);
        self.write(|b| match (b, &source) {
            (GlyphBitmap::Mono(a), GlyphBitmap::Mono(s)) => {
                a.composite_region(s, region, x, y, mode);
                Ok(())
            }
            (GlyphBitmap::Gray8(a), GlyphBitmap::Gray8(s)) => {
                a.composite_region(s, region, x, y, mode);
                Ok(())
            }
            _ => Err(mismatch()),
        })
    }
    /// `composite(source, x, y, "over")` (`Bitmap::paste`).
    #[pyo3(signature = (source, x = 0, y = 0))]
    fn paste(&self, source: &Bound<'_, PyAny>, x: i64, y: i64) -> PyResult<()> {
        let source = bitmap_arg(source)?;
        self.write(|b| match (b, &source) {
            (GlyphBitmap::Mono(a), GlyphBitmap::Mono(s)) => {
                a.paste(s, x, y);
                Ok(())
            }
            (GlyphBitmap::Gray8(a), GlyphBitmap::Gray8(s)) => {
                a.paste(s, x, y);
                Ok(())
            }
            _ => Err(mismatch()),
        })
    }
    /// `paste(source, 0, 0)` (`Bitmap::overlay`).
    #[pyo3(signature = (source))]
    fn overlay(&self, source: &Bound<'_, PyAny>) -> PyResult<()> {
        let source = bitmap_arg(source)?;
        self.write(|b| match (b, &source) {
            (GlyphBitmap::Mono(a), GlyphBitmap::Mono(s)) => {
                a.overlay(s);
                Ok(())
            }
            (GlyphBitmap::Gray8(a), GlyphBitmap::Gray8(s)) => {
                a.overlay(s);
                Ok(())
            }
            _ => Err(mismatch()),
        })
    }
    /// Binary inversion in place: zero becomes full ink, nonzero zero
    /// (`Bitmap::invert`).
    fn invert(&self) -> PyResult<()> {
        self.write(|b| {
            each!(b, |b| b.invert());
            Ok(())
        })
    }
    /// Binary inversion in place with ink value `ink` (`Bitmap::invert_with`).
    #[pyo3(signature = (ink))]
    fn invert_with(&self, ink: u8) -> PyResult<()> {
        self.write(|b| each!(b, |b| b.invert_with(ink)).py())
    }
    /// Replace every pixel equal to `old` with `new`, in place (`Bitmap::replace`).
    #[pyo3(signature = (old, new))]
    fn replace(&self, old: u8, new: u8) -> PyResult<()> {
        self.write(|b| each!(b, |b| b.replace(old, new)).py())
    }
    /// Row-wise non-overlapping pattern replacement (`Bitmap::replace_pattern`).
    #[pyo3(signature = (old, new))]
    fn replace_pattern(&self, old: &Bound<'_, PyAny>, new: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (old, new) = (bytes_arg(old)?, bytes_arg(new)?);
        self.derive(|b| each!(b, |b| b.replace_pattern(&old, &new).map(GlyphBitmap::from)))
    }
    /// Displaced silhouette behind the ink (`Bitmap::shadow`).
    #[pyo3(signature = (dx, dy, value))]
    fn shadow(&self, dx: i64, dy: i64, value: u8) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.shadow(dx, dy, value).map(GlyphBitmap::from)))
    }
    /// One-pixel glow around pixels equal to `ink` (`Bitmap::glow`).
    #[pyo3(signature = (diagonals, ink, value))]
    fn glow(&self, diagonals: bool, ink: u8, value: u8) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.glow(diagonals, ink, value).map(GlyphBitmap::from)))
    }
    /// Rightward emboldening by `n` pixels (`Bitmap::bold`).
    #[pyo3(signature = (n = 1))]
    fn bold(&self, n: u32) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.bold(n).map(GlyphBitmap::from)))
    }
    /// Slant by `floor((row_from_bottom - base) / step)` (`Bitmap::italic`).
    #[pyo3(signature = (step, base = 0))]
    fn italic(&self, step: i64, base: i64) -> PyResult<Self> {
        self.derive(|b| each!(b, |b| b.italic(step, base).map(GlyphBitmap::from)))
    }
    /// Continuous-tone inversion `255 - v` in place (`Bitmap::<Gray8>::negate`).
    fn negate(&self) -> PyResult<()> {
        self.write(|b| match b {
            GlyphBitmap::Gray8(b) => {
                b.negate();
                Ok(())
            }
            GlyphBitmap::Mono(_) => Err(only("negate", "gray8")),
        })
    }
    /// Gray8 copy with paper 0 and ink 255 (`Bitmap::<Mono>::to_gray8`).
    fn to_gray8(&self) -> PyResult<Self> {
        self.derive_mono("to_gray8", |b| b.to_gray8())
    }
    /// Gray8 copy with explicit paper and ink values
    /// (`Bitmap::<Mono>::to_gray8_palette`).
    #[pyo3(signature = (paper, ink))]
    fn to_gray8_palette(&self, paper: u8, ink: u8) -> PyResult<Self> {
        self.derive_mono("to_gray8_palette", |b| b.to_gray8_palette(paper, ink))
    }
    /// Mono copy: ink where the value is `>= threshold` (`Bitmap::<Gray8>::to_mono`).
    #[pyo3(signature = (threshold))]
    fn to_mono(&self, threshold: u8) -> PyResult<Self> {
        self.read(|b| match b {
            GlyphBitmap::Gray8(b) => Ok(Self::new(b.to_mono(threshold).py()?.into())),
            GlyphBitmap::Mono(_) => Err(only("to_mono", "gray8")),
        })
    }

    /// An independent copy.
    fn copy(&self) -> PyResult<Self> {
        Ok(Self::new(self.snapshot()?))
    }
    fn __copy__(&self) -> PyResult<Self> {
        self.copy()
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.copy()
    }
    /// Equal depth, size and pixels (limits are not compared).
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.cast::<PyBitmap>() else {
            return Ok(false);
        };
        let other = other.get().snapshot()?;
        self.read(|b| Ok(*b == other))
    }
    /// Mono: rows of `.`/`#` (Rust `Display`); Gray8: the repr.
    fn __str__(&self) -> PyResult<String> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => Ok(b.to_string()),
            GlyphBitmap::Gray8(_) => Ok(format!(
                "Bitmap(width={}, height={}, depth='gray8')",
                b.width(),
                b.height()
            )),
        })
    }
    fn __repr__(&self) -> PyResult<String> {
        self.read(|b| {
            Ok(format!(
                "Bitmap(width={}, height={}, depth='{}')",
                b.width(),
                b.height(),
                depth_name(b)
            ))
        })
    }
}

impl PyBitmap {
    fn derive_mono(
        &self,
        name: &str,
        f: impl FnOnce(&Bitmap<Mono>) -> fontpixel::Result<Bitmap<Gray8>>,
    ) -> PyResult<Self> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => Ok(Self::new(f(b).py()?.into())),
            GlyphBitmap::Gray8(_) => Err(only(name, "mono")),
        })
    }
}

fn direction_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Direction> {
    match obj {
        Some(o) if !o.is_none() => enum_arg(o),
        _ => Ok(Direction::Right),
    }
}

fn alignment_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Alignment> {
    match obj {
        Some(o) if !o.is_none() => enum_arg(o),
        _ => Ok(Alignment::Start),
    }
}

/// Default `BitmapLimits` as a dict.
#[pyfunction]
fn default_bitmap_limits(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &BitmapLimits::default())
}

/// Default `ByteLayout` as a dict.
#[pyfunction]
fn default_byte_layout(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &ByteLayout::default())
}

/// Register the bitmap functions (the class itself is registered by `handles`).
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(default_bitmap_limits, m)?)?;
    m.add_function(wrap_pyfunction!(default_byte_layout, m)?)?;
    Ok(())
}
