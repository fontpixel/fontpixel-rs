# Phase 6 reader/harness inventory (reconstructed)

The restored Phase 5 catalogue has 99 canonical IDs and the fuzz manifest has
104 declared binaries. This is entry-point correspondence, not final-source
execution acceptance. The historical Phase 5 index has 71 current campaigns;
it does not cover the complete 104-target inventory on final Phase 6 sources.

## Direct correspondence: 82 IDs

bdf, pcf, snf, hex, draw, psf, cpi, raw, yaff, pf2, fnt, fon, fontx2, hzk,
hbf, hanfont, nfnt, iigs, woff2, svg, bmfont, image, c, kbitx, kbits, pbf,
sfd, pk, gf, plan9, amiga, gem, vfont, fzx, figlet, palm, gdr, drcs, bbc,
beos, catalyst, chiwriter, comfont, consoleet, daisy, dashen, dosstart,
drhalo, edwin, fbit, geos, gpi, grasp, hppcl, hrcg, lisa, mgtk, mkwinfont,
oberon, pcgeos, pcr, plus3dos, psf2txt, polyprint, prebuilt, printshop,
pxl, riscos, signum, symbos, tasprint, vtfont, writeon, wsfont, wyse,
xbin, xerox, zapfont, qpf, qpf2, ssfn, pfr.

Each has an identically named target in fuzz/Cargo.toml.

## Remaining 17 IDs

| Catalogue ID | Existing harness |
| --- | --- |
| bdat, otb | embedded_bitmap |
| ttf | vector_raster, plus glyf/sfnt helpers |
| otf | vector_raster, plus cff/sfnt helpers |
| woff | woff1 and vector_raster |
| ttc | collection and vector_raster |
| color | color_bitmap |
| vector-raster | vector_raster |
| pilfont, sfont, vlw | corresponding *_bitmap target |
| rockbox, u8m, cybiko, playdate, hmzk, sbf | corresponding *_bitmap target |

Other targets include compat_ttf, gsub, subset, imageset_bitmap and transport.
Imagesets are an image-ID native directory profile. The six outline/container
IDs share the vector-raster reader, with additional container/table coverage.
Harness existence does not prove every native variant or registry adapter ran.

## Existing properties and shared tests

Read-only inspection found proptest in 22 integration-test modules. Required
lossless-family properties exist for BDF, PCF, YAFF, PSF2 and OpenType bitmap
tables. BDF/PCF compare selected representable fields; YAFF compares complete
Mono/Gray8 models; PSF2 includes aliases/sequences. The embedded OpenType property
primarily checks bitmap tables rather than randomized full FontSets.

Shared suites contracts.rs, transport_limits.rs and CLI input_budget.rs cover
scoped resources, directories, capacities, cumulative inputs, gzip limits,
strict diagnostics and output protection. Public transport fuzzing exercises
RAW/BDF/SSFN; most native harnesses call codecs directly. No merge/batch-specific
target appeared in the inventoried manifest. PFR's default-profile harness has
fewer typed-option combinations than its deterministic tests.

## Evidence boundary and local continuation

The Phase 5 audit records shared-source drift and absent historical
model/budget.rs hashes. Those campaigns cannot prove final Phase 6 acceptance
after shared/model/PFR changes. No final Phase 6 instrumented campaigns were
performed by this worker. Workspace loss also removed its original ordinary
focused-test logs and source manifest; reconstructed source needs fresh checks.

The user explicitly accepted recording blocked long-run validation in TODO for
continuation on their own computer. Follow the repository's existing
fuzz/README.md and docs/transport-fuzz.md for target-specific feature profiles,
resource framing and commands. Keep all writable corpus/build/artifact output
under local-work; original references, samples and validation inputs remain
read-only. Include earlier-phase targets absent from the 71-campaign index.

For each required target, local completion still requires an effective completed
>=10-minute ASAN run with exact command/profile/sanitizer settings, elapsed time,
exit status, execution count, coverage/features, RSS and complete raw log. Record
source identity before/after, including target helpers, codec and compiled
shared/model/resource/registry/transport code, manifests and lockfiles. Re-run
affected evidence after compiled source changes. Build-only checks, deterministic
unit tests, short smoke runs and seed replay do not substitute for campaigns.
Keep failed/interrupted/superseded records separate.

Also rerun applicable properties, shared/CLI budget/strict tests, final aggregate
gates, rustdoc and dependency checks. Stop compilation/testing/fuzzing before
formal performance windows. W6.4 acceptance stays pending until all required
checks actually complete. Recovery performs none of these operations.

Restoration source inspection reconfirmed the counts above. The complete
99-reader mapping, all 104 declared target paths/features and 22 proptest file
paths are recorded in `reader-target-inventory.json`. See `recovery-status.md`
for the restored-source evidence boundary.
