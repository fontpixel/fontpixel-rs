//! Name overrides (`fontpixel::metadata`), bitmap projection
//! (`fontpixel::projection`) and report helpers (`fontpixel::report`).
//!
//! Reports are plain data `{entries: [{level, impact, code, location:
//! {font_index, glyph_index, codepoint, asset}, message}]}` with every entry;
//! nothing is truncated. The report helpers take and return that plain data.
use crate::conv::{self, Options};
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::JsFontSet;
use crate::model_args::{Handle, array_arg, handle_arg, stash, wrong_handle};
use fontpixel::ResourceLimits;
use fontpixel::metadata::{NameOverrides, apply_names as core_apply_names};
use fontpixel::projection::{
    BitmapProjection, EditValue, MAX_FONT_NAME_BYTES, MAX_PROPERTY_EDITS, MAX_PROPERTY_NAME_BYTES,
    MAX_PROPERTY_TEXT_BYTES, PropertyEdit, ScalableAdvance, SizeProjection,
    apply_bitmap_projection as core_apply_projection,
};
use fontpixel::report::{Report, ReportEntry};
use js_sys::Reflect;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Apply family/style/name/notice overrides to every strike of `fontSet` as
/// one transaction (`metadata::apply_names`); on error the set is unchanged.
/// `overrides` is a `NameOverrides` object (`family`, `style`, `full_name`,
/// `postscript_name`, `version`, `copyright`, `license`, `license_url`,
/// `foundry`); an empty object changes nothing. Returns the report.
#[wasm_bindgen(js_name = applyNames, unchecked_return_type = "Report")]
pub fn apply_names(
    #[wasm_bindgen(js_name = fontSet)] font_set: &JsFontSet,
    #[wasm_bindgen(unchecked_optional_param_type = "NameOverrides")] overrides: Option<JsValue>,
    #[wasm_bindgen(unchecked_optional_param_type = "ResourceLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let overrides: NameOverrides =
        conv::from_js_or_default(&overrides.unwrap_or(JsValue::UNDEFINED))?;
    let limits: ResourceLimits = conv::from_js_or_default(&limits.unwrap_or(JsValue::UNDEFINED))?;
    let report = font_set.write(|set| core_apply_names(set, &overrides, &limits).js())?;
    conv::to_js(&report)
}

/// Whether a `NameOverrides` object requests no change
/// (`NameOverrides::is_empty`).
#[wasm_bindgen(js_name = nameOverridesIsEmpty)]
pub fn name_overrides_is_empty(
    #[wasm_bindgen(unchecked_param_type = "NameOverrides")] overrides: JsValue,
) -> JsResult<bool> {
    let overrides: NameOverrides = conv::from_js(&overrides)?;
    Ok(overrides.is_empty())
}

// ---------------------------------------------------------------- projection

/// An owned property edit (`PropertyEdit` borrows its strings). Plain data:
/// `{set: {name, value}}` (`value` an integer (number or bigint) or a
/// string) or `{remove: {name}}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnedEdit {
    /// `PropertyEdit::Set`.
    Set {
        /// Property name.
        name: String,
        /// New value.
        value: OwnedValue,
    },
    /// `PropertyEdit::Remove`.
    Remove {
        /// Property name.
        name: String,
    },
}

/// An owned `EditValue`: an integer or a string.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OwnedValue {
    /// `EditValue::Integer`.
    Integer(i64),
    /// `EditValue::Text`.
    Text(String),
}

impl OwnedEdit {
    fn name(&self) -> &str {
        match self {
            Self::Set { name, .. } | Self::Remove { name } => name,
        }
    }
    fn borrow(&self) -> PropertyEdit<'_> {
        match self {
            Self::Set { name, value } => PropertyEdit::Set {
                name,
                value: match value {
                    OwnedValue::Integer(v) => EditValue::Integer(*v),
                    OwnedValue::Text(t) => EditValue::Text(t),
                },
            },
            Self::Remove { name } => PropertyEdit::Remove { name },
        }
    }
}

/// Owned `BitmapProjection` options.
#[derive(Clone, Debug, Default)]
pub struct ProjectionSpec {
    scalable_advance: ScalableAdvance,
    font_name: Option<String>,
    size: Option<SizeProjection>,
    properties: Vec<OwnedEdit>,
}

const PROJECTION_KEYS: &[&str] = &["scalable_advance", "font_name", "size", "properties"];

impl ProjectionSpec {
    /// From the options object `{scalable_advance, font_name, size,
    /// properties}` (the `BitmapProjection` field names).
    pub fn from_options(value: &JsValue) -> JsResult<Self> {
        let options = Options::new(value, PROJECTION_KEYS, "BitmapProjection options")?;
        let properties = match options.get("properties")? {
            Some(list) => array_arg(&list, "properties")?
                .iter()
                .map(|edit| {
                    conv::from_js::<OwnedEdit>(&edit).map_err(|_| {
                        invalid(
                            "a property edit is {set: {name: string, value: integer | string}} or {remove: {name: string}}",
                        )
                    })
                })
                .collect::<JsResult<Vec<_>>>()?,
            None => Vec::new(),
        };
        Ok(Self {
            scalable_advance: options.plain("scalable_advance")?.unwrap_or_default(),
            font_name: options.string("font_name")?,
            size: options.plain("size")?,
            properties,
        })
    }

    /// Run `f` with the borrowed `BitmapProjection` these options describe.
    pub fn with<R>(&self, f: impl FnOnce(&BitmapProjection<'_>) -> R) -> R {
        let edits: Vec<PropertyEdit<'_>> = self.properties.iter().map(OwnedEdit::borrow).collect();
        let mut projection = BitmapProjection::default();
        projection.scalable_advance = self.scalable_advance;
        projection.font_name = self.font_name.as_deref();
        projection.size = self.size;
        projection.properties = &edits;
        f(&projection)
    }

    fn apply(&self, font_set: &JsFontSet, max_working_bytes: Option<f64>) -> JsResult<JsValue> {
        let limit = match max_working_bytes {
            Some(v) => conv::int(v, "maxWorkingBytes")?,
            None => usize::MAX,
        };
        let outcome = font_set.write(|set| {
            self.with(|projection| core_apply_projection(set, projection, limit))
                .js()
        })?;
        let value = conv::to_js(&outcome)?;
        Reflect::set(
            &value,
            &JsValue::from_str("retained_bytes"),
            &JsValue::from_f64(outcome.retained_bytes() as f64),
        )?;
        Ok(value)
    }
}

/// A `BitmapProjection` handle or its options object.
pub fn projection_arg(value: &JsValue) -> JsResult<ProjectionSpec> {
    match handle_arg(value)? {
        Some(Handle::Projection(spec)) => Ok(spec),
        Some(other) => Err(wrong_handle("projection", "BitmapProjection", &other)),
        None => ProjectionSpec::from_options(value),
    }
}

/// Typed bitmap-projection options (`fontpixel::projection::BitmapProjection`
/// with owned property edits). The default changes nothing.
///
/// - `scalable_advance`: `"preserve"` or
///   `{from_device: {units_per_em, reference_ppem}}`
/// - `font_name`: complete `FONT`/XLFD name
/// - `size`: `{point_size: {numerator, denominator}, resolution: {x, y}}`
/// - `properties`: edits `{set: {name, value: integer | string}}` or
///   `{remove: {name}}`, applied in order
///
/// The limits are the static getters `MAX_PROPERTY_EDITS`,
/// `MAX_PROPERTY_NAME_BYTES`, `MAX_PROPERTY_TEXT_BYTES` and
/// `MAX_FONT_NAME_BYTES` (`fontpixel::projection::MAX_*`).
#[wasm_bindgen(js_name = BitmapProjection)]
pub struct JsBitmapProjection {
    spec: ProjectionSpec,
}

#[wasm_bindgen(js_class = BitmapProjection)]
impl JsBitmapProjection {
    /// `new BitmapProjection({scalable_advance, font_name, size, properties})`
    /// (every key optional).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_optional_param_type = "BitmapProjectionArgs")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsBitmapProjection> {
        Ok(Self {
            spec: ProjectionSpec::from_options(&options.unwrap_or(JsValue::UNDEFINED))?,
        })
    }
    /// Options deriving scalable advances from device advances
    /// (`BitmapProjection::scalable_from_device`).
    #[wasm_bindgen(js_name = scalableFromDevice)]
    pub fn scalable_from_device(
        units_per_em: f64,
        reference_ppem: f64,
    ) -> JsResult<JsBitmapProjection> {
        let p = BitmapProjection::scalable_from_device(
            conv::int(units_per_em, "unitsPerEm")?,
            conv::int(reference_ppem, "referencePpem")?,
        );
        Ok(Self {
            spec: ProjectionSpec {
                scalable_advance: p.scalable_advance,
                ..ProjectionSpec::default()
            },
        })
    }
    /// Whether these options change nothing
    /// (`BitmapProjection::is_identity`).
    #[wasm_bindgen(js_name = isIdentity)]
    pub fn is_identity(&self) -> bool {
        self.spec.with(|p| p.is_identity())
    }
    /// The scalable-advance policy (plain data).
    #[wasm_bindgen(getter, js_name = scalableAdvance, unchecked_return_type = "ScalableAdvance")]
    pub fn scalable_advance(&self) -> JsResult<JsValue> {
        conv::to_js(&self.spec.scalable_advance)
    }
    /// The projected font name.
    #[wasm_bindgen(getter, js_name = fontName)]
    pub fn font_name(&self) -> Option<String> {
        self.spec.font_name.clone()
    }
    /// The projected size (plain data), or `undefined`.
    #[wasm_bindgen(getter, unchecked_return_type = "SizeProjection | undefined")]
    pub fn size(&self) -> JsResult<JsValue> {
        self.spec
            .size
            .as_ref()
            .map_or(Ok(JsValue::UNDEFINED), conv::to_js)
    }
    /// The property edits (plain data).
    #[wasm_bindgen(getter, unchecked_return_type = "PropertyEditLike[]")]
    pub fn properties(&self) -> JsResult<JsValue> {
        conv::to_js(&self.spec.properties)
    }
    /// The edited property names in order (`PropertyEdit::name`).
    #[wasm_bindgen(js_name = propertyNames)]
    pub fn property_names(&self) -> Vec<String> {
        self.spec
            .properties
            .iter()
            .map(|e| e.name().to_owned())
            .collect()
    }
    /// Apply to every strike of `fontSet` (`apply_bitmap_projection`); see
    /// the function `applyBitmapProjection`.
    #[wasm_bindgen(unchecked_return_type = "ProjectionResult")]
    pub fn apply(
        &self,
        #[wasm_bindgen(js_name = fontSet)] font_set: &JsFontSet,
        #[wasm_bindgen(js_name = maxWorkingBytes)] max_working_bytes: Option<f64>,
    ) -> JsResult<JsValue> {
        self.spec.apply(font_set, max_working_bytes)
    }
    /// `MAX_PROPERTY_EDITS` (64).
    #[wasm_bindgen(getter = MAX_PROPERTY_EDITS)]
    pub fn max_property_edits() -> usize {
        MAX_PROPERTY_EDITS
    }
    /// `MAX_PROPERTY_NAME_BYTES` (255).
    #[wasm_bindgen(getter = MAX_PROPERTY_NAME_BYTES)]
    pub fn max_property_name_bytes() -> usize {
        MAX_PROPERTY_NAME_BYTES
    }
    /// `MAX_PROPERTY_TEXT_BYTES` (65,535).
    #[wasm_bindgen(getter = MAX_PROPERTY_TEXT_BYTES)]
    pub fn max_property_text_bytes() -> usize {
        MAX_PROPERTY_TEXT_BYTES
    }
    /// `MAX_FONT_NAME_BYTES` (4096).
    #[wasm_bindgen(getter = MAX_FONT_NAME_BYTES)]
    pub fn max_font_name_bytes() -> usize {
        MAX_FONT_NAME_BYTES
    }
    /// `BitmapProjection(scalableAdvance=…, fontName=…, size=…, properties=[…])`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "BitmapProjection(scalableAdvance={:?}, fontName={:?}, size={:?}, properties={:?})",
            self.spec.scalable_advance,
            self.spec.font_name,
            self.spec.size,
            self.property_names()
        )
    }
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Projection(self.spec.clone()));
    }
}

/// Apply `projection` (a `BitmapProjection` or its options object) to every
/// strike of `fontSet` as one transaction; on error the set is unchanged
/// (`projection::apply_bitmap_projection`). Returns the `ProjectionOutcome`
/// (`report`, `scalable_advance_changes`, `metadata_changes`) plus
/// `retained_bytes` (`ProjectionOutcome::retained_bytes`).
/// `maxWorkingBytes` defaults to unbounded.
#[wasm_bindgen(js_name = applyBitmapProjection, unchecked_return_type = "ProjectionResult")]
pub fn apply_bitmap_projection(
    #[wasm_bindgen(js_name = fontSet)] font_set: &JsFontSet,
    #[wasm_bindgen(unchecked_param_type = "BitmapProjection | BitmapProjectionArgs")]
    projection: JsValue,
    #[wasm_bindgen(js_name = maxWorkingBytes)] max_working_bytes: Option<f64>,
) -> JsResult<JsValue> {
    projection_arg(&projection)?.apply(font_set, max_working_bytes)
}

// ---------------------------------------------------------------- reports

/// Whether a report has any loss entry (`Report::has_loss`).
#[wasm_bindgen(js_name = reportHasLoss)]
pub fn report_has_loss(
    #[wasm_bindgen(unchecked_param_type = "Report")] report: JsValue,
) -> JsResult<bool> {
    let report: Report = conv::from_js(&report)?;
    Ok(report.has_loss())
}

/// Whether a report has any error entry (`Report::has_errors`).
#[wasm_bindgen(js_name = reportHasErrors)]
pub fn report_has_errors(
    #[wasm_bindgen(unchecked_param_type = "Report")] report: JsValue,
) -> JsResult<bool> {
    let report: Report = conv::from_js(&report)?;
    Ok(report.has_errors())
}

/// A new report with `entry` appended, after validating both
/// (`Report::push`); the argument is not modified.
#[wasm_bindgen(js_name = reportPush, unchecked_return_type = "Report")]
pub fn report_push(
    #[wasm_bindgen(unchecked_param_type = "Report")] report: JsValue,
    #[wasm_bindgen(unchecked_param_type = "ReportEntry")] entry: JsValue,
) -> JsResult<JsValue> {
    let mut report: Report = conv::from_js(&report)?;
    let entry: ReportEntry = conv::from_js(&entry)?;
    report.push(entry);
    conv::to_js(&report)
}

/// A new report with every entry of `other` appended (`Report::extend`).
#[wasm_bindgen(js_name = reportExtend, unchecked_return_type = "Report")]
pub fn report_extend(
    #[wasm_bindgen(unchecked_param_type = "Report")] report: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Report")] other: JsValue,
) -> JsResult<JsValue> {
    let mut report: Report = conv::from_js(&report)?;
    let other: Report = conv::from_js(&other)?;
    report.extend(other);
    conv::to_js(&report)
}

/// The report in the command line's text form, one line per entry
/// (`Report: Display`).
#[wasm_bindgen(js_name = reportToText)]
pub fn report_to_text(
    #[wasm_bindgen(unchecked_param_type = "Report")] report: JsValue,
) -> JsResult<String> {
    let report: Report = conv::from_js(&report)?;
    Ok(report.to_string())
}
