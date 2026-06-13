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
- `Experimental`：实现存在，但样本、边界或自动化仍不足
- `Unsupported`：当前不承诺

## 2. 核心矩阵

| 链路 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | 备注 |
|---|---|---|---|---|---|
| E01 reader | Beta | `tiny.E01` | expected.json / 8 个测试 | open / read / seek / EOF / chunk 解压 | public-medium 目录尚空，多段复杂变体待补 |
| RAW reader | GA | `tiny.raw` | expected.json / 1 个测试 | open / read / seek | 基础链路稳定 |
| NTFS parser | Beta | `tiny.raw` (NTFS volume) | expected.json / 11 个测试 | 枚举、读取、部分 deleted / hidden / system | 复杂损坏样本不足。public-medium/ntfs 尚空 |
| FAT parser | Experimental | 无 committed fixture | 5 个单元测试 | 基本枚举 | deleted 不承诺。expected.json 待建 |
| exFAT parser | Experimental | 无 committed fixture | 37 个单元测试 | 基本枚举（boot/FAT/dir） | deleted 不承诺。expected.json 待建 |
| EVTX | Beta | `system.evtx` | 10 个测试 | 基本事件抽取（boot/shutdown） | 大样本待补。expected.json 接入校验待加强 |
| Prefetch | Beta | 无 committed fixture | expected.json / 1 个 synthetic 测试 | executable、run_count 等核心字段 | testdata/artifacts/windows/prefetch/ 仅含 .gitkeep。历史版本与压缩变体覆盖不足 |
| LNK | Beta | 无 committed fixture | expected.json（无自动化测试） | target path、时间（expected.json 契约内） | testdata/artifacts/windows/lnk/ 仅含 .gitkeep。未发现 #[test]。复杂 shell item 待补 |
| Registry | Beta | `tiny SYSTEM`、`tiny SOFTWARE` | expected.json / 32 个测试 | 系统信息、关键键值、provenance | private-real 回归 E01 未提交。全 hive 覆盖不足 |
| Recycle Bin | Beta | 无 committed fixture | expected.json（无自动化测试） | 原路径、删除时间（expected.json 契约内） | testdata/artifacts/windows/recycle-bin/ 仅含 .gitkeep。未发现 #[test]。损坏恢复不承诺 |

## 3. 数据源分析补充链路

| 链路 | 当前等级 | 已验证样本 | 字段承诺 | 备注 |
|---|---|---|---|---|
| JumpList | Experimental | 无 committed fixture | 基本提取 | 2 个单元测试。expected.json 待建 |
| SRU | Experimental | 无 committed fixture | 基本提取 | 4 个单元测试。expected.json 待建 |
| Thumbcache | Experimental | 无 committed fixture | 基本提取 | 3 个单元测试。expected.json 待建 |
| Chrome History | Unsupported | 无 | 无 | artifacts-windows 中无浏览器模块。需新建 |
| Edge History | Unsupported | 无 | 无 | artifacts-windows 中无浏览器模块。需新建 |
| Firefox History | Unsupported | 无 | 无 | artifacts-windows 中无浏览器模块。需新建 |
| Email extraction | Unsupported | 无 | 无 | artifacts-windows 中无邮件模块。PST/OST/mbox 不承诺 |

## 4. 字段承诺规则

- 核心字段：至少在 small fixture 中有自动化断言
- 真实字段：至少在一类真实样本回归中有对照基准
- 非稳定字段：只能标记为 `bestEffort`
- 当前无法稳定给出结果的字段不得写成“已支持”

## 5. V2 目标状态

| 链路 | 当前等级 | V2 目标 | 说明 |
|---|---|---|---|
| E01 reader | Beta | Beta / 接近 GA | 前提是 public-medium fixture、真实样本回归与多段边界说明补齐 |
| RAW reader | GA | GA | 维持现状并补 benchmark 与发布说明 |
| NTFS parser | Beta | Beta / 接近 GA | 重点是复杂损坏、大样本与真实回归说明。补齐 public-medium/ntfs |
| FAT parser | Experimental | Experimental / Beta | 以基本枚举稳定性和边界说明为主，不承诺 deleted recovery。需建 expected.json |
| exFAT parser | Experimental | Experimental / Beta | 以基本枚举稳定性和边界说明为主，不承诺 deleted recovery。需建 expected.json |
| EVTX | Beta | Beta | 补真实样本与支持边界说明，不夸大为全覆盖。加固 expected.json 接入校验 |
| Prefetch | Beta | Beta / 接近 GA | 需补 committed fixture 文件、medium fixture、压缩变体边界、自动化测试 |
| LNK | Beta | Beta / 接近 GA | 需补 committed fixture 文件、自动化测试、复杂 shell item 边界说明 |
| Registry | Beta | Beta | 保持定向分析链路可信，不宣称完整 hive browser。补齐 private-real 回归 E01 |
| Recycle Bin | Beta | Beta / 接近 GA | 需补 committed fixture 文件、自动化测试、损坏恢复边界说明 |
| JumpList | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| SRU | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| Thumbcache | Experimental | Experimental / Beta | 需补 fixture、expected.json、自动化测试 |
| Browser History | Unsupported | Experimental / Beta | 需新建 artifacts-windows 浏览器模块、fixture、expected.json |
| Email extraction | Unsupported | Experimental / Beta | 需新建 artifacts-windows 邮件模块、fixture、expected.json。不承诺 PST/OST/mbox |

## 6. 与文档同步要求

以下变化必须同步更新本文档：

- parser 新增支持格式
- 真实样本回归完成或失败
- 字段承诺升级或降级
- 已知不支持项变化

同步目标文档：

- `docs/validation-trust-framework.md`
- `docs/known-unsupported-formats.md`
- `docs/release-scorecard.md`
