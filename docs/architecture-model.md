# Forensics Workbench 架构模型

**版本**: v1.1
**日期**: 2026-06-01

---

## 📊 项目概览

```
┌─────────────────────────────────────────────────────────────────┐
│                    Forensics Workbench                          │
│                    数字取证桌面应用                               │
├─────────────────────────────────────────────────────────────────┤
│  技术栈: Tauri 2 + Rust + React + TypeScript + SQLite           │
│  架构: DDD (领域驱动设计) + 分层架构                             │
│  代码量: ~25,000 Rust + ~5,000 TypeScript（近似）                │
│  测试: 以 `cargo test --workspace` 与前端 Vitest 为准             │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🏗️ 分层架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        前端层 (Frontend)                        │
│  React + TypeScript + Zustand + React Query                     │
├─────────────────────────────────────────────────────────────────┤
│                        命令层 (Commands)                        │
│  Tauri Commands - IPC 桥接                                      │
├─────────────────────────────────────────────────────────────────┤
│                      应用服务层 (App Services)                   │
│  业务逻辑编排、用例实现                                          │
├─────────────────────────────────────────────────────────────────┤
│                        领域层 (Domain)                          │
│  实体、值对象、领域服务                                          │
├─────────────────────────────────────────────────────────────────┤
│                      基础设施层 (Infrastructure)                 │
│  数据库、文件系统、外部服务                                       │
└─────────────────────────────────────────────────────────────────┘
```

---

## 📦 Crate 依赖图

```
                          ┌─────────────┐
                          │   domain    │
                          │  (核心领域)  │
                          └──────┬──────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
              ▼                  ▼                  ▼
     ┌────────────────┐ ┌───────────────┐ ┌────────────────┐
     │ infrastructure │ │    transport   │ │  artifacts-core │
     │   (基础设施)    │ │   (传输层)     │ │   (工件核心)    │
     └───────┬────────┘ └───────┬───────┘ └───────┬────────┘
             │                  │                  │
             ▼                  ▼                  ▼
     ┌────────────────┐ ┌───────────────┐ ┌────────────────┐
     │persistence-sqlite│ │  app-services │ │artifacts-windows│
     │   (数据库)      │ │  (应用服务)    │ │  (Windows工件)  │
     └────────────────┘ └───────┬───────┘ └────────────────┘
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                 │
              ▼                 ▼                 ▼
     ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
     │   evidence   │  │    search    │  │   timeline   │
     │   (证据核心)  │  │   (搜索)     │  │   (时间线)    │
     └──────┬───────┘  └──────────────┘  └──────────────┘
            │
    ┌───────┼───────┬───────────┐
    │       │       │           │
    ▼       ▼       ▼           ▼
┌───────┐┌───────┐┌───────┐┌───────┐
│fs-ntfs││fs-fat ││fs-exfat││image- │
│       ││       ││       ││raw/e01│
└───────┘└───────┘└───────┘└───────┘
```

---

## 🗂️ 模块职责

### 核心层

| Crate | 职责 | 主要类型 |
|-------|------|----------|
| **domain** | 领域模型、业务规则 | CaseMeta, FileEntry, Artifact, TimelineEvent |
| **transport** | DTO 定义、IPC 数据 | FileTreeNodeDto, McpConfigDto |

### 应用层

| Crate | 职责 | 主要模块 |
|-------|------|----------|
| **app-services** | 业务逻辑编排 | case_service, file_service, import_state |
| **infrastructure** | 基础设施 | hashing, config, constants |

### 证据处理层

| Crate | 职责 | 支持格式 |
|-------|------|----------|
| **evidence-core** | 证据读取核心 | Reader, FileSystem 接口 |
| **fs-ntfs** | NTFS 文件系统 | MFT 扫描、目录遍历 |
| **fs-fat** | FAT 文件系统 | FAT12/16/32 |
| **fs-exfat** | exFAT 文件系统 | 引导扇区、目录、FAT |
| **image-raw** | 原始镜像 | dd 格式 |
| **image-e01** | E01 镜像 | EnCase 格式 |

### 工件分析层

| Crate | 职责 | 支持工件 |
|-------|------|----------|
| **artifacts-core** | 工件提取框架 | Extractor, Sink 接口 |
| **artifacts-windows** | Windows 工件 | LNK, Prefetch, Registry, RecycleBin |

### 业务功能层

| Crate | 职责 | 功能 |
|-------|------|------|
| **search** | 全文搜索 | Tantivy 索引 |
| **timeline** | 时间线分析 | MACB 投影 |
| **catalog** | 目录管理 | 索引、投影 |
| **reports** | 报告生成 | HTML, CSV, JSON |

### 基础设施层

| Crate | 职责 | 功能 |
|-------|------|------|
| **persistence-sqlite** | 数据库 | SQLite、迁移、仓库 |
| **runtime-cache** | 运行时缓存 | 文件句柄、搜索缓存 |
| **mcp-client** | MCP 客户端 | AI 助手集成 |

---

## 📊 数据库模型

```
┌─────────────────────────────────────────────────────────────────┐
│                        SQLite Database                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────┐          │
│  │  cases   │───▶│ data_sources │───▶│ file_entries  │          │
│  └──────────┘    └──────────────┘    └───────────────┘          │
│       │                │                     │                   │
│       ▼                ▼                     ▼                   │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────┐          │
│  │   jobs   │    │  partitions  │    │   artifacts   │          │
│  └──────────┘    └──────────────┘    └───────────────┘          │
│       │                                      │                   │
│       ▼                                      ▼                   │
│  ┌──────────┐                        ┌───────────────┐          │
│  │ reports  │                        │timeline_events│          │
│  └──────────┘                        └───────────────┘          │
│                                                                  │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────┐          │
│  │   tags   │    │  audit_log   │    │ schema_migrations │       │
│  └──────────┘    └──────────────┘    └───────────────┘          │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 核心表

| 表 | 记录数 (预估) | 核心字段 |
|----|--------------|----------|
| cases | 1-10 | id, name, number, examiner |
| data_sources | 1-50 | id, case_id, kind, source_path |
| file_entries | 10K-1M | id, path, name, size, MACB, hash |
| artifacts | 100-10K | id, family, title, attrs(JSON) |
| timeline_events | 10K-1M | id, event_type, ts |
| partitions | 1-100 | id, data_source_id, offset, length |

---

## 🔄 数据流

### 导入流程

```
用户选择文件
    │
    ▼
┌────────────────┐
│ 分类数据源类型  │
│ E01/RAW/逻辑   │
└───────┬────────┘
        │
        ▼
┌────────────────┐     ┌───────────────┐
│ 创建后台任务   │────▶│ JobRepo       │
└───────┬────────┘     └───────────────┘
        │
        ▼
┌────────────────┐     ┌───────────────┐
│ 枚举文件系统   │────▶│ FileRepo      │
│ (BFS 遍历)     │     │ (批量插入)    │
└───────┬────────┘     └───────────────┘
        │
        ▼
┌────────────────┐     ┌───────────────┐
│ 时间线投影     │────▶│ TimelineRepo  │
└───────┬────────┘     └───────────────┘
        │
        ▼
┌────────────────┐     ┌───────────────┐
│ 工件提取       │────▶│ ArtifactRepo  │
└───────┬────────┘     └───────────────┘
        │
        ▼
┌────────────────┐     ┌───────────────┐
│ 文本索引       │────▶│ SearchIndex   │
└────────────────┘     └───────────────┘
```

### 文件预览流程

```
用户选择文件
    │
    ▼
┌────────────────┐
│ 获取文件句柄   │
│ openFileHandle │
└───────┬────────┘
        │
        ▼
┌────────────────┐
│ 检测文件类型   │
│ MIME / 扩展名  │
└───────┬────────┘
        │
   ┌────┼────┬────────┐
   │    │    │        │
   ▼    ▼    ▼        ▼
┌─────┐┌─────┐┌─────┐┌─────┐
│ Hex ││Text ││Image││Video│
│Viewer││Viewer││Viewer││Viewer│
└─────┘└─────┘└─────┘└─────┘
```

当前约束:

- 预览读取必须通过 `FileEntryId` 和数据源类型分派到统一 reader helper，不允许用 `case_root.join(entry.path)` 拼接宿主路径读取证据内容。
- `get_text_preview` 和 hex range 读取走真实 reader；`get_image_preview` 返回有大小上限的 data URL。
- `get_media_url` 对小媒体返回 bounded data URL；对大媒体返回 `mode=protocol`、opaque `handleId`、MIME、size 和 `evidence-media://handle/<encoded>` URL。Tauri `evidence-media` protocol handler 使用同一 evidence reader helper 按 Range 读取，每次最多 1MB，返回 206/416 等 bounded response；`read_media_range` command 继续作为 mock/unsupported fallback。当前实现不暴露 evidence 宿主绝对路径，CSP 只在 `media-src` 中允许 `evidence-media:`。

### 测试 Fixture 策略

- `testdata/fixtures/tiny/logical/` 是默认 CI 可用的逻辑目录 fixture。
- `testdata/fixtures/tiny/raw/tiny.raw` 是 1024-byte deterministic RAW fixture，含 MBR signature。
- `testdata/fixtures/tiny/e01/tiny.E01` 是 4405-byte synthetic single-segment E01 fixture，用于 `image-e01` reader 的 section/table/read/seek 回归。它不是完整文件系统镜像，也不能替代真实 E01 分区/文件系统慢测。
- `testdata/fixtures/tiny/evtx/system.evtx` 是 1,118,208-byte real System.evtx fixture，用于 `evtx.boot_shutdown` parser path 回归；fixture provenance 写在同目录 `README.md`。
- `scripts/generate-tiny-fixtures.ps1` 可重建 RAW/E01 tiny fixtures。真实 E01 验收继续通过 `FORENSICS_E01_FIXTURE` opt-in ignored slow tests 执行，默认 CI 不依赖私有样本。

---

## Analysis API Contract

Analysis 功能的前后端契约以 `transport` crate 为唯一 Rust 源头，前端类型在 `frontend/src/types/models.ts` 手动镜像。

| 层级 | 文件 / 类型 | 约定 |
|------|-------------|------|
| Transport DTO | `crates/transport/src/dto/analysis.rs` | `AnalysisSystemInfoDto`, `AnalysisFieldProvenanceDto`, `AnalysisNetworkAdapterDto`, `AnalysisBootRecordDto`, `AnalysisClassifiedFileDto`, `AnalysisFileClassificationDto`，全部 `#[serde(rename_all = "camelCase")]` |
| Transport request | `crates/transport/src/commands/mod.rs` | `ClassifyFilesRequest { sample_size: Option<u32> }`，JSON 字段为 `sampleSize` |
| Tauri commands | `apps/desktop/src-tauri/src/commands/analysis_commands.rs` | `get_system_info`, `classify_files`, `generate_analysis_summary`；`sampleSize` 默认 1000，最大 5000，0 或超过最大值返回 invalid input |
| App service | `crates/app-services/src/analysis_service.rs` | 负责取证状态表达、bounded header magic 分类、summary 文本生成；不得定义可泄露到 IPC 的响应 DTO |
| Frontend API | `frontend/src/lib/api/analysis.ts` | 所有调用走 `apiClient.request(...)`，禁止页面直接 import Tauri `invoke` |
| Frontend hooks | `frontend/src/features/analysis/hooks.ts` | React Query hooks 使用 active case enabled gate；无 active case 时不发 analysis IPC/mock 请求 |
| Mock provider | `frontend/src/lib/api/provider.ts`, `frontend/src/lib/api/mock-data.ts` | mock 模式提供三类 analysis fallback，默认 `pnpm dev` 可访问 `/analysis` |
| Reports | `crates/app-services/src/report_service.rs`, `crates/reports/src/html/exporter.rs` | HTML/CSV/JSON 输出当前 Analysis summary、parser status、warnings 与 evidence provenance；HTML escaping 和 CSV formula sanitization 必须保留 |

当前 parser 状态:

- Analysis DTO 包含 `AnalysisProvenanceDto { dataSourceId, artifactPath, parser, parsedAt, status, warnings }`。系统信息、boot records、分类汇总和单文件分类均可携带来源说明；系统字段另有 `fieldProvenance` 追踪 hive path、key path、value name 和 parser。
- Registry 当前是定向字段 parser，不是完整 hive browser。`artifacts-windows::registry::lookup` bounded 读取最多 64MB，解析 `regf` base block、NK/VK、`lf/lh/li/ri` 子键列表和常用值类型；Analysis 从 `SYSTEM` / `SOFTWARE` 提取 `ComputerName`、timezone、ProductName、CurrentBuild、InstallDate、RegisteredOwner、Organization、ProductId。缺失、损坏或超限只产生 warnings，不补默认 Windows 文案。
- EVTX 当前是 boot/shutdown candidate adapter。`artifacts-windows::evtx` 使用 `evtx` crate bounded 读取最多 64MB `System.evtx`，解析 6005、6006、6008、1074 为候选事件，并明确标注这些是 EventLog/User32 proxy，不是绝对开关机事实。仓库内已有 tiny real `System.evtx` fixture 覆盖 parser path；现有测试同时覆盖 JSON 记录提取、malformed、oversized 和 truncated magic 路径。`evtx -> encoding` 仍是到期 dependency exception，不代表依赖治理已永久完成。
- 文件分类只读取每个文件有限 header（当前 8KB）并按 magic/ext 分类；读取入口为 `FileEntryId + DataSourceKind`，支持 logical directory、E01、RAW 的统一路径。
- Summary 只基于真实 DTO 状态生成；未解析信息必须显示“未解析”或 warning，不允许生成伪造取证事实。

---

## 🧩 组件架构 (前端)

```
┌─────────────────────────────────────────────────────────────────┐
│                          App.tsx                                │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                      Layout.tsx                           │  │
│  │  ┌─────────┐ ┌─────────────────────────┐ ┌────────────┐  │  │
│  │  │ TopBar  │ │       PageContent        │ │ Inspector  │  │  │
│  │  └─────────┘ └─────────────────────────┘ └────────────┘  │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │                   BottomDrawer                       │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘

页面组件:
├── CaseHome          # 案件首页
├── DataAnalysis      # 数据源分析
├── FileBrowser       # 文件浏览
├── Search            # 搜索
├── Timeline          # 时间线
├── Artifacts         # 工件
├── Reports           # 报告
└── Settings          # 设置

状态管理 (Zustand):
├── useAppStore       # 全局状态
├── useUiStore        # UI 状态
├── useSelectionStore # 选择状态
└── useMcpStore       # MCP 状态
```

Settings 当前分两类持久化：

- 路径类设置通过 `get_app_settings` / `save_app_settings` Tauri command 进入后端校验并写入配置文件。
- theme 与 dev event trace 同步写入 `localStorage`，用于 mock/dev 环境即时生效；Tauri 模式保存时同样会进入后端配置。

事件契约当前由 `transport::events::EventTopic` 枚举收口，前端 `EventTopic` union 手动同步。后端 event bridge 使用 `emit_to("main", ...)` 定向发送 case/job/artifact/timeline/search/partition 事件，payload 不应包含 evidence 宿主绝对路径。

---

## 🔌 MCP 集成架构

```
┌─────────────────────────────────────────────────────────────────┐
│                       MCP 集成架构                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐      │
│  │   Frontend   │    │   Tauri IPC  │    │   Backend    │      │
│  │   (React)    │◀──▶│   Commands   │◀──▶│   (Rust)     │      │
│  └──────────────┘    └──────────────┘    └──────────────┘      │
│         │                                       │               │
│         ▼                                       ▼               │
│  ┌──────────────┐                        ┌──────────────┐      │
│  │  useMcpStore │                        │  mcp-client  │      │
│  │  (Zustand)   │                        │  (Rust SDK)  │      │
│  └──────────────┘                        └──────┬───────┘      │
│                                                  │               │
│                              ┌───────────────────┼────────────┐ │
│                              │                   │            │ │
│                              ▼                   ▼            ▼ │
│                       ┌───────────┐      ┌───────────┐  ┌──────┐│
│                       │ SSE/HTTP  │      │   Stdio   │  │ WS   ││
│                       └─────┬─────┘      └─────┬─────┘  └──┬───┘│
│                             │                  │           │    │
│                             ▼                  ▼           ▼    │
│                       ┌─────────────────────────────────────┐  │
│                       │          MCP Servers                │  │
│                       │  Claude / Custom / Ollama            │  │
│                       └─────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 📈 性能指标

以下为静态复杂度或局部验证结果，不代表所有证据格式/机器配置下的性能承诺。

| 操作 | 复杂度 | 当前说明 |
|------|--------|----------|
| 文件枚举 | O(n) | BFS/批量插入；真实速度依赖镜像格式与 SQLite 写入 |
| 文件排序 | O(n log n) | 前端预计算排序键 |
| SHA-256 哈希 | O(n) | 流式 reader |
| Hex 格式化 | O(n) | 对已读取 range 格式化 |
| Magic 分类 | O(s·h) | s = 样本数，h = bounded header；当前 header 上限 8KB |
| 路径重建 | O(n) | 递归 + 缓存 + cycle detection |
| MFT 扫描 | O(n) | 多线程可降低 wall time，但 I/O 和 DB writer 仍可能成为瓶颈 |
| 搜索查询 | 依赖 Tantivy | 需以实际索引规模 benchmark 验证 |

---

## 🔒 安全架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        安全层次                                 │
├─────────────────────────────────────────────────────────────────┤
│  1. 输入验证                                                     │
│     - 路径遍历防护                                               │
│     - URL 编码检测                                               │
│     - Null 字节检测                                              │
│     - Windows 保留名检测                                         │
│                                                                  │
│  2. 数据库安全                                                   │
│     - 参数化查询 (防 SQL 注入)                                   │
│     - 外键约束                                                   │
│     - 审计日志                                                   │
│                                                                  │
│  3. 文件系统安全                                                 │
│     - Symlink 检测                                               │
│     - 路径规范化                                                 │
│     - 权限检查                                                   │
│                                                                  │
│  4. 内存安全                                                     │
│     - 无 unsafe 代码                                             │
│     - 边界检查                                                   │
│     - 大小限制                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🧪 测试架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        测试覆盖                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  后端默认门禁                                                   │
│  ├── cargo fmt --all -- --check                                 │
│  ├── cargo clippy --workspace --all-targets -- -D warnings       │
│  └── cargo test --workspace                                     │
│                                                                  │
│  前端默认门禁                                                   │
│  ├── pnpm --dir frontend typecheck                              │
│  ├── pnpm --dir frontend lint                                   │
│  ├── pnpm --dir frontend test                                   │
│  └── pnpm --dir frontend build                                  │
│                                                                  │
│  慢测/真实样本验收                                               │
│  ├── e01_full_pipeline_test --ignored --nocapture                │
│  └── e01_mft_scan_test --ignored --nocapture                    │
│                                                                  │
│  依赖治理 / 供应链                                               │
│  ├── cargo deny check advisories bans licenses sources           │
│  ├── cargo audit                                                 │
│  ├── pnpm --dir frontend audit --audit-level high                │
│  └── CycloneDX backend/frontend SBOM artifacts                   │
│                                                                  │
│  覆盖率报告                                                       │
│  ├── scripts/run-coverage.ps1                                    │
│  ├── cargo llvm-cov backend LCOV artifact                         │
│  └── pnpm --dir frontend test:coverage + baseline threshold       │
│                                                                  │
│  覆盖重点                                                       │
│  ├── 编译检查                                                   │
│  ├── 单元测试                                                   │
│  ├── Clippy                                                     │
│  ├── 格式检查                                                   │
│  ├── 安全审计                                                   │
│  ├── MCP 测试                                                   │
│  ├── 数据库测试                                                 │
│  ├── 前端构建                                                   │
│  └── TypeScript 检查                                            │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 📊 代码统计

| 类别 | 文件数 | 代码行数 |
|------|--------|----------|
| Rust 源码 | 164 | ~25,000 |
| TypeScript | 99 | ~5,000 |
| SQL 迁移 | 17 | ~500 |
| 文档 | 15 | ~15,000 |
| CI 配置 | 2 | ~320 |
| **总计** | **297** | **~46,000** |

---

**建模人**: MiMo AI Assistant；2026-06-02 由 Codex 更新 Analysis provenance、Registry targeted parser、EVTX candidate adapter、EVTX real fixture regression、evidence-media protocol、Reports、Job partial、CI/SBOM、coverage artifact/baseline gate、FS path helper 与慢测状态
**建模版本**: v1.3
