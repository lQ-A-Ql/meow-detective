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

如有调整，必须同步更新 `docs/v2-longterm-plan.md` 与发布评分卡。

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

- `docs/benchmark-results/`
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
