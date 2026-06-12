# MCP 使用指南

## 1. 概览

Forensics Workbench 支持通过 MCP 接入外部模型上下文服务，但默认采用最小权限策略。

MCP 在本项目中主要用于：

- 读取资源
- 查询 prompts
- 在显式授权时调用少量工具

它不是默认开放的任意执行通道。

## 2. 支持的传输

### 2.1 SSE

- 仅允许 `http/https`
- 不允许 URL 内嵌用户名和密码
- 默认只允许 localhost

### 2.2 Stdio

- command 必须是可执行名
- 不能写成绝对路径或相对路径
- 默认会把当前 command 加入 `allowedCommands`

## 3. 默认权限

若未手工配置权限，系统默认：

- 资源：只读
- 工具：禁用
- prompts：只读
- 网络：仅 localhost

## 4. 配置建议

### 4.1 本地 SSE

```text
名称: Local MCP
传输: SSE
URL: http://127.0.0.1:3001/sse
```

### 4.2 本地 stdio

```text
名称: Local Stdio MCP
传输: Stdio
Command: node
Args: ["server.js"]
```

## 5. 安全建议

- 只连接可信服务
- 工具权限不要默认打开
- 如启用 allow list，只放入必要工具
- 关键动作会记录审计日志

## 6. 常见问题

### 连接失败

检查：

- 服务是否启动
- URL / command 是否正确
- 是否被网络策略或 allow list 拒绝

### 工具不能调用

检查：

- `toolAccess` 是否为 `allowList`
- 当前工具是否在 `allowedTools` 中

### stdio 被拒绝

检查：

- command 是否写成了路径
- command 是否在 `allowedCommands` 中

## 7. 相关文档

- `docs/mcp-security-model.md`
- `docs/error-taxonomy.md`
- `docs/documentation-index.md`
