# Technical Spec: Rust + React 磁盘介质取证工具

## 1. 目标

本文定义一个桌面优先、单用户、Windows 优先的磁盘介质取证工具的技术规格。系统借鉴 Autopsy 的产品能力域，但不沿用其历史 Java 桌面耦合结构。目标是构建一套可扩展的 Rust 核心和 React 前端，使案件管理、数据源导入、索引、工件提取、时间线和报告导出之间通过清晰边界协作。

## 2. 设计原则

1. **核心能力后端化**：案件、数据源、检索、时间线、痕迹解析、报告都由 Rust 后端主导
2. **UI 纯消费型**：React 前端主要消费 API、快照和事件流，不承载核心业务状态机
3. **事件驱动**：长任务和增量分析结果通过事件总线传播
4. **案件隔离**：所有持久化数据、索引和中间结果按案件隔离
5. **插件边界清晰**：解析器、索引器、导出器走统一契约
6. **只读证据访问**：证据读取接口默认只读
7. **先单机后扩展**：保留未来多用户演进空间，但第一版不实现协作语义

## 3. 总体架构

建议采用以下部署形态：
- Rust 后端作为本地应用核心
- React 前端作为本地桌面 UI
- 最终打包为桌面应用

实现路线建议二选一：
- **Tauri 路线**：Rust backend + React frontend，共进程桌面应用
- **本地服务路线**：Rust 本地 API 服务 + React WebView/桌面壳

第一版建议优先采用 **Tauri 路线**，理由：
- 更适合 Rust 作为主核心
- 本地文件访问与桌面打包路径明确
- 可保留 command/event 模式，便于前后端解耦

## 4. 核心模块

### 4.1 Case Service
负责案件生命周期和案件级资源管理。

职责：
- 创建、打开、关闭案件
- 管理案件元数据
- 管理案件目录结构
- 维护当前案件上下文
- 初始化案件数据库与索引目录
- 提供案件级事件

建议拆分：
- `CaseAggregate`
- `CaseService`
- `CaseRepository`
- `CaseWorkspacePolicy`
- `CaseSession`

不要像 Autopsy 的 `Case.java` 那样将 UI 生命周期、目录策略、事件桥接、全局当前案件状态全部揉进一个对象。

### 4.2 Data Source Service
负责导入和挂载证据源。

职责：
- 注册数据源
- 识别镜像类型
- 打开分区/卷/文件系统
- 提供只读文件枚举与内容读取
- 记录导入错误与告警

数据源类型：
- `RawImage`
- `E01Image`
- `LogicalDirectory`

建议接口：
- `probe(source_path) -> ProbeResult`
- `attach(case_id, source_spec) -> DataSourceId`
- `list_volumes(data_source_id)`
- `open_fs(data_source_id, volume_id)`
- `read_file(file_id, range)`

### 4.3 Evidence Catalog Service
负责案件中的标准化对象目录。

职责：
- 存储文件、目录、卷、分区、工件、标签、备注等实体
- 为时间线、检索和报告提供统一读取模型
- 将原始文件系统对象映射为稳定内部 ID

建议实体：
- `CaseRecord`
- `DataSourceRecord`
- `VolumeRecord`
- `FileEntryRecord`
- `ArtifactRecord`
- `TagRecord`
- `NoteRecord`
- `TimelineEventRecord`
- `ReportRecord`

### 4.4 Ingest Orchestrator
借鉴 Autopsy 的 ingest job/pipeline 思路，但用更明确的队列与任务状态机重构。

职责：
- 创建分析任务
- 管理模块依赖顺序
- 调度解析器/索引器/导出器
- 汇总进度
- 管理取消/失败/重试
- 发布任务事件

建议核心组件：
- `JobManager`
- `JobRegistry`
- `TaskScheduler`
- `WorkerPool`
- `ProgressStore`
- `JobEventPublisher`

建议队列分层：
- `data_source_tasks`
- `file_walk_tasks`
- `text_extract_tasks`
- `artifact_extract_tasks`
- `index_tasks`
- `timeline_projection_tasks`
- `report_tasks`

并发策略建议：
- 数据源级任务串行启动
- 文件级任务并行执行
- 事件发布单线程保序
- 各任务共享取消 token

### 4.5 Search Service
借鉴 Autopsy 的 keyword search subsystem，但将搜索引擎与 UI 完全分离。

职责：
- 管理文本提取后的索引
- 执行字面量/短语/正则查询
- 提供命中片段与上下文
- 维护索引进度与状态

建议子模块：
- `TextExtractionService`
- `IndexWriterService`
- `SearchQueryService`
- `HighlightService`
- `SearchProjectionService`

查询模型：
- `LiteralQuery`
- `PhraseQuery`
- `RegexQuery`

建议数据结构：
```rust
struct SearchRequest {
    case_id: CaseId,
    query: QueryExpr,
    filters: SearchFilters,
    paging: Paging,
}
```

索引后端：
- 如果坚持纯 Rust，优先评估 `tantivy`
- 文本提取可先通过受控适配层调用外部提取器，再逐步 Rust 化

### 4.6 Timeline Service
借鉴 Autopsy 的案件级 timeline 模块。

职责：
- 接收文件时间和工件时间事件
- 维护案件级时间线投影
- 支持时间范围、类型、来源过滤
- 为前端提供缩放与聚合所需查询接口

输入源：
- 文件系统 MACB 时间
- 工件解析模块输出的时间事件
- 用户标签/备注时间点（可选）

建议拆分：
- `TimelineIngestor`
- `TimelineRepository`
- `TimelineQueryService`
- `TimelineProjectionBuilder`

建议事件模型：
```rust
struct TimelineEvent {
    id: TimelineEventId,
    case_id: CaseId,
    source_object_id: ObjectId,
    event_type: TimelineEventType,
    ts: DateTime<Utc>,
    description: String,
    attrs: BTreeMap<String, Value>,
}
```

### 4.7 Artifact Extraction Service
这是第一版的重点能力。

职责：
- 运行工件解析器
- 把解析结果标准化写入 catalog
- 产出时间线事件和可报告对象

建议按工件家族拆模块：
- `registry_parser`
- `prefetch_parser`
- `lnk_parser`
- `jump_list_parser`
- `recycle_bin_parser`
- `sru_parser`
- `thumbcache_parser`
- `shellbags_parser`（后续）
- `zone_identifier_parser`（后续）
- `chromium_history_parser`（后续）

借鉴 Autopsy `RAImageIngestModule` 的经验：
- 解析器按工件家族拆分
- 允许模块声明依赖
- 先做基础解析，再做高层关联分析

建议解析器契约：
```rust
trait ArtifactExtractor {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn dependencies(&self) -> &'static [&'static str];
    fn supports(&self, ctx: &ArtifactContext) -> bool;
    fn run(&self, ctx: &ArtifactContext, sink: &mut dyn ArtifactSink) -> Result<ExtractorReport>;
}
```

### 4.8 Report Service
借鉴 Autopsy 的报告模块接口思想。

职责：
- 管理报告模板和导出器
- 读取案件对象与筛选结果
- 生成 HTML/JSON/CSV 报告
- 记录导出历史

建议导出器类型：
- `HtmlReportExporter`
- `JsonCaseExporter`
- `CsvArtifactExporter`
- `EvidenceBundleExporter`

建议导出器契约：
```rust
trait ReportExporter {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn default_settings(&self) -> Value;
    fn validate_settings(&self, settings: &Value) -> Result<()>;
    fn export(&self, ctx: &ReportContext) -> Result<ReportOutput>;
}
```

## 5. 前端架构

前端建议使用 React + TypeScript。

### 5.1 状态分层
- 服务器状态：案件、任务、搜索结果、时间线结果、工件结果
- 瞬时 UI 状态：当前选中项、面板折叠、分页、筛选器
- 事件流状态：任务进度、模块告警、实时完成通知

建议：
- 数据查询：TanStack Query
- 全局 UI 状态：Zustand 或 Redux Toolkit
- 表格：TanStack Table
- 时间线可视化：自定义 Canvas/SVG 或高性能图形库

### 5.2 页面/视图结构
- `CaseHomeView`
- `DataSourceImportView`
- `FileBrowserView`
- `FileDetailView`
- `SearchView`
- `TimelineView`
- `ArtifactsView`
- `TagsNotesView`
- `ReportsView`
- `TasksDrawer`

### 5.3 前后端交互
建议双通道：
- 命令/查询 API
- 后端事件流

示例：
- `create_case`
- `open_case`
- `import_data_source`
- `start_job`
- `cancel_job`
- `search`
- `get_timeline`
- `list_artifacts`
- `export_report`

事件示例：
- `case.opened`
- `case.closed`
- `job.started`
- `job.progress`
- `job.completed`
- `job.failed`
- `artifact.added`
- `timeline.updated`
- `index.progress`

## 6. 持久化设计

### 6.1 案件目录结构建议
```text
case-root/
  case.json
  app.db
  evidence/
  exports/
  reports/
  indexes/
  cache/
  logs/
  jobs/
```

### 6.2 存储分层
- 案件元数据：`case.json`
- 关系型对象目录：SQLite
- 全文索引：单独 index 目录
- 导出产物：reports/exports
- 中间缓存：cache

### 6.3 SQLite 建议表
- `cases`
- `data_sources`
- `volumes`
- `file_entries`
- `file_hashes`
- `artifacts`
- `artifact_attributes`
- `timeline_events`
- `saved_searches`
- `tags`
- `tag_bindings`
- `notes`
- `jobs`
- `job_tasks`
- `reports`
- `schema_migrations`

## 7. 数据模型原则

### 7.1 统一对象引用
所有结果都应能回链到统一对象标识：
- `ObjectId` 代表文件、目录、工件、事件等统一对象引用
- 高层结果通过 `source_object_id` 指向来源对象

### 7.2 标准化工件模型
避免每种解析器直接暴露完全不同结构。

建议：
```rust
struct ArtifactRecord {
    id: ArtifactId,
    case_id: CaseId,
    data_source_id: DataSourceId,
    artifact_type: String,
    source_object_id: Option<ObjectId>,
    title: String,
    summary: String,
    attrs: serde_json::Value,
    created_at: DateTime<Utc>,
}
```

## 8. 插件与扩展机制

第一版不必开放第三方市场，但内部必须按插件边界设计。

### 8.1 可插拔对象
- 数据源探测器
- 文件系统适配器
- 文本提取器
- 工件解析器
- 报告导出器

### 8.2 注册机制
建议第一版先采用静态注册：
- 编译期注册内建模块
- 运行时统一发现和装配

后续可演进到：
- WASI 插件
- 动态库插件
- 外部命令适配器

## 9. 任务系统

### 9.1 任务状态机
```text
Queued -> Running -> Completed
                 -> Failed
                 -> Cancelled
```

### 9.2 任务元数据
- 任务 ID
- 所属案件 ID
- 任务类型
- 当前阶段
- 总体进度
- 当前模块
- 当前对象
- 开始时间
- 结束时间
- 错误摘要

### 9.3 任务分类
- 导入任务
- 文件枚举任务
- 哈希计算任务
- 文本提取任务
- 索引任务
- 工件解析任务
- 时间线投影任务
- 报告导出任务

## 10. 错误处理与可观测性

### 10.1 错误模型
所有模块返回结构化错误：
- `code`
- `message`
- `details`
- `object_ref`
- `recoverable`

### 10.2 日志分层
- 应用日志
- 案件日志
- 任务日志
- 解析器日志
- 文本提取/索引日志

借鉴 Autopsy 对 Tika 日志单独拆分的做法，索引/提取相关日志应独立于应用通用日志。

### 10.3 指标
第一版至少内部提供：
- 当前运行任务数
- 每任务阶段耗时
- 索引文件数
- 已解析工件数
- 各模块错误数

## 11. 安全与取证约束

- 默认只读访问证据
- 明确区分原始证据与派生产物路径
- 不覆盖原始文件
- 对导出产物记录来源对象与导出时间
- 对案件数据库和索引版本化
- 对时区与时间解析保留原始值和规范化值

## 12. 性能建议

### 12.1 文件浏览
- 目录懒加载
- 大目录分页
- 元数据批量读取

### 12.2 检索
- 文本提取和索引分离
- 增量索引
- 结果分页和摘要预取

### 12.3 时间线
- 预聚合
- 分桶查询
- 缩放级别缓存

### 12.4 工件解析
- 基础工件优先并行
- 依赖链任务后置
- 大文件与小文件差异调度

## 13. 推荐技术选型

### 后端
- Rust stable
- Tokio
- Serde
- SQLx 或 Diesel + SQLite
- Tantivy（若坚持纯 Rust 搜索）
- Tauri
- Tracing
- thiserror / anyhow

### 前端
- React
- TypeScript
- Vite
- TanStack Query
- Zustand 或 Redux Toolkit
- TanStack Table
- ECharts/VisX/自定义时间线渲染

## 14. 分阶段实施建议

### Phase 1: 案件与数据源基础
- 案件创建/打开/关闭
- RAW/E01/逻辑目录导入
- 文件树与基础元数据模型

### Phase 2: 文件浏览与导出
- 文件列表
- 十六进制/文本查看
- 文件导出
- 哈希能力

### Phase 3: 检索系统
- 文本提取
- 索引建立
- 字面量/短语/正则搜索
- 命中详情

### Phase 4: Windows 痕迹第一批
- Registry
- Prefetch
- LNK
- Jump List
- Recycle Bin
- SRU

### Phase 5: 时间线与报告
- 时间线查询和 UI
- HTML/JSON/CSV 报告
- 标签和备注入报告

## 15. 与 Autopsy 的明确映射关系

### 借鉴点
- `Case.java` → 案件是核心领域对象，但拆分为 service/repository/session
- `IngestManager.java` → 保留中心化任务编排，但用显式 job/scheduler/event store 重写
- `KeywordSearch.java` → 保留独立搜索子系统与 typed query 模型
- `TimeLineModule.java` → 保留案件级时间线模块与事件驱动更新
- `RAImageIngestModule.java` → 保留按工件家族拆分的解析器编排方式
- `ReportModule.java` → 保留可插拔导出器接口

### 刻意不继承的点
- 全局单例到处扩散
- UI 组件和核心生命周期强绑定
- 一个核心类同时负责状态、事件、目录策略、错误展示和外部协作
- 历史桌面框架特定耦合

## 16. 当前未决技术问题

- E01 支持是否完全纯 Rust 实现，还是允许有限封装外部库
- 文件系统解析优先级是否只做 NTFS/FAT/exFAT，还是首版包含更多类型
- 文本提取是否允许桥接现有成熟提取器作为过渡方案
- 报告 PDF 是否首版原生支持，还是通过 HTML 转换
- 桌面壳是否直接采用 Tauri，还是先本地 API 服务 + Web 前端迭代