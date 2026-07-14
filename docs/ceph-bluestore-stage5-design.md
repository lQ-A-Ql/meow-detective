# Ceph BlueStore Stage 5 设计

**基线提交**: `82f98823`
**目标日期**: 2026-07-14
**范围**: E01 -> LVM LV -> BlueFS live SST -> RocksDB block-based table inventory
**实现状态**: 2026-07-14 已通过六成员私有 PVE 串行门禁；独立提交待最终工程门禁完成

## Summary

Stage 4 已通过 RocksDB `CURRENT`、`IDENTITY` 和活动 MANIFEST 的确定性回放，
恢复三个真实 OSD 的 column family 与 live SST 集合。Stage 5 在相同只读证据
边界内验证每个 live SST 的 BlueFS 身份和物理结构，解析 block-based table
footer、block trailer、metaindex、properties 与 index，产出 source-local SST
结构和有界键空间统计。

本阶段不等同于 RocksDB 数据库恢复或 Ceph object reconstruction：

- 不调用 RocksDB runtime 打开、恢复、repair 或 compact 数据库；
- 不解析 WAL `WriteBatch`，不合并 memtable、SST 和删除记录为最新状态；
- 不解释 BlueStore onode/blob/value，不重建 RADOS object、PG、RBD 或 VM disk；
- 不保存 RocksDB raw key、internal key、value 或未经裁定的键前缀明文；
- 不把 BlueStore metadata 写入普通 `file_entries`；
- 数据源状态继续保持 `ready_metadata`。

## 开发基线

### 上游格式基线

样本 Ceph 使用 RocksDB revision：

```text
9fa4990159853479a222244574ca41202e4c95c1
```

实现以该 revision 的下列源码作为格式事实源：

- `table/format.h`、`table/format.cc`：footer、`BlockHandle`、checksum；
- `table/block_based/block.cc`：prefix-compressed entry 与 restart array；
- `table/block_based/block_based_table_builder.cc`：block trailer 和 footer 写入；
- `table/meta_blocks.cc`：metaindex、properties block 与属性编码；
- `include/rocksdb/compression_type.h`、`util/compression.h`：compression ID
  与 LZ4 format-version 2 framing；
- `db/dbformat.h`：internal key 的 sequence/value-type trailer；
- Ceph `BlueStore.cc`、`bluestore_types.h`、`RocksDBStore.cc` 和
  `KeyValueDB.h`：BlueStore key-space 与 column-family 语义。

当前样本格式基线：

```text
table magic       0x88e241b785f4cff7
footer length     53 bytes
format version    5
compression       LZ4
comparator        leveldb.BytewiseComparator
```

任何不在已验证格式集合中的 magic、footer version、checksum、需要解码的
compression、index encoding 或 table feature 必须 typed fail closed，不允许猜测。

### Stage 4 真实样本 Oracle

| Member | Live SST | Identity | Last sequence |
|---|---:|---|---:|
| `server01-disk02` | 35 | `318c61d3-7d8b-497a-b02a-d3683123595d` | 1077117 |
| `server02-disk02` | 40 | `15f9cf98-cb4f-4d78-9d94-ae6235eb075b` | 1052658 |
| `server03-disk02` | 33 | `8024bc80-69cc-4adc-9f00-364b295f5312` | 1061239 |

代表性 server01 SST 独立 `sst_dump` oracle：

| SST | CF | Data blocks | Entries | Deletes | Raw key | Raw value |
|---|---|---:|---:|---:|---:|---:|
| `000146` | `default` | 148 | 23,364 | 0 | 420,609 | 298,145 |
| `000137` | `O-0` | 192 | 1,553 | 114 | 128,842 | 696,459 |
| `000141` | `P` | 696 | 12,545 | 1,502 | 599,075 | 2,574,877 |

`000140.sst` 属于 column family `L`，包含 5 条 deletion entry。该事实只作为
entry type/count 回归 oracle；Stage 5 不持久化其 raw key。

## 架构与开发边界

```text
rocksdb-wire/
  sst/
    footer.rs
    block_handle.rs
    block.rs
    compression.rs
    properties.rs
    index.rs
    inventory.rs

app-services/import_pipeline/
  ceph_bluefs_file_reader.rs
  ceph_rocksdb_sst_locator.rs
  ceph_rocksdb_sst_inventory.rs
  ceph_rocksdb_sst_records.rs

persistence-sqlite/
  migrations/scripts/source_009_ceph_sst_inventory.sql
  repositories/ceph_rocksdb_sst_repo.rs
```

依赖方向固定为：

```text
rocksdb-wire <- app-services -> persistence-sqlite
ceph-wire    <- app-services
```

- `rocksdb-wire` 只解析调用方提供的 byte ranges，不依赖 Ceph、SQLite、
  evidence reader、Tauri 或应用服务。
- `app-services` 负责将 MANIFEST live file 关联到 BlueFS path/fnode/extents，
  执行按需只读 range IO，并将 wire facts 映射为 repository records。
- `persistence-sqlite` 只校验和持久化结构化记录，不解析 SST bytes。
- Tauri command 和前端不参与 Stage 5 计算，不新增生产 mock。

## Stage Design

### Stage 5.1 - SST Wire Foundation

#### Phase 5.1.1 - Footer 与 BlockHandle

Tasks:

- 解码 53-byte new footer：
  `checksum:u8 + metaindex handle + index handle + zero padding +
  formatVersion:u32LE + magic:u64LE`；
- `BlockHandle` 只接受 canonical varint64 offset/size；
- footer 必须位于精确文件尾，magic 必须为 block-based table magic；
- 首版只接受 format version `5`，其他版本 typed unsupported；
- handle range 必须在 footer 前结束，并为 block trailer 预留 5 bytes；
- metaindex/index handle 不允许重叠 footer、溢出或引用空范围。

Expected result:

- 可从固定 53 bytes 得到稳定 footer facts；
- truncated、non-canonical、错误 magic/version、非零 padding 和越界 handle
  在任何 block 读取前失败。

#### Phase 5.1.2 - Block trailer 与 checksum

Tasks:

- trailer 固定为 `compressionType:u8 + checksum:u32LE`；
- 支持 checksum type `CRC32C`、`xxHash32` 和 `xxHash64-low32` 的结构识别；
- 首版生产读取只启用已由样本验证的 checksum 算法；
- checksum 覆盖 stored block bytes 和 compression type；
- 需要解码的 block compression ID 只接受 `none`、`LZ4`、`LZ4HC`；
- LZ4 format-version 2 先解析 canonical varint32 uncompressed length，再执行
  有上限解压。

Expected result:

- checksum 在解压前校验；
- 损坏 trailer、未知算法、已进入解码路径的未知 compression、解压长度放大和
  短解压均 typed fail closed；未知辅助 meta block 只校验物理 checksum，不解释
  其 compression 或内容。

### Stage 5.2 - Restart Block Parser

#### Phase 5.2.1 - 通用 entry 解码

Tasks:

- 解码 `shared/nonShared/valueLength` canonical varint32；
- 校验 shared 长度不超过上一 key；
- 解析 restart count 与严格递增的 restart offsets；
- 每个 restart entry 必须 `shared=0`；
- 对 entry 数、key/value 长度、解压后 block 大小设置独立上限；
- 输出借用或受限复制后的 key/value，不保留跨 block 全量数据。

Expected result:

- metaindex、properties 和普通 data block 共享一套安全 parser；
- malformed restart、截断 entry、前缀越界和内存放大均在局部 block 内失败。

#### Phase 5.2.2 - Internal key metadata

Tasks:

- data key 至少包含 8-byte internal-key trailer；
- 解码 56-bit sequence 与 value type；
- 只统计 value/deletion/merge/range-deletion 等受支持类型；
- 不向 parser 返回或持久化 raw user key；
- 未知 value type typed unsupported，避免错误分类取证记录。

Expected result:

- 可生成确定性 entry-type 计数和 sequence 范围；
- raw key/value 生命周期限制在单 block 解析调用内。

### Stage 5.3 - Metaindex、Properties 与 Index

#### Phase 5.3.1 - Metaindex

Tasks:

- 读取 footer 指向的 metaindex block；
- 仅提取 `rocksdb.properties`、旧 `rocksdb.stats`、compression dictionary、
  range deletion 和 filter 相关 handle；
- 重复语义 handle、非法 path-like key 或 handle 越界均失败；
- 未识别 meta block 只记录计数，不保存任意 key/value。

#### Phase 5.3.2 - Properties

Tasks:

- 解析 RocksDB predefined numeric/string properties；
- 固定提取：
  `numDataBlocks/numEntries/deletedKeys/mergeOperands/numRangeDeletions`、
  `rawKeySize/rawValueSize/dataSize/indexSize/filterSize`、
  `formatVersion/compression/comparator/columnFamilyName/columnFamilyId`、
  `originalFileNumber/dbId/dbSessionId`；
- numeric property 必须为 canonical varint64 且无尾随 bytes；
- string property 必须有效 UTF-8、无 NUL 且满足长度上限；
- user-collected properties 不持久化，只记录忽略数量。

#### Phase 5.3.3 - Index

Tasks:

- 解码 index entry 指向的数据 block handle；
- 支持样本 format version 5 的 delta-encoded index value；
- handle 必须按物理 offset 严格递增、不重叠，并位于 meta/footer 之前；
- index entry count 必须与 properties 的 data-block count 一致；
- 首尾 index internal key 只保留长度、sequence/value-type 和稳定摘要，
  不保存 raw key。

Expected result:

- 每个 SST 获得可验证的数据 block 列表；
- properties、index 和文件物理范围三方不一致时不产生部分 inventory。

### Stage 5.4 - BlueFS Live-SST Integration

Tasks:

- MANIFEST live file number 精确映射为 `db/<file-number>.sst`，文件号最少补齐
  六位，超过六位时不得截断；
- path 必须存在于同一 replay snapshot，且只能关联一个 fnode；
- fnode logical size 必须等于 MANIFEST file size；
- 使用 BlueFS extent reader 分别读取 footer、meta/index/properties 和必要的
  data block ranges，不整文件加载；
- 单 SST stored block 上限 `16 MiB`，解压后 block 上限 `64 MiB`；
- 单 OSD SST 数上限与 MANIFEST replay 上限一致；
- SST 按 `(columnFamilyId, level, fileNumber)` 串行读取，禁止并发争用同一个
  E01 reader；
- 任一 live SST 缺失、重复、大小不符或结构损坏时 Stage 5 aggregate 回滚。
- 首版不解析没有 data block 的 range-tombstone-only SST；此类合法但超出当前
  inventory 模型能力的表必须返回 typed `Unsupported`，不得伪报为结构损坏。

Expected result:

- Stage 4 的 live set 与 BlueFS 实体逐项闭合；
- IO 与实际读取 block 大小线性相关，不扫描 OSD 或读取 inactive SST。

### Stage 5.5 - Source-local Persistence

新增 `source_009_ceph_sst_inventory`，至少保存：

- inventory、column family、level、file number 和 BlueFS path identity；
- validated file size、固定 16 位十六进制 table magic、footer format、
  checksum type；
- metaindex/index offset 与 size；
- data block count、entry/deletion/merge/range-deletion count；
- raw key/value、data/index/filter size；
- compression、comparator、column family name、original file number；
- DB identity/session identity；
- bounded key-space summary version 和脱敏统计。

Tasks:

- 表使用 `(inventory_id, file_number)` 主键并关联 Stage 4 live file；
- repository 校验 properties 与 MANIFEST 的 CF/file/size/sequence 关系；
- OSD、BlueFS、MANIFEST 和 SST inventory 在同一 source DB transaction 原子
  替换；
- 不允许 raw key、internal key 或 value 字段进入 schema；
- 重导必须删除已不再 live 的旧 SST inventory。

Expected result:

- 相同 file number 在不同 source DB 中互不冲突；
- 任一 record 校验或写入失败回滚整个 OSD aggregate。

### Stage 5.6 - 有界键空间统计

stage_design:

键空间统计只服务于后续 BlueStore 语义解析的风险评估，不在本阶段解释 object
value。精确 Ceph prefix/column-family 映射必须以对应 Ceph revision 上游符号和
真实样本双重确认后定版。

Tasks:

- 从每个 data block 只计算 entry type、user-key length、首字节分类和稳定
  prefix bucket；
- 只持久化批准 prefix 的名称、计数和长度范围；
- 未批准或未知 prefix 统一计入 `unknown`，不保存原始字节；
- 每 SST 设置最大扫描 entries 和最大累计解压 bytes；
- MANIFEST properties 已知 entry 数超限时在读取 data block 前 fail closed；
- 累计解压 bytes 超限时立即停止后续 block 读取并 fail closed，不持久化部分摘要；
- 不解析 onode/blob/value，不跨 SST 合并 latest state。

Expected result:

- 可以判断后续 BlueStore decoder 需要覆盖哪些已验证 key family；
- Stage 5 输出仍是结构与统计 inventory，不是 RADOS object 结果。

### Stage 5.7 - Real-sample Gate 与工程复审

Tasks:

- 六成员 PVE runner 继续保持 `max_import_workers=1`；
- 三个 OSD 精确断言 live SST 数 `35/40/33`；
- 三个 OSD 精确断言聚合 data block / entry 数分别为
  `9994/159439`、`10152/160791`、`9954/158744`；
- 每个 live SST 均存在唯一结构 inventory，且 ordinary `file_entries=0`；
- server01 对代表 SST 核对独立 `sst_dump` properties；
- server02/server03 至少核对 footer/version/compression、SST 总数和聚合
  entry/data-block 数；
- 保存 Stage 5 独立真实样本回归文档；
- 运行 module/function/test-layout、dependency、doc、PVE 和 workspace gates；
- 按架构、模块化、契约、健壮性、测试、性能六维复审，不合格立即整改。

Expected result:

- parser 自测、repository 原子性和真实样本 oracle 三层证据闭环；
- 文档继续明确 WAL、latest-state、BlueStore object、RADOS/RBD/VM 为
  unsupported。

## 测试矩阵

| 测试面 | Valid | Invalid | Edge / Oracle |
|---|---|---|---|
| Footer | v5 block-based | magic/version/padding | min size、max varint handle |
| Handle | canonical offset/size | overflow/non-canonical | footer boundary |
| Trailer | supported checksum | corrupt/unknown | empty stored block |
| Compression | none/LZ4/LZ4HC | bomb/truncated/unknown | exact output limit |
| Restart block | prefix entries | bad restart/shared | empty/single/max entries |
| Internal key | value/delete/merge/range | short/unknown type | seq 0/max 56-bit |
| Metaindex | properties/filter handles | duplicate/out-of-range/checksum | unknown block 内容不解释但物理 checksum 必须有效 |
| Properties | numeric/string fields | malformed varint/UTF-8 | optional fields absent |
| Index | full/delta handles | overlap/order/count | first/last data block |
| BlueFS routing | minimum-width SST number | missing/duplicate/size mismatch | 七位以上 file number、multi-extent SST |
| Persistence | replace/cascade | invariant failure rollback | same number cross-source |
| Isolation | three source DBs | cross-inventory FK | re-import removes stale rows |
| Real PVE | 35/40/33 SST | any incomplete OSD | representative `sst_dump` |
| Cluster | 3 host + 3 OSD | member-local failure | ready + ready_metadata |

## 性能与资源标准

- 不加载完整 SST；固定 range 读取 footer、meta/index/properties 和当前 data block。
- 单 stored block `<=16 MiB`，单解压 block `<=64 MiB`。
- 常驻 compression dictionary `<=16 MiB`，单块 entry/restart 数
  `<=100,000`。
- Census entry 上限在 data-block IO 前由 properties 预检；累计解压上限在每个
  block 后立即检查，超限不继续扫描。
- 默认每次只保留一个解压 block；不建立 RocksDB block cache。
- 每个 SST 只构建一次 BlueFS logical-extent 索引，range 定位使用二分边界，
  并拒绝同一文件内部物理 extent 重叠。
- 导入取消状态在每次 SST range 读取前检查，取消后不进入 source DB 提交。
- SST 串行读取，SQLite 使用单 source-local transaction 和 prepared statements。
- Stage 5 相比 Stage 4 的增量耗时按实际 SST block 数评估；当前真实样本目标：
  单 OSD结构 inventory `<=15s`，六成员总门禁增量 `<=45s`。
- 峰值额外内存目标 `<=128 MiB`，不得与 OSD 或全部 SST 总大小线性增长。
- 记录每成员读取 bytes、解压 bytes、SST/block/entry 数和耗时，用于后续回归。

## 评估方案

| 维度 | 权重 | 通过标准 |
|---|---:|---|
| 架构边界 | 20 | wire/service/repository 分离，无 Tauri/SQLite 反向依赖 |
| 取证健壮性 | 20 | checksum、范围、格式、预算全部 fail closed |
| 数据契约 | 15 | MANIFEST/BlueFS/properties/source DB identity 一致 |
| 模块化 | 15 | 文件/函数符合门禁，无上帝模块和测试正文回流 |
| 测试可信度 | 20 | synthetic valid/invalid/edge + independent real oracle |
| 性能 | 10 | IO/内存有界并满足真实样本阈值 |

总分低于 `90/100`、任一维度低于 `80%` 或存在 High/Critical 未解决问题时，
Stage 5 不得标记完成。

## 验收标准

- 三个真实 OSD 的 `35/40/33` 个 live SST 均完成 BlueFS identity、size、
  footer、checksum、properties 和 index 验证。
- 代表 SST 的 data block、entry/deletion/raw-size 属性与独立
  `sst_dump` oracle 一致。
- 所有读取来自已有 read-only evidence reader，不调用 RocksDB/Ceph 写路径。
- 不整文件加载，不保存 raw key/value，不创建 ordinary file tree。
- OSD/BlueFS/MANIFEST/SST inventory 原子提交到独立 source DB。
- `ready_metadata`、source isolation 和六成员串行导入语义不变。
- WAL `WriteBatch`、RocksDB latest state、BlueStore onode/blob、
  RADOS/PG/RBD/VM reconstruction 继续明确 unsupported。
- Rust 全量门禁、结构守卫、文档守卫和真实 PVE 门禁全部通过。
