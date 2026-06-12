# Forensics Workbench 模型 / 架构 / 算法流程图

本文档集中维护 Forensics Workbench 的 Mermaid 图谱。图谱描述当前工程模型和关键链路，尤其覆盖分区根节点、文件浏览排序、状态字段传播和真实 Tauri 请求路径。字段细节仍以源码、迁移和 `crates/transport` 契约为准。

## 1. 分层架构图

```mermaid
flowchart TB
  Investigator["调查员"]
  UI["React 界面<br/>frontend/"]
  Hooks["功能 hooks<br/>TanStack Query"]
  Api["API 封装<br/>frontend/src/lib/api"]
  Client["ApiClient<br/>mock 或 tauri 模式"]
  Commands["Tauri command 层<br/>apps/desktop/src-tauri/src/commands"]
  Services["应用服务层<br/>crates/app-services"]
  Transport["传输契约<br/>crates/transport"]
  Domain["领域实体<br/>crates/domain"]
  Persistence["SQLite 仓储<br/>crates/persistence-sqlite"]
  Evidence["证据读取层<br/>evidence-core image-* fs-*"]
  Analysis["分析引擎<br/>search timeline artifacts catalog reports ingest"]
  Infra["基础设施<br/>logging hashing fs text clock config"]

  Investigator --> UI
  UI --> Hooks
  Hooks --> Api
  Api --> Client
  Client -->|invoke| Commands
  Client -->|mock 回退| Mock["Mock provider<br/>mock-data.ts"]
  Commands --> Services
  Commands --> Transport
  Services --> Transport
  Services --> Domain
  Services --> Persistence
  Services --> Evidence
  Services --> Analysis
  Evidence --> Infra
  Analysis --> Infra
  Persistence --> Infra
```

## 2. Rust Crate 依赖图

```mermaid
flowchart LR
  Desktop["forensics-desktop<br/>Tauri shell"]
  App["app-services"]
  Transport["transport"]
  Domain["domain"]
  Infra["infrastructure"]
  Persistence["persistence-sqlite"]
  Runtime["runtime-cache"]
  Evidence["evidence-core"]
  Raw["image-raw"]
  E01["image-e01"]
  NTFS["fs-ntfs"]
  FAT["fs-fat"]
  EXFAT["fs-exfat"]
  Catalog["catalog"]
  Ingest["ingest"]
  Search["search"]
  Timeline["timeline"]
  ArtifactsCore["artifacts-core"]
  ArtifactsWin["artifacts-windows"]
  Reports["reports"]
  Mcp["mcp-client"]
  Testing["testing"]
  Evtx["evtx-patched"]

  Desktop --> App
  Desktop --> Transport
  App --> Transport
  App --> Domain
  App --> Persistence
  App --> Runtime
  App --> Evidence
  App --> Raw
  App --> E01
  App --> NTFS
  App --> FAT
  App --> EXFAT
  App --> Catalog
  App --> Ingest
  App --> Search
  App --> Timeline
  App --> ArtifactsWin
  App --> Reports
  App --> Mcp
  Persistence --> Domain
  Persistence --> Infra
  Runtime --> Infra
  Evidence --> Infra
  Raw --> Evidence
  E01 --> Evidence
  NTFS --> Evidence
  FAT --> Evidence
  EXFAT --> Evidence
  Catalog --> Domain
  Search --> Domain
  Timeline --> Domain
  ArtifactsWin --> ArtifactsCore
  ArtifactsWin --> Evtx
  Reports --> Domain
  Testing --> Domain
  Testing --> Persistence
```

## 3. 核心领域 / 数据库模型 ER 图

```mermaid
erDiagram
  CASES ||--o{ DATA_SOURCES : owns
  CASES ||--o{ JOBS : tracks
  CASES ||--o{ REPORTS : exports
  CASES ||--o{ TAGS : defines
  CASES ||--o{ AUDIT_LOG : records
  DATA_SOURCES ||--o{ PARTITIONS : describes
  DATA_SOURCES ||--o{ FILE_ENTRIES : contains
  FILE_ENTRIES ||--o{ FILE_ENTRIES : parent_of
  FILE_ENTRIES ||--o{ ARTIFACTS : source_object
  FILE_ENTRIES ||--o{ TIMELINE_EVENTS : source_object
  TAGS ||--o{ TAG_BINDINGS : binds

  CASES {
    text id PK
    text name
    text number
    text examiner
    text created_at
    text updated_at
  }

  DATA_SOURCES {
    text id PK
    text case_id FK
    text kind
    text source_path
    text source_hash_sha256
    text provenance_status
  }

  PARTITIONS {
    text id PK
    text data_source_id FK
    int partition_index
    text name
    text kind_label
    text status
    int offset
    int length
    text filesystem
  }

  FILE_ENTRIES {
    text id PK
    text parent_id FK
    text data_source_id FK
    text path
    text name
    text entry_type
    text ext
    int size
    int deleted
    int hidden
    int system
    text created_at
    text modified_at
    text accessed_at
    text changed_at
    text hash_sha256
  }

  ARTIFACTS {
    text id PK
    text case_id
    text data_source_id
    text artifact_type
    text source_object_id
    text extractor_id
    real confidence
  }

  TIMELINE_EVENTS {
    text id PK
    text case_id
    text source_object_id
    text event_type
    text ts
    text parser_id
    real confidence
  }

  JOBS {
    text id PK
    text case_id FK
    text kind
    text status
    int progress
  }

  REPORTS {
    text id PK
    text case_id FK
    text template_id
    text status
  }
```

## 4. 前端状态与 API 调用链

```mermaid
flowchart LR
  Page["页面 / 组件"]
  Controls["UI 状态<br/>showHidden / sortKey / sortDirection"]
  Hook["功能 hook<br/>useQuery/useMutation"]
  ApiFn["领域 API 函数"]
  ApiClient["ApiClient.request"]
  Mode{"VITE_API_MODE == tauri?"}
  Invoke["Tauri invoke"]
  Mock["Mock provider"]
  QueryCache["TanStack Query cache"]
  Formatter["partition-display / file-sort / icon overlay"]
  Store["Zustand UI / 选择状态"]

  Page --> Controls
  Page --> Store
  Controls --> Hook
  Hook --> ApiFn
  Hook --> QueryCache
  ApiFn --> ApiClient
  ApiClient --> Mode
  Mode -->|是| Invoke
  Mode -->|否| Mock
  Invoke --> Backend["Rust command"]
  Mock --> Formatter
  Backend --> Formatter
  Formatter --> Hook
  Hook --> Page
```

## 5. Tauri Command Request / Response 序列图

```mermaid
sequenceDiagram
  participant UI as React 组件
  participant Hook as 文件 hooks
  participant API as files API
  participant Client as ApiClient
  participant Cmd as file_commands
  participant Svc as file_service
  participant Repo as SQLite repo

  UI->>Hook: 切换 showHidden / sortKey / sortDirection
  Hook->>API: getFileRowsPage(parentId, offset, limit, showHidden, sortKey, sortDirection)
  API->>Client: request("get_file_rows_request", payload)
  alt tauri mode
    Client->>Cmd: invoke({request:{parentId, offset, limit, showHidden, sortKey, sortDirection}})
    Cmd->>Svc: get_file_rows_for_request(...)
    Svc->>Repo: 读取当前目录可见集合
    Repo-->>Svc: FileEntry[]
    Svc->>Svc: 过滤 hidden/system
    Svc->>Svc: 目录优先 + 状态后置 + 主排序 + 自然名兜底
    Svc->>Svc: 排序后分页切片
    Svc-->>Cmd: FileRowsPageDto(rows with deleted/hidden/system)
    Cmd-->>Client: Result<T, String>
  else mock mode
    Client-->>API: mock rows
    API->>API: 前端展示级排序兜底
  end
  Client-->>API: 类型化响应
  API-->>Hook: page DTO
  Hook-->>UI: 渲染列表、图标状态与面包屑
```

## 6. Backend Event Push 序列图

```mermaid
sequenceDiagram
  participant Task as 后台任务 / 服务
  participant Bridge as Tauri 事件桥
  participant Webview as Webview 事件监听
  participant Bus as EventBus
  participant Cache as Query 缓存
  participant UI as React UI

  Task->>Task: 构造 EventEnvelope payload
  Task->>Bridge: emit(topic, envelope)
  Bridge->>Webview: Tauri event
  Webview->>Bus: publishEvent(envelope)
  Bus->>Cache: 失效或更新相关查询
  Cache->>UI: 重新渲染最新状态
```

## 7. 数据源导入 / 镜像探测 / 分区识别

```mermaid
flowchart TB
  Start["导入数据源请求"]
  Validate["校验案件、源路径、只读打开"]
  Probe["detect_image_filesystem<br/>探测 logical/raw/e01 与分区候选"]
  PersistDS["持久化 data source 与 partition records"]
  Placeholder["按 partition index 插入 placeholder root<br/>__partition_placeholder__/{index}/{status}"]
  PartitionLoop["按分区遍历候选文件系统"]
  SelectMode{"枚举模式"}
  Serial["串行枚举<br/>replace_placeholder_root_with_real(...)"]
  Parallel["并行枚举到 staging DB"]
  Locked["locked / unsupported<br/>保留 placeholder 作为可见分区根"]
  Merge["按 partition index 合并 staging"]
  Fold["折叠裸根 \\ / / / .<br/>并重挂顶层目录到分区根"]
  Promote["提升 placeholder 显示名<br/>Partition n (LABEL)"]
  Progress["更新 job 进度与阶段事件"]
  Finish["导入完成"]
  Fail["失败 / warning / partial"]

  Start --> Validate --> Probe --> PersistDS --> Placeholder --> PartitionLoop
  PartitionLoop --> SelectMode
  SelectMode -->|NTFS / FAT / exFAT 串行| Serial
  SelectMode -->|并行 staging| Parallel
  SelectMode -->|BitLocker / unsupported| Locked
  Serial --> Progress
  Parallel --> Merge --> Fold --> Promote --> Progress
  Locked --> Progress
  Progress --> Finish
  Probe -->|无法识别 / reader 失败| Fail
  Serial -->|recoverable warning| Fail
  Merge -->|事务失败| Fail
```

## 8. 文件系统枚举、树根归一化与文件浏览排序

```mermaid
flowchart TB
  TreeReq["get_file_tree_request(showHidden)"]
  ChildReq["get_file_children_request(parentId, offset, limit, showHidden)"]
  RowReq["get_file_rows_request(parentId, offset, limit, showHidden, sortKey, sortDirection)"]
  Command["Tauri file commands"]
  Service["file_service"]
  Repo["file_entries / partitions repo"]
  Visible["可见性过滤<br/>showHidden=false 时过滤 hidden/system"]
  Normalize["首层残留裸根读侧归一化<br/>\\ / / / . -> 分区显示名"]
  TreeSort["树排序<br/>目录自然名称升序"]
  RowSort["列表排序<br/>目录优先 -> 状态后置 -> 主字段 -> 自然名兜底"]
  Page["排序后分页切片"]
  Dto["FileTreeNodeDto / FileEntryRowDto"]
  Frontend["FileBrowser<br/>formatPartitionDisplayName + FileIconWithStatusOverlay"]

  TreeReq --> Command
  ChildReq --> Command
  RowReq --> Command
  Command --> Service --> Repo
  Repo --> Visible --> Normalize
  Normalize --> TreeSort
  Normalize --> RowSort
  TreeSort --> Page
  RowSort --> Page
  Page --> Dto --> Frontend
```

## 9. 搜索索引与查询流程

```mermaid
flowchart TB
  Files["file_entries"]
  Extract["文本提取<br/>有界读取"]
  Normalize["归一化文本与元数据"]
  Index["Tantivy index writer"]
  Commit["提交索引 segment"]
  Query["搜索请求"]
  Parse["解析查询与过滤条件"]
  Searcher["Tantivy searcher"]
  Highlight["构造 snippets / highlights"]
  Page["SearchResultPage DTO"]

  Files --> Extract --> Normalize --> Index --> Commit
  Query --> Parse --> Searcher --> Highlight --> Page
  Commit --> Searcher
  Page --> UI["Search 页面"]
```

## 10. 时间线归一化与查询流程

```mermaid
flowchart TB
  Sources["来源<br/>文件元数据、工件、任务、parser"]
  Map["映射为归一化事件候选"]
  Validate["校验 timestamp 与 source_object_id"]
  Enrich["补充 parser / provenance / confidence"]
  Persist["持久化 timeline_events"]
  Query["时间线查询<br/>range / type / source / page"]
  Indexes["SQLite 索引<br/>case_id, ts, type, source"]
  Aggregate["可选分组 / 聚合"]
  Dto["TimelineEventDto 分页"]

  Sources --> Map --> Validate --> Enrich --> Persist
  Query --> Indexes --> Aggregate --> Dto
  Persist --> Indexes
```

## 11. Windows Artifact 解析流程

```mermaid
flowchart TB
  Candidate["候选文件或 registry hive"]
  Select["选择 parser<br/>EVTX / Prefetch / LNK / JumpList / Registry / RecycleBin / SRU / Thumbcache"]
  Read["通过 evidence reader 有界读取"]
  Parse["解析记录"]
  Recover{"是否为可恢复问题?"}
  Artifact["ArtifactRow / artifact DB row"]
  Timeline["可选时间线事件"]
  Warn["带 source attribution 的 warning"]
  Fatal["致命 parser error"]

  Candidate --> Select --> Read --> Parse --> Recover
  Recover -->|否| Artifact
  Recover -->|是| Warn
  Warn --> Artifact
  Artifact --> Timeline
  Read -->|invalid / truncated / unsupported| Warn
  Parse -->|不得 panic / OOM| Fatal
```

## 12. 报告导出流程

```mermaid
flowchart LR
  Request["导出范围请求"]
  Validate["校验案件与输出路径"]
  Collect["收集选中发现<br/>files / artifacts / timeline / tags"]
  ViewModel["构建报告 view model"]
  Render{"输出格式"}
  Html["HTML exporter"]
  Csv["CSV exporter"]
  Json["JSON exporter"]
  Bundle["Evidence bundle exporter"]
  Persist["持久化 reports row"]
  Result["返回路径 / 状态 DTO"]

  Request --> Validate --> Collect --> ViewModel --> Render
  Render --> Html
  Render --> Csv
  Render --> Json
  Render --> Bundle
  Html --> Persist
  Csv --> Persist
  Json --> Persist
  Bundle --> Persist
  Persist --> Result
```

## 13. Job 任务状态机

```mermaid
stateDiagram-v2
  [*] --> Pending
  Pending --> Running: 启动
  Pending --> Cancelled: 启动前取消
  Running --> Running: 更新进度
  Running --> Completed: 成功
  Running --> Failed: 致命错误
  Running --> Cancelling: 请求取消
  Cancelling --> Cancelled: 已观察到取消
  Cancelling --> Failed: 清理失败
  Completed --> [*]
  Failed --> [*]
  Cancelled --> [*]
```

## 14. MCP 集成边界图

```mermaid
flowchart TB
  UI["MCP 设置 / 组件"]
  API["前端 MCP API"]
  Cmd["mcp_commands"]
  Svc["app-services / state 中的 MCP 编排"]
  Client["mcp-client"]
  Stdio["Stdio transport"]
  SSE["SSE transport"]
  External["外部 MCP server"]
  Guard["校验与权限边界"]

  UI --> API --> Cmd --> Guard --> Svc --> Client
  Client --> Stdio --> External
  Client --> SSE --> External
  External --> Client --> Svc --> Cmd --> API --> UI
  Guard -.->|"拒绝不安全配置 / tool call"| Cmd
```

## 15. 审计使用说明

- 只要修改分区根模型、`showHidden`/排序契约、`deleted`/`hidden`/`system` 状态传播、导入 merge 规则或文件浏览读路径，就必须先同步本图谱。
- Mermaid 图不替代代码审计；字段、索引、状态枚举和 payload 形状仍以源码、迁移和 `crates/transport` 为准。
- 图谱只描述当前真实实现。尚未落地的想法应写入方案文档，不应混入这里伪装成既成事实。
