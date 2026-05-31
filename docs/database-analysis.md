# 数据库联动性分析报告

**项目**: Forensics 数字取证应用  
**数据库**: SQLite (每个案件独立)  
**日期**: 2026-05-30  

---

## 📊 数据库架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                      Forensics SQLite DB                        │
├──────────────┬──────────────┬──────────────┬───────────────────┤
│   案件管理    │   证据数据    │   分析结果    │     元数据        │
├──────────────┼──────────────┼──────────────┼───────────────────┤
│ cases        │ file_entries │ artifacts    │ tags              │
│ data_sources │              │ timeline     │ tag_bindings      │
│ jobs         │              │              │                   │
│ reports      │              │              │                   │
└──────────────┴──────────────┴──────────────┴───────────────────┘
```

---

## 📋 表结构详细分析

### 1. cases 表 — 案件管理

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 案件唯一标识 | 🔵 标识 |
| `name` | TEXT | 案件名称 | 🟡 基础 |
| `number` | TEXT | 案件编号 | 🟢 **高价值** — 案件关联 |
| `examiner` | TEXT | 检查员 | 🟢 **高价值** — 责任追溯 |
| `notes` | TEXT | 备注 | 🟡 中等 — 案件描述 |
| `created_at` | TEXT | 创建时间 | 🟡 基础 |
| `updated_at` | TEXT | 更新时间 | 🟡 基础 |

**价值评估**: ⭐⭐⭐ 中等 — 案件元数据，用于案件管理和追溯

---

### 2. data_sources 表 — 数据源

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 数据源标识 | 🔵 标识 |
| `case_id` | TEXT FK | 所属案件 | 🟡 关联 |
| `name` | TEXT | 数据源名称 | 🟡 基础 |
| `kind` | TEXT | 类型 (E01/RAW/逻辑目录) | 🟢 **高价值** — 证据类型 |
| `source_path` | TEXT | 原始路径 | 🟢 **高价值** — 证据来源 |
| `imported_at` | TEXT | 导入时间 | 🟡 基础 |

**价值评估**: ⭐⭐⭐⭐ 高 — 证据来源追踪

---

### 3. file_entries 表 — 文件条目 ⭐⭐⭐⭐⭐

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 文件标识 | 🔵 标识 |
| `parent_id` | TEXT FK | 父目录 | 🟡 结构 |
| `data_source_id` | TEXT | 数据源 | 🟡 关联 |
| `path` | TEXT | 文件路径 | 🟢 **高价值** — 文件位置 |
| `name` | TEXT | 文件名 | 🟢 **高价值** — 文件识别 |
| `entry_type` | TEXT | 类型 (file/directory) | 🟡 基础 |
| `size` | INTEGER | 文件大小 | 🟢 **高价值** — 异常检测 |
| `ext` | TEXT | 扩展名 | 🟢 **高价值** — 文件类型 |
| `deleted` | INTEGER | 是否删除 | 🔴 **极高价值** — 删除恢复 |
| `created_at` | TEXT | 创建时间 (MACB) | 🔴 **极高价值** — 时间线 |
| `modified_at` | TEXT | 修改时间 (MACB) | 🔴 **极高价值** — 时间线 |
| `accessed_at` | TEXT | 访问时间 (MACB) | 🔴 **极高价值** — 时间线 |
| `changed_at` | TEXT | 变更时间 (MACB) | 🔴 **极高价值** — 时间线 |
| `hash_sha256` | TEXT | SHA-256 哈希 | 🔴 **极高价值** — 完整性校验 |

**价值评估**: ⭐⭐⭐⭐⭐ 极高 — 核心取证数据

**关键字段说明**:
- **MACB 时间戳**: NTFS 文件系统的四个时间戳，是取证分析的核心
- **deleted**: 标记已删除文件，用于数据恢复分析
- **hash_sha256**: 用于证据完整性校验和文件去重

---

### 4. artifacts 表 — 取证工件 ⭐⭐⭐⭐⭐

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 工件标识 | 🔵 标识 |
| `case_id` | TEXT | 所属案件 | 🟡 关联 |
| `data_source_id` | TEXT | 数据源 | 🟡 关联 |
| `artifact_type` | TEXT | 工件类型 | 🔴 **极高价值** — 分类 |
| `source_object_id` | TEXT | 来源文件 | 🟢 **高价值** — 追溯 |
| `title` | TEXT | 标题 | 🟢 **高价值** — 摘要 |
| `summary` | TEXT | 摘要 | 🟢 **高价值** — 描述 |
| `attrs` | TEXT (JSON) | 扩展属性 | 🔴 **极高价值** — 详细数据 |
| `created_at` | TEXT | 创建时间 | 🟡 基础 |

**支持的工件类型**:

| 类型 | attrs 包含 | 取证价值 |
|------|-----------|----------|
| `LNK` | 目标路径、时间戳、文件大小 | 🔴 快捷方式分析 |
| `Prefetch` | 程序名、运行次数、运行时间 | 🔴 程序执行历史 |
| `RecycleBin` | 原始路径、删除时间、文件大小 | 🔴 删除文件恢复 |
| `Registry` | 注册表名、最后修改时间 | 🔴 系统配置分析 |
| `JumpList` | 最近访问文件列表 | 🟢 用户行为分析 |
| `SRU` | 系统资源使用记录 | 🟢 系统活动分析 |
| `ThumbCache` | 缩略图数据 | 🟢 文件预览 |

**价值评估**: ⭐⭐⭐⭐⭐ 极高 — 核心取证分析结果

---

### 5. timeline_events 表 — 时间线事件 ⭐⭐⭐⭐⭐

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 事件标识 | 🔵 标识 |
| `case_id` | TEXT | 所属案件 | 🟡 关联 |
| `source_object_id` | TEXT | 来源文件 | 🟢 **高价值** — 追溯 |
| `event_type` | TEXT | 事件类型 | 🔴 **极高价值** — 分类 |
| `ts` | TEXT | 时间戳 | 🔴 **极高价值** — 时间点 |
| `title` | TEXT | 标题 | 🟢 **高价值** — 摘要 |
| `description` | TEXT | 描述 | 🟢 **高价值** — 详情 |
| `attrs` | TEXT (JSON) | 扩展属性 | 🟡 中等 |

**支持的事件类型**:

| 事件类型 | 说明 | 取证价值 |
|----------|------|----------|
| `FILE_CREATED` | 文件创建 | 🔴 |
| `FILE_MODIFIED` | 文件修改 | 🔴 |
| `FILE_ACCESSED` | 文件访问 | 🔴 |
| `FILE_CHANGED` | 文件元数据变更 | 🔴 |
| `FILE_DELETED` | 文件删除 | 🔴 |
| `LINK_CREATED` | 快捷方式创建 | 🟢 |
| `LINK_MODIFIED` | 快捷方式修改 | 🟢 |
| `PROGRAM_EXECUTION` | 程序执行 | 🔴 |
| `REGISTRY_MODIFIED` | 注册表修改 | 🔴 |

**价值评估**: ⭐⭐⭐⭐⭐ 极高 — 时间线分析核心

---

### 6. jobs 表 — 任务管理

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 任务标识 | 🔵 标识 |
| `case_id` | TEXT FK | 所属案件 | 🟡 关联 |
| `kind` | TEXT | 任务类型 | 🟡 基础 |
| `status` | TEXT | 状态 | 🟡 基础 |
| `progress` | INTEGER | 进度 | 🟡 基础 |
| `detail` | TEXT | 详情 | 🟡 基础 |
| `created_at` | TEXT | 创建时间 | 🟡 基础 |
| `started_at` | TEXT | 开始时间 | 🟡 基础 |
| `finished_at` | TEXT | 完成时间 | 🟡 基础 |

**价值评估**: ⭐⭐ 低 — 运行时管理数据

---

### 7. reports 表 — 报告管理

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 报告标识 | 🔵 标识 |
| `case_id` | TEXT FK | 所属案件 | 🟡 关联 |
| `template_id` | TEXT | 模板 | 🟡 基础 |
| `file_name` | TEXT | 文件名 | 🟡 基础 |
| `created_by` | TEXT | 创建者 | 🟢 **高价值** — 责任追溯 |
| `status` | TEXT | 状态 | 🟡 基础 |
| `created_at` | TEXT | 创建时间 | 🟡 基础 |

**价值评估**: ⭐⭐⭐ 中等 — 报告管理

---

### 8. tags / tag_bindings 表 — 标签系统

| 字段 | 类型 | 说明 | 取证价值 |
|------|------|------|----------|
| `id` | TEXT PK | 标签标识 | 🔵 标识 |
| `case_id` | TEXT FK | 所属案件 | 🟡 关联 |
| `name` | TEXT | 标签名 | 🟢 **高价值** — 分类标记 |
| `color` | TEXT | 颜色 | 🟡 基础 |
| `object_id` | TEXT | 关联对象 | 🟢 **高价值** — 关联 |

**价值评估**: ⭐⭐⭐⭐ 高 — 证据标记和分类

---

## 🔍 数据联动分析

### 数据流图

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   E01/RAW   │────▶│ data_sources│────▶│ file_entries│
│   镜像文件   │     │   数据源     │     │   文件条目   │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                                               ▼
                    ┌─────────────┐     ┌─────────────┐
                    │   artifacts │◀────│   解析器     │
                    │   工件提取   │     │ LNK/Prefetch│
                    └──────┬──────┘     └─────────────┘
                           │
                           ▼
                    ┌─────────────┐     ┌─────────────┐
                    │   timeline  │────▶│   reports   │
                    │   时间线     │     │   报告生成   │
                    └─────────────┘     └─────────────┘
```

### 关键查询路径

| 查询 | 涉及表 | 索引使用 |
|------|--------|----------|
| 获取案件文件树 | file_entries | parent_id, data_source_id |
| 按时间范围查询 | timeline_events | (case_id, ts) |
| 按类型查询工件 | artifacts | artifact_type |
| 搜索文件 | file_entries | path, name |
| 获取删除文件 | file_entries | deleted |

---

## 📊 价值字段汇总

### 极高价值字段 (🔴)

| 字段 | 表 | 用途 |
|------|-----|------|
| `deleted` | file_entries | 删除文件恢复 |
| `created_at/modified_at/accessed_at/changed_at` | file_entries | MACB 时间线分析 |
| `hash_sha256` | file_entries | 证据完整性校验 |
| `artifact_type` | artifacts | 工件分类 |
| `attrs` (JSON) | artifacts | 工件详细数据 |
| `event_type` | timeline_events | 事件分类 |
| `ts` | timeline_events | 时间点分析 |

### 高价值字段 (🟢)

| 字段 | 表 | 用途 |
|------|-----|------|
| `path` | file_entries | 文件位置追踪 |
| `name` | file_entries | 文件识别 |
| `size` | file_entries | 异常检测 |
| `ext` | file_entries | 文件类型识别 |
| `source_path` | data_sources | 证据来源 |
| `kind` | data_sources | 证据类型 |
| `examiner` | cases | 责任追溯 |

---

## ⚠️ 当前问题与改进建议

### 问题 1: hash_sha256 字段未使用

**现状**: `file_entries.hash_sha256` 字段定义存在，但从未填充

**影响**: 
- 无法验证证据完整性
- 无法进行文件去重
- 无法生成 IOC

**建议**: 在文件导入时计算并存储 SHA-256

### 问题 2: 缺少分区信息持久化

**现状**: 分区信息存储在 `data_sources.partitions` JSON 中，未独立建表

**建议**: 创建 `partitions` 表存储分区详细信息

### 问题 3: 缺少用户活动追踪

**现状**: 无审计日志表

**建议**: 创建 `audit_log` 表记录用户操作

### 问题 4: timeline_events.case_id 未填充

**现状**: `case_id` 字段为空字符串

**影响**: 无法按案件过滤时间线

**建议**: 在插入时填充 case_id

---

## ✅ 优化建议

### 短期 (1 周)

1. **填充 hash_sha256**: 文件导入时计算哈希
2. **修复 timeline.case_id**: 插入时填充案件 ID
3. **添加复合索引**: 优化常用查询

### 中期 (1 个月)

4. **创建 partitions 表**: 独立存储分区信息
5. **创建 audit_log 表**: 用户操作追踪
6. **添加全文搜索索引**: FTS5 支持

### 长期 (3 个月)

7. **数据加密**: 敏感字段加密存储
8. **数据压缩**: JSON 字段压缩
9. **分表策略**: 大案件数据分表

---

## 📈 数据量预估

| 表 | 单案件预估记录数 | 存储空间 |
|----|-----------------|----------|
| cases | 1 | 1 KB |
| data_sources | 1-10 | 10 KB |
| file_entries | 10,000-1,000,000 | 100 MB-10 GB |
| artifacts | 100-10,000 | 10 MB |
| timeline_events | 10,000-1,000,000 | 100 MB-10 GB |
| jobs | 10-100 | 10 KB |
| reports | 1-10 | 10 KB |
| tags | 10-100 | 10 KB |

---

**分析人**: MiMo AI Assistant  
**分析版本**: v1.0
