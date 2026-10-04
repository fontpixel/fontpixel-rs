# MouseGraphics Toolkit / MousePaint fonts

Specification recorded before implementation, 2026-10-04. Primary source:
[Apple II Desktop MGTK Font record](https://github.com/a2stuff/a2d/blob/0fd807038f5d73593c67f1ec4b3484f7117b4115/src/mgtk/MGTK.md#font).
The independent MIT-licensed monobit reader and upstream font-dump utility are
external validation tools; no historical glyph artwork is required.

The three-byte header is type (00 single strip, 80 double strip), last character
code and height (1–16). It is followed by last+1 byte widths and then height rows
of last+1 left bytes, followed by last+1 right bytes for double-strip fonts.
Each byte contributes its low seven bits, least-significant bit first. Declared
widths are 0–7 or 0–14 and also determine advances. All character positions from
zero through last exist, including zero-width glyphs.

The model uses legacy byte codes, origin (0,0), full native height and horizontal
advance equal to width. An optional read charset labels those codes without
remapping them. Original source bytes preserve ignored high bits and trailing
data. Native replay requires actual equality; changed output reports source
records, geometry and metadata changes. Headerless structural detection remains
conservative. Canonical output chooses one or two strips, clears padding and
never clips ink; unrepresentable geometry fails. Explicit Gray8 thresholding is
required. Common cumulative limits and a same-limit writer reread cover all
glyph/index/raster/native/output/report storage. Unsupported model fields produce
Loss and strict registry writing rejects them.
