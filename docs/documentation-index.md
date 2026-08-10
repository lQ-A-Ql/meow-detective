# Meow~Detective 技术文档索引

## 1. 收录边界

Git 仅收录可长期维护的支持矩阵、IPC/数据契约、安全边界、验证基线和依赖治理文档。
架构、模块设计、开发计划、算法研究、开发日志、阶段汇报、审计过程稿、真实样本运行记录、
benchmark 输出、release drill、roadmap、runbook、walkthrough 和编辑器状态由工作站本地保存，
不进入版本库。

文档保留白名单由根目录 `.gitignore` 维护；`scripts/check-doc-archive.ps1` 验证没有被忽略的
过程文档残留在 Git 索引中，并对所有保留文档执行严格 UTF-8 校验。

## 2. 契约与支持边界

| 主题 | 文档 |
|---|---|
| IPC 事件 | `docs/ipc-event-contract.md` |
| Parser 支持矩阵 | `docs/parser-support-matrix.md` |
| 已知不支持格式 | `docs/known-unsupported-formats.md` |
| PST/OST/mbox 支持契约 | `docs/pst-ost-mbox-support.md` |
| Expected JSON | `docs/expected-json-contract.md` |

## 3. 验证、安全与依赖治理

| 主题 | 文档 |
|---|---|
| 验证信任模型 | `docs/validation-trust-framework.md` |
| Benchmark 模型 | `docs/benchmark-baseline.md` |
| 错误模型 | `docs/error-taxonomy.md`、`docs/error-classification-manual.md` |
| 导出与媒体安全 | `docs/export-and-media-safety.md` |
| MCP 安全模型 | `docs/mcp-security-model.md` |
| 依赖治理 | `docs/dependency-advisory-policy.md`、`docs/dependency-decisions.md` |
| EVTX 依赖决策 | `docs/evtx-dependency-decision.md` |
| BitLocker 依赖决策 | `docs/bitlocker-dependency-decision.md` |

## 4. 当前静态事实

| 事实 | 当前值 |
|---|---:|
| Rust workspace crate | 38 |
| Tauri commands | 132 |
| app-services source modules | 36 |
| SQLite repositories | 43 logical repositories |
| SQLite migration scripts | 79 |
| frontend test files | 110 |

| 路径 | 数量 |
|---|---:|
| `frontend/src/app/pages/*.tsx` | 11 |
| `frontend/src/**/*.test.ts(x)` | 110 |
| `apps/desktop/src-tauri/src/commands/**/*.rs` | 132 |

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
