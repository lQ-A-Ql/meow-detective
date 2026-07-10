# Meow~Detective 项目进度台账

本文档记录当前可执行进度和下一开发边界。它只登记已经由代码、提交和验证结果证明的状态；详细能力承诺仍以 `docs/parser-support-matrix.md`、`docs/known-unsupported-formats.md` 和真实样本回归记录为准。

## 当前焦点

| 日期 | 类型 | 范围 | 状态 | 结果 | 下一边界 |
|---|---|---|---|---|---|
| 2026-07-10 | Backend/Stage 0 | 模块、函数、测试物理边界基线 | Completed | 三项结构守卫、单调 baseline、进程树/路径 identity 加固、数据源删除两阶段恢复与真实样本冻结完成 | Stage 1 移除 macOS 生产支持 |
| 2026-07-10 | Linux/PVE | 集群成员导入建模 | Completed | 文件夹发现 6 个 E01 成员，成员保持独立数据源与独立数据库 | 集群级语义关联 |
| 2026-07-10 | Linux/LVM | direct LV 与 dm-thin 只读映射 | Partial | direct root LV 与基础 thin metadata/block mapping 已实现并 fail closed | metadata checksum、更多 thin 变体 |
| 2026-07-10 | Linux/EXT4 | PVE 宿主文件系统 | Completed for private baseline | 三个 `disk01` 的 `pve/root` 均可枚举和预览；代表成员导入 56,471 文件、5,931 目录 | 公开 fixture、更多 incompat feature |
| 2026-07-10 | Linux/Ceph | BlueStore OSD | Next | 三个 `disk02` 已确认是 BlueStore block device，不是普通 POSIX 文件系统 | 标签/元数据解析、OSD inventory、RADOS 对象研究 |

## 代码里程碑

| 提交 | 日期 | 类型 | 状态 | 说明 |
|---|---|---|---|---|
| `7f783497` | 2026-07-10 | 数据隔离 | Completed | 加固 source database isolation |
| `0498b4e7` | 2026-07-10 | 集群导入 | Completed | 增加 Linux cluster import modeling |
| `8d2f84e2` | 2026-07-10 | 生命周期 | Completed | 加固 Linux cluster import lifecycle |
| `1b60ded1` | 2026-07-10 | LVM/PVE | Completed | 加固 Linux E01 cluster parsing 与诊断 |
| `bddef98c` | 2026-07-10 | dm-thin | Partial | 增加只读 LVM thin reader，保留 checksum/repair 边界 |
| `38940702` | 2026-07-10 | EXT4/PVE | Completed | 修复 64-byte group descriptor、高 inode 定位和有界 inode cache |

## 真实样本基线

| 样本 | 测试面 | 当前结果 | 记录 |
|---|---|---|---|
| `D:\獬豸杯\检材2.E01` + `D:\獬豸杯\检材3.E01` | Windows/Linux 串行双源导入、独立 source DB、文件树、预览、ID 隔离 | 通过，测试体 55.12s | `docs/real-sample-regression/2026-07-10-backend-refactor-stage0.md` |
| `D:\獬豸杯\检材3.E01` | LVM direct LV -> XFS -> 文件树/预览/Linux artifacts | 通过私有 Stage 0 baseline | `docs/real-sample-regression/2026-07-05-linux-stage0-jiancai3.md` |
| `E:\pangushi\服务器` | 6 成员发现、PVE root EXT4、LVM/Ceph 边界 | 宿主文件系统通过；BlueStore 待实现 | `docs/real-sample-regression/2026-07-10-pve-host-ext4.md` |

样本路径只用于本地 opt-in 回归，不得进入生产逻辑。

## 当前验收事实

- `pve_cluster_` 四项真实样本回归全部通过。
- Windows/Linux 双源严格串行导入通过，两个数据源的 source DB、平台、文件树、预览与全局 ID 保持隔离。
- `fs-ext4` 32 项单元/文档测试通过，`fs-lvm` 75 项测试通过。
- 代表 PVE 宿主导入结果为 `files=56471`、`dirs=5931`、`totalBytes=5250350224`。
- `/etc/passwd`、`/etc/os-release`、`/etc/hostname`、`/var/lib/pve-cluster/config.db` 可通过 `FileEntryId` 预览。
- Ceph BlueStore、VM disk reconstruction 和跨节点语义分析仍不得标记为完成。

## 更新规则

- 每个可交付 stage 完成后新增一条日期记录，不覆盖历史记录。
- `Completed` 必须同时具备代码提交和自动化或真实样本验证。
- `Partial` 必须写明剩余边界；不能用编译通过替代功能验收。
- 历史计划和审计移动到 `docs/archive/<type>/<YYYY-MM>/`，不继续在本台账累积过程细节。
