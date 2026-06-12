# 真实样本回归说明

本目录用于记录非公开 fixture 的真实样本回归结果。

每次回归至少记录：

- 日期
- 链路类型
- 样本标识
- SHA256
- 样本大小
- 运行命令
- 运行环境
- 对齐基准
- 结果
- 未保证字段

建议命名：

- `YYYY-MM-DD-prefetch.md`
- `YYYY-MM-DD-lnk.md`
- `YYYY-MM-DD-registry.md`
- `YYYY-MM-DD-recycle-bin.md`

推荐模板：

```md
# 2026-06-12 Prefetch 回归

- 样本：private/prefetch/CMD.EXE-XXXXXXXX.pf
- SHA256：...
- 大小：...
- 运行命令：`cargo test -p artifacts-windows --test fixture_real_test -- --ignored`
- 环境：Windows 11 / Rust stable
- 对齐基准：`expected.json` + 人工对照
- 结果：通过 / 部分通过 / 失败
- 未保证字段：...
```
