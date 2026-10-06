//! `variants`: regional/script bitmap alternatives, deterministic
//! deduplication and locl-enabled OpenType output (`fontpixel::variants`).
//!
//! Fonts are `Font` handles (copied into the model; views too) or `FontDict`
//! plain data. `Variants` is a handle owning its faces; the consuming Rust
//! methods `encode_owned` and `into_prepared` consume the handle's model
//! (`encodeOwned()`/`intoPrepared()`; later use throws `InvalidInputError`),
//! so faces are dropped during planning exactly as in Rust. `prepare` (which
//! borrows in Rust) plans a copy and owns the result, so the returned
//! `PreparedVariants` never dangles and is independent of later edits.
//! Exports return `{bytes, report}`; options are `ExportOptions` objects
//! (see `variants.defaults()`).
//!
//! `variants.encodeSingle` keeps the physical glyph identity and order of
//! one face, so its bytes differ from `Variants` planning of the same face
//! by design (decisions.md).
use crate::conv;
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::{JsFont, borrow, borrow_mut};
use crate::sfnt_support::{
    bytes_js, font_arg, handle_of, int, object, offer, opt, req, tag_arg, tag_str, tags_arg,
};
use fontpixel::outline::Shape;
use fontpixel::sfnt::gsub::LayoutLimits;
use fontpixel::variants::{
    self, Encoded, LanguageVariant, MetadataOptions, NotdefPolicy, Options, OutlineFlavor,
    PreparedVariants, ScriptVariant, Variants,
};
use js_sys::{Array, Reflect};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const VARIANTS_TS: &str = r#"
/** A font argument: a `Font` handle (copied) or `FontDict` plain data. */
export type FontLike = Font | FontDict;

/** A language alternative as `Variants.languages` returns it (fonts are `Font` handle copies). */
export interface VariantsLanguage {
  language: string;
  scripts: string[];
  font: Font;
}

/** A script alternative as `Variants.scripts` returns it. */
export interface VariantsScript {
  scripts: string[];
  font: Font;
}

/** Input form of a language alternative (tags as strings or four bytes). */
export interface VariantsLanguageLike {
  language: TagLike;
  scripts: TagLike[];
  font: FontLike;
}

/** Input form of a script alternative. */
export interface VariantsScriptLike {
  scripts: TagLike[];
  font: FontLike;
}

/** `fontpixel::variants::Encoded` with `bytes` as a `Uint8Array`. */
export type VariantsEncoded = BytesFields<ExportEncoded, "bytes">;
"#;

fn encoded(e: &Encoded) -> JsResult<JsValue> {
    object(&[
        ("bytes", bytes_js(&e.bytes)),
        ("report", conv::to_js(&e.report)?),
    ])
}

fn consumed() -> JsValue {
    invalid("this Variants was consumed by encodeOwned() or intoPrepared()")
}

fn tag_list(t: &[[u8; 4]]) -> Array {
    t.iter().map(|&t| JsValue::from_str(&tag_str(t))).collect()
}

fn field(item: &JsValue, key: &str, what: &str) -> JsResult<JsValue> {
    let value = Reflect::get(item, &JsValue::from_str(key))?;
    if value.is_undefined() {
        return Err(invalid(format!("{what}: missing `{key}`")));
    }
    Ok(value)
}

fn check_keys(item: &JsValue, allowed: &[&str], what: &str) -> JsResult<()> {
    let object = item
        .dyn_ref::<js_sys::Object>()
        .ok_or_else(|| invalid(format!("{what}: expected an object")))?;
    for key in js_sys::Object::keys(object).iter() {
        let key = key.as_string().unwrap_or_default();
        if !allowed.contains(&key.as_str()) {
            return Err(invalid(format!(
                "{what}: unknown key `{key}`, expected one of: {}",
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

fn language_item(item: &JsValue) -> JsResult<LanguageVariant> {
    const WHAT: &str = "language alternative";
    check_keys(item, &["language", "scripts", "font"], WHAT)?;
    Ok(LanguageVariant {
        language: tag_arg(&field(item, "language", WHAT)?, "language")?,
        scripts: tags_arg(&field(item, "scripts", WHAT)?, "scripts")?,
        font: font_arg(&field(item, "font", WHAT)?, "font")?,
    })
}

fn script_item(item: &JsValue) -> JsResult<ScriptVariant> {
    const WHAT: &str = "script alternative";
    check_keys(item, &["scripts", "font"], WHAT)?;
    Ok(ScriptVariant {
        scripts: tags_arg(&field(item, "scripts", WHAT)?, "scripts")?,
        font: font_arg(&field(item, "font", WHAT)?, "font")?,
    })
}

fn list(value: &JsValue, what: &str) -> JsResult<Array> {
    value
        .dyn_ref::<Array>()
        .cloned()
        .ok_or_else(|| invalid(format!("{what}: expected an array")))
}

type Model = Rc<RefCell<Option<Variants>>>;

/// One default face plus ordered language and script alternatives
/// (`fontpixel::variants::Variants`).
#[wasm_bindgen(js_name = Variants)]
pub struct JsVariants {
    model: Model,
}

impl JsVariants {
    fn wrap(v: Variants) -> Self {
        Self {
            model: Rc::new(RefCell::new(Some(v))),
        }
    }
    fn read<R>(&self, f: impl FnOnce(&Variants) -> JsResult<R>) -> JsResult<R> {
        read_model(&self.model, f)
    }
    fn edit<R>(&self, f: impl FnOnce(&mut Variants) -> JsResult<R>) -> JsResult<R> {
        let mut guard = borrow_mut(&self.model, "Variants")?;
        f(guard.as_mut().ok_or_else(consumed)?)
    }
    fn take(&self) -> JsResult<Variants> {
        borrow_mut(&self.model, "Variants")?
            .take()
            .ok_or_else(consumed)
    }
}

fn read_model<R>(model: &Model, f: impl FnOnce(&Variants) -> JsResult<R>) -> JsResult<R> {
    let guard = borrow(model, "Variants")?;
    f(guard.as_ref().ok_or_else(consumed)?)
}

#[wasm_bindgen(js_class = Variants)]
impl JsVariants {
    /// `Variants::new`: start with a default face (`Font` handle or
    /// `FontDict`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "FontLike")] base: JsValue,
    ) -> JsResult<JsVariants> {
        Ok(Self::wrap(Variants::new(font_arg(&base, "base")?)))
    }
    /// Build from `VariantsDict` plain data (`{base, languages, scripts}`).
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "VariantsDict")] value: JsValue,
    ) -> JsResult<JsVariants> {
        Ok(Self::wrap(req(&value)?))
    }
    /// The `VariantsDict` plain data (fonts as `FontDict`s).
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "VariantsDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// Whether `encodeOwned()`/`intoPrepared()` consumed this handle.
    #[wasm_bindgen(getter)]
    pub fn consumed(&self) -> JsResult<bool> {
        Ok(borrow(&self.model, "Variants")?.is_none())
    }
    /// A copy of the default face as a `Font` handle.
    #[wasm_bindgen(getter)]
    pub fn base(&self) -> JsResult<JsFont> {
        self.read(|v| Ok(JsFont::new(v.base.clone())))
    }
    /// Replace the default face (`Font` handle or `FontDict`, copied).
    #[wasm_bindgen(setter)]
    pub fn set_base(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontLike")] font: JsValue,
    ) -> JsResult<()> {
        let f = font_arg(&font, "base")?;
        self.edit(|v| {
            v.base = f;
            Ok(())
        })
    }
    /// Language alternatives as `[{language, scripts, font}]` (OpenType tag
    /// strings, `Font` handle copies), in application order.
    #[wasm_bindgen(getter, unchecked_return_type = "VariantsLanguage[]")]
    pub fn languages(&self) -> JsResult<Array> {
        self.read(|v| {
            v.languages
                .iter()
                .map(|l| {
                    object(&[
                        ("language", JsValue::from_str(&tag_str(l.language))),
                        ("scripts", tag_list(&l.scripts).into()),
                        ("font", JsFont::new(l.font.clone()).into()),
                    ])
                })
                .collect()
        })
    }
    /// Replace the language alternatives with `[{language, scripts, font}]`
    /// (tags as four-character strings or four bytes; fonts as handles or
    /// plain data).
    #[wasm_bindgen(setter)]
    pub fn set_languages(
        &self,
        #[wasm_bindgen(unchecked_param_type = "VariantsLanguageLike[]")] value: JsValue,
    ) -> JsResult<()> {
        let items = list(&value, "languages")?
            .iter()
            .map(|item| language_item(&item))
            .collect::<JsResult<Vec<_>>>()?;
        self.edit(|v| {
            v.languages = items;
            Ok(())
        })
    }
    /// Script alternatives as `[{scripts, font}]`.
    #[wasm_bindgen(getter, unchecked_return_type = "VariantsScript[]")]
    pub fn scripts(&self) -> JsResult<Array> {
        self.read(|v| {
            v.scripts
                .iter()
                .map(|s| {
                    object(&[
                        ("scripts", tag_list(&s.scripts).into()),
                        ("font", JsFont::new(s.font.clone()).into()),
                    ])
                })
                .collect()
        })
    }
    /// Replace the script alternatives with `[{scripts, font}]`.
    #[wasm_bindgen(setter)]
    pub fn set_scripts(
        &self,
        #[wasm_bindgen(unchecked_param_type = "VariantsScriptLike[]")] value: JsValue,
    ) -> JsResult<()> {
        let items = list(&value, "scripts")?
            .iter()
            .map(|item| script_item(&item))
            .collect::<JsResult<Vec<_>>>()?;
        self.edit(|v| {
            v.scripts = items;
            Ok(())
        })
    }
    /// `Variants::add_language`: add a BCP-47/OpenType language with the
    /// default CJK script scope (DFLT, hani, kana, hang, bopo).
    #[wasm_bindgen(js_name = addLanguage)]
    pub fn add_language(
        &self,
        language: &str,
        #[wasm_bindgen(unchecked_param_type = "FontLike")] font: JsValue,
    ) -> JsResult<()> {
        let f = font_arg(&font, "font")?;
        self.edit(|v| v.add_language(language, f).js())
    }
    /// Append a language alternative with an explicit OpenType `language`
    /// tag and `scripts` tags (the `languages` field).
    #[wasm_bindgen(js_name = addLanguageVariant)]
    pub fn add_language_variant(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TagLike")] language: JsValue,
        #[wasm_bindgen(unchecked_param_type = "TagLike[]")] scripts: JsValue,
        #[wasm_bindgen(unchecked_param_type = "FontLike")] font: JsValue,
    ) -> JsResult<()> {
        let item = LanguageVariant {
            language: tag_arg(&language, "language")?,
            scripts: tags_arg(&scripts, "scripts")?,
            font: font_arg(&font, "font")?,
        };
        self.edit(|v| {
            v.languages.push(item);
            Ok(())
        })
    }
    /// Append a script alternative for `scripts` tags (the `scripts` field).
    #[wasm_bindgen(js_name = addScript)]
    pub fn add_script(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TagLike[]")] scripts: JsValue,
        #[wasm_bindgen(unchecked_param_type = "FontLike")] font: JsValue,
    ) -> JsResult<()> {
        let item = ScriptVariant {
            scripts: tags_arg(&scripts, "scripts")?,
            font: font_arg(&font, "font")?,
        };
        self.edit(|v| {
            v.scripts.push(item);
            Ok(())
        })
    }
    /// `Variants::plan`: glyph order, deduplication and locl routing as
    /// `Plan` plain data (`{glyphs: [[name, glyph]], cmap, gsub}`).
    #[wasm_bindgen(unchecked_return_type = "Plan")]
    pub fn plan(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "LayoutLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let l: LayoutLimits = opt(limits)?;
        self.read(|v| conv::to_js(&v.plan(&l).js()?))
    }
    /// `Variants::prepare`: validate and plan once for repeated exports.
    /// Plans an owned copy of the model (Rust borrows it instead), so the
    /// returned handle is independent of later edits to this one.
    pub fn prepare(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "LayoutLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsPreparedVariants> {
        let l: LayoutLimits = opt(limits)?;
        let copy = self.read(|v| Ok(v.clone()))?;
        Ok(JsPreparedVariants {
            prepared: copy.into_prepared(&l).js()?,
        })
    }
    /// `Variants::into_prepared`: as `prepare`, consuming this handle's model.
    #[wasm_bindgen(js_name = intoPrepared)]
    pub fn into_prepared(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "LayoutLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsPreparedVariants> {
        let l: LayoutLimits = opt(limits)?;
        let v = self.take()?;
        Ok(JsPreparedVariants {
            prepared: v.into_prepared(&l).js()?,
        })
    }
    /// `Variants::retained_usage`: retained bytes as an owning caller
    /// charges them; throws `ResourceLimitError` above `maxBytes`.
    #[wasm_bindgen(js_name = retainedUsage)]
    pub fn retained_usage(
        &self,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: f64,
    ) -> JsResult<usize> {
        let max: usize = int(max_bytes, "maxBytes")?;
        self.read(|v| v.retained_usage(max).js())
    }
    /// `variants::encode`: plan and export (`{bytes, report}`).
    #[wasm_bindgen(unchecked_return_type = "VariantsEncoded")]
    pub fn encode(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "ExportOptions")] options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let o: Options = opt(options)?;
        self.read(|v| encoded(&variants::encode(v, &o).js()?))
    }
    /// `Variants::encode_owned`: plan and export, consuming this handle's
    /// model and dropping faces as soon as they are planned.
    #[wasm_bindgen(js_name = encodeOwned, unchecked_return_type = "VariantsEncoded")]
    pub fn encode_owned(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "ExportOptions")] options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let o: Options = opt(options)?;
        let v = self.take()?;
        encoded(&v.encode_owned(&o).js()?)
    }
    /// An independent deep copy (a consumed handle cannot be copied).
    pub fn copy(&self) -> JsResult<JsVariants> {
        Ok(Self::wrap(self.read(|v| Ok(v.clone()))?))
    }
    /// Internal: share this handle's storage with the running call.
    #[wasm_bindgen(js_name = __fpShareStorage, skip_typescript)]
    pub fn share_storage(&self) -> JsResult<()> {
        offer(Box::new(self.model.clone()));
        Ok(())
    }
}

/// An immutable plan ready for repeated exports
/// (`fontpixel::variants::PreparedVariants`).
#[wasm_bindgen(js_name = PreparedVariants)]
pub struct JsPreparedVariants {
    prepared: PreparedVariants<'static>,
}

#[wasm_bindgen(js_class = PreparedVariants)]
impl JsPreparedVariants {
    /// `PreparedVariants::plan`: the `Plan` plain data.
    #[wasm_bindgen(unchecked_return_type = "Plan")]
    pub fn plan(&self) -> JsResult<JsValue> {
        conv::to_js(self.prepared.plan())
    }
    /// `PreparedVariants::retained_bytes`: admitted bytes of the shared plan.
    #[wasm_bindgen(js_name = retainedBytes)]
    pub fn retained_bytes(&self) -> usize {
        self.prepared.retained_bytes()
    }
    /// `PreparedVariants::encode_allowance`: bytes one `encode` may admit
    /// besides the plan.
    #[wasm_bindgen(js_name = encodeAllowance)]
    pub fn encode_allowance(&self) -> usize {
        self.prepared.encode_allowance()
    }
    /// `PreparedVariants::encode`: export without replanning;
    /// `options.limits` must equal the limits given to `prepare`.
    #[wasm_bindgen(unchecked_return_type = "VariantsEncoded")]
    pub fn encode(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "ExportOptions")] options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let o: Options = opt(options)?;
        encoded(&self.prepared.encode(&o).js()?)
    }
}

/// `variants::encode`: plan and export a `Variants` handle.
#[wasm_bindgen(js_namespace = variants, js_name = encode, unchecked_return_type = "VariantsEncoded")]
pub fn variants_encode(
    #[wasm_bindgen(unchecked_param_type = "Variants")] variants: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "ExportOptions")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let model = handle_of::<Model>(&variants, "Variants")?
        .ok_or_else(|| invalid("variants: expected a Variants handle"))?;
    let o: Options = opt(options)?;
    read_model(&model, |v| encoded(&variants::encode(v, &o).js()?))
}

/// `variants::encode_single`: export one face without alternatives. Unlike
/// regional planning (`Variants`), this keeps physical glyph identity and
/// order, so its bytes differ from `Variants.encodeOwned` by design.
#[wasm_bindgen(js_namespace = variants, js_name = encodeSingle, unchecked_return_type = "VariantsEncoded")]
pub fn encode_single(
    #[wasm_bindgen(unchecked_param_type = "FontLike")] font: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "ExportOptions")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let f = font_arg(&font, "font")?;
    let o: Options = opt(options)?;
    encoded(&variants::encode_single(&f, &o).js()?)
}

/// `variants::encode_single_owned`: as `encodeSingle`, letting the export
/// own (and drop early) a copy of the face; output is identical.
#[wasm_bindgen(js_namespace = variants, js_name = encodeSingleOwned, unchecked_return_type = "VariantsEncoded")]
pub fn encode_single_owned(
    #[wasm_bindgen(unchecked_param_type = "FontLike")] font: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "ExportOptions")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let f = font_arg(&font, "font")?;
    let o: Options = opt(options)?;
    encoded(&variants::encode_single_owned(f, &o).js()?)
}

/// `variants::script_tag`: validate an explicit four-letter script tag
/// (e.g. `"latn"`); other lengths or characters throw `InvalidInputError`.
#[wasm_bindgen(js_namespace = variants, js_name = scriptTag)]
pub fn script_tag(value: &str) -> JsResult<String> {
    Ok(tag_str(variants::script_tag(value).js()?))
}

/// `variants::language_tag`: the OpenType language-system tag of a BCP-47
/// or OpenType tag via the OpenType language registry (`"zh-HK"` →
/// `"ZHH "`); unknown tags throw `InvalidInputError`.
#[wasm_bindgen(js_namespace = variants, js_name = languageTag)]
pub fn language_tag(value: &str) -> JsResult<String> {
    Ok(tag_str(variants::language_tag(value).js()?))
}

/// `MetadataOptions::validate`: check metadata controls against an outline
/// `flavor` (`"true_type"`/`"cff"`) and `shape`.
#[wasm_bindgen(js_namespace = variants, js_name = validateMetadata)]
pub fn validate_metadata(
    #[wasm_bindgen(unchecked_param_type = "MetadataOptions")] metadata: JsValue,
    #[wasm_bindgen(unchecked_param_type = "OutlineFlavor")] flavor: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Shape")] shape: JsValue,
) -> JsResult<()> {
    let m: MetadataOptions = req(&metadata)?;
    let f: OutlineFlavor = req(&flavor)?;
    let s: Shape = req(&shape)?;
    m.validate(f, s).js()
}

/// `NotdefPolicy::empty_metrics`: `[advance, leftSideBearing]` of an empty
/// `.notdef` policy, or `null` for `"preserve"`.
#[wasm_bindgen(js_namespace = variants, js_name = notdefEmptyMetrics, unchecked_return_type = "[number, number] | null")]
pub fn notdef_empty_metrics(
    #[wasm_bindgen(unchecked_param_type = "NotdefPolicy")] policy: JsValue,
) -> JsResult<JsValue> {
    conv::to_js(&req::<NotdefPolicy>(&policy)?.empty_metrics())
}

/// Default option objects of `variants` (`"Options"`, `"MetadataOptions"`,
/// `"NotdefPolicy"`, `"LayoutLimits"`).
#[wasm_bindgen(js_namespace = variants, js_name = defaults, unchecked_return_type = "Record<string, unknown>")]
pub fn variants_defaults() -> JsResult<JsValue> {
    object(&[
        ("Options", conv::to_js(&Options::default())?),
        ("MetadataOptions", conv::to_js(&MetadataOptions::default())?),
        ("NotdefPolicy", conv::to_js(&NotdefPolicy::default())?),
        ("LayoutLimits", conv::to_js(&LayoutLimits::default())?),
    ])
}
