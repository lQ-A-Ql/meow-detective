# PVE BlueFS Stage 3 真实样本回归

## 样本与边界

- 样本目录：`E:\pangushi\服务器`，仅通过
  `FORENSICS_PVE_CLUSTER_ROOT` 本地 opt-in，不进入生产代码。
- 导入方式：六成员串行，`max_import_workers=1`、
  `max_analysis_workers=1`、metadata-only。
- Stage 3 只读取 BlueFS metadata transaction log，恢复目录、文件名、
  fnode 与 extent 元数据。
- 不读取 RocksDB 文件内容，不解析 MANIFEST/SST/WAL，不重建
  RADOS/PG/object、RBD 或 VM disk。

## 验证命令

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -FixtureRoot 'E:\pangushi\服务器' -RequireFixture
```

2026-07-13 验证结果：`1 passed`。三个 `disk01` 为 `ready`，三个
`disk02` 为 `ready_metadata`，cluster 为 `ready`，失败成员为 `0`。

## BlueFS 精确 Oracle

| 成员 | transaction | final sequence | logical bytes | directories | files | stop reason |
|---|---:|---:|---:|---:|---:|---|
| `server01-disk02.E01` | 4 | 186890 | `0x22000` | 5 | 44 | `invalidTail` |
| `server02-disk02.E01` | 4 | 185969 | `0x22000` | 5 | 49 | `invalidTail` |
| `server03-disk02.E01` | 4 | 185678 | `0x22000` | 5 | 42 | `invalidTail` |

精确目录集合：

```text
ALLOCATOR_NCB_DIR
db
db.slow
db.wal
sharding
```

代表性文件元数据：

| 成员 | MANIFEST | WAL |
|---|---|---|
| server01 | `db/MANIFEST-000143` | `db.wal/000142.log` |
| server02 | `db/MANIFEST-000121` | `db.wal/000120.log` |
| server03 | `db/MANIFEST-000128` | `db.wal/000127.log` |

每个 OSD 还必须包含 `db/CURRENT` 和至少一个 `db/*.sst`，上述代表文件
均需保留至少一个 extent。BlueStore source DB 的普通 `file_entries` 必须为
零，BlueFS metadata 只写入 `ceph_bluefs_*` 表。

## 工程结论

- 事务 framing 使用固定宽度小端 `u64` sequence，不是 DENC varint。
- `JUMP(next_seq)` 表示当前事务完成后的 `log_seq`；下一事务期望
  `next_seq + 1`。
- prefix inspection 只依赖首块头部，可安全发现跨块事务长度。
- replay 单事务上限 `16 MiB`、总逻辑读取上限 `64 MiB`，只读取已声明
  log extents，不执行全设备 recovery scan。
- OSD label、BlueFS superblock、replay snapshot 在 source DB 同一事务原子
  替换；失败不得留下部分 metadata。
