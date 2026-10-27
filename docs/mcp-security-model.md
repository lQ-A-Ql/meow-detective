# MCP 安全模型

## 服务边界

应用通过内置 Rust `rmcp` 服务向外部调查客户端提供取证能力。Streamable HTTP 地址为 `http://127.0.0.1:3001/mcp`，桌面启动时初始化，退出时停止。设置页面管理服务启停、地址复制、工具发现、逐项开关和工具调试。

应用配置目录中的 `mcp-host.json` 独立保存 `enabled`、`disabledTools` 和 `policyVersion`。默认启用服务，默认禁用 `forensics.extract_file`。缺少配置时使用默认值；读取失败、格式无效或超过 32 KiB 时停止服务。保存采用同目录中间文件、同步写入与原子替换。旧版配置升级会加入文件提取禁用项。

当前生产代码包括七个桌面命令：`get_mcp_host_status`、`set_mcp_host_settings`、`list_mcp_host_tools`、`call_mcp_host_tool`、`list_mcp_host_clients`、`create_mcp_host_client`、`revoke_mcp_host_client`。HTTP 与桌面调试共享目录、工具开关检查和执行器。

## 客户端身份

设置页面为当前打开的案件创建客户端授权。凭据由操作系统随机源生成 256 位随机数据，使用 URL-safe Base64 表达，仅在创建时返回。有效期为 1–720 小时，最多保存 128 个授权。授权记录绑定案件 ID 与本次打开案件的随机会话 ID；重新打开同一案件会使旧授权失效。

客户端在每次 HTTP 请求的 `Authorization` 中提供 Bearer 凭据。服务通过 `headers` 库读取请求头，使用 SHA-256 摘要和 [subtle 恒定时间比较](https://docs.rs/subtle/latest/subtle/trait.ConstantTimeEq.html)验证身份。通过验证后移除原始认证请求头，再将身份传递给协议处理器。缺少、无效、过期或撤销的凭据返回 401；案件会话不匹配返回 403。

应用配置目录中的 `mcp-access.json` 保存客户端元数据与摘要。写入前限制文件为当前用户可访问，再通过同目录文件同步写入与原子替换发布。配置读取或校验失败会停止监听。列表接口只返回客户端元数据。前端创建凭据时使用局部状态，关闭凭据窗口后清除该状态，凭据不进入查询缓存或持久化设置。

授权默认允许案件查询，可另外允许文件内容读取。外部客户端无法发现或执行文件提取工具。撤销授权后，新请求被拒绝，等待中的请求和运行中的案件 SQL 查询收到取消信号。

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

桌面工具发现返回完整目录；HTTP 工具发现按授权过滤。每个工具声明 `readOnlyHint` 和 `destructiveHint`；`extract_file` 分别为 `false`、`true`。工具提示属于元数据，实际权限由后端检查。

Linux 服务和 Docker 信息目前通过 `get_linux_artifact_summary` 的 `systemConfigs` 返回，使用 `configKind` 区分服务、容器与镜像。它们描述证据中的配置和状态记录。服务与容器的实际控制操作未进入目录。

## 案件和证据访问

请求入队前记录案件 ID 和会话 ID；取得执行许可后再次验证，在查询和审计完成前持有生命周期访问许可。排队期间切换案件会使请求失效。源文件 ID 使用包含数据源身份的全局 ID；文件工具验证当前案件、数据源和文件归属。目录遍历与证据读取复用应用服务、源数据库及证据读取器。

查询连接以 SQLite 只读模式打开。原始证据、源数据库保持只读；案件数据库接收审计记录。文件提取另行通过案件写入连接记录提取过程。摘要清理主机源路径、内部存储路径、凭据及原始插件诊断；文件内容工具按请求返回证据字节。

| 边界 | 当前限制 |
|---|---|
| HTTP Host | `127.0.0.1`、`localhost` |
| 浏览器来源 | 使用 SDK Origin 验证，当前拒绝携带 Origin 的请求，不开放 CORS |
| 请求大小 | 64 KiB |
| 并行执行 | 两个调用；等待总数最多 16 个，外部主体最多四个等待调用；桌面共用总容量 |
| 等待时间 | 最多五秒；容量不足立即拒绝 |
| 执行截止时间 | 普通查询 30 秒，文件读取 60 秒，超时触发协作取消 |
| 拒绝事件审计 | 两个写入许可、16 个等待许可，等待最多一秒；容量不足返回 `MCP_AUDIT_BUSY` 并记录应用审计 |
| 工具数据 | 业务数据 JSON 最多 1 MiB，流式计数检查；协议封装另计 |
| 文件目录 | 默认每页 100 条，最多 200 条 |
| 文件读取 | 每块最多 64 KiB |
| 关联快照 | 每类默认 50 条，最多 100 条，提供继续读取信息 |

查询接口不接收任意 SQL、桌面命令名称或主机文件读取路径。平台能力不足时返回错误。

## 文件内容与提取

调用 `list_data_sources` 获取数据源 ID，然后通过 `list_files` 获取目录和文件 ID。目录 ID 作为 `parentId` 继续浏览。`read_file` 接收 `fileId`、字节 `offset`、`length` 和 `encoding`。

读取结果包括 `nextOffset`、`bytesRead`、`eof` 和当前块的 `chunkSha256`。`auto` 对有效且不含 NUL 的 UTF-8 块返回文本，其他内容返回 Base64。客户端逐块解码并按字节连接，继续读取直至 `eof=true`。显式 `utf8` 遇到无效 UTF-8 返回错误。块哈希仅验证该块内容。

内容读取复用预览读取器和已验证的 BitLocker 运行状态，结束或失败均释放预览句柄。NTFS WOF 支持 XPRESS 4K/8K/16K 与 LZX 32K 的范围读取；外置 WIM 提供器返回不支持。

桌面调试中的 `extract_file` 需要工具已启用，将当前案件普通文件写入明确的外部绝对路径。路径必须位于案件工作区与注册证据源目录之外。默认 `overwrite=false`；验证目标存在性、符号链接、Windows ADS 和设备路径，记录输出大小与 SHA-256。原始证据始终只读。

执行器连接 SDK 取消信号、截止时间、服务停止与授权撤销信号。案件 SQL 连接安装进度检查和中断处理。源数据库和案件关系数据库的统一只读连接工厂接入当前读取作用域，SQL 执行与锁等待都检查取消状态。作用域属于执行线程，结束时恢复前一个作用域；连接回调读取当前作用域，复用连接不会保留旧请求的取消状态。

同步证据读取正在执行时，取消状态在读取结束后检查；实际执行结束前持续持有并行许可。底层证据 I/O 的全程协作取消仍需继续接入。

工具失败继续保留文本 `error`，并通过 `errorDetails` 返回 `ApiErrorDto` 的错误码、分类、恢复建议和脱敏详情。HTTP 工具失败声明 `isError=true`，在 `structuredContent.error` 中返回相同错误信息。取消与执行超时分别使用 `cancelled`、`timeout` 分类。工具成功返回明确的结构化数据。

## 审计

案件执行阶段会记录工具名称和成功状态，文件提取另写提取审计。审计不保存工具参数、令牌、凭据或结果正文。数据库中的既有审计记录使用字符串读取，能够继续读取历史操作名称。

HTTP 审计调用者为客户端 ID，桌面调用者为 `desktop`。工具执行、未知工具拒绝、排队容量不足、排队超时和取消均记录审计；记录包括请求 ID、入口、结果和错误码。未知工具名仅记录为 `unknown`，参数与响应正文不进入审计。

案件会话仍匹配时，事件写入案件审计；案件未打开、会话已失效、HTTP 鉴权或协议拒绝等事件记录应用审计日志。执行阶段持续持有案件访问许可直至案件审计完成。案件审计写入失败会使调用返回失败。拒绝事件使用独立的有限审计容量，容量不足会返回可重试错误并记录应用审计日志。

客户端创建与撤销记录 `mcp.access.grant`、`mcp.access.revoke` 的请求和完成事件，正文只包含操作阶段。案件已打开时写入案件审计，同时写入应用日志；案件关闭时的撤销保留应用日志。凭据和摘要不进入审计。

## 当前能力限制

| 项目 | 当前行为 |
|---|---|
| 客户端身份 | 本机预配 Bearer 凭据、过期检查和撤销；未声明 OAuth 授权发现 |
| 授权 | 案件会话绑定、案件查询与文件内容读取；外部导出目录授权待实现 |
| 批量开关 | 仅修改查询工具，保留写入工具的独立状态 |
| 请求取消 | 队列、案件 SQL、源数据库 SQL 与锁等待接入取消；底层证据 I/O 待继续接入 |
| 队列容量 | 两个执行、16 个等待；每个外部客户端最多四个等待调用 |
| 输出定义 | 输入 JSON Schema、结构化数据与分类错误；工具输出 JSON Schema 待实现 |
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
