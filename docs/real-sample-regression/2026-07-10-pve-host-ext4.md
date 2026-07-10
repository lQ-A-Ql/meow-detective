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
```

样本路径仅是本地人工环境示例，生产代码和默认 CI 不得硬编码该路径。

## 结果

通过。

- `server01`、`server02`、`server03` 的 `disk01` 均发现 `pve/root` EXT4 logical volume。
- 三个宿主均可枚举 `/etc`、`/usr`、`/var` 并读取 `/etc/passwd`、`/etc/os-release`、`/etc/hostname`、`/var/lib/pve-cluster/config.db`。
- 代表成员生产导入结果：`files=56471`、`dirs=5931`、`totalBytes=5250350224`。
- 代表成员测试体耗时约 `4.59s`（debug build，仅作本机回归参考）。
- 原始缺陷是 EXT4 reader 固定按 32-byte group descriptor 寻址；样本使用 64-bit EXT4 的 64-byte descriptor，导致高 inode 被映射到错误 inode table。
- 修复后 group descriptor 宽度、inode table 高位和 block group 数量均来自 superblock；有界 inode-block cache 在回填真实文件大小时避免 E01 随机读退化。

## 未保证范围

- `disk02` 已确认是 Ceph BlueStore OSD block device，不是可直接枚举的 POSIX 文件系统；当前不提供 RADOS object/PG/VM disk 文件树。
- 不承诺 EXT4 metadata checksum 校验、deleted recovery、journal replay 完整性或全部 incompat feature 组合。
- 不承诺集群跨节点配置归并和语义关联。
