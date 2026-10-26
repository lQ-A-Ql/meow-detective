# MCP 安全模型

## 服务边界

应用通过内置 Rust `rmcp` 服务向外部调查客户端提供取证能力。Streamable HTTP 地址为 `http://127.0.0.1:3001/mcp`，桌面启动时初始化，退出时停止。设置页面管理服务启停、地址复制、工具发现、逐项开关和工具调试。

应用配置目录中的 `mcp-host.json` 独立保存 `enabled`、`disabledTools` 和 `policyVersion`。默认启用服务，默认禁用 `forensics.extract_file`。缺少配置时使用默认值；读取失败、格式无效或超过 32 KiB 时停止服务。保存采用同目录中间文件、同步写入与原子替换。旧版配置升级会加入文件提取禁用项。

当前生产代码包括四个桌面命令：`get_mcp_host_status`、`set_mcp_host_settings`、`list_mcp_host_tools`、`call_mcp_host_tool`。HTTP 与桌面调试共享目录、权限检查和执行器。

## 工具目录

当前提供 29 个 `forensics.` 工具，其中 28 个查询工具和一个文件提取工具。

| 功能 | 工具名称，省略 `forensics.` |
|---|---|
| 案件与数据源 | `get_current_case`、`list_data_sources`、`get_data_source` |
| 文件目录与内容 | `list_files`、`get_file_metadata`、`read_file`、`extract_file` |
| 文件分类 | `get_evidence_classification_summary`、`get_file_classification_board` |
| Windows 分析 | `get_analysis_system_info`、`get_registry_summary`、`get_registry_structured_summary`、`get_browser_history_summary`、`get_email_summary`、`get_event_log_events`、`get_analysis_report` |
| Linux 分析 | `get_linux_artifact_summary` |
| Android 分析 | `get_android_device_info`、`get_android_package_summary` |
| 插件结果 | `list_plugin_modules`、`get_plugin_module`、`get_plugin_family_entries` |
| 案件聚合 | `get_case_overview`、`get_correlation_snapshot` |
| 时间线 | `get_timeline_events`、`get_timeline_event`、`get_timeline_facets` |
| 搜索与索引 | `search_files`、`get_content_index_coverage` |

工具发现返回完整目录。每个工具声明 `readOnlyHint` 和 `destructiveHint`；`extract_file` 分别为 `false`、`true`。工具提示属于元数据，实际权限由后端检查。

Linux 服务和 Docker 信息目前通过 `get_linux_artifact_summary` 的 `systemConfigs` 返回，使用 `configKind` 区分服务、容器与镜像。它们描述证据中的配置和状态记录。服务与容器的实际控制操作未进入目录。

## 案件和证据访问

执行器取得当前案件快照，在查询和审计完成前持有生命周期访问许可。源文件 ID 使用包含数据源身份的全局 ID；文件工具验证当前案件、数据源和文件归属。目录遍历与证据读取复用应用服务、源数据库及证据读取器。

查询连接以 SQLite 只读模式打开。原始证据、源数据库保持只读；案件数据库接收审计记录。文件提取另行通过案件写入连接记录提取过程。摘要清理主机源路径、内部存储路径、凭据及原始插件诊断；文件内容工具按请求返回证据字节。

| 边界 | 当前限制 |
|---|---|
| HTTP Host | `127.0.0.1`、`localhost` |
| 浏览器来源 | 使用 SDK Origin 验证，当前拒绝携带 Origin 的请求，不开放 CORS |
| 请求大小 | 64 KiB |
| 并行执行 | 两个调用；其余等待许可，排队超时 300 秒 |
| 工具结果 | JSON 序列化后最多 1 MiB |
| 文件目录 | 默认每页 100 条，最多 200 条 |
| 文件读取 | 每块最多 64 KiB |
| 关联快照 | 每类默认 50 条，最多 100 条，提供继续读取信息 |

查询接口不接收任意 SQL、桌面命令名称或主机文件读取路径。平台能力不足时返回错误。

## 文件内容与提取

调用 `list_data_sources` 获取数据源 ID，然后通过 `list_files` 获取目录和文件 ID。目录 ID 作为 `parentId` 继续浏览。`read_file` 接收 `fileId`、字节 `offset`、`length` 和 `encoding`。

读取结果包括 `nextOffset`、`bytesRead`、`eof` 和当前块的 `chunkSha256`。`auto` 对有效且不含 NUL 的 UTF-8 块返回文本，其他内容返回 Base64。客户端逐块解码并按字节连接，继续读取直至 `eof=true`。显式 `utf8` 遇到无效 UTF-8 返回错误。块哈希仅验证该块内容。

内容读取复用预览读取器和已验证的 BitLocker 运行状态，结束或失败均释放预览句柄。NTFS WOF 支持 XPRESS 4K/8K/16K 与 LZX 32K 的范围读取；外置 WIM 提供器返回不支持。

`extract_file` 需要工具已启用，将当前案件普通文件写入明确的外部绝对路径。路径必须位于案件工作区与注册证据源目录之外。默认 `overwrite=false`；验证目标存在性、符号链接、Windows ADS 和设备路径，记录输出大小与 SHA-256。原始证据始终只读。

## 审计

案件执行阶段会记录工具名称和成功状态，文件提取另写提取审计。审计不保存工具参数、令牌、凭据或结果正文。数据库中的既有审计记录使用字符串读取，能够继续读取历史操作名称。

当前审计调用者字段为 `mcp-client`，表示请求取证工具的调用方。HTTP 请求和桌面调试暂时共用该调用者标识。未知工具、案件未打开、排队失败以及执行器提前返回的部分错误尚未形成统一审计事件。

## 当前能力限制

| 项目 | 当前行为 |
|---|---|
| 客户端身份 | 本机监听与请求来源检查；未提供逐客户端身份认证 |
| 授权 | 服务开关与全局工具禁用列表；未提供案件绑定的客户端权限 |
| 批量启用 | 清空禁用列表，包含文件提取工具 |
| 请求取消 | 队列超时；执行阶段尚未接入请求取消与统一截止时间 |
| 队列容量 | 执行并行数受限；等待调用数量没有独立容量限制 |
| 输出定义 | 输入 JSON Schema、结构化结果；未声明工具输出 JSON Schema |
| 协议资源 | 当前仅声明工具能力；Resources、Prompts 与协议任务未提供 |
| 长任务 | 尚未暴露分析、索引建立、报告导出任务的查询和取消接口 |

这些项目属于服务端后续设计范围。对应的权限与验收条件需要在增加接口之前确定。

## 验证入口

- `apps/desktop/src-tauri/tests/unit/mcp_host/`：真实 HTTP 初始化、工具发现、内容读取、权限、案件切换、请求来源和设置持久化。
- `crates/app-services/tests/mcp_host_files.rs`、`mcp_host_queries.rs`：应用服务读取、查询边界及结果。
- `crates/transport/tests/unit/dto/mcp_host.rs`：字段序列化、默认权限、参数约束及字节块结果。
- `frontend/tests/features/mcp/components/`：本机服务和工具操作界面。
- `scripts/check-stage5-regression-guard.ps1`：HTTP 安全设置、共享执行器及 DTO 边界。

私有 E01 内容测试位于 `crates/app-services/tests/mcp_host_real_file.rs`，默认忽略，需显式提供 `FORENSICS_MCP_CASE_ROOT`、`FORENSICS_MCP_FILE_ID` 和 `FORENSICS_MCP_EXPECTED_PREFIX_HEX`。WOF 校验还需要 `FORENSICS_MCP_WOF_FILE_ID` 与独立来源的 `FORENSICS_MCP_WOF_EXPECTED_SHA256`。
