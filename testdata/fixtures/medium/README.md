# Medium Fixtures

`testdata/fixtures/medium/` 用于存放不适合默认 PR/CI 的中等体量样本。

用途：

- parser 边界回归
- 跨模块联调
- 人工复验

建议目录：

- `e01/`
- `ntfs/`
- `prefetch/`
- `lnk/`
- `registry/`
- `recycle-bin/`
- `browser/`
- `email/`

要求：

- 每个子目录附带样本来源说明
- 如样本可公开，需附带字段承诺和对齐基准
- 如样本不能公开，只保留 README 模板和占位说明

当前状态：

- 本目录已建立为仓库权威入口
- 具体 medium 样本仍需按链路逐步补齐
