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
| V2 长期执行计划 **(V2)** | `docs/v2-longterm-plan.md` | V2 阶段边界、测试矩阵、验收标准、评分机制 |
| 可验证性体系 **(V2)** | `docs/validation-trust-framework.md` | public fixture、expected JSON、真实样本回归说明 |
| Fixture 手册 **(V2)** | `docs/fixture-handbook.md` | fixture 分层、目录规范、元数据要求 |
| Expected JSON 契约 **(V2)** | `docs/expected-json-contract.md` | expected JSON 结构、字段分级、差异规则 |
| Parser 支持矩阵 | `docs/parser-support-matrix.md` | 支持边界、验证样本、字段承诺 |
| 已知不支持格式 | `docs/known-unsupported-formats.md` | 明确不支持或仅部分支持的格式 |
| 错误分类 | `docs/error-taxonomy.md` | 错误类别、脱敏策略、前后端约定 |
| 错误分类手册 **(V2)** | `docs/error-classification-manual.md` | V2 错误分层、脱敏与审计实施口径 |
| Benchmark 基线 **(V2)** | `docs/benchmark-baseline.md` | 数据集分级、指标口径、默认阈值 |
| 关联分析设计 **(V2)** | `docs/correlation-analysis-design.md` | 关联模型、规则集、前端工作流 |
| 发布评分卡 **(V2)** | `docs/release-scorecard.md` | 候选发布评分、硬门禁、发布材料 |
| 真实样本回归说明 **(V2)** | `docs/real-sample-regression/README.md` | 真实 E01 样本回归流程、判定规则、结果记录 |
| MCP 安全模型 | `docs/mcp-security-model.md` | MCP 权限模型、执行边界、审计要求 |
| 导出与媒体安全 | `docs/export-and-media-safety.md` | 导出路径、overwrite、media handle 与脱敏要求 |
| MCP 使用说明 | `docs/mcp-user-guide.md` | 面向使用者的 MCP 配置与权限说明 |
| CI | `ci.md` | CI 流程与检查步骤 |
| 测试策略 | `test-plan.md` | 测试分层、fixture、回归与发布 gate |
| V3 主计划 **(V3)** | `docs/v3-plan.md` | V3 五支柱：证据图、跨平台覆盖、可复现调查、规则包、离线批处理 |
| 证据图设计 **(V3)** | `docs/evidence-graph-design.md` | 图模式、查询语义、索引策略、节点/边类型定义（规划中） |
| 案例笔记本设计 **(V3)** | `docs/case-notebook-design.md` | 笔记本模型、证据引用、步骤记录与重放（规划中） |
| 规则包规范 **(V3)** | `docs/rule-pack-spec.md` | 声明式规则包 TOML 格式、校验规则、共享约定（规划中） |
| PST 依赖决策 **(V3)** | `docs/pst-dependency-decision.md` | PST 解析库选型决策记录：依赖评估、安全审计、许可兼容（规划中） |
| 批处理设计 **(V3)** | `docs/batch-processing-design.md` | 离线批处理子系统架构、检查点、资源治理（规划中） |
| Linux 制品覆盖 **(V3)** | `docs/linux-artifact-coverage.md` | Linux 解析器路线图、fixture 要求、已知缺口（规划中） |
| macOS 制品覆盖 **(V3)** | `docs/mac-artifact-coverage.md` | macOS 解析器路线图、fixture 要求、已知缺口（规划中） |
| PST/OST/mbox 支持 **(V3)** | `docs/pst-ost-mbox-support.md` | 容器邮件路线图、Outlook/Thunderbird 版本矩阵（规划中） |
| V3 演练 **(V3)** | `docs/v3-walkthrough.md` | 端到端 V3 调查工作流演练：导入、图浏览、关联、笔记本、规则包、批处理 |

## 2. 当前事实快照

以下事实基于当前仓库静态校准得出；若代码变化，必须同步更新本文档、`README.md` 与 `AGENTS.md`。

| 事实 | 当前值 | 事实源 |
|---|---:|---|
| Rust workspace crate | 22 | `crates/` |
| Tauri commands | 73 | `apps/desktop/src-tauri/src/commands/**/*.rs` 中 `#[tauri::command]` |
| app-services source modules | 18 | `crates/app-services/src/*.rs`，排除 `lib.rs` |
| SQLite repositories | 9 | `crates/persistence-sqlite/src/repositories/*_repo.rs` |
| SQLite migration scripts | 23 | `crates/persistence-sqlite/src/migrations/scripts/*.sql` |
| frontend pages | 9 | `frontend/src/app/pages/*.tsx`，排除测试 |
| frontend test files | 42 | `frontend/src/**/*.test.ts(x)` |
| Mermaid 图块 | 15 | `docs/model-architecture-algorithm-diagrams.md` |
| V3 参考文档（已规划） | 9 | `docs/v3-plan.md` 及 8 篇 V3 参考文档 |
| V3 计划新增 crate | 3 | `crates/containers-pst/`, `crates/artifacts-linux/`, `crates/artifacts-macos/` |

## 3. 路径级事实校准

| 路径模式 | 数量 | 说明 |
|---|---:|---|
| `frontend/src/app/pages/*.tsx` | 9 | 页面入口文件，不含 `*.test.tsx` |
| `frontend/src/**/*.test.ts(x)` | 42 | Vitest 测试文件总数 |
| `apps/desktop/src-tauri/src/commands/**/*.rs` | 72 | Tauri command 定义数 |
| `crates/persistence-sqlite/src/migrations/scripts/*.sql` | 23 | SQLite migration 脚本 |
| `docs/model-architecture-algorithm-diagrams.md` 中 Mermaid | 15 | Mermaid 图块总数 |
| `docs/v3-*.md` | 1 | V3 阶段文档入口（主计划） |
| `docs/` 中 V3 参考文档 | 8 | 证据图、笔记本、规则包、PST决策、批处理、Linux/macOS覆盖、PST支持 |

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

- 公开验证体系以 `testdata/fixtures/public-small/` 为默认 small fixture 来源
- `expected.json` 用于真实样本回归对齐
- 当前重点链路包括 E01、NTFS、Prefetch、LNK、Registry、Recycle Bin
- 浏览器记录与邮件提取已纳入验证框架，但仍属于低成熟度链路
- V2 权威主计划位于 `docs/v2-longterm-plan.md`
- V2 的 fixture、expected JSON、benchmark、关联分析、评分卡均已有独立专题文档
- V2 治理工作台已经接入产品页 `/v2`
- V2 关联分析首版已落地 `get_correlation_snapshot` 与 `CorrelationWorkspace`
- V2 关联分析当前已把规则家族覆盖下沉到 `CorrelationSnapshot.familyCoverage[]`，工作台不再只能绕经治理快照读取家族状态
- V2 治理工作台已接住支持矩阵明细与错误分类明细：`supportMatrixEntries`、`errorTaxonomyEntries`
- V2 治理工作台已接住结构化已知限制：`knownLimitations`
- V2 治理工作台已接住候选发布门禁明细：`releaseGates`
- V2 治理快照当前已改为“仓库内治理目录 + 单一链路目录派生 verificationChains / supportMatrixEntries / supportMatrixSummary”，当前事实源为 `testdata/governance/v2-verification-catalog.json`
- V2 benchmark 基线当前事实源为 `testdata/governance/v2-benchmark-baseline.json`
- V2 security defaults 与 error taxonomy 当前事实源为 `testdata/governance/v2-security-taxonomy.json`
- V2 release policy 当前事实源为 `testdata/governance/v2-release-policy.json`
- V2 已知限制当前事实源为 `testdata/governance/v2-known-limitations.json`
- V2 `supportMatrix.documentedLimitCount` 当前由 `testdata/governance/v2-known-limitations.json` 派生
- V2 评分卡分值与扣分文案当前事实源为 `testdata/governance/v2-release-policy.json -> scorePolicy`
- V2 治理快照当前已显式返回 `factSources`，用于列出每个治理区域的事实文件、派生输出与最近校验时间
- V2 文档漂移 / fixture 回归 / benchmark 阈值的最近一次运行结果当前事实源为 `testdata/governance/v2-runtime-results.json`
- V2 治理快照当前已显式返回 `runtimeResults`，用于列出最近一次运行型检查的结构化结果
- V2 治理快照当前已把 `runtimeResults.checks[].subChecks[]` 细化为子检查级明细，并在 `/v2` 与报告导出中展示
- V2 治理工作台当前已开始按治理快照事实派生 `core-fixture-regression`、`benchmark-thresholds`、`security-baseline`、`runtime-failures`，不再只展示静态门禁名称
- V2 治理快照当前已进入报告导出链路：HTML `Governance Snapshot`、JSON `governance`、CSV `governance` 行
- V2 关联分析已接住首批真实规则字段：`target_path`、`targetPath`、`url`、`title`、`visitTime`、`data`、`original_path`、`executable`、`attachments`、`subject`、`sentAt`
- 报告导出已开始复用关联分析结果，HTML/JSON/CSV 三种导出格式已具备首版关联摘要，其中 HTML 已具备结构化 `Correlation Lead Details`

### 4.3a V3 规划与当前状态

- V3 主计划位于 `docs/v3-plan.md`，五支柱为：证据图(Evidence Graph)、容器与跨平台覆盖、可复现调查(Notebook+Step Replay)、规则包系统(Rule Pack)、离线批处理(Batch Processing)
- V3 阶段为 V3-1(证据图基础) → V3-2(容器与跨平台覆盖) → V3-3(可复现调查与规则包) → V3-4(离线批处理与发布)
- V3 目前处于规划阶段，所有 V3 参考文档标记为“规划中”
- V3 文档入口：`docs/v3-plan.md`（主计划）、`docs/v3-walkthrough.md`（调查工作流演练）
- V3 参考文档（规划中）：`docs/evidence-graph-design.md`、`docs/case-notebook-design.md`、`docs/rule-pack-spec.md`、`docs/pst-dependency-decision.md`、`docs/batch-processing-design.md`、`docs/linux-artifact-coverage.md`、`docs/mac-artifact-coverage.md`、`docs/pst-ost-mbox-support.md`
- V3 将在 `docs/parser-support-matrix.md` 中新增 Linux/macOS 解析器条目，在 `docs/known-unsupported-formats.md` 中新增 Linux/macOS 文件系统与移动/云制品缺口
- V3 治理工作台将替代 `/v2` 为 `/v3`，引入图统计、平台覆盖、规则包覆盖、批处理状态等信号

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
- 若需要查看当前长期执行修复路线与 V2 方向，优先参考 `docs/v2-longterm-plan.md`
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
