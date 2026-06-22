# Forensics Workbench 全量工程审计报告

**审计日期**：2026-06-21  
**审计范围**：`D:/process/forensic` 全仓库（Rust 后端 38 workspace members、前端 React/TypeScript、CI/guard scripts、文档）  
**审计方法**：只读探索 + 多维度并行子代理 + 关键门禁复现  
**报告状态**：初稿，未修改源码

---

## 1. 执行摘要

### 1.1 总体结论

Forensics Workbench 是一款架构清晰、功能完整的 Windows 优先数字取证桌面应用。项目采用 Tauri 2 + Rust 37+ crate 工作空间 + React/TS/Vite/Tailwind 4 技术栈，证据读取、文件系统解析、Windows 制品提取、搜索索引、时间线、实体消解、STIX 交换等核心能力已基本成型。

**当前状态（与文档自评对照）**：
- V2：~90% 完成，Grade B（81/100），7 个真实 E01 回归测试通过 ✅
- V3：~89% 完成，22/22 phase 实现，但 Linux/macOS/PST 解析器**未真正接入取证流水线** ⚠️
- V4：核心已交付（5 个新文件系统 crate + exchange 实体/STIX/签章），V4-3 AI 阶段已推迟
- V5：已启动规划，方向为高级文件系统恢复、移动/云盘取证、GQL、生产部署

**最大风险**：代码规范执行不均衡。AGENTS.md 对错误处理、死代码、文件大小、workspace 依赖有明确要求，但在 V4 新 crate、Tauri shell、app-services 中存在大量违反，导致质量门禁当前无法全部通过。

### 1.2 关键数字（已验证）

| 指标 | 实际值 | 文档声明 | 状态 |
|---|---|---|---|
| Workspace members | 38 | AGENTS: 37；docs: 31 | ❌ 口径漂移 |
| Tauri commands | 84 | AGENTS: 84 | ✅ |
| SQLite repos | 12 | AGENTS: 12 | ✅ |
| Migration scripts | 31 | AGENTS: 31 | ✅ |
| Frontend pages | 10 | AGENTS: 10 | ✅ |
| Frontend test files | 43 | AGENTS: 43 | ✅ |
| Event topics | 18 | AGENTS: 18 | ✅ |
| DTO domain files | 25 | AGENTS: 25 | ✅ |
| `cargo fmt --check` | 失败（3 文件） | 必须通过 | ❌ |
| `pnpm lint` | 失败（2 errors, 14 warnings） | 必须通过 | ❌ |
| 生产代码 `#[allow(dead_code)]` | 68 处 | 禁止 | ❌ |
| `Result<T, String>`（app-services + commands） | 243 处 | 新代码禁止 | ❌ |
| 生产源文件 > 1500 行 | 至少 5 个 | ≤1500 | ❌ |
| 前端组件 > 500 行 | 至少 8 个 | ≤500 | ❌ |

---

## 2. 架构审计

### 2.1 分层架构（健康度：B+）

```text
React UI (frontend/)
    ↓ Tauri invoke / events
Tauri Command Layer (apps/desktop/src-tauri/commands/)
    ↓ DTO / CommandError
Application Services (crates/app-services/)
    ↓ domain / transport
Persistence (crates/persistence-sqlite/)
Evidence / FS / Artifacts / Search / Timeline / Reports / Exchange
```

**优点**：
- crate 边界清晰，核心解析器不依赖 Tauri/前端
- `crates/transport` 作为前后端唯一 IPC 契约，DTO 集中管理
- `crates/domain` 与 `crates/transport` 分离，domain 无序列化耦合
- 事件流通过 `EventTopic` 常量双向镜像

**问题**：
1. **命令层越界操作 persistence**：大量命令直接 `persistence_sqlite::open_or_create(&db_path)` 并实例化 Repo，违反“thin wrapper”目标。`check-command-sql-boundary.ps1` 因关键词列表未覆盖 `open_or_create` 而漏报。
2. **服务层错误类型化不足**：服务层大量 `.map_err(|e| e.to_string())`，命令层再通过字符串关键词分类（`CommandError::from_service_error`），丢失结构化信息。
3. **AppState 同步/异步锁混用**：`active_case/db_pool/mcp_config/runtime_cache` 用 `std::sync::Mutex`，`mcp_clients` 用 `tokio::sync::RwLock`；`get_mcp_server_status()` 使用 `block_in_place` 在同步上下文等待异步锁，存在线程池风险。

### 2.2 依赖治理（健康度：B）

**优点**：
- `deny.toml` 定义清晰，advisory/license/ban 例外均带 `owner/reason/expires`
- 16 个 advisory 例外统一 2026-09-01 到期，强制周期性审查
- `cargo deny check` 为默认门禁

**问题**：
- 大量 member `Cargo.toml` 仍写直接版本号，未集中到 root `[workspace.dependencies]`
- `apps/desktop/src-tauri/Cargo.toml` 对部分 crate 使用 `{ path = "..." }` 而非 workspace alias
- `reqwest 0.12` 与 `tauri-plugin-updater` 引入的 `reqwest 0.13` 并存，`deny.toml` 当前为 warn
- `evtx-patched` 为 vendored fork，edition 2024，依赖集与 workspace 不完全一致

### 2.3 Crate 职责与拆分（健康度：B+）

| Crate 组 | 评价 |
|---|---|
| `domain` / `transport` | A：干净、无 unsafe、测试充分 |
| `evidence-core` / `image-*` | A-：抽象稳定，E01 reader 健壮，但 `RawImageReader` 在 `evidence-core` 与 `image-raw` 重复 |
| `fs-*` | B+：NTFS/FAT/exFAT 成熟；ext4/XFS/Btrfs/APFS/HFS+ 为 V4 新增，测试覆盖 synthetic fixture |
| `artifacts-windows` | A-：Registry/SAM/EVTX/Prefetch/LNK 等成熟，但 `Result<T, String>` 仍多 |
| `artifacts-linux` / `artifacts-macos` / `containers-pst` | C+：parser 本身完整，但**未实现 `ArtifactExtractor`，未接入流水线，不设置 `source_object_id`** |
| `app-services` | B：编排能力强，但文件过大、String error 多、unwrap 聚集 |
| `forensics-desktop` (Tauri shell) | B-：命令层模板一致，但越界操作 DB、文件过大、死代码残留 |
| `mcp-client` | A-：权限模型正确，但 SSE 实际为 HTTP-RPC，stdio stderr 丢弃 |
| `search` / `timeline` / `catalog` / `reports` | B+：功能完整，indexer unwrap 偏多 |
| `exchange` | B+：STIX 2.1、Ed25519、custody chain 完整，依赖 transport DTO |
| `runtime-cache` / `updater` / `crash_handler` | A-：小而聚焦，测试较全 |

---

## 3. 模型与算法审计

### 3.1 领域模型（健康度：A-）

**核心实体**：
- ID 包装类型：`CaseId`、`FileEntryId`、`ArtifactId`、`TimelineEventId` 等，类型安全
- `FileEntry`：含 `deleted`/`hidden`/`system` 状态，前后端一致
- `Artifact` / `TimelineEvent`：共享 `source_object_id` 作为关联桥
- `GraphNode` / `GraphEdge`：支持实体消解与关联图

**时间戳策略**：
- Domain 层使用 `chrono::DateTime<Utc>`，DTO 层序列化为 RFC3339
- `domain::timestamp` 统一处理 FILETIME/NTFS/Unix/exFAT，年份限制 `[1970, 2100]`

**问题**：
1. **DTO ↔ Domain 映射无 trait 约束**：全部手写 `*_to_dto`，新增字段易遗漏
2. **Notebook 前后端模型不同步**：Rust 为 `body_markdown` + `EvidenceCitation`，前端为 `content` + `citationNodeIds`
3. **Batch 前后端模型不同步**：前端要求 `name/progress/etaMs/fileCount/artifactCount/logTail` 等字段，Rust 侧缺失
4. **`DataSourceSummaryDto.source_hash` 冗余 `#[serde(rename = "sourceHash")]`**：`rename_all = "camelCase"` 已足够

### 3.2 核心算法与实现

#### 3.2.1 证据读取（EvidenceReader）
- `RawImageReader` / `E01Reader` 实现 `Read + Seek + Send`
- E01 reader：多段检测、section chain 循环检测、chunk cache（16MB bounded）、顺序预取、seek 重置
- **风险**：全局 `E01_READER_CACHE` 为 `LazyLock<Mutex<E01ReaderCache>>`，仅按 `source_path` 索引，case 切换时清空；若路径冲突可能错配

#### 3.2.2 文件系统读取
- `FileSystemReader` trait：未要求 `Send/Sync`，内部使用 `RefCell<Box<dyn EvidenceReader>>`，限制 rayon 并行读取同一卷
- NTFS：MFT 记录解析、update sequence fixup、data runs（上限 100k）、LZNT1 解压、ADS、$LogFile
- MBR/GPT：MBR + EBR 链解析，256 次迭代安全限制；GPT 头 + 分区项解析

#### 3.2.3 制品解析
- Windows：ExtractorRegistry 路径匹配分发，7 个 extractor 已注册
- Linux/macOS/PST：仅提供独立 `parse_*` 函数，**未生成 `Artifact` / `TimelineEvent`，关联分析断层**

#### 3.2.4 搜索与索引
- Tantivy 全文索引，查询解析与高亮
- **风险**：`search/src/indexer/tantivy_writer.rs` 67 处 unwrap，需收敛

#### 3.2.5 实体消解与跨案匹配
- `EntityMergeEngine::deduplicate_entity_nodes` 先做单案消解，写入 `resolved_entities`
- `CrossCaseEntityMatcher::match_entities_across_cases` 要求至少 2 个数据库
- **风险**：cross_case.rs 当前为 fmt 失败文件之一

---

## 4. 代码质量逐模块审计

### 4.1 `crates/domain` 与 `crates/transport`（A-）

**优点**：
- 无 unsafe、无 `#[allow(dead_code)]`、无 TODO/FIXME
- DTO serde 契约规范（camelCase、skip_serializing_if）
- 测试密集：domain 48 个测试，transport 117 个测试

**问题**：
- `domain/Cargo.toml` 声明了 `anyhow` 但未使用
- 部分文件 CRLF/LF 混用
- `analysis.rs` 1279 行混合 V2 治理 + 浏览器 + 邮件 + Registry 视图，建议拆分

### 4.2 证据处理 crate（B+）

**优点**：
- 依赖方向干净，无 unsafe
- 统一使用 `thiserror` + `io::Error`，无 `Result<T, String>`
- synthetic fixture 覆盖全

**问题**：
- `fs-hfsplus/src/lib.rs` 1608 行，超出 1500 限制
- 多个 fs crate 使用 `#[allow(dead_code)]`：hfsplus（模块级！）、apfs、btrfs、xfs、ext4、ntfs
- `evidence-core` 与 `image-raw` 重复实现 `RawImageReader`
- `fs-exfat/Cargo.toml` 直接写 `byteorder = "1"`
- 多文件 CRLF 行尾

### 4.3 制品解析 crate（B-）

**Windows（A-）**：
- Registry 子系统最成熟：reader、types、sam/ntuser/system/software、txlog、recovery
- SAM RID 正确处理（VK `data_type` 字段）
- Prefetch 多版本支持，Windows Compression API 使用带 `// SAFETY:` RAII guard
- EVTX fail-closed（仅 System.evtx，6005/6006/6008/1074，16MB 上限）
- 仍有 `Result<T, String>` 和 6 处 FILETIME→UTC 重复实现

**Linux / macOS / PST（C+）**：
- parser 数据结构完整，单元/集成测试覆盖
- **致命缺口**：未实现 `ArtifactExtractor`，未注册到 `ExtractorRegistry`，不设置 `source_object_id`
- `artifacts-macos` 仍用 `Result<T, String>`
- `containers-pst/src/pst.rs` 1597 行，超出限制
- `artifacts-linux/src/journal.rs` 含大量注释掉的探索代码、死代码、未实现的压缩支持

### 4.4 `crates/app-services`（B）

**优点**：
- 已按域拆分为 `file_service/`、`correlation/`、`staging/`、`import_analysis/`、`entity_resolution/`、`report/`
- staging DB + merge 设计合理，解决 SQLite 单 writer 瓶颈
- 并行枚举、MFT 扫描、RSS 监控、取消令牌机制完整

**问题**：
- `file_service/mod.rs` 1724 行、`v2_governance_service.rs` 1733 行，超出限制
- 大量 `Result<T, String>`：`file_service/viewer.rs`、`tree_queries.rs`、`job_service.rs`、`artifact_service.rs` 等
- `staging/mod.rs` 165 处 unwrap、`parallel_enum/mod.rs` 74 处 unwrap、`import_analysis/mod.rs` 82 处 unwrap
- 全局 E01 reader cache 使用 `.expect("cache invariant")`

### 4.5 Tauri 命令层（B-）

**优点**：
- 命令模板一致：validate → 短锁取 db_path → spawn_blocking → 委托服务 → 返回 DTO
- 错误脱敏、媒体安全、审计日志、文件提取原子性（temp + rename）
- 取消机制协作式实现

**问题**：
- `commands/import/pipeline.rs` 2285 行，严重超出限制
- `file_commands.rs` 1035 行、`case_commands.rs` 866 行、`mcp_commands.rs` 723 行
- 命令层直接 `open_or_create` SQLite 连接，越过 app-services 边界
- `#[allow(dead_code)]` 残留：`partition_display.rs`、`pipeline.rs` 多处
- MCP 命令 DTO 转换冗长，未下沉到 transport DTO 的 `From`/`TryFrom`
- 部分命令同步（`create_case`/`open_case`），可能阻塞 async runtime

### 4.6 前端（B）

**优点**：
- 分层清晰：pages → hooks → `lib/api/<domain>.ts` → `client.ts` → Tauri
- `ApiClient` 是唯一调用 `invoke` 的位置
- mock 链路完整，`pnpm dev` 可独立运行
- 测试通过：228 个测试，覆盖率超过阈值

**问题**：
- **Lint 未通过**：2 errors（`BatchHistory.tsx`、`NotebookPanel.tsx`）+ 14 warnings
- 组件文件过大：`FileBrowser.tsx` 1141 行、`AnalysisPanels.tsx` 1201 行、`NotebookPanel.tsx` 1109 行等
- `FileBrowser` 未使用已实现的 `VirtualFileTree`，大目录可能退化
- `jobs/hooks.ts` 与 `import-event-state.ts` 使用全局可变状态，测试间可能污染
- `ErrorBoundary` 直接渲染 `error.message`，可能泄露敏感路径
- IPC payload 形状不一致：部分命令用 `{ request: {...} }`，部分扁平（如 `cancelImport`）
- `constants.ts` 中大量未使用常量

### 4.7 测试与 CI（B）

**优点**：
- Rust ~1,818 个 `#[test]`，239 个 `#[cfg(test)]` 模块，44 个 `#[ignore]` 真实样本测试
- 前端 43 个测试文件，覆盖率阈值（45%/35%）达标
- Guard scripts 职责单一，覆盖 SQL 边界、媒体协议、发布字符串、文档漂移、依赖例外等

**问题**：
- `cargo fmt --check` 当前失败（3 文件）
- 后端覆盖率无阈值，仅上传 LCOV artifact
- Benchmark 只实际测量 small 场景，却声明 medium/large 阈值
- `v2-runtime-results.json` 手工维护，易与 CI 真实结果漂移
- `ci.md` 规划的 `ci-desktop.yml`、`ci-docs.yml`、nightly、release workflows 未落地
- `scripts/run-liuyang-artifact-test.ps1` 硬编码 `D:\process\forensic`

---

## 5. 健壮性评估

### 5.1 并发/并行（B+）

**优点**：
- rayon 用于 CPU 密集型批量操作（artifact extraction、correlation、timeline projection、hashing）
- I/O  bound 工作（E01 reader）保持串行，避免 contention
- `ActiveCase` 用 `Mutex<Connection>` 串行化访问

**风险**：
- `FileSystemReader` 非 Sync，限制并行读取同一卷
- `block_in_place` 等待 MCP async 锁
- 全局 E01 cache 无过期策略，仅固定 4 个 reader

### 5.2 边界处理（B+）

**优点**：
- 分页 clamp（max 500）、viewer range clamp（max 1MiB）
- 路径校验：null byte、`\\.\`、`\\?\`、保留设备名
- 文件提取冲突检测、overwrite=false、temp + rename
- 时间戳年份限制

**风险**：
- `validate_export_destination_path` 未检查保留设备名（`NUL`/`CON` 等）
- media handle 在 TTL 内可无限次复用（无 single-use nonce）

### 5.3 资源管理（A-）

**优点**：
- Prefetch decompressor RAII guard
- `tempfile::TempDir` 在测试中自动清理
- `close_case` 会 cancel 任务、清空 cache、db_pool、E01 cache
- `delete_case` 对 `remove_dir_all` 做 5 次重试

**风险**：
- stdio MCP child stderr 被丢弃，诊断信息丢失
- SSE transport fallback 到默认 client 时静默丢弃超时配置

### 5.4 错误处理（B-）

**优点**：
- `ApiErrorDto` 含 category + recoverable
- Tauri 命令层不把原始错误抛给 UI

**风险**：
- 服务层大量 `Result<T, String>`，违反 AGENTS
- `CommandError::from_service_error` 依赖字符串关键词匹配，脆弱且易误判
- ErrorBoundary 直接暴露原始错误消息给前端

---

## 6. 安全与治理审计

### 6.1 证据完整性（A-）
- 原始证据只读，写入仅发生在 case workspace / SQLite / index / export path
- 文件提取默认 overwrite=false
- 路径校验优先于写入

### 6.2 路径与媒体安全（A-）
- 媒体预览走 `evidence-media://handle/<encoded>`，不暴露主机路径
- CSP 允许 `media-src 'self' data: evidence-media:`
- 句柄含 case_id + object_id，30 分钟 TTL

### 6.3 MCP 安全（A-）
- SSE URL scheme 白名单（http/https），禁止嵌入式凭证
- Stdio 命令只能是可执行名，不能是路径
- 默认最小权限：`resourceAccess=readOnly`、`toolAccess=disabled`、`promptAccess=readOnly`、`networkPolicy=localhostOnly`
- 关键操作写审计日志

### 6.4 依赖治理（B+）
- `deny.toml` 策略完整，例外带到期日
- 但 workspace 依赖集中化执行不到位

### 6.5 文档权威层级（B+）
- `AGENTS.md` > 中文工程文档 > 专题文档 > 旧英文文档
- `docs/documentation-index.md` 作为权威入口
- **当前 AGENTS.md 在工作区被修改未提交**，且计数口径与 cargo metadata 不一致

---

## 7. 最新开发方向（V4/V5）

### 7.1 V4 状态
- **已交付**：5 个新文件系统 crate（ext4/xfs/btrfs/apfs/hfsplus）、`exchange` crate（实体消解、STIX 2.1、Ed25519、custody chain、UCO mapping）
- **已推迟**：V4-3 AI 阶段
- **待收尾**：新 fs crate 代码规范清理、Linux/macOS/PST 接入流水线

### 7.2 V5 方向（来源：`docs/v5-plan.md`）
1. **V5-1 高级文件系统取证**： carving、日志/journal 深度恢复、APFS Time Machine、Btrfs scrub、HFS+ 已删除文件恢复
2. **V5-2 移动与云盘取证**：iOS/Android 备份/镜像、云盘本地同步客户端、容器镜像取证
3. **V5-3 GQL 图查询**：案件内图查询语言、关联分析 DSL
4. **V5-4 生产部署**：安装包、自动更新、崩溃上报、遥测、性能基线

### 7.3 风险
- V5 范围较大，当前 V3/V4 的规范债务（死代码、String error、未接入 parser）若不清偿，会进一步放大技术债
- `docs/release-drill/v5-rc1-report.md` 模板已存在但内容为空，文档漂移风险

---

## 8. 关键问题清单与优先级路线图

### P0 — 立即阻断 CI/提交

| # | 问题 | 位置 | 修复动作 | 预估工时 |
|---|---|---|---|---|
| 1 | `cargo fmt --check` 失败 | `entity_resolution/cross_case.rs`、`artifacts-android/contacts.rs`、`fs-hfsplus/src/lib.rs` | 运行 `cargo fmt --all` 并提交 | 10 min |
| 2 | `pnpm lint` 失败 | `BatchHistory.tsx`、`NotebookPanel.tsx` 等 | 修复 2 errors + 14 warnings | 30 min |
| 3 | AGENTS.md 未提交且计数漂移 | 根目录 | 校准 workspace member 数、crate/command/repo 计数，提交或回滚 | 1 h |

### P1 — 架构与规范债务（建议 V4 收尾前完成）

| # | 问题 | 影响 | 修复动作 | 预估工时 |
|---|---|---|---|---|
| 4 | Linux/macOS/PST 解析器未接入流水线 | V3 功能无法使用、关联分析断层 | 为各 parser 实现 `ArtifactExtractor`，注册到 `artifact_service.rs`，设置 `source_object_id` | 2-3 d |
| 5 | 服务层 `Result<T, String>` 泛滥 | 错误分类脆弱、调试困难 | 定义 typed error（`FileServiceError`、`JobServiceError` 等），实现 `From` → `CommandError` | 3-5 d |
| 6 | 生产代码 `#[allow(dead_code)]` 68 处 | 隐藏未测试/未使用代码 | 删除未使用代码，parser format 常量除外 | 1-2 d |
| 7 | 超大生产文件 | 违反项目自身门禁、可维护性差 | 拆分 `pipeline.rs`、`file_service/mod.rs`、`v2_governance_service.rs`、`fs-hfsplus/src/lib.rs`、`containers-pst/src/pst.rs` | 2-3 d |
| 8 | 命令层直接操作 SQLite | 分层边界模糊 | 在 app-services 提供 case-scoped connection 获取器，命令层只负责校验与委托 | 2-3 d |
| 9 | Notebook/Batch/MediaUrl 前后端模型不同步 | 运行时字段不匹配 | 统一字段名、补齐缺失字段、更新 `models.ts` | 1-2 d |
| 10 | Workspace 依赖未集中化 | 版本碎片化、升级困难 | 将 `rusqlite`、`uuid`、`reqwest`、`tokio`、`r2d2`、`byteorder` 等移入 root `[workspace.dependencies]` | 1-2 d |

### P2 — 健壮性与质量提升

| # | 问题 | 修复动作 |
|---|---|---|
| 11 | 全局 E01 cache `.expect("cache invariant")` | 改为返回错误并清空/重建缓存 |
| 12 | `block_in_place` 等待 MCP 锁 | 改为 async 命令或明确调用上下文 |
| 13 | `CommandError::from_service_error` 字符串匹配 | 服务层 typed error 后用显式 `From` 映射 |
| 14 | `validate_export_destination_path` 未检查保留设备名 | 复用 import 校验逻辑 |
| 15 | `ErrorBoundary` 直接暴露错误消息 | 分类展示，详情进日志 |
| 16 | 前端 `jobs/hooks.ts` 全局可变状态 | 改为 hook 局部状态或 store reset 生命周期 |
| 17 | 后端覆盖率无阈值 | 在 CI 中增加最低阈值 |
| 18 | Benchmark medium/large 阈值未实测 | 让 benchmark 按 level 选择数据集，或移除未测量声明 |

### P3 — 长期优化

| # | 问题 | 修复动作 |
|---|---|---|
| 19 | 重复 `RawImageReader` | 统一实现或从 evidence-core 移除 |
| 20 | FILETIME→UTC 转换 6 处重复 | 集中到 `infrastructure` 或 `registry/lookup` |
| 21 | DTO ↔ Domain 手写映射 | 实现 `From`/`TryFrom` trait |
| 22 | `EventTopic` 手工字符串同步 | 引入 strum 或 codegen |
| 23 | `analysis.rs` 过大 | 拆分为 `analysis/` 子目录 |
| 24 | `v2-runtime-results.json` 手工维护 | nightly pipeline 自动生成 |
| 25 | 补齐 `ci-desktop.yml` / nightly / release workflows | 按 `ci.md` 落地 |

---

## 9. 评分卡

| 维度 | 评分 | 说明 |
|---|---|---|
| 架构设计 | B+ | 分层清晰，但命令层越界、服务层错误弱类型 |
| 领域模型 | A- | ID 包装、时间戳、DTO 契约规范，但前后端同步有漂移 |
| 证据处理 | B+ | E01/NTFS 成熟，V4 fs crate 待规范清理 |
| 制品解析 | B- | Windows 成熟，Linux/macOS/PST 未接入 |
| 应用服务 | B | 编排能力强，但文件过大、unwrap 多、String error 多 |
| Tauri 命令层 | B- | 模板一致，但越界、死代码、文件过大 |
| 前端 | B | 分层好，测试达标，但 lint 失败、组件过大、全局状态 |
| 测试/CI | B | 覆盖广，guard 多，但 fmt/lint 失败、覆盖率无后端阈值 |
| 安全/治理 | B+ | 媒体/MCP/路径安全良好，依赖治理待集中化 |
| 文档/方向 | B+ | 文档层级清晰，V5 方向明确，但 AGENTS 漂移、RC 模板空 |

**综合评分：B（约 80-82/100）**

与项目自评 V2 Grade B（81/100）基本一致，但 V3/V4 的规范债务已使部分门禁实际无法通过。若 P0/P1 项得到清偿，项目可稳定达到 B+；若再补齐前后端 codegen、结构化错误、后端覆盖率阈值，可向 A- 迈进。

---

## 10. 附录：关键文件清单

| 文件 | 状态 |
|---|---|
| `Cargo.toml` | workspace members = 38 |
| `AGENTS.md` | M（未提交）|
| `crates/app-services/src/file_service/mod.rs` | 1724 行 |
| `crates/app-services/src/v2_governance_service.rs` | 1733 行 |
| `crates/containers-pst/src/pst.rs` | 1597 行 |
| `crates/fs-hfsplus/src/lib.rs` | 1608 行，含 `#![allow(dead_code)]` |
| `apps/desktop/src-tauri/src/commands/import/pipeline.rs` | 2285 行 |
| `apps/desktop/src-tauri/src/commands/file_commands.rs` | 1035 行 |
| `apps/desktop/src-tauri/src/commands/case_commands.rs` | 866 行 |
| `frontend/src/app/pages/FileBrowser.tsx` | 1141 行 |
| `frontend/src/components/analysis/AnalysisPanels.tsx` | 1201 行 |
| `frontend/src/components/notebook/NotebookPanel.tsx` | 1109 行 |

---

## 11. 审计后建议的下一步

1. **立即**：运行 `cargo fmt --all`、`pnpm lint --fix`，修复 P0 问题。
2. **本周**：审阅并提交/回滚 `AGENTS.md`，校准文档计数。
3. **V4 收尾冲刺**：
   - 接入 Linux/macOS/PST parser（最高优先级功能缺口）
   - 清理 `#[allow(dead_code)]` 和 `Result<T, String>`
   - 拆分超大文件
4. **V5 启动前**：
   - 统一 workspace dependencies
   - 实现服务层 typed error
   - 补齐后端覆盖率阈值和 desktop/nightly/release CI

---

*报告由 Kimi Code CLI 于 2026-06-21 生成。*
