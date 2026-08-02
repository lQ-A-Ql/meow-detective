# Meow~Detective

![ChatGPT Image 2026年7月29日 16_08_08.png|697](https://raw.githubusercontent.com/lQ-A-Ql/blog-image/main/ChatGPT%20Image%202026%E5%B9%B47%E6%9C%8829%E6%97%A5%2016_08_08.png)

## 项目介绍

Meow~Detective 面向磁盘镜像、逻辑目录与 Linux/PVE 证据源的本地离线分析。后端 workspace 当前包含 29 Rust crates。案件控制信息和每个数据源的取证数据分库存储：案件级数据库负责案件、数据源注册、任务和审计；分区、文件树、制品、时间线和源内索引保存于对应数据源的 `source.db`。

当前工程事实快照：10 frontend pages、115 Tauri commands、28 source modules、migration scripts (73)、101 test files。计数由 `scripts/check-doc-drift.ps1` 与仓库结构同步校验。

## 功能简介

### 证据导入与文件浏览

- 支持 E01、RAW/dd、逻辑目录及 Linux 集群目录作为数据源入口。
- 自动识别 MBR/GPT、常见分区与文件系统；支持 NTFS、FAT、exFAT、ext4、XFS、Btrfs 以及 Linux LVM 直接逻辑卷映射。
- 文件树、目录分页、筛选、自然排序、文件属性与数据源分区信息均由后端统一提供。
- 文本、Hex、图片、音视频、PDF/Office/SQLite 等预览走受限的只读证据读取链路；大文件使用范围读取，避免一次性载入完整文件。
- 支持向调查员指定的路径提取文件，默认禁止覆盖，提取过程返回真实进度和完整性信息。
- 提供 NTFS、ext4 与 XFS 的删除文件恢复/雕刻能力，恢复范围和完整性状态会显式标注。

### Windows 取证分析

- 注册表：SYSTEM、SOFTWARE、NTUSER、SAM、SECURITY、USRCLASS、Amcache 与事务日志基础解析。
- 事件日志：EVTX 解析、开关机、登录、进程、账户与应用事件分类。
- 用户行为：Prefetch、LNK、Jump List、Recycle Bin、SRU、Thumbcache、浏览器历史/下载/会话等。
- 浏览器与凭据：Chrome、Edge、Firefox 浏览器数据提取；离线 DPAPI 相关链路按前置材料与支持边界处理。
- BitLocker：卷元数据检查、密码/恢复密码解锁、已验证密钥安全存储及重开案件恢复；匹配的 Windows x64 内存镜像可用于受限的密钥恢复与卷级验证，定位链路经内嵌的 1077 build 内核符号注册表（Windows 10 10240 → Windows 11 28000），不绑定单一 Windows 版本。

### Linux 与 PVE 分析

- Linux 取证能力：systemd journal、wtmp、bash history、apt/dpkg/yum/dnf、cron、sudo/auth、系统配置、Nginx/Apache 站点与日志、MySQL/MariaDB 配置和日志。
- Linux 数据源自动进入独立分析视图；Windows 与 Linux 提取能力不会交叉调度。
- PVE/Ceph 相关能力包括成员发现、宿主 LVM/ext4、BlueStore/BlueFS/RocksDB 元数据读取、RBD 派生虚拟磁盘与文件预览。该部分仍以私有真实样本基线为主，完整 CRUSH/EC、降级副本、通用 CephFS 重建等场景尚未承诺支持。

### 调查、关联与输出

- 全文检索、时间线、实体归并、关联图、Notebook 调查记录、规则包与批处理任务。
- HTML、CSV、JSON 与证据包报告导出；报告和错误信息遵循脱敏规则。
- MCP 扩展通道使用受控权限模型，默认最小权限和审计记录。

## 支持边界

| 范围                  | 当前状态 | 说明                                              |
| ------------------- | ---- | ----------------------------------------------- |
| Windows / Linux 数据源 | 支持   | 以 Windows 为主，Linux 文件系统与取证能力按解析器分别标注成熟度。        |
| macOS / APFS / HFS+ | 不支持  | 可识别部分分区类型，但不创建文件系统 reader。                      |
| PVE / Ceph          | 实验性  | 已覆盖私有样本中的部分 BlueStore、RBD 与派生 VM 文件树；不宣称通用集群重建。 |
| BitLocker           | 部分支持 | 仅在受支持加密方法、保护器和可验证密钥材料范围内可用。                     |
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
    EVIDENCE["只读证据源\nE01 / RAW / 目录 / 集群成员"]

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
    INPUT["证据输入"] --> PROBE["镜像与分区探测\nE01/RAW/目录"]
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
- Rust stable（仓库的 `rust-toolchain.toml` 为准）。
- Node.js LTS、Corepack 与 pnpm `10.25.0`。
- Visual Studio 2022 Build Tools，安装 Desktop development with C++ 和 Windows SDK。
- WebView2 Runtime（Tauri 桌面运行时需要）。

涉及链接的 Rust 命令应在 **x64 Native Tools Command Prompt for VS 2022** 或已执行 `vcvars64.bat` 的终端中运行，避免误用 Git 自带的 `link.exe`。

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

该命令启动 Tauri 桌面应用，并使用 Vite 前端热更新。前端页面不提供独立 mock 模式；单独运行 `pnpm --dir frontend dev` 仅适合样式和组件开发，不能替代完整的取证 IPC 运行环境。

### 前端单独检查

```powershell
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
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
cargo tauri build
```

`tauri.conf.json` 将生产前端目录固定为 `frontend/dist`，因此发布构建前必须先完成前端构建。

## 工程结构

| 路径 | 职责 |
|---|---|
| `frontend/` | React 18、TypeScript、Vite、Tailwind 4 调查界面与前端测试。 |
| `apps/desktop/src-tauri/` | Tauri shell、命令薄适配器、事件桥接和桌面运行时。 |
| `crates/transport/` | Rust DTO、请求、事件与错误契约的唯一事实源。 |
| `crates/app-services/` | 案件、导入、预览、分析、BitLocker、恢复、导出等用例编排。 |
| `crates/persistence-sqlite/` | SQLite 迁移、仓储与 source-local 数据库访问。 |
| `crates/evidence-core/`、`image-e01/` | 证据 reader、镜像探测、分区和 E01 读取。 |
| `crates/fs-*/`、`fs-lvm/` | NTFS/FAT/exFAT/ext4/XFS/Btrfs 与 LVM 文件系统能力。 |
| `crates/artifacts-windows/`、`artifacts-linux/` | Windows 和 Linux 取证制品解析。 |
| `crates/search/`、`timeline/`、`reports/` | 检索、时间线和报告能力。 |
| `crates/ceph-wire/`、`rocksdb-wire/` | 只读 Ceph/BlueStore/RocksDB 低层解析原语。 |
| `docs/` | 架构约束、支持矩阵、测试基线、错误分类和专题设计文档。 |
| `scripts/` | 架构边界、质量门禁、真实样本回归和基准脚本。 |

## 质量与安全约束

- Rust DTO 必须定义在 `crates/transport/src/dto/`，前端 TypeScript 镜像需要手工同步。
- 页面不得直接调用 Tauri `invoke`；所有请求必须经由 `frontend/src/lib/api/`。
- 原始证据只读；预览不能通过拼接宿主路径访问证据内容。
- 导出前校验目标路径，默认 `overwrite=false`，使用临时文件和原子改名避免半成品。
- 生产代码与测试物理分离；后端模块、函数、命令边界均由仓库 PowerShell guard 检查。

完整门禁、真实样本测试和文档索引见 [开发工程指南](docs/development-engineering-guide.md)、[验证可信框架](docs/validation-trust-framework.md) 与 [文档索引](docs/documentation-index.md)。

### 技术文档

- [架构与数据模型](docs/architecture-model.md)
- [后端模块架构](docs/backend-module-architecture.md)
- [设计约束](docs/design-constraints.md)
- [模型、架构与算法图谱](docs/model-architecture-algorithm-diagrams.md)
- [预期 JSON 契约](docs/expected-json-contract.md)
- [错误分类手册](docs/error-classification-manual.md)
- [性能基线](docs/benchmark-baseline.md)
- [关联分析设计](docs/correlation-analysis-design.md)
- [解析器支持矩阵](docs/parser-support-matrix.md)
- [MCP 安全模型](docs/mcp-security-model.md)
- [导出与媒体安全](docs/export-and-media-safety.md)

性能门禁的结构化事实源为 `testdata/governance/v2-benchmark-baseline.json`。

## 许可证

本项目采用 [MIT License](LICENSE)；如仓库未包含独立许可证文件，以根目录 `Cargo.toml` 中的 `license = "MIT"` 声明为准。

## 致谢与第三方版权声明

本项目在设计与实现过程中借鉴或学习了以下开源项目，特此致谢。各项目的许可证文本归其原作者所有；本项目对它们的使用仅限于设计思想参考、受控适配或许可证允许的代码复用。

| 项目 | 许可证 | 借鉴/使用方式 |
|---|---|---|
| [Autopsy](https://github.com/sleuthkit/autopsy) | Apache-2.0 | 数字取证工作台的能力分层与工作流闭环（案件 → 数据源 → 文件浏览 → 工件提取 → 检索 → 时间线 → 报告）的产品与架构思想参考，详见根目录 `autopsy-borrowings.md`；未复用其源码 |
| [The Sleuth Kit](https://github.com/sleuthkit/sleuthkit) | IPL-1.0 / CPL-1.0 | BitLocker 卷侧 `metadata -> VMK -> FVEK -> sector reader` 处理顺序与元数据冗余策略的正确性参照（见 `docs/bitlocker-memory-key-recovery-design.md`）；未复用其源码 |
| [omerbenamram/EVTX](https://github.com/omerbenamram/EVTX) | MIT OR Apache-2.0 | `crates/evtx-patched` 为其本地补丁分支（去除失维护依赖），上游许可证文本保留于 `crates/evtx-patched/LICENSE-APACHE` 与 `crates/evtx-patched/LICENSE-MIT` |
| [SecurityRonin/bitlocker-forensic](https://github.com/SecurityRonin/bitlocker-forensic)（含 [elephant-diffuser](https://github.com/SecurityRonin/elephant-diffuser)） | Apache-2.0 | `crates/volume-bitlocker` 派生自 bitlocker-core 0.3.5 与 elephant-diffuser（Albert Hui 著）。上游许可证文本保留于 `crates/volume-bitlocker/LICENSE-APACHE-2.0-UPSTREAM`，修改声明（Apache-2.0 §4(b)）与逐文件来源校验见 `crates/volume-bitlocker/NOTICE` 和 `docs/bitlocker-dependency-decision.md` |
| [shadcn/ui](https://ui.shadcn.com/) | MIT | 前端 `frontend/src/app/components/ui/` 的 UI 原语组件集（Radix Slot + cva + `cn()` 结构，已按本项目主题改写），见 `frontend/ATTRIBUTIONS.md` |
| [winbindex](https://github.com/m417z/winbindex) | GPL-3.0 | ntoskrnl 各 build 元数据索引，作为内嵌内核符号注册表的采集入口（配合微软公共符号服务器 PDB），见 `crates/memory-windows/symbols/README.md`；仅消费其公开索引数据，未复用其源码 |

此外，DPAPI / TBAL 离线恢复链路参考了公开研究（[TBAL: an (accidental?) DPAPI Backdoor for local users](https://vztekoverflow.com/2018/07/31/tbal-dpapi-backdoor/) 与 [pypykatz](https://github.com/skelsec/pypykatz) 的算法说明），仅作算法对照，未复用其代码。
