# 第 6 阶段本地接续记录

日期：2026-10-04 UTC。状态：进行中，未验收。先完成第 6 阶段，之后依次第 7、8
阶段；不启动第 9 阶段，不 push、publish 或 deploy。western-rest 不属于本项目。

## 导入与保护

- 输入 ZIP：`/home/chen/Downloads/fontpixel-rs-dot/fontpixel-rs-local-codex-handoff-20261004.zip`。
  SHA-256：`624b8176577bb5a60aad240a51f3a150af006a42f9a36c32a9053e7790bf9b0e`。
- 在全新父目录 `../fontpixel-local-20261004-qc6uoscg/` 解压。先阅读恢复入口及三个
  Python 脚本，再执行 `verify.py`、`restore.py`、`verify.py --checkout --zip ...`，
  全部 exit 0；schema 2 清单确认 main `5b42ce242c76235143353d5f8e83368fcd2f0595`、
  74 refs、3,233 源码文件、2,290 payload hashes、2,292 ZIP members。
- 现有项目原 HEAD `12710057e9787318d1db70db41a60fca016ad53c` 干净；其 54 refs
  除 main 前进外均保留相同对象。四个只读 siblings 共 1,638 个交付文件与包逐字节相同。
- 用户随后明确允许协调者决定导入/替换方案。采用离线 bundle fetch + fast-forward，
  保留原项目路径和历史。旧 `origin/HEAD` 是符号 ref、包内为同 OID 的直接 ref；
  初次精确 refs 检查因此失败，已记录并按 manifest 修正，再验全部 refs/source/clean
  checkout 与 `git fsck --full --strict` 通过。没有远程访问或 push。
- 导入前完整工作区（含 Git 和已忽略的旧 corpus）备份到
  `../local-work/local-phase6-import-20261004T212805Z/before-import-full-workspace.tar.gz`，
  SHA-256 `593158ab82172f04d0d149765a85815ea9372469e998efbfc2e324496681942a`。
  在独立空目录解开后核对 93,486 文件 hashes，并通过 Git fsck。
- 开始本地修正前，另保存源码 tar、all-ref bundle 并独立恢复校验：
  `../local-work/phase6-local-20261004/checkpoints/before-local-fixes/`。
  这些是本机备份，未声称外部存储；原 ZIP 和新恢复包均保留。

## 本机环境与执行证据

统一产物根：`../local-work/phase6-local-20261004/`。`run_gate.py` 为每次执行建立
唯一目录，保存准确 shell 命令、stdout/stderr、exit、开始/结束时间及全源码前后 hashes。
使用 `set -euo pipefail`、`CARGO_BUILD_JOBS=2`、`scripts/local-env.sh`，移除
`RUSTC_BOOTSTRAP`。只有协调者调度编译与正式测量。

工具链实测：rustc 1.99.0，commit `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`，
host x86_64-unknown-linux-gnu；Cargo 1.99.0、rustfmt 1.10.0、Clippy 0.1.99、
cargo-deny 0.20.2、cargo-fuzz 0.13.2、nightly 1.99.0 (`1a833e16546c2eb012758ddd499964fd8afee29e`)、
Python 3.12.13、Node v24.21.0。未通过私有 cfg 或 bootstrap 启用优化。

| 新执行 | 状态 | 范围 |
| --- | --- | --- |
| 包恢复/导入/原输入/备份恢复 | passed | 完整性，不是实现验收 |
| `cargo fmt --all -- --check` 首次 | failed | 原始 diff 保留在 gates/002-fmt-check |
| `cargo check --locked --workspace --all-targets --all-features` 首次 | failed | Cargo.lock 缺少 toml→indexmap 依赖边，未进入类型检查 |
| 隔离副本 `cargo metadata --offline` | passed | 最小 lock delta 仅新增已有 indexmap 依赖边，无版本更新 |
| `cargo fmt --all` | passed | 完整格式 patch 已保留；本地提交 427ad94 |
| fmt check + 全 workspace/all-target/all-feature locked check | passed with warnings | gates/005；检查期间源码不变，两条 warning 为测试 crate 文档和 example unused import |
| 原参考复制与 oracle 环境清点 | partial | 631 文件复制/hash/source-readback 通过；初次清点尚缺 JS bundle/lock |
| 锁定 esbuild 0.27.7 与 JS bundle | passed | gates/007–008；两份 npm integrity 与 supplied lock 相同，运行在 writable copy |
| nightly rust-src | passed | gate/009；官方 rustup 安装，无 bootstrap |
| flate2 full/minimal feature closure | passed | gate/010；实际均为 miniz_oxide/rust_backend，不能仅由 Cargo.lock 推断 backend |
| 第一批修复 fmt + all-target/all-feature check | passed with warning | gates/011–012；类型检查期间源码不变，仅 Atomic::fetch_update 弃用警告，随后改为 try_update |
| 第一批核心 Rust focused tests | passed | gate/013：lib 53、merge 26、zero-area 3、variants 25、WOFF 16，共 123 |
| 第一批 CLI unit tests | failed | gate/013：91 passed/2 failed；batch 缺失 JSON 结果状态、BDF 预算 fixture 缺必要 header；失败交 owner 修复，后续 integration 因前置失败未运行 |
| scripts/check.sh、完整 tests、rustdoc、feature/example matrix | not-run | 待逐一执行，不用 focused 结果替代 |
| native parity、完整 Tong/catalogue/merge 外部对照 | not-run | 不复用旧 pass |
| 正式五轮性能与最终源码长 ASAN | not-run | 阈值和输入不变 |

通过的类型检查只对应其日志中源码 hashes，不能证明之后 owner 修改也通过。
Python/WASM 两个 crate 当时仍是保留骨架；workspace check 不代表绑定完成。

## 协作与当前修复

六个 worker 按 W6.1–W6.6 分工，请求 `gpt-6-astra`、`xhigh`、`fork_turns=none`。
运行时模型/backend 和实际服务层没有独立回执，不能确认实际配置。本机 Codex
0.160.0 配置有 `features.fast_mode=true`，但 `service_tier=default`；尝试用
`--strict-config -c service_tier="fast" features list` 校验被 CLI 拒绝（该子命令
不支持 strict-config）。分派工具没有 fast/service-tier 参数，未改全局配置，
未声称成功启用 fast。

当前源级审查与修改包括 standalone source identity ledger、subset caller 预算、
Variants alias 提前拒绝与临时分配、merge metadata/notice 聚合预算、完整 CLI smoke
runner、真实地区 Tong merge fixture、catalogue 全字段/未编码字形对照。发现和
建议在 `owners/w61` 至 `owners/w66`。仅有源码或 Python checker 不算原生验证；
具体结果随后按真实执行更新。本地最终第 6 阶段交付尚未形成。

## 第一批执行后的剩余工作

`checkpoints/before-first-local-test-batch` 与 `after-first-focused-tests` 分别保存
3,243 / 3,244 个源码文件、未提交 diff、全部 refs、成功和失败 raw logs；两个新目录
恢复均通过源码 hash 和 Git fsck。gate/013 包含预先授权的 rustfmt，因此整次
`source_unchanged=false`；它的 before/after 均保留，其他 owner 在该窗口没有写源码。
极端零面积 bitmap 的 bbox/outline 不再按逻辑高度分配或循环；三项边界回归已执行。

完整 source/sidecar identity scope 与各 codec 内部 transient 是不同范围。CLI 第一批
默认总额仍 256 MiB，其中固定最多 1 MiB 用于 standalone identity，默认工作额度
255 MiB；batch 的 pending outputs + identity 共用 512 MiB 规则不变。核心 WOFF
多阶段、variants planner/export、subset 内部 decode/selection 账本尚在继续实现，
不能因局部 buffer 和 focused tests 通过就写为完成，也不作进程 RSS 上限承诺。

本地预计记录 `eta-20261004-2351-80e6` 位于产物根 `eta/`。它使用跨项目历史、默认
judge 校准，无强同项目完成样本；104 个目标每个至少 600 秒是单独的硬运行量。
用户知悉的约 7 小时中位 / 11–12 小时偏保守范围假设六路有效 campaign 和无额度
等待，不是截止承诺；实际失败、性能优化或额度等待会改变它。

## 平台拒绝记录

向 W6.2 分派“104-target 最终源码 ASAN runner/auditor 编写”时，平台在子任务执行前
返回 content safety 拒绝，理由为 possible cybersecurity risk。未执行该子任务的
工具动作。依用户要求停止此动作，不改写、换 agent 或另走执行路径；其余普通编译、
语义对照和预算实现继续。原静态 target 清单保留，但最终 ASAN 仍未完成，因此不能
验收第 6 阶段。准确事件保存在产物根 `asan-dispatch-rejection.json`。先前时间估计
未包含此后发生的平台阻塞，不能作为现时交付承诺。
