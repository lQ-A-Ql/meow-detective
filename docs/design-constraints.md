# Forensics Workbench 设计与约束文档

## 1. 不可变设计边界

Forensics Workbench 的核心边界如下：

- **Desktop-first**：目标是单机桌面取证工作台，不提供 Web SaaS 或多人协作 server。
- **Windows-primary**：Windows 10/11 是主要运行、验证和交付平台。
- **Backend-led**：Rust 后端拥有取证处理、持久化、搜索、时间线、工件解析和报告导出的事实控制权。
- **No HTTP server**：前后端只通过 Tauri commands 和 events 通信，不引入常驻 HTTP server。
- **Single-user first**：当前模型面向本机单用户案件操作，不引入远程账号、权限组或协作冲突解决。
- **Evidence read-only**：原始证据源只读；所有派生数据写入 case workspace、SQLite、index 或 export 目录。

## 2. 架构约束

### 分层

```text
React UI
  -> frontend API wrappers
  -> Tauri invoke/events
  -> Tauri command 层
  -> app-services
  -> domain / transport / persistence / evidence / search / timeline / artifacts / reports
```

强约束：

- UI 不直接读写 SQLite、evidence image 或 backend filesystem。
- UI 不直接调用 `invoke`，除非封装在 `frontend/src/lib/api/client.ts` 或明确的 bridge 层。
- Tauri command 不实现领域算法，不直接承载复杂 SQL。
- `app-services` 是跨 crate 编排层，不把 Tauri 类型泄漏到底层 crate。
- `transport` 是 IPC 契约源，不依赖 app-service、Tauri 或 frontend。
- `domain` 只表达核心实体和领域规则，不依赖 persistence 或 UI。

### 分区根模型

- 每个分区在主库中必须且只允许有一个**可见分区根节点**。
- 可见分区根继续复用 placeholder root 模型，不再额外发明第二套首层根节点表示。
- placeholder 内部路径编码为 `__partition_placeholder__/{partition_index}/{status}`，分区绑定以 `partition_index` 为准，不以显示名为准。
- staging merge 必须把真实文件系统根标记节点 `\`、`/`、`.` 折叠进对应分区根，不能让它们暴露在文件树首层。
- 若主库与 staging 临时失配，merge 事务内可以合成 placeholder root，但不能回退到“把裸 staging 根行直接插入主库首层”的旧路径。
- 读取侧允许做防御性归一化：若历史数据或异常数据仍残留裸根，`file_service` 可以把它们映射成分区显示名，但这只是兜底，不替代写入侧收口。

### Crate 依赖

允许方向：

- `apps/desktop/src-tauri` -> `app-services`, `transport`
- `app-services` -> `domain`, `transport`, core/service crates
- `persistence-sqlite` -> `domain` 或 `transport`，仅在 repo 契约需要时依赖
- `search`, `timeline`, `artifacts-*`, `reports`, `evidence-core`, `fs-*`, `image-*` -> 更底层 helper

禁止：

- core parser crate 依赖 Tauri。
- `transport` 依赖 frontend 或 command implementation。
- frontend source 把 backend 文件布局当作运行时真相。

## 3. 契约约束

- DTO 位于 `crates/transport/src/dto/`。
- Request/command shape 位于 `crates/transport/src/commands/mod.rs` 或可复用的 command-local request type。
- DTO JSON 字段使用 camelCase。
- Rust optional 字段和 frontend optional 字段必须同步。
- Event topic 在 `crates/transport/src/events/mod.rs` 定义，并在 `frontend/src/types/models.ts` 中同步为 union。
- 文件浏览真实链路的查询契约以 `transport` 为唯一事实源，至少包括：
  - `showHidden`
  - `sortKey`
  - `sortDirection`
- `deleted`、`hidden`、`system` 是共享状态字段，不是前端局部展示字段；它们同时参与 DTO、持久化、可见性过滤、排序和图标表现。
- 没有 codegen 时，任何 DTO / event / request 变更都必须手工双端对照并附带测试或审计记录。

## 4. 数据与证据约束

### 案件数据

- 案件元数据、数据源、文件条目、工件、时间线、任务、报告、标签和审计日志写入 SQLite。
- 索引数据写入 case-scoped index / cache 目录。
- runtime cache 只保存运行期 handle / cache 状态，不作为取证事实唯一来源。

### 原始证据

- 原始磁盘镜像、E01、逻辑目录或文件只读打开。
- 导入时可以计算 hash、读取 metadata、枚举目录、抽取文本和 artifacts，但不得修改证据源。
- UI 展示源路径时要考虑脱敏；event payload 默认不传裸 host path。

### 状态事实

- `deleted`、`hidden`、`system` 必须视为证据事实字段，而不是纯前端推断值。
- `hidden` / `system` 优先来自文件系统属性；无法可靠取得时，才允许使用受控名称规则兜底。
- `deleted` 的收集范围和语义必须在文档中明确标注当前支持边界；未支持的文件系统不能伪造已删除状态。
- 任何状态兜底逻辑都必须保持可解释，不能掩盖原始 parser 无法确认的事实缺口。

### Provenance

可用于报告或审计的派生对象应尽量保留：

- data source id / file entry id / source object id
- parser 或 extractor id / version
- source offset、path、record id 或等价 attribution
- confidence、warnings、parse status

## 5. 路径与文件系统安全

必须做：

- 对用户输入路径和 case / export 路径做 canonicalize 或等价安全解析。
- 对相对路径做 `..`、absolute path、prefix escape 防护。
- 删除 case、导出文件、临时文件清理时限制在预期根目录内。
- viewer / media range read 检查 handle、offset、length 和 declared size。

禁止：

- 从证据内部路径直接拼接到 host filesystem 写入。
- command 层接受任意 path 后直接删除、移动或覆盖。
- 在 event 或错误消息中泄漏不必要的完整 host path。

## 6. 性能与规模约束

项目必须优先支持大镜像和大目录的渐进式体验：

- 文件树必须懒加载，避免一次性返回全树。
- 大文件预览使用 range / handle / protocol，不把整文件 base64 塞进 IPC。
- 搜索结果、时间线、artifact rows 和文件列表必须分页。
- 文件浏览的**真实排序**必须在后端完整可见集合上完成，再做分页切片，不能做“每页各自排序”。
- 树子节点排序必须在后端统一执行，保证懒加载批次之间顺序稳定。
- 前端排序器只允许用于 mock mode 或极小范围展示兜底，不能覆盖真实 Tauri 返回顺序。
- 导入、hash、index、artifact extraction 是 job 化任务，需要进度、取消和错误状态。

## 7. 错误处理约束

- Parser 错误分为 unsupported、invalid data、truncated、recoverable warning、fatal error。
- 用户可见错误要脱敏，开发日志保留足够定位信息。
- 后台任务错误必须转化为 job failed / event，不允许静默吞掉。
- `unwrap` / `expect` 不用于生产输入路径、证据解析路径或 IPC 入口。
- malformed evidence 不能导致 panic、OOM 或无限循环。

## 8. Event 与状态约束

- Backend 是 job 状态和文件浏览事实的真相源。
- Frontend event 只用于刷新和反馈，不作为唯一持久状态。
- Event topic 名称必须满足 Tauri event name 限制。
- 新增 event 需要同步 Rust constant、TS union、subscriber 和 cache invalidation。
- job 状态只允许合理转换：`pending -> running -> completed/failed/cancelled`。
- 文件浏览中的 `showHidden`、排序字段和当前目录是查询状态，不应被事件偷偷重排或重写。

## 9. Frontend UX 约束

- 第一屏是工作台，而不是营销 landing page。
- 桌面取证工具 UI 应高密度、稳定、可扫描，避免装饰性 hero 和过度卡片化。
- 常用分析页面提供 loading、empty、error、success 状态。
- 数据表、文件树、时间线和查看器必须保持布局稳定，不因角标、标签或长文本破坏工作区。
- 文件树首层、面包屑根节点、当前目录标题和案件首页分区名必须统一显示为 `分区x（LABEL）`。
- 隐藏 / 系统 / 已删除状态的主要表达应是图标级叠加；文字只用于 tooltip、aria、详情面板或测试辅助属性。
- 文件树和文件表共用一个 `showHidden` 开关，默认隐藏隐藏 / 系统文件。
- mock mode 不能继续使用失真的旧根模型；mock 树的首层必须也是分区根，而不是把 `EFI`、`Windows`、`System32` 直接放成根。

## 10. MCP 约束

- MCP 是受控扩展入口，不是绕过安全边界的万能执行通道。
- Stdio / SSE server 配置需要验证 command / url / env / path。
- Tool call 错误需要隔离并脱敏。
- MCP 输出进入 UI 或报告前应标注来源，不与取证 parser 原生事实混淆。
- 禁止默认授予会修改 evidence source 或 case 外路径的能力。

## 11. 测试与发布约束

- P0 / P1 修复默认需要 regression test；无法自动化时必须记录手工 gate。
- tiny fixtures 用于默认 CI；真实大样本使用 opt-in 慢测。
- 发布前必须通过 Rust gate、frontend gate、依赖安全 gate 和文档一致性检查。
- 文档一致性检查至少包含：
  - `powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1`
  - 必要时追加 `-RenderMermaid`
- 若某个 gate 暂时允许失败，必须有 owner、expiry 和风险说明。

## 12. 文档约束

文档更新时必须避免三类混淆：

- 把设计目标写成已实现事实。
- 把 mock 数据写成真实取证能力。
- 把历史审计结论写成当前状态。

涉及分区根模型、排序契约、`showHidden`、`deleted/hidden/system` 状态传播、架构、事件、数据库或安全边界的变更，必须同步更新：

- `docs/model-architecture-algorithm-diagrams.md`
- `docs/development-engineering-guide.md`
- `docs/engineering-audit-plan.md`
- `docs/documentation-index.md`
