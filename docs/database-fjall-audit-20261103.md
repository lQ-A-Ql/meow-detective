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
| 1 | source SQLite 与 Fjall 的明细双写 | 待处理 | source 明细仍保存在 SQLite，并在完成阶段镜像到 Fjall。 |
| 2 | Fjall 只读与读写入口重复打开数据库 | 已完成 | 进程内按规范路径共享 `SourceStore`；每个 source 使用独立初始化锁，四个空闲句柄采用 LRU 保留。source/case 删除取得关闭租约；活动句柄会在文件变动前阻止删除。 |
| 3 | 相同记录重复 upsert | 已完成 | Catalog、Timeline、Parser、Artifact 相同记录跳过写事务；Timeline revision 不再因相同内容变化。 |
| 4 | Android、nested、CephFS、旧 Artifact Service 逐条提交 | 已完成 | 改用批量 upsert；导入投影按 512 条分批。 |
| 5 | Analysis 输出误写 source 完成标记 | 已完成 | Source projection 与 analysis projection 使用独立标记；analysis 输出在 SQLite 事务前写入 `building`，完成镜像后再发布 `complete`。 |
| 6 | 未完成的 Fjall 投影被查询读取 | 已完成 | 文件浏览、Timeline、Artifact、Android/Linux Parser、RBD、nested、CephFS 和 Search 路径验证投影状态；缺少完成标记或 analysis 正在更新时使用 SQLite。 |
| 7 | Artifact Storage 对象目录不一致 | 已完成 | source runtime 与导入投影统一使用案件根目录 `artifacts/objects`。 |
| 8 | RBD Header Parser kind 不一致 | 已完成 | `ceph_bluestore_rbd_headers` 使用同名 kind 写入和查询。 |
| 9 | Timeline 查询重复扫描 | 部分完成 | 总数按 512 条 keyset 页读取 Timeline type/time 索引，不再逐条分页计数，也不再为游标打开 SQLite 读取 revision；精确总数及时间过滤仍需扫描匹配前缀的索引记录。 |
| 10 | 普通导入与恢复构建发布流程分离 | 已完成 | 首次导入按 operation attempt 写入 Fjall build generation，SyncAll 后写入 generation manifest/seal，并原子改名发布；SQLite operation receipt 与 generation 中的 projection receipt 可共同恢复。seal 只覆盖稳定的 Catalog/Partition 主记录与索引，Analysis、Parser、Meta 和 batch keyspace 的后续写入不会使 generation 失效。应用启动时会校验已发布 generation，并自动完成具有有效 seal 和 batch receipt 的待发布 build。重新导入已发布 source 时先写 building 状态，完成后重新计算并替换稳定 generation seal；若旧句柄仍被使用则拒绝发布。 |
| 11 | 分析输出重复镜像与无关 Parser 重扫 | 部分完成 | SQLite Artifact/Timeline revision 与最大 rowid 未变化时跳过投影；revision 比较不再额外执行全表计数。变化时同步新增、更新和删除，并释放旧对象引用。变化后仍扫描全部 Artifact、Timeline，并校验/登记现存对象。 |
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

仍待处理：source SQLite 与 Fjall 的完整明细双写迁移。分析输出按 revision 跳过未变化扫描；revision 改变后仍进行全量 Artifact/Timeline 对照。性能基准尚未运行，当前测试只证明行为正确，不代表已有导入提速数据。
