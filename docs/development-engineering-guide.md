# Forensics Workbench 开发工程化规范

## 1. 工程基线

Forensics Workbench 是 Windows-primary、desktop-first 的 Tauri 2 应用。Rust 后端负责证据处理、任务编排、持久化、搜索、时间线、工件解析和报告导出；React / TypeScript 前端负责调查员界面。项目没有 HTTP server，前后端只通过 Tauri commands 和 events 通信。

默认技术栈：

- Rust stable，edition 2021，workspace 依赖集中在 root `Cargo.toml`
- Tauri 2 shell 位于 `apps/desktop/src-tauri/`
- Frontend 使用 React 18、React Router 7、TanStack Query、Zustand、Vite、Tailwind CSS 4
- 包管理器使用 pnpm
- DTO、commands、events、errors、paging 的共享契约源是 `crates/transport`

## 2. Workspace 约束

root `Cargo.toml` 是 Rust workspace 的唯一成员清单。新增 crate 时必须：

- 使用 kebab-case 名称
- 加入 `[workspace].members`
- 将公共依赖优先放入 `[workspace.dependencies]`
- 明确依赖方向，不让底层 crate 依赖 Tauri、React、frontend 或 app command

推荐依赖方向：

```text
apps/desktop/src-tauri
  -> app-services
  -> transport

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
2. 如需要 request type，在 `crates/transport/src/commands/mod.rs` 定义；涉及排序、过滤、分页时，优先先改这里。
3. 在 `crates/app-services/src/<domain>_service.rs` 或相关 service 模块实现编排逻辑。
4. 在 `apps/desktop/src-tauri/src/commands/<domain>_commands.rs` 增加 thin command。
5. 在 `apps/desktop/src-tauri/src/lib.rs` 注册 command。
6. 在 `frontend/src/types/models.ts` 同步 TypeScript 类型。
7. 在 `frontend/src/lib/api/<domain>.ts` 增加 API wrapper。
8. 在 `frontend/src/lib/api/mock-data.ts` 增加 mock fallback，且 shape 必须对齐真实契约。
9. 在 `frontend/src/features/<domain>/hooks.ts` 增加 TanStack Query hook。
10. 在 page / component 中消费 hook，不直接调用 Tauri `invoke`。
11. 增加 Rust test、Vitest test、fixture 或明确不可自动化的手工验证。
12. 更新相关 docs 和 development record。

### 文件浏览 / 分区根相关功能的额外顺序

凡是涉及文件树、文件表、分区显示、`showHidden`、排序、`deleted/hidden/system` 状态的改动，必须额外按下面的链路收口：

1. **先改契约**：`transport` 中的 DTO / request 字段是前后端共同事实源。
2. **再改真实服务链路**：`file_service` 必须完成过滤、排序、分页、根归一化和状态传播。
3. **再改导入 / merge 链路**：若改动影响分区根模型，必须同时检查 import pipeline、placeholder root 和 staging merge。
4. **再接 Tauri command**：command 只传递 request，不在 command 层补排序或补树结构。
5. **再接前端**：API、hooks、页面、公有组件、公有 formatter 统一接线。
6. **最后修 mock**：mock 只能追随真实模型，不能反过来主导实现。

如果真实 Tauri 链路没有接住，禁止只在前端页面里做“视觉补丁”来伪装问题已解决。

## 4. Rust 规范

### DTO 与 serde

- 所有 serializable API DTO 放在 `crates/transport/src/dto/`
- Rust DTO 使用 `Dto` 后缀；frontend 类型通常去掉 `Dto`
- DTO 使用 `#[serde(rename_all = "camelCase")]`
- 可选字段使用 `#[serde(skip_serializing_if = "Option::is_none")]`
- event topic 字符串由 `crates/transport/src/events/mod.rs` 定义，frontend `EventTopic` union 必须同步

### Command 层

Tauri command 必须保持薄适配：

- 只做输入校验、状态获取、service 调用和 DTO 返回
- 不直接写复杂 SQL
- 不实现 parser、indexer、timeline 或 report 业务逻辑
- 不返回裸内部错误；面向 UI 的错误需要脱敏
- 返回类型遵循 Tauri 2 可序列化约束

### Service 层

`app-services` 负责跨 crate 编排：

- 控制事务和 job lifecycle
- 连接 repo、evidence reader、parser、indexer、report exporter
- 将 domain / core 输出转换为 transport DTO
- 对长任务发出 progress、partial result、failure / cancel events

额外约定：

- 文件列表真实排序必须在 service 层完成，再分页切片。
- 树子节点真实排序也必须在 service 层完成，保证懒加载时序稳定。
- 若改动分区根模型，必须同时审查：
  - import pipeline 中的 placeholder root 创建 / 替换
  - staging merge 的根折叠规则
  - `file_service` 读取侧的首层归一化
- placeholder root 的绑定键是 `partition_index`，不是显示名。

### 持久化层

- SQL 默认位于 `crates/persistence-sqlite/src/repositories/` 或 migrations
- schema 变更只通过 migration script 进入
- migration 必须可重复检测、失败回滚、不产生半应用状态
- 关键查询路径必须有索引，尤其是 case、data_source、parent、timeline ts、job status

## 5. Frontend 规范

### API 与状态分层

- `frontend/src/lib/api/client.ts` 负责 mock / tauri mode 分流
- `VITE_API_MODE === 'tauri'` 时调用 `@tauri-apps/api/core` 的 `invoke`
- mock mode 使用 mock provider，保持与 Rust DTO shape 一致
- `src/lib/api/<domain>.ts` 只做 thin wrapper
- `src/features/<domain>/hooks.ts` 使用 TanStack Query 管理 server state
- Zustand stores 只保存 UI state、selection state、MCP local state 等客户端状态
- 页面和组件不直接使用 Tauri `invoke`

### 文件浏览前端约定

- `getFileRowsPage` 必须把 `showHidden`、`sortKey`、`sortDirection` 真实传给 Tauri。
- 真实 Tauri mode 返回的 rows 不能再在页面内做第二次重排序。
- `sortFileEntries(...)` 只用于 mock mode 或展示级兜底。
- 分区显示统一走 `partition-display.ts` 中的公有 formatter。
- 文件 / 目录图标统一走公有组件，状态角标不能在各页面私有分叉实现。
- mock 样本必须覆盖：
  - normal
  - hidden / system
  - deleted
  - hidden + deleted
- mock 根模型必须和真实链路一致：首层是分区根，子层才是 `EFI`、`Windows`、`Users` 等目录。

### TypeScript 与路径

- `@/` 指向 `frontend/src/`
- `tsconfig.json` 保持 `strict: true`
- UI primitives 位于 `src/app/components/ui/`
- app-shell layout 位于 `src/components/layout/`

### Tailwind 4

- Tailwind 由 `@tailwindcss/vite` 插件和 CSS-first 配置驱动
- 不新增 `tailwind.config.js`
- 主题 token 维护在 `src/styles/theme.css`

## 6. Event 与任务工程化

Backend event push 路径：

1. Service 或 task manager 构造 payload
2. Tauri event bridge 使用 `Emitter` emit topic
3. Frontend bridge / subscriber 接收 event
4. `EventBus` publish typed envelope
5. feature hook 或缓存失效逻辑更新 TanStack Query cache

要求：

- 新增 event topic 时，Rust constants、Rust enum、TS union、subscriber / cache invalidation 必须同步
- event payload 不暴露裸 host evidence path，除非产品明确需要且已脱敏 / 授权
- job progress 必须能解释 running、completed、failed、cancelled 状态
- 取消事件不能只更新 UI，backend 任务必须检查 cancellation token 或等价状态

## 7. Evidence 与安全约束

- 原始 evidence source 视为只读输入
- 导入、预览、hash、parser、search indexing 不得修改 evidence source
- 所有来自用户输入或证据内部的路径必须 canonicalize 或做安全相对路径校验
- 导出和 case 操作必须限制在受控 case / export 路径
- parser 面对 malformed input 必须返回 typed error 或 warning，不得 panic
- 报告和 artifact / timeline 输出应保留 source attribution、parser / extractor 版本、confidence 或等价说明

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
powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1
```

专项测试选择：

- DTO / event / request 变更：Rust serde test + frontend type / API test
- Command 变更：command / service targeted test，必要时 desktop crate test
- Parser 变更：fixture test，覆盖 malformed、empty、truncated、real fixture opt-in
- Import / FS / search / timeline 变更：端到端或 service integration test
- Frontend hook / page 变更：Vitest + Testing Library，覆盖 loading / error / empty / success

文件浏览与分区根专项至少覆盖：

- placeholder root 是否按 partition index 编码和绑定
- merge 后首层是否只剩分区根，不暴露裸 `\`、`/`、`.` 或错误同级 `EFI`
- `showHidden=false` 与 `showHidden=true` 的过滤行为
- 列表排序是否为“目录优先 + 状态后置 + 主字段 + 自然名兜底”
- 树排序是否稳定为目录自然升序
- Tauri mode 下前端是否仅展示后端返回顺序而不再次重排

慢测规则：

- 真实 E01、真实 Windows hive 或大镜像测试默认 opt-in
- 报告中记录环境变量、样本来源、hash、大小和运行时间
- CI tiny fixtures 不替代真实样本验收

## 9. 文档与追溯

每轮实质开发或审计后，至少更新一个追溯位置：

- `development-reports/sessions/YYYY-MM-DD.md`：本轮目标、改动、验证、剩余风险
- `docs/remediation-plan-*.md`：阶段性修复状态
- 专项设计或审计文档：当接口、约束、架构图或算法模型发生变化时同步更新

文档必须区分：

- 设计目标
- 当前实现
- 已验证事实
- 未完成 / 风险 / 手工 gate

涉及分区根、排序契约、状态字段或 Mermaid 图谱的变更时，`docs/documentation-index.md` 的事实快照也要一起更新。

## 10. 发布前工程 Gate

发布前至少确认：

- Rust fmt、clippy、workspace tests 通过
- Frontend typecheck、lint、tests、build 通过
- `cargo audit` 和 `cargo deny` 风险有明确处理或记录
- DTO / event / topic 双端同步
- migration 从旧 schema 升级路径有测试或手工验证
- 取证主链路导入、浏览、搜索、时间线、工件、报告有 smoke 验证
- 文档入口、审计方案、修复计划和开发记录状态一致
- `scripts/check-doc-drift.ps1` 通过；文档或图谱变更较大时追加 `-RenderMermaid`
