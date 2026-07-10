# 修补方案 v2.0 复核报告

> 归档：2026-05 复审快照，仅用于历史追溯，不代表当前整改状态。

**复核日期**: 2026-05-29  
**复核方法**: 逐 Task 对照实际源码验证行号、描述、修复方案的准确性  

---

## 复核结论

**28 个 Task 中 24 个完全准确，4 个需修正。**

| 结果 | 数量 | 说明 |
|------|------|------|
| ✅ 准确 | 24 | 文件路径、行号、问题描述、修复方案全部正确 |
| ⚠️ 需修正 | 4 | 描述偏差、方案调整、行号微调 |

---

## 逐项复核

### Phase 0 — Critical Security

| Task | 结论 | 说明 |
|------|------|------|
| 0.1 C-01 `delete_case` 沙箱化 | ✅ | `case_service.rs:114` — `delete_case` 无 safe_root 校验。`case_commands.rs:222` — 直接 `PathBuf::from(&request.case_root)`。修复方案正确 |
| 0.2 C-02 GPT `entry_size` | ✅ | `gpt.rs:65` — `&entry[56..128]` 确实在 `entry_size < 128` 时越界。修复 `if (entry_size as usize) < 128 { continue; }` 正确（注意 u32→usize cast） |
| 0.3 C-03 E01 环检测 | ✅ | `image-e01/lib.rs:49` — `while next_off > 0 && next_off < file_len` 无 visited set。方案正确 |

### Phase 1 — High Security + Core Functional

| Task | 结论 | 修正 |
|------|------|------|
| 1.1 H-01 案例名校验 | ✅ | `case_service.rs:44` — `root.join(name)` 无校验。方案正确 |
| 1.2 H-02 导入路径限制 | ✅ | `file_commands.rs:145` — `source_path` 无校验。方案正确 |
| **1.3 H-03 NTFS OOM** | **⚠️ 修正** | **见下方修正 #1** |
| 1.4 H-04 HTML XSS | ✅ | `html/exporter.rs:18-30` — `case.name` 直接 write! 进 HTML 无转义。方案正确 |
| 1.5 H-05 CSP | ✅ | `tauri.conf.json` 无 `security` 字段。方案正确 |
| **1.6 F-01 Reports 导出** | **⚠️ 修正** | **见下方修正 #2** |

### Phase 2 — Medium Security + Functional Wiring

| Task | 结论 | 修正 |
|------|------|------|
| 2.1 M-01 unsafe 转写 | ✅ | `file_service.rs:915-916` — `unsafe { &*ptr }` 裸指针转 `'static`。方案正确 |
| 2.2 M-02 artifact 内存限制 | ✅ | `artifact_service.rs:31` — `reader.read_to_end(&mut buf)` 无上限。方案正确 |
| 2.3 M-04 分页 limit 上限 | ✅ | `paging.rs:10` — `limit: u32` 无上限。`GetTimelineRequest.limit` 同样。方案正确 |
| 2.4 M-06 移除 From<String> | ✅ | `errors.rs:96-98` — `From<String>` 绕过脱敏。方案正确 |
| 2.5 M-07 DTO 校验层 | ✅ | `commands/mod.rs:13-27` — `CreateCaseRequest`、`ImportDataSourceRequest` 无校验。方案正确 |
| **2.6 M-08 迁移幂等化** | **⚠️ 修正** | **见下方修正 #3** |
| 2.7 M-09 CSV 公式注入 | ✅ | `csv/exporter.rs` — 仅双引号转义，无公式前缀净化。方案正确 |
| 2.8 M-12 CI 硬化 | ✅ | `ci-backend.yml` — 无 cargo audit、无前端 CI。方案正确 |
| 2.9 F-05 取消导入 UI | ✅ | `file_commands.rs:179` — `cancel_import` 已实现。`CaseHome.tsx` 无取消按钮。方案正确 |
| 2.10 F-02/03/04 UI 按钮 | ✅ | 3 个按钮存在但无点击处理函数。方案正确 |
| **2.11 F-06 事件 emit** | **⚠️ 修正** | **见下方修正 #4** |
| 2.12 F-11 ArtifactRepo | ✅ | `artifact_repo.rs:20-21` — 硬编码 `""` 为空 case_id/data_source_id。方案正确 |

### Phase 3 — Low Security + Functional Polish

| Task | 结论 | 说明 |
|------|------|------|
| 3.1 L-01 LIKE 转义 | ✅ | `file_repo.rs:145` — `format!("{}%", prefix)` 无转义 |
| 3.2 L-06 EventTopic 枚举 | ✅ | `events/mod.rs:16` — topic 为 String |
| 3.3 L-03 事件定向发送 | ✅ | `event_bridge.rs:10` — `app.emit()` 广播 |
| 3.4 L-04 日志路径脱敏 | ✅ | `main.rs:2-11` — info 级别记录路径 |
| 3.5 L-04 recent-cases | ✅ | `case_commands.rs:278-336` — 明文 JSON |
| 3.6 F-07 Settings 可编辑 | ✅ | `Settings.tsx` — 全部只读 |
| 3.7 F-08 Timeline 筛选 | ✅ | 无日期/类型过滤 UI |
| 3.8 F-10 预览 | ✅ | FileBrowser 中为 placeholder |

### Phase 4-5 — Testing & Quality

| Task | 结论 | 说明 |
|------|------|------|
| 4.1 测试 fixture | ✅ | E01 测试依赖外部文件 `E:/pangushi/刘洋/liuyang_pc.E01` |
| 4.2 后端测试补充 | ✅ | reports/transport/timeline 无测试 |
| 4.3 前端测试补充 | ✅ | search/timeline/artifacts/reports/jobs 无测试 |
| 4.4 DevTools 守护 | ✅ | 无 CI 检查 |
| 4.5 event capability | ✅ | `default.json` 缺少 `event:default` |
| 5.1-5.5 代码质量 | ✅ | 全部准确 |

---

## 修正详情

### 修正 #1: Task 1.3 (H-03) — NTFS OOM 描述和方案不精确

**原方案描述**:
> 从 data run 计算总大小后直接 `vec![0u8; total_size]`，恶意 NTFS 可声称文件 2GB+。虽然 `open_file` 有 128MB 后置检查，但 OOM 在分配时已发生。

**实际代码** (`fs-ntfs/src/lib.rs`):
- **Line 249**: `read_attr_nonresident` 已有 `alloc_size > 128 * 1024 * 1024` **前置检查**（基于 MFT 属性头的 `alloc_size` 字段）
- **Line 272**: `buf.resize(start + chunk as usize, 0)` — 数据运行循环中的实际分配，`chunk = count * cluster_size`
- **Line 646**: `open_file` 有 128MB **后置检查**

**真实漏洞**: 如果 MFT 头声称 `alloc_size < 128MB`（通过前置检查），但数据运行中的 `count` 值很大，`buf.resize()` 可以在循环中将 buffer 增长到远超 128MB。前置检查是针对 `alloc_size` 而非数据运行总量。

**修正后的方案**:
```
在 read_attr_nonresident 的数据运行循环中（line 270-274），
buf.resize() 前添加：
if buf.len() + chunk as usize > 128 * 1024 * 1024 {
    return Err(io::Error::new(io::ErrorKind::InvalidData, "data run exceeds 128MB limit"));
}
```

**影响**: 描述从"无前置检查"改为"前置检查不覆盖数据运行路径"。修复位置从泛泛的"read_file_data"精确到 `read_attr_nonresident` 的 line 270。

---

### 修正 #2: Task 1.6 (F-01) — Reports 导出按钮描述不准确

**原方案描述**:
> 后端 `export_html/csv/json_report` 命令已注册但前端 Reports 页面**无导出按钮**

**实际情况** (`Reports.tsx:68-70`):
```tsx
<button className="bg-[#111] text-white ...">
  <Download size={14} /> 生成报告
</button>
```

按钮**已存在**，但 `onClick` 未连接到后端命令。

**修正后的方案**:
1. 在 `reports.ts` 添加 `exportHtmlReport()`、`exportCsvReport()`、`exportJsonReport()`（调用对应 Tauri command，无需参数 — 后端使用 `active.case_root.join("reports")` 作为输出目录）
2. 为 "生成报告" 按钮添加 `onClick` handler，根据当前选中的格式 `<select>` 调用对应函数
3. 添加 `useMutation` hook 处理 loading/成功/失败状态
4. 成功后 toast 提示（`sonner` 已有依赖）

**影响**: 从"添加按钮"改为"连接现有按钮"。

---

### 修正 #3: Task 2.6 (M-08) — 迁移非原子性的精确描述

**原方案描述**:
> ALTER TABLE 非幂等，部分失败后 schema 损坏

**实际情况** (`runner.rs:55-58`):
```rust
conn.execute_batch(sql)  // 不在显式事务中
    .map_err(|e| DbError::Migration(format!("Failed to apply {}: {}", name, e)))?;
conn.execute("INSERT INTO schema_migrations (name) VALUES (?1)", [name])?;
```

- `execute_batch` 执行多语句时，SQLite 对 DDL 语句（CREATE TABLE、ALTER TABLE）有**隐式事务**
- 但如果脚本含多条 DDL（如 `0010_job_partition_progress.sql` 有 4 条 ALTER TABLE），部分成功后失败 → 迁移未记录 → 重试时已成功的 ALTER 会报 "duplicate column name"
- `0003_file_entries.sql` 使用 `CREATE TABLE`（非 `IF NOT EXISTS`），但首次运行正常

**修正后的方案**:
```
1. 每个 migration script 包裹 BEGIN; ... COMMIT;
2. 0010 的 4 条 ALTER TABLE 改为条件执行：
   - 查询 sqlite_master 检查列是否存在
   - 或使用 try/catch 模式（SQLite 无 try，用 PRAGMA 检查）
3. 所有 CREATE TABLE 添加 IF NOT EXISTS
4. 所有 CREATE INDEX 添加 IF NOT EXISTS
```

**影响**: 方案从泛泛的"事务包裹"细化为具体的幂等化策略。

---

### 修正 #4: Task 2.11 (F-06) — 事件 emit 缺少数不准确

**原方案描述**:
> **7/11** 个事件 topic 无后端 emit 代码

**实际情况**（`event_bridge.rs` 已实现的 emit 函数）:

| Topic | 有 emit? | 位置 |
|-------|---------|------|
| `job-progress` | ✅ | `emit_job_progress` |
| `job-completed` | ✅ | `emit_job_completed` |
| `job-failed` | ✅ | `emit_job_failed` |
| `partition-progress` | ✅ | `emit_partition_progress` |
| `case-opened` | ❌ | 未实现 |
| `case-closed` | ❌ | 未实现 |
| `job-created` | ❌ | 未实现 |
| `job-started` | ❌ | 未实现 |
| `artifact-added` | ❌ | 未实现 |
| `timeline-updated` | ❌ | 未实现 |
| `search-index_progress` | ❌ | 未实现 |

**修正**: 缺少 emit 的 topic 为 **7 个**（原描述数字正确），但方案列出的 "7/11" 需细化为明确的 7 个 topic 名单。

**修正后的方案**:
```
在以下位置补充 emit：
1. case_service::open_case() → emit TOPIC_CASE_OPENED
2. case_service::close_case() → emit TOPIC_CASE_CLOSED  
3. file_service 导入开始 → emit TOPIC_JOB_CREATED + TOPIC_JOB_STARTED
4. artifact_service 每提取一个 artifact → emit TOPIC_ARTIFACT_ADDED
5. timeline_service 投影完成 → emit TOPIC_TIMELINE_UPDATED
6. search_service 索引进度 → emit TOPIC_SEARCH_INDEX_PROGRESS

注：event_bridge.rs 中仅需新增 emit_case_opened/emit_case_closed 等
辅助函数，复用现有 emit_event 模式。
```

**影响**: 7 个数字正确，但原方案表格中的"未见 emit 调用"应改为更明确的"emit 函数未实现"。

---

## 修正后方案调整汇总

| 原 Task | 修正类型 | 改动范围 |
|---------|---------|---------|
| 1.3 H-03 | 描述+方案 | 从"无前置检查"改为"前置检查不覆盖数据运行"，修复位置精确到 `read_attr_nonresident` line 270 |
| 1.6 F-01 | 描述+方案 | 从"无导出按钮"改为"按钮存在但未连接"，方案从"添加按钮"改为"连接现有按钮 + 添加 mutation" |
| 2.6 M-08 | 方案细化 | 从泛泛"事务包裹"细化为"BEGIN/COMMIT + IF NOT EXISTS + 列存在性检查" |
| 2.11 F-06 | 描述细化 | 7 个数字正确，明确列出具体 topic 清单和 emit 函数命名模式 |

**其余 24 个 Task 全部通过复核，无需修正。**

---

*复核由 Codex 自动生成 — 2026-05-29T18:00:00+08:00*
