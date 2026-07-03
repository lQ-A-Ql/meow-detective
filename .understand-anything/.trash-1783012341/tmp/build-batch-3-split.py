import json, os
from collections import Counter

# Load extraction results
with open('D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-3.json', 'r', encoding='utf-8') as f:
    extract = json.load(f)

# Load batch data
with open('D:/process/forensic/.understand-anything/intermediate/batches.json', 'r') as f:
    batches_data = json.load(f)
batch = batches_data['batches'][2]  # batch 3/5
bid = batch.get('batchImportData', {})
nm = batch.get('neighborMap', {})

# Build file-level node definitions with semantic metadata
file_meta = {
    'frontend/src/app/pages/Artifacts.test.tsx': {
        'summary': 'Artifacts 页面的测试文件，验证 artifacts 家族列表、行数据获取及页面渲染逻辑。',
        'tags': ['test', 'artifacts', 'page-test'],
        'complexity': 'complex',
        'languageNotes': '使用 Vitest + React Testing Library 进行组件测试。',
    },
    'frontend/src/app/pages/Artifacts.tsx': {
        'summary': 'Artifacts 页面组件，展示取证 artifacts 家族分类，支持按家族浏览 artifact 行数据并查看详情。',
        'tags': ['page', 'artifacts', 'api-handler'],
        'complexity': 'complex',
    },
    'frontend/src/app/pages/FileBrowser.test.tsx': {
        'summary': 'FileBrowser 组件的测试文件，验证文件浏览器页面中各组件集成和事件处理逻辑。',
        'tags': ['test', 'file-browser', 'integration-test'],
        'complexity': 'complex',
        'languageNotes': '大型集成测试，覆盖文件浏览器的多个子面板交互。',
    },
    'frontend/src/app/pages/FileBrowser.tsx': {
        'summary': 'FileBrowser 主页面组件，组合文件树、文件列表、预览面板和检查器，作为文件浏览的入口。',
        'tags': ['page', 'file-browser', 'entry-point'],
        'complexity': 'moderate',
    },
    'frontend/src/app/pages/FileBrowserInspector.tsx': {
        'summary': '文件浏览器检查面板组件，展示选中文件的元数据详情，支持跳转到时间线。',
        'tags': ['component', 'file-browser', 'inspector'],
        'complexity': 'moderate',
    },
    'frontend/src/app/pages/FileListPanel.tsx': {
        'summary': '文件列表面板组件，以表格形式展示目录内文件，支持排序、分页和目录导航。',
        'tags': ['component', 'file-browser', 'table'],
        'complexity': 'moderate',
    },
    'frontend/src/app/pages/FileTreePanel.tsx': {
        'summary': '文件树面板组件，展示文件系统树形结构，支持按数据源分组、目录展开折叠和搜索过滤。',
        'tags': ['component', 'file-browser', 'tree'],
        'complexity': 'moderate',
    },
    'frontend/src/app/pages/Search.test.tsx': {
        'summary': 'Search 页面的测试文件，验证搜索输入、结果展示和保存的查询交互逻辑。',
        'tags': ['test', 'search', 'page-test'],
        'complexity': 'moderate',
    },
    'frontend/src/app/pages/Search.tsx': {
        'summary': 'Search 页面组件，提供文件内容与元数据全文搜索，支持保存和选择历史查询。',
        'tags': ['page', 'search', 'api-handler'],
        'complexity': 'complex',
    },
    'frontend/src/app/pages/Timeline.test.tsx': {
        'summary': 'Timeline 页面的测试文件，验证时间线事件渲染、日期筛选和事件柱状图逻辑。',
        'tags': ['test', 'timeline', 'page-test'],
        'complexity': 'moderate',
    },
    'frontend/src/app/pages/Timeline.tsx': {
        'summary': 'Timeline 时间线页面组件，将取证事件按时间轴可视化，支持日期范围筛选和事件详情查看。',
        'tags': ['page', 'timeline', 'visualization'],
        'complexity': 'complex',
    },
    'frontend/src/components/files/FileIconWithStatusOverlay.tsx': {
        'summary': '文件图标组件，根据文件类型和状态（删除/隐藏/系统）渲染带覆盖层标记的图标。',
        'tags': ['component', 'file-browser', 'icon'],
        'complexity': 'simple',
    },
    'frontend/src/components/files/FileVisibilityToggle.tsx': {
        'summary': '文件可见性切换开关组件，用于控制是否显示已删除/隐藏/系统文件。',
        'tags': ['component', 'file-browser', 'toggle'],
        'complexity': 'simple',
    },
    'frontend/src/components/layout/InspectorPane.tsx': {
        'summary': '检查器面板布局组件，提供标题、副标题和结构化键值对展示的通用容器。',
        'tags': ['component', 'layout', 'inspector'],
        'complexity': 'simple',
    },
    'frontend/src/components/layout/PageSubbar.tsx': {
        'summary': '页面副标题栏布局组件，在页面内容区顶部展示标题和操作按钮区域。',
        'tags': ['component', 'layout', 'subheader'],
        'complexity': 'simple',
    },
    'frontend/src/components/tree/TreeConnector.tsx': {
        'summary': '树形连接线组件，为文件树节点渲染层级缩进和垂直连接线指示父子关系。',
        'tags': ['component', 'tree', 'visual'],
        'complexity': 'simple',
    },
    'frontend/src/components/tree/TreeNodeIcon.tsx': {
        'summary': '树节点图标组件，根据文件类型和目录展开状态渲染对应的图标。',
        'tags': ['component', 'tree', 'icon'],
        'complexity': 'simple',
    },
    'frontend/src/components/tree/TreeSearch.tsx': {
        'summary': '树搜索过滤组件，提供带防抖的文本输入框用于过滤文件树中的节点。',
        'tags': ['component', 'tree', 'search'],
        'complexity': 'simple',
    },
    'frontend/src/components/tree/VirtualFileTree.test.tsx': {
        'summary': 'VirtualFileTree 组件的测试文件，验证虚拟滚动树节点的渲染和点击事件。',
        'tags': ['test', 'tree', 'component-test'],
        'complexity': 'simple',
    },
    'frontend/src/components/tree/VirtualFileTree.tsx': {
        'summary': '虚拟滚动文件树组件，使用 TanStack Virtual 实现大量文件节点的高性能渲染。',
        'tags': ['component', 'tree', 'virtual-scroll'],
        'complexity': 'moderate',
    },
    'frontend/src/features/files/hooks/use-file-selection.ts': {
        'summary': '文件选择状态 hook，封装当前选中文件和目录的 Zustand store 访问逻辑。',
        'tags': ['hook', 'file-browser', 'selection'],
        'complexity': 'simple',
    },
    'frontend/src/features/search/hooks.test.ts': {
        'summary': '搜索 hooks 的测试文件，验证 useSearchResults hook 的缓存和查询逻辑。',
        'tags': ['test', 'search', 'hook-test'],
        'complexity': 'moderate',
    },
    'frontend/src/features/search/hooks.ts': {
        'summary': '搜索功能 hook，封装 useQuery 调用搜索 API 并缓存搜索结果。',
        'tags': ['hook', 'search', 'api-handler'],
        'complexity': 'simple',
    },
    'frontend/src/lib/file-icons.ts': {
        'summary': '文件图标映射工具，提供扩展名到图标类型和文件类型标签的查找函数。',
        'tags': ['utility', 'icon', 'mapping'],
        'complexity': 'moderate',
        'languageNotes': '包含超过 100 条扩展名到图标类型的静态映射表。',
    },
    'frontend/src/lib/saved-queries.test.ts': {
        'summary': 'saved-queries 工具的测试文件，验证 localStorage 中搜索查询的增删改查操作。',
        'tags': ['test', 'utility', 'local-storage'],
        'complexity': 'simple',
    },
    'frontend/src/lib/saved-queries.ts': {
        'summary': '保存的搜索查询工具，通过 localStorage 持久化用户的搜索查询历史，支持增删改查。',
        'tags': ['utility', 'search', 'local-storage'],
        'complexity': 'moderate',
    },
    'frontend/src/stores/selection-store.test.ts': {
        'summary': 'selection-store 的测试文件，验证 Zustand store 中文件和目录选择状态的正确性。',
        'tags': ['test', 'state', 'store-test'],
        'complexity': 'simple',
    },
    'frontend/src/stores/selection-store.ts': {
        'summary': '文件选择状态 Zustand store，管理当前选中的文件 ID、目录 ID 和展开的目录集合。',
        'tags': ['state', 'store', 'selection'],
        'complexity': 'simple',
    },
}

# ---- FUNCTION NODES META ----
function_nodes_meta = {
    'frontend/src/app/pages/Artifacts.tsx': [
        {'name': 'Artifacts', 'lineRange': [19, 211],
         'summary': 'Artifacts 页面主组件，渲染 artifacts 家族列表、行数据表格和详情面板。',
         'tags': ['page', 'artifacts', 'react-component'], 'complexity': 'complex', 'exported': True},
        {'name': 'ArtifactField', 'lineRange': [213, 226],
         'summary': '渲染单个 artifact 字段的键值对展示行。',
         'tags': ['component', 'artifacts', 'display'], 'complexity': 'simple', 'exported': False},
    ],
    'frontend/src/app/pages/FileBrowser.tsx': [
        {'name': 'FileBrowser', 'lineRange': [11, 161],
         'summary': 'FileBrowser 主页面组件，组合文件树、列表、预览和检查器子面板。',
         'tags': ['page', 'file-browser', 'react-component'], 'complexity': 'moderate', 'exported': True},
    ],
    'frontend/src/app/pages/FileBrowserInspector.tsx': [
        {'name': 'FileBrowserInspector', 'lineRange': [20, 127],
         'summary': '文件检查器组件，展示选中文件的元数据并提供时间线跳转。',
         'tags': ['component', 'inspector', 'file-browser'], 'complexity': 'moderate', 'exported': True},
    ],
    'frontend/src/app/pages/FileListPanel.tsx': [
        {'name': 'FileListPanel', 'lineRange': [27, 175],
         'summary': '文件列表面板，以可排序分页表格展示目录内文件条目。',
         'tags': ['component', 'file-browser', 'table'], 'complexity': 'moderate', 'exported': True},
    ],
    'frontend/src/app/pages/FileTreePanel.tsx': [
        {'name': 'FileTreePanel', 'lineRange': [33, 184],
         'summary': '文件树面板，展示数据源分组的树形文件结构，支持搜索过滤和虚拟滚动。',
         'tags': ['component', 'file-browser', 'tree'], 'complexity': 'moderate', 'exported': True},
    ],
    'frontend/src/app/pages/Search.tsx': [
        {'name': 'Search', 'lineRange': [19, 253],
         'summary': 'Search 搜索页面主组件，提供全文搜索入口、结果表格和历史查询管理。',
         'tags': ['page', 'search', 'react-component'], 'complexity': 'complex', 'exported': True},
    ],
    'frontend/src/app/pages/Timeline.tsx': [
        {'name': 'buildTimelineBars', 'lineRange': [15, 36],
         'summary': '将时间线事件按时间桶分组，生成用于柱状图渲染的数据结构。',
         'tags': ['utility', 'timeline', 'data-transform'], 'complexity': 'simple', 'exported': False},
        {'name': 'formatTs', 'lineRange': [38, 47],
         'summary': '将输入的日期值格式化为标准时间字符串供日期选择器使用。',
         'tags': ['utility', 'timeline', 'formatting'], 'complexity': 'simple', 'exported': False},
        {'name': 'Timeline', 'lineRange': [62, 361],
         'summary': 'Timeline 时间线页面主组件，渲染事件柱状图、事件表格和日期筛选面板。',
         'tags': ['page', 'timeline', 'react-component'], 'complexity': 'complex', 'exported': True},
    ],
    'frontend/src/components/files/FileIconWithStatusOverlay.tsx': [
        {'name': 'statusTitle', 'lineRange': [16, 26],
         'summary': '根据文件的删除、隐藏和系统标记生成状态提示文本。',
         'tags': ['utility', 'file-browser', 'status'], 'complexity': 'simple', 'exported': False},
        {'name': 'FileIconWithStatusOverlay', 'lineRange': [28, 65],
         'summary': '渲染带状态覆盖层的文件图标，根据扩展名和条目类型选择合适的图标。',
         'tags': ['component', 'file-browser', 'icon'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/components/files/FileVisibilityToggle.tsx': [
        {'name': 'FileVisibilityToggle', 'lineRange': [3, 24],
         'summary': '文件可见性切换开关，控制已删除/隐藏/系统文件的显示状态。',
         'tags': ['component', 'file-browser', 'toggle'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/components/layout/InspectorPane.tsx': [
        {'name': 'InspectorPane', 'lineRange': [10, 20],
         'summary': '检查器面板容器，提供统一的标题栏和可调整宽度的内容区域。',
         'tags': ['component', 'layout', 'inspector'], 'complexity': 'simple', 'exported': True},
        {'name': 'InspectorSection', 'lineRange': [22, 29],
         'summary': '检查器面板中的分组区域，带小标题的内容容器。',
         'tags': ['component', 'layout', 'inspector'], 'complexity': 'simple', 'exported': True},
        {'name': 'InspectorValue', 'lineRange': [31, 43],
         'summary': '检查器面板中的键值展示组件，支持等宽字体和强调样式。',
         'tags': ['component', 'layout', 'inspector'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/components/layout/PageSubbar.tsx': [
        {'name': 'PageSubbar', 'lineRange': [8, 20],
         'summary': '页面副标题栏，在页面顶部显示标题、元信息和操作区域。',
         'tags': ['component', 'layout', 'subheader'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/components/tree/TreeConnector.tsx': [
        {'name': 'TreeConnector', 'lineRange': [14, 46],
         'summary': '树节点连接线组件，根据深度和末位状态渲染层级缩进线。',
         'tags': ['component', 'tree', 'visual'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/components/tree/TreeNodeIcon.tsx': [
        {'name': 'TreeNodeIcon', 'lineRange': [23, 37],
         'summary': '树节点图标组件，根据文件类型和展开状态渲染对应图标。',
         'tags': ['component', 'tree', 'icon'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/components/tree/TreeSearch.tsx': [
        {'name': 'TreeSearch', 'lineRange': [22, 70],
         'summary': '树搜索输入组件，带防抖的文本过滤框用于筛选文件树节点。',
         'tags': ['component', 'tree', 'search'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/components/tree/VirtualFileTree.tsx': [
        {'name': 'VirtualFileTree', 'lineRange': [26, 135],
         'summary': '虚拟滚动文件树，使用 TanStack Virtual 高性能渲染大量树节点。',
         'tags': ['component', 'tree', 'virtual-scroll'], 'complexity': 'moderate', 'exported': True},
    ],
    'frontend/src/features/files/hooks/use-file-selection.ts': [
        {'name': 'useFileSelection', 'lineRange': [3, 17],
         'summary': '文件选择状态 hook，封装选中文件和目录的 Zustand store 操作。',
         'tags': ['hook', 'file-browser', 'selection'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/features/search/hooks.ts': [
        {'name': 'useSearchResults', 'lineRange': [4, 9],
         'summary': '搜索结果 hook，基于 React Query 缓存并获取文件搜索匹配结果。',
         'tags': ['hook', 'search', 'api-handler'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/lib/file-icons.ts': [
        {'name': 'getFileIcon', 'lineRange': [156, 179],
         'summary': '根据文件名和条目类型返回对应的 Lucide 图标组件名称。',
         'tags': ['utility', 'icon', 'mapping'], 'complexity': 'simple', 'exported': True},
        {'name': 'getFileTypeLabel', 'lineRange': [184, 216],
         'summary': '根据文件名扩展名返回可读的文件类型标签。',
         'tags': ['utility', 'icon', 'label'], 'complexity': 'simple', 'exported': True},
    ],
    'frontend/src/lib/saved-queries.ts': [
        {'name': 'readSavedSearchQueries', 'lineRange': [10, 20],
         'summary': '从 localStorage 读取并解析已保存的搜索查询列表，过滤无效条目。',
         'tags': ['utility', 'search', 'local-storage'], 'complexity': 'simple', 'exported': True},
        {'name': 'writeSavedSearchQueries', 'lineRange': [22, 24],
         'summary': '将搜索查询列表序列化存入 localStorage。',
         'tags': ['utility', 'search', 'local-storage'], 'complexity': 'simple', 'exported': True},
        {'name': 'upsertSavedSearchQuery', 'lineRange': [26, 52],
         'summary': '插入或更新已保存的搜索查询，自动记录时间戳并通过 localStorage 持久化。',
         'tags': ['utility', 'search', 'persistence'], 'complexity': 'moderate', 'exported': True},
        {'name': 'removeSavedSearchQuery', 'lineRange': [61, 66],
         'summary': '从已保存的搜索查询列表中按 ID 删除指定查询。',
         'tags': ['utility', 'search', 'persistence'], 'complexity': 'simple', 'exported': True},
        {'name': 'isSavedSearchQuery', 'lineRange': [68, 77],
         'summary': '类型守卫函数，校验 localStorage 中解析的对象是否为有效的已保存查询。',
         'tags': ['utility', 'search', 'validation'], 'complexity': 'simple', 'exported': False},
    ],
}

# Build complete node and edge lists
all_nodes = []
all_edges = []

# ---- FILE NODES ----
batch_files = batch['files']
for file_entry in batch_files:
    p = file_entry['path']
    meta = file_meta.get(p, {})
    node = {
        'id': f'file:{p}',
        'type': 'file',
        'name': p.split('/')[-1],
        'filePath': p,
        'summary': meta.get('summary', f'{p} 源文件'),
        'tags': meta.get('tags', ['source']),
        'complexity': meta.get('complexity', 'simple'),
    }
    if 'languageNotes' in meta:
        node['languageNotes'] = meta['languageNotes']
    all_nodes.append(node)

# ---- FUNCTION NODES ----
for p, funcs in function_nodes_meta.items():
    for fn in funcs:
        fnode = {
            'id': f'function:{p}:{fn["name"]}',
            'type': 'function',
            'name': fn['name'],
            'filePath': p,
            'lineRange': fn['lineRange'],
            'summary': fn['summary'],
            'tags': fn['tags'],
            'complexity': fn['complexity'],
        }
        all_nodes.append(fnode)

# ---- ALL EDGES ----

# 1. contains edges (file -> function)
for p, funcs in function_nodes_meta.items():
    for fn in funcs:
        all_edges.append({
            'source': f'file:{p}',
            'target': f'function:{p}:{fn["name"]}',
            'type': 'contains',
            'direction': 'forward',
            'weight': 1.0,
        })

# 2. imports edges (1:1 from batchImportData)
for p, imports_list in bid.items():
    for target_path in imports_list:
        all_edges.append({
            'source': f'file:{p}',
            'target': f'file:{target_path}',
            'type': 'imports',
            'direction': 'forward',
            'weight': 0.7,
        })

# 3. exports edges (file -> function for exported functions)
for p, funcs in function_nodes_meta.items():
    for fn in funcs:
        if fn['exported']:
            all_edges.append({
                'source': f'file:{p}',
                'target': f'function:{p}:{fn["name"]}',
                'type': 'exports',
                'direction': 'forward',
                'weight': 0.8,
            })

# 4. tested_by edges (production -> test)
test_prod_pairs = [
    ('frontend/src/app/pages/Artifacts.tsx', 'frontend/src/app/pages/Artifacts.test.tsx'),
    ('frontend/src/app/pages/FileBrowser.tsx', 'frontend/src/app/pages/FileBrowser.test.tsx'),
    ('frontend/src/app/pages/Search.tsx', 'frontend/src/app/pages/Search.test.tsx'),
    ('frontend/src/app/pages/Timeline.tsx', 'frontend/src/app/pages/Timeline.test.tsx'),
    ('frontend/src/components/tree/VirtualFileTree.tsx', 'frontend/src/components/tree/VirtualFileTree.test.tsx'),
    ('frontend/src/features/search/hooks.ts', 'frontend/src/features/search/hooks.test.ts'),
    ('frontend/src/lib/saved-queries.ts', 'frontend/src/lib/saved-queries.test.ts'),
    ('frontend/src/stores/selection-store.ts', 'frontend/src/stores/selection-store.test.ts'),
]
for prod, test in test_prod_pairs:
    all_edges.append({
        'source': f'file:{prod}',
        'target': f'file:{test}',
        'type': 'tested_by',
        'direction': 'forward',
        'weight': 0.5,
    })

# 5. calls edges (in-batch, confident)
all_edges.append({
    'source': 'function:frontend/src/components/files/FileIconWithStatusOverlay.tsx:FileIconWithStatusOverlay',
    'target': 'function:frontend/src/lib/file-icons.ts:getFileIcon',
    'type': 'calls',
    'direction': 'forward',
    'weight': 0.8,
})
all_edges.append({
    'source': 'function:frontend/src/app/pages/Search.tsx:Search',
    'target': 'function:frontend/src/features/search/hooks.ts:useSearchResults',
    'type': 'calls',
    'direction': 'forward',
    'weight': 0.8,
})
all_edges.append({
    'source': 'function:frontend/src/app/pages/Timeline.tsx:Timeline',
    'target': 'function:frontend/src/features/timeline/hooks.ts:useTimelineEvents',
    'type': 'calls',
    'direction': 'forward',
    'weight': 0.8,
})
all_edges.append({
    'source': 'function:frontend/src/app/pages/Timeline.tsx:Timeline',
    'target': 'function:frontend/src/features/timeline/hooks.ts:useTimelineEventById',
    'type': 'calls',
    'direction': 'forward',
    'weight': 0.8,
})
all_edges.append({
    'source': 'function:frontend/src/app/pages/Artifacts.tsx:Artifacts',
    'target': 'function:frontend/src/features/artifacts/hooks.ts:useArtifactFamilies',
    'type': 'calls',
    'direction': 'forward',
    'weight': 0.8,
})
all_edges.append({
    'source': 'function:frontend/src/app/pages/Artifacts.tsx:Artifacts',
    'target': 'function:frontend/src/features/artifacts/hooks.ts:useArtifactRows',
    'type': 'calls',
    'direction': 'forward',
    'weight': 0.8,
})
all_edges.append({
    'source': 'function:frontend/src/app/pages/Artifacts.tsx:Artifacts',
    'target': 'function:frontend/src/features/artifacts/hooks.ts:useArtifactFamilyCounts',
    'type': 'calls',
    'direction': 'forward',
    'weight': 0.8,
})

# ---- VALIDATE ----
print(f'Total nodes: {len(all_nodes)}')
print(f'Total edges: {len(all_edges)}')
import_count = sum(1 for e in all_edges if e['type'] == 'imports')
expected = sum(len(v) for v in bid.values())
print(f'Import edges: {import_count} (expected: {expected})')
assert import_count == expected, f'IMPORT COUNT MISMATCH: {import_count} != {expected}'

node_ids = [n['id'] for n in all_nodes]
assert len(node_ids) == len(set(node_ids)), f'DUPLICATE node IDs'
for e in all_edges:
    assert e['source'] != e['target'], f'SELF-REFERENCE: {e["source"]} -> {e["target"]}'

edge_types = Counter(e['type'] for e in all_edges)
print('Edge types:', dict(edge_types))

# ---- SPLIT INTO 2 PARTS ----
# Sort files alphabetically, chunk into 2 groups of ceil(28/2) = 14
sorted_paths = sorted([f['path'] for f in batch_files])
chunk_size = 14
part_files = [
    set(sorted_paths[:chunk_size]),
    set(sorted_paths[chunk_size:]),
]

print(f'\nPart 1 files: {len(part_files[0])}')
print(f'Part 2 files: {len(part_files[1])}')

out_dir = 'D:/process/forensic/.understand-anything/intermediate'

for part_idx in range(2):
    part_num = part_idx + 1
    part_file_set = part_files[part_idx]

    # Nodes whose filePath is in this part
    part_nodes = []
    for n in all_nodes:
        fp = n.get('filePath', '')
        # For file nodes, match directly
        if fp and fp in part_file_set:
            part_nodes.append(n)
            continue
        # For function nodes, check if their parent file is in this part
        # The id format is function:<filePath>:<name>
        if n['type'] == 'function' and fp and fp in part_file_set:
            part_nodes.append(n)

    part_node_ids = set(n['id'] for n in part_nodes)

    # Edges whose source is in this part
    part_edges = [e for e in all_edges if e['source'] in part_node_ids]

    output = {'nodes': part_nodes, 'edges': part_edges}
    filename = f'batch-3-part-{part_num}.json'
    out_path = os.path.join(out_dir, filename)
    with open(out_path, 'w', encoding='utf-8') as f:
        json.dump(output, f, ensure_ascii=False, indent=2)

    print(f'  {filename}: {len(part_nodes)} nodes, {len(part_edges)} edges')

    # Verify
    with open(out_path, 'r', encoding='utf-8') as f:
        v = json.load(f)
    assert len(v['nodes']) == len(part_nodes)
    assert len(v['edges']) == len(part_edges)

# Also remove the single-part file if it exists (we wrote it earlier)
single_path = os.path.join(out_dir, 'batch-3.json')
if os.path.exists(single_path):
    os.remove(single_path)
    print(f'Removed single-part batch-3.json')

print('\nDone!')
