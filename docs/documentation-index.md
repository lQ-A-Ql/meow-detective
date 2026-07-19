# Meow~Detective 文档入口与事实校准

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
| 文档索引与事实校准 | `docs/documentation-index.md` | 权威入口、历史归档路由、事实快照 |
| 当前进度台账 | `docs/progress-ledger.md` | 已验证里程碑、真实样本基线与下一开发边界 |
| 历史文档归档 | `docs/archive/README.md` | 按类型和月份浏览历史审计、方案、日志、状态与原型 |
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
| 外部实现对比（性能） | `docs/trace-ui-comparative-analysis.md` | 对比 `trace-ui` 与本项目在滚动、缓存、会话、数据访问层的实现差异与可借鉴点 |
| 大文件浏览优化设计 | `docs/large-file-browsing-optimization-design.md` | 本项目 100MB+ 文件浏览与预览性能优化的目标架构、阶段方案、测试矩阵与验收标准 |
| Ceph RBD VM 预览性能设计 | `docs/ceph-rbd-vm-preview-performance-design.md` | PVE 派生 VM 文件预览的 bounded-range 修复、source-scoped runtime、opaque session、缓存失效、真实样本性能门禁与剩余风险 |
| PVE 集群取证加固与能力路线图 | `docs/pve-cluster-forensics-hardening-and-capability-roadmap.md` | Catalog/processing/read-only 加固、OSDMap/CRUSH/PG、RBD 高级特性与 CephFS 的 Stage 0-7 设计、测试和验收边界 |
| PVE 派生源后台处理与性能优化 | `docs/pve-derived-source-performance-optimization.md` | Catalog 可浏览状态与昂贵后处理解耦、Timeline/Search/Artifacts 优化、后台任务边界、真实样本性能测试矩阵与剩余风险 |
| Frontend MVP boundary | `docs/frontend-mvp-boundary.md` | Page / Feature / Component / API / Platform / Store 边界、无 runtime mock、公共组件归属与守卫规则 |
| Backend module architecture | `docs/backend-module-architecture.md` | Stage 0 backend module/test split rules, baselines, guards, and exceptions |
| Backend Stage 3/4 delivery | `docs/backend-stage3-stage4-design.md` | Transport/command and app-services decomposition, review gates, regression matrix, and performance boundary |
| Backend Stage 5/6 delivery | `docs/backend-stage5-stage6-design.md` | Parser/core capability decomposition, physical test separation, review gates, and real-sample regression boundary |
| Backend Stage 7 final acceptance | `docs/backend-stage7-final-acceptance.md` | Final architecture audit, residual debt, real-sample evidence, performance results, quality score, and accepted boundaries |
| Ceph BlueStore Stage 2 | `docs/ceph-bluestore-stage2-design.md` | BlueFS superblock/layout inventory、真实 PVE oracle、失败关闭校验和后续 replay 边界 |
| Ceph BlueStore Stage 3 | `docs/ceph-bluestore-stage3-design.md` | BlueFS transaction framing、bounded metadata replay、source-local persistence 与真实 PVE oracle |
| Ceph BlueStore Stage 4 | `docs/ceph-bluestore-stage4-design.md` | BlueFS extent reader、RocksDB CURRENT/IDENTITY/MANIFEST、VersionEdit replay 与 source-local control-plane inventory |
| Ceph BlueStore Stage 5 | `docs/ceph-bluestore-stage5-design.md` | RocksDB live SST footer/checksum/properties/index、BlueFS identity 闭合、有界键空间统计与真实 PVE oracle |
| Ceph BlueStore Stage 5 real sample | `docs/real-sample-regression/2026-07-14-pve-rocksdb-stage5.md` | 六成员串行导入、35/40/33 live-SST 完整库存、代表 SST 独立 oracle 与剩余 unsupported 边界 |
| Ceph BlueStore Stage 6 | `docs/ceph-bluestore-stage6-design.md` | RocksDB WAL/latest-state、BlueStore onode/blob、RADOS/RBD、VM 文件系统重建边界与真实 PVE 门禁 |
| Ceph BlueStore Stage 6.1 real sample | `docs/real-sample-regression/2026-07-14-pve-rocksdb-stage6-wal.md` | 三 OSD WAL/WriteBatch oracle、source-local metadata 持久化、真实 fnode 语义纠偏与剩余 latest-state 边界 |
| Ceph BlueStore Stage 6.2 real sample | `docs/real-sample-regression/2026-07-14-pve-rocksdb-stage6-sst-stream.md` | 代表 live SST 的逐 block entry-stream foundation、独立 oracle、资源边界与全 live-set/latest-state 剩余边界 |
| Ceph BlueStore Stage 6.3 real sample | `docs/real-sample-regression/2026-07-14-pve-rocksdb-stage6-latest-state.md` | 三 OSD 全 live-set + active WAL latest-state 摘要、canonical digest、source-local 原子持久化与性能基线 |
| Ceph BlueStore Stage 6.4 real sample | `docs/real-sample-regression/2026-07-15-pve-bluestore-stage6-semantic.md` | 三 OSD `S/C/O/X` semantic snapshot、shared ref-map 语义、精确 count/digest oracle 与剩余 RADOS/RBD 边界 |
| Ceph BlueStore Stage 6.5/6.6 real sample | `docs/real-sample-regression/2026-07-15-pve-bluestore-stage6-rados-rbd.md` | 六成员 OMAP 无 Header 修复、RADOS/RBD foundation 回归、真实样本结果与 VM/CephFS 未完成边界 |
| Ceph RBD derived VM real sample | `docs/real-sample-regression/2026-07-16-pve-rbd-derived-vm.md` | 真实三副本 RBD 字节重建、派生 source DB、114,260 条 VM 文件记录、预览、性能与 CephFS indeterminate 边界 |
| Ceph RBD Catalog/Artifacts performance | `docs/real-sample-regression/2026-07-18-pve-rbd-catalog-artifact-performance.md` | 零派生源 Catalog 重建、Artifacts 冷重放、时间/内存对照、采用参数与持久化 frontier 剩余风险 |
| PVE cluster import rerun | `docs/real-sample-regression/2026-07-19-pve-cluster-import-rerun.md` | 六成员串行真实复跑、ready/ready_metadata 结果、BlueStore semantic oracle、CephFS 当前 indeterminate 结论 |
| CephFS 逐步重建设计 | `docs/cephfs-stepwise-reconstruction-design.md` | Presence proof、FSMap/MDSMap、metadata pool、journal、namespace、layout、bounded preview 与分阶段验收边界 |
| CI | `ci.md` | CI 流程与检查步骤 |
| 测试策略 | `test-plan.md` | 测试分层、fixture、回归与发布 gate |
| V3 主计划（历史设计记录） **(V3)** | `docs/v3-plan.md` | 保留阶段设计；其中 macOS 范围已被 Stage 1 平台边界取代，不代表当前支持 |
| 证据图设计 **(V3)** | `docs/evidence-graph-design.md` | 图模式、查询语义、索引策略、节点/边类型定义 |
| 案例笔记本设计 **(V3)** | `docs/case-notebook-design.md` | 笔记本模型、证据引用、步骤记录与重放 |
| 规则包规范 **(V3)** | `docs/rule-pack-spec.md` | 声明式规则包 TOML 格式、校验规则、共享约定 |
| PST 依赖决策 **(V3)** | `docs/pst-dependency-decision.md` | PST 解析库选型决策记录：依赖评估、安全审计、许可兼容 |
| 批处理设计 **(V3)** | `docs/batch-processing-design.md` | 离线批处理子系统架构、检查点、资源治理 |
| Linux 制品覆盖 **(V3)** | `docs/linux-artifact-coverage.md` | Linux 解析器路线图、fixture 要求、已知缺口 |
| PST/OST/mbox 支持 **(V3)** | `docs/pst-ost-mbox-support.md` | 容器邮件路线图、Outlook/Thunderbird 版本矩阵 |
| V3 演练 **(V3)** | `docs/v3-walkthrough.md` | 端到端 V3 调查工作流演练：导入、图浏览、关联、笔记本、规则包、批处理 |
| V4 执行计划（历史设计记录） **(V4)** | `docs/v4-plan.md` | 保留阶段设计；其中 APFS/HFS+ 范围已被 Stage 1 unsupported 边界取代 |

## 2. 当前事实快照

以下事实基于当前仓库静态校准得出；若代码变化，必须同步更新本文档、`README.md` 与 `AGENTS.md`。

| 事实 | 当前值 | 事实源 |
|---|---:|---|
| Rust workspace crate | 36 | `crates/`（Tauri shell 为独立 workspace package） |
| Tauri commands | 99 | `apps/desktop/src-tauri/src/commands/**/*.rs` 中 `#[tauri::command]` |
| app-services source modules | 27 | `crates/app-services/src/*.rs`，排除 `lib.rs` |
| SQLite repositories | 30 | `crates/persistence-sqlite/src/repositories/*_repo.rs` (含 datasource_cluster_repo、ceph_osd_repo、ceph_bluefs_repo、ceph_bluefs_replay_repo、ceph_rocksdb_repo、ceph_rocksdb_sst_repo、ceph_rocksdb_wal_repo、ceph_rocksdb_latest_state_repo、ceph_bluestore_semantic_repo、ceph_rbd_lineage_repo、processing_phase_repo、analysis_scan_repo、catalog_file_repo、catalog_publication_repo、filesystem_locator_repo) |
| SQLite migration scripts | 57 | `crates/persistence-sqlite/src/migrations/scripts/*.sql` (0001-0039 + source_001-source_017 + staging_001) |
| frontend pages | 10 | `frontend/src/app/pages/*.tsx`，排除测试 |
| frontend test files | 87 | `frontend/src/**/*.test.ts(x)` |
| Mermaid 图块 | 15 | `docs/model-architecture-algorithm-diagrams.md` |
| V3 参考文档 | 8 | 历史 V3 文档清单；当前支持事实以 parser matrix 为准 |
| V3 保留新增 crate | 2 | `crates/containers-pst/`, `crates/artifacts-linux/` |
| V4 参考文档 | 1 | `docs/v4-plan.md`（V4 阶段边界、测试矩阵、验收标准、评分机制） |
| V4 保留新增 crate | 4 | `crates/fs-ext4/`, `crates/fs-xfs/`, `crates/fs-btrfs/`, `crates/exchange/` |
| Rust tests | ~2,061 | `cargo test --workspace` 汇总 (2026-06 校准) |

## 3. 路径级事实校准

| 路径模式 | 数量 | 说明 |
|---|---:|---|
| `frontend/src/app/pages/*.tsx` | 10 | 页面入口文件，不含 `*.test.tsx` |
| `frontend/src/**/*.test.ts(x)` | 86 | Vitest 测试文件总数 |
| `apps/desktop/src-tauri/src/commands/**/*.rs` | 98 | Tauri command 定义数 |
| `crates/persistence-sqlite/src/migrations/scripts/*.sql` | 57 | SQLite migration 脚本 (0001-0039 + source_001-source_017 + staging_001) |
| `docs/model-architecture-algorithm-diagrams.md` 中 Mermaid | 15 | Mermaid 图块总数 |
| `docs/v3-*.md` | 1 | V3 阶段文档入口（主计划） |
| `docs/` 中 V3 参考文档 | 8 | 历史设计清单；macOS 覆盖入口已移除，当前支持事实不从历史计划派生 |
| `docs/v4-*.md` | 1 | V4 阶段文档入口（主计划） |
| `docs/` 中 V4 参考文档 | 1 | V4 执行计划（待扩展为多份参考文档） |

## 4. 当前实现事实

### 4.1 平台与通信

- Windows-primary、desktop-first、single-user
- Windows 与 Linux 是仅有的生产分析平台；平台判断和 capability 准入由后端负责
- macOS 数据源请求及旧 `platform='macos'` 案件返回 typed `Unsupported`，不做兼容迁移
- APFS/HFS+ 只允许保留已知 Apple 分区类型标识符的 metadata 识别，不承诺 filesystem magic/signature 识别，不实例化 reader，也不提供文件树、预览或制品提取
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
- 邮件提取（EML/EMLX/MBOX/PST/OST）已落地并接入验证框架，public-small + public-medium fixture 与 expected.json 已覆盖；MSG/TNEF 明确排除在 V2/V3 范围外
- 浏览器记录仍处于未实现状态
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

- V3 主计划位于 `docs/v3-plan.md`，作为历史阶段设计保留；其中 macOS 设计不再构成当前产品范围或支持声明
- V3 阶段为 V3-1(证据图基础) → V3-2(容器与跨平台覆盖) → V3-3(可复现调查与规则包) → V3-4(离线批处理与发布)
- V3 目前处于规划阶段，所有 V3 参考文档标记为“规划中”
- V3 文档入口：`docs/v3-plan.md`（主计划）、`docs/v3-walkthrough.md`（调查工作流演练）
- V3 参考文档中的平台覆盖只保留 Windows/Linux 生产语义；当前支持等级必须以 `docs/parser-support-matrix.md` 为准
- V3 参考文档（已实现）：`docs/pst-ost-mbox-support.md` — PST/OST/mbox 容器邮件解析已落地，含 `public-small` 与 `public-medium` fixture
- `docs/known-unsupported-formats.md` 明确记录 macOS 平台、旧案件和 APFS/HFS+ 内容读取均为 unsupported
- V3 治理工作台将替代 `/v2` 为 `/v3`，引入图统计、平台覆盖、规则包覆盖、批处理状态等信号

### 4.3b V4 规划与当前状态

- V4 主计划位于 `docs/v4-plan.md`，作为历史阶段设计保留；其中 APFS/HFS+ 设计不再构成当前产品范围或支持声明
- V4 阶段为 V4-1(实体规范化与合并引擎) → V4-2(多OS文件系统crate) → V4-3(AI辅助调查) → V4-4(调查交换与保管链) → V4-5(实时流式采集)
- V4 当前保留 ext4、XFS、Btrfs 三个 Linux 文件系统 crate 和 exchange 交换 crate；APFS/HFS+ reader 已退出生产边界
- V4 文档入口：`docs/v4-plan.md`（主计划）
- V4 参考文档（待创建）：`docs/v4-entity-resolution.md`、`docs/v4-multi-os-filesystems.md`、`docs/v4-ai-models.md`、`docs/v4-ai-privacy.md`、`docs/v4-release-signing.md`、`docs/v4-release-checklist.md`
- V4 将在完工后替代 V2 治理工作台为 `/v4`，引入实体解析统计、跨案关联指标、文件系统覆盖、AI使用审计、保管链验证、流式采集状态等信号

### 4.4 MCP 与安全边界

- MCP 已具备 SSE / stdio 基础校验
- 默认权限模型为最小权限：
  - `resourceAccess=readOnly`
  - `toolAccess=disabled`
  - `promptAccess=readOnly`
  - `networkPolicy=localhostOnly`
- MCP 关键动作需要审计留痕
- 导出与文件提取默认 `overwrite=false`

## 5. 历史文档归档规则

历史审计、已完成或被替代的方案、开发流水、状态快照和旧原型统一存放在 `docs/archive/`，不再堆放于 `docs/` 根目录。归档先按类型分类，再按文档日期月份归入 `YYYY-MM`；缺少明确日期时采用首次 Git 提交日期。

使用规则：

- 历史审计报告只说明“当时发现过什么”，不说明“现在仍然如此”。
- remediation plan 只说明“曾计划如何修复”，不自动等于“已经完成”。
- 开发记录只说明“执行过哪些步骤”，不替代当前权威设计和支持矩阵。
- 当前里程碑写入 `docs/progress-ledger.md`；真实样本结果继续写入 `docs/real-sample-regression/`。
- 旧链接通过 `docs/archive/path-map.md` 定位，不在旧路径恢复重复副本。
- 归档清单与分类统计以 `docs/archive/manifest.json` 为机器可读事实源。

## 6. Mermaid 渲染与防漂移

工程文档的防漂移脚本：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-doc-drift.ps1
```

归档结构、清单数量和 UTF-8 编码校验：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-doc-archive.ps1
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

归档守卫会检查：

- `docs/archive/<type>/<YYYY-MM>/` 分类与清单数量一致
- `docs/progress-ledger.md`、归档入口和路径映射存在
- `docs/` 下文档均可按严格 UTF-8 解码
- 历史审计、复审和整改计划不会重新堆回 `docs/` 根目录

## 7. 文档维护要求

以下变更必须同步更新本文档：

- crate / command / migration / test 数量变化
- 新增权威工程文档
- 完成可交付里程碑或改变下一开发边界
- 新增、移动或重新分类历史文档
- 文件浏览根模型、排序、状态字段、MCP 安全边界发生变化
- 可验证性、支持矩阵、错误分类、导出与媒体安全要求发生变化
