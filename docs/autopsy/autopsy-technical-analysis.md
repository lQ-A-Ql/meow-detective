# Autopsy 技术分析报告（Remote-only GitHub Audit）

## Executive Summary

Autopsy 是 Sleuth Kit 项目的图形化数字取证平台，仓库 `sleuthkit/autopsy` 以 Java 为主，默认分支为 `develop`，本次审计固定的分支 HEAD 为 `cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21`。本报告基于 `gh` 远程查询完成，未 clone、pull、fetch 或下载源码归档。

Autopsy 的架构重点是：基于 NetBeans Platform/RCP 的模块化桌面应用；`Core/` 承载案件管理、数据源添加、ingest、服务层和主要 UI 编排；`KeywordSearch/`、`RecentActivity/`、`ImageGallery/`、`ScalpelCarver/`、`Tika/` 等目录提供独立功能模块；底层取证数据模型和 JNI/native 能力依赖 Sleuth Kit Java datamodel/JAR。

关键证据：

- 仓库：<https://github.com/sleuthkit/autopsy>
- 审计分支：`develop`
- 审计 SHA：`cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21`
- `Core/`：<https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core>
- `KeywordSearch/`：<https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/KeywordSearch>
- `BUILDING.txt`：<https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/BUILDING.txt>

## Repository Snapshot

`gh repo view sleuthkit/autopsy` 返回的主要信息：

| Item | Value |
|---|---|
| Repository | `sleuthkit/autopsy` |
| Description | Autopsy is a digital forensics platform and graphical interface to The Sleuth Kit and other tools |
| Default branch | `develop` |
| Audited HEAD | `cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21` |
| Primary language | Java |
| Stars observed | 3179 |
| Recent update observed | 2026-06-04 |
| Recent releases observed | `autopsy-4.23.1`, `autopsy-4.23.0`, `autopsy-4.22.1`, `autopsy-4.22.0`, `autopsy-4.21.0` |

Top-level tree summary from `gh api repos/sleuthkit/autopsy/git/trees/develop?recursive=1` shows a large Java/RCP application with notable directory counts: `Core` (~2950 paths), `KeywordSearch` (~735), `thirdparty` (~5103), `docs` (~1298), `ImageGallery` (~225), `Experimental` (~157), `RecentActivity` (~56), `Tools` (~46), `ScalpelCarver` (~25), `Tika` (~16), plus build and CI files.

## Audit Methodology

The audit used only remote GitHub CLI/API inspection:

```powershell
gh repo view sleuthkit/autopsy --json name,description,defaultBranchRef,primaryLanguage,stargazerCount,updatedAt,licenseInfo
gh api repos/sleuthkit/autopsy/commits/develop --jq .sha
gh api "repos/sleuthkit/autopsy/git/trees/develop?recursive=1"
gh api repos/sleuthkit/autopsy/releases --jq '.[0:5] | map({tag_name,name,published_at})'
gh api repos/sleuthkit/autopsy/contents/BUILDING.txt?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21 --jq .content
gh api repos/sleuthkit/autopsy/contents/README.txt?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21 --jq .content
```

No local source checkout was created. `gh search` attempts for some issue/PR context were affected by network/API errors, so maintenance observations are based primarily on release metadata and repository/workflow tree evidence.

## Architecture Map

Autopsy is organized as a large modular Java desktop application. The audited tree indicates these principal areas:

| Area | Technical Role | Evidence |
|---|---|---|
| `Core/` | Main application module: case management, ingest orchestration, services, UI integration | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core> |
| `Core/src/org/sleuthkit/autopsy/casemodule/` | Case lifecycle and data-source addition workflow | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule> |
| `Core/src/org/sleuthkit/autopsy/ingest/` | Ingest module lifecycle, job orchestration, ingest services | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/ingest> |
| `Core/src/org/sleuthkit/autopsy/casemodule/services/` | Application service wrappers around case/datamodel operations | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/services> |
| `Core/src/org/sleuthkit/autopsy/centralrepository/` | Cross-case correlation and central repository support | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/centralrepository> |
| `KeywordSearch/` | Keyword/search subsystem, Solr/Lucene/Tika-related search functionality | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/KeywordSearch> |
| `ImageGallery/` | Media/image review workflow | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/ImageGallery> |
| `RecentActivity/` | Recent activity extraction modules | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/RecentActivity> |
| `ScalpelCarver/` | File carving integration | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/ScalpelCarver> |
| `Tika/` | Apache Tika integration | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Tika> |
| `thirdparty/` | Bundled/vendor dependencies and binaries | <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/thirdparty> |

The structure suggests a layered desktop application:

1. UI and NetBeans module layer.
2. Case/data-source workflow layer in `casemodule`.
3. Ingest orchestration and plugins.
4. Application service wrappers around Sleuth Kit datamodel concepts.
5. Feature modules for search, recent activity, image gallery, carving, and content parsing.
6. Native/disk-image capabilities through Sleuth Kit Java datamodel/JNI dependencies rather than being implemented entirely inside Autopsy.

## Major Modules

### Core

`Core/` is the dominant module. Remote tree evidence shows `Core/src/org/sleuthkit/autopsy/casemodule/Case.java`, `AddImageTask.java`, `ImageDSProcessor.java`, `LocalFilesDSProcessor.java`, and related wizard/UI classes under `casemodule`. These paths indicate `Core` owns case lifecycle and data-source ingestion entry points.

Important cited files:

- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/Case.java>
- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/AddImageTask.java>
- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/ImageDSProcessor.java>
- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/LocalFilesDSProcessor.java>

### Ingest Pipeline

`Core/src/org/sleuthkit/autopsy/ingest/` is the architectural center for automated analysis. The design likely separates ingest job setup, module lifecycle, scheduling, and result posting. This is where report authors and maintainers should focus when evaluating extensibility, cancellation, partial failure handling, and long-running job behavior.

Evidence:

- <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/ingest>

### Case Services / Blackboard / Tags

Autopsy wraps lower-level datamodel functions in service classes under `Core/src/org/sleuthkit/autopsy/casemodule/services/`. The important architectural pattern is that UI and ingest modules should interact with service abstractions rather than raw database/JNI code wherever possible.

Evidence:

- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/services/Blackboard.java>
- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/services/FileManager.java>
- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/services/Services.java>
- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/casemodule/services/TagsManager.java>

### Central Repository

The `centralrepository` package is important for multi-case correlation. It is separate from the normal case workflow and should be treated as a higher-level correlation subsystem.

Evidence:

- <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/src/org/sleuthkit/autopsy/centralrepository>

### Keyword Search

`KeywordSearch/` is large and distinct. The README and embedded software list mention Solr, Lucene, and Tika for keyword search/content parsing. This subsystem has its own Ivy file and test tree, suggesting a separately managed module.

Evidence:

- <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/KeywordSearch>
- <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/KeywordSearch/ivy.xml>

## Data / Control Flow

The high-level flow inferred from remote tree structure is:

1. User creates/opens a case through `Core/src/org/sleuthkit/autopsy/casemodule/`.
2. User adds a data source through image/local-file processors such as `ImageDSProcessor` or `LocalFilesDSProcessor`.
3. Autopsy delegates low-level image/filesystem content handling to Sleuth Kit Java datamodel/JNI artifacts.
4. Ingest jobs run modules under the ingest subsystem.
5. Findings and extracted artifacts are posted through blackboard/service abstractions.
6. Feature modules such as keyword search, recent activity, image gallery, and carving consume indexed or extracted content.
7. Results are presented via NetBeans/RCP UI modules and can be tagged, correlated, or reported.

This is consistent with Autopsy's README description as a graphical interface to The Sleuth Kit and other open-source digital forensics tools.

## Source-Grounded Algorithm Models

The following models are derived from remote source inspection of the pinned Autopsy revision. They describe source-level control-flow intent and module boundaries; they are not local runtime traces.

### Case Lifecycle and Data-Source Entry Model

Autopsy centralizes case state through `Core/src/org/sleuthkit/autopsy/casemodule/Case.java`. The source-level lifecycle is:

```text
create/open case
  -> create or open SleuthkitCase database
  -> initialize case services/resources
  -> publish current-case events
  -> add data sources through DataSourceProcessor implementations
  -> run ingest jobs
  -> cancel ingest and close services/database on case close
```

`Case.createAsCurrentCase(...)` and `Case.openAsCurrentCase(...)` lead into current-case setup, service-resource opening, and event publication through `Case.Events.CURRENT_CASE`. Closing a case reverses the dependency order: ingest jobs are cancelled through `IngestManager.cancelAllIngestJobs(CASE_CLOSED)`, then multi-user communication, service resources, the case database, and temporary files are closed. In multi-user cases, coordination service locks and remote-event channels are part of the case boundary, so case state changes are serialized around shared resources.

Data-source addition enters through `Core/src/org/sleuthkit/autopsy/corecomponentinterfaces/DataSourceProcessor.java` and `DataSourceProcessorCallback.java`. Disk-image sources are handled by `Core/src/org/sleuthkit/autopsy/casemodule/ImageDSProcessor.java`, which creates a `SleuthkitCase.makeAddImageProcess(...)` operation and delegates long-running execution to `AddImageTask.java`. Local file sources use `LocalFilesDSProcessor.java` and `FileManager.addLocalFilesDataSource(...)`. The image task model is: start TSK add-image, poll current directory/progress in the background, validate image size when complete, store MD5/SHA1/SHA256 metadata, and notify the callback with newly added `Content` objects.

### Ingest Scheduling and Pipeline Model

The ingest subsystem under `Core/src/org/sleuthkit/autopsy/ingest/` uses a job/executor/scheduler split:

```text
IngestManager.queueIngestJob() / beginIngestJob()
  -> IngestJob
  -> IngestJobExecutor.startUp()
  -> IngestTasksScheduler
  -> tiered IngestPipeline instances
  -> IngestTask.execute(threadId)
  -> pipeline shutdown and job completion notification
```

`IngestManager.java` is the public orchestration entry point. `queueIngestJob(...)` creates an `IngestJob` and submits startup work, while `beginIngestJob(...)` starts immediately. `IngestJob.java` stores the job id, data source, files, settings, and execution mode, then creates `IngestJobExecutor.java` on `start()`. The executor groups work into data-source, file, data-artifact, and analysis-result pipelines. It advances through module tiers with `checkForTierCompleted(...)`: when all tasks in a tier finish, the current pipelines shut down, the executor advances to the next tier, and final shutdown records completion/cancellation state.

`IngestTasksScheduler.java` owns task submission APIs such as `scheduleDataSourceIngestTask`, `scheduleFileIngestTasks`, `scheduleHighPriorityFileIngestTasks`, `scheduleDataArtifactIngestTasks`, and `scheduleAnalysisResultIngestTasks`. Internally, `BlockingIngestTaskQueue` and tracking queues feed consumer workers such as `IngestManager.ExecuteIngestJobTasksTask`, which blocks for the next task and calls `task.execute(threadId)`. This design lets newly extracted or carved files enter as high-priority file tasks, while blackboard-posted artifacts can be routed back into running jobs as artifact/result tasks.

Cancellation is cooperative. `IngestJobExecutor.cancel(...)` cancels pending tasks and marks job state, while modules are expected to observe ingest context cancellation flags. The executor also tracks progress snapshots, current modules, paused ingest threads, and UI progress bars, but already completed analysis is not discarded merely because a later cancellation happens.

### Result-Type Pipeline Model

Autopsy models ingest as cascading result types rather than a single flat file loop:

- `DataSourceIngestPipeline` runs modules that analyze the entire data source and may seed file or artifact work.
- `FileIngestPipeline` runs `FileIngestModule.process(AbstractFile)` over files and derived files.
- `DataArtifactIngestPipeline` reacts to `DataArtifact` outputs.
- `AnalysisResultIngestPipeline` reacts to higher-level `AnalysisResult` outputs.

`IngestModuleFactoryLoader.java` discovers modules from NetBeans Lookup, adapters, and script/module loaders. `IngestModuleTierBuilder.java` builds tier definitions from module templates. `IngestPipeline.java` provides the common lifecycle: create module instances, call `startUp(IngestJobContext)`, perform each task through the pipeline, and call `shutDown()` at tier or job completion. This gives Autopsy a plugin model where modules can generate more work without breaking the scheduler contract.

### FileManager, Blackboard, and Artifact Model

Autopsy exposes case services through `Core/src/org/sleuthkit/autopsy/casemodule/services/Services.java`. `FileManager.java` wraps common file queries and local-file data-source addition, while blackboard access is now primarily a thin compatibility surface over `org.sleuthkit.datamodel.Blackboard`. Older Autopsy methods such as `indexArtifact(...)` delegate to datamodel methods like `postArtifact(...)`.

The durable object boundary lives in Sleuth Kit Java datamodel code, especially `bindings/java/src/org/sleuthkit/datamodel/SleuthkitCase.java`, `Blackboard.java`, and `FileManager.java` in `sleuthkit/sleuthkit`. Autopsy's practical model is therefore: UI and ingest modules use Autopsy services, services delegate to the datamodel, and the datamodel owns SQL-backed content, artifact, attribute, tag, report, and timeline operations.

### Keyword Search Indexing Model

`KeywordSearch/src/org/sleuthkit/autopsy/keywordsearch/KeywordSearchIngestModule.java` connects ingest to text indexing. On startup it opens or validates a Solr core/schema; during file processing it skips virtual directories, indexes metadata-only records for directories or empty files, and extracts text/OCR/Tika metadata for supported content. On shutdown it commits through `Ingester` and records summary statistics.

`KeywordSearch/src/org/sleuthkit/autopsy/keywordsearch/Ingester.java` contains the core indexing algorithm:

```text
Reader with extracted text
  -> Chunker
  -> optional first-chunk language detection
  -> InlineSearcher keyword matching
  -> Solr chunk documents
  -> parent metadata document with NUM_CHUNKS
  -> commit lifecycle
```

`Ingester.search(...)` reads text through `Chunker`, checks `IngestJobContext` cancellation, optionally detects language on the first chunk, runs inline keyword searches against active keyword lists, and writes chunk documents to Solr. For full indexing, every chunk is written. For targeted keyword-hit indexing, the active chunk list preserves context around hit chunks. After chunks are processed, `Ingester` writes a parent metadata document whose id is the source id and whose fields include `NUM_CHUNKS`; `commit()` finalizes pending writes. `SolrSearchService.java` manages case-level text-index lifecycle, including core selection, schema/version handling, and deletion.

### Timeline and Correlation Model

Autopsy's timeline model is query-and-cluster oriented. `Core/src/org/sleuthkit/autopsy/timeline/EventsModel.java` combines root filters, SQL filters, time ranges, and `TimelineManager` queries to retrieve timeline events. `DetailsViewModel.java` groups events by event type and description into clusters, then can combine adjacent clusters into stripes for visual density. `ListViewModel.java` combines same-time, same-content, same-description filesystem events into list entries suitable for investigator review.

Cross-case correlation lives under `Core/src/org/sleuthkit/autopsy/centralrepository/`. `CorrelationAttributeNormalizer.java` normalizes attributes such as MD5, domains, email addresses, phone numbers, USB IDs, MAC addresses, IMEI, IMSI, and ICCID before persistence/query. `CentralRepoIngestModule.java` is a file ingest module that consumes existing file hashes and records `CorrelationAttributeInstance` data. The algorithmic lesson is that correlation is normalized before storage instead of being cleaned opportunistically at UI query time.

### UI / Backend Layering Model

Autopsy's UI is NetBeans RCP-driven. A representative class is `Core/src/org/sleuthkit/autopsy/directorytree/DirectoryTreeTopComponent.java`, which implements NetBeans UI interfaces such as `TopComponent` and `ExplorerManager.Provider`, listens for case/data-source events, maps datamodel content into Node trees, and sends selections to result components such as `DataResultTopComponent`.

The UI layer does not parse images or filesystems directly. It reacts to `Case` state, service-layer data, datamodel objects, ingest events, and NetBeans Lookup/Node abstractions. This separation is the main architecture boundary to preserve when translating Autopsy concepts into a Rust/Tauri application: backend services own evidence processing and event publication; frontend components render state and issue commands.

### Ingest Worker Pools, Queues, and Tier Boundaries Model

Autopsy's ingest scheduler is not a single FIFO. `Core/src/org/sleuthkit/autopsy/ingest/IngestManager.java` creates separate executor paths for data-source, file, data-artifact, and analysis-result work. `IngestTasksScheduler.java` then manages distinct blocking queues, including top-level file tasks, batched file tasks, streamed file tasks, artifact tasks, result tasks, and high-priority file tasks. Extracted or carved files can be inserted near the front of the file ingest path through high-priority scheduling, which lets derived content be analyzed before long tail batches finish.

`IngestModuleTierBuilder.java` keeps the tier model deliberately constrained. It builds a first tier that can include file modules, stage-one data-source modules, data-artifact modules, and analysis-result modules, followed by a second tier for stage-two data-source modules. `IngestJobExecutor.java` advances tiers only when the scheduler reports the current job's tasks are complete, then shuts down current pipelines before starting the next tier. This is a staged state machine rather than a free-form dependency graph.

For Forensics Workbench, the useful pattern is typed queue separation plus an explicit tier state. The risky pattern to avoid is hiding priority and derived-content scheduling inside one generic job queue; the UI needs to know whether progress is blocked on data-source modules, file modules, artifact feedback, or shutdown.

### Ingest Error Capture and Blackboard Feedback Model

`IngestPipeline.java` acts as an error firewall around ingest modules. Module instances are created lazily, `startUp(IngestJobContext)` is called per module, `performTask(...)` runs modules serially for a task, and `shutDown()` is called during tier/job completion. Startup, process, and shutdown failures are captured as `IngestModuleError` values instead of being allowed to crash the worker pool. A process error from one module does not necessarily prevent later modules in the same pipeline from seeing the same task; startup failure is more severe and can cancel the job before analysis proceeds.

Blackboard events feed back into ingest. `IngestManager.handleArtifactsPosted(...)` classifies posted blackboard objects into `DataArtifact` and `AnalysisResult` work, resolves a target job by ingest job id when available, and falls back to data-source matching when necessary. It then calls `IngestJobExecutor.addDataArtifacts(...)` or `addAnalysisResults(...)`, which schedules new artifact/result tasks. The upstream source documents race and ambiguity cases: data-source processors may post artifacts before ingest jobs exist, multiple ingest jobs for one data source can make fallback matching ambiguous, and artifacts generated during pipeline shutdown may be ignored.

For a Rust/Tauri design, persistent artifact events and scheduling feedback events should be separate event types. Scheduling feedback should carry a `job_id`; data-source fallback can exist, but it should be logged as ambiguous rather than treated as equivalent to explicit job routing.

### Keyword Chunking, Solr Indexing, and Timeline View Models

Keyword indexing is intentionally chunk-oriented. `KeywordSearch/src/org/sleuthkit/autopsy/keywordsearch/Chunker.java` uses byte-size limits, whitespace-aware boundaries, UTF-16/UTF-8 sanitization, lower-case byte-size accounting, and an overlap window that is unread into the next chunk. `Ingester.java` assigns chunk ids, optionally performs first-chunk language detection, calls `InlineSearcher` for ingest-time hits, writes Solr chunk documents, and finally writes a parent metadata document with `NUM_CHUNKS`. In targeted hit mode, it keeps hit chunks plus adjacent context chunks instead of indexing every chunk.

`SolrSearchService.java`, `Server.java`, and related index classes treat text search as a case resource with its own lifecycle. Opening a case means reading index metadata, selecting or creating a Solr core/collection, checking Solr and schema compatibility, optionally upgrading, opening the core, and registering for datamodel events. Writes are buffered and can degrade into a skip-indexing state after repeated failures, allowing ingest to continue even when indexing is unhealthy.

Timeline has a separate raw-query versus view-aggregation model. `EventsModel.java` applies time/filter constraints and asks `TimelineManager` for event ids. `DetailsViewModel.java` caches query results and groups by event type/category and description into clusters/stripes, while `ListViewModel.java` combines same-time, same-content, same-description filesystem events for list presentation. A Tauri backend should expose raw event queries and aggregated timeline DTOs separately, rather than forcing the frontend to pull every raw event for large cases.

### Central Repository and Case Close Race Boundaries Model

Central repository correlation depends on normalization before persistence/query. `CorrelationAttributeNormalizer.java` canonicalizes MD5, domains, email addresses, phones, MACs, IMEI/IMSI/ICCID, and related values by type. `CorrelationAttributeUtil.java` filters unsupported file classes and allocated-state combinations before constructing searchable correlation attributes. `CentralRepoIngestModule.java` depends on existing file hashes and does not compute MD5 itself; it skips missing/no-data hashes and bulk-commits correlation instances at shutdown. The design implication is that correlation ingest should declare an explicit dependency on hash generation instead of silently calculating hashes inside the correlation module.

Case closing exposes a notable race boundary. `Case.close()` cancels ingest jobs before closing service resources and the case database, and `IngestManager.handleCaseClosed()` also requests cancellation. `IngestJobExecutor.cancel()` cancels pending file tasks and interrupts paused ingest threads, but running modules must observe cancellation cooperatively through context flags. Upstream comments note that case close does not fully wait for cancelled ingest jobs to finish and that cancelling a single data-source module relies on a temporary flag with race potential. For Forensics Workbench, case close should be modeled as `request_cancel_all -> drain_or_timeout -> close resources`, with explicit degraded-close reporting if workers do not quiesce.

## Extension Points

Autopsy appears extensible through NetBeans modules and ingest modules. Evidence for modularity includes many top-level feature directories, `nbproject`, Ivy dependency files, and separate module directories. Key extension surfaces:

- Ingest modules under `Core/src/org/sleuthkit/autopsy/ingest/`.
- Feature module directories such as `KeywordSearch/`, `ImageGallery/`, `RecentActivity/`.
- NetBeans Platform module metadata under project/module directories.
- Python examples under `pythonExamples/`, suggesting script/plugin integration examples.

Evidence:

- <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/pythonExamples>
- <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/nbproject>

## Build / Dependency / CI Posture

Autopsy uses Ant/Ivy and NetBeans-oriented build files rather than a modern Maven/Gradle-only layout.

Key build evidence:

- `BUILDING.txt`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/BUILDING.txt>
- `build.xml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/build.xml>
- `build-windows.xml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/build-windows.xml>
- `build-unix.xml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/build-unix.xml>
- `BootstrapIvy.xml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/BootstrapIvy.xml>
- `Core/ivy.xml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/ivy.xml>
- `KeywordSearch/ivy.xml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/KeywordSearch/ivy.xml>

`BUILDING.txt` states Autopsy has been used/tested with Java 17, depends on Node/npm for MCP server component build, and relies on Sleuth Kit setup, including building Sleuth Kit Java datamodel JAR. It also notes Windows as the out-of-box supported build path, with non-Windows support requiring extra handling.

CI evidence:

- `.github/workflows/build-windows.yml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/.github/workflows/build-windows.yml>
- `appveyor.yml`: <https://github.com/sleuthkit/autopsy/blob/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/appveyor.yml>

Dependency posture:

- Large `thirdparty/` tree indicates significant bundled/vendor material.
- README embedded software section lists dependencies including JRE 17, NetBeans platform, Sleuth Kit, libewf/zlib, Solr/Lucene/Tika, GStreamer, RegRipper, Pasco2, Jericho, Metadata Extractor, Reflections, SIGAR, 7zip bindings, ImgScalr, ControlsFX, JFXtras, Mustache.java, Joda-Time, and TwelveMonkeys.
- This means supply-chain review should pay special attention to version freshness, bundled binaries, and license compatibility, though full license/security verification was outside this remote-only run.

## Testing Posture

Remote tree evidence shows test locations including:

- `test/`: <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/test>
- `Core/test/unit/src/`: <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/test/unit/src>
- `Core/test/qa-functional/src/`: <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Core/test/qa-functional/src>
- `KeywordSearch/test/unit/src/`: <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/KeywordSearch/test/unit/src>
- `Testing/test/qa-functional/src/`: <https://github.com/sleuthkit/autopsy/tree/cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21/Testing/test/qa-functional/src>

This indicates a mixed unit/QA-functional testing posture. This audit did not run tests locally.

## Maintenance Signals

Observed release cadence is active:

| Release | Published |
|---|---|
| `autopsy-4.23.1` | 2026-05-07 |
| `autopsy-4.23.0` | 2026-04-15 |
| `autopsy-4.22.1` | 2025-04-15 |
| `autopsy-4.22.0` | 2025-03-11 |
| `autopsy-4.21.0` | 2023-08-29 |

The release pattern suggests ongoing maintenance aligned with Sleuth Kit releases. The repository update timestamp observed via `gh repo view` was 2026-06-04.

## Integration Notes

Autopsy is tightly coupled to Sleuth Kit through the Java datamodel JAR and native JNI libraries. `BUILDING.txt` explicitly describes building the Sleuth Kit Java datamodel JAR from `bindings/java` and setting `TSK_HOME`. It also explains that the datamodel JAR contains native JNI libraries and dependencies such as libewf, zlib, libintl, and other DLLs/libraries depending on platform.

Practical implication: changes in Sleuth Kit Java bindings, schema, JNI packaging, or native library dependencies can directly affect Autopsy build and runtime behavior.

## Technical Risks

1. **Large bundled third-party surface**: `thirdparty/` is very large and should be periodically reviewed for stale binaries, licensing, and CVEs.
2. **Build complexity**: Ant/Ivy + NetBeans Platform + Java 17 + Node/npm + Sleuth Kit native/JNI dependencies increases reproducibility risk.
3. **Windows-primary build path**: `BUILDING.txt` says out-of-box build currently works on Windows and non-Windows requires custom handling.
4. **Native dependency boundary**: Autopsy depends on Sleuth Kit native/JNI components; failures may surface as runtime library loading or platform packaging issues.
5. **Search stack complexity**: Solr/Lucene/Tika integration is powerful but operationally heavy; index lifecycle and parsing failures should be audited in deeper runtime testing.
6. **Central repository complexity**: Cross-case correlation introduces schema, normalization, and migration risks.

## Remote-Only Limitations

This report did not:

- Clone or build Autopsy.
- Run Ant/Ivy builds.
- Run unit or QA-functional tests.
- Inspect generated artifacts.
- Verify native library loading.
- Validate Solr/Tika runtime behavior.
- Verify bundled third-party binary integrity.
- Perform a full security or CVE audit.
- Validate the source-grounded algorithm models as runtime traces, test results, or performance measurements.

Any claims about runtime behavior, build success, or test success should be treated as unvalidated unless supported by public CI metadata.

## Recommendations

1. **Prioritize reproducible build documentation**: Modernize or supplement `BUILDING.txt` with exact Java/Ant/Ivy/Node/Sleuth Kit version matrix.
2. **Track third-party inventory**: Generate a maintained SBOM for `thirdparty/` and Ivy dependencies.
3. **Document Sleuth Kit coupling**: Keep a clear compatibility table mapping Autopsy releases to Sleuth Kit releases/JARs/native libraries.
4. **Harden ingest observability**: For future code-level audits, focus on ingest cancellation, partial failures, and artifact-posting guarantees.
5. **Audit central repository schemas**: Cross-case correlation should have explicit schema migration and normalization tests.
6. **Expand remote CI transparency**: Expose more CI build/test outcomes for Windows and non-Windows paths where possible.

## Evidence Appendix

Important remote commands used:

```powershell
gh repo view sleuthkit/autopsy --json name,description,defaultBranchRef,primaryLanguage,stargazerCount,updatedAt,licenseInfo
gh api repos/sleuthkit/autopsy/commits/develop --jq .sha
gh api "repos/sleuthkit/autopsy/git/trees/develop?recursive=1" --jq '[.tree[] | .path | split("/")[0]] | group_by(.) | map({name:.[0], count:length}) | sort_by(.name)'
gh api repos/sleuthkit/autopsy/releases --jq '.[0:5] | map({tag_name,name,published_at})'
gh api repos/sleuthkit/autopsy/contents/BUILDING.txt?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21 --jq .content
gh api repos/sleuthkit/autopsy/contents/README.txt?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21 --jq .content
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/casemodule/Case.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/casemodule/AddImageTask.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/casemodule/ImageDSProcessor.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/ingest/IngestManager.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/ingest/IngestJob.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/ingest/IngestJobExecutor.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/ingest/IngestTasksScheduler.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/ingest/IngestPipeline.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/KeywordSearch/src/org/sleuthkit/autopsy/keywordsearch/KeywordSearchIngestModule.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/KeywordSearch/src/org/sleuthkit/autopsy/keywordsearch/Ingester.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/timeline/EventsModel.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
gh api repos/sleuthkit/autopsy/contents/Core/src/org/sleuthkit/autopsy/centralrepository/CorrelationAttributeNormalizer.java?ref=cb3dacdcad67abe7cf863c74f10dcdb8e25a5c21
```
