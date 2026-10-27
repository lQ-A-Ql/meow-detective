# 导入与原始磁盘哈希验收表

## 变更范围

| 项目 | 当前行为 |
| --- | --- |
| 导入完成后的哈希 | 不再自动创建 `Evidence SHA-256` 后台任务 |
| 手动哈希范围 | 用户选择算法后，按 `LogicalDisk` 读取已验证的原始磁盘逻辑字节 |
| 支持算法 | MD5、SHA-1、SHA-256、SM3 |
| E01 | 逻辑磁盘摘要读取 `E01Reader` 输出；容器分段摘要仍属于 `ContainerSet` |
| 仿真指纹 | 使用 `evidence_digests` 中当前 `source_revision` 的逻辑磁盘 SHA-256 |
| 仿真失败条件 | 缺少当前摘要、来源版本变化、重新计算值不一致、来源无法读取 |

## 验收记录

| 编号 | 验收条件 | 验证方式 | 状态 |
| --- | --- | --- | --- |
| A01 | 导入完成不创建自动哈希任务 | 导入调度代码、案件打开流程、Linux 集合完成流程均已移除自动调度入口；前端回归测试不再期待自动进度行 | 已通过静态检查与前端定向测试 |
| A02 | E01 支持原始磁盘逻辑摘要 | `HashService::hash_logical_evidence` 使用 `image_e01::E01Reader`；`digest_service` 不再拒绝 E01 | 已通过受影响代码编译；不可读 E01 单测按读取错误失败 |
| A03 | RAW 与物理磁盘保持逻辑字节边界 | 逻辑磁盘摘要继续使用逻辑 reader，容器摘要继续使用 `ContainerSet` | 已通过 `digest_service` 单元测试 |
| A04 | 手动算法选择保留四种算法 | 前端数据源面板保留 MD5、SHA-1、SHA-256、SM3 按钮，调用 `calculate_evidence_digest` | 已通过前端类型检查与定向测试 |
| A05 | 摘要持久化按来源版本复用 | `current_logical_disk_sha256` 只接受 `LogicalDisk + sha256 + completed` 且 `source_revision` 相同的记录 | 已通过 `digest_service` 单元测试 |
| A06 | 仿真缺少原始磁盘 SHA-256 时拒绝启动 | 仿真准备流程查询当前逻辑摘要，缺失时返回 `InvalidSourceFingerprint` | 已通过仿真单元测试 |
| A07 | 仿真拒绝同长度篡改 | 仿真准备流程重新读取逻辑字节并比较 SHA-256 | 已通过仿真单元测试 |
| A08 | 仿真来源不可读时拒绝启动 | 重新计算失败映射为读取错误，不创建 COW 工作区 | 已通过仿真单元测试 |
| A09 | 前端只允许已经有原始磁盘 SHA-256 的来源进入仿真 | 仿真来源筛选检查 `originalDiskHashStatus === ready` 和 `originalDiskSha256` | 已通过前端类型检查 |
| A10 | 数据源摘要展示原始磁盘 SHA-256 | `DataSourceSummaryDto` 增加 `originalDiskSha256` 与 `originalDiskHashStatus`，并按当前来源版本过滤 | DTO 单元测试通过 |

## 已执行命令

| 命令 | 结果 |
| --- | --- |
| `cargo test -p app-services mount_service::emulation --lib` | 本次测试在当前并行 MCP 修改编译失败前曾通过 45 项；当前重跑被 `mcp_host_service/access.rs` 的类型推断错误阻断 |
| `cargo test -p app-services digest_service --lib` | 同一并行 MCP 编译错误阻断当前重跑 |
| `cargo test -p transport --lib dto::case` | 5 项通过 |
| `cargo test -p persistence-sqlite --lib evidence_digest` | 0 项匹配，测试目标成功完成 |
| `pnpm --dir frontend typecheck` | 通过 |
| `pnpm --dir frontend test -- tests/features/case/components/CaseOverview.test.tsx tests/app/pages/CaseHome.test.tsx` | 18 项通过 |
| `git diff --check` | 通过；仅报告现有文件换行符提示 |

## 尚未完成的实物验收

| 编号 | 条件 | 原因 |
| --- | --- | --- |
| R01 | 使用真实 E01、RAW、VHDX 和物理磁盘分别计算四种摘要 | 当前工作区没有可公开使用的真实物理磁盘或私有 E01 样本 |
| R02 | 启动 VMware 并完成完整仿真会话 | 需要本机 VMware、可启动镜像和相应权限 |
| R03 | 全工作区 Rust 测试、全前端测试和全部仓库守卫 | 当前工作区存在并行 MCP 修改，完整门禁需在该修改完成后运行 |
