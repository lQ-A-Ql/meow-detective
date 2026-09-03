# Forensics Workbench 可验证性体系

## 1. 目标

建立一套可公开、可复验、可扩展的验证体系，明确回答以下问题：

- 哪些 parser 在哪些样本上验证过
- 输出与什么基准对齐
- 哪些字段保证、哪些字段仅尽力而为
- 哪些样本进入默认 CI，哪些只用于真实样本回归

V2 长期执行总计划见：

- `docs/documentation-index.md`
- `testdata/fixtures/` 的目录和 expected JSON 契约
- `docs/expected-json-contract.md`

## 2. 样本分层

### 2.1 Public small

用途：默认 CI、快速回归、开发机最小验证

当前公开样本：

- `testdata/fixtures/public-small/e01/tiny.E01`
- `testdata/fixtures/public-small/raw/tiny.raw`
- `testdata/fixtures/public-small/evtx/system.evtx`
- `testdata/fixtures/public-small/logical/**`

当前没有提交 ISO 或 VMDK 二进制 fixture。ISO9660/Joliet 与 flat VMDK 使用确定性的
synthetic unit fixtures 验证；本地真实 ISO 只能通过 `FORENSICS_ISO_FIXTURE` 进入
ignored 回归，不提升公开支持等级，也不把主机路径写入文档或治理 artifact。

真实 ISO 回归命令（仅在本机已有样本时运行）为：

```powershell
$env:FORENSICS_ISO_FIXTURE = '<read-only ISO path>'
cargo test -p evidence-core --test iso_real -- --ignored --nocapture
Remove-Item Env:FORENSICS_ISO_FIXTURE -ErrorAction SilentlyContinue
```

要求：

- 可直接进入仓库
- 体积小、可重复执行
- 来源明确
- 样本目录必须带 README 或生成说明

### 2.2 Public medium

用途：专项 parser 回归、跨模块联调、人工复验

当前状态：

- 仓库已经使用 `testdata/fixtures/public-medium/` 承载可公开的 email 与 Linux journal
  fixture；镜像适配器和 Windows parser 的 medium fixture 仍按链路逐项补齐。
- 后续建议在同一 `public-medium/` 根目录下按链路拆分：
  - `public-medium/e01/`
  - `public-medium/iso/`
  - `public-medium/vmdk/`
  - `public-medium/ntfs/`
  - `public-medium/prefetch/`
  - `public-medium/lnk/`
  - `public-medium/registry/`
  - `public-medium/recycle-bin/`

要求：

- 可公开
- 能覆盖 small fixture 之外的结构和字段边界
- 与 `expected.json` 配套

### 2.3 Private / real regression

用途：真实样本回归，不默认进入公开仓库

当前口径：

- `crates/artifacts-windows/tests/fixture_real_test.rs`
- `testdata/artifacts/windows/**/expected.json`

记录要求：

- 样本来源类别
- 样本 hash / 大小 / 敏感等级
- 运行命令
- 运行环境
- 对齐基准
- 结果
- 未保证字段

## 3. expected JSON 机制

真实样本回归统一使用 `expected.json` 做断言基线。更完整的结构与分级约定见 `docs/expected-json-contract.md`。

推荐结构：

```json
[
  {
    "file": "sample.pf",
    "expected": {
      "baseline": "windows-prefetch-parser-x.y",
      "assertions": {
        "executable": "CMD.EXE",
        "runCountGt": 0
      },
      "guaranteedFields": ["executable", "runCount"],
      "bestEffortFields": ["runTimes", "volumeSerialNumber"]
    }
  }
]
```

要求：

- `baseline` 必须说明对齐对象
- `guaranteedFields` 表示发布承诺字段
- `bestEffortFields` 表示非稳定字段
- 不得把样本中没有可靠基准的字段误写为 guaranteed

## 4. 核心链路验证口径

### 4.1 E01

- 样本：
  - `testdata/fixtures/public-small/e01/tiny.E01`
- 目标：
  - open
  - read
  - seek
  - EOF 行为
- 基准：
  - 项目内 synthetic fixture 预期
- 当前不保证：
  - 复杂多段 E01 的全部厂商变体（多段自动识别已实现，但仍缺公开 medium fixture）
  - 厂商自定义压缩 / segment 变体

### 4.1a 镜像格式适配器（RAW、flat VMDK、ISO9660/Joliet）

这些适配器的验证对象是“逻辑字节视图和边界行为”，不是把所有格式都标成可启动或
可写。测试与生产链路必须遵守以下分层：

| 适配器 | 必测断言 | 当前验证入口 | 公开承诺边界 |
|---|---|---|---|
| RAW/dd/img | `Read + Seek`、EOF、独立 cursor、长度与 OS metadata 一致 | `cargo test -p evidence-core --lib` | 单文件原始字节透传；不合并分卷 |
| monolithic-flat VMDK | descriptor 解析、`sector_count * 512` 逻辑容量、extent 截断/溢出、相对路径安全、跨扇区读取 | `cargo test -p evidence-core --lib`、`cargo test -p evidence-block`、`cargo test -p app-services hash_evidence_vmdk_covers_descriptor_and_flat_extent --lib` | 仅 UTF-8、单个零偏移 `FLAT` extent；descriptor 与 extent 都计入证据身份 |
| ISO9660/Joliet | PVD/终止描述符、Joliet 优先、目录递归上限、both-endian 一致性、文件/目录 extent 卷边界、seekable preview | `cargo test -p evidence-core --lib`；真实 ISO 使用 `FORENSICS_ISO_FIXTURE` ignored 测试 | 可叠加任意 `EvidenceReader`，包括 RAW、E01 和受支持 flat VMDK；不支持 UDF/Rock Ridge/交错或 multi-extent |

适配器组合测试还必须覆盖非零分区窗口：先用 `PartitionWindowReader` 将分区变为零起点，
再探测文件系统，不能把绝对镜像偏移传入 ISO 或其他文件系统 reader。ISO 的 PVD 卷大小
超过底层证据时返回 `UnexpectedEof`；记录格式自相矛盾时返回 `InvalidData`；未实现的
容器映射返回 `Unsupported`。错误类别是验收的一部分，不能用普通 RAW fallback 掩盖。

flat VMDK 的组合哈希采用稳定 manifest：按 descriptor、extent 的固定顺序记录文件长度
与 SHA-256，再对 manifest 求摘要。该摘要用于来源身份和后台任务完整性，不等同于虚拟机
启动成功证明。导入预检查同样使用 VMDK 的逻辑容量，避免用几 KB descriptor 大小低估
资源需求。

### 4.2 NTFS

- 样本：
  - `testdata/fixtures/public-small/raw/tiny.raw`
  - synthetic NTFS fixture
- 目标：
  - MFT 枚举
  - 目录树
  - 文件读取
  - deleted / hidden / system 状态
- 基准：
  - fixture builder 断言
  - 真实链路回归说明
- 当前不保证：
  - 全部损坏场景
  - 全部历史 NTFS 变体

### 4.3 Prefetch

- 样本：
  - synthetic fixture
  - `testdata/artifacts/windows/prefetch/expected.json`
- 目标：
  - 可执行名
  - run count
  - 关键时间字段
- 基准：
  - `expected.json`
  - 人工与外部解析结果对照
- 当前不保证：
  - 所有压缩变体
  - 所有历史版本字段细节

### 4.4 LNK

- 样本：
  - synthetic fixture
  - `testdata/artifacts/windows/lnk/expected.json`
- 目标：
  - target path
  - create / access / write time
  - 参数、工作目录等核心字段
- 基准：
  - `expected.json`
  - 人工对照
- 当前不保证：
  - 全部 shell item 复杂变种

### 4.5 Registry

- 样本：
  - tiny SYSTEM / SOFTWARE hive
  - `testdata/artifacts/windows/registry/expected.json`
- 目标：
  - 系统信息
  - 关键键值提取
  - provenance 对齐
- 基准：
  - tiny fixture 断言
  - `expected.json`
- 当前不保证：
  - 全 hive 类型全覆盖
  - transaction log 重放完整性

### 4.6 Recycle Bin

- 样本：
  - synthetic fixture
  - `testdata/artifacts/windows/recycle-bin/expected.json`
- 目标：
  - 删除前原路径
  - 删除时间
  - 记录结构
- 基准：
  - `expected.json`
- 当前不保证：
  - 损坏恢复
  - 全部历史版本差异

### 4.7 Linux LVM/XFS 回归契约

Linux 单盘真实样本通过 `FORENSICS_LINUX_E01_FIXTURE` opt-in 注入，默认 CI 不运行。Git 只保留可复验的契约与测试入口，不记录工作站路径、枚举数量、耗时或样本 hash。

- 目标：
  - E01/RAW/flat VMDK 读取与分区探测（检材3 baseline 实际使用 E01；其他格式以各自适配器契约为准）
  - LVM direct linear/striped LV 展开
  - XFS root LV 文件树枚举
  - 通过 `FileEntryId` 预览 `/etc/passwd`、`/etc/os-release`、`/etc/fstab`、`/root/.bash_history`、`/var/log/wtmp`
  - Linux artifacts 候选发现与提取（system config、journal、wtmp、shell history、package logs、cron、sudo/auth log）
  - Linux extraction run 返回 9 个独立板块进度：`LinuxJournal`、`LinuxLogin`、`LinuxCommands`、`LinuxPackages`、`LinuxCron`、`LinuxSudo`、`LinuxSystemConfig`、`LinuxWebServices`、`LinuxMysqlServices`
- 基准：
  - `crates/app-services/tests/linux_e01_integration.rs` 中被 `#[ignore]` 标记的真实样本回归
  - `docs/pve-cluster-parsing-design.md` 的 Stage 0 单盘验收口径
- 运行：设置环境变量后执行 `cargo test -p app-services --test linux_e01_integration -- --ignored --nocapture`；单项测试名称以该 integration test 文件为准。
- 验收指标：
  - LVM pool 分区在 persisted partition metadata 中保留为 `Expanded` / `redirected`，但不得成为可展开可见 root。
  - root LV 以 `Partition 2 (XFS) - cl/root` 进入可见 roots，并保留 PV source / LV identity，供预览链路复用。
  - 文件与目录计数必须同受控基线比较，避免枚举静默退化；基线值只保存为 CI 或本机 artifact。
  - `/etc` 枚举必须存在可验证子节点，具体数量由受控 baseline 断言。
  - `/etc/passwd`、`/etc/os-release`、`/etc/fstab`、`/root/.bash_history`、`/var/log/wtmp` 必须可通过 `FileEntryId` 预览读取。
  - 大文件预览必须覆盖 head / middle / tail range，不允许只验证首段。
  - Linux artifact extraction 必须来自真实枚举文件，不允许 synthetic insert；至少覆盖 `LinuxJournal`、`LinuxWtmp`、`LinuxBashCommand`、`LinuxCronJob`、`LinuxSudoEvent`、`LinuxSystemConfig`。
  - Linux extraction section progress 必须包含全部 9 个板块；journal、login、commands、cron、sudo、system config 在检材3上必须有真实扫描与 artifact 产出；packages 若样本存在 yum/dnf/rpm/apt/dpkg 日志则必须产出 `LinuxAptEvent` 包事件；Web/MySQL section 必须按真实候选存在性运行，缺少候选时返回独立的零结果进度，不得伪造 artifact。
- 当前不保证：
  - PVE cluster 语义解析、多 E01 聚合分析或跨节点关联
  - LVM thin-pool、cache、RAID、snapshot、VDO、writecache、partial/degraded VG 激活
  - Btrfs 已删除文件恢复与全盘 carving；XFS 未验证内容恢复不作完整文件承诺
  - 原始 Linux 文件系统支持超出当前实现可枚举范围时的完整恢复

### 4.7a EXT4/XFS deleted recovery baseline

删除恢复是独立于普通文件树与 Hex 预览的只读取证链路。扫描结果持久化到 source DB，并以数据源、扫描身份和恢复候选身份隔离；未验证内容不得进入普通文件表或被导出为完整文件。

- EXT4：JBD2 descriptor/commit/revoke、v1/v2/v3 tag、CRC32C 与 direct extent depth-0 候选已覆盖；complete 候选要求所有 content range 连续、allocation 为 `free`、每段 SHA-256 与完整内容 digest 均匹配。partial 候选只允许读取后端验证的连续 range。
- XFS：internal log checkpoint、ring wrap、transaction provenance 和显式 `nlink=0` 删除证据已覆盖；当前候选默认为 metadata-only，除非后端建立完整 allocation 与 digest 证据，不提供内容读取或导出。
- 私有真实样本只通过环境变量注入；candidate 数量、issue 数量、耗时和 snapshot digest 作为 CI/工作站 artifact 保存，不写入版本库文档。
- 服务回归命令：`cargo test -p app-services deleted_recovery --lib -- --test-threads=1`。
- 前端回归覆盖：未扫描状态、EXT4/XFS 分区筛选、已验证 range 读取、metadata-only 禁止读取/导出、complete 候选经 platform save adapter 导出。

当前不保证：间接 extent、目录内容恢复、XFS 未验证文件内容、Btrfs 删除恢复、全盘 carving，以及所有 journal/log 损坏和 filesystem feature 组合。

#### 4.7b Deleted recovery defect ledger

The following limitations are recorded defects of the current implementation,
not accepted evidence claims:

- EXT4 and XFS recovery candidates do not reconstruct the original filename or
  parent path; `originalPath` is currently absent, so inode identity cannot be
  presented as a recovered namespace path.
- XFS candidates are metadata-only. File type, mode, deletion time, declared
  size, allocation state, and file content are not reconstructed.
- The recovery UI always requests a selected range from logical offset zero
  and has no arbitrary offset control. Verified ranges whose logical offset is
  non-zero cannot currently be inspected through the UI.
- The UI caps a range preview at 1 MiB, exposes only a spinner during scanning,
  and does not render the persisted scan issue list or all scan warnings.
- JBD2 revoke status is retained internally but is not exposed as a dedicated
  DTO/UI evidence field, so reviewers cannot directly distinguish revoked
  historical mappings from ordinary candidates.
- A replacement scan removes the previous partition result. Snapshot hashes
  provide identity, but the source database does not retain scan history.
- XFS log provenance is currently serialized with `sourceKind=filesystem` even
  though its byte spans originate from the internal log; this must be corrected
  before provenance is used for content claims.

These defects remain outside the current recovery acceptance boundary and must
be closed before the feature is promoted beyond experimental status.

#### 4.7c Windows deleted-file recovery audit ledger

The Windows path is currently a deleted-entry visibility path, not a
forensic recovery path. The following defects are recorded from the current
source implementation:

- `app-services::deleted_recovery::source::recovery_filesystem` and
  `scan_target` only admit EXT4 and XFS. NTFS is rejected before a recovery
  scan can be created, so the Linux recovery DTOs and range/export commands
  must not be presented as Windows recovery support.
- `fs-ntfs::mft_scanner` maps an inactive MFT record (`FILE` flags without
  `in-use`) to `file_entries.deleted`, but the persisted `mft:<partition>:<record>`
  identity discards the MFT sequence number. A reused record number can
  therefore be mistaken for the historical deleted object. No independent
  recovery candidate, scan identity, allocation state, overwrite check,
  provenance range, or content digest is persisted for these records.
- `NtfsReader::read_file_data_range` accepts a `FILE` record without checking
  its in-use state, sequence number, or deleted-record evidence. The MFT ID
  preview fallback can consequently read bytes from a stale record when data
  runs still look valid, but the result has no recovery-grade integrity claim.
- The Recycle Bin extractor consumes only `$I` metadata. It does not discover
  or bind the corresponding `$R` payload, and it stores the artifact against
  the `$I` file object. `recovered_file_size` is metadata and must not be
  treated as recovered content.
- The current `$I` parser assumes a synthetic header-size field. Real
  Windows 8+ `$I` files use the first eight bytes as a version field and keep
  size, deletion FILETIME, and original path at offsets `0x08`, `0x10`, and
  `0x18`. The existing fixture only exercises the synthetic `0x20` header
  shape and does not protect the real version-1/version-2 layouts.
- `fs-ntfs::logfile::build_file_change_history` is not called by the import,
  analysis, persistence, or recovery services. Its operation mapping is
  therefore diagnostic-only; `$LogFile` output currently cannot create a
  reviewable or exportable Windows deletion candidate.
- Existing real-sample Recycle Bin checks tolerate an empty bin and assert
  extraction counts only as diagnostic output. There is no positive contract
  requiring a real `$I` metadata row, a paired `$R` payload, or a verified
  content read when the sample contains such a pair.

The first Windows implementation boundary is therefore: recoverable NTFS
MFT data only after sequence-aware identity and allocation/provenance checks;
use Recycle Bin `$I` as deletion/path metadata and `$R` as a separately
verified payload; keep `$LogFile` as corroborating evidence until transaction
replay is validated. No Windows candidate should be exported merely because
`file_entries.deleted = 1`.

### 4.8 Windows/Linux 双源隔离 baseline

双源隔离使用两个不入库的 Windows/Linux E01 通过环境变量注入。格式适配器的真实样本
回归同样只允许环境变量注入，且每个 reader 必须在独立 source DB 中完成。`scripts/check-stage2-real-sample-isolation.ps1`
默认验证两种串行导入顺序；带 `-RequireFixtures` 时，缺少 fixture 必须失败，不能伪装为通过。

验收必须同时证明：`app.db` 不承载文件树、两个 `source.db` 均为 `ready`、分区平台不串染、文件 ID 为全局 `ds:<dataSourceId>:<localId>` 形式、两源均可预览，且 artifact/timeline/correlation ID 保持数据源作用域。

### 4.9 PVE/Ceph 验证契约

PVE 集群测试通过 `FORENSICS_PVE_CLUSTER_ROOT` 等环境变量 opt-in，默认 CI 不运行。测试必须在缺少必需 fixture 时失败，不得以 early return 伪装通过；真实输出只保存为本机或 CI artifact。

验证分层如下：

- 宿主成员：E01 -> GPT -> LVM -> EXT4 的成员发现、source DB 隔离、文件树和关键文件 range 预览；
- BlueStore/BlueFS：bdev label、CRC32C、superblock、extent 和 bounded replay 的只读解析；
- RocksDB：CURRENT/IDENTITY/MANIFEST、VersionEdit、live-SST、active-WAL 和 latest-state 的 wire-level oracle；
- RADOS/RBD：在已验证 inventory 和映射条件下执行 source-local byte reconstruction，并绑定完整性 digest；
- CephFS：只有存在已验证 FSMap/MDSMap、namespace locator 和 layout proof 时才允许 materialize/preview，否则保持 unsupported 或 indeterminate。

所有成员写入各自 `source.db`，普通文件树、BlueStore metadata 和派生语义记录不得跨数据源混写。通用 PG/CRUSH/acting-set、EC、degraded replica、跨 PV LVM、clone/snapshot/encryption、任意 CephFS 集群和跨节点语义关联仍不属于默认支持声明。

### 4.10 聚合验收契约

真实样本验收由 ignored integration tests、环境变量和治理 artifact 组成。版本库只保存测试代码、字段契约和支持边界；不保存私有样本路径、逐次耗时、机器内存、样本 hash 或阶段报告。

聚合结果必须能够复核：

- source DB ready 状态、source-local ID 路由和跨源隔离；
- parser expected JSON、结构 oracle、输出 digest 和错误分类；
- 首屏/中段/尾部 range 读取、文件提取长度与 SHA-256；
- 各平台分析 section 的独立进度、partial/unsupported warning 与非伪造结果；
- 取消、失败关闭、重试和缓存失效行为。

## 5. 浏览器与邮件链路

当前这些链路已经纳入框架，但成熟度仍低于核心链路：

| 链路 | 当前状态 | 说明 |
|---|---|---|
| Chrome History | Experimental | 已进入数据源分析能力，但 fixture 与基准不足 |
| Edge History | Experimental | 同上 |
| Firefox History | Experimental | 同上 |
| Email extraction | Experimental | 已有抽取建模，公开样本与基准待补 |

当前产品内 `/v2` 页面已经开始承接这套口径：

- `verificationChains`
- `supportMatrixEntries`
- `releaseGates`

从 2026-06-13 起，这里的 `verificationChains` / `supportMatrixEntries` 不再只写死在 Rust 代码中，而是优先由仓库治理事实源 `testdata/governance/v2-verification-catalog.json` 提供。

其中 `releaseGates.core-fixture-regression` 已开始直接读取核心链路的验证状态，不再是假定“统一通过”。

也就是说，可信验证不再只存在于文档和测试里，已经有一条面向 investigator / release reviewer 的可见链路。
从 2026-06-13 起，`knownLimitations` 与 `supportMatrix.documentedLimitCount` 也开始由独立事实源 `testdata/governance/v2-known-limitations.json` 提供，不再继续由 Rust 侧手工维护一份并与 `docs/known-unsupported-formats.md` 漂移。

## 6. 字段承诺分级

- `guaranteed`：在公开 fixture 与至少一类回归样本上稳定验证
- `bestEffort`：已有实现，但覆盖仍不足或依赖样本条件
- `unsupported`：当前不承诺

发布或文档中引用字段时，必须显式说明属于哪一级。

## 7. 真实样本回归说明模板

每次真实样本回归至少记录：

- 日期
- 样本类别
- 样本 hash
- 样本大小
- 运行命令
- 运行环境
- 对照基准
- 结果
- 未保证字段

建议落地为：

- 私有样本运行记录不进入 Git；使用测试文件、环境变量和支持矩阵记录可复验边界

## 8. Benchmark 口径

后续 benchmark 至少包含：

- parser 名称
- 样本名称
- 样本大小
- 冷启动 / 热启动
- 耗时
- 峰值内存
- 运行环境

建议目录：

- `benchmarks/`
- benchmark 输出只作为本机/CI artifact 保存，不作为版本库文档

统一 benchmark 口径见 `docs/benchmark-baseline.md`。

## 9. 发布前最低验证要求

涉及核心链路变更时，至少完成：

1. small fixture 自动化通过
2. 对应 `expected.json` 回归通过
3. 更新 parser 支持矩阵
4. 更新已知不支持项
5. 如边界变化，更新错误分类与安全文档
6. 如镜像适配器或嵌套链路变化，补齐逻辑长度、reader 组合、哈希身份和 fail-closed 回归

## 10. 与其他权威文档的关系

- 样本目录规范：`testdata/fixtures/` 与本文件的 fixture 分层规则
- expected JSON 断言结构：`docs/expected-json-contract.md`
- 当前支持等级：`docs/parser-support-matrix.md`
- 当前已知不承诺边界：`docs/known-unsupported-formats.md`
- 治理事实源与技术文档入口：`docs/documentation-index.md`
- 验证链路事实源：`testdata/governance/v2-verification-catalog.json`
