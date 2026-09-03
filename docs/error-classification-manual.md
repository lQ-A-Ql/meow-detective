# 错误分类手册

## 1. 目标

本手册在 `docs/error-taxonomy.md` 的基础上，补齐 V2 期间对错误分类、脱敏、审计和前端承接的实施口径。

目标是让错误同时满足四件事：

- 对使用者可理解
- 对开发者可定位
- 对审计可归档
- 对安全边界不过度暴露内部细节

## 2. 分类分层

V2 统一从三层表达错误：

1. 领域层
   - parser
   - filesystem
   - image
   - search
   - timeline
   - report
2. 传输层
   - transport
   - validation
   - timeout
   - external
3. 安全与治理层
   - security
   - permission
   - policy
   - audit

## 3. 标准字段

对外错误 DTO 至少包含：

- `code`
- `message`
- `category`
- `recoverable`

建议内部还保留：

- `severity`
- `sourceLayer`
- `auditAction`
- `sensitive`

## 4. 推荐类别

| category | 说明 | 典型示例 |
|---|---|---|
| `validation` | 输入不合法或前置条件不满足 | 空路径、非法分页、参数越界 |
| `unsupported` | 当前能力不支持 | 不支持格式、未承诺字段、未实现功能；复杂 VMDK/ISO 变体 |
| `io` | 宿主文件系统或句柄读写失败 | 文件不存在、目标冲突、重命名失败、证据 extent 提前 EOF |
| `parser` | 解析失败或样本损坏 | EVTX/Prefetch/LNK/Registry/E01/ISO descriptor 损坏 |
| `security` | 被安全策略拦截 | 路径越界、非法 handle、MCP policy block、VMDK extent 目录逃逸 |
| `external` | 外部依赖失败 | MCP SSE / stdio / 网络错误 |
| `timeout` | 超时 | 外部连接超时、长任务超时 |
| `cancelled` | 用户或系统主动取消 | 分析/导入任务取消（可恢复，非超时） |
| `internal` | 其他内部错误 | join error、未分类异常 |

## 5. 脱敏口径

### 5.1 默认不返回

- 宿主完整绝对路径
- SQL 语句
- 堆栈
- 凭据、token、embedded credentials
- 外部工具完整 stderr
- 私有样本路径

### 5.2 允许返回

- 可操作的失败原因
- 错误类别
- 是否可恢复
- 简短冲突说明
- 指向文档或支持矩阵的提示

## 6. 与安全审计的关系

以下动作发生错误时，除前端提示外，还必须写入审计记录：

- MCP connect / test / list / call
- export / extract
- media handle access
- overwrite reject
- permission deny

审计记录允许写动作摘要与错误码，但不允许写入敏感凭据或原始私密载荷。

## 7. 前端承接要求

- 前端按 `category` 呈现差异化提示，但不得伪造后端语义
- 未识别类别统一降级为通用错误
- 详情页或调试面板只展示脱敏后的结构化字段
- 报告导出、MCP、媒体预览类错误必须突出“策略拒绝”与“系统异常”的差异

镜像 reader 错误必须保留结构化类别：`InvalidData`（格式字段自相矛盾）、
`UnexpectedEof`（底层证据短读）、`Unsupported`（未实现的容器/extent 映射）和
`PermissionDenied`（路径安全策略拒绝）。Tauri 层将这些类别映射为脱敏 `ApiErrorDto`；
不得把 descriptor 内容、extent 绝对路径或完整外部 stderr 原样带入 UI。

## 8. 与 V2 文档联动

以下变化必须同步更新本手册：

- 支持矩阵等级变化
- 安全策略变化
- 新增高风险命令
- 新增外部执行或导出能力
