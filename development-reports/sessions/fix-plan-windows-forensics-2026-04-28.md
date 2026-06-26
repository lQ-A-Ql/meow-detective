# Windows 取证模块修复方案

**基于**：`deep-module-audit-2026-04-28.md` 审计事实  
**范围**：仅 Windows 取证相关模块  
**编制日期**：2026-04-28  
**工程准则**：单一职责、接口隔离、测试先行、渐进式重构

---

## 0. 开发边界与工程准则

### 0.1 不可触碰的边界

| 约束 | 说明 |
|------|------|
| **domain crate 零变更** | 本次修复不修改 `crates/domain/`，除非 ID 类型变更不可避免 |
| **transport DTO 不改名** | 仅允许新增字段（`skip_serializing_if`），不允许删除/重命名现有字段 |
| **persistence 迁移向前** | 任何 schema 变更必须新增迁移脚本，编号递增，不可修改已有迁移 |
| **evidence-core 接口冻结** | `EvidenceReader` trait 和 `FileSystemReader` trait 签名不变 |
| **前端契约单向同步** | Rust DTO 先定稿 → 前端 TypeScript 镜像跟进 |
| **每个 PR ≤ 500 行变更** | 超过 500 行的变更必须拆分为前置 PR |
| **每个函数 ≤ 200 行** | 超过 200 行的函数必须拆分 |
| **每个文件 ≤ 1500 行** | 超过 1500 行的文件必须拆分 |

### 0.2 强制质量门禁（每个 PR 必须通过）

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p app-services <specific_test>
cargo test -p fs-ntfs
cargo test -p artifacts-windows

# Frontend
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test

# Guard scripts
powershell -File scripts/check-command-sql-boundary.ps1
powershell -File scripts/check-media-protocol-guard.ps1
powershell -File scripts/check-release-guard.ps1
```

### 0.3 分支策略

```
main (保护分支)
  └─ stage-1-critical-fixes (Stage 1 全部完成后合并)
       └─ stage-2-high-priority (Stage 2 全部完成后合并)
            └─ stage-3-medium-priority (Stage 3 合并)
```

每个 Stage 合并前必须通过完整的 CI 矩阵。

---

## Stage 1: Critical — 架构违规与致命缺陷

**目标**：消除运行时 panic 风险和架构分层违规  
**预估工期**：2 周  
**风险等级**：🔴 高（影响系统稳定性）

---

### Phase 1.1: 消除 `unimplemented!()` 生产路径

**原则**：所有公开 trait 方法必须返回 `Err(Unsupported)` 而非 panic。

#### Task 1.1.1: 为所有 FileSystemReader stub 方法返回正确错误

**涉及文件**：
| 文件 | 行号 | 当前行为 | 目标行为 |
|------|------|---------|---------|
| `fs-ext4/src/lib.rs` | 446 | `unimplemented!()` | `Err(io::Error::new(Unsupported, "ext4: {method} not yet implemented"))` |
| `fs-exfat/src/lib.rs` | 328 | `unimplemented!()` | 同上 |
| `fs-btrfs/src/lib.rs` | 813 | `unimplemented!()` | 同上 |
| `fs-apfs/src/lib.rs` | 830 | `unimplemented!()` | 同上 |
| `fs-hfsplus/src/lib.rs` | 73 | `unimplemented!()` | 同上 |
| `fs-xfs/src/lib.rs` | 597 | `unimplemented!()` | 同上 |
| `fs-ntfs/src/ads.rs` | 226 | `unimplemented!()` | 同上 |
| `parallel_enum/mod.rs` | 155 | `unimplemented!()` | `Err(ParallelEnumError::Unsupported)` |

**实现模式**：
```rust
// 每个 crate 的 lib.rs 中添加统一的不支持错误构造器
fn unsupported_method(method: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{}: {} not yet implemented", crate_name!(), method),
    )
}

// trait 实现方法改为：
fn open_file_by_inode(&self, _inode: u64) -> io::Result<Vec<u8>> {
    Err(unsupported_method("open_file_by_inode"))
}
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| stub 方法返回 Unsupported | 调用任何 stub 方法 | `io::ErrorKind::Unsupported` | 不 panic |
| 错误消息包含 crate 名 | 同上 | 错误消息包含 "ext4:" 等 | 消息可读 |
| 调用栈不包含 panic | 同上 | 正常返回 Err | 无 stacktrace |

**验收标准**：
- [ ] 8 个 `unimplemented!()` 全部替换
- [ ] `cargo test -p fs-ext4 -p fs-btrfs -p fs-xfs -p fs-apfs -p fs-hfsplus -p fs-ntfs` 全部通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 零警告

---

### Phase 1.2: 消除 MFT 代码重复

**原则**：单一实现，多处引用。

#### Task 1.2.1: 提取共享 NTFS MFT 工具模块

**涉及文件**：
| 操作 | 文件 |
|------|------|
| 新建 | `crates/app-services/src/ntfs_mft_utils.rs`（或 `crates/fs-ntfs/src/mft_utils.rs`） |
| 修改 | `crates/app-services/src/file_service/mft.rs` |
| 修改 | `crates/app-services/src/parallel_enum/ntfs_mft.rs` |
| 修改 | `crates/app-services/src/lib.rs`（添加 `mod ntfs_mft_utils`） |

**提取的函数**：
```rust
// ntfs_mft_utils.rs
pub fn apply_ntfs_record_fixup(record: &mut [u8], sector_size: usize) -> io::Result<()>;
pub fn parse_mft_data_runs_from_record(record: &[u8]) -> io::Result<Vec<(i64, u64)>>;
pub fn parse_ntfs_data_runs(data: &[u8]) -> io::Result<Vec<(i64, u64)>>;
pub fn read_ntfs_mft_stream(...) -> io::Result<Vec<u8>>;
pub fn read_sized_le(data: &[u8], offset: usize, size: usize) -> Option<u64>;
pub fn read_sized_le_signed(data: &[u8], offset: usize, size: usize) -> Option<i64>;
```

**迁移步骤**：
1. 创建 `ntfs_mft_utils.rs`，从 `file_service/mft.rs` 复制函数签名和实现
2. 在 `file_service/mft.rs` 中 `use crate::ntfs_mft_utils::*`
3. 在 `parallel_enum/ntfs_mft.rs` 中 `use crate::ntfs_mft_utils::*`
4. 删除 `parallel_enum/ntfs_mft.rs` 中的重复实现
5. 运行两个模块的测试确认无回归

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| fixup 正常记录 | 有效 MFT 记录 | 修正后记录 | CRC 校验通过 |
| fixup 损坏记录 | 无效 USA 计数 | `Err` | 不 panic |
| data runs 正常解析 | 标准 data runs | 正确 VCN→LCN 映射 | 与已知结果对比 |
| data runs 空记录 | 空 data runs | 空 Vec | 无 panic |
| 两个模块引用同一实现 | 运行 mft.rs 和 ntfs_mft.rs 测试 | 相同结果 | 无回归 |

**验收标准**：
- [ ] `ntfs_mft_utils.rs` 包含所有 6 个共享函数
- [ ] `file_service/mft.rs` 和 `parallel_enum/ntfs_mft.rs` 无重复实现
- [ ] `cargo test -p app-services` 通过
- [ ] 两个模块的测试用例均通过

---

### Phase 1.3: 修复 TOCTOU 竞态条件

**原则**：联合预留操作必须是原子的。

#### Task 1.3.1: 修复 `reserve_content_budget` 竞态

**涉及文件**：`crates/app-services/src/import_analysis/worker_runtime.rs:450-458`

**当前问题**：
```rust
// 两个原子操作之间存在不一致窗口
self.content_files_used.fetch_add(1, Ordering::Relaxed);
// ← 另一线程可能看到 files +1 但 bytes 未更新的状态
self.content_bytes_used.fetch_add(size, Ordering::Relaxed);
```

**修复方案**：
```rust
// 方案 A：Mutex 保护（简单可靠）
pub fn reserve_content_budget(&self, size: u64) -> bool {
    let mut state = self.budget_lock.lock().unwrap_or_else(|e| e.into_inner());
    if state.files_used + 1 > state.max_files
        || state.bytes_used + size > state.max_bytes
    {
        return false;
    }
    state.files_used += 1;
    state.bytes_used += size;
    true
}

// 方案 B：CAS 循环（无锁但更复杂）
// 不推荐，因为涉及两个变量的联合条件判断
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| 单线程预留成功 | files=0, bytes=0, limit=10 | `true` | files=1, bytes=size |
| 单线程预留超限 | files=max, limit=10 | `false` | files 不变 |
| 并发 100 线程预留 | 100 线程同时预留 | 总成功数 ≤ max_files | 无超额分配 |
| 并发预留后释放 | 预留→释放→再预留 | 可重新预留 | 无泄漏 |

**验收标准**：
- [ ] 竞态条件消除
- [ ] 并发测试通过（100 线程 × 1000 次操作）
- [ ] 无死锁

---

## Stage 2: High Priority — 架构合规与错误处理

**目标**：修复架构违规，建立错误处理规范  
**预估工期**：3-4 周  
**风险等级**：🟡 中（影响代码质量和可维护性）

---

### Phase 2.1: 消除 app-services 中的原始 SQL

**原则**：所有数据库操作必须通过 persistence-sqlite 仓库层。

#### Task 2.1.1: 扩展 persistence-sqlite 仓库层

**涉及文件**：
| 操作 | 文件 |
|------|------|
| 新建/扩展 | `crates/persistence-sqlite/src/repositories/staging_repo.rs` |
| 新建/扩展 | `crates/persistence-sqlite/src/repositories/timeline_ext_repo.rs` |
| 新建/扩展 | `crates/persistence-sqlite/src/repositories/graph_ext_repo.rs` |
| 新建/扩展 | `crates/persistence-sqlite/src/repositories/analysis_repo.rs` |
| 修改 | `crates/persistence-sqlite/src/repositories/mod.rs` |

**需要迁移的 SQL 模块**：

| 模块 | SQL 数量 | 优先级 | 迁移到 |
|------|---------|--------|--------|
| `staging/mod.rs` | ~30 | P0 | `staging_repo.rs` |
| `parallel_enum/ntfs_mft.rs` | ~12 | P0 | `staging_repo.rs` |
| `timeline_service.rs` | ~8 | P1 | `timeline_ext_repo.rs` |
| `graph_service.rs` | ~10 | P1 | `graph_ext_repo.rs` |
| `analysis_service/extraction/mod.rs` | ~8 | P2 | `analysis_repo.rs` |
| `entity_extraction.rs` | ~15 | P2 | `analysis_repo.rs` |
| `entity_resolution/merge.rs` | ~8 | P2 | `analysis_repo.rs` |
| `entity_resolution/relationships.rs` | ~8 | P2 | `analysis_repo.rs` |
| `correlation/graph.rs` | ~10 | P2 | `graph_ext_repo.rs` |
| `rule_pack/engine.rs` | ~5 | P3 | `analysis_repo.rs` |

**迁移模式**（以 staging 为例）：
```rust
// persistence-sqlite/src/repositories/staging_repo.rs

pub struct StagingRepo<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> StagingRepo<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    // 将 staging/mod.rs 中的原始 SQL 封装为方法
    pub fn insert_staging_entry(&self, entry: &StagingEntry) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO staging_entries (...) VALUES (?1, ?2, ...)",
            params![entry.id, entry.case_id, ...],
        )?;
        Ok(())
    }

    pub fn get_staging_entries(&self, case_id: &str) -> rusqlite::Result<Vec<StagingEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT ... FROM staging_entries WHERE case_id = ?1"
        )?;
        let rows = stmt.query_map(params![case_id], |row| {
            Ok(StagingEntry { ... })
        })?;
        rows.collect()
    }

    // ... 其他 staging SQL 操作
}
```

**Task 2.1.2: 修改 app-services 模块使用仓库层**

**实现模式**：
```rust
// app-services/src/staging/mod.rs

// 之前（违规）
pub fn get_staging_entries(conn: &Connection, case_id: &str) -> Result<Vec<StagingEntry>> {
    let mut stmt = conn.prepare("SELECT ... FROM staging_entries WHERE case_id = ?1")?;
    // ...
}

// 之后（合规）
pub fn get_staging_entries(repo: &StagingRepo, case_id: &str) -> Result<Vec<StagingEntry>> {
    repo.get_staging_entries(case_id)
        .map_err(|e| StagingError::Database(e))
}
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| 仓库层单元测试 | 构造测试数据 | 正确 CRUD | 所有仓库方法通过 |
| 集成测试 | app-services 调用仓库 | 相同结果 | 无回归 |
| SQL 边界脚本 | `check-command-sql-boundary.ps1` | 零违规 | 脚本通过 |
| 性能基准 | 10K 条记录操作 | 无明显退化 | ≤10% 性能损失 |

**验收标准**：
- [ ] `staging/mod.rs` 中 SQL 语句数量从 ~30 降至 0
- [ ] `parallel_enum/ntfs_mft.rs` 中 SQL 语句数量从 ~12 降至 0
- [ ] `check-command-sql-boundary.ps1` 通过
- [ ] 所有新增仓库方法有单元测试
- [ ] 集成测试无回归

---

### Phase 2.2: 建立类型化错误处理规范

**原则**：所有公共服务方法使用 `thiserror` 派生错误枚举。

#### Task 2.2.1: 为 staging 模块定义类型化错误

**涉及文件**：
| 操作 | 文件 |
|------|------|
| 新建 | `crates/app-services/src/staging/error.rs` |
| 修改 | `crates/app-services/src/staging/mod.rs` |

**实现**：
```rust
// staging/error.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StagingError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("staging entry not found: {0}")]
    NotFound(String),

    #[error("invalid staging state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("staging merge conflict: {0}")]
    MergeConflict(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

**Task 2.2.2: 批量修复 `Result<T, String>` 函数**

**涉及函数**（按优先级排序）：

| 优先级 | 文件 | 函数 | 当前签名 | 目标签名 |
|--------|------|------|---------|---------|
| P0 | `parallel_enum/ntfs_mft.rs:23` | `enumerate_ntfs_mft_to_staging` | `Result<..., String>` | `Result<..., NtfsMftError>` |
| P0 | `parallel_enum/ntfs_mft.rs:167` | `validate_mft_staging_shape` | `Result<(), String>` | `Result<(), NtfsMftError>` |
| P0 | `parallel_enum/ntfs_mft.rs:233` | `read_ntfs_mft_parameters` | `Result<NtfsMftParams, String>` | `Result<NtfsMftParams, NtfsMftError>` |
| P0 | `parallel_enum/ntfs_mft.rs:337` | `apply_ntfs_record_fixup` | `Result<(), String>` | `Result<(), NtfsMftError>` |
| P1 | `staging/analysis_merge.rs:19` | 多个函数 | `Result<..., String>` | `Result<..., StagingError>` |
| P1 | `staging/enum_merge.rs:23` | 多个函数 | `Result<..., String>` | `Result<..., StagingError>` |
| P2 | `v3_governance_service.rs:31` | 多个函数 | `Result<..., String>` | `Result<..., GovernanceError>` |

**迁移模式**：
```rust
// 之前
pub fn apply_ntfs_record_fixup(record: &mut [u8]) -> Result<(), String> {
    // ...
    Err("fixup signature mismatch".to_string())
}

// 之后
pub fn apply_ntfs_record_fixup(record: &mut [u8]) -> Result<(), NtfsMftError> {
    // ...
    Err(NtfsMftError::FixupSignatureMismatch)
}
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| 错误类型匹配 | 触发特定错误 | 对应错误枚举变体 | `matches!()` 通过 |
| 错误消息可读 | 同上 | 人类可读消息 | `Display` 实现正确 |
| 无 `String` 错误 | 所有公共方法 | 类型化错误 | grep 零匹配 |
| 向后兼容 | 调用方适配 | 编译通过 | 无 breaking change |

**验收标准**：
- [ ] P0 函数全部迁移完成
- [ ] `grep -r "Result<.*String>" crates/app-services/src/parallel_enum/` 零匹配
- [ ] `grep -r "Result<.*String>" crates/app-services/src/staging/` 零匹配
- [ ] 所有错误类型实现 `Display` + `std::error::Error`

---

### Phase 2.3: 修复 gql/ingest 架构违规

**原则**：中间层 crate 不得直接依赖持久层。

#### Task 2.3.1: 为 gql crate 引入 trait 抽象

**涉及文件**：
| 操作 | 文件 |
|------|------|
| 修改 | `crates/gql/src/engine.rs` |
| 新建 | `crates/gql/src/graph_store.rs`（trait 定义） |
| 修改 | `crates/gql/Cargo.toml`（移除 `persistence-sqlite` 依赖） |

**实现**：
```rust
// gql/src/graph_store.rs

pub trait GraphStore {
    fn get_node(&self, id: &str) -> Result<Option<GraphNode>, GraphStoreError>;
    fn get_edges(&self, node_id: &str, direction: Direction) -> Result<Vec<GraphEdge>, GraphStoreError>;
    fn traverse(&self, start: &str, max_depth: usize) -> Result<Vec<GraphTraverseResult>, GraphStoreError>;
}

// gql/src/engine.rs
pub struct GqlEngine<S: GraphStore> {
    store: S,
}

// persistence-sqlite 中实现 trait
impl GraphStore for GraphRepo<'_> {
    fn get_node(&self, id: &str) -> Result<Option<GraphNode>, GraphStoreError> {
        // 原有 SQL 实现
    }
    // ...
}
```

**Task 2.3.2: 为 ingest crate 引入 trait 抽象**

**涉及文件**：
| 操作 | 文件 |
|------|------|
| 修改 | `crates/ingest/src/graph_writer.rs` |
| 修改 | `crates/ingest/Cargo.toml`（移除 `persistence-sqlite` 依赖） |

**实现**：
```rust
// ingest/src/graph_writer.rs

pub trait GraphWriter: Send {
    fn write_node(&mut self, node: &GraphNode) -> Result<(), Box<dyn std::error::Error>>;
    fn write_edge(&mut self, edge: &GraphEdge) -> Result<(), Box<dyn std::error::Error>>;
    fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

// SqliteGraphWriter 实现移到 persistence-sqlite 或 app-services
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| gql 单元测试 | mock GraphStore | 正确结果 | 所有 gql 测试通过 |
| ingest 单元测试 | mock GraphWriter | 正确写入 | 所有 ingest 测试通过 |
| 依赖检查 | `cargo tree -p gql` | 无 persistence-sqlite | 依赖树干净 |
| 依赖检查 | `cargo tree -p ingest` | 无 persistence-sqlite | 依赖树干净 |

**验收标准**：
- [ ] `gql/Cargo.toml` 无 `persistence-sqlite` 依赖
- [ ] `ingest/Cargo.toml` 无 `persistence-sqlite` 依赖
- [ ] `cargo tree -p gql | grep persistence` 零匹配
- [ ] `cargo tree -p ingest | grep persistence` 零匹配
- [ ] 所有测试通过

---

## Stage 3: Medium Priority — 取证完整性与代码质量

**目标**：修复取证解析器缺口，清理代码债务  
**预估工期**：4-6 周  
**风险等级**：🟢 低（功能增强，非破坏性）

---

### Phase 3.1: NTFS 加密文件检测

**原则**：取证工具必须明确告知分析员证据不可访问的原因。

#### Task 3.1.1: 检测 $EFS 属性并标记

**涉及文件**：`crates/fs-ntfs/src/lib.rs`

**实现**：
```rust
// 在读取文件内容时检测 $EFS 属性
pub fn read_file_content(&self, inode: u64) -> io::Result<FileContent> {
    let record = self.read_mft_record(inode)?;
    let attributes = self.parse_attributes(&record)?;

    // 检测加密属性
    if attributes.iter().any(|a| a.attr_type == 0x80) { // $EFS
        return Ok(FileContent {
            data: vec![],
            is_encrypted: true,
            is_compressed: false,
            warning: Some("File is encrypted (EFS). Content unavailable without decryption key.".into()),
        });
    }

    // ... 原有逻辑
}

// 新增结构体
pub struct FileContent {
    pub data: Vec<u8>,
    pub is_encrypted: bool,
    pub is_compressed: bool,
    pub warning: Option<String>,
}
```

**Task 3.1.2: 前端显示加密标记**

**涉及文件**：
| 操作 | 文件 |
|------|------|
| 修改 | `crates/transport/src/dto/files.rs`（新增 `is_encrypted` 字段） |
| 修改 | `frontend/src/types/files.ts`（新增 `isEncrypted` 字段） |
| 修改 | `frontend/src/components/viewers/TextViewer.tsx`（显示警告） |

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| 加密文件检测 | 含 $EFS 属性的 MFT 记录 | `is_encrypted=true` | 标记正确 |
| 非加密文件 | 正常 $DATA 属性 | `is_encrypted=false` | 无误报 |
| 前端显示 | 打开加密文件 | 显示 "加密文件" 警告 | UI 正确 |
| 压缩文件 | 含 $DATA 压缩属性 | `is_compressed=true` | 标记正确 |

**验收标准**：
- [ ] 加密文件返回 `is_encrypted=true`
- [ ] 前端显示明确的加密警告
- [ ] 非加密文件无误报

---

### Phase 3.2: NTFS 压缩文件错误处理

**原则**：解压失败必须明确报告，不可静默降级。

#### Task 3.2.1: 替换静默降级为错误报告

**涉及文件**：`crates/fs-ntfs/src/lib.rs:1154`

**当前代码**：
```rust
// 危险：解压失败返回原始压缩数据
let decompressed = lznt1_decompress(&compressed_data)
    .unwrap_or_else(|_| compressed.to_vec()); // ← 问题
```

**修复为**：
```rust
let decompressed = lznt1_decompress(&compressed_data)
    .map_err(|e| io::Error::new(
        io::ErrorKind::InvalidData,
        format!("LZNT1 decompression failed for MFT record {}: {}", record_number, e),
    ))?;
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| 正常压缩数据 | 有效 LZNT1 数据 | 解压后数据 | 数据正确 |
| 损坏压缩数据 | 无效 LZNT1 数据 | `Err(InvalidData)` | 不返回垃圾 |
| 损坏数据错误消息 | 同上 | 包含 MFT record 号 | 可调试 |

**验收标准**：
- [ ] `unwrap_or_else(|_| compressed.to_vec())` 替换为 `?`
- [ ] 损坏压缩数据返回明确错误
- [ ] 错误消息包含 MFT record 号

---

### Phase 3.3: 拆分 `get_registry_structured_summary` 上帝函数

**原则**：每个 registry 家族独立查询/映射。

#### Task 3.3.1: 提取 per-family 查询 helper

**涉及文件**：`crates/app-services/src/analysis_service/extraction/mod.rs:247-665`

**拆分方案**：
```rust
// 新建 registry_family_queries.rs

pub fn query_sam_users(repo: &StagingRepo, case_id: &str) -> Result<Vec<SamUserDto>, AnalysisError> {
    // 原有 SAM 用户查询逻辑
}

pub fn query_user_assist(repo: &StagingRepo, case_id: &str) -> Result<Vec<UserAssistDto>, AnalysisError> {
    // 原有 UserAssist 查询逻辑
}

pub fn query_network_profiles(repo: &StagingRepo, case_id: &str) -> Result<Vec<NetworkProfileDto>, AnalysisError> {
    // 原有网络配置查询逻辑
}

// ... 其他 20+ 个家族

// 主函数变为聚合器
pub async fn get_registry_structured_summary(
    state: &AppState,
    case_id: &str,
) -> Result<RegistryStructuredSummaryDto, AnalysisError> {
    let repo = StagingRepo::new(&state.db_pool);

    // 并行查询各家族
    let (sam_users, user_assist, network_profiles, ...) = tokio::join!(
        query_sam_users(&repo, case_id),
        query_user_assist(&repo, case_id),
        query_network_profiles(&repo, case_id),
        // ...
    );

    Ok(RegistryStructuredSummaryDto {
        sam_users: sam_users?,
        user_assist: user_assist?,
        network_profiles: network_profiles?,
        // ...
    })
}
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| 每个 family 独立测试 | 构造测试数据 | 正确 DTO | 每个 helper 有单元测试 |
| 聚合测试 | 所有 family mock | 完整 Summary | 主函数测试通过 |
| 性能对比 | 10K 条记录 | 无明显退化 | ≤5% 性能损失 |
| 函数行数 | 新函数 | ≤200 行 | grep 验证 |

**验收标准**：
- [ ] 主函数行数从 418 行降至 ≤200 行
- [ ] 每个 family helper 有独立单元测试
- [ ] 所有现有测试通过

---

### Phase 3.4: 修复 `appcompatLayers` 大小写不匹配

**原则**：serde camelCase 转换必须与前端一致。

#### Task 3.4.1: 添加 `#[serde(alias)]` 或重命名

**涉及文件**：
| 操作 | 文件 |
|------|------|
| 修改 | `crates/transport/src/dto/analysis_registry.rs:426` |

**方案选择**：
```rust
// 方案 A：使用 serde alias（向后兼容）
#[serde(alias = "appcompatLayers")]
pub app_compat_layers: Vec<AppCompatLayerDto>,

// 方案 B：使用自定义 rename（推荐）
#[serde(rename = "appCompatLayers")]
pub app_compat_layers: Vec<AppCompatLayerDto>,
```

**测试矩阵**：
| 测试用例 | 输入 | 期望输出 | 通过条件 |
|---------|------|---------|---------|
| Rust 序列化 | 构造 DTO | `"appCompatLayers"` | JSON 字段名正确 |
| Rust 反序列化 | `"appCompatLayers"` JSON | 正确 DTO | 两种格式均可 |
| 前端接收 | Rust → IPC → 前端 | 字段存在 | 无 undefined |

**验收标准**：
- [ ] JSON 输出字段名为 `appCompatLayers`（大写 C）
- [ ] 前端能正确接收
- [ ] 向后兼容旧格式

---

## Stage 4: 文档更新与清理

**目标**：消除文档漂移，清理代码债务  
**预估工期**：1-2 周  
**风险等级**：🟢 低（无代码变更）

---

### Phase 4.1: 更新 AGENTS.md

#### Task 4.1.1: 修正数量声明

**涉及文件**：`AGENTS.md`

| 声明 | 当前值 | 修正为 |
|------|--------|--------|
| Tauri commands | 93 | 99 |
| Frontend pages | 10 | 16 |
| Frontend test files | 36 | 71 |
| Migration scripts | 31 | 32 |
| Event topics | 18 | 19 |

#### Task 4.1.2: 修正模块描述

**修正项**：
- `registry/lookup/` 下的 `system.rs`, `software.rs`, `ntuser.rs` 实际是目录
- `hash_decrypt.rs` 实际在 `registry/` 目录而非 `lookup/`
- 补充 8 个 guard scripts 到脚本列表

---

### Phase 4.2: 更新 design.md

#### Task 4.2.1: 修正结构描述

**修正项**：
- 前端路径：`apps/desktop/src/` → `frontend/`
- ingest 模块结构：更新为实际的 `pipeline.rs`, `sink.rs`, `stats.rs`
- 移除不存在的 traceability crate 描述
- 添加 V4 新 crate 描述（exchange, cloud-audit, gql, updater, crash_handler）
- 移除重复的 Sections 14-16

---

### Phase 4.3: 清理 `#[allow(dead_code)]`

#### Task 4.3.1: 审查并移除不必要的 dead_code 标注

**涉及文件**（按数量排序）：
| 文件 | 当前数量 | 目标 |
|------|---------|------|
| `fs-hfsplus/src/constants.rs` | 44 | 保留（格式常量豁免） |
| `fs-apfs/src/lib.rs` | 17 | 审查，移除未使用字段 |
| `fs-btrfs/src/lib.rs` | 9 | 审查 |
| `fs-xfs/src/lib.rs` | 9 | 审查 |
| `containers-pst/src/props.rs` | 7 | 审查 |
| `evtx-patched/src/utils/` | 6 | 保留（vendored） |
| `artifacts-linux/src/journal.rs` | 5 | 审查 |

**审查规则**：
- 格式常量（on-disk format）：保留
- 结构体字段（如果解析需要但未使用）：添加 `#[allow(dead_code)]` 注释说明
- 真正未使用的代码：删除

---

## 测试矩阵总览

### 单元测试（每个 Task 必须）

| 测试类型 | 覆盖率要求 | 工具 |
|---------|-----------|------|
| 函数级单元测试 | 100%（新代码） | `cargo test` |
| 错误路径测试 | 100%（新错误类型） | `cargo test` |
| 边界条件测试 | 关键路径 | `cargo test` |

### 集成测试（每个 Phase 必须）

| 测试类型 | 覆盖范围 | 工具 |
|---------|---------|------|
| 仓库层集成测试 | 新增仓库方法 | `cargo test -p persistence-sqlite` |
| 服务层集成测试 | 修改后的服务方法 | `cargo test -p app-services` |
| 端到端测试 | 关键用户流程 | `cargo test --workspace` |

### 回归测试（每个 Stage 必须）

| 测试类型 | 覆盖范围 | 工具 |
|---------|---------|------|
| 现有测试套件 | 全量 | `cargo test --workspace` |
| 前端测试 | 全量 | `pnpm --dir frontend test` |
| Guard scripts | 全量 | `powershell -File scripts/*.ps1` |

### 性能测试（Stage 2 & 3）

| 测试类型 | 基准 | 阈值 |
|---------|------|------|
| 仓库层 CRUD | 10K 记录 | ≤10% 退化 |
| 服务层查询 | 1K 记录 | ≤5% 退化 |
| 前端渲染 | 100 组件 | ≤100ms |

---

## 验收标准总览

### Stage 1 验收

- [ ] 8 个 `unimplemented!()` 全部替换为 `Err(Unsupported)`
- [ ] MFT 共享模块提取完成，重复代码消除
- [ ] TOCTOU 竞态条件修复，并发测试通过
- [ ] `cargo test --workspace` 全部通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 零警告

### Stage 2 验收

- [ ] `staging/mod.rs` SQL 从 ~30 降至 0
- [ ] `parallel_enum/ntfs_mft.rs` SQL 从 ~12 降至 0
- [ ] `check-command-sql-boundary.ps1` 通过
- [ ] P0 `Result<T, String>` 函数全部迁移为类型化错误
- [ ] `gql` 和 `ingest` 无 `persistence-sqlite` 依赖
- [ ] 所有新增代码有单元测试

### Stage 3 验收

- [ ] 加密文件返回 `is_encrypted=true`
- [ ] 压缩解压失败返回明确错误（不静默降级）
- [ ] `get_registry_structured_summary` 拆分为主函数 ≤200 行
- [ ] `appcompatLayers` 大小写修复
- [ ] 所有现有测试通过

### Stage 4 验收

- [ ] AGENTS.md 数量声明全部修正
- [ ] design.md 结构描述全部修正
- [ ] `#[allow(dead_code)]` 数量减少 ≥50%
- [ ] 文档与代码实际一致

---

## 风险评估

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| 仓库层重构引入回归 | 中 | 高 | 每个 Task 独立分支，完整测试后合并 |
| 错误类型迁移破坏调用方 | 低 | 中 | 使用 `#[non_exhaustive]` 枚举，渐进迁移 |
| 性能退化 | 低 | 中 | 每个 Stage 后运行性能基准 |
| 前端契约不同步 | 中 | 低 | Rust DTO 先定稿，前端跟进 |

---

## 时间线

```
Week 1-2:  Stage 1 (Critical)
  Week 1:  Phase 1.1 (unimplemented!) + Phase 1.2 (MFT 重复)
  Week 2:  Phase 1.3 (TOCTOU) + Stage 1 测试 + 合并

Week 3-6:  Stage 2 (High)
  Week 3-4: Phase 2.1 (SQL 迁移) - staging 模块
  Week 5:   Phase 2.2 (类型化错误) - P0 函数
  Week 6:   Phase 2.3 (架构违规) + Stage 2 测试 + 合并

Week 7-12: Stage 3 (Medium)
  Week 7-8:   Phase 3.1 (加密检测) + Phase 3.2 (压缩错误)
  Week 9-10:  Phase 3.3 (上帝函数拆分)
  Week 11:    Phase 3.4 (大小写修复)
  Week 12:    Stage 3 测试 + 合并

Week 13-14: Stage 4 (文档)
  Week 13: Phase 4.1-4.2 (文档更新)
  Week 14: Phase 4.3 (dead_code 清理) + 最终验证
```

---

## 附录：相关记忆链接

- [[rust-msvc-build-env]] — 构建环境配置
- [[file-tree-sorting-status]] — 文件树排序状态
- [[v4-development-status]] — V4 开发状态
