# 给本地 Codex Astra 的接续文本

以下内容可完整复制给本地 Codex。当前交接包是 `fontpixel-rs-local-codex-handoff-20261004.zip`。

---

请接手 fontpixel-rs 项目，先把第 6 阶段真正完成、验证并交付，再按我已有授权依次继续第 7、8 阶段。当前不是已验收版，不能因为收到交接包就跳入绑定开发。不启动第 9 阶段，不远程 push、不发布包、不部署网站。在线 fontpixel 开发已停在工具链阻塞边界，软件的后续工作交由你在本地完成；另行授权给在线会话的 western-rest 字形工作不属于这个项目。

## 先恢复、核实版本，再动手

- 解压到全新空父目录，不覆盖任何旧工作区。包根为 `fontpixel-rs-local-codex-handoff-20261004/`。先阅读其恢复说明、`verify.py`、`restore.py`、`delivery_common.py`，按真实参数校验/恢复。`restore.py` 创建 `fontpixel-rs/`。
- 从新包根目录执行 `python3 -B verify.py`、`python3 -B restore.py`、`python3 -B verify.py --checkout`；脚本也支持 `--root /absolute/package/root`。可加 `verify.py --checkout --zip /absolute/fontpixel-rs-local-codex-handoff-20261004.zip` 核验外层归档。恢复拒绝任何已有 `fontpixel-rs` 路径，离线建立 clean main 与全部 refs；不要删旧项目凑空目录。
- 最终版本以包内 schema-2 `HANDOFF-MANIFEST.json` 的提交、全部 refs、源码/payload hashes 为准，并核对 `SHA256SUMS`。本文不硬编码包含自身的最终 commit。`e4f65fe` 只是旧的未验收检查点，不是最终版本。
- 恢复到的历史基线是阶段 5 main `12710057e9787318d1db70db41a60fca016ad53c` 和原 54 refs，再加经明确授权的阶段 6 重建/修正。原在线工作区在 2026-10-04 约 18:33 UTC 消失；旧 phase-6 commits/raw logs/binaries/generated outputs 没有被完整恢复。
- 保留原只读 siblings：`reference/`、`samples/`、`validation-inputs/`、`original-task/`。执行 reference 前复制到 writable `local-work`；不向原树写 pycache/node_modules/build/cache，不修改通格或收录站原仓库。

必读：`docs/LOCAL-CODEX-HANDOFF.zh.md`、`TODO.md`、`reports/phase-6.md`；全文 `PLAN.md`、`FORMATS.md`、`docs/plan-addendum-merge.md`、`docs/decisions.md`；`docs/bitmap-model.md`、`docs/api.md`、`docs/w6.1-cli.md`、`docs/w6.1-writer-budget.md`、两份 migration 文档、`docs/bindings-scope.md`；`benches/phase6-README.md`、`benches/REPORT.md`、`reports/phase6-robustness/`。历史背景还包括阶段 5 报告和 `original-task/DOT-README.md`。旧的在线停止点或第 9 阶段计划不得覆盖这里的最新授权边界。

## 如实理解证据

当前 Rust **未常规编译、未类型/借用检查、未 rustfmt/Clippy、未运行 Rust tests、未进行恢复源码的 native parity 或正式测速**。官方 rustc 已部分可用，但 Cargo 下载受网络策略阻止，libstd/rustfmt/Clippy 不完整；允许的重试也被阻止，没有绕过。

包内 `handoff-evidence/` 包括最终 source-only interface audit、coordinator final checks、syntax-only archive、external oracle evidence 和生成 fonts ZIP/index。最终清单与 `FILE-INDEX.json` 指明准确 bytes/SHA/用途。组件恢复材料位于 `handoff-evidence/recovery-provenance/recovery-20261004/`，旧状态不覆盖最终 source audit/frozen main。

- 20:41 UTC 的官方 rustc `-Zparse-crate-root-only` sweep：600 `.rs`/10 context files，600 exit 0，捕获和检查期间无变更。只解析每个文件根，不做宏展开/依赖/feature/build.rs/类型/链接/测试；隔离诊断 `RUSTC_BOOTSTRAP` 不能带入生产构建。
- 20:46 文档文本 checker 86 fragments、27 documentation assertions、64 links 通过；标准库性能工具 tests 9+4+6 通过。首个 unittest 聚合 runner import 失败随后由正确的直接脚本调用纠正，不是 Rust 测试失败。
- 新外部-only oracle 已准备并验证：原 Python/Node 全 38,653-glyph SC 七产物字节一致；全部 encoded PCF/TTF pixels/advances 经 FreeType，OTS/fontTools；小 103-glyph sfnt/web +48 shaping；原 Python catalogue 全 181 outputs、105 sfnt 应用 names/copyright/version/UPM、8 ZIP CRC/member hashes。
- 外部结果没有运行恢复后的 Rust，也没有完整原 Rust export-rs / native Tong / native catalogue parity 或正式 timing。缺失生产 opf_vector 只用了明示的 supplied fontTools fallback。
- 丢失前的 2,918 test executions、compat parity、18 CLI/84 mini smokes 及性能中位数都只是历史观察，旧 raw proof 缺失。不可复制旧“pass”到当前报告。

## 六个 worker 与文件 ownership

需要时使用最多六个 worker，按 W6.1–W6.6 分工；请求模型 gpt-6-astra、xhigh、fork_turns="none"。尝试启用 fast mode，使用本地实际支持设置并记录；无法设置/核实如实说明，不能凭请求字段说已确认；在线接口原本没有 fast 参数。模型/服务层无法验证要直说，不暗换后宣称同配置。

协调者管理共享 main.rs/lib.rs/Cargo/registry/lock、整合、TODO/报告/decisions 和编译/正式测速窗口。每个 worker 明确文件范围，跨范围发具体 patch/发现给 owner；W6.1 batch、W6.5 library、W6.6 CLI 接口先协调。一个中央 build slot，jobs 统一分配，正式测量停止所有无关编译、测试、fuzz 和渲染。不要每人各开一套完整构建。

源码已有：并行 batch/diff/validate/charset、--each/vectorize、metadata/嵌入度量 CLI、源保护/多输出事务/spool、merge library/CLI、PFR shared cache、BDF/YAFF/OTB/compat 优化、API/格式/迁移 docs、完整 performance adapters。全部先视为待验证，具体文件表见 handoff。

## 必须优先闭环的源级问题

1. 所有 compiler/feature/rustdoc/运行期 gates 均待补，不把 syntax-only 当 build。
2. standalone Sources 尚未像 batch 一样附 aggregate identity ledger。审查并完成适用范围、边界测试；不能宣称已有进程总 cap。
3. CLI Variants/report/output overlap 已补，仍不等于 core Variants planner/export/WOFF 全 transient ledger；subset read/selection/internal-codec 范围也未完整审查。
4. convert/charset/merge writer 已采用完整 caller model/report reservation、零猜测 source credit；可能保守 double-count，导致旧默认能过的工作负载现在拒绝。保持真实 256 MiB 默认，审查生命周期再优化，禁止删收费或抬 cap 凑通过。
5. W6.6 **真实 Tong 地区 CJK+Latin merge 对照 fixture 仍缺实现与执行**。当前 merge Rust tests 主要 synthetic，`phase6_validate.py merge` 只有 Unifont union；Tong variants/locl oracle 是另一项。明确 SC/TC/JP/KR、size/spacing 原构建对应，逐字形/度量/类别差异检查，不能挑共同子集。
6. catalogue metadata/metric 配对尚未通过。原省略 bitmap family/style/copyright 的验证不算语义替换；plan v2 已显式 metadata、UPM1000/largest/ties-even，仍要实际 native 对照全部 173 fonts/8 ZIP。

不可改弱：所有 source/sidecar/目录/inode alias 防护、strict/loss/report、Gray8 与超限行为。max_pending_bytes 默认512MiB覆盖 prepared outputs+shared identity table；identity admission 失败 sticky 禁止所有 commits。job/path/task、working 和 report cap 分开。plain common max_output_bytes 是返回前 aggregate asset length 检查，after error/strict；有些 codec 先分配/编码再查，不能夸成所有 preallocation/RSS 都受控。gzip/None/default 语义不变。

BTree/clone 的优化 guard 只对审计过的 exact Rust1.99 commit/target 生效，其他环境保守回退；不能强制私有 cfg。BDF 多行 notice 有明确 loss，strict 可拒绝；不许删版权或压掉报告。full geometric 保留所选来源的度量，不自动把半角字变全角。命名、许可、.notdef、bearing、linegap、FontTools 默认等差异必须逐项实证，不随口定义为“有意差异”。

## 本地执行顺序

先检查实际权限与官方工具链，补齐 `rust-toolchain.toml` 要求的 Rust1.99/rustfmt/clippy、cargo-deny0.20.2、需要的 nightly/cargo-fuzz、Python≥3.12 和固定 oracle 依赖。被权限/网络拒绝时停止该动作并报告，不换路线规避。移除只用于诊断的 RUSTC_BOOTSTRAP。

从恢复后的仓库根目录：

```sh
export CARGO_BUILD_JOBS=2
source scripts/local-env.sh
unset RUSTC_BOOTSTRAP
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets --all-features
bash scripts/check.sh
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
cargo test --locked -p fontpixel --all-features --doc
cargo test --locked -p fontpixel --no-default-features --doc
cargo deny --manifest-path fuzz/Cargo.toml --locked --config fuzz/deny.toml check
```

这些命令在交接时尚未执行。先备份再处理格式/编译失败，不能删检查。`check.sh` 的全部生成表、fmt、两个 Clippy profile、全/default/最小/BDF+PCF/CJK tests、deny 都须完成。新日志放 sibling local-work；独立唯一目录，管道开 pipefail，保存 exact commands/exit/source before-after。补 focused shared-budget/plain-cap/metadata/embedded/merge/PFR/CLI/transaction tests 和最小 otb/bdat/color CLI Clippy；测试名字在 handoff 和最终 interface audit 已核对。

执行 `api_quickstart` 最小/全部/相关 feature matrix、warning-free doctest、99-format/13-command 文档 audit、18真实CLI smokes和84 tiny-Tong tasks。tiny 不能替代 full fonts。

Release 入口：

```sh
cargo build --locked --release --all-features -p fontpixel -p fontpixel-cli \
  --bin fontpixel --example phase6_timing --example phase6_tong_export \
  --example phase6_catalogue_export_name
cargo build --locked --release -p fontpixel-cli --features parity --bin fontpixel-parity
```

冻结/复制全 feature 二进制和 SHA，防止随后 minimal builds 覆盖。参考侧复制/准备用 `scripts/phase6_perf_setup.py`；parity 用 `tests/parity/run_all.py` 全默认规模和 corpus，不用 skip。真实 paired validators 是 `benches/phase6_validate.py` 的 codec/tong/vector/web/merge 和 `benches/phase6_catalogue_validate.py`，按实际 argparse 绑定新 artifacts。缺消费者就报缺，不让输出自证正确。

完整默认 Tong=8 TTF+8 OTF+8 WOFF2+20 PCF+20 OTB，另20 BDF copy，共84 artifacts；catalogue=173 fonts+8ZIP；Unifont+Latin输出57,106 glyphs。所有 cmap/name/metrics/pixels/outline/locl/notice/ZIP都要核对。Contour只允许循环起点/顺序变化，不改 winding/曲线或用小样本替代全量。原 reference-only OTS exception须重新证明，native不豁免。生产 opf_vector缺失需继续明示fallback，不冒称原生产扩展吞吐。

性能硬目标仍是：单线程完整SC parse≤100ms；BDF→PCF/OTB/YAFF各≤200ms；squareTTF≤1s；完整默认Tong≤40s；Unifont+Latin native merge写BDF<1s；全catalogue报告总时间及原Python对比。Python/JS整批与小调用、fontTools/export-rs、WOFF2质量/线程/体积对比都要报告。

最后历史 direct113.54ms、OTB249.72ms、YAFF278.59ms，以及TongCLI两次66.77/63.38s都未达标，raw已失踪；不能作为新数据，也不能用较快API/抬cap/删reports替代默认CLI。使用 `scripts/phase6_perf_prepare.py` 准备真实config，`phase6_perf.py freeze/measure` 五个新进程，独立 `phase6_perf_audit.py RESULT.json` 核对raw/所有archive members/medians。只有真的空闲窗口才传 `--quiet-window-confirmed`；不改变目标/输入/GC/timer口径制造通过。

长ASAN仍明确交本机，未完成。按99 reader/104 target inventory与fuzz/transport文档补最终源码≥10分钟有效campaign，保留全部日志、执行数、coverage、source/profile/exit；旧71campaign、build-only、短smoke或中断不能替代。遵守实际权限；不因这是本地就自动获得安全设置或被拒绝动作的授权。

## 频繁 durable backup 和阶段交付

每次重大修改/批量格式化/预算重构前后、长验证/测速前后，保存完整源码/未提交diff、HEAD/refs、输入SHA、命令和成功/失败raw证据。复制到已获授权的外部备份位置，读回hash并在全新目录做恢复验证；只留同机Git或/tmp不够。不要未经授权扩大文件分享范围。原始参考和用户其他项目不清理。

每阶段报告逐条对照PLAN/追加规格，清楚区分passed/failed/not-run/source-only/external-only/historical。归档所有失败和superseded结果，不用日志不存在的“历史通过”补空。第6阶段整合和验收完成后再交付，不满足的项目如实列出，不能自降门槛。

## 第7、8阶段不是仅做兼容包装

两种绑定都必须覆盖全部fontpixel-rs公共能力，另设独立bdfparser/bdfparser-js兼容API。建立逐type/function/method/option/feature/result/error的Rust→Python→JS覆盖表和新增缺项检查。全99格式registry/read/write/detect/typed options、FontSet/Font/Glyph/Bitmap所有编辑/渲染、vector/raster/OpenType/WOFF、variants/subset/charset/merge/batch/validate/diff、完整strict/reports/resources/sidecar/multi-asset全部覆盖。CLI领域逻辑抽共享可调用API，不用子进程假装绑定。

native coverage tests与legacy parity分开。Python pyo3/abi3、Python≥3.10、类型提示/handles/bytes/resources、manylinux本地构建；WASM wasm-bindgen/TypeScript、Node+browser、Uint8Array/具名资源集合、Promise/Worker/progress/cancel、Number/BigInt/内存与生命周期、feature size/init成本。真实平台不可用项必须queryable、明确报错、证据和替代方案；未实现就是未完成。W8.3 info/merge/render只是子集；不启动第9网页、不push/publish/deploy。
