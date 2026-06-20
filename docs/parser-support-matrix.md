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
| Registry | Windows | Beta | `tiny SYSTEM`、`tiny SOFTWARE`、`NTUSER` (liuyang_pc.E01)、`SAM` (liuyang_pc.E01) | expected.json / 32 个测试 | system info (computer_name, timezone, services, network_adapters), software info (product_name, build, version, registered_owner, install_date), NTUSER (user profiles, shell folders, recent files, typed paths, run MRU, mount points), SAM (local users, group membership, login counts, password policy) | **planned** (2026-Q3) — `testdata/fixtures/public-medium/registry/` full hive suite (SAM, SECURITY, NTUSER) | liuyang_pc.E01 回归已验证 SYSTEM/SOFTWARE/NTUSER/SAM 提取。SECURITY hive 与 txlog 完整重放仍未 commit |
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
| Email extraction | 跨平台 | Supported | 无 | 发件人 / 收件人 / 主题 / 正文 / 附件 | **planned** (2026-Q3) — `testdata/fixtures/public-medium/` EML/EMLX samples | medium fixture 规划中 (EML/EMLX)。PST/OST/mbox 延后至 V3 |

## 4. Linux 制品解析器 (V3 计划) — Medium Fixtures: `testdata/fixtures/public-medium/linux/`

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|---|---|
| systemd journal | Linux | Experimental | 规划中 | V3 新增，无现有测试 | 时间戳、消息、PID、UID、GID、可执行文件 | **planned** (2026-Q3) — VM snapshot, 1000+ entries, multi-boot, LZ4 compression | public-small fixture 规划中 (synthetic journal)。压缩变体 LZ4/XZ/ZSTD |
| wtmp/utmp | Linux | Experimental | 规划中 | V3 新增，无现有测试 | 用户、终端、主机、登录/登出时间、PID | **planned** (2026-Q3) — wtmp 100+ records (3+ users), btmp 10+ failed | public-small fixture 规划中。btmp 错误登录记录同步支持 |
| bash history | Linux | Experimental | 规划中 | V3 新增，无现有测试 | 命令行、可选时间戳 (HISTTIMEFORMAT) | **planned** (2026-Q3) — 500+ commands, 3 users, HISTTIMEFORMAT timestamps | public-small fixture 规划中。支持 `.bash_history` 与 `/root/.bash_history` |
| apt/dpkg history | Linux | Experimental | 规划中 | V3 新增，无现有测试 | 包名、版本、操作 (install/upgrade/remove)、时间戳 | **planned** (2026-Q3) — dpkg.log 200+ events, 5+ apt transactions, rotated log | public-small fixture 规划中。apt history.log + dpkg.log |
| cron | Linux | Experimental | 规划中 | V3 新增，无现有测试 | 调度表达式、用户、命令 | **planned** (2026-Q3) — 20+ job definitions, crontab + cron.d + cron.* directories | public-small fixture 规划中。覆盖 crontab、cron.d、cron.{hourly,daily,weekly,monthly} |
| sudo logs | Linux | Experimental | 规划中 | V3 新增，无现有测试 | 用户、命令、时间戳、终端 | **planned** (2026-Q3) — auth.log 50+ sudo sessions, success+failure+session pairs | public-small fixture 规划中。解析 /var/log/auth.log sudo 条目 |

## 5. macOS 制品解析器 (V3 计划) — Medium Fixtures: `testdata/fixtures/public-medium/macos/`

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|---|---|
| plist | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 键值对、类型信息 | **planned** (2026-Q3) — 12+ forensic plists (10 binary + 2 XML), bplist00 magic, nested dicts | public-small fixture 规划中 (synthetic binary + XML plist)。覆盖 5+ 关键取证 plist |
| unified log | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 时间戳、进程、消息、活动 ID、线程 ID | **planned** (2026-Q3) — tracev3, 1000+ entries, 2+ boot UUIDs, 5+ subsystems | public-small fixture 规划中。tracev3 格式，需处理日志碎片与轮转 |
| Spotlight | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 文件路径、显示名称、种类、内容类型、日期、作者 | **planned** (2026-Q3) — store.db, 500+ indexed files, 30+ content types, vol+user indexes | public-small fixture 规划中。解析 .store.db 与 .Spotlight-V100/ |
| Quarantine | macOS | Experimental | 规划中 | V3 新增，无现有测试 | URL、来源包标识、隔离代理、时间戳 | **planned** (2026-Q3) — QuarantineEventsV2, 50+ events, 3+ source apps, Gatekeeper entries | public-small fixture 规划中。QuarantineEventsV2 SQLite |
| Launch Services | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 服务定义、包标识、路径 | **planned** (2026-Q3) — 15+ registered apps, 10+ UTI mappings, secure LS + launchd overrides | public-small fixture 规划中。com.apple.LaunchServices.plist + /var/db/launchd.db/ |
| FSEvents | macOS | Experimental | 规划中 | V3 新增，无现有测试 | 文件路径、事件类型、时间戳 | **planned** (2026-Q3) — 1000+ events, 4+ event types, 2+ log pages, UUID file | public-small fixture 规划中。解析 .fseventsd/ 事件日志 |

## 6. 容器邮件解析器 (V3 计划)

| 链路 | 平台 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | Medium Fixture | 备注 |
|---|---|---|---|---|---|---|---|
| PST (Unicode 32/64) | 跨平台 | Experimental | 规划中 | V3 新增，无现有测试 | 主题、正文、发件人、收件人、时间、附件、文件夹路径 | **planned** (2026-Q4) — `testdata/fixtures/public-medium/pst/` sanitized real-world PSTs | public-small + public-medium fixture 规划中。加密 PST 延后至 V4 |
| OST | 跨平台 | Experimental | 规划中 | V3 新增，无现有测试 | 主题、正文、发件人、收件人、时间、附件、文件夹路径 | **planned** (2026-Q4) — `testdata/fixtures/public-medium/ost/` offline folder samples | public-small fixture 规划中。复用 PST 代码。离线文件夹表与同步元数据 |
| mbox | 跨平台 | Experimental | 规划中 | V3 新增，无现有测试 | 主题、正文、发件人、收件人、时间、附件 | **planned** (2026-Q4) — `testdata/fixtures/public-medium/mbox/` RFC 4155 variant samples | public-small + public-medium fixture 规划中。RFC 4155 变体检测 (mboxrd/mboxo/mboxcl/mboxcl2) |

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
| Registry | Windows | Beta | Beta / 接近 GA | NTUSER/SAM 提取已验证 (liuyang_pc.E01)。字段承诺已扩展至用户 profiles、shell folders、recent files、typed paths、run MRU、mount points、local users、group membership、login counts。不宣称完整 hive browser 或 txlog 完整重放 |
| Recycle Bin | Windows | Beta | Beta / 接近 GA | 需补 committed fixture 文件、自动化测试、损坏恢复边界说明 |
| JumpList | Windows | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| SRU | Windows | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| Thumbcache | Windows | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| Browser History | 跨平台 | Supported | Supported / GA | medium fixture 规划中 (Chrome/Edge/Firefox SQLite)。需新建 artifacts-windows 浏览器模块、fixture、expected.json |
| Email extraction | 跨平台 | Supported | Supported / GA | medium fixture 规划中 (EML/EMLX)。需新建 artifacts-windows 邮件模块、fixture、expected.json。PST/OST/mbox 延后 |

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
| PST | 跨平台 | Experimental | Supported / GA | public-small + public-medium + private-real fixture。Unicode 32/64-bit。消息 + 附件 + 文件夹 + 日历 + 联系人。加密 PST 延后至 V4 |
| OST | 跨平台 | Experimental | Supported | public-small fixture。离线文件夹表与同步元数据 |
| mbox | 跨平台 | Experimental | Supported / GA | public-small + public-medium fixture。RFC 4155 变体全覆盖 (mboxrd/mboxo/mboxcl/mboxcl2) |

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
