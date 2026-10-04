# fontpixel-rs 本地 Codex Astra 接续交接

日期：2026-10-04 UTC。交付文件：`fontpixel-rs-local-codex-handoff-20261004.zip`。

**当前交付的是第 6 阶段未验收源码交接包，不是已通过的发布版。** 在线环境已完成允许且可以安全推进的恢复、源码审查、逐文件语法诊断、外部参考工具准备和外部参考输出验证。完整 Cargo/libstd/rustfmt/Clippy 工具链仍不可用；恢复后的源码没有通过常规编译、类型检查、Rust 测试、原生输出对照或正式性能验收。

用户最新要求：把进展和剩余工作完整交给本地 Codex。在线 fontpixel 软件工作在交接后停止。用户随后另外授权在线恢复 western-rest 字形修补；那是另一个项目，不属于本地 fontpixel 接续范围。本地先完成第 6 阶段的实现、验证与交付，随后按既有授权依次继续第 7、8 阶段；不启动第 9 阶段，不 push、publish 或 deploy。本包恢复成功不能用作跳过第 6 阶段的理由。

最终源码提交、全部 refs、证据归档、文件大小和 SHA 以交接包的**最终清单**为版本权威。本文不硬编码包含本文自身的最终提交，避免自指。`e4f65fe5372b65d31e97254b7b67869efd90aaa7` 只是较早的未验证重建检查点，不是最终源码身份。先读包内恢复入口，再使用[可复制的接续文本](PROMPT-FOR-LOCAL-CODEX.zh.md)。

## 0. 包内目录与恢复入口

包只有一个顶层 `fontpixel-rs-local-codex-handoff-20261004/`：

```text
HANDOFF-MANIFEST.json              # schema 2，精确 refs/source/payload hashes
SHA256SUMS
fontpixel-rs-all-refs.bundle
verify.py / restore.py / delivery_common.py
LOCAL-CODEX-HANDOFF.zh.md / PROMPT-FOR-LOCAL-CODEX.zh.md
TODO.md / PLAN.md / PHASE6-CHECKPOINT-NOT-ACCEPTED.md
reference/  samples/  validation-inputs/  original-task/
handoff-evidence/
  FILE-INDEX.json
  final-interface-audit.md / final-interface-audit-source-checks.json
  coordinator-final-checks/
  oracle-setup/
  oracle-generated-fonts.zip / oracle-generated-fonts.index.json
  fontpixel-final-rust-syntax-only.tar.gz
fontpixel-rs/                     # restore.py 创建，不覆盖已有目录
local-work/                      # 本机创建的新产物目录
```

以最终 manifest 为准核对实际文件名与成员；目录示意不代替清单。审计 final-interface-audit 是最终源级 checklist，旧 recovery FINDINGS 的当前状态已被它替代。补充组件 provenance 置于 `handoff-evidence/recovery-provenance/recovery-20261004/`，映射和权威关系见其上一级 `README.md`；旧快照不覆盖 frozen main。只将最终清单实际纳入的材料视为已交付。字形任务的另一个大型备份不属于此软件包。

先在**全新空父目录**解压，进入包根目录阅读脚本和说明，然后执行真实恢复接口：

```sh
python3 -B verify.py
python3 -B restore.py
python3 -B verify.py --checkout
```

两脚本均支持 `--root /absolute/package/root`；恢复后可额外执行 `python3 -B verify.py --checkout --zip /absolute/fontpixel-rs-local-codex-handoff-20261004.zip`。恢复拒绝任何已存在的 `fontpixel-rs` 路径，在本地离线创建 clean main 与全部 refs；不要删除已有项目来使它通过，换空目录。核对 HEAD/refs/source hashes 后进入仓库。下文验证命令都从 `fontpixel-rs/` 根运行。

仓库内 `fontpixel-rs/docs/LOCAL-CODEX-HANDOFF.zh.md` 是详细说明的权威副本；根目录同名文件仅作恢复前入口，仓库相对链接以恢复后的副本为准。

## 1. 先读哪些文件

建议按以下顺序阅读；不能用本文摘要替代原规格。

1. 包内最终清单、恢复说明、校验/恢复脚本，确认包版本与源码提交。
2. [TODO](../TODO.md)、[第 6 阶段报告](../reports/phase-6.md)、本文。
3. [PLAN](../PLAN.md) 全文、[FORMATS](../FORMATS.md) 全文、[拼合追加规格](plan-addendum-merge.md) 全文、[设计决策](decisions.md)。
4. [第 5 阶段报告](../reports/phase-5.md)、[原在线交接说明](ONLINE-DOT-HANDOFF.md)、相邻 `original-task/DOT-README.md`。这些是历史背景；其中早期停止点不能覆盖最新的本地接续安排。
5. [模型与预算](bitmap-model.md)、[公共 API](api.md)、[CLI/batch 合同](w6.1-cli.md)、[调用方保留内存说明](w6.1-writer-budget.md)、两份迁移说明、[完整绑定范围](bindings-scope.md)。
6. [性能合同](../benches/phase6-README.md)、[性能状态](../benches/REPORT.md)、[外部 oracle 证据说明](../benches/phase6-oracle-setup/README.md)、[健壮性恢复状态](../reports/phase6-robustness/recovery-status.md)。
7. 随包 `handoff-evidence/final-interface-audit.md`、syntax archive 内的 `final-handoff/`，以及 `handoff-evidence/recovery-provenance/recovery-20261004/` 下各 owner 最晚 checkpoint 和 provenance。旧状态与后续修正并存时，以最晚源码及其确切 hash 为准，保留旧记录而不冒充同一次验证。

`FORMATS.md` 的 99 个规范 ID 是现有格式范围权威。第 5 阶段基线记录了 99 个 reader/writer、89 个 detector；这不表示本次重建已重新通过，也不表示支持每个历史方言、任意 RGBA 或任意格式间无损转换。格式 profile 和真实限制见 `docs/formats/`，不能为了方便删减清单。

## 2. 为什么需要重新验证

约 18:33 UTC，原工作区、Git 对象、工具链、原生二进制和未外备份的原始日志同时不可访问。原因未确认，不能写成已知平台故障原因。原始用户交接 ZIP 仍可验证，已在全新目录恢复第 5 阶段主线：

- 基线 main：`12710057e9787318d1db70db41a60fca016ad53c`。
- 原始 54 refs 保留；阶段 5 已提交的证据仍属于其原始源码范围。
- 原始 ZIP：53,478,221 字节；SHA-256 `8cc36c6f817347f808c1ef6f2ab1450715380d715bca2f4e80106e2df9bcb524`。
- 丢失后的代码来自可核对的已记录片段、基线加补丁恢复，或明确标记的新实现/修正。缺失的旧 phase-6 commits 没有因记住 SHA 而恢复成 Git 对象。

证据必须区分四层：

| 层级 | 实际含义 | 不能推导的结论 |
| --- | --- | --- |
| 阶段 5 原包与 Git 基线 | 校验后的既有源码、refs、原有阶段证据 | 后续 phase-6 改动也通过 |
| 有旧 hash 的逐字节恢复 | 特定文件 bytes 等于旧记录；例如 compat lookup 文件/测试，以及三个文档示例 | 新的跨模块组合无需再编译或测试 |
| 重建/新修正 | 依据保留合同重写 CLI、merge、预算、迁移、性能工具；有当前前后 hash/patch | 等于失踪版本或继承其测试 |
| 本次新证据 | 标准库工具测试、静态审查、parser-only、外部-only fixture | Rust 类型正确、原生输出正确或性能达标 |

明确逐字节恢复的例子包括 `compat/bdfparser/font.rs`、`phase6_compat_lookup.rs` 以及 `examples/batch.toml`、`examples/tong_batch.py`、`examples/check_documentation.py`。API/模型文档保留了可核对底稿，但后来增加了恢复状态和新控制说明；当前文件整体应按当前清单标识。`api_quickstart.rs` 是按现有接口重写，不是找回的原文件。PFR、OTB、YAFF 等恢复包各有自己的精确程度说明，不能统称完整恢复。

失踪的 phase-6 raw Cargo/parity/性能日志、生成字体、旧原生二进制和部分 proof ZIP 没有被伪造补齐。历史 2,918 次测试执行/649 个套件、3,292 cases/58,839 operations/251 corpus、18 CLI smoke、84 mini-Tong task 等只保留为历史观察，不能作当前验收。测试执行次数也不等于独立测试数。

## 3. 目前真正有什么新证据

### 3.1 最后一次语法诊断

`handoff-evidence/fontpixel-final-rust-syntax-only.tar.gz` 内 final-handoff 的 `summary.json` 记录 2026-10-04 20:41:00–20:41:08 UTC：600 个 `.rs` 文件各自作为 crate root 解析，600 次 exit 0，stderr/stdout 为空，无 syntax-error span；另外捕获 10 个 manifest/lock 上下文文件。捕获前后、诊断前后清单没有发生源码变化，snapshot bytes 再校验一致。

模式是官方 rustc 1.99.0 的 `-Zparse-crate-root-only`。`RUSTC_BOOTSTRAP=1` 只存在于隔离诊断子进程，不是项目配置，不允许带入正常构建。控制样例证实普通语法错误会失败，同时缺失依赖、错误类型、未展开宏和外部 module 不会被检查。

因此它**没有**做 Cargo、依赖解析、宏展开、proc macro、build.rs、类型/借用检查、链接、测试、feature matrix、rustdoc 代码执行。每个现存 `.rs` 被清点不等于 missing/generated/include 源码完整。最终清单若包含后来改动，必须据 hash 明确哪些文件不在该 sweep 内。

### 3.2 文档与 Python 工具检查

最近文档静态检查记录 86 个源文本片段、25 组接口引用、27 个文档要点、3 条过期声明排除、1 个示例 TOML 语法解析、6 组 example feature、5 个 crate-root doctest 代码块、64 个本地链接。它只证明文本存在/结构可读，不证明 Rust 签名、Clap parser 或示例能运行。

协调者在 20:46 UTC 重新运行上述 API 文档 checker；直接执行 `scripts/phase6_perf_test.py` 9 项、`benches/phase6_workload_test.py` 4 项、`benches/phase6_catalogue_test.py` 6 项，均通过。第一次合并 `python -m unittest` 调用因 script directory 不在 sys.path，无法 import phase6_perf；该 runner 错误保留，随后按文档直接脚本执行通过。它不是 Rust 失败。工具测试覆盖新建路径不覆盖、五轮记录、不可变输入、完整归档/报告、独立中位数复算等，不是 native correctness 或正式测速。最终记录在包内 `handoff-evidence/coordinator-final-checks/`；新 handoff 文档审查另记。

### 3.3 新的外部-only oracle 结果

[oracle-setup 说明和归档](../benches/phase6-oracle-setup/README.md) 保存了准确命令、依赖/源输入 hash、stdout/stderr 和结果：

- 相邻 `requirements-oracles.txt` 的 9 个 Python 直接依赖已按固定版本准备；Node 24.19.0/npm 11.9.0、lock 中 esbuild 0.27.7 用于复制的 JS 源码。Bun 未安装，不能声称 Bun 项目测试通过。631 个 reference 文件复制并前后校验；不在保护树运行。
- 全 Tong Large SC：38,653 字形、5,746,021 源字节。原 Python 和 Node 的 BDF/PCF/TTF/screen/atlas/crop/concat 七产物逐字节一致。
- FreeType 核对全部 38,067 个 PCF BMP 映射字形、38,653 个 TTF 编码字形的 pixels/advance；fontTools 全解码，OTS 后的字形再核对。
- 小 103-glyph sfnt/WOFF2 对照覆盖完整表/轮廓/栅格和 48 组 shaping；monobit 读取 102 个源点阵字形。非 XLFD 警告和一次错误 fixture 路径的失败都保留。
- 原 Python catalogue 流程产生 34 输入/8 families/173 fonts/8 ZIP，共 181 outputs。105 个 sfnt 全解码，并核对原应用 family/style/copyright/version/UPM；8 ZIP CRC/member hash 完整。四个 native export-name helper job 只是准备好了，尚未执行。

这些是**原参考侧**的有效 fixture，不包含恢复后 Rust 或原 Rust export-rs 的编译/配对验证，不是正式计时。缺失的生产 `opf_vector` 明确用 supplied fontTools reference 替代，不能宣称测过缺失扩展的性能。完整输入/输出 bytes 与 SHA 保留，未来只有确切相同 bytes 才能复用相应证明。

## 4. 目前的工具链阻塞

固定生产工具链见 `rust-toolchain.toml`：Rust 1.99.0，rustfmt、clippy。当前恢复环境只装成官方 rustc/rustdoc 及调试包装脚本；Cargo、目标标准库/libstd、rustfmt/Clippy 不完整。官方 Cargo 下载被网络策略阻止，包括允许的提升重试；未绕过限制。没有 Cargo 构建日志可作为本版本成功证明。

本地检查真实工具状态与权限后，通过允许的官方来源补齐。不要拷贝在线机器的绝对路径或把孤立 rustc 当完整工具链。不要为通过预算改 `RUSTC_BOOTSTRAP`、自定义 sysroot、强制私有 cfg 或降低版本而不审查。cargo-deny 0.20.2、nightly/cargo-fuzz、Python ≥3.12 及各 oracle 依赖按本机允许方式准备，版本实际记录。若仍被限制，保留失败原因并停止对应操作，不换路线规避。

## 5. W6.1–W6.6 源码清单与剩余门禁

所有“已有”均指源码存在；下面没有把任何工作项标成已验收。

| 工作项 | 主要文件/当前内容 | 必须补的验证与完成条件 |
| --- | --- | --- |
| W6.1 CLI/batch | `crates/fontpixel-cli/src/{main.rs,batch.rs,commands.rs,commands/sfnt_validate.rs,assets.rs,input_budget.rs,metadata.rs,opentype.rs,p3_formats.rs}`；并行 TOML、diff/validate/charset、事务、源保护、报告 spool、命名/嵌入度量、`--each` | 正常/最小 feature 编译与 Clippy；所有命令真实 smoke；JSON/退出码、失败保留现有输出、全任务 alias/sidecar/目录/硬链接、spool 错误/容量、worker admission、strict/resource 边界；全部新 unit/integration tests |
| W6.2 文档 | README、`docs/api*.md`、模型/格式索引/两份迁移、`examples/{batch.toml,tong_batch.py,check_documentation.py,check_api_documentation.py}`、`api_quickstart.rs` | warning-free rustdoc 和 doctests；公共项文档覆盖；example feature matrix；13 命令帮助/例子、18 实际 smoke、84 tiny-task 检查；逐参数迁移与真实 outputs 对照；99 格式文档不能仅有链接 |
| W6.3 性能/迁移配对 | `scripts/phase6_perf*.py`、`benches/phase6_*.py/.mjs`、`phase6_timing.rs`、`phase6_tong_export.rs`、`phase6_catalogue_export_name.rs`、`benches/support/catalogue_export_name.rs` | build helpers；冻结当前源码/依赖/输入/二进制；完整 native/compat/Python/JS/export-rs/fontTools 配对；全 catalogue metadata/metric/pixel；22 cases 五进程正式计时与独立 raw/archive/median 审计；未达标继续处理或如实报未完成 |
| W6.4 健壮性 | `formats/pfr/wire.rs` 共享物理表 cache、`tests/pfr_shared_physical.rs`；`reports/phase6-robustness/`；shared model/transport/transaction regressions | 全 feature/property/resource/strict gates；PFR 重复逻辑/物理表限制；最终源码的所有 required reader ASAN ≥10 分钟；不是只重放旧 71 campaigns。长 ASAN 明确交本机，权限不允许时仍是未完成 |
| W6.5 merge 库 | `src/merge/{mod.rs,selection.rs,metadata.rs,report.rs}`、`charset/merge_categories.rs`、`tests/merge.rs` | Unicode 边界/组合排除/顺序/fallback；dy/上下界/三种 spacing/几何宽度；license/RFN/完整 notices；所有 report 和预算边界；转义 metadata audit、数组增长、alias/sequence；小字体+真实 Tong/Unifont 独立验证 |
| W6.6 merge CLI/batch | `crates/fontpixel-cli/src/merge.rs`、batch 接入、`tests/merge_phase6.rs`、多输出共用 assets | 参数绑定/歧义 `--to`/多 strike index；混合格式；整组正常错误 rollback；来源/已存在输出保护；完整 JSON/text；通格地区+CJK/Latin 逐字形等价；57,106 输出 Unifont merge+BDF <1 秒目标 |

另外已经有重建/恢复优化：BDF ASCII row parser、YAFF 写入/元数据路径、OTB canonical raster/inline mapping keys、compat `drawcps` BTree lookup、共享 retained/clone accounting。相关文件测试包括 `phase6_bdf_hotpath`、`phase6_yaff_hotpath`、`phase6_otb_hotpath`、`phase6_otb_keys`、`phase6_compat_lookup`。存在优化不代表真实更快；OTB/YAFF 历史仍未达标。

### 最近源码审查修正

- sfnt `validate` 显式 `--from` 检查实际 wrapper/feature，不能仅按 magic 偷换容器。
- resolver chunk reads 同时限制返回 Vec capacity 与长度，检测多余一字节；其 source-only 回归待执行。
- common native-write limits 实际收紧 typed codec；plain aggregate-output cap 覆盖全部 assets，详见下一节。
- `phase6_tong_export` 补完整 84 reports，避免旧示例只给 report-code count；Cargo example feature 注册已补。
- 性能 case/engine 拼接路径歧义加 source guard，避免一轮挡住另一轮或覆盖证据。
- P3 三个 OTB 参数与 metadata/variants 参数已实际写入源码；旧 recovery note 的“未接入”已经过时。
- 旧只用于 tests 的 assets wrapper 加 `cfg(test)`；最小 sfnt feature 的 report/import 可变性警告有源级修正。必须用真实 Clippy 确认。

## 6. 不可退让的语义与最新预算合同

### CLI 与输出

`convert --each` 要求显式 `--to` 和已存在的输出目录；仅去掉末尾小写 `.gz` 后取 stem，追加注册表首扩展名，名称碰撞失败；所有输入输出组成一个正常错误 rollback group。`--font-index` 默认作用于合并有序输入 strike，`--each` 时各输入分别选。输入最多 1,024 paths。事务不是断电/进程崩溃原子性，也不承诺抵御敌意同时修改文件系统。

batch version=1；任务顺序决定报告顺序，variants languages/scripts map 按词典序，merge parts 用有序数组。失败优先级 2 > 1 > 3 > 0。独立任务普通准备失败可保留其他成功准备，但全局源保护、预算、输出冲突或事务失败禁止相关整体提交；不存在跨任务依赖或 batch 原地覆盖例外。`diff` 包括完整 duplicate 列表、sequence、unencoded、metrics/pixels，但不比较所有 names/raw properties；它不能代替完整迁移等价检查。`validate` 也不是 OTS 全功能替代。

### 预算不能当作 RSS

默认 `ResourceLimits.max_total_bytes` 仍为 256 MiB，常见 primary/per-resource 64 MiB；batch working 1 GiB、pending 512 MiB、单 task report 64 MiB、aggregate report 2 GiB。不要为默认工作负载通过而静默抬高限制、删 diagnostics、截断成功报告或改变计时内容。

- `max_pending_bytes` 覆盖 prepared outputs **和 shared input identity table** 的保留容量；identity admission 失败留下 sticky incomplete latch，全部任务不能提交，防止失败任务已读 sidecar 失去保护。
- Job TOML 8 MiB、1..10,000 tasks、ID 最长 256 UTF-8 bytes、100,000 declared input paths 是另外的 admission cap。jobs 请求 1..64，默认 CPU 数上限 32；实际 workers 再受有效任务与 working allowance 限制。
- report ≥1 MiB 可写单个私有 spool，quota 不变，最终 JSON 完整保留。实际磁盘 `TMPDIR` 才能降低整体 RAM；tmpfs 仍占内存。spool/source identity、目录祖先/子项、防替换和 cleanup 需要运行期测试。破损 stdout 不能被描述为提交后的可回滚事务。
- `retained_usage` 包含 headers、capacity、空 glyph slot、索引；`cloned_usage` 预检新的 owned clone，源仍收费。BTree occupancy 与 outer glyph Vec clone len 的较紧估计仅对已严格识别的官方 Rust 1.99.0 commit `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`、审计的 x86_64 GNU host/target 生效；其他 compiler/target、可识别的可疑 flags/custom sysroot 指示回退保守算法。版本元数据不能识别所有人为替换的标准库或隐藏 wrapper，不能把 guard 说成密码学证明；不能强制 cfg。完整推导/guard 见 bitmap-model，实际上限未增加。
- `convert`、`charset` 在 native writer 前先扣 live caller FontSet 和 incoming reports；merge writer 扣模型、read/merge/write reports、prior bundles 和 summaries，不给没有共同等价合同的“codec source credit”。某些 writer 因自身 source charge 会保守重复收费；不能靠猜测删除费用。最新默认 Tong/Unifont 必须重跑，旧 256 MiB 成功观察已不能验证这些新改动。
- merge 初始 Font/Report staging arrays 合并 preflight、实际 capacity 扣减发生在第一个 decoder 前。库与 CLI 的已证明 selected-source 抵消仍保留；metadata Debug 转义诊断先计长、检查 report growth，再写入。相关修正不等于所有 merge 临时分配都已经形式化证明。
- OpenType CLI 已核算 Variants 与 reports、auxiliary data、sfnt/web output 的调用方重叠；空 report 的 spare buffer 及时释放。不可变命令参数/Options strings 被明确视为配置，不含于该 operation budget；codec 自己拷贝仍受其预算。全部 core Variants/WOFF simultaneous transient 分配、subset 内部预算还需深审，不能说已有全量 peak-memory 证明。
- `WriteOptions.transport_limits=Some(common)` 在 plain output 返回前，checked-sum 所有 primary/sidecar/directory asset 的 **length** 并对 `common.max_output_bytes` 检查；exact limit 可通过，overflow 失败；合并 error/strict diagnostics 优先。它不是 capacity/RSS cap；没有原生 output cap 的 codec 可能先编码再检查。`None` 不新增 plain aggregate cap，typed per-asset cap 不改成 aggregate，gzip envelope/replay/default 行为不变。

额外明确未闭环：standalone 命令的全局 Sources identity ledger attachment 仍是审查 follow-up；batch 已接入不代表所有 standalone 路径都有同一总 ledger。不能通过交接说明悄悄宣布已修复。

### Merge 与 notices

选择是正集合 union 后应用所有排除，只有负项表示从 all 开始。第一个选中且有字的部件获胜，fallback 默认关。dy 正值向上。mono 需要 selected Han 的唯一正 modal advance；奇数半宽不静默取整；较窄西文用 advance 空白居中，不缩放 bitmap。`geometric_width=half` 为默认；full 保留提供的几何字度量，需要选对来源，不合成全角字。SWIDTH 比例缩放不整除有 `scalable_rounding` loss/strict 拒绝；DWIDTH=0 保留独立 SWIDTH 并报告。

新 family 必填，reserved-name 检查与未知/禁修改 license 的 strict 行为不能省略。完整 notices 依部件顺序保留。BDF 将多行 notice 存 COMMENT 并把 property 平铺，会发 `bdf.write.notice_line_breaks` loss；因此 strict merge→BDF 可因表达边界被拒绝，即使 license 都已识别。不要把它“修”成静默无损或删版权来过 strict。

### Naming 与嵌入度量

`metadata::apply_names` 修改 normalized names、原始 provenance 单独保留；family/style 变化会清掉旧 full/PostScript，除非显式替换。variants legacy_style/name ID2 与 style/ID17 分开；exact names table 优先于生成字段。convert 的 version 是 literal；variants 输入不带 `Version ` 前缀。

OTB/bdat 用 `--otb-upm 16..16384`、`--otb-reference first|largest|N`、`--otb-rounding half-away|ties-even`；batch 用相应下划线 scalar keys。N 是写入时 FontSet 的零基 index，不能混为 catalogue 文件序号。largest 的 y-ppem 同值取最后原输入，fallback 同 descending 优先级；不重排 physical strike。参数只接受 OTB/bdat 和已启用 feature；color/其他输出失败。vector `--upm` 是另一项。默认自动 upem/first/half-away 保留。

## 7. 收录站与通格对照：仍欠哪些证明

### 收录站

原 manifest 的 SHA `fcf8b612cbb197298291bf3c16de03fb88ddba6bd7fb8176c532e95bfbbf874e`。重新静态盘点 442 active families、71 pending、41 family keys、124 instances/64 families；详细逐参数映射见 migration 文档/JSON。当前实际 sample workload 为 8 families/34 fonts，不要把 sample 对照说成全部 442 families 运行过。

旧验证遗漏 bitmap family/style/copyright，并发现 reference strike/scaling 与 implicit FontTools defaults 差异；旧 glyph-only 结果不能接受为语义替换。重建 plan v2 已要求 explicit metadata，bitmap UPM1000/largest/ties-even，vector legacy/full/PS names。BDF/PCF 专用 native export-name helper 在计时流程内；sfnt 继续用原 source。所有 helper/PCF charset normalization/batch/ZIP 成本计入 native 工作流，准确称为“CLI + native helper”。

必须配对检查 all encoded/unencoded glyphs、actual strikes、1×/2× vector pixels、advance/bearing、全部 name record、版权/许可、hhea/OS2/head/post/maxp、timestamps、table flags/defaults、.notdef/helper glyphs、ZIP membership/licence bytes。原 FontTools fsType=4/fsSelection=0/usBreakChar=32 是库默认，不能推断作者授权或盲目强制原生拷贝。source .notdef、真实 bearings、line gap、underline、spacing 的差异逐项核对来源和消费影响，不可统称“有意差异”而跳过。

原生产 `opf_vector` 源码/二进制缺失：只可明确标记 supplied pure-fontTools oracle。变量 axes 实例化、hint 执行、per-glyph offsets 的真实未实现项保留原/外部 fallback 并报告；不能声称 native 已覆盖。应用下载/分组/许可文件/README/网站政策留在 host，不修改原仓库。

### 通格与拼合

完整通格是 2 sizes × 2 shapes × 2 spacing：8 TTF + 8 OTF + 8 WOFF2、20 PCF、20 OTB，另 20 源 BDF copy，总 84 artifacts。性能主目标是 64 exports；报告明确多带的 20 copies。tiny 84-task smoke 不等于 full-font correctness。原 `samples/tong-pixel-sans` 和 `validation-inputs/phase2/bdf` 元数据/生成来源不同，不互换身份。

配对检查全部 cmap/字形/度量/name、每个分解 outline 的 winding/line/curve/control points、native-size bitmap、语言/script `locl`，两边 WOFF2 解码、OTS/FreeType/HarfBuzz。轮廓比较仅允许循环起点和 contour 顺序变化，不为了通过改为只比少数像素。原 reference-only null-DFLT OTS exception 要在实际 bytes 重新证明，不能给 native 同样豁免。

GNU Unifont 案例：57,086 输入加 Latin，western-first/all-Unifont，prop、default budgets，输出 57,106 字形（3,645 Latin + 53,461 Unifont）。对完整 glyph records 和报告/来源/names 验证；不是仅查 count/hash。**当前还缺 W6.6 真实 Tong 地区 CJK+Latin 等价 fixture 的实现和执行。** 已有 merge Rust 测试主要是 synthetic，`phase6_validate.py merge` 只检查 Unifont union；Tong variants/locl 对照不能替代此项。需明确每个 SC/TC/JP/KR 与 size/spacing 的原始构建对应关系，取 CJK+Latin 后逐字形比较；geometric、fallback、缺字、元数据等差异类别逐条解释，不靠挑相同子集通过。

## 8. 性能目标一项也没有取消

按 [PLAN §10](../PLAN.md) 和追加规格，用实际机器 CPU/可用核数/内存/OS/工具版本，墙钟五次中位数；不以单次、debug、污染窗口或改小工作负载代替。

| 工作负载 | 必须达到或报告 |
| --- | --- |
| 完整 Large SC 38,653 glyphs BDF parse，单线程 | ≤100 ms；同时保留 direct/native registry 口径，不能挑快的一项掩盖已测的慢项 |
| 同一完整 BDF→PCF、OTB、YAFF | 各 ≤200 ms |
| 同一完整 BDF→方点 glyf TTF | ≤1 s |
| 完整默认通格 8 vector families × TTF/OTF/WOFF2 +20 PCF+20 OTB | ≤40 s；分项、WOFF2 quality/thread/reuse 的时间与体积都报告 |
| samples catalogue 全部 181 downloads | 报总时间，与原 Python policy+明确的 fontTools fallback 比较；不凭空增加没有的固定阈值 |
| Unifont+Latin 57,106 glyph native merge 写 BDF | <1 s；完整输出和 JSON report，不删除许可/缺字 diagnostics |
| Python/JS、fontTools、export-rs 对比 | 同操作 read/write/render/drawall/crop/concat、vector 与同 sfnt WOFF2；边界小调用要诚实列出慢项 |

历史最后五轮（raw 已失踪，**不是当前证据**）：direct parse 113.54 ms、public read 98.89 ms、PCF 194.90 ms、OTB 249.72 ms、YAFF 278.59 ms、square TTF 793.06 ms。direct/OTB/YAFF 尚未过线。历史 full batch CLI 两次诊断 66.77/63.38 s，超过 40 s；不是五轮 final。API 快速路径或 jobs=1/抬 cap 结果不能替代默认真实 CLI。正式 Unifont <1 s、完整 22 cases 最后五轮从未完成。

不得改 oracle/阈值/输入规模、提高 default cap、剔除 reports 或把 I/O/clone/destruction 悄悄移出 timer 来制造通过。一个合法新测法必须另建 case ID，并保留原必测 profile。原图像模式 drawall=1 使全部字形在既有限制内；mode0 超限是独立行为，不以加 cap 或丢字伪装等价。

## 9. 本地第一次执行顺序

下列命令仅是接续指令；**没有在此次在线 handoff 中执行**。先验证包、恢复到全新空目录；不要覆盖旧项目/样本。按最终清单提供的脚本恢复全部 refs，记录 before hash。安装/执行仍受本机真实权限与安全确认约束。

### 9.1 建立工作区与普通工具链

在恢复的 `fontpixel-rs/` 根目录，用新的 shell：

```sh
export CARGO_BUILD_JOBS=2
source scripts/local-env.sh
unset RUSTC_BOOTSTRAP
mkdir -p "$FONTPIXEL_WORK_ROOT/local-validation"
export TMPDIR="$FONTPIXEL_WORK_ROOT/local-validation/tmp"
mkdir -p "$TMPDIR"
rustc -Vv
cargo -V
cargo fmt --version
cargo clippy --version
python3 --version
```

`local-env.sh` 默认 jobs=6，所以上面先显式设 2，再按本机总内存协调调整；不是每个 worker 都拥有独立 2 路额度。确认 TMPDIR 真的是合适的磁盘文件系统，不能仅凭路径名字判断。固定 Cargo.lock；新机器工具/依赖版本和官方来源写进新日志。

### 9.2 格式、类型、普通门禁

先保存完整 checkpoint，再执行：

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets --all-features
bash scripts/check.sh
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
RUSTDOCFLAGS='-D warnings' cargo test --locked -p fontpixel --all-features --doc
cargo deny --manifest-path fuzz/Cargo.toml --locked --config fuzz/deny.toml check
```

格式失败时先看 diff，按 ownership 执行 `cargo fmt --all` 后保留 patch，再重新冻结与检查；不要把 checker 改成跳过。类型失败按实际诊断修复，不以 parser-only 通过为理由争辩。`scripts/check.sh` 实际包含两个生成表 `--check`、fmt、全 features/workspace/all-targets Clippy、core no-default Clippy、全 features/default/no-default/BDF+PCF/CJK 测试以及 cargo-deny；不能只跑其中一个就称整体验收。

上游 advisory 更新受阻时分别记录 cached 数据库时间与在线更新失败，不把离线检查称在线最新检查。所有运行用独立新日志目录、保留命令/exit/source before-after；`tee` 管道必须开 pipefail。

### 9.3 高优先级 focused regressions 和例子

先跑恢复改动重点，再做完整矩阵；下面 test target 均来自当前仓库：

```sh
cargo test --locked -p fontpixel --all-features \
  --test phase6_native_budget --test phase6_plain_output_budget \
  --test phase6_btree_guard --test metadata --test variants_metadata \
  --test phase6_embedded_options --test phase6_notice_metadata \
  --test pfr_shared_physical --test merge
cargo test --locked -p fontpixel-cli --all-features \
  --test phase6_cli_recovery --test merge_phase6
cargo run --locked -p fontpixel --no-default-features --example api_quickstart
cargo run --locked -p fontpixel --all-features --example api_quickstart
python3 -B examples/check_api_documentation.py --require-rustdoc-link
```

另外运行 assets/input-budget/batch/OpenType/merge CLI 的 inline unit tests、hotpath/compat 回归、OTB/bdat/raw/gzip/最小 CLI feature 分支；全 gate 负责覆盖全部，不以本列表代替。`api_quickstart` 六 feature 分支还须单独/合适组合检查，rustdoc 附加 snippets 同样要执行。README command smoke、JSON failure paths、18 文档案例、84 tiny Tong 按保存 matrix 在新目录重跑；不得把历史 logs 填回新目录。

### 9.4 建不可变 release 与独立对照

```sh
cargo build --locked --release --all-features -p fontpixel -p fontpixel-cli \
  --bin fontpixel --example phase6_timing --example phase6_tong_export \
  --example phase6_catalogue_export_name
cargo build --locked --release -p fontpixel-cli --features parity --bin fontpixel-parity
python3 -B examples/check_documentation.py --binary "$CARGO_TARGET_DIR/release/fontpixel"
```

后一个 feature build 可能重建共享 artifacts；正式测量前把所需**全功能** binary/example 复制到新的不可变目录，记录真实 build command、feature、SHA。copy 后验证身份，后续最小 feature build 不得覆盖被测路径。

使用 `scripts/phase6_perf_setup.py --work WORK --output SETUP.json --copy-references` 复制原 reference 到新的 writable WORK；它不下载/安装/编译。安装依赖只在允许的本地 venv 中做，保留 package/native library 身份。原 Rust export-rs 只能从复制树构建，按 `benches/phase6-README.md` 记录其 manifest/lock/toolchain 与 binary。

完整 §9.2 parity 入口是 `tests/parity/run_all.py`，使用复制的 Python source：

```sh
BDFPARSER_PY="$FONTPIXEL_WORK_ROOT/phase6-performance/reference/bdfparser/src" \
  python3 -B tests/parity/run_all.py \
  --rust "$CARGO_TARGET_DIR/release/fontpixel-parity" \
  --out "$FONTPIXEL_WORK_ROOT/local-validation/parity"
```

以上 reference path 必须是实际已准备的复制目录。不能带 `--skip-corpus` 或缩小随机规模来宣称 full parity；已知 K12 逐条按 `known_issues.py` 核对，禁止新放宽 oracle。独立正确性入口：

- `benches/phase6_validate.py codec`：native/compat/Python/Node 全输出字节、FreeType、monobit。
- `... tong`：完整 84 文件、所有 glyph/outline/metrics/pixels/name/locl/WOFF2。
- `... vector` / `... web`：同输入矢量/同 sfnt 包装；不要对不同 outline 比压缩快慢。
- `... merge`：完整 57,106 records；另外运行 source/license/report 测试。
- `benches/phase6_catalogue_validate.py --reference ... --plan ... --native ... --python ... --output ...`：metadata v2 和全 181 outputs。

先读各脚本实际 argparse/`--help`，所有路径绑定真实当前 artifacts；partial proof 只有全部通过才完整。外部消费者缺失要报阻塞，不能跳过。

### 9.5 正式计时和审计

`phase6_perf_prepare.py` 要求 `--work`、`--target`、`--catalogue-plan`、同源 `--woff2-square-source`、`--woff2-dot-source`、`--output`，可分 `--section`。先准备真实数据、检查 schema/case identities，再冻结。可提升 `--tong-cli-variant-bytes` 的诊断必须另标 profile，不能叫默认达标。

```sh
python3 -B scripts/phase6_perf.py freeze --config CONFIG.json --output FROZEN.json
python3 -B scripts/phase6_perf.py measure --manifest FROZEN.json \
  --directory NEW_RUN_DIR --formal --quiet-window-confirmed \
  --environment-note '填写实际机器与已停止其他负载的情况'
python3 -B scripts/phase6_perf_audit.py NEW_RUN_DIR/result.json
```

`--quiet-window-confirmed` 只有真正停止无关 build/test/fuzz/render/consumer 后才加，不是绕过检查的常量。现有脚本五进程记录，保留完整 stdout/stderr、报告、output archives、成员 hash/大小、源/输入/依赖/binary identities；独立 auditor 不导入 recorder 实现。每个目录唯一、新建、不得覆盖失败记录。只删除本次新生成且完整归档并校验过的临时产物，原始输入与用户文件不动。

### 9.6 本机 ASAN

长 ASAN 是明确延期到用户本机、仍未完成的工作。按 `reports/phase6-robustness/reader-target-inventory.json` 检查 99 IDs 与 104 declared targets 的真实覆盖；已有 71 份阶段 5 campaign 不能代替改过 shared code 的最终源码验证。每 required reader 至少有效完成 10 分钟，记录命令/profile/sanitizer、源码 before-after、执行数、coverage、RSS、时间、exit、完整 raw logs。

命令/typed feature/seed 参考 `fuzz/README.md` 和 `docs/transport-fuzz.md`；把 corpus/build/artifacts 放 sibling local-work，先核对当前 cargo-fuzz 的路径参数，不照抄旧文档中写源码树的路径。ASAN 保持启用；旧 sandbox 的 LeakSanitizer 例外仅在本机确实适用且权限允许时解释。不能擅改安全设置、借 bootstrap/别的环境绕过拒绝。被拒绝、超时、中断、只有 build 或短 seed replay 都不是通过。

## 10. 六 worker 与共享源码协作

建议本地协调者 + 六个工作项 owner，指定 `gpt-6-astra`、`reasoning_effort=xhigh`、`fork_turns="none"`；用户已要求 fast：尝试启用 fast mode，使用本地实际支持的设置并记录；无法设置/核实如实说明。在线 native spawn 只能请求模型/effort，不能设置或验证 fast，实际 backend 也未独立确认。不要把请求参数写成模型/fast 已验证的事实。

按 W6.1–W6.6 分六组；每组接收完整背景、文件 ownership、来源/测试要求与 blocker 上报方式。协调者拥有 `main.rs`、`lib.rs`、Cargo/lock/registry 共享修改、TODO/报告/decisions、编译/测量窗口与最终整合；若重新分配必须明确通知。文档 owner 不顺手改公共 Rust；性能 owner 不改 oracle 消除差异；merge library/CLI/batch schema 先协调。

禁止多个 worker 各自执行完整构建或正式测量。一个中央 build slot、明确 aggregate jobs；性能窗口停止所有无关任务。默认同一主 checkout 按文件协作，必要时由协调者安排隔离分支，避免生成未管理 worktree 群。本地 fontpixel workers 保持在本项目范围内；另行授权给在线会话的 western-rest 字形修补不混入本包或本地工作项。

## 11. 数据保护、频繁 durable backup 与交付

`reference/`、`samples/`、`validation-inputs/`、`original-task/` 视为只读原输入；不在其中安装依赖、写 pycache/node_modules、格式化、打补丁或清理。所有 build/cache/oracle/benchmark/fuzz/temp/log 和交付产物放 sibling `local-work/`。保留原名称、许可与 provenance；生成 fixtures 加来源说明。

重大改动、批量格式化、重构、更新共享预算、长测试/正式性能前后都做可恢复 checkpoint：源码/未提交 diff、当前 HEAD/所有 refs、输入和文件 hash、准确命令、失败与成功 logs、阶段状态。只存同一机器 Git 或临时目录不足以 durable；上传/保存到用户已授权的外部位置后重新读回 hash，并实际在空目录试恢复。不要把未确认上传当外备份成功。遵守具体目的地的权限，不因备份要求扩大分享范围。

包的 verify/all-ref restore/member hash/源码一致检查只证明完整性，不证明 Rust 正确。最终第 6 阶段交付要包含全部当前 gates 与原始证据、未通过项和适用限制，不能隐藏历史 misses 或将外部-only logs 放在 native-pass 名下。本在线任务交接完成后停止；后续本地进展由本地 Codex 继续保存和汇报。

## 12. 第 7、8 阶段的完整绑定范围

前置条件是第 6 阶段真正完成并交付；本次未验收源码交接不满足此前置。用户明确要求 Python 和 WASM/JS 提供**全部 fontpixel-rs 公共能力的独立原生开发 API**，另设 bdfparser / bdfparser-js 兼容 API。不能只包兼容 Font/Glyph/Bitmap、几个 bulk 函数，或用外部 CLI subprocess 充当 native binding。

建立逐模块/类型/构造/函数/方法/选项/feature/result/error 的 Rust→Python→JS 映射和自动缺项检查；native coverage tests 与 compatibility parity 分开。至少包括：

- 全 99 格式 registry/capability/detect/read/write 与 typed options。
- FontSet/Font/Glyph/Mono/Gray8、稳定 glyph IDs、mapping/sequence、names/properties/metrics、全部编辑/几何/像素操作。
- render/preview/PNG/SVG、outline/vectorization/rasterization、sfnt/collections/WOFF/WOFF2。
- variants/locl、subset、charset、merge、convert/batch/validate/diff；CLI 领域逻辑抽成可调用共享 API。
- 所有 structured reports/errors/locations、loss/strict、资源/capacity/clone、sidecars/directories/multi-assets、适用持久化事务。

Python：pyo3/abi3、Python≥3.10、完整类型提示/例子、Rust handles/生命周期/copies、bytes/buffers/资源集合；原测试兼容、独立原生矩阵、整批与小调用速度都报告；manylinux wheel 本地构建，不发布。

WASM/JS：wasm-bindgen、TypeScript types、Node/浏览器完整与精简 feature；Uint8Array/ArrayBuffer、具名资源集合/显式 resolver、Promise/Worker、进度/取消、handle release、Number/BigInt 和精度边界。浏览器 memory bundle 不等于磁盘事务。真正平台不能实现的项要 queryable、明确 error、证据/替代方案/验收状态；未实现不能冒充平台限制，更不能空成功。W8.3 info/merge/render/Worker 只是全 API 的子集。按追加规格做原生字节一致、Node/headless browser、wasm size/初始化测速。

第 9 阶段网页仍在历史计划中，但不在当前执行授权内；不因到达第 8 阶段就自动开始。远程 push、包发布、网站部署均未授权。
