# MCP 配置指南

**Forensics Workbench MCP 集成使用说明**

---

## 📋 概述

MCP (Model Context Protocol) 允许 Forensics Workbench 连接到 AI 助手，让 AI 可以：
- 查询案件数据
- 搜索文件内容
- 分析取证工件
- 生成分析报告

---

## 🚀 快速开始

### 步骤 1: 打开设置

1. 启动 Forensics Workbench
2. 点击左侧导航栏的 **设置** 图标
3. 找到 **AI 助手 (MCP)** 区域
4. 点击展开

### 步骤 2: 添加 MCP 服务器

1. 点击 **+ 添加服务器** 按钮
2. 填写服务器信息：
   - **名称**: 给服务器起一个易识别的名称
   - **传输类型**: 选择 HTTP/SSE 或 Stdio
   - **URL/命令**: 根据传输类型填写
3. 点击 **测试连接** 验证配置
4. 点击 **添加** 保存

### 步骤 3: 连接服务器

1. 在服务器列表中找到刚添加的服务器
2. 点击 **连接** 按钮 (WiFi 图标)
3. 等待连接成功
4. 查看服务器暴露的 Resources 和 Tools

---

## 🔧 配置示例

### Claude Desktop

```
名称: Claude Desktop
传输类型: HTTP/SSE
URL: http://localhost:3001
```

### 自定义 MCP Server (Python)

```
名称: Forensics AI
传输类型: Stdio
命令: python -m forensics_mcp
参数: --port 3001 --verbose
```

### Ollama 本地模型

```
名称: Local Ollama
传输类型: HTTP/SSE
URL: http://localhost:11434
```

---

## 📦 MCP 组件说明

### Resources (资源)

Resources 是 AI 可以查询的数据源。Forensics Workbench 暴露以下资源：

| 资源 | URI | 说明 |
|------|-----|------|
| 案件列表 | `forensics://cases` | 所有案件 |
| 文件树 | `forensics://case/{id}/files` | 案件文件树 |
| 时间线 | `forensics://case/{id}/timeline` | 时间线事件 |
| 工件 | `forensics://case/{id}/artifacts` | 取证工件 |

### Tools (工具)

Tools 是 AI 可以调用的操作。Forensics Workbench 提供以下工具：

| 工具 | 说明 | 参数 |
|------|------|------|
| `search_files` | 搜索文件内容 | query: 搜索关键词 |
| `get_file_content` | 获取文件内容 | file_id, format |
| `analyze_artifact` | 分析取证工件 | artifact_id |
| `generate_report` | 生成报告 | format |

### Prompts (提示词)

Prompts 是预设的分析模板：

| Prompt | 说明 |
|--------|------|
| `analyze_timeline` | 分析时间线异常 |
| `find_evidence` | 查找证据 |
| `summarize_case` | 生成案件摘要 |

---

## ❓ 常见问题

### Q: 连接失败怎么办？

A: 检查以下几点：
1. MCP 服务器是否正在运行
2. URL/端口是否正确
3. 防火墙是否允许连接
4. 服务器是否支持 MCP 协议

### Q: 如何查看连接状态？

A: 在设置页面的 MCP 区域，每个服务器旁边有状态指示器：
- 🟢 绿色: 已连接
- 🔴 红色: 连接错误
- ⚪ 灰色: 未连接

### Q: 配置保存在哪里？

A: MCP 配置保存在：
- Windows: `%APPDATA%\forensics\mcp-config.json`
- macOS: `~/Library/Application Support/forensics/mcp-config.json`
- Linux: `~/.config/forensics/mcp-config.json`

### Q: 可以同时连接多个服务器吗？

A: 是的，可以添加和连接多个 MCP 服务器。每个服务器独立管理。

---

## 🔐 安全提示

1. **仅连接可信服务器**: 只连接到您信任的 MCP 服务器
2. **检查 Resources**: 连接后查看服务器暴露的资源，确保没有敏感数据泄露
3. **限制 Tools**: 可以禁用不需要的工具
4. **审计日志**: 所有 MCP 操作都会记录在审计日志中

---

## 📚 更多资源

- [MCP 协议规范](https://modelcontextprotocol.io)
- [Forensics Workbench 文档](./README.md)
- [API 参考](./mcp-api-reference.md)

---

**文档版本**: v1.0  
**最后更新**: 2026-05-30
