# 已知不支持格式与边界

## 1. 目的

明确告诉开发、测试和使用者：

- 当前哪些格式或场景不支持
- 哪些只属于部分支持
- 哪些字段不承诺

V2 长期计划与能力评级请同时参考：

- `docs/v2-longterm-plan.md`
- `docs/parser-support-matrix.md`

## 2. 当前已知不支持或不承诺项

| 分类 | 项目 | 状态 | 说明 |
|---|---|---|---|
| E01 | 多段复杂样本 | 部分支持 | 当前公开样本仅 tiny.E01 单段。public-medium/e01 尚空 |
| NTFS | 全量损坏恢复 | 不承诺 | 极端损坏、复杂修复场景仍缺样本与回归 |
| FAT / exFAT | 已删除文件恢复 | 不承诺 | 本轮 deleted 重点仅覆盖 NTFS MFT |
| FAT / exFAT | committed fixture | 缺失 | 无 fixture 文件。expected.json 待建 |
| Registry | transaction log 完整重放 | 不承诺 | 当前以 hive 直接解析为主。txlog 集成仍为未完成项 |
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
| Browser | 全部 | 未实现 | artifacts-windows 中无浏览器模块。Chrome/Edge/Firefox 均无代码、无 fixture、无 expected.json |
| Email | MSG (Outlook .msg) | 不支持 | OLE2 复合文档，超出当前范围，规划于 V4 或后续评估 |
| Email | TNEF / winmail.dat | 不支持 | MS-OXTNEF 格式，规划于 V4 或后续评估 |
| Email | 加密或密码保护 PST/OST | 不支持 | 不尝试破解密码；检测并记录 warning |
| Linux Stage 0 | 检材3 baseline 作为公开支持证明 | 不承诺 | 检材3是私有 opt-in 真实样本 baseline，用于守住单盘 Linux 链路；不能替代 public-small/public-medium fixture 与 expected JSON |
| Linux 文件系统 | ext4 raw disk 完整支持 | 不承诺 | reader 能力与探测链路需以公开 fixture / expected JSON 补齐；检材3 baseline 不覆盖 ext4 |
| Linux 文件系统 | XFS raw disk 完整支持 | 部分支持 | Stage 0 baseline 仅覆盖单源单盘、LVM direct LV 上的 XFS root tree、预览与 Linux artifact extraction |
| Linux 文件系统 | Btrfs raw disk 完整支持 | 不承诺 | reader 能力与探测链路需以公开 fixture / expected JSON 补齐；检材3 baseline 不覆盖 Btrfs |
| Linux 文件系统 | 已删除文件恢复 (ext4/XFS/Btrfs) | 不承诺 | 文件雕刻与已删除恢复规划于 V4 |
| Linux LVM/PVE | PVE cluster 执行 | 不支持 | Stage 0 只支持单源、单盘 Linux 服务器链路；cluster service 是非执行设计边界 |
| Linux LVM/PVE | LVM thin/cache/RAID/snapshot/VDO/writecache | 不支持 | 当前 baseline 只覆盖 direct linear/striped LV；复杂映射需独立 metadata 解析与 fixture |
| Linux LVM/PVE | partial/degraded VG 激活 | 不支持 | 缺失 PV 或不一致 metadata 必须 fail closed，不猜测块映射 |
| macOS 文件系统 | APFS raw disk | 不支持 | 当前仅支持从已挂载或导入的 macOS 文件树提取制品。APFS 原始磁盘镜像解析规划于 V4 |
| macOS 文件系统 | HFS+ raw disk | 不支持 | HFS+ 原始磁盘镜像解析规划于 V4 |
| macOS 文件系统 | 已删除文件恢复 (APFS/HFS+) | 不承诺 | 文件雕刻与已删除恢复规划于 V4 |
| 移动设备 | iOS 制品 | 不支持 | iOS 备份/镜像解析（Contacts、Messages、Photos、Safari 等）规划于 V4 |
| 移动设备 | Android 制品 | 不支持 | Android 备份/镜像解析（SMS/MMS、联系人、应用数据、Chrome 历史）规划于 V4 |
| 云 | AWS CloudTrail | 不支持 | 云审计日志采集与关联规划于 V4 |
| 云 | Azure Audit Logs | 不支持 | 云审计日志采集与关联规划于 V4 |
| 云 | GCP Audit Logs | 不支持 | 云审计日志采集与关联规划于 V4 |
| 云 | Google Workspace Logs | 不支持 | 云工作空间日志规划于 V4 |
| 云 | Microsoft 365 Unified Audit Log | 不支持 | 云审计日志规划于 V4 |
| 网络 | PCAP/网络捕获 | 不支持 | 网络数据包捕获摄入与流记录解析规划于 V4 |
| 内存 | 内存镜像采集与分析 | 不支持 | 实时响应采集与内存镜像集成规划于 V4 |

## 3. V2 期间仍不得被市场化夸大的边界

以下能力即便在 V2 期间有实现增量，也不得在 README、PRD、用户文案中写成“完整支持”：

- 多段复杂 E01 全覆盖
- NTFS 极端损坏恢复
- FAT / exFAT deleted recovery
- Registry transaction log 完整重放
- Prefetch 全版本压缩变体
- LNK 全量复杂 shell item
- JumpList / SRU / Thumbcache 全格式覆盖（当前无 fixture，无 expected.json）
- Browser 全浏览器全版本兼容（当前无任何实现代码）
- Email MSG (Outlook .msg) / TNEF (winmail.dat) 格式
- Email 加密或密码保护 PST/OST
- Email 全 MAPI 属性级精度与超大 PST 流式解析

## 3a. V3 期间仍不得被市场化夸大的边界

以下能力即便在 V3 期间有实现增量，也不得在 README、PRD、用户文案中写成"完整支持"：

- Linux 文件系统 (ext4/XFS/Btrfs) 原始磁盘镜像“完整支持”；检材3只证明单盘 LVM direct LV -> XFS 的 Stage 0 baseline
- Linux 检材3私有 baseline 作为公开 GA 证明（必须补 public fixture + expected JSON 后才能升级公开承诺）
- PVE cluster 执行、多源 E01 聚合、跨节点关联
- LVM thin/cache/RAID/snapshot/VDO/writecache、partial/degraded VG 激活
- macOS 文件系统 (APFS/HFS+) 原始磁盘镜像解析（V3 仅支持从文件树提取制品）
- Linux/macOS 文件系统已删除文件恢复
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
- `docs/release-scorecard.md`
- `docs/v3-plan.md`
- `docs/linux-artifact-coverage.md`
- `docs/mac-artifact-coverage.md`
