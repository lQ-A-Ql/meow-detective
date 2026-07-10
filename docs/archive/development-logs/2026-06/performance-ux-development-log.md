# 性能与交互优化开发日志

> 归档：2026-06 开发日志，仅用于历史追溯，不代表当前性能基线。

**项目**: Forensics Workbench - 性能与交互优化  
**开发周期**: 1 轮集中收敛  
**开发人员**: MiMo AI Assistant

---

## 📅 阶段 1: 事件契约与导入进度基线

### 任务

- [x] 统一导入进度与取消状态的 typed DTO
- [x] 锁定前后端一致的事件主题
- [x] 保留 legacy `job-progress` 桥接

### 完成内容

- [x] 新增并锁定事件主题 `import.phase_progress`
- [x] 新增并锁定事件主题 `import.partial_result`
- [x] 新增并锁定事件主题 `job.cancellation`
- [x] 新增并锁定事件主题 `cache.index_status`
- [x] 新增并锁定事件主题 `performance.report_ready`
- [x] 传输层 DTO 保持 `camelCase`，枚举线缆值保持 lowerCamelCase
- [x] TypeScript `EventTopic` 与 Rust 常量完成对齐

### 技术决策

**为什么保留 legacy bridge**:
- 现有任务与导入进度 UI 仍依赖 `job-progress`
- 新 typed 事件需要逐步接入，不能一次切断旧桥接
- 双轨并存可以先锁定契约，再平滑替换前端消费路径

**契约边界**:
- typed 事件负责更清晰的阶段、取消、部分结果、缓存与性能信号
- legacy 事件继续承担兼容职责，不扩大新范围

---

## 📅 阶段 2: 后端事件发射与取消持久化

### 任务

- [x] 从 Tauri 导入流水线发射 typed 阶段进度
- [x] 明确取消状态流转
- [x] 将取消状态写入作业持久层

### 完成内容

- [x] 现有 `phase=...` profile 被映射为 typed 导入阶段
- [x] 导入阶段覆盖 Attach、Probe、Enumerate、MergeEnumeration、Analyze、MergeAnalysis、Finalize
- [x] `job.cancellation` 覆盖 `requested`、`acknowledged`、`draining`、`cancelled`
- [x] 终态取消事件带 `safeToClose=true`
- [x] `jobs.status` 使用 `cancelling` 与 `cancelled`，未引入新迁移

### 技术决策

**为什么不改调度器或数据库大结构**:
- 这轮目标是让状态真实可见，不是重写导入架构
- 既有 `jobs` 表已能承载取消阶段信息，只补足状态和值语义
- 先让事件、持久化、前端提示说同一种语言

---

## 📅 阶段 3: 部分结果、新鲜度与缓存状态可见化

### 任务

- [x] 暴露部分结果状态与新鲜度
- [x] 暴露缓存与索引复用状态
- [x] 让元数据导入、延后分析、失效状态都能诚实呈现

### 完成内容

- [x] `import.partial_result` 发出文件行、文件树、时间线、工件族、搜索索引等部分结果信号
- [x] 新鲜度覆盖 `ready`、`partial`、`deferred`、`stale`、`invalidated`
- [x] `cache.index_status` 发出 `warming`、`ready`、`deferred`、`reused`、`stale`、`invalidated`
- [x] 元数据优先或镜像导入跳过场景会明确标记 deferred，而不是伪装完成
- [x] 已合并结果恢复路径会明确标记 reused

### 技术决策

**为什么只发元数据级状态，不发大 payload**:
- UI 需要的是可解释性，不是重复搬运完整结果
- bounded metadata 更稳定，也更适合事件流
- 减少事件体积，避免把性能优化本身做成新的负担

---

## 📅 阶段 4: 调度可解释性与性能报告埋点

### 任务

- [x] 暴露分析调度与 worker budget 状态
- [x] 为时间线与搜索热点路径增加性能埋点
- [x] 生成可供前端消费的性能报告事件

### 完成内容

- [x] 调度细节可表达 `queued`、`running`、`throttled`、`deferred`、`draining`
- [x] 暴露 `workerBudget`、`activeWorkers`、`queuedTasks`、`pendingTasks` 等有界状态
- [x] `performance.report_ready` 负载为完整 `PerformanceReport { summary, metrics }`
- [x] 稳定指标键已锁定为 `timeline.query.elapsedMs`、`timeline.query.rows`、`timeline.query.totalRows`、`search.query.elapsedMs`、`search.query.rows`、`search.query.totalRows`、`search.index.elapsedMs`、`search.index.rows`
- [x] 可选吞吐指标继续使用 `*.rowsPerSec`

### 说明

- 当前性能证据是埋点契约与 elapsed-ms 覆盖，不是硬件基准跑分
- 测试只断言非负、有界、稳定的指标输出，不写入机器相关数字

---

## 📅 阶段 5: 前端 TopBar / BottomDrawer / Reports 交互收敛

### 任务

- [x] 在现有布局内接入 typed 导入状态
- [x] 增加性能摘要、缓存状态与部分结果提示
- [x] 增加证据哈希 caveat 与报告提醒

### 完成内容

- [x] `TopBar` 增加紧凑状态 chips，展示导入阶段、取消状态、partial freshness、cache state、performance summary、evidence hash 状态
- [x] `BottomDrawer` 增加有界 `Import Signals` 区块
- [x] `Reports` 页面展示 evidence hash caveat 与 warning count
- [x] 不展示原始证据路径，不展示私有样本路径
- [x] mock-safe 事件状态存储集中在共享前端状态层，不在布局组件里重复订阅

### 技术决策

**为什么坚持在现有布局内完成**:
- 任务目标是解释系统状态，不是改版 UI
- `TopBar` 适合放全局紧凑提示
- `BottomDrawer` 适合放调查员需要展开查看的状态细节
- `Reports` 适合承接证据哈希完整性 caveat

---

## 📅 阶段 6: 证据哈希 caveat、依赖修复与环境清理

### 任务

- [x] 明确 evidence hash pending/unavailable/deferred/failed 提示
- [x] 修复前端缺失的直接运行时依赖
- [x] 清理过时的 LSP 环境阻塞认知

### 完成内容

- [x] 证据哈希状态通过部分结果与数据源 `hashStatus` 共同解释
- [x] 报告导出补充有界 warnings，不包含原始证据路径
- [x] 前端补齐 8 个缺失的直接运行时依赖
- [x] 当前 Rust 与前端 LSP 已可用且干净，旧的 unavailable 记录被确认为历史信息

### 诚实 caveats

- real/private E01 fixtures 仍保持 ignored 或 env-gated，不属于默认门禁
- 私有样本路径没有写入日志、报告提示或验证摘要
- evidence hash 不可用、待处理、延后、失败的状态会被显式提示，不会被隐藏

---

## 📅 阶段 7: 最终验证与收口

### 验证结果

- [x] F1 事件契约一致性 APPROVE
- [x] F2 后端导入、取消、部分结果、缓存、性能埋点 APPROVE
- [x] F3 前端 TopBar / BottomDrawer / Reports 交互 APPROVE
- [x] F4 默认质量门禁与 targeted gates APPROVE
- [x] F5 性能证据总结与已知 caveats APPROVE

### 验证摘要

- [x] typed event coverage 已锁定，覆盖 `import.phase_progress`、`import.partial_result`、`job.cancellation`、`cache.index_status`、`performance.report_ready`
- [x] targeted Rust gates 与 frontend gates 全部通过
- [x] `cargo test --workspace` 与 `pnpm --dir frontend test --run` 已通过，real/private E01 与依赖真实样本的测试继续按设计 ignored 或 env-gated
- [x] LSP diagnostics 当前可用且 clean

### 结论

- [x] 10/10 票据完成
- [x] 本轮交付的核心价值是，让性能与进度状态说真话，让 UI 解释等待、复用、延后与不完整，而不是只显示“正在处理中”
- [x] 当前保留的性能证据是稳定埋点与契约覆盖，后续若要给出机器级基准，需要单独设计基准环境与样本集

---

## 📊 本轮范围总结

### 完成范围

| 领域 | 完成内容 |
|------|----------|
| 事件契约 | typed DTO、topic 常量、前后端 parity |
| 后端流水线 | phase progress、partial result、cache status、performance report 发射 |
| 取消语义 | requested → acknowledged → draining → cancelled，含持久化 |
| 前端 UX | TopBar、BottomDrawer、Reports 状态可见化 |
| 证据 caveat | evidence hash 状态提示与报告 warnings |
| 环境修复 | 前端 8 个直接依赖补齐，LSP 状态恢复并验证 |
| 最终验证 | F1-F5 全部 APPROVE |

### 已知边界

- 不包含私有本地证据路径
- 不把 real/private E01 fixture 当作默认自动化覆盖
- 不宣称硬件 benchmark 数字，只记录 instrumentation 与契约覆盖

---

## 📅 阶段 8: 运行时修复、NTFS 真实样本回归与分区命名收口

### 任务

- [x] 修复 Tauri 运行时拒绝 dotted event topic 的告警
- [x] 修复 NTFS `System Volume Information` 目录子项入库/展示链路
- [x] 修复 NTFS 分区显示名误用 `/` 或 `System Volume Information` 的问题
- [x] 补充真实刘洋样本 ignored/env-gated 回归
- [x] 补跑前后端 CI 门禁并完成 diff 复审

### 完成内容

- [x] 将运行时事件主题从 dotted 形式改为 Tauri-safe kebab-case：
  - `import-phase-progress`
  - `import-partial-result`
  - `job-cancellation`
  - `cache-index-status`
  - `performance-report-ready`
- [x] Rust transport、Tauri bridge、前端 `EventTopic`、事件订阅与契约测试全部同步为新主题
- [x] `fs-ntfs` 的 parent 校验会遍历多个 `$FILE_NAME` 属性，避免首个命名空间不匹配时误判目录不可达
- [x] parallel MFT fast-path 的 directory-index backfill 改为从 root BFS 遍历可达目录，并用目录索引 parentage 修正 staging 中已有记录的 parent/path
- [x] 新增真实刘洋样本回归，分别验证：
  - `Users` 在主 NTFS root 下可达，且 merge 后 `FileRepo` 可列出子项
  - `System Volume Information` 在 staging 和主库 merge 后均可列出直接子项
- [x] 分区显示名改为保守、可解释策略：
  - 无可靠名称时显示 `Partition N (NTFS)` 等确定性名称
  - 保留有意义的 GPT 名称，例如 `Partition N (NTFS) - Evidence Volume`
  - 过滤 `/`、`\`、`.`、`..`、`System Volume Information`、`Microsoft basic data`、`Basic data partition`、`Windows recovery` 等误导性名称
- [x] 修正分区 root 名二次拼接，避免 `Partition 3 (NTFS) - Partition 3 (NTFS)`

### 真实样本验证

- [x] 刘洋样本 `Users` 回归通过：MFT fast-path 枚举后 `Users` 有直接子项
- [x] 刘洋样本 `System Volume Information` 回归通过：样本自身有 SVI 子项，staging 与 merge 后主库均能访问
- [x] 刘洋样本分区命名诊断通过，当前显示为：
  - `Partition 1 (FAT)`
  - `Partition 2 (Microsoft reserved) - Microsoft reserved partition`
  - `Partition 3 (NTFS)`
  - `Partition 4 (NTFS)`
  - `Partition 5 (BitLocker)`

### 盘符提取结论

- [x] 当前修复不声称从 NTFS 本身提取 `C:`/`D:` 盘符
- [x] 真实 Windows 盘符是 Mount Manager 分配，通常需要解析离线 `SYSTEM` hive 的 `MountedDevices` 并与卷 GUID、磁盘签名或分区偏移匹配
- [x] 现阶段先保证名称不误导；后续如需展示 `C:`，应单独实现 MountedDevices 匹配链路

### 门禁与复审

- [x] `cargo fmt --all -- --check` 通过
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 通过
- [x] `cargo test --workspace` 通过
- [x] `cargo build -p forensics-desktop --release` 通过
- [x] `pnpm --dir frontend typecheck` 通过
- [x] `pnpm --dir frontend test --run` 通过
- [x] `pnpm --dir frontend build` 通过
- [x] 后端 diff 复审 APPROVE，无 findings
- [x] 前端事件主题 diff 复审 APPROVE，无 findings

### 已知边界

- 真实刘洋样本回归仍为 ignored/env-gated，不进入默认 CI
- 未提交私有样本路径、`.omo` 临时证据或 Playwright 输出目录
- 已有旧 case DB 不会自动改名或补全 NTFS 树，需要用新构建重新导入/重新枚举

---

**日志维护人**: MiMo AI Assistant  
**最后更新**: 2026-06-05
