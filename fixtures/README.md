# Test fixtures: real fonts

Large real-world fonts used by tests that need production-size inputs (WOFF2
above the 128 KiB parallel-compression threshold, 38,000-glyph merges, the
57,086-glyph Unifont). They are stored gzip-compressed (`gzip -n -9`); the
tests decompress them in memory. Nothing here is shipped in the published
crates, wheels or npm package.

| file | source | uncompressed bytes | SHA-256 of the uncompressed file |
|---|---|---:|---|
| `tong-pixel-sans/TongPixelSans14Large-SC.bdf.gz` | Tong Pixel Sans, `TongPixelSans14Large-SC.bdf` | 5,746,021 | `bade3cb87d44a55e55b7e846ee3cc03d4e89ff5c22406fc8c16c965a8e19372b` |
| `tong-pixel-sans/TongPixelSans14Large-Latin.bdf.gz` | Tong Pixel Sans, `TongPixelSans14Large-Latin.bdf` | 418,907 | `c9f922c5389e26c7ca7a5534833dfbe2ec68a1d02d74895079e0b947a6631e46` |
| `tong-pixel-sans/TongPixelSansMono14Large-Latin.bdf.gz` | Tong Pixel Sans, `TongPixelSansMono14Large-Latin.bdf` | 410,661 | `3c69438f589a76fb3239959d5e74af14a20b2f026ff15cbca04888aa830a8650` |
| `unifont/unifont-13.0.04.bdf.gz` | GNU Unifont 13.0.04 (2020-11-21), the BDF in bdfparser's `tests/fonts/` | 9,331,390 | `8cfecd45af8f2637b21a1cfc1c95f75479219fcdbe1c03255ec27cd510ef8932` |

Licenses:

- Tong Pixel Sans: SIL Open Font License 1.1, [`tong-pixel-sans/OFL.txt`](tong-pixel-sans/OFL.txt)
  (copied unchanged from the font's distribution; the fonts derive from Source
  Han Sans, Source Sans 3, Source Code Pro, Noto Sans and others, see the
  `COPYRIGHT` property of each BDF; Reserved Font Names "Source",
  "Plangothic").
- GNU Unifont 13.0.04: dual-licensed SIL Open Font License 1.1 or GPLv2+ with
  the GNU Font Embedding Exception; redistributed here under the OFL,
  [`unifont/OFL.txt`](unifont/OFL.txt).

The fixtures are unmodified apart from the compression. Check them with:

```sh
for f in fixtures/*/*.bdf.gz; do printf '%s  %s\n' "$(gzip -dc "$f" | sha256sum | cut -d' ' -f1)" "$f"; done
```

Users of these fixtures:

- `crates/fontpixel/src/woff/woff2/tests.rs` (WOFF2 of the Tong SC sfnt)
- `crates/fontpixel-wasm/tests/woff.test.mjs` (WOFF2 byte identity with the
  native CLI on Tong SC)
- `crates/fontpixel-wasm/tests/ops-page.test.mjs` (Unifont + Tong Latin and
  Tong SC + Tong Mono Latin merges)

Benchmarks and cross-checks under `benches/` and `tests/crosscheck/` keep
reading the sibling `samples/` checkout.
