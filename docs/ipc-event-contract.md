# IPC 事件契约

本文档列出后端到前端的全部 18 个 Tauri 事件 topic，及其 payload 类型来源。事件契约没有代码生成，Rust 端 `crates/transport/src/events/mod.rs` 与前端 `frontend/src/types/events.ts` 的 `EventTopic` 联合类型必须手工保持一致——`scripts/check-event-topic-drift.ps1` 在 CI 中校验两者一一对应。

发送侧统一走 `apps/desktop/src-tauri/src/events/event_bridge.rs` 中的 `emit_*` 辅助函数，每个函数构造一个 `EventEnvelope<T>`（`eventId`/`topic`/`ts`/`payload`）并通过 `app.emit_to("main", topic, event)` 发出。接收侧统一走前端 `frontend/src/lib/events/bus.ts` 的 `EventBus`，按 `EventTopic` 字符串订阅。

## Topic 一览

| Wire topic | Rust `EventTopic` 变体 | 发送函数 | Payload 类型 |
|---|---|---|---|
| `case-opened` | `CaseOpened` | `emit_case_opened` | 匿名 JSON：`{ caseId, caseName }` |
| `case-closed` | `CaseClosed` | `emit_case_closed` | 匿名 JSON：`{ caseId }` |
| `job-created` | `JobCreated` | `emit_job_created` | 匿名 JSON：`{ jobId, name }` |
| `job-started` | `JobStarted` | `emit_job_started` | 匿名 JSON：`{ jobId, detail }` |
| `job-progress` | `JobProgress` | `emit_job_progress` | 匿名 JSON：`{ jobId, progress, detail }` |
| `job-completed` | `JobCompleted` | `emit_job_completed` | 匿名 JSON：`{ jobId, message }` |
| `job-failed` | `JobFailed` | `emit_job_failed` | 匿名 JSON：`{ jobId, error }` |
| `job-cancelled` | `JobCancelled` | `emit_job_cancelled` | 匿名 JSON：`{ jobId, reason }` |
| `job-cancellation` | `JobCancellation` | `emit_job_cancellation` | `JobCancellationDto`（`crates/transport/src/dto/jobs.rs`） |
| `data-source-imported` | `DataSourceImported` | `emit_data_source_imported` | 匿名 JSON：`{ dataSourceId, name, kind, jobId }`（源自 `DataSourceSummaryDto`） |
| `artifact-added` | `ArtifactAdded` | `emit_artifact_added` | 匿名 JSON：`{ artifactId, artifactType }` |
| `timeline-updated` | `TimelineUpdated` | `emit_timeline_updated` | 匿名 JSON：`{ eventCount }` |
| `search-index-progress` | `SearchIndexProgress` | `emit_search_index_progress` | 匿名 JSON：`{ progress, detail }` |
| `partition-progress` | `PartitionProgress` | `emit_partition_progress` | 匿名 JSON：`{ jobId, currentPartition, completedPartitions, totalPartitions, partitionProgress }` |
| `import-phase-progress` | `ImportPhaseProgress` | `emit_import_phase_progress` | `ImportPhaseProgressDto`（`crates/transport/src/dto/import.rs`） |
| `import-partial-result` | `ImportPartialResult` | `emit_import_partial_result` | `PartialResultDto`（`crates/transport/src/dto/import.rs`） |
| `cache-index-status` | `CacheIndexStatus` | `emit_cache_index_status` | `IndexCacheStatusDto`（`crates/transport/src/dto/import.rs`） |
| `performance-report-ready` | `PerformanceReportReady` | `emit_performance_report_ready` | `PerformanceReportDto`（`crates/transport/src/dto/timeline.rs`） |

## 两类 payload

- **匿名 JSON payload**（大多数 topic）：直接用 `serde_json::json!({...})` 构造，字段名在发送处手写为 camelCase 字符串常量。这类 topic 没有专门的 Rust struct，因此 `check-dto-drift.ps1` 无法覆盖它们——修改字段时必须同时手动更新消费该事件的前端订阅代码，并检查 `frontend/src/lib/events/bus.ts` 或具体 hook 里的字段访问。
- **DTO payload**（`job-cancellation`、`import-phase-progress`、`import-partial-result`、`cache-index-status`、`performance-report-ready`）：直接 clone 一个已有的 `crates/transport/src/dto/*.rs` 类型作为 payload。这类 topic 的字段契约由 `scripts/check-dto-drift.ps1` 间接覆盖（只要该 DTO 与其对应 TS interface 配对成功）。

## 修改事件契约时的步骤

1. 在 `crates/transport/src/events/mod.rs` 新增/修改 `TOPIC_*` 常量与 `EventTopic` 枚举变体（保持 `#[serde(rename_all = "kebab-case")]` 或显式 `#[serde(rename = "...")]`）。
2. 在 `frontend/src/types/events.ts` 的 `EventTopic` 联合类型中添加/修改对应字符串字面量。
3. 在 `apps/desktop/src-tauri/src/events/event_bridge.rs` 添加/修改 `emit_*` 函数。
4. 若 payload 是匿名 JSON，在消费方（`frontend/src/lib/events/bus.ts` 订阅者或具体 feature hook）同步字段名；若 payload 是 DTO，同时更新 `frontend/src/types/*.ts` 对应 interface。
5. 运行 `powershell -File scripts/check-event-topic-drift.ps1` 确认 topic 集合同步；如涉及 DTO payload，同时运行 `powershell -File scripts/check-dto-drift.ps1`。
