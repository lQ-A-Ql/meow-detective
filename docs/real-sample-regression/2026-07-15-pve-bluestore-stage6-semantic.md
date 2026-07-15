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

- object/blob/extent count 改为一次聚合后精确比较。
- checksum 归属改为按排序 blob cursor 单次扫描。
- semantic child table 改为受 SQLite bind 上限约束的多行批量写入；每批最多
  `128` 行、`896` 个 bind parameter。
- `inventory_id` 与 object identity 使用共享字符串，checksum hex 使用
  compact boxed string。
- aggregate 已完成校验后进入 repository 的 validated write path，避免同一
  replacement 重复执行完整校验；repository 公共入口仍执行完整校验。

保留的真实 `server01` source DB phase benchmark 连续两次结果：

| Run | query | validation | write | commit | total | peak RSS |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 6.265s | 27.503s | 26.288s | 6.158s | 67.02s | 395MB |
| 2 | 25.468s | 23.838s | 31.298s | 7.290s | 88.60s | 395MB |
| 3 | 4.484s | 20.768s | 20.549s | 5.304s | 51.58s | 395MB |

优化后的独立 semantic phase 稳定在 `51.58..88.60s`，峰值 RSS 为
`395MB`，比旧单成员全链路测试进程观测值低 `310MB`。由于 phase benchmark
不包含 E01/BlueFS/RocksDB 前置读取，不能把这组比例直接表述为完整导入提速；
完整端到端耗时和峰值降幅等待样本盘重新挂载后确认。三次运行均保持：

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

本次优化后尚未重跑完整 E01 导入，因为当前验证进程不可见 `E:` 盘。完整
`server01-disk02` 和六成员串行结果必须在样本重新挂载后补录；在此之前，
上述 phase benchmark 只证明 semantic query/validation/persistence 路径，
不替代 E01/BlueFS/RocksDB 全链路验收。

## 能力边界

本次完成 BlueStore `S/C/O/X` metadata semantic snapshot，不等同于完整 Ceph
文件系统或 VM 磁盘恢复。以下仍为 unsupported：

- `M/P/m/p` OMAP family 与 RBD directory/header。
- RADOS object content reader。
- PG、replica、CRUSH 或 EC reconstruction。
- RBD 只读虚拟块设备与 VM 文件系统树。
- CephFS MDS metadata/data object reconstruction。
