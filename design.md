# Design: Rust + React 磁盘介质取证工具

## 1. 文档目标

本文档在 `PRD.md` 与 `spec.md` 的基础上，给出可直接指导项目初始化与分阶段开发的详细设计，重点覆盖：
- 项目目录骨架
- Rust workspace 与 crate 划分
- 后端核心算法设计
- 应用内链路数据传输形式
- 前端目录与状态骨架
- 关键数据结构与典型调用链
- MVP 实施顺序

本文档默认以下边界：
- 单用户
- 桌面优先
- Windows 宿主优先
- Rust 为核心后端
- React 为前端
- 第一版包含镜像导入、文件浏览、关键词检索、时间线、Windows 痕迹、报告导出

## 2. 总体设计结论

### 2.1 总体架构选型
第一版采用：
- **Tauri + Rust Core + React/TypeScript UI**

原因：
- 与 Rust 核心能力最契合
- 本地文件访问、桌面打包、事件回推路径清晰
- 可使用 Tauri command 作为请求通道，event 作为异步通知通道
- 不强制引入独立 HTTP 服务，减少本地部署复杂度

### 2.2 运行模型
进程内主要分成三层：

1. **UI Layer**
   - React 页面、状态管理、组件树
2. **Application Layer**
   - Tauri command handlers
   - 查询服务
   - 任务编排入口
3. **Core Layer**
   - 案件、数据源、文件系统、搜索、时间线、工件解析、报告、持久化

### 2.3 关键设计取舍
- **不做前后端共享数据库直连**：前端永远不直接接触 SQLite/index 文件
- **不做 UI 直接驱动核心状态**：前端只发请求和订阅事件
- **不做一个超级 manager**：避免出现 Autopsy 式全能单例
- **不把“搜索”“工件解析”“时间线更新”混成一个流水线类**：按能力域拆分

## 3. 项目目录骨架

建议仓库目录如下：

```text
forensics/
  .gitignore
  AGENTS.md
  PRD.md
  spec.md
  design.md
  ci.md
  test-plan.md
  frontend-ui-ux.md
  autopsy-borrowings.md
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
  apps/
    desktop/
      src-tauri/
        Cargo.toml
        tauri.conf.json
        build.rs
        src/
          main.rs
          lib.rs
          commands/
            mod.rs
            case_commands.rs
            file_commands.rs
            job_commands.rs
            search_commands.rs
            timeline_commands.rs
            artifact_commands.rs
            report_commands.rs
          events/
            mod.rs
            event_bridge.rs
          state/
            mod.rs
            app_state.rs
  frontend/
    package.json
    tsconfig.json
    vite.config.ts
    index.html
    src/
      main.tsx
      app/
        App.tsx
        routes.tsx
        providers.tsx
        pages/
          CaseHome.tsx
          FileBrowser.tsx
          Search.tsx
          Timeline.tsx
          Artifacts.tsx
          Reports.tsx
        components/
          BottomDrawer.tsx (→ moved to src/components/layout/)
          figma/
          ui/           (shadcn/radix primitives)
      features/
        case/hooks.ts
        files/hooks.ts
        jobs/hooks.ts
        search/hooks.ts
        timeline/hooks.ts
        artifacts/hooks.ts
        reports/hooks.ts
      components/
        layout/
          AppShell.tsx
          Layout.tsx
          TopBar.tsx
          BottomDrawer.tsx
          InspectorPane.tsx
          PageSubbar.tsx
        tables/
          DenseDataTable.tsx
        viewers/
          ViewerTabs.tsx
        status/
          InlineProgressRow.tsx
      lib/
        api/
          client.ts
          provider.ts
          mock-data.ts
          case.ts / files.ts / search.ts / timeline.ts / artifacts.ts / jobs.ts / reports.ts
        events/
          bus.ts
          subscribers.ts
      stores/
        ui-store.ts
        selection-store.ts
      types/
        models.ts
      styles/
  crates/
    domain/
      src/
        lib.rs
        case/mod.rs          (CaseId, CaseMeta, CaseSession)
        datasource/mod.rs    (DataSourceId, DataSource, DataSourceKind)
        file_entry/mod.rs    (FileEntryId, FileEntry, EntryType)
        artifact/mod.rs      (ArtifactId, Artifact, ArtifactFamily)
        timeline/mod.rs      (TimelineEventId, TimelineEvent)
        job/mod.rs           (JobId, Job, JobStatus, JobScope)
        report/mod.rs        (ReportId, ReportTemplate, ReportHistoryItem, ReportStatus)
        tag/mod.rs           (TagId, Tag)
    app-services/
      src/                   (depends on domain + transport)
        ...
    transport/
      src/
        dto/
          mod.rs             (re-exports from per-domain files)
          case.rs / files.rs / search.rs / timeline.rs
          artifacts.rs / jobs.rs / viewer.rs / reports.rs
        commands/mod.rs
        events/mod.rs
        paging.rs
        errors.rs
    ...
  docs/
    prototype/               (archived static prototype)
  crates/
    domain/
      src/
        lib.rs
        case/
        datasource/
        file_entry/
        artifact/
        timeline/
        report/
        tag/
        job/
    app-services/
      src/
        lib.rs
        case_service.rs
        datasource_service.rs
        file_service.rs
        search_service.rs
        timeline_service.rs
        artifact_service.rs
        report_service.rs
        job_service.rs
    infrastructure/
      src/
        lib.rs
        fs/
        hashing/
        text/
        clock/
        logging/
        config/
    persistence-sqlite/
      src/
        lib.rs
        connection.rs
        migrations/
        repositories/
          case_repo.rs
          datasource_repo.rs
          file_repo.rs
          artifact_repo.rs
          timeline_repo.rs
          report_repo.rs
          job_repo.rs
    evidence-core/
      src/
        lib.rs
        probe/
        image/
        volume/
        filesystem/
        reader/
    fs-ntfs/
      src/
        lib.rs
    fs-fat/
      src/
        lib.rs
    fs-exfat/
      src/
        lib.rs
    image-raw/
      src/
        lib.rs
    image-e01/
      src/
        lib.rs
    catalog/
      src/
        lib.rs
        projection/
        indexing/
    ingest/
      src/
        lib.rs
        job_manager.rs
        scheduler.rs
        worker_pool.rs
        progress.rs
        event_bus.rs
        task_types.rs
    search/
      src/
        lib.rs
        query/
        extractor/
        indexer/
        highlighter/
        repository/
    timeline/
      src/
        lib.rs
        ingestor.rs
        projector.rs
        query.rs
    artifacts-core/
      src/
        lib.rs
        traits.rs
        models.rs
        sink.rs
        registry.rs
    artifacts-windows/
      src/
        lib.rs
        registry/
        prefetch/
        lnk/
        jumplist/
        recycle_bin/
        sru/
        thumbcache/
    reports/
      src/
        lib.rs
        html/
        json/
        csv/
        evidence_bundle/
    transport/
      src/
        lib.rs
        dto/
        commands/
        events/
        paging.rs
        errors.rs
    testing/
      src/
        lib.rs
        fixtures/
        builders/
        fake_clock.rs
  docs/
    decisions/
    api/
    fixtures/
  development-reports/
    README.md
    sessions/
      2026-05-16/
        session-001-agent-main.md
        session-001-events.jsonl
    summaries/
      weekly/
      milestone/
  scripts/
    dev/
    release/
  testdata/
    images/
    artifacts/
    reports/
```

## 4. Rust workspace 设计

### 4.1 workspace 划分原则
- `domain`：只放核心领域模型与不依赖具体实现的规则
- `app-services`：应用服务层，编排多个领域/基础设施能力
- `infrastructure`：与 OS、日志、时间、文本提取、哈希等外部能力耦合
- `persistence-sqlite`：SQLite 仓储实现
- `evidence-*` / `fs-*` / `image-*`：证据源和文件系统支持
- `ingest`：任务编排与异步执行
- `search`：搜索、索引、提取
- `timeline`：时间线投影与查询
- `artifacts-*`：工件提取器
- `reports`：报告导出
- `transport`：前后端传输 DTO 与事件模型

### 4.2 依赖方向
强制单向依赖：

```text
desktop app -> transport/app-services
app-services -> domain + ingest + persistence + search + timeline + artifacts + reports + evidence
search/timeline/artifacts/reports/evidence -> domain + infrastructure
persistence -> domain
transport -> domain(只允许共享少量 ID/枚举) 或独立 DTO
```

当前实施状态：`app-services` 已添加 `domain` 依赖，`domain` crate 已定义核心领域模型（Case, FileEntry, Artifact, TimelineEvent, Job, Report, Tag, DataSource），`transport` DTO 已按 domain 拆分为独立文件。

禁止：
- `domain` 依赖 `tauri`
- `domain` 依赖 `sqlx`
- `artifacts-windows` 依赖前端 DTO
- `React` 直接依赖数据库结构

### 4.3 新增基础设施 crate 建议
为满足可追溯开发报告与临时缓存数据库需求，建议在第一批或第二批补充：
- `traceability/`
  - 负责开发报告记录、事件日志追加、agent 署名规范、会话摘要输出
- `runtime-cache/`
  - 负责临时 SQLite 数据库、缓存表、TTL/清理策略、临时句柄与预览缓存管理

建议目录：

```text
crates/
  traceability/
    src/
      lib.rs
      reporter.rs
      event_log.rs
      models.rs
      policy.rs
  runtime-cache/
    src/
      lib.rs
      connection.rs
      repositories/
      cleanup.rs
      models.rs
```

## 5. 后端核心模块设计

## 5.1 Case 模块

### 职责
- 创建/打开/关闭案件
- 构建案件目录结构
- 初始化 SQLite、index、cache、logs
- 管理当前打开案件 session

### 核心对象
```rust
struct CaseSession {
    case_id: CaseId,
    case_root: PathBuf,
    opened_at: DateTime<Utc>,
}

struct CaseMeta {
    id: CaseId,
    name: String,
    number: Option<String>,
    examiner: Option<String>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

### 算法/流程
#### create_case
1. 校验路径与名称
2. 创建案件目录骨架
3. 写入 `case.json`
4. 初始化 SQLite schema
5. 初始化索引目录
6. 返回 `CaseSummary`

#### open_case
1. 读取 `case.json`
2. 校验版本
3. 打开 SQLite
4. 打开索引存储
5. 构造 `CaseSession`
6. 广播 `case.opened`

## 5.2 数据源导入模块

### 支持对象
- RAW/dd
- E01
- 逻辑目录

### 关键抽象
```rust
trait DataSourceProbe {
    fn probe(&self, input: &SourceInput) -> Result<ProbeResult>;
}

trait DataSourceOpener {
    fn open(&self, spec: &DataSourceSpec) -> Result<Box<dyn OpenedDataSource>>;
}

trait OpenedDataSource {
    fn list_volumes(&self) -> Result<Vec<VolumeInfo>>;
    fn open_filesystem(&self, volume_id: &VolumeId) -> Result<Box<dyn FileSystemReader>>;
}
```

### 导入算法
#### probe_data_source
1. 根据扩展名、魔数、文件头判断类型
2. 返回 `candidate_types`
3. 对镜像提取基础几何信息
4. 输出 `ProbeResult`

#### attach_data_source
1. 将数据源元信息写入 DB
2. 通过 opener 打开数据源
3. 枚举卷/分区并写入 DB
4. 创建 `ImportDataSourceJob`
5. 将文件系统枚举工作加入 ingest 队列

## 5.3 文件系统枚举与目录树构建

### 目标
- 建立稳定的内部文件对象目录
- 支撑目录树、文件列表、时间线、检索和工件解析

### 关键抽象
```rust
trait FileSystemReader {
    fn root(&self) -> Result<FsNode>;
    fn list_children(&self, inode: FsNodeId) -> Result<Vec<FsNode>>;
    fn open_file(&self, inode: FsNodeId) -> Result<Box<dyn Read + Seek>>;
    fn stat(&self, inode: FsNodeId) -> Result<FsMetadata>;
}
```

### 枚举算法
采用 **分层 BFS + 懒详情填充**：
- 目录树初始构建以 BFS 枚举路径结构和核心元数据
- 大文件内容和扩展元数据延后加载
- UI 请求详情时再补充更多字段

#### BFS 枚举流程
1. 从 root 入队
2. 批量读取子节点
3. 写入 `file_entries`
4. 目录继续入队
5. 文件按策略加入下游任务队列：哈希、文本提取、工件解析

#### 设计原因
- 比 DFS 更适合早期 UI 展示目录结构
- 能更快让用户看到顶层结果
- 便于分批提交数据库事务

## 5.4 哈希计算算法

### 支持
- SHA-256 必选
- MD5/SHA-1 可选

### 触发策略
- 默认延后，不阻塞导入
- 用户可针对数据源或选中文件触发全量/按需哈希

### 算法
- 流式读取
- 固定块大小，例如 1MB
- 通过 job worker 并发计算
- 每 N 个文件提交一次 batch update

## 5.5 文本提取与索引算法

### 目标
- 为关键词/短语/正则提供可搜索文本
- 将二进制文件与富文档的提取流程统一成抽象层

### 两阶段算法
#### 阶段 1：文本提取
输入：文件对象
输出：
- `ExtractedTextRecord`
- 提取状态
- 可选摘要

提取策略：
1. MIME / 扩展名分类
2. 纯文本类直接读取并规范化编码
3. 常见文档类走 extractor adapter
4. 二进制/不可解析文件记录不可提取状态

#### 阶段 2：索引写入
输入：提取结果
输出：索引文档

索引字段：
- file_id
- case_id
- path
- ext
- mime
- timestamps
- extracted_text
- tags

### Tantivy 查询策略
- literal/phrase 走解析语法转义后查询
- regex 走有限字段正则匹配
- 大结果集按分页返回
- 命中高亮在应用层截取上下文窗口

### 高亮算法
1. 得到命中偏移或匹配片段
2. 前后截取固定上下文窗口
3. 生成 `HighlightSpan[]`
4. 返回前端高亮渲染

## 5.6 时间线算法设计

### 时间线数据来源
1. 文件系统 MACB
2. Windows 工件事件
3. 后续扩展的浏览器/系统事件

### 核心思路
使用 **事件归一化 + 投影表**：
- 所有时间相关结果最终投影为统一 `timeline_events`
- 前端不直接消费文件表或工件表

### 归一化算法
输入：文件或工件结果
输出：标准化时间线事件

#### 文件 MACB 归一化
对于每个文件：
- 若 created 存在，生成 `FILE_CREATED`
- 若 modified 存在，生成 `FILE_MODIFIED`
- 若 accessed 存在，生成 `FILE_ACCESSED`
- 若 changed 存在，生成 `FILE_METADATA_CHANGED`

#### 工件事件归一化
例如 Prefetch：
- last_run_time -> `PROGRAM_EXECUTION`
例如 LNK：
- target access/update -> `LINK_ACTIVITY`
例如 Recycle Bin：
- deletion_time -> `FILE_DELETED`

### 查询算法
- 按时间范围做索引扫描
- 按事件类型、数据源、路径前缀、artifact type 过滤
- 返回分页/分桶结果

### 聚合算法
时间线缩放时按 bucket 聚合：
- minute
- hour
- day
- week

#### bucket 聚合步骤
1. 根据时间跨度选 bucket 粒度
2. 将事件映射到时间桶
3. 统计数量和主要事件类型
4. 返回 `TimelineBucket[]`

## 5.7 Windows 痕迹算法设计

### 通用模式
每个工件解析器遵循统一流程：
1. 目标发现
2. 原始读取
3. 格式解析
4. 语义标准化
5. 写入 ArtifactRecord
6. 发出 TimelineEvent（如适用）

### 解析器执行模型
- 每种解析器注册为一个 `ArtifactExtractor`
- 调度器根据 `supports()` 与 `dependencies()` 决定执行顺序
- 单个文件可被多个解析器消费

### 5.7.1 Registry
#### 输入
- SYSTEM/SOFTWARE/SAM/NTUSER.DAT/USRCLASS.DAT 等 hive

#### 输出
- registry key/value artifact
- run keys / userassist / recentdocs 等后续可扩展

#### 算法
1. 定位 hive 文件
2. 建立 hive reader
3. 按规则表提取 key/value
4. 规范化路径、值类型、时间戳
5. 写入 artifact 表

### 5.7.2 Prefetch
#### 输出关键字段
- executable_name
- run_count
- last_run_times[]
- referenced_files[]

#### 算法
1. 扫描 `Windows/Prefetch`
2. 根据版本解析头和记录结构
3. 提取执行次数与时间
4. 生成程序执行类事件

### 5.7.3 LNK
#### 输出关键字段
- target_path
- working_dir
- drive_serial
- mac times
- source_path

#### 算法
1. 扫描 `.lnk`
2. 解析 shell link header 和 optional blocks
3. 统一目标路径与时间戳
4. 生成 link activity 事件

### 5.7.4 Jump List
#### 算法
1. 定位 automatic/custom destinations
2. 解析容器结构
3. 提取目标文件、应用、访问时间
4. 生成 recent activity 事件

### 5.7.5 Recycle Bin
#### 算法
1. 定位 `$Recycle.Bin`
2. 配对 `$I` / `$R`
3. 解析原始路径、删除时间、文件大小
4. 生成删除事件

### 5.7.6 SRU
#### 算法
1. 定位 SRU 数据库
2. 解析目标表
3. 将程序使用、网络或能耗相关记录标准化
4. 写入 artifact 表

## 5.8 报告导出算法

### 支持输出
- HTML
- JSON
- CSV
- Evidence bundle

### 导出流程
1. 接收报告配置
2. 解析数据范围：标签、命中、时间范围、工件类型
3. 调用 query service 聚合数据
4. 将数据映射到 report view model
5. 渲染模板或写出结构化文件
6. 记录 `ReportRecord`

### HTML 报告算法
- 将案件摘要、数据源摘要、命中摘要、时间线摘要、工件摘要渲染为静态 HTML
- 图片/附件复制到 report 目录

### CSV 导出算法
- 每种 artifact type 单独 schema
- 或者按“扁平统一字段 + attrs_json”导出

### 5.9 开发可追溯报告模块

这是项目工程治理能力，不属于最终取证产品对外功能，但必须在仓库内设计和落地。

#### 目标
- 记录每次由 agent 或人工推动的开发活动
- 为每条开发事件保留时间戳、主体、动作、对象、结果
- 支持后续生成阶段报告、里程碑报告、交付追踪报告
- 为关键设计与实现提供可追溯链路

#### 存放目录
所有开发追踪材料放在仓库根目录：

```text
development-reports/
  README.md
  sessions/
    YYYY-MM-DD/
      session-XXX-agent-main.md
      session-XXX-events.jsonl
  summaries/
    weekly/
    milestone/
```

#### 内容分层
1. `session-XXX-agent-main.md`
   - 本次会话摘要
   - 目标
   - 关键决策
   - 产出文件
   - 风险/阻塞
   - 下一步建议
2. `session-XXX-events.jsonl`
   - 逐条事件日志
   - 适合程序追加写入
3. `summaries/*`
   - 周报、里程碑报告、阶段汇总

#### 事件模型
```rust
struct DevTraceEvent {
    event_id: String,
    ts: DateTime<Utc>,
    session_id: String,
    agent_id: String,
    agent_name: String,
    actor_type: String,
    action: String,
    target_type: String,
    target_ref: String,
    status: String,
    summary: String,
    metadata: serde_json::Value,
}
```

#### agent 署名规范
每条开发事件必须包含：
- `agent_id`：唯一标识一次 agent 实例
- `agent_name`：如 `claude-main`、`planner-agent`、`review-agent`
- `actor_type`：`agent` / `human` / `system`

#### 事件戳规范
- 使用 UTC ISO8601，例如 `2026-05-16T08:12:34Z`
- 事件文件按时间追加写入 JSONL
- 汇总文件引用对应 session_id 和 event_id 范围

#### 写入策略
- 关键动作触发写入：
  - 文档创建/修改
  - 目录骨架初始化
  - 设计决策确认
  - 核心模块落地
  - 测试执行
  - 阶段性失败或阻塞
- 高频细粒度运行日志不直接写入开发报告目录，避免噪音
- 开发追溯日志是“决策/动作级”，不是 debug trace 替代品

#### 推荐事件类型
- `doc.created`
- `doc.updated`
- `decision.recorded`
- `workspace.initialized`
- `crate.created`
- `schema.created`
- `job.run`
- `test.passed`
- `test.failed`
- `blocker.identified`
- `milestone.completed`

#### session markdown 模板
```md
# Session Report

- session_id: session-001
- agent_name: claude-main
- started_at: 2026-05-16T08:00:00Z
- ended_at: 2026-05-16T09:20:00Z

## Goals
- ...

## Key Decisions
- ...

## Artifacts Changed
- design.md
- spec.md

## Risks / Open Questions
- ...

## Next Steps
- ...
```

#### 与产品内审计日志的区分
- `development-reports/`：工程研发过程追溯
- `case-root/logs/`：产品运行日志
- `case-root/reports/`：用户导出的取证报告

### 5.10 临时运行时缓存数据库

这是一个与主案件数据库分离的 **temporary runtime DB**，用于存放易失性、可重建、与运行期性能优化相关的缓存字段。

#### 目标
- 不污染主案件数据库
- 存放预览句柄、临时摘要、分页缓存、搜索中间态、媒体转码索引等字段
- 支持会话级和案件级缓存
- 支持 TTL 与启动清理

#### 设计原则
- 临时库中的数据必须视为可丢失
- 任何关键取证结果不得只存在临时库
- 临时库中的字段都应可由主库 + 原始证据重新构建
- 临时库优先服务性能和交互，不服务证据完整性

#### 存放位置
建议在案件目录和应用运行目录双层放置：

1. **案件级临时库**
```text
case-root/cache/runtime.db
```
适用于与当前案件密切相关但可重建的缓存。

2. **全局会话级临时库**
```text
app-runtime/cache/session.db
```
适用于与应用实例相关的跨案件短期缓存，如 UI 预览句柄、最近分页状态。

第一版至少实现 **案件级临时库 `case-root/cache/runtime.db`**。

#### 适合放入临时库的数据
- 文件预览句柄映射
- 大文本切片缓存
- 十六进制分页缓存
- 图片/媒体临时转码索引
- 搜索结果页缓存
- 时间线聚合 bucket 缓存
- 临时统计字段
- 任务阶段中间快照

#### 不应放入临时库的数据
- 案件元信息
- 数据源注册信息
- 文件对象主目录
- 哈希最终结果
- 正式 artifact 记录
- 正式 timeline event
- 正式 report record
- 标签/备注

#### 运行时缓存库建议表
- `cache_entries`
- `file_handles`
- `preview_chunks`
- `search_result_pages`
- `timeline_bucket_cache`
- `job_runtime_snapshots`
- `media_preview_assets`

#### 通用缓存表模型
```rust
struct CacheEntry {
    cache_key: String,
    namespace: String,
    case_id: Option<String>,
    value_json: serde_json::Value,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    last_accessed_at: DateTime<Utc>,
}
```

#### file handle 表
```rust
struct FileHandleCache {
    handle_id: String,
    case_id: String,
    object_id: String,
    opened_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    access_mode: String,
}
```

#### search result page 缓存
用于避免用户短时间反复翻页时重复跑重查询。

字段建议：
- `cache_key`
- `query_hash`
- `page`
- `page_size`
- `result_json`
- `expires_at`

#### TTL 策略
- 文件句柄：5~30 分钟
- 搜索分页缓存：5~15 分钟
- 时间线 bucket 缓存：15~60 分钟
- 媒体预览资产：会话结束或案件关闭后清理

#### 清理策略
1. 应用启动时清理过期缓存
2. 案件关闭时清理该案件相关临时缓存
3. 定时后台清理过期项
4. 用户可手动“清空缓存”

#### 与主数据库协作方式
- 主数据库提供权威数据
- 临时库只存 query result、preview state、runtime handle
- app-services 在 query path 上先查临时库，再决定是否回源主库/索引/证据源

#### 查询路径示例
##### 文件预览
1. UI 请求文件预览
2. runtime-cache 查 `file_handles`
3. 无命中则打开底层 reader，登记 handle
4. 读取指定 range
5. 如需要，将 chunk 写入 `preview_chunks`

##### 时间线聚合
1. UI 请求某时间范围聚合
2. 生成 query hash
3. runtime-cache 查 `timeline_bucket_cache`
4. 命中则直接返回
5. 未命中则主库查询 + 聚合后写缓存

#### 生命周期约束
- `runtime.db` 不纳入案件正式证据结果导出
- `runtime.db` 可在异常退出后自动恢复使用，但任何损坏都不影响案件主流程
- 若 `runtime.db` 删除，系统应仅损失性能，不损失结果正确性

## 6. Ingest/任务编排详细设计

## 6.1 任务对象模型
```rust
struct Job {
    id: JobId,
    case_id: CaseId,
    kind: JobKind,
    status: JobStatus,
    progress: JobProgress,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

struct JobTask {
    id: TaskId,
    job_id: JobId,
    task_type: TaskType,
    priority: u8,
    payload: serde_json::Value,
}
```

## 6.2 任务种类
- `EnumerateFs`
- `ComputeHash`
- `ExtractText`
- `IndexText`
- `ExtractArtifact`
- `ProjectTimeline`
- `ExportReport`

## 6.3 调度算法
使用 **多队列 + 优先级 + 可取消 token**：
- 数据源导入后先创建 `EnumerateFs`
- 文件被 catalog 收录后，根据规则投递后续任务
- 重要 UI 直观结果优先，例如目录树 > 文本提取 > 二级工件

### 优先级策略
- P0：案件创建、打开、目录树基本可见
- P1：文件详情、预览、选中文件直接相关请求
- P2：全文索引
- P3：二级痕迹提取和批量导出

## 6.4 取消与恢复
- 每个 Job 持有 `CancellationToken`
- Worker 在每个阶段检查取消状态
- 被取消任务进入 `Cancelled`
- 已完成结果不回滚，但标记任务未完成

## 7. 应用内链路数据传输形式

这里的“链路层数据传输形式”定义为：**桌面应用内前端 React 与 Rust 后端之间的请求、响应、事件、二进制数据交换协议**。

## 7.1 传输原则
- 命令请求：**Tauri command + JSON DTO**
- 异步通知：**Tauri event + JSON event envelope**
- 大文件/字节流：**句柄式读取，不直接一次性传全量 bytes**
- 表格结果：**分页 DTO**
- 事件推送：**统一 envelope，前端按 topic 分发**

## 7.2 请求/响应 DTO 规范
### 命名约定
- Request: `XxxRequest`
- Response: `XxxResponse`
- Summary: `XxxSummary`
- Detail: `XxxDetail`

### 分页结构
```rust
struct PageRequest {
    page: u32,
    page_size: u32,
}

struct PageResponse<T> {
    items: Vec<T>,
    page: u32,
    page_size: u32,
    total: u64,
}
```

### 错误结构
```rust
struct ApiErrorDto {
    code: String,
    message: String,
    details: Option<serde_json::Value>,
    recoverable: bool,
}
```

## 7.3 统一事件包结构
```rust
struct EventEnvelope<T> {
    event_id: String,
    topic: String,
    ts: DateTime<Utc>,
    payload: T,
}
```

### topic 约定
- `case.opened`
- `case.closed`
- `job.created`
- `job.started`
- `job.progress`
- `job.completed`
- `job.failed`
- `artifact.added`
- `timeline.updated`
- `search.index_progress`

## 7.4 大文件数据传输策略
### 不采用
- React 直接一次性拿全文件 bytes
- 对大十六进制内容走 JSON base64 全量返回

### 采用
**range read handle 模型**：
1. 前端请求打开某文件预览
2. 后端返回 `file_handle_id`
3. 前端按 offset/length 分段读取
4. 十六进制和文本 viewer 按需懒加载

示例：
```rust
struct OpenFileHandleResponse {
    handle_id: String,
    size: u64,
    mime: Option<String>,
}

struct ReadFileRangeRequest {
    handle_id: String,
    offset: u64,
    length: u32,
}
```

### 编码形式
- 对文本：UTF-8 字符串 + 编码信息
- 对十六进制查看：返回原始 bytes 的 base64 或十六进制字符串分块
- 对图片/媒体：导出到临时缓存路径，由前端通过 Tauri 文件协议访问

## 7.5 前端 API 封装约定
前端只通过：
- `src/lib/api/*.ts`
- `src/lib/events/*.ts`

调用 Rust 能力。

禁止：
- 组件内直接散落 `invoke()`
- 组件自行订阅裸事件 topic

## 8. 前端骨架设计

## 8.1 前端目录建议
```text
apps/desktop/src/
  app/
    App.tsx
    router.tsx
    providers.tsx
  pages/
    case-home/
      index.tsx
    import-datasource/
    file-browser/
    search/
    timeline/
    artifacts/
    reports/
  features/
    case/
      api.ts
      hooks.ts
      store.ts
      components/
    datasource/
    files/
    jobs/
    search/
    timeline/
    artifacts/
    reports/
  components/
    layout/
      AppShell.tsx
      Sidebar.tsx
      Topbar.tsx
      RightPanel.tsx
    tables/
      DataTable.tsx
    viewers/
      HexViewer.tsx
      TextViewer.tsx
      ImageViewer.tsx
    status/
      JobStatusBadge.tsx
      ProgressBar.tsx
  lib/
    api/
      client.ts
      case.ts
      datasource.ts
      files.ts
      jobs.ts
      search.ts
      timeline.ts
      artifacts.ts
      reports.ts
    events/
      bus.ts
      subscribers.ts
    schemas/
      case.ts
      job.ts
      search.ts
  stores/
    ui-store.ts
    selection-store.ts
  hooks/
    useDebounce.ts
    useEventSubscription.ts
  types/
```

## 8.2 页面布局骨架
### 主布局
- 左侧：Case Explorer / Data Source Tree / Artifact Category
- 中间：主内容区
- 右侧：Details Inspector
- 底部或抽屉：Tasks / Logs / Warnings

### 核心页面
#### CaseHomeView
- 最近案件
- 当前案件概览
- 数据源摘要
- 最近任务

#### FileBrowserView
- 左树右表
- 下方或右侧详情
- viewer tab：metadata / text / hex / preview

#### SearchView
- 查询输入区
- filters 面板
- results table
- hit preview panel

#### TimelineView
- 时间范围选择
- 粒度缩放
- 聚合图 + 事件表
- 点击事件跳转对象详情

#### ArtifactsView
- 左侧 artifact family 列表
- 中间表格
- 右侧属性详情

## 8.3 状态管理分工
### TanStack Query
负责：
- 案件详情
- 文件列表
- 搜索结果
- 时间线查询结果
- 工件分页结果

### Zustand/Redux
负责：
- 当前选中 case/data_source/file/artifact
- 右侧面板 tab
- UI 布局状态
- 当前 viewer 配置

### Event Store
负责：
- 正在运行的任务快照
- 实时进度
- 实时新增工件通知

## 8.4 组件设计原则
- 页面只负责拼装 feature
- feature 内部再拆 container/presentation
- viewer 与数据获取解耦
- 所有表格组件使用统一 column schema 模式

## 9. 关键 DTO 与前端类型设计

## 9.1 案件摘要
```ts
export interface CaseSummary {
  id: string
  name: string
  number?: string
  examiner?: string
  createdAt: string
  updatedAt: string
}
```

## 9.2 文件条目
```ts
export interface FileEntryRow {
  id: string
  parentId?: string
  path: string
  name: string
  entryType: 'file' | 'directory'
  size?: number
  ext?: string
  deleted: boolean
  createdAt?: string
  modifiedAt?: string
  accessedAt?: string
  changedAt?: string
  hashSha256?: string
}
```

## 9.3 搜索命中
```ts
export interface SearchHit {
  fileId: string
  path: string
  score: number
  snippets: Array<{
    text: string
    highlights: Array<{ start: number; end: number }>
  }>
}
```

## 9.4 时间线事件
```ts
export interface TimelineEventDto {
  id: string
  sourceObjectId: string
  eventType: string
  ts: string
  title: string
  description: string
  attrs: Record<string, unknown>
}
```

## 9.5 工件记录
```ts
export interface ArtifactRow {
  id: string
  artifactType: string
  title: string
  summary: string
  sourceObjectId?: string
  createdAt: string
  attrs: Record<string, unknown>
}
```

## 10. 典型调用链设计

## 10.1 导入数据源链路
```text
UI 点击导入
-> invoke(import_data_source)
-> datasource command handler
-> app-services::datasource_service.attach()
-> persistence 写 data_source
-> ingest 创建 job
-> ingest 投递 EnumerateFs
-> job.started 事件发给前端
-> 文件逐步入库
-> job.progress 持续推送
-> UI 树和列表刷新
```

## 10.2 搜索链路
```text
UI 输入查询
-> invoke(search)
-> search command handler
-> app-services::search_service.query()
-> tantivy 查询
-> 高亮构建
-> PageResponse<SearchHit>
-> UI 渲染列表和 snippet
```

## 10.3 Windows 痕迹提取链路
```text
UI 点击运行工件模块
-> invoke(start_artifact_job)
-> job_service 创建 job
-> ingest 调度对应 extractor
-> extractor 写 artifact records
-> timeline projector 写 timeline_events
-> artifact.added / timeline.updated 事件
-> UI 实时更新 artifacts view 和 timeline view
```

## 11. 数据库与索引初始化骨架

### 11.1 主案件数据库 migration 初始建议
- `0001_cases.sql`
- `0002_data_sources.sql`
- `0003_file_entries.sql`
- `0004_file_hashes.sql`
- `0005_artifacts.sql`
- `0006_timeline_events.sql`
- `0007_tags_notes.sql`
- `0008_jobs.sql`
- `0009_reports.sql`

### 11.2 临时运行时数据库 migration 建议
- `1001_cache_entries.sql`
- `1002_file_handles.sql`
- `1003_preview_chunks.sql`
- `1004_search_result_pages.sql`
- `1005_timeline_bucket_cache.sql`
- `1006_job_runtime_snapshots.sql`
- `1007_media_preview_assets.sql`

### 11.3 索引目录
```text
indexes/
  tantivy/
    meta.json
    segment_*
cache/
  runtime.db
```

### 11.4 开发追溯目录初始化
```text
development-reports/
  README.md
  sessions/
  summaries/
```

### 11.5 开发追溯初始化规则
- 仓库初始化时创建 `development-reports/README.md`
- 每个新开发会话创建一个 session markdown 和一个 JSONL event 文件
- milestone 完成时生成一份 summary markdown

## 12. 测试设计

## 12.1 测试分层目标
- 保证领域逻辑正确
- 保证解析器对 fixture 稳定
- 保证前后端 DTO/事件契约稳定
- 保证桌面主流程可回归
- 保证临时缓存库不会影响主结果正确性
- 保证开发追溯报告机制可写入、可回放、可汇总

## 12.2 Rust
- domain 单元测试
- parser fixture 测试
- search 集成测试
- timeline projection 测试
- sqlite repository 测试
- ingest 调度测试
- runtime-cache TTL/清理测试
- traceability writer 测试

## 12.3 前端
- feature hook 单元测试
- 关键页面组件测试
- DTO schema 校验测试
- event bus 分发测试
- range preview viewer 状态测试

## 12.4 端到端
- 创建案件
- 导入逻辑目录
- 浏览文件
- 建索引并搜索
- 运行预取/lnk fixture
- 生成 HTML 报告
- 打开文件预览并命中 runtime cache
- 生成开发追溯 session 报告

## 12.5 test 文件目录设计

建议目录：

```text
forensics/
  crates/
    domain/
      tests/
    persistence-sqlite/
      tests/
    runtime-cache/
      tests/
        ttl_cleanup.rs
        file_handles.rs
        preview_chunks.rs
    traceability/
      tests/
        event_log_append.rs
        session_report_render.rs
    search/
      tests/
        query_parser.rs
        highlighter.rs
        indexing_flow.rs
    timeline/
      tests/
        projection_macb.rs
        projection_artifacts.rs
        bucket_aggregation.rs
    artifacts-windows/
      tests/
        prefetch_fixture.rs
        lnk_fixture.rs
        recycle_bin_fixture.rs
        registry_fixture.rs
  apps/
    desktop/
      src/
      tests/
        e2e/
          create_case.spec.ts
          import_datasource.spec.ts
          search_flow.spec.ts
          timeline_flow.spec.ts
          report_export.spec.ts
          devtrace_generation.spec.ts
        component/
          AppShell.test.tsx
          FileBrowserView.test.tsx
          SearchView.test.tsx
  testdata/
    images/
    artifacts/
      windows/
        prefetch/
        lnk/
        recycle-bin/
        registry/
    reports/
    runtime-cache/
    traceability/
```

### 12.6 命名规范
- Rust 单元/集成测试：`*_test.rs` 或按能力域命名，如 `bucket_aggregation.rs`
- 前端组件测试：`*.test.tsx`
- 前端端到端测试：`*.spec.ts`
- fixture 按工件家族分目录，避免所有样本堆在一起

### 12.7 临时缓存数据库测试重点
- 缓存命中后结果与回源结果一致
- TTL 过期后自动失效
- 删除 `runtime.db` 后功能仍可正常回源
- 案件关闭后 case-scoped 缓存被清理

### 12.8 开发追溯测试重点
- 每个事件都写入 agent_name 与 ts
- JSONL 追加顺序正确
- session markdown 可从事件和摘要输入稳定生成
- 不同 session 不串写文件

## 13. 前后端 CI 设计

## 13.1 CI 目标
- 阻止格式、类型、测试回归进入主分支
- 同时校验 Rust 后端、React 前端、Tauri 壳层与文档/契约
- 在关键模块变更时校验 traceability 与 runtime-cache 相关测试

## 13.2 CI 分层
### Backend CI
触发条件：
- `crates/**`
- `apps/desktop/src-tauri/**`
- `Cargo.toml`

步骤建议：
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. 运行关键 fixture 测试集
5. 生成测试摘要 artifact

### Frontend CI
触发条件：
- `apps/desktop/src/**`
- `package.json`
- `pnpm-lock.yaml`

步骤建议：
1. `pnpm install --frozen-lockfile`
2. `pnpm lint`
3. `pnpm typecheck`
4. `pnpm test`
5. 需要时运行轻量 e2e smoke

### Desktop Integration CI
触发条件：
- `apps/desktop/**`
- `transport/**`
- 关键 DTO/事件协议变更

步骤建议：
1. 构建 Tauri app
2. 运行桌面集成 smoke test
3. 校验 command/event 契约未破坏

### Docs / Contract CI
触发条件：
- `PRD.md`
- `spec.md`
- `design.md`
- `development-reports/**`

步骤建议：
1. markdown lint
2. 校验关键文档存在
3. 可选：检查新增开发会话是否补 development report

## 13.3 推荐工作流
- `ci-backend.yml`
- `ci-frontend.yml`
- `ci-desktop.yml`
- `ci-docs.yml`

## 13.4 缓存策略
- Cargo target/cache
- pnpm store cache
- fixture 下载缓存
- 但不要缓存 `runtime.db` 或任何 case runtime 产物

## 13.5 失败阻断策略
必须阻断 merge：
- Rust fmt/clippy/test 失败
- 前端 lint/typecheck/test 失败
- DTO/schema 破坏性变更但未同步测试
- traceability 必填字段缺失的测试失败

可先告警不阻断：
- 文档建议项
- 非关键 e2e flaky 用例

## 13.6 发布前额外检查
- 运行完整后端 fixture 集
- 运行前端 smoke + 关键 e2e
- 构建桌面安装包
- 校验 development-reports 中存在当前里程碑 summary

## 14. MVP 初始化顺序

### Step 1: 工程骨架
- 建 Rust workspace
- 建 Tauri app
- 建 React app shell
- 建 transport DTO crate
- 建 development-reports 目录骨架

### Step 2: 案件与持久化
- case service
- sqlite schema
- runtime cache db schema
- recent/open case UI

### Step 3: 数据源与文件树
- raw/logical directory
- file tree + file list
- metadata/detail viewer
- preview handle cache

### Step 4: 任务系统
- job model
- progress events
- task drawer UI
- dev trace event append

### Step 5: 搜索
- 文本提取
- tantivy 索引
- 搜索页面
- 搜索分页缓存

### Step 6: Windows 工件第一批
- Prefetch
- LNK
- Recycle Bin
- Registry basic

### Step 7: 时间线与报告
- timeline projection
- timeline UI
- HTML/JSON/CSV export
- session/milestone development summary

## 15. 明确不这样做

- 不把所有逻辑放进 `src-tauri/src/main.rs`
- 不让前端直接感知 SQLite schema
- 不让解析器直接发 UI 事件
- 不让搜索模块直接改时间线表
- 不把大文件预览做成一次性全量读取
- 不先做多用户/远程协作

## 16. 当前建议的第一批 crate 最小集合

如果想尽快起步，第一批只建这些即可：
- `domain`
- `app-services`
- `persistence-sqlite`
- `runtime-cache`
- `traceability`
- `evidence-core`
- `image-raw`
- `catalog`
- `ingest`
- `search`
- `timeline`
- `artifacts-core`
- `artifacts-windows`
- `reports`
- `transport`
- `apps/desktop`

这样既不会过早拆太细，也能保证后续扩展时不推倒重来。
### Step 1: 工程骨架
- 建 Rust workspace
- 建 Tauri app
- 建 React app shell
- 建 transport DTO crate

### Step 2: 案件与持久化
- case service
- sqlite schema
- recent/open case UI

### Step 3: 数据源与文件树
- raw/logical directory
- file tree + file list
- metadata/detail viewer

### Step 4: 任务系统
- job model
- progress events
- task drawer UI

### Step 5: 搜索
- 文本提取
- tantivy 索引
- 搜索页面

### Step 6: Windows 工件第一批
- Prefetch
- LNK
- Recycle Bin
- Registry basic

### Step 7: 时间线与报告
- timeline projection
- timeline UI
- HTML/JSON/CSV export

## 14. 明确不这样做

- 不把所有逻辑放进 `src-tauri/src/main.rs`
- 不让前端直接感知 SQLite schema
- 不让解析器直接发 UI 事件
- 不让搜索模块直接改时间线表
- 不把大文件预览做成一次性全量读取
- 不先做多用户/远程协作

## 15. 当前建议的第一批 crate 最小集合

如果想尽快起步，第一批只建这些即可：
- `domain`
- `app-services`
- `persistence-sqlite`
- `evidence-core`
- `image-raw`
- `catalog`
- `ingest`
- `search`
- `timeline`
- `artifacts-core`
- `artifacts-windows`
- `reports`
- `transport`
- `apps/desktop`

这样既不会过早拆太细，也能保证后续扩展时不推倒重来。