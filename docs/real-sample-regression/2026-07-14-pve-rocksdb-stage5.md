# PVE BlueStore RocksDB Stage 5 真实样本回归

## 样本与边界

- 样本目录：`E:\pangushi\服务器`，仅通过 `FORENSICS_PVE_CLUSTER_ROOT`
  本地 opt-in，不进入生产代码。
- 六成员严格串行导入，固定 `max_import_workers=1`、
  `max_analysis_workers=1` 和 metadata-only 分析模式。
- 三个 `disk01` 继续验证宿主 `pve/root` EXT4 文件树与关键文件预览。
- 三个 `disk02` 继续保持 `ready_metadata`，普通 `file_entries` 为零。
- Stage 5 只读取活动 MANIFEST 声明的 live SST，并只沿 BlueFS replay
  恢复的 fnode/extents 做 range IO。

本阶段不打开 RocksDB runtime，不执行 recovery/repair/compact，不重放 WAL
生成 latest state，不解析 BlueStore onode/blob/value，不重建 RADOS/PG/RBD
或 VM disk，也不保存 raw key、internal key 或 value。

## 验证命令

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -FixtureRoot 'E:\pangushi\服务器' -RequireFixture
```

2026-07-14 加固运行结果均为 `1 passed`，测试体耗时观测区间为
`40.32s..60.04s`，辅助 meta block 收敛为 checksum-only 后的最近一次无并发
运行为 `40.32s`。该耗时是本机 debug build
的整条六成员门禁结果，受 E 盘与系统缓存状态影响，不作为跨机器性能 SLA。

## 完整 live-SST Oracle

| 成员 | Active MANIFEST | Live SST | DB identity | Last sequence |
|---|---|---:|---|---:|
| `server01-disk02` | `db/MANIFEST-000143` | 35 | `318c61d3-7d8b-497a-b02a-d3683123595d` | 1077117 |
| `server02-disk02` | `db/MANIFEST-000121` | 40 | `15f9cf98-cb4f-4d78-9d94-ae6235eb075b` | 1052658 |
| `server03-disk02` | `db/MANIFEST-000128` | 33 | `8024bc80-69cc-4adc-9f00-364b295f5312` | 1061239 |

聚合结构 oracle：

| 成员 | Data blocks | Entries |
|---|---:|---:|
| `server01-disk02` | 9,994 | 159,439 |
| `server02-disk02` | 10,152 | 160,791 |
| `server03-disk02` | 9,954 | 158,744 |

门禁对每个 live file 验证：

- 唯一映射到 `db/<file-number>.sst`，文件号最少补齐六位且更宽文件号不截断；
- BlueFS logical size 与 MANIFEST file size 一致；
- block-based footer magic、format version 5 和 XXH3 checksum；
- `leveldb.BytewiseComparator`、column-family identity 和 original file number；
- properties、index、data-block count、entry type/count 与 raw-size 自洽；
- MANIFEST smallest/largest sequence 与 SST internal-entry 范围一致；
- key-space census 完整、受预算约束且只保存脱敏 bucket/count/length；
- source DB 中 SST 记录数与 MANIFEST live set 精确相等。

## 代表 SST 独立 Oracle

`server01` 的 `db/000146.sst` 与独立 `sst_dump` 结果精确对齐：

| 字段 | 结果 |
|---|---:|
| File size | 307253 |
| Data blocks | 148 |
| Entries | 23364 |
| Deletions | 0 |
| Raw key size | 420609 |
| Raw value size | 298145 |
| Data size | 245834 |
| Index size | 3106 |
| Filter size | 58437 |
| Compression | LZ4 |

独立 parser test 通过 `FORENSICS_PVE_SST_FIXTURE` 显式读取导出的
`000146.sst`，不是 mock 或仅凭 MANIFEST metadata 推导；该私有样本测试标记为
`#[ignore]`，缺少环境变量或文件时必须失败，不允许静默跳过。

## 持久化与隔离

- `source_009_ceph_sst_inventory.sql` 创建 source-local SST inventory。
- `(inventory_id, file_number)` 唯一标识 SST，允许不同 source DB 使用相同
  RocksDB file number。
- OSD、BlueFS、MANIFEST 和 SST inventory 在同一 SQLite transaction 中原子替换。
- 任一 SST 缺失、重复、size/identity 不匹配、checksum 错误、结构损坏或 census
  不完整时，整组 metadata 写入失败并保留旧快照。
- 未识别或 filter 类辅助 meta block 只验证物理范围与 checksum，不解释或保存
  其压缩内容。
- Census entry 预算在 data-block IO 前预检，累计解压预算在每个 block 后立即
  fail closed；不会先扫描完整 SST 再拒绝，也不会持久化部分摘要。
- schema 和 repository 均禁止不完整 census；repository 递归拒绝 raw key/value
  类敏感字段进入摘要 JSON。

## 结论与剩余边界

Stage 5 已证明三个真实 OSD 的 `35/40/33` 个活动 SST 均可通过只读 BlueFS
range 路径完成物理结构库存，并与独立 MANIFEST、BlueFS 和代表 `sst_dump`
oracle 闭合。当前结果仍是 metadata/structure inventory，不是 RocksDB latest
state，更不是 Ceph object 或虚拟机磁盘恢复。
