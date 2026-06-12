# Forensics Workbench 文档索引与事实校准

本文档是当前工程文档的权威入口。它用于解决旧审计报告、阶段计划、开发记录和架构文档之间的重复与漂移问题。若文档之间存在状态冲突，优先按本文档的“权威入口”和“当前事实快照”解释。

## 1. 当前权威入口

| 主题 | 权威文档 | 用途 |
|---|---|---|
| 产品范围 | `PRD.md` | 产品目标、用户、MVP 范围、非目标 |
| 技术规格 | `spec.md` | 技术原则、服务职责、数据流、持久化和安全约束 |
| 详细设计 | `design.md` | 架构细节、核心算法、DTO、测试和 CI 设计 |
| 工程化审计 | `docs/engineering-audit-plan.md` | 可执行全量审计清单、风险分级、证据采集和验收标准 |
| 开发工程规范 | `docs/development-engineering-guide.md` | feature 开发流程、Rust / frontend / Tauri / transport / event / test 约定 |
| 设计约束 | `docs/design-constraints.md` | desktop-first、backend-led、证据只读、路径安全、性能和发布约束 |
| 图谱 | `docs/model-architecture-algorithm-diagrams.md` | Mermaid 架构、模型、IPC / event 和核心算法流程图 |
| CI | `ci.md` 与 `.github/workflows/` | CI 设计与当前落地工作流 |
| 测试 | `test-plan.md` | 测试分层、fixture 策略、回归和发布 gate |
| 安全 | `SECURITY.md`, `docs/security-audit-*.md`, `docs/full-security-audit-*.md` | 安全策略、历史发现和修复参考 |
| 当前修复计划 | 最新 `docs/remediation-plan-*.md` | 分阶段治理计划和剩余风险 |
| 开发记录 | `docs/开发记录.md`, `development-reports/sessions/` | 实际执行、验证命令、剩余风险 |

## 2. 当前事实快照

本快照基于 2026-06-11 的本地源码静态校准：

| 事实 | 当前值 | 事实来源 |
|---|---:|---|
| Rust workspace crate | 22 | root `Cargo.toml` / `crates/` |
| 前端页面 | 8 | `frontend/src/app/pages/*.tsx`，排除测试文件 |
| Tauri commands | 67 | `#[tauri::command]` occurrences under `apps/desktop/src-tauri/src/commands` |
| app-services source modules | 20 | `crates/app-services/src/*.rs`，排除 `lib.rs` |
| SQLite repositories | 9 | `crates/persistence-sqlite/src/repositories/*_repo.rs` |
| SQLite migration scripts | 23 | `crates/persistence-sqlite/src/migrations/scripts/*.sql` |
| 前端测试文件 | 41 | `frontend/src/**/*.test.ts(x)` |
| Mermaid 图块 | 14 | `docs/model-architecture-algorithm-diagrams.md` |

校准命令：

```powershell
(Get-ChildItem crates -Directory | Measure-Object).Count
(rg -n "#\[tauri::command\]" apps/desktop/src-tauri/src/commands | Measure-Object).Count
(Get-ChildItem crates\app-services\src -Filter *.rs | Where-Object { $_.Name -ne 'lib.rs' } | Measure-Object).Count
(Get-ChildItem crates\persistence-sqlite\src\repositories -Filter *_repo.rs | Measure-Object).Count
(Get-ChildItem crates\persistence-sqlite\src\migrations\scripts -Filter *.sql | Measure-Object).Count
(Get-ChildItem frontend\src\app\pages -Filter *.tsx | Where-Object { $_.Name -notlike '*.test.tsx' } | Measure-Object).Count
(Get-ChildItem frontend\src -Recurse -Include *.test.ts,*.test.tsx | Measure-Object).Count
rg -n "```mermaid" docs/model-architecture-algorithm-diagrams.md
```

### 2.1 当前链路事实补充

- 分区可见根继续使用 placeholder root 模型，内部路径编码为 `__partition_placeholder__/{partition_index}/{status}`。
- staging merge 以 `partition_index` 绑定 placeholder，不再依赖显示名；真实文件系统根标记节点 `\`、`/`、`.` 会在 merge 时折叠到分区根下。
- 读取侧仍保留防御性归一化，避免历史残留的裸根直接暴露到文件树首层。
- 文件列表真实排序已经下沉到后端：目录优先、状态后置、主字段排序、自然名兜底，并在排序后再分页。
- 文件树目录排序也由后端统一执行，保持自然名称升序和懒加载稳定性。
- 前端真实 Tauri 模式会透传 `showHidden`、`sortKey`、`sortDirection`，不再对返回的 rows 做第二次排序。
- `deleted`、`hidden`、`system` 已经成为跨导入、持久化、transport、排序、过滤和图标叠加的共享状态字段。

## 3. 旧文档去重规则

旧文档不删除，因为它们保留了历史审计证据、设计取舍和阶段验证记录。但阅读时应按以下规则去重：

- **历史审计报告**：如 `docs/full-project-audit-2026-06-01.md`、`docs/full-security-audit-2026-05-29.md`、`docs/architecture-algorithm-audit-2026-06-08.md`，视为审计时点快照，不代表当前实现状态。
- **修复计划**：同一主题存在多个 `remediation-plan` 时，以日期最新且在 `docs/开发记录.md` 中被后续验证引用的版本为准。
- **开发记录**：用于确认某项修复是否实际执行和验证；若与设计文档冲突，开发记录只证明执行历史，不自动修改设计目标。
- **专项提案**：如 preview、frontend optimization、database remediation 类文档，视为方案来源或 backlog，不自动代表当前功能完成。
- **Autopsy / Sleuth Kit 分析**：用于借鉴和参考，不是本项目当前能力清单。

## 4. 文档维护约束

- README 和 AGENTS 里的数量型事实必须能由 `scripts/check-doc-drift.ps1` 复算。
- 新增 Tauri command、migration、frontend test、crate 或 Mermaid 图块后，必须同步更新本文档和入口文档。
- 修改分区根模型、排序契约、`showHidden`、状态字段传播、架构、事件或主算法流程时，必须同步更新 `docs/model-architecture-algorithm-diagrams.md`。
- 修复 `P0 / P1` 风险后，必须更新对应 remediation plan 和开发记录。
- 不要在旧审计报告中直接改写历史结论；应新增复核说明或在本文档中标注当前权威状态。

## 5. Mermaid 渲染验证

2026-06-11 本地验证结果：

- 从 `docs/model-architecture-algorithm-diagrams.md` 抽取 14 个 Mermaid 图块。
- 使用 `npx --yes @mermaid-js/mermaid-cli@11.4.2` 渲染为 SVG。
- 本机使用 Microsoft Edge 作为 Puppeteer executablePath。
- 14 个 SVG 均成功生成。

推荐复验命令可见 `scripts/check-doc-drift.ps1`。若本机没有 Chrome / Edge，脚本会降级为语法块数量和文档事实检查；需要完整渲染时安装 Chrome、Edge 或 Puppeteer headless shell。

## 6. 自动化防漂移

本仓库提供：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-doc-drift.ps1
```

该脚本检查：

- README / AGENTS / 本文档中的硬编码事实是否与当前源码一致。
- `docs/model-architecture-algorithm-diagrams.md` 是否仍包含 14 个 Mermaid 图块。
- 工程化文档入口是否存在。
- 可选 `-RenderMermaid` 模式是否能把全部 Mermaid 图渲染为 SVG。
