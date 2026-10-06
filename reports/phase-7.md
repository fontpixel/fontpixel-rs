# 第 7 阶段交付报告：Python 绑定（`fontpixel-py`）

日期：2026-10-06。最终源码 main `d9ff25c`（本报告在其后提交）。只有本地提交，没有 push、publish 或 deploy；wheel 只在本地构建。
逐 gate 的命令和结果保存在本地证据目录 `../local-work/phase6-local-20261004/gates/`（下称 WORK），
每个 gate 有命令、源码前后快照、完整输出和退出码。

## 结论

第 7 阶段按 PLAN 原文和[绑定范围](../docs/bindings-scope.md)的追加要求完成：

1. **完整原生 API**：fontpixel 与 fontpixel-ops 的全部公共项（rustdoc JSON 列出 2,004 项，另有
   2,669 个字段和枚举成员）都映射到 Python 入口，每条规则都有存在的测试；覆盖率门禁 0 错误、0 未映射。
2. **bdfparser 兼容 API**：`fontpixel.compat.bdfparser`。未修改的 bdfparser 3.0 测试套件跑在它上面：
   1,755 通过、8 失败、10 跳过；8 个失败逐条说明（3 个是有意的设计边界，5 个检查 bdfparser 私有实现）；
   10 个跳过是参考实现自己的慢测试，单独跑 10/10 通过。
3. **测速**：整批和单个小操作都与纯 Python bdfparser 对比，报告里写明哪些更快、哪些持平、哪些更慢。
4. **manylinux wheel**：x86_64 与 aarch64 的 abi3 manylinux2014 wheel 本地构建成功并通过 `auditwheel`；
   x86_64 wheel 安装到 CPython 3.10 和 3.12 后测试全部通过；aarch64 wheel 没有运行环境，未运行。

最终源码通过的检查：

| gate | 内容 | 结果 |
| --- | --- | --- |
| 206 | 完整 `scripts/check.sh`（`0020c26`）（全部 Rust 测试、两个 Clippy profile、最小 CLI Clippy、`api_quickstart`、deny） | 752 个套件 4,106 项测试通过 |
| 210 | `scripts/check_python.sh`：release 扩展、pytest（含 mypy 类型测试）、覆盖率门禁 | 通过：覆盖率门禁 0 错误；pytest 1,253 项全部通过，无跳过 |
| 207 | Python 测速最终一轮 | 见下文「测速」 |
| 212 | `scripts/build_wheels.sh`：两个架构的 wheel、auditwheel、安装后测试 | 通过：两个 wheel 构建并通过 auditwheel；x86_64 wheel 在 CPython 3.10.20 与 3.12.13 上各 1,250 通过、3 跳过（wheel 测试环境没装 mypy） |

rustdoc 无警告（`0020c26` 修了 fontpixel-py 文档里指向私有项的两个链接）。gate 206 之后的提交只改了 Python 文档字符串、
生成器、脚本和文档；它们由 gate 210、212 覆盖。

## 工作项

| 要求（PLAN 第 7 阶段与绑定范围） | 交付 |
| --- | --- |
| pyo3、abi3（Python ≥ 3.10），模块名 `fontpixel` | pyo3 0.29 + pythonize 0.29，abi3-py310，maturin；扩展模块 `fontpixel._native`，Python 包 `fontpixel`（`crates/fontpixel-py/python/fontpixel`） |
| 完整原生开发 API | 模型（FontSet/Font/Glyph，稳定字形 ID、码位与序列映射、度量、名称、属性）、Mono/Gray8 bitmap 的构造/编辑/几何/像素操作、渲染/预览/PNG/SVG、轮廓/矢量化/栅格化、99 个格式 ID 的注册表/能力/检测/读写与 typed options、OpenType 表与容器、WOFF/WOFF2、variants/locl、subset、charset、merge、convert/batch/validate/diff/info、完整报告/错误/strict、资源上限、sidecar/目录/多资产 |
| CLI 编排可调用 | 单元 O 把 CLI 的命令模块抽成库 `fontpixel-ops`（typed request + typed response；内存模式与文件系统模式）；CLI 只剩 clap 解析与 I/O。绑定在进程内调用，不起 CLI 子进程 |
| 数据留在 Rust | FontSet/Font/Glyph/Bitmap 是共享句柄；`font_set.fonts[0].glyph(gid)` 等是视图，原地编辑；像素只在 `to_bytes`/`rows`/`to_list` 等调用时复制出来 |
| typed options | 约 520 个公共类型在 `serde` feature 下派生 serde（单元 S1–S3）；Python 用 dict 传选项，未知键拒绝，构造时的校验保留；枚举值名与 CLI 取值相同 |
| 映射与防漏 | `tools/bindings/inventory.py`（rustdoc JSON → 清单）+ `bindings/coverage/*.toml`（1,157 条规则）+ `tools/bindings/check.py`；新增公共项未映射时门禁失败 |
| 不可用项可查询、明确失败 | 只有 2 项 `unavailable`：`Sfnt::font_ref`、`Cff::fontations`（返回借用第三方 crate 的类型，无法跨语言；替代：表字节和解析后的表）；均附原因与证据 |
| 字节、具名资源、解析器、文件系统 | bytes/bytearray/memoryview 输入；具名资源集合与 Python 解析器回调；文件系统模式保留 sidecar 发现、源别名保护和事务写出 |
| 类型提示、文档、示例 | 每个原生可调用项都有 `.pyi` 存根（`gen_pyi.py --strict`，mypy 测试）；[概念文档](../docs/python-api.md)、[逐项参考](../docs/python-api-reference.md)；`crates/fontpixel-py/examples/` 10 个可运行示例（有测试逐个运行） |
| bdfparser 参考测试 | `crates/fontpixel-py/tests/compat_bdfparser/run_reference.py`；结果与说明见 [KNOWN_DIFFERENCES.md](../crates/fontpixel-py/tests/compat_bdfparser/KNOWN_DIFFERENCES.md) |
| 测速 | [benches/PHASE7-REPORT.md](../benches/PHASE7-REPORT.md)，方法见 [phase7-README.md](../benches/phase7-README.md) |
| manylinux wheel | `scripts/build_wheels.sh`（maturin `--zig`，不需要 Docker），见下文 |

## 单元

| 单元 | 内容 |
| --- | --- |
| O | `fontpixel-ops`：convert、batch、merge、variants、subset、charset、validate、diff、info、preview/render、格式查询；clap 定义与库共用一份，CLI 文本、JSON、退出码和输出不变。证据：收录站默认与显式两套各 194 个产物与 v5 冻结二进制比较，189 个逐字节相同、5 个只差路径；通格批量 87 个产物，85 个相同、2 个只差路径；默认负载巡检无差异 |
| S1、S2、S3 | 核心类型与格式 a–h、i–z 的 serde；不能成为纯数据的类型列在 `docs/bindings-serde-exclusions*.md`，并写明由哪个句柄或函数暴露 |
| P1 | 核心数据 API：句柄与视图、模型、bitmap、渲染、charset、元数据、资源与报告、错误层级 |
| P2 | 格式注册表、typed options、每个格式 ID 的命名空间与读写往返测试 |
| P3 | sfnt 表与容器、WOFF/WOFF2、variants/locl、轮廓、merge |
| P4 | fontpixel-ops 的 Python 绑定与 ops 收尾 |
| P5 | `fontpixel.compat.bdfparser` 与参考测试适配器 |
| P6 | 兼容层修复与性能：17 个核心失败与 5 个慢 TrueType 失败修好；`draw` 按编辑版本缓存绘图模型；`drawall()` 默认参数可用 |
| P7 | 类型存根、API 文档、示例 |
| P8 | 测速工具与报告、manylinux wheel 构建脚本 |

## 覆盖率

`tools/bindings/gate.sh python`（gate 210 内）：

```
{"items": 2004, "fields_and_variants": 2669, "mapped": 2004, "unmapped": 0,
 "by_status": {"bound": 915, "adapted": 1081, "unavailable": 2, "internal": 6}, "errors": 0}
```

`adapted` 指能力完整保留、但按 Python 习惯改了形式的项（例如 Rust 选项结构体 → dict、泛型/trait → 具体方法、
借用 → 复制或视图），每条都写了原因。`internal` 只用于仅为 crate 内部使用而公开的项，每条写了原因。

## bdfparser 兼容

- 参考套件 1,773 项：1,755 通过、8 失败、10 跳过（参考实现自身 1,763 通过、10 跳过）。慢测试单独运行 10/10 通过，
  包括 HanWangYanKai-26 和完整 unifont 的圆点 TrueType。
- 8 个失败：3 个测试往 bitmap 里放像素值 `'3'`，SPEC-v3 只允许 `'0'`/`'1'`/`'2'`，fontpixel 在构造时拒绝（有意）；
  5 个测试检查或 monkeypatch bdfparser 的私有实现（`g._store`、`Font._is_gl`、`pcf._swap_units`），在 Rust 实现里没有对应物。
- 兼容层与 bdfparser 的其余细微差异（不导致测试失败）也逐条列在 KNOWN_DIFFERENCES.md。其中一条行为选择要特别说明：
  兼容层的绘图按内存（`ResourceLimits::max_total_bytes`）而不是原生的 16,777,216 像素上限接纳结果图，
  因为 bdfparser 没有像素上限；原生 API 的像素上限不变，默认上限都没有提高。

## 测速

完整数据和逐项说明见 [benches/PHASE7-REPORT.md](../benches/PHASE7-REPORT.md)（最终一轮，main `0020c26`，
release 构建，每项 5 个独立进程取中位数）。与纯 Python bdfparser 3.0.0 相比（比值 = bdfparser 时间 ÷ fontpixel 时间）：

- **兼容 API 更快**：解析（Tong SC 4.5–4.9×）、写 PCF 4–5×、写 TTF 11–14×、绘制 2,000 个不同字 176–207×、
  `drawall` 25–290×、单字 `draw` 2.5–4×、bitmap 变换 24–1,000×、`todata(2/3/4/5)` 5–24×。
- **持平**：`tobdf()`、`glyphbycp()`、`iterglyphs()`。
- **更慢**：逐字读 `cp()`/`meta` 的遍历（0.41–0.45×）、`headers[...]`/`props[...]`（约 0.17×，每次多约 360 ns）、
  从字符串建 bitmap 和 `todata(0)`/`todata(1)`（10–50 倍慢）。
- **原生 API**：除「从字符串建 bitmap / 转回字符串」和每次返回新 dict 的属性读取外，全部更快
  （查字 11–18×，遍历 13–14×，绘制 34–690×）。

修复前（`f4eb45b`）的两轮测量也保留在报告里：当时兼容层 `draw` 每次 70–95 ms、默认 `drawall()` 报资源上限错误，
这两个缺陷已由单元 P6 修好。

## manylinux wheel

`scripts/build_wheels.sh`（maturin 1.15.0 `--zig --compatibility manylinux2014`，不需要 Docker），最终源码 `d9ff25c`，gate 212：

| wheel | 字节 | SHA-256 |
| --- | ---: | --- |
| `fontpixel-0.1.0-cp310-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl` | 10,276,951 | `8e0de142a869…` |
| `fontpixel-0.1.0-cp310-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl` | 9,684,422 | `7fe41118f72f…` |

- `auditwheel show`：两个都符合 `manylinux_2_17`，只依赖 glibc 系统库（x86_64 最高 GLIBC_2.17）。
- 安装后测试（x86_64）：CPython 3.10.20 与 3.12.13 各 1,250 通过、3 跳过；跳过的是 mypy 检查，wheel 测试环境
  只装 pytest 和 wheel，这些检查在 gate 210 里运行并通过。
- 构建最终 wheel 时发现并修正了两个问题：wheel 成员检查把 `_native/*.pyi` 存根算成扩展模块（gate 209）；
  CPython 3.12 改写了 `__contains__` 的内置文档字符串、不再把 `Point = tuple[...]` 当成类，使「生成文件是否最新」
  测试在 3.12 上失败（gate 211）。生成器已改为在各版本输出相同（`d9ff25c`）。
- aarch64 wheel 是交叉编译的（zig），没有运行：本机没有 aarch64 硬件或 qemu-user。
- 完整说明见 [benches/PHASE7-REPORT.md](../benches/PHASE7-REPORT.md#manylinux-wheels)。

## 设计说明

- **`variants::encode_single` 与 `Variants::encode_owned` 的输出不同，这是有意的。** `encode_single`（`convert` 使用）
  保留物理字形的身份和顺序；`Variants`（`variants` 命令使用）做地区规划，字形顺序和共享方式不同。两者都有测试，
  Rust 文档和 Python 文档字符串都写明了这一点。
- 选项 dict 的键是 Rust 字段名（snake_case），码位在纯数据里写成 `{"Unicode": n}` / `{"Legacy": n}`。
- 句柄由锁保护，可以在多个线程间共享；耗时调用（编解码读写、各操作、WOFF 压缩、variants 规划）期间释放 GIL。
  没有提供 async 接口（Rust 操作是同步的，调用方可以放进线程池）。兼容层的句柄只能在创建它的线程里使用。

## 已知限制

- aarch64 wheel 没有在 aarch64 机器上运行过（本机没有 aarch64 硬件或 qemu-user）。
- 兼容层仍有更慢的调用（见「测速」）；它们是每次调用的 Python 对象开销，不是每个字体的固定开销。
- 参考测试的 8 个失败见上文；它们不是 Rust 兼容层的行为错误。
- 2 个 `unavailable` 项见上文。
- 核心在本阶段有改动（serde 派生、兼容层修复、ops 抽取）。104 目标长 ASAN 与逐 feature Clippy 将在最终报告前对最终源码重跑。
