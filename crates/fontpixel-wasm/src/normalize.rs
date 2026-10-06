//! `normalize(typeName, value)`: validate plain data as a named Rust serde
//! type and return its serialized form (the JS counterpart of the Python
//! `fontpixel.formats.normalize`, extended to every serde type).
//!
//! Type names are Rust paths without the `fontpixel::` prefix
//! (`"resources::ResourceLimits"`, `"formats::fzx::FzxWriteOptions"`);
//! `fontpixel_ops` types use the `ops::` prefix (`"ops::RenderRequest"`).
//! Foundation pilot: a few types covering every plain-data convention
//! (integer-keyed maps, values beyond 2^53, flattened strict requests); the
//! formats area extends the table with the generated rows.
use crate::conv;
use crate::errors::{JsResult, invalid};
use wasm_bindgen::prelude::*;

macro_rules! normalize_table {
    ($($(#[$meta:meta])* $name:literal => $ty:ty,)*) => {
        /// Every type name `normalize` accepts.
        pub const TYPES: &[&str] = &[$($(#[$meta])* $name,)*];

        fn dispatch(name: &str, value: &JsValue) -> JsResult<JsValue> {
            match name {
                $($(#[$meta])* $name => {
                    let parsed: $ty = conv::from_js(value)?;
                    conv::to_js(&parsed)
                })*
                _ => Err(invalid(format!("unknown plain-data type `{name}`; see optionTypes()"))),
            }
        }
    };
}

normalize_table! {
    "bitmap::ByteLayout" => fontpixel::bitmap::ByteLayout,
    "model::FontMetrics" => fontpixel::model::FontMetrics,
    "model::PropertyValue" => fontpixel::model::PropertyValue,
    "resources::ResourceLimits" => fontpixel::resources::ResourceLimits,
    #[cfg(any(feature = "all-formats", feature = "merge"))]
    "sfnt::gsub::Gsub" => fontpixel::sfnt::gsub::Gsub,
    "ops::RenderRequest" => fontpixel_ops::RenderRequest,
    "ops::TaskRequest" => fontpixel_ops::TaskRequest,
}

/// Validate `value` as the Rust type `typeName` and return its serialized
/// form; `undefined`/`null` normalizes `{}`, giving the documented defaults
/// of option structs. Unknown keys, wrong types, unknown enum names and
/// out-of-range integers throw `InvalidInputError`.
#[wasm_bindgen(unchecked_return_type = "unknown")]
pub fn normalize(
    #[wasm_bindgen(js_name = typeName)] type_name: &str,
    #[wasm_bindgen(unchecked_param_type = "unknown")] value: JsValue,
) -> JsResult<JsValue> {
    let value = if value.is_undefined() || value.is_null() {
        js_sys::Object::new().into()
    } else {
        value
    };
    dispatch(type_name, &value)
}

/// Every type name `normalize` accepts.
#[wasm_bindgen(js_name = optionTypes)]
pub fn option_types() -> Vec<String> {
    TYPES.iter().map(|s| (*s).to_owned()).collect()
}
