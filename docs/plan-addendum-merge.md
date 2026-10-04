追加任务（用户 Tom Chen 已授权，2026-10-04）：在 fontpixel-rs 里加入“拼合”功能和命令行子命令，并把它做进 wasm 包和一个纯客户端网页，之后放到 fontpixel.com。

不要打断手头的阶段。先把下面的内容原样写进仓库 `fontpixel-rs/docs/plan-addendum-merge.md`，在 `docs/decisions.md` 记一笔，然后按下面说的位置插进阶段计划：工作项 W6.5、W6.6 放进第 6 阶段，W8.3 放进第 8 阶段，新增第 9 阶段（W9.1–W9.3）放在第 8 阶段之后、收尾之前。规则与原计划相同：阶段之间按顺序，同阶段内可并行，同时最多 6 个 subagent，模型 gpt-6-astra，reasoning_effort xhigh，fork_turns="none"；PLAN.md 没写清的地方自己决定并记进 decisions.md。

## 目的

把一个字体里的某类字形（例如某个汉字点阵字体里的全部中日韩字形）和另一个字体里的另一类字形（例如某个西文点阵字体里的全部西文字形）拼成一个新字体，导出成 fontpixel 能写的任何格式（BDF、PCF、TTF、OTF、WOFF、WOFF2、OTB 等）。原生命令行和浏览器里的结果必须逐字节相同。用户要在 fontpixel.com 上纯客户端完成这件事：字体不上传到服务器。

## W6.5 拼合（库）：`fontpixel::merge`

输入是若干“部件”，每个部件 = 一个 `Font`（或 `FontSet` 里选定的一个）+ 一个选择 + 可选的垂直偏移。

1. 选择（可组合，可排除）：
   - `cjk`：汉字（含扩展区、兼容汉字、部首、笔画、汉字结构描述符）、假名、韩文（音节、字母、兼容字母）、注音、中日韩符号和标点、全角与半角形式（U+FF00–FFEF）、中日韩圈号和带框字（U+3200–33FF、U+1F200–1F2FF）、竖排形式等。把判断写成一张有出处的表（按 Unicode 区块和 East Asian Width），放在 `charset/` 里，测试覆盖边界。
   - `western`：其余全部（拉丁、希腊、西里尔及其他文字，符号，数字，标点……）。
   - `geometric`：制表符、方块元素、盲文、Powerline 私用区（U+2500–259F、U+2800–28FF、U+E0A0–E0D4）。这一类在中日韩字体和西文字体里常常宽度不同，所以单独成类，默认归 `western`，可改。
   - 码位范围 `U+XXXX-YYYY`、`all`，前面加 `-` 表示排除。
2. 取字规则：一个码位取自**第一个**选中它、且有这个字形的部件。`fallback` 选项：选中的部件没有这个字时，按部件顺序从其他部件取（默认关）。
3. 对齐：所有部件按基线对齐；结果的 ascent / descent 取各部件的最大值（可用参数直接指定）；每个部件可加整数垂直偏移 `dy`。
4. 间距：
   - `prop`：各字形保留自己的步进；
   - `mono`：取 `cjk` 部件汉字最常见的步进为全角宽 W，西文半角目标为 W/2：
     - 西文步进 = W/2，照原样；
     - 西文更窄、且 W 是偶数，就左右补空白居中到 W/2；
     - 西文更宽，报错，除非给了 `allow_misaligned`（这时照原样并在报告里列出）；
   - `auto`：两个部件都是等宽就按 `mono`，否则 `prop`。
5. 名字和元数据：
   - 新字体的家族名必须由调用方给出，不能默认沿用任何一个部件的名字（部件的 OFL 保留字体名不能用在修改版上）。
   - 版权、许可证属性和 name 表依次写入各部件的版权和许可证文字，前面加一行说明“由 fontpixel 拼合，部件：…”。
   - 部件许可证不明或不允许修改时，在报告里警告（`strict` 下失败）。
6. 报告（JSON 和文字）：每个部件取了多少字，选中却缺的码位（按区块汇总），等宽调整（补空白的、对不上的），度量变化，许可证列表。

## W6.6 拼合（命令行）：`fontpixel merge`

```
fontpixel merge -o OUT [-o OUT2 ...] --family NAME \
    --part CJK.bdf --take cjk \
    --part WEST.bdf --take western,geometric [--dy -1] \
    [--part EXTRA.pcf --take U+2190-21FF,-U+21A0] \
    [--spacing auto|mono|prop] [--allow-misaligned] [--fallback] \
    [--ascent N --descent N] [--shape square|dot ...] [--report json|text] [--strict]
```

- `--take`、`--dy` 属于紧跟其前的 `--part`。多个 `-o` 一次写多种格式（格式按扩展名，矢量格式沿用 `vectorize` 的选项）。
- 也能写进 `batch` 的任务文件。
- 帮助文字和例子写进 README。

**完成标准：**
- 合成小字体的单元测试覆盖选择、取字顺序、fallback、对齐、三种间距的每个分支和报错。
- **通格等价测试**：samples/tong-pixel-sans 里，用某地区字体的汉字部分加 Latin 字体的西文部分拼合，结果与通格自己构建的同地区字体逐字形一致（地区对应写进测试；不一致的类别逐条说明原因）。
- 拼合 GNU Unifont 这种约 5.7 万字的字体加一个西文字体，写 BDF 原生要在 1 秒内（写进测速报告）。

## W8.3 wasm 的拼合接口

- `fontpixel-wasm` 导出：`info(bytes)`；`merge(parts, options) -> { files: {name: Uint8Array}, report }`，其中 parts 是 `[{bytes, take, dy}]`，options 同命令行；`render(fontBytes, text, opts)` 返回一位图或 RGBA，供网页预览（复用 §7 渲染）。
- 大输入（5 万字以上）不能卡死页面：在 Web Worker 里运行，有进度回调。
- **测试**：Node 和 headless Chrome 里，用同样的输入拼合，结果与原生命令行逐字节相同。
- 报告 wasm 大小：含与不含 woff2/brotli、矢量化等 feature 的几种组合，以及初始化时间。

## 第 9 阶段：拼合网页（`web/merge/`）

### W9.1 页面

纯静态（HTML + 原生 JS/TS 模块 + wasm），不用前端框架、不需要服务器；构建产物在 `web/merge/dist/`，直接放到 fontpixel.com 的任意路径下即可运行，加载完成后可离线使用。

1. **选字体**：每个部件可以是
   - 本地文件（拖放或选择，常见点阵格式都能读）；
   - 一份字体目录（`catalogue.json`）里的字体，格式见 W9.2。

   默认两个部件：“汉字字体”取 `cjk`，“西文字体”取 `western,geometric`；可以增加部件、改选择、调垂直偏移。
2. **实时预览**：
   - 画布按真实像素画，1× / 2× / 4×，显示基线和行高线；
   - 示例文字可编辑，默认中日韩与西文混排、带变音符的字母和数字；
   - 等宽时显示网格，对不上的西文字标红并说明原因。
3. **选项**：比例 / 等宽 / 自动，fallback，ascent / descent，点形（方点、圆点，矢量格式用），输出格式（可多选）。
4. **命名**：家族名必填，给一个建议名（如“Mix”加两个部件的简称），但建议名不能含任何部件的保留字体名。目录里登记了保留字体名的，输入时就提示。
5. **许可证**：页面上列出每个部件的许可证和版权行（目录里的字体从目录读，本地文件从文件元数据读）。
   - 目录里登记为不允许修改的字体不能选。
   - 本地文件许可证不明时提示“请确认你有权修改和再分发”，但不拦。
6. **下载**：一种格式直接下载，多种打成 zip（在浏览器里打包）；同时可下载拼合报告。
7. 界面文字放在 `web/merge/i18n/{zh,en}.json`，先做中文和英文（其他语言以后由用户另行安排翻译）。

### W9.2 字体目录格式（与 fontpixel.com 的接口）

在 `docs/web-merge.md` 定义 `catalogue.json`。每个字体至少有：id、名称（中英）、文件 URL（相对或同源）、格式、像素大小、ascent / descent、是否等宽及全角 / 半角宽、包含的类别（cjk / western / geometric）、许可证（SPDX）、是否允许修改、保留字体名列表、版权行、推荐搭配（如“汉字 12 号配西文 12、13 号”）。

- 页面只按这个文件工作，不写死任何字体。
- 附一个样例目录：只用 samples/ 里许可证允许的字体，测试和演示用。

fontpixel.com 那边的接入（从收录站元数据生成 catalogue.json、部署页面）由用户另行安排，不在本任务里改收录站仓库，只在 `docs/web-merge.md` 写清楚接入步骤。

### W9.3 测试与交付

- Playwright（或同类）端到端测试：载入样例目录的两个字体，等宽和比例各拼一次，导出 BDF 和 WOFF2，与原生命令行输出逐字节比较。
- 再测：
  - 等宽对不上时有提示；
  - 保留字体名不能用作家族名；
  - 5 万字字体拼合时页面不卡。
- 最终报告里加一节：网页功能清单、wasm 大小与加载时间、浏览器兼容范围（最近两个大版本的 Chrome、Firefox、Safari）、已知限制。

完成每一阶段时，阶段报告里写明这几个工作项的完成情况。

不要打断手头的阶段：先把以上内容原样写进 fontpixel-rs/docs/plan-addendum-merge.md，在 docs/decisions.md 记一笔，然后按附件说的位置插进阶段计划（W6.5、W6.6 进第 6 阶段，W8.3 进第 8 阶段，新增第 9 阶段放在第 8 阶段之后、收尾之前），到了对应阶段照做。规则与原计划相同：同时最多 6 个 subagent，模型 gpt-6-astra，reasoning_effort xhigh，fork_turns="none"。