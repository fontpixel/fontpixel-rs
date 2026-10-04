# 本地恢复与继续开发

2026-10-04 已完成原交接包的完整导入。随后按用户要求，已清理四个并列
工作区、原交接目录和 `../fontpixel-rs-delivery/` 进度备份目录。
全部 Git 分支和历史保留在当前仓库，参考资料和样本的已导入副本仍在。
冻结状态说明见 `../CODEX-HANDOFF.md`，最新开发结果见
`reports/local-resume-2026-10-04.md` 和各工作项的新验收报告。

## 目录

- 当前目录：主线 Git 仓库；恢复时 `main = 9e6510d`。
- 当前只保留一个工作区；W3.2 已合入主线，W3.3–W3.5 草稿保留在分支中。
- `../reference/`、`../samples/`：只读参考和样本，保持与仓库并列。
- `../validation-inputs/`、`../supplemental-evidence/`：冻结输入和历史补充证据。
- `../original-task/`、`../audit/`、`../drafts/`：原始任务、恢复审计和草稿快照。
- `../fontpixel-rs-all-refs.bundle`：原始完整 Git bundle；52 个 refs 均已恢复。
- `../local-work/`：Python 环境、各工作树独立 Cargo target、原始本地验证日志。

保留的工作项分支为 `w3.2-fnt-fon`、`w3/image-c`、`w3.4-kbitx-pbf`、
`w3.5-hbf`。后续可以在当前仓库切换分支继续开发，无需重新生成这次清理的
并列目录。原 W3.2 工作区中新生成的 763 个 fuzz 样本已校验并归档到
`../local-work/preserved-corpora/w3.2-e80e0f7-2026-10-04.tar.gz`。

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
