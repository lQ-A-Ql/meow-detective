# Sleuth Kit 技术分析报告（Remote-only GitHub Audit）

## Executive Summary

The Sleuth Kit（TSK）是面向磁盘镜像、卷系统和文件系统取证分析的 C/C++ 库与命令行工具集合。仓库 `sleuthkit/sleuthkit` 默认分支为 `develop-4.1x`，本次审计固定的分支 HEAD 为 `ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e`。本报告基于 `gh` 远程查询完成，未 clone、pull、fetch 或下载源码归档。

TSK 的架构重点是：`tsk/` 下的低层取证库；`tools/` 下的 CLI 工具；`bindings/java/` 下的 Java datamodel 与 JNI 桥接；`case-uco/` 下的 CASE/UCO 导出；`tests/` 与 `unit_tests/` 下的测试；`win32/`、Autotools 与 GitHub Actions/AppVeyor 共同支撑跨平台构建。

关键证据：

- 仓库：<https://github.com/sleuthkit/sleuthkit>
- 审计分支：`develop-4.1x`
- 审计 SHA：`ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e`
- `tsk/`：<https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk>
- `tools/`：<https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools>
- `bindings/java/`：<https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java>
- `README.md`：<https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/README.md>
- `INSTALL.txt`：<https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/INSTALL.txt>

## Repository Snapshot

`gh repo view sleuthkit/sleuthkit` 返回的主要信息：

| Item | Value |
|---|---|
| Repository | `sleuthkit/sleuthkit` |
| Description | Library and command-line digital forensics tools for volume and file system data |
| Default branch | `develop-4.1x` |
| Audited HEAD | `ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e` |
| Primary language | C |
| Stars observed | 3078 |
| Recent update observed | 2026-06-04 |
| Recent releases observed | `sleuthkit-4.15.0`, `sleuthkit-4.14.0`, `sleuthkit-4.13.0`, `sleuthkit-4.12.1`, `sleuthkit-4.12.0` |

Top-level tree summary from `gh api repos/sleuthkit/sleuthkit/git/trees/develop-4.1x?recursive=1` shows: `tsk` (~269 paths), `tools` (~164), `bindings` (~233), `case-uco` (~91), `win32` (~118), `rejistry++` (~68), `man` (~31), `tests`, `unit_tests`, build metadata, and CI files.

## Audit Methodology

The audit used only remote GitHub CLI/API inspection:

```powershell
gh repo view sleuthkit/sleuthkit --json name,description,defaultBranchRef,primaryLanguage,stargazerCount,updatedAt,licenseInfo
gh api repos/sleuthkit/sleuthkit/commits/develop-4.1x --jq .sha
gh api "repos/sleuthkit/sleuthkit/git/trees/develop-4.1x?recursive=1"
gh api repos/sleuthkit/sleuthkit/releases --jq '.[0:5] | map({tag_name,name,published_at})'
gh api repos/sleuthkit/sleuthkit/contents/README.md?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e --jq .content
gh api repos/sleuthkit/sleuthkit/contents/INSTALL.txt?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e --jq .content
```

No local source checkout was created. Some `gh search` calls failed due query/network/API behavior, so issue/PR analysis is intentionally conservative.

## Architecture Map

TSK is structured as a low-level library plus tools and bindings:

| Area | Technical Role | Evidence |
|---|---|---|
| `tsk/base/` | Base types, errors, common library support | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/base> |
| `tsk/img/` | Disk image abstraction and image-format readers | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/img> |
| `tsk/vs/` | Volume-system / partition layer | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/vs> |
| `tsk/fs/` | Filesystem parsers and metadata/content abstractions | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/fs> |
| `tsk/hashdb/` | Hash database support | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/hashdb> |
| `tsk/pool/` | Pool/storage abstraction | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/pool> |
| `tsk/auto/` | Higher-level automation over image/volume/filesystem traversal | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/auto> |
| `tsk/util/` | Utility support | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/util> |
| `tools/` | CLI wrappers and standalone forensic utilities | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools> |
| `bindings/java/` | Java datamodel and JNI bridge used by Autopsy | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java> |
| `case-uco/` | CASE/UCO export model | <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/case-uco> |

The README describes TSK as tools for analyzing Microsoft and UNIX filesystems and disks, including evidence identification/recovery from images. It also emphasizes low-level command-line tools that each perform a single task and can be combined for full analysis.

## Major Modules

### Native Core Library (`tsk/`)

The native code is organized by forensic abstraction layer:

- `tsk/img/`: image acquisition/reader layer.
- `tsk/vs/`: volume system layer.
- `tsk/fs/`: filesystem layer.
- `tsk/hashdb/`: known-file hash lookup layer.
- `tsk/auto/`: automation/traversal layer.

This structure maps well to classic forensic analysis flow: image → volume system → filesystem → metadata/content → higher-level automation.

Evidence:

- <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/img>
- <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/vs>
- <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/fs>
- <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/hashdb>
- <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/auto>

### CLI Tools (`tools/`)

`tools/` provides operational interfaces around the library. The README describes filesystem-layer commands such as `fsstat`, content-layer commands such as `blkcat`, metadata-layer commands such as `istat`, file-layer commands such as `fls`, timeline generation through `mactime`, and hash-database utilities.

Key CLI areas:

- `tools/fstools/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools/fstools>
- `tools/imgtools/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools/imgtools>
- `tools/vstools/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools/vstools>
- `tools/hashtools/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools/hashtools>
- `tools/timeline/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools/timeline>
- `tools/autotools/tsk_loaddb.cpp`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools/autotools/tsk_loaddb.cpp>
- `tools/autotools/tsk_recover.cpp`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tools/autotools/tsk_recover.cpp>

### Java Datamodel and JNI

`bindings/java/` is the most important integration point for Autopsy. It contains Java source, Ivy/build files, Doxygen docs, and JNI bridge code.

Important files:

- `bindings/java/src/org/sleuthkit/datamodel/SleuthkitCase.java`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/src/org/sleuthkit/datamodel/SleuthkitCase.java>
- `bindings/java/src/org/sleuthkit/datamodel/SleuthkitJNI.java`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/src/org/sleuthkit/datamodel/SleuthkitJNI.java>
- `bindings/java/jni/dataModel_SleuthkitJNI.cpp`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/jni/dataModel_SleuthkitJNI.cpp>
- `bindings/java/src/org/sleuthkit/datamodel/Blackboard.java`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/src/org/sleuthkit/datamodel/Blackboard.java>
- `bindings/java/src/org/sleuthkit/datamodel/FileManager.java`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/src/org/sleuthkit/datamodel/FileManager.java>
- `bindings/java/src/org/sleuthkit/datamodel/TimelineManager.java`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/src/org/sleuthkit/datamodel/TimelineManager.java>

The Java layer exposes case database, blackboard, files, timelines, reports, and content abstractions to Java applications such as Autopsy. JNI code bridges these APIs to native TSK functionality.

### CASE/UCO Export

`case-uco/` provides a separate model/export area for CASE/UCO. This matters for interoperability and standardized forensic data exchange.

Evidence:

- <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/case-uco>
- <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/case-uco/java/README.md>

## Data / Control Flow

The remote structure supports this canonical flow:

1. Open image through `tsk/img`.
2. Identify partitions/volumes through `tsk/vs`.
3. Parse filesystems through `tsk/fs`.
4. Traverse and automate workflows through `tsk/auto`.
5. Expose command-line operations through `tools/*`.
6. Expose Java-facing datamodel through `bindings/java`.
7. Bridge native functionality through JNI.
8. Feed higher-level applications such as Autopsy.

This layering is a core strength: low-level parsing remains in C/C++, while Java bindings and Autopsy use those primitives for higher-level case management and UI workflows.

## Source-Grounded Algorithm Models

The following models are derived from remote source inspection of the pinned Sleuth Kit revision. They describe source-level control-flow intent and data structures; they are not local runtime traces.

### Layered Evidence Traversal Model

TSK's native analysis pipeline is organized around a sequence of tagged abstraction objects and function-pointer tables:

```text
TSK_IMG_INFO
  -> TSK_VS_INFO / TSK_VS_PART_INFO
  -> TSK_FS_INFO
  -> TSK_FS_FILE / TSK_FS_ATTR
  -> block/attribute reads, metadata walks, bodyfile output
```

`tsk/img/tsk_img.h` defines `TSK_IMG_INFO` as the image-reader boundary. Format-specific readers expose a common `read`, `close`, and `imgstat` style interface, so higher layers do not need to know whether bytes came from raw, EWF, VMDK, VHD, AFF, or another supported image type. `tsk/img/img_open.cpp::tsk_img_open()` follows a defensive autodetection policy: it tries specific container formats before falling back to raw-style handling, and ambiguity is reported instead of silently selecting the first successful parser.

Above the image layer, `tsk/vs/mm_open.c::tsk_vs_open()` identifies a volume system and creates `TSK_VS_INFO` / `TSK_VS_PART_INFO` records. `tsk/vs/mm_part.c::tsk_vs_part_add()` keeps partition entries ordered by start sector, while `tsk_vs_part_unused()` explicitly models unallocated sector ranges as partition records with `TSK_VS_PART_FLAG_UNALLOC`. This makes allocated, metadata, and unallocated partition space visible to callers as first-class states rather than hidden side effects.

At the filesystem layer, `tsk/fs/tsk_fs.h::TSK_FS_INFO` acts as the vtable for filesystem behavior. It carries operations such as block walking, inode walking, metadata loading, directory opening, statistics, and close handling. This lets NTFS, FAT, EXT, HFS, APFS, ISO9660, and other parsers share a stable traversal contract while preserving parser-specific implementations.

### Automated Image Traversal Model

`tsk/auto/auto.cpp` provides the high-level traversal algorithm through `TskAuto`. The central flow is:

```text
TskAuto::openImage()
  -> TskAuto::findFilesInImg()
  -> TskAuto::findFilesInVs()
  -> TskAuto::vsWalkCb()
  -> TskAuto::findFilesInFsRet()
  -> TskAuto::findFilesInFsInt()
  -> tsk_fs_dir_walk(..., dirWalkCb, ...)
  -> TskAuto::processFile()
  -> TskAuto::processAttributes()
```

`openImage()` wraps `tsk_img_open()` and stores the active `TSK_IMG_INFO`. `findFilesInImg()` then chooses the next layer: logical images go straight to `findFilesInFs(0, TSK_FS_TYPE_LOGICAL)`, while normal disk images call `findFilesInVs(0)`. If a volume system opens successfully, `tsk_vs_part_walk()` drives `vsWalkCb()` for matching partitions. Each partition can be filtered, can be checked for pool support, and is normally passed to `findFilesInFsRet()` with a byte offset derived from partition start sector and volume block size.

Once a filesystem is open, `findFilesInFsInt()` applies filesystem-level filters and calls `tsk_fs_dir_walk()` with recursive directory-walk flags. The directory callback `dirWalkCb()` invokes the virtual `processFile()` hook for each discovered `TSK_FS_FILE`. `processAttributes()` then iterates file attributes with `tsk_fs_file_attr_getsize()` and `tsk_fs_file_attr_get_idx()`, calling `processAttribute()` for content streams. The important design point is callback-driven traversal: the automation layer owns discovery and ordering, while subclasses/tools decide what to do with each file and attribute.

### Volume-System Detection Model

`tsk/vs/mm_open.c::tsk_vs_open()` models volume-system detection as an ambiguity-aware candidate search. In autodetect mode it tries DOS/MBR, BSD, GPT, Sun, and Mac partition formats. It has explicit conflict handling for cases such as GPT protective MBRs and secondary GPT tables. If multiple incompatible candidates match, it returns a multi-type error instead of guessing. If no supported volume system is found, the function also checks for encryption signals before returning an unknown/unsupported type.

This is a useful forensic design pattern: detection should preserve candidate evidence and conflict reasons because the investigator may need to distinguish "no volume system," "encrypted," "unsupported," and "ambiguous multiple matches."

### Filesystem Opening Model

`tsk/fs/fs_open.c::tsk_fs_open_img_decrypt()` applies the same ambiguity-aware strategy to filesystem opening. Logical images are routed to the logical filesystem implementation. For normal image offsets, autodetect mode iterates the `FS_OPENERS[]` table for NTFS, FAT, EXT2/3/4, UFS/FFS, YAFFS2, HFS, ISO9660, and APFS. A single successful opener becomes the filesystem handle; multiple successful openers produce `TSK_ERR_FS_MULTTYPE`.

If no filesystem opens, the source checks for unsupported image signatures at offset zero and for encrypted or possibly encrypted volume content. If the caller supplies a specific filesystem type, the function dispatches directly to the matching parser such as `ntfs_open`, `fatfs_open`, `ext2fs_open`, `hfs_open`, `iso9660_open`, `rawfs_open`, `swapfs_open`, or `apfs_open`. The algorithm therefore separates three decisions: logical-image handling, autodetected parser selection, and explicit parser dispatch.

### File, Deleted-Entry, and Content Extraction Model

TSK deliberately separates filename walking, metadata walking, and content walking. `tsk/fs/fs_dir.c::tsk_fs_dir_walk_recursive()` loads directory entries, filters them by allocated/unallocated flags, recurses into child directories, and uses a stack to avoid cycles. Deleted files are not handled by a separate parser; tools such as `tools/fstools/fls.cpp` switch walk flags like `TSK_FS_DIR_WALK_FLAG_ALLOC` and `TSK_FS_DIR_WALK_FLAG_UNALLOC` to expose allocated or deleted namespace views.

Known-inode operations use metadata paths. `tsk/fs/fs_file.c::tsk_fs_file_open()` can resolve a path with `tsk_fs_path2inum()` and then open metadata with `tsk_fs_file_open_meta()`. `tsk/fs/fs_inode.c::tsk_fs_meta_walk()` dispatches metadata walking to the active filesystem's `inode_walk` implementation. This distinction lets tools choose between user-facing paths, inode/MFT addresses, and broad metadata sweeps.

Content extraction flows through attributes. `tsk/fs/icat_lib.c::tsk_fs_icat()` opens a file by inode and calls `tsk_fs_file_walk()` or `tsk_fs_file_walk_type()`. `tsk/fs/fs_attr.c::tsk_fs_attr_read()` is the lower read boundary for resident, non-resident, sparse, compressed, filler, and slack data. `tools/autotools/tsk_recover.cpp::TskRecover::processFile()` builds on this model: it skips directories/system files/empty files, sanitizes output paths, and writes bytes through file walking rather than bypassing filesystem attribute semantics.

### Timeline and Bodyfile Model

TSK's timeline pipeline converts filesystem metadata into a simple intermediate bodyfile before sorting. `tools/autotools/tsk_gettimes.cpp::TskGetTimes::filterFs()` calls `tsk_fs_fls()` with bodyfile-oriented flags such as `TSK_FS_FLS_MAC`, directory/file inclusion, and full path output. The resulting records carry modified/accessed/changed/birth-style timestamps.

`tools/timeline/mactime.base::read_body()` reads bodyfile records, expands m/a/c/b timestamp fields into timeline buckets, and `print_tl()` emits sorted events. The model is intentionally decoupled: filesystem parsers produce normalized metadata records, while `mactime` performs temporal expansion and presentation.

### NTFS Open and MFT Attribute Reconstruction Model

`tsk/fs/ntfs.c::ntfs_open()` is the concrete example of how a filesystem parser becomes a `TSK_FS_INFO` implementation. It validates NTFS boot-sector geometry, sector size, cluster size, volume block count, MFT record size, and index record size; maps NTFS clusters to TSK filesystem blocks; assigns callbacks such as `ntfs_inode_walk`, `ntfs_block_walk`, `ntfs_load_attrs`, `ntfs_inode_lookup`, `ntfs_dir_open_meta`, `ntfs_istat`, and `ntfs_close`; then opens `NTFS_MFT_MFT` through `tsk_fs_file_open_meta()` and caches `$MFT`'s `NTFS_ATYPE_DATA` attribute in `ntfs->mft_data`. The result is that later inode lookups read MFT records through the same attribute/runlist machinery used by ordinary files instead of through a separate MFT-only byte path.

NTFS attribute reconstruction is split across `ntfs_dinode_copy()`, `ntfs_proc_attrseq()`, and `ntfs_proc_attrlist()`. Resident attributes are copied with `tsk_fs_attr_set_str()`, while non-resident attributes are decoded by `ntfs_make_data_run()` and attached with `tsk_fs_attr_set_run()` or `tsk_fs_attr_add_run()`. `$ATTRIBUTE_LIST` handling adds a second phase: `ntfs_proc_attrlist()` reads the attribute-list stream, maps repeated `(type, name)` identities to stable synthetic ids, collects extension MFT records, validates that their `base_ref` points back to the base file, and merges their attribute sequences into the logical file. This means a single logical file attribute may be reconstructed from multiple MFT entries while preserving one TSK attribute identity.

For a Rust implementation, this suggests modeling NTFS attribute identity as more than the raw NTFS attribute id. A useful internal key would include attribute type, optional name, synthetic id, source MFT record, and recoverability state. That lets the parser merge split attributes while still explaining where each run or metadata field came from.

### NTFS Data Runs and Directory Index Recovery Model

`ntfs_make_data_run()` translates NTFS variable-length runlist bytes into `TSK_FS_ATTR_RUN` entries. Physical cluster addresses are delta-encoded from the previous run, sparse ranges are marked with `TSK_FS_ATTR_RUN_FLAG_SPARSE`, and suspicious/corrupt run bounds are surfaced as parser issues. `tsk/fs/fs_attr.c::tsk_fs_attr_set_run()` and `tsk_fs_attr_add_run()` add another semantic state: `TSK_FS_ATTR_RUN_FLAG_FILLER`, used when run information starts after logical offset zero or arrives out of order from extension MFT records. Reads through `tsk_fs_attr_read()` or `tsk_fs_attr_walk()` return zeroes for sparse ranges, filler ranges, and uninitialized data past `nrd.initsize`, but these cases are not equivalent. Slack reads use allocated size rather than logical size, and compressed attributes dispatch to attribute-specific read/walk functions instead of the generic non-resident reader.

Directory recovery follows a similar evidence-preserving strategy. `tsk/fs/ntfs_dent.cpp::ntfs_dir_open_meta()` reads resident `$IDX_ROOT` entries and, for larger directories, scans non-resident `$IDX_ALLOC` including slack. The source explicitly avoids relying only on the live B-tree view because deleted file names can remain outside the active tree structure. It scans for index-record magic, processes entries, and treats recoverable corruption differently from fatal parser failure. For a forensic UI, the important model is that directory entries should carry their evidence source: live index root, live index allocation, index slack, synthetic dot entry, or orphan synthesis.

### Partition Records, Deleted Namespaces, and Error Semantics Model

The volume layer also models evidence records rather than just usable partitions. `tsk/vs/mm_part.c::tsk_vs_part_add()` inserts `TSK_VS_PART_INFO` records ordered by start sector and reassigns stable walk addresses. `tsk_vs_part_unused()` then fills gaps as `TSK_VS_PART_FLAG_UNALLOC` records. DOS/MBR and GPT parsers expose metadata records as partitions too: DOS extended tables can be added as `META`, and GPT parsing records protective/header/table areas as metadata while allocated entries carry converted GPT names. This is why a robust volume model should return `Allocated`, `Metadata`, and `UnallocatedGap` records instead of only mountable filesystems.

Deleted names and orphan metadata are separate concepts. `tsk_fs_dir_walk_recursive()` walks names, filters by allocated/unallocated flags, maintains a recursion stack, and can collect unallocated names pointing to metadata in `list_inum_named`. `tsk_fs_dir_find_orphans()` first loads the set of metadata addresses still reachable from unallocated names, then runs `tsk_fs_meta_walk(..., TSK_FS_META_FLAG_UNALLOC | TSK_FS_META_FLAG_USED, ...)` to find unallocated metadata not reachable by any deleted name. Those entries become synthetic `$OrphanFiles` children such as `OrphanFile-<inum>`. Thus deleted namespace recovery and orphan recovery are two different views: one path-driven, one metadata-driven.

TSK error handling preserves that distinction between fatal and recoverable conditions. Generic error state lives in `tsk/base/tsk_error.c`, while parsers use return categories such as recoverable corruption (`TSK_COR`), fatal error (`TSK_ERR`), and ambiguity errors like `TSK_ERR_FS_MULTTYPE` / `TSK_ERR_VS_MULTTYPE`. A Rust API should therefore avoid collapsing walks into `Result<Vec<T>, String>`; a better shape is `WalkOutcome { items, issues }`, where recoverable corruption, ambiguity, encryption detection, and fatal failure remain structured.

## Extension Points

Key extension/adaptation surfaces:

- Adding or modifying image format support in `tsk/img`.
- Filesystem/parser work in `tsk/fs`.
- CLI wrappers in `tools/`.
- Java datamodel APIs in `bindings/java/src/org/sleuthkit/datamodel/`.
- JNI bridge changes in `bindings/java/jni/`.
- CASE/UCO export improvements in `case-uco/`.

Because TSK is both a library and a tool suite, API compatibility matters across C/C++, CLI, Java/JNI, and Autopsy consumers.

## Build / Dependency / CI Posture

Build evidence:

- `INSTALL.txt`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/INSTALL.txt>
- `configure.ac`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/configure.ac>
- `Makefile.am`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/Makefile.am>
- `tsk/Makefile.am`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tsk/Makefile.am>
- `bindings/java/build.xml`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/build.xml>
- `bindings/java/jni/Makefile.am`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/jni/Makefile.am>

`INSTALL.txt` states requirements including a C/C++ compiler with C++14, GNU Make, Java compiler/JDK for Java bindings, and GNU autoconf/automake/libtool for repository builds. It also lists optional libraries for AFF, EWF, VHD, VMDK, LVM, and related image formats.

CI evidence:

- `.github/workflows/build-unix.yml`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/.github/workflows/build-unix.yml>
- `.github/workflows/compile-windows.yml`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/.github/workflows/compile-windows.yml>
- `.github/workflows/code-coverage.yml`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/.github/workflows/code-coverage.yml>
- `appveyor.yml`: <https://github.com/sleuthkit/sleuthkit/blob/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/appveyor.yml>

The presence of Unix build, Windows compile, and coverage workflows suggests active cross-platform build awareness, although this audit did not execute or verify those workflows locally.

## Testing Posture

Remote tree evidence shows:

- `tests/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/tests>
- `unit_tests/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/unit_tests>
- `bindings/java/test/`: <https://github.com/sleuthkit/sleuthkit/tree/ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e/bindings/java/test>

This indicates native and Java test coverage areas exist. This audit did not run them locally.

## Maintenance Signals

Observed release cadence is active:

| Release | Published |
|---|---|
| `sleuthkit-4.15.0` | 2026-04-15 |
| `sleuthkit-4.14.0` | 2025-04-15 |
| `sleuthkit-4.13.0` | 2025-03-11 |
| `sleuthkit-4.12.1` | 2023-08-29 |
| `sleuthkit-4.12.0` | 2023-01-25 |

The repository update timestamp observed via `gh repo view` was 2026-06-04. Releases appear aligned with Autopsy release timing, which supports the conclusion that the two projects are maintained as related components.

## Integration Notes

Sleuth Kit is Autopsy's core lower-level forensic engine. The Java datamodel and JNI artifacts under `bindings/java/` are particularly important because Autopsy's build instructions require building or consuming the Sleuth Kit datamodel JAR and native libraries.

Autopsy-facing compatibility depends on:

- Java datamodel API stability.
- JNI bridge stability.
- Native library packaging and platform dependencies.
- Schema/version compatibility in case databases and blackboard/timeline APIs.
- Release synchronization between TSK and Autopsy.

## Technical Risks

1. **Native parser complexity**: Filesystem and image parsers are inherently high-risk due to malformed/untrusted evidence inputs.
2. **JNI boundary risk**: Java/native error propagation, memory ownership, and library loading require careful testing.
3. **Cross-platform build complexity**: Autotools, Windows project files, Java build files, optional libraries, and CI all need alignment.
4. **API compatibility pressure**: TSK serves CLI users, native library consumers, Java datamodel consumers, and Autopsy.
5. **Optional image-format dependencies**: AFF/EWF/VHD/VMDK/LVM support depends on external libraries and configure options.
6. **Testing gap uncertainty**: Tests exist, but this remote-only audit did not measure coverage, pass/fail status, or sanitizer/fuzzer posture.

## Remote-Only Limitations

This report did not:

- Clone or build Sleuth Kit.
- Run `autoreconf`, `configure`, `make`, or `make test`.
- Run native unit tests.
- Run Java binding tests.
- Load JNI libraries.
- Run sanitizers, fuzzers, or coverage.
- Validate generated `configure` artifacts.
- Validate platform-specific behavior.
- Perform full vulnerability research.
- Validate the source-grounded algorithm models as runtime traces, test results, or performance measurements.

Any claims about runtime safety, parser correctness, build reproducibility, or test success should be treated as unvalidated unless supported by public CI metadata.

## Recommendations

1. **Prioritize parser hardening audits**: Future deep reviews should focus on `tsk/fs`, `tsk/img`, and boundary/error handling for malformed evidence.
2. **Strengthen JNI contract tests**: Java/native bridge behavior should have explicit tests for error conversion, resource lifecycle, and platform library loading.
3. **Maintain compatibility matrix**: Track Sleuth Kit native, Java datamodel, schema, and Autopsy release compatibility.
4. **Document optional dependency behavior**: AFF/EWF/VHD/VMDK/LVM configuration should be easy to audit and reproduce.
5. **Expose CI/test status clearly**: Publish build/test matrix details for Unix, Windows, Java bindings, and coverage.
6. **Review CASE/UCO export fidelity**: Validate which evidence/artifact types are fully represented vs lossy or unsupported.

## Evidence Appendix

Important remote commands used:

```powershell
gh repo view sleuthkit/sleuthkit --json name,description,defaultBranchRef,primaryLanguage,stargazerCount,updatedAt,licenseInfo
gh api repos/sleuthkit/sleuthkit/commits/develop-4.1x --jq .sha
gh api "repos/sleuthkit/sleuthkit/git/trees/develop-4.1x?recursive=1" --jq '[.tree[] | .path | split("/")[0]] | group_by(.) | map({name:.[0], count:length}) | sort_by(.name)'
gh api repos/sleuthkit/sleuthkit/releases --jq '.[0:5] | map({tag_name,name,published_at})'
gh api repos/sleuthkit/sleuthkit/contents/README.md?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e --jq .content
gh api repos/sleuthkit/sleuthkit/contents/INSTALL.txt?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e --jq .content
gh api repos/sleuthkit/sleuthkit/contents/tsk/img/img_open.cpp?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tsk/vs/mm_open.c?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tsk/fs/fs_open.c?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tsk/auto/auto.cpp?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tsk/fs/fs_dir.c?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tsk/fs/fs_attr.c?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tools/autotools/tsk_recover.cpp?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tools/autotools/tsk_gettimes.cpp?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
gh api repos/sleuthkit/sleuthkit/contents/tools/timeline/mactime.base?ref=ee77d61aea3dc4371c391f0d7b23dfd586ba8d2e
```
