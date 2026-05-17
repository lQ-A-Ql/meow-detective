# Autopsy 可借鉴点总结

## 1. 文档目标

本文档总结 Autopsy 中对本项目有借鉴价值的部分，并区分：
- **直接借鉴**：适合在 Rust + React 新架构中延续的设计思想
- **借鉴但要改造**：产品思路正确，但实现方式需要现代化重写
- **明确避免照搬**：在新项目中应刻意规避的历史包袱

本文档的定位是“架构与产品借鉴摘要”，不是 Autopsy 全量源码分析报告。

## 2. 总体判断

Autopsy 最值得借鉴的不是某个具体 Java 类，而是它长期沉淀出来的 **数字取证工作台能力分层**：
- 案件
- 数据源
- 文件浏览
- 工件提取
- 关键词检索
- 时间线
- 报告导出

它证明了这条主链路在真实取证工作中是有价值的。对本项目而言，核心策略应是：

- **借鉴它的能力地图与工作流闭环**
- **重写它的实现边界与工程组织方式**

## 3. 直接借鉴的部分

## 3.1 案件作为顶层工作单元
### 借鉴理由
Autopsy 把 case 放在所有分析活动之上，这很符合取证工作现实：
- 数据源属于案件
- 检索结果属于案件
- 工件结果属于案件
- 报告导出属于案件

### 对本项目的启发
应继续坚持：
- case 是顶层上下文
- 所有持久化结果都按 case 隔离
- 打开/关闭 case 是系统主状态切换点

## 3.2 完整的分析闭环
Autopsy 的价值在于它不是单点工具，而是一条闭环：
- 导入镜像
- 浏览文件
- 提取工件
- 搜索文本
- 看时间线
- 导出报告

### 对本项目的启发
第一版就应覆盖闭环的最小版本，而不是只做某一个解析器集合。

## 3.3 工件提取按能力域拆分
从 RecentActivity 等模块可以看到，Autopsy 倾向于：
- 每类工件一个或一组专门提取器
- 由上层编排器决定顺序与依赖

### 借鉴点
这非常适合本项目：
- Registry
- Prefetch
- LNK
- Jump List
- Recycle Bin
- SRU

都应是独立 parser/extractor，而不是一个大模块混在一起。

## 3.4 关键词检索作为核心入口
Autopsy 的关键词检索不是附加功能，而是核心分析入口之一。

### 借鉴点
本项目应将搜索视为一级能力：
- 独立索引
- 独立 query 模型
- 命中高亮
- 与标签/报告联动

## 3.5 时间线作为统一观察视角
Autopsy 证明时间线不是“锦上添花”，而是把文件系统事件和工件事件组织成可调查视图的重要能力。

### 借鉴点
本项目应保留：
- 文件 MACB 投影
- 工件事件投影
- 时间范围与类型过滤
- 从时间线回跳对象详情

## 3.6 报告模块化
Autopsy 的 report module 思想非常值得继承：
- 不同输出类型由不同 exporter 提供
- 设置和输出形式可扩展

### 借鉴点
本项目应保持导出器接口化：
- HTML
- JSON
- CSV
- Evidence bundle

## 3.7 长任务可见性
从 IngestManager 一类设计可以看出，Autopsy 很重视：
- 长任务状态
- 模块进度
- 事件通知

### 借鉴点
本项目必须保留：
- job/task 概念
- progress event
- 可取消任务
- 运行状态可视化

## 4. 借鉴但必须改造的部分

## 4.1 Case 管理思路正确，但实现不能照搬
Autopsy 的 `Case.java` 承担了过多责任：
- 当前案件全局状态
- 生命周期编排
- 目录结构
- 事件桥接
- UI 联动
- 多用户协作逻辑

### 本项目应如何改造
拆成多个边界清晰的服务：
- `CaseService`
- `CaseRepository`
- `CaseSession`
- `CaseWorkspacePolicy`
- `CaseEventPublisher`

## 4.2 Ingest 编排思路值得借鉴，但应重构为更明确的任务系统
Autopsy 的 ingest 架构很有价值：
- job
- 多队列
- 文件级并发
- 事件发布

但其历史实现带有较强桌面线程模型痕迹。

### 本项目应如何改造
使用 Rust 异步/并发模型重构为：
- job registry
- typed task queue
- cancellation token
- progress snapshot store
- event bus

## 4.3 搜索系统思路值得继承，但要拆掉静态单例式包装
Autopsy 的 `KeywordSearch.java` 代表了把搜索作为独立子系统的思路，这点是对的。

### 本项目应如何改造
改造成：
- `TextExtractionService`
- `IndexWriterService`
- `SearchQueryService`
- `HighlightService`

而不是一个静态工具类包一层 server singleton。

## 4.4 时间线模块应保留“案件级 controller”思想，但不要与桌面 UI 强绑定
Autopsy 的 timeline 模块强调：
- case-scoped
- event-driven
- listener 驱动更新

### 本项目应如何改造
后端保留 case-scoped timeline service，前端只消费查询与事件，不把时间线控制器绑死在桌面框架生命周期上。

## 4.5 Recent Activity 的工件组织方式应保留，但执行依赖要显式化
Autopsy 的 recent activity 模块说明：
- 有些提取器依赖上游结果
- 浏览器/注册表/系统信息之间会互相补充

### 本项目应如何改造
- `dependencies()` 明确声明依赖
- 基础 parser 与派生 enrichment 分层
- 先标准化 artifact，再做高层关联分析

## 5. 明确不应照搬的部分

## 5.1 历史桌面框架耦合
Autopsy 深度耦合 Java 桌面框架、NetBeans 平台、Swing/JavaFX 生命周期。

### 为什么不该照搬
- 增加模块边界模糊度
- 不利于现代前后端职责分离
- 限制后续演进

### 本项目选择
- Tauri 作为桌面壳
- React 负责 UI
- Rust 负责核心逻辑

## 5.2 超级管理器 / 全局单例
Autopsy 中不少核心能力由大单例或全局当前状态驱动。

### 为什么不该照搬
- 测试困难
- 并发边界不清
- 状态污染风险高

### 本项目选择
- 明确 service 边界
- 用 session 和 repository 替代全局 current object
- 通过事件和 DTO 连接 UI

## 5.3 UI 与核心逻辑混写
Autopsy 里很多对象同时知道：
- 业务状态
- 弹窗
- 菜单
- 面板
- 生命周期

### 为什么不该照搬
在 React + Rust 架构中，这会直接破坏可测试性与模块清晰度。

### 本项目选择
- 核心逻辑不依赖 UI
- UI 不直接管理底层对象生命周期

## 5.4 过重的历史工具链依赖方式
Autopsy 集成了很多外部工具与库，这在成熟阶段是优势，但在新项目第一版容易带来复杂性。

### 为什么不该照搬
本项目当前目标更偏：
- Rust-first
- 单机 MVP
- 边界清晰

### 本项目选择
- 保留能力接口
- 控制第一版依赖数量
- 只在确有必要时引入受控适配层

## 6. 对本项目最重要的借鉴地图

如果把 Autopsy 的借鉴价值压缩成最重要的七点，就是：

1. **Case-first**
2. **Data source → file browser → artifact/search → timeline → report** 的闭环
3. **模块化工件提取器**
4. **独立搜索子系统**
5. **统一时间线投影**
6. **长任务编排与进度可见性**
7. **可插拔报告导出器**

## 7. 对本项目最重要的规避地图

如果把最需要避免的部分压缩成五点，就是：

1. 不做 UI/核心逻辑混写
2. 不做全局超级单例
3. 不做隐式模块依赖
4. 不做桌面框架耦合式核心架构
5. 不做把缓存、正式结果、运行时状态全部混在一个库里

## 8. 与本项目文档的映射关系

### PRD 层面借鉴
- 闭环工作流
- Case 组织方式
- 搜索/时间线/报告作为一级功能

### spec 层面借鉴
- ingest job 思想
- parser/extractor 插件接口
- report exporter 接口
- timeline projector 思想

### design 层面借鉴
- 多队列任务系统
- 工件依赖顺序
- 统一 artifact/timeline 投影
- 搜索与 viewer 的协同方式

## 9. 推荐使用方式

后续做实现时，可将本文档作为“架构借鉴判定表”：

- 如果某项能力属于闭环核心，就优先参考 Autopsy 的产品思路
- 如果某项实现涉及全局状态、桌面生命周期、静态单例、UI 耦合，则优先避免继承其旧实现方式

简化判断就是：

- **学它做什么**
- **不要学它怎么被历史包袱做出来**

## 10. 下一步建议

基于这份文档，后续最值得继续落地的实现顺序是：
1. Case + workspace
2. Data source + file catalog
3. Ingest/job system
4. Search
5. Windows artifacts
6. Timeline
7. Reports