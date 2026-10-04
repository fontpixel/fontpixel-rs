# Phase 6 reconstruction — 2026-10-04 19:35 UTC

The execution workspace became unavailable around 18:33 UTC. This checkout was
freshly restored from the verified original phase-6 handoff archive: phase-5 main
`12710057e9787318d1db70db41a60fca016ad53c` and all 54 original refs. The missing
later local commits have NOT been restored as Git objects. Recovered code is being
reapplied or rewritten with the owner's explicit authorization at 19:09 UTC.

## Current acceptance boundary

- Phase 6 is **not accepted or delivered**. Restored/reconstructed Rust source is
  uncompiled and untested in this environment. Historical passing logs are lost;
  their recorded summaries do not establish present correctness.
- The original input ZIP and all prior verified glyph backup bytes remain intact.
- Source recovery packets and coherent reconstruction checkpoints are externally
  backed up in Library before subsequent major iterations. The evolving recovery
  packet is not a complete runnable release or acceptance deliverable.
- Cargo toolchain download from the official host is network-policy blocked,
  including a supported elevated retry. Only rustc installation partly completed;
  no alternate access bypass is authorized or attempted.
- Source reconstruction is active with six engineering workers. Pending shared
  integration, tests and workload tools are explicitly tracked in each packet.
- The prior OTB/YAFF/Tong performance misses remain unresolved. Full performance
  gates are not waived; any new implementation needs fresh five-run evidence.
- Long final-source ASAN remains explicitly deferred to the owner's computer,
  as previously approved. Do not run a blocked action through another route.


## Full source checkpoint freeze — 19:35 UTC

All active source owners confirmed a safe write freeze. A new explicitly
unvalidated WIP main commit and full source/history archive are being prepared.
The original later phase-6 commit IDs remain missing; this is a new reconstruction.

Known incomplete work includes new identity-ledger tests, the catalogue native
companion executable, several full-workload comparison adapters, final integration
review, toolchain restoration and every required Rust/compiler/oracle/performance
acceptance gate. Do not interpret frozen source or archive integrity as correctness.

## Next required milestones

- [ ] Finish source reconstruction, inspect every gap and conflict, retain exact
  provenance labels for byte-identical recoveries versus reimplementations.
- [ ] Restore permitted tooling, run formatting and full compiler/feature tests,
  fix failures, perform independent output and compatibility oracles.
- [ ] Complete phase-6 performance targets, fresh evidence and final source audit.
- [ ] Produce a verified all-ref bundle and complete single ZIP, externally save
  it and report actual remaining limitations before claiming phase acceptance.
- [ ] Only after phase-6 delivery, continue authorized phase 7 then phase 8 with
  full native Python/WASM/JS APIs plus separate bdfparser compatibility coverage.
  No phase 9, remote push, package publication or site deployment.

The older handoff text below is baseline provenance and does not override the
owner's later continuation, full-API or reconstruction instructions.

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
