# Default-workload sweep (final strict source)

`scripts/phase6_default_sweep.py` on the frozen v5 binary (`8cd6d66`, gate 162) at **default limits**,
compared byte-for-byte with the frozen v4 binary run with a 1 GiB budget. Inputs: the full Tong
Pixel Sans 14 Large SC BDF (38,653 glyphs) to every writable format, full Tong TTF/OTF to WOFF/WOFF2,
subsets, a variants family and the Unifont + Latin merge.

Result: 109 cases. **47 succeed, all byte-identical to v4.** 57 are rejected by the format itself
(for example no representable legacy mappings, a cell that cannot hold CJK glyphs, or a required
code page). Five stop on a limit:

- `c`, `pbf`, `plan9`, `hmzk`: fixed format limits (C output byte cap, 16-bit PBF offsets,
  1,024 Plan 9 sidecars, 32,767 HMZK entries). They fail at 1 GiB as well.
- `merge.unifont-latin.woff2`: genuinely needs more than 256 MiB (291 MiB peak RSS; the quality-11
  Brotli stage alone admits about 281 MiB). It fails cleanly at the default and succeeds with an
  explicit `--max-total-bytes` of at least 303.9 MiB.

No case is rejected by ledger over-counting.

| case | exit | wall s | peak RSS MiB | status | identical | note |
| --- | ---: | ---: | ---: | --- | --- | --- |
| sc-bdf.bdf | 0 | 0.18 | 64.0 | ok | True |  |
| sc-bdf.pcf | 0 | 0.20 | 70.8 | ok | True |  |
| sc-bdf.snf | 0 | 0.25 | 125.6 | ok | True |  |
| sc-bdf.hex | 1 | 0.12 | 60.6 | rejected |  | fontpixel: HEX glyph does not fit the selected height/descent or has a negative left bearing |
| sc-bdf.draw | 1 | 0.59 | 109.1 | rejected |  | fontpixel: operation reported errors; no output returned Error [draw.write.failed] None: console cell dimensions must be positive |
| sc-bdf.psf | 0 | 0.28 | 134.2 | ok | True |  |
| sc-bdf.cpi | 1 | 0.61 | 109.1 | rejected |  | fontpixel: operation reported errors; no output returned Error [cpi.write.failed] None: CPI output requires code_page metadata or an explicit fallback |
| sc-bdf.raw | 1 | 0.62 | 108.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [raw.write.failed] None: console cell dimensions must be positive |
| sc-bdf.yaff | 0 | 0.25 | 90.0 | ok | True |  |
| sc-bdf.pf2 | 0 | 0.27 | 102.0 | ok | True |  |
| sc-bdf.fnt | 0 | 0.20 | 83.7 | ok | True |  |
| sc-bdf.fon | 0 | 0.18 | 83.9 | ok | True |  |
| sc-bdf.fontx2 | 0 | 0.20 | 85.3 | ok | True |  |
| sc-bdf.hzk | 1 | 0.12 | 78.9 | rejected |  | fontpixel: HZK/ASC requires an explicit preset or complete geometry |
| sc-bdf.hbf | 1 | 0.12 | 78.9 | rejected |  | fontpixel: HBF: empty bitmap cell |
| sc-bdf.hanfont | 1 | 0.13 | 78.9 | rejected |  | fontpixel: hanfont writing syllables requires retained native components; inverse decomposition is undefined |
| sc-bdf.nfnt | 1 | 0.23 | 78.9 | rejected |  | xel: operation reported errors; no output returned Error [nfnt.no_representable_glyphs] Loss: No legacy byte mappings or designated missing glyph can be encoded |
| sc-bdf.bdat | 0 | 0.27 | 110.7 | ok | True |  |
| sc-bdf.iigs | 1 | 0.14 | 78.9 | rejected |  | fontpixel: IIgs output requires at least one legacy byte mapping |
| sc-bdf.otb | 0 | 0.28 | 110.4 | ok | True |  |
| sc-bdf.ttf | 0 | 0.68 | 128.7 | ok | True |  |
| sc-bdf.otf | 0 | 0.74 | 88.7 | ok | True |  |
| sc-bdf.woff | 0 | 1.30 | 89.0 | ok | True |  |
| sc-bdf.woff2 | 0 | 8.44 | 238.3 | ok | True |  |
| sc-bdf.ttc | 0 | 1.01 | 89.2 | ok | True |  |
| sc-bdf.color | 0 | 0.71 | 180.9 | ok | True |  |
| sc-bdf.vector-raster | 0 | 0.85 | 88.9 | ok | True |  |
| sc-bdf.svg | 0 | 0.93 | 88.3 | ok | True |  |
| sc-bdf.bmfont | 0 | 1.30 | 193.3 | ok | True |  |
| sc-bdf.image | 0 | 0.58 | 130.2 | ok | True |  |
| sc-bdf.c | 1 | 0.16 | 78.9 | resource-limit |  | still fails with the probe budget at 145 MiB peak RSS (a fixed codec/format limit): fontpixel: I/O error: C output exceeds byte budget |
| sc-bdf.kbitx | 0 | 0.28 | 88.2 | ok | True |  |
| sc-bdf.kbits | 0 | 0.28 | 127.6 | ok | True |  |
| sc-bdf.pbf | 1 | 0.14 | 78.9 | resource-limit |  | still fails with the probe budget at 90 MiB peak RSS (a fixed codec/format limit): fontpixel: PBF bucket table offset exceeds u16 |
| sc-bdf.sfd | 0 | 0.31 | 123.6 | ok | True |  |
| sc-bdf.pk | 0 | 0.34 | 139.1 | ok | True |  |
| sc-bdf.gf | 0 | 0.19 | 78.9 | ok | True |  |
| sc-bdf.plan9 | 1 | 0.13 | 78.9 | resource-limit |  | still fails with the probe budget at 79 MiB peak RSS (a fixed codec/format limit): fontpixel: Plan 9 output sidecars exceeds configured limit 1024 |
| sc-bdf.amiga | 0 | 0.22 | 82.6 | ok | True |  |
| sc-bdf.gem | 1 | 0.29 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [gem.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.vfont | 1 | 0.27 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [vfont.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.fzx | 1 | 0.31 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [fzx.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.figlet | 1 | 0.14 | 78.9 | rejected |  | fontpixel: FIGfont cell cannot contain glyph bounds and advance without clipping |
| sc-bdf.palm | 1 | 0.27 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [palm.write.failed] Loss: Palm font has no representable character slots |
| sc-bdf.gdr | 0 | 0.33 | 111.5 | ok | True |  |
| sc-bdf.drcs | 1 | 0.15 | 78.9 | rejected |  | fontpixel: DRCS output has no representable Legacy glyphs |
| sc-bdf.bbc | 1 | 0.14 | 78.9 | rejected |  | fontpixel: native output requires legacy mappings or explicit unicode_as_legacy |
| sc-bdf.beos | 0 | 0.47 | 167.1 | ok | True |  |
| sc-bdf.catalyst | 1 | 0.28 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [catalyst.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.chiwriter | 1 | 0.32 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [chiwriter.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.comfont | 1 | 0.15 | 78.9 | rejected |  | fontpixel: native output requires legacy mappings or explicit unicode_as_legacy |
| sc-bdf.consoleet | 1 | 0.16 | 78.9 | rejected |  | fontpixel: Consoleet requires storable code labels |
| sc-bdf.daisy | 1 | 0.41 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [daisy.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.dashen | 1 | 0.36 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [dashen.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.dosstart | 1 | 0.16 | 78.9 | rejected |  | fontpixel: native output requires legacy labels (or explicit unicode_as_legacy) |
| sc-bdf.drhalo | 1 | 0.15 | 78.9 | rejected |  | fontpixel: native output requires legacy labels (or explicit unicode_as_legacy) |
| sc-bdf.edwin | 1 | 0.14 | 78.9 | rejected |  | fontpixel: native output requires legacy labels (or explicit unicode_as_legacy) |
| sc-bdf.fbit | 1 | 0.15 | 78.9 | rejected |  | fontpixel: fbit needs mapped nonempty cells |
| sc-bdf.geos | 1 | 0.15 | 78.9 | rejected |  | fontpixel: GEOS byte-coded output requires explicit unicode_as_legacy |
| sc-bdf.gpi | 1 | 0.14 | 78.9 | rejected |  | fontpixel: GPI output requires word legacy codes (or explicit unicode_as_legacy) |
| sc-bdf.grasp | 1 | 0.15 | 78.9 | rejected |  | fontpixel: native output requires legacy mappings or explicit unicode_as_legacy |
| sc-bdf.hppcl | 1 | 0.28 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [hppcl.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.hrcg | 1 | 0.15 | 78.9 | rejected |  | fontpixel: native output requires legacy mappings or explicit unicode_as_legacy |
| sc-bdf.lisa | 1 | 0.28 | 78.9 | rejected |  | xel: operation reported errors; no output returned Error [nfnt.no_representable_glyphs] Loss: No legacy byte mappings or designated missing glyph can be encoded |
| sc-bdf.mgtk | 1 | 0.14 | 78.9 | rejected |  | fontpixel: MGTK needs a legacy byte mapping |
| sc-bdf.mkwinfont | 1 | 0.14 | 78.9 | rejected |  | fontpixel: native output requires legacy labels (or explicit unicode_as_legacy) |
| sc-bdf.oberon | 0 | 0.20 | 82.3 | ok | True |  |
| sc-bdf.pcgeos | 1 | 0.18 | 78.9 | rejected |  | fontpixel: PC/GEOS SBCS requires explicit unicode_as_legacy |
| sc-bdf.pcr | 1 | 0.14 | 78.9 | rejected |  | fontpixel: native output requires legacy labels (or explicit unicode_as_legacy) |
| sc-bdf.pilfont | 1 | 0.15 | 78.9 | rejected |  | fontpixel: no representable image-font mappings |
| sc-bdf.plus3dos | 1 | 0.14 | 78.9 | rejected |  | fontpixel: native output requires legacy labels (or explicit unicode_as_legacy) |
| sc-bdf.psf2txt | 0 | 0.75 | 165.4 | ok | True |  |
| sc-bdf.polyprint | 1 | 0.37 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [polyprint.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.prebuilt | 1 | 0.15 | 78.9 | rejected |  | fontpixel: Prebuilt requires 1..32767 character names |
| sc-bdf.printshop | 1 | 0.28 | 78.9 | rejected |  | fontpixel: operation reported errors; no output returned Error [printshop.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.pxl | 1 | 0.28 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [pxl.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.riscos | 1 | 0.29 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [riscos.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.sfont | 0 | 0.20 | 79.0 | ok | True |  |
| sc-bdf.signum | 1 | 0.30 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [signum.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.symbos | 1 | 0.29 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [symbos.write.failed] Loss: SymbOS cell height cannot contain source glyphs |
| sc-bdf.tasprint | 1 | 0.29 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [tasprint.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.vtfont | 1 | 0.15 | 79.0 | rejected |  | fontpixel: VFNT source ink does not fit the selected cell |
| sc-bdf.writeon | 1 | 0.15 | 79.0 | rejected |  | fontpixel: native output requires legacy labels (or explicit unicode_as_legacy) |
| sc-bdf.wsfont | 1 | 0.15 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [wsfont.write.failed] Loss: WSF cells cannot contain negative left bearings |
| sc-bdf.wyse | 1 | 0.16 | 79.0 | rejected |  | fontpixel: native output requires legacy mappings or explicit unicode_as_legacy |
| sc-bdf.xbin | 1 | 0.14 | 79.0 | rejected |  | fontpixel: native output requires legacy mappings or explicit unicode_as_legacy |
| sc-bdf.xerox | 1 | 0.31 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [xerox.no_representable_glyphs] Loss: No representable character mappings remain |
| sc-bdf.zapfont | 0 | 0.40 | 151.9 | ok | True |  |
| sc-bdf.rockbox | 1 | 0.16 | 79.0 | rejected |  | fontpixel: glyph does not fit native device cell without clipping |
| sc-bdf.u8m | 0 | 0.43 | 135.6 | ok | True |  |
| sc-bdf.cybiko | 1 | 0.17 | 79.0 | rejected |  | fontpixel: no representable device font mappings |
| sc-bdf.playdate | 1 | 0.14 | 79.0 | rejected |  | fontpixel: glyph does not fit native device cell without clipping |
| sc-bdf.hmzk | 1 | 0.15 | 79.0 | resource-limit |  | still fails with the probe budget at 79 MiB peak RSS (a fixed codec/format limit): fontpixel: HMZK directory entries exceeds configured limit 32767 |
| sc-bdf.sbf | 1 | 0.15 | 79.0 | rejected |  | fontpixel: no representable device font mappings |
| sc-bdf.qpf | 0 | 0.17 | 79.8 | ok | True |  |
| sc-bdf.qpf2 | 0 | 0.20 | 95.6 | ok | True |  |
| sc-bdf.ssfn | 0 | 0.40 | 125.8 | ok | True |  |
| sc-bdf.vlw | 0 | 0.38 | 159.3 | ok | True |  |
| sc-bdf.pfr | 1 | 0.15 | 79.0 | rejected |  | fontpixel: operation reported errors; no output returned Error [pfr.write.failed] Loss: PFR Unicode-to-legacy conversion must be explicit |
| tong-ttf.woff | 0 | 1.90 | 133.0 | ok | True |  |
| tong-ttf.woff2 | 0 | 16.86 | 288.3 | ok | True |  |
| tong-otf.woff | 0 | 3.80 | 79.4 | ok | True |  |
| tong-otf.woff2 | 0 | 15.70 | 254.2 | ok | True |  |
| subset.tong-ttf.ascii.woff2 | 0 | 0.08 | 79.0 | ok | True |  |
| subset.tong-ttf.all-mapped.woff2 | 0 | 6.82 | 172.4 | ok | True |  |
| merge.unifont-latin.ttf | 0 | 1.08 | 195.4 | ok | True |  |
| variants.tong-large.ttf | 0 | 1.02 | 235.6 | ok | True |  |
| merge.unifont-latin.woff2 | 1 | 1.23 | 139.3 | resource-limit |  | genuine: peak RSS 291 MiB with the probe budget exceeds the 256 MiB cap |
| variants.tong-large.woff2 | 0 | 9.19 | 274.0 | ok | True |  |
