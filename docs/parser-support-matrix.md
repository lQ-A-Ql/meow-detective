# Parser 支持矩阵

## 1. 说明

本文档描述“当前实现支持到什么程度”，不是路线图，也不是 PRD 愿景。

支持等级定义：

- `GA`：公开 small fixture + 至少一类回归样本已验证
- `Beta`：公开 fixture 已验证，但真实样本覆盖仍不足
- `Experimental`：实现存在，但样本、边界或自动化仍不足
- `Unsupported`：当前不承诺

## 2. 核心矩阵

| 链路 | 当前等级 | 已验证样本 | 对齐基准 | 字段承诺 | 备注 |
|---|---|---|---|---|---|
| E01 reader | Beta | `tiny.E01` | 项目内 fixture | open / read / seek / EOF | 多段与复杂变体待补 |
| RAW reader | GA | `tiny.raw` | 项目内 fixture | open / read / seek | 基础链路稳定 |
| NTFS parser | Beta | synthetic NTFS fixture、tiny raw | fixture 断言 | 枚举、读取、部分 deleted / hidden / system | 复杂损坏样本不足 |
| FAT parser | Experimental | 有限代码路径验证 | 现有测试 | 基本枚举 | deleted 不承诺 |
| exFAT parser | Experimental | 有限代码路径验证 | 现有测试 | 基本枚举 | deleted 不承诺 |
| EVTX | Beta | `system.evtx` | tiny fixture | 基本事件抽取 | 大样本待补 |
| Prefetch | Beta | synthetic fixture、real harness | `expected.json` / 人工对照 | executable、run count 等核心字段 | 历史版本覆盖不足 |
| LNK | Beta | synthetic fixture、real harness | `expected.json` / 人工对照 | target path、时间、参数 | 复杂 shell item 待补 |
| Registry | Beta | tiny hive、real harness | tiny fixture / `expected.json` | 系统信息、关键键值、provenance | 全 hive 覆盖不足 |
| Recycle Bin | Beta | synthetic fixture、real harness | `expected.json` | 原路径、删除时间 | 损坏恢复不承诺 |

## 3. 数据源分析补充链路

| 链路 | 当前等级 | 已验证样本 | 字段承诺 | 备注 |
|---|---|---|---|---|
| Chrome History | Experimental | mock / 部分开发样本 | visits / downloads 基本字段 | 公开 medium fixture 待补 |
| Edge History | Experimental | mock / 部分开发样本 | visits / downloads 基本字段 | 公开 medium fixture 待补 |
| Firefox History | Experimental | mock / 部分开发样本 | visits 基本字段 | schema 差异需继续收口 |
| Email extraction | Experimental | mock / 部分开发样本 | sender / recipients / subject / preview | 样本与 expected 基线待补 |

## 4. 字段承诺规则

- 核心字段：至少在 small fixture 中有自动化断言
- 真实字段：至少在一类真实样本回归中有对照基准
- 非稳定字段：只能标记为 `bestEffort`
- 当前无法稳定给出结果的字段不得写成“已支持”

## 5. 与文档同步要求

以下变化必须同步更新本文档：

- parser 新增支持格式
- 真实样本回归完成或失败
- 字段承诺升级或降级
- 已知不支持项变化

