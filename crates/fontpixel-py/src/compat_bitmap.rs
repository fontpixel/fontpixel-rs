//! `fontpixel.compat.bdfparser.Bitmap`: the SPEC-v3 bitmap over the Rust
//! `compat::bdfparser::Bitmap` (a native Gray8 raster of labels 0, 1 and 2).
//!
//! The pixels stay in Rust; `bindata` materializes the row strings on each
//! access (a new list every time, assign to `bindata` to replace the raster).
//! Editing methods mutate in place and return the same object for chaining.
use crate::compat::{done, stash, v2py, warn};
use fontpixel::compat::bdfparser as bp;
use pyo3::basic::CompareOp;
use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyInt, PyString};

/// A bitmap of `'0'`/`'1'`/`'2'` rows (row 0 at the top), backed by a native
/// raster. Equality compares pixels; instances are unhashable (mutable).
#[pyclass(name = "Bitmap", module = "fontpixel.compat.bdfparser", subclass, skip_from_py_object)]
#[derive(Clone)]
pub struct Bitmap {
    /// The Rust compatibility bitmap.
    pub inner: bp::Bitmap,
}

impl Bitmap {
    pub(crate) fn wrap(inner: bp::Bitmap) -> Self {
        Self { inner }
    }
}

fn rows_of(obj: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    if obj.is_instance_of::<PyString>() {
        return Err(PyTypeError::new_err(
            "Bitmap() expects a list of row strings, not a str",
        ));
    }
    obj.try_iter()?
        .map(|r| {
            let r = r?;
            if let Ok(b) = r.cast::<Bitmap>() {
                return Err(PyTypeError::new_err(format!(
                    "Bitmap rows must be str, not {}",
                    b.get_type().name()?
                )));
            }
            r.extract::<String>()
        })
        .collect()
}

fn check_rows(rows: &[String]) -> PyResult<()> {
    for (i, r) in rows.iter().enumerate() {
        if let Some(c) = r.chars().find(|c| !matches!(c, '0' | '1' | '2')) {
            return Err(PyValueError::new_err(format!(
                "Invalid character {c:?} in bitmap row {i} (only '0', '1' and '2' are pixels)"
            )));
        }
    }
    if rows.iter().any(|r| r.len() != rows[0].len()) {
        return Err(PyValueError::new_err("Bitmap rows have different lengths"));
    }
    Ok(())
}

fn label(v: &Bound<'_, PyAny>) -> PyResult<String> {
    if v.is_instance_of::<PyInt>() {
        Ok(v.str()?.to_string())
    } else {
        v.extract::<String>()
    }
}

/// Call a Python `fn(Bitmap) -> Bitmap | None` on a Rust bitmap (used by
/// `transform`); the result (or the argument, if `fn` returned None) is written back.
pub(crate) fn call_bitmap_fn(function: &Py<PyAny>, b: &mut bp::Bitmap) -> fontpixel::Result<()> {
    let failed = || fontpixel::Error::InvalidInput("transform function failed".into());
    Python::attach(|py| {
        let arg = Py::new(py, Bitmap::wrap(b.clone())).map_err(|e| {
            stash(e);
            failed()
        })?;
        let out = function.call1(py, (arg.clone_ref(py),)).map_err(|e| {
            stash(e);
            failed()
        })?;
        let result = if out.is_none(py) { arg.into_any() } else { out };
        match result.bind(py).cast::<Bitmap>() {
            Ok(r) => {
                *b = r.borrow().inner.clone();
                Ok(())
            }
            Err(_) => {
                stash(PyTypeError::new_err(
                    "transform function must return a Bitmap or None",
                ));
                Err(failed())
            }
        }
    })
}

#[pymethods]
impl Bitmap {
    /// `Bitmap(bin_bitmap_list)`: rows of equal length made of `'0'`, `'1'`, `'2'`.
    #[new]
    #[pyo3(signature = (bin_bitmap_list))]
    fn new(bin_bitmap_list: &Bound<'_, PyAny>) -> PyResult<Self> {
        let rows = rows_of(bin_bitmap_list)?;
        check_rows(&rows)?;
        done(bp::Bitmap::new(&rows)).map(Self::wrap)
    }

    /// The rows as a new list of strings (`Bitmap::bindata`); assign to replace.
    #[getter]
    fn get_bindata(&self) -> Vec<String> {
        self.inner.bindata()
    }

    #[setter]
    fn set_bindata(&mut self, rows: &Bound<'_, PyAny>) -> PyResult<()> {
        let rows = rows_of(rows)?;
        check_rows(&rows)?;
        self.inner = done(bp::Bitmap::new(&rows))?;
        Ok(())
    }

    /// `#`/`.`/`&` picture of the rows.
    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    /// `Bitmap(['…',\n        '…'])` (`Bitmap::repr`).
    fn __repr__(&self) -> String {
        self.inner.repr()
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: CompareOp) -> PyResult<Py<PyAny>> {
        let py = other.py();
        let Ok(other) = other.cast::<Bitmap>() else {
            return Ok(py.NotImplemented());
        };
        let eq = self.inner.equals(&other.borrow().inner);
        Ok(match op {
            CompareOp::Eq => eq.into_pyobject(py)?.to_owned().into_any().unbind(),
            CompareOp::Ne => (!eq).into_pyobject(py)?.to_owned().into_any().unbind(),
            _ => py.NotImplemented(),
        })
    }

    /// Same as `==` (`Bitmap::equals`).
    fn equals(&self, other: PyRef<'_, Bitmap>) -> bool {
        self.inner.equals(&other.inner)
    }

    /// Parse a `str()` picture (`#`=1, `.`=0, `&`=2, digits accepted; outer empty
    /// lines ignored) (`Bitmap::fromstring`).
    #[classmethod]
    fn fromstring(cls: &Bound<'_, pyo3::types::PyType>, text: &str) -> PyResult<Py<PyAny>> {
        let s = text.replace('\r', "");
        let lines = s.split('\n').collect::<Vec<_>>();
        let start = lines
            .iter()
            .position(|r| !r.trim().is_empty())
            .unwrap_or(lines.len());
        let end = lines
            .iter()
            .rposition(|r| !r.trim().is_empty())
            .map_or(start, |i| i + 1);
        for (i, line) in lines[start..end].iter().enumerate() {
            if let Some(c) = line
                .chars()
                .find(|c| !matches!(c, '#' | '.' | '&' | '0' | '1' | '2'))
            {
                return Err(PyValueError::new_err(format!(
                    "Invalid character {c:?} in bitmap row {i}"
                )));
            }
        }
        let rows = lines[start..end]
            .iter()
            .map(|r| r.replace('#', "1").replace('.', "0").replace('&', "2"))
            .collect::<Vec<_>>();
        check_rows(&rows)?;
        let b = done(bp::Bitmap::fromstring(text))?;
        construct(cls, b)
    }

    /// An all-zero `w`×`h` bitmap (`Bitmap::blank`).
    #[classmethod]
    fn blank(cls: &Bound<'_, pyo3::types::PyType>, w: i64, h: i64) -> PyResult<Py<PyAny>> {
        if w < 0 || h < 0 {
            return Err(PyValueError::new_err("Bitmap size must not be negative"));
        }
        let b = done(bp::Bitmap::blank(w, h))?;
        construct(cls, b)
    }

    /// Width in pixels (`Bitmap::width`).
    fn width(&self) -> u32 {
        self.inner.width()
    }

    /// Number of rows (`Bitmap::height`).
    fn height(&self) -> u32 {
        self.inner.height()
    }

    /// Independent copy.
    fn clone(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        let b = slf.borrow().inner.clone();
        construct(&slf.get_type(), b)
    }

    fn __copy__(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        Self::clone(slf)
    }

    fn __deepcopy__(slf: &Bound<'_, Self>, _memo: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Self::clone(slf)
    }

    /// Crop/extend to `w`×`h`; `(xoff, yoff)` is the new bottom-left corner
    /// relative to the current one, y up (`Bitmap::crop`).
    #[pyo3(signature = (w, h, xoff=0, yoff=0))]
    fn crop(mut slf: PyRefMut<'_, Self>, w: i64, h: i64, xoff: i64, yoff: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.crop(w, h, xoff, yoff).map(|_| ()))?;
        Ok(slf)
    }

    /// Overlay another bitmap aligned top-left, keeping this size; warns when the
    /// sizes differ (`Bitmap::overlay`).
    fn overlay<'py>(mut slf: PyRefMut<'py, Self>, bitmap: PyRef<'_, Bitmap>) -> PyResult<PyRefMut<'py, Self>> {
        let py = slf.py();
        if slf.inner.height() != bitmap.inner.height() {
            warn(py, "the bitmaps to overlay have different height")?;
        }
        if slf.inner.width() != bitmap.inner.width() {
            warn(py, "the bitmaps to overlay have different width")?;
        }
        slf.inner.overlay(&bitmap.inner);
        Ok(slf)
    }

    /// Concatenate bitmaps: `direction` 1 right, 2 left, 0 down, -1 up; `align`
    /// 1 bottom/left, 0 top/right; `offsetlist` gaps between neighbours
    /// (`Bitmap::concatall`).
    #[classmethod]
    #[pyo3(signature = (bitmaplist, direction=1, align=1, offsetlist=None))]
    fn concatall(
        cls: &Bound<'_, pyo3::types::PyType>,
        bitmaplist: Vec<PyRef<'_, Bitmap>>,
        direction: i64,
        align: i64,
        offsetlist: Option<Vec<i64>>,
    ) -> PyResult<Py<PyAny>> {
        let bs = bitmaplist.iter().map(|b| b.inner.clone()).collect::<Vec<_>>();
        let b = done(bp::Bitmap::concatall(
            &bs,
            direction,
            align,
            &offsetlist.unwrap_or_default(),
        ))?;
        construct(cls, b)
    }

    /// `a + b`: a new bitmap of `b` concatenated right of `a`, bottom-aligned
    /// (`Bitmap::concatall`).
    fn __add__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let Ok(other) = other.cast::<Bitmap>() else {
            return Ok(py.NotImplemented());
        };
        let bs = [slf.borrow().inner.clone(), other.borrow().inner.clone()];
        let b = done(bp::Bitmap::concatall(&bs, 1, 1, &[]))?;
        construct(&slf.get_type(), b)
    }

    /// `a * n` or `a * (x, y)`: a new enlarged bitmap (`Bitmap::enlarge`).
    fn __mul__(slf: &Bound<'_, Self>, mul: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let (x, y) = if let Ok(n) = mul.extract::<i64>() {
            (n, n)
        } else if let Ok(xy) = mul.extract::<(i64, i64)>() {
            xy
        } else {
            return Ok(py.NotImplemented());
        };
        let mut b = slf.borrow().inner.clone();
        done(b.enlarge(x, y).map(|_| ()))?;
        construct(&slf.get_type(), b)
    }

    /// Concatenate another bitmap in place (`Bitmap::concat`).
    #[pyo3(signature = (bitmap, direction=1, align=1, offset=0))]
    fn concat<'py>(
        mut slf: PyRefMut<'py, Self>,
        bitmap: PyRef<'_, Bitmap>,
        direction: i64,
        align: i64,
        offset: i64,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let other = bitmap.inner.clone();
        done(slf.inner.concat(&other, direction, align, offset).map(|_| ()))?;
        Ok(slf)
    }

    /// Integer scale (`Bitmap::enlarge`).
    #[pyo3(signature = (x=1, y=1))]
    fn enlarge(mut slf: PyRefMut<'_, Self>, x: i64, y: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.enlarge(x, y).map(|_| ()))?;
        Ok(slf)
    }

    /// Replace a label substring (str or int) in every row (`Bitmap::replace`).
    fn replace<'py>(
        mut slf: PyRefMut<'py, Self>,
        substr: &Bound<'_, PyAny>,
        newsubstr: &Bound<'_, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let (a, b) = (label(substr)?, label(newsubstr)?);
        done(slf.inner.replace(&a, &b).map(|_| ()))?;
        Ok(slf)
    }

    /// Add a label-2 shadow (`Bitmap::shadow`).
    #[pyo3(signature = (xoff=1, yoff=-1))]
    fn shadow(mut slf: PyRefMut<'_, Self>, xoff: i64, yoff: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.shadow(xoff, yoff).map(|_| ()))?;
        Ok(slf)
    }

    /// Add a label-2 glow (`mode` 0: 4-neighbour, 1: 8-neighbour) (`Bitmap::glow`).
    #[pyo3(signature = (mode=0))]
    fn glow(mut slf: PyRefMut<'_, Self>, mode: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.glow(mode).map(|_| ()))?;
        Ok(slf)
    }

    /// Pad the width to a multiple of `bits` (`Bitmap::bytepad`).
    #[pyo3(signature = (bits=8))]
    fn bytepad(mut slf: PyRefMut<'_, Self>, bits: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.bytepad(bits).map(|_| ()))?;
        Ok(slf)
    }

    /// Export data type 0 (str), 1 (rows), 2 (int rows), 3 (flat ints), 4 (hex
    /// rows) or 5 (int per row) (`Bitmap::todata`).
    #[pyo3(signature = (datatype=1))]
    fn todata<'py>(&self, py: Python<'py>, datatype: i64) -> PyResult<Bound<'py, PyAny>> {
        if !(0..=5).contains(&datatype) {
            return Err(PyValueError::new_err(format!("Unknown datatype {datatype}")));
        }
        v2py(py, &done(self.inner.todata(datatype))?)
    }

    /// Pillow `Image.frombytes` data for mode `'1'`, `'L'`, `'RGB'` or `'RGBA'`;
    /// `bytesdict` maps labels 0/1/2 to bytes (or bits for mode `'1'`)
    /// (`Bitmap::tobytes`). Unknown modes warn and fall back to RGB.
    #[pyo3(signature = (mode="RGB", bytesdict=None))]
    fn tobytes<'py>(
        &self,
        py: Python<'py>,
        mode: &str,
        bytesdict: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        if !matches!(mode, "1" | "L" | "RGB" | "RGBA") {
            warn(py, "Unknown mode, fallback to RGB")?;
        }
        let palette = match bytesdict {
            None => None,
            Some(d) => {
                let entry = |k: u8| -> PyResult<Vec<u8>> {
                    let v = d
                        .get_item(k)?
                        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(k))?;
                    if mode == "1" {
                        Ok(vec![v.extract::<u8>()?])
                    } else {
                        v.extract::<Vec<u8>>()
                    }
                };
                Some([entry(0)?, entry(1)?, entry(2)?])
            }
        };
        let b = done(self.inner.tobytes(mode, palette.as_ref()))?;
        Ok(PyBytes::new(py, &b))
    }

    /// Pixel value at `(x, y)` (top-left origin) (`Bitmap::getpixel`).
    fn getpixel(&self, x: i64, y: i64) -> PyResult<u8> {
        self.check_xy(x, y)?;
        done(self.inner.getpixel(x, y))
    }

    /// Set the pixel at `(x, y)` to 0, 1 or 2 (`Bitmap::setpixel`).
    #[pyo3(signature = (x, y, value=None))]
    fn setpixel<'py>(
        mut slf: PyRefMut<'py, Self>,
        x: i64,
        y: i64,
        value: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let v = match value {
            None => 1,
            Some(v) if v.is_instance_of::<PyInt>() && !v.is_instance_of::<pyo3::types::PyBool>() => {
                v.extract::<i64>().unwrap_or(-1)
            }
            Some(v) => {
                return Err(PyValueError::new_err(format!(
                    "Pixel value must be 0, 1 or 2, not {}",
                    v.repr()?
                )));
            }
        };
        if !(0..=2).contains(&v) {
            return Err(PyValueError::new_err(format!(
                "Pixel value must be 0, 1 or 2, not {v}"
            )));
        }
        slf.check_xy(x, y)?;
        done(slf.inner.setpixel(x, y, v).map(|_| ()))?;
        Ok(slf)
    }

    /// Mirror left-right (`Bitmap::fliph`).
    fn fliph(mut slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.fliph().map(|_| ()))?;
        Ok(slf)
    }

    /// Mirror top-bottom (`Bitmap::flipv`).
    fn flipv(mut slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.flipv().map(|_| ()))?;
        Ok(slf)
    }

    /// Rotate clockwise by a multiple of 90 degrees (`Bitmap::rotate`).
    #[pyo3(signature = (angle=90))]
    fn rotate(mut slf: PyRefMut<'_, Self>, angle: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.rotate(angle).map(|_| ()))?;
        Ok(slf)
    }

    /// Swap ink and background (`Bitmap::invert`).
    fn invert(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.inner.invert();
        slf
    }

    /// Move the content by `(dx, dy)` (y up), keeping the size (`Bitmap::shift`).
    #[pyo3(signature = (dx=0, dy=0))]
    fn shift(mut slf: PyRefMut<'_, Self>, dx: i64, dy: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.shift(dx, dy).map(|_| ()))?;
        Ok(slf)
    }

    /// Ink bounding box `(w, h, xoff, yoff)`, bottom-left origin (`Bitmap::bbox`).
    fn bbox(&self) -> (u32, u32, i64, i64) {
        let b = self.inner.bbox();
        (b.width, b.height, i64::from(b.x), i64::from(b.y))
    }

    /// Remove empty outer rows and columns (`Bitmap::trim`).
    fn trim(mut slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.trim().map(|_| ()))?;
        Ok(slf)
    }

    /// Embolden by `n` pixels (`Bitmap::bold`).
    #[pyo3(signature = (n=1))]
    fn bold(mut slf: PyRefMut<'_, Self>, n: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.bold(n).map(|_| ()))?;
        Ok(slf)
    }

    /// Shear rows: one pixel per `step` rows above `base` (`Bitmap::italic`).
    #[pyo3(signature = (step=2, base=0))]
    fn italic(mut slf: PyRefMut<'_, Self>, step: i64, base: i64) -> PyResult<PyRefMut<'_, Self>> {
        done(slf.inner.italic(step, base).map(|_| ()))?;
        Ok(slf)
    }

    /// Paste non-zero pixels of `bitmap` at `(x, y)` (top-left origin) (`Bitmap::paste`).
    #[pyo3(signature = (bitmap, x=0, y=0))]
    fn paste<'py>(mut slf: PyRefMut<'py, Self>, bitmap: PyRef<'_, Bitmap>, x: i64, y: i64) -> PyRefMut<'py, Self> {
        let other = bitmap.inner.clone();
        slf.inner.paste(&other, x, y);
        slf
    }

    /// SVG output: `format` `'path'`, `'pathOnly'` or `'rect'`; `shape` `'square'`
    /// or `'circle'` (`Bitmap::tosvg`).
    #[pyo3(signature = (pixelsize=1.0, format="path", inverted=false, xoff=0.0, yoff=0.0, shape="square"))]
    fn tosvg(
        &self,
        pixelsize: f64,
        format: &str,
        inverted: bool,
        xoff: f64,
        yoff: f64,
        shape: &str,
    ) -> PyResult<String> {
        done(self.inner.tosvg(pixelsize, format, inverted, xoff, yoff, shape))
    }
}

impl Bitmap {
    fn check_xy(&self, x: i64, y: i64) -> PyResult<()> {
        let (w, h) = (i64::from(self.inner.width()), i64::from(self.inner.height()));
        if !(0 <= y && y < h && 0 <= x && x < w) {
            return Err(PyIndexError::new_err(format!(
                "Pixel ({x}, {y}) is out of range of the {w}x{h} bitmap"
            )));
        }
        Ok(())
    }
}

/// Build an instance of `cls` (Bitmap or a Python subclass) holding `b`.
fn construct(cls: &Bound<'_, pyo3::types::PyType>, b: bp::Bitmap) -> PyResult<Py<PyAny>> {
    let py = cls.py();
    if cls.is(py.get_type::<Bitmap>()) {
        return Ok(Py::new(py, Bitmap::wrap(b))?.into_any());
    }
    let obj = cls.call1((Vec::<String>::new(),))?;
    obj.cast::<Bitmap>()?.borrow_mut().inner = b;
    Ok(obj.unbind())
}
