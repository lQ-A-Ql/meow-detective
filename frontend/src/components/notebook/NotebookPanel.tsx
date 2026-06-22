import { useState, useCallback, useMemo } from 'react';
import {
  BookOpen,
  FileText,
  Link2,
  Loader2,
  MessageSquare,
  Plus,
  Search,
  Tag,
  X,
  ChevronDown,
  ChevronRight,
  Clock,
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
import {
  useNotebookEntries,
  useNotebookEntry,
  useCreateNotebookEntry,
  useUpdateNotebookEntry,
  useAddCitation,
} from '@/features/notebook/hooks';
import { useGraphSnapshot } from '@/features/graph/hooks';
import { getNodeNeighborhood } from '@/lib/api/graph';
import { useQuery } from '@tanstack/react-query';
import type {
  NotebookEntryListItem,
  NotebookEntryType,
  NotebookEntryStatus,
  GraphNode,
} from '@/types/models';

const ENTRY_TYPE_CONFIG: Record<NotebookEntryType, { label: string; order: number }> = {
  note: { label: '笔记', order: 0 },
  observation: { label: '观察', order: 1 },
  finding: { label: '发现', order: 2 },
  lead: { label: '线索', order: 3 },
};

const ENTRY_TYPE_BADGE: Record<NotebookEntryType, string> = {
  note: 'bg-blue-50 text-blue-700',
  observation: 'bg-purple-50 text-purple-700',
  finding: 'bg-amber-50 text-amber-700',
  lead: 'bg-red-50 text-red-700',
};

const STATUS_BADGE: Record<NotebookEntryStatus, string> = {
  draft: 'bg-gray-100 text-gray-600',
  review: 'bg-amber-50 text-amber-700',
  final: 'bg-green-50 text-green-700',
};

const STATUS_LABEL: Record<NotebookEntryStatus, string> = {
  draft: '草稿',
  review: '审核中',
  final: '定稿',
};

const NODE_TYPE_BADGE: Record<string, string> = {
  File: 'bg-blue-50 text-blue-700',
  Artifact: 'bg-purple-50 text-purple-700',
  TimelineEvent: 'bg-amber-50 text-amber-700',
  Entity: 'bg-green-50 text-green-700',
  Lead: 'bg-red-50 text-red-700',
  NotebookEntry: 'bg-gray-50 text-gray-700',
};

function formatTimestamp(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

function formatTimestampShort(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

function simpleMarkdownToHtml(md: string): string {
  const escaped = md
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  const html = escaped
    .replace(/^### (.+)$/gm, '<h4 class="text-[13px] font-semibold mt-3 mb-1">$1</h4>')
    .replace(/^## (.+)$/gm, '<h3 class="text-[14px] font-semibold mt-3 mb-1">$1</h3>')
    .replace(/^# (.+)$/gm, '<h2 class="text-[15px] font-semibold mt-3 mb-1">$1</h2>')
    .replace(/^- \[x\] (.+)$/gm, '<label class="text-[11px] text-[#555]"><input type="checkbox" checked disabled class="mr-1" />$1</label>')
    .replace(/^- \[ \] (.+)$/gm, '<label class="text-[11px] text-[#555]"><input type="checkbox" disabled class="mr-1" />$1</label>')
    .replace(/^\* (.+)$/gm, '<li class="text-[11px] text-[#555] ml-4">$1</li>')
    .replace(/^(\d+)\. (.+)$/gm, '<li class="text-[11px] text-[#555] ml-4">$1. $2</li>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/`([^`]+)`/g, '<code class="font-mono text-[11px] bg-[#f0f0f0] px-1 rounded">$1</code>')
    .replace(/^> (.+)$/gm, '<blockquote class="border-l-2 border-[#ccc] pl-3 my-1 text-[11px] text-[#777]">$1</blockquote>')
    .replace(/\n\n/g, '<br/><br/>')
    .replace(/\n/g, '<br/>');
  return html;
}

interface CitationPickerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedIds: string[];
  onConfirm: (nodeIds: string[]) => void;
}

function CitationPicker({ open, onOpenChange, selectedIds, onConfirm }: CitationPickerProps) {
  const [searchTerm, setSearchTerm] = useState('');
  const [typeFilter, setTypeFilter] = useState<string>('');
  const [tempSelected, setTempSelected] = useState<Set<string>>(new Set(selectedIds));

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
    onConfirm(Array.from(tempSelected));
    onOpenChange(false);
  };

  const handleCancel = () => {
    setTempSelected(new Set(selectedIds));
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

function EntryEditor({ parentId, onSaved, onCancel }: EntryEditorProps) {
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [entryType, setEntryType] = useState<NotebookEntryType>('note');
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
        content: content.trim(),
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
            value={content}
            onChange={(e) => setContent(e.target.value)}
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

function EntryDetailView({ entryId }: { entryId: string }) {
  const { data: entry, isLoading, isError } = useNotebookEntry(entryId);
  const updateMutation = useUpdateNotebookEntry();
  const addCitationMutation = useAddCitation();
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState('');
  const [editContent, setEditContent] = useState('');
  const [citationPickerOpen, setCitationPickerOpen] = useState(false);

  const startEdit = useCallback(() => {
    if (entry) {
      setEditTitle(entry.title);
      setEditContent(entry.content);
      setIsEditing(true);
    }
  }, [entry]);

  const saveEdit = useCallback(() => {
    if (!entry) return;
    updateMutation.mutate(
      {
        entryId: entry.id,
        title: editTitle.trim(),
        content: editContent.trim(),
      },
      {
        onSuccess: () => setIsEditing(false),
      },
    );
  }, [entry, editTitle, editContent, updateMutation]);

  const handleAddCitations = useCallback(
    (nodeIds: string[]) => {
      if (!entry) return;
      addCitationMutation.mutate({ entryId: entry.id, nodeIds });
    },
    [entry, addCitationMutation],
  );

  if (isLoading) {
    return (
      <div className="flex h-40 items-center justify-center text-[#999]">
        <Loader2 size={20} className="mr-2 animate-spin" />
        加载笔记...
      </div>
    );
  }

  if (isError || !entry) {
    return (
      <div className="flex h-40 flex-col items-center justify-center gap-2">
        <BookOpen size={24} className="text-[#ccc]" />
        <div className="text-[12px] text-[#999]">笔记加载失败</div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          {isEditing ? (
            <input
              type="text"
              value={editTitle}
              onChange={(e) => setEditTitle(e.target.value)}
              className="w-full rounded border border-[#111] bg-white px-2 py-1 text-[14px] font-semibold outline-none"
            />
          ) : (
            <h3 className="text-[15px] font-semibold text-[#111]">{entry.title}</h3>
          )}
          <div className="mt-1 flex flex-wrap items-center gap-2 text-[10px] text-[#777]">
            <Badge className={ENTRY_TYPE_BADGE[entry.entryType]}>
              {ENTRY_TYPE_CONFIG[entry.entryType].label}
            </Badge>
            <Badge className={STATUS_BADGE[entry.status]}>
              {STATUS_LABEL[entry.status]}
            </Badge>
            <span className="flex items-center gap-1">
              <Clock size={10} />
              更新: {formatTimestampShort(entry.updatedAt)}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {isEditing ? (
            <>
              <Button
                type="button"
                variant="outline"
                onClick={() => setIsEditing(false)}
                className="h-7 rounded border-[#ddd] bg-white px-3 text-[11px] hover:bg-[#f5f5f5]"
              >
                取消
              </Button>
              <Button
                type="button"
                onClick={saveEdit}
                disabled={updateMutation.isPending}
                className="h-7 rounded border border-[#111] bg-[#111] px-3 text-[11px] text-white hover:bg-[#333]"
              >
                {updateMutation.isPending ? <Loader2 size={12} className="animate-spin" /> : '保存'}
              </Button>
            </>
          ) : (
            <Button
              type="button"
              variant="outline"
              onClick={startEdit}
              className="h-7 rounded border-[#ddd] bg-white px-3 text-[11px] hover:bg-[#f5f5f5]"
            >
              编辑
            </Button>
          )}
        </div>
      </div>

      {/* Content */}
      <div className="rounded border border-[#e0e0e0] bg-white p-4">
        {isEditing ? (
          <textarea
            value={editContent}
            onChange={(e) => setEditContent(e.target.value)}
            rows={10}
            className="w-full resize-y rounded border border-[#e0e0e0] bg-white px-3 py-2 font-mono text-[12px] leading-6 outline-none focus:border-[#999]"
          />
        ) : (
          <div
            className="prose prose-sm max-w-none text-[12px] leading-6 text-[#444]"
            dangerouslySetInnerHTML={{ __html: simpleMarkdownToHtml(entry.content) }}
          />
        )}
      </div>

      {/* Tags */}
      {entry.tags.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <Tag size={12} className="text-[#999]" />
          {entry.tags.map((tag) => (
            <Badge
              key={tag}
              variant="secondary"
              className="bg-[#f0f0f0] text-[10px] text-[#555]"
            >
              {tag}
            </Badge>
          ))}
        </div>
      )}

      {/* Citations */}
      <div>
        <div className="mb-2 flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 text-[11px] font-semibold text-[#666]">
            <Link2 size={12} />
            引用节点 ({entry.citationNodeIds.length})
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setCitationPickerOpen(true)}
            className="h-6 rounded border-[#ddd] bg-white px-2 text-[10px] hover:bg-[#f5f5f5]"
          >
            <Plus size={10} />
            添加引用
          </Button>
        </div>
        {entry.citationNodeIds.length > 0 ? (
          <div className="grid grid-cols-1 gap-1.5 md:grid-cols-2">
            {entry.citationNodeIds.map((nodeId) => (
              <CitedNodeBadge key={nodeId} nodeId={nodeId} />
            ))}
          </div>
        ) : (
          <div className="rounded border border-dashed border-[#d8d8d8] bg-[#fcfcfc] px-3 py-2 text-[11px] text-[#999]">
            暂无引用节点，可使用图谱节点作为证据引用
          </div>
        )}
      </div>

      <CitationPicker
        open={citationPickerOpen}
        onOpenChange={setCitationPickerOpen}
        selectedIds={entry.citationNodeIds}
        onConfirm={handleAddCitations}
      />
    </div>
  );
}

function CitedNodeBadge({ nodeId }: { nodeId: string }) {
  const { data, isLoading } = useQuery({
    queryKey: ['graph', 'cited-node', nodeId],
    queryFn: async () => {
      const result = await getNodeNeighborhood(nodeId, 0);
      return result.nodes.find((n) => n.id === nodeId) ?? null;
    },
    staleTime: 60_000,
    retry: false,
  });

  if (isLoading) {
    return (
      <div className="flex items-center gap-1 rounded border border-[#e0e0e0] bg-[#f8f8f8] px-2 py-1 text-[10px] text-[#aaa]">
        <Loader2 size={10} className="animate-spin" />
        {nodeId}
      </div>
    );
  }

  if (!data) {
    return (
      <div className="rounded border border-[#e0e0e0] bg-[#f8f8f8] px-2 py-1 font-mono text-[10px] text-[#888]">
        {nodeId}
      </div>
    );
  }

  return (
    <div className="flex items-center gap-1.5 rounded border border-[#e0e0e0] bg-[#f8f8f8] px-2 py-1">
      <Link2 size={10} className="text-[#aaa]" />
      <span className="truncate text-[11px] font-medium text-[#333]">{data.label}</span>
      <Badge
        variant="outline"
        className={`shrink-0 text-[9px] py-0 ${NODE_TYPE_BADGE[data.nodeType] ?? 'bg-gray-50 text-gray-600'}`}
      >
        {data.nodeType}
      </Badge>
    </div>
  );
}

function EntryTreeItem({
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

export function NotebookPanel() {
  const { data: entries = [], isLoading, isError, refetch } = useNotebookEntries();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showNewEntry, setShowNewEntry] = useState(false);
  const [showNewReply, setShowNewReply] = useState(false);
  const [filterType, setFilterType] = useState<NotebookEntryType | ''>('');
  const [filterStatus, setFilterStatus] = useState<NotebookEntryStatus | ''>('');
  const [filterDate, setFilterDate] = useState<'all' | 'today' | 'week'>('all');

  const rootEntries = useMemo(() => {
    let filtered = entries.filter((e) => !e.parentId);

    if (filterType) {
      filtered = filtered.filter((e) => e.entryType === filterType);
    }
    if (filterStatus) {
      filtered = filtered.filter((e) => e.status === filterStatus);
    }
    if (filterDate === 'today') {
      const today = new Date();
      today.setHours(0, 0, 0, 0);
      filtered = filtered.filter((e) => new Date(e.createdAt) >= today);
    } else if (filterDate === 'week') {
      const weekAgo = new Date();
      weekAgo.setDate(weekAgo.getDate() - 7);
      filtered = filtered.filter((e) => new Date(e.createdAt) >= weekAgo);
    }

    return filtered.sort(
      (a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime(),
    );
  }, [entries, filterType, filterStatus, filterDate]);

  const typeCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    rootEntries.forEach((e) => {
      counts[e.entryType] = (counts[e.entryType] ?? 0) + 1;
    });
    return counts;
  }, [rootEntries]);

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center text-[#999]">
        <Loader2 size={24} className="mr-2 animate-spin" />
        正在加载笔记...
      </div>
    );
  }

  if (isError) {
    return (
      <div className="flex h-64 flex-col items-center justify-center gap-3">
        <BookOpen size={32} className="text-[#ccc]" />
        <div className="text-[13px] text-[#666]">无法加载笔记列表</div>
        <Button
          type="button"
          variant="outline"
          onClick={() => refetch()}
          className="h-8 rounded border-[#ddd] bg-white px-4 text-[12px] hover:bg-[#f5f5f5]"
        >
          重试
        </Button>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0">
      {/* Left sidebar: entry list */}
      <div className="flex w-[320px] shrink-0 flex-col border-r border-[#e0e0e0] bg-[#fafafa]">
        {/* Header */}
        <div className="shrink-0 border-b border-[#e0e0e0] p-4">
          <div className="flex items-center justify-between gap-2">
            <div className="font-serif text-[15px] tracking-tight text-[#111]">笔记面板</div>
            <Button
              type="button"
              onClick={() => setShowNewEntry(true)}
              className="h-7 rounded border border-[#111] bg-[#111] px-3 text-[11px] text-white hover:bg-[#333]"
            >
              <Plus size={12} />
              新建
            </Button>
          </div>

          {/* Filters */}
          <div className="mt-3 space-y-2">
            <div className="flex items-center gap-2">
              <select
                value={filterType}
                onChange={(e) => setFilterType(e.target.value as NotebookEntryType | '')}
                className="flex-1 rounded border border-[#e0e0e0] bg-white px-2 py-1 text-[11px] text-[#555] outline-none"
              >
                <option value="">全部类型</option>
                {Object.entries(ENTRY_TYPE_CONFIG).map(([key, { label }]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
              </select>
              <select
                value={filterStatus}
                onChange={(e) => setFilterStatus(e.target.value as NotebookEntryStatus | '')}
                className="flex-1 rounded border border-[#e0e0e0] bg-white px-2 py-1 text-[11px] text-[#555] outline-none"
              >
                <option value="">全部状态</option>
                {Object.entries(STATUS_LABEL).map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
              </select>
            </div>
            <select
              value={filterDate}
              onChange={(e) => setFilterDate(e.target.value as 'all' | 'today' | 'week')}
              className="w-full rounded border border-[#e0e0e0] bg-white px-2 py-1 text-[11px] text-[#555] outline-none"
            >
              <option value="all">全部日期</option>
              <option value="today">今天</option>
              <option value="week">最近一周</option>
            </select>
          </div>

          {/* Count badges */}
          <div className="mt-2 flex flex-wrap gap-1.5">
            <Badge variant="secondary" className="bg-[#f0f0f0] text-[10px] text-[#666]">
              总计: {rootEntries.length}
            </Badge>
            {Object.entries(typeCounts).map(([key, count]) => (
              <Badge
                key={key}
                variant="secondary"
                className={`text-[10px] ${ENTRY_TYPE_BADGE[key as NotebookEntryType]}`}
              >
                {ENTRY_TYPE_CONFIG[key as NotebookEntryType]?.label}: {count}
              </Badge>
            ))}
          </div>
        </div>

        {/* Entry tree */}
        <ScrollArea className="flex-1">
          {showNewEntry && (
            <div className="border-b border-[#e0e0e0] p-3">
              <EntryEditor
                onSaved={() => {
                  setShowNewEntry(false);
                }}
                onCancel={() => setShowNewEntry(false)}
              />
            </div>
          )}
          {rootEntries.length === 0 ? (
            <div className="flex h-40 flex-col items-center justify-center px-4">
              <BookOpen size={28} className="text-[#ddd]" />
              <div className="mt-3 text-[12px] text-[#999]">暂无笔记</div>
              <div className="mt-1 text-[10px] text-[#bbb]">点击"新建"创建第一条分析笔记</div>
            </div>
          ) : (
            rootEntries.map((item) => (
              <EntryTreeItem
                key={item.id}
                item={item}
                allItems={entries}
                selectedId={selectedId}
                onSelect={setSelectedId}
              />
            ))
          )}
        </ScrollArea>
      </div>

      {/* Right content pane */}
      <div className="flex flex-1 flex-col min-w-0 bg-white">
        {selectedId ? (
          <div className="flex flex-1 flex-col">
            {/* Detail header bar */}
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-[#e0e0e0] bg-[#fafafa] px-6 py-3">
              <div className="text-[11px] text-[#888]">
                笔记详情
                {(() => {
                  const entry = entries.find((e) => e.id === selectedId);
                  if (!entry) return null;
                  return (
                    <span className="ml-2 font-mono text-[#bbb]">
                      #{entry.id}
                    </span>
                  );
                })()}
              </div>
              <Button
                type="button"
                onClick={() => setShowNewReply(true)}
                className="h-7 rounded border border-[#111] bg-[#111] px-3 text-[11px] text-white hover:bg-[#333]"
              >
                <MessageSquare size={12} />
                回复
              </Button>
            </div>

            {/* Detail content */}
            <ScrollArea className="flex-1">
              <div className="p-6">
                {showNewReply && (
                  <EntryEditor
                    parentId={selectedId}
                    onSaved={() => {
                      setShowNewReply(false);
                    }}
                    onCancel={() => setShowNewReply(false)}
                  />
                )}
                <EntryDetailView entryId={selectedId} />

                {/* Threaded replies */}
                <RepliesSection
                  parentId={selectedId}
                  allEntries={entries}
                  selectedId={selectedId}
                  onSelect={setSelectedId}
                />
              </div>
            </ScrollArea>
          </div>
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 text-[#bbb]">
            <BookOpen size={40} />
            <div className="text-[14px]">从左侧列表选择一条笔记</div>
            <div className="text-[11px]">支持 Markdown 格式分析和丰富类型</div>
          </div>
        )}
      </div>
    </div>
  );
}

function RepliesSection({
  parentId,
  allEntries,
  selectedId,
  onSelect,
}: {
  parentId: string;
  allEntries: NotebookEntryListItem[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const replies = allEntries.filter((e) => e.parentId === parentId);

  if (replies.length === 0) {
    return null;
  }

  return (
    <div className="mt-6">
      <div className="mb-3 flex items-center gap-2 text-[12px] font-semibold text-[#666]">
        <MessageSquare size={14} />
        回复 ({replies.length})
      </div>
      <div className="space-y-4 border-l-2 border-[#f0f0f0] pl-4">
        {replies
          .sort((a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime())
          .map((reply) => (
            <div
              key={reply.id}
              className={`cursor-pointer rounded border p-4 hover:border-[#999] transition-colors ${
                selectedId === reply.id
                  ? 'border-[#111] bg-[#fafafa]'
                  : 'border-[#e0e0e0] bg-white'
              }`}
              onClick={() => onSelect(reply.id)}
            >
              <div className="flex items-center gap-2 mb-2">
                <Badge className={`text-[9px] py-0 ${ENTRY_TYPE_BADGE[reply.entryType]}`}>
                  {ENTRY_TYPE_CONFIG[reply.entryType].label}
                </Badge>
                <Badge className={`text-[9px] py-0 ${STATUS_BADGE[reply.status]}`}>
                  {STATUS_LABEL[reply.status]}
                </Badge>
                <span className="text-[10px] text-[#aaa]">
                  {formatTimestampShort(reply.updatedAt)}
                </span>
              </div>
              <div className="text-[12px] font-medium text-[#222]">{reply.title}</div>
              {reply.tags.length > 0 && (
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {reply.tags.map((tag) => (
                    <Badge
                      key={tag}
                      variant="secondary"
                      className="bg-[#f0f0f0] text-[9px] text-[#777] py-0"
                    >
                      {tag}
                    </Badge>
                  ))}
                </div>
              )}
            </div>
          ))}
      </div>
    </div>
  );
}
