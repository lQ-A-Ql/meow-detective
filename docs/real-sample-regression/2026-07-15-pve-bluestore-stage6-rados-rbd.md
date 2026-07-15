# PVE BlueStore Stage 6.5/6.6 真实样本回归

## 基线

- 日期：2026-07-15
- 样本：`E:\pangushi\服务器`
- 模式：六成员串行导入，`max_import_workers=1`、
  `max_analysis_workers=1`、metadata-only analysis
- 证据访问：E01、LVM LV、BlueStore block device 全程只读
- 关联前置：`docs/real-sample-regression/2026-07-15-pve-bluestore-stage6-semantic.md`

执行命令：

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT='E:\pangushi\服务器'
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -RequireFixture -TimeoutSeconds 3600
```

## 结果

本轮真实回归耗时 `353.71s`，结果为：

```text
PVE cluster outcome: state=ready, ready=6, failed=0
```

成员状态：

| 成员 | 状态 | 普通文件行 |
|---|---|---:|
| server01-disk01.E01 | `ready` | 62,403 |
| server02-disk01.E01 | `ready` | 62,380 |
| server03-disk01.E01 | `ready` | 62,405 |
| server01-disk02.E01 | `ready_metadata` | 0 |
| server02-disk02.E01 | `ready_metadata` | 0 |
| server03-disk02.E01 | `ready_metadata` | 0 |

三个 BlueStore source DB 继续通过 Stage 6.4 semantic count/digest oracle：

| OSD | objects | blobs | physical extents | checksums | semantic SHA-256 |
|---|---:|---:|---:|---:|---|
| server01 | 2,924 | 116,135 | 134,148 | 1,839,658 | `794ab1ea6632d809bac456d9cd5e5e54c3a46b93977d2224f98c0d564a46c73b` |
| server02 | 2,927 | 116,135 | 134,154 | 1,839,666 | `441e1a48ec5ca51e5ff2caa94eac106d283d9375bbbc08d841196eb84fbe78e9` |
| server03 | 2,930 | 116,135 | 134,150 | 1,839,646 | `d5eb02ba6e77a66476a2c84f010bca75ec77d870858d15e6b57681fb075028bc` |

## 本轮修复

真实样本第一次暴露的失败是：

```text
BlueStore OMAP scope family=PerPg ... has no header
```

原因是非 RBD 的 `PerPg`/`PgMeta` scope 可以没有 `Header` marker，旧状态机
却把所有 entry 都视为 RBD header scope。修复后：

- 所有 OMAP family 的无 Header scope 可以由 entry + tail 闭合。
- 无 Header scope 只保存 scope、entry count 和 recognized count 元数据。
- 无 Header scope 不解码 RBD 字段，不绑定 RBD owner，不生成 directory/header。

相关合成测试：

- `accepts_headerless_pg_metadata_scopes_without_rbd_projection`
- `accepts_headerless_bulk_and_per_pool_scopes_without_rbd_projection`

## Stage 6.5/6.6 实现边界

当前已落地的 foundation：

- source DB OMAP aggregate 与 RBD directory/header catalog。
- source-bound BlueStore LVM 重新打开与设备身份校验。
- RADOS logical/blob/physical extent range reader、sparse hole 零填充和
  CRC32C 校验。
- 多 source DB 仅按显式配置的 inventory 集合读取；副本内容冲突时拒绝静默选择。
- 同一 inventory 或同一 data source 不能重复计入 expected replica count。
- RBD head image object striping、`Read + Seek + EvidenceReader` 和现有
  MBR/GPT/LVM/EXT4/XFS/Btrfs filesystem probe 复用。
- `layering`、journaling、parent/clone、snapshot、encryption、data-pool
  等会改变读取语义的 RBD feature typed fail closed。

当前不能宣称完成：

- 本次 PVE 样本尚未建立真实 RBD image ID 与 `rbd export`/`rbd-nbd`
  独立 byte oracle。
- 未恢复 PG/CRUSH/acting set/EC placement，也未证明 intended replica set。
- 未创建 VM derived source、VM 文件树、VM 文件预览或 CephFS 文件树。
- `ready_metadata` 仍是正确状态，不能改为普通 POSIX 文件系统 `ready`。

## 验证摘要

定向测试：

```text
app-services ceph_reconstruction: 39 passed
app-services ceph_bluestore_omap: 8 passed
persistence-sqlite ceph_bluestore_omap_repo: 5 passed
persistence-sqlite ceph_osd_device_binding_repo: 4 passed
ceph-wire: passed
app-services clippy --lib --no-deps: passed
```

最终收口还通过：

- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- frontend typecheck、lint、86 个 Vitest 文件 / 547 项测试和 production build
- module/function/test-layout、Stage 0/2/3/4/5/6、SQL、media、release、
  dependency、documentation 和 benchmark guards

通用 `检材2.E01` 性能门禁的功能、完整性、时间和内存指标通过；第二次
三轮复测为 total median `24.315s`、enumeration median `14.828s`、
peak RSS `584MB`、每轮 `91,737` rows，中位吞吐 `6,187 rows/s`。其中单次
`5,634 rows/s` 低于脚本要求的“每一轮均不低于 `6,000 rows/s`”，因此该
通用性能门禁仍保留为未全绿的波动信号；未修改阈值，也不把它误记为 Stage 6
Ceph 正确性失败。

本回归证明的是：OMAP 非 RBD scope 不再阻断导入，source DB/RADOS/RBD
foundation 可以在真实六成员串行导入中完成并保持语义 snapshot 不变；它不是
RBD 或 CephFS 的完整取证验收。
