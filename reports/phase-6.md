# 第 6 阶段：本地接续交接报告，尚未验收

**本地接续更新：** 已完成独立恢复、导入原项目路径及首次真实普通类型检查；
新增执行证据与仍未完成项见 [本地接续记录](phase6-local.md)。下文保留云端交接时
状态，历史工具链阻塞不代表本机现状。第 6 阶段仍未验收。

日期：2026-10-04 UTC。交接包名 `fontpixel-rs-local-codex-handoff-20261004.zip`。最终源码提交、refs、文件与证据 SHA 由包内 `HANDOFF-MANIFEST.json` 决定；本文不写自身最终提交号。

## 结论与停止边界

第 6 阶段已有恢复/重建的实现、文档、回归源码和完整工作负载工具，但**尚未通过正常编译、测试、原生独立对照或正式性能验收**。官方 Cargo 获取受网络策略阻止，libstd、rustfmt/Clippy 也不完整。在线可安全完成的源码审查、parser-only、文档/工具检查与外部-only oracle 已做并保留；本次交付是供本地 Codex 继续的未验收源码包。

用户最新要求在线 fontpixel 在交接后停止，后续软件工作转本地。另行授权在线恢复 western-rest 字形修补不属于本包。本地须先完成第 6 阶段，再按授权依序继续第 7、8 阶段；本包本身不满足此前置。第 9 阶段、push、package publish、website deploy 不在当前授权中。

详细操作、源文件清单、所有边界和命令见 [本地交接](../docs/LOCAL-CODEX-HANDOFF.zh.md)；可直接复制 [本地接续 prompt](../docs/PROMPT-FOR-LOCAL-CODEX.zh.md)。[TODO](../TODO.md) 是当前行动清单。

## 来源与恢复程度

约18:33 UTC 原工作区/工具链/未外备份 Git objects、二进制、raw logs 和 generated outputs 不可访问，原因未确认。随后从完整校验的原始阶段5交接包恢复 main `12710057e9787318d1db70db41a60fca016ad53c` 和原54 refs。原 ZIP SHA `8cc36c6f817347f808c1ef6f2ab1450715380d715bca2f4e80106e2df9bcb524`，53,478,221 bytes。原始参考、样本、validation inputs 保留只读。

恢复过程混合了有旧hash的逐字节恢复（compat lookup生产/测试、三个文档示例）、记录补丁重放、新实现和新的源级修正。重建 checkpoint `e4f65fe5372b65d31e97254b7b67869efd90aaa7` 之后又有已记录的未提交 owner 改动，最终包清单才标明真正交付源码。组件 provenance 放 `handoff-evidence/recovery-provenance/`，旧快照不会覆盖最终 main/source audit。

旧 phase-6 的2,918执行/649套件、完整compat、18文档CLI/84mini tasks、已生成字体与五轮性能raw证据没有完整恢复；历史summary/hash不能代替其内容，更不能验证重写后的组合。第5阶段原报告保留，但只属于原版本。

## W6.1–W6.6 状态

| 项目 | 已有源码/资料 | 当前剩余验收 |
| --- | --- | --- |
| W6.1 CLI/batch | main/batch/commands/sfnt_validate/assets/input_budget/metadata/opentype/p3_formats；并行TOML、diff/validate/charset、--each、命名与嵌入度量、整组事务/源保护/report spool | 所有普通/最小feature编译、Clippy、运行期测试；报告/退出码/strict/容量/alias/sidecar/目录/rollback/spool stress |
| W6.2 文档 | README、API/rustdoc、bitmap-model、99格式索引、两迁移、batch/Tong示例、重写api_quickstart、静态checkers、中文本地交接 | Rustdoc无警告/doctest、公共API覆盖、example矩阵、全部CLI真实smoke、99格式profile文档审查及真实迁移输出 |
| W6.3 性能 | recorder/独立auditor、22cases准备、Python/JS/fontTools/Tong/catalogue/native helper与完整validator源码 | helpers编译；默认真实full native correctness；全部配对和五次测量；raw/member/median独立审计；硬目标未取消 |
| W6.4 健壮性 | PFR共享物理表cache、8项专用回归源码；99reader/104target/22proptest文件inventory；共享预算回归 | 所有性质/资源/feature gates，最后源码逐required reader≥10分钟有效ASAN；明确用户本地待做 |
| W6.5 merge库 | merge模块、Unicode17分类/选择、spacing/baseline/fallback、license/RFN/full notices、结构化报告、metadata escaped诊断预算 | synthetic与真实输入、全部分支/错误/strict/resource测试；所有临时分配边界与源码保持 |
| W6.6 mergeCLI | 顺序绑定parts/outputs、显式from/to/font-index、多输出事务、batch接入、CLI回归 | 真实Tong地区CJK+Latinfixture尚缺实现/执行；Unifont完整union和<1s；license/name/report/现有文件保护与mixed输出 |

已有 BDF ASCII row、YAFF、OTB、compat lookup 热点优化及共享模型预算 guard；所有当前原生性能/正确性仍待执行，不能称“已恢复历史性能”。

## 当前新证据及其严格范围

1. **最终语法快照，20:41 UTC**：600个Rust文件+10 context，600 parser exit0，无syntax-error spans，捕获/检查期间文件稳定。官方rustc `parse-crate-root-only`，不依赖解析、宏展开、features、build.rs、类型/借用、链接或测试。诊断bootstrap仅限隔离子进程，不能用于生产。归档在 `handoff-evidence/fontpixel-final-rust-syntax-only.tar.gz`。
2. **最终source-only audit**：48文件身份及已修复/剩余项在 `handoff-evidence/final-interface-audit.md` 和 `final-interface-audit-source-checks.json`。没有发现新的确证API调用拼写缺陷，不等于整个工程类型正确。
3. **协调者20:46 checks**：API文档checker86源片段/27文档要点/64links；标准库performance scripts9、workload4、catalogue6 tests通过。初次聚合unittest因import路径失败，随后正确direct scripts通过；两者都保留。没有Rust执行。日志在 `handoff-evidence/coordinator-final-checks/`。
4. **外部-only fixtures**：9个直接Python pins、Node/esbuild准备；631reference文件复制前后hash一致；原Python/Node full SC七产物byte parity；FreeType核对38,067 PCF BMP与38,653 TTF glyph pixels/advance；fontTools/OTS；103glyph web fixture/48 shaping；原Python catalogue181 outputs/105 sfnt应用字段/8ZIP CRC+members。原生Rust/export-rs尚未build或pair，正式timing未做。详见 [oracle setup](../benches/phase6-oracle-setup/README.md)。

外部evidence ZIP：759,210bytes/74members，SHA `ecea9b1bbb1fa446ea7972b5976e4ee3391061ded35196bea63308143b2da65e`；generated fonts ZIP：16,392,704bytes/199members，SHA `60e5b57832a2e580a31cecf4e0020716f1a37d152de09e6950cf35b98783d32b`。这些hash标识已验证外部材料，不是Rust结果。

## 最新修正与仍然存在的源级范围

最终source audit确认：显式format/feature验证、resolver capacity、NativeWriter common limits和plain所有asset length cap、CLI真实caller model/report保留、merge初始两数组、escaped audit诊断、typed OTB flags、helper feature注册、测试专属wrapper cfg等都已在源码。旧proposal/FINDINGS中“尚未修”的对应条目属历史。

必须继续闭环：

- standalone Sources 未附 batch那样的aggregate identity ledger。
- CLI Variants caller overlap已修，core planner/export/WOFF全部同时存活transients和subset read/selection/internal code仍未完整覆盖。
- zero guessed source credit可能保守重复收费；InputBudget也保留已释放选择前storage的保守reserve。默认fullTong/Unifont/catalogue必须重测，不删收费/抬默认cap/删报告强行通过。
- W6.6真实Tong CJK+Latin对应区域构建的fixture缺少。现有Unifont union或Tong variants/locl都不能替代；须新增地区/尺寸/spacing映射、完整逐glyph验证和差异类别说明。
- catalogue旧检查遗漏bitmap family/style/copyright。reference policy/defaults现已识别且v2 adapter/flags存在，native完整metadata/metric/name/notice/pixel/.notdef/table/ZIP配对仍缺。原fontTools fsType/selection/breakChar默认不等于许可指示。
- 原生产opf_vector source/binary缺失，只能明示supplied fontTools fallback；不声称其生产速度。variable axes/hints/per-glyph offsets真实未实现项保留外部fallback，不伪装原生覆盖。

所有ResourceLimits实际默认保持256MiB；batch working1GiB/pending512MiB/单task report64MiB/总report2GiB。pending包括outputs+共享identity capacities，identity admission失败sticky禁全部提交；job/path/task限额独立。plain common cap检查返回asset length，不能阻止所有codec先分配；None/gzip/default路径不变。没有process RSS保证。更完整说明见 [writer budget](../docs/w6.1-writer-budget.md) 和 [模型](../docs/bitmap-model.md)。

## 性能：未达标/待测，禁止改门槛

| 必需工作负载 | 目标 | 最后历史观察，仅供方向 |
| --- | --- | --- |
| Full SC单线程parse | ≤100ms | direct113.54ms未过；public98.89ms是另一口径；raw已失踪 |
| Full SC→PCF/OTB/YAFF | 各≤200ms | PCF194.90、OTB249.72、YAFF278.59ms；后两项未过 |
| Full SC→squareTTF | ≤1s | 793.06ms仅历史 |
| 默认完整Tong8vector families+20PCF+20OTB | ≤40s | 66.77/63.38s为两次诊断，非五轮final，未过 |
| Unifont+Latin→57,106glyph BDF | <1s | 正式五轮尚未完成 |
| 全catalogue与Python、bdfparser/JS/fontTools/export-rs/WOFF2 | 报完整同语义比较/时间/体积 | 最终22cases五进程suite未完成 |

新机器需完整correctness后冻结source/input/dependency/binary，五个新进程/实际quiet window/全raw与outputarchives/独立audit。不能用API、tiny、改变defaultcap、删reports或移动计时范围取代目标。详见 [性能合同](../benches/phase6-README.md)。

## 本地完成顺序与安全边界

1. 按包根verify/restore/verify-checkout恢复全新空目录，核对最终manifest；不要覆盖旧项目。
2. 官方完整工具链、最先backup后fmt/typecheck/scripts/check.sh/rustdoc/feature/fuzz-dependency；真实失败按源码ownership修复，保留失败记录。
3. 全共享预算/transaction/merge/PFR/metadata tests、API examples、fullparity/corpus、99profiles、CLI docs smokes；做缺失真实Tong mergefixture。
4. 完整native/oracle真实字体配对；metadata/defaults/metrics/notice独立审计；保留能力边界。
5. 正式性能五轮与全部日志/archive复核；所要求ASAN最终源码每required reader≥10分钟由用户本机完成，遵守实际权限。不把被拒绝/中断/短smoke/旧71campaign称通过。
6. phase6整体完成、记录全部验收后交付，然后phase7、8。两绑定必须覆盖所有native公开能力+独立legacy compatibility；逐type/operation/option/feature/result/error覆盖表和tests，平台差异显式，不静默省略。见 [绑定范围](../docs/bindings-scope.md)。

建议六Astra/xhigh/fork-none workers；用户已要求尝试fast mode，须按本地实际支持配置/记录，不能设置或核实就如实说明；在线接口未能设置验证。协调者统一共享代码与build/timing窗口。只读siblings不改，产物放local-work；重大改动和长验证前后频繁外部durable backups，hash读回+空目录restore。final ZIP/all-ref完整性、外部保存成功以最终delivery record为准，不能提前宣称。
