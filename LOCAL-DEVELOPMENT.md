# 本地恢复与继续开发

2026-10-04 已从 `../fontpixel-rs-local-handoff-2026-10-04/` 完整导入。
原交接包保持不变；冻结状态说明见 `../CODEX-HANDOFF.md`，最新开发结果见
`reports/local-resume-2026-10-04.md` 和各工作项的新验收报告。

## 目录

- 当前目录：主线 Git 仓库；恢复时 `main = 9e6510d`。
- `../fontpixel-rs-w3.2/` 至 `../fontpixel-rs-w3.5/`：四个草稿工作树。
- `../reference/`、`../samples/`：只读参考和样本，保持与仓库并列。
- `../validation-inputs/`、`../supplemental-evidence/`：冻结输入和历史补充证据。
- `../original-task/`、`../audit/`、`../drafts/`：原始任务、恢复审计和草稿快照。
- `../fontpixel-rs-all-refs.bundle`：原始完整 Git bundle；52 个 refs 均已恢复。
- `../local-work/`：Python 环境、各工作树独立 Cargo target、原始本地验证日志。

主线使用更新后的 `PLAN.md` 和 `docs/plan-addendum-merge.md`，不要用
`original-task/PLAN.md` 覆盖。按阶段顺序继续；第 3 阶段尚未全部完成。
未配置外部 remote URL。发布、推送和站点部署不在本次工作内。

## 本机环境

已安装仓库固定的 Rust 1.99.0、rustfmt、clippy、cargo-deny 0.20.2；
cargo-fuzz 0.13.2 和 nightly 1.99.0-nightly 已可用。
独立 Python 环境在 `../local-work/oracles/`，CPython 3.12.13，主要 oracle
版本与 `../requirements-oracles.txt` 一致，完整依赖清单在
`../local-work/results/setup/oracles-freeze.txt`。与交接机器的 Python 3.12.14
存在补丁版本差异；字符表检查和完整 Python 差分已在本机重新通过。

每次切换工作树后，从该工作树执行：

```sh
source scripts/local-env.sh
bash scripts/check.sh
```

尚未合入主线环境脚本的草稿可直接设置相同变量。不要共享工作树之间的
`CARGO_TARGET_DIR`，也不要在 `reference/` 或 `samples/` 里生成文件。
`FONTPIXEL_WORK_ROOT` 可更换缓存根目录，`FONTPIXEL_TARGET_DIR` 可显式指定
单次 target。性能测量须等本任务的 build、oracle 和 fuzz 结束后执行。

本地验证包装器 `../local-work/run.py` 保存命令、源码指纹、退出码、耗时和
完整日志；同名结果会拒绝覆盖。它不是生产转换依赖。
