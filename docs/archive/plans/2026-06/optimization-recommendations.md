# 代码优化建议 (低优先级)

> 归档：2026-06 优化建议快照，仅用于历史追溯，不作为当前 backlog。

本文档记录了代码审查中发现的低优先级优化建议。这些优化不阻塞当前版本的发布，但建议在后续迭代中逐步实施。

## 1. FileBrowser.tsx: 使用 LRU 缓存替代 FIFO

**当前实现**: `frontend/src/app/pages/FileBrowser.tsx:157-160`

```typescript
// 如果缓存超过限制，删除最早的条目 (FIFO)
if (!current[activeDirectoryId] && keys.length >= MAX_TREE_CACHE_SIZE) {
  const { [keys[0]]: _, ...rest } = current;
  return { ...rest, [activeDirectoryId]: nextChildren };
}
```

**问题**: FIFO 策略可能清除最近访问过的热数据。

**建议**: 实现 LRU (Least Recently Used) 缓存策略：

```typescript
// 维护访问时间戳
const [treeChildrenAccessTime, setTreeChildrenAccessTime] = useState<Record<string, number>>({});

// 更新时记录访问时间
setTreeChildren((current) => {
  if (keys.length >= MAX_TREE_CACHE_SIZE && !current[activeDirectoryId]) {
    // 找到最久未访问的条目
    const lruKey = keys.reduce((oldest, key) => 
      (treeChildrenAccessTime[key] || 0) < (treeChildrenAccessTime[oldest] || 0) ? key : oldest
    );
    const { [lruKey]: _, ...rest } = current;
    return { ...rest, [activeDirectoryId]: nextChildren };
  }
  return { ...current, [activeDirectoryId]: nextChildren };
});

setTreeChildrenAccessTime((current) => ({
  ...current,
  [activeDirectoryId]: Date.now(),
}));
```

**优先级**: 低 - 当前缓存大小 100 对大多数用例足够，FIFO 在顺序浏览时表现良好。

---

## 2. 提取自定义 hooks: useFileTreeCache 和 useFileJump

**当前实现**: `frontend/src/app/pages/FileBrowser.tsx` 有 7 个 `useEffect` hooks。

**建议**: 将复杂逻辑提取为自定义 hooks：

### `useFileTreeCache.ts`

```typescript
export function useFileTreeCache(
  activeDirectoryId: string | undefined,
  activeChildren: FileTreeNode[] | undefined,
  activeChildrenOffset: number,
  showHidden: boolean
) {
  const [treeChildren, setTreeChildren] = useState<Record<string, FileTreeNode[]>>({});
  const [treeChildOffsets, setTreeChildOffsets] = useState<Record<string, number>>({});

  // 重置缓存当 showHidden 变化
  useEffect(() => {
    setTreeChildren({});
    setTreeChildOffsets({});
  }, [showHidden]);

  // 合并分页数据
  useEffect(() => {
    if (!activeDirectoryId || !activeChildren) return;
    // ... 现有的缓存逻辑
  }, [activeChildren, activeChildrenOffset, activeDirectoryId]);

  return {
    treeChildren,
    treeChildOffsets,
    setTreeChildOffsets,
    loadMoreChildren: (directoryId: string, pageSize: number) => {
      setTreeChildOffsets((current) => ({
        ...current,
        [directoryId]: (current[directoryId] ?? 0) + pageSize,
      }));
    },
  };
}
```

### `useFileJump.ts`

```typescript
export function useFileJump(
  selectedFileId: string | undefined,
  jumpContext: FileJumpContext | undefined,
  showHidden: boolean,
  setShowHidden: (value: boolean) => void,
  setSelectedDirectoryId: (id: string) => void,
  setExpandedDirectoryIds: React.Dispatch<React.SetStateAction<string[]>>,
  setRowsOffset: (offset: number) => void
) {
  const rowsOffsetRef = useRef(0);

  useEffect(() => {
    if (!jumpContext || !selectedFileId) return;
    if (jumpContext.target.id !== selectedFileId) return;

    // 需要显示隐藏文件
    if (jumpContext.requiresShowHidden && !showHidden) {
      setShowHidden(true);
      return;
    }

    // 切换目录
    if (selectedDirectoryId !== jumpContext.directory.id) {
      setSelectedDirectoryId(jumpContext.directory.id);
    }

    // 展开祖先目录
    setExpandedDirectoryIds((current) => {
      const next = new Set(current);
      for (const id of jumpContext.ancestorDirectoryIds) {
        next.add(id);
      }
      return next.size === current.length ? current : Array.from(next);
    });

    // 跳转到正确的分页
    if (rowsOffsetRef.current !== jumpContext.rowOffset) {
      setRowsOffset(jumpContext.rowOffset);
      rowsOffsetRef.current = jumpContext.rowOffset;
    }
  }, [jumpContext, selectedFileId, showHidden, /* ... */]);
}
```

**优先级**: 低 - 当前代码虽然 useEffect 较多，但逻辑清晰，职责分明。

---

## 3. 替换 window.confirm 为自定义对话框

**当前实现**: `frontend/src/app/pages/CaseActions.tsx:126-129`

```typescript
if (window.confirm(`确定删除案件 "${item.name}"？\n\n该操作将删除案件目录及其所有数据，且不可撤销。`)) {
  onDeleteCase(item.caseRoot);
}
```

**问题**: 原生 `window.confirm` 样式不可控，与应用 UI 不一致。

**建议**: 创建自定义确认对话框组件：

```typescript
// frontend/src/components/dialogs/ConfirmDialog.tsx
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/app/components/ui/alert-dialog';

interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  confirmText?: string;
  cancelText?: string;
  onConfirm: () => void;
  variant?: 'default' | 'destructive';
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmText = '确认',
  cancelText = '取消',
  onConfirm,
  variant = 'default',
}: ConfirmDialogProps) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{cancelText}</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            className={variant === 'destructive' ? 'bg-red-600 hover:bg-red-700' : ''}
          >
            {confirmText}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
```

使用示例：

```typescript
const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
const [pendingDeleteCase, setPendingDeleteCase] = useState<string | null>(null);

// 在 JSX 中
<button
  onClick={() => {
    setPendingDeleteCase(item.caseRoot);
    setDeleteConfirmOpen(true);
  }}
>
  <Trash2 size={12} />
</button>

<ConfirmDialog
  open={deleteConfirmOpen}
  onOpenChange={setDeleteConfirmOpen}
  title="删除案件"
  description={`确定删除案件 "${item.name}"？该操作将删除案件目录及其所有数据，且不可撤销。`}
  confirmText="删除"
  variant="destructive"
  onConfirm={() => {
    if (pendingDeleteCase) {
      onDeleteCase(pendingDeleteCase);
    }
    setDeleteConfirmOpen(false);
    setPendingDeleteCase(null);
  }}
/>
```

**优先级**: 低 - 功能正常，仅为 UI 一致性改进。

---

## 4. Settings.tsx: 实时验证案件根路径

**当前实现**: `frontend/src/app/pages/Settings.tsx:77-80`

```typescript
async function saveSettings() {
  if (!settings.caseRoot.trim()) {
    setSettingsMessage('案件默认存储路径不能为空。');
    return;
  }
  // ...
}
```

**问题**: 仅在保存时验证，用户体验不佳。

**建议**: 添加实时验证和视觉反馈：

```typescript
const [caseRootError, setCaseRootError] = useState<string | null>(null);

// 实时验证
useEffect(() => {
  if (!settings.caseRoot.trim()) {
    setCaseRootError('路径不能为空');
  } else if (settings.caseRoot.includes('\0')) {
    setCaseRootError('路径包含非法字符');
  } else {
    setCaseRootError(null);
  }
}, [settings.caseRoot]);

// 在 JSX 中
<div>
  <input
    id="settings-case-root"
    value={settings.caseRoot}
    onChange={(event) =>
      setSettings((current) => ({ ...current, caseRoot: event.target.value }))
    }
    className={`w-full max-w-3xl bg-[#f8f8f8] border p-3 font-mono text-[12px] text-[#111] ${
      caseRootError ? 'border-red-500' : 'border-[#e0e0e0]'
    }`}
  />
  {caseRootError && (
    <div className="mt-1 text-[10px] text-red-600">{caseRootError}</div>
  )}
</div>

// 禁用保存按钮
<button
  type="button"
  onClick={saveSettings}
  disabled={savingSettings || Boolean(caseRootError)}
  className="border border-[#111] bg-[#111] px-4 py-2 text-[12px] text-white hover:bg-[#333] disabled:opacity-50 disabled:cursor-not-allowed"
>
  {savingSettings ? '保存中...' : '保存设置'}
</button>
```

**优先级**: 低 - 当前验证逻辑正确，仅为 UX 改进。

---

## 5. FileBrowser.tsx: 拆分计划

**当前状态**: 670 行，接近但未超过 1500 行限制。

**建议**: 当文件超过 800 行时，考虑以下拆分策略：

1. **FileTreeState.ts** - 树状态管理逻辑
2. **FileListState.ts** - 列表状态管理逻辑  
3. **FileBrowserLayout.tsx** - 纯布局组件
4. **useFileBrowser.ts** - 主 hook（协调所有状态）

拆分示例：

```typescript
// useFileBrowser.ts
export function useFileBrowser() {
  const treeState = useFileTreeState();
  const listState = useFileListState();
  const previewState = useFilePreviewState();
  
  return {
    tree: treeState,
    list: listState,
    preview: previewState,
  };
}

// FileBrowser.tsx (简化后)
export function FileBrowser() {
  const { tree, list, preview } = useFileBrowser();
  
  return (
    <FileBrowserLayout
      tree={tree}
      list={list}
      preview={preview}
    />
  );
}
```

**优先级**: 低 - 当前 670 行可接受，仅作为未来增长的预防措施。

---

## 6. Hex 查看器性能基准测试

**建议**: 为分块加载和虚拟滚动添加性能基准测试。

```typescript
// frontend/src/components/viewers/HexViewer.bench.ts
import { bench, describe } from 'vitest';
import { mergeLoadedRanges } from '@/lib/hex-range-merger';
import { parseOffsetInput } from '@/lib/hex-offset-parser';

describe('HexViewer performance', () => {
  bench('merge 100 ranges', () => {
    let ranges: HexLoadedRange[] = [];
    for (let i = 0; i < 100; i++) {
      ranges = mergeLoadedRanges(ranges, { 
        start: i * 1024, 
        end: (i + 1) * 1024 
      });
    }
  });

  bench('parse 1000 hex offsets', () => {
    const offsets = [
      '0x0', '0x1000', '0xABCD', '1234h', '5678',
      'DEADBEEF', '0x7FFFFFFF', 'FFFFh',
    ];
    for (let i = 0; i < 1000; i++) {
      parseOffsetInput(offsets[i % offsets.length]);
    }
  });
});
```

运行基准测试：

```bash
pnpm --dir frontend vitest bench --run
```

**优先级**: 低 - 当前性能良好，仅作为性能回归预防。

---

## 实施建议

这些优化建议按以下优先级实施：

1. **第一批** (提升用户体验): #3 自定义对话框, #4 实时验证
2. **第二批** (代码可维护性): #2 提取自定义 hooks, #1 LRU 缓存
3. **第三批** (预防性): #5 拆分计划, #6 性能基准测试

每项优化可独立实施，互不依赖。
