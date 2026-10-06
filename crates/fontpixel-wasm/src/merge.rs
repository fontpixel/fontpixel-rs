//! `merge`: merge selected glyphs from ordered font parts
//! (`fontpixel::merge`, the library-level merge; the command-level merge
//! with bytes in and named outputs is in the operations API).
//!
//! `MergePart` borrows its font in Rust; here a part is a plain object
//! `{font, selection, dy?, label?, license?}` where `font` is a `Font` handle
//! (copied for the call) or `FontDict`, `selection` a `Selection` handle or
//! expression string, `dy` an integer (number or bigint) and `license` a
//! `PartLicense` object. `merge.merge(parts, options)` returns `{font,
//! report}` with `font` an owned `Font` handle and `report` the
//! `MergeReport` plain data; `merge.reportToJson` gives the exact CLI JSON.
use crate::conv;
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::JsFont;
use crate::sfnt_support::{font_arg, handle_of, object, offer, req};
use fontpixel::Font;
use fontpixel::merge::{
    self, GeometricWidth, MergeOptions, MergePart, MergeReport, MergeSpacing,
    ModificationPermission, PartLicense, Selection,
};
use js_sys::{Array, Object, Reflect};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const MERGE_TS: &str = r#"
/** One part of `merge.merge` (`fontpixel::merge::MergePart`). */
export interface MergePartLike {
  /** Source strike: a `Font` handle (copied) or `FontDict` plain data. */
  font: Font | FontDict;
  /** Unicode selection: a `Selection` handle or an expression such as `"U+4E00-9FFF,-U+4E01"` or `"cjk"`. */
  selection: Selection | string;
  /** Vertical offset in pixels, positive up (default 0). */
  dy?: WideInt;
  /** Component identifier for the report (default: the font's family). */
  label?: string | null;
  /** Supplemental notices (default: none, modification "unknown"). */
  license?: PartLicense | null;
}

/** Result of `merge.merge`: the new strike (an owned handle) and the complete report. */
export interface MergeLibResult {
  font: Font;
  report: MergeReport;
}
"#;

/// A parsed Unicode selection (`fontpixel::merge::Selection`): a union of
/// ranges with exclusions applied last, e.g. `"U+4E00-9FFF,-U+4E01"`, or
/// category names such as `"cjk"`.
#[wasm_bindgen(js_name = Selection)]
pub struct JsSelection {
    selection: Rc<Selection>,
}

#[wasm_bindgen(js_class = Selection)]
impl JsSelection {
    /// `Selection::parse`: parse a selection expression.
    #[wasm_bindgen(constructor)]
    pub fn constructor(expression: &str) -> JsResult<JsSelection> {
        Self::parse(expression)
    }
    /// `Selection::parse`: parse a selection expression.
    pub fn parse(expression: &str) -> JsResult<JsSelection> {
        Ok(Self {
            selection: Rc::new(Selection::parse(expression).js()?),
        })
    }
    /// `Selection::contains`: whether `codepoint` is selected.
    pub fn contains(&self, codepoint: f64) -> JsResult<bool> {
        Ok(self.selection.contains(conv::int(codepoint, "codepoint")?))
    }
    /// `Selection::ranges`: normalized inclusive ranges `[{start, end}]`.
    #[wasm_bindgen(unchecked_return_type = "CodepointRange[]")]
    pub fn ranges(&self) -> JsResult<JsValue> {
        conv::to_js(self.selection.ranges())
    }
    /// `Selection::expression`: the original expression text.
    #[wasm_bindgen(getter)]
    pub fn expression(&self) -> String {
        self.selection.expression().to_owned()
    }
    /// `Selection::scalar_count`: number of selected Unicode scalars.
    #[wasm_bindgen(js_name = scalarCount)]
    pub fn scalar_count(&self) -> f64 {
        // At most 0x110000 scalars: always exact as a number.
        self.selection.scalar_count() as f64
    }
    /// `Selection("U+41-5A,-U+45")`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!("Selection({:?})", self.selection.expression())
    }
    /// Internal: share this handle's storage with the running call.
    #[wasm_bindgen(js_name = __fpShareStorage, skip_typescript)]
    pub fn share_storage(&self) -> JsResult<()> {
        offer(Box::new(self.selection.clone()));
        Ok(())
    }
}

fn selection_arg(value: &JsValue) -> JsResult<Selection> {
    if let Some(selection) = handle_of::<Rc<Selection>>(value, "Selection")? {
        return Ok((*selection).clone());
    }
    let expression = value
        .as_string()
        .ok_or_else(|| invalid("merge part `selection` must be a Selection or a string"))?;
    Selection::parse(&expression).js()
}

struct Part {
    selection: Selection,
    dy: i64,
    label: Option<String>,
    license: PartLicense,
}

const PART_KEYS: [&str; 5] = ["font", "selection", "dy", "label", "license"];

fn part_arg(value: &JsValue) -> JsResult<(Font, Part)> {
    let object = value
        .dyn_ref::<Object>()
        .filter(|_| !value.is_function() && !Array::is_array(value))
        .ok_or_else(|| invalid("a merge part must be a plain object"))?;
    for key in Object::keys(object).iter() {
        let key = key.as_string().unwrap_or_default();
        if !PART_KEYS.contains(&key.as_str()) {
            return Err(invalid(format!(
                "unknown merge part key {key:?}; expected one of {PART_KEYS:?}"
            )));
        }
    }
    let get = |k: &str| -> JsResult<Option<JsValue>> {
        let v = Reflect::get(object, &JsValue::from_str(k))?;
        Ok((!v.is_undefined() && !v.is_null()).then_some(v))
    };
    let font = font_arg(
        &get("font")?.ok_or_else(|| invalid("merge part needs `font`"))?,
        "merge part `font`",
    )?;
    let selection =
        selection_arg(&get("selection")?.ok_or_else(|| invalid("merge part needs `selection`"))?)?;
    let dy = get("dy")?
        .map(|v| conv::int_from_js::<i64>(&v, "merge part `dy`"))
        .transpose()?
        .unwrap_or(0);
    let label = get("label")?
        .map(|v| {
            v.as_string()
                .ok_or_else(|| invalid("merge part `label` must be a string"))
        })
        .transpose()?;
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
/// (`MergeOptions`; `family` is required). Returns `{font, report}`.
#[wasm_bindgen(js_namespace = merge, js_name = merge, unchecked_return_type = "MergeLibResult")]
pub fn merge_parts(
    #[wasm_bindgen(unchecked_param_type = "MergePartLike[]")] parts: JsValue,
    #[wasm_bindgen(unchecked_param_type = "MergeOptions")] options: JsValue,
) -> JsResult<JsValue> {
    let o: MergeOptions = req(&options)?;
    let list = parts
        .dyn_ref::<Array>()
        .ok_or_else(|| invalid("merge parts must be an array"))?;
    let (fonts, specs): (Vec<Font>, Vec<Part>) = list
        .iter()
        .map(|p| part_arg(&p))
        .collect::<JsResult<Vec<_>>>()?
        .into_iter()
        .unzip();
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
    let result = merge::merge(&borrowed, &o).js()?;
    drop(borrowed);
    drop(fonts);
    let report = conv::to_js(&result.report)?;
    object(&[
        ("font", JsFont::new(result.font).into()),
        ("report", report),
    ])
}

/// `MergeReport::to_json`: the deterministic JSON text of a report, exactly
/// as the CLI prints it.
#[wasm_bindgen(js_namespace = merge, js_name = reportToJson)]
pub fn report_to_json(
    #[wasm_bindgen(unchecked_param_type = "MergeReport")] report: JsValue,
) -> JsResult<String> {
    Ok(req::<MergeReport>(&report)?.to_json())
}

/// `MergeSpacing::as_str`: the CLI/JSON spelling of a spacing value.
#[wasm_bindgen(js_namespace = merge, js_name = spacingAsStr)]
pub fn spacing_as_str(
    #[wasm_bindgen(unchecked_param_type = "MergeSpacing")] value: JsValue,
) -> JsResult<String> {
    Ok(req::<MergeSpacing>(&value)?.as_str().to_owned())
}

/// `GeometricWidth::as_str`: the CLI/JSON spelling of a geometric width.
#[wasm_bindgen(js_namespace = merge, js_name = geometricWidthAsStr)]
pub fn geometric_width_as_str(
    #[wasm_bindgen(unchecked_param_type = "GeometricWidth")] value: JsValue,
) -> JsResult<String> {
    Ok(req::<GeometricWidth>(&value)?.as_str().to_owned())
}

/// `ModificationPermission::as_str`: the JSON spelling of a permission.
#[wasm_bindgen(js_namespace = merge, js_name = modificationAsStr)]
pub fn modification_as_str(
    #[wasm_bindgen(unchecked_param_type = "ModificationPermission")] value: JsValue,
) -> JsResult<String> {
    Ok(req::<ModificationPermission>(&value)?.as_str().to_owned())
}

/// Default `MergeOptions` (with an empty `family`, which `merge` rejects)
/// and `PartLicense` objects.
#[wasm_bindgen(js_namespace = merge, js_name = defaults, unchecked_return_type = "Record<string, unknown>")]
pub fn merge_defaults() -> JsResult<JsValue> {
    object(&[
        ("MergeOptions", conv::to_js(&MergeOptions::default())?),
        ("PartLicense", conv::to_js(&PartLicense::default())?),
    ])
}
