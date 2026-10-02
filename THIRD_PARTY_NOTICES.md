# Third-party notices

The W0.1 skeleton contains no copied third-party implementation or font sample.
The supplied PLAN.md and FORMATS.md define this project's requirements; the
catalogue metadata transcribes their format names and extension hints.

Rust dependencies (resolved versions are in Cargo.lock):
- clap, serde, serde_json, and most supporting crates: MIT OR Apache-2.0
- strsim and zmij: MIT
- memchr: Unlicense OR MIT (this project selects the MIT alternative)
- unicode-ident: (MIT OR Apache-2.0) AND Unicode-3.0

Use `cargo deny --all-features check` to check every resolved license, source,
and advisory. Dependencies keep their upstream licenses; this notice does not
relicense them. Update this file with exact origins before porting reference code.
