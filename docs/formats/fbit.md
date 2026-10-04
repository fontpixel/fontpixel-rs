# Apple Japanese fbit and HFNT resources

Specification recorded before implementation, 2026-10-04. Public references are
[Apple Japan Technote 100004, Resolvable Font Format](http://mirror.informatimago.com/next/developer.apple.com/ja/technotes/tn10004.html)
and the independently implemented MIT-licensed
[monobit fbit/HFNT reader](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/apple/fbit.py).
The Apple mirror is currently unavailable from this environment; its header
declaration is quoted by the independently published MIT source. HFNT's remaining
unknown words are empirical, not assigned invented meanings.

fbit has a 36-byte big-endian header: flags, four-byte resource type, resource ID,
version, fdef ID, fdef pointer, FOND ID, priority, reserved long, height, width,
style, reserved word, first/last encoded codes. Native fdef 0 embeds bitmap bytes
after its counted six-byte run table. fdef 5 adds 78 bytes of native extension and
places rasters in the data fork; fdef 7 adds 64 bytes and contains the data-fork
bitmap offset at extension byte 16. Unknown fdef execution is never attempted.
Run records contain first ordinal, last ordinal and count. Each 188-position page
maps onto Shift-JIS trails 40–7E/80–FC and leads 81–9F/E0–FC. Glyphs are fixed
cells with continuous MSB-first bit packing and whole-glyph byte padding.

HFNT begins with three unknown words, a per-glyph word count and an encoding-table
byte count, then u16 codes, a sentinel and rasters. The published consumer infers
a cell width from the square root of the per-glyph bit count; explicit read
geometry handles cases whose dimensions cannot be uniquely inferred. Native
encoding order and source bytes survive exact rewriting. Unicode conversion is
never inferred from arbitrary Kanji ordinals; native legacy mappings are retained.

The codec supports bare records and existing Mac resource/fork envelopes through
the shared native container layer. External data forks require explicit bounded
ReadContext resources; writers return an OutputBundle for split output. Native
fdef code resources, unknown records and envelope metadata remain opaque. Missing
forks, malformed/overlapping runs and unrepresentable geometry fail explicitly.
Header changes, normalization, discarded aliases/names/metrics and Gray8 threshold
projection receive complete model loss reports. Successful output is reread with
the same cumulative limits before strict registry handling.

Envelope signatures take precedence over the bare-resource tag, because a valid
MacBinary filename can begin `fbit` at the same byte offset. Explicit HFNT kind
applies to bare input; envelope resource types remain authoritative. AppleDouble
pairs can be read with the data fork as primary and
`FbitReadOptions::apple_double_resource` selecting the scoped companion. Reading
the resource companion as primary with an explicit `data_fork` also works.
No implicit companion lookup occurs. Native fork bytes are retained once per
collection rather than duplicated for every external-data font resource.
