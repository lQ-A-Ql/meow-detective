# Meow~Detective 项目进度台账

> 2026-07-13：新增 PVE 六成员串行导入门禁
> `scripts/check-pve-cluster-import.ps1`。生产后台 runner 在单成员失败后继续尝试
> 后续成员，并在最终 cluster/job 中保存 ready/failed partial 计数。真实样本通过
> `FORENSICS_PVE_CLUSTER_ROOT` opt-in；Ceph BlueStore、VM disk reconstruction
> 和跨节点语义分析仍保持 unsupported。
>
> 同轮结构债务从 17 个模块基线降至 3 个历史 module-root 基线；另有 5 个
> 501-800 行普通生产模块按 `owner/reason/expires=2026-09-30` 登记正式临时例外。
> 函数债务从 17 降至 9。例外只用于本轮无法安全拆分的 parser/repository 边界，
> 到期前必须完成后续能力族拆分。

本文档记录当前可执行进度和下一开发边界。它只登记已经由代码、提交和验证结果证明的状态；详细能力承诺仍以 `docs/parser-support-matrix.md`、`docs/known-unsupported-formats.md` 和真实样本回归记录为准。

## 当前焦点

| 日期 | 类型 | 范围 | 状态 | 结果 | 下一边界 |
|---|---|---|---|---|---|
| 2026-07-12 | Backend/Stage 7 | 文档、最终工程审计、全量门禁与真实样本验收 | Completed | 结构守卫、Rust/frontend 全量门禁、检材3 20 项、双顺序隔离和检材2性能门禁通过；工程评分 99/100 | 继续按 baseline 单调清理剩余 3 个历史模块根、5 个正式临时例外与 9 个函数债务 |
| 2026-07-12 | Backend/Stage 5-6 | Parser/core 能力拆分与测试物理隔离 | Completed | parser/filesystem 能力族完成；非 vendored `src/` 测试债务降至 0 | Stage 7 最终验收 |
| 2026-07-11 | Backend/Stage 3-4 | Transport/command 与 app-services 拆分 | Completed | command/service 边界守卫通过，command raw SQL 为 0，service 保持 Tauri-free | Stage 5 parser/core 拆分 |
| 2026-07-12 | Backend/Stage 4 closure | 清理 app-services 剩余上帝模块与函数债务 | Completed | app-services 模块基线 7→0、函数基线 20→0；全 workspace 门禁、双顺序检材2/检材3隔离、报告/Registry/临时文件失败路径回归通过 | 保持 app-services 零债务并继续清理 parser/core 历史基线 |
| 2026-07-11 | Backend/Stage 2 | Windows/Linux 平台域与多源读写隔离 | Completed | 双顺序真实 E01 回归通过；ready-source、报告归属、Graph 分页、前端切源均加固 | Stage 3 transport/command 拆分 |
| 2026-07-10 | Backend/Stage 0 | 模块、函数、测试物理边界基线 | Completed | 三项结构守卫、单调 baseline、进程树/路径 identity 加固、数据源删除两阶段恢复与真实样本冻结完成 | Stage 1 移除 macOS 生产支持 |
| 2026-07-10 | Linux/PVE | 集群成员导入建模 | Completed | 文件夹发现 6 个 E01 成员，成员保持独立数据源与独立数据库 | 集群级语义关联 |
| 2026-07-10 | Linux/LVM | direct LV 与 dm-thin 只读映射 | Partial | direct root LV 与基础 thin metadata/block mapping 已实现并 fail closed | metadata checksum、更多 thin 变体 |
| 2026-07-10 | Linux/EXT4 | PVE 宿主文件系统 | Completed for private baseline | 三个 `disk01` 的 `pve/root` 均可枚举和预览；代表成员导入 56,471 文件、5,931 目录 | 公开 fixture、更多 incompat feature |
| 2026-07-10 | Linux/Ceph | BlueStore OSD | Next | 三个 `disk02` 已确认是 BlueStore block device，不是普通 POSIX 文件系统 | 标签/元数据解析、OSD inventory、RADOS 对象研究 |

## 代码里程碑

| 提交 | 日期 | 类型 | 状态 | 说明 |
|---|---|---|---|---|
| `72493fce` | 2026-07-12 | Stage 6 | Completed | Rust 测试正文与生产 `src/` 物理隔离 |
| `4c2bd3a7` | 2026-07-12 | Stage 5 | Completed | Parser 与 filesystem 能力族拆分 |
| `49561c9a` | 2026-07-11 | Stage 4 | Completed | Application service 上帝模块拆分 |
| `c3ae351b` | 2026-07-11 | Stage 3 | Completed | Transport 与 desktop command 模块拆分 |
| `7ac7e695` | 2026-07-11 | Stage 2 | Completed | Windows/Linux 平台同层与隔离 |
| `aed82c02` | 2026-07-11 | Stage 1 | Completed | 移除 macOS 生产支持 |
| `7f783497` | 2026-07-10 | 数据隔离 | Completed | 加固 source database isolation |
| `0498b4e7` | 2026-07-10 | 集群导入 | Completed | 增加 Linux cluster import modeling |
| `8d2f84e2` | 2026-07-10 | 生命周期 | Completed | 加固 Linux cluster import lifecycle |
| `1b60ded1` | 2026-07-10 | LVM/PVE | Completed | 加固 Linux E01 cluster parsing 与诊断 |
| `bddef98c` | 2026-07-10 | dm-thin | Partial | 增加只读 LVM thin reader，保留 checksum/repair 边界 |
| `38940702` | 2026-07-10 | EXT4/PVE | Completed | 修复 64-byte group descriptor、高 inode 定位和有界 inode cache |

## 真实样本基线

| 样本 | 测试面 | 当前结果 | 记录 |
|---|---|---|---|
| `D:\獬豸杯\检材2.E01` + `D:\獬豸杯\检材3.E01` | Windows/Linux 双顺序串行导入、独立 source DB、分区、文件树、预览、分析 ID 隔离 | 通过，Windows -> Linux 96.92s；Linux -> Windows 94.63s | `docs/real-sample-regression/2026-07-11-backend-refactor-stage2.md` |
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
- Stage 7 后续清理事实：模块 baseline 3 行、正式临时例外 5 行、函数 baseline 9 行（其中 1 个历史函数超过 150 行）、test-layout baseline 0 行；`app-services` 模块与函数 baseline 均为 0，所有 baseline 只允许减少，临时例外不得无审查延期。
- 检材2三次性能回归：total median `13.479s`、enumeration median `8.488s`、RSS `582MB`、每次 `91,737` rows、最低 `9,892 rows/s`。

## 更新规则

- 每个可交付 stage 完成后新增一条日期记录，不覆盖历史记录。
- `Completed` 必须同时具备代码提交和自动化或真实样本验证。
- `Partial` 必须写明剩余边界；不能用编译通过替代功能验收。
- 历史计划和审计移动到 `docs/archive/<type>/<YYYY-MM>/`，不继续在本台账累积过程细节。
