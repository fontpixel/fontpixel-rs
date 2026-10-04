# 2026-10-05 本地暂停记录：第 6 阶段未验收

用户最新指令是「停止，把进度详细记下来」。2026-10-05 00:50 Europe/Paris
（2026-10-04 22:50 UTC）停止开发与验证，只做进度记录及备份校验。
**等待用户明确恢复后再继续。** 不进入第 7、8 阶段，不启动第 9 阶段，
不远程 push、不发布包、不部署；western-rest 不属于本项目。

本文件是本次本地暂停的优先入口。[本地执行记录](../reports/phase6-local.md)
补充逐轮过程；[原云端交接](LOCAL-CODEX-HANDOFF.zh.md)、PLAN/TODO 中较早的状态
不能覆盖本文件。原授权的后续顺序仍是先真正验收第 6 阶段，再依次第 7、8 阶段；
当前暂停期间不自动执行这些后续任务。

## 1. 目录与版本：不要重新覆盖导入

| 名称 | 位置 |
| --- | --- |
| 当前项目 ROOT | `/home/chen/githubprojects/fontpixel/fontpixel-rs` |
| 本轮证据 WORK | `/home/chen/githubprojects/fontpixel/local-work/phase6-local-20261004` |
| 原 ZIP | `/home/chen/Downloads/fontpixel-rs-dot/fontpixel-rs-local-codex-handoff-20261004.zip` |
| 独立恢复包 PKG | `/home/chen/githubprojects/fontpixel/fontpixel-local-20261004-qc6uoscg/fontpixel-rs-local-codex-handoff-20261004` |
| 可写参考副本 | `WORK/performance-work/reference` |
| 固定 oracle Python | `/home/chen/githubprojects/fontpixel/local-work/oracles/bin/python` |
| Cargo target | `/home/chen/githubprojects/fontpixel/local-work/target-fontpixel-rs` |
| 本次停止状态清单 | `WORK/user-stop-20261004T2250Z/` |
| 最终暂停备份 | `WORK/checkpoints/user-stop-20261004T2250Z-final/` |

导入已经完成：按恢复脚本在全新目录运行 verify、restore、verify --checkout --zip，
再通过离线 bundle fetch/fast-forward 导入原项目路径。原只读 reference/samples/
validation-inputs/original-task 保留；没有删除旧项目凑空目录。详情及原工作区完整
备份 SHA 在本地执行记录。原 ZIP SHA256 为
`624b8176577bb5a60aad240a51f3a150af006a42f9a36c32a9053e7790bf9b0e`。

版本链：阶段 5 `12710057e9787318d1db70db41a60fca016ad53c` → 云端 manifest
`5b42ce242c76235143353d5f8e83368fcd2f0595` → 本地 `427ad94`（格式与 lock 最小修正）
→ `5e65c3a3c812832b077cebcff639c766d7970573`（第一批实现/工具/证据）
→ **代码检查点 `a1b40b01008de04236b00010d83c56dfebbfdf74`**（部分 codec 预算及完整
对照发现）。这些本地提交均没有宣称第 6 阶段验收通过。

之后停止记录可能有单独文档提交；准确 HEAD、所有 refs、dirty status 和文件 SHA
以最终暂停备份的 `source.json` 为准。云端 HANDOFF-MANIFEST 只验证原冻结包，
不适用于已经继续开发的当前 dirty 工作区。

## 2. 暂停时源码不是可直接宣称通过的版本

`a1b40b0` 后有未提交实现，**尚未运行 Rust 编译、fmt/Clippy 或 Rust tests**。
全部保留，不回退或清理。尤其：

- `lib.rs` 已加入 `pub mod projection;`，但 **`src/projection.rs` 尚不存在**。
  因此当前工作区在停止时就有明确的不完整模块，不能拿此前 cargo check 的 pass
  证明现在可以编译。不要为了让检查绿而悄悄删登记、丢草稿或伪造空实现。
- `sfnt/working.rs` 的共享预算实现已移到新 `src/working.rs`，旧路径改为 reexport；
  `lib.rs` 已登记私有模块。outline 的公共/私有调用和预算接线处于半完成状态。
- `model/mod.rs` 增加私有 `set_scalable_advance_prevalidated`：供未来 projection 在
  独占 FontSet、预验证所有 live GlyphId、完成全部 staging 后无分配提交 SWIDTH。
  它尚无实际调用者；不能从该 helper 推断 projection 已实现。
- GSUB/subset 已有新共享预算草稿，未编译，glyf/CFF/其它 table 生命周期尚未补齐。
- CLI 三个失败回归已有最小修正源码，但修正后尚未执行 Rust 测试。

暂停前原始 dirty snapshot/diff 在
`WORK/user-stop-20261004T2250Z/source-before-record.json` 和
`working-before-record.patch`；最终归档还保存所有 untracked 源码文件，不能只用
`git diff` 恢复。该清单之后仅补停止文档，没有恢复功能开发。

## 3. 六个 worker 的真实分工与停止点

确实使用了六个 subagent，名称 `/root/w61` 到 `/root/w66`，不是只由主会话执行。
按要求请求 gpt-6-astra/xhigh/fork_turns=none；工具没有实际 backend/service-tier
独立回执，不声称配置已验证。本机 fast_mode 配置可见但 service_tier 为 default；
strict-config 校验尝试不受该子命令支持，不能声称已启用实际 fast 服务。

编译、Cargo/registry/lock/main/lib、集成与正式测速窗口始终由协调者管理，jobs=2。
停止时 w61/w63/w64/w65/w66 已 interrupt，w62 为平台拒绝错误状态；没有继续分派任务。
持久记录在 `WORK/user-stop-20261004T2250Z/agents-stopped.json`。

| Worker | 已形成的工作 | 暂停时待续/证据入口 |
| --- | --- | --- |
| W6.1 | standalone Sources identity ledger；subset CLI caller 预算；batch 状态与 fixture 修正；CLI 93 unit 曾通过，input_budget 8 新回归通过 | `owners/w61/fix-038/` 保存三项 CLI 修正的 patch/manifest；commands.rs 与 phase6_cli_recovery.rs 未重新测试。embedded/name 的完整 metadata/Arc 名字表接口还在 owner 草稿，未落 core。将来接 W66 projection；main/registry patch 交协调者 |
| W6.2 | 99 格式/13 命令文档 checker、api_quickstart 与 18 CLI/84 tiny runner；真实 smoke 已由中央执行通过 | `owners/w62/central-validation-checklist.md`、`central-validation-plan.json`。九个 API feature profile 的运行/Clippy、rustdoc/doctest 尚未执行。ASAN runner 与后来独立 Brotli encoder 草稿两次被平台拒绝；不得冒充已实现 |
| W6.3 | 真实性能 adapters、固定依赖清点、完整 catalogue/Tong 元数据/物理字形 validator；全部 181 项差异分类 | 新 catalogue checksum/ZIP 状态/集合 hoist Python 改动已落地，owner 的 stdlib/小 consumer tests 通过；没有重新完整跑 173+8。`VALIDATION-RUNBOOK.txt`、`catalogue-all181-classification.json`、`SEMANTIC-DIFFERENCE-CLASSIFICATION.json`、`tong-actual-20-otb-fields.json` |
| W6.4 | partial shared Budget/Lease/Owned/Vector、Variants alias 提前检查、零面积边界、固定 miniz zlib 与 Brotli decoder allocator；已有相关 tests 通过 | 新 src/working.rs、outline/{mod,packed,spec}.rs 半完成；后续 dot_rows/export/plan 与 WOFF 外层共享生命周期仍待补。Lease::merge 应改传 &mut Lease，异预算错误不能提前释放另一侧 charge。Brotli encoder vendor 动作被拒，明确未实现 |
| W6.5 | merge notice/LICENSETEXT/报告聚合预算；subset directory/cmap/final buffer 首批；新 subset 3 tests 通过 | 新 sfnt/gsub.rs 与 subset.rs 草稿未编译；下一批目标完整 GSUB decode/closure/remap/encode、keep/mapping/queue，同一 Budget；glyf/CFF/其它表与 report 仍未覆盖。`subset-batch1-status.md`、`subset-budget-design.md` |
| W6.6 | 原 tools/build.py 真实 20-case fixture；4 profiles 共 80 个 native Tong merge cases 全通过；Unifont 完整 provenance gate 通过 | `CATALOGUE-BITMAP-PROJECTION-PROPOSAL.txt` 与 68 BDF/PCF 全量审查。通用 projection API/PCF precedence 获准开始，但停止前尚无 projection.rs；仅中央加了 module/helper。未实施新版 projection，不得写为完成 |

下一批文件 ownership：W61 CLI、core formats/embedded.rs 与 sfnt/name.rs；W64 新
working、outline、variants/dot_rows/export、WOFF；W65 sfnt subset/container/cmap/
GSUB/glyf/CFF/metrics/post/os2/bitmap；W66 新 projection/PCF writer/options/tests；
W63 benches/scripts/examples；协调者 lib/model/main/Cargo/registry/lock/报告。
跨 owner 只发具体 patch/接口，不直接抢写。

## 4. 真实本地执行结果

全部 41 个中央 gate 都已有 result.json；准确命令、exit、source before/after 在
`WORK/gates/NNN-*/`，汇总和各 result hash 在停止清单的 `gates.json`。
`source_unchanged=false` 可能来自明确 formatter、并行 owner 修改、文档或 refs 变化；
不能将该 gate 当成最终冻结源码验收。旧不可变二进制的输出单独绑定 frozen manifest。

| 执行 | 结果 |
| --- | --- |
| 恢复/导入/原工作区备份恢复 | passed；原包 74 refs/3233 source files 校验通过 |
| 本机工具链 | Rust 1.99 b940084d、rustfmt、Clippy、deny0.20.2、fuzz0.13.2、nightly/rust-src、Python3.12.13 可用；不用 RUSTC_BOOTSTRAP |
| 初次 fmt/check | 002 fmt failed；003 locked check 因 toml→indexmap 缺依赖边失败；保存原 diff 后最小修正，无依赖版本更新 |
| 005/012/034 all-target/all-feature workspace check | passed with warnings，仅覆盖各自快照。034 的 warning 是尚未使用的预算 helper，不以 allow 压制 |
| 013 初批 core | 123 tests passed；CLI unit 91 passed/2 failed，后续已修并在 017 达到 93 passed |
| 017 | core lib55/WOFF16/CLI unit93 passed；旧 input fixture 7 passed/1 failed；修后 038 input8 全通过 |
| 020 all-feature release | passed，source stable；不可变副本已冻结，见下一节 |
| W62 18/84 runner | 18 real CLI smokes + 84 tiny tasks + 13 help spellings passed；tiny 是明示新 CC0 fixture，不代替 full fonts |
| 015/021 原 Tong | 原 Python build 重建20 BDF byte equal；原 export-rs 完整84 artifacts 生成，external-only |
| 024/028 native Tong CLI/API | 各完整84 artifacts 生成成功，默认 cap、完整 reports；generation-only，语义见 031/032 |
| 029 真实地区 Tong merge | 4 profiles × 20 cases = 80，全部 native contract passed；含 strict/mono control；900 artifacts；原构建差异逐项留存 |
| 026/027/030 Unifont + Latin | 57,106 glyphs 完整 source/index/pixels/metrics/names/notices/JSON provenance passed，0 glyph/report errors |
| 018/019/022 catalogue | 173 fonts + 8 ZIP 两侧完整生成；Python明确 supplied FontTools fallback，生产 opf_vector 缺失 |
| 025 catalogue paired | 全部检查已跑；173 fonts 中 8 pass、164 unresolved、1 fail。8 ZIP 原包装检查 pass；后审证明5个仍继承字体成员未决，不能全称语义 pass |
| 035/036 decoder fixture | 035 core lib67 pass/1 fail；036证明正常5bytes在1/4/8MiB成功，但截断在1MiB先 ResourceLimit |
| 037 decoder fixture 修正 | passed：保留低额度 ResourceLimit，8MiB下4种畸形条件必须 InvalidInput；只调测试条件，生产 cap 不动 |
| 038 core integration | cff11 + sfnt29 + subset3 + variants25 + WOFF16 = 84 passed |
| 038 CLI integration | input_budget8 + merge26 passed；phase6_cli_recovery36 pass/3 fail；后续修正未重测 |
| 039/040 full SC | 完整38,653字形输入；Rust compat/Python/Node七产物 byte equal；native PCF/OTB/TTF FreeType、YAFF monobit 合计11 checks passed |
| 041 SC vector/dot preparation | native square、FontTools square、Latin dot source 生成成功；独立 vector paired 与 same-sfnt web 尚未运行 |

原生 performance helper 在普通生成中会打印计时，有的写 `formal_eligible=true`，
该字段仅表示配置适用，**不表示这一运行是正式计时**。本机仍有其它项目负载，
没有 quiet-window-confirmed 的正式五进程结果；所有性能硬目标仍未验收。

### 031/032 的用户中断，不算完成

这两项 full Tong CLI/API paired 在停止指令后被 SIGTERM；各 result.exit=-15，
外层程序退出码241。初次进程筛选没有匹配绝对解释器路径；随后按 /proc 的精确
argv 找到两个 gate 的子进程并终止，未触碰用户其它项目。
`USER-STOP-processes.json` 与 `USER-STOP-processes-exact-argv.json` 分别保存两次记录。

两份 proof 都是 partial=true，各完成45条记录：20 PCF pass、20 OTB metadata fail、
3完整 vector paired 和2 same-sfnt web 成功记录。3 vector 分别44833/44833/44834 GIDs，
每条912 shaping batches，完整几何/metrics/pixels 均已检查。不能外推剩余字体通过。
两侧 OTB 的全部20字体 cmapGID、hmtx、physical presence、FreeType metrics/pixels
已有相同证据，但 names/global/strike 字段仍未解决。停止状态与 hashes 在
`user-stop-20261004T2250Z/proof-status.json`。

### 当前三项 CLI 回归及修正草稿

1. charset strict 预期exit3却exit2：测试使用尚无的 `--max-table-bytes`。草稿增加
   **输出 writer 专用**的参数，不改变 reader 限制；mapping 后派生 writer resources，
   strict loss 仍优先。加入默认bytes/report不变、strict3、permissive cap失败/旧输出保护。
2. embedded fixture duplicate strike：SIZE/BBX改4但 XLFD仍2，parser实际读成两个2ppem。
   草稿同步 XLFD4/40，并断言 parsed ppem2/4。
3. cmap 超限 fixture：原0..0x10ffff跨 surrogate，先触发非法输入；草稿改为合法
   0x10000..0x10ffff，仍超过完整 mapping work cap，并另保留跨 surrogate InvalidInput 控制。

原失败都保留。修正位于当前 dirty CLI 文件及 `owners/w61/fix-038/`，**尚未 native 重测**。

## 5. 实际差异与正在设计的接口

Catalogue 的164 unresolved不是164个已定位的独立实现错误；含真实应用配置差异、
结构派生值、原程序有损政策及未完成的证明。四类全覆盖表覆盖173 fonts/8 ZIP、
61类字段，不允许用全局 ignore 名单改为 pass。

- 向量 `.notdef` 原应用为空且 advance=max(unit*pixel_size//2,1)、lsb0；native 默认
  Tong box/源 `.notdef`，13px所需494 design units不能用整数pixel advance表达。
  W64建议将来 `NotdefPolicy::Preserve`（默认）与 `Empty { advance_width:u16,
  left_side_bearing:i16 }`；有源字形替换需真实Loss，不修成品sfnt。**仅建议未实现**。
- names/ID3/16/17/Mac records、OS2 vendor/fsType/selection/break、head flags/lowestPPEM、
  PANOSE/fixedPitch、post、平均宽度及 EBLC widthMax 有实际差异。W61拟提供通用 typed
  embedded metadata、Arc<NameTable>、完整 caller/scratch预算，默认保持现有行为。
  原Tong widthMax按最大advance；native按bitmap width，须显式line-metric选项，不能
  修改bitmap几何或直接豁免。新options尚未落地。
- Cozette/Terminus/TomThumb/Proggy的 BDF/PCF需要显式 FONT/SIZE/SWIDTH 投影；SWIDTH
  按DWIDTH/ppem精确ties-even，覆盖所有encoded/unencoded物理记录。不是全部简单500→462。
  PCF已有属性与normalized model的优先级需通用选项。Proggy原PCF下载走原字节分支，
  与BDF重建分流；不得借透传隐藏core缺陷。
- 原Python会丢COMMENT；Cozette/Spleen其中有版权/许可证。新projection不得删除这些
  文字来追平输出。逐字保留差异、真实Loss/strict和完整来源审计是必要条件。
- checksum adjustment是派生值，须各自完整目录/逐表/全文件校验后才能解释不同值。
  W63新validator对此有独立证明和负例；hmtx压缩/maxp/cmap/bitmap存储方式仍须逐项实证。
- Spleen64的U+2502，在14ppem、native/reference×TTF/WOFF2均发生FT2.13.2 error98；
  64/128ppem该字形四侧raster相同。只是特定消费者限制证据，不能吞其它异常或替代
  full all-GID实际字号web gate。原025 failure仍保留，新ppem接口还未修改共享validator。

## 6. 预算与平台阻塞

默认256MiB总限制、standalone identity I=min(1MiB,L/256)与working L-I、batch
prepared outputs+shared identity默认512MiB及sticky admission禁止commit均保持。
job/path/task、working/report各cap分开；plain common output cap仍仅最终返回前
aggregate asset length检查，不能宣称所有codec preallocation或进程RSS受控。

已实现的是局部生命周期：source/CLI caller、merge、sfnt directory/cmap/output、
固定miniz workspace、Brotli decoder cells。整个Variants/WOFF阶段、subset全部内部表、
新metadata/projection仍未闭环。BTree与clone exact Rust/target guard不强制私有cfg；
不猜source credit、不删reports、不抬cap。

两次平台拒绝均发生在对应子任务任何工具动作之前：

1. W62 最终源码104-target ASAN runner/auditor；
2. 后来独立的普通 Brotli encoder 可失败分配分析/vendor草稿。

均返回 possible cybersecurity risk。已停止各动作，没有换worker或路线重试；
第二项后还明确通知W64不能接替该被拒的encoder补丁。现有source-only proposal保留，
**不代表实现**。记录为 WORK 下 `asan-dispatch-rejection.json` 与
`brotli-encoder-dispatch-rejection.json`。这些阻塞未解除，不能验收第6阶段。

## 7. 尚未执行或未闭环的验收

- 当前dirty源码的fmt/check/tests；完整 scripts/check.sh 的生成表、两个Clippy profiles、
  全/default/最小/BDF+PCF/CJK tests、deny；warning-free rustdoc/doctests。
- api_quickstart九个feature profiles各运行+Clippy、最小OTB/bdat/color CLI Clippy、
  focused PFR/其它必要资源与事务检查；静态文档checker不能替代它们。
- fuzz workspace lock可能因新增miniz/Brotli decoder依赖边而需正常同步，并执行要求的
  locked dependency audit；此项不是ASAN campaign，不得混称。
- native legacy parity release binary尚未构建/执行全默认2000+1000+corpus/probes。
- 最终源码新release/freeze后重新绑定全部full oracle证据；完成所有Tong profiles、
  catalogue173/8真实语义替换、SC vector、square/dot same-sfnt web和质量/线程/体积对比。
- 真正空闲窗口中的22cases×5新进程正式测量、所有raw/archives/member hashes/medians独立
  audit。硬目标不改：SCparse≤100ms；PCF/OTB/YAFF各≤200ms；squareTTF≤1s；
  默认fullTong CLI≤40s；57106glyph merge+BDF+完整report<1s；catalogue整批时间及原Python对比。
- 最终源码99reader/104target、各≥10分钟有效ASAN未做，当前还有平台阻塞。
- 第6阶段逐条最终交付未形成；第7/8尚未启动。py/wasm workspace skeleton能check不是绑定覆盖。

重新估算记录 `eta-20261005-0045-1fc1` 位于 WORK/eta。5小时中位/10小时保守只是
可继续工作的粗估，排除平台阻塞与额度等待；无强同项目样本、默认judge校准，最近
周额度记录89%。当前用户已经暂停，估算不再作为正在计时的交付承诺。

## 8. 冻结二进制、备份与恢复

`WORK/frozen-binaries-v1/` 是gate020对应的不可变诊断二进制，不包含后续预算草稿：

| 二进制 | SHA256 |
| --- | --- |
| release/fontpixel | `1a10a457ea8ce1fa4fd4f51c85f8fb4b8eba2f0d98db691a8a92bed0815bbe26` |
| examples/phase6_timing | `f3649c53cc7c764cb94063b6d79df5022a47de5212e0d4a26351c1c9b27bc426` |
| examples/phase6_tong_export | `020eb3c64c3235a8801b3fbb85b3e66bbdddb37ee236312c1a12853882e4a2ef` |
| examples/phase6_catalogue_export_name | `4d64f56b584d44015f6e44038dbf0e40dabff8368ee181a12485e35f4836a133` |
| release/tong-export（原参考） | `e6ec5fc8a78072fbf3edbce4ea5107c8fd8666433aa8579e9872bf05996b67c4` |

完整exact build/source身份在该目录的manifest/source/build-result，不由当前HEAD推断。
oracle为九项固定pins；原parent旧八项requirements保留，使用performance-work显式新版。

最终暂停checkpoint包含源码tar（含所有未跟踪源码）、未提交binary diff、全部refs bundle、
完整现有raw证据tar、每member SHA、准确源码清单、独立空目录restore与Git fsck结果。
验证结果以该目录 `result.json`/`SHA256SUMS` 为准，若缺result不得自行视作备份成功。
既有失败checkpoint和被替代结果都保留。所有副本仍是本机；用户未提供外部目的地，
没有擅自上传，也不声称异机备份。

恢复时优先直接使用当前工作区；如需独立复原，使用暂停checkpoint的全新空目录
restore或源码tar+bundle，核对 source.json 的每个文件与全部refs。**不能只checkout
a1b40b0或apply working.patch就认为包含所有untracked草稿。** 不覆盖或删除当前树。
原云端restore脚本用于原冻结包，不能拿它恢复本次新增工作。

收到明确恢复指令后，先读本文件/dirty snapshot/owner冻结材料，审查中断中的模块与
缺失projection.rs；再整合可编译批次、备份、中央gate。不要盲目续跑被终止的旧proof或
重用旧二进制给新源码背书；新执行使用独立路径，旧partial记录永久保留。
