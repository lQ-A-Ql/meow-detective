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
