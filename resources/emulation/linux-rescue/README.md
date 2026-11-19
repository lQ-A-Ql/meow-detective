# Linux 救援手动路径（linux-rescue）

本目录只包含**自研**内容：`BYPASS.SH` 手动补丁脚本与本说明。产品**不随包分发**任何 GPL/LGPL 操作系统、内核、initramfs、BusyBox、appliance ISO 或第三方镜像，也不提供自动救援 VM 路径。

## 1. 何时需要它

主机侧就地改写（A 路径）在以下阻塞条件下被判定不安全，无法直接改仿真盘副本上的 `/etc/shadow`：

| 阻塞 | 含义 |
| --- | --- |
| `SharedExtents` | 目标文件在 XFS 上使用 reflink 共享 extent，就地改写会污染共享副本 |
| `DirtyLog` | XFS 内部日志不洁，就地改写前无法安全收敛 |
| `UnsupportedLayout` | inode 版本 / extent 布局 / 本地 fork 等不满足安全谓词 |

此时由用户在**自备的救援环境**中手工执行 `BYPASS.SH`，把 payload CD 里的新 shadow 写到已物化的仿真盘 COW 副本上。

## 2. 使用前提（许可边界）

- 用户自行准备 recovery ISO（产品既有 `recovery_iso_path` 配置指向的镜像），**该 ISO 的许可与合法性由用户自行负责**；本仓库不携带、不下载、不链接该 ISO。
- 仓库内容仅为自研 Rust/TypeScript 代码与本目录自研 `BYPASS.SH`；脚本不含任何第三方 GPL/LGPL 代码或许可文本。
- 脚本只操作**仿真盘副本**，**绝不挂载、读取或写入原始 E01 证据原件**；原件全程只读。

## 3. payload CD 内容契约

payload CD（用户挂载后由产品生成）根目录包含三件：

| 文件 | 内容 |
| --- | --- |
| `SHADOW.NEW` | 编辑后的完整 `/etc/shadow` 内容（纯文本，整文件覆盖） |
| `BYPASS.JSON` | 元数据，字段：`partitionIndex`、`filesystem`、`username`、`blocker`、`shadowSha256`、`sizeBytes`、`generatedAt` |
| `BYPASS.SH` | 本脚本副本，供救援环境直接执行 |

`BYPASS.JSON` 字段说明：

- `partitionIndex`：目标分区序号；
- `filesystem`：`xfs` / `ext4`；
- `username`：被改账号；
- `blocker`：触发本次手动路径的阻塞类型；
- `shadowSha256`：`SHADOW.NEW` 的 SHA-256（脚本据此校验完整性）；
- `sizeBytes`：`SHADOW.NEW` 字节数；
- `generatedAt`：生成时间（ISO8601）。

## 4. 完整手工操作步骤

1. **启动救援环境**：用自备 recovery ISO 引导，挂载 payload CD（示例挂载点 `/mnt/meow-payload`，脚本也会自动探测 `/media/cdrom`、`/run/media/*/*` 等候选）。
2. **dry-run 校验**（只校验、不写盘）：

   ```sh
   sh BYPASS.SH --disk /dev/vg/root --payload /mnt/meow-payload --dry-run
   ```

   脚本依次校验：`SHADOW.NEW` / `BYPASS.JSON` 存在且非空；`shadowSha256`、`sizeBytes` 与实际文件一致；目标设备存在且为块设备；`blkid` 报告文件系统为 `xfs` 或 `ext4`（其它一律拒绝）。
3. **真实执行**：

   ```sh
   sh BYPASS.SH --disk /dev/vg/root --payload /mnt/meow-payload
   ```

   流程：只读挂载确认 `/etc/shadow` 存在 → 原 shadow 备份到 `/tmp/meow-shadow.orig.<时间戳>` → 读写挂载 → `install -m 000 -o root -g shadow`（无 `shadow` 组时退回 `-g 0` 并告警；无 `install` 时用 `cp` + `chmod 000` + `chown`）→ `sync` → 卸载 → 重新只读挂载并比对大小与首 32 字节，打印 `OK`。
4. **只读复核**（可选）：

   ```sh
   sh BYPASS.SH --disk /dev/vg/root --verify-only
   ```

   不写盘，仅比对盘上 shadow 与 `SHADOW.NEW`。
5. **启动仿真 VM 登录**：关闭救援环境，从该盘副本引导主 VM，用被改账号与临时密码 `123456` 登录；原 shadow 备份在救援环境 `/tmp`（tmpfs，重启丢失，需要先拷出）。

## 5. 边界与未验证项

- `BYPASS.SH` **未在真实救援环境中自动化验证**；不同发行版的 `blkid` 输出、`install`/`getent` 可用性、组名差异可能需要手工微调。
- 脚本仅接受 `xfs` / `ext4`；其它文件系统直接拒绝，不尝试挂载。
- 自备 recovery ISO 的获取、许可与合规由用户负责；本仓库不分发任何 OS 二进制。
- 自动救援 VM、串口协议回报、appliance 编排均不在本目录范围内。
