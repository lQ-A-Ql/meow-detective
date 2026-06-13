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
| Registry | transaction log 完整重放 | 不承诺 | 当前以 hive 直接解析为主 |
| Registry | private-real 回归 E01 | 缺失 | testdata/fixtures/private-real-regression/ 仅存 metadata.json，E01 镜像未提交 |
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
| Email | 全部 | 未实现 | artifacts-windows 中无邮件模块。EML/EMLX/PST/OST/mbox 均无实现 |

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
- Email 各类邮箱容器全覆盖（当前无任何实现代码）

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
