# 第 1 阶段验收报告：核心、BDF/PCF、渲染、兼容层与 YAFF

日期：2026-10-02 UTC。状态：**第 1 阶段完成；整个项目尚未完成。**

W1.1–W1.6 已审查并合并到本地 `main`。最终实现提交为
`51db4e8e40def36972677db9ab5a4439aea22538`；本报告及 README 修订只改文档。
交付的 `REFS.txt` 记录包含报告提交后的精确分支指针。
本次交付不开始第 2 阶段，不表示已发布、推送或通过远程 CI。

## 1. 已完成工作项

| 工作项 | 实现提交 | 交付内容 |
|---|---|---|
| W1.1 | `e23cc1f` | 有界 Mono/Gray8 Bitmap、全部核心位图操作、Glyph/Font/FontSet、稳定字形 ID、多重映射/序列、精确指标与来源属性、性能基准 |
| W1.2 | `4c4b6f7` | SPEC-v3 BDF 读写、精确规范输出与警告、共享有界 gzip、实际 info/convert、严格损失报告与原子输出 |
| W1.3 | `71bdf36` | 独立 PCF 读写及 gzip，字节序/位序/pad/scan unit、全部金样与真实语料、无需运行 bdftopcf |
| W1.4 | `4367cf0` | 16 个方向组合、原生渲染、preview/render、PNG/SVG 图像、借用位图而不逐字形分配、图像独立验证 |
| W1.5 | `1a8d290` | bdfparser 兼容 façade、精确 JSONL parity runner、完整默认差分测试、为该验收提前实现的窄范围 TTF/SVG 兼容功能 |
| W1.6 | `51db4e8` | YAFF 读写/多字体/gzip、Mono/Gray8、版本化无损原生元数据、可见像素金样与 monobit 交叉验证 |

完整细节、复现命令与各自限制见 `reports/w1.1.md` 至 `reports/w1.6.md`。
格式字段/依据/损失说明见 `docs/formats/{bdf,pcf,yaff,ttf-compat}.md`。

交付审计实际运行了默认及 all-features 的 `fontpixel formats --json`：
两者均列出 **46** 个唯一格式；只有 **bdf、pcf、yaff** 的 detect/read/write
全部为 true，其余 **43** 项仍明确未实现。PNG/SVG 图像输出不冒充 image/SVG
字体 codec；兼容层 TTF/SVG 也没有被注册为通用原生 codec。

## 2. 最终整体检查

协调者在合并全部工作项后的 `51db4e8` 上重新执行 `scripts/check.sh`，
已确认 **exit 0**。交付审计检查并保留该日志，不把它称作自己另跑的一次测试。

- CPython 3.12.14 生成表漂移检查：53 非 Unicode key、35 core + 12 CJK 表、
  207 GB18030 区段、1,087,996 个四字节映射；BDF Unicode printability 712 区段
- `cargo fmt --all -- --check`
- workspace/all-targets/all-features 及 core/no-default Clippy，均 `-D warnings`
- workspace all-features、workspace defaults、core no-default、core BDF+PCF、
  core CJK-only 五种测试配置
- 原有未放宽的 cargo-deny：advisories、bans、licenses、sources 全部通过

日志共记录 **461 次通过的测试调用、0 failed、0 ignored**，跨配置会重复
同一测试，并非 461 个独立测试。生成 oracle 内部含更多案例，不与此数字混计。
Python/WASM 保留 crate 编译成功不代表它们已有功能绑定。

`checks/phase1-parent-check.log`：51,688 bytes，SHA-256：
`51bb05e62f8f48e3ffcc889c5c33ae0c106f6d5ba18abf1a9bce5df90f276ebe`。
此后仅改文档，未改经测实现、锁文件、feature 或测试。
各工作项另有 warning-free Rustdoc 和独立 feature 验证，详见对应报告；
最终整体脚本不包含外部工具全语料重跑或全部 reader 的 fuzz 重跑。

## 3. PLAN §9.2 完整默认差分门槛

W1.6 完成后对最终源码实际重跑完整 suite，**exit 0，56.9 秒**：

| 阶段 | 规模 | 结果 |
|---|---|---|
| tables | 67 charset tables，原始代码空间/BMP 比较 | 0 differing tables |
| random | 2,000 cases / 35,941 operations | 0 divergence |
| random_all | 1,000 cases / 17,588 operations | 6 个严格识别的 K12，0 NEW |
| probes | 41 cases / 89 operations | 1 个 K12，0 意外状态变化 |
| corpus | 251 files / 5,221 operations | 0 divergence |

合计 **3,292 cases、58,839 operations、0 NEW differences**，表比较另计。
采用原始 seed/default counts/全部 operation families/完整 corpus；没有
`--skip-corpus` 或缩减阶段。K12 仅对应非法 ragged bitmap，Rust 事务式拒绝；
保留全部 7 项报告，不声称这些输入也逐项相同。K11 大整数精度豁免不适用 Rust，
已移除；其余历史差异逐条审查并作为严格回归，见 `tests/parity/README.md`。
104 个离线操作脚本（1,718 operations）、Mini BDF/SVG 及 8 组精确 TTF 金样
也纳入普通 Cargo 测试。

证据：`tests/crosscheck/yaff-parity-summary.json` 与
`reports/w1.6-evidence/parity.log.gz`；较早 W1.5 的完整逐项报告保存在
`reports/evidence/w1.5/`。交付审计独立重新计算当前源码 fingerprint：
Rust `28af08cdeda073b7`、Python `938d9ed560ef29ba`，与 W1.6 该次测试
开始/结束均相同。证据标记当时源码尚未提交；现在相同字节已提交到 `51db4e8`。
审计同时验证 W1.6 全部 5 份压缩日志解压后的大小与 SHA-256 均匹配验证记录。
这不是新跑一次 parity 的声明。

## 4. 金样、外部工具与稳健性证据

以下为已经提交的工作项验证，最终 Cargo 回归另外覆盖其离线部分：

- Bitmap：3,841 Python oracle cases；Mono/Gray8 各 4,000 随机 scalar
  对照；1,456 个布局往返；全部通过
- BDF：924 精确 oracle cases，512 fresh-model 属性案例；59 个原始 BDF
  字体的规范字节/警告与 Python 一致，含全部 7 个通格 BDF 与 190 个 HanWang
  unencoded glyphs；两种小字体共 574 glyphs 的 monobit 双向语义对照通过
- PCF：455 oracle cases，512 fresh-model 属性案例，139 个物理 PCF、
  107 个原始 hash goldens、800 个 TypeScript 随机 hash crosschecks、
  全部 59 BDF 的 Python/bdftopcf 默认字节对照通过
- PCF 的非默认 scan-unit/pad-8 沿用两个参考实现修正后的行为；107 金样中的
  7 个及 800 随机项中的 67 个与当前 bdftopcf 的已知编译器行为不同，
  不隐藏或改写它们。139 个 PCF 中 115 个首轮即与输入一致，另外 24 个
  按 Python 同样规则规范化，全部第二轮稳定
- monobit 0.54 的 PCF 双字节编码行/列轴反转问题有单独披露：完整 pixels/
  metrics 双向一致，单字节子集的代码表一致；完整双字节代码表不冒称通过。
  `fc-query` 接受通格 SC PCF，family/pixel size 14 正确
- 渲染：83 字体、2,919 cases（2,509 精确像素/尺寸，410 预期错误）在
  Python/Rust/TypeScript 间通过；Pillow/CairoSVG 独立校验 48 种方向/缩放/
  透明调色组合；588×108 PNG 与 SVG 栅格化 RGBA 完全一致
- YAFF：11 个确定性测试、200 生成案例及 DEL regression；574 glyphs 的
  monobit 双向像素/指标/Unicode 与规范 BDF 重建通过；另有两 strikes、四
  glyphs 的灰度/label/加性指标对照。没有声称 YAFF 的独立逐字节规范 oracle
- 真实 reader fuzz：BDF **1,425,975**、PCF **3,904,582**、YAFF
  **1,785,199** 次执行；各验收 run **611 秒、exit 0**，无 crash/timeout/
  ASan error。PCF 最后 fuzz 后的小幅等价迭代改写及 BDF-only 损失诊断边界
  已在 W1.3 报告披露。早期 LeakSanitizer 退出检查被 sandbox ptrace 限制
  阻塞，干净验收 run 只关闭 LeakSanitizer，ASan 保留；未改变安全设置，
  不声称已证明无内存泄漏

新兼容 TTF reader 的独立 fontTools/OTS/FreeType/HarfBuzz 全覆盖与十分钟
fuzz 仍待第 2 阶段，现有 malformed tests 和 parity 不能替代这些检查。
第 1 阶段所需工具未因安装失败而跳过；后续工具/格式验收尚未开始。

## 5. 性能：五次墙钟中位数及边界

机器：共享 Linux x86_64 guest，AMD EPYC 9V74，9 个可用逻辑 CPU，
10,206,508 KiB RAM（约 9.73 GiB）；Rust 1.99.0 release、Python 3.12.14、
Node 24.19.0。resident input bytes、单线程，排除输入文件 I/O/进程启动。
输入通格 SC：5,746,021 bytes、38,653 glyphs；PCF 输出 2,995,428 bytes。

| 操作 | Rust | Python | Node |
|---|---:|---:|---:|
| BDF parse（W1.3 最终测量） | **85.623 ms** | 351.299 ms | 251.798 ms |
| BDF parse + PCF write | **135.772 ms** | 946.846 ms | 721.040 ms |
| 仅 PCF write（已解析 model） | 52.543 ms | 393.515 ms | 543.451 ms |
| PCF read | 16.484 ms | 364.847 ms | 58.667 ms |
| 80×25 字符 screen | 1.006 ms | 43.804 ms | 9.282 ms |
| drawall：38,653 个 effective codes | 29.189 ms | 1103.633 ms | 423.255 ms |

第 1 阶段指定的 **BDF parse ≤100 ms、BDF→PCF ≤200 ms** 按 PLAN 五次
中位数方法通过。交付审计从已提交 raw JSON 重新计算上述中位数，未新跑基准。
PCF 端到端 Criterion 点估计也为 147.73 ms（95% 145.52–150.71 ms）。
共享机器性能没有固定 SLA：W1.2 parse 中位数 92.418 ms，但 Criterion
点估计 100.38 ms；W1.3 与完整检查并发的早期 parse 中位数 104.550 ms。
这些不利结果仍保存在基准报告，未只保留最快数字。

YAFF 从已解析 FontSet 写出 31,774,112 bytes：五次中位数 **175.820 ms**；
读回 **338.544 ms**，每次 exact native-model equality 成立。这里**排除
BDF parse**，因此只证明 YAFF writer 的测量，不证明 PLAN §10 的完整
BDF→YAFF 转换 ≤200 ms。W1.6 中“YAFF write target pass”应按此范围理解；
端到端 YAFF 目标仍需后续复测。OTB、TTF/WOFF2、整套通格导出和收录站 batch
目标也尚未验收，不影响本阶段明确限定的 BDF/PCF 门槛。

原始数据/方法：`benches/{REPORT,BDF-REPORT,PCF-REPORT,RENDER-REPORT}.md`、
`benches/results/`、`tests/crosscheck/yaff-timing-results.json`。

## 6. 新增决定摘要

日期、问题、选择与理由全部保留在 `docs/decisions.md`；本阶段共 21 条：

1. Bitmap 使用有界 fallible 几何与显式 palette，保留零面积尺寸；核心保持
   矩形不变量；稳定 IDs、重复源映射、原始属性与无效 Unicode 数值来源可检查
2. 真实 FontSet 接到原 registry；benchmark 依赖仅为开发用途，no-default
   production graph 保持空；不以 feature 开关冒充已实现功能
3. BDF 逐字段 provenance 兼顾 SPEC canonical output 与语义编辑优先级；
   gzip 属于传输层；fuzz 依赖许可与 sandbox LSan 限制单独记录
4. PCF 校验 wire accounting，共享 X11 投影而不强制开启 BDF；保留 aliases，
   明确编译器修正与 monobit 编码局限
5. 渲染遵循 Python cell/direction/width wrap，另加 native ink；借用 raster、
   限制输出与诊断预算；图像导出和未来字体 codec 分开
6. 完整 phase-1 parity 必须覆盖 TTF/SVG，因此把窄范围 compatibility
   算法提前；W2 仍负责通用格式架构与独立验证
7. 动态部分指标仍附着 native model，generation-check 防止 stale handle
   错删；精确 UTF-16/lone surrogate 与任意大整数，只有严格 K12 例外；
   既有 BDF float 测试保持原来的 f64 语义，未弱化整数/字节比较
8. YAFF 用普通可见像素/metrics 加版本化 namespaced metadata 实现无损
   golden；portable 编辑优先。拒绝坏结构与 path-only glyph，保留尚未解释
   的 vertical/kerning/scalable 字段，不宣称已有 stroke renderer

## 7. 保护原始输入、恢复及交付审计

交付审计从原始 ZIP 直接提取每个文件的 SHA-256 并与现场比对：
**631 reference + 59 sample = 690 文件，0 changed/missing/extra**；原 ZIP CRC
全部通过，PLAN.md/FORMATS.md 与原件逐字节一致。

- 原任务 ZIP SHA-256：
  `c3c46654818ae828ddf5f40a2d4ac8cb2412d78fde08c5dbaac4a8b84c493040`
- 原始保护清单 SHA-256：
  `2e91f1883c5e61353ecaeeb6a565c2cddec5144fa7b96c8a4d56208d349ae7c1`

原 `fontpixel-rs/` 的 W1.4 工作被中断后，在独立 recovery worktree 完成。
审计确认原目录全部 **32 个 dirty/untracked 文件**和路径集合仍与恢复前
快照一致，没有 reset/clean/覆盖。旧 `w1.4-rendering` 分支仍指向 W1.3
`71bdf36`；完成版是 `w1.4-rendering-recovery` 的 `4367cf0`，且已进入 main。
两分支都保留；旧分支不是尚未合并的待办。Git bundle 不包含未提交工作目录，
交付的是已审查的完成版及完整提交历史。

交付目录 `fontpixel-rs-delivery/phase1/` 包含 `--all` bundle、本报告、
恢复说明、分支清单、校验清单、最终 aggregate log、source audit 和格式实测
JSON。没有 target、依赖下载、原始大字体或原工具链。打包步骤验证完整 bundle、
全新 clone、全部 local branch 恢复、`git fsck --full`、ZIP CRC、精确文件覆盖
和 SHA-256；最终结果与精确大小见同包 `VERIFICATION.json` 及包外校验文件。
每份独立普通 ZIP 严格小于 17,000,000 bytes，不使用需拼接的 split ZIP。

## 8. 已知限制、模型记录与下一阶段

除上述 K12、共享性能波动、外部 PCF 工具差异及 YAFF 测速范围外：

- 任意 native metadata 可能在第三方 YAFF rewrite 后丢失；stock monobit
  的零面积处理与原生 exact roundtrip 不同。详见 YAFF schema/损失说明
- native renderer 没有 shaping/bidi；PNG 是 Mono + 两色 RGBA palette；
  灰度转换必须显式给 threshold 并产生损失诊断
- BDF/PCF 不支持全部 native 字段或 multi-strike；loss/strict 明确处理。
  资源上限约束每次 model/操作，不是全进程 allocator quota
- 通用 sfnt/CFF/OTB/WOFF/WOFF2/SVG-font codecs、variants、批处理、其余
  格式及 Python/WASM bindings 都属于后续阶段

W1.1、W1.2、W1.3、原 W1.4 与恢复续作、W1.5、W1.6 及本次交付审计的
请求配置均为 `gpt-6-astra`、`xhigh`、不继承聊天记录；项目要求 fast。
实际服务侧 model/effort/fast 路由缺少独立可核实状态：**无法核实**。
本次审计未启动子 worker，也未以可设置参数冒充已核实的运行状态。

目前无须用户作出阻塞性决定。值得重点检查的是 YAFF 元数据的跨工具可移植性、
K12 的非法输入边界、BDF 性能余量，以及第 2 阶段完整 sfnt 验证。
下阶段按 PLAN §12 进入 **W2.1 sfnt 公共层**，再按依赖整合轮廓/CFF/内嵌
点阵/WOFF/variants/raster/SVG；复用已测试的兼容算法，不把它误算作整个
OpenType 阶段完成。
