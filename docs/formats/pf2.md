# GRUB PF2 / PFF2

Implementation basis: GNU GRUB Developers Manual, [PFF2 file structure](https://www.gnu.org/software/grub/manual/grub-dev/html_node/File-Structure.html). Consulted 2026-10-04. Only the public format description is used; no GPL implementation is incorporated.

| Wire field | Model |
| --- | --- |
| FILE / PFF2 | Mandatory leading signature |
| NAME, FAMI | Full name, family (NUL-terminated ASCII) |
| WEIG, SLAN | Native ordered header properties |
| PTSZ | Integer point size |
| MAXW, MAXH | Native maximum dimensions, retained headers |
| ASCE, DESC | Ascent and descent |
| CHIX | Sorted 9-byte Unicode / flags / absolute offset records |
| DATA | Sentinel length 0xffffffff, remainder contains glyphs |
| glyph width, height, signed x/y, signed advance | Bounding rectangle and horizontal advance |
| bitmap | Continuous MSB-first bits, top row first, no row padding |

The CHIX section length is the ordinary section length, not an additional payload integer. Equal offsets represent aliases of the same glyph. Uncompressed storage (flags zero) is supported; compressed blocks are reserved by the specification without a published compression definition and fail explicitly. Invalid Unicode other than the explicitly documented GRUB joining variants, duplicate codes, overlapping records, and missing DATA are rejected. Unknown sections are retained as ordered byte properties. Integer overflow, table, input, pixel, and aggregate allocations are bounded.

Output is deterministic and uncompressed. Unsupported codepoints, names, properties, metrics, aliases lost through collisions, gray thresholding, and glyph omissions have located loss reports. Gray input requires an explicit threshold. Native reads written without edits preserve their model. No external tool is used in production.

Metrics convention: PTSZ is interpreted at 72 dpi, giving the nominal pixel size. When absent, nominal size comes from the glyph union height. Missing ascent/descent derive from the glyph union. These derived values allow direct BDF export and are preserved by native roundtrip.

Real GRUB fonts extend the CHIX code with left/right joining bits 0x40000000 and 0x80000000. The public `include/grub/font.h` wire constants describe these fields ([published declaration](https://fossies.org/linux/grub/include/grub/font.h)); no implementation logic is copied. The model retains a variant as a raw `Codepoint::Unicode` source number, as permitted by the model contract; `scalar()` returns None. An informational diagnostic identifies each variant. Only PF2-native source models may write these flagged values; the remaining base bits must be a Unicode scalar. Removing the flags would erase joining state and is never done implicitly.

A missing display name is normalized to `Unknown`; absent maximum-dimension sections are derived from glyph rectangles. Writers always emit NAME, PTSZ, ASCE, DESC, MAXW and MAXH. Global unsigned metrics outside the 16-bit range are clamped with field-specific loss diagnostics; unrepresentable per-glyph geometry fails with located omission diagnostics. Existing oversized maxima are retained, while undersized maxima are increased and reported. Unknown section order and byte data survive; altered property order, duplicate singleton suppression, spelling or raw-record provenance are reported.
