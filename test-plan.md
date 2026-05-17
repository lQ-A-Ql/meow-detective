# Test Plan

## 1. 文档目标

本文档独立描述该 Rust + React 取证工具的测试策略、测试分层、样本组织、关键场景与执行方式。它是 `design.md` 中测试章节的可执行展开版本。

目标：
- 定义测试边界与优先级
- 规定测试目录与命名
- 明确 fixture 组织方式
- 明确 runtime cache 与 traceability 的专项测试
- 支撑 CI、回归与发布验证

## 2. 测试原则

1. **权威结果以主库和正式产出为准**
2. **临时缓存只验证性能和行为，不作为唯一正确性来源**
3. **解析器必须对固定 fixture 稳定输出**
4. **前后端契约必须可测试，不靠人工约定**
5. **开发追溯机制本身也必须被测试**
6. **优先建立小而稳定的 fixture 集，再扩充大样本回归**

## 3. 测试分层

### 3.1 Rust 单元测试
测试目标：
- 领域对象规则
- 小型算法
- query/parser/highlighter
- cache key 生成
- trace event model 校验

### 3.2 Rust 集成测试
测试目标：
- SQLite repository
- runtime-cache 生命周期
- search 索引与查询链路
- timeline projection
- artifact parser 对 fixture 的输出
- report exporter 输出结构

### 3.3 前端单元/组件测试
测试目标：
- hooks
- event bus
- stores
- viewer 行为
- 页面级关键组件交互

### 3.4 端到端测试
测试目标：
- 从案件创建到报告导出的完整主链路
- command/event 与 UI 的真实协同
- development report 的生成链路

### 3.5 回归测试
测试目标：
- 重型 fixture
- 跨模块链路
- Windows 痕迹样本回放

## 4. 测试目录设计

```text
forensics/
  crates/
    domain/
      src/
      tests/
    persistence-sqlite/
      src/
      tests/
        case_repo.rs
        file_repo.rs
        artifact_repo.rs
    runtime-cache/
      src/
      tests/
        ttl_cleanup.rs
        file_handles.rs
        preview_chunks.rs
        search_pages.rs
    traceability/
      src/
      tests/
        event_log_append.rs
        required_fields.rs
        session_report_render.rs
    search/
      src/
      tests/
        query_parser.rs
        highlighter.rs
        indexing_flow.rs
    timeline/
      src/
      tests/
        projection_macb.rs
        projection_artifacts.rs
        bucket_aggregation.rs
    artifacts-windows/
      src/
      tests/
        prefetch_fixture.rs
        lnk_fixture.rs
        recycle_bin_fixture.rs
        registry_fixture.rs
        sru_fixture.rs
    reports/
      src/
      tests/
        html_export.rs
        json_export.rs
        csv_export.rs
  apps/
    desktop/
      src/
      tests/
        component/
          AppShell.test.tsx
          FileBrowserView.test.tsx
          SearchView.test.tsx
          TimelineView.test.tsx
        e2e/
          create_case.spec.ts
          import_datasource.spec.ts
          search_flow.spec.ts
          timeline_flow.spec.ts
          report_export.spec.ts
          devtrace_generation.spec.ts
  testdata/
    images/
    artifacts/
      windows/
        prefetch/
        lnk/
        recycle-bin/
        registry/
        sru/
    runtime-cache/
    traceability/
    reports/
```

## 5. 命名规范

### Rust
- 单元或小型集成测试文件按能力域命名，如：`bucket_aggregation.rs`
- 更细粒度测试函数使用 `should_*` 风格

### Frontend
- 组件测试：`*.test.tsx`
- e2e：`*.spec.ts`

### Fixture
- 目录名按工件家族组织
- 文件名尽量表达版本/来源/场景

示例：
- `prefetch-win10-calc.pf`
- `lnk-recent-word-doc.lnk`
- `recyclebin-delete-userdoc.i`

## 6. Fixture 设计策略

### 6.1 分层
#### Small fixtures
- 小体积
- 适合 PR / CI 快速执行
- 覆盖核心解析路径

#### Medium fixtures
- 适合 nightly
- 覆盖更多边界字段

#### Large fixtures
- 适合手动或发布前回归
- 覆盖真实样本组合场景

### 6.2 不变性要求
- fixture 一旦纳入测试，不应频繁修改
- 若必须替换，需同步更新期望输出与说明

### 6.3 预期结果存放
建议为关键 parser fixture 配期望 JSON：

```text
testdata/
  artifacts/
    windows/
      prefetch/
        sample1.pf
        sample1.expected.json
```

这样可对解析输出做快照对比。

## 7. 后端专项测试设计

## 7.1 Case Service
至少覆盖：
- 创建案件目录骨架
- 重复打开案件
- schema/version 校验
- case close 后 session 释放

## 7.2 Data Source / File System
至少覆盖：
- RAW/逻辑目录 probe
- attach 后 volume/file entry 写入
- BFS 枚举顺序基本稳定
- 目录/文件混合结构处理
- 异常文件节点不阻断整体枚举

## 7.3 Search
至少覆盖：
- literal 查询
- phrase 查询
- regex 查询
- snippet/highlight 生成
- 翻页结果稳定
- 删除 runtime cache 后仍能回源查询

## 7.4 Timeline
至少覆盖：
- MACB 投影
- 工件事件投影
- bucket 聚合
- 时间范围过滤
- source_object 回链

## 7.5 Artifact Parsers
### Prefetch
覆盖：
- run_count
- last_run_times
- executable name
- referenced files

### LNK
覆盖：
- target path
- working dir
- time fields
- optional blocks

### Recycle Bin
覆盖：
- `$I/$R` 配对
- 删除时间
- 原始路径
- 大小

### Registry
覆盖：
- hive 打开
- key/value 提取
- 时间戳标准化

### SRU
覆盖：
- 表读取
- 字段标准化
- 事件映射

## 8. runtime-cache 专项测试

这是必须独立关注的测试面。

### 8.1 核心断言
- 缓存命中与回源结果一致
- TTL 到期后命中失效
- 案件关闭后 case-scoped cache 被清理
- `runtime.db` 被删除后功能仍可恢复
- 临时缓存绝不污染主库正式结果

### 8.2 建议用例
- `ttl_cleanup.rs`
- `file_handles.rs`
- `preview_chunks.rs`
- `search_pages.rs`
- `timeline_bucket_cache.rs`

### 8.3 特殊场景
- 异常退出后再次启动清理过期缓存
- handle 已过期但前端仍请求 range read
- timeline bucket cache 命中后主库数据未变化

## 9. traceability 专项测试

开发追溯机制必须作为一等测试对象。

### 9.1 核心断言
- 每个事件都包含：
  - `event_id`
  - `ts`
  - `agent_id`
  - `agent_name`
  - `action`
- JSONL 为合法逐行对象
- session markdown 渲染稳定
- 不同 session 不会串写文件

### 9.2 建议用例
- `event_log_append.rs`
- `required_fields.rs`
- `session_report_render.rs`
- `summary_rollup.rs`

### 9.3 典型场景
- 文档创建事件
- crate 初始化事件
- 测试执行通过/失败事件
- blocker 记录事件

## 10. 前端测试设计

## 10.1 Hook 测试
至少覆盖：
- 搜索输入 debounce
- event subscription 生命周期
- 当前选中对象切换

## 10.2 Store 测试
至少覆盖：
- UI 状态切换
- selection store
- task panel 更新

## 10.3 组件测试
至少覆盖：
- `AppShell`
- `FileBrowserView`
- `SearchView`
- `TimelineView`
- `HexViewer`
- `TextViewer`

### 关键断言
- 分页切换时 UI 正确更新
- 搜索结果高亮渲染正确
- timeline 过滤条件变化能触发请求
- viewer 读取 chunk 时能正确处理 loading/error

## 11. 端到端测试设计

### 11.1 最小 smoke 集
- 创建案件
- 导入逻辑目录
- 展示文件树
- 运行一次搜索
- 导出一次 HTML 报告

### 11.2 核心主链路集
- 创建案件
- 导入数据源
- 文件浏览
- 预览文件
- 索引并搜索
- 运行 Windows 工件提取
- 查看时间线
- 导出报告
- 写 development report

### 11.3 建议 e2e 文件
- `create_case.spec.ts`
- `import_datasource.spec.ts`
- `search_flow.spec.ts`
- `timeline_flow.spec.ts`
- `report_export.spec.ts`
- `devtrace_generation.spec.ts`

## 12. 测试环境

### 12.1 PR 环境
- 快速单元测试
- 小 fixture 集
- 轻量 smoke e2e

### 12.2 Nightly 环境
- 中等 fixture
- 全链路集成测试
- 更多解析器回归

### 12.3 Release 环境
- 全量关键 fixture
- 桌面构建验证
- 报告导出验证
- development report 完整性验证

## 13. 通过标准

### 13.1 PR Gate
必须通过：
- Rust 单元/集成
- 前端 lint/typecheck/test
- runtime-cache 关键测试
- traceability 关键测试
- smoke e2e

### 13.2 Release Gate
必须通过：
- 所有 PR Gate
- Windows artifact fixture 回归
- timeline/report 回归
- development report summary 检查

## 14. 缺陷定位建议

### 如果失败发生在：
- `runtime-cache/*`：优先排查 TTL、回源、清理策略
- `traceability/*`：优先排查 JSONL 结构和 session 分流
- `artifacts-windows/*`：优先比对 fixture 与 expected JSON
- e2e：优先区分是 command 层、event 层还是 UI 渲染层失败

## 15. 后续扩展建议

后续可以继续补：
1. 每个 parser fixture 的 expected schema
2. 每个 e2e 的步骤级断言清单
3. 覆盖率目标
4. flaky test 管理规则
5. fixture 版本说明文档