# Historical executable font loaders (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[Historical executable font loaders](https://github.com/viler-int10h/Fontraption/blob/master/FORMATS.inc). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

Seven typed variants share one family: FONTEDIT (height byte50, raster99), Fontraption plain (height21, raster25), Fontraption TSR (signature40, height93, raster99), Font Mania (LE offsets/size in header), PSFCOM (height from signature and absolute font address), LETAFONT (raster44), and UDG (raster114). Native signatures and field offsets identify wrappers; no input code is executed. DOS outputs use independently authored 8086 BIOS-font installers; CP/M outputs require independently authored Z80 installation code for documented platform behavior. Input bytes are never executed. Prefix-only inert files are not valid writer output.


## API, projection and limits

The feature and registry ID equal this document's filename. `*Codec` exposes
`read_options` and `write_options`; the module also exposes `read` and `write`.
Readers take `ReadContext` and typed options. Native positions stay `Legacy`;
`baseline` and `charset` supply interpretation without guessing an encoding.
Writers accept only Mono unless `gray_threshold` explicitly converts Gray8.
`cell` selects the output rectangle and `unicode_as_legacy` explicitly permits
numeric Unicode projection into legacy slots. Unsupported labels, additional
fonts, geometry, aliases and metadata are compared with the decoded output and
reported as Loss; `WriteOptions.strict` rejects any such loss.

Input, source model, temporary maps/rasters, output, readback and reports share a
checked cumulative budget. Pixel, glyph, table and resource limits apply before
material allocation. A successful write is reopened with the selected limits.
The implementation uses ordered maps or linear scans, without pairwise glyph
matching. Original tests and crosschecks are in `tests/crosscheck/w5a2.py` and
`reports/w5.a2*`; external source and tools stay under `local-work/w5a2`.

## Executable profiles and authorship

`ComfontVariant` selects `Fontedit`, `Frapt` (default), `FraptTsr`, `Letafont`,
`Mania`, `Psfcom`, or `Udg`. `CpmTarget::{Pcw,Cpc,Spectrum}` is an explicit machine
selection for CP/M variants. Unsupported targets have no enum case; the library
does not pretend to perform universal hardware autodetection. Native executable
prefix and suffix bytes are retained as opaque properties when reading. Writing
replaces them with the project's original instruction emitter; replacement of
unrelated executable metadata is reported as Loss. Recognition signatures and
numeric field offsets are compatibility data, not copied vendor loader bodies.
`comfont/stubs.rs` contains independently authored instructions. No GPL loader
template is embedded or translated into the implementation.

* FONTEDIT and Font Mania execute BIOS INT 10h AX=1110h for 256 glyphs, then DOS
  termination. Fontraption's constrained 25-byte prefix installs the same font
  and returns through the COM PSP. Heights 1–32 are supported.
* Fontraption TSR obtains and hooks INT 10h using DOS services, installs the font,
  then stays resident with a computed paragraph count. Text modes 0–3 and 7 call
  the previous BIOS and reinstall the font with registers/flags restored. Other
  calls chain to the previous handler. The small save frame uses 80186 `PUSHA`,
  consistent with the VGA target. It is not a general DOS resident utility.
* LETAFONT and UDG preserve only their minimal native recognition prefix. New
  code and data relocate into common memory before bank switching. UDG's native
  relocation covers only 250 slots; new code finishes the last six explicitly.
  Both install all 256 eight-row characters on the selected target.
* PSFCOM is a DOS/Z80 polyglot. DOS exits with status 1 (unsupported target), while
  CP/M executes a new installer relocated above C000h. PCW uses XBIOS function
  00E9h with a callback; CPC and Spectrum use a direct bank-zero callback.
  Destinations are B800h, 8000h and 3000h respectively. Spectrum permits a second
  256-slot bank at 3800h. The typed target assumes documented CP/M Plus BIOS
  contracts and does not support PcW16-specific extensions.
* A native 16-row PSFCOM payload on these eight-row targets installs alternate
  scanlines, as the historical profile specifies; this produces an explicit
  `installation.scanlines` Loss. Spectrum's second bank displays 5×8 cells while
  payload rows retain eight bits; `installation.narrow-font` reports this
  runtime geometry projection. Strict output rejects either projection.

The independent execution test uses unmodified Unicorn 2.1.4 and kosarev/z80
1.2.0. Minimal BIOS hooks implement documented service contracts; they do not
interpret installer instructions. Font memory bytes and boundaries are checked.
CP/M bank switching deliberately hides low-memory source bytes, checking that
relocation is functional. TSR tests switch text modes 3/7 and graphics mode 13h,
checking reinstall/chain behavior and preserved registers. These are CPU and
BIOS-contract checks, not a claim of physical-machine or complete-OS testing.

Primary platform contracts: [XBIOS USERF and SCR RUN ROUTINE](https://www.seasip.info/Cpm/xbios.html#00E9),
[BIOS memory maps](https://www.seasip.info/Cpm/xbiosint.html),
[LETAFONT documentation](https://www.seasip.info/Unix/PSF/Amstrad/Letafont/index.html),
[UDG documentation](https://www.seasip.info/Unix/PSF/Amstrad/UDG/index.html).
