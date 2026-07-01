# 完整审计：Forensics Workbench 代码质量与架构审计

## [S1] 目标与范围

对 `Forensics Workbench` 项目进行一次综合审计，覆盖前后端代码质量、架构/算法、模型、功能实现逻辑。最终产出一份结构化 Markdown 报告，明确亮点、风险与改进优先级。

- **标准覆盖**：项目整体健康、后端代码质量、前端代码质量、架构与数据流、功能实现逻辑。
- **重点深入**：
  - 搜索与目录索引（`crates/search`、`crates/catalog`、前端 Search 页面）
  - 时间线生成（`crates/timeline`、sourceObjectId 关联、前端 Timeline 页面）
  - 取证总览（后端 `artifacts-core` / `artifacts-windows` 等 + 前端 CaseOverview / V3Dashboard / V3ScoreCards）

## [S2] 审计方法

1. **文档与源码阅读**：PRD、spec、design、AGENTS、v2/v3/v4/v5 计划、治理 JSON。
2. **结构扫描**：Cargo workspace、frontend 目录、crate 依赖图、命令/事件/DTO 清单。
3. **代码质量检查**：运行不依赖链接器的门禁（fmt、typecheck、lint、select PowerShell 检查）；观察 clippy/test 是否存在阻塞风险（因需要 VS 环境，不强制全量运行）。
4. **重点模块走查**：对三个重点模块进行源码级阅读，关注接口边界、数据流、错误处理、并发与可测试性。
5. **报告编写**：按章节整理结论、量化指标、风险清单与改进建议。

## [S3] 输出报告结构

- **执行摘要**：总体评分、关键结论、Top 5 优先级改进项。
- **项目健康度**：结构、依赖、文档、门禁、已知限制。
- **后端代码质量**：类型化错误、unsafe、死码、模块大小、命令边界、SQL 边界、仓库模式。
- **前端代码质量**：TypeScript 严格性、状态管理、API 层、组件边界、测试覆盖。
- **架构与数据流**：IPC 契约、事件系统、数据流方向、分层清晰度。
- **搜索与目录索引**（重点深入）：设计、算法、可扩展性、风险。
- **时间线生成**（重点深入）：事件来源、MACB 投影、sourceObjectId 关联、聚合逻辑。
- **取证总览**（重点深入）：artifact 提取框架、解析器实现、前端展示耦合度。
- **改进建议**：按优先级排序（P0/P1/P2），给出可执行动作。

## [S4] 约束与假设

- 不修改代码；只生成报告。
- 不跑需要 VS 链接器的全量 `cargo test` / `cargo clippy`；但会报告已执行的门禁结果。
- 重点模块深入 1-2 层源码，不做逐行全文件走查。

## [S5] 完成标准

- `docs/compose/reports/YYYY-MM-DD-forensics-audit-report.md` 已生成。
- 报告覆盖 S3 中所有章节。
- 已运行并通过门禁：`cargo fmt --check`、`pnpm typecheck`、`pnpm lint`、至少 3 个 PowerShell 检查脚本。
