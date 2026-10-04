# 当前进度：第 6 阶段本地接续中，尚未验收

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
