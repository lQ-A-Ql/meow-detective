# SQLite 与 Fjall 数据库审计

审计日期：2026-11-10；并发写入更新：2026-11-11
当前 source 格式：`source_041_fjall_detail_ownership`
验证分支基线：`044c5566`

## 存储规则

- 案件数据库保存案件、来源注册、导入任务、Graph、审计和领域控制信息。
- Source SQLite 只保存来源身份、Fjall 代际、导入/分析作业、候选检查点、发布回执和恢复控制，不建立 `file_entries`、`artifacts`、`timeline_events`、`forensic_fingerprints`。
- Catalog、Artifact、Timeline 记录由该来源的 Fjall generation 保存。
- Analysis worker 直接使用 Fjall 原生 `Database::batch` 并发写入记录与索引；同一数据库的 journal append 由 Fjall 排定顺序，不再持有应用层单写事务锁。
- Artifact/Timeline 的共享计数与 revision 在 worker 全部结束后汇总一次；SQLite 仅在候选检查点全部准备后提交生命周期控制事务。
- 文件枚举和分析 worker 使用独立 staging SQLite；staging 表不属于已发布 source 或案件数据库。
- Source 身份存在时，Fjall generation 缺失会返回错误。只读请求不会创建 generation；显式导入初始化可以创建 build generation。
- 案件 migration registry 不再包含 source detail 建表、索引、字段升级或删除迁移。旧 source SQLite 明细布局由 source schema 检查拒绝。

## 修复状态

| 编号 | 审计项 | 状态 | 当前实现与直接验证 |
|---|---|---|---|
| 1 | Source SQLite 与 Fjall 的明细写入重复 | 已完成 | 新 source schema 不含三类明细表；导入 staging、Analysis merge、Logical 导入和原生 generation 发布均有回归。所有 source detail Repository 在缺少 source identity 或 generation 时失败关闭，生产 SQL 扫描仅保留 staging 和 NTFS staging。 |
| 2 | Fjall 句柄重复打开及删除冲突 | 已完成 | 共享 SourceStore、按路径关闭租约、活动句柄删除保护；案件关闭和删除回归通过。 |
| 3 | 相同内容重复写入 | 已完成 | Artifact、Timeline 和 Catalog 幂等写跳过相同记录；计数、索引及 revision 由有效写入更新。 |
| 4 | Parser 明细逐条提交 | 已完成 | 导入记录采用分批写入；来源分析、CephFS 及 Carving 相关回归通过。 |
| 5 | Analysis 输出污染 Source 完成标记 | 已完成 | Catalog 与 Analysis projection 标记分开发布。 |
| 6 | 未完成或缺失 Generation 被读取 | 已完成 | 未完成 projection 返回未就绪；source identity 存在而 Fjall generation 缺失时读取失败关闭。 |
| 7 | Artifact 对象路径不一致 | 已完成 | Runtime 与投影共同使用案件级 `artifacts/objects`。 |
| 8 | RBD Header Parser kind 不一致 | 已完成 | 写入与读取使用同一 Parser kind。 |
| 9 | Timeline 查询重复扫描 | 已完成 | Fjall 提供总数、类型和时间桶计数；分页读取使用时间与类型索引及稳定游标。 |
| 10 | 普通导入与恢复发布流程分离 | 已完成 | 导入按 attempt 建立 generation，SyncAll 后 seal，再发布；恢复依赖 SQLite operation receipt 和 generation receipt。 |
| 11 | Analysis 重复镜像与 Parser 重扫 | 已完成 | 删除 SQLite detail change log 与触发器；原生 revision 仅随有效变更更新。Analysis 重提取通过临时 generation 与 SQLite checkpoint 协同发布。 |
| 12 | Android Parser 重复身份 | 已完成 | Android Source projection 与分析使用稳定 Parser id，重复内容执行幂等写入。 |
| 13 | Artifact source 列表索引选择错误 | 已完成 | Source 列表读取主记录 keyspace；类型和对象查询使用对应索引。 |
| 14 | 缓存句柄阻止案件删除 | 已完成 | 删除先取得关闭租约；活动句柄使删除以可恢复错误退出。 |
| 15 | Artifact 删除未释放对象引用 | 已完成 | Artifact 删除和重复投影同步更新对象引用。 |
| 16 | Source schema 检查晚于恢复状态写入 | 已完成 | 打开案件时先检查来源 schema，再进行恢复扫描。 |
| 17 | 恢复诊断错误使用 operation 外键 | 已完成 | 写入恢复状态前校验 operation 归属。 |
| 18 | Analysis 写入受单写事务锁串行限制 | 已完成 | 独立记录批次改用 Fjall `Database::batch`；4 worker 并发写入回归通过；共享指标在代际封存前一次重建。 |
| 19 | Source SQLite 残留指纹业务明细 | 已完成 | `forensic_fingerprints` 从 source migration 删除；导入、Catalog/Analysis staging 合并和 hash 更新不再写 source SQLite；旧 SQL detail fallback 已删除。 |
| 20 | Linux 摘要冷加载重复遍历 Catalog、Artifact 和候选检查点 | 已完成 | 候选与 kernel catalog 在分析发现阶段随 Catalog 单次流式遍历记录到 source 控制元数据；Artifact 单次聚合生成 counts、首页和系统信息；Fjall Meta 按 Artifact revision 与分析检查点 revision 缓存静态摘要及分页。导入 finalize 预热 `offset=0, limit=200`，后续页使用 family-created 索引读取。真实检材冷进程读缓存 `1,697 ms`，Artifact `49,461`；原接口超过 MCP `30 s` 超时。 |

## 第一项验收表

| 编号 | 验收条件 | 状态 | 验证证据 | 未通过项 |
|---|---|---|---|---|
| FJ-01 | PartitionRecord 只写 Fjall | 已完成 | `partition_index_materialization`、分区查询回归 | 无 |
| FJ-02 | staging 合并在 Catalog SyncAll 后提交 SQLite 控制事务 | 已完成 | `staging_merge_test` 12 项；`staging_partition_root_test` 5 项 | 无 |
| FJ-03 | 普通目录枚举流式写 Fjall，完整 generation 不重复镜像 Catalog | 已完成 | `parallel_enumeration` 4 项；Logical import 与导入基准回归 | 无 |
| FJ-04 | Analysis worker 在 merged 标记前完成 Fjall 输出 | 已完成 | staging Analysis merge 与 Logical 导入回归 | 无 |
| FJ-05 | 独立重提取的 Artifact、Timeline、checkpoint 可一起发布，失败保留旧输出 | 已完成 | `app-services --lib` 中原子输出、取消、读错和写错回归；浏览器 WAL 替换回归 1 项 | 大型私有样本资源测量未执行 |
| FJ-06 | 案件指标从 Fjall 读取 Catalog、Artifact、Timeline 数量 | 已完成 | `ready_source_aggregation` 6 项及服务单元测试 | 无 |
| FJ-07 | Analysis candidate 读取 Fjall Catalog | 已完成 | `analysis_candidate_discovery` 6 项、平台编排 17 项 | 无 |
| FJ-08 | 报告计数读取 Fjall Catalog | 已完成 | 报告单元测试和 source 报告路径测试 | 无 |
| FJ-09 | JSON/HTML 导出行从 Fjall 分页读取 | 已完成 | 报告 provenance、CSV 与 source analysis 回归 | 无 |
| FJ-10 | Graph provenance 从 Fjall Artifact 读取 | 已完成 | Graph provenance 与平台完整性测试 | 无 |
| FJ-11 | 浏览、行分页、定位和预览元数据从 Fjall Catalog 读取 | 已完成 | `file_browser_fjall`、`file_service_real_test` 10 项、`partition_index_materialization` | 无 |
| FJ-12-A | 新案件和 source schema 不创建三类 detail 表；旧 source detail schema 被拒绝 | 已完成 | `connection_test` 8 项；`source_migration_version` 3 项 | 无 |
| FJ-12-B | 三类 detail 写入 Fjall；保留未知加密状态和分区信息 | 已完成 | `fjall_source_detail` 4 项；source Repository 原生适配器测试 | 无 |
| FJ-12-C | 多分区、根折叠、重复执行、取消与来源隔离正确 | 已完成 | staging merge 12 项、分区折叠 3 项、并行枚举 4 项、双来源隔离 1 项 | 私有真实镜像未运行 |
| FJ-12-D | Catalog 任务游标排序、快照与来源隔离稳定 | 已完成 | 导入 benchmark 分页 oracle 与并行 RAW/E01 比较测试 | 无 |
| FJ-12-E | Artifact 属性筛选、时间排序与跨页读取稳定 | 已完成 | Artifact Repository 页测试、Linux 摘要、EVTX 和 Plugin 摘要回归 | 无 |
| FJ-12-F | Artifact/Timeline 的记录、索引、计数和 revision 原子更新 | 已完成 | Fjall Artifact/Timeline 测试、Analysis 输出替换和失败注入单测 | 真实掉电样本未执行 |
| FJ-12-G | 导入流程可达 Catalog、Search、Artifact、Timeline、Graph 和 seal | 已完成 | Logical benchmark 与双来源实际导入；Search、内容索引和 Graph 断言 | 公共小 E01 不含可导入文件系统 |
| FJ-12-H | Analysis 重提取及 checkpoint 一致发布 | 已完成 | `app-services --lib` 原子输出测试、`browser_wal_pipeline`、`registry_page_log_pipeline` | 超大私有样本资源边界未测 |
| FJ-12-I | 所有业务 Repository 消除 source detail 的 SQLite fallback | 已完成 | 生产 Repository SQL 扫描仅余 partition/NTFS staging；FileRepo、ArtifactRepo、TimelineRepo 在缺少 source identity 或 generation 时失败关闭；Correlation、Systemd、ContentIndex、NetworkFact、Graph file projection 和 Timeline aggregation 均移除 source detail SQL fallback | 无 |
| FJ-12-J | 缺失 generation、重新打开、取消、失败重试、删除和进程终止行为正确 | 已完成 | `source_detail_repo_fail_closed` 2 项、案件服务 32 项；Fjall process recovery 与 generation 测试 | PVE/Kubernetes 私有镜像未运行 |
| FJ-12-K | 大 Catalog 的计数、排序、分页及导入缓存受界限约束 | 已完成 | Fjall 单页上限 8192（包含内部 lookahead）；文件浏览排序、跳转、根目录和子目录窗口上限 8192；Artifact 查询集合上限 100000、分页上限 1024；Timeline 报告、Correlation Artifact ID 和 FileRepo 全量查询均有界；分页与超限失败回归通过 | 无 |
| FJ-12-L | 相关测试、Clippy、格式和架构门禁通过，文档及分批提交完整 | 已完成 | `app-services --lib` 1167 通过、16 忽略；`persistence-sqlite --lib` 140 通过、1 忽略；Fjall Catalog 8 项、文件浏览、Artifact 超限、source detail fail-closed 回归通过；格式、Clippy、模块规模、函数规模、测试布局和 SQL 边界门禁通过；修复按功能分批提交 | 私有 E01、PVE、Kubernetes 和真实掉电样本未执行 |
| FJ-12-M | 分析输出由 worker 并发写 Fjall，SQLite 只提交生命周期控制 | 代码验收完成 | `concurrent_analysis_outputs` 并发 4 worker 写入 400 Artifact 与 400 Timeline；核心检查点一次性重建且 revision 各为 1；EVTX、Browser、Plugin 替换和失败发布回滚回归通过 | 私有 Windows/Linux 样本性能复测尚未完成 |

## 当前验证

- `cargo test -j 1 -p app-services --lib`：1167 通过、16 忽略、0 失败。
- `cargo test -j 1 -p persistence-sqlite --lib`：141 通过、1 忽略、0 失败；Correlation、Artifact、Timeline Repository 夹具已迁移到真实 source.db + Fjall。
- `cargo test -j 1 -p app-services --test timeline_cursor_snapshot`：2 通过、0 失败。
- `cargo test -j 1 -p app-services --test file_browser_fjall`：1 通过、0 失败。
- `cargo test -j 1 -p persistence-sqlite --test source_detail_repo_fail_closed`：2 通过。
- `cargo test -j 1 -p app-services --test import_benchmark`：12 通过、4 忽略、0 失败。
- `cargo test -p app-services linux_summary --lib`：3 通过、0 失败；覆盖 Artifact 单次聚合、来源缓存首页、family 索引下一页、Catalog 元数据与 kernel 版本恢复。
- `cargo test -p app-services file_service::source_read --lib`：18 通过、0 失败；覆盖无 FileEntry 的 `mft` inode 与分区 descriptor 快路径。
- `cargo test -p app-services --test linux_identity_pipeline raw_ext4_import_catalog_and_artifact_identity_agree_on_primary_file_and_symlink`：1 通过、0 失败；验证导入 finalize 预热 Linux 摘要。
- `cargo test -p app-services --test page_queries_read_only`：6 通过、0 失败；Linux 摘要查询继续使用只读来源句柄。
- 真实 Linux `检材3.E01`：Artifact `49,461`，已有摘要缓存的新进程读取 `1,697 ms`；导入后调试构建预热 `15,651 ms`，预热位于导入完成阶段。
- 定向导入、staging、Carving、文件浏览、Timeline、CephFS、来源发布和来源隔离测试均通过；各命令结果详见本次修复记录。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、模块规模、函数规模、测试布局三项仓库门禁通过。
- 完整 `cargo test -p app-services --no-fail-fast` 由于 Windows 为 144 个集成测试目标逐个链接，执行时间过长而停止；相关测试目标随后分组执行。没有将该完整命令标记通过。
- 公开 `tiny.E01` 完整导入测试实际触发，但探测结果为 0 个支持的文件系统分区；这份样本不能验证 RAW/E01 文件系统导入。
- 完整 `persistence-sqlite` 集成测试套件、完整工作区测试、私有 E01、PVE 与 Kubernetes 样本和真实掉电样本未在本次执行。
