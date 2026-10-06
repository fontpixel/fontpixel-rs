//! Writer results: `Written` and `OutputBundle` (`fontpixel::resources`).
//!
//! Every registry, direct and custom writer returns a `Written` (the output
//! bundle plus the complete report). A bundle is immutable: its assets come
//! out as fresh `{path, data}` objects (`data` a new `Uint8Array` copy), it
//! can be read back (`read(written)` / `read(bundle)`, the primary file with
//! every other asset as a named sidecar, or a directory bundle's entry point),
//! and JS writer hooks build one to return. Writing never touches a file
//! system: a browser bundle is not a disk transaction; persist it yourself.
use crate::conv;
use crate::errors::{IntoJs, JsResult, invalid};
use crate::formats::{self as fmt, Shared};
use fontpixel::Report;
use fontpixel::resources::{OutputAsset, OutputBundle, ResourcePath, Written};
use js_sys::{Array, Object, Reflect};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// In-memory output files with a primary entry point
/// (`fontpixel::resources::OutputBundle`).
#[wasm_bindgen(js_name = OutputBundle)]
pub struct JsOutputBundle {
    pub(crate) bundle: Rc<OutputBundle>,
}

impl JsOutputBundle {
    /// Wrap a bundle.
    pub fn new(bundle: OutputBundle) -> Self {
        Self {
            bundle: Rc::new(bundle),
        }
    }
}

/// A validated relative resource path.
pub fn path_arg(path: &str) -> JsResult<ResourcePath> {
    ResourcePath::new(path).js()
}

fn asset_arg(value: &JsValue, index: u32) -> JsResult<OutputAsset> {
    let what = format!("assets[{index}]");
    let (path, data) = if Array::is_array(value) {
        let pair: &Array = value.unchecked_ref();
        if pair.length() != 2 {
            return Err(invalid(format!("{what}: expected [path, data]")));
        }
        (pair.get(0), pair.get(1))
    } else if value.is_object() {
        let path = Reflect::get(value, &JsValue::from_str("path"))?;
        let data = Reflect::get(value, &JsValue::from_str("data"))?;
        (path, data)
    } else {
        return Err(invalid(format!(
            "{what}: expected {{path, data}} or [path, data]"
        )));
    };
    let path = path
        .as_string()
        .ok_or_else(|| invalid(format!("{what}: path must be a string")))?;
    Ok(OutputAsset {
        path: path_arg(&path)?,
        bytes: conv::bytes_from_js(&data, &format!("{what}.data"))?,
    })
}

/// `{path, data}` with a fresh `Uint8Array`.
pub fn asset_to_js(asset: &OutputAsset) -> JsResult<JsValue> {
    let out = Object::new();
    Reflect::set(
        &out,
        &JsValue::from_str("path"),
        &JsValue::from_str(asset.path.as_str()),
    )?;
    Reflect::set(
        &out,
        &JsValue::from_str("data"),
        &conv::bytes_to_js(&asset.bytes),
    )?;
    Ok(out.into())
}

fn assets_to_js(bundle: &OutputBundle) -> JsResult<Array> {
    let out = Array::new();
    for asset in bundle.assets() {
        out.push(&asset_to_js(asset)?);
    }
    Ok(out)
}

#[wasm_bindgen(js_class = OutputBundle)]
impl JsOutputBundle {
    /// `new OutputBundle(primary, assets, {directory})`: `assets` holds
    /// `{path, data}` objects or `[path, data]` pairs. A file bundle must
    /// contain `primary` (`OutputBundle::new`); with `directory: true` the
    /// primary path names a directory holding the assets
    /// (`OutputBundle::directory`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        primary: &str,
        #[wasm_bindgen(unchecked_param_type = "Iterable<OutputAssetLike | [string, BytesLike]>")]
        assets: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "{ directory?: boolean }")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsOutputBundle> {
        let options = conv::Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["directory"],
            "OutputBundle options",
        )?;
        let directory = options.bool("directory")?.unwrap_or(false);
        let primary = path_arg(primary)?;
        let list = Array::from(&assets);
        let mut items = Vec::with_capacity(list.length() as usize);
        for (i, item) in list.iter().enumerate() {
            items.push(asset_arg(&item, i as u32)?);
        }
        let bundle = if directory {
            OutputBundle::directory(primary, items)
        } else {
            OutputBundle::new(primary, items)
        };
        Ok(Self::new(bundle.js()?))
    }

    /// A one-file bundle (`OutputBundle::single`).
    pub fn single(
        path: &str,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    ) -> JsResult<JsOutputBundle> {
        Ok(Self::new(OutputBundle::single(
            path_arg(path)?,
            conv::bytes_from_js(&data, "data")?,
        )))
    }

    /// From the plain-data form (`{primary, assets: [{path, bytes}],
    /// directory}`), validated like the constructor.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "OutputBundleDict")] data: JsValue,
    ) -> JsResult<JsOutputBundle> {
        Ok(Self::new(conv::from_js(&data)?))
    }

    /// The plain-data form (`bytes` as number arrays, the serde shape).
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "OutputBundleDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        conv::to_js(&*self.bundle)
    }

    /// Whether the primary path names a directory (`OutputBundle::is_directory`).
    #[wasm_bindgen(getter, js_name = isDirectory)]
    pub fn is_directory(&self) -> bool {
        self.bundle.is_directory()
    }

    /// The primary path (`OutputBundle::primary`).
    #[wasm_bindgen(getter)]
    pub fn primary(&self) -> String {
        self.bundle.primary().as_str().to_owned()
    }

    /// Copies of every asset in order, as `{path, data}` objects
    /// (`OutputBundle::assets`).
    #[wasm_bindgen(getter, unchecked_return_type = "OutputAssetLike[]")]
    pub fn assets(&self) -> JsResult<Array> {
        assets_to_js(&self.bundle)
    }

    /// The assets as a list (`OutputBundle::into_assets`; the bundle stays
    /// usable).
    #[wasm_bindgen(js_name = intoAssets, unchecked_return_type = "OutputAssetLike[]")]
    pub fn into_assets(&self) -> JsResult<Array> {
        assets_to_js(&self.bundle)
    }

    /// The only file's bytes (`OutputBundle::into_single`); throws
    /// `MultipleAssetsError` (with `count`) for several assets and
    /// `InvalidInputError` for a directory bundle.
    #[wasm_bindgen(js_name = intoSingle)]
    pub fn into_single(&self) -> JsResult<js_sys::Uint8Array> {
        let bytes = (*self.bundle).clone().into_single().js()?;
        Ok(conv::bytes_to_js(&bytes))
    }

    /// Retained bytes of the bundle, checked against `maxBytes` (default
    /// unbounded; `OutputBundle::retained_bytes`).
    #[wasm_bindgen(js_name = retainedBytes)]
    pub fn retained_bytes(&self, max_bytes: Option<f64>) -> JsResult<f64> {
        let max = match max_bytes {
            Some(v) => conv::int::<usize>(v, "maxBytes")?,
            None => usize::MAX,
        };
        Ok(self.bundle.retained_bytes(max).js()? as f64)
    }

    /// Number of assets.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.bundle.assets().len()
    }

    /// Whether `other` (an `OutputBundle`) holds the same primary, assets
    /// and directory flag.
    pub fn equals(&self, other: JsValue) -> JsResult<bool> {
        Ok(match fmt::bundle_arg(&other)? {
            Some(other) => *other == *self.bundle,
            None => false,
        })
    }

    /// `OutputBundle(primary=…, assets=…, directory=…)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "OutputBundle(primary={:?}, assets={}, directory={})",
            self.bundle.primary().as_str(),
            self.bundle.assets().len(),
            self.bundle.is_directory()
        )
    }

    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fontpixelShare, skip_typescript)]
    pub fn formats_share(&self) -> JsOutputBundle {
        JsOutputBundle {
            bundle: self.bundle.clone(),
        }
    }
}

/// A writer result: the output bundle and its complete report
/// (`fontpixel::resources::Written`).
#[wasm_bindgen(js_name = Written)]
pub struct JsWritten {
    pub(crate) output: Rc<OutputBundle>,
    pub(crate) report: Shared<Report>,
}

impl JsWritten {
    /// Wrap a Rust result.
    pub fn new(written: Written) -> Self {
        Self {
            output: Rc::new(written.output),
            report: crate::handles::shared(written.report),
        }
    }

    /// The Rust value (a copy).
    pub fn snapshot(&self) -> JsResult<Written> {
        Ok(Written {
            output: (*self.output).clone(),
            report: crate::handles::borrow(&self.report, "Written")?.clone(),
        })
    }
}

#[wasm_bindgen(js_class = Written)]
impl JsWritten {
    /// `new Written(output, report?)`: an `OutputBundle` and an optional
    /// report (plain data, default empty).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "OutputBundle")] output: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "Report")] report: Option<JsValue>,
    ) -> JsResult<JsWritten> {
        let output = fmt::bundle_arg(&output)?
            .ok_or_else(|| invalid("Written: output must be an OutputBundle"))?;
        let report: Report = conv::from_js_or_default(&report.unwrap_or(JsValue::UNDEFINED))?;
        Ok(Self {
            output,
            report: crate::handles::shared(report),
        })
    }

    /// The output files (an `OutputBundle` sharing this result's bundle).
    #[wasm_bindgen(getter)]
    pub fn output(&self) -> JsOutputBundle {
        JsOutputBundle {
            bundle: self.output.clone(),
        }
    }

    /// The complete report (plain data, every entry).
    #[wasm_bindgen(getter, unchecked_return_type = "Report")]
    pub fn report(&self) -> JsResult<JsValue> {
        conv::to_js(&*crate::handles::borrow(&self.report, "Written")?)
    }

    /// The plain-data form (`{output: {...}, report: {...}}`).
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "WrittenDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        conv::to_js(&self.snapshot()?)
    }

    /// From the plain-data form, validated like `OutputBundle.fromDict`.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "WrittenDict")] data: JsValue,
    ) -> JsResult<JsWritten> {
        Ok(Self::new(conv::from_js(&data)?))
    }

    /// `Written(output=OutputBundle(…), entries=…)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        Ok(format!(
            "Written(output={}, entries={})",
            self.output().to_string_js(),
            crate::handles::borrow(&self.report, "Written")?
                .entries
                .len()
        ))
    }

    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fontpixelShare, skip_typescript)]
    pub fn formats_share(&self) -> JsWritten {
        JsWritten {
            output: self.output.clone(),
            report: self.report.clone(),
        }
    }
}
