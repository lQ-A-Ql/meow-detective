# 大文件浏览与提取架构

## 1. 目标与边界

本文档描述大文件预览与证据文件提取的当前实现、性能边界和后续约束。目标是在不修改原始证据、不暴露宿主路径、且不改变既有 Tauri IPC 契约的前提下，使内存占用与请求窗口而非文件总大小相关。

覆盖范围：

- Hex、文本、图片、音视频预览的按需读取；
- `evidence-media://` 的 byte-range 请求；
- E01、RAW、逻辑目录、BitLocker NTFS、派生 RBD 与已物化 CephFS 文件的读取路由；
- 单文件导出时的完整性校验、原子发布和受限并行读取。

这不是编辑器或内容搜索设计。预览与提取始终只读证据源，输出只可写入明确校验过的目标路径。

## 2. 当前读取模型

### 2.1 前端请求编排

文件浏览器按当前 viewer tab 激活真实请求：

- `hex` 仅启用 range Hex 读取；
- `text` 仅启用文本预览；
- `preview` 根据 MIME/扩展名启用图片或媒体协议；
- `metadata` 仅请求文件句柄与元数据。

前端不为同一选中文件并行启动所有预览类型，不持有文件系统定位、extent、分区或路径推导逻辑。Hex 对小文件全量读取；大文件按最大 1 MiB range 分段加载并支持 offset 跳转。

### 2.2 后端描述符与缓存

`file_service::viewer::descriptor` 在每次预览或提取前构造 `PreviewDescriptor`，其中包含数据源种类、已选分区候选、文件系统种类、目录项路径候选、文件大小和可选 CephFS locator。

描述符使用 runtime cache 保存，但每次命中都校验文件大小、修改时间、分区路由和可读性；任一不匹配、反序列化失败或源文件消失都会丢弃缓存并重新解析。描述符只保存解析元数据，不缓存或落盘证据正文。

### 2.3 统一 range reader

`evidence_core::FileSystemReader` 当前提供三层能力：

- `open_file`：顺序读取兼容入口；
- `read_file_range(path, offset, length)`：可由文件系统实现的有界随机读取；
- `open_file_seekable`：返回 `Read + Seek`，不支持时显式返回 `Unsupported`。

`file_service::viewer::io` 优先使用 seekable reader；若文件系统不能提供 seekable stream，则回退顺序 reader，并仅在该回退场景按字节跳过 offset。大 offset 顺序跳过会产生日志警告，不能被误认为随机访问优化。

预览 range 的路由顺序为：

1. 从 source-local catalog 解析全局文件 ID 并加载校验后的描述符；
2. 对逻辑目录直接使用受限的宿主文件 seek；
3. 对 E01/RAW 依分区和文件系统调用专用 range reader；
4. 对派生 RBD/CephFS 使用已验证 locator 和 source-local runtime；
5. 仅在没有 range/seek 能力时使用顺序 reader fallback，且保持长度上限。

所有 range 请求仍受统一最大长度限制，不能通过 offset 或 length 取得宿主原始路径。

## 3. 已实现的文件系统路径

| 数据源或文件系统 | 当前路径 | 读取特性 | 说明 |
|---|---|---|---|
| 逻辑目录 | `std::fs::File` + seek | 真正随机读取 | 仅在经过 source root containment 校验后使用 |
| E01/RAW NTFS | `range_fs::ntfs` | `read_file_range`、seekable NTFS data-run stream | 可用于预览和提取 |
| E01/RAW FAT | `range_fs::fat` | `read_file_range` | 按 cluster chain 有界读取 |
| E01/RAW exFAT | `range_fs::exfat` | `read_file_range` | 支持文件系统标识探测后的路径读取 |
| E01/RAW EXT4/XFS/BTRFS | `range_fs::linux` | 通过对应 reader 的 `read_file_range` 或 seekable stream | 可包含经 LVM 转译后的逻辑卷 |
| BitLocker NTFS | `range_fs::bitlocker` / `source_read::bitlocker` | 解锁后的 NTFS range/seek | 仅适用于已验证、持久化的解锁密钥包 |
| 派生 RBD / CephFS | `source_read::derived_cache` | source-local runtime 缓存 + range 读取 | 依赖已发布 locator；不扩大为任意 CephFS 支持 |

E01 打开使用共享 chunk table 缓存，但每个 reader 保持独立读取状态。LVM 映射只作为候选 block reader 的偏移转译层，不向前端暴露物理路径或逻辑卷内部结构。

## 4. 大文件导出

### 4.1 默认路径

提取从 `SourceReadContext::extraction_plan_by_id` 创建计划：优先选择专用 range/seek stream，目标端由 `extraction::policy` 先校验绝对路径、case scope、证据源重叠、符号链接和 Windows ADS。复制写入同目录临时文件，完成后同步并原子发布；默认不覆盖已有目标。

复制期间计算 SHA-256，长度、读取错误、取消与 worker panic 都会失败关闭。进度经 `file-extract-progress` 事件发送，阶段为 `preparing`、`copying`、`finalizing` 及终态；终态和阶段切换不受普通节流丢弃。

### 4.2 受限并行路径

仅当同时满足下列条件时，E01/RAW 中的 NTFS 文件使用并行读取：

- 文件大小至少 `512 MiB`；
- 可获得至少两个 CPU；
- 当前进程 RSS 距软内存上限仍保留至少 `128 MiB`；
- 可为该文件重新打开两个独立、可 seek 的 NTFS data-run stream；
- 数据源不是不支持该模型的种类或文件系统。

当前参数固定为两个 reader、每块 `4 MiB`、最多 `4` 个在途块。worker 只读取目标块；协调器按序写入单个临时文件，因此输出顺序、hash 和原子发布语义保持确定性。任一条件不满足即回退到单 reader 顺序复制，而不是强行并行抢占内存或 E01 I/O。

## 5. 分析调度关系

文件预览、文件提取和 artifact 提取使用不同的并发边界：

- 预览是短生命周期、按 range 请求的读路径；
- 文件提取可为单个满足条件的 NTFS 大文件启用两个读 worker；
- artifact 提取采用有界有序调度，worker 数由运行时 CPU 与 RSS 预算解析，默认最多 `256 MiB` 在途数据；达到软内存上限时降为 `128 MiB`；
- artifact 结果仍由协调器有序持久化，SQLite connection 不跨 worker 共享。

因此不能把预览、导出和分析的并发指标相互替代。性能评估必须分别记录首屏/跳转延迟、导出吞吐和分析吞吐/峰值 RSS。

## 6. 验证要求

### 6.1 预览

- 大文件 Hex 在 1 MiB range 上限内加载，滚动和 offset 跳转不触发整文件读取；
- 文本、图片和媒体只触发当前 tab 所需链路；
- 非零 offset 的 seekable 路径不执行线性 discard；
- 不支持 seek/range 的 reader 明确走顺序 fallback，并保持长度限制；
- source DB、分区、路径和 descriptor 缓存均不会跨数据源复用。

### 6.2 提取

- 输出字节数、SHA-256 和目标文件内容与顺序基线路径一致；
- 已存在目标、符号链接、ADS、case workspace 或证据源重叠目标被拒绝；
- 取消、读失败或 worker panic 不发布部分目标；
- 并行条件不足时自动走串行，不提升内存峰值；
- 真实样本只由 ignored test 或本机/CI artifact 验证，不把路径、hash 或性能结果提交到技术文档。

## 7. 剩余边界

- `FileSystemReader` 的 range/seek 是可选能力，并非每个文件系统实现都保证相同复杂度；
- 顺序 fallback 在超大 offset 上仍可能慢，必须通过指标识别，不可伪装为优化完成；
- 两 reader 并行导出是 NTFS E01/RAW 的受限优化，不适用于所有文件系统、压缩属性或派生数据源；
- 当前不引入 mmap、前端全文字节搜索、编辑、书签或证据正文持久缓存；
- CephFS 仅在已有、验证过的 source-local locator 下提供读取基础，不声明真实集群的通用 CephFS 文件树能力。

## 8. 关联文档

- `docs/export-and-media-safety.md`：输出路径、覆盖与协议安全边界；
- `docs/ceph-rbd-vm-preview-performance-design.md`：派生 VM/RBD 预览的专门性能约束；
- `docs/trace-ui-comparative-analysis.md`：可借鉴的可视化与数据窗口思想，不能替代本项目证据访问模型。
