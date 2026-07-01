# Forensics Workbench 完整审计执行计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 对 Forensics Workbench 进行一次综合审计，生成一份覆盖项目健康、前后端代码质量、架构/数据流、以及三个重点模块（搜索/索引、时间线、取证总览）的 Markdown 报告。

**架构：** 本计划将审计工作拆分为资料收集、门禁检查、分层分析、重点模块走查和报告整合五阶段。每个阶段产出一个可独立审阅的文档片段，最终合并为完整报告。

**Tech Stack：** Rust workspace（Tauri 2）、React 18 + TypeScript + Vite + Tailwind 4、SQLite + WAL、pnpm。

## Global Constraints

- 不修改业务代码；只生成报告。
- 不跑需要 VS 链接器的全量 `cargo test` / `cargo clippy`。
- 必须运行并通过：`cargo fmt --all -- --check`、`pnpm --dir frontend typecheck`、`pnpm --dir frontend lint`。
- 必须运行至少 3 个 PowerShell 检查脚本并记录结果。
- 报告保存到 `docs/compose/reports/2026-06-30-forensics-audit-report.md`。

---

### Task 1: 项目资料收集与目录扫描

**Covers:** [S1], [S2], [S3]

**Files:**
- Read: `Cargo.toml`, `frontend/package.json`, `frontend/vite.config.ts`, `AGENTS.md`, `docs/v5-plan.md`, `testdata/governance/v2-known-limitations.json`
- Read: `crates/transport/src/events/mod.rs`, `crates/transport/src/dto/mod.rs`, `apps/desktop/src-tauri/src/lib.rs`
- List: `crates/`, `frontend/src/app/pages/`, `frontend/src/lib/api/`, `scripts/`

**Interfaces:**
- Produces: 项目基础指标清单（crate 数、命令数、页面数、测试数、已知限制数）。

- [ ] **Step 1: 收集项目基础指标**
  读取 `Cargo.toml` 确认 workspace members；读取 `lib.rs` 确认命令数；读取 `frontend/src/app/pages/` 和 `frontend/src/lib/api/` 确认页面与 API 文件数；读取 `testdata/governance/v2-known-limitations.json` 确认已知限制数。

- [ ] **Step 2: 记录文档状态**
  检查 `docs/` 目录下的关键文档（PRD.md、spec.md、design.md、ci.md、test-plan.md、v2/v3/v4/v5 计划）是否存在且最近更新。

- [ ] **Step 3: 汇总已知事实**
  将收集到的指标写入临时审计笔记，供后续章节引用。

---

### Task 2: 运行快速门禁检查

**Covers:** [S2], [S5]

**Files:**
- Run: `cargo fmt --all -- --check`
- Run: `pnpm --dir frontend typecheck`
- Run: `pnpm --dir frontend lint`
- Run: `powershell -ExecutionPolicy Bypass -File scripts/check-command-sql-boundary.ps1`
- Run: `powershell -ExecutionPolicy Bypass -File scripts/check-dead-code-allow-guard.ps1`
- Run: `powershell -ExecutionPolicy Bypass -File scripts/check-media-protocol-guard.ps1`
- Run: `powershell -ExecutionPolicy Bypass -File scripts/check-frontend-lockfile-policy.ps1`

**Interfaces:**
- Produces: 各门禁脚本的执行结果（PASS/FAIL）和任何错误输出。

- [ ] **Step 1: 运行格式化检查**
  命令：`cargo fmt --all -- --check`
  预期：零退出码。

- [ ] **Step 2: 运行前端类型检查**
  命令：`pnpm --dir frontend typecheck`
  预期：无类型错误。

- [ ] **Step 3: 运行前端 lint**
  命令：`pnpm --dir frontend lint`
  预期：无 lint 错误。

- [ ] **Step 4: 运行 PowerShell 检查脚本**
  命令：逐个运行上述 4 个脚本。
  预期：全部 PASS。

- [ ] **Step 5: 记录结果**
  将结果写入临时审计笔记。

---

### Task 3: 后端代码质量分析

**Covers:** [S3]

**Files:**
- Read: `crates/transport/src/errors/mod.rs` 或 `crates/transport/src/errors.rs`
- Read: `crates/app-services/src/lib.rs` 及周边服务文件
- Read: `crates/persistence-sqlite/src/repositories/*.rs`（选读 3-5 个核心）
- Grep: `Result<String, String>`、`#[allow(dead_code)]`、`unsafe` 在 `crates/` 中的分布
- Read: `deny.toml`

**Interfaces:**
- Consumes: Task 1 的项目基础指标。
- Produces: 后端质量分析章节草稿。

- [ ] **Step 1: 检查错误处理类型化**
  统计 `Result<T, String>` 出现次数；确认新增 crate 是否使用 `thiserror` 定义 typed error；检查 `ApiErrorDto` 使用情况。

- [ ] **Step 2: 检查 unsafe 与死码**
  使用 grep 统计 `unsafe` 块数量，抽查是否有 `// SAFETY:` 注释；使用 grep 确认生产代码中无 `#[allow(dead_code)]`。

- [ ] **Step 3: 检查模块大小与 SQL 边界**
  检查是否有超过 1500 行的生产源文件；确认 Tauri 命令中无裸 SQL，SQL 集中在 repository 层。

- [ ] **Step 4: 检查依赖治理**
  阅读 `deny.toml` 中的 license/ban/advisory/source 规则；确认异常是否带 owner/reason/expires。

- [ ] **Step 5: 撰写后端质量分析章节**
  将发现写入报告草稿。

---

### Task 4: 前端代码质量分析

**Covers:** [S3]

**Files:**
- Read: `frontend/tsconfig.json`, `frontend/src/types/models.ts`, `frontend/src/lib/api/*.ts`
- Read: `frontend/src/stores/*.ts`
- Read: `frontend/src/app/pages/*.tsx`（选读 5-8 个核心页面）
- Grep: `invoke(` 在 `frontend/src/` 中的分布
- Read: `frontend/src/test/setup.ts`

**Interfaces:**
- Consumes: Task 1 的项目基础指标。
- Produces: 前端质量分析章节草稿。

- [ ] **Step 1: 检查 TypeScript 与类型契约**
  确认 `tsconfig.json` 启用 strict；检查 `models.ts` 与 `crates/transport/src/dto/` 是否大致同步；检查 API 层是否统一使用 `apiClient.request`。

- [ ] **Step 2: 检查状态管理**
  分析 Zustand stores 的边界、命名和依赖关系；检查是否存在 store 间循环依赖。

- [ ] **Step 3: 检查组件与页面**
  确认页面文件大小不超过 500 行；检查是否通过 hooks 消费 API；检查 `invoke` 是否只出现在 API 层。

- [ ] **Step 4: 检查测试与覆盖**
  统计测试文件数量；检查是否配置 Vitest coverage thresholds；记录当前阈值。

- [ ] **Step 5: 撰写前端质量分析章节**
  将发现写入报告草稿。

---

### Task 5: 架构与数据流分析

**Covers:** [S3]

**Files:**
- Read: `apps/desktop/src-tauri/src/lib.rs` 命令注册
- Read: `apps/desktop/src-tauri/src/commands/*.rs`（选读 2-3 个）
- Read: `crates/transport/src/events/mod.rs`
- Read: `crates/transport/src/commands/mod.rs` 或类似命令定义文件
- Read: `apps/desktop/src-tauri/src/state/app_state.rs`

**Interfaces:**
- Consumes: Task 1 基础指标、Task 3 后端分析。
- Produces: 架构与数据流章节草稿。

- [ ] **Step 1: 映射命令分层**
  选择一个命令（如 `get_file_tree` 或 `search_indexed`），追踪其路径：Tauri command → app-services service → repository/DTO → 前端 API 层。

- [ ] **Step 2: 检查事件系统**
  确认 `crates/transport/src/events/mod.rs` 与 `frontend/src/types/models.ts` 中的 `EventTopic` 是否同步；检查 emitter 使用模式。

- [ ] **Step 3: 检查状态管理**
  分析 `AppState` 的职责边界：case、task manager、pool、MCP、settings 是否解耦。

- [ ] **Step 4: 撰写架构与数据流章节**
  将发现写入报告草稿。

---

### Task 6: 重点深入——搜索与目录索引

**Covers:** [S3], [S4]

**Files:**
- Read: `crates/search/src/lib.rs`, `crates/search/src/*.rs`
- Read: `crates/catalog/src/lib.rs`, `crates/catalog/src/*.rs`
- Read: `frontend/src/app/pages/Search.tsx` 与 `frontend/src/lib/api/search.ts`
- Read: `frontend/src/features/search/hooks.ts`（如果存在）

**Interfaces:**
- Consumes: Task 5 架构分析。
- Produces: 搜索与索引章节草稿。

- [ ] **Step 1: 理解索引架构**
  阅读 `crates/catalog` 和 `crates/search`，绘制索引生命周期：文件目录 → Catalog → ExtensionProjection / PathPrefixProjection / CatalogIndex → tantivy index。

- [ ] **Step 2: 检查查询与召回逻辑**
  检查搜索查询解析、高亮、评分逻辑；确认错误处理类型化。

- [ ] **Step 3: 检查前端集成**
  检查前端 Search 页面如何调用 API、管理状态、展示结果；确认是否遵循“页面不直接 invoke”原则。

- [ ] **Step 4: 撰写搜索与索引章节**
  将发现写入报告草稿，包括亮点、风险与改进建议。

---

### Task 7: 重点深入——时间线生成

**Covers:** [S3], [S4]

**Files:**
- Read: `crates/timeline/src/lib.rs`, `crates/timeline/src/*.rs`
- Read: `crates/transport/src/dto/timeline.rs`
- Read: `frontend/src/app/pages/Timeline.tsx` 与 `frontend/src/lib/api/timeline.ts`
- Grep: `source_object_id` / `sourceObjectId` 在 `crates/` 和 `frontend/` 中的使用

**Interfaces:**
- Consumes: Task 5 架构分析。
- Produces: 时间线章节草稿。

- [ ] **Step 1: 理解时间线事件来源**
  阅读 `crates/timeline`，确认事件生成逻辑、MACB 投影、聚合函数。

- [ ] **Step 2: 检查 sourceObjectId 关联**
  使用 grep 确认 artifact 提取器是否设置 `sourceObjectId`；检查时间线如何与 artifact 关联。

- [ ] **Step 3: 检查前端集成**
  检查 Timeline 页面如何展示时间线、过滤、聚合。

- [ ] **Step 4: 撰写时间线章节**
  将发现写入报告草稿，包括亮点、风险与改进建议。

---

### Task 8: 重点深入——取证总览（后端 + 前端）

**Covers:** [S3], [S4]

**Files：**
- Read: `crates/artifacts-core/src/lib.rs`
- Read: `crates/artifacts-windows/src/registry/mod.rs` 与 1-2 个 extractor（如 `system.rs`, `amcache.rs`）
- Read: `crates/artifacts-windows/src/evtx.rs` 或 `evtx.rs` 主文件
- Read: `frontend/src/app/pages/CaseOverview.tsx`, `V3Dashboard.tsx`, `V3ScoreCards.tsx`
- Read: `crates/transport/src/dto/artifacts.rs` 或相关 DTO

**Interfaces:**
- Consumes: Task 5 架构分析。
- Produces: 取证总览章节草稿。

- [ ] **Step 1: 理解 artifact 框架**
  阅读 `artifacts-core`，确认 extractor 注册、执行、错误处理模型。

- [ ] **Step 2: 检查 Windows 解析器实现**
  抽查注册表、EVTX、Amcache 解析器，确认错误处理、边界条件、测试覆盖。

- [ ] **Step 3: 检查前端概览页面**
  阅读 CaseOverview、V3Dashboard、V3ScoreCards，确认数据获取、展示、状态管理边界。

- [ ] **Step 4: 撰写取证总览章节**
  将发现写入报告草稿，包括亮点、风险与改进建议。

---

### Task 9: 整合报告与最终验证

**Covers:** [S1], [S3], [S5]

**Files：**
- Create: `docs/compose/reports/2026-06-30-forensics-audit-report.md`
- Read: 前 8 个任务产生的所有章节草稿

**Interfaces:**
- Consumes: 所有任务输出。
- Produces: 最终审计报告。

- [ ] **Step 1: 撰写执行摘要**
  综合所有章节，给出总体评分、Top 5 优先级改进项。

- [ ] **Step 2: 合并章节并统一格式**
  将各章节草稿合并为一个 Markdown 文件，统一标题、表格、列表风格。

- [ ] **Step 3: 最终检查**
  再次确认所有数字、路径、结论一致；确认无占位符。

- [ ] **Step 4: 通知用户**
  报告路径：`docs/compose/reports/2026-06-30-forensics-audit-report.md`。

---

## Self-Review

- **Spec coverage:** S1→Task 1；S2→Tasks 1-2；S3→Tasks 3-8；S4→Tasks 6-8；S5→Tasks 2, 9。全覆盖。
- **Placeholder scan:** 无 TBD/TODO/实现细节占位符。
- **Type consistency:** 任务间通过报告草稿和笔记共享，无需严格类型签名。
