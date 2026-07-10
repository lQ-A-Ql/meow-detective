# Forensics Workbench — Registry 模块质量审计报告

> 归档：2026-06 审计快照，仅用于历史追溯，不代表当前 Registry 能力。

> 审计日期：2026-06-21  
> 审计范围：`crates/artifacts-windows/src/registry/*`、`crates/app-services/src/analysis_service/extraction/*`、相关 Transport DTO、前端 Registry 面板、依赖与文档  
> 审计基线：R1–R4 功能已实现，`cargo fmt/clippy/test --workspace` 全绿，`liuyang_pc.E01` 注册表提取通过  
> 关联报告：`docs/architecture-algorithm-audit-2026-06-08.md`、`docs/engineering-audit-plan.md`、`docs/registry-quality-remediation-plan.md`

---

## 1. 执行摘要

Registry 模块在 2026-06-08 全量审计后经历了 R1–R4 四阶段能力建设，现已实现 SYSTEM/SOFTWARE/SAM/NTUSER/USRCLASS/Amcache.hve/SECURITY 的提取，并完成了 SAM LM/NT 哈希解密、USBSTOR/USB、MountedDevices、ShimCache、NetworkList、ShellBags、MuiCache、OpenSave/LastVisited/RunMRU、LSA Secrets 元数据、缓存域凭证元数据等能力。功能验证已在 `liuyang_pc.E01` 样本上通过。

但本次 diff 复审发现，功能落地过程中引入了 **1 个 P0 安全问题**、**8 个 P1 工程/正确性问题** 和 **5 个 P2 可维护性问题**。其中最严重的是 **SAM 密码哈希默认通过 DTO、artifact 属性、前端表格和 mock 数据外泄**，直接违反取证工具对敏感数据的受控披露原则。

本报告将 14 项 Registry 专项发现与 2026-06-08 全量审计中的 H/M/A 项关联，给出风险矩阵、整改路线和验收标准。

---

## 2. 审计范围与方法

### 2.1 范围

| 维度 | 具体对象 |
|---|---|
| Registry 解析器 | `crates/artifacts-windows/src/registry/lookup/*.rs`、`hash_decrypt.rs`、`sam_structs.rs`、`txlog.rs`、`recovery.rs`、`parser.rs` |
| 提取调度 | `crates/app-services/src/analysis_service/extraction/registry.rs`、`mod.rs` |
| 解析器注册 | `crates/app-services/src/artifact_service.rs` |
| Artifact 构建 | `crates/app-services/src/analysis_service/artifact_builders.rs` |
| Transport 契约 | `crates/transport/src/dto/analysis.rs`、`crates/transport/src/dto/registry.rs` |
| 前端 | `frontend/src/types/models.ts`、`frontend/src/lib/api/mock-data.ts`、`frontend/src/components/analysis/AnalysisPanels.tsx` |
| 依赖与治理 | `Cargo.toml`、`crates/artifacts-windows/Cargo.toml`、`deny.toml` |
| 文档 | `docs/parser-support-matrix.md`、`docs/registry-capability-roadmap.md`、`AGENTS.md` |

### 2.2 方法

1. **代码走查**：逐文件阅读新增/修改代码，记录 `file:line` 锚点。
2. **契约对账**：对照 Rust DTO、前端 `types/models.ts`、mock-data、页面消费。
3. **真实样本回归**：在 `liuyang_pc.E01` 上运行 Registry 提取测试。
4. **静态检查**：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`。
5. **依赖审计**：`cargo deny check licenses`、`cargo tree`。
6. **与前期审计关联**：将 REG 项映射到 2026-06-08 的 H/M/A 编号。

---

## 3. 基线状态

### 3.1 已实现能力（R1–R4）

| Hive | 能力 |
|---|---|
| SYSTEM | ComputerName、TimeZone、Network Adapter、Services/Drivers、USBSTOR/USB、MountedDevices、ShutdownTime、ShimCache、LSA Packages |
| SOFTWARE | ProductName/Build/Version/InstallDate/Owner、Installed Software、HKLM Run/RunOnce/RunOnceEx、Winlogon、NetworkList Profiles/Signatures、AppCompatFlags Layers |
| SAM | 本地用户/组/RID/SID、组成员、登录统计、密码策略、LM/NT 哈希解密 |
| NTUSER | Run/RunOnce、RecentDocs、UserAssist、TypedURLs、WordWheelQuery、MountPoints2、OpenSavePidlMRU、LastVisitedPidlMRU、RunMRU、默认浏览器 |
| USRCLASS | ShellBags、MuiCache |
| Amcache.hve | InventoryApplication、InventoryApplicationFile |
| SECURITY | 本地安全策略、LSA Secrets 元数据（encrypted blob hex，不解密）、缓存域凭证元数据（encrypted blob hex，不解密） |

### 3.2 已通过门禁

- `cargo fmt --all -- --check` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- `cargo test --workspace` ✅
- `pnpm --dir frontend typecheck` ✅
- `pnpm --dir frontend test --run` ✅（228 项 Vitest 测试）
- `liuyang_pc.E01` Registry 提取输出非空 ✅

### 3.3 已知未收口项

- 部分 R1–R4 能力缺少 `testdata/fixtures/` expected JSON。
- `docs/parser-support-matrix.md` 已更新，但 `AGENTS.md`、`release-scorecard.md` 仍需同步。
- txlog dirty-page 合并与 deleted key 恢复未在 structured summary 中真实反映。

---

## 4. 整体结论

Registry 模块在**功能完整性**上已达到 V2 预期，但在**安全默认、工程化、正确性可追溯、架构对齐**四个方面存在明显短板。若不整改，存在以下发布风险：

1. **P0 安全阻断**：SAM 哈希默认外泄，不符合取证工具合规要求。
2. **P1 质量阻断**：typed error、模块规模、DTO 冲突、依赖管理、txlog 覆盖、BootKey 诊断等问题会降低长期维护能力与结果可信度。
3. **P2 发布风险**：旧 `parser.rs` 仍注册在生产路径、expected JSON 缺失、文档未同步，会影响发布评分卡与 CI 回归门禁。

**建议**：按 `docs/registry-quality-remediation-plan.md` 的 Q1→Q4 阶段执行整改，Q1 必须优先完成。

---

## 5. 详细发现项

### 5.1 安全与合规类

#### REG-01（P0）SAM 密码哈希默认外泄到前端、UI 与 mock 数据

- **证据**：
  - `crates/transport/src/dto/analysis.rs:216-220`：`SamUserAccountDto` 包含 `password_hash: Option<String>` 和 `password_hash_type: Option<String>`。
  - `crates/app-services/src/analysis_service/extraction/registry.rs:437-443`：将 `passwordHash` 写入 artifact attrs。
  - `crates/app-services/src/analysis_service/extraction/mod.rs:224-225`：从 artifact attrs 映射到 structured summary 的 `password_hash`。
  - `frontend/src/components/analysis/AnalysisPanels.tsx:587-590`：默认渲染 `passwordHash` 列，且使用 `select-all text-[#b42318]` 高亮。
  - `frontend/src/lib/api/mock-data.ts:635-636`：包含明文 NTLM 哈希 `aad3b435b51404eeaad3b435b51404ee:31d6cfe0d16ae931b73c59d7e0c089c0` 与 `8846f7eaee8fb117ad06bdd830b7586c`。
- **影响**：本地账户 LM/NT 哈希在默认 API 响应、前端表格和 mock 数据中直接可见，违反受控披露原则，存在合规与证据链污染风险。
- **根因**：实现 SAM 解密能力时，未在 DTO/artifact/前端层设置"默认不输出"闸门。
- **建议**：
  1. 从 `SamUserAccountDto` 默认字段中移除 `password_hash`/`password_hash_type`。
  2. 后端新增受控导出路径（如需授权 + audit log），不在默认 summary 返回。
  3. 前端默认隐藏 hash 列，opt-in 时弹窗提示并记录审计。
  4. mock-data 使用占位符或 `null`。
  5. 报告导出默认过滤敏感字段。
- **验证**：`grep passwordHash` 在默认路径消失；前端 typecheck；UI 手动复核。

---

### 5.2 工程化类

#### REG-02（P1）新增 Registry/SAM 代码仍返回 `Result<T, String>`

- **证据**：
  - `crates/artifacts-windows/src/registry/lookup/sam.rs:52`：`pub fn extract_sam_fields(...) -> Result<SamInfo, String>`。
  - `crates/artifacts-windows/src/registry/hash_decrypt.rs`：全部函数返回 `Option<T>`，错误信息丢失。
  - 多个 lookup 模块（`system.rs`、`software.rs`、`ntuser.rs`、`security.rs` 等）沿用 `String` 错误。
- **影响**：违反 `AGENTS.md` 中"新 crate 必须使用 typed errors"的约定，跨 crate 错误语义不可控，难以做分类脱敏与国际化。
- **建议**：在 `artifacts-windows` 引入 `RegistryError`/`SamError`/`HashDecryptError` 等 typed error；新模块先替换，旧 `parser.rs` 保持不动以避免范围蔓延。
- **验证**：clippy 扫描新模块无 `Result<_, String>`；测试全绿。

#### REG-03（P1）`registry.rs` 单文件 1742 行，超出模块规模约定

- **证据**：
  - `crates/app-services/src/analysis_service/extraction/registry.rs` 共 1742 行，包含 30+ 个 artifact builder 函数，覆盖 SYSTEM/SOFTWARE/SAM/SECURITY/NTUSER/USRCLASS/Amcache 全部 hive。
- **影响**：维护困难、单测成本高、并行开发冲突风险大。
- **建议**：拆分为 `analysis_service/extraction/registry/{mod,system,software,sam,security,ntuser,usrclass,amcache,common}.rs`，每个子模块职责单一。
- **验证**：最大子模块 < 300 行；`cargo test` 通过。

#### REG-04（P1）加密依赖未走 workspace 集中管理

- **证据**：
  - `crates/artifacts-windows/Cargo.toml:24-27`：
    ```toml
    md-5 = "0.10"
    des = "0.8"
    aes = "0.8"
    cipher = "0.4"
    ```
- **影响**：版本漂移、审计困难、与 workspace 约定不一致。
- **建议**：加入 root `Cargo.toml [workspace.dependencies]`，子 manifest 改为 `workspace = true`。
- **验证**：`cargo check -p artifacts-windows`；`cargo tree` 无重复版本；`cargo deny check` 通过。

#### REG-08（P1）`UserAssistEntryDto` 重复定义

- **证据**：
  - `crates/transport/src/dto/registry.rs:66-74`：`UserAssistEntryDto` 字段为 `executable`、`run_count`、`last_run`、`focus_time_ms`。
  - `crates/transport/src/dto/analysis.rs:244-255`：`UserAssistEntryDto` 字段为 `program_path`、`exec_count`、`last_exec_time`、`is_suspicious`、`suspicious_reason`。
  - `frontend/src/types/models.ts:294-301` 仅定义了 analysis 侧 shape。
- **影响**：命名冲突易导致服务/前端用错契约；内部 NTUSER DTO 与 structured summary DTO 未对齐。
- **建议**：将 registry 内部 DTO 重命名为 `NtuserUserAssistEntryDto`（或合并字段），同步 `NtuserInfoDto` 与前端类型。
- **验证**：typecheck；grep 无重复同名 DTO。

---

### 5.3 正确性类

#### REG-05（P1）SAM 解密失败无结构化诊断

- **证据**：
  - `crates/artifacts-windows/src/registry/hash_decrypt.rs:51`：`derive_hashed_boot_key(...)` 返回 `Option<[u8; 32]>`。
  - `crates/artifacts-windows/src/registry/lookup/sam.rs:64-71`：当 BootKey 推导失败时直接 `None`，不产生 warning。
  - `decrypt_user_hashes` 同样返回 `Option<SamHashes>`，无法区分"BootKey 缺失""Account F 不可读""AES/RC4 不支持""校验失败"。
- **影响**：调查员无法判断是系统未提供 SYSTEM hive 还是解密算法失败，导致结果不可解释。
- **建议**：引入 `SamDecryptStatus` enum，将每种失败原因映射为可翻译的 warning code。
- **验证**：单元测试覆盖 5+ 种状态；真实 E01 在无 SYSTEM 时仍输出用户并附带诊断 warning。

#### REG-06（P1）SYSTEM BootKey 与 SAM/SECURITY 解耦不足

- **证据**：
  - `crates/app-services/src/analysis_service/extraction/registry.rs:209`：`let boot_key = system_hive_bytes.and_then(artifacts_windows::extract_boot_key);`。
  - `crates/artifacts-windows/src/registry/sam_structs.rs:24`：`extract_boot_key(system_hive: &[u8]) -> Option<[u8; 16]>` 失败时静默返回 `None`。
- **影响**：虽然当前调度器会预加载 SYSTEM bytes（`extraction/mod.rs:48-80`），但如果未来调用路径变化或 SYSTEM hive 损坏，SAM/SECURITY 解密失败将无迹可寻。
- **建议**：
  1. 调度器确保 SYSTEM bytes 先于 SAM/SECURITY 可用。
  2. `extract_boot_key` 返回 `Result<[u8; 16], BootKeyError>` 并携带失败原因。
  3. SAM/SECURITY 无 BootKey 时强制生成 warning。
- **验证**：单测模拟 SYSTEM 缺失/损坏场景；真实 E01 回归。

#### REG-07（P1/P2）PIDL 启发式路径覆盖有限

- **证据**：
  - `crates/artifacts-windows/src/registry/lookup/ntuser.rs`：OpenSavePidlMRU / LastVisitedPidlMRU 通过 PIDL 启发式恢复路径。
  - `crates/artifacts-windows/src/registry/lookup/shellbags.rs`：ShellBags 同样依赖 PIDL 解析。
- **影响**：复杂 PIDL 结构下路径恢复不完整，但当前 DTO 未标注置信度，易导致过度解读。
- **建议**：
  1. `OpenSaveMruEntryDto`、`LastVisitedMruEntryDto`、`ShellbagEntryDto` 增加 `path_confidence: high/medium/low`。
  2. 文档化 PIDL 解析边界与已知不支持项。
  3. 收集真实样本输出与 ForensicsTool/其他解析器做 diff，逐步补齐常见 item type。
- **验证**：真实 E01 非空路径数统计；合成 PIDL 单元测试。

#### REG-09（P1/P2）`sourceObjectId` / `sourceAttribution` 未专项审计

- **证据**：
  - `crates/app-services/src/analysis_service/artifact_builders.rs:22`：`make_artifact` 默认设置 `source_object_id: Some(candidate.file_id.clone())`、`source_attribution: Some(candidate.path.clone())`。
  - `crates/app-services/src/analysis_service/extraction/registry.rs` 未显式设置 `source_object_id`，依赖默认值。
  - Registry 提取当前不生成 `TimelineEvent`。
- **影响**：
  - 默认 source_object_id = file_id 对 file↔artifact 关联足够，但用户级 artifact（SAM/NTUSER）缺少 `subject_sid`/`subject_username`，限制 cross-artifact correlation。
  - 缺少 registry 派生的 timeline 事件，时间线无法直接反映服务启动变更、用户登录、关机等。
- **建议**：
  1. 每 family 复核 `source_object_id`/`source_attribution`。
  2. SAM/NTUSER artifact attrs 增加 `subjectSid`/`subjectUsername`。
  3. 为 ShutdownTime、服务 StartType、用户 LastLogin 等补充 `TimelineEvent`，`source_object_id` 仍用 file_id。
- **验证**：`correlation_service` 测试包含 Registry family；timeline 查询可见 registry 事件。

#### REG-11（P1/P2）txlog 未覆盖 SAM/SECURITY，summary 硬编码 txlog/deleted 状态

- **证据**：
  - `crates/artifacts-windows/src/registry/lookup/txlog_util.rs` 仅对 `ParsedRegistryField` 做 txlog 覆盖，不适用于 SAM/SECURITY 的二进制结构读取。
  - `crates/app-services/src/analysis_service/extraction/mod.rs:270-271`：`RegistryHiveOverviewDto.txlog_merged: false`、`deleted_keys_found: 0` 为硬编码。
- **影响**：SAM/SECURITY 无法从事务日志恢复最新值；summary 状态与真实处理情况不符。
- **建议**：
  1. 对 SAM/SECURITY 在解析前尝试 txlog dirty-page 合并（复用 `crates/artifacts-windows/src/registry/txlog.rs`）。
  2. 从 `recovery.rs` 获取每个 hive 的 deleted key/value 计数。
  3. 回填 `txlog_merged`/`deleted_keys_found` 真实值。
- **验证**：合成 LOG1/LOG2 单测；真实 E01 无 panic；summary 状态非全 false/0。

---

### 5.4 架构、CI 与文档类

#### REG-10（P2）`deny.toml` 新增 license 缺说明

- **证据**：
  - `deny.toml:56`：`CDLA-Permissive-2.0` 直接加入 `[licenses].allow`，无注释说明来源与复核日期。
- **影响**：依赖治理不可追溯；与 `deny.toml` 头部"例外必须包含 owner/reason/expiry"的精神不符。
- **建议**：添加行内注释说明引入依赖与复核日期；必要时在 `docs/dependency-decisions.md` 登记。
- **验证**：`cargo deny check licenses`；`scripts/check-deny-exceptions.ps1`。

#### REG-12（P2）Registry 生产路径仍注册旧 `parser.rs`

- **证据**：
  - `crates/artifacts-windows/src/registry/parser.rs:16`：定义 `pub struct RegistryExtractor`，仅解析 base block 与 root key name。
  - `crates/artifacts-windows/src/lib.rs:53`：`pub use registry::parser::RegistryExtractor;`。
  - `crates/app-services/src/artifact_service.rs:34`：`registry.register(Box::new(artifacts_windows::RegistryExtractor));`。
  - 高质量的 lookup 路径（`extract_registry_candidate`）仅在 `analysis_service` 中使用，未注册到 `ExtractorRegistry`。
- **影响**：生产路径与高质量实现分叉；旧 parser.rs 可能成为遗留路径误导开发者。
- **建议**：
  1. 将 `extract_registry_candidate` 包装为 `ArtifactExtractor` 实现并注册到 `ExtractorRegistry`，或明确 `analysis_service` 为 canonical 路径。
  2. 将 `parser.rs` 降级为 fallback/测试辅助，或标记为 deprecated。
- **验证**：`parser-support-matrix.md` 更新；expected JSON 回归通过；旧 parser.rs 测试不破坏。

#### REG-13（P2）新增 family 缺少 expected JSON / CI 回归

- **证据**：
  - `docs/registry-capability-roadmap.md` 中 R1/R2/R3 多处"expected JSON"未打勾。
  - 新增 artifact family（RegistrySystemService、RegistryUsbDevice、RegistryMountedDevice、RegistryShutdownTime、RegistryShimCache、RegistryNetworkProfile、RegistryShellbag、RegistryAmcache、RegistryLsaSecret、RegistryCachedCredential 等）缺少 `testdata/fixtures/` 期望输出。
- **影响**：CI 无法阻止后续 parser 回归；发布评分卡缺乏证据。
- **建议**：为每个新增 family 补充 expected JSON，并接入现有 fixture diff CI（`scripts/check-doc-drift.ps1`）。
- **验证**：`check-doc-drift.ps1` 通过。

#### REG-14（P2）警告去敏感化与数量控制不足

- **证据**：
  - `crates/app-services/src/analysis_service/extraction/registry.rs` 中大量使用 `outcome.warnings.push(format!("{} ...", candidate.path, err))`，err 可能来自底层并包含原始字节或绝对路径。
- **影响**：损坏 hive 可能产生大量警告淹没 UI；某些 warning 可能泄漏绝对路径或原始二进制片段。
- **建议**：
  1. 每个 extractor 设置 warning 上限（如 20 条）并去重。
  2. 对包含文件路径的 warning 做脱敏（保留 hive 相对路径，去掉主机绝对前缀）。
  3. 引入结构化 warning code，便于前端分类展示。
- **验证**：单元测试验证上限与 redaction。

---

## 6. 与 2026-06-08 全量审计的关联

| 前期发现 | 严重度 | 关联 REG 项 | 说明 |
|---|---|---|---|
| M6：Registry 注册旧 parser.rs | 中 | REG-12 | 功能实现后必须切换到 lookup 路径 |
| A-03：Transport 契约同步 | P1 | REG-01、REG-08 | 敏感字段与 DTO 命名冲突必须收口 |
| A-08：导入正确性 | P0/P1 | REG-06、REG-11 | BootKey 顺序与 txlog 合并影响结果保真 |
| A-12：Windows artifact parser | P0/P1 | REG-02、REG-05、REG-07、REG-12 | 错误语义、解密诊断、PIDL 正确性 |
| A-15：事件与缓存失效 | P1 | REG-09 | sourceObjectId / timeline / correlation |
| A-18：测试覆盖 | P1/P2 | REG-13 | expected JSON 与回归测试 |
| A-19：CI / 依赖治理 | P1/P2 | REG-04、REG-10 | workspace 依赖与 deny 文档 |
| H14：双 DB 访问模型 | 高 | — | Registry 侧不新增独立连接状态，避免加剧 |
| H7：EVTX WEVT 模板 feature-gate | 高 | — | 不在 Registry 整改范围，但应确认 Registry 不依赖该 feature |

---

## 7. 风险矩阵

| 风险 | 可能性 | 影响 | 等级 | 缓解措施 |
|---|---|---|---|---|
| SAM 哈希默认外泄被用于合规审计 | 高 | 极高 | **P0** | Q1 立即移除默认输出，增加受控导出 |
| typed error 不一致导致错误分类错误 | 中 | 高 | P1 | Q2 引入 RegistryError 系列 |
| `registry.rs` 单文件过大导致维护失控 | 高 | 中 | P1 | Q2 拆分模块 |
| SAM/SECURITY 无 txlog 覆盖导致证据陈旧 | 中 | 高 | P1 | Q3 接入 dirty-page 合并 |
| BootKey 获取失败无提示 | 中 | 高 | P1 | Q3 结构化诊断 |
| PIDL 路径恢复不完整被误读 | 高 | 中 | P1/P2 | Q3 增加 path_confidence |
| 旧 parser.rs 与新 lookup 路径分叉 | 中 | 中 | P2 | Q4 统一注册 |
| expected JSON 缺失导致回归失守 | 高 | 中 | P2 | Q4 补齐 fixtures |

---

## 8. 整改方案摘要

完整执行计划见 **`docs/registry-quality-remediation-plan.md`**，分四个阶段：

| 阶段 | 主题 | 关键任务 | 周期 |
|---|---|---|---|
| Q1 | 安全与合规收口 | REG-01、REG-10；LSA/cached 默认不导出 | ~1 周 |
| Q2 | 工程化与代码健康 | REG-02、REG-03、REG-04、REG-08 | ~1–1.5 周 |
| Q3 | 正确性与覆盖 | REG-05、REG-06、REG-07、REG-09、REG-11 | ~1.5–2 周 |
| Q4 | 架构对齐与发布准备 | REG-12、REG-13、REG-14；文档/governance 同步 | ~1.5–2 周 |

---

## 9. 测试矩阵

| 测试类型 | 目标 | 覆盖内容 | 通过标准 |
|---|---|---|---|
| 静态检查 | 全仓库 | `cargo fmt --check`、`cargo clippy -D warnings`、`git diff --check` | 全绿 |
| Rust 单元测试 | 合成 hive | lookup 模块、hash_decrypt、txlog、recovery | 全绿，新增函数覆盖率 ≥ 60% |
| TxLog 覆盖测试 | 合成 LOG1/LOG2 | SYSTEM/SOFTWARE/SAM/SECURITY 字段覆盖 | 至少 4 个 family 通过 |
| 真实 E01 回归 | jc2 / liuyang | 全部 hive 非空、格式正确 | 两个样本均通过 |
| Expected JSON 回归 | `testdata/fixtures/` | R1–R4 新增 family | CI fixture diff 通过 |
| 前端契约测试 | `frontend/` | typecheck、test、mock-data 同步 | 全绿 |
| 依赖安全审计 | `deny.toml` | `cargo deny check advisories bans licenses sources` | 全绿 |
| 敏感数据扫描 | 仓库 | grep 明文 hash / 敏感 blob | 默认路径无命中 |
| 手动 UI 复核 | Registry 面板 | hash 列默认隐藏、LSA secrets 脱敏 | 通过 |

---

## 10. 验收标准

### 10.1 阶段验收

- **Q1**：默认 API/UI/mock 中不再出现 LM/NT 哈希；`deny.toml` 有说明；LSA/cached 默认不导出到报告。
- **Q2**：crypto 依赖 workspace 化；新模块无 `Result<_, String>`；`registry.rs` 拆分；`UserAssistEntryDto` 冲突消除。
- **Q3**：SAM/SECURITY 处理有 BootKey warning；解密状态可解释；`txlog_merged`/`deleted_keys_found` 为真实值；correlation 包含 registry family。
- **Q4**：lookup 为 canonical 路径；expected JSON 回归全绿；文档/governance 同步；release drill 通过。

### 10.2 总体发布验收

- 所有 **P0** 修复或明确阻断发布；所有 **P1** 有 owner、计划、验证路径。
- `cargo test --workspace`、`pnpm --dir frontend test --run`、`cargo deny check` 全绿。
- 至少 2 个真实 E01 样本注册表提取非空且无 panic。
- `/v2` governance dashboard 中 Registry family coverage 与实现一致。

---

## 11. 附录

### 11.1 关键文件索引

| 文件 | 作用 |
|---|---|
| `crates/app-services/src/analysis_service/extraction/registry.rs` | Registry 提取调度器与 artifact 构建（待拆分） |
| `crates/app-services/src/analysis_service/extraction/mod.rs` | structured summary 构建、hash 映射、txlog_merged 硬编码 |
| `crates/app-services/src/analysis_service/artifact_builders.rs` | `make_artifact`、`make_timeline_event`、`base_attrs` |
| `crates/app-services/src/artifact_service.rs:34` | 旧 `RegistryExtractor` 注册点 |
| `crates/artifacts-windows/src/registry/lookup/sam.rs` | SAM 用户/组提取与 hash 解密入口 |
| `crates/artifacts-windows/src/registry/hash_decrypt.rs` | BootKey / LM/NT 解密 |
| `crates/artifacts-windows/src/registry/sam_structs.rs:24` | BootKey 提取 |
| `crates/artifacts-windows/src/registry/txlog.rs` | 事务日志解析 |
| `crates/artifacts-windows/src/registry/recovery.rs` | 已删除 cell 恢复 |
| `crates/artifacts-windows/src/registry/parser.rs` | 旧 RegistryExtractor（仅 base block） |
| `crates/transport/src/dto/analysis.rs` | `SamUserAccountDto`、`UserAssistEntryDto`（summary 侧） |
| `crates/transport/src/dto/registry.rs` | `NtuserInfoDto`、`UserAssistEntryDto`（NTUSER 内部侧） |
| `frontend/src/components/analysis/AnalysisPanels.tsx` | SAM 用户表格渲染 |
| `frontend/src/lib/api/mock-data.ts` | mock registry summary |
| `frontend/src/types/models.ts` | 前端类型契约 |
| `deny.toml` | license allow list |

### 11.2 复用命令

```bash
# 静态检查
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 测试
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend test --run

# 依赖治理
cargo deny check advisories bans licenses sources

# 真实 E01 回归（需本地样本路径）
$env:FORENSICS_JC2_E01_FIXTURE="D:/獬豸杯/检材2.E01"
$env:FORENSICS_LIUYANG_E01_FIXTURE="E:/pangushi/刘洋/liuyang_pc.E01"
cargo test -p app-services --test liuyang_registry_extract_test -- --ignored --nocapture

# 敏感数据扫描（默认路径不应命中）
grep -R "passwordHash" crates/app-services/src/analysis_service crates/transport/src/dto frontend/src/lib/api/mock-data.ts
```

### 11.3 报告输出物

- 本报告：`docs/registry-module-quality-audit-2026-06-21.md`
- 整改计划：`docs/registry-quality-remediation-plan.md`
- 执行记录：后续补充 `development-reports/sessions/2026-06-21-registry-remediation.md`

---

*报告结束。所有证据基于 2026-06-21 仓库快照的真实源码阅读，file:line 锚点可直接跳转复核。*
