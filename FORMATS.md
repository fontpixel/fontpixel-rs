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
