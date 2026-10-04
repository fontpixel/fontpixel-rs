# W3.3 paused checkpoint — 2026-10-04

Historical handoff only. Superseded by [the completed implementation and review](../w3.3.md)
and [integrated phase 3 validation](../phase-3.md).

User reprioritized to a glyph round and permits only one software worker. W3.3
paused immediately; W3.2 retains that slot. This is NOT a candidate for merge.

## Saved work

Branch `w3/image-c`, isolated worktree `fontpixel-rs-w3-image-c`, starting from
accepted main `6ce6931d9df703d76ea481ff9378c2027ac015a8`.

- Read DOT-README, PLAN §§9.5/12, FORMATS, decisions, importer sources and tests.
- Recorded format evidence, metrics, safe input grammar and loss choices in
  docs/formats/image.md, docs/formats/c.md and decisions.md before implementation.
- Added feature-gated image-codec (image crate 0.25.10, selected PNG/BMP/GIF/TGA/PNM
  codecs), TOML, serde dependencies; Cargo.lock updated through successful download.
- Extended existing model metadata serde attributes to image feature.
- Drafted image.rs: catalogue maps, native exact metadata, JSON/TOML, atlas
  packing, PNG/BMP/GIF/TGA/PBM/PGM, resource checks, explicit gray threshold,
  single-frame GIF validation, registered image hooks.
- No C parser/writer, CLI sidecar plumbing, tests, fuzzing or benchmarks yet.

## Verification (honest status)

Dependency-only cargo check completed before image implementation. The next
focused check found three compile errors in the draft: two `from_raw` calls must
borrow `b`, and `Font::recompute_bounds` is not the existing method's name. Check
its real union-bounds API before changing. No fixes/builds started after pause.
No scripts/check.sh, standalone matrix, cargo doc, deny, external crosschecks,
image visual inspection or fuzzing has run. Do not claim image implementation
complete. No images have been generated or visually inspected yet.

## Known work to address on resumption

Beyond compile errors, audit draft carefully: aggregate metadata deserialization
allocation accounting, finite property depth and rational denominators; raw bitmap
scratch duplication; native source path semantics; source/output alias protections;
GIF/PNG frame behavior; serializer allocation bounds; standalone feature clippy;
zero-dimension glyphs; slot-hole/default-ID preservation; JSON/TOML unknown fields.
Native mapping clone on user-supplied options needs budget validation. Output should
not create mappings that need more memory than the same configured reader permits.
Implement full tests before treating documented intended behavior as established.

CLI ownership belongs to W3.3, agreed with HBF worker. Intended shared helpers:
canonical primary-folder scoped resource resolver, strict containment/no symlink
escape, bounded reads, multi-asset persistence with destination/source-alias
preflight and staging before rename. Nothing of this is implemented yet.
HBF worker independently committed additive `resource_limited(path,max_bytes)`
core support on w3.5-hbf c9ee2cf; do not duplicate blindly. Merge retained-input
cfg guards (image,c vs hbf) when coordinating.

## Environment

Use /workspace/scratch/ef9059dd510b/toolchain/env.sh. W3.3 target is
/tmp/fontpixel-w33-target, with CARGO_BUILD_JOBS=2, CARGO_PROFILE_DEV_DEBUG=0,
CARGO_INCREMENTAL=0. Build log /tmp/w33-deps.log. No ongoing build remains.
Requested model Astra/xhigh; actual runtime model/service tier cannot be verified;
fast mode unavailable/unverified. Reference and samples were read only.
