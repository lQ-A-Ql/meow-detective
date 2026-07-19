# CephFS 逐步重建设计

## 1. 文档定位

本文档是 CephFS 能力的首版权威设计。目标是从只读 Ceph 证据中逐步恢复：

1. CephFS 是否存在以及证据是否足够新鲜。
2. filesystem、metadata pool、data pool、MDS rank 和 epoch 的绑定关系。
3. MDS journal 对 namespace 的影响。
4. inode、dirfrag、dentry、backtrace、xattr、snapshot realm 和 hard link。
5. 文件 layout 到 RADOS 对象的映射。
6. 独立 source DB 中的可浏览文件树和 bounded-range 文件预览。

本设计不把“发现 Ceph 集群”或“存在 RBD”解释为“存在 CephFS”。当前
`E:\pangushi\服务器` 仍为 `indeterminate (strongly leaning absent)`，直到
FSMap freshness proof 成立之前，不创建 CephFS 数据源。

## 2. 开发基线

### 2.1 已验证基线

2026-07-19 对 `E:\pangushi\服务器` 执行六成员串行回归：

- `ready=6`，`failed=0`。
- 三个宿主 `disk01` 完成 EXT4 文件树，分别为 `62,403`、`62,380`、
  `62,405` 条记录。
- 三个 BlueStore `disk02` 保持 `ready_metadata`，普通文件记录为 `0`。
- 显式三 OSD inventory 重建 `vm-100-disk-0`，得到 `114,260` 条文件记录。
- 内部总耗时 `712.968s`，测试进程墙钟 `805.22s`。
- RBD、BlueStore、source DB、文件树和预览链路保持通过。

完整结果记录在
`docs/real-sample-regression/2026-07-19-pve-cluster-import-rerun.md`。

### 2.2 当前代码事实

- `domain::DataSourceKind` 目前只有 `E01`、`Raw`、`LogicalDirectory`、
  `CephRbd`，没有 `CephFs`。
- 现有 RBD 入口由 `rbd_catalog`、`rbd_reader`、`rados_provider`、
  `derived_source` 和 source-bound LVM 组成。
- 现有 RBD provider 的 object lookup 包含 RBD head/snapshot 语义，不能直接
  当作 CephFS object locator。
- source DB 独立路径、publication seal、manifest、phase lease/heartbeat/recovery
  已具备复用条件。
- 当前没有从真实 monitor store 自动采集的 FSMap/MDSMap canonical snapshot，也没有
  CephFS inode/dirfrag/dentry decoder 或 CephFS layout reader。FSMap/MDSMap wire
  foundation 与 bounded MDS journal framing/audit 已实现，但仍只由 synthetic fixture
  验证，不能据此创建 CephFS 数据源。

### 2.3 官方实现依据

Ceph 官方文档和源码将 CephFS 拆成 metadata pool、data pool、MDS 状态、
filesystem map、journal、directory fragmentation、inode/backtrace 和 file
layout 等相互关联的语义。实现必须以这些对象和 epoch 为证据，而不是以
`ceph.conf` 或 PMXCFS storage 配置为唯一事实。

- [CephFS Concepts](https://docs.ceph.com/en/latest/cephfs/)
- [CephFS File Layouts](https://docs.ceph.com/en/latest/cephfs/file-layouts/)
- [CephFS Snapshots](https://docs.ceph.com/en/latest/cephfs/snapshots/)
- [CephFS Journal Tool](https://docs.ceph.com/en/latest/cephfs/cephfs-journal-tool/)
- [CephFS Disaster Recovery](https://docs.ceph.com/en/latest/cephfs/disaster-recovery-experts/)
- [Ceph MDS source tree](https://github.com/ceph/ceph/tree/main/src/mds)

## 3. 目标架构

### 3.1 平台和数据源模型

新增独立的 `DataSourceKind::CephFs`，不复用 `CephRbd`。建议稳定存储值为
`ceph_fs`，派生源 identity 至少包含：

```text
ceph-fs:<clusterId>:<filesystemId>:<fsmapEpoch>:<metadataPoolId>
```

`fsmapEpoch` 是 provenance 和 invalidation identity，不是 UI 展示名称。
filesystem 名称、FSID、pool 名称只能作为显示字段和校验字段。

CephFS source DB 的物理路径仍为：

```text
sources/<dataSourceId>/source.db
```

app.db 只保存 cluster、filesystem 注册、来源 source IDs、presence 状态、
当前 epoch、processing 状态、publication 和审计；文件树、inode、dentry、
artifact-local metadata 和 file locator 写入 CephFS 独立 source DB。

### 3.2 模块拆分

首版生产模块按单一能力拆分，不把 CephFS 追加到 RBD 上帝模块：

```text
crates/ceph-wire/src/cephfs/
  fsmap.rs
  mdsmap.rs
  journal.rs
  inode.rs
  dirfrag.rs
  dentry.rs
  backtrace.rs
  layout.rs
  object_name.rs

crates/persistence-sqlite/src/repositories/
  ceph_fs_repo.rs
  ceph_fs_map_repo.rs
  ceph_fs_journal_repo.rs
  ceph_fs_namespace_repo.rs
  ceph_fs_locator_repo.rs

crates/app-services/src/ceph_reconstruction/cephfs/
  presence.rs
  inventory.rs
  freshness.rs
  journal_replay.rs
  namespace.rs
  layout_reader.rs
  materialization.rs
  recovery.rs
```

`ceph-wire` 只解码不访问 SQLite、不访问 Tauri、不读取宿主路径。
`persistence-sqlite` 只负责 source-local/app-level 持久化和查询。
`app-services` 负责 use-case orchestration、完整性策略、取消、lease 和
publication。命令层只做 DTO 验证和服务调用。

### 3.3 可复用但不能直接复用的部分

可复用：

- source DB locator / source connection manager。
- source-bound RADOS bounded read primitive、三副本字节一致性和 verified page cache。
- source DB manifest、publication seal、atomic rename、quick-check 和 recovery。
- processing phase ledger、heartbeat、stale attempt 拒绝和后台任务生命周期。
- 文件预览的 bounded-range、opaque session、read-drain 和缓存失效。

不能直接复用：

- `RbdImageDescriptor`：CephFS 不是 RBD head image。
- `RBD_HEAD_SNAP_HEX`：CephFS 对象命名和 namespace 规则不同。
- RBD partition/LVM/XFS 探测链：CephFS 已经是文件系统 namespace，不应伪装成磁盘
  分区。
- RBD catalog digest：CephFS digest 必须覆盖 filesystem identity、epoch、inode/
  dentry graph、layout 和 journal boundary。

## 4. 状态和一致性模型

### 4.1 Presence 三态

```text
present
  FSMap freshness proof 成立，至少一个 filesystem record 有完整 pool/MDS binding

absent
  FSMap freshness proof 成立，且 filesystem record 数量明确为 0

indeterminate
  FSMap 缺失、epoch 不可验证、source 集合不闭合、或 map 之间冲突
```

只有 `present` 才能进入 CephFS 重建；`absent` 只显示证据结论，不创建空
source；`indeterminate` 必须显示原因和证据范围。

### 4.2 Freshness proof

freshness proof 至少需要：

- 来自已注册 source 的 FSMap/MDSMap snapshot。
- snapshot 的 source/inventory identity、读取范围、解析版本和 epoch。
- map 内 filesystem、metadata pool、data pool、MDS rank 的字段完整。
- 同一 cluster scope 下没有互相冲突的 map epoch/identity。
- OSD/pool inventory 能证明相关 pool 的来源集合，或明确标记为不完整。
- journal 头部 epoch/sequence 与 map 的边界可比较，无法比较时降级。

`ceph.conf`、PMXCFS 的 `cephfs:` storage、MDS 目录存在与否只能是 corroborating
evidence，不能单独产生 `present` 或 `absent`。

### 4.3 发布边界

CephFS source 只有在以下条件同时满足时才可以 `ready`：

- presence=`present`。
- FSMap/MDSMap/pool binding 通过校验。
- namespace manifest 已封印。
- layout/object locator 校验通过。
- publication seal 与 source DB path、digest、attempt identity 匹配。
- journal 状态明确为 `clean` 或 `replayed_to_boundary`。

若 journal 截断、MDS rank 不确定、dirfrag 缺失、pool inventory 不完整或
object locator 冲突，只能发布 `ready_metadata` 或 `incomplete`，不能假装
为完整可预览文件树。

## 5. Stage 设计

### Stage 0：Presence proof 与样本冻结

#### stage_design

先证明“有没有 CephFS”，再写任何 CephFS 文件树。当前 PVE 样本只作为
negative/indeterminate regression，不作为 CephFS positive fixture。

#### phase / tasks

**Phase 0.1 证据盘点**

- 建立 FSMap/MDSMap/pool/object evidence inventory。
- 标记每个字段的 source ID、inventory ID、offset/object identity 和 epoch。
- 记录当前样本 `indeterminate` 原因，不创建 CephFS source。

**Phase 0.2 三态判定器**

- 新增 typed `CephFsPresenceState` 和 `CephFsPresenceDiagnostic`。
- 固定 `present/absent/indeterminate` 的判定顺序。
- 禁止使用字符串 warning 或 UI 文案参与状态判定。

**Phase 0.3 fixture**

- 构造无 FSMap、空 FSMap、FSMap 有 filesystem、map 冲突、epoch 不连续 fixture。
- 每个 fixture 固定 expected JSON 和 provenance。

#### Stage 0 snapshot contract

Stage 0 只读取 source DB 的两个受控 `source_meta` key，不创建新的 CephFS
数据源，也不把 RBD 或 BlueStore 记录推断为 CephFS：

```text
ceph.fsmap.presence.v1
ceph.mdsmap.presence.v1
```

两个 snapshot 都必须包含 `schemaVersion=1`、cluster/source/inventory identity、
大于零的 map epoch、RFC3339 `capturedAt`，并且通过 source set 内的一致性校验。
FSMap 的每个 filesystem record 必须绑定非零 metadata pool 和至少一个 data
pool；MDSMap 必须引用同一 FSMap epoch，并为每个 filesystem 提供 MDS record。
`rankCount=0` 不代表 filesystem 不存在，只表示当前没有可证明的 active MDS。

任何 key 缺失、JSON 损坏、freshness 无法证明、source set 不闭合、cluster identity
或 epoch 冲突、FSMap/MDSMap filesystem 集合不一致，均只能返回
`indeterminate` 并附带结构化 diagnostic。只有完整空 FSMap + 空 MDSMap 才能返回
`absent`；只有完整非空 FSMap 且 pool/MDS binding 闭合才能返回 `present`。

#### 预期结果

当前 PVE 样本仍为 `indeterminate`；不存在“因为有 RBD 所以创建 CephFS”
的路径。

#### 完成门槛

三态判定器单测、presence assessment serde round-trip、negative real-sample
regression 和文档守卫通过。

### Stage 1：FSMap、MDSMap 和 pool binding

#### stage_design

把 filesystem identity、metadata pool、data pool、MDS rank 和 epoch 规范化，
形成后续所有 parser 的唯一输入。

#### phase / tasks

**Phase 1.1 FSMap decoder**

- 解码 filesystem ID/name、metadata pool、data pool 列表和 map epoch。
- 拒绝缺字段、重复 filesystem ID、非法 pool ID 和不一致 epoch。

**Phase 1.2 MDSMap decoder**

- 解码 rank、gid、state、incarnation、map epoch 和 metadata boundary。
- 将 `up:active`、recovering、replay、stopped 等状态映射为 typed enum。

**Phase 1.3 binding validator**

- 验证 FSMap 与 MDSMap filesystem ID 一致。
- 验证 metadata pool 与 data pool 都属于同一 cluster scope。
- 不因“没有 active MDS”直接认定 filesystem 不存在；状态进入
  `present_but_not_replayable`。

#### 预期结果

得到稳定的 `CephFsDescriptor`，包含 FSID、pool identity、MDS map identity、
epoch 和 evidence provenance。

#### 完成门槛

跨 source 冲突 fail closed；同一 source 重复导入幂等；所有字段都有 source
provenance。

#### 2026-07-19 implementation snapshot

Stage 1 的 decoder/binding foundation 已实现，但不改变 CephFS 的生产支持等级：

- `ceph-wire::cephfs` 按 Ceph upstream 当前编码顺序有界解码 FSMap v7-v8、
  MDSMap v4-v5、Filesystem v1-v2 和 `mds_info_t` v4-v10；未知未来版本、截断、
  非法 bool、未知 MDS state、重复 filesystem/GID/rank/pool 和 epoch 冲突均返回
  typed error。
- MDS state 使用 Ceph `ceph_fs.h` 的原始整数定义，保留 rank、GID、incarnation、
  state sequence、`in/up/failed/stopped/damaged` 集合和 last-failure OSD epoch。
- `app-services::ceph_reconstruction::cephfs` 对跨 source FSMap 和独立 pool
  evidence 执行确定性 binding；同一 source/inventory 重复输入幂等，不同 source
  的 cluster/map 冲突、缺失 pool 或跨 cluster pool 一律 fail closed。
- 输出的 `CephFsDescriptor` identity 固定为
  `ceph-fs:<clusterId>:<filesystemId>:<fsmapEpoch>:<metadataPoolId>`，并分别保存
  map provenance 与 metadata/data pool provenance。无 active MDS 时保持
  `present_but_not_replayable`，不降级为 absent。

实现依据固定为 Ceph upstream `src/mds/FSMap.cc`、`src/mds/FSMap.h`、
`src/mds/MDSMap.cc`、`src/mds/MDSMap.h`、`src/include/ceph_fs.h`、
`src/common/CompatSet.cc`、`src/msg/msg_types.h`、`src/msg/msg_types.cc` 和
`src/include/encoding.h`。当前尚未实现 monitor store
中的 map 自动发现/采集，也没有 CephFS 正样本，因此不会创建 `ceph_fs` source；
`E:\pangushi\服务器` 仍保持 `indeterminate (strongly leaning absent)`。

### Stage 2：metadata pool inventory 与只读 object locator

#### stage_design

先建立 metadata pool 的对象库存和可信 locator，再解析 namespace。不能通过
扫描得到的路径字符串反推 inode。

#### phase / tasks

- 为 metadata pool 建立 object identity、pool ID、namespace、generation、
  checksum/provenance inventory。
- 识别 inode、dirfrag、dentry、backtrace、xattr、snap realm 候选对象。
- 对未知对象保持 raw metadata-only 记录，不当作普通文件。
- 将 object locator 设计为：

```text
<filesystem-id>:<pool-id>:<namespace>:<object-name>:<epoch>
```

- 复用 source-bound RADOS bounded read，但把 RBD head lookup 替换为
  `CephFsObjectLocator`。

#### 预期结果

可以确定“哪些对象属于 CephFS metadata pool”，但尚不生成可浏览文件树。

#### 完成门槛

object identity 冲突、跨 pool 引用、越界 range、未知 object type 均有 typed
错误或 metadata-only 降级。

#### 2026-07-19 implementation snapshot

Stage 2 的 metadata inventory、locator 与 source-bound object range foundation 已实现，
但仍不创建 CephFS 数据源，也不生成文件树：

- `ceph-wire::cephfs::object_name` 按 Ceph metadata object 命名语义区分
  dirfrag candidate、standalone inode candidate、journal data/pointer、purge queue、
  rank table、open-file table、snap table 和 anchor table。`<ino>.<frag>` 只标记为
  dirfrag candidate；inode、dentry、xattr、snapshot realm 是对象内能力，不伪装成
  独立 object kind。未知和非 UTF-8 名称保留为 metadata-only。
- `CephFsObjectLocator` 使用
  `<filesystemId>:<poolId>:h<namespaceHex>:h<objectNameHex>:<fsmapEpoch>` canonical
  形式，保持 namespace/object name binary-safe，并拒绝非 canonical 数字、hex、空名称、
  range overflow 和 EOF 越界。
- `source_018_cephfs_metadata_inventory.sql` 与
  `ceph_fs_metadata_inventory_repo` 保存 source-local manifest 和 object projection。
  BlueStore metadata pool 通过 SHA-256 keyset cursor 分页，单页最大 4,096 条；一次
  inventory 在同一 deferred SQLite snapshot 内读取，跨页 semantic digest 改变会失败。
  replacement 原子、重复写幂等、digest 冲突和跨 pool 引用回滚。inventory digest
  使用顺序无关的 canonical projection 集合计算，写入与跨 source merge 都校验
  schema/profile、source binding、计数、locator 和实际 digest，禁止内存构造绕过持久化约束。
- `SourceDbCephFsObjectReader` 必须绑定完整 `CephFsDescriptor.identity`、metadata pool
  provenance、每个 source 的完整 manifest 和 persisted locator projection。read plan 只按
  projection 中的 `object_identity_sha256` 查询，不调用 RBD name/head lookup。完整 source
  集合可大于可配置副本数；对象存在数必须精确等于期望副本数，且 replica metadata、
  object size 和返回 bytes 全部一致。
- 单次 object range 上限保持 1 MiB，offset/length 使用 checked arithmetic，精确 EOF
  的零长度读取允许通过。原有 RBD 三副本策略、RBD object naming、verified cache、
  文件预览和 Hex 链路均未改变。

实现语义依据固定为 Ceph upstream
`ceph/ceph@42b8bc661d12759cc5fbb65e49804615263c679d` 的
`src/mds/CInode.cc`、`src/mds/CDir.cc`、`src/mds/CDir.h`、
`src/mds/mdstypes.h`、`src/mds/OpenFileTable.cc`、`src/mds/SessionMap.cc` 和
`src/mds/MDSTable.cc`。backtrace 按 RADOS `parent` xattr 处理，不作为独立对象。

当前测试覆盖 classifier 3 项、repository 5 项、service inventory/merge 6 项和
source-bound reader 6 项（含 stale semantic projection 拒绝）。尚未具备 monitor store 自动 map 采集、MDS journal replay、
inode/dirfrag/dentry 解码、namespace tree、CephFS source 或前端入口；
`E:\pangushi\服务器` 仍为 `indeterminate (strongly leaning absent)`。

### Stage 3：MDS journal bounded replay

#### stage_design

journal 是 crash consistency 和 namespace 最新状态的关键。Stage 3 先建立可审计的
wire/framing 边界，Stage 4 再复用同一输入解码 `EMetaBlob` 并与 inode/dirfrag
基础快照做版本敏感的幂等合并。不能把已完成 frame 扫描等同于已恢复 namespace。

实现固定对照 `ceph/ceph@42b8bc661d12759cc5fbb65e49804615263c679d`：

- `JournalPointer.h/.cc`：rank pointer 的 `front/back` 与对象命名。
- `Journaler.h/.cc`：header、legacy/resilient stream framing 和保守持久化尾。
- `Striper.cc::file_to_extents`：通用 `stripe_count/stripe_unit/object_size` 映射。
- `LogEvent.h/.cc`：event header、rank-local segment boundary 和 event type。

`front` 是权威 journal；`back != 0` 只记录为 rewrite/cleanup 中间态，不与
`front` 合并回放。首版不实现 Ceph recovery 的 tail probe，因此只读取
`expire_pos..write_pos`，并把 `write_pos` 命名为 `committed_header_tail`，不宣称它是
磁盘上的最新完整尾。

#### phase / tasks

- 通过 MDSMap `up[rank] -> daemon GID` 与 `mds_info_t.inc` 建立当前 rank
  incarnation，并将 rank、GID、incarnation、daemon state 一致性作为 journal replay
  前置条件。MDSMap 的 legacy `inc` map 是上游按 epoch 伪造并在解码后丢弃的兼容字段，
  不作为 daemon incarnation 事实源。
- 解析 `JournalPointer`、legacy-v1/current-v2 header、magic、position、layout、pool 和
  stream format；pointer/header 建立失败时不写 replay projection。
- 按 Ceph Striper 算法读取条带化逻辑流；所有 object range 继续通过 Stage 2
  source-bound replica reader，不增加宿主路径或单副本旁路。
- 解码 legacy/resilient frame、sentinel、payload length、trailing `start_ptr`、
  versioned `LogEvent` 和 `ESegment/ELid` rank-local sequence。
- 分别输出 `framing_safe_pos` 与 `namespace_safe_pos`。未知 event、classic event、
  未支持 mutation payload、sequence 回退、截断、预算耗尽和副本冲突不会越过安全边界。
- 保存 frame payload SHA-256、逻辑 range、object range、source/inventory/object
  provenance；不保存原始 payload，不修改 evidence。
- source-local `source_019` 只持久化 replay audit projection，不写 inode/dentry/file row。
- 禁止修改原始 journal；不调用宿主 `cephfs-journal-tool` 的写恢复模式。

#### 预期结果

得到可审计的 rank-local journal framing 结果：

```text
JournalPointer(front)
  -> verified Header(committed_header_tail)
  -> replica-verified striped ranges
  -> complete LogEvent frames <= framing_safe_pos
  -> supported namespace boundary <= namespace_safe_pos
```

Stage 3 不创建 CephFS source、不生成文件树、不接 preview。`EUpdate/EPeerUpdate/EOpen`
中的 `EMetaBlob`、snapshot、peer rollback、fragment 和跨 rank resolve 仍为 Stage 4
输入；遇到它们时保留 opaque digest/provenance，并冻结 namespace boundary。

#### 完成门槛

pointer/header/layout golden wire、`stripe_count > 1`、legacy/resilient frame、所有截断
前缀、坏 sentinel/start pointer、未知 event/version、重复/倒退 segment sequence、
byte/event budget、缺对象和副本冲突均有测试。跨 FSMap/MDSMap epoch 或 stale rank
incarnation 必须在读取/聚合前拒绝。只有 `framing_safe_pos == committed_header_tail` 才能
标记 `complete_to_header_tail`；该状态仍不等于 namespace complete。

#### 2026-07-19 implementation snapshot

Stage 3 的 journal framing、bounded replay 与 source-local audit persistence 已实现，
但不创建 CephFS 数据源、文件树或预览入口：

- `ceph-wire::cephfs::journal` 解析 `JournalPointer`、legacy-v1/current-v2 header、
  `ceph fs volume v011` magic、legacy/resilient frame、sentinel、payload length、
  resilient `start_ptr` 与 versioned/classic event envelope。decoder 维持 16 MiB 单 event
  上限，并对控制结构和 frame prefix 做 exhaustive truncation 回归。
- journal 数据读取复用 Stage 2 `CephFsObjectRangeReader`，按 Ceph Striper 规则覆盖
  `stripe_count > 1`，单次 object range 仍受 1 MiB 上限约束。pointer、header 和每个
  event frame 都保留 canonical locator、逻辑/对象 range digest 以及完整副本 provenance。
- rank 必须处于 Ceph `MAX_MDS=0x100` 范围内，并与当前 MDSMap 的 GID、incarnation、
  active daemon 一致。major/segment boundary 使用其安全解码出的 encoded sequence，
  无显式 sequence 的受支持 boundary 使用逻辑 offset；普通 event 从当前 boundary
  sequence 递增，不做跨 rank 全局排序。非初始 `ELid` 仅保留物理审计并标记为忽略，
  不重置当前 sequence。
- `write_pos` 仅命名为 `committed_header_tail`。完整 frame 推进
  `framing_safe_pos`；unknown/classic/未支持 mutation event 只冻结
  `namespace_safe_pos`，保留 opaque payload SHA-256，不伪造 namespace mutation。
  `unused_pos` 作为原始 header 字段保留，但不参与 `trimmed <= expire <= write`
  的提交边界排序。
- `source_019_cephfs_journal_replay.sql` 和 `ceph_fs_journal_repo` 在目标 source DB
  中原子替换 manifest、event metadata 与 object spans。service adapter 先在内存中
  完成全部投影，再要求每个 span 对目标 `(dataSourceId, inventoryId)` 恰好存在一条
  provenance；缺失、重复、跨 inventory/object、determinism conflict 均在写入前或
  同一事务内 fail closed。schema 不保存 raw event payload。
- replay digest 覆盖 pointer/header、header layout/positions、framing/namespace boundary、
  event metadata、所有 object range 以及排序后的 source provenance。两个独立 source DB
  对同一回放生成各自绑定的 projection，重复写为 no-op。

当前测试覆盖 wire 12 项、striped layout 3 项、service replay/discovery 14 项、
source-local persistence 8 项和 repository 16 项；`cargo clippy` 及模块、函数、测试布局、
service boundary 守卫通过。当前私有 PVE 样本仍无 fresh positive FSMap/MDSMap evidence，
因此继续保持 `indeterminate`，Stage 3 不改变 presence 结论，也不写 CephFS file row。

### Stage 4：inode / dirfrag / dentry namespace graph

#### stage_design

把 CephFS namespace 视为 inode graph，不以路径覆盖事实。目录 fragment、hard
link、orphan/stray、snapshot realm 和 backtrace 必须分别建模。

#### phase / tasks

- 解码 inode core、mode、uid/gid、size、timestamps、nlink、layout xattr。
- 解码 dirfrag hash/range、dentry name、inode reference 和 fragment identity。
- 用 root inode 构建主树，不能创建无来源 placeholder root。
- 解析 backtrace 做父目录校验；父子关系冲突时保留 diagnostic。
- hard link 共享 inode identity，不复制成无 provenance 的普通文件。
- symlink 保存 target bytes，不把 target 当作 host path。
- orphan/stray 作为独立 namespace 状态，不强行挂入正常目录。
- snapshot realm 先保存 lineage；未实现 snapshot view 前不得混入 live tree。

#### 预期结果

生成可重建的 source-local namespace manifest：

- path 是派生视图。
- inode/dentry/fragment/backtrace 是事实源。
- 每个目录和文件都有 completeness/provenance。

#### 完成门槛

目录 fragment 不完整、重复 dentry、nlink 不一致、循环父链、跨 filesystem
引用均不能静默发布为完整树。

### Stage 5：CephFS file layout 与 bounded data reader

#### stage_design

实现内容预览前，先闭合 file layout 到 data pool object 的数学映射。该层
不能套用 RBD image striping。

#### phase / tasks

- 解码 layout：stripe unit、stripe count、object size、data pool、layout
  version 和 inherited/default 状态。
- 明确 logical offset -> stripe/object number/object offset 的公式，并为每次
  映射保留输入 layout 和 object identity。
- 支持 inline data、sparse hole、partial tail、zero extent。
- 对 data pool object 缺失区分“经完整性证明的 sparse hole”和“无法证明的
  缺失”，后者 fail closed。
- 复用 verified RADOS page cache 和三副本字节一致性，但 cache key 必须包含
  filesystem、pool、object identity、epoch 和 locator version。
- 不整文件 materialize；所有 preview 使用 bounded range。

#### 预期结果

任意已闭合 locator 的文件可以通过 `FileEntryId`/opaque preview session
读取 bounded range；无法闭合的文件只返回 typed unsupported/incomplete。

#### 完成门槛

inline、sparse、跨 object、文件尾、错误 layout、缺失 object、重复 object
和副本字节冲突测试全部通过。

### Stage 6：CephFS derived source DB 与前端只读链路

#### stage_design

把 CephFS 作为独立数据源接入现有 source DB 和 preview contract，不将它塞进
RBD partition/LVM/XFS 分支。

#### phase / tasks

- 增加 `ceph_fs` source kind 和 source-local schema version。
- 增加 CephFS lineage：FSMap/MDSMap epoch、pool bindings、source IDs、journal
  boundary、parser version、catalog digest。
- source DB 写入 namespace/file locator/diagnostic，不写原始 object bytes。
- 复用 Catalog publication seal、manifest、processing phase 和 recovery。
- preview router 依据 `source_kind=ceph_fs` 选择 CephFS reader。
- Hex/text/media 继续使用统一 bounded range；不新增前端旁路 API。
- CephFS 不自动触发 Windows/Linux 主机 artifact extractor；只有明确的
  CephFS file-content extractor 才能运行。

#### 预期结果

present 且 metadata/data locator 闭合的 CephFS filesystem 具备独立 source DB、
纵向文件树、任意文件 bounded preview 和 provenance；其他状态不显示伪文件树。

#### 完成门槛

source isolation、open/recovery/delete、前端 DTO、preview session 和 media
协议回归通过。

### Stage 7：真实样本验收与能力分级

#### stage_design

先使用 synthetic CephFS fixture 闭合算法，再引入真实 CephFS 样本；当前 PVE
样本只继续承担 negative/indeterminate 门禁。

#### phase / tasks

- 增加受控 CephFS fixture，包含至少一个 metadata pool、data pool、active/
  replay journal、dirfrag、hard link、symlink、sparse file、inline file。
- 用独立工具或可审计 oracle 固定 FSMap/MDSMap、namespace counts、layout
  mapping 和 file bytes。
- 运行当前 PVE negative proof，确认不会新增 CephFS source。
- 运行多 source isolation：CephFS + RBD + Windows/Linux E01 串行和反序导入。
- 输出 capability level：

```text
metadata-only
metadata-browseable
bounded-preview
snapshot-aware
```

- snapshot、encryption、multi-filesystem、multi-PV 语义未闭合时保持
  unsupported。

#### 预期结果

CephFS 能力按证据闭合程度发布，不以“能列出一些目录”作为完成标准。

## 6. 测试矩阵

| 层级 | 场景 | 必须断言 |
|---|---|---|
| Unit | FSMap 三态 | present/absent/indeterminate 判定稳定 |
| Unit | freshness | 缺 epoch、冲突 epoch、跨 source map 冲突 fail closed |
| Unit | MDSMap | rank/state/incarnation 解码与未知状态保真 |
| Unit | pool binding | metadata/data pool 与 filesystem/cluster scope 一致 |
| Unit | object inventory | inode/dirfrag/dentry/backtrace/layout locator 稳定 |
| Unit | journal | 完整、截断、乱序、重复、未知 event、safe boundary |
| Unit | namespace | root、dirfrag、hard link、symlink、orphan、循环父链 |
| Unit | layout | inline、sparse、stripe boundary、tail、错误 pool/object |
| Unit | RADOS | 三副本一致、缺失对象、sparse zero、字节冲突 |
| Repository | source DB | source-local replacement、foreign source 拒绝、manifest digest |
| Service | recovery | prepared/publication、stale attempt、journal boundary recovery |
| API | DTO | `cephFs` 类型、presence、diagnostic、provenance round-trip |
| Frontend | preview | FileEntryId、Hex/text/media bounded range，无 host path 泄露 |
| Isolation | 多数据源 | CephFS/RBD/Windows/Linux source DB 与 file ID 不交叉 |
| Real negative | `E:\pangushi\服务器` | 当前样本不创建 CephFS source，保留 indeterminate |
| Real positive | CephFS fixture | 固定 namespace/layout/file byte oracle |
| Performance | metadata | 不读取 data pool 全量内容即可完成 namespace |
| Performance | preview | 只读取必要 object range，内存与缓存页数相关 |

## 7. 性能与资源预算

- Presence proof 只读取 map/pool metadata，不启动文件树物化。
- metadata inventory 使用 cursor/page 分段，禁止一次性载入全部 object metadata。
- namespace 构建使用有界 inode/dirfrag cache；超过预算输出可审计 incomplete。
- data preview 使用现有 verified page cache，但 CephFS cache key 单独命名空间。
- 首版不承诺 CephFS 全量 data pool materialization。
- 任何性能优化不得跳过三副本一致性、layout 校验、journal boundary 或
  provenance。

初始门禁在 positive fixture 建立后标定：

| 指标 | 初始目标 |
|---|---:|
| presence proof | <= 1s（不含 Cargo 编译） |
| metadata inventory | 以每 100,000 objects 分段，RSS 增量 <= 256MiB |
| namespace materialization | 以 fixture 基线为准，退化 <= 10% |
| 64KiB bounded preview | p95 <= 原生 source 基线的 3 倍 |
| data pool 全量读取 | 禁止作为默认路径 |

## 8. 工程边界

- 不修改已修复的 Hex 预览。
- 不修改 RBD object naming 或 RBD lineage 语义来“顺带支持”CephFS。
- 不调用宿主挂载、`ceph-fuse`、`mount.ceph`、`rbd map`、qemu-nbd 或写模式
  `cephfs-journal-tool` 作为生产旁路。
- 不执行 repair、compact、fsck repair、journal trim 或任何证据写入。
- 不将 `ceph.conf`、PMXCFS storage 配置、MDS 目录存在性当作唯一事实。
- 不在 presence 为 `indeterminate` 时创建空 CephFS root。
- 不将 incomplete dirfrag、unknown journal event 或缺失 data object 静默当作
  空目录/零填充。
- 不在 snapshot realm、encryption、multi-filesystem 或 alternate layout
  语义未闭合前宣称通用支持。
- 所有新增 DTO 必须位于 `crates/transport/src/dto/`，前端手工同步。
- 所有测试正文位于物理 `tests/`，不写回生产 `src/`。

## 9. Stage review 与评分

每个 Stage 完成后独立复审，评分低于 90 或任一维度低于 80 不得进入下一阶段：

| 维度 | 权重 |
|---|---:|
| 取证正确性与 oracle | 25 |
| 模块化与职责边界 | 20 |
| 只读安全与 provenance | 15 |
| DTO/API 契约 | 15 |
| 恢复与 fail-closed | 10 |
| 测试覆盖 | 10 |
| 性能与资源 | 5 |

每个 Stage 必须产出：

- 代码和测试 diff。
- 真实/fixture 结果。
- 失败边界和 unsupported 清单。
- guard、fmt、clippy、test、diff check 结果。
- 文档和 progress ledger 更新。

## 10. 验收标准

### Presence

- 能输出 present/absent/indeterminate 三态和结构化原因。
- 当前 PVE 样本不被误报为 CephFS present 或 absent。

### Metadata

- filesystem、pool、MDS rank、epoch 和 source provenance 可追溯。
- inode/dirfrag/dentry/backtrace graph 可重建，冲突不静默覆盖。
- journal 可重放到明确 safe boundary，不能证明的最新状态明确降级。

### Data

- layout 到 object 的映射有独立 byte oracle。
- inline、sparse、跨 object、文件尾和副本冲突均有测试。
- 预览只走统一 bounded range API，不产生宿主路径或临时明文副本。

### Product

- CephFS 是独立 `ceph_fs` source，不显示为 RBD、分区或 LVM。
- 每个 filesystem 拥有独立 source DB 和 publication seal。
- 删除、重开、失败恢复不会影响其他 source DB。

### Release boundary

- 在 positive fixture、真实样本、source isolation、前后端契约和质量门禁
  全部通过前，不升级为通用 CephFS 支持。

## 11. 首个实施顺序

1. Stage 0：只实现 presence evidence inventory 和三态判定，不增加 CephFS 文件树。
2. Stage 1：实现 FSMap/MDSMap/pool binding canonical records。
3. Stage 2：实现 metadata pool inventory 和 CephFS object locator。
4. Stage 3：实现 bounded MDS journal decoder/replay。
5. Stage 4：实现 namespace graph 和 manifest。
6. Stage 5：实现 layout mapping 与 bounded file reader。
7. Stage 6：接入独立 source DB、preview 和 publication/recovery。
8. Stage 7：positive fixture、真实样本和能力分级验收。

Stage 0-3 foundation 已完成；下一实施边界是 Stage 4 namespace graph 与
`EMetaBlob` mutation 解码。继续冻结 RBD 路径，且在 positive presence proof 成立前，
不得向 `E:\pangushi\服务器` 写入 CephFS 假数据、空 source、文件树或预览入口。
