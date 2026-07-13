# PVE BlueStore RocksDB Stage 4 真实样本回归

## 样本与边界

- 样本目录：`E:\pangushi\服务器`，仅通过
  `FORENSICS_PVE_CLUSTER_ROOT` 本地 opt-in，不进入生产代码。
- BlueStore 成员大小：
  - `server01-disk02.E01`: `2,803,257,355` bytes
  - `server02-disk02.E01`: `2,822,486,782` bytes
  - `server03-disk02.E01`: `2,806,928,349` bytes
- 导入方式：六成员串行，`max_import_workers=1`、
  `max_analysis_workers=1`、metadata-only。
- Stage 4 只读取 BlueFS 声明的控制文件 extent，解析 RocksDB
  `CURRENT`、可选 `IDENTITY` 和活动 MANIFEST 的 physical log /
  VersionEdit，恢复控制面 inventory。
- 不读取 SST/WAL 内容，不恢复 RocksDB key/value，不重建
  RADOS/PG/object、RBD 或 VM disk。

## 验证命令

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -FixtureRoot 'E:\pangushi\服务器' -RequireFixture
```

2026-07-14 最终门禁结果为 `1 passed`，测试体耗时 `37.78s`；增量编译耗时
`1.06s`。六个成员全部导入成功，cluster 为 `ready`，三个宿主成员为
`ready`，三个 BlueStore 成员为 `ready_metadata`。

独立 oracle 使用 WSL 中只读设备映射和导出：

```bash
sudo bash scripts/dev/inspect-pve-rocksdb-manifest.sh \
  '/mnt/e/pangushi/服务器/server01/server01-disk02.E01'
```

环境为 WSL `ceph-bluestore-tool 18.2.8` 与 `ldb 9.11.2`。样本 OSD
label 记录创建版本 `19.2.3`；Ceph `v18.2.8` 与 `v19.2.3` 均固定
RocksDB revision `9fa4990159853479a222244574ca41202e4c95c1`
（RocksDB `7.9.2`）。

## RocksDB 精确 Oracle

| 成员 | Active MANIFEST | Identity | Edits | CF | Live SST | Next file | Last sequence | Min log |
|---|---|---|---:|---:|---:|---:|---:|---:|
| server01 | `db/MANIFEST-000143` | `318c61d3-7d8b-497a-b02a-d3683123595d` | 39 | 12 | 35 | 148 | 1077117 | 127 |
| server02 | `db/MANIFEST-000121` | `15f9cf98-cb4f-4d78-9d94-ae6235eb075b` | 39 | 12 | 40 | 126 | 1052658 | 105 |
| server03 | `db/MANIFEST-000128` | `8024bc80-69cc-4adc-9f00-364b295f5312` | 39 | 12 | 33 | 132 | 1061239 | 110 |

三个样本的 previous log number 均为 `0`，maximum column-family ID 均为
`11`。活动 column family 集合为：

```text
0 default
1 m-0
2 m-1
3 m-2
4 p-0
5 p-1
6 p-2
7 O-0
8 O-1
9 O-2
10 L
11 P
```

`ldb manifest_dump --verbose` 的最终 DebugString 只显示 `10/8/12` 条
SST 行，不能代表完整 live set。`ldb list_live_files_metadata` 返回
`35/40/33`，与只读 BlueFS export 中的 SST 文件集合逐项一致，且与 native
MANIFEST replay 一致。

## 持久化与隔离

- 每个 `disk02` 保持独立 `source.db`，状态为 `ready_metadata`。
- 普通 `file_entries` 行数为零，不把 BlueStore metadata 伪装成文件树。
- `ceph_rocksdb_manifests`、`ceph_rocksdb_column_families`、
  `ceph_rocksdb_live_files` 与 OSD/BlueFS inventory 在同一事务原子替换。
- live-file metadata 必须能关联到同 inventory 中的 BlueFS `db/*.sst`，
  且不保存 RocksDB internal-key bytes。

## 工程结论

- native parser 与独立 RocksDB 工具对 control fields 和 live set 达成一致。
- live-set oracle 必须使用 `list_live_files_metadata`，不能使用
  `manifest_dump` DebugString 行数。
- Stage 4 证明的是 RocksDB 控制面恢复，不代表 SST/WAL 内容、Ceph object
  或虚拟机磁盘已支持。
