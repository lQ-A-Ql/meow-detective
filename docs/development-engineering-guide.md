# Forensics Workbench 开发工程指南

## 1. 工程基线

- Rust stable
- Tauri 2
- React 18
- React Router 7
- Tailwind 4 CSS-first
- pnpm
- 无 HTTP server
- `crates/transport` 是契约源

V2 长期执行主计划见 `docs/v2-longterm-plan.md`。

## 2. 典型开发顺序

1. 修改或新增 `crates/transport` DTO / request / event
2. 修改 `app-services` 服务逻辑
3. 接入 Tauri command
4. 同步 `frontend/src/types/models.ts`
5. 同步 `frontend/src/lib/api/*`
6. 同步 hooks、页面、公有组件
7. 同步 mock 数据
8. 执行 Rust / frontend 测试
9. 更新文档与开发记录

## 3. 文件浏览专项流程

涉及以下内容时，必须走真实链路检查：

- 分区根显示
- 文件树懒加载
- `showHidden`
- `deleted / hidden / system`
- 排序与分页

推荐顺序：

1. transport 契约
2. 持久化 / repo / migration
3. service 真实排序 / 真实过滤 / 真实根模型
4. import / staging merge
5. Tauri command
6. frontend API / hooks / page
7. mock 与测试

禁止只在前端做“视觉补丁”掩盖真实链路未接住的问题。

## 4. MCP 与安全边界开发流程

涉及 MCP 时，额外遵循：

1. 配置 DTO 与 Rust 类型同步
2. 默认最小权限
3. transport / state / command / frontend API 一起更新
4. connect、test、resource、tool、prompt 都要考虑审计
5. 错误出口统一脱敏

## 5. 导出与提取开发流程

- 文件提取默认 `overwrite=false`
- 报告导出默认 `overwrite=false`
- 目标已存在时返回冲突
- 路径校验优先于写入
- 写入优先采用临时文件 + rename
- 如有审计要求，默认记录动作摘要

## 6. 前端工程约定

- 页面不直接 `invoke`
- 统一通过 `apiClient.request(...)`
- 公有 UI 放在公有组件目录
- mock 与真实契约保持同 shape
- 本地排序只允许用于 mock 或极小范围展示兜底

## 7. 测试策略

### Rust

- DTO / serde 测试
- service 单元测试
- repo / migration 测试
- fixture / expected JSON 测试
- 真实样本回归说明

### Frontend

- API layer 测试
- store 测试
- hook 测试
- 页面回归测试

### 文档

- `git diff --check`
- `scripts/check-doc-drift.ps1`
- 如图谱变更较大，再跑 `-RenderMermaid`

### V2 专项

- fixture 变更同步 `docs/fixture-handbook.md`
- expected JSON 变更同步 `docs/expected-json-contract.md`
- benchmark 变更同步 `docs/benchmark-baseline.md`
- 关联分析变更同步 `docs/correlation-analysis-design.md`
- 发布门禁变更同步 `docs/release-scorecard.md`

## 8. 默认 gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend test
pnpm --dir frontend build
git diff --check
powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1
```

## 9. 变更追溯

每轮有实质改动后，至少同步一项：

- `development-reports/sessions/YYYY-MM-DD.md`
- 对应 remediation plan
- 对应专题文档
- `docs/documentation-index.md`

## 10. 文档同步要求

以下变化必须同步更新图谱和专题文档：

- 分区根模型
- 文件浏览排序
- 状态字段传播
- MCP 权限模型
- 导出覆盖策略
- media handle 安全边界
- fixture / expected JSON / parser 支持边界
