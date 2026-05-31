# 前端优化实施计划 — 详细任务分解

**范围**: 文件树显示 + 文件列表排序  
**总工期**: 7 天 (35 个工作日时)  

---

## 📅 Phase 1: 核心视觉优化 (2 天)

> **目标**: 提升文件树视觉层级，实现基础排序功能

---

### Task 1.1: 添加层级连接线

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.1.1 | 创建 TreeConnector 组件 | 渲染层级连接线 | 组件可复用 |
| 1.1.2 | 计算连接线逻辑 | 根据 depth 和 isLast 计算 | 逻辑正确 |
| 1.1.3 | 集成到文件树 | 替换现有缩进 | 连接线显示 |
| 1.1.4 | 样式调整 | 连接线颜色、粗细 | 符合设计规范 |

#### 代码结构

```tsx
// 新增组件: frontend/src/components/tree/TreeConnector.tsx

interface TreeConnectorProps {
  depth: number;
  isLast: boolean;
  isExpanded?: boolean;
}

export function TreeConnector({ depth, isLast }: TreeConnectorProps) {
  return (
    <span className="inline-flex shrink-0" aria-hidden="true">
      {/* 每层的竖线 */}
      {Array.from({ length: depth }, (_, i) => (
        <span
          key={i}
          className="inline-block w-4 border-l border-[#d0d0d0]"
        />
      ))}
      {/* 当前层级的横线 */}
      {depth > 0 && (
        <span 
          className="inline-block w-4 border-b border-[#d0d0d0]"
          style={{ 
            height: isLast ? '14px' : '28px',
            borderBottom: isLast ? 'none' : undefined 
          }}
        />
      )}
    </span>
  );
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.1.1 | 根节点渲染 | depth=0 | 无连接线 |
| T1.1.2 | 一级子节点 | depth=1, isLast=false | 1 条竖线 + 横线 |
| T1.1.3 | 最后一个节点 | depth=1, isLast=true | 横线只到一半 |
| T1.1.4 | 深层节点 | depth=3 | 3 条竖线 |

---

### Task 1.2: 增强图标系统

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.2.1 | 创建图标映射函数 | 根据文件类型返回图标 | 映射完整 |
| 1.2.2 | 添加图标依赖 | 导入新的 lucide 图标 | 编译通过 |
| 1.2.3 | 替换现有图标 | 使用新图标系统 | 图标正确显示 |
| 1.2.4 | 颜色规范 | 统一图标颜色 | 符合设计 |

#### 图标映射表

```typescript
// 新增: frontend/src/lib/file-icons.ts

import { 
  File, Folder, FolderOpen, Lock, HelpCircle,
  Terminal, FileText, Image, Archive, FileCode,
  FileVideo, FileAudio, Database, Settings
} from 'lucide-react';

interface FileIconInfo {
  icon: typeof File;
  color: string;
}

const EXTENSION_ICON_MAP: Record<string, FileIconInfo> = {
  // 可执行文件
  exe: { icon: Terminal, color: '#e74c3c' },
  dll: { icon: Terminal, color: '#e74c3c' },
  bat: { icon: Terminal, color: '#e74c3c' },
  cmd: { icon: Terminal, color: '#e74c3c' },
  msi: { icon: Terminal, color: '#e74c3c' },
  
  // 文档
  txt: { icon: FileText, color: '#3498db' },
  doc: { icon: FileText, color: '#3498db' },
  docx: { icon: FileText, color: '#3498db' },
  pdf: { icon: FileText, color: '#e74c3c' },
  rtf: { icon: FileText, color: '#3498db' },
  
  // 代码
  js: { icon: FileCode, color: '#f1c40f' },
  ts: { icon: FileCode, color: '#3498db' },
  py: { icon: FileCode, color: '#2ecc71' },
  rs: { icon: FileCode, color: '#e67e22' },
  html: { icon: FileCode, color: '#e74c3c' },
  css: { icon: FileCode, color: '#3498db' },
  json: { icon: FileCode, color: '#f1c40f' },
  xml: { icon: FileCode, color: '#e67e22' },
  
  // 图片
  jpg: { icon: Image, color: '#2ecc71' },
  jpeg: { icon: Image, color: '#2ecc71' },
  png: { icon: Image, color: '#2ecc71' },
  gif: { icon: Image, color: '#2ecc71' },
  bmp: { icon: Image, color: '#2ecc71' },
  svg: { icon: Image, color: '#2ecc71' },
  ico: { icon: Image, color: '#2ecc71' },
  
  // 压缩包
  zip: { icon: Archive, color: '#f39c12' },
  rar: { icon: Archive, color: '#f39c12' },
  '7z': { icon: Archive, color: '#f39c12' },
  tar: { icon: Archive, color: '#f39c12' },
  gz: { icon: Archive, color: '#f39c12' },
  
  // 视频
  mp4: { icon: FileVideo, color: '#9b59b6' },
  avi: { icon: FileVideo, color: '#9b59b6' },
  mkv: { icon: FileVideo, color: '#9b59b6' },
  mov: { icon: FileVideo, color: '#9b59b6' },
  
  // 音频
  mp3: { icon: FileAudio, color: '#1abc9c' },
  wav: { icon: FileAudio, color: '#1abc9c' },
  flac: { icon: FileAudio, color: '#1abc9c' },
  
  // 数据库
  db: { icon: Database, color: '#34495e' },
  sqlite: { icon: Database, color: '#34495e' },
  mdb: { icon: Database, color: '#34495e' },
  
  // 系统
  sys: { icon: Settings, color: '#7f8c8d' },
  ini: { icon: Settings, color: '#7f8c8d' },
  log: { icon: FileText, color: '#95a5a6' },
};

export function getFileIcon(node: { name: string; entryType?: string; status?: string; expanded?: boolean }): FileIconInfo {
  // 目录特殊处理
  if (node.entryType === 'directory') {
    if (node.status === 'locked') return { icon: Lock, color: '#e67e22' };
    if (node.status === 'unsupported') return { icon: HelpCircle, color: '#bdc3c7' };
    return { icon: node.expanded ? FolderOpen : Folder, color: '#888' };
  }
  
  // 文件 - 根据扩展名
  const ext = node.name.split('.').pop()?.toLowerCase() ?? '';
  return EXTENSION_ICON_MAP[ext] ?? { icon: File, color: '#888' };
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.2.1 | 目录图标 | entryType='directory' | Folder 图标 |
| T1.2.2 | 展开目录 | entryType='directory', expanded=true | FolderOpen 图标 |
| T1.2.3 | 加密目录 | status='locked' | Lock 图标, 橙色 |
| T1.2.4 | 可执行文件 | name='test.exe' | Terminal 图标, 红色 |
| T1.2.5 | 文档文件 | name='readme.txt' | FileText 图标, 蓝色 |
| T1.2.6 | 图片文件 | name='photo.jpg' | Image 图标, 绿色 |
| T1.2.7 | 未知扩展名 | name='file.xyz' | File 图标, 灰色 |

---

### Task 1.3: 实现目录/文件分离排序

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.3.1 | 创建排序函数 | 实现 sortFileEntries | 排序正确 |
| 1.3.2 | 目录优先逻辑 | 目录排在文件前面 | 逻辑正确 |
| 1.3.3 | 集成到数据流 | 在渲染前排序 | 排序生效 |
| 1.3.4 | 测试验证 | 验证排序结果 | 符合预期 |

#### 排序函数

```typescript
// 新增: frontend/src/lib/file-sort.ts

import { FileEntryRow } from '@/types/models';

export type FileSortKey = 'name' | 'size' | 'modifiedAt' | 'ext' | 'entryType';
export type FileSortDirection = 'asc' | 'desc';

export function sortFileEntries(
  rows: FileEntryRow[],
  sortKey: FileSortKey = 'name',
  direction: FileSortDirection = 'asc'
): FileEntryRow[] {
  return [...rows].sort((a, b) => {
    // 1. 目录优先
    if (a.entryType !== b.entryType) {
      return a.entryType === 'directory' ? -1 : 1;
    }
    
    // 2. 按指定字段排序
    let comparison = 0;
    switch (sortKey) {
      case 'name':
        comparison = a.name.localeCompare(b.name, undefined, { 
          sensitivity: 'base',
          numeric: true  // 支持数字排序 (file2 < file10)
        });
        break;
        
      case 'size':
        comparison = (a.size ?? 0) - (b.size ?? 0);
        break;
        
      case 'modifiedAt':
        comparison = (a.modifiedAt ?? '').localeCompare(b.modifiedAt ?? '');
        break;
        
      case 'ext':
        const extA = a.ext ?? a.name.split('.').pop() ?? '';
        const extB = b.ext ?? b.name.split('.').pop() ?? '';
        comparison = extA.localeCompare(extB);
        break;
        
      default:
        comparison = 0;
    }
    
    // 3. 应用排序方向
    return direction === 'asc' ? comparison : -comparison;
  });
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.3.1 | 目录优先 | [file, dir, file] | [dir, file, file] |
| T1.3.2 | 名称排序 | [b.txt, a.txt] | [a.txt, b.txt] |
| T1.3.3 | 数字排序 | [file2, file10] | [file2, file10] |
| T1.3.4 | 大小排序 | [100B, 1KB] | [100B, 1KB] |
| T1.3.5 | 降序排序 | direction='desc' | 反向排序 |

---

### Task 1.4: 实现表头排序交互

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`, `frontend/src/components/tables/DenseDataTable.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.4.1 | 扩展 DenseDataTable | 添加排序属性 | 接口兼容 |
| 1.4.2 | 创建 SortIndicator 组件 | 显示排序箭头 | 箭头正确 |
| 1.4.3 | 添加点击事件 | 表头点击触发排序 | 事件正确 |
| 1.4.4 | 集成到 FileBrowser | 使用排序功能 | 排序生效 |

#### DenseDataTable 扩展

```typescript
// 修改: frontend/src/components/tables/DenseDataTable.tsx

export interface DenseColumn<T> {
  key: string;
  title: ReactNode;
  className?: string;
  sortable?: boolean;  // 新增: 是否可排序
  sortKey?: string;    // 新增: 排序键
  render: (row: T) => ReactNode;
}

interface DenseDataTableProps<T> {
  columns: DenseColumn<T>[];
  rows: T[];
  getRowKey: (row: T) => string;
  selectedRowKey?: string;
  onRowClick?: (row: T) => void;
  emptyTitle?: string;
  emptyDescription?: string;
  sortKey?: string;           // 新增: 当前排序键
  sortDirection?: 'asc' | 'desc';  // 新增: 排序方向
  onSort?: (key: string) => void;  // 新增: 排序回调
}
```

#### SortIndicator 组件

```tsx
// 新增: frontend/src/components/tables/SortIndicator.tsx

import { ArrowUp, ArrowDown, ArrowUpDown } from 'lucide-react';

interface SortIndicatorProps {
  active: boolean;
  direction?: 'asc' | 'desc';
}

export function SortIndicator({ active, direction }: SortIndicatorProps) {
  if (!active) {
    return <ArrowUpDown size={10} className="text-[#ccc] opacity-0 group-hover:opacity-100" />;
  }
  
  return direction === 'asc' 
    ? <ArrowUp size={10} className="text-[#666]" />
    : <ArrowDown size={10} className="text-[#666]" />;
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.4.1 | 表头渲染 | sortable=true | 显示排序图标 |
| T1.4.2 | 点击排序 | 点击名称列 | onSort('name') 调用 |
| T1.4.3 | 方向切换 | 连续点击 | asc → desc 切换 |
| T1.4.4 | 当前列高亮 | sortKey='name' | 显示箭头 |

---

## 📅 Phase 2: 交互增强 (2 天)

> **目标**: 添加键盘导航、搜索过滤、排序持久化

---

### Task 2.1: 添加排序状态管理

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/stores/ui-store.ts`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.1.1 | 扩展 UiState | 添加排序字段 | 类型正确 |
| 2.1.2 | 实现 setFileSortKey | 设置排序键 | 状态更新 |
| 2.1.3 | 实现 toggleFileSortDirection | 切换排序方向 | 方向切换 |
| 2.1.4 | 从 localStorage 加载 | 恢复保存的排序 | 持久化生效 |

#### Store 扩展

```typescript
// 修改: frontend/src/stores/ui-store.ts

type FileSortKey = 'name' | 'size' | 'modifiedAt' | 'ext';

type UiState = {
  // ... 现有状态
  
  // 文件列表排序 (新增)
  fileSortKey: FileSortKey;
  fileSortDirection: 'asc' | 'desc';
  setFileSortKey: (key: FileSortKey) => void;
  toggleFileSortDirection: () => void;
};

export const useUiStore = create<UiState>((set) => ({
  // ... 现有状态
  
  // 文件列表排序 (新增)
  fileSortKey: (localStorage.getItem('fileSortKey') as FileSortKey) ?? 'name',
  fileSortDirection: (localStorage.getItem('fileSortDirection') as 'asc' | 'desc') ?? 'asc',
  
  setFileSortKey: (key) => {
    localStorage.setItem('fileSortKey', key);
    set({ fileSortKey: key });
  },
  
  toggleFileSortDirection: () => {
    set((state) => {
      const newDirection = state.fileSortDirection === 'asc' ? 'desc' : 'asc';
      localStorage.setItem('fileSortDirection', newDirection);
      return { fileSortDirection: newDirection };
    });
  },
}));
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.1.1 | 默认排序 | 初始状态 | key='name', direction='asc' |
| T2.1.2 | 设置排序键 | setFileSortKey('size') | fileSortKey='size' |
| T2.1.3 | 切换方向 | toggleFileSortDirection() | 'asc' → 'desc' |
| T2.1.4 | 持久化 | 设置后刷新 | 保持设置 |

---

### Task 2.2: 实现键盘导航

**工期**: 1 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.2.1 | 添加键盘事件监听 | 监听 keydown | 事件捕获 |
| 2.2.2 | 上下箭头导航 | 移动选中项 | 高亮移动 |
| 2.2.3 | 左右箭头展开 | 展开/折叠目录 | 状态切换 |
| 2.2.4 | Enter 打开 | 打开文件/目录 | 动作正确 |
| 2.2.5 | Home/End 跳转 | 首尾跳转 | 位置正确 |
| 2.2.6 | 滚动跟随 | 选中项滚动到可见 | 滚动正确 |

#### 键盘处理函数

```typescript
// 新增: frontend/src/hooks/use-file-tree-keyboard.ts

import { useCallback, useEffect } from 'react';

interface UseFileTreeKeyboardOptions {
  nodes: FileTreeNode[];
  activeNodeId?: string;
  onNodeSelect: (nodeId: string) => void;
  onNodeToggle: (nodeId: string) => void;
  onNodeOpen: (nodeId: string) => void;
  scrollContainerRef: React.RefObject<HTMLDivElement>;
}

export function useFileTreeKeyboard({
  nodes,
  activeNodeId,
  onNodeSelect,
  onNodeToggle,
  onNodeOpen,
  scrollContainerRef,
}: UseFileTreeKeyboardOptions) {
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    const currentIndex = nodes.findIndex((n) => n.id === activeNodeId);
    if (currentIndex === -1) return;
    
    const currentNode = nodes[currentIndex];
    
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        if (currentIndex < nodes.length - 1) {
          onNodeSelect(nodes[currentIndex + 1].id);
          scrollToNode(currentIndex + 1);
        }
        break;
        
      case 'ArrowUp':
        e.preventDefault();
        if (currentIndex > 0) {
          onNodeSelect(nodes[currentIndex - 1].id);
          scrollToNode(currentIndex - 1);
        }
        break;
        
      case 'ArrowRight':
        e.preventDefault();
        if (currentNode.hasChildren && !currentNode.expanded) {
          onNodeToggle(currentNode.id);
        }
        break;
        
      case 'ArrowLeft':
        e.preventDefault();
        if (currentNode.expanded) {
          onNodeToggle(currentNode.id);
        }
        break;
        
      case 'Enter':
        e.preventDefault();
        onNodeOpen(currentNode.id);
        break;
        
      case 'Home':
        e.preventDefault();
        if (nodes.length > 0) {
          onNodeSelect(nodes[0].id);
          scrollToNode(0);
        }
        break;
        
      case 'End':
        e.preventDefault();
        if (nodes.length > 0) {
          onNodeSelect(nodes[nodes.length - 1].id);
          scrollToNode(nodes.length - 1);
        }
        break;
    }
  }, [nodes, activeNodeId, onNodeSelect, onNodeToggle, onNodeOpen]);
  
  const scrollToNode = useCallback((index: number) => {
    const container = scrollContainerRef.current;
    if (!container) return;
    
    const nodeHeight = 28;
    const nodeTop = index * nodeHeight;
    const nodeBottom = nodeTop + nodeHeight;
    
    if (nodeTop < container.scrollTop) {
      container.scrollTop = nodeTop;
    } else if (nodeBottom > container.scrollTop + container.clientHeight) {
      container.scrollTop = nodeBottom - container.clientHeight;
    }
  }, [scrollContainerRef]);
  
  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    
    container.addEventListener('keydown', handleKeyDown);
    return () => container.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown, scrollContainerRef]);
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.2.1 | 下箭头 | ArrowDown | 选中下一项 |
| T2.2.2 | 上箭头 | ArrowUp | 选中上一项 |
| T2.2.3 | 右箭头展开 | ArrowRight (折叠) | 目录展开 |
| T2.2.4 | 左箭头折叠 | ArrowLeft (展开) | 目录折叠 |
| T2.2.5 | Enter 打开 | Enter | 打开文件 |
| T2.2.6 | Home 跳转 | Home | 跳转到第一个 |
| T2.2.7 | End 跳转 | End | 跳转到最后一个 |
| T2.2.8 | 边界处理 | 首项按上箭头 | 无变化 |

---

### Task 2.3: 实现搜索过滤

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.3.1 | 添加搜索输入框 | 在树上方添加 | 输入框显示 |
| 2.3.2 | 实现过滤逻辑 | 模糊匹配文件名 | 过滤正确 |
| 2.3.3 | 高亮匹配文字 | 匹配部分高亮 | 高亮显示 |
| 2.3.4 | 防抖处理 | 输入防抖 | 性能优化 |
| 2.3.5 | 清除按钮 | 清除搜索 | 状态重置 |

#### 搜索组件

```tsx
// 新增: frontend/src/components/tree/TreeSearch.tsx

import { useState, useMemo } from 'react';
import { Search, X } from 'lucide-react';

interface TreeSearchProps {
  onFilter: (query: string) => void;
}

export function TreeSearch({ onFilter }: TreeSearchProps) {
  const [query, setQuery] = useState('');
  
  const handleChange = (value: string) => {
    setQuery(value);
    onFilter(value);
  };
  
  return (
    <div className="px-2 py-1.5 border-b border-[#e0e0e0]">
      <div className="relative">
        <Search size={12} className="absolute left-2 top-1/2 -translate-y-1/2 text-[#999]" />
        <input
          type="text"
          value={query}
          onChange={(e) => handleChange(e.target.value)}
          placeholder="过滤目录..."
          className="w-full pl-7 pr-6 py-1 text-[11px] border border-[#ddd] rounded bg-white 
                     focus:outline-none focus:border-[#999] placeholder:text-[#bbb]"
        />
        {query && (
          <button
            onClick={() => handleChange('')}
            className="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 hover:bg-[#f0f0f0] rounded"
          >
            <X size={10} className="text-[#999]" />
          </button>
        )}
      </div>
    </div>
  );
}
```

#### 过滤逻辑

```typescript
// 在 FileBrowser.tsx 中

const [filterQuery, setFilterQuery] = useState('');

const filteredTree = useMemo(() => {
  if (!filterQuery.trim()) return treeNodes;
  
  const query = filterQuery.toLowerCase();
  return treeNodes.filter((node) => 
    node.name.toLowerCase().includes(query)
  );
}, [treeNodes, filterQuery]);
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.3.1 | 输入过滤 | "test" | 只显示包含 "test" 的节点 |
| T2.3.2 | 大小写不敏感 | "TEST" | 匹配 "test" |
| T2.3.3 | 清除过滤 | 点击 X | 显示全部 |
| T2.3.4 | 空输入 | "" | 显示全部 |
| T2.3.5 | 无匹配 | "xyzxyz" | 显示空状态 |

---

## 📅 Phase 3: 高级功能 (2 天)

> **目标**: 右键菜单、拖拽调整宽度、虚拟滚动

---

### Task 3.1: 实现右键菜单

**工期**: 1 天  
**负责**: 前端  
**文件**: `frontend/src/components/tree/TreeContextMenu.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.1.1 | 创建菜单组件 | 渲染菜单项 | 组件可复用 |
| 3.1.2 | 定位逻辑 | 鼠标位置定位 | 位置正确 |
| 3.1.3 | 点击外部关闭 | 点击其他区域关闭 | 关闭正确 |
| 3.1.4 | 菜单项动作 | 绑定动作回调 | 动作触发 |
| 3.1.5 | 集成到文件树 | 右键触发 | 菜单显示 |

#### 菜单组件

```tsx
// 新增: frontend/src/components/tree/TreeContextMenu.tsx

import { useEffect, useRef } from 'react';
import { FolderOpen, FolderMinus, Copy, Clock, Download, Info } from 'lucide-react';

interface ContextMenuItem {
  label: string;
  icon: typeof FolderOpen;
  shortcut?: string;
  action: () => void;
  divider?: boolean;
}

interface TreeContextMenuProps {
  x: number;
  y: number;
  node: FileTreeNode;
  onClose: () => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  onCopyPath: () => void;
  onViewTimeline: () => void;
  onExtract: () => void;
  onProperties: () => void;
}

export function TreeContextMenu({
  x, y, node, onClose,
  onExpandAll, onCollapseAll, onCopyPath, 
  onViewTimeline, onExtract, onProperties
}: TreeContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  
  // 点击外部关闭
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);
  
  // 边界检测
  const menuStyle = {
    left: Math.min(x, window.innerWidth - 200),
    top: Math.min(y, window.innerHeight - 300),
  };
  
  const items: ContextMenuItem[] = [
    { label: '展开全部', icon: FolderOpen, action: onExpandAll },
    { label: '折叠全部', icon: FolderMinus, action: onCollapseAll, divider: true },
    { label: '复制路径', icon: Copy, shortcut: 'Ctrl+C', action: onCopyPath },
    { label: '在时间线中查看', icon: Clock, action: onViewTimeline, divider: true },
    { label: '提取文件', icon: Download, action: onExtract },
    { label: '属性', icon: Info, action: onProperties },
  ];
  
  return (
    <div
      ref={menuRef}
      className="fixed z-50 min-w-[180px] bg-white border border-[#ddd] shadow-lg rounded py-1"
      style={menuStyle}
    >
      {items.map((item, index) => (
        <div key={index}>
          {item.divider && <div className="border-t border-[#eee] my-1" />}
          <button
            onClick={() => {
              item.action();
              onClose();
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-[#333] 
                       hover:bg-[#f0f0f0] text-left"
          >
            <item.icon size={14} className="text-[#666]" />
            <span className="flex-1">{item.label}</span>
            {item.shortcut && (
              <span className="text-[10px] text-[#999]">{item.shortcut}</span>
            )}
          </button>
        </div>
      ))}
    </div>
  );
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.1.1 | 右键显示 | 右键点击节点 | 菜单显示 |
| T3.1.2 | 定位正确 | 鼠标位置 (100, 200) | 菜单在 (100, 200) |
| T3.1.3 | 边界检测 | 靠近右边缘 | 菜单不超出屏幕 |
| T3.1.4 | 点击关闭 | 点击外部 | 菜单关闭 |
| T3.1.5 | 复制路径 | 点击 "复制路径" | 剪贴板有内容 |

---

### Task 3.2: 实现拖拽调整宽度

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.2.1 | 添加拖拽手柄 | 在分隔线上添加 | 手柄显示 |
| 3.2.2 | 实现拖拽逻辑 | mousemove 调整宽度 | 宽度变化 |
| 3.2.3 | 最小/最大宽度限制 | 限制宽度范围 | 不超出范围 |
| 3.2.4 | 保存宽度 | localStorage 保存 | 持久化生效 |

#### 拖拽实现

```typescript
// 在 FileBrowser.tsx 中

const [treeWidth, setTreeWidth] = useState(() => {
  const saved = localStorage.getItem('fileTreeWidth');
  return saved ? parseInt(saved) : 224; // 默认 224px (w-56)
});

const [isResizing, setIsResizing] = useState(false);

const handleResizeStart = useCallback((e: React.MouseEvent) => {
  e.preventDefault();
  setIsResizing(true);
  
  const startX = e.clientX;
  const startWidth = treeWidth;
  
  const handleMouseMove = (e: MouseEvent) => {
    const diff = e.clientX - startX;
    const newWidth = Math.max(160, Math.min(400, startWidth + diff));
    setTreeWidth(newWidth);
  };
  
  const handleMouseUp = () => {
    setIsResizing(false);
    localStorage.setItem('fileTreeWidth', treeWidth.toString());
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', handleMouseUp);
  };
  
  document.addEventListener('mousemove', handleMouseMove);
  document.addEventListener('mouseup', handleMouseUp);
}, [treeWidth]);
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.2.1 | 拖拽调整 | 拖拽 50px | 宽度变化 50px |
| T3.2.2 | 最小宽度 | 拖拽到 100px | 宽度 = 160px |
| T3.2.3 | 最大宽度 | 拖拽到 500px | 宽度 = 400px |
| T3.2.4 | 持久化 | 调整后刷新 | 宽度保持 |

---

### Task 3.3: 实现虚拟滚动

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.3.1 | 安装依赖 | @tanstack/react-virtual | 安装成功 |
| 3.3.2 | 创建虚拟列表组件 | 封装虚拟滚动 | 组件可用 |
| 3.3.3 | 集成到文件树 | 替换普通列表 | 渲染正确 |
| 3.3.4 | 性能测试 | 1000+ 节点 | 无卡顿 |

#### 虚拟滚动实现

```tsx
// 修改: frontend/src/app/pages/FileBrowser.tsx

import { useVirtualizer } from '@tanstack/react-virtual';

function FileTreePanel({ treeNodes, ... }) {
  const parentRef = useRef<HTMLDivElement>(null);
  
  const virtualizer = useVirtualizer({
    count: treeNodes.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28, // 每行高度
    overscan: 10, // 预渲染 10 行
  });
  
  return (
    <div 
      ref={parentRef} 
      className="flex-1 overflow-auto"
      tabIndex={0}
    >
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const node = treeNodes[virtualRow.index];
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

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.3.1 | 渲染正确 | 100 个节点 | 显示正确 |
| T3.3.2 | 滚动流畅 | 快速滚动 | 无卡顿 |
| T3.3.3 | 节点复用 | 滚动后返回 | DOM 节点复用 |
| T3.3.4 | 大量节点 | 10000 个节点 | 性能正常 |

---

## 📅 Phase 4: 打磨优化 (1 天)

> **目标**: 动画效果、预加载优化、最终测试

---

### Task 4.1: 添加动画效果

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/app/pages/FileBrowser.tsx`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 4.1.1 | 展开/折叠动画 | 子节点滑入滑出 | 动画流畅 |
| 4.1.2 | 选中高亮动画 | 背景色渐变 | 过渡自然 |
| 4.1.3 | 悬停效果 | 悬停背景色 | 效果明显 |
| 4.1.4 | 性能优化 | 使用 CSS 动画 | 不影响性能 |

#### CSS 动画

```css
/* 新增: frontend/src/styles/tree-animations.css */

.tree-node {
  transition: background-color 100ms ease;
}

.tree-node:hover {
  background-color: #f5f5f5;
}

.tree-node.active {
  background-color: #e0e8f0;
  transition: background-color 100ms ease;
}

.tree-children-enter {
  animation: slideDown 150ms ease-out;
}

.tree-children-exit {
  animation: slideUp 150ms ease-in;
}

@keyframes slideDown {
  from {
    opacity: 0;
    max-height: 0;
  }
  to {
    opacity: 1;
    max-height: 1000px;
  }
}

@keyframes slideUp {
  from {
    opacity: 1;
    max-height: 1000px;
  }
  to {
    opacity: 0;
    max-height: 0;
  }
}
```

---

### Task 4.2: 预加载优化

**工期**: 0.5 天  
**负责**: 前端  
**文件**: `frontend/src/features/files/hooks.ts`

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 4.2.1 | 预加载展开目录 | 自动加载子节点 | 加载正确 |
| 4.2.2 | 缓存策略 | staleTime 优化 | 缓存生效 |
| 4.2.3 | 错误处理 | 加载失败处理 | 不影响使用 |

#### 预加载逻辑

```typescript
// 修改: frontend/src/features/files/hooks.ts

export function usePrefetchFileChildren(expandedIds: string[]) {
  const queryClient = useQueryClient();
  
  useEffect(() => {
    expandedIds.forEach((id) => {
      queryClient.prefetchQuery({
        queryKey: ['files', 'children', id],
        queryFn: () => getFileChildren(id),
        staleTime: 60_000, // 1 分钟内不重新请求
      });
    });
  }, [expandedIds, queryClient]);
}
```

---

## 📊 测试用例汇总

| Phase | 测试数量 | 通过标准 |
|-------|---------|----------|
| Phase 1 | 20 | 100% |
| Phase 2 | 18 | 100% |
| Phase 3 | 18 | 100% |
| Phase 4 | 8 | 100% |
| **总计** | **64** | **100%** |

---

## 📋 交付物清单

| 交付物 | 文件 | 说明 |
|--------|------|------|
| TreeConnector | `components/tree/TreeConnector.tsx` | 层级连接线 |
| file-icons | `lib/file-icons.ts` | 图标映射 |
| file-sort | `lib/file-sort.ts` | 排序函数 |
| SortIndicator | `components/tables/SortIndicator.tsx` | 排序指示器 |
| TreeSearch | `components/tree/TreeSearch.tsx` | 搜索过滤 |
| TreeContextMenu | `components/tree/TreeContextMenu.tsx` | 右键菜单 |
| useFileTreeKeyboard | `hooks/use-file-tree-keyboard.ts` | 键盘导航 |
| tree-animations | `styles/tree-animations.css` | 动画样式 |

---

**计划版本**: v1.0  
**最后更新**: 2026-05-30
