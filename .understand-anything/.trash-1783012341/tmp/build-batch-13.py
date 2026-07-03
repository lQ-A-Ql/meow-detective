import json
import sys

output = {
    'nodes': [],
    'edges': []
}

# ============================================================
# FILE NODES (28)
# ============================================================
files = [
    {
        'path': 'frontend/src/features/reports/hooks.ts',
        'name': 'hooks.ts',
        'type': 'file',
        'summary': '报告模块的 React Query hooks，封装 useReportTemplates 和 useReportHistory 查询，为报告页面提供模板列表和历史记录的数据获取能力。',
        'tags': ['hook', 'reports', 'react-query', 'api-consumer'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/features/timeline/hooks.test.ts',
        'name': 'hooks.test.ts',
        'type': 'file',
        'summary': '时间线 hooks 的单元测试文件，验证 useTimelineEvents 和 useTimelineEventById 的查询键构造、启用条件和参数传递。',
        'tags': ['test', 'timeline', 'hooks', 'unit-test'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/features/timeline/hooks.ts',
        'name': 'hooks.ts',
        'type': 'file',
        'summary': '时间线模块的 React Query hooks，提供 useTimelineEvents（带缓存的批量事件查询）和 useTimelineEventById（单事件条件查询）两个数据获取接口。',
        'tags': ['hook', 'timeline', 'react-query', 'api-consumer'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/hooks/use-file-tree-keyboard.ts',
        'name': 'use-file-tree-keyboard.ts',
        'type': 'file',
        'summary': '文件树键盘导航自定义 Hook，支持方向键上下移动、左右展开/折叠目录、Enter 打开文件、Home/End 跳转首尾，并自动处理虚拟滚动容器的滚动跟随。',
        'tags': ['hook', 'keyboard-navigation', 'file-tree', 'accessibility'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/hooks/use-resizable-height.ts',
        'name': 'use-resizable-height.ts',
        'type': 'file',
        'summary': '可调整面板高度 Hook，支持鼠标拖拽调整高度、最小/最大高度限制和 localStorage 持久化保存，用于底部抽屉等垂直可调面板。',
        'tags': ['hook', 'resizable', 'ui-component', 'localstorage'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/hooks/use-resizable-panel.ts',
        'name': 'use-resizable-panel.ts',
        'type': 'file',
        'summary': '可调整面板宽度 Hook，支持鼠标拖拽调整宽度、最小/最大宽度限制和 localStorage 持久化保存，用于侧边栏等水平可调面板。',
        'tags': ['hook', 'resizable', 'ui-component', 'localstorage'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/i18n/index.ts',
        'name': 'index.ts',
        'type': 'file',
        'summary': '国际化初始化入口，配置 i18next 与 react-i18next，加载中英文翻译资源，默认使用中文并设置英文为回退语言，关闭 Suspense 模式。',
        'tags': ['i18n', 'entry-point', 'configuration', 'localization'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/analysis.test.ts',
        'name': 'analysis.test.ts',
        'type': 'file',
        'summary': '分析模块 API 的单元测试文件，覆盖系统信息获取、文件分类、证据分类、各类提取摘要和治理快照等 15 个 API 函数的调用验证。',
        'tags': ['test', 'api-test', 'analysis', 'unit-test'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/analysis.ts',
        'name': 'analysis.ts',
        'type': 'file',
        'summary': '分析模块的 API 层，封装 15 个 Tauri 命令调用，涵盖系统信息、文件分类、注册表/浏览器/邮件/EVTX/Linux 工件提取和治理快照等分析功能。',
        'tags': ['api-layer', 'analysis', 'tauri-command', 'serialization'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/artifacts.test.ts',
        'name': 'artifacts.test.ts',
        'type': 'file',
        'summary': '工件模块 API 的单元测试文件，验证 getArtifactFamilies、getArtifactRows、getArtifactById 和 getArtifactFamilyCounts 四个接口的调用。',
        'tags': ['test', 'api-test', 'artifacts', 'unit-test'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/artifacts.ts',
        'name': 'artifacts.ts',
        'type': 'file',
        'summary': '工件模块的 API 层，封装 4 个薄层 Tauri 命令调用，提供工件族列表、行数据、单工件详情和统计计数的数据获取。',
        'tags': ['api-layer', 'artifacts', 'tauri-command', 'serialization'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/batch.test.ts',
        'name': 'batch.test.ts',
        'type': 'file',
        'summary': '批处理模块 API 的单元测试文件，验证 createBatchPlan、startBatch、pauseBatch、resumeBatch、cancelBatch、getBatchJob 和 listBatchJobs 七个接口。',
        'tags': ['test', 'api-test', 'batch', 'unit-test'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/batch.ts',
        'name': 'batch.ts',
        'type': 'file',
        'summary': '批处理模块的 API 层，封装 7 个 Tauri 命令调用，提供批量作业的创建计划、启动、暂停、恢复、取消和查询等生命周期管理功能。',
        'tags': ['api-layer', 'batch', 'tauri-command', 'job-management'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/case.test.ts',
        'name': 'case.test.ts',
        'type': 'file',
        'summary': '案件模块 API 的单元测试文件，覆盖案件创建/打开/关闭/删除、数据源管理、指标查询和最近对象等 13 个 API 函数。',
        'tags': ['test', 'api-test', 'case', 'unit-test'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/case.ts',
        'name': 'case.ts',
        'type': 'file',
        'summary': '案件模块的 API 层，封装 13 个 Tauri 命令调用，提供案件的完整生命周期管理（创建、打开、关闭、删除）以及数据源和指标查询。',
        'tags': ['api-layer', 'case', 'tauri-command', 'case-management'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/client.test.ts',
        'name': 'client.test.ts',
        'type': 'file',
        'summary': 'API 客户端模块的单元测试，验证 ApiClient 类的错误处理逻辑，包括 toApiError 对不同错误类型的规范化转换。',
        'tags': ['test', 'api-client-test', 'unit-test'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/client.ts',
        'name': 'client.ts',
        'type': 'file',
        'summary': 'Tauri 命令调用的统一客户端封装，包含错误规范化（toApiError）、类型守卫（isApiErrorDto）和 ApiClient 请求类，提供统一的 IPC 调用入口和错误映射。',
        'tags': ['api-client', 'error-handling', 'tauri', 'type-guard'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/commands.test.ts',
        'name': 'commands.test.ts',
        'type': 'file',
        'summary': '命令常量模块的验证测试，从后端 Rust handler 源码中解析命令字符串并验证与 COMMANDS 常量定义的一致性，确保前后端命令名契约同步。',
        'tags': ['test', 'commands-test', 'validation', 'contract-test'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/commands.ts',
        'name': 'commands.ts',
        'type': 'file',
        'summary': '所有 Tauri 命令名的唯一真实来源（SSOT），按领域（case/files/jobs/settings/timeline/search/artifacts/reports/graph/analysis/rulePacks/mcp/batch/notebook）组织为常量映射，避免各处使用裸字符串。',
        'tags': ['constants', 'tauri-commands', 'barrel', 'contract'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/files.test.ts',
        'name': 'files.test.ts',
        'type': 'file',
        'summary': '文件模块 API 的全面单元测试，覆盖文件树、行数据、分页、导入、预览（文本/图片/媒体）、提取和跳转上下文等 15 个 API 函数的调用验证。',
        'tags': ['test', 'api-test', 'files', 'unit-test'],
        'complexity': 'complex'
    },
    {
        'path': 'frontend/src/lib/api/files.ts',
        'name': 'files.ts',
        'type': 'file',
        'summary': '文件模块的 API 层，封装 15 个 Tauri 命令调用，提供文件树浏览、分页查询、数据源导入、文件预览（文本/图片/媒体）、范围读取和文件导出等完整的文件操作接口。',
        'tags': ['api-layer', 'files', 'tauri-command', 'file-browser'],
        'complexity': 'complex'
    },
    {
        'path': 'frontend/src/lib/api/graph.test.ts',
        'name': 'graph.test.ts',
        'type': 'file',
        'summary': '图谱模块 API 的单元测试文件，验证 getGraphSnapshot、queryGraph、getNodeNeighborhood 和 getProvenanceChain 四个图查询接口的调用。',
        'tags': ['test', 'api-test', 'graph', 'unit-test'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/graph.ts',
        'name': 'graph.ts',
        'type': 'file',
        'summary': '图谱模块的 API 层，封装 4 个薄层 Tauri 命令调用，提供知识图谱快照、查询、节点邻域和溯源链的数据获取。',
        'tags': ['api-layer', 'graph', 'tauri-command', 'knowledge-graph'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/jobs.test.ts',
        'name': 'jobs.test.ts',
        'type': 'file',
        'summary': '作业模块 API 的单元测试文件，验证 getJobsSnapshot、getWarnings 和 getTraceItems 三个作业监控接口的调用。',
        'tags': ['test', 'api-test', 'jobs', 'unit-test'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/jobs.ts',
        'name': 'jobs.ts',
        'type': 'file',
        'summary': '作业模块的 API 层，封装 3 个薄层 Tauri 命令调用，提供作业快照、警告列表和追踪项的实时监控数据获取。',
        'tags': ['api-layer', 'jobs', 'tauri-command', 'monitoring'],
        'complexity': 'simple'
    },
    {
        'path': 'frontend/src/lib/api/mcp.test.ts',
        'name': 'mcp.test.ts',
        'type': 'file',
        'summary': 'MCP 客户端 API 的全面单元测试文件（299行），覆盖 MCP 配置管理、服务器增删连接、工具/资源/提示调用等完整 MCP 协议交互的测试场景。',
        'tags': ['test', 'api-test', 'mcp', 'unit-test'],
        'complexity': 'complex'
    },
    {
        'path': 'frontend/src/lib/api/notebook.test.ts',
        'name': 'notebook.test.ts',
        'type': 'file',
        'summary': '笔记本模块 API 的单元测试文件，覆盖笔记条目的 CRUD 操作、证据引用添加和调查步骤查询等 6 个 API 函数。',
        'tags': ['test', 'api-test', 'notebook', 'unit-test'],
        'complexity': 'moderate'
    },
    {
        'path': 'frontend/src/lib/api/notebook.ts',
        'name': 'notebook.ts',
        'type': 'file',
        'summary': '笔记本模块的 API 层，封装 6 个 Tauri 命令调用，提供调查笔记条目的查询、创建、更新以及证据引用关联和调查步骤管理功能。',
        'tags': ['api-layer', 'notebook', 'tauri-command', 'investigation'],
        'complexity': 'moderate'
    },
]

for f in files:
    output['nodes'].append({
        'id': 'file:' + f['path'],
        'type': 'file',
        'name': f['name'],
        'filePath': f['path'],
        'summary': f['summary'],
        'tags': f['tags'],
        'complexity': f['complexity']
    })

# ============================================================
# FUNCTION NODES (5)
# ============================================================
funcs = [
    {
        'id': 'function:frontend/src/lib/api/client.ts:toApiError',
        'name': 'toApiError',
        'filePath': 'frontend/src/lib/api/client.ts',
        'lineRange': [4, 34],
        'summary': '将各种类型的错误（字符串、Error 对象、未知类型）规范化为统一的 ApiErrorDto 结构，确保前端所有 API 调用获得一致格式的错误信息。',
        'tags': ['error-handling', 'normalization', 'utility'],
        'complexity': 'moderate'
    },
    {
        'id': 'function:frontend/src/lib/api/client.ts:isApiErrorDto',
        'name': 'isApiErrorDto',
        'filePath': 'frontend/src/lib/api/client.ts',
        'lineRange': [36, 46],
        'summary': 'TypeScript 类型守卫，运行时检查一个值是否符合 ApiErrorDto 接口结构（code、message 为字符串，可选的 category 和 recoverable 字段）。',
        'tags': ['type-guard', 'validation', 'error-handling'],
        'complexity': 'simple'
    },
    {
        'id': 'function:frontend/src/hooks/use-file-tree-keyboard.ts:useFileTreeKeyboard',
        'name': 'useFileTreeKeyboard',
        'filePath': 'frontend/src/hooks/use-file-tree-keyboard.ts',
        'lineRange': [34, 140],
        'summary': '实现文件树的完整键盘导航逻辑：方向键移动选择、左右键展开折叠目录、Enter 打开文件、Home/End 跳转首尾，内置虚拟滚动自动跟随和节点索引定位。',
        'tags': ['hook', 'keyboard-navigation', 'file-tree', 'accessibility', 'virtual-scroll'],
        'complexity': 'complex'
    },
    {
        'id': 'function:frontend/src/hooks/use-resizable-height.ts:useResizableHeight',
        'name': 'useResizableHeight',
        'filePath': 'frontend/src/hooks/use-resizable-height.ts',
        'lineRange': [32, 86],
        'summary': '实现垂直方向拖拽调整面板高度，支持最小/最大高度钳制、localStorage 持久化状态恢复和基于鼠标事件的平滑拖拽体验。',
        'tags': ['hook', 'resizable', 'drag', 'localstorage', 'ui-state'],
        'complexity': 'moderate'
    },
    {
        'id': 'function:frontend/src/hooks/use-resizable-panel.ts:useResizablePanel',
        'name': 'useResizablePanel',
        'filePath': 'frontend/src/hooks/use-resizable-panel.ts',
        'lineRange': [32, 87],
        'summary': '实现水平方向拖拽调整面板宽度，支持最小/最大宽度钳制、localStorage 持久化状态恢复和基于鼠标事件的平滑拖拽体验。',
        'tags': ['hook', 'resizable', 'drag', 'localstorage', 'ui-state'],
        'complexity': 'moderate'
    },
]

for fn in funcs:
    output['nodes'].append({
        'id': fn['id'],
        'type': 'function',
        'name': fn['name'],
        'filePath': fn['filePath'],
        'lineRange': fn['lineRange'],
        'summary': fn['summary'],
        'tags': fn['tags'],
        'complexity': fn['complexity']
    })

# ============================================================
# CLASS NODES (1)
# ============================================================
output['nodes'].append({
    'id': 'class:frontend/src/lib/api/client.ts:ApiClient',
    'type': 'class',
    'name': 'ApiClient',
    'filePath': 'frontend/src/lib/api/client.ts',
    'lineRange': [55, 66],
    'summary': 'Tauri 命令调用的统一封装类，通过 request 方法将所有 IPC 调用集中处理，自动捕获异常并转换为规范化的 ApiErrorDto 结构。',
    'tags': ['api-client', 'tauri', 'error-handling', 'singleton'],
    'complexity': 'simple'
})

# ============================================================
# IMPORT EDGES - from batchImportData (63 edges)
# ============================================================
import_data = {
    'frontend/src/features/reports/hooks.ts': [
        'frontend/src/lib/api/reports.ts'
    ],
    'frontend/src/features/timeline/hooks.test.ts': [
        'frontend/src/features/timeline/hooks.ts'
    ],
    'frontend/src/features/timeline/hooks.ts': [
        'frontend/src/features/cache-invalidation.ts',
        'frontend/src/lib/api/timeline.ts'
    ],
    'frontend/src/hooks/use-file-tree-keyboard.ts': [
        'frontend/src/lib/constants.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/hooks/use-resizable-height.ts': [],
    'frontend/src/hooks/use-resizable-panel.ts': [],
    'frontend/src/i18n/index.ts': [
        'frontend/src/i18n/en.json',
        'frontend/src/i18n/zh-CN.json'
    ],
    'frontend/src/lib/api/analysis.test.ts': [
        'frontend/src/lib/api/analysis.ts',
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts'
    ],
    'frontend/src/lib/api/analysis.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/artifacts.test.ts': [
        'frontend/src/lib/api/artifacts.ts',
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts'
    ],
    'frontend/src/lib/api/artifacts.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/batch.test.ts': [
        'frontend/src/lib/api/batch.ts',
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts'
    ],
    'frontend/src/lib/api/batch.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/case.test.ts': [
        'frontend/src/lib/api/case.ts',
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts'
    ],
    'frontend/src/lib/api/case.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/client.test.ts': [
        'frontend/src/lib/api/client.ts'
    ],
    'frontend/src/lib/api/client.ts': [
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/commands.test.ts': [
        'frontend/src/lib/api/commands.ts'
    ],
    'frontend/src/lib/api/commands.ts': [],
    'frontend/src/lib/api/files.test.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/lib/api/files.ts'
    ],
    'frontend/src/lib/api/files.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/lib/file-sort.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/graph.test.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/lib/api/graph.ts'
    ],
    'frontend/src/lib/api/graph.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/jobs.test.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/lib/api/jobs.ts'
    ],
    'frontend/src/lib/api/jobs.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/types/models.ts'
    ],
    'frontend/src/lib/api/mcp.test.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/lib/api/mcp.ts'
    ],
    'frontend/src/lib/api/notebook.test.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/lib/api/notebook.ts'
    ],
    'frontend/src/lib/api/notebook.ts': [
        'frontend/src/lib/api/client.ts',
        'frontend/src/lib/api/commands.ts',
        'frontend/src/types/models.ts'
    ],
}


def get_target_prefix(path):
    """Determine node type prefix for a target path based on file type."""
    if path.endswith('.json') and ('i18n' in path or 'locale' in path):
        return 'config'
    return 'file'


for source_path, imports in import_data.items():
    for target_path in imports:
        prefix = get_target_prefix(target_path)
        output['edges'].append({
            'source': 'file:' + source_path,
            'target': prefix + ':' + target_path,
            'type': 'imports',
            'direction': 'forward',
            'weight': 0.7
        })

# ============================================================
# CONTAINS EDGES (file -> function/class)
# ============================================================
contains_edges = [
    ('file:frontend/src/lib/api/client.ts', 'function:frontend/src/lib/api/client.ts:toApiError'),
    ('file:frontend/src/lib/api/client.ts', 'function:frontend/src/lib/api/client.ts:isApiErrorDto'),
    ('file:frontend/src/lib/api/client.ts', 'class:frontend/src/lib/api/client.ts:ApiClient'),
    ('file:frontend/src/hooks/use-file-tree-keyboard.ts', 'function:frontend/src/hooks/use-file-tree-keyboard.ts:useFileTreeKeyboard'),
    ('file:frontend/src/hooks/use-resizable-height.ts', 'function:frontend/src/hooks/use-resizable-height.ts:useResizableHeight'),
    ('file:frontend/src/hooks/use-resizable-panel.ts', 'function:frontend/src/hooks/use-resizable-panel.ts:useResizablePanel'),
]

for src, tgt in contains_edges:
    output['edges'].append({
        'source': src,
        'target': tgt,
        'type': 'contains',
        'direction': 'forward',
        'weight': 1.0
    })

# ============================================================
# EXPORTS EDGES (file -> exported function/class)
# ============================================================
exports_edges = [
    ('file:frontend/src/lib/api/client.ts', 'function:frontend/src/lib/api/client.ts:isApiErrorDto'),
    ('file:frontend/src/lib/api/client.ts', 'class:frontend/src/lib/api/client.ts:ApiClient'),
    ('file:frontend/src/hooks/use-file-tree-keyboard.ts', 'function:frontend/src/hooks/use-file-tree-keyboard.ts:useFileTreeKeyboard'),
    ('file:frontend/src/hooks/use-resizable-height.ts', 'function:frontend/src/hooks/use-resizable-height.ts:useResizableHeight'),
    ('file:frontend/src/hooks/use-resizable-panel.ts', 'function:frontend/src/hooks/use-resizable-panel.ts:useResizablePanel'),
]

for src, tgt in exports_edges:
    output['edges'].append({
        'source': src,
        'target': tgt,
        'type': 'exports',
        'direction': 'forward',
        'weight': 0.8
    })

# ============================================================
# TESTED_BY EDGES (production -> test)
# ============================================================
test_pairs = [
    ('frontend/src/features/timeline/hooks.ts', 'frontend/src/features/timeline/hooks.test.ts'),
    ('frontend/src/lib/api/analysis.ts', 'frontend/src/lib/api/analysis.test.ts'),
    ('frontend/src/lib/api/artifacts.ts', 'frontend/src/lib/api/artifacts.test.ts'),
    ('frontend/src/lib/api/batch.ts', 'frontend/src/lib/api/batch.test.ts'),
    ('frontend/src/lib/api/case.ts', 'frontend/src/lib/api/case.test.ts'),
    ('frontend/src/lib/api/client.ts', 'frontend/src/lib/api/client.test.ts'),
    ('frontend/src/lib/api/commands.ts', 'frontend/src/lib/api/commands.test.ts'),
    ('frontend/src/lib/api/files.ts', 'frontend/src/lib/api/files.test.ts'),
    ('frontend/src/lib/api/graph.ts', 'frontend/src/lib/api/graph.test.ts'),
    ('frontend/src/lib/api/jobs.ts', 'frontend/src/lib/api/jobs.test.ts'),
    ('frontend/src/lib/api/mcp.ts', 'frontend/src/lib/api/mcp.test.ts'),
    ('frontend/src/lib/api/notebook.ts', 'frontend/src/lib/api/notebook.test.ts'),
]

for prod, test in test_pairs:
    output['edges'].append({
        'source': 'file:' + prod,
        'target': 'file:' + test,
        'type': 'tested_by',
        'direction': 'forward',
        'weight': 0.5
    })

# ============================================================
# VALIDATION
# ============================================================
node_count = len(output['nodes'])
edge_count = len(output['edges'])
import_edge_count = sum(1 for e in output['edges'] if e['type'] == 'imports')

print('Total nodes:', node_count)
print('Total edges:', edge_count)
print('Import edges:', import_edge_count)

# Verify node IDs are unique
node_ids = set()
for n in output['nodes']:
    nid = n['id']
    if nid in node_ids:
        print('ERROR: Duplicate node ID:', nid)
        sys.exit(1)
    node_ids.add(nid)

# Verify all edges reference valid nodes or cross-batch paths
all_node_ids = {n['id'] for n in output['nodes']}
valid_prefixes = {'file:', 'config:', 'schema:', 'service:', 'pipeline:', 'resource:', 'table:', 'endpoint:', 'document:', 'function:', 'class:'}

edge_issues = []
for e in output['edges']:
    src = e['source']
    tgt = e['target']
    for endpoint in [src, tgt]:
        if endpoint not in all_node_ids:
            is_valid = any(endpoint.startswith(p) for p in valid_prefixes)
            if not is_valid:
                edge_issues.append('Edge endpoint not valid: ' + endpoint)

if edge_issues:
    print('WARNING:', len(edge_issues), 'edge issues (may be cross-batch):')
    for issue in edge_issues[:5]:
        print(' ', issue)
else:
    print('All edges reference known nodes or valid cross-batch paths.')

# No self-referencing edges
for e in output['edges']:
    if e['source'] == e['target']:
        print('ERROR: Self-referencing edge:', e)
        sys.exit(1)

# Verify no duplicate edges (same source, target, type)
edge_keys = set()
for e in output['edges']:
    key = (e['source'], e['target'], e['type'])
    if key in edge_keys:
        print('ERROR: Duplicate edge:', key)
        sys.exit(1)
    edge_keys.add(key)

print('All validation checks passed.')

# ============================================================
# WRITE OUTPUT
# ============================================================
out_path = 'D:/process/forensic/.understand-anything/intermediate/batch-13.json'
with open(out_path, 'w', encoding='utf-8') as f:
    json.dump(output, f, indent=2, ensure_ascii=False)

print('Written to', out_path)
print('Part check: nodeCount=%d <= 60, edgeCount=%d <= 120 => single file' % (node_count, edge_count))
