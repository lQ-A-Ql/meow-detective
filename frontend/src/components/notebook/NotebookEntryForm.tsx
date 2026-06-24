import { useState, useMemo } from 'react';
import {
  Loader2,
  MessageSquare,
  Search,
  X,
  ChevronDown,
  ChevronRight,
  FileText,
  Plus,
  Tag,
} from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/app/components/ui/card';
import { Badge } from '@/app/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import { Checkbox } from '@/app/components/ui/checkbox';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { useCreateNotebookEntry } from '@/features/notebook/hooks';
import { useGraphSnapshot } from '@/features/graph/hooks';
import { getNodeNeighborhood } from '@/lib/api/graph';
import { useQuery } from '@tanstack/react-query';
import type {
  NotebookEntryListItem,
  NotebookEntryType,
  GraphNode,
} from '@/types/models';
import {
  ENTRY_TYPE_BADGE,
  ENTRY_TYPE_CONFIG,
  NODE_TYPE_BADGE,
  STATUS_BADGE,
  STATUS_LABEL,
  formatTimestampShort,
} from './helpers';

export interface CitationPickerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedNodeIds: string[];
  onConfirm: (nodes: GraphNode[]) => void;
}

export function CitationPicker({ open, onOpenChange, selectedNodeIds, onConfirm }: CitationPickerProps) {
  const [searchTerm, setSearchTerm] = useState('');
  const [typeFilter, setTypeFilter] = useState<string>('');
  const [tempSelected, setTempSelected] = useState<Set<string>>(new Set(selectedNodeIds));

  const { data: snapshot } = useGraphSnapshot('case-2026-fx-091');

  const startIds = useMemo(() => {
    if (!snapshot) return [];
    return Object.keys(snapshot.nodeCountByType);
  }, [snapshot]);

  const { data: neighborhood, isLoading } = useQuery({
    queryKey: ['graph', 'citation-search', startIds, typeFilter],
    queryFn: async () => {
      const results: GraphNode[] = [];
      const seen = new Set<string>();
      for (const nodeId of startIds) {
        const result = await getNodeNeighborhood(nodeId, 1);
        for (const node of result.nodes) {
          if (!seen.has(node.id)) {
            seen.add(node.id);
            results.push(node);
          }
        }
      }
      return results;
    },
    enabled: startIds.length > 0,
    retry: false,
  });

  const allNodes = neighborhood ?? [];

  const filteredNodes = useMemo(() => {
    return allNodes.filter((node) => {
      if (typeFilter && node.nodeType !== typeFilter) return false;
      if (searchTerm) {
        const term = searchTerm.toLowerCase();
        return (
          node.label.toLowerCase().includes(term) ||
          node.summary.toLowerCase().includes(term) ||
          node.tags.some((t) => t.toLowerCase().includes(term))
        );
      }
      return true;
    });
  }, [allNodes, typeFilter, searchTerm]);

  const toggleNode = (nodeId: string) => {
    setTempSelected((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  };

  const nodeTypes = useMemo(() => {
    const types = new Set(allNodes.map((n) => n.nodeType));
    return Array.from(types);
  }, [allNodes]);

  const handleConfirm = () => {
    const selectedNodes = allNodes.filter((n) => tempSelected.has(n.id));
    onConfirm(selectedNodes);
    onOpenChange(false);
  };

  const handleCancel = () => {
    setTempSelected(new Set(selectedNodeIds));
    setSearchTerm('');
    setTypeFilter('');
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={handleCancel}>
      <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="text-[15px]">引用选择器</DialogTitle>
          <DialogDescription className="text-[11px]">
            搜索并选择图谱节点作为引用来源
          </DialogDescription>
        </DialogHeader>

        {/* Search & filter bar */}
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search size={14} className="absolute left-2 top-1/2 -translate-y-1/2 text-[#999]" />
            <input
              type="text"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              placeholder="按名称、摘要或标签搜索..."
              className="w-full rounded border border-[#e0e0e0] py-1.5 pl-8 pr-3 text-[12px] outline-none placeholder:text-[#bbb] focus:border-[#999]"
            />
          </div>
          <select
            value={typeFilter}
            onChange={(e) => setTypeFilter(e.target.value)}
            className="rounded border border-[#e0e0e0] bg-white px-2 py-1.5 text-[12px] text-[#555] outline-none"
          >
            <option value="">全部类型</option>
            {nodeTypes.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </div>

        {/* Node list */}
        <ScrollArea className="flex-1 min-h-[240px] max-h-[400px] border border-[#e0e0e0] rounded">
          {isLoading ? (
            <div className="flex h-32 items-center justify-center">
              <Loader2 size={20} className="animate-spin text-[#ccc]" />
            </div>
          ) : filteredNodes.length === 0 ? (
            <div className="flex h-32 items-center justify-center text-[12px] text-[#999]">
              未找到匹配的节点
            </div>
          ) : (
            <div>
              {filteredNodes.map((node) => (
                <div
                  key={node.id}
                  className="flex items-start gap-3 border-b border-[#f0f0f0] px-3 py-2 hover:bg-[#fafafa] cursor-pointer last:border-b-0"
                  onClick={() => toggleNode(node.id)}
                >
                  <Checkbox
                    checked={tempSelected.has(node.id)}
                    onCheckedChange={() => toggleNode(node.id)}
                    className="mt-0.5"
                  />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-[12px] font-semibold text-[#111]">
                        {node.label}
                      </span>
                      <Badge
                        variant="outline"
                        className={`shrink-0 text-[10px] ${NODE_TYPE_BADGE[node.nodeType] ?? 'bg-gray-50 text-gray-600'}`}
                      >
                        {node.nodeType}
                      </Badge>
                    </div>
                    <div className="mt-0.5 text-[10px] text-[#888] truncate">
                      {node.summary}
                    </div>
                    {node.tags.length > 0 && (
                      <div className="mt-1 flex flex-wrap gap-1">
                        {node.tags.map((tag) => (
                          <Badge
                            key={tag}
                            variant="secondary"
                            className="bg-[#f0f0f0] text-[9px] text-[#777] py-0 px-1"
                          >
                            {tag}
                          </Badge>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </ScrollArea>

        <div className="flex items-center justify-between gap-3">
          <span className="text-[11px] text-[#999]">
            已选 {tempSelected.size} 个节点
          </span>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={handleCancel}
              className="h-7 rounded border-[#ddd] bg-white px-3 text-[11px] hover:bg-[#f5f5f5]"
            >
              取消
            </Button>
            <Button
              type="button"
              onClick={handleConfirm}
              className="h-7 rounded border border-[#111] bg-[#111] px-3 text-[11px] text-white hover:bg-[#333]"
            >
              确认 ({tempSelected.size})
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

interface EntryEditorProps {
  parentId?: string;
  onSaved: () => void;
  onCancel: () => void;
}

export function EntryEditor({ parentId, onSaved, onCancel }: EntryEditorProps) {
  const [title, setTitle] = useState('');
  const [bodyMarkdown, setBodyMarkdown] = useState('');
  const [entryType, setEntryType] = useState<NotebookEntryType>('observation');
  const [tagInput, setTagInput] = useState('');
  const [tags, setTags] = useState<string[]>([]);

  const createMutation = useCreateNotebookEntry();

  const handleAddTag = () => {
    const tag = tagInput.trim().toLowerCase();
    if (tag && !tags.includes(tag)) {
      setTags([...tags, tag]);
      setTagInput('');
    }
  };

  const handleRemoveTag = (tag: string) => {
    setTags(tags.filter((t) => t !== tag));
  };

  const handleSave = () => {
    if (!title.trim()) return;
    createMutation.mutate(
      {
        title: title.trim(),
        bodyMarkdown: bodyMarkdown.trim(),
        entryType,
        tags,
        parentId,
      },
      {
        onSuccess: () => {
          onSaved();
        },
      },
    );
  };

  return (
    <Card className="mb-4 border-[#111] bg-[#fafafa]">
      <CardHeader className="pb-2">
        <CardTitle className="text-[13px]">
          {parentId ? '回复' : '新建笔记'}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="笔记标题"
            className="w-full rounded border border-[#e0e0e0] bg-white px-3 py-1.5 text-[13px] outline-none placeholder:text-[#bbb] focus:border-[#999]"
          />
        </div>

        <div>
          <textarea
            value={bodyMarkdown}
            onChange={(e) => setBodyMarkdown(e.target.value)}
            placeholder="使用 Markdown 格式记录分析笔记..."
            rows={6}
            className="w-full resize-y rounded border border-[#e0e0e0] bg-white px-3 py-2 font-mono text-[12px] leading-6 outline-none placeholder:text-[#bbb] focus:border-[#999]"
          />
        </div>

        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1.5">
            <label className="text-[11px] text-[#666]">类型:</label>
            <select
              value={entryType}
              onChange={(e) => setEntryType(e.target.value as NotebookEntryType)}
              className="rounded border border-[#e0e0e0] bg-white px-2 py-1 text-[12px] text-[#555] outline-none"
            >
              {Object.entries(ENTRY_TYPE_CONFIG).map(([key, { label }]) => (
                <option key={key} value={key}>
                  {label}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* Tag chips */}
        <div>
          <div className="mb-1.5 flex items-center gap-1.5">
            <Tag size={12} className="text-[#999]" />
            <input
              type="text"
              value={tagInput}
              onChange={(e) => setTagInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  handleAddTag();
                }
              }}
              placeholder="输入标签后回车添加..."
              className="flex-1 rounded border border-[#e0e0e0] bg-white px-2 py-1 text-[11px] outline-none placeholder:text-[#bbb] focus:border-[#999]"
            />
          </div>
          {tags.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {tags.map((tag) => (
                <Badge
                  key={tag}
                  variant="secondary"
                  className="cursor-pointer bg-[#f0f0f0] text-[10px] text-[#555] hover:bg-[#e0e0e0]"
                >
                  {tag}
                  <X
                    size={10}
                    className="ml-1"
                    onClick={() => handleRemoveTag(tag)}
                  />
                </Badge>
              ))}
            </div>
          )}
        </div>

        {createMutation.isError && (
          <div className="rounded border border-red-200 bg-red-50 px-3 py-1.5 text-[11px] text-red-700">
            {(createMutation.error as Error)?.message ?? '保存失败'}
          </div>
        )}

        <div className="flex items-center justify-end gap-2 pt-2">
          <Button
            type="button"
            variant="outline"
            onClick={onCancel}
            className="h-7 rounded border-[#ddd] bg-white px-3 text-[11px] hover:bg-[#f5f5f5]"
          >
            取消
          </Button>
          <Button
            type="button"
            onClick={handleSave}
            disabled={!title.trim() || createMutation.isPending}
            className="h-7 rounded border border-[#111] bg-[#111] px-3 text-[11px] text-white hover:bg-[#333]"
          >
            {createMutation.isPending ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <Plus size={12} />
            )}
            保存
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

export function EntryTreeItem({
  item,
  allItems,
  depth = 0,
  selectedId,
  onSelect,
}: {
  item: NotebookEntryListItem;
  allItems: NotebookEntryListItem[];
  depth?: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const children = allItems.filter((i) => i.parentId === item.id);
  const [expanded, setExpanded] = useState(depth < 1);
  const isSelected = selectedId === item.id;

  return (
    <div>
      <div
        className={`flex cursor-pointer items-center gap-2 border-b border-[#f5f5f5] px-3 py-2 hover:bg-[#f8f8f8] ${
          isSelected ? 'bg-[#f0f0f0]' : ''
        }`}
        style={{ paddingLeft: `${12 + depth * 16}px` }}
        onClick={() => onSelect(item.id)}
      >
        {children.length > 0 ? (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setExpanded(!expanded);
            }}
            className="shrink-0 text-[#bbb] hover:text-[#666]"
          >
            {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </button>
        ) : (
          <div className="w-3 shrink-0" />
        )}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            {item.entryType === 'observation' ? (
              <MessageSquare size={12} className="shrink-0 text-[#aaa]" />
            ) : (
              <FileText size={12} className="shrink-0 text-[#aaa]" />
            )}
            <span className="truncate text-[12px] font-medium text-[#222]">{item.title}</span>
          </div>
          <div className="mt-0.5 flex items-center gap-2 text-[10px] text-[#999]">
            <Badge className={`text-[9px] py-0 ${ENTRY_TYPE_BADGE[item.entryType]}`}>
              {ENTRY_TYPE_CONFIG[item.entryType].label}
            </Badge>
            <Badge className={`text-[9px] py-0 ${STATUS_BADGE[item.status]}`}>
              {STATUS_LABEL[item.status]}
            </Badge>
            <span>{formatTimestampShort(item.updatedAt)}</span>
            {item.replyCount > 0 && (
              <span className="text-[#bbb]">
                {item.replyCount} 回复
              </span>
            )}
          </div>
        </div>
      </div>
      {expanded &&
        children.map((child) => (
          <EntryTreeItem
            key={child.id}
            item={child}
            allItems={allItems}
            depth={depth + 1}
            selectedId={selectedId}
            onSelect={onSelect}
          />
        ))}
    </div>
  );
}
