# Forensics Workbench 模型 / 架构 / 算法流程图

本文档集中维护 Forensics Workbench 的 Mermaid 图谱。图谱描述当前工程模型和审计关注点，详细字段与实现仍以代码、迁移和 transport DTO 为准。

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
    text entry_type
    int size
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
  Hook["功能 hook<br/>useQuery/useMutation"]
  ApiFn["领域 API 函数"]
  ApiClient["ApiClient.request"]
  TauriMode{"VITE_API_MODE == tauri?"}
  Invoke["Tauri invoke"]
  Mock["Mock provider"]
  QueryCache["TanStack Query cache"]
  Store["Zustand UI/选择状态"]

  Page --> Hook
  Page --> Store
  Hook --> ApiFn
  Hook --> QueryCache
  ApiFn --> ApiClient
  ApiClient --> TauriMode
  TauriMode -->|是| Invoke
  TauriMode -->|否| Mock
  Invoke --> Backend["Rust command"]
  Mock --> Hook
  Backend --> Hook
```

## 5. Tauri Command Request / Response 序列图

```mermaid
sequenceDiagram
  participant UI as React 组件
  participant Hook as 功能 hook
  participant API as 前端 API 封装
  participant Client as ApiClient
  participant Cmd as Tauri command
  participant Svc as app-services
  participant Repo as SQLite 仓储 / 核心 crate

  UI->>Hook: 用户操作或查询挂载
  Hook->>API: 调用领域 API
  API->>Client: request(command, mockFallback, payload)
  alt tauri mode
    Client->>Cmd: invoke(command, payload)
    Cmd->>Svc: 校验并委派
    Svc->>Repo: 读取 / 写入 / 查询 / 处理
    Repo-->>Svc: 领域或核心结果
    Svc-->>Cmd: transport DTO
    Cmd-->>Client: Result<T, String>
  else mock mode
    Client-->>API: mock provider 结果
  end
  Client-->>API: 类型化响应
  API-->>Hook: data
  Hook-->>UI: 渲染状态
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
  Validate["校验案件与源路径"]
  Job["创建导入任务"]
  Probe["探测源类型<br/>logical/raw/e01"]
  Reader["打开只读 reader"]
  Hash["可选证据 hash/provenance"]
  Volume["识别卷系统<br/>MBR/GPT/none"]
  Partitions["持久化分区记录"]
  FSLoop["遍历支持的分区"]
  FsOpen["打开文件系统 parser<br/>NTFS/FAT/exFAT"]
  Enumerate["枚举根目录或 staged 文件条目"]
  Store["写入 file_entries 与进度"]
  Partial["发出 partial result 与 phase progress"]
  Finish["标记任务完成"]
  Fail["标记任务失败并脱敏错误"]

  Start --> Validate --> Job --> Probe --> Reader --> Hash --> Volume --> Partitions --> FSLoop
  FSLoop --> FsOpen --> Enumerate --> Store --> Partial --> Finish
  Probe -->|不支持| Fail
  Reader -->|打开失败| Fail
  FsOpen -->|不支持的文件系统| Partial
  Enumerate -->|可恢复 warning| Partial
  Enumerate -->|致命错误| Fail
```

## 8. 文件系统枚举与目录树懒加载

```mermaid
flowchart LR
  UI["FileBrowser 页面"]
  Hook["useFileChildren / useFileRows"]
  API["files API"]
  Command["get_file_children_request"]
  Service["file_service"]
  Repo["file_repo"]
  FS["filesystem reader 回退"]
  Page["分页 children DTO"]

  UI --> Hook
  Hook --> API
  API --> Command
  Command --> Service
  Service --> Repo
  Service -->|缓存未命中或直接读取路径| FS
  Repo --> Page
  FS --> Page
  Page --> Hook
  Hook --> UI
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
  Highlight["构造 snippets/highlights"]
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
  Enrich["补充 parser/provenance/confidence"]
  Persist["持久化 timeline_events"]
  Query["时间线查询<br/>range/type/source/page"]
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
  Select["选择 parser<br/>EVTX/Prefetch/LNK/JumpList/Registry/RecycleBin/SRU/Thumbcache"]
  Read["通过 evidence reader 有界读取"]
  Parse["解析记录"]
  Recover{"是否可恢复问题?"}
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
  Parse -->|不允许 panic/OOM| Fatal
```

## 12. 报告导出流程

```mermaid
flowchart LR
  Request["导出范围请求"]
  Validate["校验案件与输出路径"]
  Collect["收集选中发现<br/>files/artifacts/timeline/tags"]
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
  Svc["app-services/state 中的 MCP 编排"]
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

- 修改架构、契约、事件、导入、搜索、时间线、工件、报告或 MCP 时，先更新对应图，再更新实现或审计记录。
- Mermaid 图不替代代码审计；字段、索引、enum 和 payload 形状仍以源码和 migrations 为准。
- 图谱中出现的新边界若尚未实现，必须在正文标注为设计目标，避免误导为当前能力。
