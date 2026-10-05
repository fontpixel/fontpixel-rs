# 第 6 阶段本地接续记录

日期：2026-10-04 UTC 起。**2026-10-05 用户明确恢复，范围只做第 6 阶段**（不做第 7、8、9
阶段，不 push、publish 或 deploy；western-rest 不属于本项目）。恢复后的进展见文末
「2026-10-05 恢复」一节；暂停时的完整状态见
[2026-10-05 暂停记录](../docs/LOCAL-PAUSE-20261005.zh.md)。下文较早段落中的 running、
pending、待重测等字样是当时状态。

## 导入与保护

- 输入 ZIP：`/home/chen/Downloads/fontpixel-rs-dot/fontpixel-rs-local-codex-handoff-20261004.zip`。
  SHA-256：`624b8176577bb5a60aad240a51f3a150af006a42f9a36c32a9053e7790bf9b0e`。
- 在全新父目录 `../fontpixel-local-20261004-qc6uoscg/` 解压。先阅读恢复入口及三个
  Python 脚本，再执行 `verify.py`、`restore.py`、`verify.py --checkout --zip ...`，
  全部 exit 0；schema 2 清单确认 main `5b42ce242c76235143353d5f8e83368fcd2f0595`、
  74 refs、3,233 源码文件、2,290 payload hashes、2,292 ZIP members。
- 现有项目原 HEAD `12710057e9787318d1db70db41a60fca016ad53c` 干净；其 54 refs
  除 main 前进外均保留相同对象。四个只读 siblings 共 1,638 个交付文件与包逐字节相同。
- 用户随后明确允许协调者决定导入/替换方案。采用离线 bundle fetch + fast-forward，
  保留原项目路径和历史。旧 `origin/HEAD` 是符号 ref、包内为同 OID 的直接 ref；
  初次精确 refs 检查因此失败，已记录并按 manifest 修正，再验全部 refs/source/clean
  checkout 与 `git fsck --full --strict` 通过。没有远程访问或 push。
- 导入前完整工作区（含 Git 和已忽略的旧 corpus）备份到
  `../local-work/local-phase6-import-20261004T212805Z/before-import-full-workspace.tar.gz`，
  SHA-256 `593158ab82172f04d0d149765a85815ea9372469e998efbfc2e324496681942a`。
  在独立空目录解开后核对 93,486 文件 hashes，并通过 Git fsck。
- 开始本地修正前，另保存源码 tar、all-ref bundle 并独立恢复校验：
  `../local-work/phase6-local-20261004/checkpoints/before-local-fixes/`。
  这些是本机备份，未声称外部存储；原 ZIP 和新恢复包均保留。

## 本机环境与执行证据

统一产物根：`../local-work/phase6-local-20261004/`。`run_gate.py` 为每次执行建立
唯一目录，保存准确 shell 命令、stdout/stderr、exit、开始/结束时间及全源码前后 hashes。
使用 `set -euo pipefail`、`CARGO_BUILD_JOBS=2`、`scripts/local-env.sh`，移除
`RUSTC_BOOTSTRAP`。只有协调者调度编译与正式测量。

工具链实测：rustc 1.99.0，commit `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`，
host x86_64-unknown-linux-gnu；Cargo 1.99.0、rustfmt 1.10.0、Clippy 0.1.99、
cargo-deny 0.20.2、cargo-fuzz 0.13.2、nightly 1.99.0 (`1a833e16546c2eb012758ddd499964fd8afee29e`)、
Python 3.12.13、Node v24.21.0。未通过私有 cfg 或 bootstrap 启用优化。

| 新执行 | 状态 | 范围 |
| --- | --- | --- |
| 包恢复/导入/原输入/备份恢复 | passed | 完整性，不是实现验收 |
| `cargo fmt --all -- --check` 首次 | failed | 原始 diff 保留在 gates/002-fmt-check |
| `cargo check --locked --workspace --all-targets --all-features` 首次 | failed | Cargo.lock 缺少 toml→indexmap 依赖边，未进入类型检查 |
| 隔离副本 `cargo metadata --offline` | passed | 最小 lock delta 仅新增已有 indexmap 依赖边，无版本更新 |
| `cargo fmt --all` | passed | 完整格式 patch 已保留；本地提交 427ad94 |
| fmt check + 全 workspace/all-target/all-feature locked check | passed with warnings | gates/005；检查期间源码不变，两条 warning 为测试 crate 文档和 example unused import |
| 原参考复制与 oracle 环境清点 | partial | 631 文件复制/hash/source-readback 通过；初次清点尚缺 JS bundle/lock |
| 锁定 esbuild 0.27.7 与 JS bundle | passed | gates/007–008；两份 npm integrity 与 supplied lock 相同，运行在 writable copy |
| nightly rust-src | passed | gate/009；官方 rustup 安装，无 bootstrap |
| flate2 full/minimal feature closure | passed | gate/010；实际均为 miniz_oxide/rust_backend，不能仅由 Cargo.lock 推断 backend |
| 第一批修复 fmt + all-target/all-feature check | passed with warning | gates/011–012；类型检查期间源码不变，仅 Atomic::fetch_update 弃用警告，随后改为 try_update |
| 第一批核心 Rust focused tests | passed | gate/013：lib 53、merge 26、zero-area 3、variants 25、WOFF 16，共 123 |
| 第一批 CLI unit tests | failed | gate/013：91 passed/2 failed；batch 缺失 JSON 结果状态、BDF 预算 fixture 缺必要 header；失败交 owner 修复，后续 integration 因前置失败未运行 |
| scripts/check.sh、完整 tests、rustdoc、feature/example matrix | not-run | 待逐一执行，不用 focused 结果替代 |
| native parity、完整 Tong/catalogue/merge 外部对照 | partial | 新的全量运行见下文；没有复用旧 pass |
| 正式五轮性能与最终源码长 ASAN | not-run | 阈值和输入不变 |

通过的类型检查只对应其日志中源码 hashes，不能证明之后 owner 修改也通过。
Python/WASM 两个 crate 当时仍是保留骨架；workspace check 不代表绑定完成。

## 协作与当前修复

六个 worker 按 W6.1–W6.6 分工，请求 `gpt-6-astra`、`xhigh`、`fork_turns=none`。
运行时模型/backend 和实际服务层没有独立回执，不能确认实际配置。本机 Codex
0.160.0 配置有 `features.fast_mode=true`，但 `service_tier=default`；尝试用
`--strict-config -c service_tier="fast" features list` 校验被 CLI 拒绝（该子命令
不支持 strict-config）。分派工具没有 fast/service-tier 参数，未改全局配置，
未声称成功启用 fast。

当前源级审查与修改包括 standalone source identity ledger、subset caller 预算、
Variants alias 提前拒绝与临时分配、merge metadata/notice 聚合预算、完整 CLI smoke
runner、真实地区 Tong merge fixture、catalogue 全字段/未编码字形对照。发现和
建议在 `owners/w61` 至 `owners/w66`。仅有源码或 Python checker 不算原生验证；
具体结果随后按真实执行更新。本地最终第 6 阶段交付尚未形成。

## 第一批执行后的剩余工作

`checkpoints/before-first-local-test-batch` 与 `after-first-focused-tests` 分别保存
3,243 / 3,244 个源码文件、未提交 diff、全部 refs、成功和失败 raw logs；两个新目录
恢复均通过源码 hash 和 Git fsck。gate/013 包含预先授权的 rustfmt，因此整次
`source_unchanged=false`；它的 before/after 均保留，其他 owner 在该窗口没有写源码。
极端零面积 bitmap 的 bbox/outline 不再按逻辑高度分配或循环；三项边界回归已执行。

完整 source/sidecar identity scope 与各 codec 内部 transient 是不同范围。CLI 第一批
默认总额仍 256 MiB，其中固定最多 1 MiB 用于 standalone identity，默认工作额度
255 MiB；batch 的 pending outputs + identity 共用 512 MiB 规则不变。核心 WOFF
多阶段、variants planner/export、subset 内部 decode/selection 账本尚在继续实现，
不能因局部 buffer 和 focused tests 通过就写为完成，也不作进程 RSS 上限承诺。

本地预计记录 `eta-20261004-2351-80e6` 位于产物根 `eta/`。它使用跨项目历史、默认
judge 校准，无强同项目完成样本；104 个目标每个至少 600 秒是单独的硬运行量。
用户知悉的约 7 小时中位 / 11–12 小时偏保守范围假设六路有效 campaign 和无额度
等待，不是截止承诺；实际失败、性能优化或额度等待会改变它。

## 平台拒绝记录

向 W6.2 分派“104-target 最终源码 ASAN runner/auditor 编写”时，平台在子任务执行前
返回 content safety 拒绝，理由为 possible cybersecurity risk。未执行该子任务的
工具动作。依用户要求停止此动作，不改写、换 agent 或另走执行路径；其余普通编译、
语义对照和预算实现继续。原静态 target 清单保留，但最终 ASAN 仍未完成，因此不能
验收第 6 阶段。准确事件保存在产物根 `asan-dispatch-rejection.json`。先前时间估计
未包含此后发生的平台阻塞，不能作为现时交付承诺。

## 本地完整输入与第二批代码检查（22:33 UTC 更新）

以下均为本次新执行。正式测速尚未开始，生成过程的 elapsed 不充当硬目标数据。
首次 release 二进制冻结在产物根 `frozen-binaries-v1/`，保存每个二进制 SHA、
完整 `source.json` 和 gate/020 的 build result。该源码是 427ad94 加明确列出的
工作区修改，随后形成本地提交 `5e65c3a3c812832b077cebcff639c766d7970573`；
不能把后续工作区改动当成这些二进制已经包含的实现。

| 新执行 | 状态 | 范围与限制 |
| --- | --- | --- |
| gate/014 原 export-rs release | passed | 只在 writable reference copy 构建，原 source 逐文件 hash 读回相同 |
| gate/015 真实 Tong merge reference fixture | passed/external-only | 原 tools/build.py 重建 20 BDF 与基线逐字节相同；这一步未运行 native merge |
| gate/017 第一批回归 | partial | core lib 55、WOFF 16、CLI unit 93 passed；input_budget 7 passed/1 failed，后续集成未执行；失败是旧 fixture 的 identity allowance 提前耗尽，改为有单输入成功控制的双输入累计预算 fixture 后仍需执行 |
| gate/018–019 catalogue plan/reference | passed/external-only | 完整 173 fonts + 8 ZIP；生产 opf_vector 不存在，明确使用 supplied FontTools fallback |
| gate/020 native all-feature release | passed with warnings | 编译期间源码稳定；后续 minimal build 不覆盖已冻结副本 |
| W6.2 native documentation runner | passed | 18 真实 CLI smokes、84 tiny-Tong tasks、13 help spellings；新 CC0 tiny fixture，不能替代完整字体 |
| gate/021 原 Tong 完整构建 | passed/external-only | 8 TTF + 8 OTF + 8 WOFF2 + 20 PCF + 20 OTB + 20 BDF |
| gate/022 catalogue native 完整生成 | passed/generation-only | 173 fonts + 8 ZIP，默认资源额度与完整报告；语义结果由 gate/025 单独判定 |
| gate/024、028 Tong CLI/API 完整生成 | passed/generation-only | 两条路径各 84 artifacts，默认额度，保存完整报告；paired gate/031–032 仍在运行 |
| gate/026–027 Unifont + Latin | passed | native merge/BDF 生成及基础字形对照 |
| gate/030 Unifont 完整来源对照 | passed | 57,106 glyphs，全部 source/index、pixels、metrics、names/notices 与完整报告检查；0 glyph errors、0 report errors |
| gate/029 真实地区 Tong merge | passed | 4 profiles × 20 cases，另含 strict/mono 拒绝控制；完整 native contract 通过，原构建策略差异逐项保留 |
| gate/025 catalogue paired | failed | 173 fonts + 8 ZIP 全部执行；8 font pass、164 unresolved、1 fail；.notdef、名字、字体全局与 strike 字段、部分 BDF/PCF XLFD/SWIDTH 尚未通过语义替换 |
| gate/031–032 Tong CLI/API paired | running, known differences | 初始 PCF 字节/完整物理记录相同；OTB 的 names/global/strike metadata 尚有未决差异 |
| gate/033–034 新预算 batch | passed with warnings | fmt + locked workspace/all-target/all-feature check；仍有未接入 helper dead_code，无 lint suppression |
| gate/035 新预算 focused tests | failed | core lib 67 passed/1 failed；Brotli malformed fixture 未得到预期 InvalidInput，正在定位，后续 integration 未执行 |
| gate/037 Brotli fixture 修正 | passed | 单项回归：低额度截断流 ResourceLimit，足额四类畸形流 InvalidInput，全部 charge 释放；未改生产额度 |
| gate/038 core/CLI integration | partial | core sfnt/subset/variants/cff/woff、CLI input_budget/merge_phase6 全通过；phase6_cli_recovery 36 passed/3 failed，失败原始输出保留 |

新 oracle 清单显式绑定包内九个固定版本；旧项目 parent 的八项 requirements 保持
原样，没有拿旧清单冒充完整环境。清点产物在 `owners/w63/checks-03/`。

Catalogue Spleen 64px 的额外 14ppem web 检查遇 FreeType 2.13.2 error 98。
有限重现已经确认 U+2502 在 native/reference × TTF/WOFF2 四侧同样溢出，64/128ppem
四侧该字形 raster SHA 相同。这只定位消费者限制；原 gate/025 failure 保留，不能由
单字形重现替代完整 all-GID、实际字号的 web 检查。

新预算实现覆盖部分 sfnt directory/cmap/subset 输出存储、固定 miniz 0.9.1 流式
workspace 与 Brotli 6.0.1 decoder 分配。Cargo.lock 只增加已有依赖边，没有更新版本。
Variants/WOFF 多阶段、subset 内部 GSUB/glyf/CFF 等仍有未接入的存储；Brotli encoder
尚不具备可传播的动态分配失败路径。局部 admission 成功不是全 codec 或进程总 cap。

`checkpoints/before-codec-budget-batch4` 首次 evidence 打包遇运行中的 OTS 临时目录
删除，失败包和记录保留。`before-codec-budget-batch4-retry1` 改用同一打开句柄捕获
文件内容后，3,249 个源码文件、全部 refs 独立恢复通过，4,349 evidence members
读回通过；四项正在运行的 gate 明示为 partial，未冒充最终完整日志。所有备份仍为
本机保存，未向外分享文件。

随后 gate/029 完整 80 个 case 全部通过 native contract，原构建策略差异逐项保留，
未以共同子集替代。`owners/w66/tong-native-01/summary.json` 绑定冻结二进制 SHA，
汇总 900 artifacts。gate/036 独立诊断确认正常 5-byte Brotli fixture 在 1/4/8 MiB
均解码成功，而截断输入在 1 MiB 先触发 ResourceLimit；原失败断言及日志保留，
下一步分开验证低额度拒绝与充分 workspace 时的 InvalidInput，生产 cap 不变。

另一独立子任务“Brotli encoder 可失败分配源码分析及 vendor 草稿”也在任何工具
动作前被平台以 possible cybersecurity risk 拒绝。已停止，通知其他 owner 不接替
该被拒补丁；不更换 agent 或执行路径来重试。记录为
`brotli-encoder-dispatch-rejection.json`。因此 encoder 动态分配闭环仍未实现，
与 ASAN 一并是最终验收的明确阻塞；已有 decoder 及项目自有代码的普通修复继续。

gate/038 的三项待修正回归是 strict charset 退出码为 2 而预期 3、embedded 控制
fixture 出现 duplicate physical strike size、validate cmap fixture 的 stderr 未命中
预期 limit。尚未确定其最终修正，不能删断言或抬生产 cap 来通过。具体测试名及行号
都在 gate 原始日志；源码检查和测试失败分别记录。

`checkpoints/after-codec-budget-first-integrations` 再次完整保存并独立恢复 3,249 个
源码文件/全部 refs，4,725 evidence members 全部读回；为降低频繁完整备份的 CPU
成本，证据改为未压缩 tar，源码仍保留压缩包。只有 gate/031–032 尚未完结并标为
partial。这次备份也只是本机保存，不能称为异机副本。

## 2026-10-05 恢复（Claude Code，Opus 5.5）

用户 2026-10-05 明确「恢复，只做第六阶段」。恢复前核对：当前源码、HEAD 与 refs 与暂停
备份 `checkpoints/user-stop-20261004T2250Z-final/source.json` 逐文件完全相同。

### 验收范围决定（用户选择）

PLAN.md §9.6 只要求解压大小、字形数、单字形像素数、表大小等有可配上限、超限返回错误，
加每个读取器 ≥10 分钟模糊测试。用户 2026-10-05 选择**按 PLAN 原文验收**：

- 资源：保留现有可配上限及其测试，补齐每类上限至少一项测试；各 codec 内部（WOFF/WOFF2、
  variants、subset 内部表、Brotli 编码器堆）的完整分配账本**不再作为验收条件**，
  在文档中作为已知限制写明。不降低任何现有上限或 strict/loss/报告行为。
- catalogue：保留 SWIDTH 投影并作为 CLI/batch 显式选项；其余与原 Python 下载流程的差异
  按已有 181 项分类写入迁移说明（PLAN §11 允许写明差异和原因），不再要求逐项追平。

此前云端/Codex 协调者审计加入的「全账本」「逐项语义追平」门槛来自交接清单，不是 PLAN 原文。

### 源码修正

- 新增 `crates/fontpixel/src/projection.rs`（暂停时已登记模块但文件缺失）：
  `apply_bitmap_projection` 以精确整数 half-even 把每个物理字形（含 unencoded、重复码位）
  的水平 SWIDTH 设为 `DWIDTH*units/ppem`；先验证并暂存全部改动，再无分配提交，失败时
  FontSet 不变；覆盖有效源值记 Loss、补缺记 Repair；返回逐字形审计。6 个单元测试 +
  1 个 doctest。CLI `convert --swidth-ppem N [--swidth-units U]`，batch 用
  `[tasks.options] swidth_ppem`；新增 `crates/fontpixel-cli/tests/projection_cli.rs` 5 项。
- 暂停时未编译的草稿（working/outline/GSUB/subset/CLI 回归修正）补上 projection 后**一次
  编译通过**，没有出现级联错误。CLI 三个回归修正首次重测即通过。
- **已提交的 `a1b40b0` 本身存在、此前未被发现的问题**（不是暂停草稿引入）：
  1. CLI 单元测试 `direct_sfnt_is_counted_once_and_codec_errors_precede_strict_loss` 失败：
     `a1b40b0` 让 `Sfnt::parse` 按 `max_total_bytes` 建工作预算，而 CLI 直出 sfnt 路径把
     全部总额度交给 parse，忽略 CLI 已持有的 sfnt/报告/aux。修正：parse 只拿剩余额度
     （与 WOFF 路径一致）；测试改为精确边界：先独立测出目录校验所需 working，再断言
     `exact+working-1` 资源超限、`exact+working` 得到 StrictLoss，以此证明 sfnt 只计一次。
  2. `cargo clippy -D warnings`（两个 profile）在已提交代码上就不通过：assets/batch/main/
     p3_formats/commands（8 参数）、merge 与 model/budget、sfnt/cmap 测试、
     catalogue export-name 示例、outline spec（8 参数）。均按 lint 做等价改写，无 allow。
  3. `RUSTDOCFLAGS=-D warnings cargo doc` 不通过：`charset/merge_categories.rs` 裸 URL。
  4. 未用的 `working::Vector::capacity` 删除；`tree_bytes/reserve_tree` 仅 sfnt 与测试使用，
     加 `cfg(any(feature = "sfnt", test))`。

### gate 与备份

| gate | 结果 |
| --- | --- |
| 042 `scripts/check.sh`（生成表、fmt、两个 Clippy、workspace 全/默认测试、最小/BDF+PCF/CJK、deny） | exit 0，source_unchanged=true；688 个测试套件共 3,453 项通过；deny advisories/bans/licenses/sources ok |
| 043 check.sh + `-D warnings` rustdoc + doctests（含 projection CLI） | exit 0；693 个测试套件共 3,480 项通过；rustdoc/doctest/deny 通过；期间只有本记录文件变化 |
| 044 冻结 v2 release 二进制 | `WORK/frozen-binaries-v2/manifest.json`；构建期间只有 `fuzz/Cargo.lock` 变化 |
| 100–121（验证 subagent） | 用 v2 完成全部真实字体对照，见 `WORK/full-correctness-v2/v2-gate-index.json` 和 [第 6 阶段报告](phase-6.md)；无原生缺陷，输出与 v1 逐字节相同 |
| 045 最终冻结 v3 | 代码与 v2 相同，树中加入文档、fuzz 种子与 campaign 脚本；因 `include_str!` 的 rustdoc 文档变化，二进制哈希不同 |
| 046 v3 通格 batch 等价 | 84/84 产物与 v2 哈希完全相同 |
| 047 W62 文档 CLI 冒烟回放（v3） | 18 个 case 全部 pass，源码前后一致 |
| 048 最终 ASAN campaign | 104/104 目标 PASS，各约 611 s，301,696,538 次执行，rss 上限 2048 MB，源码前后一致；`WORK/fuzz-final/phase6-final-20261005T040104Z/` |
| 049 正式五轮测速 | 22 个 case × 5 = 110 个进程，全部 exit 0，源码前后一致；`WORK/perf-final/run-01/` |
| 050 | 审计命令参数写错（`--result`），exit 2，未执行审计；保留记录 |
| 051 独立测速审计 | pass：2,893 个冻结文件、110 次调用、220 个原始流、9,215 个归档产物哈希、22 行中位数独立重算 |

### subagent 分工

用户 2026-10-05 允许按需使用 subagent。三个 subagent 分别负责：ASAN 准备（盘点、
`scripts/phase6_fuzz_campaign.py`、补种子 `fuzz/seed_phase6.py`）；文档（迁移说明、
格式页面核对、过时状态清理，清单在 `WORK/docs-stale-status.md`）；真实字体对照重跑
（gate 100–121）。源码修正、冻结、ASAN 与测速的启动由协调者执行。

`checkpoints/resume-042-green-20261005/`：源码 3,255 文件 + 全部 refs 空目录恢复校验通过
（本机备份，非异机）。

### 非正式性能探测（单次，非正式五轮）

本机 20 核 / 62 GB。`phase6_timing native`（常驻输入，一次 warmup 后一次计时）：
parse 51 ms、public parse 48 ms、BDF→PCF 84 ms、→OTB 118 ms、→YAFF 135 ms、
→方点 TTF 434 ms。gate 024（与其他 gate 并行时）完整 Tong 默认 batch CLI
pipeline_wall 30.3 s。均只是方向性观察，正式结论以冻结后安静窗口的五轮测量为准。

### 测速结果

PLAN §10 全部硬目标达标（五轮中位数）：解析 53.3 ms；→PCF/OTB/yaff 88.2/119.9/144.9 ms；
→方点 TTF 449.4 ms；通格默认 batch 32.2 s；57,106 字形拼合 0.36 s；收录站全部下载
5.50 s（原 Python 43.54 s）。完整表格见 [benches/REPORT.md](../benches/REPORT.md)。
