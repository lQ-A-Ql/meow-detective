# Forensics Workbench 设计与约束

## 1. 不可变边界

- desktop-first
- Windows-primary
- single-user
- backend-led
- 无 HTTP server
- 原始证据只读
- `crates/transport` 是前后端契约源

## 2. 架构约束

### 2.1 分层

```text
React UI
  -> frontend API wrappers
  -> Tauri commands / events
  -> app-services
  -> domain / persistence / evidence / search / timeline / reports / artifacts
```

要求：

- 前端页面不直接调用底层 `invoke`，统一通过 `src/lib/api/*`
- Tauri command 只做 `validate -> service -> DTO`
- `app-services` 负责跨 crate 编排
- parser / repo / core crate 不依赖前端或 Tauri

### 2.2 分区根模型

- 每个分区在主库中必须且只允许有一个可见根节点
- 前端 formatter 只负责显示，不负责“修树结构”
- `\`、`/`、`.` 这类真实文件系统根节点不应直接暴露到首层树根
- 根折叠必须在导入 / merge 主链路完成，而不是靠前端兜底

### 2.3 文件浏览状态

- `deleted`、`hidden`、`system` 是共享事实字段
- 文件树与文件列表共享 `showHidden`
- 状态主要通过图标 overlay 表达，文字只用于 tooltip、详情、aria 与测试辅助属性
- 排序以“目录优先 + 状态后置 + 自然名称排序”为准
- 后端是真实排序主入口，前端只做 mock 或展示兜底

## 3. 契约约束

- DTO 位于 `crates/transport/src/dto/`
- command request 位于 `crates/transport/src/commands/mod.rs`
- 对前端输出默认使用 camelCase
- Rust 与 TypeScript 类型需要手工同步
- Event topic 在 Rust 常量和 TS union 两侧保持一致

## 4. 证据与取证约束

- 原始证据源不可写
- 所有派生数据只写入 case workspace、SQLite、index 或 export 目录
- parser 遇到损坏、截断或未知输入时必须返回 error 或 warning，不能 panic
- provenance 应尽量保留：
  - data source id
  - file entry id / source object id
  - parser / extractor id 与版本
  - source attribution / offset / path / record id
  - confidence / warnings / parse status

## 5. 路径与文件系统安全

- 用户输入路径必须校验
- 导出路径必须在允许边界内使用
- 覆盖与删除行为必须显式
- `overwrite` 默认 `false`
- 文件提取与报告导出默认拒绝覆盖
- media range 与 viewer range 必须校验 handle、offset、length

## 6. MCP 安全约束

- MCP 是受控扩展通道，不是任意执行后门
- SSE 仅允许 `http/https`，禁止 embedded credentials
- stdio command 只能是可执行名，不能是路径
- 默认最小权限：
  - resources：只读
  - tools：禁用
  - prompts：只读
  - network：`localhostOnly`
- MCP 关键动作必须审计
- MCP 输出进入 UI 或报告前必须保持来源边界

## 7. 导出与媒体安全

- 文件提取与报告导出默认不覆盖
- 导出路径必须避免静默覆盖和路径逃逸
- 媒体协议不得暴露宿主真实文件路径
- 媒体 handle 必须是短期、受限、可失效的 token

## 8. 性能与规模约束

- 文件树必须懒加载
- 大文件预览必须基于 handle / range / protocol
- 搜索、时间线、文件列表必须分页
- 文件列表排序必须在完整可见集合逻辑下稳定工作
- 树子节点排序必须与懒加载一致

## 9. 前端约束

- 新增 UI 只能使用或创建公有组件
- mock 数据必须和真实链路 shape 一致
- 页面不得用文字 badge 替代真实状态字段
- 文件状态图标统一走公有组件入口

## 10. 测试与发布约束

最低 gate：

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

如图谱变更较大，再追加：

```bash
powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1 -RenderMermaid
```
