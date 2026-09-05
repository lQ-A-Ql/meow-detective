# Expected JSON 契约说明

## 1. 目标

Expected JSON 是 V2 可信验证体系的断言基线，用来回答两个问题：

- 输出与什么基准对齐
- 哪些字段属于发布承诺，哪些字段只是尽力而为

Expected JSON 不用于描述 parser 的全部原始输出，而用于描述“哪些输出必须稳定”。

## 2. 统一结构

推荐结构如下：

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
      "bestEffortFields": ["runTimes", "volumeSerialNumber"],
      "notGuaranteedFields": ["devicePath"]
    }
  }
]
```

## 3. 字段语义

### 3.1 baseline

必须说明对齐对象，例如：

- 项目内 synthetic fixture 预期
- 外部基准工具版本
- 人工校验记录
- 真实样本回归说明

### 3.2 assertions

仅写稳定断言，不写噪声字段。

允许的断言类型：

- 完全相等
- 范围
- 非空
- 数量下限
- 集合包含

### 3.3 guaranteedFields

表示发布承诺字段，要求：

- 至少在 public-small 中自动验证
- 至少在一类真实样本或 medium fixture 中对齐过
- 失败视为发布阻断

### 3.4 bestEffortFields

表示已实现但覆盖仍不足或依赖样本条件的字段，要求：

- 可以展示
- 不能写成“稳定保证”
- 差异不自动等于发布阻断，但必须有说明

### 3.5 notGuaranteedFields

表示当前不保证稳定的字段，要求：

- 可以存在于输出
- 不纳入核心通过标准
- 在支持矩阵或已知不支持文档中有对应说明

## 4. 归一化规则

所有 expected JSON 在写入前统一遵守以下口径：

- 时间统一使用明确时区或 UTC 表达
- 路径统一使用该链路的标准显示形式
- 枚举值统一大小写与命名
- 集合字段必须定义排序规则
- 空值必须区分：
  - 未解析
  - 不存在
  - 不支持

## 5. 差异分类

Expected JSON 比对至少区分以下三类差异：

1. 结构差异
   - 字段缺失
   - 字段类型变化
2. 值差异
   - 相同字段值不一致
3. 允许漂移
   - bestEffort 字段
   - 样本相关且不纳入保证的字段

推荐输出摘要：

- pass
- fail
- partial
- warning

## 6. 最低实施要求

V2 期间，以下链路至少应配套 expected JSON：

- Prefetch
- LNK
- Registry
- Recycle Bin
- Browser History
- Email

对于 E01 / RAW / flat VMDK / ISO9660 / NTFS，如果更适合用结构化测试或 synthetic 断言承接，也必须在文档中说明其对齐口径。

### 6.1 镜像适配器的结构化断言

RAW 仍主要使用结构化单元测试作为第一阶段基线；flat VMDK 和 ISO9660/Joliet 已有
`testdata/fixtures/public-medium/` 中的公开中型二进制 fixture，并由结构化单元测试
补充边界断言。所有断言都应覆盖逻辑字节视图，而不是依赖主机文件名或平台特定的错误文本：

| 链路 | 应固定的断言 | 不应写入 guaranteedFields |
|---|---|---|
| RAW/dd/img | `kind=raw`、长度、首/尾部读取、seek/EOF、重复读取字节相等 | 主机文件句柄、缓存命中率、设备路径 |
| flat VMDK | `kind=vmdk`、descriptor/extent 组合身份、`sectorCount * 512` 逻辑长度、跨 extent 读取、截断/溢出拒绝 | descriptor 的物理路径、文件系统探测结果、VMware 启动结果 |
| ISO9660/Joliet | PVD 存在、逻辑块大小、Joliet 优先、目录/文件名、文件内容、卷边界与 seekable read | Rock Ridge/UDF 语义、未提交样本的时间字段、宿主路径 |

适配器的错误断言可按稳定类别表达：`InvalidData` 表示结构自相矛盾，`UnexpectedEof`
表示底层证据提前结束，`Unsupported` 表示未实现容器或 extent 映射，`PermissionDenied`
表示 VMDK extent 路径逃逸。断言不应要求完整错误字符串，以免把实现诊断文案误当作证据
字段；但错误类别变化必须同步更新支持矩阵和已知限制。

flat VMDK 的 manifest digest 是由 descriptor 与 FLAT extent 的长度和 SHA-256 组成的稳定
派生值。Expected JSON 可以断言 manifest 规则和 segment 顺序，但不能把该派生摘要冒充
为“原始单文件镜像 SHA-256”，也不能省略 descriptor 或 extent 的来源说明。

## 7. 文档联动

以下文档必须与 expected JSON 同步：

- `docs/parser-support-matrix.md`
- `docs/known-unsupported-formats.md`
- `docs/validation-trust-framework.md`
- `testdata/governance/v2-verification-catalog.json`
- 私有真实样本不进入 Git；样本执行入口和结果摘要必须与
  `docs/parser-support-matrix.md` 的支持等级保持一致

## 8. 禁止事项

- 不允许把没有可靠基准的字段写进 `guaranteedFields`
- 不允许把明显样本相关噪声写成通过标准
- 不允许 expected JSON 与公开文档对同一字段承诺给出相互矛盾的表述
