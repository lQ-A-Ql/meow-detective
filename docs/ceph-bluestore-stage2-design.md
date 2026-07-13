# Ceph BlueStore Stage 2 设计

**基线提交**: `91fae47f`
**目标日期**: 2026-07-13
**范围**: E01 -> LVM LV -> BlueStore label -> BlueFS superblock inventory

## Summary

Stage 1 已完成 BlueStore bdev label、CRC32C、多副本 epoch 和脱敏 OSD
inventory。Stage 2 在不改变证据只读边界的前提下，继续解析 BlueFS
superblock，并保存日志 fnode、日志 extents 和设备布局元数据。

本阶段明确不实现：

- BlueFS transaction log replay；
- RocksDB MANIFEST/SST/WAL 解析；
- BlueStore object metadata 解码；
- placement group、RADOS object、RBD/VM disk reconstruction；
- 将 BlueStore/BlueFS 注册为 `ImageFilesystemKind` 或生成伪文件树。

## 开发基线与边界

Ceph 上游稳定结构依据：

- `src/os/bluestore/bluestore_common.h`：
  `BLUEFS_SUPER_POSITION=4096`、`BLUEFS_SUPER_BLOCK_SIZE=4096`；
- `src/os/bluestore/bluefs_types.h/.cc`：
  `bluefs_super_t`、`bluefs_fnode_t`、`bluefs_extent_t`、`bluefs_layout_t`；
- `src/include/denc.h`：varint、low-zero varint 和 LBA 编码；
- `src/os/bluestore/BlueFS.cc::_open_super`：
  superblock envelope 后使用独立 CRC32C 校验。

工程边界：

- `ceph-wire` 只负责有界 wire decode，不依赖 SQLite、Tauri 或 app service。
- `app-services` 只负责从已验证的 LVM LV 读取固定 4 KiB block、交叉校验
  OSD UUID 并编排持久化。
- `persistence-sqlite` 只保存 source-local inventory。
- BlueFS inventory 只允许随所属 OSD label inventory 在同一事务写入；独立
  repository 只暴露查询能力。
- 前端本阶段不新增 Ceph 语义计算；后续 UI 只消费后端 DTO。
- 任何结构不兼容、CRC 错误、extent 越界或 UUID 不匹配都 fail closed。

## Stage Design

### Stage 2.1 - BlueFS Superblock Wire Decoder

#### Phase 2.1.1 - 通用编码能力

Tasks:

- 在 `ceph-wire` 增加有界 unsigned varint、low-zero varint 和 LBA decode。
- 对移位溢出、超长 varint、容器数量和 payload 边界返回 typed error。
- 保持已有 BlueStore label decode 行为不变。

#### Phase 2.1.2 - Superblock 解码

Tasks:

- 固定读取 4096-byte BlueFS superblock。
- 解码并校验 Ceph struct envelope 与 CRC32C。
- 返回 BlueFS UUID、OSD UUID、sequence、block size。
- 解码日志 fnode 的 inode、size、mtime、encoding、content size。
- 解码全部日志 extents 的 offset、length、bdev。
- 解码 optional memorized layout。

Expected result:

- 可原生解析三个真实 PVE `disk02` 的 BlueFS superblock。
- decoder 不读取 superblock 之外的数据。

### Stage 2.2 - Source-local Inventory

#### Phase 2.2.1 - Schema

Tasks:

- 新增 `ceph_bluefs_superblocks`。
- 新增 `ceph_bluefs_log_extents`，通过外键归属 superblock。
- 不保存 RocksDB 文件内容、OSD key 或任意凭据。

#### Phase 2.2.2 - Repository

Tasks:

- 使用单事务替换一个 OSD inventory 的 BlueFS 记录。
- 写入前校验 extent 引用、序号和 source 归属。
- 查询结果按 extent ordinal 稳定排序。

Expected result:

- 每个 source DB 独立保存一个 OSD 的 BlueFS layout。
- 重导不会留下旧 extents。

### Stage 2.3 - Import Integration

#### Phase 2.3.1 - Read and Validate

Tasks:

- 复用 Stage 1 已打开的只读 LVM LV reader。
- 当 label `bluefs=1` 时读取 offset `4096` 的 4 KiB block。
- 要求 BlueFS `osd_uuid` 与选中的 bdev label `osd_uuid` 一致。
- 单设备布局仅接受 `BDEV_DB=1`，并校验每个日志 extent 不进入前 8 KiB
  label/superblock 保留区、不超过已知设备大小。

#### Phase 2.3.2 - Persist and Report

Tasks:

- 在同一个 metadata-only import 中保存 label 与 BlueFS inventory。
- 保持 `ready_metadata`、零 `file_entries`、不运行 Linux artifact/search/timeline。
- 审计记录增加 BlueFS sequence、extent count 和 layout mode。

Expected result:

- Stage 1 行为不回退。
- UI 仍不会把 metadata source 当作 POSIX 文件系统。

### Stage 2.4 - BlueFS Log Replay Feasibility Gate

本 phase 只做设计和样本评估，不在首批代码中宣称完成。

Tasks:

- 从日志 fnode extents 有界读取 transaction log。
- 识别 envelope mode 和 transaction sequence。
- 评估目录/file map replay 所需 opcode 覆盖。
- 只有在三个真实 OSD 均可稳定 replay 后，才进入 RocksDB 文件 inventory。

## 测试矩阵

| 测试面 | 用例 | 标准 |
|---|---|---|
| Wire valid | synthetic v2/v3 superblock | 字段和 CRC round-trip |
| Wire invalid | short block、bad CRC、bad compat、varint overflow | typed error，无 panic |
| Extent bounds | offset/length 溢出或越设备尾 | import fail closed |
| UUID binding | BlueFS OSD UUID 与 label 不同 | typed parser error，不写库 |
| Persistence | superblock + 多 extent round-trip | UTF-8、稳定顺序、事务原子性 |
| Replacement | 第二次写入较新 sequence | 旧 extents 被删除 |
| Source isolation | 两个 source 使用相同 local identity | inventory 不交叉 |
| Stage 1 regression | label-only / bluefs disabled | 保持 `ready_metadata` |
| Real PVE | 三个 `disk02` | OSD UUID 对齐、CRC valid、block size 4096 |
| Cluster regression | 六成员串行导入 | 三个 `ready` + 三个 `ready_metadata` |

## 性能与安全标准

- 每个 OSD 首批额外读取固定 4096 bytes。
- 不扫描整个 BlueStore LV。
- 不启动 RocksDB，不挂载 BlueFS，不生成明文对象副本。
- 单次分配受 superblock 4 KiB 和显式容器上限约束。
- WSL oracle 脚本必须使用 read-only loop、read-only LV，并只清理本次资源。

## 评估方案

功能评估：

- 三个真实 OSD 的 BlueFS UUID 唯一；
- OSD UUID 分别匹配 Stage 1 OSD 0/1/2；
- sequence、日志 extent 和 layout 可稳定复现。

工程评估：

- production file 维持模块大小门禁；
- tests 与 `src/` 物理分离；
- command/Tauri 层无 Ceph parser 或 SQL；
- source DB schema 和文档事实同步。

性能评估：

- 六成员真实门禁总耗时不得较 Stage 1 稳定基线退化超过 10%；
- BlueStore source 仍保持零文件和 bounded memory。

## 验收标准

- 三个真实 `disk02` 均保存 CRC-valid BlueFS superblock inventory。
- BlueFS OSD UUID 与 Stage 1 label UUID 一致。
- 日志 fnode/extents 和 memorized layout 可查询。
- 错误结构不会产生部分 inventory。
- Stage 1 label inventory、EXT4 文件树和关键文件预览不回退。
- BlueFS log replay、RocksDB 和 RADOS object tree 仍明确标记 unsupported。

## 2026-07-13 实施结果

- `ceph-wire` 已实现有界 varint、low-zero varint、LBA、BlueFS super/fnode/
  extent/layout 解码和独立 CRC32C 校验。
- source schema 已增加 `ceph_bluefs_superblocks` 与
  `ceph_bluefs_log_extents`；OSD label 与 BlueFS inventory 在同一事务提交。
- 导入链路只读取 LV offset `4096` 的固定 4 KiB，不扫描整个 BlueStore
  device；OSD UUID、single-device bdev、8 KiB 保留区和 extent end 任一
  不一致即 fail closed。
- 三个真实 PVE `disk02` 均通过：BlueFS UUID 唯一、sequence `50`、block
  size `4096`、一个有界 shared-device log extent、零 `file_entries`。
- 六成员桌面串行门禁测试体耗时约 `38.09s`（本机 debug build，仅作本次回归
  记录，不作为跨机器 SLA）。
- Debian Reef 18.2.8 的 `ceph-bluestore-tool` 不识别
  `bluefs-super-dump`；WSL oracle 只读导出第二个 4 KiB block，由原生
  decoder 校验。该限制属于工具版本，不影响 Stage 2 数据存在性结论。
