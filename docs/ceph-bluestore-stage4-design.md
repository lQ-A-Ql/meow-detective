# Ceph BlueStore Stage 4 设计

**基线提交**: `15904d97`
**目标日期**: 2026-07-13
**范围**: E01 -> LVM LV -> BlueFS file read -> RocksDB MANIFEST inventory

## Summary

Stage 3 已完成 BlueFS transaction log 的有界回放，并恢复 `db/CURRENT`、
`db/IDENTITY`、活动 MANIFEST、WAL 与 SST 的文件名、fnode 和 extent
metadata。Stage 4 在相同只读证据边界内读取 RocksDB 控制文件，解析活动
MANIFEST 的 log framing 与 `VersionEdit`，产出 source-local RocksDB
control-plane inventory。

本阶段明确不是 RocksDB 内容或 Ceph object reconstruction：

- 不解析 SST block、internal key/value 或 table properties；
- 不解析 WAL `WriteBatch`；
- 不调用 RocksDB library 打开数据库，不挂载或 repair BlueFS；
- 不恢复 BlueStore `PREFIX_OBJ`、PG、RADOS object、RBD 或 VM disk；
- 不把 BlueFS/RocksDB metadata 写入普通 `file_entries`；
- 不在 SQLite 保存任意 RocksDB internal-key 明文。

## 开发基线

### 上游格式依据

Ceph Reef bundled RocksDB 子模块 `9fa4990...` 自报版本为 `7.9.2`，并包含
PR3488 pseudo-NewFile 兼容补丁。实现以该 Ceph bundled revision 为样本
兼容基线，同时用 upstream RocksDB `v7.10.2`
(`3258b5c3e2488464de0827343c8c27bc6499765e`) 核对相邻稳定格式。两者不一致
处必须显式兼容或 typed unsupported，不能把 7.10.2 行为直接假定为 Reef
行为。

格式事实源：

- `facebook/rocksdb/db/log_format.h`：32 KiB block、physical record header、
  FULL/FIRST/MIDDLE/LAST 与 recyclable record type；
- `facebook/rocksdb/db/log_reader.cc`：CRC32C、fragment reassembly、trailer
  和损坏记录处理；
- `facebook/rocksdb/db/version_edit.h/.cc`：VersionEdit tag、column family、
  deleted/new file 与 `NewFile4` custom field；
- `facebook/rocksdb/file/filename.cc`：`CURRENT` 内容和 MANIFEST 文件名规则；
- `ceph/ceph/src/os/bluestore/BlueRocksEnv*`：RocksDB 文件通过 BlueFS
  environment 映射到 `db` / `db.wal`。

实现必须独立解码 wire metadata，不链接 RocksDB，不复用会写入、恢复或
打开数据库的上游 runtime。

### Stage 3 真实样本基线

| Member | BlueFS files | CURRENT | Active MANIFEST |
|---|---:|---|---|
| `server01-disk02` | 44 | `db/CURRENT` | `db/MANIFEST-000143` |
| `server02-disk02` | 49 | `db/CURRENT` | `db/MANIFEST-000121` |
| `server03-disk02` | 42 | `db/CURRENT` | `db/MANIFEST-000128` |

只读 WSL `ldb manifest_dump --verbose` 与
`ldb list_live_files_metadata --sort_by_filename` 的独立 oracle：

| Member | Identity | Edits | CFs | Live SST | Next file | Last sequence | Min log |
|---|---|---:|---:|---:|---:|---:|---:|
| `server01-disk02` | `318c61d3-7d8b-497a-b02a-d3683123595d` | 39 | 12 | 35 | 148 | 1077117 | 127 |
| `server02-disk02` | `15f9cf98-cb4f-4d78-9d94-ae6235eb075b` | 39 | 12 | 40 | 126 | 1052658 | 105 |
| `server03-disk02` | `8024bc80-69cc-4adc-9f00-364b295f5312` | 39 | 12 | 33 | 132 | 1061239 | 110 |

三个样本的 previous log number 均为 `0`，maximum column-family ID 均为
`11`。`manifest_dump` 最终 DebugString 中可见的 SST 行数分别为
`10/8/12`，但该输出不是完整 live-file 枚举；RocksDB
`list_live_files_metadata` 返回 `35/40/33`，并与 BlueFS 导出的 SST 文件集合
逐项一致。因此真实样本断言以 `list_live_files_metadata` 为 live-set oracle，
保留 DebugString 行数只作诊断，不用于判定 parser 正确性。

column families 为：

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

## 架构边界

```text
ceph-wire
  BlueFS superblock and transaction wire only

rocksdb-wire
  physical log framing
  VersionEdit decode
  deterministic manifest replay

app-services/import_pipeline
  BlueFS extent-backed file reader
  CURRENT / IDENTITY validation
  active MANIFEST selection
  RocksDB inventory record mapping

persistence-sqlite
  source_008 migration
  source-local RocksDB repository
  OSD aggregate atomic replacement
```

依赖方向固定为：

```text
rocksdb-wire <- app-services -> persistence-sqlite
ceph-wire    <- app-services
```

`rocksdb-wire` 不依赖 Ceph、SQLite、Tauri、evidence reader 或 application
service。`persistence-sqlite` 不解析 wire bytes。Tauri command 和前端不参与
Stage 4 解析与计算。

## Stage Design

### Stage 4.1 - RocksDB Wire Decoder

#### Phase 4.1.1 - Physical log framing

Tasks:

- block size 固定为 `32768` bytes；
- standard header 解码 `masked_crc32c:u32 + length:u16 + type:u8`；
- recyclable header 额外解码 `log_number:u32`；
- 支持 FULL/FIRST/MIDDLE/LAST 与 recyclable 对应类型；
- CRC 覆盖 record type、可选 recyclable log number 和 payload；
- fragmented records 只允许 FIRST -> zero-or-more MIDDLE -> LAST；
- block trailer、zero record 和 truncated tail 按 RocksDB reader 规则处理；
- manifest 文件上限 `64 MiB`、单 logical record 上限 `16 MiB`、logical
  record 数量上限 `1_000_000`。

Expected result:

- synthetic standard/recyclable、single/fragmented、多 block fixtures 可精确
  解码；
- CRC、非法 fragment sequence、跨 block header、length overflow 与超限
  全部 typed fail closed；
- decoder 不为声明的超大 payload 预分配内存。

#### Phase 4.1.2 - VersionEdit decode

Tasks:

- 支持 comparator、log number、previous log、next file、last sequence、
  minimum log to keep、maximum column-family ID；
- 支持 column-family selector/add/drop；
- 支持 deleted file 和 NewFile/NewFile2/NewFile3/NewFile4；
- NewFile4 custom field 只跳过被 upstream safe-ignore mask 允许的 length-
  prefixed field；未知 mandatory field typed unsupported；
- internal key 只验证最小结构并记录长度，不保存原始 bytes；
- 每个 edit tag 数量、字符串和 custom field 设独立上限。

Expected result:

- parser 能消费 Ceph Reef 活动 MANIFEST；
- malformed varint、未知 mandatory tag、重复非法字段和截断记录不会产生
  部分 inventory；
- crate 输出 wire facts，不输出 SQLite record。

### Stage 4.2 - Deterministic Manifest Replay

Tasks:

- replay state 维护当前 column-family selector；
- `ColumnFamilyAdd` 创建 ID/name，`ColumnFamilyDrop` 标记 dropped；
- deleted file 以 `(column_family_id, level, file_number)` 删除；
- new file 以同一 identity 替换/插入 live SST metadata；
- 保存最终 comparator、log/previous/min-log、next file、last sequence、
  max column-family ID；
- logical edit ordinal、column family 和 live SST 输出稳定排序；
- 冲突、缺失 column family 或无效 level/file number typed fail closed。

Expected result:

- 同一 MANIFEST 重放结果确定；
- inventory 只描述最终 live version，不保留任意 key bytes；
- server01 结果与 `ldb manifest_dump` 的控制字段及
  `list_live_files_metadata` 的最终 live set 一致。

### Stage 4.3 - Bounded BlueFS File Reader

Tasks:

- 从 Stage 3 replay snapshot 按精确 path 解析 fnode；
- 仅允许读取当前 single shared bdev；
- 每个 extent 校验 device ID、非零长度、物理范围、保留区和 overflow；
- logical read 以 `fnode.size` 为 EOF，不能读取 allocated padding；
- Stage 4 单控制文件上限固定为 `64 MiB`；
- 非零 BlueFS content encoding typed unsupported，不猜测压缩语义；
- `CURRENT`、`IDENTITY` 和 MANIFEST 分别使用最小必要读取。

Expected result:

- 不扫描整个 LV，物理 IO 只与三个控制文件的 extent 大小线性相关；
- corrupted extent、跨设备引用、size 大于 allocated bytes 和短读在解析前
  失败；
- reader 只借用已有 read-only `EvidenceReader`。

### Stage 4.4 - Control-file Selection

Tasks:

- `db/CURRENT` 必须是 UTF-8，内容必须以单个 LF 结尾；
- 去掉 LF 后只接受 `MANIFEST-<decimal>` basename，不接受 slash、反斜线、
  NUL、`.`、`..` 或其他 file type；
- active path 固定解析为 `db/<manifest-name>`，且必须存在于 replay snapshot；
- `db/IDENTITY` 可选；存在时必须是单行规范 UUID；
- 不自动选择“数字最大”的 MANIFEST 作为 fallback；
- CURRENT 或活动 MANIFEST 无效时整个 Stage 4 inventory 失败并回滚。

Expected result:

- 不受伪造路径、旧 MANIFEST 或目录穿越影响；
- 活动 MANIFEST 选择与 RocksDB `CURRENT` 语义一致；
- identity 缺失可明确记录为 `None`，格式损坏不能静默忽略。

### Stage 4.5 - Source-local Persistence

Tasks:

- 新增 `ceph_rocksdb_manifests`；
- 新增 `ceph_rocksdb_column_families`；
- 新增 `ceph_rocksdb_live_files`；
- manifest 保存 active path、identity、logical edit count、最终 sequence/file
  number fields 和 comparator；
- column family 保存 ID/name/drop state；
- live file 保存 CF ID、level、file number、file size、smallest/largest
  sequence、smallest/largest internal-key length；
- 不保存 internal-key bytes；如后续需要关联，只允许保存稳定 hash；
- OSD label、BlueFS inventory/replay 和 RocksDB inventory 在同一 source DB
  transaction 原子替换。

Expected result:

- 重导不会留下旧 column family 或 live SST；
- 任一 RocksDB record 校验/写入失败会回滚整个 OSD aggregate；
- 不同 source DB 中相同 file number 不冲突。

### Stage 4.6 - Import Integration and Audit

Tasks:

- BlueFS replay 成功后立即构建 RocksDB inventory；
- 成功状态仍为 `ready_metadata`，普通 `file_entries` 保持零；
- audit 增加 active manifest、identity presence、edit/CF/live-file 数量和
  last sequence，不记录 internal key；
- 错误继续使用 parser/unsupported/io 分类并经过 UI 脱敏边界；
- 不新增前端业务计算或 Tauri command。

Expected result:

- Stage 4 对现有六成员 cluster runner 透明接入；
- 三个 host `disk01` 文件树/预览不回退；
- 三个 OSD `disk02` 增加 RocksDB control-plane inventory。

### Stage 4.7 - Real-sample Gate

Tasks:

- 六成员串行门禁继续使用 `max_import_workers=1`；
- 为三个 OSD 固定 CURRENT、IDENTITY、edit count、CF count、关键 sequence
  和 live-file count oracle；
- server01 必须逐项核对独立 `ldb manifest_dump` 控制字段，并用
  `list_live_files_metadata` 核对最终 live set；
- server02/server03 的精确数字只接受 native parser 与只读 BlueFS export
  上的 `ldb manifest_dump` / `list_live_files_metadata` 双重确认；
- 保持 source DB 路径唯一、普通文件行零、三个 OSD UUID/BlueFS UUID 唯一。

Expected result:

- 真实样本同时验证 extent reader、RocksDB wire、manifest replay、repository
  和 cluster isolation；
- oracle 不依赖生产 parser 自证；
- 失败能定位到具体成员和 Stage 4 字段。

## 测试矩阵

| 测试面 | 用例 | 标准 |
|---|---|---|
| Log framing | FULL、fragmented、block trailer | logical record bytes 精确 |
| Recyclable log | FULL/FIRST/MIDDLE/LAST | log number 与 CRC 精确 |
| CRC | valid、corrupt、masked/unmasked confusion | corrupt typed failure |
| Fragment state | MIDDLE without FIRST、nested FIRST、missing LAST | fail closed |
| Bounds | 16 MiB record、64 MiB manifest、record count | 读取/分配前拒绝 |
| VersionEdit | tags 1-10、100/102/103、200-203、300 | 字段精确 |
| NewFile4 | known custom、safe unknown、mandatory unknown | 仅 safe unknown 可跳过 |
| Replay | add/drop CF、add/delete SST、multi-CF | 最终 live set 确定 |
| CURRENT | valid LF、path traversal、wrong type、missing manifest | 仅合法活动文件通过 |
| IDENTITY | absent、UUID、invalid UTF-8/multiline | absent 可记录，损坏失败 |
| BlueFS read | multi-extent、EOF、wrong bdev、short extent | 不越界且不读 padding |
| Persistence | replace、rollback、cascade、source isolation | aggregate 原子 |
| Real PVE | 三个 `disk02` | exact control-plane oracle |
| Cluster regression | 六成员串行 | 三 ready + 三 ready_metadata |

## 性能与资源标准

- 不启动 RocksDB runtime，不建立 block cache 或 background thread；
- 单 OSD 控制文件总读取上限 `64 MiB + small control files`；
- manifest replay 时间复杂度目标 `O(edit_count + file_mutations log N)`；
- live file 与 column family 结果使用有序 map，输出无需额外不确定排序；
- SQLite 写入使用一个 source-local transaction 和 prepared statements；
- 不并行读取同一个 E01 evidence reader；
- 相比 Stage 3 六成员串行门禁，Stage 4 增量耗时应主要由 MANIFEST 大小决定，
  当前样本目标每个 OSD低于 2 秒，不含 E01/LVM 初始化。

## 工程与安全标准

- `rocksdb-wire`、BlueFS file reader、inventory orchestration、record mapping、
  repository 分属独立文件；
- 生产文件目标不超过 500 行，函数目标不超过 100 行；
- 测试正文只在物理 `tests/`；
- 所有文件 UTF-8，手工编辑只使用 repository edit tool；
- 原始证据严格只读，不调用 `ceph-bluestore-tool repair`、RocksDB recovery
  或任意 mount；
- 不使用 mock 生产路径；synthetic fixtures 只用于 parser unit tests；
- 真实 oracle 来自只读 `ceph-bluestore-tool`、`ldb manifest_dump` 与
  `ldb list_live_files_metadata`。

## 验收标准

- 三个真实 OSD 均可读取合法 CURRENT、IDENTITY 与活动 MANIFEST。
- RocksDB physical log CRC、fragment、VersionEdit tag 和 NewFile4 custom field
  规则均有 valid/invalid/edge tests。
- server01 inventory 与独立 `ldb manifest_dump` 的 edit、CF、sequence
  控制字段及 `list_live_files_metadata` 的 live set 一致；server02/server03
  完成同级精确核验。
- OSD/BlueFS/RocksDB inventory 在一个 source DB transaction 原子提交。
- `ready_metadata`、零普通文件行、source DB 隔离和证据只读边界不变。
- 文档不宣称 SST/WAL content、RADOS/PG/object、RBD 或 VM disk 已支持。
- workspace Rust 门禁、结构 guard、文档 guard 和真实 PVE 门禁全部通过。
