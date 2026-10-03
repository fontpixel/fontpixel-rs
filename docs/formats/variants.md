# Regional variants and single-substitution OpenType layout

Design recorded before implementation, 2026-10-03 (W2.6).

Sources: PLAN §§4.5, 5.3, 9.3–9.4; the supplied read-only Tong
`tools/export-rs/python_reference.py` (`plan`, `signature`, `feature_text`),
`prepare.py`, and `src/main.rs`; Microsoft's OpenType
[GSUB](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub),
[layout common tables](https://learn.microsoft.com/en-us/typography/opentype/spec/chapter2),
and [language tags](https://learn.microsoft.com/en-us/typography/opentype/spec/languagetags).

## Model and deterministic planning

A `Variants` owns one default Font, ordered language alternatives (an explicit
OpenType tag or a recognized BCP-47 tag), and ordered script alternatives.
Language alternatives each specify their scripts; defaults are DFLT/hani/kana/
hang/bopo. Script alternatives populate that script's default LangSys, inherited
by otherwise unconfigured languages, which reproduces Tong's narrow western
punctuation. A language alternative configured for that script combines with its
script default in lookup order; conflicting explicit mappings are diagnosed.
Only effective Unicode mappings participate; alternative-only codepoints become
new default glyphs. Alternate glyph identity compares bitmap depth, dimensions,
bytes and complete metrics, excluding spelling, encoding and source properties.
Glyph order is deterministic Unicode-style name order, matching Tong's plan.
Limits bound source glyphs, bytes, resulting IDs and layout expansion.

## Wire mapping

GSUB 1.0 ScriptList/FeatureList/LookupList are constructed directly. SingleSubst
format 2 uses explicit substitutions. Type 7 extension wrappers and bounded
subtable splitting avoid the 64 KiB relative-offset trap on full CJK fonts.
A bounded reader accepts SingleSubst formats 1/2, Coverage formats 1/2, and type
7 wrappers, validating every glyph, index, offset, ordering and work budget.
Unsupported lookup behavior is an explicit error, never a silent omission.

Native export combines the existing outline, common-table, bitmap and WOFF
layers. Both actual CLI commands are included in W2.6; no general outline
rasterization/FontSet reader or collection work is claimed here (W2.7).

## Subset preservation boundary

Subsetting starts from codepoints or Unicode text, closes over relevant GSUB
single substitutions and TrueType composite dependencies, then compacts IDs.
It rewrites glyph-indexed outlines, cmap, metric arrays, post names, embedded
bitmap strikes and GSUB together, retaining names and safe global metadata.
Unknown glyph-indexed tables must not be copied after remapping. Unsupported
tables or GSUB types fail explicitly unless the caller explicitly requests their
loss, which is reported and rejected in strict mode. Digital signatures become
invalid after editing and are removed with a loss diagnostic.

Independent tests will compare source and subset shaping across all required
languages and western scripts, full Tong plans, fontTools, OTS and FreeType.

## CFF subset bytecode fidelity

CFF subsetting rebuilds CharStrings/charset/FDSelect and relocates Top DICT,
Font DICT and Private DICT offsets. It copies retained Type 2 programs, local and
global subroutines, hint parameters, widths, matrices, strings and other DICT
entries unchanged. Unused subroutines/string entries are deliberately retained:
this avoids changing bias, computed subroutine calls, hints or private semantics.
CID Font DICT indices remain stable. Selected programs have cumulative preflight
budgets. Nondefault CFF legacy encodings fail explicitly until remapping exists;
Unicode cmap and default StandardEncoding behavior remain valid.

Subsets retain sfnt outline flavor; output containers can be sfnt/WOFF/WOFF2.
TTF-to-CFF conversion is generation, not subsetting. CLI rejects a misleading
TTF/OTF extension instead of silently changing the source format. WOFF auxiliary
blocks survive web-to-web subsets; removing them for sfnt is reported.
