# 当前进度：第 8 阶段进行到一半，已按用户要求暂停（2026-10-06 晚）

用户要求一路做完第 7、8、9 阶段和最终报告（只做本地提交，不 push、不发布、不部署）。2026-10-06 晚用户因 token
额度不足要求在合适的点暂停，并详细记录进度和接续方法——见下面「第 8 阶段暂停记录」。

- 第 6 阶段：[reports/phase-6.md](reports/phase-6.md)（严格标准）。
- 第 7 阶段：[reports/phase-7.md](reports/phase-7.md)；测速与 wheel [benches/PHASE7-REPORT.md](benches/PHASE7-REPORT.md)。

## 第 8 阶段暂停记录（接续从这里开始）

### 1. 现在的状态

- **main**：含第 7 阶段全部、第 8 阶段地基 A（核心/ops 的 wasm32 适配、拼合/转换进度与取消，`15cf8af`）、
  地基 F（`crates/fontpixel-wasm` 骨架与全部约定，`40bc508`）、单元 J2（bitmap/渲染/charset/轮廓的 JS API，`f263b9e`）。
- **未合并的 5 个分支**（都已提交、工作区干净，worktree 已删除、分支保留；各自的构建目录已删）。每个分支的详细交接
  （已完成、剩余清单、已知问题、恢复命令）在 `../local-work/p8-<单元>/HANDOFF.md`：

| 分支 | 单元 | 最后提交 | 状态 | 交接文件 |
| --- | --- | --- | --- | --- |
| `p8-js-model` | J1 模型/元数据/资源（model/core/root.toml） | `fbebed7` | 覆盖完成，范围内门禁 0 错误；合并 J2 后只跑过快速构建（96 过 1 跳），完整 check_wasm.sh 未在合并后重跑 | `../local-work/p8-j1/HANDOFF.md` |
| `p8-js-formats` | J3 99 个格式（formats.toml，生成器） | `38a53d3`（WIP 说明） | full 构建 1,408 项格式测试全过；merge 构建 1 项失败（`setWriteOptions('bdat')` 在该构建没报错）；改后未重跑 wasm32 clippy；无 TS 冒烟/文档 | `../local-work/p8-j3/HANDOFF.md` |
| `p8-js-sfnt` | J4 sfnt/WOFF/variants/merge 库 | `191bbb3` | 241 条规则全有 JS 入口与测试；full 构建通过（WOFF2 逐字节 1 项 TODO，见第 2 节）；minimal/merge 构建未跑 | `../local-work/p8-j4/HANDOFF.md` |
| `p8-js-ops` | J5 ops 全部 22 个操作 + W8.3 `info`/`merge`/`render` + Worker/进度/取消 + Node 文件辅助 | `a5cbbd3` | 126 条规则完成；5.7 万字 Unifont+Tong Latin 拼合 BDF 与 CLI 逐字节相同；WOFF2 逐字节 TODO；check_wasm.sh 未跑完 | `../local-work/p8-j5/HANDOFF.md` |
| `p8-js-compat` | J6 bdfparser-js 兼容 API + 参考测试 | `6f85876` | 自测 27 过；参考套件 1178 过 / 11 败 / 3 跳（慢测试 1181/11），11 个失败已写入 KNOWN_DIFFERENCES.md；改了 Rust 兼容核心 3 处文本，Python 仍 1262 过、参考 1755/8/10 不变；check.sh 中途停、check_wasm.sh 未跑、参考结果 JSON 需重跑 | `../local-work/p8-j6/HANDOFF.md` |

### 2. WOFF2 在 wasm 与原生不逐字节相同——**用户已决定采用方案 (a) `portable-float`（2026-10-06）**，接续时实施

- 现象：同一个字体，wasm 生成的 WOFF2 与原生 CLI 不同（解压后的表完全相同）。例：Tong SC CFF→WOFF2 原生 890,304 B，
  wasm 890,000 B；Tong 拼合 wasm 949,852 B / 原生 953,060 B。BDF 等其他输出逐字节相同。
- 原因：brotli 9.0.0 用平台数学库算 `f32::log2`，Linux 原生与 wasm 在 1,670 万个输入里有 88,636 个最后一位不同。
  与线程无关（单线程也不同）。地基 A 的「逐字节相同」只在 x86 上比较了多线程/单线程，结论有误，decisions.md 已加更正。
- 可选方案：
  - (a) 在 `crates/fontpixel/Cargo.toml` 给 brotli 开 `portable-float`：试验中 wasm 与原生全部一致，但原生 WOFF2 字节
    也会变（Tong SC 变为 892,576 B）；需要重建冻结参考二进制、更新与 WOFF2 字节相关的测试/报告。
  - (b) 不改：报告写明 WOFF2 在 wasm 与原生「解码后相同、压缩字节不同」，W8.3/W9.3 的「逐字节相同」对 WOFF2 不成立。
- 建议 (a)，因为 PLAN W8.3/W9.3 明确要求网页输出与命令行逐字节相同。

### 3. 接续步骤（建议一次只跑一个单元以省 token）

1. 依次合并分支到 main（推荐顺序与要处理的冲突）：
   1. `p8-js-model`（J1）：先合并；之后核对 `docs/bindings-design.md` 的 Handles/Resources 两节（J1 的 `__fpStash()` +
      `model_args::handle_arg` 让句柄可放进数组/联合参数而不被消耗；J2 当前用 `copy()`，可改用它）。
   2. `p8-js-formats`（J3）：J3 自己定义了 `Written`、`OutputBundle` 类，与 J1 可能冲突，保留一份；修 merge 构建的
      `bdat` 测试（在 `formats_options.rs` 先检查格式是否编入）；重跑 wasm32 clippy、TS 冒烟；测量 wasm 体积
      （full 构建 wasm-opt 前 11.4→16.6 MB，可能来自 703 个类型的 `normalize` 表，考虑精简）。
   3. `p8-js-sfnt`（J4）：其 `merge` 命名空间与 J5 的顶层 `merge()` 函数重名，需改名之一（建议库层叫 `mergeLib`
      或把 W8.3 的放在 `web.merge`/顶层，二选一并写进文档）；在 minimal/merge 构建跑测试。
   4. `p8-js-ops`（J5）：跑 check_wasm.sh；`gen_dts` 对 `TaskRequest` 生成的 TS 形状不对（J5 用自写 `BatchTaskSpec` 绕过），
      修生成器；决定 W8.3 `render` 是否改用 J2 的 `Font.draw`（现在复用原生 `preview`）。
   5. `p8-js-compat`（J6）：跑 `scripts/check.sh`（改了兼容核心）、`scripts/check_python.sh`、check_wasm.sh；
      重跑 JS 参考测试刷新结果 JSON（`crates/fontpixel-wasm/tests/compat_bdfparser/run_reference.mjs`，依赖装在
      `../local-work/p8-j6/ref-run`，方法见其 HANDOFF）；Python 参考测试需要装 `freetype-py`，否则多 15 个失败；
      Python 慢测试参考运行被中断，需重跑。
   每合并一个：`scripts/build_wasm.sh full` + `node --test crates/fontpixel-wasm/tests/*.test.mjs`；全部合并后跑一次
   `scripts/check_wasm.sh`（最后一步是 `tools/bindings/gate.sh python,js`，必须 0 错误）和 `scripts/check.sh`。
2. 实施 WOFF2 方案 (a)：`crates/fontpixel/Cargo.toml` 给 brotli 开 `portable-float`；重新核对 brotli 堆上界
   （`brotli_encode.rs` 经验包络、`fontpixel-heap-check`）；重建冻结参考二进制；更新受影响的 WOFF2 字节测试、
   phase 6 报告里引用的 WOFF2 大小/哈希（注明是改动后的新值）；把 J4/J5 的 WOFF2 逐字节 TODO 测试改成正式断言。
3. 第 8 阶段剩余工作：
   - 浏览器测试：Playwright，Chromium/Firefox/WebKit（浏览器已在 `~/.cache/ms-playwright`；playwright npm 包装在
     `../local-work/` 下，不要装进仓库或 reference/）；WebKit ≠ Safari 的措辞要写清。
   - 与纯 JS bdfparser-js 测速（Node 与 headless Chrome，逐类对比，慢的照实写），参考 `benches/phase7_python.py` 的方法。
   - wasm 体积（full/merge/minimal，含与不含 WOFF2/brotli、CJK 字符集、矢量化）与初始化时间（Node、浏览器）。
   - JS 文档（类似 `docs/python-api.md`）、示例、TS 类型检查；Node 18 加载验证（目前只在 Node 24 跑过）。
   - 第 8 阶段报告 `reports/phase-8.md` 与 `benches/PHASE8-REPORT.md`。
4. **第 9 阶段（2026-10-07 用户改定）：直接更新 `/home/chen/githubprojects/fontpixel/fontpixel.com`，加一个 wasm 纯前端的
   字体工具页（切割、合并、转格式、像素化）。** 原独立页 `web/merge/` 不再单独做，其设计（预览、等宽提示、命名校验、许可证、
   Worker 进度、catalogue 思路）并入工具页。依赖第 8 阶段的 ops/formats/render/merge/Worker 接口全部合并。详见
   docs/decisions.md「2026-10-07 — Phase 9 retargeted to fontpixel.com」。
   - 动手前注意：fontpixel.com 当时有未提交修改（`.github/workflows/ci.yml`、`deploy.yml`、`.gitignore`），不是本项目改的，
     先确认归属，不要覆盖；网站技术栈是 Astro + Svelte 岛组件（`site/`），工具页沿用 Svelte 5（runes），不再适用
     「不用前端框架」（那是独立静态页的要求）；构建产物在 `dist-downloads/`，下载由 `site/scripts/stage-downloads.mjs` 暂存；
     预览目前用 bdfparser(JS)。
   - 用户要求：
     1. 对象：fontpixel.com 上的任何字体，或用户上传的任何字体。
     2. 转成我们支持的常见格式。
     3. 选一个或多个 Unicode block 或手动输入区段进行切割；或选多个字体，各自选 block/区段，进行切割和/或合并。
     4. 像素化：TTF/OTF/WOFF/WOFF2 矢量字体按指定像素大小栅格化成点阵（核心 `vector-raster`，像素网格矢量字自动识别网格；
        不执行 hinting，普通矢量字小字号效果有限，页面要提示）。
     5. 每个字体页的下载区只提供预生成的 BDF 和 WOFF2；其他所有格式在用户点击下载时由客户端 wasm 即时生成。
   - 补充项（我提出，用户 2026-10-07 只修改了第 3 条，其余默认采纳，用户可随时删）：
     1. OFL 保留字体名（RFN）：切割/合并属于修改，必须改名；读取保留名，改名必填且不能沿用；不允许修改的字体禁用切割/合并。
     2. 按文字切割：粘贴一段文字，只保留用到的字（做网页字体常用）。
     3. **生成前预览（用户细化）**：用结果字体渲染各种文字——CJK 与西文都要，按每个 Unicode block 或书写体系各显示若干
        代表字；同时显示每个 block 的字数/覆盖情况、缺字和损失报告。
     4. 常见输出格式：BDF、PCF、OTB、位图 TTF、矢量 TTF/OTF（方点/圆点）、WOFF/WOFF2、PSF、FNT/FON、Unifont hex、YAFF，
        加 BMFont（PNG+.fnt）和 C 数组。
     5. wasm 按需加载：字体页平时不加载，点「下载其他格式」或进工具页才加载；用只含常用格式的瘦身 profile；在 Web Worker
        里运行，有进度和取消；生成过的文件缓存在浏览器（Cache API/IndexedDB），再次下载即时。
     6. 可分享链接：字体、范围、格式写进 URL 参数；字体页可直接跳到预填好的工具页。
     7. 旧下载链接：去掉预生成 PCF/TTF 后旧 URL 会 404——让旧 URL 跳到即时生成页；更新 README、文档、`llms.txt`；
        不开 JS 的访问者仍可直接下载 BDF/WOFF2。
     8. 构建流水线瘦身：停止预生成 PCF/TTF。
     9. 许可证随文件走：生成的字体写入原许可证/版权信息，zip 附 LICENSE。
     10. 手机内存：大字体超出资源上限时捕获错误并给明确提示。
     11. 界面多语言沿用网站的 en / zh / zh-Hant；翻译交给 Opus 或 Sonnet 子任务（全局规则）。
     12. 测试：Playwright 端到端，生成结果与命令行逐字节比较（BDF、WOFF2 及几种常见格式）；5 万字大字体不卡；三个浏览器。
     13. 技术栈：沿用网站的 Astro + Svelte 5。
     14. 只本地提交，不 push、不部署；部署由用户安排。
   - wasm 体积决定（2026-10-06，仍适用）：npm 包维持完整原生 API 按 full/merge/minimal 分档，没编入的格式明确报
     `UnsupportedError`；网站用只含常用格式的瘦身 profile（新增，如 `web`）；冷门格式上传时提示「不支持此格式，请先用
     命令行转换」，不按需加载完整版；CJK 编码表看实际需要，必要时按需加载；记录体积与加载时间。
5. 收尾：对最终源码重跑 104 目标 ASAN（参考第 6 阶段 gate 077 与 `reports/phase6-validation/fuzz-final.md`）和逐 feature
   Clippy（`feature_clippy_serde.sh`，218 种组合；之前 gate 201 被中止未完成）；写 `FINAL-REPORT.md` 逐条对照 PLAN §1.2。

### 4. 省 token 的做法（用户 2026-10-06 的要求）

- 一次只开一个子任务；开工前先告诉用户预计消耗。参考：本阶段每个 Opus 子任务约 37 万～83 万 token。
- 子任务一律用 Opus（用户 2026-10-06 决定「以后还是用opus」），不为省 token 换 Sonnet。
- 少跑重复的全量门禁：单元内用快速构建迭代，合并后只跑一次完整检查。

### 5. 环境与证据

- 证据目录 `../local-work/phase6-local-20261004/`：`run_gate.py LABEL "命令"`（LABEL 自带编号，最后一个是
  `212-p7-wheels-final`）、`checkpoint.py`；第 7 阶段轻量备份 `checkpoints/phase7-final-20261006/`（全部引用的 bundle + wheel）。
- Python：`../local-work/py310-venv`（CPython 3.10，maturin/pytest/mypy），`../local-work/py-bindings-venv`（maturin、ziglang、auditwheel）。
- WASM：wasm-bindgen 0.2.129（锁定），wasm-pack 0.15（~/.cargo/bin），wasm-opt 117（~/.cache/.wasm-pack），Node 24.21；
  TypeScript 只读使用 `../bdfparser-js/node_modules/.bin/tsc`。各子任务约定见 `../local-work/p8-common-rules.md`。
- 主构建目录 `../local-work/target-fontpixel-rs`；子任务各用 `../local-work/target-p8-<单元>`，完成后删除。
- 删除只用明确的绝对路径；定时清理构建目录和临时文件。

- [x] 第 6 阶段按 PLAN 原文交付（`6297998`）。
- [x] 第 6 阶段严格标准：单元 A–K 全账本与追平选项，最终源码 `8cd6d66`，冻结 v5；
  完整检查、109 种 feature 组合 Clippy、真实字体对照、104 目标 ASAN、默认负载巡检、正式测速与 A/B。
- [x] 第 7 阶段：Python 绑定（完整原生 API + bdfparser 兼容 API），最终源码 `d9ff25c`：覆盖率 2,004 项 0 错误，
  pytest 1,253 通过，bdfparser 参考测试 1,755/1,773 通过（8 个失败逐条说明），测速报告，manylinux wheel（x86_64 在 3.10/3.12 测试通过）。
- [ ] 第 8 阶段：WASM 绑定。**暂停中**，详见上面「第 8 阶段暂停记录」。
- [ ] 最终报告前：对最终源码重跑 104 目标 ASAN 和逐 feature Clippy（核心在第 7、8 阶段有改动）。
- [ ] 第 9 阶段：改为更新 fontpixel.com，加 wasm 字体工具页（切割/合并/转格式/像素化），见上面接续步骤第 4 项。
- 只做本地提交；不 push、不发布、不部署。

已知限制见 reports/phase-6.md「已知限制」。

以下本地接续/云端交接文字是历史过程；最终暂停状态以最新记录及 checkpoint 为准。

2026-10-04 本地更新：交接包已在新目录独立恢复校验，再通过离线 Git bundle
快进导入原 `/home/chen/githubprojects/fontpixel/fontpixel-rs`。原工作区已完整备份并
在空目录恢复验证；原参考/样本不变。用户已澄清采用协调者决定的本地导入方案，
未配置或声称完成外部备份。详细新证据见 [本地接续记录](reports/phase6-local.md)。

当前普通 Rust 1.99 工具链可用；修复锁文件漏项并格式化后，全 workspace、
all-targets、all-features `cargo check --locked` 已通过（两条 warning 待修）。
这次通过仅覆盖该次源码快照；后续预算、许可及对照修正需要重新验证。
完整 gates、原生对照、性能与最终源码长 ASAN 仍待完成，不进入第 7、8 阶段。
下方保留的是云端交接时的证据状态，不能覆盖本地新记录。

更新：2026-10-04 UTC。交接文件 `fontpixel-rs-local-codex-handoff-20261004.zip`。

**在线 fontpixel 工作已到工具链阻塞边界，完成交接后停止。** 用户最新要求完整记录、打包、提供独立接续文本，由本地 Codex Astra 继续。另行授权在线恢复的 western-rest 字形修补是另一项目，不属于本软件包。本交接不等于第 6 阶段已验收。

先读 [本地交接详解](docs/LOCAL-CODEX-HANDOFF.zh.md)、[可复制 prompt](docs/PROMPT-FOR-LOCAL-CODEX.zh.md)、[阶段 6 报告](reports/phase-6.md)。最终 main/refs/source SHA 只以包内 schema-2 `HANDOFF-MANIFEST.json` 为准；不把旧 `e4f65fe` 检查点或缺失的原 phase-6 commits 当最终源码。

## 当前有证据的状态

- [x] 从验证过的原始包恢复阶段 5 main `12710057e9787318d1db70db41a60fca016ad53c` 及原 54 refs；原输入保留。
- [x] 六项 phase-6 实现、文档、回归源码和 workload 工具已恢复/重写；各组件 provenance、前后 hash、diff、最新 source-only audit 保留。源码存在不等于验收。
- [x] 20:41 UTC 最终快照 600 `.rs` parser-only 全 exit 0，600 源码+10 context 文件清单稳定。不是宏展开、类型检查、build、feature 或 test。
- [x] 20:46 标准库 API 文本检查通过；性能工具直接脚本 tests 9+4+6 通过。初次 unittest import runner 失败和正确重跑结果分别保留。
- [x] 新外部-only oracle 环境/fixture：原 Python/Node full-SC 七产物 parity、全 encoded PCF/TTF pixels/advances、OTS/fontTools、小 sfnt/web/shaping；原 Python catalogue 181 outputs、105 sfnt 应用字段、8 ZIP。不含重建 Rust 对照/正式计时。
- [ ] Cargo/libstd/rustfmt/Clippy 完整工具链；官方 Cargo 下载受策略阻止。孤立 rustc 不能运行生产门禁。
- [ ] 恢复源码普通编译、类型/借用、fmt/Clippy、所有 tests/rustdoc/features、完整 native/oracle 对照、正式五轮性能和 final-source ASAN 均待本地完成。
- [ ] 最终软件包/all-ref/外部保存与独立恢复由最终清单和 coordinator delivery record 确认；本文不提前声称包已交付成功。

## 本地按顺序完成

1. 校验 ZIP/manifest；新空目录执行包根 `python3 -B verify.py`、`python3 -B restore.py`、`python3 -B verify.py --checkout`。核对 frozen main/refs/source 后再动源码。
2. 补齐允许的官方工具链；先 durable backup，再 formatter/typecheck/`scripts/check.sh`、warning-free rustdoc、fuzz dependency audit。所有失败留原始日志，不降门禁。
3. 修复真实编译/运行问题，重跑共享预算、strict、metadata、embedded、PFR、batch、assets、merge/CLI/最小 features 与 API examples。当前只有 source/parser/tooling 证据。
4. 完成完整 compatibility corpus、99 格式与 profile 的适用回归；真实 CLI docs smokes、84 tiny tasks 后再 full fonts。Tiny 不是 full correctness。
5. 完成 native 与原参考的 full Tong 84 artifacts、catalogue 173 fonts+8 ZIP、vector/WOFF2/locl/全部 glyph/name/metric/notice 配对。catalogue 早期遗漏元数据的结果不算语义替换；默认值已识别，当前 native 配对仍未做。
6. **新增并执行 W6.6 真实 Tong 各地区 CJK+Latin merge-equivalence fixture。** 明确 SC/TC/JP/KR、size/spacing 对应原构建和差异类别。现有 synthetic merge tests、Unifont union validator、Tong variants/locl oracle 都不能替代。
7. 处理审计明确 gap：standalone Sources aggregate identity ledger；core Variants/planner/export/WOFF transient 与 subset read/selection 预算；保守零 source credit 对默认工作负载的影响。不得用提高 cap/删 reports/猜测 source refund 掩盖。
8. 冻结正确 binary/source/dependencies/input，停止无关负载；22 workload cases 五进程正式测量，独立审计全部 raw streams/archives/medians。硬目标：SC parse≤100ms，PCF/OTB/YAFF各≤200ms，squareTTF≤1s，默认 full Tong≤40s，57,106-glyph merge+BDF<1s。历史 OTB/YAFF/direct parse/Tong CLI misses 仍是未解，不变成当前数据。
9. 用户本机执行最后源码 required reader ASAN≥10分钟，按 99 reader/104 target inventory 补完整记录。旧71campaign不能代替；实际权限拒绝时停并记未完成，不绕过。
10. 所有适用 gates 真正完成后交付第 6 阶段，再依序第 7、8 阶段；Python/WASM 全原生公共 API+独立 legacy compatibility，两套 coverage tests。不启动9、不push/publish/deploy。

## 保护与备份

- reference/samples/validation-inputs/original-task 只读；新 build/cache/oracle/fuzz/log/tmp 放 sibling local-work，执行参考代码用副本。
- 重大改动/格式化/共享预算修改、长验证/正式计时前后做 checkpoint，保存源码/未提交patch/refs/SHA/命令/失败与成功日志到已授权外部位置并读回验证。单机 Git 不足以防再次丢失。
- 原 phase-6 raw logs/binaries/outputs 已失踪，历史 passing summaries 不能补作当前证据；final manifest、source-only、syntax-only、external-only 和 native acceptance 严格分清。
- 最新默认 native cap仍256MiB；batch working1GiB/pending512MiB/report64MiB per task/2GiB aggregate。这些不是 RSS cap；改预算必须有完整分配/生命周期依据。

## 以下为阶段 5 原交接记录，仅供历史溯源

下方保留原文字节内容。其“在线继续/停止”“第9阶段”等旧描述不构成当前任务安排；以上最新边界和本地交接说明优先。

---

# 开发进度与接续入口

## 停止点

用户最新安排：**本地完成第 5 阶段后停止，并制作交接包；在线 dot 使用 6 个 Astra/xhigh/fast worker 完成第 6 阶段。**
本轮只做第 5 阶段验收、报告、备份与交接；本地不启动第 6 阶段。
在线任务授权和分工见 [完整 prompt](docs/ONLINE-DOT-PHASE6-PROMPT.md)，恢复说明见 [交接说明](docs/ONLINE-DOT-HANDOFF.md)。在线 dot 完成第 6 阶段后也应停止，不自动进入第 7 阶段。

## 当前进度

- [x] 第 0–4 阶段已验收；历史报告在 `reports/phase-0.md` 至 `phase-4.md`。
- [x] 第 5 阶段实现已合入本地 `main`，合并提交 `f64b729`，实现提交 `81b577b`。
- [x] 99 个格式 ID 均接入原生读写；89 个有内容检测器，其余需要显式选择。
- [x] 候选与合并后主线的 `scripts/check.sh` 均通过：各 2,480 次测试执行、597 个套件；包含重复 feature 配置，并非 2,480 个不同测试。
- [x] 全特性 rustdoc、最小 BDF/raw CLI Clippy、fuzz 工作区依赖检查通过；验收/构建记录包含 718 个 SHA，生产代码保持一致；后续仅修复测速脚本日志命名，另有 22 项检查及全量重测。
- [x] 71 路有效长 ASAN 模糊测试通过，共 319,144,269 次执行；另保留 5 路被修复后重跑取代的历史记录。
- [x] 发布版 CLI 已构建和检查：Linux x86-64，全功能去符号后 13,103,232 字节（12.50 MiB），gzip 后 5,634,032 字节（5.37 MiB）。
- [x] 第 5 阶段正式性能验收：185 组用例、301 条引擎记录，五轮全部完成；1,445 次测量进程、2,890 个独立日志均核验 SHA，全部中位数由原始记录独立复算。v2 日志覆盖问题已修复并保留被拒绝记录，最终采用 v3 数据。
- [x] 第 5 阶段最终报告和接续 prompt 完成。本地在此停止；阶段 6 交由在线 dot。

交接目录：`../local-work/delivery/phase6-handoff/`。上传全部 ZIP 分卷及外层校验清单，粘贴 `PROMPT-FOR-DOT.md`。源码最终提交号、全量 refs、分卷大小/SHA 和实际恢复校验以该目录的 `delivery-manifest.json` / 包内 `HANDOFF-MANIFEST.json` 为准。完整正式数据见 [benches/PHASE5-REPORT.md](benches/PHASE5-REPORT.md)。


## 验证与限制

- 阶段报告：[reports/phase-5.md](reports/phase-5.md)。
- 聚合门禁、SHA、发布体积、长测索引：[reports/phase5-validation/](reports/phase5-validation/)。长测按各模块源码快照解释，不能声称全部历史长测使用完全相同的共享代码。
- 当前 worker 冻结清单：18 份、459 个唯一文件已核对；独立检测复核覆盖 189 个样本、75 次原 CLI 检查。
- 格式边界：[reports/w5.scope-limitations-review.md](reports/w5.scope-limitations-review.md) 与 `docs/formats/`。99 IDs 不代表所有历史变体或任意格式间无损转换；模型是 Mono/Gray8，彩色输出为黑色像素加 alpha 覆盖率，不能保留任意 RGBA 图片。
- PFR 多逻辑字体共享物理表的重复解析已有累计资源预算，后续仍应评估缓存优化；见 [scaling review](reports/w5.scaling-review.md)。
- 原参考资料和样本保持只读：690 个保护文件校验通过。不修改收录站/通格仓库，不发布远程或部署。

## 下次接续（本轮不执行）

先读 `PLAN.md`、`docs/plan-addendum-merge.md`、`docs/decisions.md` 和第 5 阶段最终报告；遵守阶段验收顺序。

- [ ] 第 6 阶段：W6.1 CLI batch/diff/validate/charset；W6.2 文档与迁移说明；W6.3 完整性能目标；W6.4 健壮性收尾；W6.5 库拼合；W6.6 CLI 拼合与批量接入。
- [ ] 第 7 阶段：Python 原生/兼容绑定、参考测试、边界调用测速、manylinux wheel。
- [ ] 第 8 阶段：WASM 原生/兼容/拼合接口，Node/浏览器验证、体积与初始化测速。
- [ ] 第 9 阶段：离线拼合网页、目录接口、浏览器验收；不部署。
- [ ] 最终报告逐条对照 `PLAN.md` 成功标准；本项目整体尚未完成。

仅保留当前主源码目录；构建、oracle、性能原始产物和交付备份位于相邻 `../local-work/`。后续协作优先复用已有 worker、按文件分工，避免再生成根目录 worktree 群。
