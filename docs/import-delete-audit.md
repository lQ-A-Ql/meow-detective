# 导入/删除阶段数据库与磁盘读写审计

**审计日期**: 2026-05-31  
**审计范围**: 导入流程、删除流程、数据库操作、磁盘 I/O  

---

## 📊 审计结果总览

| 类别 | 问题数 | 严重 | 中等 | 轻微 |
|------|--------|------|------|------|
| 导入流程 | 4 | 1 | 2 | 1 |
| 删除流程 | 3 | 0 | 2 | 1 |
| 数据库操作 | 3 | 0 | 2 | 1 |
| 磁盘 I/O | 2 | 0 | 1 | 1 |
| **总计** | **12** | **1** | **7** | **4** |

---

## 一、导入流程审计

### 1.1 导入流程数据库操作

```
┌─────────────────────────────────────────────────────────────────┐
│                      导入流程数据库操作                          │
├─────────────────────────────────────────────────────────────────┤
│  1. INSERT INTO jobs                  ← 创建任务                 │
│  2. INSERT INTO data_sources          ← 附加数据源               │
│  3. INSERT INTO file_entries (批量)   ← 枚举文件                 │
│  4. UPDATE jobs (progress)            ← 更新进度                 │
│  5. INSERT INTO data_source_partitions ← 存储分区信息             │
│  6. INSERT INTO timeline_events       ← 时间线投影               │
│  7. INSERT INTO artifacts             ← 工件提取                 │
│  8. UPDATE jobs (complete/fail)       ← 完成/失败                │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 发现的问题

#### IMP-001: 批量插入无事务保护 [严重]

**位置**: `file_service/mod.rs:168-181`

```rust
// 当前实现
while let Some((parent_id, dir_path)) = queue.pop_front() {
    // ... 构建 batch
    if batch.len() >= batch_size {
        repo.insert_batch(&batch)?;  // 每批次单独事务
        batch.clear();
    }
}
```

**问题**: 
- 每批次单独事务，如果中途失败，部分数据已写入
- 无法保证原子性

**建议**: 使用单个大事务包裹整个枚举过程

```rust
let tx = conn.unchecked_transaction()?;
// ... 所有 insert_batch 操作
tx.commit()?;
```

---

#### IMP-002: 文件哈希计算阻塞 I/O [中等]

**位置**: `file_service/enumeration.rs:195-220`

```rust
for entry in file_entries {
    let file_path = source_root.join(&entry.path);
    match HashService::sha256_file(&file_path) {
        Ok(hash) => { /* 更新数据库 */ }
        Err(e) => { /* 记录警告 */ }
    }
}
```

**问题**:
- 同步读取每个文件计算哈希
- 大量小文件时 I/O 密集

**建议**: 
- 使用异步 I/O
- 批量读取文件
- 限制并发数

---

#### IMP-003: 进度更新频繁 [中等]

**位置**: `pipeline.rs:598`

```rust
job_repo.update_progress(j, overall.min(65), &format!("{root_name} {pct}%"));
```

**问题**:
- 每处理一个分区都更新进度
- 频繁数据库写入

**建议**: 批量更新进度（每 100 个文件更新一次）

---

#### IMP-004: 后处理流水线串行执行 [轻微]

**位置**: `pipeline.rs:94-165`

```rust
// 时间线投影
let tl_count = timeline_service::project_and_store_macb(conn, &all_files)?;

// 工件提取
for file in all_files.iter().take(ARTIFACT_EXTRACTION_LIMIT) { ... }

// 文本索引
let index_result = search_service::index_files(conn, index_dir, &to_index, &reader_fn);
```

**问题**:
- 三个阶段串行执行
- 可以并行化

**建议**: 使用并行处理

---

## 二、删除流程审计

### 2.1 删除案件流程

```
┌─────────────────────────────────────────────────────────────────┐
│                      删除案件流程                                │
├─────────────────────────────────────────────────────────────────┤
│  1. validate_case_root_is_safe()  ← 安全验证                    │
│  2. 检查 case.json 存在           ← 验证是案件目录              │
│  3. fs::remove_dir_all()          ← 递归删除目录                │
│  4. 重试机制 (最多5次)            ← Windows 兼容性              │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 删除数据源流程

```
┌─────────────────────────────────────────────────────────────────┐
│                      删除数据源流程                              │
├─────────────────────────────────────────────────────────────────┤
│  1. BEGIN TRANSACTION                                         │
│  2. DELETE FROM artifacts WHERE source_object_id IN (...)      │
│  3. DELETE FROM timeline_events WHERE source_object_id IN (...)│
│  4. DELETE FROM file_entries WHERE data_source_id = ?          │
│  5. DELETE FROM data_source_partitions WHERE data_source_id = ?│
│  6. DELETE FROM data_sources WHERE id = ?                      │
│  7. COMMIT                                                     │
└─────────────────────────────────────────────────────────────────┘
```

### 2.3 发现的问题

#### DEL-001: 删除案件不清理数据库 [中等]

**位置**: `case_service.rs:184-217`

```rust
pub fn delete_case(root: &Path) -> Result<()> {
    // 只删除目录，不清理数据库
    fs::remove_dir_all(root)?;
}
```

**问题**:
- 如果数据库文件在其他位置，不会被清理
- 数据库连接可能未关闭

**建议**: 
- 先关闭数据库连接
- 显式删除数据库文件
- 清理相关缓存

---

#### DEL-002: 级联删除无大小限制 [中等]

**位置**: `datasource_repo.rs:57-86`

```rust
pub fn delete_cascade(&self, data_source_id: &DataSourceId) -> DbResult<()> {
    let tx = self.conn.unchecked_transaction()?;
    // 删除所有关联数据，无大小限制
    tx.execute("DELETE FROM artifacts WHERE ...")?;
    tx.execute("DELETE FROM timeline_events WHERE ...")?;
    tx.execute("DELETE FROM file_entries WHERE ...")?;
    // ...
    tx.commit()?;
}
```

**问题**:
- 大量数据时可能长时间锁定数据库
- 无法取消删除操作

**建议**: 
- 分批删除
- 添加进度回调
- 支持取消

---

#### DEL-003: 删除后索引未清理 [轻微]

**位置**: 删除流程未清理 Tantivy 索引

**问题**: 
- 删除数据源后，搜索索引仍保留
- 可能返回已删除文件的搜索结果

**建议**: 删除数据源时同步清理索引

---

## 三、数据库操作审计

### 3.1 外键约束分析

| 表 | 外键 | ON DELETE | 评价 |
|----|------|-----------|------|
| data_sources | case_id → cases(id) | 无 | 🟡 需要级联 |
| file_entries | parent_id → file_entries(id) | 无 | ✅ 自引用 |
| file_entries | data_source_id | 无 | 🟡 需要级联 |
| jobs | case_id → cases(id) | 无 | 🟡 需要级联 |
| reports | case_id → cases(id) | 无 | 🟡 需要级联 |
| tags | case_id → cases(id) | 无 | 🟡 需要级联 |
| data_source_partitions | data_source_id | CASCADE | ✅ 正确 |

### 3.2 发现的问题

#### DB-001: 缺少外键级联删除 [中等]

**问题**: 多数外键没有 ON DELETE CASCADE

**建议**: 
```sql
ALTER TABLE data_sources 
  ADD CONSTRAINT fk_data_sources_case 
  FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE;

ALTER TABLE jobs 
  ADD CONSTRAINT fk_jobs_case 
  FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE;
```

---

#### DB-002: 索引覆盖不完整 [中等]

**缺失索引**:
- `timeline_events(source_object_id)` — 删除时查询
- `artifacts(source_object_id)` — 删除时查询

**建议**: 
```sql
CREATE INDEX idx_timeline_source ON timeline_events(source_object_id);
CREATE INDEX idx_artifacts_source ON artifacts(source_object_id);
```

---

#### DB-003: 无审计日志记录删除操作 [轻微]

**问题**: 删除案件/数据源时未记录审计日志

**建议**: 删除前记录审计日志

---

## 四、磁盘 I/O 审计

### 4.1 导入阶段 I/O 分析

| 阶段 | 操作 | 大小 | 频率 |
|------|------|------|------|
| 文件枚举 | 读取目录 | 4KB | 每目录 |
| 文件读取 | 读取文件内容 | 变化 | 每文件 |
| 哈希计算 | 读取文件 | 8KB 缓冲 | 每文件 |
| 数据库写入 | 写入 SQLite | 页大小 | 批量 |

### 4.2 发现的问题

#### IO-001: 无缓冲读取优化 [中等]

**位置**: `skip_reader_bytes()`

```rust
let mut buffer = vec![0u8; 65536]; // 64KB
```

**问题**: 虽然已有 64KB 缓冲区，但未针对顺序读取优化

**建议**: 使用 `BufReader` 包装

---

#### IO-002: 临时文件未清理 [轻微]

**位置**: 导入过程中可能创建临时文件

**建议**: 使用 `tempfile` crate 并确保清理

---

## 📋 修复优先级

| 优先级 | 问题 | 工时 | 影响 |
|--------|------|------|------|
| **P0** | IMP-001: 批量插入无事务 | 1 天 | 数据一致性 |
| **P1** | DEL-002: 级联删除无限制 | 1 天 | 性能 |
| **P1** | DB-001: 缺少外键级联 | 0.5 天 | 数据完整性 |
| **P1** | DB-002: 索引覆盖不完整 | 0.5 天 | 性能 |
| **P2** | IMP-002: 哈希计算阻塞 | 2 天 | 性能 |
| **P2** | DEL-001: 删除不清理数据库 | 1 天 | 数据清理 |
| **P2** | IO-001: 无缓冲优化 | 0.5 天 | 性能 |
| **P3** | 其他轻微问题 | 1 天 | 代码质量 |

---

## ✅ 验收标准

### 导入流程

- [ ] 批量插入使用单一事务
- [ ] 进度更新合理频率
- [ ] 后处理可并行执行
- [ ] 失败时能回滚

### 删除流程

- [ ] 级联删除有大小限制
- [ ] 删除后索引同步清理
- [ ] 删除操作记录审计日志

### 数据库

- [ ] 外键级联删除完整
- [ ] 索引覆盖关键查询
- [ ] 删除操作有审计日志

---

**审计人**: MiMo AI Assistant  
**审计版本**: v1.0
