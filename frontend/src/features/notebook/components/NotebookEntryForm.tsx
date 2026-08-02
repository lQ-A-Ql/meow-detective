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
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import { Textarea } from '@/app/components/ui/textarea';
import type { NotebookEntryDraft } from '@/features/notebook/model/notebook-panel-model';
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

const ALL_TYPES_FILTER = '__all_types__';

export interface CitationPickerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedNodeIds: string[];
  nodes: GraphNode[];
  loading: boolean;
  onConfirm: (nodes: GraphNode[]) => void;
}

export function CitationPicker({
  open,
  onOpenChange,
  selectedNodeIds,
  nodes,
  loading,
  onConfirm,
}: CitationPickerProps) {
  const [searchTerm, setSearchTerm] = useState('');
  const [typeFilter, setTypeFilter] = useState<string>('');
  const [tempSelected, setTempSelected] = useState<Set<string>>(new Set(selectedNodeIds));

  const filteredNodes = useMemo(() => {
    return nodes.filter((node) => {
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
  }, [nodes, typeFilter, searchTerm]);

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
    const types = new Set(nodes.map((node) => node.nodeType));
    return Array.from(types);
  }, [nodes]);

  const handleConfirm = () => {
    const selectedNodes = nodes.filter((node) => tempSelected.has(node.id));
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
            <Search size={14} className="absolute left-2 top-1/2 -translate-y-1/2 text-forensics-muted-lighter" />
            <Input
              type="text"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              placeholder="按名称、摘要或标签搜索..."
              variant="forensics"
              inputSize="compact"
              className="pl-8"
            />
          </div>
          <Select
            value={typeFilter || ALL_TYPES_FILTER}
            onValueChange={(value) => setTypeFilter(value === ALL_TYPES_FILTER ? '' : value)}
          >
            <SelectTrigger size="sm" variant="forensics" className="w-36">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL_TYPES_FILTER}>全部类型</SelectItem>
              {nodeTypes.map((t) => (
                <SelectItem key={t} value={t}>
                  {t}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {/* Node list */}
        <ScrollArea className="flex-1 min-h-[240px] max-h-[400px] border border-forensics-border rounded-none">
          {loading ? (
            <div className="flex h-32 items-center justify-center">
              <Loader2 size={20} className="opacity-70 text-forensics-muted-lighter" />
            </div>
          ) : filteredNodes.length === 0 ? (
            <div className="flex h-32 items-center justify-center text-[12px] text-forensics-muted-lighter">
              未找到匹配的节点
            </div>
          ) : (
            <div>
              {filteredNodes.map((node) => (
                <div
                  key={node.id}
                  className="flex items-start gap-3 border-b border-forensics-border-light px-3 py-2 hover:bg-forensics-panel cursor-pointer last:border-b-0"
                  onClick={() => toggleNode(node.id)}
                >
                  <Checkbox
                    checked={tempSelected.has(node.id)}
                    onClick={(event) => event.stopPropagation()}
                    onCheckedChange={() => toggleNode(node.id)}
                    className="mt-0.5"
                  />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-[12px] font-light text-forensics-text">
                        {node.label}
                      </span>
                      <Badge
                        variant="outline"
                        className={`shrink-0 text-[10px] ${NODE_TYPE_BADGE[node.nodeType] ?? 'bg-forensics-panel text-forensics-muted'}`}
                      >
                        {node.nodeType}
                      </Badge>
                    </div>
                    <div className="mt-0.5 text-[10px] text-forensics-muted-light truncate">
                      {node.summary}
                    </div>
                    {node.tags.length > 0 && (
                      <div className="mt-1 flex flex-wrap gap-1">
                        {node.tags.map((tag) => (
                          <Badge
                            key={tag}
                            variant="secondary"
                            className="bg-forensics-panel-strong text-[9px] text-forensics-muted py-0 px-1"
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
          <span className="text-[11px] text-forensics-muted-lighter">
            已选 {tempSelected.size} 个节点
          </span>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={handleCancel}
              className="h-7 rounded-none border-forensics-border bg-forensics-surface px-3 text-[11px] hover:bg-forensics-panel-strong"
            >
              取消
            </Button>
            <Button
              type="button"
              onClick={handleConfirm}
              className="h-7 rounded-none border border-forensics-text bg-forensics-text px-3 text-[11px] text-white hover:bg-forensics-text-secondary"
            >
              确认 ({tempSelected.size})
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export interface EntryEditorProps {
  parentId?: string;
  onSaved: () => void;
  onCancel: () => void;
  pending: boolean;
  error?: string;
  onCreate: (request: NotebookEntryDraft, onSuccess: () => void) => void;
}

export function EntryEditor({
  parentId,
  onSaved,
  onCancel,
  pending,
  error,
  onCreate,
}: EntryEditorProps) {
  const [title, setTitle] = useState('');
  const [bodyMarkdown, setBodyMarkdown] = useState('');
  const [entryType, setEntryType] = useState<NotebookEntryType>('observation');
  const [tagInput, setTagInput] = useState('');
  const [tags, setTags] = useState<string[]>([]);

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
    onCreate(
      {
        title: title.trim(),
        bodyMarkdown: bodyMarkdown.trim(),
        entryType,
        tags,
        parentId,
      },
      onSaved,
    );
  };

  return (
    <Card className="mb-4 border-forensics-text bg-forensics-panel">
      <CardHeader className="pb-2">
        <CardTitle className="text-[13px]">
          {parentId ? '回复' : '新建笔记'}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div>
          <Input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="笔记标题"
            variant="forensics"
            inputSize="compact"
          />
        </div>

        <div>
          <Textarea
            value={bodyMarkdown}
            onChange={(e) => setBodyMarkdown(e.target.value)}
            placeholder="使用 Markdown 格式记录分析笔记..."
            rows={6}
            variant="mono"
            textareaSize="compact"
            className="leading-6"
          />
        </div>

        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1.5">
            <label className="text-[11px] text-forensics-muted">类型:</label>
            <Select value={entryType} onValueChange={(value) => setEntryType(value as NotebookEntryType)}>
              <SelectTrigger size="xs" variant="forensics" className="w-32">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(ENTRY_TYPE_CONFIG).map(([key, { label }]) => (
                  <SelectItem key={key} value={key}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        {/* Tag chips */}
        <div>
          <div className="mb-1.5 flex items-center gap-1.5">
            <Tag size={12} className="text-forensics-muted-lighter" />
            <Input
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
              variant="forensics"
              inputSize="inline"
              className="flex-1"
            />
          </div>
          {tags.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {tags.map((tag) => (
                <Badge
                  key={tag}
                  variant="secondary"
                  className="cursor-pointer bg-forensics-panel-strong text-[10px] text-forensics-text-tertiary hover:bg-forensics-hover"
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

        {error && (
          <div className="rounded-none border border-forensics-error-border bg-forensics-error-bg px-3 py-1.5 text-[11px] text-forensics-error-text">
            {error}
          </div>
        )}

        <div className="flex items-center justify-end gap-2 pt-2">
          <Button
            type="button"
            variant="outline"
            onClick={onCancel}
            className="h-7 rounded-none border-forensics-border bg-forensics-surface px-3 text-[11px] hover:bg-forensics-panel-strong"
          >
            取消
          </Button>
          <Button
            type="button"
            onClick={handleSave}
            disabled={!title.trim() || pending}
            className="h-7 rounded-none border border-forensics-text bg-forensics-text px-3 text-[11px] text-white hover:bg-forensics-text-secondary"
          >
            {pending ? (
              <Loader2 size={12} className="opacity-70" />
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
        className={`flex cursor-pointer items-center gap-2 border-b border-forensics-border-light px-3 py-2 hover:bg-forensics-panel ${
          isSelected ? 'bg-forensics-panel-strong' : ''
        }`}
        style={{ paddingLeft: `${12 + depth * 16}px` }}
        onClick={() => onSelect(item.id)}
      >
        {children.length > 0 ? (
          <Button
            type="button"
            variant="viewerControl"
            size="iconXs"
            onClick={(e) => {
              e.stopPropagation();
              setExpanded(!expanded);
            }}
            className="shrink-0 text-forensics-muted-lighter hover:text-forensics-muted"
            aria-label={expanded ? '折叠条目' : '展开条目'}
          >
            {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </Button>
        ) : (
          <div className="w-3 shrink-0" />
        )}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            {item.entryType === 'observation' ? (
              <MessageSquare size={12} className="shrink-0 text-forensics-muted-lighter" />
            ) : (
              <FileText size={12} className="shrink-0 text-forensics-muted-lighter" />
            )}
            <span className="truncate text-[12px] font-light text-forensics-text">{item.title}</span>
          </div>
          <div className="mt-0.5 flex items-center gap-2 text-[10px] text-forensics-muted-lighter">
            <Badge className={`text-[9px] py-0 ${ENTRY_TYPE_BADGE[item.entryType]}`}>
              {ENTRY_TYPE_CONFIG[item.entryType].label}
            </Badge>
            <Badge className={`text-[9px] py-0 ${STATUS_BADGE[item.status]}`}>
              {STATUS_LABEL[item.status]}
            </Badge>
            <span>{formatTimestampShort(item.updatedAt)}</span>
            {item.replyCount > 0 && (
              <span className="text-forensics-muted-lighter">
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
