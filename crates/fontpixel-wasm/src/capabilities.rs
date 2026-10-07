//! Queryable platform and build capabilities.
//!
//! `capabilities()` lists every capability whose availability differs from
//! the native library, with `available` and, when unavailable, the `reason`,
//! `evidence` and `alternative`. Entry points that need an unavailable
//! capability throw `UnsupportedError` carrying the same fields (via
//! [`unsupported`]); they never return an empty success. Area modules add an
//! entry to [`CAPABILITIES`] for every new unavailable item they expose.
use crate::conv;
use crate::errors::{self, Fields, JsResult};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// One capability record (plain data; snake_case keys).
#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    /// Stable identifier (`filesystem`, `threads`, ...).
    pub name: &'static str,
    /// Whether this build on this target provides it.
    pub available: bool,
    /// What the capability covers.
    pub description: &'static str,
    /// Why it is unavailable (empty when available).
    pub reason: &'static str,
    /// Evidence for the reason (empty when available).
    pub evidence: &'static str,
    /// What to use instead (empty when available).
    pub alternative: &'static str,
}

const fn available(name: &'static str, description: &'static str) -> Capability {
    Capability {
        name,
        available: true,
        description,
        reason: "",
        evidence: "",
        alternative: "",
    }
}

const fn missing(
    name: &'static str,
    description: &'static str,
    reason: &'static str,
    evidence: &'static str,
    alternative: &'static str,
) -> Capability {
    Capability {
        name,
        available: false,
        description,
        reason,
        evidence,
        alternative,
    }
}

const FILESYSTEM: Capability = missing(
    "filesystem",
    "Paths, sidecar discovery beside a path, source-alias protection and transactional multi-file outputs (fontpixel-ops `Storage::Filesystem`, the Python `*_path`/`batch_file` entry points).",
    "The WebAssembly package has no file system: browsers have none, and the package never reads paths in Node either, so results do not depend on the host.",
    "wasm32-unknown-unknown has no OS file API (std::fs returns Unsupported); fontpixel-ops builds `Storage::Filesystem` only with its `fs` feature (cap-std), which fontpixel-wasm never enables.",
    "Pass bytes (Uint8Array/ArrayBuffer) and named resource collections or resolver functions; operations return their outputs as named byte arrays. In Node, read and write files with node:fs around the call; for asynchronous sources use withAsyncResources.",
);

const THREADS: Capability = missing(
    "threads",
    "Parallel batch workers (`jobs` > 1) and other multi-threaded execution inside one call.",
    "This build of the WebAssembly module is single-threaded: it has no shared memory or thread spawning.",
    "fontpixel_ops::capabilities().threads is false for target_family = \"wasm\"; a batch whose admission computes more than one worker fails with an Unsupported I/O error instead of running serially.",
    "Run batches with one worker (inline), or spread independent calls across Web Workers / worker_threads, each with its own module instance (compile the module once and pass the WebAssembly.Module to each worker).",
);

#[cfg(feature = "toml")]
const TOML_BATCH: Capability = available(
    "toml_batch",
    "Batch job files in TOML text, parsed through the shared command-line definition.",
);
#[cfg(not(feature = "toml"))]
const TOML_BATCH: Capability = missing(
    "toml_batch",
    "Batch job files in TOML text, parsed through the shared command-line definition.",
    "Compiled out of this build profile (fontpixel-wasm feature `toml`).",
    "scripts/build_wasm.sh enables `toml` only in the `full` profile, to keep the clap and toml parsers out of smaller modules.",
    "Use typed batch requests (plain data), or load the `full` build.",
);

#[cfg(feature = "compat-bdfparser")]
const COMPAT: Capability = available("compat_bdfparser", "The bdfparser-js compatible API.");
#[cfg(not(feature = "compat-bdfparser"))]
const COMPAT: Capability = missing(
    "compat_bdfparser",
    "The bdfparser-js compatible API.",
    "Compiled out of this build profile (fontpixel-wasm feature `compat-bdfparser`).",
    "scripts/build_wasm.sh enables `compat-bdfparser` only in the `full` profile.",
    "Use the native API, or load the `full` build.",
);

#[cfg(feature = "cjk-charsets")]
const CJK: Capability = available(
    "cjk_charsets",
    "Legacy CJK charset tables (GB 2312, Big5, Shift_JIS, KS X 1001, ...).",
);
#[cfg(not(feature = "cjk-charsets"))]
const CJK: Capability = missing(
    "cjk_charsets",
    "Legacy CJK charset tables (GB 2312, Big5, Shift_JIS, KS X 1001, ...).",
    "Compiled out of this build profile (fontpixel feature `cjk-charsets`), which saves the tables' size.",
    "scripts/build_wasm.sh enables `cjk-charsets` in the `full` and `merge` profiles.",
    "Use Unicode-encoded fonts, or load the `full` build.",
);

#[cfg(any(feature = "all-formats", feature = "merge"))]
const OPENTYPE: Capability = available(
    "opentype",
    "OpenType tables and containers (`sfnt`, `Sfnt`, `Cff`), WOFF1/WOFF2 (`woff`) and regional variants (`variants`, `Variants`).",
);
#[cfg(not(any(feature = "all-formats", feature = "merge")))]
const OPENTYPE: Capability = missing(
    "opentype",
    "OpenType tables and containers (`sfnt`, `Sfnt`, `Cff`), WOFF1/WOFF2 (`woff`) and regional variants (`variants`, `Variants`).",
    "Compiled out of this build profile (fontpixel feature `sfnt` and the WOFF codecs), which keeps the module small.",
    "scripts/build_wasm.sh enables them in the `full` and `merge` profiles; the `minimal` profile has BDF/PCF/YAFF only, so the `sfnt`, `woff` and `variants` namespaces are absent there.",
    "Load the `full` (or `merge`) build.",
);

#[cfg(feature = "all-formats")]
const SFNT_COLLECTIONS: Capability = available(
    "sfnt_collections",
    "TrueType/OpenType collections (`Collection`, `sfnt.collection.encode`, WOFF2 collections).",
);
#[cfg(not(feature = "all-formats"))]
const SFNT_COLLECTIONS: Capability = missing(
    "sfnt_collections",
    "TrueType/OpenType collections (`Collection`, `sfnt.collection.encode`, WOFF2 collections).",
    "Compiled out of this build profile (fontpixel feature `ttc`).",
    "scripts/build_wasm.sh enables `ttc` only in the `full` profile; `Collection` and `sfnt.collection` are absent elsewhere and WOFF2 collections are rejected as in a native build without `ttc`.",
    "Load the `full` build.",
);

#[cfg(feature = "all-formats")]
const COLOR_BITMAPS: Capability = available(
    "color_bitmaps",
    "Color embedded bitmaps: CBDT/CBLC and sbix decoding to gray (`sfnt.bitmap.decodeWithColor`, `decodeSbix`, `encodeSbix`).",
);
#[cfg(not(feature = "all-formats"))]
const COLOR_BITMAPS: Capability = missing(
    "color_bitmaps",
    "Color embedded bitmaps: CBDT/CBLC and sbix decoding to gray (`sfnt.bitmap.decodeWithColor`, `decodeSbix`, `encodeSbix`).",
    "Compiled out of this build profile (fontpixel feature `color`, which brings the PNG decoder).",
    "scripts/build_wasm.sh enables `color` only in the `full` profile; the three functions are absent elsewhere.",
    "Load the `full` build.",
);

const SYNC_RESOLVERS: Capability = available(
    "sync_resolvers",
    "Synchronous JS resolver and file-provider callbacks invoked while a call runs (asynchronous sources go through withAsyncResources).",
);

#[cfg(feature = "toml")]
const CLI_PARSER: Capability = available(
    "cli_parser",
    "Parsing `fontpixel` command lines into typed requests with the shared command-line definition (`ops.parseArgs`).",
);
#[cfg(not(feature = "toml"))]
const CLI_PARSER: Capability = missing(
    "cli_parser",
    "Parsing `fontpixel` command lines into typed requests with the shared command-line definition (`ops.parseArgs`).",
    "Compiled out of this build profile: the command-line parser (clap, fontpixel_ops::cli) comes with the fontpixel-wasm feature `toml`.",
    "scripts/build_wasm.sh enables `toml` (and with it fontpixel-ops `clap`) only in the `full` profile.",
    "Pass request objects to the ops functions directly (ops.inspectRequest validates and normalizes them in every profile), or load the `full` build.",
);

#[cfg(any(feature = "all-formats", feature = "merge"))]
const SFNT_OPERATIONS: Capability = available(
    "sfnt_operations",
    "The OpenType operations `ops.variants` (locl-enabled regional faces) and `ops.subset` (sfnt/WOFF/WOFF2 subsetting), and their batch task kinds.",
);
#[cfg(not(any(feature = "all-formats", feature = "merge")))]
const SFNT_OPERATIONS: Capability = missing(
    "sfnt_operations",
    "The OpenType operations `ops.variants` (locl-enabled regional faces) and `ops.subset` (sfnt/WOFF/WOFF2 subsetting), and their batch task kinds.",
    "Compiled out of this build profile (fontpixel-ops feature `opentype`, which needs the TTF/OTF/WOFF/WOFF2 codecs).",
    "scripts/build_wasm.sh builds fontpixel-ops with `opentype` in the `full` and `merge` profiles; `minimal` has BDF, PCF and YAFF only.",
    "Load the `full` or `merge` build.",
);

/// Every capability this build reports, in a fixed order. Area modules
/// append entries for the unavailable items they expose.
pub const CAPABILITIES: &[Capability] = &[
    FILESYSTEM,
    THREADS,
    TOML_BATCH,
    COMPAT,
    CJK,
    SYNC_RESOLVERS,
    OPENTYPE,
    SFNT_COLLECTIONS,
    COLOR_BITMAPS,
    CLI_PARSER,
    SFNT_OPERATIONS,
];

/// The capability records with the operations layer's own answers applied
/// (`fontpixel_ops::capabilities()`: `threads` is false on `wasm32`).
pub fn list() -> Vec<Capability> {
    let ops = fontpixel_ops::capabilities();
    CAPABILITIES
        .iter()
        .map(|cap| {
            let mut cap = cap.clone();
            if cap.name == "threads" {
                cap.available = ops.threads;
            }
            cap
        })
        .collect()
}

/// Every capability whose availability differs from the native library, as
/// `{name, available, description, reason, evidence, alternative}` records.
#[wasm_bindgen(unchecked_return_type = "Capability[]")]
pub fn capabilities() -> JsResult<JsValue> {
    conv::to_js(&list())
}

/// Throw `UnsupportedError` (with `capability`, `reason`, `evidence`,
/// `alternative`) unless `name` is available; unknown names throw
/// `InvalidInputError`.
#[wasm_bindgen(js_name = requireCapability)]
pub fn require_capability(name: &str) -> JsResult<()> {
    let caps = list();
    let cap = caps.iter().find(|c| c.name == name).ok_or_else(|| {
        let names: Vec<_> = caps.iter().map(|c| c.name).collect();
        errors::invalid(format!(
            "unknown capability `{name}`, expected one of: {}",
            names.join(", ")
        ))
    })?;
    if cap.available {
        Ok(())
    } else {
        Err(unsupported(
            cap.name,
            &format!("requireCapability({name:?})"),
        ))
    }
}

/// `UnsupportedError` for an entry point (`what`, e.g. `"batchFile"`) that
/// needs the unavailable capability `name`. Area modules call this from
/// entry points that exist for API completeness but cannot work here.
pub fn unsupported(name: &str, what: &str) -> JsValue {
    let Some(cap) = CAPABILITIES.iter().find(|c| c.name == name) else {
        return errors::invalid(format!("unknown capability `{name}`"));
    };
    Fields::new()
        .set("exitCode", 1)
        .set("capability", cap.name)
        .set("reason", cap.reason)
        .set("evidence", cap.evidence)
        .set("alternative", cap.alternative)
        .error(
            "Unsupported",
            &format!(
                "{what} needs the `{}` capability, which is unavailable: {} Alternative: {}",
                cap.name, cap.reason, cap.alternative
            ),
        )
}
