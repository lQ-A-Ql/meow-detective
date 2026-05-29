# Forensics Workbench — 综合修补方案 v2.1

**来源**: 安全审计 + 功能性审计 + v2.0 代码复核  
**基线**: audit-remediation-plan.md 已完成项 + v2.0 复核修正  
**总计**: 6 Phase / 33 Task / 100 验收条件  
**预计总工期**: ~15 天  

---

## Phase 0 — Critical Security（1 天）

> 消除 3 个可导致任意删除/崩溃/死循环的阻断级漏洞。

### T0.1: 沙箱化 `delete_case`

| 属性 | 值 |
|------|-----|
| 编号 | C-01 |
| 文件 | `apps/desktop/src-tauri/src/commands/case_commands.rs:222`<br>`crates/app-services/src/case_service.rs:114` |
| 问题 | `delete_case` 接受前端 `case_root` 直接传给 `fs::remove_dir_all()`，仅校验 `case.json` 存在 |
| 方案 | 1. `infrastructure::config` 定义 `safe_cases_root()` → `%APPDATA%/ForensicsWorkbench/cases/`<br>2. `case_service::delete_case()` 中 `canonicalize()` + `starts_with(safe_root)`<br>3. `create_case` 同步使用 safe_root 作为默认根 |
| 验收 | - [ ] 传入 `C:\Windows\System32` → Err("path outside safe root")<br>- [ ] 传入合法子目录 → 成功删除<br>- [ ] `cargo test --workspace` 通过 |

### T0.2: GPT `entry_size` 校验

| 属性 | 值 |
|------|-----|
| 编号 | C-02 |
| 文件 | `crates/evidence-core/src/volume/gpt.rs:65` |
| 问题 | `entry[56..128]` 在 `entry_size < 128` 时越界 panic |
| 方案 | 循环体首行：`if (entry_size as usize) < 128 { continue; }` |
| 验收 | - [ ] `entry_size=64` → 不 panic，返回空<br>- [ ] `entry_size=128` → 正常解析<br>- [ ] `cargo test --test gpt_test` 通过 |

### T0.3: E01 section walker 环检测

| 属性 | 值 |
|------|-----|
| 编号 | C-03 |
| 文件 | `crates/image-e01/src/lib.rs:49` |
| 问题 | `while next_off > 0` 无 visited set，恶意 E01 可构造环形链表死循环 |
| 方案 | 循环前 `let mut visited = HashSet::<u64>::new();`，循环体首行 `if !visited.insert(next_off) { break; }` |
| 验收 | - [ ] 环形 section → 不死循环<br>- [ ] `cargo test --test e01_regression_test` 通过 |

---

## Phase 1 — High Security + High Functional（2 天）

> 修复 5 个 High 安全 + 1 个 High 功能缺陷。

### T1.1: 案例名称校验

| 属性 | 值 |
|------|-----|
| 编号 | H-01 |
| 文件 | `crates/app-services/src/case_service.rs:44` |
| 问题 | `root.join(name)` 中 `name` 可含 `../../` |
| 方案 | 1. `case_service` 添加 `fn validate_case_name(name: &str) -> Result<(), String>`<br>2. 正则 `^[a-zA-Z0-9_\x20-]{1,100}$`，拒绝 `/` `\` `..` `\0`<br>3. `create_case()` 入口调用校验 |
| 验收 | - [ ] `"../../etc"` → Err(INVALID_INPUT)<br>- [ ] `"valid-case_01"` → Ok<br>- [ ] `""` → Err(INVALID_INPUT)<br>- [ ] 前端显示校验错误消息 |

### T1.2: 导入路径限制

| 属性 | 值 |
|------|-----|
| 编号 | H-02 |
| 文件 | `apps/desktop/src-tauri/src/commands/file_commands.rs:145` |
| 问题 | `source_path` 无校验 |
| 方案 | 校验路径存在性 + 类型，拒绝特殊路径（`CON`, `NUL`, `\\.\`）。前端已使用 `tauri_plugin_dialog::open()` |
| 验收 | - [ ] `"CON"` → Err(INVALID_INPUT)<br>- [ ] dialog 选择的正常路径 → 成功导入 |

### T1.3: NTFS data run 内存检查

| 属性 | 值 |
|------|-----|
| 编号 | H-03 |
| 文件 | `crates/fs-ntfs/src/lib.rs:270` (`read_attr_nonresident`) |
| 问题 | `read_attr_nonresident` line 249 有 `alloc_size > 128MB` 前置检查，但数据运行循环中 `buf.resize(start + chunk, 0)` 可超过 128MB — `chunk` 来自不受信的 data run `count` 值 |
| 方案 | `buf.resize()` 前添加：`if buf.len() + chunk as usize > 128 * 1024 * 1024 { return Err(InvalidData, "data run exceeds 128MB"); }` |
| 验收 | - [ ] 构造 data run 声称 >128MB → Err，不 OOM<br>- [ ] `cargo test --test mft_test` 通过 |

### T1.4: HTML 报告 XSS

| 属性 | 值 |
|------|-----|
| 编号 | H-04 |
| 文件 | `crates/reports/src/html/exporter.rs:18-30` |
| 问题 | `case.name`/`examiner`/`number`/文件路径直接 write! 进 HTML 无转义 |
| 方案 | `infrastructure::text` 添加 `fn html_escape(s: &str) -> String`（转义 `<>&"'`），exporter 中对所有动态内容调用 |
| 验收 | - [ ] `case.name = "<script>alert(1)</script>"` → 输出 `&lt;script&gt;`<br>- [ ] `cargo test` 通过 |

### T1.5: CSP 配置

| 属性 | 值 |
|------|-----|
| 编号 | H-05 |
| 文件 | `apps/desktop/src-tauri/tauri.conf.json` |
| 问题 | `app.security` 缺失 |
| 方案 | 添加 `"security": { "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost; font-src 'self'" }` |
| 验收 | - [ ] 应用正常启动，无 CSP 违规<br>- [ ] Tailwind 样式正常 |

### T1.6: 连接 Reports 导出功能 ⭐功能

| 属性 | 值 |
|------|-----|
| 编号 | F-01 (High) |
| 文件 | `frontend/src/app/pages/Reports.tsx:68-70`<br>`frontend/src/lib/api/reports.ts` |
| 问题 | "生成报告" 按钮已存在（`<Download /> 生成报告`）但 `onClick` 未连接后端命令。后端 `export_html/csv/json_report` 已实现 |
| 方案 | 1. `reports.ts` 添加 `exportHtmlReport()` / `exportCsvReport()` / `exportJsonReport()`，调用对应 Tauri command（无需参数 — 后端输出到 `case_root/reports/`）<br>2. `Reports.tsx` 添加 `useMutation` hook，"生成报告" 按钮 `onClick` 根据格式 `<select>` 调用对应函数<br>3. 成功/失败用 `sonner` toast 提示<br>4. 完成后 `invalidateQueries(['reports'])` 刷新历史 |
| 验收 | - [ ] 选择 HTML 格式 → 点击"生成报告" → 文件生成到 `case_root/reports/`<br>- [ ] 选择 CSV → 同上<br>- [ ] 历史列表自动刷新<br>- [ ] `pnpm typecheck` + `pnpm test` 通过 |

---

## Phase 2 — Medium Security + Medium Functional（4 天）

> 修复 8 个 Medium 安全 + 7 个 Medium 功能。

### T2.1: unsafe 生命周期修复

| 编号 | M-01 | 文件 | `file_service.rs:880,915-916` |
| 问题 | `cancel: Option<&AtomicBool>` 裸指针转 `'static` |
| 方案 | 参数改为 `cancel: Option<Arc<AtomicBool>>`，spawned thread 直接 move 所有权。`file_commands.rs:197` 传 `Some(cancel_token.clone())` |
| 验收 | - [ ] `unsafe` 块移除<br>- [ ] MFT 扫描取消功能正常<br>- [ ] `cargo clippy -D warnings` 通过 |

### T2.2: artifact 提取内存限制

| 编号 | M-02 | 文件 | `artifact_service.rs:31` |
| 问题 | `reader.read_to_end(&mut buf)` 无大小限制 |
| 方案 | 读取前检查文件大小（从 `FsNode.size`），超过 `ARTIFACT_FILE_LIMIT`（50MB）跳过并记录 warning |
| 验收 | - [ ] 100MB 文件 → 跳过 + warning<br>- [ ] 正常大小 → 正常提取 |

### T2.3: 分页 limit 上限

| 编号 | M-04 | 文件 | `transport/paging.rs:10`, `commands/mod.rs:93` |
| 问题 | `PageRequest.limit: u32` 和 `GetTimelineRequest.limit: u32` 无上限 |
| 方案 | 1. `PageRequest` 添加 `fn clamp(&mut self) { self.limit = self.limit.min(1000); }`<br>2. `GetTimelineRequest` 同理<br>3. 各 command handler 入口调用 |
| 验收 | - [ ] `limit = u32::MAX` → 实际 1000<br>- [ ] `limit = 50` → 不变 |

### T2.4: 移除 `From<String> for CommandError`

| 编号 | M-06 | 文件 | `transport/errors.rs:96-98` |
| 问题 | 绕过 `from_service_error()` 脱敏 |
| 方案 | 1. 删除 `impl From<String> for CommandError`<br>2. 全局搜索依赖此 impl 的调用点<br>3. 改为 `.map_err(CommandError::from_service_error)` |
| 验收 | - [ ] `cargo check` 通过<br>- [ ] 触发 DB 错误 → 前端收到泛化消息 |

### T2.5: DTO 输入校验

| 编号 | M-07 | 文件 | `transport/commands/mod.rs` |
| 问题 | `CreateCaseRequest`、`ImportDataSourceRequest`、`ViewerRangeRequestDto` 无校验 |
| 方案 | 各 Request 添加 `fn validate(&self) -> Result<(), CommandError>`，handler 入口调用 |
| 验收 | - [ ] `case_root = ""` → Err(INVALID_INPUT)<br>- [ ] `length = u32::MAX` → clamp 到 MAX_RANGE_LENGTH |

### T2.6: 迁移脚本幂等化

| 编号 | M-08 | 文件 | `persistence-sqlite/migrations/runner.rs:55`<br>`migrations/scripts/0010_job_partition_progress.sql` |
| 问题 | `conn.execute_batch(sql)` 不在显式事务中。`0010` 有 4 条 ALTER TABLE，部分成功后重试报 "duplicate column name" |
| 方案 | 1. 每个 script 包裹 `BEGIN;` / `COMMIT;`<br>2. `0010` 的 ALTER TABLE 添加列存在性检查（`SELECT * FROM pragma_table_info('jobs', 'current_partition')` 无结果才 ALTER）<br>3. 所有 `CREATE TABLE` / `CREATE INDEX` 添加 `IF NOT EXISTS` |
| 验收 | - [ ] 0010 部分失败后重试 → 成功<br>- [ ] `cargo test --test connection_test` 通过 |

### T2.7: CSV 公式注入净化

| 编号 | M-09 | 文件 | `reports/csv/exporter.rs` |
| 问题 | 未防 `=` `+` `-` `@` 开头公式注入 |
| 方案 | 每个 cell 首字符为 `=+-@` 时前缀 `\t` |
| 验收 | - [ ] `"=cmd\|'/C calc'!A0"` → 输出 `"\t=cmd\|..."` |

### T2.8: CI 硬化

| 编号 | M-12 | 文件 | `.github/workflows/ci-backend.yml`（新建 `ci-frontend.yml`） |
| 问题 | 无依赖审计、无前端 CI |
| 方案 | 1. `ci-backend.yml` 添加 `cargo audit` step<br>2. 新建 `ci-frontend.yml`：pnpm install → typecheck → lint → build → test |
| 验收 | - [ ] PR 触发 backend + frontend CI<br>- [ ] CI 红灯时无法合并 |

### T2.9: 取消导入 UI ⭐功能

| 编号 | F-05 | 文件 | `frontend/src/app/pages/CaseHome.tsx`<br>`frontend/src/lib/api/files.ts` |
| 问题 | 后端 `cancel_import` 已实现（`file_commands.rs:179`）但前端无取消按钮 |
| 方案 | 1. `files.ts` 添加 `cancelImport(jobId: string)` → `apiClient.request('cancel_import', ..., { jobId })`<br>2. CaseHome 导入进度条旁添加"取消"按钮（仅 `status === 'running'` 时显示）<br>3. 取消后 `invalidateQueries(['jobs'])` 刷新状态 |
| 验收 | - [ ] 导入中 → 显示取消按钮<br>- [ ] 点击取消 → job 停止，状态变更<br>- [ ] 取消后可重新导入 |

### T2.10: UI 按钮逻辑连接 ⭐功能

| 编号 | F-02, F-03, F-04 | 文件 | 见下方 |
| 问题 | 3 个按钮有 UI 但无点击逻辑 |

**T2.10a: "提取文件" (F-02)**
| 文件 | `FileBrowser.tsx:379` |
| 方案 | `onClick` → 调用 `tauri_plugin_dialog::save()` + 新建 `extract_file` Tauri 命令（将文件从 evidence 提取到用户选择的路径）。如提取命令暂不实现，可先 disabled + tooltip "即将支持" |
| 验收 | - [ ] 点击 → 弹出保存对话框 → 文件写入目标路径<br>- [ ] 或显示 "即将支持" tooltip |

**T2.10b: "在时间线中查看" (F-03)**
| 文件 | `FileBrowser.tsx:382` |
| 方案 | `onClick` → `useSelectionStore.setSelectedTimelineId(selectedFile.id)` + `useNavigate()('/timeline')` |
| 验收 | - [ ] 点击 → 跳转 Timeline 页，对应事件高亮 |

**T2.10c: "在文件浏览中打开" (F-04)**
| 文件 | `Search.tsx:136` |
| 方案 | `onClick` → `useSelectionStore.setSelectedFileId(selectedHit.fileId)` + `useNavigate()('/files')` |
| 验收 | - [ ] 点击 → 跳转 FileBrowser，对应文件高亮 |

### T2.11: 补充事件 emit ⭐功能

| 编号 | F-06 | 文件 | `apps/desktop/src-tauri/src/events/event_bridge.rs`<br>`apps/desktop/src-tauri/src/commands/*.rs` |
| 问题 | 7 个 topic 无 emit 函数。已有 emit 的 4 个：`job-progress`/`completed`/`failed` + `partition-progress` |
| 方案 | 1. `event_bridge.rs` 新增 5 个辅助函数（复用 `emit_event` 模式）：<br>  - `emit_case_opened(app, case_id, case_name)`<br>  - `emit_case_closed(app, case_id)`<br>  - `emit_artifact_added(app, artifact_id, artifact_type)`<br>  - `emit_timeline_updated(app, event_count)`<br>  - `emit_search_index_progress(app, progress, detail)`<br>2. `case_commands.rs::open_case` → 调用 `emit_case_opened`<br>3. `case_commands.rs::close_case` → 调用 `emit_case_closed`<br>4. `file_commands.rs` 导入流程 → 调用 `emit_job_created` + `emit_job_started`<br>5. `file_commands.rs` artifact 阶段 → 循环内调用 `emit_artifact_added`<br>6. `file_commands.rs` timeline 阶段 → 调用 `emit_timeline_updated` |
| 验收 | - [ ] 打开案件 → 前端 console 收到 `case-opened`<br>- [ ] 导入完成 → 收到 `artifact-added`、`timeline-updated`<br>- [ ] 11 个 topic 全部有 emit |

### T2.12: ArtifactRepo 空 ID 修复 ⭐功能

| 编号 | F-11 | 文件 | `persistence-sqlite/repositories/artifact_repo.rs:20-21` |
| 问题 | `insert_batch` 硬编码 `""` 为空 case_id/data_source_id |
| 方案 | 1. `Artifact` domain 类型添加 `case_id: CaseId` 和 `data_source_id: DataSourceId` 字段<br>2. `artifact_service::run_extractors_on_file` 传入当前 case_id/data_source_id<br>3. `insert_batch` 使用实际值 |
| 验收 | - [ ] 导入后查询 artifacts → 每条有正确 case_id/data_source_id<br>- [ ] 多 case → artifacts 按 case 隔离 |

### T2.13: 跨页面导航基础设施 ⭐功能

| 编号 | — (支撑 F-03, F-04) | 文件 | `frontend/src/app/pages/*.tsx` |
| 问题 | 无页面使用 `useNavigate()`，无法跨页跳转 |
| 方案 | 1. 在 `FileBrowser`、`Search`、`Timeline`、`Artifacts` 中引入 `useNavigate()` from `react-router`<br>2. selection store 的 ID 用于跨页状态传递（已存在）<br>3. 在 URL query param 或 zustand store 中传递锚点 ID |
| 验收 | - [ ] FileBrowser "在时间线中查看" → 跳转 Timeline 并高亮<br>- [ ] Search "在文件浏览中打开" → 跳转 FileBrowser 并选中<br>- [ ] `pnpm typecheck` 通过 |

---

## Phase 3 — Low Security + Functional Polish（3 天）

> 6 个 Low 安全 + 4 个 Low 功能。

### T3.1: LIKE 通配符转义

| 编号 | L-01 | 文件 | `file_repo.rs:145` |
| 方案 | `prefix.replace('%', "\\%").replace('_', "\\_")`，SQL 使用 `ESCAPE '\\'` |
| 验收 | - [ ] `prefix = "test%file"` → 只匹配字面量 |

### T3.2: EventTopic 枚举

| 编号 | L-06 | 文件 | `transport/events/mod.rs:16` |
| 方案 | `EventEnvelope.topic` 从 `String` 改为 `EventTopic` 枚举 |
| 验收 | - [ ] 反序列化未知 topic → serde 报错 |

### T3.3: 事件定向发送

| 编号 | L-03 | 文件 | `event_bridge.rs:10` |
| 方案 | `app.emit()` → `app.emit_to("main", ...)` |
| 验收 | - [ ] 单窗口行为不变 |

### T3.4: 日志路径脱敏

| 编号 | L-04 | 文件 | `main.rs:2-11` |
| 方案 | 日志级别 info → warn，或添加 file appender 写入安全目录 |
| 验收 | - [ ] stderr 无 evidence 路径 |

### T3.5: recent-cases.json 完整性

| 编号 | L-04 | 文件 | `case_commands.rs:278-336` |
| 方案 | 写入前校验每个 case_root 指向有效 case.json，目录权限 0o600 |
| 验收 | - [ ] 篡改 JSON 添加无效路径 → 打开时报错不崩溃 |

### T3.6: Settings 可编辑化 ⭐功能

| 编号 | F-07 | 文件 | `frontend/src/app/pages/Settings.tsx` |
| 问题 | 所有配置项只读展示 |
| 方案 | 1. 案件目录：可编辑 `<input>` + "保存"按钮<br>2. 镜像搜索路径：可编辑<br>3. 添加暗色/亮色主题切换（`next-themes` 已有依赖）<br>4. 设置写入 Tauri 后端 config（新建 `save_config` Tauri command）或 localStorage |
| 验收 | - [ ] 修改案件目录 → 保存后生效<br>- [ ] 主题切换 → localStorage 持久化<br>- [ ] `pnpm typecheck` 通过 |

### T3.7: Timeline 筛选 ⭐功能

| 编号 | F-08 | 文件 | `frontend/src/app/pages/Timeline.tsx`<br>`frontend/src/lib/api/timeline.ts` |
| 问题 | 无日期范围筛选器和事件类型过滤 |
| 方案 | 1. PageSubbar 添加日期范围选择器（`date-fns` 已有依赖，`react-day-picker` 已有）<br>2. 事件类型下拉过滤（从 `getTimelineEvents` 返回数据中提取唯一类型）<br>3. `TimelineRequest` 扩展 `startDate?` / `endDate?` / `eventType?` 参数<br>4. 后端 `get_timeline_events` 命令添加对应过滤逻辑 |
| 验收 | - [ ] 选择日期范围 → 结果按范围过滤<br>- [ ] 选择事件类型 → 仅显示匹配事件<br>- [ ] 空结果 → 显示友好提示 |

### T3.8: Search "保存的查询" ⭐功能

| 编号 | F-09 | 文件 | `frontend/src/app/pages/Search.tsx` |
| 问题 | "保存的查询" UI 存在但无保存/加载逻辑 |
| 方案 | 1. Zustand store 或 localStorage 存储 saved queries（name + query string）<br>2. 点击"保存的查询" → 下拉列表展示已保存查询<br>3. 点击条目 → 填入搜索框并执行<br>4. 添加"保存当前查询"按钮 |
| 验收 | - [ ] 保存查询 → 关闭重开后仍在<br>- [ ] 点击已保存查询 → 自动执行 |

### T3.9: FileBrowser 文本/图片预览 ⭐功能

| 编号 | F-10 | 文件 | `frontend/src/app/pages/FileBrowser.tsx` |
| 问题 | 文本预览和媒体预览为 placeholder |
| 方案 | 1. **文本预览**：后端 `read_file_range` 返回 `kind: 'text'` 时，前端渲染文本内容（检测编码）<br>2. **图片预览**：`mime` 为 `image/*` 时，后端返回 base64 → 前端 `<img src="data:${mime};base64,..." />`<br>3. 后端添加 `read_file_as_base64` Tauri command（或复用 `read_file_range` + base64 编码） |
| 验收 | - [ ] 选中 .txt → 文本预览显示内容<br>- [ ] 选中 .png → 图片预览显示图像<br>- [ ] 大文件 → 显示截断提示 |

---

## Phase 4 — Testing & CI（3 天）

### T4.1: 测试 fixture 管理

| 文件 | `crates/testing/src/fixtures/mod.rs` |
| 方案 | 创建小型测试 E01（<1MB）放入 `testdata/images/`，`testing` crate 提供 `fn test_e01_path()`，替换所有 `skip()` + 硬编码路径 |
| 验收 | - [ ] CI 无外部 E01 → 全部 E01 测试不再 skip |

### T4.2: 后端测试补充

| 方案 | `search_service_test.rs` + `timeline_service_test.rs` + `artifact_service_test.rs` + `transport` DTO roundtrip + `reports` 输出验证 |
| 验收 | - [ ] 新增 ≥20 测试，全通过 |

### T4.3: 前端测试补充

| 方案 | `search.test.ts` + `timeline.test.ts` + `artifacts.test.ts` + `reports.test.ts` + `jobs.test.ts` |
| 验收 | - [ ] 新增 ≥15 测试<br>- [ ] 前端 API 覆盖率 100% |

### T4.4: DevTools CI 守护

| 编号 | M-10 | 文件 | `ci-backend.yml` |
| 方案 | CI step 检查 tauri features 不含 `devtools` |
| 验收 | - [ ] CI 红灯阻止 devtools 泄露 |

### T4.5: 补充 event capability

| 编号 | M-11 | 文件 | `capabilities/default.json` |
| 方案 | 添加 `"event:default"` |
| 验收 | - [ ] 前端 `listen()` 正常 |

---

## Phase 5 — Code Quality & Backlog（2 天）

### T5.1: NTFS/FAT 枚举去重
| 文件 | `file_commands.rs:554-616` | 验收 | 代码行减少 ≥50，测试通过 |

### T5.2: 魔法数字常量化
| 文件 | `infrastructure/constants.rs` | 验收 | clippy 无 magic number 警告 |

### T5.3: SQL 迁移到 Repository
| 文件 | `case_commands.rs:78-88` | 验收 | Tauri command 层无 SQL |

### T5.4: 公共 API 文档注释
| 验收 | `cargo doc --workspace --no-deps` 无 warning |

### T5.5: 依赖版本锁定 + SBOM
| 验收 | CI 包含 `cargo deny` step |

### T5.6: Stub 模块 backlog（不阻断发布）
| 项 | 说明 |
|----|------|
| `fs-exfat` | exFAT 解析器实现 |
| `catalog` | 文件目录索引/投影 |
| `ingest` | 管线编排（当前逻辑在 app-services） |
| JumpList 解析器 | `artifacts-windows/src/jumplist/` |
| SRU 解析器 | `artifacts-windows/src/sru/` |
| Thumbcache 解析器 | `artifacts-windows/src/thumbcache/` |
| evidence_bundle 导出 | `reports/src/evidence_bundle/` |

---

## 执行顺序

```
Phase 0 ─→ Phase 1 ──┬─→ Phase 2 ──┬─→ Phase 4 ─→ Phase 5
(Critical)  (High)    │  (Medium)   │  (Testing)   (Quality)
                      │             │
                      └─→ Phase 3 ──┘
                        (Low/Polish)
```

- P0 → P1 串行（P0 是 P1 的前提）
- P2 + P3 可并行（安全 medium 和功能 polish 无交叉依赖）
- P4 依赖 P0-P2（测试需功能连通后才有意义）
- P5 最后收尾

## 统计

| Phase | 安全 | 功能 | 工期 | 验收条件 |
|-------|------|------|------|---------|
| P0 Critical | 3 | 0 | 1 天 | 10 |
| P1 High | 5 | 1 | 2 天 | 19 |
| P2 Medium | 8 | 7 | 4 天 | 38 |
| P3 Low | 5 | 4 | 3 天 | 17 |
| P4 Testing | 2 | 0 | 3 天 | 10 |
| P5 Quality | 1 | 0 | 2 天 | 6 |
| Backlog | — | 7 stub | — | — |
| **合计** | **24** | **12+7** | **~15 天** | **100** |

---

*v2.1 — Codex — 2026-05-29T18:30:00+08:00*  
*融合来源：安全审计 + 功能性审计 + v2.0 复核修正（4 处）*
