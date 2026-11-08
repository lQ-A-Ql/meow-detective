# SQLite 与 Fjall 混合数据库审计与修复记录

日期：2026-11-08
审计基线：`0c49f1c6 perf: skip catalog mirror during analysis output`

## 审计结论

当前 source 格式为 `source_041_fjall_detail_ownership`，Catalog 记录 schema 为 v2。source SQLite 只保存控制信息、Graph 以及独立领域状态，不创建 `file_entries`、`artifacts`、`timeline_events`。三类明细由 Fjall 保存；枚举和分析使用的 SQLite staging 是中间工作区。FJ-12 仍在实施，业务读取、独立重提取事务及故障恢复需要按下表逐项验收。

已确认的主要问题如下：

1. **source 明细双写**：分区暂存库先写 SQLite，合并到 source SQLite 后再整库写入 Fjall。
2. **句柄生命周期影响并发与删除**：只读与读写共享同一 Fjall 进程句柄；数据源与案件删除必须关闭空闲缓存并拒绝活动句柄。
3. **重复镜像没有内容去重**：相同记录会重新删除并写入索引，Timeline revision 也会重复递增，批次收据持续增长。
4. **多处逐条写入**：Android、nested、CephFS 和旧 Artifact Service 曾逐条提交 Fjall transaction，现已批量提交。
5. **投影发布标记范围过大**：分析输出路径可能写入 `source_projection_complete`，将没有 Catalog 的分析投影误认为完整 source 投影。
6. **部分投影可能被查询读取**：读取路径需要检查完成标记，并在 Analysis 输出更新期间回退 source SQLite。
7. **Artifact Storage 路径分裂**：运行时 source 句柄使用 `source.fjall/artifacts/objects`，投影使用案件根目录的 `artifacts/objects`。
8. **RBD Header Parser kind 映射缺失**：表投影与查询必须使用一致的 Parser kind。
9. **Timeline 查询重复扫描**：精确总数需要扫描完整索引；原实现另有逐条分页和 SQLite revision 打开成本。
10. **恢复链与普通导入路径分离**：普通导入直接写已发布 `source.fjall`，恢复服务却按 build、seal、manifest、publish 流程检查目录；同时 Analysis 在 source 发布后还会写同一 Fjall generation，完整目录 seal 随后失效。
11. **Artifact 按 source 列表 keyspace 不匹配**：source 列表必须扫描主记录 keyspace；类型与对象查找走索引。
12. **恢复扫描与 source schema 检查顺序颠倒**：恢复扫描先写控制库状态；旧 source schema 尚需迁移时，诊断状态写入会先于 typed re-import 错误发生。

## 修复状态

| 编号 | 修复项 | 状态 | 结果 |
|---|---|---|---|
| 1 | source SQLite 与 Fjall 的明细双写 | 进行中 | 新 source schema 不创建三类明细表。枚举 staging 和 Analysis worker 分批写入 Fjall；Logical 导入已验证 Catalog、Artifact、Timeline、Graph、Search 和 generation seal。独立重提取的跨领域原子替换、剩余直接 SQL 查询及完整恢复矩阵仍需验收。 |
| 2 | Fjall 只读与读写入口重复打开数据库 | 已完成 | 进程内按规范路径共享 `SourceStore`；每个 source 使用独立初始化锁，四个空闲句柄采用 LRU 保留。source/case 删除取得关闭租约；活动句柄会在文件变动前阻止删除。 |
| 3 | 相同记录重复 upsert | 已完成 | Catalog、Timeline、Parser、Artifact 相同记录跳过写事务；Timeline revision 不再因相同内容变化。 |
| 4 | Android、nested、CephFS、旧 Artifact Service 逐条提交 | 已完成 | 改用批量 upsert；导入投影按 512 条分批。 |
| 5 | Analysis 输出误写 source 完成标记 | 已完成 | Source projection 与 analysis projection 使用独立标记；analysis 输出在 SQLite 事务前写入 `building`，完成镜像后再发布 `complete`。 |
| 6 | 未完成的 Fjall 投影被查询读取 | 进行中 | 新格式要求未完成 generation 返回明确的未就绪结果。需要消除 facade 返回空值后由 Repository 直接读取 Fjall 的入口，并验证查询不会观察到部分替换结果。 |
| 7 | Artifact Storage 对象目录不一致 | 已完成 | source runtime 与导入投影统一使用案件根目录 `artifacts/objects`。 |
| 8 | RBD Header Parser kind 不一致 | 已完成 | `ceph_bluestore_rbd_headers` 使用同名 kind 写入和查询。 |
| 9 | Timeline 查询重复扫描 | 已完成 | Fjall Timeline 在 source projection 中维护总数、按类型总数以及日/月/年计数索引；查询无过滤条件时为 O(1)，跨日范围使用聚合计数，仅在边界日期扫描受限时间索引。旧 source generation 没有计数 marker 时保留兼容扫描，并在下一次完整 Timeline mirror 后建立计数索引。分页仍使用 keyset 和冻结 revision。 |
| 10 | 普通导入与恢复构建发布流程分离 | 已完成 | 首次导入按 operation attempt 写入 Fjall build generation，SyncAll 后写入 generation manifest/seal，并原子改名发布；SQLite operation receipt 与 generation 中的 projection receipt 可共同恢复。seal 只覆盖稳定的 Catalog/Partition 主记录与索引，Analysis、Parser、Meta 和 batch keyspace 的后续写入不会使 generation 失效。应用启动时会校验已发布 generation，并自动完成具有有效 seal 和 batch receipt 的待发布 build。重新导入已发布 source 时先写 building 状态，完成后重新计算并替换稳定 generation seal；若旧句柄仍被使用则拒绝发布。 |
| 11 | 分析输出重复镜像与无关 Parser 重扫 | 进行中 | SQLite 明细变更日志及其触发器已删除。Artifact 和 Timeline 原生 revision 跟随有效写入变化，相同记录跳过写入；还需消除分析完成路径中的整域计数读取、对象重扫和 Parser 重扫。 |
| 12 | Android Parser 双重身份造成重复记录 | 已完成 | SQLite source projection 与 Android 分析共用 field/file、package/file 的稳定 Parser id；相同 payload 使用幂等 upsert。 |
| 13 | Artifact source 列表 keyspace 错用 | 已完成 | 按 source 查询直接分页扫描 Artifact records 主记录 keyspace，不再在未建立的 source index 上查询。 |
| 14 | 已缓存 Fjall 句柄阻止案件目录删除 | 已完成 | source/case 删除设置关闭租约；活动句柄会让删除以可恢复状态退出，关闭期间拒绝该路径的新打开。 |
| 15 | Artifact 元数据对象引用未随记录删除 | 已完成 | 删除过期 Fjall Artifact 时释放其 source owner 引用；相同 Artifact 再次投影时重新激活唯一引用。 |
| 16 | 旧 source schema 的恢复状态写入外键失败 | 已完成 | 案件打开先检查 source schema，再运行会写 recovery status 的扫描；保留明确的 re-import 错误。 |
| 17 | 恢复诊断 scope 错用为 operation 外键 | 已完成 | 持久化诊断状态前确认 operation 行存在；search、manifest、artifact 等独立 scope 不再触发 SQLite 外键失败。 |

## 本轮验证

## 第一项细化验收表

验收基准：普通导入、Logical 导入、Analysis 输出和独立重提取均直接写入 Fjall；所有 Catalog、Artifact、Timeline 消费者读取 Fjall；source SQLite 不保存这三类明细。取消、失败、重复执行、案件重新打开和来源隔离保持正确，查询及导入工作区具备资源边界。

| 验收编号 | 领域 | 验收条件 | 当前状态 | 代码提交 | 已执行验证 | 剩余边界 |
|---|---|---|---|---|---|---|
| FJ-01 | 分区元数据 | PartitionRecord 只写 Fjall；旧 SQLite 分区仓库、表和迁移脚本不存在 | 已完成 | `d381c766` | `cargo test -p persistence-fjall`、分区查询测试 | 无 |
| FJ-02 | 镜像分区 Catalog | staging root folding 在 SQLite 未提交事务中完成；Fjall SyncAll 成功后才提交 SQLite | 已完成 | `981d30b2`、`e17ea3f8` | `staging_merge_test` 12 项 | Fjall 写入失败后的 build 清理依赖导入失败收尾 |
| FJ-03 | Logical Catalog | LogicalDirectory/Archive 枚举事务先写 Fjall，再提交 SQLite；最终 generation 不重复 Catalog 镜像 | 已完成 | `daa896d8` | Logical import 回归 | 直接调用旧枚举 API 的外部调用者仍保留 SQLite-only 行为 |
| FJ-04 | Analysis worker | Worker rows 在 SQLite 未提交事务内合并；Artifact/Timeline 先写 Fjall；成功后标记 merged | 已完成 | `e6ec91f9` | staging Analysis merge 回归、Logical import 回归 | 非导入独立输出入口单独验收见 FJ-05 |
| FJ-05 | 独立重提取 | Artifact、Timeline 和 checkpoint 原子提交；失败与取消保留完整旧结果 | 进行中 | — | 需要使用当前格式运行原子替换回归 | 单独提交 Fjall 后再提交 SQLite 不能满足此条件 |
| FJ-06 | 案例指标 | Catalog、Artifact、Timeline 计数优先读取 Fjall | 已完成 | `5782684c` | app-services 指标回归 | 未完成 generation 使用 SQLite fallback |
| FJ-07 | Analysis candidate | evidence candidate collection 优先读取 Fjall Catalog | 已完成 | `4ee39926` | candidate ordering/discovery 回归 | 旧 source 或未完成 projection 使用 SQLite fallback |
| FJ-08 | HTML 报告计数 | source file count 优先读取 Fjall Catalog | 已完成 | `c4b642c5` | `cargo check -p app-services --lib` | 完整报告行仍需继续迁移 |
| FJ-09 | JSON/HTML 完整行 | Artifact、Timeline 导出行全部来自 Fjall 分页 | 已完成 | `b7fd1477` | report 19 项回归、Clippy | 仅无案件路径的内存测试保留 SQLite-only legacy API |
| FJ-10 | Graph provenance | Artifact parser/version provenance 来自 Fjall Artifact records | 已完成 | `99bc8138` | `cargo check -p app-services --lib`、Clippy | Graph edge/node 本身仍属于独立 Graph projection |
| FJ-11 | 文件浏览读取 | tree、rows、jump、preview metadata 读取 Fjall Catalog | 已完成 | `304a83b5`、`26d61543` | `cargo check -p app-services --lib`、Clippy | 未完成或未发布 projection 保留 SQLite fallback |
| FJ-12 | SQLite 明细删除 | source SQLite 不再保存 FileEntry、Artifact、Timeline 明细，全部业务和恢复路径通过验收 | 进行中 | `f01fb315`，后续批次实施中 | 当前格式 schema、native detail、分区合并和 Logical 导入定向回归 | 见 FJ-12 子验收表 |

第一项整体处于进行中。FJ-12 必须全部通过后才能标记完成；历史验证不能代替当前格式的业务与故障回归。

### FJ-12 子验收表

| 编号 | 验收条件 | 状态 | 直接证据或剩余工作 |
|---|---|---|---|
| FJ-12-A | 新 source 不创建三类 SQLite 明细表；旧明细布局直接拒绝且不修改数据 | 通过 | `source_migration_version` 3 项；`source_041_fjall_detail_ownership` |
| FJ-12-B | 三类明细完整写入 Fjall；只读连接拒绝修改；未知加密状态和分区信息保持完整 | 通过 | `fjall_source_detail`；`FileEntryRecord.encrypted` 保存 `None/Some(false)/Some(true)` |
| FJ-12-C | 多分区合并、根目录合并、重复执行及跨来源拒绝 | 定向通过 | `staging_merge_folds_roots_and_replays_multiple_partitions_without_duplicates`；NTFS 与进程终止矩阵待验证 |
| FJ-12-D | path/id 文件任务分页保持二进制顺序、稳定快照和来源隔离 | 通过 | `file_feed_uses_binary_path_and_id_order_with_stable_snapshot` |
| FJ-12-E | Artifact 属性、EVTX 排序、Linux 主机筛选以及跨页完整性 | 定向通过 | 1,301 条 native Artifact、SQLite 时间排序 oracle、主机属性筛选 3 项；其余页面入口待迁移 |
| FJ-12-F | Artifact/Timeline 更新与索引、计数、revision 同时提交 | 通过 | 原生写计划持有写事务；重复 ID、相同数量替换、并发更新与失败保持旧状态的回归 |
| FJ-12-G | Logical 导入从枚举至 Search、Artifact、Timeline、Graph 和 seal 均可达 | 定向通过 | `logical_import` 2 项；Archive、RAW/E01、派生来源待验证 |
| FJ-12-H | 独立分析和流式重提取的输出与 checkpoint 同时提交 | 进行中 | Fjall source 已使用临时 generation 完成 Artifact/Timeline 替换、checkpoint 与 generation 指针同事务提交；EVTX、Browser 和 plugin 的 SQLite-only 测试夹具及大结果资源验收仍需迁移 |
| FJ-12-I | 报告、Graph、MCP、Android、预览、恢复、索引及领域聚合全部消除 source 明细 SQL | 未完成 | 按实际服务入口迁移并验证；保留 SQLite control/Graph/领域状态查询 |
| FJ-12-J | 未完成 generation 查询、重新打开、取消、失败重试、source 删除和进程终止 | 未完成 | 完成故障与生命周期矩阵，验证不会遗漏结果或读到部分更新 |
| FJ-12-K | 原生计数、排序分页及导入过程满足内存和 I/O 边界 | 进行中 | Catalog/Artifact 原生计数与 Catalog 二进制 ID 分页已加入；剩余 Repository 全量收集、Timeline 全源替换内存增长及对象重扫仍需消除 |
| FJ-12-L | 完整相关测试、严格 Clippy、格式检查及架构检查通过；文档和分批提交完整 | 进行中 | 模块和函数大小检查通过；完整 SQLite/App Services 测试仍在处理 |

2026-11-07 当前验证记录：

- `f032609b` 保存来源归属、明细原生写入、取证指纹、分区 staging 和 Logical 导入修复。
- Catalog 计数、文件数量和 revision 随记录及索引在同一写事务中更新；Artifact 数量同样随原生事务更新。导入任务计数使用文件数量，索引分页使用二进制 ID 索引，祖先查询通过稳定快照逐级读取。
- CephFS published catalog 校验逐条比对原生记录，保留 ID、父目录、来源、路径、名称、类型、大小及总数校验；7 项回归通过。
- 共享数据库注册表按底层数据库状态管理生命周期，Repository 克隆与读取快照均保持所有权。关闭与缓存驱逐同时检查底层持有者。
- Fjall worker 退出计数完成后，其闭包仍可能短暂持有文件句柄。仓库内依赖补丁等待 worker 完整退出，确保 Windows generation 发布可以立即改名。原生存储完整测试、进程终止矩阵及 32 次连续发布通过。
- SQLite 完整包测试执行至全部目标，14 个目标失败；其中 CephFS 当前格式校验已修复并通过，其余目标涉及旧 source 明细布局、staging 与索引恢复测试，继续迁移和验证。
- 浏览器预加载的 Chromium 密钥文件、Android APK 与 package-restrictions 查询在原生 source 上改用 Fjall Catalog 访问；SQLite 查询仅保留 staging/旧格式路径。
- 未完成 generation 现在由 source storage 直接返回 `SourceNotReady`，文件浏览、Artifact 聚合、Timeline 分页和计数入口不再用空值回退到明细仓库。
- 独立 Analysis 输出在存在 Fjall source 时使用临时 build generation 完成 Artifact/Timeline 替换；SQLite checkpoint 与 generation 指针在同一事务提交，失败时自动清理 build 目录并保留旧 generation。Artifact forensic fingerprint 在同一 SQLite 事务写入。

### 2026-11-03

- `cargo fmt --all`
- `cargo check -p app-services`
- `cargo test -p persistence-fjall --quiet`
- `cargo test -p app-services --test file_browser_fjall --test timeline_fjall --test storage_runtime --quiet`
- `cargo test -p app-services --lib rbd_header_lookup_prefers_bounded_fjall_parser_projection --quiet`
- `cargo test -p app-services --test cluster_import_queries --test android_analysis --test cephfs_object_reader --test nested_evidence --quiet`
- `cargo test -p app-services --test analysis_platform_orchestration --quiet`
- `cargo test -p app-services --test android_analysis --quiet`
- `cargo test -p app-services --test import_source_publication --quiet`
- `cargo test -p app-services --test storage_recovery --quiet`
- `cargo test -p app-services --test artifact_storage_service batch_reference_release_can_be_reactivated_by_reprojection --quiet`
- `cargo test -p app-services --test case_service_test delete_case_requires_fjall_source_handles_to_close --quiet`
- `cargo test -p app-services --test case_service_test delete_data_source_in_removes_isolated_storage_and_writes_audit_log --quiet`
- `cargo test -p app-services --test case_service_test open_case_rejects_ready_source_schema_that_requires_migration --quiet`
- `cargo test -p app-services --lib analysis_projection_removes_deleted_rows_and_skips_unchanged_replays --quiet`
- `cargo test -p app-services --lib output_replacement_commits_artifacts_timeline_and_checkpoint_atomically --quiet`
- `cargo test -p app-services --lib logical_import_post_pipeline_indexes_marker_and_extracts_artifact -- --nocapture`
- `cargo test -p persistence-fjall --test source_generation`
- `cargo test -p persistence-sqlite source_projection_change -- --nocapture`
- `cargo test -p app-services --test storage_recovery startup_recovery_publishes_a_sealed_import_generation_and_finishes_its_operation -- --nocapture`
- `cargo test -p persistence-fjall --test process_recovery child_process_termination_matrix_reopens_each_durable_boundary -- --nocapture`
- `cargo test -p app-services --lib analysis_projection_removes_deleted_rows_and_skips_unchanged_replays -- --nocapture`
- `cargo test -p app-services --lib`：1185 项通过、2 项失败、19 项忽略；`etcd_bolt_parser_reads_bucket_geometry_and_redacts_values` 单独运行仍以 `Bolt(WorkerFailed)` 失败，`parsers_reject_unsafe_yaml_and_corrupt_bolt_input` 单独运行通过。
- `cargo check -p app-services`
- `cargo clippy -p persistence-fjall -p app-services --lib -- -D warnings`
- `cargo fmt --all -- --check`
- `cargo test -p persistence-sqlite --quiet`
- `cargo test -p app-services --test case_service_test --quiet`
- `cargo test -p app-services --test artifact_storage_service --quiet`
- `powershell -ExecutionPolicy Bypass -File scripts/check-module-size.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/check-rust-function-size.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/check-rust-test-layout.ps1`（仍报告 `apps/desktop/src-tauri/src/commands/import/background_job.rs` 的源码内测试布局问题）
- `powershell -ExecutionPolicy Bypass -File scripts/check-doc-archive.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1`
- `git diff --check`

### 2026-11-04

- `cargo check -p app-services --lib -p persistence-fjall`
- `cargo test -p persistence-fjall`
- `cargo test -p app-services --test partition_query_service`
- `cargo test -p app-services --lib`：1185 项通过、2 项失败、19 项忽略；分区、RBD 清单、预览、BitLocker、仿真和恢复测试均通过，剩余失败为既有 Kubernetes bbolt worker 测试。
- `cargo test -p persistence-sqlite`：159 项通过、1 项忽略；新 source schema 不创建 SQLite 分区表。
- `cargo clippy -p persistence-sqlite -p persistence-fjall -p app-services --lib -- -D warnings`
- 分区替换、更新和删除都会清理运行时 Catalog 状态，防止旧的 ready/filesystem 值污染新探测结果。
- 活动 import 同时存在旧 published generation 时，分区查询固定读取活动 attempt build；活动 build 缺失会返回错误，不会静默读取旧 generation。
- 删除 SQLite `PartitionRepo`、`DataSourcePartitionRecord`、旧分区迁移模块、分区表迁移脚本和 source 分区表定义；新 source schema 不再创建分区 SQLite 表，测试夹具全部经过 Fjall 分区服务写入。
- 普通导入在 Enumeration/Merge 完成后立即发布 Catalog Fjall generation；Analysis 阶段只执行 Analysis projection，预发布阶段不再写空的 Timeline、Artifact 和 Parser 镜像。
- staging merge 现在在未提交的 SQLite 事务中完成 root folding，先将最终 Catalog batch 写入 Fjall 并完成 `SyncAll`，随后才提交 SQLite 投影；取消、冲突和写入失败不会标记 staging merged。
- Analysis worker merge 现在在未提交的 SQLite 事务中合并 Artifact/Timeline，使用独立 worker attach alias，将最终 Analysis records 写入 Fjall 并 `SyncAll` 后才提交 SQLite；worker 只有在两边成功后才标记 merged。
- LogicalDirectory/LogicalArchive 的枚举和 Analysis 使用当前 source import generation；完整镜像结束后更新 generation seal，避免分析阶段另建冲突目录。
- 独立 Analysis extraction/re-extraction 的输出事务也在 SQLite commit 前写入当前 Fjall generation；纯内存单元测试在没有案件存储时保持 SQLite-only，实际案件路径统一 Fjall-first。
- 案例指标的 Catalog、Timeline、Artifact 计数已优先使用已发布 Fjall projection；仅在 generation 或完成标记缺失时读取 SQLite。
- Analysis evidence candidate collection now reads the completed Fjall Catalog before using the source SQLite fallback.
- Case HTML report file counts now use completed Fjall Catalog counts before SQLite fallback; report row and Graph provenance migration remains open.

### 2026-11-04

- `case_service::get_case_metrics_for_case` 与 `step_recorder::compute_case_state_hash` 在 source projection 和 Timeline count marker 均有效时读取 Fjall O(1) 总数；缺少 Fjall 目录或投影未完成时读取 SQLite。
- Search 覆盖率在 source projection 完成时通过 Fjall Catalog 分页计算目录总数，并跳过 SQLite source 打开；旧 source 或未完成投影才读取 SQLite。
- source 全量投影直接使用当前 attempt 的 `PartitionRecord` 执行 source 级替换，主记录、source/index/type index 与 tombstone 在一个 Fjall 批次中更新；source 投影完成标记写入前完成此步骤。
- `partition_replace_removes_stale_rows_and_preserves_other_sources` 与 `partition_projection_replaces_stale_source_rows` 覆盖 source 隔离、过期删除、索引更新和重复投影。
- 新增 `case_metrics_and_search_coverage_use_published_fjall_counts`，验证案例指标读取 Fjall 的 Timeline 总数，Search 覆盖率读取 Fjall 的 Catalog 总数。
- `cargo test -p persistence-sqlite`
- `cargo test -p persistence-fjall --test timeline`
- `cargo test -p app-services --lib analysis_projection_removes_deleted_rows_and_skips_unchanged_replays -- --nocapture`
- `cargo test -p app-services --lib logical_import_post_pipeline_indexes_marker_and_extracts_artifact -- --nocapture`
- `cargo test -p app-services --test analysis_platform_orchestration --test android_analysis --test import_source_publication --test storage_recovery`
- `cargo clippy -p persistence-sqlite -p persistence-fjall -p app-services --lib -- -D warnings`
- `cargo fmt --all -- --check`
- `powershell -ExecutionPolicy Bypass -File scripts/check-module-size.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/check-rust-function-size.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/check-doc-archive.ps1`
- `git diff --check`

仍待处理：FileEntry、Artifact、Timeline 的主要业务写入仍先写 source SQLite，再投影到 Fjall；分析候选发现、报告、图查询、搜索输入及复杂聚合仍读取 SQLite。完成完整迁移前，需要逐域替换这些读取与写入入口，并保留明确的 reference/control 表。性能收益仍需固定数据集正式测量，不根据实现路径推定。
