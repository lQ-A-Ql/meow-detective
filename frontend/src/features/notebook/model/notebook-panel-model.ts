import type {
  CreateEntryRequest,
  GraphNode,
  NotebookEntry,
  NotebookEntryListItem,
  NotebookEntryStatus,
  NotebookEntryType,
  UpdateEntryRequest,
} from '@/types/models';

export type NotebookEntryDraft = Omit<CreateEntryRequest, 'author' | 'status'>;

export interface NotebookPanelModel {
  caseLoading: boolean;
  hasActiveCase: boolean;
  entriesLoading: boolean;
  entriesError: boolean;
  entries: NotebookEntryListItem[];
  rootEntries: NotebookEntryListItem[];
  typeCounts: Partial<Record<NotebookEntryType, number>>;
  selectedId: string | null;
  selectedEntry?: NotebookEntryListItem;
  showNewEntry: boolean;
  showNewReply: boolean;
  filterType: NotebookEntryType | '';
  filterStatus: NotebookEntryStatus | '';
  filterDate: 'all' | 'today' | 'week';
  createPending: boolean;
  createError?: string;
  detail?: NotebookEntry;
  detailLoading: boolean;
  detailError: boolean;
  updatePending: boolean;
  citationNodes: GraphNode[];
  citationNodesLoading: boolean;
  selectEntry: (entryId: string) => void;
  setShowNewEntry: (visible: boolean) => void;
  setShowNewReply: (visible: boolean) => void;
  setFilterType: (value: NotebookEntryType | '') => void;
  setFilterStatus: (value: NotebookEntryStatus | '') => void;
  setFilterDate: (value: 'all' | 'today' | 'week') => void;
  retryEntries: () => void;
  createEntry: (request: NotebookEntryDraft, onSuccess: () => void) => void;
  updateEntry: (request: UpdateEntryRequest, onSuccess: () => void) => void;
  addCitations: (entryId: string, nodes: GraphNode[]) => void;
}
