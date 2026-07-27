# Meow~Detective 架构模型

**版本**: v2.0
**日期**: 2026-07-27
**校准方式**: 以代码为基准静态实测；本文档不含未经代码核实的规划内容

---

## 📊 项目概览

```
┌─────────────────────────────────────────────────────────────────┐
│                       Meow~Detective                            │
│                       数字取证桌面应用                            │
├─────────────────────────────────────────────────────────────────┤
│  技术栈: Tauri 2 + Rust + React 18 + TypeScript + SQLite        │
│  架构: 分层 + 能力族拆分，backend-led                            │
│  代码量: 1,719 个 .rs / ~296k 行；256 个 .ts(x) / ~29k 行        │
│  测试: ~3,038 个 Rust 测试函数；86 个前端 Vitest 测试文件         │
│  workspace: 27 个 crate + 1 个 Tauri host package               │
└─────────────────────────────────────────────────────────────────┘
```

不可变边界（详见 `docs/design-constraints.md`）：desktop-first、Windows-primary、
single-user、backend-led、无 HTTP server、原始证据只读、`crates/transport`
是前后端契约唯一源头。

生产分析平台只有 Windows 与 Linux。macOS 数据源请求与旧 `platform='macos'`
案件返回 typed `Unsupported`；APFS/HFS+ 仅做分区类型元数据识别，不实例化
filesystem reader。

---

## 🏗️ 分层架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        前端层 (Frontend)                        │
│  React 18 + TypeScript + Zustand + React Query + Tailwind 4     │
│  页面不直接 invoke，统一经 src/lib/api/*                         │
├─────────────────────────────────────────────────────────────────┤
│                        命令层 (Commands)                        │
│  Tauri 2 Commands + Events，105 个 command                      │
│  只做 validate -> service -> DTO，禁止裸 SQL                     │
├─────────────────────────────────────────────────────────────────┤
│                      应用服务层 (App Services)                   │
│  跨 crate 用例编排；保持 Tauri-free                              │
├─────────────────────────────────────────────────────────────────┤
│              领域 / 契约层 (Domain / Transport)                  │
│  实体、ID、值对象 / DTO、request、event topic、error             │
├─────────────────────────────────────────────────────────────────┤
│         基础设施与解析层 (Persistence / Evidence / Parsers)       │
│  SQLite、镜像 reader、文件系统 reader、工件解析、索引、报告        │
└─────────────────────────────────────────────────────────────────┘
```

依赖方向单向向下。parser / repository / core crate 不依赖前端或 Tauri，
由 `scripts/check-stage3-command-boundary.ps1`、
`scripts/check-stage4-service-boundary.ps1`、
`scripts/check-stage5-parser-boundary.ps1` 与
`scripts/check-command-sql-boundary.ps1` 在 CI 固定。

---

## 📦 Crate 依赖图

以下为实测的内部依赖关系（`Cargo.toml` 中 workspace 内部依赖）。

```
                    ┌──────────┐        ┌───────────┐
                    │  domain  │        │ transport │
                    └────┬─────┘        └─────┬─────┘
                         │                    │
        ┌────────────────┼──────────┬─────────┴────────┐
        ▼                ▼          ▼                  ▼
┌───────────────┐ ┌──────────┐ ┌────────┐  ┌──────────────────┐
│ artifacts-core│ │  search  │ │timeline│  │persistence-sqlite│
└───────┬───────┘ └──────────┘ └────────┘  └──────────────────┘
        │                                            ▲
        ▼                                            │
┌──────────────────┐   ┌─────────┐        ┌──────────────────┐
│ artifacts-windows│   │ reports │───────▶│  infrastructure  │
│  (+evtx-patched) │   └─────────┘        └──────────────────┘
└──────────────────┘

┌───────────────┐
│ evidence-core │  证据读取抽象；RawImageReader 与 RAW/dd 读取在此
└───────┬───────┘
        │
   ┌────┼─────┬─────┬─────┬─────┬─────┬─────┬──────────┐
   ▼    ▼     ▼     ▼     ▼     ▼     ▼     ▼          ▼
fs-ntfs fs-fat fs-exfat fs-ext4 fs-xfs fs-btrfs fs-lvm image-e01

无内部依赖的独立 crate:
  ceph-wire, rocksdb-wire, containers-pst, artifacts-linux,
  runtime-cache, testing, evtx-patched
```

`app-services` 是最大的汇聚点，依赖 23 个内部 crate：domain、transport、
infrastructure、persistence-sqlite、evidence-core、六个 fs-*、fs-lvm、
image-e01、search、timeline、artifacts-core/windows/linux、reports、
containers-pst、ceph-wire、rocksdb-wire、testing。

`forensics-desktop`（Tauri host）依赖 12 个：domain、transport、
infrastructure、persistence-sqlite、evidence-core、fs-ntfs、fs-fat、
image-e01、artifacts-core、runtime-cache、app-services、mcp-client。

### 边界说明

- RAW/dd 读取由 `evidence_core::RawImageReader` 独家提供（约 20 处调用点）。历史上曾有一个 `crates/image-raw` 持有同名同职责的重复实现且零调用者，已于 2026-07-27 删除；其目录拒绝与测试用例已移植进 `evidence-core`，`SeekFrom::End` 静默钳位未移植（钳位会掩盖调用方的偏移错误，标准库的报错语义更适合证据路径）。`.dd`/`.img`/`.001`/无扩展名等 RAW 变体由 `classify_data_source_path` 以"非目录且非 E01 即 Raw"兜底识别，不依赖扩展名白名单。
- VMDK/VHD/VHDX/QCOW 当前**不能作为数据源导入**。`DataSourceKind` 无对应变体，仓库内也没有任何容器 reader；VMDK 仅在 `analysis_service::file_classification` 中按 `KDMV` 签名做文件类型识别。未来支持这类稀疏容器需要新增独立 crate，与 BitLocker 需要独立 volume 层同理。
- `runtime-cache` 只被 Tauri host 消费，且不得成为事实源。
- `evtx` 依赖指向 `crates/evtx-patched`（见 `docs/evtx-dependency-decision.md`），由 `scripts/check-evtx-dependency-decision.ps1` 固定。

---

## 🗂️ 模块职责

### 核心与契约层

| Crate | 职责 | 备注 |
|-------|------|------|
| **domain** | 领域实体、ID、时间戳、值对象 | CaseMeta、FileEntry、Artifact、TimelineEvent、Job、Report、Tag |
| **transport** | DTO、command request、event topic、error | 32 个 `src/dto/*.rs`；前后端契约唯一源头 |

### 应用与基础设施层

| Crate | 职责 | 备注 |
|-------|------|------|
| **app-services** | 跨 crate 用例编排 | 24 个能力目录 + 28 个根模块文件；模块与函数结构债务 baseline 均为 0 |
| **persistence-sqlite** | 连接、迁移、仓库 | 45 个逻辑 repository；70 个迁移脚本（主库 42 + source 27 + staging 1） |
| **infrastructure** | 日志、哈希、fs 工具、文本、时钟、配置 | 跨切面工具 |
| **runtime-cache** | 运行时临时缓存 | 只被 Tauri host 消费；不得成为事实源 |

### 证据与文件系统层

| Crate | 职责 | 支持范围 |
|-------|------|----------|
| **evidence-core** | 证据读取抽象 + RAW/dd reader | `EvidenceReader`、`FileSystemReader`、`RawImageReader`（含目录拒绝、`try_clone`、41 个 lib 测试） |
| **image-e01** | EnCase E01 reader | section/table/chunk 解压/seek |
| **fs-ntfs** | NTFS | MFT、属性、data run、INDX、压缩、路径解析 |
| **fs-fat** / **fs-exfat** | FAT12/16/32、exFAT | 基本枚举 |
| **fs-ext4** | ext4 | 64-bit、64-byte group descriptor、JBD2 journal |
| **fs-xfs** | XFS | v1/v2/v3 inode、MACB、internal log |
| **fs-btrfs** | Btrfs | reader 能力存在，公开 fixture 未补齐 |
| **fs-lvm** | Linux LVM | direct linear/striped、基础 dm-thin 只读映射 |

### 分布式存储重建层

| Crate | 职责 | 边界 |
|-------|------|------|
| **ceph-wire** | Ceph 只读 wire primitive | BlueStore label、BlueFS superblock/layout/transaction、FSMap/MDSMap/`mds_info_t` decoder、CephFS namespace wire |
| **rocksdb-wire** | RocksDB 只读重放 | CURRENT/IDENTITY/MANIFEST、VersionEdit、live-SST 单次流式、WAL/WriteBatch、有界 latest-state reduction |

这两个 crate 支撑 `app-services/src/ceph_reconstruction/`：BlueStore →
RocksDB → BlueStore `S/C/O/X` 语义快照 → OMAP → RADOS range reader →
RBD head reader → 派生 VM source DB。全部只由私有 opt-in 真实样本验证。

### 工件与功能层

| Crate | 职责 | 覆盖 |
|-------|------|------|
| **artifacts-core** | 提取框架 | Extractor / Sink 接口 |
| **artifacts-windows** | Windows 工件 | Browser、EVTX、Prefetch、LNK、JumpList、Registry、RecycleBin、SRU、Thumbcache |
| **artifacts-linux** | Linux 工件 | systemd journal、wtmp、bash history、apt/dpkg、cron、sudo |
| **containers-pst** | 邮件容器 | PST（Unicode 32/64）、OST、mbox（RFC 4155 四变体） |
| **search** | 全文索引与查询 | tantivy |
| **timeline** | 时间线投影 | MACB + 工件事件 |
| **reports** | 报告生成 | HTML、CSV、JSON、证据包 |
| **mcp-client** | MCP 客户端 | SSE、Stdio |
| **evtx-patched** | vendored EVTX parser | 作为 `evtx` 依赖消费 |
| **testing** | 共享测试工具 | 与 `testdata/` 配合 |

Registry 子模块结构：`registry/lookup/{mod,types,reader,txlog_util,utf16,system,software,ntuser,sam}`，
覆盖 SYSTEM、SOFTWARE、NTUSER、SAM 与 `.LOG1/.LOG2` 事务日志脏页合并。

---

## 📊 存储模型

存储不是单库结构，这是理解本项目数据流的关键。

```
┌──────────────────────────────────────────────────────────────┐
│  主案件库 (case.db)                                           │
│  cases, data_sources, jobs, reports, tags, audit_log,        │
│  schema_migrations, data_source_processing_phases            │
│  迁移: 0001-0042                                              │
└───────────────────────┬──────────────────────────────────────┘
                        │ 1:N
        ┌───────────────┴────────────────┐
        ▼                                ▼
┌──────────────────────┐      ┌──────────────────────┐
│ per-source DB        │      │ per-source DB        │
│ file_entries,        │      │ (每个数据源独立)      │
│ partitions,          │      │                      │
│ artifacts,           │      │ 迁移: source_001-027 │
│ timeline_events,     │      └──────────────────────┘
│ graph, source_meta   │
└──────────┬───────────┘
           │ 派生（如 RBD 重建的 VM disk）
           ▼
┌──────────────────────┐      ┌──────────────────────┐
│ 派生 source DB       │      │ staging DB           │
│ 独立文件树与全局 ID   │      │ 迁移: staging_001    │
└──────────────────────┘      └──────────────────────┘
```

### 就绪状态两层语义

`import_state=ready` **只**代表 Catalog 可浏览。真实处理进度独立记录在
`data_source_processing_phases`：

| 阶段 | 状态集 | 附加字段 |
|---|---|---|
| Catalog / Graph / Platform / Artifacts / Timeline / Search | pending / running / ready / failed / deferred | version、input fingerprint、owner、attempt、lease、heartbeat |

ready reopen 从 `source_meta` 以 O(1) 读取版本化 manifest；完整行级 digest
只由显式 deep audit 执行。父 source DB 经 reconstruction route 只读打开，
不执行 migration，不创建 WAL/SHM。

### 关键约束

- 原始证据只读；派生数据只写入 case workspace、SQLite、index 或 export 目录
- 每个分区在主库中必须且只有一个可见根节点；根折叠在导入/merge 主链路完成，不靠前端兜底
- `deleted` / `hidden` / `system` 是真实状态字段，不是前端推断
- 参数化查询；SQL 只允许出现在 repository 层或更低层

---

## 🔄 数据流

### 导入流程

```
用户选择数据源 (E01 / RAW / 逻辑目录 / PVE 集群目录)
    │
    ▼  import_precheck: 分类、校验、结构化错误
┌────────────────────┐
│ 创建后台 job + 调度  │──▶ JobRepo；跨源串行、单源有界并行
└─────────┬──────────┘     (docs/import-scheduling.md)
          ▼
┌────────────────────┐
│ 分区探测 + FS 识别   │──▶ MBR/GPT/EBR、LVM 展开、filesystem magic
└─────────┬──────────┘
          ▼
┌────────────────────┐
│ 文件系统枚举 (BFS)   │──▶ per-source DB file_entries 批量插入
└─────────┬──────────┘
          ▼
   ┌──────┼──────┬──────────┬──────────┐
   ▼      ▼      ▼          ▼          ▼
时间线投影  工件提取  文本索引   Graph 投影  (各 phase 独立记录状态、可 deferred)
   │      │      │          │
   ▼      ▼      ▼          ▼
TimelineRepo ArtifactRepo SearchIndex GraphRepo
```

### 文件预览流程

```
选中文件 → openFileHandle(FileEntryId, DataSourceKind)
              │  按数据源类型分派到统一 reader helper
              │  禁止 case_root.join(entry.path) 拼宿主路径
              ▼
        MIME / 扩展名 / magic 探测
              │
   ┌──────┬──────┬───────┬───────┬─────────────┐
   ▼      ▼      ▼       ▼       ▼             ▼
 Text    Hex   Image   Audio   Video   Document/Table
                                        (PDF/Office/SQLite/xlsx)
```

预览约束：

- Text 与 hex range 走真实 reader；`get_image_preview` 返回有大小上限的 data URL
- `get_media_url` 对小媒体返回 bounded data URL；对大媒体返回 `mode=protocol` + opaque `handleId` + `evidence-media://handle/<encoded>`
- Tauri `evidence-media` protocol handler 用同一 reader helper 按 Range 读取，单次最多 1MB，返回 206/416 等 bounded response
- 不暴露宿主绝对路径；CSP 只在 `media-src` 允许 `evidence-media:`
- `scripts/check-media-protocol-guard.ps1` 固定协议注册、CSP、fallback command 与禁止 host-path asset URL 回退
- 派生源（RBD VM）预览额外有 source-scoped runtime、opaque preview session、scope generation、retire/read-drain 与请求内 256 KiB 页合并（见 `docs/ceph-rbd-vm-preview-performance-design.md`）

---

## 🔗 契约机制

`crates/transport/` 是唯一契约源，**无 codegen**，两侧手工同步。

| 层级 | 位置 | 约定 |
|---|---|---|
| DTO | `crates/transport/src/dto/*.rs`（32 个文件） | `#[serde(rename_all = "camelCase")]`；可选字段用 `skip_serializing_if` |
| command request | `crates/transport/src/commands/mod.rs` | 请求结构集中定义 |
| event topic | `crates/transport/src/events/mod.rs` | `EventTopic` 枚举 |
| error | `crates/transport/src/errors/` | 跨 crate 共享 `ApiErrorDto` |
| 前端镜像 | `frontend/src/types/models.ts` | 接口去掉 `Dto` 后缀（`TimelineEventDto` 例外）；`EventTopic` union 手工同步 |

Tauri command 返回 `Result<T, String>`。后端 event bridge 用
`emit_to("main", ...)` 定向发送，payload 不含证据宿主绝对路径。

两个漂移守卫：

- `scripts/check-dto-drift.ps1` — 按名配对 `FooDto` ↔ `Foo` 比对字段名；只有已配对类型内部字段不一致才失败，未配对只作提示
- `scripts/check-event-topic-drift.ps1` — Rust 常量与 TS union 一致性

### 新增功能的契约流

```
transport DTO/request
  → app-services/src/<domain>_service*
  → apps/desktop/src-tauri/src/commands/<domain>_commands*
  → 在 src/lib.rs invoke_handler 注册
  → frontend/src/types/models.ts 镜像
  → frontend/src/lib/api/<domain>.ts
  → frontend/src/features/<domain>/hooks.ts
  → 页面/组件
```

### 预留契约面

`crates/transport/src/dto/android.rs` 是**预留**契约面：DTO 形状与说明在
文件头注释中，但无 parser crate、无 command、无 TS 镜像，Android 数据源仍为
typed `Unsupported`。该模块应保持零消费者，直到 Android parser 落地。

对应的 iOS 与云审计日志 DTO 已随 crate 退役一并删除，不再保留契约面。

---

## 🧩 前端架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         AppShell / Layout                       │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ TopBar: 品牌 + 导航 + 案件上下文 + 全局搜索 + 任务状态     │   │
│  ├────────────────────────────┬─────────────────────────────┤   │
│  │ PageSubbar + 页面工作区     │      InspectorPane          │   │
│  │ (表格 / 分栏 / viewer)      │      (详情字段组)            │   │
│  ├────────────────────────────┴─────────────────────────────┤   │
│  │ BottomDrawer: 任务 / 警告 / trace                        │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

10 个路由页面（`frontend/src/app/routes.tsx`，全部 lazy）：CaseHome、
DataAnalysis、V2Workbench（仅 dev/audit 模式）、V3Dashboard、FileBrowser、
Search、Timeline、Artifacts、Reports、Settings。

### 目录职责

| 路径 | 职责 |
|---|---|
| `src/app/pages/` | 路由级页面入口（10 个） |
| `src/app/components/ui/` | shadcn/radix UI 基元（24 个） |
| `src/components/layout/` | AppShell、Layout、TopBar、PageSubbar、InspectorPane、BottomDrawer、CollapsibleSection、HorizontalScroll |
| `src/components/` | 公共域组件：`tables/`（DenseDataTable）、`tree/`、`viewers/`（Hex/Text/Image/Audio/Video）、`preview/`（FilePreviewDialog/Tabs）、`data-display/`、`status/`、`tabs/`、`brand/` |
| `src/features/*/` | 17 个 feature：hooks + 领域组件 |
| `src/lib/api/` | API 包装，统一经 `apiClient.request(...)` |
| `src/stores/` | Zustand：ui、selection、analysis、mcp（含 server/resource/error 拆分） |
| `src/lib/events/bus.ts` | `EventBus` 事件订阅 |

### 状态与样式

- React Query 是默认服务端状态层（`src/app/providers.tsx`）：staleTime 30s，窗口聚焦不重取；hooks 用 active-case enabled gate，无活动案件时不发 IPC
- Zustand 只持有本地 UI / 选择 / 分析 / MCP 状态
- Tailwind 4 CSS-first：无 `tailwind.config.js`，配置写在 CSS 中并用 `@source` 声明扫描范围；`@/` 别名映射 `frontend/src/`
- 视觉方向见 `frontend-ui-ux.md`（Anime Detective Archive）

### 无 mock 模式

前端始终调用真实 Tauri command。历史 mock provider 已移除，`pnpm dev`
需要 Tauri 环境，完整开发循环用 `cargo tauri dev`。
`scripts/check-frontend-runtime-guard.ps1` 固定这条边界。

Settings 持久化分两类：路径类设置经 `get_app_settings` /
`save_app_settings` 进入后端校验并写配置文件；theme 与 dev event trace
同时写 `localStorage` 以即时生效。

feature 边界规则见 `docs/frontend-mvp-boundary.md`。前端 feature 必须有页面
或上层消费者；`features/gql` 与 `features/marketplace` 曾因后端 crate 退役而
成为无消费者空壳，已随本轮清理移除。

---

## 🔌 MCP 集成

```
Frontend (useMcpStore)  ◀──▶  Tauri commands  ◀──▶  mcp-client (Rust)
                                                         │
                                              ┌──────────┴──────────┐
                                              ▼                     ▼
                                          SSE/HTTP              Stdio
                                              │                     │
                                              └────────┬────────────┘
                                                       ▼
                                                  MCP Servers
```

MCP 是受控扩展通道，不是任意执行后门。默认最小权限：
`resourceAccess=readOnly`、`toolAccess=disabled`、`promptAccess=readOnly`、
`networkPolicy=localhostOnly`。SSE 仅允许 `http/https` 且禁止内嵌凭据；
stdio command 只能是可执行名、不能是路径。关键动作必须审计。
详见 `docs/mcp-security-model.md`；transport 契约由
`scripts/check-stage5-regression-guard.ps1` 固定。

---

## 🔒 安全架构

| 层次 | 措施 |
|---|---|
| 输入验证 | 路径遍历防护、URL 编码检测、null 字节检测、Windows 保留名检测 |
| 数据库 | 参数化查询、外键约束、审计日志；command 层禁止裸 SQL |
| 文件系统 | symlink 检测、路径规范化、导出路径边界校验、`overwrite` 默认 `false` |
| 媒体与导出 | 短期可失效 handle、range 校验、不暴露宿主路径 |
| 解析 | 损坏/截断/未知输入返回 error 或 warning，不 panic；不补默认取证文案 |
| 溯源 | 保留 data source id、file entry id、parser/extractor 与版本、offset/path/record id、confidence、warnings、parse status |

---

## 🧪 测试与门禁

最低 gate：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
git diff --check
powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1
```

Windows-first 约束：任何需要链接的 cargo 命令（build/test/check/带
`--all-targets` 的 clippy）必须在 VS 2022 开发环境中运行，否则 bash 的
`PATH` 会把 `link.exe` 解析到 Git 的副本，或找不到
`kernel32.lib`/`ntdll.lib`。`cargo fmt` 不链接，可在普通 shell 运行。

### 守卫脚本体系（33 个）

结构与架构边界是本仓库的主治理机制，由 CSV baseline + 守卫脚本共同固定，
baseline 只允许减少：

| 类别 | 脚本 |
|---|---|
| 结构债务 | `check-module-size`、`check-rust-function-size`、`check-rust-test-layout`、`check-dead-code-allow-guard` |
| 分层边界 | `check-command-sql-boundary`、`check-stage0-boundary-guard`、`check-stage2-platform-boundary`、`check-stage3-command-boundary`、`check-stage4-service-boundary`、`check-stage5-parser-boundary`、`check-stage6-test-separation` |
| 契约漂移 | `check-dto-drift`、`check-event-topic-drift`、`check-doc-drift`、`check-doc-archive` |
| 证据与安全 | `check-media-protocol-guard`、`check-release-guard`、`check-lvm-offset-guard` |
| 依赖治理 | `check-deny-exceptions`、`check-dependency-security`、`check-evtx-dependency-decision`、`check-frontend-lockfile-policy` |
| 前端 | `check-frontend-runtime-guard` |
| 回归与性能 | `check-stage5-regression-guard`、`check-import-optimization-guard`、`check-benchmark-regression`、`check-e01-import-performance`、`check-pve-cluster-import`、`check-pve-rbd-preview-performance`、`check-private-real-sample-tests` |

辅助脚本：`run-benchmark`、`run-coverage`、`run-e01-import-profile`、
`run-liuyang-artifact-test`、`run-webview2-media-smoke`、
`generate-tiny-fixtures`、`generate_medium_email_fixtures.py`。

当前结构债务：模块 baseline 0 行、正式临时例外 4 行、函数 baseline 8 行
（历史硬债务 0，新代码函数硬上限 150 行）、test-layout baseline 0 行；
`app-services` 模块与函数 baseline 均为 0。

### Fixture 分层

| Fixture | 用途 |
|---|---|
| `testdata/fixtures/public-small/logical/` | 默认 CI 逻辑目录 |
| `public-small/raw/tiny.raw` | 1024 B deterministic RAW，含 MBR signature |
| `public-small/e01/tiny.E01` | 4405 B synthetic 单段 E01；覆盖 reader section/table/read/seek，不是完整文件系统镜像 |
| `public-small/logical/Windows/System32/config/{SYSTEM,SOFTWARE}` | deterministic synthetic registry hive；不是完整 registry corpus |
| `public-small/evtx/system.evtx` | 1,118,208 B 真实 System.evtx；provenance 在同目录 README |
| `public-small/email/` + `public-medium/email/` | EML/EMLX/MBOX/PST/OST |

真实 E01 验收通过 `FORENSICS_E01_FIXTURE`、`FORENSICS_LINUX_E01_FIXTURE`、
`FORENSICS_PVE_CLUSTER_ROOT` 等 opt-in ignored slow test 执行，默认 CI 不依赖
私有样本。详见 `docs/fixture-handbook.md`、
`docs/validation-trust-framework.md`。

---

## 📈 复杂度与实测性能

复杂度是静态分析；实测数字来自私有真实样本的特定提交，不是跨格式/跨机器的
性能承诺。

| 操作 | 复杂度 | 说明 |
|---|---|---|
| 文件枚举 | O(n) | BFS + 批量插入；真实速度取决于镜像格式与 SQLite 写入 |
| 文件排序 | O(n log n) | 后端为排序主入口，前端预计算排序键 |
| SHA-256 | O(n) | 流式 reader |
| Hex 格式化 | O(n) | 只对已读取 range |
| Magic 分类 | O(s·h) | s = 样本数，h = bounded header（当前 8KB） |
| 路径重建 | O(n) | 递归 + 缓存 + cycle detection |
| MFT 扫描 | O(n) | 多线程降 wall time，但 I/O 与 DB writer 仍可能成瓶颈 |
| 搜索查询 | 依赖 tantivy | 需按实际索引规模 benchmark |

### 已记录实测基线

| 场景 | 结果 |
|---|---|
| 检材2 三次导入 | total median `13.479s`、enumeration median `8.488s`、RSS `582MB`、每次 `91,737` rows |
| Windows/Linux 双源双顺序串行 | `96.92s` / `94.63s` |
| PVE 六成员完整串行复跑 | `ready=6`、`failed=0`；内部 `712.968s`，进程墙钟 `805.22s` |
| RBD 派生 VM 首次物化 | `46.28s` / `54.73s`（历史旧路径 `>1h47min` 仅产生约 6,828 行） |
| RBD 幂等物化 | `124ms` / `136ms` |
| RBD 预览（提交 `db49698a`） | cold file read `349.992ms`；warm 64 KiB p95 `0.699ms`；`4x1 MiB` p95 `234.152ms`；614 MiB 文件随机 64 KiB p95 `73.804ms` |
| 零派生源 Catalog 重建 | `83.574s`（4,000 行 / 16MiB 双上限 + 64MiB WAL checkpoint） |
| Artifacts 冷重放 | `130.62s / 715MiB`（8MiB read-plan cache、关闭 mmap） |

剩余性能债务：首次三 OSD E01/LVM runtime 初始化（中位约 `3.186s`）、
浏览器端 media 时序、容量 LRU eviction、Catalog 持久化 frontier/cursor。

---

## 📊 代码统计

| 类别 | 数量 |
|---|---|
| Rust 源文件 / 行 | 1,719 / ~296,000 |
| TypeScript 源文件 / 行（不含测试） | 256 / ~28,900 |
| workspace 成员 | 28（27 crate + Tauri host package） |
| Tauri commands / 命令文件 | 105 / 68 |
| transport DTO 文件 | 32 |
| SQLite 逻辑 repository / 迁移脚本 | 45 / 70 |
| 前端页面 / feature / UI 基元 | 10 / 17 / 24 |
| Rust 测试函数 | ~3,038 |
| 前端测试文件 | 86 |
| 守卫脚本 | 33 |
| Mermaid 图块（`docs/model-architecture-algorithm-diagrams.md`） | 15 |

---

## 🧭 当前开发方向

V2/V3/V4/V5 阶段计划均已转为历史设计记录。V5 五根柱子中只有"高级文件系统
取证"部分落地（NTFS/ext4/XFS 已删除文件恢复 + header/footer carving）；移动
（iOS/Android）、云审计日志、GQL、生产化部署与规则包市场四根柱子已随
`a3c1f265` 的 8 个无消费者 crate 退役而终止。

2026-07 的实际方向是**深度而非广度**：

- 分析链路加固 — 离线 LSA secret 恢复、Chrome App-Bound v20 分链解密（flag `0x01`/`0x02`/`0x03`；Edge 及其他 Chromium 走双 DPAPI）、EVTX 不信任 header chunk count、browser preload hive locator 容错
- 预览深度 — PDF/Office/SQLite/xlsx 文档渲染、完整 media MIME map、dialog 化预览
- 分类能力 — 两级文件分类板（magic detection + 可折叠分组）
- 前端性能 — 大列表渲染 jank、文件树遍历 cycle guard
- 数据库 — 停止 per-open migration 写入，浏览路径改只读连接

下一个明示技术边界（`docs/progress-ledger.md`）：CephFS 需要真实 fresh
FSMap/MDSMap、`EMetaBlob` mutation/dirfrag/backtrace 与真实 layout byte
oracle；生产 cluster runner 目前只执行 presence assessment，尚无
materialization 调用方。

### 权威文档路由

| 主题 | 文档 |
|---|---|
| 已验证进度与真实样本基线 | `docs/progress-ledger.md` |
| 支持边界与字段承诺 | `docs/parser-support-matrix.md`、`docs/known-unsupported-formats.md` |
| 硬约束 | `docs/design-constraints.md` |
| 文档入口与事实快照 | `docs/documentation-index.md` |
| 前端边界 | `docs/frontend-mvp-boundary.md` |
| 后端模块规则与 baseline | `docs/backend-module-architecture.md` |

---

**维护要求**: 本文档以代码为基准。任何 crate、command、DTO、页面、feature 或
守卫脚本数量变化，必须同步更新本文档、`README.md` 与
`docs/documentation-index.md`，并以
`powershell -File scripts/check-doc-drift.ps1` 验证后两者。
