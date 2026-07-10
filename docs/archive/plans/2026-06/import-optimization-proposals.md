# 案件与数据源导入流程优化方案

> 归档：2026-06 优化方案快照，仅用于历史追溯，不代表当前导入架构。

**项目**: Forensics 数字取证应用  
**范围**: 案件创建、数据源导入、后处理流水线  
**日期**: 2026-05-31  

---

## 📊 当前流程分析

### 案件创建流程

```
┌─────────────────────────────────────────────────────────┐
│                    案件创建流程                          │
├─────────────────────────────────────────────────────────┤
│  1. 验证案件名称                                         │
│  2. 创建目录结构 (cases/, exports/, indexes/)           │
│  3. 创建 SQLite 数据库                                  │
│  4. 运行数据库迁移 (15 个迁移脚本)                      │
│  5. 创建 CaseMeta 记录                                  │
│  6. 写入 case.json                                      │
│  7. 记录审计日志                                         │
└─────────────────────────────────────────────────────────┘
```

**当前问题**:
- ❌ 每次创建案件都重新运行所有迁移
- ❌ 没有案件模板支持
- ❌ 没有案件元数据验证

### 数据源导入流程

```
┌─────────────────────────────────────────────────────────┐
│                    数据源导入流程                         │
├─────────────────────────────────────────────────────────┤
│  1. 分类数据源类型 (E01/RAW/逻辑目录)                   │
│  2. 创建后台任务                                         │
│  3. 附加数据源到案件                                     │
│  4. 枚举文件系统                                         │
│     - 逻辑目录: 直接遍历                                │
│     - E01/RAW: 探测分区 → 枚举每个分区                  │
│  5. 运行后处理流水线                                     │
│     - 时间线投影                                         │
│     - 工件提取                                           │
│     - 文本索引                                           │
│  6. 完成任务                                             │
└─────────────────────────────────────────────────────────┘
```

**当前问题**:
- ❌ 导入过程中无法暂停/恢复
- ❌ 没有增量导入支持
- ❌ 大文件导入内存占用高
- ❌ 错误恢复机制不完善
- ❌ 没有导入进度详细报告

---

## 🎯 优化方案

### 方案 A: 增量导入 + 断点续传

**目标**: 支持导入暂停/恢复，避免重复工作

**实现**:
```rust
// 导入状态持久化
struct ImportState {
    job_id: String,
    data_source_id: String,
    phase: ImportPhase,
    processed_files: u64,
    total_files: u64,
    last_processed_path: Option<String>,
    errors: Vec<ImportError>,
}

enum ImportPhase {
    Classifying,
    Enumerating,
    PostProcessing,
    Completed,
    Failed,
    Paused,
}
```

**优势**:
- ✅ 大文件导入可暂停恢复
- ✅ 导入失败可从断点重试
- ✅ 减少重复工作

**工时**: 3-5 天

---

### 方案 B: 并行分区枚举

**目标**: 多分区并行枚举，提升导入速度

**实现**:
```rust
// 并行枚举多个分区
async fn enumerate_partitions_parallel(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partitions: Vec<PartitionInfo>,
) -> Result<EnumerationStats> {
    let handles: Vec<_> = partitions
        .into_iter()
        .map(|partition| {
            tokio::spawn(async move {
                enumerate_single_partition(partition).await
            })
        })
        .collect();
    
    // 等待所有分区完成
    let mut total_stats = EnumerationStats::default();
    for handle in handles {
        let stats = handle.await??;
        total_stats.merge(stats);
    }
    Ok(total_stats)
}
```

**优势**:
- ✅ 多分区并行处理
- ✅ 充分利用多核 CPU
- ✅ 导入速度提升 2-4x

**工时**: 2-3 天

---

### 方案 C: 流式文件处理

**目标**: 减少内存占用，支持大文件

**实现**:
```rust
// 流式处理文件，避免全量加载到内存
fn process_file_streaming(
    reader: &mut dyn Read,
    file_size: u64,
    processors: &mut [Box<dyn FileProcessor>],
) -> Result<()> {
    let mut buffer = vec![0u8; 65536]; // 64KB 缓冲区
    let mut total_read = 0;
    
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        
        // 流式处理每个处理器
        for processor in processors.iter_mut() {
            processor.process_chunk(&buffer[..bytes_read])?;
        }
        
        total_read += bytes_read as u64;
    }
    
    // 完成处理
    for processor in processors {
        processor.finalize()?;
    }
    
    Ok(())
}
```

**优势**:
- ✅ 内存占用降低 90%
- ✅ 支持超大文件 (10GB+)
- ✅ 处理速度更快

**工时**: 3-4 天

---

### 方案 D: 智能预检 + 优化路径

**目标**: 导入前预检，选择最优处理路径

**实现**:
```rust
// 导入前预检
fn pre_import_analysis(source_path: &Path) -> Result<ImportPlan> {
    let metadata = fs::metadata(source_path)?;
    let file_count = estimate_file_count(source_path)?;
    let total_size = metadata.len();
    
    // 根据特征选择最优策略
    let strategy = match (file_count, total_size) {
        (0..=1000, 0..=100_000_000) => ImportStrategy::Sequential,
        (1001..=10000, _) => ImportStrategy::Parallel { workers: 4 },
        (_, 100_000_001..) => ImportStrategy::Streaming,
        _ => ImportStrategy::Adaptive,
    };
    
    Ok(ImportPlan {
        strategy,
        estimated_time: estimate_import_time(file_count, total_size),
        memory_required: estimate_memory_usage(strategy),
    })
}
```

**优势**:
- ✅ 自动选择最优策略
- ✅ 预估导入时间
- ✅ 避免资源浪费

**工时**: 2-3 天

---

### 方案 E: 导入模板 + 批量操作

**目标**: 支持导入模板，批量导入多个数据源

**实现**:
```rust
// 导入模板
struct ImportTemplate {
    name: String,
    source_paths: Vec<PathBuf>,
    options: ImportOptions,
    post_processors: Vec<String>,
}

// 批量导入
async fn batch_import(
    conn: &Connection,
    template: ImportTemplate,
) -> Result<Vec<ImportResult>> {
    let mut results = Vec::new();
    
    for source_path in template.source_paths {
        let result = import_single_source(
            conn,
            &source_path,
            &template.options,
        ).await?;
        results.push(result);
    }
    
    Ok(results)
}
```

**优势**:
- ✅ 一键批量导入
- ✅ 可复用导入配置
- ✅ 减少重复操作

**工时**: 2-3 天

---

### 方案 F: 导入报告 + 可视化

**目标**: 生成详细导入报告，可视化导入过程

**实现**:
```rust
// 导入报告
struct ImportReport {
    data_source: DataSourceSummary,
    statistics: ImportStatistics,
    timeline: Vec<ImportEvent>,
    warnings: Vec<ImportWarning>,
    errors: Vec<ImportError>,
    performance: PerformanceMetrics,
}

struct ImportStatistics {
    total_files: u64,
    total_directories: u64,
    total_size: u64,
    imported_files: u64,
    skipped_files: u64,
    error_files: u64,
    duration: Duration,
    throughput: f64, // files per second
}
```

**优势**:
- ✅ 详细导入报告
- ✅ 性能指标统计
- ✅ 问题快速定位

**工时**: 2-3 天

---

## 📋 推荐实施计划

### Phase 1: 基础优化 (1 周)

| 方案 | 工时 | 优先级 | 依赖 |
|------|------|--------|------|
| D: 智能预检 | 2 天 | P0 | 无 |
| A: 增量导入 | 3 天 | P0 | D |

### Phase 2: 性能提升 (1 周)

| 方案 | 工时 | 优先级 | 依赖 |
|------|------|--------|------|
| B: 并行枚举 | 2 天 | P1 | 无 |
| C: 流式处理 | 3 天 | P1 | 无 |

### Phase 3: 功能增强 (1 周)

| 方案 | 工时 | 优先级 | 依赖 |
|------|------|--------|------|
| E: 导入模板 | 2 天 | P2 | A |
| F: 导入报告 | 3 天 | P2 | 无 |

---

## 📊 预期效果

| 指标 | 当前 | 优化后 | 提升 |
|------|------|--------|------|
| 10GB 镜像导入时间 | 30 min | 10 min | **3x** |
| 内存占用 (10GB) | 2 GB | 200 MB | **10x** |
| 导入失败恢复 | 全部重来 | 断点续传 | **∞** |
| 批量导入 | 手动逐个 | 一键批量 | **5x** |

---

## ✅ 验收标准

### Phase 1 验收

- [ ] 导入前显示预估时间
- [ ] 导入可暂停/恢复
- [ ] 导入失败可从断点重试

### Phase 2 验收

- [ ] 多分区并行枚举
- [ ] 大文件流式处理
- [ ] 内存占用 < 500MB

### Phase 3 验收

- [ ] 支持导入模板
- [ ] 生成详细导入报告
- [ ] 性能指标可视化

---

**方案版本**: v1.0  
**制定人**: MiMo AI Assistant  
**日期**: 2026-05-31
