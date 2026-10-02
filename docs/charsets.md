# Exact per-glyph charsets (W0.2)

## Scope and source of truth

`fontpixel::charset` implements all 62 canonical keys from the supplied
bdfparser SPEC-v3 §§4, 11.1 and 11.6: nine Unicode identities and 53 non-Unicode
keys. Aliases and registry/encoding normalization match the reference, including
leading zeros, ASCII-only whitespace/digits, unknown names, `KOI8-RU`, `SJIS`,
and `KSX1001`. The checked-in `tools/charsets/registry.json` records every key,
Python codec, kind, GL flag, property pair, and alias.

The initial maps were generated with **CPython 3.12.14**. This is Python codec
parity, not an assertion of conformance to every edition of a charset standard
(e.g. newer revisions of GB18030). Rust uses neither Python at runtime nor a
platform/WHATWG fallback. CPython changes must produce an explicit generated
source change and pass independent comparison before acceptance.

No font reader/writer, Font model, compatibility warning cache, or charset CLI
is implemented by this item. Those remain with their scheduled work items.

## API and exactness

- `charset_table()` provides all canonical metadata in every feature build
- `sanitize_charset` works on BDF properties; `canonical_charset` works on user
  names; `charset_props` reverses a canonical key to the reference properties
- `CharsetCodec::new(name, gl)` resolves a name; `from_key` takes a canonical key
- `cp2unicode(u32)`/`unicode2cp(u32)` preserve the reference's numeric Unicode
  identity semantics, including surrogate values. Values above U+10FFFF fail
- `cp2char` only returns Rust scalar characters, so numeric surrogates return
  `None`; `char2cp(char)` accepts exactly one scalar by construction
- Decode accepts exactly one Unicode code point. A two-character Big5-HKSCS
  decode is unmapped. Encode uses Python's preferred code and must round-trip;
  it is **not** the first/smallest reverse mapping or a best-fit conversion
- Single-byte codecs only accept one-byte codes, DBCS one or two, GB18030 one,
  two or four. Values are big-endian glyph codes, not byte strings with leading
  zero padding. EUC-JP three-byte JIS X 0212 codes and EUC-KR eight-byte sequences
  are deliberately rejected by the reference contract
- Negative integers must be rejected by callers before converting to `u32`
- Unknown names use Unicode identity and `known() == false`. The future font/
  compatibility layer must issue its once-per-name warning; this low-level module
  never logs globally or silently labels an unknown charset as supported

### GL, HBF/HZK, FONTX2, and ISO 2022

`detect_gl` implements the reference's smallest two-byte glyph rule, independent
of iterator order. GL uses both bytes in 0x21..0x7E and translates by 0x8080;
SS2 half-width katakana has no GL representation. Callers must recompute GL if
properties or the glyph set change. `None` selects the normal default; explicit
`Some(false)` overrides even `iso2022cn`, as in Python.

**`iso2022cn` is SPEC's name for GB2312 glyph codes in GL form. It is not an
ISO-2022 text-stream decoder.** An escape/shift stream cannot be represented by
one integer glyph code. ISO-2022-JP/KR/CN streams would require a separate
stateful interface, which is not implemented or advertised here; `hz` is excluded
as required by §11.1. Names outside the SPEC table remain unknown, distinguishable
by `known()`. No escape sequence is parsed or guessed.

HBF can select the exact GB2312, Big5, JIS X 0208 or KSC 5601 map, with explicit
GL selection where appropriate. `kuten_to_euc` and `euc_to_kuten` convert all
94×94 one-based cells for HZK/HBF, while actual assignment is checked by the codec.
FONTX2 can select exact `shiftjis` or `cp932`; they are deliberately different,
so the future format implementation must choose/document its dialect.

### Windows FNT `dfCharSet`

`windows_charset` describes every documented Windows charset byte. Fixed sets
map to cp1250..1258, cp874, cp932/936/949/950, or johab as appropriate. Windows CJK
sets are not approximated by similarly named standards (CP936 ≠ GB2312, etc.).
DEFAULT, OEM, MAC and SYMBOL values return distinct contextual requirements;
a caller must supply the original code page or font-specific mapping. Macintosh
Roman is available as `macroman`, but MAC_CHARSET alone does not identify its
script. Unknown values remain unknown. No current machine locale is guessed.
This does not bypass a future FNT writer's own glyph-code width/loss checks.

Sources checked 2026-10-02:
- Microsoft [CharacterSet enumeration](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-wmf/0d0b32ac-a836-4bd2-a112-b6000a1b4fc9)
- Microsoft [LOGFONTA character-set semantics](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-logfonta)
- Microsoft [code-page identifiers](https://learn.microsoft.com/en-us/windows/win32/intl/code-page-identifiers)

## Representation and feature behavior

After initial name resolution, both directions use **O(log n)** binary search in
sorted, disjoint `Segment { start, end, value }` arrays. Ascending value runs are
stored directly; arbitrary runs index packed `u32` arrays. No runtime table
building, heap allocation, decompression, or hash map is needed for a lookup.
Duplicate maps share storage (for example cp936/gbk and charset aliases). Separate
reverse maps retain Python encoder preferences and omit non-round-tripping codes.

The `cjk-charsets` feature gates the entire CJK data module and GB18030 algorithms.
Without it, all metadata remains queryable and resolving a known CJK charset
returns `CharsetError::FeatureDisabled`; no runtime fallback approximates it.
`jisx0201`'s one-byte subset is in core even though its Python codec is Shift-JIS.
No format feature automatically pulls in CJK data. No new Cargo dependency was
added. Default/no-default builds remain dependency-free in production unless
`serde` is selected.

GB18030 uses 207 `(linear byte index, Unicode start, length)` runs for all
1,087,996 valid four-byte mappings. Generation enumerates every one of the
1,587,600 syntactically possible sequences, including invalid/unassigned gaps,
then independently checks the preferred reverse mapping for every Unicode value.
Runtime arithmetic validates all four bytes before binary search. It never
assumes all four-byte patterns are valid or treats the byte integer as a linear
Unicode offset.

Initial table payload (before compiler/linker metadata, pointers or code):

| Data | Unique tables | Segment/array payload | Generated source |
|---|---:|---:|---:|
| Core | 35 | 29,492 bytes | 113,062 bytes |
| CJK | 12 | 1,592,384 bytes | 5,734,080 bytes including GB18030 ranges |
| GB18030 four-byte ranges | 207 | 2,484 bytes | included above |

The CJK source is readable numeric Rust, not a runtime-compressed blob. Binary
payload is much smaller than rectangular forward/reverse Unicode-domain arrays.
Per-table counts and sizes are reproducibly recorded in
`tools/charsets/manifest.json`. Source bytes are not final application size.

## Regeneration and verification

Python >= 3.12 and its standard library suffice. No reference checkout, network,
Python dependency, or Rust build script is required to generate the data:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tools/gen_charsets.py
PYTHONDONTWRITEBYTECODE=1 python3 tools/gen_charsets.py --check
```

The generated `.rs` sources and regression digests are committed. `--check`
regenerates in memory and errors on any drift without writing anything. The
registry is deliberately checked in rather than importing a private absolute
reference path. Both scripts disable Python bytecode writes themselves.

The independent reference test requires the supplied, unmodified Python source:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tools/check_charsets.py --reference ../reference/bdfparser
PYTHONDONTWRITEBYTECODE=1 python3 tools/check_charsets.py --reference ../reference/bdfparser --no-cjk
```

It builds a small release Rust probe and compares complete binary mapping pairs
byte for byte, with no digest collision possibility. Rust scans every 16-bit
input and the entire Unicode reverse domain for every key/GL variant. Python
uses `CharsetCodec` directly; for SBCS/DBCS its complete forward image is a proven
complete reverse candidate set under the round-trip requirement. Unicode and
GB18030 use the entire Unicode domain; GB18030 additionally tests all four-byte
candidates. Feature-off runs verify explicit errors for all 17 CJK keys.

`cargo test` checks imported reference canonicalization vectors, Python-generated
full-domain regression digests, feature availability, GL, encoder duplicates,
HKSCS sequences/astral values, forbidden lengths, strict variants, code-grid
round trips and all GB18030 candidate sequences. FNV-1a digests are regression
checks only, never a security primitive or a substitute for the independent
byte-for-byte crosscheck. `scripts/check.sh` includes generator drift validation,
formatting, Clippy, the feature test matrix and cargo-deny.
