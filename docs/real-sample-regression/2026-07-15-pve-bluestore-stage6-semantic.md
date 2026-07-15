# PVE BlueStore Stage 6.4 真实样本回归

## 基线

- 日期：2026-07-15
- 样本：`E:\pangushi\服务器`
- 模式：六成员串行导入，`max_import_workers=1`、
  `max_analysis_workers=1`、metadata-only analysis
- 上游语义基线：Ceph Squid `v19.2.3`
  (`c92aebb279828e9c3c1f5d24613efca272649e62`)
- 证据访问：E01、LVM LV、BlueStore block device 全程只读

执行命令：

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 -RequireFixture
```

单 OSD 诊断命令：

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
cargo test -p forensics-desktop --lib `
  real_pve_bluestore_member_persists_semantic_snapshot `
  -- --ignored --test-threads=1
```

## 结果

六个成员全部完成：

| 成员 | 状态 | 普通文件行 |
|---|---|---:|
| server01-disk01.E01 | `ready` | 62,403 |
| server02-disk01.E01 | `ready` | 62,380 |
| server03-disk01.E01 | `ready` | 62,405 |
| server01-disk02.E01 | `ready_metadata` | 0 |
| server02-disk02.E01 | `ready_metadata` | 0 |
| server03-disk02.E01 | `ready_metadata` | 0 |

Stage 6.4 semantic oracle：

| OSD | collections | objects | blobs | shards | logical | physical | checksums | shared / refs |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| server01 | 34 | 2,924 | 116,135 | 18,971 | 116,487 | 134,148 | 1,839,658 | 23,316 / 27,897 |
| server02 | 34 | 2,927 | 116,135 | 18,970 | 116,487 | 134,154 | 1,839,666 | 23,316 / 27,900 |
| server03 | 34 | 2,930 | 116,135 | 18,974 | 116,487 | 134,150 | 1,839,646 | 23,316 / 27,911 |

| OSD | semantic SHA-256 |
|---|---|
| server01 | `794ab1ea6632d809bac456d9cd5e5e54c3a46b93977d2224f98c0d564a46c73b` |
| server02 | `441e1a48ec5ca51e5ff2caa94eac106d283d9375bbbc08d841196eb84fbe78e9` |
| server03 | `d5eb02ba6e77a66476a2c84f010bca75ec77d870858d15e6b57681fb075028bc` |

每个 source DB 必须满足：

- 仅一个完整 semantic scan，schema/profile 与 repository 常量一致。
- `profile_complete=true`。
- semantic `latest_state_sha256` 等于 persisted RocksDB latest-state set digest。
- semantic SHA-256 覆盖全部规范化行。
- scan count 与 collection/object/blob/shard/logical/physical/checksum/shared/ref
  向量长度精确闭合。
- OSD、BlueFS、RocksDB latest state 与 semantic snapshot 在同一 source-local
  replacement transaction 中提交。
- raw RocksDB key/value、attrs value 和 checksum bytes 不进入数据库、日志、
  审计或普通文件树。

## Shared Blob 修复

首次真实运行暴露的失败为同一 shared blob ID 下两个不同 blob slice 的部分
物理重叠：

```text
0xd12000..0xd14000
0xd12000..0xd16000
shared=0000000000005601
```

旧 validator 只允许“范围完全相同”的共享重叠，与 Ceph
`bluestore_shared_blob_t::ref_map` 语义不符。修复后：

- 同一 blob 的内部物理重叠仍拒绝。
- 不同 blob 的空 shared ID 或不同 shared ID 重叠拒绝。
- 不同 blob 使用相同非零 shared ID 时允许部分重叠。
- 每个 shared blob 已分配物理范围必须由对应 `X` ref-map 连续覆盖。
- ref-map 缺口、范围溢出或非 canonical 行使整个 replacement 回滚。

## 性能记录

首次实现基线：

- 单 `server01-disk02`：约 `486.17s`，观测峰值 RSS 约 `705MB`。
- 六成员串行：约 `1757.04s`。

性能审计确认主要瓶颈不是 E01 顺序读取，而是 semantic snapshot 的校验与
SQLite 写入：

- 旧 object count 校验对每个 object 重扫全部 blob/physical rows，形成
  `O(objects * children)` 行为。`server01` 约为 `2,924 * 116,135`，仅 blob
  侧就超过 3.39 亿次比较。
- checksum 归属校验为约 184 万行重复构造并查询 64-byte object key。
- child table 使用逐行 prepared-statement 执行，SQLite 调用次数与约 210 万
  semantic rows 等量。
- checksum row 重复拥有 inventory/object identity 字符串，放大常驻内存。

2026-07-15 优化保持 schema、count/digest oracle、fail-closed 校验和单事务
replacement 不变，并完成：

- object/blob/extent count 改为一次聚合后精确比较，同时移除 object finalize
  阶段的第二处 `O(objects * children)` 重扫。
- checksum 归属改为按 canonical object ordinal + blob cursor 单次扫描。
- checksum 常驻模型从重复字符串/hex allocation 收敛为
  `object_ordinal + u64 value + width`；inventory ID 从 aggregate scan 派生，
  object identity 从 canonical object row 派生。SQLite 仍保存原有
  `inventory_id/object_identity_sha256/checksum_value_hex` schema。
- semantic child table 按运行时 SQLite `MAX_VARIABLE_NUMBER` 选择批量大小，
  并限制为最多 `8,192` 个 bind parameter、`1,024` 行，低上限 SQLite
  自动回退。
- checksum canonical digest 使用有界 stack buffer 合并固定字段更新，digest
  byte stream 与原实现保持一致。
- aggregate 已完成校验后进入 repository 的 validated write path，避免同一
  replacement 重复执行完整校验；repository 公共入口仍执行完整校验。

保留的真实 `server01` source DB 最新 phase benchmark：

| Run | query | validation | write | commit | total | peak RSS |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 7.750s | 24.114s | 28.909s | 6.862s | 68.34s | 311MB |
| 2 | 7.360s | 25.052s | 35.719s | 8.735s | 77.69s | 311MB |

phase benchmark 完整查询、校验并重写约 210 万 semantic child rows。
compact checksum 表示将该 benchmark 的峰值 RSS 从本轮早期 `366MB`
降至 `311MB`。最新运行保持：

```text
semantic_sha256 = 794ab1ea6632d809bac456d9cd5e5e54c3a46b93977d2224f98c0d564a46c73b
objects         = 2,924
blobs           = 116,135
checksum rows   = 1,839,658
```

独立 phase benchmark 命令：

```powershell
$env:FORENSICS_BLUESTORE_SOURCE_DB_FIXTURE='<retained server01 source.db>'
cargo test -p persistence-sqlite --lib `
  real_bluestore_semantic_phase_performance `
  -- --ignored --test-threads=1 --nocapture
```

门禁预算为 query `<=60s`、validation `<=90s`、write `<=90s`、commit
`<=30s`、peak RSS `<=512MB`。该测试会完整查询和校验约 210 万 semantic
child rows，并写入全新的 source DB，不以摘要或 mock 替代真实数据。

`E:` 重新挂载后使用同一单成员生产导入链路完成最终全链复跑：

| 指标 | 优化前复跑 | 优化后 |
|---|---:|---:|
| import total | 544.105s | 92.673s |
| RocksDB + semantic recovery | 460.934s | 25.017s |
| semantic validation | - | 24.673s |
| semantic write | - | 27.137s |
| transaction commit | - | 6.506s |
| observed peak RSS | 589MB | 537MB |

相同命令下总耗时约缩短 `83%`，约为原来的 `5.87x`；峰值 RSS 降低约
`8.8%`。更早的 Stage 6.4 初始实现曾记录 `486.17s / 705MB`，该结果受缓存
和当时实现差异影响，仅保留为历史基线。最终复跑的 count、semantic digest、
source-local transaction、零普通文件行和 `ready_metadata` 状态全部不变。

## 能力边界

本次完成 BlueStore `S/C/O/X` metadata semantic snapshot，不等同于完整 Ceph
文件系统或 VM 磁盘恢复。以下仍为 unsupported：

- `M/P/m/p` OMAP family 与 RBD directory/header。
- RADOS object content reader。
- PG、replica、CRUSH 或 EC reconstruction。
- RBD 只读虚拟块设备与 VM 文件系统树。
- CephFS MDS metadata/data object reconstruction。
