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

发布前如需对三个父 BlueStore `source.db` 做完整文件 SHA-256 前后比较，可在
retained 模式增加 `-DeepParentHash`。默认门禁比较文件长度、mtime、首尾
64 KiB 摘要和 WAL/SHM sidecar，以避免每轮额外读取约 6 GiB；完整 hash 作为
较慢的深度只读审计运行。

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
- `/etc/passwd`、直接 XFS 文件、home LV 根级文件、16 MiB 文件和 614 MiB
  文件可通过全局文件 ID、opaque preview session 和 bounded range API 读取。
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
- 直接 XFS、`centos/home`、`centos/root` 的五个代表文件真实预览，覆盖
  `1,019 B` 至 `614,794,240 B`、连续/随机/文件尾 range 和跨运行 SHA-256
  oracle。

当前仍不承诺：

- 通用 PG/CRUSH/acting-set 计算。
- EC pool reconstruction。
- 不完整或 degraded replica set 恢复。
- 多 PV、跨 RBD、thin/cache/RAID/snapshot LVM 组合。
- CephFS MDS metadata/data object reconstruction。
- 以该私有样本替代公开 fixture 与 expected JSON。

## 2026-07-17 VM 预览性能加固

本轮已完成：

1. Ceph RBD range 接入文件系统 bounded-range，不再整文件 materialize。
2. source-scoped `DerivedRbdRuntime` 复用三副本 provider、connection、device、
   plan cache 与 verified page cache。
3. `preview:<uuid>` opaque session、TTL/LRU、显式 close、scope generation 和
   case/source retire + active-read drain。
4. XFS target-only path lookup，避免每级目录为全部兄弟条目读取 inode。
5. 64 KiB verified page 保持不变；大请求最多合并四个连续 miss 为一次
   256 KiB 三副本读取。
6. 新增 `scripts/check-pve-rbd-preview-performance.ps1`，默认缺 fixture 跳过，
   `-RequireFixture` 时缺失即失败；关键 range 必须匹配固定 SHA-256 oracle，
   Cargo 编译预热不计入单轮性能超时。
7. 门禁增加检材3原生 XFS 与 PVE 宿主 `pve/root` EXT4 对照，两个对照均使用
   固定样本 fingerprint 和固定 range SHA-256。
8. 真实样本验证 viewer/media 字节完全一致，并覆盖 source/case retire、
   旧 handle 拒绝、reactivate 冷重建和 session 归零。

三轮统一门禁结果：

| 指标 | 中位结果 |
|---|---:|
| cold 文件读取，不含 runtime open | `349.992ms` |
| cold runtime + 文件读取，仅报告 | `3,185.749ms` |
| warm 同范围 64 KiB p95 | `0.699ms` |
| 连续 `16x64 KiB` p95 | `16.741ms` |
| 连续 `4x1 MiB` p95 | `234.152ms` |
| 614 MiB 文件随机 64 KiB p95 | `73.804ms` |
| 原生 XFS warm 64 KiB / `4x1 MiB` p95 | `0.058ms / 11.165ms` |
| PVE 宿主 EXT4 warm 64 KiB / `4x1 MiB` p95 | `0.077ms / 7.991ms` |
| RBD/native warm 原始比值 | `12.074x`，仅报告 |
| RBD/native warm 门禁比值 | `0.699x`，使用 `1ms` 分母噪声下限 |
| provider construction | steady `1`，两次 invalidation 冷重建后 `2 / 3` |
| runtime cache capacity | `117,440,512 B` |
| RSS delta | `399-448 MiB` |

当前状态应表述为：

```text
VM 文件树与代表性大文件 bounded-range 预览已通过私有样本性能门禁；
viewer/media 字节一致性及 source/case 失效冷重建已验证；
首次三 OSD E01/LVM runtime 初始化、浏览器端 media 时序和容量 LRU eviction 仍需继续验收。
```

完整设计、门禁阈值和剩余风险见
`docs/ceph-rbd-vm-preview-performance-design.md`。

## 2026-07-17 Catalog 与后处理完整性加固

本轮将派生 source 的“可浏览”和“全部投影完成”拆为独立状态：

- `data_sources.import_state=ready` 只证明文件 Catalog 可浏览。
- `data_source_processing_phases` 保存 Catalog、Graph、Platform、Artifacts、
  Timeline、Search 六个 phase 的版本、input fingerprint、owner/attempt、lease、
  heartbeat 和 terminal state。
- `get_data_sources` 返回后端聚合的 processing state、计数、lastError 和 phase
  明细，前端不自行重建依赖语义。

ready source 的重复物化不再扫描 114,260 条 `file_entries`。版本化 Catalog
manifest 保存于 `source_meta`，绑定 lineage fingerprint 和 materializer version；
显式 `verify_derived_source_catalog` 才执行全表深度 digest。digest 覆盖
path/name/type/size/status/MACB/partition 和稳定父路径身份，可检出父子关系漂移，
同时不包含随机 UUID。

XFS 目录和 inode 路径新增 typed completeness diagnostic；派生 Catalog 只有在
不存在 `DirectoryPartial`、`DirectoryUnreadable`、`EntryUnavailable` 时才能
ready。XFS v1/v2 inode core 按 100 bytes、v3 按 176 bytes 解码，BIGTIME/crtime
与 MACB 继续传播。

三个父 BlueStore source DB 在 RADOS/RBD reconstruction 中统一 read-only 打开，
不运行 migration，不创建缺失数据库，也不产生 WAL/SHM。后续真实样本门禁将继续
把父库 hash/mtime/sidecar 零变化、manifest 快速复用、deep audit 和六 phase
状态作为硬断言。
