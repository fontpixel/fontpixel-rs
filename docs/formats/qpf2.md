# Qt QPF2 prerendered fonts

Specification recorded before implementation, 2026-10-04. Sources:
[Qt's QPF2 documentation](https://doc.qt.io/Archives/qt-4.7/qt-embedded-fonts.html),
[public packed declarations](https://github.com/qt/qt/blob/0a2f2382541424726168804be2c90b91381608c6/src/gui/text/qfontengine_qpf_p.h)
and Qt 4.8.7's actual font engine as an external consumer. No Qt implementation is copied
into production; packed formats, tags and units are interoperability facts.

Version 2.0 begins with QPF2 magic, a 32-bit lock value, major/minor bytes and a
big-endian u16 tagged-header length. TLVs have big-endian u16 tag and length.
Tags include UTF-8 family, linked filename/index/revision, free text, signed
26.6 fixed metrics, glyph depth, byte pixel size/weight/style, writing systems,
and end tag 19. Block headers are tag u16, padding u16 and size u32; payloads
are aligned to four bytes. CMap is a real sfnt cmap, GMap is big-endian u32 glyph
offsets with FFFFFFFF meaning absent, and Glyph contains packed six-byte records
followed by stride×height bytes. Glyph fields are unsigned width/height/stride
and signed x, top-down y and advance. Model y = -native_y-height. Formats 1 and
8 represent packed Mono and Gray8 respectively.

The reader preserves glyph IDs, unencoded cached glyphs, Unicode aliases,
ordered header tags, unknown blocks, padding and native source. Existing bounded
sfnt cmap code interprets supported Unicode maps without scanning every possible
code point. Missing cached rasters and external TTF linkage are explicit source
semantics: no implicit path access or invented outlines/shaping. Retained linkage
and unresolved glyph state must receive loss/error where the bitmap model cannot
represent them fully. Canonical self-contained output uses a read-only lock,
actual cmap/GMap/Glyph blocks and Mono or Gray8 rasters. Fractional metrics retain
their exact native fixed values even where integer model metrics need a reported
projection. Native replay must be verified against the current model. All source,
expanded cmap, models, output, reread and diagnostic storage shares limits, and
strict writing rejects every recorded model difference.

Qt treats glyph ID zero as missing even if a raster is cached there. That raster
is retained as unencoded native data, without inventing a model default glyph.
Canonical output reserves zero when an encoded model glyph occupies that slot;
changed stable IDs and unsupported default-glyph identity are reported.

The default writer emits Gray8, including exact 0/255 expansion of Mono pixels.
The typed `GlyphFormat::Mono` option emits the documented packed one-bit layout,
but Qt 4.8.7's alpha renderer assumes Gray8 regardless of this tag. Packed Mono is
therefore wire support, not a claim of display compatibility with that engine.
An explicit threshold is required to reduce Gray8 to Mono. Native engine checks
cover signed advance/bearings, padded rows, empty-width cells, Unicode aliases
and supplementary-plane mappings.
