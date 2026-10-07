# 第 8 阶段交付报告：WASM / JavaScript 绑定

日期：2026-10-07。工作仅在本地 `main` 提交，没有 push、publish 或 deploy。
本阶段完成后停止，不启动第 9 阶段，也不编写项目最终报告。

## 结论

第 8 阶段按 PLAN、绑定范围与 W8.3 完成。交接的五个分支全部合并；原生 API 与独立的
bdfparser-js 兼容 API 都可使用。brotli `portable-float` 已启用，大字体 WOFF2 在 Node、
Chromium、Firefox、WebKit 与重新冻结的原生 CLI 之间逐字节相同，原生默认资源上限不变。

三个主门禁 `scripts/check_wasm.sh`、`scripts/check.sh`、`scripts/check_python.sh` 全部通过；
双语言覆盖率门禁 **2,008 项全部映射，0 未映射、0 错误**。Node 18 加载与 Worker 生命周期通过；
三浏览器各完成 66 项功能检查和 14 项示例断言。Node 与 headless Chromium 五轮配对基准、
八种 feature 组合的体积及初始化矩阵、JS 文档和可运行示例均已交付。

兼容参考套件保留有说明的失败：Python 8 个，JS 11 个及 2 个依赖参考包构建产物的套件加载错误。
这些结果与交接基线逐测试一致；不能表述成“参考套件全绿”。

## 重做范围与源码证据

按用户追加要求，撤销上一轮 Sol 在交接点之后的 17 个提交，把 `main` 恢复到 `cac4e0d`，
删除上一轮构建目录后，由本轮当前模型重新合并、审查、实现和实测。被撤销的 `302897f` 历史仅保留在
外部 `../local-work/p8-sol-discarded-20261007/discarded.bundle`，不作为交付代码或验收依据。
交接时已存在的 A/F/J2 以及五个原始单元分支按原任务保留。

合并顺序为 J1 → J3 → J4 → J5 → J6。原分支末提交 `fbebed7`、`38a53d3`、`191bbb3`、
`a5cbbd3`、`6f85876` 均为当前 main 的祖先。关键新提交：

- `79be012` / `c0672d7` / `fb0cf3c` / `5e7b118` / `70163a3`：五个单元合并及冲突处理。
- `1f77b59`：portable-float、共享 tagged request 类型修复；`4c505a8`：新金样与正式三 profile 门禁。
- `b7679d5` / `7346faa`：JS 文档、示例、浏览器、基准和 feature 矩阵工具。
- `4622ab2`：稳定生成式参考文档、按输入和输出 SHA 验证的 wasm-opt 复用、基准 checksum sink。
- `154f077`：参考套件重跑与 WOFF2 更新；`d1f7914`：正式浏览器和基准证据。
- `ef796b4`：初始化矩阵使用异步实例化，修正文档中的浏览器同步实例化限制。
- `3d3e707`：先完成构建，再轮换各组合测初始化；保留首轮数据，逐样本记录负载。

正式三 profile 包构建于 `4622ab2`；之后没有修改被测 Rust/JS 行为，loader 只改了说明注释。
浏览器/基准执行源码为 `154f077`；实验组合构建于 `ef796b4`，最终初始化工具为 `3d3e707`。
包和 JSON 各自记录准确 commit、dirty、工具版本与 SHA，不把这些不同快照冒充同一个提交。

完整命令、前后源码快照、退出码与日志在 `../local-work/phase6-local-20261004/gates/`。
提交的 [gate 索引](../benches/results/phase8-gates.json) 记录命令、日志 SHA 和快照差异。
本轮证据从 gate 224 起；213–223 属于被撤销历史。

## 验收门禁

| gate | 内容 | 结果 |
| --- | --- | --- |
| 224、226–229 | 每个单元合并后的 fast full 构建与 Node 测试 | 全部通过；此时保留的 WOFF2 TODO 随 portable-float 改为正式断言 |
| 230–234 | 新冻结 CLI、Python 扩展/类型、金样、堆复核和大 WOFF2 冒烟 | 通过；准备步骤允许生成文件变化，不能当作冻结后的全量门禁 |
| **236** | **scripts/check_wasm.sh**：release full/minimal/merge、Clippy、Rust、Node、TS strict、示例、文档、双语言 gate | **通过**；full 1,616 过 / 2 跳；minimal 395 过 / 381 跳；merge 563 过 / 351 跳；全为 0 失败、0 TODO；内部 Rust 7 项通过 |
| **237** | **scripts/check.sh**：原生完整门禁 | **通过**；753 个套件、4,136 项通过、0 失败、8 ignored；格式、Clippy、生成检查、示例、cargo deny 通过 |
| **238** | **scripts/check_python.sh**：新 release 扩展、类型、pytest、覆盖率 | **通过**；pytest **1,262 全部通过**，无跳过，覆盖率 0 错误 |
| 239 | Node 18.20.8：三 profile 加载/Worker 生命周期及 full 示例 | 50 过、1 跳；12 组示例 / 14 断言通过 |
| 240、241 | Python 未修改参考测试；慢测试单独运行 | 默认 1,755 过 / 8 败 / 10 跳，原始 exit 1；慢测试 10 过，exit 0 |
| 242、243 | JS 未修改参考测试；含慢测试重跑 | 默认 1,178 过 / 11 败 / 3 跳；慢轮 1,181 过 / 11 败 / 0 跳；各 2 套件加载错误，原始 exit 1 |
| 244 | Chromium / Firefox / WebKit | 全部通过；各 66 项检查 + 14 项示例断言；页面、控制台、服务错误均为 0 |
| 245 | Node 与 headless Chromium A/B 基准 | 通过；每宿主五轮，pure/compat 各 30 用例、native 13 用例（均含 noop），正确性摘要通过 |
| 247 | 八组合 release 体积与首次异步初始化矩阵 | 通过；运行时功能检查与 writer 调用通过；初始化次序/负载混杂，后续重测 |
| 248 | 构建完成后轮换组合重测初始化 | 通过；两个宿主、八组合、各五个新进程，逐样本记录负载；不重编译 |

full 的两项跳过是仅适用于禁用 feature 的检查；精简 profile 的跳过明确对应未编入功能。
完整原生门禁的通过数含 feature 配置下的重复执行，不代表 4,136 个不同测试。
236–239 的前后源码快照完全一致；244/245/247/248 的差异仅为各自生成的证据 JSON。
233 的快照差异仅是当时新增的两个 JS 示例，所测 Rust/Cargo 源码未变。

失败记录保留：225 暴露 J1/J3 合并后的诊断不一致，修正后 226 通过；235 的 full Node 测试已全过，
但参考文档含 codegen 不稳定的内部闭包名字，修正生成器后 236 全通过。246 的同步初始化测量触发
Chromium 主线程 8 MB 限制，改用异步实例化后 247 通过；248 在构建结束后按轮旋转/反转组合次序，
减少 profile 与测量时间段/系统负载的混杂。所有轮次保留，没有降低门禁或把失败退出码改成成功。

## 要求、接口与单元

| 工作项 | 交付 |
| --- | --- |
| 完整原生 API | 模型/元数据/资源、Mono/Gray8 bitmap、渲染、charset、轮廓、99 格式 catalogue/typed options、sfnt/WOFF/WOFF2、variants/locl/subset、merge 与全部 22 个 ops 操作；覆盖清单逐项核对 |
| A / F 地基 | wasm32 单线程 spawner、progress/Cancelled、wasm-bindgen 0.2.129、JSON/BigInt bridge、句柄与错误、capabilities、构建及防漏门禁 |
| J1 / J2 | 模型与共享视图、元数据、资源集合/解析器；bitmap、渲染、字符集、轮廓 API |
| J3 | 99 格式命名空间和注册表；合并后只保留一份 Written/OutputBundle；merge profile 的 bdat 可用性按 catalogue 判断 |
| J4 | sfnt 表/容器、WOFF、variants 与库层 `mergeLib`；与 W8.3 顶层 `merge()` 避免重名 |
| J5 / W8.3 | `info`、`merge(parts, options)`、`render`，返回内存文件/完整报告；typed ops、Worker、进度、取消、Node 文件辅助；在进程内调用库，绑定不调用 CLI 子进程 |
| J6 | 独立 `fontpixel/compat/bdfparser`，mutable views、兼容 CLI/helper 子路径及未修改参考测试适配器 |
| JS/TS 文档与示例 | [API 指南](../docs/js-api.md)、[生成式逐项参考](../docs/js-api-reference.md)、[12 组共享示例](../examples/js/recipes.mjs) 和 Node runner；TS 6.0.3 `tsc --strict` 通过 |
| 性能与体积 | [PHASE8-REPORT.md](../benches/PHASE8-REPORT.md)，Node/Chromium 逐类对比，full/merge/minimal 和五个实验组合 |

补充审查修复了 Worker 的四类生命周期问题：fatal trap 拒绝所有排队请求；structured clone 失败清理 pending；
启动时 terminate 关闭待就绪端口；progress 抛出 `undefined` 仍按异常传回。协议 worker 覆盖故障后重启与重用，
不在 Rust 生产代码插入 panic/unsafe。`initSync()` 在 `await init()` 后幂等，兼容 Node 18 和浏览器。

`TaskRequest` 的 internally tagged newtype 声明改成 `{kind, ...payload}`；修复在共享 serde generator，
Python 和 TypeScript 都采用真实运行时形状。`BatchTaskSpec = {id} & TaskRequest` 保留未知键类型检查。

## 原生覆盖率

`tools/bindings/gate.sh python,js` 在 gate 236 内运行，Python 门禁也再次验证：

```json
{"items":2008,"fields_and_variants":2679,"mapped":2008,"unmapped":0,
 "by_status":{"bound":915,"adapted":1085,"unavailable":2,"internal":6},"errors":0}
```

两种语言均为 0 missing entries / 0 missing tests。原始 inventory 共 4,687 个条目，包含模块、
类型、方法、函数及字段/成员；2,008 是覆盖门禁的公共项口径，不能与总 inventory 混用。
`adapted` 保留能力、按语言改变表示；`internal` 每条有用途说明。
两个 unavailable 仍是 `Sfnt::font_ref`、`Cff::fontations`，返回第三方借用类型，替代是表字节和解析后的表。
不是把未实现接口归为平台不支持。

## portable-float、输出与堆上界

`crates/fontpixel/Cargo.toml` 的 brotli 9.0.0 启用 `std, portable-float`，Cargo.lock 不变。
原始 crate 归档 SHA 与锁文件匹配，缓存源码与归档核对一致，没有依赖本地篡改的 brotli。
新原生冻结 CLI SHA-256：`d660d98beb978b9f32f70b99ea71e5b0d178123221079fbbb1ebf669784dcf03`。

全部原金样重新生成后比较，只有四个 WOFF2 期望需要更新：`woff2.encode`、`woff2.encode_many`、
`cli.variants.woff2`、`cli.subset-AB.woff2`。临时输出路径差异没有冒充语义差异。
阶段 6 原数据保留为历史，另附改动后值；原生/WASM 的大输入 WOFF2 TODO 均已变成正式断言。

| 新产物 | 字节 | SHA-256 |
| --- | ---: | --- |
| Tong SC sfnt（CFF，WOFF2 输入） | 5,621,172 | `61fe4b721b032ef425e9980f7c592d5e2685ad00b4937506750da0266ad8af42` |
| Tong SC WOFF2 | 892,576 | `a25e58b03bd6e0b798d9b0db506860eda9616e8218e6b7c1f3a75bea5fee8c46` |
| Unifont + Tong Latin，57,106 字形 BDF | 9,258,883 | `fafcc4e7edd451abe9b84ee8ddf4e2a6c51012b660e2c87b75c13e98bfec2c90` |
| Tong SC + Mono Latin，38,643 字形 BDF | 5,729,331 | `fc312ab66231939b534493a91a0cde71c0b763d8c69797edb1117019f2820911` |
| 同一 Tong 拼合 WOFF2 | 956,372 | `c4ea301583e30e21c8972bda839ad418192c0dcc1f07b204cfb0b54f17e81df0` |

gate 233 对 counting allocator 复测：432 个 parallel + 252 个单线程 spawner 用例，质量 0–11，
含结构化/随机/字体型输入与上述大 SC sfnt，所有 admission 断言通过。
实测分配总峰值 / 接纳总额最大为 parallel **0.969987**（lgwin24）、默认 lgwin22 **0.899023**、
sequential **0.443660**。[汇总](../benches/results/phase8-portable-heap.json) 与完整日志保留。
质量 10–11 的界限依然是经验包络，不是数学证明；原资源上限和接纳公式没有放宽。
旧文中 1.54× 裕量指数据相关包络，不是总堆分配裕量。

## 浏览器、线程与资源语义

[正式浏览器 JSON](../benches/results/phase8-browser.json)：Chromium 153.0.8010.12、Firefox 155.0、
WebKit 26.6。使用 Playwright 的真实模块 Worker；WebKit 的本机兼容动态库在 local-work，未修改系统或兄弟仓库。
**WebKit 测试不等于 Safari 产品或历史版本认证。**

每个浏览器：隔离上下文 39 项 + 非隔离 27 项；前者验证 cooperative cancel，后者验证 terminate fallback。
同时验证进度中取消、预取消、重复使用、terminate、strict/loss/error report、资源 cap、Number/BigInt、
句柄失效/共享视图/复制、异步 sidecar rerun、compat 加载，以及与 CLI 的 info/render 相等。
大字体 merge 的 BDF/WOFF2 **字节和 JSON 报告**均与 native 相同，主线程 heartbeat 继续运行。

Rust 计算本身同步，wasm 内部不创建原生线程。Promise 不会自动把同步计算移出主线程；大任务使用 Worker，
每个 Worker 有独立 instance/memory，compiled module 可共享，句柄不能跨 Worker。进度是操作检查点，
长编码步骤可能没有细粒度进度；共享原子取消可在检查点协作，没有 SharedArrayBuffer 时 terminate。
取消不能承诺压缩循环中立即响应。

原生句柄显式 `free()`/`Symbol.dispose`，GC 仅后备；字节输入输出复制，视图生命周期由共享存储保护。
非安全整数入参拒绝，纯数据的大整数按 bigint 保精度。浏览器具名资源替代任意路径；异步解析通过明确的
缺资源重跑协议，可能多次调用 run。输出 bundle 是内存结果；Node 文件辅助持久化由宿主负责，不能冒充
原生文件系统的多文件事务。这些约定和示例已写入 JS 指南。

## 兼容参考结果

Python 默认 1,755 / 8 / 10，慢测试另跑 10/10；JS 默认 1,178 / 11 / 3，慢轮 1,181 / 11 / 0。
每个测试的 outcome 均与交接基线相同，刷新了四份结果 JSON。
Python 的 8 项为 3 个非法像素标签与 5 个参考私有实现检查；JS 的 11 项涉及标签、矩形 raster、
完整 CJK 表、Rust gzip、bool 属性语义及 3 个 PCF 诊断文本断言。
详见 [Python 差异说明](../crates/fontpixel-py/tests/compat_bdfparser/KNOWN_DIFFERENCES.md) 和
[JS 差异说明](../crates/fontpixel-wasm/tests/compat_bdfparser/KNOWN_DIFFERENCES.md)。

JS 的 `build.test.ts` / `browser.test.ts` 重建和检查 bdfparser-js 自己的 tsup/IIFE 产物，适配环境不加载；
它们不替代本阶段的真实 WASM 三浏览器验收。Shim 中 gunzip/flattenContour 的少数检查验证的是参考 helper
移植，不能计作 Rust 绑定行为覆盖；原生完整性由独立 inventory/coverage 门禁负责。

## 测速、体积与已知限制

详细表和全部原始数据见 [benches/PHASE8-REPORT.md](../benches/PHASE8-REPORT.md)。兼容 API 的 SC 解析
配对比值 Node 1.62× / Chromium 2.40×，PCF 3.12× / 4.83×，混合文字 65.84× / 71.54×。
BDF 写出、小调用和迭代有较慢项目，尤其 compat `Font.length` 的线性扫描；报告没有省略这些项目。
共享桌面负载明显，范围是实测波动，不是置信区间，近 noop 下限的小调用不报告精确加速比。

正式 full/merge/minimal `.wasm` 优化后分别 **16,666,863 / 8,666,341 / 3,925,657 B**，
gzip **6,044,581 / 3,393,814 / 1,352,234 B**。八组合矩阵分别检查 CJK、WOFF2 与 vector writers/raster；
minimal 仍有基础轮廓 API，不声称剔除一切轮廓代码。初始化测量预加载 WASM/字体字节，浏览器 JS import 包含 loopback HTTP 模块获取，
Chromium 大模块主线程必须使用异步实例化；Worker 内同步实例化已经验证。最终初始化样本的 load1
为 9.30–11.95，时间波动仍大；数据只描述此次测量，不能据此给 profile 作稳定速度排名。

未覆盖所有历史浏览器或手机。full 模块较大，精简 profile 需先查询 capability；未编入功能明确失败。
portable-float 的跨目标字节一致性已有上述输入证据，不泛化为所有未来 Brotli 版本的保证。
阶段 7/8 之后的 104 目标长 ASAN、218 种逐 feature Clippy 与项目最终报告按原 TODO 留给最终收尾，
本次不冒充完成。fontpixel.com 页面和第 9 阶段尚未开始。

## 本地交付与清理

八种包、构建元数据和 [SHA 清单](../benches/results/phase8-packages.json) 保存在 `../local-work/p8-redo/packages/`，正式验收日志与新冻结 CLI 保留。
报告、原始 JSON、脚本、JS 文档与 TODO 已提交；主构建 target 和本轮可再生缓存清理后，工作区干净。
保留的外部工具环境用于复现，不向 npm/PyPI 或远程 Git 发布。
