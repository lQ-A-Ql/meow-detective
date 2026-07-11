# Runbook: `source_object_id` 关联桥

## 原理

`source_object_id` 是取证数据模型里连接三类实体的关联桥：

- `domain::Artifact.source_object_id: Option<FileEntryId>` —— 该 Artifact 是从哪个证据文件（`FileEntry`）解析出来的。
- `domain::TimelineEvent.source_object_id: String` —— 该时间线事件对应的来源对象 ID（通常是同一个 `FileEntryId`，某些情况下是 `artifact:<artifact_id>` 前缀形式，指向一个 Artifact 而非文件）。

每个生产 extractor（`crates/artifacts-windows`、`crates/artifacts-linux` 下的 parser）在产出 `Artifact`/`TimelineEvent` 时，必须把调用方传入的 `FileEntryId` 原样写回 `source_object_id`。这个字段是后续两条链路的唯一桥梁：

1. **investigative graph 关联**：`app-services/src/timeline_service.rs` 里的 `populate_timeline_event_graph` 在 MACB 时间线投影完成后，为每个 `TimelineEvent` 写一个 `TimelineEvent` 图节点，并且——只有当 `source_object_id` 非空时——写一条 `References` 边指向该 `source_object_id`。`source_object_id` 为空的事件不会产生边，只会产生一条 warning（`"N timeline event(s) skipped because source_object_id was empty"`），不会导致整个投影失败。
2. **跨证据关联分析**：`app-services/src/correlation/` 用图上的 `References` 边做实体解析、聚簇、线索生成（V2/V3 governance snapshot 里的关联统计）。如果某个 extractor 漏填 `source_object_id`，那么它产生的所有 Artifact/TimelineEvent 在关联分析里都是"孤岛"——不会出现在任何簇里，也不会贡献线索计数。

## 强制校验

`crates/app-services/tests/extractor_source_object_id_enforcement.rs` 是这个不变量的测试夹具：对每个已知 extractor（Prefetch、Registry、LNK、RecycleBin 等，用 `fixture_builder` 构造的合成字节样本 + 已 check in 的 tiny registry hive）运行提取，断言产出的每个 Artifact 的 `source_object_id == Some(file_id)`，每个 TimelineEvent 的 `source_object_id == file_id.0`。新增 extractor 时必须在这个文件里补一个对应的断言用例。

运行方式：

```bash
cargo test -p app-services --test extractor_source_object_id_enforcement
```

## 排查"关联分析缺线索/缺聚簇"的步骤

如果用户报告某个已导入的证据文件的痕迹没有出现在关联分析（Correlation Snapshot / V3 governance dashboard 的"关联统计"）里，按以下顺序排查：

1. **确认痕迹本身被正确提取**：在 Artifacts 页面按家族筛选，确认对应的 Artifact/TimelineEvent 行本身存在。如果连原始记录都没有，问题在 extractor 本身，不是关联桥。
2. **检查该记录的 `source_object_id` 是否为空**：
   ```sql
   -- 在案件 app.db 上执行
   SELECT id, source_object_id, event_type FROM timeline_events WHERE source_object_id = '' LIMIT 20;
   SELECT id, source_object_id FROM artifacts WHERE source_object_id IS NULL LIMIT 20;
   ```
   如果命中的正是用户报告的记录，说明产出它的 extractor 存在漏填 `source_object_id` 的缺陷——回到对应 extractor 源码（`crates/artifacts-windows/src/...`、`crates/artifacts-linux/src/...`）检查构造 `Artifact`/`TimelineEvent` 的代码路径，并在 `extractor_source_object_id_enforcement.rs` 里补一个回归断言。
3. **检查 timeline graph 是否已经跑过**：`populate_timeline_event_graph` 只在 MACB 投影"本次新插入了行"（`inserted > 0`）时才会运行；如果时间线事件是通过某个不触发 MACB 投影的路径写入的（例如手工导入的旧数据），graph 节点/边可能从未生成过。检查 `timeline_projection_meta` 表里 `macb` key 的状态，或者直接重跑一次 `project_macb_timeline_sql` + `populate_timeline_event_graph`（目前没有独立的 Tauri 命令重跑 graph 填充，需要走重新导入或直接调用 service 函数）。
4. **检查日志里的 graph 填充 warning/error**：`populate_timeline_event_graph` 失败时会在原有导入流程里降级为 warning（不阻塞导入），日志里会有 `"Timeline graph population failed: ..."`；如果看到这条日志，说明是数据库层面的问题（连接/SQL 错误），不是 `source_object_id` 缺失。
5. **确认关联分析读的是同一批数据**：`correlation::get_correlation_snapshot` 目前有增量缓存路径（`get_correlation_snapshot_incremental`），如果怀疑缓存过期，用 `invalidate_correlation_cache` 强制重算后再核对。

## 新增 extractor 时的清单

- 在提取函数签名里显式接收 `file_id: &FileEntryId`（不要从字符串路径反推）。
- 每个构造出来的 `Artifact` 设置 `source_object_id: Some(file_id.clone())`。
- 每个构造出来的 `TimelineEvent` 设置 `source_object_id: file_id.0.clone()`。
- 在 `extractor_source_object_id_enforcement.rs` 里添加一个使用该 extractor 的测试用例，复用 `assert_source_object_id` / `assert_outcome_source_object_id` 断言帮助函数。
