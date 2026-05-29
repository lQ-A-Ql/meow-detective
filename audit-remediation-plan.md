# Audit Remediation Plan

## Phase 0 — Security Fixes (P0)

### Task 0.1: 修复 `create_file_reader_fn` 路径遍历漏洞
- **文件**: `apps/desktop/src-tauri/src/commands/file_commands.rs:19-32`
- **问题**: `source_path.join(&file_id.0)` 未校验 `file_id` 是否含 `../`
- **方案**: 复用 `file_service::safe_relative_path()`，对 `file_id.0` 做清洗后再 join
- **验证**: 编写测试用例传入含 `../` 的 file_id，确认被拒绝

### Task 0.2: 收敛错误信息泄露
- **文件**: 全部 `apps/desktop/src-tauri/src/commands/*.rs`
- **问题**: `.map_err(|e| e.to_string())` 将内部路径、SQL 细节暴露给前端
- **方案**:
  1. 在 `transport::errors` 中定义 `CommandError` 枚举（NotFound / Forbidden / Internal / ...）
  2. 实现 `From<CaseServiceError> for CommandError`、`From<DbError> for CommandError`
  3. `Internal` 变体只暴露泛化消息，详细错误写日志
  4. Tauri command 返回 `Result<T, CommandError>`（Tauri 2 支持 `Serialize` 的 error type）
- **验证**: 故意触发 DB 错误，确认前端收到的 message 不含 SQL 语句

### Task 0.3: 近案例文件权限加固
- **文件**: `apps/desktop/src-tauri/src/commands/case_commands.rs:250-265`
- **问题**: `%APPDATA%/ForensicsWorkbench/forensics-recent-cases.json` 明文存储
- **方案**:
  1. 创建目录时设置仅当前用户可读写（Windows: `CreateDirectoryW` + ACL，或使用 `dirs::data_dir()` + `.permissions(0o600)`）
  2. 写入前检查父目录权限
- **验证**: 用另一 OS 用户尝试读取该文件，确认被拒绝

---

## Phase 1 — Error Handling & Logging (P0)

### Task 1.1: 引入 `tracing` 日志框架
- **文件**: `Cargo.toml`（workspace deps）、`apps/desktop/src-tauri/src/main.rs`
- **方案**:
  1. workspace deps 添加 `tracing` + `tracing-subscriber`
  2. `main.rs` 初始化 `tracing_subscriber`（file appender + stderr）
  3. 将所有 `eprintln!`（37处）替换为 `tracing::error!` / `tracing::warn!` / `tracing::info!`
- **涉及文件**:
  - `file_commands.rs` (4处)
  - `artifact_service.rs` (1处)
  - 全部 `eprintln!` 调用
- **验证**: 导入一个损坏的镜像，确认日志文件中记录了错误

### Task 1.2: 事件发射错误处理
- **文件**: `apps/desktop/src-tauri/src/events/event_bridge.rs`
- **问题**: `let _ = emit_event(...)` 静默忽略失败
- **方案**:
  1. `emit_event` 返回 `Result`，调用方 match 结果
  2. 发射失败时 `tracing::warn!` 记录 topic + error
  3. 对于 `job.failed` 事件，失败时 fallback 到直接更新 DB job 状态
- **验证**: 模拟 AppHandle 无效场景，确认日志有 warn 记录

### Task 1.3: 后台线程错误回传
- **文件**: `apps/desktop/src-tauri/src/commands/file_commands.rs:134-140`
- **问题**: `std::thread::spawn` 中错误只 `eprintln!`
- **方案**:
  1. 已有 `job_repo.fail()` 逻辑，确保其错误不被忽略
  2. `run_background_import_job` 中，`job_repo.fail()` 失败时通过 `tracing::error!` 记录
  3. 考虑引入 channel 将错误传回主线程（可选，Phase 2 扩展）
- **验证**: 导入不存在的路径，确认 Jobs 面板显示 failed 状态

### Task 1.4: 消除生产代码中的 unwrap()
- **文件**:
  - `apps/desktop/src-tauri/src/lib.rs:69` — `.expect()` 改为 graceful error + exit code
  - `crates/app-services/src/case_service.rs:118` — `last_err.unwrap()` 改为 `unwrap_or_else` 或提前 return
- **方案**: 逐个替换为 `?` 或 `unwrap_or_else(|| unreachable!())` 并加注释
- **验证**: `cargo clippy -- -D warnings` 通过

---

## Phase 2 — Concurrency & State (P1)

### Task 2.1: 缩短 Mutex 持锁范围
- **文件**:
  - `apps/desktop/src-tauri/src/commands/case_commands.rs:70-108` (`get_case_metrics`)
  - `case_commands.rs:111-128` (`get_recent_objects`)
  - `case_commands.rs:131-149` (`get_data_sources`)
- **问题**: 在持锁状态下执行多条 SQL 查询
- **方案**:
  1. 在锁内只获取 `conn` 引用（或 clone db_path 后新开连接）
  2. 释放锁后执行 SQL
  3. 对于 `ActiveCase`，提取 `with_conn` 的 guard 为独立 RAII 结构
- **验证**: 写并发测试同时调用 `get_case_metrics` + `get_file_tree`，无死锁

### Task 2.2: 同步 Tauri 命令改异步
- **文件**:
  - `apps/desktop/src-tauri/src/commands/search_commands.rs` — `search_files` / `search_files_request`
  - `apps/desktop/src-tauri/src/commands/timeline_commands.rs` — `get_timeline_events`
- **方案**:
  1. 改为 `async fn` + `spawn_blocking`
  2. 搜索索引打开操作考虑缓存（避免每次 reopen）
- **验证**: 大数据量搜索时 UI 不冻结

### Task 2.3: 后台线程取消机制
- **文件**: `apps/desktop/src-tauri/src/commands/file_commands.rs:134`
- **方案**:
  1. 引入 `Arc<AtomicBool>` 作为 cancel token
  2. 存入 `AppState` 的 `HashMap<JobId, Arc<AtomicBool>>`
  3. 在遍历循环中每 N 次迭代检查 cancel flag
  4. 暴露 `cancel_import(job_id)` Tauri 命令
- **验证**: 导入进行中调用 cancel，确认线程停止且 job 状态为 cancelled

---

## Phase 3 — Data Layer Fixes (P1)

### Task 3.1: 时间线分页支持
- **文件**:
  - `apps/desktop/src-tauri/src/commands/timeline_commands.rs`
  - `crates/app-services/src/timeline_service.rs`
  - `crates/transport/src/commands/mod.rs`
- **方案**:
  1. 定义 `GetTimelineRequest { offset: u64, limit: u64, time_start: Option<String>, time_end: Option<String> }`
  2. `timeline_service::query_timeline` 接受 offset/limit 参数
  3. 前端 `useTimeline` hook 支持分页参数
  4. 返回 `PagedResult<TimelineEventDto>` 含 `total` 字段
- **验证**: 导入含 >100 条时间线的数据源，确认分页加载正确

### Task 3.2: 搜索结果计时与总数
- **文件**: `crates/app-services/src/search_service.rs:59-102`
- **方案**:
  1. 用 `std::time::Instant` 计时替换 `took_ms: 0`
  2. `search` 接口返回 `(total_count, items)`，`total` 用索引的总文档数
- **验证**: 搜索后确认 `tookMs > 0`，`total` >= `items.length`

### Task 3.3: 硬编码限制配置化
- **文件**:
  - `file_commands.rs:59` — artifact 提取限制 500
  - `file_commands.rs:83` — 文本索引限制 1000
  - `file_service.rs:27` — `MAX_RANGE_LENGTH` 1MB
  - `job_service.rs:7` — job 列表限制 12
  - `timeline_commands.rs:14` — 时间线限制 100
- **方案**:
  1. 在 `crates/infrastructure/src/config.rs` 新增 `IngestConfig` 结构体
  2. 使用 `serde` + 配置文件（`case.json` 或独立 `config.toml`）
  3. Tauri command 层读取配置传入 service 层
- **验证**: 修改配置文件后重新导入，确认限制生效

### Task 3.4: 消除硬编码 Mock 函数
- **文件**:
  - `crates/app-services/src/search_service.rs:104-129` — `search_files()`
  - `crates/app-services/src/job_service.rs:69-108` — `get_jobs_snapshot()`
  - `crates/app-services/src/artifact_service.rs:83-129` — `get_artifact_families()` / `get_artifact_rows()`
- **问题**: 这些函数返回硬编码假数据，在 Tauri 模式下可能被误调用
- **方案**:
  1. 删除这些 mock 函数
  2. 前端 mock 调用只走 `mockProvider`，后端不再提供 mock 数据
  3. 确保 Tauri command 层全部走 DB 查询路径
- **验证**: `cargo build` 无编译错误，前端 mock 模式仍正常

---

## Phase 4 — Frontend-Backend Contract (P1)

### Task 4.1: EventTopic 类型同步
- **文件**: `frontend/src/types/models.ts:10-21`
- **方案**:
  1. 添加 `| 'partition.progress'` 到 `EventTopic` union
  2. 添加对应的 payload 类型 `PartitionProgressPayload`
- **验证**: TypeScript 编译通过，前端能接收 partition progress 事件

### Task 4.2: FileEntryRow DTO 补全
- **文件**: `frontend/src/types/models.ts:95-109`
- **方案**: 确认 `changedAt` 字段存在（当前已存在），检查 mock 数据是否一致
- **验证**: mock 数据与接口定义完全匹配

### Task 4.3: RecentObject kind 类型对齐
- **文件**: `frontend/src/types/models.ts:46-52`
- **方案**: 将 `kind` 改为 `string` 或扩展 union 以匹配后端可能的值
- **验证**: 后端返回任意 kind 字符串时前端不报类型错误

### Task 4.4: Mock 数据时间格式统一
- **文件**: `frontend/src/lib/api/mock-data.ts`
- **问题**: 时间格式 `YYYY-MM-DD HH:MM:SS` vs 后端 ISO 8601
- **方案**: 统一为 ISO 8601（`2026-05-16T10:30:00Z`）
- **验证**: mock 模式下时间显示正确

---

## Phase 5 — Code Quality (P2)

### Task 5.1: NTFS/FAT 枚举重复代码消除
- **文件**: `apps/desktop/src-tauri/src/commands/file_commands.rs:554-616`
- **方案**:
  1. 提取 `fn enumerate_partition_filesystem(reader, candidate, conn, ...) -> Result<EnumerationStats>`
  2. NTFS/FAT 分支只在创建 reader 时不同，后续逻辑合并
- **验证**: 现有测试全部通过

### Task 5.2: 文件遍历逻辑去重
- **文件**: `crates/app-services/src/file_service.rs`
- **问题**: `enumerate_filesystem_with_root_name` 与 `replace_placeholder_root_with_real` 有大量重复
- **方案**:
  1. 提取 `fn walk_and_insert_filesystem(repo, fs, parent_id, data_source_id, stats, batch_size, progress_fn)`
  2. 两个函数复用此核心遍历逻辑
- **验证**: 现有测试全部通过

### Task 5.3: 魔法数字常量化
- **文件**: 多个文件
- **方案**:
  1. `file_commands.rs` — `ARTIFACT_EXTRACTION_LIMIT = 500`, `TEXT_INDEX_LIMIT = 1000`
  2. `job_service.rs` — `JOB_LIST_LIMIT = 12`
  3. `case_commands.rs` — `MAX_RECENT_CASES` 已存在，确认其他位置
  4. 集中定义在 `infrastructure/src/constants.rs`
- **验证**: `cargo clippy` 无 magic number 警告

### Task 5.4: SQL 查询迁移到 Repository 层
- **文件**: `apps/desktop/src-tauri/src/commands/case_commands.rs:78-88`
- **方案**:
  1. 在 `case_repo` 或新建 `metrics_repo` 中添加 `fn get_case_metrics(conn) -> Result<CaseMetricsDto>`
  2. Tauri command 只调用 repository 方法
- **验证**: 现有测试通过

---

## Phase 6 — Testing (P2)

### Task 6.1: 前端测试框架搭建
- **文件**: `frontend/package.json`, `frontend/vitest.config.ts`（新建）
- **方案**:
  1. 添加 `vitest` + `@testing-library/react` + `jsdom`
  2. `package.json` 添加 `"test": "vitest"` script
  3. 编写第一个 smoke test
- **验证**: `pnpm test` 通过

### Task 6.2: 前端 API hooks 测试
- **文件**: `frontend/src/features/case/hooks.test.ts`（新建）
- **方案**:
  1. 测试 `useCreateCase`、`useOpenCase` 的 mutation 逻辑
  2. 使用 `msw` mock Tauri invoke
- **验证**: hooks 测试覆盖主要 mutation 场景

### Task 6.3: 测试 fixture 管理
- **文件**: `crates/testing/src/fixtures/mod.rs`
- **问题**: 多个测试硬编码外部 E01 路径
- **方案**:
  1. 创建小型测试 E01 镜像（<1MB）放入 `testdata/`
  2. 提供 `fn test_e01_path() -> PathBuf` 在 `testing` crate 中
  3. 替换所有 `skip()` + 硬编码路径
- **验证**: CI 环境下不再 skip 测试

### Task 6.4: 补充 service 层集成测试
- **文件**: `crates/app-services/tests/` 目录
- **方案**:
  1. `search_service_test.rs` — 索引 + 搜索 + 分页
  2. `timeline_service_test.rs` — 投影 + 查询 + 分页
  3. `artifact_service_test.rs` — 提取 + 存储 + 查询
- **验证**: `cargo test --workspace` 新增测试全部通过

---

## Phase 7 — Observability & Polish (P3)

### Task 7.1: React Error Boundary
- **文件**: `frontend/src/app/App.tsx`, `frontend/src/app/components/ErrorBoundary.tsx`（新建）
- **方案**:
  1. 创建 `ErrorBoundary` 组件，捕获渲染错误
  2. 在 `App.tsx` 的 router 外层包裹
  3. 显示友好错误页 + "重新加载" 按钮
- **验证**: 故意 throw 错误，确认 error boundary 捕获

### Task 7.2: 依赖版本锁定
- **文件**: `Cargo.toml`
- **方案**:
  1. 将 `"1"` 改为 `"~1.0"` 或具体 patch 版本
  2. 特别关注 `serde`, `rusqlite`, `tauri` 等关键依赖
  3. 添加 `cargo-deny` CI step 检查已知漏洞
- **验证**: `cargo update --dry-run` 无意外 major 升级

### Task 7.3: 公共 API 文档注释
- **文件**: `crates/app-services/src/*.rs`, `crates/transport/src/**/*.rs`
- **方案**:
  1. 为所有 `pub fn` 添加 `///` 文档注释
  2. 重点：`file_service`, `case_service`, `datasource_service` 的公共接口
  3. 复杂算法（分区检测、MBR/GPT 解析）添加内联注释
- **验证**: `cargo doc --workspace --no-deps` 无警告

### Task 7.4: 删除无用的后端 Mock 函数残留
- **文件**: 检查 `job_service::get_jobs_snapshot`, `get_warnings`, `get_trace_items`
- **方案**: 确认是否仍有调用点，若有则改为 DB 查询；若无则删除
- **验证**: `cargo build` 无 dead code 警告

---

## Execution Order

```
Phase 0 ──→ Phase 1 ──→ Phase 2 ──→ Phase 3
  (安全)      (错误)      (并发)      (数据)
                                          │
                                          ▼
Phase 7 ←── Phase 6 ←── Phase 5 ←── Phase 4
 (polish)    (测试)      (质量)      (契约)
```

Phase 0-1 可并行。Phase 2-3 可并行。Phase 4-5 可并行。Phase 6 依赖 Phase 0-5 完成。Phase 7 最后收尾。
