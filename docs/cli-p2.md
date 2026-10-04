# Additional bitmap formats

These options belong to the phase 4 implementation. Coordinated acceptance is
recorded separately in the phase report. `fontpixel formats --json` reports the
capabilities actually included in the executable. The CLI's default build enables
these formats; a small build can select, for example,
`--no-default-features --features bdf,raw,cpi`. Exact CJK charset conversion also
needs `fontpixel/cjk-charsets`. Reading native legacy codes needs no implicit
Unicode conversion.

Conversion reports describe changes to pixels, mappings and metadata. Add
`--strict` to prevent output on any reported loss. All assets for a multi-file
font must stay together. The CLI stages output before replacing destination
files and protects source assets from accidental overwrite.

## Raw cells, hexdraw and DOS fonts

Raw input requires an explicit preset or width and height. Additional options
specify count, first slot, charset, bit order, row order, extra bytes after each
row and a file prefix. A preset supplies defaults; explicit options override them.

```sh
fontpixel info font.f16 --from raw --raw-preset vga16
fontpixel convert font.f16 -o font.cpi --from raw --raw-preset vga16 \
  --raw-charset cp437 --cpi-code-page 437
fontpixel convert font.cpi -o standalone.cp --cpi-version cp
fontpixel convert font.cpi -o nt.cpi --cpi-version font-nt
fontpixel info EGA.CPX --from cpi --json
fontpixel convert font.cpi -o compressed.cpx
fontpixel convert unicode.bdf -o font.draw --to draw
```

Other raw presets include `vga8`, `vga14`, `c64`, `64c`, `spectrum`, `ch8` and
`msx`. The `64c` preset accepts partial character sets and preserves the source's
two-byte little-endian load address. New files use `$3800`; override the fallback
with `--raw-default-load-address 0x2000`. An explicit prefix size other than two
bytes clears the preset's fallback address.

Use `--cpi-pointer-mode segmented` only for input with segment:offset
pointers. CPI writes FONT, FONT.NT, DRFONT, standalone CP and compressed CPX;
`.cp` defaults to CP, `.cpx` to CPX and other CPI destinations to FONT. An
explicit incompatible version with a `.cpx` destination is rejected. CPX has
DOS single-segment size and compression constraints; inputs that do not fit
receive an error with a conversion report. CPI expects legacy code-page slots,
not unconverted Unicode mappings.

## GRUB, FONTX2 and HZK

```sh
fontpixel convert unicode.bdf -o grub.pf2
fontpixel convert ascii.bdf -o ascii.fon --to fontx2 \
  --fontx2-mode single --fontx2-charset ascii --fontx2-width 8 --fontx2-height 16
fontpixel info ascii.fon --from fontx2 --fontx2-read-charset ascii
fontpixel info HZK16 --from hzk --hzk-preset HZK16
fontpixel convert chinese.bdf -o HZK16 --to hzk --hzk-preset HZK16
```

The last command needs CJK charset tables. HZK filenames alone do not determine
geometry. All presets are listed in [the HZK specification](formats/hzk.md).
Custom geometry uses `--hzk-width`, `--hzk-height`, `--hzk-first`, `--hzk-last`
and `--hzk-code-space ascii|gb2312`; `--hzk-storage columns` selects transposed
storage. A complete custom geometry can also override a preset. `--hzk-count`
requires an exact number of input/output glyph cells. Code arguments accept
decimal, `0xA1A1` and `U+0041` notation.

## Macintosh containers

```sh
fontpixel info legacy.dfont --from nfnt
fontpixel convert legacy.dfont -o legacy.bin --to nfnt --nfnt-container macbinary
fontpixel convert legacy.dfont -o legacy.font --to nfnt --nfnt-container apple-double
fontpixel info legacy.font --from nfnt --nfnt-apple-double-resource ._legacy.font
```

`--nfnt-container` accepts `bare`, `resource-fork`, `dfont`, `suitcase`,
`macbinary`, `apple-single` and `apple-double`; default output is dfont.
`--nfnt-resource-type font|nfnt` chooses the resource kind. Macintosh glyph codes
are legacy bytes; `--nfnt-read-charset` declares their identity without remapping.
Gray8 is retained by default. `--gray-threshold` explicitly requests monochrome.
AppleDouble's companion must be a safe relative resource path.

## Editor formats and texture pages

```sh
fontpixel convert unicode.bdf -o atlas.fnt --to bmfont --bmfont-encoding binary \
  --bmfont-page-width 256 --bmfont-page-height 256 --bmfont-padding 1
fontpixel info atlas.fnt --from bmfont
fontpixel convert bitmap.sfd -o bitmap.kbits
```

BMFont supports text, XML and binary descriptions, with PNG page assets.
`--bmfont-read-threshold` explicitly reduces imported coverage to monochrome;
otherwise grayscale samples survive. SFD reads and writes bitmap strikes;
outlines are not rasterized. Kbits preserves grayscale and native metadata.
Shared extensions such as `.fnt` require `--from` or `--to` when content or the
requested output is ambiguous.

## TeX and Plan 9

```sh
fontpixel convert bitmap.bdf -o bitmap.pk --tex-design-size 21/2 --tex-dpi 72
fontpixel convert bitmap.pk -o bitmap.gf
fontpixel convert unicode.bdf -o bitmap.plan9 --to plan9 \
  --plan9-container font --plan9-max-glyphs 128 --plan9-compression literal
```

TeX resolution accepts a common `--tex-dpi` or both `--tex-dpi-x` and
`--tex-dpi-y`; design size is an exact positive integer/fraction in points.
Plan 9 writes a Unicode mapping file with subfonts, or one direct legacy subfont
via `--plan9-container subfont`. Compression is `raw` or deterministic standard
LZ77 `literal` blocks. Gray8 remains coverage unless a threshold is requested.

## Machine-specific fonts

```sh
fontpixel convert unicode.bdf -o bitmap.gem --to gem --gem-unicode-as-legacy \
  --gem-byte-order big --gem-word-order big
fontpixel convert bitmap.gem -o bitmap.snf --snf-byte-order little \
  --snf-layout msb,big,4,1,top-down
fontpixel convert latin.bdf -o bitmap.font --to amiga --amiga-packaging contents
```

GEM's explicit Unicode reinterpretation reports loss. Reader overrides are
`--gem-read-byte-order` and `--gem-read-word-order`. SNF layout fields are bit
order, byte order, row alignment, scan unit and row order. New output includes
a vendor property describing its bitmap layout; `--snf-no-layout-property`
omits it, so reopening may require `--snf-read-layout`. Ambiguous old layouts
require explicit configuration. Amiga packaging is `auto`, `contents` or
`strike`; contents output includes its size assets. `--amiga-color-depth 1..8`
selects planar coverage depth. Exact codec restrictions and color projections
are documented in the individual [format notes](formats/).
