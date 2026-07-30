# Forensics Workbench 模型 / 架构 / 算法图谱

本文档维护项目当前的 Mermaid 图谱，图谱以当前实现为准。

## 1. 分层架构图

```mermaid
flowchart TB
  UI["React UI<br/>frontend/"]
  Hooks["Hooks / Query / Store"]
  API["frontend/src/lib/api"]
  Tauri["Tauri Commands"]
  App["app-services"]
  Transport["crates/transport"]
  Domain["crates/domain"]
  Persistence["persistence-sqlite"]
  Evidence["evidence-core / image-* / fs-*"]
  Analysis["search / timeline / artifacts / reports / ingest"]
  Infra["infrastructure"]

  UI --> Hooks --> API --> Tauri
  Tauri --> App
  App --> Transport
  App --> Domain
  App --> Persistence
  App --> Evidence
  App --> Analysis
  Persistence --> Infra
  Evidence --> Infra
  Analysis --> Infra
```

## 2. Rust crate 依赖图

```mermaid
flowchart LR
  Desktop["forensics-desktop"]
  App["app-services"]
  Transport["transport"]
  Domain["domain"]
  Persistence["persistence-sqlite"]
  Evidence["evidence-core"]
  NTFS["fs-ntfs"]
  FAT["fs-fat"]
  EXFAT["fs-exfat"]
  E01["image-e01"]
  Search["search"]
  Timeline["timeline"]
  Artifacts["artifacts-windows"]
  Reports["reports"]
  Mcp["mcp-client"]
  Infra["infrastructure"]

  Desktop --> App
  Desktop --> Transport
  App --> Transport
  App --> Domain
  App --> Persistence
  App --> Evidence
  App --> NTFS
  App --> FAT
  App --> EXFAT
  App --> E01
  App --> Search
  App --> Timeline
  App --> Artifacts
  App --> Reports
  App --> Mcp
  Persistence --> Domain
  Persistence --> Infra
  Evidence --> Infra
```

## 3. 核心领域 / 数据库模型 ER 图

```mermaid
erDiagram
  CASES ||--o{ DATA_SOURCES : owns
  CASES ||--o{ JOBS : tracks
  CASES ||--o{ REPORTS : exports
  CASES ||--o{ AUDIT_LOG : records
  DATA_SOURCES ||--|| SOURCE_DATABASES : routes_to
  SOURCE_DATABASES ||--o{ PARTITIONS : describes
  SOURCE_DATABASES ||--o{ FILE_ENTRIES : contains
  FILE_ENTRIES ||--o{ FILE_ENTRIES : parent_of
  FILE_ENTRIES ||--o{ ARTIFACTS : source_object
  FILE_ENTRIES ||--o{ TIMELINE_EVENTS : source_object

  DATA_SOURCES {
    text id PK
    text case_id FK
    text platform
    text import_state
    text source_db_rel_path
  }

  SOURCE_DATABASES {
    text data_source_id PK
    text relative_path
    int schema_version
  }

  FILE_ENTRIES {
    text id PK
    text parent_id FK
    text data_source_id FK
    text path
    text name
    text entry_type
    int deleted
    int hidden
    int system
  }

  PARTITIONS {
    text id PK
    int partition_index
    text filesystem
    text kind_label
    text status
  }

  AUDIT_LOG {
    text id PK
    text case_id
    text action
    text resource_type
    text resource_id
    text details
  }
```

## 4. 前端状态与 API 调用链图

```mermaid
flowchart LR
  Page["Page / Component"]
  Control["showHidden / sort / selection"]
  Hook["Feature hooks"]
  ApiFn["API wrapper"]
  Client["ApiClient.request"]
  Invoke["invoke"]
  Runtime{"Tauri runtime available?"}
  Error["explicit runtime error"]
  Cache["TanStack Query"]

  Page --> Control --> Hook --> ApiFn --> Client --> Runtime
  Runtime -->|yes| Invoke
  Runtime -->|no| Error
  Invoke --> Cache
  Cache --> Page
```

生产 UI 没有 mock fallback。显式的 `create_analysis_demo_case` 仅用于开发/审计时复制公开 fixture；它是受命令调用的独立数据源创建动作，不参与普通 API 请求失败后的路由。

## 5. Tauri command request / response 序列图

```mermaid
sequenceDiagram
  participant UI as FileBrowser
  participant Hook as hooks
  participant API as files API
  participant Cmd as file_commands
  participant Svc as file_service

  UI->>Hook: 切换 showHidden / sort
  Hook->>API: getFileRowsRequest(...)
  API->>Cmd: invoke(request)
  Cmd->>Svc: validate -> query
  Svc->>Svc: filter -> sort -> paginate
  Svc-->>Cmd: FileRowsPageDto
  Cmd-->>API: DTO
  API-->>Hook: typed result
  Hook-->>UI: render
```

## 6. Backend event push 序列图

```mermaid
sequenceDiagram
  participant Task as Backend Task
  participant Emit as Tauri emit
  participant Bus as EventBus
  participant Cache as Query Cache
  participant UI as React UI

  Task->>Emit: emit(topic, payload)
  Emit->>Bus: frontend listener
  Bus->>Cache: invalidate / patch
  Cache->>UI: rerender
```

## 7. 数据源导入 / 镜像探测 / 分区识别流程图

```mermaid
flowchart TB
  Start["Import request"]
  Validate["校验路径与输入"]
  Probe["detect_image_filesystem"]
  Control["app.db: 注册 data source / job"]
  SourceDb["sources/<dataSourceId>/source.db"]
  Persist["持久化 partition / file tree / artifacts"]
  Placeholder["创建 partition placeholder root"]
  Enumerate["串行或并行枚举"]
  Merge["staging merge / root fold"]
  Finish["导入完成"]

  Start --> Validate --> Probe --> Control --> SourceDb --> Placeholder --> Enumerate --> Merge --> Persist --> Finish
```

## 8. 文件系统枚举与目录树懒加载流程图

```mermaid
flowchart TB
  Tree["get_file_tree_request"]
  Children["get_file_children_request"]
  Repo["repo query"]
  Visible["showHidden 过滤"]
  Root["分区根归一化"]
  Sort["目录自然排序"]
  Page["分页"]
  DTO["FileTreeNodeDto / FileChildrenDto"]

  Tree --> Repo
  Children --> Repo
  Repo --> Visible --> Root --> Sort --> Page --> DTO
```

## 9. 搜索索引与查询流程图

```mermaid
flowchart TB
  File["source.db / file_entries"]
  Page["主键游标分批读取元数据"]
  Normalize["NFKC + 1/2/3-gram + 排序 fast fields"]
  FileIndex["sources/{id}/index<br/>完整文件名与路径索引"]
  Content["有界正文提取"]
  ContentIndex["sources/{id}/index-content<br/>旧正文检索与步骤回放"]
  Query["文件名 / 路径 / 扩展名 / 类型 / 数据源"]
  Snapshot["每数据源固定 index generation + opstamp"]
  Cursor["sort value + file id 稳定游标"]
  Merge["跨数据源有序归并"]
  Result["SearchFileResultPageDto + coverage"]

  File --> Page --> Normalize --> FileIndex
  File --> Content --> ContentIndex
  Query --> Snapshot
  FileIndex --> Snapshot --> Cursor --> Merge --> Result
```

主搜索页面采用 Everything 式元数据查询，不读取证据文件正文；目录、空文件、
超大文件和加密文件均可进入文件索引。正文索引保留为独立兼容能力，不能覆盖或
污染文件元数据索引。索引生成采用临时目录构建与原子发布，查询游标绑定数据源
集合、查询参数、schema version、generation 和 opstamp；任一条件变化时拒绝旧
游标并要求重新查询。

## 10. 时间线归一化与查询流程图

```mermaid
flowchart LR
  Source["artifacts / metadata / jobs"]
  Map["normalize event"]
  Persist["timeline_events"]
  Query["timeline query"]
  DTO["TimelineEventDto"]

  Source --> Map --> Persist --> Query --> DTO
```

## 11. Windows artifact 解析流程图

```mermaid
flowchart TB
  Candidate["candidate file / hive"]
  Select["select parser"]
  Read["bounded read"]
  Parse["parse"]
  Artifact["artifact row"]
  Timeline["timeline event"]
  Warning["warning / partial"]
  Fatal["fatal error"]

  Candidate --> Select --> Read --> Parse
  Parse --> Artifact --> Timeline
  Read --> Warning
  Parse --> Fatal
```

## 12. 报告导出流程图

```mermaid
flowchart TB
  Request["export request"]
  Validate["validate scope / active case"]
  Prepare["prepare_report_output"]
  Exists{"target exists?"}
  Overwrite{"overwrite=true?"}
  Reject["return conflict"]
  Temp["write temp file"]
  Rename["rename to final path"]
  Persist["persist report row"]
  Done["return file name"]

  Request --> Validate --> Prepare --> Exists
  Exists -->|no| Temp
  Exists -->|yes| Overwrite
  Overwrite -->|no| Reject
  Overwrite -->|yes| Temp
  Temp --> Rename --> Persist --> Done
```

## 13. MCP 权限判定与审计图

```mermaid
sequenceDiagram
  participant UI as MCP UI
  participant API as mcp API
  participant Cmd as mcp_commands
  participant Policy as permission policy
  participant Client as mcp-client
  participant Audit as audit_log
  participant External as MCP server

  UI->>API: connect / test / list / call
  API->>Cmd: invoke
  Cmd->>Policy: validate config / permission
  Policy-->>Cmd: allow or reject
  alt allow
    Cmd->>Client: connect / list / call
    Client->>External: SSE or stdio
    External-->>Client: result
    Cmd->>Audit: write action summary
    Cmd-->>API: response
  else reject
    Cmd->>Audit: write blocked action
    Cmd-->>API: security error
  end
```

## 14. 可验证性体系与错误分类图

```mermaid
flowchart TB
  Fixture["small / medium / real fixture"]
  Expected["expected JSON / baseline"]
  Test["parser / service / UI tests"]
  Result["pass / fail / partial"]
  Taxonomy["error taxonomy"]
  Docs["support matrix / trust framework / unsupported list"]

  Fixture --> Test
  Expected --> Test
  Test --> Result
  Result --> Taxonomy
  Result --> Docs
```

## 15. V2 关联分析与发布治理图

```mermaid
flowchart LR
  Sources["NTFS / Prefetch / LNK / Registry / Recycle Bin / Browser / Email"]
  Normalize["normalize + provenance"]
  Proximity["BrowserHistory / BrowserDownload / Email 邻近时间线信号"]
  Correlate["correlation rules"]
  Leads["lead / cluster / confidence"]
  Views["timeline / artifacts / files / reports"]
  Release["scorecard / release gate"]

  Sources --> Normalize --> Correlate --> Leads --> Views
  Normalize --> Proximity --> Correlate
  Leads --> Release
```

当前首版真实链路已经实现：

- `Artifact.sourceObjectId -> File`
- `Timeline.sourceObjectId -> File`
- `Artifact <-> Timeline` shared `sourceObjectId`
- `LNK.target_path -> File.path`
- `BrowserDownload.targetPath -> File.path`
- `BrowserHistory.url/title + visitTime -> timeline proximity`
- `RegistryValue.data -> File.path / File.name`
- `RecycleBin.original_path -> deleted File.path`
- `Prefetch.executable -> File.name`
- `EmailMessage.attachments[] / subject / sentAt -> File.name / timeline proximity`
- `JumpList.target_path -> File.path`
- 命中目标若已有 timeline event，会补挂 `TemporalContext`
- 产品内展示入口为 `V2Workbench -> CorrelationWorkspace`
- 报告导出已复用同一条关联快照：
  - HTML `Correlation Leads`
  - HTML `Correlation Lead Details`
  - JSON `correlation`

## 16. 维护说明

- 当前文档保持 `15` 个 Mermaid 图块，以配合防漂移脚本
- 图谱描述当前真实实现与确定的安全边界
- 如分区根模型、排序契约、MCP 权限、导出 overwrite 或验证体系变化，必须同步更新
