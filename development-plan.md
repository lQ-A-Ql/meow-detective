# 开发与优化方案

> 基于 Forensics Workbench 当前状态 (27 commits, 61 tests, 19 crates)
> 参照 TSK `ntfs_dent.cpp` 已知模式，对标 Autopsy 能力闭环

---

## 评分机制

每个 Phase 完成后按 4 个维度打分 (0–4)，总分 ≤16：

| 维度 | 0 | 1 | 2 | 3 | 4 |
|------|---|---|---|---|---|
| **Correctness** 正确性 | 编译失败 | 有 panic | 边界错误 | 主力路径正确 | 全路径 + 边界通过 |
| **Completeness** 完整性 | 未实现 | 骨架 | 存明显缺口 | 主力场景覆盖 | 所有规范路径覆盖 |
| **Test Coverage** 测试 | 0 tests | 1 test | happy-path only | 边界 + 错误 | 边界 + 错误 + 回归 |
| **Code Quality** 质量 | 编译不过 | clippy 报错 | 无文档 | 有文档 | 文档 + 无警告 + 可读性好 |

**通过线**: Phase 必须 ≥ 10 分方可通过评审。P0 Phase 优先修复，P1 增量增强，P2 健壮性加固。

---

## Phase 16: NTFS `$INDEX_ALLOCATION` B-Tree 遍历

**优先级**: P0 — 修复大目录枚举不完整缺陷

### 背景

当前 `parse_index_root` 只读 `$INDEX_ROOT` (0x90) 属性的内嵌 INDX 条目。条目数超过 `$INDEX_ROOT` 容量时（通常 ~20-30 个文件），剩余条目存于 `$INDEX_ALLOCATION` (0xA0) 属性的 data run 中，以 B-Tree INDX record 形式组织。

TSK 参考实现: `ntfs_dir_open_meta` 在 `ntfs_dent.cpp:540-890`

### Task 16.1: 解析 `$INDEX_ALLOCATION` 属性的 data runs

**文件**: `crates/fs-ntfs/src/lib.rs`

**实现**:
- 查找 MFT 记录中 `$INDEX_ALLOCATION` (0xA0) 属性
- 判断该属性是否为 non-resident (`flags & TSK_FS_ATTR_NONRES`)
- 解析 data run list 获取 cluster 链表
- 将所有 cluster 数据读入连续 buffer

**关键结构** (参照 NTFS 规范):
```
$INDEX_ALLOCATION attribute:
  non-resident header:
    +0x00: type (0xA0)
    +0x04: length
    +0x08: non-resident flag (bit 0 = 1)
    +0x10: start VCN
    +0x18: last VCN
    +0x20: data run offset
    +0x28: allocated size
    +0x30: real size
  data runs follow at offset specified
```

**测试**: `test_index_alloc_reads_data_runs`
- 构造含 `$INDEX_ALLOCATION` 的合成 fixture
- 验证能读取到 INDX buffer

### Task 16.2: INDX Record 遍历与 fixup

**文件**: `crates/fs-ntfs/src/lib.rs`

**实现**:
- 在 INDX buffer 中按 cluster 边界扫描 `INDX` magic (0x58444E49)
- 对每个 INDX record 应用 update sequence fixup
- 解析 INDX record 内的 index entry list

**INDX Record 结构**:
```
+0x00: magic "INDX" (4 bytes)
+0x04: update sequence offset (2 bytes)
+0x06: update sequence count (2 bytes)
+0x08: LSN (8 bytes)
+0x10: VCN of this record (8 bytes)
+0x18: index entry list header
  +0x00: entries offset (4 bytes)
  +0x04: total size of entries (4 bytes)
  +0x08: allocated size of entries (4 bytes)
  +0x0C: flags (4 bytes)
  entries start at list + entries_offset
```

**测试**: `test_indx_record_fixup`
- 构造含 update sequence array 的 INDX record
- 验证 fixup 后数据正确

### Task 16.3: 合并 $INDEX_ROOT 和 $INDEX_ALLOCATION 条目

**文件**: `crates/fs-ntfs/src/lib.rs`

**实现**:
- 修改 `list_dir_by_inode` → 先读 `$INDEX_ROOT` 条目，再读 `$INDEX_ALLOCATION` 条目
- 去重：以 `mft_ref` 为键，$INDEX_ALLOCATION 条目优先（更新）
- 返回合并后的 `Vec<DirEntry>`

**测试**: `test_large_dir_enumeration`
- 构造 >30 个条目的合成 fixture（$INDEX_ROOT 溢出）
- 验证返回全部条目

**测试**: `test_merged_entries_no_duplicates`
- 同一文件同时出现在 $INDEX_ROOT 和 $INDEX_ALLOCATION 中
- 验证去重后只出现一次

### Phase 16 预期

| 维度 | 预期分 | 说明 |
|------|--------|------|
| Correctness | 3 | 主力路径正确；TRUNCATED INDX record 容错待后续 |
| Completeness | 3 | 覆盖 $INDEX_ROOT + $INDEX_ALLOCATION；deleted entries 待后续 |
| Test Coverage | 3 | 3 tests: data run 读取 + fixup + 大目录合并 |
| Code Quality | 3 | 文档 + clippy clean |
| **合计** | **12/16** | ✅ 通过 |

---

## Phase 17: NTFS `$DATA` 属性 + 文件读取

**优先级**: P0 — 解除 `open_file` 的 Unsupported 状态

### 背景

当前 `open_file` 返回 `Unsupported`。需要解析 `$DATA` (0x80) 属性的 data runs 以读取文件内容。文件完整路径由 Phase 16 的路径解析获得。

### Task 17.1: 解析 `$DATA` 属性 data runs

**文件**: `crates/fs-ntfs/src/lib.rs`

**实现**:
- 新增 `read_file_data(&self, inode: u64) -> io::Result<Vec<u8>>`
- 查找 `$DATA` (0x80) 属性
- 如果是 resident → 直接返回属性体内的数据
- 如果是 non-resident → 解析 data run list → 按 cluster 链读取

**Data Run 编码**:
```
首字节 = size_count (低 4 位) | offset_count (高 4 位)
接下来的 size_count 字节 = run 长度 (cluster 数)
接下来的 offset_count 字节 = run 起始偏移 (有符号，相对 VCN)
```

**测试**: `test_read_resident_data`
- 合成 $DATA resident 属性
- 验证读取正确内容

**测试**: `test_read_nonresident_data`
- 合成 $DATA non-resident 属性（至少 2 个 data runs）
- 验证读取跨 run 内容正确

### Task 17.2: 接入 `FileSystemReader::open_file`

**文件**: `crates/fs-ntfs/src/lib.rs`

**实现**:
- `open_file(path)` → 调用 `resolve_path` 获取 inode → `read_file_data` → 返回 `Box<dyn Read>`
- 使用 `std::io::Cursor` 包装读取结果

**测试**: `test_open_file_reads_content`
- 构建嵌套 fixture → 放一个含 $DATA 的文件
- `open_file("\\Windows\\System32\\ntdll.dll")` → 读取验证

### Task 17.3: 大文件延迟读取

**文件**: `crates/fs-ntfs/src/lib.rs`

**实现**:
- 对大文件 (>1MB) 使用 `lazy_reader`：只记录 data run offsets，按需 seek + read
- 不再一次性把整个文件读入内存

**测试**: `test_large_file_lazy_read`
- 合成 5MB 文件 → 验证 seek-to-middle + read 部分正确

### Phase 17 预期

| 维度 | 预期分 | 说明 |
|------|--------|------|
| Correctness | 4 | resident + non-resident + 大文件 |
| Completeness | 4 | 覆盖 $DATA 所有形态 |
| Test Coverage | 4 | 4 tests: resident + non-resident + open_file + lazy read |
| Code Quality | 3 | 文档 + clippy clean |
| **合计** | **15/16** | ✅ 通过 |

---

## Phase 18: 多 Segment E01 支持

**优先级**: P1 — 扩展镜像格式支持

### 背景

大型 E01 镜像通常分割为 `.E01`, `.E02`, `.E03` 等多个 segment 文件。当前仅支持单 segment。

### Task 18.1: 多 segment 探测

**文件**: `crates/image-e01/src/lib.rs`

**实现**:
- 修改 `E01Reader::open` → 接受文件路径而非 boxed reader
- 根据 `.E01` 扩展名推导 `.E02`, `.E03` 等
- 打开所有 segment 文件

**测试**: `test_probe_multi_segment`
- 构造 2 个 segment 合成 fixture
- 验证探测到全部 segment

### Task 18.2: 跨 segment chunk 读取

**文件**: `crates/image-e01/src/lib.rs`

**实现**:
- 修改 `read_chunk` → 根据 chunk index 定位到对应 segment
- 维护 segment 内 chunk 偏移表
- 跨 segment 边界读取无缝衔接

**测试**: `test_cross_segment_read`
- 合成 2-segment E01 → 读取跨边界的 4K 数据
- 验证数据连续正确

### Task 18.3: 回归测试更新

**文件**: `crates/image-e01/tests/e01_regression_test.rs`

**实现**:
- 在现有的 31GB 单文件回归测试基础上，添加多 segment 路径
- 使用 `cfg(feature = "real-fixture")` 控制真实文件测试

### Phase 18 预期

| 维度 | 预期分 | 说明 |
|------|--------|------|
| Correctness | 3 | 2-segment 正确；boundary case 待后续 |
| Completeness | 3 | 覆盖主流 segment 数量 |
| Test Coverage | 3 | 3 tests: probe + cross-segment + regression |
| Code Quality | 3 | 文档 + clippy clean |
| **合计** | **12/16** | ✅ 通过 |

---

## Phase 19: 健壮性加固

**优先级**: P2 — 提升生产可靠性

### Task 19.1: `$FILE_NAME` par_ref 链一致性校验

**文件**: `crates/fs-ntfs/src/lib.rs`

**背景**: TSK 的 `ntfs_find_file` 在自底向上遍历时会验证 `par_ref` 和 `par_seq` 是否匹配父目录的 MFT 序列号。当前 `resolve_path` 仅自顶向下查找名字，不做一致性校验。

**实现**:
- 在 `resolve_path` 每个步骤中，打开子目录 MFT 记录，读取其 `$FILE_NAME` 属性
- 验证 `par_ref` 字段指向当前父目录 inode
- 验证 `par_seq` 与父目录 MFT 记录序列号一致
- 不一致时返回 `None`（路径断裂）

**测试**: `test_par_ref_consistency_pass`
- 构建正确 fixture → 验证路径解析成功

**测试**: `test_par_ref_consistency_fail`
- 构建 par_ref 不匹配的 fixture → 验证路径解析失败

### Task 19.2: 错误恢复与 panic-free 保障

**文件**: 全局 `crates/fs-ntfs/src/lib.rs`, `crates/image-e01/src/lib.rs`

**实现**:
- 对所有 `unwrap()` 调用添加错误处理
- 对 `try_into().unwrap()` 替换为 `try_into().map_err()`
- 添加整体 fuzz 入口（接受任意字节流不 panic）

**测试**: `test_malformed_record_no_panic`
- 输入随机字节到 `parse_index_root` / `parse_indx_entries`
- 验证不 panic

**测试**: `test_e01_malformed_header_no_panic`
- 输入随机字节到 E01 header 解析
- 验证不 panic

### Task 19.3: 性能基准测试

**文件**: `crates/fs-ntfs/benches/`, `crates/image-e01/benches/`

**实现**:
- `bench_list_root_children` — 目录枚举基准
- `bench_resolve_deep_path` — 深层路径解析基准
- `bench_read_chunk` — E01 chunk 读取基准

使用 `criterion` crate。

### Task 19.4: 真实镜像端到端测试

**文件**: `crates/app-services/tests/e2e_import_test.rs`

**实现**:
- 从真实 `.E01` 文件导入 → 枚举根目录 → 进入子目录 → 读取文件
- 全链路验证：`E01 → MBR → NTFS → list_children → resolve_path → open_file`
- 使用 `#[ignore]` + `cfg(feature = "real-fixture")`

### Phase 19 预期

| 维度 | 预期分 | 说明 |
|------|--------|------|
| Correctness | 4 | par_ref 校验 + panic-free |
| Completeness | 3 | 覆盖主力错误路径 |
| Test Coverage | 4 | 6+ tests: 一致性 x2 + malformed x2 + benchmarks + e2e |
| Code Quality | 4 | 无 unwrap + 文档 + clippy clean |
| **合计** | **15/16** | ✅ 通过 |

---

## Phase 20: FAT / exFAT 文件系统支持

**优先级**: P2 — 扩展文件系统覆盖

### Task 20.1: FAT boot sector 解析

**文件**: `crates/fs-fat/src/lib.rs`

**实现**:
- 解析 FAT12/16/32 BPB (BIOS Parameter Block)
- 计算 FAT 表偏移、根目录偏移、数据区偏移

**测试**: `test_fat32_boot_parse`

### Task 20.2: FAT 目录枚举

**文件**: `crates/fs-fat/src/lib.rs`

**实现**:
- 读取根目录 cluster 链
- 解析 32-byte 目录项（LFN 长文件名 + 短文件名）
- 实现 `FileSystemReader` trait

**测试**: `test_fat_list_root`

### Phase 20 预期

| 维度 | 预期分 | 说明 |
|------|--------|------|
| Correctness | 3 | FAT32 主力路径 |
| Completeness | 2 | FAT32 only; FAT12/16 + exFAT 后续 |
| Test Coverage | 2 | 2 tests: boot + list |
| Code Quality | 3 | 文档 + clippy clean |
| **合计** | **10/16** | ✅ 通过 (底线) |

---

## 阶段规划总览

```
Phase 16: NTFS $INDEX_ALLOCATION     [P0][─── 3 tasks, 3 tests, 12/16]
Phase 17: NTFS $DATA + file read     [P0][─── 3 tasks, 4 tests, 15/16]
Phase 18: Multi-segment E01          [P1][─── 3 tasks, 3 tests, 12/16]
Phase 19: Robustness hardening       [P2][─── 4 tasks, 6 tests, 15/16]
Phase 20: FAT/exFAT support          [P2][─── 2 tasks, 2 tests, 10/16]
```

| Phase | 任务 | 测试增量 | 时间估计 | 依赖 |
|-------|------|----------|----------|------|
| 16 | 3 | +3 | 3h | 当前 |
| 17 | 3 | +4 | 3h | 16 |
| 18 | 3 | +3 | 2h | 当前 |
| 19 | 4 | +6 | 3h | 16+17 |
| 20 | 2 | +2 | 2h | 当前 |

**总计**: 15 tasks, 18 tests, ~13h

---

## 里程碑

| 里程碑 | 完成 Phase | 状态特征 |
|--------|-----------|----------|
| M1: 目录枚举完整 | 16 | $INDEX_ALLOCATION → 大目录不再丢条目 |
| M2: 文件内容可读 | 17 | open_file 可用 → 搜索可索引文件内容 |
| M3: 格式覆盖 | 18+20 | E01 multi-segment + FAT → 镜像覆盖面扩展 |
| M4: 生产就绪 | 19 | par_ref 校验 + panic-free + benchmark + e2e |

---

## 评审流程

每个 Phase 完成后：
1. `cargo test -p <crate>` 全部通过
2. `cargo clippy -p <crate> --all-targets` 无新增警告
3. 4 维度评分，合计 ≥ 10 方可通过
4. 未达标 → 列出缺口 → 修复 → 重新评分
5. 通过 → commit → 更新本文档 Phase 状态
