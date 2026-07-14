# Ceph BlueStore Stage 6 设计

**开发基线**: `2f541a7e`
**设计日期**: 2026-07-14
**当前实现切片**: RocksDB WAL / WriteBatch 恢复基础
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

本次首个实现切片交付 active WAL 选择、物理日志恢复、WriteBatch wire
decoder 和 source-local metadata 持久化。它不持久化 raw key/value，不解释
BlueStore 结构，也不改变三个 `disk02` 数据源的 `ready_metadata` 状态。

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
Ceph Reef v19.2.3
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

### 本阶段不做

- 不在首个切片中持久化 RocksDB logical key/value。
- 不在首个切片中实现 merge operator。
- 不在首个切片中解析 BlueStore onode/blob。
- 不在首个切片中生成普通 `file_entries`。
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
- range tombstone 走独立 visitor。
- raw key/value 只在 callback 生命周期内有效。
- caller 可按 column family 和批准的 BlueStore prefix 过滤。
- 不建立全 SST key/value vector。
- 保留 checksum、compression、restart 和 count 的现有校验。

Expected result:

- 为 latest-state reducer 提供已验证的 SST mutation stream。
- Stage 5 的全部 properties/count oracle 保持不变。

### Stage 6.3 - RocksDB Latest-State Reducer

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

### Stage 6.4 - BlueStore Semantic Decode

Tasks:

- 固定并验证 Ceph key-space prefix 与 column-family sharding 映射。
- 先实现 `S/C/O/X`：
  super、collection、object onode、shared blob。
- 解码 object key 为 collection/PG/ghobject identity。
- 解码 onode size、attrs、extent-map shard、blob 和 logical extent。
- 解码 physical extent、compression、checksum、shared blob 引用。
- onode shard 必须闭合；缺失 shard、重叠 logical extent、越界 physical
  extent和未知 DENC version typed fail。
- `T/B/b/L/M/P/m/p` 后续按读取需求分批实现，不一次性扩大攻击面。

Expected result:

- 可以从一个 OSD 构建 RADOS object 的逻辑大小和物理读取计划。
- 不直接把 BlueStore object 伪装成 POSIX 文件。

### Stage 6.5 - RADOS / PG Reconstruction

Tasks:

- 从 collection 和 object key 恢复 pool、PG、namespace、snap 和 object name。
- 按 Ceph FSID、pool、PG、object identity 跨三个 source DB 关联副本。
- replicated pool 首版要求至少一个完整、校验通过的副本。
- 对多个有效副本计算内容摘要并验证一致性。
- EC pool 在 profile、k/m 和 shard index 未闭合前保持 unsupported。
- 不依赖在线 monitor/OSD map；缺失必要 map 时只输出可证明的 object 集合。

Expected result:

- 可按 object identity 提供稳定的只读 RADOS object reader。
- 副本不一致时保留来源和冲突，不静默选择。

### Stage 6.6 - RBD Image Reconstruction

Tasks:

- 从 RBD directory、id、header 和 metadata object 发现镜像。
- 解析 image size、order、object prefix、features、striping unit/count。
- 将 RBD logical offset 映射为 object number 和 object-local range。
- 支持缺失对象的零填充语义，但只在 RBD metadata 证明该范围合法时启用。
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
| SST stream | value/delete/range | checksum/restart/type | all `35/40/33` live SST |
| Latest state | SST + WAL | sequence regression/conflict | deterministic digest |
| BlueStore | onode/blob/shard | DENC version/extent overlap | three OSD replica comparison |
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

首轮真实门禁：

1. 只读导出三个 `db.wal/*.log`，记录文件号、逻辑大小和 checksum。
2. native decoder 与 `ldb`/RocksDB recovery oracle 比较 batch、mutation、
   first/last sequence。
3. 保持六成员串行导入，三个 host 为 `ready`，三个 OSD 在
   Stage 6.1 仍为 `ready_metadata`。
4. ordinary BlueStore `file_entries` 继续为零。
5. Stage 5 `35/40/33` live-SST 和聚合 entry/block 数不得变化。

| OSD | WAL | 文件字节 | logical records | empty batches | mutations | payload bytes | sequence |
|---|---:|---:|---:|---:|---:|---:|---|
| server01 | 142 | 3,921,274 | 3,710 | 1,107 | 9,338 | 3,894,471 | 1,077,118..1,086,455 |
| server02 | 120 | 4,142,839 | 3,782 | 1,084 | 9,644 | 4,115,489 | 1,052,659..1,062,302 |
| server03 | 127 | 4,145,432 | 3,812 | 1,112 | 9,644 | 4,117,873 | 1,061,240..1,070,883 |

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
| SST 访问 | 单解压 block 常驻 |
| latest-state | 外部排序/spool，有界内存 |
| OSD IO | 串行 range read，不并发争用同一 E01 reader |
| RBD preview | 仅读取请求覆盖的 objects |

性能回归要求：

- Stage 6.1 不得让现有 Stage 5 六成员导入耗时退化超过 10%。
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

### 最终重建目标

- 三个 OSD 可以恢复可验证的 RocksDB latest state。
- BlueStore onode/blob/extents 可以生成稳定的 RADOS object reader。
- replicated object 的副本一致性经过校验。
- 样本中的 RBD image 可以作为只读虚拟块设备随机读取。
- 受支持 VM 文件系统可完整展开文件树并按 FileEntryId 预览任意文件。
- 不生成或修改原始证据，不依赖生产 mock，不泄露宿主路径。
- CephFS 只有在样本证据与独立测试闭合后才标记为 supported。
