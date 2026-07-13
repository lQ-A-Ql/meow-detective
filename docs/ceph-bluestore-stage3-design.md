# Ceph BlueStore Stage 3 设计

**基线提交**: `1f928c1c`
**目标日期**: 2026-07-13
**范围**: E01 -> LVM LV -> BlueFS log replay -> BlueFS file inventory

## Summary

Stage 2 已完成 BlueStore label 与 BlueFS superblock/layout inventory。Stage 3
只读回放 BlueFS transaction log，恢复 BlueFS 的目录、文件、fnode 与 extent
映射，并保存 source-local metadata inventory。

本阶段不是 RocksDB 或 RADOS 解析：

- 不解析 `CURRENT`、MANIFEST、SST、WAL 的内部键值；
- 不重建 placement group、RADOS object 或 RBD/VM disk；
- 不把 BlueFS 文件伪装成案件 POSIX 文件树；
- 不启用 Ceph 上游的全设备 replay recovery scan；
- 不修改、挂载或 repair 原始 OSD。

## 开发基线

Ceph Reef 与 main 的上游实现依据：

- `src/os/bluestore/bluefs_types.h/.cc`：
  `bluefs_transaction_t`、opcode、fnode/fnode-delta wire layout；
- `src/os/bluestore/BlueFS.cc::_replay`：
  block-aligned transaction 读取、UUID/sequence、jump 和状态应用；
- `src/include/encoding.h`：
  transaction envelope 与长度前缀；
- `src/include/denc.h`：
  varint、low-zero varint 和 LBA；
- `src/os/bluestore/bluestore_common.h`：
  前 8 KiB label/superblock 保留区。

真实只读 WSL oracle：

| Member | Valid tx | Final seq | Replayed size | Dirs | Links |
|---|---:|---:|---:|---:|---:|
| `server01-disk02` | 4 | 186890 | `0x22000` | 5 | 44 |
| `server02-disk02` | 4 | 185969 | `0x22000` | 5 | 49 |
| `server03-disk02` | 4 | 185678 | `0x22000` | 5 | 42 |

三个样本均包含 `ALLOCATOR_NCB_DIR`、`db`、`db.slow`、`db.wal`、
`sharding`，并恢复 RocksDB 文件名，但本阶段只把它们作为 BlueFS metadata
inventory。

## Stage Design

### Stage 3.1 - Wire Decoder

#### Phase 3.1.1 - Transaction framing

Tasks:

- 解码 version-1 transaction envelope、BlueFS UUID、固定宽度小端 `u64`
  sequence。
- 有界读取 operation blob，独立校验 operation CRC32C。
- 提供不分配超限 payload 的 transaction prefix inspection。
- 单事务 payload 上限固定为 `16 MiB`，总 replay byte 上限固定为
  `64 MiB`，单个 BlueFS block 上限固定为 `1 MiB`。

#### Phase 3.1.2 - Operation decoding

Tasks:

- 支持 `INIT/JUMP/JUMP_SEQ`。
- 支持 `ALLOC_ADD/ALLOC_RM` 的 legacy consume-only。
- 支持 `DIR_CREATE/DIR_REMOVE/DIR_LINK/DIR_UNLINK`。
- 支持 `FILE_UPDATE/FILE_UPDATE_INC/FILE_REMOVE`。
- fnode delta 必须校验 append offset 等于当前 allocated bytes。
- 未知 opcode、字符串/extent/count 超限和 CRC 错误必须 typed fail closed。
- 单事务 operation 数量上限固定为 `262144`，避免最小 opcode 造成内存放大。

Expected result:

- synthetic transaction fixtures 覆盖全部 opcode。
- decoder 不依赖 SQLite、Tauri、evidence reader 或 replay state。

### Stage 3.2 - Bounded Extent Reader

Tasks:

- 从 Stage 2 已验证的 log fnode extents 创建逻辑文件 reader。
- 读取只允许访问 layout 已绑定的 shared bdev。
- 所有物理范围必须在设备内、4 KiB 对齐且不进入前 8 KiB 保留区。
- transaction 先读一个 BlueFS block，再按 envelope length 向上取整读取。
- jump offset 必须 block-aligned、单调向前且不超过 replay 总上限。
- Stage 3 不实现 `_do_replay_recovery_read` 的全设备扫描；extent 耗尽或尾部
  不完整在至少一个有效事务后作为 bounded stop reason 记录。

Expected result:

- 读取量与 transaction replay size 线性相关，不扫描整个 OSD。
- 单个损坏 transaction 不产生部分 committed inventory。

### Stage 3.3 - Replay State

Tasks:

- 使用独立 replay engine 管理 directory map、fnode map 和 link map。
- `INIT` 只能出现在 sequence 1。
- transaction sequence 必须连续，除非同一 transaction 的合法
  `JUMP/JUMP_SEQ` 显式推进。
- directory link 必须引用已存在的非零 inode。
- 删除目录前必须为空；删除 file 前必须不存在残余 link。
- replay 完成后每个可见文件必须至少有一个 link。
- 产出 deterministic snapshot：目录按 path 排序，文件按 path/inode 排序，
  extents 按 ordinal 排序。

Expected result:

- 可恢复 `db/*.sst`、`db/CURRENT`、`db/MANIFEST-*`、`db.wal/*.log` 等
  BlueFS 文件元数据。
- 不读取上述文件内容。

### Stage 3.4 - Source-local Persistence

Tasks:

- 增加 `ceph_bluefs_replays`、`ceph_bluefs_directories`、
  `ceph_bluefs_files`、`ceph_bluefs_file_extents`。
- replay snapshot 必须随 OSD/BlueFS superblock inventory 在同一 source DB
  事务提交。
- repository 写入口只允许通过 OSD aggregate repository。
- 保存 stop reason、transaction count、first/final sequence、logical bytes。
- 不保存 OSD key、宿主路径或明文 RocksDB 内容。

Expected result:

- 重导原子替换旧 replay snapshot。
- 失败回滚完整恢复旧 OSD、superblock 与 replay inventory。

### Stage 3.5 - Import and Real-sample Gate

Tasks:

- `bluefs=true` 且 Stage 2 校验通过后执行 bounded replay。
- 成功仍保持 `ready_metadata` 和零 `file_entries`。
- 审计增加 transaction/file/directory 数量和 final sequence。
- 六成员串行门禁按成员核对上表精确 oracle。
- 抽查 `CURRENT`、`MANIFEST-*`、`.sst` 与 `.log` inventory 存在。

Expected result:

- 三个 `disk02` 均恢复稳定 BlueFS metadata file inventory。
- 三个 `disk01` EXT4 文件树和关键文件预览不回退。

## 测试矩阵

| 测试面 | 用例 | 标准 |
|---|---|---|
| Framing | single/multi-block transaction | envelope、length、CRC 正确 |
| Sequence | continuous、jump、jump-seq、rollback | 仅合法单调推进 |
| Opcode | 12 个可序列化 opcode | 字段精确，`OP_NONE`/未知 opcode typed error |
| Replay | create/update/link/unlink/remove | snapshot 确定性且不留 dangling link |
| Delta | append offset match/mismatch | mismatch fail closed |
| Bounds | 16 MiB tx、64 MiB total、extent/device end | 读取前拒绝超限 |
| Tail | 首事务损坏、有效事务后截断 | 前者失败，后者记录 bounded stop |
| Persistence | replacement 与失败回滚 | aggregate transaction 原子 |
| Isolation | 相同 inode/filename 跨 source | source DB 不交叉 |
| Real PVE | 三个 `disk02` | 4 tx、精确 final seq/dirs/links/size |
| Cluster | 六成员串行 | 三 `ready` + 三 `ready_metadata` |

## 工程与安全标准

- `ceph-wire`、extent reader、replay engine、repository 分属独立文件。
- 生产文件目标不超过 500 行，函数目标不超过 100 行。
- 测试正文只在物理 `tests/`。
- 固定证据只读；不调用 Ceph repair/fsck 写路径。
- 不使用 mock 生产数据；真实 oracle 来自只读工具输出和原生生产导入。
- source/docs 统一 UTF-8。

## 验收标准

- 三个真实 OSD 均完成 bounded BlueFS log replay。
- transaction CRC、UUID、sequence、jump 和 opcode 状态约束全部验证。
- 恢复目录、文件、fnode 和 extent metadata，且结果与 oracle 一致。
- `ready_metadata`、零普通文件行和独立 source DB 边界不变。
- 不宣称 RocksDB、RADOS、PG、object 或 VM disk reconstruction 已支持。
- workspace 测试、Clippy、结构 guard、文档 guard、依赖治理和真实 PVE
  门禁全部通过。
