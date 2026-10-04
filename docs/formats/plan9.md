# Plan 9 font and subfont

The authorities are the Plan 9 / plan9port
[font manual](https://9fans.github.io/plan9port/man/man7/font.html) and
[image manual](https://9fans.github.io/plan9port/man/man7/image.html), retrieved
from the upstream `9fans/plan9port` manual sources. These describe the format;
implementation code is original Rust.

A font is a height/ascent pair followed by Unicode ranges, optional subfont
starting offsets and relative subfont names. Resolution is solely through the
caller's `ReadContext`; absolute paths, parent traversal and runtime font-server
expressions grant no filesystem authority. A subfont is a Plan 9 image, a
36-byte header and `(n+1)` six-byte little-endian character records. Direct
subfont input exposes legacy slot codes; font manifests expose Unicode.

Modern and legacy image headers and bounded LZ77 compression are recognized.
Pixel coordinates determine byte alignment even for negative rectangle origins.
The image's pixels represent mask coverage. Character rectangles use Fontchar x,
top and bottom; left is signed and width is the unsigned device advance.

Writers return a complete `OutputBundle`, either a direct subfont or a manifest
with as many subfont assets as needed. They never access the filesystem. Sparse
Unicode coverage is represented by separate ranges instead of scanning missing
codepoints. Every lossy mapping, alias/slot change, property, metric, grayscale
conversion and normalization is reported. Identical resource limits apply to
successful output and its reread.

Modern `k1` is monochrome; `k2`, `k4` and `k8` become exact Gray8 coverage. Other
legal RGB, palette, alpha and padding channel layouts are interpreted using
alpha when present, otherwise grayscale or integer RGB luminance, with explicit
read-loss diagnostics for discarded channel information. Legacy ldepth images
invert every encoded sample bit as required by Plan 9. LZ77 codewords are bounded
by both their block and scanline; backward references cannot precede the block.
Unreferenced image ink is reported. Pure storage choices (atlas placement,
compression and blank padding) are canonicalized without claiming semantic loss.

`Compression::Raw` is the default; `Compression::Literal` emits standard bounded
LZ77 blocks with literal codewords (no search compressor), each at most 6000 bytes.
Literal output therefore rejects rows that cannot fit one block. Both modes write
`k1` or exact `k8`; an explicit `gray_threshold` requests lossy binary reduction.
The line height and ascent remain independent of the byte-sized Fontchar image
coordinates and may exceed 255. Plan 9 has no physical dpi field: the model uses
72 dpi with point size equal to line height as a documented conversion convention.

The independent BSD Go `golang.org/x/image/font/plan9font` v0.24.0 decoder verifies
our compressed output and two public-domain real Plan 9 fixed subfonts in both
directions. It only handles compressed modern k1/k2; the native reader's broader
raw/legacy/grayscale coverage is exercised with original wire fixtures. Monobit
0.54 has no Plan 9 codec. No third-party font binaries enter the repository;
[`tests/crosscheck/w45.py`](../../tests/crosscheck/w45.py) downloads hash-checked
samples into the caller's local work directory.
