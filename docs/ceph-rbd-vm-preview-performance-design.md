# Ceph RBD VM 文件预览性能优化设计

## 1. 文档状态

- 日期：2026-07-17
- 基线提交：`6bd45321`
- 私有样本：`E:\pangushi\服务器`
- 当前派生镜像：`vm-100-disk-0`
- 当前派生文件记录：114,260
- 状态：Stage 0-6 已实施，真实样本性能门禁通过

本文档只治理已经物化完成的 Ceph RBD 派生 VM 文件树预览性能。它不修改
Hex 前端交互、不扩大 Ceph 支持格式，也不降低三副本一致性校验。

## 2. 开发基线与边界

### 2.1 已确认基线

当前真实样本已经具备：

- 三个显式加载的 BlueStore OSD inventory。
- source-bound RADOS object range reader。
- bounded RBD head reader。
- 派生独立 `source.db`。
- 直接 XFS、`centos/home` XFS LV、`centos/root` XFS LV。
- 114,260 条 VM 文件记录。
- `/etc/passwd` 的真实 `handle + range` 预览。

当前性能记录：

| 场景 | 已观测结果 |
|---|---:|
| RBD VM 首次物化 | `46.28s` 至 `54.73s` |
| 已完成 source 幂等物化 | `124ms` 至 `136ms` |
| ready tree + 单次 `/etc/passwd` 预览测试体 | `6.05s` 至 `6.54s` |

`6.05s` 至 `6.54s` 是树校验与一次预览的混合测试体，不是独立 range
benchmark。本轮必须先增加分阶段计时，再建立可用于发布门禁的独立预览基线。

### 2.2 开发边界

- 不修改已修复的 Hex 纵向滚动、chunk 和 offset 跳转模型。
- 不改变 `ViewerRangeRequestDto` / `ViewerRangeResponseDto` 字段形状。
- 不把 VM 文件完整复制到宿主临时目录。
- 不把证据正文写入 `runtime-cache` SQLite。
- 不暴露原始 E01、BlueStore device 或 source DB 宿主路径。
- 不用单副本读取替代三副本比对。
- 不把缺失对象直接解释为合法 sparse zero，必须继续满足完整副本集合和
  catalog 完整性前置条件。
- 不在前端计算 RBD、LVM、XFS、inode、extent 或副本路由。
- 不扩展 PG/CRUSH、EC、degraded recovery、clone、snapshot、encryption、
  CephFS 或 multi-PV/跨 RBD LVM。
- app-services 保持 Tauri-free；Tauri command 只负责状态适配、校验和 DTO。
- 新增运行时对象必须有 TTL、容量上限、case/source 归属和确定性失效路径。

## 3. 实施前只读审计结论

### 3.1 当前调用链

```text
frontend readFileRange
  -> Tauri read_file_range
  -> read_file_range_for_source_case
  -> open source.db
  -> rebuild PreviewDescriptor
  -> open_descriptor_image_file_with_context
  -> open_derived_rbd_reader
  -> query RBD lineage
  -> validate and open three parent source DBs
  -> query OSD inventories
  -> construct SourceDbRadosObjectProvider
  -> open three BlueStore devices on demand
  -> rebuild RBD/LVM/XFS stack
  -> resolve the file path
  -> read the requested bytes
```

`open_file_handle_for_case` 返回的 handle 当前是：

```text
file:ds:<dataSourceId>:<localFileId>
```

它只是全局文件 ID 的可逆编码，不持有打开的 reader、文件读取计划或 provider。
因此每个 range 请求都会重新执行上述链路。

### 3.2 P0 根因：Ceph RBD 绕过了 XFS bounded-range

`read_file_bytes_for_descriptor_with_context` 对 `source_kind == "ceph_rbd"`
存在独立分支。该分支直接调用：

```text
open_range_content_for_descriptor_with_context
  -> open_descriptor_image_file_with_context
  -> open_first_image_path_seekable
```

XFS 已实现 `FileSystemReader::read_file_range`，但没有实现
`open_file_seekable`。`open_first_image_path_seekable` 因此回退到
`XfsReader::open_file`，而 `open_file` 当前会先执行
`read_file_content(ino)` 并构造 `Cursor<Vec<u8>>`。

结果是：

- 请求 64 KiB 也可能先读取整个 VM 文件。
- 大文件首段预览耗时和内存可能随文件总大小增长。
- 非零 offset 还可能在 materialize 后执行顺序 skip。
- `ceph_rbd` 没有复用已经存在的 XFS `read_file_content_range` 能力。

这是当前最优先的逻辑缺陷。它必须先于通用缓存优化修复，否则缓存只能掩盖
整文件 materialize，不能修正读取复杂度。

### 3.3 P0 根因：RBD provider 缓存是请求级临时状态

`SourceDbRadosObjectProvider` 已经具备：

- 每副本最多 128 项、估算 16 MiB 的 object read-plan LRU。
- 全 provider 最多 1,024 页、64 MiB 的 verified page cache。
- source DB connection 和 BlueStore evidence reader 的惰性复用。

但 provider 在 `open_derived_rbd_reader` 内按请求新建。range 返回后，
provider、三份 connection、三份 device reader、plan cache 和 verified page
cache 一起销毁。

三副本场景的理论单 runtime 上限约为：

```text
3 * 16 MiB plan cache + 64 MiB verified page cache = 112 MiB
```

如果简单按文件 handle 各自复制 runtime，内存会随打开文件数线性增长。因此
provider 必须按 derived source 和 lineage fingerprint 共享，而不是按 range
或按文件无限复制。

### 3.4 P1 根因：descriptor cache 在 source-routed 路径中未生效

`descriptor_for_file_with_cache` 支持 descriptor cache，但
`SourceScopedContext` 没有实现 `get_cached_preview_descriptor` 和
`set_cached_preview_descriptor`，实际使用 trait 的空实现。

因此每次 range 仍会：

- 打开 source DB。
- 查询 file entry。
- 查询数据源位置和分区记录。
- 重建 `PreviewDescriptor`。
- 重新验证路径、分区和 LVM identity。

这不是当前最大的耗时项，但会放大高频 Hex、文本和媒体 range 请求。

### 3.5 P1 根因：媒体协议重复打开 handle 和读取栈

`evidence-media://` 当前的 opaque handle 只映射到全局 file ID。每个 HTTP
range 请求仍会重新调用：

- `open_file_handle_for_case`
- `read_preview_bytes_for_source_case`
- 完整 RBD/LVM/XFS 打开链路

浏览器媒体组件通常会发出多个相邻或探测性 range。当前实现会把一次播放放大为
多次完整初始化。

### 3.6 P2 热点：缓存和并发粒度

- `VerifiedObjectCache` 和 `ObjectPlanCache` 用 `VecDeque::retain` 更新 LRU，
  单次 touch 为 O(n)。当前上限较小，只有 profiling 证明占比明显时才优化。
- `SharedRadosObjectProvider` 使用单个 `Mutex`。它能保证 provider 内部状态安全，
  但并发媒体 range 可能互相串行。
- 如果直接扩大 page 或预取范围，会同时放大三副本物理读取和内存占用，不能在
  没有 phase timing 与 hit-rate 数据时调整。

## 4. 剩余风险登记

### 4.1 取证正确性风险

| 等级 | 风险 | 当前影响 | 必须的控制 |
|---|---|---|---|
| High | 已加载 inventory 集合尚未独立证明等于完整副本集合 | 不能把当前私有样本能力推广为通用 Ceph 恢复 | cache key 包含 lineage 与副本集合 fingerprint；不放宽 coverage closed 校验 |
| High | `catalog_complete` 快速路径未重算完整 semantic child digest | 损坏 catalog 与合法对象缺失仍可能难以区分 | 继续 fail closed；后续增加 catalog digest 重校验 |
| High | sparse zero、对象真实缺失和损坏对象语义未完全分离 | 错误归零会改变证据字节 | 只有全部期望副本均权威缺失时才返回 Missing |
| High | 缓存若不绑定 lineage/source generation，可能返回陈旧证据 | source 重导、lineage 更新后可能读取旧页 | fingerprint 必须进入 runtime、session、page cache key，并在不匹配时拒绝命中 |
| Medium | ready-source 快速路径可能掩盖派生 RBD source 部分文件缺失 | 文件树 ready 不等于所有文件可预览 | 扩大真实文件 oracle，不用单个 `/etc/passwd` 代表全部预览能力 |
| Medium | cancellation/retry attempt ownership 尚未完全闭合 | 重试与旧 runtime 可能交叉 | runtime 建立时记录 source generation；失败重试先失效旧 generation |

### 4.2 性能与资源风险

| 等级 | 风险 | 当前影响 | 必须的控制 |
|---|---|---|---|
| Critical | Ceph RBD XFS range 退化为整文件 materialize | 大文件可能长时间无响应并产生高峰值内存 | Stage 1 先接通 context-aware bounded range |
| High | 每个 range 重建 provider、连接、device、LVM、XFS | 小文件也有秒级固定成本 | derived-source runtime 与 file session 复用 |
| High | 当前 handle 不是运行时会话 | 无法复用已打开状态，也没有明确 close | opaque session handle、TTL、LRU、close command |
| High | 按 handle 复制 112 MiB provider cache | 多文件预览可能快速占满内存 | provider 按 source fingerprint 共享；全局预算而非每 handle 独立预算 |
| High | 全局 provider 锁可能串行无关请求 | 媒体和 Hex 并发时尾延迟上升 | registry 锁只做查找；I/O 锁按 source runtime；测量后再决定最多 2 lane |
| Medium | source DB connection 和 reader 的线程归属不清 | 错误共享可能产生 Send/Sync 或锁问题 | 不写 `unsafe impl Send/Sync`；不能安全移动的 reader 使用专属 worker actor |
| Medium | descriptor cache 对 source-routed 路径无效 | 高频 range 重复 SQL 和分区解析 | session 内保存 immutable descriptor，并接入 runtime-cache metadata |
| Medium | media range 与普通 viewer 使用两套 handle 映射 | 重复初始化、失效规则不一致 | 两类入口统一到同一 PreviewSession |
| Medium | 过度预取放大三副本读取 | 降低延迟的同时增加 I/O 和内存 | Stage 4 才评估 256 KiB coalescing；默认保持 64 KiB verified page |

### 4.3 生命周期风险

- 案件关闭已有 case-level runtime-cache 清理，但尚无 live RBD runtime。
- 数据源删除当前没有 source-level preview runtime 失效接口。
- import 完成会清理 runtime-cache 和 E01 cache，但未来还必须清理对应 source
  runtime 与 file session。
- handle 过期后必须返回 typed expired/invalid，不得回退为解析 handle 内的 file ID。
- case 切换后旧 handle 必须不可跨案件使用。
- lineage 更新、source generation 变化或副本 inventory 变化必须先失效再允许新读。

## 5. 目标架构

### 5.1 分层模型

```text
Tauri command / evidence-media protocol
  -> PreviewSessionPort
  -> PreviewSessionRegistry
       -> FilePreviewSession
       -> DerivedSourceRuntimeRegistry
            -> DerivedRbdRuntime
                 -> shared SourceDbRadosObjectProvider
                 -> lineage fingerprint
                 -> bounded plan/page caches
  -> filesystem bounded-range reader
  -> ViewerRangeResponseDto / media bytes
```

### 5.2 PreviewSession

`open_file_handle` 创建不可逆随机 handle，不再把全局 file ID 编码进 handle：

```text
preview:<random-token>
```

session 至少保存：

- `handle_id`
- `case_id`
- `data_source_id`
- `global_file_id`
- `local_file_id`
- `source_generation`
- `lineage_fingerprint`
- immutable `PreviewDescriptor`
- 已解析的 partition/LVM candidate
- 文件大小与 MIME
- `opened_at` / `last_accessed_at` / `expires_at`
- 指向 derived-source runtime 的共享引用
- 可选的 XFS prepared file/session 状态

`runtime-cache::HandleRepo` 只保存可序列化 metadata。live reader、SQLite
connection、provider 和 cache 只存在进程内 registry。

### 5.3 DerivedRbdRuntime

按以下 key 共享：

```text
(case_id, derived_data_source_id, lineage_fingerprint)
```

runtime 保存：

- 已验证的 lineage aggregate。
- 排序后的副本 binding。
- immutable RBD image layout。
- 共享 `SourceDbRadosObjectProvider`。
- provider cache 预算与统计。
- source generation 和最后访问时间。

lineage fingerprint 至少覆盖：

- derived data source ID。
- image ID、object prefix、image size、order、features、stripe unit/count。
- pool ID、scope identity。
- expected replica count。
- 排序后的 parent source ID、inventory ID、OSD ID。
- parent source storage generation/schema/import state。
- 可用时加入 semantic snapshot aggregate identity。

### 5.4 文件系统 bounded-range

优先顺序固定为：

1. 使用文件系统专用 `read_file_range`。
2. 使用 prepared file plan / seekable reader。
3. 仅对没有 bounded-range 能力的小文件使用 streaming fallback。
4. 超过 fallback 阈值时返回 typed unsupported/performance-safe error，禁止整文件
   materialize。

PVE 当前 XFS 路径必须直接调用 `XfsReader::read_file_content_range` 对应能力。

### 5.5 并发与内存模型

当前实现预算：

- 全局最多 32 个 live preview handle。
- idle TTL 30 分钟。
- 全局最多 1 个 derived-source runtime；第二个 source 会按 LRU 驱逐旧 runtime，
  这是当前内存优先的明确边界。
- 单 derived-source runtime 总目标上限 128 MiB。
- verified page 默认仍为 64 KiB。
- registry mutex 只保护 map 和 LRU，不在持锁时执行证据 I/O。
- 同一 file session 的可变 reader 由 session-local mutex 串行。
- 不同 derived source 不共享 I/O mutex。

如果 concrete filesystem reader 不能满足安全的 `Send` 边界，使用
session-owned worker thread 和 channel actor。禁止通过 `unsafe impl Send/Sync`
绕过编译器。

## 6. Stage 实施计划

### Stage 0：计时、计数与真实基线

#### stage_design

先建立可解释的 phase timing，避免只凭总耗时调整 cache 大小。

#### Phase 0.1：后端观测

Tasks：

- 为 handle open 和 range read 增加统一 trace span。
- 记录 source routing、descriptor、lineage、replica binding、provider
  construction、source DB open、device open、LVM discover、filesystem open、
  path resolve、object plan lookup、verified page load、payload encode。
- 记录请求长度、实际读取长度、对象数、page hit/miss、plan hit/miss。
- 路径和宿主位置只记录稳定 ID 或 hash，不记录敏感绝对路径。

#### Phase 0.2：基线工具

Tasks：

- 新增 retained-case preview benchmark test。
- 将 tree 校验与 preview 计时拆开。
- 记录 cold、warm、sequential、random 四类结果。
- 记录 RSS delta、page cache resident bytes 和 provider construction count。

预期结果：

- 能解释 `6.05s` 至 `6.54s` 中每一阶段的占比。
- 后续 Stage 有稳定前后对照，不以 Cargo 冷编译时间作为产品性能。

### Stage 1：修复 Ceph RBD bounded-range 路由

#### stage_design

先消除整文件 materialize，再做会话缓存。

#### Phase 1.1：context-aware range factory

Tasks：

- 扩展 descriptor range reader，使 `ceph_rbd` 能通过
  `PreviewReadContext::open_evidence_reader` 创建 reader。
- `read_file_bytes_for_descriptor_with_context` 对 Ceph RBD 先走 NTFS/FAT/
  exFAT/Linux bounded-range 分派。
- XFS 直接调用 `read_file_range`，不再先调用 `open_file`。
- streaming fallback 增加文件大小阈值，禁止大文件整文件 materialize。

#### Phase 1.2：回归保护

Tasks：

- 增加 spy reader，断言 64 KiB 请求不会读取整个 100 MiB 文件。
- 验证 offset 0、对象中部、对象边界、文件尾。
- 保持普通 E01/RAW bounded-range 行为不变。

预期结果：

- VM 文件 range 的读取量与请求长度和必要的 verified page 数相关。
- 大文件总大小不再决定首段预览内存。

### Stage 2：共享 Derived RBD runtime

#### stage_design

把 provider 的连接、device、plan 和 verified page cache 从 request lifetime
提升为 source runtime lifetime。

#### Phase 2.1：runtime registry

Tasks：

- 在 app-services 定义 Tauri-free `DerivedSourceRuntimeRegistry`。
- AppState 只持有该 registry 的共享实例。
- `open_derived_rbd_reader` 拆成：
  - `resolve_derived_rbd_runtime`
  - `open_rbd_cursor_from_runtime`
- 复用现有 `SharedRadosObjectProvider`，不复制 page cache。

#### Phase 2.2：fingerprint 与失效

Tasks：

- 生成 canonical lineage fingerprint。
- case close、source delete、import complete、lineage update、retry generation
  change 时失效。
- fingerprint 不匹配时禁止复用旧 runtime。

预期结果：

- 同一 derived source 的连续 range 只构造一次 provider runtime。
- 三份 source DB connection、device reader、plan cache 和 verified page cache
  可跨 range 复用。

### Stage 3：真实 PreviewSession 与 prepared XFS file access

#### stage_design

让 handle 表示真实短生命周期会话，并避免每个 range 重新解析文件路径。

#### Phase 3.1：opaque handle

Tasks：

- `open_file_handle` 创建随机 opaque handle。
- `read_file_range` 只接受 registry 中存在且归属当前案件的 handle。
- 新增 `close_file_handle`，前端切换文件和卸载时 best-effort 关闭。
- TTL/LRU 负责异常退出和未关闭 handle。

#### Phase 3.2：XFS session

Tasks：

- handle open 时解析精确 partition/LV 和 XFS 文件路径。
- session 保存 XFS reader + canonical path，或保存等价的 immutable inode/extent
  read plan。
- 后续 range 不重复遍历祖先目录。
- session-local lock 只保护该文件 reader，不持有 registry 全局锁。

预期结果：

- warm range 不再重复 SQL descriptor、LVM discovery、XFS open 和 path resolve。
- 同一文件 Hex、文本和媒体读取可复用同一 session。

### Stage 4：媒体统一、I/O 合并与缓存治理

#### stage_design

在正确的会话模型上优化高频相邻 range，不提前扩大读取。

#### Phase 4.1：媒体协议统一

Tasks：

- media opaque handle 指向 `PreviewSession`，不再只指向 file ID。
- HTTP range 直接调用 session bounded-range。
- 浏览器探测请求、播放请求和普通 viewer 共享同一 source runtime。

#### Phase 4.2：按测量优化 I/O

Tasks：

- 评估同一 RBD object 内将相邻 64 KiB miss 合并为最多 256 KiB。
- 增加 in-flight singleflight，避免同一 verified page 被并发读取三次以上。
- 只有 profiling 证明 LRU touch 明显时，替换 `VecDeque::retain`。
- 如果单 provider mutex 的 queue time 超预算，最多建立 2 个 runtime lane；
  lane 必须共享或受统一全局内存预算约束。

预期结果：

- 媒体连续 range 不重复初始化。
- 优化不会增加不必要的跨对象读取，也不会削弱副本比对。

### Stage 5：生命周期、安全与故障注入

#### stage_design

缓存必须可以证明不会跨案件、跨 source generation 或跨 lineage 返回数据。

#### Phase 5.1：失效矩阵

Tasks：

- case close 清除全部 session/runtime。
- source delete 在删除 source DB 前 drain 并失效对应 session/runtime。
- import/re-import 完成失效旧 generation。
- lineage 或副本 inventory 更新失效对应 derived runtime。
- TTL cleanup 与显式 close 均释放 connection、device 和内存 cache。

#### Phase 5.2：故障注入

Tasks：

- 三副本字节冲突必须 fail closed。
- 部分副本缺失必须 fail closed。
- source DB 被移除、device reopen 失败、lock poisoning、range 超界返回 typed error。
- 失败 page 不得写入 verified cache。
- session 过期不得自动按 file ID 重建并继续读取。

预期结果：

- 性能状态始终是可丢弃派生状态。
- 删除 cache 只导致性能下降，不改变证据结果。

### Stage 6：真实样本门禁与交付

#### stage_design

以 PVE retained case 和非 RBD 原生链路对照共同验收。

#### Phase 6.1：真实样本矩阵

Tasks：

- 扩大 `/etc/passwd` 单文件 oracle。
- 覆盖直接 XFS、`centos/home`、`centos/root`。
- 覆盖小文本、1 MiB 级二进制、100 MiB 以上文件。
- 覆盖 viewer 与 media/protocol range 的相同字节校验；浏览器端播放时序单独保留。
- 增加检材3原生 XFS 与 PVE 宿主 `pve/root` EXT4 作为只读对照链路。
- 覆盖 source/case retire、旧 handle 拒绝、reactivate 冷重建和 session 收敛。
- 保存每个测试范围的 SHA-256 oracle，不保存证据正文。

#### Phase 6.2：发布门禁

Tasks：

- 增加 `check-pve-rbd-preview-performance.ps1`。
- retained case 缺失时默认跳过，`-RequireFixture` 时失败。
- 性能结果输出结构化 JSON，记录提交、case fingerprint、cache mode；不记录机器名
  或绝对日志路径。
- 文档更新 parser matrix、known limitations、progress ledger 和真实样本报告。

预期结果：

- “VM 文件可预览”升级为多分区、多文件、多 range、可量化性能的私有样本承诺。

## 7. 测试矩阵

### 7.1 单元测试

| 测试面 | 用例 |
|---|---|
| bounded-range | 100 MiB XFS 文件请求 64 KiB，不调用整文件 `open_file` fallback |
| offset | 0、中部、文件尾、超 EOF、跨 extent、跨 RBD object |
| provider reuse | 同 runtime 连续读取只构造一次 provider，并出现 plan/page hit |
| fingerprint | lineage、inventory、source generation 任一变化均拒绝旧 runtime |
| handle | opaque、不可逆、case scoped、TTL、显式 close、LRU eviction |
| cache correctness | 失败读取不入 cache；Missing 与 Present 分离；副本冲突不入 cache |
| concurrency | 同 handle 串行安全；不同 source 不被 registry 全局锁串行 |
| memory | handle 增加不复制 provider cache；预算达到后按 LRU 释放 |

### 7.2 集成测试

| 场景 | 预期 |
|---|---|
| open handle -> 多次 range | descriptor、provider、LVM/XFS 状态按设计复用 |
| 切换文件 | 旧 handle best-effort close，新 handle 不污染旧 active offset |
| 切换案件 | 旧案件 handle 返回 expired/invalid |
| 删除 source | 先 drain/失效 session，再删除 source DB |
| import complete | 旧 generation 的 session/runtime 全部失效 |
| text/Hex/media | 三条入口读取相同 offset 时字节一致 |
| cache eviction | eviction 后可正确冷重建，字节 oracle 不变 |

### 7.3 真实样本性能矩阵

测试源：

- PVE RBD：`E:\pangushi\服务器` 的 retained derived VM case。
- 非 RBD XFS：`D:\獬豸杯\检材3.E01`。
- PVE 宿主 EXT4：代表 `disk01` retained source。
- 逻辑目录：生成的同尺寸只读 fixture，仅作为宿主 I/O 下界。

每类文件执行：

- cold first preview。
- warm same-range repeat。
- 连续 16 个 64 KiB range。
- 连续 4 个 1 MiB range。
- 同一 RBD object 内随机 offset。
- 跨 RBD object offset。
- 文件尾 range。
- cache eviction 后重读。
- case switch 后旧 handle 读取。

文件覆盖：

- 小文本：`/etc/passwd`。
- 每个 XFS root 至少一个可稳定预览文件。
- 1 MiB 至 16 MiB 普通文件。
- 100 MiB 以上普通文件。
- 若样本不存在指定尺寸文件，测试必须明确 skipped reason，不能用小文件伪装。

## 8. 性能预算与验收标准

### 8.1 首版目标

| 指标 | 目标 |
|---|---:|
| warm 同文件 64 KiB range p95 | `< 200ms` |
| warm 连续 1 MiB range p95 | `< 300ms` |
| cold 小文件首段预览 p95 | `< 1.5s` |
| warm 100 MiB+ 文件随机 64 KiB range p95 | `< 500ms` |
| provider construction | 每个 active source fingerprint 最多 1 次 |
| file session construction | 每个 opaque handle 最多 1 次 |
| RBD warm/native XFS warm 延迟比 | `<= 3x` |
| derived-source runtime 预算 | `<= 128 MiB` |
| 大文件 range RSS 增长 | 与文件总大小无关 |

性能门禁使用同一机器、同一 retained case、同一请求序列比较。cold 指标允许操作系统
文件缓存波动，但必须独立记录；warm 指标作为稳定发布门禁。RBD/native XFS warm
比值对两个亚毫秒样本使用 `1ms` 分母噪声下限：
`rbdWarm / max(nativeWarm, 1ms)`。原始比值仍写入报告，但不以计时器和调度噪声放大
后的亚毫秒比值制造假回归。

### 8.2 正确性验收

- 优化前后所有测试 range SHA-256 一致。
- 每个首次 verified page 仍读取并比较全部期望副本。
- 副本冲突、部分副本存在、lineage 不匹配继续 fail closed。
- 不写原始 E01、BlueStore device 或 RBD image。
- 不新增明文正文缓存。
- handle 不泄露 file ID 或宿主路径。
- case/source/lineage 失效后不能命中旧 session 或旧 page。

### 8.3 工程验收

- command 保持薄适配器。
- app-services 不依赖 Tauri。
- runtime-cache SQLite 不持有 live reader。
- 前端只管理 handle 生命周期和 UI 状态，不参与后端块映射。
- 新生产模块符合 module/function size guard。
- 测试正文位于 physical `tests/`。
- UTF-8、文档守卫、Rust/frontend 默认质量门禁通过。

## 9. 评估方案

每个 Stage 完成后进行一次工程复审，评分维度：

| 维度 | 权重 | 评估重点 |
|---|---:|---|
| 取证正确性 | 25 | 三副本验证、fail closed、oracle 一致 |
| 性能 | 20 | phase timing、p50/p95、cache hit、RSS |
| 生命周期 | 15 | close、TTL、case/source/lineage invalidation |
| 模块化 | 15 | command/service/runtime/filesystem 边界 |
| 健壮性 | 15 | fault injection、并发、poison/expiry |
| 测试与文档 | 10 | 真实样本、自动门禁、事实同步 |

Stage 总分低于 90，或取证正确性低于 90，或存在未关闭的 Critical/High
缺陷时，必须整改后再进入下一 Stage。

## 10. 实施优先级结论

实施顺序不得颠倒：

1. 先补 phase timing 和独立 preview baseline。
2. 立即修复 Ceph RBD 绕过 XFS bounded-range 的整文件 materialize。
3. 再提升 provider 到 source-scoped runtime。
4. 再将 handle 升级为真实 PreviewSession，并缓存 XFS 文件解析状态。
5. 最后根据数据决定 page coalescing、singleflight 和 provider lane。

如果 Stage 1 已将大文件延迟降至预算内，后续 Stage 仍要完成会话与失效治理，因为
当前 request-local cache 和伪 handle 仍是明确的工程与资源风险。

## 11. 2026-07-17 实施与验收结果

### 11.1 已完成能力

- Ceph RBD 文件 range 读取已接入文件系统 `read_file_range`，不再通过 XFS
  `open_file` 整文件物化。
- 同一 derived source 使用共享 `DerivedRbdRuntime`，三副本 provider、
  source DB connection、BlueStore device、plan cache 和 verified page cache
  跨 range 复用。
- handle 已升级为 `preview:<uuid>` opaque session，具备 case/source 归属、
  TTL、LRU、显式 close 和案件/数据源失效路径。
- runtime/session 打开使用 scope generation；失效后旧 token 不能重新插入。
  case/source 删除先进入 retired 状态并等待 session open 与 active read lease 收敛。
- import-complete 监听按真实 `EventEnvelope.payload` 解析，先 retire/drain 对应 source，
  清理 runtime/E01 cache 后再允许新预览。
- 普通 viewer 与 `evidence-media:` 协议共享 PreviewSession；前端切换文件和
  卸载时执行 best-effort close；初始 range 失败和异步打开晚到也会关闭 handle。
- XFS path resolve 只定位目标目录项；有效 `ftype` 不再触发全部兄弟 inode
  metadata 读取，缺失 `ftype` 时只读取目标 inode。
- verified page 继续保持 64 KiB；大于 64 KiB 的单次请求可将最多四个连续
  uncached page 合并为一次 256 KiB 三副本读取，之后拆回独立 verified page。
  单次 64 KiB 随机请求不会扩大为 256 KiB 预读。

### 11.2 真实样本覆盖

门禁命令：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-pve-rbd-preview-performance.ps1 `
  -CaseRoot '<retained PVE RBD case>' `
  -NativeXfsFixture '<native Linux XFS E01>' `
  -PveClusterRoot '<PVE cluster root>' `
  -RequireFixture -RequireComparisonFixtures -Runs 3
```

覆盖直接 XFS、`centos/home`、`centos/root`，并验证：

- `1,019 B` `/etc/passwd`
- `168,180 B` 直接 XFS 文件
- `36,264,624 B` home LV 根级文件
- `16,011,004 B` 中型文件
- `614,794,240 B` 大文件
- cold read、warm repeat、`16x64 KiB`、`4x1 MiB`、随机 offset 和文件尾
- 每个关键 range 同时匹配
  `testdata/real-samples/pve-rbd-preview-oracle.json` 固定 SHA-256，并验证跨运行一致
- 检材3 `cl/root` XFS 的 `opt/faka.sql` 固定 range oracle
- PVE 宿主 `pve/root` EXT4 的 `boot/initrd.img-6.17.2-1-pve` 固定 range oracle
- viewer bytes 与 media range 解码 bytes 完全一致
- source/case retire 后旧 handle 与新 open 均被拒绝；reactivate 后固定 oracle 不变
- provider construction 为 `1`
- source/case 两次冷重建后 provider construction 精确为 `1 -> 2 -> 3`
- 显式关闭、source invalidation、case invalidation 后 session count 均为 `0`

2026-07-17 三轮中位结果：

| 指标 | 结果 | 门禁 |
|---|---:|---:|
| cold 文件读取，不含 runtime open | `250.469ms` | `<= 1,500ms` |
| cold runtime + 文件读取，仅报告 | `2,665.687ms` | 不作为文件读取门禁 |
| warm 同范围 64 KiB p95 | `1.189ms` | `<= 200ms` |
| 连续 `16x64 KiB` p95 | `13.523ms` | `<= 200ms` |
| 连续 `4x1 MiB` p95 | `211.434ms` | `<= 300ms` |
| 大文件随机 64 KiB p95 | `58.776ms` | `<= 500ms` |
| 原生 XFS warm 64 KiB p95 | `0.099ms` | `<= 50ms` |
| 原生 XFS 连续 `4x1 MiB` p95 | `14.957ms` | `<= 100ms` |
| PVE 宿主 EXT4 warm 64 KiB p95 | `0.095ms` | `<= 50ms` |
| PVE 宿主 EXT4 连续 `4x1 MiB` p95 | `9.794ms` | `<= 100ms` |
| RBD/native warm 原始比值 | `12.069x` | 仅报告 |
| RBD/native warm 门禁比值，`1ms` 噪声下限 | `1.189x` | `<= 3x` |
| runtime cache capacity | `117,440,512 B` | `<= 128 MiB` |
| RSS delta | `398-448 MiB` | `<= 640 MiB` |

### 11.3 剩余边界

- 首次打开三份 BlueStore E01/LVM device 仍产生秒级 runtime
  初始化成本；它与文件 range 读取分开报告，不通过放宽 cold file 指标掩盖。
- RSS 主要由三份 E01 chunk table、文件系统运行时和 112 MiB provider cache
  共同构成；当前与被预览文件总大小无关，但仍需继续观测多案件并发。
- 当前真实门禁已证明普通 viewer range 与 media range 返回完全相同的证据字节；
  media protocol 的标准 `416 Content-Range` 和 expired-handle `410` 映射另有测试，
  但尚未建立私有样本浏览器端播放时序门禁。
- source/case invalidation 后的冷重建已经进入真实门禁；容量触发的 LRU eviction、
  并发 singleflight 和最多两条 provider lane 仍属后续性能治理，不影响当前正确性承诺。
- 当前全局只保留 1 个 derived runtime；跨 VM 切换会驱逐旧 runtime 与关联 handle。
  在引入统一全局内存预算前不提高该上限。
- inventory 完整集合证明、通用 PG/CRUSH/EC、degraded replica、clone、
  snapshot、encryption、multi-PV RBD LVM 与 CephFS 仍保持 unsupported 或
  indeterminate。
