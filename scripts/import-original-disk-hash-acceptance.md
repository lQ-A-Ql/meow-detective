# 导入与原始磁盘哈希验收表

## 变更范围

| 项目 | 当前行为 |
| --- | --- |
| 导入完成后的哈希 | 不再自动创建 `Evidence SHA-256` 后台任务 |
| 手动哈希范围 | 用户选择算法后，按 `LogicalDisk` 读取已验证的原始磁盘逻辑字节 |
| 支持算法 | MD5、SHA-1、SHA-256、SM3 |
| E01 | 逻辑磁盘摘要读取 `E01Reader` 输出；按 2 MiB 有序分块和受内存约束的 32 lane 并行读取；容器分段摘要仍属于 `ContainerSet` |
| 仿真准入 | 已导入且导入状态就绪的 E01/RAW 数据源直接进入仿真 |
| 仿真绑定 | 使用案件 ID、数据源 ID 和镜像类型派生的稳定身份令牌；provenance 记录 `sourceTrustPolicy=importTrusted` |
| 仿真失败条件 | 来源未就绪、来源类型超出边界、来源无法读取或当前大小与导入记录不一致 |
| 强制重算 | 摘要面板提供“忽略已完成结果并重新读取”；普通重复提交继续复用当前版本结果 |
| 文件摘要同步 | 文件 SHA-256 完成后同步 `file_entries.hash_sha256` 与文件取证指纹 |
| 来源哈希状态 | `hashStatus` 表示来源哈希；原始磁盘摘要使用独立字段，不覆盖来源状态 |

## 验收记录

| 名称 | 验收条件 | 验证方式 | 状态 |
| --- | --- | --- | --- |
| NO_AUTOMATIC_IMPORT_HASH | 导入完成不创建自动哈希任务 | 导入调度代码、案件打开流程、Linux 集合完成流程均已移除自动调度入口 | 已通过静态检查与前端定向测试 |
| E01_LOGICAL_DIGEST | E01 支持原始磁盘逻辑摘要 | `HashService::hash_logical_evidence` 使用 `image_e01::E01Reader` | 不可读 E01、逻辑字节范围和四算法测试通过 |
| E01_PARALLEL_DIGEST | E01 大样本读取使用受限多 lane，并保持逻辑顺序摘要 | 每个 lane 使用独立 reader、两个 2 MiB 缓冲和单一共享分页索引；摘要按块偏移顺序更新 | 合成 5 MiB E01 四算法与已知字节一致；真实 E01 SHA-256 两轮一致 |
| RAW_LOGICAL_BOUNDARY | RAW 与物理磁盘保持逻辑字节边界 | 逻辑磁盘摘要使用逻辑 reader，容器摘要使用 `ContainerSet` | `digest_service` 单元测试通过 |
| MANUAL_ALGORITHM_CHOICE | 手动算法选择保留四种算法 | 前端数据源面板保留 MD5、SHA-1、SHA-256、SM3 按钮 | 前端类型检查与定向测试通过 |
| DIGEST_REVISION_HISTORY | 摘要历史按来源版本展示 | 摘要结果持久化 `source_revision`，历史查询按当前版本标记是否陈旧 | `digest_service` 单元测试通过 |
| EMULATION_WITHOUT_DIGEST | 仿真不要求原始磁盘 SHA-256 | `prepare_emulation_source` 只执行导入来源与文件大小检查 | 44 项仿真服务测试通过 |
| EMULATION_SOURCE_IDENTITY | 仿真绑定使用来源身份令牌 | provenance schema 3 写入确定性身份令牌与 `sourceTrustPolicy=importTrusted` | 稳定性和范围测试通过 |
| EMULATION_UNREADABLE_SOURCE | 仿真来源不可读时拒绝启动 | 来源元数据或读取准备失败返回 I/O 错误 | 不可读来源测试通过 |
| EMULATION_SOURCE_SELECTION | 前端允许没有摘要状态的就绪 E01/RAW 来源进入仿真 | 仿真来源筛选只检查类型与导入状态 | 前端 20 项定向测试和类型检查通过 |
| ORIGINAL_DISK_DIGEST_DISPLAY | 数据源摘要展示原始磁盘 SHA-256 | `DataSourceSummaryDto` 提供 `originalDiskSha256` 与 `originalDiskHashStatus` | DTO 单元测试通过 |

## 已执行命令

| 命令 | 结果 |
| --- | --- |
| `cargo test -p app-services mount_service::emulation --lib` | 44 项通过 |
| `cargo test -p forensics-desktop emulation_registry --lib` | 28 项通过，3 项按私有环境条件忽略 |
| `cargo test -p app-services digest_service --lib` | 4 项通过 |
| `cargo test -p app-services --lib hash_service` | 24 项通过，4 项按外部设备或样本条件忽略 |
| `FORENSICS_VHDX_FIXTURE=artifacts/hash-acceptance/vhdx-small.vhdx cargo test -p app-services --lib vhdx_fixture_supports_all_digest_algorithms -- --ignored` | 四种算法通过，测试耗时约 4.08 秒 |
| `FORENSICS_E01_FIXTURE=D:\獬豸杯\检材2.E01 cargo test --release -p app-services --lib e01_fixture_supports_all_digest_algorithms -- --ignored --nocapture` | 100.60 GiB 四算法通过；32 lane；MD5 657.5 MiB/s、SHA-1 1,962.9 MiB/s、SHA-256 1,880.8 MiB/s、SM3 300.6 MiB/s |
| `FORENSICS_E01_FIXTURE=D:\獬豸杯\检材2.E01 cargo test --release -p app-services --lib e01_fixture_sha256_is_deterministic -- --ignored` | SHA-256 两轮共 111.16 秒，摘要与字节数一致 |
| `cargo test -p app-services --lib public_e01_and_raw_fixtures_prepare_without_persisted_digests` | 公开 E01 与 RAW 来源准备通过 |
| `cargo test -p app-services --test evidence_tasks` | 7 项通过 |
| `cargo test -p app-services --lib json_export_warns_when_evidence_hash_is_pending_or_unavailable` | 1 项通过 |
| `cargo test -p infrastructure hashing` | 15 项通过 |
| `cargo test -p persistence-sqlite sha256_update_refreshes` | 1 项通过 |
| `cargo test -p transport --lib commands::digest` | 3 项通过 |
| `cargo clippy -p app-services -p infrastructure -p persistence-sqlite -p transport -p forensics-desktop --all-targets -- -D warnings` | 通过 |
| `pnpm --dir frontend typecheck` | 通过 |
| `pnpm --dir frontend lint` | 通过 |
| `pnpm --dir frontend test -- tests/features/emulation/use-emulation-workspace-model.test.tsx` | 20 项通过 |
| `pnpm --dir frontend test -- tests/features/files/hooks/use-evidence-digest-model.test.tsx tests/features/files/components/DigestAlgorithmActions.test.tsx tests/features/jobs/import-event-state.test.ts` | 7 项通过 |
| `pnpm --dir frontend build` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-doc-archive.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-stage3-command-boundary.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-stage4-service-boundary.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-module-size.ps1` | 通过，2 项既有正式例外 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-rust-function-size.ps1` | 通过 |
| `powershell -ExecutionPolicy Bypass -File scripts/check-rust-test-layout.ps1` | 通过 |

## 真实 E01 性能记录

| 算法 | 耗时 | 平均吞吐 | 摘要 |
| --- | ---: | ---: | --- |
| MD5 | 156,685 ms | 657.5 MiB/s | `56674dba14308a8dee1ed07298b4b645` |
| SHA-1 | 52,481 ms | 1,962.9 MiB/s | `8aeb4440f6f52d5c6eae0ade3c86ee7e491a8745` |
| SHA-256 | 54,770 ms | 1,880.8 MiB/s | `56be362bf08d6b4c8a592e51d06cd0fa4237910bdbc1699e2cb13c9e8738db6f` |
| SM3 | 342,661 ms | 300.6 MiB/s | `467093295d3565be53874257d71479b87c527b6fea0b4f968ea65060fbe1eafe` |

该轮进程工作集约 145 MiB。SHA-1 与 SHA-256 已超过 1000 MB/s。MD5 和 SM3 受标准摘要顺序状态限制，增加读取 lane 不能提高摘要计算吞吐。

## 尚未完成的实物验收

| 名称 | 条件 | 原因 |
| --- | --- | --- |
| REAL_ALGORITHM_MATRIX | 使用受控物理磁盘或小型物理设备完成 LocalDisk 四种摘要 | 公开 E01、公开 RAW 与生成 VHDX 已完成；物理设备验收需要明确测试盘与环境变量 |
| REAL_EMULATION_WITHOUT_DIGEST | 使用没有预先逻辑磁盘摘要的可启动 E01 与 RAW 完成 VMware 会话 | 来源准备已完成；VMware、`vmrun.exe` 与 Dokan2 已可用，缺少可启动测试镜像 |
| FULL_QUALITY_GATES | 全工作区 Rust 测试、全前端测试和全部仓库守卫 | `app-services` 全库测试仍有两项既有 Kubernetes bbolt worker 失败 |
