//! WASM/JS bindings for fontpixel (npm package `fontpixel`, private).
//!
//! The package exposes the complete native fontpixel API (plus, in the
//! `full` build, a bdfparser-js compatible layer). Conventions shared by
//! every area module (full text: `docs/bindings-design.md`, "JS/WASM"):
//!
//! - Naming: classes PascalCase (`#[wasm_bindgen(js_name = Font)]` on a
//!   `Js*` struct), methods and functions camelCase (`js_name`), plain-data
//!   keys snake_case (the Rust field names).
//! - Plain data crosses through serde ([`conv`]): numbers per value (`number`
//!   up to 2^53 − 1, else `bigint`), unknown keys rejected, bytes as
//!   `Uint8Array`.
//! - Errors map to the classes of `js/errors.js` ([`errors`]): one
//!   `FontpixelError` subclass per `fontpixel::Error` variant with `kind`,
//!   `exitCode` and structured fields; `UnsupportedError` for unavailable
//!   capabilities ([`capabilities`]).
//! - Fonts, glyphs and bitmaps stay in WASM memory behind the handle
//!   classes of [`handles`]; each area adds methods in its own
//!   `#[wasm_bindgen(js_class = …)] impl` blocks.
//! - Synchronous calls never read files or the network; sidecars come from
//!   named collections or synchronous resolvers ([`resources`],
//!   [`callbacks`]); asynchronous sources go through the JS wrapper.
#![deny(unsafe_code)]

pub mod bitmap;
pub mod callbacks;
pub mod capabilities;
pub mod conv;
pub mod errors;
pub mod formats;
pub mod handles;
pub mod model;
pub mod normalize;
pub mod ops;
pub mod ops_page;
pub mod resources;
pub mod types;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

/// Report Rust panics on the console before the instance traps. A panic is
/// a bug; the trapped instance must be discarded (re-initialize or restart
/// the Worker).
#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        console_error(&format!(
            "fontpixel: internal panic (the WebAssembly instance is now unusable): {info}"
        ));
    }));
}

/// The fontpixel version this package was built from.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
