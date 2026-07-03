import fs from 'fs';

const extractData = JSON.parse(fs.readFileSync('D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-2.json','utf8'));
const batches = JSON.parse(fs.readFileSync('D:/process/forensic/.understand-anything/intermediate/batches.json','utf8'));
const batch = batches.batches.find(b=>b.batchIndex===2);
const importData = batch.batchImportData;

const nodes = [];
const edges = [];

function addNode(node) { nodes.push(node); }
function addEdge(source, target, type, weight) {
  if (source === target) return;
  edges.push({ source, target, type, direction: 'forward', weight });
}

function fnLen(f) { return f.endLine - f.startLine; }

// ========== FILE DEFINITIONS ==========
const fileDefs = [
  ['apps/desktop/src-tauri/src/cache_invalidation.rs','file','cache_invalidation.rs','管理案例缓存失效逻辑，监听数据源导入事件并清理相关的预览缓存（文本、图像、媒体、树结构），防止案例切换后脏缓存影响数据正确性。',['cache-invalidation','event-handler','state-management'],'simple','通过 Tauri 事件监听机制实现缓存失效，使用 AppHandle 全局状态访问。'],
  ['apps/desktop/src-tauri/src/commands/analysis_commands.rs','file','analysis_commands.rs','证据分析命令集，提供系统信息获取、文件分类、分析提取（注册表/浏览器/邮件/EVTX/Linux 工件）、治理快照和关联分析功能，是分析模块的 Tauri IPC 入口。',['api-handler','analysis','command','ipc','forensics'],'complex','分析命令涵盖 15 个公开接口，从样本量解析到多种证据类型提取，均为无状态委托给 app_services 层。'],
  ['apps/desktop/src-tauri/src/commands/artifact_commands.rs','file','artifact_commands.rs','工件查询命令集，提供工件族列表、工件行数据、工件计数和按 ID 检索工件的 Tauri 命令接口。',['api-handler','command','artifacts','ipc'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/batch_commands.rs','file','batch_commands.rs','批量作业命令集，管理批量导入计划（创建、启动、暂停、恢复、取消）与作业状态查询，支持大规模证据数据的增量导入编排。',['api-handler','command','job-management','batch-processing','ipc'],'complex','采用公开薄包装函数 + 内部 _impl 实现函数的双模式，公开函数负责参数校验与状态获取，_impl 函数承载业务逻辑。'],
  ['apps/desktop/src-tauri/src/commands/benchmarks.rs','file','benchmarks.rs','基准测试模块（仅 test cfg），生成可解析的基准时序数据（如搜索性能、时间线性能），供 scripts/run-benchmark.ps1 采集分析。',['test','benchmark','performance'],'complex','使用 #[cfg(test)] 条件编译确保基准代码不进入发布构建；输出 [BENCH-OUTPUT] JSON 格式供外部脚本解析。'],
  ['apps/desktop/src-tauri/src/commands/case_commands.rs','file','case_commands.rs','案件生命周期管理命令集，涵盖创建、打开、关闭、删除案例及数据源，以及案例指标、最近对象、最近案例列表和审计日志记录。',['api-handler','command','case-management','ipc'],'complex','782 行，22 个函数，为模块内命令数最多的文件之一；同时管理案例数据库初始化和最近案例持久化（JSON 文件）。'],
  ['apps/desktop/src-tauri/src/commands/command_support.rs','file','command_support.rs','命令层共享工具模块，提供活动案例快照提取、案例连接获取、当前案例 ID 读取和审计日志写入等跨命令公共服务。',['utility','state-management','audit'],'moderate','ActiveCaseSnapshot 结构体封装案例上下文，供各命令模块统一使用。'],
  ['apps/desktop/src-tauri/src/commands/file_commands.rs','file','file_commands.rs','文件浏览与预览命令集，提供文件树、文件行、跳转上下文、文件句柄、十六进制/文本/图像/媒体预览、范围读取和文件导出等完整文件操作接口。',['api-handler','command','file-browser','preview','ipc'],'complex','1384 行，25 个函数，为项目中最长的单文件命令模块；支持证据文件的安全预览与只读导出。'],
  ['apps/desktop/src-tauri/src/commands/graph_commands.rs','file','graph_commands.rs','知识图谱查询命令集，提供图谱快照、查询、节点邻域和溯源链查询接口。',['api-handler','command','knowledge-graph','ipc'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/import/background_job.rs','file','background_job.rs','后台导入作业执行器，封装异步导入作业的线程启动、进度追踪、分析触发和取消信号监听逻辑。',['import','job-management','async'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/import/cancellation.rs','file','cancellation.rs','导入取消机制实现，提供 cancel_import 命令和取消状态 DTO 转换工具。',['import','cancellation','command'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/import/mod.rs','file','mod.rs','导入命令模块的 Barrel 文件，重新导出 background_job、cancellation、pipeline 和 schedule 四个子模块。',['barrel','import','module'],'simple','Rust 模块 barrel 模式，6 行纯模块声明。'],
  ['apps/desktop/src-tauri/src/commands/import/pipeline.rs','file','pipeline.rs','导入管道命令入口，提供 import_data_source 和 cancel_import 两个顶层 Tauri 命令。',['api-handler','command','import','pipeline'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/import/schedule.rs','file','schedule.rs','导入调度模块，负责加载导入设置、构建导入配置，并调度后台导入作业（含分析模式选择与配置校验）。',['import','scheduling','configuration'],'moderate','6 个函数组成完整的导入调度链路：设置加载 -> 配置构建 -> 后台调度 -> 错误转换。'],
  ['apps/desktop/src-tauri/src/commands/job_commands.rs','file','job_commands.rs','作业状态查询命令集，提供作业快照、警告列表和追踪条目查询接口，支持前端作业监控面板。',['api-handler','command','job-monitoring','ipc'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/mcp_commands.rs','file','mcp_commands.rs','MCP（Model Context Protocol）客户端命令集，管理 MCP 服务器配置（增删改查）、连接/断开、资源/工具/提示列表和工具调用，是 AI 集成功能的 IPC 入口。',['api-handler','command','mcp','ai-integration','ipc'],'complex','686 行，29 个函数；包含完整的 DTO <-> 领域模型双向转换函数族，实现 SSE 和 Stdio 两种传输协议支持。'],
  ['apps/desktop/src-tauri/src/commands/mod.rs','file','mod.rs','Commands 模块的 Barrel 文件，声明并重新导出全部 18 个命令子模块。',['barrel','command','module'],'simple','Rust 模块 barrel 模式，18 行纯模块声明。'],
  ['apps/desktop/src-tauri/src/commands/notebook_commands.rs','file','notebook_commands.rs','调查笔记本命令集，提供笔记条目 CRUD、调查线程、证据引用和调查步骤列表等调查文档功能。',['api-handler','command','notebook','investigation','ipc'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/report_commands.rs','file','report_commands.rs','报告导出命令集，支持 HTML、CSV、CSV 关联分析和 JSON 四种格式的报告生成与导出。',['api-handler','command','report','export','ipc'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/rule_pack_commands.rs','file','rule_pack_commands.rs','规则包管理命令集，提供规则包加载、验证和已加载列表查询功能。',['api-handler','command','rules','validation'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/search_commands.rs','file','search_commands.rs','搜索命令集，提供全文本文件搜索功能，支持分页和过滤参数。',['api-handler','command','search','ipc'],'moderate','查询委托给 app_services::search_service，命令层仅负责参数转换与状态获取。'],
  ['apps/desktop/src-tauri/src/commands/settings_commands.rs','file','settings_commands.rs','应用设置命令集，提供读取、保存和加载应用设置的持久化接口。',['api-handler','command','settings','configuration','ipc'],'moderate',null],
  ['apps/desktop/src-tauri/src/commands/timeline_commands.rs','file','timeline_commands.rs','时间线命令集，提供时间线事件分页查询和按 ID 检索功能。',['api-handler','command','timeline','ipc'],'moderate',null],
  ['apps/desktop/src-tauri/src/events/event_bridge.rs','file','event_bridge.rs','Tauri 事件桥接层，封装所有应用事件（案件开关、作业生命周期、数据源导入、工件更新、时间线、搜索索引、分区进度等）的标准发射接口。',['event-handler','ipc','bridge','pubsub'],'complex','使用统一的事件信封（envelope）模式包装所有事件负载，确保前后端事件格式一致性。'],
  ['apps/desktop/src-tauri/src/events/mod.rs','file','mod.rs','Events 模块的 Barrel 文件，重新导出 event_bridge 子模块。',['barrel','module'],'simple','单行模块声明文件。'],
  ['apps/desktop/src-tauri/src/lib.rs','file','lib.rs','Tauri 应用主入口，注册所有命令、事件、媒体协议、缓存失效、平台安全策略并启动应用运行循环。',['entry-point','application','bootstrap'],'complex','286 行，集中式命令注册（invoke_handler）和插件初始化（dialog, shell, fs），是桌面应用的后端启动枢纽。'],
  ['apps/desktop/src-tauri/src/media_protocol.rs','file','media_protocol.rs','自定义 evidence-media:// 协议处理器，提供媒体文件的安全流式预览（支持 Range 请求和部分内容传输），防止证据文件通过文件系统路径直接访问。',['media','protocol','security','preview'],'complex','787 行，12 个函数；实现 HTTP Range 请求解析和 Content-Range 响应构建，支持大型媒体文件的按需流式传输。'],
  ['apps/desktop/src-tauri/src/platform_security.rs','file','platform_security.rs','平台安全策略模块，通过 ACL 限制证据文件和案例目录仅当前用户可访问，包含 Windows 和 Unix 双平台实现。',['security','platform','access-control'],'moderate','使用 #[cfg(windows)] 和 #[cfg(unix)] 条件编译提供平台特定的文件权限限制实现。'],
  ['apps/desktop/src-tauri/src/state/app_state.rs','file','app_state.rs','全局应用状态管理，持有活动案例、任务管理器、MCP 客户端集合、数据库连接池、设置路径和运行时缓存。',['state-management','singleton','database','mcp'],'complex','AppState 是 Tauri 托管状态的根对象，364 行，16 个方法；通过 Arc<Mutex<>> 实现线程安全的内部可变性。'],
  ['apps/desktop/src-tauri/src/state/mod.rs','file','mod.rs','State 模块的 Barrel 文件，重新导出 app_state 和 task_manager 子模块。',['barrel','module'],'simple',null],
  ['apps/desktop/src-tauri/src/state/task_manager.rs','file','task_manager.rs','异步任务管理器，提供任务注册（含取消令牌）、取消、等待和清理功能，支撑导入、分析等长时间运行操作的并发管理。',['task-management','async','concurrency','cancellation'],'complex','使用 tokio::task::JoinHandle 管理异步任务，通过 tokio_util CancellationToken 实现协作式取消。']
];

// Create file nodes
for (const [path, type, name, summary, tags, complexity, langNotes] of fileDefs) {
  const node = { id: type+':'+path, type, name, filePath: path, summary, tags, complexity };
  if (langNotes) node.languageNotes = langNotes;
  addNode(node);
}

// Map result by path
const resultMap = {};
for (const r of extractData.results) resultMap[r.path] = r;

// ========== FUNCTION/CLASS NODES ==========
// Significance filter: (exported) OR (non-exported AND lines >= 10) OR (class with 2+ methods OR 20+ lines OR exported)

// Helper to check if a function is significant
function isFnSignificant(fn, fileExports) {
  const exportNames = new Set((fileExports||[]).map(e=>e.name));
  if (exportNames.has(fn.name)) return true; // exported
  if (fnLen(fn) >= 10) return true; // 10+ lines
  return false;
}

function isClassSignificant(cls, fileExports) {
  const exportNames = new Set((fileExports||[]).map(e=>e.name));
  if (exportNames.has(cls.name)) return true;
  if ((cls.methods||[]).length >= 2) return true;
  if ((cls.endLine - cls.startLine) >= 20) return true;
  return false;
}

// Define function/class summaries and tags per file
// Structure: { fnName: [summary, tags] }
const fnMeta = {
  // cache_invalidation.rs
  'cache_invalidation.rs::register': ['注册数据源导入事件监听器，当新数据源导入后触发缓存清理流程。',['event-listener','cache-invalidation','registration']],
  'cache_invalidation.rs::clear_preview_caches_for_case': ['遍历并清理指定案例关联的所有预览缓存（文本、图像、媒体、树结构），释放运行时资源。',['cache-cleanup','preview','resource-management']],

  // analysis_commands.rs
  'analysis_commands.rs::resolve_sample_size': ['根据请求参数解析有效采样大小，将百分比转换为实际字节数。',['sampling','parameter-resolution']],
  'analysis_commands.rs::get_system_info': ['获取当前案例关联的操作系统信息摘要。',['system-info','analysis','command']],
  'analysis_commands.rs::classify_files': ['对证据文件进行分类识别（文档、媒体、可执行文件等类型）。',['file-classification','analysis','command']],
  'analysis_commands.rs::get_evidence_classification_summary': ['获取证据分类统计摘要，返回各类型文件的数量分布。',['classification-summary','statistics','command']],
  'analysis_commands.rs::run_evidence_classification': ['执行全量证据分类扫描，将分类结果持久化到数据库。',['classification','analysis','batch-processing']],
  'analysis_commands.rs::run_analysis_extraction': ['执行分析提取流程，根据请求参数提取注册表、浏览器等特定工件类型。',['analysis-extraction','batch-processing','command']],
  'analysis_commands.rs::get_registry_extraction_summary': ['获取注册表工件提取的统计摘要信息。',['registry','extraction-summary','command']],
  'analysis_commands.rs::get_registry_structured_summary': ['获取注册表结构化摘要，包含键值统计和用户配置信息。',['registry','structured-summary','command']],
  'analysis_commands.rs::get_browser_history_summary': ['获取浏览器历史记录工件的提取摘要。',['browser-history','extraction-summary','command']],
  'analysis_commands.rs::get_email_extraction_summary': ['获取邮件工件提取的统计摘要（PST/OST/mbox）。',['email','extraction-summary','command']],
  'analysis_commands.rs::get_evtx_event_summary': ['获取 Windows EVTX 事件日志工件的提取摘要。',['evtx','event-log','extraction-summary']],
  'analysis_commands.rs::get_linux_artifact_summary': ['获取 Linux 系统工件（systemd journal、wtmp、bash history 等）的提取摘要。',['linux','artifact-summary','command']],
  'analysis_commands.rs::get_v2_governance_snapshot': ['生成 V2 版本的治理快照，汇总案例结构、文件状态和元数据一致性。',['governance','snapshot','audit']],
  'analysis_commands.rs::get_v3_governance_snapshot': ['生成 V3 版本的增强治理快照，包含更详细的数据完整性和合规性检查。',['governance','snapshot','audit','compliance']],
  'analysis_commands.rs::get_correlation_snapshot': ['生成跨工件的关联分析快照，揭示文件、时间线和注册表之间的隐含关系。',['correlation','snapshot','analysis']],
  'analysis_commands.rs::generate_analysis_summary': ['生成综合分析摘要报告，聚合所有分析模块的发现结果。',['analysis-summary','report','aggregation']],

  // artifact_commands.rs
  'artifact_commands.rs::get_artifact_families': ['获取案例中已发现的所有工件族列表。',['artifacts','families','command']],
  'artifact_commands.rs::get_artifact_rows': ['获取指定工件族的原始行数据（分页）。',['artifacts','rows','command']],
  'artifact_commands.rs::get_artifact_rows_request': ['带请求参数的工件行数据查询，支持排序、过滤和分页。',['artifacts','query','command']],
  'artifact_commands.rs::get_artifact_family_counts': ['获取各工件族的数据行数统计。',['artifacts','statistics','command']],
  'artifact_commands.rs::get_artifact_by_id': ['按工件 ID 检索单个工件的详细信息。',['artifacts','detail','command']],

  // batch_commands.rs
  'batch_commands.rs::create_batch_plan': ['创建批量导入计划，接收前端请求并委托给内部实现。',['batch','plan','command']],
  'batch_commands.rs::create_batch_plan_impl': ['实现批量计划创建逻辑，解析文件列表并生成导入任务序列。',['batch','plan','implementation']],
  'batch_commands.rs::start_batch': ['启动已创建的批量导入计划。',['batch','start','command']],
  'batch_commands.rs::start_batch_impl': ['实现批量导入启动逻辑，激活取消令牌并开始执行任务序列。',['batch','start','implementation']],
  'batch_commands.rs::pause_batch': ['暂停正在执行的批量导入计划。',['batch','pause','command']],
  'batch_commands.rs::pause_batch_impl': ['实现批量导入暂停逻辑，设置暂停标志等待当前任务完成。',['batch','pause','implementation']],
  'batch_commands.rs::resume_batch': ['恢复已暂停的批量导入计划。',['batch','resume','command']],
  'batch_commands.rs::resume_batch_impl': ['实现批量导入恢复逻辑，清除暂停标志继续执行后续任务。',['batch','resume','implementation']],
  'batch_commands.rs::cancel_batch': ['取消正在执行的批量导入计划。',['batch','cancel','command']],
  'batch_commands.rs::cancel_batch_impl': ['实现批量导入取消逻辑，触发取消信号并清理任务状态。',['batch','cancel','implementation']],
  'batch_commands.rs::get_batch_job': ['查询指定批量作业的当前状态。',['batch','status','command']],
  'batch_commands.rs::get_batch_job_impl': ['实现批量作业状态查询，从任务管理器读取运行信息。',['batch','status','implementation']],
  'batch_commands.rs::list_batch_jobs': ['列出所有批量作业的概览信息。',['batch','list','command']],
  'batch_commands.rs::list_batch_jobs_impl': ['实现批量作业列表查询，遍历任务管理器汇总作业状态。',['batch','list','implementation']],

  // case_commands.rs
  'case_commands.rs::drain_active_case_jobs': ['等待并清空当前活动案例的所有进行中作业，确保案例关闭前作业全部完成。',['case','cleanup','job-drain']],
  'case_commands.rs::create_case': ['创建新案例，初始化目录结构、数据库（WAL 模式）和元数据记录。',['case','create','command']],
  'case_commands.rs::open_case': ['打开已有案例，加载数据库连接、元数据并设置活动案例状态。',['case','open','command']],
  'case_commands.rs::create_analysis_demo_case': ['创建预置分析数据的演示案例，用于功能演示和测试。',['case','demo','command']],
  'case_commands.rs::get_current_case': ['获取当前活动案例的元数据摘要。',['case','metadata','command']],
  'case_commands.rs::close_case': ['关闭当前案例，释放数据库连接、清空运行时缓存并保存最近案例记录。',['case','close','command']],
  'case_commands.rs::get_case_metrics': ['获取案例统计指标（文件数、数据源数、工件数、作业数等）。',['case','metrics','statistics']],
  'case_commands.rs::get_recent_objects': ['获取案例中最近访问或修改的对象列表。',['case','recent-objects','command']],
  'case_commands.rs::get_data_sources': ['获取案例关联的所有数据源列表。',['datasource','list','command']],
  'case_commands.rs::rename_data_source': ['重命名指定数据源的显示名称。',['datasource','rename','command']],
  'case_commands.rs::get_recent_cases': ['获取全局最近使用案例列表（从持久化 JSON 文件读取）。',['case','recent-list','command']],
  'case_commands.rs::remove_case_from_list': ['从最近案例列表中移除指定案例条目。',['case','list-management','command']],
  'case_commands.rs::delete_case': ['删除案例，清理所有相关文件、目录和数据库记录，含安全检查。',['case','delete','dangerous-operation']],
  'case_commands.rs::delete_data_source': ['从案例中删除指定数据源及其关联的文件记录和工件数据。',['datasource','delete','command']],
  'case_commands.rs::save_recent_cases': ['将最近案例列表持久化到 JSON 文件，处理路径校验和容错。',['case','persistence','recent-list']],
  'case_commands.rs::remember_recent_case': ['将指定案例路径添加到最近案例列表并持久化保存。',['case','remember','recent-list']],
  'case_commands.rs::read_recent_cases': ['从持久化 JSON 文件读取最近案例列表，处理文件不存在和解析错误。',['case','read','persistence']],

  // command_support.rs
  'command_support.rs::snapshot_active_case': ['从应用状态中提取当前活动案例的快照，包含案例 ID、根路径、数据库路径和元数据。',['state','snapshot','utility']],
  'command_support.rs::require_active_case': ['断言活动案例存在，否则返回错误；用于命令前置校验。',['validation','guard','utility']],
  'command_support.rs::get_case_connection': ['获取当前案例的数据库连接（带 WAL/外键/busy_timeout 配置）。',['database','connection','utility']],
  'command_support.rs::current_case_id': ['获取当前活动案例的 ID，快速只读访问。',['state','case-id','utility']],
  'command_support.rs::write_audit_log': ['向审计日志表写入操作记录，用于合规和溯源追踪。',['audit','logging','utility']],

  // file_commands.rs
  'file_commands.rs::increment_preview_read_counter': ['递增预览读取计数器，用于监控和限制预览资源消耗。',['preview','throttling','counter']],
  'file_commands.rs::current_case_id_for_preview': ['获取当前案例 ID 用于预览操作上下文。',['preview','context','utility']],
  'file_commands.rs::get_file_children': ['获取指定目录的子文件和子目录列表。',['file-tree','children','command']],
  'file_commands.rs::get_file_children_request': ['带请求参数的子文件查询，支持排序、过滤和分页。',['file-tree','children','command']],
  'file_commands.rs::get_file_tree': ['获取指定路径的文件树结构（懒加载模式）。',['file-tree','structure','command']],
  'file_commands.rs::get_file_tree_request': ['带请求参数的文件树查询，支持深度控制和节点过滤。',['file-tree','structure','command']],
  'file_commands.rs::get_file_rows': ['获取文件列表行数据（表格视图），支持分页。',['file-list','rows','command']],
  'file_commands.rs::get_file_rows_request': ['带请求参数的文件行查询，支持排序、搜索和过滤。',['file-list','rows','command']],
  'file_commands.rs::get_file_jump_context': ['获取文件在目录树中的跳转上下文（父路径、兄弟节点等）。',['file-navigation','context','command']],
  'file_commands.rs::open_file_handle': ['为指定文件打开读取句柄，用于后续范围读取操作。',['file-handle','open','command']],
  'file_commands.rs::open_file_handle_request': ['带请求参数的文件句柄打开操作，支持偏移量和读取模式配置。',['file-handle','open','command']],
  'file_commands.rs::read_file_range': ['从已打开的文件句柄读取指定字节范围的数据。',['file-read','range','command']],
  'file_commands.rs::format_hex_dump': ['将二进制数据格式化为十六进制转储视图，支持偏移量和 ASCII 预览。',['hex-dump','formatting','utility']],
  'file_commands.rs::get_text_preview': ['获取文件的文本预览内容（UTF-8/UTF-16 自动检测与转码）。',['text-preview','encoding','command']],
  'file_commands.rs::get_image_preview': ['获取证据文件的图像缩略图预览（限定尺寸）。',['image-preview','thumbnail','command']],
  'file_commands.rs::get_media_url': ['生成 evidence-media:// 协议 URL，用于安全传输媒体文件到前端。',['media','url','command']],
  'file_commands.rs::read_media_range': ['通过 evidence-media:// 协议读取媒体文件的指定字节范围（支持 Range 请求）。',['media','range-read','command']],
  'file_commands.rs::extract_file': ['将证据文件导出到主机文件系统（只读复制），含路径安全校验。',['file-export','extraction','command']],
  'file_commands.rs::image_preview_for_file': ['实现文件图像预览的内部逻辑：打开文件 -> 解码图像 -> 缩放 -> 返回 Base64/URL。',['image-preview','implementation','utility']],
  'file_commands.rs::media_data_url_for_file': ['生成媒体文件的 Data URL（小文件）或 evidence-media:// URL（大文件）。',['media','data-url','implementation']],
  'file_commands.rs::media_range_for_file': ['实现媒体文件 Range 请求的内部逻辑，支持部分内容传输。',['media','range','implementation']],
  'file_commands.rs::text_preview_for_file': ['实现文本预览的内部逻辑，自动检测编码并截取预览范围。',['text-preview','encoding','implementation']],
  'file_commands.rs::read_media_bytes_for_file': ['从媒体文件读取原始字节数据，用于 evidence-media:// 协议后端。',['media','bytes','implementation']],
  'file_commands.rs::read_inline_preview_bytes_for_file': ['读取内联预览字节（小文件全量读取，大文件截取预览范围）。',['preview','inline','implementation']],
  'file_commands.rs::read_preview_bytes_for_file': ['读取预览字节的通用入口，根据文件大小选择合适的内存策略。',['preview','bytes','implementation']],

  // graph_commands.rs
  'graph_commands.rs::get_graph_snapshot': ['获取当前案例知识图谱的完整快照。',['knowledge-graph','snapshot','command']],
  'graph_commands.rs::query_graph': ['对知识图谱执行自定义查询，返回匹配的节点和边。',['knowledge-graph','query','command']],
  'graph_commands.rs::get_node_neighborhood': ['获取指定图谱节点的邻域子图（k 跳邻居）。',['knowledge-graph','neighborhood','command']],
  'graph_commands.rs::get_provenance_chain': ['获取指定节点的完整溯源链（数据来源和变换路径）。',['knowledge-graph','provenance','command']],

  // import/background_job.rs
  'import/background_job.rs::run_background_import_job': ['在后台线程中执行导入作业，包含进度报告、分析触发和取消信号监听循环。',['import','background-job','async']],

  // import/cancellation.rs
  'import/cancellation.rs::cancel_import': ['取消指定导入作业，设置取消信号并通知后台任务停止。',['import','cancellation','command']],
  'import/cancellation.rs::job_cancellation_dto': ['将作业取消状态转换为客户端可消费的 DTO 格式。',['import','dto','cancellation']],
  'import/cancellation.rs::is_import_cancelled_message': ['检查错误消息是否为导入取消错误，用于错误分类处理。',['import','error-classification','utility']],

  // import/pipeline.rs
  'import/pipeline.rs::import_data_source': ['执行数据源导入的顶层命令入口，委托给调度模块处理。',['import','command','entry-point']],
  'import/pipeline.rs::cancel_import': ['取消数据源导入的顶层命令入口。',['import','cancel','command']],

  // import/schedule.rs
  'import/schedule.rs::import_data_source': ['执行数据源导入的完整流程：验证请求 -> 构建配置 -> 调度后台作业。',['import','orchestration','command']],
  'import/schedule.rs::load_import_settings': ['加载用户的导入设置（编码、分析深度、文件类型过滤器等）。',['import','settings','configuration']],
  'import/schedule.rs::schedule_import_for_active_case': ['为活动案例调度导入作业，包含取消注册和事件发送逻辑。',['import','scheduling','job-management']],
  'import/schedule.rs::import_analysis_mode_from_settings': ['从导入设置中解析分析模式（完整/快速/仅索引）。',['import','analysis-mode','configuration']],
  'import/schedule.rs::prepare_import_config': ['构建导入配置对象，合并请求参数、用户设置和默认值。',['import','configuration','preparation']],
  'import/schedule.rs::import_config_error_to_command_error': ['将导入配置错误转换为统一的命令层错误类型。',['import','error-conversion','utility']],

  // job_commands.rs
  'job_commands.rs::get_jobs_snapshot': ['获取当前所有作业的快照列表（状态、进度、错误信息）。',['job','snapshot','command']],
  'job_commands.rs::get_warnings': ['获取指定作业产生的警告信息列表。',['job','warnings','command']],
  'job_commands.rs::get_trace_items': ['获取指定作业的执行追踪条目（分步详细记录）。',['job','trace','command']],

  // mcp_commands.rs
  'mcp_commands.rs::get_connected_mcp_client': ['获取已连接的 MCP 客户端实例，不存在则返回错误。',['mcp','client','utility']],
  'mcp_commands.rs::transport_from_dto': ['将传输层 DTO 转换为领域模型 TransportConfig。',['mcp','dto-conversion','transport']],
  'mcp_commands.rs::resource_access_from_dto': ['将资源访问控制 DTO 转换为领域模型。',['mcp','dto-conversion','access-control']],
  'mcp_commands.rs::tool_access_from_dto': ['将工具访问控制 DTO 转换为领域模型。',['mcp','dto-conversion','access-control']],
  'mcp_commands.rs::prompt_access_from_dto': ['将提示访问控制 DTO 转换为领域模型。',['mcp','dto-conversion','access-control']],
  'mcp_commands.rs::network_policy_from_dto': ['将网络策略 DTO 转换为领域模型。',['mcp','dto-conversion','network']],
  'mcp_commands.rs::permissions_from_dto': ['将权限配置 DTO 转换为领域模型 Permissions。',['mcp','dto-conversion','permissions']],
  'mcp_commands.rs::resource_access_to_dto': ['将领域模型 ResourceAccess 转换为传输 DTO。',['mcp','dto-conversion','serialization']],
  'mcp_commands.rs::tool_access_to_dto': ['将领域模型 ToolAccess 转换为传输 DTO。',['mcp','dto-conversion','serialization']],
  'mcp_commands.rs::prompt_access_to_dto': ['将领域模型 PromptAccess 转换为传输 DTO。',['mcp','dto-conversion','serialization']],
  'mcp_commands.rs::network_policy_to_dto': ['将领域模型 NetworkPolicy 转换为传输 DTO。',['mcp','dto-conversion','serialization']],
  'mcp_commands.rs::permissions_to_dto': ['将领域模型 Permissions 转换为传输 DTO。',['mcp','dto-conversion','serialization']],
  'mcp_commands.rs::server_config_from_dto': ['将服务器配置 DTO 转换为领域模型 McpServerConfig。',['mcp','dto-conversion','configuration']],
  'mcp_commands.rs::config_from_dto': ['将完整 MCP 配置 DTO 转换为领域模型 McpConfig。',['mcp','dto-conversion','configuration']],
  'mcp_commands.rs::status_to_dto': ['将 MCP 服务器连接状态转换为客户端可消费的 DTO 格式。',['mcp','dto-conversion','status']],
  'mcp_commands.rs::summarize_transport': ['生成 MCP 传输层配置的人类可读摘要字符串。',['mcp','summary','transport']],
  'mcp_commands.rs::test_transport_summary_from_request': ['从测试请求中提取传输配置摘要，用于连接测试日志。',['mcp','test','summary']],
  'mcp_commands.rs::get_mcp_config': ['读取当前 MCP 配置（从 AppState 缓存或磁盘文件）。',['mcp','configuration','command']],
  'mcp_commands.rs::save_mcp_config': ['保存 MCP 配置到磁盘并同步到 AppState 运行时状态。',['mcp','configuration','command']],
  'mcp_commands.rs::add_mcp_server': ['添加新的 MCP 服务器配置条目。',['mcp','server','command']],
  'mcp_commands.rs::remove_mcp_server': ['移除指定的 MCP 服务器配置条目。',['mcp','server','command']],
  'mcp_commands.rs::connect_mcp_server': ['连接到指定的 MCP 服务器（初始化传输层和握手）。',['mcp','connect','command']],
  'mcp_commands.rs::disconnect_mcp_server': ['断开与指定 MCP 服务器的连接并清理资源。',['mcp','disconnect','command']],
  'mcp_commands.rs::test_mcp_connection': ['测试 MCP 服务器连接可用性，返回连接测试结果摘要。',['mcp','test','command']],
  'mcp_commands.rs::list_mcp_resources': ['列出 MCP 服务器提供的所有资源（文件、数据源等）。',['mcp','resources','command']],
  'mcp_commands.rs::list_mcp_tools': ['列出 MCP 服务器提供的所有工具。',['mcp','tools','command']],
  'mcp_commands.rs::call_mcp_tool': ['调用 MCP 服务器上的指定工具并返回执行结果。',['mcp','tool-call','command']],
  'mcp_commands.rs::list_mcp_prompts': ['列出 MCP 服务器提供的所有提示模板。',['mcp','prompts','command']],
  'mcp_commands.rs::get_mcp_prompt': ['获取指定 MCP 提示模板的完整内容。',['mcp','prompt','command']],

  // notebook_commands.rs
  'notebook_commands.rs::create_notebook_entry': ['创建新的调查笔记条目，关联当前案例。',['notebook','create','command']],
  'notebook_commands.rs::update_notebook_entry': ['更新已有调查笔记条目的内容。',['notebook','update','command']],
  'notebook_commands.rs::list_notebook_entries': ['列出案例的所有调查笔记条目（分页）。',['notebook','list','command']],
  'notebook_commands.rs::get_notebook_thread': ['获取指定笔记条目的完整讨论线程（回复链）。',['notebook','thread','command']],
  'notebook_commands.rs::add_evidence_citation': ['向笔记条目添加证据文件引用，链接到具体文件和偏移量。',['notebook','citation','evidence']],
  'notebook_commands.rs::list_investigation_steps': ['列出案例的调查步骤清单（时间线操作记录）。',['notebook','investigation','steps']],

  // report_commands.rs
  'report_commands.rs::get_report_templates': ['获取可用报告模板列表。',['report','templates','command']],
  'report_commands.rs::get_report_history': ['获取案例的历史报告生成记录。',['report','history','command']],
  'report_commands.rs::export_html_report': ['导出 HTML 格式的综合取证报告。',['report','export','html']],
  'report_commands.rs::export_csv_report': ['导出 CSV 格式的表格数据报告。',['report','export','csv']],
  'report_commands.rs::export_csv_correlation_report': ['导出 CSV 格式的关联分析报告。',['report','export','correlation']],
  'report_commands.rs::export_json_report': ['导出 JSON 格式的结构化取证报告。',['report','export','json']],

  // rule_pack_commands.rs
  'rule_pack_commands.rs::list_loaded_rule_packs': ['列出当前已加载的全部规则包及其状态。',['rules','list','command']],
  'rule_pack_commands.rs::load_rule_pack': ['从指定路径加载规则包文件并注册到分析引擎。',['rules','load','command']],
  'rule_pack_commands.rs::validate_rule_pack': ['验证指定规则包的结构和规则语法正确性。',['rules','validation','command']],

  // search_commands.rs
  'search_commands.rs::search_files': ['执行全文本搜索，查询索引中的文件匹配结果（分页）。',['search','full-text','command']],
  'search_commands.rs::search_files_request': ['带请求参数的全文本搜索，支持过滤条件、排序方式和结果限制。',['search','full-text','command']],

  // settings_commands.rs
  'settings_commands.rs::get_app_settings': ['读取当前应用设置（主题、语言、预览限制等）。',['settings','read','command']],
  'settings_commands.rs::save_app_settings': ['保存应用设置到磁盘并通知相关组件配置变更。',['settings','save','command']],
  'settings_commands.rs::load_app_settings': ['从磁盘加载应用设置（含默认值回退逻辑）。',['settings','load','persistence']],

  // timeline_commands.rs
  'timeline_commands.rs::get_timeline_events': ['获取时间线事件列表（分页、按时间排序）。',['timeline','events','command']],
  'timeline_commands.rs::get_timeline_event_by_id': ['按 ID 检索单个时间线事件的详细信息。',['timeline','detail','command']],

  // events/event_bridge.rs
  'events/event_bridge.rs::emit_event': ['底层事件发射函数，将事件负载序列化后通过 Tauri AppHandle 发送。',['events','emit','core']],
  'events/event_bridge.rs::emit_case_opened': ['发射案件打开事件，携带案件元数据。',['events','case','opened']],
  'events/event_bridge.rs::emit_case_closed': ['发射案件关闭事件。',['events','case','closed']],
  'events/event_bridge.rs::emit_job_created': ['发射作业创建事件，携带作业 ID 和类型。',['events','job','created']],
  'events/event_bridge.rs::emit_job_started': ['发射作业开始事件，携带作业进度信息。',['events','job','started']],
  'events/event_bridge.rs::emit_job_progress': ['发射作业进度更新事件，携带百分比和当前步骤描述。',['events','job','progress']],
  'events/event_bridge.rs::emit_job_completed': ['发射作业完成事件，携带结果摘要。',['events','job','completed']],
  'events/event_bridge.rs::emit_job_failed': ['发射作业失败事件，携带错误信息和堆栈追踪。',['events','job','failed']],
  'events/event_bridge.rs::emit_job_cancelled': ['发射作业取消事件。',['events','job','cancelled']],
  'events/event_bridge.rs::emit_job_cancellation': ['发射作业取消信号事件（与 cancelled 事件区分阶段）。',['events','job','cancellation']],
  'events/event_bridge.rs::emit_data_source_imported': ['发射数据源导入完成事件，携带数据源元数据和统计信息。',['events','import','completed']],
  'events/event_bridge.rs::emit_artifact_added': ['发射新工件发现事件，携带工件族和 ID。',['events','artifacts','added']],
  'events/event_bridge.rs::emit_timeline_updated': ['发射时间线更新事件，通知前端刷新时间线视图。',['events','timeline','updated']],
  'events/event_bridge.rs::emit_search_index_progress': ['发射搜索索引构建进度事件。',['events','search','index']],
  'events/event_bridge.rs::emit_partition_progress': ['发射分区处理进度事件（文件系统分区扫描）。',['events','partition','progress']],
  'events/event_bridge.rs::emit_import_phase_progress': ['发射导入阶段进度事件（如解析、索引、分析进度）。',['events','import','phase']],
  'events/event_bridge.rs::emit_import_partial_result': ['发射导入局部结果事件，支持增量导入结果通知。',['events','import','partial']],
  'events/event_bridge.rs::emit_cache_index_status': ['发射缓存索引状态事件，通知前端缓存可用性。',['events','cache','index']],
  'events/event_bridge.rs::emit_performance_report_ready': ['发射性能报告就绪事件，通知基准测试或性能分析结果可用。',['events','performance','report']],

  // lib.rs
  'lib.rs::run': ['Tauri 应用启动主函数，配置命令注册、插件初始化、媒体协议设置和窗口创建，并启动事件循环。',['entry-point','bootstrap','application']],

  // media_protocol.rs
  'media_protocol.rs::status': ['将 RangeError 转换为 HTTP 状态码字符串。',['protocol','http','utility']],
  'media_protocol.rs::media_protocol_url': ['构建 evidence-media:// 协议的完整 URL。',['protocol','url','utility']],
  'media_protocol.rs::create_scoped_media_handle': ['创建带作用域的媒体句柄，限制访问范围和有效期。',['protocol','security','handle']],
  'media_protocol.rs::resolve_scoped_media_handle': ['解析并验证带作用域的媒体句柄，返回原始文件信息。',['protocol','security','resolution']],
  'media_protocol.rs::resolve_media_handle_from_uri': ['从 evidence-media:// URI 字符串中解析并验证媒体句柄。',['protocol','uri-parsing','security']],
  'media_protocol.rs::parse_media_range_header': ['解析 HTTP Range 请求头，提取字节范围参数并处理边缘情况。',['protocol','range','parsing']],
  'media_protocol.rs::build_content_range': ['构建 HTTP Content-Range 响应头字符串。',['protocol','http','utility']],
  'media_protocol.rs::register': ['向 Tauri 注册 evidence-media:// 自定义协议处理器。',['protocol','registration','entry-point']],
  'media_protocol.rs::handle_media_protocol_request': ['处理 evidence-media:// 协议请求的入口，包含错误处理和日志记录。',['protocol','handler','entry-point']],
  'media_protocol.rs::handle_media_protocol_request_inner': ['实现媒体协议请求的核心处理逻辑：句柄解析 -> 文件打开 -> Range 处理 -> 响应构建。',['protocol','handler','core']],
  'media_protocol.rs::read_media_protocol_bytes': ['从证据文件系统读取媒体数据字节，支持偏移量和大小限制。',['protocol','read','core']],
  'media_protocol.rs::text_response': ['构建纯文本 HTTP 响应（用于错误消息和状态回报）。',['protocol','http','response']],

  // platform_security.rs
  'platform_security.rs::restrict_file_to_current_user': ['通过 Windows ACL 或 Unix chmod 限制文件仅当前用户可读写，保护证据和案例数据安全。',['security','acl','filesystem']],

  // state/app_state.rs
  'state/app_state.rs::get_mcp_client': ['获取指定 ID 的 MCP 客户端实例。',['mcp','client','state']],
  'state/app_state.rs::replace_mcp_client': ['替换或更新指定 ID 的 MCP 客户端实例。',['mcp','client','state']],
  'state/app_state.rs::remove_mcp_client': ['移除指定 ID 的 MCP 客户端实例。',['mcp','client','state']],
  'state/app_state.rs::sync_mcp_clients_with_config': ['根据 MCP 配置同步客户端实例（新增、移除、重连）。',['mcp','sync','state']],
  'state/app_state.rs::init_db_pragmas': ['初始化数据库连接 PRAGMA 设置（WAL、外键、busy_timeout、synchronous 模式）。',['database','pragmas','initialization']],
  'state/app_state.rs::get_connection': ['获取案例数据库连接池，如未初始化则先执行初始化。',['database','connection','lazy-init']],
  'state/app_state.rs::clear_db_state': ['清理案例数据库相关的运行时状态（不删除数据库文件）。',['database','cleanup','state']],
  'state/app_state.rs::clear_runtime_cache_for_case': ['清空指定案例的运行时缓存条目。',['cache','cleanup','state']],
  'state/app_state.rs::load_mcp_config': ['从磁盘加载 MCP 配置文件。',['mcp','configuration','persistence']],
  'state/app_state.rs::save_mcp_config': ['保存 MCP 配置到磁盘并在内存中更新缓存。',['mcp','configuration','persistence']],
  'state/app_state.rs::add_mcp_server': ['向内存配置添加 MCP 服务器条目并持久化。',['mcp','server','configuration']],
  'state/app_state.rs::remove_mcp_server': ['从内存配置移除 MCP 服务器条目并持久化。',['mcp','server','configuration']],
  'state/app_state.rs::get_mcp_server_status': ['获取指定 MCP 服务器的当前连接状态。',['mcp','status','state']],
  'state/app_state.rs::connect_mcp_server': ['连接到指定 MCP 服务器，初始化传输层和协议握手。',['mcp','connect','state']],
  'state/app_state.rs::disconnect_mcp_server': ['断开与指定 MCP 服务器的连接并清理关联资源。',['mcp','disconnect','state']],

  // state/task_manager.rs
  'state/task_manager.rs::new': ['创建新的 TaskManager 实例，初始化任务集合和默认值。',['task','initialization']],
  'state/task_manager.rs::register': ['注册新任务到管理器，返回任务 ID 用于后续追踪。',['task','registration']],
  'state/task_manager.rs::register_with_token': ['注册带外部取消令牌的任务，支持级联取消。',['task','registration','cancellation']],
  'state/task_manager.rs::cancel': ['取消指定任务，触发取消令牌并清理任务句柄。',['task','cancel']],
  'state/task_manager.rs::cancel_all': ['取消所有已注册的任务。',['task','cancel','bulk']],
  'state/task_manager.rs::wait_all': ['等待所有已注册任务完成。',['task','wait','synchronization']],
  'state/task_manager.rs::wait_task': ['等待指定任务完成并返回结果。',['task','wait','synchronization']],
  'state/task_manager.rs::cleanup_finished': ['清理已完成的无效任务句柄，释放内存资源。',['task','cleanup','resource']],
  'state/task_manager.rs::running_tasks': ['返回当前运行中任务的 ID 列表。',['task','monitoring']],
  'state/task_manager.rs::task_count': ['返回当前注册的任务总数。',['task','monitoring','statistics']],
  'state/task_manager.rs::is_running': ['检查指定任务是否仍在运行。',['task','status']],
  'state/task_manager.rs::task_elapsed': ['获取指定任务的已运行时长。',['task','timing']],
  'state/task_manager.rs::is_cancelled': ['检查指定任务是否已被取消。',['task','status','cancellation']],
  'state/task_manager.rs::get_cancel_token': ['获取指定任务的取消令牌（用于传递给子任务）。',['task','cancellation','token']],
};

// Class meta
const classMeta = {
  'command_support.rs::ActiveCaseSnapshot': ['活动案例上下文快照结构体，封装案例 ID、根路径、数据库路径和元数据引用。',['struct','snapshot','state']],
  'import/background_job.rs::BackgroundImportJob': ['后台导入作业配置结构体，持有数据库路径、案例信息、源路径和分析参数。',['struct','import','configuration']],
  'rule_pack_commands.rs::LoadRulePackRequest': ['规则包加载请求 DTO，指定规则包文件路径。',['dto','request','rules']],
  'rule_pack_commands.rs::ValidateRulePackRequest': ['规则包验证请求 DTO，指定待验证的规则包 ID。',['dto','request','validation']],
  'media_protocol.rs::ResolvedRange': ['HTTP Range 请求解析结果，包含起始、结束、长度和状态码信息。',['struct','range','http']],
  'media_protocol.rs::RangeError': ['Range 请求错误枚举，定义空文件、无效范围和不可满足范围三种错误类型。',['enum','error','http']],
  'state/app_state.rs::AppState': ['全局应用状态结构体，持有活动案例、任务管理器、MCP 客户端、配置路径和运行时缓存。',['struct','state','singleton']],
  'state/task_manager.rs::TaskEntry': ['任务条目结构体，持有 JoinHandle、取消令牌和启动时间戳。',['struct','task','cancellation']],
  'state/task_manager.rs::TaskManager': ['异步任务管理器结构体，管理所有注册任务的注册、取消、等待和清理操作。',['struct','task-management','concurrency']],
};

// Now process each file
for (const filePath of Object.keys(resultMap)) {
  const result = resultMap[filePath];
  const exports = result.exports || [];
  const exportNames = new Set(exports.map(e=>e.name));

  // Function nodes
  for (const fn of (result.functions||[])) {
    if (!isFnSignificant(fn, exports)) continue;

    // For platform_security.rs, there are two functions with same name (cfg-gated).
    // Use startLine to disambiguate the Windows version (lines 9-114) from Unix (117-121)
    let suffix = '';
    if (filePath.includes('platform_security') && fn.name === 'restrict_file_to_current_user') {
      suffix = fn.startLine < 50 ? '_windows' : '_unix';
      // Skip the trivial Unix stub (only 4 lines) unless it's the only one
      if (fnLen(fn) < 10) continue;
    }

    const fnId = 'function:'+filePath+':'+fn.name+suffix;
    const shortPath = filePath.replace('apps/desktop/src-tauri/src/','');
    const metaKey = shortPath+'::'+fn.name;
    const meta = fnMeta[metaKey] || [fn.name+' 函数。',['function']];

    addNode({
      id: fnId,
      type: 'function',
      name: fn.name,
      filePath: filePath,
      lineRange: [fn.startLine, fn.endLine],
      summary: meta[0],
      tags: meta[1],
      complexity: fnLen(fn) > 30 ? 'complex' : fnLen(fn) > 15 ? 'moderate' : 'simple'
    });

    addEdge('file:'+filePath, fnId, 'contains', 1.0);
    if (exportNames.has(fn.name)) {
      addEdge('file:'+filePath, fnId, 'exports', 0.8);
    }
  }

  // Class nodes
  for (const cls of (result.classes||[])) {
    if (!isClassSignificant(cls, exports)) continue;

    const clsId = 'class:'+filePath+':'+cls.name;
    const shortPath = filePath.replace('apps/desktop/src-tauri/src/','');
    const metaKey = shortPath+'::'+cls.name;
    const meta = classMeta[metaKey] || [cls.name+' 结构体/类。',['class']];

    addNode({
      id: clsId,
      type: 'class',
      name: cls.name,
      filePath: filePath,
      lineRange: [cls.startLine, cls.endLine],
      summary: meta[0],
      tags: meta[1],
      complexity: (cls.endLine - cls.startLine) > 100 ? 'complex' : (cls.endLine - cls.startLine) > 30 ? 'moderate' : 'simple'
    });

    addEdge('file:'+filePath, clsId, 'contains', 1.0);
    if (exportNames.has(cls.name)) {
      addEdge('file:'+filePath, clsId, 'exports', 0.8);
    }
  }
}

// IMPORT edges
for (const [filePath, imports] of Object.entries(importData)) {
  for (const imp of imports) {
    addEdge('file:'+filePath, 'file:'+imp, 'imports', 0.7);
  }
}

// Verify import count
let totalImports = 0;
for (const arr of Object.values(importData)) totalImports += arr.length;
console.log('Expected import edges: ' + totalImports + ', generated: ' + edges.filter(e=>e.type==='imports').length);

// Write output
fs.mkdirSync('D:/process/forensic/.understand-anything/intermediate', {recursive: true});
fs.writeFileSync('D:/process/forensic/.understand-anything/intermediate/batch-2.json', JSON.stringify({nodes, edges}, null, 2));
console.log('Total nodes: ' + nodes.length);
console.log('Total edges: ' + edges.length);
console.log('Output written to batch-2.json');
