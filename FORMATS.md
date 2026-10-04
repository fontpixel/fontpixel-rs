# fontpixel：点阵字体格式清单

本表是 `fontpixel` 要支持的全部格式。**每一种都要能读、也要能写**（“读”= 转成内部模型，“写”= 从内部模型生成该格式文件）。
只有在格式本身做不到时才可例外（例如只有 256 个码位的格式写不下 CJK）：这时写入器照样要有，必须把装不下的内容逐项列在转换报告里（见 PLAN.md §5.4），不许静默丢弃。

优先级决定做的先后（PLAN.md §8 的阶段），不决定做不做：P1、P2、P3 都要做完。
“规格来源”指实现时依据的公开资料；不确定的地方先查资料、在 `docs/formats/<id>.md` 里写清依据和判断，再写代码。
写到“调研”的条目，由第 5 阶段先核实格式细节和样本来源，再实现。

`id` 是 CLI 和 API 里的格式名（`--from`、`--to`）。扩展名有歧义（`.fnt` 至少有 5 种格式），所以读的时候先按魔数和结构判断，判断不了才看扩展名，并允许用 `--from` 指定。

## A. Unix / X11 / 控制台

| id | 名称 | 扩展名 | 优先级 | 规格来源与要点 |
|---|---|---|---|---|
| `bdf` | Glyph Bitmap Distribution Format 2.1（含 2.2 的竖排度量） | `.bdf`（及 `.bdf.gz`） | P1 | Adobe BDF 2.1 规格；**行为以 `reference/bdfparser/docs/SPEC-v3.md` 为准**（解析规则 §3、写出格式 §5、加固 §11），输出须与 bdfparser 逐字节一致。容错照 `reference/fontpixel/pipeline/opf/parsers/bdf.py`（CRLF、行长不对、重复 ENCODING、缺 ascent/descent）。 |
| `pcf` | X11 Portable Compiled Format | `.pcf`、`.pcf.gz` | P1 | SPEC-v3 §6；`reference/bdfparser/tests/fonts/golden/*.pcf.gz` 是各种字节序、位序、填充组合的金样。**写 PCF 不依赖外部 `bdftopcf`**。注意 PCF 编码表只有 16 位：U+FFFF 以上的码位写不进去，要报告。 |
| `snf` | X11 Server Natural Format（X11R4 及更早） | `.snf` | P3 | 调研：X11R4/R5 源码中的 `snfstr.h`（MIT/X11 许可，可参考）；与机器字节序、填充相关，读时自动判断，写时可指定。 |
| `hex` | GNU Unifont hex | `.hex` | P1 | 每行 `码位:十六进制`；宽 8 或 16（也接受 Unifont 工具支持的 24、32 宽扩展）；高 16。其他高度的“hex”变体做成选项。 |
| `draw` | Unifont hexdraw 文本 | `.txt`、`.draw` | P2 | Unifont 的 `hexdraw`/`unihex` 工具格式（`-` 和 `#`）。 |
| `psf` | Linux 控制台 PSF1 / PSF2（含 Unicode 表） | `.psf`、`.psfu`、`.psf.gz` | P1 | Linux kbd 文档 `psf.txt`、内核 `include/linux/font.h`、`console_font`。PSF1 只有 256/512 字形、宽 8；PSF2 任意。Unicode 表含序列（组合字符）。 |
| `cpi` | DOS / FreeDOS 代码页字体 CPI、CP、CPX | `.cpi`、`.cp`、`.cpx` | P2 | MS-DOS CPI 格式（FONT 和 DRFONT 两种）、FreeDOS 的 CPX（UPX 压缩的 CPI，读时需解 UPX）；多代码页、多尺寸 → 多个字体。 |
| `raw` | 固定格子的原始点阵（无文件头） | `.f08` `.f14` `.f16` `.ch8` `.64c` `.rom` `.bin` 等 | P2 | 参数化：格宽、格高、字数、首码位、位序、行填充、字符集；常见预设（VGA 8×16、C64、ZX Spectrum `.ch8`、MSX 等）做成命名预设。 |
| `yaff` | monobit 的文本点阵格式 | `.yaff` | P1 | monobit 仓库 `YAFF.md`。人能读、能 diff，适合作内部交换和测试金样。 |
| `pf2` | GRUB PF2 | `.pf2` | P2 | GRUB 源码 `grub-core/font/font.c` 的文件格式说明（只参考格式描述，不抄 GPL 代码）。 |

## B. Windows / DOS / 中日韩 DOS

| id | 名称 | 扩展名 | 优先级 | 规格来源与要点 |
|---|---|---|---|---|
| `fnt` | Windows FNT 位图字体 v1 / v2 / v3 | `.fnt` | P1 | Microsoft “Windows 3.x font file format”（v2）与 v3 扩展；只做点阵（矢量 FNT 读时报不支持）。单字节字符集，最多 256 字形：写时按选定代码页映射，装不下的列入报告。 |
| `fon` | Windows FON（NE 可执行文件里的 FONTDIR / FONT 资源） | `.fon` | P1 | NE 文件格式 + 资源表；一个 FON 含多个 FNT（多尺寸、多样式）。读还要支持 PE（32 位）FON。写 NE（与 Windows 自带工具生成的结构一致，Windows 能装载）。 |
| `fontx2` | DOS/V FONTX2（日文） | `.fnt`、`.f16`、`.fx2` | P2 | 头 `FONTX2`；单字节（ANK）与双字节（Shift-JIS 码段表）两种。 |
| `hzk` | UCDOS / 汉字库 HZK、ASC（GB2312 区位排列的原始点阵） | `HZK12` `HZK14` `HZK16` `HZK24S/K/H/F` `HZK32` `HZK40` `HZK48` `ASC12` `ASC16` `ASC24` … | P2 | 无文件头，按 GB2312 区位顺序（94×94）排列；不同尺寸的行字节数不同；HZK24 系列不含 1–15 区符号（从 16 区开始）、有的为转置（列优先）存放。做成带预设的格式，写时只能写 GB2312 范围，其余报告。 |
| `hbf` | Hanzi Bitmap Font（HBF） | `.hbf` + 点阵文件 | P1 | HBF 规格 1.1（Ross Paterson 等，文本头 + 码段表 + 一个或多个原始点阵文件，支持 GB2312、Big5、JIS、KSC 等双字节码）。读写都要处理多个点阵文件和码段（`HBF_CODE_RANGE`、`HBF_BYTE_2_RANGE`）。 |
| `hanfont` | 韩文 DOS 组合点阵字体（조합형 8×4×4 等） | `.han` 等 | P3 | 调研；把初声、中声、终声的成套点阵按组合规则合成音节，写时可选只写成套部件。 |

## C. Apple / Mac

| id | 名称 | 扩展名 | 优先级 | 规格来源与要点 |
|---|---|---|---|---|
| `nfnt` | Mac NFNT / FONT 资源（及 FOND 家族资源） | 资源分支、`.dfont`、手提箱（suitcase）、MacBinary `.bin`、AppleSingle / AppleDouble | P2 | Inside Macintosh: Text（NFNT 结构）；资源分支格式。读写都要支持这些容器；一个文件可含多个尺寸、样式。 |
| `bdat` | Apple `bdat`/`bloc`（TrueType 里的苹果版内嵌点阵） | `.ttf`、`.dfont` | P2 | 与 `EBDT`/`EBLC` 几乎相同（表名不同、少数字段不同）；放在 sfnt 读写器里一起做。 |
| `iigs` | Apple IIgs QuickDraw II 字体 | 文件类型 `$C8` | P3 | 调研（Apple IIgs Toolbox Reference：Font Manager）。 |

## D. OpenType / 网页 / 矢量化

| id | 名称 | 扩展名 | 优先级 | 规格来源与要点 |
|---|---|---|---|---|
| `otb` | 只含点阵的 OpenType（`EBDT`/`EBLC`，无轮廓） | `.otb` | P1 | OpenType 规格 EBDT/EBLC/EBSC；参考 `reference/tong-pixel-sans/tools/export-rs/src/otb.rs`（已与 FreeType 逐像素核对）和 `reference/fontpixel/pipeline/opf/ingest/otb.py`（读）。可含多个 strike（多尺寸）。 |
| `ttf` | TrueType（`glyf` 轮廓；可选内嵌点阵 strike） | `.ttf` | P1 | 像素轮廓三种：方点合并轮廓（`square`，SPEC-v3 §7.1；通格 `outlines.rs`）、独立方点、圆点（`dot`：一个圆形字形 + 复合字形引用）。`--embed-bitmaps` 时加 `EBDT`/`EBLC`。bdfparser 兼容模式下与 SPEC-v3 §8 的 `write_ttf` 逐字节一致。 |
| `otf` | OpenType CFF（`CFF ` 轮廓；可选内嵌点阵） | `.otf` | P1 | 自己写最小 CFF 编码器（参考通格 `cff.rs`：Top DICT、CharStrings、Private DICT、Global Subrs；圆点用一个全局子程序）。读 CFF 轮廓用 `read-fonts`/`skrifa`。 |
| `woff` | WOFF 1.0 | `.woff` | P1 | W3C WOFF 1.0；zlib；可包装上面任一 sfnt。 |
| `woff2` | WOFF 2.0 | `.woff2` | P1 | W3C WOFF2；brotli；实现 `glyf`/`loca` 变换（必须）和 `hmtx` 变换（可选）；读写都要。压缩质量可调、可并行。 |
| `ttc` | TrueType / OpenType 集合 | `.ttc`、`.otc` | P2 | 读：取其中任一或全部字体。写：多个字体共享表（例如通格四个地区做成一个 TTC 的方案）。 |
| `color` | 彩色点阵：`CBDT`/`CBLC`、`sbix` | `.ttf` | P3 | 读：PNG 解码后按阈值转 1 位（或保留灰度，见 PLAN.md §5.2）。写：可选。 |
| `vector-raster` | 像素化的矢量字体退回点阵（任何 TTF/OTF/WOFF/WOFF2/TTC 的轮廓） | 同上 | P1 | 照 `reference/fontpixel/pipeline/opf/ingest/rasterize.py`：所有轮廓坐标都落在 `upem` 的公约数网格上、且 ppem 在 4–64 之间才视为像素字体；有内嵌点阵且尺寸对得上时优先取点阵。读所有 OpenType 都走这一条。 |
| `svg` | SVG：整段文字的图像、单字 SVG、SVG 字体（`<font>` 元素） | `.svg` | P1 写 / P2 读 | 写照 SPEC-v3 §7.3（图像）与 bdfparser 的 `tosvgfont`。读：先支持本工具写出的 SVG 字体；通用 SVG 字体按路径栅格化（同 `vector-raster` 的网格判断）。 |

## E. 游戏、嵌入式及其他

| id | 名称 | 扩展名 | 优先级 | 规格来源与要点 |
|---|---|---|---|---|
| `bmfont` | AngelCode BMFont（文本、XML、二进制描述 + PNG 页面） | `.fnt` + `.png` | P2 | AngelCode BMFont 文档（file format）。写：生成描述文件 + 页面图（装箱排列）。 |
| `image` | 图片字表（PNG、BMP、GIF、TGA、PBM/PGM） | `.png` 等 + 映射 | P1 | 照 `reference/fontpixel/pipeline/opf/ingest/spritesheet.py`：规则格子（`cell_size` + 字符序列）或不规则（每格 `rect`、字符或名字、可选步进和基线）；前景用调色板或不透明像素；未映射但有内容的格子报错，不丢弃。写：输出字表图 + 同名 JSON/TOML 映射，能原样读回。 |
| `c` | C/C++ 源码里的点阵数组 | `.h`、`.c` | P1 | 照 `reference/fontpixel/pipeline/opf/ingest/header8x8.py`（dhepper font8x8、t-mat）；另做通用数组（`uint8_t font[N][H]`，参数化）和 **Adafruit GFX**（`GFXfont`/`GFXglyph` 结构）。写：通用数组 + Adafruit GFX。P3 再加 **U8g2**（压缩字符串）和 **LVGL**（`lv_font_t` 的 `.c`）。 |
| `kbitx` | Bits'n'Picas kbitx | `.kbitx` | P1 | 照 `reference/fontpixel/pipeline/opf/ingest/kbitx.py`（XML + base64 + WIB 游程编码，灰度 ≥ 0x80 为墨）。写：生成 Bits'n'Picas 能打开的 kbitx。 |
| `kbits` | Bits'n'Picas kbits（旧格式） | `.kbits` | P2 | 调研（Bits'n'Picas 源码，Mozilla 公共许可：只参考格式，不抄代码）。 |
| `pbf` | PebbleOS 字体 PBF（v1–v3） | `.pbf` | P1 | 照 `reference/fontpixel/pipeline/opf/ingest/pebble.py`（v3 详解：无基线，按 `max_height` 放基线）及 PebbleOS 的 `fontgen.py` 说明；v1/v2 调研。 |
| `sfd` | FontForge SFD 里的点阵 strike（`BitmapFont` 段） | `.sfd` | P2 | FontForge SFD 格式文档（只参考文档，不抄 GPL 代码）；只处理点阵部分，轮廓部分按原样保留或忽略（读时报告）。 |
| `pk` / `gf` | TeX PK（packed）与 GF（generic font） | `.pk`、`.gf`、`.NNNpk` | P2 | TeX 的 `pktype`/`gftype` 文档（Knuth，公开）；分辨率（dpi）与设计尺寸换算写进度量。 |
| `plan9` | Plan 9 font / subfont | 无固定扩展名、`.font` | P2 | Plan 9 手册 `font(6)`、`image(6)`；subfont 是图像 + 字形信息，font 文件是码段到 subfont 的映射。 |
| `amiga` | Amiga 字体（`.font` 描述 + `尺寸` 号文件） | `.font` + `8`、`9` … | P2 | AmigaOS `diskfont`（`FontContentsHeader`、`DiskFontHeader`、`TextFont`）；hunk 可执行格式。 |
| `gem` | Atari GEM / GDOS 字体 | `.fnt`、`.gem` | P2 | GEM VDI 字体头（GDOS），字节序随来源（Atari 大端、PC GEM 小端）。 |
| `vfont` | Berkeley vfont | 无固定扩展名 | P3 | BSD `vfont(5)` 手册。 |
| `fzx` | ZX Spectrum FZX 比例字体 | `.fzx` | P3 | FZX 格式说明（Andrew Owen）。 |
| `figlet` | FIGlet 字体（字符画当点阵：非空格为墨） | `.flf` | P3 | FIGlet 规格 `figfont.txt`；写时用 `#`。 |
| `palm` | Palm OS 字体（NFNT 类） | `.pdb`/资源 | P3 | 调研。 |
| `gdr` | Psion / EPOC GDR | `.gdr` | P3 | 调研。 |
| `drcs` | DEC DRCS 软字体（VT220 等终端的可下载字符集） | 文本转义序列 | P3 | DEC VT 手册 DECDLD。 |
| 其他 | monobit 支持而上表没有的通用格式 | — | P3 | 第 5 阶段对照 monobit（MIT 许可，github.com/robhagemans/monobit）的格式清单逐一核对，补进本表后实现。**游戏专用的私有格式一律不收**（PLAN.md §1.3）。 |

## 内部交换与测试金样

- 测试的标准表示用 `yaff`（文本，可 diff），bdfparser 兼容测试用 `bdf`（逐字节）。
- **monobit**（`pip install monobit`，MIT）可以读写上表大部分格式，用作交叉验证的对照工具（PLAN.md §9.3）。不一致时以格式规格和真实软件的行为为准，monobit 自身的 bug 记在 `docs/formats/<id>.md`。

## F. 第 5 阶段普查补充（2026-10-04）

对 monobit 0.54 发布包、固定 Git 提交、Bits’N’Picas 与开发者原始资料逐项普查，
在原 46 个 id 之外增加以下 53 个通用格式家族。**列入计划不表示实现完成**；
实际读写能力仍以 `fontpixel formats` 为准。相同 wire family 的变体共用一个 id，
通用压缩、源码包装和纯轮廓格式不因此重复计为点阵字体家族。
普查范围、全部原始条目去向、可实施依据、样本许可、独立 oracle 和已排除的
游戏专用格式见 [完整报告](reports/w5.1-survey.md) 与
[机器清单](reports/w5.1-survey.json)。本表的全部新增格式均为 P3，读写均须实现。

| id | 格式范围与变体 | 依据 |
| --- | --- | --- |
| `bbc` | BBC Micro VDU 23 character definitions, code byte and eight bitmap rows; deterministic streamed writer. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/softfont/bbc.py) |
| `beos` | BeOS \|Be; bitmap fonts: Unicode location hash table, variable metrics, grayscale rasters. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/beos.py)；[来源 2](http://www.eonet.ne.jp/~hirotsu/bin/bmf_format.txt) |
| `catalyst` | Quark Catalyst SYSTEM.FONTS collection: sized resources, 128 offsets/widths, 7-bit Apple pixels and vertical shifts. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/catalyst.py) |
| `chiwriter` | ChiWriter v3/v4 and compatible FONTRIX/PCPaint/GRASP new-format headers, width/offset tables and fixed-height rasters. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/chiwriter.py)；[来源 2](http://jphdupre.chez-alice.fr/chiwriter/technic3/gcw3v01.html#pagev09) |
| `comfont` | DOS FONTEDIT, Fontraption normal/TSR, Font Mania and CP/M PSFCOM, LETAFONT, UDG binary loader envelopes; extract and write their raw font payload layouts. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/comloaders.py)；[来源 2](https://github.com/viler-int10h/Fontraption/blob/master/FORMATS.inc) |
| `consoleet` | Consoleet/vfontas per-glyph text files in a directory, hexadecimal filenames and raster text. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/consoleet.py) |
| `daisy` | Daisy-Dot I/II/III and magnified multi-file variants NLQ/NL2/NL3/NL4, variable widths and vertical bitmap streams. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printer/daisydot.py)；[来源 2](https://archive.org/stream/daisydotiiii/Daisy%20Dot%20III_djvu.txt) |
| `dashen` | Dashen PFT screen font directory and packed glyphs, optionally followed by an HP PCL printer strike. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/dashen.py) |
| `dosstart` | DOSStart DSF bitmap branch, encoded glyph rows, height and advance information. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/dosstart.py) |
| `drhalo` | Dr. Halo/Dr. Genius fixed-cell FON header and bitmap payload. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/drhalo.py) |
| `edwin` | EDWIN text bitmap definitions, code, dimensions, metrics and raster rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/edwin.py)；[来源 2](https://history.dcs.ed.ac.uk/archive/apps/edwin/) |
| `fbit` | Apple Japanese fbit and HFNT resources, resolution/encoding tables and optional data-fork bitmap payloads. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/apple/fbit.py)；[来源 2](http://mirror.informatimago.com/next/developer.apple.com/ja/technotes/tn10004.html) |
| `geos` | Commodore GEOS CVT font collections and bare VLIR strike records, including MEGA extensions documented by Bits'N'Picas. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/geos.py)；[来源 2](https://www.lyonlabs.org/commodore/onrequest/geos/geos-fonts.html) |
| `gpi` | OS/2 GPI font resources with FontStart/Metrics/Definition/Kerning/End records, character definition types 1/2/3; NE/LX resource extraction. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/fon/os2/gpifont.py)；[来源 2](https://github.com/altsan/os2-gpi-font-tools) |
| `grasp` | PCPaint/GRASP old raster format with fixed-cell header and raw rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/grasp.py) |
| `hppcl` | HP PCL bitmap soft fonts: font/character escape records, descriptor formats, character continuation, orientation and symbol sets. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/softfont/pcl.py)；[来源 2](https://developers.hp.com/system/files/attachments/PCL%20Implementors%20Guide-10-downloading%20fonts.pdf) |
| `hrcg` | AppleSoft Toolkit Hi-Res Character Generator SET bitmaps including high-bit half-dot shifts. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/hrcg.py) |
| `lisa` | Apple Lisa font libraries with resource directory and Lisa NFNT-like bitmaps, including compressed strike semantics. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/apple/lisa.py)；[来源 2](https://www.kreativekorp.com/swdownload/lisa/AppleLisaFontFormat.pdf) |
| `mgtk` | Apple II MouseGraphics Toolkit / MousePaint fonts, one/two 7-bit-wide strips and widths. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/mgtk.py)；[来源 2](https://github.com/a2stuff/a2d/blob/main/mgtk/MGTK.md#font) |
| `mkwinfont` | Simon Tatham mkwinfont FD text, header properties and per-character ASCII rasters. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/fd.py)；[来源 2](https://www.chiark.greenend.org.uk/~sgtatham/fonts/) |
| `oberon` | Project Oberon bitmap FNT screen/printer files, character runs, glyph boxes and little-endian raster words. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/oberon.py)；[来源 2](https://www.projectoberon.net/) |
| `pcgeos` | PC/GEOS 2+ BSWF font files, font metadata, point-size strikes, metrics, bitmaps and kerning. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/pcgeos.py)；[来源 2](https://github.com/bluewaysw/pcgeos) |
| `pcr` | Optiks PCR fixed-width 8-pixel font header and per-glyph rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/optiks.py) |
| `pilfont` | Pillow PILfont .pil metrics plus PBM/PNG glyph atlas. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/image/pilfont.py)；[来源 2](https://github.com/python-pillow/Pillow/blob/main/src/PIL/FontFile.py) |
| `plus3dos` | ZX Spectrum +3DOS font file header, length/type/load metadata, checksum and 8x8 glyph payload. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/plus3dos.py) |
| `psf2txt` | psftools textual PSF representation with glyph index, dimensions, Unicode aliases/sequences and bitmap rows. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/psf2txt.py) |
| `polyprint` | PolyPrint printer bitmap fonts and proportional glyph packing as documented by its utility maintainer. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printer/polyprint.py)；[来源 2](https://www.seasip.info/ZX/polyprint.html) |
| `prebuilt` | Adobe prebuilt BEPF/LEPF files: endian variants, character names, strikes and grayscale glyph data. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/prebuilt.py)；[来源 2](https://github.com/johnsonjh/NeXTDPS/blob/master/fonts-21/bin/prebuiltformat.h) |
| `printshop` | The Print Shop PNF printer fonts, native raster/width arrangement. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printshop.py) |
| `pxl` | TeX PXL 1001/1002 font bitmaps, 128-entry directory, TFM widths, design size and dpi; optional TFM metrics. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/metafont/pxl.py)；[来源 2](https://tug.org/TUGboat/tb02-3/tb04fuchspxl.pdf) |
| `riscos` | RISC OS older x90y45 and newer Font bitmap files, strike indexes, metrics and grayscale packing. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/riscos.py)；[来源 2](https://gitlab.riscosopen.org/RiscOS/Sources/SystemRes/Fonts) |
| `sfont` | SDL SFont marker-row PNG atlas and Kreative RFont extension, explicit code sequence and proportional spacing. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/image/sfont.py)；[来源 2](https://github.com/karlb/sfont) |
| `signum` | Signum! editor E24 and printer P9/P24/L30 raster character sets. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/signum.py)；[来源 2](https://sdo.dseiler.eu/formats/eset) |
| `symbos` | SymbOS FNT variable-width bitmap font header and glyph byte offsets. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/symbos.py)；[来源 2](https://www.symbos.org/) |
| `tasprint` | TasPrint screen/printer character sets, widths up to 16, 16-row glyphs and multi-font sequences; optional AMSDOS envelope. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/printer/tasprint.py)；[来源 2](https://www.seasip.info/ZX/tasprint.html) |
| `vtfont` | FreeBSD VFNT0002 console fonts: glyph table and normal/bold left/right Unicode maps. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/vtfont.py)；[来源 2](https://github.com/freebsd/freebsd-src/blob/main/sys/sys/font.h) |
| `writeon` | Hercules Write On WOF bitmaps and native header, cells in 8x14 multiples. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/writeon.py) |
| `wsfont` | NetBSD WSF binary fonts with geometry, stride, encoding, byte/bit order; c/netbsd represents the same kernel font structure in C. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/wsfont.py)；[来源 2](https://github.com/NetBSD/src/blob/trunk/sys/dev/wscons/wsconsio.h) |
| `wyse` | Wyse-60 downloadable soft-font escape sequences, 8x16 glyph rows and encoded character selectors. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/softfont/wyse.py)；[来源 2](http://bitsavers.org/pdf/wyse/WY-60/880261-01A_WY-60_Programmers_Guide_Jan87.pdf) |
| `xbin` | XBIN font section with XBIN header, 256/512 glyph flags and optional palette/screen data. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/xbin.py)；[来源 2](https://web.archive.org/web/20120204063040/http://www.acid.org/info/xbin/x_spec.htm) |
| `xerox` | Xerox Alto CONVERT .al, BITBLT .strike/.ks and PrePress .ac variants, each with native directory/metrics/raster packing. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/xerox.py)；[来源 2](http://www.bitsavers.org/pdf/xerox/alto/printing/AltoFontFormats_Oct1980.pdf) |
| `zapfont` | RISC OS ZapFont old fixed-cell form and ZapRedraw UCS chunk/range form. | [来源 1](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/zapfont.py)；[来源 2](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/zapredraw.py) |
| `rockbox` | Rockbox RB11/RB12 binary bitmap fonts, glyph/offset/width tables and native row packing. | [来源 1](https://www.rockbox.org/wiki/FontFormat)；[来源 2](https://github.com/Rockbox/rockbox/blob/master/tools/convbdf.c) |
| `u8m` | Kreative U8/M UTF-8 font format: bitmap glyphs, mapping tables and platform maps. | [来源 1](https://github.com/kreativekorp/u8m/blob/master/U8MFontFormat.pdf)；[来源 2](https://github.com/kreativekorp/u8m) |
| `cybiko` | Cybiko device font files including native bitmap and compression variants identified by the original SDK documentation. | [来源 1](https://web.archive.org/web/20010701031854/http://groups.yahoo.com/group/CybikoDev/files/Pazera/font.txt)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/CybikoBitmapFontImporter.java) |
| `playdate` | Playdate source FNT metrics/kerning plus PNG image table fonts; native compiled font container is a separately tracked variant. | [来源 1](https://sdk.play.date/3.1.2/Inside%20Playdate.html#_fonts)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/PlaydateBitmapFontImporter.java) |
| `hmzk` | Mi Band 2 HMZK device bitmap font header, code directory and glyph data. | [来源 1](https://freeyourgadget.codeberg.page/gadgetbridge_org/internals/specifics/mi-band-2/)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/HMZKBitmapFontImporter.java) |
| `sbf` | Sabriel Bitmap Font binary headers, glyph maps, bitmap data and metrics. | [来源 1](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/README.md)；[来源 2](https://github.com/kreativekorp/bitsnpicas/blob/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/SBFBitmapFontImporter.java) |
| `qpf` | Qt/Embedded 2/3 legacy QPF prerendered bitmap fonts: glyph tree, metrics and mono/grayscale rasters. | [来源 1](https://doc.qt.io/Archives/qt-4.7/qt-embedded-fonts.html)；[来源 2](https://github.com/qt/qt/blob/0a2f2382541424726168804be2c90b91381608c6/src/gui/text/qfontengine_qws.cpp) |
| `qpf2` | Qt for Embedded Linux QPF2 prerendered fonts: tagged header, cmap/glyph maps, raster glyph blocks and optional TrueType linkage. | [来源 1](https://doc.qt.io/Archives/qt-4.7/qt-embedded-fonts.html)；[来源 2](https://github.com/qt/qt/blob/0a2f2382541424726168804be2c90b91381608c6/src/gui/text/qfontengine_qpf_p.h) |
| `ssfn` | Scalable Screen Font 2 binary SFN bitmap/pixmap fragments, glyph references and mappings, plus ASCII source form. | [来源 1](https://gitlab.com/bztsrc/scalable-font2/-/blob/master/docs/sfn_format.md)；[来源 2](https://gitlab.com/bztsrc/scalable-font2/-/blob/master/docs/asc_format.md) |
| `vlw` | Processing PFont VLW versions with per-glyph metrics, Gray8 bitmaps and Java data-stream strings. | [来源 1](https://processing.org/reference/loadFont_)；[来源 2](https://github.com/processing/processing4/blob/main/core/src/processing/core/PFont.java) |
| `pfr` | Portable Font Resource bitmap strikes, character maps and packed bitmap data in bitmap-bearing PFR files. | [来源 1](https://freetype.org/freetype2/docs/reference/ft2-pfr_fonts.html)；[来源 2](https://github.com/freetype/freetype/blob/42e28aa7adfad33d496987a408438b726a82e645/src/pfr/pfrsbit.c) |

已有 id 的补充变体也属于本阶段：韩文 8×4×4 全音节合成；Palm afnx/GrayFont；
C 的 U8g2/LVGL、NetBSD/U8glib/U8x8 与 LVGL binary；逐字形 imageset；
NFNT 的 BinHex、IIgs 的 BinSCII；BMFont JSON；raw 的整条 strike、列优先及
非字节行对齐布局。它们不另增 id，详细范围见普查清单的 `existing_expansions`。
