import { useState, useCallback } from 'react';
import {
  BookOpen,
  Clock,
  Link2,
  Loader2,
  MessageSquare,
  Plus,
  Tag,
} from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Badge } from '@/app/components/ui/badge';
import { Input } from '@/app/components/ui/input';
import { Textarea } from '@/app/components/ui/textarea';
import type {
  NotebookEntry,
  NotebookEntryListItem,
  GraphNode,
  UpdateEntryRequest,
} from '@/types/models';
import {
  ENTRY_TYPE_BADGE,
  ENTRY_TYPE_CONFIG,
  STATUS_BADGE,
  STATUS_LABEL,
  formatTimestampShort,
  simpleMarkdownToHtml,
} from './helpers';
import { CitationPicker } from './NotebookEntryForm';

export interface EntryDetailViewProps {
  entry?: NotebookEntry;
  loading: boolean;
  error: boolean;
  updatePending: boolean;
  citationNodes: GraphNode[];
  citationNodesLoading: boolean;
  onUpdate: (request: UpdateEntryRequest, onSuccess: () => void) => void;
  onAddCitations: (entryId: string, nodes: GraphNode[]) => void;
}

export function EntryDetailView({
  entry,
  loading,
  error,
  updatePending,
  citationNodes,
  citationNodesLoading,
  onUpdate,
  onAddCitations,
}: EntryDetailViewProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState('');
  const [editBodyMarkdown, setEditBodyMarkdown] = useState('');
  const [citationPickerOpen, setCitationPickerOpen] = useState(false);

  const startEdit = useCallback(() => {
    if (entry) {
      setEditTitle(entry.title);
      setEditBodyMarkdown(entry.bodyMarkdown);
      setIsEditing(true);
    }
  }, [entry]);

  const saveEdit = useCallback(() => {
    if (!entry) return;
    onUpdate(
      {
        entryId: entry.id,
        title: editTitle.trim(),
        bodyMarkdown: editBodyMarkdown.trim(),
      },
      () => setIsEditing(false),
    );
  }, [editBodyMarkdown, editTitle, entry, onUpdate]);

  const handleAddCitations = useCallback(
    (nodes: GraphNode[]) => {
      if (!entry) return;
      onAddCitations(entry.id, nodes);
    },
    [entry, onAddCitations],
  );

  if (loading) {
    return (
      <div className="flex h-40 items-center justify-center text-forensics-muted-lighter">
        <Loader2 size={20} className="mr-2 opacity-70" />
        加载笔记...
      </div>
    );
  }

  if (error || !entry) {
    return (
      <div className="flex h-40 flex-col items-center justify-center gap-2">
        <BookOpen size={24} className="text-forensics-muted-lighter" />
        <div className="text-[12px] text-forensics-muted-lighter">笔记加载失败</div>
      </div>
    );
  }

  const citationNodeIds: string[] = [];

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          {isEditing ? (
            <Input
              type="text"
              value={editTitle}
              onChange={(e) => setEditTitle(e.target.value)}
              variant="forensics"
              inputSize="compact"
              className="text-[14px] font-light"
            />
          ) : (
            <h3 className="text-[15px] font-light text-forensics-text">{entry.title}</h3>
          )}
          <div className="mt-1 flex flex-wrap items-center gap-2 text-[10px] text-forensics-muted">
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
                className="h-7 rounded-none border-forensics-border bg-forensics-surface px-3 text-[11px] hover:bg-forensics-panel-strong"
              >
                取消
              </Button>
              <Button
                type="button"
                onClick={saveEdit}
                disabled={updatePending}
                className="h-7 rounded-none border border-forensics-text bg-forensics-text px-3 text-[11px] text-white hover:bg-forensics-text-secondary"
              >
                {updatePending ? <Loader2 size={12} className="opacity-70" /> : '保存'}
              </Button>
            </>
          ) : (
            <Button
              type="button"
              variant="outline"
              onClick={startEdit}
              className="h-7 rounded-none border-forensics-border bg-forensics-surface px-3 text-[11px] hover:bg-forensics-panel-strong"
            >
              编辑
            </Button>
          )}
        </div>
      </div>

      {/* Content */}
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-4">
        {isEditing ? (
          <Textarea
            value={editBodyMarkdown}
            onChange={(e) => setEditBodyMarkdown(e.target.value)}
            rows={10}
            variant="mono"
            textareaSize="compact"
            className="leading-6"
          />
        ) : (
          <div
            className="prose prose-sm max-w-none text-[12px] leading-6 text-forensics-text-secondary"
            dangerouslySetInnerHTML={{ __html: simpleMarkdownToHtml(entry.bodyMarkdown) }}
          />
        )}
      </div>

      {/* Tags */}
      {entry.tags.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <Tag size={12} className="text-forensics-muted-lighter" />
          {entry.tags.map((tag) => (
            <Badge
              key={tag}
              variant="secondary"
              className="bg-forensics-panel-strong text-[10px] text-forensics-text-tertiary"
            >
              {tag}
            </Badge>
          ))}
        </div>
      )}

      {/* Citations */}
      <div>
        <div className="mb-2 flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 text-[11px] font-light text-forensics-muted">
            <Link2 size={12} />
            引用节点
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setCitationPickerOpen(true)}
            className="h-6 rounded-none border-forensics-border bg-forensics-surface px-2 text-[10px] hover:bg-forensics-panel-strong"
          >
            <Plus size={10} />
            添加引用
          </Button>
        </div>
        <div className="rounded-none border border-dashed border-forensics-border-strong bg-forensics-surface px-3 py-2 text-[11px] text-forensics-muted-lighter">
          暂无引用节点，可使用图谱节点作为证据引用
        </div>
      </div>

      <CitationPicker
        open={citationPickerOpen}
        onOpenChange={setCitationPickerOpen}
        selectedNodeIds={citationNodeIds}
        nodes={citationNodes}
        loading={citationNodesLoading}
        onConfirm={handleAddCitations}
      />
    </div>
  );
}

export function RepliesSection({
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
      <div className="mb-3 flex items-center gap-2 text-[12px] font-light text-forensics-muted">
        <MessageSquare size={14} />
        回复 ({replies.length})
      </div>
      <div className="space-y-4 border-l-2 border-forensics-border-light pl-4">
        {replies
          .sort((a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime())
          .map((reply) => (
            <div
              key={reply.id}
              className={`cursor-pointer rounded-none border p-4 hover:border-forensics-border-strong transition-colors ${
                selectedId === reply.id
                  ? 'border-forensics-text bg-forensics-panel'
                  : 'border-forensics-border bg-forensics-surface'
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
                <span className="text-[10px] text-forensics-muted-lighter">
                  {formatTimestampShort(reply.updatedAt)}
                </span>
              </div>
              <div className="text-[12px] font-light text-forensics-text">{reply.title}</div>
              {reply.tags.length > 0 && (
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {reply.tags.map((tag) => (
                    <Badge
                      key={tag}
                      variant="secondary"
                      className="bg-forensics-panel-strong text-[9px] text-forensics-muted py-0"
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
