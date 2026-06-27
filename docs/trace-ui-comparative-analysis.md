# trace-ui 对比分析与可借鉴点报告

## 1. 目标

本文档对比本项目与 `imj01y/trace-ui` 在“大规模数据浏览与性能优化”上的关键实现，重点关注以下 4 层：

1. 前端滚动与浏览状态层
2. 后端数据访问与浏览接口层
3. 缓存与索引层
4. 会话状态与跨功能复用层

报告目标不是评估两个项目的业务功能，而是回答两个工程问题：

- `trace-ui` 的哪些设计思路可以迁移到本项目？
- 哪些实现只适用于 trace 浏览，不适合直接照搬到镜像/文件取证预览？

## 2. 证据来源

### 2.1 本项目源码证据

- 前端预览入口与按需启用：`frontend/src/app/pages/FileBrowser.tsx`
- 前端预览 hooks：`frontend/src/features/files/hooks.ts`
- Hex 视图：`frontend/src/components/viewers/HexViewer.tsx`
- Tauri 预览命令：`apps/desktop/src-tauri/src/commands/file_commands.rs`
- 预览读取底座：`crates/app-services/src/file_service/viewer.rs`
- 媒体协议：`apps/desktop/src-tauri/src/media_protocol.rs`
- 运行时缓存：`crates/runtime-cache/src/lib.rs`
- 文件系统抽象：`crates/evidence-core/src/filesystem/mod.rs`
- 文件系统 reader：
  - `crates/fs-ntfs/src/lib.rs`
  - `crates/fs-fat/src/lib.rs`
  - `crates/fs-exfat/src/lib.rs`

### 2.2 trace-ui 远端源码证据

通过 GitHub CLI 远端读取以下源码：

- `src-web/src/hooks/useVirtualScroll.ts`
- `src-web/src/hooks/useTraceStore.ts`
- `src-web/src/components/VirtualScrollArea.tsx`
- `crates/trace-core/src/cache.rs`
- `crates/trace-core/src/engine/browse.rs`

### 2.3 证据使用原则

- 优先使用源码结构、函数职责和状态模型作为判断依据。
- README 仅用来辅助确认项目意图，不作为核心工程结论来源。
- “可借鉴”必须明确到“借鉴哪一层、借鉴什么模式、为什么成立”。
- “不可直接借鉴”必须说明边界差异，而不是简单下结论。

## 3. 分层比对

## 3.1 前端滚动与浏览状态层

### 本项目现状

本项目当前的 Hex 大文件浏览是组件内自管滚动：

- `HexViewer.tsx` 内部维护 `scrollTop`、`containerHeight`、`visibleRange`、`visibleLines`
- 当滚动接近顶部/底部时，通过 `onNeedMoreRange(direction)` 通知外层加载前后 chunk
- `useFileViewer()` 维护：
  - `loadedRanges`
  - `loadedChunks`
  - `activeOffset`
  - `jumpOffsetInput`
  - `loadNextRange()` / `loadPreviousRange()`

对应证据：

- `frontend/src/components/viewers/HexViewer.tsx:38-116`
- `frontend/src/features/files/hooks.ts:128-271`

### trace-ui 的做法

`trace-ui` 将滚动状态抽成独立 hook：

- `useVirtualScroll` 输入：
  - `totalCount`
  - `rowHeight`
  - `overscan`
  - `wheelSpeed`
- 输出：
  - `currentRow`
  - `visibleRows`
  - `maxRow`
  - `startIdx`
  - `endIdx`
  - `scrollToRow()`
  - `containerRef`
  - `scrollbarProps`

它还显式实现了：

- `wheel` 事件的节流合并更新
- `ResizeObserver` 驱动容器测量
- 自定义滚动条与右侧 gutter 分层

对应证据：

- `trace-ui/src-web/src/hooks/useVirtualScroll.ts:53-174`
- `trace-ui/src-web/src/components/VirtualScrollArea.tsx:28-67`

### 可借鉴点

- 借鉴“滚动控制层与显示层分离”的结构，而不是把全部逻辑塞进 `HexViewer`。
- 将以下状态抽成统一 preview scroll model：
  - 当前视口
  - 当前 chunk
  - 当前 offset
  - jump-to-offset
  - near-edge prefetch
- 若后续还要优化大文本预览，也可以复用同一 scroll model。

### 不可直接照搬点

- `trace-ui` 的 `currentRow` / `startIdx` 建立在“按行随机访问廉价”的前提上。
- 本项目的预览数据源不是“行索引文件”，而是：
  - 普通文件
  - 镜像内文件
  - 通过 range/chunk 读取的数据窗口
- 因此不能把本项目直接改造成“全局 row-based 浏览器”；需要保留 byte offset / chunk 模型。

### 建议落地顺序

1. 先保留现有 chunk 模型
2. 把 `HexViewer` 内部滚动控制抽到 hook
3. 再考虑是否让文本预览也复用该 hook

## 3.2 后端数据访问与浏览接口层

### 本项目现状

当前本项目的预览接口名义上支持 range，但底层实现存在两类结构性热点。

#### 热点 A：预览请求需要重新走文件定位链路

每次 range/媒体请求都会从 `file_id` 重新解析到具体 reader：

- `read_file_range_for_case()` -> `read_file_bytes_for_case()`
- `FileRepo::find_by_id`
- `find_data_source_location`
- `open_range_content_for_entry`
- 根据 `logical_directory` / `e01` / `raw` 选择实际 reader

对应证据：

- `crates/app-services/src/file_service/viewer.rs:126-204`

#### 热点 B：多个文件系统 reader 仍然整文件 materialize

文件系统抽象当前只要求 `open_file(&self, path) -> io::Result<Box<dyn Read>>`：

- `crates/evidence-core/src/filesystem/mod.rs:23-27`

这导致多个 reader 的 `open_file()` 实现是：

- NTFS：`read_file_data()` 读完整个 `$DATA` 后 `Cursor::new(data)`
  - `crates/fs-ntfs/src/lib.rs:927-957`
- FAT：`walk_cluster_chain()` 全量读取后 `Cursor::new(data)`
  - `crates/fs-fat/src/lib.rs:478-486`
- exFAT：`read_entry_data()` 全量读取后 `Cursor::new(data)`
  - `crates/fs-exfat/src/lib.rs:265-279`

这意味着：

- 即使前端只请求首个 1MB
- 底层也可能先把 100MB+ 文件完整读进内存

### trace-ui 的做法

`trace-ui` 的浏览层不直接重走原始 trace 文件解析，而是依赖预先准备好的浏览友好表示：

- `TraceEngine::get_lines(session_id, seqs)` 基于已有 handle/state 工作
- 从 `line_index_view()` 在 mmap 上取原始行片段
- 再做结构化解析与 UI 数据填充

对应证据：

- `trace-ui/crates/trace-core/src/engine/browse.rs:6-79`

### 可借鉴点

- 借鉴“浏览接口建立在中间表示上，而不是直接建立在原始文件解析上”。
- 本项目适合引入：
  - `PreviewDescriptor`
  - `RangeReadableFile`
  - `ExtentMap / ClusterMap`
- 让预览浏览路径依赖这些可复用中间表示，而不是每次重跑“entry -> datasource -> filesystem -> open_file”。

### 不可直接照搬点

- `trace-ui` 的 `get_lines()` 针对的是 text trace 的“序号 -> 行片段”访问。
- 本项目处理的是：
  - 普通文件
  - E01/RAW 中的文件系统对象
- 我们不能直接引入“line index + mmap”作为通用文件预览底座。

### 建议落地顺序

1. 先把逻辑目录 reader 升级为真正 `Read + Seek`
2. 再为镜像内文件建立 preview descriptor + extent map
3. 最后再统一成可复用的 range reader 抽象

## 3.3 缓存与索引层

### 本项目现状

本项目已有 `runtime-cache`，但当前预览路径对它的利用不足。

现有能力：

- 泛型 cache entries
- file handles
- `PREVIEW_CHUNKS` namespace 语义预留
- cleanup / clear_case

对应证据：

- `crates/runtime-cache/src/lib.rs:1-64`

但当前预览路径并没有系统化缓存以下高成本结果：

- `file_id -> preview descriptor`
- `file_id -> partition/filesystem resolution`
- `file_id -> extent map / cluster map`

现有最实质的缓存只有 E01 reader chunk table：

- `E01_READER_CACHE`
- `open_e01_reader_cached()`

对应证据：

- `crates/app-services/src/file_service/viewer.rs:13-102`

### trace-ui 的做法

`trace-ui` 的缓存层设计非常系统：

- 对 `file_path` 做 SHA-256 作为缓存主键
- 用文件大小 + 头部 hash 校验缓存有效性
- 将多类高代价中间结果分别缓存：
  - phase2
  - scan
  - lidx
  - strings
  - crypto
- 再次打开时直接 mmap 缓存文件

对应证据：

- `trace-ui/crates/trace-core/src/cache.rs:28-47`
- `trace-ui/crates/trace-core/src/cache.rs:50-77`
- `trace-ui/crates/trace-core/src/cache.rs:126-183`
- `trace-ui/crates/trace-core/src/cache.rs:188-239`

### 可借鉴点

这是最值得借鉴的一层。

可迁移的不是“缓存 trace line index”，而是这套策略：

- 缓存高重建成本的中间表示
- 缓存键与源文件绑定
- 缓存有效性做 size/hash 校验
- 二次打开时直接复用缓存，而不是再次重建

适合本项目缓存的对象：

- `PreviewDescriptor`
- `ResolvedPartitionTarget`
- `ExtentMap / ClusterChainMap`
- 轻量 preview metadata

### 不可直接照搬点

- 不建议把大块预览正文缓存到 SQLite JSON cache
- 不建议照搬成“所有缓存文件都 mmap”
- 对 E01/RAW 内文件，正文不是天然连续文件，缓存策略应偏“描述符/映射”，不是“正文镜像”

### 建议落地顺序

1. 先缓存 preview descriptor
2. 再缓存 extent/cluster map
3. 如仍有热点，再考虑进程内 chunk LRU

## 3.4 会话状态与跨功能复用层

### 本项目现状

本项目的文件预览目前仍是“多 hook 分治”：

- `useFileHandle()`
- `useFileViewer()`
- `useTextPreview()`
- `useImagePreview()`
- `useMediaUrl()`

虽然当前已经开始做按 tab 启用，但这些 hook 之间仍没有统一 preview session：

- handle 不统一持有
- descriptor 不统一复用
- chunk 加载状态只属于 Hex
- text/image/media 没有共享“已解析的预览定位结果”

对应证据：

- `frontend/src/features/files/hooks.ts:119-369`
- `frontend/src/app/pages/FileBrowser.tsx:291-331`

### trace-ui 的做法

`useTraceStore()` 是一个完整的会话层：

- `sessions: Map<string, SessionData>`
- `activeSessionId`
- loading / indexing / error / scroll restore / search state 全部统一管理
- 打开同一文件时优先复用 session

对应证据：

- `trace-ui/src-web/src/hooks/useTraceStore.ts:24-101`
- `trace-ui/src-web/src/hooks/useTraceStore.ts:159-224`
- `trace-ui/src-web/src/hooks/useTraceStore.ts:226-303`

### 可借鉴点

- 借鉴“一个文件打开后形成会话”的工程模型。
- 本项目不需要照搬成复杂 trace 多 session store，但可以收敛出：
  - `PreviewSession`
  - `PreviewDescriptor`
  - `PreviewState`
- 让 Hex / Text / Media 共用：
  - handle
  - mime
  - size
  - 文件定位结果
  - 失败状态

### 不可直接照搬点

- `trace-ui` 会话模型服务于：
  - index build
  - search
  - taint
  - multiple trace tabs
- 本项目预览会话不需要扩展到那么大
- 否则会把简单的文件预览问题升级成重量级 session 系统重构

### 建议落地顺序

1. 先做单文件 `PreviewSession`
2. 再决定是否提升到 case 级多预览会话管理

## 4. 综合结论

## 4.1 最值得借鉴的不是 mmap，而是结构

`trace-ui` 最值得借鉴的是以下结构模式：

- 按需激活
- 统一滚动控制层
- 统一会话状态层
- 缓存高代价中间表示
- 常量内存目标

这些都比“直接上 mmap”更适合本项目当前阶段。

## 4.2 `trace-ui` 的 mmap/line-index 不能直接照搬到本项目镜像预览

原因是两类数据模型不同：

- `trace-ui`：顺序文本 trace，天然适合 `line index + mmap`
- 本项目：普通文件 + E01/RAW 内文件系统对象，尤其镜像内文件不是天然连续物理文件

所以：

- 逻辑目录大文件可单独评估 mmap
- 但 E01/RAW 内文件预览更适合：
  - `PreviewDescriptor`
  - `ExtentMap`
  - `Read + Seek` 或 chunk-based reader

## 4.3 本项目当前最大的性能债不在 UI，而在底层 reader 契约

真正的结构性问题是：

- `FileSystemReader::open_file()` 只返回 `Box<dyn Read>`
- 多个 fs reader 在 `open_file()` 内整文件 materialize

这会让上层 range 语义失真。

只做前端懒加载虽然必要，但不足以从根上解决 100MB+ 预览延迟。

## 5. 建议落地顺序

### Phase A：低风险止血

1. 前端按 tab 启用预览请求
2. 逻辑目录大文件范围读取改为真实 seek
3. media protocol 中后段 range 消除线性 discard

### Phase B：结构性优化

1. 引入 `PreviewDescriptor`
2. 缓存 partition/filesystem resolution
3. 为镜像内文件建立 extent/cluster map

### Phase C：高级优化

1. 统一 preview session
2. 抽象 preview scroll model
3. 只对逻辑目录大文件评估 mmap

## 6. 明确的可借鉴项清单

### 可直接借鉴

- 按需激活（only load active view）
- 中间表示缓存
- 文件级缓存有效性校验（size/hash）
- 统一滚动状态 hook
- 统一 preview session 思想

### 条件借鉴

- 自定义滚轮节流
- 自定义 gutter/minimap
- 本地持久化滚动位置

### 不建议直接借鉴

- 将镜像内文件预览统一改造成 mmap
- 引入 trace 风格 line-index 作为文件预览主模型
- 缓存大块正文而不是缓存描述符/索引

## 7. 后续建议

- 在 `docs/optimization-recommendations.md` 和未来的大文件预览优化方案中，引用本报告作为“外部设计对照依据”。
- 若后续需要正式实施，可先从逻辑目录与媒体协议的 `Read + Seek` 基础设施改造开始，再扩展到 E01/RAW。
- 如果要持续对照外部项目，建议优先跟踪 `trace-ui` 的：
  - 缓存文件格式演进
  - session 状态组织
  - 虚拟滚动性能治理  
  而不是其 trace 专属分析算法。
