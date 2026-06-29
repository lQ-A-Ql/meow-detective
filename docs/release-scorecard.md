# 发布评分卡

## 1. 目标

本评分卡用于把 V2 的可信验证、关联分析、性能稳定性和安全治理收敛到一个可执行的候选发布门禁中。

## 2. 当前评分口径（2026-06）

> 权威总分：**81 / 100，等级 B**。该口径与 `README.md`、`AGENTS.md` 中的 V2 状态保持一致。
>
> 四个阶段按等权平均计算：`(95 + 85 + 70 + 75) / 4 = 81.25 ≈ 81`。

| 阶段 | 维度 | 阶段完成度 | 说明 |
|---|---|---:|---|
| V2-1 | 可信验证体系 | 95% | 核心 fixture 与真实 E01 回归覆盖较完整 |
| V2-2 | 多工件关联分析 | 85% | 关联规则与 lead 已落地，部分家族覆盖待补齐 |
| V2-3 | 性能与稳定性 | 70% | benchmark 覆盖与长任务稳定性仍在推进 |
| V2-4 | 安全治理与发布治理 | 75% | 发布门禁与治理快照已建立，部分 gate 仍为 warning |

### 评分历史

| 日期 | 总分 | 等级 | 说明 |
|---|---:|---|---|
| 2026-06-13 | 70 | C | 早期 V2-4 治理运行快照；当时 correlation-family-coverage 为 blocked |
| 2026-06 | 81 | B | 修复文档漂移、MCP Stage 5 回归守卫、lockfile 配置后重新校准的口径 |

## 3. 总分结构

总分 100 分：

- 可信验证体系：30 分
- 多工件关联分析：25 分
- 性能与稳定性：20 分
- 安全治理与发布治理：25 分

## 4. 评分项

### 4.1 可信验证体系（30）

| 子项 | 分值 | 通过标准 |
|---|---:|---|
| public-small 完整性 | 8 | 核心链路 small fixture 齐全 |
| public-medium 完整性 | 6 | 核心链路 medium fixture 齐全 |
| expected JSON 完整性 | 8 | 核心链路 expected JSON 与字段承诺齐全 |
| 真实样本回归说明 | 8 | 核心链路具备真实样本回归摘要 |

### 4.2 多工件关联分析（25）

| 子项 | 分值 | 通过标准 |
|---|---:|---|
| 统一关联模型 | 6 | node / edge / lead / provenance / confidence 一致 |
| 核心规则集 | 8 | 至少 6 类规则落地 |
| UI 联动 | 5 | timeline / artifacts / files / reports 共用同一套结果 |
| walkthrough | 6 | 至少 3 个真实案例 |

### 4.3 性能与稳定性（20）

| 子项 | 分值 | 通过标准 |
|---|---:|---|
| benchmark 记录 | 6 | small / medium / large 口径明确 |
| 性能阈值达标 | 8 | medium / large 关键指标达标 |
| 长任务稳定性 | 6 | cancel / retry / repeat / partial recovery 可验证 |

### 4.4 安全治理与发布治理（25）

| 子项 | 分值 | 通过标准 |
|---|---:|---|
| MCP 权限与审计 | 8 | 权限模型、审计记录、拒绝路径完整 |
| 导出 / 提取 / 媒体安全 | 7 | overwrite、路径、handle、脱敏齐全 |
| 防漂移与依赖治理 | 5 | 文档门禁与 dependency gate 通过 |
| 发布演练 | 5 | release candidate 完整演练一次 |

## 5. 硬门禁

以下任一失败则总评直接不合格：

- 核心链路 fixture 回归失败
- 支持矩阵与真实实现不一致
- 导出 / MCP / 媒体边界存在可复现安全绕过
- 真实样本回归无法说明验证范围与未保证字段
- 发布文档严重漂移且无豁免审批

## 6. 等级解释

- A（90-100）：可进入 V2 发布收尾
- B（80-89）：可进入候选发布，但需关闭全部 P1
- C（70-79）：仅可继续内测，不可对外宣称能力稳定
- D（<70）：继续开发，不进入候选发布

## 7. 发布材料清单

每次候选发布至少附带：

- 支持矩阵
- 已知不支持格式
- 真实样本回归摘要
- benchmark 摘要
- 安全回归摘要
- 风险登记与豁免说明

## 8. 当前产品内落地（2026-06-13）

`/v2` 治理工作台当前已经提供第一版可见发布治理链路：

- `supportMatrixEntries`
- `errorTaxonomyEntries`
- `releaseGates`
- `releaseScorecard`

当前 `releaseGates` 主要用于把候选发布的关键门禁显式展示到产品内，包括：

- 核心 fixture 回归
- 文档防漂移
- benchmark 阈值
- 安全基线
- 证据哈希完整性
- 运行时失败任务 / provenance warning / running jobs / partial jobs

这还不是完整发布系统，但已经把“发布口径”从纯文档推进成了真实 DTO + 前端面板链路。

当前实现已不是完全静态打分，`releaseGates` 和 `releaseScorecard` 已开始由以下治理快照事实派生：

- `verificationChains`
- `supportMatrix`
- `testdata/governance/v2-verification-catalog.json`
- `benchmark.scenarios`
- `benchmark.requiredChecks`
- `testdata/governance/v2-benchmark-baseline.json`
- `testdata/governance/v2-security-taxonomy.json`
- `testdata/governance/v2-known-limitations.json`
- `testdata/governance/v2-release-policy.json`
- `testdata/governance/v2-runtime-results.json`
- `security`
- `runtimeSignals`

从 2026-06-13 起，`releaseScorecard.breakdown` 的扣分文案与分值也开始由 `testdata/governance/v2-release-policy.json -> scorePolicy` 派生，而不是继续完全硬编码在 Rust 中。

当前 `/v2` 也开始显式展示 `factSources`，用于回答“当前治理面板的数据究竟来自哪一份仓库事实源、派生出哪些 DTO 字段、最近一次校验时间是什么”。

从 2026-06-13 起，`/v2` 还会直接展示 `knownLimitations`，并由 `testdata/governance/v2-known-limitations.json` 统一派生 `knownLimitations` 与 `supportMatrix.documentedLimitCount`，把 `docs/known-unsupported-formats.md` 中的关键“不承诺 / 部分支持”边界收束成结构化列表，便于 reviewer 在同一页内同时看到：

- 当前支持矩阵
- 当前运行结果
- 当前已知限制

从 2026-06-13 起，`docs-drift`、`core-fixture-regression`、`benchmark-thresholds`、`security-baseline` 四个 gate 也开始优先合并 `testdata/governance/v2-runtime-results.json`，用于表达最近一次真实治理运行结果，而不是只依赖静态治理目录。

当前治理快照还新增了 `runtimeResults`，会把最近一次运行结果以结构化列表直接返回给 `/v2` 与报告导出，便于 reviewer 直接看到：

- 哪些运行型检查被执行过
- 每项的 `status / evidence / detail / checkedAt`
- 每项下的 `subChecks` 子检查明细
- 当前 gate 是由静态快照、运行结果，还是二者合并后得出的

当前默认产品内口径示例：

- `core-fixture-regression` 会根据 E01、RAW、NTFS、Prefetch、LNK、Registry、RecycleBin 的通过状态给出 `passed / warning / blocked`
- `benchmark-thresholds` 会检查 medium / large 的必需场景是否缺失或超阈值
- `security-baseline` 会检查导出路径、防覆盖、MCP 白名单、SSE 协议限制、媒体句柄、错误脱敏、审计记录是否全部开启
- `runtime-failures` 会把 failed job、running job、partial job、provenance warning 汇总成可见门禁
- `correlation-family-coverage` 会检查：
  - 是否已生成关联快照
  - 是否存在 lead
  - covered 家族数是否达到规则家族总数的一半以上
  - 是否存在至少一个高置信家族

`releaseScorecard.breakdown` 也已开始对齐这些 gate，而不是只基于手工分值。

当前 `/v2` 的 benchmark 面板不再只展示已采集场景，还会按 required check 逐项列出：

- 场景名
- 数据集等级
- 阈值 p95
- 实测 p95
- `covered / missing / exceeded`

这样 investigator / release reviewer 可以直接看见 gate 为什么是 warning 或 blocked，而不是只看到缺失数量。

当前 `runtimeSignals` 中与关联发布口径直接相关的字段已经包括：

- `correlationSnapshotAvailable`
- `correlationLeadCount`
- `correlationHighConfidenceLeadCount`
- `correlationReviewLeadCount`
- `correlationClusterCount`
- `correlationRuleFamilyCount`
- `correlationCoveredFamilyCount`
- `correlationHighConfidenceFamilyCount`
- `correlationFamilyCoverage[]`

其中 `correlationFamilyCoverage[]` 当前按 `LNK / Prefetch / Registry / RecycleBin / BrowserDownload / BrowserHistory / Email / JumpList` 统计，并在 `/v2` 中直接展示 `covered / review / missing`。

注意：当前 `/v2` 中的 support matrix / release gates 仍属于“治理快照口径”，并不自动等价于真实 CI / benchmark 平台结果。后续必须继续把：

- `verificationChains`
- `supportMatrixEntries`
- `releaseGates`
- `releaseScorecard`

逐步替换为可由 fixture、expected JSON、benchmark 输出和安全回归结果派生的数据源，避免产品页与文档再次漂移。

当前产品内评分卡还新增了 `breakdown`，用于解释每个维度为什么得分是当前值，而不是只给一个总分。这样 investigator / release reviewer 在产品内就能直接看到：

- 满分是多少
- 实得分是多少
- 扣分项是什么

另外，当前治理快照已经开始进入真实报告导出链路：

- HTML 报告新增 `Governance Snapshot`
- JSON 导出新增 `governance`
- CSV 导出新增 `governance` 行
- HTML / CSV 现已带出 `factSources` 与 `knownLimitations` 摘要行，便于对照治理事实源与已知限制
- HTML / JSON / CSV 现在还会带出 `correlationFamilyCoverage` 对应的治理摘要行，便于候选发布材料直接审阅规则家族覆盖情况

这意味着 V2 的发布门禁与评分口径不再只停留在 `/v2` 页面里，而是已经能跟随案件导出物一起被审阅和留档。

## 9. V2-4 治理运行结果（2026-06-13）

### 9.1 安全守护脚本运行结果

| 脚本 | 结果 | 备注 |
|---|---|---|
| `check-command-sql-boundary.ps1` | PASSED | Tauri 命令层无原始 SQL |
| `check-media-protocol-guard.ps1` | PASSED | 媒体预览使用 evidence-media: 协议 |
| `check-release-guard.ps1` | PASSED | 无 debug! / 无 release profile 泄露 |
| `check-stage5-regression-guard.ps1` | PASSED | MCP 传输验证 + 嵌套 DTO 合约 + staging 冲突可见性均已锁定。修复：staging.rs 重构为 staging/mod.rs 后更新了 guard 路径和文件读取逻辑；补充了 mcp.rs 中的 `tool_call_request_documents_camel_case_boundary_is_top_level_only` 合约标记 |
| `check-doc-drift.ps1` | PASSED | 修复：v2-known-limitations.json 从 9 items 扩展至 18 items 对齐 known-unsupported-formats.md section 2；README / AGENTS 补齐了 v2-benchmark-baseline.json 引用 |

### 9.2 依赖治理

`cargo-deny` 未安装。建议在 CI 和本地发布流程中引入 `cargo deny check` 自动扫描 advisory / license / source 三类问题。当前尚未发现已知 CVE 依赖，但无自动化扫描无法保证。

### 9.3 当前评分

基于 `v2-release-policy.json` scorePolicy 派生（`testdata/governance/v2-runtime-results.json` 驱动）：

| 维度 | 满分 | 实得分 | 扣分项 |
|---|---|---|---|
| 可信验证体系 | 30 | 22 | 核心 fixture 未全量通过 -4；待哈希证据源 -4 |
| 多工件关联分析 | 25 | 11 | 核心 fixture 非全通过 -2；关联规则家族覆盖 blocked -4；provenance warning -2；关联快照无 lead -4；高质量规则家族覆盖不足半数 -2 |
| 性能与稳定性 | 20 | 15 | benchmark 覆盖未完整 -2；运行时仍有任务告警 -1；partial job -2 |
| 安全治理 | 25 | 22 | 待哈希证据源 -3 |
| **总分** | **100** | **70** | **等级：C** |

### 9.4 当前门禁状态

| Gate | 状态 | 说明 |
|---|---|---|
| core-fixture-regression | warning | E01 仍为 partial |
| docs-drift | passed | 已修复 |
| benchmark-thresholds | warning | 仍缺少 3 个必需场景 |
| security-baseline | passed | 全部控件已启用 |
| evidence-hash-completeness | warning | 存在待哈希证据源 |
| runtime-failures | warning | partial job 存在 |
| correlation-family-coverage | **blocked** | 关联快照无 lead，8 个家族均为 missing |

### 9.5 阻断项（Blockers）

1. **关联规则家族全覆盖缺失**：当前关联快照 lead_count=0，全部 8 个规则家族（LNK、Prefetch、Registry、RecycleBin、BrowserDownload、BrowserHistory、EmailMessage、JumpList）均为 missing 状态。候选发布前需至少为 LNK / Prefetch / Registry / RecycleBin 补充规则落地并生成关联快照。
2. **cargo-deny 未入 CI**：缺少自动化 advisory/license/source 扫描，无法在每次构建中拦截已知漏洞依赖。

### 9.6 残余风险（Residual Risks）

- Browser 仍处于 Beta，版本边界需要持续回归。
- Email 仍为 EML/EMLX-first，PST/OST/mbox 需留在已知限制。
- benchmark 仍未覆盖全部 medium/large 阈值场景（缺 medium 时间线筛选、large 文件树首展开、large 搜索热查询）。
- 当前案件含 provenance warning，需在 investigator 视图显式提示。
- 当前关联分析快照没有 lead，需复核规则覆盖或样本充分性。
- 当前高质量关联规则家族覆盖仍不足（Browser / Email / Registry 等链路仍需补齐）。
- FAT/exFAT、JumpList、SRU、Thumbcache 缺少 committed fixture 与 expected.json。
- Browser / Email 模块在 artifacts-windows 中尚无任何实现代码。

### 9.7 发布建议

当前总分 70（等级 C），存在 1 个 blocked gate（correlation-family-coverage）和多个 warning gates。建议：

1. 优先关闭 correlation-family-coverage blocked：为 LNK / Prefetch / Registry / RecycleBin 4 个核心家族补齐关联规则并生成有效 lead
2. 引入 `cargo-deny` 入 CI pipeline
3. 补齐 benchmark medium/large 缺失场景
4. 完成以上三项后重新评估，目标分数 >= 80（等级 B）方可进入候选发布
