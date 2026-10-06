# MCP 安全模型

## 1. 目标

MCP 在取证工具中天然敏感。本项目里的 MCP 必须是“最小权限、可审计、可解释”的受控扩展边界。

V2 安全治理与发布门禁总计划见：

- `docs/documentation-index.md`
- `docs/documentation-index.md`

## 2. 默认权限模型

默认权限：

- `resourceAccess = readOnly`
- `toolAccess = allowAll`
- `promptAccess = readOnly`
- `networkPolicy = localhostOnly`
- `allowedTools = []`
- `deniedTools = []`
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

- 默认允许发现和调用
- 前端连接后列出工具，用户可以逐项加入 `deniedTools`
- 被禁用的工具仍然显示在列表中，便于重新启用
- `allowList` 作为高级模式，仅允许 `allowedTools`
- `disabled` 作为显式全禁用模式

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

- 前端必须显式承接权限配置，并在工具列表提供逐项禁用/启用
- 未声明权限的 server 配置自动回落到最小权限
- UI 文案不得暗示 MCP 是任意执行通道

## 7. 后续增强建议

- case-scope 审计过滤视图
- MCP 权限模板
- tool call 参数脱敏策略
- 更强的 host allowlist / denylist

## 8. V2 期间的新增实施要求

- connect、disconnect、test、resource、tool、prompt 都必须纳入统一审计记录模型
- MCP 的权限、能力、审计结果必须能进入发布评分卡
- 未经说明的权限提升不得进入候选发布
