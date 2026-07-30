# 已知不支持格式与边界

## 1. 目的

明确告诉开发、测试和使用者：

- 当前哪些格式或场景不支持
- 哪些只属于部分支持
- 哪些字段不承诺

V2 长期计划与能力评级请同时参考：

- `docs/documentation-index.md`
- `docs/parser-support-matrix.md`

## 2. 当前已知不支持或不承诺项

| 分类 | 项目 | 状态 | 说明 |
|---|---|---|---|
| E01 | 多段复杂样本 | 部分支持 | 当前公开样本仅 tiny.E01 单段。public-medium/e01 尚空 |
| NTFS | 全量损坏恢复 | 不承诺 | 极端损坏、复杂修复场景仍缺样本与回归 |
| FAT / exFAT | 已删除文件恢复 | 不承诺 | 本轮 deleted 重点仅覆盖 NTFS MFT |
| FAT / exFAT | committed fixture | 缺失 | 无 fixture 文件。expected.json 待建 |
| BitLocker | TPM / TPM+PIN / 启动密钥 / clear-key 解锁 | 不支持 | 当前仅清点 protector inventory；不会自动使用 clear key，也不处理 TPM-sealed 或 `.BEK` 密钥 |
| BitLocker | AES-256-CBC + Elephant Diffuser (`0x8001`) | 仅识别 | 缺少可信 oracle，拒绝产生可能看似合理但错误的明文 |
| BitLocker | 已验证密钥包持久化与调查员 UI | 已实现（公开支持等级仍为 Experimental） | Stage 4-6 已完成真实密钥包持久化、restore/forget、文件浏览器解锁面板、非秘密报告 inventory 与恢复性能回归；内存恢复从首 1 MiB CR3 bootstrap 经受审计的 PE/CodeView profile 定位 `fvevol.sys`，只读取 data roots 和有界虚拟指针图，以 FIPS-197 AES schedule 预筛，并要求目标卷 NTFS boot/MFT、`$UpCase`、`$Bitmap` oracle 全部通过。生产服务不调用全物理 pool-tag scanner，且物理读取有独立硬预算。当前只验证 Windows 11 build 26100 的一个 profile；未知 build typed unsupported。密钥、schedule 和物理地址不进入 DTO、日志、报告或 SQLite；因无公开镜像/内存 fixture，支持等级仍为 Experimental |
| Registry | transaction log 完整重放 | 部分支持 | `.LOG1/.LOG2` dirty-page bitmap、page recovery 与 hive 合并已实现并有测试；不承诺全部损坏组合、已删除 cell 恢复或任意历史版本的完整重放 |
| Registry | private-real 回归 E01 | 部分完成 | liuyang_pc.E01 已验证 SYSTEM/SOFTWARE/NTUSER/SAM 提取；E01 镜像本身未提交至仓库 |
| Registry | 已删除 cell 恢复 | 不承诺 | 当前不解析 hive bin 中未分配 cell。恢复已删除键值需要 cell 分配图与 txlog 交叉引用 |
| Registry | 完整 registry browser | 不承诺 | 当前为定向字段提取，不提供交互式 key path 枚举与全 hive 浏览 |
| Prefetch | committed fixture 文件 | 缺失 | testdata/artifacts/windows/prefetch/ 仅含 .gitkeep。public-medium/prefetch 尚空 |
| Prefetch | 自动化测试 | 不足 | 仅 1 个 synthetic 单元测试 |
| Prefetch | 全版本压缩变体 | 部分支持 | 需要继续补样本与 expected baseline |
| LNK | committed fixture 文件 | 缺失 | testdata/artifacts/windows/lnk/ 仅含 .gitkeep。无自动化测试 |
| LNK | 全量复杂 shell item | 部分支持 | 当前重点是 target path 与核心时间字段 |
| Recycle Bin | committed fixture 文件 | 缺失 | testdata/artifacts/windows/recycle-bin/ 仅含 .gitkeep。无自动化测试 |
| Recycle Bin | 全损坏恢复场景 | 不承诺 | 当前以标准结构提取为主 |
| JumpList | committed fixture | 缺失 | 有实现 (2 测试)，无 fixture，无 expected.json |
| SRU | committed fixture | 缺失 | 有实现 (4 测试)，无 fixture，无 expected.json |
| Thumbcache | committed fixture | 缺失 | 有实现 (3 测试)，无 fixture，无 expected.json |
| Browser | Chrome/Edge/Firefox 全版本完整兼容 | 部分支持 | Chromium/Firefox history、download、cookie、session 与 password metadata 解析链路已实现；缺公开完整 fixture/expected JSON、加密凭据解密和全部历史版本覆盖 |
| Email | MSG (Outlook .msg) | 不支持 | OLE2 复合文档，超出当前范围，规划于 V4 或后续评估 |
| Email | TNEF / winmail.dat | 不支持 | MS-OXTNEF 格式，规划于 V4 或后续评估 |
| Email | 加密或密码保护 PST/OST | 不支持 | 不尝试破解密码；检测并记录 warning |
| Linux Stage 0 | 检材3 baseline 作为公开支持证明 | 不承诺 | 检材3是私有 opt-in 真实样本 baseline，用于守住单盘 Linux 链路；不能替代 public-small/public-medium fixture 与 expected JSON |
| Linux 文件系统 | ext4 raw disk 完整支持 | 部分支持 | PVE 私有样本已验证 E01 -> LVM direct root LV -> 64-bit EXT4（64-byte group descriptor）-> 完整文件树、inode size 与 `FileEntryId` 预览；仍缺公开 fixture / expected JSON、metadata checksum 与全部 feature 组合覆盖 |
| Linux 文件系统 | XFS raw disk 完整支持 | 部分支持 | Stage 0 baseline 仅覆盖单源单盘、LVM direct LV 上的 XFS root tree、预览与 Linux artifact extraction |
| Linux 文件系统 | Btrfs raw disk 完整支持 | 不承诺 | reader 能力与探测链路需以公开 fixture / expected JSON 补齐；检材3 baseline 不覆盖 Btrfs |
| Linux 文件系统 | ext4 已删除文件恢复的全部组合 | Partial | 当前只支持 JBD2 证据驱动的 direct extent depth-0 候选；complete 候选可导出，partial 候选只能读取连续 verified range；间接 extent、目录恢复、复杂 feature 组合和全盘 carving 不承诺 |
| Linux 文件系统 | XFS 已删除文件内容恢复 | Partial | XFS log 可报告明确删除证据与 metadata-only 候选；未经过 allocation/SHA-256 完整验证的内容不会提供预览或导出 |
| Linux 文件系统 | Btrfs 已删除文件恢复与 carving | 不承诺 | 当前没有 Btrfs 删除恢复实现 |
| Linux LVM/PVE | PVE cluster 语义解析执行 | 部分支持 | 集群建模、成员串行导入、宿主 `pve/root` EXT4 文件树、BlueStore/RocksDB/semantic/OMAP、source-bound RADOS range reader 已验证。私有样本显式加载的三 OSD inventory 可重建 `vm-100-disk-0`，派生独立 source DB 含直接 XFS、`centos/home`、`centos/root` 和 114,260 条文件记录。代表文件覆盖 `1,019 B` 至 `614,794,240 B`、连续/随机/文件尾 bounded range；大文件整文件 materialize 与 request-local runtime 缺陷已修复。提交 `db49698a` 的三轮统一门禁以检材3原生 XFS 和 PVE 宿主 EXT4 为固定对照，验证 viewer/media 字节一致及 source/case invalidation 冷重建；RBD cold runtime 中位为 `3.186s`，RSS delta 为 `399-448 MiB`。浏览器端 media 时序与容量 LRU eviction 尚未建立私有样本门禁。inventory 完整性尚未独立证明，通用 PG/CRUSH/EC、degraded replica、multi-PV/跨 RBD LVM、clone/snapshot/encryption、CephFS 与跨节点关联仍不支持 |
| Linux LVM/PVE | 非集群导入跨镜像 multi-PV VG 聚合 | 不支持 | 普通导入与恢复导入仅允许当前数据源参与 LVM 展开，不扫描案件内其他 E01/RAW；缺失 PV 时保留明确诊断并 fail closed。显式多源组合 API 仅供未来经原子成员注册的集群编排使用 |
| Linux LVM/PVE | LVM thin/cache/RAID/snapshot/VDO/writecache | 部分支持 | direct linear/striped 与基础 dm-thin 只读映射已实现；thin metadata checksum/repair、cache、RAID、snapshot、VDO、writecache 仍不支持且必须 fail closed |
| Linux LVM/PVE | partial/degraded VG 激活 | 不支持 | 缺失 PV 或不一致 metadata 必须 fail closed，不猜测块映射 |
| Linux artifacts | systemd journal 压缩/轮转完整覆盖 | 部分支持 | 当前解析器支持 uncompressed 与部分 LZ4/ZSTD 字段，但缺 public fixture 覆盖 multi-boot、rotated、XZ 与损坏 journal；字段只能 bestEffort |
| Linux artifacts | SSH 结构化登录/配置语义解析 | 部分支持 | 当前通过 auth/journal/wtmp/sudo 与 `LinuxSystemConfig` 文本记录覆盖 SSH 相关线索；`authorized_keys`、`known_hosts`、`sshd_config` 不生成独立 SSH DTO，也不解析密钥信任图 |
| Linux artifacts | sudoers policy 语义解析 | 部分支持 | `/etc/sudoers` 与 `/etc/sudoers.d/*` 会生成 `LinuxSystemConfig` 文本记录；不解析 include、alias、Defaults、effective rule |
| Linux artifacts | systemd/init/profile.d shell 语义解析 | 部分支持 | systemd unit、init.d、rc.local、profile.d 会生成 `LinuxSystemConfig` 文本记录；不解释 shell 脚本、环境变量生效顺序、systemd 依赖图或执行图 |
| Linux artifacts | nginx/Apache 站点完整语义解析 | 部分支持 | 当前提取站点配置、access/error log 与 Web root script finding；不展开全部 include/module/继承语义，不把 IIS 混入 Linux section |
| Linux artifacts | MySQL/MariaDB 数据内容恢复 | 不支持 | 当前只提取配置、服务日志与风险 finding；不读取表空间、不恢复 InnoDB page、不解析业务表数据或账户有效权限 |
| 平台 | macOS 数据源分析与制品提取 | 不支持 | Windows 与 Linux 是仅有的生产分析平台；macOS 数据源请求和 MacArtifacts 能力返回 typed unsupported，不运行候选发现或提取器 |
| 文件系统 | APFS/HFS+ 分区内容解析 | 不支持 | 仅识别已知 Apple 分区类型标识符并记录为元数据；当前不识别 APFS/HFS+ 文件系统 magic/signature，不实例化文件系统 reader，也不提供文件树、预览、制品提取或已删除恢复 |
| 案件兼容 | 旧 macOS 案件 | 不支持 | 含 platform='macos' 的旧案件不做迁移；当前开发版本打开时返回 typed unsupported，需要新建案件，并仅将可归类为 Windows 或 Linux 的证据重新导入 |
| 移动设备 | iOS 制品 | 不支持 | iOS 备份/镜像解析（Contacts、Messages、Photos、Safari 等）已退役；artifacts-ios crate 与 transport DTO 均已移除，不保留预留契约面 |
| 移动设备 | Android 制品 | 不支持 | Android 备份/镜像解析（SMS/MMS、联系人、应用数据、Chrome 历史）无实现；artifacts-android crate 已退役，仅 transport/src/dto/android.rs 保留为零消费者预留契约面 |
| 云 | AWS CloudTrail | 不支持 | 云审计日志采集与关联已退役；cloud-audit crate 与 transport DTO 均已移除 |
| 云 | Azure Audit Logs | 不支持 | 云审计日志采集与关联已退役；cloud-audit crate 与 transport DTO 均已移除 |
| 云 | GCP Audit Logs | 不支持 | 云审计日志采集与关联已退役；cloud-audit crate 与 transport DTO 均已移除 |
| 云 | Google Workspace Logs | 不支持 | 云工作空间日志无实现，且不在当前磁盘取证范围内 |
| 云 | Microsoft 365 Unified Audit Log | 不支持 | 云审计日志已退役；cloud-audit crate 与 transport DTO 均已移除 |
| 网络 | PCAP/网络捕获 | 不支持 | 网络数据包捕获摄入与流记录解析从未实现，且不在当前磁盘取证范围内 |
| 内存 | 内存镜像采集与分析 | 不支持 | 不提供实时采集、进程/句柄/VAD/网络等通用内存取证；唯一例外是匹配 BitLocker 卷的有界密钥候选恢复，不能据此宣称完整内存分析能力 |

## 3. V2 期间仍不得被市场化夸大的边界

以下能力即便在 V2 期间有实现增量，也不得在 README、PRD、用户文案中写成“完整支持”：

- 多段复杂 E01 全覆盖
- NTFS 极端损坏恢复
- FAT / exFAT deleted recovery
- Registry transaction log 完整重放
- Prefetch 全版本压缩变体
- LNK 全量复杂 shell item
- JumpList / SRU / Thumbcache 全格式覆盖（当前无 fixture，无 expected.json）
- Browser 全浏览器全版本兼容（已有 Chromium/Firefox 主链路，但缺公开完整 fixture、expected JSON 与历史版本覆盖）
- Email MSG (Outlook .msg) / TNEF (winmail.dat) 格式
- Email 加密或密码保护 PST/OST
- Email 全 MAPI 属性级精度与超大 PST 流式解析

## 3a. V3 期间仍不得被市场化夸大的边界

以下能力即便在 V3 期间有实现增量，也不得在 README、PRD、用户文案中写成"完整支持"：

- Linux 文件系统 (ext4/XFS/Btrfs) 原始磁盘镜像“完整支持”；检材3只证明 LVM direct LV -> XFS，PVE 私有样本只证明 LVM direct LV -> 64-bit EXT4
- Linux 检材3私有 baseline 作为公开 GA 证明（必须补 public fixture + expected JSON 后才能升级公开承诺）
- PVE 通用 PG/CRUSH/acting-set、EC、degraded replica、multi-PV/跨 RBD LVM、clone/snapshot/encryption、CephFS 与跨节点关联。当前只对 `E:\pangushi\服务器` 私有样本显式加载的三 OSD inventory、RBD head、派生独立 source DB、114,260 条 VM 文件记录和代表性大文件 bounded-range 性能建立 baseline；已加载 inventory 是否等于完整副本集合尚未独立证明，不将其扩大为任意 Ceph 集群或公开 GA 承诺
- 普通数据源导入自动借用案件内其他镜像补齐 multi-PV VG；必须先有原子集群成员注册与一致性校验
- LVM thin 的全部变体与 cache/RAID/snapshot/VDO/writecache、partial/degraded VG 激活；当前仅实现受限的只读 dm-thin 映射
- systemd journal 压缩/轮转/损坏样本完整覆盖（当前为 bestEffort）
- SSH 结构化登录/配置 parser（当前只通过日志、wtmp、sudo 与 `LinuxSystemConfig` 文本记录侧面覆盖）
- sudoers policy effective rule 解析（当前仅保留文本记录）
- systemd/init/profile.d shell 语义解析（当前仅保留文本记录）
- macOS 数据源分析、制品提取、治理覆盖字段或前端入口；Windows/Linux 是仅有生产平台
- APFS/HFS+ 内容解析；已知 Apple 分区类型标识符识别仅是 metadata，不得被表述为 filesystem magic/signature 或 reader 支持
- 旧 `platform='macos'` 案件迁移；必须返回 typed unsupported，并仅对可归类为 Windows/Linux 的证据重新建案导入
- Btrfs 已删除文件恢复与全盘 carving；XFS 未验证内容恢复；APFS/HFS+ 当前连内容 reader 都不提供
- PST 加密消息支持
- PST 全 MAPI 属性级精度
- iOS/Android 移动设备制品
- 云服务商审计日志 (AWS CloudTrail / Azure Audit / GCP Audit)
- 网络数据包捕获 (PCAP) 摄入
- 内存镜像采集与分析
- 全浏览器全版本全平台兼容（V3 目标为 Chrome/Edge/Firefox 主流版本）
- Email MSG (Outlook .msg) / TNEF (winmail.dat) 格式
- Email 加密或密码保护 PST/OST
- Email 超大 PST/OST 流式解析（当前为完整文件加载）
- 规则包 DSL / 脚本逻辑（V3 规则条件为声明式字段断言）
- 多用户协作笔记本（V3 为单用户）
- 图形查询语言 DSL（V3 使用结构化 Rust API）

## 4. 前端与链路层限制

| 项目 | 状态 | 说明 |
|---|---|---|
| HTTP server 模式 | Unsupported | 项目架构明确不提供 |
| 旧 case 历史库自动回填全部新字段 | Unsupported | 当前验证口径以重新导入为准 |
| 页面内私有重复组件承接状态图标 | Unsupported | 文件状态必须走公有组件与统一链路 |

## 5. 使用要求

当某条能力属于“部分支持”或“不承诺”时：

- 不在 README、PRD、用户文档中写成“完整支持”
- 需要在测试计划或回归说明中标明验证边界
- 前端展示不得暗示结果一定完整可靠

## 6. 文档联动

以下文档与本文件同步维护：

- `docs/parser-support-matrix.md`
- `docs/validation-trust-framework.md`
- `docs/documentation-index.md`
- `docs/parser-support-matrix.md`
- `docs/linux-artifact-coverage.md`
