# 2026-10-05 暂停记录：第 6 阶段严格标准补完进行中

用户 2026-10-05 下午指示「把当前正在做的做完就停止、休息」，并要求把进度记录到 TODO 或其他文件。
本文件是恢复时的入口。第 6 阶段按 PLAN 原文的交付见 [reports/phase-6.md](../reports/phase-6.md)。
之后用户要求「按更严的标准来」并「一干到底」，包括第 7、8、9 阶段；本地提交可以，不 push、
不发布、不部署。严格标准的定义写在 Claude 项目记忆 `phase6-acceptance-scope` 和下文第 1 节。

## 1. 严格标准是什么

- **全账本**：每个 codec 的每一块自有分配都在分配前计入调用方的同一预算，并在存储释放后才退还。
  覆盖 WOFF/WOFF2、subset 内部全部表、variants 规划/导出和轮廓。Brotli 编码器的堆内存用
  「按依赖源码推导的上界，调用前预留，计数分配器实测证明」的方式处理，不修改或 vendor 依赖。
- **追平**：默认行为和默认输出不变；加上显式选项后，收录站和通格 OTB 与原程序的每一处差异
  要么消除，要么证明属于以下三类之一：原程序有损（不能照抄，例如丢 COMMENT/版权）、派生值
  （校验和、时间戳）、消费端限制（Spleen64 在 FreeType 下光栅溢出）。
- **前提**：任何默认上限都不能提高。默认的通格（≤40 s）和收录站负载必须仍然通过。

## 2. 当前状态（main 已提交）

| 提交 | 内容 |
| --- | --- |
| `6297998` | 第 6 阶段按 PLAN 原文交付 |
| `9786644` | 合并单元 E：收录站投影扩展与追平选项 |
| `865ca55` | 合并单元 B：subset 内部全部分配进同一账本 |
| `ca25acf` + `71a6064` | 合并单元 D：OTB/bdat 完整元数据选项；整合修正（`runs.rs` 改为从 sfnt 导入 buffer/p16；`convert` 读取前预留 OTB 元数据） |
| `0a135fe` | 合并单元 A 的 `bc024c8`+`5146bde`：WOFF1/WOFF2 全生命周期账本。**Brotli 上界预留未合并**，见第 3 节 |

每次合并后都跑了完整 `scripts/check.sh`（gate 在 `../local-work/phase6-local-20261004/gates/`）：

| gate | 结果 |
| --- | --- |
| 052（合并 E 后） | exit 0，690 个测试套件共 3,524 项通过 |
| 053（合并 B 后） | exit 0，690 个套件共 3,554 项通过 |
| 054（合并 D 及整合修正后） | exit 0，692 个套件共 3,584 项通过 |
| 055（合并 A 账本部分后） | exit 0，697 个套件共 3,605 项通过 |
| 056/057 | 当前 main 的 release 版跑默认通格整套导出：exit 0，84 个产物与 v2 逐字节相同 |

`source_unchanged=false` 的 gate 只是因为 subagent 在各自工作树提交时改变了 Git refs，源码文件逐个核对没有变化。

**测速噪声**：057 的 pipeline_wall 为 45.3 s，随后同一时刻对比，新版 29.0 s、冻结 v3 43.7 s。
差异来自桌面负载（localsearch 索引约 38% CPU、Chrome），不是性能退化。最终正式测速必须在
确认空闲后重做。

### 各单元结果

| 单元 | 分支 / 工作树 | 结果 |
| --- | --- | --- |
| A WOFF 账本 + Brotli 上界 | `strict/unit-a-woff-ledger`，`.claude/worktrees/agent-a12da3e7a8c1e1baf` | 账本部分已合并；Brotli 上界预留（`73eb8bc`、`1facfb7`）**未合并** |
| B subset 账本 | `strict-unit-b-subset-ledger` | 已合并。255 个 subset 案例与 30 个 CFF 输出逐字节不变；默认预算内最大约 86 MiB（OTB SC 全字形）。未完成：CID 多 FD 共用 Private 的专门测试（缺 fixture）；CBDT PNG 缓冲在生成后才计费（subset 不走 CBDT） |
| C variants/轮廓账本 | `strict-c-variants-ledger`，`.claude/worktrees/agent-a4e30812c86ee0dbb` | **未合并**，见第 3 节 |
| D OTB/bdat 元数据 | `strict-d-embedded-metadata` | 已合并。parity profile 下 20 个通格 OTB 只剩 `head.checkSumAdjustment`（写表顺序不同，派生值）；收录站 21 个位图 TTF 的大部分元数据也已追平 |
| E 收录站投影 | `strict/unit-e` | 已合并。explicit profile 下未解决字体 164 → 132，BDF/PCF 通过 8 → 40 |

各单元的验收清单在仓库 `strict/ACCEPTANCE-{A,B,C,D,E}.md`。合并进 main 的只有 B、D、E 三份和
A 的早期版本；A 的最新版和 C 的清单在各自分支上。证据：`../local-work/strict-e/`、
`../local-work/strict-D-evidence/`。

## 3. 两个已决定的处理方案（恢复后先做）

**A：Brotli 编码器上界。** 按源码可证明的上界在质量 10–11 时非常保守：完整通格约 13.5 GiB，
实测堆约 163 MB；如果预留，默认 256 MiB 下真实字体的 WOFF2 几乎都会失败。用户同意按协调者建议：

1. 先限时尝试更紧的证明：根据分块与聚类的实际结构，界定直方图数量随输入长度的上限。
2. 证明不出来就退一步：质量 ≤9 用可证明上界；质量 10–11 预留实测峰值乘安全系数，文档写明
   这是经验上界而不是证明上界，并由计数分配器测试覆盖。
3. 不走修改或 vendor 依赖的路线。

**C：variants 导出。** 严格账本下，默认 CLI `variants` 导出完整通格 Large 报 `ResourceLimit`：
CLI 为 5 个地区源字体保留约 252 MB，导出还真实需要 98–255 MB（实测 RSS 本来就超过 256 MB）。
处理方案：规划阶段已把需要的位图复制进 plan，规划完成后真正释放源 FontSet，并精确退还它们
实际占用的额度（不靠猜测），使导出时的真实内存和账本都下降，不调高任何上限。完成后再合并 C。
合并时注意 E 的 `.notdef` 提交（`0e45bc6`）也改了 `variants/export.rs`。

## 4. 恢复后的顺序

1. A 的 Brotli 上界（第 3 节），合并 A 剩余提交。
2. C 的源模型释放（第 3 节），合并 C；按 C 报告把 `variants/tables.rs` 的保守上界逐步换成直接
   接收账本的编码器；`ledger::tree_nodes` 改为复用 `model::budget::tree_node_bound`。
3. 后续小单元 F：矢量字体（variants 导出）的 name 记录、OS/2 fsType/fsSelection/usBreakChar、
   head created/modified 显式选项，CLI 语法复用 D 的名字表参数；`.notdef` 策略接到 `convert --to ttf`。
4. 已知的小问题：
   - `batch.rs` 的 `with_source_scope` 在精简 CLI feature 组合下是死代码，会让该组合的 Clippy 失败；
   - A 报告 `api_quickstart` 示例在 `6297998` 上就在第 169 行（PCF 断言）失败，需要核实并修复；
   - `projection_cli`/`phase6_cli_recovery` 测试在工作树中运行会在 `.claude/worktrees/local-work/` 下留空目录。
5. 文档：
   - `docs/w6.1-writer-budget.md` 的已知限制按最终结果改写，WOFF 和 subset 两条已关闭；
   - 两份迁移说明里「Option: none」的行已经过时，改成新选项；
   - `lib.rs` 等源码注释中的过时状态措辞也一并改掉。
6. 最终冻结后一次性完整复验：
   - 对照全量重跑，加上 D、E 的新 parity profile；
   - 104 目标 ASAN（`scripts/phase6_fuzz_campaign.py`，约 1 小时 40 分钟）；
   - 确认机器空闲后跑正式五轮测速：`scripts/phase6_perf_prepare.py` 需要显式传入
     `--requirements ../local-work/phase6-local-20261004/performance-work/requirements-oracles.txt`；
   - 更新 `reports/phase-6.md`。
7. 第 7 阶段 Python、第 8 阶段 WASM、第 9 阶段拼合网页，都要求完整原生 API，范围见
   [bindings-scope.md](bindings-scope.md)。工具已就绪：`wasm32-unknown-unknown` target、
   `../local-work/py-bindings-venv`（maturin 1.15、pytest）、Docker（manylinux）、Node 24、wasm-pack。
   CLI 编排要先抽成库 API。

## 5. 其他记录

- 5 个工作树保留在 `.claude/worktrees/`（已加入 `.git/info/exclude`），不要删除：A、C 的未合并提交在里面。
- 单元 A 报告：会话 scratchpad 的 `base/` 目录曾被它用 `6297998` 的归档覆盖过。其他单元的证据都在
  `../local-work/` 下，不受影响。
- 时间估算 `eta-20261005-1010-9f3d`（严格补完）还在进行中。上一条估算记录因追踪工具漏算 subagent
  时间已撤销。
- 本机备份见 `../local-work/phase6-local-20261004/checkpoints/` 中最新的 `strict-pause-*` 目录。
