# 检材3 LV 与 RAID 分析结论

## 1. 分析对象

分析对象为 `L:\检材3-服务器`，目录包含：

- `检材3-1.E01`
- `检材3-2.E01`

两个 E01 都能被现有 `image-e01::E01Reader` 打开。TSK `ewfinfo` 报告每个文件对应 60 GiB、512 字节扇区的 FTK Imager 固定磁盘映像。

## 2. 外层布局

TSK `mmls` 结果：

### `检材3-1.E01`

- 两个 Linux LVM 分区。

### `检材3-2.E01`

- EFI 分区。
- Linux swap 分区。
- 两个 Linux LVM 分区。

对 LVM 分区直接运行 TSK `fsstat` 返回无法确定文件系统类型。这个结果符合工具边界，因为它还没有获得跨 E01 的 LV 视图。

## 3. LVM 组合结果

当前提交 `fbbd4809` 已将 Linux 证据集合中的其他成员作为 `LvmDiscoverySource` 传入探测流程。应用可以在同一个 VG 中同时看到两个 PV。

已发现的 VG/LV：

| VG | LV | 角色 |
|---|---|---|
| `root` | `data` | public |
| `volum` | `root` | public |

`volum/root` 的 LVM extent map：

```text
logical_start=0
physical_offset=4201644032
length=29997662208
pv=pv0

logical_start=29997662208
physical_offset=2097152
length=29997662208
pv=pv1
```

两个 extent 连续覆盖 `59995324416` 字节。LVM 层的跨 PV 读取具备可行性。

## 4. LV 内部 RAID 证据

在 LV 逻辑偏移 `4096` 处发现：

```text
FC 4E 2B A9
```

这个值对应 Linux MD RAID v1 superblock magic `0xA92B4EFC`。同一结构的字段显示：

- major version：`1`。
- array UUID：`88 0F 20 57 30 E6 4D A6 5B FD B0 6F 70 01 31 BB`。
- name：`mac:0`。
- level：`0`。
- layout：`1`。
- chunk size：`1024` 个 512 字节扇区，即 512 KiB。
- raid disk count：`2`。
- 当前成员编号：`1`。
- data offset：`67584` 个 512 字节扇区。
- superblock offset：`117110784` 个 512 字节扇区。
- recovery offset union：`8` 个 512 字节扇区。

这组字段足以把 RAID 作为首要分析假设。现有 LVM reader 仍然把 LV 当作直接 block reader，所以文件系统探测在 RAID 层之前执行，最终报告“没有支持的文件系统”。

第二个 RAID 成员已经确认：`root/data` LV，成员编号 `0`。当前不能宣布阵列已经可读，因为两个成员的 data size 不同，且尚未完成 RAID0 reader 重组后的文件系统验证。

独立只读重组已经得到进一步文件系统证据：按两个成员的 RAID0 chunk 规则重组约 1 GiB 片段后，在重组逻辑空间发现有效 Btrfs superblock。该 superblock 的 fsid 为 `32-31-E5-2F-5E-15-44-C4-B2-24-E2-9C-B4-20-1C-0E`，generation 为 `4389`，sectorsize 为 `4096`，nodesize 为 `16384`，total bytes 为 `124568731648`，bytes used 为 `26126225408`。

Btrfs total bytes 大于当前两个 MD 成员 data zone 形成的逻辑容量，说明仍需检查 Btrfs chunk tree、设备条带和可能的多设备布局。当前代码已经能够递归读取 chunk tree，并在真实检材的 RAID0 重组 reader 上打开 Btrfs、枚举 `@rootfs` 根目录、读取 `/etc/passwd`；这还不能作为完整文件树验收。

高价值路径覆盖已经扩展到 `@rootfs/etc`、`@rootfs/var`、`@rootfs/usr`、`@rootfs/root` 和 `@rootfs/home`。`/etc/passwd`、`/etc/os-release`、`/etc/hostname` 均可读取；`/var/log/syslog`、`/var/log/messages` 在本检材中不存在，已按缺失证据记录，没有当作读取故障。

## 5. 当前结论

| 层级 | 结论 | 证据状态 |
|---|---|---|
| E01 | 两个输入均可读取 | 已确认 |
| 外层分区 | LVM/EFI/swap 分区可识别 | 已确认 |
| 跨 E01 PV | 同 VG 的两个 PV 可组合 | 已确认 |
| LV extent map | `volum/root` 两段 extent 连续 | 已确认 |
| LV 内 MD RAID | RAID v1 superblock 候选、level 0、两成员 | 高可信候选 |
| RAID 成员完整性 | 成员编号 0 和 1 已匹配，data size 不同 | 已匹配，待一致性验证 |
| 文件系统 | 直接 LV 首部没有 XFS/EXT4/Btrfs 标识；RAID0 重组片段发现 Btrfs superblock | RAID 后已确认候选 |
| 文件树 | RAID0 reader 已接入并可枚举 `@rootfs`、读取 `/etc/passwd`；完整多设备范围仍待验证 | 部分完成 |

## 6. 读取路径调整

当前样本不能按以下路径读取：

```text
E01 → LVM PV → VG → LV → 文件系统
```

需要验证并实现：

```text
E01 → LVM PV → VG → LV → MD RAID0 → 文件系统
```

第一阶段只处理 RAID0。阵列 reader 需要按照 chunk size、layout 和成员编号把逻辑读请求分配到两个 LV reader，并按较小的 data size 截断逻辑容量。重组后的 MD reader 再交给 XFS/EXT4/Btrfs 探测器。

## 7. 不应采用的判定方式

- 看到一个 magic 就直接创建文件系统 reader。
- 把 LV 首部全零直接判为损坏文件系统。
- 把 LV 中部出现的 `NTFS`、`LUKS`、`XFSB` 字符串当作独立文件系统。
- 只使用一个 E01 的 PV 读取跨成员 LV。
- 借助宿主机 `mdadm` 或 LVM 激活修改证据环境。

候选格式必须同时通过固定位置、容量、UUID、成员关系和 checksum 验证。

## 8. 下一步验证

1. 在所有集合成员的全部可读 LV 中按 MD v1 superblock 规则扫描。
2. 按阵列 UUID 分组。
3. 验证两个成员的 `data_size`、`data_offset`、superblock offset、chunk、layout、checksum 和事件状态。
4. 用两个成员构建只读 RAID0 reader，并按 MD RAID0 zone 计算逻辑容量。
5. 在重组后的逻辑空间检查 XFS AG superblock、EXT4 superblock 和 Btrfs superblock。
6. 对确认的文件系统执行 TSK `fsstat`、`fls` 交叉验证。
7. 若成员缺失或元数据冲突，保留 `MdArrayIncomplete` 或 `MdArrayConflicted`，不创建文件树。

## 9. 证据边界

本报告中的 RAID 结论来自 E01 只读读取、TSK 外层分区分析和应用内部 LVM reader 的有界读取。两个 RAID 成员已经匹配，RAID reader 已能打开 Btrfs 并枚举根目录；完整多设备 chunk 范围、文件预览和制品提取仍需继续验证。
