# Forensics Workbench 文档入口与事实校准

本文档是当前工程文档的权威入口，用于解决旧审计报告、阶段方案、开发记录和架构文档之间的重复、漂移与引用混乱问题。

若多份文档对同一主题存在冲突，优先级如下：
1. `AGENTS.md`
2. 本文档
3. 对应主题的当前权威文档
4. 历史审计 / 历史方案 / 开发记录

## 1. 当前权威文档

| 主题 | 权威文档 | 用途 |
|---|---|---|
| 产品范围 | `PRD.md` | 产品目标、用户、MVP 范围、非目标 |
| 技术规格 | `spec.md` | 技术原则、模块职责、数据流与安全边界 |
| 详细设计 | `design.md` | 架构细节、数据结构、阶段设计 |
| 工程化审计 | `docs/engineering-audit-plan.md` | 全量工程化审计清单与执行口径 |
| 开发工程规范 | `docs/development-engineering-guide.md` | 日常开发、契约、测试与发布约定 |
| 设计与约束 | `docs/design-constraints.md` | 架构、证据、安全、前端、发布硬约束 |
| 模型 / 架构 / 流程图 | `docs/model-architecture-algorithm-diagrams.md` | Mermaid 图谱合集 |
| 文档索引与事实校准 | `docs/documentation-index.md` | 权威入口、旧文档去重、事实快照 |
| 可验证性体系 | `docs/validation-trust-framework.md` | public fixture、expected JSON、真实样本回归说明 |
| Parser 支持矩阵 | `docs/parser-support-matrix.md` | 支持边界、验证样本、字段承诺 |
| 已知不支持格式 | `docs/known-unsupported-formats.md` | 明确不支持或仅部分支持的格式 |
| 错误分类 | `docs/error-taxonomy.md` | 错误类别、脱敏策略、前后端约定 |
| MCP 安全模型 | `docs/mcp-security-model.md` | MCP 权限模型、执行边界、审计要求 |
| 导出与媒体安全 | `docs/export-and-media-safety.md` | 导出路径、overwrite、media handle 与脱敏要求 |
| MCP 使用说明 | `docs/mcp-user-guide.md` | 面向使用者的 MCP 配置与权限说明 |
| CI | `ci.md` | CI 流程与检查步骤 |
| 测试策略 | `test-plan.md` | 测试分层、fixture、回归与发布 gate |

## 2. 当前事实快照

以下事实基于当前仓库静态校准得出；若代码变化，必须同步更新本文档、`README.md` 与 `AGENTS.md`。

| 事实 | 当前值 | 事实源 |
|---|---:|---|
| Rust workspace crate | 22 | `crates/` |
| Tauri commands | 67 | `apps/desktop/src-tauri/src/commands/**/*.rs` 中 `#[tauri::command]` |
| app-services source modules | 16 | `crates/app-services/src/*.rs`，排除 `lib.rs` |
| SQLite repositories | 9 | `crates/persistence-sqlite/src/repositories/*_repo.rs` |
| SQLite migration scripts | 23 | `crates/persistence-sqlite/src/migrations/scripts/*.sql` |
| frontend pages | 8 | `frontend/src/app/pages/*.tsx`，排除测试 |
| frontend test files | 41 | `frontend/src/**/*.test.ts(x)` |
| Mermaid 图块 | 14 | `docs/model-architecture-algorithm-diagrams.md` |

## 3. 路径级事实校准

| 路径模式 | 数量 | 说明 |
|---|---:|---|
| `frontend/src/app/pages/*.tsx` | 8 | 页面入口文件，不含 `*.test.tsx` |
| `frontend/src/**/*.test.ts(x)` | 41 | Vitest 测试文件总数 |
| `apps/desktop/src-tauri/src/commands/**/*.rs` | 67 | Tauri command 定义数 |
| `crates/persistence-sqlite/src/migrations/scripts/*.sql` | 23 | SQLite migration 脚本 |
| `docs/model-architecture-algorithm-diagrams.md` 中 Mermaid | 14 | Mermaid 图块总数 |

## 4. 当前实现事实

### 4.1 平台与通信

- Windows-primary、desktop-first、single-user
- Tauri 2 桌面应用
- 无 HTTP server
- 前后端通过 Tauri commands 与 events 通信
- `crates/transport` 是前后端契约源

### 4.2 取证与文件浏览

- 文件树与文件表共享 `showHidden`
- `deleted` / `hidden` / `system` 为真实状态字段，不是纯前端推断
- 分区显示统一为 `分区x（LABEL）`
- 列表与树的排序以“目录优先 + 状态后置 + 自然名称排序”为准

### 4.3 可验证性与支持边界

- 公开验证体系以 `testdata/fixtures/tiny/` 为默认 small fixture 来源
- `expected.json` 用于真实样本回归对齐
- 当前重点链路包括 E01、NTFS、Prefetch、LNK、Registry、Recycle Bin
- 浏览器记录与邮件提取已纳入验证框架，但仍属于低成熟度链路

### 4.4 MCP 与安全边界

- MCP 已具备 SSE / stdio 基础校验
- 默认权限模型为最小权限：
  - `resourceAccess=readOnly`
  - `toolAccess=disabled`
  - `promptAccess=readOnly`
  - `networkPolicy=localhostOnly`
- MCP 关键动作需要审计留痕
- 导出与文件提取默认 `overwrite=false`

## 5. 旧文档去重规则

以下文档保留，但默认视为历史快照或过程文档，不代表当前实现状态：

- `docs/full-project-audit-*.md`
- `docs/full-security-audit-*.md`
- `docs/architecture-algorithm-audit-*.md`
- `docs/remediation-plan-*.md`
- `docs/preview-*.md`
- `docs/frontend-optimization-*.md`
- `docs/*development-log*.md`

使用规则：

- 历史审计报告只说明“当时发现过什么”，不说明“现在仍然如此”
- remediation plan 只说明“曾计划如何修复”，不自动等于“已经完成”
- 开发记录只说明“执行过哪些步骤”，不替代当前权威设计

## 6. Mermaid 渲染与防漂移

工程文档的防漂移脚本：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-doc-drift.ps1
```

可选 Mermaid 渲染校验：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-doc-drift.ps1 -RenderMermaid
```

当前脚本会检查：

- `README.md` / `AGENTS.md` / 本文档中的事实数量是否过期
- 工程化文档入口是否缺失
- Mermaid 图块数量是否漂移
- 可选地将全部 Mermaid 图渲染为 SVG

## 7. 文档维护要求

以下变更必须同步更新本文档：

- crate / command / migration / test 数量变化
- 新增权威工程文档
- 文件浏览根模型、排序、状态字段、MCP 安全边界发生变化
- 可验证性、支持矩阵、错误分类、导出与媒体安全要求发生变化
