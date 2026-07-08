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
import {
  useNotebookEntry,
  useUpdateNotebookEntry,
  useAddEvidenceCitation,
} from '@/features/notebook/hooks';
import type {
  NotebookEntryListItem,
  GraphNode,
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

export function EntryDetailView({ entryId }: { entryId: string }) {
  const { data: thread, isLoading, isError } = useNotebookEntry(entryId);
  const updateMutation = useUpdateNotebookEntry();
  const addCitationMutation = useAddEvidenceCitation();
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState('');
  const [editBodyMarkdown, setEditBodyMarkdown] = useState('');
  const [citationPickerOpen, setCitationPickerOpen] = useState(false);

  const entry = thread?.[0];

  const startEdit = useCallback(() => {
    if (entry) {
      setEditTitle(entry.title);
      setEditBodyMarkdown(entry.bodyMarkdown);
      setIsEditing(true);
    }
  }, [entry]);

  const saveEdit = useCallback(() => {
    if (!entry) return;
    updateMutation.mutate(
      {
        entryId: entry.id,
        title: editTitle.trim(),
        bodyMarkdown: editBodyMarkdown.trim(),
      },
      {
        onSuccess: () => setIsEditing(false),
      },
    );
  }, [entry, editTitle, editBodyMarkdown, updateMutation]);

  const handleAddCitations = useCallback(
    (nodes: GraphNode[]) => {
      if (!entry) return;
      for (const node of nodes) {
        addCitationMutation.mutate({
          entryId: entry.id,
          targetNodeType: node.nodeType,
          targetNodeId: node.id,
          displayLabel: node.label,
          snippet: node.summary,
        });
      }
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
              className="text-[14px] font-semibold"
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
            className="prose prose-sm max-w-none text-[12px] leading-6 text-[#444]"
            dangerouslySetInnerHTML={{ __html: simpleMarkdownToHtml(entry.bodyMarkdown) }}
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
            引用节点
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
        <div className="rounded border border-dashed border-[#d8d8d8] bg-[#fcfcfc] px-3 py-2 text-[11px] text-[#999]">
          暂无引用节点，可使用图谱节点作为证据引用
        </div>
      </div>

      <CitationPicker
        caseId={entry.caseId}
        open={citationPickerOpen}
        onOpenChange={setCitationPickerOpen}
        selectedNodeIds={citationNodeIds}
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
