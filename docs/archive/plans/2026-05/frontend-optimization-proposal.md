# 前端优化方案 — 文件树显示与排序

> 归档：2026-05 优化提案快照，仅用于历史追溯，不代表当前文件树实现。

**范围**: FileBrowser 页面、文件树组件、文件列表排序  
**类型**: 仅方案，暂不改动  

---

## 📊 现状分析

### 当前实现

**文件树 (左侧)**:
- 使用扁平化渲染 (`treeNodes.map`)
- 缩进通过 `paddingLeft` 实现
- 仅使用 `ChevronDown/Right` + `Folder` 图标
- 状态标识使用文字 (`locked`, `unsupported`, `queued`)

**文件列表 (右侧)**:
- 使用 `DenseDataTable` 组件
- 列: 名称、大小、修改时间、属性
- **无排序功能** — 按后端返回顺序显示
- 目录和文件混合显示

### 存在的问题

| 问题 | 影响 |
|------|------|
| 文件树视觉层级不清晰 | 深层目录难以区分层级 |
| 缺少连接线 | 层级关系不直观 |
| 图标单一 | 无法快速识别文件类型 |
| 无排序功能 | 文件多时难以定位 |
| 目录/文件未分离 | 目录散落在文件中 |

---

## 🎨 优化方案一：文件树视觉层级

### 1.1 添加层级连接线

**目标**: 类似 VS Code / Explorer 的树形连接线

**实现方案**:

```tsx
// 层级连接线组件
function TreeIndent({ depth, isLast }: { depth: number; isLast: boolean }) {
  return (
    <span className="inline-flex shrink-0">
      {Array.from({ length: depth }, (_, i) => (
        <span
          key={i}
          className="inline-block w-4 border-l border-[#d0d0d0]"
          style={{ marginLeft: i === 0 ? 0 : 0 }}
        />
      ))}
      {depth > 0 && (
        <span className="inline-block w-4 border-b border-[#d0d0d0]" 
              style={{ height: isLast ? '50%' : '100%', borderBottom: isLast ? 'none' : undefined }} />
      )}
    </span>
  );
}
```

**效果**:
```
📁 Root
├── 📁 Folder A
│   ├── 📄 file1.txt
│   └── 📄 file2.txt
└── 📁 Folder B
    └── 📄 file3.txt
```

### 1.2 增强图标系统

**目标**: 根据文件类型显示不同图标

**图标映射**:

| 类型 | 图标 | 颜色 |
|------|------|------|
| 文件夹 | `Folder` | #888 |
| 文件夹 (展开) | `FolderOpen` | #888 |
| 可执行文件 | `Terminal` | #e74c3c |
| 文档 | `FileText` | #3498db |
| 图片 | `Image` | #2ecc71 |
| 压缩包 | `Archive` | #f39c12 |
| 删除文件 | `Trash2` | #95a5a6 |
| 加密分区 | `Lock` | #e67e22 |
| 不支持 | `HelpCircle` | #bdc3c7 |

**实现方案**:

```tsx
function getFileIcon(node: FileTreeNode): { icon: LucideIcon; color: string } {
  // 目录
  if (node.entryType === 'directory') {
    if (node.status === 'locked') return { icon: Lock, color: '#e67e22' };
    if (node.status === 'unsupported') return { icon: HelpCircle, color: '#bdc3c7' };
    return { icon: node.expanded ? FolderOpen : Folder, color: '#888' };
  }
  
  // 文件 - 根据扩展名
  const ext = node.name.split('.').pop()?.toLowerCase();
  const iconMap: Record<string, { icon: LucideIcon; color: string }> = {
    exe: { icon: Terminal, color: '#e74c3c' },
    dll: { icon: Terminal, color: '#e74c3c' },
    txt: { icon: FileText, color: '#3498db' },
    doc: { icon: FileText, color: '#3498db' },
    pdf: { icon: FileText, color: '#e74c3c' },
    jpg: { icon: Image, color: '#2ecc71' },
    png: { icon: Image, color: '#2ecc71' },
    zip: { icon: Archive, color: '#f39c12' },
    // ... 更多映射
  };
  
  return iconMap[ext ?? ''] ?? { icon: File, color: '#888' };
}
```

### 1.3 深度指示器

**目标**: 视觉强化不同层级

**方案**:
- 层级越深，背景色微调 (每层加深 2%)
- 选中项使用左侧色条指示
- 展开/折叠动画

```tsx
// 层级背景色
const depthBg = (depth: number) => {
  const lightness = 98 - depth * 2; // 98%, 96%, 94%, ...
  return `hsl(0, 0%, ${lightness}%)`;
};

// 选中指示器
{node.active && (
  <div className="absolute left-0 top-0 bottom-0 w-0.5 bg-blue-500" />
)}
```

### 1.4 节点计数

**目标**: 显示子节点数量，帮助判断目录大小

```tsx
<span className="ml-auto text-[10px] text-[#999]">
  {node.hasChildren ? `(${childCount})` : ''}
</span>
```

---

## 📋 优化方案二：文件列表排序

### 2.1 排序状态管理

**新增 Store 状态**:

```typescript
// frontend/src/stores/ui-store.ts
interface UiState {
  // ... 现有状态
  
  // 文件列表排序
  fileSortKey: 'name' | 'size' | 'modifiedAt' | 'ext' | 'entryType';
  fileSortDirection: 'asc' | 'desc';
  setFileSortKey: (key: UiState['fileSortKey']) => void;
  toggleFileSortDirection: () => void;
}
```

### 2.2 默认排序规则

**推荐默认排序**:

```
1. 目录优先 (directories first)
2. 按名称排序 (case-insensitive)
3. 升序 (A → Z)
```

**排序逻辑**:

```typescript
function sortFileEntries(rows: FileEntryRow[], sortKey: string, direction: 'asc' | 'desc'): FileEntryRow[] {
  return [...rows].sort((a, b) => {
    // 1. 目录优先
    if (a.entryType !== b.entryType) {
      return a.entryType === 'directory' ? -1 : 1;
    }
    
    // 2. 按指定字段排序
    let comparison = 0;
    switch (sortKey) {
      case 'name':
        comparison = a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
        break;
      case 'size':
        comparison = (a.size ?? 0) - (b.size ?? 0);
        break;
      case 'modifiedAt':
        comparison = (a.modifiedAt ?? '').localeCompare(b.modifiedAt ?? '');
        break;
      case 'ext':
        comparison = (a.ext ?? '').localeCompare(b.ext ?? '');
        break;
      default:
        comparison = 0;
    }
    
    return direction === 'asc' ? comparison : -comparison;
  });
}
```

### 2.3 表头排序交互

**UI 实现**:

```tsx
// 列头点击排序
<TableHead 
  className="cursor-pointer hover:bg-[#f0f0f0]"
  onClick={() => handleSort('name')}
>
  <div className="flex items-center gap-1">
    名称
    {sortKey === 'name' && (
      <ArrowUpDown size={10} className={sortDirection === 'asc' ? 'rotate-180' : ''} />
    )}
  </div>
</TableHead>
```

**排序指示器**:
- 当前排序列显示箭头图标
- 升序: ↑ (或 ArrowUp)
- 降序: ↓ (或 ArrowDown)

### 2.4 排序持久化

**方案**: 将排序偏好保存到 localStorage

```typescript
// 保存
localStorage.setItem('fileSortKey', sortKey);
localStorage.setItem('fileSortDirection', sortDirection);

// 加载
const savedKey = localStorage.getItem('fileSortKey') ?? 'name';
const savedDirection = localStorage.getItem('fileSortDirection') ?? 'asc';
```

---

## 🎯 优化方案三：交互增强

### 3.1 键盘导航

**支持快捷键**:

| 快捷键 | 功能 |
|--------|------|
| `↑` / `↓` | 上下移动 |
| `←` | 折叠目录 / 返回上级 |
| `→` | 展开目录 / 进入子目录 |
| `Enter` | 打开文件 / 展开目录 |
| `Home` | 跳转到第一个 |
| `End` | 跳转到最后一个 |

### 3.2 右键菜单

**文件树右键菜单**:

```
展开全部
折叠全部
复制路径
在时间线中查看
提取文件
```

**文件列表右键菜单**:

```
打开
复制路径
复制名称
在时间线中查看
属性
```

### 3.3 搜索过滤

**方案**: 在文件树上方添加搜索框

```tsx
<div className="px-2 py-1 border-b border-[#e0e0e0]">
  <input
    type="text"
    placeholder="过滤目录..."
    value={filterQuery}
    onChange={(e) => setFilterQuery(e.target.value)}
    className="w-full px-2 py-1 text-[11px] border rounded"
  />
</div>
```

**过滤逻辑**:
- 实时过滤，输入即过滤
- 支持模糊匹配
- 高亮匹配文字

### 3.4 拖拽调整宽度

**方案**: 左侧文件树支持拖拽调整宽度

```tsx
<div 
  className="w-56 border-r border-[#e0e0e0] bg-[#fafafa] flex flex-col shrink-0"
  style={{ width: treeWidth }}
>
  {/* ... */}
  <div 
    className="absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-blue-200"
    onMouseDown={handleResizeStart}
  />
</div>
```

---

## 📐 优化方案四：性能优化

### 4.1 虚拟滚动

**问题**: 文件树节点过多时渲染卡顿

**方案**: 使用虚拟滚动只渲染可见节点

```tsx
import { useVirtualizer } from '@tanstack/react-virtual';

function VirtualFileTree({ nodes }: { nodes: FileTreeNode[] }) {
  const parentRef = useRef<HTMLDivElement>(null);
  
  const virtualizer = useVirtualizer({
    count: nodes.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28, // 每行高度
    overscan: 10,
  });
  
  return (
    <div ref={parentRef} className="flex-1 overflow-auto">
      <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const node = nodes[virtualRow.index];
          return (
            <div
              key={node.id}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              <TreeNodeItem node={node} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
```

### 4.2 懒加载优化

**当前**: 切换目录时才加载子节点

**优化**: 预加载相邻目录

```typescript
// 预加载展开目录的子节点
useEffect(() => {
  expandedDirectoryIds.forEach((id) => {
    if (!treeChildren[id]) {
      // 预加载
      queryClient.prefetchQuery({
        queryKey: ['files', 'children', id],
        queryFn: () => getFileChildren(id),
      });
    }
  });
}, [expandedDirectoryIds]);
```

### 4.3 记忆化优化

**方案**: 使用 `useMemo` 缓存计算结果

```typescript
// 排序结果缓存
const sortedRows = useMemo(() => {
  if (!rows) return [];
  return sortFileEntries(rows, sortKey, sortDirection);
}, [rows, sortKey, sortDirection]);

// 过滤结果缓存
const filteredTree = useMemo(() => {
  if (!filterQuery) return treeNodes;
  return treeNodes.filter((node) => 
    node.name.toLowerCase().includes(filterQuery.toLowerCase())
  );
}, [treeNodes, filterQuery]);
```

---

## 🎨 UI 样式规范

### 颜色方案

| 元素 | 颜色 | 用途 |
|------|------|------|
| 层级连接线 | `#d0d0d0` | 树形连接线 |
| 选中背景 | `#e0e8f0` | 选中行背景 |
| 选中边框 | `#3b82f6` | 左侧色条 |
| 目录图标 | `#888` | 普通目录 |
| 加密目录 | `#e67e22` | BitLocker 分区 |
| 不支持 | `#bdc3c7` | 不支持的分区 |
| 删除文件 | `#95a5a6` | 已删除文件 |

### 尺寸规范

| 元素 | 尺寸 | 说明 |
|------|------|------|
| 树节点高度 | 28px | 固定行高 |
| 缩进宽度 | 16px/层 | 每层缩进 |
| 图标大小 | 12px | 统一图标 |
| 字体大小 | 11px | 等宽字体 |
| 连接线宽度 | 1px | 细线 |

### 动画规范

| 动画 | 时长 | 缓动 |
|------|------|------|
| 展开/折叠 | 150ms | ease-in-out |
| 选中高亮 | 100ms | ease |
| 悬停效果 | 100ms | ease |

---

## 📋 实施计划

### Phase 1: 核心视觉优化 (2 天)

| 任务 | 工时 | 优先级 |
|------|------|--------|
| 添加层级连接线 | 0.5 天 | P0 |
| 增强图标系统 | 0.5 天 | P0 |
| 目录/文件分离排序 | 0.5 天 | P0 |
| 表头排序交互 | 0.5 天 | P0 |

### Phase 2: 交互增强 (2 天)

| 任务 | 工时 | 优先级 |
|------|------|--------|
| 键盘导航 | 1 天 | P1 |
| 搜索过滤 | 0.5 天 | P1 |
| 排序持久化 | 0.5 天 | P1 |

### Phase 3: 高级功能 (2 天)

| 任务 | 工时 | 优先级 |
|------|------|--------|
| 右键菜单 | 1 天 | P2 |
| 拖拽调整宽度 | 0.5 天 | P2 |
| 虚拟滚动 | 0.5 天 | P2 |

### Phase 4: 打磨优化 (1 天)

| 任务 | 工时 | 优先级 |
|------|------|--------|
| 动画效果 | 0.5 天 | P3 |
| 预加载优化 | 0.5 天 | P3 |

---

## ✅ 验收标准

### 视觉验收

- [ ] 层级连接线清晰可见
- [ ] 不同文件类型有对应图标
- [ ] 选中状态有明确视觉反馈
- [ ] 目录优先显示

### 功能验收

- [ ] 点击表头可排序
- [ ] 排序方向可切换
- [ ] 排序偏好可持久化
- [ ] 键盘导航可用
- [ ] 搜索过滤可用

### 性能验收

- [ ] 1000+ 节点无卡顿
- [ ] 排序操作 < 100ms
- [ ] 过滤操作实时响应

---

**方案版本**: v1.0  
**制定人**: MiMo AI Assistant  
**日期**: 2026-05-30
