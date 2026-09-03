# Meow~Detective 插件开发契约

## 1. 状态与范围

本文档是 Meow~Detective DLL 解析器插件的公开开发契约，当前固定为 **ABI v1**。
它约束插件作者、宿主 `app-services/plugin_loader`、构建脚本和发布包之间的边界。

- 宿主运行平台始终是 Windows x64；`Windows`/`Linux` 表示被分析证据的平台，不表示
  插件 DLL 可以在对应操作系统上直接运行。
- 插件在宿主进程内运行，继承宿主的管理员令牌和地址空间。ABI 检查不是沙箱；只应
  加载经过组织审查、签名或哈希白名单确认的可信 DLL。
- 插件源工程位于独立的 `plugins-src/`，不加入主 Rust workspace，也不随公共应用源码
  发布。发布物是 exe 旁 `plugins/` 目录中的 DLL；本契约和 `crates/plugin-api` 是公开
  接口。
- 原始证据由宿主以只读、受限字节缓冲提供。插件不能修改证据、案件数据库或宿主文件；
  需要副作用的动作必须单独审查并在动作描述中披露。

插件只负责解析和返回结构化结果。候选发现、source DB 写入、时间线/关联投影、错误
脱敏和 provenance 归属由宿主完成。

## 2. 工程与依赖边界

插件必须以 `cdylib` 构建，并只依赖：

1. `plugin-api`（唯一允许依赖的本项目 ABI crate）；
2. 插件自身的纯解析依赖。

插件不得依赖 `app-services`、Tauri、前端或任何带宿主全局 allocator 的应用 crate。
这样可以避免 mimalloc/系统堆跨 DLL 交叉释放，也避免把宿主生命周期带入插件。

推荐布局：

```text
plugins-src/
  Cargo.toml                 # 独立 workspace，不加入根 Cargo.toml
  <plugin-name>/
    Cargo.toml               # crate-type = ["cdylib"]
    src/lib.rs
```

插件 workspace 的 `panic` profile 必须保持 `unwind`，但所有导出函数仍必须在 DLL 内部
自捕获 panic。不得用 `panic = "abort"` 代替导出层保护。

## 3. ABI v1 导出符号

跨 DLL 只允许 `extern "C"`、`#[repr(C)]` 类型、原始指针和显式长度。禁止传递 `String`、
`Vec`、`Box`、trait object、未锁定布局的 Rust enum，或任何由一侧分配、另一侧直接析构
的 Rust 堆对象。

### 3.1 必需符号

| 符号 | C ABI 签名 | 契约 |
|---|---|---|
| `meow_plugin_info` | `unsafe extern "C" fn() -> MeowPluginInfo` | 返回静态插件元信息；宿主首先调用并完成 ABI 握手 |
| `meow_plugin_extract` | `unsafe extern "C" fn(*const MeowExtractRequest) -> MeowExtractResponse` | 处理一次宿主请求；不得保存任何请求指针 |
| `meow_plugin_free_buffer` | `unsafe extern "C" fn(*mut u8, u64)` | 释放该 DLL 分配的 payload 或 error buffer |

### 3.2 ABI 类型和布局

类型定义位于 `crates/plugin-api/src/types.rs`，字段顺序、大小和 enum discriminant 由
`crates/plugin-api/tests/abi_layout.rs` 锁定。每个结构体的 `struct_size` 必须填写插件
编译时的 `size_of::<T>()`；宿主拒绝小于当前最低布局的值。

`MeowPluginInfo` 包含：

- `abi_version`：必须等于 `MEOW_PLUGIN_ABI_VERSION`（当前为 `1`）；
- `plugin_id`：非空、稳定且全局唯一，例如 `meow.plugin.prefetch`；重复 ID 时宿主按
  确定性加载顺序只保留第一个；
- `plugin_version`：非空 SemVer，宿主写入 artifact 的 `extractor_version`；
- `display_name`：面向调查员的名称；
- `evidence_platform`：`Windows=0` 或 `Linux=1`；
- `families_json`：非空 JSON 字符串数组，声明插件允许产生的 artifact family；
- `path_patterns_json`：JSON 字符串数组，声明候选路径匹配规则。

所有字符串指针都是 DLL 生命周期内有效的 NUL 结尾 UTF-8。宿主会立即复制并校验，
插件不能返回临时栈缓冲或可变全局字符串。

`MeowExtractRequest` 由宿主拥有：

- `file_path` 是逻辑证据路径，可能带 `[P{n}]` 分区前缀，只用于匹配和 provenance；
- `file_id` 是 `ds:<dataSourceId>:<localId>` 形式的 FileEntryId；
- `data/data_len` 是宿主按分析上限提供的只读主文件缓冲；
- `companions/companion_count` 是可选的同目录伴随文件数组，例如 SQLite `.db-wal`。

请求指针、伴随数组指针和其中所有数据指针只在本次调用期间有效。插件必须在返回前
完成复制或解析；不得把指针交给后台线程、缓存或全局状态。

`MeowExtractResponse` 的 `payload` 和 `error_message` 由插件分配：

- `payload` 是长度定界的 UTF-8 JSON，长度由 `payload_len` 给出；
- `error_message` 是可选的 NUL 结尾 UTF-8 字符串，不包含宿主绝对路径、凭据或完整外部
  stderr；
- 宿主读取后必须调用同一 DLL 的 `meow_plugin_free_buffer`，不能使用宿主 allocator。

## 4. 元数据、路径匹配与输出

`path_patterns_json` 由宿主按不区分大小写的方式解释：

- `*.pf`：后缀匹配；
- 含 `/` 或 `\\` 的字符串：规范化路径片段匹配；
- 其他字符串：精确文件名匹配。

`families_json` 是输出白名单。每条 artifact 的 `family` 不在声明集合中时由宿主丢弃
并记录 warning，不能通过插件返回值绕过 family 路由。

成功 payload 的最小形状：

```json
{
  "artifacts": [
    {
      "family": "Prefetch",
      "title": "CMD.EXE",
      "summary": "run_count=3",
      "confidence": 0.85,
      "attrs": { "runCount": 3 }
    }
  ],
  "timelineEvents": [
    {
      "timestampUtc": "2026-01-02T03:04:05Z",
      "eventType": "Execution",
      "description": "CMD.EXE executed",
      "attrs": {}
    }
  ],
  "warnings": []
}
```

输出要求：

- `attrs` 使用 camelCase；值可以是任意有界 JSON；
- `timestampUtc` 默认必须是带时区的 RFC 3339 UTC 时间；无法可靠解析时返回 warning，
  不伪造时间；
- 只有插件明确声明 `timesAreLocal=true` 时，宿主才按已解析的主机时区转换无时区墙钟
  时间；
- 空值必须区分“不存在”“未解析”和“不支持”，与 `docs/expected-json-contract.md`
  一致；
- 插件不得在 payload 中自报可信 provenance。

宿主始终覆写 `source_object_id`、`source_attribution`、`extractor_id`、
`extractor_version`。source object 固定为请求的 FileEntryId，extractor identity 来自
已握手的插件 ID/版本；插件返回的伪造值不会改变归属。

`MeowStatus` 的含义：

| 状态 | 适用情况 | 宿主处理 |
|---|---|---|
| `Ok` | payload 合法 | 校验 JSON、family、时间和 provenance 后写入统一 sink |
| `ParseError` | 输入损坏、截断或未知版本，无法可靠解析 | 归类为 parser，跳过该候选，不中断其他 extractor |
| `Unsupported` | 已识别目标但该内容变体不在插件范围 | 归类为 unsupported，并保留可操作 warning |
| `InternalError` | 插件内部失败或被捕获的 panic | 归类为 external/internal，跳过该候选 |

## 5. 内存所有权与 panic 硬契约

### 5.1 谁分配谁释放

插件返回的每个非空 buffer 都必须由插件分配，并由插件的 `meow_plugin_free_buffer`
释放。推荐使用“长度等于容量”的 owned `Vec<u8>` 转移；释放时使用与该长度匹配的
`Vec::from_raw_parts(ptr, len, len)`。空指针、零长度、重复释放和跨 DLL allocator 释放
均属于插件 bug。`error_message` 的 NUL 终止符也属于插件分配的内存。

### 5.2 导出函数必须自捕获 panic

MSVC 下 foreign exception 不能可靠地被宿主侧 `catch_unwind` 拦截；panic unwind 越过
FFI 边界可能直接终止宿主进程。因此 `meow_plugin_extract` 和可选的
`meow_plugin_action` 必须使用 `plugin_api::guarded_extract` / `guarded_action`，在 DLL
内部把 panic 转为 `InternalError`。`meow_plugin_info` 也不得 panic。

宿主侧的 `catch_unwind` 只是纵深防御，不能替代插件自捕获。插件测试必须包含故意 panic
分支，并确认进程不因跨边界 unwind 退出。

## 6. 可选动作通道

插件可以额外导出：

```text
meow_plugin_action(*const u8, u64) -> MeowExtractResponse
```

它接收长度定界的 UTF-8 JSON：`{"action":"<id>","params":{...}}`，返回复用
`MeowExtractResponse` 的 JSON。动作符号缺失表示插件没有动作，宿主优雅降级，不拒载；
导出动作不改变 ABI v1 版本。

动作通道要求：

- 必须支持 `describe`，返回 `actions[]`，每项包含 `id`、`label`、`description` 和
  `inputKind`（`file` 或 `none`）；
- 每个动作入口同样使用 `guarded_action`；
- 宿主按插件维度串行调用，动作不能依赖未保护的全局可变状态；
- `description` 必须说明输入、输出、是否读取大文件以及任何副作用；
- 动作结果可能含敏感数据，调用方负责后续脱敏、ACL 和审计，插件不得写日志；
- 动作不是候选发现或普通 artifact extraction 的旁路权限，不得借此修改原始证据。

## 7. 宿主加载与运行生命周期

宿主从运行中 exe 的同级目录扫描：

```text
<exe-dir>/plugins/windows/*.dll
<exe-dir>/plugins/linux/*.dll
```

`app-settings.json` 可通过 `plugins.enabled=false` 整体禁用，或用 `plugins.dir` 指定
插件根目录；缺少配置、目录或目录项都按空插件集处理。宿主按 DLL 路径确定性排序，
以绝对路径和受限 Windows DLL 搜索标志加载，然后依次：

1. 解析 `meow_plugin_info`；
2. 校验 `struct_size`、ABI 版本、字符串、family/path JSON 和 evidence platform；
3. 解析三个必需符号，按存在性探测可选 action；
4. 对重复 plugin ID 拒载，其余插件继续加载；
5. 将每个插件包装成 `ArtifactExtractor`，按插件维度 Mutex 串行调用；
6. 单个插件加载、解析或提取失败只记 rejection/warning，不中断其他插件和内置解析器。

插件 DLL 的生命周期通常覆盖宿主进程；插件不能依赖卸载回调来提交案件数据。宿主在
非 Windows 目标上返回空插件集，以保持跨平台 crate 图可编译，但不会宣称 Linux 主机
可以直接加载这些 Windows DLL。

## 8. 构建、分发与平台登记

统一构建入口：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-plugins.ps1
```

脚本使用 `plugins-src/Cargo.toml` 的独立 workspace 做 release 构建，并按插件元信息中
的 `evidence_platform` 将 DLL staged 到：

```text
target/release/plugins/windows/
target/release/plugins/linux/
```

若 Tauri release 目录已存在，也同步复制到其 exe 旁的 `plugins/`。构建脚本中的插件
清单、`meow_plugin_info.evidence_platform`、支持矩阵和发布包目录必须一致。绿色软件
发布将 `plugins/` 与 exe 一起分发；一期不把插件 DLL 嵌入 NSIS bundle，也不自动安装
依赖项。

新增插件时必须登记：

- 稳定 `plugin_id`、SemVer 和 display name；
- evidence platform、families 和 path patterns；
- 纯解析依赖及许可证；
- fixture/expected JSON、宿主回归入口和支持等级；
- DLL 签名或发布哈希（由组织发布流程维护，不由 ABI 自动保证）。

## 9. 测试与验收清单

插件自身至少覆盖：

- ABI 版本、元信息、字符串 NUL 终止和声明集合校验；
- 正常 payload、空输入、截断输入、非法 UTF-8/JSON、未知 family；
- `ParseError`、`Unsupported`、`InternalError` 三类状态；
- panic 自捕获、payload/error buffer 的分配与释放、重复调用和确定性输出；
- companion 文件存在、缺失、长度不一致和恶意内容；
- 时间字段 UTC/本地墙钟规则，以及 warning 不伪造事实。

宿主集成至少证明：

- 真 DLL 能通过 `struct_size`/ABI 握手并出现在插件模块列表；
- 路径模式只命中声明的候选，evidence platform 和 family 不串用；
- plugin artifact 的四个 provenance 字段由宿主覆写；
- 插件失败不会阻断内置 extractor 或同批其他插件；
- 删除 DLL 后内置解析器仍可工作；
- `plugins.enabled=false` 和缺目录路径均安全降级；
- 动作通道缺失、`describe`、非法动作和动作 panic 都有明确结果；
- 宿主日志、审计、报告和 UI 不出现绝对证据路径、凭据、token 或 raw payload。

发布前同步更新：

- `docs/parser-support-matrix.md` 的对应 family 与插件形态备注；
- `testdata/governance/v2-verification-catalog.json` 的验证链路（若该插件进入治理）；
- `docs/expected-json-contract.md` 的 guaranteed/bestEffort/notGuaranteed 字段；
- `docs/known-unsupported-formats.md` 的格式缺口；
- 本文档、构建脚本清单和插件许可证/发布哈希记录。

## 10. 版本演进

- 仅新增 JSON payload 字段：ABI 版本不变，旧宿主可忽略未知字段；
- 新增可选导出符号：按符号存在性探测，ABI 版本不变；
- `struct_size` 尾部新增字段：新宿主按结构长度读取，旧插件保持旧长度；
- 改变既有字段布局、enum discriminant、指针所有权或语义：`MEOW_PLUGIN_ABI_VERSION`
  必须递增，且同时更新 `plugin-api` 布局测试和本文档；
- 每个插件版本必须可追溯到构建提交、依赖清单和发布哈希；解析结果的
  `extractor_version` 不得复用宿主应用版本。

## 11. 明确不承诺

ABI v1 不提供：

- 宿主侧 seek/range-read 回调；输入受宿主 artifact 内容上限约束；
- 第三方插件沙箱、权限降级、独立进程隔离或恶意 DLL 防护；
- 自动修复原始证据、写回案件源文件或隐式访问任意宿主路径；
- 通过插件 ABI 自动获得 BitLocker 密钥、用户密码或其他凭据；
- 浏览器/邮件/Linux 等静态路由自动变成插件接口；每个 family 仍需单独登记和验收。

当插件需要超出这些边界的能力时，应先提出 ABI v2 设计和安全评审，不得私自扩展
结构体、复用未声明字段或把动作通道当作隐藏的宿主执行接口。
