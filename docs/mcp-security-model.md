# MCP 安全模型

## 1. 目标

MCP 在取证工具中天然敏感。本项目里的 MCP 必须是“最小权限、可审计、可解释”的受控扩展边界。

## 2. 默认权限模型

默认权限：

- `resourceAccess = readOnly`
- `toolAccess = disabled`
- `promptAccess = readOnly`
- `networkPolicy = localhostOnly`
- `allowedTools = []`
- `allowedCommands = []`

对于 stdio：

- 若 `allowedCommands` 为空，默认补入当前 command
- 若显式 allowlist 不包含当前 command，则拒绝

## 3. 传输安全

### 3.1 SSE

- 仅允许 `http/https`
- 禁止 embedded credentials
- 受 `networkPolicy` 约束：
  - `localhostOnly`
  - `privateLanAllowed`
  - `anyHost`

### 3.2 Stdio

- command 必须是可执行名，不是路径
- command 和 args 禁止 NUL
- command 必须满足 allow list

## 4. 能力访问控制

### 4.1 Resources

- `disabled` 时拒绝
- `readOnly` 时仅允许 list / read

### 4.2 Tools

- 默认禁用
- `allowList` 时仅允许 `allowedTools`

### 4.3 Prompts

- `disabled` 时拒绝
- `readOnly` 时允许 list / get

## 5. 审计要求

以下动作必须写审计日志：

- connect
- disconnect
- test
- resource list / read
- tool list / call
- prompt list / get

建议记录：

- server id
- transport 类型
- host 或 command 名称
- tool / prompt / resource 摘要
- success / failed

禁止记录：

- 完整凭据
- 原始敏感响应
- 不必要的本地绝对路径

## 6. 前端约束

- 前端必须显式承接权限配置，不得默认“全开”
- 未声明权限的 server 配置自动回落到最小权限
- UI 文案不得暗示 MCP 是任意执行通道

## 7. 后续增强建议

- case-scope 审计过滤视图
- MCP 权限模板
- tool call 参数脱敏策略
- 更强的 host allowlist / denylist

