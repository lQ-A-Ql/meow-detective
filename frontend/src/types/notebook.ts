export type NotebookEntryType = 'note' | 'observation' | 'finding' | 'lead';

export type NotebookEntryStatus = 'draft' | 'review' | 'final';

export interface NotebookEntry {
  id: string;
  caseId: string;
  parentId?: string;
  title: string;
  content: string;
  entryType: NotebookEntryType;
  status: NotebookEntryStatus;
  tags: string[];
  citationNodeIds: string[];
  createdAt: string;
  updatedAt: string;
}

export interface NotebookEntryListItem {
  id: string;
  parentId?: string;
  title: string;
  entryType: NotebookEntryType;
  status: NotebookEntryStatus;
  tags: string[];
  replyCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateEntryRequest {
  title: string;
  content: string;
  entryType: NotebookEntryType;
  tags?: string[];
  parentId?: string;
}

export interface UpdateEntryRequest {
  entryId: string;
  title?: string;
  content?: string;
  entryType?: NotebookEntryType;
  tags?: string[];
  status?: NotebookEntryStatus;
}

export interface AddCitationRequest {
  entryId: string;
  nodeIds: string[];
}

export interface NotebookStats {
  entryCount: number;
  citationCount: number;
}
