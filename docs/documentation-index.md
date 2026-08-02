# Meow~Detective 技术文档索引

## 1. 收录边界

Git 仅收录可长期维护的架构、领域模型、算法、协议、安全边界、解析器契约和技术选型文档。
开发日志、阶段汇报、审计过程稿、真实样本运行记录、benchmark 输出、release drill、
roadmap、runbook、walkthrough 和编辑器状态由工作站本地保存，不进入版本库。

文档保留白名单由根目录 `.gitignore` 维护；`scripts/check-doc-archive.ps1` 验证没有被忽略的
过程文档残留在 Git 索引中，并对所有保留文档执行严格 UTF-8 校验。

## 2. 架构与模型

| 主题 | 文档 | 内容 |
|---|---|---|
| 总体架构 | `docs/architecture-model.md` | Tauri、应用服务、解析器、存储和前端分层 |
| 后端模块边界 | `docs/backend-module-architecture.md` | 平台域、服务、command、parser 的依赖方向 |
| 设计约束 | `docs/design-constraints.md` | 只读证据、契约、错误、资源和模块边界 |
| 图谱合集 | `docs/model-architecture-algorithm-diagrams.md` | 架构、数据流和算法 Mermaid 图 |
| 前端 MVP | `docs/frontend-mvp-boundary.md` | Page、Feature、Component、API、Platform、Store 边界 |
| 前端状态 | `docs/frontend-state-management.md` | Zustand、TanStack Query 与后端事实源边界 |
| IPC 事件 | `docs/ipc-event-contract.md` | Tauri command/event 契约和进度语义 |
| 关联分析 | `docs/correlation-analysis-design.md` | 节点、边、聚类、线索、置信度和 provenance 模型 |
| 批处理 | `docs/batch-processing-design.md` | 离线批处理、检查点和资源治理模型 |

## 3. 文件系统、卷与集群算法

| 主题 | 文档 |
|---|---|
| BitLocker 卷层、持久化与内存恢复设计 | `docs/bitlocker-volume-layer-design.md`、`docs/bitlocker-memory-key-recovery-design.md`、`docs/bitlocker-dependency-decision.md` |
| LVM 解析层 | `docs/lvm-parsing-layer-design.md` |
| Linux 集群导入 | `docs/linux-cluster-import-design.md` |
| PVE 集群解析 | `docs/pve-cluster-parsing-design.md` |
| Ceph BlueStore/BlueFS/RocksDB | `docs/ceph-bluestore-stage2-design.md` 至 `docs/ceph-bluestore-stage6-design.md` |
| Ceph RBD VM | `docs/ceph-rbd-vm-preview-performance-design.md`、`docs/pve-derived-source-performance-optimization.md` |
| CephFS 重建 | `docs/cephfs-stepwise-reconstruction-design.md` |
| 大文件浏览 | `docs/large-file-browsing-optimization-design.md` |
| 导入调度 | `docs/import-scheduling.md` |
| 分析提取调度 | `docs/analysis-extraction-scheduling.md` |

## 4. 解析、验证与安全契约

| 主题 | 文档 |
|---|---|
| Parser 支持矩阵 | `docs/parser-support-matrix.md` |
| 已知不支持格式 | `docs/known-unsupported-formats.md` |
| PST/OST/mbox | `docs/pst-ost-mbox-support.md` |
| Expected JSON | `docs/expected-json-contract.md` |
| 验证信任模型 | `docs/validation-trust-framework.md` |
| Benchmark 模型 | `docs/benchmark-baseline.md` |
| 错误模型 | `docs/error-taxonomy.md`、`docs/error-classification-manual.md` |
| 导出与媒体安全 | `docs/export-and-media-safety.md` |
| MCP 安全模型 | `docs/mcp-security-model.md` |
| 依赖治理 | `docs/dependency-advisory-policy.md`、`docs/dependency-decisions.md` |
| EVTX 依赖决策 | `docs/evtx-dependency-decision.md` |

## 5. 外部源码研究

| 主题 | 文档 |
|---|---|
| trace-ui | `docs/trace-ui-comparative-analysis.md` |

这些文档只记录可由源码证明的架构和算法借鉴，不作为当前能力完成度或真实样本通过状态。

## 6. 当前静态事实

| 事实 | 当前值 |
|---|---:|
| Rust workspace crate | 29 |
| Tauri commands | 115 |
| app-services source modules | 28 |
| SQLite repositories | 42 logical repositories |
| SQLite migration scripts | 73 |
| frontend test files | 101 |
| Mermaid 图块 | 15 |

| 路径 | 数量 |
|---|---:|
| `frontend/src/app/pages/*.tsx` | 10 |
| `frontend/src/**/*.test.ts(x)` | 101 |
| `apps/desktop/src-tauri/src/commands/**/*.rs` | 115 |

治理事实源：

- `testdata/governance/v2-known-limitations.json`
- `testdata/governance/v2-benchmark-baseline.json`
- `testdata/governance/v2-security-taxonomy.json`
- `testdata/governance/v2-release-policy.json`

静态事实变化时同步更新本文档和 `README.md`，并运行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-doc-archive.ps1
```
