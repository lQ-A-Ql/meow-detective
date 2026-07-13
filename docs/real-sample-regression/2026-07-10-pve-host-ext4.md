# PVE 宿主 EXT4 回归

**日期**: 2026-07-10
**链路类型**: E01 -> GPT -> LVM `pve/root` -> 64-bit EXT4 -> file tree / `FileEntryId` preview
**样本标识**: private/pve/server01-disk01.E01
**SHA-256**: `AE1806B49754FBC2C6ABB9219CE5EEA98EF59F3C0D442B44BDA0E2B4FC95F841`
**大小**: 2,978,936,364 bytes
**运行环境**: Windows host, Rust stable toolchain, opt-in real sample test

## 运行命令

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
cargo test -p app-services --test linux_e01_integration pve_cluster_host_root_filesystems_enumerate_and_preview -- --ignored --nocapture
cargo test -p app-services --test linux_e01_integration pve_cluster_representative_host_imports_tree_and_previews_by_file_id -- --ignored --nocapture
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 -RequireFixture
```

## Six-member import lifecycle gate

The desktop real-sample gate uses the production background cluster runner to
attempt all six E01 members serially. The three `disk01` members must become
ready and preview key files from independent `source.db` files. The three
`disk02` BlueStore members must become metadata-only sources. They must not
prevent later members from registering or masquerade as POSIX filesystems.

The pre-fix run observed `ready=3`, `failed=3`; each BlueStore member retained
an isolated diagnostic `source.db` with zero file entries and reported the
generic error `No supported filesystem partitions were detected`.

On 2026-07-13 the probe path was hardened and the private six-member gate was
rerun successfully. The three `disk02` images are whole-disk LVM PVs. Their
readable OSD logical volumes carry the upstream Ceph signature
`bluestore block device` at LV-relative offset `0`; for `server01-disk02` this
maps to image offset `1 MiB`. The previous generic failure occurred because LVM
expansion only retained LVs whose first bytes matched a supported POSIX
filesystem and silently discarded the BlueStore LV before import classification.

The detector checks Ceph's official device-relative label positions `0`,
`1 GiB`, `10 GiB`, `100 GiB`, and `1000 GiB` against the logical block device,
not the outer E01 or PV address space. The native decoder validates the Ceph
struct envelope and CRC32C, selects the highest consistent multi-label epoch,
removes `osd_key`, and persists only sanitized inventory metadata. BlueStore is
never added as an `ImageFilesystemKind`, filesystem candidate, or file-tree
root. The real gate requires OSD IDs `0,1,2`, one shared cluster FSID, unique
OSD UUIDs, zero file rows, and continued import of all three `disk01` members.
The post-fix production run completed all six members: the three filesystem
members became `ready`, the three BlueStore members became `ready_metadata`,
and the host source DB file counts were `62,403`, `62,380`, and `62,405`.

The 2026-07-13 Stage 2 rerun additionally decoded the fixed 4 KiB BlueFS
superblock at LV offset `4096`. All three records passed independent CRC32C and
OSD UUID binding, reported sequence `50` and block size `4096`, and persisted
one bounded shared-device log extent. BlueFS and OSD inventory are committed
atomically in each source database.

样本路径仅是本地人工环境示例，生产代码和默认 CI 不得硬编码该路径。

## 结果

通过。

- `server01`、`server02`、`server03` 的 `disk01` 均发现 `pve/root` EXT4 logical volume。
- 三个宿主均可枚举 `/etc`、`/usr`、`/var` 并读取 `/etc/passwd`、`/etc/os-release`、`/etc/hostname`、`/var/lib/pve-cluster/config.db`。
- 代表成员生产导入结果：`files=56471`、`dirs=5931`、`totalBytes=5250350224`。
- 代表成员测试体耗时约 `4.59s`（debug build，仅作本机回归参考）。
- 原始缺陷是 EXT4 reader 固定按 32-byte group descriptor 寻址；样本使用 64-bit EXT4 的 64-byte descriptor，导致高 inode 被映射到错误 inode table。
- 修复后 group descriptor 宽度、inode table 高位和 block group 数量均来自 superblock；有界 inode-block cache 在回填真实文件大小时避免 E01 随机读退化。
- 三个 `disk02` 的 BlueFS UUID 分别为 `394d12df-4023-44dc-b4c5-10b5e5dd48f4`、`e1b8a63e-3c93-4743-8232-b236b82fec83`、`d8f0162e-aefe-4397-ad64-16b28af988a1`；均与对应 OSD UUID 一对一绑定。

## 未保证范围

- `disk02` 已由生产 LVM LV reader 按 Ceph 官方标签确认是 BlueStore OSD block device，并保存脱敏 label metadata 与 BlueFS superblock/layout inventory。本文记录的是 2026-07-10 Stage 2 边界；后续 BlueFS metadata-log replay 结果见 `docs/real-sample-regression/2026-07-13-pve-bluefs-stage3.md`。BlueStore 仍不是可直接枚举的 POSIX 文件系统，RocksDB 内容、RADOS object/PG/VM disk 文件树不在当前支持范围。
- 当前 metadata-only 路径只承诺“无普通文件系统 candidate 且可唯一选择一个 BlueStore LV”的数据源；混合 filesystem + BlueStore 单源和多 BlueStore LV 尚未支持。
- 不承诺 EXT4 metadata checksum 校验、deleted recovery、journal replay 完整性或全部 incompat feature 组合。
- 不承诺集群跨节点配置归并和语义关联。
