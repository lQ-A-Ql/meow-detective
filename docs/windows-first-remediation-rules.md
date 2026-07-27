# Windows 板块优先修复与开发规则（首版）

**生效日期**：2026-06-21  
**适用范围**：Forensics Workbench 项目，针对 Windows 取证板块的修复与增量开发  
**状态**：首版试行，后续随 V4 收尾与 V5-1 迭代更新

---

## 1. 边界限定

### 1.1 优先级范围
- **优先处理**：Windows 证据处理、Windows 制品解析（`artifacts-windows`）、Windows 相关应用服务与前端展示。
- **暂不处理**：Linux/macOS 制品解析器接入流水线、Linux/macOS 前端面板、PST/OST/mbox 邮件容器端到端集成。
  - 这些模块的 crate 级实现可继续保留，但不在当前 sprint 中修复其接入问题。
- **证据处理通用层**（`evidence-core`、`image-e01`、`fs-ntfs`、`fs-fat`、`fs-exfat`）属于 Windows 取证基础，纳入优先范围。

### 1.2 工作类型优先级
按以下顺序投入精力：
1. **结构性/质量性/可维护性问题**（高于功能新增）
2. **阻塞门禁的问题**（`cargo fmt`、`pnpm lint`、`cargo clippy -D warnings`）
3. **影响用户可见功能的 bug**（如注册表面板报错、表格不展示）
4. **Windows 板块的功能补全**（在质量债务可控前提下）
5. **性能优化与新增能力**

---

## 2. 代码规范红线

### 2.1 必须立即清理
- 生产代码中禁止新增 `#[allow(dead_code)]`；现有 68 处逐步移除。
- 禁止新增 `Result<T, String>`；现有 243 处逐步替换为 `thiserror` 类型。
- 生产源文件 ≤ 1500 行；前端组件文件 ≤ 500 行。
- 所有 member `Cargo.toml` 必须使用 `{ workspace = true }`，禁止直接写版本号。

### 2.2 错误处理
- Windows 板块新代码必须使用 typed error（如 `WindowsArtifactError`、`RegistryExtractionError`）。
- 服务层错误不得通过 `.map_err(|e| e.to_string())` 丢给命令层。
- 命令层使用 `CommandError` 统一脱敏；分类优先使用显式 `From` 转换，而非字符串匹配。

### 2.3 分层边界
- Tauri 命令层必须是薄包装：**校验 → 取 case → 委托 `app-services` → 返回 DTO**。
- 禁止在命令层直接调用 `persistence_sqlite::open_or_create` 或实例化 Repository。
- SQL 只能出现在 `persistence-sqlite` 和 `app-services` 内部；`check-command-sql-boundary.ps1` 覆盖范围将逐步扩展。

### 2.4 测试
- 每个新 public 函数至少一个单元测试。
- 修改注册表/EVTX/Prefetch/LNK 解析器后，必须更新或补充 `testdata/fixtures/` 中的 expected JSON。
- 真实 E01 回归测试（`#[ignore]`）在发布前必须手动触发并记录结果。

---

## 3. Windows 注册表板块专项规则

### 3.1 Hive 支持策略
- **v1 必支持**：SYSTEM、SOFTWARE、SAM、NTUSER.DAT、UsrClass.dat
- **v1 仅做识别与元数据**：SECURITY、DEFAULT（不提取敏感安全策略，避免误报）
- **路径匹配**：统一使用 `normalize_evidence_path` 做大小写不敏感后缀匹配。

### 3.2 Artifact 类型约定
分析提取流水线（`analysis_service::extraction::registry`）产生的 artifact 类型：
- `RegistryValue`：SYSTEM/SOFTWARE 的关键键值（原始键值表）。
- `RegistrySamUser`：SAM hive 解析出的本地用户账户。
- `RegistryUserAssist`：NTUSER.DAT 解析出的 UserAssist 程序执行记录。
- `RegistryHive`：其他 hive 的元数据（名称、最后写入时间等）。

### 3.3 前端展示约定
- `RegistryExtractionPanel` 使用 `RegistryStructuredSummary` 作为结构化视图数据源。
- 后端必须提供 `get_registry_structured_summary` 命令，返回：
  - `hiveOverviews`
  - `samUsers`
  - `userAssistEntries`
  - `networkProfiles`（当前可空）
  - `installedSoftware`（当前可空）
  - `usbDevices`（当前可空）
- 空数据时前端显示对应空状态，而不是隐藏整个面板。

### 3.4 错误降级
- 对 SECURITY/DEFAULT 等 v1 不深入解析的 hive，**不得使用 warning 级别消息**报告“仅支持 SYSTEM/SOFTWARE”。
- 改为 debug/info 级别日志，或记录为 `RegistryHive` 元数据 artifact。
- 用户可见的 warning 应保留给真正的解析失败或数据异常。

---

## 4. 开发流程

1. **每次修改前**：先运行 `cargo fmt --all -- --check`、`pnpm lint`、`cargo clippy --workspace --all-targets -- -D warnings`，确认基线。
2. **修改时**：最小变更原则；一次 PR 聚焦一个 bug 或一个功能点。
3. **修改后**：更新相关测试、expected JSON、mock data；确保前端 typecheck 通过。
4. **提交前**：运行 AGENTS.md 默认质量门禁。

---

## 5. 与 V5 的衔接

- V5-1 高级文件系统恢复与 Windows 深度取证将继承本规则。
- Linux/macOS/PST 板块将在 Windows 板块质量债务清偿后，按相同规范重新接入。
