# Cybiko device fonts

The native stream starts with version byte 1, an unsigned character count,
maximum width and maximum height. Characters occur in native byte-code order
starting at 32. Each record contains unsigned left, top, width and height,
followed by MSB-first byte-padded rows. The record (0,0,1,1) with zero raster is
an absent-character sentinel. Space, when present, anchors the baseline at its
top+height; otherwise maximum height supplies the ascent. Advance equals width.
The reader exposes Legacy slots and the charset label Cybiko; it does not claim
that proprietary symbol slots are Windows-1252 Unicode characters.

The known producer uses the same stream for `.fnt`, `.fntz`, `.fnty`, and `.cyf`.
Extensions alone do not imply a compression codec. The original SDK note’s
archived URL is unavailable; separate compressed variants will not be invented.
Geometry outside unsigned byte bounds fails rather than clipping. Unsupported
left bearings, advances, names, and Unicode interpretations are audited as Loss.

Primary provenance: [author’s documented Cybiko support](https://github.com/kreativekorp/bitsnpicas)
and the original SDK note URL recorded by that tool:
[Eric R. R. Pazera font.txt (archive)](https://web.archive.org/web/20010701031854/http://groups.yahoo.com/group/CybikoDev/files/Pazera/font.txt).
Native importer/exporter execution supplies independent pixel/metric evidence.

## Model, loss, and resource contract

Each file is one strike. Bitmap coordinates use the model’s baseline and upward
positive y; native top-down coordinates are converted explicitly. Unsupported
names, metadata, Unicode sequences, bearings, vertical metrics, and mapping
changes are diagnosed by comparing the source with a native readback. Strict
writes reject every Loss. Grayscale reduction requires an explicit threshold
unless the selected wire layout stores grayscale. No private metadata envelope
is emitted.

Input, tables, mapping expansion, output, conversion scratch, diagnostics, and
writer readback share a cumulative ResourceLimits budget. Counts and products
are checked before allocation; writer output must be readable with the same
limits. Selection uses ordered maps, and mapping trees are traversed at bounded
depth rather than scanning the Unicode address space. Outputs are deterministic.
Original fixtures and external tool observations are recorded in reports/w5.e1.
Reference implementations are external consumers and sources of field facts;
no GPL/MPL implementation is incorporated into the Rust codec.
