# Meow~Detective

![ChatGPT Image 2026年7月29日 16_08_08.png|697](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/ChatGPT%20Image%202026%E5%B9%B47%E6%9C%8829%E6%97%A5%2016_08_08.png)

## 项目介绍

Meow~Detective 面向磁盘镜像、逻辑目录、Android 镜像与 Linux/PVE 证据源的本地离线分析。后端 workspace 当前包含 42 Rust crates。案件控制信息和每个数据源的取证数据分库存储：案件级数据库负责案件、数据源注册、任务和审计；分区、文件树、制品、时间线和源内索引保存于对应数据源的 `source.db`；可重建的案件级跨源关系投影保存于 `indexes/case-graph.db`。

当前工程事实快照：12 frontend pages（其中 `/v2` V2 workbench 仅在 dev/audit 模式注册，生产路由 11 个）、179 Tauri commands、49 source modules、migration scripts (105)、143 test files。计数由 `scripts/check-doc-drift.ps1` 与仓库结构同步校验。

RAW/E01 导入采用作业级 SourceRuntime、独立 reader 与共享 E01 OpenPlan，默认 worker 数为 `min(CPU 数, 2)`；并发由 1536 MB 软限的估算资源准入与独立的 2048 MB RSS 硬限共同约束。来源元数据身份用于续导绑定和失效检查，证据内容摘要仍由字节校验路径生成。

> [!IMPORTANT]
> **Windows 权限与系统服务特别说明**：发布版桌面应用使用 Windows manifest 的
> `requireAdministrator`，启动时会触发 UAC 并以管理员令牌运行。物理磁盘挂载会使用系统
> `MSiSCSI` 服务；应用可能在挂载期间临时启动该服务，并在服务租约结束后恢复由应用自身
> 修改的启动类型。应用不会自动安装 Dokan 或 iSCSI 组件，也不会强制停止可能被其他程序
> 使用的 `MSiSCSI` 服务。详细边界见下文 [Windows 权限与系统服务](#windows-权限与系统服务)。



## UI展示

|                          案件导入页                          |                          镜像导入页                          |
| :----------------------------------------------------------: | :----------------------------------------------------------: |
| ![image-20261021220435665](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/image-20261021220435665.png) | ![image-20261021220512877](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/image-20261021220512877.png) |
|                        **文件树预览**                        |                        **数据源分析**                        |
| ![image-20261021220611664](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/image-20261021220611664.png) | ![image-20261021220943464](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/image-20261021220943464.png) |
|                           **搜索**                           |                         **TIMELINE**                         |
| ![image-20261021221114880](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/image-20261021221114880.png) | ![image-20261021221153786](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/image-20261021221153786.png) |





## 功能简介

### 证据导入与文件浏览

- 当前数据源入口：E01/EWF、单文件 RAW/dd/img、VHD（fixed/dynamic/differencing）、VHDX、QCOW/QCOW2、Android sparse、逻辑目录、tar/gzip 逻辑包及 Linux 集群目录；Windows 主机物理磁盘可通过 `\\.\PhysicalDriveN` 以只读方式导入。
- ISO9660/Joliet 与 monolithic-flat VMDK 的只读 reader 已实现，可用于镜像内嵌套卷解析；但数据源分类器当前没有为 `.iso`/`.vmdk` 直选入口接线，从文件选择器直选这两类文件会返回 `unrecognized evidence container`（已知限制，见“支持边界”）。
- 镜像格式先进入统一的只读 reader，再进行分区探测和文件系统解析：E01/EWF 使用 E01 reader；RAW/dd/img 使用原始字节 reader；VHD/VHDX/QCOW/QCOW2 由 RawImageReader 按 footer/magic 识别为只读逻辑字节流；主机物理磁盘使用 LocalDiskReader；Android sparse 由 AndroidSparseReader 展开为逻辑镜像；ISO9660/Joliet 与 monolithic-flat VMDK 使用各自的只读 reader（直选入口见上一条限制）。适配器不会修改原始文件，也不会把不支持的容器猜测成 RAW。
- 文件选择器过滤器列出 `.e01`/`.E01`/`.ewf`、`.dd`/`.raw`/`.img`、`.iso`、`.vmdk`、`.simg`/`.sparse`、`.tar`/`.tar.gz`/`.tgz`/`.gz`/`.gzip`；VHD/VHDX/QCOW/QCOW2 未列入过滤器，可通过选择器“所有文件”选择。扩展名参与数据源类型判定（`.dd/.raw/.img/.bin/.001` 兜底为 RAW），最终是否可读仍由 magic、descriptor、PVD、分区与长度校验决定。
- VMDK 目前仅接受 UTF-8、`createType="monolithicFlat"`、单个零偏移 `FLAT` extent；descriptor 与 extent 的逻辑长度、截断、溢出和路径边界都会校验。`.001/.002/...` 分卷集合、sparse/streamOptimized VMDK 明确拒绝；分卷 RAW（`.001/.002`）同样拒绝，只接受单文件镜像。VHD fixed/dynamic/differencing、带相对父链的 VHDX，以及无压缩 QCOW/QCOW2 backing chain 提供只读读取，按 footer/magic 分类、不依赖扩展名；压缩 QCOW2 cluster 和损坏的虚拟磁盘元数据 fail-closed。
- ISO 适配器优先使用 Joliet 名称，目录和文件 extent 均受卷边界限制；可解析嵌套在 E01/RAW/VHD/受支持 flat VMDK 字节流中的 ISO 卷，直接打开卷由同一 reader 完成。`.iso` 直选导入入口当前未接线（见上）。UDF、Rock Ridge、交错或 multi-extent 记录不在当前承诺范围。
- NTFS 导入复用数据源 reader 工厂，按资源预算并行读取和解析 MFT，再按记录顺序由单一 writer 写入数值 staging。新 source DB 的次级索引在合并后重建并执行 ANALYZE，完成前不会发布 Ready；异常退出后的索引状态可随导入恢复。
- 自动识别 MBR/GPT、常见分区与文件系统；支持 NTFS、FAT、exFAT、ext4、XFS、Btrfs、F2FS、EROFS 以及 Linux LVM 直接逻辑卷映射（F2FS/EROFS 主要面向 Android 镜像）。
- 文件树、目录分页、筛选、自然排序、文件属性与数据源分区信息均由后端统一提供。
- 文本、Hex、图片、音视频、PDF/Office/SQLite 等预览走受限的只读证据读取链路；大文件使用范围读取，避免一次性载入完整文件。
- 支持向调查员指定的路径提取文件，默认禁止覆盖，提取过程返回真实进度和完整性信息。
- 提供 NTFS、ext4 与 XFS 的删除文件恢复/雕刻能力，恢复范围和完整性状态会显式标注。
- 镜像挂载提供逻辑分区和完整物理磁盘两种只读模式；逻辑分区通过 Dokan Mount Manager
  发布为普通资源管理器可见的只读盘符，物理磁盘模式通过本机回环 iSCSI 交给 Windows
  原生卷管理，不依赖自写驱动或 Arsenal 授权。

### Windows 权限与系统服务

这是挂载功能的运行前置和系统影响边界，使用前应明确区分两种模式：

| 范围 | 权限/服务要求 | 应用行为 | 退出与恢复边界 |
|---|---|---|---|
| 桌面应用 | 发布版需要管理员令牌 | 启动时由 UAC 授权，不在运行中静默提权 | 拒绝 UAC 后无法启动桌面应用 |
| 逻辑分区挂载 | 需要预先安装 Dokan runtime/driver | 使用 Dokan `Mount Manager` 发布全局只读盘符；不使用 `CURRENT_SESSION`，不修改 Windows 服务 | 应用负责卸载并清理挂载；Dokan 未安装时返回明确错误，不静默调用外部挂载工具 |
| 物理磁盘挂载 | 需要管理员令牌和 Microsoft iSCSI Initiator (`MSiSCSI`) | 通过本机 loopback iSCSI 将 E01、单文件 RAW/dd/img 或已验证 flat VMDK 的逻辑字节流交给 Windows；不安装自写驱动、不依赖 Arsenal 授权 | 首个物理挂载建立服务租约；最后一个挂载释放后恢复应用临时修改的启动类型，不强制停止服务 |
| `MSiSCSI` 已禁用 | 需要服务配置权限 | 挂载前临时改为 `Manual` 并启动；仅由应用持有的租约负责恢复 | 若其他程序同时改变配置，应用保留外部修改；应用崩溃或被强制终止后需管理员复核服务启动类型 |
| Windows Search (`WSearch`) | 不修改系统服务 | 逻辑挂载的虚拟文件节点声明 `NOT_CONTENT_INDEXED`，减少宿主索引对目录回调的争用 | 不停止、不禁用、不修改全局搜索策略 |

无论哪种模式，原始证据始终只读；服务启动类型、挂载生命周期和错误状态由后端维护，前端只显示后端返回的状态。常规 Rust 测试使用非提权测试 manifest，不需要 UAC；只有桌面应用启动、Dokan 互操作和物理 iSCSI 互操作测试需要管理员权限。

### Windows 取证分析

- 注册表：SYSTEM、SOFTWARE、NTUSER、SAM、SECURITY、USRCLASS、Amcache 与事务日志基础解析。
- 事件日志：EVTX 解析、开关机、登录、进程、账户与应用事件分类。
- 用户行为：Prefetch、LNK、Jump List、Recycle Bin、SRU、Thumbcache、UsrClass.dat 中的 Shellbags/MuiCache、浏览器历史/下载/会话等。
- 浏览器与凭据：Chrome、Edge、Firefox 浏览器数据提取；离线 DPAPI 相关链路按前置材料与支持边界处理。
- BitLocker：卷元数据检查、密码/恢复密码解锁，以及在文件浏览器为已锁定卷选择本地 UTF-8 密码字典并逐行验证；字典任务显示已尝试数与读取进度，可取消，且不向前端返回候选密码。匹配成功后只沿既有已验证密钥链路解锁和持久化；原始证据始终只读。匹配的 Windows x64 内存镜像可用于受限的密钥恢复与卷级验证，定位链路经内嵌的 1077 build 内核符号注册表（Windows 10 10240 → Windows 11 28000），不绑定单一 Windows 版本。

### Linux 与 PVE 分析

- Linux 取证能力：systemd journal、wtmp、bash history、apt/dpkg/yum/dnf、cron、sudo/auth.log(/var/log/secure)、系统配置、Git/GitLab 配置与提交记录、Nginx/Apache 站点与日志、MySQL/MariaDB 配置和日志。
- Linux 数据源自动进入独立分析视图；Windows 与 Linux 提取能力不会交叉调度。
- PVE/Ceph 相关能力包括成员发现、宿主 LVM/ext4、BlueStore/BlueFS/RocksDB 元数据读取、RBD 派生虚拟磁盘与文件预览。RBD 只有在 OSD inventory 覆盖报告为完整、报告摘要和副本策略指纹均匹配时才会物化或复用；覆盖不足或显式多池证据无法绑定到数据池时仅保留元数据并失败关闭。该部分仍以私有真实样本基线为主，完整 CRUSH/EC、降级副本、通用 CephFS 重建等场景尚未承诺支持。

### Android 分析

- Android sparse 镜像（`.simg`/`.sparse`）作为独立 `AndroidSparse` 数据源入口，导入时需选择 Android 平台；稀疏镜像展开为逻辑镜像后按 MBR/GPT 或直接卷探测，镜像内 ext4/F2FS/EROFS 文件系统可浏览、预览与提取。
- Android 分析工作区读取 `build.prop`（system/system_ext/product/vendor/odm，含型号/厂商/版本/指纹/序列号）、SettingsProvider `settings_secure`（Android ID，并标注“Android 8+ 可按应用签名与用户分区”的置信度提示）与 `settings_global`（设备名）、`packages.xml`/`packages.list`（包名、版本、安装/更新时间、安装器、UID、code path），并从 `package-restrictions.xml` 派生每个用户的安装/停用/挂起/隐藏/停止状态；另可按需读取单个 APK 的标签与 PNG/WebP 图标（有界、不落地）。
- 不提供 FBE/FDE 解密，也不提供 IMEI、SMS/MMS、联系人、浏览器/应用私有数据等专用制品解析（对应文件仍可在文件浏览器中只读查看）；动态分区（super/liblp）解析尚未接入生产路径。这些场景不做猜测性推断。

### 仿真取证

- E01、单文件 RAW/dd/img 和已验证 flat VMDK 镜像通过写时复制（COW）overlay 直通给虚拟机：guest 的全部写入只落入单次会话的 overlay 日志，原始证据保持只读；会话结束后 overlay 不复用，崩溃语义等同于物理机断电丢失未落盘写入。ISO 文件系统适配器本身是只读导入/预览路径，不等同于已验证的物理磁盘启动介质。
- 应用生成 VMware 材料（VMDK 描述文件与 VMX 配置），并通过 `vmrun` 直接启动、软关机和释放仿真会话；需要用户自行安装 VMware Workstation，应用不捆绑或再分发 VMware 组件。
- 预检直接消费镜像导入阶段已建立的全局索引，不重复扫描镜像：Windows 检查注册表配置单元与启动风险，Linux 识别发行版、根文件系统与 LVM 布局并给出启动路线建议。
- 维护引导 ISO 由应用内生成并可在 UI 探测存在性；guest 侧工具完成 OSDATA 命名空间筛查清除。登录绕密在主机侧通过 COW overlay 写入：Windows 支持 SAM 与 Utilman 绕密，Linux 支持 `/etc/shadow` 绕密，另提供 GRUB 引导参数的手册路线。
- 虚拟机配置由用户显式选择：CPU 核心数（1–64）与内存（512–65536 MiB）滑块、网络模式（off/host-only/NAT/bridged）、剪贴板与时间同步勾选框。共享文件夹在 VMX 中强制关闭（`sharedFolder.maxNum=0`、HGFS 禁用），不提供开关。

### 调查、关联与输出

- 全文检索、时间线、实体归并、关联图、Notebook 调查记录、规则包（加载/校验/覆盖度）与批处理计划。批处理执行控制（启动/暂停/恢复/取消）当前返回 typed unsupported，仅创建与查询可用。
- HTML、CSV、JSON 与证据包报告导出；报告和错误信息遵循脱敏规则。
- 内置本机 MCP 服务端，随应用启动并监听 `http://127.0.0.1:3001/mcp`，支持设置页启停、连接地址复制及逐项禁用工具。提供 29 个工具，覆盖案件、数据源、插件模块、目录分页、文件元数据、文件内容读取、分析摘要、时间线、搜索和内容索引；文件内容每块最多 64 KiB，UTF-8/Base64。`forensics.extract_file` 默认禁用，启用后可将当前案件普通文件提取到案件工作区和证据源目录之外的绝对路径，默认拒绝覆盖。工具调用使用当前打开案件并记录审计，原始证据保持只读。
- NTFS WOF 文件压缩支持 XPRESS 4K/8K/16K 与 LZX 32K 的按需解压，适用于预览、导出、流式读取及 MCP；大文件按块定位并限制缓存。外置 WIM 提供器需要关联 WIM 数据源，当前明确返回不支持。

### 制品查询与分页

- 制品列表以数据源独立的快照游标为事实边界；游标绑定案件、family、数据源集合、
  source revision、rowid high-water 和已消费排序键，跨源结果按创建时间、数据源 ID、
  制品 ID 确定性合并。
- 游标 API 返回不透明 `nextCursor`。数据源 revision、记录数量或数据源集合变化时，
  已发出的游标会失效并返回 typed invalid-input 错误，不会把新旧快照混合展示。
- 旧客户端仍可提交 offset 请求；该兼容入口只把 offset 分段转换为游标消费，不再维护
  独立的 offset 合并算法。前端无限列表直接使用游标接口，避免按固定页码重复扫描跨源数据。

## 支持边界

| 范围                  | 当前状态 | 说明                                              |
| ------------------- | ---- | ----------------------------------------------- |
| Windows / Linux 数据源 | 支持   | 以 Windows 为主，Linux 文件系统与取证能力按解析器分别标注成熟度。        |
| macOS / APFS / HFS+ | 不支持  | 可识别部分分区类型，但不创建文件系统 reader。                      |
| Android 数据源 | 支持（有界） | sparse 镜像作为 AndroidSparse 入口（需选择 Android 平台）；镜像内 ext4/F2FS/EROFS 可浏览与预览；分析读取 build.prop、SettingsProvider（Android ID/设备名）、`packages.xml`/`packages.list`、`package-restrictions.xml` 与 APK 标签/图标。FBE/FDE 解密与 IMEI/SMS/MMS/联系人等专用制品不在范围（文件本身仍可只读查看）。 |
| ISO / flat VMDK 直选导入 | 入口未接通 | 只读 reader 已实现并可解析镜像内嵌套卷；`.iso`/`.vmdk` 直选会被数据源分类拒绝，需作为嵌套卷或改为 RAW 扩展名（`.img` 等）读取。 |
| 逻辑包（tar/gzip） | 支持 | `LogicalArchive` 只读枚举与范围读取；7z/RAR 等其它容器不作为数据源入口。 |
| PVE / Ceph          | 实验性  | 已覆盖私有样本中的部分 BlueStore、RBD 与派生 VM 文件树；不宣称通用集群重建。 |
| BitLocker           | 部分支持 | 仅在受支持加密方法、保护器和可验证密钥材料范围内可用。                     |
| 仿真取证               | 部分支持 | 仅 VMware Workstation 后端；Windows 与 Linux 镜像可仿真；输入支持 E01、单文件 RAW/dd/img 和已验证 flat VMDK（flat VMDK 需以 RAW 扩展名导入，见上）；btrfs 根卷不支持主机侧绕密编辑。 |
| 原始证据写入              | 禁止   | 系统仅读取原始证据；派生数据写入案件工作区或调查员显式选择的导出目录。             |

完整的解析器成熟度、样本基线和已知限制见 [解析器支持矩阵](docs/parser-support-matrix.md) 与 [已知不支持格式](docs/known-unsupported-formats.md)。

## 架构图

### 分层架构

```mermaid
flowchart TB
    UI["React 18 + TypeScript + Vite\n调查工作台"]
    IPC["Tauri 2 IPC\ncommands / events"]
    CMD["桌面命令层\n校验、DTO 适配、调用服务"]
    SVC["应用服务层\n案件、导入、预览、分析、导出编排"]
    CORE["核心能力层\n证据读取、文件系统、制品、检索、时间线、报告"]
    STORE["持久化与运行时\nSQLite、索引、受限句柄、密钥存储"]
    EVIDENCE["只读证据源\nE01 / RAW / VHD / VHDX / QCOW2 / ISO / flat VMDK / Android sparse / 目录 / 逻辑包 / 集群成员"]

    UI --> IPC --> CMD --> SVC
    SVC --> CORE
    SVC --> STORE
    CORE --> EVIDENCE
    STORE --> EVIDENCE
    IPC -. "进度与状态事件" .-> UI
```

### 案件与数据源模型

```mermaid
erDiagram
    CASE ||--|| CASE_CONTROL_DB : "拥有 app.db"
    CASE ||--o{ DATA_SOURCE : "注册"
    CASE ||--o{ JOB : "调度"
    DATA_SOURCE ||--|| SOURCE_DB : "独立 source.db"
    DATA_SOURCE ||--|| SOURCE_INDEX : "独立索引"
    DATA_SOURCE ||--o{ PARTITION : "包含"
    PARTITION ||--o{ FILE_ENTRY : "枚举"
    FILE_ENTRY ||--o{ ARTIFACT : "产生"
    FILE_ENTRY ||--o{ TIMELINE_EVENT : "投影"
    ARTIFACT }o--o{ ENTITY : "关联"

    CASE {
        string case_id
        string name
        string case_root
    }
    DATA_SOURCE {
        string data_source_id
        string platform
        string import_state
        string source_db_rel_path
    }
    SOURCE_DB {
        string partitions
        string file_entries
        string artifacts
        string timeline_events
    }
    FILE_ENTRY {
        string global_file_id
        string local_file_id
        string parent_id
        string evidence_path
    }
```

全局文件 ID 采用 `ds:<dataSourceId>:<localId>` 形式。前端只接收 DTO 和逻辑 ID，不接触主机原始路径或数据库物理路径。

### 导入状态与处理阶段

```mermaid
stateDiagram-v2
    [*] --> Pending: 注册数据源
    Pending --> Importing: 调度器取得写入租约
    Importing --> Ready: 目录与分区可浏览
    Importing --> Failed: 读取、解析或校验失败
    Failed --> Pending: 删除后重新导入
    Ready --> [*]

    state Importing {
        [*] --> Catalog
        Catalog --> Graph
        Graph --> Platform
        Platform --> Artifacts
        Artifacts --> Timeline
        Timeline --> Search
        Search --> [*]
    }
```

`ready` 表示文件目录已可浏览；Catalog、Graph、Platform、Artifacts、Timeline、Search 的完成情况分别记录，避免把“已导入”误判为“所有分析已完成”。

## 模型与算法图

### 证据解析与预览算法链路

```mermaid
flowchart LR
    INPUT["证据输入"] --> ADAPTER{"只读镜像适配器"}
    ADAPTER -->|"E01/EWF"| E01["E01 reader"]
    ADAPTER -->|"RAW/dd/img"| RAW["RawImageReader"]
    ADAPTER -->|"monolithic-flat VMDK"| VMDK["descriptor + FLAT extent"]
    ADAPTER -->|"ISO9660/Joliet"| ISO["ISO reader"]
    E01 --> PROBE["镜像与分区探测"]
    RAW --> PROBE
    VMDK --> PROBE
    ISO --> PROBE
    PROBE --> ROUTE{"卷类型"}
    ROUTE -->|"NTFS/FAT/exFAT"| WINFS["Windows 文件系统 reader"]
    ROUTE -->|"ext4/XFS/Btrfs"| LINUXFS["Linux 文件系统 reader"]
    ROUTE -->|"LVM PV"| LVM["PV -> VG -> LV 偏移映射"]
    LVM --> LINUXFS
    ROUTE -->|"BitLocker"| BDE["保护器解锁与扇区解密\n卷级验证"]
    BDE --> WINFS
    WINFS --> CATALOG["分区文件目录与 source.db"]
    LINUXFS --> CATALOG
    CATALOG --> ANALYSIS["平台制品提取\n检索、时间线、关联"]
    CATALOG --> VIEWER["文件句柄 + 范围读取\n文本/Hex/媒体/文档预览"]
    ANALYSIS --> REPORT["可追溯报告与调查视图"]
```

关键算法约束：I/O 密集的镜像读取保持有界和顺序化，CPU 密集的独立解析任务可使用 Rayon 并行；SQLite 每个数据源单写者、多读者；大文件预览和媒体读取使用范围请求与短生命周期句柄。

### BitLocker 内存辅助验证模型

```mermaid
flowchart TD
    MEM["只读 Windows x64 内存镜像"] --> BOOT["首 1 MiB 启动结构\nCR3 与内核入口"]
    BOOT --> GUID["ntoskrnl CodeView GUID"]
    GUID --> REG["内嵌符号注册表\n1077 build（10240 → 28000）"]
    GUID -->|"未知 build"| CARVE["版本无关 driver-object 雕刻"]
    REG --> OBJ["Object Manager / Driver 对象链"]
    CARVE --> OBJ
    OBJ --> KEY["FVEVol keyring / device context\nVMK datum"]
    KEY --> VERIFY["卷级验证\nNTFS boot/MFT/$UpCase/$Bitmap"]
    VERIFY -->|"成功"| RUNTIME["只读运行时解锁注册表\n安全持久化已验证密钥包"]
    VERIFY -->|"失败"| REJECT["拒绝候选\n不记录密钥材料"]
    RUNTIME --> PREVIEW["文件树、预览与提取"]
```

该链路不把 FVEK、VMK 或未验证候选写入日志、报告、案件数据库或前端。只有经过卷级认证的运行时解锁状态才可用于后续只读访问；内存解锁 command 只返回卷状态。恢复密码反推在反向 datum 与内存 VMK 同代时成立并以 recovered 呈现，不同代时显式报 unavailable，不提供用户密码恢复 command、DTO 或 UI。

## 开发模式运行

### 环境要求

- Windows 10/11 x64。
- Rust `1.99.0`（仓库 `rust-toolchain.toml` 固定 channel，并声明 rustfmt/clippy 组件）。
- Node.js LTS、Corepack 与 pnpm `10.25.0`。
- Visual Studio 2022 Build Tools，安装 Desktop development with C++ 和 Windows SDK。
- WebView2 Runtime（Tauri 桌面运行时需要）。
- 仿真取证功能需要用户自行安装 VMware Workstation（含 `vmrun`）；不使用仿真时无此前置。
- 构建签名便携版需要 Python 3.11+（`scripts/portable-release.py`，仅标准库）。
- 发布版应用清单请求管理员权限，启动时会显示 UAC；挂载模式、Dokan 前置和
  `MSiSCSI` 服务租约的详细边界见 [Windows 权限与系统服务](#windows-权限与系统服务)。

涉及链接的 Rust 命令应在 **x64 Native Tools Command Prompt for VS 2022** 或已执行 `vcvars64.bat` 的终端中运行，避免误用 Git 自带的 `link.exe`。
管理员 manifest 仅链接最终桌面 binary；Rust 测试 harness 使用独立的非提权 Common Controls
manifest，因此常规 `cargo test` 不需要 UAC。只有启动桌面应用和物理磁盘挂载互操作测试需要管理员权限。

### 安装依赖

```powershell
corepack enable
pnpm --dir frontend install --frozen-lockfile
cargo fetch
```

### 启动完整桌面开发模式

在仓库根目录执行：

```powershell
Set-Location apps/desktop/src-tauri
cargo tauri dev
```

`tauri.conf.json` 未配置 `devUrl`/`beforeDevCommand`，因此 `cargo tauri dev` 直接加载 `frontend/dist`，启动前需先执行 `pnpm --dir frontend build`，不提供 Vite 热更新。

需要不依赖真实 IPC 的 UI 走查时，使用 `pnpm --dir frontend dev:mock`（`vite --mode mock`）：它把 `@tauri-apps/api/core` 等模块映射到 `frontend/mocks/`。mock 仅限 dev server——`vite.config.ts` 会拒绝在 mock 模式下构建，mock 模块在生产 bundle 中会抛错，因此不会作为证据界面发布。单独运行 `pnpm --dir frontend dev` 只适合样式和组件开发，不能替代完整的取证 IPC 运行环境。

### 前端单独检查

```powershell
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
# UI 走查（可选，mock IPC 仅 dev server，构建会被拒绝）
pnpm --dir frontend dev:mock
```

### Rust 后端检查

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p forensics-desktop
```

### 构建发布包

```powershell
pnpm --dir frontend build
Set-Location apps/desktop/src-tauri
cargo build --release
```

`tauri.conf.json` 将生产前端目录固定为 `frontend/dist`，因此发布构建前必须先完成前端构建。发布产物是**签名便携 EXE**，不生成安装器：配置中 `"bundle": { "active": false }`，`cargo tauri build` 只编译可执行文件、不会产出 MSI/NSIS。

发布流程由 `scripts/portable-release.py` 完成（`preflight` → `package` → 签名 → `manifest`）：

1. `pnpm --dir frontend build`，随后在 `apps/desktop/src-tauri` 执行 `cargo build --release`。
2. `python scripts/portable-release.py preflight --tag vX.Y.Z`：校验 tag 与 workspace/frontend/Tauri 版本一致，并检查发布环境注入的 `MEOW_UPDATER_PUBLIC_KEY`。
3. `python scripts/portable-release.py package --tag vX.Y.Z --binary <exe>`：在复制的 EXE 尾部追加 `MEOW-PORTABLE-V1` 元数据（版本与案件库 schema 摘要），输出到 `artifacts/portable-release/`。
4. 使用 Tauri signer 生成 `.sig` 后执行 `python scripts/portable-release.py manifest --tag vX.Y.Z --binary <signed exe> [--notes <file>]`，生成静态更新索引 `latest.json`。

### 应用更新

应用内置更新检查与自更新（`crates/portable-updater` + Tauri updater 插件，仅 Windows x86_64）。便携 EXE 尾部携带版本与案件库 schema 摘要；更新索引 `latest.json` 使用 Tauri/minisign 签名，运行时校验签名、禁止降级、要求 `case_schema` 与编译期 `MEOW_CASE_SCHEMA` 一致，并把下载 URL 限制为 GitHub release 的 `*.exe`、体积上限 512 MiB。替换由独立 helper 在父进程退出后执行：先备份原 EXE，应用候选并确认能再次启动，失败则回滚；程序目录出现 reparse point 或符号链接时拒绝更新。公钥在编译期由 `MEOW_UPDATER_PUBLIC_KEY` 注入；本地开发构建为空时会显式报告“不可更新”。

## 工程结构

| 路径 | 职责 |
|---|---|
| `frontend/` | React 18、TypeScript、Vite、Tailwind 4 调查界面与前端测试。 |
| `apps/desktop/src-tauri/` | Tauri shell、命令薄适配器、事件桥接和桌面运行时。 |
| `crates/transport/` | Rust DTO、请求、事件与错误契约的唯一事实源。 |
| `crates/app-services/` | 案件、导入、预览、分析、BitLocker、恢复、导出等用例编排。 |
| `crates/persistence-sqlite/` | SQLite 迁移、仓储与 source-local 数据库访问。 |
| `crates/evidence-core/`、`image-e01/` | 统一证据 reader、RAW/dd/img、tar/gzip 逻辑包、ISO9660/Joliet、flat VMDK 适配、镜像探测、分区和 E01 读取。 |
| `crates/fs-*/`、`fs-lvm/` | NTFS/FAT/exFAT/ext4/XFS/Btrfs 与 LVM 文件系统能力。 |
| `crates/image-android/`、`artifacts-android/` | Android sparse 镜像读取，以及 build.prop、SettingsProvider、packages.xml/list 与 APK 标签/图标解析。 |
| `crates/artifacts-windows/`、`artifacts-linux/` | Windows 和 Linux 取证制品解析。 |
| `crates/search/`、`timeline/`、`reports/` | 检索、时间线和报告能力。 |
| `crates/ceph-wire/`、`rocksdb-wire/` | 只读 Ceph/BlueStore/RocksDB 低层解析原语。 |
| `crates/evidence-emulation/`、`winpe-maintenance/` | COW 虚拟磁盘、VMDK/VMX/ISO9660 材料生成，以及维护引导 ISO 的 guest 侧工具。 |
| `docs/` | 支持矩阵、测试基线、错误分类、安全和前后端契约。模块设计与开发文档仅保留在工作站。 |
| `scripts/` | 架构边界、质量门禁、真实样本回归和基准脚本。 |
| `crates/portable-updater/`、`scripts/portable-release.py` | 签名便携 EXE 的自更新校验、备份/替换/回滚，以及静态更新索引生成。 |

## 质量与安全约束

- Rust DTO 必须定义在 `crates/transport/src/dto/`，前端 TypeScript 镜像需要手工同步。
- 页面不得直接调用 Tauri `invoke`；所有请求必须经由 `frontend/src/lib/api/`。
- 原始证据只读；预览不能通过拼接宿主路径访问证据内容。
- 导出前校验目标路径，默认 `overwrite=false`，使用临时文件和原子改名避免半成品。
- 生产代码与测试物理分离；后端模块、函数、命令边界均由仓库 PowerShell guard 检查。
- 自更新只接受签名匹配、禁止降级且案件库 schema 一致的便携 EXE；程序目录含符号链接或 reparse point 时拒绝替换。

完整门禁、真实样本测试和文档索引见 [验证可信框架](docs/validation-trust-framework.md) 与 [文档索引](docs/documentation-index.md)。

### 技术文档

- [预期 JSON 契约](docs/expected-json-contract.md)
- [错误分类手册](docs/error-classification-manual.md)
- [性能基线](docs/benchmark-baseline.md)
- [解析器支持矩阵](docs/parser-support-matrix.md)
- [已知不支持格式](docs/known-unsupported-formats.md)
- [文档索引](docs/documentation-index.md)
- [验证可信框架](docs/validation-trust-framework.md)
- [错误分类与脱敏口径](docs/error-taxonomy.md)
- [插件开发契约](docs/plugin-development-contract.md)
- [MCP 安全模型](docs/mcp-security-model.md)
- [导出与媒体安全](docs/export-and-media-safety.md)

性能门禁的结构化事实源为 `testdata/governance/v2-benchmark-baseline.json`。

## 许可证

本项目采用 [MIT License](LICENSE)；如仓库未包含独立许可证文件，以根目录 `Cargo.toml` 中的 `license = "MIT"` 声明为准。

## 致谢与第三方版权声明

本项目在设计与实现过程中借鉴或学习了以下开源项目，特此致谢。各项目的许可证文本归其原作者所有；本项目对它们的使用仅限于设计思想参考、受控适配或许可证允许的代码复用。

| 项目 | 许可证 | 借鉴/使用方式 |
|---|---|---|
| [Autopsy](https://github.com/sleuthkit/autopsy) | Apache-2.0 | 数字取证工作台的能力分层与工作流闭环（案件 → 数据源 → 文件浏览 → 工件提取 → 检索 → 时间线 → 报告）的产品与架构思想参考；未复用其源码 |
| [The Sleuth Kit](https://github.com/sleuthkit/sleuthkit) | IPL-1.0 / CPL-1.0 | BitLocker 卷侧 `metadata -> VMK -> FVEK -> sector reader` 处理顺序与元数据冗余策略的正确性参照；未复用其源码 |
| [Dokany](https://github.com/dokan-dev/dokany) / [dokan-rust](https://github.com/dokan-dev/dokan-rust) | Dokany runtime/driver: LGPL；Rust binding: MIT | Windows 逻辑分区只读盘符的用户态文件系统 runtime 与 Rust API；本项目不自动安装或再分发 Dokany runtime，挂载前置要求见本文 Windows 权限与服务章节 |
| [libewf](https://github.com/libyal/libewf) | LGPL-3.0 | 参考 `ewfmount.c`、Dokan/FUSE 回调和 EWF handle 生命周期设计，确定最小只读操作集、句柄边界与卸载清理；生产链路使用本项目 `image-e01` reader，未复用 libewf 源码或调用其 CLI |
| [Microsoft go-winio](https://github.com/microsoft/go-winio) | MIT | `crates/fs-ntfs` 的 WOF WIM-LZX 解压按 go-winio `wim/lzx` 的算法与帧结构适配，上游 MIT 许可证文本保留于 `crates/fs-ntfs/NOTICE`，仓库未包含其 Go 源码文件 |
| [libyal/libfwevt](https://github.com/libyal/libfwevt) | LGPL-3.0 | `crates/evtx-patched` 的 `WEVT_TEMPLATE`（manifest）解析为按 libfwevt《Windows Event manifest binary format》文档重写的 Rust 解析器，字段语义与参考 C 实现对齐，见 `crates/evtx-patched/src/wevt_templates/manifest/mod.rs`；仓库未包含其 C 源码文件 |
| [AOSP libsparse / liblp](https://android.googlesource.com/platform/system/core/+/refs/heads/main/libsparse/) | Apache-2.0 | `crates/image-android` 的 sparse 格式/CRC32 与 `crates/volume-android` 的 liblp 逻辑分区线格式按 AOSP 实现契约与已知测试向量对照，见对应 crate 注释；仓库未包含 AOSP 源码文件 |
| [XFS 内核日志格式](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/fs/xfs) / [xfsprogs](https://git.kernel.org/pub/scm/fs/xfs/xfsprogs-dev.git) | GPL-2.0 | `crates/fs-xfs` 的日志解析按 `xfs_log_format.h`、`xfs_log_recover.c` 与 xfsprogs log-print/recovery 的结构和行为对照实现，见 `crates/fs-xfs/src/log/mod.rs`；仅解析已提交事务元数据，仓库未包含上游源码文件 |
| [iscsi-target](https://github.com/lawless-m/iscsi-crate) | MIT OR Apache-2.0 | 物理磁盘模式的本机 loopback iSCSI target 基础实现；仓库在 `vendor/iscsi-target` 保留许可证并增加写保护 SCSI 与 Windows 互操作补丁，修改说明见 `vendor/iscsi-target/MEOW_PATCH.md` |
| [Microsoft Windows iSCSI API](https://learn.microsoft.com/windows/win32/iscsi/portal) | Microsoft Learn / Windows SDK 使用条款 | 物理挂载使用系统 `iscsidsc.dll`、Microsoft iSCSI Initiator 和 Service Control Manager API；不复制或分发 Windows 组件，服务修改范围见本文“Windows 权限与系统服务” |
| [omerbenamram/EVTX](https://github.com/omerbenamram/EVTX) | MIT OR Apache-2.0 | `crates/evtx-patched` 为其本地补丁分支（去除失维护依赖），上游许可证文本保留于 `crates/evtx-patched/LICENSE-APACHE` 与 `crates/evtx-patched/LICENSE-MIT` |
| [SecurityRonin/bitlocker-forensic](https://github.com/SecurityRonin/bitlocker-forensic)（含 [elephant-diffuser](https://github.com/SecurityRonin/elephant-diffuser)） | Apache-2.0 | `crates/volume-bitlocker` 派生自 bitlocker-core 0.3.5 与 elephant-diffuser（Albert Hui 著）。上游许可证文本保留于 `crates/volume-bitlocker/LICENSE-APACHE-2.0-UPSTREAM`，修改声明（Apache-2.0 §4(b)）与逐文件来源校验见 `crates/volume-bitlocker/NOTICE` 和 `docs/bitlocker-dependency-decision.md` |
| [shadcn/ui](https://ui.shadcn.com/) | MIT | 前端 `frontend/src/app/components/ui/` 的 UI 原语组件集（Radix Slot + cva + `cn()` 结构，已按本项目主题改写），见 `frontend/ATTRIBUTIONS.md` |
| [winbindex](https://github.com/m417z/winbindex) | GPL-3.0 | ntoskrnl 各 build 元数据索引，作为内嵌内核符号注册表的采集入口（配合微软公共符号服务器 PDB），见 `crates/memory-windows/symbols/README.md`；仅消费其公开索引数据，未复用其源码 |

此外，DPAPI / TBAL 离线恢复链路参考了公开研究（[TBAL: an (accidental?) DPAPI Backdoor for local users](https://vztekoverflow.com/2018/07/31/tbal-dpapi-backdoor/) 与 [pypykatz](https://github.com/skelsec/pypykatz) 的算法说明），仅作算法对照，未复用其代码。仿真取证的绕密与维护引导设计（SAM/Utilman 绕密、OSDATA 命名空间处理、Linux `/etc/shadow` 编辑与 GRUB 单用户路线）参考了公开技术资料中的通用技术原理，实现为本项目独立代码。

此外，以下公开格式规范作为字段语义与边界校验的参考（仅文档对照，未复制源码）：Microsoft exFAT 规范、NTFS `$LogFile`（`[MS-FSA]`/`[MS-NRBF]`）、BinXml（`[MS-EVEN6]`）、PE/COFF 资源目录、AOSP sparse/liblp 线格式、Linux XFS journal、Ceph BlueStore/BlueFS/RocksDB 磁盘格式，以及 VMware VMX/VMDK 材料约定。

### 主要开源依赖

桌面端与前端构建在以下开源项目之上（各项目许可证归其作者所有，完整依赖清单以 `Cargo.lock`/`pnpm-lock.yaml` 为准）：Tauri 2、tauri-plugin-updater、dokan-rust、React 18、React Router、Radix UI、Tailwind CSS 4、TanStack Query/Virtual、zustand、i18next、lucide-react、recharts；Rust 侧 SQLite/rusqlite（SQLite 为公有领域）、tantivy、flate2/tar、serde、tokio、rayon、mimalloc、zip 等。
