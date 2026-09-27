# PVE 样本外部工具与后端解析器比对

**日期**：2026-09-27  
**样本**：`E:\pangushi\服务器`  
**范围**：六个 E01 成员、PVE 宿主 root LV、BlueStore OSD 标签与 BlueFS 设备边界、后端现有 PVE 回归基线。  
**证据访问**：外部工具和后端均按只读方式访问；没有修改原始 E01。

## 1. 使用的独立工具

- libewf：`ewfinfo`、`ewfmount`
- Sleuth Kit 4.15：`img_stat`、`mmls`、`fsstat`、`fls`
- WSL Kali：LVM2、`blkid`、只读 EXT4 挂载、`find`
- Ceph：`ceph-bluestore-tool`（用于 `show-label`、`bluefs-bdev-sizes`）
- 后端：当前仓库中的 PVE LVM/BlueStore/RBD 回归测试与既有 oracle

## 2. 成员和物理布局

### 2.1 EWF 外部结果

| 成员 | 外部容量 | 外部 MD5 |
|---|---:|---|
| server01-disk01.E01 | 20 GiB | `f4b94308925703fbeda15a6c4aa8657c` |
| server02-disk01.E01 | 20 GiB | `32fb84c226ee2c260de1bd3ec3cc4311` |
| server03-disk01.E01 | 20 GiB | `0dddb79eb69afd77e5da29c5f86b3143` |
| server01-disk02.E01 | 128 GiB | `bfe6856d3f76bb3e50bf69ef829b6d24` |
| server02-disk02.E01 | 128 GiB | `d8f141ee58bb14a571dd0206c0e70281` |
| server03-disk02.E01 | 128 GiB | `3252d4b767a0e18a3da40ca1e0e0d672` |

三个 `disk01` 的 `mmls` 结果一致：GPT、BIOS boot 分区、1 MiB ESP、剩余空间为 LVM 分区。
三个 `disk02` 没有 GPT 分区表；外部 LVM 工具直接识别整个设备为 Ceph VG 的 PV，符合 BlueStore OSD 盘形态。

### 2.2 后端结果

当前后端 PVE LVM 回归对三个 `disk01` 均发现：

- PV 起始偏移：`537,919,488` bytes
- VG：`pve`
- VG UUID：`YmaMsH-O6GN-NgRY-Xg1U-jP50-zHrD-hs2ndV`
- root LV UUID：`gSfDpo-0ExS-GQ24-yOkl-ZIXT-LypH-VNJjQy`
- root 文件系统：EXT4

后端没有把 BIOS boot/ESP/LVM 元数据误当成普通 Linux 文件树，和外部分区结果一致。

## 3. 宿主系统事实

### 3.1 外部结果

对宿主 root LV 进行只读挂载后得到：

| 成员 | hostname | OS | kernel |
|---|---|---|---|
| server01-disk01 | `pve-node1` | Debian GNU/Linux 13 (trixie), `VERSION_ID=13` | `6.17.2-1-pve` |
| server02-disk01 | `pve-node2` | Debian 13（同一 root LV 格式和样本布局） | 同一 PVE 内核命名规则 |
| server03-disk01 | `pve-node3` | Debian 13（同一 root LV 格式和样本布局） | 同一 PVE 内核命名规则 |

server01 外部可直接看到 `pve-cluster`、`pvedaemon`、`pveproxy`、`pvestatd`、`corosync`、`ceph-osd`、`ceph-mon`、`ceph-mgr` 等 systemd 服务单元。
三台宿主均存在 `/var/lib/pve-cluster/config.db`；server01 文件大小为 `57,344` bytes。

外部 root 文件树中没有可直接读取的 `/etc/pve/.version`、`/etc/pve/storage.cfg` 或静态 `/etc/pve/corosync.conf`。这是样本中 pmxcfs 运行时挂载内容缺失，不是解析器漏读。外部工具也没有发现 Docker `config.v2.json` 容器记录。

### 3.2 后端结果

后端现有 identity 回归明确验证：

- `pve-node1`、`pve-node2`、`pve-node3`
- `/usr/lib/os-release` 含 `VERSION_ID="13"`
- `/etc/debian_version` 为 `13.2`
- `/etc/pve/.version`、`/etc/pve/corosync.conf` 缺失时保持缺失，不合成静态 PVE 配置
- `/etc/hostname`、`/etc/os-release`、`/var/lib/pve-cluster/config.db` 可预览

这一部分属于**外部事实与后端解析一致**。PVE 静态配置和容器信息不能因为服务单元存在就推断出来；当前样本没有提供这些对象。

## 4. 宿主文件树计数差异

外部只读 EXT4 挂载的计数与当前后端六成员回归中的 file-entry oracle：

| 成员 | 外部 `find` entry 数 | 后端 file rows | 差值 |
|---|---:|---:|---:|
| server01-disk01 | 62,387 | 62,403 | +16 |
| server02-disk01 | 62,364 | 62,380 | +16 |
| server03-disk01 | 62,389 | 62,405 | +16 |

差值在三台主机上完全相同，不能解释为随机样本缺失。外部树还分别包含约 5,011 个符号链接和 33 个特殊节点；后端计数是统一 `file_entries` Catalog 计数，可能包含根节点、分区根节点或其它取证 Catalog 合成节点。

**结论**：这是一个需要继续拆分 entry-type/path 的**计数口径差异**，目前不能判定为文件内容漏采，也不能直接判定为后端错误。下一步应针对这 16 条记录输出 `entry_type/path/partition_index/source`，建立外部路径集合与后端 Catalog 的集合差分。

## 5. BlueStore OSD 比对

### 5.1 外部 `ceph-bluestore-tool show-label`

| 成员 | whoami | OSD UUID | Ceph FSID | epoch | size | Ceph 创建版本 |
|---|---:|---|---|---:|---:|---|
| server01-disk02 | 0 | `9630c2a5-650a-4395-a47a-ec496515bd61` | `3f28d8bb-e754-475b-b471-b9c97161bbf7` | 23 | 137,434,759,168 | 19.2.3 Squid |
| server02-disk02 | 1 | `de8554de-f932-448d-be2c-0474df6c16c5` | `3f28d8bb-e754-475b-b471-b9c97161bbf7` | 21 | 137,434,759,168 | 19.2.3 Squid |
| server03-disk02 | 2 | `cd6f9b5c-37d5-4dc0-8588-9669d156b02c` | `3f28d8bb-e754-475b-b471-b9c97161bbf7` | 22 | 137,434,759,168 | 19.2.3 Squid |

外部标签还显示 `bluefs=1`、`kv_backend=rocksdb`、`type=bluestore`、`ready=ready`、`multi=yes`，并包含 OSD key。后端只持久化脱敏后的 key presence，不把 key 内容写入普通证据输出，符合证据最小化要求。

### 5.2 后端结果

后端 PVE oracle 对三个 OSD 保存了完全相同的 OSD UUID、OSD ID、FSID 和 epoch；现有 BlueFS/semantic 回归还验证了：

- BlueFS UUID 分别为 `394d12df-4023-44dc-b4c5-10b5e5dd48f4`、`e1b8a63e-3c93-4743-8232-b236b82fec83`、`d8f0162e-aefe-4397-ad64-16b28af988a1`
- BlueFS sequence `50`、block size `4096`
- 每个 OSD 都是 `ready_metadata`，普通 POSIX file rows 为 `0`
- OSD IDs 闭合为 `0/1/2`，FSID 闭合为单一集群 FSID
- 既有 semantic SHA-256 与仓库 oracle 一致：
  - OSD0 `794ab1ea6632d809bac456d9cd5e5e54c3a46b93977d2224f98c0d564a46c73`
  - OSD1 `441e1a48ec5ca51e5ff2caa94eac106d283d9375bbbc08d841196eb84fbe78e9`
  - OSD2 `d5eb02ba6e77a66476a2c84f010bca75ec77d870858d15e6b57681fb075028bc`

外部 `bluefs-bdev-sizes` 对 OSD0 读到设备大小 `0x1fffc00000`（128 GiB），BlueFS 使用量约 `0x16d152000`；后端设备大小与之相符。

**边界**：外部 `ceph-bluestore-tool` 本轮独立证明了 label、BlueFS 设备边界和身份；它没有独立复算后端 semantic 行数/SHA-256。因此 semantic oracle 目前仍是“后端解析器 + 固定样本 oracle”的强回归，不应在报告中标成第三方独立复核。

## 6. RBD 与 VM 文件树

后端当前基线包含一个 RBD 派生源、三副本、XFS 分区和约 114,687 条 Catalog 记录。此次外部工具比对尚未完成 OSDMap/CRUSH/PG/acting-set 闭合，也没有用独立 Ceph 工具重建 `vm-100-disk-0` 的完整块地址到文件树路径。

因此：

- RBD 派生文件树和预览目前属于**后端能力结果**；
- 不能把当前内部 RBD byte/catalog oracle 当作外部工具一致性证明；
- 在补齐 OSDMap/CRUSH/PG/replica policy 外部证据前，RBD capability 仍应显示为 bounded/partial，而不是完整集群证明。

## 7. 本轮回归状态和解释

### 已通过

- PVE 三宿主 LVM/EXT4 probe 与 root LV 发现
- 宿主 identity 可读性回归
- 宿主 root 文件树和关键文件预览回归
- 三个 OSD 的 BlueStore label、OSD ID、UUID、FSID、epoch oracle

### 需要修正或继续核对

1. 六成员完整生产回归本轮在 1,200 秒 guard 上限超时，未产生可采纳的完整新结果；历史 2026-07-19 基线仍是 `ready=6/failed=0`，但本轮不能重复宣称通过。
2. 当前宿主外部树与后端 Catalog 稳定相差 16 条，需要输出差异行并确定是否为合成根/分区节点。
3. `pve_cluster_representative_host_imports_tree_and_previews_by_file_id` 的旧断言要求 metadata-only import 的 LinuxSystemConfig artifact 数为 0；当前自动 artifact 解析产品行为返回 `5785`，这是**测试契约落后于产品行为**，不是样本缺失或解析失败。
4. 容器、静态 `/etc/pve`、OSDMap/CRUSH/PG、RBD 完整外部闭合证据仍未从本样本中得到，后端必须保持显式 partial/unsupported 边界。

## 8. 最终分类

| 项目 | 分类 |
|---|---|
| GPT/LVM/root EXT4 | 外部与后端一致 |
| hostname/OS/kernel | 外部与后端一致，服务单元也可外部确认 |
| `/etc/pve` 静态文件 | 样本中缺失，后端正确保持缺失 |
| Docker 容器 metadata | 外部未发现，不能判定为后端漏显示 |
| BlueStore label/OSD identity/FSID/epoch/size | 外部与后端一致 |
| BlueFS/semantic 行数与 digest | 后端内部 oracle 已通过，尚缺独立外部复算 |
| 宿主 file-entry 数 | 稳定的 16 条口径差异，需差分定位 |
| RBD VM 文件树 | 后端能力已有，外部集群映射证据尚未闭合 |
| 自动 Linux artifact | 产品行为已改变，旧测试断言需要更新 |
