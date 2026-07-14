# PVE BlueStore RocksDB Stage 6.1 WAL 真实样本回归

## 样本与边界

- 样本目录：`E:\pangushi\服务器`，只通过
  `FORENSICS_PVE_CLUSTER_ROOT` 本地 opt-in。
- 六成员严格串行导入，三个 `disk01` 继续验证 PVE 宿主 EXT4 文件树与预览，
  三个 `disk02` 保持 `ready_metadata` 且普通 `file_entries` 为零。
- Stage 6.1 只恢复 RocksDB physical WAL 与 WriteBatch metadata，不持久化 raw
  key/value，不执行 RocksDB runtime recovery，不生成 latest state。
- BlueStore onode/blob/value、RADOS/PG、RBD 和 VM disk 重建仍为 unsupported。

## 验证命令

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -FixtureRoot 'E:\pangushi\服务器' -RequireFixture
```

2026-07-14 最终复跑结果为 `1 passed`，测试体耗时 `58.37s`。该结果来自本机 debug
build，受 E 盘和系统缓存影响，不作为跨机器性能 SLA。

三个只读导出的 WAL 还分别通过独立 parser gate：

```powershell
$env:FORENSICS_PVE_ROCKSDB_WAL_FIXTURE='<exported db.wal/*.log>'
cargo test -p rocksdb-wire --test write_batch_real_sample -- --ignored --nocapture
```

## WAL Oracle

| 成员 | WAL | 文件字节 | logical records | empty batches | mutations | payload bytes | sequence |
|---|---:|---:|---:|---:|---:|---:|---|
| `server01-disk02` | 142 | 3,921,274 | 3,710 | 1,107 | 9,338 | 3,894,471 | 1,077,118..1,086,455 |
| `server02-disk02` | 120 | 4,142,839 | 3,782 | 1,084 | 9,644 | 4,115,489 | 1,052,659..1,062,302 |
| `server03-disk02` | 127 | 4,145,432 | 3,812 | 1,112 | 9,644 | 4,117,873 | 1,061,240..1,070,883 |

独立导出文件 SHA-256：

- server01:
  `5562AD8C98D932E5B00EDDA4C984FCD55B658D799841C7E329ECB8BFCF53F0A9`
- server02:
  `E850987DD2D926289DB89BD4BF07C8EE1D9CB3186FB78AEA179051FA1A3F18D2`
- server03:
  `6EDC9181A59346A6D305A13015861D0152EEF6DE81D04AA4472DA8FA533180A7`

## 恢复与持久化语义

- `db.wal` 存在时优先使用；否则回退 legacy `db`。
- active column family 缺失 `log_number` 时解析为 `0`。
- 恢复下界为
  `max(min_log_number_to_keep, min(active CF resolved log_number))`。
- WAL number 允许间隙，不要求下界文件实体存在。
- `wal_number >= next_file_number` 的 crash-window WAL 仍可选，并保存
  `post_manifest` 来源标记。
- batch sequence 只要求单调且 mutation range 不重叠，不要求连续。
- dropped column-family mutation 保留 sequence/provenance；unknown CF fail closed。
- `LogData` 可记录但不产生 KV effect；`Noop` 在实现 `seq_per_batch` 前
  typed fail closed。
- tracked-WAL MANIFEST addition/deletion 在完整实现前 typed fail closed。
- recyclable physical header 只校验 WAL number 的低 32 位。

`source_010_ceph_wal_inventory.sql` 只保存 WAL 文件摘要和 logical record
provenance。schema 与 repository 均不包含 raw key、raw value 或 batch payload。
OSD、BlueFS、MANIFEST、SST 和 WAL inventory 在同一 source DB transaction
中原子替换。

## 真实样本纠偏

首轮六成员回归使三个 BlueStore source 同时失败。根因不是证据损坏，而是
repository 错误要求未编码 BlueFS fnode 的 `content_size == size`。Ceph Reef
的普通 fnode wire 语义不提供该等式，真实样本在 `encoding=0` 时可保留
`content_size=0`。删除伪约束并继续校验 `encoding`、logical size、extent 和
实际读取长度后，三个 OSD 全部恢复为 `ready_metadata`。

该纠偏已由 synthetic repository test 和完整六成员真实样本门禁共同覆盖。

## 结论

Stage 6.1 已完成三个真实 OSD 的 RocksDB WAL/WriteBatch 物理恢复和 source-local
metadata 持久化，且保持 Stage 5 的 `35/40/33` live-SST oracle 不变。当前尚未
把 SST 与 WAL 合并为 RocksDB latest state，因此不能宣称 BlueStore object、
RADOS、RBD 或 VM 文件系统已经恢复。
