# Last confirmed W2.4 state

Implemented before rollback:
- sfnt::bitmap low-level EBDT/EBLC and bdat/bloc read/write, EBSC read/write.
- Locator formats 1–5, images 1/2/5/6/7/8/9, 1/2/4/8-bit coverage, sparse
  strikes, bounded composite recursion and shared metrics.
- CBDT/CBLC version 3: PNG 17/18/19 and premultiplied BGRA; sbix PNG/dupe reads.
- Native OTB/bdat detector/reader/writer hooks; color read/detect only.
- Typed native options, explicit loss reporting, shared sfnt assembly,
  multiple physical sizes, aliases/unencoded glyphs, sparse glyph coverage,
  glyph/line/vertical metrics, nominal cmap, names and source-table provenance.
- Final fix: gray composite overlap uses bitwise OR, independently verified
  by 0x55 OR 0xAA == 0xFF in FreeType. It initially used max and was corrected.
- Final fix e08d69c: native diagnostics use an explicit budget (one quarter of
  max_total_bytes, capped at 64 MiB); exhaustion fails before asset return.
- Optional png 0.18.1; exact duplicate-ban exception miniz_oxide =0.8.9,
  because png requires ^0.8 and existing flate2 1.1.10 requires ^0.9.
  No license or advisory exception was added.

Completed validation observed in tool outputs:
- 15 targeted unit/integration tests passed after e08d69c, in 0.57 s.
- cargo clippy --locked -p fontpixel --all-features --all-targets -- -D warnings
  passed after e08d69c (3.70 s).
- scripts/check.sh passed (exit 0, including cargo-deny) before final gray
  composite and diagnostic refinements. The full final-reader rerun was started
  but its completion was NOT observed before rollback.
- Default complete parity passed in 52.2 s before final refinements: 67 charset
  tables; 2,000 random/35,941 operations (0 differences); 1,000 random_all/
  17,588 operations (6 existing K12); 41 probes/89 operations (1 existing K12);
  all 251 corpus/5,221 operations (0 differences); 0 NEW divergence.
  Python fingerprint 938d9ed560ef29ba, Rust 5a41f9c31433e3cc; source tree dirty
  during that run. Final clean-commit parity had not been rerun.
- Independent crosscheck on ece15ea-equivalent reader: 140 cases, 380,808 glyph
  comparisons, 41.864 s. All seven Tong BDFs written as OTB and bdat; both supplied
  OTB reference round trips; 121 small synthetic cases; monobit EBDT/bdat round
  trips and EBSC; native sfnt common-table OTS integration. This predates only
  e08d69c's diagnostic-budget change. The attempted final re-run failed to start
  when the worktree disappeared.
- Initial embedded_bitmap fuzz: exit 0; 611 s; 10,061,754 executions;
  cov 2637, ft 5427, RSS 589 MiB.
- Initial color_bitmap fuzz: exit 0; 611 s; 23,839,981 executions;
  cov 4513, ft 10024, RSS 528 MiB.
  ASAN_OPTIONS=detect_leaks=0 (sandbox ptrace limitation), -timeout=10,
  -rss_limit_mb=2048, -max_len=65536. These initial runs predate final refinements
  and MUST NOT substitute for final-code acceptance. A second pair and then a
  final pair were launched. Final sessions 67132/7492 and full check 82174 were
  lost during executor recovery; their outcome is unknown.

Independent-tool limitations, reproduced rather than concealed:
- System FreeType 2.13.3 is PNG-enabled; freetype-py's bundled library returns
  "unimplemented feature" for PNG. Tests used a temporary copy of the Python
  wrapper with libfreetype.so symlinked to the installed system library.
- Width-3 bit-aligned depth-2/4 rows: native/fontTools follow the standard;
  FreeType shifts the second row. The test asserts exactly
  [0,85,170,170,85,0] for the known shifted fixture instead of weakening all
  comparisons. Byte-aligned native writer pixels match FreeType strictly.
- OTS 9.2.0 rejects bitmap-only sfnt: "no supported glyph data table(s) present".
  It accepts an outline-bearing font with native tables but strips EBDT/EBLC.
  OTS acceptance is not claimed as validation of bitmap payloads.
- monobit bdat plus requested EBSC generation raises KeyError('EBLC'). Its bdat
  writer was tested without EBSC; EBDT EBSC and native shared EBSC tested apart.
- sbix duplicate uses referenced image placement, ignoring duplicate header
  offsets, as measured with FreeType. Current glyph's glyf bounds apply separately.

Known intentional limits:
- Apple Huffman image 4 and obsolete image 3 rejected; nonzero colorRef rejected.
- PNG/dupe sbix only; JPEG/TIFF/PDF/mask rejected. No color writer.
- Alpha coverage default, optional alpha-weighted darkness/explicit threshold;
  RGB/metadata projection always reported as loss. BGRA composite darkness
  rejected because componentwise projection cannot preserve RGB composition.
- No arbitrary vector FontSet rasterization, TTC/resource forks or native
  TTF/OTF hook activation; those remain later work.
- Composite components outside parent rectangle rejected; cycles/depth/work and
  whole-font raster/model/diagnostic budgets enforced.
- Native sfnt rewrite reports unrepresented source records and replaces common
  table provenance rather than allowing stale bytes to override edits.
- Vertical pixel metrics retained; design-space vhea/vmtx not generated.
- Global scale 100 units/reference pixel, 64 if ppem >163; overflow errors,
  inexact design metric conversions reported. This is not arbitrary-file
  byte-identical or all-native-metadata-lossless serialization.

Prior-phase caveat requested by parent: W2.3 advisory database online refresh
hit network policy; its standard offline check passed using local advisory data.
W2.4's observed check.sh cargo-deny step passed, but no independent proof of a fresh
online advisory refresh was recorded. Do not claim advisory freshness.

Requested worker parameters: gpt-6-astra, xhigh, fast. Actual model and service
 tier were unverified. No additional agents were started.
