# NetBSD WSF binary console fonts

W5.B research recorded before implementation, 2026-10-04.

Primary source: [NetBSD WSF binary console fonts authority](https://raw.githubusercontent.com/NetBSD/xsrc/trunk/local/programs/bdfload/bdfload.c). Supplementary executable format evidence is MIT monobit 0.54, module `wsfont`; the installed release fingerprints and exact external-oracle scope are recorded in `reports/w5.b-crosscheck.json`.

100-byte header: WSFT magic, 64-byte name, then eight LE u32 fields first code/count/encoding/width/height/stride/bit order/byte order. Data follows as fixed cells with explicit byte stride. Orders 0 (known), 1 (left-to-right), 2 (right-to-left) require distinct handling of byte order and bit order. Native numeric codes remain Legacy, with encoding ID retained. Kernel C declarations belong to the separate c/netbsd profile.

One fixed-cell strike and contiguous codes. Preserve exact supported encoding/order/stride metadata; pad missing slots with reported changes. Unknown order identifiers and undersized strides reject. No implicit Unicode remapping or grayscale thresholding.

All reads budget retained input, cumulative fonts/glyphs, table allocations, decompressed pixels and diagnostics before allocation. Every successful write is reopened under identical limits and compared field by field. Source resources and external fonts are never fetched implicitly. Synthetic fixtures are original MIT artwork; third-party tools remain outside the repository.

The [NetBSD kernel font interface](https://raw.githubusercontent.com/NetBSD/src/trunk/sys/dev/wsfont/wsfont.h) identifies alpha fonts when `stride >= width`: those use Gray8 samples, with any remaining bytes as row padding. Monochrome is the complementary case. The kernel's [order conversions](https://raw.githubusercontent.com/NetBSD/src/trunk/sys/dev/wsfont/wsfont.c) independently reverse bits in each byte and reverse bytes in each complete stride; they do not shift partial-width rows. This is also applied to alpha sample bytes when explicit order conversion is requested. The `KNOWN` value leaves orientation unspecified; the read options explicitly define its interpretation and report that interpretation as a Repair. No order-0 inference is hidden. Padding and the full 64-byte name field are retained for native round trips.
