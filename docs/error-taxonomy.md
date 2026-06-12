# 错误分类与脱敏口径

## 1. 目标

统一后端、Tauri 和前端对错误的表达方式，保证：

- 用户能拿到足够清晰的失败语义
- 内部路径、SQL、系统细节不直接泄露
- 测试、审计与 UI 能按类别处理错误

## 2. 当前错误分类

| 类别 | 含义 | 示例 |
|---|---|---|
| `validation` | 输入不合法或前置条件不满足 | `NO_ACTIVE_CASE`、`INVALID_INPUT` |
| `unsupported` | 当前能力不支持 | 不支持的格式或操作 |
| `io` | 文件系统或宿主 IO 失败 | 导出写入失败、文件读取失败 |
| `parser` | 解析输入不可靠或损坏 | hive / prefetch / lnk / image parse 失败 |
| `security` | 被安全策略拦截 | MCP policy block、路径越界、非法 handle |
| `external` | 外部依赖失败 | MCP SSE / stdio / 网络错误 |
| `timeout` | 超时 | 外部连接或长操作超时 |
| `internal` | 其他内部错误 | join error、未分类内部异常 |

## 3. CommandError 口径

后端 `CommandError` 对外输出：

- `code`
- `message`
- `category`
- `recoverable`

要求：

- `message` 面向前端可见，必须脱敏
- 原始错误细节只留在日志里

## 4. 脱敏要求

默认不直接返回：

- 宿主完整绝对路径
- SQL 语句
- 栈信息
- 凭据、token、embedded credentials
- 外部工具完整 stderr

允许返回：

- 用户可操作的失败原因
- 错误类别
- 是否可恢复
- 简短冲突说明

## 5. 前端要求

- `ApiErrorDto` 必须可解析 `category`
- UI 可以按 `category` 做差异化提示，但不得伪造后端语义
- 未识别类别时降级为通用错误

## 6. 与审计关系

- 面向用户的错误是脱敏后的
- 审计日志记录动作、结果和必要摘要
- tracing / 本地调试日志可保留更细上下文，但不作为前端直接展示内容

