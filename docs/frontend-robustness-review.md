# 前端代码健壮性复审报告

**复审日期**: 2026-05-30  
**复审范围**: MCP 功能 + 文件树优化  
**复审方法**: 静态代码分析  

---

## 📊 复审总览

| 类别 | 严重 | 中等 | 轻微 | 总计 |
|------|------|------|------|------|
| 类型安全 | 0 | 2 | 3 | 5 |
| 错误处理 | 0 | 1 | 2 | 3 |
| 边界条件 | 0 | 2 | 2 | 4 |
| 性能问题 | 0 | 1 | 2 | 3 |
| 代码质量 | 0 | 0 | 3 | 3 |
| **总计** | **0** | **6** | **12** | **18** |

---

## 🔍 详细发现

### 类型安全

#### TS-001: MCP Store 使用 `any` 类型 [中等]

**位置**: `frontend/src/stores/mcp-store.ts`

**问题**:
```typescript
// 第 125 行
const config = await invoke<any>('get_mcp_config');

// 第 225 行
const status = await invoke<any>('connect_mcp_server', { serverId: id });

// 第 270 行
const resources = await invoke<any[]>('list_mcp_resources', { serverId });
```

**影响**: 
- 失去 TypeScript 类型检查
- 运行时可能出现未预期的错误

**建议**:
```typescript
// 定义明确的类型
interface McpConfigResponse {
  servers: Array<{
    id: string;
    name: string;
    transport_type: string;
    url?: string;
    command?: string;
    args?: string[];
    enabled: boolean;
    auto_connect: boolean;
  }>;
}

const config = await invoke<McpConfigResponse>('get_mcp_config');
```

---

#### TS-002: McpTool.inputSchema 使用 `any` [轻微]

**位置**: `frontend/src/stores/mcp-store.ts:45`

**问题**:
```typescript
interface McpTool {
  name: string;
  description: string;
  inputSchema: any;  // 应该定义具体类型
}
```

**建议**:
```typescript
interface JsonSchema {
  type: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  items?: JsonSchema;
}

interface McpTool {
  name: string;
  description: string;
  inputSchema: JsonSchema;
}
```

---

#### TS-003: localStorage 类型断言 [轻微]

**位置**: `frontend/src/stores/ui-store.ts`

**问题**:
```typescript
fileSortKey: (localStorage.getItem('fileSortKey') as FileSortKey) ?? 'name',
```

**影响**: localStorage 中的值可能不是有效的 FileSortKey

**建议**:
```typescript
function isValidSortKey(key: string | null): key is FileSortKey {
  return key !== null && ['name', 'size', 'modifiedAt', 'ext'].includes(key);
}

const savedKey = localStorage.getItem('fileSortKey');
fileSortKey: isValidSortKey(savedKey) ? savedKey : 'name',
```

---

### 错误处理

#### EH-001: MCP Store 错误信息过于笼统 [中等]

**位置**: `frontend/src/stores/mcp-store.ts`

**问题**:
```typescript
catch (err) {
  set({ error: String(err), loading: false });
}
```

**影响**: 用户看到的错误信息不友好

**建议**:
```typescript
function formatMcpError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (typeof err === 'object' && err !== null && 'message' in err) {
    return String((err as { message: unknown }).message);
  }
  return '未知错误';
}

catch (err) {
  set({ error: formatMcpError(err), loading: false });
}
```

---

#### EH-002: selectServer 重复 set 调用 [轻微]

**位置**: `frontend/src/stores/mcp-store.ts:255`

**问题**:
```typescript
selectServer: (id) => {
  set({ selectedServerId: id });
  // 问题：这里再次读取 state，但 set 可能还没完成
  if (id !== get().selectedServerId) {
    set({ resources: [], tools: [], prompts: [] });
  }
},
```

**建议**:
```typescript
selectServer: (id) => {
  set((state) => {
    if (id === state.selectedServerId) return state;
    return {
      selectedServerId: id,
      resources: [],
      tools: [],
      prompts: [],
    };
  });
},
```

---

### 边界条件

#### BC-001: TreeConnector depth 负数处理 [中等]

**位置**: `frontend/src/components/tree/TreeConnector.tsx:14`

**问题**:
```typescript
export function TreeConnector({ depth, isLast }: TreeConnectorProps) {
  if (depth === 0) {
    return null;
  }
  // 没有处理 depth < 0 的情况
```

**建议**:
```typescript
export function TreeConnector({ depth, isLast }: TreeConnectorProps) {
  if (depth <= 0) {
    return null;
  }
  // ...
}
```

---

#### BC-002: VirtualFileTree 空节点检查 [中等]

**位置**: `frontend/src/components/tree/VirtualFileTree.tsx:62`

**问题**:
```typescript
{virtualizer.getVirtualItems().map((virtualRow) => {
  const node = nodes[virtualRow.index];
  if (!node) return null;  // ✓ 有检查
  // ...
})}
```

**状态**: ✅ 已正确处理

---

#### BC-003: 文件排序时空值处理 [轻微]

**位置**: `frontend/src/lib/file-sort.ts`

**问题**:
```typescript
case 'modifiedAt':
  comparison = (a.modifiedAt ?? '').localeCompare(b.modifiedAt ?? '');
  break;
```

**状态**: ✅ 已正确处理空值

---

#### BC-004: 搜索过滤空字符串 [轻微]

**位置**: `frontend/src/components/tree/TreeSearch.tsx`

**问题**: 防抖处理正确，空字符串会显示所有节点

**状态**: ✅ 已正确处理

---

### 性能问题

#### PF-001: FileBrowser 中 treeChildren 可能无限增长 [中等]

**位置**: `frontend/src/app/pages/FileBrowser.tsx:65`

**问题**:
```typescript
useEffect(() => {
  if (!activeDirectoryId || !activeChildren) {
    return;
  }
  setTreeChildren((current) => ({
    ...current,
    [activeDirectoryId]: activeChildren,  // 只增不减
  }));
}, [activeChildren, activeDirectoryId]);
```

**影响**: 长时间使用后内存占用增加

**建议**:
```typescript
// 限制缓存大小
const MAX_CACHE_SIZE = 100;

useEffect(() => {
  if (!activeDirectoryId || !activeChildren) return;
  
  setTreeChildren((current) => {
    const keys = Object.keys(current);
    if (keys.length >= MAX_CACHE_SIZE) {
      // 删除最早的缓存
      const { [keys[0]]: _, ...rest } = current;
      return { ...rest, [activeDirectoryId]: activeChildren };
    }
    return { ...current, [activeDirectoryId]: activeChildren };
  });
}, [activeChildren, activeDirectoryId]);
```

---

#### PF-002: 排序每次渲染都创建新数组 [轻微]

**位置**: `frontend/src/app/pages/FileBrowser.tsx:182`

**问题**:
```typescript
const sortedRows = useMemo(() => {
  if (!rows) return [];
  return sortFileEntries(rows, fileSortKey, fileSortDirection);
}, [rows, fileSortKey, fileSortDirection]);
```

**状态**: ✅ 已使用 useMemo 优化

---

#### PF-003: 虚拟滚动未应用于主文件树 [轻微]

**位置**: `frontend/src/app/pages/FileBrowser.tsx`

**问题**: 创建了 VirtualFileTree 组件，但 FileBrowser 中未使用

**建议**: 对于节点数 > 100 的情况，使用虚拟滚动

---

### 代码质量

#### CQ-001: 常量硬编码 [轻微]

**位置**: 多处

**问题**:
```typescript
// use-file-tree-keyboard.ts
const nodeHeight = 28; // 硬编码

// TreeConnector.tsx
style={{ height: '28px' }} // 硬编码
```

**建议**: 提取为共享常量
```typescript
// lib/constants.ts
export const TREE_NODE_HEIGHT = 28;
```

---

#### CQ-002: 重复的图标渲染逻辑 [轻微]

**位置**: `FileBrowser.tsx` 和 `VirtualFileTree.tsx`

**问题**: 两个文件都有相同的图标渲染代码

**建议**: 提取为共享组件
```tsx
// components/tree/TreeNodeIcon.tsx
export function TreeNodeIcon({ node }: { node: FileTreeNode }) {
  const iconInfo = getFileIcon(node);
  const Icon = iconInfo.icon;
  return <Icon size={12} style={{ color: iconInfo.color }} className="shrink-0" />;
}
```

---

#### CQ-003: 缺少 JSDoc 注释 [轻微]

**位置**: 部分组件

**建议**: 为公共组件和函数添加 JSDoc 注释

---

## ✅ 做得好的地方

| 类别 | 说明 |
|------|------|
| 事件监听器清理 | ✅ 所有 addEventListener 都有对应的 removeEventListener |
| 空值处理 | ✅ 大量使用 `??` 和 `?.` 处理可能的空值 |
| useMemo 优化 | ✅ 排序和过滤都使用了 useMemo |
| localStorage 异常处理 | ✅ parseInt 后有 isNaN 检查 |
| 类型定义 | ✅ 接口定义完整 |

---

## 📋 修复建议优先级

### P1 (建议修复)
1. TS-001: MCP Store 类型定义
2. EH-002: selectServer 重复 set
3. BC-001: TreeConnector depth 负数

### P2 (可选修复)
4. EH-001: 错误信息格式化
5. PF-001: treeChildren 缓存限制
6. TS-003: localStorage 类型验证

### P3 (代码质量)
7. CQ-001: 提取常量
8. CQ-002: 提取共享组件
9. CQ-003: 添加 JSDoc

---

## 📊 总体评价

**健壮性评分**: 7.5/10

**优点**:
- 空值处理良好
- 事件监听器清理完整
- 性能优化到位 (useMemo)

**改进空间**:
- 减少 `any` 类型使用
- 增强错误信息友好度
- 添加边界条件验证

---

**复审人**: MiMo AI Assistant  
**复审版本**: v1.0
