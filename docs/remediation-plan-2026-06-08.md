# Forensics Workbench 51 项中/高风险修复方案

## 背景与目标

需要针对 `docs/architecture-algorithm-audit-2026-06-08.md` 中已识别的 51 个中/高风险项，制定一份可执行的修复方案。目标不是一次性“全改完”，而是把这些风险按取证正确性、攻击面、数据一致性、可维护性和回归成本进行分阶段治理，形成可落地的 stage design、phase tasks、测试矩阵、验收标准和评分机制。

本方案优先遵循现有架构：Rust backend-led 分层、`crates/transport` 作为 IPC 契约、`app-services` 负责编排、`persistence-sqlite` 承担 SQL、桌面命令层保持薄适配器。实现时应尽量复用现有 guard scripts、fixture、repo/service 分层与导入/投影测试模式，而不是引入新的平行框架。

## 推荐方案

### 总体策略

采用 **5-stage / 10-phase** 的治理路线，先修“取证结果可能错误或丢失”的问题，再收敛“远程执行/死锁/运行时安全”问题，随后补“解析器格式正确性与搜索正确性”，最后做架构清债与前端一致性收口。

阶段排序原则：
1. **证据保真优先**：任何可能造成漏检、误检、静默丢数据的问题优先。
2. **攻击面优先**：MCP 任意命令/URL、锁跨阻塞 I/O、runtime 滥建等安全/稳定性问题次优先。
3. **格式正确性优先**：SRU/Prefetch/exFAT/EVTX 等解析正确性在核心数据链路稳定后修复。
4. **架构统一优先**：双 DB 模型、死迁移、双实现、双事件路径最后统一，避免在前序阶段反复返工。
5. **每阶段必须可独立验收**：每个 stage 都要有明确退出条件、回归集合和评分门槛。

---

### Stage 1 — 取证保真与导入正确性

#### 目标
消除“静默丢数据”“碎片化元数据漏读”“文件系统路径不可达”这类会直接破坏案件结果可信度的问题。

#### 覆盖风险
- 高：H1, H2, H3, H4
- 中：M1, M2, M4, M5

#### Phase 1.1 — staging merge 正确性
- 修改 `crates/app-services/src/staging.rs`
- 将 `INSERT OR IGNORE` 的关键合并路径改为：
  - 显式冲突检测
  - 记录冲突计数/样本
  - 将“跳过”提升为 import summary / audit event / job telemetry
- 若业务上允许幂等，则定义“允许重复导入但不丢失更新”的规则：按稳定业务键 upsert，而不是静默 ignore
- 为文件、timeline、artifact、search staging 合并分别定义冲突语义

#### Phase 1.2 — NTFS / exFAT / FAT 正确性
- 修改 `crates/fs-ntfs/src/mft_scanner.rs`
- 修改 `crates/app-services/src/file_service/mod.rs`
- bulk MFT scanner 改为沿 `$MFT` data runs 读取，不再假设单 run 连续
- exFAT 接入点补齐：
  - `crates/app-services/src/datasource_service.rs`
  - 相关 evidence/fs 分发逻辑
- 在 `crates/fs-exfat` 中落实 `NoFatChain` 的连续簇读取路径
- 在 `crates/fs-fat/src/lib.rs` 增加 cluster chain 环检测与上限
- 若 GPT 分区 entry 数未校验，增加边界和容量保护
- E01 若已有校验元数据可读，先补“暴露校验结果/告警”，完整强校验可后置为增量项

#### Stage 1 关键文件
- `crates/app-services/src/staging.rs`
- `crates/fs-ntfs/src/mft_scanner.rs`
- `crates/app-services/src/file_service/mod.rs`
- `crates/app-services/src/datasource_service.rs`
- `crates/fs-exfat/src/lib.rs`
- `crates/fs-exfat/src/dir.rs`
- `crates/fs-fat/src/lib.rs`
- `crates/image-e01/src/lib.rs`

#### Stage 1 验收标准
- 任意重复/冲突导入不允许无告警静默丢文件
- 碎片化 `$MFT` fixture 能完整枚举记录
- exFAT 镜像路径能被识别、枚举、读取
- FAT 恶意链不会死循环或无限增长内存
- 相关 import job 结果包含明确的冲突/降级统计

---

### Stage 2 — 取消语义、事务边界与 DB 一致性

#### 目标
防止取消后继续写库、长事务不可打断、双连接模型导致状态漂移。

#### 覆盖风险
- 高：H10, H11, H12, H13, H14
- 中：M17, M18

#### Phase 2.1 — 任务取消与 join 语义统一
- 修改桌面任务管理与 import pipeline：
  - `apps/desktop/src-tauri/src/.../task_manager.rs`
  - `apps/desktop/src-tauri/src/commands/import/pipeline.rs`
  - `crates/app-services/src/enumeration.rs`
  - 相关 import / analysis worker 协调点
- 从“仅 AtomicBool 协作取消”升级为：
  - cancel signal
  - worker drain/abort
  - join / wait group
  - 明确的 terminal state（cancelled/aborted/partial-failed）
- 长事务枚举改为可分批提交，插入取消检查点

#### Phase 2.2 — persistence schema 与访问模型收敛
- 梳理 `ActiveCase` 与 `AppState` 对同一 `app.db` 的访问模式
- 统一为单一 case-scoped pool 或单一连接模型，禁止双真相源
- 清理分区三重表示：
  - 保留真实使用的 schema
  - 为 0013/0014 制定 retire/repair migration
  - 补 migration state 修复脚本与兼容路径
- 复核 0016 rebuild migration 的 FK 策略：
  - 明确外键开关时机
  - 数据复制与校验步骤
  - rollback 路径

#### Stage 2 关键文件
- `app_services::active_case` 对应文件
- `apps/desktop/src-tauri/src/state/app_state.rs`
- `apps/desktop/src-tauri/src/commands/import/pipeline.rs`
- `crates/app-services/src/enumeration.rs`
- `crates/persistence-sqlite/src/migrations/runner.rs`
- `crates/persistence-sqlite/src/migrations/*`
- `crates/persistence-sqlite/src/*repo.rs`

#### Stage 2 验收标准
- cancel 后后台线程在限定时间内全部退出并停止写库
- 大目录逻辑枚举可中断，数据库不残留半状态事务
- 同一 case 的所有后端访问经单一策略完成
- migration log 不再永久 pending，历史/新库都能稳定升级

---

### Stage 3 — MCP 安全与运行时模型重构

#### 目标
把 MCP 从“可工作但高风险”状态提升为“默认安全、可观测、可维护”。

#### 覆盖风险
- 高：H15, H16, H17
- 中：M30, M31, M32

#### Phase 3.1 — runtime / lock / lifecycle 重构
- 修改：
  - `apps/desktop/src-tauri/src/commands/mcp_commands.rs`
  - `crates/mcp-client/src/stdio.rs`
  - `crates/mcp-client/src/...` transport/client 管理模块
- 建立单例或 case/app scoped Tokio runtime，不再每次命令新建 runtime
- 去掉“Mutex 持有跨阻塞 I/O”模式，改为：
  - 先复制必要状态
  - 锁外执行 I/O
  - 通过 actor/channel 或 async task 管理连接生命周期
- 明确 server session 生命周期：connect / initialize / capability cache / close

#### Phase 3.2 — 安全闸门与配置加载
- 为 stdio command、args、cwd、URL/scheme 建立 allowlist/validation
- 把配置加载、存储、能力协商结果纳入 `AppState`
- transport DTO 与前端 settings UI 对齐，避免 capabilities 被丢弃
- 为所有拒绝/校验失败路径输出结构化错误，而非裸字符串

#### Stage 3 关键文件
- `apps/desktop/src-tauri/src/commands/mcp_commands.rs`
- `apps/desktop/src-tauri/src/state/app_state.rs`
- `crates/mcp-client/src/stdio.rs`
- `crates/mcp-client/src/*`
- `crates/transport/src/dto/mcp.rs`
- `frontend/src/lib/api/*mcp*`
- `frontend/src/features/*mcp*`
- `frontend/src/types/models.ts`

#### Stage 3 验收标准
- 单次会话中不再反复创建独立 runtime
- 无任何锁跨阻塞 I/O
- 未授权命令、非法 URL、未知 scheme 被拒绝且返回结构化错误
- MCP 配置能被读取、展示、保存、应用
- capabilities 从后端到前端可完整观察

---

### Stage 4 — Artifact / Search / Projection 正确性

#### 目标
修复解析器格式错误、搜索重复与覆盖不足、双投影潜在分叉。

#### 覆盖风险
- 高：H5, H6, H7, H8, H9
- 中：M6, M10, M11, M13, M15, M33, M34

#### Phase 4.1 — Artifact parser correctness
- SRU：由 SQLite 路径改为 ESE/JET 正确解析策略；若短期无法完整实现，至少在 unsupported 时明确失败，不输出伪结果
- Prefetch：增加 MAM 解压支持，再进入现有 parser
- Registry：将主路径注册到更可靠的 `lookup.rs` 能力，保留旧 parser 仅作 fallback/实验
- EVTX：重新评估 `evtx-patched` feature gate；若默认关闭有产品原因，至少在文档、设置和输出中明确能力边界

#### Phase 4.2 — Search / Timeline / Frontend consistency
- Search writer 引入稳定 document key、`delete_term`/upsert 或等价 dedup 语义
- 避免每次查询/索引反复 create-on-every-call；引入 writer/index 生命周期管理
- 文本抽取与 snippet 高亮补边界测试
- timeline 双投影路径统一为单一主实现，另一条退役或仅保留测试 oracle
- 前端事件失效策略改为 key-aware invalidation，减少全局抖动

#### Stage 4 关键文件
- `crates/artifacts-windows/src/sru/mod.rs`
- `crates/artifacts-windows/src/prefetch/parser.rs`
- `crates/artifacts-windows/src/registry/*`
- `crates/evtx-patched/*`
- `crates/search/src/indexer/tantivy_writer.rs`
- `crates/search/src/extractor/text_extractor.rs`
- `crates/search/src/highlighter/mod.rs`
- `crates/app-services/src/search_service.rs`
- `crates/app-services/src/timeline_service.rs`
- `frontend/src/features/*/hooks.ts`

#### Stage 4 验收标准
- SRU 不再把非 SQLite 数据当 SQLite 解析
- 现代 Prefetch 样本可正确解压并产出结构化结果
- 重复导入不会在搜索索引中重复累计文档
- timeline 单一事实来源清晰且回归一致
- 前端 query invalidation 仅影响对应域/键

---

### Stage 5 — 技术债清理、可观测性与收口

#### 目标
清理死代码/双实现，补足指标、文档、评分与发布门槛，避免“修复后再次退化”。

#### 覆盖风险
- 中：剩余未关闭项
- 低：与本轮改动相邻、低成本可顺手关闭的项

#### Phase 5.1 — retire dead/forked paths
- 评估并清理：
  - `crates/catalog` 若确认无消费者则降级/隔离/移除入口
  - `crates/ingest` 与生产路径双实现的收敛策略
  - `streaming.rs`、search stubs、legacy event path、未使用 DTO/迁移辅助模块
- 在文档中明确哪些为 roadmap、哪些为 dead code、哪些为 internal-only

#### Phase 5.2 — observability / release gating / docs
- 为 import/search/mcp/artifact 增加结构化 counters 与 result summary
- 将关键验证纳入现有脚本或新增 CI gate
- 更新：
  - `CLAUDE.md`
  - `AGENTS.md`
  - 相关 docs/architecture 文档
  - 风险台账/修复对照表

#### Stage 5 关键文件
- `crates/catalog/**`
- `crates/ingest/**`
- `crates/app-services/src/streaming.rs`
- `docs/**`
- `scripts/**`
- `CLAUDE.md`
- `AGENTS.md`

#### Stage 5 验收标准
- 死代码/双实现状态在代码与文档中一致
- 所有高风险项都有关闭、降级或明确延期说明
- 新 guard / CI gate 能阻止已修复问题回归

---

## Phase Tasks 明细

### Phase A — 风险台账标准化
- 建立 51 项 risk register
- 字段：ID、标题、严重度、影响面、触发条件、复现方式、owner、stage、phase、状态、证据链接、验收条目
- 输出一份“风险 → 代码路径 → 测试 → 验收”的追踪矩阵

### Phase B — Fixture 与回归资产补齐
- 新增/整理最小 fixture：
  - 碎片化 MFT 样本
  - exFAT NoFatChain 样本
  - FAT 环链损坏样本
  - 非 SQLite SRU 样本
  - 压缩 Prefetch(MAM) 样本
  - MCP 非法 command / URL 配置样本
- 统一放入 `testdata/fixtures/` 并在 `test-plan.md` 记录用途

### Phase C — 设计评审与接口冻结
- 在每个 Stage 开始前冻结：
  - DTO 变化
  - 事件主题变化
  - migration 策略
  - 失败/降级语义
- 对 transport / migration / MCP 安全策略做一次专项设计评审

### Phase D — 分阶段实施
- 每个 stage 单独分支/PR
- 每个 PR 附：风险编号、测试矩阵、回归截图/日志、未解问题列表

### Phase E — 端到端验收与评分
- 按评分机制打分
- 未达标 stage 不进入下一 stage，除非明确记录例外并获批准

---

## 测试矩阵

### 1. 单元测试
- 文件系统读取：
  - NTFS data runs / fragmented MFT
  - exFAT `NoFatChain`
  - FAT cycle detection / max chain bound
  - E01 section/chunk/integrity metadata
- Search：
  - dedup / upsert 语义
  - text extractor binary-vs-text 判定
  - snippet offset / 多字节字符
- MCP：
  - command allowlist
  - URL validation
  - config loading
  - capability retention
- Artifact：
  - SRU unsupported / correct parser dispatch
  - Prefetch MAM decompression
  - Registry parser routing

### 2. 集成测试
- import pipeline：
  - staging→main merge 冲突场景
  - cancel midway 后不继续写库
  - exFAT image import end-to-end
- persistence：
  - migration from old DB → current schema
  - 0013/0014 retire/repair path
  - FK-safe rebuild flow
- MCP：
  - desktop command → mcp-client → transport DTO → frontend normalization
- search/timeline：
  - repeated import 后 search 去重
  - timeline 主路径投影一致性

### 3. 属性/鲁棒性测试
- 针对 FAT/exFAT/NTFS run/chain/entry 解析做 fuzz/边界测试
- 对 search highlighter、path normalization、URL validation 做 property-style inputs
- 对 migration runner 做半失败回滚测试

### 4. 回归脚本 / 仓库 guard
- `powershell -File scripts/check-command-sql-boundary.ps1`
- `powershell -File scripts/check-media-protocol-guard.ps1`
- `powershell -File scripts/check-release-guard.ps1`
- `powershell -File scripts/run-coverage.ps1 -Rust`
- 如有必要新增：
  - `scripts/check-mcp-command-allowlist.ps1`
  - `scripts/check-risk-regression.ps1`

### 5. 端到端场景测试
- RAW/E01 → detect partition → NTFS/exFAT/FAT enumerate → import → search → timeline → artifacts → report
- 取消导入、重复导入、损坏样本导入、MCP 设置保存与连接失败路径
- 前端 mock/tauri 双模式下关键页面回归：Files / Search / Timeline / Artifacts / Settings(MCP)

### 6. 非功能测试
- 性能：导入吞吐、内存上限、search rebuild 时间、MCP 连接建立时延
- 稳定性：重复 cancel/retry、重复 open/close case、重复 import 同一 evidence
- 安全：非法 MCP command/URL、路径穿越样本、损坏镜像/损坏 EVTX/损坏 registry hive

---

## 验收标准

### 全局硬性标准
1. **所有高风险项** 必须达到以下三态之一：
   - Closed：代码修复 + 测试通过 + 文档更新
   - Mitigated：有明确保护/降级/告警，且不会静默产生错误结果
   - Deferred with owner：有明确阻塞原因、时间点、风险隔离措施
2. 不允许新增无测试的 schema / transport / command 安全改动。
3. 所有修复必须附至少一个可复现的“修复前失败、修复后通过”的证据。
4. 不允许以“隐藏错误”代替“修复错误”；unsupported 必须显式报错或降级。
5. 文档、测试、实现的行为说明必须一致。

### Stage 级通过标准
- **Stage 1**：0 个 P0/P1 数据保真缺陷残留；导入冲突有观测；关键 fixture 全覆盖
- **Stage 2**：取消后无残留写库；migration 路径稳定；DB 访问模型单一
- **Stage 3**：MCP 默认拒绝危险输入；运行时/锁模型通过专项审查
- **Stage 4**：artifact/search/timeline 的核心正确性回归全绿
- **Stage 5**：技术债状态透明；新增 CI/guard 生效；修复闭环文档完成

### 发布前最终标准
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --dir frontend test`
- `pnpm --dir frontend typecheck`
- 相关 PowerShell guard scripts 全部通过
- 新增 fixture 和风险台账均已纳入仓库文档

---

## 评分机制

采用 **100 分制 + 强制门槛**。

### A. 风险关闭质量（40 分）
- 高风险每项：2 分
- 中风险每项：0.4 分
- 关闭标准：修复 + 测试 + 文档
- 仅 Mitigated：得 50%
- Deferred：0 分
- 若存在“静默错误仍未消除”的高风险项，本维度封顶 20 分

### B. 测试覆盖与证明力（20 分）
- 单元覆盖：8 分
- 集成覆盖：6 分
- 端到端/fixture 证明：6 分
- 若缺少“修复前失败/修复后通过”证据，最多 10 分

### C. 架构一致性（15 分）
- 是否复用现有 repo/service/transport 分层：5 分
- 是否消除双实现/双真相源：5 分
- 是否避免把业务逻辑推回 command layer / frontend：5 分

### D. 安全与可观测性（15 分）
- MCP / path / parser / import 风险防护：10 分
- telemetry / audit / structured error / counters：5 分

### E. 发布准备度（10 分）
- 文档、脚本、迁移、回归流程齐全：5 分
- CI/guard 可阻止回归：5 分

### 强制门槛（不满足则总评直接 Fail）
- 任一高风险项仍处于“已知可复现且无保护”的状态
- 取消后仍可能继续写库
- MCP 仍允许未校验的任意命令或 URL
- 修复引入新的 schema/transport 不兼容且无迁移策略
- 回归测试未覆盖实际修复点

### 评级解释
- **90–100**：可发布，且具备后续扩展基础
- **75–89**：可合并，发布前需清尾项
- **60–74**：仅适合内部验证，不建议发布
- **<60**：方案未达标，需要重做分阶段设计

## 建议执行顺序与 PR 切分

1. PR1: Stage 1 / merge + MFT + exFAT/FAT correctness
2. PR2: Stage 2 / cancellation + DB model + migration cleanup
3. PR3: Stage 3 / MCP runtime + security gates
4. PR4: Stage 4 / artifact parser correctness
5. PR5: Stage 4 / search + timeline + frontend invalidation
6. PR6: Stage 5 / dead code retirement + docs + CI guards

每个 PR 必须在说明中列出：
- 对应风险编号
- 修改文件列表
- 新增/更新 fixture
- 测试矩阵命中项
- 验收结果与得分

## 验证

实施阶段应至少执行以下验证：
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm --dir frontend typecheck`
- `pnpm --dir frontend test`
- `powershell -File scripts/check-command-sql-boundary.ps1`
- `powershell -File scripts/check-media-protocol-guard.ps1`
- `powershell -File scripts/check-release-guard.ps1`
- `powershell -File scripts/run-coverage.ps1 -Rust`
- 按 stage 运行对应 fixture 的导入、搜索、时间线、artifact、MCP 端到端回归
