# Apple embedded bitmaps (`bdat`/`bloc`)

Primary sources (read before implementation, 2026-10-03): Apple TrueType Reference
Manual [bdat](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6bdat.html)
and [bloc](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6bloc.html).
The implementation shares the bounded EBDT/EBLC machinery described in `otb.md`.
The table names and version-2 header are chosen explicitly; Apple sfnt input is
accepted by W2.1. Native output uses a standalone sfnt, not a resource-fork or
`.dfont` wrapper (the latter belongs to W4.3).

Supported Apple images are 1, 2, 5, 6, 7; index forms 1, 2, 3 are the documented
Apple forms. OpenType sparse forms 4/5 and composites are decoded by the common
layer when encountered, without pretending these are guaranteed AAT support.
Canonical Apple output uses index 1 and image 1/6 and sorts strikes by ppem.
Bitmap-size flags and horizontal/vertical metric mapping follow `otb.md`.

Apple image 4 describes a modified Huffman tree but the public chapter does not
specify its complete node/stream coding. It is explicitly unsupported; the codec
never interprets compressed bytes as raw pixels. Nonzero colorRef and legacy
palette `colr` semantics are rejected. AAT specifically recommends depth 1;
other depths with colorRef=0 are exposed using the documented shared grayscale
coverage convention and should be checked against the intended Apple consumer.
Canonical writing supports monochrome and exact Gray8 representations, with
this interoperability boundary recorded. All native loss diagnostics, validation
and provenance limitations are the same as `otb`.
