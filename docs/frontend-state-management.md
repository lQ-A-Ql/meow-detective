# 前端状态管理规范

本文档明确 Forensics Workbench 前端在 TanStack Query（React Query）与 Zustand 之间的选型边界，避免同一份状态在两套机制中重复维护。

## 基本原则

前端状态分两大类：

1. **服务器状态（Server State）**：来自 Tauri 后端命令的数据快照，如案件、文件树、搜索结果、时间线事件、Artifact 统计等。这类数据的所有权在后端，前端只是缓存和展示，需要处理加载中/错误/过期/重新拉取等生命周期。
2. **客户端状态（Client/UI State）**：仅存在于当前浏览器会话中的交互状态，如当前选中的文件 ID、视图切换 Tab、抽屉是否展开、排序字段。这类状态没有对应的后端持久化来源（或只是简单地把最终结果传给后端，而不是缓存后端返回值）。

**判断准则**：如果这份状态的“真相来源”是 Tauri 命令返回的数据，用 React Query；如果“真相来源”就是用户当前的交互动作本身，用 Zustand。

## TanStack Query 使用场景

- 所有通过 `frontend/src/lib/api/*.ts` 调用 Tauri 命令得到的数据，一律通过 `frontend/src/features/*/hooks.ts` 中的 `useQuery`/`useMutation` 包装后消费，不在组件里直接 `invoke()`。
- 查询命中的场景：案件详情、数据源列表、文件树/文件行分页、搜索结果、时间线事件、Artifact 统计、图快照、MCP 配置读取、报告生成状态等。
- 使用 `queryClient.invalidateQueries` 在写操作（导入、删除、创建案件等）完成后刷新相关查询，而不是手动把返回值塞进 Zustand。
- 默认 30 秒 `staleTime`，`refetchOnWindowFocus: false`（见 `frontend/src/app/providers.tsx`），页面如需更激进的刷新策略，在具体 hook 里覆盖，不要修改全局默认值。
- 长任务（导入、批处理）通过 job snapshot 查询 + 事件订阅驱动的重新拉取，不要用 Zustand 存储任务进度本身。

## Zustand 使用场景

- 纯 UI/选择状态，例如：
  - `selection-store.ts`：当前选中的目录/文件/搜索命中/时间线事件/Artifact ID。
  - `ui-store.ts`：当前页面、抽屉开关、Viewer Tab、文件排序字段与方向。
  - `mcp-store.ts`（拆分为 `mcp-server-store.ts` + `mcp-resource-store.ts`）：MCP 服务器/资源/工具/提示的本地缓存与操作方法，因为这些数据变更频繁地由用户交互触发（连接/断开/调用工具），且需要在多个组件间共享可变的“当前选中服务器”等派生状态。
- 只在确实需要跨组件共享、且不适合作为 props 逐层传递的状态时才建 store；单个页面内部的临时状态优先用 `useState`（参考 `use-file-browser.ts` 拆分后的 `use-file-tree.ts`/`use-file-pagination.ts` 等 hook，内部状态仍是 `useState`，不是 Zustand）。
- Store 内如需调用 Tauri 命令（如 `mcp-store.ts` 中的 `loadConfig`/`connectServer`），把结果规范化后存入 store 字段，而不是让页面组件在 `useEffect` 里手动同步 React Query 数据到 Zustand——这类情况优先重新评估是否应该迁移为 React Query。

## 反模式（禁止）

- **不要**把 React Query 的返回结果原样 `set()` 进 Zustand store 做“二次缓存”，这会导致两份状态失去同步保证。
- **不要**在 Zustand store 里存放可以由现有 props/query 派生出的值（如 `selectedFile` 对象本身），优先存 ID，派生对象在消费处通过 query 结果 `find()` 得到（参考 `use-file-browser.ts` 中 `selectedFile = pagination.rows?.find(...)` 的写法）。
- **不要**为了避免 prop drilling 而滥用 Zustand；组件树浅层可以直接传 props，深层再考虑 store 或 Context。

## 拆分大型 Hook/Store 的原则

当一个 hook 或 store 文件职责过多（导航、分页、预览等混杂）导致行数超过约 300 行时，按“状态子域”拆分成多个小 hook/slice，由一个组合层（如 `use-file-browser.ts`、`mcp-store.ts`）负责把子 hook/slice 的返回值拼装成消费方需要的统一接口，保持外部调用方（页面组件、既有测试）不感知内部拆分。参考实现：

- `frontend/src/features/files/hooks/{use-file-tree,use-file-selection,use-file-pagination,use-file-preview}.ts` + `frontend/src/app/pages/use-file-browser.ts`（组合层）。
- `frontend/src/stores/{mcp-server-store,mcp-resource-store,mcp-error-utils,mcp-types}.ts` + `frontend/src/stores/mcp-store.ts`（组合层，用 Zustand slice pattern 通过 `StateCreator` 组合）。
