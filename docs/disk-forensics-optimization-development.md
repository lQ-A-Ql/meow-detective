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
