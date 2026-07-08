import { useState, useMemo } from 'react';
import {
  BookOpen,
  Loader2,
  MessageSquare,
  Plus,
} from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Badge } from '@/app/components/ui/badge';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import {
  useNotebookEntries,
} from '@/features/notebook/hooks';
import { useCurrentCase } from '@/features/case/hooks';
import type {
  NotebookEntryType,
  NotebookEntryStatus,
} from '@/types/models';
import {
  ENTRY_TYPE_BADGE,
  ENTRY_TYPE_CONFIG,
  STATUS_LABEL,
} from './helpers';
import { EntryEditor, EntryTreeItem } from './NotebookEntryForm';
import { EntryDetailView, RepliesSection } from './NotebookEntryDetail';

const ALL_TYPES_FILTER = '__all_types__';
const ALL_STATUS_FILTER = '__all_status__';

export function NotebookPanel() {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id;
  const {
    data: entries = [],
    isLoading,
    isError,
    refetch,
  } = useNotebookEntries();
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

  if (currentCase.isLoading) {
    return (
      <div className="flex h-64 items-center justify-center text-[#999]">
        <Loader2 size={24} className="mr-2 animate-spin" />
        正在加载案件...
      </div>
    );
  }

  if (!caseId) {
    return (
      <div className="flex h-64 flex-col items-center justify-center gap-3">
        <BookOpen size={32} className="text-[#ccc]" />
        <div className="text-[13px] text-[#666]">请先打开或创建一个案件</div>
      </div>
    );
  }

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
              <Select
                value={filterType || ALL_TYPES_FILTER}
                onValueChange={(value) =>
                  setFilterType(value === ALL_TYPES_FILTER ? '' : (value as NotebookEntryType))
                }
              >
                <SelectTrigger size="xs" variant="forensics" className="flex-1">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL_TYPES_FILTER}>全部类型</SelectItem>
                  {Object.entries(ENTRY_TYPE_CONFIG).map(([key, { label }]) => (
                    <SelectItem key={key} value={key}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select
                value={filterStatus || ALL_STATUS_FILTER}
                onValueChange={(value) =>
                  setFilterStatus(value === ALL_STATUS_FILTER ? '' : (value as NotebookEntryStatus))
                }
              >
                <SelectTrigger size="xs" variant="forensics" className="flex-1">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL_STATUS_FILTER}>全部状态</SelectItem>
                  {Object.entries(STATUS_LABEL).map(([key, label]) => (
                    <SelectItem key={key} value={key}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Select
              value={filterDate}
              onValueChange={(value) => setFilterDate(value as 'all' | 'today' | 'week')}
            >
              <SelectTrigger size="xs" variant="forensics">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">全部日期</SelectItem>
                <SelectItem value="today">今天</SelectItem>
                <SelectItem value="week">最近一周</SelectItem>
              </SelectContent>
            </Select>
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
