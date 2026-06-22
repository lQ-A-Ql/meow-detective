# Registry 模块质量维护综合方案

> 日期：2026-06-21  
> 基线：R1–R4 功能已落地，`cargo fmt/clippy/test --workspace` 全绿，`liuyang_pc.E01` 注册表提取通过  
> 目标：在功能实现后完成安全、工程、正确性、可维护性收口，使 Registry 链路达到 V2 发布口径

---

## 1. 背景与范围

本次方案是 **Registry 功能实现后的质量维护阶段** 产物。前期已完成 R1–R4 的提取能力（SYSTEM/SOFTWARE/SAM/NTUSER/USRCLASS/Amcache/SECURITY），并通过了真实 E01 样本的功能验证。现对本次 diff 进行复审，并结合 2026-06-08 架构/算法审计与工程化审计方案，输出可执行的整改计划。

**范围**：
- `crates/artifacts-windows/src/registry/*`（lookup、hash_decrypt、txlog、recovery、sam_structs）
- `crates/app-services/src/analysis_service/extraction/*`（registry 提取调度、structured summary 构建）
- `crates/transport/src/dto/{analysis,registry}.rs`
- `frontend/src/{types/models.ts, lib/api/mock-data.ts, components/analysis/AnalysisPanels.tsx}`
- `Cargo.toml`、`deny.toml`、相关文档

**不范围**：
- 新增大规模 artifact 家族（如 JumpList/SRU/Thumbcache 的完整实现）
- EVTX WEVT 模板 feature-gate（属于 H7，不在 Registry 整改内）
- 双 DB 访问模型（H14）的全局重构，但 Registry 侧不引入新的状态不一致

---

## 2. Diff 复审发现项（REG-01 ~ REG-14）

| ID | 优先级 | 标题 | 关键证据 | 影响 | 整改方向 | 验证方法 |
|---|---|---|---|---|---|---|
| REG-01 | **P0** | SAM 密码哈希默认外泄到前端/UI | `crates/transport/src/dto/analysis.rs:216-220` `SamUserAccountDto.password_hash`；`crates/app-services/src/analysis_service/extraction/registry.rs:437-443` 写入 `passwordHash`；`crates/app-services/src/analysis_service/extraction/mod.rs:224-225` 映射到 summary；`frontend/src/components/analysis/AnalysisPanels.tsx:587-590` 默认渲染；`frontend/src/lib/api/mock-data.ts:635-636` 含明文 NTLM | 违反受控披露原则，LM/NT 哈希直接出现在 UI 与 mock | 默认 DTO/artifact 中移除 hash；新增带审计的 opt-in 导出路径；UI 默认隐藏；mock 使用占位符 | grep 无默认 plaintext hash；前端 typecheck；UI 截图复核 |
| REG-02 | P1 | 新增 Registry/SAM 代码仍返回 `Result<T, String>` | `crates/artifacts-windows/src/registry/lookup/sam.rs:52` `Result<SamInfo, String>`；`hash_decrypt.rs` 全 `Option`；多数 lookup 模块沿用 `String` 错误 | 违反 AGENTS.md  typed-error 约定，跨 crate 错误语义不可控 | 在 `artifacts-windows` 引入 `RegistryError`/`SamError`，新模块先替换；旧 parser.rs 不强制迁移 | clippy 扫描新模块无 `Result<_, String>`；测试全绿 |
| REG-03 | P1 | `registry.rs` 单文件 1742 行，超模块规模约定 | `crates/app-services/src/analysis_service/extraction/registry.rs` 共 1742 行 | 维护困难，难以单测与并行开发 | 拆分为 `registry/{mod,system,software,sam,security,ntuser,usrclass,amcache,common}.rs` | 各子模块 < 300 行；`cargo test` 通过 |
| REG-04 | P1 | 加密依赖未走 workspace 集中管理 | `crates/artifacts-windows/Cargo.toml:24-27` `md-5 = "0.10"`、`des = "0.8"`、`aes = "0.8"`、`cipher = "0.4"` | 版本漂移、审计困难 | 加入 root `Cargo.toml [workspace.dependencies]` 并使用 `workspace = true` | `cargo check -p artifacts-windows`；`cargo tree` 无重复版本 |
| REG-05 | P1 | SAM 解密失败无结构化诊断 | `hash_decrypt.rs:51` `derive_hashed_boot_key` 返回 `Option`；`sam.rs:64-71` 静默 `None` | 无法区分"缺 SYSTEM""Account F 不可读""AES/RC4 不支持""校验失败" | 引入 `SamDecryptStatus` enum，每种失败生成可翻译的 warning code | 单元测试覆盖各状态；真实 E01 仍输出用户 |
| REG-06 | P1 | SYSTEM BootKey 与 SAM/SECURITY 解耦不足 | `registry.rs:209` `system_hive_bytes.and_then(extract_boot_key)`；`sam_structs.rs:24` `extract_boot_key` 返回 `Option` 无日志 | 若枚举顺序导致 SAM 先处理，哈希无法解密且无提示 | 调度器优先/缓存 SYSTEM bytes；SAM/SECURITY 处理时无 BootKey 必产生 warning；支持后补 SYSTEM 时重触发 | 单测模拟无 SYSTEM 场景；真实 E01 回归 |
| REG-07 | P1/P2 | PIDL 启发式路径覆盖有限 | `lookup/ntuser.rs` OpenSave/LastVisited MRU；`lookup/shellbags.rs` | 路径恢复不完整却未标注置信度 | DTO 增加 `path_confidence`；文档化不支持项；收集真实样本比对 | 真实 E01 非空路径数统计；合成 PIDL 单测 |
| REG-08 | P1 | `UserAssistEntryDto` 重复定义 | `crates/transport/src/dto/registry.rs:66-74`（executable/run_count/last_run/focus_time_ms） vs `crates/transport/src/dto/analysis.rs:244-255`（program_path/exec_count/last_exec_time/...） | 前端/服务易用错；契约不同步 | 重命名内部 DTO 为 `NtuserUserAssistEntryDto` 或合并字段；前端同步 | typecheck；grep 无重复同名 DTO |
| REG-09 | P1/P2 | sourceObjectId / sourceAttribution 未专项审计 | `artifact_builders.rs:22` 默认 `source_object_id = file_id`；registry 无 timeline 事件 | 用户级 artifact 无法按 SID/用户名关联 | 每 family 确认 `source_object_id`/`source_attribution`；增加 `subject_sid`/`subject_username` attr；补充 registry timeline 事件 | `correlation_service` 测试包含 registry family |
| REG-10 | P2 | `deny.toml` 新增 license 缺说明 | `deny.toml:56` `CDLA-Permissive-2.0` 直接加入 allow 列表，无 owner/reason | 依赖治理不可追溯 | 添加行内注释，说明引入依赖与复核日期；必要时在 `docs/dependency-decisions.md` 登记 | `cargo deny check licenses`；`scripts/check-deny-exceptions.ps1` |
| REG-11 | P1/P2 | txlog 未覆盖 SAM/SECURITY，summary 硬编码 txlog/deleted 状态 | `lookup/txlog_util.rs` 仅覆盖 `ParsedRegistryField`；`extraction/mod.rs:270-271` `txlog_merged: false`、`deleted_keys_found: 0` | SAM/SECURITY 无法从事务日志恢复最新值；summary 状态失真 | 对 SAM/SECURITY 在解析前尝试 txlog dirty-page 合并；从 `recovery.rs` 取 deleted count；真实值回填 DTO | 合成 LOG1/LOG2 单测；真实 E01 无 panic |
| REG-12 | P2 | Registry 生产路径仍注册旧 `parser.rs` | 2026-06-08 审计 M6：Registry 注册较弱 `parser.rs`，未切换到 `lookup.rs` | 生产路径与高质量实现分叉 | 将 `extract_registry_candidate` 设为 canonical 生产路径；`parser.rs` 降级为 fallback/废弃 | parser-support-matrix 更新；expected JSON 回归通过 |
| REG-13 | P2 | 新增 family 缺少 expected JSON / CI 回归 | roadmap 多处 expected JSON 未打勾；新 family 无 `testdata/fixtures/` | 无法阻止后续回归 | 为 R1–R4 新增 family 补充 expected JSON；接入现有 fixture diff CI | `check-doc-drift.ps1` 通过 |
| REG-14 | P2 | 警告去敏感化与数量控制不足 | `registry.rs` 多处 `warnings.push(format!(...))` 含原始路径/错误 | 损坏 hive 可能淹没 UI；可能泄漏绝对路径 | 每 extractor 警告上限与去重；敏感字段脱敏；引入 warning code | 单测验证上限与 redaction |

---

## 3. 与前期审计的关联映射

| 前期发现 | 关联 REG 项 | 说明 |
|---|---|---|
| M6：Registry 注册旧 parser.rs | REG-12 | 功能实现后必须切换到 lookup 路径 |
| A-03：Transport 契约同步 | REG-01、REG-08 | 敏感字段与 DTO 命名冲突必须收口 |
| A-08：导入正确性 | REG-06、REG-11 | BootKey 顺序与 txlog 合并影响结果保真 |
| A-12：Windows artifact parser | REG-02、REG-05、REG-07、REG-12 | 错误语义、解密诊断、PIDL 正确性 |
| A-15：事件与缓存失效 | REG-09 | sourceObjectId / timeline / correlation |
| A-18：测试覆盖 | REG-13 | expected JSON 与回归测试 |
| A-19：CI / 依赖治理 | REG-04、REG-10 | workspace 依赖与 deny 文档 |
| H14：双 DB 访问模型 | — | Registry 侧不新增独立连接状态，避免加剧 |

---

## 4. 整改阶段设计

### Q1：安全与合规收口（P0/P1，预计 1 周）

**目标**：默认情况下绝不泄露 LM/NT 哈希、LSA Secrets 与缓存凭证；依赖许可文档补齐。

| Task | 内容 | 交付物 | 验收标准 |
|---|---|---|---|
| Q1.1 | 从默认 DTO/artifact/attrs 中移除 `password_hash`/`password_hash_type` | `analysis.rs` 改型；`registry.rs` 不再写 `passwordHash`；`extraction/mod.rs` 不映射 | `grep passwordHash` 在默认路径消失；测试仍通过 |
| Q1.2 | 新增受控 SAM hash 导出路径 | 新 command/transport DTO `SamHashExportRequest`，需显式授权并写 audit log | 未授权调用返回 403/拒绝；授权调用记录审计 |
| Q1.3 | 前端默认隐藏 hash 列，opt-in 弹窗带审计说明 | `AnalysisPanels.tsx` 调整；mock-data 使用占位符 | UI 截图复核；mock-data 无真实 hash |
| Q1.4 | LSA Secrets / Cached Credentials 标记敏感并默认不进入报告 | DTO 加 `sensitive: true`；报告导出过滤 | 报告输出不含 `encrypted_blob_hex` 除非授权 |
| Q1.5 | `deny.toml` license 说明 | 行内注释 + `docs/dependency-decisions.md` 条目 | `cargo deny check licenses` 通过 |

### Q2：工程化与代码健康（P1，预计 1–1.5 周）

**目标**：符合 workspace 约定、typed error、模块规模、DTO 唯一性。

| Task | 内容 | 交付物 | 验收标准 |
|---|---|---|---|
| Q2.1 | Workspace 集中管理加密依赖 | root `Cargo.toml` 加入 md-5/des/aes/cipher；子 manifest 改 `workspace = true` | 无 literal crypto 版本；deny 通过 |
| Q2.2 | 新 registry 模块引入 typed error | `RegistryError`、`SamError` 等；替换新模块中 `Result<_, String>` | clippy 新模块无 `Result<_, String>`；测试全绿 |
| Q2.3 | 拆分 `registry.rs` | `analysis_service/extraction/registry/` 子模块 | 最大子模块 < 300 行；编译通过 |
| Q2.4 | 解决 `UserAssistEntryDto` 命名冲突 | 重命名或合并；前端 `types/models.ts` 同步 | typecheck；grep 无冲突 |
| Q2.5 | `cargo fmt/clippy/test` 与前端 lint | — | 全绿 |

### Q3：正确性与覆盖（P1/P2，预计 1.5–2 周）

**目标**：BootKey 可解释、txlog 覆盖 SAM/SECURITY、PIDL 有置信度、sourceObjectId 与 correlation 可用。

| Task | 内容 | 交付物 | 验收标准 |
|---|---|---|---|
| Q3.1 | SYSTEM BootKey 缓存与重触发 | 调度器优先收集 SYSTEM；SAM/SECURITY 无 BootKey 时 warning | 单测 + 真实 E01 |
| Q3.2 | SAM 解密结构化诊断 | `SamDecryptStatus` enum；warnings 可区分原因 | 单元测试覆盖 5+ 状态 |
| Q3.3 | txlog dirty-page 合并覆盖 SAM/SECURITY | 解析前合并 LOG1/LOG2；回填 `txlog_merged`/`deleted_keys_found` | 合成 txlog 测试；summary 状态真实 |
| Q3.4 | PIDL 路径置信度 | `OpenSaveMruEntryDto`、`LastVisitedMruEntryDto`、`ShellbagEntryDto` 加 `path_confidence` | 真实 E01 统计；文档更新 |
| Q3.5 | sourceObjectId / subject attribution 审计 | 每 family 复核；SAM/NTUSER artifact 加 `subject_sid`/`subject_username`；补充 registry timeline 事件 | correlation 测试覆盖 registry family |

### Q4：架构对齐与发布准备（P2，预计 1.5–2 周）

**目标**：lookup 成为 canonical 路径、expected JSON 回归、文档与 governance 同步。

| Task | 内容 | 交付物 | 验收标准 |
|---|---|---|---|
| Q4.1 | Registry 生产路径切换到 lookup | 注册表使用 `extract_registry_candidate`；旧 parser.rs 降级/弃用 | parser-support-matrix 更新 |
| Q4.2 | 新增 family 的 expected JSON | `testdata/fixtures/registry_*.json` 10+ 份 | `check-doc-drift.ps1` 通过 |
| Q4.3 | 文档与 governance 同步 | `AGENTS.md`、`parser-support-matrix.md`、`registry-capability-roadmap.md`、`release-scorecard.md` | 文档互相一致 |
| Q4.4 | 完整 release drill | 在 jc2 + liuyang 样本上跑通 import → analysis → report | 无 P0/P1 遗留 |

---

## 5. 测试矩阵

| 测试类型 | 目标 | 覆盖内容 | 通过标准 |
|---|---|---|---|
| Workspace 静态检查 | 全仓库 | `cargo fmt --check`、`cargo clippy -D warnings`、`git diff --check` | 全绿 |
| Rust 单元测试 | 合成 hive | 每个 lookup 模块、hash_decrypt、txlog、recovery | 全绿，新增函数覆盖率 ≥ 60% |
| TxLog 覆盖测试 | 合成 LOG1/LOG2 | SYSTEM/SOFTWARE/SAM/SECURITY 字段被覆盖 | 至少 4 个关键 family 通过 |
| 真实 E01 回归 | jc2 / liuyang | SYSTEM/SOFTWARE/SAM/NTUSER/USRCLASS/Amcache/SECURITY 非空 | 两个样本均通过 |
| Expected JSON 回归 | `testdata/fixtures/` | R1–R4 新增 family | CI fixture diff 通过 |
| 前端契约测试 | `frontend/` | `typecheck`、`test --run`、mock-data 同步 | 全绿 |
| 依赖与安全审计 | `deny.toml` | `cargo deny check advisories bans licenses sources` | 全绿 |
| 敏感数据扫描 | 仓库 | grep 明文 hash / 敏感 blob | 默认路径无命中 |
| 手动 UI 复核 | Registry 面板 | hash 列默认隐藏、LSA secrets 脱敏 | 通过 |

---

## 6. 验收标准

### 6.1 阶段验收

| 阶段 | 必须满足 |
|---|---|
| Q1 | 默认 API/UI/mock 中不再出现 LM/NT 哈希；`deny.toml` 有说明；LSA/cached 默认不导出到报告 |
| Q2 | crypto 依赖 workspace 化；新模块无 `Result<_, String>`；`registry.rs` 拆分；`UserAssistEntryDto` 冲突消除 |
| Q3 | SAM/SECURITY 处理有 BootKey warning；解密状态可解释；txlog_merged/deleted_keys_found 为真实值；correlation 包含 registry family |
| Q4 | lookup 为 canonical 路径；expected JSON 回归全绿；文档/governance 同步；release drill 通过 |

### 6.2 总体发布验收

- 所有 **P0** 修复或明确阻断发布；所有 **P1** 有 owner、计划、验证路径。
- `cargo test --workspace`、`pnpm --dir frontend test --run`、`cargo deny check` 全绿。
- 至少 2 个真实 E01 样本（jc2、liuyang）注册表提取非空且无 panic。
- `/v2` governance dashboard 中 Registry family coverage 与实现一致。

---

## 7. 风险评估与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 移除默认 hash 后影响现有演示/截图 | 中 | 提供 opt-in 授权路径；mock-data 保留占位样式 |
| typed error 改动面广 | 中 | 仅替换**新**模块；旧 parser.rs 保持不动 |
| txlog 合并 SAM/SECURITY 引入新崩溃路径 | 中 | 损坏 txlog 必须 fallback 到原 hive，异常进入 warnings |
| PIDL 路径置信度字段影响前端表格 | 低 | 仅新增可选字段，不改变现有必填字段 |
| 模块拆分导致 git blame 丢失 | 低 | 拆分通过 `git mv` + 逐步迁移，保留历史 |

---

## 8. 评估方案

| 维度 | 权重 | 评估方式 |
|---|---|---|
| 安全与合规 | 25% | 敏感字段默认不外泄、授权与审计完整、deny 通过 |
| 代码健康 | 25% | typed error、模块规模、依赖集中、DTO 唯一、clippy 全绿 |
| 正确性与覆盖 | 30% | 真实 E01 回归、txlog/解密诊断单测、correlation 覆盖 |
| 文档与 CI | 20% | expected JSON、parser-support-matrix、AGENTS、release drill |

**等级目标**：
- Q1 完成后：A（安全合规无扣分）
- Q2 完成后：A–
- Q3 完成后：A
- Q4 完成后：A（达到 V2 Registry Beta/接近 GA 口径）

---

## 9. 近期下一步

1. **等待本方案审批**（如果通过，立即进入 Q1 实施）。
2. Q1 实施顺序：REG-01 → REG-10 → Q1.5，同步更新前端 mock。
3. Q1 完成后跑一遍真实 E01 回归，确认无 hash 外泄且功能未退化。
4. 进入 Q2 工程化整改。

---

## 附录：关键文件速查

- `crates/app-services/src/analysis_service/extraction/registry.rs` — 调度器与 artifact 构建（待拆分）
- `crates/app-services/src/analysis_service/extraction/mod.rs` — structured summary 构建（hash 映射、txlog_merged 硬编码）
- `crates/artifacts-windows/src/registry/lookup/sam.rs` — SAM 用户/组提取与 hash 解密入口
- `crates/artifacts-windows/src/registry/hash_decrypt.rs` — BootKey / LM/NT 解密
- `crates/artifacts-windows/src/registry/sam_structs.rs` — BootKey 提取
- `crates/transport/src/dto/analysis.rs` — `SamUserAccountDto`、`UserAssistEntryDto`（structured summary 侧）
- `crates/transport/src/dto/registry.rs` — `NtuserInfoDto`、`UserAssistEntryDto`（NTUSER 内部侧）
- `frontend/src/components/analysis/AnalysisPanels.tsx` — SAM 用户表格渲染
- `frontend/src/lib/api/mock-data.ts` — mock registry summary
- `deny.toml` — license allow list
