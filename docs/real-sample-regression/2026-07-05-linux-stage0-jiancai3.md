# Linux Stage 0 单盘回归 — 检材3

**日期**: 2026-07-05
**链路类型**: E01/RAW -> partition table -> LVM direct LV -> XFS -> file tree / preview / Linux artifact candidates
**样本标识**: private/linux/jiancai3.E01
**SHA-256**: `164AD86C83AD68137F96D770A5B8A676703ED0B075A6DCEB3ECC61B1FA5D64B4`
**大小**: 2,770,096,513 bytes
**运行环境**: Windows host, Rust stable toolchain, opt-in real sample test

## 运行命令

```powershell
$env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'
cargo test -p app-services --test linux_e01_integration linux_e01_lvm_expansion_discovers_logical_volumes -- --ignored --nocapture
```

样本路径仅为本地人工环境示例。生产代码、公开 fixture、CI 默认任务不得硬编码该路径。

## 对齐基准

- `crates/app-services/tests/linux_e01_integration.rs`
- `docs/validation-trust-framework.md` 中的 Linux 检材3 baseline
- `docs/parser-support-matrix.md` 中的 Linux Stage 0 单盘镜像 baseline
- `scripts/check-stage5-regression-guard.ps1` 中的 Stage 5 工程边界与 baseline 反回归检查

## 结果

通过。

关键输出摘要：

- `/etc` 枚举返回 201 个 children。
- 必要样本路径存在性：`passwd=true`、`os-release=true`、`hostname=true`。
- LVM pool root candidate：`Partition 1 (LVM)`，状态为 `Expanded`。
- Root LV root candidate：`Partition 2 (XFS) - cl/root`。
- import 输出：`files=51261`、`dirs=7149`、`total=4368444442`。
- 可见 roots：`Partition 0 (XFS) - Partition 0`、`Partition 2 (XFS) - cl/root`。
- swap LV 被识别为无支持文件系统并进入 warning，不作为可见文件树 root。

## 验收口径

- LVM pool 分区不得作为可展开文件树 root 暴露。
- `PartitionStatus::Expanded` 必须保留，用于表达 pool 已中继到 logical volume。
- root LV 必须能作为 XFS root 导入并出现在可见 roots。
- 关键 Linux 文件可通过 `FileEntryId` 路径链路进入预览测试面。
- 该 baseline 只证明当前私有单盘 Linux 链路，不提升公开发布支持等级。

## 未保证字段

- 不承诺 PVE cluster、多 E01 聚合或跨节点关联。
- 不承诺 LVM thin-pool、cache、RAID、snapshot、VDO、writecache、partial/degraded VG 激活。
- 不承诺 XFS/ext4/Btrfs deleted recovery、journal replay 或 carving。
- 不承诺把该私有样本纳入默认 CI。
