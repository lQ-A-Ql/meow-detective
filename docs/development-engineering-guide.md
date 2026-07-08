# Meow~Detective 开发工程指南

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
7. 同步 fixture / test double，禁止新增 runtime mock
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
- 页面、业务组件、store 不直接 import `@/lib/api/*` 业务模块；业务 API 访问集中在 `features/<domain>/hooks.ts`
- 公有 UI 放在公有组件目录
- runtime 禁止 mock / fake / dummy 业务数据集；test double 仅允许出现在测试文件
- runtime 禁止硬编码 demo case、真实样本路径或生产可点击的 demo-case 创建入口；演示数据只能存在于测试、fixture 或受控文档示例中
- 本地排序只允许用于极小范围展示兜底，不得伪造后端业务结果
- Frontend MVP boundary 以 `docs/frontend-mvp-boundary.md` 为准：Page 只做 route shell，Feature 负责请求与状态编排，Component 只做 UI，API 只封装 Tauri command，Platform 只封装 Tauri/browser adapter，Store 只保存 UI 状态和选中 ID。

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

## 8. 代码质量硬约束

以下约束由 V3 代码审计 (2026-06-15) 确立，任何 PR 必须满足：

### 8.1 格式与 Lint

- `cargo fmt --all -- --check` — 零 diff
- `cargo clippy --workspace --all-targets -- -D warnings` — 零 error
- `.gitattributes` 强制 LF 行尾；二进制文件标记为 binary

### 8.2 文件大小

| 约束 | 阈值 | 说明 |
|------|------|------|
| 单文件上限 | 1500 行 | 超过必须拆分（V3 已将 correlation_service 2315→4 文件，report_service 2131→5 文件） |
| 函数上限 | 200 行 | 超过建议提取辅助函数或子模块 |
| 测试文件 | 不限 | 测试文件可豁免大小限制 |

### 8.3 错误处理

| 约束 | 要求 |
|------|------|
| 新 crate | 必须使用 `thiserror` 定义类型化错误枚举，**禁止** `Result<T, String>` |
| 已有 crate | `artifacts-linux`、`containers-pst` 已修复；app-services 逐步迁移 |
| Error 命名 | `{CrateName}Error` (如 `LinuxArtifactError`, `PstError`) |
| 前向兼容 | `#[error(transparent)]` 或 `impl From<OtherError>` 供上层转换 |

### 8.4 依赖治理

- 所有外部 crate 版本集中在根 `Cargo.toml` `[workspace.dependencies]`
- 成员 crate 使用 `{crate} = { workspace = true }` 引用，**禁止**直接写版本号
- `cargo deny check` 必须通过 (advisories / bans / licenses / sources)
- 新增依赖需评估：安全审计、许可兼容、重复依赖

### 8.5 Unsafe 代码

- 每个 `unsafe` 块**必须**附 `// SAFETY:` 注释说明安全前提
- 优先使用 RAII guard 模式管理 FFI 资源
- 生产代码中的 unsafe 必须通过 code review

### 8.6 Dead Code

- 生产代码中**禁止** `#[allow(dead_code)]`；未使用的代码应删除
- 解析器 crate 中的格式常量（如 `const FIELD_OFFSET: usize`）豁免
- 每个 PR 必须检查是否有新增 dead_code 警告

## 9. 模块化约束

### 9.1 Crate 边界

| 规则 | 说明 |
|------|------|
| 单向依赖 | domain ← transport ← app-services ← Tauri commands |
| 禁止反向 | parser / repo / core crate 不得依赖 Tauri 或前端 |
| Service Tauri-free | `app-services` 不得依赖 Tauri；事件、窗口、runtime cache、media protocol 等桌面适配停留在 command/state 层 |
| Command thin wrapper | Tauri command 仅做请求校验、active case/cache/state 适配、service 调用、DTO/error 映射；不得承载业务编排、parser 逻辑或 raw SQL |
| contracts-pst | 独立 crate，不耦合 artifacts-windows |
| artifacts-{linux,macos} | 独立 crate，各自管理 parser 族 |

### 9.2 模块拆分模式

- service 超过 1500 行 → 拆分为 `{service}/{mod, sub_a, sub_b, tests}.rs`
- 保持 `mod.rs` 为公开 API 入口 + 共享常量和辅助函数
- `tests.rs` 包含 `#[cfg(test)] mod tests { ... }`

### 9.3 前端组件约束

- 页面组件 (`pages/`) 通过 hooks 获取数据，不直接调 API
- 业务组件 (`components/`) 通过 props 接收数据
- shadcn/ui 基础组件在 `app/components/ui/`
- 每个组件文件 ≤ 500 行

## 10. 并行化约束

### 10.1 Rayon 使用规范

- 使用 `rayon::prelude::*` 导入
- `par_iter()` 用于 CPU 密集型批量操作
- I/O 密集型保持串行，避免竞争 E01 reader
- 共享可变状态使用 `Mutex<T>` 保护，锁粒度尽量小
- 传递给 `par_iter()` 的闭包必须 `Sync`
- 输出排序：并行收集后用 `sort_by_key` 确保确定性

### 10.2 适用场景

| 场景 | 并行方式 |
|------|---------|
| Artifact 批提取 | `par_iter()` — 已实施 |
| Correlation 规则匹配 | `par_iter()` + `Mutex<BTreeMap>` — 已实施 |
| Timeline MACB 投影 | `par_iter()` + `flat_map_iter` — 已实施 |
| Hash 计算 | `par_iter()` — 推荐 |
| MFT 路径重建 | `par_iter()` 子树并行 — 推荐 |

### 10.3 注意事项

- SQLite `Connection` 不是 `Sync`，每个线程必须打开独立连接
- E01 reader 不建议跨线程共享，需要 `Arc<Mutex<>>` 或每线程独立打开
- 控制内存：`chunks(N)` 分块避免同时持有过多中间结果

## 11. 测试约束

### 11.1 覆盖率期望

| 层 | 期望 |
|----|------|
| DTO / serde | 每个 DTO 至少 1 个 round-trip 测试 |
| Service | 每个公开函数至少 1 个测试 |
| Repo | 每个 CRUD 操作至少 1 个测试 |
| Parser | 每个 parser 族至少 3 个测试 (valid / invalid / edge) |
| 前端页面 | 渲染 / 加载 / 空状态 / 错误状态 |

### 11.2 真实样本测试

- 每个受支持的 E01 样本至少 1 个回归测试
- 测试标记 `#[ignore]`，运行时设置 `FORENSICS_E01_FIXTURE` 或 `FORENSICS_LIUYANG_E01_FIXTURE`
- 真实样本路径**禁止**提交；使用环境变量

## 12. 默认 gate（完整版）

```bash
# 格式 + Lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 测试
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend test
pnpm --dir frontend build

# 安全
cargo deny check

# 文档
git diff --check
powershell -File scripts/check-doc-drift.ps1
powershell -File scripts/check-command-sql-boundary.ps1
powershell -File scripts/check-media-protocol-guard.ps1
powershell -File scripts/check-release-guard.ps1
powershell -File scripts/check-stage5-regression-guard.ps1
powershell -File scripts/check-frontend-runtime-guard.ps1
powershell -File scripts/check-frontend-lockfile-policy.ps1
```

`check-stage5-regression-guard.ps1` 同时锁定以下边界：

- MCP transport validation、MCP nested DTO snake_case 契约与 staging merge conflict visibility。
- `import_pipeline` 生产代码保持 Tauri-free，只通过 `ImportEventSink` 与命令层桥接事件。
- `analysis_service/extraction/mod.rs` 保持 facade，runner、registry preload、summary 不回流到模块根。
- `file_service/preview.rs` 保持 Tauri-free DTO assembly，`file_commands.rs` 仅做 active case、cache、media protocol 适配并委托 app-services。
- `datasource_service.rs` 保持 facade，attach/probe/LVM/fs magic/reader/types/partition index 不回流成上帝模块。
- Linux Stage 0 检材3 baseline 继续使用 `FORENSICS_LINUX_E01_FIXTURE` opt-in，不进入默认 CI，不提交私有样本。

`check-frontend-runtime-guard.ps1` 锁定以下前端运行时边界：

- `frontend/src/lib/api/client.ts` 是唯一 Tauri `invoke` 入口，页面、hooks、组件不得直接调用 `@tauri-apps/api/core`。
- 非测试 runtime 文件不得包含 `vi.mock` / `jest.mock` / `mockResolvedValue` 等测试 mock wiring。
- 非测试 runtime 文件不得定义 mock/fake/dummy 业务数据集、mock/demo runtime mode 或生产 demo-case 创建入口。
- 页面、业务组件、store 不得直接 import `@/lib/api/*` 业务模块；必须通过 feature hooks。`apiClient` 错误类型和 MCP store 适配例外。
- Graph citation/search 代码不得把 `nodeCountByType` 的类型统计 key 当作 node id 调用 neighborhood 查询。
- `setTimeout` 只允许用于 UI debounce/copy/menu 交互，不允许伪造业务加载延迟。
- `Math.random` 只允许用于图布局扰动、skeleton 宽度和本地 saved query ID fallback，不允许生成业务/取证数据。

## 13. 变更追溯

每轮有实质改动后，至少同步一项：

- `development-reports/sessions/YYYY-MM-DD.md`
- 对应 remediation plan
- 对应专题文档
- `docs/documentation-index.md`

## 14. 文档同步要求

以下变化必须同步更新图谱和专题文档：

- 分区根模型
- 文件浏览排序
- 状态字段传播
- MCP 权限模型
- 导出覆盖策略
- media handle 安全边界
- fixture / expected JSON / parser 支持边界

### V3 新增

| 变更类型 | 同步文档 |
|---------|---------|
| 新 parser | `docs/parser-support-matrix.md` + `docs/known-unsupported-formats.md` |
| 图 schema | `docs/evidence-graph-design.md` |
| 笔记本/回放 | `docs/case-notebook-design.md` |
| 规则包 | `docs/rule-pack-spec.md` |
| 批处理 | `docs/batch-processing-design.md` |
| crate 新建/删除 | `README.md` + `AGENTS.md` + `CLAUDE.md` + `docs/documentation-index.md` |
| 事实计数变更 | `docs/documentation-index.md` Section 2 快照表 |
| 质量门禁变更 | 本文档 Section 8-12 |
