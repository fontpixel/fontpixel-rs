# Format documentation index

Before implementing a format, add <id>.md with public specification references, field/model mapping, ambiguities and choices, and write-time information loss. Catalogue entries alone are not format implementations.

This index reflects the unchanged 99 IDs in [FORMATS.md](../../FORMATS.md). It
is a documentation inventory, not fresh read/write or variant validation. Every ID
listed by `fontpixel formats` has a page here; see [recovery status](../recovery-status.md).

| ID | Existing catalogue name | Extensions |
| --- | --- | --- |
| [`bdf`](bdf.md) | Glyph Bitmap Distribution Format 2.1（含 2.2 的竖排度量） | `.bdf`（及 `.bdf.gz`） |
| [`pcf`](pcf.md) | X11 Portable Compiled Format | `.pcf`、`.pcf.gz` |
| [`snf`](snf.md) | X11 Server Natural Format（X11R4 及更早） | `.snf` |
| [`hex`](hex.md) | GNU Unifont hex | `.hex` |
| [`draw`](draw.md) | Unifont hexdraw 文本 | `.txt`、`.draw` |
| [`psf`](psf.md) | Linux 控制台 PSF1 / PSF2（含 Unicode 表） | `.psf`、`.psfu`、`.psf.gz` |
| [`cpi`](cpi.md) | DOS / FreeDOS 代码页字体 CPI、CP、CPX | `.cpi`、`.cp`、`.cpx` |
| [`raw`](raw.md) | 固定格子的原始点阵（无文件头） | `.f08` `.f14` `.f16` `.ch8` `.64c` `.rom` `.bin` 等 |
| [`yaff`](yaff.md) | monobit 的文本点阵格式 | `.yaff` |
| [`pf2`](pf2.md) | GRUB PF2 | `.pf2` |
| [`fnt`](fnt.md) | Windows FNT 位图字体 v1 / v2 / v3 | `.fnt` |
| [`fon`](fon.md) | Windows FON（NE 可执行文件里的 FONTDIR / FONT 资源） | `.fon` |
| [`fontx2`](fontx2.md) | DOS/V FONTX2（日文） | `.fnt`、`.f16`、`.fx2` |
| [`hzk`](hzk.md) | UCDOS / 汉字库 HZK、ASC（GB2312 区位排列的原始点阵） | `HZK12` `HZK14` `HZK16` `HZK24S/K/H/F` `HZK32` `HZK40` `HZK48` `ASC12` `ASC16` `ASC24` … |
| [`hbf`](hbf.md) | Hanzi Bitmap Font（HBF） | `.hbf` + 点阵文件 |
| [`hanfont`](hanfont.md) | 韩文 DOS 组合点阵字体（조합형 8×4×4 等） | `.han` 等 |
| [`nfnt`](nfnt.md) | Mac NFNT / FONT 资源（及 FOND 家族资源） | 资源分支、`.dfont`、手提箱（suitcase）、MacBinary `.bin`、AppleSingle / AppleDouble |
| [`bdat`](bdat.md) | Apple `bdat`/`bloc`（TrueType 里的苹果版内嵌点阵） | `.ttf`、`.dfont` |
| [`iigs`](iigs.md) | Apple IIgs QuickDraw II 字体 | 文件类型 `$C8` |
| [`otb`](otb.md) | 只含点阵的 OpenType（`EBDT`/`EBLC`，无轮廓） | `.otb` |
| [`ttf`](ttf.md) | TrueType（`glyf` 轮廓；可选内嵌点阵 strike） | `.ttf` |
| [`otf`](otf.md) | OpenType CFF（`CFF ` 轮廓；可选内嵌点阵） | `.otf` |
| [`woff`](woff.md) | WOFF 1.0 | `.woff` |
| [`woff2`](woff2.md) | WOFF 2.0 | `.woff2` |
| [`ttc`](ttc.md) | TrueType / OpenType 集合 | `.ttc`、`.otc` |
| [`color`](color.md) | 彩色点阵：`CBDT`/`CBLC`、`sbix` | `.ttf` |
| [`vector-raster`](vector-raster.md) | 像素化的矢量字体退回点阵（任何 TTF/OTF/WOFF/WOFF2/TTC 的轮廓） | 同上 |
| [`svg`](svg.md) | SVG：整段文字的图像、单字 SVG、SVG 字体（`<font>` 元素） | `.svg` |
| [`bmfont`](bmfont.md) | AngelCode BMFont（文本、XML、二进制描述 + PNG 页面） | `.fnt` + `.png` |
| [`image`](image.md) | 图片字表（PNG、BMP、GIF、TGA、PBM/PGM） | `.png` 等 + 映射 |
| [`c`](c.md) | C/C++ 源码里的点阵数组 | `.h`、`.c` |
| [`kbitx`](kbitx.md) | Bits'n'Picas kbitx | `.kbitx` |
| [`kbits`](kbits.md) | Bits'n'Picas kbits（旧格式） | `.kbits` |
| [`pbf`](pbf.md) | PebbleOS 字体 PBF（v1–v3） | `.pbf` |
| [`sfd`](sfd.md) | FontForge SFD 里的点阵 strike（`BitmapFont` 段） | `.sfd` |
| [`pk`](pk.md) | TeX PK（packed）与 GF（generic font） | `.pk`、`.gf`、`.NNNpk` |
| [`gf`](gf.md) | TeX PK（packed）与 GF（generic font） | `.pk`、`.gf`、`.NNNpk` |
| [`plan9`](plan9.md) | Plan 9 font / subfont | 无固定扩展名、`.font` |
| [`amiga`](amiga.md) | Amiga 字体（`.font` 描述 + `尺寸` 号文件） | `.font` + `8`、`9` … |
| [`gem`](gem.md) | Atari GEM / GDOS 字体 | `.fnt`、`.gem` |
| [`vfont`](vfont.md) | Berkeley vfont | 无固定扩展名 |
| [`fzx`](fzx.md) | ZX Spectrum FZX 比例字体 | `.fzx` |
| [`figlet`](figlet.md) | FIGlet 字体（字符画当点阵：非空格为墨） | `.flf` |
| [`palm`](palm.md) | Palm OS 字体（NFNT 类） | `.pdb`/资源 |
| [`gdr`](gdr.md) | Psion / EPOC GDR | `.gdr` |
| [`drcs`](drcs.md) | DEC DRCS 软字体（VT220 等终端的可下载字符集） | 文本转义序列 |
| [`bbc`](bbc.md) | BBC Micro VDU 23 character definitions, code byte and eight bitmap rows; deterministic streamed writer. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/softfont/bbc.py) |
| [`beos`](beos.md) | BeOS \ | Be; bitmap fonts: Unicode location hash table, variable metrics, grayscale rasters. |
| [`catalyst`](catalyst.md) | Quark Catalyst SYSTEM.FONTS collection: sized resources, 128 offsets/widths, 7-bit Apple pixels and vertical shifts. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/catalyst.py) |
| [`chiwriter`](chiwriter.md) | ChiWriter v3/v4 and compatible FONTRIX/PCPaint/GRASP new-format headers, width/offset tables and fixed-height rasters. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/chiwriter.py)；[来源 2](http://jphdupre.chez-alice.fr/chiwriter/technic3/gcw3v01.html#pagev09) |
| [`comfont`](comfont.md) | DOS FONTEDIT, Fontraption normal/TSR, Font Mania and CP/M PSFCOM, LETAFONT, UDG binary loader envelopes; extract and write their raw font payload layouts. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/comloaders.py)；[来源 2](https://github.com/viler-int10h/Fontraption/blob/master/FORMATS.inc) |
| [`consoleet`](consoleet.md) | Consoleet/vfontas per-glyph text files in a directory, hexadecimal filenames and raster text. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/consoleet.py) |
| [`daisy`](daisy.md) | Daisy-Dot I/II/III and magnified multi-file variants NLQ/NL2/NL3/NL4, variable widths and vertical bitmap streams. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printer/daisydot.py)；[来源 2](https://archive.org/stream/daisydotiiii/Daisy%20Dot%20III_djvu.txt) |
| [`dashen`](dashen.md) | Dashen PFT screen font directory and packed glyphs, optionally followed by an HP PCL printer strike. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/dashen.py) |
| [`dosstart`](dosstart.md) | DOSStart DSF bitmap branch, encoded glyph rows, height and advance information. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/dosstart.py) |
| [`drhalo`](drhalo.md) | Dr. Halo/Dr. Genius fixed-cell FON header and bitmap payload. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/drhalo.py) |
| [`edwin`](edwin.md) | EDWIN text bitmap definitions, code, dimensions, metrics and raster rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/edwin.py)；[来源 2](https://history.dcs.ed.ac.uk/archive/apps/edwin/) |
| [`fbit`](fbit.md) | Apple Japanese fbit and HFNT resources, resolution/encoding tables and optional data-fork bitmap payloads. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/apple/fbit.py)；[来源 2](http://mirror.informatimago.com/next/developer.apple.com/ja/technotes/tn10004.html) |
| [`geos`](geos.md) | Commodore GEOS CVT font collections and bare VLIR strike records, including MEGA extensions documented by Bits'N'Picas. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/geos.py)；[来源 2](https://www.lyonlabs.org/commodore/onrequest/geos/geos-fonts.html) |
| [`gpi`](gpi.md) | OS/2 GPI font resources with FontStart/Metrics/Definition/Kerning/End records, character definition types 1/2/3; NE/LX resource extraction. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/fon/os2/gpifont.py)；[来源 2](https://github.com/altsan/os2-gpi-font-tools) |
| [`grasp`](grasp.md) | PCPaint/GRASP old raster format with fixed-cell header and raw rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/grasp.py) |
| [`hppcl`](hppcl.md) | HP PCL bitmap soft fonts: font/character escape records, descriptor formats, character continuation, orientation and symbol sets. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/softfont/pcl.py)；[来源 2](https://developers.hp.com/system/files/attachments/PCL%20Implementors%20Guide-10-downloading%20fonts.pdf) |
| [`hrcg`](hrcg.md) | AppleSoft Toolkit Hi-Res Character Generator SET bitmaps including high-bit half-dot shifts. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/hrcg.py) |
| [`lisa`](lisa.md) | Apple Lisa font libraries with resource directory and Lisa NFNT-like bitmaps, including compressed strike semantics. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/apple/lisa.py)；[来源 2](https://www.kreativekorp.com/swdownload/lisa/AppleLisaFontFormat.pdf) |
| [`mgtk`](mgtk.md) | Apple II MouseGraphics Toolkit / MousePaint fonts, one/two 7-bit-wide strips and widths. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/mgtk.py)；[来源 2](https://github.com/a2stuff/a2d/blob/main/mgtk/MGTK.md#font) |
| [`mkwinfont`](mkwinfont.md) | Simon Tatham mkwinfont FD text, header properties and per-character ASCII rasters. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/fd.py)；[来源 2](https://www.chiark.greenend.org.uk/~sgtatham/fonts/) |
| [`oberon`](oberon.md) | Project Oberon bitmap FNT screen/printer files, character runs, glyph boxes and little-endian raster words. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/oberon.py)；[来源 2](https://www.projectoberon.net/) |
| [`pcgeos`](pcgeos.md) | PC/GEOS 2+ BSWF font files, font metadata, point-size strikes, metrics, bitmaps and kerning. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/pcgeos.py)；[来源 2](https://github.com/bluewaysw/pcgeos) |
| [`pcr`](pcr.md) | Optiks PCR fixed-width 8-pixel font header and per-glyph rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/optiks.py) |
| [`pilfont`](pilfont.md) | Pillow PILfont .pil metrics plus PBM/PNG glyph atlas. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/image/pilfont.py)；[来源 2](https://github.com/python-pillow/Pillow/blob/main/src/PIL/FontFile.py) |
| [`plus3dos`](plus3dos.md) | ZX Spectrum +3DOS font file header, length/type/load metadata, checksum and 8x8 glyph payload. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/plus3dos.py) |
| [`psf2txt`](psf2txt.md) | psftools textual PSF representation with glyph index, dimensions, Unicode aliases/sequences and bitmap rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/psf2txt.py) |
| [`polyprint`](polyprint.md) | PolyPrint printer bitmap fonts and proportional glyph packing as documented by its utility maintainer. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printer/polyprint.py)；[来源 2](https://www.seasip.info/ZX/polyprint.html) |
| [`prebuilt`](prebuilt.md) | Adobe prebuilt BEPF/LEPF files: endian variants, character names, strikes and grayscale glyph data. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/prebuilt.py)；[来源 2](https://github.com/johnsonjh/NeXTDPS/blob/master/fonts-21/bin/prebuiltformat.h) |
| [`printshop`](printshop.md) | The Print Shop PNF printer fonts, native raster/width arrangement. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printshop.py) |
| [`pxl`](pxl.md) | TeX PXL 1001/1002 font bitmaps, 128-entry directory, TFM widths, design size and dpi; optional TFM metrics. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/metafont/pxl.py)；[来源 2](https://tug.org/TUGboat/tb02-3/tb04fuchspxl.pdf) |
| [`riscos`](riscos.md) | RISC OS older x90y45 and newer Font bitmap files, strike indexes, metrics and grayscale packing. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/riscos.py)；[来源 2](https://gitlab.riscosopen.org/RiscOS/Sources/SystemRes/Fonts) |
| [`sfont`](sfont.md) | SDL SFont marker-row PNG atlas and Kreative RFont extension, explicit code sequence and proportional spacing. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/image/sfont.py)；[来源 2](https://github.com/karlb/sfont) |
| [`signum`](signum.md) | Signum! editor E24 and printer P9/P24/L30 raster character sets. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/signum.py)；[来源 2](https://sdo.dseiler.eu/formats/eset) |
| [`symbos`](symbos.md) | SymbOS FNT variable-width bitmap font header and glyph byte offsets. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/symbos.py)；[来源 2](https://www.symbos.org/) |
| [`tasprint`](tasprint.md) | TasPrint screen/printer character sets, widths up to 16, 16-row glyphs and multi-font sequences; optional AMSDOS envelope. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printer/tasprint.py)；[来源 2](https://www.seasip.info/ZX/tasprint.html) |
| [`vtfont`](vtfont.md) | FreeBSD VFNT0002 console fonts: glyph table and normal/bold left/right Unicode maps. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/vtfont.py)；[来源 2](https://github.com/freebsd/freebsd-src/blob/main/sys/sys/font.h) |
| [`writeon`](writeon.md) | Hercules Write On WOF bitmaps and native header, cells in 8x14 multiples. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/writeon.py) |
| [`wsfont`](wsfont.md) | NetBSD WSF binary fonts with geometry, stride, encoding, byte/bit order; c/netbsd represents the same kernel font structure in C. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/wsfont.py)；[来源 2](https://github.com/NetBSD/src/blob/trunk/sys/dev/wscons/wsconsio.h) |
| [`wyse`](wyse.md) | Wyse-60 downloadable soft-font escape sequences, 8x16 glyph rows and encoded character selectors. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/softfont/wyse.py)；[来源 2](http://bitsavers.org/pdf/wyse/WY-60/880261-01A_WY-60_Programmers_Guide_Jan87.pdf) |
| [`xbin`](xbin.md) | XBIN font section with XBIN header, 256/512 glyph flags and optional palette/screen data. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/xbin.py)；[来源 2](https://web.archive.org/web/20120204063040/http://www.acid.org/info/xbin/x_spec.htm) |
| [`xerox`](xerox.md) | Xerox Alto CONVERT .al, BITBLT .strike/.ks and PrePress .ac variants, each with native directory/metrics/raster packing. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/xerox.py)；[来源 2](http://www.bitsavers.org/pdf/xerox/alto/printing/AltoFontFormats_Oct1980.pdf) |
| [`zapfont`](zapfont.md) | RISC OS ZapFont old fixed-cell form and ZapRedraw UCS chunk/range form. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/zapfont.py)；[来源 2](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/zapredraw.py) |
| [`rockbox`](rockbox.md) | Rockbox RB11/RB12 binary bitmap fonts, glyph/offset/width tables and native row packing. | [来源 1](https://www.rockbox.org/wiki/FontFormat)；[来源 2](https://github.com/Rockbox/rockbox/blob/master/tools/convbdf.c) |
| [`u8m`](u8m.md) | Kreative U8/M UTF-8 font format: bitmap glyphs, mapping tables and platform maps. | [来源 1](https://github.com/kreativekorp/u8m/blob/master/U8MFontFormat.pdf)；[来源 2](https://github.com/kreativekorp/u8m) |
| [`cybiko`](cybiko.md) | Cybiko device font files including native bitmap and compression variants identified by the original SDK documentation. | [来源 1](https://web.archive.org/web/20010701031854/http://groups.yahoo.com/group/CybikoDev/files/Pazera/font.txt)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/CybikoBitmapFontImporter.java) |
| [`playdate`](playdate.md) | Playdate source FNT metrics/kerning plus PNG image table fonts; native compiled font container is a separately tracked variant. | [来源 1](https://sdk.play.date/3.1.2/Inside%20Playdate.html#_fonts)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/PlaydateBitmapFontImporter.java) |
| [`hmzk`](hmzk.md) | Mi Band 2 HMZK device bitmap font header, code directory and glyph data. | [来源 1](https://freeyourgadget.codeberg.page/gadgetbridge_org/internals/specifics/mi-band-2/)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/HMZKBitmapFontImporter.java) |
| [`sbf`](sbf.md) | Sabriel Bitmap Font binary headers, glyph maps, bitmap data and metrics. | [来源 1](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/README.md)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/SBFBitmapFontImporter.java) |
| [`qpf`](qpf.md) | Qt/Embedded 2/3 legacy QPF prerendered bitmap fonts: glyph tree, metrics and mono/grayscale rasters. | [来源 1](https://doc.qt.io/Archives/qt-4.7/qt-embedded-fonts.html)；[来源 2](https://github.com/qt/qt/blob/0a2f2382541424726168804be2c90b91381608c6/src/gui/text/qfontengine_qws.cpp) |
| [`qpf2`](qpf2.md) | Qt for Embedded Linux QPF2 prerendered fonts: tagged header, cmap/glyph maps, raster glyph blocks and optional TrueType linkage. | [来源 1](https://doc.qt.io/Archives/qt-4.7/qt-embedded-fonts.html)；[来源 2](https://github.com/qt/qt/blob/0a2f2382541424726168804be2c90b91381608c6/src/gui/text/qfontengine_qpf_p.h) |
| [`ssfn`](ssfn.md) | Scalable Screen Font 2 binary SFN bitmap/pixmap fragments, glyph references and mappings, plus ASCII source form. | [来源 1](https://gitlab.com/bztsrc/scalable-font2/-/blob/master/docs/sfn_format.md)；[来源 2](https://gitlab.com/bztsrc/scalable-font2/-/blob/master/docs/asc_format.md) |
| [`vlw`](vlw.md) | Processing PFont VLW versions with per-glyph metrics, Gray8 bitmaps and Java data-stream strings. | [来源 1](https://processing.org/reference/loadFont_)；[来源 2](https://github.com/processing/processing4/blob/main/core/src/processing/core/PFont.java) |
| [`pfr`](pfr.md) | Portable Font Resource bitmap strikes, character maps and packed bitmap data in bitmap-bearing PFR files. | [来源 1](https://freetype.org/freetype2/docs/reference/ft2-pfr_fonts.html)；[来源 2](https://github.com/freetype/freetype/blob/42e28aa7adfad33d496987a408438b726a82e645/src/pfr/pfrsbit.c) |
