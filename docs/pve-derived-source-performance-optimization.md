# PVE 派生数据源后台处理与性能优化

**记录日期**: 2026-07-18  
**适用样本**: `E:\pangushi\服务器`  
**适用范围**: Ceph RBD 派生 VM 数据源的 Catalog 物化、Graph、Platform、Artifacts、Search 与 Timeline

## 1. 问题定义

PVE 集群导入完成物理成员和 RBD Catalog 后，派生 VM 文件树已经具备浏览条件。
旧链路仍在同一个导入任务中串行执行 Graph、Platform、Artifacts、Search 和
Timeline，导致前端持续显示导入未完成。用户感知到的阻塞并非全部来自 Catalog，
而是可浏览状态与昂贵后处理没有分离。

保留案件的阶段剖析基线如下：

| 阶段 | 基线耗时 |
|---|---:|
| Catalog | 约 83s |
| Graph | 约 7s |
| Platform | 小于 1s |
| Artifacts | 约 133s |
| Search | 约 150s |
| Timeline | 约 380s |
| 合计 | 约 753s |

这些数字只适用于当前私有样本和测试环境，不构成公开性能承诺。它们用于定位：
Catalog 在约 83 秒后已经可浏览，其余约 670 秒不应继续阻塞前端导入完成状态。

### 1.1 2026-07-18 已验证优化测量

在六成员真实样本回归中，第一轮优化后的完整测试耗时为 `594.31s`，其中编译耗时
`30.31s`。从 `data_source_processing_phases` 读取的阶段耗时如下：

| 阶段 | 已验证耗时 |
|---|---:|
| Catalog | 52s |
| Graph | 6s |
| Platform | 小于 1s |
| Artifacts | 132s |
| Search | 6s |
| Timeline | 74s |

该轮记录的峰值 RSS 为 `806,780,928` bytes，Timeline WAL 峰值约 `279.82MiB`。
Catalog 产出 `114,260` 条记录；Artifacts 扫描 `15,323` 个候选并生成
`32,262` 条 artifact；Search 在 `11,078` 个 eligible 文件中按预算索引 `100`
个；Timeline 生成 `393,952` 条 MACB 事件。该测量发生在 Timeline 有界事务改造
之前，后续结果必须单独记录，不覆盖本基线。

### 1.2 Artifacts 热路径复审

对保留案件的派生 VM source DB 复审后，Artifacts 的主要成本已经定位到完整
Web 根扫描，而不是候选发现或目录遍历：

| 指标 | 结果 |
|---|---:|
| 全部候选 | `15,323` |
| Linux Web Services 候选 | `14,933` |
| `/var/www` 候选 | `14,924` |
| PHP 候选 | `14,923` |
| Web 根候选总字节 | `81,611,578` |
| source read 耗时 | `83,391ms` |
| processing 总耗时 | `92,834ms` |

`/var/www` 覆盖属于取证能力，不能通过跳过 PHP/JSP 或设置候选数量上限换取性能。
现有 XFS file locator 可覆盖 `14,921/14,924` 个 Web 根候选。静态局部性分析显示，
按路径顺序读取时近似 inode page 发生 `3,233` 次切换；按
`partition + inode + path + fileId` 的确定性顺序读取时降为 `181` 次。该结果只证明
优化方向具备充分依据，实际 RBD 耗时改善仍必须由冷样本回归确认。

### 1.3 2026-07-18 冷重放与 Catalog 重建结果

Artifacts 冷重放最终采用 `8MiB` read-plan SQLite cache 且关闭 mmap。严格三副本
逐字节一致性校验、`15,323` 个候选和 `119,326,220` 证据字节覆盖均保持不变：

| 方案 | 测试体耗时 | 峰值 RSS | 结论 |
|---|---:|---:|---|
| compact checksum、8MiB cache、32MiB mmap | `144.74s` | `730MiB` | 基线 |
| 4MiB cache、无 mmap | `157.66s` | `701MiB` | 时间退化，拒绝 |
| 8MiB cache、无 mmap | `130.62s` | `715MiB` | 采用 |

采用方案中 read-plan lookup 为 `7.456s`，设备读取为 `75.290s`，持久化为
`6.829s`；相对基线总时间降低约 `9.8%`，峰值 RSS 降低约 `2.1%`。

独立 Catalog 重建使用只含六个物理成员、零派生源的可丢弃案件副本：

| 方案 | Catalog 物化耗时 | 结论 |
|---|---:|---|
| 500 行提交、默认 WAL checkpoint | `100.314s` | 安全基线 |
| 4,000 行提交、默认 WAL checkpoint | `86.087s` | 有效 |
| 4,000 行 / 16MiB 双上限、64MiB WAL checkpoint | `83.574s` | 采用 |
| 16,000 行 / 16MiB 双上限、64MiB WAL checkpoint | `83.234s` | 收益仅 `0.34s`，拒绝 |

最终 Catalog 仍为 `114,260` 条记录、`15,749` 个目录、`98,511` 个文件和三个
XFS 分区；`/etc/passwd` 预览、deep manifest、`PRAGMA quick_check` 均通过，
进程退出后无 `.build`、`-wal` 或 `-shm` 残留。

### 1.4 2026-07-18 三副本 RBD 预览回归

在保留案件
`artifacts/pve-performance-20260718-r11/pve-cluster-case`
上执行三次真实预览回归。三副本读取改为“先完成各副本计划解析，再并行读取并逐字节比对”，
没有降低副本覆盖或完整性校验强度。三次结果均通过：

| 指标 | 第 1 次 | 第 2 次 | 第 3 次 |
|---|---:|---:|---:|
| 顺序 `4 x 1MiB` p95 | `168.57ms` | `99.67ms` | `99.01ms` |
| 大文件随机 `64KiB` p95 | `77.56ms` | `42.03ms` | `42.19ms` |
| 峰值 RSS | `585MiB` | `577MiB` | `577MiB` |
| provider constructions | `1` | `1` | `1` |
| filesystem constructions | `3` | `3` | `3` |

三次回归均保持固定 byte oracle、媒体范围预览 parity、source/case
invalidation 生命周期断言通过。该结果表明三副本串行读取是此前 `1MiB` 延迟的重要放大因素；
仍需在冷启动和低缓存命中场景继续观察，不将单次热回归结果表述为所有样本的性能承诺。

## 2. 已落地设计

### 2.1 Catalog readiness 与后处理解耦

- `data_sources.import_state='ready'` 只表达派生 source DB 的 Catalog 可浏览。
- Catalog 完成后持久化六个 processing phase：
  `catalog/graph/platform/artifacts/timeline/search`。
- Catalog 标记 `ready`，其余阶段先标记 `pending`。
- 前端可见的集群导入 Job 在派生 Catalog 可浏览后完成。
- 后续处理由 `TaskManager` 管理的独立任务继续执行，不使用无法取消和收敛的裸后台线程。
- 后处理失败不回滚已验证的 Catalog，也不把可浏览数据源重新标记为导入失败。

### 2.2 Timeline 优化

- 文件 MACB 投影和 timeline graph 写入保持既有取证语义。
- graph 分页由 `OFFSET` 改为基于稳定 `id` 的 keyset paging。
- 查询和写入语句复用 prepared statement。
- MACB 与 timeline graph 使用独立完成标记；graph 失败不会把不完整投影永久标为完成。
- MACB 和 graph 均按有界批次提交，降低长事务的取消延迟与 WAL 峰值。
- graph 写入仍是非致命投影；失败必须记录 phase error，不能破坏 Catalog。

### 2.3 Search 优化

- 删除 priority/non-priority 两次 Rust 全表扫描。
- 由 SQL 计算 eligible count，并按 `priority_rank/path/id` 做 keyset paging。
- 达到 100 文件索引上限后停止继续读取证据内容。
- 强制满足 `eligible = indexed + skipped + failed`，避免统计掩盖未处理候选。
- Search index 仍写入派生数据源自己的 index 目录，不污染父物理 source DB。

### 2.4 Linux artifact 读取上限

旧链路可能为每个候选统一构造 128MiB 读取窗口，造成明显读取放大。当前按能力路由：

| 候选类型 | 单候选读取上限 |
|---|---:|
| 小型配置与脚本 | 4MiB |
| 文本、Web、MySQL 日志 | 16MiB |
| Journal、登录记录、Registry preload | 128MiB |

读取上限是资源边界，不改变证据只读语义；超过当前能力范围的内容必须记录截断或
unsupported 诊断，不能静默宣称完整提取。

### 2.5 RADOS 对象计划会话

- 每个只读 BlueStore 父 `source.db` 在派生 RBD runtime 生命周期内只初始化一次
  `CephBluestoreReadPlanSession`，集中校验 semantic scan、BlueStore super、
  RocksDB latest-state、OSD 设备绑定与设备边界。
- 对象级读取仍使用独立只读事务，逐对象校验 object、blob、logical extent、
  physical extent、checksum 与 shared-blob 绑定，不把一次全局校验替代对象完整性校验。
- 候选定位和对象读取计划在同一个对象事务内完成，避免同一对象重复读取 object row。
- targeted read plan 使用计划内本地 `objectOrdinal=0` 关联 checksum rows，不再为每个
  对象执行一次全量 catalog 排名统计；完整 aggregate 的全局 ordinal 契约保持不变。
- 对象计划 SQL 使用连接级 prepared-statement cache；对象计划和已验证字节页继续使用
  有界 LRU cache，不按 VM 总大小增长。
- 三个副本仍分别读取并逐字节比对；该优化不减少副本覆盖，也不改变 sparse-hole 判定。
- Artifacts phase 持久化 `radosReadPlanSessionInitializations`、
  `radosReadPlanSessionElapsedMicros`、plan cache 和设备读取指标。当前三副本样本的
  session 初始化次数必须为 `3`，不能随对象 miss 数增长。

### 2.6 Catalog 隐藏构建与有界提交

- 派生 Catalog 只写入 `source.db.build`，完整性验证、WAL 收敛并切换到 DELETE
  journal 后才原子重命名为 `source.db`。
- 文件系统证据读取发生在 SQLite 写事务之外；提交器跨目录聚合，按
  `4,000` 行或估算 `16MiB` 堆内存双上限提交。
- 隐藏构建库将 WAL 自动 checkpoint 阈值提高到约 `64MiB`，最终发布前仍强制
  `quick_check` 与 `wal_checkpoint(TRUNCATE)`。
- 受控取消、解析失败或发布失败会在连接关闭后删除未发布 build DB 与 sidecar；
  cleanup 失败只记录日志，不覆盖原始错误。
- 进程崩溃遗留的 build DB 在下次干净重试前删除。当前尚未实现持久化 frontier
  恢复，因此崩溃后会重做 Catalog，而不会从半成品继续。

## 3. 后台任务边界

- 后处理任务必须注册到 `TaskManager`，案件关闭和删除能够发出取消并等待收敛。
- 取消令牌由桌面任务贯通到 Artifacts、Search 与 Timeline；长循环按候选、读取块或
  数据库批次检查取消。
- 处理 phase 的状态、错误和 heartbeat 持久化到控制库；前端不需要保持页面打开。
- UI 可以不显示持续进度动画，但后端错误必须可通过数据源 processing summary 和错误抽屉查询。
- 应用重启时只把租约已过期的 `running` phase 收敛为 `failed`；未过期租约保持运行，
  避免第二个应用实例误伤仍存活的 worker。心跳确认失去租约后，阶段不得再发布
  `ready` 结果。随后自动发现
  `ready` Ceph RBD 数据源的 pending/failed/deferred phase 并注册后台恢复任务。

## 4. 测试矩阵

| 测试面 | 验证内容 |
|---|---|
| Catalog readiness | Job 完成时派生文件树、分区和代表文件预览可用 |
| Catalog rebuild | `-CatalogRebuild` 要求零派生源基线，验证 114,260 行、三分区、预览、deep manifest 与零 sidecar |
| 后处理隔离 | Job 完成后非 Catalog phase 可为 pending/running，不影响文件浏览 |
| phase 完整性 | 六个 phase 均持久化，最终状态和 output count 一致 |
| 幂等 | ready Catalog 重开不重复全量物化；ready phase 不重复执行 |
| 中断恢复 | 遗留 running phase 在案件打开时被收敛，stale attempt 不能提交 |
| 取消与删除 | 案件删除先取消并等待后台任务；超时则 fail closed |
| 父库只读 | 三个 BlueStore 父 source DB 的长度、mtime 和边界摘要不变 |
| Search | keyset 无重复/遗漏，统计恒等式成立，达到索引上限后停止证据读取 |
| Timeline | keyset 无重复/遗漏，graph 写失败保持非致命且 phase 可诊断 |
| 内存 | 记录峰值 RSS；不得通过一次性加载完整 VM 文件树或大文件换取时间 |

## 5. 剩余风险与优化顺序

1. 为超大 Catalog 设计持久化 frontier/cursor；恢复信息必须只存在于隐藏 build DB，
   不得暴露部分文件树，且新增写放大必须通过真实样本性能门禁。
2. 为 Search keyset 查询补充与 predicate/order 对齐的 source DB 索引，并用
   `EXPLAIN QUERY PLAN` 锁定不出现大规模临时排序。
3. 保留 Web root 全量覆盖，使用持久化 partition/file locator、候选描述符批量准备和
   inode 局部性排序降低离散 RBD 读取；禁止用跳过 PHP/JSP 文件伪造性能提升。
4. 真实样本回归增加生产 TaskManager 子任务生命周期、取消延迟和案件重开恢复断言。
5. 在最终 phase 完成后再次校验三个 BlueStore 父 source DB 的长度、mtime 与边界摘要。
6. 记录 browseable time、post-processing time 与总耗时三个独立指标，禁止再用单一总耗时代表交互性能。
7. Artifacts phase 记录 `processingRowsPerSec`、`sourceReadAvgMicros`、`rssMb` 和
   `peakRssMb`；外部脚本不得把 cargo/rustc 进程内存相加后冒充测试进程峰值。

### 5.1 冷重放门禁

保留案件性能回归支持显式 `-ColdArtifactReplay`：

- 只清理派生 source DB 的 analysis candidate checkpoint。
- 不修改三个父 BlueStore source DB。
- 第一轮必须对全部未命中候选执行真实 source read，并输出
  `PVE_RBD_ARTIFACT_COLD`。
- 第二轮只重置 Artifacts phase、保留 checkpoint，必须满足
  `sourceReadCount = 0` 且 `checkpointHitCount = scannedCount`。
- 该模式会修改传入的派生案件，仅允许对 disposable retained case 使用。

### 5.2 Catalog 重建门禁

`scripts/check-pve-cluster-import.ps1 -ExistingRbdCaseRoot <case> -CatalogRebuild`
只接受零派生 RBD source 的可丢弃案件副本。该模式在 Catalog 可浏览后停止，不运行
Graph、Artifacts、Search 或 Timeline，用于单独测量前端可浏览时间。

## 6. 验收标准

- 派生 Catalog 可浏览后，前端导入 Job 不再等待全部后处理完成。
- 后处理由受管理任务执行，案件删除不会在任务未收敛时删除目录。
- 文件树、文件预览、RBD byte oracle、Catalog count/digest 和父 source DB 只读断言不退化。
- Search、Timeline 和 Artifacts 的输出与优化前基线一致，失败能够定位到具体 phase。
- 真实样本报告分别给出 browseable、post-processing、总耗时和峰值 RSS。
- 未经真实样本验证的性能改进不得写为已达成指标。
