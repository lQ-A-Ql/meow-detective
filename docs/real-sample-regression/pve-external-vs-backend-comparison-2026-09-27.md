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

差值在三台主机上完全相同，不能解释为随机样本缺失。进一步对 server01 做了外部 ESP 只读挂载：ESP 有 **13** 个子项。后端导入链路还会为 GPT BIOS boot 分区和 ESP 分区各保留一个 partition-root Catalog 节点，并为 `pve/root` 保留一个文件系统根节点。于是：

```text
外部 pve/root 子项                  62,387
+ ESP 子项                              13
+ BIOS/ESP partition-root 节点           2
+ pve/root 文件系统根节点               1
                                      -----
后端 file_entries                      62,403
```

三台主机都满足同一 `+16` 关系。外部 root 挂载树还分别包含约 5,011 个符号链接和 33 个特殊节点；后端计数是整块镜像统一 Catalog 计数，而不是只计 `/dev/pve/root` 的后代。

**结论**：这不是样本缺失，也不是文件内容漏采，而是外部比较最初只统计了宿主 root LV，后端统计了整张镜像的 ESP、分区根和 root 节点。该差异已解释闭合，不需要修改枚举器。

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

### 本轮性能复跑

原配置将六成员限制为串行成员导入，且 Linux artifact 自动解析在每个成员导入线程内同步执行。单宿主生产导入回归实测约 314 秒；三个宿主再叠加 BlueStore/RBD 阶段后，六成员回归超过 1,200 秒 guard。

本轮做了两项调整：

1. Linux evidence-set 成员先发布 Catalog/`ready`，Linux artifact extraction 改为独立后台任务。分析仍受全局 extraction gate 保护，不会并发写同一 source DB；它不再阻塞下一个成员的 E01/BlueStore 读取和哈希。
2. 集群低 worker 配置允许三成员同时 admitted。每成员仍限制为一个导入 worker 时，磁盘读取并行度提高到 3；CPU/内存 admission 仍限制总权重。后台 artifact task 也持久化为独立 Job，成功/失败可追踪。

激进磁盘调度复跑结果（本次回归入口测量的是 Catalog/BlueStore/RBD 可浏览链路；它没有等待桌面 TaskManager 中随后排队的 Linux artifact 任务和镜像 hash 任务）：

| 指标 | 串行超时复跑 | 三成员并行复跑 |
|---|---:|---:|
| 测试结果 | 1,200 秒超时 | 通过 |
| 集群状态 | 未形成新结果 | `ready` |
| ready/failed | 未形成新结果 | `6 / 0` |
| browseable 阶段 | >1,200 秒 | 451,987 ms |
| 派生后处理 | 未形成新结果 | 158,467 ms |
| 测试进程总耗时 | >1,200 秒 | 786.43 秒 |
| 峰值 RSS | 未形成新结果 | 约 1,291 MiB |

后端 semantic oracle、RBD Catalog 和成员隔离均通过。性能瓶颈主要集中在：

- 三个 BlueStore semantic snapshot 的 RocksDB/SQLite 解析与持久化；
- RBD 派生 VM 文件树 Catalog 构建与文件系统枚举；
- 旧链路中每个宿主同步 Linux artifact extraction 对成员队列的阻塞。

并行度提高有效消除了本轮超时，但峰值 RSS 上升到约 1.3 GiB，不能继续无上限增加成员数。当前实现还把 artifact extraction 和 hash 任务拆成独立后台 Job；完整案件“分析完成”时间应以这些 Job 的终态为准，而不是以 cluster parent job 的 `ready` 为准。生产默认采用每成员最多 2 个导入/分析 worker、最多 3 个成员并行的 weighted admission；内存不足时自动降低并行度。

### 已通过

- PVE 三宿主 LVM/EXT4 probe 与 root LV 发现
- 宿主 identity 可读性回归
- 宿主 root 文件树和关键文件预览回归
- 三个 OSD 的 BlueStore label、OSD ID、UUID、FSID、epoch oracle

### 需要修正或继续核对

1. `pve_cluster_representative_host_imports_tree_and_previews_by_file_id` 的旧断言要求 metadata-only import 的 LinuxSystemConfig artifact 数为 0；当前自动 artifact 解析产品行为返回 `5785`，测试断言已按产品契约更新。这是**测试契约落后于产品行为**，不是样本缺失或解析失败。
2. 容器、静态 `/etc/pve`、OSDMap/CRUSH/PG、RBD 完整外部闭合证据仍未从本样本中得到，后端必须保持显式 partial/unsupported 边界。

## 8. 最终分类

| 项目 | 分类 |
|---|---|
| GPT/LVM/root EXT4 | 外部与后端一致 |
| hostname/OS/kernel | 外部与后端一致，服务单元也可外部确认 |
| `/etc/pve` 静态文件 | 样本中缺失，后端正确保持缺失 |
| Docker 容器 metadata | 外部未发现，不能判定为后端漏显示 |
| BlueStore label/OSD identity/FSID/epoch/size | 外部与后端一致 |
| BlueFS/semantic 行数与 digest | 后端内部 oracle 已通过，尚缺独立外部复算 |
| 宿主 file-entry 数 | 已解释：ESP 13 项 + BIOS/ESP 两个分区根 + root 文件系统根节点 |
| RBD VM 文件树 | 后端能力已有，外部集群映射证据尚未闭合 |
| 自动 Linux artifact | 已拆为集群成员完成后的独立后台 Job，旧测试断言已更新 |
