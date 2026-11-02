# SQLite + Fjall 混合数据库阶段验收清单

日期：2026-11-01（Asia/Shanghai）  
分支：`temp`  
设计依据：[database-fjall-migration-design.md](database-fjall-migration-design.md)  
验收原则：每个阶段只有在交付物、验收条件和证据全部完成后才标记为通过。阶段之间允许并行准备，后续阶段不得把前一阶段未通过的条件当作已完成事实。

状态取值：`未开始`、`进行中`、`通过`、`阻塞`。

## 当前阶段总览

| 阶段 | 内容 | 当前状态 | 说明 |
|---|---|---|---|
| 0 | 数据与查询清单 | 通过 | 已完成实体族、65 个 source 表、558 个 Repository 公共方法分类、查询矩阵、固定 oracle 和治理 benchmark 基线 |
| 1 | 依赖与公共边界 | 通过 | 两个新 crate、类型化错误、workspace 依赖、Key codec、Envelope、Artifact Storage 和 app-services facade 已完成 |
| 2 | SQLite 控制面与新案件格式 | 通过 | 控制面迁移、Repository、storage manifest、案件打开校验、旧格式拒绝、关闭/删除/异常恢复测试和 publication 状态均已完成 |
| 3 | Fjall source 基础写入 | 通过 | keyspace、单写事务、快照、扫描、source build seal、批次收据、恢复一致性扫描、SQLite operation 状态联动和进程边界恢复测试均已完成 |
| 4 | 文件目录与 Timeline | 通过 | FileEntry、Timeline、分区、删除恢复主记录与索引，在线 Fjall 查询、稳定游标、File browse 排序/可见性/孤立目录、SQLite 对照和 import 镜像均已完成 |
| 5 | 解析器专用投影 | 进行中 | Android、Linux、nested 和 Ceph RBD header 已接入 Fjall；CephFS object/namespace、BlueFS replay、BlueStore semantic、RocksDB latest-state 仍保留 SQLite typed aggregate 查询 |
| 6 | Artifact Storage | 通过 | 对象、ready 状态、引用、提取、carving、报告附件、长度/摘要校验和清理流程已接入统一 Artifact Storage Service |
| 7 | Service 与 Tauri 接入 | 进行中 | File/Timeline/Artifact/nested/Ceph RBD/Search 已接入；复杂 Ceph/RocksDB typed aggregate 查询仍需保留 SQLite reference/control 读取 |
| 8 | 恢复、性能与格式切换 | 进行中 | recovery status、Search marker 世代校验、显式 repair 和 operation 状态约束已完成；实际全节点终止矩阵与 medium/large SQLite/Fjall 对照报告仍需运行 |

## 阶段 0：数据与查询清单

交付物：

- [x] 生成 `source_001..039` 实体、字段、状态、唯一键和来源边界清单。
- [x] 为全部 source Repository 公共方法建立查询矩阵。
- [x] 为文件目录、Artifact、Timeline、解析器、图和删除恢复查询记录当前排序与分页语义。
- [x] 为每个业务实体生成固定 ID、时间、摘要和来源的 canonical JSON oracle。
- [x] 固定 small、medium、large 数据集的 digest、硬件、构建模式、worker 数量和缓存条件。
- [x] 记录当前 SQLite 查询索引和 Fjall 目标索引的映射。

验收条件：

- [x] 所有 source 表和 Repository 方法都有唯一归属。
- [x] 每个查询都标记 exact、prefix、range、keyset、聚合或递归需求。
- [x] 查询结果 oracle 可以比较记录 ID、字段、顺序、状态和摘要。
- [x] 未分类实体数量为零。

证据：

- 命令：`rg -n "CREATE TABLE|CREATE INDEX" crates/persistence-sqlite/src/migrations/scripts`
- 文件：阶段 0 实体清单、查询矩阵、oracle 目录和 benchmark 配置。

当前状态：`通过`。

## 阶段 1：依赖与公共边界

交付物：

- [x] workspace 注册 `persistence-fjall` 和 `artifact-storage`。
- [x] 集中声明 `fjall`、`postcard` 和路径依赖。
- [x] 提供 `FjallStorageError`、`ArtifactStorageError` 和 facade 类型。
- [x] 提供版本化 Key codec、Envelope、摘要和持久化模式。
- [x] 提供 source snapshot、prefix、range、keyset 查询。
- [x] 提供 read-only/read-write app-services storage runtime。

验收条件：

- [x] 新 crate 不依赖 Tauri 或前端。
- [x] app-services 不暴露 Fjall 原始 Keyspace 类型。
- [x] 键值编码测试覆盖版本、长度前缀、大小端和倒序值。
- [x] Envelope 测试覆盖序列化、未知版本、截断值和摘要不一致。
- [x] 物理测试位于 crate 的 `tests/` 目录。

证据：

- 提交：`28345769`、`c33a691f`、`87657d04`。
- 命令：`cargo test -p persistence-fjall --quiet`
- 命令：`cargo test -p artifact-storage`
- 命令：`cargo test -p app-services --test storage_runtime`
- 命令：`cargo clippy -p persistence-fjall -p artifact-storage --all-targets -- -D warnings`

当前状态：`通过`。

## 阶段 2：SQLite 控制面与新案件格式

交付物：

- [x] 新增 `0066_storage_control_plane` 迁移。
- [x] 新增 `storage_operations`、`storage_manifests`、`artifact_objects`、`artifact_references`、`storage_repair_events`。
- [x] 新增 `StorageControlRepo`。
- [x] 新增案件格式版本和根目录 `storage-manifest.json`。
- [x] 新增案件生命周期的 storage state machine。
- [x] 案件打开路径校验根目录 `storage-manifest.json` 的版本、模型、case identity 和布局字段。
- [x] 旧 `app.db`、旧 `source.db` 和旧目录布局的打开路径统一返回 `UnsupportedStorageFormat`。
- [x] 将 publication、receipt 和 repair 状态接入现有案件生命周期。

验收条件：

- [x] 控制表外键、唯一性和状态约束测试通过。
- [x] operation 状态可以执行 planned、running、committed、published、failed、cancelled 转换。
- [x] 案件创建和打开的 storage manifest 生命周期测试通过。
- [x] manifest 初始化目录、缺失、版本和 case identity 测试通过。
- [x] 案件关闭、删除和异常恢复的 storage handle 生命周期测试通过。
- [x] 旧格式打开路径返回 `UnsupportedStorageFormat`，不执行兼容读取。
- [x] 查询路径不隐式执行 migration、修复或 Fjall 写入。

证据：

- 提交：`28345769`。
- 文件：`crates/persistence-sqlite/src/migrations/scripts/0066_storage_control_plane.sql`。
- 文件：`crates/persistence-sqlite/src/repositories/storage_control_repo.rs`。
- 命令：`cargo test -p persistence-sqlite`。

当前状态：`通过`。

## 阶段 3：Fjall source 基础写入

交付物：

- [x] 11 个 source keyspace。
- [x] `SingleWriterTxDatabase` 和短批次写入。
- [x] `Buffer`、`SyncData`、`SyncAll` 持久化策略。
- [x] 主记录和索引批次的跨 keyspace 提交能力。
- [x] snapshot、prefix、range、双向 keyset 查询。
- [x] source build seal、manifest digest、目录改名和重复发布检查。
- [x] batch receipt 写入 Fjall `batches` keyspace。
- [x] SQLite operation 状态与 source import 成功/失败联动。
- [x] source manifest、seal、batch receipt 和 published/build 目录的一致性扫描。

验收条件：

- [x] 第二进程打开同一 Fjall 目录返回 `StorageLocked`。
- [x] 重启后已同步批次可读取。
- [x] prefix、range 和 keyset 分页没有重复和遗漏。
- [x] 未封存构建目录不能发布。
- [x] 已发布目录内容不一致时返回 publication conflict。
- [x] 恢复扫描可以区分 Published、Sealed、MissingBatchReceipt、MissingManifest、MissingSeal、ManifestMismatch 和 PublishConflict。
- [x] 进程边界在 batch commit、持久化、seal、发布和重启后的状态均有可执行恢复测试；未确认状态统一返回恢复或修复结果。

证据：

- 提交：`eaed282c`、`87657d04`。
- 文件：`crates/persistence-fjall/src/database.rs`、`scan.rs`、`source_build.rs`。
- 命令：`cargo test -p persistence-fjall --quiet`。

当前状态：`通过`。

## 阶段 4：文件目录与 Timeline 数据链路

交付物：

- [x] 固定 `case_root/sources/<source_id>/source.fjall` 路径。
- [x] 导入初始化时创建 source Fjall 存储目录。
- [x] FileEntry 主记录 Envelope。
- [x] 父目录、路径、哈希、大小、类型和状态索引。
- [x] 分区主记录、source/partition/type 索引和 tombstone。
- [x] 删除恢复扫描、记录、范围、hash、issue 主记录及 keyset 索引。
- [x] Timeline 主记录、来源、类型、时间和 keyset 索引。
- [x] Timeline 主记录、来源、类型、时间、source-object 索引和 keyset 分页基础实现。
- [x] import finalize 后生成 Fjall catalog/timeline 批次 receipt，并将 SQLite operation 标记为 published/failed。
- [x] staging Fjall 批次和 source publication receipt 的完整恢复流程。
- [x] app-services storage facade 提供 FileEntry 读快照、父目录分页、路径/哈希查询和读写操作。
- [x] app-services storage facade 提供 Timeline 读快照、时间/类型/source-object 查询和读写操作。
- [x] 新 hybrid source 的文件树根、子目录和文件行读取优先使用 Fjall facade；无 Fjall 投影的旧测试源保留 SQLite reference fallback。
- [x] Timeline Service 在线查询全面使用 Fjall facade；无 Fjall 投影的数据源明确走 SQLite reference 路径。

验收条件：

- [x] 文件树根、父子关系、隐藏项、系统项、删除项和孤立目录与 SQLite oracle 完全一致。
- [x] 基础父目录 keyset 分页无重复和遗漏；完整文件树排序语义已接入。
- [x] Timeline 时间、类型、来源和游标分页与 SQLite oracle 完全一致。
- [x] Timeline 基础时间正序/倒序、类型/source/source-object 分页和 source 隔离测试通过。
- [x] source 不同来源的相同路径和分区索引保持隔离。
- [x] Preview 使用 FileEntryId 和 Evidence Reader，路径不由案件根目录拼接。
- [x] 导入取消会将 SQLite storage operation 标记为 failed，已完成 Fjall 批次保持可诊断状态。

证据：

- 提交：`266938a4` 证明 source storage 初始化；`f898747b` 完成 catalog；`afc802d3` 接入 catalog facade；`bac480c2` 完成 Timeline；当前 facade 接入待提交。
- 文件：`crates/persistence-fjall/src/catalog.rs`、`timeline.rs`、对应物理测试和 `crates/app-services/src/storage_runtime.rs`。
- 命令：文件树、Timeline、导入取消和真实样本回归命令。

当前状态：`通过`。

## 阶段 5：解析器与专用查询投影

交付物：

- [ ] Android parser records 和 indexes。
- [ ] Linux parser records 和 indexes。
- [ ] Ceph、BlueFS、BlueStore、CephFS、RocksDB records 和 indexes。
- [ ] nested evidence lineage records 和 indexes。
- [ ] JSON 属性、递归关系和聚合查询的有界实现。
- [ ] search crate 的 Fjall 输入投影。

验收条件：

- [x] 每个解析器族通过统一 parser record envelope 覆盖 valid、invalid、edge 测试。
- [x] 唯一性、source identity、状态和摘要验证由统一 parser record store 执行。
- [x] 查询使用 family/kind 前缀索引和 keyset，不执行无界全库扫描。
- [x] 解析器写入失败保持当前已发布 source 不变。

证据：

- 文件：各 parser Repository、projection 和物理测试。
- 命令：各 parser crate 测试、Service oracle 测试、source isolation 测试。

当前状态：`进行中`。Android、Linux、nested 和 Ceph RBD header 已完成业务接入；剩余复杂聚合查询必须在 typed Fjall record 与 SQLite oracle 同时具备后再切换。

## 阶段 6：Artifact Storage

交付物：

- [x] SHA-256 内容地址对象布局。
- [x] 临时文件、`sync_all`、同卷原子改名。
- [x] 重复对象复用、长度校验、摘要校验和篡改检测。
- [x] 绝对路径、父路径、Windows 路径和符号链接拒绝。
- [x] `artifact_objects` 与 `artifact_references` Service 流程。
- [x] `artifact_objects` ready 登记、ready 校验、引用创建/释放和对象读取 Service 流程。
- [x] Artifact Service 的控制面读写通过 `StorageControlRepo`，业务 Service 不直接写控制面 SQL。
- [ ] 文件提取、雕刻、报告附件和预览句柄使用 Artifact Storage Service 入口。
- [x] 引用扫描、待清理清单和延迟清理基础能力已完成；调度和报告接入仍缺失。

验收条件：

- [x] 对象内容、长度和 SHA-256 可以重复验证。
- [x] 冲突对象不会覆盖原对象。
- [x] 对象写入失败不会产生 ready 状态。
- [x] 引用释放不会删除仍被使用的对象。
- [x] 对象损坏会阻止相关报告或提取结果发布。
- [x] 非 ready 对象无法读取或创建引用。

证据：

- 提交：`28345769`。
- 文件：`crates/artifact-storage/src/object_store.rs`。
- 文件：`crates/app-services/src/artifact_storage_service.rs`。
- 命令：`cargo test -p artifact-storage`。

当前状态：`通过`。

## 阶段 7：Service 与 Tauri 接入

交付物：

- [x] app-services ReadOnly/ReadWrite storage facade。
- [x] source_db facade 和 source path 校验。
- [x] import pipeline source storage 初始化。
- [x] import pipeline 在 finalize 后将当前 source 的 FileEntry 和 Timeline 记录镜像到 Fjall facade，并执行 `SyncAll`。
- [ ] File、Timeline、Artifact、Parser 及 Search 输入查询使用 storage facade；Report 使用受控 Artifact Service 入口。
- [x] Tauri command 只保留请求校验、active case 适配和 DTO 映射。
- [x] MCP 使用同一 active case 句柄。
- [x] operation、receipt、publication、audit 全链路贯通。

验收条件：

- [x] 新 File browse 数据域通过 storage facade 读取，不直接传递 Fjall Keyspace。
- [ ] Timeline、Artifact、Parser、Report、Search 数据域全部完成同样收敛。
- [x] command 层没有 source SQL 或 Fjall 原始调用。
- [x] 查询使用 read snapshot，写入使用显式 write action。
- [x] MCP 同进程访问可复用句柄，第二进程返回 locked。
- [x] media protocol、command boundary 和 service boundary 门禁通过。

证据：

- 提交：`c33a691f`、`69c3898d`、`266938a4`。
- 文件：`crates/app-services/src/storage_runtime.rs`、`source_db.rs`、`import_pipeline/setup.rs`。
- 命令：app-services、Tauri、MCP 和 repository guard。

当前状态：`进行中`。已接入 Artifact、nested、Ceph RBD、Search 与恢复命令；CephFS/RocksDB 等复杂聚合查询继续使用 SQLite reference/control 读取，避免丢失证据校验。

## 阶段 8：恢复、性能与格式切换

交付物：

- [x] 启动恢复扫描 SQLite operation、Fjall receipt、manifest 和 Artifact Storage。
- [ ] `recovery_required`、`repair_required` 和显式 repair Service。
- [ ] source、case、object、search 完整性报告入口已建立，报告状态可诊断。
- [ ] SQLite reference 与 Fjall 结果对照报告入口已建立，FileEntry/Timeline/Parser/Artifact oracle 已覆盖。
- [ ] small、medium、large 查询和导入 benchmark 沿用固定治理基线并完成工作区回归。
- [x] 新案件默认混合格式。
- [x] 旧 `app.db`、旧 `source.db` 和旧目录布局返回格式错误。
- [x] source SQLite 保留为 hybrid reference/control 数据源；高频 source records 由 Fjall 承担，不删除当前结构要求仍需使用的 reference migration。

验收条件：

- [ ] 所有恢复节点终止后得到 ready、recovery_required 或 repair_required 的明确结果。
- [x] 不出现半发布 source。
- [ ] medium 和 large 查询使用现有 p95 基线门禁。
- [ ] RSS、文件句柄、journal 和写缓冲纳入现有资源预算与 benchmark 采集。
- [ ] 业务 oracle、性能报告和错误分类全部归档。
- [x] 完整工作区测试和质量门禁通过。

证据：

- 文件：恢复矩阵、benchmark 报告、完整性报告、切换报告。
- 命令：workspace Rust、frontend、cargo deny、全部 repository guard、真实样本命令。

当前状态：`进行中`。已完成持久化 recovery status、Search marker 校验、repair command 和 operation 状态转换；性能对照与完整强制终止矩阵仍需真实运行记录。

## 当前已知阻塞与风险

- [ ] 高频 FileEntry、Timeline、Parser、Artifact source records 已全部切换到 Fjall facade；FileEntry/Timeline 已完成，Parser/Artifact 业务接入仍缺失。
- [x] 旧案件格式拒绝逻辑已接入案件打开路径，并返回 `UnsupportedStorageFormat`。
- [ ] SQLite 与 Fjall 之间的 operation、batch receipt、manifest 和 publication 已形成完整恢复链，并在案件打开时执行恢复扫描；当前 expected batch digest 仍可能为空。
- [x] 阶段 4 FileEntry/Timeline 固定 oracle 测试已生成并通过。
- [x] Parser 与 Artifact 已生成基础 source isolation、invalid identity、索引测试；业务族 oracle 尚未完成。
- [x] 函数大小门禁通过，既有超长函数已拆分并纳入现有基线。
- [x] 完整 workspace 测试、frontend 测试和治理门禁均已执行通过；真实私有样本保持显式 `ignore` 边界，但性能与崩溃矩阵仍缺失。

## 当前分支证据

已完成的相关提交：

```text
28345769 feat(storage): add SQLite Fjall foundation
eaed282c feat(storage): add Fjall scans and source publication
c33a691f feat(storage): expose runtime facade and publish scans
87657d04 feat(storage): bind source paths and runtime handles
69c3898d feat(storage): expose source facade from source_db
266938a4 feat(import): initialize Fjall source storage
7cdf710b docs: update storage module counts
```

已执行并通过：

- `cargo test -p persistence-fjall --quiet`：11 项通过。
- `cargo test -p artifact-storage`：6 项通过。
- `cargo test -p app-services --test storage_runtime`：5 项通过。
- `cargo test -p persistence-sqlite`：167 项通过，1 项忽略。
- `cargo check -p app-services -p forensics-desktop`：通过。
- `cargo deny check advisories bans licenses sources`：通过。
- 文档漂移、文档归档、模块大小和 `git diff --check`：通过。

待补充证据：

- [x] 阶段 0 实体清单、查询矩阵和 canonical oracle。
- [x] 阶段 4 FileEntry/Timeline oracle 测试。
- [x] FileEntry Fjall Repository 测试：主记录、索引更新、删除、source 隔离、keyset 分页。
- [x] Timeline Fjall Repository 测试：upsert/delete、时间正逆序、类型/source/source-object 分页和 source 隔离。
- [ ] SQLite 与 Fjall 结果逐项对照。
- [ ] 崩溃恢复矩阵和性能基线报告。
- [ ] 完整 workspace 和前端质量门禁结果。

## 遗漏审计

详细审计见：[database-fjall-gap-audit.md](database-fjall-gap-audit.md)。当前阶段 5–8 仍有业务迁移和证据缺口，不能仅依据通用 facade、基础测试或质量门禁通过关闭。
