# 第 6 阶段交付报告：CLI 完善、批量、文档、测速、健壮性、拼合（严格标准）

日期：2026-10-06。最终源码 main `8cd6d66`，冻结二进制 v5。第 7、8、9 阶段尚未开始；只有本地提交，
没有 push、publish 或 deploy。逐 gate 的命令和结果见 [本地接续记录](phase6-local.md)，本地证据目录
`../local-work/phase6-local-20261004/`（下称 WORK），每个 gate 在 `WORK/gates/NNN-*/` 保存命令、源码
前后快照、完整输出和退出码。

## 结论

第 6 阶段按**严格标准**完成。PLAN 原文要求的 W6.1–W6.6 全部验收，严格标准的两项也已做到：

1. **全账本**：每个编解码器自有的分配都在分配之前计入调用方的同一预算，释放后才退还；默认上限都没有提高。
2. **追平**：默认行为和输出不变；用显式选项后，收录站和通格 OTB 与原程序的每一处差异，要么消除，
   要么归入三类之一：原程序有损（不应照抄）、派生值、消费端限制。

最终源码通过的检查：

- 完整 `scripts/check.sh`：731 个套件 3,759 项测试，两个 Clippy profile `-D warnings`，最小 CLI
  feature Clippy，`api_quickstart` 运行，deny；
- 无警告 rustdoc 和 doctest；
- `api_quickstart` 的 9 个 feature profile 都能运行并通过 Clippy；
- fontpixel 的 108 个 feature 加上不开 feature，共 109 种组合，各自单独跑 Clippy `-D warnings`（gate 075）；
- 用冻结二进制做真实字体对照，没有原生缺陷；
- 104 个 ASAN 目标，每个约 611 s，共 298,741,042 次执行，0 崩溃（gate 077）；
- 默认负载巡检 109 个 case，没有一个因为多算而被拒；
- PLAN §10 性能目标在安静窗口和 A/B 对比中都达标（见下文）。

## 范围决定的经过（2026-10-05）

1. 用户恢复第 6 阶段后，先选择按 PLAN 原文验收，当天上午交付了 `6297998`。
2. 之后用户要求「按那个更严的标准来」「一干到底」，第 9 阶段也包括在内。中途用户要求暂停一次，
   恢复后又要求「第 6 阶段完成了再停止」。本报告对应的就是这个严格标准下的第 6 阶段。

## W6.1–W6.6 验收

| 工作项 | 证据 |
| --- | --- |
| W6.1 `batch`/`diff`/`validate`/`charset`，帮助与示例 | 12 个子命令都有示例；CLI 测试全部通过；W62 文档回放 v5 跑了 40 条命令，全部符合预期（18 个 CLI 冒烟、84 个 tiny 产物、各子命令帮助），gate 160 |
| W6.2 文档 | 99 个格式页面齐全；README/docs 的 CLI 示例和 `--help` 一致；rustdoc 无警告；迁移说明写了显式追平选项和剩余差异的分类 |
| W6.3 测速报告 | [benches/REPORT.md](../benches/REPORT.md)：v3、v5 两次正式五轮测量，加 A/B 对比 |
| W6.4 健壮性 | 见 [fuzz-final.md](phase6-validation/fuzz-final.md) 和 [default-sweep.md](phase6-validation/default-sweep.md)：<ul><li>ASAN 104/104 通过，逐目标的执行次数和覆盖率都有记录；</li><li>每类资源上限都有测试，另有 proptest；</li><li>全账本，见 [writer budget](../docs/w6.1-writer-budget.md#codec-working-ledgers-strict-phase-6-2026-10-05)</li></ul> |
| W6.5 拼合（库） | Unifont 57,086 + Tong Latin → 57,106 字形，字形证明和来源审计 0 错误 |
| W6.6 拼合（CLI） | 通格真实地区拼合 4 个 profile × 20 个 case = 80/80 通过；57,106 字形拼合写 BDF 用时 0.36 s |

## 严格标准：单元与结果

| 单元 | 内容 | 结果 |
| --- | --- | --- |
| A | WOFF1/WOFF2 全生命周期账本；Brotli 编码器堆在调用前按上界预留 | 质量 0–9 用由源码证明的上界；质量 10–11 用经验包络（816 个案例的计数分配器矩阵，余量 ≥1.54×），文档标明「经验、非证明」 |
| B | subset 内部所有表共用一个账本 | 255 个 subset 案例、30 个 CFF 输出逐字节不变 |
| C | variants 规划、导出和轮廓接入账本；规划后释放源模型并精确退还 | 默认 Tong variants 在 256 MiB 内完成，峰值 RSS 从 552 MB 降到 250 MB |
| D | OTB/bdat 完整元数据显式选项（`--otb-*`） | Tong OTB parity profile：20/20 只剩 `head.checkSumAdjustment` 不同（表的字节相同，只是在文件里的顺序不同，属于派生值）；633,028 个字形像素全部一致 |
| E、F | 收录站投影扩展、PCF 属性优先级、矢量和位图 TTF 的元数据选项、`.notdef` 策略 | explicit profile：173 个字体中 99 个通过，73 个未解决但全部已分类，1 个是消费端限制，0 个未分类 |
| G | 所有权写出路线：`convert` 把模型交给轮廓写出器 | 完整 CJK 字体的 `convert` 到 WOFF2/OTF/WOFF/TTC 可在默认上限内完成，输出不变 |
| H | 去掉 10 个写出器对模型的重复计费；CFF 流式编码 | SC→OTF 的最小预算从 214.7 MiB 降到 85.7 MiB；Unifont 拼合→OTF 可在默认上限内完成 |
| I | 显式预算能抬高轮廓写出器的上限；merge/charset/`.gz` 走保留路线；pk/gf/zapfont 精确计费 | pk/gf 的「先分配后计费」已修正 |
| J | pf2 精确账本；TeX 失败条目在计费后才分配 | SC→pf2 最小预算从 319 MiB 降到 131.9 MiB |
| K | color（CBDT/sbix）走同一个精确账本 | SC→color 最小预算从 548.9 MiB 降到 219.9 MiB |

收尾时还修了这些问题：

- yaff 写出器没有给首尾的 Unicode 空白加引号（proptest 发现），导致 U+00A0 往返丢失且没有报告；
- `api_quickstart` 断言写错；
- 精简 feature 组合下的死代码；
- 一个有歧义的 rustdoc 链接。

## 真实字体对照（v4 全量重跑，v5 以输出一致沿用）

v4（gate 122–146）全量重跑了下列配对验证。v5（gate 150–161）重新生成全部产物，字体和输出文件与 v4
**逐字节相同**，所以 v4 的配对结论直接沿用；legacy parity 和 W62 回放在 v5 上重新跑过。

| 对照 | 结果 |
| --- | --- |
| Tong batch CLI 与 API（各 72 条记录） | <ul><li>20 PCF、20 OTB FreeType 逐像素一致，PLAN §9.4 对 OTB 只要求这一项；</li><li>16 个矢量字体：cmap、hhea、OS/2 关键字段、name、轮廓栅格逐像素、14,592 批 locl 塑形，全部一致；</li><li>16 个网页字体通过</li></ul> |
| Tong OTB parity profile | 见单元 D |
| 通格地区拼合 | 80/80 |
| Unifont 拼合 | 通过 |
| SC 编解码 | 11/11，Rust、Python、Node 七个产物逐字节一致 |
| SC 矢量、同 sfnt 网页字体 | 38,654 字形几何加 912 批塑形；方点和圆点都通过 |
| 收录站默认 profile | 8 个通过，164 个未解决（已文档化的类别），1 个是消费端限制 |
| 收录站 explicit profile | 99 个通过；73 个未解决全部已分类：<ul><li>41 个 WOFF2 只差派生的校验和；</li><li>28 个 BDF 只差源 COMMENT（fontpixel 保留，原程序丢弃）；</li><li>4 个位图 TTF：原程序只保留已编码的字形；</li></ul>1 个是 Spleen64 的 FreeType error 98 |
| bdfparser 差分 | 67 个表一致；2,000 random + 1,000 random_all + 41 probes + 251 个语料：0 个新差异，7 个是已登记的 K12 |

## 性能（PLAN §10）

| 任务 | 目标 | v3 安静窗口 | v5 正式（有外部负载） | A/B v3→v5 |
| --- | --- | --- | --- | --- |
| 解析 SC | ≤100 ms | 53.3 ms | 57.2 ms | 59.5→59.9 ms |
| → PCF / OTB / yaff | 各 ≤200 ms | 88 / 120 / 145 ms | 95 / 130 / 150 ms | OTB +9 %，其余 ≤3 % |
| → 方点 TTF | ≤1 s | 449 ms | 500 ms | 491→498 ms |
| Tong 整套默认导出 | ≤40 s | 32.2 s | 43.7 s | 30.1→30.6 s |
| 57,106 字形拼合写 BDF | <1 s | 0.36 s | 0.36 s | 0.40→0.44 s |

v5 正式测量期间，桌面和其他 agent 仍在运行，负载中位数 5.2，峰值 17。从未改动的原程序在同一窗口也慢了
约 1.33 倍：export-rs 88.4 s 对 66.3 s，Python 收录站 48.0 s 对 43.5 s。在同样负载下交替运行 v3 和 v5 的
A/B 对比中，Tong 整套导出为 30.6 s 对 30.1 s。由此判断 v5 的 43.7 s 来自外部负载，而不是代码退化。
严格账本只让 BDF→OTB 慢了约 9%（仍远低于 200 ms），其他项目没有可测的变化。

## 已知限制

- **Brotli 质量 10–11**：预留量是经验包络，不是证明的上界；聚类分配远超所有实测输入的数据可能超出它。
- **仍按源码推导的保守上界计费的部分**：variants 导出里的 glyf/name/maxp/gasp；共享的 B-tree
  节点估计；pk/gf 读回时按每字形 2,048 B 预留（有测试核对）；color 写出时的 ID 树和映射树；zapfont
  诊断按 2× 计（zapfont 在默认上限内仍可完成）。所有上限都不是 RSS 上限，也不计分配器开销。
- **真实超过 256 MiB 的工作负载**：Unifont 拼合→WOFF2（峰值 RSS 291 MiB），在默认上限下干净地报
  `ResourceLimit`；显式给 `--max-total-bytes` ≥303.9 MiB 后可以完成。Tong 2 jobs × 4 个 WOFF2 线程
  这种非默认配置，需要显式给每次调用 400 MiB 的 WOFF2 预算。
- **格式自身的限制**：c、pbf、plan9、hmzk 装不下 38k 字形的 CJK 字体，给 1 GiB 预算也一样。
- **收录站默认 profile 的位图 `.ttf`**：用 OTB 写出，没有轮廓表，所以过不了 OTS。浏览器能加载的位图 TTF
  要用 `--otb-outline-tables empty-glyf`；explicit profile 下 21/21 都过了 OTS。
- 收录站生产用的 `opf_vector` 扩展没有提供，矢量对照用的是 fontTools 后备。
- 正式测速在用户的桌面工作站上进行，没有办法得到完全空闲的窗口。

## 备份

源码和全部 refs 在 Git main；证据在 WORK。本机检查点保存在 `WORK/checkpoints/`，并在空目录中独立恢复验证过。
没有异机备份。
