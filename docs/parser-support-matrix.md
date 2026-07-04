# Parser 支持矩阵

## 1. 说明

本文档描述“当前实现支持到什么程度”，不是路线图，也不是 PRD 愿景。

V2 长期执行与发布口径见：

- `docs/v2-longterm-plan.md`
- `docs/validation-trust-framework.md`
- `docs/expected-json-contract.md`

支持等级定义：

- `GA`：公开 small fixture + 至少一类回归样本已验证
- `Beta`：公开 fixture 已验证，但真实样本覆盖仍不足
- `Supported`：平台已确认支持，medium fixture 规划中（当前无 committed fixture）
- `Experimental`：实现存在，但样本、边界或自动化仍不足
- `Unsupported`：当前不承诺

## 2. 核心矩阵

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|---|
| E01 reader | Windows | Beta | `tiny.E01` | expected.json / 8 个测试 | open / read / seek / EOF / chunk 解压 | **planned** (2026-Q3) — `testdata/fixtures/public-medium/e01/` multi-segment complex variants | public-medium 目录尚空，多段复杂变体待补 |
| RAW reader | Windows | GA | `tiny.raw` | expected.json / 1 个测试 | open / read / seek | **planned** (2026-Q3) — `testdata/fixtures/public-medium/` partitioned RAW images | 基础链路稳定 |
| NTFS parser | Windows | Beta | `tiny.raw` (NTFS volume) | expected.json / 11 个测试 | 枚举、读取、部分 deleted / hidden / system | **planned** (2026-Q3) — `testdata/fixtures/public-medium/ntfs/` $MFT extracts, INDX buffers | 复杂损坏样本不足。public-medium/ntfs 尚空 |
| FAT parser | Windows | Experimental | 无 committed fixture | 5 个单元测试 | 基本枚举 | **planned** (2026-Q3) — `testdata/fixtures/public-medium/` FAT volume samples | deleted 不承诺。expected.json 待建 |
| exFAT parser | Windows | Experimental | 无 committed fixture | 37 个单元测试 | 基本枚举（boot/FAT/dir） | **planned** (2026-Q3) — `testdata/fixtures/public-medium/` exFAT volume samples | deleted 不承诺。expected.json 待建 |
| EVTX | Windows | Beta | `system.evtx` | 10 个测试 | 基本事件抽取（boot/shutdown） | **planned** (2026-Q3) — `testdata/fixtures/public-medium/evtx/` larger multi-channel samples | 大样本待补。expected.json 接入校验待加强 |
| Prefetch | Windows | Beta | 无 committed fixture | expected.json / 1 个 synthetic 测试 | executable、run_count 等核心字段 | **planned** (2026-Q3) — `testdata/fixtures/public-medium/prefetch/` historical + compressed .pf variants | testdata/artifacts/windows/prefetch/ 仅含 .gitkeep。历史版本与压缩变体覆盖不足 |
| LNK | Windows | Beta | 无 committed fixture | expected.json（无自动化测试） | target path、时间（expected.json 契约内） | **planned** (2026-Q3) — `testdata/fixtures/public-medium/lnk/` complex shell item samples | testdata/artifacts/windows/lnk/ 仅含 .gitkeep。未发现 #[test]。复杂 shell item 待补 |
| Registry | Windows | Beta | `tiny SYSTEM`、`tiny SOFTWARE`、`NTUSER` (liuyang_pc.E01)、`SAM` (liuyang_pc.E01)、`USRCLASS`、`Amcache.hve`、`SECURITY` (controlled) | expected.json / 48 个测试 | system info (computer_name, timezone, **services/drivers**, **USBSTOR/USB device history**, **MountedDevices**, **shutdown time**, **ShimCache**, network_adapters), software info (product_name, build, version, registered_owner, install_date), **machine persistence** (HKLM Run/RunOnce/RunOnceEx, Winlogon Shell/Userinit/Notify/AutoAdminLogon, LSA Authentication/Notification/Security Packages), NTUSER (user profiles, shell folders, recent files, typed paths, **OpenSavePidlMRU**, **LastVisitedPidlMRU**, **RunMRU**, mount points, UserAssist), USRCLASS (**ShellBags**, **MuiCache**), NetworkList (Profiles/Signatures first/last connect), AppCompatFlags Layers, Amcache.hve (InventoryApplication, InventoryApplicationFile), SAM (local users, group membership, login counts, password policy), SECURITY (local security policy, LSA Secrets metadata, cached domain credentials — encrypted blobs only) | **planned** (2026-Q3) — `testdata/fixtures/public-medium/registry/` full hive suite (SAM, SECURITY, NTUSER) | liuyang_pc.E01 回归已验证 SYSTEM/SOFTWARE/NTUSER/SAM 提取。R1–R4 已落地（服务/驱动、USB、MountedDevices、ShutdownTime、ShimCache、HKLM Run/Winlogon/LSA Packages、NTUSER MRU、USRCLASS ShellBags/MuiCache、NetworkList、Amcache、AppCompatFlags Layers、SECURITY 基础字段合成测试通过）。txlog dirty page 合并与键值恢复已完成。UserAssist ROT13 解码已实现。**SECURITY hive 支持以受控披露方式加入：仅输出策略字段与加密 blob 元数据，默认不解密 LSA Secrets / 缓存凭证。** 生产路径已切换为 `analysis_service::extraction::registry::extract_registry_candidate`；旧 `registry::parser::RegistryExtractor` 降级为 fallback/legacy。 |
| Recycle Bin | Windows | Beta | 无 committed fixture | expected.json（无自动化测试） | 原路径、删除时间（expected.json 契约内） | **planned** (2026-Q3) — `testdata/fixtures/public-medium/recycle-bin/` $I/$R paired samples | testdata/artifacts/windows/recycle-bin/ 仅含 .gitkeep。未发现 #[test]。损坏恢复不承诺 |

## 3. 数据源分析补充链路 — Browser Medium Fixtures: `testdata/fixtures/public-medium/browser/`

| 链路 | 平台 | 当前等级 | 已验证样本 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|
| JumpList | Windows | Experimental | 无 committed fixture | 基本提取 | **planned** (2026-Q3) — `testdata/fixtures/public-medium/` JumpList samples | 2 个单元测试。expected.json 待建 |
| SRU | Windows | Experimental | 无 committed fixture | 基本提取 | **planned** (2026-Q3) — `testdata/fixtures/public-medium/` SRU samples | 4 个单元测试。expected.json 待建 |
| Thumbcache | Windows | Experimental | 无 committed fixture | 基本提取 | **planned** (2026-Q3) — `testdata/fixtures/public-medium/` Thumbcache samples | 3 个单元测试。expected.json 待建 |
| Chrome History | 跨平台 | Supported | 无 | URL / 标题 / 访问时间 / 下载 | **planned** (2026-Q3) — controlled VM snapshot, 500+ history entries, 50+ downloads, 100+ cookies, 2 profiles | medium fixture 规划中 (Chrome SQLite) |
| Edge History | 跨平台 | Supported | 无 | URL / 标题 / 访问时间 / 下载 | **planned** (2026-Q3) — controlled VM snapshot, 300+ history, 20+ downloads, 80+ cookies | medium fixture 规划中 (Edge Chromium SQLite) |
| Firefox History | 跨平台 | Supported | 无 | URL / 标题 / 访问时间 / 下载 | **planned** (2026-Q3) — controlled VM snapshot, 300+ URLs in places.sqlite, 500+ visits, 30+ downloads.json, 80+ cookies | medium fixture 规划中 (Firefox SQLite) |
| Email extraction | 跨平台 | Supported | `testdata/fixtures/public-small/email/` `testdata/fixtures/public-medium/email/` | 发件人 / 收件人 / Cc/Bcc / 主题 / 正文(plain/HTML) / 附件 / Message-ID / References / Received / Container path / Folder path / Message class | **completed** — `public-small` 覆盖 EML/EMLX/MBOX/PST/OST；`public-medium` 覆盖 13 EML、55-message mbox、10-message PST/OST | EML/EMLX/MBOX/PST/OST 已接入。加密 PST/OST 延后至 V4 |

## 4. Linux 制品解析器 (V3 计划) — Medium Fixtures: `testdata/fixtures/public-medium/linux/`

当前 Stage 0 另有一个私有真实样本 baseline：检材3（通过 `FORENSICS_LINUX_E01_FIXTURE` opt-in）。它验证的是单盘 Linux 服务器链路：E01/RAW -> partition table -> LVM direct LV -> XFS -> 文件树预览 -> Linux artifact extraction。该 baseline 不等同于公开 fixture，也不升级下表的公开支持等级；公开升级仍要求 committed fixture 与 expected JSON。

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|---|
| systemd journal | Linux | Experimental | 检材3 opt-in 私有回归（非默认 CI） | `linux_e01_integration` ignored tests / `docs/pve-cluster-parsing-design.md` Stage 0 | 时间戳、消息、PID、UID、GID、可执行文件（bestEffort；压缩 journal 仍需公开 fixture） | **planned** (2026-Q3) — VM snapshot, 1000+ entries, multi-boot, LZ4 compression | public-small fixture 规划中 (synthetic journal)。压缩变体 LZ4/XZ/ZSTD 仍不宣称完整覆盖 |
| wtmp/utmp | Linux | Experimental | 检材3 opt-in 私有回归（`/var/log/wtmp` 可预览/提取） | `linux_e01_integration` ignored tests | 用户、终端、主机、登录/登出时间、PID（bestEffort） | **planned** (2026-Q3) — wtmp 100+ records (3+ users), btmp 10+ failed | public-small fixture 规划中。btmp 错误登录记录需公开 expected JSON 后再升级 |
| bash history | Linux | Experimental | 检材3 opt-in 私有回归（`/root/.bash_history` 可预览/提取） | `linux_e01_integration` ignored tests | 命令行、可选 epoch 时间戳（bestEffort；HISTTIMEFORMAT 变体未全覆盖） | **planned** (2026-Q3) — 500+ commands, 3 users, HISTTIMEFORMAT timestamps | public-small fixture 规划中。支持 `.bash_history` 与 `/root/.bash_history` |
| apt/dpkg history | Linux | Experimental | synthetic 单元/集成测试；检材3候选发现按日志存在性决定 | `artifacts-linux` tests / `linux_macos_artifact_extraction` | 包名、版本、操作 (install/upgrade/remove/configure)、时间戳（bestEffort） | **planned** (2026-Q3) — dpkg.log 200+ events, 5+ apt transactions, rotated log | public-small fixture 规划中。apt history.log + dpkg.log；rotated/compressed logs 待补 |
| cron | Linux | Experimental | 检材3 opt-in 私有回归（`/etc/crontab`、`/var/spool/cron/root` 候选） | `linux_e01_integration` ignored tests | 调度表达式、用户、命令（bestEffort） | **planned** (2026-Q3) — 20+ job definitions, crontab + cron.d + cron.* directories | public-small fixture 规划中。覆盖 crontab、cron.d、cron.{hourly,daily,weekly,monthly} |
| sudo logs | Linux | Experimental | 检材3 opt-in 私有回归（auth/secure/messages 候选按发行版日志存在性决定） | `linux_e01_integration` ignored tests | 用户、命令、时间戳、终端、成功/失败（bestEffort） | **planned** (2026-Q3) — auth.log 50+ sudo sessions, success+failure+session pairs | public-small fixture 规划中。Ubuntu `/var/log/auth.log` 与 RHEL/CentOS `/var/log/secure` 需分别建 baseline |
| Linux SSH text/config discovery | Linux | Experimental | 检材3 opt-in 私有回归按路径存在性发现 | `linux_e01_integration` ignored tests / LinuxArtifacts candidate discovery | sourcePath 与文本预览（bestEffort）；`authorized_keys`、`known_hosts`、`ssh_config`、`sshd_config`、config.d 文件仅作为文本候选 | **planned** (2026-Q3) — SSH auth log + config fixture | 当前不提供独立结构化 SSH session/config parser；SSH 登录仍依赖 journal/auth log/wtmp/sudo 等已建模来源 |
| Linux sudoers policy | Linux | Unsupported | 无 committed fixture | 无 | 不承诺 policy AST、include 解析、effective rule 计算 | **planned** (post Stage 0) | 当前仅对 sudo/auth log 做事件解析；`/etc/sudoers`、`/etc/sudoers.d/*` 可作为文件树/文本预览对象 |
| Linux profile.d shell semantics | Linux | Unsupported | 无 committed fixture | 无 | 不承诺 shell 语义解释、环境变量生效顺序或脚本执行图 | **planned** (post Stage 0) | `/etc/profile`、`/etc/profile.d/*` 可被枚举和预览，但不生成结构化 artifact |

### 4a. Linux Stage 0 单盘镜像 baseline（检材3）

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | 备注 |
|---|---|---|---|---|---|---|
| E01/RAW -> LVM direct LV -> XFS file tree | Linux | Beta for private baseline / Experimental for public release | 检材3 opt-in 私有真实样本 | `FORENSICS_LINUX_E01_FIXTURE` + `cargo test -p app-services --test linux_e01_integration -- --ignored` | 分区探测、LVM direct LV 展开、XFS root LV 枚举、`FileEntryId` 预览高价值路径、Linux artifact candidate/extraction coverage | 私有 baseline 要求 LVM pool 以 `Expanded`/`redirected` 保留但不作为可见 root，root LV 可见并支持预览。PVE cluster 暂缓，仅保留非执行设计边界。不承诺 LVM thin/cache/RAID/snapshot/VDO/writecache、partial VG、partial/degraded VG、deleted recovery。公开等级仍需可提交 fixture/expected JSON |

## 5. macOS 制品解析器 (V3 计划) — Medium Fixtures: `testdata/fixtures/public-medium/macos/`

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|---|
| plist | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 键值对、类型信息 | **planned** (2026-Q3) — 12+ forensic plists (10 binary + 2 XML), bplist00 magic, nested dicts | public-small fixture 规划中 (synthetic binary + XML plist)。覆盖 5+ 关键取证 plist |
| unified log | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 时间戳、进程、消息、活动 ID、线程 ID | **planned** (2026-Q3) — tracev3, 1000+ entries, 2+ boot UUIDs, 5+ subsystems | public-small fixture 规划中。tracev3 格式，需处理日志碎片与轮转 |
| Spotlight | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 文件路径、显示名称、种类、内容类型、日期、作者 | **planned** (2026-Q3) — store.db, 500+ indexed files, 30+ content types, vol+user indexes | public-small fixture 规划中。解析 .store.db 与 .Spotlight-V100/ |
| Quarantine | macOS | Experimental | 规划中 | V3 新增，无现有测试 | URL、来源包标识、隔离代理、时间戳 | **planned** (2026-Q3) — QuarantineEventsV2, 50+ events, 3+ source apps, Gatekeeper entries | public-small fixture 规划中。QuarantineEventsV2 SQLite |
| Launch Services | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 服务定义、包标识、路径 | **planned** (2026-Q3) — 15+ registered apps, 10+ UTI mappings, secure LS + launchd overrides | public-small fixture 规划中。com.apple.LaunchServices.plist + /var/db/launchd.db/ |
| FSEvents | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 文件路径、事件类型、时间戳 | **planned** (2026-Q3) — 1000+ events, 4+ event types, 2+ log pages, UUID file | public-small fixture 规划中。解析 .fseventsd/ 事件日志 |

## 6. 容器邮件解析器 (V3 计划)

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|---|
| PST (Unicode 32/64) | 跨平台 | Supported | `testdata/fixtures/public-small/email/synthetic.pst` `testdata/fixtures/public-medium/email/medium-pst/medium.pst` | V3 接入；支持 NBT/BBT 遍历、MAPI 属性上下文、邮件/附件/文件夹路径 | 主题、正文、发件人、收件人、时间、附件、文件夹路径 | **completed** — `public-small` synthetic + `public-medium` 10-message synthetic；sanitized real-world PSTs 仍 planned (2026-Q4) | public-small synthetic fixture 已覆盖。加密 PST 延后至 V4 |
| OST | 跨平台 | Supported | `testdata/fixtures/public-small/email/synthetic.ost` `testdata/fixtures/public-medium/email/medium-pst/medium.ost` | V3 接入；复用 PST 解析代码，按扩展名检测文件类型 | 主题、正文、发件人、收件人、时间、附件、文件夹路径 | **completed** — `public-small` synthetic + `public-medium` 10-message synthetic；offline folder samples 仍 planned (2026-Q4) | public-small synthetic fixture 已覆盖。复用 PST 代码。离线文件夹表与同步元数据 |
| mbox | 跨平台 | Supported | `testdata/fixtures/public-small/email/` `testdata/fixtures/public-medium/email/medium-mbox/` | V3 提前接入；支持 RFC 4155 四种变体拆分、附件元数据、容器路径 | 主题、正文(plain/HTML)、发件人、收件人、时间、附件、容器路径 | **completed** — `public-small` 3 samples + `public-medium` 55-message Thunderbird-style mbox；real-world Takeout 样本仍 planned (2026-Q3) | public-small fixture 已覆盖 simple/multipart/mboxrd_escaped。RFC 4155 变体检测 (mboxrd/mboxo/mboxcl/mboxcl2) |

## 7. 字段承诺规则

- 核心字段：至少在 small fixture 中有自动化断言
- 真实字段：至少在一类真实样本回归中有对照基准
- 非稳定字段：只能标记为 `bestEffort`
- 当前无法稳定给出结果的字段不得写成“已支持”

## 8. V2 目标状态

| 链路 | 平台 | 当前等级 | V2 目标 | 说明 |
|---|---|---|---|---|
| E01 reader | Windows | Beta | Beta / 接近 GA | 前提是 public-medium fixture、真实样本回归与多段边界说明补齐 |
| RAW reader | Windows | GA | GA | 维持现状并补 benchmark 与发布说明 |
| NTFS parser | Windows | Beta | Beta / 接近 GA | 重点是复杂损坏、大样本与真实回归说明。补齐 public-medium/ntfs |
| FAT parser | Windows | Experimental | Experimental / Beta | 以基本枚举稳定性和边界说明为主，不承诺 deleted recovery。需建 expected.json |
| exFAT parser | Windows | Experimental | Experimental / Beta | 以基本枚举稳定性和边界说明为主，不承诺 deleted recovery。需建 expected.json |
| EVTX | Windows | Beta | Beta | 补真实样本与支持边界说明，不夸大为全覆盖。加固 expected.json 接入校验 |
| Prefetch | Windows | Beta | Beta / 接近 GA | 需补 committed fixture 文件、medium fixture、压缩变体边界、自动化测试 |
| LNK | Windows | Beta | Beta / 接近 GA | 需补 committed fixture 文件、自动化测试、复杂 shell item 边界说明 |
| Registry | Windows | Beta | Beta / 接近 GA | NTUSER/SAM/txlog 提取已验证 (liuyang_pc.E01)。字段承诺已扩展至用户 profiles、shell folders、recent files、typed paths、run MRU、mount points、UserAssist、local users、group membership、login counts。R1–R4 能力已落地，包括 USRCLASS ShellBags/MuiCache、NetworkList、Amcache、AppCompatFlags Layers、SECURITY 基础字段（受控披露）。txlog dirty page 合并与键值恢复已实现。不宣称完整 hive browser；SECURITY 解密默认关闭 |
| Recycle Bin | Windows | Beta | Beta / 接近 GA | 需补 committed fixture 文件、自动化测试、损坏恢复边界说明 |
| JumpList | Windows | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| SRU | Windows | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| Thumbcache | Windows | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| Browser History | 跨平台 | Supported | Supported / GA | medium fixture 规划中 (Chrome/Edge/Firefox SQLite)。需新建 artifacts-windows 浏览器模块、fixture、expected.json |
| Email extraction | 跨平台 | Supported | Supported / GA | public-small fixture 已覆盖 EML/EMLX/MBOX/PST/OST；medium fixture 规划中。加密 PST/OST 延后 |

## 9. V3 目标状态 (Linux / macOS / 容器邮件)

| 链路 | 平台 | 当前等级 | V3 目标 | 说明 |
|---|---|---|---|---|
| systemd journal | Linux | Experimental | Supported | public-small + public-medium fixture (synthetic + VM 快照)。压缩变体 LZ4/XZ/ZSTD 全覆盖 |
| wtmp/utmp | Linux | Experimental | Supported | public-small + public-medium fixture。btmp 同步覆盖。登录会话时间线完整 |
| bash history | Linux | Experimental | Supported | public-small + public-medium fixture。HISTTIMEFORMAT 时间戳解析 |
| apt/dpkg history | Linux | Experimental | Supported | public-small + public-medium fixture。安装/升级/移除操作全覆盖 |
| cron | Linux | Experimental | Supported | public-small + public-medium fixture。多源 crontab 覆盖 (crontab/cron.d/cron.{hourly,daily,weekly,monthly}) |
| sudo logs | Linux | Experimental | Supported | public-small + public-medium fixture。auth.log sudo 会话开/关 |
| plist | macOS | Experimental | Supported | public-small + public-medium fixture。binary + XML plist。5+ 关键取证 plist |
| unified log | macOS | Experimental | Supported | public-small + public-medium fixture。tracev3 格式。日志碎片与轮转处理 |
| Spotlight | macOS | Experimental | Supported | public-small + public-medium fixture。.store.db + .Spotlight-V100/ |
| Quarantine | macOS | Experimental | Supported | public-small + public-medium fixture。QuarantineEventsV2 SQLite 全字段 |
| Launch Services | macOS | Experimental | Supported | public-small + public-medium fixture |
| FSEvents | macOS | Experimental | Supported | public-small + public-medium fixture。.fseventsd/ 事件日志 |
| PST | 跨平台 | Supported | Supported / GA | public-small + public-medium + private-real fixture。Unicode 32/64-bit。消息 + 附件 + 文件夹 + 日历 + 联系人。加密 PST 延后至 V4 |
| OST | 跨平台 | Supported | Supported | public-small fixture 已覆盖。离线文件夹表与同步元数据 |
| mbox | 跨平台 | Supported | Supported / GA | public-small fixture 已覆盖；public-medium 规划中。RFC 4155 变体全覆盖 (mboxrd/mboxo/mboxcl/mboxcl2) |

## 10. 与文档同步要求

以下变化必须同步更新本文档：

- parser 新增支持格式
- 真实样本回归完成或失败
- 字段承诺升级或降级
- 已知不支持项变化
- Linux/macOS 解析器首次实现后从 Experimental 升级

同步目标文档：

- `docs/validation-trust-framework.md`
- `docs/known-unsupported-formats.md`
- `docs/release-scorecard.md`
- `docs/v3-plan.md`
- `docs/linux-artifact-coverage.md`
- `docs/mac-artifact-coverage.md`
- `docs/pst-ost-mbox-support.md`
