# TeX PK

The implementation is based on Tomas Rokicki's public PK description in
[PKtype](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pktype.web)
and [TUGboat 6(3), 1985](https://www.tug.org/TUGboat/tb06-3/tb13pk.pdf).
The descriptive format sections are the authority; no GPL implementation is used.
The MIT monobit PK reader is consulted as an independent interpretation and oracle.

PK accepts short, extended and long packets, raw continuous MSB-first bits, and
all dyn-f packed runs including repeated rows. Packet length, run/repeat lengths,
geometry and every allocation are bounded. Output uses deterministic raw packets and chooses the smallest short, extended
or long header that fits. Extended unsigned 16-bit escapements retain their full
range; long packets support signed fractional 16.16 escapements.

Design size is in 2^-20 **TeX points**, and pixels-per-point is 16.16 fixed point.
One inch is 72.27 TeX points. `point_size` retains an exact rational; `resolution`
is the nearest integer dpi, with exact hppp/vppp retained in properties. Pixel
size/ppem are rounded design-size times pixels-per-point. TFM width is a signed
2^-20 em fraction; scalable advance is rounded per 1000 units and the original
fixed point is retained. Fractional pixel escapements retain their original
fixed point fields alongside the rounded integral advance. The reference pixel
has its lower-left corner on the baseline: bounds are `(-hoff, voff-height+1)`.
Codes are legacy TeX slot numbers, never guessed to be Unicode.

Preamble comment, checksum, fixed point metrics and special records are preserved
as ordered `tex.*`/`pk.*` properties. Canonical writing may change packet layout
and no-ops. Changes to model mappings, slot order, metadata, aliases, raw property
provenance, metrics and grayscale reduction receive explicit loss entries.

The writer defaults to 10 TeX points and 72 dpi only when the source supplies no
physical metrics. Typed `design_size` and `resolution` options override them.
Every override or fixed-point rounding that changes the resulting model is
reported. Unicode-tagged input codes keep their numbers but become TeX legacy
slots, with one loss entry per mapping; no character-set conversion is guessed.

Independent verification uses TeX Live PKtype and GFtoPK/PKtoGF, monobit 0.54,
and CTAN's original CMR10 300/329 dpi bitmaps. Monobit has no PK writer and ignores
PK preamble physical metrics; the TeX tools and independent wire fixtures cover
those fields. Source URLs and SHA-256 values are recorded by
[`tests/crosscheck/w45.py`](../../tests/crosscheck/w45.py).
