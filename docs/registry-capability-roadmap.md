# Registry 模块长期可执行方案

> 目标：在现有 `crates/artifacts-windows` 注册表解析基础上，系统性地补齐未完成的分析与提取能力，使其覆盖主流取证场景，并达到可验证、可回归、可发布的工程标准。
> 当前基线：SYSTEM/SOFTWARE/SAM/NTUSER 已实现核心字段提取与 txlog 覆盖；SAM LM/NT 哈希解密已落地。

---

## 1. 现状与差距分析

### 1.1 已实现能力

| Hive | 已实现提取 |
|------|-----------|
| SYSTEM | ComputerName、TimeZone、Network Adapter（TCP/IP + 友好名/MAC）、Services/Drivers（Type/Start/ImagePath/ServiceDll/Group/ObjectName）、ShutdownTime |
| SOFTWARE | ProductName/Build/Version/InstallDate/Owner、已安装软件（Uninstall）、HKLM Run/RunOnce/RunOnceEx、Winlogon（Shell/Userinit/Notify/AutoAdminLogon/DefaultDomainName/DefaultUserName）、LSA Packages（Authentication/Notification/Security）、NetworkList Profiles/Signatures、AppCompatFlags Layers |
| SAM | 本地用户/组/RID/SID、组成员、登录统计、密码策略、LM/NT 哈希解密 |
| NTUSER | Run/RunOnce、RecentDocs MRU、UserAssist、TypedURLs、WordWheelQuery、MountPoints2、默认浏览器、OpenSavePidlMRU、LastVisitedPidlMRU、RunMRU |
| USRCLASS | ShellBags（BagMRU/Bags）、MuiCache |
| Amcache.hve | InventoryApplication、InventoryApplicationFile |
| SECURITY | 本地安全策略（domain name、account domain name、machine SID、audit policy hex）、LSA Secrets 元数据（secret names + encrypted blob hex，不解密）、缓存域凭证（NL$ entries + encrypted blob hex，不解密） |
| TxLog | SYSTEM/SOFTWARE 字段覆盖、NTUSER Run/UserAssist 覆盖 |

### 1.2 与 ForensicsTool 等成熟工具的差距

| 能力域 | 当前状态 | 取证价值 |
|--------|---------|---------|
| **SYSTEM 服务/驱动** | R1-Phase-1 已落地（合成测试通过） | 持久化、rootkit、异常驱动；真实样本回归待补 |
| **USBSTOR/USB 设备历史** | R1-Phase-2 已落地（合成测试通过） | 可移动介质取证、数据外泄；真实样本回归待补 |
| **MountedDevices** | R1-Phase-2 已落地（合成测试通过） | 卷标映射、盘符历史；与 USBSTOR 的 volume/drive 关联待增强 |
| **ShimCache / AppCompatCache** | R1-Phase-3 已落地（Win10/11 格式 + fallback 路径扫描，合成测试通过） | 程序执行/存在证据；多版本格式覆盖待真实样本验证 |
| **系统关机/启动时间** | R1-Phase-3 已落地（ShutdownTime 合成测试通过） | 时间线边界；启动时间仍依赖 EVTX 6005/6006 |
| **SOFTWARE 级 Run/Winlogon/LSA** | R2-Phase-1 已落地（合成测试通过） | 系统级持久化；真实样本回归待补 |
| **NetworkList / 网络配置文件** | R3-Phase-1 已落地（合成测试通过） | 网络历史、首次连接时间；真实样本回归待补 |
| **USRCLASS ShellBags** | R2-Phase-3 已落地（合成测试通过） | 用户目录浏览、删除文件夹痕迹；真实样本回归待补 |
| **NTUSER OpenSave/LastVisited/RunMRU** | R2-Phase-2 已落地（合成测试通过） | 用户文件操作、程序运行 |
| **Amcache.hve** | R3-Phase-2 已落地（合成测试通过） | 程序执行、SHA1、安装/删除时间；真实样本回归待补 |
| **SECURITY / LSA Secrets** | R4 已落地（基础字段 + 加密 blob 元数据，默认不解密） | 域缓存凭证、服务密码；解密功能受控，需显式授权 |
| **注册表键 Last-Write-Time** | 未读取 | 时间线精确化 |

---

## 2. 总体设计原则

1. **最小侵入**：复用 `RegistryHiveReader`、`txlog_util`、`lookup` 模块的导航与值读取能力。
2. **契约先行**：新增字段必须先定义在 `crates/transport/src/dto/`，再同步 frontend `types/models.ts`。
3. **expected JSON 优先**：每新增 parser 必须先更新 `testdata/fixtures/` 中的期望输出，CI 回归门禁比对。
4. **Source Object ID 必填**：新增 artifact 必须设置 `sourceObjectId`，确保跨工件关联可用。
5. **txlog 可覆盖**：关键字段（如服务配置、用户活动）应支持 transaction log 覆盖。
6. **防御式解析**：损坏、缺失、版本差异不得 panic，统一通过 `warnings` 报告。

---

## 3. 阶段设计（Stage R1 → R4）

### Stage R1：系统级证据基线（预计 4–6 周）

**目标**：补齐 SYSTEM hive 中最常用、价值最高的系统证据。

#### R1-Phase-1 服务与驱动 ✅
- **[done] Task R1.1.1** 在 `lookup/system.rs` 新增 `extract_services_from_system_hive`，遍历 `ControlSetXXX\Services`。
  - 提取：ServiceName、DisplayName、ImagePath、StartType、Type、Group、ObjectName、ErrorControl、DelayedAutoStart、DependOnService/Group、FailureCommand、RequiredPrivileges、ServiceDll（svchost）。
  - 输出 DTO `SystemServiceDto`，新增 artifact type `RegistrySystemService`。
- **[done] Task R1.1.2** 标记关键服务（kernel driver / auto-start service）作为 timeline/correlation 输入（confidence 由调用方评估）。
- **[done] Task R1.1.3** 合成 fixture：构造包含 3 个服务（own-process、svchost share-process、kernel driver）的最小 SYSTEM hive，产出 4 个单元测试。

#### R1-Phase-2 USB 与挂载设备 ✅
- **[done] Task R1.2.1** 新增 `extract_usb_devices_from_system_hive`：
  - `Enum\USBSTOR`：设备描述、序列号、友好名、首次/最后插入时间（从父键/子键 Last-Write-Time 推断）。
  - 可选 `Enum\USB` VID/PID 富化。
- **[done] Task R1.2.2** 新增 `extract_mounted_devices_from_system_hive`：
  - `MountedDevices`：DOS 设备名 ↔ 卷 GUID ↔ 磁盘签名映射。
- **[done] Task R1.2.3** 复用/新增 DTO `UsbDeviceHistoryDto`、`MountedDeviceDto`，artifact type 分别为 `RegistryUsbDevice`、`RegistryMountedDevice`。

#### R1-Phase-3 系统时间与 ShimCache ✅
- **[done] Task R1.3.1** 新增 `extract_shutdown_time_from_system_hive`：读取 `ControlSetXXX\Control\Windows\ShutdownTime`（REG_BINARY/REG_QWORD FILETIME）。
- **[done] Task R1.3.2** 新增 `extract_shimcache_from_system_hive`：解析 `Session Manager\AppCompatCache` 二进制 blob，提取路径、文件修改时间；支持 Win10/11 `10ts` 条目签名，并带 UTF-16LE 路径扫描 fallback。
- **[done] Task R1.3.3** 在 `analysis_service::extraction::registry.rs` 中集成新提取器，artifact type 为 `RegistryShutdownTime` / `RegistryShimCache`。

#### R1 验收标准
- [x] R1-Phase-1 服务/驱动 artifact 通过合成 hive 单元测试（4 tests）。
- [x] R1-Phase-2 USB/MountedDevices artifact 通过合成 hive 单元测试（2 tests）。
- [x] R1-Phase-3 ShutdownTime/ShimCache artifact 通过合成 hive 单元测试（2 tests）。
- [ ] 在至少 1 个真实 E01 样本上输出非空结果。
- [x] `docs/parser-support-matrix.md` 更新支持状态。
- [ ] 新增 expected JSON 并通过 CI 回归比对。

---

### Stage R2：持久化与用户活动（预计 4–6 周）

**目标**：覆盖系统级持久化和 NTUSER/USRCLASS 中高频用户活动痕迹。

#### R2-Phase-1 系统级持久化（SOFTWARE） ✅
- **[done] Task R2.1.1** 新增 `extract_machine_run_keys_from_software_hive`：
  - `SOFTWARE\Microsoft\Windows\CurrentVersion\Run`、`RunOnce`、`RunOnceEx`。
  - `SOFTWARE\WOW6432Node\...Run`、`RunOnce`。
- **[done] Task R2.1.2** 新增 `extract_winlogon_fields_from_software_hive`：
  - `Winlogon\Shell`、`Userinit`、`Notify`、`AutoAdminLogon`、`DefaultDomainName`、`DefaultUserName`。
- **[done] Task R2.1.3** 新增 `extract_lsa_packages_from_system_hive`：
  - `SYSTEM\CurrentControlSet\Control\Lsa\Authentication Packages`、`Notification Packages`、`Security Packages`。
- **[done] Task R2.1.4** DTO：复用/新增 `RegistryRunKeyDto`（区分 user/machine）、`WinlogonConfigDto`、`LsaPackageDto`。

#### R2-Phase-2 NTUSER 深度用户活动
- **[done] Task R2.2.1** 扩展 `ntuser.rs`：
  - `OpenSavePidlMRU` / `OpenSavePidlMRU\*`：最近打开/保存文件 MRU。
  - `LastVisitedPidlMRU`：最近访问目录 MRU。
  - `RunMRU`：`Win+R` 运行历史。
  - `ComDlg32\OpenSavePidlMRU`、`ComDlg32\LastVisitedPidlMRU`。
- **[done] Task R2.2.2** 解析 PIDL/MRUListEx 结构，恢复文件/目录路径。
- **[done] Task R2.2.3** DTO：扩展 `NtuserInfo`，新增 `open_save_mru`、`last_visited_mru`、`run_mru`。

#### R2-Phase-3 USRCLASS ShellBags 与 MuiCache ✅
- **[done] Task R2.3.1** 新增 `extract_shellbags_from_usrclass_hive`：
  - `Local Settings\Software\Microsoft\Windows\Shell\BagMRU` + `Shell\Bags`。
  - 提取条目：文件夹路径、NodeSlot、LastWriteTime、视图模式。
- **[done] Task R2.3.2** 新增 `extract_muicache_from_usrclass_hive`：
  - `Local Settings\Software\Microsoft\Windows\Shell\MuiCache`：程序路径 ↔ 友好描述。
- **[done] Task R2.3.3** DTO：`ShellbagEntryDto`、`MuiCacheEntryDto`。
- **[done] Task R2.3.4** 在 `registry.rs` 中把 `/usrclass.dat` 分支从 `extract_ntuser_fields` 拆出，分别调用 NTUSER/USRCLASS 专用提取器。

#### R2 验收标准
- [x] HKLM Run / Winlogon / LSA 在合成 SOFTWARE hive 上可验证。
- [ ] NTUSER MRU 与 USRCLASS ShellBags 在真实 E01 的多个用户配置文件中输出非空。
- [x] 新增 artifact family 自动进入 `correlation_service` 规则家族。
- [ ] 前端 Artifact/Registry 表格新增对应列。

---

### Stage R3：网络、Amcache 与应用兼容（预计 4–6 周）

**目标**：补齐网络连接历史、应用执行证据（Amcache）及应用兼容性标记。

#### R3-Phase-1 NetworkList 与网络签名 ✅
- **[done] Task R3.1.1** 新增 `extract_network_profiles_from_software_hive`：
  - `Microsoft\Windows NT\CurrentVersion\NetworkList\Profiles`：ProfileName、Description、DateCreated、DateLastConnected、Managed。
  - `...\Signatures\Unmanaged` / `Managed`：DNS Suffix、FirstNetwork、DefaultGatewayMac。
- **[done] Task R3.1.2** DTO：`NetworkProfileDto`（注意与 SYSTEM 网络适配器区分命名）。
- **[done] Task R3.1.3** 将网络配置加入 timeline 事件（首次连接、最后连接）。

#### R3-Phase-2 Amcache.hve ✅
- **[done] Task R3.2.1** 新增 `crates/artifacts-windows/src/registry/amcache.rs`：
  - 识别 `Amcache.hve` 路径：`/Windows/appcompat/Programs/Amcache.hve`。
  - 解析 `Root\InventoryApplication`、`InventoryApplicationFile`、`InventoryDriverBinary`。
  - 提取：程序名、版本、路径、SHA1、首次运行/安装/删除时间、ProgramId。
- **[done] Task R3.2.2** DTO：`AmcacheEntryDto`、`AmcacheDriverDto`。
- **[done] Task R3.2.3** 在 `analysis_service` 的 evidence discovery 中把 `Amcache.hve` 识别为 Registry 类别。

#### R3-Phase-3 AppCompatFlags / Layers ✅
- **[done] Task R3.3.1** 新增 `extract_appcompat_layers_from_software_hive`：
  - `SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers`：程序路径 + 兼容模式/权限提升标记。
- **[done] Task R3.3.2** DTO：`AppCompatLayerDto`。

#### R3 验收标准
- [ ] Amcache 在真实 E01 上输出 >=20 条程序执行记录。
- [ ] NetworkList 输出至少包含 1 条首次/最后连接时间。
- [x] 新增 timeline 事件类型并通过 `timeline_service` 投影测试。

---

### Stage R4：高级凭证与企业级证据（预计 6–8 周）

**目标**：在风险可控前提下实现 SECURITY hive 解密与高级凭证提取。

#### R4-Phase-1 SECURITY hive 基础解析 ✅
- **[done] Task R4.1.1** 新增 `crates/artifacts-windows/src/registry/security.rs`：
  - 读取 `SECURITY\Policy\PolAdtEv`（审计策略 hex）。
  - 读取 `SECURITY\Policy\PolPrDmN`（默认域名 / 账户域域名 / machine SID）。
- **[done] Task R4.1.2** 定义 artifact type `RegistrySecurityPolicy`。

#### R4-Phase-2 LSA Secrets 元数据（不解密） ✅
- **[done] Task R4.2.1** 复用 SAM `hash_decrypt.rs` 的 BootKey 派生逻辑（为受控解密预留）。
- **[done] Task R4.2.2** 实现 `SECURITY\Policy\Secrets` 下 secret 名称与加密 blob hex 提取；**默认不解密**，仅输出元数据。
- **[done] Task R4.2.3** DTO：`LsaSecretDto`，敏感值默认不导出到报告，前端需脱敏展示。

#### R4-Phase-3 域缓存凭证元数据（不解密） ✅
- **[done] Task R4.3.1** 解析 `SECURITY\Cache` 中的 NL$ 缓存条目。
- **[done] Task R4.3.2** 提取 NL$ 条目加密 blob hex；**默认不解密**（PEK / MSCash2 解密受控）。
- **[done] Task R4.3.3** DTO：`CachedCredentialDto`，标记为 `Sensitive` 字段。

#### R4 验收标准
- [x] SECURITY 基础字段在真实 E01 上可读取。
- [ ] LSA Secrets 解密功能在已知明文测试向量上验证通过（当前仅输出加密 blob 元数据，解密受控）。
- [x] 敏感数据遵循项目的错误脱敏与导出安全策略（`docs/error-classification-manual.md`、`docs/export-and-media-safety.md`）。
- [x] 默认不输出到 HTML/CSV/JSON 报告，需 investigator 显式授权。

---

## 4. 测试矩阵

| 测试类型 | 目标 | 覆盖内容 | 通过标准 |
|---------|------|---------|---------|
| **单元测试** | 合成 hive | 每个新 extractor 在最小 regf 上输出预期字段 | 100% 新增函数有独立测试 |
| **TxLog 覆盖测试** | 合成 LOG1/LOG2 | 关键字段被 txlog 覆盖后取最新值 | 至少覆盖 50% 关键字段 |
| **Tiny Fixture 测试** | 已提交的最小真实 hive | 与 `testing::fixtures::tiny_registry_*` 集成 | 不破坏现有 tiny fixture 断言 |
| **Expected JSON 回归** | `testdata/fixtures/` | 新增/修改 expected JSON | CI `check-doc-drift` / fixture diff 通过 |
| **真实 E01 回归** | `e01_registry_structured_summary_test` | 新 artifact 在真实样本上非空且格式正确 | 至少 1 个真实样本通过 |
| **多版本 E01 验证** | Win10/Win11/Server 样本 | 检查版本相关结构差异（ShimCache、Amcache） | 至少覆盖 2 个 Windows 版本 |
| **Correlation 回归** | `correlation_service` | 新增 family 与 File/Timeline 关联 | `correlation_snapshot` 测试包含新 family |
| **性能基准** | `jc2_pipeline` | 新增提取不使整体 import 时间退化 >10% | benchmark 结果落入 baseline |

---

## 5. 验收标准

### 5.1 工程门禁（每阶段必须满足）

- `cargo fmt --all -- --check` 通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo test --workspace` 通过（忽略真实样本测试）。
- 新增代码单元测试覆盖率 >= 60%（新增函数行）。
- 所有新增 DTO 已同步到 frontend `types/models.ts`。
- `AGENTS.md` 与 `docs/parser-support-matrix.md` 已更新。

### 5.2 功能验收（每阶段）

| 阶段 | 功能验收点 |
|------|-----------|
| R1 | SYSTEM 新 artifact 在真实 E01 上非空；ShimCache 至少输出路径+时间 |
| R2 | HKLM Run/Winlogon/LSA 可提取；NTUSER MRU、USRCLASS ShellBags 在真实样本上非空 |
| R3 | Amcache 输出可执行文件路径、SHA1、首次运行时间；NetworkList 输出首次/最后连接 |
| R4 | SECURITY 基础字段可读；LSA Secrets/Cached Credentials 元数据受控输出（加密 blob hex，默认不解密） |

### 5.3 发布验收

- 所有新增 family 出现在 `/v2` governance dashboard 的 `familyCoverage` 中。
- `docs/known-unsupported-formats.md` 更新仍不支持的 registry artifact。
- 至少完成 1 次 full release drill，包含真实 E01 样本。

---

## 6. 评估方案

### 6.1 评分维度与权重

| 维度 | 权重 | 评估方式 |
|------|------|---------|
| 功能完整度 | 35% | 计划任务完成率 + 真实样本输出覆盖率 |
| 测试质量 | 25% | 单元/集成测试通过率 + 新增代码覆盖率 |
| 回归稳定性 | 20% | `cargo test --workspace`、jc2 pipeline、E01 回归是否全绿 |
| 契约与文档 | 15% | DTO 同步、`AGENTS.md`/`parser-support-matrix` 更新、expected JSON 维护 |
| 安全与合规 | 5% | 敏感字段脱敏、导出权限控制、LSA secrets 不默认泄露 |

### 6.2 阶段等级目标

| 阶段 | 目标等级 | 关键指标 |
|------|---------|---------|
| R1 完成 | B+ | 新增 3 个 artifact family，真实 E01 回归全绿 |
| R2 完成 | A- | 新增 5 个 artifact family，ShellBags/MRU 可解释时间线 |
| R3 完成 | A | Amcache + NetworkList 落地，governance familyCoverage >= 12 |
| R4 完成 | A | SECURITY 解密能力受控可用，敏感数据零泄露 |

### 6.3 持续评估机制

1. **每周迭代看板**：每个 Task 必须绑定测试用例与 expected JSON 变更。
2. **双周真实样本回归**：在 `检材2.E01` 与 Liu Yang 样本上运行 `e01_registry_structured_summary_test` 与 `sam_check2`。
3. **月度治理评分**：更新 `testdata/governance/v2-runtime-results.json`，由 `/v2` dashboard 自动反映 Registry 覆盖率变化。
4. **每次 PR 门禁**：
   - `cargo fmt && cargo clippy && cargo test`
   - `scripts/check-doc-drift.ps1`
   - `scripts/check-benchmark-regression.ps1`

---

## 7. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| ShimCache/Amcache 格式随 Windows 版本变化 | 解析失败或误解析 | 按版本分支实现；收集多版本 fixture；版本未知时降级为原始二进制展示 |
| ShellBags PIDL 结构复杂 | 路径恢复不完整 | 先实现 BagMRU 基础条目，再逐步支持 ItemIdList 解析 |
| LSA Secrets / 缓存凭证属于敏感数据 | 合规风险 | 默认不解密输出；增加前端显式授权；审计日志记录访问 |
| 真实样本中某些 hive 损坏 | 提取崩溃 | 所有解析必须走 `RegistryHiveReader`，异常进入 `warnings` |
| 新增提取拖慢 import 性能 | 大样本超时 | 对 Amcache/ShimCache 等大 key 设置读取上限；并行提取 |

---

## 8. 依赖与前置条件

- `crates/transport/src/dto/` 中 artifact/registry DTO 可扩展。
- `RegistryHiveReader` 已支持键导航、值读取、子键枚举、原始字节读取。
- `txlog_util` 已支持单字段覆盖，新关键字段需接入。
- frontend `VITE_API_MODE=tauri` 与 mock 模式需要同步 mock data。
- 真实样本：`FORENSICS_E01_FIXTURE` 与 `FORENSICS_LIUYANG_E01_FIXTURE` 需可访问。

---

## 9. 近期下一步（Next Actions）

1. 文档同步：将 R1–R4 已完成能力同步到 `AGENTS.md`、`docs/parser-support-matrix.md`、`docs/validation-trust-framework.md`、`docs/release-scorecard.md`。
2. Expected JSON：补齐 `testdata/fixtures/` 中 R2/R3/R4 新增 artifact 的期望输出，并接入 CI 回归比对。
3. 真实 E01 回归：在 `检材2.E01` 与 Liu Yang 样本上运行 `e01_registry_structured_summary_test`，验证 USRCLASS/Amcache/NetworkList/SECURITY 非空且格式正确。
4. Frontend 类型与 mock data：同步 `types/models.ts`、`lib/api/mock-data.ts`、Artifact/Registry 表格列。
5. 后续可选：在显式授权与审计条件下，实现 LSA Secrets / 缓存凭证的受控解密。
