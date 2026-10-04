# 在线 dot 交接说明

交接目标：在线环境完成第 6 阶段；本地在第 5 阶段验收后停止。完整授权 prompt 在 [ONLINE-DOT-PHASE6-PROMPT.md](ONLINE-DOT-PHASE6-PROMPT.md)。以交接包最终 `HANDOFF-MANIFEST.json`、[TODO.md](../TODO.md) 和 [阶段报告](../reports/phase-5.md) 的状态为准。

## 恢复目录与版本

每次都解压到全新空父目录，禁止覆盖式解压已有源码或参考资料；恢复脚本无法撤销解压过程中的覆盖。包内恢复脚本会校验所有交接输入，恢复 `fontpixel-rs/` 的 main 与全部历史 refs。不要把旧 P3 分支当最终主线，也不要覆盖已有工作区。目录结构：

```text
handoff/
  START-HERE.md
  PROMPT-FOR-DOT.md
  HANDOFF-MANIFEST.json
  verify.py / restore.py
  fontpixel-rs-all-refs.bundle
  fontpixel-rs/                 # restore.py 创建
  reference/                   # 原始只读参考
  samples/                     # 原始只读样本
  validation-inputs/phase2/     # 完整通格基线、20 份 BDF 及源数据
  original-task/               # 原始流程与历史任务文本
  requirements-oracles.txt
  local-work/                  # 新机器自行创建：工具、构建、输出
```

仓库 `PLAN.md` 包含后续拼合追加计划，`FORMATS.md` 包含普查后的 99 IDs。`original-task/` 中的旧版本仅用于溯源，不得回盖当前计划。

## 当前实现与验收入口

99 个格式 ID 有原生 reader/writer，89 个有 detector；未登记 detector 或真实多格式歧义必须用显式输入格式。第 5 阶段源码在 `81b577b`，整合提交 `f64b729`；此后验收/交接提交由包清单精确记录。生产代码不依赖 Python、Qt、Java 或格式原厂工具。

候选与主线 `scripts/check.sh` 均通过 2,480 次测试执行 / 597 个套件。长测 71 路、319,144,269 次执行；229 个 codec/target/专属 helper 路径与记录相同，共享代码与历史 SHA 缺口另有完整说明，见 `reports/phase5-validation/fuzz-source-audit.md`。这些历史长测不能代替 W6.4 对修改后的最终共享实现的验证。

Linux x86-64 全功能 CLI 去符号后 12.50 MiB，gzip 5.37 MiB；其他平台和后续功能体积尚未测定。阶段 5 正式性能的最终入口是 `benches/PHASE5-REPORT.md`；第一个 v2 quiet 数据因部分原始日志文件覆盖被拒绝，不能使用 superseded 目录里的 `formal:true` 作为验收结论。正式结果以最终报告链接的完整日志/SHA 验证为准。

完整能力边界见 `reports/w5.scope-limitations-review.md`：Mono/Gray8 模型，不保存任意 RGBA；SSFN/PFR 只支持文档覆盖的嵌入位图；不宣称所有历史变体或所有原厂运行时兼容。目录输出、多资产、累计输入和 gzip 预算、strict 的输入/已有输出保护不可回退。PFR 共享表重复解析是已记录、受预算限制但待进一步评估的性能项。

## 新环境准备

1. 安装 `rust-toolchain.toml` 指定的 Rust 1.99.0、rustfmt、clippy；cargo-deny 0.20.2；nightly 与 cargo-fuzz（原机 0.13.2）。不能假设本机 `/home/chen/...` 或早期 `../../toolchain/activate.sh` 路径存在。
2. Python ≥3.12。`scripts/check.sh` 的生成表检查只需要标准库。独立对照另建 `local-work/oracles` venv，使用包内 `requirements-oracles.txt`；原机 Python 3.12.13，历史早期数据用过 3.12.14，须报告实际版本。
3. Node 24、Bun 用于 JS 对照。按对应参考项目的锁文件，在可写副本安装依赖，不能往只读参考树安装 node_modules。
4. `source scripts/local-env.sh` 默认把目标产物放在相邻 `local-work/target-fontpixel-rs`，开发/测试无调试符号、incremental 关闭。根据实际内存统一分配构建 jobs；6 个 worker 不意味着每个都可开 6 路编译。
5. 对照工具如 OTS、FreeType、HarfBuzz、bdftopcf、fontconfig 按实际工作需要安装。阶段 5 的 Qt4、NeXT 原工具、模拟器等外部大工具不在交接包，生产和普通 Rust 测试无需这些工具；相关格式发生变化时，按已提交 provenance/脚本重建所需对照，不能把缺工具记为通过。
6. 恢复后先跑主线门禁确认环境，然后推进 W6.1–W6.6。新机器性能必须重测，不能拿共享笔记本的阶段 5 数字当验收值。

## 通格与已有外部资料

`samples/tong-pixel-sans/` 是原始输入样本；`validation-inputs/phase2/` 是已接受阶段 2 的完整基线（包含大小尺寸、比例/等宽、四地区和 Latin 的 20 份 BDF、生成用 glyph/build-data 和 OFL）。二者的 metadata/生成来源不同，不能混称完全相同的文件。完整来源和 SHA 在 `reports/phase2-acceptance/baseline-source-manifest.json` 及外部基线清单。

`reference/bdfparser/tests/fonts/unifont-13.0.04.bdf` 已随参考资料携带，可用于大字体规模验证，保留其许可证与来源。通格/收录站迁移只写映射说明与示例，不修改实际两个项目。原机绝对路径的测量/消费者记录是历史溯源；新环境应显式重建路径和依赖。

不搬运 `target/`、venv、下载的第三方源码、历史重复 parity 大输出和 mutation corpus。生产源码、提交历史、原创回归种子生成器、已提交对照报告/关键原始证据、原始参考资料和完整通格输入均在包内；历史 mutation corpus 可由已提交 seed 脚本重建再扩展，不能把仅重播种子当十分钟 fuzz。必要网络依赖无法安装时，明确报告阻断及未做验证，不能降低验收标准。

## 并行与停止边界

按 prompt 开 6 个 Astra/xhigh/fast worker；协调者拥有共享文件、合并和最终验收。W6.1 的 batch schema、W6.5 的库接口应尽早与 W6.6 对齐；性能正式测量时停止无关的编译/fuzz/外部消费者，本轮被测实现除外。只有全部 W6 工作项及验收完成才标记第 6 阶段完成；本轮在线任务不进入 Python/WASM/网页阶段。
