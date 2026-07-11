# 后端模块重构 Stage 2 双源隔离回归

**日期**: 2026-07-11
**父代码基线**: `aed82c02f80968243b1a4c0cc1f842f9cc59d17f`
**运行环境**: Windows / `x86_64-pc-windows-msvc` / Rust stable
**目的**: 验证 Windows/Linux 平台域重构后，导入顺序不会改变 source DB、分区、文件树、预览和分析归属。

## 样本

| 平台 | 私有样本标识 | 大小 | SHA-256 |
|---|---|---:|---|
| Windows | `private/windows/jiancai2.E01` | 19,141,752,608 bytes | `D819689946FC2197DEC73F51BB65B1DA696C4258DB91FAC432DB734629081DA5` |
| Linux | `private/linux/jiancai3.E01` | 2,770,096,513 bytes | `164AD86C83AD68137F96D770A5B8A676703ED0B075A6DCEB3ECC61B1FA5D64B4` |

私有路径只通过 `FORENSICS_STAGE2_WINDOWS_E01` / `FORENSICS_STAGE2_LINUX_E01` 或脚本参数注入，不存在于测试生产逻辑中。

## 命令

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-stage2-real-sample-isolation.ps1 `
  -WindowsFixturePath 'D:\獬豸杯\检材2.E01' `
  -LinuxFixturePath 'D:\獬豸杯\检材3.E01' `
  -Order both `
  -RequireFixtures
```

无参数运行时脚本明确报告 skip；阶段验收必须使用 `-RequireFixtures`，缺少任一私有样本即失败。

## 结果

| 顺序 | 测试 | 结果 | 测试体耗时 |
|---|---|---|---:|
| Windows -> Linux | `real_samples_import_into_isolated_source_databases_serially` | 通过 | 96.92s |
| Linux -> Windows | `real_samples_remain_isolated_when_linux_imports_first` | 通过 | 94.63s |

脚本总耗时约 209s，包含首次增量编译；两项测试均为单导入 worker、单分析 worker 的严格串行路径。

## 验收事实

- 每个数据源均拥有独立 `sources/<dataSourceId>/source.db`，状态为 `ready`。
- `app.db.file_entries` 保持为空，控制库不承载文件树主数据。
- Windows 数据源只保留 NTFS/FAT/exFAT 分区族，不出现 XFS/LVM/ext/Btrfs 串染。
- Linux 数据源保留 XFS/LVM/ext/Btrfs 分区族，导入顺序不改变平台元数据。
- 聚合文件树根节点均使用 `ds:<dataSourceId>:<localId>`。
- Windows 与 Linux 两源均通过真实文件 handle + range preview smoke。
- artifact、timeline、correlation 的 case-level 返回 ID 保持 source-scoped，不发生本地 ID 冲突。

## 未保证范围

- 私有样本不替代可提交 public fixture 与 expected JSON。
- 本测试验证双源严格串行导入，不声明并行重 I/O 导入的吞吐 SLA。
- PVE cluster 多成员语义、Ceph BlueStore、VM disk reconstruction 不属于 Stage 2。
- 本轮不改变 Hex、媒体或文件预览协议。
