# Benchmark 基线说明

## 1. 目标

V2 的 benchmark 目标不是展示漂亮数字，而是给出可复现、可比较、可回归阻断的性能基线。

所有 benchmark 都必须回答以下问题：

- 在什么数据集上跑的
- 在什么机器上跑的
- 冷启动还是热启动
- 用什么版本代码跑的
- 哪些指标是发布门槛

## 2. 数据集分级

### 2.1 Small

用途：

- 默认 PR 验证
- parser / service 基础回归

典型规模：

- tiny RAW / E01 / logical fixture
- 少量 artifact
- 小型文件树

### 2.2 Medium

用途：

- Nightly 基线
- Release candidate 主要性能对照

典型规模：

- 可公开 medium fixture
- 完整文件树、索引、时间线
- 多工件联合分析

### 2.3 Large

用途：

- release 前手工或定时专项验证
- 真实规模边界说明

典型规模：

- 真实或私有大镜像
- 百万级文件枚举场景
- 大索引、大时间线、大批量 artifact

## 3. 指标口径

所有 benchmark 至少记录：

- 时间
- Git 提交或版本标识
- 目标链路
- 数据集等级
- 样本名称
- 样本大小
- 冷启动 / 热启动
- p50 / p95 耗时
- 峰值内存
- 宿主机说明

建议额外记录：

- CPU 型号
- 核心数
- RAM
- 存储介质类型
- Windows 版本

## 4. 核心覆盖链路

V2 benchmark 至少覆盖：

- 导入
- 文件树首展开
- 文件分页
- 搜索查询
- 时间线过滤
- 核心 artifact 提取
- 报告导出
- 长任务取消

镜像适配器专项至少增加三类轻量测量：

- RAW/dd/img 顺序读取与随机 seek：报告逻辑字节吞吐、p50/p95 延迟和峰值 RSS；
- flat VMDK descriptor 解析、首扇区/跨扇区读取与 block provider：同时记录 descriptor/extent
  physical bytes、逻辑容量和 manifest hash 阶段耗时；不得只用 descriptor 文件大小计算吞吐；
- ISO9660/Joliet 首次 PVD 探测、根目录展开与 bounded 文件 preview：分别记录裸 ISO、E01
  嵌套和 flat VMDK 嵌套的 reader 准备时间；目录深度和 entry 上限触发时记录 typed failure。

适配器 benchmark 只测只读路径，不转换为完整 VMDK、不 materialize sparse 容器，也不把
VMware 启动耗时归因于 ISO 文件系统 reader。当前没有公开 medium 镜像，适配器数字只能
作为本机/CI artifact，不能写入支持矩阵作为发布阈值。

## 5. 默认阈值

以下阈值作为 V2 当前默认门槛：

| 场景 | 指标 |
|---|---|
| medium 搜索热查询 | p95 ≤ 1.5s |
| medium 时间线筛选 | p95 ≤ 2s |
| medium 文件树首展开 | p95 ≤ 800ms |
| large 搜索热查询 | p95 ≤ 4s |
| large 时间线筛选 | p95 ≤ 5s |
| large 文件树首展开 | p95 ≤ 2s |
| 取消任务 UI 确认 | ≤ 500ms |
| 后端协作停止 | ≤ 3s |
| small 1 MiB mbox 解析 | ≤ 1.0s |
| small 10-message 合成 PST 解析 | ≤ 100ms |
| medium 10 MiB PST 解析 | ≤ 10s（待流式解析器完成后实测） |
| 私有 Windows E01 导入 | 总耗时中位数 ≤ 45s，枚举中位数 ≤ 30s，峰值 RSS ≤ 1024 MiB |

如有调整，必须同步更新 `testdata/governance/v2-benchmark-baseline.json`、支持矩阵和文档索引中的字段承诺。

Windows E01 导入门禁还要求至少 90,000 条文件记录、枚举吞吐至少 6,000
条/秒，并验证 NTFS 树形、导入完成后的 Timeline 和系统信息结果。搜索索引优化不得通过
减少文件记录、跳过 Timeline 或放宽解析告警来满足耗时阈值。

## 6. 运行分层

### 6.1 PR

- 只跑 small
- 不阻塞在大样本

### 6.2 Nightly

- 跑 small + medium
- 记录趋势

### 6.3 Release candidate

- 跑 small + medium + large
- 生成正式 benchmark 摘要

## 7. 输出位置

推荐输出：

- 私有 benchmark 输出只保存在工作站或 CI artifact；不作为 Git 技术文档提交。
- `artifacts/import-profiles/`

建议命名：

- `YYYY-MM-DD-parser-bench.md`
- `YYYY-MM-DD-search-bench.md`
- `YYYY-MM-DD-report-export-bench.md`
- `YYYY-MM-DD-large-case-summary.md`

## 8. 与发布治理的关系

以下情况必须阻断候选发布：

- benchmark 数据缺失
- 核心链路性能明显退化且无豁免说明
- 数据集说明、宿主机说明、版本标识缺失
- 文档与实际 benchmark 输出不一致

## 9. 当前产品内落地（2026-06-13）

`/v2` 治理工作台已经承接第一版 benchmark 与发布门禁可见链路：

- `benchmark.scenarios`
- `benchmark.requiredChecks`
- `releaseGates`
- `releaseScorecard`

当前仍是“治理快照驱动”的展示模式，不是自动 benchmark 平台；但这条链路已经为后续真实采集与候选发布 gate 留好了稳定 DTO 入口。

当前 `/v2` 中与 benchmark 直接相关的发布门禁已经细化为：

- `benchmark-thresholds`
- `releaseScorecard.breakdown.performance`

当前产品内还会把每个必需 benchmark 校验项显式展开为：

- `datasetLevel`
- `scenario`
- `thresholdP95Ms`
- `measuredP95Ms`
- `status = covered / missing / exceeded`

并汇总：

- `coveredRequiredCount`
- `missingRequiredCount`
- `exceededRequiredCount`

从 2026-06-13 起，`/v2` 中的 benchmark 基线不再只写死在 `v2_governance_service.rs`，当前仓库事实源为：

- `testdata/governance/v2-benchmark-baseline.json`

其中 `requiredChecks` 阈值清单也以内嵌 JSON 为事实源，不再仅由 Rust 常量定义。

当前产品内 gate 规则会检查：

- medium 文件树首展开
- medium 搜索热查询
- medium 时间线筛选
- large 文件树首展开
- large 搜索热查询
- large 时间线筛选

若场景缺失，当前口径记为 `warning`；若场景超出默认阈值，当前口径记为 `blocked`。

## 10. 大文件提取测量协议

大文件提取应分别计量 reader/filesystem 准备、复制与 SHA-256、目标同步及原子发布。
私有样本的路径、文件名、hash、吞吐和原始日志只保存在工作站或 CI artifact，不写入 Git 文档。

当前实现对满足条件的 E01/RAW/flat VMDK NTFS 文件可使用两个独立 reader、`4 MiB` 分片和最多四个在途块；条件不足时回退串行。评估时必须分别报告：

- 串行与并行模式、文件系统和数据源种类；
- 端到端耗时、复制阶段耗时、吞吐、峰值 RSS/CPU；
- 输出长度和独立 SHA-256 复核结果；
- 冷/热缓存状态、软内存上限和回退原因。

单次结果不构成发布阈值。只有在受控缓存条件、重复运行 p50/p95 与提取窗口高频 RSS/CPU 采样齐备后，才可更新治理事实源中的 large dataset gate。

## 11. NTFS 导入细粒度测量协议

`scripts/run-import-profile.ps1` 使用与普通导入相同的 application-service 用例，
在独立案件中测量 RAW/E01 NTFS 导入。Windows 下需先进入 Visual Studio developer 环境。

```powershell
powershell -ExecutionPolicy Bypass -File scripts/run-import-profile.ps1 -SyntheticRecords 100000 -Runs 3
powershell -ExecutionPolicy Bypass -File scripts/run-import-profile.ps1 -FixturePath $env:FORENSICS_IMPORT_FIXTURE -Runs 3 -Release
```

合成样本是固定 record 布局、resident 文件和空目录索引的数据卷；它验证扫描规模和结果一致性，
不能代表真实大目录 INDX、碎片化、复杂属性或真实 E01 压缩率。测试 profile 与 release
profile 必须分开比较；脚本默认将 OS 缓存条件标记为 `uncontrolled`，不声称是冷缓存。

报告版本为 `schemaVersion=1`。`report.samples` 保留 source 级 wall time 与各 partition
记录；`partitionAggregates` 是相同 phase 的分区工作量之和，不能当成并行 source wall time。
`inclusive=true` 的 workflow/enumeration/analysis/timeline/fallbackEnumeration 不再与子 phase 相加。

MFT 读取、解析、stage、目录 reader 初始化、大小校正、目录回填、路径解析、验证和 commit
分开计时。scanned records、parsed records 与实际 staged rows 各自记录；逻辑读取字节
不等于 E01 物理 I/O。没有物理 I/O 观测时省略该字段，未执行的索引构建为 `notApplicable`，
不是零耗时成功。RSS end 与进程 lifetime 高水位分别记录，oracle 快照在采集 import RSS 后构造。

`timelineEvents` 和 `timelineGraph` 进一步区分事件批量写入与图谱投影；后者包含
分页查询、节点/关联写入及各批次提交。它们使用投影统计的毫秒观测值转换为纳秒字段，不代表纳秒精度。
`timeline`、`timelineEvents`、`timelineGraph` 均为 inclusive wall time，不与子阶段相加。
事件阶段的 clear/cursor/insert/indexes/commit，以及图谱阶段的 page/nodes/edges/commit
各自记录互不重叠的 `Duration`；其中 indexes 包含空表检查、暂缓和重建索引。
仅新增内部 benchmark phase，不增加业务 IPC 事件。

首次文件活动投影进入事务时若 Timeline 为空，会在现有事件事务的 savepoint 中暂缓普通
非唯一二级索引，并在提交前按原始 SQL 全部重建。主键及唯一索引始终保留；已有事件的重投影
保持索引在线。写入或重建出错、取消及外层事务回滚均恢复数据和索引，WAL 读连接只能看到
已提交的完整状态；无需 schema 迁移或跨事务恢复清单。图谱仍逐批提交并保留既有重试语义。

`workflow` 覆盖导入用例入口到返回，包括 finalization 之后可能执行的自动制品提取；不包含
队列等待、测试案件创建或 oracle 查询。旧 E01 runner、日志和 IPC 进度事件继续兼容。

每次运行输出 catalog、graph、timeline 的行数及规范化 SHA-256。oracle 映射本次生成的
case/source/partition-root ID，以及按分区和路径确定的 Generic FS UUID；原生 MFT 文件 ID
保持不变。重复 Generic 路径会使 oracle 显式失败。排除 Graph 的运行时 created_at；路径、属性、
null、取证时间和来源字段保留。重复运行摘要必须一致，缺少必要 workflow/MFT 数据使脚本失败。
默认回归另比较 profiling 开/关的全部 catalog 字段、Graph 和 Timeline，并验证合成 E01/RAW
逻辑字节及 resident 文件范围读取一致。

报告、私有证据路径、摘要和原始日志写入本地 `artifacts/import-profiles/`；不写入版本库。
`selectedInputFileSha256` 仅标识传入的文件，不能冒充 E01 全部 segment 的容器集合摘要。
这些测量暂不修改现有发布阈值。

## 12. NTFS 导入矩阵与流式 oracle

`scripts/run-import-benchmark.ps1` 是重复测量入口。默认固定一次合成输入，分别执行 RAW/E01
的 1/2/4 workers，每个条件先预热一次，再测量五次。Cargo 只编译一次；fixture 准备和
每次导入均使用独立进程及独立临时案件。编译限制为两个 jobs，并在测量前结束。

```powershell
# S/M/L 合成规模分别使用 100000、500000、1000000 records；正式比较使用 Release。
./scripts/run-import-benchmark.ps1 -Records 100000 -Runs 5 -WarmupRuns 1 -Release
./scripts/run-import-benchmark.ps1 -FixturePath $env:FORENSICS_IMPORT_FIXTURE -Runs 5 -Release
# 快速验证入口和完整逻辑结果，不据此设定吞吐目标。
./scripts/run-import-benchmark.ps1 -Records 20003 -Runs 1 -WarmupRuns 0
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/import-benchmark.ps1
```

输入限于单文件裸 RAW/dd/img 或从 `.E01` 开始的完整数值分段集。虚拟磁盘 adapter、
非首段、分段编号不连续均拒绝。矩阵开始和结束时复核每个分段的长度、修改时间和 SHA-256；
Rust/SQL 源文件、TOML 配置、锁文件、PowerShell 脚本及 test executable 同样复核，
变化使本次矩阵失败。真实样本只从显式 `FixturePath` 参数读入，不搜寻工作站证据。

报告保存在本地 `artifacts/import-benchmarks/matrix-*/report.json`，记录 CPU、内存、OS、
toolchain、代码文件摘要、输入分段 manifest、build profile、随机 seed 和实际执行顺序。
每轮使用可复现的随机排列，每个条件恰好执行一次。`workersRequested` 是预算，
`workersResolved` 是机器上解析后的预算；二者均不表示已观测到的同时工作线程数。

预热轮也必须通过 oracle，且保留原始报告，但不参与统计。缓存口径为 `warm-by-full-import`
或 `uncontrolled`；输入哈希本身会读取证据，脚本从不清除 OS 缓存，因此不能宣称冷缓存。
测试 profile、release、不同缓存条件、不同硬件和不同输入之间不能直接推导优化倍数。
报告内的 median 与 nearest-rank p95 只汇总同一输入和 worker 条件。

`schemaVersion=2` / `oracleVersion=2` 使用 SQLite 文件临时排序和逐行 SHA-256，避免把整个
Catalog/Graph/Timeline 放入 Rust 内存。按规范化后的唯一主键排序；Graph 顺序固定为 nodes
后接 edges；单元格仍保留 SQLite 类型。生成 UUID 的规范化规则与旧 oracle 相同，映射
超过 100k entries 或估计 64 MiB 时显式失败。v1 按整行排序，摘要与 v2 不可直接比较。
所有重复轮次及 workers 必须保持行数和摘要相同；合成 RAW/E01 还必须互相一致。

`workflowSeconds` 使用 source workflow wall time，排除案件创建、oracle 和清理。
`importProcessPeakRssMb` 是导入完成、oracle 之前捕获的进程 lifetime RSS 高水位；
不代表独立扣除启动开销的工作集。外部每 100 ms 采样的 handles 和 working-set 高水位
覆盖整个子进程，包括 oracle 和清理；短暂峰值可能漏采。两种口径分开报告。
阶段汇总明确标为 `source-wall-inclusive` 或 `partition-work-sum`，不得相加推算总耗时。

合成数据卷仍以 resident 文件和空根目录索引为主；真实 INDX、碎片化、多分段 E01、
恢复/崩溃注入和发布阈值需要独立验收。矩阵入口通过不等于 RAW/E01 加速目标已经达成。

### Timeline 空表索引维护对照（2026-10-16）

优化前为 `26592adc` 加相同细分计时；前后使用冻结的 Release 可执行文件，均为
Windows / metadataOnly / 4 workers。每条件预热 1 次、测量 5 次，并打乱各轮执行顺序。
以下均为中位数，单位为秒；每轮 Catalog / Graph / Timeline 行数及完整 SHA-256 相同。

| 输入 | 总导入（前 → 后） | Timeline（前 → 后） |
|---|---:|---:|
| 十万条 RAW | 11.544 → 10.976 | 7.294 → 6.596 |
| 十万条 E01 | 11.424 → 11.122 | 6.986 → 6.584 |
| 真实 E01 | 20.594 → 21.030 | 2.918 → 2.689 |

真实 E01 的 Timeline 局部收益未转化为整体收益，总导入中位数增加约 2.1%，不能宣称全面提速。
百万条 RAW 只做单次压力对照：总导入 143.988 → 126.591，Timeline 95.031 → 80.116，
峰值 RSS 1150 → 1407 MB，均低于本轮 2048 MB 门限。单次结果不作为稳定加速比例。
原始输入、源码和冻结二进制在测量前后均复核；OS 缓存未清除。

本地完整报告、p95、阶段分解及源码补丁保存在
`artifacts/timeline-optimization/20261016/atomic-index-acceptance.md` 与其同目录。
事务、索引重建失败及强制终止恢复见 `persistence-sqlite/tests/timeline_bulk_indexes.rs`。
下一步对照图谱节点写入与批次提交，并继续核对真实 E01 的整体耗时。

### 分阶段优化的测量协议

`scripts/baselines/import-optimization-protocol.json` 冻结正式测量至少 5 轮/1 预热、Release、
2048 MiB 导入 RSS 门限、热点/总耗时收益目标与其他场景 median/p95 回退阈值。
协议和 `import-optimization-contracts.json` 均计入源码输入摘要，运行前后还独立核对协议 SHA。
契约清单锁定当前 oracleVersion=2 的导出列与允许的身份规范化，额外标出尚不在表摘要内的 scope、
Search、partition UUID 和 reader/range 断言；不能把表摘要等价解释为已经覆盖全部语义。

`run-import-benchmark.ps1` 的 `-RunKind` 为 Exploratory（默认）、Formal 或 Pressure。
Formal 缺 Release、轮数或预热即拒绝；Pressure 保留单次压力口径。报告的 assessment 同时检查
每条件轮数、唯一 round/sequence、warmup 分离、完整 oracle、失败状态与导入峰值 RSS，包含预热资源违规。
单版本报告只证明选定输入的测量/资源检查，不判定性能提升或完整发布矩阵通过。
显式传入空 FixturePath、目录、非首段或不完整 E01 集合必须在构建前拒绝，不能退回合成测试。

`scripts/new-import-benchmark-snapshot.ps1` 用当前 Cargo 构建结果捕获独立可执行文件、代码、协议、
工具链和显式输入 manifest，并在结束时复核。该入口只生成快照，不产生性能通过结论。
baselineStrategyReference 只描述冻结基线，不会隐式启用未来 Path/channel/cache/WAL 候选参数。
普通 RAW/E01 仍直接写 source.db 并通过 import_state 门控；派生源 build/seal/rename 是独立路径。
构建 helper 的 16384 页 checkpoint 常量不代表普通源连接也使用此值。

### 导入窗口与波次观测

`processUsage` 在 profiled workflow 前后捕获进程 CPU、OS read/write transfer 和 handles，
其 wallTimeNs 与 workflow 的 elapsedNs 相同，排除后续 oracle。CPU 使用 core-equivalents
（CPU 秒/墙钟秒）；2.0 表示约两个核心。进程计数包含同一进程其他并发工作，不能当作独占 job
归因或把重叠窗口相加。OS transfer 不是设备物理 I/O；handlesAtStart/End 也不是窗口句柄峰值。
API 不可用、省略字段、重复复用 profiler 窗口时，不补造零值。

`mftReadParseWall` 测量每个分区当前波次的实际 read/parse 完成窗口；`mftWriterWall` 测量
其后单 writer callback（包含 Catalog bookkeeping/进度）。当前 writer 在前一窗口等待，reader
在后一窗口暂停；这不是持续 channel 的 queue latency。两者仍属于分区观测，跨分区汇总为 work-sum。
`mftRead`/`mftParse` 是 worker 累计工作时间，不能直接与这两个 inclusive 父窗口相加。
`mftPathCompute` 与 `mftPathSql` 分开记录，父级 `mftPathResolve` 因此也为 inclusive。

`maxBufferedBatches/Records` 是单分区已读波次的驻留载荷计数，不是内存字节；聚合取单分区 max。
`mainTableUpdatedRows` 记录路径物化实际 UPDATE 的受影响行，聚合求和；不能当作物理写入量。
`sampledWalMaxBytes` 是批次/阶段边界采样到的 WAL 文件长度，聚合取 max，不保证捕获瞬时峰值。
MFT staging、路径 SQL、commit 与 Timeline Graph 结束处均有对应观测边界。
匿名 SQLite 排序文件峰值和设备物理 I/O 尚未采集，在 `observationLimits` 明确声明。
这些均为 oracleVersion=2 / report schemaVersion=1 的可选兼容扩展，不修改业务 IPC 事件。

### S1 样本与语义验收入口

`run-import-comparison.ps1` 接收显式 JSON definition：`schemaVersion=1`，`versions` 中每项含
`name/executable/sha256/revision/analysisModes`，`inputs` 中每项含 `name/path/logicalGroup`。
只运行冻结的 Release 可执行文件；不同版本/输入/worker/mode 在每轮按固定 seed 打乱，预热独立。
运行前后复核输入、源码、协议、definition 和二进制。报告逐轮落盘，异常立即失败并保留原始日志、
exit/timeout/process status 和短 TEMP；失败轮不进入成功汇总。重试必须新建整个矩阵，不能挑换失败轮。
正式矩阵中不要编译、编辑或提交；确认回退时另开五轮矩阵。单轮筛选与 Pressure 不能作收益结论。

新增的 `semanticOracle.version=2` 按 `import-optimization-semantic-oracle.json` 独立版本管理。
保留完整 v2 表摘要与身份规范化，只增加每列/64 个 ID 桶的变异定位、文件/目录/删除/parent/root 统计、
元数据及正文 Search 文档集合/固定查询、实际 source-scoped 预览与提取摘要。内容索引与元数据索引分开。
正文覆盖跳过原因、投影 revision/游标/幂等、E01 分段/分区/派生窗口/初始化长度字节语义另有定向测试。
Oracle 位于 workflow 和导入 RSS 采集之后；它的开销、进程退出与清理不计入导入时间。
Search 查询结果在磁盘临时库中按规范化 ID 排序后摘要，并拒绝重复文档 ID。v1 曾直接沿用 Search
的原始 UUID tie-break 顺序，在独立案件的相同排序键下产生假阳性，失败矩阵与 v1 契约提交均保留。
实际 API 排序、跨源合并、游标及 revision 行为由服务回归验证；不扩大身份规范化或更改 v2 表摘要。

原 resident/空根目录索引样本继续明确标识。`FORENSICS_IMPORT_BENCH_SCENARIO=indexed-fragmented`
启用独立的固定布局：真实 INDX、嵌套 parent、分散时间戳、deleted、ATTRIBUTE_LIST 外部 DATA、
两段非连续 DATA 与初始化尾部。生成器固定且没有随机时钟输入，RAW/E01 对应相同逻辑字节。
私有 inventory 测试只接收显式 `FORENSICS_IMPORT_FIXTURE`，默认 ignored。规模按实际 MFT 槽位和目录数登记，
记录完整分段 SHA 与容器/逻辑大小比；碎片报告只表示首 128 个有效记录抽样，不能视作全卷统计。
缺失、不适用和探测失败分别记录，不由合成集替代，也不构成完整发布矩阵通过。

S1 于 2026-10-17 完成校准。冻结测量版本为 `53f55c31`，5 个有效矩阵共 100 个独立子进程
（86 测量、14 预热）；完整 v2 表摘要一致，64 个当前候选还验证 semanticOracle v2。
最大导入峰值 1419 MiB，低于 2048 MiB。首轮 100K RAW median/p95 与中型 E01 p95 触线后，
各追加五轮确认：RAW median +1.73% / p95 +1.46%，中型 E01 -1.36% / -0.83%，均未再次触线。
首轮和确认报告分别保留；本轮没有生产策略切换，不宣称提速或零开销。

完整分析五轮代表集为合成 128-record RAW/E01；大型真实 E01 本轮只登记，完整性能矩阵未运行。
真实 DD 候选被 native FAT reader 以 invalid BPB 拒绝，IMG 候选 probe EOF，尚无已验证的真实 RAW 验收样本。
逐 task、样本和运行边界见本地 `docs/architecture-performance-stage-S1-20261016.md`，
原始报告及完整性核对索引见 `artifacts/stage-optimization/20261016/S1/acceptance.json`。

### S2 路径与索引生命周期验收

S2 于 2026-10-17 完成所选输入验收。冻结候选为 `811d7dc9`，对照 S1 的 `53f55c31`；
完整 v2 表摘要和 semanticOracle v2 保持不变。4 个矩阵共 92 次独立子进程：12 次筛选、
60 次正式测量、12 次预热、8 次压力。源码、协议、definition、输入分段及冻结 Release binary
均前后复核。正式条件为 workers=4、独立预热一次和测量五次；失败轮不替换。

| 输入 / 分析模式 | 基线 median s | 候选 median s | median 变化 | p95 变化 |
|---|---:|---:|---:|---:|
| 100K RAW / metadataOnly | 13.114669 | 12.971296 | -1.09% | +2.52% |
| 100K E01 / metadataOnly | 13.756376 | 13.216385 | -3.93% | -1.77% |
| 小型真实 E01 / metadataOnly | 2.072240 | 1.965240 | -5.16% | -5.50% |
| 中型真实 E01 / metadataOnly | 24.377009 | 24.409533 | +0.13% | -2.32% |
| 合成 128 RAW / fullContent | 0.812450 | 0.826391 | +1.72% | -0.20% |
| 合成 128 E01 / fullContent | 0.631652 | 0.623705 | -1.26% | +0.92% |

四个 metadataOnly 正式输入的路径 SQL median 下降 29.78%–38.77%，候选路径主表 UPDATE
计数均为 0。100K E01 和小型真实 E01 同时达到热点下降至少 10% 与 workflow 下降至少 3%
的门槛；其他正式条件未触发 median +3% / p95 +5% 的追加确认门槛。RAW、中型真实 E01
和 fullContent 代表集不宣称整链提速。workflow 含 commit、索引及 final checkpoint。

默认使用 ntfs-v3 窄路径投影与验证后的数值 merge；generic merge 保留。known v2 transient
由 staging owner 原子清除并重枚举，unknown layout 和 published source 拒绝替换。
`ntfs_parent` 仅在校验期维护；source bulk manifest 恢复前，读写 Ready 路由都拒绝打开。
writer batch 1024/2048/4096/10000 的五轮 native 实验无有意义差异，保留 10000 与整分区事务。
绑定/调用未成热点，不启用多行 INSERT；本轮没有持续 channel 或 worker 默认策略变更。

新增可选计数 `materializedPathRows` / `writerBatches` / `partitionCommits` 聚合求和；
`writerBatchMaxRecords` / `readChunkRecords` / `stagingLogicalBytes` 取单分区 max。
最后一项为路径物化结束时 `page_count * page_size`，不含 WAL，不代表 TEMP 峰值。
100K 的 ingest + path 已观测 DML 两版均为 199972 行：宽表 UPDATE 改为窄表 INSERT。
全部 SQL/触发器/索引写次数未采集，不据此宣称总写操作减少。100K E01 commit WAL
采样长度从 18732512 增至 19923832 bytes，workflow OS write transfer median 从
1335878132 变为 1335076088 bytes；没有普遍显著物理写量下降的结论。

全部轮次完整 C/G/T 和 semantic v2 在同 logicalGroup/mode 内一致；最大导入峰值 1421 MiB，
候选最大 1413 MiB，低于 2048 MiB。500K/1M RAW/E01 为单次压力，rich-100K 为单轮筛选；
不据此宣称稳定提速。fullContent 仅合成 128 代表集，大型真实 E01、真实 RAW 性能和完整
私有 artifact/plugin 发布矩阵未运行。新增校验成本、WAL 增长及匿名排序空间观察限制均保留。

阶段回归：app-services 库 1094 passed / 10 ignored、import_benchmark 12 / 4、Ready 路由 2 / 0，
persistence-sqlite 全 crate 391 / 4；相关 all-targets clippy 与架构守卫通过。这不是 workspace
全量或 S8 发布门禁。逐 task 决策见本地 `docs/architecture-performance-stage-S2-20261017.md`，
原始报告/日志摘要、oracle、成本和门槛复核见 `artifacts/stage-optimization/20261017/S2/acceptance.json`。
测量完成后仅调整 import-optimization 守卫以跟随实际引用的独立 SQL；前后 SHA 另行登记，
生产源码与冻结二进制保持测量版本，原失败日志保留。

### S3 身份、资源 admission 与 Release 验收（功能完成，性能回归开放）

S3 首批改动将普通导入的 SourceRuntime lease 接到 source DB、analysis staging merge
和 Ready 状态的事务提交点；来源失效时拒绝提交。独立 reader 打开前后检查 epoch，
RAW backing members 与 E01 segments 在重开和提交前验证元数据。元数据身份带
`metadata-v1` 前缀，绑定到 source_meta 以拒绝来源变化或缺少绑定的旧续导工作，
包括损坏 manifest 目录。该标记不是证据内容哈希。新 owner 已接手后，旧失败状态也不能
覆盖新结果；owner 次序不会随新 runtime 析构而丢失。未增加 UI 步骤或修改外部文件 ID。

持续 channel 仍未启用。依据固定 S2 原报告，即使把全部 `mftReadParseWall` 分区 work-sum
完全从 workflow 移除、保留单 writer 工作，所选正式输入所有轮次的节省上界也仅 2.2631%，
低于 3% 门槛；采用这个宽松上界也考虑了消除慢批波次屏障的空间。它是当前 warm-cache
workload 的 No-Go 预评估，不是 channel 实测，不代表新/冷缓存/慢 I/O 输入都没有收益。

普通 RAW/E01 的 reader、metadata、handle、worker 和阶段工作集 admission 已接入，
SourceRuntime publication fence 已覆盖 source DB、analysis staging merge、Ready 状态及
Ready 后自动 Linux 制品动作。当前提交为 `e668eae8`；功能门禁、E01 byte oracle、
publication、reader budget、module/function/test-layout 守卫通过。

S3 iteration-02 的 Release 矩阵已完成：原生 MFT 90/90，screen 24/24，focused 48/48，
full 24/24，pressure 8/8；所有 trial 语义 oracle 一致，最大 RSS 1417 MiB，低于 2048 MiB。
按协议独立确认后，medium E01 workflow median `+5.35%`（门槛 `+3%`）和 full128 E01
workflow P95 `+9.33%`（门槛 `+5%`）仍未关闭；详见
`artifacts/stage-optimization/20261017/S3/iteration-03/targeted-followup.json`。
因此 S3 的资源与发布保护可以进入后续阶段，性能验收不能宣称整体通过，也不能修改阈值
或 verifier 来掩盖这两项回归。持续 channel 依据当前 warm-cache 上界保留 No-Go；
PVE、preview 和多任务 RSS 组合仍属于 S5/S8。

已完成的 phase 级复核见本地 `artifacts/stage-optimization/20261017/S3/iteration-03/phase-profile.json`。
medium E01 的回归分布在 analysis、enumeration、graph 与 MFT directory backfill，
不是单独的 snapshot 校验热点；候选 RSS 中位约 1,115 MiB，对基线约 982 MiB，增加约
130 MiB。下一轮应先补充 analysis worker、search/index、SQLite staging merge 和
E01 reader admission 的独立计时与等待计数，再针对确认后的热点做最小修复；full128 E01
优先按 P95 抖动复核，不把一次较好的中位数当作已解决。

benchmark harness 已支持 `FORENSICS_IMPORT_BENCH_PHASE_TRACE=1`，只在显式启用时将
现有 import phase 事件写入测试日志；不改变生产默认事件流、数据库或 oracle。medium
E01 的一次 trace 位于 `S3/iteration-03/phase-trace-medium.log`，full128 E01 位于
`phase-trace-full128.log`。full128 样本只有 114 条记录，analysis worker 实际收敛为 1，
因此不能用它证明并发资源路径的稳定 P95；后续优先复测 medium E01 的 phase trace。
medium E01 第二次 trace 的 workflow 为约 93.5 秒，第一次约 95.4 秒；两次阶段比例
稳定为 enumeration 约 27 秒、post-import metadata/index 约 49 秒、timeline 约 3.8 秒。
这说明当前回归不是一次性 snapshot 校验抖动，后续应优先拆解 analysis/search/index
与 enumeration 内部等待，而不是继续调整 publication fence。

随后使用 Release benchmark executable 重跑同一 medium E01，得到 workflow 23.35 秒、
enumeration 4.42 秒、metadata search index 6.52 秒、timeline 2.91 秒、峰值 RSS 1113 MiB；
完整诊断记录为 `artifacts/stage-optimization/20261017/S3/iteration-03/phase-trace-medium-release.json`。
此前 debug trace 的 93–95 秒不再作为性能证据。Release trace 只用于定位阶段，不替换
iteration-02/03 的正式矩阵或回归结论。
