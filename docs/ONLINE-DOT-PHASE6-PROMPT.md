# 发给在线 dot 的接续 prompt

我授权你恢复附件中的 fontpixel-rs 项目，继续并完成 **第 6 阶段全部工作项 W6.1–W6.6**。不要从零重做项目，不要停在计划或能力说明；自行推进、修复、验证、提交和打包。常规实现选择不要问我，记录在 `docs/decisions.md`。第 6 阶段验收交付后停止，不自动开始第 7–9 阶段，不推送远程、不发布包、不部署网站。

先在全新空父目录中，把所有交接分卷解压到同一目录，禁止覆盖式解压已有项目/参考资料。阅读 `START-HERE.md`，运行包里的校验/恢复脚本。恢复后目录中 `fontpixel-rs/`、`reference/`、`samples/`、`validation-inputs/`、`original-task/` 必须互为兄弟目录。以 `HANDOFF-MANIFEST.json` 的 main 提交和源码校验为准；不要恢复到旧阶段分支。

你是协调者。**开启 6 个并行 worker，每个 `model="gpt-6-astra"`、`reasoning_effort="xhigh"`、`fork_turns="none"`，启用 fast mode。** 用环境真实支持的 fast/服务等级设置；必须记录请求参数及实际可核实状态。如果接口没有 fast 设置或无法核实，就如实写“fast mode 无法设置/核实”，不要谎称已启用，也不要擅自降低模型或 reasoning effort。同时最多 6 个 worker，不允许再嵌套派生 worker。

恢复后先读：

1. `fontpixel-rs/TODO.md`、`fontpixel-rs/docs/ONLINE-DOT-HANDOFF.md`、第 5 阶段最终报告；确认前置验收已完成。
2. `original-task/DOT-README.md` 的执行规则。
3. **仓库当前** `fontpixel-rs/PLAN.md`、`FORMATS.md`，以及 `docs/plan-addendum-merge.md` 全文。原始任务目录里的 PLAN/FORMATS 是历史副本，不能覆盖仓库扩展后的计划和 99 格式清单。
4. `docs/decisions.md`、`docs/bitmap-model.md`、当前 CLI 和相关库接口，再分配文件所有权。不要只按这个 prompt 的摘要实现，完整验收以 PLAN 和追加规格为准。

工作分配如下；六名 worker 可同时做各自独立部分，涉及接口依赖先由协调者统一契约：

| Worker | 工作项 | 范围和验收重点 |
| --- | --- | --- |
| 1 | W6.1 CLI / batch | 完成 TOML 并行任务、每任务及整体 JSON 报告、`diff`、`validate`、`charset`、全部帮助和示例。遵守既有退出码 0/1/2/3、严格模式、资源预算、输入保护、目录/多资产事务输出。为 worker 6 预留真正的 merge 批处理接入。 |
| 2 | W6.2 文档 | 安装/快速上手/每命令示例、公共 API 说明和示例、无警告 rustdoc、全部格式文档、`migration-tong.md`、`migration-fontpixel.md`。逐项映射旧工具/manifest 参数，写出输出差异，不改那两个参考项目。 |
| 3 | W6.3 性能 | 完成 PLAN §10 的整个报告和可重复脚本：大字体解析/转换、通格整套导出、真实收录站批量、Python/JS/fontTools/export-rs 对照及小调用。接入 worker 6 的大字体拼合测速。必须真实五次中位数、内存、CPU、版本、输入/二进制/源码 SHA；不把阶段 5 的小样本测速冒充完整目标。 |
| 4 | W6.4 健壮性 | 盘点所有读取器与 target 对应关系，完成至少十分钟以上有效 ASAN、属性测试、预算边界、CLI/共享路径覆盖。阅读阶段 5 的 fuzz-source-audit 和 scaling review，处理最终共享代码变化及 PFR 共享物理表重复解析；改动后重新冻结相关源和检查，不能把旧源码长测算成新源码结果。 |
| 5 | W6.5 库拼合 | 实现 `fontpixel::merge` 全部规则：有出处的 cjk/western/geometric 分类和范围排除；first-selected-present/fallback；baseline/dy/ascent/descent；prop/mono/auto 全分支；必填新家族名；部件版权/许可/保留名与 strict；完整 JSON/文字报告及合成字体回归。保留位图/映射/度量，不做隐式阈值或字符集猜测。 |
| 6 | W6.6 CLI 拼合 | 实现追加规格的 `fontpixel merge`，`--take`/`--dy` 绑定紧前部件，多 `-o` 输出、矢量参数、完整报告及 strict，真正接入 batch。通过通格逐字形等价对照和约 5.7 万字拼合写 BDF 的性能验收。 |

协调者负责 `main`、共享 Cargo/registry/CLI 分发和集成；worker 不得同时改同一个共享文件。优先在同一 checkout 按文件分工，或将必要的隔离工作区放到 `local-work/` 中，避免在项目根目录旁堆叠 worktree。提交信息用英文。接口依赖不能成为空转理由：W6.5 与 W6.6 先约定类型和语义，W6.1 与 W6.6 先约定 batch schema；worker 2/3/4 可先做审查、样本/测试设计和独立工作，最终验证必须针对合入的实现。

性能硬要求按 PLAN §10 和追加规格执行：

- Tong SC 38,653 字形解析单线程 ≤100 ms；转 PCF、OTB、YAFF 各 ≤200 ms；方点 TTF ≤1 s。
- 通格两尺寸 × 两点形 × 两间距的完整矢量导出，加 20 PCF / 20 OTB，默认设置 ≤40 s；保留 `locl` 和原输出等价验证。
- 约 5.7 万字字体与西文字体拼合并写 BDF，原生 ≤1 s。
- 批量导出真实样本，报告与现有 Python 流程的总耗时对比。跨机器不能直接宣称比本机旧报告更快；所有待比较实现须在同一机器、相同语义/输入上测量。
- 只有协调者确认无关的编译、fuzz、外部消费者都停止后，才能做正式性能测量；本轮正在计时的 Rust/Python/JS/其他对照实现除外。每轮原始日志名必须唯一，并核验 SHA，不能只信 `formal:true`。阶段 5 曾修复含扩展名 case ID 造成的日志覆盖，须保留此回归。

`reference/`、`samples/`、`validation-inputs/` 只读。把需要执行并产生缓存的参考工程复制到 `local-work/`，设置 `PYTHONDONTWRITEBYTECODE=1`，不要改独立 oracle 来迎合结果。公开规格优先，GPL/MPL/EPL 等限制实现只可作为外部对照，不复制进 MIT 生产代码。既有 99 个格式的真实变体边界、Gray8 投影、显式选择和 Loss/strict 规则不能为方便批量或拼合而削弱。

交付前：审查并合并全部 W6.1–W6.6，跑 `scripts/check.sh` 和各工作项完整验收、无警告文档与所需独立对照；源码改变后重跑受影响检查，禁止伪造通过。更新 `TODO.md`、`reports/phase-6.md`、`benches/REPORT.md` 和迁移文档；记录机器/模型/fast 状态、真实限制与未通过项。生成包含所有 refs 的可验证 Git bundle、恢复说明和 ZIP，每份不超过 20 MB，不放 `target/`、venv、node_modules 或下载的第三方大文件。报告验收结果和交付路径，然后停止。
