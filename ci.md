# CI Design

## 1. 文档目标

本文档细化项目的 CI 设计，作为 `design.md` 中“前后端 CI 设计”章节的独立展开版本。目标是让后续可以直接据此落地 GitHub Actions、内部 CI 或本地 pre-merge 流水线。

适用范围：
- Rust workspace
- React/Tauri 前端
- 文档与契约
- 测试样本与 fixture
- 开发追溯目录 `development-reports/`

## 2. CI 设计目标

CI 必须解决以下问题：
1. 阻止格式、类型、测试、契约回归进入主分支
2. 同时覆盖后端、前端、桌面壳和关键文档
3. 对运行时缓存数据库和开发追溯机制做专项校验
4. 让失败信号足够清晰，便于快速定位
5. 保持运行时间可接受，避免所有改动都触发全量重量级测试

## 3. 流水线分层

建议至少拆成四类工作流：

- `ci-backend.yml`
- `ci-frontend.yml`
- `ci-desktop.yml`
- `ci-docs.yml`

后续可扩展：
- `ci-fixtures.yml`
- `release.yml`
- `nightly-full-regression.yml`

## 4. 触发策略

### 4.1 Pull Request
PR 是主校验入口。

触发：
- 所有 PR
- 根据变更路径选择性运行子流水线

### 4.2 Push to main/master
主分支合并后跑：
- 与 PR 同级的必要检查
- 可附加更重的集成任务

### 4.3 Nightly
每天夜间执行：
- 全量 fixture 回归
- 较重的 e2e
- 可选安装包构建验证

### 4.4 Release Tag
发布标签触发：
- 全量后端/前端测试
- 桌面构建
- 产物打包
- 里程碑开发追溯检查

## 5. Backend CI

### 5.1 触发路径
- `crates/**`
- `apps/desktop/src-tauri/**`
- `Cargo.toml`
- `Cargo.lock`
- `.cargo/**`

### 5.2 检查步骤
推荐顺序：

1. Rust toolchain 安装
2. Cargo 缓存恢复
3. `cargo fmt --all -- --check`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `cargo test --workspace`
6. 运行关键集成测试集
7. 上传测试摘要 artifact

### 5.3 必须覆盖的模块
- `domain`
- `app-services`
- `persistence-sqlite`
- `runtime-cache`
- `traceability`
- `search`
- `timeline`
- `artifacts-windows`

### 5.4 专项检查
#### runtime-cache
至少验证：
- TTL 过期逻辑
- 案件关闭清理逻辑
- 删除 `runtime.db` 后可回源恢复

#### traceability
至少验证：
- 事件 JSONL 追加顺序
- `agent_name`、`agent_id`、`ts` 不缺失
- session markdown 渲染稳定

## 6. Frontend CI

### 6.1 触发路径
- `apps/desktop/src/**`
- `apps/desktop/package.json`（若拆分）
- `package.json`
- `pnpm-lock.yaml`

### 6.2 检查步骤
1. Node/pnpm 安装
2. pnpm 缓存恢复
3. `pnpm install --frozen-lockfile`
4. `pnpm lint`
5. `pnpm typecheck`
6. `pnpm test`
7. 上传覆盖率或测试摘要

### 6.3 必须覆盖
- feature hooks
- DTO schema 对齐
- 事件订阅总线
- 关键页面组件
- 文件预览 viewer 状态逻辑

## 7. Desktop Integration CI

### 7.1 触发路径
- `apps/desktop/**`
- `crates/transport/**`
- `crates/app-services/**`
- `crates/runtime-cache/**`
- `crates/traceability/**`

### 7.2 检查目标
- Tauri command 是否仍可编译
- command / event DTO 是否兼容
- 基本桌面主流程是否未被破坏

### 7.3 推荐步骤
1. 构建 Tauri app
2. 运行 smoke e2e：
   - 创建案件
   - 导入逻辑目录
   - 打开文件浏览页
   - 触发搜索
   - 触发 development trace 记录
3. 收集日志 artifact

## 8. Docs / Contract CI

### 8.1 触发路径
- `PRD.md`
- `spec.md`
- `design.md`
- `ci.md`
- `test-plan.md`
- `autopsy-borrowings.md`
- `development-reports/**`

### 8.2 检查内容
1. markdown lint
2. 核心文档存在性检查
3. 文档链接完整性检查（如后续引入）
4. 开发追溯规则检查：
   - 新 session 是否带事件文件
   - 事件行是否含 `agent_name` 与 `ts`

### 8.3 可选规则
- 如果代码有较大变更但没有新增 session report，则给出警告或阻断
- milestone merge 前必须存在 summary 文档

## 9. 路径过滤策略

建议按路径分流，避免每次都跑全量：

### 仅文档变更
只跑：
- `ci-docs`

### 仅前端变更
跑：
- `ci-frontend`
- `ci-desktop`（轻量 smoke）

### 仅后端变更
跑：
- `ci-backend`
- 若改到 transport/app-services，再跑 `ci-desktop`

### 核心契约变更
涉及：
- `transport/**`
- `app-services/**`
- `design.md`

应跑：
- `ci-backend`
- `ci-frontend`
- `ci-desktop`
- `ci-docs`

## 10. 缓存策略

### 允许缓存
- Cargo registry
- Cargo target（审慎）
- pnpm store
- 测试 fixture 下载缓存

### 禁止缓存
- `case-root/cache/runtime.db`
- 任意运行期生成的 case cache
- `development-reports/` 里的生成内容作为构建缓存输入
- 临时预览资产

原因：
- `runtime.db` 是易失缓存，不应污染 CI 结果
- development reports 是工程追溯结果，不应被缓存污染

## 11. 环境矩阵建议

### PR 默认矩阵
- Windows latest
- Ubuntu latest

说明：
- 宿主优先是 Windows，但部分纯 Rust 逻辑可跨平台先验证
- 真正依赖 Windows 行为的测试可只在 Windows 上跑

### Nightly / release 矩阵
- Windows latest 必跑
- 可选 Ubuntu latest
- 如未来支持 macOS，再加入 macOS latest

## 12. 失败阻断策略

### 必须阻断 merge
- `cargo fmt` 失败
- `clippy` 失败
- Rust 单元/集成测试失败
- 前端 lint/typecheck/test 失败
- DTO/schema 契约测试失败
- traceability 必填字段测试失败
- runtime-cache 关键回源/TTL 测试失败

### 可先告警不阻断
- 文档建议项
- 部分 flaky 的重型 e2e
- 非关键 nightly-only fixture

## 13. 产物与报告

建议每条 CI 上传：
- test summary
- failure logs
- e2e screenshot/video（如有）
- 构建产物摘要

对于 nightly / release：
- 额外上传 regression report
- 可上传 fixture 统计汇总

## 14. 推荐 GitHub Actions 结构

```text
.github/
  workflows/
    ci-backend.yml
    ci-frontend.yml
    ci-desktop.yml
    ci-docs.yml
    nightly-full-regression.yml
    release.yml
```

## 15. 推荐作业拆分

### ci-backend.yml
- `fmt`
- `clippy`
- `unit-tests`
- `integration-tests`
- `runtime-cache-tests`
- `traceability-tests`
- `documentation-drift-guard`

### ci-frontend.yml
- `lint`
- `typecheck`
- `unit-tests`

### ci-desktop.yml
- `build-tauri`
- `smoke-e2e`

### ci-docs.yml
- `markdown-lint`
- `traceability-structure-check`

## 16. Nightly Full Regression

Nightly 跑更重的内容：
- 全量 fixture 测试
- 全量 Windows 痕迹解析样本
- 搜索与时间线长链路
- HTML/JSON/CSV 报告导出回归
- development report 生成回归

## 17. Release Gate

发布前必须全部通过：
- Backend CI
- Frontend CI
- Desktop Integration CI
- Docs / Contract CI
- Nightly regression 最近一次成功
- milestone summary 已存在于 `development-reports/summaries/`

## 18. 与开发追溯机制的集成

CI 不是只测代码，也要测“可追溯性”本身。

要求：
- 新增重要实现时，允许后续要求补 session report
- traceability 测试校验 JSONL 结构
- release gate 校验 milestone summary 存在

## 19. 后续落地建议

建议下一步继续细化为：
1. 每个 workflow 的 job 级 YAML 草案
2. 每类测试命令清单
3. fixture 体积与分层策略
4. PR 模板与 development report 提交规范
