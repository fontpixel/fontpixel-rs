# DEC DRCS / DECDLD (W5.5)

Evidence recorded before implementation, 2026-10-04: DEC VT510 Programmer
Information, [DECDLD](https://vt100.net/docs/vt510-rm/DECDLD.html), parameters
and [VT320 soft characters](https://vt100.net/dec/vt320/soft_characters),
and sixel patterns; DEC VT220 software-compatible 5/6/7 by 10 matrix selectors.
The parser reads download data; it never sends escape sequences to a terminal.

DCS is ESC P or byte 90 hex; ST is ESC backslash or byte 9c hex. Eight optional
unsigned parameters end at `{`. The designator has zero to two intermediates
20–2f followed by a final 30–7e. Sixels 3f–7e store six vertical pixels, least
significant bit uppermost; slash advances six rows and semicolon starts a glyph.
Missing pixels are blank. CR/LF are formatting whitespace. Coordinates are raw
font matrices, before terminal-specific centering, doubling or rendition.

Pfn/Pe/Pss/Pt and the designator are retained as native properties. Character
codes remain Legacy values 20–7f: a DRCS designation is not a Unicode mapping.
94-character sets prohibit slots 20 and 7f; 96-character sets allow them. A single
standalone complete download is supported; partial erase/reload commands requiring
previous terminal state and arbitrary surrounding terminal commands fail explicitly.
A complete 94/96-slot Pe=1 reload is self-contained and accepted. A matching
trailing G0 SCS designation is recognized without terminal execution. A final
semicolon is optional on read and emitted for compatibility with monobit 0.54.0.
An explicit terminal profile resolves omitted dimensions: the default VT320
profile uses 15×12 (9×12 at 132 columns), while VT510 uses 10×16 (6×16).
VT220 selectors 2–4 mean widths 5–7, height 10. VT510 widths 5–10 and heights
1–16 are supported with documented screen-size combinations; invalid combinations
fail. Native baseline is unspecified, defaulting to the cell bottom; callers may
supply an explicit read baseline. Writer full cells use explicit dimensions,
font buffer 0, erase 2, 80-column mode and an unregistered designator by default.

Writes project into a fixed cell and report all changed metrics, mappings,
metadata, aliases, grayscale reduction and added blank slots. Gray8 requires an
explicit threshold. Bounds, control parameters, input, model, diagnostic and
output allocations are bounded cumulatively. Same-limit writer readback validates
the emitted file before success. No unsupported command is silently executed or
ignored.
