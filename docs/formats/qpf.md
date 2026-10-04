# Qt/Embedded legacy QPF

Specification recorded before implementation, 2026-10-04. Primary references:
[Qt's archived font documentation](https://doc.qt.io/Archives/qt-4.7/qt-embedded-fonts.html)
and the published packed record declarations in
[Qt's legacy QPF engine](https://github.com/qt/qt/blob/0a2f2382541424726168804be2c90b91381608c6/src/gui/text/qfontengine_qws.cpp).
Only declarative wire facts are used in original production Rust. The licensed
Qt implementation remains an external consumer; its code is not copied here.

The packed ten-byte metrics record contains signed-byte ascent, descent, minimum
left/right bearing and leading; unsigned max width, flags, underline position,
underline thickness and one reserved byte. Flag bit 0 selects Gray8; otherwise
rasters are MSB-first one-bit rows. Unknown native fields remain preserved.

The glyph tree is serialized in three complete preorder passes: five-byte nodes,
then glyph metric arrays, then raster arrays. Each node has big-endian inclusive
u16 minimum/maximum and a byte with child-presence bits 0/1. A glyph metric is
eight bytes: row stride, width, height, flags, signed x bearing, unsigned advance,
signed top bearing and reserved byte. Raster length is stride×height. The model's
bottom bearing is native top bearing minus height. Unicode BMP code positions
are explicit; surrogate positions remain legacy identities rather than invalid
Unicode scalars. Read/write geometry must fit the actual byte fields.

Readers validate tree ordering, disjoint ranges, counts, flags, all three pass
boundaries and row strides using a bounded iterative traversal. Writers emit a
balanced tree of populated code ranges and preserve native bytes only after
verification. No host struct padding or integer endianness is assumed. Names and
point-size identity carried only by filenames are not invented. Native fields,
unknown bytes, geometry, Mono/Gray8 changes and all other model differences are
reported through same-limit reread; strict mode rejects Loss. No pointer data,
allocation or executable behavior from the upstream implementation is retained.
