# 方案细化

## 方案一：Registry/EVTX 解析器补全

### 1.1 现状分析

**Registry（`crates/artifacts-windows/src/registry/parser.rs`）**

已有能力：
- 解析 `regf` base block → 提取 `last_written` FILETIME
- 读取 root NK cell 的 key name（UTF-16LE 解码）
- 作为 `ArtifactExtractor` 注册，产出 artifact + timeline event

缺失能力：
- **无法遍历子 key 树**：没有 hbin 寻址、cell 解析、subkey list（lf/lh/ri）遍历
- **无法读取 value entry**：没有 VK record 解析、data 类型判断（REG_SZ/REG_DWORD/REG_BINARY）
- `supports_path` 只匹配 `.dat` 后缀，**SYSTEM/SOFTWARE hive 文件无扩展名**，当前不会被选中

**EVTX（零实现）**

- `analysis_service.rs` 中 `inspect_evtx_boot_source()` 是纯 stub
- 产出 `unavailable` 状态 + "未提供 EVTX parser" 警告
- crates.io 上有成熟 crate：`evtx = "0.11.2"`，支持 `from_path`/`from_buffer`，record 可序列化为 JSON/XML

### 1.2 Registry 实现方案

#### 1.2.1 数据结构（新增于 `parser.rs`）

```
BaseBlock (已有)
  ├── root_cell_offset = 0x1000 + root_cell_offset_field
  │
  ▼
HbinHeader (新增)
  offset: root_cell_offset 按 0x1000 对齐
  fields:
    signature: [u8; 4] = "hbin"
    offset_from_first: u32
    size: u32          (本 hbin 块大小，始终是 0x1000 的倍数)
    reserved: [u8; 8]
    timestamp: u64     (FILETIME)
    unknown: u32

Cell (新增)
  每个 hbin 内以 8 字节对齐的 cell 链
  header:
    size: i32          (负=已分配, 正=空闲)
  根据 signature 分派：
    b"nk" → NkRecord
    b"vk" → VkRecord (只在读取 value 时按偏移跳转)
    b"lf"/b"lh"/b"li"/b"ri" → SubkeyList
    b"sk" → SecurityRecord (跳过)
    b"db" → BigData (跳过)

NkRecord (扩展现有 read_hive_name 为独立结构)
  偏移 0x00: cell_size (i32, 已在外部读取)
  偏移 0x04: signature [u8; 2] = "nk"
  偏移 0x06: flags (u16)
      bit 0: KEY_HIVE_ENTRY (root key)
      bit 1: KEY_NO_DELETE
      bit 2: KEY_SYM_LINK
      bit 3: KEY_COMP_NAME (name is compressed ASCII)
      bit 4: KEY_PREDEF_HANDLE
  偏移 0x08: last_written (u64 FILETIME)
  偏移 0x10: access_bits (u32)
  偏移 0x14: parent_offset (u32) → hive file offset
  偏移 0x18: num_subkeys (u32)
  偏移 0x1C: num_volatile_subkeys (u32)
  偏移 0x20: subkeys_list_offset (u32) → hive file offset
  偏移 0x24: volatile_subkeys_list_offset (u32)
  偏移 0x28: num_values (u32)
  偏移 0x2C: values_list_offset (u32) → hive file offset
  偏移 0x30: security_offset (u32)
  偏移 0x34: classname_offset (u32)
  偏移 0x38: max_subkey_name_len (u32)
  偏移 0x3C: max_subkey_class_len (u32)
  偏移 0x40: max_value_name_len (u32)
  偏移 0x44: max_value_data_len (u32)
  偏移 0x48: workvar (u32)
  偏移 0x4C: name_len (u16)
  偏移 0x4E: classname_len (u16)
  偏移 0x50: name (变长)
  注：cell_size 在 signature 之前 4 字节，所以 signature 从 cell 起始+4

VkRecord
  偏移 0x00: cell_size (i32)
  偏移 0x04: signature [u8; 2] = "vk"
  偏移 0x06: name_len (u16)
  偏移 0x08: data_len (u32)
      bit 31 set → data_inline（data_offset 字段内嵌数据，长度 = data_len & 0x7FFFFFFF）
      bit 31 clear → data_offset 是 hive 文件偏移
  偏移 0x0C: data_offset (u32)
  偏移 0x10: data_type (u32)
      1=REG_SZ, 2=REG_EXPAND_SZ, 3=REG_BINARY, 4=REG_DWORD, 7=REG_MULTI_SZ, 11=REG_QWORD
  偏移 0x14: flags (u16)
      bit 0: VALUE_COMP_NAME (name is ASCII)
  偏移 0x16: unknown (u16)
  偏移 0x18: name (变长)

SubkeyList（三种格式）
  lf 格式 (leaf fast):
    signature: "lf"
    count: u16
    entries[count]: { hash: [u8; 4], named_key_offset: u32 }
  lh 格式 (leaf hash):
    signature: "lh"
    count: u16
    entries[count]: { hash: u32, named_key_offset: u32 }
  ri 格式 (root index → 间接索引):
    signature: "ri"
    count: u16
    entries[count]: { subkey_list_offset: u32 }  → 指向另一个 lf/lh
  li 格式 (leaf index, rare):
    signature: "li"
    count: u16
    entries[count]: { named_key_offset: u16 或 u32 }
```

#### 1.2.2 函数设计

```rust
// === 底层 hive 寻址 ===

/// 从 hive 文件偏移读取 cell（偏移 = hbin 数据区起始 + cell 在 hbin 内偏移）
/// hive 文件布局：0x000 base_block(4096) | 0x1000 hbin_0 | 0x2000 hbin_1 | ...
fn read_cell_at(reader, hive_file_offset: u32) -> Result<Cell>

/// 读取 hbin header，返回 (data_start_offset, size)
fn read_hbin_header(reader, hbin_offset: u64) -> Result<(u64, u32)>

// === NK 遍历 ===

/// 解析 NkRecord 字段
fn parse_nk_record(reader, cell_offset: u32) -> Result<NkRecord>

/// 从 NkRecord 读取所有 subkey 名称+偏移
fn read_subkeys(reader, nk: &NkRecord) -> Result<Vec<(String, u32)>>
  → 读取 subkeys_list_offset 处的 lf/lh/ri
  → ri 递归展开
  → 对每个 named_key_offset，读取子 NkRecord 的 name

/// 从 NkRecord 读取所有 value
fn read_values(reader, nk: &NkRecord) -> Result<Vec<(String, ValueData)>>
  → 读取 values_list_offset 处的 u32 数组
  → 对每个 offset 读取 VkRecord
  → 根据 data_type + data_len + data_offset 读取实际数据

/// 按路径遍历到目标 key
fn navigate_to_key(reader, root_offset: u32, path: &[&str]) -> Result<NkRecord>
  → 从 root NkRecord 开始
  → 对 path 中每一级，调用 read_subkeys 找到匹配项（不区分大小写）
  → 支持 KEY_COMP_NAME 标志（ASCII 压缩名称）

// === 值数据类型读取 ===

enum ValueData {
    String(String),           // REG_SZ, REG_EXPAND_SZ
    Dword(u32),               // REG_DWORD
    Binary(Vec<u8>),          // REG_BINARY
    MultiString(Vec<String>), // REG_MULTI_SZ
    Qword(u64),               // REG_QWORD
}

fn read_value_data(reader, vk: &VkRecord) -> Result<ValueData>
```

#### 1.2.3 SYSTEM hive 提取逻辑

```rust
/// 从 SYSTEM hive 提取系统信息字段
pub fn extract_system_fields(bytes: &[u8]) -> SystemHiveInfo {
    // 1. 解析 base block
    // 2. 读取 Select\Current → 确定 ControlSet 编号 (e.g. "1")
    // 3. 导航到 ControlSet001\Control\ComputerName
    //    → 读取 ComputerName value → computer_name
    // 4. 导航到 ControlSet001\Control\TimeZoneInformation
    //    → 读取 TimeZoneKeyName (REG_SZ) → timezone
    //    → 读取 StandardName 作为 fallback
    // 5. 导航到 ControlSet001\Control\Windows
    //    → 读取 CSDVersion (REG_DWORD) → build_number (service pack)
}

SystemHiveInfo {
    computer_name: Option<String>,
    timezone: Option<String>,
    build_number: Option<String>,
    warnings: Vec<String>,
}
```

#### 1.2.4 SOFTWARE hive 提取逻辑

```rust
pub fn extract_software_fields(bytes: &[u8]) -> SoftwareHiveInfo {
    // 1. 导航到 Microsoft\Windows NT\CurrentVersion
    // 2. 读取以下 values：
    //    ProductName    (REG_SZ) → os_version  e.g. "Microsoft Windows 10 Pro"
    //    CurrentVersion (REG_SZ) → e.g. "6.3"
    //    CurrentBuild   (REG_SZ) → e.g. "19041"
    //    RegisteredOwner (REG_SZ) → registered_owner
    //    RegisteredOrganization (REG_SZ) → registered_organization
    //    InstallDate    (REG_DWORD) → install_date (Unix timestamp)
}

SoftwareHiveInfo {
    os_version: Option<String>,
    registered_owner: Option<String>,
    registered_organization: Option<String>,
    install_date: Option<DateTime<Utc>>,
    warnings: Vec<String>,
}
```

#### 1.2.5 `supports_path` 修正

当前 `supports_path` 只匹配 `.dat`，但 SYSTEM/SOFTWARE 无扩展名。修改为：

```rust
fn supports_path(&self, file_path: &str) -> bool {
    let lower = file_path.replace('\\', "/").to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    // 匹配 .dat 后缀的 hive
    name.ends_with(".dat") && (name.contains("ntuser") || ...)
    // 或匹配无扩展名的标准 hive 文件
    || matches!(name, "system" | "software" | "sam" | "security" | "default")
}
```

#### 1.2.6 `analysis_service.rs` 对接

改造 `inspect_registry_hive()`：

```rust
fn inspect_registry_hive(
    entry, parser, parsed_at,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
    warnings, provenance, system_info_fields, // 新增输出参数
) {
    match entry {
        Some(entry) => {
            // 1. 读取完整 hive（bounded 4MB，覆盖绝大多数 SYSTEM/SOFTWARE）
            let bytes = read_header_fn(&entry.id, 4 * 1024 * 1024);
            match bytes {
                Ok(bytes) => {
                    match parser {
                        REGISTRY_SYSTEM_PARSER => {
                            match registry_parser::extract_system_fields(&bytes) {
                                Ok(info) => {
                                    // 填充 computer_name, timezone, build_number
                                    // status = Parsed
                                }
                                Err(e) => {
                                    warnings.push(format!("{}: {}", entry.path, e));
                                    // status = NotParsed
                                }
                            }
                        }
                        REGISTRY_SOFTWARE_PARSER => {
                            match registry_parser::extract_software_fields(&bytes) {
                                Ok(info) => {
                                    // 填充 os_version, registered_owner 等
                                    // status = Parsed
                                }
                                Err(e) => { /* NotParsed */ }
                            }
                        }
                    }
                }
                Err(e) => { /* Unavailable */ }
            }
        }
        None => { /* Unavailable, 当前逻辑 */ }
    }
}
```

`extract_system_info_for_case` 返回类型需要从 `AnalysisSystemInfoDto` 扩展，或通过 mutable reference 传入字段。

#### 1.2.7 测试计划

| 测试 | 内容 |
|---|---|
| `parse_nk_record_minimal` | 构造最小 NK cell bytes，验证字段解析 |
| `read_subkeys_lf` | 构造 lf 格式 subkey list，验证名称+偏移 |
| `read_subkeys_lh` | lh 格式（hash-based） |
| `read_subkeys_ri` | ri 格式（间接索引，2 级） |
| `read_value_string` | REG_SZ inline + external |
| `read_value_dword` | REG_DWORD inline |
| `navigate_to_key_deep` | 3 级路径遍历 |
| `navigate_case_insensitive` | 不区分大小写匹配 |
| `extract_system_fields_real` | 用 tiny SYSTEM hive fixture |
| `extract_software_fields_real` | 用 tiny SOFTWARE hive fixture |
| `comp_name_flag` | KEY_COMP_NAME ASCII 压缩名称 |
| `no_such_key_returns_none` | 路径不存在时不 panic |

**Fixture**：需要生成精简的 SYSTEM/SOFTWARE hive 文件（可通过 Python `python-registry` 或手写二进制构造）。建议放在 `testdata/fixtures/tiny/registry/`。

### 1.3 EVTX 实现方案

#### 1.3.1 依赖

```toml
# crates/artifacts-windows/Cargo.toml
[dependencies]
evtx = "0.11.2"
```

#### 1.3.2 新增文件

`crates/artifacts-windows/src/evtx/mod.rs`：
```rust
pub mod parser;
```

`crates/artifacts-windows/src/evtx/parser.rs`：

```rust
use artifacts_core::{ArtifactExtractor, ArtifactContext, ArtifactSink, ExtractorReport};
use evtx::EvtxParser;
use std::io::{Read, Seek, SeekFrom};

pub struct EvtxBootShutdownExtractor;

/// System.evtx 中与 boot/shutdown 相关的 EventID
const BOOT_EVENT_IDS: &[u64] = &[6005];   // Event Log service started
const SHUTDOWN_EVENT_IDS: &[u64] = &[6006, 6008, 1074]; // clean/forced/planned

impl ArtifactExtractor for EvtxBootShutdownExtractor {
    fn id(&self) -> &'static str { "evtx.boot_shutdown" }
    fn display_name(&self) -> &'static str { "Windows EVTX Boot/Shutdown Parser" }

    fn supports_path(&self, file_path: &str) -> bool {
        let lower = file_path.replace('\\', "/").to_lowercase();
        lower.contains("winevt") && lower.contains("logs") && lower.ends_with("system.evtx")
    }

    fn run(&self, ctx: ArtifactContext, sink: &mut dyn ArtifactSink) -> Result<ExtractorReport, String> {
        // 1. evtx crate 需要 Read+Seek，但 ctx.reader 是 Box<dyn Read>
        //    → 读入内存（bounded 16MB），构造 Cursor
        let mut buf = Vec::new();
        ctx.reader.take(16 * 1024 * 1024).read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;

        // 2. 解析
        let parser = EvtxParser::from_buffer(buf)
            .map_err(|e| format!("EVTX parse error: {}", e))?;

        let mut boot_records = Vec::new();
        let mut errors = Vec::new();

        // 3. 遍历所有 records
        for result in parser.records_json() {
            match result {
                Ok(record) => {
                    // record.data 是 JSON string
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&record.data) {
                        let event_id = json["Event"]["System"]["EventID"]
                            .as_u64()
                            .or_else(|| json["Event"]["System"]["EventID"].as_str()?.parse().ok());

                        if let Some(eid) = event_id {
                            let is_boot = BOOT_EVENT_IDS.contains(&eid);
                            let is_shutdown = SHUTDOWN_EVENT_IDS.contains(&eid);

                            if is_boot || is_shutdown {
                                let timestamp = record.timestamp;  // chrono::DateTime<Utc>
                                let event_type = if is_boot { "BOOT" } else { "SHUTDOWN" };

                                boot_records.push((timestamp, event_type, eid));
                            }
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("EVTX record error: {}", e));
                }
            }
        }

        // 4. 按时间排序，写入 timeline events + artifacts
        boot_records.sort_by_key(|(ts, _, _)| *ts);
        let count = boot_records.len();

        for (ts, event_type, eid) in &boot_records {
            let ev = new_timeline_event(
                &ctx.file_id,
                &format!("EVTX_{}", event_type),
                *ts,
                format!("{} event #{}", event_type, eid),
                format!("Windows Event Log: {} (EventID {})", event_type, eid),
                BTreeMap::new(),
            );
            sink.write_timeline_event(ev);
        }

        Ok(ExtractorReport {
            artifacts_found: count as u32,
            timeline_events: count as u32,
            errors,
        })
    }
}
```

#### 1.3.3 注册

`crates/artifacts-windows/src/lib.rs` 中新增：

```rust
pub mod evtx;

// 在 artifact_service.rs 的 create_registry() 中注册：
registry.register(Box::new(artifacts_windows::evtx::parser::EvtxBootShutdownExtractor));
```

#### 1.3.4 `analysis_service.rs` 对接

改造 `inspect_evtx_boot_source()`：

```rust
fn inspect_evtx_boot_source(
    entry: Option<&FileEntry>,
    parsed_at: &str,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
    boot_history: &mut Vec<BootRecord>, // 新增输出
) {
    match entry {
        Some(entry) => {
            // 1. 读取前 16MB（EVTX 文件通常 1-10MB）
            match read_header_fn(&entry.id, 16 * 1024 * 1024) {
                Ok(bytes) => {
                    // 2. 调用 EvtxBootShutdownExtractor 逻辑
                    //    (提取为独立函数 extract_boot_shutdown_events(bytes))
                    match extract_boot_shutdown_events(&bytes) {
                        Ok(events) => {
                            for (ts, event_type, eid) in &events {
                                boot_history.push(BootRecord {
                                    timestamp: ts.to_rfc3339(),
                                    boot_type: event_type.clone(),
                                    source: format!("EVTX EventID {}", eid),
                                    provenance: /* ... */,
                                });
                            }
                            // status = Parsed
                        }
                        Err(e) => { /* NotParsed + warning */ }
                    }
                }
                Err(e) => { /* Unavailable */ }
            }
        }
        None => { /* Unavailable, 当前逻辑 */ }
    }
}
```

#### 1.3.5 测试计划

| 测试 | 内容 |
|---|---|
| `supports_path_system_evtx` | 匹配 `Windows/System32/winevt/Logs/System.evtx` |
| `supports_path_other_evtx` | 不匹配 `Application.evtx` |
| `extract_boot_events_minimal` | 构造最小 EVTX buffer（含 EventID 6005） |
| `extract_shutdown_events` | 含 EventID 6006 |
| `empty_evtx_returns_no_events` | 无 boot/shutdown 事件 |
| `malformed_record_continues` | 单条记录损坏不影响其他记录 |
| `integration_analysis_service` | 端到端：file entry → boot_history 非空 |

**Fixture**：需要一个包含少量 EventID 6005/6006 记录的精简 EVTX 文件。可用 `python-evtx` 或 Windows 事件导出生成。放在 `testdata/fixtures/tiny/evtx/`。

### 1.4 工作量与顺序

```
Phase 1: Registry 基础设施（hbin/cell/Nk/Vk 结构 + 寻址）
         ~250 行 + 6 个单元测试

Phase 2: Registry 路径遍历（navigate_to_key + read_values）
         ~150 行 + 5 个单元测试

Phase 3: SYSTEM/SOFTWARE 特定字段提取
         ~100 行 + 3 个单元测试

Phase 4: analysis_service 对接 + supports_path 修正
         ~80 行 + 2 个集成测试

Phase 5: EVTX parser（evtx crate 集成 + boot/shutdown 提取）
         ~150 行 + 4 个单元测试

Phase 6: EVTX → analysis_service 对接 + boot_history 输出
         ~60 行 + 1 个集成测试

Phase 7: Fixture 生成 + 端到端验证
         ~100 行
```

总代码量：**~900 行**，预计 **5-7 个 commit**。

---

## 方案二：大媒体连续流式预览

### 2.1 现状分析

**后端**：
- `media_data_url_for_file()`：小文件 → `data:mime;base64,...` 内联；大文件 → 返回 `handle_id` + `can_read_ranges: true`
- `media_range_for_file()`：IPC command，按 `offset`/`length` 读取 → base64 返回。每次调用 = 一次 IPC roundtrip

**前端**：
- `useMediaUrl()`：大文件时只读首个 1MB chunk → `Blob URL` → `<video src="blob:...">`
- 用户无法拖动进度条（只有首 chunk）
- 前端代码复杂：需要管理 `previewMode`、`previewBytes`、range IPC、blob 生命周期

**问题本质**：浏览器 `<video>` 需要一个支持 HTTP Range 请求的 URL，但 Tauri IPC command 不是 HTTP server。

### 2.2 方案：`evidence-media://` 自定义协议

#### 2.2.1 原理

Tauri 2 支持 `register_asynchronous_uri_scheme_protocol()`，允许注册自定义 URI scheme。WebView 加载 `evidence-media://file:{handleId}` 时，Rust handler 收到 HTTP 请求（含 Range header），可返回 `206 Partial Content`。

浏览器 `<video>` 原生支持 HTTP Range 请求 → 注册协议后，大文件视频/音频可原生拖动播放。

#### 2.2.2 Rust 侧实现

**`apps/desktop/src-tauri/src/commands/media_protocol.rs`（新增）**

```rust
use crate::state::AppState;
use tauri::{Manager, UriSchemeContext, UriSchemeResponder};

const MAX_PROTOCOL_READ_BYTES: u64 = 4 * 1024 * 1024; // 单次最大读取 4MB

pub fn register_evidence_media_protocol(app: &tauri::App) {
    let app_handle = app.handle().clone();

    app.register_asynchronous_uri_scheme_protocol(
        "evidence-media",
        move |_ctx, request, responder| {
            let app_handle = app_handle.clone();

            // 1. 解析 handle_id
            //    URL 格式: evidence-media://file:{file_id} 或 evidence-media://handle:{handle_id}
            let uri = request.uri().to_string();
            let handle_id = uri
                .strip_prefix("evidence-media://")
                .unwrap_or_default()
                .to_string();

            if handle_id.is_empty() {
                respond_error(responder, 400, "missing handle id");
                return;
            }

            // 2. 解析 Range header
            let range_header = request.headers()
                .get("range")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // 3. 在新线程中处理（避免阻塞 WebView 线程）
            std::thread::spawn(move || {
                // 获取 DB 连接
                let state = app_handle.state::<AppState>();
                let db_path = {
                    let guard = match state.active_case.lock() {
                        Ok(g) => g,
                        Err(e) => { respond_error(responder, 500, &e.to_string()); return; }
                    };
                    match guard.as_ref() {
                        Some(active) => active.db_path(),
                        None => { respond_error(responder, 404, "no active case"); return; }
                    }
                };

                let conn = match persistence_sqlite::open_or_create(&db_path) {
                    Ok(c) => c,
                    Err(e) => { respond_error(responder, 500, &e.to_string()); return; }
                };

                // 打开文件 handle
                let file_id_str = handle_id.strip_prefix("file:").unwrap_or(&handle_id);
                let handle = match app_services::file_service::open_file_handle_real(
                    &conn, file_id_str
                ) {
                    Ok(h) => h,
                    Err(e) => { respond_error(responder, 404, &e.to_string()); return; }
                };

                let total_size = handle.size;
                let mime = handle.mime.unwrap_or_else(|| "application/octet-stream".into());

                // 4. 解析 Range 请求
                let (start, end) = parse_range(&range_header, total_size);

                // 5. 限制单次读取量
                let end = end.min(start + MAX_PROTOCOL_READ_BYTES - 1).min(total_size - 1);
                let length = end - start + 1;

                // 6. 读取字节
                let mut reader = match app_services::file_service::open_file_content_by_id(
                    &conn, &domain::FileEntryId(file_id_str.to_string())
                ) {
                    Ok(r) => r,
                    Err(e) => { respond_error(responder, 500, &e.to_string()); return; }
                };

                if start > 0 {
                    let _ = app_services::file_service::skip_reader_bytes(reader.as_mut(), start);
                }

                let mut buf = vec![0u8; length as usize];
                let bytes_read = reader.read(&mut buf).unwrap_or(0);
                buf.truncate(bytes_read);

                // 7. 构建响应
                let is_partial = range_header.is_some();
                let status = if is_partial { 206 } else { 200 };

                let response = http::Response::builder()
                    .status(status)
                    .header("Content-Type", &mime)
                    .header("Accept-Ranges", "bytes")
                    .header("Content-Length", bytes_read)
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Access-Control-Allow-Headers", "Range")
                    .header("Access-Control-Expose-Headers", "Content-Range, Content-Length, Accept-Ranges")
                    .header("Content-Range", format!("bytes {}-{}/{}", start, start + bytes_read as u64 - 1, total_size))
                    .body(buf)
                    .unwrap();

                responder.respond(response);
            });
        },
    );
}

/// 解析 Range: bytes=start-end header
fn parse_range(header: &Option<String>, total_size: u64) -> (u64, u64) {
    match header {
        Some(h) if h.starts_with("bytes=") => {
            let spec = &h[6..];
            if let Some((start_str, end_str)) = spec.split_once('-') {
                let start: u64 = start_str.parse().unwrap_or(0);
                let end: u64 = end_str.parse().unwrap_or(total_size - 1);
                (start, end.min(total_size - 1))
            } else {
                (0, total_size - 1)
            }
        }
        _ => (0, total_size - 1),
    }
}

fn respond_error(responder: UriSchemeResponder, status: u16, msg: &str) {
    responder.respond(
        http::Response::builder()
            .status(status)
            .header("Content-Type", "text/plain")
            .body(msg.as_bytes().to_vec())
            .unwrap()
    );
}
```

**`lib.rs` 注册：**

```rust
pub mod commands::media_protocol; // 新增

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .register_asynchronous_uri_scheme_protocol(
            "evidence-media",
            commands::media_protocol::evidence_media_handler,
        )
        .invoke_handler(tauri::generate_handler![
            // ... 现有 commands
        ])
        // ...
}
```

#### 2.2.3 CSP 配置

`apps/desktop/src-tauri/tauri.conf.json`：

```json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; media-src 'self' evidence-media: data:; connect-src ipc: http://ipc.localhost; font-src 'self'"
    }
  }
}
```

在 `media-src` 中添加 `evidence-media:`。

#### 2.2.4 前端改造

**`frontend/src/features/files/hooks.ts`**

```typescript
// 改造 useMediaUrl
export function useMediaUrl(fileId?: string) {
  // ...
  return useQuery({
    queryKey: ['mediaUrl', fileId],
    queryFn: async () => {
      const media = await getMediaUrl(fileId!);

      // 大文件：使用自定义协议直连
      if (media.handleId && !media.url) {
        return {
          ...media,
          url: `evidence-media://${media.handleId}`,
          previewMode: 'stream' as const,
        };
      }

      // 小文件：保持现有 data: URL 方式
      // (当 media.url 存在时已经是 data: URL)
      return { ...media, previewMode: 'inline' as const };
    },
    // ...
  });
}
```

**`frontend/src/app/pages/FileBrowser.tsx`**

简化视频/音频预览逻辑：

```tsx
// 之前：区分 range/inline/fallback 三种模式
// 之后：统一用 <video>/<audio>，src 直接指向 evidence-media:// 或 data:

// 视频预览
if (mime.startsWith('video/') || ['mp4', 'webm', 'avi', 'mkv'].includes(mime)) {
  if (mediaUrl?.url) {
    return (
      <div className="h-full flex flex-col">
        <div className="flex-1 min-h-0">
          <VideoViewer
            src={mediaUrl.url}
            mimeType={mediaUrl.mimeType}
            fileName={selectedFile?.name}
          />
        </div>
        {mediaUrl.previewMode === 'stream' && (
          <div className="border-t border-[#e0e0e0] bg-[#f8f8f8] px-3 py-1 text-[10px] text-[#666]">
            流式预览 · {formatBytes(mediaUrl.size)} · 支持拖动进度条
          </div>
        )}
      </div>
    );
  }
  return <div className="flex items-center justify-center h-full text-[#888]">加载中...</div>;
}

// 音频类似
```

可删除的代码：
- `LargeMediaFallback` 组件（不再需要）
- `previewMode === 'range'` 分支逻辑
- `readMediaRange` IPC 调用（大文件不再需要前端主动分块读取）

**`frontend/src/types/models.ts`**

```typescript
export interface MediaUrl {
  url?: string;              // data: URL (小文件) 或 evidence-media:// URL (大文件)
  handleId?: string;
  mimeType: string;
  size: number;
  canReadRanges: boolean;
  previewMode?: 'inline' | 'stream';  // 'range' 删除，改为 'stream'
  previewBytes?: number;     // 小文件时等于 size
}
```

#### 2.2.5 保留降级路径

如果协议注册失败（极端情况），保留现有 chunk blob 方式作为 fallback：

```typescript
// hook 中
if (media.handleId && !media.url) {
  // 检测 evidence-media 协议是否可用
  // 方式：尝试加载一个小请求，或检测 window.__TAURI__ 是否存在
  // 简单方式：直接尝试 evidence-media://，如果 <video> 触发 error 再降级
  return {
    ...media,
    url: `evidence-media://${media.handleId}`,
    previewMode: 'stream' as const,
  };
}
```

VideoViewer 组件中增加 `onError` 降级：

```tsx
<VideoViewer
  src={mediaUrl.url}
  onError={() => {
    // 协议不可用时降级到 chunk blob
    setPreviewMode('chunk');
  }}
/>
```

### 2.3 需要删除的代码

| 文件 | 删除内容 |
|---|---|
| `file_commands.rs` | `media_range_for_file()` 函数（可保留但不再前端调用） |
| `FileBrowser.tsx` | `LargeMediaFallback` 组件、`previewMode === 'range'` 分支 |
| `files/hooks.ts` | `readMediaRange` import、chunk blob 构造逻辑 |
| `models.ts` | `previewMode: 'range'` 类型 |

### 2.4 测试计划

| 测试 | 内容 |
|---|---|
| `parse_range_header` | 各种 Range header 格式解析 |
| `parse_range_no_header` | 无 Range header 时返回全文 |
| `parse_range_suffix` | `bytes=-500` 后缀格式 |
| `protocol_returns_206_with_range` | 集成测试：Range 请求返回 206 + Content-Range |
| `protocol_returns_200_without_range` | 无 Range 返回 200 + 全文 |
| `protocol_rejects_invalid_handle` | 无效 handle 返回 404 |
| `protocol_does_not_leak_host_path` | 响应中不含宿主路径 |
| `protocol_clamps_read_size` | 单次读取不超过 4MB |
| Frontend: `VideoViewer loads evidence-media URL` | 前端组件测试 |

### 2.5 工作量与顺序

```
Phase 1: media_protocol.rs（Range 解析 + 异步 handler）
         ~120 行 + 5 个单元测试

Phase 2: lib.rs 注册 + CSP 配置
         ~10 行

Phase 3: 前端 hook 改造（useMediaUrl 简化）
         ~30 行修改 + 删除 ~40 行

Phase 4: 前端 FileBrowser 简化（删除 LargeMediaFallback、统一预览）
         删除 ~60 行 + 修改 ~20 行

Phase 5: 类型清理（models.ts）
         ~10 行

Phase 6: 端到端验证
         1-2 个集成测试
```

总代码量：**净增 ~100 行，删除 ~100 行**。预计 **2-3 个 commit**。

---

## 综合建议

| | 方案一（Registry/EVTX） | 方案二（媒体流式） |
|---|---|---|
| 复杂度 | ⭐⭐⭐⭐⭐ | ⭐⭐ |
| 代码量 | ~900 行 | ~净 0 行（增删平衡） |
| 外部依赖 | `evtx` crate | 无 |
| 风险 | Registry 二进制格式解析细节多 | CSP scheme 兼容性需验证 |
| 用户可见 | Analysis 页显示真实系统信息 | 大视频可拖动播放 |
| **建议顺序** | **第二** | **第一（快速出成果）** |

建议先实施方案二（1-2 天），再实施方案一（3-5 天）。
