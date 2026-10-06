//! `fontpixel.merge`: merge selected glyphs from ordered font parts
//! (`fontpixel::merge`).
//!
//! `MergePart` borrows its font in Rust; here a part is a dict
//! `{font, selection, dy=0, label=None, license=None}` where `font` is a
//! `fontpixel.Font` handle or font dict, `selection` a `Selection` handle or
//! expression string and `license` a PartLicense dict. The fonts are copied
//! for the call. `merge` returns `{font: Font, report: dict}`; the report dict
//! is the serde MergeReport, and `report_to_json` gives the exact CLI JSON.
use crate::errors::{IntoPy, InvalidInputError};
use crate::handles::PyFont;
use crate::sfnt_support::{dict, out, req, submodule};
use crate::variants::font_arg;
use fontpixel::Font;
use fontpixel::merge::{
    self, GeometricWidth, MergeOptions, MergePart, MergeReport, MergeSpacing,
    ModificationPermission, PartLicense, Selection,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// A parsed Unicode selection (`fontpixel::merge::Selection`): a union of
/// ranges with exclusions applied last, e.g. `"U+4E00-9FFF,-U+4E01"` or
/// category names such as `"cjk"`.
#[pyclass(name = "Selection", module = "fontpixel.merge", frozen)]
pub struct PySelection {
    selection: Selection,
}

#[pymethods]
impl PySelection {
    /// `Selection::parse`: parse a selection expression.
    #[new]
    fn py_new(expression: &str) -> PyResult<Self> {
        Self::parse(expression)
    }
    /// `Selection::parse`: parse a selection expression.
    #[staticmethod]
    fn parse(expression: &str) -> PyResult<Self> {
        Ok(Self {
            selection: Selection::parse(expression).py()?,
        })
    }
    /// `Selection::contains`: whether `codepoint` is selected.
    fn contains(&self, codepoint: u32) -> bool {
        self.selection.contains(codepoint)
    }
    fn __contains__(&self, codepoint: u32) -> bool {
        self.selection.contains(codepoint)
    }
    /// `Selection::ranges`: normalized inclusive ranges `[{start, end}]`.
    fn ranges<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        out(py, self.selection.ranges())
    }
    /// `Selection::expression`: the original expression text.
    #[getter]
    fn expression(&self) -> &str {
        self.selection.expression()
    }
    /// `Selection::scalar_count`: number of selected Unicode scalars.
    fn scalar_count(&self) -> u64 {
        self.selection.scalar_count()
    }
    fn __repr__(&self) -> String {
        format!("Selection({:?})", self.selection.expression())
    }
}

fn selection_arg(obj: &Bound<'_, PyAny>) -> PyResult<Selection> {
    if let Ok(s) = obj.cast::<PySelection>() {
        return Ok(s.get().selection.clone());
    }
    let expression: String = obj.extract()?;
    Selection::parse(&expression).py()
}

struct Part {
    selection: Selection,
    dy: i64,
    label: Option<String>,
    license: PartLicense,
}

const PART_KEYS: [&str; 5] = ["font", "selection", "dy", "label", "license"];

fn part_arg(obj: &Bound<'_, PyAny>) -> PyResult<(Font, Part)> {
    let d = obj
        .cast::<PyDict>()
        .map_err(|_| InvalidInputError::new_err("a merge part must be a dict"))?;
    for key in d.keys() {
        let key: String = key.extract()?;
        if !PART_KEYS.contains(&key.as_str()) {
            return Err(InvalidInputError::new_err(format!(
                "unknown merge part key {key:?}; expected one of {PART_KEYS:?}"
            )));
        }
    }
    let get = |k: &str| -> PyResult<Option<Bound<'_, PyAny>>> {
        Ok(d.get_item(k)?.filter(|v| !v.is_none()))
    };
    let font = font_arg(
        &get("font")?.ok_or_else(|| InvalidInputError::new_err("merge part needs `font`"))?,
    )?;
    let selection = selection_arg(
        &get("selection")?
            .ok_or_else(|| InvalidInputError::new_err("merge part needs `selection`"))?,
    )?;
    let dy = get("dy")?.map(|v| v.extract()).transpose()?.unwrap_or(0);
    let label = get("label")?.map(|v| v.extract()).transpose()?;
    let license = match get("license")? {
        Some(l) => req(&l)?,
        None => PartLicense::default(),
    };
    Ok((
        font,
        Part {
            selection,
            dy,
            label,
            license,
        },
    ))
}

/// `merge::merge`: merge `parts` in priority order under `options`
/// (MergeOptions dict; `family` is required). Returns `{font, report}`.
#[pyfunction]
#[pyo3(name = "merge")]
fn merge_parts<'py>(
    py: Python<'py>,
    parts: Vec<Bound<'py, PyAny>>,
    options: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let o: MergeOptions = req(options)?;
    let (fonts, specs): (Vec<Font>, Vec<Part>) = parts
        .iter()
        .map(part_arg)
        .collect::<PyResult<Vec<_>>>()?
        .into_iter()
        .unzip();
    let result = py
        .detach(|| {
            let borrowed: Vec<MergePart<'_>> = fonts
                .iter()
                .zip(specs)
                .map(|(font, p)| {
                    let mut part = MergePart::new(font, p.selection);
                    part.dy = p.dy;
                    part.label = p.label;
                    part.license = p.license;
                    part
                })
                .collect();
            merge::merge(&borrowed, &o)
        })
        .py()?;
    dict(
        py,
        vec![
            ("font", Bound::new(py, PyFont::new(result.font))?.into_any()),
            ("report", out(py, &result.report)?),
        ],
    )
}

/// `MergeReport::to_json`: the deterministic JSON of a report dict, exactly
/// as the CLI prints it.
#[pyfunction]
fn report_to_json(report: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(req::<MergeReport>(report)?.to_json())
}

/// `MergeSpacing::as_str`: the CLI/JSON spelling of a spacing value.
#[pyfunction]
fn spacing_as_str(value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    Ok(req::<MergeSpacing>(value)?.as_str())
}

/// `GeometricWidth::as_str`: the CLI/JSON spelling of a geometric width.
#[pyfunction]
fn geometric_width_as_str(value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    Ok(req::<GeometricWidth>(value)?.as_str())
}

/// `ModificationPermission::as_str`: the JSON spelling of a permission.
#[pyfunction]
fn modification_as_str(value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    Ok(req::<ModificationPermission>(value)?.as_str())
}

/// Default MergeOptions (with an empty `family`, which `merge` rejects) and
/// PartLicense dicts.
#[pyfunction]
#[pyo3(name = "defaults")]
fn merge_defaults<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("MergeOptions", out(py, &MergeOptions::default())?),
            ("PartLicense", out(py, &PartLicense::default())?),
        ],
    )
}

/// Register `fontpixel.merge`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let g = submodule(
        m,
        "merge",
        "fontpixel.merge",
        "Merge selected glyphs from ordered font parts.",
    )?;
    g.add_class::<PySelection>()?;
    g.add_function(wrap_pyfunction!(merge_parts, &g)?)?;
    g.add_function(wrap_pyfunction!(report_to_json, &g)?)?;
    g.add_function(wrap_pyfunction!(spacing_as_str, &g)?)?;
    g.add_function(wrap_pyfunction!(geometric_width_as_str, &g)?)?;
    g.add_function(wrap_pyfunction!(modification_as_str, &g)?)?;
    g.add_function(wrap_pyfunction!(merge_defaults, &g)?)?;
    Ok(())
}
