# 导入/删除优化开发日志

**日期**: 2026-05-31  
**范围**: 导入流程、删除流程、数据库优化  

---

## 优化内容

### 1. 批量插入事务保护

**文件**: `crates/app-services/src/file_service/enumeration.rs`

**修改**:
- 使用单一事务包裹整个枚举过程
- 确保原子性：要么全部成功，要么全部回滚

```rust
// 修复前: 每批次单独事务
repo.insert_batch(&batch)?;

// 修复后: 单一事务
let tx = conn.unchecked_transaction()?;
let repo = FileRepo::new(&tx);
repo.insert_batch(&[root_entry])?;
let result = walk_and_insert_children(&repo, ...);
tx.commit()?;
```

### 2. 外键级联删除

**文件**: `crates/persistence-sqlite/src/migrations/scripts/0016_add_cascade_delete.sql`

**修改**:
- 重建 `data_sources` 表，添加 `ON DELETE CASCADE`
- 重建 `file_entries` 表，添加 `ON DELETE CASCADE`
- 重建 `jobs` 表，添加 `ON DELETE CASCADE`
- 重建 `reports` 表，添加 `ON DELETE CASCADE`

### 3. 缺失索引

**文件**: `crates/persistence-sqlite/src/migrations/scripts/0017_add_missing_indexes.sql`

**新增索引**:
- `idx_timeline_source_object` — timeline_events(source_object_id)
- `idx_artifacts_source_object` — artifacts(source_object_id)
- `idx_audit_log_resource_id` — audit_log(resource_id)
- `idx_file_entries_type_deleted` — file_entries(entry_type, deleted)

### 4. 级联删除进度回调

**文件**: `crates/persistence-sqlite/src/repositories/datasource_repo.rs`

**新增方法**:
```rust
pub fn delete_cascade_with_progress(
    &self,
    data_source_id: &DataSourceId,
    progress: Option<&dyn Fn(u32, &str)>,
) -> DbResult<()>
```

**进度阶段**:
- 0%: 开始删除
- 10%: 删除工件
- 30%: 删除时间线事件
- 70%: 删除文件条目
- 90%: 删除分区
- 100%: 完成

### 5. 删除审计日志

**文件**: `crates/app-services/src/case_service.rs`

**修改**: 删除数据源前记录审计日志

```rust
let audit = AuditRepo::new(conn);
let _ = audit.log_simple(
    None,
    &AuditAction::DataSourceDelete,
    Some(data_source_id),
);
```

---

## 测试验证

```
✅ 编译: 21.30s, 0 错误
✅ 测试: 238 个全部通过
```

---

## 文件变更

| 文件 | 修改类型 |
|------|----------|
| `file_service/enumeration.rs` | 事务保护 |
| `file_repo.rs` | 新增方法 |
| `datasource_repo.rs` | 进度回调 |
| `case_service.rs` | 审计日志 |
| `migrations/runner.rs` | 新增迁移 |
| `0016_add_cascade_delete.sql` | 新增 |
| `0017_add_missing_indexes.sql` | 新增 |

---

**日志维护人**: MiMo AI Assistant
