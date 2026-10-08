# MCP 安全模型

## 1. 目标

MCP 在取证工具中天然敏感。本项目里的 MCP 必须是“最小权限、可审计、可解释”的受控扩展边界。

## 内置本机服务端

- 应用内置 Streamable HTTP MCP 服务，默认启用，只监听 `127.0.0.1:3001/mcp`；设置页可停止、重新启动、复制地址及逐项禁用工具。退出应用会停止服务并释放端口。
- 服务与外部 MCP 客户端配置独立，设置保存在应用配置目录的 `mcp-host.json`，使用临时文件与原子替换；配置解析失败不会启动监听。
- 当前提供 8 个工具：`forensics.get_current_case`、`forensics.list_data_sources`、`forensics.get_data_source`、`forensics.list_plugin_modules`、`forensics.get_plugin_module`、`forensics.list_files`、`forensics.get_file_metadata` 和 `forensics.read_file`。
- HTTP 与设置页调用复用同一工具目录和应用服务。每次读取绑定当前案件，原始证据和源数据库保持只读；摘要不返回主机源路径、内部存储路径、凭据或原始插件诊断，插件告警仅返回数量。文件内容工具按用户选择返回证据原始字节，可能包含证据中的敏感内容。
- 文件列表按 `dataSourceId` 查询根目录，或用源内目录的 `parentId` 继续浏览；默认每页 100 条，最多 200 条，包含隐藏、系统及已删除条目，分页由数据库执行。`get_file_metadata` 和 `read_file` 仅接受列表返回的全局 `fileId`，验证当前案件及数据源归属，不接受主机路径。
- `read_file` 复用证据预览读取器及当前 BitLocker 解锁状态，单次最多 64 KiB。`offset`/`nextOffset` 是字节偏移；返回 `bytesRead`、`eof` 和该块的 `chunkSha256`。`encoding=auto` 对有效且不含 NUL 的 UTF-8 块返回文本，其他字节以 Base64 返回；显式 `utf8` 不做有损替换，多字节字符被分块截断时应改用 `auto`/`base64`。哈希只针对当前块，不代表整份文件已校验。读取结束或失败均释放临时预览句柄。
- NTFS WOF 文件提供器支持 XPRESS 4K、8K、16K 及 LZX 32K；按块索引定位 `WofCompressedData`，只解码读取范围涉及的块，流式读取最多缓存一个解压块。支持跨块读取、seek、原样存储块及 4 GiB 以上文件的 64 位块索引。外置 WIM 提供器仍返回不支持，需要另行关联 WIM 数据源。损坏的重解析点、块索引、Huffman 表或越界匹配返回错误，不回退到稀疏占位流。

### 文件工具使用与回归

先调用 `forensics.list_data_sources` 获取数据源 ID，再调用 `forensics.list_files`：

```json
{"dataSourceId":"数据源ID","limit":100}
```

用返回目录的 `id` 作为 `parentId` 浏览子目录；用文件的 `id` 调用 `forensics.get_file_metadata` 或 `forensics.read_file`：

```json
{"fileId":"ds:数据源ID:文件局部ID","offset":0,"length":65536,"encoding":"auto"}
```

读取后用响应的 `nextOffset` 继续，直到 `eof=true`。每块分别依据响应的 `encoding` 解码，按字节拼接；不要把 Base64 字符串直接拼接后再解码。

2026-10-07 回归验证：逻辑目录中的 UTF-8、空文件及 130,001 字节二进制；真实 HTTP 工具发现、内容读取、审计、禁用策略和切换案件；已导入 E01 的 `$MFT`（262,144 字节，4 块）和 `Windows/win.ini`（92 字节，1 块），以及带 WOF 标记的 `ntdll.dll` 明确拒绝。E01 读取校验独立文件头预期、字节总数、块哈希、EOF、句柄释放和原镜像大小/修改时间；文件数据库和镜像均只读打开。

2026-10-08 WOF 回归验证：真实 E01 的 XPRESS8K `ntdll.dll` 恢复 2,174,872 字节、34 个 MCP 数据块，SHA-256 为 `abb30adf05bd71ae8283d8a44e55268de3c408421ec059c626b92b9168adc0f9`，与 Windows Cabinet 原生解码器逐块解码得到的完整文件哈希一致。LZX 的 verbatim、aligned、raw 与 E8 变换样本也使用微软公开参考解码器交叉验证；正式程序和自动回归测试均使用 Rust，无 Go 运行依赖。

私有 E01 测试位于 `crates/app-services/tests/mcp_host_real_file.rs`，默认忽略，运行需显式提供 `FORENSICS_MCP_CASE_ROOT`、`FORENSICS_MCP_FILE_ID` 和 `FORENSICS_MCP_EXPECTED_PREFIX_HEX`；WOF 内容测试另需 `FORENSICS_MCP_WOF_FILE_ID` 和来自独立解码器的 `FORENSICS_MCP_WOF_EXPECTED_SHA256`。测试不包含工作站路径回退。独立验证需要压缩原流时，可显式设置 `FORENSICS_WOF_ORACLE_EXPORT_DIR`，运行 `wof_real_oracle_export` 的忽略测试；导出使用独立临时子目录并保持源镜像只读。

WOF 文件将压缩内容保存在替代数据流或外部 WIM 中，由 Windows 过滤驱动提供逻辑内容；参见 [微软对 WofCompressedData 的说明](https://devblogs.microsoft.com/oldnewthing/20190618-00/?p=102597)及[四种文件压缩算法](https://learn.microsoft.com/en-us/windows/win32/api/wofapi/ns-wofapi-wof_file_compression_info_v1)。LZX 格式交叉验证依据 [Microsoft go-winio 的 WIM-LZX 实现](https://github.com/Microsoft/go-winio/tree/v0.6.2/wim/lzx)，许可声明保留于 `crates/fs-ntfs/NOTICE`。
- 工具发现始终返回完整目录，被禁用工具的实际调用在后端拒绝。成功和失败调用在其绑定案件中记录工具名和结果状态，不记录参数或结果正文。
- 校验 Host，拒绝携带浏览器 Origin 的请求，不提供 CORS；每个请求最多 64 KiB，最多两个同时执行的案件查询，结果上限 1 MiB。工具能力限定为只读查询，不提供任意 IPC、SQL、路径读取或插件执行入口。

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
