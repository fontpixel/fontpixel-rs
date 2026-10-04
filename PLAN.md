# fontpixel：点阵字体的 Rust 库与命令行工具 — 设计与实施计划

版本 2026-10-02。作者：Tom Chen（项目所有者）与 Claude（起草）。执行：Codex（dot）。

本文件是规格加实施计划。格式清单在 `FORMATS.md`，执行流程（怎么分批、怎么交付）在 `DOT-README.md`。
三份文件冲突时，以 `DOT-README.md` 的流程规定和本文件的技术规定为准，`FORMATS.md` 的格式细节以各格式的公开规格为准。

2026-10-04 追加规格：[拼合功能完整原文](docs/plan-addendum-merge.md)。追加工作项只在第 6、8、9 阶段执行，不打断当前阶段；现有阶段顺序、完成标准和工作项保留。第 9 阶段的纯客户端拼合网页是 §1.3「不做图形界面」的明确追加范围，收录站仓库修改与部署仍不在本任务内。

---

## 1. 目标

做一个 Rust 包 **`fontpixel`**：读、写、编辑、渲染、互转**一切点阵字体格式**，并能把点阵字体矢量化成 OpenType（TTF、OTF、WOFF、WOFF2，可带内嵌点阵、可带 `locl`），又快又准。

它要同时服务三方：

1. **Tong Pixel Sans（通格像素黑体）**（`reference/tong-pixel-sans/`）：取代 `tools/export-rs`（PCF、OTB、方点/圆点 TTF/OTF/WOFF2、四地区合一的 `locl`）和首页构建里 fontTools 做的子集化（`site/build.py` 的 `make_subset`）。
2. **FontPixel.com 收录站**（`reference/fontpixel/`）：取代 `pipeline/opf/parsers/`（BDF、PCF、字符集）、`pipeline/opf/ingest/` 里的各种格式导入器、`pipeline/rust/opf_vector`（pyo3 矢量化）和 `opf/downloads.py`、`opf/ttfexport.py` 里 fontTools 做的导出。收录站一次要处理几百个字体，**批量处理的速度**最重要。
3. **bdfparser（Python）与 bdfparser-js**（`reference/bdfparser/`、`reference/bdfparser-js/`）：两者遵循同一规格 `SPEC-v3.md`、输出逐字节一致。`fontpixel` 是这套规格的**第三个实现**：它的 bdfparser 兼容层必须通过同一套跨语言逐字节测试。以后 bdfparser 可能把 `fontpixel` 当作可选的加速引擎（第 7、8 阶段先做绑定并测速，换不换由用户决定）。

### 1.1 名字

- crates.io：`fontpixel`（库）、`fontpixel-cli`（提供命令 `fontpixel`）；以后 PyPI、npm 也叫 `fontpixel`（2026-10-02 查过三处都没人用）。
- 仓库目录 `fontpixel-rs/`（GitHub 上会是 `fontpixel/fontpixel-rs`，因为 `fontpixel/fontpixel` 是收录站）。
- 许可证 MIT（与 bdfparser、收录站代码一致），版权 `Tom Chen`。

### 1.2 成功标准（全部达到才算完成）

1. `FORMATS.md` 里每种格式都能读、能写；每种都有往返测试，有对照工具的都做交叉验证（§9）。
2. bdfparser 兼容层通过跨语言逐字节测试：与 `reference/bdfparser` 的 Python 实现在 SPEC-v3 覆盖的全部功能上结果一致（§9.2）。
3. 能完全复现通格的导出：同样的输入，生成的 PCF、OTB、TTF、OTF、WOFF2（方点、圆点，比例、等宽，带 `locl`）与 `reference/tong-pixel-sans/tools/export-rs` 的结果**字形级等价**（§9.4）。
4. 能完全复现收录站的导入和导出（§9.5）。
5. 达到 §10 的性能目标，并交一份与 bdfparser（Python）、bdfparser-js、fontTools、通格 `export-rs` 对比的测速报告。
6. 任何输入都不会让程序崩溃（panic）或耗尽内存：所有读取器都做过模糊测试（§9.6）。
7. 命令行工具和 Rust API 有完整文档和示例。

### 1.3 不做的事

- 不做普通（非像素）矢量字体的设计或编辑；矢量字体只在“像素化矢量字体退回点阵”和“点阵矢量化”两个方向上处理。
- 不做任何游戏专用的私有字体格式（如魔法门 VI–VIII 与英雄无敌 III 的 FNT、匕首雨的 FNT、辐射的 FON/AAF、暴雪游戏的 FNT、Infinity 引擎的 BAM、ZDoom 的 FON1/FON2 等；用户 2026-10-02 决定）。第 5 阶段普查也只收通用的点阵字体格式；游戏和引擎通用的格式（如 BMFont）照常支持。数据模型和 API 要便于以后另写插件支持这类格式。
- 不做图形界面。（编辑器另有项目；这里只提供 API。）
- 不把通格、收录站的代码改成用 `fontpixel`：集成由用户和 Claude 在本机做。dot 只提供库、CLI、绑定和迁移说明（§11）。

---

## 2. 参考资料（都在本包 `reference/` 里）

| 路径 | 是什么 | 用来做什么 |
|---|---|---|
| `reference/bdfparser/` | bdfparser Python v3（未发布的最新版），`docs/SPEC-v3.md` 是规格，`tests/` 有测试和金样 | 兼容层的行为基准；Python 实现是“参考实现”（§11.6 写明 JS 必须跟随 Python） |
| `reference/bdfparser-js/` | bdfparser TypeScript v3；`tools/parity/` 是跨语言差分测试 | 测试方法照搬；第 8 阶段 WASM 绑定的 API 基准 |
| `reference/fontpixel/pipeline/` | 收录站的 Python 流水线和 Rust 扩展 `rust/opf_vector`；`tests/` 是它的测试 | 各种导入器的行为基准；矢量化（`vectorize.py`）、内嵌点阵 TTF（`ttfexport.py`）、下载文件（`downloads.py`）的输出要求 |
| `reference/fontpixel/ingest/manifest.toml` | 收录站 513 个字体的导入配置 | 看每种导入器实际怎么被调用、有哪些参数 |
| `reference/tong-pixel-sans/tools/export-rs/` | 通格的 Rust 导出器（`outlines.rs` 描轮廓、`cff.rs`、`otb.rs`、`main.rs`、`cli.rs`）及其测试、Python 参考实现 `python_reference.py` | 矢量化、CFF、OTB、`locl`、WOFF2 的已验证实现，可以直接移植（同一作者，MIT） |
| `reference/tong-pixel-sans/tools/build.py`、`docs/building.md`、`docs/format.md` | 通格怎么从源文件生成四个地区的 BDF | 理解 `locl` 的地区借用规则和度量 |
| `reference/tong-pixel-sans/site/build.py` | 通格首页构建（字体子集化） | 子集化功能的需求 |

`reference/` 是只读资料，**不要改**，也不要整个复制进新仓库（需要的测试金样按 §9.1 的规则拷进 `fontpixel-rs/tests/data/`，并注明来源）。

---

## 3. 总体结构

一个 Cargo 工作区 `fontpixel-rs/`：

```
fontpixel-rs/
  Cargo.toml                 工作区
  crates/
    fontpixel/               库（发布为 crate `fontpixel`）
      src/
        lib.rs               公共 API：read / write / detect / Font / FontSet …
        model/               数据模型（§4）
        bitmap/              Bitmap 及其操作（裁剪、拼接、阴影、发光、缩放、旋转…）
        charset/             字符集表（§6）
        render/              文字排版与渲染（§7）
        outline/             描轮廓、圆点、栅格化（§8.1）
        sfnt/                OpenType 读写：表、cmap、name、OS/2、EBDT/EBLC、CFF、glyf、GSUB(locl)、子集化
        woff/                WOFF1、WOFF2 编解码
        formats/<id>.rs      每种格式一个模块（FORMATS.md 的 id）
        compat/bdfparser/    bdfparser SPEC-v3 兼容层（§5.5）
        report.rs            转换报告（§5.4）
    fontpixel-cli/           命令行工具 `fontpixel`（§5.6）
    fontpixel-py/            第 7 阶段：Python 绑定（pyo3，abi3）
    fontpixel-wasm/          第 8 阶段：WASM 绑定（wasm-bindgen）
  tests/
    data/                    小型测试字体和金样（注明来源与许可证，见 §9.1）
    parity/                  与 bdfparser 的差分测试（§9.2）
    crosscheck/              与 monobit、fontTools、FreeType、HarfBuzz 等的交叉验证脚本（§9.3）
  benches/                   criterion 基准与端到端测速脚本（§10）
  fuzz/                      cargo-fuzz 目标（§9.6）
  docs/
    formats/<id>.md          每种格式：依据、字段对照、我们怎么处理有歧义的地方、已知限制
    decisions.md             设计决定记录（日期、问题、选择、理由）
    migration-tong.md        通格改用 fontpixel 的说明（§11）
    migration-fontpixel.md   收录站改用 fontpixel 的说明（§11）
  README.md、CHANGELOG.md、LICENSE、THIRD_PARTY_NOTICES.md
```

- Rust：最新稳定版，edition 2024，`#![forbid(unsafe_code)]`（绑定 crate 例外，需要时局部允许并说明）。
- 每种格式和重依赖都放在 Cargo feature 后面（如 `woff2`（brotli）、`image`（PNG/BMP/GIF 解码）、`cff`、`fea`），默认开启常用的；`fontpixel` 只开 BDF/PCF 时编译要快、依赖要少。
- 依赖只用许可证兼容 MIT 的 crate（MIT、Apache-2.0、BSD、Zlib、Unicode）。推荐：`read-fonts`/`write-fonts`/`skrifa`（fontations）、`fea-rs`（如用）、`brotli`、`flate2`、`png`/`image`（只开需要的编解码器）、`quick-xml`、`clap`、`rayon`、`serde`。用 `cargo deny` 检查许可证。
- 不抄 GPL 代码（FontForge、GRUB、FreeType 的 GPL 部分等），只参考公开的格式文档；MIT/BSD/Apache 代码（如 monobit、X11 源码）可以参考或移植，在 `THIRD_PARTY_NOTICES.md` 注明。

---

## 4. 数据模型

目标：一个模型能无损表示 BDF 的全部内容，又能容纳其他格式的特有信息（多尺寸、灰度、竖排度量、多码位对应一个字形、未编码字形）。

### 4.1 `Bitmap`

- 宽、高（可为 0）和像素。主表示为 **1 位**：按行存，每行按位紧凑排列（MSB 在左），行对齐到字节。另有 `Bitmap<Gray8>`（每像素 1 字节，0–255）给灰度来源用（kbitx 灰度、BMFont 的 PNG、`CBDT`、`EBDT` 的 2/4/8 位深度）。
- 1 位与灰度互转要显式给阈值（默认 ≥ 128 为墨，与收录站 `kbitx.py` 一致）；带调色板的来源（如 BMFont 的多通道页面）映射到灰度值，规则写进对应的 `docs/formats/<id>.md`。
- 操作（与 SPEC-v3 §2.5 对齐）：裁剪、扩展、拼接（水平、竖直）、叠加（各种合成模式）、阴影、发光、放大（整数倍）、翻转、旋转 90°、反色、取墨迹边界、比较、与字符串互转（`#`/`.` 等）、与字节流互转（各种位序、行填充）。热点路径（拼接、叠加、渲染）要按字或按字节做位运算，不逐像素循环。

### 4.2 `Glyph`

- 码位：一个字形可以对应 0 个（未编码）、1 个或多个码位；码位可以是 Unicode，也可以是某个字符集里的码（见 §6）。另存可选的 Unicode 序列（PSF 的 Unicode 表、组合字符）。
- 名字（可选，如 BDF 的 `STARTCHAR`、PostScript 名）。
- 度量：横排步进（DWIDTH x, y）、可缩放宽度（SWIDTH）、墨迹框（BBX：宽、高、x 偏移、y 偏移）、竖排度量（DWIDTH1、SWIDTH1、VVECTOR），以及其他格式的左右间距（如 FNT v3 的 ABC 宽度、BMFont 的 xoffset/xadvance）。
- 点阵：`Bitmap`（只存墨迹框内的部分；也能取出按字格对齐的整格点阵）。
- 每字的附加属性（有序键值，保留读入时的原样，供原格式写回）。

### 4.3 `Font`（一个 strike：一个尺寸、一个样式）

- 有序的字形表 + 码位到字形的映射（`cmap`）。查找、遍历按码位或按文件顺序都要快。
- 全局度量：像素大小（pixel size / ppem）、point size、分辨率（x、y dpi）、ascent、descent、字体墨迹框、行距、下划线位置和粗细、默认字形、默认字符、空格宽度、比例/等宽/字符格类型（P/M/C）。
- 名字信息：家族名、样式名、完整名、PostScript 名、版本、版权、许可证、厂商（foundry）、XLFD 字段。
- 字符集：Unicode 或其他（`CHARSET_REGISTRY`、`CHARSET_ENCODING`，或 FNT 的代码页、HBF 的码段）。
- 原格式的全部属性（有序键值，如 BDF 的属性、PCF 的属性表、FNT 头的字段），未被模型吸收的原样保留，写回原格式时用。

### 4.4 `FontSet`（一个文件里的多个字体）

FON、TTC、OTB 多 strike、Mac 手提箱、CPI、Amiga 等一个文件里有多个尺寸或样式。读取一律返回 `FontSet`；只含一个时 API 有便捷方法直接取 `Font`。写入格式不支持多个时，CLI 可选“只写第 n 个”或“每个写成单独文件”。

### 4.5 `Variants`（地区字形与 `locl`）

给通格这类“一个文件装四个地区”的需求：一个基础 `Font`（默认，如简体）+ 若干 `(语言标签, Font)`。写 OpenType 时，基础字体的字形为默认，各语言与基础不同的字形加成替代字形，生成 `GSUB` 的 `locl` 单替换查找：
- 语言标签用 OpenType 语言系统标签（`ZHT `、`ZHH `、`JAN `、`KOR ` 等），API 也接受 BCP 47（`zh-Hant`、`zh-HK`、`ja`、`ko`）并按 OpenType 的对照表转换；
- 每个语言挂在哪些脚本下可配置（通格：`DFLT hani kana hang bopo`）；
- 另外支持“按脚本切换”的替代（通格：在 `latn grek cyrl` 下改用西文字体的窄引号和省略号），即 `Variants` 里也可以有 `(脚本, Font)`；
- 完全相同的替代字形不重复存储（按点阵 + 度量去重）。
具体规则照通格 `export-rs` 的 `main.rs`/`prepare.py` 和 `python_reference.py` 的 `plan()`、`feature_text()`；要求两者生成的 `locl` 在 HarfBuzz 下塑形结果相同（§9.4）。

---

## 5. 功能与接口

### 5.1 读

```rust
let loaded = fontpixel::read(&bytes, &ReadOptions { format: None /*自动判断*/, ..Default::default() })?;
let set: FontSet = loaded.fonts;          // 一个文件可含多个字体
let report: Report = loaded.report;       // 警告：修正了什么、忽略了什么
let font = fontpixel::read_path("a.bdf")?.into_single()?;
```

- 自动判断格式：先看魔数和结构（gzip、`STARTFONT`、`\x01fcp`、`MZ`/`NE`/`PE`、sfnt 标签 `\0\1\0\0`/`true`/`OTTO`/`ttcf`/`wOFF`/`wOF2`、`PSF` 魔数、`FONTX2`、`yaff` 等），再看扩展名；`.fnt` 这种多义扩展名按结构区分 Windows FNT、BMFont（文本/XML/二进制）、GEM、FONTX2、原始点阵。判断不了时报错并列出候选，让用户用 `--from` 指定。
- gzip 自动解压（`.pcf.gz`、`.bdf.gz`、`.psf.gz`），要防解压炸弹（大小上限可配）。
- 容错：照 bdfparser §3、§11 和收录站各读取器的容错规则；修正过的地方写进报告。

### 5.2 写

```rust
let bytes = fontpixel::write(&font_or_set, Format::Pcf, &WriteOptions { ..Default::default() })?;
```

- 每种格式的写选项（如 PCF 的字节序、位序、填充；FNT 的代码页、版本；WOFF2 的压缩质量；TTF 的轮廓形状、点大小、每像素单位数、是否内嵌点阵）都是强类型的结构体，CLI 用同名参数。
- 默认输出要稳定（同样输入、同样选项 → 字节完全相同），不写时间戳（必要时可用选项或 `SOURCE_DATE_EPOCH`）。

### 5.3 OpenType 输出（矢量化与内嵌点阵）

照通格 `export-rs` 和收录站 `vectorize.py`、`ttfexport.py`、`downloads.py` 的要求，统一成一套：

- 单位：`units_per_pixel`（通格 100）与 `upm`（通格 1400 = 14 × 100）可配；ascent、descent、行距从字体度量来。
- 轮廓形状：
  - `square`：方点，相邻像素合并成一个轮廓，包括洞；只在角上相接的像素不连在一起；只在拐点放点（SPEC-v3 §7.1；通格 `outlines.rs`）；
  - `square-separate`：每个像素一个方块（SPEC-v3 §7.2，`dotsize` < 1 时用）；
  - `dot`：圆点，直径可配（通格 90/100）；TrueType 用一个圆形字形加复合字形引用，CFF 用一个全局子程序；
  - `dotsize`、`shape` 与 bdfparser 的 `write_ttf` 参数一致（兼容模式下逐字节一致）。
- 内嵌点阵：任意 sfnt 输出都可加 `EBDT`/`EBLC`（一个或多个 strike），`otb` 则只有点阵。编码格式要小：同尺寸字形多时用共享度量的索引格式（如 EBLC index 2/5 + EBDT image 5），其余用逐字度量的位对齐格式（image 2/7）；`EBSC` 可选。
- 表：`head`、`hhea`、`maxp`、`OS/2`（含 Unicode 范围和代码页范围的计算）、`name`（家族、样式、完整名、PostScript 名、版本、版权、许可证与链接）、`cmap`（format 4 + format 12，不含代理对）、`post`（等宽标志）、`hmtx`、`gasp`、可选 `vhea`/`vmtx`；`glyf`/`loca` 或 `CFF `；`GSUB`（`locl`）。等宽字体要让 `fc-query` 识别为 mono 或 dual（通格等宽有半角、全角两种宽度）。
- WOFF2：方点建议从 CFF 版生成（通格实测小约五分之一），圆点从 TrueType 版生成——做成选项 `--woff2-source cff|ttf|auto`，`auto` 照此规则。
- 子集化：按码位集合或文本取子集，保留相关的 `locl` 替换、`cmap`、`name`，输出任意 sfnt/WOFF/WOFF2（取代通格 `site/build.py` 与收录站 `mergesubsets.py` 里 fontTools 的子集化）。

### 5.4 转换报告

每次读写都产生 `Report`：结构化的条目（级别、代码、字形或码位、说明），CLI 可以输出人读的文字或 JSON。
- 读：修正了什么（如行长不对、重复编码）。
- 写：目标格式装不下而丢掉或改动的东西（如 PCF 写不进 U+FFFF 以上的码位、FNT 只能 256 个、PSF1 只能宽 8、HZK 只能 GB2312、灰度转 1 位）。
- `--strict`：报告里有丢失信息的条目时不写文件、返回错误码。

### 5.5 bdfparser 兼容层（`compat::bdfparser`）

- 完整实现 SPEC-v3 的 `Font`、`Glyph`、`Bitmap` 与全部方法（§2），包括 v2 的查询方法、v3 的编辑方法、`draw`（任意书写方向）、`drawall`、`tobdf`、PCF 读写、`write_ttf`/`read_ttf`、`tosvg`/`tosvgfont`、字符集转换、警告与错误类型（§2.9）。
- 行为与 Python 参考实现完全一致（包括边界情况、错误信息的类别）。为此建一个“差分运行器”二进制 `fontpixel-parity`：读 `tests/parity` 生成的 JSON 操作脚本，执行，按 bdfparser-js `tools/parity` 的规范序列化结果（§9.2）。
- 兼容层建在核心模型上，不复制一套独立实现。

### 5.6 命令行 `fontpixel`

```
fontpixel convert IN... -o OUT [--from F] [--to F] [格式选项...] [--strict] [--report json|text]
fontpixel info IN [--json]                      # 格式、字体数、字形数、度量、字符集、属性、覆盖的 Unicode 区块
fontpixel preview IN TEXT [--direction lr|rl|tb|bt...] [--chars '#.']
fontpixel render IN TEXT -o out.png|out.svg [--scale N] [--color ...]
fontpixel vectorize IN -o out.ttf|otf|woff|woff2 [--shape square|square-separate|dot] [--dotsize F] [--ppu N] [--embed-bitmaps]
fontpixel variants --base SC.bdf --lang zh-Hant=TC.bdf --lang ja=JP.bdf --lang ko=KR.bdf --script latn,grek,cyrl=Latin.bdf -o out.woff2 [...]
fontpixel subset IN -o OUT --text '...' | --unicodes U+4E00-9FFF,... | --text-file F
fontpixel diff A B [--json]                     # 逐字形比较点阵与度量
fontpixel validate IN                           # 结构检查；sfnt 另用内置检查（并在测试里对照 OTS）
fontpixel charset IN -o OUT --to gb2312|big5|cp1252|...
fontpixel formats                               # 列出全部格式、能读能写、扩展名
fontpixel batch JOBS.toml [--jobs N]            # 批量任务并行执行（收录站、通格导出用）
```

- 多个输入：`convert` 可把多个文件合并成一个 `FontSet`（如多尺寸写成一个 FON 或 OTB）；`--each` 则逐个转换。
- `batch`：一个 TOML 文件列出许多转换任务（输入、输出、选项），用线程池并行；整体失败与否、每个任务的报告都写成 JSON。通格导出一次要 84 个文件、收录站几百个字体，都用它。
- 退出码：0 成功；1 失败；2 用法错误；3 `--strict` 下有信息丢失。

---

## 6. 字符集

- 与 SPEC-v3 §4、§11.1 一致：**逐表精确**（不能用近似映射），表由 Python 的编解码器生成。bdfparser-js 已经有生成脚本 `tools/gen_charsets.py`，照它生成 Rust 表（生成的源文件进仓库，生成脚本也进仓库）。
- 需要的字符集：SPEC-v3 列出的全部（ISO 8859 系列、Windows 代码页、cp437/850/852/866/874/932/936/949/950、johab、KOI8、GB2312、GBK、GB18030、Big5、Shift-JIS、EUC-JP、EUC-KR、ISO 2022 等），加上格式需要的（FNT 的 `dfCharSet` 对应代码页、HBF 的码段、HZK 的 GB2312 区位、FONTX2 的 Shift-JIS）。
- 大表（CJK）按需编译（feature `cjk-charsets`），并做成紧凑表（区段 + 数组），查找 O(1) 或 O(log n)。

---

## 7. 渲染

- 照 SPEC-v3 的 `Font.draw` 与 `drawall`：任意书写方向（左右、右左、上下、下上及组合）、字距、行距、缺字处理、`mode`（按步进或按墨迹拼接）。
- 输出：`Bitmap`、PNG、SVG、终端字符画（`preview`）。
- 速度：渲染一屏文字（如 80×25 字符）在微秒到毫秒级；不为每个字形分配内存。

---

## 8. 各部分的实现要点

### 8.1 描轮廓与栅格化

- 方点合并轮廓：移植通格 `outlines.rs`（已与 Python 版逐像素核对）；与 SPEC-v3 §7.1 的点序、起点、方向规则对齐（兼容模式逐字节一致）。
- 栅格化（矢量 → 点阵）：用于读取像素化矢量字体（`vector-raster`）和 `read_ttf`。实现网格检测（所有坐标是否落在 `upem / ppem` 的整数倍上，ppem 4–64），然后按非零绕数精确填充像素中心；与 SPEC-v3 §7.4 一致。不依赖 FreeType。

### 8.2 sfnt 读写

- 写：可以用 `write-fonts` 生成各表，也可以自己写（字段少、格式简单的表自己写更快更可控）；要求输出稳定、校验和正确、表按标签排序、4 字节对齐。
- `CFF `：照通格 `cff.rs` 自写最小编码器；字形全是直线（`rlineto`/`hlineto`/`vlineto`），圆点用全局子程序；`read-fonts` 能读回、OTS 能通过。
- 读：用 `read-fonts`/`skrifa` 解析各表和轮廓；`EBDT`/`EBLC`、`bdat`/`bloc`、`CBDT`/`CBLC`、`sbix` 自己解析。
- `GSUB`：直接构造单替换查找和 `locl` 特性（不必经过 FEA）；若用 `fea-rs`，则要保证结果与直接构造的一致。

### 8.3 WOFF2

- 编码：`glyf`/`loca` 变换必须做（这是 WOFF2 体积小的主要原因）；brotli 质量默认 11，可调。
- 速度：通格每个圆点字体的 WOFF2 现在要 30 秒（brotli 质量 11），是总时间的大头。研究并实现可行的加速：多个字体并行（必须）、单个字体内分块并行压缩（若 brotli 实现支持且结果仍是合法 WOFF2）、可选较低质量（`--woff2-quality`）。把各方案的速度和体积对比写进测速报告。
- 解码也要实现（读 WOFF2、验证往返）。

### 8.4 各格式

每种格式一个模块，接口统一（`detect`、`read`、`write`、各自的选项结构体）。实现前先写 `docs/formats/<id>.md`：规格出处、字段对照表、与内部模型的映射、歧义与我们的选择、写入时会丢失什么。

---

## 9. 测试与验证

### 9.1 测试数据规则

- 进仓库的测试字体只能是自由许可（OFL、MIT、BSD、CC0、公有领域等）且小的；每个在 `tests/data/SOURCES.md` 注明来源、许可证、用途。可以用本包 `samples/` 里的（`samples/fontpixel-catalogue/*` 各目录里有许可证文件；`samples/tong-pixel-sans/` 是 OFL）。
- 能自己生成的测试数据尽量生成（随机字体、用本工具写出再读回、用 monobit 生成）。
- 需要从网上下载的样本（例如某格式本包没有样本）：只用自由许可的，下载脚本进仓库（`tests/data/fetch.sh`，带 SHA-256 校验），文件本身不进仓库。

### 9.2 与 bdfparser 的差分测试

- 移植 `reference/bdfparser-js/tools/parity`：生成随机字体和操作脚本（`gen_cases.py`），分别交给 Python 参考实现（`reference/bdfparser/src`，纯 Python 无依赖）和 `fontpixel-parity` 执行，规范化序列化后逐项比较；也跑它的语料阶段（`--skip-corpus` 以外的全部）。
- 要求：默认规模（与 bdfparser-js 的 `run_all.py` 默认相同）全部一致；`known_issues.py` 里列的已知差异要逐条核对是否适用于 Rust，适用的照样标记，不适用的必须一致。
- 另外直接用 bdfparser 的金样（`tests/fonts/golden/`、各测试里的期望输出）做 Rust 单元测试。

### 9.3 交叉验证（对照外部工具，写成可重复运行的脚本）

| 对照工具 | 验证什么 |
|---|---|
| monobit（MIT） | 它支持的每种格式：我们写的它能读、它写的我们能读，点阵与度量一致 |
| fontTools（`ttx`） | 我们写的 sfnt/WOFF/WOFF2 能被完整解析 |
| OTS（`ots-sanitize`，浏览器用的字体检查器） | 网页用的 TTF/OTF/WOFF/WOFF2 全部通过 |
| FreeType（`freetype-py`） | TTF/OTF/OTB 在原生像素大小下渲染结果与源点阵逐像素相同（方点）；内嵌点阵 strike 与源点阵逐像素相同 |
| HarfBuzz（`uharfbuzz` 或 `hb-shape`） | `locl`：同一段文字在 `zh-Hans`、`zh-Hant`、`zh-HK`、`ja`、`ko`、`en` 下的塑形结果与通格参考输出一致 |
| `bdftopcf`（如可安装） | 我们写的 PCF 与 `bdftopcf` 从同一 BDF 生成的 PCF 读回后完全相同 |
| `fc-query` | 等宽、像素大小、家族名等被 fontconfig 正确识别 |

### 9.4 通格等价测试

- 用 `samples/tong-pixel-sans/` 的 BDF（Large 的 SC、TC、JP、KR、Latin，等宽 Latin，以及 Small Latin）：
  - 照 `reference/tong-pixel-sans/tools/export-rs/` 的做法，用 `fontpixel variants` 生成方点和圆点的 TTF、OTF、WOFF2，与 `export-rs` 的输出比较：cmap、字形数、度量（hmtx、hhea、OS/2 关键字段、name）一致；每个字形的轮廓栅格化后与源点阵逐像素一致；HarfBuzz 的 `locl` 塑形结果一致。
  - 比较 `export-rs` 的 OTB 与我们的 OTB（FreeType 逐像素一致）；PCF 同上。
- 运行 `export-rs` 需要它的 Python 准备脚本（`prepare.py`、`tools/build.py`）；若环境不便，至少用 `python_reference.py`（fontTools）对一部分字体生成参考输出。

### 9.5 收录站等价测试

- `reference/fontpixel/pipeline/tests/` 里的各导入器测试（`test_bdf.py`、`test_pcf.py`、`test_otb.py`、`test_kbitx.py`、`test_pebble.py`、`test_header8x8.py`、`test_spritesheet.py`、`test_rasterize.py`、`test_vectorize*.py`、`test_ttfexport.py`、`test_downloads.py` 等）：把其中的期望结果移植成 Rust 测试，或写适配脚本让这些 Python 测试直接调用 `fontpixel`（CLI 或第 7 阶段的 Python 绑定）并通过。
- 用 `samples/fontpixel-catalogue/` 的字体做 BDF ↔ PCF ↔ OTB ↔ TTF（内嵌点阵）往返。

### 9.6 健壮性

- 每个读取器一个 `cargo-fuzz` 目标；每个至少跑到 10 分钟以上没有崩溃（交付时报告各自的执行次数和覆盖率）。
- 资源上限：解压大小、字形数、单字形像素数、表大小等都有上限（可配），超限返回错误。
- 属性测试（`proptest`）：随机字体经无损格式（BDF、PCF、yaff、OTB、PSF2…）往返后完全相同。
- `cargo clippy -- -D warnings`、`cargo fmt --check`、`cargo deny check` 全部通过。

---

## 10. 性能目标与测速

测试机按 dot 的实际环境；报告里写明 CPU、核数、内存。时间是墙钟时间，取 5 次中位数。

| 任务 | 目标 |
|---|---|
| 解析 `TongPixelSans14Large-SC.bdf`（5.7 MB，38,653 个字形） | ≤ 100 ms（单线程） |
| 同一文件 → PCF / OTB / yaff | 各 ≤ 200 ms |
| 同一文件 → 方点 TTF（含 `glyf`） | ≤ 1 s |
| 通格整套导出：两种尺寸 × 方点/圆点 × 比例/等宽 = 8 个矢量字体（各 TTF + OTF + WOFF2，带 `locl`）+ 20 个 PCF + 20 个 OTB | 默认设置 ≤ 40 s；报告各部分耗时，WOFF2 加速方案（§8.3）各自的时间与体积 |
| 收录站：`samples/fontpixel-catalogue/` 全部字体批量转出下载文件（`fontpixel batch`） | 报告总时间，并与收录站现有 Python 流程对比 |

测速报告（`benches/REPORT.md`）还要包括：
- 与 bdfparser（Python）、bdfparser-js（Node）在同样操作上的对比：解析、写 BDF、写 PCF、写 TTF、渲染一段文字、`drawall`、裁剪/拼接等单个小操作（单次调用的耗时，用来判断第 7、8 阶段绑定是否值得）；
- 与 fontTools（通格 `python_reference.py`、收录站 `downloads.py`）在矢量化和 WOFF2 上的对比；
- 与通格 `export-rs` 的对比。

---

## 11. 迁移说明（dot 只写文档，不改那两个仓库）

- `docs/migration-tong.md`：通格的 `tools/export-rs` 每个功能对应 `fontpixel` 的哪个命令或 API；给出能直接替换 `export-rs` 的 `fontpixel batch` 任务文件示例；`site/build.py` 的子集化怎么改用 `fontpixel subset`。
- `docs/migration-fontpixel.md`：收录站 `opf/parsers`、`opf/ingest/*`、`rust/opf_vector`、`downloads.py`、`ttfexport.py` 每个功能的替代；`manifest.toml` 的 `convert = …` 各取值对应的 `fontpixel` 格式和参数；收录站专用的 `shinonome.py`、`uwttyp0.py`、`tong.py`、`extract.py`、`run.py` 不在本包范围，保持原样。
- 两份说明都要写清楚：输出与现在有什么不同（若有），为什么。

---

## 12. 阶段与工作项

每个阶段内的工作项可以并行（最多 6 个 subagent 同时做，每个 subagent 一个工作项）。一个阶段的全部工作项合并、整体测试通过后才进入下一阶段。每个工作项的完成标准：代码 + 测试 + `docs/formats/<id>.md`（如是格式）+ 全部 CI 检查通过。

### 第 0 阶段：骨架（1 个工作项，协调者自己做或交 1 个 subagent）

- W0.1 建工作区、各 crate、feature 结构、`report.rs`、错误类型、格式注册表（`detect`/`read`/`write` 的 trait 与注册）、CLI 骨架（`formats`、`info`、`convert` 的分发）、测试目录、`docs/decisions.md`、CI 脚本（`scripts/check.sh`：fmt、clippy、test、deny）。
- W0.2 生成字符集表（§6）。
- 完成标准：`scripts/check.sh` 通过；`fontpixel formats` 列出 `FORMATS.md` 全部格式（未实现的标“未实现”）。

### 第 1 阶段：核心与 bdfparser 兼容

- W1.1 数据模型与 `Bitmap`（§4.1–4.4），全部位图操作，含性能基准。
- W1.2 BDF 读写（§FORMATS `bdf`；与 SPEC-v3 逐字节一致）。
- W1.3 PCF 读写（含 gz；全部金样通过；不依赖 `bdftopcf`）。
- W1.4 渲染（§7）与 `preview`、`render`。
- W1.5 bdfparser 兼容层 + `fontpixel-parity` + 差分测试移植（§5.5、§9.2）。依赖 W1.1–W1.4，可先写测试框架和兼容层外壳，最后接上。
- W1.6 `yaff` 读写（作为测试金样格式）。
- 完成标准：§9.2 全部一致；BDF/PCF 达到 §10 前两项目标。

### 第 2 阶段：OpenType

- W2.1 sfnt 公共层：表写入、校验和、`name`、`OS/2`（范围计算）、`cmap`、`post`、`hhea`/`hmtx`、`gasp`、`vhea`/`vmtx`；sfnt 读取。
- W2.2 轮廓：方点合并、独立方点、圆点（§8.1）；`glyf`/`loca`（含复合字形）；bdfparser `write_ttf`/`read_ttf` 兼容（逐字节）。
- W2.3 `CFF ` 编码器与读取。
- W2.4 内嵌点阵 `EBDT`/`EBLC`/`EBSC`，`otb` 读写；`bdat`/`bloc` 读写；`CBDT`/`sbix` 读。
- W2.5 WOFF1、WOFF2 编解码（§8.3），加速方案研究与测速。
- W2.6 `Variants` 与 `GSUB` `locl`（§4.5）；`fontpixel variants`；子集化（§5.3）与 `fontpixel subset`。
- W2.7 `vector-raster`：像素化矢量字体退回点阵（网格检测、栅格化、优先取内嵌点阵）；TTC/OTC 读写。
- W2.8 SVG 输出（图像、单字、SVG 字体）与 SVG 字体读取。
- 完成标准：§9.3 中 fontTools、OTS、FreeType、HarfBuzz 各项通过；§9.4 通格等价测试通过；§10 通格整套导出的测速完成。

### 第 3 阶段：第一优先级的其他格式（FORMATS.md 里 P1 且尚未做的）

- W3.1 `hex` + `psf`（PSF1/PSF2，含 Unicode 表与序列）。
- W3.2 `fnt` + `fon`（NE 读写、PE 读）。
- W3.3 `image`（字表图读写，含映射文件）+ `c`（font8x8、t-mat、通用数组、Adafruit GFX）。
- W3.4 `kbitx` + `pbf`。
- W3.5 `hbf`。
- 完成标准：每种格式往返测试 + monobit 交叉验证（monobit 支持的）+ 收录站对应导入器的测试（§9.5）通过。

### 第 4 阶段：第二优先级（FORMATS.md 里 P2）

- W4.1 `draw`、`raw`（含命名预设）、`cpi`（CPI/CP/CPX）。
- W4.2 `pf2`、`fontx2`、`hzk`。
- W4.3 `nfnt`（含资源分支、`.dfont`、手提箱、MacBinary、AppleSingle/Double 容器）。
- W4.4 `bmfont`、`sfd`（点阵部分）、`kbits`。
- W4.5 `pk`、`gf`、`plan9`。
- W4.6 `amiga`、`gem`、`snf`（若时间允许提前做）。
- 完成标准同第 3 阶段。

### 第 5 阶段：第三优先级与格式普查

- W5.1 对照 monobit 的格式清单和其他资料做一次普查：列出所有已知的通用点阵字体格式（不含游戏专用的私有格式，见 §1.3），FORMATS.md 没有的补进去（附依据），决定是否可实现（不能实现的写明原因）。
- W5.2–W5.6 实现 P3 及普查新增的格式（`snf`、`hanfont`、`iigs`、`color` 写、`vfont`、`fzx`、`figlet`、`palm`、`gdr`、`drcs`、U8g2、LVGL …），每个 subagent 一组。

### 第 6 阶段：CLI 完善、批量、文档、测速

- W6.1 `batch`（并行任务文件、JSON 报告）、`diff`、`validate`、`charset` 完成；全部命令的帮助文字与示例。
- W6.2 文档：README（安装、快速上手、每个命令的例子）、API 文档（`cargo doc` 无警告，公共项都有说明和例子）、`docs/formats/*` 齐全、迁移说明（§11）。
- W6.3 测速报告（§10）定稿。
- W6.4 模糊测试与健壮性（§9.6）收尾。
- W6.5 拼合（库）：`fontpixel::merge`；完整接口、选择、取字、对齐、间距、命名、许可证、报告与测试要求按[追加规格 W6.5–W6.6](docs/plan-addendum-merge.md)。
- W6.6 拼合（命令行）：`fontpixel merge`、多格式输出、`batch` 接入与帮助文档；完整语法、通格等价测试和约 5.7 万字原生拼合写 BDF 在 1 秒内的测速要求按[追加规格 W6.6](docs/plan-addendum-merge.md)。

### 第 7 阶段：Python 绑定（`fontpixel-py`）

- pyo3、abi3（Python ≥ 3.10），模块名 `fontpixel`。两层：
  1. 完整原生开发 API：覆盖 fontpixel-rs 全部公共能力，包括整批操作和细粒度模型/字形/bitmap 编辑；不能只提供少量入口或以兼容 API 代替；
  2. bdfparser 兼容 API：`fontpixel.compat.bdfparser` 提供与 bdfparser 相同的 `Font`/`Glyph`/`Bitmap`；数据留在 Rust 一侧，Python 只拿句柄，取像素时才转换。
- 用适配器让 `reference/bdfparser/tests/` 的测试跑在兼容 API 上并通过（不能通过的逐条说明原因）。
- 测速：每类操作（整批、单个小操作）与纯 Python bdfparser 对比，写进报告，**明确哪些操作更快、哪些不快甚至更慢**，供用户决定 bdfparser 要不要把它做成可选加速。
- 能构建 manylinux wheel（本地构建成功即可，不发布）。

- 用户追加要求：Python 与 WASM/JS 都必须提供完整 fontpixel-rs 原生 API，并分别保留 bdfparser/bdfparser-js 兼容 API。完整要求见 [绑定范围](docs/bindings-scope.md)。这些是未来实现/验收要求，不是恢复源码已完成或通过的声明。
- 建立逐公共模块、类型、构造/操作、选项、返回/错误类型与 feature 条件的 Rust → Python → WASM/JS 映射；每项有绑定入口、平台差异和覆盖测试，新增公共能力不能静默漏映射。
- 覆盖现有全部格式注册表/查询/检测/读写及 typed options，FontSet/Font/Glyph/bitmap/名称与度量编辑，渲染/轮廓/矢量化/栅格化，OpenType 表与容器/WOFF/WOFF2、variants/locl、subset、charset、merge、batch、validate、diff、完整报告/错误/strict 和资源/sidecar/多资产语义。摘要不限制其他公共能力。
- 当前 CLI 编排能力须复用或抽取为可调用 API，不能用外部 CLI 子进程补齐原生接口。原生覆盖测试独立于兼容测试，逐操作/格式验证结果、边界/错误、strict/资源限制及完整工作流。
- 明确 bytes/缓冲区、具名文件/资源集合、句柄寿命/复制、整数范围与同步/异步适配；真正平台不可用项须可查询、明确失败并附原因/证据/替代方案。未实现仍是未完成，不能冒充平台限制或返回空成功。
- 当前授权是第 6 阶段交付后依次继续第 7、8 阶段；不启动第 9 阶段，不 push/publish/deploy。阶段 6 恢复源码尚未编译/测试，阻塞的长 ASAN 仍明确交给用户本机，不因后续授权而视为通过。


### 第 8 阶段：WASM 绑定（`fontpixel-wasm`）

- wasm-bindgen，npm 包名 `fontpixel`，完整原生 API 与独立 bdfparser-js 兼容 API 两层；第 7 阶段全部能力映射和原生覆盖验收同样适用，不能只提供 W8.3 的几个入口。
- Node/浏览器与完整/精简 feature 组合分别记录入口、类型定义、覆盖测试、真实不可用项；供 JS/TypeScript 独立开发。浏览器用 Uint8Array/ArrayBuffer、具名资源集合与显式解析器代替任意文件路径，异步读取由 Promise/Worker 适配层处理，不能隐式读文件/网络。
- 测试线程/同步异步/进度取消、句柄释放/复制、Number/BigInt 精度及资源边界；保持原生结果、strict/报告和适用事务语义。浏览器内存 bundle 不等于磁盘事务，兼容通过不等于全原生 API 覆盖。
- 用适配器跑 `reference/bdfparser-js/test/` 并通过（同上，不能通过的说明原因）。
- 测速：与纯 JS bdfparser-js 在 Node 和浏览器（headless Chrome）里逐类对比；报告 WASM 文件大小（含与不含 WOFF2/brotli、CJK 字符集等 feature 的几种组合）、初始化时间。
- W8.3 wasm 的拼合接口：`info`、`merge`、预览 `render`、Web Worker 与进度回调；Node/headless Chrome 与原生命令行输出逐字节相同，feature 组合大小和初始化时间按[追加规格 W8.3](docs/plan-addendum-merge.md)验证。

### 第 9 阶段：拼合网页（`web/merge/`）

本阶段在第 8 阶段全部完成并通过原有及追加验收后执行；完整要求见[追加规格第 9 阶段](docs/plan-addendum-merge.md)。纯客户端、字体不上传服务器；静态网页不使用前端框架，加载后可离线使用。

- W9.1 页面：本地文件/字体目录选取、像素预览与等宽提示、拼合选项、必填家族名和保留字体名校验、许可证提示、浏览器内下载/ZIP、中文与英文界面，逐项按追加规格实现。
- W9.2 字体目录格式：在 `docs/web-merge.md` 定义 `catalogue.json` 及接入步骤；文件 URL 仅相对或同源，样例仅用获许可的 samples 字体；不修改收录站仓库、不部署。
- W9.3 测试与交付：浏览器端等宽/比例拼合输出 BDF/WOFF2 与原生命令行逐字节比较；验证等宽错误提示、保留字体名限制、5 万字页面响应，并报告 wasm 大小/加载时间及 Chrome、Firefox、Safari 最近两个大版本兼容范围和已知限制。
- 完成标准与每阶段报告要求均按追加规格全文；不得用本计划中的工作项摘要替代其详细要求。

### 收尾

- 最终报告 `FINAL-REPORT.md`：完成情况逐条对照 §1.2；每种格式的读写状态、测试、已知限制；测速结果；未完成或有疑问的事项列表。

---

## 13. 风险与处理

| 风险 | 处理 |
|---|---|
| 某些冷门格式找不到规格或样本 | 先普查资料（含 monobit 源码），写进 `docs/formats/<id>.md`；确实找不到的标“只读”或“暂缓”，在最终报告里说明，不硬猜 |
| `write-fonts` 等依赖不支持某功能（如 CFF 写入） | 自己实现最小编码器（通格 `cff.rs` 已证明可行） |
| WOFF2 压缩时间达不到目标 | 报告实测数据和各方案对比，让用户选择质量与速度的取舍 |
| bdfparser 差分测试出现规格本身没写清的情况 | 以 Python 参考实现为准，并把发现写进 `docs/decisions.md`，在最终报告里列出，供用户决定是否改 SPEC-v3 |
| 绑定的小操作比纯 Python/JS 慢 | 照实报告，不为测速好看而改测法 |
