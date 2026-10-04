# Phase 5 long-fuzz source audit

Audited 2026-10-04T11:26:22.490628+00:00. Index SHA-256: `16e0cb82777d12b87235e595bef3b9a613ad40be10841e6ca8eeb51d3109fd9f`.

71 current passed campaigns, 5 superseded campaigns; 319,144,269 current executions. All 71 current campaigns match their target, codec subtree and reviewed dedicated helpers (229 distinct source paths). 0 compressed-evidence integrity issues; 0 missing or changed paths in that module scope.

This is a per-module source-identity result. It does **not** mean that every campaign ran against an identical final working tree. Broad shared code and build metadata changed, and some snapshots omitted files.

## Scope and method

Read-only Python SHA-256 comparison against each original compressed source dictionary, with compressed JSON/log hashes checked against the index. No Git, compilation, tests, fuzz or performance commands were run; only these two audit reports were written. The current index and all inspected current source hashes were checked again at the end.

The module scope includes each target and its explicit fuzz helpers; its codec main file and complete .rs subtree; FNT for FON; NFNT for FBIT/Lisa; IIGS support for Hanfont; called sfnt/bitmap, cmap and table helpers; and aliased group-specific helpers. RAW/BDF/SSFN plus their helpers are the transport codec scope. legacy_common, model/bitmap/charset, resources, registry/transport and Cargo metadata are separate. Unrelated codec changes in repository-wide snapshots are excluded from a target result.

## Missing snapshots and shared changes

| Shared path | Changed campaigns | Missing snapshot campaigns |
| --- | ---: | ---: |
| `crates/fontpixel/src/formats/gem/common.rs` | 33 | 0 |
| `crates/fontpixel/src/bitmap/mod.rs` | 46 | 0 |
| `crates/fontpixel/src/formats/catalogue.rs` | 46 | 0 |
| `crates/fontpixel/src/formats/mod.rs` | 54 | 0 |
| `crates/fontpixel/src/formats/registry_codecs.rs` | 38 | 8 |
| `crates/fontpixel/src/formats/transport.rs` | 0 | 59 |
| `crates/fontpixel/src/lib.rs` | 46 | 0 |
| `crates/fontpixel/src/model/budget.rs` | 0 | 71 |
| `crates/fontpixel/src/model/mod.rs` | 46 | 0 |
| `crates/fontpixel/src/model/properties.rs` | 46 | 0 |
| `crates/fontpixel/src/resources.rs` | 60 | 0 |
| `crates/fontpixel/Cargo.toml` | 46 | 0 |
| `fuzz/Cargo.toml` | 69 | 0 |

`model/budget.rs` is absent from the original fuzz source dictionaries listed in the JSON. This is an evidence gap, not proof that it was absent or uncompiled during a run. Its current SHA and the later aggregate snapshots are recorded separately; no old evidence has been rewritten.

The JSON records every affected campaign, each original/current SHA and missing path. Shared helper hash drift is not silently accepted as source equality, and is not counted as unrelated codec failure.

## Transport and later integration evidence

The current transport campaign matches final `resources.rs`, `formats/transport.rs`, registry entry points and RAW/BDF/SSFN codec/helper scope. Its `model/budget.rs` historical SHA is still missing. It covers public native/gzip paths and bounded writer profiles; it does not prove all codecs or all model/CLI APIs were fuzzed.

| Later evidence | Source entries | Snapshot matches current | model/budget.rs present/matches | Exit |
| --- | ---: | --- | --- | ---: |
| `phase5-check-candidate-final1.json.gz` | 718 | True | True | 0 |
| `phase5-extra-gates-final1.json.gz` | 718 | True | True | 0 |
| `phase5-cli-release-confirmed.json.gz` | 718 | True | True | 0 |

These aggregate checks/builds supply final integration evidence and preserve the missing historical fuzz SHA distinction. They are not replacements for per-module long-fuzz campaigns.

## Current campaigns

| Target | Active campaign | Module/harness/dedicated helpers | Shared legacy helper |
| --- | --- | --- | --- |
| `bbc` | `phase5-bbc-fuzz-final` | match | changed |
| `beos` | `phase5-beos-fuzz-final` | match | match |
| `bmfont` | `phase5-bmfont-fuzz-final` | match | not used |
| `c` | `phase5-c-fuzz-final2` | match | not used |
| `catalyst` | `phase5-catalyst-fuzz-final` | match | match |
| `chiwriter` | `phase5-chiwriter-fuzz-final` | match | match |
| `color_bitmap` | `phase5-color_bitmap-fuzz-final` | match | not used |
| `comfont` | `phase5-comfont-fuzz-final` | match | changed |
| `consoleet` | `phase5-consoleet-fuzz-final` | match | changed |
| `cybiko_bitmap` | `phase5-cybiko_bitmap-fuzz-final` | match | changed |
| `daisy` | `phase5-daisy-fuzz-final` | match | changed |
| `dashen` | `phase5-dashen-fuzz-final` | match | match |
| `dosstart` | `phase5-dosstart-fuzz-final` | match | changed |
| `drcs` | `phase5-drcs-fuzz-final` | match | not used |
| `drhalo` | `phase5-drhalo-fuzz-final` | match | changed |
| `edwin` | `phase5-edwin-fuzz-final` | match | changed |
| `embedded_bitmap` | `phase5-embedded_bitmap-fuzz-final` | match | not used |
| `fbit` | `phase5-fbit-fuzz-final` | match | not used |
| `figlet` | `phase5-figlet-fuzz-final` | match | changed |
| `fon` | `phase5-fon-fuzz-final` | match | not used |
| `fzx` | `phase5-fzx-fuzz-final` | match | changed |
| `gdr` | `phase5-gdr-fuzz-final` | match | changed |
| `geos` | `phase5-geos-fuzz-final` | match | match |
| `gpi` | `phase5-gpi-fuzz-final2` | match | match |
| `grasp` | `phase5-grasp-fuzz-final` | match | changed |
| `hanfont` | `phase5-hanfont-fuzz-final3` | match | not used |
| `hmzk_bitmap` | `phase5-hmzk_bitmap-fuzz-final` | match | changed |
| `hppcl` | `phase5-hppcl-fuzz-final` | match | match |
| `hrcg` | `phase5-hrcg-fuzz-final` | match | changed |
| `iigs` | `phase5-iigs-fuzz-final3` | match | not used |
| `image` | `phase5-image-fuzz-final` | match | match |
| `imageset_bitmap` | `phase5-imageset_bitmap-fuzz-final` | match | match |
| `lisa` | `phase5-lisa-fuzz-final` | match | not used |
| `mgtk` | `phase5-mgtk-fuzz-final` | match | not used |
| `mkwinfont` | `phase5-mkwinfont-fuzz-final` | match | changed |
| `nfnt` | `phase5-nfnt-fuzz-final` | match | not used |
| `oberon` | `phase5-oberon-fuzz-final` | match | match |
| `palm` | `phase5-palm-fuzz-final` | match | changed |
| `pcgeos` | `phase5-pcgeos-fuzz-final` | match | match |
| `pcr` | `phase5-pcr-fuzz-final` | match | changed |
| `pfr` | `phase5-pfr-fuzz-final` | match | match |
| `pilfont_bitmap` | `phase5-pilfont_bitmap-fuzz-final` | match | match |
| `playdate_bitmap` | `phase5-playdate_bitmap-fuzz-final` | match | changed |
| `plus3dos` | `phase5-plus3dos-fuzz-final` | match | changed |
| `polyprint` | `phase5-polyprint-fuzz-final` | match | changed |
| `prebuilt` | `phase5-prebuilt-fuzz-final` | match | match |
| `printshop` | `phase5-printshop-fuzz-final` | match | changed |
| `psf2txt` | `phase5-psf2txt-fuzz-final` | match | changed |
| `pxl` | `phase5-pxl-fuzz-final` | match | changed |
| `qpf` | `phase5-qpf-fuzz-final` | match | not used |
| `qpf2` | `phase5-qpf2-fuzz-final` | match | not used |
| `raw` | `phase5-raw-fuzz-final` | match | not used |
| `riscos` | `phase5-riscos-fuzz-final` | match | match |
| `rockbox_bitmap` | `phase5-rockbox_bitmap-fuzz-final` | match | changed |
| `sbf_bitmap` | `phase5-sbf_bitmap-fuzz-final` | match | changed |
| `sfont_bitmap` | `phase5-sfont_bitmap-fuzz-final` | match | match |
| `signum` | `phase5-signum-fuzz-final` | match | changed |
| `ssfn` | `phase5-ssfn-fuzz-final` | match | match |
| `symbos` | `phase5-symbos-fuzz-final3` | match | match |
| `tasprint` | `phase5-tasprint-fuzz-final` | match | changed |
| `transport` | `phase5-transport-fuzz-final` | match | match |
| `u8m_bitmap` | `phase5-u8m_bitmap-fuzz-final` | match | changed |
| `vfont` | `phase5-vfont-fuzz-final` | match | changed |
| `vlw_bitmap` | `phase5-vlw_bitmap-fuzz-final` | match | match |
| `vtfont` | `phase5-vtfont-fuzz-final` | match | match |
| `writeon` | `phase5-writeon-fuzz-final` | match | changed |
| `wsfont` | `phase5-wsfont-fuzz-final` | match | match |
| `wyse` | `phase5-wyse-fuzz-final` | match | changed |
| `xbin` | `phase5-xbin-fuzz-final` | match | changed |
| `xerox` | `phase5-xerox-fuzz-final` | match | match |
| `zapfont` | `phase5-zapfont-fuzz-final` | match | changed |

Targets without a current passed indexed campaign: none. Index stable during audit: True; current-source races: 0.

The five superseded records remain listed in the JSON. Only replacements count as current coverage. Source equality is not branch coverage or a correctness proof; feature-disabled shared code and external dependencies are not inferred from hashes. No Phase 6 work was performed.
