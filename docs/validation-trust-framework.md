# Forensics Workbench 可验证性体系

## 1. 目标

建立一套可公开、可复验、可扩展的验证体系，明确回答以下问题：

- 哪些 parser 在哪些样本上验证过
- 输出与什么基准对齐
- 哪些字段保证、哪些字段仅尽力而为
- 哪些样本进入默认 CI，哪些只用于真实样本回归

V2 长期执行总计划见：

- `docs/v2-longterm-plan.md`
- `docs/fixture-handbook.md`
- `docs/expected-json-contract.md`
- `docs/release-scorecard.md`

## 2. 样本分层

### 2.1 Public small

用途：默认 CI、快速回归、开发机最小验证

当前公开样本：

- `testdata/fixtures/public-small/e01/tiny.E01`
- `testdata/fixtures/public-small/raw/tiny.raw`
- `testdata/fixtures/public-small/evtx/system.evtx`
- `testdata/fixtures/public-small/logical/**`

要求：

- 可直接进入仓库
- 体积小、可重复执行
- 来源明确
- 样本目录必须带 README 或生成说明

### 2.2 Public medium

用途：专项 parser 回归、跨模块联调、人工复验

当前状态：

- 仓库内尚未形成稳定的 `testdata/fixtures/medium/` 目录
- 后续建议按链路拆分：
  - `medium/e01/`
  - `medium/ntfs/`
  - `medium/prefetch/`
  - `medium/lnk/`
  - `medium/registry/`
  - `medium/recycle-bin/`

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
  - 多段 E01
  - 厂商自定义压缩 / segment 变体

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

### 4.7 Linux 检材3 baseline

Linux 检材3是当前 Stage 0 Linux 单盘链路的真实样本 baseline，样本不提交仓库，默认 CI 不运行。公开仓库只记录可复验口径与 opt-in 命令。

- 样本：
  - 本地私有 E01：`FORENSICS_LINUX_E01_FIXTURE` 指向检材3
  - 参考路径：`D:\獬豸杯\检材3.E01`（仅作人工环境示例，不得硬编码到生产代码）
- 目标：
  - E01/RAW 读取与分区探测
  - LVM direct linear/striped LV 展开
  - XFS root LV 文件树枚举
  - 通过 `FileEntryId` 预览 `/etc/passwd`、`/etc/os-release`、`/etc/fstab`、`/root/.bash_history`、`/var/log/wtmp`
  - Linux artifacts 候选发现与提取（system config、journal、wtmp、bash history、cron、sudo/auth log）
- 基准：
  - `crates/app-services/tests/linux_e01_integration.rs` 中被 `#[ignore]` 标记的真实样本回归
  - `docs/pve-cluster-parsing-design.md` 的 Stage 0 单盘验收口径
- 运行命令：
  - 全量 opt-in：`$env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'; cargo test -p app-services --test linux_e01_integration -- --ignored --nocapture`
  - LVM smoke：`$env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'; cargo test -p app-services --test linux_e01_integration linux_e01_lvm_expansion_discovers_logical_volumes -- --ignored --nocapture`
  - 文件树/预览 smoke：`$env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'; cargo test -p app-services --test linux_e01_integration linux_e01_root_lv_system_info_paths_are_enumerated_readable_and_candidates -- --ignored --nocapture`
- 验收指标：
  - LVM pool 分区在 persisted partition metadata 中保留为 `Expanded` / `redirected`，但不得成为可展开可见 root。
  - root LV 以 `Partition 2 (XFS) - cl/root` 进入可见 roots，并保留 PV source / LV identity，供预览链路复用。
  - root LV 导入规模不得明显退化：文件数不低于 50,000，目录数不低于 7,000；当前 baseline 记录为 `files=51261`、`dirs=7149`。
  - `/etc` 枚举不得明显退化：当前 baseline 记录为 201 children。
  - `/etc/passwd`、`/etc/os-release`、`/etc/fstab`、`/root/.bash_history`、`/var/log/wtmp` 必须可通过 `FileEntryId` 预览读取。
  - 大文件预览必须覆盖 head / middle / tail range，不允许只验证首段。
  - Linux artifact extraction 必须来自真实枚举文件，不允许 synthetic insert；至少覆盖 `LinuxJournal`、`LinuxWtmp`、`LinuxBashCommand`、`LinuxCronJob`、`LinuxSudoEvent`、`LinuxSystemConfig`。
- 当前不保证：
  - PVE cluster 执行、多 E01 聚合或跨节点关联
  - LVM thin-pool、cache、RAID、snapshot、VDO、writecache、partial/degraded VG 激活
  - XFS/ext4/Btrfs 已删除文件恢复或 carving
  - 原始 Linux 文件系统支持超出当前实现可枚举范围时的完整恢复

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

- `docs/real-sample-regression/README.md`
- 或 `docs/real-sample-regression/YYYY-MM-DD-*.md`

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
- 或 `docs/benchmark-results/`

统一 benchmark 口径见 `docs/benchmark-baseline.md`。

## 9. 发布前最低验证要求

涉及核心链路变更时，至少完成：

1. small fixture 自动化通过
2. 对应 `expected.json` 回归通过
3. 更新 parser 支持矩阵
4. 更新已知不支持项
5. 如边界变化，更新错误分类与安全文档

## 10. 与其他权威文档的关系

- 样本目录规范：`docs/fixture-handbook.md`
- expected JSON 断言结构：`docs/expected-json-contract.md`
- 当前支持等级：`docs/parser-support-matrix.md`
- 当前已知不承诺边界：`docs/known-unsupported-formats.md`
- 发布评分：`docs/release-scorecard.md`
