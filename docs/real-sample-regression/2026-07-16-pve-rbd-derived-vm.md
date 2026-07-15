# PVE Stage 6 RBD 派生 VM 文件系统真实样本回归

## 基线

- 日期：2026-07-16
- 样本：`E:\pangushi\服务器`
- 成员：六个 E01，三个 PVE 宿主盘与三个 BlueStore OSD 盘
- 模式：串行导入，`max_import_workers=1`、`max_analysis_workers=1`
- 证据访问：E01、LVM、BlueStore、RADOS、RBD 全程只读
- 派生数据：只写入案件目录中的控制库和独立 source DB

完整导入：

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -RequireFixture -TimeoutSeconds 1200
```

保留案件快速回归：

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
$env:FORENSICS_PVE_RBD_CASE_ROOT='<retained case root>'
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -RequireFixture -TimeoutSeconds 300
```

快速模式只复用已经导入的六个物理成员，验证 RBD lineage、派生 source DB、
VM 文件树和真实预览。完整模式仍重新串行导入全部六个 E01，并执行物理成员、
BlueStore semantic oracle、独立 source DB 和派生 RBD 的深度验收。

## 已验证结果

完整导入结果：

```text
cluster state       = ready
physical sources    = 6
derived RBD sources = 1
total sources       = 7
derived image       = vm-100-disk-0
VM file records     = 114,260
```

派生 RBD source DB 中识别并枚举：

- 直接 XFS 分区。
- `centos/home` XFS logical volume。
- `centos/root` XFS logical volume。
- `/etc/passwd` 可通过全局文件 ID、文件 handle 和 range API 读取。
- 文本、Hex、媒体和通用 range 预览继续复用既有只读证据读取链路。

派生 source 使用独立的
`sources/<derivedDataSourceId>/source.db`，控制库只保存数据源注册、RBD lineage
和副本来源。文件树不会写回三个 BlueStore source DB，也不会写入其他宿主
source DB。

## 性能问题与修复

旧实现运行超过 `1h47min` 时只生成约 `6,828` 条 VM 文件记录。性能审计确认
这是多层放大叠加，不是单一 Ceph 算法问题：

1. 重新打开三个 source DB。
2. 重新查询并构造 BlueStore object read plan。
3. 重新打开三个 source-bound evidence reader。
4. 对同一 RBD object 的重叠小范围重复执行三副本读取和比较。
5. placeholder 文件树路径缺少外层事务，批量插入退化为 SQLite autocommit。
6. Graph 投影使用两遍 `LIMIT/OFFSET` 扫描和多次小事务。
7. XFS BFS 枚举每次按完整路径从根目录重新解析祖先目录。

修复后：

- 每个副本持有长期 source DB connection 和 evidence reader。
- 每个副本使用最多 128 项、估算上限 16 MiB 的 object read-plan LRU。
- provider 使用 64 KiB 对齐的 verified page cache。
- verified page cache 上限为 1,024 页、64 MiB。
- 缺失对象只在完整副本集合全部缺失时按 sparse hole 处理。
- 每个首次加载的 page 仍从三个副本读取并逐字节比较；缓存不会降低副本一致性。
- placeholder 更新和分区文件树写入纳入同一事务，失败时整体回滚。
- Graph 节点和 contains 边改为 SQLite 内部两条 `INSERT ... SELECT` 集合投影。
- XFS 使用有界目录缓存：最多 100,000 个 path-to-inode 记录和 32,768 个目录
  inode；不缓存全部普通文件 inode。
- Ceph 父 source 读取统一走 typed reconstruction route，只接受同案件的
  `ready` / `ready_metadata` 数据源。

真实样本结果：

| 测试 | 结果 |
|---|---:|
| 优化前运行 | `>1h47min`，约 6,828 records，未完成 |
| 64 KiB page cache 后 RBD 文件系统物化 | 约 `187.5s` |
| provider 持久连接、plan/page cache、range 索引后首次物化 | `200.17s` |
| 文件树事务 + 集合式 Graph 投影后首次物化 | `120.21s` |
| XFS 有界目录缓存后首次物化 | `46.28s`、`54.73s` 两次实测 |
| 主 `centos/root` XFS LV 枚举 | `113.27s` 降至 `39.25s` / `46.59s` |
| 已完成 source 幂等物化 | `124ms`、`136ms` 两次实测 |
| ready source 的 tree + preview 回归测试体 | `6.05s`、`6.54s` 两次实测 |
| 保留案件全量 source isolation 深度审计 | 本轮 `131.53s` |

阶段计时来自 `build_and_enumerate_source` 内部 `Instant`，不含 Cargo 编译。
`PVE_RBD_MATERIALIZE` 包含派生 source 注册、物化调用和结果收敛，也不等同于
完整六成员导入。XFS 优化后的两次测试因 Windows 文件缓存和 E: 盘状态存在
波动，因此当前基线按 `46.28s` 到 `54.73s` 的观测区间记录。

`131.53s` 深度审计主要来自读取三个约 1 GiB BlueStore source DB 的完整
semantic oracle 和数据库健康检查，不代表 RBD 文件树被重新构建。此前文档中的
`507.19s` 混合了 Cargo、六成员导入、完整性检查与断言，缺少独立墙钟记录，
不再作为发布级性能基线。本轮未重新执行完整六成员冷导入；日常开发使用 retained
快速模式，发布或 schema/物理导入链路变更再运行完整模式。

## 证据完整性边界

- 副本集合来自已导入集群成员和持久化 lineage，不扫描案件目录猜测来源。
- 当前样本已验证三个 OSD inventory、source ID、inventory ID 和 OSD ID 唯一。
- 部分副本存在、部分副本缺失时 fail closed。
- 三副本返回不同字节时 fail closed。
- 原始 E01 和 BlueStore device 不写入。
- 不把 RBD 伪装为 RAW；数据源类型保持 `CephRbd`。
- 多 PV 或跨 RBD 的 LVM 尚未启用，遇到时 fail closed。
- RBD parent/clone、snapshot、encryption、journaling 和改变读取语义的 feature
  仍返回 typed unsupported。
- 当前实现尚未独立证明“加载到的 OSD inventory 集合就是集群完整副本集合”；
  degraded/missing-inventory 场景不得解释为已闭合。
- `catalog_complete` 的缺失对象快速路径尚未重算全部 semantic child digest；
  损坏 catalog 与合法 sparse zero 的语义仍需继续加固。

## CephFS 结论

当前样本对 CephFS 的结论是：

```text
indeterminate (strongly leaning absent)
```

倾向不存在的证据包括：PMXCFS 未配置 `cephfs:` storage、`ceph.conf` 无 MDS
section、未发现真实 MDS daemon 目录、OSD object catalog 以 RBD 与
`.mgr/devicehealth` 为主，以及 monitor `mdsmap` epoch 1 疑似零 filesystem。

这些证据尚不足以证明 monitor FSMap 的新鲜度和权威性，因此当前不得创建
CephFS 数据源，也不得把 CephFS 标记为已支持。后续判定规则保持：

```text
FSMap.filesystems > 0                     => present
FSMap.filesystems == 0 + freshness proof => absent
otherwise                                => indeterminate
```

## 当前能力结论

本私有样本已经验证：

- 六成员独立导入。
- 三个 BlueStore OSD 的 metadata、RADOS object plan 与三副本读取。
- 一个真实 RBD head image 的只读字节重建。
- RBD 分区、单 PV LVM、三个 XFS 文件系统的完整枚举。
- 114,260 条派生 VM 文件记录。
- `/etc/passwd` 的真实 handle/range 预览；通用预览路由已接通，但“任意文件、
  多分区、多 range”仍需扩大真实样本 oracle。

当前仍不承诺：

- 通用 PG/CRUSH/acting-set 计算。
- EC pool reconstruction。
- 不完整或 degraded replica set 恢复。
- 多 PV、跨 RBD、thin/cache/RAID/snapshot LVM 组合。
- CephFS MDS metadata/data object reconstruction。
- 以该私有样本替代公开 fixture 与 expected JSON。
