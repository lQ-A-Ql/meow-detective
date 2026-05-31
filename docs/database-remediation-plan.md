# 数据库修补方案

**项目**: Forensics 数字取证应用  
**范围**: hash_sha256 填充、case_id 修复、分区表规范化、审计日志  
**总工期**: 2 周 (10 个工作日)  

---

## 📅 Phase 1: 核心字段修复 (3 天)

> **目标**: 修复 hash_sha256 和 timeline.case_id 字段

---

### Task 1.1: 实现 SHA-256 哈希计算服务

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.1.1 | 创建 HashService | 封装哈希计算逻辑 | 服务可实例化 |
| 1.1.2 | 实现 sha256_file | 计算文件哈希 | 返回正确哈希 |
| 1.1.3 | 实现 sha256_reader | 从 Reader 计算 | 流式计算 |
| 1.1.4 | 添加错误处理 | IO 错误处理 | 不 panic |

#### 代码实现

```rust
// crates/app-services/src/hash_service.rs

use std::io::{self, Read};
use sha2::{Digest, Sha256};

/// 哈希计算服务
pub struct HashService;

impl HashService {
    /// 计算 Reader 的 SHA-256 哈希
    pub fn sha256_reader(reader: &mut dyn Read) -> io::Result<String> {
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        
        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        
        Ok(hex::encode(hasher.finalize()))
    }
    
    /// 计算文件的 SHA-256 哈希
    pub fn sha256_file(path: &std::path::Path) -> io::Result<String> {
        let mut file = std::fs::File::open(path)?;
        Self::sha256_reader(&mut file)
    }
    
    /// 计算字节切片的 SHA-256 哈希
    pub fn sha256_bytes(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.1.1 | 空文件哈希 | 空 Reader | e3b0c442... |
| T1.1.2 | 已知内容哈希 | "hello world" | b94d27b9... |
| T1.1.3 | 大文件哈希 | 10MB 数据 | 正确哈希 |
| T1.1.4 | IO 错误处理 | 无效路径 | 返回 Err |

---

### Task 1.2: 集成哈希计算到文件导入流程

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.2.1 | 修改 file_service | 添加哈希计算 | 代码可编译 |
| 1.2.2 | 逻辑目录导入 | 计算文件哈希 | 哈希正确存储 |
| 1.2.3 | 镜像导入 | 计算文件哈希 | 哈希正确存储 |
| 1.2.4 | 可选配置 | 是否计算哈希 | 配置生效 |

#### 代码修改

```rust
// crates/app-services/src/file_service/enumeration.rs

use crate::hash_service::HashService;

/// 枚举文件系统并计算哈希
pub fn enumerate_filesystem_with_hash(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
    root_name_override: Option<&str>,
    compute_hash: bool,
    source_path: &Path,
) -> DbResult<EnumerationStats> {
    // ... 现有逻辑 ...
    
    for child in children {
        let hash = if compute_hash && !child.is_dir {
            // 计算文件哈希
            match compute_file_hash(&child, source_path) {
                Ok(hash) => Some(hash),
                Err(e) => {
                    stats.warnings.push(format!("Hash error for {}: {}", child.name, e));
                    None
                }
            }
        } else {
            None
        };
        
        let entry = FileEntry {
            // ... 其他字段 ...
            hash_sha256: hash,
            // ...
        };
    }
}

/// 计算单个文件的哈希
fn compute_file_hash(
    child: &FsNode,
    source_path: &Path,
) -> Result<String, String> {
    let file_path = source_path.join(&child.path);
    HashService::sha256_file(&file_path)
        .map_err(|e| format!("Failed to compute hash: {}", e))
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.2.1 | 逻辑目录哈希 | 包含文件的目录 | hash_sha256 已填充 |
| T1.2.2 | 目录跳过哈希 | 目录条目 | hash_sha256 为 None |
| T1.2.3 | 哈希错误处理 | 无权限文件 | 记录警告，继续处理 |
| T1.2.4 | 禁用哈希计算 | compute_hash=false | hash_sha256 为 None |

---

### Task 1.3: 修复 timeline_events.case_id

**工期**: 0.5 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.3.1 | 修改 TimelineRepo | 接收 case_id 参数 | 接口变更 |
| 1.3.2 | 修改调用方 | 传递 case_id | 参数传递正确 |
| 1.3.3 | 数据迁移 | 修复历史数据 | 数据正确更新 |

#### 代码修改

```rust
// crates/persistence-sqlite/src/repositories/timeline_repo.rs

impl<'a> TimelineRepo<'a> {
    /// 插入时间线事件 (带 case_id)
    pub fn insert_batch_with_case(
        &self,
        events: &[TimelineEvent],
        case_id: &str,
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO timeline_events (id, case_id, source_object_id, event_type, ts, title, description, attrs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for ev in events {
                stmt.execute(params![
                    ev.id.0,
                    case_id,  // 现在填充 case_id
                    ev.source_object_id,
                    ev.event_type,
                    ev.timestamp.to_rfc3339(),
                    ev.title,
                    ev.description,
                    serde_json::to_string(&ev.attrs).unwrap_or_default(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
```

#### 数据迁移脚本

```sql
-- 0009_fix_timeline_case_id.sql
UPDATE timeline_events 
SET case_id = (
    SELECT ds.case_id 
    FROM file_entries fe
    JOIN data_sources ds ON fe.data_source_id = ds.id
    WHERE fe.id = timeline_events.source_object_id
)
WHERE case_id = '' AND source_object_id IN (
    SELECT id FROM file_entries
);
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.3.1 | 插入带 case_id | 事件 + case_id | case_id 已填充 |
| T1.3.2 | 查询按 case_id | case_id 过滤 | 返回正确结果 |
| T1.3.3 | 数据迁移 | 历史数据 | case_id 已修复 |
| T1.3.4 | 无关联数据 | source_object_id 不存在 | case_id 保持空 |

---

### Task 1.4: 添加数据库索引优化

**工期**: 0.5 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.4.1 | 分析慢查询 | 识别性能瓶颈 | 查询计划分析 |
| 1.4.2 | 添加复合索引 | 优化常用查询 | 索引创建成功 |
| 1.4.3 | 验证索引效果 | EXPLAIN 查询 | 查询计划改善 |

#### 索引添加

```sql
-- 0010_add_indexes.sql

-- file_entries 复合索引
CREATE INDEX idx_file_entries_type_deleted ON file_entries(entry_type, deleted);
CREATE INDEX idx_file_entries_hash ON file_entries(hash_sha256) WHERE hash_sha256 IS NOT NULL;
CREATE INDEX idx_file_entries_size ON file_entries(size);

-- timeline_events 复合索引
CREATE INDEX idx_timeline_case_type_ts ON timeline_events(case_id, event_type, ts);

-- artifacts 复合索引
CREATE INDEX idx_artifacts_case_type ON artifacts(case_id, artifact_type);
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.4.1 | 索引创建 | 执行迁移 | 无错误 |
| T1.4.2 | 按哈希查询 | hash_sha256 查询 | 使用索引 |
| T1.4.3 | 按类型+删除查询 | entry_type + deleted | 使用索引 |
| T1.4.4 | 按案件+时间查询 | case_id + ts 范围 | 使用索引 |

---

## 📅 Phase 2: 数据规范化 (3 天)

> **目标**: 创建 partitions 表，规范化分区数据

---

### Task 2.1: 创建 partitions 表

**工期**: 0.5 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.1.1 | 设计表结构 | 定义字段 | 结构合理 |
| 2.1.2 | 创建迁移脚本 | SQL 脚本 | 执行成功 |
| 2.1.3 | 添加索引 | 查询优化 | 索引存在 |

#### 表结构设计

```sql
-- 0011_create_partitions.sql

CREATE TABLE partitions (
    id TEXT PRIMARY KEY NOT NULL,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id),
    partition_index INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind_label TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'unsupported',
    type_guid TEXT,
    offset INTEGER NOT NULL DEFAULT 0,
    length INTEGER NOT NULL DEFAULT 0,
    filesystem TEXT,
    unlock_hint TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_partitions_data_source ON partitions(data_source_id);
CREATE INDEX idx_partitions_status ON partitions(status);
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.1.1 | 表创建 | 执行迁移 | 表存在 |
| T2.1.2 | 插入数据 | 分区信息 | 插入成功 |
| T2.1.3 | 外键约束 | 无效 data_source_id | 约束失败 |

---

### Task 2.2: 创建 PartitionRepo

**工期**: 0.5 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.2.1 | 创建 Repo 结构 | 定义方法 | 编译通过 |
| 2.2.2 | 实现 CRUD | 增删改查 | 功能正确 |
| 2.2.3 | 添加测试 | 单元测试 | 测试通过 |

#### 代码实现

```rust
// crates/persistence-sqlite/src/repositories/partition_repo.rs

use crate::connection::DbResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct PartitionRecord {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub name: String,
    pub kind_label: String,
    pub status: String,
    pub type_guid: Option<String>,
    pub offset: u64,
    pub length: u64,
    pub filesystem: Option<String>,
    pub unlock_hint: Option<String>,
}

pub struct PartitionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PartitionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    
    /// 插入分区记录
    pub fn insert(&self, record: &PartitionRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO partitions (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.id,
                record.data_source_id,
                record.partition_index,
                record.name,
                record.kind_label,
                record.status,
                record.type_guid,
                record.offset,
                record.length,
                record.filesystem,
                record.unlock_hint,
            ],
        )?;
        Ok(())
    }
    
    /// 批量插入分区记录
    pub fn insert_batch(&self, records: &[PartitionRecord]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO partitions (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for record in records {
                stmt.execute(params![
                    record.id,
                    record.data_source_id,
                    record.partition_index,
                    record.name,
                    record.kind_label,
                    record.status,
                    record.type_guid,
                    record.offset,
                    record.length,
                    record.filesystem,
                    record.unlock_hint,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    
    /// 按数据源查询分区
    pub fn find_by_data_source(&self, data_source_id: &str) -> DbResult<Vec<PartitionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint
             FROM partitions WHERE data_source_id = ?1 ORDER BY partition_index",
        )?;
        let rows = stmt.query_map(params![data_source_id], |row| {
            Ok(PartitionRecord {
                id: row.get(0)?,
                data_source_id: row.get(1)?,
                partition_index: row.get(2)?,
                name: row.get(3)?,
                kind_label: row.get(4)?,
                status: row.get(5)?,
                type_guid: row.get(6)?,
                offset: row.get(7)?,
                length: row.get(8)?,
                filesystem: row.get(9)?,
                unlock_hint: row.get(10)?,
            })
        })?;
        
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
    
    /// 删除数据源的所有分区
    pub fn delete_by_data_source(&self, data_source_id: &str) -> DbResult<usize> {
        let count = self.conn.execute(
            "DELETE FROM partitions WHERE data_source_id = ?1",
            params![data_source_id],
        )?;
        Ok(count)
    }
    
    /// 替换数据源的分区 (删除旧的，插入新的)
    pub fn replace_for_data_source(
        &self,
        data_source_id: &str,
        records: &[PartitionRecord],
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            // 删除旧分区
            tx.execute(
                "DELETE FROM partitions WHERE data_source_id = ?1",
                params![data_source_id],
            )?;
            
            // 插入新分区
            let mut stmt = tx.prepare_cached(
                "INSERT INTO partitions (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for record in records {
                stmt.execute(params![
                    record.id,
                    record.data_source_id,
                    record.partition_index,
                    record.name,
                    record.kind_label,
                    record.status,
                    record.type_guid,
                    record.offset,
                    record.length,
                    record.filesystem,
                    record.unlock_hint,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.2.1 | 插入单条 | 1 条记录 | 插入成功 |
| T2.2.2 | 批量插入 | 3 条记录 | 全部插入 |
| T2.2.3 | 按数据源查询 | data_source_id | 返回正确记录 |
| T2.2.4 | 删除分区 | data_source_id | 记录删除 |
| T2.2.5 | 替换分区 | 新记录列表 | 旧删除，新插入 |

---

### Task 2.3: 迁移现有分区数据

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.3.1 | 解析 JSON 数据 | 从 data_sources 提取 | 数据正确 |
| 2.3.2 | 插入新表 | 写入 partitions | 数据完整 |
| 2.3.3 | 验证数据 | 比对新旧数据 | 数据一致 |
| 2.3.4 | 清理旧数据 | 移除 JSON 字段 | 可选 |

#### 迁移脚本

```sql
-- 0012_migrate_partitions.sql

-- 从 data_sources.partitions JSON 迁移数据
-- 注意：此脚本需要在应用层执行，因为需要解析 JSON
```

#### 应用层迁移代码

```rust
// crates/persistence-sqlite/src/migrations/partition_migration.rs

use crate::connection::DbResult;
use rusqlite::Connection;
use serde_json::Value;

pub fn migrate_partitions(conn: &Connection) -> DbResult<()> {
    // 1. 获取所有 data_sources
    let mut stmt = conn.prepare("SELECT id, partitions FROM data_sources WHERE partitions IS NOT NULL")?;
    let rows: Vec<(String, String)> = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?.collect::<Result<Vec<_>, _>>()?;
    
    // 2. 解析 JSON 并插入新表
    for (ds_id, partitions_json) in rows {
        if let Ok(partitions) = serde_json::from_str::<Vec<Value>>(&partitions_json) {
            for (index, partition) in partitions.iter().enumerate() {
                let id = uuid::Uuid::new_v4().to_string();
                let name = partition["name"].as_str().unwrap_or("Unknown");
                let kind_label = partition["kind_label"].as_str().unwrap_or("Unknown");
                let status = partition["status"].as_str().unwrap_or("unsupported");
                let offset = partition["offset"].as_u64().unwrap_or(0);
                let length = partition["length"].as_u64().unwrap_or(0);
                
                conn.execute(
                    "INSERT INTO partitions (id, data_source_id, partition_index, name, kind_label, status, offset, length)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![id, ds_id, index, name, kind_label, status, offset, length],
                )?;
            }
        }
    }
    
    Ok(())
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.3.1 | 正常迁移 | 有效 JSON | 数据正确迁移 |
| T2.3.2 | 空 JSON | 空数组 | 无记录插入 |
| T2.3.3 | 无效 JSON | 格式错误 | 跳过，记录警告 |
| T2.3.4 | 数据完整性 | 迁移前后比对 | 记录数一致 |

---

### Task 2.4: 更新 store_data_source_partitions

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.4.1 | 修改函数签名 | 使用新 Repo | 编译通过 |
| 2.4.2 | 更新调用方 | 传递正确参数 | 功能正确 |
| 2.4.3 | 测试验证 | 端到端测试 | 分区正确存储 |

#### 代码修改

```rust
// crates/app-services/src/file_service.rs

use persistence_sqlite::repositories::partition_repo::{PartitionRepo, PartitionRecord};

pub fn store_data_source_partitions(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partitions: &[PartitionRecord],
) -> Result<(), String> {
    let repo = PartitionRepo::new(conn);
    repo.replace_for_data_source(&data_source_id.0, partitions)
        .map_err(|e| format!("Failed to store partitions: {}", e))
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.4.1 | 存储分区 | 分区列表 | 数据库中有记录 |
| T2.4.2 | 替换分区 | 新分区列表 | 旧记录删除，新记录插入 |
| T2.4.3 | 查询分区 | data_source_id | 返回正确分区 |

---

## 📅 Phase 3: 审计日志 (2 天)

> **目标**: 创建审计日志表，记录用户操作

---

### Task 3.1: 创建 audit_log 表

**工期**: 0.5 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.1.1 | 设计表结构 | 定义字段 | 结构合理 |
| 3.1.2 | 创建迁移脚本 | SQL 脚本 | 执行成功 |
| 3.1.3 | 添加索引 | 查询优化 | 索引存在 |

#### 表结构设计

```sql
-- 0013_create_audit_log.sql

CREATE TABLE audit_log (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT,
    user_id TEXT NOT NULL DEFAULT 'system',
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT NOT NULL DEFAULT '{}',
    ip_address TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_audit_log_case ON audit_log(case_id);
CREATE INDEX idx_audit_log_action ON audit_log(action);
CREATE INDEX idx_audit_log_created ON audit_log(created_at);
CREATE INDEX idx_audit_log_resource ON audit_log(resource_type, resource_id);
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.1.1 | 表创建 | 执行迁移 | 表存在 |
| T3.1.2 | 插入日志 | 日志记录 | 插入成功 |
| T3.1.3 | 索引查询 | 按 case_id | 使用索引 |

---

### Task 3.2: 创建 AuditLogRepo

**工期**: 0.5 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.2.1 | 创建 Repo 结构 | 定义方法 | 编译通过 |
| 3.2.2 | 实现 CRUD | 增删改查 | 功能正确 |
| 3.2.3 | 添加测试 | 单元测试 | 测试通过 |

#### 代码实现

```rust
// crates/persistence-sqlite/src/repositories/audit_repo.rs

use crate::connection::DbResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct AuditLogEntry {
    pub id: String,
    pub case_id: Option<String>,
    pub user_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: String,
    pub ip_address: Option<String>,
    pub created_at: String,
}

pub struct AuditRepo<'a> {
    conn: &'a Connection,
}

impl<'a> AuditRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    
    /// 记录操作日志
    pub fn log(
        &self,
        case_id: Option<&str>,
        user_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: &str,
    ) -> DbResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO audit_log (id, case_id, user_id, action, resource_type, resource_id, details)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, case_id, user_id, action, resource_type, resource_id, details],
        )?;
        Ok(())
    }
    
    /// 查询日志
    pub fn query(
        &self,
        case_id: Option<&str>,
        action: Option<&str>,
        limit: u32,
        offset: u64,
    ) -> DbResult<Vec<AuditLogEntry>> {
        let mut sql = String::from(
            "SELECT id, case_id, user_id, action, resource_type, resource_id, details, ip_address, created_at
             FROM audit_log WHERE 1=1"
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_index = 1;
        
        if let Some(cid) = case_id {
            sql.push_str(&format!(" AND case_id = ?{}", param_index));
            param_values.push(Box::new(cid.to_string()));
            param_index += 1;
        }
        
        if let Some(act) = action {
            sql.push_str(&format!(" AND action = ?{}", param_index));
            param_values.push(Box::new(act.to_string()));
            param_index += 1;
        }
        
        sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
        param_values.push(Box::new(limit));
        param_values.push(Box::new(offset));
        
        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(AuditLogEntry {
                id: row.get(0)?,
                case_id: row.get(1)?,
                user_id: row.get(2)?,
                action: row.get(3)?,
                resource_type: row.get(4)?,
                resource_id: row.get(5)?,
                details: row.get(6)?,
                ip_address: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }
    
    /// 统计日志数量
    pub fn count(&self, case_id: Option<&str>) -> DbResult<u64> {
        let (sql, params) = match case_id {
            Some(cid) => (
                "SELECT COUNT(*) FROM audit_log WHERE case_id = ?1".to_string(),
                vec![Box::new(cid.to_string()) as Box<dyn rusqlite::types::ToSql>],
            ),
            None => (
                "SELECT COUNT(*) FROM audit_log".to_string(),
                vec![],
            ),
        };
        
        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let count: i64 = stmt.query_row(params_refs.as_slice(), |r| r.get(0))?;
        Ok(count as u64)
    }
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.2.1 | 记录日志 | 操作信息 | 插入成功 |
| T3.2.2 | 按案件查询 | case_id | 返回正确记录 |
| T3.2.3 | 按操作查询 | action | 返回正确记录 |
| T3.2.4 | 分页查询 | limit/offset | 正确分页 |
| T3.2.5 | 统计数量 | case_id | 返回正确数量 |

---

### Task 3.3: 集成审计日志到关键操作

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.3.1 | 案件操作 | 创建/打开/删除案件 | 日志记录 |
| 3.3.2 | 数据源操作 | 导入/删除数据源 | 日志记录 |
| 3.3.3 | 报告操作 | 生成报告 | 日志记录 |
| 3.3.4 | 搜索操作 | 执行搜索 | 日志记录 |

#### 审计日志操作定义

```rust
// crates/domain/src/audit.rs

/// 审计操作类型
pub enum AuditAction {
    // 案件操作
    CaseCreate,
    CaseOpen,
    CaseClose,
    CaseDelete,
    
    // 数据源操作
    DataSourceImport,
    DataSourceDelete,
    DataSourceRename,
    
    // 文件操作
    FileView,
    FileExtract,
    
    // 搜索操作
    SearchExecute,
    
    // 报告操作
    ReportGenerate,
    ReportExport,
    
    // 工件操作
    ArtifactView,
    
    // 时间线操作
    TimelineView,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CaseCreate => "case.create",
            Self::CaseOpen => "case.open",
            Self::CaseClose => "case.close",
            Self::CaseDelete => "case.delete",
            Self::DataSourceImport => "datasource.import",
            Self::DataSourceDelete => "datasource.delete",
            Self::DataSourceRename => "datasource.rename",
            Self::FileView => "file.view",
            Self::FileExtract => "file.extract",
            Self::SearchExecute => "search.execute",
            Self::ReportGenerate => "report.generate",
            Self::ReportExport => "report.export",
            Self::ArtifactView => "artifact.view",
            Self::TimelineView => "timeline.view",
        }
    }
    
    pub fn resource_type(&self) -> &'static str {
        match self {
            Self::CaseCreate | Self::CaseOpen | Self::CaseClose | Self::CaseDelete => "case",
            Self::DataSourceImport | Self::DataSourceDelete | Self::DataSourceRename => "datasource",
            Self::FileView | Self::FileExtract => "file",
            Self::SearchExecute => "search",
            Self::ReportGenerate | Self::ReportExport => "report",
            Self::ArtifactView => "artifact",
            Self::TimelineView => "timeline",
        }
    }
}
```

#### 集成示例

```rust
// crates/app-services/src/case_service.rs

use domain::audit::AuditAction;
use persistence_sqlite::repositories::audit_repo::AuditRepo;

pub fn create_case(
    conn: &Connection,
    case_root: &Path,
    name: &str,
    examiner: Option<&str>,
) -> Result<CaseMeta, String> {
    // ... 现有逻辑 ...
    
    // 记录审计日志
    let audit = AuditRepo::new(conn);
    audit.log(
        None,
        "system",
        AuditAction::CaseCreate.as_str(),
        AuditAction::CaseCreate.resource_type(),
        Some(&case_id),
        &serde_json::json!({
            "name": name,
            "examiner": examiner,
            "case_root": case_root.to_string_lossy(),
        }).to_string(),
    ).map_err(|e| tracing::warn!("Audit log error: {}", e)).ok();
    
    Ok(case_meta)
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.3.1 | 创建案件日志 | 创建案件 | audit_log 有记录 |
| T3.3.2 | 导入数据源日志 | 导入数据源 | audit_log 有记录 |
| T3.3.3 | 搜索日志 | 执行搜索 | audit_log 有记录 |
| T3.3.4 | 报告日志 | 生成报告 | audit_log 有记录 |

---

## 📅 Phase 4: 测试验证 (2 天)

> **目标**: 全面测试验证，确保修补正确

---

### Task 4.1: 单元测试

**工期**: 1 天  
**负责**: 后端  

#### 测试列表

| 测试 ID | 模块 | 测试名称 | 验收标准 |
|---------|------|----------|----------|
| UT-001 | hash_service | sha256 空输入 | 返回正确哈希 |
| UT-002 | hash_service | sha256 已知输入 | 返回正确哈希 |
| UT-003 | hash_service | sha256 大文件 | 流式计算正确 |
| UT-004 | partition_repo | 插入分区 | 数据库有记录 |
| UT-005 | partition_repo | 查询分区 | 返回正确记录 |
| UT-006 | partition_repo | 替换分区 | 旧删除，新插入 |
| UT-007 | audit_repo | 记录日志 | 数据库有记录 |
| UT-008 | audit_repo | 查询日志 | 返回正确记录 |
| UT-009 | audit_repo | 按案件查询 | 过滤正确 |
| UT-010 | timeline_repo | 带 case_id 插入 | case_id 已填充 |

---

### Task 4.2: 集成测试

**工期**: 0.5 天  
**负责**: 后端  

#### 测试列表

| 测试 ID | 测试名称 | 测试步骤 | 验收标准 |
|---------|----------|----------|----------|
| IT-001 | 文件导入带哈希 | 1. 导入逻辑目录<br>2. 查询 file_entries | hash_sha256 已填充 |
| IT-002 | 时间线带 case_id | 1. 导入数据源<br>2. 查询 timeline_events | case_id 已填充 |
| IT-003 | 分区数据迁移 | 1. 执行迁移<br>2. 查询 partitions | 数据完整 |
| IT-004 | 审计日志记录 | 1. 创建案件<br>2. 查询 audit_log | 日志存在 |

---

### Task 4.3: 性能测试

**工期**: 0.5 天  
**负责**: 后端  

#### 测试列表

| 测试 ID | 测试名称 | 测试条件 | 验收标准 |
|---------|----------|----------|----------|
| PT-001 | 大量文件哈希 | 10,000 文件 | < 30 秒 |
| PT-002 | 索引效果 | 1,000,000 记录 | 查询 < 100ms |
| PT-003 | 批量插入 | 10,000 条记录 | < 5 秒 |
| PT-004 | 并发写入 | 10 线程 | 无死锁 |

---

## 📊 验收标准汇总

### Phase 1 验收标准

- [ ] hash_sha256 字段在文件导入时正确填充
- [ ] 目录类型的 hash_sha256 为 NULL
- [ ] timeline_events.case_id 正确填充
- [ ] 历史数据迁移完成
- [ ] 新增索引创建成功
- [ ] 所有单元测试通过

### Phase 2 验收标准

- [ ] partitions 表创建成功
- [ ] PartitionRepo 功能完整
- [ ] 现有分区数据迁移完成
- [ ] 数据一致性验证通过
- [ ] 所有单元测试通过

### Phase 3 验收标准

- [ ] audit_log 表创建成功
- [ ] AuditRepo 功能完整
- [ ] 关键操作已集成审计日志
- [ ] 日志记录正确
- [ ] 所有单元测试通过

### Phase 4 验收标准

- [ ] 所有单元测试通过 (≥ 20 个)
- [ ] 所有集成测试通过 (≥ 4 个)
- [ ] 性能测试达标
- [ ] 无数据丢失
- [ ] 无编译警告

---

## 📋 交付物清单

| 交付物 | 文件路径 | 说明 |
|--------|----------|------|
| HashService | `crates/app-services/src/hash_service.rs` | 哈希计算服务 |
| PartitionRepo | `crates/persistence-sqlite/src/repositories/partition_repo.rs` | 分区仓库 |
| AuditRepo | `crates/persistence-sqlite/src/repositories/audit_repo.rs` | 审计日志仓库 |
| AuditAction | `crates/domain/src/audit.rs` | 审计操作定义 |
| 迁移脚本 | `crates/persistence-sqlite/src/migrations/scripts/0009-0013.sql` | 5 个迁移脚本 |
| 单元测试 | 各模块 tests 目录 | ≥ 20 个测试 |
| 集成测试 | `crates/persistence-sqlite/tests/` | ≥ 4 个测试 |

---

## 📅 甘特图

```
Week 1                    Week 2
│                         │
├─ Phase 1 (3d) ──────────┤
│  ├─ Task 1.1 (1d)       │
│  ├─ Task 1.2 (1d)       │
│  ├─ Task 1.3 (0.5d)     │
│  └─ Task 1.4 (0.5d)     │
│                         │
│  ├─ Phase 2 (3d) ───────┤
│  │  ├─ Task 2.1 (0.5d)  │
│  │  ├─ Task 2.2 (0.5d)  │
│  │  ├─ Task 2.3 (1d)    │
│  │  └─ Task 2.4 (1d)    │
│  │                      │
│  │  ├─ Phase 3 (2d) ────┤
│  │  │  ├─ Task 3.1 (0.5d)
│  │  │  ├─ Task 3.2 (0.5d)
│  │  │  └─ Task 3.3 (1d) │
│  │  │                   │
│  │  │  ├─ Phase 4 (2d) ─┤
│  │  │  │  ├─ Task 4.1 (1d)
│  │  │  │  ├─ Task 4.2 (0.5d)
│  │  │  │  └─ Task 4.3 (0.5d)
│  │  │  │               │
└──┴──┴──┴────────────────┘
```

---

**方案版本**: v1.0  
**制定人**: MiMo AI Assistant  
**日期**: 2026-05-30
