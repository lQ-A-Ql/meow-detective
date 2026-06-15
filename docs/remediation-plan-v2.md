# Forensics Workbench — 综合修补方案 v2.0

**来源**: 安全审计 (2026-05-29) + 功能性审计 (2026-05-29)  
**基线**: 前次修补计划 (audit-remediation-plan.md) 中已完成项已标记 ✅  
**总计**: 5 Phase / 28 Task / 60+ 验收条件  

---

## Phase 0 — Critical Security Fixes（阻断级）

> 目标：消除可导致任意文件删除、程序崩溃、无限循环的 3 个 Critical 漏洞。  
> 预计工期：1 天  
> 依赖：无  

### Task 0.1: 沙箱化 `delete_case` — 防止任意目录删除

| 属性 | 值 |
|------|-----|
| 安全编号 | C-01 (Critical) |
| 文件 | `apps/desktop/src-tauri/src/commands/case_commands.rs:221-253`<br>`crates/app-services/src/case_service.rs:96-124` |
| 问题 | `delete_case` 接受前端 `case_root` 字符串直接传给 `fs::remove_dir_all()`。仅校验目录内存在 `case.json`，任何含该文件的目录均可被删除 |
| 方案 | 1. 在 `infrastructure::config` 中定义 `safe_cases_root()` → `%APPDATA%/ForensicsWorkbench/cases/`<br>2. `case_service::delete_case()` 中对 `case_root` 做 `canonicalize()` + `starts_with(safe_root)` 校验<br>3. 拒绝符号链接逃逸（`canonicalize` 已包含）<br>4. `create_case` 同步使用此 safe_root 作为默认根 |
| 验收标准 | - [ ] 单元测试：传入 `C:\Windows\System32`（含 case.json）→ 返回 Err("path outside safe root")<br>- [ ] 单元测试：传入含 symlink 的路径 → canonicalize 后被拒绝<br>- [ ] 单元测试：传入合法子目录 → 成功删除<br>- [ ] `cargo test --workspace` 全通过 |

### Task 0.2: GPT 解析器 `entry_size` 校验

| 属性 | 值 |
|------|-----|
| 安全编号 | C-02 (Critical) |
| 文件 | `crates/evidence-core/src/volume/gpt.rs:65` |
| 问题 | `parse_gpt_entries` 从磁盘镜像头读取 `entry_size`，当 < 128 时切片 `entry[56..128]` 越界导致 panic |
| 方案 | 在循环体内切片前添加：<br>`if (entry_size as usize) < 128 { continue; }` |
| 验收标准 | - [ ] 测试：构造 `entry_size=64` 的 GPT buffer → 不 panic，返回空列表<br>- [ ] 测试：构造 `entry_size=0` 的 GPT buffer → 不 panic<br>- [ ] 测试：`entry_size=128` 正常解析<br>- [ ] `cargo test --test gpt_test` 全通过 |

### Task 0.3: E01 section walker 环检测

| 属性 | 值 |
|------|-----|
| 安全编号 | C-03 (Critical) |
| 文件 | `crates/image-e01/src/lib.rs` (section descriptor linked list walk) |
| 问题 | `while next_off > 0` 循环遍历 section 链表无 visited set，恶意 E01 可构造环形链表导致死循环 |
| 方案 | 在循环前添加 `let mut visited = HashSet::<u64>::new();`，循环体开头添加：<br>`if !visited.insert(next_off) { break; }` |
| 验收标准 | - [ ] 测试：构造 `next` 指向自身偏移的 section buffer → 不死循环，正常退出<br>- [ ] 测试：正常 E01 文件 → section 解析结果不变<br>- [ ] `cargo test --test e01_regression_test` 全通过 |

---

## Phase 1 — High Security + Core Functional（发布阻断）

> 目标：修复 5 个 High 安全漏洞 + 1 个 High 功能缺陷。  
> 预计工期：2 天  
> 依赖：Phase 0 完成  

### Task 1.1: `create_case` 名称校验 — 防止路径穿越

| 属性 | 值 |
|------|-----|
| 安全编号 | H-01 (High) |
| 文件 | `crates/app-services/src/case_service.rs:44-45` |
| 问题 | `root.join(name)` 中 `name` 可含 `../../`，在预期根目录外创建目录 |
| 方案 | 1. 在 `case_service` 添加 `fn validate_case_name(name: &str) -> Result<(), String>`<br>2. 校验规则：`^[a-zA-Z0-9_\x20-]{1,100}$`，拒绝 `/` `\` `..` `\0`<br>3. 在 `create_case()` 入口调用校验<br>4. 在 transport 层添加对应的 `CommandError::invalid_input` |
| 验收标准 | - [ ] 测试：`create_case(root, "../../etc")` → Err(INVALID_INPUT)<br>- [ ] 测试：`create_case(root, "valid-case_01")` → Ok<br>- [ ] 测试：`create_case(root, "")` → Err(INVALID_INPUT)<br>- [ ] 测试：`create_case(root, "a".repeat(101))` → Err(INVALID_INPUT)<br>- [ ] 前端 CaseHome 创建案件时显示校验错误消息 |

### Task 1.2: `import_data_source` 路径限制

| 属性 | 值 |
|------|-----|
| 安全编号 | H-02 (High) |
| 文件 | `apps/desktop/src-tauri/src/commands/file_commands.rs:145-160` |
| 问题 | `source_path` 无校验，可读取系统任意文件 |
| 方案 | 1. 在 `file_commands::import_data_source` 中校验路径存在性 + 类型（regular file 或 directory）<br>2. 拒绝特殊路径：`/dev/`, `\\.\`, `CON`, `NUL` 等<br>3. 前端 `CaseHome` 已使用 `tauri_plugin_dialog::open()` 选择文件 → 确认 dialog 返回值校验 |
| 验收标准 | - [ ] 测试：`import_data_source("C:\Windows\System32\config\SAM")` → 可以打开但不泄露内容到错误消息<br>- [ ] 测试：`import_data_source("CON")` → Err(INVALID_INPUT)<br>- [ ] 前端导入按钮使用 dialog 选择器（已实现） |

### Task 1.3: NTFS `read_file_data` 内存前置检查

| 属性 | 值 |
|------|-----|
| 安全编号 | H-03 (High) |
| 文件 | `crates/fs-ntfs/src/lib.rs` (`read_file_data` 方法) |
| 问题 | 从 data run 计算总大小后 `vec![0u8; total_size]`，恶意 NTFS 声称 2GB+ 文件导致 OOM（128MB 后置检查来不及生效） |
| 方案 | 在 data run 累积循环中添加前置检查：<br>`if accumulated > MAX_FILE_BUFFER { return Err(io::Error::new(OutOfMemory, ...)); }`<br>`const MAX_FILE_BUFFER: u64 = 128 * 1024 * 1024; // 128MB` |
| 验收标准 | - [ ] 测试：构造声称 2GB 的 data run → 返回 Err(OutOfMemory)，不 OOM<br>- [ ] 测试：正常大小文件 → 正常读取<br>- [ ] `cargo test --test mft_test` 全通过 |

### Task 1.4: HTML 报告输出转义 — 防止 XSS

| 属性 | 值 |
|------|-----|
| 安全编号 | H-04 (High) |
| 文件 | `crates/reports/src/html/exporter.rs:18-30` |
| 问题 | `case.name`、`case.number`、`case.examiner`、文件路径直接 `write!` 进 HTML |
| 方案 | 1. 在 `infrastructure::text` 添加 `fn html_escape(s: &str) -> String`（转义 `<>&"'`）<br>2. HTML exporter 中对所有动态内容调用 `html_escape()` |
| 验收标准 | - [ ] 测试：`case.name = "<script>alert(1)</script>"` → 输出中为 `&lt;script&gt;alert(1)&lt;/script&gt;`<br>- [ ] 测试：正常名称 → 输出不变<br>- [ ] `cargo test --workspace` 全通过 |

### Task 1.5: 添加 CSP 配置

| 属性 | 值 |
|------|-----|
| 安全编号 | H-05 (High) |
| 文件 | `apps/desktop/src-tauri/tauri.conf.json` |
| 问题 | `app.security` 完全缺失，WebView 无 CSP 限制 |
| 方案 | 在 `tauri.conf.json` 的 `app` 对象中添加：<br>`"security": { "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost; font-src 'self'" }` |
| 验收标准 | - [ ] 应用正常启动，无 CSP 违规错误<br>- [ ] `pnpm dev` mock 模式正常<br>- [ ] `cargo tauri dev` 正常（如可用）<br>- [ ] 前端样式（Tailwind inline）不受影响 |

### Task 1.6: 连接 Reports 导出按钮

| 属性 | 值 |
|------|-----|
| 功能编号 | F-01 (High) |
| 文件 | `frontend/src/app/pages/Reports.tsx`<br>`frontend/src/lib/api/reports.ts` |
| 问题 | 后端 `export_html/csv/json_report` 命令已注册但前端 Reports 页面无导出按钮 |
| 方案 | 1. 在 `reports.ts` 添加 `exportHtmlReport()`、`exportCsvReport()`、`exportJsonReport()` 函数<br>2. Reports 页面的模板卡片添加"导出"按钮<br>3. 使用 `tauri_plugin_dialog::save()` 选择保存路径<br>4. 导出中显示进度（复用 InlineProgressRow）<br>5. 导出完成 toast 提示 |
| 验收标准 | - [ ] 点击"执行摘要"模板的导出按钮 → 弹出保存对话框 → 生成 HTML 文件<br>- [ ] 点击"IOC 导出"模板 → 生成 CSV 文件<br>- [ ] 导出过程中 Reports 页面显示 running 状态<br>- [ ] `pnpm typecheck` + `pnpm test` 通过 |

---

## Phase 2 — Medium Security + Functional Wiring（Sprint 级）

> 目标：修复 12 个 Medium 安全问题 + 5 个 Medium 功能断点。  
> 预计工期：3-4 天  
> 依赖：Phase 1 完成  

### Task 2.1: 修复 unsafe 生命周期转写

| 属性 | 值 |
|------|-----|
| 安全编号 | M-01 (Medium) |
| 文件 | `crates/app-services/src/file_service.rs:915-917` |
| 问题 | `&AtomicBool` 裸指针转 `'static`，无结构性生命周期保证 |
| 方案 | 将 `cancel: Option<&AtomicBool>` 参数改为 `cancel: Option<Arc<AtomicBool>>`，传所有权到 spawned thread |
| 验收标准 | - [ ] `unsafe` 块完全移除<br>- [ ] MFT 扫描取消功能正常<br>- [ ] `cargo clippy -- -D warnings` 通过 |

### Task 2.2: artifact 提取内存限制

| 属性 | 值 |
|------|-----|
| 安全编号 | M-02 (Medium) |
| 文件 | `crates/app-services/src/artifact_service.rs:30-33` |
| 问题 | `reader.read_to_end()` 无大小限制，多 GB 文件 OOM |
| 方案 | 读取前检查文件大小（从 `FsNode.size` 或 reader metadata），超过 `ARTIFACT_FILE_LIMIT`（50MB）则跳过并记录 warning |
| 验收标准 | - [ ] 测试：传入 100MB 的 mock reader → 返回 warning，不 OOM<br>- [ ] 正常大小文件 → 正常提取 |

### Task 2.3: 分页 limit 上限

| 属性 | 值 |
|------|-----|
| 安全编号 | M-04 (Medium) |
| 文件 | `crates/transport/src/paging.rs`<br>`crates/transport/src/commands/mod.rs:87-100` |
| 问题 | `PageRequest.limit` 为 u32 无上限，可请求 4B 行 |
| 方案 | 1. 在 `PageRequest` 添加验证方法 `fn clamp(&mut self) { self.limit = self.limit.min(1000); }`<br>2. 在各 command handler 入口调用 |
| 验收标准 | - [ ] 测试：`limit = u32::MAX` → 实际查询 limit=1000<br>- [ ] 测试：`limit = 50` → 不变 |

### Task 2.4: 移除 `From<String> for CommandError`

| 属性 | 值 |
|------|-----|
| 安全编号 | M-06 (Medium) |
| 文件 | `crates/transport/src/errors.rs:95-98` |
| 问题 | 绕过 `from_service_error()` 脱敏，原始字符串直达前端 |
| 方案 | 1. 删除 `impl From<String> for CommandError`<br>2. 全局搜索所有依赖此 impl 的调用点（`.map_err(String::from)` 或 `?` on `Result<T, String>`）<br>3. 逐一改为 `.map_err(CommandError::from_service_error)` |
| 验收标准 | - [ ] `cargo check --workspace` 通过（无编译错误）<br>- [ ] 全局搜索确认无 `From<String>` 依赖残留<br>- [ ] 故意触发 DB 错误 → 前端收到泛化消息 |

### Task 2.5: DTO 字段输入校验层

| 属性 | 值 |
|------|-----|
| 安全编号 | M-07 (Medium) |
| 文件 | `crates/transport/src/commands/mod.rs` |
| 问题 | `CreateCaseRequest.case_root`、`ImportDataSourceRequest.source_path`、`ViewerRangeRequestDto.length` 无校验 |
| 方案 | 1. 在各 Request 类型添加 `fn validate(&self) -> Result<(), CommandError>` 方法<br>2. `case_root`: 校验为绝对路径，无 null 字节<br>3. `source_path`: 校验存在性<br>4. `ViewerRangeRequestDto.length`: clamp 到 `MAX_RANGE_LENGTH`（1MB）<br>5. 在 command handler 入口调用 `request.validate()?` |
| 验收标准 | - [ ] 测试：`case_root = ""` → Err(INVALID_INPUT)<br>- [ ] 测试：`length = u32::MAX` → clamp 到 1MB<br>- [ ] `cargo test --workspace` 通过 |

### Task 2.6: 迁移脚本幂等化

| 属性 | 值 |
|------|-----|
| 安全编号 | M-08 (Medium) |
| 文件 | `crates/persistence-sqlite/src/migrations/runner.rs:31-45`<br>`crates/persistence-sqlite/src/migrations/scripts/0010_*.sql` |
| 问题 | ALTER TABLE 非幂等，部分失败后 schema 损坏 |
| 方案 | 1. 每个 migration script 包裹 `BEGIN;` ... `COMMIT;`<br>2. `0010` 的 ALTER TABLE 添加列存在性检查（`SELECT * FROM pragma_table_info(...)` 再决定是否 ALTER）<br>3. 或使用 `CREATE TABLE IF NOT EXISTS` 模式 |
| 验收标准 | - [ ] 测试：运行到 0010 一半时 kill → 重启后 migration 正常继续<br>- [ ] 测试：完整运行 → `PRAGMA user_version` 正确<br>- [ ] `cargo test --test connection_test` 通过 |

### Task 2.7: CSV 公式注入净化

| 属性 | 值 |
|------|-----|
| 安全编号 | M-09 (Medium) |
| 文件 | `crates/reports/src/csv/exporter.rs` |
| 问题 | 双引号转义正确，但未防 `=` `+` `-` `@` 开头的公式注入 |
| 方案 | 对每个 cell 值：如果首字符为 `=` `+` `-` `@`，前缀单引号 `'` 或改为 `\t` 前缀 |
| 验收标准 | - [ ] 测试：cell 值为 `=cmd\|' /C calc'!A0` → 输出 `'=cmd\|...`<br>- [ ] 测试：正常值 → 不变 |

### Task 2.8: CI 硬化

| 属性 | 值 |
|------|-----|
| 安全编号 | M-12 (Medium) |
| 文件 | `.github/workflows/ci-backend.yml`（新建 `ci-frontend.yml`） |
| 问题 | 无依赖审计、无前端 CI |
| 方案 | 1. `ci-backend.yml` 添加 `cargo audit` step<br>2. 新建 `ci-frontend.yml`：pnpm install → typecheck → lint → build → test<br>3. 添加 `cargo clippy` comment lint（可选） |
| 验收标准 | - [ ] PR 触发 → backend CI 运行 cargo audit<br>- [ ] PR 触发 → frontend CI 运行 typecheck + build + test<br>- [ ] CI 红灯时 PR 无法合并 |

### Task 2.9: 取消导入 UI

| 属性 | 值 |
|------|-----|
| 功能编号 | F-05 (Medium) |
| 文件 | `frontend/src/app/pages/CaseHome.tsx`<br>`frontend/src/lib/api/files.ts` |
| 问题 | 后端 `cancel_import` 已实现但前端无取消按钮 |
| 方案 | 1. `files.ts` 添加 `cancelImport(jobId: string)` 函数<br>2. CaseHome 导入进度条旁添加"取消"按钮<br>3. 取消后刷新 jobs snapshot |
| 验收标准 | - [ ] 导入进行中 → 显示取消按钮<br>- [ ] 点击取消 → 后端停止导入，job 状态变为 cancelled/failed<br>- [ ] 取消后可重新导入 |

### Task 2.10: UI 按钮逻辑连接

| 属性 | 值 |
|------|-----|
| 功能编号 | F-02, F-03, F-04 (Medium) |
| 文件 | `frontend/src/app/pages/FileBrowser.tsx`<br>`frontend/src/app/pages/Search.tsx` |
| 问题 | 3 个按钮有 UI 但无点击逻辑 |
| 方案 | 1. **"提取文件"**：调用 `tauri_plugin_dialog::save()` + 新建 `extract_file` Tauri 命令<br>2. **"在时间线中查看"**：`useSelectionStore.setSelectedTimelineId()` + `router.navigate('/timeline')`<br>3. **"在文件浏览中打开"**：`useSelectionStore.setSelectedFileId()` + `router.navigate('/files')` |
| 验收标准 | - [ ] FileBrowser "在时间线中查看" → 跳转 Timeline 页并选中对应事件<br>- [ ] Search "在文件浏览中打开" → 跳转 FileBrowser 并选中对应文件<br>- [ ] "提取文件" → 弹出保存对话框 → 文件写入目标路径（可后续实现） |

### Task 2.11: 补充缺失的事件 emit

| 属性 | 值 |
|------|-----|
| 功能编号 | F-06 (Medium) |
| 文件 | `crates/app-services/src/case_service.rs`<br>`crates/app-services/src/file_service.rs`<br>`crates/app-services/src/artifact_service.rs` |
| 问题 | 7/11 个事件 topic 无后端 emit 代码 |
| 方案 | 在以下位置补充 `emit_event` 调用：<br>1. `case_service::open_case()` → emit `case-opened`<br>2. `case_service::close_case()` → emit `case-closed`<br>3. `file_service` 导入开始 → emit `job-created` + `job-started`<br>4. `artifact_service` 每提取一个 artifact → emit `artifact-added`<br>5. `file_service` 时间线投影完成 → emit `timeline-updated`<br>6. `search_service` 索引进度 → emit `search-index_progress` |
| 验收标准 | - [ ] 打开案件 → 前端 console 收到 `case-opened` 事件（Tauri 模式）<br>- [ ] 导入完成 → 前端收到 `timeline-updated`、`artifact-added`<br>- [ ] 搜索索引 → 前端收到 `search-index_progress` |

### Task 2.12: ArtifactRepo 空 ID 修复

| 属性 | 值 |
|------|-----|
| 功能编号 | F-11 (Low → 提升为 Medium 因影响数据完整性) |
| 文件 | `crates/persistence-sqlite/src/repositories/artifact_repo.rs:20-21` |
| 问题 | `insert_batch` 硬编码空 `case_id` / `data_source_id` |
| 方案 | 1. `Artifact` domain 类型添加 `case_id` 和 `data_source_id` 字段<br>2. `artifact_service::store_artifacts` 传入当前 case_id 和 data_source_id<br>3. `insert_batch` 使用实际值而非空字符串 |
| 验收标准 | - [ ] 导入后查询 artifacts → 每条记录有正确的 case_id 和 data_source_id<br>- [ ] 多 case 场景 → artifacts 按 case 隔离 |

---

## Phase 3 — Low Security + Functional Polish（Hardening）

> 目标：修复 10 个 Low 安全问题 + 6 个 Low 功能问题。  
> 预计工期：2-3 天  
> 依赖：Phase 2 完成  

### Task 3.1: LIKE 通配符转义

| 属性 | 值 |
|------|-----|
| 安全编号 | L-01 |
| 文件 | `crates/persistence-sqlite/src/repositories/file_repo.rs:145` |
| 方案 | 在 `format!("{}%", prefix)` 前对 `prefix` 中的 `%` 和 `_` 转义：`prefix.replace('%', "\\%").replace('_', "\\_")`，SQL 中使用 `ESCAPE '\\'` |
| 验收标准 | - [ ] 测试：`prefix = "test%file"` → 只匹配字面量 `test%file%` |

### Task 3.2: 事件 topic 校验

| 属性 | 值 |
|------|-----|
| 安全编号 | L-06 |
| 文件 | `crates/transport/src/events/mod.rs:16` |
| 方案 | 将 `EventEnvelope.topic` 从 `String` 改为 `EventTopic` 枚举（枚举值为已知 topic 常量），serde 自动校验 |
| 验收标准 | - [ ] 反序列化未知 topic → serde 报错<br>- [ ] 所有已知 topic → 正常序列化/反序列化 |

### Task 3.3: 事件定向发送

| 属性 | 值 |
|------|-----|
| 安全编号 | L-03 |
| 文件 | `apps/desktop/src-tauri/src/events/event_bridge.rs:8` |
| 方案 | `app.emit()` 改为 `app.emit_to("main", topic, event)` |
| 验收标准 | - [ ] 单窗口场景 → 行为不变<br>- [ ] 添加第二个窗口 → 第二个窗口不收到主窗口事件 |

### Task 3.4: 日志路径脱敏

| 属性 | 值 |
|------|-----|
| 安全编号 | L-04 |
| 文件 | `apps/desktop/src-tauri/src/main.rs:2-11` |
| 方案 | 1. 日志级别从 `info` 降为 `warn`<br>2. 或添加 file appender 写入 `%APPDATA%/ForensicsWorkbench/logs/`<br>3. 日志文件权限设为仅当前用户可读 |
| 验收标准 | - [ ] 启动后 stderr 无 evidence 路径<br>- [ ] 日志文件存在于安全目录 |

### Task 3.5: 近案例文件完整性保护

| 属性 | 值 |
|------|-----|
| 安全编号 | L-04 (原 Task 0.3) |
| 文件 | `apps/desktop/src-tauri/src/commands/case_commands.rs:278-336` |
| 方案 | 1. 写入前校验每个 case_root 指向有效 case.json<br>2. 使用 `dirs::data_dir()` 存储，目录权限 0o600<br>3. 可选：HMAC 签名 |
| 验收标准 | - [ ] 篡改 recent-cases.json 添加不存在的路径 → 打开时报错而非崩溃<br>- [ ] 文件权限仅当前用户 |

### Task 3.6: Settings 可编辑化

| 属性 | 值 |
|------|-----|
| 功能编号 | F-07 (Low) |
| 文件 | `frontend/src/app/pages/Settings.tsx` |
| 方案 | 1. 案件目录：可编辑 input + 保存按钮 → 调用后端 config 写入<br>2. 镜像搜索路径：可编辑<br>3. 添加主题切换（暗色/亮色） |
| 验收标准 | - [ ] 修改案件目录 → 保存后重启生效<br>- [ ] 主题切换 → localStorage 持久化 |

### Task 3.7: Timeline 筛选

| 属性 | 值 |
|------|-----|
| 功能编号 | F-08 (Low) |
| 文件 | `frontend/src/app/pages/Timeline.tsx` |
| 方案 | 1. 添加日期范围选择器（date-fns 已有依赖）<br>2. 添加事件类型下拉过滤<br>3. 过滤参数传入 `getTimelineEvents({ startDate, endDate, eventType })` |
| 验收标准 | - [ ] 选择日期范围 → 结果按范围过滤<br>- [ ] 选择事件类型 → 仅显示匹配事件 |

### Task 3.8: FileBrowser 文本/媒体预览

| 属性 | 值 |
|------|-----|
| 功能编号 | F-10 (Low) |
| 文件 | `frontend/src/app/pages/FileBrowser.tsx` |
| 方案 | 1. 文本预览：后端 `read_file_range` 返回 text 类型 → 前端渲染<br>2. 图片预览：对 mime 类型为 image/* 的文件 → base64 后端返回 → 前端 `<img>` 标签 |
| 验收标准 | - [ ] 选中 .txt 文件 → 文本预览显示内容<br>- [ ] 选中 .png 文件 → 图片预览显示图像 |

---

## Phase 4 — Testing & CI/CD（质量保障）

> 目标：补齐测试覆盖、fixture 管理、CI 管线。  
> 预计工期：2-3 天  
> 依赖：Phase 0-2 完成  

### Task 4.1: 测试 fixture 管理

| 文件 | `crates/testing/src/fixtures/mod.rs` |
| 方案 | 1. 创建小型测试 E01 镜像（<1MB）放入 `testdata/images/`<br>2. `testing` crate 提供 `fn test_e01_path() -> PathBuf`<br>3. 替换所有 `skip()` + 硬编码外部路径 |
| 验收标准 | - [ ] CI 环境（无外部 E01）→ 全部 E01 测试不再 skip<br>- [ ] `cargo test --workspace` 全通过（含 E01 集成测试） |

### Task 4.2: 后端测试补充

| 文件 | `crates/app-services/tests/`<br>`crates/transport/`<br>`crates/reports/` |
| 方案 | 1. `search_service_test.rs` — 索引 + 搜索 + 分页<br>2. `timeline_service_test.rs` — 投影 + 查询<br>3. `artifact_service_test.rs` — 提取 + 存储<br>4. `transport/` — DTO 序列化/反序列化 roundtrip<br>5. `reports/` — HTML/CSV/JSON 输出验证 + XSS 转义验证 |
| 验收标准 | - [ ] `cargo test --workspace` 新增 ≥20 个测试<br>- [ ] reports crate 覆盖率 > 80% |

### Task 4.3: 前端测试补充

| 文件 | `frontend/src/lib/api/*.test.ts`<br>`frontend/src/features/*/hooks.test.ts` |
| 方案 | 1. `search.test.ts` — searchFiles API<br>2. `timeline.test.ts` — getTimelineEvents API<br>3. `artifacts.test.ts` — getArtifactFamilies/Rows API<br>4. `reports.test.ts` — export 命令调用<br>5. `jobs.test.ts` — 轮询逻辑（useJobsSnapshot） |
| 验收标准 | - [ ] `pnpm test` 新增 ≥15 个测试<br>- [ ] 前端 API 覆盖率 100%（所有 API 函数至少 1 个测试） |

### Task 4.4: DevTools CI 守护

| 安全编号 | M-10 |
| 文件 | `.github/workflows/ci-backend.yml` |
| 方案 | 添加 CI step 检查 tauri features 不含 `devtools`：<br>`cargo metadata ... \| grep devtools && exit 1 \|\| true` |
| 验收标准 | - [ ] CI 中 `devtools` feature → CI 红灯 |

### Task 4.5: 补充 event capability

| 安全编号 | M-11 |
| 文件 | `apps/desktop/src-tauri/capabilities/default.json` |
| 方案 | 添加 `"event:default"` 到 permissions 数组 |
| 验收标准 | - [ ] 前端 `listen()` 调用不报错<br>- [ ] 事件桥接正常工作 |

---

## Phase 5 — Code Quality & Technical Debt（收尾）

> 目标：消除代码重复、魔法数字，补充文档，清理 stub。  
> 预计工期：2 天  
> 依赖：Phase 0-4 完成  

### Task 5.1: NTFS/FAT 枚举去重

| 文件 | `apps/desktop/src-tauri/src/commands/file_commands.rs:554-616` |
| 方案 | 提取 `fn enumerate_partition_filesystem(reader, candidate, conn, ...) -> Result<EnumerationStats>`，NTFS/FAT 分支只在创建 reader 时不同 |
| 验收标准 | - [ ] 代码行减少 ≥50 行<br>- [ ] 现有测试全通过 |

### Task 5.2: 魔法数字常量化

| 文件 | `infrastructure/src/constants.rs` |
| 方案 | 集中定义：`ARTIFACT_EXTRACTION_LIMIT`, `TEXT_INDEX_LIMIT`, `MAX_RANGE_LENGTH`, `JOB_LIST_LIMIT`, `MAX_FILE_BUFFER`, `ARTIFACT_FILE_LIMIT`, `MAX_PAGE_LIMIT`, `MAX_CASE_NAME_LEN` |
| 验收标准 | - [ ] `cargo clippy` 无 magic number 警告<br>- [ ] 所有引用点使用常量 |

### Task 5.3: SQL 查询迁移到 Repository 层

| 文件 | `apps/desktop/src-tauri/src/commands/case_commands.rs:78-88` |
| 方案 | `get_case_metrics` 中的内联 SQL 迁移到 `case_repo::get_metrics(conn)` |
| 验收标准 | - [ ] Tauri command 层无 SQL<br>- [ ] 现有测试通过 |

### Task 5.4: 公共 API 文档注释

| 文件 | `crates/app-services/src/*.rs`, `crates/transport/src/**/*.rs` |
| 方案 | 为所有 `pub fn` 添加 `///` 文档注释 |
| 验收标准 | - [ ] `cargo doc --workspace --no-deps` 无 warning<br>- [ ] 核心模块（file_service, case_service）注释覆盖率 100% |

### Task 5.5: 依赖版本锁定 + SBOM

| 文件 | `Cargo.toml`, CI |
| 方案 | 1. workspace deps 使用 `~major.minor` 范围（已有）<br>2. CI 添加 `cargo deny` 检查 license + advisory<br>3. 生成 SBOM（`cargo sbom` 或 `cyclonedx-bom`） |
| 验收标准 | - [ ] `cargo update --dry-run` 无意外 major 升级<br>- [ ] CI 包含 `cargo deny` step |

---

## 执行顺序与依赖图

```
Phase 0 (Critical Security)        ← 1 天，无依赖
    │
    ├──────────────────────┐
    ▼                      ▼
Phase 1 (High)             │      ← 2 天，依赖 P0
    │                      │
    ├──────────┐           │
    ▼          ▼           │
Phase 2      Phase 3       │      ← 3-4 天 / 2-3 天，可部分并行
    │          │           │
    ▼          ▼           │
Phase 4 (Testing)          │      ← 2-3 天，依赖 P0-2
    │                      │
    ▼                      ▼
Phase 5 (Quality)                  ← 2 天，最后收尾
```

## 统计汇总

| Phase | 安全修复 | 功能修复 | 预计工期 | 验收条件数 |
|-------|---------|---------|---------|-----------|
| P0 Critical | 3 (C-01,C-02,C-03) | 0 | 1 天 | 10 |
| P1 High | 5 (H-01~H-05) | 1 (F-01) | 2 天 | 18 |
| P2 Medium | 8 (M-01~M-12) | 5 (F-02~F-11) | 3-4 天 | 28 |
| P3 Low | 6 (L-01~L-10) | 4 (F-07~F-10) | 2-3 天 | 12 |
| P4 Testing | 2 (M-10,M-11) | 0 | 2-3 天 | 10 |
| P5 Quality | 1 (M-12补充) | 0 | 2 天 | 8 |
| **合计** | **25** | **10** | **~13 天** | **86** |

---

*方案由 Codex 自动生成 — 2026-05-29T17:30:00+08:00*  
*来源：full-security-audit-2026-05-29.md + full-functional-audit-2026-05-29.md*
