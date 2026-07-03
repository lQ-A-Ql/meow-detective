import { readFileSync, writeFileSync } from 'fs';

// Read the structural summary
const summary = JSON.parse(readFileSync('D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-4-summary.json', 'utf8'));

// Read the batch data
const batchRaw = readFileSync('D:/process/forensic/.understand-anything/tmp/batch-4-extracted.json', 'utf8').replace(/^﻿/, '');
const batchData = JSON.parse(batchRaw);
const batchImportData = batchData.batchImportData || {};
const neighborMap = batchData.neighborMap || {};
const files = batchData.files || [];
const filePathsInBatch = new Set(files.map(f => f.path));

// ====== COLLECTORS ======
const graphNodes = [];
const graphEdges = [];
const nodeIds = new Set();
const edgeKeys = new Set();

function addNode(node) {
    if (nodeIds.has(node.id)) return false;
    nodeIds.add(node.id);
    graphNodes.push(node);
    return true;
}

function addEdge(source, target, type, weight) {
    if (source === target) return false;
    const key = `${source}|${target}|${type}`;
    if (edgeKeys.has(key)) return false;
    edgeKeys.add(key);
    graphEdges.push({ source, target, type, direction: 'forward', weight });
    return true;
}

// ====== SIGNIFICANCE FILTERS ======
function isSignificantFunction(func, isExported) {
    const lineCount = func.endLine - func.startLine;
    if (lineCount >= 10) return true;
    // Even exported, skip trivial one-liners and simple getters
    if (isExported && lineCount >= 4) return true;
    return false;
}

function isSignificantClass(cls, isExported) {
    if (isExported) return true;
    if ((cls.methods || []).length >= 2) return true;
    if ((cls.endLine - cls.startLine) >= 20) return true;
    return false;
}

// ====== FILE SUMMARIES & TAGS ======
const pathToSummary = {
    'crates/app-services/src/import_analysis/mod.rs': {
        summary: '导入分析模块入口，通过 pub mod 和 pub use 组织并重新导出分析子模块（budget、error、finalize、options、priority_queue、progress、task_feed、tier、worker_pool、worker_runtime），包含 #[cfg(test)] 内联集成测试。',
        tags: ['barrel', 'entry-point', 'import-analysis', 're-export']
    },
    'crates/app-services/src/import_analysis/options.rs': {
        summary: '定义导入分析配置选项类型，包括 ImportAnalysisMode（元数据/预算内容/完整内容三模式）、ImportAnalysisOptions（工作线程数、内容预算、内存限制等）、PostImportPipelineOptions 及统计类型。',
        tags: ['configuration', 'data-model', 'import-analysis', 'type-definition']
    },
    'crates/app-services/src/import_analysis/priority_queue.rs': {
        summary: '实现基于优先级的任务队列 PriorityTaskQueue，支持高/普通/低三级优先级的入队、出队操作，用于排序导入分析任务。',
        tags: ['data-structure', 'task-queue', 'import-analysis', 'utility']
    },
    'crates/app-services/src/import_analysis/progress.rs': {
        summary: '提供导入分析进度监控工具，包括当前 RSS 内存获取、调度状态评估、内存硬限制检查、吞吐量计算（rows/sec），支持测试环境的 RSS 覆盖。',
        tags: ['utility', 'monitoring', 'progress-tracking', 'import-analysis']
    },
    'crates/app-services/src/import_analysis/task_feed.rs': {
        summary: '实现分析任务供给层，从数据库分页查询待分析文件条目并按优先级入队到任务队列，计算任务队列容量边界。',
        tags: ['task-queue', 'data-access', 'import-analysis', 'utility']
    },
    'crates/app-services/src/import_analysis/tier.rs': {
        summary: '定义三级分析流程状态机（Catalog → ExtractArtifacts → CorrelateAndIndex），每层完成后记录结果和警告，驱动导入分析的阶段性推进。',
        tags: ['state-machine', 'tiered-processing', 'import-analysis', 'data-model']
    },
    'crates/app-services/src/import_analysis/worker_pool.rs': {
        summary: '实现导入分析工作池协调器，负责启动和管理多个分析 Worker 线程，合并各 Worker 的临时数据库结果，运行导入后管线。',
        tags: ['worker-pool', 'orchestration', 'import-analysis', 'concurrency']
    },
    'crates/app-services/src/import_analysis/worker_runtime.rs': {
        summary: '定义单个分析 Worker 的运行时逻辑，包括文件任务处理循环、工件提取、时间线事件生成、文本索引、内容预算预留和统计持久化。',
        tags: ['worker', 'runtime', 'import-analysis', 'artifact-extraction', 'concurrency']
    },
    'crates/app-services/src/import_pipeline/emit.rs': {
        summary: '封装 Tauri 事件发射工具函数，涵盖作业进度、分区进度、时间线更新、搜索索引进度、数据源导入完成等导入管线中的各类事件推送。',
        tags: ['event-emission', 'ipc', 'import-pipeline', 'utility']
    },
    'crates/app-services/src/import_pipeline/execute.rs': {
        summary: '实现导入作业执行引擎，协调分类、枚举、后处理等阶段的执行流程，生成进度报告、部分结果和缓存状态 DTO，提供取消处理和性能指标计算。',
        tags: ['import-pipeline', 'execution-engine', 'orchestration', 'cancellation']
    },
    'crates/app-services/src/import_pipeline/mod.rs': {
        summary: '导入管线模块入口，通过 pub mod 声明组织 emit、execute、options、partition、phases、tests、types 等子模块。',
        tags: ['barrel', 'entry-point', 'import-pipeline']
    },
    'crates/app-services/src/import_pipeline/options.rs': {
        summary: '定义导入作业选项类型 ImportJobOptions（Tauri App 句柄、取消令牌、工作线程数、分析模式）和统计计数类型 JobOutcomeCounts。',
        tags: ['configuration', 'data-model', 'import-pipeline']
    },
    'crates/app-services/src/import_pipeline/partition.rs': {
        summary: '实现镜像数据源的分区发现和枚举逻辑，支持从磁盘镜像中检测文件系统分区，为每个分区创建枚举工作项，格式化分区名称和进度信息。',
        tags: ['partition-enumeration', 'image-processing', 'import-pipeline']
    },
    'crates/app-services/src/import_pipeline/phases.rs': {
        summary: '定义导入管线各阶段函数：attach（附加数据源）、enumeration（枚举分区和文件，含逻辑目录和镜像数据源两种路径）、post-import（后处理分析）和 finalize（收尾和持久化），构成完整的导入生命周期。',
        tags: ['import-pipeline', 'pipeline-phases', 'orchestration', 'lifecycle']
    },
    'crates/app-services/src/import_pipeline/tests.rs': {
        summary: '导入管线综合测试套件，覆盖分区根命名、进度 DTO 映射、枚举合并进度、分析进度、调度状态、缓存状态、取消流程、逻辑导入后管线、E01 完整导入等场景。',
        tags: ['test', 'integration-test', 'import-pipeline', 'e01']
    },
    'crates/app-services/src/import_pipeline/types.rs': {
        summary: '定义导入管线核心类型：ImportJobContext（包含连接、案例、作业、数据源的完整上下文）、PhaseTelemetry（阶段耗时测量）、以及辅助访问器和进度报告方法。',
        tags: ['data-model', 'type-definition', 'import-pipeline', 'telemetry']
    },
    'crates/app-services/src/import_precheck.rs': {
        summary: '实现导入前预检逻辑，包括数据源路径验证、源类型识别（逻辑目录/磁盘镜像）、递归目录分析、镜像大小估计、导入策略选择（Sequential/Parallel/Streaming/Adaptive）和预检结果生成。',
        tags: ['validation', 'pre-check', 'import-pipeline', 'configuration']
    },
    'crates/app-services/src/import_report.rs': {
        summary: '实现导入报告生成器 ImportReport，跟踪导入事件、警告、错误和性能指标（分类/枚举/后处理阶段耗时、文件速率、内存峰值），支持输出 Markdown 格式报告。',
        tags: ['reporting', 'import-pipeline', 'markdown', 'monitoring']
    },
    'crates/app-services/src/import_state.rs': {
        summary: '定义导入状态机和资源估计模型，包含 ImportState（阶段、进度、可恢复状态）、ImportPhase（分类/枚举/后处理/完成/失败/暂停）、ImportPlan（策略、时间/内存估计）等核心类型。',
        tags: ['state-machine', 'data-model', 'import-pipeline', 'estimation']
    },
    'crates/app-services/src/job_service.rs': {
        summary: '实现作业管理服务层，提供数据库层面的作业查询、警告条目获取、跟踪条目获取、取消作业和中断作业恢复功能，解析分区进度元数据。',
        tags: ['job-management', 'service', 'cancellation', 'recovery']
    },
    'crates/app-services/src/lib.rs': {
        summary: 'app-services crate 根模块，通过 pub mod 声明和组织所有子模块（active_case、analysis_service、artifact_service、case_service、import_analysis、import_pipeline 等），是整个服务层的公共入口。',
        tags: ['barrel', 'entry-point', 'crate-root', 'module-registry']
    },
    'crates/app-services/src/notebook_service/error.rs': {
        summary: '定义笔记本服务的错误类型 NotebookError，支持数据库错误、SQLite 错误、未找到和其他通用错误的分类。',
        tags: ['error-handling', 'notebook', 'data-model']
    },
    'crates/app-services/src/notebook_service/mod.rs': {
        summary: '实现笔记本服务核心功能，包括笔记条目的创建/更新/查询、线程查询、引文管理、操作步骤记录和查询，提供 DTO 与领域类型之间的双向转换。',
        tags: ['notebook', 'service', 'crud', 'dto-conversion']
    },
    'crates/app-services/src/parallel_enum/error.rs': {
        summary: '定义并行枚举的错误类型 ParallelEnumError，支持取消、IO 错误、MFT 参数错误和数据库错误四种分类。',
        tags: ['error-handling', 'parallel-enumeration', 'data-model']
    },
    'crates/app-services/src/parallel_enum/mod.rs': {
        summary: '并行枚举模块入口，通过 pub mod 声明组织 error、ntfs_mft、partition_worker、progress、scheduler、staging_meta 子模块，包含 #[cfg(test)] 内联集成测试。',
        tags: ['barrel', 'entry-point', 'parallel-enumeration', 're-export']
    },
    'crates/app-services/src/parallel_enum/ntfs_mft.rs': {
        summary: '实现 NTFS MFT 并行枚举核心逻辑，从 MFT 记录中读取文件元数据写入 staging 数据库，包括 MFT 参数读取、记录修复、数据运行解析、目录索引回填、路径解析和分区条目构建。',
        tags: ['ntfs', 'mft', 'filesystem', 'parallel-enumeration', 'staging']
    },
    'crates/app-services/src/parallel_enum/partition_worker.rs': {
        summary: '实现单个分区的枚举 Worker，将分区文件系统条目写入 staging 数据库，支持通用文件系统和 NTFS MFT 两种枚举路径。',
        tags: ['worker', 'partition-enumeration', 'filesystem', 'staging']
    },
    'crates/app-services/src/parallel_enum/progress.rs': {
        summary: '提供并行枚举进度计算工具 heartbeat_percent，基于已完成和已提交条目数估算进度百分比，用于定期进度报告。',
        tags: ['utility', 'progress-tracking', 'parallel-enumeration']
    },
    'crates/app-services/src/parallel_enum/scheduler.rs': {
        summary: '实现并行分区枚举调度器 enumerate_partitions_parallel，将多个分区分配给有限数量的工作线程并发枚举，收集各 Worker 的结果和警告。',
        tags: ['scheduler', 'concurrency', 'parallel-enumeration', 'orchestration']
    },
};

// Function-specific summaries
const funcSummaries = {
    'execute_import_job': '导入作业的主执行入口，协调分类、枚举、后处理阶段的完整流程。',
    'execute_import_job_with_counts': '执行导入作业并返回带统计计数的结果，包含取消处理和进度事件发射。',
    'run_import_analysis_staging': '启动导入分析 staging 流程，创建多个 Worker 线程并行处理文件任务，合并临时数据库结果。',
    'run_analysis_worker': '单个分析 Worker 主循环，从任务通道接收文件任务，依次执行工件提取、文本索引和时间线生成。',
    'run_attach_phase': '执行 attach 阶段：验证数据源路径、附加到数据库、发射事件。',
    'run_enumeration_phase': '执行枚举阶段：根据数据源类型分发到逻辑目录或镜像数据源枚举路径。',
    'run_post_import_phase': '执行后导入阶段：运行分析管线、合并 worker 结果、产生缓存状态。',
    'run_finalize_phase': '执行最终化阶段：持久化统计、更新数据源状态、发射完成事件。',
    'enumerate_ntfs_mft_to_staging': '将 NTFS MFT 记录枚举写入 staging 数据库，包括 MFT 参数读取、记录解析和路径构建。',
    'enumerate_partitions_parallel': '并行调度多个分区枚举任务到工作线程池，收集各分区结果。',
    'enumerate_single_partition': '枚举单个分区的文件系统条目，根据文件系统类型选择通用枚举或 NTFS MFT 路径。',
    'enqueue_analysis_tasks_prioritized': '按优先级从数据库分页获取文件条目并入队到分析任务队列。',
    'pre_import_check': '执行导入前预检，验证源路径、分析目录结构、估计规模并生成导入计划。',
    'reserve_content_budget': '检查内容预算并可选择性地从共享状态中预留空间，用于控制内容提取的内存使用。',
    'update_mft_staging_paths_via_sqlite': '通过 SQLite 批量更新 MFT staging 中的文件路径和父 ID。',
    'flush_worker_rows': '将 Worker 累积的工件、时间线事件和索引文档批量写入数据库。',
    'enumerate_image_data_source': '发现并枚举磁盘镜像中的所有文件系统分区，为每个分区创建枚举工作项。',
    'build_pending_partition_work': '从 manifest 中构建待处理分区的枚举工作项列表。',
    'should_extract_artifact': '判断给定文件是否应进行工件提取，基于文件类型和注册的提取器。',
    'should_index_file': '判断文件是否应被文本索引，基于扩展名和文件大小。',
    'enumerate_fs_to_staging': '将文件系统条目递归写入 staging 数据库。',
    'create_entry': '创建笔记本条目，支持指定父条目形成线程结构。',
    'add_citation': '为笔记本条目添加引文，关联到文件、工件或其他实体。',
    'record_step': '记录用户操作步骤，用于审计和笔记本复现。',
    'cancel_job': '取消指定的作业，设置取消原因。',
    'recover_interrupted_jobs': '恢复因应用崩溃而中断的作业，将其状态重置为可恢复。',
    'to_markdown': '将导入报告渲染为 Markdown 格式字符串。',
    'pre_import_check': '执行导入前预检，验证源路径、分析目录结构、估计规模并生成导入计划。',
};

const classSummaries = {
    'ImportAnalysisOptions': '导入分析主配置结构体，包含案例路径、数据库路径、工作线程数、内存预算、分析模式（元数据/预算内容/完整内容）等参数。',
    'ImportAnalysisMode': '枚举导入分析模式：MetadataOnly（仅元数据）、BudgetedContent（有预算的内容提取）、FullContent（完整内容）。',
    'ImportJobContext': '导入作业完整上下文，聚合数据库连接、案例/数据源/作业 ID、导入配置、作业仓库和统计计数。',
    'ImportState': '导入状态机，跟踪当前阶段、已处理文件数、最后处理路径、错误列表，支持保存/恢复以实现断点续传。',
    'ImportReport': '导入报告聚合器，收集统计信息、时间线事件、警告和错误，支持将报告渲染为 Markdown。',
    'ImportSourceConfig': '导入源配置，封装源路径、显示名、导入模式和类型（逻辑目录/磁盘镜像）。',
    'TierStateMachine': '三级分析状态机，顺序推进 Catalog → ExtractArtifacts → CorrelateAndIndex 三个阶段。',
    'NtfsMftParams': 'NTFS MFT 解析参数集，包含卷偏移、MFT 起始簇、簇大小、记录大小、扇区字节数和数据运行列表。',
    'PartitionWork': '单个分区的枚举工作项，包含分区索引、名称、文件系统类型和访问路径。',
    'SharedAnalysisState': '跨 Worker 共享的分析状态，包含已处理总数、活跃 Worker 数、已索引总数、内容预算使用量。',
    'PriorityTaskQueue': '基于优先级的任务队列，内部维护高/正常/低三个 VecDeque 并按优先级出队。',
    'FileTask': '抽象的文件分析任务，包含文件 ID、路径、名称、类型、大小、时间戳和哈希等元数据。',
    'Tier': '定义分析流程的三个阶段：Catalog（编目）、ExtractArtifacts（提取工件）、CorrelateAndIndex（关联和索引）。',
    'ImportPhase': '导入流程的阶段枚举：Classifying、Enumerating、PostProcessing、Completed、Failed、Paused。',
    'ImportPlan': '导入计划模型，包含策略选择、时间/内存估计和文件/大小统计。',
    'ImportJobOptions': '导入作业配置，包含 Tauri App 句柄、取消令牌、最大工作线程数和分析模式。',
    'WorkerStats': '单个分析 Worker 的统计计数器，包含已处理、工件、时间线、索引、警告、跳过和失败的数量。',
    'ImportStatistics': '导入统计聚合，包含总文件数、目录数、大小、已导入/跳过/错误文件、哈希、工件、时间线和文本索引计数。',
    'ParallelEnumError': '并行枚举的错误类型，覆盖取消、IO、MFT 参数和数据库错误。',
    'NotebookError': '笔记本服务的错误类型枚举。',
};

// ====== PROCESS EACH FILE ======
for (const entry of files) {
    const filePath = entry.path;
    const fileData = summary.find(s => s.path === filePath);
    if (!fileData) continue;

    const fileName = filePath.split('/').pop();
    const totalLines = fileData.totalLines;
    const nonEmptyLines = fileData.nonEmptyLines;

    let complexity = 'simple';
    if (nonEmptyLines > 200) complexity = 'complex';
    else if (nonEmptyLines > 50) complexity = 'moderate';

    const info = pathToSummary[filePath] || {};
    const fileSummary = info.summary || `app-services crate 中的 Rust 源文件。`;
    const fileTags = info.tags || ['rust', 'service'];

    // Add test tag for test files
    if (fileName.includes('test') || filePath.includes('/tests/')) {
        if (!fileTags.includes('test')) fileTags.push('test');
    }

    // Create file node
    addNode({
        id: `file:${filePath}`,
        type: 'file',
        name: fileName,
        filePath: filePath,
        summary: fileSummary,
        tags: fileTags,
        complexity: complexity
    });

    // Build exported names set
    const exportedNames = new Set((fileData.exports || []).map(e => e.name));

    // Function sub-nodes
    for (const func of (fileData.functions || [])) {
        const isExported = exportedNames.has(func.name);
        if (!isSignificantFunction(func, isExported)) continue;

        const lineCount = func.endLine - func.startLine;
        const fComplexity = lineCount > 30 ? 'complex' : lineCount > 10 ? 'moderate' : 'simple';
        const funcTags = ['function'];
        if (isExported) funcTags.push('exported');
        if (func.name.startsWith('run_') || func.name.startsWith('enumerate_') || func.name.startsWith('execute_')) funcTags.push('orchestration');
        if (func.name.startsWith('emit_')) funcTags.push('event-emission');
        if (func.name.includes('worker')) funcTags.push('worker');
        if (func.name.includes('staging') || func.name.includes('mft')) funcTags.push('staging');
        if (func.name.endsWith('_to_dto') || func.name.endsWith('_from_dto')) funcTags.push('dto-conversion');
        if (func.name.startsWith('read_') && func.name.includes('mft')) funcTags.push('ntfs');
        if (func.name === 'to_markdown' || func.name.includes('report')) funcTags.push('reporting');

        let funcSummary = funcSummaries[func.name] || `Rust function in ${fileName}.`;

        addNode({
            id: `function:${filePath}:${func.name}`,
            type: 'function',
            name: func.name,
            filePath: filePath,
            lineRange: [func.startLine, func.endLine],
            summary: funcSummary,
            tags: funcTags,
            complexity: fComplexity
        });

        addEdge(`file:${filePath}`, `function:${filePath}:${func.name}`, 'contains', 1.0);
        if (isExported) {
            addEdge(`file:${filePath}`, `function:${filePath}:${func.name}`, 'exports', 0.8);
        }
    }

    // Class sub-nodes
    for (const cls of (fileData.classes || [])) {
        const isExported = exportedNames.has(cls.name);
        if (!isSignificantClass(cls, isExported)) continue;

        const clsLineCount = cls.endLine - cls.startLine;
        const cComplexity = clsLineCount > 50 ? 'complex' : clsLineCount > 15 ? 'moderate' : 'simple';
        const clsTags = ['data-model'];
        if (isExported) clsTags.push('exported');
        if (cls.name.endsWith('Error')) clsTags.push('error-handling');
        if (cls.name.endsWith('Options') || cls.name.endsWith('Config')) clsTags.push('configuration');
        if (cls.name.endsWith('Stats') || cls.name.endsWith('Metrics') || cls.name.includes('Telemetry')) clsTags.push('monitoring');
        if (cls.name.includes('Mode') || cls.name.includes('Phase') || cls.name.includes('Strategy') || cls.name.includes('Tier')) clsTags.push('enum');
        if (cls.name.includes('State') || cls.name.includes('Machine')) clsTags.push('state-machine');
        if (cls.name.includes('Queue')) clsTags.push('data-structure');
        if (cls.name.includes('Worker') || cls.name.includes('Partition')) clsTags.push('worker');
        if (cls.name.includes('Budget') || cls.name.includes('Budget')) clsTags.push('resource-management');

        let clsSummary = classSummaries[cls.name] || `${cls.name} — Rust 结构体/枚举，定义于 ${fileName}。`;

        addNode({
            id: `class:${filePath}:${cls.name}`,
            type: 'class',
            name: cls.name,
            filePath: filePath,
            lineRange: [cls.startLine, cls.endLine],
            summary: clsSummary,
            tags: clsTags,
            complexity: cComplexity
        });

        addEdge(`file:${filePath}`, `class:${filePath}:${cls.name}`, 'contains', 1.0);
        if (isExported) {
            addEdge(`file:${filePath}`, `class:${filePath}:${cls.name}`, 'exports', 0.8);
        }
    }
}

// ====== IMPORT EDGES (from batchImportData) ======
let importsEmitted = 0;
for (const filePath of Object.keys(batchImportData)) {
    const imports = batchImportData[filePath];
    for (const importPath of imports) {
        addEdge(`file:${filePath}`, `file:${importPath}`, 'imports', 0.7);
        importsEmitted++;
    }
}

// ====== SEMANTIC EDGES ======

// Barrel module contains sub-modules in the same batch
const importAnalysisMod = 'crates/app-services/src/import_analysis/mod.rs';
const importAnalysisSubsInBatch = [
    'crates/app-services/src/import_analysis/options.rs',
    'crates/app-services/src/import_analysis/priority_queue.rs',
    'crates/app-services/src/import_analysis/progress.rs',
    'crates/app-services/src/import_analysis/task_feed.rs',
    'crates/app-services/src/import_analysis/tier.rs',
    'crates/app-services/src/import_analysis/worker_pool.rs',
    'crates/app-services/src/import_analysis/worker_runtime.rs',
];
for (const sub of importAnalysisSubsInBatch) {
    addEdge(`file:${importAnalysisMod}`, `file:${sub}`, 'contains', 1.0);
}
// cross-batch contains for budget, error, finalize (not in batch 4)
addEdge(`file:${importAnalysisMod}`, 'file:crates/app-services/src/import_analysis/budget.rs', 'contains', 1.0);
addEdge(`file:${importAnalysisMod}`, 'file:crates/app-services/src/import_analysis/error.rs', 'contains', 1.0);
addEdge(`file:${importAnalysisMod}`, 'file:crates/app-services/src/import_analysis/finalize.rs', 'contains', 1.0);

const importPipelineMod = 'crates/app-services/src/import_pipeline/mod.rs';
const importPipelineSubsInBatch = [
    'crates/app-services/src/import_pipeline/emit.rs',
    'crates/app-services/src/import_pipeline/execute.rs',
    'crates/app-services/src/import_pipeline/options.rs',
    'crates/app-services/src/import_pipeline/partition.rs',
    'crates/app-services/src/import_pipeline/phases.rs',
    'crates/app-services/src/import_pipeline/tests.rs',
    'crates/app-services/src/import_pipeline/types.rs',
];
for (const sub of importPipelineSubsInBatch) {
    addEdge(`file:${importPipelineMod}`, `file:${sub}`, 'contains', 1.0);
}

const parallelEnumMod = 'crates/app-services/src/parallel_enum/mod.rs';
const parallelEnumSubsInBatch = [
    'crates/app-services/src/parallel_enum/error.rs',
    'crates/app-services/src/parallel_enum/ntfs_mft.rs',
    'crates/app-services/src/parallel_enum/partition_worker.rs',
    'crates/app-services/src/parallel_enum/progress.rs',
    'crates/app-services/src/parallel_enum/scheduler.rs',
];
for (const sub of parallelEnumSubsInBatch) {
    addEdge(`file:${parallelEnumMod}`, `file:${sub}`, 'contains', 1.0);
}
// cross-batch contains for staging_meta
addEdge(`file:${parallelEnumMod}`, 'file:crates/app-services/src/parallel_enum/staging_meta.rs', 'contains', 1.0);

// notebook_service/mod.rs contains error.rs
addEdge('file:crates/app-services/src/notebook_service/mod.rs', 'file:crates/app-services/src/notebook_service/error.rs', 'contains', 1.0);

// lib.rs contains all top-level service modules in this batch
const libRs = 'crates/app-services/src/lib.rs';
const libSubsInBatch = [
    'crates/app-services/src/import_analysis/mod.rs',
    'crates/app-services/src/import_pipeline/mod.rs',
    'crates/app-services/src/import_precheck.rs',
    'crates/app-services/src/import_report.rs',
    'crates/app-services/src/import_state.rs',
    'crates/app-services/src/job_service.rs',
    'crates/app-services/src/notebook_service/mod.rs',
    'crates/app-services/src/parallel_enum/mod.rs',
];
for (const sub of libSubsInBatch) {
    addEdge(`file:${libRs}`, `file:${sub}`, 'contains', 1.0);
}

// Test files tested_by production
const testsFile = 'crates/app-services/src/import_pipeline/tests.rs';
for (const sub of importPipelineSubsInBatch) {
    if (sub !== testsFile) {
        addEdge(`file:${testsFile}`, `file:${sub}`, 'tested_by', 0.5);
    }
}

// Key depends_on edges between modules in this batch
// worker_pool depends on worker_runtime
addEdge('file:crates/app-services/src/import_analysis/worker_pool.rs', 'file:crates/app-services/src/import_analysis/worker_runtime.rs', 'depends_on', 0.6);
// phases depends on partition for enumeration
addEdge('file:crates/app-services/src/import_pipeline/phases.rs', 'file:crates/app-services/src/import_pipeline/partition.rs', 'depends_on', 0.6);
// phases depends on execute for DTO functions
addEdge('file:crates/app-services/src/import_pipeline/phases.rs', 'file:crates/app-services/src/import_pipeline/execute.rs', 'depends_on', 0.6);
// execute depends on emit
addEdge('file:crates/app-services/src/import_pipeline/execute.rs', 'file:crates/app-services/src/import_pipeline/emit.rs', 'depends_on', 0.6);
// execute depends on phases for phase entry points
addEdge('file:crates/app-services/src/import_pipeline/execute.rs', 'file:crates/app-services/src/import_pipeline/phases.rs', 'depends_on', 0.6);
// partition depends on datasource_service (from neighborMap)
addEdge('file:crates/app-services/src/import_pipeline/partition.rs', 'file:crates/app-services/src/datasource_service.rs', 'depends_on', 0.6);
// scheduler depends on partition_worker
addEdge('file:crates/app-services/src/parallel_enum/scheduler.rs', 'file:crates/app-services/src/parallel_enum/partition_worker.rs', 'depends_on', 0.6);
// scheduler depends on ntfs_mft
addEdge('file:crates/app-services/src/parallel_enum/scheduler.rs', 'file:crates/app-services/src/parallel_enum/ntfs_mft.rs', 'depends_on', 0.6);
// notebook_service depends on step_recorder (neighborMap)
addEdge('file:crates/app-services/src/notebook_service/mod.rs', 'file:crates/app-services/src/step_recorder.rs', 'depends_on', 0.6);

// Cross-function calls
addEdge('function:crates/app-services/src/import_analysis/worker_pool.rs:run_import_analysis_staging', 'function:crates/app-services/src/import_analysis/worker_runtime.rs:run_analysis_worker', 'calls', 0.8);
addEdge('function:crates/app-services/src/import_pipeline/phases.rs:enumerate_image_data_source_with_staging', 'function:crates/app-services/src/import_pipeline/partition.rs:enumerate_image_data_source', 'calls', 0.8);
addEdge('function:crates/app-services/src/import_pipeline/partition.rs:enumerate_image_data_source', 'function:crates/app-services/src/import_pipeline/partition.rs:build_partition_work', 'calls', 0.8);
addEdge('function:crates/app-services/src/import_pipeline/phases.rs:run_post_import_phase', 'function:crates/app-services/src/import_analysis/worker_pool.rs:run_import_analysis_staging', 'calls', 0.8);
addEdge('function:crates/app-services/src/import_pipeline/execute.rs:execute_import_job_with_counts', 'function:crates/app-services/src/import_pipeline/phases.rs:run_attach_phase', 'calls', 0.8);
addEdge('function:crates/app-services/src/import_pipeline/execute.rs:execute_import_job_with_counts', 'function:crates/app-services/src/import_pipeline/phases.rs:run_enumeration_phase', 'calls', 0.8);
addEdge('function:crates/app-services/src/import_pipeline/execute.rs:execute_import_job_with_counts', 'function:crates/app-services/src/import_pipeline/phases.rs:run_post_import_phase', 'calls', 0.8);
addEdge('function:crates/app-services/src/import_pipeline/execute.rs:execute_import_job_with_counts', 'function:crates/app-services/src/import_pipeline/phases.rs:run_finalize_phase', 'calls', 0.8);
addEdge('function:crates/app-services/src/parallel_enum/scheduler.rs:enumerate_partitions_parallel', 'function:crates/app-services/src/parallel_enum/partition_worker.rs:enumerate_single_partition', 'calls', 0.8);

// Write output
const output = { nodes: graphNodes, edges: graphEdges };
writeFileSync('D:/process/forensic/.understand-anything/tmp/batch-4-graph.json', JSON.stringify(output, null, 2), 'utf8');

console.log('=== BATCH 4 GRAPH STATS ===');
console.log('Total nodes:', graphNodes.length);
console.log('  File nodes:', graphNodes.filter(n => n.type === 'file').length);
console.log('  Function nodes:', graphNodes.filter(n => n.type === 'function').length);
console.log('  Class nodes:', graphNodes.filter(n => n.type === 'class').length);
console.log('Total edges:', graphEdges.length);
console.log('  Import edges:', graphEdges.filter(e => e.type === 'imports').length);
console.log('  Contains edges:', graphEdges.filter(e => e.type === 'contains').length);
console.log('  Exports edges:', graphEdges.filter(e => e.type === 'exports').length);
console.log('  Depends_on edges:', graphEdges.filter(e => e.type === 'depends_on').length);
console.log('  Calls edges:', graphEdges.filter(e => e.type === 'calls').length);
console.log('  Tested_by edges:', graphEdges.filter(e => e.type === 'tested_by').length);
console.log('Expected import edges:', importsEmitted);
console.log('Node IDs:', graphNodes.map(n => n.id).join('\n  '));
