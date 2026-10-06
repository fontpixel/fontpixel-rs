//! `fontpixel.sfnt`: the bounded sfnt container, TrueType collections, CFF
//! reading/writing and Unicode subsetting.
//!
//! The Rust parsers borrow the font bytes (`Sfnt<'a>`, `Collection<'a>`,
//! `Cff<'a>`). Their Python handles instead own the input buffer (a `bytes`
//! object is kept alive without copying) and the limits used to validate it,
//! and re-run the same parser for each method call. Parsing only reads the
//! table directory, so this costs a few microseconds per call and needs no
//! self-referential storage. Table records and collection faces are returned as
//! owned dicts whose `data`/`signature` are `bytes`.
//!
//! Table-level codecs live in the submodules registered by
//! [`crate::sfnt_tables`].
use crate::errors::IntoPy;
use crate::sfnt_support::{
    bytes_arg, dict, opt, out, pybytes, req, submodule, tag_arg, tag_str, with_bytes,
};
use fontpixel::sfnt::{
    self, Flavor, Sfnt, SfntBuilder, TableRecord, cff, collection::Collection, subset,
};
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::PyList;

fn record<'py>(py: Python<'py>, t: &TableRecord<'_>) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("tag", tag_str(t.tag).into_pyobject(py)?.into_any()),
            ("checksum", t.checksum.into_pyobject(py)?.into_any()),
            ("offset", t.offset.into_pyobject(py)?.into_any()),
            ("data", pybytes(py, t.data)),
        ],
    )
}

fn records<'py>(py: Python<'py>, tables: &[TableRecord<'_>]) -> PyResult<Bound<'py, PyList>> {
    let list = PyList::empty(py);
    for t in tables {
        list.append(record(py, t)?)?;
    }
    Ok(list)
}

/// A parsed standalone sfnt (`fontpixel::sfnt::Sfnt`).
///
/// The handle owns the input bytes and the `Limits` it was validated with;
/// every method re-parses the directory from those bytes.
#[pyclass(name = "Sfnt", module = "fontpixel.sfnt", frozen)]
pub struct PySfnt {
    /// The font bytes (shared with the caller's `bytes` object).
    pub data: PyBackedBytes,
    /// Limits used for parsing.
    pub limits: sfnt::Limits,
}

impl PySfnt {
    /// Parse and validate.
    fn create(data: PyBackedBytes, limits: sfnt::Limits) -> PyResult<Self> {
        Sfnt::parse(&data, &limits).py()?;
        Ok(Self { data, limits })
    }
    /// Run `f` on a fresh parse of the owned bytes.
    pub fn with<R>(&self, f: impl FnOnce(&Sfnt<'_>) -> PyResult<R>) -> PyResult<R> {
        let s = Sfnt::parse(&self.data, &self.limits).py()?;
        f(&s)
    }
}

#[pymethods]
impl PySfnt {
    /// Parse a standalone sfnt (`Sfnt.parse`); same as `Sfnt.parse(data, limits)`.
    #[new]
    #[pyo3(signature = (data, limits = None))]
    fn py_new(data: &Bound<'_, PyAny>, limits: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Self::create(bytes_arg(data)?, opt(limits)?)
    }
    /// Parse a standalone sfnt directory, checking table bounds and limits.
    ///
    /// `limits` is a `fontpixel.sfnt` Limits dict (see `defaults()`); checksums
    /// are checked separately by `validate_checksums()`.
    #[staticmethod]
    #[pyo3(signature = (data, limits = None), text_signature = "(data, limits=None)")]
    fn parse(data: &Bound<'_, PyAny>, limits: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Self::create(bytes_arg(data)?, opt(limits)?)
    }
    /// The font bytes this handle owns.
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.data.clone_ref(py).into_pyobject(py)?.into_any())
    }
    /// The limits dict used for parsing.
    #[getter]
    fn limits<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        out(py, &self.limits)
    }
    /// Outline flavor: `"true_type"`, `"cff"` or `"apple_true_type"`.
    #[getter]
    fn flavor<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.with(|s| out(py, &s.flavor()))
    }
    /// Directory records in tag order: dicts `{tag, checksum, offset, data}`.
    fn tables<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.with(|s| records(py, s.tables()))
    }
    /// The bytes of table `tag` (`str` such as `"OS/2"` or 4 `bytes`), or `None`.
    fn table<'py>(
        &self,
        py: Python<'py>,
        tag: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        let t = tag_arg(tag)?;
        self.with(|s| Ok(s.table(t).map(|d| pybytes(py, d))))
    }
    /// Check every table checksum and the head checksum adjustment.
    fn validate_checksums(&self) -> PyResult<()> {
        self.with(|s| s.validate_checksums().py())
    }
    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        self.with(|s| {
            Ok(format!(
                "<fontpixel.sfnt.Sfnt {} tables={} bytes={}>",
                out(py, &s.flavor())?,
                s.tables().len(),
                self.data.len()
            ))
        })
    }
}

/// Assembles a standalone sfnt (`fontpixel::sfnt::SfntBuilder`).
#[pyclass(name = "SfntBuilder", module = "fontpixel.sfnt")]
pub struct PySfntBuilder {
    builder: SfntBuilder,
}

#[pymethods]
impl PySfntBuilder {
    /// Start an empty font of `flavor` (`"true_type"`, `"cff"`, `"apple_true_type"`).
    #[new]
    fn py_new(flavor: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            builder: SfntBuilder::new(req::<Flavor>(flavor)?),
        })
    }
    /// Add table `tag`; duplicate or malformed tags raise `InvalidInputError`.
    fn insert(&mut self, tag: &Bound<'_, PyAny>, table: &Bound<'_, PyAny>) -> PyResult<()> {
        let t = tag_arg(tag)?;
        self.builder.insert(t, bytes_arg(table)?.to_vec()).py()
    }
    /// Build the font with sorted tables, checksums and head adjustment.
    #[pyo3(signature = (limits = None))]
    fn build<'py>(
        &self,
        py: Python<'py>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let l: sfnt::Limits = opt(limits)?;
        let bytes = self.builder.build(&l).py()?;
        Ok(pybytes(py, &bytes))
    }
}

/// `fontpixel::sfnt::checksum`: wrapping big-endian 32-bit word sum.
#[pyfunction]
#[pyo3(name = "checksum")]
fn sfnt_checksum(data: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(sfnt::checksum(&bytes_arg(data)?))
}

/// `Flavor::version`: the four-byte sfnt version of a flavor string.
#[pyfunction]
fn flavor_version(flavor: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(req::<Flavor>(flavor)?.version())
}

/// Default limit and option dicts of `fontpixel.sfnt` and its submodules,
/// keyed by Rust type path relative to `fontpixel::sfnt`.
#[pyfunction]
#[pyo3(name = "defaults")]
fn sfnt_defaults<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    use fontpixel::sfnt::{collection, glyf, gsub};
    dict(
        py,
        vec![
            ("Limits", out(py, &sfnt::Limits::default())?),
            (
                "collection.Limits",
                out(py, &collection::Limits::default())?,
            ),
            ("glyf.Limits", out(py, &glyf::Limits::default())?),
            ("cff.Limits", out(py, &cff::Limits::default())?),
            ("cff.DotOptions", out(py, &cff::DotOptions::default())?),
            (
                "bitmap.Limits",
                out(py, &fontpixel::sfnt::bitmap::Limits::default())?,
            ),
            (
                "gsub.LayoutLimits",
                out(py, &gsub::LayoutLimits::default())?,
            ),
            ("subset.Options", out(py, &subset::Options::default())?),
        ],
    )
}

// ---- collection --------------------------------------------------------------

/// A parsed TrueType/OpenType collection (`fontpixel::sfnt::collection::Collection`).
///
/// Owns the bytes and limits; each call re-parses (every face is checked).
#[pyclass(name = "Collection", module = "fontpixel.sfnt.collection", frozen)]
pub struct PyCollection {
    data: PyBackedBytes,
    limits: sfnt::collection::Limits,
}

impl PyCollection {
    fn with<R>(&self, f: impl FnOnce(&Collection<'_>) -> PyResult<R>) -> PyResult<R> {
        let c = Collection::parse(&self.data, &self.limits).py()?;
        f(&c)
    }
}

#[pymethods]
impl PyCollection {
    /// Parse a `ttcf` collection; same as `Collection.parse(data, limits)`.
    #[new]
    #[pyo3(signature = (data, limits = None))]
    fn py_new(data: &Bound<'_, PyAny>, limits: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Self::parse(data, limits)
    }
    /// Parse a `ttcf` collection, checking every face before exposing any.
    #[staticmethod]
    #[pyo3(signature = (data, limits = None), text_signature = "(data, limits=None)")]
    fn parse(data: &Bound<'_, PyAny>, limits: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let data = bytes_arg(data)?;
        let limits = opt(limits)?;
        Collection::parse(&data, &limits).py()?;
        Ok(Self { data, limits })
    }
    /// The collection bytes this handle owns.
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.data.clone_ref(py).into_pyobject(py)?.into_any())
    }
    /// TTC version (`0x00010000` or `0x00020000`).
    #[getter]
    fn version(&self) -> PyResult<u32> {
        self.with(|c| Ok(c.version))
    }
    /// Faces as dicts `{flavor, offset, tables}` with table records as in `Sfnt.tables()`.
    #[getter]
    fn faces<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.with(|c| {
            let list = PyList::empty(py);
            for f in &c.faces {
                list.append(dict(
                    py,
                    vec![
                        ("flavor", out(py, &f.flavor)?),
                        ("offset", f.offset.into_pyobject(py)?.into_any()),
                        ("tables", records(py, &f.tables)?.into_any()),
                    ],
                )?)?;
            }
            Ok(list)
        })
    }
    /// The opaque version-2 DSIG block, or `None`.
    #[getter]
    fn signature<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.with(|c| Ok(c.signature.map(|s| pybytes(py, s))))
    }
    /// Check every face's table checksums and head adjustment.
    fn validate_checksums(&self) -> PyResult<()> {
        self.with(|c| c.validate_checksums().py())
    }
    /// Extract face `index` as a standalone sfnt: `(bytes, report)`.
    #[pyo3(signature = (index, limits = None))]
    fn extract<'py>(
        &self,
        py: Python<'py>,
        index: usize,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
        let l = match limits {
            Some(l) if !l.is_none() => req(l)?,
            _ => self.limits.clone(),
        };
        self.with(|c| {
            let (bytes, report) = c.extract(index, &l).py()?;
            Ok((pybytes(py, &bytes), out(py, &report)?))
        })
    }
}

/// `collection::encode`: build a collection from standalone fonts, sharing
/// identical tables. Returns `(bytes, report)`.
#[pyfunction]
#[pyo3(name = "encode", signature = (fonts, version, limits = None))]
fn collection_encode<'py>(
    py: Python<'py>,
    fonts: Vec<Bound<'py, PyAny>>,
    version: u32,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let l: sfnt::collection::Limits = opt(limits)?;
    let inputs = fonts
        .iter()
        .map(|f| bytes_arg(f))
        .collect::<PyResult<Vec<_>>>()?;
    let (bytes, report) = py
        .detach(|| {
            let slices: Vec<&[u8]> = inputs.iter().map(|b| &**b).collect();
            sfnt::collection::encode(&slices, version, &l)
        })
        .py()?;
    Ok((pybytes(py, &bytes), out(py, &report)?))
}

// ---- CFF ---------------------------------------------------------------------

enum CffSource {
    Raw(PyBackedBytes, u16),
    Sfnt(PyBackedBytes, sfnt::Limits),
}

/// A parsed CFF1 table (`fontpixel::sfnt::cff::Cff`).
///
/// Owns the table (or containing sfnt) bytes and limits; each call re-parses.
#[pyclass(name = "Cff", module = "fontpixel.sfnt.cff", frozen)]
pub struct PyCff {
    source: CffSource,
    limits: cff::Limits,
}

impl PyCff {
    fn with<R>(&self, f: impl FnOnce(&cff::Cff<'_>) -> PyResult<R>) -> PyResult<R> {
        match &self.source {
            CffSource::Raw(d, upm) => f(&cff::Cff::parse(d, *upm, &self.limits).py()?),
            CffSource::Sfnt(d, sl) => {
                let s = Sfnt::parse(d, sl).py()?;
                f(&cff::Cff::from_sfnt(&s, &self.limits).py()?)
            }
        }
    }
    fn limits_or(&self, limits: Option<&Bound<'_, PyAny>>) -> PyResult<cff::Limits> {
        match limits {
            Some(l) if !l.is_none() => req(l),
            _ => Ok(self.limits.clone()),
        }
    }
}

#[pymethods]
impl PyCff {
    /// Parse a raw `CFF ` table with the containing font's units per em.
    #[staticmethod]
    #[pyo3(signature = (data, units_per_em, limits = None))]
    fn parse(
        data: &Bound<'_, PyAny>,
        units_per_em: u16,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let me = Self {
            source: CffSource::Raw(bytes_arg(data)?, units_per_em),
            limits: opt(limits)?,
        };
        me.with(|_| Ok(()))?;
        Ok(me)
    }
    /// The `CFF ` table of an `OTTO` sfnt handle (requires maxp 0.5 and head).
    #[staticmethod]
    #[pyo3(signature = (font, limits = None))]
    fn from_sfnt(
        py: Python<'_>,
        font: &crate::sfnt::PySfnt,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let me = Self {
            source: CffSource::Sfnt(font.data.clone_ref(py), font.limits.clone()),
            limits: opt(limits)?,
        };
        me.with(|_| Ok(()))?;
        Ok(me)
    }
    /// The CFF table bytes.
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.with(|c| Ok(pybytes(py, c.data())))
    }
    /// Number of glyphs.
    fn __len__(&self) -> PyResult<usize> {
        self.with(|c| Ok(c.len()))
    }
    /// Number of glyphs (`Cff::len`).
    fn len(&self) -> PyResult<usize> {
        self.with(|c| Ok(c.len()))
    }
    /// Whether the font has no glyphs.
    fn is_empty(&self) -> PyResult<bool> {
        self.with(|c| Ok(c.is_empty()))
    }
    /// Whether this is a CID-keyed font.
    fn is_cid(&self) -> PyResult<bool> {
        self.with(|c| Ok(c.is_cid()))
    }
    /// The glyph name bytes of a name-keyed font, or `None`.
    fn glyph_name<'py>(&self, py: Python<'py>, gid: u16) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.with(|c| Ok(c.glyph_name(gid).map(|n| pybytes(py, n))))
    }
    /// The SID or CID of glyph `gid`, or `None`.
    fn identifier(&self, gid: u16) -> PyResult<Option<u16>> {
        self.with(|c| Ok(c.identifier(gid)))
    }
    /// Number of global subroutines.
    fn global_subr_count(&self) -> PyResult<usize> {
        self.with(|c| Ok(c.global_subr_count()))
    }
    /// Evaluate one glyph: `{advance, commands}` (DecodedGlyph dict).
    #[pyo3(signature = (gid, limits = None))]
    fn glyph<'py>(
        &self,
        py: Python<'py>,
        gid: u16,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let l = self.limits_or(limits)?;
        self.with(|c| out(py, &c.glyph(gid, &l).py()?))
    }
    /// Evaluate every glyph under one whole-font budget.
    #[pyo3(signature = (limits = None))]
    fn read_all<'py>(
        &self,
        py: Python<'py>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let l = self.limits_or(limits)?;
        self.with(|c| out(py, &c.read_all(&l).py()?))
    }
    /// Subset to `glyphs` (old IDs in new order, `.notdef` first): CFF bytes.
    #[pyo3(signature = (glyphs, limits = None))]
    fn subset<'py>(
        &self,
        py: Python<'py>,
        glyphs: Vec<u16>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let l = self.limits_or(limits)?;
        self.with(|c| Ok(pybytes(py, &c.subset(&glyphs, &l).py()?)))
    }
}

/// `cff::Font::encode`: encode a CFF Font dict; returns `{data, bounds, maxp}`
/// with `data`/`maxp` as `bytes`.
#[pyfunction]
#[pyo3(name = "encode", signature = (font, limits = None))]
fn cff_encode<'py>(
    py: Python<'py>,
    font: &Bound<'_, PyAny>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let f: cff::Font = req(font)?;
    let l: cff::Limits = opt(limits)?;
    let e = py.detach(|| f.encode(&l)).py()?;
    with_bytes(py, &e, &[("data", &e.data), ("maxp", &e.maxp)])
}

/// `cff::Glyph::from_bitmap`: a CFF Glyph dict from a Mono bitmap handle.
#[pyfunction]
#[pyo3(signature = (name, advance, bitmap, options = None, limits = None))]
fn glyph_from_bitmap<'py>(
    py: Python<'py>,
    name: String,
    advance: f64,
    bitmap: &Bound<'py, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let b = crate::sfnt_support::mono(bitmap)?;
    let o: fontpixel::outline::Options = opt(options)?;
    let l: cff::Limits = opt(limits)?;
    out(
        py,
        &cff::Glyph::from_bitmap(name, advance, &b, &o, &l).py()?,
    )
}

// ---- subset ------------------------------------------------------------------

/// `subset::subset`: subset a standalone sfnt. Returns
/// `{bytes, glyph_mapping, report}`; `glyph_mapping` maps old to new glyph IDs.
#[pyfunction]
#[pyo3(name = "subset", signature = (data, options = None))]
fn subset_subset<'py>(
    py: Python<'py>,
    data: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let input = bytes_arg(data)?;
    let o: subset::Options = opt(options)?;
    let s = py.detach(|| subset::subset(&input, &o)).py()?;
    dict(
        py,
        vec![
            ("bytes", pybytes(py, &s.bytes)),
            ("glyph_mapping", out(py, &s.glyph_mapping)?),
            ("report", out(py, &s.report)?),
        ],
    )
}

/// `subset::Options::text`: options selecting every scalar of `text`.
#[pyfunction]
fn options_text<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    out(py, &subset::Options::text(text))
}

/// Register `fontpixel.sfnt` and its submodules.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let s = submodule(
        m,
        "sfnt",
        "fontpixel.sfnt",
        "Bounded sfnt container, OpenType tables, collections, CFF and subsetting.",
    )?;
    s.add_class::<PySfnt>()?;
    s.add_class::<PySfntBuilder>()?;
    s.add_function(wrap_pyfunction!(sfnt_checksum, &s)?)?;
    s.add_function(wrap_pyfunction!(flavor_version, &s)?)?;
    s.add_function(wrap_pyfunction!(sfnt_defaults, &s)?)?;

    let c = submodule(
        &s,
        "collection",
        "fontpixel.sfnt.collection",
        "TrueType/OpenType collections (ttcf).",
    )?;
    c.add_class::<PyCollection>()?;
    c.add_function(wrap_pyfunction!(collection_encode, &c)?)?;

    let f = submodule(
        &s,
        "cff",
        "fontpixel.sfnt.cff",
        "CFF1 reading, writing and subsetting.",
    )?;
    f.add_class::<PyCff>()?;
    f.add_function(wrap_pyfunction!(cff_encode, &f)?)?;
    f.add_function(wrap_pyfunction!(glyph_from_bitmap, &f)?)?;

    let u = submodule(
        &s,
        "subset",
        "fontpixel.sfnt.subset",
        "Compact Unicode subsets with GSUB/composite closure.",
    )?;
    u.add_function(wrap_pyfunction!(subset_subset, &u)?)?;
    u.add_function(wrap_pyfunction!(options_text, &u)?)?;

    crate::sfnt_tables::register(&s)?;
    Ok(())
}
