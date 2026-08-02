import { useMemo, useState } from 'react';
import { useCurrentCase } from '@/features/case/hooks';
import { useGraphNodes } from '@/features/graph/hooks';
import {
  useAddEvidenceCitation,
  useCreateNotebookEntry,
  useNotebookEntries,
  useNotebookEntry,
  useUpdateNotebookEntry,
} from '@/features/notebook/hooks';
import type {
  GraphNode,
  NotebookEntryStatus,
  NotebookEntryType,
  UpdateEntryRequest,
} from '@/types/models';
import type {
  NotebookEntryDraft,
  NotebookPanelModel,
} from '@/features/notebook/model/notebook-panel-model';

export function useNotebookPanelModel(): NotebookPanelModel {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id ?? '';
  const entriesQuery = useNotebookEntries();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showNewEntry, setShowNewEntry] = useState(false);
  const [showNewReply, setShowNewReply] = useState(false);
  const [filterType, setFilterType] = useState<NotebookEntryType | ''>('');
  const [filterStatus, setFilterStatus] = useState<NotebookEntryStatus | ''>('');
  const [filterDate, setFilterDate] = useState<'all' | 'today' | 'week'>('all');
  const detailQuery = useNotebookEntry(selectedId ?? undefined);
  const graphNodesQuery = useGraphNodes(caseId, 100, 0);
  const createMutation = useCreateNotebookEntry();
  const updateMutation = useUpdateNotebookEntry();
  const addCitationMutation = useAddEvidenceCitation();
  const entries = entriesQuery.data ?? [];

  const rootEntries = useMemo(() => {
    const startOfToday = new Date();
    startOfToday.setHours(0, 0, 0, 0);
    const weekAgo = new Date();
    weekAgo.setDate(weekAgo.getDate() - 7);

    return entries
      .filter((entry) => !entry.parentId)
      .filter((entry) => !filterType || entry.entryType === filterType)
      .filter((entry) => !filterStatus || entry.status === filterStatus)
      .filter((entry) => {
        const createdAt = new Date(entry.createdAt);
        if (filterDate === 'today') return createdAt >= startOfToday;
        if (filterDate === 'week') return createdAt >= weekAgo;
        return true;
      })
      .sort((left, right) => Date.parse(right.createdAt) - Date.parse(left.createdAt));
  }, [entries, filterDate, filterStatus, filterType]);

  const typeCounts = useMemo(() => {
    const counts: Partial<Record<NotebookEntryType, number>> = {};
    for (const entry of rootEntries) {
      counts[entry.entryType] = (counts[entry.entryType] ?? 0) + 1;
    }
    return counts;
  }, [rootEntries]);

  return {
    caseLoading: currentCase.isLoading,
    hasActiveCase: Boolean(caseId),
    entriesLoading: entriesQuery.isLoading,
    entriesError: entriesQuery.isError,
    entries,
    rootEntries,
    typeCounts,
    selectedId,
    selectedEntry: entries.find((entry) => entry.id === selectedId),
    showNewEntry,
    showNewReply,
    filterType,
    filterStatus,
    filterDate,
    createPending: createMutation.isPending,
    createError: mutationErrorMessage(createMutation.error),
    detail: detailQuery.data?.[0],
    detailLoading: detailQuery.isLoading,
    detailError: detailQuery.isError,
    updatePending: updateMutation.isPending,
    citationNodes: graphNodesQuery.data ?? [],
    citationNodesLoading: graphNodesQuery.isLoading,
    selectEntry: setSelectedId,
    setShowNewEntry,
    setShowNewReply,
    setFilterType,
    setFilterStatus,
    setFilterDate,
    retryEntries: () => {
      void entriesQuery.refetch();
    },
    createEntry(request: NotebookEntryDraft, onSuccess: () => void) {
      createMutation.mutate(request, { onSuccess });
    },
    updateEntry(request: UpdateEntryRequest, onSuccess: () => void) {
      updateMutation.mutate(request, { onSuccess });
    },
    addCitations(entryId: string, nodes: GraphNode[]) {
      for (const node of nodes) {
        addCitationMutation.mutate({
          entryId,
          targetNodeType: node.nodeType,
          targetNodeId: node.id,
          displayLabel: node.label,
          snippet: node.summary,
        });
      }
    },
  };
}

function mutationErrorMessage(error: unknown): string | undefined {
  if (!error) return undefined;
  return error instanceof Error ? error.message : '保存失败';
}
