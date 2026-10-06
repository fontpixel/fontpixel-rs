//! W8.3: the merge page's high-level calls, `info(bytes)`,
//! `merge(parts, options)` and `render(fontBytes, text, options)`.
//!
//! Each is a thin layer over the same `fontpixel_ops` operation the command
//! line runs (`info --json`, `merge`, `preview`) on memory storage, so the
//! results equal the native command line's for the same inputs and options:
//! merge outputs byte for byte, the documents as parsed JSON. Results are
//! plain data and fresh byte arrays (no handles), so they can be posted from
//! a Web Worker unchanged (see `createWorker` in `ops-client.js`).
//!
//! Input names matter as they do on the command line: they are the storage
//! keys, the filename hints of format detection and the asset names in
//! diagnostics. Pass the original file names to reproduce a command line.
use crate::callbacks::ProgressCallback;
use crate::conv::{self, Options};
use crate::errors::{self, JsResult};
use crate::ops::{
    Files, as_request_error, document_to_js, outputs_object, request_from_value, request_value,
};
use fontpixel::Error;
use fontpixel_ops::{InfoRequest, MemoryFiles, MergeRequest, PreviewRequest};
use js_sys::{Array, Object, Reflect, Uint8ClampedArray};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// The keys of `value` (a plain object) as a JSON map, without `skip`.
fn option_map(value: &JsValue, what: &str, skip: &[&str]) -> JsResult<Map<String, Value>> {
    if value.is_undefined() || value.is_null() {
        return Ok(Map::new());
    }
    if !value.is_object() || value.is_function() || Array::is_array(value) {
        return Err(errors::request_error(format!(
            "{what}: expected a plain options object"
        )));
    }
    let copy = Object::assign(&Object::new(), value.unchecked_ref());
    for key in skip {
        Reflect::delete_property(&copy, &JsValue::from_str(key))?;
    }
    match request_value(&copy.into())? {
        Value::Object(map) => Ok(map),
        _ => Err(errors::request_error(format!(
            "{what}: expected a plain options object"
        ))),
    }
}

fn reserved(map: &Map<String, Value>, what: &str, keys: &[&str]) -> JsResult<()> {
    for key in keys {
        if map.contains_key(*key) {
            return Err(errors::request_error(format!(
                "{what}: `{key}` is set by the call and cannot be given as an option"
            )));
        }
    }
    Ok(())
}

fn string_option(value: &JsValue, key: &str, what: &str) -> JsResult<Option<String>> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    let item = Reflect::get(value, &JsValue::from_str(key))?;
    if item.is_undefined() || item.is_null() {
        return Ok(None);
    }
    item.as_string()
        .map(Some)
        .ok_or_else(|| errors::request_error(format!("{what}: `{key}` must be a string")))
}

fn set(object: &Object, key: &str, value: &JsValue) -> JsResult<()> {
    Reflect::set(object, &JsValue::from_str(key), value).map(|_| ())
}

// ---------------------------------------------------------------- info

/// Describe a font (`fontpixel info --json`): the parsed JSON document
/// `{format, font_count, fonts, report}`, equal to the command line's for
/// the same bytes and file name.
///
/// `options`: `name` (the file name: storage key, detection hint and
/// diagnostic asset name; default `"font"`) plus any `InfoRequest` key
/// except `input`/`json` (`from`, `max_input_bytes`, codec options, …).
/// For the text output, the `FontSet` handle or per-font JSON use
/// `ops.info`.
#[wasm_bindgen(js_name = info, unchecked_return_type = "InfoDocument")]
pub fn page_info(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] bytes: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "InfoArgs")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let options = options.unwrap_or(JsValue::UNDEFINED);
    let data = conv::bytes_from_js(&bytes, "bytes")?;
    let name = string_option(&options, "name", "info options")?.unwrap_or_else(|| "font".into());
    let mut map = option_map(&options, "info options", &["name"])?;
    reserved(&map, "info options", &["input", "json"])?;
    map.insert("input".into(), Value::String(name.clone()));
    map.insert("json".into(), Value::Bool(true));
    let request: InfoRequest = request_from_value(Value::Object(map))?;
    let mut memory = MemoryFiles::new();
    memory.insert(&name, data);
    let files = Files::memory(memory);
    let response = fontpixel_ops::info(request, &files.storage).map_err(errors::to_js)?;
    let mut document = Vec::new();
    response
        .write_json(&mut document, false)
        .map_err(errors::to_js)?;
    document_to_js(&String::from_utf8_lossy(&document))
}

// ---------------------------------------------------------------- merge

/// Merge selected glyphs from ordered parts (`fontpixel merge`) and return
/// `{files, report, json, text}`:
///
/// - `files`: every output asset as `{name: Uint8Array}` (keys are the
///   requested destinations, plus sidecars where a format has them);
/// - `report`: the parsed `--report json` document (family, glyphs, the
///   merge report, per-output summaries and diagnostics);
/// - `json`/`text`: that document and the text report exactly as the
///   command line prints them.
///
/// `parts`: `[{bytes, take, dy, name, from, font_index}]` in priority order
/// (`take` is the selection expression, e.g. `"cjk"` or
/// `"western,geometric,-U+2190-21FF"`; `name` defaults to `part-<n>` and
/// acts as the file name on the command line). `options`: the command
/// line's merge options as plain data (`family` (required), `outputs` (file
/// names or `{destination, format}`), `spacing`, `geometric_width`,
/// `allow_misaligned`, `fallback`, `ascent`, `descent`, `strict`, `report`,
/// `shape`, resource limits and writer options), plus `progress`: a
/// function receiving `Progress` after each part is read, after the merge
/// and after each output is encoded; returning `false` cancels
/// (`CancelledError`), a throw cancels and is rethrown.
#[wasm_bindgen(js_name = merge, unchecked_return_type = "MergeFilesResult")]
pub fn page_merge(
    #[wasm_bindgen(unchecked_param_type = "MergePart[]")] parts: JsValue,
    #[wasm_bindgen(unchecked_param_type = "MergeArgs")] options: JsValue,
) -> JsResult<JsValue> {
    let list = parts
        .dyn_ref::<Array>()
        .ok_or_else(|| errors::request_error("merge parts: expected an array"))?;
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut part_specs = Vec::new();
    for (index, part) in list.iter().enumerate() {
        let what = format!("merge part {}", index + 1);
        let fields = Options::new(
            &part,
            &["bytes", "take", "dy", "name", "from", "font_index"],
            "merge part",
        )
        .map_err(as_request_error)?;
        let data = conv::bytes_from_js(
            &fields.get("bytes")?.unwrap_or(JsValue::UNDEFINED),
            &format!("{what}: bytes"),
        )?;
        let name = fields
            .string("name")
            .map_err(as_request_error)?
            .unwrap_or_else(|| format!("part-{}", index + 1));
        let key = fontpixel_ops::memory::key(std::path::Path::new(&name));
        match files.get(&key) {
            Some(existing) if *existing != data => {
                return Err(errors::request_error(format!(
                    "{what}: name {name:?} is already used by another part with different bytes"
                )));
            }
            Some(_) => {}
            None => {
                files.insert(key, data);
            }
        }
        let mut spec = Map::new();
        spec.insert("input".into(), Value::String(name));
        for key in ["take", "dy", "from", "font_index"] {
            if let Some(value) = fields.get(key)? {
                spec.insert(key.into(), request_value(&value)?);
            }
        }
        part_specs.push(Value::Object(spec));
    }
    let progress = if options.is_object() {
        ProgressCallback::new(&Reflect::get(&options, &JsValue::from_str("progress"))?)?
    } else {
        None
    };
    let mut map = option_map(&options, "merge options", &["progress"])?;
    reserved(&map, "merge options", &["parts"])?;
    if let Some(Value::Array(outputs)) = map.get_mut("outputs") {
        for output in outputs.iter_mut() {
            if let Value::String(destination) = output {
                let mut spec = Map::new();
                spec.insert("destination".into(), Value::String(destination.clone()));
                *output = Value::Object(spec);
            }
        }
    }
    map.insert("parts".into(), Value::Array(part_specs));
    let request: MergeRequest = request_from_value(Value::Object(map))?;
    let mut memory = MemoryFiles::new();
    for (name, data) in files {
        memory.insert(name, data);
    }
    let files = Files::memory(memory);
    let result = match &progress {
        None => fontpixel_ops::merge(request, &files.storage),
        Some(callback) => {
            fontpixel_ops::merge_with_progress(request, &files.storage, &mut callback.observer())
        }
    };
    let result = result.map_err(errors::to_js);
    let completed = match &progress {
        Some(callback) => callback.finish(result)?,
        None => result?,
    };
    let response = &completed.response;
    let mut pretty = Vec::new();
    response
        .write_json(&mut pretty, true)
        .map_err(errors::to_js)?;
    let mut compact = Vec::new();
    response
        .write_json(&mut compact, false)
        .map_err(errors::to_js)?;
    let mut text = Vec::new();
    response.write_text(&mut text).map_err(errors::to_js)?;
    let out = Object::new();
    set(&out, "files", &outputs_object(&completed.outputs)?.into())?;
    set(
        &out,
        "report",
        &document_to_js(&String::from_utf8_lossy(&compact))?,
    )?;
    set(
        &out,
        "json",
        &JsValue::from_str(&String::from_utf8_lossy(&pretty)),
    )?;
    set(
        &out,
        "text",
        &JsValue::from_str(&String::from_utf8_lossy(&text)),
    )?;
    Ok(out.into())
}

// ---------------------------------------------------------------- render

/// Ink pixels of preview art (`#` ink, `.` paper), row-major.
struct Art {
    width: usize,
    height: usize,
    ink: Vec<bool>,
}

fn parse_art(text: &str) -> JsResult<Art> {
    if text.is_empty() {
        return Ok(Art {
            width: 0,
            height: 0,
            ink: Vec::new(),
        });
    }
    let rows: Vec<&str> = text.split('\n').collect();
    let width = rows[0].len();
    let mut ink = Vec::with_capacity(width * rows.len());
    for row in &rows {
        if row.len() != width {
            return Err(errors::invalid("preview art rows differ in width"));
        }
        for byte in row.bytes() {
            match byte {
                b'#' => ink.push(true),
                b'.' => ink.push(false),
                _ => return Err(errors::invalid("unexpected preview art character")),
            }
        }
    }
    Ok(Art {
        width,
        height: rows.len(),
        ink,
    })
}

fn pixel_limit(limit: usize) -> JsValue {
    errors::to_js(Error::ResourceLimit {
        resource: "render image pixels",
        limit,
    })
}

/// Lay out `text` with a font (the native renderer, exactly as `fontpixel
/// preview`) and return the pixels for a page preview:
/// `{format, width, height, stride, scale, data, art, report}`.
///
/// - `format: "mono"` (default): `data` is a `Uint8Array` of packed 1-bit
///   rows (most significant bit first, each row padded to whole bytes,
///   `stride` bytes per row), `width`×`height` source pixels.
/// - `format: "rgba"`: `data` is a `Uint8ClampedArray` of RGBA pixels
///   (`stride` = 4 × width), each source pixel scaled to `scale`×`scale`
///   (default 1) in `color` (default opaque black) on `background` (default
///   opaque white), both `{red, green, blue, alpha}`; ready for `ImageData`.
///   The scaled image is bounded by `max_pixels`.
///
/// `art` is the `#`/`.` character art (`fontpixel preview` stdout without
/// its final newline) and `report` the read and render diagnostics.
/// `options`: `name` (file name, default `"font"`) plus every `preview`
/// layout key (`from`, `font_index`, `direction`, `line_limit`, `mode`,
/// `letter_spacing`, `line_spacing`, `missing`, `fail_missing`,
/// `gray_threshold`, `strict`, `all`, `range`, `order`, limits). With
/// `all: true`, pass `null` or `""` as `text`.
#[wasm_bindgen(js_name = render, unchecked_return_type = "RenderResult")]
pub fn page_render(
    #[wasm_bindgen(js_name = fontBytes, unchecked_param_type = "BytesLike")] font_bytes: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string | null | undefined")] text: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "RenderArgs")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let options = options.unwrap_or(JsValue::UNDEFINED);
    let data = conv::bytes_from_js(&font_bytes, "fontBytes")?;
    let name = string_option(&options, "name", "render options")?.unwrap_or_else(|| "font".into());
    let format =
        string_option(&options, "format", "render options")?.unwrap_or_else(|| "mono".into());
    if format != "mono" && format != "rgba" {
        return Err(errors::request_error(format!(
            "render options: `format` must be \"mono\" or \"rgba\", not {format:?}"
        )));
    }
    let mut map = option_map(
        &options,
        "render options",
        &["name", "format", "scale", "color", "background"],
    )?;
    reserved(&map, "render options", &["input", "text", "chars"])?;
    let scale: u32 = match option_value(&options, "scale")? {
        Some(value) => conv::int_from_js::<u32>(&value, "render options: scale")
            .map_err(as_request_error)
            .and_then(|s| {
                if s == 0 {
                    Err(errors::request_error(
                        "render options: `scale` must be at least 1",
                    ))
                } else {
                    Ok(s)
                }
            })?,
        None => 1,
    };
    let color: fontpixel::render::Rgba = match option_value(&options, "color")? {
        Some(value) => conv::from_js(&value).map_err(as_request_error)?,
        None => fontpixel::render::Rgba::BLACK,
    };
    let background: fontpixel::render::Rgba = match option_value(&options, "background")? {
        Some(value) => conv::from_js(&value).map_err(as_request_error)?,
        None => fontpixel::render::Rgba::WHITE,
    };
    if format == "mono"
        && (scale != 1
            || option_value(&options, "color")?.is_some()
            || option_value(&options, "background")?.is_some())
    {
        return Err(errors::request_error(
            "render options: `scale`, `color` and `background` apply to format \"rgba\"",
        ));
    }
    let all = matches!(map.get("all"), Some(Value::Bool(true)));
    match text.as_string() {
        Some(text) if !(all && text.is_empty()) => {
            map.insert("text".into(), Value::String(text));
        }
        Some(_) => {}
        None if text.is_undefined() || text.is_null() => {}
        None => return Err(errors::request_error("render: `text` must be a string")),
    }
    map.insert("input".into(), Value::String(name.clone()));
    map.insert("chars".into(), Value::String("#.".into()));
    let request: PreviewRequest = request_from_value(Value::Object(map))?;
    let max_pixels = request.layout.max_pixels;
    let mut memory = MemoryFiles::new();
    memory.insert(&name, data);
    let files = Files::memory(memory);
    let response = fontpixel_ops::preview(request, &files.storage).map_err(errors::to_js)?;
    let art = parse_art(&response.text)?;
    let out = Object::new();
    set(&out, "format", &JsValue::from_str(&format))?;
    set(&out, "width", &JsValue::from_f64(art.width as f64))?;
    set(&out, "height", &JsValue::from_f64(art.height as f64))?;
    if format == "mono" {
        let stride = art.width.div_ceil(8);
        let mut bits = vec![0u8; stride * art.height];
        for y in 0..art.height {
            for x in 0..art.width {
                if art.ink[y * art.width + x] {
                    bits[y * stride + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        set(&out, "stride", &JsValue::from_f64(stride as f64))?;
        set(&out, "scale", &JsValue::from_f64(1.0))?;
        set(&out, "data", &conv::bytes_to_js(&bits).into())?;
    } else {
        let scale = scale as usize;
        let width = art
            .width
            .checked_mul(scale)
            .ok_or_else(|| pixel_limit(max_pixels))?;
        let height = art
            .height
            .checked_mul(scale)
            .ok_or_else(|| pixel_limit(max_pixels))?;
        let pixels = width
            .checked_mul(height)
            .ok_or_else(|| pixel_limit(max_pixels))?;
        if pixels > max_pixels {
            return Err(pixel_limit(max_pixels));
        }
        let bytes = pixels
            .checked_mul(4)
            .ok_or_else(|| pixel_limit(max_pixels))?;
        let ink = [color.red, color.green, color.blue, color.alpha];
        let paper = [
            background.red,
            background.green,
            background.blue,
            background.alpha,
        ];
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(bytes)
            .map_err(|_| pixel_limit(max_pixels))?;
        for y in 0..height {
            let row = &art.ink[(y / scale) * art.width..(y / scale + 1) * art.width];
            for x in 0..width {
                rgba.extend_from_slice(if row[x / scale] { &ink } else { &paper });
            }
        }
        let array = Uint8ClampedArray::new_with_length(rgba.len() as u32);
        array.copy_from(&rgba);
        set(&out, "stride", &JsValue::from_f64((width * 4) as f64))?;
        set(&out, "scale", &JsValue::from_f64(scale as f64))?;
        set(&out, "width", &JsValue::from_f64(width as f64))?;
        set(&out, "height", &JsValue::from_f64(height as f64))?;
        set(&out, "data", &array.into())?;
    }
    set(&out, "art", &JsValue::from_str(&response.text))?;
    set(&out, "report", &conv::to_js(&response.report)?)?;
    Ok(out.into())
}

fn option_value(options: &JsValue, key: &str) -> JsResult<Option<JsValue>> {
    if options.is_undefined() || options.is_null() {
        return Ok(None);
    }
    let value = Reflect::get(options, &JsValue::from_str(key))?;
    Ok((!value.is_undefined() && !value.is_null()).then_some(value))
}
