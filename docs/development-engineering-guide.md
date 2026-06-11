# Forensics Workbench 开发工程化规范

## 1. 工程基线

Forensics Workbench 是 Windows-primary、desktop-first 的 Tauri 2 应用。Rust 后端负责证据处理、任务编排、持久化、搜索、时间线、工件解析和报告导出；React/TypeScript 前端负责调查员界面。项目没有 HTTP server，前后端只通过 Tauri commands 和 events 通信。

默认技术栈：

- Rust stable，edition 2021，workspace 依赖集中在 root `Cargo.toml`。
- Tauri 2 shell 位于 `apps/desktop/src-tauri/`。
- Frontend 使用 React 18、React Router 7、TanStack Query、Zustand、Vite 6、Tailwind CSS 4。
- 包管理器使用 pnpm，版本由 `frontend/package.json` 声明。
- DTO、commands、events、errors、paging 的共享契约源是 `crates/transport`。

## 2. Workspace 约束

root `Cargo.toml` 是 Rust workspace 的唯一成员清单。新增 crate 时必须：

- 使用 kebab-case 名称。
- 加入 `[workspace].members`。
- 将公共依赖优先放入 `[workspace.dependencies]`。
- 明确依赖方向，不让底层 crate 依赖 Tauri、React、frontend 或 app command。

推荐依赖方向：

```text
apps/desktop/src-tauri
  -> app-services
  -> transport
  -> domain

app-services
  -> domain
  -> transport
  -> persistence-sqlite
  -> evidence/search/timeline/artifacts/reports crates

core crates
  -> domain/infrastructure as needed
  -> 不依赖 Tauri command 或 frontend
```

## 3. Feature 开发流程

新增或修改用户可见能力时，按以下顺序推进：

1. 在 `crates/transport/src/dto/<domain>.rs` 定义或扩展 DTO，并在 `dto/mod.rs` re-export。
2. 如需要 request type，在 `crates/transport/src/commands/mod.rs` 定义。
3. 在 `crates/app-services/src/<domain>_service.rs` 实现编排逻辑。
4. 在 `apps/desktop/src-tauri/src/commands/<domain>_commands.rs` 增加 thin command。
5. 在 `apps/desktop/src-tauri/src/lib.rs` 注册 command。
6. 在 `frontend/src/types/models.ts` 同步 TypeScript 类型。
7. 在 `frontend/src/lib/api/<domain>.ts` 增加 API wrapper。
8. 在 `frontend/src/lib/api/mock-data.ts` 增加 mock fallback。
9. 在 `frontend/src/features/<domain>/hooks.ts` 增加 TanStack Query hook。
10. 在 page/component 中消费 hook，不直接调用 Tauri `invoke`。
11. 增加对应 Rust test、Vitest test、fixture 或明确不可自动化的手工验证。
12. 更新相关 docs 与 development record。

## 4. Rust 规范

### DTO 与 serde

- 所有 serializable API DTO 放在 `crates/transport/src/dto/`。
- Rust DTO 使用 `Dto` 后缀；frontend 类型通常去掉 `Dto` 后缀。
- DTO 使用 `#[serde(rename_all = "camelCase")]`。
- 可选字段使用 `#[serde(skip_serializing_if = "Option::is_none")]`。
- event topic 字符串由 `crates/transport/src/events/mod.rs` 定义，frontend `EventTopic` union 必须同步。

### Command 层

Tauri command 必须保持薄适配：

- 做输入校验、状态获取、service 调用和 DTO 返回。
- 不直接写复杂 SQL。
- 不实现 parser、indexer、timeline 或 report 业务逻辑。
- 不返回裸内部错误；面向 UI 的错误需要脱敏。
- 返回类型遵循 Tauri 2 可序列化约束，当前 command 以 `Result<T, String>` 为主。

### Service 层

`app-services` 负责跨 crate 编排：

- 控制事务和 job lifecycle。
- 连接 repo、evidence reader、parser、indexer、report exporter。
- 将 domain/core 输出转换为 transport DTO。
- 对长任务发出 progress、partial result、failure/cancel events。

### 持久化层

- SQL 默认位于 `crates/persistence-sqlite/src/repositories/` 或 migrations。
- schema 变更只通过 migration script 进入。
- migration 必须可重复检测、失败 rollback、不产生半应用状态。
- 关键查询路径必须有索引，尤其是 case、data_source、parent、timeline ts、job status。

## 5. Frontend 规范

### API 与状态分层

- `frontend/src/lib/api/client.ts` 负责 mock/tauri mode 分流。
- `VITE_API_MODE === 'tauri'` 时调用 `@tauri-apps/api/core` 的 `invoke`。
- mock mode 使用 mock provider，保持与 Rust DTO shape 一致。
- `src/lib/api/<domain>.ts` 只做 thin wrapper。
- `src/features/<domain>/hooks.ts` 使用 TanStack Query 管理 server state。
- Zustand stores 只保存 UI state、selection state、MCP local state 等客户端状态。
- 页面和组件不直接使用 Tauri `invoke`。

### TypeScript 与路径

- `@/` 指向 `frontend/src/`，由 `vite.config.ts` 和 `tsconfig.json` 共同配置。
- `tsconfig.json` 保持 `strict: true`。
- UI primitives 位于 `src/app/components/ui/`。
- app-shell layout 位于 `src/components/layout/`，不要在 `src/app/components/` 新建布局组件。

### Tailwind 4

- Tailwind 由 `@tailwindcss/vite` 插件和 CSS-first 配置驱动。
- 不添加 `tailwind.config.js`。
- 主题 token 维护在 `src/styles/theme.css`。

## 6. Event 与任务工程化

Backend event push 路径：

1. Service 或 task manager 构造 payload。
2. Tauri event bridge 使用 `Emitter` emit topic。
3. Frontend bridge/subscriber 接收 event。
4. `EventBus` publish typed envelope。
5. feature hook 或缓存失效逻辑更新 TanStack Query 缓存。

要求：

- 新增 event topic 时，Rust constants、Rust enum、TS union、subscriber/cache invalidation 必须同步。
- event payload 不暴露裸 host evidence path，除非产品明确需要且已脱敏/授权。
- job progress 必须能解释 running、completed、failed、cancelled 状态。
- 取消事件不能只更新 UI，backend 任务必须检查 cancellation token 或等价状态。

## 7. Evidence 与安全约束

- 原始 evidence source 视为只读输入。
- 导入、预览、hash、parser、search indexing 不得修改 evidence source。
- 所有来自用户输入或证据内部的路径必须 canonicalize 或做安全相对路径校验。
- 导出和 case 操作必须限制在受控 case/export 路径。
- parser 面对 malformed input 必须返回 typed error 或 warning，不得 panic。
- 报告和 artifact/timeline 输出应保留 source attribution、parser/extractor 版本、confidence 或等价说明。

## 8. 测试策略

默认 gate：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
git diff --check
```

专项测试选择：

- DTO/event 变更：Rust serde test + frontend type/API test。
- Command 变更：command/service targeted test，必要时 desktop crate test。
- Parser 变更：fixture test，覆盖 malformed、empty、truncated、real fixture opt-in。
- Import/FS/search/timeline 变更：端到端或 service integration test。
- Frontend hook/page 变更：Vitest + Testing Library，覆盖 loading/error/empty/success。

慢测规则：

- 真实 E01、真实 Windows hive 或大镜像测试默认 opt-in。
- 报告中记录环境变量、样本来源、hash、大小和运行时间。
- CI tiny fixtures 不替代真实样本验收，只覆盖默认回归路径。

## 9. 文档与追溯

每轮实质开发或审计后，至少更新一个追溯位置：

- `development-reports/sessions/YYYY-MM-DD.md`：本轮目标、改动、验证、剩余风险。
- `docs/remediation-plan-*.md`：阶段性修复状态。
- 专项设计或审计文档：当接口、约束、架构图或算法模型发生变化时更新。

文档必须区分：

- 设计目标。
- 当前实现。
- 已验证事实。
- 未完成/风险/手工 gate。

## 10. 发布前工程 Gate

发布前至少确认：

- Rust fmt、clippy、workspace tests 通过。
- Frontend typecheck、lint、tests、build 通过。
- `cargo audit` 和 `cargo deny` 风险有明确处理或记录。
- DTO/event/topic 双端同步。
- migration 从旧 schema 升级路径有测试或手工验证。
- 取证主链路导入、浏览、搜索、时间线、工件、报告有 smoke 验证。
- 文档入口、审计方案、修复计划和开发记录状态一致。
