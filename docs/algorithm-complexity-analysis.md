# 算法性能静态分析报告

**分析日期**: 2026-06-01
**分析范围**: 核心算法时间/空间复杂度  
**分析方法**: 代码审查 + 复杂度推导  

---

## 📊 复杂度总览

| 算法 | 时间复杂度 | 空间复杂度 | 评价 |
|------|------------|------------|------|
| 文件枚举 (BFS) | O(n) | O(w) | ✅ 最优 |
| 文件排序 | O(n log n) | O(n) | ✅ 最优 |
| SHA-256 哈希 | O(n) | O(1) | ✅ 最优 |
| Hex 格式化 | O(n) | O(n) | ✅ 最优 |
| 编码检测 | O(n) | O(1) | ✅ 最优 |
| Magic 分类 | O(s·(h + m·k)) | O(h) | ✅ bounded header |
| 路径重建 | O(n) | O(n) | ✅ 已优化 |
| MFT 批量扫描 | O(n) | O(p·b) | ✅ 可并行降低 wall time |
| 并行枚举 | O(n) | O(p) | ✅ 可并行降低 wall time |

---

## 一、文件枚举算法

### 算法描述

```rust
// crates/app-services/src/file_service/enumeration.rs
fn walk_and_insert_children(
    repo: &FileRepo<'_>,
    fs: &dyn FileSystemReader,
    data_source_id: &DataSourceId,
    root_id: FileEntryId,
    progress_fn: Option<&dyn Fn(u32)>,
) -> DbResult<EnumerationStats> {
    let mut queue: VecDeque<(FileEntryId, String)> = VecDeque::new();
    queue.push_back((root_id, String::new()));
    
    while let Some((parent_id, dir_path)) = queue.pop_front() {
        let children = fs.list_children(&dir_path)?;
        for child in children {
            // 插入数据库
            batch.push(entry);
            if child.is_dir {
                queue.push_back((id, child.path));
            }
        }
        // 批量插入
        if batch.len() >= batch_size {
            repo.insert_batch(&batch)?;
        }
    }
}
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(n) | 每个文件/目录访问一次 |
| 空间 | O(w) | w = 最大宽度（队列大小） |

**最佳情况**: O(n) - 平衡树  
**最坏情况**: O(n) - 链表结构  
**平均情况**: O(n)

### 优化空间

| 优化 | 效果 | 难度 |
|------|------|------|
| 批量插入 | 减少 IO 次数 | ✅ 已实现 |
| 并行枚举 | 多核加速 | ✅ 已实现 |

---

## 二、文件排序算法

### 算法描述

```typescript
// frontend/src/lib/file-sort.ts
export function sortFileEntries(
  rows: FileEntryRow[],
  sortKey: FileSortKey = 'name',
  direction: FileSortDirection = 'asc'
): FileEntryRow[] {
  // 预计算排序键
  const keysArray: SortKeys[] = new Array(len);
  for (let i = 0; i < len; i++) {
    keysArray[i] = computeSortKeys(rows[i]);
  }
  
  // 索引排序
  indices.sort((a, b) => {
    const ka = keysArray[a];
    const kb = keysArray[b];
    // 目录优先
    const fileDiff = ka.isFile - kb.isFile;
    if (fileDiff !== 0) return fileDiff * dirMul;
    // 按字段排序
    let cmp = 0;
    switch (sortKey) {
      case 'name':
        cmp = fastCompare(ka.nameLower, kb.nameLower);
        break;
      // ...
    }
    return cmp * dirMul;
  });
}
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(n log n) | 排序算法 |
| 空间 | O(n) | 索引数组 + 预计算 |

**优化点**:
- ✅ 预计算排序键，避免重复提取
- ✅ 使用索引排序，减少大对象移动
- ✅ 使用 `fastCompare` 替代 `localeCompare`

---

## 三、SHA-256 哈希算法

### 算法描述

```rust
// crates/infrastructure/src/hashing/mod.rs
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
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(n) | n = 数据大小 |
| 空间 | O(1) | 固定缓冲区 |

**优化点**:
- ✅ 流式处理，不加载全部数据
- ✅ 8KB 缓冲区平衡 IO 和内存

---

## 四、Hex 格式化算法

### 算法描述

```rust
// crates/app-services/src/file_service/mod.rs
fn format_hex_lines(base_offset: u64, bytes: &[u8]) -> Vec<String> {
    let line_count = bytes.len().div_ceil(16);
    let mut result = Vec::with_capacity(line_count);
    
    for (line_idx, chunk) in bytes.chunks(16).enumerate() {
        let offset = base_offset + (line_idx * 16) as u64;
        let mut line = String::with_capacity(8 + 2 + chunk.len() * 4);
        
        write!(line, "{offset:08X}");
        line.push_str("  ");
        
        for (i, byte) in chunk.iter().enumerate() {
            if i > 0 { line.push(' '); }
            write!(line, "{byte:02X}");
        }
        
        result.push(line);
    }
    result
}
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(n) | n = 字节数 |
| 空间 | O(n) | 输出字符串 |

**优化点**:
- ✅ 预分配容量
- ✅ 使用 `write!` 宏减少分配

---

## 五、编码检测算法

### 算法描述

```rust
// crates/app-services/src/text_service.rs
pub fn detect_encoding(data: &[u8]) -> EncodingInfo {
    // 1. BOM 检测 O(3)
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) { ... }
    
    // 2. UTF-8 验证 O(n)
    if std::str::from_utf8(data).is_ok() { ... }
    
    // 3. GBK 检测 O(n)
    let (decoded, _, errors) = GBK.decode(data);
    
    // 4. Shift-JIS 检测 O(n)
    let (decoded_sjis, _, errors_sjis) = SHIFT_JIS.decode(data);
    
    // 5. EUC-KR 检测 O(n)
    let (decoded_kr, _, errors_kr) = EUC_KR.decode(data);
}
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(n) | 最多扫描 5 次 |
| 空间 | O(1) | 固定变量 |

**优化建议**: 可以合并为单次扫描

---

## 六、Magic 分类算法

### 算法描述

```rust
// crates/app-services/src/analysis_service.rs
const MAGIC_HEADER_LIMIT: usize = 8 * 1024;

fn classify_files_by_magic(
    files: &[FileEntry],
    sample_size: u32,
    mut read_header_fn: impl FnMut(&FileEntryId) -> Result<Vec<u8>, String>,
) -> Vec<AnalysisFileClassificationDto> {
    for entry in files.iter().take(sample_size as usize) {
        let header = read_header_fn(&entry.id); // bounded header read
        let file_type = detect_file_type(&entry.path, header.as_deref().ok());
        // ...
    }
}

fn detect_file_type(path: &str, header: Option<&[u8]>) -> Option<...> {
    if let Some(data) = header {
        for sig in MAGIC_SIGNATURES {
            if data.len() >= sig.offset + sig.bytes.len() {
                if &data[sig.offset..sig.offset + sig.bytes.len()] == sig.bytes {
                    return Some(...);
                }
            }
        }
    }
    
    // 2. 扩展名回退
    let ext = Path::new(path).extension();
    // ...
}
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(s·(h + m·k)) | s = 样本数；h = header 上限；m = 签名数；k = 签名长度 |
| 空间 | O(h) | 单文件 bounded header buffer |

**当前**:

- `sampleSize` 默认 1000，命令层最大 5000。
- header 上限 8KB，不读取整文件。
- exact-length magic 已用 `>=` 判断，`%PDF`、`PK\x03\x04`、`regf` 等刚好等长样本可识别。
- 文件内容读取通过 `FileEntryId + DataSourceKind` helper，避免拼接宿主路径。

---

## 七、MFT 路径重建算法

### 算法描述

```rust
// crates/app-services/src/file_service/mod.rs
fn update_entry_paths(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> DbResult<()> {
    let mut resolved: HashMap<String, String> = HashMap::with_capacity(path_map.len());
    let mut visiting: HashSet<String> = HashSet::new();
    
    fn resolve_path(
        record: &str,
        path_map: &HashMap<String, (Option<String>, String, bool)>,
        resolved: &mut HashMap<String, String>,
        visiting: &mut HashSet<String>,
    ) -> String {
        if let Some(path) = resolved.get(record) {
            return path.clone();
        }
        if !visiting.insert(record.to_string()) {
            return String::new();
        }
        // parent recursion + cache
        // ...
    }
}
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(n) | 每个 record 最多解析并缓存一次 |
| 空间 | O(n) | resolved HashMap |

**当前**: 已采用递归 + 缓存 + `visiting` cycle detection，移除了原先最坏 O(n²) 的迭代收敛模型。

---

## 八、MFT 批量扫描算法

### 算法描述

```rust
// crates/app-services/src/file_service/mod.rs
pub fn enumerate_filesystem_mft(...) -> DbResult<EnumerationStats> {
    // Reader Thread → channel → Parser Thread Pool → channel → DB Writer Thread
    
    let num_parsers = num_cpus::get().clamp(2, 8);
    
    // 读取线程
    let reader_handle = thread::spawn(move || {
        for chunk in chunks {
            chunk_tx.send(chunk);
        }
    });
    
    // 解析线程池
    for parser_id in 0..num_parsers {
        thread::spawn(move || {
            for chunk in rx.iter() {
                let records = scanner.parse_chunk(&chunk.data, ...);
                tx.send(records);
            }
        });
    }
}
```

### 复杂度分析

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间 | O(n)；wall time 约 O(n/p) | p = parser 线程数；最终受 E01/RAW I/O 与 SQLite writer 约束 |
| 空间 | O(p·b) | p = 线程数, b = 缓冲区 |

**优化点**:
- ✅ 多线程并行解析
- ✅ 通道缓冲减少等待
- ✅ 批量插入数据库

---

## 📊 复杂度汇总表

| 算法 | 时间 | 空间 | 并行 | 评价 |
|------|------|------|------|------|
| 文件枚举 | O(n) | O(w) | ✅ | 最优 |
| 文件排序 | O(n log n) | O(n) | ❌ | 最优 |
| SHA-256 | O(n) | O(1) | ❌ | 最优 |
| Hex 格式化 | O(n) | O(n) | ❌ | 最优 |
| 编码检测 | O(n) | O(1) | ❌ | 可优化 |
| Magic 分类 | O(s·(h + m·k)) | O(h) | ❌ | bounded |
| 路径重建 | O(n) | O(n) | ❌ | 已优化 |
| MFT 扫描 | O(n) | O(p·b) | ✅ | 可并行降低 wall time |

---

## 🎯 优化建议

### P1 (短期)

| 问题 | 算法 | 建议 |
|------|------|------|
| 编码检测多次扫描 | detect_encoding | 合并为单次扫描 |
| 大媒体预览 | get_media_url/read_media_range | 小文件 data URL；大文件返回 scoped handle 并按 1MB 窗口读取。完整连续 streaming/protocol 仍待实现 |

### P2 (长期)

| 问题 | 算法 | 建议 |
|------|------|------|
| 排序不支持并行 | sortFileEntries | 分块并行排序 |

---

**分析人**: MiMo AI Assistant；2026-06-01 由 Codex 按当前实现更新媒体 range、Analysis 和 bounded preview 状态
**分析版本**: v1.1
