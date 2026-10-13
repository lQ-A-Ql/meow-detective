# IPC 事件契约

本文档列出后端到前端的全部 20 个 Tauri 事件 topic，及其 payload 类型来源。事件契约没有代码生成，Rust 端 `crates/transport/src/events/mod.rs` 与前端 `frontend/src/types/events.ts` 的 `EventTopic` 联合类型必须手工保持一致——`scripts/check-event-topic-drift.ps1` 在 CI 中校验两者一一对应。

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
| `analysis-extraction-progress` | `AnalysisExtractionProgress` | `emit_analysis_extraction_progress` | `AnalysisExtractionProgressDto`（`crates/transport/src/dto/analysis.rs`） |
| `file-extract-progress` | `FileExtractProgress` | `emit_file_extraction_progress` | `FileExtractionProgressDto`（`crates/transport/src/dto/files.rs`） |

`file-extract-progress.phase` 依次使用 `preparing`、`copying`、`finalizing` 和终态。`finalizing` 表示证据字节复制完成，但仍在执行目标文件 flush、持久化同步和原子发布；终态为 `completed`、`completedWithWarning` 或 `failed`。进度事件按字节与时间双重节流，终态和阶段切换不丢弃。

## 两类 payload

- **匿名 JSON payload**（大多数 topic）：直接用 `serde_json::json!({...})` 构造，字段名在发送处手写为 camelCase 字符串常量。这类 topic 没有专门的 Rust struct，因此 `check-dto-drift.ps1` 无法覆盖它们——修改字段时必须同时手动更新消费该事件的前端订阅代码，并检查 `frontend/src/lib/events/bus.ts` 或具体 hook 里的字段访问。
- **DTO payload**（`job-cancellation`、`import-phase-progress`、`import-partial-result`、`cache-index-status`、`performance-report-ready`、`analysis-extraction-progress`、`file-extract-progress`）：直接 clone 一个已有的 `crates/transport/src/dto/*.rs` 类型作为 payload。这类 topic 的字段契约由 `scripts/check-dto-drift.ps1` 间接覆盖（只要该 DTO 与其对应 TS interface 配对成功）。

## 修改事件契约时的步骤

1. 在 `crates/transport/src/events/mod.rs` 新增/修改 `TOPIC_*` 常量与 `EventTopic` 枚举变体（保持 `#[serde(rename_all = "kebab-case")]` 或显式 `#[serde(rename = "...")]`）。
2. 在 `frontend/src/types/events.ts` 的 `EventTopic` 联合类型中添加/修改对应字符串字面量。
3. 在 `apps/desktop/src-tauri/src/events/event_bridge.rs` 添加/修改 `emit_*` 函数。
4. 若 payload 是匿名 JSON，在消费方（`frontend/src/lib/events/bus.ts` 订阅者或具体 feature hook）同步字段名；若 payload 是 DTO，同时更新 `frontend/src/types/*.ts` 对应 interface。
5. 运行 `powershell -File scripts/check-event-topic-drift.ps1` 确认 topic 集合同步；如涉及 DTO payload，同时运行 `powershell -File scripts/check-dto-drift.ps1`。

## 注册表浏览会话契约

注册表页的“原始键值”Tab 使用案件和数据源绑定的只读 Hive 会话。旧的
`browse_registry_key` 入口和旧 DTO 保持兼容；文件预览仍可调用旧接口。
新浏览器读取基础 Hive，事务日志和已删除 cell 不参与此视图。

权威 DTO 位于 `crates/transport/src/dto/registry_browser/`；手工镜像位于
`frontend/src/types/registryBrowser.ts`。`RegistryBrowserClient` 位于
`frontend/src/lib/api/registry-browser.ts`，通过 `apiClient` 调用以下已注册命令。
所有命令参数使用 `{ request: <DTO> }` 封装，不新增事件。

| 命令 | 请求 / 响应 DTO（省略 Dto 后缀） |
| --- | --- |
| `list_registry_hives` | RegistryHiveListRequest / RegistryHivePage |
| `open_registry_hive` | RegistryHiveOpenRequest / RegistryHiveSession |
| `resolve_registry_key` | RegistryKeyResolveRequest / RegistryKeyMetadata |
| `list_registry_subkeys` | RegistrySubkeysRequest / RegistrySubkeyPage |
| `list_registry_values` | RegistryValuesRequest / RegistryValuePage |
| `read_registry_value_range` | RegistryValueRangeRequest / RegistryValueRange |
| `close_registry_hive` | RegistryHiveCloseRequest / null（客户端归一化为 true） |

- 新会话只声明 `view=base`；`merged/recovered` 不属于此浏览器能力。
- 来源绑定 `caseId/dataSourceId/fileId/sourceRevision/view`；所有 keyId、valueId、
  cursor 和 sessionId 都是来源/会话范围内的身份，不能跨会话复用。
- 根键相对路径为 `""`；值名 `""` 表示确实存在的默认值。无默认值时不制造值行。
- `typeCode/typeName` 保留 VK 原生类型；`lastWriteFiletime` 与解码后的整数文本
  以十进制字符串保留精度。`cellOffset` 是 HBIN 相对偏移，不是整盘偏移。
- Coverage 的 `declaredCount` 表示集合声明数量，`returnedCount` 是本页返回数量，
  `skippedCount` 表示本页解析诊断中的缺失项，`status` 表示解析覆盖；分页是否继续
  由 `nextCursor` 决定。只在无后续页、无缺失、加载数等于声明数时呈现完整集合。
- 值列表不携带原始 payload；大值保留元数据行，选中后按窗口读取。range 的 offset
  是组装后值 payload 内的偏移，不能伪称为连续物理区间。`decodedText` 可以缺省，
  `rawOnly/partial` 不等于成功解码。
- 页大小 1–200（前端默认 100）、窗口 1–65,536 字节。单 Hive 上限 128 MiB，
  浏览器保留预算 256 MiB，最多 8 个会话；预算包含 cell bitmap 和每会话预留的
  16 MiB 元数据。分配前预留，打开和读取持锁，淘汰空闲快照无外部租借。
- 会话空闲 30 分钟过期；案件/数据源生命周期和 BitLocker generation 改变时失效。
  HivePage.sourceRevision 与打开会话的 revision 一致；游标绑定来源、revision、键和
  列表类型。无会话时关闭幂等成功。键和值 ID 为已签发的不透明 token，不能传入
  任意 cell 偏移读取。失效后的 UI 清除值数据并提供重新发现、打开操作。
- 大值按 REGF 1.5+ 的 `db` 分段组装；每段最多 16,344 字节，窗口最多 64 KiB。
  所有分段边界验证后才返回 payload，参考 [REGF 格式文档](https://github.com/libyal/libregf/blob/main/documentation/Windows%20NT%20Registry%20File%20%28REGF%29%20format.asciidoc)。
- 键树和值表按需加载，键树 DOM 虚拟化；路径定位会翻过中间子键页。筛选明确限定
  已加载节点，完整 Hive 检索及事务恢复浏览尚未提供。
- `diagnostics` 使用稳定代码而非宿主路径/堆栈文本；新增可选字段省略 None。

共享合成 wire fixture：`frontend/mocks/registry-browser/contract.json`。
`cargo test -p transport --test registry_browser_contract` 与前端
`tests/features/registry-browser/mock-contract.test.ts` 锁定字段、来源、精度和空值语义。

服务回归：`cargo test -p app-services --test registry_browser_service`。真实样本回归
`registry_browser_e01` 默认忽略；显式设置 `FORENSICS_E01_FIXTURE` 后使用 `--ignored`。
可通过 `FORENSICS_REGISTRY_BROWSER_REPORT` 和 `FORENSICS_REGISTRY_BROWSER_ORACLE_DIR`
指定报告及独立比对副本输出，文件均以 create_new 写入，原始镜像始终只读。
