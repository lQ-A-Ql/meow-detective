# Ceph BlueStore Stage 6 设计

**开发基线**: `20188bb0`
**设计日期**: 2026-07-14
**当前实现切片**: RocksDB latest state + BlueStore `S/C/O/X` semantic snapshot
**最终目标**: BlueStore OSD -> RocksDB latest state -> RADOS object -> RBD image -> VM 文件系统

## Summary

Stage 5 已经完成三个真实 PVE OSD 的 BlueFS、RocksDB MANIFEST 和全部 live
SST 结构库存，但仍没有恢复 RocksDB 的逻辑最新状态，也没有解释 BlueStore
onode/blob/extents。Stage 6 从 RocksDB 恢复语义开始，逐层建立后续 RADOS 和
RBD 重建所需的可信基础。

本阶段严格区分两条存储产品链路：

- PVE VM 磁盘通常位于 RBD。目标是恢复 RBD 对象序列并提供只读虚拟块设备，
  再复用已有分区和文件系统解析器展开 VM 文件树。
- CephFS 依赖 MDS 元数据池和数据池。只有真实样本证明存在 CephFS 时才进入
  独立设计，不把 RBD 对象误报为 CephFS 文件树。

Stage 6.1 已交付 active WAL 选择、物理日志恢复、WriteBatch wire decoder
和 source-local metadata 持久化；Stage 6.2 已交付 live-SST entry stream。
Stage 6.3 现已将全部 `35/40/33` live SST 与 active WAL 写入 source-local
临时 spool，按 RocksDB internal-key 顺序执行 value/delete/single-delete、
range-delete 和批准的 Ceph `T`/`b` merge reduction，并只在 source DB
持久化每个 column family 的计数、sequence boundary 和 canonical digest。
raw key/value 不进入 source DB、日志或报告，三个 `disk02` 数据源继续保持
`ready_metadata`。Stage 6.4 已在同一 reducer 生命周期内解码 `S/C/O/X`
最新值，持久化 super、collection、onode、shard、blob、logical/physical
extent、规范化 checksum chunk 与 shared-blob ref-map。语义快照绑定
latest-state digest，并通过 canonical aggregate digest 和三 OSD 精确行数
oracle 验证；当前仍未生成 RADOS object reader。

## 开发基线与事实源

### 样本与已验证 Oracle

真实样本根目录：

```text
E:\pangushi\服务器
```

Ceph 集群事实：

| 项目 | Oracle |
|---|---|
| Ceph FSID | `3f28d8bb-e754-475b-b471-b9c97161bbf7` |
| OSD | `0 / 1 / 2` |
| BlueFS 文件数 | `44 / 49 / 42` |
| live SST 数 | `35 / 40 / 33` |
| RocksDB last sequence | `1077117 / 1052658 / 1061239` |

RocksDB 格式基线：

```text
revision: 9fa4990159853479a222244574ca41202e4c95c1
version:  7.9.2
```

Ceph 语义基线：

```text
Ceph Squid v19.2.3 (`c92aebb279828e9c3c1f5d24613efca272649e62`)
```

### 上游源码证据

实现以对应 revision 的源码符号为事实源：

- RocksDB `db/write_batch_internal.h::WriteBatchInternal::kHeader`：
  WriteBatch header 固定为 12 bytes。
- RocksDB `db/write_batch.cc::ReadRecordFromWriteBatch`：
  tag、column-family ID、key/value 的精确 wire 顺序。
- RocksDB `db/write_batch.cc::WriteBatchInternal::{Sequence,Count}`：
  header 为 `fixed64 sequence + fixed32 count`。
- RocksDB `db/dbformat.h::ValueType`：
  mutation、column-family、transaction、blob 和 wide-column tag 编号。
- RocksDB `db/db_impl/db_impl_open.cc::RecoverLogFiles`：
  WAL 按 file number 排序，并基于 minimum log boundary 跳过过旧 WAL。
- Ceph `src/os/bluestore/BlueStore.cc`：
  `S/T/C/O/M/P/m/p/L/B/b/X` key-space prefix。
- Ceph `src/os/bluestore/bluestore_types.h`：
  `bluestore_onode_t`、`bluestore_blob_t`、physical extent 和 shared blob
  的 DENC 布局。
- Ceph `src/kv/RocksDBStore.cc`：
  prefix、column-family sharding 和 merge operator 路由。
- Ceph `src/cls/rbd` 与 `src/librbd`：
  RBD directory/header/object naming、striping 和 snapshot 语义。

任何未被固定 revision 和真实样本双重验证的格式必须 typed fail closed。

## 开发边界

### 必须保持

- 原始 E01、LVM LV 和 BlueStore block device 严格只读。
- 不调用 `repair`、`compact`、`mkfs`、`fsck --repair` 或写模式 RocksDB。
- 不依赖宿主挂载后的 Ceph 集群状态作为生产事实源。
- 不猜测 PG、CRUSH、replica 或 EC shard 映射。
- raw RocksDB key/value 只允许以当前 logical record 的借用切片存在；当前物理
  log decoder 会在单个 WAL 的有界生命周期内保留文件 bytes 与重组后的
  logical-record payload，但二者都不得持久化或写入日志。
- parser crate 不依赖 Ceph、SQLite、Tauri 或 app-services。
- 测试正文只存在于物理 `tests/` 目录。

### 当前仍不做

- 不持久化 RocksDB logical raw key/value；只持久化 digest-only latest-state
  summary。
- 不执行未经批准的 merge operator；当前仅支持 Ceph `T` 的 int64-array
  wrapping-add 与 `b` 的 bitwise-XOR。
- 不持久化 BlueStore attrs 原值、RocksDB key/value 或 checksum 原始字节；
  只保存规范化字段、计数和摘要。
- 不解释 `M/P/m/p` OMAP family，不生成 RADOS object content reader。
- 不生成普通 `file_entries`。
- 不宣称 RADOS、RBD、VM 文件树或 CephFS 已完成。

## 目标架构

```text
E01 / LVM LV
  -> BlueStore label
  -> BlueFS replay
  -> CURRENT / MANIFEST / live SST / active WAL
  -> RocksDB logical recovery stream
  -> bounded latest-state reducer
  -> BlueStore key/value decoder
  -> onode + blob + physical extents
  -> RADOS object reader
  -> pool / PG / replica correlation
  -> RBD metadata + object striping
  -> read-only virtual block device
  -> existing partition / filesystem readers
  -> VM file tree + FileEntryId preview
```

依赖方向：

```text
rocksdb-wire <- app-services -> persistence-sqlite
ceph-wire    <- app-services
domain       <- app-services
```

计划增加的稳定能力族：

```text
rocksdb-wire/src/
  write_batch/
    mod.rs
    model.rs
    parser.rs
  recovery/
    wal_selection.rs
    latest_state.rs
  sst/
    entry_stream.rs

ceph-wire/src/
  bluestore/
    key.rs
    onode.rs
    blob.rs
    extent.rs
  rados/
    object_id.rs
    pg.rs
  rbd/
    metadata.rs
    striping.rs

app-services/src/
  ceph_reconstruction/
    rocksdb.rs
    bluestore.rs
    rados.rs
    rbd.rs
    vm_reader.rs
```

## Stage Design

### Stage 6.1 - WAL / WriteBatch Foundation

#### Phase 6.1.1 - WriteBatch wire decoder

Tasks:

- 解码固定 12-byte header：
  `sequence:fixed64LE + count:fixed32LE`。
- 支持普通和 column-family 版本的：
  `Put/Delete/SingleDelete/Merge/DeleteRange`。
- 保留不参与 mutation count 的 `LogData` 与 `Noop` 身份；Ceph recovery profile
  接受 `LogData`，在实现 RocksDB `seq_per_batch` 语义前对 `Noop` typed fail closed。
- transaction、blob index、timestamp deletion、wide-column 和未知 tag
  首版全部 typed unsupported。
- 为每个 mutation 计算确定性的 sequence：
  `batchSequence + mutationOrdinal`。
- 校验 sequence 不超过 RocksDB 56-bit 上限。
- 完整解析成功并验证 declared count 后才向调用方暴露 mutation。
- key/value 使用借用切片，不复制原始 payload。
- 对 batch bytes、mutation count、auxiliary count、key 和 value 设置独立上限。

Expected result:

- 可安全解析普通 Ceph RocksDB WriteBatch。
- malformed、count mismatch、越界长度、非 canonical varint、sequence overflow
  和未支持 tag 都不会产出部分有效 batch。
- WriteBatch parser 内存与 mutation 数量线性相关，不对 key/value 再做复制。

#### Phase 6.1.2 - WAL 文件选择与物理日志恢复

Tasks:

- BlueFS replay 存在 `db.wal` 目录时只选择其 canonical
  `<decimal>.log` 直接子文件；否则回退到 legacy `db/<decimal>.log`。
- active column family 缺失 `log_number` 时按 RocksDB 语义解析为 `0`。
- 恢复下界固定为
  `max(min_log_number_to_keep, min(active column-family resolved log_number))`。
- WAL 按 file number 升序读取。
- WAL file number 允许存在间隙；不要求恢复下界对应的 WAL 实体存在。
- `wal_number >= MANIFEST next_file_number` 的 crash-window WAL 仍参与恢复，
  并持久化 `post_manifest` 来源标记。
- recyclable log header 必须与文件号一致。
- physical CRC、fragment sequence 和 logical record limits 继续复用
  `decode_log`。
- logical record 必须完整解码为 WriteBatch。
- batch sequence 只要求单调且 mutation range 不重叠，不要求全局连续。
- dropped column-family mutation 保留 sequence/provenance 但不产生 KV effect；
  未知 column-family ID typed fail closed。
- MANIFEST 出现 tracked-WAL addition/deletion 时，在完整实现其恢复语义前
  typed fail closed，不能按普通 safe-ignore 字段跳过。
- 记录 WAL number、record ordinal、physical offset、batch sequence/count，
  不记录 raw payload。

Expected result:

- 可以确定三个真实 OSD 当前需要参与恢复的 WAL 集合。
- 任何缺失、重复、过旧、名称异常或 recyclable identity 不一致的 WAL
  都会被明确分类。

### Stage 6.2 - SST Entry Stream

stage_design:

Stage 5 已验证全部 live SST 的物理结构。Stage 6.2 在相同 block reader 上增加
只读 entry visitor，不改变 `inspect_sst` 的结构库存行为。

Tasks:

- 逐 data block 流式访问 internal key、sequence、type 和 value。
- range tombstone 走独立 visitor；raw block 允许上游合法的未排序记录，
  `start == end` 保留为空区间，只有 `start > end` 非法。
- raw key/value 只在 callback 生命周期内有效。
- caller 可按 column family 和批准的 BlueStore prefix 过滤。
- 不建立全 SST key/value vector。
- 保留 checksum、compression、restart 和 count 的现有校验。
- 按 RocksDB bytewise comparator 校验 point data 跨 block 的 internal-key 全局顺序：
  user key 升序，同 user key 的 sequence/type trailer 降序。
- range block 要求官方 writer 的 no-compression 与每 entry restart 形状。
- external SST version/global-sequence property 首版 typed fail closed，不能把
  encoded sequence `0` 当作真实 sequence。
- point/range callback 暴露 raw internal key、block handle、block ordinal 和
  entry ordinal，供后续 spool 建立可审计 provenance。
- 使用独立的 data-block、total-entry、range-delete 和累计解压预算；超限时
  在触发对应 callback 前失败。
- visitor error 保留调用方错误类型并立即停止，不读取后续 data block。

Expected result:

- 为 latest-state reducer 提供有界、带 provenance 的 SST mutation stream
  foundation。
- Stage 5 的全部 properties/count oracle 保持不变。
- 常驻内存保持为 layout/index、单个解压 block 和当前 reconstructed key，
  不随 SST entry 总数线性增长。

Implemented validation:

- synthetic valid/invalid/edge tests 覆盖 value/delete/merge/range、空/反向
  range、external-SST global sequence、typed visitor stop、跨 block key
  regression 和四类独立预算。
- 代表 live SST `000146.sst` 以真实只读导出 fixture 验证：
  `148` 个 data block、`23,364` 条 entry、`420,609 / 298,145`
  raw key/value bytes，与 `inspect_sst` 的 count、sequence 和解压字节精确一致。
- Stage 6.2 当前只提供 parser foundation，尚未对全部 `35/40/33` live SST
  建立 entry digest 门禁，未在 source DB 持久化 raw key/value，也未合并
  SST/WAL latest state。range-only SST 仍 typed unsupported。

### Stage 6.3 - RocksDB Latest-State Reducer

Status: completed for the private PVE baseline.

Tasks:

- 按 column family、user key、sequence 和 value type 合并 live SST。
- 在 SST 基线后按 WAL number/record/mutation 顺序应用 WAL。
- delete、single delete 和 range delete 保留不同语义。
- merge operand 不在 wire 层解释；仅由批准的 Ceph merge operator 解码器处理。
- 对 dropped column family、重复 internal key、sequence regression 和
  comparator 不匹配 fail closed。
- latest state 使用外部排序或分区 spool，禁止把全部 OSD key/value
  常驻内存。
- spool 仅写入 case workspace，使用原子替换并带来源和 schema version。

Expected result:

- 得到可重复、可审计的 RocksDB logical latest-state stream。
- 相同证据重复导入产生相同 key digest、entry count 和 sequence boundary。

Implemented result:

- `inspect_sst_with_visitor` 在同一次 data-block 解压中同时完成 Stage 5 结构
  inventory、脱敏 census 与 mutation streaming，避免每个 SST 重读 payload
  block。
- 全部 SST/WAL point mutation 与 range tombstone 进入
  `staging/<dataSourceId>/ceph-rocksdb-recovery-*` 下的临时 SQLite spool；
  spool 失败或作用域结束后自动删除。
- spool 使用受限 schema、确定性 internal-key 顺序、单 writer 和每个 column
  family 独立 read-only recovery connection；无 range/merge 的 column family
  使用 borrowed point-only fast path。
- reducer 支持 value、delete、single-delete、range-delete，以及 Ceph `T`
  int64-array 和 `b` bitwise-XOR merge；未知 operator、重复 internal key、
  非递减 history、inactive column family 和资源上限均 typed fail closed。
- `source_011_ceph_latest_state.sql` 只保存每个 active column family 的 mutation
  分类计数、latest/deleted 决策计数、sequence boundary、sharding/point/range/
  latest-state SHA-256 和 `scan_complete=1`；不含 raw key/value。
- 三个 OSD 各产生 12 个 column-family summary。真实样本聚合 oracle：
  `b4f31e...22eed`、`0cf9b7...20880`、`32d7af...e978`。
- OSD、BlueFS、MANIFEST、SST、WAL 与 latest-state summary 在同一 source DB
  replacement transaction 中提交；任一步失败保留上一完整快照。

### Stage 6.4 - BlueStore Semantic Decode

Status: completed for `S/C/O/X` on the private PVE baseline.

Tasks:

- 固定并验证 Ceph key-space prefix 与 column-family sharding 映射；专用
  column family 的 logical key 不带 default-CF 的 `prefix + NUL` 包装。
- 先实现 `S/C/O/X`：
  super、collection、object onode、shared blob。
- `O` key 只直接恢复 `ghobject_t`；collection/PG membership 必须结合 `C`
  value 的 `bits` 与 collection containment 规则，不从 object key 猜测。
- 解码 onode size、attrs、extent-map shard、blob 和 logical extent。
- 解码 physical extent、compression、checksum、shared blob 引用。
- onode shard 必须闭合；缺失 shard、重叠 logical extent、越界 physical
  extent和未知 DENC version typed fail。
- `M/P/m/p` OMAP family 是 RBD directory/header metadata 的硬前置，必须作为
  Stage 6.4b 在 RADOS/RBD 前完成；`T/B/b/L` 再按读取需求分批实现。

Expected result:

- 可以从一个 OSD 构建 RADOS object 的逻辑大小和物理读取计划。
- 不直接把 BlueStore object 伪装成 POSIX 文件。

Implemented result:

- `ceph-wire` 按 pinned Ceph Squid `v19.2.3` 解码 `S/C/O/X`，覆盖
  ghobject key、onode、spanning/local blob、inline/sharded extent map、
  physical extent、checksum、use tracker 与 shared blob ref-map。
- 解码器设置独立 input、blob、extent、checksum、shared-ref 和 aggregate
  work budget；未知 DENC、截断、非 canonical key、长度溢出与不闭合 shard
  typed fail closed。
- `source_012_ceph_bluestore_semantics.sql` 保存规范化语义行。attrs 只保存
  value-byte count 与 SHA-256，checksum 按数值语义规范化，raw key/value、
  attrs value 和 checksum bytes 均不持久化。
- repository replacement 与 OSD/BlueFS/RocksDB/latest-state 使用同一事务；
  semantic snapshot 强制绑定 persisted latest-state digest、OSD inventory
  和 device bounds。
- 物理 extent 校验遵循 Ceph shared blob 语义：同一 blob 自重叠拒绝；
  不同 blob 只有在非零 shared ID 相同且其全部已分配范围被 `X` ref-map
  连续覆盖时才允许部分重叠。不同/空 shared ID 的重叠拒绝。
- 六成员真实样本于 2026-07-15 串行通过，三个 `disk02` 均为
  `ready_metadata`、普通文件行保持零，并固化以下 semantic oracle：
- 后续性能审计消除了 object-child count 的 `O(objects * children)` 重扫，
  checksum 归属改为排序 cursor 单次扫描，SQLite child rows 使用 bind-budget
  内的多行批量写入，并共享重复 identity 字符串。保留的真实 `server01`
  source DB phase benchmark 为 `51.58..88.60s / 395MB`；旧单成员全链路
  基线为 `486.17s / 705MB`，二者范围不同，不能直接计算端到端提速比例。
  精确 count/digest、单事务 replacement 和 fail-closed 校验保持不变；
  完整 E01 优化后复跑等待 `E:` 样本盘重新挂载。

| OSD | objects | blobs | shards | logical | physical | checksums | shared / refs | semantic SHA-256 |
|---|---:|---:|---:|---:|---:|---:|---|---|
| server01 | 2,924 | 116,135 | 18,971 | 116,487 | 134,148 | 1,839,658 | 23,316 / 27,897 | `794ab1ea6632d809bac456d9cd5e5e54c3a46b93977d2224f98c0d564a46c73b` |
| server02 | 2,927 | 116,135 | 18,970 | 116,487 | 134,154 | 1,839,666 | 23,316 / 27,900 | `441e1a48ec5ca51e5ff2caa94eac106d283d9375bbbc08d841196eb84fbe78e9` |
| server03 | 2,930 | 116,135 | 18,974 | 116,487 | 134,150 | 1,839,646 | 23,316 / 27,911 | `d5eb02ba6e77a66476a2c84f010bca75ec77d870858d15e6b57681fb075028bc` |

每个 OSD 另有 `34` 个 collection。当前 semantic snapshot 是后续 RADOS
读取计划的可信基础，不等同于对象内容、PG 副本或 RBD 镜像已恢复。

### Stage 6.5 - RADOS / PG Reconstruction

Tasks:

- 从 collection 和 object key 恢复 pool、PG、namespace、snap 和 object name。
- 按 Ceph FSID、pool、PG、object identity 跨三个 source DB 关联副本。
- replicated pool 首版要求至少一个完整、校验通过的副本。
- 对多个有效副本计算内容摘要并验证一致性。
- EC pool 在 profile、k/m 和 shard index 未闭合前保持 unsupported。
- 无 OSD map 时只输出已观察到的 local collection/object 与候选副本集合，
  不声明 intended replica count、acting set、primary 或 CRUSH placement。
- 若恢复 `osd_superblock` 与连续 offline OSDMap epoch，则结果必须绑定历史
  epoch，不能称为在线当前状态。

Expected result:

- 可按 object identity 提供稳定的只读 RADOS object reader。
- 副本不一致时保留来源和冲突，不静默选择。

### Stage 6.6 - RBD Image Reconstruction

Tasks:

- 从 RBD directory OMAP、双向 name/id 映射、`rbd_id.<name>` body、
  `rbd_header.<id>` OMAP 交叉发现镜像。
- 解析 image size、order、object prefix、features、striping unit/count。
- 将 RBD logical offset 映射为 object number 和 object-local range。
- 支持缺失对象的零填充语义，但只在 image size 内且 active parent overlap
  不要求回读 parent 时启用。
- unsupported feature、parent/clone、encryption、journal 或 snapshot
  必须显式暴露，不能默认为普通 head image。
- 实现 `RbdEvidenceReader: Read + Seek + EvidenceReader`，不生成完整磁盘副本。

Expected result:

- 每个受支持 RBD image 表现为独立只读虚拟块设备。
- 随机 range read 只读取覆盖范围内的必要 RADOS objects。

### Stage 6.7 - VM Partition / Filesystem Integration

Tasks:

- 将 `RbdEvidenceReader` 接入现有 volume detection。
- 复用 MBR/GPT、LVM、NTFS、EXT4、XFS、Btrfs 等 reader。
- VM data source 使用独立 source DB 和全局 `FileEntryId`。
- 文件预览继续走 evidence handle，不暴露临时宿主路径。
- VM 文件树、preview、Hex、文本和媒体走现有统一链路。
- cluster source 与 derived VM source 建立明确 lineage。

Expected result:

- 可以展开 PVE VM 的完整受支持文件系统树。
- 任意可读文件可以通过 FileEntryId 预览和提取。

### Stage 6.8 - CephFS Capability Decision

Tasks:

- 在 latest state 中检查 MDS metadata/data pool 的真实证据。
- 若样本只有 RBD，则文档明确 CephFS not present，不创建空入口。
- 若存在 CephFS，则另立设计覆盖 inode、dirfrag、backtrace、layout、
  snapshot realm 和 data object mapping。

Expected result:

- 产品不会把“Ceph 集群存储已恢复”误写为“CephFS 已恢复”。

## 测试矩阵

| 测试面 | Valid | Invalid | Edge / Real oracle |
|---|---|---|---|
| WriteBatch header | sequence/count | `<12 bytes`、count overflow | count 0、max 56-bit sequence |
| Mutation tag | put/delete/single/merge/range + CF | unknown/transaction/blob/wide | mixed default/CF |
| Length prefix | canonical varint32 | truncated/non-canonical/overflow | empty key/value、configured max |
| Count | exact mutation count | under/over declared | auxiliary records excluded |
| Sequence | monotonic non-overlap | regression/overlap/56-bit overflow | valid gaps、zero mutation batch |
| WAL log | full/fragmented/recyclable | CRC/fragment/log-number mismatch | block trailer、preallocated zero tail |
| WAL selection | active numbered files | malformed/duplicate/unknown CF | min-log boundary、legacy db、post-MANIFEST、number gaps |
| SST stream | value/delete/range/provenance | checksum/restart/type/external-seq | representative SST + full `35/40/33` live set |
| Latest state | SST + WAL + `T`/`b` merge | sequence regression/conflict/unknown CF/operator | 12 CF rows per OSD + deterministic aggregate digest |
| BlueStore | `S/C/O/X`、onode/blob/shard/checksum/shared ref-map | DENC、长度、shard closure、blob self-overlap、无关 shared overlap、ref-map coverage | 三 OSD 精确 semantic digest/count；replica comparison 尚未开始 |
| RADOS | replicated object | divergent replicas | missing-but-redundant OSD |
| RBD | head image/striping | unsupported feature/clone | first/middle/last object reads |
| VM block | MBR/GPT/LVM | short range/out of image | partition tail |
| VM tree | supported FS | unsupported/encrypted FS | arbitrary FileEntryId preview |

### 合成测试

- 每个 parser family 至少提供 valid / invalid / edge 三类。
- WriteBatch 测试必须覆盖全部已支持 tag 和全部 unsupported tag family。
- fuzz-like truncation 测试逐 byte 截断 header、varint、key 和 value。
- 任何 parser 错误后不得留下已提交 reducer 或 repository 状态。

### 真实样本测试

Stage 6.3 真实门禁：

1. 只读导出三个 `db.wal/*.log`，记录文件号、逻辑大小和 checksum。
2. native decoder 与 `ldb`/RocksDB recovery oracle 比较 batch、mutation、
   first/last sequence。
3. 保持六成员串行导入，三个 host 为 `ready`，三个 OSD 保持
   `ready_metadata`。
4. ordinary BlueStore `file_entries` 继续为零。
5. Stage 5 `35/40/33` live-SST 和聚合 entry/block 数不得变化。
6. 每个 OSD 必须持久化 12 个 active column-family summary，并验证 mutation
   分类恒等式、sequence boundary、`scan_complete` 和 aggregate digest。
7. raw key/value 不得出现在 source DB schema、日志、审计或普通文件树。

| OSD | WAL | 文件字节 | logical records | empty batches | mutations | payload bytes | sequence |
|---|---:|---:|---:|---:|---:|---:|---|
| server01 | 142 | 3,921,274 | 3,710 | 1,107 | 9,338 | 3,894,471 | 1,077,118..1,086,455 |
| server02 | 120 | 4,142,839 | 3,782 | 1,084 | 9,644 | 4,115,489 | 1,052,659..1,062,302 |
| server03 | 127 | 4,145,432 | 3,812 | 1,112 | 9,644 | 4,117,873 | 1,061,240..1,070,883 |

| OSD | latest-state aggregate SHA-256 |
|---|---|
| server01 | `b4f31e224ff485b29b1b3ac7c21e079344250bf37a954b304d43294b1da22eed` |
| server02 | `0cf9b7ead1e5953fa84f1c57a16be4f1a2d5fd4713d2ed1ad20cf8cf9d320880` |
| server03 | `32d7af9d9eda6ca168cb9a85a7b17a36c9fce012f9301b354aebb1b633bee978` |

Stage 6.4 在同一六成员门禁中追加：

1. 每个 `disk02` 必须存在且仅存在一个完整 semantic scan。
2. schema/profile 固定为当前 repository 常量，`profile_complete=true`。
3. semantic snapshot 的 `latest_state_sha256` 必须等于 persisted latest-state
   set digest，`semantic_sha256` 必须覆盖全部规范化行。
4. scan count 必须与 collection/object/blob/shard/logical/physical/checksum/
   shared/ref 向量长度精确闭合。
5. 三 OSD 的精确计数与 semantic SHA-256 必须匹配 Stage 6.4 oracle。
6. shared blob 的合法部分重叠与 ref-map union coverage 必须通过；同 blob
   自重叠、不同 shared ID 重叠、ref-map 缺口必须回滚失败。

RBD 门禁建立后：

- 精确断言发现的 image ID、名称、size、object order 和 feature flags。
- 读取每个镜像 offset `0`、对象边界、分区表、文件系统 superblock 和尾部。
- 与只读 `rbd export` 或 `rbd-nbd --read-only` 的 range digest 比较。
- 至少一个 VM 完成完整树统计和关键文件 FileEntryId 预览。

## 性能与资源预算

| 能力 | 默认预算 |
|---|---|
| 单 WriteBatch bytes | `<=16 MiB` |
| 单 batch mutations | `<=1,000,000` |
| 单 key | `<=1 MiB` |
| 单 value / operand | `<=64 MiB` |
| 单 WAL file | `<=64 MiB`，超过需显式提升并记录 |
| WAL 解析内存 | WAL 文件 bytes + 重组后的 logical-record bytes + O(mutation metadata)，受 file/record limits 约束；WriteBatch 不再复制 key/value |
| SST 访问 | 单解压 block 常驻，累计解压默认 `<=1 GiB` |
| latest-state | source-local 临时 SQLite spool；point `<=5,000,000`、range `<=500,000`、resident range bytes `<=64 MiB`、aggregate raw bytes `<=8 GiB`；range tombstone 全量装载与 coverage end-key 副本受独立常驻预算约束 |
| OSD IO | 串行 range read，不并发争用同一 E01 reader |
| RBD preview | 仅读取请求覆盖的 objects |

性能回归要求：

- Stage 6.1 不得让现有 Stage 5 六成员导入耗时退化超过 10%。
- Stage 6.3 首次增加全 live-set mutation spool 和约 50 万 mutation 的
  latest-state reduction，不能直接沿用 Stage 5 纯结构扫描的 `40.32s`
  绝对门槛。2026-07-14 本机 debug feature-adjusted baseline 为 `50.31s`；
  相比优化前约 `59.5s` 改善约 `15.4%`。后续变更不得在相同机器、相同串行
  配置和热缓存条件下退化超过 10%。
- Stage 6.4 首次持久化每 OSD 约 210 万条规范化 semantic child rows，不能
  沿用 Stage 6.3 的 digest-only `50.31s` 绝对门槛。2026-07-15 本机 debug
  串行六成员 baseline 为 `1757.04s`；单 `server01-disk02` 为 `486.17s`，
  观测峰值 RSS 约 `705MB`。完成批量写入、共享 identity、单次 child-count
  聚合和 checksum cursor 校验后，保留真实 source DB 的完整 semantic phase
  三次为 `51.58..88.60s / 395MB`。phase 门禁预算固定为
  query `60s`、validation `90s`、write `90s`、commit `30s`、peak RSS
  `512MB`；该 phase 结果不替代完整导入基线，完整 E01 优化后耗时和峰值待
  样本盘重新挂载后补录。所有性能结果必须保证 semantic digest 与行数 oracle
  不变。
- latest-state 和 BlueStore 阶段必须分别记录读取 bytes、解压 bytes、key
  count、spill bytes、wall time 和 peak memory。
- RBD 随机 1 MiB range read 不得线性扫描全部 OSD 或全部 object。

## 评估方案

| 维度 | 权重 | 通过标准 |
|---|---:|---|
| 格式正确性 | 20 | 固定 revision 源码与独立工具 oracle 一致 |
| 取证完整性 | 20 | 来源、sequence、replica、extent 可追溯 |
| 健壮性 | 15 | 所有长度、数量、版本、范围 fail closed |
| 模块化 | 15 | wire / semantic / service / repository 分离 |
| 测试可信度 | 20 | synthetic + real PVE + independent oracle |
| 性能 | 10 | 有界内存、按需 IO、满足回归阈值 |

总分低于 `90/100`、任一维度低于 `80%`，或存在未解决的 High/Critical
问题时，不得进入下一语义层。

每个 Phase 完成后必须执行：

- production file/function size review；
- parser error taxonomy review；
- raw key/value 生命周期 review；
- evidence read-only review；
- synthetic tests、real sample gate 和 workspace guard；
- 独立提交，禁止把未验证的下一层算法混入同一提交。

## 验收标准

### Stage 6.1

- WriteBatch header、count、sequence 和支持 tag 与 pinned RocksDB 源码一致。
- malformed batch 不会返回部分 mutation 集合。
- raw key/value 不发生第二份复制，不写日志和数据库。
- transaction/blob/wide/timestamp tag typed unsupported。
- tests 位于 `crates/rocksdb-wire/tests/`。
- `cargo fmt`、`cargo test -p rocksdb-wire`、clippy 和结构 guard 通过。

### Stage 6.2

- live SST 以 visitor 逐 block 流式读取，不建立整表 key/value 集合。
- data entry 与 range tombstone 使用不同 callback，raw slices 不越过 callback。
- point internal-key 顺序、range no-compression/restart、value type、external
  global sequence、properties count 和独立资源预算全部 fail closed。
- callback 带 raw internal key、block/entry ordinal；输出在最终 properties
  校验前属于 provisional，只能进入可回滚 spool。
- visitor error 不被字符串化，且不会继续读取后续 block。
- 代表 `000146.sst` 的 `148 / 23,364 / 420,609 / 298,145` oracle 精确闭合。
- `inspect_sst`、Stage 5 `35/40/33` live-SST inventory、Stage 6.1 WAL 和
  `ready_metadata` 状态保持不变。
- 当前验收等级为 parser foundation；全 live-set digest、range-only SST 和
  reducer publish/rollback 门禁完成前，不进入 semantic completion。

### Stage 6.3

- 全部 `35/40/33` live SST 与 active WAL 通过统一 spool 进入 reducer。
- value/delete/single-delete/range-delete 与批准的 `T`/`b` merge 语义闭合；
  unknown operator 和 malformed history typed fail closed。
- 每个 OSD 精确产生 12 个 active column-family summary，aggregate digest 与
  私有真实样本 oracle 一致。
- source DB 只保存 digest-only summary，不保存 raw RocksDB key/value。
- disposable spool 位于 case staging，完成或失败后删除；读取证据保持只读。
- OSD/BlueFS/MANIFEST/SST/WAL/latest-state replacement 原子提交。
- 六成员真实回归通过，三个 host `ready`、三个 OSD `ready_metadata`、普通
  BlueStore `file_entries=0`。
- 当前完成的是可验证 logical latest-state summary，不是 BlueStore
  onode/blob/value semantic decode；Stage 6.4 前不得宣称对象或 VM 磁盘恢复。

### Stage 6.4

- `S/C/O/X` latest values 全部进入有界、typed、source-local semantic decode。
- onode/blob/shard/logical/physical/checksum/shared ref-map 规范化行与 scan
  count、latest-state digest、semantic digest 精确闭合。
- shared blob 部分重叠仅在相同非零 shared ID 且 ref-map 完整覆盖时接受。
- 三 OSD 精确 semantic count/digest oracle 通过，普通 `file_entries=0`。
- raw RocksDB key/value、attrs value、checksum bytes 不进入 source DB、
  日志、审计或报告。
- 当前仍不得宣称 RADOS object content、PG/replica、OMAP、RBD、VM 文件树
  或 CephFS 已恢复。

### 最终重建目标

- 三个 OSD 可以恢复可验证的 RocksDB latest state。
- BlueStore onode/blob/extents 可以生成稳定的 RADOS object reader。
- replicated object 的副本一致性经过校验。
- 样本中的 RBD image 可以作为只读虚拟块设备随机读取。
- 受支持 VM 文件系统可完整展开文件树并按 FileEntryId 预览任意文件。
- 不生成或修改原始证据，不依赖生产 mock，不泄露宿主路径。
- CephFS 只有在样本证据与独立测试闭合后才标记为 supported。
