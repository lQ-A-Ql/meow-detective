# 文档归档目录

本目录保存已经完成、被替代或仅用于历史追溯的审计、方案、开发日志、状态快照与原型。归档文档不是当前实现事实源；当前设计与支持边界统一从 [`docs/documentation-index.md`](../documentation-index.md) 进入。

## 归档维度

- 第一维是类型：`audits`、`plans`、`development-logs`、`status`、`prototypes`。
- 第二维是文档日期月份：`YYYY-MM`。优先采用文档内日期；缺少日期时采用首次提交日期。
- 文件名保持原名，便于通过 Git 历史和旧引用追溯。
- 完整材料包允许在月份目录下保留自己的 `plans/reports/specs` 子结构。

## 分类统计

| 类型 | 月份 | 文档数 | 内容边界 |
|---|---:|---:|---|
| 审计与复审 | 2026-05 | 9 | 功能、安全、数据库、前端与预览历史审计 |
| 审计与复审 | 2026-06 | 17 | 架构、算法、导入、媒体、Registry 与完整审计材料包 |
| 审计与复审 | 2026-07 | 1 | Stage 6 前端运行时审计快照 |
| 方案与提案 | 2026-05 | 6 | 数据库、前端、预览与 V2 修补方案 |
| 方案与提案 | 2026-06 | 4 | 导入、工程优化、Registry 与风险整改方案 |
| 开发日志 | 2026-05 | 2 | MCP 与预览开发流水 |
| 开发日志 | 2026-06 | 3 | 导入、性能交互与旧综合开发流水 |
| 状态快照 | 2026-06 | 3 | 优化完成、暂停状态与 Stage 5 风险状态 |
| 原型 | 2026-05 | 2 | 不参与生产构建的旧 UI 原型 |

归档业务文档合计 47 份，不含本说明、`manifest.json` 和 `path-map.md`。

## 按日期浏览

- `2026-05`：早期全量功能/安全审计、数据库与前端修补方案、预览/MCP 日志及 UI 原型。
- `2026-06`：架构与算法审计、导入/媒体/Registry 专题、完整审计材料包、工程优化和风险状态。
- `2026-07`：前端 Stage 6 运行时审计快照。

## 主要材料

- [2026-05 全量功能审计](audits/2026-05/full-functional-audit-2026-05-29.md)
- [2026-05 全量安全审计](audits/2026-05/full-security-audit-2026-05-29.md)
- [2026-06 架构与算法审计](audits/2026-06/architecture-algorithm-audit-2026-06-08.md)
- [2026-06 完整审计报告](audits/2026-06/forensics-complete-audit/reports/2026-06-30-forensics-audit-report.md)
- [V2.1 历史修补方案](plans/2026-05/remediation-plan-v2.1.md)
- [旧综合开发流水](development-logs/2026-06/开发记录.md)
- [旧路径到归档路径的完整映射](path-map.md)

## 使用规则

- 新代码、验收结论和产品文案不得只引用归档文档作为当前事实依据。
- 需要恢复旧路径时查阅 [`path-map.md`](path-map.md) 并更新引用，不在根目录复制第二份内容。
- 当前开发进度写入 [`docs/progress-ledger.md`](../progress-ledger.md)，真实样本结果写入 `docs/real-sample-regression/`。
- 归档结构由 `scripts/check-doc-archive.ps1` 校验，禁止在 `docs/` 根目录重新堆放历史审计或开发日志。
