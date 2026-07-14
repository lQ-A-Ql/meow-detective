# PVE BlueStore RocksDB Stage 6.3 Latest-State 真实样本回归

## 范围与结论

- 私有样本：`E:\pangushi\服务器`，仅通过
  `FORENSICS_PVE_CLUSTER_ROOT` 本地 opt-in。
- 六成员严格串行导入：三个 `disk01` 宿主文件系统保持 `ready`，三个
  `disk02` BlueStore OSD 保持 `ready_metadata`。
- 全部 `35/40/33` live SST 与 active WAL 已进入同一有界 recovery spool，
  并按 column family、user key、sequence 和 value type 恢复 logical
  latest-state。
- 每个 OSD 产生 12 个 active column-family summary；source DB 只保存计数、
  sequence boundary 与 canonical digest，不保存 raw RocksDB key/value。
- 普通 BlueStore `file_entries` 继续为零，不把对象存储伪装成 POSIX 文件树。

Stage 6.3 已完成可验证的 RocksDB logical latest-state 摘要恢复。它尚未解析
BlueStore onode/blob/value，也未重建 RADOS object、RBD image 或 VM 文件系统。

## 验证命令

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -FixtureRoot 'E:\pangushi\服务器' -RequireFixture
```

2026-07-14 最终复跑：

```text
running 1 test
test commands::import::background_job::tests::real_pve_cluster_import_attempts_every_member_and_isolates_source_databases ... ok
test result: ok. 1 passed; 0 failed
finished in 50.31s
```

该耗时来自本机 debug build，不作为跨机器 SLA。Stage 5 的 `40.32s` 基线只做
SST 结构 inventory；Stage 6.3 新增全 live-set mutation streaming、临时 spool
和约 50 万 mutation 的 latest-state reduction。优化前同功能路径约 `59.5s`，
最终结果改善约 `15.4%`。

## Latest-State Oracle

每个 OSD 的 12 行 column-family summary 按 column-family ID 排序，将下列字段
写入 versioned canonical text 后计算 SHA-256：

- point/SST/WAL mutation counts；
- range/SST/WAL tombstone counts；
- latest value、delete、single-delete、range-delete decision counts；
- merge operand/resolved counts；
- range-hidden version count；
- smallest/largest sequence；
- sharding、point、range 与 latest-state digest。

| 成员 | Active CF rows | Aggregate SHA-256 |
|---|---:|---|
| `server01-disk02` | 12 | `b4f31e224ff485b29b1b3ac7c21e079344250bf37a954b304d43294b1da22eed` |
| `server02-disk02` | 12 | `0cf9b7ead1e5953fa84f1c57a16be4f1a2d5fd4713d2ed1ad20cf8cf9d320880` |
| `server03-disk02` | 12 | `32d7af9d9eda6ca168cb9a85a7b17a36c9fce012f9301b354aebb1b633bee978` |

这些 oracle 同时覆盖完整 live-SST 集合、active WAL overlay、column-family
sharding 和 latest-state 决策；任一 mutation 分类、顺序或 reducer 语义变化
都会改变聚合摘要。

## 恢复语义

- `inspect_sst_with_visitor` 在单次 data-block 读取/解压中同时完成结构
  inventory、脱敏 census 和 mutation callback，避免 Stage 5 与 Stage 6
  分别重读 payload block。
- SST 与 WAL mutation 写入
  `staging/<dataSourceId>/ceph-rocksdb-recovery-*` 下的 disposable SQLite
  spool。spool 只存在于 case workspace，完成或失败后自动删除。
- spool 以 `(column_family_id, user_key, sequence DESC)` 提供确定性 point
  history；range tombstone 单独排序并计算覆盖 sequence。
- reducer 支持 value、delete、single-delete、range-delete，以及经
  `sharding/def` 验证路由的 Ceph `T` int64-array wrapping-add 和 `b`
  bitwise-XOR merge。
- duplicate internal key、非严格递减 history、inactive column family、
  unknown merge operator、越界 sequence 和资源预算超限均 typed fail closed。
- 无 range/merge 的 column family 使用 borrowed point-only fast path；每个
  column family 通过独立 read-only SQLite connection 恢复，结果最终按 ID
  排序，确保并行不改变输出。

## 持久化与原子性

`source_011_ceph_latest_state.sql` 新增
`ceph_rocksdb_latest_state`，约束包括：

- 每个 active column family 最多一行且必须绑定现有 MANIFEST inventory；
- point/range 分类计数必须分别等于 SST + WAL 子计数；
- deleted count 必须等于 delete + single-delete + range-delete；
- sequence boundary 与 mutation 是否为空保持一致；
- digest 必须是 64 位小写十六进制；
- `scan_complete` 必须为 `1`。

OSD、label、BlueFS replay、RocksDB MANIFEST、SST、WAL 与 latest-state summary
通过 `CephOsdRepo` 在同一 replacement transaction 中提交。任何 validation
或写入失败都会回滚完整 replacement，不发布半成品状态。

## 取证与资源边界

- 原始 E01、LVM LV、BlueStore device、SST 和 WAL 始终只读。
- raw key/value 仅存在于 block callback、WAL batch slice 或 disposable spool；
  不进入 source DB、日志、审计、报告或前端 DTO。
- spool 上限：point mutation `5,000,000`、range tombstone `500,000`、
  resident range bytes `64 MiB`、aggregate raw bytes `8 GiB`。range tombstone
  会在 reduction 前装入内存并为 coverage end key 建立副本，因此使用独立的
  resident range-byte fail-closed 预算，而不是仅依赖磁盘 spool 总量。
- 单 SST 继续受 block、entry 和累计解压预算约束；WAL 继续受 file、
  logical-record、batch、key/value 和 mutation 预算约束。
- 同一 E01 reader 的重 IO 仍串行；latest-state 的 column-family recovery
  只并行读取已封存的本地 spool。

## 自动化覆盖

- `cargo test -p rocksdb-wire`
- `cargo test -p persistence-sqlite`
- `cargo test -p app-services --lib`
- `cargo clippy -p rocksdb-wire -p persistence-sqlite -p app-services --all-targets -- -D warnings`
- module-size、function-size、test-layout guards
- `scripts/check-pve-cluster-import.ps1`

新增或扩展的测试覆盖：

- borrowed reducer 与 owned compatibility API 等价；
- value/delete/single-delete/range-delete/merge 和资源边界；
- empty `[a,a)` range tombstone 是合法 no-op，反向 range 被拒绝；
- combined SST scan 与既有 inspection/stream API 等价，payload block 只读取一次；
- spool seal 后可由多个 read-only connection 读取；
- latest-state repository schema、round-trip、source-local replacement、
  foreign-key cascade 与整组原子回滚；
- 三 OSD canonical aggregate oracle。

## 剩余边界

Stage 6.3 的持久化结果是 digest-only logical summary，不是可直接浏览的
RocksDB key/value table。Stage 6.4 必须在 disposable reducer 生命周期内接入
批准的 BlueStore semantic decoder，解析 `S/C/O/X` 与后续 `M/P/m/p` key-space，
再建立 onode/blob/extent、RADOS object、RBD 和 VM 文件系统链路。
