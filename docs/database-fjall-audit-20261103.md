# SQLite 与 Fjall 混合数据库审计与修复记录

日期：2026-11-03
审计基线：`0c49f1c6 perf: skip catalog mirror during analysis output`

## 审计结论

当前实现仍然把 source SQLite 作为完整明细库，再把文件目录、Timeline、Artifact 和 Parser 记录镜像到 Fjall。导入路径因此产生 SQLite、Fjall 和分析暂存之间的重复写入。Fjall 查询入口曾反复打开 `SingleWriterTxDatabase`，读写并行时会竞争目录锁。

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
| 1 | source SQLite 与 Fjall 的明细双写 | 进行中 | 案例指标与调查步骤状态哈希的 Timeline 总数，以及 Search 覆盖率的 Catalog 总数，在 source projection 完成时读取 Fjall；普通导入探测阶段直接写入当前 attempt 的 Fjall PartitionRecord，枚举开始前即可读取，重新导入优先读取活动 attempt build，避免返回旧 published generation。FileEntry、Artifact、Timeline 及复杂分析路径仍有 SQLite 主写入与 Fjall 投影，尚未完成明细写入迁移。 |
| 2 | Fjall 只读与读写入口重复打开数据库 | 已完成 | 进程内按规范路径共享 `SourceStore`；每个 source 使用独立初始化锁，四个空闲句柄采用 LRU 保留。source/case 删除取得关闭租约；活动句柄会在文件变动前阻止删除。 |
| 3 | 相同记录重复 upsert | 已完成 | Catalog、Timeline、Parser、Artifact 相同记录跳过写事务；Timeline revision 不再因相同内容变化。 |
| 4 | Android、nested、CephFS、旧 Artifact Service 逐条提交 | 已完成 | 改用批量 upsert；导入投影按 512 条分批。 |
| 5 | Analysis 输出误写 source 完成标记 | 已完成 | Source projection 与 analysis projection 使用独立标记；analysis 输出在 SQLite 事务前写入 `building`，完成镜像后再发布 `complete`。 |
| 6 | 未完成的 Fjall 投影被查询读取 | 已完成 | 文件浏览、Timeline、Artifact、Android/Linux Parser、RBD、nested、CephFS 和 Search 路径验证投影状态；缺少完成标记或 analysis 正在更新时使用 SQLite。 |
| 7 | Artifact Storage 对象目录不一致 | 已完成 | source runtime 与导入投影统一使用案件根目录 `artifacts/objects`。 |
| 8 | RBD Header Parser kind 不一致 | 已完成 | `ceph_bluestore_rbd_headers` 使用同名 kind 写入和查询。 |
| 9 | Timeline 查询重复扫描 | 已完成 | Fjall Timeline 在 source projection 中维护总数、按类型总数以及日/月/年计数索引；查询无过滤条件时为 O(1)，跨日范围使用聚合计数，仅在边界日期扫描受限时间索引。旧 source generation 没有计数 marker 时保留兼容扫描，并在下一次完整 Timeline mirror 后建立计数索引。分页仍使用 keyset 和冻结 revision。 |
| 10 | 普通导入与恢复构建发布流程分离 | 已完成 | 首次导入按 operation attempt 写入 Fjall build generation，SyncAll 后写入 generation manifest/seal，并原子改名发布；SQLite operation receipt 与 generation 中的 projection receipt 可共同恢复。seal 只覆盖稳定的 Catalog/Partition 主记录与索引，Analysis、Parser、Meta 和 batch keyspace 的后续写入不会使 generation 失效。应用启动时会校验已发布 generation，并自动完成具有有效 seal 和 batch receipt 的待发布 build。重新导入已发布 source 时先写 building 状态，完成后重新计算并替换稳定 generation seal；若旧句柄仍被使用则拒绝发布。 |
| 11 | 分析输出重复镜像与无关 Parser 重扫 | 已完成 | source migration `source_040_projection_change_log` 由 SQLite 触发器按 Artifact/Timeline ID 合并记录最新增删改序号。投影取得 SQLite source 写屏障后读取一致的 revision 和变更高水位；已有 Fjall revision marker 首次启用时执行一次完整校准，之后仅按 keyset 读取游标至高水位之间的 ID，并批量 upsert/delete。Artifact 删除同步释放对象引用。Fjall 数据和完成游标 `SyncAll` 后才清理已应用变更。旧 marker 自动走完整校准；无变化时 O(1) 检查并跳过。 |
| 12 | Android Parser 双重身份造成重复记录 | 已完成 | SQLite source projection 与 Android 分析共用 field/file、package/file 的稳定 Parser id；相同 payload 使用幂等 upsert。 |
| 13 | Artifact source 列表 keyspace 错用 | 已完成 | 按 source 查询直接分页扫描 Artifact records 主记录 keyspace，不再在未建立的 source index 上查询。 |
| 14 | 已缓存 Fjall 句柄阻止案件目录删除 | 已完成 | source/case 删除设置关闭租约；活动句柄会让删除以可恢复状态退出，关闭期间拒绝该路径的新打开。 |
| 15 | Artifact 元数据对象引用未随记录删除 | 已完成 | 删除过期 Fjall Artifact 时释放其 source owner 引用；相同 Artifact 再次投影时重新激活唯一引用。 |
| 16 | 旧 source schema 的恢复状态写入外键失败 | 已完成 | 案件打开先检查 source schema，再运行会写 recovery status 的扫描；保留明确的 re-import 错误。 |
| 17 | 恢复诊断 scope 错用为 operation 外键 | 已完成 | 持久化诊断状态前确认 operation 行存在；search、manifest、artifact 等独立 scope 不再触发 SQLite 外键失败。 |

## 本轮验证

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
- `cargo test -p app-services --lib`：本轮分区、RBD 清单、预览、BitLocker、仿真和恢复测试均通过；剩余失败为既有 Kubernetes bbolt worker 测试。
- 分区替换、更新和删除都会清理运行时 Catalog 状态，防止旧的 ready/filesystem 值污染新探测结果。
- 活动 import 同时存在旧 published generation 时，分区查询固定读取活动 attempt build；活动 build 缺失会返回错误，不会静默读取旧 generation。
- 删除 SQLite `PartitionRepo`、`DataSourcePartitionRecord`、旧分区迁移模块、分区表迁移脚本和 source 分区表定义；新 source schema 不再创建分区 SQLite 表，测试夹具全部经过 Fjall 分区服务写入。

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
