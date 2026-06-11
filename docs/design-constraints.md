# Forensics Workbench 设计与约束文档

## 1. 不可变设计边界

Forensics Workbench 的核心边界：

- **Desktop-first**：第一目标是单机桌面取证工作台，不提供 Web SaaS 或多人协作 server。
- **Windows-primary**：Windows 10/11 是主要运行与验证平台。
- **Backend-led**：Rust 后端拥有取证处理、数据持久化、搜索、时间线、工件解析和报告导出。
- **No HTTP server**：前端/后端不通过 HTTP 通信；只使用 Tauri commands 和 events。
- **Single-user first**：当前模型以本机单用户案件为主，不引入远程账号、权限组或协作冲突解决。
- **Evidence read-only**：原始证据源是只读输入，所有派生数据写入 case workspace、database、index 或 export output。

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
- `transport` 是 IPC 契约源，不能依赖 app service、Tauri 或 frontend。
- `domain` 只表达核心实体和领域规则，不依赖 persistence 或 UI。

### Crate 依赖

允许的方向：

- `apps/desktop/src-tauri` -> `app-services`, `transport`
- `app-services` -> `domain`, `transport`, core/service crates
- `persistence-sqlite` -> `domain`/`transport` only when required by repo contract
- `search`, `timeline`, `artifacts-*`, `reports`, `evidence-core`, `fs-*`, `image-*` -> lower-level helpers only

禁止：

- core parser crate 依赖 Tauri。
- `transport` 依赖 frontend 或 command implementation。
- frontend source 依赖 backend file layout 作为运行时真相。

## 3. 契约约束

- DTO 位于 `crates/transport/src/dto/`。
- Request/command shape 位于 `crates/transport/src/commands/mod.rs` 或 command-local request type，优先可复用契约。
- DTO JSON 字段使用 camelCase。
- Rust optional 字段和 frontend optional 字段必须同步。
- Event topic 在 `crates/transport/src/events/mod.rs` 定义，并在 `frontend/src/types/models.ts` 中同步为 union。
- 没有 codegen 时，任何 DTO/event 修改都必须手工双端对照并增加测试或审计记录。

## 4. 数据与证据约束

### 案件数据

- 案件元数据、数据源、文件条目、工件、时间线、任务、报告、标签和审计日志写入 SQLite。
- 索引数据写入 case-scoped index/cache 目录。
- runtime cache 只保存运行期 handle/cache 状态，不作为取证事实唯一来源。

### 原始证据

- 原始磁盘镜像、E01、逻辑目录或文件只读打开。
- 导入时可计算 hash、读取 metadata、枚举目录、抽取文本和 artifacts，但不得修改证据源。
- UI 展示源路径时要考虑脱敏；event payload 默认不传裸 host path。

### Provenance

可用于报告或审计的派生对象应尽量保留：

- data source id / file entry id / source object id。
- parser 或 extractor id/version。
- source offset、path、record id 或等价 attribution。
- confidence、warnings、parse status。

## 5. 路径与文件系统安全

必须做：

- 对用户输入路径和 case/export 路径做 canonicalize 或等价安全解析。
- 对相对路径做 `..`、absolute path、prefix escape 防护。
- 删除 case、导出文件、临时文件清理时限制在预期根目录内。
- viewer/media range read 检查 handle、offset、length 和 declared size。

禁止：

- 从证据内部路径直接拼接到 host filesystem 写入。
- command 层接受任意 path 后直接删除、移动或覆盖。
- 在 event 或错误消息中泄漏不必要的完整 host path。

## 6. 性能与规模约束

项目必须优先支持大镜像和大目录的渐进式体验：

- 文件树使用懒加载、分页或按父节点查询，避免一次性返回全树。
- 大文件预览使用 range/handle/protocol，不把整文件 base64 塞进 IPC。
- 搜索结果、时间线、artifact rows 必须分页。
- 导入、hash、index、artifact extraction 是 job 化任务，需要进度、取消和错误状态。
- SQLite 写入应批量化或 staged，避免每个文件同步小事务。
- 搜索、时间线、catalog projection 的延迟构建必须有 cache status 或 progress 可见性。

## 7. 错误处理约束

- Parser 错误分为 unsupported、invalid data、truncated、recoverable warning、fatal error。
- 用户可见错误要脱敏，开发日志保留足够定位信息。
- 后台任务错误必须转化为 job failed/event，不允许静默吞掉。
- `unwrap`/`expect` 不用于生产输入路径、证据解析路径或 IPC 入口。
- malformed evidence 不能导致 panic、OOM 或无限循环。

## 8. Event 与状态约束

- Backend 是 job 状态真相源。
- Frontend event 只用于刷新和反馈，不作为唯一持久状态。
- Event topic 名称必须满足 Tauri event name 限制。
- 新增 event 需要同步 Rust enum、string constant、TS union、subscriber/cache invalidation。
- job 状态只允许合理转换：pending -> running -> completed/failed/cancelled。

## 9. Frontend UX 约束

- 应用第一屏是工作台体验，不做营销 landing page。
- 桌面取证工具 UI 应高密度、稳定、可扫描，避免装饰性 hero 和过大卡片。
- 常用分析页面提供 loading、empty、error、success 状态。
- 数据表、树、时间线和查看器必须保持布局稳定，不因内容长度破坏工作区。
- mock mode 用于 standalone frontend dev，不得让 mock-only 能力伪装为真实取证事实。

## 10. MCP 约束

- MCP 是受控扩展入口，不是绕过安全边界的万能执行通道。
- Stdio/SSE server 配置需要验证 command/url/env/path。
- Tool call 错误需要隔离并脱敏。
- MCP 输出进入 UI 或报告前应标注来源，不与取证 parser 原生事实混淆。
- 禁止默认授予会修改 evidence source 或 case 外路径的能力。

## 11. 测试与发布约束

- P0/P1 修复默认需要 regression test；无法自动化时必须记录手工 gate。
- Tiny fixtures 用于默认 CI；真实大样本使用 opt-in 慢测。
- 发布前必须通过 Rust gate、frontend gate、依赖安全 gate 和文档一致性检查。
- 若某个 gate 暂时允许失败，必须有 owner、expiry 和风险说明。

## 12. 文档约束

文档更新时必须避免三类混淆：

- 把设计目标写成已实现事实。
- 把 mock 数据写成真实取证能力。
- 把历史审计结论写成当前状态。

涉及架构、契约、算法、事件、数据库或安全边界的变更，必须同步更新相应 docs 或 development report。
