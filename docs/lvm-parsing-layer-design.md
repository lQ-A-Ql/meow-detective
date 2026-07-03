# LVM 解析层设计文档

## 1. 问题陈述

### 1.1 当前状态

Forensics Workbench 当前对 Linux 磁盘镜像的取证能力存在一个关键的架构断层：**LVM（Logical Volume Manager）解析层完全缺失**。

多数生产 Linux 服务器部署（Ubuntu Server、RHEL/CentOS、Debian 等默认安装）都使用 LVM 管理根文件系统。当导入此类服务器的磁盘镜像（E01/RAW）时：

| 能读取 | 不能读取 |
|--------|---------|
| `/boot` 分区（通常在 LVM 之外，ext4/xfs） | `/` 根文件系统（在 LVM 内） |
| 非 LVM 数据分区 | `/home`、`/var`、`/etc` 等关键路径 |
| | 系统日志（systemd journal） |
| | Bash 历史、apt 日志、cron 任务 |
| | 用户数据 |

### 1.2 证据

`linux_e01_integration.rs` 测试中已明确记录此限制（L233-236）：

```rust
// The first detected candidate on this sample is the XFS /boot
// partition (grub/vmlinuz/initramfs), not the Linux root filesystem
// (which lives on the unsupported "Linux LVM" partition), so root-fs
// paths like /etc or /home are not expected here.
```

### 1.3 根因

分区表解析器虽然能识别 LVM 分区类型，但标记为 `Unsupported`：

- **MBR** (`evidence-core/src/volume/mbr.rs`): 类型 `0x8E` → `"Linux LVM"` → `MbrPartitionStatus::Unsupported`
- **GPT** (`evidence-core/src/volume/gpt.rs`): GUID `E6D6D379-F507-44C2-A23C-238F2A3DF928`（Linux LVM）未识别，归入 `Unknown`

## 2. LVM2 磁盘格式详解

### 2.1 整体布局

```
Offset 0:        空扇区 (512 bytes, 全零)
Offset 512:      PV Label 扇区
                   ├── Label Header (32 bytes)
                   └── PV Header (variable)
Offset 变量:       Metadata Area (环形缓冲区)
                   ├── MDA Header (512 bytes)
                   └── Raw Location → Metadata Text (ASCII)
剩余空间:           Data Area (实际逻辑卷数据)
```

### 2.2 PV Label 扇区（Sector 1, Offset 512）

#### Label Header（32 bytes）

| Offset | 大小 | 字段 | 描述 |
|--------|------|------|------|
| 0 | 8 | `signature` | `"LABELONE"` |
| 8 | 8 | `sector_number` | 此 label 所在扇区号（u64 LE） |
| 16 | 4 | `checksum` | 弱 CRC-32，覆盖 bytes 20 到扇区末尾 |
| 20 | 4 | `data_offset` | PV Header 在此扇区内的字节偏移（u32 LE） |
| 24 | 8 | `type_indicator` | `"LVM2 001"` |

**CRC 算法：** 多项式 `0xedb88320`，初始值 `0xf597a6cf`，无最终 XOR。与标准 CRC32/IEEE 不同。

#### PV Header（variable，位于 data_offset 处）

| Offset | 大小 | 字段 | 描述 |
|--------|------|------|------|
| 0 | 32 | `pv_uuid` | PV UUID，ASCII 字符串（例：`9LBcEB7PQTGIlLI0KxrtzrynjuSL983W`） |
| 32 | 8 | `pv_size` | PV 总大小，字节（u64 LE） |
| 40 | N×16 | `data_areas` | DataDescriptor 数组，以 16 字节全零终止 |
| ... | M×16 | `metadata_areas` | DataDescriptor 数组，以 16 字节全零终止 |

#### DataDescriptor（16 bytes）

| Offset | 大小 | 字段 |
|--------|------|------|
| 0 | 8 | `offset` — 从 PV 起始的字节偏移（u64 LE） |
| 8 | 8 | `size` — 区域大小，字节（u64 LE） |

### 2.3 Metadata Area Header（512 bytes）

位于 `metadata_descriptor.offset` 处。

| Offset | 大小 | 字段 | 描述 |
|--------|------|------|------|
| 0 | 4 | `checksum` | 弱 CRC-32，覆盖 bytes 4 到 header 末尾 |
| 4 | 16 | `signature` | `" LVM2 x[5A%r0N*>"`（注意前导空格） |
| 20 | 4 | `version` | 版本号，通常为 1（u32 LE） |
| 24 | 8 | `metadata_area_offset` | Metadata 环形缓冲区偏移（u64 LE） |
| 32 | 8 | `metadata_area_size` | 缓冲区大小（u64 LE） |
| 40 | 96 | `raw_location_descriptors` | 4 个 RawLocation，每个 24 bytes |
| 136 | 376 | 保留（零填充） |

#### RawLocation（24 bytes × 4）

| Offset | 大小 | 字段 | 描述 |
|--------|------|------|------|
| 0 | 8 | `offset` | 数据相对于 metadata_area_offset 的偏移（u64 LE） |
| 8 | 8 | `size` | 数据大小（u64 LE） |
| 16 | 4 | `checksum` | 数据块的 CRC-32（u32 LE） |
| 20 | 4 | `flags` | `0x00000001` = RAW_LOCN_IGNORED，应跳过 |

### 2.4 Metadata Text 格式（ASCII）

Metadata 文本位于 `mda_header.metadata_area_offset + raw_locn.offset`，是自定义 ASCII 配置格式。

#### 语法规则

- `#` 引入行注释
- `<key> = <value>` 赋值，值可以是整数、带引号字符串、或列表 `["...", "..."]`
- `name { ... }` 花括号块
- 空白（含换行）在 token 间忽略

#### 全局参数

```
contents = "Text Format Volume Group"
version = 1
description = "..."
creation_host = "hostname"
creation_time = <unix_timestamp>   # <ctime_comment>
```

#### 卷组块

```
<vg_name> {
    id = "VG-UUID..."
    seqno = <monotonic_sequence_number>
    status = ["READ", "WRITE", "RESIZEABLE"]
    extent_size = 8192              # PE size in sectors (512B)
    max_lv = 0
    max_pv = 0
    metadata_copies = 0

    physical_volumes {
        pv0 {
            id = "pv-uuid..."
            device = "/dev/sda1"    # device hint
            status = ["ALLOCATABLE"]
            dev_size = <sectors>
            pe_start = <sectors>    # offset to first PE
            pe_count = <number>
        }
        # pv1, pv2...
    }

    logical_volumes {
        <lv_name> {
            id = "lv-uuid..."
            status = ["READ", "WRITE", "VISIBLE"]
            segment_count = 1
            segment1 {
                start_extent = 0
                extent_count = 2560
                type = "striped"    # stripe_count=1 = linear
                stripe_count = 1
                stripes = ["pv0", 0]
            }
            # segment2...
        }
    }
}
```

#### Segment 类型

| 类型 | 支持优先级 | 描述 |
|------|:---:|------|
| `striped`（stripe_count=1） | Phase 1 ✅ | 线性映射——最常见 |
| `striped`（stripe_count>1） | Phase 2 | 条带化交错映射 |
| `linear` | Phase 1 ✅ | 同 striped 1 |
| `mirror` | Phase 3 | 镜像（RAID-1） |
| `raid0`, `raid0_meta` | Phase 3 | RAID-0 |
| `raid1`, `raid10` | Phase 3 | RAID-1/10 |
| `raid4`, `raid5*`, `raid6*` | Phase 3 | 带校验的 RAID |
| `thin`, `thin-pool` | Phase 3 | 精简置备 |
| `snapshot` | Phase 3 | 写时复制快照 |
| `cache`, `cache-pool` | Future | 缓存层 |

### 2.5 逻辑→物理地址映射

对于 linear 段（最常见配置）：

```
物理偏移 = PV数据区起始 + (stripe_start_extent + le_index) × extent_size × 512
```

其中：
- `stripe_start_extent` 来自 `stripes = ["pv0", start_extent]`
- `extent_size` 以 512 字节扇区为单位
- `le_index` 是段内的逻辑 extent 编号（0..extent_count-1）

## 3. Crate 设计：`crates/fs-lvm/`

### 3.1 模块结构

```
crates/fs-lvm/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 公共 API: probe_lvm(), LvmPool, open_pv()
│   ├── label.rs            # PV Label 解析
│   │   ├── parse_label_header()   # LABELONE 验证 + CRC
│   │   └── parse_pv_header()      # UUID, size, descriptors
│   ├── metadata.rs         # MDA 解析 + Metadata Text 解析
│   │   ├── parse_mda_header()     # MDA header + CRC
│   │   ├── parse_metadata_text()  # ASCII 文本 → 结构化数据
│   │   └── resolve_vg()           # 解析 VG/LV/Segment
│   ├── segment.rs          # 段映射引擎
│   │   ├── build_extent_map()     # LE → 物理扇区映射表
│   │   ├── map_linear()           # Linear segment 映射
│   │   └── map_striped()          # Striped segment 映射 (Phase 2)
│   ├── crc.rs              # LVM 专用弱 CRC-32
│   │   ├── lvm_crc32()            # 核心算法
│   │   └── verify_sector_crc()    # Label/MDA CRC 验证
│   ├── error.rs            # 错误类型
│   └── lv_reader.rs        # LV 读取器，实现 EvidenceReader
│       ├── LvReader              # 单 PV LV 读取器
│       ├── MultiPvLvReader       # 多 PV LV 读取器 (Phase 2)
│       └── impl Read + Seek + EvidenceReader
└── tests/
    ├── label_test.rs       # Label 格式解析测试
    ├── metadata_test.rs    # Metadata 文本解析测试
    ├── segment_test.rs     # 段映射测试
    ├── crc_test.rs         # CRC 算法测试
    └── integration_test.rs # 端到端测试（E01 → LVM → ext4/xfs）
```

### 3.2 核心数据结构

```rust
// ===== lib.rs =====

/// 在指定偏移处探测 LVM2 PV
/// 读取 sector 1，验证 "LABELONE" + "LVM2 001" magic
pub fn probe_lvm(
    reader: &mut (impl Read + Seek),
    offset: u64,
) -> Result<bool>;

/// 打开 LVM2 物理卷，返回卷组的所有逻辑卷
pub struct LvmPool {
    pub volume_group: VolumeGroup,
    logical_volumes: Vec<LvInfo>,
    pv_readers: Vec<Box<dyn Read + Seek>>,
}

pub struct LvInfo {
    pub name: String,        // e.g. "root", "home", "swap"
    pub uuid: String,
    pub size_bytes: u64,
}

impl LvmPool {
    pub fn open(
        readers: Vec<Box<dyn EvidenceReader>>,
        pv_offsets: Vec<u64>,
    ) -> Result<Self>;

    pub fn list_volumes(&self) -> &[LvInfo];

    /// 打开指定逻辑卷，返回虚拟块设备
    pub fn open_volume(&self, index: usize) -> Result<LvVolume>;
}

/// LV 虚拟块设备，实现 EvidenceReader 接口
pub struct LvVolume {
    inner: Box<dyn EvidenceReader>,
}
// ===== label.rs =====

pub struct LvmLabel {
    pub pv_uuid: String,             // 32-char ASCII
    pub pv_size: u64,                // bytes
    pub data_areas: Vec<DataRegion>,
    pub metadata_areas: Vec<DataRegion>,
}

pub struct DataRegion {
    pub offset: u64,   // absolute, from PV start
    pub size: u64,
}
// ===== metadata.rs =====

pub struct VolumeGroup {
    pub name: String,
    pub id: String,                  // VG UUID
    pub extent_size: u64,           // sectors (typically 512B each)
    pub seqno: u64,                 // monotonic, highest = authoritative
    pub physical_volumes: Vec<PvMeta>,
    pub logical_volumes: Vec<LvMeta>,
}

pub struct PvMeta {
    pub name: String,               // "pv0", "pv1"...
    pub uuid: String,
    pub device: Option<String>,     // from metadata hint
    pub pe_start: u64,              // sectors
    pub pe_count: u64,
}

pub struct LvMeta {
    pub name: String,
    pub uuid: String,
    pub segments: Vec<SegmentMeta>,
    pub size_bytes: u64,            // derived
}

pub struct SegmentMeta {
    pub start_extent: u64,
    pub extent_count: u64,
    pub seg_type: SegmentType,
    pub stripes: Vec<(String, u64)>, // (pv_name, start_extent)
}

pub enum SegmentType {
    Linear,
    Striped { stripe_count: u64 },
    Unsupported { type_name: String },
}
// ===== segment.rs =====

pub struct ExtentMapping {
    pub logical_offset: u64,    // offset within LV, in bytes
    pub physical_offset: u64,   // absolute offset on PV, in bytes
    pub length: u64,            // contiguous length in bytes
    pub pv_index: usize,        // which PV reader to use
}
```

### 3.3 CRC-32 算法实现

```rust
/// LVM2 专用 CRC-32
/// - 多项式: 0xEDB88320 (反射形式)
/// - 初始值: 0xF597A6CF (非标准)
/// - 无最终 XOR (与标准 CRC32/IEEE 的关键区别)
pub fn lvm_crc32(data: &[u8]) -> u32 {
    const POLY: u32 = 0xEDB8_8320;
    let mut crc: u32 = 0xF597_A6CF;

    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ POLY;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}
```

## 4. 集成方案

### 4.1 GPT 分区类型扩展（`evidence-core/src/volume/gpt.rs`）

```rust
// 新增 LVM GPT 分区类型 GUID
const LINUX_LVM: [u8; 16] = [
    0x79, 0xD3, 0xD6, 0xE6, 0x07, 0xF5, 0xC2, 0x44,
    0xA2, 0x3C, 0x23, 0x8F, 0x2A, 0x3D, 0xF9, 0x28,
];

pub enum GptPartitionType {
    // ... existing variants ...
    LinuxLvm,      // NEW
}

pub fn classify_partition_type(type_guid: &[u8; 16]) -> GptPartitionType {
    match *type_guid {
        // ... existing matches ...
        LINUX_LVM => GptPartitionType::LinuxLvm,
        _ => GptPartitionType::Unknown,
    }
}
```

### 4.2 数据源检测（`datasource_service.rs`）

```rust
pub enum ImageFilesystemKind {
    // ... existing ...
    LvmPool,    // NEW: LVM 卷组，需要进一步展开
}

fn read_boot_filesystem<R>(reader: &mut R, offset: u64) -> Result<Option<ImageFilesystemKind>>
where R: Read + Seek
{
    // ... existing superblock checks (NTFS, FAT, XFS, Ext4, Btrfs) ...

    // NEW: LVM check — after existing filesystem checks
    if fs_lvm::probe_lvm(reader, offset).unwrap_or(false) {
        return Ok(Some(ImageFilesystemKind::LvmPool));
    }

    Ok(None)
}

fn detect_image_filesystem<R>(reader: &mut R) -> Result<ImageFilesystemProbe>
where R: Read + Seek
{
    // ... existing MBR/GPT partition parsing ...

    // 对于 MBR type 0x8E 或 GPT LVM GUID:
    //   status 设为 Supported（因为可以展开为 LV）
    //   如果 read_boot_filesystem 检测到 LVM:
    //     调用 expand_lvm_pool() → 对每个 LV 检测文件系统
}

/// 展开 LVM Pool 为独立的文件系统候选
fn expand_lvm_pool(
    reader: &mut impl Read + Seek,
    partition_offset: u64,
    partition_index: Option<usize>,
    partition_name: Option<String>,
    candidates: &mut Vec<ImageFilesystemCandidate>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let pool = fs_lvm::LvmPool::open(
        vec![Box::new(reader)],
        vec![partition_offset],
    )?;

    for (i, lv_info) in pool.list_volumes().iter().enumerate() {
        let lv_vol = pool.open_volume(i)?;
        let lv_offset = 0; // LV starts at offset 0 on virtual device
        match read_boot_filesystem(&mut lv_vol, lv_offset)? {
            Some(fs_kind) => {
                candidates.push(ImageFilesystemCandidate {
                    partition_index,
                    partition_name: Some(format!("{}/{}", 
                        partition_name.as_deref().unwrap_or("LVM"), 
                        lv_info.name)),
                    kind: fs_kind,
                    offset: 0,
                    source: ImageFilesystemSource::LvmLogicalVolume,
                });
            }
            None => {
                warnings.push(format!(
                    "LV '{}' ({}): no recognized filesystem",
                    lv_info.name, lv_info.size_bytes
                ));
            }
        }
    }
    Ok(())
}
```

### 4.3 导入管线集成（`import_pipeline/partition.rs`）

```rust
// 在 discover_partitions() 中:
//  遍历所有分区记录
//  ├─ 如果 filesystem == Some(LvmPool):
//  │   ├─ fs_lvm::open_pv(image_reader, partition_offset)
//  │   ├─ 遍历每个 LV
//  │   ├─ 为每个含文件系统的 LV 创建 "虚拟分区"
//  │   └─ 替换原 LVM 分区为 N 个 LV 子分区
//  └─ 对最终分区列表执行文件系统枚举
```

## 5. 解析流程

```
输入: Box<dyn EvidenceReader>, PV起始偏移

1. label::parse_label(reader, offset)
   ├─ Seek(offset + 512), Read(512 bytes)
   ├─ magic[0..8] == b"LABELONE" ?
   ├─ type[24..32] == b"LVM2 001" ?
   ├─ crc32(bytes[20..512]) == stored_crc ?
   ├─ data_offset = u32_le(bytes[20..24])
   └─ 定位 PV Header: bytes[data_offset..]

2. label::parse_pv_header(bytes[data_offset..])
   ├─ pv_uuid: String::from_utf8(bytes[0..32])
   ├─ pv_size: u64_le(bytes[32..40])
   ├─ 遍历 data_areas: 读 DataDescriptor (16B) 直到 (0,0)
   └─ 遍历 metadata_areas: 读 DataDescriptor (16B) 直到 (0,0)

3. metadata::parse_metadata(reader, mda_region)
   ├─ Seek(mda_region.offset), Read(512)
   ├─ magic[4..20] == b" LVM2 x[5A%r0N*>" ?
   ├─ crc32(bytes[4..512]) == stored_crc ?
   ├─ mda_offset: u64_le(bytes[24..32])
   ├─ mda_size: u64_le(bytes[32..40])
   └─ 遍历 4 个 raw_location_descriptors (各 24B)
       ├─ 跳过 flags & RAW_LOCN_IGNORED
       ├─ Seek(mda_offset + raw_locn.offset)
       ├─ Read(raw_locn.size) → metadata_text
       └─ crc32(text) == raw_locn.checksum ?

4. metadata::parse_metadata_text(ascii_text)
   ├─ 验证 contents == "Text Format Volume Group"
   ├─ 验证 version == 1
   ├─ 解析 vg_name { ... } 块
   │   ├─ id, seqno, extent_size
   │   ├─ physical_volumes { pv0 { id, pe_start, pe_count } }
   │   └─ logical_volumes { <name> { segment1 { ... } } }
   └─ 返回 VolumeGroup

5. segment::build_extent_map(vg)
   ├─ 对每个 LV 的每个 segment:
   │   ├─ 构建 ExtentMapping 表
   │   └─ 计算 total_size
   └─ 返回 Vec<ExtentMapping>

6. LvVolume 构造
   ├─ 持有 PV reader + extent mapping 表
   ├─ read(offset, length) → 查找映射表 → 发起 PV reader.read()
   └─ 实现 Read + Seek + EvidenceReader
```

## 6. 错误处理策略

```rust
pub enum LvmError {
    /// Label 扇区 magic 不匹配
    NotLvm,
    /// Label CRC 校验失败
    LabelCrcMismatch { expected: u32, actual: u32 },
    /// MDA CRC 校验失败
    MdaCrcMismatch { expected: u32, actual: u32 },
    /// Metadata text CRC 校验失败
    MetadataCrcMismatch { index: usize, expected: u32, actual: u32 },
    /// Metadata text 解析失败
    MetadataParseError { line: usize, message: String },
    /// 不支持的 segment 类型
    UnsupportedSegment { lv_name: String, seg_type: String },
    /// 段映射中的 PV 未在 VG 中找到
    UnknownPhysicalVolume { name: String },
    /// I/O 错误
    Io(std::io::Error),
}
```

**容错策略：**
- 多副本 MDA：选取最高 `seqno` 的副本；如校验失败，fallback 到下一副本
- Metadata text 部分损坏：解析到损坏处为止，返回部分结果 + 警告
- 多个 raw_location_descriptors：按序尝试，首个验证通过即使用

## 7. 实现路线图

### Phase 1: 最小可行实现（核心场景覆盖）

| 模块 | 代码量 | 描述 |
|------|--------|------|
| `crc.rs` | ~50 行 | LVM CRC-32 实现 + 单元测试 |
| `error.rs` | ~60 行 | 错误类型定义 |
| `label.rs` | ~200 行 | PV Label + PV Header 解析 |
| `metadata.rs` | ~400 行 | MDA Header + Metadata Text 解析器 |
| `segment.rs` | ~300 行 | Linear segment 映射引擎 |
| `lv_reader.rs` | ~250 行 | LV 虚拟块设备 (Read+Seek+EvidenceReader) |
| `lib.rs` | ~100 行 | 公共 API + probe_lvm() + LvmPool |
| 单元测试 | ~500 行 | 格式解析 + CRC + 映射 + 集成测试 |
| GPT 集成 | ~30 行 | 添加 LVM GUID |
| datasource 集成 | ~150 行 | 检测 + 展开 |
| import 管线集成 | ~100 行 | LV 虚拟分区 |
| **合计** | **~2,140 行** | |

### Phase 2: 多 PV + Striped

| 模块 | 代码量 | 描述 |
|------|--------|------|
| 多 PV 支持 | ~200 行 | 跨 PV 的 metadata 发现 + LV 映射 |
| Striped segment | ~150 行 | stripe_count>1 的交错映射 |
| 测试 | ~200 行 | 多 PV + striped 场景测试 |

### Phase 3: RAID + Thin Pool

| 模块 | 代码量 | 描述 |
|------|--------|------|
| RAID (mirror/raid5/raid6) | ~400 行 | 冗余阵列映射 + 校验验证 |
| Thin Pool | ~300 行 | thin LV 的额外 metadata 层 |
| Snapshot | ~150 行 | 写时复制快照读取 |

## 8. 参考资料

| 资源 | 描述 |
|------|------|
| [libvslvm 文档](https://github.com/libyal/libvslvm/blob/main/documentation/Logical%20Volume%20Manager%20(LVM)%20format.asciidoc) | LVM 格式规范，所有字段偏移和算法描述 |
| [Kaitai Struct LVM2](https://formats.kaitai.io/lvm2/) | 声明式格式规范，可生成多语言解析器 |
| [forensicxlab/exhume_lvm](https://github.com/forensicxlab/exhume_lvm) | Rust 法证 LVM 解析器，使用 nom + serde |
| [lamlvm](https://crates.io/crates/lamlvm) | Rust no_std LVM reader，支持 linear LV |
| [util-linux libblkid](https://github.com/util-linux/util-linux/blob/master/libblkid/src/superblocks/lvm.c) | C 参考实现 |
| [SleuthKit Pool Layer](https://github.com/sleuthkit/sleuthkit/blob/develop/tsk/pool/pool_open.cpp) | TSK 的池抽象层设计 |
| [LVM2 源码](https://sourceware.org/git/?p=lvm2.git) | 官方 LVM 实现（`pvck`, `pv_header` 结构体） |

## 10. 精化模块规格（基于代码库研究）

### 10.1 EvidenceReader trait 接口确认

**文件:** `crates/evidence-core/src/reader/mod.rs`

```rust
pub trait EvidenceReader: Read + Seek + Send {
    fn info(&self) -> &ReaderInfo;
}
```

- **Layer 1 trait** — 提供原始字节读取，不涉及文件系统语义
- **超 trait**: `Read + Seek + Send`
- **唯一必需方法**: `fn info() -> &ReaderInfo`
- LvReader 属于 **Layer 1**（EvidenceReader），不是 Layer 2（FileSystemReader）
- LV 是虚拟块设备，传给 ext4/xfs/btrfs reader 时 offset=0

### 10.2 LvReader 构造器签名（对齐现有模式）

```rust
pub struct LvReader {
    device_reader: RefCell<Box<dyn EvidenceReader>>,
    info: ReaderInfo,
    extent_map: Vec<LvExtent>,      // LE → 物理偏移映射表
    current_pos: u64,
    total_size: u64,
}

pub struct LvExtent {
    pub logical_start: u64,   // 在 LV 内的字节偏移
    pub physical_offset: u64, // 在物理设备上的绝对字节偏移
    pub length: u64,          // 本段长度（字节）
}

impl LvReader {
    /// 打开逻辑卷。offset 为 LV 在虚拟设备中的偏移（通常为 0）。
    /// 与 Ext4Reader::open / XfsReader::open / BtrfsReader::open 签名完全一致。
    pub fn open(
        mut device_reader: Box<dyn EvidenceReader>,
        offset: u64,
    ) -> io::Result<Self>;
}

// trait 实现
impl Read for LvReader { /* 查 extent_map → seek → read */ }
impl Seek for LvReader { /* 仅更新 current_pos */ }
impl EvidenceReader for LvReader { /* 返回 &ReaderInfo */ }
```

### 10.3 LvmPool 公共 API

```rust
pub struct LvmPool {
    volume_group: VolumeGroup,
    pv_readers: Vec<RefCell<Box<dyn EvidenceReader>>>,
    logical_volumes: Vec<LvInfo>,
}

pub struct LvInfo {
    pub name: String,
    pub uuid: String,
    pub size_bytes: u64,
}

impl LvmPool {
    /// 扫描 PV 并返回发现的卷组池。
    /// readers: 每个 PV 一个 EvidenceReader
    /// offsets: 每个 PV 在 reader 中的起始偏移
    pub fn discover(
        readers: Vec<Box<dyn EvidenceReader>>,
        offsets: Vec<u64>,
    ) -> io::Result<Self>;

    /// 列出卷组中的所有逻辑卷
    pub fn list_volumes(&self) -> &[LvInfo];

    /// 打开指定逻辑卷，返回实现了 EvidenceReader 的虚拟块设备
    pub fn open_volume(&self, index: usize) -> io::Result<LvReader>;
}

/// 探测指定偏移是否为 LVM2 PV
pub fn probe_lvm(reader: &mut (impl Read + Seek), offset: u64) -> io::Result<bool>;
```

### 10.4 各模块精确函数签名

#### `label.rs`

```rust
pub struct LvmLabel {
    pub pv_uuid: String,               // 32 ASCII chars
    pub pv_size: u64,                  // bytes
    pub data_areas: Vec<DataRegion>,
    pub metadata_areas: Vec<DataRegion>,
}

pub struct DataRegion {
    pub offset: u64,                   // absolute, from PV start
    pub size: u64,
}

/// 读取 PV Label 扇区 (offset + 512)，验证 magic，返回 LvmLabel
pub fn parse_pv_label(
    reader: &mut (impl Read + Seek),
    pv_offset: u64,
) -> io::Result<LvmLabel>;
```

#### `crc.rs`

```rust
/// LVM2 专用弱 CRC-32
/// - 多项式: 0xEDB88320
/// - 初始值: 0xF597A6CF
/// - 无最终 XOR（与标准 CRC32/IEEE 的关键区别）
pub fn lvm_crc32(data: &[u8]) -> u32;

/// 验证 PV Label 扇区的 CRC（bytes 20..512）
pub fn verify_label_crc(sector: &[u8; 512]) -> bool;

/// 验证 MDA Header 的 CRC（bytes 4..512）
pub fn verify_mda_crc(header: &[u8; 512]) -> bool;
```

#### `metadata.rs`

```rust
pub struct VolumeGroup {
    pub name: String,
    pub id: String,                    // VG UUID
    pub extent_size: u64,              // sectors
    pub seqno: u64,                    // monotonic, higher = newer
    pub physical_volumes: Vec<PvMeta>,
    pub logical_volumes: Vec<LvMeta>,
}

pub struct PvMeta {
    pub name: String,
    pub uuid: String,
    pub pe_start: u64,                 // sectors
    pub pe_count: u64,
}

pub struct LvMeta {
    pub name: String,
    pub uuid: String,
    pub segments: Vec<SegmentMeta>,
    pub size_bytes: u64,               // derived
}

pub struct SegmentMeta {
    pub start_extent: u64,
    pub extent_count: u64,
    pub seg_type: SegmentType,
    pub stripes: Vec<(String, u64)>,   // (pv_name, start_extent)
}

pub enum SegmentType {
    Linear,
    Striped { stripe_count: u64 },
    Unsupported { type_name: String },
}

/// 从 metadata area 解析 VolumeGroup。
/// 多个 raw location descriptor 中选取最高 seqno 的副本。
pub fn parse_metadata(
    reader: &mut (impl Read + Seek),
    mda_region: &DataRegion,
) -> io::Result<VolumeGroup>;
```

#### `segment.rs`

```rust
/// 构建逻辑→物理映射表
pub fn build_extent_map(
    vg: &VolumeGroup,
    lv: &LvMeta,
    pv_regions: &[(String, u64)],     // (pv_name, data_area_start)
    extent_size_bytes: u64,           // extent_size * 512
) -> Vec<LvExtent>;
```

#### `lv_reader.rs`

```rust
impl LvReader {
    pub fn new(
        device_reader: Box<dyn EvidenceReader>,
        lv_name: String,
        lv_size: u64,
        extent_map: Vec<LvExtent>,
    ) -> Self;
}

impl Read for LvReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

impl Seek for LvReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
}

impl EvidenceReader for LvReader {
    fn info(&self) -> &ReaderInfo;
}
```

## 11. 精确集成规格（file:line 级别）

### 11.1 GPT 分区识别变更

**文件:** `crates/evidence-core/src/volume/gpt.rs`

**L26-32** — `GptPartitionType` 枚举，添加 `LinuxLvm`:

```rust
pub enum GptPartitionType {
    EfiSystem,
    MicrosoftReserved,
    MicrosoftBasicData,
    WindowsRecovery,
    LinuxLvm,      // NEW
    Unknown,
}
```

**L102-113** — 添加 LVM GUID 常量:

```rust
const LINUX_LVM: [u8; 16] = [
    0x79, 0xD3, 0xD6, 0xE6, 0x07, 0xF5, 0xC2, 0x44,
    0xA2, 0x3C, 0x23, 0x8F, 0x2A, 0x3D, 0xF9, 0x28,
];
```

**L115-123** — `classify_partition_type()` 匹配:

```rust
LINUX_LVM => GptPartitionType::LinuxLvm,
```

**L125-133** — `partition_type_name()` 添加:

```rust
GptPartitionType::LinuxLvm => "Linux LVM",
```

### 11.2 MBR 分区处理变更（状态提升）

**文件:** `crates/evidence-core/src/volume/mbr.rs`

**L76-79** — LVM 类型 `0x8E` 的 `status` 从 `Unsupported` 改为 `Supported`（因为现在有 LVM reader 可以处理）:

```rust
0x8E => MbrPartitionClass {
    name: "Linux LVM",
    status: MbrPartitionStatus::Supported,  // 原为 Unsupported
},
```

### 11.3 数据源检测变更

**文件:** `crates/app-services/src/datasource_service.rs`

**L33-41** — `ImageFilesystemKind` 枚举，添加 `LvmPool`:

```rust
pub enum ImageFilesystemKind {
    Ntfs,
    Fat,
    BitLocker,
    Ext4,
    Xfs,
    Btrfs,
    LvmPool,    // NEW
}
```

**L43-48** — `ImageFilesystemSource` 枚举，添加 `LvmLogicalVolume`:

```rust
pub enum ImageFilesystemSource {
    DirectVolume,
    MbrPartition,
    GptPartition,
    LvmLogicalVolume,   // NEW
}
```

**L695-727** — `read_boot_filesystem()` 函数末尾，在 `Ok(None)` 之前添加 LVM 检测:

```rust
// NEW: LVM2 PV label detection — check sector 1
reader.seek(SeekFrom::Start(offset + 512))?;
let mut label_sector = [0u8; 512];
if reader.read_exact(&mut label_sector).is_ok()
    && &label_sector[0..8] == b"LABELONE"
    && &label_sector[24..32] == b"LVM2 001"
{
    return Ok(Some(ImageFilesystemKind::LvmPool));
}
```

**L211-386** — `detect_image_filesystem()` 函数，MBR 分区遍历中 (L286-349):

在处理完每个 MBR 分区记录后，检测到 `LvmPool` 时调用展开逻辑:

```rust
// L288 — 在 for entry in &mbr_entries 循环内
// 在 push_candidate() 之后，PartitionRecord 构建之前

if let Some(ImageFilesystemKind::LvmPool) = fs_kind {
    // 展开 LVM Pool 为多个 LV 候选
    expand_lvm_pool(reader, offset, Some(entry.partition_number),
        Some(display_name.clone()), &mut candidates, &mut warnings)?;
    // 跳过为一个 LVM 物理分区创建 PartitionRecord
    //（因为每个 LV 会被创建为独立的候选/分区）
    continue;
}
```

**L616-619** — `kind_label()` 函数，添加 LVM:

```rust
ImageFilesystemKind::LvmPool => "LVM".to_string(),
```

### 11.4 `expand_lvm_pool()` 函数

**文件:** `crates/app-services/src/datasource_service.rs` — 新增函数（放置在 `detect_image_filesystem` 之后）:

```rust
fn expand_lvm_pool<R: Read + Seek>(
    reader: &mut R,
    partition_offset: u64,
    partition_index: Option<usize>,
    partition_name: Option<String>,
    candidates: &mut Vec<ImageFilesystemCandidate>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let boxed: Box<dyn EvidenceReader> = /* wrap reader for LvmPool */;
    let pool = fs_lvm::LvmPool::discover(vec![boxed], vec![partition_offset])?;

    for (i, lv) in pool.list_volumes().iter().enumerate() {
        let lv_reader = pool.open_volume(i)?;
        // lv_reader 已经实现 EvidenceReader, offset 始终为 0
        match read_boot_filesystem(&mut lv_reader, 0)? {
            Some(fs_kind) => {
                let name = format!("{}/{}",
                    partition_name.as_deref().unwrap_or("LVM"),
                    &lv.name);
                candidates.push(ImageFilesystemCandidate {
                    partition_index,
                    partition_name: Some(name),
                    kind: fs_kind,
                    offset: 0,  // LV 虚拟设备从 offset 0 开始
                    source: ImageFilesystemSource::LvmLogicalVolume,
                });
            }
            None => {
                warnings.push(format!(
                    "LVM logical volume '{}' ({:.1} MB): no recognized filesystem",
                    lv.name,
                    lv.size_bytes as f64 / 1_048_576.0
                ));
            }
        }
    }
    Ok(())
}
```

### 11.5 导入管线枚举 — `ImageFilesystemKind→Reader` 映射

需要更新 **三个位置**，它们都执行 `match candidate.kind { Ntfs => ..., Fat => ..., Ext4 => ..., Xfs => ..., Btrfs => ... }`:

| 位置 | 文件:行 |
|------|---------|
| Legacy path | `crates/app-services/src/import_pipeline/partition.rs:281-370` |
| Modern path | `crates/app-services/src/import_pipeline/partition.rs:440-483` |
| RAW file preview | `crates/app-services/src/file_service/viewer/image_open.rs:221-278` |

每处添加:

```rust
ImageFilesystemKind::LvmPool => {
    // LVM pool 已在 probe 阶段被 expand_lvm_pool() 展开为
    // 带具体 FS kind 的 LvmLogicalVolume 候选。
    // 不应该有 LvmPool 候选到达此处。
    return None; // or continue;
}
```

### 11.6 导入管线 — 分区展开

**文件:** `crates/app-services/src/import_pipeline/partition.rs`

- `build_partition_work()` (L422) — 添加对 `LvmPool` 的映射（实际上 `LvmPool` 候选应在 probe 阶段已被展开）
- `enumerate_image_data_source()` (L105) — 同上的遗留路径

## 12. 测试计划

### 12.1 测试分层

遵循 `fs-ext4`/`fs-xfs`/`fs-btrfs` 已建立的模式，采用三层测试：

**Layer 1: 单元测试** (in `src/*.rs`, `#[cfg(test)] mod tests`)

使用 `FakeReader`（`Vec<u8>` 包装为 `Read + Seek + EvidenceReader`）:
- CRC-32 算法正确性（已知向量验证）
- PV Label 解析：验证 magic、CRC、UUID 提取
- PV Header 解析：验证 data_descriptors 和 metadata_descriptors 数组
- MDA Header 解析：验证 magic、raw_location_descriptors
- Metadata Text 解析：单 PV、单 VG、单 linear LV
- Segment 映射：linear segment → 正确物理偏移
- LvReader read/seek: 跨 extent 读取、边界错误
- 错误路径：非 LVM magic、CRC 失败、不支持的 segment 类型

**Layer 2: 集成测试** (in `crates/fs-lvm/tests/`)

使用合成的多 PV 或复杂 VG 夹具:
- 多 LV 卷组（root + swap + home）
- 每个 LV 可读验证（文件系统无关，直接读字节验证）
- 多 metadata 副本选取（最高 seqno）
- 损坏 metadata 容错（部分 descriptor 损坏时选取有效副本）
- 嵌入 ext4 文件系统的 LV：通过 `Ext4Reader::open(lv_reader, 0)` 验证透明读取

**Layer 3: 管线集成测试** (在 `crates/app-services/tests/linux_e01_integration.rs` 中扩展，或新建)

- 扩展 `ImageFilesystemKind` 枚举包含 `LvmPool`
- 在 `detect_image_filesystem()` 中添加 LVM 检测
- 使用合成 LVM + ext4 E01 镜像验证端到端流程
- `#[ignore]` 保护的实机 E01 LVM 测试

### 12.2 FakeReader 模板（与 ext4/xfs/btrfs 一致）

```rust
#[cfg(test)]
struct FakeReader {
    data: Vec<u8>,
    pos: u64,
    info: evidence_core::ReaderInfo,
}

impl Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = self.pos as usize;
        let end = (start + buf.len()).min(self.data.len());
        let len = end.saturating_sub(start);
        buf[..len].copy_from_slice(&self.data[start..end]);
        self.pos += len as u64;
        Ok(len)
    }
}

impl Seek for FakeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(o) => o,
            SeekFrom::End(o) => (self.data.len() as i64 + o) as u64,
            SeekFrom::Current(o) => (self.pos as i64 + o) as u64,
        };
        Ok(self.pos)
    }
}

impl EvidenceReader for FakeReader {
    fn info(&self) -> &ReaderInfo { &self.info }
}
```

### 12.3 合成 LVM 夹具构建器

```rust
/// 构建一个最小 LVM2 PV 夹具 (~1 MB)
/// 结构:
///   sector 0:   empty (zero-filled)
///   sector 1:   PV label header + PV header
///   sector 2-N: metadata area (circular buffer, ASCII text)
///   sector N+1+: data area (LV 数据，可嵌入 ext4 文件系统镜像)
fn build_lvm_fixture() -> Vec<u8>;
```

## 13. 与原设计文档的差异修正

基于代码库研究，原设计文档有以下几点需要修正：

| 原设计 | 修正 | 原因 |
|--------|------|------|
| `LvmPool::open()` 接受 `Vec<Box<dyn EvidenceReader>>` | 改为 `LvmPool::discover()` | 更准确地描述"扫描 PV + 发现 VG + 解析 LV"语义 |
| `LvVolume` 类型 | 改为 `LvReader` | 与 `Ext4Reader`/`XfsReader` 命名一致 |
| `LvReader` 构造器签名 `open(mut reader, offset)` | 保留此签名 | 与所有 FS reader 完全一致 |
| `ImageFilesystemKind::LvmPool` 需要新 reader 映射 | LV 展开在 probe 阶段完成，管线不需要额外映射 | LVM 候选在 `detect_image_filesystem()` 中已展开为具体 FS |
| MBR `0x8E` 保持 `Unsupported` | 改为 `Supported` | 有 LVM reader 后可以处理 |
| 需要 `ImageFilesystemKind` 新变体 | 需要 `LvmPool` 变体 | probe 阶段标记，供 expand 逻辑触发 |
| 需要 `ImageFilesystemSource` 新变体 | 需要 `LvmLogicalVolume` 变体 | 区分 LVM 内部卷与直接分区表条目 |

## 9. 设计决策记录

| # | 决策 | 理由 |
|---|------|------|
| 1 | 自己实现 vs 复用 lamlvm | lamlvm 仅支持单 PV linear LV，且依赖 `embedded-io`（no_std 生态），与本项目 `EvidenceReader` trait 不兼容 |
| 2 | 纯 Rust vs 绑定 C (libvslvm) | 项目现有文件系统解析器全部纯 Rust；无外部 C 依赖保证跨平台编译简单 |
| 3 | `fs-lvm` 命名 | 遵循项目 `fs-{filesystem}` 命名约定，尽管 LVM 严格说是卷管理器而非文件系统 |
| 4 | Metadata 文本解析：手写 vs 解析框架 | LVM 文本格式简单（嵌套键值+花括号），不需要引入 nom/pest；手写递归下降足够 |
| 5 | LV 暴露为 EvidenceReader | 已有 ext4/xfs/btrfs reader 无需修改，直接消费虚拟块设备 |
| 6 | 按各独立 PV reader 传入 | 多 PV 场景下每个 PV 可能需要独立的 reader（不同偏移/不同镜像），用 vec 传入最灵活 |
| 7 | `discover()` 而非 `open()` | 语义更准确：扫描 PV + 验证 UUID + 解析 VG/LV metadata + 构建 extent map |
| 8 | Probe 阶段展开 LVM vs 管线阶段 | 在 `detect_image_filesystem()` 的 probe 阶段展开 LV 为具体 FS 候选，避免修改下游管线 |
| 9 | LvReader 构造器与 FS reader 一致 | `fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self>` 完全对齐 ext4/xfs/btrfs |
| 10 | MBR 0x8E 状态提升 | 从 `Unsupported` 改为 `Supported`，表明 LVM reader 可处理 |
| 11 | FakeReader 测试模式 | 与 ext4/xfs/btrfs 完全一致的 `Vec<u8>` + `Read + Seek + EvidenceReader` 合成夹具模式 |
