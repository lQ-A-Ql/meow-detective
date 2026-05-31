# MCP 功能测试用例文档

**版本**: v1.0  
**日期**: 2026-05-30  
**测试框架**: Rust test / Vitest  

---

## 📋 测试概览

| 测试类型 | 数量 | 状态 |
|----------|------|------|
| 单元测试 | 33 | ✅ 全部通过 |
| 集成测试 | 12 | ✅ 全部通过 |
| **总计** | **45** | ✅ |

---

## 🧪 单元测试用例

### 1. types 模块测试 (14 个)

#### T1.1: test_mcp_server_config_sse
```
测试目标: SSE 服务器配置序列化
输入: McpServerConfig { transport: Sse { url: "http://localhost:3001" } }
预期结果: JSON 包含 "Sse" 和 "localhost:3001"
状态: ✅ PASS
```

#### T1.2: test_mcp_server_config_stdio
```
测试目标: Stdio 服务器配置序列化
输入: McpServerConfig { transport: Stdio { command: "python", args: ["-m", "mcp_server"] } }
预期结果: JSON 包含 "Stdio" 和 "python"
状态: ✅ PASS
```

#### T1.3: test_mcp_resource
```
测试目标: Resource 序列化
输入: McpResource { uri: "forensics://cases", name: "Cases", ... }
预期结果: JSON 包含 "forensics://cases"
状态: ✅ PASS
```

#### T1.4: test_mcp_tool
```
测试目标: Tool 序列化
输入: McpTool { name: "search_files", ... }
预期结果: JSON 包含 "search_files"
状态: ✅ PASS
```

#### T1.5: test_mcp_config_default
```
测试目标: 默认配置
输入: McpConfig::default()
预期结果: servers/resources/tools 均为空
状态: ✅ PASS
```

#### T1.6: test_mcp_capabilities_default
```
测试目标: 默认能力
输入: McpCapabilities::default()
预期结果: resources/tools/prompts 均为 false
状态: ✅ PASS
```

#### T1.7: test_json_rpc_request
```
测试目标: JSON-RPC 请求格式
输入: JsonRpcRequest { method: "initialize", ... }
预期结果: JSON 包含 "jsonrpc":"2.0" 和 "method":"initialize"
状态: ✅ PASS
```

#### T1.8: test_mcp_server_status
```
测试目标: 服务器状态序列化
输入: McpServerStatus { connected: true, ... }
预期结果: JSON 包含 "connected":true
状态: ✅ PASS
```

#### T1.9: test_mcp_server_status_no_error
```
测试目标: 服务器状态（无错误）
输入: McpServerStatus { last_error: Some("Connection refused"), ... }
预期结果: last_error 正确存储
状态: ✅ PASS
```

#### T1.10: test_mcp_prompt
```
测试目标: Prompt 序列化
输入: McpPrompt { name: "analyze_timeline", arguments: [...] }
预期结果: JSON 包含 "analyze_timeline" 和参数列表
状态: ✅ PASS
```

#### T1.11: test_mcp_resource_no_optional
```
测试目标: Resource 可选字段处理
输入: McpResource { description: None, mime_type: None }
预期结果: JSON 包含 "description":null
状态: ✅ PASS
```

---

### 2. error 模块测试 (10 个)

#### T2.1: test_error_display_connection
```
测试目标: Connection 错误显示
输入: McpError::Connection("timeout")
预期结果: "Connection error: timeout"
状态: ✅ PASS
```

#### T2.2: test_error_display_transport
```
测试目标: Transport 错误显示
输入: McpError::Transport("SSE disconnected")
预期结果: "Transport error: SSE disconnected"
状态: ✅ PASS
```

#### T2.3: test_error_display_protocol
```
测试目标: Protocol 错误显示
输入: McpError::Protocol("invalid message")
预期结果: "Protocol error: invalid message"
状态: ✅ PASS
```

#### T2.4: test_error_display_timeout
```
测试目标: Timeout 错误显示
输入: McpError::Timeout
预期结果: "Connection timeout"
状态: ✅ PASS
```

#### T2.5: test_error_display_not_connected
```
测试目标: NotConnected 错误显示
输入: McpError::NotConnected
预期结果: "Not connected to server"
状态: ✅ PASS
```

#### T2.6: test_error_display_tool_not_found
```
测试目标: ToolNotFound 错误显示
输入: McpError::ToolNotFound("search")
预期结果: "Tool not found: search"
状态: ✅ PASS
```

#### T2.7: test_error_display_resource_not_found
```
测试目标: ResourceNotFound 错误显示
输入: McpError::ResourceNotFound("forensics://test")
预期结果: "Resource not found: forensics://test"
状态: ✅ PASS
```

#### T2.8: test_error_display_server
```
测试目标: Server 错误显示
输入: McpError::Server { code: -32600, message: "Invalid Request" }
预期结果: "Server error: -32600 - Invalid Request"
状态: ✅ PASS
```

#### T2.9: test_error_from_io
```
测试目标: IO 错误转换
输入: std::io::Error
预期结果: 转换为 McpError::Io
状态: ✅ PASS
```

#### T2.10: test_error_from_json
```
测试目标: JSON 错误转换
输入: serde_json::Error
预期结果: 转换为 McpError::Json
状态: ✅ PASS
```

---

### 3. transport::sse 模块测试 (2 个)

#### T3.1: test_sse_transport_new
```
测试目标: SSE 传输创建
输入: SseTransport::new("http://localhost:3001")
预期结果: url 正确，connected = false
状态: ✅ PASS
```

#### T3.2: test_request_id_increment
```
测试目标: 请求 ID 递增
输入: 连续两次 fetch_add
预期结果: 第二次值 = 第一次值 + 1
状态: ✅ PASS
```

---

### 4. client 模块测试 (9 个)

#### T4.1: test_client_new
```
测试目标: 客户端创建
输入: McpClient::new(config)
预期结果: connected = false, config.name = "Test Server"
状态: ✅ PASS
```

#### T4.2: test_client_not_connected
```
测试目标: 未连接状态检查
输入: 新创建的客户端
预期结果: is_connected() = false
状态: ✅ PASS
```

#### T4.3: test_client_not_connected_error
```
测试目标: 未连接时调用 list_resources
输入: 未连接的客户端
预期结果: 返回 McpError::NotConnected
状态: ✅ PASS
```

#### T4.4: test_client_list_tools_not_connected
```
测试目标: 未连接时调用 list_tools
输入: 未连接的客户端
预期结果: 返回 McpError::NotConnected
状态: ✅ PASS
```

#### T4.5: test_client_list_prompts_not_connected
```
测试目标: 未连接时调用 list_prompts
输入: 未连接的客户端
预期结果: 返回 McpError::NotConnected
状态: ✅ PASS
```

#### T4.6: test_client_call_tool_not_connected
```
测试目标: 未连接时调用 call_tool
输入: 未连接的客户端
预期结果: 返回 McpError::NotConnected
状态: ✅ PASS
```

#### T4.7: test_client_read_resource_not_connected
```
测试目标: 未连接时调用 read_resource
输入: 未连接的客户端
预期结果: 返回 McpError::NotConnected
状态: ✅ PASS
```

#### T4.8: test_client_config_sse
```
测试目标: SSE 配置客户端
输入: McpServerConfig { transport: Sse, auto_connect: true }
预期结果: config().auto_connect = true
状态: ✅ PASS
```

#### T4.9: test_client_config_stdio
```
测试目标: Stdio 配置客户端
输入: McpServerConfig { transport: Stdio, enabled: false }
预期结果: config().enabled = false
状态: ✅ PASS
```

---

## 🔗 集成测试用例 (12 个)

### I1: test_mcp_server_config_creation
```
测试目标: 服务器配置创建
输入: 完整的 McpServerConfig
预期结果: 所有字段正确存储
状态: ✅ PASS
```

### I2: test_mcp_config_serialization_roundtrip
```
测试目标: 配置序列化往返
输入: 包含多个服务器和资源的 McpConfig
预期结果: 序列化 -> 反序列化后数据一致
状态: ✅ PASS
```

### I3: test_mcp_server_status
```
测试目标: 服务器状态完整性
输入: 完整的 McpServerStatus
预期结果: 所有字段正确
状态: ✅ PASS
```

### I4: test_mcp_resource_list
```
测试目标: 资源列表
输入: 包含多个资源的 Vec<McpResource>
预期结果: 列表长度和内容正确
状态: ✅ PASS
```

### I5: test_mcp_tool_list
```
测试目标: 工具列表
输入: 包含多个工具的 Vec<McpTool>
预期结果: 列表长度和内容正确
状态: ✅ PASS
```

### I6: test_mcp_prompt_with_arguments
```
测试目标: 带参数的 Prompt
输入: McpPrompt 包含 required/optional 参数
预期结果: 参数列表正确，required 标志正确
状态: ✅ PASS
```

### I7: test_mcp_client_lifecycle
```
测试目标: 客户端生命周期
输入: 创建客户端
预期结果: 初始状态正确
状态: ✅ PASS
```

### I8: test_mcp_client_operations_when_not_connected
```
测试目标: 未连接时所有操作
输入: 未连接的客户端调用所有方法
预期结果: 所有方法返回 NotConnected 错误
状态: ✅ PASS
```

### I9: test_mcp_error_types
```
测试目标: 所有错误类型
输入: 10 种不同的 McpError
预期结果: 所有错误都有正确的 Display 实现
状态: ✅ PASS
```

### I10: test_json_rpc_request_format
```
测试目标: JSON-RPC 请求格式
输入: 包含 initialize 参数的请求
预期结果: JSON 格式正确，包含所有必要字段
状态: ✅ PASS
```

### I11: test_json_rpc_response_parsing
```
测试目标: JSON-RPC 成功响应解析
输入: 包含 result 的 JSON 字符串
预期结果: 正确解析为 JsonRpcResponse
状态: ✅ PASS
```

### I12: test_json_rpc_error_response_parsing
```
测试目标: JSON-RPC 错误响应解析
输入: 包含 error 的 JSON 字符串
预期结果: 正确解析 error 字段
状态: ✅ PASS
```

---

## 🖥️ Tauri 命令测试用例

### 配置命令

#### TC1: get_mcp_config
```
测试目标: 获取 MCP 配置
前置条件: 应用已启动
测试步骤: 调用 get_mcp_config
预期结果: 返回 McpConfigDto，servers 列表可能为空
状态: ⏳ 手动测试
```

#### TC2: save_mcp_config
```
测试目标: 保存 MCP 配置
前置条件: 无
测试步骤: 
1. 构造 McpConfigDto
2. 调用 save_mcp_config
3. 调用 get_mcp_config 验证
预期结果: 配置正确保存和加载
状态: ⏳ 手动测试
```

### 服务器管理命令

#### TC3: add_mcp_server
```
测试目标: 添加 MCP 服务器
前置条件: 无
测试步骤:
1. 构造 McpServerConfigDto
2. 调用 add_mcp_server
3. 调用 get_mcp_config 验证
预期结果: 服务器添加到列表
状态: ⏳ 手动测试
```

#### TC4: remove_mcp_server
```
测试目标: 删除 MCP 服务器
前置条件: 已有服务器
测试步骤:
1. 调用 remove_mcp_server
2. 调用 get_mcp_config 验证
预期结果: 服务器从列表移除
状态: ⏳ 手动测试
```

### 连接命令

#### TC5: connect_mcp_server
```
测试目标: 连接 MCP 服务器
前置条件: 已有服务器配置
测试步骤:
1. 调用 connect_mcp_server
2. 检查返回的状态
预期结果: connected = true
状态: ⏳ 手动测试
```

#### TC6: disconnect_mcp_server
```
测试目标: 断开 MCP 服务器
前置条件: 已连接的服务器
测试步骤:
1. 调用 disconnect_mcp_server
2. 检查状态
预期结果: connected = false
状态: ⏳ 手动测试
```

#### TC7: test_mcp_connection
```
测试目标: 测试连接
前置条件: 无
测试步骤:
1. 构造 McpTestConnectionRequest
2. 调用 test_mcp_connection
预期结果: 返回连接结果
状态: ⏳ 手动测试
```

### 资源/工具命令

#### TC8: list_mcp_resources
```
测试目标: 列出资源
前置条件: 已连接的服务器
测试步骤:
1. 调用 list_mcp_resources
预期结果: 返回资源列表
状态: ⏳ 手动测试
```

#### TC9: list_mcp_tools
```
测试目标: 列出工具
前置条件: 已连接的服务器
测试步骤:
1. 调用 list_mcp_tools
预期结果: 返回工具列表
状态: ⏳ 手动测试
```

#### TC10: call_mcp_tool
```
测试目标: 调用工具
前置条件: 已连接的服务器
测试步骤:
1. 构造 McpToolCallRequest
2. 调用 call_mcp_tool
预期结果: 返回调用结果
状态: ⏳ 手动测试
```

#### TC11: list_mcp_prompts
```
测试目标: 列出提示词
前置条件: 已连接的服务器
测试步骤:
1. 调用 list_mcp_prompts
预期结果: 返回提示词列表
状态: ⏳ 手动测试
```

#### TC12: get_mcp_prompt
```
测试目标: 获取提示词
前置条件: 已连接的服务器
测试步骤:
1. 调用 get_mcp_prompt
预期结果: 返回提示词内容
状态: ⏳ 手动测试
```

---

## 🎨 前端组件测试用例

### McpServerItem 组件

#### FC1: 渲染已连接服务器
```
测试目标: 已连接状态显示
输入: server.connected = true
预期结果: 显示绿色指示器，"已连接" 文本
状态: ⏳ 手动测试
```

#### FC2: 渲染未连接服务器
```
测试目标: 未连接状态显示
输入: server.connected = false
预期结果: 显示灰色指示器，"未连接" 文本
状态: ⏳ 手动测试
```

#### FC3: 渲染错误状态
```
测试目标: 错误状态显示
输入: server.lastError = "Connection refused"
预期结果: 显示红色指示器，"错误" 文本
状态: ⏳ 手动测试
```

#### FC4: 点击连接按钮
```
测试目标: 连接操作
输入: 点击连接按钮
预期结果: onConnect 回调被调用
状态: ⏳ 手动测试
```

#### FC5: 点击删除按钮
```
测试目标: 删除操作
输入: 点击删除按钮
预期结果: onRemove 回调被调用
状态: ⏳ 手动测试
```

### McpServerDialog 组件

#### FC6: 新建模式
```
测试目标: 新建对话框
输入: 无 server prop
预期结果: 表单为空
状态: ⏳ 手动测试
```

#### FC7: SSE 传输选择
```
测试目标: SSE 选项
输入: 选择 SSE 传输
预期结果: 显示 URL 输入框
状态: ⏳ 手动测试
```

#### FC8: Stdio 传输选择
```
测试目标: Stdio 选项
输入: 选择 Stdio 传输
预期结果: 显示命令和参数输入框
状态: ⏳ 手动测试
```

#### FC9: 表单验证
```
测试目标: 必填验证
输入: 空名称，点击添加
预期结果: 显示错误提示
状态: ⏳ 手动测试
```

#### FC10: 测试连接
```
测试目标: 测试连接功能
输入: 填写配置，点击测试连接
预期结果: 显示连接结果（成功/失败）
状态: ⏳ 手动测试
```

### McpResourceList 组件

#### FC11: 渲染资源列表
```
测试目标: 资源列表显示
输入: 3 个资源
预期结果: 显示 3 个资源项
状态: ⏳ 手动测试
```

#### FC12: 空列表显示
```
测试目标: 空状态
输入: 0 个资源
预期结果: 显示 "暂无资源"
状态: ⏳ 手动测试
```

#### FC13: 刷新按钮
```
测试目标: 刷新功能
输入: 点击刷新按钮
预期结果: refreshResources 被调用
状态: ⏳ 手动测试
```

### McpToolList 组件

#### FC14: 渲染工具列表
```
测试目标: 工具列表显示
输入: 5 个工具
预期结果: 显示 5 个工具项
状态: ⏳ 手动测试
```

#### FC15: 测试调用
```
测试目标: 工具测试调用
输入: 点击测试按钮
预期结果: 显示调用结果
状态: ⏳ 手动测试
```

### Settings 页面

#### FC16: MCP 区域显示
```
测试目标: MCP 折叠区域
输入: 打开设置页面
预期结果: 显示 "AI 助手 (MCP)" 区域
状态: ⏳ 手动测试
```

#### FC17: 折叠/展开
```
测试目标: 折叠功能
输入: 点击标题
预期结果: 内容显示/隐藏
状态: ⏳ 手动测试
```

#### FC18: 添加服务器
```
测试目标: 添加流程
输入: 点击 "添加服务器"
预期结果: 打开对话框
状态: ⏳ 手动测试
```

#### FC19: 连接状态显示
```
测试目标: 状态统计
输入: 有 2 个已连接服务器
预期结果: 显示 "2 个服务器已连接"
状态: ⏳ 手动测试
```

---

## 📊 测试覆盖率

### mcp-client 模块

| 模块 | 代码行数 | 测试行数 | 覆盖率估算 |
|------|----------|----------|------------|
| types.rs | 200 | 180 | ~90% |
| error.rs | 60 | 40 | ~67% |
| client.rs | 120 | 100 | ~83% |
| transport/sse.rs | 200 | 80 | ~40% |
| **总计** | **580** | **400** | **~69%** |

### 测试分布

| 测试类型 | 数量 | 百分比 |
|----------|------|--------|
| 类型测试 | 14 | 31% |
| 错误测试 | 10 | 22% |
| 客户端测试 | 9 | 20% |
| 传输测试 | 2 | 4% |
| 集成测试 | 12 | 27% |

---

## 🔧 测试运行命令

### 运行所有 mcp-client 测试
```bash
cargo test -p mcp-client
```

### 运行单元测试
```bash
cargo test -p mcp-client --lib
```

### 运行集成测试
```bash
cargo test -p mcp-client --test integration_test
```

### 运行特定测试
```bash
cargo test -p mcp-client test_mcp_server_config_sse
```

### 运行全量测试
```bash
cargo test --workspace
```

---

**文档版本**: v1.0  
**最后更新**: 2026-05-30
