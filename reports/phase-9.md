# 第 9 阶段交付报告：网站浏览器字体工具

日期：2026-10-07。范围是用户改定的 fontpixel.com 工具页，不创建 `web/merge/`。
只做本地提交，没有 push、publish 或 deploy；没有编写项目最终报告。

## 结论与交付

网站分支 `tool-page` 从 `add-tong@a7dd031` 创建；最终提交 **`88bb8a2`**，工作区干净。实现提交 `dd191b0`；`f4c4c80` 修复 Astro 生产预览的旧下载路由接入。
`edb2e0b` 限制网页诊断文本规模，同时完整保留报告下载及 strict 语义；
`4e2347f` 修正 PCF 预览验收，直接检查结果覆盖和编码损失。
Rust `main` 的 `8bdd6de` 增加 web profile、显式 catalogue BDF 写法及绑定；
`d7d30f8` 按实测结果保留 CJK 编码表，保证 sfnt/WOFF2 与原生字节一致。

浏览器工具提供选字、合并、格式转换、轮廓像素化，结果预览之后才能下载。
网站只预生成 BDF/WOFF2；旧 PCF/TTF/ZIP 下载地址转到工具，保留对应字体信息。
最终发布目录 **13,072 文件**，相对原来的 19,925 净减少 **6,853**，
20,000 文件上限下余量由 **75 增至 6,928**。尚未部署。

## 验收记录

所有要求的门禁已通过。浏览器生产包测试没有跳过，失败轮次保留并注明修复。

- gate 249：`scripts/check.sh`，758 个套件、4,150 次通过、0 失败、8 ignored。
  通过数包含不同 feature 下的重复执行，不代表不同测试的数量。
- gate 250：首次四 profile 完整 WASM 检查；full 1,619/2 跳，minimal 398/381 跳，
  merge 566/351 跳，以上均零失败。web 有 4 个真实失败：测试 profile 表遗漏，以及
  缺 CJK 表导致 CP936/OS/2 code-page、sfnt/WOFF2 字节不同。未修改金样或放宽断言。
- gate 254：修复后 `scripts/check_wasm.sh full web` 全过；full 1,619/2 跳，
  web 660/306 跳。四 profile wasm32 Clippy、内部 Rust 7 项、声明生成、TS strict、
  示例、API 参考、`python,js` 覆盖门禁均过。minimal/merge 的 feature 图未改变，
  使用 gate 250 已通过的结果，没有无理由重跑。
- 覆盖门禁：2,009 项、全部映射、0 未映射、0 错误；Python/JS 都无遗漏入口/测试。
- gate 255：`scripts/check_python.sh`，1,265 项通过，无跳过。
- gate 253：网站流水线 1,613 项通过、26 跳过；类型检查 0 错误，Vitest 179 项通过。
- gate 256：最终生产源码再次通过类型/单元、生产构建、SEO 和文件上限检查。
  4,168 页面、4,167 sitemap URL、656 social images，SEO 0 错误。
- gate 257：网站原有 292 项 e2e 首轮 290 过、2 失败；旧导航断言已更新，
  Astro 5 静态 preview 不装载用户 Vite plugin，改为共享 HTTP handler 接入。
  原来的 404 失败保留，修复后原断言单测通过；258 重跑完整 e2e。
- gate 258：网站完整 e2e **292 项通过**，无跳过，包含原有 axe 与响应式测试。
- gate 259：工具页三浏览器首轮 38 过、1 败；Chromium 合成五万字测试的主线程最大间隔
  2,729.5 ms，超过原阈值 2,000 ms。完整诊断约 9.4 MB 被放进 DOM，文本检查触发停顿；
  生产修复改为 Worker 生成完整报告文件和有界网页摘录，没有放宽时间断言或 strict。
- gate 260：最终 UI 改动后的类型/179 项单元、生产构建、SEO 和 Pages 文件上限检查全部通过。
- gate 261：33 过、6 败，均为 mono/prop 案例中错误的 PCF 预览断言：
  PCF 的 16 位编码不能保留 U+20000 映射，旧断言误匹配报告里的输入 block 名。
  改为只检查实际覆盖区，并强制要求 `pcf.write.code_range` 的 loss 诊断；
  没有改变 PCF 输出或字节比较。
- gate 262：三浏览器 **39 项全部通过**，0 跳过、0 flaky。BDF/WOFF2/PCF/OTF/HEX
  的 mono/prop 输出与当前原生 CLI 逐字节比较均通过。
- gate 263：五个新 Node 进程与五个新 Chromium 进程的初始化、三字体 × 两格式 ×
  五轮生成共 30 次测量完成，无错误、无结果缓存命中；逐次负载与数据保留。

首轮 site gate 252 的三个失败是旧 ZIP 发布合同/旧 fixture；按新的 BDF/WOFF2 合同
更新测试并重新生成 fixture 后，253 全过。失败原始日志保留。
249 前后源码变化只有 `docs/js-api-reference.md` 和 `docs/decisions.md`；被测 Rust 不变。
254/255/256 Rust 快照不变；253/256 的网站独立快照不变。
本轮不重跑阶段 8 的参考兼容套件，也不把其已有差异改称通过。

## 五项用户要求

| 要求 | 实现与证据 |
| --- | --- |
| 站内字体或本地上传 | 655 家族的 catalogue 和 size/style 选择；本地字体留在浏览器；BMFont 可同时选择 PNG 页。冷门格式明确提示先用 CLI 转换。 |
| 常见格式转换 | BDF、PCF、OTB、位图 TTF、矢量 TTF/OTF、WOFF/WOFF2、PSF、FNT/FON、HEX、YAFF、BMFont、C，共 15 个输出选项；输出重新读取后预览。 |
| 按 block/区段选字及多字体合并 | 每个来源分别选择多个 block、inclusive 十六进制区段或文字；合并使用共享 ops，保留优先级、fallback、基线偏移、mono/prop 选项。 |
| 矢量像素化 | 原生 vector-raster 路径，指定 ppem 或自动识别像素网格；显式阈值 128/255；页面说明不执行 hinting，量化损失仍报告。 |
| 只预生成 BDF/WOFF2 | pipeline 默认 `prebuilt_only=True`，缓存元数据过滤与 staging 清理旧产物；其余格式从字体页进入预填工具并由 Worker 生成。 |

## 十四项补充要求

| 项 | 交付与检查 |
| --- | --- |
| 1 RFN/修改权限 | 子集、合并、像素化、显式字号/spacing 改动必须新名称和确认权限；禁止修改的来源拒绝操作。检查 catalogue 保留名及原输入内嵌声明，Unicode 规范化后拒绝复用。 |
| 2 按文字选字 | 与 block/range 取并集，支持 Unicode 补充平面；测试含 CJK Extension B。 |
| 3 下载前预览 | 每个已覆盖 block 和 Unicode Script 均有统计及最多 12 个分散代表字；实际字体栅格 SVG，包括 Mono/Gray8。网页诊断显示完整计数及最多 200 条/32,000 字符摘录；完整缺字和报告下载不截断。改选项后旧下载失效。 |
| 4 常见格式 | 三浏览器遍历全部 15 个选项，含 BMFont PNG sidecar 再打开；TTF/OTF 支持方点/圆点。 |
| 5 按需、Worker、取消、缓存 | 字体页不加载 WASM；生成时加载 web runtime。同步编解码留在 Worker，取消 terminate；IndexedDB 上限 64 MiB，hash/recipe/来源/选项共同决定缓存键。 |
| 6 分享/预填 | 版本化、校验过的 URL state；不含本地字节和私人许可证文本；字体页预填 font/variant/format。 |
| 7 旧链接/无 JS/文档 | 三条 suffix redirect + 6,873 条旧文件映射，dev/preview 对应行为；旧多字号 TTF 提示选现行字号。无 JS 仍可下载 BDF/WOFF2；README、站点文档及 llms.txt 更新。 |
| 8 流水线瘦身 | 下载条目 11,631 → 4,758；manifest 与 staging 都仅保留 BDF/WOFF2。离线回归调用可显式请求旧生成方式。 |
| 9 许可证随文件 | 原始 Unicode 名称/版权/许可先保存；能承载的输出恢复这些元数据，不能承载的报告损失；ZIP 含 LICENSE.txt 和 REPORT.json。canonical GPL fallback 也实测。 |
| 10 手机资源 | 原生默认 caps 不变；UI 额外限制 16 个来源、每个 64 MiB、合计 256 MiB（含 sidecar 与 gzip 解压检查）；资源错误提示缩小输入/ppem 或用 CLI。 |
| 11 三语言 | en/zh/zh-Hant；两种中文均由用户指定的 gpt-5.6-sol 子任务翻译，主任务核对键/占位符。其他站点语言链接英文工具。 |
| 12 验收 | 三浏览器 CLI 逐字节比较、真实 50,763 字形和合成 50,000 字形响应性，见下表。 |
| 13 技术栈 | Astro 5、Svelte 5 runes、bun；大结果用 `$state.raw`，SVG 路径在 Worker 生成，屏外样字延迟绘制。 |
| 14 本地提交 | 只在网站 tool-page 和 Rust main 提交；不改其他项目，不安装 npm 依赖，不发布。 |

## BDF、命名及字符编码一致性

显式 `bdf_catalogue_family` 对齐流水线 `bdfwrite.py` 的 FONT、SIZE、COMMENT、SWIDTH、
属性顺序、Latin-1、位图大小写及未改动字形的尾部 padding。通用 BDF writer 默认行为不变。
`Registry.setBdfWriteOptions` 同时覆盖 Python/JS，CLI/ops 共享同一实现。
真实 BDF oracle 包括 Spleen、Cozette、Galmuri，以及 3x5-block、Gossip、Terminus 三个
export_name 案例；还在浏览器核对 3x5-block、Poxiao 与流水线解压 BDF 完全相同。

凡有 export_name 的 catalogue 转换都应用它；FONT/FULL_NAME/FACE_NAME 处理对齐
`downloads.py::_renamed_props()`。上传字体的 Unicode 家族名、版权和许可在 Latin-1
中间格式之前捕获，恢复到能承载的输出和 ZIP；Latin-1 损失没有隐藏。

上传 Legacy 编码先通过共享 `ops.charset` 映射至 Unicode，YAFF 中间格式保留 Gray8，
再写 catalogue BDF；实测 CP1252 的 0x80 正确成为 U+20AC，且与 CLI 同配方字节一致。
输出 FNT/FON 的预览也按明确的 charset 解码，不能假设字节值就是 Unicode。

## 三浏览器与响应性

正式环境：Chromium **151.0.7922.34**、Firefox **155.0**、WebKit **26.5**，
Playwright **1.62.1**。每个浏览器 13 项测试；每组 mono/prop 都比较 BDF、WOFF2、PCF、OTF、HEX，
并验证实际覆盖区的 CJK/Greek、缺字、SVG ink 和 axe。PCF 丢失 U+20000 编码必须报告；
其余四种格式的覆盖区必须保留扩展 B。额外检查 Legacy FNT → Unicode BDF 的原生字节一致性。

| 浏览器 | 合成 50,000 字形 Worker ms | 最大主线程采样间隔 ms | 真实 50,763 字形 Worker ms | 最大主线程采样间隔 ms |
| --- | ---: | ---: | ---: | ---: |
| Chromium | 1,243.9 | 86.2 | 1,304.6 | 67 |
| Firefox | 1,956 | 91 | 2,451 | 58 |
| WebKit | 1,168 | 253 | 1,252 | 216 |

定时器间隔 25 ms，最大间隔须小于原有 2,000 ms 阈值；没有因失败提高阈值。
该数值是页面采样间隔，不是操作系统输入延迟或手机帧率保证。
合成字体覆盖 149 个 block、选取 1,788 个 block 样字，另有 Script 样字；
统计完整，屏外 SVG 延迟绘制。进度、取消、再次生成和缓存命中均通过。
完整诊断下载的条目数量与未截断的计数相等。

另外人工查看 1280 px 浅色桌面和 390 px 深色手机布局；Poxiao CJK 代表字有实际像素笔画，
手机 document scrollWidth = clientWidth = 390。补充 axe 扫描 0 violations、2 incomplete
（保留待人工判断项，不表述为完整无障碍认证）。两种中文共 110 键，键和占位符检查均通过。

## 体积、初始化与生成耗时

单位为字节，gzip 使用 level 9；正式包来自干净的 `d7d30f8`。

| profile | wasm-opt 前 | 前 gzip | wasm-opt 后 | 后 gzip |
| --- | ---: | ---: | ---: | ---: |
| full | 20,602,608 | 6,416,667 | 16,689,463 | 6,054,064 |
| web（交付） | 11,548,810 | 3,929,192 | 9,683,922 | 3,764,582 |
| web 无 CJK 实验（弃用） | 9,977,030 | 3,139,577 | 8,113,644 | 2,974,888 |

web 相对 full 的优化后体积减少约 42.0%，gzip 减少约 37.8%。CJK 表不能省去：
OpenType 自动 code-page metadata 需要它，首轮无表构建确实未通过大字体字节一致性。
交付 WASM SHA-256 为 `f199ec890b8e5834aad964288c29d5dd288e7b526d111ba44d5680a01c0c2cb5`。

网站 JS loader 为 **113,889 B / gzip 21,608 B**。测量机是 Intel Core i7-12700H，
Node **24.21.0**。在构建完成、关闭额外 QA 浏览器之后采集；机器仍与其他任务共享，
生成采样时 1 分钟 load average 为 3.12–6.19，不能视为完全隔离的跑分。

初始化各五个全新 Node/Chromium 进程，WASM 字节预先读取，异步 compile/instantiate，
OS 文件缓存为热；浏览器 JS import 包含 localhost 获取，**不含公网 WASM 下载时间**。
各列分别取五次中位数，因此分列中位数不一定相加等于总中位数。

| 环境 | JS import ms | WASM compile ms | instantiate ms | 总计中位数 ms | 总计范围 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Node | 16.42 | 22.15 | 31.56 | 68.72 | 51.50–78.55 |
| Chromium | 16.00 | 12.10 | 2.60 | 30.70 | 29.50–37.00 |

生成使用生产 UI、真实 Worker，每次新 browser context、清空结果缓存；每格五次。
Worker 时间包含读取/选字/转换/结果预览准备，不包含引擎初始化；wall 时间从点击生成到结果回传，
包含初始化、完整报告/ZIP、缓存写入等，不等于全部延迟样字绘制结束。

| 字体 / 已映射字形 | 输出 | 输出 B | Worker 中位数 ms | Worker 范围 ms | wall 中位数 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| Spleen / 472 | BDF | 59,796 | 65.2 | 59.5–89.2 | 261.5 |
| Spleen / 472 | WOFF2 | 5,720 | 258.3 | 222.7–311.6 | 467.8 |
| Cozette / 6,046 | BDF | 683,147 | 227.2 | 219.9–232.2 | 409.9 |
| Cozette / 6,046 | WOFF2 | 92,484 | 1,704.8 | 1,653.1–1,761.6 | 1,859.7 |
| Poxiao / 50,763 | BDF | 6,561,075 | 1,677.0 | 1,400.6–3,409.1 | 2,039.8 |
| Poxiao / 50,763 | WOFF2 | 905,116 | 19,205.8 | 19,034.3–35,747.8 | 19,426.3 |

大字体 WOFF2 并不快，五次中一次达到 35.75 s；不删除该次，也不降低压缩质量或资源上限。
预生成 WOFF2 仍可直接下载，其他生成操作显示进度并可取消。

## 证据与复现

- [验收索引与测量数据](phase9-validation/summary.json)：完整门禁命令/退出码、日志 SHA、
  源码差异、各浏览器逐测试结果及输出 SHA、每一次测量和负载。
- [本机测量脚本](phase9-validation/measure.mjs)：与 gate 263 执行内容完全相同，路径按交接环境固定；
  使用 `bun run preview --port 4399` 的生产站点和已构建 web 包，不依赖公网服务。
- [桌面浅色](phase9-validation/desktop.png)、[手机深色 CJK](phase9-validation/mobile-cjk-dark.png)。
- 完整原始证据：`../local-work/phase6-local-20261004/gates/249-*` 至 `263-*`，以及
  `../local-work/p9/evidence/`。失败 JSON 和日志保留，索引中的长错误只摘录开头并记录原 SHA。
- 网站源码独立快照：253、256、258、260、262 前后均一致；被测应用最后改动为 `edb2e0b`，
  其后只有测试及文档。Rust 254/255/263 快照均一致，正式包来自 `d7d30f8`。
- 复现常规门禁见网站 `docs/font-tool.md`；环境中 Firefox 使用已安装的 `firefox-1543`，
  WebKit 使用 `local-work/p8-tools/webkit-local.sh` 宿主库包装器，没有改动系统或参考仓库。

## 范围与限制

- WebKit 指 Playwright WebKit，不能表述为 Safari 产品或历史浏览器认证。
- 代表字预览逐字展示栅格，不执行复杂文字排版；覆盖率是码位统计，不等于语言完整支持。
- web 排除冷门 codec、bdfparser 兼容层、TOML 和 sfnt collections；不回退加载 full。
- 输出格式自身的字符/元数据限制仍有损失报告；strict 拒绝损失。一般轮廓小字号不执行 hinting。
- 旧 grouped TTF 重定向到现行单个 size/style 的工具，不承诺重新生成历史多 strike 文件的旧字节。
- 五万字响应性在本机桌面浏览器测试；没有实测实体手机峰值内存或公网冷下载。
- 保留原生 release、最终 WASM、失败和成功日志；已清理 9.4 GiB debug 构建。
- 阶段 9 完成后停止；TODO 中最后的 ASAN/218 feature 组合与 FINAL-REPORT.md 仍未启动。
