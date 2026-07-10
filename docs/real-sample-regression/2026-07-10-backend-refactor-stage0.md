# 后端模块重构 Stage 0 基线

**日期**: 2026-07-10
**代码基线**: `2087df1cc5209fa879cdb3796e9a1437196bc2f4`
**运行环境**: Windows / `x86_64-pc-windows-msvc` / Rust stable
**目的**: 在移除 macOS、拆分 Windows/Linux 平台域和迁移测试前，冻结结构债务、默认测试与真实样本行为。

## 结构基线

三项守卫均使用严格 UTF-8、PowerShell 5.1、自测试和 reference-revision 单调 baseline 校验。

| 检查面 | 当前基线 |
|---|---:|
| 超目标生产模块 | 125 |
| 扫描到的生产函数 | 3,795 |
| 超过 100 行的既有函数 | 98 |
| 其中超过 150 行的既有函数 | 32 |
| 含 `src` 测试债务的文件 | 258 |
| inline test modules | 224 |
| test attributes | 1,943 |
| `src` test-only 文件行数 | 11,896 |

既有债务只能下降。新普通模块超过 500 行需要正式临时例外且不得超过 800 行；新 `mod.rs/lib.rs` 不得超过 200 行；非 baseline 新函数超过 100 行直接失败。所有 `src/**/*.rs` 均计入模块/函数守卫，不因 `tests.rs`、`*_tests.rs` 或 `src/tests/**` 命名豁免；新口径补录 10 个模块和 12 个超长函数，复审整改又把数据源删除能力拆出 `case_service.rs`，因此最终模块债务净值为 125。生产 target 必须位于所属 package 的 `src/`，仅根 `build.rs` 例外；测试 bridge 只允许指向 `tests/unit/**`。Windows metadata 超时会终止完整进程树。

## 复审整改

- Windows 文件 identity 按文件系统语义去重，manifest 中仅大小写不同的 target 不再重复进入 baseline；Linux 保持大小写敏感。
- metadata 超时依次具备精确 PID `taskkill`、Job Object kill-on-close 和 PID/父子关系/创建时间快照三层收敛路径；自测试覆盖禁用前两层后的子进程清理。
- 数据源删除改为同卷 tombstone 两阶段流程：先验证并暂存受管目录，再在单一 SQLite transaction 中删除注册并写成功审计；事务失败恢复原路径，提交后的清理失败返回 typed cleanup-pending，重复调用可继续清理 tombstone。
- 未注册 ID、非法受管路径、原始证据路径重叠、暂存失败和审计注入失败均不写成功审计，也不删除原始 evidence source。
- `case_service` 的测试正文已迁入物理 `tests/`，数据源删除能力拆入单一职责子模块。

## 默认测试

```powershell
cargo fmt --all -- --check
cargo test -p domain
cargo test -p transport
cargo test -p app-services
```

结果：

- `domain`: 45 unit tests + 3 doc tests 通过。
- `transport`: 125 通过，1 ignored。
- `app-services`: 493 通过，3 ignored；全部默认 integration/doc tests 通过。
- 原 `delete_data_source_cascades_rows_and_writes_audit_log` 测试仍按旧单库模型向 `app.db` 写 artifact，已改为验证真实 `source.db` 生命周期：删除目标 source/index/staging 与控制库注册，保留另一数据源和原始证据。

## Windows + Linux 双源隔离

样本：

| 平台 | 样本 | 大小 | SHA-256 |
|---|---|---:|---|
| Windows | `private/windows/jiancai2.E01` | 19,141,752,608 bytes | `D819689946FC2197DEC73F51BB65B1DA696C4258DB91FAC432DB734629081DA5` |
| Linux | `private/linux/jiancai3.E01` | 2,770,096,513 bytes | `164AD86C83AD68137F96D770A5B8A676703ED0B075A6DCEB3ECC61B1FA5D64B4` |

```powershell
cargo test -p forensics-desktop --test dual_source_import -- --ignored --nocapture --test-threads=1
```

结果：`1 passed`。测试体耗时 `55.12s`，含本轮增量编译总耗时约 `137s`。

验收事实：

- Windows 与 Linux 严格串行导入，各自产生独立 `sources/<dataSourceId>/source.db`。
- `app.db` 不承载两个数据源的文件树主数据。
- 平台分别持久化为 `windows` / `linux`，未发生 NTFS 与 XFS/LVM 元数据串源。
- 聚合文件树全部返回 `ds:<dataSourceId>:<localId>` 作用域 ID。
- 两个数据源均通过真实 `FileEntryId` 预览 smoke。
- source-scoped analysis ID 不冲突。

本地参考路径为 `D:\獬豸杯\检材2.E01` 与 `D:\獬豸杯\检材3.E01`；它们只用于 opt-in 测试，不得进入生产逻辑或默认 CI。

## PVE 回归

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
cargo test -p app-services --test linux_e01_integration pve_cluster_ -- --ignored --nocapture --test-threads=1
```

结果：`4 passed`，测试体耗时 `12.28s`。

- 发现 6 个 E01 成员。
- 三个 `disk01` 均发现并可读取 `pve/root` EXT4。
- 代表宿主导入 `files=56,471`、`dirs=5,931`、`totalBytes=5,250,350,224`。
- 三个 `disk02` 的 Ceph BlueStore OSD 继续明确报告 unsupported，不伪装成普通文件系统。

详细 PVE 字段基线见 `docs/real-sample-regression/2026-07-10-pve-host-ext4.md`。

## 未保证范围

- Stage 0 未改变 parser、预览、Hex、独立数据库或导入业务语义。
- macOS artifact/APFS/HFS+ 生产入口将在 Stage 1 删除，本记录不提升其支持等级。
- PVE Ceph BlueStore、VM disk reconstruction、跨节点语义分析仍不支持。
- 文件系统与 SQLite 仍不具备跨介质原子提交；当前以同卷 tombstone、事务内注册/审计、rollback restore 和 typed cleanup/recovery 状态保证可诊断、可重试。若 rollback restore 本身失败，需要按错误中给出的案件内相对 tombstone 路径执行人工恢复。
- 私有真实样本不进入仓库或默认 CI。
