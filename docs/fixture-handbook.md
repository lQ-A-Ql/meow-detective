# Fixture 手册

## 1. 目标

本手册用于规范 V2 期间的 fixture 资产，确保样本可追溯、可公开复验、可分层运行，并与 expected JSON、支持矩阵、真实样本回归说明保持一致。

## 2. 分层模型

### 2.1 public-small

用途：

- 默认 CI
- 本地快速回归
- parser / service / frontend 最小能力校验

要求：

- 可直接进入仓库
- 体积小，适合高频执行
- 来源明确
- 目录必须带 README 或生成说明

当前默认来源：

- `testdata/fixtures/public-small/e01/`
- `testdata/fixtures/public-small/raw/`
- `testdata/fixtures/public-small/evtx/`
- `testdata/fixtures/public-small/logical/`

### 2.2 public-medium

用途：

- 专项 parser 回归
- 跨模块联调
- 人工复验
- benchmark small/medium 基线

要求：

- 可公开
- 能覆盖 small fixture 之外的结构和字段边界
- 与 `expected.json` 配套
- 必须附带 README，说明样本来源、覆盖点和未覆盖点

建议目录：

- `testdata/fixtures/public-medium/e01/`
- `testdata/fixtures/public-medium/ntfs/`
- `testdata/fixtures/public-medium/prefetch/`
- `testdata/fixtures/public-medium/lnk/`
- `testdata/fixtures/public-medium/registry/`
- `testdata/fixtures/public-medium/recycle-bin/`
- `testdata/fixtures/public-medium/browser/`
- `testdata/fixtures/public-medium/email/`

### 2.3 private-real-regression

用途：

- 真实样本回归
- release candidate 回归
- investigator 级验证摘要

要求：

- 不默认进入公开仓库
- 每次回归必须形成记录
- 不能只记录“通过”，必须记录对齐基准与未保证字段

配套记录：

- `docs/real-sample-regression/README.md`
- `docs/real-sample-regression/YYYY-MM-DD-*.md`

## 3. 样本元数据要求

每个 fixture 至少带以下元数据：

- 样本名称
- 链路类型
- 来源说明
- 合法性说明
- SHA-256
- 大小
- 预期覆盖点
- 敏感字段说明
- 是否允许公开
- 对应 expected JSON 路径

推荐 README 模板：

```md
# sample-name

- Chain: NTFS
- Visibility: public-small
- Source: synthetic / upstream sample / handcrafted
- Legal: reusable in repo
- SHA-256: ...
- Size: ...
- Coverage:
  - deleted record
  - hidden/system flags
  - orphan path fallback
- Expected JSON: `expected.json`
- Notes:
  - not suitable for damaged-volume recovery validation
```

## 4. 核心链路最低要求

| 链路 | public-small | public-medium | private-real-regression |
|---|---|---|---|
| E01 | 必需 | 必需 | 建议 |
| RAW | 必需 | 可选 | 可选 |
| NTFS | 必需 | 必需 | 建议 |
| Prefetch | 建议 | 必需 | 建议 |
| LNK | 建议 | 必需 | 建议 |
| Registry | 建议 | 必需 | 建议 |
| Recycle Bin | 建议 | 必需 | 建议 |
| Browser | smoke 必需 | 建议 | 建议 |
| Email | smoke 必需 | 建议 | 建议 |

## 5. 运行分层

### 5.1 默认 PR 回归

- public-small
- 最少量 public-medium smoke
- 不依赖私有样本

### 5.2 Nightly / 定时回归

- public-small
- 全量 public-medium
- benchmark medium

### 5.3 Release candidate

- public-small
- 全量 public-medium
- private-real-regression
- benchmark medium / large

## 6. 变更要求

以下情况必须同步更新 fixture 资产或说明：

- parser 新增支持格式
- parser 升级字段承诺
- expected JSON 结构变化
- 支持矩阵等级变化
- 已知不支持边界变化

## 7. 禁止事项

- 不允许把 mock 数据当成真实链路验证证据
- 不允许在没有 expected JSON 或基准说明的情况下把样本写为“已验证”
- 不允许把私有真实样本的敏感路径、账号、主机名直接写进公开文档

## 8. 当前 V2 治理事实源

当前 `/v2` 页面中的可信验证与支持矩阵，不再只来自 `v2_governance_service.rs` 内部硬编码，而是由仓库治理事实源驱动：

- `testdata/governance/v2-verification-catalog.json`

该文件当前承接：

- `verificationChains`
- `supportMatrixEntries`
- `supportMatrixSummary`
- `core-fixture-regression` gate 的上游事实基础

后续如果 fixture、expected JSON、支持等级、字段承诺、样本层级发生变化，必须同步更新该治理事实源。
