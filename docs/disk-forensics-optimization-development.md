# Meow~Detective 磁盘取证优化开发文档

> 文档版本：1.0
> 更新日期：2026-10-08
> 依据：磁盘取证能力评估与优化建议报告、当前仓库实现、现行架构与安全约束

## 1. 文档目的

本文档把附件中的评估结论转换为可执行的开发规范，供需求拆分、架构评审、实现、测试和发布验收使用。

目标范围限定为磁盘、文件系统和主机落盘制品取证：

- E01/EWF、RAW/dd/img、逻辑目录和受支持的虚拟磁盘；
- 分区、卷、NTFS/FAT/exFAT/ext4/XFS/Btrfs/LVM；
- Windows/Linux 落盘制品、删除恢复、文件元数据和时间线；
- 搜索、证据关联、嵌套证据和调查工作台。

内存、PCAP、云和移动设备不属于本阶段目标。新功能必须复用现有只读 Evidence Reader，不得通过宿主路径直接读取证据。

## 2. 当前分析结论

项目已有较完整的基础解析能力：NTFS/FAT/exFAT/ext4/XFS/Btrfs/LVM、E01/RAW、Windows Registry、Prefetch/LNK/JumpList/SRU/Thumbcache/Recycle Bin、浏览器、Linux 日志与服务、BitLocker、PVE/Ceph、Timeline 和全文搜索均已存在。

当前主要缺口是证据粒度和调查链路：

1. 哈希结果没有严格区分容器、逻辑磁盘、分区和文件内容。
2. NTFS 解析丢弃部分 DataRun 原始字段，缺少技术检查视图。
3. 全文索引白名单小于预览文本范围，容易漏掉服务器配置和脚本。
4. 图片、systemd、Git/GitLab、Firefox 扩展缺少结构化结果。
5. VHD/Nested Evidence 和 Registry Browser 尚未形成通用调查链。
6. 前端部分页面曾把数据请求、业务编排和 JSX 放在同一文件；UI 原语、文案、图标和动效需要集中治理。

因此优先级按“证据语义准确性 → 技术结构可见性 → 高频落盘证据 → 复杂嵌套证据”推进。

## 3. 统一架构约束

### 3.1 后端

- 一个生产文件只负责一个具体用例或功能；公共纯函数放共享模块。
- DTO 只定义于 `crates/transport/src/dto/`，使用 `camelCase` 序列化，Rust 类型以 `Dto` 结尾。
- Tauri 命令只做请求校验、调用应用服务和 DTO 返回，不写业务 SQL、文件解析或证据读取。
- 应用服务负责用例编排；Parser/Repository/Core crate 不依赖 Tauri 或前端。
- 原始证据和源数据库只读打开；派生数据只写案件工作区或用户明确的导出目录。
- 证据结果必须带 `source_object_id`、`source_attribution`、解析器 ID/版本和必要的 provenance。
- 错误使用 typed errors，跨 IPC 使用 `ApiErrorDto` 和统一错误分类。

### 3.2 前端

- UI/UX 渲染组件只接收 props 并渲染；请求、状态、校验、异步副作用放 hooks/controllers/model。
- 原生 `button/input/select/textarea/dialog` 只允许出现在 `frontend/src/app/components/ui/` 原语实现中。
- 文案进入 `frontend/src/i18n/`；颜色进入 `frontend/src/styles/theme.css`；图标进入集中映射；图片进入 `frontend/src/assets/`；动效进入共享 motion token/CSS。
- 页面组件只组合 feature container 和 view，不直接调用 `invoke` 或持有业务请求。
- 测试只放 `frontend/tests/`，不得在 `frontend/src` 放 `.test.*`/`.spec.*`。

### 3.3 性能与安全

- 大文件和大目录使用范围读取、分页、游标或流式接口，禁止无界载入。
- 所有 offset、length、页大小、解压输出和缓存均有上限。
- 搜索、预览和恢复结果必须报告跳过原因、截断状态或不支持原因。
- 不把候选密码、原始凭据或宿主绝对路径返回前端或审计正文。

## 4. 总体数据流

```text
证据源
  ↓ 只读 EvidenceReader
镜像/分区/文件系统层
  ↓ source.db 文件对象与原始结构
应用服务用例
  ↓ transport DTO + provenance
前端 Controller/Hook
  ↓ props
纯渲染 View / 统一 UI 原语
```

所有优化都必须沿此链路实现。MCP、文件预览、导出和 UI Inspector 不得另开宿主文件读取旁路。

## 5. Stage 1：证据语义与通用调查能力（P0）

### 5.1 Hash Scope 与多算法

#### 目标

支持 MD5、SHA1、SHA256、SM3，并明确以下范围：

```text
ContainerFile
ContainerSet
LogicalDisk
Partition
File
DerivedEvidence
```

#### 数据模型

```rust
EvidenceDigest {
    scope: DigestScope,
    algorithm: DigestAlgorithm,
    value: String,
    byte_length: u64,
}
```

E01 分段文件的容器集合哈希不能冒充逻辑磁盘哈希。每个结果必须说明输入范围、算法、字节数和计算状态。

#### 后端拆分

- `transport::dto::digest.rs`：DTO、scope、algorithm。
- `infrastructure/hash`：流式算法实现和固定缓冲区。
- `app-services/hash_service/container.rs`：容器文件/分段集合。
- `app-services/hash_service/logical_disk.rs`：解压后的逻辑字节流。
- `app-services/hash_service/partition.rs`：分区范围。
- `app-services/hash_service/file.rs`：文件内容范围。
- Repository 保存 digest 结果和计算状态，不保存原始密钥或临时明文。

#### UI

Evidence Inspector 的 `Hash` Tab 使用统一 `HashInspector`：

| Scope | Algorithm | Digest | Bytes | 状态 |
|---|---|---|---:|---|
| Container Set | SHA256 | ... | ... | 已验证 |
| Logical Disk | SHA1 | ... | ... | 计算中/完成 |
| Partition 1 | MD5 | ... | ... | 完成 |
| File | SHA256 | ... | ... | 完成 |

#### 验收

- 同一范围同一算法重复结果一致。
- 容器集合、逻辑磁盘、分区和文件结果互不混淆。
- 大文件使用流式读取，内存不随文件大小线性增长。
- 原证据文件大小、修改时间和哈希保持不变。
- 中止、短读、读取错误返回 typed error，并保留失败范围。

### 5.2 NTFS Technical Inspector

#### 目标

把已有 NTFS 解析结果提升为可审计的底层结构视图：MFT、属性、DataRun、原始 Hex、物理偏移和逻辑偏移。

#### 数据结构

普通读取不携带原始字节；技术视图按需加载 `ForensicDataRun`：

```rust
ForensicDataRun {
    header: u8,
    length_field_size: u8,
    offset_field_size: u8,
    cluster_count: u64,
    relative_lcn: Option<i64>,
    absolute_lcn: Option<i64>,
    raw: Vec<u8>,
    physical_offset: Option<u64>,
}
```

`fs-ntfs` 保持唯一 DataRun 解析实现；app-services 只负责用例和 DTO 转换。

#### UI 结构

Evidence Inspector → `Forensics` → `NTFS`：

- MFT record number、sequence、flags、parent reference、record offset；
- `$STANDARD_INFORMATION`、`$FILE_NAME`、`$DATA`、`$ATTRIBUTE_LIST`；
- resident/non-resident、allocated/real/initialized size；
- DataRun 表：Raw、Header、Length Bytes、Offset Bytes、Run Length、Relative/Absolute LCN；
- “跳转 Hex”把结构 offset 传给 Hex Viewer 并高亮范围；
- 删除恢复显示 parent path 重建状态、allocation、sequence 校验和恢复置信度。

#### 验收

- 已知 DataRun 原始字节能还原所有字段。
- MFT、DataRun、物理范围与 Hex 视图相互跳转。
- 删除记录 sequence 变化或源数据损坏时失败关闭。
- 技术视图不改变普通文件浏览性能。

### 5.3 全文索引策略

#### 第一阶段扩展

新增扩展名：

```text
sql conf cfg ini yaml yml toml properties rb py sh ps1 bat cmd
```

#### 第二阶段 Content Sniffing

```text
读取前 4–64 KiB
  ↓
NUL 比例检测
  ↓
ASCII / UTF-8 / UTF-16 检测
  ↓
扩展名作为 hint
  ↓
进入索引或记录跳过原因
```

索引状态必须区分 `indexed`、`binary`、`oversized`、`unreadable`、`unsupported_encoding`。

#### 验收

- `.sql`、`gitlab.rb`、`.conf`、`.yaml` 可检索。
- 文本预览和全文索引使用同一文本判断策略。
- 搜索结果显示命中来源：Filename、Path、Indexed Content 或 Artifact Field。
- 页面显示覆盖率和跳过统计。

## 6. Stage 2：高频落盘证据（P1）

### 6.1 图片 EXIF/媒体元数据

新增 `ImageMetadata`：

```text
Format, Width, Height, Orientation, Make, Model, Software,
DateTimeOriginal, CreateDate, ModifyDate, LensModel,
Latitude, Longitude, Altitude, GPSDateTime
```

解析器必须支持普通文件和删除恢复输出。无 EXIF 时返回明确的 `absent`，不能制造空的成功结果。

UI 在 Inspector 的 `Forensics → Image` 显示表格，底部保留图片预览；字段可跳回文件来源和恢复记录。

### 6.2 Structured systemd

抽出共享 `systemd-analysis`，同时供 Linux 预检和取证分析。

```rust
LinuxServiceUnit {
    name, description, state, enabled, masked, static,
    unit_file, exec_start, exec_stop, user, group,
    working_directory, restart, wanted_by, required_by,
    enablement_symlink,
}
```

UI 提供 Enabled、Disabled、Masked、Static 统计和过滤，详情显示 `ExecStart`、enablement symlink 与源文件定位。

### 6.3 Git/GitLab

#### GitRepository

支持 `.git/config`、`HEAD`、refs、packed-refs、logs 和 commit 对象：

```text
repository path, current branch, remote name/url,
ref, commit hash, author, committer, author time,
committer time, message
```

#### GitLabArtifact

覆盖 `/etc/gitlab/` 和 `/var/log/gitlab/`，提取：

```text
external_url, listen port, repository path,
user, IP, action, status, project, timestamp
```

日志读取支持 `*.log.N.gz`，透明解压但设置单文件输出上限。所有记录带 source object 和行号/偏移，可回到证据。

### 6.4 Firefox Extensions

新增：

```text
FirefoxExtension {
    id, name, version, active, userDisabled,
    installDate, updateDate, signedState,
    permissions, sourceProfile
}
```

结果进入浏览器制品集合，未来与 Chrome/Edge 统一为 `BrowserExtension`。

## 7. Stage 3：复杂磁盘证据（P1/P2）

### 7.1 VHD/Nested Evidence

第一阶段支持 Fixed/Dynamic VHD；VHDX、QCOW/QCOW2 继续明确不支持。

```text
Parent Data Source
  → Nested File
  → Derived Data Source
  → Partition
  → File System
```

右键文件提供“作为嵌套证据打开”。派生源必须保存 parent source ID、file ID、offset、length、探测结果和 provenance。不得将未知容器猜测成 RAW。

### 7.2 Registry Browser

在现有定向 Registry extractor 之外增加只读浏览器：

- Hive Tree、Key、Subkey、Value、Type、Raw/Decoded Data；
- Last Write Time、Cell Offset、Hex View；
- Key、Value Name、Value Data 搜索；
- LOG1/LOG2 overlay 来源标记：Base、Recovered、Merged。

UI 复用三栏 Evidence Workbench，不使用新的大弹窗，不复制 Registry 解析逻辑。

## 8. 统一 Evidence Workbench UI

保留现有导航：Overview、Files、Artifacts、Analysis、Timeline、Search、Reports。新能力采用 Evidence Object → Contextual Capability，不新增大量一级页面。

```text
顶部：Case / Data Source / Partition / Path / Search
左侧：Evidence Tree
中部：File/Object List
底部：Preview / Text / Hex / Structure / Metadata
右侧：Evidence Inspector
```

### Inspector Tabs

1. `Overview`：文件名、路径、大小、时间、属性、权限。
2. `Forensics`：按类型显示 NTFS、EXIF、LNK、Registry、VHD、Git。
3. `Hash`：scope、algorithm、digest、byte length、状态。
4. `Relations`：Artifact、Timeline、Parent、Derived、Recovered、Related Path。

### 通用 Context Menu

```text
Open / Preview / Go to Hex / View Forensic Structure /
View Timeline / Calculate Hashes / Extract /
Open Nested / Copy Path / Copy File ID
```

按类型扩展：View MFT/DataRuns、View EXIF、Open Nested、View Git History。

## 9. IPC、服务和错误契约

每个新增能力遵循：

```text
transport DTO
  → app-services use case
  → Tauri command validate → service → DTO
  → frontend apiClient
  → feature hook/controller
  → pure View
```

错误分类：

| 场景 | 分类 |
|---|---|
| 参数/范围/游标无效 | validation |
| 格式不支持 | unsupported |
| 证据短读/数据库失败 | io |
| 结构损坏/校验失败 | parser |
| 权限或跨源访问 | security |
| 用户取消 | cancelled |
| 内部未预期状态 | internal |

所有查询结果限制大小；所有写入使用临时文件和原子替换；原始证据永不修改。

## 10. 测试与验收矩阵

### 单元测试

- Hash：每个 scope、算法、空输入、短读、取消。
- DataRun：合法、稀疏、负偏移、字段溢出、截断。
- Text sniffing：UTF-8、UTF-16、NUL、扩展名冲突、超大文件。
- EXIF：完整、缺失、损坏、恢复文件。
- systemd：enabled/disabled/masked/static、语义字段缺失。
- Git/GitLab：HEAD、remote、packed refs、损坏对象、gzip 日志。
- VHD：fixed/dynamic、边界、截断、溢出、父源关联。
- Registry：Hive、LOG overlay、搜索和 Hex 范围。

### 集成测试

- 数据源隔离和 provenance 传递。
- 预览/搜索/导出共用同一 Evidence Reader。
- Inspector Tab 和 Hex 跳转。
- MCP 只读文件查询和审计。

### 真实样本测试

真实样本测试必须 `#[ignore]`，只通过环境变量传入路径，不允许工作站 fallback，不提交样本。

### 前端验收

- View 文件无 API、业务 hook、剪贴板和异步副作用。
- 非 UI 原语目录没有原生控件。
- 所有界面文案有 zh-CN/en 资源。
- 测试位于 `frontend/tests`。
- 键盘焦点、Escape、屏幕阅读器语义由统一原语提供。

## 11. 性能边界

- 目录分页默认 100，单页最大 200。
- MCP 文件读取单块最大 64 KiB。
- Hash、全文索引和 EXIF 读取均使用流式或受限范围。
- 压缩/解压输出必须有硬上限；禁止根据证据声明值无界分配。
- UI 大列表使用 VirtualList，Inspector 只在选中对象时加载技术结构。
- 搜索索引不因二进制或超大文本阻塞导入主流程。

## 12. 交付顺序与发布门槛

### Stage 1

Hash Scope、多算法、NTFS Inspector、全文索引扩展。

门槛：证据范围语义通过、DataRun 原始值可复核、配置文件可搜索、全量只读回归通过。

### Stage 2

EXIF、Structured systemd、Git/GitLab、Firefox Extensions。

门槛：结构化字段可回源、gzip 日志有上限、缺失字段不伪造成功。

### Stage 3

VHD/Nested Evidence、Registry Browser。

门槛：派生源 provenance 完整、嵌套容器 fail-closed、Hive overlay 来源可见。

发布前统一运行：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
powershell -ExecutionPolicy Bypass -File scripts/check-frontend-architecture.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-media-protocol-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-doc-archive.ps1
```

## 13. Definition of Done

一个优化项只有同时满足以下条件才算完成：

1. 有 transport DTO、服务用例、必要 Repository 和前端 API。
2. 生产文件按单用例拆分，未扩大既有模块债务。
3. View 与逻辑物理隔离，UI 原语和素材集中。
4. 原始证据只读，结果有 provenance 和可回源对象 ID。
5. 错误、取消、超限和不支持情况显式返回。
6. 单元、集成和必要的 ignored 真实样本测试齐全。
7. 文档、支持矩阵、UI 文案和架构门禁同步更新。
8. 相关性能指标和资源上限有自动化验收。

## 14. 当前实施状态

已完成：

- MCP 前端 View/Controller/Hook 分层；
- MCP 文案、图标、动效集中；
- 全前端原生控件架构门禁；
- 媒体协议门禁入口同步；
- MCP 查询、数据源摘要、案件创建/删除用例拆分；
- NTFS WOF XPRESS/LZX 按块读取和真实 E01 验证。

待开发：

- Hash Scope 与 MD5/SHA1/SM3；
- NTFS Technical Inspector；
- 全文索引 Content Sniffing；
- EXIF、Structured systemd、Git/GitLab、Firefox Extensions；
- VHD/Nested Evidence；
- Registry Browser。

## 15. 长耗时计算与流程调度设计

哈希、全文索引、EXIF 批量提取、NTFS 技术扫描、恢复、BitLocker 字典、报告生成和嵌套证据导入都可能持续数秒到数小时。它们不能在 Tauri command、React 事件处理器或共享 UI 线程中同步执行。

### 15.1 用户体验目标

任何超过 200 ms 的操作都应进入任务系统或显示明确的 loading 状态；超过 1 秒的操作必须具备：

- 后台执行，界面保持可操作；
- 阶段、当前对象、已完成量、总量或不确定状态；
- 取消按钮和取消确认状态；
- 失败原因、是否可重试、是否产生部分结果；
- 任务结束后不强制跳转页面，由用户决定查看结果。

前端使用现有 Bottom Drawer/JobStatusCard 作为统一任务入口。任务详情显示在当前页，跨页面任务显示在全局任务抽屉；禁止每个 feature 自建一套进度条、toast 和取消协议。

### 15.2 任务模型

新增或统一以下概念：

```rust
TaskKind {
    Hash,
    SearchIndex,
    ArtifactAnalysis,
    NtfsInspection,
    MediaMetadata,
    Recovery,
    BitLockerDictionary,
    NestedEvidenceImport,
    ReportExport,
}

TaskPriority { Interactive, Normal, Background, Maintenance }
TaskState { Queued, Admitted, Running, Pausing, Paused, Cancelling,
            Completed, CompletedWithWarnings, Failed, Cancelled }
ResourceClass { Io, Cpu, Memory, Gpu, Mixed }
```

每个任务必须保存：

```text
taskId, caseId, dataSourceIds, kind, priority, state,
createdAt, startedAt, finishedAt, progress,
currentItem, completedItems, totalItems, bytesRead,
throughput, eta, resourceReservation, checkpoint,
cancelRequested, warningCount, errorCode, retryable
```

任务 DTO 放在 `transport`，字段使用 camelCase。任务参数不得包含密码、完整候选字典内容或未脱敏宿主路径。

### 15.3 调度器分层

现有 `TaskManager` 负责任务生命周期和取消，`HeavyTaskQueue` 负责重任务排队，`ImportAdmission` 负责 CPU/内存资源准入。后续统一成三层，不重复创建线程池：

```text
Task Registry
  ├─ 状态、审计、取消、恢复
Priority Scheduler
  ├─ Interactive > Normal > Background > Maintenance
Resource Admission
  ├─ CPU 权重、内存预留、I/O 槽位、GPU 槽位
Workers
  ├─ Rayon/阻塞线程池/专用解码器
```

应用服务只提交 `TaskSpec`，不直接 `std::thread::spawn`、不为每个任务创建无限线程池。Tauri 命令立即返回 `taskId`，进度通过既有事件总线推送。

### 15.4 资源准入策略

默认资源预算由主机逻辑核数和可用内存计算，并允许用户在设置中调整软上限：

| 任务 | CPU 权重 | 内存预留 | I/O 槽位 | 默认优先级 |
|---|---:|---:|---:|---|
| 当前文件 Hash | 1 | 64 MiB | 1 | Interactive |
| 批量 Hash | 2 | 256 MiB | 1 | Background |
| 全文索引 | 2 | 512 MiB | 1 | Background |
| NTFS Inspector | 1 | 128 MiB | 1 | Interactive |
| EXIF/Artifact 批处理 | 2 | 256 MiB | 1 | Background |
| BitLocker 字典 | 逻辑核上限 | 256 MiB | 1 | Background |
| 报告导出 | 1 | 256 MiB | 1 | Normal |

规则：

1. Interactive 任务可以抢占尚未开始的 Background 任务。
2. 同一证据源的顺序 I/O 任务默认只允许一个读槽位，避免 E01 随机读放大。
3. CPU 密集任务共享全局 CPU 预算；不得每个 BitLocker/Hash 任务各自占满所有逻辑核。
4. UI 预览读取保留一个低延迟 I/O 配额，不被批量索引饿死。
5. 内存预算是准入信号；解压、EXIF 和全文解析必须另有输出上限。
6. GPU 仅作为可选 ResourceClass；没有可用 GPU 时回退 CPU，不阻塞队列。

### 15.5 调度算法

采用“优先级 + 老化 + 资源准入”的公平队列：

```text
effectivePriority = basePriority + aging(seconds / 30)
```

每次调度：

1. 丢弃已取消且尚未开始的任务。
2. 优先选择准入条件满足且 `effectivePriority` 最高的任务。
3. Interactive 任务最多连续占用两个调度轮次，随后允许 Background 任务进入，避免长期饥饿。
4. 任务完成、取消或释放资源时唤醒等待队列。
5. 同一 `caseId + dataSourceId + TaskKind + inputFingerprint` 的任务进行去重：已有运行任务返回原 `taskId`，已完成任务返回可复用结果。

调度器必须暴露快照：CPU/内存预算、当前占用、队列长度、每类任务等待时间、峰值占用和被限流原因。

### 15.6 哈希任务流程

```text
创建 TaskSpec(Hash, scope, algorithm, inputFingerprint)
  ↓
快照证据 revision/size/mtime
  ↓
资源准入
  ↓
按固定范围读取（建议 4–16 MiB）
  ↓ 每个块检查 cancelToken
更新哈希、bytesRead、throughput
  ↓
每 250–500 ms 合并一次进度事件
  ↓
完成后校验源 revision 未改变
  ↓
写入 digest 结果和审计记录
```

哈希不应阻塞文件浏览；用户打开同一文件时，预览读取拥有独立低延迟配额。若源 revision 在任务期间变化，任务失败为 `evidence_changed`，不得返回未经确认的 digest。

### 15.7 全文索引和批量分析流程

全文索引按数据源分片，每个分片提交 checkpoint：

```text
sourceRevision + lastGlobalFileId + indexedCount + skippedCount
```

取消后保留已提交分片，恢复时从 checkpoint 继续；源 revision 变化则丢弃旧 checkpoint 并重新规划。单个损坏文件只记录 warning，不让整源任务失败；数据库提交按 500–2000 条批量事务完成，避免长事务阻塞案件读取。

EXIF、Firefox、Git 和 systemd 分析复用同一批处理协议：先枚举候选 ID，再按小批次解析，每批提交结果和进度。UI 显示“已处理/跳过/失败”，而不是只显示百分比。

### 15.8 BitLocker 字典流程

BitLocker 字典是 CPU 密集任务，使用专用 `ResourceClass::Cpu` 预算：

- 字典读取线程负责有界缓冲，不把整个字典载入内存；
- KDF worker 使用全局 CPU 配额，不再创建无上限并行池；
- 每个候选在 KDF 内部和 metadata 身份之间检查取消；
- 成功后设置共享 stop 标志，其余 worker 尽快退出；
- 进度按已读取字节和已尝试候选分别上报；
- 前端只显示计数、吞吐、ETA 和终态，不返回候选密码；
- 任务取消后密钥、密码和候选缓冲区立即清零或释放。

### 15.9 进度事件与前端体验

统一事件负载：

```json
{
  "taskId": "...",
  "state": "running",
  "phase": "hashing",
  "completedItems": 120,
  "totalItems": 900,
  "bytesRead": 8388608,
  "throughput": 5242880,
  "etaSeconds": 148,
  "currentItem": "Windows/System32/ntdll.dll",
  "cancelRequested": false,
  "warningCount": 2
}
```

事件节流：

- 进度变化达到 1% 或距离上次事件超过 500 ms 才推送；
- 终态事件必须立即推送；
- 前端以 `taskId` 去重，不能因重复事件增加计数；
- 页面卸载不取消任务，用户明确点击取消才发送取消命令。

任务卡提供：暂停（仅支持 checkpoint 的任务）、取消、重试、查看详情、打开结果。取消显示 `cancelling`，直到 worker 确认 `cancelled`，不提前伪造终态。

### 15.10 失败、重试和回退

| 错误 | 是否自动重试 | 处理 |
|---|---|---|
| 临时 I/O/数据库锁 | 最多 2 次指数退避 | 保留 checkpoint |
| 证据 revision 变化 | 否 | 要求重新规划任务 |
| 格式损坏 | 否 | 记录文件级 warning，继续批次 |
| 资源不足/排队超时 | 否 | 提示降低并发或稍后重试 |
| 用户取消 | 否 | 保留已提交结果和取消原因 |
| 内存预算超限 | 否 | 降低批次或改流式路径 |

任何部分结果必须标记 `partial=true`，报告中显示覆盖范围和跳过原因。失败重试不能覆盖成功结果，使用新的 task attempt ID。

### 15.11 验收测试

- 启动哈希或索引后，文件浏览、搜索输入和案件切换仍可响应。
- 两个 CPU 密集任务不会超过全局 CPU/内存预算。
- Interactive 预览能够在 Background 索引运行时获得读取配额。
- 取消任务在 1 秒内进入 `cancelling`，worker 在安全检查点进入 `cancelled`。
- 关闭案件会取消该案件任务，且不会向新案件发送旧任务事件。
- 应用重启后，支持 checkpoint 的任务可恢复；不支持恢复的任务显示可重试。
- 任务事件节流有效，前端不会因高频事件卡顿。
- 相同输入重复提交会复用运行任务或已完成 digest，不重复计算。
- 真实 E01、RAW、逻辑目录和 BitLocker 字典任务均验证源只读、结果可追溯和资源上限。

### 15.12 实施顺序

1. 为现有 TaskManager 增加统一 `TaskSpec`、优先级、资源快照和事件节流。
2. 将 Hash、全文索引和 BitLocker 字典迁移到统一调度入口。
3. 增加 checkpoint repository 和任务 attempt 记录。
4. 统一 Bottom Drawer 任务卡、取消/暂停/重试交互。
5. 增加资源预算、排队等待和吞吐指标的回归测试。
6. 最后接入 GPU 作为可选资源类，保持 CPU 回退路径和相同结果校验。
