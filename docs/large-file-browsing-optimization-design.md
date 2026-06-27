# 大文件浏览优化设计文档

## 1. 目标

本文档定义本项目针对 100MB 以上文件浏览与预览的优化方案，覆盖：

- Hex 预览
- 文本预览
- 图片预览
- 音视频预览
- `evidence-media://` 协议 range 读取

目标不是扩大支持格式，而是在现有 Windows-first 取证边界内，降低大文件首屏延迟、跳转延迟和无效并发开销，并保持证据只读与前后端契约稳定。

## 2. 当前实现基线

### 2.1 前端预览激活模型

当前前端浏览入口位于：

- `frontend/src/app/pages/FileBrowser.tsx`
- `frontend/src/app/pages/FilePreviewPanel.tsx`
- `frontend/src/features/files/hooks.ts`

当前实现已经开始按 `viewerTab` 收敛预览请求：

- `hex` tab 仅启用 `useFileViewer`
- `text` tab 仅启用 `useTextPreview`
- `preview` tab 按扩展名或 MIME 分流到 `useImagePreview` / `useMediaUrl`
- `metadata` tab 可单独启用轻量 `useFileHandle`

对应证据：

- `frontend/src/app/pages/FileBrowser.tsx:291-331`
- `frontend/src/features/files/hooks.ts:119-369`

这一步已经解决了“选中文件即并发拉 4 条预览链路”的前端浪费问题，但还没有从根上解决大文件读取热点。

### 2.2 后端 range 接口现状

当前 Tauri 命令入口：

- `apps/desktop/src-tauri/src/commands/file_commands.rs`
- `apps/desktop/src-tauri/src/media_protocol.rs`

当前应用服务底座：

- `crates/app-services/src/file_service/viewer.rs`

当前逻辑目录文件已具备 seekable 优化：

- `read_file_bytes_for_case()` / `read_file_bytes_for_entry()` 拆出了纯 bytes range 读取底座
- `logical_directory` 分支走 `std::fs::File + seek`
- `media_range_for_file()` 与 `evidence-media` 协议已改成复用纯 bytes helper，而不是走 Hex DTO 格式化路径

对应证据：

- `crates/app-services/src/file_service/viewer.rs:165-219`
- `crates/app-services/src/file_service/viewer.rs:284-369`
- `apps/desktop/src-tauri/src/commands/file_commands.rs:498-554`
- `apps/desktop/src-tauri/src/media_protocol.rs:247-355`

### 2.3 当前仍存在的结构性瓶颈

文件系统抽象层当前仍然要求：

- `FileSystemReader::open_file(&self, path) -> io::Result<Box<dyn Read>>`

对应证据：

- `crates/evidence-core/src/filesystem/mod.rs:23-27`

这导致多个文件系统 reader 仍然会在 `open_file()` 内整文件 materialize：

- NTFS：`read_file_data()` -> `Cursor::new(data)`
  - `crates/fs-ntfs/src/lib.rs:927-957`
- FAT：`walk_cluster_chain()` -> `Cursor::new(data)`
  - `crates/fs-fat/src/lib.rs:478-486`
- exFAT：`read_entry_data()` -> `Cursor::new(data)`
  - `crates/fs-exfat/src/lib.rs:265-279`

因此，即使上层只请求首个 1MB range，镜像内文件仍可能先整文件读入内存。

### 2.4 当前缓存能力

当前已有 `runtime-cache`：

- 泛型 `cache_entries`
- `file_handles`
- `PREVIEW_CHUNKS` namespace 预留

对应证据：

- `crates/runtime-cache/src/lib.rs:1-64`

但预览链路当前没有系统化缓存以下高成本结果：

- `file_id -> preview descriptor`
- `file_id -> partition/filesystem resolution`
- `file_id -> extent map / cluster chain`

现有最主要的预览缓存仅是 E01 reader chunk table：

- `E01_READER_CACHE`
- `open_e01_reader_cached()`

对应证据：

- `crates/app-services/src/file_service/viewer.rs:13-102`

## 3. 设计目标

## 3.1 性能目标

- 选中 100MB+ 文件时，只激活当前 tab 所需预览链路
- 逻辑目录文件的非零 offset 预览必须是 O(1) seek + O(length) read
- `evidence-media://` 中后段 range 请求不得再随 offset 线性恶化
- 镜像内文件的预览逐步收敛为“只读取所需 chunk”，不再整文件 materialize
- 前端内存占用与当前可见窗口/chunk 数量相关，而不是与文件总大小线性相关

## 3.2 约束目标

- 不修改公开 IPC 命令名
- 不修改已存在 DTO 字段形状
- 不引入运行时 mock / fallback 数据分支
- 不写入证据源，不落明文临时副本
- 保持 Windows-first；Linux/macOS 相关文件系统优化延后

## 4. 目标架构

## 4.1 PreviewSession

引入单文件级 `PreviewSession` 概念，用于统一当前文件的浏览状态。首版不需要做成全局复杂 session store，但至少在单文件维度统一：

- `file_id`
- `handle_id`
- `mime`
- `size`
- `preview_kind`
- `active_offset`
- `loaded_ranges`
- `last_error`

前端继续通过 hooks 消费，但避免 Hex/Text/Image/Media 各自独立重复初始化。

## 4.2 PreviewDescriptor

在后端引入 `PreviewDescriptor` 中间表示，封装“浏览该文件所需的高成本解析结果”：

- `case_id`
- `file_id`
- `source_kind`
- `source_path`
- `partition_index`
- `filesystem_kind`
- `catalog_path`
- `mime`
- `size`
- 若可得，则附带 `extent_map` / `cluster_chain`

`PreviewDescriptor` 不直接暴露给前端，属于应用服务层与 runtime-cache 内部结构。

## 4.3 Seekable Preview Reader

预览底座统一收敛到“可随机访问的预览 reader”能力：

- 逻辑目录：`std::fs::File` 直接 seek
- E01/RAW 内文件：基于 extent / cluster map 的 range reader

设计原则：

- `open_file_content_by_id()` 继续保留给旧调用方
- 预览路径新增专用入口，例如：
  - `open_preview_reader_by_id()`
  - `resolve_preview_descriptor()`
  - `read_preview_bytes(...)`

预览路径不再依赖整文件 `Vec<u8>` materialize。

## 4.4 Extent / Cluster Map

镜像内文件随机访问的核心是为文件建立逻辑偏移到物理片段的映射。

首批支持：

- NTFS：基于 `$DATA` non-resident data runs
- FAT：基于 cluster chain
- exFAT：基于 cluster chain / no-fat-chain 模式

首版目标不是修改所有 fs crate 的公开 trait，而是先在预览路径旁路建立“预览专用随机访问能力”。

## 4.5 预览缓存策略

缓存分两层：

### 持久缓存（runtime-cache SQLite）

缓存高代价中间结果，而不是大块正文：

- preview descriptor
- partition/filesystem resolution
- extent/cluster map

### 进程内缓存（可选）

若后续需要降低重复 chunk 读取，可增加短生命周期 LRU：

- key: `(case_id, file_id, chunk_index)`
- value: `Vec<u8>` 或压缩 bytes
- 关闭案件时清空

本轮不要求实现正文 chunk 落盘缓存。

## 5. 分阶段落地方案

## Phase A — 已落地/低风险止血

1. 前端按 tab 启用预览请求
2. 逻辑目录大文件 range 走真实 seek
3. 媒体协议中后段 range 不再经过线性 discard

对应当前代码状态：

- `frontend/src/app/pages/FileBrowser.tsx`
- `frontend/src/features/files/hooks.ts`
- `crates/app-services/src/file_service/viewer.rs`
- `apps/desktop/src-tauri/src/commands/file_commands.rs`
- `apps/desktop/src-tauri/src/media_protocol.rs`

## Phase B — 结构性优化

1. 引入 `PreviewDescriptor`
2. 缓存 `file_id -> descriptor`
3. 缓存 partition/filesystem resolution
4. 为镜像内文件建立 extent/cluster map
5. 让 Hex/Text/Media 共用 descriptor，而不是各自重复定位

## Phase C — 预览随机访问统一化

1. 为 NTFS/FAT/exFAT 预览路径新增 seekable reader
2. 将 `read_file_range` / `get_text_preview` / `read_media_range` 全部切到统一 preview reader
3. 将镜像内文件从“整文件 materialize fallback”推进到“真实 chunk 读取”

## Phase D — 高级优化

1. 前端统一 preview scroll model
2. 仅对逻辑目录超大文本/二进制评估 mmap
3. 根据测量结果决定是否增加进程内 chunk LRU

## 6. 测试矩阵

## 6.1 前端

- `hex` tab 不触发 text/image/media 请求
- `text` tab 不触发 hex/media 请求
- `preview` tab 只触发匹配的 image 或 media 请求
- Hex offset 跳转和滚动加载行为正确

## 6.2 逻辑目录文件

- 100MB 文件首屏仅读取必要窗口
- 非零 offset range 不调用线性 discard
- 文本预览只读取前 `DEFAULT_TEXT_PREVIEW_MAX_BYTES`

## 6.3 媒体协议

- `bytes=80MB-80MB+N` 的 protocol range 可正常返回
- 耗时不再随 offset 线性恶化
- host path 不泄露

## 6.4 镜像内文件

- NTFS/FAT/exFAT 大文件首屏只读取首块
- 跳转到中后段时仅读取目标 chunk
- descriptor / extent map 可复用

## 6.5 回归

- 小文件 Hex/Text/Image/Media 预览不退化
- IPC DTO 与命令名不变
- 关闭案件时 preview 相关 cache 被清理

## 7. 验收标准

- 选中 100MB+ 文件时，不再出现多条无关预览链路并发请求
- 逻辑目录大文件的 Hex/Text/Media 首屏与中段跳转明显提速
- `evidence-media` 协议对大 offset range 的响应明显提速
- 镜像内文件浏览路径进入 descriptor + chunk 化改造后，不再依赖整文件 materialize
- 前后端真实数据链路保持不变，不引入 mock

## 8. 已知风险与非目标

### 已知风险

- 现有 fs reader trait 只暴露 `Read`，镜像内文件 seekable reader 改造会触及多个 fs crate
- E01/RAW 随机访问优化需要谨慎处理 extent 映射正确性与边界错误
- 前端若未来统一 preview session，需避免状态机复杂度失控

### 非目标

- 本轮不扩展新的预览格式支持
- 本轮不承诺 Linux/macOS 文件系统同时完成同等级优化
- 本轮不把所有大文件预览统一改成 mmap
- 本轮不做预览内容编辑、搜索字节序列、书签等高级交互

## 9. 与 `trace-ui` 对比报告的关系

本设计文档与 `docs/trace-ui-comparative-analysis.md` 的关系如下：

- 对比报告回答“哪些结构思路可借鉴、哪些不能直接照搬”
- 本设计文档回答“本项目具体怎么落地、按什么顺序落地、验证什么结果”

引用规则：

- 若需要论证为何不直接照搬 `mmap + line index`，引用对比报告
- 若需要实施任务拆分、测试矩阵与验收，引用本文档
