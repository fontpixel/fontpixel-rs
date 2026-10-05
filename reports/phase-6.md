# 第 6 阶段交付报告：CLI 完善、批量、文档、测速、健壮性、拼合

日期：2026-10-05。用户在 2026-10-05 恢复本阶段，范围只做第 6 阶段。按 PLAN.md 原文
验收（用户当天的决定，见下文「验收范围」）。第 7、8 阶段尚未开始，第 9 阶段未授权；
没有 push、publish 或 deploy。

逐轮过程、每个 gate 的命令与结果见 [本地接续记录](phase6-local.md)；暂停前的历史
状态见 [暂停记录](../docs/LOCAL-PAUSE-20261005.zh.md)。本地证据目录为
`../local-work/phase6-local-20261004/`（下称 WORK），gate 记录在 `WORK/gates/NNN-*/`，
每个都有命令、源码前后快照、完整输出和退出码。

## 结论

第 6 阶段的 PLAN 工作项 W6.1–W6.6 全部完成并通过验收：

- 完整 `scripts/check.sh` 通过：生成表、fmt、两个 Clippy profile `-D warnings`、
  workspace 全 feature/默认/最小/BDF+PCF/CJK 测试、`cargo deny`。另有无警告 rustdoc
  和 doctest。
- 用冻结的 release 二进制完成全部真实字体对照，没有发现原生缺陷。
- 104 个 fuzz 目标各跑 ASAN 10 分钟以上，全部无崩溃。
- PLAN §10 全部性能硬目标在正式五轮测量中达标。

## 验收范围（用户 2026-10-05 决定）

PLAN §9.6 原文要求：解压大小、字形数、单字形像素数、表大小等有可配上限且超限报错；
每个读取器一个 fuzz 目标、各跑 ≥10 分钟；proptest 往返；clippy/fmt/deny 通过。
这几项均已满足。

此前云端/Codex 协调者审计额外加入了两项门槛，它们不是 PLAN 原文，用户选择不作为验收条件：

- 各 codec 内部（WOFF/WOFF2、variants、subset 内部表、Brotli 编码器堆）的完整字节账本；
  作为已知限制写入 [writer budget](../docs/w6.1-writer-budget.md#known-limitations-accepted-for-phase-6-2026-10-05)。
- catalogue 下载与原 Python 输出逐项追平；差异按类别写入
  [迁移说明](../docs/migration-fontpixel.md)（PLAN §11 要求写明差异和原因）。
  SWIDTH 约定可用新选项 `--swidth-ppem` 复现。

## W6.1–W6.6 验收

| 工作项 | 验收证据 |
| --- | --- |
| W6.1 `batch`/`diff`/`validate`/`charset`，帮助与示例 | 12 个子命令的 `--help` 都有示例；CLI 单元与集成测试全部通过；W62 文档回放用冻结 v3 二进制执行 40 条命令全部符合预期（18 个 CLI 冒烟 case、84 个 tiny 产物、各子命令帮助），gate 047；新增 `convert --swidth-ppem` 及 batch `options.swidth_ppem`（5 项 CLI 测试） |
| W6.2 文档 | README、API、99 个格式页面齐全（`fontpixel formats --json` 的 99 个 ID 都有页面）；README 和 docs 中 105 处 CLI 示例的参数都与当前 `--help` 核对；`RUSTDOCFLAGS=-D warnings cargo doc` 和 doctest 通过；workspace `missing_docs` 配合 Clippy `-D warnings` 保证公共项都有说明；两份迁移说明已更新（catalogue 差异按类别和数量列出） |
| W6.3 测速报告 | [benches/REPORT.md](../benches/REPORT.md)：22 个 case 各五个新进程，独立审计通过（gate 049/051），硬目标见下表 |
| W6.4 健壮性 | 104 个 ASAN fuzz 目标覆盖全部 99 个读取器 ID，各跑约 611 秒，共 301,696,538 次执行，0 崩溃/超时/OOM/泄漏，源码前后一致（gate 048）；逐目标执行次数与覆盖率见 [phase6-validation/fuzz-final.md](phase6-validation/fuzz-final.md)；各类资源上限都有测试；proptest 往返测试保留 |
| W6.5 拼合（库） | merge 单元/集成测试通过；Unifont 57,086 + Tong Latin → 57,106 字形，字形证明与来源审计 0 错误（gate 105–107） |
| W6.6 拼合（CLI） | 通格真实地区拼合 4 个 profile × 20 个 case = 80/80 通过，900 个产物，0 字形/报告错误（gate 104）；57,106 字形拼合并写 BDF 0.36 s |

## PLAN §10 性能（正式五轮中位数）

测试机：i7-12700H，20 逻辑 CPU，62 GB，Linux 7.0.0。测量时没有构建、测试或其他验证
任务；桌面会话（文件索引、浏览器）未关闭，如实记录在测量结果里。

| 任务 | 目标 | 中位数 |
| --- | --- | --- |
| 解析 SC 38,653 字形 | ≤ 100 ms | 53.3 ms |
| → PCF / OTB / yaff | 各 ≤ 200 ms | 88.2 / 119.9 / 144.9 ms |
| → 方点 TTF | ≤ 1 s | 449.4 ms（CLI 全流程 653.6 ms） |
| 通格整套默认导出 | ≤ 40 s | 32.2 s（`fontpixel batch` 默认） |
| 57,106 字形拼合写 BDF | < 1 s | 0.36 s |
| 收录站全部下载 | 报告 | 5.50 s，原 Python 43.54 s |

与 bdfparser、bdfparser-js、fontTools、export-rs 的逐项对比，包括兼容层哪些操作反而更慢，
见 [benches/REPORT.md](../benches/REPORT.md)。

## 真实字体对照（冻结二进制 v2，gate 100–121）

所有原生输出与 v1 逐字节相同；v3 只因文档变化重编译，通格 84 个产物与 v2 逐字节相同
（gate 046）。

| 对照 | 结果 |
| --- | --- |
| 通格 batch CLI 与 API，各 72 条记录 | 20 PCF FreeType 逐像素通过；20 OTB FreeType 逐像素全部一致（633,028 个字形），这正是 PLAN §9.4 对 OTB 的要求（「FreeType 逐像素一致」）。OTB 另有元数据差异（见下节），不在 §9.4 要求内；16 个矢量字体完整几何/度量/14,592 批 shaping 通过；16 个网页字体通过（8 个 OTS 拒绝全部来自原导出器） |
| 通格地区拼合 | 80/80 通过 |
| Unifont 拼合 | 通过 |
| SC 编解码 | 11/11，Rust、Python、Node 七个产物逐字节一致 |
| SC 矢量、同一 sfnt 网页字体 | 38,654 字形全量几何与 912 批 shaping 通过；方点/圆点网页字体通过（首次执行） |
| 收录站 173 个字体 + 8 个 ZIP | 8 个完全一致，164 个只有已分类差异（无新类别、无像素/字形集/cmap 差异），1 个是 Spleen 64px 在 FreeType 14ppem 下 U+2502 光栅溢出（消费端限制）；8 个 ZIP 打包结构全部通过 |
| bdfparser 差分（legacy parity） | 67 个字符集表一致；2,000 random + 1,000 random_all + 41 探测 + 251 个语料文件：0 个新差异，7 个属于已登记的 K12-ragged-bitmap |

PLAN §9.4 对矢量字体要求 cmap、字形数、hmtx/hhea/OS/2 关键字段、name 一致，轮廓栅格化逐像素一致，HarfBuzz `locl` 塑形一致；验证脚本 `benches/phase6_validate.py` 对这些逐项断言，16 对全部通过。

### 通格 OTB 元数据差异（PLAN 未要求，如实列出）

两条路径的 20 个 OTB 与 export-rs 相比，在以下字段不同（20 个文件一致）：OS/2 achVendID、
panose 4 项、ulUnicodeRange1–3、usBreakChar（10 个文件还有 xAvgCharWidth）；head
checkSumAdjustment、created/modified、flags、lowestRecPPEM；post formatType/extraNames
（10 个文件还有 isFixedPitch）；EBLC 的 widthMax 与索引子表组织。名字表方面，原生 OTB 只写
name ID 0、1、2、4、5、8，export-rs 还写 3、6、13、14、16、17。**其中 13/14 是许可证说明与
许可证 URL**，对 OFL 字体有实际意义；PLAN 没有要求，但若希望 OTB 也带这两条记录，可作为
后续改进。

## 本次恢复中修正的问题

- 补写 `projection` 模块（暂停时已登记但文件缺失），实现 SWIDTH 投影的事务、审计与报告。
- 已提交的 `a1b40b0` 中 CLI 直出 sfnt 时把全部额度交给 `Sfnt::parse`，忽略 CLI 已持有的
  数据，导致 1 项 CLI 单元测试失败；改为只给剩余额度，测试改为精确边界证明 sfnt 只计一次。
- 已提交代码在 `-D warnings` Clippy（两个 profile）和 rustdoc 下不通过，均做等价改写修正，
  没有使用 allow。
- 7 个 fuzz 目标的种子过弱，补充了可复现的小种子（`fuzz/seed_phase6.py` 与 manifest，
  只用 MIT/OFL 来源）。

## 已知限制（如实保留）

- codec 内部完整字节账本未覆盖的路径见上文链接；这些路径都受输入、表、字形、像素、点数
  上限约束，也都在 fuzz 范围内，但没有一个覆盖全部同时存活临时分配的统一字节上限。
  所有上限都不是进程 RSS 上限。
- catalogue 与原 Python 的差异（名字表、OS/2/head 标志与时间戳、`.notdef`、未编码字形、
  BDF/PCF 属性、校验和派生值等）按类别写入迁移说明。fontpixel 保留原 Python 会丢弃的
  COMMENT/版权文字，这是有意保留，不是缺陷。
- 收录站生产用的 `opf_vector` 扩展没有提供，矢量对照与测速使用原 fontTools 后备实现。
- 正式测速在用户的桌面工作站上进行，桌面会话保持打开。
- 默认 batch CLI 与默认 API 示例的 84 个通格产物中 76 个逐字节相同；8 个 WOFF2 不同，
  因为 CLI 默认用 2 个 Brotli 线程压缩，API 示例用 1 个；8 个都用 fontTools 解压后
  得到逐字节相同的 sfnt，只有压缩流不同。
- 少量源码注释仍是暂停前的过时状态措辞（例如 `crates/fontpixel/src/lib.rs` 模块说明
  「Current phase-6 reconstruction is unvalidated」、`docs/api-rustdoc.md` 中的状态句，
  清单见 `WORK/docs-stale-status.md` C 节）。改动它们会改变编译产物、使冻结二进制的证据
  绑定失效，所以留到第 7 阶段开始时一并修改。

## 冻结与交付源码的关系

ASAN（gate 048）、测速（049/051）和文档回放（047）都在冻结 v3 的源码树上执行，前后源码一致。
冻结之后到交付只改了文档与报告：README.md、TODO.md、benches/REPORT.md、
benches/phase6-README.md、docs/api.md、docs/recovery-status.md、docs/w6.1-cli.md、
docs/w6.1-writer-budget.md、reports/ 下的报告和 fuzz 结果表；`crates/`、`Cargo.*`、`fuzz/`、
`scripts/` 和测试均未改动，所以冻结二进制的证据适用于交付源码。

## 备份

全部源码、refs、未提交改动与证据在 `WORK/checkpoints/` 下做了本机备份并在空目录独立恢复
校验。只是本机副本，没有异机备份。本阶段改动尚未提交到 Git。
