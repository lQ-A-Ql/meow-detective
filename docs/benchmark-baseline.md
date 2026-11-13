# 性能基准与验收协议

## 执行入口

`scripts/run-benchmark.ps1` 通过桌面测试程序调用当前生产 API。每个场景使用独立进程，准备独立案件与数据源，准备数据的过程排除在测量区间之外。

```powershell
./scripts/run-benchmark.ps1 -Scenario all -Runs 3 -WarmupRuns 2 -FixtureLevel small
./scripts/run-benchmark.ps1 -Scenario timeline_native_facets_narrow -Runs 5 -FixtureLevel medium -BuildProfile release
./scripts/check-benchmark-regression.ps1 -StructureOnly
./scripts/check-benchmark-regression.ps1 -FixtureLevel small -Runs 1
./scripts/check-benchmark-result-validation.ps1 -ResultsPath <实际结果文件> -Runs 1 -FixtureLevel small
```

`Runs` 为 1 至 100，`WarmupRuns` 为 0 至 10。`all` 执行完整场景集合，`smoke` 执行六个核心查询和导出场景。省略预热应显式传入 `-WarmupRuns 0`。新进程不代表设备缓存已清空，结果中记录实际的进程隔离与预热条件。

需要链接的 Cargo 命令在 Windows Visual Studio developer 环境执行。所有中间文件使用仓库内已忽略的 `output/` 目录。脚本不删除构建目录。

## 场景和规模

场景名称与规模的唯一配置为 `scripts/baselines/performance-benchmark-scenarios.json`。Rust fixture 与 PowerShell 验证器读取同一文件。

| 场景 | 测量对象 |
|---|---|
| `search_query` | 当前案件文件搜索首屏 |
| `file_tree_expand` | 当前案件文件树 |
| `file_paginate` | 当前数据源文件列表分页 |
| `timeline_filter` | 当前案件 Timeline 类型筛选 |
| `artifact_list` | 当前案件 Artifact 类型分页 |
| `report_export` | 当前案件 HTML 报告生成及实际输出文件 |
| `reprocess_generation_clone` | 带取消和进度接口的独立代际复制 |
| `timeline_native_cursor_p50` / `timeline_native_cursor_p256` | 分页遍历全部指定类型事件 |
| `timeline_native_facets_narrow` / `timeline_native_facets_full` | 计数、时间范围和直方图 |
| `artifact_native_facets` | families 与 counts 两个案件级请求 |
| `case_search_cross_source` | 跨源文件搜索首屏及 continuation |
| `case_search_cross_source_offset` | 有界 offset 文件搜索 |
| `correlation_paths` | 250 个 Artifact 与文件目录的关联 |
| `file_header_reads` | 经实际文件头读取路径使用描述符缓存 |

| 配置项 | small | medium | large |
|---|---:|---:|---:|
| 核心场景实体文件 | 4 | 5004 | 20004 |
| 代际复制键值 | 2000 | 100000 | 1000000 |
| Timeline 分页事件 | 1000 | 10000 | 100000 |
| Timeline facets 事件 | 2000 | 100000 | 1000000 |
| Artifact facets 记录 | 2000 | 100000 | 1000000 |
| 关联文件 | 2000 | 20000 | 100000 |
| 文件头读取文件 | 256 | 20000 | 100000 |
| 搜索源数量 | 8 | 8 | 64 |
| 每个搜索源的文件 | 128 | 1000 | 1000 |

文件场景创建实体文件并经过生产导入流程。Timeline 与 Artifact 场景通过原生存储接口准备确定性记录，并对实际查询结果断言。代际场景使用真实 SourceStore 写入、复制、读取和持久化。场景不使用 IPC mock 或预设返回结果。

代际场景分别记录 payload 规模和完整源库存。准备完成后，以有界快照遍历统计包含 Batches 回执的总条目、总逻辑字节与非空 keyspace；完成进度必须与这些实际总量精确一致。

## 测量结果

结果格式为 `schemaVersion=2`，测试程序版本为 `2026.11`。每次测量保留：

- `elapsedNs`、`elapsedMs`；场景级 p95 使用 nearest-rank。
- 每次运行的 Fjall point reads、扫描次数、扫描项数、逻辑读写字节、写事务及 SyncAll 次数。
- 分别记录 `fjallDatabaseOpens`、`sourceSessionOpens`、`searchIndexOpens`、`querySessionOpens`。
- `rankedHits`、`mergeComparisons`、`pathNormalizations` 及场景实际返回行数。
- 2 ms 间隔采样的区间 RSS、开始和结束 RSS，以及进程生命周期峰值。区间采样峰值与进程生命周期峰值分别保存。
- 进程 CPU 时间、传输字节、I/O 操作和句柄计数。进程传输字节不等同于设备物理 I/O。
- 源码 revision、包含暂存与未暂存修改的差异摘要、未跟踪源码摘要、编译模式、实际 rustc 版本和查询到的主机信息。

没有采集的场景指标保存为空值。实际为零的已采集计数保存零。验证器要求所有必要计数存在，并核对实际场景集合、样本数、规模和 p95。

## 阈值与结构门禁

`testdata/governance/v2-benchmark-baseline.json` 保留六个核心场景、三个等级的 18 个延迟预算。`artifact_list` 的预算对应 Artifact 列表查询。该文件的时间与测量值保留其原始来源；当前执行结果由新的运行产物提供。

| 场景 | small p95 ms | medium p95 ms | large p95 ms |
|---|---:|---:|---:|
| 文件搜索 | 200 | 1500 | 4000 |
| 文件树 | 100 | 800 | 2000 |
| 文件分页 | 50 | 200 | 500 |
| Timeline 筛选 | 100 | 2000 | 5000 |
| Artifact 列表 | 100 | 3000 | 8000 |
| HTML 报告 | 500 | 5000 | 15000 |

新增场景的确定性约束由 `scripts/lib/Benchmark.Thresholds.ps1` 验证，包括：

- clone 的一次 SyncAll 与足够的实际写入字节。
- Timeline 全遍历的结果数量和读取操作上限。
- facets 请求内每源会话数量与范围读取上限；Artifact 两个端点分别计为两次请求。
- 搜索 continuation 的索引复用、排名条数与堆比较数量。
- 关联路径规范化次数与有效关联结果。
- 文件头实际读取数量。

`-StructureOnly` 验证配置与完整、唯一的 18 项预算集合。实际运行模式还检查全部指定场景的测量证据。`-ResultsPath` 只接受当前格式，规模必须与指定等级配置一致。

发布时应使用指定参考机及相同构建模式进行比较。新算法的延迟目标需要实际重复运行支持；完整性、取消、只读证据和稳定排序始终独立验收。

## CI 与证据保存

Backend CI 执行 small 的完整运行与结果验证自测。定时任务执行 large，手动任务可以选择 small、medium 或 large。CI 上传原始日志与结构化结果。

默认本地结果位于 `output/performance-benchmarks/`。每次运行生成独立名称，保留原始 stdout/stderr。结果验证自测首先验证实际文件，再检查样本缺失、计数缺失和错误规模会被拒绝；自测不会生成成功的性能测量记录。

## 导入与真实样本

NTFS 导入使用 `scripts/run-import-profile.ps1`；固定可执行文件对比使用 `scripts/run-import-comparison.ps1`。比较定义明确声明输入、版本、analysis mode、worker 数和投影 workspace 估算值。投影 policy 报告 `workspaceKiB`、`reservationBytes` 与 `estimatedWorkspace` 类型。

导入报告区分端到端耗时、各 phase 耗时与进程峰值。包含关系的 phase 不相加。实际输入的 SHA-256、源码/可执行文件摘要和语义 oracle 必须一致，取消后需验证资源释放和发布状态。

真实 E01、RAW、PVE 等私有样本保持显式 opt-in。私有路径、文件名、hash 和原始日志存放在本地输出或受控 CI artifact。公开生成 fixture 的通过结果不替代真实大镜像验收。

`real_sample_performance_acceptance` 提供 Windows/Linux E01 的完整导入与查询验收。分别设置 `FORENSICS_PERF_WINDOWS_E01` 或 `FORENSICS_PERF_LINUX_E01`，并设置独立的 `FORENSICS_PERF_OUTPUT`；`TEMP`、`TMP` 必须位于该输出目录内。测试使用默认自动分析流程和两个 worker，保留案件及 JSON 报告，逐项验证文件列表、Timeline/Artifact facets 与分页、文件搜索和预览。

```powershell
cargo test -p app-services --test real_sample_performance_acceptance windows_real_sample_performance_acceptance -- --ignored --exact --nocapture
cargo test -p app-services --test real_sample_performance_acceptance linux_real_sample_performance_acceptance -- --ignored --exact --nocapture
```

报告区分 `passed`、`passedWithWarnings` 和 `failed`，记录自动分析警告、可重试失败、构建模式、进程内存及查询结构计数。查询测量排除独立 oracle 校验；检测到后台任务或源版本变化时，测量无效。E01 分段文件大小与修改时间的前后校验只证明元数据保持相同，不能替代完整内容摘要。

分析工作阶段覆盖发现、预加载、代际复制、处理、计数重建、封存、发布、结果构建、审计、定位信息保存和完成处理。总持久化耗时包含结果构建与后处理，分类完成事件在这些操作结束后发送。Linux 来源信息另外记录 `sourceFactsCollect`、`sourceFactsPersist` 及包含两者的 `sourceFacts`，对象计数表示来源信息聚合对象数量。

无条件 Artifact 数量查询使用当前快照的类型计数；包含字段过滤条件的查询保留记录级匹配。Timeline 直方图使用完整日、月、年的计数汇总与边界时间索引，整数秒范围无需逐条读取事件正文；纳秒范围保留精确边界校验。结构回归同时检查查询结果、记录读取次数和损坏索引的分类错误。

保留案件的诊断入口同样属于显式启用的测试：设置 `FORENSICS_ANALYSIS_CASE_ROOT`、`FORENSICS_ANALYSIS_SOURCE_ID` 和工作区内的 `FORENSICS_PERF_OUTPUT`，并将 `TEMP`、`TMP` 置于输出目录内。`retained_source_registry_metadata` 只读取来源目录元数据；`retained_source_chromium_metadata` 只报告密钥字段的存在性、类型、长度和编码有效性，不输出密钥值；`retained_source_analysis_diagnostic` 重新执行该来源的分析并保存完整 DTO、警告、阶段记录及代际绑定。这些入口均要求来源已登记于指定案件；分析诊断在结果保存后仍检查分析失败数量。
