# 导入与原始磁盘哈希验收表

## 变更范围

| 项目 | 当前行为 |
| --- | --- |
| 导入完成后的哈希 | 不再自动创建 `Evidence SHA-256` 后台任务 |
| 手动哈希范围 | 用户选择算法后，按 `LogicalDisk` 读取已验证的原始磁盘逻辑字节 |
| 支持算法 | MD5、SHA-1、SHA-256、SM3 |
| E01 | 逻辑磁盘摘要读取 `E01Reader` 输出；容器分段摘要仍属于 `ContainerSet` |
| 仿真准入 | 已导入且导入状态就绪的 E01/RAW 数据源直接进入仿真 |
| 仿真绑定 | 使用案件 ID、数据源 ID 和镜像类型派生的稳定身份令牌；provenance 记录 `sourceTrustPolicy=importTrusted` |
| 仿真失败条件 | 来源未就绪、来源类型超出边界、来源无法读取或当前大小与导入记录不一致 |

## 验收记录

| 编号 | 验收条件 | 验证方式 | 状态 |
| --- | --- | --- | --- |
| A01 | 导入完成不创建自动哈希任务 | 导入调度代码、案件打开流程、Linux 集合完成流程均已移除自动调度入口；前端回归测试不再期待自动进度行 | 已通过静态检查与前端定向测试 |
| A02 | E01 支持原始磁盘逻辑摘要 | `HashService::hash_logical_evidence` 使用 `image_e01::E01Reader`；`digest_service` 不再拒绝 E01 | 已通过受影响代码编译；不可读 E01 单测按读取错误失败 |
| A03 | RAW 与物理磁盘保持逻辑字节边界 | 逻辑磁盘摘要继续使用逻辑 reader，容器摘要继续使用 `ContainerSet` | 已通过 `digest_service` 单元测试 |
| A04 | 手动算法选择保留四种算法 | 前端数据源面板保留 MD5、SHA-1、SHA-256、SM3 按钮，调用 `calculate_evidence_digest` | 已通过前端类型检查与定向测试 |
| A05 | 摘要历史按来源版本展示 | 摘要结果持久化 `source_revision`，历史查询按当前版本标记是否陈旧 | 已通过 `digest_service` 单元测试 |
| A06 | 仿真不要求原始磁盘 SHA-256 | `prepare_emulation_source` 只执行导入来源与文件大小检查 | 44 项仿真服务测试通过 |
| A07 | 仿真绑定使用来源身份令牌 | provenance schema 3 写入确定性身份令牌与 `sourceTrustPolicy=importTrusted` | 稳定性和范围测试通过 |
| A08 | 仿真来源不可读时拒绝启动 | 来源元数据或读取准备失败返回 I/O 错误 | 不可读来源测试通过 |
| A09 | 前端允许没有摘要状态的就绪 E01/RAW 来源进入仿真 | 仿真来源筛选只检查类型与导入状态 | 前端 20 项定向测试和类型检查通过 |
| A10 | 数据源摘要展示原始磁盘 SHA-256 | `DataSourceSummaryDto` 增加 `originalDiskSha256` 与 `originalDiskHashStatus`，并按当前来源版本过滤 | DTO 单元测试通过 |

## 已执行命令

| 命令 | 结果 |
| --- | --- |
| `cargo test -p app-services mount_service::emulation --lib` | 44 项通过 |
| `cargo test -p forensics-desktop emulation_registry --lib` | 28 项通过，3 项按私有环境条件忽略 |
| `cargo test -p app-services digest_service --lib` | 4 项通过 |
| `cargo test -p infrastructure hashing` | 10 项通过 |
| `cargo clippy -p app-services -p forensics-desktop --all-targets -- -D warnings` | 通过 |
| `pnpm --dir frontend typecheck` | 通过 |
| `pnpm --dir frontend lint` | 通过 |
| `pnpm --dir frontend test -- tests/features/emulation/use-emulation-workspace-model.test.tsx` | 20 项通过 |
| `pnpm --dir frontend build` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-doc-archive.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-stage4-service-boundary.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-module-size.ps1` | 通过，2 项既有正式例外 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-rust-function-size.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-rust-test-layout.ps1` | 通过 |

## 尚未完成的实物验收

| 编号 | 条件 | 原因 |
| --- | --- | --- |
| R01 | 使用真实 E01、RAW、VHDX 和物理磁盘分别计算四种摘要 | 当前工作区没有可公开使用的真实物理磁盘或私有 E01 样本 |
| R02 | 使用没有预先逻辑磁盘摘要的 E01 与 RAW 启动 VMware 并完成会话 | 需要本机 VMware、可启动镜像和相应权限 |
| R03 | 全工作区 Rust 测试、全前端测试和全部仓库守卫 | 仍需在提交候选上运行完整门禁 |
