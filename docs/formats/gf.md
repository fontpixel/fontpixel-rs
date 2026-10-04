# TeX GF

The authority is Donald Knuth's public description in
[GFtype](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/gftype.web).
No GPL implementation is used. MIT monobit's GF reader is used independently.

All paint, skip, new-row, short/long beginning-of-character, special and locator
commands are decoded with checked bounds. Pointers are validated against actual
record boundaries, including the special records allowed before a character.
The postamble's modulo-256 metric-sharing rule is enforced by the writer.
Design size, dpi, TFM width and fractional escapements follow the PK conversion
rules in [pk.md](pk.md).

GF's `max_m` bounds the cursor after a paint command, hence the raster width is
`max_m-min_m`; row numbers describe pixels, hence height is `max_n-min_n+1`.
This follows GFtype's `max_subcol := max_m-min_m-1`. Monobit 0.54 includes an
additional all-white right column; oracle comparisons account for this disclosed
rectangle difference without changing pixels or bearings.

Output uses long character starts and deterministic bounded paint runs, with
backpointer chains and complete postamble locators. Native ordered comments and
specials survive; unsupported metadata/mappings are reported individually.

Character-prefixed specials (including the sequences addressed by backpointers)
belong to that glyph. Specials between the final character and the postamble
belong to the font. Writers retain that ownership, validate pointers against
command boundaries rather than matching bytes inside special payloads, and
retain earlier omission diagnostics when a subsequent encoding check fails.

Monobit 0.54 provides no GF writer. Independent reciprocal validation therefore
uses TeX Live GFtoPK/PKtoGF and GFtype. Native and independently converted files
are compared by absolute ink coordinates, device advances, TFM data and dpi;
wire tests additionally assert exact native model equality.
