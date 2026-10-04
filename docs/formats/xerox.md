# Xerox Alto font family

Primary specification: [Alto Font Formats, October1980](https://bitsavers.org/pdf/xerox/alto/printing/AltoFontFormats_Oct1980.pdf). The MIT monobit Xerox reader supplies independent decoders. Three distinct native layouts are exposed under the family id `xerox` with a typed variant.

Alto CONVERT `.al` has a height/baseline header and self-relative word pointers to per-character metrics after sixteen-column rasters. Even XW values link extension characters; odd values terminate with advance. Native traversal must validate indices, detect cycles and bound the total joined raster rather than repeatedly concatenate unbounded intermediates.

BITBLT `.strike`/kerned `.ks` has a format word, inclusive code range, optional bounding box, a shared word-aligned strike, x-boundary indexes, optional unsigned spacing pairs and a final dummy/default glyph. Plain and kerned forms preserve their distinct bearings/default mapping. Both indexed and non-indexed plain/kerned layouts are read and written. StrikeIndex maps select one of up to128 bodies and one of256 entries per body; absent mappings and the final dummy are distinguished. Byte pair255/255 is reserved for absence and is rejected as an output spacing pair.

PrePress `.ac` uses typed length-prefixed index entries, named families, character segments, fixed-point horizontal/vertical widths, signed bounding offsets and word-oriented raster directories. Native parsing uses segment lengths and directory anchors. Rotation and padding must have explicit pixel/metric interpretation; source fields and unknown index entries remain properties.

All three variants have native output, bounded table/raster allocation, legacy codes, original-field retention and field-level loss audits. Structural recognition is at most Possible where no distinctive magic exists. Native capability for one variant does not stand in for the others.

The primary AL formula is `y = baseline - HD - XH`; monobit0.54 adds an extra row. AC uses signed16.16 advances in both axes and BBdy as logical height; storage padding above the glyph is excluded. The independent oracle ignores Wy and retains storage height, so validation compares physical pixels and separately asserts native y advances from original fixtures. Nonzero AC rotation is retained with Loss because the core model does not encode its compositor transform.

`XeroxReadOptions` selects variant and optional Alto baseline. `XeroxWriteOptions` adds `XeroxBitbltLayout::{Plain,Kerned,PlainIndex,KernedIndex}`, baseline and rotation overrides. AL supports legacy codes0..255 and extension chains, BITBLT supports inclusive u16 ranges, and AC stores collections of character segments with shared family names. Detection never claims Certain. AC permits fractional fixed-point source widths via retained exact properties; changed values are range checked and losses are audited.

Autodetection uses an allocation-free table probe. BITBLT boundaries, indexed body maps and spacing must fit and the final declared body/spacing position must equal the input length. AC requires bounded character segments and matching raster definitions. This excludes a valid Palm/NFNT font whose initial flags resemble a BITBLT header while preserving the legitimate Palm/NFNT ambiguity.
