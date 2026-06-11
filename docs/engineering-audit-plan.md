# Forensics Workbench 工程化全量审计方案

## 1. 目标与原则

本文档定义 Forensics Workbench 的工程化全量审计方法。它不是一次性的评审报告，而是一套可重复执行的审计清单，用于在功能开发、阶段收口、发布前门禁和风险修复后确认项目仍满足取证工具的正确性、安全性、可维护性和可验证性要求。

审计遵循以下原则：

- **证据优先**：每条结论必须能回到代码、迁移、测试、CI、文档或实际运行输出。
- **取证正确性优先**：凡可能造成证据遗漏、证据篡改、错误归因、错误时间线或不可复现报告的问题，优先级高于普通工程质量问题。
- **契约先行**：跨层变更先审 `crates/transport` DTO/commands/events，再审 Tauri command、service、前端类型、API、hook 和 mock。
- **边界清晰**：React UI 不绕过 API 层；Tauri command 不承载业务；SQL 不散落在 command 层；证据读取保持只读和范围受控。
- **可执行闭环**：每个发现项必须给出风险等级、证据、修复建议、验证命令和验收标准。

## 2. 审计范围

| 维度 | 审计对象 | 主要证据 |
|---|---|---|
| 产品与范围 | `PRD.md`, `spec.md`, `design.md`, `frontend-ui-ux.md` | MVP 边界、非目标、用户主链路 |
| 架构边界 | `Cargo.toml`, `crates/*/Cargo.toml`, `apps/desktop/src-tauri/src`, `frontend/src` | workspace members、crate 依赖、调用方向 |
| Transport 契约 | `crates/transport/src/dto`, `crates/transport/src/commands`, `crates/transport/src/events` | DTO、request shape、event topic、serde 命名 |
| Tauri command | `apps/desktop/src-tauri/src/commands`, `src/lib.rs`, capabilities | command 注册、权限、错误返回、状态注入 |
| 应用服务 | `crates/app-services/src` | 编排职责、事务边界、任务/取消/进度 |
| 证据读取 | `evidence-core`, `image-*`, `fs-*`, `artifacts-windows` | 只读读取、边界校验、解析器错误语义 |
| 持久化 | `crates/persistence-sqlite/src`, migrations scripts | schema、repo 边界、迁移原子性、索引 |
| 搜索/时间线/报告 | `search`, `timeline`, `reports`, frontend pages | 索引、查询、归一化、导出可信度 |
| 前端工程 | `frontend/src/lib/api`, `features/*/hooks.ts`, `types/models.ts`, stores/pages | mock/tauri mode、类型同步、状态分层 |
| 事件系统 | Rust event constants、Tauri emit bridge、frontend `EventBus` | topic 同步、payload 安全、订阅失效 |
| 测试与 CI | `test-plan.md`, `ci.md`, `.github/workflows`, tests | 默认 gate、慢测、fixtures、覆盖阈值 |
| 依赖与安全 | `deny.toml`, lockfiles, `SECURITY.md`, audit docs | advisories、license、source、例外过期 |
| 文档一致性 | `docs/`, `development-reports/`, root docs | 当前状态、风险登记、开发记录 |

## 3. 风险分级

| 等级 | 定义 | 示例 |
|---|---|---|
| `P0` | 阻断取证正确性、安全边界或发布可信度，必须立即修复 | 任意路径删除、证据写入、时间线错误归因、command 层绕过路径校验 |
| `P1` | 阻断核心主链路可靠性或可能造成数据丢失 | 导入取消后继续写库、迁移失败部分落库、DTO 两侧不兼容 |
| `P2` | 影响维护性、性能、一致性或审计可追溯性 | SQL 分散、重复解析逻辑、缺少专项回归、文档状态过期 |
| `P3` | 优化项或低风险体验问题 | 辅助文案、非关键图表、低频页面 polish |

## 4. 审计执行顺序

1. **基线确认**：记录 git 状态、当前分支、未提交文件、工具链版本和关键命令可用性。
2. **事实源读取**：先读 root docs、`Cargo.toml`、transport、commands、frontend API/hook/mock、migrations。
3. **主链路审计**：按案件创建/打开、导入、文件浏览、预览、搜索、时间线、工件、报告、MCP 顺序检查。
4. **横切审计**：覆盖错误处理、路径安全、事件、任务取消、事务、依赖、安全、测试、CI。
5. **发现项归档**：每个发现项写入报告，包含风险等级、证据位置、影响、修复建议、验证方法。
6. **复核闭环**：修复后复跑对应 gate，更新 remediation plan 与 development record。

## 5. 可执行检查矩阵

| ID | 检查项 | 对象 | 方法 | 通过标准 | 风险 | 输出 |
|---|---|---|---|---|---|---|
| A-01 | 产品范围一致性 | PRD/spec/design/frontend docs | 对照当前页面、commands、crate 能力 | 文档不承诺未实现能力；非目标清晰 | P2 | 范围差异表 |
| A-02 | 分层依赖方向 | root `Cargo.toml`, crate manifests | 检查 workspace members 与依赖方向 | `transport/domain` 不反向依赖 app/service/UI；command 层薄适配 | P1 | 依赖图与异常 |
| A-03 | Transport DTO 同步 | Rust DTO 与 `frontend/src/types/models.ts` | 字段、命名、optional、enum 对照 | camelCase、optional、union topic 一致 | P1 | 契约差异清单 |
| A-04 | Command 注册完整性 | command files 与 `src/lib.rs` | 查 `#[tauri::command]` 与 handler | 新 command 已注册且命名被前端 API 使用 | P1 | command map |
| A-05 | Command 责任边界 | `apps/desktop/src-tauri/src/commands` | 查 SQL、复杂业务、裸路径操作 | command 只 validate -> service -> DTO | P1 | 边界违规项 |
| A-06 | 路径与证据只读 | case/import/viewer/media/filesystem | 查 canonicalize、starts_with、range、write/delete | 证据源不被修改；输出写入 case/export 安全路径 | P0 | 路径安全发现 |
| A-07 | SQLite schema 与 repo | migrations、repositories | 检查 schema、索引、外键、事务 | migration 原子；SQL 在 repo 层；关键查询有索引 | P1 | DB 审计表 |
| A-08 | 导入流水线正确性 | import commands、app-services、ingest、staging | 跟踪 job、phase、partition、partial result | 进度可信；失败可解释；取消无残留写入 | P0/P1 | 导入链路报告 |
| A-09 | 文件系统枚举 | `evidence-core`, `fs-*`, `file_service` | 查边界、分页、懒加载、deleted/orphan | 大目录不全量阻塞；错误可恢复 | P1 | FS 风险表 |
| A-10 | 搜索索引与查询 | `search`, search service/API/page | 查索引 schema、query parse、highlight、paging | 结果来源可追踪；分页/高亮稳定 | P1/P2 | 搜索审计表 |
| A-11 | 时间线归一化 | `timeline`, repo, frontend timeline | 查 timestamp、source_object、confidence/provenance | 同源多时间字段可解释；过滤/分页正确 | P0/P1 | 时间线审计表 |
| A-12 | Windows artifact parser | `artifacts-windows`, tests, analysis service | 查 parser 错误语义、fixture、provenance | malformed input 不 panic；输出含来源与可信度 | P0/P1 | parser 发现项 |
| A-13 | 报告可信度 | `reports`, report commands/frontend | 检查导出 scope、模板、source attribution | 报告可复现，包含案件/证据/时间/来源 | P1 | 报告审计表 |
| A-14 | 前端 API 分层 | `frontend/src/lib/api`, features hooks, pages | 查直接 `invoke`、mock fallback、query keys | 页面只用 hook/API；mock 与 tauri shape 同步 | P1/P2 | 前端分层报告 |
| A-15 | 事件与缓存失效 | event constants、event bridge、subscribers | 对照 Rust/TS topics 和 query invalidation | topic 同步；payload 不泄漏裸 host path | P1 | event map |
| A-16 | 任务状态机 | jobs repo/service/hooks | 查状态转换、取消、失败、progress | pending/running/completed/failed/cancelled 不乱跳 | P1 | 状态机差异 |
| A-17 | MCP 安全边界 | `mcp-client`, mcp commands/frontend | 查 SSE/Stdio、tool call、path/env/config | 默认最小权限；错误不泄漏敏感信息 | P0/P1 | MCP 风险表 |
| A-18 | 测试覆盖 | Rust tests、Vitest、fixtures | 统计主链路与回归测试 | P0/P1 修复必须带 regression test 或明确例外 | P1/P2 | 测试缺口 |
| A-19 | CI 与依赖治理 | workflows、`deny.toml`, lockfiles | 查 gate、audit、deny、pnpm audit | 默认 gate 可复现；例外有 owner/expiry | P1/P2 | CI 缺口 |
| A-20 | 文档与开发记录 | root docs、`docs/`, `development-reports` | 对照实现与最新状态 | 文档不互相矛盾；开发记录注明验证 | P2 | 文档修补清单 |

## 6. 证据采集命令

```bash
git status --short
rg --files -g "*.md" -g "*.rs" -g "*.ts" -g "*.tsx" -g "*.toml" -g "*.json"
rg -n "#\\[tauri::command\\]|invoke_handler|generate_handler" apps/desktop/src-tauri/src
rg -n "apiClient.request|invoke\\(" frontend/src/lib frontend/src/features frontend/src/app
rg -n "pub struct .*Dto|pub enum EventTopic|serde\\(rename_all" crates/transport/src
rg -n "CREATE TABLE|ALTER TABLE|CREATE INDEX" crates/persistence-sqlite/src/migrations/scripts
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
git diff --check
```

安全与依赖专项：

```bash
cargo audit
cargo deny check advisories bans licenses sources
pnpm --dir frontend audit --audit-level high
powershell -ExecutionPolicy Bypass -File scripts/check-command-sql-boundary.ps1
```

慢测和真实样本测试只在具备 fixture 时执行，并在报告中记录环境变量、样本 hash、运行时间和失败原因。

## 7. 发现项模板

```md
### P1: <短标题>

- ID: A-<编号>
- 证据: `<file>:<line>` / command output / test failure
- 影响: <对取证正确性、安全、性能或维护性的影响>
- 根因: <最小可验证根因>
- 建议: <修复方向，包含边界和不应做的事>
- 验证: <命令、测试、人工检查>
- 状态: Open / In Progress / Fixed / Accepted Risk
```

## 8. 验收标准

全量审计完成时必须满足：

- 产出一份审计报告，列出所有 P0-P3 发现项和无发现的维度。
- P0 必须已修复或明确阻断发布；P1 必须有 owner、计划和验证路径。
- 每个跨层契约变更都有 Rust DTO、Tauri command、前端类型、API、hook 和 mock 的同步记录。
- 每个取证 parser 或 timeline/search 修复都有 fixture、targeted test 或无法测试的明确说明。
- 文档入口、风险登记、开发记录和 remediation plan 状态一致。

## 9. 审计输出物

- `docs/full-project-audit-YYYY-MM-DD.md`：全量审计报告。
- `docs/remediation-plan-YYYY-MM-DD.md`：分阶段修复计划。
- `development-reports/sessions/YYYY-MM-DD.md`：执行记录、验证命令与剩余风险。
- 必要时更新 `docs/model-architecture-algorithm-diagrams.md`，确保图谱与实现同步。
